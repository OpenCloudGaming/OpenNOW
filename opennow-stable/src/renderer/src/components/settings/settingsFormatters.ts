import type {
  AspectRatio,
  EntitledResolution,
  AppUpdaterState,
  NativeStreamerStatus,
  NativeVideoBackendCapability,
  NativeVideoBackendPreference,
  VideoCodec,
  ColorQuality,
  VideoAccelerationPreference,
  MicrophoneMode,
  MicrophonePermissionResult,
  GameLanguage,
  Settings,
  ShortcutSettingKey,
} from "@shared/gfn";
import {
  createPlatformShortcutDefaults,
  resolveRuntimePlatform,
  SHORTCUT_SETTING_KEYS,
  USER_FACING_VIDEO_CODEC_OPTIONS,
} from "@shared/gfn";
import { getAccentColorOptions } from "../../lib/uiCustomization";
import { normalizeShortcut } from "../../shortcuts";

export const POSTER_SIZE_MIN = 75;
export const POSTER_SIZE_MAX = 135;
export const POSTER_SIZE_STEP = 5;

export const codecOptions: VideoCodec[] = [...USER_FACING_VIDEO_CODEC_OPTIONS];

export const accelerationOptions: { value: VideoAccelerationPreference; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "hardware", label: "Hardware" },
  { value: "software", label: "Software (CPU)" },
];

export const allColorQualityOptions: { value: ColorQuality; label: string; description: string }[] = [
  { value: "8bit_420", label: "8-bit 4:2:0", description: "Most compatible" },
  { value: "8bit_444", label: "8-bit 4:4:4", description: "Sharper chroma" },
  { value: "10bit_420", label: "10-bit 4:2:0", description: "Higher bit depth" },
  { value: "10bit_444", label: "10-bit 4:4:4", description: "Highest chroma and bit depth" },
];

export const colorQualityOptions: { value: ColorQuality; label: string; description: string }[] = [...allColorQualityOptions];

export const nativeVideoBackendOptions: { value: NativeVideoBackendPreference; label: string; description: string }[] = [
  { value: "auto", label: "Auto", description: "Pick the default native path for the session" },
  { value: "d3d12", label: "DirectX 12", description: "Use the D3D12 decoder and renderer" },
  { value: "d3d11", label: "DirectX 11", description: "Use the D3D11 decoder and renderer" },
  { value: "nvdec", label: "NVIDIA NVDEC", description: "Use NVIDIA's native Linux hardware decoders" },
  { value: "vaapi", label: "VAAPI", description: "Use Linux VA-API hardware decoding" },
  { value: "v4l2", label: "V4L2", description: "Use Linux V4L2 hardware decoding, including Raspberry Pi" },
  { value: "vulkan", label: "Vulkan", description: "Use Vulkan presentation where supported; Windows internal mode uses a compatible D3D presentation path" },
  { value: "software", label: "Software", description: "Decode on the CPU with libav" },
];

export const APP_LANGUAGE_LABELS: Record<string, string> = {
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

export const accentColorOptions = getAccentColorOptions();

export function getAppLanguageLabel(locale: string): string {
  return APP_LANGUAGE_LABELS[locale] ?? locale.toUpperCase();
}

export function formatNativeVideoBackendName(backend: string | undefined): string {
  switch (backend) {
    case "d3d12":
      return "D3D12";
    case "d3d11":
      return "D3D11";
    case "videotoolbox":
      return "VideoToolbox";
    case "nvdec":
      return "NVIDIA NVDEC";
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

export function formatNativeVideoCodec(codec: string): string {
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

export function getAvailableNativeCodecLabels(backend: NativeVideoBackendCapability | undefined): string[] {
  return backend?.codecs
    .filter((codec) => codec.available)
    .map((codec) => formatNativeVideoCodec(codec.codec)) ?? [];
}

export function formatNativeRuntimeLabel(status: NativeStreamerStatus | null): string {
  switch (status?.runtime.source) {
    case "self-contained":
      return status.available ? "Native Runtime Ready" : "Native Runtime Found";
    case "missing":
      return "Runtime Missing";
    default:
      return "Runtime Unknown";
  }
}

export function getNativeRuntimeBadgeClass(status: NativeStreamerStatus | null): string {
  if (status?.runtime.source === "self-contained" && status.available) return "settings-inline-badge--codec-gpu";
  return "settings-inline-badge--updater-error";
}

/* ── Static fallbacks (used when MES API is unavailable) ─────────── */

export interface ResolutionPreset {
  value: string;
  label: string;
}

export interface FpsPreset {
  value: number;
}

export function inferAspectRatioFromResolution(resolution: string): AspectRatio {
  const parts = resolution.split("x");
  const width = parseInt(parts[0] ?? "", 10);
  const height = parseInt(parts[1] ?? "", 10);
  if (!Number.isFinite(width) || !Number.isFinite(height) || height === 0) {
    return "16:9";
  }

  const ratio = width / height;
  if (Math.abs(ratio - 32 / 9) < 0.08) return "32:9";
  if (Math.abs(ratio - 21 / 9) < 0.08) return "21:9";
  if (Math.abs(ratio - 16 / 10) < 0.05) return "16:10";
  return "16:9";
}

export const STATIC_RESOLUTION_PRESETS: ResolutionPreset[] = [
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

export const STATIC_FPS_PRESETS: FpsPreset[] = [
  { value: 30 },
  { value: 60 },
  { value: 90 },
  { value: 120 },
  { value: 144 },
  { value: 165 },
  { value: 240 },
  { value: 360 },
];

const runtimePlatform = resolveRuntimePlatform(navigator.platform);
const platformShortcutDefaults = createPlatformShortcutDefaults(runtimePlatform);

export const isMac = runtimePlatform === "darwin";
export const shortcutExamples = "Examples: Ctrl+N, Ctrl+Shift+Q, Ctrl+Shift+K";
export const shortcutDefaults = platformShortcutDefaults.bindings;

/** Canonical shortcut for toggling the stream sidebar (must match StreamView key handler). */
export const SIDEBAR_TOGGLE_SHORTCUT_RAW = platformShortcutDefaults.sidebarToggle;
export const SIDEBAR_TOGGLE_SHORTCUT_ALIASES = platformShortcutDefaults.sidebarToggleAliases;
export { SHORTCUT_SETTING_KEYS };
export type { ShortcutSettingKey };

export function extractRemoteInvokeErrorMessage(error: unknown, fallback: string): string {
  if (!(error instanceof Error) || !error.message.trim()) {
    return fallback;
  }

  const ipcMatch = error.message.match(/^Error invoking remote method '[^']+': (?:(?:Error|TypeError|RangeError): )?([\s\S]+)$/);
  return ipcMatch?.[1]?.trim() || error.message.trim();
}

export function getShortcutConflictMessage(
  editingKey: ShortcutSettingKey,
  candidateCanonical: string,
  currentSettings: Settings,
): string | null {
  const sidebarShortcuts = SIDEBAR_TOGGLE_SHORTCUT_ALIASES
    .map((value) => normalizeShortcut(value))
    .filter((parsed) => parsed.valid)
    .map((parsed) => parsed.canonical);
  if (sidebarShortcuts.includes(candidateCanonical)) {
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

export const microphoneModeOptions: Array<{ value: MicrophoneMode; label: string }> = [
  { value: "disabled", label: "Disabled" },
  { value: "push-to-talk", label: "Push-to-Talk" },
  { value: "voice-activity", label: "Voice Activity" },
];

export function getMicrophonePermissionError(result: MicrophonePermissionResult): string {
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

export const gameLanguageOptions: Array<{ value: GameLanguage; label: string }> = [
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

export const ASPECT_RATIO_ORDER = [
  "16:9 Standard",
  "16:10 Widescreen",
  "21:9 Ultrawide",
  "32:9 Super Ultrawide",
  "4:3 Legacy",
  "Other",
] as const;

export function classifyAspectRatio(width: number, height: number): string {
  const ratio = width / height;
  if (Math.abs(ratio - 16 / 9) < 0.05) return "16:9 Standard";
  if (Math.abs(ratio - 16 / 10) < 0.05) return "16:10 Widescreen";
  if (Math.abs(ratio - 21 / 9) < 0.05) return "21:9 Ultrawide";
  if (Math.abs(ratio - 32 / 9) < 0.05) return "32:9 Super Ultrawide";
  if (Math.abs(ratio - 4 / 3) < 0.05) return "4:3 Legacy";
  if (ratio > 2 && ratio < 3.5) return "21:9 Ultrawide";
  return "Other";
}

export function friendlyResolutionName(width: number, height: number): string {
  if (width === 1280 && height === 720) return "720p (HD)";
  if (width === 1920 && height === 1080) return "1080p (FHD)";
  if (width === 2560 && height === 1440) return "1440p (QHD)";
  if (width === 3840 && height === 2160) return "4K (UHD)";
  if (width === 2560 && height === 1080) return "2560x1080 (UW)";
  if (width === 3440 && height === 1440) return "3440x1440 (UW)";
  if (width === 5120 && height === 1440) return "5120x1440 (SUW)";
  return `${width}x${height}`;
}

export interface ResolutionGroup {
  category: string;
  resolutions: { width: number; height: number; value: string; label: string }[];
}

export function groupResolutions(entitled: EntitledResolution[]): ResolutionGroup[] {
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

export function getFpsForResolution(entitled: EntitledResolution[], resolution: string): number[] {
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

export const ENTITLED_RESOLUTIONS_STORAGE_KEY = "opennow.entitled-resolutions.v3";

export interface EntitledResolutionsCache {
  userId: string;
  membershipTier: string;
  /** MES resolution catalogs differ for Steam Deck vs desktop identity. */
  identifyAsSteamDeck: boolean;
  entitledResolutions: EntitledResolution[];
}

export function loadCachedEntitledResolutions(
  identifyAsSteamDeck = false,
): EntitledResolutionsCache | null {
  try {
    const raw = window.sessionStorage.getItem(ENTITLED_RESOLUTIONS_STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<EntitledResolutionsCache>;
    if (!parsed || typeof parsed.userId !== "string" || !Array.isArray(parsed.entitledResolutions)) {
      return null;
    }
    const cachedIdentifyAsSteamDeck = parsed.identifyAsSteamDeck === true;
    if (cachedIdentifyAsSteamDeck !== identifyAsSteamDeck) {
      return null;
    }
    return {
      userId: parsed.userId,
      membershipTier: typeof parsed.membershipTier === "string" ? parsed.membershipTier : "",
      identifyAsSteamDeck: cachedIdentifyAsSteamDeck,
      entitledResolutions: parsed.entitledResolutions,
    };
  } catch {
    return null;
  }
}

export function formatBytes(value: number): string {
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

export function formatStorageGb(value: number | undefined): string | null {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return null;
  }

  const rounded = Math.round(value * 10) / 10;
  const display = Number.isInteger(rounded) ? rounded.toFixed(0) : rounded.toFixed(1);
  return `${display} GB`;
}

export function formatUpdaterTimestamp(value?: number): string | null {
  if (!value) return null;
  try {
    return new Date(value).toLocaleString();
  } catch {
    return null;
  }
}

export function getUpdaterBadgeLabel(state: AppUpdaterState): string {
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

export function formatGameAccountSyncDate(value: string | undefined): string | null {
  if (!value) return null;
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return null;
  return date.toLocaleString();
}

export function saveCachedEntitledResolutions(cache: EntitledResolutionsCache): void {
  try {
    window.sessionStorage.setItem(ENTITLED_RESOLUTIONS_STORAGE_KEY, JSON.stringify(cache));
  } catch {
    // Ignore storage failures
  }
}
