import { Globe, Check, Search, X, Loader, Zap, Mic, FileDown, Wifi, Trash2, Heart, Users, ExternalLink, Monitor, Keyboard, Download, RefreshCcw, Info, Cpu, AlertTriangle } from "lucide-react";
import { useState, useCallback, useMemo, useEffect, useRef } from "react";
import type { JSX } from "react";

import type {
  Settings,
  StreamRegion,
  VideoCodec,
  ColorQuality,
  EntitledResolution,
  VideoAccelerationPreference,
  MicrophoneMode,
  PingResult,
  GameLanguage,
  MicrophonePermissionResult,
  ThankYouDataResult,
  ThankYouContributor,
  ThankYouSupporter,
  AppUpdaterState,
  NativeStreamerStatus,
  NativeVideoBackendCapability,
  NativeVideoBackendPreference,
} from "@shared/gfn";
import {
  createUnsupportedNativeStreamerStatus,
  isNativeStreamerSupportedPlatform,
  NATIVE_STREAMER_WINDOWS_ONLY_MESSAGE,
  colorQualityRequiresHevc,
  keyboardLayoutOptions,
  USER_FACING_COLOR_QUALITY_OPTIONS,
  USER_FACING_VIDEO_CODEC_OPTIONS,
} from "@shared/gfn";
import { formatShortcutForDisplay, normalizeShortcut, shortcutFromKeyboardEvent } from "../shortcuts";
import { getCodecDecodeBadgeState, type CodecTestResult } from "../lib/codecDiagnostics";
import { getAccentColorOption, getAccentColorOptions } from "../lib/uiCustomization";
import { useTranslation } from "../i18n";
import {
  clearStoredRegionPingResults,
  loadStoredRegionPingResults,
  saveStoredRegionPingResults,
} from "../utils/pingResultsStorage";

interface SettingsPageProps {
  settings: Settings;
  regions: StreamRegion[];
  codecResults: CodecTestResult[] | null;
  codecTesting: boolean;
  onRunCodecTest: () => Promise<void>;
  onSettingChange: <K extends keyof Settings>(key: K, value: Settings[K]) => void;
}

type ThanksLoadState = "idle" | "loading" | "loaded" | "error";

type SettingsSectionId = "stream" | "native-streamer" | "game" | "audio" | "input" | "interface" | "about" | "thanks";
type SettingsSearchScopeId =
  | "stream-region"
  | "stream-video"
  | "stream-codec-diagnostics"
  | "native-streamer"
  | "game"
  | "audio"
  | "input"
  | "interface"
  | "about"
  | "thanks";

const SETTINGS_SCOPE_SEARCH_TERMS: Record<SettingsSearchScopeId, readonly string[]> = {
  "stream-region": [
    "stream",
    "region",
    "latency",
    "ping",
    "server",
    "route",
    "auto best",
    "proxy",
    "vpn",
    "queue",
    "session",
    "sponsor",
    "github sponsor",
    "supporter",
  ],
  "stream-video": [
    "stream",
    "video",
    "quality",
    "codec",
    "fps",
    "resolution",
    "bitrate",
    "aspect ratio",
    "l4s",
    "cloud gsync",
    "video acceleration",
  ],
  "stream-codec-diagnostics": [
    "stream",
    "codec diagnostics",
    "diagnostics",
    "decode",
    "encode",
    "gpu",
    "cpu",
    "test codecs",
  ],
  "native-streamer": [
    "native",
    "streamer",
    "native streaming",
    "gstreamer",
    "backend",
    "directx",
    "dx11",
    "dx12",
    "cloud gsync",
    "diagnostics",
    "stats",
    "overlay",
    "experimental",
    "shortcuts",
    "alt-tab",
    "exit",
    "issue",
    "github",
    "discord",
    "report",
    "bug",
  ],
  game: ["game", "language", "keyboard layout", "store", "launch"],
  audio: ["audio", "microphone", "mic", "push to talk", "voice activity"],
  input: [
    "input",
    "mouse",
    "keyboard layout",
    "shortcut",
    "hotkey",
    "keybind",
    "controls",
    "anti afk",
    "pointer lock",
    "recording",
    "screenshot",
  ],
  interface: [
    "interface",
    "ui",
    "language",
    "locale",
    "translation",
    "app language",
    "accent color",
    "theme color",
    "overlay",
    "library",
    "fullscreen",
    "discord",
    "rich presence",
    "poster",
    "session timer",
    "session time left",
    "session countdown",
    "free tier time",
    "priority time",
    "ultimate time",
    "counter",
    "controller",
    "gamepad",
    "big picture",
  ],
  about: ["about", "update", "version", "logs", "cache", "download"],
  thanks: ["thanks", "contributors", "supporters", "sponsors", "community"],
};

const POSTER_SIZE_MIN = 75;
const POSTER_SIZE_MAX = 135;
const POSTER_SIZE_STEP = 5;

const codecOptions: VideoCodec[] = [...USER_FACING_VIDEO_CODEC_OPTIONS];

const accelerationOptions: { value: VideoAccelerationPreference; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "hardware", label: "Hardware" },
  { value: "software", label: "Software (CPU)" },
];

const allColorQualityOptions: { value: ColorQuality; label: string; description: string }[] = [
  { value: "8bit_420", label: "8-bit 4:2:0", description: "Most compatible" },
  { value: "8bit_444", label: "8-bit 4:4:4", description: "Sharper chroma" },
  { value: "10bit_420", label: "10-bit 4:2:0", description: "Higher bit depth" },
  { value: "10bit_444", label: "10-bit 4:4:4", description: "Highest chroma and bit depth" },
];

const colorQualityOptions: { value: ColorQuality; label: string; description: string }[] = [...allColorQualityOptions];

const nativeVideoBackendOptions: { value: NativeVideoBackendPreference; label: string; description: string }[] = [
  { value: "auto", label: "Auto", description: "Pick the default native path for the session" },
  { value: "d3d12", label: "DirectX 12", description: "Use the D3D12 decoder and renderer" },
  { value: "d3d11", label: "DirectX 11", description: "Use the D3D11 decoder and renderer" },
];

const APP_LANGUAGE_LABELS: Record<string, string> = {
  en: "English",
  es: "Español",
  fr: "Français",
  de: "Deutsch",
  ja: "日本語",
  zh: "中文",
  pl: "Polski",
  ru: "Русский",
  tr: "Türkçe",
  ko: "한국어",
  nl: "Nederlands",
  ro: "Română",
};

const accentColorOptions = getAccentColorOptions();

function getAppLanguageLabel(locale: string): string {
  return APP_LANGUAGE_LABELS[locale] ?? locale.toUpperCase();
}

function formatNativeVideoBackendName(backend: string | undefined): string {
  switch (backend) {
    case "d3d12":
      return "D3D12";
    case "d3d11":
      return "D3D11";
    case "videotoolbox":
      return "VideoToolbox";
    case "vaapi":
      return "VAAPI";
    case "v4l2":
      return "V4L2";
    case "vulkan":
      return "Vulkan";
    case "software":
      return "Software";
    default:
      return backend ?? "Unknown";
  }
}

function formatNativeVideoCodec(codec: string): string {
  switch (codec.toLowerCase()) {
    case "h264":
      return "H.264";
    case "h265":
      return "H.265";
    case "av1":
      return "AV1";
    default:
      return codec.toUpperCase();
  }
}

function getAvailableNativeCodecLabels(backend: NativeVideoBackendCapability | undefined): string[] {
  return backend?.codecs
    .filter((codec) => codec.available)
    .map((codec) => formatNativeVideoCodec(codec.codec)) ?? [];
}

function formatGstreamerRuntimeLabel(status: NativeStreamerStatus | null): string {
  switch (status?.gstreamerRuntime.source) {
    case "bundled":
      return status.gstreamerAvailable ? "Bundled Runtime Used" : "Bundled Runtime Found";
    case "system":
      return "System Runtime";
    case "missing":
      return "Runtime Missing";
    default:
      return "Runtime Unknown";
  }
}

function getGstreamerRuntimeBadgeClass(status: NativeStreamerStatus | null): string {
  if (status?.gstreamerRuntime.source === "bundled" && status.gstreamerAvailable) return "settings-inline-badge--codec-gpu";
  if (status?.gstreamerRuntime.source === "system" && status.gstreamerAvailable) return "settings-inline-badge--codec-testing";
  return "settings-inline-badge--updater-error";
}

/* ── Static fallbacks (used when MES API is unavailable) ─────────── */

interface ResolutionPreset {
  value: string;
  label: string;
}

interface FpsPreset {
  value: number;
}

interface AspectRatioPreset {
  value: string;
  label: string;
}

const STATIC_ASPECT_RATIO_PRESETS: AspectRatioPreset[] = [
  { value: "16:9", label: "16:9 (Widescreen)" },
  { value: "16:10", label: "16:10 (Widescreen)" },
  { value: "21:9", label: "21:9 (Ultrawide)" },
  { value: "32:9", label: "32:9 (Super Ultrawide)" },
];

const STATIC_RESOLUTION_PRESETS: ResolutionPreset[] = [
  { value: "1280x720", label: "720p (16:9)" },
  { value: "1280x800", label: "720p (16:10)" },
  { value: "1440x900", label: "WXGA (16:10)" },
  { value: "1680x1050", label: "WSXGA (16:10)" },
  { value: "1920x1080", label: "1080p (16:9)" },
  { value: "1920x1200", label: "1200p (16:10)" },
  { value: "2560x1080", label: "Ultrawide 1080p (21:9)" },
  { value: "2560x1440", label: "1440p (16:9)" },
  { value: "2560x1600", label: "1600p (16:10)" },
  { value: "3440x1440", label: "Ultrawide 1440p (21:9)" },
  { value: "3840x2160", label: "4K (16:9)" },
  { value: "3840x2400", label: "4K (16:10)" },
  { value: "5120x1440", label: "Super Ultrawide (32:9)" },
];

const STATIC_FPS_PRESETS: FpsPreset[] = [
  { value: 30 },
  { value: 60 },
  { value: 90 },
  { value: 120 },
  { value: 144 },
  { value: 165 },
  { value: 240 },
  { value: 360 },
];

const isMac = navigator.platform.toLowerCase().includes("mac");
const isWindows = isNativeStreamerSupportedPlatform(`${navigator.platform} ${navigator.userAgent}`);
const shortcutExamples = "Examples: F3, Ctrl+Shift+Q, Ctrl+Shift+K";
const shortcutDefaults = {
  shortcutToggleStats: "F3",
  shortcutTogglePointerLock: "F8",
  shortcutToggleFullscreen: "F10",
  shortcutStopStream: "Ctrl+Shift+Q",
  shortcutToggleAntiAfk: "Ctrl+Shift+K",
  shortcutToggleMicrophone: "Ctrl+Shift+M",
  shortcutScreenshot: "F11",
  shortcutToggleRecording: "F12",
} as const;

/** Canonical shortcut for toggling the stream sidebar (must match StreamView key handler). */
const SIDEBAR_TOGGLE_SHORTCUT_RAW = isMac ? "Meta+G" : "Ctrl+Shift+G";
const NATIVE_STREAMER_ENABLE_PROMPT_EXIT_MS = 160;

type ShortcutSettingKey = keyof typeof shortcutDefaults;

const SHORTCUT_SETTING_KEYS = Object.keys(shortcutDefaults) as ShortcutSettingKey[];

function getShortcutConflictMessage(
  editingKey: ShortcutSettingKey,
  candidateCanonical: string,
  currentSettings: Settings,
): string | null {
  const sidebarParsed = normalizeShortcut(SIDEBAR_TOGGLE_SHORTCUT_RAW);
  if (sidebarParsed.valid && candidateCanonical === sidebarParsed.canonical) {
    return "Shortcut conflicts with the settings sidebar toggle.";
  }
  for (const key of SHORTCUT_SETTING_KEYS) {
    if (key === editingKey) continue;
    const parsed = normalizeShortcut(currentSettings[key]);
    if (parsed.valid && parsed.canonical === candidateCanonical) {
      return "Shortcut conflicts with another binding.";
    }
  }
  return null;
}

const microphoneModeOptions: Array<{ value: MicrophoneMode; label: string }> = [
  { value: "disabled", label: "Disabled" },
  { value: "push-to-talk", label: "Push-to-Talk" },
  { value: "voice-activity", label: "Voice Activity" },
];

function getMicrophonePermissionError(result: MicrophonePermissionResult): string {
  switch (result.status) {
    case "denied":
      return "Microphone access was denied. Enable microphone access for OpenNOW in System Settings → Privacy & Security → Microphone.";
    case "restricted":
      return "Microphone access is restricted by macOS and cannot be enabled from OpenNOW.";
    case "unknown":
      return "Unable to determine microphone permission status. Check macOS microphone privacy settings for OpenNOW.";
    default:
      return "Microphone access is not available.";
  }
}

const gameLanguageOptions: Array<{ value: GameLanguage; label: string }> = [
  { value: "en_US", label: "English (US)" },
  { value: "en_GB", label: "English (UK)" },
  { value: "de_DE", label: "Deutsch" },
  { value: "fr_FR", label: "Français" },
  { value: "es_ES", label: "Español (ES)" },
  { value: "es_MX", label: "Español (MX)" },
  { value: "it_IT", label: "Italiano" },
  { value: "pt_PT", label: "Português (PT)" },
  { value: "pt_BR", label: "Português (BR)" },
  { value: "ru_RU", label: "Русский" },
  { value: "pl_PL", label: "Polski" },
  { value: "tr_TR", label: "Türkçe" },
  { value: "ar_SA", label: "العربية" },
  { value: "ja_JP", label: "日本語" },
  { value: "ko_KR", label: "한국어" },
  { value: "zh_CN", label: "简体中文" },
  { value: "zh_TW", label: "繁體中文" },
  { value: "th_TH", label: "ไทย" },
  { value: "vi_VN", label: "Tiếng Việt" },
  { value: "id_ID", label: "Bahasa Indonesia" },
  { value: "cs_CZ", label: "Čeština" },
  { value: "el_GR", label: "Ελληνικά" },
  { value: "hu_HU", label: "Magyar" },
  { value: "ro_RO", label: "Română" },
  { value: "uk_UA", label: "Українська" },
  { value: "nl_NL", label: "Nederlands" },
  { value: "sv_SE", label: "Svenska" },
  { value: "da_DK", label: "Dansk" },
  { value: "fi_FI", label: "Suomi" },
  { value: "no_NO", label: "Norsk" },
];

/* ── Aspect ratio helpers ─────────────────────────────────────────── */

const ASPECT_RATIO_ORDER = [
  "16:9 Standard",
  "16:10 Widescreen",
  "21:9 Ultrawide",
  "32:9 Super Ultrawide",
  "4:3 Legacy",
  "Other",
] as const;

function classifyAspectRatio(width: number, height: number): string {
  const ratio = width / height;
  if (Math.abs(ratio - 16 / 9) < 0.05) return "16:9 Standard";
  if (Math.abs(ratio - 16 / 10) < 0.05) return "16:10 Widescreen";
  if (Math.abs(ratio - 21 / 9) < 0.05) return "21:9 Ultrawide";
  if (Math.abs(ratio - 32 / 9) < 0.05) return "32:9 Super Ultrawide";
  if (Math.abs(ratio - 4 / 3) < 0.05) return "4:3 Legacy";
  if (ratio > 2 && ratio < 3.5) return "21:9 Ultrawide";
  return "Other";
}

function friendlyResolutionName(width: number, height: number): string {
  if (width === 1280 && height === 720) return "720p (HD)";
  if (width === 1920 && height === 1080) return "1080p (FHD)";
  if (width === 2560 && height === 1440) return "1440p (QHD)";
  if (width === 3840 && height === 2160) return "4K (UHD)";
  if (width === 2560 && height === 1080) return "2560x1080 (UW)";
  if (width === 3440 && height === 1440) return "3440x1440 (UW)";
  if (width === 5120 && height === 1440) return "5120x1440 (SUW)";
  return `${width}x${height}`;
}

interface ResolutionGroup {
  category: string;
  resolutions: { width: number; height: number; value: string; label: string }[];
}

function groupResolutions(entitled: EntitledResolution[]): ResolutionGroup[] {
  // Deduplicate by (width, height)
  const seen = new Set<string>();
  const unique: { width: number; height: number }[] = [];
  // Sort by width desc, height desc
  const sorted = [...entitled].sort((a, b) => b.width - a.width || b.height - a.height);
  for (const res of sorted) {
    const key = `${res.width}x${res.height}`;
    if (seen.has(key)) continue;
    seen.add(key);
    unique.push(res);
  }

  // Group by aspect ratio
  const groupMap = new Map<string, { width: number; height: number; value: string; label: string }[]>();
  for (const res of unique) {
    const cat = classifyAspectRatio(res.width, res.height);
    const value = `${res.width}x${res.height}`;
    const label = friendlyResolutionName(res.width, res.height);
    if (!groupMap.has(cat)) groupMap.set(cat, []);
    groupMap.get(cat)!.push({ width: res.width, height: res.height, value, label });
  }

  // Return in canonical order
  const result: ResolutionGroup[] = [];
  for (const cat of ASPECT_RATIO_ORDER) {
    const items = groupMap.get(cat);
    if (items && items.length > 0) {
      result.push({ category: cat, resolutions: items });
    }
  }
  return result;
}

function getFpsForResolution(entitled: EntitledResolution[], resolution: string): number[] {
  const parts = resolution.split("x");
  const w = parseInt(parts[0], 10);
  const h = parseInt(parts[1], 10);

  let fpsList = entitled
    .filter((r) => r.width === w && r.height === h)
    .map((r) => r.fps);

  // Fallback: if no exact match, collect all FPS from all resolutions
  if (fpsList.length === 0) {
    fpsList = entitled.map((r) => r.fps);
  }

  // Deduplicate and sort ascending
  return [...new Set(fpsList)].sort((a, b) => a - b);
}

const ENTITLED_RESOLUTIONS_STORAGE_KEY = "opennow.entitled-resolutions.v1";

interface EntitledResolutionsCache {
  userId: string;
  entitledResolutions: EntitledResolution[];
}

function loadCachedEntitledResolutions(): EntitledResolutionsCache | null {
  try {
    const raw = window.sessionStorage.getItem(ENTITLED_RESOLUTIONS_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<EntitledResolutionsCache>;
    if (!parsed || typeof parsed.userId !== "string" || !Array.isArray(parsed.entitledResolutions)) {
      return null;
    }
    return {
      userId: parsed.userId,
      entitledResolutions: parsed.entitledResolutions,
    };
  } catch {
    return null;
  }
}

function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let size = value;
  let unitIndex = 0;
  while (size >= 1024 && unitIndex < units.length - 1) {
    size /= 1024;
    unitIndex += 1;
  }
  const digits = size >= 100 || unitIndex === 0 ? 0 : size >= 10 ? 1 : 2;
  return `${size.toFixed(digits)} ${units[unitIndex]}`;
}

function formatUpdaterTimestamp(value?: number): string | null {
  if (!value) return null;
  try {
    return new Date(value).toLocaleString();
  } catch {
    return null;
  }
}

function getUpdaterBadgeLabel(state: AppUpdaterState): string {
  switch (state.status) {
    case "disabled":
      return "Packaged builds only";
    case "idle":
      return "Idle";
    case "checking":
      return "Checking";
    case "available":
      return "Update available";
    case "not-available":
      return "Up to date";
    case "downloading":
      return "Downloading";
    case "downloaded":
      return "Ready to install";
    case "error":
      return "Error";
    default:
      return "Idle";
  }
}

function saveCachedEntitledResolutions(cache: EntitledResolutionsCache): void {
  try {
    window.sessionStorage.setItem(ENTITLED_RESOLUTIONS_STORAGE_KEY, JSON.stringify(cache));
  } catch {
    // Ignore storage failures
  }
}

/* ── Component ────────────────────────────────────────────────────── */

export function SettingsPage({ settings, regions, onSettingChange, codecResults, codecTesting, onRunCodecTest }: SettingsPageProps): JSX.Element {
  const { locale, availableLocales, setLocale, t } = useTranslation();
  const [savedIndicator, setSavedIndicator] = useState(false);
  const [activeSection, setActiveSection] = useState<SettingsSectionId>("stream");
  const [thanksData, setThanksData] = useState<ThankYouDataResult | null>(null);
  const [thanksLoadState, setThanksLoadState] = useState<ThanksLoadState>("idle");
  const [thanksFetchError, setThanksFetchError] = useState<string | null>(null);
  const thanksRequestIdRef = useRef(0);
  const thanksMountedRef = useRef(true);
  const [regionSearch, setRegionSearch] = useState("");
  const [regionDropdownOpen, setRegionDropdownOpen] = useState(false);

  const codecTestOpen = codecResults !== null || codecTesting;

  // Region ping state
  const initialPingResults = useMemo(() => loadStoredRegionPingResults(), []);
  const [pingResults, setPingResults] = useState<Map<string, number | null>>(initialPingResults ?? new Map());
  const [isPinging, setIsPinging] = useState(false);
  const [bestRegionUrl, setBestRegionUrl] = useState<string | null>(() => {
    if (!initialPingResults) return null;
    let bestUrl: string | null = null;
    let bestPing = Infinity;
    initialPingResults.forEach((pingMs, url) => {
      if (pingMs !== null && pingMs < bestPing) {
        bestPing = pingMs;
        bestUrl = url;
      }
    });
    return bestUrl;
  });

  const runPingTest = useCallback(async () => {
    if (regions.length === 0) return;
    setIsPinging(true);
    try {
      const results = await window.openNow.pingRegions(regions);
      const pingMap = new Map<string, number | null>();
      let bestUrl: string | null = null;
      let bestPing = Infinity;

      for (const result of results) {
        pingMap.set(result.url, result.pingMs);
        if (result.pingMs !== null && result.pingMs < bestPing) {
          bestPing = result.pingMs;
          bestUrl = result.url;
        }
      }

      setPingResults(pingMap);
      setBestRegionUrl(bestUrl);
      saveStoredRegionPingResults(pingMap);
    } catch (err) {
      console.error("Ping test failed:", err);
    } finally {
      setIsPinging(false);
    }
  }, [regions]);

  // Validate cached results match current regions
  useEffect(() => {
    if (regions.length > 0 && pingResults.size > 0) {
      // Check if all current regions are in the cache
      const allRegionsCached = regions.every(r => pingResults.has(r.url));
      if (!allRegionsCached) {
        // Regions changed, clear cache and re-test
        setPingResults(new Map());
        setBestRegionUrl(null);
        clearStoredRegionPingResults();
      }
    }
  }, [regions, pingResults]);

  // Run ping test when regions are available and we don't have cached results
  useEffect(() => {
    if (regions.length > 0 && pingResults.size === 0 && !isPinging) {
      runPingTest();
    }
  }, [regions, pingResults.size, isPinging, runPingTest]);

  const [toggleStatsInput, setToggleStatsInput] = useState(settings.shortcutToggleStats);
  const [togglePointerLockInput, setTogglePointerLockInput] = useState(settings.shortcutTogglePointerLock);
  const [toggleFullscreenInput, setToggleFullscreenInput] = useState(settings.shortcutToggleFullscreen);
  const [stopStreamInput, setStopStreamInput] = useState(settings.shortcutStopStream);
  const [toggleAntiAfkInput, setToggleAntiAfkInput] = useState(settings.shortcutToggleAntiAfk);
  const [toggleMicrophoneInput, setToggleMicrophoneInput] = useState(settings.shortcutToggleMicrophone);
  const [screenshotInput, setScreenshotInput] = useState(settings.shortcutScreenshot);
  const [recordingInput, setRecordingInput] = useState(settings.shortcutToggleRecording);
  const [toggleStatsError, setToggleStatsError] = useState<string | null>(null);
  const [togglePointerLockError, setTogglePointerLockError] = useState<string | null>(null);
  const [toggleFullscreenError, setToggleFullscreenError] = useState<string | null>(null);
  const [stopStreamError, setStopStreamError] = useState<string | null>(null);
  const [toggleAntiAfkError, setToggleAntiAfkError] = useState<string | null>(null);
  const [toggleMicrophoneError, setToggleMicrophoneError] = useState<string | null>(null);
  const [screenshotError, setScreenshotError] = useState<string | null>(null);
  const [recordingError, setRecordingError] = useState<string | null>(null);

  const [keyboardLayoutDropdownOpen, setKeyboardLayoutDropdownOpen] = useState(false);
  const keyboardLayoutDropdownRef = useRef<HTMLDivElement | null>(null);

  const [appLanguageDropdownOpen, setAppLanguageDropdownOpen] = useState(false);
  const appLanguageDropdownRef = useRef<HTMLDivElement | null>(null);
  const [accentColorDropdownOpen, setAccentColorDropdownOpen] = useState(false);
  const accentColorDropdownRef = useRef<HTMLDivElement | null>(null);

  // Game language dropdown state
  const [gameLanguageDropdownOpen, setGameLanguageDropdownOpen] = useState(false);
  const gameLanguageDropdownRef = useRef<HTMLDivElement | null>(null);

  const [resolutionDropdownOpen, setResolutionDropdownOpen] = useState(false);
  const resolutionDropdownRef = useRef<HTMLDivElement | null>(null);
  const [settingsSearch, setSettingsSearch] = useState("");
  const [codecAdvancedOpen, setCodecAdvancedOpen] = useState(false);
  const [nativeStreamerStatus, setNativeStreamerStatus] = useState<NativeStreamerStatus | null>(null);
  const [nativeStreamerStatusLoading, setNativeStreamerStatusLoading] = useState(false);
  const [nativeStreamerEnablePromptOpen, setNativeStreamerEnablePromptOpen] = useState(false);
  const [nativeStreamerEnablePromptClosing, setNativeStreamerEnablePromptClosing] = useState(false);
  const nativeStreamerEnablePromptRef = useRef<HTMLDivElement | null>(null);
  const nativeStreamerEnablePromptConfirmRef = useRef<HTMLButtonElement | null>(null);
  const nativeStreamerEnablePromptPreviousFocusRef = useRef<HTMLElement | null>(null);
  const nativeStreamerEnablePromptCloseTimerRef = useRef<number | null>(null);
  const nativeStreamerEnablePromptVisible =
    nativeStreamerEnablePromptOpen || nativeStreamerEnablePromptClosing;
  const [updaterState, setUpdaterState] = useState<AppUpdaterState>({
    status: "idle",
    currentVersion: "0.0.0",
    currentDisplayVersion: "0.0.0",
    updateSource: "github-releases",
    canCheck: false,
    canDownload: false,
    canInstall: false,
    isPackaged: false,
  });

  // Dynamic entitled resolutions from MES API
  const [entitledResolutions, setEntitledResolutions] = useState<EntitledResolution[]>([]);
  const [subscriptionLoading, setSubscriptionLoading] = useState(true);

  useEffect(() => {
    setToggleStatsInput(settings.shortcutToggleStats);
  }, [settings.shortcutToggleStats]);

  useEffect(() => {
    setTogglePointerLockInput(settings.shortcutTogglePointerLock);
  }, [settings.shortcutTogglePointerLock]);

  useEffect(() => {
    setToggleFullscreenInput(settings.shortcutToggleFullscreen);
  }, [settings.shortcutToggleFullscreen]);

  useEffect(() => {
    setStopStreamInput(settings.shortcutStopStream);
  }, [settings.shortcutStopStream]);

  useEffect(() => {
    setToggleAntiAfkInput(settings.shortcutToggleAntiAfk);
  }, [settings.shortcutToggleAntiAfk]);

  useEffect(() => {
    setToggleMicrophoneInput(settings.shortcutToggleMicrophone);
  }, [settings.shortcutToggleMicrophone]);

  useEffect(() => {
    setScreenshotInput(settings.shortcutScreenshot);
  }, [settings.shortcutScreenshot]);

  useEffect(() => {
    setRecordingInput(settings.shortcutToggleRecording);
  }, [settings.shortcutToggleRecording]);

  useEffect(() => {
    let cancelled = false;

    void window.openNow.getUpdaterState().then((state) => {
      if (!cancelled) {
        setUpdaterState(state);
      }
    }).catch((error) => {
      console.warn("[Settings] Failed to load updater state:", error);
    });

    const unsubscribe = window.openNow.onUpdaterStateChanged((state) => {
      if (!cancelled) {
        setUpdaterState(state);
      }
    });

    return () => {
      cancelled = true;
      unsubscribe();
    };
  }, []);

  const refreshNativeStreamerStatus = useCallback(async () => {
    if (!isWindows) {
      setNativeStreamerStatus(createUnsupportedNativeStreamerStatus());
      setNativeStreamerStatusLoading(false);
      return;
    }

    setNativeStreamerStatusLoading(true);
    try {
      setNativeStreamerStatus(await window.openNow.getNativeStreamerStatus());
    } catch (error) {
      console.warn("[Settings] Failed to detect native streamer:", error);
      setNativeStreamerStatus({
        detected: false,
        gstreamerAvailable: false,
        supportsOfferAnswer: false,
        gstreamerRuntime: {
          source: "unknown",
          bundled: false,
          message: "GStreamer runtime could not be checked.",
        },
        message: "Native streamer status could not be checked.",
      });
    } finally {
      setNativeStreamerStatusLoading(false);
    }
  }, []);

  useEffect(() => {
    if (activeSection === "native-streamer" || settingsSearch.length > 0) {
      void refreshNativeStreamerStatus();
    }
  }, [activeSection, refreshNativeStreamerStatus, settingsSearch.length]);

  // Fetch subscription data (cached per account; reload only when account changes)
  useEffect(() => {
    let cancelled = false;

    async function load(): Promise<void> {
      try {
        const sessionResult = await window.openNow.getAuthSession();
        const session = sessionResult.session;
        if (!session || cancelled) {
          setEntitledResolutions([]);
          setSubscriptionLoading(false);
          return;
        }

        const userId = session.user.userId;
        const cached = loadCachedEntitledResolutions();
        if (cached && cached.userId === userId) {
          setEntitledResolutions(cached.entitledResolutions);
          setSubscriptionLoading(false);
          return;
        }

        const sub = await window.openNow.fetchSubscription({
          userId,
        });

        if (!cancelled) {
          setEntitledResolutions(sub.entitledResolutions);
          saveCachedEntitledResolutions({
            userId,
            entitledResolutions: sub.entitledResolutions,
          });
        }
      } catch (err) {
        console.warn("Failed to fetch subscription for settings:", err);
      } finally {
        if (!cancelled) setSubscriptionLoading(false);
      }
    }

    load();
    return () => { cancelled = true; };
  }, []);

  const hasDynamic = entitledResolutions.length > 0;

  // Grouped resolution presets (dynamic)
  const resolutionGroups = useMemo(
    () => (hasDynamic ? groupResolutions(entitledResolutions) : []),
    [entitledResolutions, hasDynamic]
  );

  // Dynamic FPS presets based on current resolution
  const dynamicFpsOptions = useMemo(
    () => (hasDynamic ? getFpsForResolution(entitledResolutions, settings.resolution) : []),
    [entitledResolutions, settings.resolution, hasDynamic]
  );
  const posterSizePercent = Math.round(settings.posterSizeScale * 100);
  const updaterLastCheckedLabel = useMemo(() => formatUpdaterTimestamp(updaterState.lastCheckedAt), [updaterState.lastCheckedAt]);
  const updaterProgressPercent = updaterState.progress ? Math.max(0, Math.min(100, Math.round(updaterState.progress.percent))) : 0;
  const updaterProgressLabel = updaterState.progress
    ? `${formatBytes(updaterState.progress.transferred)} / ${formatBytes(updaterState.progress.total || updaterState.progress.transferred)}`
    : null;
  const updaterDownloadRateLabel = updaterState.progress?.bytesPerSecond
    ? `${formatBytes(updaterState.progress.bytesPerSecond)}/s`
    : null;
  const updaterBadgeLabel = useMemo(() => getUpdaterBadgeLabel(updaterState), [updaterState]);

  const selectedResolutionLabel = useMemo(() => {
    if (hasDynamic) {
      for (const group of resolutionGroups) {
        const found = group.resolutions.find(r => r.value === settings.resolution);
        if (found) return found.label;
      }
      return settings.resolution || "Select";
    }
    const found = STATIC_RESOLUTION_PRESETS.find(r => r.value === settings.resolution);
    return found ? found.label : settings.resolution || "Select";
  }, [settings.resolution, hasDynamic, resolutionGroups]);

  const handleChange = useCallback(
    <K extends keyof Settings>(key: K, value: Settings[K]) => {
      onSettingChange(key, value);
      setSavedIndicator(true);
      setTimeout(() => setSavedIndicator(false), 1500);
    },
    [onSettingChange]
  );

  const openNativeStreamerEnablePrompt = useCallback((): void => {
    if (nativeStreamerEnablePromptCloseTimerRef.current !== null) {
      window.clearTimeout(nativeStreamerEnablePromptCloseTimerRef.current);
      nativeStreamerEnablePromptCloseTimerRef.current = null;
    }

    setNativeStreamerEnablePromptClosing(false);
    setNativeStreamerEnablePromptOpen(true);
  }, []);

  const closeNativeStreamerEnablePrompt = useCallback((): void => {
    if (nativeStreamerEnablePromptCloseTimerRef.current !== null) {
      return;
    }

    setNativeStreamerEnablePromptOpen(false);
    setNativeStreamerEnablePromptClosing(true);
    nativeStreamerEnablePromptCloseTimerRef.current = window.setTimeout(() => {
      nativeStreamerEnablePromptCloseTimerRef.current = null;
      setNativeStreamerEnablePromptClosing(false);
    }, NATIVE_STREAMER_ENABLE_PROMPT_EXIT_MS);
  }, []);

  const confirmNativeStreamerEnablePrompt = useCallback((): void => {
    handleChange("streamClientMode", "native");
    closeNativeStreamerEnablePrompt();
  }, [closeNativeStreamerEnablePrompt, handleChange]);

  const handleNativeStreamerToggleChange = useCallback((checked: boolean): void => {
    if (!checked) {
      handleChange("streamClientMode", "web");
      return;
    }

    if (settings.streamClientMode === "native") {
      return;
    }

    openNativeStreamerEnablePrompt();
  }, [handleChange, openNativeStreamerEnablePrompt, settings.streamClientMode]);

  useEffect(() => {
    return () => {
      if (nativeStreamerEnablePromptCloseTimerRef.current !== null) {
        window.clearTimeout(nativeStreamerEnablePromptCloseTimerRef.current);
        nativeStreamerEnablePromptCloseTimerRef.current = null;
      }
    };
  }, []);

  useEffect(() => {
    if (!nativeStreamerEnablePromptVisible) {
      return;
    }

    nativeStreamerEnablePromptPreviousFocusRef.current =
      document.activeElement instanceof HTMLElement ? document.activeElement : null;

    const getFocusableElements = (): HTMLElement[] => {
      const dialog = nativeStreamerEnablePromptRef.current;
      if (!dialog) {
        return [];
      }

      return Array.from(
        dialog.querySelectorAll<HTMLElement>(
          [
            "a[href]",
            "button:not([disabled])",
            "input:not([disabled])",
            "select:not([disabled])",
            "textarea:not([disabled])",
            '[tabindex]:not([tabindex="-1"])',
          ].join(","),
        ),
      ).filter((element) => element.tabIndex >= 0 && element.getAttribute("aria-hidden") !== "true");
    };

    const handleKeyDown = (event: KeyboardEvent): void => {
      if (event.key === "Escape") {
        event.preventDefault();
        closeNativeStreamerEnablePrompt();
        return;
      }

      if (event.key !== "Tab") {
        return;
      }

      const dialog = nativeStreamerEnablePromptRef.current;
      const focusableElements = getFocusableElements();
      if (!dialog || focusableElements.length === 0) {
        event.preventDefault();
        dialog?.focus({ preventScroll: true });
        return;
      }

      const activeElement = document.activeElement;
      const firstElement = focusableElements[0];
      const lastElement = focusableElements[focusableElements.length - 1];
      const focusIsOnDialog = activeElement === dialog;

      if (event.shiftKey && (focusIsOnDialog || activeElement === firstElement || !dialog.contains(activeElement))) {
        event.preventDefault();
        lastElement.focus({ preventScroll: true });
        return;
      }

      if (!event.shiftKey && (focusIsOnDialog || activeElement === lastElement || !dialog.contains(activeElement))) {
        event.preventDefault();
        firstElement.focus({ preventScroll: true });
      }
    };

    const focusFrame = window.requestAnimationFrame(() => {
      nativeStreamerEnablePromptConfirmRef.current?.focus({ preventScroll: true });
    });

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      window.cancelAnimationFrame(focusFrame);
      document.removeEventListener("keydown", handleKeyDown);

      const previousFocus = nativeStreamerEnablePromptPreviousFocusRef.current;
      nativeStreamerEnablePromptPreviousFocusRef.current = null;
      if (previousFocus?.isConnected) {
        previousFocus.focus({ preventScroll: true });
      }
    };
  }, [closeNativeStreamerEnablePrompt, nativeStreamerEnablePromptVisible]);

  const handleAppLanguageChange = useCallback((nextLocale: string): void => {
    setAppLanguageDropdownOpen(false);
    void setLocale(nextLocale).catch((error) => {
      console.warn("[Settings] Failed to change app language:", error);
    });
    setSavedIndicator(true);
    setTimeout(() => setSavedIndicator(false), 1500);
  }, [setLocale]);

  const setNativeFramePacing = useCallback((mode: "low-latency" | "smooth") => {
    if (mode === "low-latency") {
      handleChange("enableCloudGsync", false);
      handleChange("nativeCloudGsyncMode", "disabled");
      handleChange("nativeD3dFullscreenMode", "disabled");
      return;
    }

    handleChange("enableCloudGsync", true);
    handleChange("nativeCloudGsyncMode", "auto");
    handleChange("nativeD3dFullscreenMode", "auto");
  }, [handleChange]);

  const handleColorQualityChange = useCallback(
    (cq: ColorQuality) => {
      handleChange("colorQuality", cq);
      if (colorQualityRequiresHevc(cq) && settings.codec === "H264") {
        handleChange("codec", "H265");
      }
    },
    [handleChange, settings.codec]
  );

  const handleCodecChange = useCallback(
    (codec: VideoCodec) => {
      handleChange("codec", codec);
      if (codec === "H264" && settings.colorQuality !== "8bit_420") {
        handleChange("colorQuality", "8bit_420");
      }
    },
    [handleChange, settings.colorQuality]
  );

  // Microphone devices
  const [microphoneDevices, setMicrophoneDevices] = useState<MediaDeviceInfo[]>([]);
  const [microphonePermissionError, setMicrophonePermissionError] = useState<string | null>(null);
  const [microphoneModeDropdownOpen, setMicrophoneModeDropdownOpen] = useState(false);
  const [microphoneDeviceDropdownOpen, setMicrophoneDeviceDropdownOpen] = useState(false);
  const microphoneModeDropdownRef = useRef<HTMLDivElement | null>(null);
  const microphoneDeviceDropdownRef = useRef<HTMLDivElement | null>(null);
  const latestMicrophoneDeviceIdRef = useRef(settings.microphoneDeviceId);

  useEffect(() => {
    latestMicrophoneDeviceIdRef.current = settings.microphoneDeviceId;
  }, [settings.microphoneDeviceId]);

  // Enumerate microphone devices when mic mode is enabled
  useEffect(() => {
    if (settings.microphoneMode === "disabled") {
      setMicrophoneDevices([]);
      setMicrophonePermissionError(null);
      return;
    }

    let cancelled = false;

    async function enumerateDevices(): Promise<void> {
      const applyDeviceList = (audioInputs: MediaDeviceInfo[]): void => {
        if (cancelled) {
          return;
        }

        setMicrophoneDevices(audioInputs);
        setMicrophonePermissionError(null);

        if (
          latestMicrophoneDeviceIdRef.current
          && !audioInputs.some((device) => device.deviceId === latestMicrophoneDeviceIdRef.current)
        ) {
          handleChange("microphoneDeviceId", "");
        }
      };

      try {
        if (typeof window.openNow?.getMicrophonePermission === "function") {
          const permission = await window.openNow.getMicrophonePermission();
          if (cancelled) {
            return;
          }

          if (permission.isMacOs && !permission.granted) {
            setMicrophoneDevices([]);
            setMicrophonePermissionError(getMicrophonePermissionError(permission));
            return;
          }
        }

        const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
        stream.getTracks().forEach((track) => track.stop());

        const devices = await navigator.mediaDevices.enumerateDevices();
        applyDeviceList(devices.filter((device) => device.kind === "audioinput"));
      } catch (err) {
        console.error("[SettingsPage] Failed to enumerate microphone devices:", err);

        try {
          const devices = await navigator.mediaDevices.enumerateDevices();
          if (cancelled) {
            return;
          }

          const audioInputs = devices.filter((device) => device.kind === "audioinput");
          if (audioInputs.length > 0) {
            setMicrophoneDevices(audioInputs);
            setMicrophonePermissionError("Microphone access is required to show device names and use voice chat. Allow access and try again.");
            if (
              latestMicrophoneDeviceIdRef.current
              && !audioInputs.some((device) => device.deviceId === latestMicrophoneDeviceIdRef.current)
            ) {
              handleChange("microphoneDeviceId", "");
            }
            return;
          }
        } catch {
          // Ignore secondary enumerate failure and fall through to stable error state.
        }

        if (!cancelled) {
          const message = err instanceof DOMException && err.name === "NotAllowedError"
            ? "Microphone access was denied. Allow access for OpenNOW and try again."
            : "Unable to access microphone devices right now.";
          setMicrophonePermissionError(message);
          setMicrophoneDevices([]);
        }
      }
    }

    void enumerateDevices();
    return () => { cancelled = true; };
  }, [handleChange, settings.microphoneMode]);

  const filteredRegions = useMemo(() => {
    const q = regionSearch.trim().toLowerCase();
    const filtered = q
      ? regions.filter((r) => r.name.toLowerCase().includes(q))
      : [...regions];

    // Sort by ping (best first), then by name
    filtered.sort((a, b) => {
      const pingA = pingResults.get(a.url);
      const pingB = pingResults.get(b.url);

      // If both have ping results, sort by ping
      if (pingA !== undefined && pingB !== undefined && pingA !== null && pingB !== null) {
        return pingA - pingB;
      }
      // If only A has ping, A comes first
      if (pingA !== undefined && pingA !== null) return -1;
      // If only B has ping, B comes first
      if (pingB !== undefined && pingB !== null) return 1;
      // If neither has ping, sort by name
      return a.name.localeCompare(b.name);
    });

    return filtered;
  }, [regions, regionSearch, pingResults]);

  const selectedRegionName = useMemo(() => {
    if (!settings.region) return t("settings.region.autoBest");
    const found = regions.find((r) => r.url === settings.region);
    return found?.name ?? settings.region;
  }, [settings.region, regions, locale, t]);

  const appLanguageOptions = useMemo(
    () => availableLocales.map((value) => ({ value, label: getAppLanguageLabel(value) })),
    [availableLocales],
  );

  const selectedAppLanguageName = useMemo(() => {
    return appLanguageOptions.find((option) => option.value === locale)?.label ?? getAppLanguageLabel(locale);
  }, [appLanguageOptions, locale]);

  const selectedAccentColor = useMemo(() => getAccentColorOption(settings.appAccentColor), [settings.appAccentColor]);
  const getMicrophoneModeLabel = useCallback((mode: MicrophoneMode): string => {
    switch (mode) {
      case "push-to-talk":
        return t("settings.audio.pushToTalk");
      case "voice-activity":
        return t("settings.audio.voiceActivity");
      case "disabled":
      default:
        return t("settings.audio.disabled");
    }
  }, [locale, t]);

  const selectedMicrophoneModeName = useMemo(() => {
    return getMicrophoneModeLabel(settings.microphoneMode);
  }, [settings.microphoneMode, getMicrophoneModeLabel]);

  const selectedMicrophoneDeviceName = useMemo(() => {
    if (!settings.microphoneDeviceId) return t("app.labels.defaultDevice");
    const found = microphoneDevices.find((device) => device.deviceId === settings.microphoneDeviceId);
    return found?.label || t("settings.audio.selectedDevice");
  }, [settings.microphoneDeviceId, microphoneDevices, locale, t]);

  const selectedGameLanguageName = useMemo(() => {
    return gameLanguageOptions.find((option) => option.value === settings.gameLanguage)?.label ?? "English (US)";
  }, [settings.gameLanguage]);

  const selectedKeyboardLayoutName = useMemo(() => {
    return keyboardLayoutOptions.find((option) => option.value === settings.keyboardLayout)?.label ?? "English (US)";
  }, [settings.keyboardLayout]);

  useEffect(() => {
    if (settings.microphoneMode === "disabled") {
      setMicrophoneDeviceDropdownOpen(false);
    }
  }, [settings.microphoneMode]);

  useEffect(() => {
    const handlePointerDown = (event: MouseEvent): void => {
      const target = event.target as Node;
      if (microphoneModeDropdownRef.current && !microphoneModeDropdownRef.current.contains(target)) {
        setMicrophoneModeDropdownOpen(false);
      }
      if (microphoneDeviceDropdownRef.current && !microphoneDeviceDropdownRef.current.contains(target)) {
        setMicrophoneDeviceDropdownOpen(false);
      }
      if (keyboardLayoutDropdownRef.current && !keyboardLayoutDropdownRef.current.contains(target)) {
        setKeyboardLayoutDropdownOpen(false);
      }
      if (appLanguageDropdownRef.current && !appLanguageDropdownRef.current.contains(target)) {
        setAppLanguageDropdownOpen(false);
      }
      if (accentColorDropdownRef.current && !accentColorDropdownRef.current.contains(target)) {
        setAccentColorDropdownOpen(false);
      }
      if (gameLanguageDropdownRef.current && !gameLanguageDropdownRef.current.contains(target)) {
        setGameLanguageDropdownOpen(false);
      }
      if (resolutionDropdownRef.current && !resolutionDropdownRef.current.contains(target)) {
        setResolutionDropdownOpen(false);
      }
    };

    document.addEventListener("mousedown", handlePointerDown);
    return () => document.removeEventListener("mousedown", handlePointerDown);
  }, []);

  const handleShortcutBlur = (key: ShortcutSettingKey, rawValue: string): void => {
    const trimmed = rawValue.trim();
    if (!trimmed) {
      const msg = "Shortcut cannot be empty.";
      switch (key) {
        case "shortcutToggleStats": setToggleStatsError(msg); break;
        case "shortcutTogglePointerLock": setTogglePointerLockError(msg); break;
        case "shortcutToggleFullscreen": setToggleFullscreenError(msg); break;
        case "shortcutStopStream": setStopStreamError(msg); break;
        case "shortcutToggleAntiAfk": setToggleAntiAfkError(msg); break;
        case "shortcutToggleMicrophone": setToggleMicrophoneError(msg); break;
        case "shortcutScreenshot": setScreenshotError(msg); break;
        case "shortcutToggleRecording": setRecordingError(msg); break;
      }
      return;
    }

    const normalized = normalizeShortcut(trimmed);
    if (!normalized.valid) {
      const msg = "Invalid shortcut format.";
      switch (key) {
        case "shortcutToggleStats": setToggleStatsError(msg); break;
        case "shortcutTogglePointerLock": setTogglePointerLockError(msg); break;
        case "shortcutToggleFullscreen": setToggleFullscreenError(msg); break;
        case "shortcutStopStream": setStopStreamError(msg); break;
        case "shortcutToggleAntiAfk": setToggleAntiAfkError(msg); break;
        case "shortcutToggleMicrophone": setToggleMicrophoneError(msg); break;
        case "shortcutScreenshot": setScreenshotError(msg); break;
        case "shortcutToggleRecording": setRecordingError(msg); break;
      }
      return;
    }

    const conflict = getShortcutConflictMessage(key, normalized.canonical, settings);
    if (conflict) {
      switch (key) {
        case "shortcutToggleStats": setToggleStatsError(conflict); break;
        case "shortcutTogglePointerLock": setTogglePointerLockError(conflict); break;
        case "shortcutToggleFullscreen": setToggleFullscreenError(conflict); break;
        case "shortcutStopStream": setStopStreamError(conflict); break;
        case "shortcutToggleAntiAfk": setToggleAntiAfkError(conflict); break;
        case "shortcutToggleMicrophone": setToggleMicrophoneError(conflict); break;
        case "shortcutScreenshot": setScreenshotError(conflict); break;
        case "shortcutToggleRecording": setRecordingError(conflict); break;
      }
      return;
    }

    switch (key) {
      case "shortcutToggleStats": setToggleStatsError(null); break;
      case "shortcutTogglePointerLock": setTogglePointerLockError(null); break;
      case "shortcutToggleFullscreen": setToggleFullscreenError(null); break;
      case "shortcutStopStream": setStopStreamError(null); break;
      case "shortcutToggleAntiAfk": setToggleAntiAfkError(null); break;
      case "shortcutToggleMicrophone": setToggleMicrophoneError(null); break;
      case "shortcutScreenshot": setScreenshotError(null); break;
      case "shortcutToggleRecording": setRecordingError(null); break;
    }

    switch (key) {
      case "shortcutToggleStats": setToggleStatsInput(normalized.canonical); break;
      case "shortcutTogglePointerLock": setTogglePointerLockInput(normalized.canonical); break;
      case "shortcutToggleFullscreen": setToggleFullscreenInput(normalized.canonical); break;
      case "shortcutStopStream": setStopStreamInput(normalized.canonical); break;
      case "shortcutToggleAntiAfk": setToggleAntiAfkInput(normalized.canonical); break;
      case "shortcutToggleMicrophone": setToggleMicrophoneInput(normalized.canonical); break;
      case "shortcutScreenshot": setScreenshotInput(normalized.canonical); break;
      case "shortcutToggleRecording": setRecordingInput(normalized.canonical); break;
    }

    if (settings[key] !== normalized.canonical) {
      handleChange(key, normalized.canonical);
    }
  };

  const applyShortcutCapture = (key: ShortcutSettingKey, canonical: string): void => {
    const conflict = getShortcutConflictMessage(key, canonical, settings);
    if (conflict) {
      switch (key) {
        case "shortcutToggleStats": setToggleStatsError(conflict); break;
        case "shortcutTogglePointerLock": setTogglePointerLockError(conflict); break;
        case "shortcutToggleFullscreen": setToggleFullscreenError(conflict); break;
        case "shortcutStopStream": setStopStreamError(conflict); break;
        case "shortcutToggleAntiAfk": setToggleAntiAfkError(conflict); break;
        case "shortcutToggleMicrophone": setToggleMicrophoneError(conflict); break;
        case "shortcutScreenshot": setScreenshotError(conflict); break;
        case "shortcutToggleRecording": setRecordingError(conflict); break;
      }
      return;
    }

    switch (key) {
      case "shortcutToggleStats": setToggleStatsError(null); break;
      case "shortcutTogglePointerLock": setTogglePointerLockError(null); break;
      case "shortcutToggleFullscreen": setToggleFullscreenError(null); break;
      case "shortcutStopStream": setStopStreamError(null); break;
      case "shortcutToggleAntiAfk": setToggleAntiAfkError(null); break;
      case "shortcutToggleMicrophone": setToggleMicrophoneError(null); break;
      case "shortcutScreenshot": setScreenshotError(null); break;
      case "shortcutToggleRecording": setRecordingError(null); break;
    }

    switch (key) {
      case "shortcutToggleStats": setToggleStatsInput(canonical); break;
      case "shortcutTogglePointerLock": setTogglePointerLockInput(canonical); break;
      case "shortcutToggleFullscreen": setToggleFullscreenInput(canonical); break;
      case "shortcutStopStream": setStopStreamInput(canonical); break;
      case "shortcutToggleAntiAfk": setToggleAntiAfkInput(canonical); break;
      case "shortcutToggleMicrophone": setToggleMicrophoneInput(canonical); break;
      case "shortcutScreenshot": setScreenshotInput(canonical); break;
      case "shortcutToggleRecording": setRecordingInput(canonical); break;
    }

    if (settings[key] !== canonical) {
      handleChange(key, canonical);
    }
  };

  const handleShortcutCaptureKeyDown = (key: ShortcutSettingKey, e: React.KeyboardEvent<HTMLInputElement>): void => {
    if (e.key === "Escape") {
      e.preventDefault();
      e.currentTarget.blur();
      return;
    }
    if (e.key === "Enter" || e.key === "Tab") {
      return;
    }

    const captured = shortcutFromKeyboardEvent(e.nativeEvent);
    if (!captured) {
      return;
    }

    e.preventDefault();
    e.stopPropagation();
    applyShortcutCapture(key, captured);
  };

  const handleShortcutPaste = (key: ShortcutSettingKey, e: React.ClipboardEvent<HTMLInputElement>): void => {
    const text = e.clipboardData.getData("text/plain").trim();
    if (!text) {
      return;
    }
    e.preventDefault();
    const normalized = normalizeShortcut(text);
    if (!normalized.valid) {
      const msg = "Invalid shortcut format.";
      switch (key) {
        case "shortcutToggleStats": setToggleStatsError(msg); break;
        case "shortcutTogglePointerLock": setTogglePointerLockError(msg); break;
        case "shortcutToggleFullscreen": setToggleFullscreenError(msg); break;
        case "shortcutStopStream": setStopStreamError(msg); break;
        case "shortcutToggleAntiAfk": setToggleAntiAfkError(msg); break;
        case "shortcutToggleMicrophone": setToggleMicrophoneError(msg); break;
        case "shortcutScreenshot": setScreenshotError(msg); break;
        case "shortcutToggleRecording": setRecordingError(msg); break;
      }
      return;
    }
    applyShortcutCapture(key, normalized.canonical);
  };

  const areShortcutsDefault = useMemo(
    () =>
      settings.shortcutToggleStats === shortcutDefaults.shortcutToggleStats
      && settings.shortcutTogglePointerLock === shortcutDefaults.shortcutTogglePointerLock
      && settings.shortcutToggleFullscreen === shortcutDefaults.shortcutToggleFullscreen
      && settings.shortcutStopStream === shortcutDefaults.shortcutStopStream
      && settings.shortcutToggleAntiAfk === shortcutDefaults.shortcutToggleAntiAfk
      && settings.shortcutToggleMicrophone === shortcutDefaults.shortcutToggleMicrophone
      && settings.shortcutScreenshot === shortcutDefaults.shortcutScreenshot
      && settings.shortcutToggleRecording === shortcutDefaults.shortcutToggleRecording,
    [
      settings.shortcutToggleStats,
      settings.shortcutTogglePointerLock,
      settings.shortcutToggleFullscreen,
      settings.shortcutStopStream,
      settings.shortcutToggleAntiAfk,
      settings.shortcutToggleMicrophone,
      settings.shortcutScreenshot,
      settings.shortcutToggleRecording,
    ]
  );

  const handleResetShortcuts = useCallback(() => {
    setToggleStatsInput(shortcutDefaults.shortcutToggleStats);
    setTogglePointerLockInput(shortcutDefaults.shortcutTogglePointerLock);
    setToggleFullscreenInput(shortcutDefaults.shortcutToggleFullscreen);
    setStopStreamInput(shortcutDefaults.shortcutStopStream);
    setToggleAntiAfkInput(shortcutDefaults.shortcutToggleAntiAfk);
    setToggleMicrophoneInput(shortcutDefaults.shortcutToggleMicrophone);
    setScreenshotInput(shortcutDefaults.shortcutScreenshot);
    setRecordingInput(shortcutDefaults.shortcutToggleRecording);
    setToggleStatsError(null);
    setTogglePointerLockError(null);
    setToggleFullscreenError(null);
    setStopStreamError(null);
    setToggleAntiAfkError(null);
    setToggleMicrophoneError(null);
    setScreenshotError(null);
    setRecordingError(null);

    for (const key of SHORTCUT_SETTING_KEYS) {
      const value = shortcutDefaults[key];
      if (settings[key] !== value) {
        handleChange(key, value);
      }
    }
  }, [handleChange, settings]);

  useEffect(() => {
    thanksMountedRef.current = true;
    return () => {
      thanksMountedRef.current = false;
      thanksRequestIdRef.current += 1;
    };
  }, []);

  useEffect(() => {
    const normalizedSearch = settingsSearch.trim().toLowerCase();
    const showAll = normalizedSearch.length > 0;
    const shouldShowThanks = activeSection === "thanks" || (showAll && (() => {
      const terms = SETTINGS_SCOPE_SEARCH_TERMS["thanks"];
      const searchTokens = normalizedSearch.split(/[^a-z0-9]+/).filter((token) => token.length > 0);
      if (searchTokens.length === 0) {
        return true;
      }
      const searchableWords = Array.from(
        new Set(
          terms
            .join(" ")
            .toLowerCase()
            .split(/[^a-z0-9]+/)
            .filter((word) => word.length > 0),
        ),
      );
      const tokenMatchesWord = (token: string, word: string): boolean => token === word || word.startsWith(token);
      return searchTokens.every((token) => searchableWords.some((word) => tokenMatchesWord(token, word)));
    })());

    if (!shouldShowThanks) {
      thanksRequestIdRef.current += 1;
      setThanksLoadState((current) => (current === "loading" || current === "error" ? "idle" : current));
      setThanksFetchError(null);
      return;
    }

    if (thanksData || thanksLoadState !== "idle") {
      return;
    }

    const requestId = ++thanksRequestIdRef.current;
    let requestPromise: Promise<ThankYouDataResult>;

    try {
      const getThanksData = window.openNow?.getThanksData;
      if (typeof getThanksData !== "function") {
        throw new Error("openNow.getThanksData is unavailable");
      }
      requestPromise = getThanksData();
    } catch (error) {
      console.error("[SettingsPage] Failed to start thanks data request:", error);
      setThanksData(null);
      setThanksFetchError("Unable to load community acknowledgements right now.");
      setThanksLoadState("error");
      return;
    }

    setThanksLoadState("loading");
    setThanksFetchError(null);

    void requestPromise.then(
      (data) => {
        if (!thanksMountedRef.current || requestId !== thanksRequestIdRef.current) {
          return;
        }
        setThanksData(data);
        setThanksLoadState("loaded");
      },
      (error) => {
        if (!thanksMountedRef.current || requestId !== thanksRequestIdRef.current) {
          return;
        }
        setThanksData(null);
        setThanksFetchError("Unable to load community acknowledgements right now.");
        setThanksLoadState("error");
      },
    );
  }, [activeSection, thanksData, thanksLoadState, settingsSearch]);

  const renderPersonLink = useCallback((person: ThankYouContributor | ThankYouSupporter, content: JSX.Element) => {
    if (!person.profileUrl) {
      return <div className="settings-person-card">{content}</div>;
    }

    return (
      <a className="settings-person-card settings-person-card--link" href={person.profileUrl} target="_blank" rel="noreferrer">
        {content}
      </a>
    );
  }, []);

  const thanksContributors = thanksData?.contributors ?? [];
  const thanksSupporters = thanksData?.supporters ?? [];
  const hasThanksError = Boolean(thanksFetchError || thanksData?.contributorsError || thanksData?.supportersError);

  const handleRetryThanks = useCallback(() => {
    thanksRequestIdRef.current += 1;
    setThanksData(null);
    setThanksFetchError(null);
    setThanksLoadState("idle");
  }, []);

  const renderContributorCard = useCallback((contributor: ThankYouContributor) => {
    return renderPersonLink(
      contributor,
      <>
        <img className="settings-person-avatar" src={contributor.avatarUrl} alt={contributor.login} loading="lazy" />
        <div className="settings-person-body">
          <div className="settings-person-title-row">
            <span className="settings-person-name">{contributor.login}</span>
            <span className="settings-person-badge">{t("settings.thanks.contributor")}</span>
          </div>
          <div className="settings-person-meta">
            <span>{contributor.contributions} contribution{contributor.contributions === 1 ? "" : "s"}</span>
            <ExternalLink size={14} />
          </div>
        </div>
      </>,
    );
  }, [renderPersonLink]);

  const renderSupporterCard = useCallback((supporter: ThankYouSupporter) => {
    return renderPersonLink(
      supporter,
      <>
        <div className={`settings-person-avatar settings-person-avatar--fallback ${supporter.avatarUrl ? "" : "is-placeholder"}`.trim()}>
          {supporter.avatarUrl ? (
            <img className="settings-person-avatar" src={supporter.avatarUrl} alt={supporter.name} loading="lazy" />
          ) : (
            <Heart size={18} />
          )}
        </div>
        <div className="settings-person-body">
          <div className="settings-person-title-row">
            <span className="settings-person-name">{supporter.name || "Private"}</span>
            <span className="settings-person-badge settings-person-badge--supporter">{t("settings.thanks.supporter")}</span>
          </div>
          <div className="settings-person-meta">
            <span>{supporter.isPrivate ? "Private sponsor" : "GitHub Sponsors"}</span>
            {supporter.profileUrl && <ExternalLink size={14} />}
          </div>
        </div>
      </>,
    );
  }, [renderPersonLink]);

  const thanksTabContent = (
    <div className="settings-thanks-layout">
      <section className="settings-section settings-thanks-hero">
        <div className="settings-thanks-hero-icon">
          <Heart size={18} />
        </div>
        <div className="settings-thanks-hero-copy">
          <h2>{t("settings.thanks.title")}</h2>
          <p>{t("settings.thanks.subtitle")}</p>
        </div>
      </section>

      {thanksFetchError && (
        <section className="settings-section settings-thanks-status settings-thanks-status--error">
          <strong>{t("settings.thanks.communityDataUnavailable")}</strong>
          <span>{thanksFetchError}</span>
          <div className="settings-thanks-actions">
            <button type="button" className="settings-chip settings-thanks-retry-btn" onClick={handleRetryThanks}>
              {t("app.actions.retry")}
            </button>
          </div>
        </section>
      )}

      <div className="settings-thanks-grid">
        <section className="settings-section">
          <div className="settings-section-header settings-section-header--thanks">
            <Users size={18} />
            <div>
              <h2>{t("settings.thanks.contributorsTitle")}</h2>
              <p className="settings-section-subtitle">{t("settings.thanks.contributorsSubtitle")}</p>
            </div>
          </div>
          {thanksLoadState === "loading" && !thanksData ? (
            <div className="settings-thanks-state">
              <Loader size={16} className="settings-loading-icon" />
              <span>{t("settings.thanks.loadingContributors")}</span>
            </div>
          ) : thanksContributors.length > 0 ? (
            <div className="settings-people-grid">
              {thanksContributors.map((contributor) => (
                <div key={contributor.login}>{renderContributorCard(contributor)}</div>
              ))}
            </div>
          ) : (
            <div className="settings-thanks-state settings-thanks-state--muted">
              <span>{thanksData?.contributorsError ?? t("settings.thanks.noContributors")}</span>
            </div>
          )}
        </section>

        <section className="settings-section">
          <div className="settings-section-header settings-section-header--thanks">
            <Heart size={18} />
            <div>
              <h2>{t("settings.thanks.supportersTitle")}</h2>
              <p className="settings-section-subtitle">{t("settings.thanks.supportersSubtitle")}</p>
            </div>
          </div>
          {thanksLoadState === "loading" && !thanksData ? (
            <div className="settings-thanks-state">
              <Loader size={16} className="settings-loading-icon" />
              <span>{t("settings.thanks.loadingSupporters")}</span>
            </div>
          ) : thanksSupporters.length > 0 ? (
            <div className="settings-people-grid">
              {thanksSupporters.map((supporter, index) => (
                <div key={`${supporter.name}-${supporter.profileUrl ?? index}`}>{renderSupporterCard(supporter)}</div>
              ))}
            </div>
          ) : (
            <div className="settings-thanks-state settings-thanks-state--muted">
              <span>{thanksData?.supportersError ?? t("settings.thanks.noSupporters")}</span>
            </div>
          )}
        </section>
      </div>

      {hasThanksError && thanksData && (
        <section className="settings-section settings-thanks-status">
          {thanksData.contributorsError && <span>{t("settings.thanks.contributorsTitle")}: {thanksData.contributorsError}</span>}
          {thanksData.supportersError && <span>{t("settings.thanks.supportersTitle")}: {thanksData.supportersError}</span>}
        </section>
      )}
    </div>
  );


  const normalizedSettingsSearch = settingsSearch.trim().toLowerCase();
  const showAll = normalizedSettingsSearch.length > 0;
  const tokenMatchesWord = (token: string, word: string): boolean => token === word || word.startsWith(token);
  const scopeMatchesSearch = (scopeId: SettingsSearchScopeId): boolean => {
    if (!showAll) {
      return true;
    }
    const terms = SETTINGS_SCOPE_SEARCH_TERMS[scopeId];
    const searchTokens = normalizedSettingsSearch.split(/[^a-z0-9]+/).filter((token) => token.length > 0);
    if (searchTokens.length === 0) {
      return true;
    }
    const searchableWords = Array.from(
      new Set(
        terms
          .join(" ")
          .toLowerCase()
          .split(/[^a-z0-9]+/)
          .filter((word) => word.length > 0),
      ),
    );
    return searchTokens.every((token) => searchableWords.some((word) => tokenMatchesWord(token, word)));
  };

  const showStreamRegion = showAll ? scopeMatchesSearch("stream-region") : activeSection === "stream";
  const showStreamVideo = showAll ? scopeMatchesSearch("stream-video") : activeSection === "stream";
  const showStreamCodecDiagnostics = showAll ? scopeMatchesSearch("stream-codec-diagnostics") : activeSection === "stream";
  const showStream = showStreamRegion || showStreamVideo || showStreamCodecDiagnostics;
  const showNativeStreamer = showAll ? scopeMatchesSearch("native-streamer") : activeSection === "native-streamer";
  const showGame = showAll ? scopeMatchesSearch("game") : activeSection === "game";
  const showAudio = showAll ? scopeMatchesSearch("audio") : activeSection === "audio";
  const showInput = showAll ? scopeMatchesSearch("input") : activeSection === "input";
  const showInterface = showAll ? scopeMatchesSearch("interface") : activeSection === "interface";
  const showAbout = showAll ? scopeMatchesSearch("about") : activeSection === "about";
  const showThanks = showAll ? scopeMatchesSearch("thanks") : activeSection === "thanks";
  const hasAnySearchMatches = showStream || showNativeStreamer || showGame || showAudio || showInput || showInterface || showAbout || showThanks;
  const shouldRenderSettingsSections = showAll || activeSection !== "thanks";

  return (
    <div className="settings-page">
      <header className="settings-header">
        <h1>{t("settings.title")}</h1>
        <div className={`settings-saved ${savedIndicator ? "visible" : ""}`}>
          <Check size={14} />
          {t("settings.saved")}
        </div>
      </header>

      {nativeStreamerEnablePromptVisible && (
        <div
          className={`native-streamer-warning ${nativeStreamerEnablePromptClosing ? "native-streamer-warning--closing" : ""}`}
          role="dialog"
          aria-modal="true"
          aria-labelledby="native-streamer-warning-title"
          aria-describedby="native-streamer-warning-copy"
        >
          <button
            type="button"
            className="native-streamer-warning-backdrop"
            aria-label={t("app.actions.cancel")}
            aria-hidden="true"
            tabIndex={-1}
            onClick={closeNativeStreamerEnablePrompt}
          />
          <div ref={nativeStreamerEnablePromptRef} className="native-streamer-warning-card" tabIndex={-1}>
            <div className="native-streamer-warning-kicker">
              <AlertTriangle size={14} />
              {t("settings.nativeStreamer.enablePromptKicker")}
            </div>
            <h3 id="native-streamer-warning-title" className="native-streamer-warning-title">
              {t("settings.nativeStreamer.enablePromptTitle")}
            </h3>
            <p id="native-streamer-warning-copy" className="native-streamer-warning-text">
              {t("settings.nativeStreamer.enablePromptBody")}
            </p>

            <div className="native-streamer-warning-list">
              <div className="native-streamer-warning-list-item">
                <Cpu size={16} />
                <span>{t("settings.nativeStreamer.enablePromptNewSessions")}</span>
              </div>
              <div className="native-streamer-warning-list-item">
                <Keyboard size={16} />
                <span>{t("settings.nativeStreamer.enablePromptShortcuts")}</span>
              </div>
              <div className="native-streamer-warning-list-item">
                <Monitor size={16} />
                <span>{t("settings.nativeStreamer.enablePromptAltTab")}</span>
              </div>
            </div>

            <div className="native-streamer-warning-actions">
              <button
                type="button"
                className="native-streamer-warning-btn native-streamer-warning-btn--secondary"
                onClick={closeNativeStreamerEnablePrompt}
              >
                {t("settings.nativeStreamer.enablePromptCancel")}
              </button>
              <button
                type="button"
                className="native-streamer-warning-btn native-streamer-warning-btn--primary"
                onClick={confirmNativeStreamerEnablePrompt}
                ref={nativeStreamerEnablePromptConfirmRef}
                autoFocus
              >
                {t("settings.nativeStreamer.enablePromptEnable")}
              </button>
            </div>
            <div className="native-streamer-warning-hint">
              <kbd>Esc</kbd> {t("settings.nativeStreamer.enablePromptEsc")}
            </div>
          </div>
        </div>
      )}

      <div className="settings-layout">

      {/* ── Sidebar ───────────────────────────────────────── */}
      <nav className="settings-sidebar">
        <div className="settings-search-wrap">
          <Search size={13} className="settings-search-icon" />
          <input
            type="text"
            className="settings-search-input"
            placeholder={t("settings.searchPlaceholder")}
            value={settingsSearch}
            onChange={e => setSettingsSearch(e.target.value)}
          />
          {settingsSearch && (
            <button type="button" className="settings-search-clear" onClick={() => setSettingsSearch("")}>
              <X size={11} />
            </button>
          )}
        </div>
        <nav className="settings-nav">
          {([
            { id: "stream" as SettingsSectionId, label: t("settings.sections.stream"), icon: <Wifi size={15} /> },
            { id: "native-streamer" as SettingsSectionId, label: t("settings.sections.nativeStreamer"), icon: <Cpu size={15} /> },
            { id: "game" as SettingsSectionId, label: t("settings.sections.game"), icon: <Globe size={15} /> },
            { id: "audio" as SettingsSectionId, label: t("settings.sections.audio"), icon: <Mic size={15} /> },
            { id: "input" as SettingsSectionId, label: t("settings.sections.input"), icon: <Keyboard size={15} /> },
            { id: "interface" as SettingsSectionId, label: t("settings.sections.interface"), icon: <Monitor size={15} /> },
            { id: "about" as SettingsSectionId, label: t("settings.sections.about"), icon: <Info size={15} /> },
            { id: "thanks" as SettingsSectionId, label: t("settings.sections.thanks"), icon: <Heart size={15} /> },
          ]).map(item => (
            <button
              key={item.id}
              type="button"
              className={`settings-nav-item ${!showAll && activeSection === item.id ? "active" : ""}`}
              onClick={() => { setActiveSection(item.id); setSettingsSearch(""); }}
            >
              {item.icon}
              {item.label}
            </button>
          ))}
        </nav>
      </nav>

      {/* ── Content ───────────────────────────────────────── */}
      <div className="settings-content">
        {showAll && !hasAnySearchMatches ? (
          <section className="settings-section">
            <div className="settings-thanks-state settings-thanks-state--muted">
              <span>{t("settings.noMatches", { query: settingsSearch.trim() })}</span>
            </div>
          </section>
        ) : (
          <>
            {showThanks && thanksTabContent}
            {shouldRenderSettingsSections && (
              <>
            {/* ═══ STREAM ════════════════════════════════════ */}
            {showStream && (
              <>
                {/* ── Region ── */}
                {showStreamRegion && (
                <section className="settings-section">
                  {showAll && <div className="settings-section-context">{t("settings.sections.stream")}</div>}
                  <div className="settings-section-header">
                    <h2>{t("settings.region.title")}</h2>
                  </div>
                  <div className="settings-rows">
                    <div className="region-selector">
              <button
                className={`region-selected ${regionDropdownOpen ? "open" : ""}`}
                onClick={() => setRegionDropdownOpen(!regionDropdownOpen)}
                type="button"
              >
                <span className="region-selected-name">{selectedRegionName}</span>
                {!settings.region && bestRegionUrl && (
                  (() => {
                    const bestRegion = regions.find(r => r.url === bestRegionUrl);
                    const pingValue = pingResults.get(bestRegionUrl);
                    if (bestRegion && pingValue !== undefined && pingValue !== null) {
                      return (
                        <span className="region-selected-best-info">
                          {bestRegion.name} • <span className={`region-selected-ping-inline ${pingValue <= 50 ? 'good' : pingValue <= 100 ? 'medium' : 'poor'}`}>{pingValue}ms</span>
                        </span>
                      );
                    }
                    return null;
                  })()
                )}
                {settings.region && (
                  (() => {
                    const pingValue = pingResults.get(settings.region);
                    if (pingValue !== undefined && pingValue !== null) {
                      return (
                        <span className={`region-selected-ping ${pingValue <= 50 ? 'good' : pingValue <= 100 ? 'medium' : 'poor'}`}>
                          {pingValue}ms
                        </span>
                      );
                    } else if (pingValue === null) {
	                      return <span className="region-selected-ping-unavailable">{t("app.status.failed")}</span>;
                    } else if (isPinging) {
                      return <span className="region-selected-ping-unavailable">{t("app.status.testing")}</span>;
                    }
                    return null;
                  })()
                )}
                <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" className={`region-chevron ${regionDropdownOpen ? "flipped" : ""}`}>
                  <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                </svg>
              </button>

              {regionDropdownOpen && (
                <div className="region-dropdown">
                  <div className="region-dropdown-header">
                    <div className="region-dropdown-search">
                      <Search size={14} className="region-dropdown-search-icon" />
                      <input
                        type="text"
                        className="region-dropdown-search-input"
                        placeholder={t("settings.region.searchPlaceholder")}
                        value={regionSearch}
                        onChange={(e) => setRegionSearch(e.target.value)}
                        autoFocus
                      />
                      {regionSearch && (
                        <button className="region-dropdown-clear" onClick={() => setRegionSearch("")} type="button">
                          <X size={12} />
                        </button>
                      )}
                    </div>
                    <button
                      className="region-ping-refresh"
                      onClick={runPingTest}
                      disabled={isPinging}
                      type="button"
                      title={t("settings.region.refreshPing")}
                    >
                      {isPinging ? (
                        <Loader size={14} className="spin" />
                      ) : (
                        <Wifi size={14} />
                      )}
                    </button>
                  </div>

                  <div className="region-dropdown-list">
                    <button
                      className={`region-dropdown-item ${!settings.region ? "active" : ""}`}
                      onClick={() => {
                        handleChange("region", "");
                        setRegionDropdownOpen(false);
                        setRegionSearch("");
                      }}
                      type="button"
                    >
                      <Globe size={14} />
                      <div className="region-auto-best-info">
                        <span>{t("settings.region.autoBest")}</span>
                        {bestRegionUrl && (() => {
                          const bestRegion = regions.find(r => r.url === bestRegionUrl);
                          const bestPing = pingResults.get(bestRegionUrl);
                          if (bestRegion && bestPing !== undefined && bestPing !== null) {
                            return (
                              <span className="region-auto-best-details">
                                {bestRegion.name} • {bestPing}ms
                              </span>
                            );
                          }
                          return null;
                        })()}
                      </div>
                      {!settings.region && <Check size={14} className="region-check" />}
                    </button>

                    {filteredRegions.map((region) => (
                      <button
                        key={region.url}
                        className={`region-dropdown-item ${settings.region === region.url ? "active" : ""}`}
                        onClick={() => {
                          handleChange("region", region.url);
                          setRegionDropdownOpen(false);
                          setRegionSearch("");
                        }}
                        type="button"
                      >
                        <Globe size={14} />
                        <span className="region-name-with-badge">
                          {region.name}
                          {region.url === bestRegionUrl && (
                            <span className="region-best-badge">{t("app.labels.best")}</span>
                          )}
                        </span>
                        <span className="region-ping">
                          {isPinging ? (
                            <span className="region-ping-loading">...</span>
                          ) : (
                            (() => {
                              const pingValue = pingResults.get(region.url);
                              if (pingValue === undefined) {
                                return <span className="region-ping-unavailable">-</span>;
                              } else if (pingValue === null) {
                                return <span className="region-ping-error">{t("app.status.failed")}</span>;
                              } else {
                                return (
                                  <span className={`region-ping-value ${pingValue <= 50 ? 'good' : pingValue <= 100 ? 'medium' : 'poor'}`}>
                                    {pingValue}ms
                                  </span>
                                );
                              }
                            })()
                          )}
                        </span>
                        {settings.region === region.url && <Check size={14} className="region-check" />}
                      </button>
                    ))}

                    {filteredRegions.length === 0 && regions.length > 0 && (
                      <div className="region-dropdown-empty">{t("settings.region.noRegionsMatch", { query: regionSearch })}</div>
                    )}
                  </div>
                </div>
              )}
                </div>
              </div>
            </section>

                )}
            {showStreamVideo && (
            <section className="settings-section">
              {showAll && <div className="settings-section-context">{t("settings.sections.stream")}</div>}
              <div className="settings-section-header">
                <h2>{t("settings.video.title")}</h2>
              </div>
              <div className="settings-rows">
                {/* Aspect Ratio — static chips */}
                <div className="settings-row">
                  <label className="settings-label">{t("settings.video.aspectRatio")}</label>
                  <div className="settings-chip-row">
                    {STATIC_ASPECT_RATIO_PRESETS.map((preset) => (
                      <button
                        key={preset.value}
                        className={`settings-chip ${settings.aspectRatio === preset.value ? "active" : ""}`}
                        onClick={() => { handleChange("aspectRatio", preset.value as any); }}
                      >
                        <span>{preset.label}</span>
                      </button>
                    ))}
                  </div>
                </div>

                {/* Resolution — grouped dropdown */}
                <div className="settings-row settings-row--column">
                  <label className="settings-label">
                    {t("settings.video.resolution")}
                    {subscriptionLoading && <Loader size={12} className="settings-loading-icon" />}
                  </label>
                  <div className="settings-dropdown settings-resolution-dropdown" ref={resolutionDropdownRef}>
                    <button
                      type="button"
                      className={`settings-dropdown-selected ${resolutionDropdownOpen ? "open" : ""}`}
                      onClick={() => setResolutionDropdownOpen(o => !o)}
                    >
                      <span className="settings-dropdown-selected-name">{selectedResolutionLabel}</span>
                      <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" className={`settings-dropdown-chevron ${resolutionDropdownOpen ? "flipped" : ""}`}>
                        <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                      </svg>
                    </button>
                    {resolutionDropdownOpen && (
                      <div className="settings-dropdown-menu settings-dropdown-menu--grouped">
                        {(hasDynamic ? resolutionGroups : [{ category: "All", resolutions: STATIC_RESOLUTION_PRESETS.map(p => ({ ...p, width: 0, height: 0 })) }]).map(group => (
                          <div key={group.category} className="settings-dropdown-group">
                            <div className="settings-dropdown-group-label">{group.category}</div>
                            {group.resolutions.map(res => (
                              <button
                                key={res.value}
                                type="button"
                                className={`settings-dropdown-item ${settings.resolution === res.value ? "active" : ""}`}
                                onClick={() => { handleChange("resolution", res.value); setResolutionDropdownOpen(false); }}
                              >
                                <span>{res.label}</span>
                                {settings.resolution === res.value && <Check size={14} className="settings-dropdown-check" />}
                              </button>
                            ))}
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </div>

                {/* FPS — dynamic or static chips */}
                <div className="settings-row">
                  <label className="settings-label">{t("settings.video.fps")}</label>
                  <div className="settings-chip-row">
                    {(hasDynamic ? dynamicFpsOptions.map((v) => ({ value: v })) : STATIC_FPS_PRESETS).map((preset) => (
                      <button
                        key={preset.value}
                        className={`settings-chip ${settings.fps === preset.value ? "active" : ""}`}
                        onClick={() => { handleChange("fps", preset.value); }}
                      >
                        <span>{preset.value}</span>
                      </button>
                    ))}
                  </div>
                </div>

                {/* Codec */}
                <div className="settings-row">
                  <label className="settings-label">{t("settings.video.codec")}</label>
                  <div className="settings-chip-row">
                    {codecOptions.map((codec) => {
                      const badgeState = getCodecDecodeBadgeState(codec, codecResults, codecTesting);
                      return (
                        <button
                          key={codec}
                          className={`settings-chip settings-chip--codec ${settings.codec === codec ? "active" : ""}`}
                          onClick={() => handleCodecChange(codec)}
                        >
                          <span>{codec}</span>
                          {badgeState && (
                            <span className={`settings-inline-badge settings-inline-badge--codec settings-inline-badge--codec-${badgeState}`}>
                              {badgeState === "gpu" ? t("settings.video.gpu") : badgeState === "cpu" ? t("settings.video.cpu") : t("settings.video.testing")}
                            </span>
                          )}
                        </button>
                      );
                    })}
                  </div>
                </div>

                <div className="settings-row settings-row--column">
                  <label className="settings-label">{t("settings.video.decoder")}</label>
                  <div className="settings-chip-row">
                    {accelerationOptions.map((option) => (
                      <button
                        key={`decoder-${option.value}`}
                        className={`settings-chip ${settings.decoderPreference === option.value ? "active" : ""}`}
                        onClick={() => handleChange("decoderPreference", option.value)}
                      >
                        {option.value === "auto"
                          ? t("app.labels.auto")
                          : option.value === "hardware"
                            ? t("app.labels.hardware")
                            : t("settings.video.softwareCpu")}
                      </button>
                    ))}
                  </div>
                  <span className="settings-subtle-hint">{t("settings.video.appliesAfterRestart")}</span>
                </div>

                <div className="settings-row settings-row--column">
                  <label className="settings-label">{t("settings.video.encoder")}</label>
                  <div className="settings-chip-row">
                    {accelerationOptions.map((option) => (
                      <button
                        key={`encoder-${option.value}`}
                        className={`settings-chip ${settings.encoderPreference === option.value ? "active" : ""}`}
                        onClick={() => handleChange("encoderPreference", option.value)}
                      >
                        {option.value === "auto"
                          ? t("app.labels.auto")
                          : option.value === "hardware"
                            ? t("app.labels.hardware")
                            : t("settings.video.softwareCpu")}
                      </button>
                    ))}
                  </div>
                  <span className="settings-subtle-hint">{t("settings.video.appliesAfterRestart")}</span>
                </div>

                {/* Color Quality */}
                <div className="settings-row settings-row--column">
                  <label className="settings-label">{t("settings.video.colorDepth")}</label>
                  <div className="settings-chip-row">
                    {colorQualityOptions.map((opt) => {
                      const needsHevc = colorQualityRequiresHevc(opt.value);
                      const colorDescription = opt.value === "8bit_420"
                        ? t("settings.colorQuality.mostCompatible")
                        : opt.value === "8bit_444"
                          ? t("settings.colorQuality.sharperChroma")
                          : opt.value === "10bit_420"
                            ? t("settings.colorQuality.higherBitDepth")
                            : t("settings.colorQuality.highestChromaAndBitDepth");
                      return (
                        <button
                          key={opt.value}
                          className={`settings-chip ${settings.colorQuality === opt.value ? "active" : ""}`}
                          onClick={() => handleColorQualityChange(opt.value)}
                          title={needsHevc ? t("settings.colorQuality.requiresH265OrAv1Title", { description: colorDescription }) : colorDescription}
                        >
                          <span>{opt.label}</span>
                        </button>
                      );
                    })}
                  </div>
                  {colorQualityRequiresHevc(settings.colorQuality) && settings.codec === "H264" && (
                    <span className="settings-input-hint">{t("settings.video.requiresH265OrAv1")}</span>
                  )}
                </div>

                {/* Bitrate slider */}
                <div className="settings-row settings-row--column">
                  <div className="settings-row-top">
                    <label className="settings-label">{t("settings.video.maxBitrate")}</label>
                    <span className="settings-value-badge">{settings.maxBitrateMbps} Mbps</span>
                  </div>
                  <input
                    type="range"
                    className="settings-slider"
                    min={5}
                    max={150}
                    step={5}
                    value={settings.maxBitrateMbps}
                    onChange={(e) => handleChange("maxBitrateMbps", parseInt(e.target.value, 10))}
                  />
                </div>

                <div className="settings-row settings-row--column">
                  <div className="settings-row-top settings-row-top--compact">
                    <label className="settings-label settings-label--wrap">
                      <span className="settings-label-title">
                        {t("settings.video.sessionProxy")}
                        <span className="settings-inline-badge settings-inline-badge--beta">{t("app.labels.beta")}</span>
                      </span>
                    </label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.sessionProxyEnabled}
                        onChange={(e) => handleChange("sessionProxyEnabled", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>
                  <span className="settings-subtle-hint">
                    {t("settings.video.sessionProxyHint")}
                  </span>
                  {settings.sessionProxyEnabled && (
                    <input
                      type="text"
                      className="settings-text-input"
                      placeholder="http://127.0.0.1:8080"
                      value={settings.sessionProxyUrl}
                      onChange={(e) => handleChange("sessionProxyUrl", e.target.value)}
                    />
                  )}
                </div>

                <div className="settings-row settings-row--column">
                  <div className="settings-row-top settings-row-top--compact">
                    <label className="settings-label settings-label--wrap">
                      <span className="settings-label-title">
                        {t("settings.video.experimentalL4SRequest")}
                        <span className="settings-inline-badge settings-inline-badge--beta">{t("app.labels.beta")}</span>
                      </span>
                    </label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.enableL4S}
                        onChange={(e) => handleChange("enableL4S", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>
                  <span className="settings-subtle-hint">
                    {t("settings.video.experimentalL4SRequestHint")}
                  </span>
                </div>
              </div>
            </section>

            )}
            {showStreamCodecDiagnostics && (
            <div className="settings-advanced-wrap">
              <button
                type="button"
                className="settings-advanced-toggle"
                onClick={() => setCodecAdvancedOpen(v => !v)}
              >
                <Zap size={14} />
                {t("settings.codecDiagnostics.advanced")}
                <svg viewBox="0 0 16 16" width="12" height="12" fill="currentColor" className={`settings-advanced-chevron ${codecAdvancedOpen ? "flipped" : ""}`}>
                  <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                </svg>
              </button>
              {codecAdvancedOpen && (
                <section className="settings-section">
                  {showAll && <div className="settings-section-context">{t("settings.sections.stream")}</div>}
                  <div className="settings-section-header">
                    <h2>{t("settings.codecDiagnostics.title")}</h2>
                  </div>
                  <div className="settings-rows">
                    <div className="settings-row codec-test-row">
                      <label className="settings-label codec-test-description">
                        {t("settings.codecDiagnostics.description")}
                      </label>
                      <button
                        className="codec-test-btn"
                        onClick={() => { void onRunCodecTest(); }}
                        disabled={codecTesting}
                        type="button"
                      >
                        {codecTesting ? (
                          <>
                            <Loader size={16} className="settings-loading-icon" />
                            {t("settings.video.testing")}
                          </>
                        ) : (
                          <>
                            <Zap size={16} />
                            {codecResults ? t("settings.codecDiagnostics.retest") : t("settings.codecDiagnostics.testCodecs")}
                          </>
                        )}
                      </button>
                    </div>
                    {codecTestOpen && codecResults && (
                      <div className="codec-results">
                        {codecResults.map((result) => (
                          <div key={result.codec} className="codec-result-card">
                            <div className="codec-result-header">
                              <span className="codec-result-name">{result.codec}</span>
                              <span className={`codec-result-badge ${result.webrtcSupported ? "supported" : "unsupported"}`}>
                                {result.webrtcSupported ? t("settings.codecDiagnostics.webrtcReady") : t("settings.codecDiagnostics.notInWebrtc")}
                              </span>
                            </div>
                            <div className="codec-result-rows">
                              <div className="codec-result-row">
                                <span className="codec-result-direction">{t("settings.codecDiagnostics.decode")}</span>
                                <span className={`codec-result-status ${result.decodeSupported ? (result.hwAccelerated ? "hw" : "sw") : "none"}`}>
                                  {result.decodeSupported ? (result.hwAccelerated ? t("settings.video.gpu") : t("settings.video.cpu")) : t("app.labels.no")}
                                </span>
                                <span className="codec-result-via">{result.decodeVia}</span>
                              </div>
                              <div className="codec-result-row">
                                <span className="codec-result-direction">{t("settings.codecDiagnostics.encode")}</span>
                                <span className={`codec-result-status ${result.encodeSupported ? (result.encodeHwAccelerated ? "hw" : "sw") : "none"}`}>
                                  {result.encodeSupported ? (result.encodeHwAccelerated ? t("settings.video.gpu") : t("settings.video.cpu")) : t("app.labels.no")}
                                </span>
                                <span className="codec-result-via">{result.encodeVia}</span>
                              </div>
                            </div>
                            {result.profiles.length > 0 && (
                              <div className="codec-result-profiles">
                                <span className="codec-result-profiles-label">{t("settings.codecDiagnostics.profiles")}</span>
                                <div className="codec-result-profiles-list">
                                  {result.profiles.map((p, i) => (
                                    <code key={i} className="codec-result-profile">{p}</code>
                                  ))}
                                </div>
                              </div>
                            )}
                          </div>
                        ))}
                      </div>
                    )}
                  </div>
                </section>
              )}
            </div>
            )}
          </>
        )}

        {/* ═══ NATIVE STREAMER ═════════════════════════════ */}
        {showNativeStreamer && (
          <section className="settings-section">
            {showAll && <div className="settings-section-context">{t("settings.sections.nativeStreamer")}</div>}
            <div className="settings-section-header">
              <h2>{t("settings.nativeStreamer.title")}</h2>
            </div>
            <div className="settings-rows">
              {!isWindows ? (
                <div className="settings-row settings-row--column">
                  <div className="settings-row-top settings-row-top--compact">
                    <label className="settings-label settings-label--wrap">
                      <span className="settings-label-title">
                        {t("settings.nativeStreamer.nativeStreaming")}
                        <span className="settings-inline-badge settings-inline-badge--beta">{t("app.labels.experimental")}</span>
                      </span>
                    </label>
                  </div>
                  <span className="settings-input-hint">
                    {NATIVE_STREAMER_WINDOWS_ONLY_MESSAGE}
                  </span>
                </div>
              ) : (
                <>
                  <div className="settings-row settings-row--column">
                    <div className="settings-row-top settings-row-top--compact">
                      <label className="settings-label settings-label--wrap">
                        <span className="settings-label-title">
                          {t("settings.nativeStreamer.nativeStreaming")}
                          <span className="settings-inline-badge settings-inline-badge--beta">{t("app.labels.experimental")}</span>
                        </span>
                      </label>
                      <label className="settings-toggle">
                        <input
                          type="checkbox"
                          checked={settings.streamClientMode === "native"}
                          onChange={(e) => handleNativeStreamerToggleChange(e.target.checked)}
                        />
                        <span className="settings-toggle-track" />
                      </label>
                    </div>
                    <span className="settings-subtle-hint">
                      {t("settings.nativeStreamer.nativeStreamingHint")}
                    </span>
                    <div className="settings-chip-row">
                      <a className="settings-chip" href="https://github.com/OpenCloudGaming/OpenNOW/issues" target="_blank" rel="noreferrer">
                        <span>{t("settings.nativeStreamer.reportOnGithubIssues")}</span>
                        <ExternalLink size={13} />
                      </a>
                      <a className="settings-chip" href="https://discord.gg/8EJYaJcNfD" target="_blank" rel="noreferrer">
                        <span>{t("settings.nativeStreamer.reportOnDiscord")}</span>
                        <ExternalLink size={13} />
                      </a>
                    </div>
                  </div>

                  <div className="settings-row">
                    <label className="settings-label">
                      {t("settings.nativeStreamer.showNativeStreamerStats")}
                      <span className="settings-hint">{t("settings.nativeStreamer.showNativeStreamerStatsHint")}</span>
                    </label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.showNativeStreamerStats}
                        onChange={(e) => handleChange("showNativeStreamerStats", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>

                  <div className="settings-row settings-row--column">
                    <div className="settings-row-top settings-row-top--compact">
                      <label className="settings-label settings-label--wrap">
                        <span className="settings-label-title">{t("settings.nativeStreamer.streamerStatus")}</span>
                      </label>
                      <button
                        type="button"
                        className="settings-icon-button"
                        onClick={() => void refreshNativeStreamerStatus()}
                        disabled={nativeStreamerStatusLoading}
                        title={t("settings.nativeStreamer.checkNativeStreamer")}
                        aria-label={t("settings.nativeStreamer.checkNativeStreamer")}
                      >
                        {nativeStreamerStatusLoading ? <Loader size={15} className="spin" /> : <RefreshCcw size={15} />}
                      </button>
                    </div>
                    <div className="settings-chip-row">
                      <span
                        className={`settings-inline-badge ${
                          nativeStreamerStatusLoading
                            ? "settings-inline-badge--codec-testing"
                            : nativeStreamerStatus?.gstreamerAvailable
                              ? "settings-inline-badge--codec-gpu"
                              : "settings-inline-badge--updater-error"
                        }`}
                      >
                        {nativeStreamerStatusLoading
                          ? t("app.status.checking")
                          : nativeStreamerStatus?.gstreamerAvailable
                            ? t("settings.nativeStreamer.gstreamerReady")
                            : t("settings.nativeStreamer.notReady")}
                      </span>
                    </div>
                    <span className="settings-subtle-hint">
                      {nativeStreamerStatus?.message ?? t("settings.nativeStreamer.statusDefaultHint")}
                    </span>
                  </div>

                  <div className="settings-row settings-row--column">
                    <label className="settings-label">{t("settings.nativeStreamer.gstreamerRuntime")}</label>
                    <div className="settings-chip-row">
                      <span className={`settings-inline-badge ${getGstreamerRuntimeBadgeClass(nativeStreamerStatus)}`}>
                        {formatGstreamerRuntimeLabel(nativeStreamerStatus)}
                      </span>
                      {nativeStreamerStatus?.gstreamerRuntime.path ? (
                        <span className="settings-inline-badge settings-inline-badge--codec">
                          {t("settings.nativeStreamer.bundledPathDetected")}
                        </span>
                      ) : null}
                    </div>
                    <span className="settings-subtle-hint">
                      {nativeStreamerStatus?.gstreamerRuntime.message ?? t("settings.nativeStreamer.runtimeDefaultHint")}
                    </span>
                    {!nativeStreamerStatus?.gstreamerAvailable && nativeStreamerStatus?.gstreamerRuntime.installInstructions?.length ? (
                      <div className="settings-install-steps">
                        <span className="settings-subtle-hint">
                          {t("settings.nativeStreamer.linuxRuntimeHint")}
                        </span>
                        {nativeStreamerStatus.gstreamerRuntime.installInstructions.map((instruction) => (
                          <div key={instruction.distro} className="settings-install-step">
                            <span className="settings-install-step-title">{instruction.distro}</span>
                            <code>{instruction.command}</code>
                            {instruction.note ? <span className="settings-subtle-hint">{instruction.note}</span> : null}
                          </div>
                        ))}
                      </div>
                    ) : null}
                  </div>

                  <div className="settings-row settings-row--column">
                    <label className="settings-label">{t("settings.nativeStreamer.videoPath")}</label>
                    <div className="settings-chip-row">
                      <span
                        className={`settings-inline-badge ${
                          nativeStreamerStatus?.activeVideoBackend?.available
                            ? nativeStreamerStatus.activeVideoBackend.backend === "software"
                              ? "settings-inline-badge--codec-testing"
                              : "settings-inline-badge--codec-gpu"
                            : "settings-inline-badge--updater-error"
                        }`}
                      >
                        {formatNativeVideoBackendName(nativeStreamerStatus?.activeVideoBackend?.backend)}
                      </span>
                      {getAvailableNativeCodecLabels(nativeStreamerStatus?.activeVideoBackend).map((codec) => (
                        <span key={codec} className="settings-inline-badge settings-inline-badge--codec">
                          {codec}
                        </span>
                      ))}
                    </div>
                    <span className="settings-subtle-hint">
                      {nativeStreamerStatus?.gstreamerAvailable
                        ? `${nativeStreamerStatus.codecSummary ?? t("settings.nativeStreamer.codecSupportUnknown")}. ${nativeStreamerStatus.zeroCopySummary ?? t("settings.nativeStreamer.memoryPathUnknown")}.`
                        : nativeStreamerStatus?.activeVideoBackend?.reason
                          ?? t("settings.nativeStreamer.videoPathDefaultHint")}
                    </span>
                  </div>

                  <div className="settings-row settings-row--column">
                    <label className="settings-label">{t("settings.nativeStreamer.directxBackend")}</label>
                    <div className="settings-chip-row">
                      {nativeVideoBackendOptions.map((option) => (
                        <button
                          key={option.value}
                          type="button"
                          className={`settings-chip ${settings.nativeVideoBackend === option.value ? "active" : ""}`}
                          onClick={() => handleChange("nativeVideoBackend", option.value)}
                          title={option.description}
                        >
                          <span>{option.label}</span>
                        </button>
                      ))}
                    </div>
                    <span className="settings-subtle-hint">
                      {t("settings.nativeStreamer.directxBackendHint")}
                    </span>
                  </div>

                  <div className="settings-row settings-row--column">
                    <label className="settings-label">{t("settings.nativeStreamer.framePacing")}</label>
                    <div className="settings-chip-row">
                      <button
                        type="button"
                        className={`settings-chip ${!settings.enableCloudGsync ? "active" : ""}`}
                        onClick={() => setNativeFramePacing("low-latency")}
                      >
                        <span>{t("settings.nativeStreamer.lowestLatency")}</span>
                      </button>
                      <button
                        type="button"
                        className={`settings-chip ${settings.enableCloudGsync ? "active" : ""}`}
                        onClick={() => setNativeFramePacing("smooth")}
                      >
                        <span>{t("settings.nativeStreamer.smoothGsync")}</span>
                      </button>
                    </div>
                    <span className="settings-subtle-hint">
                      {t("settings.nativeStreamer.framePacingHint")}
                    </span>
                  </div>
                </>
              )}
            </div>
          </section>
        )}

        {/* ═══ GAME ══════════════════════════════════════ */}
        {showGame && (
          <section className="settings-section">
            {showAll && <div className="settings-section-context">{t("settings.sections.game")}</div>}
            <div className="settings-section-header">
              <h2>{t("settings.game.title")}</h2>
            </div>
            <div className="settings-rows">
              <div className="settings-row">
                <label className="settings-label">
                  {t("settings.game.language")}
                  <span className="settings-hint">{t("settings.game.inGameLanguageHint")}</span>
                </label>
                <div className="settings-dropdown" ref={gameLanguageDropdownRef}>
                  <button
                    type="button"
                    className={`settings-dropdown-selected ${gameLanguageDropdownOpen ? "open" : ""}`}
                    onClick={() => setGameLanguageDropdownOpen((open) => !open)}
                  >
                    <span className="settings-dropdown-selected-name">{selectedGameLanguageName}</span>
                    <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" className={`settings-dropdown-chevron ${gameLanguageDropdownOpen ? "flipped" : ""}`}>
                      <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                    </svg>
                  </button>
                  {gameLanguageDropdownOpen && (
                    <div className="settings-dropdown-menu settings-dropdown-menu--tall">
                      {gameLanguageOptions.map((option) => (
                        <button
                          key={option.value}
                          type="button"
                          className={`settings-dropdown-item ${settings.gameLanguage === option.value ? "active" : ""}`}
                          onClick={() => {
                            handleChange("gameLanguage", option.value);
                            setGameLanguageDropdownOpen(false);
                          }}
                        >
                          <span>{option.label}</span>
                          {settings.gameLanguage === option.value && <Check size={14} className="settings-dropdown-check" />}
                        </button>
                      ))}
                    </div>
                  )}
                </div>
              </div>
            </div>
          </section>
        )}

        {/* ═══ AUDIO ══════════════════════════════════════ */}
        {showAudio && (
          <section className="settings-section">
            {showAll && <div className="settings-section-context">{t("settings.sections.audio")}</div>}
            <div className="settings-section-header">
              <h2>{t("settings.audio.title")}</h2>
            </div>
              <div className="settings-rows">
                <div className="settings-row">
                  <label className="settings-label">
                    {t("settings.audio.microphone")}
                    <span className="settings-hint">{t("settings.audio.microphoneHint")}</span>
                  </label>
                  <div className="settings-dropdown" ref={microphoneModeDropdownRef}>
                    <button
                      type="button"
                      className={`settings-dropdown-selected ${microphoneModeDropdownOpen ? "open" : ""}`}
                      onClick={() => {
                        setMicrophoneModeDropdownOpen((open) => !open);
                        setMicrophoneDeviceDropdownOpen(false);
                      }}
                    >
                      <span className="settings-dropdown-selected-name">{selectedMicrophoneModeName}</span>
                      <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" className={`settings-dropdown-chevron ${microphoneModeDropdownOpen ? "flipped" : ""}`}>
                        <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                      </svg>
                    </button>
                    {microphoneModeDropdownOpen && (
                      <div className="settings-dropdown-menu">
                        {microphoneModeOptions.map((option) => (
                          <button
                            key={option.value}
                            type="button"
                            className={`settings-dropdown-item ${settings.microphoneMode === option.value ? "active" : ""}`}
                            onClick={() => {
                              handleChange("microphoneMode", option.value);
                              setMicrophoneModeDropdownOpen(false);
                            }}
                          >
                            <span>{getMicrophoneModeLabel(option.value)}</span>
                            {settings.microphoneMode === option.value && <Check size={14} className="settings-dropdown-check" />}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </div>

                {settings.microphoneMode !== "disabled" && (
                  <div className="settings-row">
                    <label className="settings-label">
                      <div className="flex items-center gap-2">
                        <Mic size={14} />
                        {t("settings.audio.microphoneDevice")}
                      </div>
                      <span className="settings-hint">{t("settings.audio.microphoneDeviceHint")}</span>
                    </label>
                    <div className="settings-mic-device-wrap">
                      <div className="settings-dropdown" ref={microphoneDeviceDropdownRef}>
                        <button
                          type="button"
                          className={`settings-dropdown-selected ${microphoneDeviceDropdownOpen ? "open" : ""}`}
                          onClick={() => {
                            if (microphoneDevices.length === 0) return;
                            setMicrophoneDeviceDropdownOpen((open) => !open);
                            setMicrophoneModeDropdownOpen(false);
                          }}
                          disabled={microphoneDevices.length === 0}
                        >
                          <span className="settings-dropdown-selected-name">{selectedMicrophoneDeviceName}</span>
                          <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" className={`settings-dropdown-chevron ${microphoneDeviceDropdownOpen ? "flipped" : ""}`}>
                            <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                          </svg>
                        </button>
                        {microphoneDeviceDropdownOpen && (
                          <div className="settings-dropdown-menu settings-dropdown-menu--tall">
                            <button
                              type="button"
                              className={`settings-dropdown-item ${settings.microphoneDeviceId === "" ? "active" : ""}`}
                              onClick={() => {
                                handleChange("microphoneDeviceId", "");
                                setMicrophoneDeviceDropdownOpen(false);
                              }}
                            >
                              <span>{t("app.labels.defaultDevice")}</span>
                              {settings.microphoneDeviceId === "" && <Check size={14} className="settings-dropdown-check" />}
                            </button>
                            {microphoneDevices.map((device, index) => (
                              <button
                                key={device.deviceId}
                                type="button"
                                className={`settings-dropdown-item ${settings.microphoneDeviceId === device.deviceId ? "active" : ""}`}
                                onClick={() => {
                                  handleChange("microphoneDeviceId", device.deviceId);
                                  setMicrophoneDeviceDropdownOpen(false);
                                }}
                              >
                                <span>{device.label || t("settings.audio.microphoneIndexed", { index: index + 1 })}</span>
                                {settings.microphoneDeviceId === device.deviceId && <Check size={14} className="settings-dropdown-check" />}
                              </button>
                            ))}
                          </div>
                        )}
                      </div>
                      {microphonePermissionError && (
                        <span className="text-red-400 text-xs mt-1">{microphonePermissionError}</span>
                      )}
                      {microphoneDevices.length === 0 && !microphonePermissionError && (
                        <span className="text-yellow-400 text-xs mt-1">{t("settings.audio.noMicrophoneDevicesFound")}</span>
                      )}
                    </div>
                  </div>
                )}
              </div>
            </section>
        )}

        {/* ═══ INPUT ═══════════════════════════════════════ */}
        {showInput && (
            <section className="settings-section">
              {showAll && <div className="settings-section-context">{t("settings.sections.input")}</div>}
              <div className="settings-section-header">
                <h2>{t("settings.input.title")}</h2>
              </div>
              <div className="settings-rows">
                <div className="settings-row">
                  <label className="settings-label">{t("settings.input.clipboardPaste")}</label>
                  <label className="settings-toggle">
                    <input
                      type="checkbox"
                      checked={settings.clipboardPaste}
                      onChange={(e) => handleChange("clipboardPaste", e.target.checked)}
                    />
                    <span className="settings-toggle-track" />
                  </label>
                </div>

                <div className="settings-row settings-row--top-aligned">
                  <label className="settings-label settings-label--wrap">
                    {t("settings.game.keyboardLayout")}
                    <span className="settings-hint">{t("settings.input.keyboardLayoutHint")}</span>
                  </label>
                  <div className="settings-dropdown settings-dropdown--constrained" ref={keyboardLayoutDropdownRef}>
                    <button
                      type="button"
                      className={`settings-dropdown-selected ${keyboardLayoutDropdownOpen ? "open" : ""}`}
                      onClick={() => setKeyboardLayoutDropdownOpen((open) => !open)}
                    >
                      <span className="settings-dropdown-selected-name">{selectedKeyboardLayoutName}</span>
                      <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" className={`settings-dropdown-chevron ${keyboardLayoutDropdownOpen ? "flipped" : ""}`}>
                        <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                      </svg>
                    </button>
                    {keyboardLayoutDropdownOpen && (
                      <div className="settings-dropdown-menu settings-dropdown-menu--tall">
                        {keyboardLayoutOptions.map((option) => (
                          <button
                            key={option.value}
                            type="button"
                            className={`settings-dropdown-item ${settings.keyboardLayout === option.value ? "active" : ""}`}
                            onClick={() => {
                              handleChange("keyboardLayout", option.value);
                              setKeyboardLayoutDropdownOpen(false);
                            }}
                          >
                            <span>{option.label}</span>
                            {settings.keyboardLayout === option.value && <Check size={14} className="settings-dropdown-check" />}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </div>

                {/* Mouse Sensitivity */}
                <div className="settings-row settings-row--column">
                  <div className="settings-row-top">
                    <label className="settings-label">{t("settings.input.mouseSensitivity")}</label>
                    <span className="settings-value-badge">{settings.mouseSensitivity.toFixed(2)}x</span>
                  </div>
                  <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                    <input
                      type="range"
                      className="settings-slider"
                      min={0.1}
                      max={4}
                      step={0.01}
                      value={settings.mouseSensitivity}
                      onChange={(e) => handleChange("mouseSensitivity", parseFloat(e.target.value))}
                    />
                    <input
                      type="number"
                      className="settings-number-input"
                      style={{ width: 80 }}
                      min={0.1}
                      max={4}
                      step={0.01}
                      value={Number(settings.mouseSensitivity.toFixed(2))}
                      onChange={(e) => {
                        const v = parseFloat(e.target.value || "0");
                        if (Number.isFinite(v)) handleChange("mouseSensitivity", Math.max(0.1, Math.min(4, v)));
                      }}
                    />
                  </div>
                  <span className="settings-subtle-hint">{t("settings.input.mouseSensitivityHint")}</span>
                </div>

                <div className="settings-row settings-row--column">
                  <div className="settings-row-top">
                    <label className="settings-label">{t("settings.input.mouseAccelerator")}</label>
                    <span className="settings-value-badge">{Math.round(settings.mouseAcceleration)}%</span>
                  </div>
                  <div style={{ display: "flex", alignItems: "center", gap: "8px" }}>
                    <input
                      type="range"
                      className="settings-slider"
                      min={1}
                      max={150}
                      step={1}
                      value={Math.round(settings.mouseAcceleration)}
                      onChange={(e) => handleChange("mouseAcceleration", Math.max(1, Math.min(150, Math.round(Number(e.target.value) || 1))))}
                    />
                    <input
                      type="number"
                      className="settings-number-input"
                      style={{ width: 80 }}
                      min={1}
                      max={150}
                      step={1}
                      value={Math.round(settings.mouseAcceleration)}
                      onChange={(e) => {
                        const v = Number(e.target.value || "1");
                        if (Number.isFinite(v)) {
                          handleChange("mouseAcceleration", Math.max(1, Math.min(150, Math.round(v))));
                        }
                      }}
                    />
                  </div>
                  <span className="settings-subtle-hint">{t("settings.input.mouseAcceleratorHint")}</span>
                </div>

                {/* Shortcuts */}
                <div className="settings-row settings-row--column">
                  <div className="settings-row-top">
                    <label className="settings-label">{t("settings.input.shortcuts")}</label>
                    <div className="settings-shortcut-actions">
                      <span className="settings-value-badge">{t("settings.input.editable")}</span>
                      <button
                        type="button"
                        className="settings-shortcut-reset-btn"
                        onClick={handleResetShortcuts}
                        disabled={areShortcutsDefault}
                      >
                        {t("settings.input.resetToDefaults")}
                      </button>
                    </div>
                  </div>

                  <div className="settings-shortcut-grid">
                <div className="settings-shortcut-row">
                  <span className="settings-shortcut-label" id="shortcut-toggle-stats-label">{t("settings.input.toggleStats")}</span>
                  <input
                    type="text"
                    id="shortcut-toggle-stats"
                    aria-labelledby="shortcut-toggle-stats-label"
                    readOnly
                    className={`settings-text-input settings-shortcut-input ${toggleStatsError ? "error" : ""}`}
                    value={toggleStatsInput}
                    onFocus={(e) => e.target.select()}
                    onBlur={() => handleShortcutBlur("shortcutToggleStats", toggleStatsInput)}
                    onPaste={(e) => handleShortcutPaste("shortcutToggleStats", e)}
                    onKeyDown={(e) => handleShortcutCaptureKeyDown("shortcutToggleStats", e)}
                    placeholder={t("stream.shortcuts.clickHereThenPress")}
                    title={t("stream.shortcuts.focusAndPress")}
                    spellCheck={false}
                  />
                </div>

                <div className="settings-shortcut-row">
                  <span className="settings-shortcut-label" id="shortcut-pointer-lock-label">{t("settings.input.togglePointerLock")}</span>
                  <input
                    type="text"
                    id="shortcut-pointer-lock"
                    aria-labelledby="shortcut-pointer-lock-label"
                    readOnly
                    className={`settings-text-input settings-shortcut-input ${togglePointerLockError ? "error" : ""}`}
                    value={togglePointerLockInput}
                    onFocus={(e) => e.target.select()}
                    onBlur={() => handleShortcutBlur("shortcutTogglePointerLock", togglePointerLockInput)}
                    onPaste={(e) => handleShortcutPaste("shortcutTogglePointerLock", e)}
                    onKeyDown={(e) => handleShortcutCaptureKeyDown("shortcutTogglePointerLock", e)}
                    placeholder={t("stream.shortcuts.clickHereThenPress")}
                    title={t("stream.shortcuts.focusAndPress")}
                    spellCheck={false}
                  />
                </div>

                <div className="settings-shortcut-row">
                  <span className="settings-shortcut-label" id="shortcut-fullscreen-label">{t("settings.input.toggleFullscreen")}</span>
                  <input
                    type="text"
                    id="shortcut-fullscreen"
                    aria-labelledby="shortcut-fullscreen-label"
                    readOnly
                    className={`settings-text-input settings-shortcut-input ${toggleFullscreenError ? "error" : ""}`}
                    value={toggleFullscreenInput}
                    onFocus={(e) => e.target.select()}
                    onBlur={() => handleShortcutBlur("shortcutToggleFullscreen", toggleFullscreenInput)}
                    onPaste={(e) => handleShortcutPaste("shortcutToggleFullscreen", e)}
                    onKeyDown={(e) => handleShortcutCaptureKeyDown("shortcutToggleFullscreen", e)}
                    placeholder={t("stream.shortcuts.clickHereThenPress")}
                    title={t("stream.shortcuts.focusAndPress")}
                    spellCheck={false}
                  />
                </div>

                <div className="settings-shortcut-row">
                  <span className="settings-shortcut-label" id="shortcut-stop-stream-label">{t("settings.input.stopStream")}</span>
                  <input
                    type="text"
                    id="shortcut-stop-stream"
                    aria-labelledby="shortcut-stop-stream-label"
                    readOnly
                    className={`settings-text-input settings-shortcut-input ${stopStreamError ? "error" : ""}`}
                    value={stopStreamInput}
                    onFocus={(e) => e.target.select()}
                    onBlur={() => handleShortcutBlur("shortcutStopStream", stopStreamInput)}
                    onPaste={(e) => handleShortcutPaste("shortcutStopStream", e)}
                    onKeyDown={(e) => handleShortcutCaptureKeyDown("shortcutStopStream", e)}
                    placeholder={t("stream.shortcuts.clickHereThenPress")}
                    title={t("stream.shortcuts.focusAndPress")}
                    spellCheck={false}
                  />
                </div>

                <div className="settings-shortcut-row">
                  <span className="settings-shortcut-label" id="shortcut-anti-afk-label">{t("settings.input.toggleAntiAfk")}</span>
                  <input
                    type="text"
                    id="shortcut-anti-afk"
                    aria-labelledby="shortcut-anti-afk-label"
                    readOnly
                    className={`settings-text-input settings-shortcut-input ${toggleAntiAfkError ? "error" : ""}`}
                    value={toggleAntiAfkInput}
                    onFocus={(e) => e.target.select()}
                    onBlur={() => handleShortcutBlur("shortcutToggleAntiAfk", toggleAntiAfkInput)}
                    onPaste={(e) => handleShortcutPaste("shortcutToggleAntiAfk", e)}
                    onKeyDown={(e) => handleShortcutCaptureKeyDown("shortcutToggleAntiAfk", e)}
                    placeholder={t("stream.shortcuts.clickHereThenPress")}
                    title={t("stream.shortcuts.focusAndPress")}
                    spellCheck={false}
                  />
                </div>

                <div className="settings-shortcut-row">
                  <span className="settings-shortcut-label" id="shortcut-mic-label">{t("settings.input.toggleMicrophone")}</span>
                  <input
                    type="text"
                    id="shortcut-mic"
                    aria-labelledby="shortcut-mic-label"
                    readOnly
                    className={`settings-text-input settings-shortcut-input ${toggleMicrophoneError ? "error" : ""}`}
                    value={toggleMicrophoneInput}
                    onFocus={(e) => e.target.select()}
                    onBlur={() => handleShortcutBlur("shortcutToggleMicrophone", toggleMicrophoneInput)}
                    onPaste={(e) => handleShortcutPaste("shortcutToggleMicrophone", e)}
                    onKeyDown={(e) => handleShortcutCaptureKeyDown("shortcutToggleMicrophone", e)}
                    placeholder={t("stream.shortcuts.clickHereThenPress")}
                    title={t("stream.shortcuts.focusAndPress")}
                    spellCheck={false}
                  />
                </div>

                <div className="settings-shortcut-row">
                  <span className="settings-shortcut-label" id="shortcut-screenshot-label">{t("settings.input.screenshot")}</span>
                  <input
                    type="text"
                    id="shortcut-screenshot"
                    aria-labelledby="shortcut-screenshot-label"
                    readOnly
                    className={`settings-text-input settings-shortcut-input ${screenshotError ? "error" : ""}`}
                    value={screenshotInput}
                    onFocus={(e) => e.target.select()}
                    onBlur={() => handleShortcutBlur("shortcutScreenshot", screenshotInput)}
                    onPaste={(e) => handleShortcutPaste("shortcutScreenshot", e)}
                    onKeyDown={(e) => handleShortcutCaptureKeyDown("shortcutScreenshot", e)}
                    placeholder={t("stream.shortcuts.clickHereThenPress")}
                    title={t("stream.shortcuts.focusAndPress")}
                    spellCheck={false}
                  />
                </div>

                <div className="settings-shortcut-row">
                  <span className="settings-shortcut-label" id="shortcut-recording-label">{t("settings.input.recording")}</span>
                  <input
                    type="text"
                    id="shortcut-recording"
                    aria-labelledby="shortcut-recording-label"
                    readOnly
                    className={`settings-text-input settings-shortcut-input ${recordingError ? "error" : ""}`}
                    value={recordingInput}
                    onFocus={(e) => e.target.select()}
                    onBlur={() => handleShortcutBlur("shortcutToggleRecording", recordingInput)}
                    onPaste={(e) => handleShortcutPaste("shortcutToggleRecording", e)}
                    onKeyDown={(e) => handleShortcutCaptureKeyDown("shortcutToggleRecording", e)}
                    placeholder={t("stream.shortcuts.clickHereThenPress")}
                    title={t("stream.shortcuts.focusAndPress")}
                    spellCheck={false}
                  />
                </div>

                <div className="settings-shortcut-row">
                  <span className="settings-shortcut-label" id="shortcut-sidebar-label">{t("settings.input.toggleStreamSidebar")}</span>
                  <input
                    type="text"
                    id="shortcut-sidebar"
                    aria-labelledby="shortcut-sidebar-label"
                    value={formatShortcutForDisplay(SIDEBAR_TOGGLE_SHORTCUT_RAW, isMac)}
                    className="settings-text-input settings-shortcut-input settings-shortcut-input--static"
                    readOnly
                    tabIndex={-1}
                  />
                </div>
              </div>

              {(toggleStatsError || togglePointerLockError || toggleFullscreenError || stopStreamError || toggleAntiAfkError || toggleMicrophoneError || screenshotError || recordingError) && (
                <span className="settings-input-hint">
                  {toggleStatsError
                    || togglePointerLockError
                    || toggleFullscreenError
                    || stopStreamError
                    || toggleAntiAfkError
                    || toggleMicrophoneError
                    || screenshotError
                    || recordingError}
                </span>
              )}

              {!toggleStatsError && !togglePointerLockError && !toggleFullscreenError && !stopStreamError && !toggleAntiAfkError && !toggleMicrophoneError && !screenshotError && !recordingError && (
                <span className="settings-shortcut-hint">
                  {t("settings.input.shortcutHint", {
                    examples: t("stream.shortcuts.examples"),
                    fullscreen: formatShortcutForDisplay(settings.shortcutToggleFullscreen, isMac),
                    stop: formatShortcutForDisplay(settings.shortcutStopStream, isMac),
                    mic: formatShortcutForDisplay(settings.shortcutToggleMicrophone, isMac),
                    screenshot: formatShortcutForDisplay(settings.shortcutScreenshot, isMac),
                    recording: formatShortcutForDisplay(settings.shortcutToggleRecording, isMac),
                  })}
                </span>
              )}
                </div>
              </div>
            </section>
        )}

        {/* ═══ INTERFACE ══════════════════════════════════ */}
        {showInterface && (
          <>
            {/* ── Appearance ── */}
            <section className="settings-section">
              {showAll && <div className="settings-section-context">{t("settings.sections.interface")}</div>}
              <div className="settings-section-header">
                <h2>{t("settings.interface.appearance")}</h2>
              </div>
              <div className="settings-rows">
                <div className="settings-row">
                  <label className="settings-label">
                    {t("settings.interface.appLanguage")}
                    <span className="settings-hint">{t("settings.interface.appLanguageHint")}</span>
                  </label>
                  <div className="settings-dropdown" ref={appLanguageDropdownRef}>
                    <button
                      type="button"
                      className={`settings-dropdown-selected ${appLanguageDropdownOpen ? "open" : ""}`}
                      onClick={() => setAppLanguageDropdownOpen((open) => !open)}
                    >
                      <span className="settings-dropdown-selected-name">{selectedAppLanguageName}</span>
                      <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" className={`settings-dropdown-chevron ${appLanguageDropdownOpen ? "flipped" : ""}`}>
                        <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                      </svg>
                    </button>
                    {appLanguageDropdownOpen && (
                      <div className="settings-dropdown-menu">
                        {appLanguageOptions.map((option) => (
                          <button
                            key={option.value}
                            type="button"
                            className={`settings-dropdown-item ${locale === option.value ? "active" : ""}`}
                            onClick={() => handleAppLanguageChange(option.value)}
                          >
                            <span>{option.label}</span>
                            {locale === option.value && <Check size={14} className="settings-dropdown-check" />}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </div>

                <div className="settings-row">
                  <label className="settings-label">
                    {t("settings.interface.accentColor")}
                    <span className="settings-hint">{t("settings.interface.accentColorHint")}</span>
                  </label>
                  <div className="settings-dropdown" ref={accentColorDropdownRef}>
                    <button
                      type="button"
                      className={`settings-dropdown-selected ${accentColorDropdownOpen ? "open" : ""}`}
                      onClick={() => setAccentColorDropdownOpen((open) => !open)}
                    >
                      <span className="settings-dropdown-selected-name" style={{ display: "inline-flex", alignItems: "center", gap: 10 }}>
                        <span
                          aria-hidden="true"
                          style={{
                            width: 12,
                            height: 12,
                            borderRadius: 9999,
                            background: selectedAccentColor.hex,
                            boxShadow: `0 0 0 1px color-mix(in srgb, ${selectedAccentColor.hex} 48%, rgba(255, 255, 255, 0.22))`,
                          }}
                        />
                        {t(selectedAccentColor.labelKey)}
                      </span>
                      <svg viewBox="0 0 16 16" width="14" height="14" fill="currentColor" className={`settings-dropdown-chevron ${accentColorDropdownOpen ? "flipped" : ""}`}>
                        <path d="M4.47 5.97a.75.75 0 0 1 1.06 0L8 8.44l2.47-2.47a.75.75 0 1 1 1.06 1.06l-3 3a.75.75 0 0 1-1.06 0l-3-3a.75.75 0 0 1 0-1.06Z" />
                      </svg>
                    </button>
                    {accentColorDropdownOpen && (
                      <div className="settings-dropdown-menu">
                        {accentColorOptions.map((option) => (
                          <button
                            key={option.value}
                            type="button"
                            className={`settings-dropdown-item ${settings.appAccentColor === option.value ? "active" : ""}`}
                            onClick={() => {
                              handleChange("appAccentColor", option.value);
                              setAccentColorDropdownOpen(false);
                            }}
                          >
                            <span style={{ display: "inline-flex", alignItems: "center", gap: 10, flex: 1 }}>
                              <span
                                aria-hidden="true"
                                style={{
                                  width: 20,
                                  height: 20,
                                  borderRadius: 9999,
                                  flexShrink: 0,
                                  flex: "0 0 20px",
                                  background: option.hex,
                                  boxShadow: `0 0 0 1px color-mix(in srgb, ${option.hex} 48%, rgba(255, 255, 255, 0.22))`,
                                }}
                              />
                              <span style={{ flex: 1, overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                                {t(option.labelKey)}
                              </span>
                            </span>
                            {settings.appAccentColor === option.value && <Check size={14} className="settings-dropdown-check" />}
                          </button>
                        ))}
                      </div>
                    )}
                  </div>
                </div>

                {/* 4-toggle grid */}
                <div className="settings-toggle-grid">
                  <div className="settings-row">
                    <label className="settings-label">
                      {t("settings.interface.hideStreamOverlayButtons")}
                      <span className="settings-hint">{t("settings.interface.hideStreamOverlayButtonsHint")}</span>
                    </label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.hideStreamButtons}
                        onChange={(e) => handleChange("hideStreamButtons", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>

                  <div className="settings-row">
                    <label className="settings-label">
                      {t("settings.interface.showStatsOnStreamLaunch")}
                      <span className="settings-hint">{t("settings.interface.showStatsOnStreamLaunchHint")}</span>
                    </label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.showStatsOnLaunch}
                        onChange={(e) => handleChange("showStatsOnLaunch", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>

                  <div className="settings-row">
                    <label className="settings-label">
                      {t("settings.interface.hideServerSelector")}
                      <span className="settings-hint">{t("settings.interface.hideServerSelectorHint")}</span>
                    </label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.hideServerSelector}
                        onChange={(e) => handleChange("hideServerSelector", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>

                  <div className="settings-row">
                    <label className="settings-label">
                      {t("settings.interface.showAntiAfkIndicator")}
                      <span className="settings-hint">{t("settings.interface.showAntiAfkIndicatorHint")}</span>
                    </label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.showAntiAfkIndicator}
                        onChange={(e) => handleChange("showAntiAfkIndicator", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>

                  <div className="settings-row">
                    <label className="settings-label">
                      {t("settings.interface.autoFullScreen")}
                      <span className="settings-hint">{t("settings.interface.autoFullScreenHint")}</span>
                    </label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.autoFullScreen}
                        onChange={(e) => handleChange("autoFullScreen", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>

                  <div className="settings-row">
                    <label className="settings-label">
                      {t("settings.interface.controllerMode")}
                      <span className="settings-hint">{t("settings.interface.controllerModeHint")}</span>
                    </label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.controllerMode}
                        onChange={(e) => handleChange("controllerMode", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>

                  <div className="settings-row">
                    <label className="settings-label">
                      {t("settings.interface.escapeExitsFullscreen")}
                      <span className="settings-hint">{t("settings.interface.escapeExitsFullscreenHint")}</span>
                    </label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={Boolean(settings.allowEscapeToExitFullscreen)}
                        onChange={(e) => handleChange("allowEscapeToExitFullscreen", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>

                  <div className="settings-row">
                    <label className="settings-label">
                      {t("settings.interface.discordRichPresence")}
                      <span className="settings-hint">{t("settings.interface.discordRichPresenceHint")}</span>
                    </label>
                    <label className="settings-toggle">
                      <input
                        type="checkbox"
                        checked={settings.discordRichPresence}
                        onChange={(e) => handleChange("discordRichPresence", e.target.checked)}
                      />
                      <span className="settings-toggle-track" />
                    </label>
                  </div>
                </div>

                <div className="settings-row settings-row--column">
                  <div className="settings-row-top">
                    <label className="settings-label">{t("settings.interface.posterSize")}</label>
                    <span className="settings-value-badge">{posterSizePercent}%</span>
                  </div>
                  <input
                    type="range"
                    className="settings-slider"
                    min={POSTER_SIZE_MIN}
                    max={POSTER_SIZE_MAX}
                    step={POSTER_SIZE_STEP}
                    value={posterSizePercent}
                    onChange={(e) => handleChange("posterSizeScale", Number(e.target.value) / 100)}
                  />
                  <span className="settings-subtle-hint">{t("settings.interface.posterSizeHint")}</span>
                </div>

                <div className="settings-row">
                  <label className="settings-label">
                    {t("settings.interface.showSessionTimeRemainingInStatsOverlay")}
                    <span className="settings-hint">{t("settings.interface.showSessionTimeRemainingInStatsOverlayHint")}</span>
                  </label>
                  <label className="settings-toggle">
                    <input
                      type="checkbox"
                      checked={settings.showSessionTimeRemainingInStatsOverlay}
                      onChange={(e) => handleChange("showSessionTimeRemainingInStatsOverlay", e.target.checked)}
                    />
                    <span className="settings-toggle-track" />
                  </label>
                </div>

                {/* Session Counter */}
                <div className="settings-row">
                  <label className="settings-label">
                    {t("settings.interface.sessionElapsedCounter")}
                    <span className="settings-hint">{t("settings.interface.sessionElapsedCounterHint")}</span>
                  </label>
                  <label className="settings-toggle">
                    <input
                      type="checkbox"
                      checked={settings.sessionCounterEnabled}
                      onChange={(e) => handleChange("sessionCounterEnabled", e.target.checked)}
                    />
                    <span className="settings-toggle-track" />
                  </label>
                </div>

                <div className="settings-row settings-row--column">
                  <div className="settings-row-top">
                    <label className="settings-label">{t("settings.interface.sessionTimerReappear")}</label>
                    <span className="settings-value-badge">
                      {!settings.sessionCounterEnabled
                        ? t("app.status.disabled")
                        : settings.sessionClockShowEveryMinutes === 0
                          ? t("settings.interface.off")
                          : t("settings.interface.everyMinutes", { count: settings.sessionClockShowEveryMinutes })}
                    </span>
                  </div>
                  <input
                    type="range"
                    className="settings-slider"
                    min={0}
                    max={120}
                    step={5}
                    value={settings.sessionClockShowEveryMinutes}
                    onChange={(e) => handleChange("sessionClockShowEveryMinutes", parseInt(e.target.value, 10))}
                    disabled={!settings.sessionCounterEnabled}
                  />
                  <span className="settings-subtle-hint">{t("settings.interface.sessionTimerReappearHint")}</span>
                </div>

                <div className="settings-row settings-row--column">
                  <div className="settings-row-top">
                    <label className="settings-label">{t("settings.interface.sessionTimerVisibleTime")}</label>
                    <span className="settings-value-badge">
                      {settings.sessionCounterEnabled
                        ? t("app.units.seconds", { value: settings.sessionClockShowDurationSeconds })
                        : t("app.status.disabled")}
                    </span>
                  </div>
                  <input
                    type="range"
                    className="settings-slider"
                    min={5}
                    max={120}
                    step={5}
                    value={settings.sessionClockShowDurationSeconds}
                    onChange={(e) => handleChange("sessionClockShowDurationSeconds", parseInt(e.target.value, 10))}
                    disabled={!settings.sessionCounterEnabled}
                  />
                  <span className="settings-subtle-hint">{t("settings.interface.sessionTimerVisibleTimeHint")}</span>
                </div>

                <div className="settings-row settings-row--column">
                  <span className="settings-subtle-hint">{t("settings.interface.sessionTimerDisabledHint")}</span>
                </div>
              </div>
            </section>

          </>
        )}

        {showAbout && (
          <section className="settings-section">
            {showAll && <div className="settings-section-context">{t("settings.sections.about")}</div>}
            <div className="settings-section-header">
              <h2>{t("settings.sections.about")}</h2>
            </div>
            <div className="settings-rows">
              <div className="settings-row">
                <label className="settings-label settings-label--wrap">
                  <span className="settings-label-title">
                    {t("settings.about.applicationUpdates")}
                    <span className={`settings-inline-badge settings-inline-badge--updater settings-inline-badge--updater-${updaterState.status}`}>
                      {updaterBadgeLabel}
                    </span>
                  </span>
                  <span className="settings-hint">
                    {t("settings.about.version", { version: updaterState.currentDisplayVersion ?? updaterState.currentVersion })} · {settings.autoCheckForUpdates
                      ? t("settings.about.backgroundChecksOn")
                      : t("settings.about.backgroundChecksOff")}
                  </span>
                  {updaterState.message ? (
                    <span className="settings-hint settings-hint--updater-message">{updaterState.message}</span>
                  ) : null}
                  {updaterLastCheckedLabel ? (
                    <span className="settings-hint">{t("settings.about.lastChecked", { value: updaterLastCheckedLabel })}</span>
                  ) : null}
                  {updaterState.availableVersion && updaterState.status !== "downloaded" ? (
                    <span className="settings-hint">{t("settings.about.availableVersion", { version: updaterState.availableVersion })}</span>
                  ) : null}
                  {updaterState.downloadedVersion ? (
                    <span className="settings-hint">{t("settings.about.downloadedVersion", { version: updaterState.downloadedVersion })}</span>
                  ) : null}
                  {updaterState.status === "downloading" && updaterState.progress ? (
                    <span className="settings-hint">
                      {t("settings.about.downloadProgress", { percent: updaterProgressPercent })}{updaterProgressLabel ? ` · ${updaterProgressLabel}` : ""}{updaterDownloadRateLabel ? ` · ${updaterDownloadRateLabel}` : ""}
                    </span>
                  ) : null}
                </label>
                <div className="settings-updater-actions">
                  <button
                    type="button"
                    className="settings-export-logs-btn"
                    disabled={!updaterState.canCheck}
                    onClick={() => {
                      void window.openNow.checkForUpdates().catch((error) => {
                        console.error("[Settings] Failed to trigger update check:", error);
                      });
                    }}
                  >
                    {updaterState.status === "checking" ? <Loader size={16} className="spin" /> : <RefreshCcw size={16} />}
                    {t("settings.about.checkForUpdates")}
                  </button>
                  {updaterState.status === "available" ? (
                    <button
                      type="button"
                      className="settings-export-logs-btn"
                      disabled={!updaterState.canDownload}
                      onClick={() => {
                        void window.openNow.downloadUpdate().catch((error) => {
                          console.error("[Settings] Failed to download update:", error);
                        });
                      }}
                    >
                      <Download size={16} />
                      {t("settings.about.downloadUpdate")}
                    </button>
                  ) : null}
                  {updaterState.status === "downloaded" ? (
                    <button
                      type="button"
                      className="settings-save-btn settings-save-btn--compact"
                      disabled={!updaterState.canInstall}
                      onClick={() => {
                        void window.openNow.installUpdateAndRestart().catch((error) => {
                          console.error("[Settings] Failed to install update:", error);
                        });
                      }}
                    >
                      <RefreshCcw size={16} />
                      {t("settings.about.restartToInstall")}
                    </button>
                  ) : null}
                </div>
              </div>

              <div className="settings-row">
                <label className="settings-label settings-label--wrap">
                  {t("settings.about.automaticallyCheckForUpdates")}
                  <span className="settings-hint">
                    {t("settings.about.automaticallyCheckForUpdatesOnHint")}
                  </span>
                  <span className="settings-hint">
                    {t("settings.about.automaticallyCheckForUpdatesOffHint")}
                  </span>
                </label>
                <label className="settings-toggle">
                  <input
                    type="checkbox"
                    checked={settings.autoCheckForUpdates}
                    onChange={(e) => handleChange("autoCheckForUpdates", e.target.checked)}
                  />
                  <span className="settings-toggle-track" />
                </label>
              </div>

              {updaterState.status === "downloading" && updaterState.progress ? (
                <div className="settings-row settings-row--column">
                  <div className="settings-updater-progress">
                    <div className="settings-updater-progress-bar" style={{ width: `${updaterProgressPercent}%` }} />
                  </div>
                </div>
              ) : null}

              <div className="settings-row">
                <label className="settings-label">
                  {t("settings.about.exportLogs")}
                  <span className="settings-hint">{t("settings.about.exportLogsHint")}</span>
                </label>
                <button
                  type="button"
                  className="settings-export-logs-btn"
                  onClick={async () => {
                    try {
                      const logs = await window.openNow.exportLogs("text");
                      const blob = new Blob([logs], { type: "text/plain" });
                      const url = URL.createObjectURL(blob);
                      const a = document.createElement("a");
                      a.href = url;
                      a.download = `opennow-logs-${new Date().toISOString().replace(/[:.]/g, "-")}.txt`;
                      document.body.appendChild(a);
                      a.click();
                      document.body.removeChild(a);
                      URL.revokeObjectURL(url);
                    } catch (err) {
                      console.error("[Settings] Failed to export logs:", err);
                      alert(t("settings.about.exportLogsFailed"));
                    }
                  }}
                >
                  <FileDown size={16} />
                  {t("settings.about.exportLogs")}
                </button>
              </div>

              <div className="settings-row">
                <label className="settings-label">
                  {t("settings.about.deleteCache")}
                  <span className="settings-hint">{t("settings.about.deleteCacheHint")}</span>
                </label>
                <button
                  type="button"
                  className="settings-delete-cache-btn"
                  onClick={async () => {
                    if (!window.confirm(t("settings.about.deleteCacheConfirm"))) {
                      return;
                    }
                    try {
                      await window.openNow.deleteCache();
                      alert(t("settings.about.cacheCleared"));
                    } catch (err) {
                      console.error("[Settings] Failed to delete cache:", err);
                      alert(t("settings.about.deleteCacheFailed"));
                    }
                  }}
                >
	                  <Trash2 size={16} />
	                  {t("settings.about.deleteCache")}
	                </button>
              </div>
            </div>
          </section>
                )}
              </>
            )}
          </>
        )}
      </div>
      </div>
    </div>
  );
}
