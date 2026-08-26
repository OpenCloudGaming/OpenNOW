import type {
  CodecPreference,
  ColorQuality,
  FallbackCodecPreference,
  NativeTransitionDiagnostics,
  StreamClientMode,
  VideoAccelerationPreference,
} from "./stream";
import type {
  NativeStreamerFeatureMode,
  NativeVideoBackendPreference,
  StreamTransportMode,
} from "./nativeStreamer";
import { DEFAULT_KEYBOARD_LAYOUT, type GameLanguage, type KeyboardLayout } from "./keyboard";
import { DEFAULT_VIDEO_SHADER_SETTINGS, type VideoShaderSettings } from "./videoShader";
import {
  DEFAULT_FRAME_INTERPOLATION_SETTINGS,
  type FrameInterpolationSettings,
} from "./frameInterpolation";
import type { UpdateChannel } from "./updater";
import { normalizeFallbackCodecPreference, normalizeStreamPreferences } from "./stream";

export type AppAccentColor = "green" | "blue" | "violet" | "amber" | "rose";
export type AppTheme = "light" | "dark" | "auto";
export type MicrophoneMode = "disabled" | "push-to-talk" | "voice-activity";
export type StatsOverlayPosition = "bottom-left" | "bottom-right" | "top-left" | "top-right";
export type AspectRatio = "16:9" | "16:10" | "21:9" | "32:9";
export type ErrorReportingConsent = "unset" | "granted" | "denied";
export const RECORDING_RESOLUTION_OPTIONS = ["720p", "1080p", "1440p"] as const;
export type RecordingResolution = typeof RECORDING_RESOLUTION_OPTIONS[number];
export const RECORDING_FPS_OPTIONS = [30, 60] as const;
export type RecordingFps = typeof RECORDING_FPS_OPTIONS[number];
export const DEFAULT_RECORDING_RESOLUTION: RecordingResolution = "720p";
export const DEFAULT_RECORDING_FPS: RecordingFps = 30;
export const DEFAULT_CUSTOM_RECORDING_BITRATE_MBPS = 8;
export const MAX_RECORDING_BITRATE_MBPS = 12;
export type RuntimePlatform =
  | "aix"
  | "android"
  | "cygwin"
  | "darwin"
  | "freebsd"
  | "haiku"
  | "linux"
  | "netbsd"
  | "openbsd"
  | "sunos"
  | "win32"
  | "unknown";

export type MacOsMicrophoneAccessStatus = "not-determined" | "granted" | "denied" | "restricted" | "unknown";

export interface MicrophonePermissionResult {
  platform: RuntimePlatform;
  isMacOs: boolean;
  status: MacOsMicrophoneAccessStatus | "not-applicable";
  granted: boolean;
  canRequest: boolean;
  shouldUseBrowserApi: boolean;
}

export interface Settings {
  resolution: string;
  aspectRatio: AspectRatio;
  posterSizeScale: number;
  fps: number;
  maxBitrateMbps: number;
  /** Recording video bitrate in Mbps; null means let MediaRecorder choose automatically */
  recordingBitrateMbps: number | null;
  recordingResolution: RecordingResolution;
  recordingFps: RecordingFps;
  streamClientMode: StreamClientMode;
  nativeVideoBackend: NativeVideoBackendPreference;
  nativeStreamerExecutablePath: string;
  nativeCloudGsyncMode: NativeStreamerFeatureMode;
  nativeD3dFullscreenMode: NativeStreamerFeatureMode;
  nativeExternalRenderer: boolean;
  transportMode: StreamTransportMode;
  showNativeStreamerStats: boolean;
  codec: CodecPreference;
  fallbackCodec: FallbackCodecPreference;
  decoderPreference: VideoAccelerationPreference;
  encoderPreference: VideoAccelerationPreference;
  colorQuality: ColorQuality;
  region: string;
  sessionProxyEnabled: boolean;
  sessionProxyUrl: string;
  clipboardPaste: boolean;
  /** Enable experimental gyroscope controller input mapping */
  enableGyroscopeControls: boolean;
  /** macOS-only workaround that restores Chromium's older HID path for Steam Controller compatibility */
  steamControllerCompatibilityMode: boolean;
  /** Use the WebRTC cursor_channel overlay instead of leaving cursor rendering to the stream. */
  nativeCursorOverlay: boolean;
  mouseSensitivity: number;
  mouseAcceleration: number;
  shortcutToggleStats: string;
  shortcutTogglePointerLock: string;
  shortcutToggleFullscreen: string;
  shortcutStopStream: string;
  shortcutToggleAntiAfk: string;
  shortcutToggleMicrophone: string;
  shortcutScreenshot: string;
  shortcutToggleRecording: string;
  microphoneMode: MicrophoneMode;
  microphoneDeviceId: string;
  hideStreamButtons: boolean;
  showAntiAfkIndicator: boolean;
  antiAfkReminderEveryMinutes: number;
  antiAfkReminderDurationSeconds: number;
  showStatsOnLaunch: boolean;
  statsOverlayPosition: StatsOverlayPosition;
  /** Skip the free-tier queue server selection modal and launch with default routing */
  hideServerSelector: boolean;
  /** Desktop UI accent preset */
  appAccentColor: AppAccentColor;
  /** UI Theme */
  appTheme: AppTheme;
  /** Use translucent overlays for settings and navbars */
  translucentUI: boolean;
  /** Use the large-screen controller-oriented shell and library layout */
  controllerMode: boolean;
  /** Permanently suppress the controller-detected suggestion after the user declines it */
  controllerModePromptDismissed: boolean;
  /** Request GeForce NOW's gamepad-friendly app launch mode for new sessions */
  launchInConsoleMode: boolean;
  /** Show the "Who's playing?" profile picker when console mode starts */
  consoleProfilePickerOnLaunch: boolean;
  autoFullScreen: boolean;
  favoriteGameIds: string[];
  sessionCounterEnabled: boolean;
  /** Show an evidence-based quality summary after each completed stream. */
  showSessionReport: boolean;
  /** Also show the session-limit countdown in the stats overlay while streaming */
  showSessionTimeRemainingInStatsOverlay: boolean;
  sessionClockShowEveryMinutes: number;
  sessionClockShowDurationSeconds: number;
  windowWidth: number;
  windowHeight: number;
  /** Keyboard layout for mapping physical keys inside the remote session */
  keyboardLayout: KeyboardLayout;
  /** In-game language setting (sent to GFN servers via languageCode parameter) */
  gameLanguage: GameLanguage;
  /** User opt-in for NVIDIA's per-game in-game graphics/settings persistence. */
  enablePersistingInGameSettings: boolean;
  /** Experimental request for Low Latency, Low Loss, Scalable throughput on new sessions */
  enableL4S: boolean;
  /**
   * Advertise OpenNOW as the official Steam Deck GFN client via nv-device-* headers
   * and clientPlatformName (does not switch OAuth client ID).
   */
  identifyAsSteamDeck: boolean;
  /** Request Cloud G-Sync / Variable Refresh Rate on new sessions */
  enableCloudGsync: boolean;
  /** Hidden diagnostics for native transition recovery and 240 FPS server-side stream changes */
  nativeTransitionDiagnostics?: NativeTransitionDiagnostics;
  /** Show the currently streaming game as Discord Rich Presence activity */
  discordRichPresence: boolean;
  /** Automatically check GitHub Releases for app updates in the background */
  autoCheckForUpdates: boolean;
  /** Release channel used for application updates */
  updateChannel: UpdateChannel;
  /** When true, pressing Escape will exit fullscreen; when false Escape is sent to the game while pointer-locked */
  allowEscapeToExitFullscreen?: boolean;
  /** Last version for which the release highlights modal was acknowledged (empty = never) */
  lastSeenReleaseHighlightsVersion: string;
  /** Client-side GPU post-processing shaders applied to the stream (web client mode) */
  videoShader: VideoShaderSettings;
  /**
   * Experimental client-side neural frame interpolation (Framegen WebGPU runtime).
   * Web client mode only; adds display latency in exchange for smoother motion.
   */
  frameInterpolation: FrameInterpolationSettings;
  /**
   * First-run consent for anonymous error reporting.
   * `"unset"` shows the one-time prompt; only `"granted"` enables exception capture.
   */
  errorReportingConsent: ErrorReportingConsent;
  /** Anonymous install UUID used for opt-in telemetry and hashed bug-report abuse control. */
  telemetryInstallId: string;
}

export const SHORTCUT_SETTING_KEYS = [
  "shortcutToggleStats",
  "shortcutTogglePointerLock",
  "shortcutToggleFullscreen",
  "shortcutStopStream",
  "shortcutToggleAntiAfk",
  "shortcutToggleMicrophone",
  "shortcutScreenshot",
  "shortcutToggleRecording",
] as const satisfies readonly (keyof Settings)[];

export type ShortcutSettingKey = typeof SHORTCUT_SETTING_KEYS[number];
export type ShortcutSettings = Pick<Settings, ShortcutSettingKey>;

export const DEFAULT_SHORTCUT_SETTINGS: Readonly<ShortcutSettings> = Object.freeze({
  shortcutToggleStats: "Ctrl+N",
  shortcutTogglePointerLock: "F8",
  shortcutToggleFullscreen: "F10",
  shortcutStopStream: "Ctrl+Shift+Q",
  shortcutToggleAntiAfk: "Ctrl+Shift+K",
  shortcutToggleMicrophone: "Ctrl+Shift+M",
  shortcutScreenshot: "F11",
  shortcutToggleRecording: "F12",
});

export interface PlatformShortcutDefaults {
  bindings: ShortcutSettings;
  sidebarToggle: string;
  sidebarToggleAliases: string[];
}

export function resolveRuntimePlatform(platform: string): RuntimePlatform {
  const normalized = platform.trim().toLowerCase();
  const exactPlatforms: readonly RuntimePlatform[] = [
    "aix",
    "android",
    "cygwin",
    "darwin",
    "freebsd",
    "haiku",
    "linux",
    "netbsd",
    "openbsd",
    "sunos",
    "win32",
  ];
  if (exactPlatforms.includes(normalized as RuntimePlatform)) {
    return normalized as RuntimePlatform;
  }
  if (normalized.includes("mac")) return "darwin";
  if (normalized.includes("win")) return "win32";
  if (normalized.includes("linux")) return "linux";
  return "unknown";
}

export function normalizeRecordingResolution(raw: unknown): RecordingResolution {
  return RECORDING_RESOLUTION_OPTIONS.includes(raw as RecordingResolution)
    ? raw as RecordingResolution
    : DEFAULT_RECORDING_RESOLUTION;
}

export function normalizeRecordingFps(raw: unknown): RecordingFps {
  const value = Number(raw);
  if (!Number.isFinite(value)) {
    return DEFAULT_RECORDING_FPS;
  }
  return value > 45 ? 60 : 30;
}

export function normalizeRecordingBitrateMbps(raw: unknown): number | null {
  if (raw === null || raw === undefined) {
    return null;
  }
  const value = Number(raw);
  if (!Number.isFinite(value)) {
    return null;
  }
  return Math.max(1, Math.min(MAX_RECORDING_BITRATE_MBPS, Math.round(value)));
}

export function createPlatformShortcutDefaults(platform: string): PlatformShortcutDefaults {
  const isMacOs = resolveRuntimePlatform(platform) === "darwin";
  const sidebarToggle = isMacOs ? "Meta+G" : "Ctrl+G";
  return {
    bindings: { ...DEFAULT_SHORTCUT_SETTINGS },
    sidebarToggle,
    sidebarToggleAliases: isMacOs ? [sidebarToggle] : [sidebarToggle, "Ctrl+Shift+G"],
  };
}

export function createDefaultSettings(platform: string): Settings {
  const shortcuts = createPlatformShortcutDefaults(platform);
  return {
    resolution: "1920x1080",
    aspectRatio: "16:9",
    posterSizeScale: 1.05,
    fps: 60,
    maxBitrateMbps: 75,
    recordingBitrateMbps: null,
    recordingResolution: DEFAULT_RECORDING_RESOLUTION,
    recordingFps: DEFAULT_RECORDING_FPS,
    streamClientMode: "web",
    nativeVideoBackend: "auto",
    nativeStreamerExecutablePath: "",
    nativeCloudGsyncMode: "auto",
    nativeD3dFullscreenMode: "auto",
    nativeExternalRenderer: false,
    transportMode: "webrtc",
    showNativeStreamerStats: false,
    codec: DEFAULT_STREAM_PREFERENCES.codec,
    fallbackCodec: DEFAULT_STREAM_PREFERENCES.fallbackCodec,
    decoderPreference: "auto",
    encoderPreference: "auto",
    colorQuality: DEFAULT_STREAM_PREFERENCES.colorQuality,
    region: "",
    sessionProxyEnabled: false,
    sessionProxyUrl: "",
    clipboardPaste: false,
    enableGyroscopeControls: false,
    steamControllerCompatibilityMode: false,
    nativeCursorOverlay: true,
    mouseSensitivity: 1,
    mouseAcceleration: 1,
    ...shortcuts.bindings,
    microphoneMode: "disabled",
    microphoneDeviceId: "",
    hideStreamButtons: false,
    showAntiAfkIndicator: true,
    antiAfkReminderEveryMinutes: 15,
    antiAfkReminderDurationSeconds: 5,
    showStatsOnLaunch: false,
    statsOverlayPosition: "bottom-left",
    hideServerSelector: false,
    appAccentColor: "green",
    appTheme: "auto",
    translucentUI: false,
    controllerMode: false,
    controllerModePromptDismissed: false,
    launchInConsoleMode: false,
    consoleProfilePickerOnLaunch: true,
    autoFullScreen: false,
    favoriteGameIds: [],
    sessionCounterEnabled: false,
    showSessionReport: true,
    showSessionTimeRemainingInStatsOverlay: false,
    sessionClockShowEveryMinutes: 60,
    sessionClockShowDurationSeconds: 30,
    windowWidth: 1400,
    windowHeight: 900,
    keyboardLayout: DEFAULT_KEYBOARD_LAYOUT,
    gameLanguage: "en_US",
    enablePersistingInGameSettings: false,
    enableL4S: false,
    identifyAsSteamDeck: false,
    enableCloudGsync: false,
    nativeTransitionDiagnostics: undefined,
    discordRichPresence: false,
    autoCheckForUpdates: true,
    updateChannel: "stable",
    allowEscapeToExitFullscreen: false,
    lastSeenReleaseHighlightsVersion: "",
    videoShader: { ...DEFAULT_VIDEO_SHADER_SETTINGS },
    frameInterpolation: { ...DEFAULT_FRAME_INTERPOLATION_SETTINGS },
    errorReportingConsent: "unset",
    telemetryInstallId: "",
  };
}

export const DEFAULT_STREAM_PREFERENCES: Readonly<Pick<Settings, "codec" | "fallbackCodec" | "colorQuality">> = Object.freeze({
  codec: "auto",
  fallbackCodec: "auto",
  colorQuality: "8bit_420",
});

export function getDefaultStreamPreferences(): Pick<Settings, "codec" | "fallbackCodec" | "colorQuality"> {
  const normalized = normalizeStreamPreferences(
    DEFAULT_STREAM_PREFERENCES.codec,
    DEFAULT_STREAM_PREFERENCES.colorQuality,
  );
  return {
    codec: normalized.codec,
    fallbackCodec: normalizeFallbackCodecPreference(DEFAULT_STREAM_PREFERENCES.fallbackCodec),
    colorQuality: normalized.colorQuality,
  };
}
