import { app } from "electron";
import { join } from "node:path";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import type { VideoCodec, ColorQuality } from "@shared/gfn";

export interface Settings {
  /** Video resolution (e.g., "1920x1080") */
  resolution: string;
  /** Target FPS (30, 60, 120, etc.) */
  fps: number;
  /** Maximum bitrate in Mbps (200 = unlimited) */
  maxBitrateMbps: number;
  /** Preferred video codec */
  codec: VideoCodec;
  /** Color quality (bit depth + chroma subsampling) */
  colorQuality: ColorQuality;
  /** Window width */
  windowWidth: number;
  /** Window height */
  windowHeight: number;
}

const DEFAULT_SETTINGS: Settings = {
  resolution: "1920x1080",
  fps: 60,
  maxBitrateMbps: 75,
  codec: "H264",
  colorQuality: "10bit_420",
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
      return {
        ...DEFAULT_SETTINGS,
        ...parsed,
      };
    } catch (error) {
      console.error("Failed to load settings, using defaults:", error);
      return { ...DEFAULT_SETTINGS };
    }
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
