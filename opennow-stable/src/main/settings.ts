import { app } from "electron";
import { join } from "node:path";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import type { VideoCodec, ColorQuality, VideoAccelerationPreference, MicrophoneMode } from "@shared/gfn";
import type { VideoCodec, ColorQuality, VideoAccelerationPreference, FlightSlotConfig, HdrStreamingMode, MicMode, HevcCompatMode, VideoDecodeBackend } from "@shared/gfn";
import { defaultFlightSlots } from "@shared/gfn";
export interface Settings {
  /** Video resolution (e.g., "1920x1080") */
  resolution: string;
  /** Target FPS (30, 60, 120, etc.) */
  fps: number;
  /** Maximum bitrate in Mbps (200 = unlimited) */
  maxBitrateMbps: number;
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
  /** Enable clipboard paste into stream */
  clipboardPaste: boolean;
  /** Mouse sensitivity multiplier */
  mouseSensitivity: number;
  /** Toggle stats overlay shortcut */
  shortcutToggleStats: string;
  /** Toggle pointer lock shortcut */
  shortcutTogglePointerLock: string;
  /** Stop stream shortcut */
  shortcutStopStream: string;
  /** Toggle anti-AFK shortcut */
  shortcutToggleAntiAfk: string;
  /** Toggle microphone shortcut */
  shortcutToggleMicrophone: string;
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
  /** Window width */
  windowWidth: number;
  /** Window height */
  windowHeight: number;
  /** Enable Discord Rich Presence */
  discordPresenceEnabled: boolean;
  /** Discord Application Client ID */
  discordClientId: string;
  /** Enable flight controls (HOTAS/joystick) */
  flightControlsEnabled: boolean;
  /** Controller slot for flight controls (0-3) — legacy, kept for compat */
  flightControlsSlot: number;
  /** Per-slot flight configurations */
  flightSlots: FlightSlotConfig[];
  /** HDR streaming mode: off, auto, on */
  hdrStreaming: HdrStreamingMode;
  /** Microphone mode: off, on, push-to-talk */
  micMode: MicMode;
  /** Selected microphone device ID (empty = default) */
  micDeviceId: string;
  /** Microphone input gain 0.0 - 2.0 */
  micGain: number;
  /** Enable noise suppression */
  micNoiseSuppression: boolean;
  /** Enable automatic gain control */
  micAutoGainControl: boolean;
  /** Enable echo cancellation */
  micEchoCancellation: boolean;
  /** Toggle mic on/off shortcut (works in-stream) */
  shortcutToggleMic: string;
  /** HEVC compatibility mode: auto, force_h264, force_hevc, hevc_software */
  hevcCompatMode: HevcCompatMode;
  /** Linux video decode backend override: auto, vaapi, v4l2, software */
  videoDecodeBackend: VideoDecodeBackend;
  /** Show session clock every N minutes (0 = always visible) */
  sessionClockShowEveryMinutes: number;
  /** Duration in seconds to show session clock when periodically revealed */
  sessionClockShowDurationSeconds: number;}

const defaultStopShortcut = "Ctrl+Shift+Q";
const defaultAntiAfkShortcut = "Ctrl+Shift+K";
const defaultMicShortcut = "Ctrl+Shift+M";
const LEGACY_STOP_SHORTCUTS = new Set(["META+SHIFT+Q", "CMD+SHIFT+Q"]);
const LEGACY_ANTI_AFK_SHORTCUTS = new Set(["META+SHIFT+F10", "CMD+SHIFT+F10", "CTRL+SHIFT+F10"]);

const DEFAULT_SETTINGS: Settings = {
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
  shortcutToggleStats: "F3",
  shortcutTogglePointerLock: "F8",
  shortcutStopStream: defaultStopShortcut,
  shortcutToggleAntiAfk: defaultAntiAfkShortcut,
  shortcutToggleMicrophone: defaultMicShortcut,
  microphoneMode: "disabled",
  microphoneDeviceId: "",
  hideStreamButtons: false,
  windowWidth: 1400,
  windowHeight: 900,
  discordPresenceEnabled: false,
  discordClientId: "",
  flightControlsEnabled: false,
  flightControlsSlot: 3,
  flightSlots: defaultFlightSlots(),
  hdrStreaming: "off",
  micMode: "off",
  micDeviceId: "",
  micGain: 1.0,
  micNoiseSuppression: true,
  micAutoGainControl: true,
  micEchoCancellation: true,
  shortcutToggleMic: "Ctrl+Shift+M",
  hevcCompatMode: "auto",
  videoDecodeBackend: "auto",  sessionClockShowEveryMinutes: 60,
  sessionClockShowDurationSeconds: 30,
  windowWidth: 1400,
  windowHeight: 900,
};

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
        return { ...DEFAULT_SETTINGS };
      }

      const content = readFileSync(this.settingsPath, "utf-8");
      const parsed = JSON.parse(content) as Partial<Settings>;

      // Merge with defaults to ensure all fields exist
      const merged: Settings = {
        ...DEFAULT_SETTINGS,
        ...parsed,
      };

      const migrated = this.migrateLegacyShortcutDefaults(merged);
      if (migrated) {
        writeFileSync(this.settingsPath, JSON.stringify(merged, null, 2), "utf-8");
      }

      return merged;
    } catch (error) {
      console.error("Failed to load settings, using defaults:", error);
      return { ...DEFAULT_SETTINGS };
    }
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
    this.save();
  }

  /**
   * Reset all settings to defaults
   */
  reset(): Settings {
    this.settings = { ...DEFAULT_SETTINGS };
    this.save();
    return { ...this.settings };
  }

  /**
   * Get the default settings
   */
  getDefaults(): Settings {
    return { ...DEFAULT_SETTINGS };
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
