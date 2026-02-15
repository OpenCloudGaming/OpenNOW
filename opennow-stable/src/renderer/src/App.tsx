import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { JSX } from "react";

import type {
  ActiveSessionInfo,
  AuthSession,
  AuthUser,
  GameInfo,
  LoginProvider,
  MainToRendererSignalingEvent,
  SessionInfo,
  Settings,
  StreamRegion,
  VideoCodec,
} from "@shared/gfn";

import { GfnWebRtcClient, type StreamDiagnostics } from "./gfn/webrtcClient";
import { formatShortcutForDisplay, isShortcutMatch, normalizeShortcut } from "./shortcuts";

// UI Components
import { LoginScreen } from "./components/LoginScreen";
import { Navbar } from "./components/Navbar";
import { HomePage } from "./components/HomePage";
import { LibraryPage } from "./components/LibraryPage";
import { SettingsPage } from "./components/SettingsPage";
import { StreamLoading } from "./components/StreamLoading";
import { StreamView } from "./components/StreamView";

const codecOptions: VideoCodec[] = ["H264", "H265", "AV1"];
const resolutionOptions = ["1280x720", "1920x1080", "2560x1440", "3840x2160", "2560x1080", "3440x1440"];
const fpsOptions = [30, 60, 120, 144, 240];

type GameSource = "main" | "library" | "public";
type AppPage = "home" | "library" | "settings";
type StreamStatus = "idle" | "queue" | "setup" | "starting" | "connecting" | "streaming";
type ExitPromptState = { open: boolean; gameTitle: string };

const isMac = navigator.platform.toLowerCase().includes("mac");

const DEFAULT_SHORTCUTS = {
  shortcutToggleStats: "F3",
  shortcutTogglePointerLock: "F8",
  shortcutStopStream: "Ctrl+Shift+Q",
  shortcutToggleAntiAfk: "Ctrl+Shift+K",
} as const;

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, ms));
}

function isNumericId(value: string | undefined): value is string {
  if (!value) return false;
  return /^\d+$/.test(value);
}

function defaultVariantId(game: GameInfo): string {
  const fallback = game.variants[0]?.id;
  const preferred = game.variants[game.selectedVariantIndex]?.id;
  return preferred ?? fallback ?? game.id;
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
    gpuType: "",
    serverRegion: "",
  };
}

function isSessionLimitError(error: unknown): boolean {
  if (error && typeof error === "object" && "gfnErrorCode" in error) {
    return error.gfnErrorCode === 3237093643;
  }
  if (error instanceof Error) {
    const msg = error.message.toUpperCase();
    return msg.includes("SESSION LIMIT") || msg.includes("INSUFFICIENT_PLAYABILITY");
  }
  return false;
}

export function App(): JSX.Element {
  // Auth State
  const [authSession, setAuthSession] = useState<AuthSession | null>(null);
  const [providers, setProviders] = useState<LoginProvider[]>([]);
  const [providerIdpId, setProviderIdpId] = useState("");
  const [isLoggingIn, setIsLoggingIn] = useState(false);
  const [loginError, setLoginError] = useState<string | null>(null);
  const [isInitializing, setIsInitializing] = useState(true);

  // Navigation
  const [currentPage, setCurrentPage] = useState<AppPage>("home");

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
    decoderPreference: "auto",
    encoderPreference: "auto",
    colorQuality: "10bit_420",
    region: "",
    clipboardPaste: false,
    mouseSensitivity: 1,
    shortcutToggleStats: DEFAULT_SHORTCUTS.shortcutToggleStats,
    shortcutTogglePointerLock: DEFAULT_SHORTCUTS.shortcutTogglePointerLock,
    shortcutStopStream: DEFAULT_SHORTCUTS.shortcutStopStream,
    shortcutToggleAntiAfk: DEFAULT_SHORTCUTS.shortcutToggleAntiAfk,
    windowWidth: 1400,
    windowHeight: 900,
  });
  const [settingsLoaded, setSettingsLoaded] = useState(false);
  const [regions, setRegions] = useState<StreamRegion[]>([]);

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
  const [queuePosition, setQueuePosition] = useState<number | undefined>();

  // Refs
  const videoRef = useRef<HTMLVideoElement | null>(null);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const clientRef = useRef<GfnWebRtcClient | null>(null);
  const sessionRef = useRef<SessionInfo | null>(null);
  const hasInitializedRef = useRef(false);
  const regionsRequestRef = useRef(0);
  const launchInFlightRef = useRef(false);
  const exitPromptResolverRef = useRef<((confirmed: boolean) => void) | null>(null);

  // Session ref sync
  useEffect(() => {
    sessionRef.current = session;
  }, [session]);

  // Derived state
  const selectedProvider = useMemo(() => {
    return providers.find((p) => p.idpId === providerIdpId) ?? authSession?.provider ?? null;
  }, [providers, providerIdpId, authSession]);

  const effectiveStreamingBaseUrl = useMemo(() => {
    return selectedProvider?.streamingServiceUrl ?? "";
  }, [selectedProvider]);

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

        // Load providers and session
        const [providerList, persistedSession] = await Promise.all([
          window.openNow.getLoginProviders(),
          window.openNow.getAuthSession(),
        ]);

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

          // Load games
          try {
            const mainGames = await window.openNow.fetchMainGames({
              token,
              providerStreamingBaseUrl: persistedSession.provider.streamingServiceUrl,
            });
            setGames(mainGames);
            setSource("main");
            setSelectedGameId(mainGames[0]?.id ?? "");
            setVariantByGameId(
              mainGames.reduce((acc, g) => {
                acc[g.id] = defaultVariantId(g);
                return acc;
              }, {} as Record<string, string>)
            );

            // Also load library
            const libGames = await window.openNow.fetchLibraryGames({
              token,
              providerStreamingBaseUrl: persistedSession.provider.streamingServiceUrl,
            });
            setLibraryGames(libGames);
          } catch {
            // Fallback to public games
            const publicGames = await window.openNow.fetchPublicGames();
            setGames(publicGames);
            setSource("public");
          }
        } else {
          // Load public games for non-logged in users
          const publicGames = await window.openNow.fetchPublicGames();
          setGames(publicGames);
          setSource("public");
        }
      } catch (error) {
        console.error("Initialization failed:", error);
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
    return { toggleStats, togglePointerLock, stopStream, toggleAntiAfk };
  }, [
    settings.shortcutToggleStats,
    settings.shortcutTogglePointerLock,
    settings.shortcutStopStream,
    settings.shortcutToggleAntiAfk,
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
              onLog: (line: string) => console.log(`[WebRTC] ${line}`),
              onStats: (stats) => setDiagnostics(stats),
              onEscHoldProgress: (visible, progress) => {
                setEscHoldReleaseIndicator({ visible, progress });
              },
            });
          }

          if (clientRef.current) {
            await clientRef.current.handleOffer(event.sdp, activeSession, {
              codec: settings.codec,
              colorQuality: settings.colorQuality,
              resolution: settings.resolution,
              fps: settings.fps,
              maxBitrateKbps: settings.maxBitrateMbps * 1000,
            });
            setStreamStatus("streaming");
          }
        } else if (event.type === "remote-ice") {
          await clientRef.current?.addRemoteCandidate(event.candidate);
        } else if (event.type === "disconnected") {
          console.warn("Signaling disconnected:", event.reason);
          clientRef.current?.dispose();
          clientRef.current = null;
          setStreamStatus("idle");
          setSession(null);
          setStreamingGame(null);
          setEscHoldReleaseIndicator({ visible: false, progress: 0 });
          setDiagnostics(defaultDiagnostics());
          launchInFlightRef.current = false;
        } else if (event.type === "error") {
          console.error("Signaling error:", event.message);
        }
      } catch (error) {
        console.error("Signaling event error:", error);
      }
    });

    return () => unsubscribe();
  }, [settings]);

  // Save settings when changed
  const updateSetting = useCallback(async <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setSettings((prev) => ({ ...prev, [key]: value }));
    if (settingsLoaded) {
      await window.openNow.setSetting(key, value);
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

      // Load games
      const mainGames = await window.openNow.fetchMainGames({
        token,
        providerStreamingBaseUrl: session.provider.streamingServiceUrl,
      });
      setGames(mainGames);
      setSource("main");
      setSelectedGameId(mainGames[0]?.id ?? "");

      // Load library
      const libGames = await window.openNow.fetchLibraryGames({
        token,
        providerStreamingBaseUrl: session.provider.streamingServiceUrl,
      });
      setLibraryGames(libGames);
    } catch (error) {
      setLoginError(error instanceof Error ? error.message : "Login failed");
    } finally {
      setIsLoggingIn(false);
    }
  }, [providerIdpId]);

  // Logout handler
  const handleLogout = useCallback(async () => {
    await window.openNow.logout();
    setAuthSession(null);
    setGames([]);
    setLibraryGames([]);
    setCurrentPage("home");
    const publicGames = await window.openNow.fetchPublicGames();
    setGames(publicGames);
    setSource("public");
  }, []);

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
      } else if (targetSource === "public") {
        result = await window.openNow.fetchPublicGames();
      }

      if (targetSource !== "library") {
        setGames(result);
        setSource(targetSource);
        setSelectedGameId(result[0]?.id ?? "");
      }
    } catch (error) {
      console.error("Failed to load games:", error);
    } finally {
      setIsLoadingGames(false);
    }
  }, [authSession, effectiveStreamingBaseUrl]);

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

    setStreamingGame(game);
    setStreamStatus("queue");
    setQueuePosition(undefined);

    try {
      const token = authSession?.tokens.idToken ?? authSession?.tokens.accessToken;
      const selectedVariantId = variantByGameId[game.id] ?? defaultVariantId(game);

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
            // Show conflict dialog - for now just claim the first one
            const existingSession = activeSessions[0];
            const claimed = await window.openNow.claimSession({
              token,
              streamingBaseUrl: effectiveStreamingBaseUrl,
              serverIp: existingSession.serverIp ?? "",
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

            // Wait a moment for the server to be ready
            await sleep(1000);

            setSession(claimed);
            // Sync ref immediately — useEffect is async and may not fire
            // before the signaling offer arrives
            sessionRef.current = claimed;
            setStreamStatus("connecting");
            await window.openNow.connectSignaling({
              sessionId: claimed.sessionId,
              signalingServer: claimed.signalingServer,
              signalingUrl: claimed.signalingUrl,
            });
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

      // Poll for readiness
      let readyCount = 0;
      for (let attempt = 1; attempt <= 30; attempt++) {
        await sleep(2000);

        const polled = await window.openNow.pollSession({
          token: token || undefined,
          streamingBaseUrl: newSession.streamingBaseUrl ?? effectiveStreamingBaseUrl,
          serverIp: newSession.serverIp,
          zone: newSession.zone,
          sessionId: newSession.sessionId,
        });

        setSession(polled);

        console.log(`Poll attempt ${attempt}: status=${polled.status}, signalingUrl=${polled.signalingUrl}`);

        if (polled.status === 2 || polled.status === 3) {
          readyCount++;
          console.log(`Ready count: ${readyCount}/3`);
          if (readyCount >= 3) break;
        }

        // Update status based on session state
        if (polled.status === 1) {
          setStreamStatus("setup");
        }
      }

      if (readyCount < 3) {
        throw new Error("Session did not become ready in time");
      }

      setStreamStatus("connecting");

      // Use the polled session data which has the latest signaling info
      const finalSession = sessionRef.current ?? newSession;
      console.log("Connecting signaling with:", {
        sessionId: finalSession.sessionId,
        signalingServer: finalSession.signalingServer,
        signalingUrl: finalSession.signalingUrl,
        status: finalSession.status,
      });

      await window.openNow.connectSignaling({
        sessionId: finalSession.sessionId,
        signalingServer: finalSession.signalingServer,
        signalingUrl: finalSession.signalingUrl,
      });
    } catch (error) {
      console.error("Launch failed:", error);
      setStreamStatus("idle");
      setStreamingGame(null);
    } finally {
      launchInFlightRef.current = false;
    }
  }, [authSession, effectiveStreamingBaseUrl, settings, selectedProvider, streamStatus, variantByGameId]);

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
      setSession(null);
      setStreamStatus("idle");
      setStreamingGame(null);
      setEscHoldReleaseIndicator({ visible: false, progress: 0 });
      setDiagnostics(defaultDiagnostics());
    } catch (error) {
      console.error("Stop failed:", error);
    }
  }, [authSession, resolveExitPrompt]);

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
      }
    };

    // Use capture phase so app shortcuts run before stream input capture listeners.
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
  }, [
    exitPrompt.open,
    handleExitPromptCancel,
    handleExitPromptConfirm,
    handlePromptedStopStream,
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

  // Show login screen if not authenticated
  if (!authSession) {
    return (
      <LoginScreen
        providers={providers}
        selectedProviderId={providerIdpId}
        onProviderChange={setProviderIdpId}
        onLogin={handleLogin}
        isLoading={isLoggingIn}
        error={loginError}
        isInitializing={isInitializing}
      />
    );
  }

  // Show stream view if streaming
  if (streamStatus !== "idle") {
    return (
      <>
        <StreamView
          videoRef={videoRef}
          audioRef={audioRef}
          stats={diagnostics}
          showStats={showStatsOverlay}
          shortcuts={{
            toggleStats: formatShortcutForDisplay(settings.shortcutToggleStats, isMac),
            togglePointerLock: formatShortcutForDisplay(settings.shortcutTogglePointerLock, isMac),
            stopStream: formatShortcutForDisplay(settings.shortcutStopStream, isMac),
          }}
          serverRegion={session?.serverIp}
          connectedControllers={diagnostics.connectedGamepads}
          antiAfkEnabled={antiAfkEnabled}
          escHoldReleaseIndicator={escHoldReleaseIndicator}
          exitPrompt={exitPrompt}
          isConnecting={streamStatus === "connecting"}
          gameTitle={streamingGame?.title ?? "Game"}
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
        />
        {streamStatus !== "streaming" && (
          <StreamLoading
            gameTitle={streamingGame?.title ?? "Game"}
            gameCover={streamingGame?.imageUrl}
            status={streamStatus}
            queuePosition={queuePosition}
            onCancel={() => {
              void handlePromptedStopStream();
            }}
          />
        )}
      </>
    );
  }

  // Main app layout
  return (
    <div className="app-container">
      <Navbar
        currentPage={currentPage}
        onNavigate={setCurrentPage}
        user={authSession.user}
        onLogout={handleLogout}
      />

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
    </div>
  );
}
