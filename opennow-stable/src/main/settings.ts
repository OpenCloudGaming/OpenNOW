import { app } from "electron";
import { join } from "node:path";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import type {
  VideoCodec,
  ColorQuality,
  VideoAccelerationPreference,
  MicrophoneMode,
  GameLanguage,
  AspectRatio,
  KeyboardLayout,
  StreamClientMode,
  NativeStreamerBackendPreference,
  NativeVideoBackendPreference,
  NativeStreamerFeatureMode,
  NativeTransitionDiagnostics,
  AppAccentColor,
  AppTheme,
  VideoShaderSettings,
} from "@shared/gfn";
import {
  DEFAULT_KEYBOARD_LAYOUT,
  DEFAULT_VIDEO_SHADER_SETTINGS,
  getDefaultStreamPreferences,
  normalizeStreamClientModeForPlatform,
  normalizeStreamPreferences,
  normalizeVideoShaderSettings,
} from "@shared/gfn";

export interface Settings {
  /** Video resolution (e.g., "1920x1080") */
  resolution: string;
  /** Aspect ratio (16:9, 16:10, 21:9, 32:9) */
  aspectRatio: AspectRatio;
  /** Game poster size multiplier used by the renderer */
  posterSizeScale: number;
  /** Target FPS (30, 60, 120, etc.) */
  fps: number;
  /** Maximum bitrate in Mbps (cap at 150) */
  maxBitrateMbps: number;
  /** Recording video bitrate in Mbps (null = MediaRecorder auto, cap at 200) */
  recordingBitrateMbps: number | null;
  /** Stream client implementation to use for new sessions */
  streamClientMode: StreamClientMode;
  /** Native streamer backend preference for new native sessions */
  nativeStreamerBackend: NativeStreamerBackendPreference;
  /** Native GStreamer video backend preference for Windows DirectX paths */
  nativeVideoBackend: NativeVideoBackendPreference;
  /** Optional path to a custom native streamer executable */
  nativeStreamerExecutablePath: string;
  /** Native-only override for Cloud G-Sync / VRR display detection */
  nativeCloudGsyncMode: NativeStreamerFeatureMode;
  /** Native D3D sink fullscreen presentation override */
  nativeD3dFullscreenMode: NativeStreamerFeatureMode;
  /** Use the native GStreamer renderer window instead of Electron HWND embedding */
  nativeExternalRenderer: boolean;
  /** Show the native streamer's own stats overlay while native streaming */
  showNativeStreamerStats: boolean;
  /** Preferred video codec */
  codec: VideoCodec;
  /** Preferred video decode acceleration mode */
  decoderPreference: VideoAccelerationPreference;
  /** Preferred video encode acceleration mode */
  encoderPreference: VideoAccelerationPreference;
  /** Color quality (bit depth + chroma subsampling) */
  colorQuality: ColorQuality;
  /** Preferred region URL (empty = auto) */
  region: string;
  /** Enable the optional proxy for Nvidia games catalog, session creation, and queue polling */
  sessionProxyEnabled: boolean;
  /** Optional proxy used for Nvidia games catalog, session creation, and queue polling */
  sessionProxyUrl: string;
  /** Enable clipboard paste into stream */
  clipboardPaste: boolean;
  /** Enable experimental gyroscope controller input mapping */
  enableGyroscopeControls: boolean;
  /** macOS-only workaround that restores Chromium's older HID path for Steam Controller compatibility */
  steamControllerCompatibilityMode: boolean;
  /** Use the WebRTC cursor_channel overlay instead of leaving cursor rendering to the stream */
  nativeCursorOverlay: boolean;
  /** Mouse sensitivity multiplier */
  mouseSensitivity: number;
  /** Software mouse acceleration strength percentage (1-150) */
  mouseAcceleration: number;
  /** Toggle stats overlay shortcut */
  shortcutToggleStats: string;
  /** Toggle pointer lock shortcut */
  shortcutTogglePointerLock: string;
  /** Toggle fullscreen shortcut */
  shortcutToggleFullscreen: string;
  /** Stop stream shortcut */
  shortcutStopStream: string;
  /** Toggle anti-AFK shortcut */
  shortcutToggleAntiAfk: string;
  /** Toggle microphone shortcut */
  shortcutToggleMicrophone: string;
  /** Take screenshot shortcut */
  shortcutScreenshot: string;
  /** Toggle stream recording shortcut */
  shortcutToggleRecording: string;
  /** How often to re-show the session timer while streaming (0 = off) */
  sessionClockShowEveryMinutes: number;
  /** How long the session timer stays visible when it appears */
  sessionClockShowDurationSeconds: number;
  /** Microphone mode: disabled, push-to-talk, or voice-activity */
  microphoneMode: MicrophoneMode;
  /** Preferred microphone device ID (empty = default) */
  microphoneDeviceId: string;
  /** Hide stream buttons (mic/fullscreen/end-session) while streaming */
  hideStreamButtons: boolean;
  /** Show the Anti-AFK indicator badge while streaming */
  showAntiAfkIndicator: boolean;
  /** Show the stats overlay automatically when a stream launches */
  showStatsOnLaunch: boolean;
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
  /** Launch fullscreen with Controller Mode enabled, like GeForce NOW's TV mode */
  launchInConsoleMode: boolean;
  /** Automatically enter fullscreen when launching a stream */
  autoFullScreen: boolean;
  favoriteGameIds: string[];
  /** Enable the live elapsed session counter */
  sessionCounterEnabled: boolean;
  /** Also show the session-limit countdown in the stats overlay while streaming */
  showSessionTimeRemainingInStatsOverlay: boolean;
  /** Window width */
  windowWidth: number;
  /** Window height */
  windowHeight: number;
  /** Keyboard layout for mapping physical keys inside the remote session */
  keyboardLayout: KeyboardLayout;
  /** In-game language setting (sent to GFN servers via languageCode parameter) */
  gameLanguage: GameLanguage;
  /** User opt-in for NVIDIA's per-game in-game graphics/settings persistence */
  enablePersistingInGameSettings: boolean;
  /** Experimental request for Low Latency, Low Loss, Scalable throughput on new sessions */
  enableL4S: boolean;
  /** Request Cloud G-Sync / Variable Refresh Rate on new sessions */
  enableCloudGsync: boolean;
  /** Hidden diagnostics for native transition recovery and 240 FPS server-side stream changes */
  nativeTransitionDiagnostics?: NativeTransitionDiagnostics;
  /** Show the currently streaming game as Discord Rich Presence activity */
  discordRichPresence: boolean;
  /** Automatically check GitHub Releases for app updates in the background */
  autoCheckForUpdates: boolean;
  /** When true, pressing Escape will exit fullscreen; when false Escape is sent to the game while pointer-locked */
  allowEscapeToExitFullscreen?: boolean;
  /** Automatically select a server and rejoin after a free-tier session ends. */
  enableFastQueueJoin: boolean;
  /** Last version for which the release highlights modal was acknowledged (empty = never) */
  lastSeenReleaseHighlightsVersion: string;
  /** Client-side GPU post-processing shaders applied to the stream (web client mode) */
  videoShader: VideoShaderSettings;
}

const defaultStopShortcut = "Ctrl+Shift+Q";
const defaultAntiAfkShortcut = "Ctrl+Shift+K";
const defaultMicShortcut = "Ctrl+Shift+M";
const LEGACY_STOP_SHORTCUTS = new Set(["META+SHIFT+Q", "CMD+SHIFT+Q"]);
const LEGACY_ANTI_AFK_SHORTCUTS = new Set(["META+SHIFT+F10", "CMD+SHIFT+F10", "CTRL+SHIFT+F10"]);
const DEFAULT_STREAM_PREFERENCES = getDefaultStreamPreferences();

const NATIVE_VIDEO_BACKEND_PREFERENCES = new Set<NativeVideoBackendPreference>(["auto", "d3d11", "d3d12"]);
const APP_ACCENT_COLORS = new Set<AppAccentColor>(["green", "blue", "violet", "amber", "rose"]);
const APP_THEMES = new Set<AppTheme>(["light", "dark", "auto"]);

function normalizeNativeVideoBackendPreference(raw: unknown): NativeVideoBackendPreference {
  return NATIVE_VIDEO_BACKEND_PREFERENCES.has(raw as NativeVideoBackendPreference)
    ? (raw as NativeVideoBackendPreference)
    : "auto";
}

function normalizeAppAccentColor(raw: unknown): AppAccentColor {
  return APP_ACCENT_COLORS.has(raw as AppAccentColor) ? (raw as AppAccentColor) : "green";
}

function normalizeAppTheme(raw: unknown): AppTheme {
  return APP_THEMES.has(raw as AppTheme) ? (raw as AppTheme) : "auto";
}

function normalizeRecordingBitrateMbps(raw: unknown): number | null {
  if (raw === null || raw === undefined) {
    return null;
  }
  const value = Number(raw);
  if (!Number.isFinite(value)) {
    return null;
  }
  return Math.max(1, Math.min(200, Math.round(value)));
}

const DEFAULT_SETTINGS: Settings = {
  resolution: "1920x1080",
  aspectRatio: "16:9",
  posterSizeScale: 1,
  fps: 60,
  maxBitrateMbps: 75,
  recordingBitrateMbps: null,
  streamClientMode: "web",
  nativeStreamerBackend: "gstreamer",
  nativeVideoBackend: "auto",
  nativeStreamerExecutablePath: "",
  nativeCloudGsyncMode: "auto",
  nativeD3dFullscreenMode: "auto",
  nativeExternalRenderer: true,
  showNativeStreamerStats: false,
  codec: DEFAULT_STREAM_PREFERENCES.codec,
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
  shortcutToggleStats: "F3",
  shortcutTogglePointerLock: "F8",
  shortcutToggleFullscreen: "F10",
  shortcutStopStream: defaultStopShortcut,
  shortcutToggleAntiAfk: defaultAntiAfkShortcut,
  shortcutToggleMicrophone: defaultMicShortcut,
  shortcutScreenshot: "F11",
  shortcutToggleRecording: "F12",
  microphoneMode: "disabled",
  microphoneDeviceId: "",
  hideStreamButtons: false,
  showAntiAfkIndicator: true,
  showStatsOnLaunch: false,
  hideServerSelector: false,
  appAccentColor: "green",
  appTheme: "auto",
  translucentUI: false,
  controllerMode: false,
  launchInConsoleMode: false,
  autoFullScreen: false,
  favoriteGameIds: [],
  sessionCounterEnabled: false,
  showSessionTimeRemainingInStatsOverlay: false,
  sessionClockShowEveryMinutes: 60,
  sessionClockShowDurationSeconds: 30,
  windowWidth: 1400,
  windowHeight: 900,
  keyboardLayout: DEFAULT_KEYBOARD_LAYOUT,
  gameLanguage: "en_US",
  enablePersistingInGameSettings: false,
  enableL4S: false,
  enableCloudGsync: false,
  nativeTransitionDiagnostics: undefined,
  discordRichPresence: false,
  autoCheckForUpdates: true,
  allowEscapeToExitFullscreen: false,
  enableFastQueueJoin: false,
  lastSeenReleaseHighlightsVersion: "",
  videoShader: { ...DEFAULT_VIDEO_SHADER_SETTINGS },
};

const SHORTCUT_SETTING_KEYS = [
  "shortcutToggleStats",
  "shortcutTogglePointerLock",
  "shortcutToggleFullscreen",
  "shortcutStopStream",
  "shortcutToggleAntiAfk",
  "shortcutToggleMicrophone",
  "shortcutScreenshot",
  "shortcutToggleRecording",
] as const satisfies readonly (keyof Settings)[];

type ShortcutSettingKey = typeof SHORTCUT_SETTING_KEYS[number];

const SIDEBAR_RESERVED_SHORTCUTS_NON_MAC = new Set(["CTRL+G", "CTRL+SHIFT+G"]);
const SIDEBAR_RESERVED_SHORTCUTS_MAC = new Set(["META+G", "CMD+G", "COMMAND+G"]);
const SIDEBAR_RESERVED_SHORTCUT_FALLBACKS: Record<ShortcutSettingKey, readonly string[]> = {
  shortcutToggleStats: ["F3", "Ctrl+Shift+F3", "Ctrl+Alt+F3"],
  shortcutTogglePointerLock: ["F8", "Ctrl+Shift+F8", "Ctrl+Alt+F8"],
  shortcutToggleFullscreen: ["F10", "Ctrl+Shift+F10", "Ctrl+Alt+F10"],
  shortcutStopStream: [defaultStopShortcut, "Ctrl+Alt+Q", "Ctrl+Alt+Shift+Q"],
  shortcutToggleAntiAfk: [defaultAntiAfkShortcut, "Ctrl+Alt+K", "Ctrl+Alt+Shift+K"],
  shortcutToggleMicrophone: [defaultMicShortcut, "Ctrl+Alt+M", "Ctrl+Alt+Shift+M"],
  shortcutScreenshot: ["F11", "Ctrl+Shift+S", "Ctrl+Alt+S", "Ctrl+Shift+F11", "Ctrl+Alt+Shift+S"],
  shortcutToggleRecording: ["F12", "Ctrl+Shift+R", "Ctrl+Alt+R", "Ctrl+Shift+F12", "Ctrl+Alt+Shift+R"],
};

function normalizeShortcutForComparison(value: string): string {
  return value.replace(/\s+/g, "").toUpperCase();
}

function isSidebarReservedShortcut(value: string): boolean {
  const normalized = normalizeShortcutForComparison(value);
  const reserved = process.platform === "darwin"
    ? SIDEBAR_RESERVED_SHORTCUTS_MAC
    : SIDEBAR_RESERVED_SHORTCUTS_NON_MAC;
  return reserved.has(normalized);
}

function isShortcutAvailable(
  settings: Settings,
  key: ShortcutSettingKey,
  candidate: string,
): boolean {
  const normalizedCandidate = normalizeShortcutForComparison(candidate);
  if (isSidebarReservedShortcut(candidate)) {
    return false;
  }

  return SHORTCUT_SETTING_KEYS.every((otherKey) => (
    otherKey === key ||
    normalizeShortcutForComparison(settings[otherKey]) !== normalizedCandidate
  ));
}

export class SettingsManager {
  private settings: Settings;
  private readonly settingsPath: string;

  constructor() {
    this.settingsPath = join(app.getPath("userData"), "settings.json");
    this.settings = this.load();
  }

  /**
   * Load settings from disk or return defaults if file doesn't exist
   */
  private load(): Settings {
    try {
      if (!existsSync(this.settingsPath)) {
        const defaults = { ...DEFAULT_SETTINGS };
        this.enforceCompatibility(defaults);
        return defaults;
      }

      const content = readFileSync(this.settingsPath, "utf-8");
      type PersistedSettings = Partial<Settings> & {
        sessionTimeRemainingDisplay?: unknown;
      };
      const parsed = JSON.parse(content) as PersistedSettings;
      const {
        sessionTimeRemainingDisplay: legacySessionTimeDisplay,
        ...parsedSettings
      } = parsed;

      // Merge with defaults to ensure all fields exist
      const merged: Settings = {
        ...DEFAULT_SETTINGS,
        ...parsedSettings,
      };

      let migrated = this.migrateLegacyShortcutDefaults(merged);
      migrated = this.enforceCompatibility(merged) || migrated;

      const accentColorBefore = merged.appAccentColor;
      merged.appAccentColor = normalizeAppAccentColor(merged.appAccentColor);
      if (merged.appAccentColor !== accentColorBefore) {
        migrated = true;
      }

      const themeBefore = merged.appTheme;
      merged.appTheme = normalizeAppTheme(merged.appTheme);
      if (merged.appTheme !== themeBefore) {
        migrated = true;
      }

      // Migrate legacy boolean accelerator setting to percentage slider.
      if (typeof (parsed as { mouseAcceleration?: unknown }).mouseAcceleration === "boolean") {
        merged.mouseAcceleration = (parsed as { mouseAcceleration?: boolean }).mouseAcceleration ? 100 : 1;
        migrated = true;
      }

      // Migrate a short-lived prerelease display enum while keeping the old key out of saved settings.
      if (legacySessionTimeDisplay === "stats" || legacySessionTimeDisplay === "both") {
        merged.showSessionTimeRemainingInStatsOverlay = true;
        migrated = true;
      }

      merged.mouseAcceleration = Math.max(1, Math.min(150, Math.round(merged.mouseAcceleration)));
      const recordingBitrateBefore = merged.recordingBitrateMbps;
      merged.recordingBitrateMbps = normalizeRecordingBitrateMbps(merged.recordingBitrateMbps);
      if (merged.recordingBitrateMbps !== recordingBitrateBefore) {
        migrated = true;
      }
      if (migrated) {
        writeFileSync(this.settingsPath, JSON.stringify(merged, null, 2), "utf-8");
      }

      return merged;
    } catch (error) {
      console.error("Failed to load settings, using defaults:", error);
      const defaults = { ...DEFAULT_SETTINGS };
      this.enforceCompatibility(defaults);
      return defaults;
    }
  }

  private enforceCompatibility(settings: Settings): boolean {
    let migrated = false;
    const normalized = normalizeStreamPreferences(settings.codec, settings.colorQuality);
    if (normalized.migrated) {
      console.warn(
        `[Settings] Migrating unsupported stream settings codec="${settings.codec}" colorQuality="${settings.colorQuality}" to ${normalized.codec}/${normalized.colorQuality}`,
      );
      settings.codec = normalized.codec;
      settings.colorQuality = normalized.colorQuality;
      migrated = true;
    }

    const streamClientMode = normalizeStreamClientModeForPlatform(settings.streamClientMode, process.platform);
    if (settings.streamClientMode !== streamClientMode) {
      settings.streamClientMode = streamClientMode;
      migrated = true;
    }

    if (settings.nativeStreamerBackend !== "gstreamer") {
      settings.nativeStreamerBackend = "gstreamer";
      migrated = true;
    }
    const appAccentColor = normalizeAppAccentColor(settings.appAccentColor);
    if (settings.appAccentColor !== appAccentColor) {
      settings.appAccentColor = appAccentColor;
      migrated = true;
    }
    const appTheme = normalizeAppTheme(settings.appTheme);
    if (settings.appTheme !== appTheme) {
      settings.appTheme = appTheme;
      migrated = true;
    }
    if (typeof settings.translucentUI !== "boolean") {
      settings.translucentUI = false;
      migrated = true;
    }
    if (!settings.nativeExternalRenderer) {
      settings.nativeExternalRenderer = true;
      migrated = true;
    }
    const nativeVideoBackend = normalizeNativeVideoBackendPreference(settings.nativeVideoBackend);
    if (settings.nativeVideoBackend !== nativeVideoBackend) {
      settings.nativeVideoBackend = nativeVideoBackend;
      migrated = true;
    }

    const recordingBitrate = normalizeRecordingBitrateMbps(settings.recordingBitrateMbps);
    if (settings.recordingBitrateMbps !== recordingBitrate) {
      settings.recordingBitrateMbps = recordingBitrate;
      migrated = true;
    }

    if (typeof settings.steamControllerCompatibilityMode !== "boolean") {
      settings.steamControllerCompatibilityMode = false;
      migrated = true;
    }

    const videoShader = normalizeVideoShaderSettings(settings.videoShader);
    if (JSON.stringify(settings.videoShader) !== JSON.stringify(videoShader)) {
      settings.videoShader = videoShader;
      migrated = true;
    }

    return migrated;
  }

  private migrateLegacyShortcutDefaults(settings: Settings): boolean {
    let migrated = false;

    const normalizeShortcut = (value: string): string => value.replace(/\s+/g, "").toUpperCase();
    const stopShortcut = normalizeShortcut(settings.shortcutStopStream);
    const antiAfkShortcut = normalizeShortcut(settings.shortcutToggleAntiAfk);

    if (LEGACY_STOP_SHORTCUTS.has(stopShortcut)) {
      settings.shortcutStopStream = defaultStopShortcut;
      migrated = true;
    }

    if (LEGACY_ANTI_AFK_SHORTCUTS.has(antiAfkShortcut)) {
      settings.shortcutToggleAntiAfk = defaultAntiAfkShortcut;
      migrated = true;
    }

    for (const key of SHORTCUT_SETTING_KEYS) {
      if (!isSidebarReservedShortcut(settings[key])) {
        continue;
      }

      const fallback = SIDEBAR_RESERVED_SHORTCUT_FALLBACKS[key].find((candidate) =>
        isShortcutAvailable(settings, key, candidate),
      ) ?? DEFAULT_SETTINGS[key];
      settings[key] = fallback;
      migrated = true;
    }

    return migrated;
  }

  /**
   * Save current settings to disk
   */
  private save(): void {
    try {
      const dir = join(app.getPath("userData"));
      if (!existsSync(dir)) {
        mkdirSync(dir, { recursive: true });
      }

      writeFileSync(this.settingsPath, JSON.stringify(this.settings, null, 2), "utf-8");
    } catch (error) {
      console.error("Failed to save settings:", error);
    }
  }

  /**
   * Get all current settings
   */
  getAll(): Settings {
    return { ...this.settings };
  }

  /**
   * Get a specific setting value
   */
  get<K extends keyof Settings>(key: K): Settings[K] {
    return this.settings[key];
  }

  /**
   * Update a specific setting value
   */
  set<K extends keyof Settings>(key: K, value: Settings[K]): void {
    this.settings[key] = value;
    this.enforceCompatibility(this.settings);
    this.save();
  }

  /**
   * Update multiple settings at once
   */
  setMultiple(updates: Partial<Settings>): void {
    this.settings = {
      ...this.settings,
      ...updates,
    };
    this.enforceCompatibility(this.settings);
    this.save();
  }

  /**
   * Reset all settings to defaults
   */
  reset(): Settings {
    this.settings = { ...DEFAULT_SETTINGS };
    this.enforceCompatibility(this.settings);
    this.save();
    return { ...this.settings };
  }

  /**
   * Get the default settings
   */
  getDefaults(): Settings {
    const defaults = { ...DEFAULT_SETTINGS };
    this.enforceCompatibility(defaults);
    return defaults;
  }
}

// Singleton instance
let settingsManager: SettingsManager | null = null;

export function getSettingsManager(): SettingsManager {
  if (!settingsManager) {
    settingsManager = new SettingsManager();
  }
  return settingsManager;
}

export { DEFAULT_SETTINGS };
