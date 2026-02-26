import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";

import type {
  ActiveSessionInfo,
  AuthSession,
  AuthUser,
  GameInfo,
  GameVariant,
  LoginProvider,
  MainToRendererSignalingEvent,
  SessionInfo,
  Settings,
  SubscriptionInfo,
  StreamRegion,
  VideoCodec,
} from "@shared/gfn";

import {
  GfnWebRtcClient,
  type StreamDiagnostics,
  type StreamTimeWarning,
} from "./gfn/webrtcClient";
import {
  GAMEPAD_A,
  GAMEPAD_B,
  GAMEPAD_BACK,
  GAMEPAD_DPAD_DOWN,
  GAMEPAD_DPAD_LEFT,
  GAMEPAD_DPAD_RIGHT,
  GAMEPAD_DPAD_UP,
  GAMEPAD_GUIDE,
  GAMEPAD_LB,
  GAMEPAD_LS,
  GAMEPAD_RB,
  GAMEPAD_RS,
  GAMEPAD_START,
  GAMEPAD_X,
  GAMEPAD_Y,
  mapCodeToKey,
  normalizeToInt16,
  normalizeToUint8,
} from "./gfn/inputProtocol";
import { formatShortcutForDisplay, isShortcutMatch, normalizeShortcut } from "./shortcuts";
import { useControllerNavigation } from "./controllerNavigation";
import {
  createPluginScript,
  loadPluginAccentColor,
  loadPluginScripts,
  persistPluginAccentColor,
  persistPluginScripts,
  type PluginRunState,
  type PluginScript,
} from "./plugins";

// UI Components
import { LoginScreen } from "./components/LoginScreen";
import { Navbar } from "./components/Navbar";
import { HomePage } from "./components/HomePage";
import { LibraryPage } from "./components/LibraryPage";
import { PluginPage } from "./components/PluginPage";
import { SettingsPage } from "./components/SettingsPage";
import { StreamLoading } from "./components/StreamLoading";
import { StreamView } from "./components/StreamView";

const codecOptions: VideoCodec[] = ["H264", "H265", "AV1"];
const resolutionOptions = ["1280x720", "1920x1080", "2560x1440", "3840x2160", "2560x1080", "3440x1440"];
const fpsOptions = [30, 60, 120, 144, 240];
const SESSION_READY_POLL_INTERVAL_MS = 2000;
const SESSION_READY_TIMEOUT_MS = 180000;
const PLUGIN_SAFETY_ACK_STORAGE_KEY = "opennow.plugins.safety-ack.v1";

type GameSource = "main" | "library" | "public";
type AppPage = "home" | "library" | "plugins" | "settings";
type StreamStatus = "idle" | "queue" | "setup" | "starting" | "connecting" | "streaming";
type StreamLoadingStatus = "queue" | "setup" | "starting" | "connecting";
type ExitPromptState = { open: boolean; gameTitle: string };
type StreamWarningState = {
  code: StreamTimeWarning["code"];
  message: string;
  tone: "warn" | "critical";
  secondsLeft?: number;
};
type LaunchErrorState = {
  stage: StreamLoadingStatus;
  title: string;
  description: string;
  codeLabel?: string;
};

const APP_PAGE_ORDER: AppPage[] = ["home", "library", "plugins", "settings"];

const isMac = navigator.platform.toLowerCase().includes("mac");

const DEFAULT_SHORTCUTS = {
  shortcutToggleStats: "F3",
  shortcutTogglePointerLock: "F8",
  shortcutStopStream: "Ctrl+Shift+Q",
  shortcutToggleAntiAfk: "Ctrl+Shift+K",
  shortcutToggleMicrophone: "Ctrl+Shift+M",
} as const;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function isSessionReadyForConnect(status: number): boolean {
  return status === 2 || status === 3;
}

function isSessionInQueue(session: SessionInfo): boolean {
  // Official client treats seat setup step 1 as queue state even when queuePosition reaches 1.
  // Fallback to queuePosition-based inference for payloads that do not expose seatSetupStep.
  if (session.seatSetupStep === 1) {
    return true;
  }
  return (session.queuePosition ?? 0) > 1;
}

function isNumericId(value: string | undefined): value is string {
  if (!value) return false;
  return /^\d+$/.test(value);
}

function parseNumericId(value: string | undefined): number | null {
  if (!isNumericId(value)) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function defaultVariantId(game: GameInfo): string {
  return game.variants[0]?.id ?? game.id;
}

function getSelectedVariant(game: GameInfo, variantId: string): GameVariant | undefined {
  return game.variants.find((variant) => variant.id === variantId) ?? game.variants[0];
}

function mergeVariantSelections(
  current: Record<string, string>,
  catalog: GameInfo[],
): Record<string, string> {
  if (catalog.length === 0) {
    return current;
  }

  const next = { ...current };
  for (const game of catalog) {
    const selectedVariantId = next[game.id];
    const hasSelectedVariant = !!selectedVariantId && game.variants.some((variant) => variant.id === selectedVariantId);
    if (!hasSelectedVariant) {
      next[game.id] = defaultVariantId(game);
    }
  }
  return next;
}

function defaultDiagnostics(): StreamDiagnostics {
  return {
    connectionState: "closed",
    inputReady: false,
    connectedGamepads: 0,
    resolution: "",
    codec: "",
    isHdr: false,
    bitrateKbps: 0,
    decodeFps: 0,
    renderFps: 0,
    packetsLost: 0,
    packetsReceived: 0,
    packetLossPercent: 0,
    jitterMs: 0,
    rttMs: 0,
    framesReceived: 0,
    framesDecoded: 0,
    framesDropped: 0,
    decodeTimeMs: 0,
    renderTimeMs: 0,
    jitterBufferDelayMs: 0,
    inputQueueBufferedBytes: 0,
    inputQueuePeakBufferedBytes: 0,
    inputQueueDropCount: 0,
    inputQueueMaxSchedulingDelayMs: 0,
    gpuType: "",
    serverRegion: "",
    micState: "uninitialized",
    micEnabled: false,
  };
}

function isSessionLimitError(error: unknown): boolean {
  if (error && typeof error === "object" && "gfnErrorCode" in error) {
    const candidate = error.gfnErrorCode;
    if (typeof candidate === "number") {
      return candidate === 3237093643 || candidate === 3237093718;
    }
  }
  if (error instanceof Error) {
    const msg = error.message.toUpperCase();
    return msg.includes("SESSION LIMIT") || msg.includes("INSUFFICIENT_PLAYABILITY") || msg.includes("DUPLICATE SESSION");
  }
  return false;
}

function warningTone(code: StreamTimeWarning["code"]): "warn" | "critical" {
  if (code === 3) {
    return "critical";
  }
  return "warn";
}

function warningMessage(code: StreamTimeWarning["code"]): string {
  if (code === 1) return "Session time limit approaching";
  if (code === 2) return "Idle timeout approaching";
  return "Maximum session time approaching";
}

function toLoadingStatus(status: StreamStatus): StreamLoadingStatus {
  switch (status) {
    case "queue":
    case "setup":
    case "starting":
    case "connecting":
      return status;
    default:
      return "queue";
  }
}

function toCodeLabel(code: number | undefined): string | undefined {
  if (code === undefined) return undefined;
  if (code === 3237093643) return `SessionLimitExceeded (${code})`;
  if (code === 3237093718) return `SessionInsufficientPlayabilityLevel (${code})`;
  return `GFN Error ${code}`;
}

function extractLaunchErrorCode(error: unknown): number | undefined {
  if (error && typeof error === "object") {
    if ("gfnErrorCode" in error) {
      const directCode = error.gfnErrorCode;
      if (typeof directCode === "number") return directCode;
    }
    if ("statusCode" in error) {
      const statusCode = error.statusCode;
      if (typeof statusCode === "number" && statusCode > 0 && statusCode < 255) {
        return 3237093632 + statusCode;
      }
    }
  }
  if (error instanceof Error) {
    const match = error.message.match(/\b(3237\d{6,})\b/);
    if (match) {
      const code = Number(match[1]);
      if (Number.isFinite(code)) return code;
    }
  }
  return undefined;
}

function toLaunchErrorState(error: unknown, stage: StreamLoadingStatus): LaunchErrorState {
  const unknownMessage = "The game could not start. Please try again.";

  const titleFromError =
    error && typeof error === "object" && "title" in error && typeof error.title === "string"
      ? error.title.trim()
      : "";
  const descriptionFromError =
    error && typeof error === "object" && "description" in error && typeof error.description === "string"
      ? error.description.trim()
      : "";
  const statusDescription =
    error && typeof error === "object" && "statusDescription" in error && typeof error.statusDescription === "string"
      ? error.statusDescription.trim()
      : "";
  const messageFromError = error instanceof Error ? error.message.trim() : "";
  const combined = `${statusDescription} ${messageFromError}`.toUpperCase();
  const code = extractLaunchErrorCode(error);

  if (
    isSessionLimitError(error) ||
    combined.includes("INSUFFICIENT_PLAYABILITY") ||
    combined.includes("SESSION_LIMIT") ||
    combined.includes("DUPLICATE SESSION")
  ) {
    return {
      stage,
      title: "Duplicate Session Detected",
      description: "Another session is already running on your account. Close it first or wait for it to timeout, then launch again.",
      codeLabel: toCodeLabel(code),
    };
  }

  return {
    stage,
    title: titleFromError || "Launch Failed",
    description: descriptionFromError || messageFromError || statusDescription || unknownMessage,
    codeLabel: toCodeLabel(code),
  };
}

const PLUGIN_GAMEPAD_BUTTONS = {
  DPAD_UP: GAMEPAD_DPAD_UP,
  DPAD_DOWN: GAMEPAD_DPAD_DOWN,
  DPAD_LEFT: GAMEPAD_DPAD_LEFT,
  DPAD_RIGHT: GAMEPAD_DPAD_RIGHT,
  START: GAMEPAD_START,
  BACK: GAMEPAD_BACK,
  LS: GAMEPAD_LS,
  RS: GAMEPAD_RS,
  LB: GAMEPAD_LB,
  RB: GAMEPAD_RB,
  GUIDE: GAMEPAD_GUIDE,
  A: GAMEPAD_A,
  B: GAMEPAD_B,
  X: GAMEPAD_X,
  Y: GAMEPAD_Y,
} as const;

function normalizeHexColor(input: string): string | null {
  const value = input.trim();
  const shortHexMatch = value.match(/^#([0-9a-fA-F]{3})$/);
  if (shortHexMatch) {
    const [r, g, b] = shortHexMatch[1].split("");
    return `#${r}${r}${g}${g}${b}${b}`.toLowerCase();
  }
  const hexMatch = value.match(/^#([0-9a-fA-F]{6})$/);
  if (!hexMatch) {
    return null;
  }
  return `#${hexMatch[1].toLowerCase()}`;
}

function hexToRgb(hex: string): [number, number, number] | null {
  const normalized = normalizeHexColor(hex);
  if (!normalized) return null;
  const color = normalized.slice(1);
  const r = Number.parseInt(color.slice(0, 2), 16);
  const g = Number.parseInt(color.slice(2, 4), 16);
  const b = Number.parseInt(color.slice(4, 6), 16);
  return [r, g, b];
}

function rgbToHex(r: number, g: number, b: number): string {
  return `#${[r, g, b]
    .map((value) => Math.max(0, Math.min(255, Math.round(value))).toString(16).padStart(2, "0"))
    .join("")}`;
}

function mixHex(base: string, target: string, ratio: number): string {
  const baseRgb = hexToRgb(base);
  const targetRgb = hexToRgb(target);
  if (!baseRgb || !targetRgb) {
    return base;
  }
  const t = Math.max(0, Math.min(1, ratio));
  const r = baseRgb[0] + (targetRgb[0] - baseRgb[0]) * t;
  const g = baseRgb[1] + (targetRgb[1] - baseRgb[1]) * t;
  const b = baseRgb[2] + (targetRgb[2] - baseRgb[2]) * t;
  return rgbToHex(r, g, b);
}

function withAlpha(hex: string, alpha: number): string {
  const rgb = hexToRgb(hex);
  if (!rgb) {
    return "rgba(88, 217, 138, 0.2)";
  }
  const a = Math.max(0, Math.min(1, alpha));
  return `rgba(${rgb[0]}, ${rgb[1]}, ${rgb[2]}, ${a})`;
}

function applyPluginAccentTheme(accent: string | null): void {
  const root = document.documentElement;
  if (!accent) {
    root.style.removeProperty("--accent");
    root.style.removeProperty("--accent-hover");
    root.style.removeProperty("--accent-press");
    root.style.removeProperty("--accent-glow");
    root.style.removeProperty("--accent-surface");
    root.style.removeProperty("--accent-surface-strong");
    return;
  }

  const normalized = normalizeHexColor(accent);
  if (!normalized) {
    return;
  }

  root.style.setProperty("--accent", normalized);
  root.style.setProperty("--accent-hover", mixHex(normalized, "#ffffff", 0.12));
  root.style.setProperty("--accent-press", mixHex(normalized, "#000000", 0.2));
  root.style.setProperty("--accent-glow", withAlpha(normalized, 0.25));
  root.style.setProperty("--accent-surface", withAlpha(normalized, 0.08));
  root.style.setProperty("--accent-surface-strong", withAlpha(normalized, 0.15));
}

function modifierMask(modifiers?: {
  shift?: boolean;
  ctrl?: boolean;
  alt?: boolean;
  meta?: boolean;
}): number {
  let mask = 0;
  if (modifiers?.shift) mask |= 0x01;
  if (modifiers?.ctrl) mask |= 0x02;
  if (modifiers?.alt) mask |= 0x04;
  if (modifiers?.meta) mask |= 0x08;
  return mask;
}

function mapPluginMouseButton(button: "left" | "middle" | "right" | "back" | "forward" | number): number {
  if (typeof button === "number") {
    return Math.max(1, Math.min(5, Math.round(button)));
  }
  if (button === "left") return 1;
  if (button === "middle") return 2;
  if (button === "right") return 3;
  if (button === "back") return 4;
  return 5;
}

export function App(): JSX.Element {
  // Auth State
  const [authSession, setAuthSession] = useState<AuthSession | null>(null);
  const [providers, setProviders] = useState<LoginProvider[]>([]);
  const [providerIdpId, setProviderIdpId] = useState("");
  const [isLoggingIn, setIsLoggingIn] = useState(false);
  const [loginError, setLoginError] = useState<string | null>(null);
  const [isInitializing, setIsInitializing] = useState(true);
  const [startupStatusMessage, setStartupStatusMessage] = useState("Restoring saved session...");
  const [startupRefreshNotice, setStartupRefreshNotice] = useState<{
    tone: "success" | "warn";
    text: string;
  } | null>(null);

  // Navigation
  const [currentPage, setCurrentPage] = useState<AppPage>("home");
  const [pluginSafetyAcknowledged, setPluginSafetyAcknowledged] = useState<boolean>(() => {
    try {
      return window.localStorage.getItem(PLUGIN_SAFETY_ACK_STORAGE_KEY) === "1";
    } catch {
      return false;
    }
  });
  const [pluginSafetyPromptOpen, setPluginSafetyPromptOpen] = useState(false);

  const initialPluginState = useMemo(() => {
    const loadedPlugins = loadPluginScripts();
    const selectedId = loadedPlugins[0]?.id ?? "";
    return {
      plugins: loadedPlugins,
      selectedId,
      accent: loadPluginAccentColor(),
    };
  }, []);

  const [plugins, setPlugins] = useState<PluginScript[]>(initialPluginState.plugins);
  const [selectedPluginId, setSelectedPluginId] = useState(initialPluginState.selectedId);
  const [pluginAccentColor, setPluginAccentColor] = useState<string | null>(initialPluginState.accent);
  const [pluginRunStates, setPluginRunStates] = useState<Record<string, PluginRunState>>({});

  // Games State
  const [games, setGames] = useState<GameInfo[]>([]);
  const [libraryGames, setLibraryGames] = useState<GameInfo[]>([]);
  const [source, setSource] = useState<GameSource>("main");
  const [searchQuery, setSearchQuery] = useState("");
  const [selectedGameId, setSelectedGameId] = useState("");
  const [variantByGameId, setVariantByGameId] = useState<Record<string, string>>({});
  const [isLoadingGames, setIsLoadingGames] = useState(false);

  // Settings State
  const [settings, setSettings] = useState<Settings>({
    resolution: "1920x1080",
    fps: 60,
    maxBitrateMbps: 75,
    codec: "H264",
    colorQuality: "10bit_420",
    region: "",
    clipboardPaste: false,
    mouseSensitivity: 1,
    shortcutToggleStats: DEFAULT_SHORTCUTS.shortcutToggleStats,
    shortcutTogglePointerLock: DEFAULT_SHORTCUTS.shortcutTogglePointerLock,
    shortcutStopStream: DEFAULT_SHORTCUTS.shortcutStopStream,
    shortcutToggleAntiAfk: DEFAULT_SHORTCUTS.shortcutToggleAntiAfk,
    shortcutToggleMicrophone: DEFAULT_SHORTCUTS.shortcutToggleMicrophone,
    microphoneMode: "disabled",
    microphoneDeviceId: "",
    hideStreamButtons: false,
    sessionClockShowEveryMinutes: 60,
    sessionClockShowDurationSeconds: 30,
    windowWidth: 1400,
    windowHeight: 900,
  });
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [regions, setRegions] = useState<StreamRegion[]>([]);
  const [subscriptionInfo, setSubscriptionInfo] = useState<SubscriptionInfo | null>(null);

  // Stream State
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [streamStatus, setStreamStatus] = useState<StreamStatus>("idle");
  const [diagnostics, setDiagnostics] = useState<StreamDiagnostics>(defaultDiagnostics());
  const [showStatsOverlay, setShowStatsOverlay] = useState(true);
  const [antiAfkEnabled, setAntiAfkEnabled] = useState(false);
  const [escHoldReleaseIndicator, setEscHoldReleaseIndicator] = useState<{ visible: boolean; progress: number }>({
    visible: false,
    progress: 0,
  });
  const [exitPrompt, setExitPrompt] = useState<ExitPromptState>({ open: false, gameTitle: "Game" });
  const [streamingGame, setStreamingGame] = useState<GameInfo | null>(null);
  const [streamingStore, setStreamingStore] = useState<string | null>(null);
  const [queuePosition, setQueuePosition] = useState<number | undefined>();
  const [navbarActiveSession, setNavbarActiveSession] = useState<ActiveSessionInfo | null>(null);
  const [isResumingNavbarSession, setIsResumingNavbarSession] = useState(false);
  const [launchError, setLaunchError] = useState<LaunchErrorState | null>(null);
  const [sessionStartedAtMs, setSessionStartedAtMs] = useState<number | null>(null);
  const [sessionElapsedSeconds, setSessionElapsedSeconds] = useState(0);
  const [streamWarning, setStreamWarning] = useState<StreamWarningState | null>(null);

  const navigateToPage = useCallback((nextPage: AppPage): void => {
    if (nextPage === "plugins" && !pluginSafetyAcknowledged) {
      setPluginSafetyPromptOpen(true);
      return;
    }
    setCurrentPage(nextPage);
  }, [pluginSafetyAcknowledged]);

  const acknowledgePluginSafety = useCallback((): void => {
    setPluginSafetyAcknowledged(true);
    setPluginSafetyPromptOpen(false);
    try {
      window.localStorage.setItem(PLUGIN_SAFETY_ACK_STORAGE_KEY, "1");
    } catch {
      // ignore storage failures
    }
    setCurrentPage("plugins");
  }, []);

  const dismissPluginSafety = useCallback((): void => {
    setPluginSafetyPromptOpen(false);
  }, []);

  const handleControllerPageNavigate = useCallback((direction: "prev" | "next"): void => {
    if (!authSession || streamStatus !== "idle") {
      return;
    }
    const currentIndex = APP_PAGE_ORDER.indexOf(currentPage);
    const step = direction === "next" ? 1 : -1;
    const nextIndex = (currentIndex + step + APP_PAGE_ORDER.length) % APP_PAGE_ORDER.length;
    navigateToPage(APP_PAGE_ORDER[nextIndex]);
  }, [authSession, currentPage, navigateToPage, streamStatus]);

  const handleControllerBackAction = useCallback((): boolean => {
    if (pluginSafetyPromptOpen) {
      dismissPluginSafety();
      return true;
    }
    if (!authSession || streamStatus !== "idle") {
      return false;
    }
    if (currentPage !== "home") {
      setCurrentPage("home");
      return true;
    }
    return false;
  }, [authSession, currentPage, dismissPluginSafety, pluginSafetyPromptOpen, streamStatus]);

  const controllerConnected = useControllerNavigation({
    enabled: streamStatus !== "streaming" || exitPrompt.open,
    onNavigatePage: handleControllerPageNavigate,
    onBackAction: handleControllerBackAction,
  });

  // Refs
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const clientRef = useRef<GfnWebRtcClient | null>(null);
  const sessionRef = useRef<SessionInfo | null>(null);
  const hasInitializedRef = useRef(false);
  const regionsRequestRef = useRef(0);
  const launchInFlightRef = useRef(false);
  const exitPromptResolverRef = useRef<((confirmed: boolean) => void) | null>(null);
  const pluginRunLockRef = useRef<Set<string>>(new Set());

  const applyVariantSelections = useCallback((catalog: GameInfo[]): void => {
    setVariantByGameId((prev) => mergeVariantSelections(prev, catalog));
  }, []);

  const resetLaunchRuntime = useCallback((options?: {
    keepLaunchError?: boolean;
    keepStreamingContext?: boolean;
  }): void => {
    setSession(null);
    setStreamStatus("idle");
    setQueuePosition(undefined);
    setSessionStartedAtMs(null);
    setSessionElapsedSeconds(0);
    setStreamWarning(null);
    setEscHoldReleaseIndicator({ visible: false, progress: 0 });
    setDiagnostics(defaultDiagnostics());

    if (!options?.keepStreamingContext) {
      setStreamingGame(null);
      setStreamingStore(null);
    }

    if (!options?.keepLaunchError) {
      setLaunchError(null);
    }
  }, []);

  // Session ref sync
  useEffect(() => {
    sessionRef.current = session;
  }, [session]);

  useEffect(() => {
    document.body.classList.toggle("controller-mode", controllerConnected);
    return () => {
      document.body.classList.remove("controller-mode");
    };
  }, [controllerConnected]);

  useEffect(() => {
    persistPluginScripts(plugins);
  }, [plugins]);

  useEffect(() => {
    if (!selectedPluginId || !plugins.some((plugin) => plugin.id === selectedPluginId)) {
      setSelectedPluginId(plugins[0]?.id ?? "");
    }
  }, [plugins, selectedPluginId]);

  const setPluginRunState = useCallback((id: string, next: PluginRunState): void => {
    setPluginRunStates((prev) => ({
      ...prev,
      [id]: next,
    }));
  }, []);

  const updatePluginAccent = useCallback((nextColor: string | null) => {
    const normalized = nextColor ? normalizeHexColor(nextColor) : null;
    setPluginAccentColor(normalized);
    persistPluginAccentColor(normalized);
  }, []);

  useEffect(() => {
    applyPluginAccentTheme(pluginAccentColor);
  }, [pluginAccentColor]);

  const createPlugin = useCallback((): void => {
    const created = createPluginScript({
      name: `Plugin ${plugins.length + 1}`,
    });
    setPlugins((prev) => [...prev, created]);
    setSelectedPluginId(created.id);
  }, [plugins.length]);

  const updatePlugin = useCallback((id: string, update: Partial<PluginScript>): void => {
    setPlugins((prev) => prev.map((plugin) => (plugin.id === id ? { ...plugin, ...update } : plugin)));
  }, []);

  const deletePlugin = useCallback((id: string): void => {
    setPlugins((prev) => prev.filter((plugin) => plugin.id !== id));
    setPluginRunStates((prev) => {
      if (!(id in prev)) {
        return prev;
      }
      const next = { ...prev };
      delete next[id];
      return next;
    });
  }, []);

  const executePlugin = useCallback(async (pluginId: string): Promise<void> => {
    const plugin = plugins.find((candidate) => candidate.id === pluginId);
    if (!plugin) {
      return;
    }
    if (pluginRunLockRef.current.has(pluginId)) {
      return;
    }

    pluginRunLockRef.current.add(pluginId);
    setPluginRunState(pluginId, { status: "running", message: "Running plugin..." });

    const client = clientRef.current;
    const parseAxis = (value: number | undefined): number => {
      const numeric = Number(value ?? 0);
      if (!Number.isFinite(numeric)) return 0;
      if (numeric >= -1 && numeric <= 1) return normalizeToInt16(numeric);
      return Math.max(-32768, Math.min(32767, Math.round(numeric)));
    };
    const parseTrigger = (value: number | undefined): number => {
      const numeric = Number(value ?? 0);
      if (!Number.isFinite(numeric)) return 0;
      if (numeric >= 0 && numeric <= 1) return normalizeToUint8(numeric);
      return Math.max(0, Math.min(255, Math.round(numeric)));
    };

    const api = {
      sleep: async (ms: number) => {
        const timeout = Number.isFinite(ms) ? Math.max(0, Math.min(60_000, Math.round(ms))) : 0;
        await sleep(timeout);
      },
      log: (...values: unknown[]) => {
        console.log(`[Plugin:${plugin.name}]`, ...values);
      },
      input: {
        buttons: PLUGIN_GAMEPAD_BUTTONS,
        keyDown: (
          code: string,
          modifiers?: { shift?: boolean; ctrl?: boolean; alt?: boolean; meta?: boolean },
        ): boolean => {
          const mapped = mapCodeToKey(code);
          if (!mapped || !client) return false;
          return client.sendVirtualKey(mapped.vk, mapped.scancode, modifierMask(modifiers), true);
        },
        keyUp: (
          code: string,
          modifiers?: { shift?: boolean; ctrl?: boolean; alt?: boolean; meta?: boolean },
        ): boolean => {
          const mapped = mapCodeToKey(code);
          if (!mapped || !client) return false;
          return client.sendVirtualKey(mapped.vk, mapped.scancode, modifierMask(modifiers), false);
        },
        keyTap: (
          code: string,
          modifiers?: { shift?: boolean; ctrl?: boolean; alt?: boolean; meta?: boolean },
        ): boolean => {
          const mapped = mapCodeToKey(code);
          if (!mapped || !client) return false;
          return client.sendVirtualKeyTap(mapped.vk, mapped.scancode, modifierMask(modifiers));
        },
        mouseMove: (dx: number, dy: number): boolean => {
          if (!client) return false;
          return client.sendVirtualMouseMove(dx, dy);
        },
        mouseButton: (
          button: "left" | "middle" | "right" | "back" | "forward" | number,
          action: "down" | "up" | "click" = "click",
        ): boolean => {
          if (!client) return false;
          const mappedButton = mapPluginMouseButton(button);
          if (action === "down") {
            return client.sendVirtualMouseButton(mappedButton, true);
          }
          if (action === "up") {
            return client.sendVirtualMouseButton(mappedButton, false);
          }
          const downSent = client.sendVirtualMouseButton(mappedButton, true);
          const upSent = client.sendVirtualMouseButton(mappedButton, false);
          return downSent && upSent;
        },
        mouseWheel: (delta: number): boolean => {
          if (!client) return false;
          return client.sendVirtualMouseWheel(delta);
        },
        controllerFrame: (frame?: {
          controllerId?: number;
          buttons?: number;
          leftTrigger?: number;
          rightTrigger?: number;
          leftStickX?: number;
          leftStickY?: number;
          rightStickX?: number;
          rightStickY?: number;
          connected?: boolean;
          usePartiallyReliable?: boolean;
        }): boolean => {
          if (!client) return false;
          return client.sendVirtualGamepadState({
            controllerId: frame?.controllerId ?? 0,
            buttons: frame?.buttons ?? 0,
            leftTrigger: parseTrigger(frame?.leftTrigger),
            rightTrigger: parseTrigger(frame?.rightTrigger),
            leftStickX: parseAxis(frame?.leftStickX),
            leftStickY: parseAxis(frame?.leftStickY),
            rightStickX: parseAxis(frame?.rightStickX),
            rightStickY: parseAxis(frame?.rightStickY),
            connected: frame?.connected,
            usePartiallyReliable: frame?.usePartiallyReliable,
          });
        },
      },
      stream: {
        isReady: (): boolean => streamStatus === "streaming" && Boolean(client?.isInputReady()),
        sendText: (text: string): number => client?.sendText(text) ?? 0,
        pasteShortcut: (): boolean => client?.sendPasteShortcut(isMac) ?? false,
        antiAfkPulse: (): boolean => client?.sendAntiAfkPulse() ?? false,
      },
      theme: {
        getAccent: (): string | null => pluginAccentColor,
        setAccent: (color: string): boolean => {
          const normalized = normalizeHexColor(color);
          if (!normalized) {
            throw new Error(`Invalid accent color: ${color}`);
          }
          updatePluginAccent(normalized);
          return true;
        },
        resetAccent: (): boolean => {
          updatePluginAccent(null);
          return true;
        },
      },
    };

    try {
      const AsyncFunction = Object.getPrototypeOf(async () => undefined).constructor as new (
        apiName: string,
        source: string,
      ) => (apiValue: typeof api) => Promise<void>;

      const runner = new AsyncFunction(
        "api",
        [
          '"use strict";',
          "const { input, stream, theme, sleep, log } = api;",
          plugin.script,
        ].join("\n"),
      );

      await runner(api);
      setPluginRunState(pluginId, { status: "success", message: "Plugin completed." });
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setPluginRunState(pluginId, { status: "error", message });
    } finally {
      pluginRunLockRef.current.delete(pluginId);
    }
  }, [pluginAccentColor, plugins, setPluginRunState, streamStatus, updatePluginAccent]);

  useEffect(() => {
    const registered = plugins
      .filter((plugin) => plugin.enabled && plugin.shortcut.trim().length > 0)
      .map((plugin) => {
        const normalized = normalizeShortcut(plugin.shortcut);
        return {
          pluginId: plugin.id,
          shortcut: normalized,
        };
      })
      .filter((entry) => entry.shortcut.valid);

    if (registered.length === 0) {
      return;
    }

    const handlePluginHotkeys = (event: KeyboardEvent) => {
      const target = event.target as HTMLElement | null;
      const isTyping = !!target && (
        target.tagName === "INPUT"
        || target.tagName === "TEXTAREA"
        || target.isContentEditable
      );
      if (isTyping || exitPrompt.open || pluginSafetyPromptOpen) {
        return;
      }

      for (const entry of registered) {
        if (!isShortcutMatch(event, entry.shortcut)) {
          continue;
        }
        event.preventDefault();
        event.stopPropagation();
        event.stopImmediatePropagation();
        void executePlugin(entry.pluginId);
        break;
      }
    };

    window.addEventListener("keydown", handlePluginHotkeys, true);
    return () => window.removeEventListener("keydown", handlePluginHotkeys, true);
  }, [executePlugin, exitPrompt.open, pluginSafetyPromptOpen, plugins]);

  // Derived state
  const selectedProvider = useMemo(() => {
    return providers.find((p) => p.idpId === providerIdpId) ?? authSession?.provider ?? null;
  }, [providers, providerIdpId, authSession]);

  const effectiveStreamingBaseUrl = useMemo(() => {
    return selectedProvider?.streamingServiceUrl ?? "";
  }, [selectedProvider]);

  const loadSubscriptionInfo = useCallback(
    async (session: AuthSession): Promise<void> => {
      const token = session.tokens.idToken ?? session.tokens.accessToken;
      const subscription = await window.openNow.fetchSubscription({
        token,
        providerStreamingBaseUrl: session.provider.streamingServiceUrl,
        userId: session.user.userId,
      });
      setSubscriptionInfo(subscription);
    },
    [],
  );

  const refreshNavbarActiveSession = useCallback(async (): Promise<void> => {
    if (!authSession) {
      setNavbarActiveSession(null);
      return;
    }
    const token = authSession.tokens.idToken ?? authSession.tokens.accessToken;
    if (!token || !effectiveStreamingBaseUrl) {
      setNavbarActiveSession(null);
      return;
    }
    try {
      const activeSessions = await window.openNow.getActiveSessions(token, effectiveStreamingBaseUrl);
      const candidate = activeSessions.find((entry) => entry.status === 3 || entry.status === 2) ?? null;
      setNavbarActiveSession(candidate);
    } catch (error) {
      console.warn("Failed to refresh active sessions:", error);
    }
  }, [authSession, effectiveStreamingBaseUrl]);

  useEffect(() => {
    if (!startupRefreshNotice) return;
    const timer = window.setTimeout(() => setStartupRefreshNotice(null), 7000);
    return () => window.clearTimeout(timer);
  }, [startupRefreshNotice]);

  useEffect(() => {
    if (!authSession || streamStatus !== "idle") {
      return;
    }
    void refreshNavbarActiveSession();
    const timer = window.setInterval(() => {
      void refreshNavbarActiveSession();
    }, 10000);
    return () => window.clearInterval(timer);
  }, [authSession, refreshNavbarActiveSession, streamStatus]);

  // Initialize app
  useEffect(() => {
    if (hasInitializedRef.current) return;
    hasInitializedRef.current = true;

    const initialize = async () => {
      try {
        // Load settings first
        const loadedSettings = await window.openNow.getSettings();
        setSettings(loadedSettings);
        setSettingsLoaded(true);

        // Load providers and session (refresh only if token is near expiry)
        setStartupStatusMessage("Restoring saved session...");
        const [providerList, sessionResult] = await Promise.all([
          window.openNow.getLoginProviders(),
          window.openNow.getAuthSession(),
        ]);
        const persistedSession = sessionResult.session;

        if (sessionResult.refresh.outcome === "refreshed") {
          setStartupRefreshNotice({
            tone: "success",
            text: "Session restored. Token refreshed.",
          });
          setStartupStatusMessage("Token refreshed. Loading your account...");
        } else if (sessionResult.refresh.outcome === "failed") {
          setStartupRefreshNotice({
            tone: "warn",
            text: "Token refresh failed. Using saved session token.",
          });
          setStartupStatusMessage("Token refresh failed. Continuing with saved session...");
        } else if (sessionResult.refresh.outcome === "missing_refresh_token") {
          setStartupStatusMessage("Saved session has no refresh token. Continuing...");
        } else if (persistedSession) {
          setStartupStatusMessage("Session restored.");
        } else {
          setStartupStatusMessage("No saved session found.");
        }

        // Update isInitializing FIRST so UI knows we're done loading
        setIsInitializing(false);
        setProviders(providerList);
        setAuthSession(persistedSession);

        const activeProviderId = persistedSession?.provider?.idpId ?? providerList[0]?.idpId ?? "";
        setProviderIdpId(activeProviderId);

        if (persistedSession) {
          // Load regions
          const token = persistedSession.tokens.idToken ?? persistedSession.tokens.accessToken;
          const discovered = await window.openNow.getRegions({ token });
          setRegions(discovered);

          try {
            await loadSubscriptionInfo(persistedSession);
          } catch (error) {
            console.warn("Failed to load subscription info:", error);
            setSubscriptionInfo(null);
          }

          // Load games
          try {
            const mainGames = await window.openNow.fetchMainGames({
              token,
              providerStreamingBaseUrl: persistedSession.provider.streamingServiceUrl,
            });
            setGames(mainGames);
            setSource("main");
            setSelectedGameId(mainGames[0]?.id ?? "");
            applyVariantSelections(mainGames);

            // Also load library
            const libGames = await window.openNow.fetchLibraryGames({
              token,
              providerStreamingBaseUrl: persistedSession.provider.streamingServiceUrl,
            });
            setLibraryGames(libGames);
            applyVariantSelections(libGames);
          } catch {
            // Fallback to public games
            const publicGames = await window.openNow.fetchPublicGames();
            setGames(publicGames);
            setSource("public");
            applyVariantSelections(publicGames);
          }
        } else {
          // Load public games for non-logged in users
          const publicGames = await window.openNow.fetchPublicGames();
          setGames(publicGames);
          setSource("public");
          applyVariantSelections(publicGames);
          setSubscriptionInfo(null);
        }
      } catch (error) {
        console.error("Initialization failed:", error);
        setStartupStatusMessage("Session restore failed. Please sign in again.");
        // Always set isInitializing to false even on error
        setIsInitializing(false);
      }
    };

    void initialize();
  }, []);

  const shortcuts = useMemo(() => {
    const parseWithFallback = (value: string, fallback: string) => {
      const parsed = normalizeShortcut(value);
      return parsed.valid ? parsed : normalizeShortcut(fallback);
    };
    const toggleStats = parseWithFallback(settings.shortcutToggleStats, DEFAULT_SHORTCUTS.shortcutToggleStats);
    const togglePointerLock = parseWithFallback(settings.shortcutTogglePointerLock, DEFAULT_SHORTCUTS.shortcutTogglePointerLock);
    const stopStream = parseWithFallback(settings.shortcutStopStream, DEFAULT_SHORTCUTS.shortcutStopStream);
    const toggleAntiAfk = parseWithFallback(settings.shortcutToggleAntiAfk, DEFAULT_SHORTCUTS.shortcutToggleAntiAfk);
    const toggleMicrophone = parseWithFallback(settings.shortcutToggleMicrophone, DEFAULT_SHORTCUTS.shortcutToggleMicrophone);
    return { toggleStats, togglePointerLock, stopStream, toggleAntiAfk, toggleMicrophone };
  }, [
    settings.shortcutToggleStats,
    settings.shortcutTogglePointerLock,
    settings.shortcutStopStream,
    settings.shortcutToggleAntiAfk,
    settings.shortcutToggleMicrophone,
  ]);

  const requestEscLockedPointerCapture = useCallback(async (target: HTMLVideoElement) => {
    if (!document.fullscreenElement) {
      await document.documentElement.requestFullscreen().catch(() => {});
    }

    const nav = navigator as any;
    if (document.fullscreenElement && nav.keyboard?.lock) {
      await nav.keyboard.lock([
        "Escape", "F11", "BrowserBack", "BrowserForward", "BrowserRefresh",
      ]).catch(() => {});
    }

    await (target.requestPointerLock({ unadjustedMovement: true } as any) as unknown as Promise<void>)
      .catch((err: DOMException) => {
        if (err.name === "NotSupportedError") {
          return target.requestPointerLock();
        }
        throw err;
      })
      .catch(() => {});
  }, []);

  const resolveExitPrompt = useCallback((confirmed: boolean) => {
    const resolver = exitPromptResolverRef.current;
    exitPromptResolverRef.current = null;
    setExitPrompt((prev) => (prev.open ? { ...prev, open: false } : prev));
    resolver?.(confirmed);
  }, []);

  const requestExitPrompt = useCallback((gameTitle: string): Promise<boolean> => {
    return new Promise((resolve) => {
      if (exitPromptResolverRef.current) {
        // Close any previous pending prompt to avoid dangling promises.
        exitPromptResolverRef.current(false);
      }
      exitPromptResolverRef.current = resolve;
      setExitPrompt({
        open: true,
        gameTitle: gameTitle || "this game",
      });
    });
  }, []);

  const handleExitPromptConfirm = useCallback(() => {
    resolveExitPrompt(true);
  }, [resolveExitPrompt]);

  const handleExitPromptCancel = useCallback(() => {
    resolveExitPrompt(false);
  }, [resolveExitPrompt]);

  useEffect(() => {
    return () => {
      if (exitPromptResolverRef.current) {
        exitPromptResolverRef.current(false);
        exitPromptResolverRef.current = null;
      }
    };
  }, []);

  // Listen for F11 fullscreen toggle from main process (uses W3C Fullscreen API
  // so navigator.keyboard.lock() can capture Escape in fullscreen)
  useEffect(() => {
    const unsubscribe = window.openNow.onToggleFullscreen(() => {
      if (document.fullscreenElement) {
        document.exitFullscreen().catch(() => {});
      } else {
        document.documentElement.requestFullscreen().catch(() => {});
      }
    });
    return () => unsubscribe();
  }, []);

  // Anti-AFK interval
  useEffect(() => {
    if (!antiAfkEnabled || streamStatus !== "streaming") return;

    const interval = window.setInterval(() => {
      clientRef.current?.sendAntiAfkPulse();
    }, 240000); // 4 minutes

    return () => clearInterval(interval);
  }, [antiAfkEnabled, streamStatus]);

  // Restore focus to video element when navigating away from Settings during streaming
  useEffect(() => {
    if (streamStatus === "streaming" && currentPage !== "settings" && videoRef.current) {
      // Small delay to let React finish rendering the new page
      const timer = window.setTimeout(() => {
        if (videoRef.current && document.activeElement !== videoRef.current) {
          videoRef.current.focus();
          console.log("[App] Restored focus to video element after leaving Settings");
        }
      }, 50);
      return () => clearTimeout(timer);
    }
  }, [currentPage, streamStatus]);

  useEffect(() => {
    if (streamStatus === "idle" || sessionStartedAtMs === null) {
      setSessionElapsedSeconds(0);
      return;
    }

    const updateElapsed = () => {
      const elapsed = Math.max(0, Math.floor((Date.now() - sessionStartedAtMs) / 1000));
      setSessionElapsedSeconds(elapsed);
    };

    updateElapsed();
    const timer = window.setInterval(updateElapsed, 1000);
    return () => window.clearInterval(timer);
  }, [sessionStartedAtMs, streamStatus]);

  useEffect(() => {
    if (streamStatus !== "streaming" || sessionStartedAtMs !== null) {
      return;
    }

    const hasLiveFrames = diagnostics.framesDecoded > 0 || diagnostics.framesReceived > 0 || diagnostics.renderFps > 0;
    if (!hasLiveFrames) {
      return;
    }

    setSessionStartedAtMs(Date.now());
  }, [
    diagnostics.framesDecoded,
    diagnostics.framesReceived,
    diagnostics.renderFps,
    sessionStartedAtMs,
    streamStatus,
  ]);

  useEffect(() => {
    if (!streamWarning) return;
    const warning = streamWarning;
    const timer = window.setTimeout(() => {
      setStreamWarning((current) => (current === warning ? null : current));
    }, 12000);
    return () => window.clearTimeout(timer);
  }, [streamWarning]);

  // Signaling events
  useEffect(() => {
    const unsubscribe = window.openNow.onSignalingEvent(async (event: MainToRendererSignalingEvent) => {
      console.log(`[App] Signaling event: ${event.type}`, event.type === "offer" ? `(SDP ${event.sdp.length} chars)` : "", event.type === "remote-ice" ? event.candidate : "");
      try {
        if (event.type === "offer") {
          const activeSession = sessionRef.current;
          if (!activeSession) {
            console.warn("[App] Received offer but no active session in sessionRef!");
            return;
          }
          console.log("[App] Active session for offer:", JSON.stringify({
            sessionId: activeSession.sessionId,
            serverIp: activeSession.serverIp,
            signalingServer: activeSession.signalingServer,
            mediaConnectionInfo: activeSession.mediaConnectionInfo,
            iceServersCount: activeSession.iceServers?.length,
          }));

          if (!clientRef.current && videoRef.current && audioRef.current) {
            clientRef.current = new GfnWebRtcClient({
              videoElement: videoRef.current,
              audioElement: audioRef.current,
              microphoneMode: settings.microphoneMode,
              microphoneDeviceId: settings.microphoneDeviceId || undefined,
              mouseSensitivity: settings.mouseSensitivity,
              onLog: (line: string) => console.log(`[WebRTC] ${line}`),
              onStats: (stats) => setDiagnostics(stats),
              onEscHoldProgress: (visible, progress) => {
                setEscHoldReleaseIndicator({ visible, progress });
              },
              onTimeWarning: (warning) => {
                setStreamWarning({
                  code: warning.code,
                  message: warningMessage(warning.code),
                  tone: warningTone(warning.code),
                  secondsLeft: warning.secondsLeft,
                });
              },
              onMicStateChange: (state) => {
                console.log(`[App] Mic state: ${state.state}${state.deviceLabel ? ` (${state.deviceLabel})` : ""}`);
              },
            });
            // Auto-start microphone if mode is enabled
            if (settings.microphoneMode !== "disabled") {
              void clientRef.current.startMicrophone();
            }
          }

          if (clientRef.current) {
            await clientRef.current.handleOffer(event.sdp, activeSession, {
              codec: settings.codec,
              colorQuality: settings.colorQuality,
              resolution: settings.resolution,
              fps: settings.fps,
              maxBitrateKbps: settings.maxBitrateMbps * 1000,
            });
            setLaunchError(null);
            setStreamStatus("streaming");
          }
        } else if (event.type === "remote-ice") {
          await clientRef.current?.addRemoteCandidate(event.candidate);
        } else if (event.type === "disconnected") {
          console.warn("Signaling disconnected:", event.reason);
          clientRef.current?.dispose();
          clientRef.current = null;
          resetLaunchRuntime();
          launchInFlightRef.current = false;
        } else if (event.type === "error") {
          console.error("Signaling error:", event.message);
        }
      } catch (error) {
        console.error("Signaling event error:", error);
      }
    });

    return () => unsubscribe();
  }, [resetLaunchRuntime, settings]);

  // Save settings when changed
  const updateSetting = useCallback(async <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setSettings((prev) => ({ ...prev, [key]: value }));
    if (settingsLoaded) {
      await window.openNow.setSetting(key, value);
    }
    // If a running client exists, push certain settings live
    if (key === "mouseSensitivity") {
      try {
        (clientRef.current as any)?.setMouseSensitivity?.(value as number);
      } catch {
        // ignore
      }
    }
  }, [settingsLoaded]);

  // Login handler
  const handleLogin = useCallback(async () => {
    setIsLoggingIn(true);
    setLoginError(null);
    try {
      const session = await window.openNow.login({ providerIdpId: providerIdpId || undefined });
      setAuthSession(session);
      setProviderIdpId(session.provider.idpId);

      // Load regions
      const token = session.tokens.idToken ?? session.tokens.accessToken;
      const discovered = await window.openNow.getRegions({ token });
      setRegions(discovered);

      try {
        await loadSubscriptionInfo(session);
      } catch (error) {
        console.warn("Failed to load subscription info:", error);
        setSubscriptionInfo(null);
      }

      // Load games
      const mainGames = await window.openNow.fetchMainGames({
        token,
        providerStreamingBaseUrl: session.provider.streamingServiceUrl,
      });
      setGames(mainGames);
      setSource("main");
      setSelectedGameId(mainGames[0]?.id ?? "");
      applyVariantSelections(mainGames);

      // Load library
      const libGames = await window.openNow.fetchLibraryGames({
        token,
        providerStreamingBaseUrl: session.provider.streamingServiceUrl,
      });
      setLibraryGames(libGames);
      applyVariantSelections(libGames);
    } catch (error) {
      setLoginError(error instanceof Error ? error.message : "Login failed");
    } finally {
      setIsLoggingIn(false);
    }
  }, [applyVariantSelections, loadSubscriptionInfo, providerIdpId]);

  // Logout handler
  const handleLogout = useCallback(async () => {
    await window.openNow.logout();
    setAuthSession(null);
    setGames([]);
    setLibraryGames([]);
    setVariantByGameId({});
    resetLaunchRuntime();
    setNavbarActiveSession(null);
    setIsResumingNavbarSession(false);
    setSubscriptionInfo(null);
    setCurrentPage("home");
    const publicGames = await window.openNow.fetchPublicGames();
    setGames(publicGames);
    setSource("public");
    applyVariantSelections(publicGames);
  }, [applyVariantSelections, resetLaunchRuntime]);

  // Load games handler
  const loadGames = useCallback(async (targetSource: GameSource) => {
    setIsLoadingGames(true);
    try {
      const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
      const baseUrl = effectiveStreamingBaseUrl;

      let result: GameInfo[] = [];
      if (targetSource === "main" && token) {
        result = await window.openNow.fetchMainGames({ token, providerStreamingBaseUrl: baseUrl });
      } else if (targetSource === "library" && token) {
        result = await window.openNow.fetchLibraryGames({ token, providerStreamingBaseUrl: baseUrl });
        setLibraryGames(result);
        applyVariantSelections(result);
      } else if (targetSource === "public") {
        result = await window.openNow.fetchPublicGames();
      }

      if (targetSource !== "library") {
        setGames(result);
        setSource(targetSource);
        setSelectedGameId(result[0]?.id ?? "");
        applyVariantSelections(result);
      }
    } catch (error) {
      console.error("Failed to load games:", error);
    } finally {
      setIsLoadingGames(false);
    }
  }, [applyVariantSelections, authSession, effectiveStreamingBaseUrl]);

  const handleSelectGameVariant = useCallback((gameId: string, variantId: string): void => {
    setVariantByGameId((prev) => {
      if (prev[gameId] === variantId) {
        return prev;
      }
      return { ...prev, [gameId]: variantId };
    });
  }, []);

  const claimAndConnectSession = useCallback(async (existingSession: ActiveSessionInfo): Promise<void> => {
    const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
    if (!token) {
      throw new Error("Missing token for session resume");
    }
    if (!existingSession.serverIp) {
      throw new Error("Active session is missing server address. Start the game again to create a new session.");
    }

    const claimed = await window.openNow.claimSession({
      token,
      streamingBaseUrl: effectiveStreamingBaseUrl,
      serverIp: existingSession.serverIp,
      sessionId: existingSession.sessionId,
      settings: {
        resolution: settings.resolution,
        fps: settings.fps,
        maxBitrateMbps: settings.maxBitrateMbps,
        codec: settings.codec,
        colorQuality: settings.colorQuality,
      },
    });

    console.log("Claimed session:", {
      sessionId: claimed.sessionId,
      signalingServer: claimed.signalingServer,
      signalingUrl: claimed.signalingUrl,
      status: claimed.status,
    });

    await sleep(1000);

    setSession(claimed);
    sessionRef.current = claimed;
    setQueuePosition(undefined);
    setStreamStatus("connecting");
    await window.openNow.connectSignaling({
      sessionId: claimed.sessionId,
      signalingServer: claimed.signalingServer,
      signalingUrl: claimed.signalingUrl,
    });
  }, [authSession, effectiveStreamingBaseUrl, settings]);

  // Play game handler
  const handlePlayGame = useCallback(async (game: GameInfo) => {
    if (!selectedProvider) return;

    if (launchInFlightRef.current || streamStatus !== "idle") {
      console.warn("Ignoring play request: launch already in progress or stream not idle", {
        inFlight: launchInFlightRef.current,
        streamStatus,
      });
      return;
    }

    launchInFlightRef.current = true;
    let loadingStep: StreamLoadingStatus = "queue";
    const updateLoadingStep = (next: StreamLoadingStatus): void => {
      loadingStep = next;
      setStreamStatus(next);
    };

    setSessionStartedAtMs(null);
    setSessionElapsedSeconds(0);
    setStreamWarning(null);
    setLaunchError(null);
    const selectedVariantId = variantByGameId[game.id] ?? defaultVariantId(game);
    const selectedVariant = getSelectedVariant(game, selectedVariantId);
    setStreamingGame(game);
    setStreamingStore(selectedVariant?.store ?? null);
    updateLoadingStep("queue");
    setQueuePosition(undefined);

    try {
      const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;

      // Resolve appId
      let appId: string | null = null;
      if (isNumericId(selectedVariantId)) {
        appId = selectedVariantId;
      } else if (isNumericId(game.launchAppId)) {
        appId = game.launchAppId;
      }

      if (!appId && token) {
        try {
          const resolved = await window.openNow.resolveLaunchAppId({
            token,
            providerStreamingBaseUrl: effectiveStreamingBaseUrl,
            appIdOrUuid: game.uuid ?? selectedVariantId,
          });
          if (resolved && isNumericId(resolved)) {
            appId = resolved;
          }
        } catch {
          // Ignore resolution errors
        }
      }

      if (!appId) {
        throw new Error("Could not resolve numeric appId for this game");
      }

      // Check for active sessions first
      if (token) {
        try {
          const activeSessions = await window.openNow.getActiveSessions(token, effectiveStreamingBaseUrl);
          if (activeSessions.length > 0) {
            const existingSession = activeSessions[0];
            await claimAndConnectSession(existingSession);
            setNavbarActiveSession(null);
            return;
          }
        } catch (error) {
          console.error("Failed to claim/resume session:", error);
          // Continue to create new session
        }
      }

      // Create new session
      const newSession = await window.openNow.createSession({
        token: token || undefined,
        streamingBaseUrl: effectiveStreamingBaseUrl,
        appId,
        internalTitle: game.title,
        accountLinked: game.playType !== "INSTALL_TO_PLAY",
        zone: "prod",
        settings: {
          resolution: settings.resolution,
          fps: settings.fps,
          maxBitrateMbps: settings.maxBitrateMbps,
          codec: settings.codec,
          colorQuality: settings.colorQuality,
        },
      });

      setSession(newSession);
      setQueuePosition(newSession.queuePosition);

      // Poll for readiness.
      // Queue mode: no timeout - users wait indefinitely and see position updates.
      // Setup/Starting mode: 180s timeout applies while machine is being allocated.
      let finalSession: SessionInfo | null = null;
      let isInQueueMode = isSessionInQueue(newSession);
      let timeoutStartAttempt = 1;
      const maxAttempts = Math.ceil(SESSION_READY_TIMEOUT_MS / SESSION_READY_POLL_INTERVAL_MS);
      let attempt = 0;

      while (true) {
        attempt++;
        await sleep(SESSION_READY_POLL_INTERVAL_MS);

        const polled = await window.openNow.pollSession({
          token: token || undefined,
          streamingBaseUrl: newSession.streamingBaseUrl ?? effectiveStreamingBaseUrl,
          serverIp: newSession.serverIp,
          zone: newSession.zone,
          sessionId: newSession.sessionId,
        });

        setSession(polled);
        setQueuePosition(polled.queuePosition);

        // Check if queue just cleared - transition from queue mode to setup mode
        const wasInQueueMode = isInQueueMode;
        isInQueueMode = isSessionInQueue(polled);
        if (wasInQueueMode && !isInQueueMode) {
          // Queue just cleared, start timeout counting from now
          timeoutStartAttempt = attempt;
        }

        console.log(
          `Poll attempt ${attempt}: status=${polled.status}, seatSetupStep=${polled.seatSetupStep ?? "n/a"}, queuePosition=${polled.queuePosition ?? "n/a"}, serverIp=${polled.serverIp}, queueMode=${isInQueueMode}`,
        );

        if (isSessionReadyForConnect(polled.status)) {
          finalSession = polled;
          break;
        }

        // Update status based on session state
        if (isInQueueMode) {
          updateLoadingStep("queue");
        } else if (polled.status === 1) {
          updateLoadingStep("setup");
        }

        // Only check timeout when NOT in queue mode (i.e., during setup/starting)
        if (!isInQueueMode && attempt - timeoutStartAttempt >= maxAttempts) {
          throw new Error(`Session did not become ready in time (${Math.round(SESSION_READY_TIMEOUT_MS / 1000)}s)`);
        }
      }

      // finalSession is guaranteed to be set here (we only exit the loop via break when session is ready)
      // Timeout only applies during setup/starting phase, not during queue wait

      setQueuePosition(undefined);
      updateLoadingStep("connecting");

      // Use the polled session data which has the latest signaling info
      const sessionToConnect = sessionRef.current ?? finalSession ?? newSession;
      console.log("Connecting signaling with:", {
        sessionId: sessionToConnect.sessionId,
        signalingServer: sessionToConnect.signalingServer,
        signalingUrl: sessionToConnect.signalingUrl,
        status: sessionToConnect.status,
      });

      await window.openNow.connectSignaling({
        sessionId: sessionToConnect.sessionId,
        signalingServer: sessionToConnect.signalingServer,
        signalingUrl: sessionToConnect.signalingUrl,
      });
    } catch (error) {
      console.error("Launch failed:", error);
      setLaunchError(toLaunchErrorState(error, loadingStep));
      await window.openNow.disconnectSignaling().catch(() => {});
      clientRef.current?.dispose();
      clientRef.current = null;
      resetLaunchRuntime({ keepLaunchError: true, keepStreamingContext: true });
      void refreshNavbarActiveSession();
    } finally {
      launchInFlightRef.current = false;
    }
  }, [
    authSession,
    claimAndConnectSession,
    effectiveStreamingBaseUrl,
    refreshNavbarActiveSession,
    resetLaunchRuntime,
    selectedProvider,
    settings,
    streamStatus,
    variantByGameId,
  ]);

  const handleResumeFromNavbar = useCallback(async () => {
    if (!selectedProvider || !navbarActiveSession || isResumingNavbarSession) {
      return;
    }
    if (launchInFlightRef.current || streamStatus !== "idle") {
      return;
    }

    launchInFlightRef.current = true;
    setIsResumingNavbarSession(true);
    let loadingStep: StreamLoadingStatus = "setup";
    const updateLoadingStep = (next: StreamLoadingStatus): void => {
      loadingStep = next;
      setStreamStatus(next);
    };

    setLaunchError(null);
    setQueuePosition(undefined);
    setSessionStartedAtMs(null);
    setSessionElapsedSeconds(0);
    setStreamWarning(null);
    setStreamingStore(null);
    updateLoadingStep("setup");

    try {
      await claimAndConnectSession(navbarActiveSession);
      setNavbarActiveSession(null);
    } catch (error) {
      console.error("Navbar resume failed:", error);
      setLaunchError(toLaunchErrorState(error, loadingStep));
      await window.openNow.disconnectSignaling().catch(() => {});
      clientRef.current?.dispose();
      clientRef.current = null;
      resetLaunchRuntime({ keepLaunchError: true });
      void refreshNavbarActiveSession();
    } finally {
      launchInFlightRef.current = false;
      setIsResumingNavbarSession(false);
    }
  }, [
    claimAndConnectSession,
    isResumingNavbarSession,
    navbarActiveSession,
    refreshNavbarActiveSession,
    resetLaunchRuntime,
    selectedProvider,
    streamStatus,
  ]);

  // Stop stream handler
  const handleStopStream = useCallback(async () => {
    try {
      resolveExitPrompt(false);
      await window.openNow.disconnectSignaling();

      const current = sessionRef.current;
      if (current) {
        const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
        await window.openNow.stopSession({
          token: token || undefined,
          streamingBaseUrl: current.streamingBaseUrl,
          serverIp: current.serverIp,
          zone: current.zone,
          sessionId: current.sessionId,
        });
      }

      clientRef.current?.dispose();
      clientRef.current = null;
      setNavbarActiveSession(null);
      resetLaunchRuntime();
      void refreshNavbarActiveSession();
    } catch (error) {
      console.error("Stop failed:", error);
    }
  }, [authSession, refreshNavbarActiveSession, resetLaunchRuntime, resolveExitPrompt]);

  const handleDismissLaunchError = useCallback(async () => {
    await window.openNow.disconnectSignaling().catch(() => {});
    clientRef.current?.dispose();
    clientRef.current = null;
    resetLaunchRuntime();
    void refreshNavbarActiveSession();
  }, [refreshNavbarActiveSession, resetLaunchRuntime]);

  const releasePointerLockIfNeeded = useCallback(async () => {
    if (document.pointerLockElement) {
      document.exitPointerLock();
      setEscHoldReleaseIndicator({ visible: false, progress: 0 });
      await sleep(75);
    }
  }, []);

  const handlePromptedStopStream = useCallback(async () => {
    if (streamStatus === "idle") {
      return;
    }

    await releasePointerLockIfNeeded();

    const gameName = (streamingGame?.title || "this game").trim();
    const shouldExit = await requestExitPrompt(gameName);
    if (!shouldExit) {
      return;
    }

    await handleStopStream();
  }, [handleStopStream, releasePointerLockIfNeeded, requestExitPrompt, streamStatus, streamingGame?.title]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null;
      const isTyping = !!target && (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.isContentEditable
      );
      if (isTyping) {
        return;
      }

      if (pluginSafetyPromptOpen) {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          e.stopImmediatePropagation();
          dismissPluginSafety();
        } else if (e.key === "Enter") {
          e.preventDefault();
          e.stopPropagation();
          e.stopImmediatePropagation();
          acknowledgePluginSafety();
        }
        return;
      }

      if (exitPrompt.open) {
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          e.stopImmediatePropagation();
          handleExitPromptCancel();
        } else if (e.key === "Enter") {
          e.preventDefault();
          e.stopPropagation();
          e.stopImmediatePropagation();
          handleExitPromptConfirm();
        }
        return;
      }

      const isPasteShortcut = e.key.toLowerCase() === "v" && !e.altKey && (isMac ? e.metaKey : e.ctrlKey);
      if (streamStatus === "streaming" && isPasteShortcut) {
        // Always stop local/browser paste behavior while streaming.
        // If clipboard paste is enabled, send clipboard text into the stream.
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();

        if (settings.clipboardPaste) {
          void (async () => {
            const client = clientRef.current;
            if (!client) return;

            try {
              const text = await navigator.clipboard.readText();
              if (text && client.sendText(text) > 0) {
                return;
              }
            } catch (error) {
              console.warn("Clipboard read failed, falling back to paste shortcut:", error);
            }

            client.sendPasteShortcut(isMac);
          })();
        }
        return;
      }

      if (isShortcutMatch(e, shortcuts.toggleStats)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        setShowStatsOverlay((prev) => !prev);
        return;
      }

      if (isShortcutMatch(e, shortcuts.togglePointerLock)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        if (streamStatus === "streaming" && videoRef.current) {
          if (document.pointerLockElement === videoRef.current) {
            document.exitPointerLock();
          } else {
            void requestEscLockedPointerCapture(videoRef.current);
          }
        }
        return;
      }

      if (isShortcutMatch(e, shortcuts.stopStream)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        void handlePromptedStopStream();
        return;
      }

      if (isShortcutMatch(e, shortcuts.toggleAntiAfk)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        if (streamStatus === "streaming") {
          setAntiAfkEnabled((prev) => !prev);
        }
        return;
      }

      if (isShortcutMatch(e, shortcuts.toggleMicrophone)) {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation();
        if (streamStatus === "streaming") {
          clientRef.current?.toggleMicrophone();
        }
      }
    };

    // Use capture phase so app shortcuts run before stream input capture listeners.
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [
    acknowledgePluginSafety,
    dismissPluginSafety,
    exitPrompt.open,
    handleExitPromptCancel,
    handleExitPromptConfirm,
    handlePromptedStopStream,
    pluginSafetyPromptOpen,
    requestEscLockedPointerCapture,
    settings.clipboardPaste,
    shortcuts,
    streamStatus,
  ]);

  // Filter games by search
  const filteredGames = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return games;
    return games.filter((g) => g.title.toLowerCase().includes(query));
  }, [games, searchQuery]);

  const filteredLibraryGames = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return libraryGames;
    return libraryGames.filter((g) => g.title.toLowerCase().includes(query));
  }, [libraryGames, searchQuery]);

  const gameTitleByAppId = useMemo(() => {
    const titles = new Map<number, string>();
    const allKnownGames = [...games, ...libraryGames];

    for (const game of allKnownGames) {
      const idsForGame = new Set<number>();
      const launchId = parseNumericId(game.launchAppId);
      if (launchId !== null) {
        idsForGame.add(launchId);
      }
      for (const variant of game.variants) {
        const variantId = parseNumericId(variant.id);
        if (variantId !== null) {
          idsForGame.add(variantId);
        }
      }
      for (const appId of idsForGame) {
        if (!titles.has(appId)) {
          titles.set(appId, game.title);
        }
      }
    }

    return titles;
  }, [games, libraryGames]);

  const activeSessionGameTitle = useMemo(() => {
    if (!navbarActiveSession) return null;
    const mappedTitle = gameTitleByAppId.get(navbarActiveSession.appId);
    if (mappedTitle) {
      return mappedTitle;
    }
    if (session?.sessionId === navbarActiveSession.sessionId && streamingGame?.title) {
      return streamingGame.title;
    }
    return null;
  }, [gameTitleByAppId, navbarActiveSession, session?.sessionId, streamingGame?.title]);

  // Show login screen if not authenticated
  if (!authSession) {
    return (
      <>
        <LoginScreen
          providers={providers}
          selectedProviderId={providerIdpId}
          onProviderChange={setProviderIdpId}
          onLogin={handleLogin}
          isLoading={isLoggingIn}
          error={loginError}
          isInitializing={isInitializing}
          statusMessage={startupStatusMessage}
        />
        {controllerConnected && (
          <div className="controller-hint">
            <span>D-pad Navigate</span>
            <span>A Select</span>
            <span>B Back</span>
          </div>
        )}
      </>
    );
  }

  const showLaunchOverlay = streamStatus !== "idle" || launchError !== null;

  // Show stream lifecycle (waiting/connecting/streaming/failure)
  if (showLaunchOverlay) {
    const loadingStatus = launchError ? launchError.stage : toLoadingStatus(streamStatus);
    return (
      <>
        {streamStatus !== "idle" && (
          <StreamView
            videoRef={videoRef}
            audioRef={audioRef}
            stats={diagnostics}
            showStats={showStatsOverlay}
            shortcuts={{
              toggleStats: formatShortcutForDisplay(settings.shortcutToggleStats, isMac),
              togglePointerLock: formatShortcutForDisplay(settings.shortcutTogglePointerLock, isMac),
              stopStream: formatShortcutForDisplay(settings.shortcutStopStream, isMac),
              toggleMicrophone: formatShortcutForDisplay(settings.shortcutToggleMicrophone, isMac),
            }}
            hideStreamButtons={settings.hideStreamButtons}
            serverRegion={session?.serverIp}
            connectedControllers={diagnostics.connectedGamepads}
            antiAfkEnabled={antiAfkEnabled}
            escHoldReleaseIndicator={escHoldReleaseIndicator}
            exitPrompt={exitPrompt}
            sessionElapsedSeconds={sessionElapsedSeconds}
            sessionClockShowEveryMinutes={settings.sessionClockShowEveryMinutes}
            sessionClockShowDurationSeconds={settings.sessionClockShowDurationSeconds}
            streamWarning={streamWarning}
            isConnecting={streamStatus === "connecting"}
            gameTitle={streamingGame?.title ?? "Game"}
            platformStore={streamingStore ?? undefined}
            onToggleFullscreen={() => {
              if (document.fullscreenElement) {
                document.exitFullscreen().catch(() => {});
              } else {
                document.documentElement.requestFullscreen().catch(() => {});
              }
            }}
            onConfirmExit={handleExitPromptConfirm}
            onCancelExit={handleExitPromptCancel}
            onEndSession={() => {
              void handlePromptedStopStream();
            }}
            onToggleMicrophone={() => {
              clientRef.current?.toggleMicrophone();
            }}
          />
        )}
        {streamStatus !== "streaming" && (
          <StreamLoading
            gameTitle={streamingGame?.title ?? "Game"}
            gameCover={streamingGame?.imageUrl}
            platformStore={streamingStore ?? undefined}
            status={loadingStatus}
            queuePosition={queuePosition}
            error={
              launchError
                ? {
                    title: launchError.title,
                    description: launchError.description,
                    code: launchError.codeLabel,
                  }
                : undefined
            }
            onCancel={() => {
              if (launchError) {
                void handleDismissLaunchError();
                return;
              }
              void handlePromptedStopStream();
            }}
          />
        )}
        {controllerConnected && streamStatus !== "streaming" && (
          <div className="controller-hint controller-hint--overlay">
            <span>D-pad Navigate</span>
            <span>A Select</span>
            <span>B Back</span>
          </div>
        )}
      </>
    );
  }

  // Main app layout
  return (
    <div className="app-container">
      {startupRefreshNotice && (
        <div className={`auth-refresh-notice auth-refresh-notice--${startupRefreshNotice.tone}`}>
          {startupRefreshNotice.text}
        </div>
      )}
      <Navbar
        currentPage={currentPage}
        onNavigate={navigateToPage}
        user={authSession.user}
        subscription={subscriptionInfo}
        activeSession={navbarActiveSession}
        activeSessionGameTitle={activeSessionGameTitle}
        isResumingSession={isResumingNavbarSession}
        onResumeSession={() => {
          void handleResumeFromNavbar();
        }}
        onLogout={handleLogout}
      />

      {pluginSafetyPromptOpen && (
        <div className="plugin-safety-backdrop" onClick={dismissPluginSafety}>
          <div
            className="plugin-safety-modal"
            role="dialog"
            aria-modal="true"
            aria-labelledby="plugin-safety-title"
            onClick={(event) => event.stopPropagation()}
          >
            <h2 id="plugin-safety-title">Plugin Safety Warning</h2>
            <p>
              Plugins run custom scripts with access to input automation and client theming.
              They can send keyboard, mouse, and controller events to your cloud session.
            </p>
            <p>
              Only run scripts you trust and review yourself. OpenNOW cannot verify third-party plugin safety.
              This warning appears once.
            </p>
            <div className="plugin-safety-actions">
              <button
                type="button"
                className="plugin-safety-btn plugin-safety-btn--ghost"
                onClick={dismissPluginSafety}
              >
                Cancel
              </button>
              <button
                type="button"
                className="plugin-safety-btn plugin-safety-btn--confirm"
                onClick={acknowledgePluginSafety}
              >
                I Understand
              </button>
            </div>
          </div>
        </div>
      )}

      <main className="main-content">
        {currentPage === "home" && (
          <HomePage
            games={filteredGames}
            source={source}
            onSourceChange={loadGames}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            onPlayGame={handlePlayGame}
            isLoading={isLoadingGames}
            selectedGameId={selectedGameId}
            onSelectGame={setSelectedGameId}
            selectedVariantByGameId={variantByGameId}
            onSelectGameVariant={handleSelectGameVariant}
          />
        )}

        {currentPage === "library" && (
          <LibraryPage
            games={filteredLibraryGames}
            searchQuery={searchQuery}
            onSearchChange={setSearchQuery}
            onPlayGame={handlePlayGame}
            isLoading={isLoadingGames}
            selectedGameId={selectedGameId}
            onSelectGame={setSelectedGameId}
            selectedVariantByGameId={variantByGameId}
            onSelectGameVariant={handleSelectGameVariant}
          />
        )}

        {currentPage === "plugins" && (
          <PluginPage
            plugins={plugins}
            selectedPluginId={selectedPluginId}
            runStates={pluginRunStates}
            inputReady={Boolean(clientRef.current?.isInputReady())}
            onSelectPlugin={setSelectedPluginId}
            onCreatePlugin={createPlugin}
            onDeletePlugin={deletePlugin}
            onRunPlugin={(id) => {
              void executePlugin(id);
            }}
            onUpdatePlugin={updatePlugin}
          />
        )}

        {currentPage === "settings" && (
          <SettingsPage
            settings={settings}
            regions={regions}
            onSettingChange={updateSetting}
          />
        )}
      </main>
      {controllerConnected && (
        <div className="controller-hint">
          <span>D-pad Navigate</span>
          <span>A Select</span>
          <span>B Back</span>
          <span>LB/RB Tabs</span>
        </div>
      )}
    </div>
  );
}
