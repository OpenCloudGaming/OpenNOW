export interface PluginScript {
  id: string;
  name: string;
  description: string;
  shortcut: string;
  enabled: boolean;
  script: string;
}

export type PluginRunStatus = "idle" | "running" | "success" | "error";

export interface PluginRunState {
  status: PluginRunStatus;
  message?: string;
}

const PLUGIN_STORAGE_KEY = "opennow.plugins.v1";
const PLUGIN_THEME_STORAGE_KEY = "opennow.plugins.theme-accent.v1";

const DEFAULT_PLUGIN_SCRIPT = [
  "// This script runs inside OpenNOW with access to:",
  "// input, stream, theme, sleep(ms), and log(...args)",
  "await input.keyTap(\"F13\");",
].join("\n");

function makeId(): string {
  if (typeof crypto !== "undefined" && typeof crypto.randomUUID === "function") {
    return crypto.randomUUID();
  }
  return `plugin-${Date.now()}-${Math.floor(Math.random() * 1_000_000)}`;
}

export function createPluginScript(seed?: Partial<PluginScript>): PluginScript {
  return {
    id: seed?.id ?? makeId(),
    name: seed?.name ?? "New Plugin",
    description: seed?.description ?? "",
    shortcut: seed?.shortcut ?? "",
    enabled: seed?.enabled ?? true,
    script: seed?.script ?? DEFAULT_PLUGIN_SCRIPT,
  };
}

function sanitizePluginScript(raw: unknown): PluginScript | null {
  if (!raw || typeof raw !== "object") {
    return null;
  }
  const candidate = raw as Partial<PluginScript>;
  if (typeof candidate.id !== "string" || typeof candidate.name !== "string" || typeof candidate.script !== "string") {
    return null;
  }
  return {
    id: candidate.id,
    name: candidate.name,
    description: typeof candidate.description === "string" ? candidate.description : "",
    shortcut: typeof candidate.shortcut === "string" ? candidate.shortcut : "",
    enabled: typeof candidate.enabled === "boolean" ? candidate.enabled : true,
    script: candidate.script,
  };
}

export function loadPluginScripts(): PluginScript[] {
  try {
    const raw = window.localStorage.getItem(PLUGIN_STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) {
      return [];
    }
    return parsed
      .map((item) => sanitizePluginScript(item))
      .filter((item): item is PluginScript => item !== null);
  } catch {
    return [];
  }
}

export function persistPluginScripts(plugins: PluginScript[]): void {
  try {
    window.localStorage.setItem(PLUGIN_STORAGE_KEY, JSON.stringify(plugins));
  } catch {
    // ignore storage failures
  }
}

export function loadPluginAccentColor(): string | null {
  try {
    const raw = window.localStorage.getItem(PLUGIN_THEME_STORAGE_KEY);
    if (!raw || !raw.trim()) {
      return null;
    }
    return raw.trim();
  } catch {
    return null;
  }
}

export function persistPluginAccentColor(color: string | null): void {
  try {
    if (!color) {
      window.localStorage.removeItem(PLUGIN_THEME_STORAGE_KEY);
      return;
    }
    window.localStorage.setItem(PLUGIN_THEME_STORAGE_KEY, color);
  } catch {
    // ignore storage failures
  }
}
