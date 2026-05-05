import { app } from "electron";
import { join } from "node:path";
import { readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import type { Settings } from "@shared/gfn";
import { DEFAULT_SETTINGS, DEFAULT_SHORTCUTS, normalizeStreamPreferences } from "@shared/gfn";

const LEGACY_STOP_SHORTCUTS = new Set(["META+SHIFT+Q", "CMD+SHIFT+Q"]);
const LEGACY_ANTI_AFK_SHORTCUTS = new Set(["META+SHIFT+F10", "CMD+SHIFT+F10", "CTRL+SHIFT+F10"]);

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
      const parsed = JSON.parse(content) as Partial<Settings>;

      // Merge with defaults to ensure all fields exist
      const merged: Settings = {
        ...DEFAULT_SETTINGS,
        ...parsed,
      };

      let migrated = this.migrateLegacyShortcutDefaults(merged);
      migrated = this.enforceCompatibility(merged) || migrated;

      // Migrate legacy boolean accelerator setting to percentage slider.
      if (typeof (parsed as { mouseAcceleration?: unknown }).mouseAcceleration === "boolean") {
        merged.mouseAcceleration = (parsed as { mouseAcceleration?: boolean }).mouseAcceleration ? 100 : 1;
        migrated = true;
      }

      merged.mouseAcceleration = Math.max(1, Math.min(150, Math.round(merged.mouseAcceleration)));
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
    const normalized = normalizeStreamPreferences(settings.codec, settings.colorQuality);
    if (!normalized.migrated) {
      return false;
    }

    console.warn(
      `[Settings] Migrating unsupported stream settings codec="${settings.codec}" colorQuality="${settings.colorQuality}" to ${normalized.codec}/${normalized.colorQuality}`,
    );
    settings.codec = normalized.codec;
    settings.colorQuality = normalized.colorQuality;
    return true;
  }

  private migrateLegacyShortcutDefaults(settings: Settings): boolean {
    let migrated = false;

    const normalizeShortcut = (value: string): string => value.replace(/\s+/g, "").toUpperCase();
    const stopShortcut = normalizeShortcut(settings.shortcutStopStream);
    const antiAfkShortcut = normalizeShortcut(settings.shortcutToggleAntiAfk);

    if (LEGACY_STOP_SHORTCUTS.has(stopShortcut)) {
      settings.shortcutStopStream = DEFAULT_SHORTCUTS.shortcutStopStream;
      migrated = true;
    }

    if (LEGACY_ANTI_AFK_SHORTCUTS.has(antiAfkShortcut)) {
      settings.shortcutToggleAntiAfk = DEFAULT_SHORTCUTS.shortcutToggleAntiAfk;
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
