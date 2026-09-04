import { app } from "electron";
import { join } from "node:path";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import type {
  NativeVideoBackendPreference,
  AppAccentColor,
  AppTheme,
  ErrorReportingConsent,
  Settings,
} from "@shared/gfn";
import {
  createDefaultSettings,
  createPlatformShortcutDefaults,
  SHORTCUT_SETTING_KEYS,
  normalizeNativeExternalRendererForPlatform,
  normalizeFallbackCodecPreference,
  normalizeStreamClientModeForPlatform,
  normalizeStreamPreferences,
  normalizeTransportModeForPlatform,
  normalizeVideoShaderSettings,
  normalizeFrameInterpolationSettings,
  normalizeUpdateChannel,
  normalizeRecordingBitrateMbps,
  normalizeRecordingFps,
  normalizeRecordingResolution,
} from "@shared/gfn";
import type { StatsOverlayPosition } from "@shared/gfn";

export type { Settings } from "@shared/gfn";

const DEFAULT_SHORTCUTS = createPlatformShortcutDefaults(process.platform).bindings;
const defaultStopShortcut = DEFAULT_SHORTCUTS.shortcutStopStream;
const defaultAntiAfkShortcut = DEFAULT_SHORTCUTS.shortcutToggleAntiAfk;
const defaultMicShortcut = DEFAULT_SHORTCUTS.shortcutToggleMicrophone;
const LEGACY_STOP_SHORTCUTS = new Set(["META+SHIFT+Q", "CMD+SHIFT+Q"]);
const LEGACY_ANTI_AFK_SHORTCUTS = new Set(["META+SHIFT+F10", "CMD+SHIFT+F10", "CTRL+SHIFT+F10"]);

const NATIVE_VIDEO_BACKEND_PREFERENCES = new Set<NativeVideoBackendPreference>([
  "auto",
  "d3d11",
  "d3d12",
  "nvdec",
  "vaapi",
  "v4l2",
  "vulkan",
  "software",
]);
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

const ERROR_REPORTING_CONSENTS = new Set<ErrorReportingConsent>(["unset", "granted", "denied"]);

function normalizeErrorReportingConsent(raw: unknown): ErrorReportingConsent {
  return ERROR_REPORTING_CONSENTS.has(raw as ErrorReportingConsent)
    ? (raw as ErrorReportingConsent)
    : "unset";
}

function normalizeTelemetryInstallId(raw: unknown): string {
  return typeof raw === "string" ? raw.trim() : "";
}

type ShortcutSettingKey = typeof SHORTCUT_SETTING_KEYS[number];

const SIDEBAR_RESERVED_SHORTCUTS_NON_MAC = new Set(["CTRL+G", "CTRL+SHIFT+G"]);
const SIDEBAR_RESERVED_SHORTCUTS_MAC = new Set(["META+G", "CMD+G", "COMMAND+G"]);
const SIDEBAR_RESERVED_SHORTCUT_FALLBACKS: Record<ShortcutSettingKey, readonly string[]> = {
  shortcutToggleStats: ["F3", "Ctrl+Shift+F3", "Ctrl+Alt+F3"],
  shortcutTogglePointerLock: ["F8", "Ctrl+Shift+F8", "Ctrl+Alt+F8"],
  shortcutToggleFullscreen: ["F11", "Ctrl+Shift+F11", "Ctrl+Alt+F11"],
  shortcutStopStream: [defaultStopShortcut, "Ctrl+Alt+Q", "Ctrl+Alt+Shift+Q"],
  shortcutToggleAntiAfk: [defaultAntiAfkShortcut, "Ctrl+Alt+K", "Ctrl+Alt+Shift+K"],
  shortcutToggleMicrophone: [defaultMicShortcut, "Ctrl+Alt+M", "Ctrl+Alt+Shift+M"],
  shortcutScreenshot: ["Ctrl+F11", "Ctrl+Shift+S", "Ctrl+Alt+S", "Ctrl+Alt+Shift+F11", "Ctrl+Alt+Shift+S"],
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
        const defaults = createDefaultSettings(process.platform);
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
        ...createDefaultSettings(process.platform),
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
      const statsOverlayPositionBefore = merged.statsOverlayPosition;
      merged.statsOverlayPosition = normalizeStatsOverlayPosition(merged.statsOverlayPosition);
      if (merged.statsOverlayPosition !== statsOverlayPositionBefore) {
        migrated = true;
      }
      const recordingBitrateBefore = merged.recordingBitrateMbps;
      merged.recordingBitrateMbps = normalizeRecordingBitrateMbps(merged.recordingBitrateMbps);
      if (merged.recordingBitrateMbps !== recordingBitrateBefore) {
        migrated = true;
      }
      const recordingResolutionBefore = merged.recordingResolution;
      merged.recordingResolution = normalizeRecordingResolution(merged.recordingResolution);
      if (merged.recordingResolution !== recordingResolutionBefore) {
        migrated = true;
      }
      const recordingFpsBefore = merged.recordingFps;
      merged.recordingFps = normalizeRecordingFps(merged.recordingFps);
      if (merged.recordingFps !== recordingFpsBefore) {
        migrated = true;
      }
      if (migrated) {
        writeFileSync(this.settingsPath, JSON.stringify(merged, null, 2), "utf-8");
      }

      return merged;
    } catch (error) {
      console.error("Failed to load settings, using defaults:", error);
      const defaults = createDefaultSettings(process.platform);
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

    const fallbackCodec = normalizeFallbackCodecPreference(settings.fallbackCodec);
    if (settings.fallbackCodec !== fallbackCodec) {
      settings.fallbackCodec = fallbackCodec;
      migrated = true;
    }

    const streamClientMode = normalizeStreamClientModeForPlatform(settings.streamClientMode, process.platform);
    if (settings.streamClientMode !== streamClientMode) {
      settings.streamClientMode = streamClientMode;
      migrated = true;
    }

    if ("nativeStreamerBackend" in settings) {
      delete (settings as Settings & { nativeStreamerBackend?: unknown }).nativeStreamerBackend;
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
    const updateChannel = normalizeUpdateChannel(settings.updateChannel);
    if (settings.updateChannel !== updateChannel) {
      settings.updateChannel = updateChannel;
      migrated = true;
    }
    if (typeof settings.translucentUI !== "boolean") {
      settings.translucentUI = false;
      migrated = true;
    }
    if (typeof settings.controllerModePromptDismissed !== "boolean") {
      settings.controllerModePromptDismissed = false;
      migrated = true;
    }
    if (typeof settings.showSessionReport !== "boolean") {
      settings.showSessionReport = true;
      migrated = true;
    }
    if (typeof settings.nativeExternalRenderer !== "boolean") {
      settings.nativeExternalRenderer = false;
      migrated = true;
    }
    const nativeExternalRenderer = normalizeNativeExternalRendererForPlatform(
      settings.nativeExternalRenderer,
      process.platform,
    );
    if (settings.nativeExternalRenderer !== nativeExternalRenderer) {
      settings.nativeExternalRenderer = nativeExternalRenderer;
      migrated = true;
    }
    const transportMode = normalizeTransportModeForPlatform(
      settings.transportMode === "nvst" ? "nvst" : "webrtc",
      process.platform,
      settings.streamClientMode,
    );
    if (settings.transportMode !== transportMode) {
      settings.transportMode = transportMode;
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
    const recordingResolution = normalizeRecordingResolution(settings.recordingResolution);
    if (settings.recordingResolution !== recordingResolution) {
      settings.recordingResolution = recordingResolution;
      migrated = true;
    }
    const recordingFps = normalizeRecordingFps(settings.recordingFps);
    if (settings.recordingFps !== recordingFps) {
      settings.recordingFps = recordingFps;
      migrated = true;
    }

    if (typeof settings.steamControllerCompatibilityMode !== "boolean") {
      settings.steamControllerCompatibilityMode = false;
      migrated = true;
    }
    if (typeof settings.identifyAsSteamDeck !== "boolean") {
      settings.identifyAsSteamDeck = false;
      migrated = true;
    }

    const videoShader = normalizeVideoShaderSettings(settings.videoShader);
    if (JSON.stringify(settings.videoShader) !== JSON.stringify(videoShader)) {
      settings.videoShader = videoShader;
      migrated = true;
    }

    const frameInterpolation = normalizeFrameInterpolationSettings(settings.frameInterpolation);
    if (JSON.stringify(settings.frameInterpolation) !== JSON.stringify(frameInterpolation)) {
      settings.frameInterpolation = frameInterpolation;
      migrated = true;
    }

    const consentBefore = settings.errorReportingConsent;
    settings.errorReportingConsent = normalizeErrorReportingConsent(settings.errorReportingConsent);
    if (settings.errorReportingConsent !== consentBefore) {
      migrated = true;
    }

    const installIdBefore = settings.telemetryInstallId;
    settings.telemetryInstallId = normalizeTelemetryInstallId(settings.telemetryInstallId);
    if (settings.telemetryInstallId !== installIdBefore) {
      migrated = true;
    }

    return migrated;
  }

  private migrateLegacyShortcutDefaults(settings: Settings): boolean {
    let migrated = false;

    const normalizeShortcut = (value: string): string => value.replace(/\s+/g, "").toUpperCase();
    const stopShortcut = normalizeShortcut(settings.shortcutStopStream);
    const antiAfkShortcut = normalizeShortcut(settings.shortcutToggleAntiAfk);
    const fullscreenShortcut = normalizeShortcut(settings.shortcutToggleFullscreen);
    const screenshotShortcut = normalizeShortcut(settings.shortcutScreenshot);

    if (fullscreenShortcut === "F10" && screenshotShortcut === "F11") {
      settings.shortcutToggleFullscreen = DEFAULT_SHORTCUTS.shortcutToggleFullscreen;
      settings.shortcutScreenshot = DEFAULT_SHORTCUTS.shortcutScreenshot;
      if (settings.statsOverlayPosition === "bottom-left") {
        settings.statsOverlayPosition = "top-right";
      }
      migrated = true;
    }

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
      ) ?? DEFAULT_SHORTCUTS[key];
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
    this.settings = createDefaultSettings(process.platform);
    this.enforceCompatibility(this.settings);
    this.save();
    return { ...this.settings };
  }

  /**
   * Get the default settings
   */
  getDefaults(): Settings {
    const defaults = createDefaultSettings(process.platform);
    this.enforceCompatibility(defaults);
    return defaults;
  }
}

function normalizeStatsOverlayPosition(value: unknown): StatsOverlayPosition {
  switch (value) {
    case "bottom-right":
    case "top-left":
    case "top-right":
      return value;
    default:
      return "bottom-left";
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
