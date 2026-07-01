import { app } from "electron";
import { join } from "node:path";
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import type { ReleaseHighlightsPayload } from "@shared/gfn";

const GITHUB_API_BASE = "https://api.github.com/repos/OpenCloudGaming/OpenNOW";
const FETCH_TIMEOUT_MS = 8000;
const CACHE_FILE = "release-notes-cache.json";
const UPDATER_TOKEN_ENV_KEYS = ["OPENNOW_GH_TOKEN", "GH_TOKEN"] as const;

// ---------------------------------------------------------------------------
// Version comparison
// ---------------------------------------------------------------------------

interface SemverParts {
  major: number;
  minor: number;
  patch: number;
  /** Prerelease tag, e.g. "beta.1" or "rc.2". Empty string = stable release. */
  prerelease: string;
}

function parseSemver(v: string): SemverParts {
  const clean = v.replace(/^v/, "");
  const dashIdx = clean.indexOf("-");
  const numericPart = dashIdx === -1 ? clean : clean.slice(0, dashIdx);
  const prerelease = dashIdx === -1 ? "" : clean.slice(dashIdx + 1);
  const [rawMajor = "0", rawMinor = "0", rawPatch = "0"] = numericPart.split(".");
  return {
    major: Math.max(0, parseInt(rawMajor, 10) || 0),
    minor: Math.max(0, parseInt(rawMinor, 10) || 0),
    patch: Math.max(0, parseInt(rawPatch, 10) || 0),
    prerelease,
  };
}

/**
 * Compare two prerelease strings.
 * Stable (empty string) is GREATER than any prerelease per semver spec.
 * When both have a prerelease, the numeric suffix is compared if present;
 * otherwise a lexicographic comparison is used.
 * Returns: positive if a > b, negative if a < b, 0 if equal.
 */
function comparePrerelease(a: string, b: string): number {
  if (a === b) return 0;
  // Stable release beats any prerelease
  if (a === "") return 1;
  if (b === "") return -1;
  // Both have prerelease tags: try comparing trailing numeric identifiers
  const numA = parseInt(a.replace(/^.*?(\d+)$/, "$1"), 10);
  const numB = parseInt(b.replace(/^.*?(\d+)$/, "$1"), 10);
  if (!Number.isNaN(numA) && !Number.isNaN(numB)) {
    return numA - numB;
  }
  // Fall back to lexicographic order
  return a < b ? -1 : 1;
}

/**
 * Returns true if `current` is strictly greater than `lastSeen` (semver-aware).
 * An empty `lastSeen` means the user has never seen highlights.
 */
export function shouldShowReleaseHighlights(current: string, lastSeen: string): boolean {
  if (!current) return false;
  if (!lastSeen) return true;
  const a = parseSemver(current);
  const b = parseSemver(lastSeen);
  if (a.major !== b.major) return a.major > b.major;
  if (a.minor !== b.minor) return a.minor > b.minor;
  if (a.patch !== b.patch) return a.patch > b.patch;
  return comparePrerelease(a.prerelease, b.prerelease) > 0;
}

// ---------------------------------------------------------------------------
// GitHub token
// ---------------------------------------------------------------------------

function pickRuntimeToken(): string | null {
  for (const key of UPDATER_TOKEN_ENV_KEYS) {
    const value = process.env[key]?.trim();
    if (value) return value;
  }
  return null;
}

// ---------------------------------------------------------------------------
// Release-notes cache (for offline fallback)
// ---------------------------------------------------------------------------

type ReleaseNotesCache = Record<string, string>;

function getCachePath(): string {
  return join(app.getPath("userData"), CACHE_FILE);
}

function readCache(): ReleaseNotesCache {
  try {
    const path = getCachePath();
    if (!existsSync(path)) return {};
    return JSON.parse(readFileSync(path, "utf-8")) as ReleaseNotesCache;
  } catch {
    return {};
  }
}

export function writeCacheEntry(version: string, body: string): void {
  try {
    const cache = readCache();
    cache[version] = body;
    writeFileSync(getCachePath(), JSON.stringify(cache, null, 2), "utf-8");
  } catch (error) {
    console.warn("[ReleaseHighlights] Failed to write cache:", error);
  }
}

function readCacheEntry(version: string): string | null {
  const cache = readCache();
  return cache[version] ?? null;
}

// ---------------------------------------------------------------------------
// GitHub API fetch
// ---------------------------------------------------------------------------

interface GitHubRelease {
  tag_name: string;
  name: string | null;
  body: string | null;
}

async function fetchFromGitHub(version: string): Promise<string | null> {
  const tag = version.startsWith("v") ? version : `v${version}`;
  const url = `${GITHUB_API_BASE}/releases/tags/${tag}`;

  const headers: HeadersInit = {
    Accept: "application/vnd.github+json",
    "User-Agent": `OpenNOW/${version} (electron)`,
  };

  const token = pickRuntimeToken();
  if (token) {
    (headers as Record<string, string>)["Authorization"] = `token ${token}`;
  }

  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), FETCH_TIMEOUT_MS);

  try {
    const response = await fetch(url, { headers, signal: controller.signal });
    if (!response.ok) {
      console.warn(`[ReleaseHighlights] GitHub API returned ${response.status} for ${tag}`);
      return null;
    }
    const data = (await response.json()) as GitHubRelease;
    return data.body ?? null;
  } catch (error) {
    if ((error as Error).name === "AbortError") {
      console.warn("[ReleaseHighlights] GitHub fetch timed out");
    } else {
      console.warn("[ReleaseHighlights] GitHub fetch failed:", error);
    }
    return null;
  } finally {
    clearTimeout(timer);
  }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

const FALLBACK_BODY_TEMPLATE = (version: string): string =>
  `Release notes for OpenNOW v${version} could not be loaded right now.\n\nView the full changelog on GitHub.`;

/**
 * Build the full ReleaseHighlightsPayload for a given version.
 * Tries GitHub first, then the local updater cache, then a fallback string.
 * Never throws — always returns a payload.
 */
export async function getReleaseHighlightsPayload(version: string): Promise<ReleaseHighlightsPayload> {
  const cleanVersion = version.replace(/^v/, "");
  const displayTitle = `OpenNOW v${cleanVersion}`;

  // 1. Try GitHub
  const githubBody = await fetchFromGitHub(cleanVersion);
  if (githubBody) {
    // Cache successful GitHub fetch for offline fallback next time
    writeCacheEntry(cleanVersion, githubBody);
    return {
      version: cleanVersion,
      title: displayTitle,
      bodyMarkdown: githubBody,
      source: "github",
    };
  }

  // 2. Try local updater cache
  const cachedBody = readCacheEntry(cleanVersion);
  if (cachedBody) {
    return {
      version: cleanVersion,
      title: displayTitle,
      bodyMarkdown: cachedBody,
      source: "updater-cache",
    };
  }

  // 3. Fallback copy
  return {
    version: cleanVersion,
    title: displayTitle,
    bodyMarkdown: FALLBACK_BODY_TEMPLATE(cleanVersion),
    source: "fallback",
  };
}
