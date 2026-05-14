import { protocol, app, shell, BrowserWindow, session, ipcMain, dialog, systemPreferences } from "electron";
import { fileURLToPath } from "node:url";
import { resolve, join, relative, dirname, sep, delimiter } from "node:path";
import { createReadStream, existsSync, readFileSync, writeFileSync, mkdirSync, createWriteStream, statSync, rmSync, cpSync, renameSync, realpathSync } from "node:fs";
import { Readable } from "node:stream";
import { stat, realpath, mkdir, readFile, writeFile, unlink, readdir, rm, access, copyFile, rename } from "node:fs/promises";
import { EventEmitter } from "node:events";
import crypto, { randomBytes, createHash, randomUUID } from "node:crypto";
import dns from "node:dns";
import { createRequire } from "node:module";
import { createServer } from "node:http";
import * as net from "node:net";
import net__default from "node:net";
import os, { tmpdir } from "node:os";
import { Client } from "discord-rpc";
import electronUpdater from "electron-updater";
import { Buffer as Buffer$1 } from "node:buffer";
import { spawn, execFile } from "node:child_process";
import WebSocket from "ws";
import __cjs_mod__ from "node:module";
const __filename = import.meta.filename;
const __dirname = import.meta.dirname;
const require2 = __cjs_mod__.createRequire(import.meta.url);
const IPC_CHANNELS = {
  AUTH_GET_SESSION: "auth:get-session",
  AUTH_GET_PROVIDERS: "auth:get-providers",
  AUTH_GET_REGIONS: "auth:get-regions",
  AUTH_LOGIN: "auth:login",
  AUTH_LOGOUT: "auth:logout",
  AUTH_LOGOUT_ALL: "auth:logout-all",
  AUTH_GET_SAVED_ACCOUNTS: "auth:get-saved-accounts",
  AUTH_SWITCH_ACCOUNT: "auth:switch-account",
  AUTH_REMOVE_ACCOUNT: "auth:remove-account",
  PING_REGIONS: "gfn:ping-regions",
  SUBSCRIPTION_FETCH: "subscription:fetch",
  GAMES_FETCH_MAIN: "games:fetch-main",
  GAMES_FETCH_LIBRARY: "games:fetch-library",
  GAMES_BROWSE_CATALOG: "games:browse-catalog",
  GAMES_FETCH_PUBLIC: "games:fetch-public",
  GAMES_RESOLVE_LAUNCH_ID: "games:resolve-launch-id",
  CREATE_SESSION: "gfn:create-session",
  POLL_SESSION: "gfn:poll-session",
  REPORT_SESSION_AD: "gfn:report-session-ad",
  STOP_SESSION: "gfn:stop-session",
  GET_ACTIVE_SESSIONS: "gfn:get-active-sessions",
  CLAIM_SESSION: "gfn:claim-session",
  SESSION_CONFLICT_DIALOG: "gfn:session-conflict-dialog",
  CONNECT_SIGNALING: "gfn:connect-signaling",
  DISCONNECT_SIGNALING: "gfn:disconnect-signaling",
  SEND_ANSWER: "gfn:send-answer",
  SEND_ICE_CANDIDATE: "gfn:send-ice-candidate",
  NATIVE_INPUT: "gfn:native-input",
  NATIVE_RENDER_SURFACE: "gfn:native-render-surface",
  REQUEST_KEYFRAME: "gfn:request-keyframe",
  SIGNALING_EVENT: "gfn:signaling-event",
  TOGGLE_FULLSCREEN: "window:toggle-fullscreen",
  SET_FULLSCREEN: "window:set-fullscreen",
  TOGGLE_POINTER_LOCK: "window:toggle-pointer-lock",
  POINTER_LOCK_CHANGE: "window:pointer-lock-change",
  EXTERNAL_ESCAPE: "app:external-escape",
  QUIT_APP: "app:quit",
  APP_UPDATER_GET_STATE: "app-updater:get-state",
  APP_UPDATER_CHECK: "app-updater:check",
  APP_UPDATER_DOWNLOAD: "app-updater:download",
  APP_UPDATER_INSTALL: "app-updater:install",
  APP_UPDATER_STATE_CHANGED: "app-updater:state-changed",
  SETTINGS_GET: "settings:get",
  SETTINGS_SET: "settings:set",
  SETTINGS_RESET: "settings:reset",
  SETTINGS_SELECT_NATIVE_STREAMER_EXECUTABLE: "settings:select-native-streamer-executable",
  NATIVE_STREAMER_STATUS: "native:streamer-status",
  NATIVE_CLOUD_GSYNC_CAPABILITIES: "native:cloud-gsync-capabilities",
  MICROPHONE_PERMISSION_GET: "microphone:permission:get",
  LOGS_EXPORT: "logs:export",
  LOGS_GET_RENDERER: "logs:get-renderer",
  SCREENSHOT_SAVE: "screenshot:save",
  SCREENSHOT_LIST: "screenshot:list",
  SCREENSHOT_DELETE: "screenshot:delete",
  SCREENSHOT_SAVE_AS: "screenshot:save-as",
  RECORDING_BEGIN: "recording:begin",
  RECORDING_CHUNK: "recording:chunk",
  RECORDING_FINISH: "recording:finish",
  RECORDING_ABORT: "recording:abort",
  RECORDING_LIST: "recording:list",
  RECORDING_DELETE: "recording:delete",
  RECORDING_SHOW_IN_FOLDER: "recording:showInFolder",
  CACHE_REFRESH_MANUAL: "cache:refresh-manual",
  CACHE_STATUS_UPDATE: "cache:status-update",
  CACHE_DELETE_ALL: "cache:delete-all",
  COMMUNITY_GET_THANKS: "community:get-thanks",
  // Media browsing
  MEDIA_LIST_BY_GAME: "media:list-by-game",
  MEDIA_THUMBNAIL: "media:thumbnail",
  MEDIA_SHOW_IN_FOLDER: "media:show-in-folder",
  MEDIA_PLAYBACK_URL: "media:playback-url",
  MEDIA_DELETE_FILE: "media:delete-file",
  MEDIA_REGEN_THUMBNAIL: "media:regen-thumbnail",
  // PrintedWaste queue integration
  PRINTEDWASTE_QUEUE_FETCH: "printedwaste:queue-fetch",
  PRINTEDWASTE_SERVER_MAPPING_FETCH: "printedwaste:server-mapping-fetch",
  // Discord Rich Presence
  DISCORD_CLEAR_ACTIVITY: "discord:clear-activity"
};
const PLAYABLE_VIDEO_EXTENSIONS = [".mp4", ".webm", ".mkv", ".mov"];
function isPlayableVideoFilePath(filePath) {
  const lower = filePath.toLowerCase();
  return PLAYABLE_VIDEO_EXTENSIONS.some((ext) => lower.endsWith(ext));
}
const MAX_MEDIA_PATH_LENGTH = 4096;
const OPENNOW_MEDIA_HOST = "opennow";
let openNowMediaProtocolHandleInstalled = false;
function videoMimeTypeForPath(filePath) {
  const lower = filePath.toLowerCase();
  if (lower.endsWith(".webm")) return "video/webm";
  if (lower.endsWith(".mp4")) return "video/mp4";
  if (lower.endsWith(".mov")) return "video/quicktime";
  if (lower.endsWith(".mkv")) return "video/x-matroska";
  return "application/octet-stream";
}
function parseByteRangeHeader(rangeHeader, fileSize) {
  const m = /^bytes=(\d*)-(\d*)$/i.exec(rangeHeader.trim());
  if (!m) return null;
  const g1 = m[1];
  const g2 = m[2];
  if (g1 !== "" && g2 !== "") {
    const start = Number(g1);
    const end = Number(g2);
    if (!Number.isFinite(start) || !Number.isFinite(end) || start > end || start >= fileSize) return null;
    return { start, end: Math.min(end, fileSize - 1) };
  }
  if (g1 !== "" && g2 === "") {
    const start = Number(g1);
    if (!Number.isFinite(start) || start >= fileSize) return null;
    return { start, end: fileSize - 1 };
  }
  if (g1 === "" && g2 !== "") {
    const len = Number(g2);
    if (!Number.isFinite(len) || len <= 0) return null;
    if (len >= fileSize) return { start: 0, end: fileSize - 1 };
    return { start: fileSize - len, end: fileSize - 1 };
  }
  return { start: 0, end: fileSize - 1 };
}
async function resolveTrustedOpenNowMediaPath(rawFp) {
  if (typeof rawFp !== "string" || rawFp.length > MAX_MEDIA_PATH_LENGTH) return null;
  try {
    const allowedRoot = resolve(join(app.getPath("pictures"), "OpenNOW"));
    const fpResolved = resolve(rawFp);
    const allowedRootReal = await realpath(allowedRoot).catch(() => allowedRoot);
    const fpReal = await realpath(fpResolved).catch(() => fpResolved);
    const rel = relative(allowedRootReal, fpReal);
    if (rel.startsWith("..")) return null;
    return fpReal;
  } catch {
    return null;
  }
}
async function getTrustedVideoPlaybackFileUrl(rawFp) {
  const fpReal = await resolveTrustedOpenNowMediaPath(rawFp);
  if (!fpReal || !isPlayableVideoFilePath(fpReal)) return null;
  return `opennow-media://${OPENNOW_MEDIA_HOST}/playback?p=${encodeURIComponent(fpReal)}`;
}
function registerOpenNowMediaProtocol() {
  if (openNowMediaProtocolHandleInstalled) return;
  openNowMediaProtocolHandleInstalled = true;
  protocol.handle("opennow-media", async (request) => {
    try {
      const url = new URL(request.url);
      if (url.hostname.toLowerCase() !== OPENNOW_MEDIA_HOST) {
        return new Response(null, { status: 404 });
      }
      const pathNorm = url.pathname.replace(/\/$/, "") || "/";
      if (!pathNorm.endsWith("/playback")) {
        return new Response(null, { status: 404 });
      }
      const p = url.searchParams.get("p");
      if (!p) return new Response(null, { status: 400 });
      const fpReal = await resolveTrustedOpenNowMediaPath(p);
      if (!fpReal || !isPlayableVideoFilePath(fpReal)) return new Response(null, { status: 404 });
      const mime = videoMimeTypeForPath(fpReal);
      const { size } = await stat(fpReal);
      const baseHeaders = {
        "Content-Type": mime,
        "Accept-Ranges": "bytes"
      };
      if (request.method === "HEAD") {
        return new Response(null, {
          status: 200,
          headers: {
            ...baseHeaders,
            "Content-Length": String(size)
          }
        });
      }
      const rangeRaw = request.headers.get("range");
      if (rangeRaw) {
        const firstRange = rangeRaw.split(",")[0]?.trim() ?? "";
        const parsed = firstRange ? parseByteRangeHeader(firstRange, size) : null;
        if (parsed) {
          const { start, end } = parsed;
          const chunkLength = end - start + 1;
          const nodeStream2 = createReadStream(fpReal, { start, end });
          const body2 = Readable.toWeb(nodeStream2);
          return new Response(body2, {
            status: 206,
            headers: {
              ...baseHeaders,
              "Content-Length": String(chunkLength),
              "Content-Range": `bytes ${start}-${end}/${size}`
            }
          });
        }
      }
      const nodeStream = createReadStream(fpReal);
      const body = Readable.toWeb(nodeStream);
      return new Response(body, {
        status: 200,
        headers: {
          ...baseHeaders,
          "Content-Length": String(size)
        }
      });
    } catch (err) {
      console.warn("[opennow-media] protocol handler:", err);
      return new Response(null, { status: 500 });
    }
  });
}
const MAX_LOG_ENTRIES = 5e3;
const SENSITIVE_PATTERNS = [
  // Email addresses
  { pattern: /[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/g, replacement: "[Redacted for privacy]" },
  // Authorization tokens (GFNJWT, Bearer, etc.)
  { pattern: /Authorization["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]+\s+[a-zA-Z0-9_\-]+/gi, replacement: "Authorization: [Redacted for privacy]" },
  // JWT tokens (three base64url parts separated by dots)
  { pattern: /[a-zA-Z0-9_\-]{10,}\.[a-zA-Z0-9_\-]{10,}\.[a-zA-Z0-9_\-]{10,}/g, replacement: "[Redacted for privacy]" },
  // Client tokens, access tokens
  { pattern: /client[_-]?token["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]{20,}/gi, replacement: "client_token: [Redacted for privacy]" },
  { pattern: /access[_-]?token["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]{20,}/gi, replacement: "access_token: [Redacted for privacy]" },
  { pattern: /refresh[_-]?token["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]{20,}/gi, replacement: "refresh_token: [Redacted for privacy]" },
  // Session IDs (UUID-like)
  { pattern: /session[_-]?id["']?\s*[:=]\s*["']?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, replacement: "session_id: [Redacted for privacy]" },
  // Passwords
  { pattern: /password["']?\s*[:=]\s*["']?[^\s"']{4,}/gi, replacement: "password: [Redacted for privacy]" },
  // API keys
  { pattern: /api[_-]?key["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]{16,}/gi, replacement: "api_key: [Redacted for privacy]" },
  // Credential/secret
  { pattern: /credential["']?\s*[:=]\s*["']?[^\s"']{8,}/gi, replacement: "credential: [Redacted for privacy]" },
  { pattern: /secret["']?\s*[:=]\s*["']?[^\s"']{8,}/gi, replacement: "secret: [Redacted for privacy]" },
  // IP addresses (might be sensitive in some contexts)
  { pattern: /\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}\b/g, replacement: "[Redacted IP]" },
  // Device IDs, client IDs (UUID-like patterns)
  { pattern: /device[_-]?id["']?\s*[:=]\s*["']?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, replacement: "device_id: [Redacted for privacy]" },
  { pattern: /client[_-]?id["']?\s*[:=]\s*["']?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, replacement: "client_id: [Redacted for privacy]" },
  // User IDs that look like UUIDs
  { pattern: /user[_-]?id["']?\s*[:=]\s*["']?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, replacement: "user_id: [Redacted for privacy]" },
  // OAuth codes
  { pattern: /code["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]{20,}/gi, replacement: "code: [Redacted for privacy]" },
  // Peer names/IDs in signaling
  { pattern: /peer[_-]?name["']?\s*[:=]\s*["']?peer-\d+/gi, replacement: "peer_name: [Redacted for privacy]" }
];
function redactSensitiveData(text) {
  let redacted = text;
  for (const { pattern, replacement } of SENSITIVE_PATTERNS) {
    redacted = redacted.replace(pattern, replacement);
  }
  return redacted;
}
function formatLogEntry(entry) {
  const date = new Date(entry.timestamp);
  const timeStr = date.toISOString();
  const levelStr = entry.level.toUpperCase().padStart(5);
  const prefixStr = entry.prefix ? `[${entry.prefix}] ` : "";
  const argsStr = entry.args.length > 0 ? " " + entry.args.map((arg) => {
    try {
      if (typeof arg === "object" && arg !== null) {
        return JSON.stringify(arg);
      }
      return String(arg);
    } catch {
      return "[Object]";
    }
  }).join(" ") : "";
  return `${timeStr} ${levelStr} ${prefixStr}${entry.message}${argsStr}`;
}
function createRedactedLogExport(entries) {
  const lines = entries.map((entry) => {
    const formatted = formatLogEntry(entry);
    return redactSensitiveData(formatted);
  });
  return lines.join("\n");
}
class LogCapture {
  entries = [];
  originalConsole = null;
  processName;
  constructor(processName) {
    this.processName = processName;
  }
  /**
   * Get all captured log entries
   */
  getEntries() {
    return [...this.entries];
  }
  /**
   * Clear all log entries
   */
  clear() {
    this.entries = [];
  }
  /**
   * Get count of captured entries
   */
  getCount() {
    return this.entries.length;
  }
  /**
   * Add a log entry directly
   */
  addEntry(level, prefix, message, args) {
    const entry = {
      timestamp: Date.now(),
      level,
      prefix,
      message,
      args
    };
    this.entries.push(entry);
    if (this.entries.length > MAX_LOG_ENTRIES) {
      this.entries.shift();
    }
  }
  /**
   * Intercept console methods to capture logs
   */
  interceptConsole() {
    if (this.originalConsole) {
      return;
    }
    this.originalConsole = {
      log: console.log,
      error: console.error,
      warn: console.warn,
      info: console.info,
      debug: console.debug
    };
    const extractPrefix = (args) => {
      if (args.length > 0 && typeof args[0] === "string") {
        const match = args[0].match(/^\[([^\]]+)\]\s*(.*)$/);
        if (match) {
          return {
            prefix: match[1],
            message: match[2],
            rest: args.slice(1)
          };
        }
      }
      return {
        prefix: this.processName,
        message: args.length > 0 ? String(args[0]) : "",
        rest: args.slice(1)
      };
    };
    console.log = (...args) => {
      const { prefix, message, rest } = extractPrefix(args);
      this.addEntry("log", prefix, message, rest);
      this.originalConsole?.log?.apply(console, args);
    };
    console.error = (...args) => {
      const { prefix, message, rest } = extractPrefix(args);
      this.addEntry("error", prefix, message, rest);
      this.originalConsole?.error?.apply(console, args);
    };
    console.warn = (...args) => {
      const { prefix, message, rest } = extractPrefix(args);
      this.addEntry("warn", prefix, message, rest);
      this.originalConsole?.warn?.apply(console, args);
    };
    console.info = (...args) => {
      const { prefix, message, rest } = extractPrefix(args);
      this.addEntry("info", prefix, message, rest);
      this.originalConsole?.info?.apply(console, args);
    };
    console.debug = (...args) => {
      const { prefix, message, rest } = extractPrefix(args);
      this.addEntry("debug", prefix, message, rest);
      this.originalConsole?.debug?.apply(console, args);
    };
  }
  /**
   * Restore original console methods
   */
  restoreConsole() {
    if (this.originalConsole) {
      if (this.originalConsole.log) console.log = this.originalConsole.log;
      if (this.originalConsole.error) console.error = this.originalConsole.error;
      if (this.originalConsole.warn) console.warn = this.originalConsole.warn;
      if (this.originalConsole.info) console.info = this.originalConsole.info;
      if (this.originalConsole.debug) console.debug = this.originalConsole.debug;
      this.originalConsole = null;
    }
  }
  /**
   * Export logs as redacted text
   */
  exportRedacted() {
    const header = `OpenNOW Logs Export
Generated: ${(/* @__PURE__ */ new Date()).toISOString()}
Source: ${this.processName}
Total Entries: ${this.entries.length}
${"=".repeat(60)}

`;
    const redactedLogs = createRedactedLogExport(this.entries);
    return header + redactedLogs;
  }
  /**
   * Export logs as JSON (for programmatic use)
   */
  exportJSON() {
    const exportData = {
      source: this.processName,
      generatedAt: Date.now(),
      entryCount: this.entries.length,
      entries: this.entries.map((entry) => ({
        ...entry,
        // Redact sensitive data in messages and args
        message: redactSensitiveData(entry.message),
        args: entry.args.map((arg) => {
          if (typeof arg === "string") {
            return redactSensitiveData(arg);
          }
          if (typeof arg === "object" && arg !== null) {
            try {
              return JSON.parse(redactSensitiveData(JSON.stringify(arg)));
            } catch {
              return arg;
            }
          }
          return arg;
        })
      }))
    };
    return JSON.stringify(exportData, null, 2);
  }
}
let globalLogCapture = null;
function initLogCapture(processName) {
  if (!globalLogCapture) {
    globalLogCapture = new LogCapture(processName);
    globalLogCapture.interceptConsole();
  }
  return globalLogCapture;
}
function exportLogs(format = "text") {
  if (!globalLogCapture) {
    return "No logs captured";
  }
  return format === "json" ? globalLogCapture.exportJSON() : globalLogCapture.exportRedacted();
}
const CACHE_DIRECTORY = "gfn-cache";
const CACHE_TTL_MS = 12 * 60 * 60 * 1e3;
const THUMBNAILS_DIRECTORY = "media-thumbs";
class CacheManager {
  cacheDir;
  initialized = false;
  constructor() {
    this.cacheDir = join(app.getPath("userData"), CACHE_DIRECTORY);
  }
  async initialize() {
    if (this.initialized) return;
    try {
      await mkdir(this.cacheDir, { recursive: true });
      this.initialized = true;
      console.log(`[CACHE] Initialized cache directory: ${this.cacheDir}`);
    } catch (error) {
      console.error(`[CACHE] Failed to initialize cache directory:`, error);
      throw error;
    }
  }
  getCacheFilePath(key) {
    const sanitized = key.replace(/[^a-z0-9-]/gi, "_");
    return join(this.cacheDir, `${sanitized}.json`);
  }
  async loadFromCache(key) {
    if (!this.initialized) {
      console.warn(`[CACHE] Cache not initialized, skipping load for key: ${key}`);
      return null;
    }
    const filePath = this.getCacheFilePath(key);
    if (!existsSync(filePath)) {
      console.log(`[CACHE] Cache miss (file not found): ${key}`);
      return null;
    }
    try {
      const content = await readFile(filePath, "utf-8");
      const parsed = JSON.parse(content);
      if (!parsed.metadata || typeof parsed.metadata.expiresAt !== "number") {
        console.warn(`[CACHE] Cache corrupted (invalid metadata): ${key}`);
        await this.invalidateCache(key);
        return null;
      }
      const now = Date.now();
      if (now > parsed.metadata.expiresAt) {
        console.log(`[CACHE] Cache expired: ${key} (expired ${Math.round((now - parsed.metadata.expiresAt) / 1e3)}s ago)`);
        return null;
      }
      const ageSeconds = Math.round((now - parsed.metadata.timestamp) / 1e3);
      console.log(`[CACHE] Cache hit: ${key} (age: ${ageSeconds}s)`);
      return parsed;
    } catch (error) {
      console.error(`[CACHE] Error reading cache file: ${key}`, error);
      try {
        await this.invalidateCache(key);
      } catch (deleteError) {
        console.error(`[CACHE] Failed to delete corrupted cache file: ${key}`, deleteError);
      }
      return null;
    }
  }
  async saveToCache(key, data) {
    if (!this.initialized) {
      console.warn(`[CACHE] Cache not initialized, skipping save for key: ${key}`);
      return;
    }
    const filePath = this.getCacheFilePath(key);
    const now = Date.now();
    const cached = {
      data,
      metadata: {
        timestamp: now,
        expiresAt: now + CACHE_TTL_MS
      }
    };
    try {
      await writeFile(filePath, JSON.stringify(cached, null, 2), "utf-8");
      console.log(`[CACHE] Saved to cache: ${key}`);
    } catch (error) {
      console.error(`[CACHE] Error writing cache file: ${key}`, error);
      throw error;
    }
  }
  async invalidateCache(key) {
    const filePath = this.getCacheFilePath(key);
    if (!existsSync(filePath)) {
      console.log(`[CACHE] Cache already invalid or missing: ${key}`);
      return;
    }
    try {
      await unlink(filePath);
      console.log(`[CACHE] Invalidated cache: ${key}`);
    } catch (error) {
      console.error(`[CACHE] Error deleting cache file: ${key}`, error);
      throw error;
    }
  }
  async deleteAll() {
    if (!this.initialized) {
      console.warn(`[CACHE] Cache not initialized, skipping deleteAll`);
      return;
    }
    try {
      const files = await readdir(this.cacheDir);
      for (const file of files) {
        const filePath = join(this.cacheDir, file);
        try {
          await unlink(filePath);
          console.log(`[CACHE] Deleted cache file: ${file}`);
        } catch (err) {
          console.error(`[CACHE] Error deleting cache file: ${file}`, err);
        }
      }
      console.log(`[CACHE] Cleared all cache files in ${this.cacheDir}`);
      const thumbsDir = join(app.getPath("userData"), THUMBNAILS_DIRECTORY);
      try {
        await rm(thumbsDir, { recursive: true, force: true });
        console.log(`[CACHE] Removed thumbnail cache directory: ${thumbsDir}`);
      } catch (err) {
        console.warn(`[CACHE] Failed to remove thumbnail cache directory: ${thumbsDir}`, err);
      }
    } catch (error) {
      console.error(`[CACHE] Error clearing all cache:`, error);
      throw error;
    }
  }
  isExpired(timestamp) {
    const ageMs = Date.now() - timestamp;
    return ageMs > CACHE_TTL_MS;
  }
  getCacheTtlMs() {
    return CACHE_TTL_MS;
  }
}
const cacheManager = new CacheManager();
class CacheEventBus extends EventEmitter {
  emit(event, ...args) {
    return super.emit(event, ...args);
  }
}
const cacheEventBus = new CacheEventBus();
class RefreshScheduler {
  refreshTimer = null;
  isRefreshing = false;
  authContext = null;
  fetchMainGames = null;
  fetchLibraryGames = null;
  fetchPublicGames = null;
  refreshIntervalMs = 12 * 60 * 60 * 1e3;
  initialize(fetchMainGames2, fetchLibraryGames2, fetchPublicGames2) {
    this.fetchMainGames = fetchMainGames2;
    this.fetchLibraryGames = fetchLibraryGames2;
    this.fetchPublicGames = fetchPublicGames2;
    console.log(`[CACHE] RefreshScheduler initialized (interval: ${this.refreshIntervalMs / 6e4} minutes)`);
  }
  updateAuthContext(token, providerStreamingBaseUrl) {
    this.authContext = { token, providerStreamingBaseUrl };
    console.log(`[CACHE] Auth context updated for refresh scheduler`);
  }
  start() {
    if (this.refreshTimer) {
      console.warn(`[CACHE] RefreshScheduler already started`);
      return;
    }
    if (!this.fetchMainGames || !this.fetchLibraryGames || !this.fetchPublicGames) {
      console.error(`[CACHE] Cannot start RefreshScheduler: fetch functions not initialized`);
      return;
    }
    console.log(`[CACHE] Starting RefreshScheduler`);
    this.performRefresh();
    this.refreshTimer = setInterval(() => {
      void this.performRefresh();
    }, this.refreshIntervalMs);
    this.refreshTimer.unref?.();
  }
  stop() {
    if (!this.refreshTimer) {
      console.log(`[CACHE] RefreshScheduler already stopped`);
      return;
    }
    clearInterval(this.refreshTimer);
    this.refreshTimer = null;
    console.log(`[CACHE] RefreshScheduler stopped`);
  }
  async performRefresh() {
    if (this.isRefreshing) {
      console.log(`[CACHE] Refresh already in progress, skipping`);
      return;
    }
    if (!this.authContext) {
      console.log(`[CACHE] Auth context not available, skipping refresh`);
      return;
    }
    if (!this.fetchMainGames || !this.fetchLibraryGames || !this.fetchPublicGames) {
      console.error(`[CACHE] Fetch functions not available`);
      return;
    }
    this.isRefreshing = true;
    const startTime = Date.now();
    console.log(`[CACHE] Refresh cycle started`);
    try {
      cacheEventBus.emit("cache:refresh-start");
      const shouldRefreshLibrary = !await cacheManager.loadFromCache("games:library");
      if (!shouldRefreshLibrary) {
        console.log("[CACHE] Skipping library refresh; cached library is still fresh");
      }
      const refreshTasks = [
        this.fetchMainGames(this.authContext.token, this.authContext.providerStreamingBaseUrl),
        shouldRefreshLibrary ? this.fetchLibraryGames(this.authContext.token, this.authContext.providerStreamingBaseUrl) : Promise.resolve([]),
        this.fetchPublicGames()
      ];
      const results = await Promise.allSettled(refreshTasks);
      let hasErrors = false;
      for (let i = 0; i < results.length; i++) {
        const result = results[i];
        const name = ["main", "library", "public"][i];
        if (result.status === "rejected") {
          hasErrors = true;
          console.error(`[CACHE] Refresh failed for ${name} games:`, result.reason);
          cacheEventBus.emit("cache:refresh-error", {
            key: `games:${name}`,
            error: result.reason instanceof Error ? result.reason.message : String(result.reason)
          });
        }
      }
      const duration = Date.now() - startTime;
      console.log(`[CACHE] Refresh cycle completed in ${duration}ms`);
      if (!hasErrors) {
        cacheEventBus.emit("cache:refresh-success");
      }
    } catch (error) {
      console.error(`[CACHE] Refresh cycle error:`, error);
      cacheEventBus.emit("cache:refresh-error", {
        key: "refresh-cycle",
        error: error instanceof Error ? error.message : "Unknown error"
      });
    } finally {
      this.isRefreshing = false;
    }
  }
  async manualRefresh() {
    console.log(`[CACHE] Manual refresh requested`);
    await this.performRefresh();
  }
  setRefreshInterval(intervalMs) {
    console.log(`[CACHE] Refresh interval updated: ${this.refreshIntervalMs}ms -> ${intervalMs}ms`);
    this.refreshIntervalMs = intervalMs;
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer);
      this.refreshTimer = setInterval(() => {
        void this.performRefresh();
      }, this.refreshIntervalMs);
      this.refreshTimer.unref?.();
    }
  }
}
const refreshScheduler = new RefreshScheduler();
const NATIVE_STREAMER_WINDOWS_ONLY_MESSAGE = "experimental feature: Windows only. Mac and Linux support is being worked on";
function isNativeStreamerSupportedPlatform(platform) {
  const normalized = platform.toLowerCase();
  return normalized === "win32" || normalized.startsWith("win") || normalized.includes("windows");
}
function normalizeStreamClientModeForPlatform(mode, platform) {
  return mode === "native" && !isNativeStreamerSupportedPlatform(platform) ? "web" : mode;
}
function nativeStreamerFeatureModeToEnvValue(mode) {
  switch (mode) {
    case "disabled":
      return "0";
    case "forced":
      return "1";
    default:
      return "auto";
  }
}
const DEFAULT_KEYBOARD_LAYOUT = "en-US";
const keyboardLayoutOptions = [
  { value: "en-US", label: "English (US)", macValue: "m-us" },
  { value: "en-GB", label: "English (UK)", macValue: "m-brit" },
  { value: "tr-TR", label: "Turkish Q", macValue: "m-tr-qty" },
  { value: "de-DE", label: "German" },
  { value: "fr-FR", label: "French" },
  { value: "es-ES", label: "Spanish" },
  { value: "es-MX", label: "Spanish (Latin America)" },
  { value: "it-IT", label: "Italian" },
  { value: "pt-PT", label: "Portuguese (Portugal)" },
  { value: "pt-BR", label: "Portuguese (Brazil)" },
  { value: "pl-PL", label: "Polish" },
  { value: "ru-RU", label: "Russian" },
  { value: "ja-JP", label: "Japanese" },
  { value: "ko-KR", label: "Korean" },
  { value: "zh-CN", label: "Chinese (Simplified)" },
  { value: "zh-TW", label: "Chinese (Traditional)" }
];
function resolveGfnKeyboardLayout(layout, platform) {
  const option = keyboardLayoutOptions.find((candidate) => candidate.value === layout);
  if (platform === "darwin" && option?.macValue) {
    return option.macValue;
  }
  return option?.value ?? DEFAULT_KEYBOARD_LAYOUT;
}
function colorQualityBitDepth(cq) {
  return cq.startsWith("10bit") ? 10 : 0;
}
function colorQualityChromaFormat(cq) {
  return cq.endsWith("444") ? 2 : 0;
}
const USER_FACING_VIDEO_CODEC_OPTIONS = ["H264", "H265", "AV1"];
const USER_FACING_COLOR_QUALITY_OPTIONS = ["8bit_420", "8bit_444", "10bit_420", "10bit_444"];
function isSupportedUserFacingCodec(codec) {
  return USER_FACING_VIDEO_CODEC_OPTIONS.includes(codec);
}
function normalizeStreamPreferences(codec, colorQuality) {
  const normalizedCodec = isSupportedUserFacingCodec(codec) ? codec : USER_FACING_VIDEO_CODEC_OPTIONS[0];
  const normalizedColorQuality = USER_FACING_COLOR_QUALITY_OPTIONS.includes(colorQuality) ? colorQuality : USER_FACING_COLOR_QUALITY_OPTIONS[0];
  return {
    codec: normalizedCodec,
    colorQuality: normalizedColorQuality,
    migrated: normalizedCodec !== codec || normalizedColorQuality !== colorQuality
  };
}
function createUnsupportedNativeStreamerStatus() {
  return {
    detected: false,
    gstreamerAvailable: false,
    supportsOfferAnswer: false,
    gstreamerRuntime: {
      source: "unknown",
      bundled: false,
      message: NATIVE_STREAMER_WINDOWS_ONLY_MESSAGE
    },
    message: NATIVE_STREAMER_WINDOWS_ONLY_MESSAGE
  };
}
const DEFAULT_STREAM_PREFERENCES$1 = Object.freeze({
  codec: "H264",
  colorQuality: "10bit_420"
});
function getDefaultStreamPreferences() {
  const normalized = normalizeStreamPreferences(
    DEFAULT_STREAM_PREFERENCES$1.codec,
    DEFAULT_STREAM_PREFERENCES$1.colorQuality
  );
  return {
    codec: normalized.codec,
    colorQuality: normalized.colorQuality
  };
}
const OWNED_LIBRARY_STATUSES = ["MANUAL", "PLATFORM_SYNC", "IN_LIBRARY"];
function normalizeGameStore(store) {
  return store.toUpperCase().replace(/[\s-]+/g, "_");
}
function isOwnedLibraryStatus(status) {
  return typeof status === "string" && OWNED_LIBRARY_STATUSES.includes(status);
}
const GFN_USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36 NVIDIACEFClient/HEAD/debb5919f6 GFN-PC/2.0.80.173";
const GFN_CLIENT_VERSION = "2.0.80.173";
const LCARS_CLIENT_ID = "ec7e38d4-03af-4b58-b131-cfb0495903ab";
const GFN_PLAY_ORIGIN = "https://play.geforcenow.com";
const GFN_PLAY_REFERER = "https://play.geforcenow.com/";
const NVIDIA_FILE_ORIGIN = "https://nvfile";
const NVIDIA_FILE_REFERER = "https://nvfile/";
function gfnJwtAuthorization(token) {
  return `GFNJWT ${token}`;
}
function bearerAuthorization(token) {
  return `Bearer ${token}`;
}
function platformToGfnDeviceOs(platform = process.platform) {
  if (platform === "win32") {
    return "WINDOWS";
  }
  if (platform === "darwin") {
    return "MACOS";
  }
  return "LINUX";
}
function buildNvidiaAuthHeaders(options = {}) {
  const headers = {};
  if (options.bearerToken !== void 0) {
    headers.Authorization = bearerAuthorization(options.bearerToken);
  }
  if (options.contentType) {
    headers["Content-Type"] = options.contentType;
  }
  headers.Origin = NVIDIA_FILE_ORIGIN;
  if (options.includeReferer) {
    headers.Referer = NVIDIA_FILE_REFERER;
  }
  headers.Accept = options.accept ?? "application/json, text/plain, */*";
  headers["User-Agent"] = GFN_USER_AGENT;
  return headers;
}
function buildGfnLcarsHeaders(options) {
  const headers = {
    Accept: options.accept ?? "application/json"
  };
  if (options.token || options.includeEmptyTokenAuthorization && options.token !== void 0) {
    headers.Authorization = gfnJwtAuthorization(options.token);
  }
  headers["nv-client-id"] = options.clientId ?? LCARS_CLIENT_ID;
  headers["nv-client-type"] = options.clientType;
  headers["nv-client-version"] = GFN_CLIENT_VERSION;
  headers["nv-client-streamer"] = options.clientStreamer;
  headers["nv-device-os"] = options.deviceOs ?? "WINDOWS";
  headers["nv-device-type"] = "DESKTOP";
  if (options.includeUserAgent) {
    headers["User-Agent"] = GFN_USER_AGENT;
  }
  return headers;
}
function buildGfnGraphQlHeaders(token) {
  return {
    Accept: "application/json, text/plain, */*",
    "Content-Type": "application/json",
    Origin: GFN_PLAY_ORIGIN,
    Referer: GFN_PLAY_REFERER,
    ...token ? { Authorization: gfnJwtAuthorization(token) } : {},
    "nv-client-id": LCARS_CLIENT_ID,
    "nv-client-type": "NATIVE",
    "nv-client-version": GFN_CLIENT_VERSION,
    "nv-client-streamer": "NVIDIA-CLASSIC",
    "nv-device-os": "WINDOWS",
    "nv-device-type": "DESKTOP",
    "nv-device-make": "UNKNOWN",
    "nv-device-model": "UNKNOWN",
    "nv-browser-type": "CHROME",
    "User-Agent": GFN_USER_AGENT
  };
}
function resolveCloudMatchIdentity(options) {
  return {
    clientId: options.clientId ?? crypto.randomUUID(),
    deviceId: options.deviceId ?? crypto.randomUUID()
  };
}
function buildGfnCloudMatchHeaders(options) {
  const { clientId, deviceId } = resolveCloudMatchIdentity(options);
  const headers = {
    "User-Agent": GFN_USER_AGENT,
    Authorization: gfnJwtAuthorization(options.token),
    "Content-Type": "application/json",
    "nv-browser-type": "CHROME",
    "nv-client-id": clientId,
    "nv-client-streamer": "NVIDIA-CLASSIC",
    "nv-client-type": "NATIVE",
    "nv-client-version": GFN_CLIENT_VERSION,
    "nv-device-make": "UNKNOWN",
    "nv-device-model": "UNKNOWN",
    "nv-device-os": platformToGfnDeviceOs(),
    "nv-device-type": "DESKTOP",
    "x-device-id": deviceId
  };
  if (options.includeOrigin !== false) {
    headers.Origin = GFN_PLAY_ORIGIN;
    headers.Referer = GFN_PLAY_REFERER;
  }
  return headers;
}
function buildGfnCloudMatchClaimHeaders(options) {
  const { clientId, deviceId } = resolveCloudMatchIdentity(options);
  return {
    "User-Agent": GFN_USER_AGENT,
    Authorization: gfnJwtAuthorization(options.token),
    "Content-Type": "application/json",
    Origin: GFN_PLAY_ORIGIN,
    Referer: GFN_PLAY_REFERER,
    "nv-client-id": clientId,
    "nv-client-streamer": "NVIDIA-CLASSIC",
    "nv-client-type": "NATIVE",
    "nv-client-version": GFN_CLIENT_VERSION,
    "nv-device-os": platformToGfnDeviceOs(),
    "nv-device-type": "DESKTOP",
    "x-device-id": deviceId
  };
}
const PRIMARY_CATALOG_STORE_KEYS = /* @__PURE__ */ new Set([
  "STEAM",
  "EPIC",
  "EPIC_GAMES_STORE",
  "EGS",
  "XBOX",
  "XBOX_GAME_PASS",
  "MICROSOFT",
  "MICROSOFT_STORE"
]);
function inferPublicGameStore(item) {
  const explicitStore = item.store?.trim();
  if (explicitStore) {
    return explicitStore;
  }
  const publisher = item.publisher?.trim();
  if (publisher) {
    const publisherName = publisher.toLowerCase();
    if (publisherName.includes("ncsoft")) {
      return "NCSoft";
    }
  }
  return "Unknown";
}
function isNumericId$1(value) {
  if (!value) {
    return false;
  }
  return /^\d+$/.test(value);
}
function publicGameToGameInfo(item) {
  const id = String(item.id ?? item.title ?? "unknown");
  const steamAppId = item.steamUrl?.split("/app/")[1]?.split("/")[0];
  const imageUrl = steamAppId ? `https://cdn.cloudflare.steamstatic.com/steam/apps/${steamAppId}/library_600x900.jpg` : void 0;
  const store = inferPublicGameStore(item);
  return {
    id,
    uuid: id,
    launchAppId: isNumericId$1(id) ? id : void 0,
    title: item.title ?? id,
    searchText: [item.title ?? id, item.store, item.publisher].filter((value) => typeof value === "string" && value.trim().length > 0).join(" ").toLowerCase(),
    selectedVariantIndex: 0,
    variants: [{ id, store, supportedControls: [] }],
    imageUrl,
    availableStores: [store],
    isInLibrary: false
  };
}
function normalizeTitleKey(title) {
  return title.trim().toLowerCase().replace(/[^a-z0-9]+/g, " ").trim();
}
function mergeSearchText(left, right) {
  const merged = [left, right].filter((value) => typeof value === "string" && value.trim().length > 0).join(" ").trim();
  return merged || void 0;
}
function getSupplementalPublicVariants(game, publicGame) {
  const existingStores = new Set(game.variants.map((variant) => normalizeGameStore(variant.store)));
  return publicGame.variants.filter((variant) => {
    const storeKey = normalizeGameStore(variant.store);
    return !PRIMARY_CATALOG_STORE_KEYS.has(storeKey) && !existingStores.has(storeKey);
  });
}
function mergePublicGameVariants(games, publicGames) {
  const publicGameByTitle = /* @__PURE__ */ new Map();
  for (const publicGame of publicGames) {
    const titleKey = normalizeTitleKey(publicGame.title);
    if (titleKey && !publicGameByTitle.has(titleKey)) {
      publicGameByTitle.set(titleKey, publicGame);
    }
  }
  return games.map((game) => {
    const publicGame = publicGameByTitle.get(normalizeTitleKey(game.title));
    if (!publicGame) {
      return game;
    }
    const supplementalVariants = getSupplementalPublicVariants(game, publicGame);
    if (supplementalVariants.length === 0) {
      return game;
    }
    return {
      ...game,
      uuid: game.uuid ?? publicGame.uuid,
      launchAppId: game.launchAppId ?? publicGame.launchAppId,
      imageUrl: game.imageUrl ?? publicGame.imageUrl,
      variants: [...game.variants, ...supplementalVariants],
      availableStores: [
        ...new Set([
          ...game.availableStores ?? [],
          ...supplementalVariants.map((variant) => variant.store),
          ...publicGame.availableStores ?? []
        ].filter((value) => typeof value === "string" && value.trim().length > 0))
      ],
      searchText: mergeSearchText(game.searchText, publicGame.searchText)
    };
  });
}
async function fetchPublicGamesUncached() {
  const response = await fetch(
    "https://static.nvidiagrid.net/supported-public-game-list/locales/gfnpc-en-US.json",
    {
      headers: {
        "User-Agent": GFN_USER_AGENT
      }
    }
  );
  if (!response.ok) {
    throw new Error(`Public games fetch failed (${response.status})`);
  }
  const payload = await response.json();
  return payload.filter((item) => item.status === "AVAILABLE" && item.title).map(publicGameToGameInfo);
}
const GRAPHQL_URL = "https://games.geforce.com/graphql";
const PANELS_QUERY_HASH = "f8e26265a5db5c20e1334a6872cf04b6e3970507697f6ae55a6ddefa5420daf0";
const APP_METADATA_QUERY_HASH = "39187e85b6dcf60b7279a5f233288b0a8b69a8b1dbcfb5b25555afdcb988f0d7";
const LIBRARY_WITH_TIME_QUERY_HASH = "039e8c0d553972975485fee56e59f2549d2fdb518e247a42ab5022056a74406f";
const DEFAULT_LOCALE = "en_US";
const DEFAULT_CATALOG_FETCH_COUNT = 120;
const MAX_CATALOG_PAGES = 3;
const DEFAULT_SORT_ID = "relevance";
const PUBLIC_GAMES_CACHE_KEY = "games:public:v2";
function optimizeImage(url) {
  if (url.includes("img.nvidiagrid.net")) {
    return `${url};f=webp;w=272`;
  }
  return url;
}
function isNumericId(value) {
  if (!value) {
    return false;
  }
  return /^\d+$/.test(value);
}
function randomHuId() {
  return `${Date.now().toString(16)}${Math.random().toString(16).slice(2)}`;
}
async function postGraphQl(query, variables, token) {
  const response = await fetch(GRAPHQL_URL, {
    method: "POST",
    headers: buildGfnGraphQlHeaders(token),
    body: JSON.stringify({ query, variables })
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`GFN GraphQL failed (${response.status}): ${text.slice(0, 400)}`);
  }
  return await response.json();
}
async function getVpcId(token, providerStreamingBaseUrl) {
  const base = providerStreamingBaseUrl?.trim() || "https://prod.cloudmatchbeta.nvidiagrid.net/";
  const normalizedBase = base.endsWith("/") ? base : `${base}/`;
  const response = await fetch(`${normalizedBase}v2/serverInfo`, {
    headers: buildGfnLcarsHeaders({
      token,
      clientType: "NATIVE",
      clientStreamer: "NVIDIA-CLASSIC",
      includeUserAgent: true,
      includeEmptyTokenAuthorization: true
    })
  });
  if (!response.ok) {
    return "GFN-PC";
  }
  const payload = await response.json();
  return payload.requestStatus?.serverId ?? "GFN-PC";
}
function parseFeatureLabel(value) {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : null;
  }
  if (value && typeof value === "object") {
    const candidate = value;
    const keys = ["name", "label", "title", "displayName"];
    for (const key of keys) {
      const raw = candidate[key];
      if (typeof raw === "string") {
        const trimmed = raw.trim();
        if (trimmed.length > 0) {
          return trimmed;
        }
      }
    }
  }
  return null;
}
function extractFeatureLabels(app2) {
  const buckets = [
    app2.features,
    app2.gameFeatures,
    app2.appFeatures,
    app2.genres,
    app2.tags,
    app2.gfn?.catalogSkuStrings?.SKU_BASED_TAG
  ];
  const labels = [];
  for (const bucket of buckets) {
    if (!Array.isArray(bucket)) {
      continue;
    }
    for (const entry of bucket) {
      const label = parseFeatureLabel(entry);
      if (label) {
        labels.push(label);
      }
    }
  }
  return [...new Set(labels)];
}
function extractGenres(app2) {
  if (!Array.isArray(app2.genres)) {
    return [];
  }
  const genres = [];
  for (const entry of app2.genres) {
    const genre = parseFeatureLabel(entry);
    if (genre) {
      genres.push(genre);
    }
  }
  return [...new Set(genres)];
}
function extractContentRatings(app2) {
  if (!Array.isArray(app2.contentRatings)) {
    return [];
  }
  const labels = [];
  for (const entry of app2.contentRatings) {
    const label = parseFeatureLabel(entry);
    if (label) {
      labels.push(label);
    }
  }
  return [...new Set(labels)];
}
function buildSearchText(title, variants, genres, featureLabels, publisherName) {
  const stores = variants.map((variant) => variant.store);
  return [title, publisherName, ...stores, ...genres, ...featureLabels].filter((value) => typeof value === "string" && value.trim().length > 0).join(" ").toLowerCase();
}
function matchesPublicGameSearch(game, query) {
  const normalizedQuery = query.trim().toLowerCase();
  if (!normalizedQuery) {
    return true;
  }
  return [
    game.title,
    game.searchText,
    ...game.availableStores ?? [],
    ...game.variants.map((variant) => variant.store)
  ].filter((value) => typeof value === "string" && value.trim().length > 0).some((value) => value.toLowerCase().includes(normalizedQuery));
}
function resolveAppData(app2) {
  const variants = app2.variants ?? [];
  const selectedVariantIndex = variants.findIndex((variant) => variant.gfn?.library?.selected === true);
  const preferredVariant = selectedVariantIndex >= 0 ? variants[selectedVariantIndex] : void 0;
  const numericVariants = variants.filter((variant) => isNumericId(variant.id));
  const preferredNumericVariant = preferredVariant && isNumericId(preferredVariant.id) ? preferredVariant.id : void 0;
  const fallbackNumericVariant = numericVariants[0]?.id;
  const numericAppId = preferredNumericVariant ?? fallbackNumericVariant ?? (isNumericId(app2.id) ? app2.id : void 0);
  const preferredVariantId = preferredVariant?.id ?? numericAppId ?? variants[0]?.id ?? app2.id;
  const lastPlayed = variants.map((variant) => variant.gfn?.library?.lastPlayedDate).find((value) => typeof value === "string" && value.length > 0);
  const isInLibrary = variants.some((variant) => isOwnedLibraryStatus(variant.gfn?.library?.status));
  return {
    numericAppId,
    preferredVariantId,
    selectedVariantIndex: selectedVariantIndex >= 0 ? selectedVariantIndex : Math.max(0, variants.findIndex((variant) => variant.id === preferredVariantId)),
    lastPlayed,
    isInLibrary
  };
}
function appToVariants(app2) {
  return app2.variants?.map((variant) => ({
    id: variant.id,
    store: variant.appStore,
    supportedControls: variant.supportedControls ?? [],
    librarySelected: variant.gfn?.library?.selected,
    libraryStatus: variant.gfn?.library?.status,
    lastPlayedDate: variant.gfn?.library?.lastPlayedDate,
    gfnStatus: variant.gfn?.status
  })) ?? [];
}
function appToGame(app2) {
  const variants = appToVariants(app2);
  const resolution = resolveAppData(app2);
  const imageUrl = app2.images?.KEY_ART ?? app2.images?.GAME_BOX_ART ?? app2.images?.TV_BANNER ?? app2.images?.HERO_IMAGE ?? void 0;
  const genres = extractGenres(app2);
  const featureLabels = extractFeatureLabels(app2);
  return {
    id: app2.id,
    uuid: app2.id,
    launchAppId: resolution.numericAppId,
    title: app2.title,
    description: app2.description,
    longDescription: app2.longDescription,
    featureLabels,
    genres,
    imageUrl: imageUrl ? optimizeImage(imageUrl) : void 0,
    playType: app2.gfn?.playType,
    membershipTierLabel: app2.gfn?.minimumMembershipTierLabel,
    catalogSkuStrings: app2.gfn?.catalogSkuStrings,
    publisherName: app2.publisherName,
    contentRatings: extractContentRatings(app2),
    playabilityState: app2.gfn?.playabilityState,
    availableStores: [...new Set(variants.map((variant) => variant.store).filter(Boolean))],
    searchText: buildSearchText(app2.title, variants, genres, featureLabels, app2.publisherName),
    lastPlayed: resolution.lastPlayed,
    isInLibrary: resolution.isInLibrary,
    selectedVariantIndex: Math.max(0, Math.min(resolution.selectedVariantIndex, Math.max(variants.length - 1, 0))),
    variants
  };
}
function mergeAppMetaIntoGame(game, app2) {
  const merged = appToGame(app2);
  const selectedVariantId = game.variants[game.selectedVariantIndex]?.id;
  const selectedVariantIndex = selectedVariantId ? merged.variants.findIndex((variant) => variant.id === selectedVariantId) : -1;
  return {
    ...game,
    ...merged,
    id: game.id,
    selectedVariantIndex: selectedVariantIndex >= 0 ? selectedVariantIndex : merged.selectedVariantIndex
  };
}
function dedupeGames(games) {
  const byId = /* @__PURE__ */ new Map();
  for (const game of games) {
    const existing = byId.get(game.id);
    if (!existing) {
      byId.set(game.id, game);
      continue;
    }
    const mergedVariants = /* @__PURE__ */ new Map();
    for (const variant of [...existing.variants, ...game.variants]) {
      mergedVariants.set(variant.id, variant);
    }
    const merged = {
      ...existing,
      ...game,
      id: existing.id,
      uuid: existing.uuid ?? game.uuid,
      launchAppId: existing.launchAppId ?? game.launchAppId,
      title: existing.title || game.title,
      description: existing.description ?? game.description,
      longDescription: existing.longDescription ?? game.longDescription,
      imageUrl: existing.imageUrl ?? game.imageUrl,
      playType: existing.playType ?? game.playType,
      membershipTierLabel: existing.membershipTierLabel ?? game.membershipTierLabel,
      catalogSkuStrings: existing.catalogSkuStrings ?? game.catalogSkuStrings,
      publisherName: existing.publisherName ?? game.publisherName,
      playabilityState: existing.playabilityState ?? game.playabilityState,
      lastPlayed: existing.lastPlayed ?? game.lastPlayed,
      isInLibrary: existing.isInLibrary || game.isInLibrary,
      variants: [...mergedVariants.values()],
      genres: [.../* @__PURE__ */ new Set([...existing.genres ?? [], ...game.genres ?? []])],
      featureLabels: [.../* @__PURE__ */ new Set([...existing.featureLabels ?? [], ...game.featureLabels ?? []])],
      contentRatings: [.../* @__PURE__ */ new Set([...existing.contentRatings ?? [], ...game.contentRatings ?? []])],
      availableStores: [.../* @__PURE__ */ new Set([...existing.availableStores ?? [], ...game.availableStores ?? []])],
      searchText: [existing.searchText, game.searchText].filter(Boolean).join(" ").trim() || void 0,
      selectedVariantIndex: Math.max(0, existing.variants[existing.selectedVariantIndex] ? [...mergedVariants.values()].findIndex((variant) => variant.id === existing.variants[existing.selectedVariantIndex]?.id) : game.selectedVariantIndex)
    };
    byId.set(game.id, merged);
  }
  return [...byId.values()];
}
async function fetchAppMetaData(token, appIds, vpcId) {
  const normalizedIds = [...new Set(appIds.map((id) => id.trim()).filter((id) => id.length > 0))];
  if (normalizedIds.length === 0) {
    return { data: { apps: { items: [] } } };
  }
  const variables = JSON.stringify({
    vpcId,
    locale: DEFAULT_LOCALE,
    appIds: normalizedIds
  });
  const extensions = JSON.stringify({
    persistedQuery: {
      sha256Hash: APP_METADATA_QUERY_HASH
    }
  });
  const params = new URLSearchParams({
    requestType: "appMetaData",
    extensions,
    huId: randomHuId(),
    variables
  });
  const response = await fetch(`${GRAPHQL_URL}?${params.toString()}`, {
    headers: {
      ...buildGfnGraphQlHeaders(token),
      "Content-Type": "application/graphql"
    }
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`App metadata failed (${response.status}): ${text.slice(0, 400)}`);
  }
  return await response.json();
}
async function enrichGamesWithMetadata(token, vpcId, games) {
  const uuids = [...new Set(games.map((game) => game.uuid).filter((uuid) => !!uuid))];
  if (uuids.length === 0) {
    return games;
  }
  const chunkSize = 40;
  const appById = /* @__PURE__ */ new Map();
  for (let index = 0; index < uuids.length; index += chunkSize) {
    const chunk = uuids.slice(index, index + chunkSize);
    const payload = await fetchAppMetaData(token, chunk, vpcId);
    if (payload.errors?.length) {
      throw new Error(payload.errors.map((error) => error.message).join(", "));
    }
    for (const app2 of payload.data?.apps.items ?? []) {
      appById.set(app2.id, app2);
    }
  }
  return dedupeGames(
    games.map((game) => {
      const metadata = game.uuid ? appById.get(game.uuid) : void 0;
      return metadata ? mergeAppMetaIntoGame(game, metadata) : game;
    })
  );
}
async function fetchPanels(token, panelNames, vpcId, options) {
  const variables = JSON.stringify({
    vpcId,
    locale: DEFAULT_LOCALE,
    panelNames
  });
  const extensions = JSON.stringify({
    persistedQuery: {
      sha256Hash: options?.withLibraryTime ? LIBRARY_WITH_TIME_QUERY_HASH : PANELS_QUERY_HASH
    }
  });
  const requestType = panelNames.includes("LIBRARY") ? "panels/Library" : "panels/MainV2";
  const params = new URLSearchParams({
    requestType,
    extensions,
    huId: randomHuId(),
    variables
  });
  const response = await fetch(`${GRAPHQL_URL}?${params.toString()}`, {
    headers: {
      ...buildGfnGraphQlHeaders(token),
      "Content-Type": "application/graphql"
    }
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Games GraphQL failed (${response.status}): ${text.slice(0, 400)}`);
  }
  return await response.json();
}
function flattenPanels(payload) {
  if (payload.errors?.length) {
    throw new Error(payload.errors.map((error) => error.message).join(", "));
  }
  const games = [];
  for (const panel of payload.data?.panels ?? []) {
    for (const section of panel.sections ?? []) {
      for (const item of section.items ?? []) {
        if (item.__typename === "GameItem" && item.app) {
          games.push(appToGame(item.app));
        }
      }
    }
  }
  return dedupeGames(games);
}
async function fetchFilterAndSortDefinitions(token) {
  const query = `query GetFilterGroupAndSortOrderDefinitions($locale: String!) {
    filterGroupDefinitions(language: $locale) {
      id
      label
      filters {
        id
        label
        filters
      }
    }
    sortOrderDefinitions(language: $locale) {
      id
      label
      orderBy
    }
  }`;
  const payload = await postGraphQl(query, { locale: DEFAULT_LOCALE }, token);
  if (payload.errors?.length) {
    throw new Error(payload.errors.map((error) => error.message).join(", "));
  }
  const filterPayloadById = {};
  const filterGroups = [];
  for (const group of payload.data?.filterGroupDefinitions ?? []) {
    const options = (group.filters ?? []).flatMap((entry) => {
      const filterJson = entry.filters?.[0];
      if (!filterJson) {
        return [];
      }
      try {
        filterPayloadById[entry.id] = JSON.parse(filterJson);
        return [{
          id: entry.id,
          rawId: entry.id,
          label: entry.label,
          groupId: group.id,
          groupLabel: group.label
        }];
      } catch {
        return [];
      }
    });
    if (options.length > 0) {
      filterGroups.push({ id: group.id, label: group.label, options });
    }
  }
  const sortOptions = (payload.data?.sortOrderDefinitions ?? []).map((sort) => ({
    id: sort.id,
    label: sort.label,
    orderBy: sort.orderBy
  }));
  return {
    filterGroups,
    sortOptions,
    filterPayloadById
  };
}
function mergeFilterPayloads(filterIds, filterPayloadById) {
  const merged = {};
  for (const filterId of filterIds) {
    const payload = filterPayloadById[filterId];
    if (!payload || typeof payload !== "object") {
      continue;
    }
    Object.assign(merged, payload);
  }
  return merged;
}
async function browseCatalogUncached(input) {
  const token = input.token;
  if (!token) {
    throw new Error("Catalog browsing requires an authenticated token");
  }
  const vpcId = await getVpcId(token, input.providerStreamingBaseUrl);
  const definitions = await fetchFilterAndSortDefinitions(token);
  const normalizedFilterIds = (input.filterIds ?? []).filter((id) => id in definitions.filterPayloadById);
  const selectedSort = definitions.sortOptions.find((option) => option.id === input.sortId) ?? definitions.sortOptions.find((option) => option.id === DEFAULT_SORT_ID) ?? definitions.sortOptions[0] ?? { id: DEFAULT_SORT_ID, orderBy: "itemMetadata.relevance:DESC,sortName:ASC" };
  const searchQuery = input.searchQuery?.trim() ?? "";
  const fetchCount = Math.max(24, Math.min(input.fetchCount ?? DEFAULT_CATALOG_FETCH_COUNT, 200));
  const filters = mergeFilterPayloads(normalizedFilterIds, definitions.filterPayloadById);
  const appFields = `
      numberReturned
      numberSupported
      pageInfo { hasNextPage endCursor totalCount }
      items {
        id
        title
        images { KEY_ART GAME_BOX_ART TV_BANNER HERO_IMAGE }
        variants {
          id
          appStore
          supportedControls
          gfn {
            status
            library { status selected }
          }
        }
        gfn {
          playabilityState
          minimumMembershipTierLabel
          catalogSkuStrings {
            SKU_BASED_TAG
            SKU_BASED_PLAYABILITY_TEXT
            SKU_BASED_UNPLAYABLE_DIALOG_HEADER
            SKU_BASED_UNPLAYABLE_DIALOG_BODY_UPGRADE
            SKU_BASED_UNPLAYABLE_DIALOG_BODY_UPGRADE_ECOMM_RESTRICTED
          }
        }
        itemMetadata { campaignIds }
      }
  `;
  const query = searchQuery.length > 0 ? `query GetSearchFilterResults(
      $vpcId: String!,
      $locale: String!,
      $sortString: String!,
      $fetchCount: Int!,
      $cursor: String!,
      $searchString: String!,
      $filters: AppFilterFields!
    ) {
      apps(
        vpcId: $vpcId,
        language: $locale,
        orderBy: $sortString,
        first: $fetchCount,
        after: $cursor,
        searchQuery: $searchString,
        filters: $filters
      ) {
${appFields}
      }
    }` : `query GetFilterBrowseResults(
      $vpcId: String!,
      $locale: String!,
      $sortString: String!,
      $fetchCount: Int!,
      $cursor: String!,
      $filters: AppFilterFields!
    ) {
      apps(
        vpcId: $vpcId,
        language: $locale,
        orderBy: $sortString,
        first: $fetchCount,
        after: $cursor,
        filters: $filters
      ) {
${appFields}
      }
    }`;
  const collectedApps = [];
  let numberReturned = 0;
  let numberSupported = 0;
  let totalCount = 0;
  let hasNextPage = false;
  let endCursor = "";
  let cursor = "";
  for (let page = 0; page < MAX_CATALOG_PAGES; page += 1) {
    const payload = await postGraphQl(
      query,
      searchQuery.length > 0 ? {
        vpcId,
        locale: DEFAULT_LOCALE,
        sortString: selectedSort.orderBy,
        fetchCount,
        cursor,
        searchString: searchQuery,
        filters
      } : {
        vpcId,
        locale: DEFAULT_LOCALE,
        sortString: selectedSort.orderBy,
        fetchCount,
        cursor,
        filters
      },
      token
    );
    if (payload.errors?.length) {
      throw new Error(payload.errors.map((error) => error.message).join(", "));
    }
    const apps = payload.data?.apps;
    const items = apps?.items ?? [];
    collectedApps.push(...items);
    numberReturned += apps?.numberReturned ?? items.length;
    numberSupported = apps?.numberSupported ?? numberSupported;
    hasNextPage = apps?.pageInfo?.hasNextPage ?? false;
    endCursor = apps?.pageInfo?.endCursor ?? "";
    totalCount = apps?.pageInfo?.totalCount ?? totalCount;
    if (!hasNextPage || !endCursor) {
      break;
    }
    cursor = endCursor;
  }
  let games = dedupeGames(collectedApps.map(appToGame));
  const publicGames = await fetchPublicGames();
  if (searchQuery.length > 0) {
    const publicSearchMatches = publicGames.filter((game) => matchesPublicGameSearch(game, searchQuery));
    games = dedupeGames([...games, ...publicSearchMatches]);
  }
  const gamesWithPublicVariants = mergePublicGameVariants(games, publicGames);
  return {
    games: gamesWithPublicVariants,
    numberReturned,
    numberSupported: Math.max(numberSupported, gamesWithPublicVariants.length),
    totalCount: Math.max(totalCount, gamesWithPublicVariants.length),
    hasNextPage,
    endCursor: endCursor || void 0,
    searchQuery,
    selectedSortId: selectedSort.id,
    selectedFilterIds: normalizedFilterIds,
    filterGroups: definitions.filterGroups,
    sortOptions: definitions.sortOptions
  };
}
async function browseCatalog(input) {
  return browseCatalogUncached(input);
}
async function fetchMainGames(token, providerStreamingBaseUrl) {
  const cached = await cacheManager.loadFromCache("games:main");
  if (cached) {
    return mergePublicGameVariants(cached.data, await fetchPublicGames());
  }
  const games = await fetchMainGamesUncached(token, providerStreamingBaseUrl);
  await cacheManager.saveToCache("games:main", games);
  return games;
}
async function fetchMainGamesUncached(token, providerStreamingBaseUrl) {
  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  const payload = await fetchPanels(token, ["MAIN"], vpcId);
  const games = flattenPanels(payload);
  return mergePublicGameVariants(await enrichGamesWithMetadata(token, vpcId, games), await fetchPublicGames());
}
async function fetchLibraryGames(token, providerStreamingBaseUrl) {
  const cached = await cacheManager.loadFromCache("games:library");
  if (cached) {
    return mergePublicGameVariants(cached.data, await fetchPublicGames());
  }
  const games = await fetchLibraryGamesUncached(token, providerStreamingBaseUrl);
  await cacheManager.saveToCache("games:library", games);
  return games;
}
async function fetchLibraryGamesUncached(token, providerStreamingBaseUrl) {
  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  let payload;
  try {
    payload = await fetchPanels(token, ["LIBRARY"], vpcId, { withLibraryTime: true });
  } catch {
    payload = await fetchPanels(token, ["LIBRARY"], vpcId);
  }
  const games = flattenPanels(payload);
  return mergePublicGameVariants(await enrichGamesWithMetadata(token, vpcId, games), await fetchPublicGames());
}
async function fetchPublicGames() {
  const cached = await cacheManager.loadFromCache(PUBLIC_GAMES_CACHE_KEY);
  if (cached) {
    return cached.data;
  }
  const games = await fetchPublicGamesUncached();
  await cacheManager.saveToCache(PUBLIC_GAMES_CACHE_KEY, games);
  return games;
}
async function resolveLaunchAppId(token, appIdOrUuid, providerStreamingBaseUrl) {
  if (isNumericId(appIdOrUuid)) {
    return appIdOrUuid;
  }
  const vpcId = await getVpcId(token, providerStreamingBaseUrl);
  const payload = await fetchAppMetaData(token, [appIdOrUuid], vpcId);
  if (payload.errors?.length) {
    throw new Error(payload.errors.map((error) => error.message).join(", "));
  }
  const app2 = payload.data?.apps.items?.[0];
  if (!app2) {
    return null;
  }
  return resolveAppData(app2).numericAppId ?? null;
}
const defaultStopShortcut = "Ctrl+Shift+Q";
const defaultAntiAfkShortcut = "Ctrl+Shift+K";
const defaultMicShortcut = "Ctrl+Shift+M";
const LEGACY_STOP_SHORTCUTS = /* @__PURE__ */ new Set(["META+SHIFT+Q", "CMD+SHIFT+Q"]);
const LEGACY_ANTI_AFK_SHORTCUTS = /* @__PURE__ */ new Set(["META+SHIFT+F10", "CMD+SHIFT+F10", "CTRL+SHIFT+F10"]);
const DEFAULT_STREAM_PREFERENCES = getDefaultStreamPreferences();
const CONTROLLER_THEME_STYLES_SET = /* @__PURE__ */ new Set(["aurora", "nebula", "grid", "minimal", "pulse"]);
const NATIVE_VIDEO_BACKEND_PREFERENCES = /* @__PURE__ */ new Set(["auto", "d3d11", "d3d12"]);
const APP_ACCENT_COLORS = /* @__PURE__ */ new Set(["green", "blue", "violet", "amber", "rose"]);
function clampThemeByte(value) {
  const n = typeof value === "number" && Number.isFinite(value) ? Math.round(value) : NaN;
  if (!Number.isFinite(n)) return 0;
  return Math.max(0, Math.min(255, n));
}
function normalizeControllerThemeColor(raw, fallback) {
  if (!raw || typeof raw !== "object") return { ...fallback };
  const o = raw;
  return {
    r: clampThemeByte(o.r),
    g: clampThemeByte(o.g),
    b: clampThemeByte(o.b)
  };
}
function normalizeControllerThemeStyle(raw) {
  return CONTROLLER_THEME_STYLES_SET.has(raw) ? raw : "aurora";
}
function normalizeNativeVideoBackendPreference(raw) {
  return NATIVE_VIDEO_BACKEND_PREFERENCES.has(raw) ? raw : "auto";
}
function normalizeAppAccentColor(raw) {
  return APP_ACCENT_COLORS.has(raw) ? raw : "green";
}
const DEFAULT_SETTINGS = {
  resolution: "1920x1080",
  aspectRatio: "16:9",
  posterSizeScale: 1,
  fps: 60,
  maxBitrateMbps: 75,
  streamClientMode: "web",
  nativeStreamerBackend: "gstreamer",
  nativeVideoBackend: "auto",
  nativeStreamerExecutablePath: "",
  nativeCloudGsyncMode: "auto",
  nativeD3dFullscreenMode: "auto",
  nativeExternalRenderer: true,
  codec: DEFAULT_STREAM_PREFERENCES.codec,
  decoderPreference: "auto",
  encoderPreference: "auto",
  colorQuality: DEFAULT_STREAM_PREFERENCES.colorQuality,
  region: "",
  sessionProxyEnabled: false,
  sessionProxyUrl: "",
  clipboardPaste: false,
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
  controllerMode: false,
  controllerUiSounds: false,
  controllerBackgroundAnimations: false,
  controllerThemeStyle: "aurora",
  controllerThemeColor: { r: 124, g: 241, b: 177 },
  controllerLibraryGameBackdrop: true,
  autoLoadControllerLibrary: false,
  autoFullScreen: false,
  favoriteGameIds: [],
  sessionCounterEnabled: false,
  sessionClockShowEveryMinutes: 60,
  sessionClockShowDurationSeconds: 30,
  windowWidth: 1400,
  windowHeight: 900,
  keyboardLayout: DEFAULT_KEYBOARD_LAYOUT,
  gameLanguage: "en_US",
  enableL4S: false,
  enableCloudGsync: false,
  nativeTransitionDiagnostics: void 0,
  discordRichPresence: false,
  autoCheckForUpdates: true,
  allowEscapeToExitFullscreen: false
};
class SettingsManager {
  settings;
  settingsPath;
  constructor() {
    this.settingsPath = join(app.getPath("userData"), "settings.json");
    this.settings = this.load();
  }
  /**
   * Load settings from disk or return defaults if file doesn't exist
   */
  load() {
    try {
      if (!existsSync(this.settingsPath)) {
        const defaults = { ...DEFAULT_SETTINGS };
        this.enforceCompatibility(defaults);
        return defaults;
      }
      const content = readFileSync(this.settingsPath, "utf-8");
      const parsed = JSON.parse(content);
      const merged = {
        ...DEFAULT_SETTINGS,
        ...parsed
      };
      let migrated = this.migrateLegacyShortcutDefaults(merged);
      migrated = this.enforceCompatibility(merged) || migrated;
      const themeStyleBefore = merged.controllerThemeStyle;
      const themeColorBefore = { ...merged.controllerThemeColor };
      merged.controllerThemeStyle = normalizeControllerThemeStyle(merged.controllerThemeStyle);
      merged.controllerThemeColor = normalizeControllerThemeColor(merged.controllerThemeColor, DEFAULT_SETTINGS.controllerThemeColor);
      const accentColorBefore = merged.appAccentColor;
      merged.appAccentColor = normalizeAppAccentColor(merged.appAccentColor);
      if (merged.appAccentColor !== accentColorBefore || merged.controllerThemeStyle !== themeStyleBefore || merged.controllerThemeColor.r !== themeColorBefore.r || merged.controllerThemeColor.g !== themeColorBefore.g || merged.controllerThemeColor.b !== themeColorBefore.b) {
        migrated = true;
      }
      if (typeof parsed.mouseAcceleration === "boolean") {
        merged.mouseAcceleration = parsed.mouseAcceleration ? 100 : 1;
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
  enforceCompatibility(settings) {
    let migrated = false;
    const normalized = normalizeStreamPreferences(settings.codec, settings.colorQuality);
    if (normalized.migrated) {
      console.warn(
        `[Settings] Migrating unsupported stream settings codec="${settings.codec}" colorQuality="${settings.colorQuality}" to ${normalized.codec}/${normalized.colorQuality}`
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
    if (!settings.nativeExternalRenderer) {
      settings.nativeExternalRenderer = true;
      migrated = true;
    }
    const nativeVideoBackend = normalizeNativeVideoBackendPreference(settings.nativeVideoBackend);
    if (settings.nativeVideoBackend !== nativeVideoBackend) {
      settings.nativeVideoBackend = nativeVideoBackend;
      migrated = true;
    }
    return migrated;
  }
  migrateLegacyShortcutDefaults(settings) {
    let migrated = false;
    const normalizeShortcut = (value) => value.replace(/\s+/g, "").toUpperCase();
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
  save() {
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
  getAll() {
    return { ...this.settings };
  }
  /**
   * Get a specific setting value
   */
  get(key) {
    return this.settings[key];
  }
  /**
   * Update a specific setting value
   */
  set(key, value) {
    this.settings[key] = value;
    this.enforceCompatibility(this.settings);
    this.save();
  }
  /**
   * Update multiple settings at once
   */
  setMultiple(updates) {
    this.settings = {
      ...this.settings,
      ...updates
    };
    this.enforceCompatibility(this.settings);
    this.save();
  }
  /**
   * Reset all settings to defaults
   */
  reset() {
    this.settings = { ...DEFAULT_SETTINGS };
    this.enforceCompatibility(this.settings);
    this.save();
    return { ...this.settings };
  }
  /**
   * Get the default settings
   */
  getDefaults() {
    const defaults = { ...DEFAULT_SETTINGS };
    this.enforceCompatibility(defaults);
    return defaults;
  }
}
let settingsManager$1 = null;
function getSettingsManager() {
  if (!settingsManager$1) {
    settingsManager$1 = new SettingsManager();
  }
  return settingsManager$1;
}
const DEFAULT_MINIMUM_FPS_FOR_CLOUD_GSYNC = 60;
const DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR = 120;
function unsupportedNativeCloudGsyncCapabilities(reason = "unsupported") {
  return {
    platformSupportsCloudGsync: false,
    isVrrCapableDisplay: false,
    isGsyncDisplay: false,
    minimumFpsForCloudGsync: DEFAULT_MINIMUM_FPS_FOR_CLOUD_GSYNC,
    minimumFpsForReflexWithoutVrr: DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR,
    detectionSource: "unsupported",
    reason
  };
}
function normalizeCloudGsyncOverride(value) {
  if (value === "0" || value === "1") {
    return value;
  }
  if (value === "disabled") {
    return "0";
  }
  if (value === "forced") {
    return "1";
  }
  return "auto";
}
function normalizeNativeCloudGsyncCapabilities(capabilities) {
  return {
    platformSupportsCloudGsync: capabilities?.platformSupportsCloudGsync ?? false,
    isVrrCapableDisplay: capabilities?.isVrrCapableDisplay ?? false,
    isGsyncDisplay: capabilities?.isGsyncDisplay ?? false,
    minimumFpsForCloudGsync: capabilities?.minimumFpsForCloudGsync ?? DEFAULT_MINIMUM_FPS_FOR_CLOUD_GSYNC,
    minimumFpsForReflexWithoutVrr: capabilities?.minimumFpsForReflexWithoutVrr ?? DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR,
    detectionSource: capabilities?.detectionSource ?? "unsupported",
    reason: capabilities?.reason
  };
}
function resolveCloudGsync(input) {
  const override = normalizeCloudGsyncOverride(input.override);
  const capabilities = normalizeNativeCloudGsyncCapabilities(input.capabilities);
  const minimumFpsForCloudGsync = Math.max(
    0,
    capabilities.minimumFpsForCloudGsync || DEFAULT_MINIMUM_FPS_FOR_CLOUD_GSYNC
  );
  const minimumFpsForReflexWithoutVrr = Math.max(
    0,
    capabilities.minimumFpsForReflexWithoutVrr || DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR
  );
  const reflexEnabledWithoutVrr = input.fps >= minimumFpsForReflexWithoutVrr;
  if (!input.userRequested) {
    return {
      requested: false,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "user-disabled",
      capabilities
    };
  }
  if (input.clientMode === "web") {
    return {
      requested: true,
      enabled: true,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "web-mode",
      capabilities
    };
  }
  if (!input.nativeBackendAvailable) {
    return {
      requested: true,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "unsupported-backend",
      capabilities
    };
  }
  if (override === "0") {
    return {
      requested: true,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "force-disabled",
      capabilities
    };
  }
  if (input.fps < minimumFpsForCloudGsync) {
    return {
      requested: true,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "fps-too-low",
      capabilities
    };
  }
  if (override === "1") {
    return {
      requested: true,
      enabled: true,
      reflexEnabled: true,
      reason: "force-enabled",
      capabilities
    };
  }
  if (!capabilities.platformSupportsCloudGsync) {
    return {
      requested: true,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: capabilities.detectionSource === "unsupported" ? "unsupported-backend" : "detection-failed",
      capabilities
    };
  }
  if (!capabilities.isVrrCapableDisplay) {
    return {
      requested: true,
      enabled: false,
      reflexEnabled: reflexEnabledWithoutVrr,
      reason: "unsupported-display",
      capabilities
    };
  }
  return {
    requested: true,
    enabled: true,
    reflexEnabled: true,
    reason: "enabled",
    capabilities
  };
}
var GfnErrorCode = /* @__PURE__ */ ((GfnErrorCode2) => {
  GfnErrorCode2[GfnErrorCode2["Success"] = 15859712] = "Success";
  GfnErrorCode2[GfnErrorCode2["InvalidOperation"] = 3237085186] = "InvalidOperation";
  GfnErrorCode2[GfnErrorCode2["NetworkError"] = 3237089282] = "NetworkError";
  GfnErrorCode2[GfnErrorCode2["GetActiveSessionServerError"] = 3237089283] = "GetActiveSessionServerError";
  GfnErrorCode2[GfnErrorCode2["AuthTokenNotUpdated"] = 3237093377] = "AuthTokenNotUpdated";
  GfnErrorCode2[GfnErrorCode2["SessionFinishedState"] = 3237093378] = "SessionFinishedState";
  GfnErrorCode2[GfnErrorCode2["ResponseParseFailure"] = 3237093379] = "ResponseParseFailure";
  GfnErrorCode2[GfnErrorCode2["InvalidServerResponse"] = 3237093381] = "InvalidServerResponse";
  GfnErrorCode2[GfnErrorCode2["PutOrPostInProgress"] = 3237093382] = "PutOrPostInProgress";
  GfnErrorCode2[GfnErrorCode2["GridServerNotInitialized"] = 3237093383] = "GridServerNotInitialized";
  GfnErrorCode2[GfnErrorCode2["DOMExceptionInSessionControl"] = 3237093384] = "DOMExceptionInSessionControl";
  GfnErrorCode2[GfnErrorCode2["InvalidAdStateTransition"] = 3237093386] = "InvalidAdStateTransition";
  GfnErrorCode2[GfnErrorCode2["AuthTokenUpdateTimeout"] = 3237093387] = "AuthTokenUpdateTimeout";
  GfnErrorCode2[GfnErrorCode2["SessionServerErrorBegin"] = 3237093632] = "SessionServerErrorBegin";
  GfnErrorCode2[GfnErrorCode2["RequestForbidden"] = 3237093634] = "RequestForbidden";
  GfnErrorCode2[GfnErrorCode2["ServerInternalTimeout"] = 3237093635] = "ServerInternalTimeout";
  GfnErrorCode2[GfnErrorCode2["ServerInternalError"] = 3237093636] = "ServerInternalError";
  GfnErrorCode2[GfnErrorCode2["ServerInvalidRequest"] = 3237093637] = "ServerInvalidRequest";
  GfnErrorCode2[GfnErrorCode2["ServerInvalidRequestVersion"] = 3237093638] = "ServerInvalidRequestVersion";
  GfnErrorCode2[GfnErrorCode2["SessionListLimitExceeded"] = 3237093639] = "SessionListLimitExceeded";
  GfnErrorCode2[GfnErrorCode2["InvalidRequestDataMalformed"] = 3237093640] = "InvalidRequestDataMalformed";
  GfnErrorCode2[GfnErrorCode2["InvalidRequestDataMissing"] = 3237093641] = "InvalidRequestDataMissing";
  GfnErrorCode2[GfnErrorCode2["RequestLimitExceeded"] = 3237093642] = "RequestLimitExceeded";
  GfnErrorCode2[GfnErrorCode2["SessionLimitExceeded"] = 3237093643] = "SessionLimitExceeded";
  GfnErrorCode2[GfnErrorCode2["InvalidRequestVersionOutOfDate"] = 3237093644] = "InvalidRequestVersionOutOfDate";
  GfnErrorCode2[GfnErrorCode2["SessionEntitledTimeExceeded"] = 3237093645] = "SessionEntitledTimeExceeded";
  GfnErrorCode2[GfnErrorCode2["AuthFailure"] = 3237093646] = "AuthFailure";
  GfnErrorCode2[GfnErrorCode2["InvalidAuthenticationMalformed"] = 3237093647] = "InvalidAuthenticationMalformed";
  GfnErrorCode2[GfnErrorCode2["InvalidAuthenticationExpired"] = 3237093648] = "InvalidAuthenticationExpired";
  GfnErrorCode2[GfnErrorCode2["InvalidAuthenticationNotFound"] = 3237093649] = "InvalidAuthenticationNotFound";
  GfnErrorCode2[GfnErrorCode2["EntitlementFailure"] = 3237093650] = "EntitlementFailure";
  GfnErrorCode2[GfnErrorCode2["InvalidAppIdNotAvailable"] = 3237093651] = "InvalidAppIdNotAvailable";
  GfnErrorCode2[GfnErrorCode2["InvalidAppIdNotFound"] = 3237093652] = "InvalidAppIdNotFound";
  GfnErrorCode2[GfnErrorCode2["InvalidSessionIdMalformed"] = 3237093653] = "InvalidSessionIdMalformed";
  GfnErrorCode2[GfnErrorCode2["InvalidSessionIdNotFound"] = 3237093654] = "InvalidSessionIdNotFound";
  GfnErrorCode2[GfnErrorCode2["EulaUnAccepted"] = 3237093655] = "EulaUnAccepted";
  GfnErrorCode2[GfnErrorCode2["MaintenanceStatus"] = 3237093656] = "MaintenanceStatus";
  GfnErrorCode2[GfnErrorCode2["ServiceUnAvailable"] = 3237093657] = "ServiceUnAvailable";
  GfnErrorCode2[GfnErrorCode2["SteamGuardRequired"] = 3237093658] = "SteamGuardRequired";
  GfnErrorCode2[GfnErrorCode2["SteamLoginRequired"] = 3237093659] = "SteamLoginRequired";
  GfnErrorCode2[GfnErrorCode2["SteamGuardInvalid"] = 3237093660] = "SteamGuardInvalid";
  GfnErrorCode2[GfnErrorCode2["SteamProfilePrivate"] = 3237093661] = "SteamProfilePrivate";
  GfnErrorCode2[GfnErrorCode2["InvalidCountryCode"] = 3237093662] = "InvalidCountryCode";
  GfnErrorCode2[GfnErrorCode2["InvalidLanguageCode"] = 3237093663] = "InvalidLanguageCode";
  GfnErrorCode2[GfnErrorCode2["MissingCountryCode"] = 3237093664] = "MissingCountryCode";
  GfnErrorCode2[GfnErrorCode2["MissingLanguageCode"] = 3237093665] = "MissingLanguageCode";
  GfnErrorCode2[GfnErrorCode2["SessionNotPaused"] = 3237093666] = "SessionNotPaused";
  GfnErrorCode2[GfnErrorCode2["EmailNotVerified"] = 3237093667] = "EmailNotVerified";
  GfnErrorCode2[GfnErrorCode2["InvalidAuthenticationUnsupportedProtocol"] = 3237093668] = "InvalidAuthenticationUnsupportedProtocol";
  GfnErrorCode2[GfnErrorCode2["InvalidAuthenticationUnknownToken"] = 3237093669] = "InvalidAuthenticationUnknownToken";
  GfnErrorCode2[GfnErrorCode2["InvalidAuthenticationCredentials"] = 3237093670] = "InvalidAuthenticationCredentials";
  GfnErrorCode2[GfnErrorCode2["SessionNotPlaying"] = 3237093671] = "SessionNotPlaying";
  GfnErrorCode2[GfnErrorCode2["InvalidServiceResponse"] = 3237093672] = "InvalidServiceResponse";
  GfnErrorCode2[GfnErrorCode2["AppPatching"] = 3237093673] = "AppPatching";
  GfnErrorCode2[GfnErrorCode2["GameNotFound"] = 3237093674] = "GameNotFound";
  GfnErrorCode2[GfnErrorCode2["NotEnoughCredits"] = 3237093675] = "NotEnoughCredits";
  GfnErrorCode2[GfnErrorCode2["InvitationOnlyRegistration"] = 3237093676] = "InvitationOnlyRegistration";
  GfnErrorCode2[GfnErrorCode2["RegionNotSupportedForRegistration"] = 3237093677] = "RegionNotSupportedForRegistration";
  GfnErrorCode2[GfnErrorCode2["SessionTerminatedByAnotherClient"] = 3237093678] = "SessionTerminatedByAnotherClient";
  GfnErrorCode2[GfnErrorCode2["DeviceIdAlreadyUsed"] = 3237093679] = "DeviceIdAlreadyUsed";
  GfnErrorCode2[GfnErrorCode2["ServiceNotExist"] = 3237093680] = "ServiceNotExist";
  GfnErrorCode2[GfnErrorCode2["SessionExpired"] = 3237093681] = "SessionExpired";
  GfnErrorCode2[GfnErrorCode2["SessionLimitPerDeviceReached"] = 3237093682] = "SessionLimitPerDeviceReached";
  GfnErrorCode2[GfnErrorCode2["ForwardingZoneOutOfCapacity"] = 3237093683] = "ForwardingZoneOutOfCapacity";
  GfnErrorCode2[GfnErrorCode2["RegionNotSupportedIndefinitely"] = 3237093684] = "RegionNotSupportedIndefinitely";
  GfnErrorCode2[GfnErrorCode2["RegionBanned"] = 3237093685] = "RegionBanned";
  GfnErrorCode2[GfnErrorCode2["RegionOnHoldForFree"] = 3237093686] = "RegionOnHoldForFree";
  GfnErrorCode2[GfnErrorCode2["RegionOnHoldForPaid"] = 3237093687] = "RegionOnHoldForPaid";
  GfnErrorCode2[GfnErrorCode2["AppMaintenanceStatus"] = 3237093688] = "AppMaintenanceStatus";
  GfnErrorCode2[GfnErrorCode2["ResourcePoolNotConfigured"] = 3237093689] = "ResourcePoolNotConfigured";
  GfnErrorCode2[GfnErrorCode2["InsufficientVmCapacity"] = 3237093690] = "InsufficientVmCapacity";
  GfnErrorCode2[GfnErrorCode2["InsufficientRouteCapacity"] = 3237093691] = "InsufficientRouteCapacity";
  GfnErrorCode2[GfnErrorCode2["InsufficientScratchSpaceCapacity"] = 3237093692] = "InsufficientScratchSpaceCapacity";
  GfnErrorCode2[GfnErrorCode2["RequiredSeatInstanceTypeNotSupported"] = 3237093693] = "RequiredSeatInstanceTypeNotSupported";
  GfnErrorCode2[GfnErrorCode2["ServerSessionQueueLengthExceeded"] = 3237093694] = "ServerSessionQueueLengthExceeded";
  GfnErrorCode2[GfnErrorCode2["RegionNotSupportedForStreaming"] = 3237093695] = "RegionNotSupportedForStreaming";
  GfnErrorCode2[GfnErrorCode2["SessionForwardRequestAllocationTimeExpired"] = 3237093696] = "SessionForwardRequestAllocationTimeExpired";
  GfnErrorCode2[GfnErrorCode2["SessionForwardGameBinariesNotAvailable"] = 3237093697] = "SessionForwardGameBinariesNotAvailable";
  GfnErrorCode2[GfnErrorCode2["GameBinariesNotAvailableInRegion"] = 3237093698] = "GameBinariesNotAvailableInRegion";
  GfnErrorCode2[GfnErrorCode2["UekRetrievalFailed"] = 3237093699] = "UekRetrievalFailed";
  GfnErrorCode2[GfnErrorCode2["EntitlementFailureForResource"] = 3237093700] = "EntitlementFailureForResource";
  GfnErrorCode2[GfnErrorCode2["SessionInQueueAbandoned"] = 3237093701] = "SessionInQueueAbandoned";
  GfnErrorCode2[GfnErrorCode2["MemberTerminated"] = 3237093702] = "MemberTerminated";
  GfnErrorCode2[GfnErrorCode2["SessionRemovedFromQueueMaintenance"] = 3237093703] = "SessionRemovedFromQueueMaintenance";
  GfnErrorCode2[GfnErrorCode2["ZoneMaintenanceStatus"] = 3237093704] = "ZoneMaintenanceStatus";
  GfnErrorCode2[GfnErrorCode2["GuestModeCampaignDisabled"] = 3237093705] = "GuestModeCampaignDisabled";
  GfnErrorCode2[GfnErrorCode2["RegionNotSupportedAnonymousAccess"] = 3237093706] = "RegionNotSupportedAnonymousAccess";
  GfnErrorCode2[GfnErrorCode2["InstanceTypeNotSupportedInSingleRegion"] = 3237093707] = "InstanceTypeNotSupportedInSingleRegion";
  GfnErrorCode2[GfnErrorCode2["InvalidZoneForQueuedSession"] = 3237093710] = "InvalidZoneForQueuedSession";
  GfnErrorCode2[GfnErrorCode2["SessionWaitingAdsTimeExpired"] = 3237093711] = "SessionWaitingAdsTimeExpired";
  GfnErrorCode2[GfnErrorCode2["UserCancelledWatchingAds"] = 3237093712] = "UserCancelledWatchingAds";
  GfnErrorCode2[GfnErrorCode2["StreamingNotAllowedInLimitedMode"] = 3237093713] = "StreamingNotAllowedInLimitedMode";
  GfnErrorCode2[GfnErrorCode2["ForwardRequestJPMFailed"] = 3237093714] = "ForwardRequestJPMFailed";
  GfnErrorCode2[GfnErrorCode2["MaxSessionNumberLimitExceeded"] = 3237093715] = "MaxSessionNumberLimitExceeded";
  GfnErrorCode2[GfnErrorCode2["GuestModePartnerCapacityDisabled"] = 3237093716] = "GuestModePartnerCapacityDisabled";
  GfnErrorCode2[GfnErrorCode2["SessionRejectedNoCapacity"] = 3237093717] = "SessionRejectedNoCapacity";
  GfnErrorCode2[GfnErrorCode2["SessionInsufficientPlayabilityLevel"] = 3237093718] = "SessionInsufficientPlayabilityLevel";
  GfnErrorCode2[GfnErrorCode2["ForwardRequestLOFNFailed"] = 3237093719] = "ForwardRequestLOFNFailed";
  GfnErrorCode2[GfnErrorCode2["InvalidTransportRequest"] = 3237093720] = "InvalidTransportRequest";
  GfnErrorCode2[GfnErrorCode2["UserStorageNotAvailable"] = 3237093721] = "UserStorageNotAvailable";
  GfnErrorCode2[GfnErrorCode2["GfnStorageNotAvailable"] = 3237093722] = "GfnStorageNotAvailable";
  GfnErrorCode2[GfnErrorCode2["SessionServerErrorEnd"] = 3237093887] = "SessionServerErrorEnd";
  GfnErrorCode2[GfnErrorCode2["SessionSetupCancelled"] = 15867905] = "SessionSetupCancelled";
  GfnErrorCode2[GfnErrorCode2["SessionSetupCancelledDuringQueuing"] = 15867906] = "SessionSetupCancelledDuringQueuing";
  GfnErrorCode2[GfnErrorCode2["RequestCancelled"] = 15867907] = "RequestCancelled";
  GfnErrorCode2[GfnErrorCode2["SystemSleepDuringSessionSetup"] = 15867909] = "SystemSleepDuringSessionSetup";
  GfnErrorCode2[GfnErrorCode2["NoInternetDuringSessionSetup"] = 15868417] = "NoInternetDuringSessionSetup";
  GfnErrorCode2[GfnErrorCode2["SocketError"] = 3237101580] = "SocketError";
  GfnErrorCode2[GfnErrorCode2["AddressResolveFailed"] = 3237101581] = "AddressResolveFailed";
  GfnErrorCode2[GfnErrorCode2["ConnectFailed"] = 3237101582] = "ConnectFailed";
  GfnErrorCode2[GfnErrorCode2["SslError"] = 3237101583] = "SslError";
  GfnErrorCode2[GfnErrorCode2["ConnectionTimeout"] = 3237101584] = "ConnectionTimeout";
  GfnErrorCode2[GfnErrorCode2["DataReceiveTimeout"] = 3237101585] = "DataReceiveTimeout";
  GfnErrorCode2[GfnErrorCode2["PeerNoResponse"] = 3237101586] = "PeerNoResponse";
  GfnErrorCode2[GfnErrorCode2["UnexpectedHttpRedirect"] = 3237101587] = "UnexpectedHttpRedirect";
  GfnErrorCode2[GfnErrorCode2["DataSendFailure"] = 3237101588] = "DataSendFailure";
  GfnErrorCode2[GfnErrorCode2["DataReceiveFailure"] = 3237101589] = "DataReceiveFailure";
  GfnErrorCode2[GfnErrorCode2["CertificateRejected"] = 3237101590] = "CertificateRejected";
  GfnErrorCode2[GfnErrorCode2["DataNotAllowed"] = 3237101591] = "DataNotAllowed";
  GfnErrorCode2[GfnErrorCode2["NetworkErrorUnknown"] = 3237101592] = "NetworkErrorUnknown";
  return GfnErrorCode2;
})(GfnErrorCode || {});
const ERROR_MESSAGES = /* @__PURE__ */ new Map([
  // Success
  [15859712, { title: "Success", description: "Session started successfully." }],
  // Client errors
  [
    3237085186,
    {
      title: "Invalid Operation",
      description: "The requested operation is not valid at this time."
    }
  ],
  [
    3237089282,
    {
      title: "Network Error",
      description: "A network error occurred. Please check your internet connection."
    }
  ],
  [
    3237093377,
    {
      title: "Authentication Required",
      description: "Your session has expired. Please log in again."
    }
  ],
  [
    3237093379,
    {
      title: "Server Response Error",
      description: "Failed to parse server response. Please try again."
    }
  ],
  [
    3237093381,
    {
      title: "Invalid Server Response",
      description: "The server returned an invalid response."
    }
  ],
  [
    3237093384,
    {
      title: "Session Error",
      description: "An error occurred during session setup."
    }
  ],
  [
    3237093387,
    {
      title: "Authentication Timeout",
      description: "Authentication token update timed out. Please log in again."
    }
  ],
  // Server errors
  [
    3237093634,
    {
      title: "Access Forbidden",
      description: "Access to this service is forbidden."
    }
  ],
  [
    3237093635,
    {
      title: "Server Timeout",
      description: "The server timed out. Please try again."
    }
  ],
  [
    3237093636,
    {
      title: "Server Error",
      description: "An internal server error occurred. Please try again later."
    }
  ],
  [
    3237093637,
    {
      title: "Invalid Request",
      description: "The request was invalid."
    }
  ],
  [
    3237093639,
    {
      title: "Too Many Sessions",
      description: "You have too many active sessions. Please close some sessions and try again."
    }
  ],
  [
    3237093643,
    {
      title: "Session Limit Exceeded",
      description: "You have reached your session limit. Another session may already be running on your account."
    }
  ],
  [
    3237093645,
    {
      title: "Session Time Exceeded",
      description: "Your session time has been exceeded."
    }
  ],
  [
    3237093646,
    {
      title: "Authentication Failed",
      description: "Authentication failed. Please log in again."
    }
  ],
  [
    3237093648,
    {
      title: "Session Expired",
      description: "Your authentication has expired. Please log in again."
    }
  ],
  [
    3237093650,
    {
      title: "Entitlement Error",
      description: "You don't have access to this game or service."
    }
  ],
  [
    3237093651,
    {
      title: "Game Not Available",
      description: "This game is not currently available."
    }
  ],
  [
    3237093652,
    {
      title: "Game Not Found",
      description: "This game was not found in the library."
    }
  ],
  [
    3237093655,
    {
      title: "EULA Required",
      description: "You must accept the End User License Agreement to continue."
    }
  ],
  [
    3237093656,
    {
      title: "Under Maintenance",
      description: "The service is currently under maintenance. Please try again later."
    }
  ],
  [
    3237093657,
    {
      title: "Service Unavailable",
      description: "The service is temporarily unavailable. Please try again later."
    }
  ],
  [
    3237093658,
    {
      title: "Steam Guard Required",
      description: "Steam Guard authentication is required. Please complete Steam Guard verification."
    }
  ],
  [
    3237093659,
    {
      title: "Steam Login Required",
      description: "You need to link your Steam account to play this game."
    }
  ],
  [
    3237093660,
    {
      title: "Steam Guard Invalid",
      description: "Steam Guard code is invalid. Please try again."
    }
  ],
  [
    3237093661,
    {
      title: "Steam Profile Private",
      description: "Your Steam profile is private. Please make it public or friends-only."
    }
  ],
  [
    3237093667,
    {
      title: "Email Not Verified",
      description: "Please verify your email address to continue."
    }
  ],
  [
    3237093673,
    {
      title: "Game Updating",
      description: "This game is currently being updated. Please try again later."
    }
  ],
  [
    3237093674,
    {
      title: "Game Not Found",
      description: "This game was not found."
    }
  ],
  [
    3237093675,
    {
      title: "Insufficient Credits",
      description: "You don't have enough credits for this session."
    }
  ],
  [
    3237093678,
    {
      title: "Session Taken Over",
      description: "Your session was taken over by another device."
    }
  ],
  [
    3237093681,
    {
      title: "Session Expired",
      description: "Your session has expired."
    }
  ],
  [
    3237093682,
    {
      title: "Device Limit Reached",
      description: "You have reached the session limit for this device."
    }
  ],
  [
    3237093683,
    {
      title: "Region At Capacity",
      description: "Your region is currently at capacity. Please try again later."
    }
  ],
  [
    3237093684,
    {
      title: "Region Not Supported",
      description: "The service is not available in your region."
    }
  ],
  [
    3237093685,
    {
      title: "Region Banned",
      description: "The service is not available in your region."
    }
  ],
  [
    3237093686,
    {
      title: "Free Tier On Hold",
      description: "Free tier is temporarily unavailable in your region."
    }
  ],
  [
    3237093687,
    {
      title: "Paid Tier On Hold",
      description: "Paid tier is temporarily unavailable in your region."
    }
  ],
  [
    3237093688,
    {
      title: "Game Maintenance",
      description: "This game is currently under maintenance."
    }
  ],
  [
    3237093690,
    {
      title: "No Capacity",
      description: "No gaming rigs are available right now. Please try again later or join the queue."
    }
  ],
  [
    3237093694,
    {
      title: "Queue Full",
      description: "The queue is currently full. Please try again later."
    }
  ],
  [
    3237093695,
    {
      title: "GeForce NOW Unavailable in Your Region",
      description: "GeForce NOW has restricted streaming in your region. This is not an OpenNOW issue — NVIDIA has blocked access from your location. You may need to use a VPN or check GeForce NOW's supported countries list."
    }
  ],
  [
    3237093698,
    {
      title: "Game Not Available",
      description: "This game is not available in your region."
    }
  ],
  [
    3237093701,
    {
      title: "Queue Abandoned",
      description: "Your session in queue was abandoned."
    }
  ],
  [
    3237093702,
    {
      title: "Account Terminated",
      description: "Your account has been terminated."
    }
  ],
  [
    3237093703,
    {
      title: "Queue Maintenance",
      description: "The queue was cleared due to maintenance."
    }
  ],
  [
    3237093704,
    {
      title: "Zone Maintenance",
      description: "This server zone is under maintenance."
    }
  ],
  [
    3237093711,
    {
      title: "Ads Timeout",
      description: "Session expired while waiting for ads. Free tier users must watch ads to play. Please start a new session."
    }
  ],
  [
    3237093712,
    {
      title: "Ads Cancelled",
      description: "Session cancelled because ads were skipped. Free tier users must watch ads to play."
    }
  ],
  [
    3237093713,
    {
      title: "Limited Mode",
      description: "Streaming is not allowed in limited mode."
    }
  ],
  [
    3237093715,
    {
      title: "Session Limit",
      description: "Maximum number of sessions reached."
    }
  ],
  [
    3237093717,
    {
      title: "No Capacity",
      description: "No gaming rigs are available. Please try again later."
    }
  ],
  [
    3237093718,
    {
      title: "Membership Upgrade Required",
      description: "Your current GeForce NOW membership is not high enough to play this game. Upgrade to a higher tier and try again."
    }
  ],
  [
    3237093721,
    {
      title: "Storage Unavailable",
      description: "User storage is not available."
    }
  ],
  [
    3237093722,
    {
      title: "Storage Error",
      description: "Service storage is not available."
    }
  ],
  // Cancellation
  [
    15867905,
    {
      title: "Session Cancelled",
      description: "Session setup was cancelled."
    }
  ],
  [
    15867906,
    {
      title: "Queue Cancelled",
      description: "You left the queue."
    }
  ],
  [
    15867907,
    {
      title: "Request Cancelled",
      description: "The request was cancelled."
    }
  ],
  [
    15867909,
    {
      title: "System Sleep",
      description: "Session setup was interrupted by system sleep."
    }
  ],
  [
    15868417,
    {
      title: "No Internet",
      description: "No internet connection during session setup."
    }
  ],
  // Network errors
  [
    3237101580,
    {
      title: "Socket Error",
      description: "A socket error occurred. Please check your network."
    }
  ],
  [
    3237101581,
    {
      title: "DNS Error",
      description: "Failed to resolve server address. Please check your network."
    }
  ],
  [
    3237101582,
    {
      title: "Connection Failed",
      description: "Failed to connect to the server. Please check your network."
    }
  ],
  [
    3237101583,
    {
      title: "SSL Error",
      description: "A secure connection error occurred."
    }
  ],
  [
    3237101584,
    {
      title: "Connection Timeout",
      description: "Connection timed out. Please check your network."
    }
  ],
  [
    3237101585,
    {
      title: "Receive Timeout",
      description: "Data receive timed out. Please check your network."
    }
  ],
  [
    3237101586,
    {
      title: "No Response",
      description: "Server not responding. Please try again."
    }
  ],
  [
    3237101590,
    {
      title: "Certificate Error",
      description: "Server certificate was rejected."
    }
  ]
]);
class SessionError extends Error {
  /** HTTP status code */
  httpStatus;
  /** CloudMatch status code from requestStatus.statusCode */
  statusCode;
  /** Status description from requestStatus.statusDescription */
  statusDescription;
  /** Unified error code from requestStatus.unifiedErrorCode */
  unifiedErrorCode;
  /** Session error code from session.errorCode */
  sessionErrorCode;
  /** Computed service error code */
  gfnErrorCode;
  /** User-friendly title */
  title;
  constructor(info) {
    super(info.description);
    this.name = "SessionError";
    this.httpStatus = info.httpStatus;
    this.statusCode = info.statusCode;
    this.statusDescription = info.statusDescription;
    this.unifiedErrorCode = info.unifiedErrorCode;
    this.sessionErrorCode = info.sessionErrorCode;
    this.gfnErrorCode = info.gfnErrorCode;
    this.title = info.title;
  }
  /** Get error type as a string (e.g., "SessionLimitExceeded") */
  get errorType() {
    const entry = Object.entries(GfnErrorCode).find(([, value]) => value === this.gfnErrorCode);
    if (entry) {
      return entry[0];
    }
    if (this.statusCode > 0) {
      return `StatusCode${this.statusCode}`;
    }
    return "UnknownError";
  }
  /** Get user-friendly error message */
  get errorDescription() {
    return this.message;
  }
  /**
   * Parse error from CloudMatch response JSON
   */
  static fromResponse(httpStatus, responseBody) {
    let json = {};
    try {
      json = JSON.parse(responseBody);
    } catch {
    }
    const statusCode = json.requestStatus?.statusCode ?? 0;
    const statusDescription = json.requestStatus?.statusDescription;
    const unifiedErrorCode = json.requestStatus?.unifiedErrorCode;
    const sessionErrorCode = json.session?.errorCode;
    const gfnErrorCode = SessionError.computeErrorCode(statusCode, unifiedErrorCode);
    const { title, description } = SessionError.getErrorMessage(
      gfnErrorCode,
      statusDescription,
      httpStatus
    );
    return new SessionError({
      httpStatus,
      statusCode,
      statusDescription,
      unifiedErrorCode,
      sessionErrorCode,
      gfnErrorCode,
      title,
      description
    });
  }
  /**
   * Compute service error code from CloudMatch response
   */
  static computeErrorCode(statusCode, unifiedErrorCode) {
    let errorCode = 3237093632;
    if (statusCode === 1) {
      errorCode = 15859712;
    } else if (statusCode > 0 && statusCode < 255) {
      errorCode = 3237093632 + statusCode;
    }
    if (unifiedErrorCode !== void 0) {
      switch (errorCode) {
        case 3237093632:
        // SessionServerErrorBegin
        case 3237093636:
        // ServerInternalError
        case 3237093381:
          errorCode = unifiedErrorCode;
          break;
      }
    }
    return errorCode;
  }
  /**
   * Get user-friendly error message
   */
  static getErrorMessage(errorCode, statusDescription, httpStatus) {
    const knownError = ERROR_MESSAGES.get(errorCode);
    if (knownError) {
      return knownError;
    }
    if (statusDescription) {
      const descUpper = statusDescription.toUpperCase();
      if (descUpper.includes("INSUFFICIENT_PLAYABILITY")) {
        return {
          title: "Membership Upgrade Required",
          description: "Your current GeForce NOW membership is not high enough to play this game. Upgrade to a higher tier and try again."
        };
      }
      if (descUpper.includes("SESSION_LIMIT")) {
        return {
          title: "Session Limit Exceeded",
          description: "You have reached your maximum number of concurrent sessions."
        };
      }
      if (descUpper.includes("MAINTENANCE")) {
        return {
          title: "Under Maintenance",
          description: "The service is currently under maintenance. Please try again later."
        };
      }
      if (descUpper.includes("CAPACITY") || descUpper.includes("QUEUE")) {
        return {
          title: "No Capacity Available",
          description: "All gaming rigs are currently in use. Please try again later."
        };
      }
      if (descUpper.includes("AUTH") || descUpper.includes("TOKEN")) {
        return {
          title: "Authentication Error",
          description: "Please log in again."
        };
      }
      if (descUpper.includes("ENTITLEMENT")) {
        return {
          title: "Access Denied",
          description: "You don't have access to this game or service."
        };
      }
    }
    switch (httpStatus) {
      case 401:
        return {
          title: "Unauthorized",
          description: "Please log in again."
        };
      case 403:
        return {
          title: "Access Denied",
          description: "Access to this resource was denied."
        };
      case 404:
        return {
          title: "Not Found",
          description: "The requested resource was not found."
        };
      case 429:
        return {
          title: "Too Many Requests",
          description: "Please wait a moment and try again."
        };
    }
    if (httpStatus >= 500 && httpStatus < 600) {
      return {
        title: "Server Error",
        description: "A server error occurred. Please try again later."
      };
    }
    return {
      title: "Error",
      description: `An error occurred (HTTP ${httpStatus}).`
    };
  }
  /**
   * Check if this error indicates another session is running
   */
  isSessionConflict() {
    const sessionConflictCodes = [
      3237093643,
      // 3237093643
      3237093682,
      // 3237093682
      3237093715
      /* MaxSessionNumberLimitExceeded */
      // 3237093715
    ];
    if (sessionConflictCodes.includes(this.gfnErrorCode)) {
      return true;
    }
    return false;
  }
  /**
   * Check if this is a temporary error that might resolve with retry
   */
  isRetryable() {
    const retryableCodes = [
      3237089282,
      // 3237089282
      3237093635,
      // 3237093635
      3237093636,
      // 3237093636
      3237093683,
      // 3237093683
      3237093690,
      // 3237093690
      3237093717,
      // 3237093717
      3237101584,
      // 3237101584
      3237101585,
      // 3237101585
      3237101586
      /* PeerNoResponse */
      // 3237101586
    ];
    return retryableCodes.includes(this.gfnErrorCode);
  }
  /**
   * Check if user needs to log in again
   */
  needsReauth() {
    const reauthCodes = [
      3237093377,
      // 3237093377
      3237093387,
      // 3237093387
      3237093646,
      // 3237093646
      3237093647,
      // 3237093647
      3237093648,
      // 3237093648
      3237093649,
      // 3237093649
      3237093668,
      // 3237093668
      3237093669,
      // 3237093669
      3237093670
      /* InvalidAuthenticationCredentials */
      // 3237093670
    ];
    if (reauthCodes.includes(this.gfnErrorCode)) {
      return true;
    }
    if (this.httpStatus === 401) {
      return true;
    }
    return false;
  }
  /**
   * Convert to a plain object for serialization
   */
  toJSON() {
    return {
      httpStatus: this.httpStatus,
      statusCode: this.statusCode,
      statusDescription: this.statusDescription,
      unifiedErrorCode: this.unifiedErrorCode,
      sessionErrorCode: this.sessionErrorCode,
      gfnErrorCode: this.gfnErrorCode,
      title: this.title,
      description: this.message
    };
  }
}
function isSessionError(error) {
  return error instanceof SessionError;
}
const INVALID_PROXY_MESSAGE = "Invalid session proxy URL. Use http://host:port, https://host:port, socks4://host:port, or socks5://host:port.";
const SUPPORTED_PROXY_PROTOCOLS = /* @__PURE__ */ new Set(["http:", "https:", "socks4:", "socks5:"]);
const CLOUDMATCH_PROXY_PARTITION_PREFIX = "opennow:gfn-session-proxy";
const proxyPartitions = /* @__PURE__ */ new Map();
function sessionProxyPartitionForUrl(normalizedProxyUrl) {
  const existing = proxyPartitions.get(normalizedProxyUrl);
  if (existing) return existing;
  const partition = `${CLOUDMATCH_PROXY_PARTITION_PREFIX}:${crypto.randomUUID()}`;
  proxyPartitions.set(normalizedProxyUrl, partition);
  return partition;
}
function normalizeSessionProxyUrl(raw) {
  const trimmed = raw?.trim() ?? "";
  if (!trimmed) return null;
  const candidate = /^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed) ? trimmed : `http://${trimmed}`;
  let parsed;
  try {
    parsed = new URL(candidate);
  } catch {
    throw new Error(INVALID_PROXY_MESSAGE);
  }
  if (!SUPPORTED_PROXY_PROTOCOLS.has(parsed.protocol) || !parsed.hostname || !parsed.port) {
    throw new Error(INVALID_PROXY_MESSAGE);
  }
  const username = parsed.username ? encodeURIComponent(decodeURIComponent(parsed.username)) : "";
  const password = parsed.password ? encodeURIComponent(decodeURIComponent(parsed.password)) : "";
  const credentials = username ? `${username}${password ? `:${password}` : ""}@` : "";
  return `${parsed.protocol}//${credentials}${parsed.host}`;
}
async function fetchWithOptionalProxy(input, init, proxyUrl) {
  const normalizedProxyUrl = normalizeSessionProxyUrl(proxyUrl);
  if (!normalizedProxyUrl) {
    return fetch(input, init);
  }
  const { session: electronSession } = await import("electron");
  const proxySession = electronSession.fromPartition(sessionProxyPartitionForUrl(normalizedProxyUrl), { cache: false });
  await proxySession.setProxy({ proxyRules: normalizedProxyUrl });
  if (typeof proxySession.fetch === "function") {
    return proxySession.fetch(input, init);
  }
  throw new Error("Electron session fetch is unavailable for session proxy requests.");
}
async function readCloudMatchResponseText(response, options = {}) {
  const text = await response.text();
  options.onText?.(text);
  if (!response.ok) {
    options.onErrorText?.(text);
    throw SessionError.fromResponse(response.status, text);
  }
  return text;
}
async function readCloudMatchJson(response, options = {}) {
  const text = await readCloudMatchResponseText(response, options);
  return {
    text,
    payload: JSON.parse(text)
  };
}
async function throwIfCloudMatchResponseError(response) {
  if (response.ok) {
    return;
  }
  const text = await response.text();
  throw SessionError.fromResponse(response.status, text);
}
const SESSION_MODIFY_ACTION_AD_UPDATE = 6;
const READY_SESSION_STATUSES = /* @__PURE__ */ new Set([2, 3]);
const GFN_DEVICE_ID_FILENAME = "gfn-device-id.json";
let cachedStableDeviceId = null;
const require$1 = createRequire(import.meta.url);
function getElectronApp() {
  try {
    return require$1("electron").app ?? null;
  } catch {
    return null;
  }
}
const AD_ACTION_CODES = {
  start: 1,
  pause: 2,
  resume: 3,
  finish: 4,
  cancel: 5
};
const GFN_AD_MEDIA_PROFILE_ORDER = /* @__PURE__ */ new Map([
  ["mp4deinterlaced720p", 0],
  ["webm", 1],
  ["hlsadaptive", 2]
]);
function buildRequestedStreamingFeatures(settings, bitDepth, chromaFormat, hdrEnabled) {
  const cloudGsync = settings.enableCloudGsync;
  return {
    reflex: shouldRequestReflex(settings),
    bitDepth,
    cloudGsync,
    enabledL4S: settings.enableL4S,
    mouseMovementFlags: 0,
    trueHdr: hdrEnabled,
    supportedHidDevices: 0,
    profile: 0,
    fallbackToLogicalResolution: false,
    hidDevices: null,
    chromaFormat,
    prefilterMode: 0,
    prefilterSharpness: 0,
    prefilterNoiseReduction: 0,
    hudStreamingMode: 0,
    sdrColorSpace: 2,
    hdrColorSpace: hdrEnabled ? 4 : 0
  };
}
function shouldRequestReflex(settings) {
  if (typeof settings.cloudGsyncResolution?.reflexEnabled === "boolean") {
    return settings.cloudGsyncResolution.reflexEnabled;
  }
  const reflexMinimum = settings.cloudGsyncResolution?.capabilities.minimumFpsForReflexWithoutVrr ?? DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR;
  return settings.enableCloudGsync || settings.fps >= reflexMinimum;
}
function isReadySessionStatus(status) {
  return READY_SESSION_STATUSES.has(status);
}
function getStableDeviceId() {
  if (cachedStableDeviceId) {
    return cachedStableDeviceId;
  }
  try {
    const electronApp = getElectronApp();
    if (!electronApp) {
      throw new Error("Electron app is unavailable outside the main process.");
    }
    const path = join(electronApp.getPath("userData"), GFN_DEVICE_ID_FILENAME);
    if (existsSync(path)) {
      const parsed = JSON.parse(readFileSync(path, "utf-8"));
      if (typeof parsed.deviceId === "string" && parsed.deviceId.length > 0) {
        cachedStableDeviceId = parsed.deviceId;
        return parsed.deviceId;
      }
    }
    const deviceId = crypto.randomUUID();
    writeFileSync(path, JSON.stringify({ deviceId }, null, 2), "utf-8");
    cachedStableDeviceId = deviceId;
    return deviceId;
  } catch (error) {
    const fallback = crypto.randomUUID();
    cachedStableDeviceId = fallback;
    console.warn("[CloudMatch] Failed to load persisted device ID, using in-memory fallback:", error);
    return fallback;
  }
}
async function resolveHostnameWithFallback(hostname) {
  try {
    const r = await dns.promises.lookup(hostname);
    if (r && r.address) return r.address;
  } catch {
  }
  const fallbackServers = ["1.1.1.1", "8.8.8.8"];
  for (const server of fallbackServers) {
    try {
      const resolver = new dns.Resolver();
      resolver.setServers([server]);
      const addrs = await new Promise((resolve2, reject) => {
        resolver.resolve4(hostname, (err, addresses) => {
          if (err) reject(err);
          else resolve2(addresses);
        });
      });
      if (addrs && addrs.length > 0) return addrs[0];
    } catch {
    }
  }
  return null;
}
async function normalizeIceServers(response) {
  const raw = response.session.iceServerConfiguration?.iceServers ?? [];
  const servers = raw.map((entry) => {
    const urls = Array.isArray(entry.urls) ? entry.urls : [entry.urls];
    return {
      urls,
      username: entry.username,
      credential: entry.credential
    };
  }).filter((entry) => entry.urls.length > 0);
  if (servers.length > 0) {
    const resolvedServers = [];
    for (const s of servers) {
      const resolvedUrls = [];
      for (const u of s.urls) {
        try {
          const m = u.match(/^([a-zA-Z0-9+.-]+):([^/]+)/);
          if (m) {
            const scheme = m[1];
            const hostPort = m[2];
            const host = hostPort.split(":")[0];
            const portPart = hostPort.includes(":") ? ":" + hostPort.split(":").slice(1).join(":") : "";
            const bracketIfIpv6 = (h) => {
              if (h.startsWith("[") && h.endsWith("]")) return h;
              if (h.includes(":") && !/^\d{1,3}(?:\.\d{1,3}){3}$/.test(h)) {
                return `[${h}]`;
              }
              return h;
            };
            if (/^\d{1,3}(?:\.\d{1,3}){3}$/.test(host) || /^\[[0-9a-fA-F:]+\]$/.test(host)) {
              resolvedUrls.push(u);
            } else {
              const ip = await resolveHostnameWithFallback(host);
              const finalHost = ip ?? host;
              const maybeBracketted = bracketIfIpv6(finalHost);
              resolvedUrls.push(`${scheme}:${maybeBracketted}${portPart}`);
            }
          } else {
            resolvedUrls.push(u);
          }
        } catch {
          resolvedUrls.push(u);
        }
      }
      resolvedServers.push({ urls: resolvedUrls, username: s.username, credential: s.credential });
    }
    return resolvedServers;
  }
  const defaults = ["s1.stun.gamestream.nvidia.com:19308", "stun.l.google.com:19302", "stun1.l.google.com:19302"];
  const out = [];
  for (const d of defaults) {
    const parts = d.split(":");
    const host = parts[0];
    const port = parts.length > 1 ? `:${parts.slice(1).join(":")}` : "";
    const ip = await resolveHostnameWithFallback(host);
    const bracketIfIpv6 = (h) => h.includes(":") && !h.startsWith("[") ? `[${h}]` : h;
    if (ip) out.push({ urls: [`stun:${bracketIfIpv6(ip)}${port}`] });
    else out.push({ urls: [`stun:${bracketIfIpv6(host)}${port}`] });
  }
  return out;
}
function streamingServerIp(response) {
  const connections = response.session.connectionInfo ?? [];
  const sigConn = connections.find((conn) => conn.usage === 14);
  if (sigConn) {
    const rawIp = sigConn.ip;
    const directIp = Array.isArray(rawIp) ? rawIp[0] : rawIp;
    if (directIp && directIp.length > 0) {
      return directIp;
    }
    if (sigConn.resourcePath) {
      const host = extractHostFromUrl(sigConn.resourcePath);
      if (host) return host;
    }
  }
  const controlIp = response.session.sessionControlInfo?.ip;
  if (controlIp && controlIp.length > 0) {
    return Array.isArray(controlIp) ? controlIp[0] : controlIp;
  }
  return null;
}
function extractHostFromUrl(url) {
  const prefixes = ["rtsps://", "rtsp://", "wss://", "https://"];
  let afterProto = null;
  for (const prefix of prefixes) {
    if (url.startsWith(prefix)) {
      afterProto = url.slice(prefix.length);
      break;
    }
  }
  if (!afterProto) return null;
  const host = afterProto.split(":")[0]?.split("/")[0];
  if (!host || host.length === 0 || host.startsWith(".")) return null;
  return host;
}
function isZoneHostname(ip) {
  return ip.includes("cloudmatchbeta.nvidiagrid.net") || ip.includes("cloudmatch.nvidiagrid.net");
}
function resolveSignaling(response) {
  const connections = response.session.connectionInfo ?? [];
  const signalingConnection = connections.find((conn) => conn.usage === 14 && conn.ip) ?? connections.find((conn) => conn.ip);
  const serverIp = streamingServerIp(response);
  if (!serverIp) {
    throw new Error("CloudMatch response did not include a signaling host");
  }
  const resourcePath = signalingConnection?.resourcePath ?? "/nvst/";
  const { signalingUrl, signalingHost } = buildSignalingUrl(resourcePath, serverIp);
  const effectiveHost = signalingHost ?? serverIp;
  const signalingServer = effectiveHost.includes(":") ? effectiveHost : `${effectiveHost}:443`;
  return {
    serverIp,
    signalingServer,
    signalingUrl,
    mediaConnectionInfo: resolveMediaConnectionInfo(connections, serverIp, {
      logMissing: isReadySessionStatus(response.session.status)
    })
  };
}
function resolveMediaConnectionInfo(connections, serverIp, options) {
  const extractIp = (conn) => {
    const rawIp = conn.ip;
    const directIp = Array.isArray(rawIp) ? rawIp[0] : rawIp;
    if (directIp && directIp.length > 0) return directIp;
    if (conn.resourcePath) {
      const host = extractHostFromUrl(conn.resourcePath);
      if (host) return host;
    }
    return null;
  };
  const extractPort = (conn) => {
    if (conn.port > 0) return conn.port;
    if (conn.resourcePath) {
      try {
        const url = new URL(conn.resourcePath.replace("rtsps://", "https://").replace("rtsp://", "http://"));
        const portStr = url.port;
        if (portStr) return parseInt(portStr, 10);
      } catch {
      }
    }
    return 0;
  };
  const primary = connections.find((c) => c.usage === 2);
  if (primary) {
    const ip = extractIp(primary);
    const port = extractPort(primary);
    console.log(`[CloudMatch] resolveMediaConnectionInfo: usage=2 candidate: ip=${ip}, port=${port}`);
    if (ip && port > 0) return { ip, port };
  }
  const alt = connections.find((c) => c.usage === 17);
  if (alt) {
    const ip = extractIp(alt);
    const port = extractPort(alt);
    console.log(`[CloudMatch] resolveMediaConnectionInfo: usage=17 candidate: ip=${ip}, port=${port}`);
    if (ip && port > 0) return { ip, port };
  }
  const alliance = connections.filter((c) => c.usage === 14).sort((a, b) => b.port - a.port);
  for (const conn of alliance) {
    const ip = extractIp(conn) ?? serverIp;
    const port = extractPort(conn);
    console.log(`[CloudMatch] resolveMediaConnectionInfo: usage=14 candidate: ip=${ip}, port=${port} (serverIp fallback=${serverIp})`);
    if (ip && port > 0) return { ip, port };
  }
  if (options?.logMissing ?? true) {
    console.log("[CloudMatch] resolveMediaConnectionInfo: NO valid media connection info found");
  }
  return void 0;
}
function buildSignalingUrl(raw, serverIp) {
  if (raw.startsWith("rtsps://") || raw.startsWith("rtsp://")) {
    const withoutScheme = raw.startsWith("rtsps://") ? raw.slice("rtsps://".length) : raw.slice("rtsp://".length);
    const host = withoutScheme.split(":")[0]?.split("/")[0];
    if (host && host.length > 0 && !host.startsWith(".")) {
      return {
        signalingUrl: `wss://${host}/nvst/`,
        signalingHost: host
      };
    }
    return {
      signalingUrl: `wss://${serverIp}:443/nvst/`,
      signalingHost: null
    };
  }
  if (raw.startsWith("wss://")) {
    const withoutScheme = raw.slice("wss://".length);
    const host = withoutScheme.split("/")[0] ?? null;
    return { signalingUrl: raw, signalingHost: host };
  }
  if (raw.startsWith("/")) {
    return {
      signalingUrl: `wss://${serverIp}:443${raw}`,
      signalingHost: null
    };
  }
  return {
    signalingUrl: `wss://${serverIp}:443/nvst/`,
    signalingHost: null
  };
}
function parseResolution(input) {
  const [rawWidth, rawHeight] = input.split("x");
  const width = Number.parseInt(rawWidth ?? "", 10);
  const height = Number.parseInt(rawHeight ?? "", 10);
  if (!Number.isFinite(width) || !Number.isFinite(height) || width <= 0 || height <= 0) {
    return { width: 1920, height: 1080 };
  }
  return { width, height };
}
function timezoneOffsetMs() {
  return -(/* @__PURE__ */ new Date()).getTimezoneOffset() * 60 * 1e3;
}
function webRtcSessionMetadata(width, height) {
  return [
    { key: "SubSessionId", value: crypto.randomUUID() },
    { key: "wssignaling", value: "1" },
    { key: "GSStreamerType", value: "WebRTC" },
    { key: "networkType", value: "Unknown" },
    { key: "ClientImeSupport", value: "0" },
    {
      key: "clientPhysicalResolution",
      value: JSON.stringify({ horizontalPixels: width, verticalPixels: height })
    },
    { key: "surroundAudioInfo", value: "2" }
  ];
}
function buildSessionRequestBody(input, deviceHashId) {
  const { width, height } = parseResolution(input.settings.resolution);
  const cq = input.settings.colorQuality;
  const hdrEnabled = false;
  const bitDepth = colorQualityBitDepth(cq);
  const chromaFormat = colorQualityChromaFormat(cq);
  const accountLinked = input.accountLinked ?? true;
  return {
    sessionRequestData: {
      appId: input.appId,
      internalTitle: input.internalTitle || null,
      availableSupportedControllers: [],
      networkTestSessionId: null,
      parentSessionId: null,
      clientIdentification: "GFN-PC",
      // Keep device identity stable across create -> reconnect/resume flows.
      // The official client preserves this identity, and resume reliability depends on it.
      deviceHashId,
      clientVersion: "30.0",
      sdkVersion: "1.0",
      streamerVersion: 1,
      clientPlatformName: "windows",
      clientRequestMonitorSettings: [
        {
          monitorId: 0,
          positionX: 0,
          positionY: 0,
          widthInPixels: width,
          heightInPixels: height,
          framesPerSecond: input.settings.fps,
          sdrHdrMode: hdrEnabled ? 1 : 0,
          displayData: hdrEnabled ? {
            desiredContentMaxLuminance: 1e3,
            desiredContentMinLuminance: 0,
            desiredContentMaxFrameAverageLuminance: 500
          } : null,
          hdr10PlusGamingData: null,
          dpi: 100
        }
      ],
      useOps: true,
      audioMode: 2,
      metaData: webRtcSessionMetadata(width, height),
      sdrHdrMode: hdrEnabled ? 1 : 0,
      clientDisplayHdrCapabilities: hdrEnabled ? {
        version: 1,
        hdrEdrSupportedFlagsInUint32: 1,
        staticMetadataDescriptorId: 0
      } : null,
      surroundAudioInfo: 0,
      remoteControllersBitmap: 0,
      clientTimezoneOffset: timezoneOffsetMs(),
      enhancedStreamMode: 1,
      appLaunchMode: 1,
      secureRTSPSupported: false,
      partnerCustomData: "",
      accountLinked,
      enablePersistingInGameSettings: true,
      userAge: 26,
      requestedStreamingFeatures: buildRequestedStreamingFeatures(
        input.settings,
        bitDepth,
        chromaFormat,
        hdrEnabled
      )
    }
  };
}
function cloudmatchUrl(zone) {
  return `https://${zone}.cloudmatchbeta.nvidiagrid.net`;
}
function resolveStreamingBaseUrl(zone, provided) {
  if (provided && provided.trim()) {
    const trimmed = provided.trim();
    return trimmed.endsWith("/") ? trimmed.slice(0, -1) : trimmed;
  }
  return cloudmatchUrl(zone);
}
function shouldUseServerIp(baseUrl) {
  return baseUrl.includes("cloudmatchbeta.nvidiagrid.net");
}
function resolvePollStopBase(zone, provided, serverIp) {
  const base = resolveStreamingBaseUrl(zone, provided);
  if (serverIp && shouldUseServerIp(base) && !isZoneHostname(serverIp)) {
    return `https://${serverIp}`;
  }
  return base;
}
function toPositiveInt(value) {
  if (typeof value === "number" && Number.isFinite(value)) {
    const normalized = Math.trunc(value);
    return normalized > 0 ? normalized : void 0;
  }
  if (typeof value === "string" && value.trim().length > 0) {
    const parsed = Number.parseInt(value, 10);
    return Number.isFinite(parsed) && parsed > 0 ? parsed : void 0;
  }
  return void 0;
}
function toBoolean(value) {
  if (typeof value === "boolean") {
    return value;
  }
  if (typeof value === "number" && Number.isFinite(value)) {
    return value !== 0;
  }
  if (typeof value === "string") {
    const normalized = value.trim().toLowerCase();
    if (normalized === "true" || normalized === "1") {
      return true;
    }
    if (normalized === "false" || normalized === "0") {
      return false;
    }
  }
  return void 0;
}
function toOptionalString(value) {
  if (typeof value !== "string") {
    return void 0;
  }
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : void 0;
}
function extractQueuePosition(payload) {
  const direct = toPositiveInt(payload.session.queuePosition);
  if (direct !== void 0) {
    return direct;
  }
  const seatSetup = payload.session.seatSetupInfo;
  if (seatSetup) {
    const nested = toPositiveInt(seatSetup.queuePosition);
    if (nested !== void 0) {
      return nested;
    }
  }
  const nestedSessionProgress = payload.session.sessionProgress;
  if (nestedSessionProgress) {
    const nested = toPositiveInt(nestedSessionProgress.queuePosition);
    if (nested !== void 0) {
      return nested;
    }
  }
  const nestedProgressInfo = payload.session.progressInfo;
  if (nestedProgressInfo) {
    const nested = toPositiveInt(nestedProgressInfo.queuePosition);
    if (nested !== void 0) {
      return nested;
    }
  }
  return void 0;
}
function extractSeatSetupStep(payload) {
  const raw = payload.session.seatSetupInfo?.seatSetupStep;
  if (typeof raw === "number" && Number.isFinite(raw)) {
    return Math.trunc(raw);
  }
  return void 0;
}
function normalizeSessionAdInfo(ad, index) {
  const adId = toOptionalString(ad.adId);
  const adMediaFiles = (ad.adMediaFiles ?? []).map((file) => ({
    mediaFileUrl: toOptionalString(file.mediaFileUrl),
    encodingProfile: toOptionalString(file.encodingProfile)
  })).filter((file) => file.mediaFileUrl || file.encodingProfile).sort((left, right) => {
    const leftRank = left.encodingProfile ? GFN_AD_MEDIA_PROFILE_ORDER.get(left.encodingProfile) ?? Number.MAX_SAFE_INTEGER : Number.MAX_SAFE_INTEGER;
    const rightRank = right.encodingProfile ? GFN_AD_MEDIA_PROFILE_ORDER.get(right.encodingProfile) ?? Number.MAX_SAFE_INTEGER : Number.MAX_SAFE_INTEGER;
    return leftRank - rightRank;
  });
  const preferredMediaFile = adMediaFiles.find((file) => file.mediaFileUrl);
  const mediaUrl = preferredMediaFile?.mediaFileUrl ?? toOptionalString(ad.adUrl) ?? toOptionalString(ad.mediaUrl) ?? toOptionalString(ad.videoUrl) ?? toOptionalString(ad.url);
  const adUrl = toOptionalString(ad.adUrl);
  const clickThroughUrl = toOptionalString(ad.clickThroughUrl);
  const title = toOptionalString(ad.title);
  const description = toOptionalString(ad.description);
  const adLengthInSeconds = typeof ad.adLengthInSeconds === "number" && Number.isFinite(ad.adLengthInSeconds) && ad.adLengthInSeconds > 0 ? ad.adLengthInSeconds : void 0;
  const durationMs = (adLengthInSeconds !== void 0 ? Math.round(adLengthInSeconds * 1e3) : void 0) ?? toPositiveInt(ad.durationMs) ?? toPositiveInt(ad.durationInMs);
  const adState = typeof ad.adState === "number" && Number.isFinite(ad.adState) ? Math.trunc(ad.adState) : void 0;
  if (!adId && !mediaUrl && !adUrl && adMediaFiles.length === 0 && !title && !description) {
    return null;
  }
  return {
    adId: adId ?? `ad-${index + 1}`,
    state: adState,
    adState,
    adUrl,
    mediaUrl,
    adMediaFiles,
    clickThroughUrl,
    adLengthInSeconds,
    durationMs,
    title,
    description
  };
}
function extractAdState(payload) {
  const sessionAdsRequired = toBoolean(payload.session.sessionAdsRequired) ?? toBoolean(payload.session.isAdsRequired) ?? toBoolean(payload.session.sessionProgress?.isAdsRequired) ?? toBoolean(payload.session.progressInfo?.isAdsRequired);
  if (sessionAdsRequired) {
    console.log(
      `[CloudMatch] extractAdState: sessionAdsRequired=${payload.session.sessionAdsRequired}, isAdsRequired=${payload.session.isAdsRequired}, sessionAds=${JSON.stringify(payload.session.sessionAds ?? null)}, opportunity=${JSON.stringify(payload.session.opportunity ?? null)}`
    );
  }
  const ads = (payload.session.sessionAds ?? []).map((ad, index) => normalizeSessionAdInfo(ad, index)).filter((ad) => ad !== null);
  const opportunity = payload.session.opportunity;
  const normalizedOpportunity = opportunity ? {
    state: toOptionalString(opportunity.state),
    queuePaused: toBoolean(opportunity.queuePaused),
    gracePeriodSeconds: toPositiveInt(opportunity.gracePeriodSeconds),
    message: toOptionalString(opportunity.message),
    title: toOptionalString(opportunity.title),
    description: toOptionalString(opportunity.description)
  } : void 0;
  const queuePaused = normalizedOpportunity?.queuePaused ?? (typeof normalizedOpportunity?.state === "string" ? normalizedOpportunity.state.toLowerCase() === "graceperiodstart" : void 0);
  const gracePeriodSeconds = normalizedOpportunity?.gracePeriodSeconds;
  const effectiveIsAdsRequired = sessionAdsRequired ?? ads.length > 0;
  const message = normalizedOpportunity?.message ?? normalizedOpportunity?.description ?? (queuePaused ? "Resume ads to stay in queue." : effectiveIsAdsRequired ? "Finish ads to stay in queue." : void 0);
  if (!effectiveIsAdsRequired && ads.length === 0 && !queuePaused && !message) {
    return void 0;
  }
  return {
    isAdsRequired: effectiveIsAdsRequired,
    sessionAdsRequired,
    isQueuePaused: queuePaused,
    gracePeriodSeconds,
    message,
    sessionAds: ads,
    ads,
    opportunity: normalizedOpportunity,
    // Mark whether the server sent sessionAds=null (transient gap) so the
    // renderer's mergeAdState can safely restore the previous ad list for the
    // ad player, while NOT restoring it after an explicit client-side clear
    // that follows a rejected finish action.
    serverSentEmptyAds: payload.session.sessionAds == null
  };
}
function toColorQuality(bitDepth, chromaFormat) {
  if (bitDepth !== 0 && bitDepth !== 10) {
    return void 0;
  }
  if (chromaFormat !== 0 && chromaFormat !== 2) {
    return void 0;
  }
  if (bitDepth === 10) {
    return chromaFormat === 2 ? "10bit_444" : "10bit_420";
  }
  return chromaFormat === 2 ? "8bit_444" : "8bit_420";
}
function normalizeStreamingFeatures(features) {
  if (!features) {
    return void 0;
  }
  const normalized = {};
  if (typeof features.reflex === "boolean") {
    normalized.reflex = features.reflex;
  }
  if (typeof features.bitDepth === "number" && Number.isFinite(features.bitDepth)) {
    normalized.bitDepth = Math.trunc(features.bitDepth);
  }
  if (typeof features.cloudGsync === "boolean") {
    normalized.cloudGsync = features.cloudGsync;
  }
  if (typeof features.chromaFormat === "number" && Number.isFinite(features.chromaFormat)) {
    normalized.chromaFormat = Math.trunc(features.chromaFormat);
  }
  if (typeof features.enabledL4S === "boolean") {
    normalized.enabledL4S = features.enabledL4S;
  }
  if ("trueHdr" in features && typeof features.trueHdr === "boolean") {
    normalized.trueHdr = features.trueHdr;
  }
  return Object.keys(normalized).length > 0 ? normalized : void 0;
}
function extractNegotiatedStreamProfile(payload) {
  const monitor = payload.session.sessionRequestData?.clientRequestMonitorSettings?.[0];
  const finalizedFeatures = payload.session.finalizedStreamingFeatures;
  const requestedFeatures = payload.session.sessionRequestData?.requestedStreamingFeatures;
  const width = monitor?.widthInPixels;
  const height = monitor?.heightInPixels;
  const fps = monitor?.framesPerSecond;
  const colorQuality = toColorQuality(
    finalizedFeatures?.bitDepth ?? requestedFeatures?.bitDepth,
    finalizedFeatures?.chromaFormat ?? requestedFeatures?.chromaFormat
  );
  const enabledL4S = finalizedFeatures?.enabledL4S ?? requestedFeatures?.enabledL4S;
  const enabledCloudGsync = finalizedFeatures?.cloudGsync ?? requestedFeatures?.cloudGsync;
  const enabledReflex = finalizedFeatures?.reflex ?? requestedFeatures?.reflex;
  const profile = {};
  if (typeof width === "number" && Number.isFinite(width) && width > 0 && typeof height === "number" && Number.isFinite(height) && height > 0) {
    profile.resolution = `${Math.trunc(width)}x${Math.trunc(height)}`;
  }
  if (typeof fps === "number" && Number.isFinite(fps) && fps > 0) {
    profile.fps = Math.trunc(fps);
  }
  if (colorQuality) {
    profile.colorQuality = colorQuality;
  }
  if (typeof enabledL4S === "boolean") {
    profile.enableL4S = enabledL4S;
  }
  if (typeof enabledCloudGsync === "boolean") {
    profile.enableCloudGsync = enabledCloudGsync;
  }
  if (typeof enabledReflex === "boolean") {
    profile.enableReflex = enabledReflex;
  }
  return Object.keys(profile).length > 0 ? profile : void 0;
}
async function toSessionInfo(options) {
  const { zone, streamingBaseUrl, payload, clientId, deviceId } = options;
  if (payload.requestStatus.statusCode !== 1) {
    const errorJson = JSON.stringify(payload);
    throw SessionError.fromResponse(200, errorJson);
  }
  const signaling = resolveSignaling(payload);
  const queuePosition = extractQueuePosition(payload);
  const seatSetupStep = extractSeatSetupStep(payload);
  const adState = extractAdState(payload);
  const negotiatedStreamProfile = extractNegotiatedStreamProfile(payload);
  const requestedStreamingFeatures = normalizeStreamingFeatures(
    payload.session.sessionRequestData?.requestedStreamingFeatures
  );
  const finalizedStreamingFeatures = normalizeStreamingFeatures(
    payload.session.finalizedStreamingFeatures
  );
  const connections = payload.session.connectionInfo ?? [];
  const connectionSummary = connections.map((conn) => {
    const rawIp = Array.isArray(conn.ip) ? conn.ip[0] : conn.ip;
    return `{usage=${conn.usage},ip=${rawIp ?? "null"},port=${conn.port},resourcePath=${conn.resourcePath ?? "null"}}`;
  }).join(", ");
  console.log(
    `[CloudMatch] toSessionInfo: status=${payload.session.status}, seatSetupStep=${seatSetupStep ?? "n/a"}, queuePosition=${queuePosition ?? "n/a"}, connectionInfo=${connections.length} entries, serverIp=${signaling.serverIp}, signalingServer=${signaling.signalingServer}, signalingUrl=${signaling.signalingUrl}, connections=[${connectionSummary}]`
  );
  console.log(
    `[CloudMatch] negotiated streaming features: requested=${JSON.stringify(requestedStreamingFeatures ?? {})} finalized=${JSON.stringify(finalizedStreamingFeatures ?? {})} cloudGsync=${negotiatedStreamProfile?.enableCloudGsync ?? "n/a"}, reflex=${negotiatedStreamProfile?.enableReflex ?? "n/a"}, l4s=${negotiatedStreamProfile?.enableL4S ?? "n/a"}`
  );
  return {
    sessionId: payload.session.sessionId,
    status: payload.session.status,
    seatSetupStep,
    queuePosition,
    adState,
    zone,
    streamingBaseUrl,
    serverIp: signaling.serverIp,
    signalingServer: signaling.signalingServer,
    signalingUrl: signaling.signalingUrl,
    gpuType: payload.session.gpuType,
    iceServers: await normalizeIceServers(payload),
    mediaConnectionInfo: signaling.mediaConnectionInfo,
    negotiatedStreamProfile,
    requestedStreamingFeatures,
    finalizedStreamingFeatures,
    clientId,
    deviceId
  };
}
async function createSession(input) {
  if (!input.token) {
    throw new Error("Missing token for session creation");
  }
  if (!/^\d+$/.test(input.appId)) {
    throw new Error(`Invalid launch appId '${input.appId}' (must be numeric)`);
  }
  const clientId = crypto.randomUUID();
  const deviceId = getStableDeviceId();
  const body = buildSessionRequestBody(input, deviceId);
  const base = resolveStreamingBaseUrl(input.zone, input.streamingBaseUrl);
  const keyboardLayout = resolveGfnKeyboardLayout(input.settings.keyboardLayout ?? DEFAULT_KEYBOARD_LAYOUT, process.platform);
  const languageCode = input.settings.gameLanguage ?? "en_US";
  const url = `${base}/v2/session?${new URLSearchParams({ keyboardLayout, languageCode }).toString()}`;
  const response = await fetchWithOptionalProxy(url, {
    method: "POST",
    headers: buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: true }),
    body: JSON.stringify(body)
  }, input.proxyUrl);
  const { payload } = await readCloudMatchJson(response);
  return await toSessionInfo({ zone: input.zone, streamingBaseUrl: base, payload, clientId, deviceId });
}
async function pollSession(input) {
  if (!input.token) {
    throw new Error("Missing token for session polling");
  }
  const clientId = input.clientId ?? crypto.randomUUID();
  const deviceId = input.deviceId ?? crypto.randomUUID();
  const base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp);
  const baseHost = new URL(base).hostname;
  const pollProxyUrl = isZoneHostname(baseHost) ? input.proxyUrl : void 0;
  const url = `${base}/v2/session/${input.sessionId}`;
  const headers = buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
  const response = await fetchWithOptionalProxy(url, {
    method: "GET",
    headers
  }, pollProxyUrl);
  const { payload } = await readCloudMatchJson(response);
  const realServerIp = streamingServerIp(payload);
  const polledViaZone = isZoneHostname(baseHost);
  const realIpDiffers = realServerIp && realServerIp.length > 0 && !isZoneHostname(realServerIp) && realServerIp !== input.serverIp;
  if (polledViaZone && realIpDiffers && isReadySessionStatus(payload.session.status)) {
    console.log(
      `[CloudMatch] Session ready: re-polling via real server IP ${realServerIp} (was: ${baseHost})`
    );
    const directBase = `https://${realServerIp}`;
    const directUrl = `${directBase}/v2/session/${input.sessionId}`;
    try {
      const directResponse = await fetch(directUrl, {
        method: "GET",
        headers
      });
      if (directResponse.ok) {
        const directText = await directResponse.text();
        const directPayload = JSON.parse(directText);
        if (directPayload.requestStatus.statusCode === 1) {
          console.log("[CloudMatch] Direct re-poll succeeded, using direct response for signaling info");
          return await toSessionInfo({ zone: input.zone, streamingBaseUrl: directBase, payload: directPayload, clientId, deviceId });
        }
      }
    } catch (e) {
      console.warn("[CloudMatch] Direct re-poll failed, using zone LB response:", e);
    }
  }
  return await toSessionInfo({ zone: input.zone, streamingBaseUrl: base, payload, clientId, deviceId });
}
async function reportSessionAd(input) {
  if (!input.token) {
    throw new Error("Missing token for ad update");
  }
  const clientId = input.clientId ?? crypto.randomUUID();
  const deviceId = input.deviceId ?? crypto.randomUUID();
  const base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp);
  const url = `${base}/v2/session/${input.sessionId}`;
  const clientTimestamp = input.clientTimestamp ?? Math.floor(Date.now() / 1e3);
  const adUpdate = {
    adId: input.adId,
    adAction: AD_ACTION_CODES[input.action],
    clientTimestamp,
    ...typeof input.watchedTimeInMs === "number" ? { watchedTimeInMs: Math.max(0, Math.round(input.watchedTimeInMs)) } : {},
    ...typeof input.pausedTimeInMs === "number" ? { pausedTimeInMs: Math.max(0, Math.round(input.pausedTimeInMs)) } : {},
    ...input.cancelReason ? { cancelReason: input.cancelReason } : {}
  };
  const requestBody = {
    action: SESSION_MODIFY_ACTION_AD_UPDATE,
    adUpdates: [adUpdate]
  };
  console.log(
    `[CloudMatch] reportSessionAd: sending action=${input.action}(${requestBody.adUpdates[0].adAction}), adId=${input.adId}, sessionId=${input.sessionId}, zone=${input.zone}, url=${url}, cancelReason=${input.cancelReason ?? "n/a"}, errorInfo=${input.errorInfo ?? "n/a"}`
  );
  const response = await fetch(url, {
    method: "PUT",
    // Official browser requests include Origin/Referer on cross-origin ad updates.
    headers: buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: true }),
    body: JSON.stringify(requestBody)
  });
  const { text, payload } = await readCloudMatchJson(response, {
    onErrorText: (text2) => {
      console.warn(
        `[CloudMatch] reportSessionAd: backend error status=${response.status}, sessionId=${input.sessionId}, adId=${input.adId}, action=${input.action}, body=${text2.slice(0, 500)}`
      );
    }
  });
  if (payload.requestStatus.statusCode !== 1) {
    console.warn(
      `[CloudMatch] reportSessionAd: API error requestStatus=${payload.requestStatus.statusCode}, description=${payload.requestStatus.statusDescription ?? "unknown"}, sessionId=${input.sessionId}, adId=${input.adId}, action=${input.action}`
    );
    throw SessionError.fromResponse(200, text);
  }
  console.log(
    `[CloudMatch] reportSessionAd: success sessionId=${input.sessionId}, adId=${input.adId}, action=${input.action}, status=${payload.session.status}, queuePosition=${extractQueuePosition(payload) ?? "n/a"}, adsRequired=${extractAdState(payload)?.isAdsRequired ?? false}`
  );
  return await toSessionInfo({ zone: input.zone, streamingBaseUrl: base, payload, clientId, deviceId });
}
async function stopSession(input) {
  if (!input.token) {
    throw new Error("Missing token for session stop");
  }
  const clientId = input.clientId ?? crypto.randomUUID();
  const deviceId = input.deviceId ?? crypto.randomUUID();
  const base = resolvePollStopBase(input.zone, input.streamingBaseUrl, input.serverIp);
  const url = `${base}/v2/session/${input.sessionId}`;
  const response = await fetch(url, {
    method: "DELETE",
    headers: buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: false })
  });
  await throwIfCloudMatchResponseError(response);
}
async function getActiveSessions(token, streamingBaseUrl) {
  if (!token) {
    throw new Error("Missing token for getting active sessions");
  }
  const base = streamingBaseUrl.trim().endsWith("/") ? streamingBaseUrl.trim().slice(0, -1) : streamingBaseUrl.trim();
  const url = `${base}/v2/session`;
  const response = await fetch(url, {
    method: "GET",
    headers: buildGfnCloudMatchHeaders({ token, includeOrigin: false })
  });
  const text = await response.text();
  if (!response.ok) {
    console.warn(`Get sessions failed: ${response.status} - ${text.slice(0, 200)}`);
    return [];
  }
  let sessionsResponse;
  try {
    sessionsResponse = JSON.parse(text);
  } catch {
    return [];
  }
  if (sessionsResponse.requestStatus.statusCode !== 1) {
    console.warn(`Get sessions API error: ${sessionsResponse.requestStatus.statusDescription}`);
    return [];
  }
  const activeSessions = sessionsResponse.sessions.filter((s) => s.status === 1 || s.status === 2 || s.status === 3).map((s) => {
    const appId = s.sessionRequestData?.appId ? Number(s.sessionRequestData.appId) : 0;
    const connInfo = s.connectionInfo?.find((conn) => conn.usage === 14 && conn.ip);
    const rawConnIp = connInfo?.ip;
    const connIp = Array.isArray(rawConnIp) ? rawConnIp[0] : rawConnIp;
    const rawControlIp = s.sessionControlInfo?.ip;
    const controlIp = Array.isArray(rawControlIp) ? rawControlIp[0] : rawControlIp;
    const serverIp = connIp ?? controlIp;
    const signalingUrl = connIp ? `wss://${connIp}:443/nvst/` : controlIp ? `wss://${controlIp}:443/nvst/` : void 0;
    const monitorSettings = s.monitorSettings?.[0];
    const resolution = monitorSettings ? `${monitorSettings.widthInPixels ?? 0}x${monitorSettings.heightInPixels ?? 0}` : void 0;
    const fps = monitorSettings?.framesPerSecond ?? void 0;
    return {
      sessionId: s.sessionId,
      appId,
      gpuType: s.gpuType,
      status: s.status,
      streamingBaseUrl: base,
      serverIp,
      signalingUrl,
      resolution,
      fps
    };
  });
  return activeSessions;
}
function buildClaimRequestBody(sessionId, appId, settings) {
  const deviceId = getStableDeviceId();
  const subSessionId = crypto.randomUUID();
  const timezoneMs = timezoneOffsetMs();
  return {
    action: 2,
    data: "RESUME",
    sessionRequestData: {
      // Minimal fields required for resume - NO streaming parameter renegotiation
      audioMode: 2,
      remoteControllersBitmap: 0,
      sdrHdrMode: 0,
      networkTestSessionId: null,
      availableSupportedControllers: [],
      clientVersion: "30.0",
      deviceHashId: deviceId,
      internalTitle: null,
      clientPlatformName: "windows",
      metaData: [
        { key: "SubSessionId", value: subSessionId },
        { key: "wssignaling", value: "1" },
        { key: "GSStreamerType", value: "WebRTC" },
        { key: "networkType", value: "Unknown" },
        { key: "ClientImeSupport", value: "0" },
        { key: "surroundAudioInfo", value: "2" }
      ],
      surroundAudioInfo: 0,
      clientTimezoneOffset: timezoneMs,
      clientIdentification: "GFN-PC",
      parentSessionId: null,
      appId: parseInt(appId, 10),
      streamerVersion: 1,
      appLaunchMode: 1,
      sdkVersion: "1.0",
      enhancedStreamMode: 1,
      useOps: true,
      clientDisplayHdrCapabilities: null,
      accountLinked: true,
      partnerCustomData: "",
      enablePersistingInGameSettings: true,
      secureRTSPSupported: false,
      userAge: 26
    },
    metaData: []
  };
}
async function claimSession(input) {
  if (!input.token) {
    throw new Error("Missing token for session claim");
  }
  const deviceId = input.deviceId ?? getStableDeviceId();
  const clientId = input.clientId ?? crypto.randomUUID();
  const appId = input.appId ?? "0";
  const settings = input.settings ?? {
    keyboardLayout: DEFAULT_KEYBOARD_LAYOUT,
    gameLanguage: "en_US"
  };
  const keyboardLayout = resolveGfnKeyboardLayout(settings.keyboardLayout ?? DEFAULT_KEYBOARD_LAYOUT, process.platform);
  const languageCode = settings.gameLanguage ?? "en_US";
  let effectiveServerIp = input.serverIp;
  console.log(`[CloudMatch] claimSession: input serverIp=${input.serverIp}, isZone=${isZoneHostname(input.serverIp)}`);
  if (isZoneHostname(effectiveServerIp)) {
    const zoneBase = `https://${effectiveServerIp}`;
    const prefetchUrl = `${zoneBase}/v2/session/${input.sessionId}`;
    console.log(`[CloudMatch] claimSession: pre-flight query ${prefetchUrl}`);
    const prefetchHeaders = buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
    try {
      const prefetchResp = await fetch(prefetchUrl, { method: "GET", headers: prefetchHeaders });
      console.log(`[CloudMatch] claimSession: pre-flight response status=${prefetchResp.status}`);
      if (prefetchResp.ok) {
        const prefetchPayload = JSON.parse(await prefetchResp.text());
        const realIp = streamingServerIp(prefetchPayload);
        console.log(`[CloudMatch] claimSession: extracted realIp=${realIp}, isZone=${realIp ? isZoneHostname(realIp) : "N/A"}`);
        if (realIp) {
          effectiveServerIp = realIp;
          const ipType = isZoneHostname(realIp) ? "zone LB" : "direct IP";
          console.log(`[CloudMatch] claimSession: using extracted ${ipType}: ${realIp}`);
        }
      } else {
        console.warn(`[CloudMatch] claimSession: pre-flight returned HTTP ${prefetchResp.status}, text=${await prefetchResp.text()}`);
      }
    } catch (e) {
      console.warn("[CloudMatch] claimSession: pre-flight poll failed, proceeding with zone hostname:", e);
    }
  }
  const claimUrl = `https://${effectiveServerIp}/v2/session/${input.sessionId}?${new URLSearchParams({ keyboardLayout, languageCode }).toString()}`;
  let preClaimStatus = null;
  let shouldSendResumeClaim = true;
  try {
    const validationUrl = `https://${effectiveServerIp}/v2/session/${input.sessionId}`;
    const validationHeaders = buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
    const validationResp = await fetch(validationUrl, { method: "GET", headers: validationHeaders });
    if (validationResp.ok) {
      const validationText = await validationResp.text();
      const validationPayload = JSON.parse(validationText);
      preClaimStatus = validationPayload.session?.status ?? 0;
      const errorCode = validationPayload.session?.errorCode ?? 0;
      console.log(`[CloudMatch] claimSession: pre-claim validation status=${preClaimStatus}, errorCode=${errorCode}`);
      console.log(`[CloudMatch] claimSession: validation response (first 1000 chars): ${validationText.slice(0, 1e3)}`);
      if (preClaimStatus === 1) {
        console.log(`[CloudMatch] claimSession: session is still launching (status=1), skipping RESUME claim — polling directly to ready state`);
      } else if (input.recoveryMode === true && (preClaimStatus === 2 || preClaimStatus === 3)) {
        shouldSendResumeClaim = false;
        console.log(
          `[CloudMatch] claimSession: recoveryMode and session already ready (status=${preClaimStatus}); skipping redundant RESUME claim`
        );
      } else if (preClaimStatus !== 2 && preClaimStatus !== 3) {
        console.warn(`[CloudMatch] claimSession: session not in ready state (status=${preClaimStatus}), claim may fail`);
      }
    } else {
      console.warn(`[CloudMatch] claimSession: pre-claim validation returned HTTP ${validationResp.status}`);
    }
  } catch (e) {
    console.warn("[CloudMatch] claimSession: pre-claim validation failed:", e);
  }
  if (preClaimStatus !== 1 && shouldSendResumeClaim) {
    const payload = buildClaimRequestBody(input.sessionId, appId);
    const headers = buildGfnCloudMatchClaimHeaders({ token: input.token, clientId, deviceId });
    console.log(`[CloudMatch] claimSession PUT ${claimUrl}`);
    console.log(`[CloudMatch] claimSession body: ${JSON.stringify(payload)}`);
    const response = await fetch(claimUrl, {
      method: "PUT",
      headers,
      body: JSON.stringify(payload)
    });
    const { text, payload: apiResponse } = await readCloudMatchJson(response, {
      onText: (text2) => {
        console.log(`[CloudMatch] claimSession response: HTTP ${response.status}`);
        console.log(`[CloudMatch] claimSession response body FULL: ${text2}`);
      }
    });
    if (apiResponse.requestStatus.statusCode !== 1) {
      throw SessionError.fromResponse(200, text);
    }
  }
  const getUrl = `https://${effectiveServerIp}/v2/session/${input.sessionId}`;
  const maxAttempts = 60;
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    if (attempt > 1) {
      await new Promise((resolve2) => setTimeout(resolve2, 1e3));
    }
    const pollHeaders = buildGfnCloudMatchHeaders({ token: input.token, clientId, deviceId, includeOrigin: false });
    const pollResponse = await fetch(getUrl, {
      method: "GET",
      headers: pollHeaders
    });
    if (!pollResponse.ok) {
      continue;
    }
    const pollText = await pollResponse.text();
    let pollApiResponse;
    try {
      pollApiResponse = JSON.parse(pollText);
    } catch {
      continue;
    }
    const sessionData = pollApiResponse.session;
    if (sessionData.status === 2 || sessionData.status === 3) {
      const signaling = resolveSignaling(pollApiResponse);
      const queuePosition = extractQueuePosition(pollApiResponse);
      const negotiatedStreamProfile = extractNegotiatedStreamProfile(pollApiResponse);
      const requestedStreamingFeatures = normalizeStreamingFeatures(
        pollApiResponse.session.sessionRequestData?.requestedStreamingFeatures
      );
      const finalizedStreamingFeatures = normalizeStreamingFeatures(
        pollApiResponse.session.finalizedStreamingFeatures
      );
      console.log(
        `[CloudMatch] claimed negotiated streaming features: requested=${JSON.stringify(requestedStreamingFeatures ?? {})} finalized=${JSON.stringify(finalizedStreamingFeatures ?? {})} cloudGsync=${negotiatedStreamProfile?.enableCloudGsync ?? "n/a"}, reflex=${negotiatedStreamProfile?.enableReflex ?? "n/a"}, l4s=${negotiatedStreamProfile?.enableL4S ?? "n/a"}`
      );
      return {
        sessionId: sessionData.sessionId,
        status: sessionData.status,
        queuePosition,
        zone: "",
        // Zone not applicable for claimed sessions
        streamingBaseUrl: `https://${effectiveServerIp}`,
        serverIp: signaling.serverIp,
        signalingServer: signaling.signalingServer,
        signalingUrl: signaling.signalingUrl,
        gpuType: sessionData.gpuType,
        iceServers: await normalizeIceServers(pollApiResponse),
        mediaConnectionInfo: signaling.mediaConnectionInfo,
        negotiatedStreamProfile: negotiatedStreamProfile ?? extractNegotiatedStreamProfile(pollApiResponse),
        requestedStreamingFeatures,
        finalizedStreamingFeatures,
        clientId,
        deviceId
      };
    }
    if (sessionData.status > 3 && sessionData.status !== 6) {
      break;
    }
  }
  throw new Error("Session did not become ready after claiming");
}
const MES_URL = "https://mes.geforcenow.com/v4/subscriptions";
function parseMinutes(value) {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  if (typeof value === "string" && value.trim().length > 0) {
    const parsed = Number(value);
    if (Number.isFinite(parsed)) {
      return parsed;
    }
  }
  return void 0;
}
function parseNumberText(value) {
  if (typeof value !== "string" || value.trim().length === 0) {
    return void 0;
  }
  const parsed = Number(value);
  if (!Number.isFinite(parsed)) {
    return void 0;
  }
  return parsed;
}
function parseIsoDate(value) {
  return typeof value === "string" && value.length > 0 ? value : void 0;
}
async function fetchSubscription(token, userId, vpcId = "NP-AMS-08") {
  const url = new URL(MES_URL);
  url.searchParams.append("serviceName", "gfn_pc");
  url.searchParams.append("languageCode", "en_US");
  url.searchParams.append("vpcId", vpcId);
  url.searchParams.append("userId", userId);
  const response = await fetch(url.toString(), {
    headers: buildGfnLcarsHeaders({
      token,
      clientType: "NATIVE",
      clientStreamer: "NVIDIA-CLASSIC"
    })
  });
  if (!response.ok) {
    const body = await response.text();
    throw new Error(`Subscription API failed with status ${response.status}: ${body}`);
  }
  const data = await response.json();
  const membershipTier = data.membershipTier ?? "FREE";
  const allottedMinutes = parseMinutes(data.allottedTimeInMinutes) ?? 0;
  const purchasedMinutes = parseMinutes(data.purchasedTimeInMinutes) ?? 0;
  const rolledOverMinutes = parseMinutes(data.rolledOverTimeInMinutes) ?? 0;
  const fallbackTotalMinutes = allottedMinutes + purchasedMinutes + rolledOverMinutes;
  const totalMinutes = parseMinutes(data.totalTimeInMinutes) ?? fallbackTotalMinutes;
  const remainingMinutes = parseMinutes(data.remainingTimeInMinutes) ?? 0;
  const usedMinutes = Math.max(totalMinutes - remainingMinutes, 0);
  const allottedHours = allottedMinutes / 60;
  const purchasedHours = purchasedMinutes / 60;
  const rolledOverHours = rolledOverMinutes / 60;
  const usedHours = usedMinutes / 60;
  const remainingHours = remainingMinutes / 60;
  const totalHours = totalMinutes / 60;
  const isUnlimited = data.subType === "UNLIMITED";
  let storageAddon;
  const storageAddonResponse = data.addons?.find(
    (addon) => addon.type === "STORAGE" && addon.subType === "PERMANENT_STORAGE" && addon.status === "OK"
  );
  if (storageAddonResponse) {
    const sizeAttr = storageAddonResponse.attributes?.find(
      (attr) => attr.key === "TOTAL_STORAGE_SIZE_IN_GB"
    );
    const usedAttr = storageAddonResponse.attributes?.find(
      (attr) => attr.key === "USED_STORAGE_SIZE_IN_GB"
    );
    const regionNameAttr = storageAddonResponse.attributes?.find(
      (attr) => attr.key === "STORAGE_METRO_REGION_NAME"
    );
    const regionCodeAttr = storageAddonResponse.attributes?.find(
      (attr) => attr.key === "STORAGE_METRO_REGION"
    );
    const sizeGb = parseNumberText(sizeAttr?.textValue);
    const usedGb = parseNumberText(usedAttr?.textValue);
    const regionName = regionNameAttr?.textValue;
    const regionCode = regionCodeAttr?.textValue;
    storageAddon = {
      type: "PERMANENT_STORAGE",
      sizeGb,
      usedGb,
      regionName,
      regionCode
    };
  }
  const entitledResolutions = [];
  if (data.features?.resolutions) {
    for (const res of data.features.resolutions) {
      entitledResolutions.push({
        width: res.widthInPixels,
        height: res.heightInPixels,
        fps: res.framesPerSecond
      });
    }
    entitledResolutions.sort((a, b) => {
      if (b.width !== a.width) return b.width - a.width;
      if (b.height !== a.height) return b.height - a.height;
      return b.fps - a.fps;
    });
  }
  return {
    membershipTier,
    subscriptionType: data.type,
    subscriptionSubType: data.subType,
    allottedHours,
    purchasedHours,
    rolledOverHours,
    usedHours,
    remainingHours,
    totalHours,
    firstEntitlementStartDateTime: parseIsoDate(data.firstEntitlementStartDateTime),
    serverRegionId: vpcId,
    currentSpanStartDateTime: parseIsoDate(data.currentSpanStartDateTime),
    currentSpanEndDateTime: parseIsoDate(data.currentSpanEndDateTime),
    notifyUserWhenTimeRemainingInMinutes: parseMinutes(
      data.notifications?.notifyUserWhenTimeRemainingInMinutes
    ),
    notifyUserOnSessionWhenRemainingTimeInMinutes: parseMinutes(
      data.notifications?.notifyUserOnSessionWhenRemainingTimeInMinutes
    ),
    state: data.currentSubscriptionState?.state,
    isGamePlayAllowed: data.currentSubscriptionState?.isGamePlayAllowed,
    isUnlimited,
    storageAddon,
    entitledResolutions
  };
}
async function fetchDynamicRegions(token, streamingBaseUrl) {
  const base = streamingBaseUrl.endsWith("/") ? streamingBaseUrl : `${streamingBaseUrl}/`;
  const url = `${base}v2/serverInfo`;
  const headers = buildGfnLcarsHeaders({
    token,
    clientType: "BROWSER",
    clientStreamer: "WEBRTC"
  });
  let response;
  try {
    response = await fetch(url, { headers });
  } catch {
    return { regions: [], vpcId: null };
  }
  if (!response.ok) {
    return { regions: [], vpcId: null };
  }
  const data = await response.json();
  const vpcId = data.requestStatus?.serverId ?? null;
  const regions = (data.metaData ?? []).filter(
    (entry) => entry.value.startsWith("https://") && entry.key !== "gfn-regions" && !entry.key.startsWith("gfn-")
  ).map((entry) => ({
    name: entry.key,
    url: entry.value.endsWith("/") ? entry.value : `${entry.value}/`
  })).sort((a, b) => a.name.localeCompare(b.name));
  return { regions, vpcId };
}
const SERVICE_URLS_ENDPOINT = "https://pcs.geforcenow.com/v1/serviceUrls";
const TOKEN_ENDPOINT = "https://login.nvidia.com/token";
const CLIENT_TOKEN_ENDPOINT = "https://login.nvidia.com/client_token";
const USERINFO_ENDPOINT = "https://login.nvidia.com/userinfo";
const AUTH_ENDPOINT = "https://login.nvidia.com/authorize";
const CLIENT_ID = "ZU7sPN-miLujMD95LfOQ453IB0AtjM8sMyvgJ9wCXEQ";
const SCOPES = "openid consent email tk_client age";
const DEFAULT_IDP_ID = "PDiAhv2kJTFeQ7WOPqiQ2tRZ7lGhR2X11dXvM4TZSxg";
const REDIRECT_PORTS = [2259, 6460, 7119, 8870, 9096];
const TOKEN_REFRESH_WINDOW_MS = 10 * 60 * 1e3;
const CLIENT_TOKEN_REFRESH_WINDOW_MS = 5 * 60 * 1e3;
function defaultProvider() {
  return {
    idpId: DEFAULT_IDP_ID,
    code: "NVIDIA",
    displayName: "NVIDIA",
    streamingServiceUrl: "https://prod.cloudmatchbeta.nvidiagrid.net/",
    priority: 0
  };
}
function normalizeProvider(provider) {
  return {
    ...provider,
    streamingServiceUrl: provider.streamingServiceUrl.endsWith("/") ? provider.streamingServiceUrl : `${provider.streamingServiceUrl}/`
  };
}
function decodeBase64Url(value) {
  const normalized = value.replace(/-/g, "+").replace(/_/g, "/");
  const padding = normalized.length % 4;
  const padded = padding === 0 ? normalized : `${normalized}${"=".repeat(4 - padding)}`;
  return Buffer.from(padded, "base64").toString("utf8");
}
function parseJwtPayload(token) {
  const parts = token.split(".");
  if (parts.length !== 3) {
    return null;
  }
  try {
    const payload = decodeBase64Url(parts[1]);
    return JSON.parse(payload);
  } catch {
    return null;
  }
}
function toExpiresAt(expiresInSeconds, defaultSeconds = 86400) {
  return Date.now() + (expiresInSeconds ?? defaultSeconds) * 1e3;
}
function isExpired(expiresAt) {
  if (!expiresAt) {
    return true;
  }
  return expiresAt <= Date.now();
}
function isNearExpiry(expiresAt, windowMs) {
  if (!expiresAt) {
    return true;
  }
  return expiresAt - Date.now() < windowMs;
}
function generateDeviceId() {
  const host = os.hostname();
  const username = os.userInfo().username;
  return createHash("sha256").update(`${host}:${username}:opennow-stable`).digest("hex");
}
function generatePkce() {
  const verifier = randomBytes(64).toString("base64").replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "").slice(0, 86);
  const challenge = createHash("sha256").update(verifier).digest("base64").replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
  return { verifier, challenge };
}
function buildAuthUrl(provider, challenge, port) {
  const redirectUri = `http://localhost:${port}`;
  const nonce = randomBytes(16).toString("hex");
  const params = new URLSearchParams({
    response_type: "code",
    device_id: generateDeviceId(),
    scope: SCOPES,
    client_id: CLIENT_ID,
    redirect_uri: redirectUri,
    ui_locales: "en_US",
    nonce,
    prompt: "select_account",
    code_challenge: challenge,
    code_challenge_method: "S256",
    idp_id: provider.idpId
  });
  return `${AUTH_ENDPOINT}?${params.toString()}`;
}
async function isPortAvailable(port) {
  return new Promise((resolve2) => {
    const server = net__default.createServer();
    server.once("error", () => resolve2(false));
    server.once("listening", () => {
      server.close(() => resolve2(true));
    });
    server.listen(port, "127.0.0.1");
  });
}
async function findAvailablePort() {
  for (const port of REDIRECT_PORTS) {
    if (await isPortAvailable(port)) {
      return port;
    }
  }
  throw new Error("No available OAuth callback ports");
}
async function waitForAuthorizationCode(port, timeoutMs) {
  return new Promise((resolve2, reject) => {
    const server = createServer((request, response) => {
      const url = new URL(request.url ?? "/", `http://localhost:${port}`);
      const code = url.searchParams.get("code");
      const error = url.searchParams.get("error");
      const html = `<!doctype html><html><head><meta charset="utf-8"><title>OpenNOW Login</title></head><body style="font-family:Segoe UI,Arial,sans-serif;background:#0b1220;color:#dbe7ff;display:flex;justify-content:center;align-items:center;height:100vh"><div style="background:#111a2c;padding:24px 28px;border:1px solid #30425f;border-radius:12px;max-width:460px"><h2 style="margin-top:0">OpenNOW Login</h2><p>${code ? "Login complete. You can close this window and return to OpenNOW Stable." : "Login failed or was cancelled. You can close this window and return to OpenNOW Stable."}</p></div></body></html>`;
      response.statusCode = 200;
      response.setHeader("Content-Type", "text/html; charset=utf-8");
      response.end(html);
      server.close(() => {
        if (code) {
          resolve2(code);
          return;
        }
        reject(new Error(error ?? "Authorization failed"));
      });
    });
    server.listen(port, "127.0.0.1", () => {
      const timer = setTimeout(() => {
        server.close(() => reject(new Error("Timed out waiting for OAuth callback")));
      }, timeoutMs);
      server.once("close", () => clearTimeout(timer));
    });
  });
}
async function exchangeAuthorizationCode(code, verifier, port) {
  const body = new URLSearchParams({
    grant_type: "authorization_code",
    code,
    redirect_uri: `http://localhost:${port}`,
    code_verifier: verifier
  });
  const response = await fetch(TOKEN_ENDPOINT, {
    method: "POST",
    headers: buildNvidiaAuthHeaders({
      contentType: "application/x-www-form-urlencoded; charset=UTF-8",
      includeReferer: true
    }),
    body
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Token exchange failed (${response.status}): ${text.slice(0, 400)}`);
  }
  const payload = await response.json();
  return {
    accessToken: payload.access_token,
    refreshToken: payload.refresh_token,
    idToken: payload.id_token,
    expiresAt: toExpiresAt(payload.expires_in)
  };
}
async function refreshAuthTokens(refreshToken) {
  const body = new URLSearchParams({
    grant_type: "refresh_token",
    refresh_token: refreshToken,
    client_id: CLIENT_ID
  });
  const response = await fetch(TOKEN_ENDPOINT, {
    method: "POST",
    headers: buildNvidiaAuthHeaders({
      contentType: "application/x-www-form-urlencoded; charset=UTF-8"
    }),
    body
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Token refresh failed (${response.status}): ${text.slice(0, 400)}`);
  }
  const payload = await response.json();
  return {
    accessToken: payload.access_token,
    refreshToken: payload.refresh_token ?? refreshToken,
    idToken: payload.id_token,
    expiresAt: toExpiresAt(payload.expires_in)
  };
}
async function requestClientToken(accessToken) {
  const response = await fetch(CLIENT_TOKEN_ENDPOINT, {
    headers: buildNvidiaAuthHeaders({ bearerToken: accessToken })
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Client token request failed (${response.status}): ${text.slice(0, 400)}`);
  }
  const payload = await response.json();
  const expiresAt = toExpiresAt(payload.expires_in);
  return {
    token: payload.client_token,
    expiresAt,
    lifetimeMs: Math.max(0, expiresAt - Date.now())
  };
}
async function refreshWithClientToken(clientToken, userId) {
  const body = new URLSearchParams({
    grant_type: "urn:ietf:params:oauth:grant-type:client_token",
    client_token: clientToken,
    client_id: CLIENT_ID,
    sub: userId
  });
  const response = await fetch(TOKEN_ENDPOINT, {
    method: "POST",
    headers: buildNvidiaAuthHeaders({
      contentType: "application/x-www-form-urlencoded; charset=UTF-8"
    }),
    body
  });
  if (!response.ok) {
    const text = await response.text();
    throw new Error(`Client-token refresh failed (${response.status}): ${text.slice(0, 400)}`);
  }
  return await response.json();
}
function mergeTokenSnapshot(base, refreshed) {
  return {
    accessToken: refreshed.access_token,
    refreshToken: refreshed.refresh_token ?? base.refreshToken,
    idToken: refreshed.id_token,
    expiresAt: toExpiresAt(refreshed.expires_in),
    clientToken: refreshed.client_token ?? base.clientToken,
    clientTokenExpiresAt: base.clientTokenExpiresAt,
    clientTokenLifetimeMs: base.clientTokenLifetimeMs
  };
}
function gravatarUrl(email, size = 80) {
  const normalized = email.trim().toLowerCase();
  const hash = createHash("md5").update(normalized).digest("hex");
  return `https://www.gravatar.com/avatar/${hash}?s=${size}&d=identicon`;
}
async function fetchUserInfo(tokens) {
  const jwtToken = tokens.idToken ?? tokens.accessToken;
  const parsed = parseJwtPayload(jwtToken);
  if (parsed?.sub) {
    const emailFromToken = parsed.email;
    const pictureFromToken = parsed.picture;
    if (emailFromToken || pictureFromToken) {
      const avatar2 = pictureFromToken ?? (emailFromToken ? gravatarUrl(emailFromToken) : void 0);
      return {
        userId: parsed.sub,
        displayName: parsed.preferred_username ?? emailFromToken?.split("@")[0] ?? "User",
        email: emailFromToken,
        avatarUrl: avatar2,
        membershipTier: parsed.gfn_tier ?? "FREE"
      };
    }
  }
  const response = await fetch(USERINFO_ENDPOINT, {
    headers: buildNvidiaAuthHeaders({
      bearerToken: tokens.accessToken,
      accept: "application/json"
    })
  });
  if (!response.ok) {
    throw new Error(`User info failed (${response.status})`);
  }
  const payload = await response.json();
  const email = payload.email;
  const avatar = payload.picture ?? (email ? gravatarUrl(email) : void 0);
  return {
    userId: payload.sub,
    displayName: payload.preferred_username ?? email?.split("@")[0] ?? "User",
    email,
    avatarUrl: avatar,
    membershipTier: "FREE"
  };
}
class AuthService {
  constructor(statePath) {
    this.statePath = statePath;
  }
  statePath;
  providers = [];
  sessions = /* @__PURE__ */ new Map();
  activeUserId = null;
  selectedProvider = defaultProvider();
  cachedSubscription = null;
  cachedVpcId = null;
  async initialize() {
    try {
      await access(this.statePath);
    } catch {
      await mkdir(dirname(this.statePath), { recursive: true });
      await this.persist();
      return;
    }
    try {
      const raw = await readFile(this.statePath, "utf8");
      const parsed = JSON.parse(raw);
      if (parsed.selectedProvider) {
        this.selectedProvider = normalizeProvider(parsed.selectedProvider);
      }
      this.sessions.clear();
      if (Array.isArray(parsed.sessions)) {
        for (const persistedSession of parsed.sessions) {
          if (!persistedSession?.user?.userId) {
            continue;
          }
          this.sessions.set(persistedSession.user.userId, {
            ...persistedSession,
            provider: normalizeProvider(persistedSession.provider)
          });
        }
      } else if (parsed.session?.user?.userId) {
        this.sessions.set(parsed.session.user.userId, {
          ...parsed.session,
          provider: normalizeProvider(parsed.session.provider)
        });
      }
      if (typeof parsed.activeUserId === "string" && this.sessions.has(parsed.activeUserId)) {
        this.activeUserId = parsed.activeUserId;
      } else {
        this.activeUserId = this.sessions.keys().next().value ?? null;
      }
      const restoredSession = this.getSession();
      if (restoredSession) {
        this.selectedProvider = restoredSession.provider;
        await this.enrichUserTier();
        await this.persist();
      }
    } catch {
      this.sessions.clear();
      this.activeUserId = null;
      this.selectedProvider = defaultProvider();
      await this.persist();
    }
  }
  async persist() {
    const payload = {
      sessions: Array.from(this.sessions.values()),
      activeUserId: this.activeUserId,
      selectedProvider: this.selectedProvider
    };
    await mkdir(dirname(this.statePath), { recursive: true });
    await writeFile(this.statePath, JSON.stringify(payload, null, 2), "utf8");
  }
  async ensureClientToken(tokens, userId) {
    const hasUsableClientToken = Boolean(tokens.clientToken) && !isNearExpiry(tokens.clientTokenExpiresAt, CLIENT_TOKEN_REFRESH_WINDOW_MS);
    if (hasUsableClientToken) {
      return tokens;
    }
    if (isExpired(tokens.expiresAt)) {
      return tokens;
    }
    const clientToken = await requestClientToken(tokens.accessToken);
    return {
      ...tokens,
      clientToken: clientToken.token,
      clientTokenExpiresAt: clientToken.expiresAt,
      clientTokenLifetimeMs: clientToken.lifetimeMs
    };
  }
  async getProviders() {
    if (this.providers.length > 0) {
      return this.providers;
    }
    let response;
    try {
      response = await fetch(SERVICE_URLS_ENDPOINT, {
        headers: {
          Accept: "application/json",
          "User-Agent": GFN_USER_AGENT
        }
      });
    } catch (error) {
      console.warn("Failed to fetch providers, using default:", error);
      this.providers = [defaultProvider()];
      return this.providers;
    }
    if (!response.ok) {
      console.warn(`Providers fetch failed with status ${response.status}, using default`);
      this.providers = [defaultProvider()];
      return this.providers;
    }
    try {
      const payload = await response.json();
      const endpoints = payload.gfnServiceInfo?.gfnServiceEndpoints ?? [];
      const providers = endpoints.map((entry) => ({
        idpId: entry.idpId,
        code: entry.loginProviderCode,
        displayName: entry.loginProviderCode === "BPC" ? "bro.game" : entry.loginProviderDisplayName,
        streamingServiceUrl: entry.streamingServiceUrl,
        priority: entry.loginProviderPriority ?? 0
      })).sort((a, b) => a.priority - b.priority).map(normalizeProvider);
      this.providers = providers.length > 0 ? providers : [defaultProvider()];
      console.log(`Loaded ${this.providers.length} providers`);
      return this.providers;
    } catch (error) {
      console.warn("Failed to parse providers response, using default:", error);
      this.providers = [defaultProvider()];
      return this.providers;
    }
  }
  setSession(session2) {
    if (!session2) {
      this.sessions.clear();
      this.activeUserId = null;
      this.selectedProvider = defaultProvider();
      this.clearSubscriptionCache();
      this.clearVpcCache();
      void this.persist();
      return;
    }
    const normalized = {
      ...session2,
      provider: normalizeProvider(session2.provider)
    };
    this.sessions.set(normalized.user.userId, normalized);
    this.activeUserId = normalized.user.userId;
    this.selectedProvider = normalized.provider;
    this.clearSubscriptionCache();
    this.clearVpcCache();
    void this.persist();
  }
  getSession() {
    if (!this.activeUserId) {
      return null;
    }
    return this.sessions.get(this.activeUserId) ?? null;
  }
  setActiveAccount(userId) {
    this.activeUserId = userId && this.sessions.has(userId) ? userId : null;
    this.selectedProvider = this.getSession()?.provider ?? defaultProvider();
    this.clearSubscriptionCache();
    this.clearVpcCache();
  }
  getSavedAccounts() {
    return Array.from(this.sessions.values()).map((session2) => ({
      userId: session2.user.userId,
      displayName: session2.user.displayName,
      email: session2.user.email,
      avatarUrl: session2.user.avatarUrl,
      membershipTier: session2.user.membershipTier,
      providerCode: session2.provider.code
    }));
  }
  async switchAccount(userId) {
    const target = this.sessions.get(userId);
    if (!target) {
      throw new Error("Saved account not found");
    }
    const previousActiveUserId = this.activeUserId;
    const previousSelectedProvider = this.selectedProvider;
    this.activeUserId = userId;
    this.selectedProvider = target.provider;
    this.clearSubscriptionCache();
    this.clearVpcCache();
    const result = await this.ensureValidSessionWithStatus(true, userId);
    const missingRefreshToken = result.refresh.outcome === "missing_refresh_token";
    const refreshFailed = result.refresh.outcome === "failed";
    const switchedUserMismatch = result.session?.user.userId !== userId;
    if (!result.session || refreshFailed || missingRefreshToken || switchedUserMismatch) {
      const fallbackMessage = "Failed to switch account due to an invalid or expired session.";
      if (missingRefreshToken) {
        await this.removeAccount(userId);
        this.setActiveAccount(previousActiveUserId);
        await this.persist();
        throw new Error("Saved login for this account is incomplete. Please log in to this account again.");
      }
      this.activeUserId = previousActiveUserId;
      this.selectedProvider = previousActiveUserId && this.sessions.has(previousActiveUserId) ? previousSelectedProvider : this.getSession()?.provider ?? defaultProvider();
      this.clearSubscriptionCache();
      this.clearVpcCache();
      await this.persist();
      if (switchedUserMismatch) {
        throw new Error("Switched session did not match the selected account.");
      }
      throw new Error(result.refresh.message || fallbackMessage);
    }
    return result.session;
  }
  async removeAccount(userId) {
    const removed = this.sessions.delete(userId);
    if (!removed) {
      return;
    }
    if (this.activeUserId === userId) {
      this.setActiveAccount(this.sessions.keys().next().value ?? null);
    } else {
      this.clearSubscriptionCache();
      this.clearVpcCache();
    }
    await this.persist();
  }
  async logoutAll() {
    this.sessions.clear();
    this.activeUserId = null;
    this.selectedProvider = defaultProvider();
    this.cachedSubscription = null;
    this.clearVpcCache();
    await this.persist();
  }
  getSelectedProvider() {
    return this.getSession()?.provider ?? this.selectedProvider;
  }
  async getRegions(explicitToken) {
    const provider = this.getSelectedProvider();
    const base = provider.streamingServiceUrl.endsWith("/") ? provider.streamingServiceUrl : `${provider.streamingServiceUrl}/`;
    let token = explicitToken;
    if (!token) {
      const session2 = await this.ensureValidSession();
      token = session2 ? session2.tokens.idToken ?? session2.tokens.accessToken : void 0;
    }
    const headers = buildGfnLcarsHeaders({
      token,
      clientType: "BROWSER",
      clientStreamer: "WEBRTC",
      includeUserAgent: true
    });
    let response;
    try {
      response = await fetch(`${base}v2/serverInfo`, {
        headers
      });
    } catch {
      return [];
    }
    if (!response.ok) {
      return [];
    }
    const payload = await response.json();
    const regions = (payload.metaData ?? []).filter((entry) => entry.value.startsWith("https://")).filter((entry) => entry.key !== "gfn-regions" && !entry.key.startsWith("gfn-")).map((entry) => ({
      name: entry.key,
      url: entry.value.endsWith("/") ? entry.value : `${entry.value}/`
    })).sort((a, b) => a.name.localeCompare(b.name));
    return regions;
  }
  async login(input) {
    const providers = await this.getProviders();
    const selected = providers.find((provider) => provider.idpId === input.providerIdpId) ?? this.selectedProvider ?? providers[0] ?? defaultProvider();
    this.selectedProvider = normalizeProvider(selected);
    const { verifier, challenge } = generatePkce();
    const port = await findAvailablePort();
    const authUrl = buildAuthUrl(this.selectedProvider, challenge, port);
    const codePromise = waitForAuthorizationCode(port, 12e4);
    await shell.openExternal(authUrl);
    const code = await codePromise;
    const initialTokens = await exchangeAuthorizationCode(code, verifier, port);
    const user = await fetchUserInfo(initialTokens);
    console.debug("auth: fetched user info during login", { userId: user.userId, email: user.email, avatarUrl: user.avatarUrl });
    let tokens = initialTokens;
    try {
      tokens = await this.ensureClientToken(initialTokens, user.userId);
    } catch (error) {
      console.warn("Unable to fetch client token after login. Falling back to OAuth token only:", error);
    }
    const nextSession = {
      provider: this.selectedProvider,
      tokens,
      user
    };
    this.sessions.set(user.userId, nextSession);
    this.activeUserId = user.userId;
    this.selectedProvider = nextSession.provider;
    this.clearSubscriptionCache();
    this.clearVpcCache();
    await this.enrichUserTier();
    await this.persist();
    return this.getSession();
  }
  async logout() {
    if (!this.activeUserId) {
      return;
    }
    this.sessions.delete(this.activeUserId);
    this.activeUserId = this.sessions.keys().next().value ?? null;
    this.selectedProvider = this.getSession()?.provider ?? defaultProvider();
    this.cachedSubscription = null;
    this.clearVpcCache();
    await this.persist();
  }
  /**
   * Fetch subscription info for the current user.
   * Uses caching - call clearSubscriptionCache() to force refresh.
   */
  async getSubscription() {
    if (this.cachedSubscription) {
      return this.cachedSubscription;
    }
    const session2 = await this.ensureValidSession();
    if (!session2) {
      return null;
    }
    const token = session2.tokens.idToken ?? session2.tokens.accessToken;
    const userId = session2.user.userId;
    const { vpcId } = await fetchDynamicRegions(token, session2.provider.streamingServiceUrl);
    const subscription = await fetchSubscription(token, userId, vpcId ?? void 0);
    this.cachedSubscription = subscription;
    return subscription;
  }
  /**
   * Clear the cached subscription info.
   * Called automatically on logout.
   */
  clearSubscriptionCache() {
    this.cachedSubscription = null;
  }
  /**
   * Get the cached subscription without fetching.
   * Returns null if not cached.
   */
  getCachedSubscription() {
    return this.cachedSubscription;
  }
  /**
   * Get the VPC ID for the current provider.
   * Returns cached value if available, otherwise fetches from serverInfo endpoint.
   * The VPC ID is used for Alliance partner support and routing to correct data center.
   */
  async getVpcId(explicitToken) {
    if (this.cachedVpcId) {
      return this.cachedVpcId;
    }
    const provider = this.getSelectedProvider();
    const base = provider.streamingServiceUrl.endsWith("/") ? provider.streamingServiceUrl : `${provider.streamingServiceUrl}/`;
    let token = explicitToken;
    if (!token) {
      const session2 = await this.ensureValidSession();
      token = session2 ? session2.tokens.idToken ?? session2.tokens.accessToken : void 0;
    }
    const headers = buildGfnLcarsHeaders({
      token,
      clientType: "BROWSER",
      clientStreamer: "WEBRTC",
      includeUserAgent: true
    });
    try {
      const response = await fetch(`${base}v2/serverInfo`, {
        headers
      });
      if (!response.ok) {
        return null;
      }
      const payload = await response.json();
      const vpcId = payload.requestStatus?.serverId ?? null;
      if (vpcId) {
        this.cachedVpcId = vpcId;
      }
      return vpcId;
    } catch {
      return null;
    }
  }
  /**
   * Clear the cached VPC ID.
   * Called automatically on logout.
   */
  clearVpcCache() {
    this.cachedVpcId = null;
  }
  /**
   * Get the cached VPC ID without fetching.
   * Returns null if not cached.
   */
  getCachedVpcId() {
    return this.cachedVpcId;
  }
  /**
   * Enrich the current session's user with the real membership tier from MES API.
   * Falls back silently to the existing tier if the fetch fails.
   */
  async enrichUserTier() {
    const session2 = this.getSession();
    if (!session2) return;
    try {
      const subscription = await this.getSubscription();
      if (subscription && subscription.membershipTier) {
        this.sessions.set(session2.user.userId, {
          ...session2,
          user: {
            ...session2.user,
            membershipTier: subscription.membershipTier
          }
        });
        console.log(`Resolved membership tier: ${subscription.membershipTier}`);
      }
    } catch (error) {
      console.warn("Failed to fetch subscription tier, keeping fallback:", error);
    }
  }
  shouldRefresh(tokens) {
    return isNearExpiry(tokens.expiresAt, TOKEN_REFRESH_WINDOW_MS);
  }
  async ensureValidSessionWithStatus(forceRefresh = false, expectedUserId) {
    const currentSession = this.getSession();
    if (!currentSession) {
      return {
        session: null,
        refresh: {
          attempted: false,
          forced: forceRefresh,
          outcome: "not_attempted",
          message: "No saved session found."
        }
      };
    }
    const userId = currentSession.user.userId;
    let tokens = currentSession.tokens;
    if (!tokens.clientToken && !isExpired(tokens.expiresAt)) {
      try {
        const withClientToken = await this.ensureClientToken(tokens, userId);
        if (withClientToken.clientToken && withClientToken.clientToken !== tokens.clientToken) {
          this.sessions.set(userId, {
            ...currentSession,
            tokens: withClientToken
          });
          tokens = withClientToken;
          await this.persist();
        }
      } catch (error) {
        console.warn("Unable to bootstrap client token from saved session:", error);
      }
    }
    const shouldRefreshNow = forceRefresh || this.shouldRefresh(tokens);
    if (!shouldRefreshNow) {
      return {
        session: this.getSession(),
        refresh: {
          attempted: false,
          forced: forceRefresh,
          outcome: "not_attempted",
          message: "Session token is still valid."
        }
      };
    }
    const applyRefreshedTokens = async (refreshedTokens, source) => {
      const latestSession = this.getSession() ?? currentSession;
      const baseSession = latestSession.user.userId === userId ? latestSession : currentSession;
      const expectedRefreshUserId = expectedUserId ?? userId;
      let refreshedUser = null;
      let userInfoError;
      try {
        refreshedUser = await fetchUserInfo(refreshedTokens);
        console.debug("auth: fetched user info on token refresh", {
          userId: refreshedUser.userId,
          email: refreshedUser.email,
          avatarUrl: refreshedUser.avatarUrl
        });
      } catch (error) {
        console.warn("Token refresh succeeded but user info refresh failed. Keeping cached user:", error);
        userInfoError = error instanceof Error ? error.message : "Unknown error while fetching user info";
      }
      const resolvedUser = refreshedUser ?? baseSession.user;
      if (resolvedUser.userId !== expectedRefreshUserId) {
        return {
          session: baseSession,
          refresh: {
            attempted: true,
            forced: forceRefresh,
            outcome: "failed",
            message: refreshedUser ? "Token refresh returned a different account than expected." : "Token refresh kept a cached account identity that did not match the expected account.",
            error: refreshedUser ? `expected_user_id:${expectedRefreshUserId} actual_user_id:${refreshedUser.userId}` : userInfoError ? `expected_user_id:${expectedRefreshUserId} cached_user_id:${resolvedUser.userId} user_info_error:${userInfoError}` : `expected_user_id:${expectedRefreshUserId} cached_user_id:${resolvedUser.userId}`
          }
        };
      }
      const updatedSession = {
        provider: baseSession.provider,
        tokens: refreshedTokens,
        user: resolvedUser
      };
      this.sessions.set(updatedSession.user.userId, updatedSession);
      this.clearSubscriptionCache();
      await this.enrichUserTier();
      await this.persist();
      const sourceText = source === "client_token" ? "client token" : "refresh token";
      return {
        session: this.getSession(),
        refresh: {
          attempted: true,
          forced: forceRefresh,
          outcome: "refreshed",
          message: forceRefresh ? `Saved session token refreshed via ${sourceText}.` : `Session token refreshed via ${sourceText} because it was near expiry.`
        }
      };
    };
    const refreshErrors = [];
    if (tokens.clientToken) {
      try {
        const refreshedFromClientToken = await refreshWithClientToken(tokens.clientToken, userId);
        let refreshedTokens = mergeTokenSnapshot(tokens, refreshedFromClientToken);
        refreshedTokens = await this.ensureClientToken(refreshedTokens, userId);
        return applyRefreshedTokens(refreshedTokens, "client_token");
      } catch (error) {
        const message = error instanceof Error ? error.message : "Unknown error while refreshing with client token";
        refreshErrors.push(`client_token: ${message}`);
      }
    }
    if (tokens.refreshToken) {
      try {
        const refreshedOAuth = await refreshAuthTokens(tokens.refreshToken);
        let refreshedTokens = {
          ...tokens,
          ...refreshedOAuth,
          // OAuth refresh does not always return a new client token.
          clientToken: tokens.clientToken,
          clientTokenExpiresAt: tokens.clientTokenExpiresAt,
          clientTokenLifetimeMs: tokens.clientTokenLifetimeMs
        };
        refreshedTokens = await this.ensureClientToken(refreshedTokens, userId);
        return applyRefreshedTokens(refreshedTokens, "refresh_token");
      } catch (error) {
        const message = error instanceof Error ? error.message : "Unknown error while refreshing token";
        refreshErrors.push(`refresh_token: ${message}`);
      }
    }
    const errorText = refreshErrors.length > 0 ? refreshErrors.join(" | ") : void 0;
    const expired = isExpired(tokens.expiresAt);
    if (!tokens.clientToken && !tokens.refreshToken) {
      if (expired) {
        await this.logout();
        return {
          session: null,
          refresh: {
            attempted: true,
            forced: forceRefresh,
            outcome: "missing_refresh_token",
            message: "Saved session expired and has no refresh mechanism. Please log in again."
          }
        };
      }
      return {
        session: this.getSession(),
        refresh: {
          attempted: true,
          forced: forceRefresh,
          outcome: "missing_refresh_token",
          message: "No refresh token available. Using saved session token."
        }
      };
    }
    if (expired) {
      await this.logout();
      return {
        session: null,
        refresh: {
          attempted: true,
          forced: forceRefresh,
          outcome: "failed",
          message: "Token refresh failed and the saved session expired. Please log in again.",
          error: errorText
        }
      };
    }
    return {
      session: this.getSession(),
      refresh: {
        attempted: true,
        forced: forceRefresh,
        outcome: "failed",
        message: "Token refresh failed. Using saved session token.",
        error: errorText
      }
    };
  }
  async ensureValidSession() {
    const result = await this.ensureValidSessionWithStatus(false);
    return result.session;
  }
  async resolveJwtToken(explicitToken) {
    if (this.getSession()) {
      const session22 = await this.ensureValidSession();
      if (!session22) {
        throw new Error("No authenticated session available");
      }
      return session22.tokens.idToken ?? session22.tokens.accessToken;
    }
    if (explicitToken && explicitToken.trim()) {
      return explicitToken.trim();
    }
    const session2 = await this.ensureValidSession();
    if (!session2) {
      throw new Error("No authenticated session available");
    }
    return session2.tokens.idToken ?? session2.tokens.accessToken;
  }
}
const DISCORD_CLIENT_ID = "1479944467112001669";
let rpcClient = null;
let connected = false;
let lastActivity = null;
let pendingActivity = null;
async function connectDiscordRpc() {
  if (rpcClient) return;
  const client = new Client({ transport: "ipc" });
  client.on("disconnected", () => {
    connected = false;
    rpcClient = null;
    console.log("[DiscordRPC] Disconnected.");
  });
  try {
    await client.login({ clientId: DISCORD_CLIENT_ID });
    rpcClient = client;
    connected = true;
    console.log("[DiscordRPC] Connected.");
    if (pendingActivity) {
      await setActivity(pendingActivity.gameName, pendingActivity.startTimestamp, pendingActivity.appId);
      pendingActivity = null;
    } else if (lastActivity) {
      await setActivity(lastActivity.gameName, lastActivity.startTimestamp, lastActivity.appId);
    } else {
      await client.clearActivity().catch(() => {
      });
    }
  } catch (err) {
    console.warn("[DiscordRPC] Failed to connect (Discord may not be running):", err.message);
    rpcClient = null;
    connected = false;
  }
}
function getCurrentActivity() {
  return lastActivity;
}
function isDiscordRpcConnected() {
  return connected && rpcClient !== null;
}
async function setActivity(gameName, startTimestamp, appId) {
  pendingActivity = { gameName, startTimestamp, appId };
  if (!connected || !rpcClient) {
    return;
  }
  try {
    await rpcClient.setActivity({
      details: gameName,
      state: "Streaming via OpenNow",
      startTimestamp,
      instance: false
    });
    lastActivity = pendingActivity;
    pendingActivity = null;
  } catch (err) {
    pendingActivity = null;
    console.warn("[DiscordRPC] setActivity failed:", err.message);
  }
}
async function clearActivity() {
  lastActivity = null;
  pendingActivity = null;
  if (!connected || !rpcClient) return;
  try {
    await rpcClient.clearActivity();
  } catch (err) {
    console.warn("[DiscordRPC] clearActivity failed:", err.message);
  }
}
async function destroyDiscordRpc() {
  lastActivity = null;
  pendingActivity = null;
  if (!rpcClient) return;
  try {
    await rpcClient.destroy();
  } catch {
  } finally {
    rpcClient = null;
    connected = false;
  }
}
function normalizeMetadataValue(value) {
  const trimmed = value?.trim();
  return trimmed ? trimmed : void 0;
}
function getEmbeddedBuildNumber() {
  return normalizeMetadataValue(
    ""
  );
}
function getEmbeddedCommit() {
  return normalizeMetadataValue(
    ""
  );
}
function getAppBuildInfo() {
  const version = app.getVersion();
  const buildNumber = getEmbeddedBuildNumber();
  const commit = getEmbeddedCommit();
  return {
    version,
    displayVersion: buildNumber ? `${version} (build ${buildNumber})` : version,
    buildNumber,
    commit
  };
}
const { autoUpdater } = electronUpdater;
const STARTUP_CHECK_DELAY_MS = 12e3;
const PERIODIC_CHECK_INTERVAL_MS = 6 * 60 * 60 * 1e3;
const UPDATER_TOKEN_ENV_KEYS = ["OPENNOW_GH_TOKEN", "GH_TOKEN"];
function isPrereleaseVersion(version) {
  return version.includes("-");
}
function pickRuntimeToken() {
  for (const key of UPDATER_TOKEN_ENV_KEYS) {
    const value = process.env[key]?.trim();
    if (value) {
      return value;
    }
  }
  return null;
}
function normalizeErrorMessage(error) {
  if (!(error instanceof Error)) {
    return "Update check failed.";
  }
  const message = error.message.trim();
  if (/sha(2|512)|checksum|signature/i.test(message)) {
    return "Downloaded update failed verification.";
  }
  if (/latest.*yml|app-update\.yml|Cannot find channel/i.test(message)) {
    return "Update metadata is unavailable for this release.";
  }
  if (/404/.test(message)) {
    return "No published update metadata was found on GitHub Releases.";
  }
  if (/403/.test(message)) {
    return "GitHub Releases rejected the update request.";
  }
  if (/net::|network|ENOTFOUND|ECONN|ETIMEDOUT|EAI_AGAIN|offline/i.test(message)) {
    return "Unable to reach GitHub Releases right now.";
  }
  return message || "Update check failed.";
}
function getUpdateVersion(info) {
  return info?.version;
}
function createDisabledState(buildInfo, message) {
  return {
    status: "disabled",
    currentVersion: buildInfo.version,
    currentDisplayVersion: buildInfo.displayVersion,
    currentBuildNumber: buildInfo.buildNumber,
    updateSource: "github-releases",
    message,
    canCheck: false,
    canDownload: false,
    canInstall: false,
    isPackaged: app.isPackaged
  };
}
function createAppUpdaterController(options) {
  const buildInfo = getAppBuildInfo();
  const currentVersion = buildInfo.version;
  if (!app.isPackaged) {
    const disabledState = createDisabledState(buildInfo, "Auto-updates are only available in packaged builds.");
    return {
      initialize() {
        options.onStateChanged(disabledState);
      },
      dispose() {
      },
      getState() {
        return disabledState;
      },
      setAutomaticChecksEnabled() {
        return disabledState;
      },
      async checkForUpdates() {
        return disabledState;
      },
      async downloadUpdate() {
        return disabledState;
      },
      async quitAndInstall() {
        return disabledState;
      }
    };
  }
  const updater = autoUpdater;
  const token = pickRuntimeToken();
  if (token) {
    updater.requestHeaders = {
      ...updater.requestHeaders,
      Authorization: `token ${token}`
    };
  }
  updater.autoDownload = false;
  updater.autoInstallOnAppQuit = false;
  updater.autoRunAppAfterInstall = true;
  updater.allowPrerelease = isPrereleaseVersion(currentVersion);
  updater.allowDowngrade = false;
  updater.fullChangelog = false;
  let disposed = false;
  let startupTimer = null;
  let intervalTimer = null;
  let checkInFlight = false;
  let downloadInFlight = false;
  let automaticChecksEnabled = options.automaticChecksEnabled;
  let availableUpdateInfo = null;
  let downloadedUpdateInfo = null;
  const baseState = {
    currentVersion,
    currentDisplayVersion: buildInfo.displayVersion,
    currentBuildNumber: buildInfo.buildNumber,
    updateSource: "github-releases",
    isPackaged: true
  };
  let state = {
    ...baseState,
    status: "idle",
    canCheck: true,
    canDownload: false,
    canInstall: false
  };
  const emitState = () => {
    if (!disposed) {
      options.onStateChanged(state);
    }
  };
  const recomputeActionFlags = (nextState) => ({
    ...nextState,
    canCheck: !checkInFlight && !downloadInFlight,
    canDownload: !checkInFlight && !downloadInFlight && nextState.status === "available" && Boolean(availableUpdateInfo),
    canInstall: nextState.status === "downloaded"
  });
  const updateState = (patch) => {
    state = recomputeActionFlags({
      ...state,
      ...patch,
      ...baseState
    });
    emitState();
  };
  const clearAutomaticCheckTimers = () => {
    if (startupTimer) {
      clearTimeout(startupTimer);
      startupTimer = null;
    }
    if (intervalTimer) {
      clearInterval(intervalTimer);
      intervalTimer = null;
    }
  };
  const scheduleAutomaticChecks = () => {
    clearAutomaticCheckTimers();
    if (disposed || !automaticChecksEnabled) {
      return;
    }
    startupTimer = setTimeout(() => {
      void controller.checkForUpdates("auto");
    }, STARTUP_CHECK_DELAY_MS);
    startupTimer.unref?.();
    intervalTimer = setInterval(() => {
      void controller.checkForUpdates("auto");
    }, PERIODIC_CHECK_INTERVAL_MS);
    intervalTimer.unref?.();
  };
  updater.on("checking-for-update", () => {
    updateState({
      status: "checking",
      message: "Checking GitHub Releases for updates…",
      errorCode: void 0
    });
  });
  updater.on("update-available", (info) => {
    availableUpdateInfo = info;
    downloadedUpdateInfo = null;
    updateState({
      status: "available",
      availableVersion: info.version,
      downloadedVersion: void 0,
      progress: void 0,
      lastCheckedAt: Date.now(),
      message: `OpenNOW ${info.version} is available. Download when ready.`
    });
  });
  updater.on("update-not-available", () => {
    availableUpdateInfo = null;
    downloadedUpdateInfo = null;
    updateState({
      status: "not-available",
      availableVersion: void 0,
      downloadedVersion: void 0,
      progress: void 0,
      lastCheckedAt: Date.now(),
      message: "OpenNOW is up to date."
    });
  });
  updater.on("download-progress", (progress) => {
    updateState({
      status: "downloading",
      availableVersion: availableUpdateInfo?.version,
      progress: {
        percent: progress.percent,
        transferred: progress.transferred,
        total: progress.total,
        bytesPerSecond: progress.bytesPerSecond
      },
      message: `Downloading OpenNOW ${availableUpdateInfo?.version ?? "update"}…`
    });
  });
  updater.on("update-downloaded", (info) => {
    downloadedUpdateInfo = info;
    const downloadedVersion = getUpdateVersion(info) ?? availableUpdateInfo?.version;
    updateState({
      status: "downloaded",
      availableVersion: downloadedVersion,
      downloadedVersion,
      progress: void 0,
      message: `OpenNOW ${downloadedVersion ?? "update"} is ready to install. Restart when convenient.`
    });
  });
  updater.on("error", (error) => {
    checkInFlight = false;
    downloadInFlight = false;
    updateState({
      status: "error",
      availableVersion: availableUpdateInfo?.version,
      progress: void 0,
      message: normalizeErrorMessage(error),
      errorCode: error.code
    });
  });
  const controller = {
    initialize() {
      emitState();
      scheduleAutomaticChecks();
    },
    dispose() {
      disposed = true;
      clearAutomaticCheckTimers();
      updater.removeAllListeners("checking-for-update");
      updater.removeAllListeners("update-available");
      updater.removeAllListeners("update-not-available");
      updater.removeAllListeners("download-progress");
      updater.removeAllListeners("update-downloaded");
      updater.removeAllListeners("error");
    },
    getState() {
      return state;
    },
    setAutomaticChecksEnabled(enabled) {
      automaticChecksEnabled = enabled;
      scheduleAutomaticChecks();
      return state;
    },
    async checkForUpdates(source = "manual") {
      if (disposed || checkInFlight || downloadInFlight) {
        return state;
      }
      if (source === "auto" && !automaticChecksEnabled) {
        return state;
      }
      checkInFlight = true;
      updateState({
        status: "checking",
        message: source === "auto" ? "Checking for updates in the background…" : "Checking GitHub Releases for updates…",
        errorCode: void 0
      });
      try {
        await updater.checkForUpdates();
      } catch (error) {
        updateState({
          status: "error",
          availableVersion: availableUpdateInfo?.version,
          progress: void 0,
          lastCheckedAt: Date.now(),
          message: normalizeErrorMessage(error),
          errorCode: error instanceof Error ? error.code : void 0
        });
      } finally {
        checkInFlight = false;
        updateState({});
      }
      return state;
    },
    async downloadUpdate() {
      if (disposed || checkInFlight || downloadInFlight || !availableUpdateInfo) {
        return state;
      }
      downloadInFlight = true;
      updateState({
        status: "downloading",
        availableVersion: availableUpdateInfo.version,
        progress: {
          percent: 0,
          transferred: 0,
          total: 0,
          bytesPerSecond: 0
        },
        message: `Downloading OpenNOW ${availableUpdateInfo.version}…`
      });
      try {
        await updater.downloadUpdate();
      } catch (error) {
        updateState({
          status: "error",
          availableVersion: availableUpdateInfo.version,
          progress: void 0,
          message: normalizeErrorMessage(error),
          errorCode: error instanceof Error ? error.code : void 0
        });
      } finally {
        downloadInFlight = false;
        updateState({});
      }
      return state;
    },
    async quitAndInstall() {
      if (disposed || !downloadedUpdateInfo) {
        return state;
      }
      updateState({
        status: "downloaded",
        message: `Restarting to install OpenNOW ${downloadedUpdateInfo.version}…`
      });
      setImmediate(() => {
        try {
          options.onBeforeQuitAndInstall?.();
          updater.quitAndInstall(false, true);
        } catch (error) {
          options.onQuitAndInstallError?.();
          updateState({
            status: "error",
            message: normalizeErrorMessage(error)
          });
        }
      });
      return state;
    }
  };
  return controller;
}
function registerAccountCatalogIpcHandlers(deps) {
  const { ipcMain: ipcMain2, authService: authService2, refreshScheduler: refreshScheduler2, resolveJwt: resolveJwt2 } = deps;
  ipcMain2.handle(
    IPC_CHANNELS.AUTH_GET_SESSION,
    async (_event, payload = {}) => {
      return authService2.ensureValidSessionWithStatus(
        Boolean(payload.forceRefresh)
      );
    }
  );
  ipcMain2.handle(IPC_CHANNELS.AUTH_GET_PROVIDERS, async () => {
    return authService2.getProviders();
  });
  ipcMain2.handle(
    IPC_CHANNELS.AUTH_GET_REGIONS,
    async (_event, payload) => {
      return authService2.getRegions(payload?.token);
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.AUTH_LOGIN,
    async (_event, payload) => {
      return authService2.login(payload);
    }
  );
  ipcMain2.handle(IPC_CHANNELS.AUTH_LOGOUT, async () => {
    await authService2.logout();
  });
  ipcMain2.handle(IPC_CHANNELS.AUTH_LOGOUT_ALL, async () => {
    await authService2.logoutAll();
  });
  ipcMain2.handle(IPC_CHANNELS.AUTH_GET_SAVED_ACCOUNTS, async () => {
    return authService2.getSavedAccounts();
  });
  ipcMain2.handle(
    IPC_CHANNELS.AUTH_SWITCH_ACCOUNT,
    async (_event, userId) => {
      return authService2.switchAccount(userId);
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.AUTH_REMOVE_ACCOUNT,
    async (_event, userId) => {
      await authService2.removeAccount(userId);
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.SUBSCRIPTION_FETCH,
    async (_event, payload) => {
      const token = await resolveJwt2(payload?.token);
      const streamingBaseUrl = payload?.providerStreamingBaseUrl ?? authService2.getSelectedProvider().streamingServiceUrl;
      const userId = payload.userId;
      const { vpcId } = await fetchDynamicRegions(token, streamingBaseUrl);
      return fetchSubscription(token, userId, vpcId ?? void 0);
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.GAMES_FETCH_MAIN,
    async (_event, payload) => {
      const token = await resolveJwt2(payload?.token);
      const streamingBaseUrl = payload?.providerStreamingBaseUrl ?? authService2.getSelectedProvider().streamingServiceUrl;
      refreshScheduler2.updateAuthContext(token, streamingBaseUrl);
      return fetchMainGames(token, streamingBaseUrl);
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.GAMES_FETCH_LIBRARY,
    async (_event, payload) => {
      const token = await resolveJwt2(payload?.token);
      const streamingBaseUrl = payload?.providerStreamingBaseUrl ?? authService2.getSelectedProvider().streamingServiceUrl;
      refreshScheduler2.updateAuthContext(token, streamingBaseUrl);
      return fetchLibraryGames(token, streamingBaseUrl);
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.GAMES_BROWSE_CATALOG,
    async (_event, payload) => {
      const token = await resolveJwt2(payload?.token);
      const streamingBaseUrl = payload?.providerStreamingBaseUrl ?? authService2.getSelectedProvider().streamingServiceUrl;
      refreshScheduler2.updateAuthContext(token, streamingBaseUrl);
      return browseCatalog({
        ...payload,
        token,
        providerStreamingBaseUrl: streamingBaseUrl
      });
    }
  );
  ipcMain2.handle(IPC_CHANNELS.GAMES_FETCH_PUBLIC, async () => {
    return fetchPublicGames();
  });
  ipcMain2.handle(
    IPC_CHANNELS.GAMES_RESOLVE_LAUNCH_ID,
    async (_event, payload) => {
      const token = await resolveJwt2(payload?.token);
      const streamingBaseUrl = payload?.providerStreamingBaseUrl ?? authService2.getSelectedProvider().streamingServiceUrl;
      return resolveLaunchAppId(token, payload.appIdOrUuid, streamingBaseUrl);
    }
  );
}
function sanitizeTitleForFileName(value) {
  const source = (value ?? "").trim().toLowerCase();
  const compact = source.replace(/[^a-z0-9]+/g, "-").replace(/^-+|-+$/g, "");
  if (!compact) return "stream";
  return compact.slice(0, 48);
}
function dataUrlToBuffer(dataUrl) {
  const match = /^data:image\/(png|jpeg|jpg|webp);base64,([a-z0-9+/=\s]+)$/i.exec(dataUrl);
  if (!match || !match[1] || !match[2]) {
    throw new Error("Invalid screenshot payload");
  }
  const rawExt = match[1].toLowerCase();
  const ext = rawExt === "jpeg" ? "jpg" : rawExt;
  const buffer = Buffer$1.from(match[2].replace(/\s+/g, ""), "base64");
  if (!buffer.length) {
    throw new Error("Empty screenshot payload");
  }
  return { ext, buffer };
}
function buildImageDataUrl(ext, buffer) {
  const mime = ext === "jpg" || ext === "jpeg" ? "image/jpeg" : ext === "webp" ? "image/webp" : "image/png";
  return `data:${mime};base64,${buffer.toString("base64")}`;
}
function assertSafeMediaId(id, label) {
  if (!id || id.includes("/") || id.includes("\\") || id.includes("..")) {
    throw new Error(`Invalid ${label} id`);
  }
}
const SCREENSHOT_LIMIT = 60;
function getScreenshotDirectory() {
  return join(app.getPath("pictures"), "OpenNOW", "Screenshots");
}
async function ensureScreenshotDirectory() {
  const dir = getScreenshotDirectory();
  await mkdir(dir, { recursive: true });
  return dir;
}
function buildScreenshotDataUrl(ext, buffer) {
  const mime = ext === "jpg" ? "image/jpeg" : ext === "webp" ? "image/webp" : "image/png";
  return `data:${mime};base64,${buffer.toString("base64")}`;
}
function assertSafeScreenshotId(id) {
  assertSafeMediaId(id, "screenshot");
}
async function listScreenshots() {
  const dir = await ensureScreenshotDirectory();
  const entries = await readdir(dir, { withFileTypes: true });
  const screenshotFiles = entries.filter((entry) => entry.isFile()).map((entry) => entry.name).filter((name) => /\.(png|jpg|jpeg|webp)$/i.test(name));
  const loaded = await Promise.all(
    screenshotFiles.map(async (fileName) => {
      const filePath = join(dir, fileName);
      try {
        const fileStats = await stat(filePath);
        const fileBuffer = await readFile(filePath);
        const extMatch = /\.([^.]+)$/.exec(fileName);
        const ext = (extMatch?.[1] ?? "png").toLowerCase();
        return {
          id: fileName,
          fileName,
          filePath,
          createdAtMs: fileStats.birthtimeMs || fileStats.mtimeMs,
          sizeBytes: fileStats.size,
          dataUrl: buildScreenshotDataUrl(ext, fileBuffer)
        };
      } catch {
        return null;
      }
    })
  );
  return loaded.filter((item) => item !== null).sort((a, b) => b.createdAtMs - a.createdAtMs).slice(0, SCREENSHOT_LIMIT);
}
async function saveScreenshot(input) {
  const { ext, buffer } = dataUrlToBuffer(input.dataUrl);
  const dir = await ensureScreenshotDirectory();
  const stamp = (/* @__PURE__ */ new Date()).toISOString().replace(/[:.]/g, "-");
  const title = sanitizeTitleForFileName(input.gameTitle);
  const fileName = `${stamp}-${title}-${Math.random().toString(16).slice(2, 8)}.${ext}`;
  const filePath = join(dir, fileName);
  await writeFile(filePath, buffer);
  return {
    id: fileName,
    fileName,
    filePath,
    createdAtMs: Date.now(),
    sizeBytes: buffer.byteLength,
    dataUrl: buildScreenshotDataUrl(ext, buffer)
  };
}
async function deleteScreenshot(input) {
  assertSafeScreenshotId(input.id);
  const dir = await ensureScreenshotDirectory();
  const filePath = join(dir, input.id);
  await unlink(filePath);
}
async function saveScreenshotAs(input, deps) {
  assertSafeScreenshotId(input.id);
  const dir = await ensureScreenshotDirectory();
  const sourcePath = join(dir, input.id);
  const saveDialogOptions = {
    title: "Save Screenshot",
    defaultPath: join(app.getPath("pictures"), input.id),
    filters: [
      { name: "PNG Image", extensions: ["png"] },
      { name: "JPEG Image", extensions: ["jpg", "jpeg"] },
      { name: "WebP Image", extensions: ["webp"] },
      { name: "All Files", extensions: ["*"] }
    ]
  };
  const mainWindow2 = deps.getMainWindow();
  const target = mainWindow2 && !mainWindow2.isDestroyed() ? await deps.dialog.showSaveDialog(mainWindow2, saveDialogOptions) : await deps.dialog.showSaveDialog(saveDialogOptions);
  if (target.canceled || !target.filePath) {
    return { saved: false };
  }
  await copyFile(sourcePath, target.filePath);
  return { saved: true, filePath: target.filePath };
}
const RECORDING_LIMIT = 20;
const activeRecordings = /* @__PURE__ */ new Map();
function getRecordingsDirectory() {
  return join(app.getPath("pictures"), "OpenNOW", "Recordings");
}
async function ensureRecordingsDirectory() {
  const dir = getRecordingsDirectory();
  await mkdir(dir, { recursive: true });
  return dir;
}
function assertSafeRecordingId(id) {
  assertSafeMediaId(id, "recording");
}
function extFromMimeType(mimeType) {
  return mimeType.startsWith("video/mp4") ? ".mp4" : ".webm";
}
async function listRecordings() {
  const dir = await ensureRecordingsDirectory();
  const entries = await readdir(dir, { withFileTypes: true });
  const webmFiles = entries.filter((e) => e.isFile()).map((e) => e.name).filter((name) => /\.(mp4|webm)$/i.test(name));
  const loaded = await Promise.all(
    webmFiles.map(async (fileName) => {
      const filePath = join(dir, fileName);
      try {
        const fileStats = await stat(filePath);
        const stem = fileName.replace(/\.(mp4|webm)$/i, "");
        const thumbName = `${stem}-thumb.jpg`;
        const thumbPath = join(dir, thumbName);
        let thumbnailDataUrl;
        try {
          const thumbBuf = await readFile(thumbPath);
          thumbnailDataUrl = `data:image/jpeg;base64,${thumbBuf.toString("base64")}`;
        } catch {
        }
        const durMatch = /-dur(\d+)\.(mp4|webm)$/i.exec(fileName);
        const durationMs = durMatch ? Number(durMatch[1]) : 0;
        const titleMatch = /^[^-]+-[^-]+-([^-]+(?:-[^-]+)*?)-[a-f0-9]{6}(?:-dur\d+)?\.(mp4|webm)$/i.exec(
          fileName
        );
        const gameTitle = titleMatch ? titleMatch[1].replace(/-/g, " ") : void 0;
        return {
          id: fileName,
          fileName,
          filePath,
          createdAtMs: fileStats.birthtimeMs || fileStats.mtimeMs,
          sizeBytes: fileStats.size,
          durationMs,
          gameTitle,
          thumbnailDataUrl
        };
      } catch {
        return null;
      }
    })
  );
  return loaded.filter((item) => item !== null).sort((a, b) => b.createdAtMs - a.createdAtMs).slice(0, RECORDING_LIMIT);
}
async function beginRecording(input) {
  const dir = await ensureRecordingsDirectory();
  const recordingId = randomUUID();
  const ext = extFromMimeType(input.mimeType);
  const tempPath = join(dir, `${recordingId}${ext}.tmp`);
  const writeStream = createWriteStream(tempPath);
  activeRecordings.set(recordingId, {
    writeStream,
    tempPath,
    mimeType: input.mimeType
  });
  return { recordingId };
}
async function appendRecordingChunk(input) {
  const rec = activeRecordings.get(input.recordingId);
  if (!rec) {
    throw new Error("Unknown recording id");
  }
  await new Promise((resolve2, reject) => {
    rec.writeStream.write(Buffer$1.from(input.chunk), (err) => {
      if (err) reject(err);
      else resolve2();
    });
  });
}
async function finishRecording(input) {
  const rec = activeRecordings.get(input.recordingId);
  if (!rec) {
    throw new Error("Unknown recording id");
  }
  activeRecordings.delete(input.recordingId);
  await new Promise((resolve2, reject) => {
    rec.writeStream.end((err) => {
      if (err) reject(err);
      else resolve2();
    });
  });
  const dir = getRecordingsDirectory();
  const stamp = (/* @__PURE__ */ new Date()).toISOString().replace(/[:.]/g, "-");
  const title = sanitizeTitleForFileName(input.gameTitle);
  const rand = Math.random().toString(16).slice(2, 8);
  const durSuffix = input.durationMs > 0 ? `-dur${Math.round(input.durationMs)}` : "";
  const ext = extFromMimeType(rec.mimeType);
  const fileName = `${stamp}-${title}-${rand}${durSuffix}${ext}`;
  const finalPath = join(dir, fileName);
  await rename(rec.tempPath, finalPath);
  let thumbnailDataUrl;
  if (input.thumbnailDataUrl) {
    try {
      const { buffer } = dataUrlToBuffer(input.thumbnailDataUrl);
      const stem = fileName.replace(/\.(mp4|webm)$/i, "");
      const thumbPath = join(dir, `${stem}-thumb.jpg`);
      await writeFile(thumbPath, buffer);
      thumbnailDataUrl = input.thumbnailDataUrl;
    } catch {
    }
  }
  const all = await listRecordings();
  if (all.length > RECORDING_LIMIT) {
    const toDelete = all.slice(RECORDING_LIMIT);
    await Promise.all(
      toDelete.map(async (entry) => {
        await unlink(entry.filePath).catch(() => void 0);
        const stem = entry.fileName.replace(/\.(mp4|webm)$/i, "");
        await unlink(join(dir, `${stem}-thumb.jpg`)).catch(() => void 0);
      })
    );
  }
  const fileStats = await stat(finalPath);
  return {
    id: fileName,
    fileName,
    filePath: finalPath,
    createdAtMs: Date.now(),
    sizeBytes: fileStats.size,
    durationMs: input.durationMs,
    gameTitle: input.gameTitle,
    thumbnailDataUrl
  };
}
async function abortRecording(input) {
  const rec = activeRecordings.get(input.recordingId);
  if (!rec) {
    return;
  }
  activeRecordings.delete(input.recordingId);
  rec.writeStream.destroy();
  await unlink(rec.tempPath).catch(() => void 0);
}
async function deleteRecording(input) {
  assertSafeRecordingId(input.id);
  const dir = await ensureRecordingsDirectory();
  const filePath = join(dir, input.id);
  await unlink(filePath);
  const stem = input.id.replace(/\.(mp4|webm)$/i, "");
  await unlink(join(dir, `${stem}-thumb.jpg`)).catch(() => void 0);
}
async function getRecordingFilePath(id) {
  assertSafeRecordingId(id);
  const dir = await ensureRecordingsDirectory();
  return join(dir, id);
}
function getThumbnailCacheDirectory() {
  return join(app.getPath("userData"), "media-thumbs");
}
async function ensureThumbnailCacheDirectory() {
  const dir = getThumbnailCacheDirectory();
  await mkdir(dir, { recursive: true });
  return dir;
}
function md5(input) {
  return createHash("md5").update(input).digest("hex");
}
async function probeVideoDurationSeconds(sourcePath) {
  return new Promise((resolve2) => {
    const args = [
      "-v",
      "error",
      "-show_entries",
      "format=duration",
      "-of",
      "default=noprint_wrappers=1:nokey=1",
      sourcePath
    ];
    const child = spawn("ffprobe", args, {
      stdio: ["ignore", "pipe", "ignore"]
    });
    let out = "";
    child.stdout?.on("data", (chunk) => {
      out += chunk.toString();
    });
    child.on("error", () => resolve2(null));
    child.on("close", (code) => {
      if (code !== 0) {
        resolve2(null);
        return;
      }
      const n = Number.parseFloat(out.trim());
      resolve2(Number.isFinite(n) && n > 0 ? n : null);
    });
  });
}
function randomThumbnailSeekSeconds(durationSec) {
  if (durationSec !== null && durationSec > 0.2) {
    const margin = Math.min(0.35, durationSec * 0.08);
    const hi = Math.max(durationSec - margin, margin + 0.05);
    const lo = Math.min(margin, hi * 0.5);
    return lo + Math.random() * (hi - lo);
  }
  return 0.2 + Math.random() * 4.8;
}
function ffmpegExtractOneFrame(sourcePath, outPath, seekSec) {
  const ss = seekSec.toFixed(3);
  return new Promise((resolve2) => {
    const args = [
      "-y",
      "-ss",
      ss,
      "-i",
      sourcePath,
      "-frames:v",
      "1",
      "-q:v",
      "2",
      outPath
    ];
    const child = spawn("ffmpeg", args, { stdio: "ignore" });
    child.on("error", () => resolve2(false));
    child.on("close", (code) => {
      resolve2(code === 0);
    });
  });
}
async function generateVideoThumbnail(sourcePath, outPath) {
  const durationSec = await probeVideoDurationSeconds(sourcePath);
  const seekSec = randomThumbnailSeekSeconds(durationSec);
  if (await ffmpegExtractOneFrame(sourcePath, outPath, seekSec)) return true;
  if (seekSec > 0.02) return ffmpegExtractOneFrame(sourcePath, outPath, 0);
  return false;
}
async function ensureThumbnailForMedia(filePath) {
  try {
    const stats = await stat(filePath);
    const key = md5(`${filePath}|${stats.mtimeMs}`);
    const cacheDir = await ensureThumbnailCacheDirectory();
    const outPath = join(cacheDir, `${key}.jpg`);
    try {
      await stat(outPath);
      return outPath;
    } catch {
    }
    const lower = filePath.toLowerCase();
    if (isVideoMediaFilePath(lower)) {
      const ok = await generateVideoThumbnail(filePath, outPath);
      if (ok) return outPath;
      return null;
    }
    if (isImageMediaFilePath(lower)) {
      try {
        const buf = await readFile(filePath);
        await writeFile(outPath, buf);
        return outPath;
      } catch {
        return null;
      }
    }
    return null;
  } catch (err) {
    console.warn("ensureThumbnailForMedia error:", err);
    return null;
  }
}
function isImageMediaFilePath(filePath) {
  const lower = filePath.toLowerCase();
  return lower.endsWith(".png") || lower.endsWith(".jpg") || lower.endsWith(".jpeg") || lower.endsWith(".webp");
}
function isVideoMediaFilePath(filePath) {
  const lower = filePath.toLowerCase();
  return lower.endsWith(".mp4") || lower.endsWith(".webm") || lower.endsWith(".mkv") || lower.endsWith(".mov");
}
async function readThumbnailDataUrlForTrustedPath(filePath) {
  const lower = filePath.toLowerCase();
  if (isImageMediaFilePath(lower)) {
    const buf = await readFile(filePath);
    const extMatch = /\.([^.]+)$/.exec(filePath);
    const ext = (extMatch?.[1] || "png").toLowerCase();
    return buildImageDataUrl(ext, buf);
  }
  if (isVideoMediaFilePath(lower)) {
    const stem = filePath.replace(/\.(mp4|webm|mkv|mov)$/i, "");
    const thumbPath = `${stem}-thumb.jpg`;
    try {
      const b = await readFile(thumbPath);
      return `data:image/jpeg;base64,${b.toString("base64")}`;
    } catch {
    }
    const gen = await ensureThumbnailForMedia(filePath);
    if (gen) {
      try {
        const b2 = await readFile(gen);
        return `data:image/jpeg;base64,${b2.toString("base64")}`;
      } catch {
        return null;
      }
    }
    return null;
  }
  return null;
}
async function deleteThumbnailArtifactsForTrustedPath(filePath) {
  let st;
  try {
    st = await stat(filePath);
  } catch {
    return false;
  }
  const key = md5(`${filePath}|${st.mtimeMs}`);
  const cacheDir = await ensureThumbnailCacheDirectory();
  await unlink(join(cacheDir, `${key}.jpg`)).catch(() => void 0);
  const stem = filePath.replace(
    /\.(mp4|webm|mkv|mov|png|jpg|jpeg|webp)$/i,
    ""
  );
  await unlink(`${stem}-thumb.jpg`).catch(() => void 0);
  return true;
}
async function regenerateThumbnailForTrustedPath(filePath) {
  const st = await stat(filePath);
  const key = md5(`${filePath}|${st.mtimeMs}`);
  const cacheDir = await ensureThumbnailCacheDirectory();
  await unlink(join(cacheDir, `${key}.jpg`)).catch(() => void 0);
  if (/\.(mp4|webm|mkv|mov)$/i.test(filePath)) {
    const videoStem = filePath.replace(/\.(mp4|webm|mkv|mov)$/i, "");
    await unlink(`${videoStem}-thumb.jpg`).catch(() => void 0);
  }
  const genPath = await ensureThumbnailForMedia(filePath);
  if (!genPath) return { ok: false, thumbnailDataUrl: null };
  if (/\.(mp4|webm|mkv|mov)$/i.test(filePath)) {
    const videoStem = filePath.replace(/\.(mp4|webm|mkv|mov)$/i, "");
    const sidecar = `${videoStem}-thumb.jpg`;
    await copyFile(genPath, sidecar).catch((err) => {
      console.warn("MEDIA_REGEN_THUMBNAIL sidecar copy:", err);
    });
  }
  const b = await readFile(genPath);
  return {
    ok: true,
    thumbnailDataUrl: `data:image/jpeg;base64,${b.toString("base64")}`
  };
}
function registerMediaIpcHandlers(deps) {
  deps.ipcMain.handle(
    IPC_CHANNELS.SCREENSHOT_SAVE,
    async (_event, input) => {
      return saveScreenshot(input);
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.SCREENSHOT_LIST,
    async () => {
      return listScreenshots();
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_LIST_BY_GAME,
    async (_event, payload = {}) => {
      const title = (payload?.gameTitle || "").trim().toLowerCase();
      const screenshots = await listScreenshots();
      const recordings = await listRecordings();
      const normalize = (s) => (s || "").replace(/[^a-z0-9]+/gi, "").toLowerCase();
      const needle = normalize(title);
      const matchedScreens = screenshots.filter((s) => {
        if (!needle) return true;
        const candidate = normalize(s.fileName) + normalize(s.filePath || "");
        return candidate.includes(needle);
      });
      const matchedRecordings = recordings.filter((r) => {
        if (!needle) return true;
        const candidate = normalize(r.gameTitle ?? r.fileName ?? "");
        return candidate.includes(needle);
      });
      return {
        screenshots: matchedScreens,
        videos: matchedRecordings
      };
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.SCREENSHOT_DELETE,
    async (_event, input) => {
      return deleteScreenshot(input);
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.SCREENSHOT_SAVE_AS,
    async (_event, input) => {
      return saveScreenshotAs(input, deps);
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_BEGIN,
    async (_event, input) => {
      return beginRecording(input);
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_CHUNK,
    async (_event, input) => {
      return appendRecordingChunk(input);
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_FINISH,
    async (_event, input) => {
      return finishRecording(input);
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_ABORT,
    async (_event, input) => {
      return abortRecording(input);
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_LIST,
    async () => {
      return listRecordings();
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_DELETE,
    async (_event, input) => {
      return deleteRecording(input);
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.RECORDING_SHOW_IN_FOLDER,
    async (_event, id) => {
      deps.shell.showItemInFolder(await getRecordingFilePath(id));
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_THUMBNAIL,
    async (_event, payload) => {
      const rawFp = payload?.filePath;
      if (typeof rawFp !== "string") return null;
      try {
        const fpReal = await resolveTrustedOpenNowMediaPath(rawFp);
        if (!fpReal) return null;
        return readThumbnailDataUrlForTrustedPath(fpReal);
      } catch (err) {
        console.warn("MEDIA_THUMBNAIL error:", err);
        return null;
      }
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_SHOW_IN_FOLDER,
    async (_event, payload) => {
      const rawFp = payload?.filePath;
      if (typeof rawFp !== "string") return;
      try {
        const fpReal = await resolveTrustedOpenNowMediaPath(rawFp);
        if (!fpReal) return;
        deps.shell.showItemInFolder(fpReal);
      } catch {
        return;
      }
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_PLAYBACK_URL,
    async (_event, payload) => {
      const rawFp = payload?.filePath;
      if (typeof rawFp !== "string") return null;
      try {
        return await getTrustedVideoPlaybackFileUrl(rawFp);
      } catch (err) {
        console.warn("MEDIA_PLAYBACK_URL error:", err);
        return null;
      }
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_DELETE_FILE,
    async (_event, payload) => {
      const rawFp = payload?.filePath;
      if (typeof rawFp !== "string") return { ok: false };
      try {
        const fpReal = await resolveTrustedOpenNowMediaPath(rawFp);
        if (!fpReal) return { ok: false };
        const mediaFileExists = await deleteThumbnailArtifactsForTrustedPath(fpReal);
        if (!mediaFileExists) return { ok: false };
        await unlink(fpReal);
        return { ok: true };
      } catch (err) {
        console.warn("MEDIA_DELETE_FILE error:", err);
        return { ok: false };
      }
    }
  );
  deps.ipcMain.handle(
    IPC_CHANNELS.MEDIA_REGEN_THUMBNAIL,
    async (_event, payload) => {
      const rawFp = payload?.filePath;
      if (typeof rawFp !== "string")
        return { ok: false, thumbnailDataUrl: null };
      try {
        const fpReal = await resolveTrustedOpenNowMediaPath(rawFp);
        if (!fpReal) return { ok: false, thumbnailDataUrl: null };
        return regenerateThumbnailForTrustedPath(fpReal);
      } catch (err) {
        console.warn("MEDIA_REGEN_THUMBNAIL error:", err);
        return { ok: false, thumbnailDataUrl: null };
      }
    }
  );
}
const MAX_CAUSE_DEPTH = 8;
function isRecord$1(value) {
  return typeof value === "object" && value !== null;
}
function errnoExtras(err) {
  const r = err;
  const parts = [];
  if (typeof r.code === "string" && r.code.length > 0) parts.push(`code=${r.code}`);
  if (typeof r.errno === "number" && Number.isFinite(r.errno)) parts.push(`errno=${r.errno}`);
  if (typeof r.syscall === "string" && r.syscall.length > 0) parts.push(`syscall=${r.syscall}`);
  if (typeof r.path === "string" && r.path.length > 0) parts.push(`path=${r.path}`);
  if (typeof r.address === "string" && r.address.length > 0) parts.push(`address=${r.address}`);
  if (typeof r.port === "number" && Number.isFinite(r.port)) parts.push(`port=${r.port}`);
  if (typeof r.hostname === "string" && r.hostname.length > 0) parts.push(`hostname=${r.hostname}`);
  return parts.length > 0 ? ` [${parts.join(", ")}]` : "";
}
function formatOneLevel(err) {
  const msg = err.message?.trim() ? err.message : "(no message)";
  return `${err.name}: ${msg}${errnoExtras(err)}`;
}
function nextCause(current) {
  if (!(current instanceof Error)) return void 0;
  const { cause } = current;
  if (cause === void 0 || cause === null) return void 0;
  if (cause instanceof Error) return cause;
  if (typeof cause === "string") return cause;
  if (typeof cause === "number" || typeof cause === "boolean" || typeof cause === "bigint") return String(cause);
  if (isRecord$1(cause) && typeof cause.message === "string") {
    return `${cause.name && typeof cause.name === "string" ? `${cause.name}: ` : ""}${cause.message}`;
  }
  try {
    return JSON.stringify(cause);
  } catch {
    return String(cause);
  }
}
function formatErrorChainForLog(error) {
  if (error === void 0) return "undefined";
  if (error === null) return "null";
  if (!(error instanceof Error)) {
    if (typeof error === "string") return error;
    try {
      return JSON.stringify(error);
    } catch {
      return String(error);
    }
  }
  const lines = [];
  const seen = /* @__PURE__ */ new WeakSet();
  let current = error;
  let depth = 0;
  while (depth < MAX_CAUSE_DEPTH && current != null) {
    if (current instanceof Error) {
      if (seen.has(current)) {
        lines.push(`${"  ".repeat(depth)}(cycle — ${current.name})`);
        break;
      }
      seen.add(current);
      const prefix = depth === 0 ? "" : `${"  ".repeat(depth)}caused by: `;
      lines.push(`${prefix}${formatOneLevel(current)}`);
      current = nextCause(current);
    } else if (typeof current === "string") {
      lines.push(`${"  ".repeat(depth)}caused by: ${current}`);
      break;
    } else {
      lines.push(`${"  ".repeat(depth)}caused by: ${String(current)}`);
      break;
    }
    depth++;
  }
  if (depth >= MAX_CAUSE_DEPTH && current instanceof Error) {
    lines.push(`${"  ".repeat(depth)}… (max depth ${MAX_CAUSE_DEPTH})`);
  }
  return lines.join("\n");
}
function chainToOneLine(chain) {
  return chain.replace(/\s*\n+\s*/g, " | ").replace(/\s+/g, " ").trim();
}
function enrichErrorForIpc(error) {
  if (!(error instanceof Error)) {
    return new Error(chainToOneLine(formatErrorChainForLog(error)));
  }
  if (!error.cause) {
    return error;
  }
  return new Error(chainToOneLine(formatErrorChainForLog(error)));
}
const SESSION_ERROR_TRANSPORT_KIND = "opennow.session-error";
const SESSION_ERROR_TRANSPORT_PREFIX = "__OPENNOW_SESSION_ERROR__:";
function toSerializedSessionError(info) {
  return {
    ...info,
    kind: SESSION_ERROR_TRANSPORT_KIND,
    name: "SessionError",
    message: info.description
  };
}
function serializeSessionErrorTransport(info) {
  return `${SESSION_ERROR_TRANSPORT_PREFIX}${JSON.stringify(toSerializedSessionError(info))}`;
}
async function showSessionConflictDialog$1(deps) {
  const mainWindow2 = deps.getMainWindow();
  if (!mainWindow2 || mainWindow2.isDestroyed()) {
    return "cancel";
  }
  const result = await deps.dialog.showMessageBox(mainWindow2, {
    type: "question",
    buttons: ["Resume", "Start New", "Cancel"],
    defaultId: 0,
    cancelId: 2,
    title: "Active Session Detected",
    message: "You have an active session running.",
    detail: "Resume it or start a new one?"
  });
  switch (result.response) {
    case 0:
      return "resume";
    case 1:
      return "new";
    default:
      return "cancel";
  }
}
function isSessionConflictError(error) {
  if (isSessionError(error)) {
    return error.isSessionConflict();
  }
  return false;
}
function rethrowSerializedSessionError(error) {
  if (error instanceof SessionError) {
    throw new Error(serializeSessionErrorTransport(error.toJSON()));
  }
  throw enrichErrorForIpc(error);
}
function assumedWindowsCapabilities(reason) {
  return {
    platformSupportsCloudGsync: true,
    isVrrCapableDisplay: true,
    isGsyncDisplay: true,
    minimumFpsForCloudGsync: DEFAULT_MINIMUM_FPS_FOR_CLOUD_GSYNC,
    minimumFpsForReflexWithoutVrr: DEFAULT_MINIMUM_FPS_FOR_REFLEX_WITHOUT_VRR,
    detectionSource: "assumed",
    reason
  };
}
function execFileText(file, args, timeoutMs) {
  return new Promise((resolve2, reject) => {
    execFile(file, args, { timeout: timeoutMs, windowsHide: true }, (error, stdout) => {
      if (error) {
        reject(error);
        return;
      }
      resolve2(stdout);
    });
  });
}
async function probeWindowsDisplayMetadata() {
  const script = `
$ErrorActionPreference = "SilentlyContinue"
$adapters = @(Get-CimInstance Win32_VideoController | ForEach-Object { $_.Name } | Where-Object { $_ })
$monitors = @(Get-CimInstance -Namespace root\\wmi -ClassName WmiMonitorID | ForEach-Object {
  $chars = @($_.UserFriendlyName | Where-Object { $_ -gt 0 })
  if ($chars.Count -gt 0) { -join ($chars | ForEach-Object { [char]$_ }) }
} | Where-Object { $_ })
[Console]::Out.Write((ConvertTo-Json -Compress @{ adapters = $adapters; monitors = $monitors }))
`;
  const stdout = await execFileText("powershell.exe", [
    "-NoProfile",
    "-NonInteractive",
    "-ExecutionPolicy",
    "Bypass",
    "-Command",
    script
  ], 2500);
  return JSON.parse(stdout || "{}");
}
async function getNativeCloudGsyncCapabilities(overrideValue = process.env.OPENNOW_NATIVE_CLOUD_GSYNC) {
  const override = normalizeCloudGsyncOverride(overrideValue);
  if (override === "1") {
    return assumedWindowsCapabilities("OPENNOW_NATIVE_CLOUD_GSYNC=1");
  }
  if (process.platform !== "win32") {
    return unsupportedNativeCloudGsyncCapabilities("unsupported-platform");
  }
  try {
    const probe = await probeWindowsDisplayMetadata();
    const adapters = Array.isArray(probe.adapters) ? probe.adapters.filter(Boolean) : [];
    const monitors = Array.isArray(probe.monitors) ? probe.monitors.filter(Boolean) : [];
    const hasNvidiaAdapter = adapters.some((name) => /nvidia/i.test(name));
    if (!hasNvidiaAdapter) {
      return {
        ...unsupportedNativeCloudGsyncCapabilities("no-nvidia-adapter"),
        reason: `no-nvidia-adapter adapters=${adapters.join(",") || "none"} monitors=${monitors.join(",") || "none"}`
      };
    }
    return assumedWindowsCapabilities(
      `nvidia-adapter-assumed-vrr adapters=${adapters.join(",")} monitors=${monitors.join(",") || "unknown"}`
    );
  } catch (error) {
    return {
      ...unsupportedNativeCloudGsyncCapabilities("detection-failed"),
      reason: `detection-failed ${error instanceof Error ? error.message : String(error)}`
    };
  }
}
function shouldForceNewSession(strategy) {
  return strategy === "force-new";
}
async function resolveSessionCloudGsyncSettings(settings) {
  const userRequested = settings.enableCloudGsync;
  const clientMode = settings.clientMode ?? "web";
  const cloudGsyncMode = settings.nativeCloudGsyncMode ?? "auto";
  const capabilities = clientMode === "native" ? await getNativeCloudGsyncCapabilities(cloudGsyncMode) : void 0;
  const resolution = resolveCloudGsync({
    userRequested,
    fps: settings.fps,
    clientMode,
    nativeBackendAvailable: clientMode === "native",
    capabilities,
    override: normalizeCloudGsyncOverride(cloudGsyncMode)
  });
  console.log(
    `[CloudGsync] requested=${resolution.requested} resolved=${resolution.enabled} reflex=${resolution.reflexEnabled} reason=${resolution.reason} clientMode=${clientMode} fps=${settings.fps} capabilities=${JSON.stringify(resolution.capabilities)}`
  );
  if (resolution.enabled) {
    console.log(
      "[CloudGsync] Native Cloud G-Sync/VRR mode is resolved on; keeping low-latency unthrottled presentation."
    );
  }
  return {
    ...settings,
    requestedCloudGsync: userRequested,
    enableCloudGsync: resolution.enabled,
    cloudGsyncResolution: resolution
  };
}
const AUTO_RESUME_SESSION_STATUSES = /* @__PURE__ */ new Set([2, 3]);
const ACTIVE_CREATE_SESSION_STATUSES = /* @__PURE__ */ new Set([1, 2, 3]);
function isAutoResumeReadySession(entry) {
  return entry.serverIp != null && AUTO_RESUME_SESSION_STATUSES.has(entry.status);
}
function isActiveCreateSessionConflict(entry) {
  return ACTIVE_CREATE_SESSION_STATUSES.has(entry.status);
}
function selectReadySessionToClaim(activeSessions, numericAppId) {
  return activeSessions.find(
    (session2) => isAutoResumeReadySession(session2) && session2.appId === numericAppId
  ) ?? activeSessions.find((session2) => isAutoResumeReadySession(session2)) ?? null;
}
function selectLaunchingSession(activeSessions, numericAppId) {
  return activeSessions.find(
    (session2) => session2.serverIp && session2.appId === numericAppId && session2.status === 1
  ) ?? activeSessions.find(
    (session2) => session2.serverIp && session2.status === 1
  ) ?? null;
}
async function stopActiveSessionsForCreate(params) {
  const { token, streamingBaseUrl, zone, appId } = params;
  const numericAppId = Number.parseInt(appId, 10);
  const activeSessions = await getActiveSessions(token, streamingBaseUrl);
  const sessionsToStop = activeSessions.filter(isActiveCreateSessionConflict);
  if (sessionsToStop.length === 0) {
    return;
  }
  console.log(
    `[CreateSession] Force-new requested; stopping ${sessionsToStop.length} existing active session(s) before create.`
  );
  for (const activeSession of sessionsToStop) {
    if (!activeSession.serverIp) {
      console.warn(
        `[CreateSession] Cannot stop existing session ${activeSession.sessionId} (appId=${activeSession.appId}, status=${activeSession.status}) because serverIp is missing.`
      );
      continue;
    }
    console.log(
      `[CreateSession] Stopping existing session id=${activeSession.sessionId}, appId=${activeSession.appId}, status=${activeSession.status}${activeSession.appId === numericAppId ? " (same app)" : ""}.`
    );
    await stopSession({
      token,
      streamingBaseUrl,
      serverIp: activeSession.serverIp,
      zone,
      sessionId: activeSession.sessionId
    });
  }
}
function registerSessionIpcHandlers(deps) {
  const {
    ipcMain: ipcMain2,
    authService: authService2,
    settingsManager: settingsManager2,
    resolveJwt: resolveJwt2,
    setActivity: setActivity2,
    clearActivity: clearActivity2
  } = deps;
  ipcMain2.handle(
    IPC_CHANNELS.CREATE_SESSION,
    async (_event, payload) => {
      const token = await resolveJwt2(payload.token);
      const streamingBaseUrl = payload.streamingBaseUrl ?? authService2.getSelectedProvider().streamingServiceUrl;
      const forceNewSession = shouldForceNewSession(
        payload.existingSessionStrategy
      );
      const resolvedSettings = await resolveSessionCloudGsyncSettings(
        payload.settings
      );
      const resolvedPayload = {
        ...payload,
        settings: resolvedSettings
      };
      const tryClaimExisting = async () => {
        if (!token) return null;
        try {
          const activeSessions = await getActiveSessions(
            token,
            streamingBaseUrl
          );
          if (activeSessions.length === 0) return null;
          const numericAppId = parseInt(resolvedPayload.appId, 10);
          const readyCandidate = selectReadySessionToClaim(
            activeSessions,
            numericAppId
          );
          if (readyCandidate) {
            console.log(
              `[CreateSession] Resuming existing session (id=${readyCandidate.sessionId}, appId=${readyCandidate.appId}, status=${readyCandidate.status}) instead of creating new.`
            );
            return claimSession({
              token,
              streamingBaseUrl,
              sessionId: readyCandidate.sessionId,
              serverIp: readyCandidate.serverIp,
              appId: resolvedPayload.appId,
              settings: resolvedPayload.settings
            });
          }
          const launchingCandidate = selectLaunchingSession(
            activeSessions,
            numericAppId
          );
          if (launchingCandidate) {
            console.log(
              `[CreateSession] Found launching session (id=${launchingCandidate.sessionId}, appId=${launchingCandidate.appId}, status=1); returning for renderer queue/ad polling.`
            );
            try {
              return await pollSession({
                token,
                streamingBaseUrl,
                serverIp: launchingCandidate.serverIp,
                zone: resolvedPayload.zone,
                sessionId: launchingCandidate.sessionId,
                proxyUrl: payload.proxyUrl
              });
            } catch (hydrateError) {
              console.warn(
                `[CreateSession] Failed to hydrate launching session ${launchingCandidate.sessionId}; falling back to minimal handoff: ${formatErrorChainForLog(hydrateError)}`
              );
              return {
                sessionId: launchingCandidate.sessionId,
                status: 1,
                zone: resolvedPayload.zone,
                streamingBaseUrl,
                serverIp: launchingCandidate.serverIp,
                signalingServer: launchingCandidate.serverIp,
                signalingUrl: launchingCandidate.signalingUrl ?? `wss://${launchingCandidate.serverIp}:443/nvst/`,
                iceServers: []
              };
            }
          }
          return null;
        } catch (claimError) {
          console.warn(
            `[CreateSession] Failed to claim existing session: ${formatErrorChainForLog(claimError)}`
          );
          return null;
        }
      };
      if (!forceNewSession) {
        const preChecked = await tryClaimExisting();
        if (preChecked) {
          if (settingsManager2.get("discordRichPresence")) {
            void setActivity2(
              payload.internalTitle || payload.appId,
              /* @__PURE__ */ new Date(),
              payload.appId
            );
          }
          return preChecked;
        }
      }
      try {
        if (forceNewSession && token) {
          await stopActiveSessionsForCreate({
            token,
            streamingBaseUrl,
            zone: resolvedPayload.zone,
            appId: resolvedPayload.appId
          });
        }
        const sessionResult = await createSession({
          ...resolvedPayload,
          token,
          streamingBaseUrl
        });
        if (settingsManager2.get("discordRichPresence")) {
          void setActivity2(
            payload.internalTitle || payload.appId,
            /* @__PURE__ */ new Date(),
            payload.appId
          );
        }
        return sessionResult;
      } catch (error) {
        if (!forceNewSession && error instanceof SessionError && error.statusCode === 11) {
          console.warn(
            "[CreateSession] SESSION_LIMIT_EXCEEDED — retrying as session claim."
          );
          const fallback = await tryClaimExisting();
          if (fallback) {
            if (settingsManager2.get("discordRichPresence")) {
              void setActivity2(
                payload.internalTitle || payload.appId,
                /* @__PURE__ */ new Date(),
                payload.appId
              );
            }
            return fallback;
          }
        }
        rethrowSerializedSessionError(error);
      }
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.POLL_SESSION,
    async (_event, payload) => {
      try {
        const token = await resolveJwt2(payload.token);
        return pollSession({
          ...payload,
          token,
          streamingBaseUrl: payload.streamingBaseUrl ?? authService2.getSelectedProvider().streamingServiceUrl
        });
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.REPORT_SESSION_AD,
    async (_event, payload) => {
      try {
        const token = await resolveJwt2(payload.token);
        return reportSessionAd({
          ...payload,
          token,
          streamingBaseUrl: payload.streamingBaseUrl ?? authService2.getSelectedProvider().streamingServiceUrl
        });
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.STOP_SESSION,
    async (_event, payload) => {
      try {
        const token = await resolveJwt2(payload.token);
        const result = await stopSession({
          ...payload,
          token,
          streamingBaseUrl: payload.streamingBaseUrl ?? authService2.getSelectedProvider().streamingServiceUrl
        });
        void clearActivity2();
        return result;
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.GET_ACTIVE_SESSIONS,
    async (_event, token, streamingBaseUrl) => {
      const jwt = await resolveJwt2(token);
      const baseUrl = streamingBaseUrl ?? authService2.getSelectedProvider().streamingServiceUrl;
      return getActiveSessions(jwt, baseUrl);
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.CLAIM_SESSION,
    async (_event, payload) => {
      try {
        const token = await resolveJwt2(payload.token);
        const streamingBaseUrl = payload.streamingBaseUrl ?? authService2.getSelectedProvider().streamingServiceUrl;
        const resolvedSettings = payload.settings ? await resolveSessionCloudGsyncSettings(payload.settings) : void 0;
        return claimSession({
          ...payload,
          token,
          streamingBaseUrl,
          settings: resolvedSettings
        });
      } catch (error) {
        rethrowSerializedSessionError(error);
      }
    }
  );
  ipcMain2.handle(
    IPC_CHANNELS.SESSION_CONFLICT_DIALOG,
    async () => {
      return showSessionConflictDialog$1(deps);
    }
  );
}
const USER_AGENT = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36";
class GfnSignalingClient {
  constructor(signalingServer, sessionId, signalingUrl) {
    this.signalingServer = signalingServer;
    this.sessionId = sessionId;
    this.signalingUrl = signalingUrl;
  }
  signalingServer;
  sessionId;
  signalingUrl;
  ws = null;
  peerId = 0;
  remotePeerId = 1;
  peerName = `peer-${Math.floor(Math.random() * 1e10)}`;
  ackCounter = 0;
  heartbeatTimer = null;
  connectionGeneration = 0;
  listeners = /* @__PURE__ */ new Set();
  buildSignInUrl() {
    const fallbackHost = this.signalingServer.includes(":") ? this.signalingServer : `${this.signalingServer}:443`;
    const baseUrl = this.signalingUrl?.trim() || `wss://${fallbackHost}/nvst/`;
    const signInUrl = new URL(baseUrl);
    signInUrl.protocol = "wss:";
    signInUrl.pathname = `${signInUrl.pathname.replace(/\/?$/, "/")}sign_in`;
    signInUrl.search = "";
    signInUrl.searchParams.set("peer_id", this.peerName);
    signInUrl.searchParams.set("version", "2");
    signInUrl.searchParams.set("peer_role", "1");
    signInUrl.searchParams.set("pairing_id", this.sessionId);
    const url = signInUrl.toString();
    console.log("[Signaling] URL:", url, "(server:", this.signalingServer, ", signalingUrl:", this.signalingUrl, ")");
    return url;
  }
  onEvent(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }
  emit(event) {
    for (const listener of this.listeners) {
      listener(event);
    }
  }
  nextAckId() {
    this.ackCounter += 1;
    return this.ackCounter;
  }
  sendJson(payload) {
    if (!this.ws || this.ws.readyState !== WebSocket.OPEN) {
      return;
    }
    this.ws.send(JSON.stringify(payload));
  }
  setupHeartbeat() {
    this.clearHeartbeat();
    this.heartbeatTimer = setInterval(() => {
      this.sendJson({ hb: 1 });
    }, 5e3);
    this.heartbeatTimer.unref?.();
  }
  clearHeartbeat() {
    if (this.heartbeatTimer) {
      clearInterval(this.heartbeatTimer);
      this.heartbeatTimer = null;
    }
  }
  sendPeerInfo() {
    this.sendJson({
      ackid: this.nextAckId(),
      peer_info: {
        browser: "Chrome",
        browserVersion: "131",
        connected: true,
        id: this.peerId,
        name: this.peerName,
        peerRole: 0,
        resolution: "1920x1080",
        version: 2
      }
    });
  }
  async connect() {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      return;
    }
    const url = this.buildSignInUrl();
    const protocol2 = `x-nv-sessionid.${this.sessionId}`;
    const generation = ++this.connectionGeneration;
    console.log("[Signaling] Connecting to:", url);
    console.log("[Signaling] Session ID:", this.sessionId);
    console.log("[Signaling] Protocol:", protocol2);
    await new Promise((resolve2, reject) => {
      const urlHost = url.replace(/^wss?:\/\//, "").split("/")[0];
      const ws = new WebSocket(url, protocol2, {
        rejectUnauthorized: false,
        headers: {
          Host: urlHost,
          Origin: "https://play.geforcenow.com",
          "User-Agent": USER_AGENT,
          "Sec-WebSocket-Key": randomBytes(16).toString("base64")
        }
      });
      this.ws = ws;
      const isCurrentSocket = () => this.ws === ws && this.connectionGeneration === generation;
      ws.once("error", (error) => {
        if (!isCurrentSocket()) {
          return;
        }
        this.emit({ type: "error", message: `Signaling connect failed: ${String(error)}` });
        reject(error);
      });
      ws.once("open", () => {
        if (!isCurrentSocket()) {
          return;
        }
        this.sendPeerInfo();
        this.setupHeartbeat();
        this.emit({ type: "connected" });
        resolve2();
      });
      ws.on("message", (raw) => {
        if (!isCurrentSocket()) {
          return;
        }
        const text = typeof raw === "string" ? raw : raw.toString("utf8");
        this.handleMessage(text);
      });
      ws.on("close", (_code, reason) => {
        this.clearHeartbeat();
        if (!isCurrentSocket()) {
          return;
        }
        this.ws = null;
        const reasonText = typeof reason === "string" ? reason : reason.toString("utf8");
        this.emit({ type: "disconnected", reason: reasonText || "socket closed" });
      });
    });
  }
  handleMessage(text) {
    let parsed;
    try {
      parsed = JSON.parse(text);
    } catch {
      this.emit({ type: "log", message: `Ignoring non-JSON signaling packet: ${text.slice(0, 120)}` });
      return;
    }
    if (parsed.peer_info) {
      if (typeof parsed.peer_info.id === "number" && parsed.peer_info.name === this.peerName) {
        this.peerId = parsed.peer_info.id;
        console.log(`[Signaling] Local peer id assigned: ${this.peerId}`);
      }
    }
    if (typeof parsed.ackid === "number") {
      const shouldAck = parsed.peer_info?.id !== this.peerId;
      if (shouldAck) {
        this.sendJson({ ack: parsed.ackid });
      }
    }
    if (parsed.hb) {
      this.sendJson({ hb: 1 });
      return;
    }
    if (!parsed.peer_msg?.msg) {
      return;
    }
    if (typeof parsed.peer_msg.from === "number") {
      this.remotePeerId = parsed.peer_msg.from;
      console.log(`[Signaling] Remote peer id: ${this.remotePeerId}`);
    }
    let peerPayload;
    try {
      peerPayload = JSON.parse(parsed.peer_msg.msg);
    } catch {
      this.emit({ type: "log", message: "Received non-JSON peer payload" });
      return;
    }
    if (peerPayload.type === "offer" && typeof peerPayload.sdp === "string") {
      console.log(`[Signaling] Received OFFER SDP (${peerPayload.sdp.length} chars), first 500 chars:`);
      console.log(peerPayload.sdp.slice(0, 500));
      this.emit({ type: "offer", sdp: peerPayload.sdp });
      return;
    }
    if (typeof peerPayload.candidate === "string") {
      console.log(`[Signaling] Received remote ICE candidate: ${peerPayload.candidate}`);
      this.emit({
        type: "remote-ice",
        candidate: {
          candidate: peerPayload.candidate,
          sdpMid: typeof peerPayload.sdpMid === "string" || peerPayload.sdpMid === null ? peerPayload.sdpMid : void 0,
          sdpMLineIndex: typeof peerPayload.sdpMLineIndex === "number" || peerPayload.sdpMLineIndex === null ? peerPayload.sdpMLineIndex : void 0
        }
      });
      return;
    }
    console.log("[Signaling] Unhandled peer message keys:", Object.keys(peerPayload));
  }
  async sendAnswer(payload) {
    console.log(`[Signaling] Sending ANSWER SDP (${payload.sdp.length} chars), first 500 chars:`);
    console.log(payload.sdp.slice(0, 500));
    if (payload.nvstSdp) {
      console.log(`[Signaling] Sending nvstSdp (${payload.nvstSdp.length} chars):`);
      console.log(payload.nvstSdp);
    }
    const answer = {
      type: "answer",
      sdp: payload.sdp,
      ...payload.nvstSdp ? { nvstSdp: payload.nvstSdp } : {}
    };
    console.log(`[Signaling] Sending answer peer_msg from=${this.peerId} to=${this.remotePeerId}`);
    this.sendJson({
      peer_msg: {
        from: this.peerId,
        to: this.remotePeerId,
        msg: JSON.stringify(answer)
      },
      ackid: this.nextAckId()
    });
  }
  async sendIceCandidate(candidate) {
    if (isTcpIceCandidate(candidate.candidate)) {
      console.log(`[Signaling] Dropping TCP local ICE candidate: ${candidate.candidate}`);
      return;
    }
    console.log(`[Signaling] Sending local ICE candidate: ${candidate.candidate} (sdpMid=${candidate.sdpMid})`);
    console.log(`[Signaling] Sending ICE peer_msg from=${this.peerId} to=${this.remotePeerId}`);
    this.sendJson({
      peer_msg: {
        from: this.peerId,
        to: this.remotePeerId,
        msg: JSON.stringify({
          candidate: candidate.candidate,
          sdpMid: candidate.sdpMid,
          sdpMLineIndex: candidate.sdpMLineIndex
        })
      },
      ackid: this.nextAckId()
    });
  }
  async requestKeyframe(payload) {
    this.sendJson({
      peer_msg: {
        from: this.peerId,
        to: this.remotePeerId,
        msg: JSON.stringify({
          type: "request_keyframe",
          reason: payload.reason,
          backlogFrames: payload.backlogFrames,
          attempt: payload.attempt
        })
      },
      ackid: this.nextAckId()
    });
    console.log(
      `[Signaling] Sent keyframe request (reason=${payload.reason}, backlog=${payload.backlogFrames}, attempt=${payload.attempt})`
    );
  }
  disconnect() {
    this.connectionGeneration += 1;
    this.clearHeartbeat();
    if (this.ws) {
      const socket = this.ws;
      this.ws = null;
      socket.close();
    }
  }
}
function isTcpIceCandidate(candidate) {
  const parts = candidate.trim().split(/\s+/);
  return parts[2]?.toLowerCase() === "tcp";
}
const NATIVE_STREAMER_PROTOCOL_VERSION = 2;
const HELLO_TIMEOUT_MS = 1e4;
const BUNDLED_GSTREAMER_HELLO_TIMEOUT_MS = process.platform === "win32" ? 12e4 : 3e4;
const CONTROL_TIMEOUT_MS = 8e3;
const SESSION_START_TIMEOUT_MS = process.platform === "win32" ? 9e4 : 45e3;
const SURFACE_UPDATE_TIMEOUT_MS = 15e3;
const OFFER_TIMEOUT_MS = 2e4;
const STOP_TIMEOUT_MS = 1200;
const MAX_INPUT_STDIN_BUFFER_BYTES = 64 * 1024;
const MIN_NATIVE_BITRATE_KBPS = 5e3;
const MAX_NATIVE_BITRATE_KBPS = 15e4;
function nativeStreamerExecutableName() {
  return process.platform === "win32" ? "opennow-streamer.exe" : "opennow-streamer";
}
function nativeStreamerPlatformKey() {
  return `${process.platform}-${process.arch}`;
}
function isExistingFile(path) {
  try {
    return existsSync(path) && statSync(path).isFile();
  } catch {
    return false;
  }
}
function isExistingDirectory(path) {
  try {
    return existsSync(path) && statSync(path).isDirectory();
  } catch {
    return false;
  }
}
function normalizePathForComparison(path) {
  let resolvedPath = resolve(path);
  try {
    resolvedPath = realpathSync.native(resolvedPath);
  } catch {
  }
  return process.platform === "win32" ? resolvedPath.toLowerCase() : resolvedPath;
}
function isPathInside(parent, child) {
  const normalizedParent = normalizePathForComparison(parent);
  const normalizedChild = normalizePathForComparison(child);
  return normalizedChild === normalizedParent || normalizedChild.startsWith(`${normalizedParent}${sep}`);
}
function hasBundledRuntimeNextToExecutable(executablePath) {
  return isExistingDirectory(join(dirname(executablePath), "gstreamer"));
}
function safePathSegment(value) {
  return value.replace(/[^a-zA-Z0-9._-]/g, "_") || "unknown";
}
function fileSha256(path) {
  return createHash("sha256").update(readFileSync(path)).digest("hex");
}
function prependEnvPath(env, key, directory) {
  env[key] = env[key] ? `${directory}${delimiter}${env[key]}` : directory;
}
function prependProcessPath(env, directory) {
  const pathKey = Object.keys(env).find((key) => key.toLowerCase() === "path") || "PATH";
  prependEnvPath(env, pathKey, directory);
}
const LINUX_GSTREAMER_INSTALL_INSTRUCTIONS = [
  {
    distro: "Debian / Ubuntu / Mint / Pop!_OS / KDE neon",
    command: "sudo apt update && sudo apt install libgstreamer1.0-0 libgstreamer-plugins-base1.0-0 gstreamer1.0-tools gstreamer1.0-libav gstreamer1.0-plugins-base gstreamer1.0-plugins-good gstreamer1.0-plugins-bad gstreamer1.0-plugins-ugly gstreamer1.0-gl gstreamer1.0-vaapi gstreamer1.0-x gstreamer1.0-alsa libva2 libva-drm2 libvulkan1 mesa-vulkan-drivers"
  },
  {
    distro: "Fedora / RHEL / Nobara / Bazzite",
    command: "sudo dnf install gstreamer1 gstreamer1-plugins-base gstreamer1-plugins-good gstreamer1-plugins-bad-free gstreamer1-plugins-bad-freeworld gstreamer1-plugins-ugly gstreamer1-libav gstreamer1-vaapi gstreamer1-plugin-openh264 mesa-vulkan-drivers libva",
    note: "RPM Fusion may be required for libav, ugly, or bad-freeworld packages."
  },
  {
    distro: "Arch / Manjaro / EndeavourOS / SteamOS",
    command: "sudo pacman -S --needed gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad gst-plugins-ugly gst-libav gst-plugin-va libva mesa vulkan-radeon",
    note: "NVIDIA users should use their distro NVIDIA/Vulkan driver packages instead of vulkan-radeon."
  },
  {
    distro: "openSUSE Tumbleweed / Leap",
    command: "sudo zypper install gstreamer gstreamer-plugins-base gstreamer-plugins-good gstreamer-plugins-bad gstreamer-plugins-ugly gstreamer-plugins-libav gstreamer-plugins-vaapi libva2 Mesa-vulkan-device-select"
  }
];
function linuxInstallInstructions() {
  return process.platform === "linux" ? LINUX_GSTREAMER_INSTALL_INSTRUCTIONS : void 0;
}
function configureBundledGstreamerRuntime(env, executablePath) {
  const runtimeRoot = join(dirname(executablePath), "gstreamer");
  if (!isExistingDirectory(runtimeRoot)) {
    return {
      source: "system",
      bundled: false,
      message: process.platform === "linux" ? "No bundled GStreamer runtime was found. Linux uses distro GStreamer packages so VAAPI/V4L2/Vulkan plugins match the host driver stack." : "No bundled GStreamer runtime was found; using the system runtime if available.",
      installInstructions: linuxInstallInstructions()
    };
  }
  const binDir = join(runtimeRoot, "bin");
  const libDir = join(runtimeRoot, "lib");
  const pluginDir = join(runtimeRoot, "lib", "gstreamer-1.0");
  const scanner = join(
    runtimeRoot,
    "libexec",
    "gstreamer-1.0",
    process.platform === "win32" ? "gst-plugin-scanner.exe" : "gst-plugin-scanner"
  );
  const gioModulesDir = join(runtimeRoot, "lib", "gio", "modules");
  if (process.platform === "win32") prependProcessPath(env, dirname(executablePath));
  if (isExistingDirectory(binDir)) prependProcessPath(env, binDir);
  if (isExistingDirectory(pluginDir)) {
    env.GST_PLUGIN_PATH = pluginDir;
    env.GST_PLUGIN_PATH_1_0 = pluginDir;
    env.GST_PLUGIN_SYSTEM_PATH = pluginDir;
    env.GST_PLUGIN_SYSTEM_PATH_1_0 = pluginDir;
  }
  if (isExistingFile(scanner)) {
    env.GST_PLUGIN_SCANNER = scanner;
    env.GST_PLUGIN_SCANNER_1_0 = scanner;
  }
  env.GST_REGISTRY_REUSE_PLUGIN_SCANNER = "no";
  if (isExistingDirectory(gioModulesDir)) {
    env.GIO_MODULE_DIR = gioModulesDir;
    env.GIO_EXTRA_MODULES = gioModulesDir;
  }
  const registryDir = join(app.getPath("userData"), "native-streamer", "gstreamer");
  const registryPath = join(registryDir, `${nativeStreamerPlatformKey()}-registry.bin`);
  mkdirSync(registryDir, { recursive: true });
  env.GST_REGISTRY = registryPath;
  if (process.platform === "linux") {
    if (isExistingDirectory(libDir)) prependEnvPath(env, "LD_LIBRARY_PATH", libDir);
    if (isExistingDirectory(binDir)) prependEnvPath(env, "LD_LIBRARY_PATH", binDir);
  }
  if (process.platform === "darwin") {
    if (isExistingDirectory(libDir)) {
      prependEnvPath(env, "DYLD_LIBRARY_PATH", libDir);
      prependEnvPath(env, "DYLD_FALLBACK_LIBRARY_PATH", libDir);
    }
    if (isExistingDirectory(binDir)) {
      prependEnvPath(env, "DYLD_LIBRARY_PATH", binDir);
      prependEnvPath(env, "DYLD_FALLBACK_LIBRARY_PATH", binDir);
    }
  }
  return {
    source: "bundled",
    bundled: true,
    path: runtimeRoot,
    message: "Using bundled GStreamer runtime next to the native streamer executable."
  };
}
function isWindowsDllLoadFailure(error) {
  const message = formatError(error);
  return process.platform === "win32" && (message.includes("3221225781") || message.toLowerCase().includes("0xc0000135"));
}
function formatNativeStreamerDetectionFailure(error, runtime) {
  if (isWindowsDllLoadFailure(error)) {
    return runtime?.bundled ? `Native streamer could not load a required DLL even though bundled GStreamer was detected at ${runtime.path}. The packaged runtime may be incomplete or blocked. ${formatError(error)}` : `Native streamer could not load a required DLL and no bundled GStreamer runtime was detected. ${formatError(error)}`;
  }
  return `Native streamer was not detected: ${formatError(error)}`;
}
function formatError(error) {
  return error instanceof Error ? error.message : String(error);
}
function normalizeBitrateKbps(value) {
  if (!Number.isFinite(value)) {
    return MIN_NATIVE_BITRATE_KBPS;
  }
  return Math.min(
    MAX_NATIVE_BITRATE_KBPS,
    Math.max(MIN_NATIVE_BITRATE_KBPS, Math.round(value))
  );
}
function formatVideoBackendName(backend) {
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
function formatVideoCodec(codec) {
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
function resolveActiveVideoBackend(videoBackends, preferredBackend = "auto") {
  const currentPlatform = process.platform === "win32" ? "windows" : process.platform === "darwin" ? "macos" : process.platform === "linux" ? "linux" : "other";
  if (preferredBackend !== "auto") {
    const preferred = videoBackends.find((candidate) => candidate.available && candidate.backend === preferredBackend);
    if (preferred) return preferred;
  }
  return videoBackends.find((candidate) => candidate.available && candidate.platform === currentPlatform) ?? videoBackends.find((candidate) => candidate.available && candidate.platform === "cross-platform") ?? videoBackends.find((candidate) => candidate.available);
}
function summarizeCodecs(backend) {
  const codecs = backend?.codecs.filter((codec) => codec.available).map((codec) => formatVideoCodec(codec.codec)) ?? [];
  return codecs.length > 0 ? codecs.join(", ") : "No hardware codec path";
}
function summarizeZeroCopy(backend) {
  if (!backend) {
    return "Not available";
  }
  return backend.zeroCopyModes.length > 0 ? `Hardware memory: ${backend.zeroCopyModes.join(", ")}` : "System memory";
}
function isRecord(value) {
  return typeof value === "object" && value !== null;
}
function isResponse(message) {
  return isRecord(message) && typeof message["id"] === "string";
}
function isEvent(message) {
  return isRecord(message) && typeof message["id"] !== "string";
}
function shouldUseStablePackagedNativeStreamerCache() {
  return app.isPackaged && process.platform === "win32" && isPathInside(tmpdir(), process.resourcesPath);
}
function buildPackagedNativeStreamerCacheMarker(sourceDirectory, exeName, platformKey) {
  const runtimeManifest = join(sourceDirectory, "gstreamer", "OPENNOW-GSTREAMER-RUNTIME.txt");
  return {
    appVersion: app.getVersion(),
    platformKey,
    exeName,
    exeSha256: fileSha256(join(sourceDirectory, exeName)),
    bundledRuntime: isExistingDirectory(join(sourceDirectory, "gstreamer")),
    runtimeManifestSha256: isExistingFile(runtimeManifest) ? fileSha256(runtimeManifest) : void 0
  };
}
function readCacheMarker(markerPath) {
  try {
    return JSON.parse(readFileSync(markerPath, "utf8"));
  } catch {
    return null;
  }
}
function isSameCacheMarker(left, right) {
  if (!left) {
    return false;
  }
  return left.appVersion === right.appVersion && left.platformKey === right.platformKey && left.exeName === right.exeName && left.exeSha256 === right.exeSha256 && left.bundledRuntime === right.bundledRuntime && left.runtimeManifestSha256 === right.runtimeManifestSha256;
}
function materializePackagedNativeStreamerCache(sourceExecutablePath, platformKey, exeName) {
  if (!shouldUseStablePackagedNativeStreamerCache()) {
    return null;
  }
  const sourceDirectory = dirname(sourceExecutablePath);
  const cacheDirectory = join(
    app.getPath("userData"),
    "native-streamer",
    "runtime",
    safePathSegment(app.getVersion()),
    safePathSegment(platformKey)
  );
  const cachedExecutablePath = join(cacheDirectory, exeName);
  const markerPath = join(cacheDirectory, ".opennow-native-runtime.json");
  let stagingDirectory = null;
  try {
    const expectedMarker = buildPackagedNativeStreamerCacheMarker(sourceDirectory, exeName, platformKey);
    const cachedMarker = readCacheMarker(markerPath);
    if (isExistingFile(cachedExecutablePath) && isSameCacheMarker(cachedMarker, expectedMarker) && (!expectedMarker.bundledRuntime || hasBundledRuntimeNextToExecutable(cachedExecutablePath))) {
      return cachedExecutablePath;
    }
    stagingDirectory = `${cacheDirectory}.tmp-${process.pid}-${Date.now()}`;
    rmSync(stagingDirectory, { recursive: true, force: true });
    mkdirSync(dirname(stagingDirectory), { recursive: true });
    cpSync(sourceDirectory, stagingDirectory, {
      recursive: true,
      force: true,
      dereference: true,
      filter: (entry) => {
        const lower = entry.toLowerCase();
        return !lower.endsWith(".pdb") && !lower.endsWith(".lib") && !lower.endsWith(".a");
      }
    });
    writeFileSync(
      join(stagingDirectory, ".opennow-native-runtime.json"),
      `${JSON.stringify(expectedMarker, null, 2)}
`,
      "utf8"
    );
    if (!isExistingFile(join(stagingDirectory, exeName))) {
      throw new Error(`Cached native streamer executable was not created: ${join(stagingDirectory, exeName)}`);
    }
    if (expectedMarker.bundledRuntime && !hasBundledRuntimeNextToExecutable(join(stagingDirectory, exeName))) {
      throw new Error("Cached native streamer runtime is missing its bundled GStreamer directory.");
    }
    rmSync(cacheDirectory, { recursive: true, force: true });
    renameSync(stagingDirectory, cacheDirectory);
    stagingDirectory = null;
    console.log("[NativeStreamer] Cached packaged native streamer in stable runtime path:", cachedExecutablePath);
    return cachedExecutablePath;
  } catch (error) {
    console.warn("[NativeStreamer] Failed to prepare stable packaged runtime cache; using packaged resource path:", error);
    return null;
  } finally {
    if (stagingDirectory) {
      rmSync(stagingDirectory, { recursive: true, force: true });
    }
  }
}
class NativeStreamerManager {
  constructor(options) {
    this.options = options;
  }
  options;
  child = null;
  startupPromise = null;
  stdoutBuffer = "";
  stderrTail = [];
  gstreamerRuntime = null;
  pending = /* @__PURE__ */ new Map();
  capabilities = null;
  activeSessionId = null;
  inputBackpressureWarned = false;
  answerInFlight = false;
  queuedLocalIce = [];
  queuedRemoteIceSessionId = null;
  queuedRemoteIce = [];
  lastSurface = null;
  surfaceUpdateInFlight = false;
  surfaceUpdateQueued = false;
  isRunning() {
    return this.child !== null;
  }
  hasActiveSession() {
    return this.activeSessionId !== null;
  }
  async prepareForSession(context) {
    if (this.activeSessionId && this.activeSessionId !== context.session.sessionId) {
      await this.stop("new native streamer session");
    }
    this.prepareRemoteIceQueue(context.session.sessionId);
    await this.ensureProcess();
    if (this.activeSessionId === context.session.sessionId) {
      return;
    }
    if (context.settings.enableCloudGsync) {
      console.log(
        "[NativeStreamer] Cloud G-Sync/VRR mode resolved for this session; preserving unthrottled low-latency present behavior."
      );
    }
    await this.request({
      type: "start",
      context
    }, SESSION_START_TIMEOUT_MS);
    this.activeSessionId = context.session.sessionId;
    await this.flushQueuedRemoteIce(context.session.sessionId);
  }
  async handleOffer(sdp, context) {
    const negotiatedProfile = context.session.negotiatedStreamProfile;
    console.log(
      "[NativeStreamer] Session context:",
      JSON.stringify({
        sessionId: context.session.sessionId,
        requestedResolution: context.settings.resolution,
        requestedFps: context.settings.fps,
        requestedCodec: context.settings.codec,
        negotiatedResolution: negotiatedProfile?.resolution,
        negotiatedFps: negotiatedProfile?.fps,
        negotiatedCodec: negotiatedProfile?.codec ?? context.settings.codec,
        requestedStreamingFeatures: context.session.requestedStreamingFeatures,
        finalizedStreamingFeatures: context.session.finalizedStreamingFeatures
      })
    );
    await this.prepareForSession(context);
    if (!this.capabilities?.supportsOfferAnswer) {
      console.warn(
        `[NativeStreamer] Backend "${this.capabilities?.backend ?? "unknown"}" reports offer/answer is not ready; forwarding offer for validation/fallback.`
      );
    }
    this.answerInFlight = true;
    this.queuedLocalIce = [];
    try {
      const response = await this.request({
        type: "offer",
        sdp,
        context
      }, OFFER_TIMEOUT_MS);
      if (response.type !== "answer") {
        throw new Error(`Native streamer returned ${response.type} instead of answer.`);
      }
      await this.options.sendAnswer(response.answer);
      this.answerInFlight = false;
      await this.flushQueuedLocalIce();
    } catch (error) {
      this.answerInFlight = false;
      this.queuedLocalIce = [];
      throw error;
    }
    this.options.emit({
      type: "log",
      message: "Native streamer accepted the WebRTC offer; waiting for decoded media."
    });
  }
  async probeStatus() {
    if (!isNativeStreamerSupportedPlatform(process.platform)) {
      return createUnsupportedNativeStreamerStatus();
    }
    try {
      await this.ensureProcess();
      const backend = this.capabilities?.backend;
      const gstreamerAvailable = backend === "gstreamer" && this.capabilities?.supportsOfferAnswer === true;
      const videoBackends = this.capabilities?.videoBackends ?? [];
      const activeVideoBackend = resolveActiveVideoBackend(
        videoBackends,
        this.options.getVideoBackendPreference()
      );
      const codecSummary = summarizeCodecs(activeVideoBackend);
      const zeroCopySummary = summarizeZeroCopy(activeVideoBackend);
      const runtime = this.gstreamerRuntime ?? {
        source: "unknown",
        bundled: false,
        message: "GStreamer runtime has not been checked yet.",
        installInstructions: linuxInstallInstructions()
      };
      const effectiveRuntime = gstreamerAvailable ? runtime.bundled ? runtime : {
        ...runtime,
        source: "system",
        message: "Using system GStreamer runtime; packaged Windows/macOS builds should use the bundled runtime."
      } : {
        ...runtime,
        source: runtime.bundled ? "bundled" : process.platform === "linux" ? "missing" : runtime.source,
        message: runtime.bundled ? "Bundled GStreamer runtime was found, but the GStreamer backend is not ready." : process.platform === "linux" ? "GStreamer is not ready. Install distro GStreamer packages so plugins match the host GPU/driver stack." : runtime.message,
        installInstructions: runtime.installInstructions ?? linuxInstallInstructions()
      };
      return {
        detected: true,
        gstreamerAvailable,
        supportsOfferAnswer: this.capabilities?.supportsOfferAnswer === true,
        backend,
        fallbackReason: this.capabilities?.fallbackReason,
        videoBackends,
        activeVideoBackend,
        codecSummary,
        zeroCopySummary,
        gstreamerRuntime: effectiveRuntime,
        message: gstreamerAvailable ? `${effectiveRuntime.message} Video path: ${formatVideoBackendName(activeVideoBackend?.backend)}.` : this.capabilities?.fallbackReason ?? effectiveRuntime.message
      };
    } catch (error) {
      const runtime = this.gstreamerRuntime ?? {
        source: process.platform === "linux" ? "missing" : "unknown",
        bundled: false,
        message: process.platform === "linux" ? "GStreamer is not ready. Linux uses distro packages because private AppImage GStreamer bundling is unreliable across glibc, libdrm/VAAPI/Vulkan, and GPU driver stacks." : "GStreamer runtime could not be checked because the native streamer did not start.",
        installInstructions: linuxInstallInstructions()
      };
      return {
        detected: false,
        gstreamerAvailable: false,
        supportsOfferAnswer: false,
        gstreamerRuntime: runtime,
        message: formatNativeStreamerDetectionFailure(error, runtime)
      };
    }
  }
  async addRemoteIce(candidate, context) {
    const sessionId = context.session.sessionId;
    if (!this.child || this.activeSessionId !== sessionId) {
      this.queueRemoteIce(sessionId, candidate);
      return;
    }
    await this.sendRemoteIce(candidate);
  }
  drainQueuedRemoteIce(sessionId) {
    if (this.queuedRemoteIceSessionId !== sessionId) {
      return [];
    }
    const queued = this.queuedRemoteIce;
    this.queuedRemoteIceSessionId = null;
    this.queuedRemoteIce = [];
    return queued;
  }
  sendInput(input) {
    const child = this.child;
    if (!child || child.killed || !child.stdin.writable || !this.activeSessionId || !this.capabilities?.supportsInput) {
      return;
    }
    if (child.stdin.writableLength > MAX_INPUT_STDIN_BUFFER_BYTES) {
      if (!this.inputBackpressureWarned) {
        this.inputBackpressureWarned = true;
        console.warn("[NativeStreamer] Dropping native input while streamer stdin is backpressured.");
      }
      return;
    }
    const payload = {
      id: randomUUID(),
      type: "input",
      input
    };
    const flushed = child.stdin.write(`${JSON.stringify(payload)}
`, "utf8", (error) => {
      if (error && !this.inputBackpressureWarned) {
        this.inputBackpressureWarned = true;
        console.warn("[NativeStreamer] Failed to write native input:", error);
      }
    });
    if (!flushed && !this.inputBackpressureWarned) {
      this.inputBackpressureWarned = true;
      console.warn("[NativeStreamer] Native input writer reported backpressure; input will be dropped until it drains.");
      child.stdin.once("drain", () => {
        this.inputBackpressureWarned = false;
      });
    } else if (flushed) {
      this.inputBackpressureWarned = false;
    }
  }
  updateSurface(surface) {
    this.lastSurface = surface;
    void this.flushSurfaceUpdate();
  }
  updateBitrateLimit(maxBitrateKbps) {
    if (!this.child || !this.activeSessionId) {
      return;
    }
    void this.request({
      type: "bitrate",
      maxBitrateKbps: normalizeBitrateKbps(maxBitrateKbps)
    }, CONTROL_TIMEOUT_MS).catch((error) => {
      console.warn("[NativeStreamer] Failed to update native bitrate limit:", error);
    });
  }
  async stop(reason = "stopped") {
    const child = this.child;
    this.activeSessionId = null;
    this.capabilities = null;
    this.clearQueuedRemoteIce();
    if (!child) {
      return;
    }
    try {
      await this.request({ type: "stop", reason }, STOP_TIMEOUT_MS);
    } catch (error) {
      console.warn("[NativeStreamer] Stop request failed:", error);
    } finally {
      this.terminateProcess();
    }
  }
  dispose(reason = "disposed") {
    this.activeSessionId = null;
    this.capabilities = null;
    this.clearQueuedRemoteIce();
    this.rejectPending(new Error(`Native streamer ${reason}.`));
    this.terminateProcess();
  }
  async ensureProcess() {
    if (!isNativeStreamerSupportedPlatform(process.platform)) {
      throw new Error(NATIVE_STREAMER_WINDOWS_ONLY_MESSAGE);
    }
    if (this.child && this.capabilities) {
      return;
    }
    if (this.startupPromise) {
      await this.startupPromise;
      return;
    }
    if (this.child && !this.capabilities) {
      console.warn("[NativeStreamer] Restarting native streamer after an incomplete startup handshake.");
      this.rejectPending(new Error("Native streamer startup handshake did not complete."));
      this.terminateProcess();
      this.stdoutBuffer = "";
      this.stderrTail = [];
    }
    const startupPromise = (async () => {
      const backendPreference = this.options.getBackendPreference();
      let lastError = null;
      for (const executablePath of this.resolveExecutableCandidates()) {
        try {
          await this.startProcess(executablePath, backendPreference);
          return;
        } catch (error) {
          lastError = error instanceof Error ? error : new Error(String(error));
          console.warn(
            `[NativeStreamer] Failed to initialize ${executablePath}: ${formatError(lastError)}`
          );
          this.rejectPending(lastError);
          this.terminateProcess();
          this.stdoutBuffer = "";
          this.stderrTail = [];
          this.capabilities = null;
        }
      }
      throw lastError ?? new Error("Native streamer could not be initialized from any candidate path.");
    })();
    this.startupPromise = startupPromise;
    try {
      await startupPromise;
    } finally {
      if (this.startupPromise === startupPromise) {
        this.startupPromise = null;
      }
    }
  }
  async startProcess(executablePath, backendPreference) {
    console.log("[NativeStreamer] Starting:", executablePath);
    console.log("[NativeStreamer] Backend preference:", backendPreference);
    const videoBackendPreference = this.options.getVideoBackendPreference();
    console.log("[NativeStreamer] Video backend preference:", videoBackendPreference);
    const childEnv = {
      ...process.env,
      OPENNOW_NATIVE_STREAMER_PROTOCOL: String(NATIVE_STREAMER_PROTOCOL_VERSION)
    };
    delete childEnv.OPENNOW_NATIVE_VIDEO_API;
    delete childEnv.OPENNOW_NATIVE_VIDEO_BACKEND;
    if (videoBackendPreference !== "auto") {
      childEnv.OPENNOW_NATIVE_VIDEO_BACKEND = videoBackendPreference;
    }
    if (process.platform === "win32") {
      childEnv.OPENNOW_NATIVE_EXTERNAL_RENDERER = this.options.getExternalRendererEnabled() ? "1" : "0";
    }
    childEnv.OPENNOW_NATIVE_CLOUD_GSYNC = nativeStreamerFeatureModeToEnvValue(this.options.getCloudGsyncMode());
    childEnv.OPENNOW_NATIVE_D3D_FULLSCREEN = nativeStreamerFeatureModeToEnvValue(this.options.getD3dFullscreenMode());
    if (backendPreference !== "auto") {
      childEnv.OPENNOW_NATIVE_STREAMER_BACKEND = backendPreference;
    }
    const runtimeStatus = configureBundledGstreamerRuntime(childEnv, executablePath);
    this.gstreamerRuntime = runtimeStatus;
    if (runtimeStatus.bundled) {
      console.log("[NativeStreamer] Using bundled GStreamer runtime:", runtimeStatus.path);
    } else {
      console.log("[NativeStreamer]", runtimeStatus.message);
    }
    const child = spawn(executablePath, [], {
      stdio: "pipe",
      // The default native path lets the GStreamer video sink create its own
      // render window. Hiding the child process also hides that sink window on
      // Windows, which leaves the Electron input placeholder black.
      windowsHide: false,
      env: childEnv
    });
    this.child = child;
    this.stdoutBuffer = "";
    this.stderrTail = [];
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => this.handleStdout(chunk));
    child.stderr.setEncoding("utf8");
    child.stderr.on("data", (chunk) => {
      for (const line of chunk.split(/\r?\n/)) {
        if (line.trim()) {
          this.appendStderr(line);
          console.warn(`[NativeStreamer] ${line}`);
        }
      }
    });
    child.once("error", (error) => {
      this.options.emit({ type: "error", message: `Native streamer failed to start: ${formatError(error)}` });
      this.handleProcessExit(`spawn error: ${formatError(error)}`);
    });
    child.once("exit", (code, signal) => {
      const reason = signal ? `signal ${signal}` : `exit code ${code ?? "unknown"}`;
      this.handleProcessExit(reason);
    });
    const helloTimeoutMs = runtimeStatus.bundled ? BUNDLED_GSTREAMER_HELLO_TIMEOUT_MS : HELLO_TIMEOUT_MS;
    const response = await this.request({
      type: "hello",
      protocolVersion: NATIVE_STREAMER_PROTOCOL_VERSION
    }, helloTimeoutMs);
    if (response.type !== "ready") {
      throw new Error(`Native streamer returned ${response.type} instead of ready.`);
    }
    this.capabilities = response.capabilities;
    console.log("[NativeStreamer] Capabilities:", response.capabilities);
    if (response.capabilities.protocolVersion !== NATIVE_STREAMER_PROTOCOL_VERSION) {
      throw new Error(
        `Native streamer reported protocolVersion=${response.capabilities.protocolVersion}, expected ${NATIVE_STREAMER_PROTOCOL_VERSION}.`
      );
    }
    this.assertBackendPreference(response.capabilities, backendPreference);
    await this.flushSurfaceUpdate();
  }
  assertBackendPreference(capabilities, backendPreference) {
    if (backendPreference === "auto" || capabilities.backend === backendPreference) {
      return;
    }
    const reason = capabilities.fallbackReason ? ` ${capabilities.fallbackReason}` : "";
    throw new Error(
      `Native streamer backend "${backendPreference}" is unavailable; process selected "${capabilities.backend}".${reason}`
    );
  }
  resolveExecutableCandidates() {
    const exeName = nativeStreamerExecutableName();
    const platformKey = nativeStreamerPlatformKey();
    const bundledCandidates = [
      join(process.resourcesPath, "native", "opennow-streamer", platformKey, exeName),
      join(process.resourcesPath, "native", "opennow-streamer", exeName)
    ];
    const candidates = [];
    const addCandidate = (candidate) => {
      if (!candidate || !isExistingFile(candidate) || candidates.includes(candidate)) {
        return;
      }
      candidates.push(candidate);
    };
    if (app.isPackaged) {
      for (const candidate of bundledCandidates) {
        if (!isExistingFile(candidate) || !hasBundledRuntimeNextToExecutable(candidate)) {
          continue;
        }
        addCandidate(materializePackagedNativeStreamerCache(candidate, platformKey, exeName) ?? void 0);
      }
    }
    bundledCandidates.forEach(addCandidate);
    if (app.isPackaged && candidates.length > 0) {
      const packagedBundledCandidates = candidates.filter(
        (candidate) => hasBundledRuntimeNextToExecutable(candidate)
      );
      return packagedBundledCandidates.length > 0 ? packagedBundledCandidates : candidates;
    }
    const configuredPath = this.options.getExecutablePathOverride().trim();
    if (configuredPath) {
      if (isExistingFile(configuredPath)) {
        if (!this.shouldIgnorePackagedExecutableOverride(configuredPath)) {
          addCandidate(configuredPath);
        } else {
          console.warn(
            "[NativeStreamer] Ignoring packaged executable override without bundled runtime:",
            configuredPath
          );
        }
      } else {
        throw new Error(`Configured native streamer executable was not found: ${configuredPath}`);
      }
    }
    [
      process.env.OPENNOW_NATIVE_STREAMER,
      ...bundledCandidates,
      resolve(this.options.mainDir, "../../../native/opennow-streamer/bin", platformKey, exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/bin", exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/dist", platformKey, exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/dist", exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/target/release", platformKey, exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/target/release", exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/target/debug", platformKey, exeName),
      resolve(this.options.mainDir, "../../../native/opennow-streamer/target/debug", exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/bin", platformKey, exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/bin", exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/dist", platformKey, exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/dist", exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/target/release", platformKey, exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/target/release", exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/target/debug", platformKey, exeName),
      resolve(app.getAppPath(), "../native/opennow-streamer/target/debug", exeName)
    ].filter((candidate) => Boolean(candidate)).forEach(addCandidate);
    if (candidates.length > 0) {
      return candidates;
    }
    throw new Error(`Native streamer binary not found. Checked: ${candidates.join(", ")}`);
  }
  shouldIgnorePackagedExecutableOverride(configuredPath) {
    if (hasBundledRuntimeNextToExecutable(configuredPath)) {
      return false;
    }
    const packagedRoots = [
      join(process.resourcesPath, "native", "opennow-streamer"),
      resolve(app.getAppPath(), "../native/opennow-streamer"),
      resolve(this.options.mainDir, "../../../dist-release/win-unpacked/resources/native/opennow-streamer"),
      resolve(this.options.mainDir, "../../../dist-release/win-unpacked/resources/app.asar.unpacked/native/opennow-streamer")
    ];
    return packagedRoots.some((root) => isPathInside(root, configuredPath));
  }
  request(input, timeoutMs) {
    const child = this.child;
    if (!child || child.killed || !child.stdin.writable) {
      return Promise.reject(new Error("Native streamer process is not running."));
    }
    const id = randomUUID();
    const payload = { ...input, id };
    return new Promise((resolveRequest, rejectRequest) => {
      const timeout = setTimeout(() => {
        this.pending.delete(id);
        rejectRequest(new Error(`Native streamer request "${input.type}" timed out after ${timeoutMs}ms.${this.formatStderrTail()}`));
      }, timeoutMs);
      timeout.unref?.();
      this.pending.set(id, {
        resolve: (message) => {
          clearTimeout(timeout);
          resolveRequest(message);
        },
        reject: (error) => {
          clearTimeout(timeout);
          rejectRequest(error);
        },
        timeout
      });
      child.stdin.write(`${JSON.stringify(payload)}
`, "utf8", (error) => {
        if (!error) {
          return;
        }
        const pending = this.pending.get(id);
        if (pending) {
          this.pending.delete(id);
          pending.reject(error);
        }
      });
    });
  }
  async flushSurfaceUpdate() {
    if (this.surfaceUpdateInFlight) {
      this.surfaceUpdateQueued = true;
      return;
    }
    while (this.child && this.lastSurface) {
      this.surfaceUpdateInFlight = true;
      this.surfaceUpdateQueued = false;
      const surface = this.lastSurface;
      try {
        await this.request({ type: "surface", surface }, SURFACE_UPDATE_TIMEOUT_MS);
      } catch (error) {
        console.warn("[NativeStreamer] Failed to update native render surface:", error);
        break;
      } finally {
        this.surfaceUpdateInFlight = false;
      }
      if (!this.surfaceUpdateQueued || this.lastSurface === surface) {
        break;
      }
    }
  }
  handleStdout(chunk) {
    this.stdoutBuffer += chunk;
    const lines = this.stdoutBuffer.split(/\r?\n/);
    this.stdoutBuffer = lines.pop() ?? "";
    for (const line of lines) {
      const trimmed = line.trim();
      if (!trimmed) {
        continue;
      }
      this.handleLine(trimmed);
    }
  }
  handleLine(line) {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      console.log(`[NativeStreamer] ${line}`);
      return;
    }
    if (isResponse(message)) {
      this.handleResponse(message);
      return;
    }
    if (isEvent(message)) {
      this.handleEvent(message);
    }
  }
  handleResponse(message) {
    const pending = this.pending.get(message.id);
    if (!pending) {
      console.warn("[NativeStreamer] Ignoring response for unknown request:", message.id);
      return;
    }
    this.pending.delete(message.id);
    if (message.type === "error") {
      pending.reject(new Error(message.code ? `${message.code}: ${message.message}` : message.message));
      return;
    }
    pending.resolve(message);
  }
  handleEvent(message) {
    if (message.type === "log") {
      const text = `[NativeStreamer] ${message.message}`;
      if (message.level === "error") {
        console.error(text);
      } else if (message.level === "warn") {
        console.warn(text);
      } else {
        console.log(text);
      }
      this.options.emit({ type: "log", message: text });
      return;
    }
    if (message.type === "local-ice") {
      if (this.answerInFlight) {
        this.queuedLocalIce.push(message.candidate);
        return;
      }
      this.forwardLocalIce(message.candidate);
      return;
    }
    if (message.type === "input-ready") {
      console.log(`[NativeStreamer] Input protocol ready: v${message.protocolVersion}`);
      this.options.emit({ type: "native-input-ready", protocolVersion: message.protocolVersion });
      return;
    }
    if (message.type === "video-stall") {
      const formatAge = (value) => value === void 0 ? "n/a" : `${value}ms`;
      const stats = [
        `stall=${message.stallMs}ms`,
        `stage=${message.likelyStage ?? "unknown"}`,
        `encoded=${(message.encodedKbps ?? 0).toFixed(0)}kbps`,
        `decoded=${message.decodedFps.toFixed(1)}fps`,
        `sink=${message.sinkFps.toFixed(1)}fps`,
        `requestedFps=${message.requestedFps ?? "n/a"}`,
        `capsFramerate=${message.capsFramerate ?? "n/a"}`,
        `queueMode=${message.queueMode ?? "unknown"}`,
        `partialFlushes=${message.partialFlushCount ?? 0}`,
        `completeFlushes=${message.completeFlushCount ?? 0}`,
        `lastTransition=${message.lastTransitionType ?? "none"}`,
        `ages=encoded:${formatAge(message.encodedAgeMs)} decoded:${formatAge(message.decodedAgeMs)} sink:${formatAge(message.sinkAgeMs)}`,
        `rendered=${message.sinkRendered ?? "n/a"}`,
        `dropped=${message.sinkDropped ?? "n/a"}`,
        `memory=${message.memoryMode ?? "unknown"}`,
        `zeroCopy=${message.zeroCopy ?? "unknown"}`,
        `zeroCopyD3D11=${message.zeroCopyD3D11}`,
        `zeroCopyD3D12=${message.zeroCopyD3D12}`
      ].join(" ");
      console.warn(`[NativeStreamer] Video stall recovery attempt ${message.recoveryAttempt}: ${stats}`);
      this.options.emit({
        type: "log",
        message: `[NativeStreamer] Video stall recovery attempt ${message.recoveryAttempt}: ${stats}`
      });
      void this.options.requestKeyframe({
        reason: "native-video-stall",
        backlogFrames: 0,
        attempt: message.recoveryAttempt
      }).catch((error) => {
        console.warn("[NativeStreamer] Failed to request video keyframe after stall:", error);
      });
      return;
    }
    if (message.type === "video-transition") {
      const transition = message.transition;
      const summary = transition.summary ?? `${transition.transitionType} @ ${transition.atMs}ms`;
      console.warn(`[NativeStreamer] Video transition: ${summary}`);
      this.options.emit({
        type: "native-stream-transition",
        transition
      });
      this.options.emit({
        type: "log",
        message: `[NativeStreamer] Video transition: ${summary}`
      });
      return;
    }
    if (message.type === "stats") {
      this.options.emit({
        type: "native-stream-stats",
        stats: message.stats
      });
      return;
    }
    if (message.type === "status") {
      console.log(`[NativeStreamer] Status: ${message.status}${message.message ? ` (${message.message})` : ""}`);
      if (message.status === "streaming") {
        this.options.emit({ type: "native-stream-started", message: message.message });
      } else if (message.status === "stopped") {
        this.options.emit({ type: "native-stream-stopped", reason: message.message });
      }
      return;
    }
    if (message.type === "error") {
      this.options.emit({ type: "error", message: `Native streamer error: ${message.message}` });
    }
  }
  handleProcessExit(reason) {
    if (!this.child) {
      return;
    }
    const tail = this.formatStderrTail();
    const hadActiveSession = this.activeSessionId !== null;
    const stoppedReason = `process ended (${reason})`;
    console.warn(`[NativeStreamer] Process ended (${reason})${tail}`);
    this.child = null;
    this.stdoutBuffer = "";
    this.stderrTail = [];
    this.activeSessionId = null;
    this.capabilities = null;
    this.clearQueuedRemoteIce();
    this.rejectPending(new Error(`Native streamer process ended (${reason}).${tail}`));
    if (hadActiveSession) {
      this.options.emit({ type: "native-stream-stopped", reason: stoppedReason });
      this.options.emit({ type: "error", message: `Native streamer ${stoppedReason}.${tail}` });
    }
  }
  appendStderr(line) {
    this.stderrTail.push(line);
    if (this.stderrTail.length > 12) this.stderrTail.shift();
  }
  formatStderrTail() {
    return this.stderrTail.length > 0 ? ` Recent stderr: ${this.stderrTail.join(" | ")}` : "";
  }
  rejectPending(error) {
    for (const [id, pending] of this.pending.entries()) {
      clearTimeout(pending.timeout);
      pending.reject(error);
      this.pending.delete(id);
    }
  }
  async flushQueuedLocalIce() {
    const queued = this.queuedLocalIce;
    this.queuedLocalIce = [];
    for (const candidate of queued) {
      await this.forwardLocalIce(candidate);
    }
  }
  prepareRemoteIceQueue(sessionId) {
    if (this.queuedRemoteIceSessionId !== null && this.queuedRemoteIceSessionId !== sessionId) {
      this.clearQueuedRemoteIce();
    }
    this.queuedRemoteIceSessionId = sessionId;
  }
  queueRemoteIce(sessionId, candidate) {
    this.prepareRemoteIceQueue(sessionId);
    this.queuedRemoteIce.push(candidate);
  }
  clearQueuedRemoteIce() {
    this.queuedRemoteIceSessionId = null;
    this.queuedRemoteIce = [];
  }
  async flushQueuedRemoteIce(sessionId) {
    const queued = this.drainQueuedRemoteIce(sessionId);
    for (const candidate of queued) {
      await this.sendRemoteIce(candidate);
    }
  }
  async sendRemoteIce(candidate) {
    await this.request({
      type: "remote-ice",
      candidate
    }, CONTROL_TIMEOUT_MS);
  }
  async forwardLocalIce(candidate) {
    try {
      await this.options.sendIceCandidate(candidate);
    } catch (error) {
      console.warn("[NativeStreamer] Failed to forward local ICE candidate:", error);
    }
  }
  terminateProcess() {
    const child = this.child;
    if (!child) {
      return;
    }
    this.child = null;
    try {
      child.kill();
    } catch (error) {
      console.warn("[NativeStreamer] Failed to terminate process:", error);
    }
  }
}
const MAX_NATIVE_INPUT_PACKET_BYTES = 4096;
function normalizeNativeInputPacket(input) {
  if (!input || typeof input !== "object") {
    return null;
  }
  const packet = input;
  const rawPayload = packet.payload;
  let bytes;
  if (rawPayload instanceof ArrayBuffer) {
    bytes = new Uint8Array(rawPayload);
  } else if (ArrayBuffer.isView(rawPayload)) {
    bytes = new Uint8Array(
      rawPayload.buffer,
      rawPayload.byteOffset,
      rawPayload.byteLength
    );
  } else if (Array.isArray(rawPayload)) {
    if (rawPayload.length === 0 || rawPayload.length > MAX_NATIVE_INPUT_PACKET_BYTES || rawPayload.some(
      (byte) => !Number.isInteger(byte) || byte < 0 || byte > 255
    )) {
      return null;
    }
    bytes = Uint8Array.from(rawPayload);
  } else {
    return null;
  }
  if (bytes.byteLength === 0 || bytes.byteLength > MAX_NATIVE_INPUT_PACKET_BYTES) {
    return null;
  }
  return {
    payloadBase64: Buffer.from(
      bytes.buffer,
      bytes.byteOffset,
      bytes.byteLength
    ).toString("base64"),
    partiallyReliable: packet.partiallyReliable === true
  };
}
function nativeWindowHandleToHex(window) {
  const handle = window.getNativeWindowHandle();
  if (handle.byteLength >= 8) {
    return `0x${handle.readBigUInt64LE(0).toString(16)}`;
  }
  if (handle.byteLength >= 4) {
    return `0x${handle.readUInt32LE(0).toString(16)}`;
  }
  return null;
}
function normalizeNativeRenderSurface(window, input) {
  if (!input || typeof input !== "object") {
    return null;
  }
  const windowHandle = nativeWindowHandleToHex(window);
  if (!windowHandle) {
    return null;
  }
  const deviceScaleFactor = Number.isFinite(input.deviceScaleFactor) ? Math.min(8, Math.max(0.25, input.deviceScaleFactor)) : 1;
  const rect = input.rect;
  const visible = input.visible === true && rect !== null && Number.isFinite(rect.x) && Number.isFinite(rect.y) && Number.isFinite(rect.width) && Number.isFinite(rect.height) && rect.width >= 2 && rect.height >= 2;
  return {
    windowHandle,
    deviceScaleFactor,
    visible,
    showStats: input.showStats === true,
    rect: visible ? {
      x: Math.round(rect.x),
      y: Math.round(rect.y),
      width: Math.max(2, Math.round(rect.width)),
      height: Math.max(2, Math.round(rect.height))
    } : null
  };
}
class SignalingCoordinator {
  constructor(deps) {
    this.deps = deps;
  }
  deps;
  signalingClient = null;
  signalingClientKey = null;
  nativeStreamerManager = null;
  nativeStreamerContext = null;
  nativeStreamerFallbackSessionId = null;
  registerIpcHandlers() {
    const { ipcMain: ipcMain2 } = this.deps;
    ipcMain2.handle(
      IPC_CHANNELS.CONNECT_SIGNALING,
      async (_event, payload) => {
        await this.connectSignaling(payload);
      }
    );
    ipcMain2.handle(IPC_CHANNELS.DISCONNECT_SIGNALING, async () => {
      await this.disconnectSignaling();
    });
    ipcMain2.handle(
      IPC_CHANNELS.SEND_ANSWER,
      async (_event, payload) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        return this.signalingClient.sendAnswer(payload);
      }
    );
    ipcMain2.handle(
      IPC_CHANNELS.SEND_ICE_CANDIDATE,
      async (_event, payload) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        return this.signalingClient.sendIceCandidate(payload);
      }
    );
    ipcMain2.on(
      IPC_CHANNELS.NATIVE_INPUT,
      (_event, payload) => {
        if (!this.isNativeStreamerSelected()) {
          return;
        }
        const context = this.nativeStreamerContext;
        if (!context || this.nativeStreamerFallbackSessionId === context.session.sessionId) {
          return;
        }
        const packet = normalizeNativeInputPacket(payload);
        if (!packet) {
          return;
        }
        this.nativeStreamerManager?.sendInput(packet);
      }
    );
    ipcMain2.on(
      IPC_CHANNELS.NATIVE_RENDER_SURFACE,
      (event, payload) => {
        if (!this.isNativeStreamerSelected()) {
          return;
        }
        const window = BrowserWindow.fromWebContents(event.sender);
        if (!window || window.isDestroyed()) {
          return;
        }
        const surface = normalizeNativeRenderSurface(window, payload);
        if (!surface) {
          return;
        }
        this.getNativeStreamerManager().updateSurface(surface);
      }
    );
    ipcMain2.handle(
      IPC_CHANNELS.REQUEST_KEYFRAME,
      async (_event, payload) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        return this.signalingClient.requestKeyframe(payload);
      }
    );
    ipcMain2.handle(
      IPC_CHANNELS.NATIVE_STREAMER_STATUS,
      async () => {
        return this.getNativeStreamerManager().probeStatus();
      }
    );
    ipcMain2.handle(IPC_CHANNELS.NATIVE_CLOUD_GSYNC_CAPABILITIES, async () => {
      const capabilities = await getNativeCloudGsyncCapabilities(
        this.deps.settingsManager?.get("nativeCloudGsyncMode") ?? "auto"
      );
      console.log(
        `[CloudGsync] capability probe: ${JSON.stringify(capabilities)}`
      );
      return capabilities;
    });
  }
  disconnectForShutdown(options) {
    if (options.emitDisconnectEvent) {
      this.signalingClient?.disconnect();
    }
    this.signalingClient = null;
    this.signalingClientKey = null;
    this.nativeStreamerManager?.dispose(options.reason);
    this.nativeStreamerManager = null;
    this.nativeStreamerContext = null;
    this.nativeStreamerFallbackSessionId = null;
  }
  stopNativeStreamer(reason) {
    void this.nativeStreamerManager?.stop(reason);
  }
  resetNativeStreamerContext() {
    this.nativeStreamerContext = null;
    this.nativeStreamerFallbackSessionId = null;
  }
  nativeStreamerHasActiveSession() {
    return this.nativeStreamerManager?.hasActiveSession() ?? false;
  }
  updateNativeStreamerBitrateSetting(value) {
    const maxBitrateMbps = normalizeMaxBitrateMbps(value);
    if (maxBitrateMbps === null) {
      return;
    }
    if (this.nativeStreamerContext) {
      this.nativeStreamerContext = {
        ...this.nativeStreamerContext,
        settings: {
          ...this.nativeStreamerContext.settings,
          maxBitrateMbps
        }
      };
    }
    this.nativeStreamerManager?.updateBitrateLimit(maxBitrateMbps * 1e3);
  }
  applySettingsChange(key, value) {
    if (key === "streamClientMode" && value !== "native" || key === "nativeStreamerBackend" || key === "nativeStreamerExecutablePath" || key === "nativeCloudGsyncMode" || key === "nativeD3dFullscreenMode" || key === "nativeExternalRenderer") {
      this.stopNativeStreamer(
        key === "nativeStreamerBackend" ? "native streamer backend changed" : key === "nativeStreamerExecutablePath" ? "native streamer executable changed" : key === "nativeCloudGsyncMode" ? "native Cloud G-Sync mode changed" : key === "nativeD3dFullscreenMode" ? "native D3D fullscreen mode changed" : key === "nativeExternalRenderer" ? "native external renderer setting changed" : "native streamer disabled"
      );
      this.resetNativeStreamerContext();
    }
    if (key === "nativeVideoBackend") {
      if (this.nativeStreamerHasActiveSession()) {
        console.log(
          "[NativeStreamer] Native video backend changed; active session will keep its current backend until the next native streamer restart."
        );
      } else {
        this.stopNativeStreamer("native video backend changed");
      }
    }
    if (key === "maxBitrateMbps") {
      this.updateNativeStreamerBitrateSetting(value);
    }
  }
  async connectSignaling(payload) {
    const nextKey = `${payload.sessionId}|${payload.signalingServer}|${payload.signalingUrl ?? ""}`;
    this.nativeStreamerContext = payload.nativeStreamer ?? null;
    this.nativeStreamerFallbackSessionId = null;
    if (this.nativeStreamerContext) {
      console.log(
        "[NativeStreamer] Signaling connect context:",
        JSON.stringify({
          sessionId: this.nativeStreamerContext.session.sessionId,
          resolution: this.nativeStreamerContext.settings.resolution,
          fps: this.nativeStreamerContext.settings.fps,
          codec: this.nativeStreamerContext.settings.codec,
          negotiatedStreamProfile: this.nativeStreamerContext.session.negotiatedStreamProfile,
          requestedStreamingFeatures: this.nativeStreamerContext.session.requestedStreamingFeatures,
          finalizedStreamingFeatures: this.nativeStreamerContext.session.finalizedStreamingFeatures
        })
      );
    }
    if (this.signalingClient && this.signalingClientKey === nextKey) {
      console.log(
        "[Signaling] Reuse existing signaling connection (duplicate connect request ignored)"
      );
      return;
    }
    if (this.signalingClient) {
      this.signalingClient.disconnect();
    }
    await this.resetNativeStreamerForSignalingReconnect();
    await this.prepareNativeStreamerBeforeSignaling();
    this.signalingClient = new GfnSignalingClient(
      payload.signalingServer,
      payload.sessionId,
      payload.signalingUrl
    );
    this.signalingClientKey = nextKey;
    this.signalingClient.onEvent((event) => this.routeSignalingEvent(event));
    try {
      await this.signalingClient.connect();
    } catch (error) {
      await this.nativeStreamerManager?.stop("signaling connect failed").catch(() => void 0);
      this.signalingClient = null;
      this.signalingClientKey = null;
      throw error;
    }
  }
  async disconnectSignaling() {
    await this.nativeStreamerManager?.stop("signaling disconnect");
    this.nativeStreamerContext = null;
    this.nativeStreamerFallbackSessionId = null;
    this.signalingClient?.disconnect();
    this.signalingClient = null;
    this.signalingClientKey = null;
  }
  emitToRenderer(event) {
    const mainWindow2 = this.deps.getMainWindow();
    if (mainWindow2 && !mainWindow2.isDestroyed()) {
      mainWindow2.webContents.send(IPC_CHANNELS.SIGNALING_EVENT, event);
    }
  }
  getNativeStreamerManager() {
    this.nativeStreamerManager ??= new NativeStreamerManager({
      mainDir: this.deps.mainDir,
      getBackendPreference: () => "gstreamer",
      getVideoBackendPreference: () => this.deps.settingsManager?.get("nativeVideoBackend") ?? "auto",
      getExecutablePathOverride: () => this.deps.settingsManager?.get("nativeStreamerExecutablePath") ?? "",
      getCloudGsyncMode: () => this.deps.settingsManager?.get("nativeCloudGsyncMode") ?? "auto",
      getD3dFullscreenMode: () => this.deps.settingsManager?.get("nativeD3dFullscreenMode") ?? "auto",
      getExternalRendererEnabled: () => true,
      emit: (event) => this.emitToRenderer(event),
      sendAnswer: async (payload) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        await this.signalingClient.sendAnswer(payload);
      },
      sendIceCandidate: async (candidate) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        await this.signalingClient.sendIceCandidate(candidate);
      },
      requestKeyframe: async (payload) => {
        if (!this.signalingClient) {
          throw new Error("Signaling is not connected");
        }
        await this.signalingClient.requestKeyframe(payload);
      }
    });
    return this.nativeStreamerManager;
  }
  isNativeStreamerSelected() {
    return this.deps.settingsManager?.get("streamClientMode") === "native";
  }
  routeSignalingEvent(event) {
    if (event.type === "disconnected") {
      void this.nativeStreamerManager?.stop(
        `signaling disconnected: ${event.reason}`
      );
      this.nativeStreamerContext = null;
      this.nativeStreamerFallbackSessionId = null;
      this.emitToRenderer(event);
      return;
    }
    const context = this.nativeStreamerContext;
    const nativeFallbackActive = context !== null && this.nativeStreamerFallbackSessionId === context.session.sessionId;
    if (!this.isNativeStreamerSelected() || !context || nativeFallbackActive) {
      this.emitToRenderer(event);
      return;
    }
    if (event.type === "offer") {
      void this.handleNativeStreamerOffer(event.sdp, context);
      return;
    }
    if (event.type === "remote-ice") {
      void this.getNativeStreamerManager().addRemoteIce(event.candidate, context).catch((error) => {
        this.emitToRenderer({
          type: "error",
          message: `Native streamer ICE failed: ${String(error)}`
        });
      });
      return;
    }
    this.emitToRenderer(event);
  }
  async handleNativeStreamerOffer(sdp, context) {
    try {
      await this.getNativeStreamerManager().handleOffer(sdp, context);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.warn("[NativeStreamer] Falling back to web streamer:", message);
      this.nativeStreamerFallbackSessionId = context.session.sessionId;
      const queuedRemoteIce = this.nativeStreamerManager?.drainQueuedRemoteIce(
        context.session.sessionId
      ) ?? [];
      await this.nativeStreamerManager?.stop("native streamer fallback").catch(() => void 0);
      this.emitToRenderer({
        type: "error",
        message: `Native streamer failed: ${message}. Falling back to web streamer.`
      });
      this.emitToRenderer({ type: "offer", sdp });
      for (const candidate of queuedRemoteIce) {
        this.emitToRenderer({ type: "remote-ice", candidate });
      }
    }
  }
  async resetNativeStreamerForSignalingReconnect() {
    if (!this.nativeStreamerManager) {
      return;
    }
    if (!this.isNativeStreamerSelected() || !this.nativeStreamerContext || this.nativeStreamerManager.hasActiveSession()) {
      await this.nativeStreamerManager.stop("signaling reconnect");
    }
  }
  async prepareNativeStreamerBeforeSignaling() {
    const context = this.nativeStreamerContext;
    if (!this.isNativeStreamerSelected() || !context) {
      return;
    }
    try {
      this.emitToRenderer({
        type: "log",
        message: "Preparing native streamer before signaling attach."
      });
      await this.getNativeStreamerManager().prepareForSession(context);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.warn(
        "[NativeStreamer] Pre-attach startup failed; falling back to web streamer:",
        message
      );
      this.nativeStreamerFallbackSessionId = context.session.sessionId;
      await this.nativeStreamerManager?.stop("native streamer pre-attach fallback").catch(() => void 0);
      this.emitToRenderer({
        type: "error",
        message: `Native streamer failed before signaling attach: ${message}. Falling back to web streamer.`
      });
    }
  }
}
function registerSignalingIpcHandlers(deps) {
  const coordinator = new SignalingCoordinator(deps);
  coordinator.registerIpcHandlers();
  return coordinator;
}
function normalizeMaxBitrateMbps(value) {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    return null;
  }
  return Math.min(150, Math.max(5, Math.round(value)));
}
function withTimeout(promise, timeoutMs, label) {
  return new Promise((resolve2, reject) => {
    const timeout = setTimeout(
      () => reject(new Error(`${label} timed out after ${timeoutMs}ms`)),
      timeoutMs
    );
    promise.then(
      (value) => {
        clearTimeout(timeout);
        resolve2(value);
      },
      (error) => {
        clearTimeout(timeout);
        reject(error);
      }
    );
  });
}
async function fetchWithTimeout(url, init, timeoutMs, label) {
  const controller = new AbortController();
  const timeout = setTimeout(
    () => controller.abort(new Error(`${label} timed out after ${timeoutMs}ms`)),
    timeoutMs
  );
  try {
    return await fetch(url, {
      ...init,
      signal: controller.signal
    });
  } catch (error) {
    if (error instanceof Error && error.name === "AbortError" || controller.signal.aborted) {
      const reason = controller.signal.reason;
      const message = reason instanceof Error ? reason.message : `${label} timed out after ${timeoutMs}ms`;
      throw new Error(message);
    }
    throw error;
  } finally {
    clearTimeout(timeout);
  }
}
const PRINTEDWASTE_TIMEOUT_MS = 7e3;
const PRINTEDWASTE_QUEUE_URL = "https://api.printedwaste.com/gfn/queue/";
const PRINTEDWASTE_SERVER_MAPPING_URL = "https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING";
async function fetchPrintedWasteQueue(appVersion) {
  const response = await fetchWithTimeout(
    PRINTEDWASTE_QUEUE_URL,
    {
      headers: {
        "User-Agent": `opennow/${appVersion}`,
        Accept: "application/json"
      }
    },
    PRINTEDWASTE_TIMEOUT_MS,
    "PrintedWaste queue request"
  );
  if (!response.ok) {
    throw new Error(`PrintedWaste API returned HTTP ${response.status}`);
  }
  const body = await withTimeout(
    response.json(),
    PRINTEDWASTE_TIMEOUT_MS,
    "PrintedWaste queue response parse"
  );
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    throw new Error("PrintedWaste API response was not an object");
  }
  const apiBody = body;
  if (typeof apiBody.status !== "boolean") {
    throw new Error("PrintedWaste API response missing boolean status");
  }
  if (!apiBody.status) {
    throw new Error("PrintedWaste API returned status:false");
  }
  if (!apiBody.data || typeof apiBody.data !== "object" || Array.isArray(apiBody.data)) {
    throw new Error("PrintedWaste API response missing data object");
  }
  const normalizedData = {};
  for (const [zoneId, rawZone] of Object.entries(
    apiBody.data
  )) {
    if (!rawZone || typeof rawZone !== "object" || Array.isArray(rawZone)) {
      continue;
    }
    const zone = rawZone;
    const queuePosition = zone.QueuePosition;
    const lastUpdated = zone["Last Updated"];
    const region = zone.Region;
    const eta = zone.eta;
    if (typeof queuePosition !== "number" || !Number.isFinite(queuePosition)) {
      continue;
    }
    if (typeof lastUpdated !== "number" || !Number.isFinite(lastUpdated)) {
      continue;
    }
    if (typeof region !== "string" || region.length === 0) {
      continue;
    }
    if (eta !== void 0 && (typeof eta !== "number" || !Number.isFinite(eta))) {
      continue;
    }
    normalizedData[zoneId] = {
      QueuePosition: queuePosition,
      "Last Updated": lastUpdated,
      Region: region,
      ...typeof eta === "number" ? { eta } : {}
    };
  }
  if (Object.keys(normalizedData).length === 0) {
    throw new Error("PrintedWaste API returned no valid zones");
  }
  return normalizedData;
}
async function fetchPrintedWasteServerMapping(appVersion) {
  const response = await fetchWithTimeout(
    PRINTEDWASTE_SERVER_MAPPING_URL,
    {
      headers: {
        "User-Agent": `opennow/${appVersion}`,
        Accept: "application/json"
      }
    },
    PRINTEDWASTE_TIMEOUT_MS,
    "PrintedWaste server mapping request"
  );
  if (!response.ok) {
    throw new Error(
      `PrintedWaste server mapping returned HTTP ${response.status}`
    );
  }
  const body = await withTimeout(
    response.json(),
    PRINTEDWASTE_TIMEOUT_MS,
    "PrintedWaste server mapping response parse"
  );
  if (!body || typeof body !== "object" || Array.isArray(body)) {
    throw new Error("PrintedWaste server mapping response was not an object");
  }
  const apiBody = body;
  if (typeof apiBody.status !== "boolean") {
    throw new Error(
      "PrintedWaste server mapping response missing boolean status"
    );
  }
  if (!apiBody.status) {
    throw new Error("PrintedWaste server mapping returned status:false");
  }
  if (!apiBody.data || typeof apiBody.data !== "object" || Array.isArray(apiBody.data)) {
    throw new Error(
      "PrintedWaste server mapping response missing data object"
    );
  }
  const normalizedData = {};
  for (const [zoneId, rawZone] of Object.entries(
    apiBody.data
  )) {
    if (!rawZone || typeof rawZone !== "object" || Array.isArray(rawZone)) {
      continue;
    }
    const zone = rawZone;
    const title = zone.title;
    const region = zone.region;
    const is4080Server = zone.is4080Server;
    const is5080Server = zone.is5080Server;
    const nuked = zone.nuked;
    normalizedData[zoneId] = {
      ...typeof title === "string" ? { title } : {},
      ...typeof region === "string" ? { region } : {},
      ...typeof is4080Server === "boolean" ? { is4080Server } : {},
      ...typeof is5080Server === "boolean" ? { is5080Server } : {},
      ...typeof nuked === "boolean" ? { nuked } : {}
    };
  }
  return normalizedData;
}
async function tcpPing(hostname, port, timeoutMs = 3e3) {
  return new Promise((resolve2) => {
    const startTime = Date.now();
    const socket = new net.Socket();
    socket.setTimeout(timeoutMs);
    socket.once("connect", () => {
      const pingMs = Date.now() - startTime;
      socket.destroy();
      resolve2(pingMs);
    });
    socket.once("timeout", () => {
      socket.destroy();
      resolve2(null);
    });
    socket.once("error", () => {
      socket.destroy();
      resolve2(null);
    });
    socket.connect(port, hostname);
  });
}
async function pingRegions(regions) {
  const pingPromises = regions.map(async (region) => {
    try {
      const url = new URL(region.url);
      const hostname = url.hostname;
      const port = url.protocol === "https:" ? 443 : 80;
      const validPings = [];
      await tcpPing(hostname, port, 3e3);
      for (let i = 0; i < 3; i++) {
        if (i > 0) {
          await new Promise((resolve2) => setTimeout(resolve2, 100));
        }
        const pingMs = await tcpPing(hostname, port, 3e3);
        if (pingMs !== null) {
          validPings.push(pingMs);
        }
      }
      if (validPings.length > 0) {
        const avgPing = Math.round(
          validPings.reduce((a, b) => a + b, 0) / validPings.length
        );
        return { url: region.url, pingMs: avgPing };
      } else {
        return {
          url: region.url,
          pingMs: null,
          error: "All ping tests failed"
        };
      }
    } catch {
      return {
        url: region.url,
        pingMs: null,
        error: "Invalid URL"
      };
    }
  });
  return Promise.all(pingPromises);
}
const __filename$1 = fileURLToPath(import.meta.url);
const __dirname$1 = dirname(__filename$1);
function isAccelerationPreference(value) {
  return value === "auto" || value === "hardware" || value === "software";
}
function loadBootstrapVideoPreferences() {
  const defaults = {
    decoderPreference: "auto",
    encoderPreference: "auto"
  };
  try {
    const settingsPath = join(app.getPath("userData"), "settings.json");
    if (!existsSync(settingsPath)) {
      return defaults;
    }
    const parsed = JSON.parse(
      readFileSync(settingsPath, "utf-8")
    );
    return {
      decoderPreference: isAccelerationPreference(parsed.decoderPreference) ? parsed.decoderPreference : defaults.decoderPreference,
      encoderPreference: isAccelerationPreference(parsed.encoderPreference) ? parsed.encoderPreference : defaults.encoderPreference
    };
  } catch {
    return defaults;
  }
}
const bootstrapVideoPrefs = loadBootstrapVideoPreferences();
console.log(
  `[Main] Video acceleration preference: decode=${bootstrapVideoPrefs.decoderPreference}, encode=${bootstrapVideoPrefs.encoderPreference}`
);
const platformFeatures = [];
const isLinuxArm = process.platform === "linux" && (process.arch === "arm64" || process.arch === "arm");
if (process.platform === "win32") {
  if (bootstrapVideoPrefs.decoderPreference !== "software") {
    platformFeatures.push("D3D11VideoDecoder");
  }
  if (bootstrapVideoPrefs.decoderPreference !== "software" || bootstrapVideoPrefs.encoderPreference !== "software") {
    platformFeatures.push("MediaFoundationD3D11VideoCapture");
  }
} else if (process.platform === "linux") {
  if (isLinuxArm) {
    if (bootstrapVideoPrefs.decoderPreference !== "software") {
      platformFeatures.push("UseChromeOSDirectVideoDecoder");
    }
  } else {
    if (bootstrapVideoPrefs.decoderPreference !== "software") {
      platformFeatures.push("VaapiVideoDecoder");
    }
    if (bootstrapVideoPrefs.encoderPreference !== "software") {
      platformFeatures.push("VaapiVideoEncoder");
    }
    if (bootstrapVideoPrefs.decoderPreference !== "software" || bootstrapVideoPrefs.encoderPreference !== "software") {
      platformFeatures.push("VaapiIgnoreDriverChecks");
    }
  }
}
app.commandLine.appendSwitch(
  "enable-features",
  [
    // --- MP4 recording via MediaRecorder (Chromium 127+) ---
    "MediaRecorderEnableMp4Muxer",
    // --- AV1 support (cross-platform) ---
    "Dav1dVideoDecoder",
    // Fast AV1 software fallback via dav1d (if no HW decoder)
    // --- Additional (cross-platform) ---
    "HardwareMediaKeyHandling",
    // --- Platform-specific HW decode/encode ---
    ...platformFeatures
  ].join(",")
);
const disableFeatures = [
  // Prevents mDNS candidate generation — faster ICE connectivity
  "WebRtcHideLocalIpsWithMdns"
];
if (process.platform === "linux" && !isLinuxArm) {
  disableFeatures.push("UseChromeOSDirectVideoDecoder");
}
app.commandLine.appendSwitch("disable-features", disableFeatures.join(","));
app.commandLine.appendSwitch(
  "force-fieldtrials",
  [
    // Disable send-side pacing — we are receive-only, pacing adds latency to RTCP feedback
    "WebRTC-Video-Pacing/Disabled/"
  ].join("/")
);
if (bootstrapVideoPrefs.decoderPreference === "hardware") {
  app.commandLine.appendSwitch("enable-accelerated-video-decode");
} else if (bootstrapVideoPrefs.decoderPreference === "software") {
  app.commandLine.appendSwitch("disable-accelerated-video-decode");
}
if (bootstrapVideoPrefs.encoderPreference === "hardware") {
  app.commandLine.appendSwitch("enable-accelerated-video-encode");
} else if (bootstrapVideoPrefs.encoderPreference === "software") {
  app.commandLine.appendSwitch("disable-accelerated-video-encode");
}
app.commandLine.appendSwitch("ignore-gpu-blocklist");
app.commandLine.appendSwitch("disable-renderer-backgrounding");
app.commandLine.appendSwitch("disable-backgrounding-occluded-windows");
app.commandLine.appendSwitch("max-gum-fps", "999");
protocol.registerSchemesAsPrivileged([
  {
    scheme: "opennow-media",
    privileges: {
      standard: true,
      secure: true,
      supportFetchAPI: true,
      stream: true,
      corsEnabled: true
    }
  }
]);
let mainWindow = null;
let rendererControlledFullscreen = false;
let signalingCoordinator = null;
let authService;
let settingsManager;
let appUpdater = null;
const EXPLICIT_SHUTDOWN_FORCE_EXIT_DELAY_MS = 2e3;
let isShutdownRequested = false;
let isShutdownCleanupComplete = false;
let isUpdaterInstallQuitInProgress = false;
let explicitShutdownFallbackTimer = null;
let isPointerLockActiveRuntime = false;
function clearExplicitShutdownFallback() {
  if (explicitShutdownFallbackTimer) {
    clearTimeout(explicitShutdownFallbackTimer);
    explicitShutdownFallbackTimer = null;
  }
}
function runShutdownCleanup(reason = "app-quit") {
  if (isShutdownCleanupComplete) {
    return;
  }
  isShutdownCleanupComplete = true;
  console.log(`[Main] Running shutdown cleanup (${reason})`);
  refreshScheduler.stop();
  const shouldSkipExplicitSignalingDisconnect = reason === "renderer-explicit-exit" || reason === "app-quit" || reason === "before-quit" || reason === "window-all-closed";
  signalingCoordinator?.disconnectForShutdown({
    emitDisconnectEvent: !shouldSkipExplicitSignalingDisconnect,
    reason
  });
  signalingCoordinator = null;
  void destroyDiscordRpc();
  appUpdater?.dispose();
  appUpdater = null;
  const windowToClose = mainWindow;
  if (windowToClose && !windowToClose.isDestroyed()) {
    mainWindow = null;
    try {
      windowToClose.close();
    } catch (error) {
      console.warn(
        "[Main] Failed to close main window during shutdown:",
        error
      );
    }
    if (!windowToClose.isDestroyed()) {
      try {
        windowToClose.destroy();
      } catch (error) {
        console.warn(
          "[Main] Failed to destroy main window during shutdown:",
          error
        );
      }
    }
  }
}
function scheduleExplicitShutdownFallback(reason, exitCode = 0) {
  if (explicitShutdownFallbackTimer || isUpdaterInstallQuitInProgress) {
    return;
  }
  explicitShutdownFallbackTimer = setTimeout(() => {
    explicitShutdownFallbackTimer = null;
    console.warn(
      `[Main] Explicit shutdown fallback triggered (${reason}); forcing process exit.`
    );
    app.exit(exitCode);
  }, EXPLICIT_SHUTDOWN_FORCE_EXIT_DELAY_MS);
  explicitShutdownFallbackTimer.unref?.();
}
function requestAppShutdown(options = {}) {
  const {
    reason = "app-quit",
    forceExitFallback = false,
    exitCode = 0
  } = options;
  if (!isShutdownRequested) {
    isShutdownRequested = true;
    discordMonitor.stop();
    runShutdownCleanup(reason);
  }
  if (forceExitFallback) {
    scheduleExplicitShutdownFallback(reason, exitCode);
  }
  app.quit();
}
class DiscordStatusMonitor {
  timer = null;
  intervalMs = 60 * 1e3;
  isSyncing = false;
  hasPerformedInitialSync = false;
  start() {
    if (this.timer) return;
    this.timer = setInterval(() => void this.sync(), this.intervalMs);
    void this.sync();
  }
  stop() {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = null;
    }
  }
  async sync() {
    if (this.isSyncing) return;
    this.isSyncing = true;
    try {
      if (!settingsManager.get("discordRichPresence")) {
        this.stop();
        void clearActivity();
        return;
      }
      if (!isDiscordRpcConnected()) {
        await connectDiscordRpc().catch(() => {
        });
      }
      if (!this.hasPerformedInitialSync) {
        console.log("[DiscordRPC] Startup: clearing any stale Discord status.");
        await clearActivity().catch(() => {
        });
        this.hasPerformedInitialSync = true;
      }
      const token = await resolveJwt().catch(() => null);
      if (!token) return;
      const provider = authService.getSelectedProvider();
      const streamingBaseUrl = provider.streamingServiceUrl;
      const activeSessions = await getActiveSessions(
        token,
        streamingBaseUrl
      ).catch(() => []);
      const activeSession = activeSessions.find(
        (s) => [1, 2, 3].includes(s.status)
      );
      const currentActivity = getCurrentActivity();
      if (activeSession) {
        const sessionAppId = activeSession.appId.toString();
        if (!currentActivity || currentActivity.appId !== sessionAppId) {
          const title = sessionAppId;
          const startTime = /* @__PURE__ */ new Date();
          void setActivity(title, startTime, sessionAppId);
        }
      } else if (currentActivity) {
        console.log("[DiscordRPC] Monitor clearing stale status.");
        void clearActivity();
      }
    } catch (err) {
      console.warn("[DiscordRPC] Monitor sync failed:", err.message);
    } finally {
      this.isSyncing = false;
    }
  }
}
const discordMonitor = new DiscordStatusMonitor();
function emitUpdaterStateToRenderer(state) {
  if (mainWindow && !mainWindow.isDestroyed()) {
    mainWindow.webContents.send(IPC_CHANNELS.APP_UPDATER_STATE_CHANGED, state);
  }
}
async function createMainWindow() {
  const preloadMjsPath = join(__dirname$1, "../preload/index.mjs");
  const preloadJsPath = join(__dirname$1, "../preload/index.js");
  const preloadPath = existsSync(preloadMjsPath) ? preloadMjsPath : preloadJsPath;
  const settings = settingsManager.getAll();
  mainWindow = new BrowserWindow({
    width: settings.windowWidth || 1400,
    height: settings.windowHeight || 900,
    minWidth: 1024,
    minHeight: 680,
    autoHideMenuBar: true,
    backgroundColor: "#0f172a",
    webPreferences: {
      preload: preloadPath,
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  });
  if (process.platform === "win32") {
    mainWindow.webContents.on("enter-html-full-screen", () => {
      if (mainWindow && !mainWindow.isDestroyed() && !mainWindow.isFullScreen()) {
        mainWindow.setFullScreen(true);
      }
    });
    mainWindow.webContents.on("leave-html-full-screen", () => {
      if (rendererControlledFullscreen) {
        return;
      }
      if (mainWindow && !mainWindow.isDestroyed() && mainWindow.isFullScreen()) {
        mainWindow.setFullScreen(false);
      }
    });
  }
  ipcMain.on(IPC_CHANNELS.POINTER_LOCK_CHANGE, (_ev, active) => {
    isPointerLockActiveRuntime = Boolean(active);
  });
  mainWindow.webContents.on("before-input-event", (event, input) => {
    try {
      if (input.type === "keyDown" && input.key === "Escape" && isPointerLockActiveRuntime && settingsManager && !settingsManager.get("allowEscapeToExitFullscreen")) {
        event.preventDefault();
        if (mainWindow && mainWindow.webContents) {
          mainWindow.webContents.send(IPC_CHANNELS.EXTERNAL_ESCAPE);
        }
      }
    } catch {
    }
  });
  if (process.env.ELECTRON_RENDERER_URL) {
    await mainWindow.loadURL(process.env.ELECTRON_RENDERER_URL);
  } else {
    await mainWindow.loadFile(join(__dirname$1, "../../dist/index.html"));
  }
  mainWindow.on("closed", () => {
    mainWindow = null;
    rendererControlledFullscreen = false;
  });
}
async function resolveJwt(token) {
  return authService.resolveJwtToken(token);
}
async function showSessionConflictDialog() {
  return showSessionConflictDialog$1({
    dialog,
    getMainWindow: () => mainWindow
  });
}
const THANKS_CONTRIBUTORS_URL = "https://api.github.com/repos/OpenCloudGaming/OpenNOW/contributors?per_page=100";
const THANKS_SUPPORTERS_URL = "https://github.com/sponsors/zortos293";
const THANKS_REQUEST_HEADERS = {
  Accept: "application/vnd.github+json",
  "User-Agent": "OpenNOW-DesktopClient"
};
const THANKS_EXCLUDED_PATTERN = /(copilot|claude|cappy)/i;
const THANKS_FETCH_TIMEOUT_MS = 8e3;
function decodeHtmlEntities(value) {
  return value.replace(/&amp;/g, "&").replace(/&quot;/g, '"').replace(/&#39;/g, "'").replace(/&lt;/g, "<").replace(/&gt;/g, ">");
}
function stripHtml(value) {
  return decodeHtmlEntities(value.replace(/<[^>]+>/g, " ")).replace(/\s+/g, " ").trim();
}
function normalizeUrl(value) {
  if (!value) return void 0;
  const decoded = decodeHtmlEntities(value.trim());
  if (!decoded) return void 0;
  if (decoded.startsWith("//")) return `https:${decoded}`;
  if (decoded.startsWith("/")) return `https://github.com${decoded}`;
  return decoded;
}
function shouldExcludeContributor(contributor) {
  const login = contributor.login?.trim() ?? "";
  const name = contributor.name?.trim() ?? "";
  if (!login || !contributor.avatar_url || !contributor.html_url) return true;
  if (contributor.type === "Bot") return true;
  if (/\[bot\]$/i.test(login)) return true;
  if (THANKS_EXCLUDED_PATTERN.test(login) || THANKS_EXCLUDED_PATTERN.test(name))
    return true;
  return false;
}
async function fetchThanksContributors() {
  const response = await fetchWithTimeout(
    THANKS_CONTRIBUTORS_URL,
    { headers: THANKS_REQUEST_HEADERS },
    THANKS_FETCH_TIMEOUT_MS,
    "GitHub contributors request"
  );
  if (!response.ok) {
    throw new Error(`GitHub contributors request failed (${response.status})`);
  }
  const payload = await withTimeout(
    response.json(),
    THANKS_FETCH_TIMEOUT_MS,
    "GitHub contributors response"
  );
  if (!Array.isArray(payload)) {
    throw new Error("GitHub contributors response was not an array");
  }
  const contributors = payload.filter((contributor) => !shouldExcludeContributor(contributor)).map((contributor) => ({
    login: contributor.login.trim(),
    avatarUrl: contributor.avatar_url,
    profileUrl: contributor.html_url,
    contributions: typeof contributor.contributions === "number" ? contributor.contributions : 0
  })).sort(
    (a, b) => b.contributions - a.contributions || a.login.localeCompare(b.login)
  );
  return contributors;
}
function parseSupporterName(entryHtml) {
  const privateHrefMatch = entryHtml.match(
    /href="https:\/\/docs\.github\.com\/sponsors\/sponsoring-open-source-contributors\/managing-your-sponsorship#managing-the-privacy-setting-for-your-sponsorship"/i
  );
  const privateTooltipMatch = entryHtml.match(
    /<tool-tip[^>]*>\s*Private Sponsor\s*<\/tool-tip>/i
  );
  const privateAriaMatch = entryHtml.match(/aria-label="Private Sponsor"/i);
  if (privateHrefMatch || privateTooltipMatch || privateAriaMatch) {
    return { name: "Private", isPrivate: true };
  }
  const altMatch = entryHtml.match(/<img[^>]+alt="([^"]+)"/i);
  const altText = altMatch ? stripHtml(altMatch[1]) : "";
  const normalizedAlt = altText.replace(/^@/, "").trim();
  if (normalizedAlt) {
    return { name: normalizedAlt, isPrivate: false };
  }
  const ariaMatch = entryHtml.match(/aria-label="([^"]+)"/i);
  const ariaText = ariaMatch ? stripHtml(ariaMatch[1]) : "";
  const normalizedAria = ariaText.replace(/^@/, "").trim();
  if (normalizedAria && !/private sponsor/i.test(normalizedAria)) {
    return { name: normalizedAria, isPrivate: false };
  }
  const hrefMatch = entryHtml.match(/<a[^>]+href="\/([^"/?#]+)"/i);
  const normalizedHref = hrefMatch ? decodeHtmlEntities(hrefMatch[1]).trim() : "";
  if (normalizedHref && !/sponsors/i.test(normalizedHref)) {
    return { name: normalizedHref.replace(/^@/, ""), isPrivate: false };
  }
  return { name: "Private", isPrivate: true };
}
function parseSupportersFromHtml(html) {
  const sponsorsSectionMatch = html.match(
    /<div class="tmp-mt-3 tmp-pb-4" id="sponsors">([\s\S]*?)<\/remote-pagination>/i
  );
  if (!sponsorsSectionMatch) {
    return [];
  }
  const listHtml = sponsorsSectionMatch[1];
  const entryMatches = listHtml.match(/<div class="d-flex mb-1 mr-1"[^>]*>[\s\S]*?<\/div>/gi) ?? [];
  const supporters = [];
  const seenKeys = /* @__PURE__ */ new Set();
  for (const entryHtml of entryMatches) {
    const { name, isPrivate } = parseSupporterName(entryHtml);
    const hrefMatch = entryHtml.match(/<a[^>]+href="([^"]+)"/i);
    const profileUrl = isPrivate ? void 0 : normalizeUrl(hrefMatch?.[1]);
    const avatarMatch = entryHtml.match(/<img[^>]+src="([^"]+)"/i);
    const avatarUrl = normalizeUrl(avatarMatch?.[1]);
    const dedupeKey = `${name}|${profileUrl ?? ""}|${avatarUrl ?? ""}`;
    if (seenKeys.has(dedupeKey)) continue;
    seenKeys.add(dedupeKey);
    supporters.push({
      name: name || "Private",
      avatarUrl,
      profileUrl,
      isPrivate: isPrivate || !name
    });
  }
  return supporters;
}
async function fetchThanksSupporters() {
  const response = await fetchWithTimeout(
    THANKS_SUPPORTERS_URL,
    {
      headers: {
        ...THANKS_REQUEST_HEADERS,
        Accept: "text/html,application/xhtml+xml"
      }
    },
    THANKS_FETCH_TIMEOUT_MS,
    "GitHub sponsors request"
  );
  if (!response.ok) {
    throw new Error(`GitHub sponsors page request failed (${response.status})`);
  }
  const html = await withTimeout(
    response.text(),
    THANKS_FETCH_TIMEOUT_MS,
    "GitHub sponsors response"
  );
  const supporters = parseSupportersFromHtml(html);
  return supporters;
}
async function fetchThanksData() {
  const result = {
    contributors: [],
    supporters: []
  };
  const [contributorsResult, supportersResult] = await Promise.allSettled([
    fetchThanksContributors(),
    fetchThanksSupporters()
  ]);
  if (contributorsResult.status === "fulfilled") {
    result.contributors = contributorsResult.value;
  } else {
    result.contributorsError = contributorsResult.reason instanceof Error ? contributorsResult.reason.message : "Unable to load contributors right now.";
  }
  if (supportersResult.status === "fulfilled") {
    result.supporters = supportersResult.value;
    if (result.supporters.length === 0) {
      result.supportersError = "No public supporters were found on GitHub Sponsors.";
    }
  } else {
    result.supportersError = supportersResult.reason instanceof Error ? supportersResult.reason.message : "Unable to load supporters right now.";
  }
  return result;
}
function registerIpcHandlers() {
  registerAccountCatalogIpcHandlers({
    ipcMain,
    authService,
    resolveJwt,
    refreshScheduler
  });
  registerSessionIpcHandlers({
    ipcMain,
    dialog,
    authService,
    settingsManager,
    resolveJwt,
    setActivity,
    clearActivity,
    getMainWindow: () => mainWindow
  });
  signalingCoordinator = registerSignalingIpcHandlers({
    ipcMain,
    mainDir: __dirname$1,
    settingsManager,
    getMainWindow: () => mainWindow
  });
  ipcMain.handle(IPC_CHANNELS.DISCORD_CLEAR_ACTIVITY, async () => {
    void clearActivity();
  });
  ipcMain.handle(IPC_CHANNELS.TOGGLE_FULLSCREEN, async () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      const isFullScreen = mainWindow.isFullScreen();
      const nextFullscreen = !isFullScreen;
      mainWindow.setFullScreen(nextFullscreen);
      rendererControlledFullscreen = nextFullscreen;
    }
  });
  ipcMain.handle(
    IPC_CHANNELS.SET_FULLSCREEN,
    async (_event, value) => {
      if (mainWindow && !mainWindow.isDestroyed()) {
        try {
          const nextFullscreen = Boolean(value);
          mainWindow.setFullScreen(nextFullscreen);
          rendererControlledFullscreen = nextFullscreen;
        } catch (err) {
          console.warn("Failed to set fullscreen:", err);
        }
      }
    }
  );
  ipcMain.handle(IPC_CHANNELS.TOGGLE_POINTER_LOCK, async () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send("app:toggle-pointer-lock");
    }
  });
  ipcMain.handle(IPC_CHANNELS.QUIT_APP, async () => {
    requestAppShutdown({
      reason: "renderer-explicit-exit",
      forceExitFallback: true
    });
  });
  ipcMain.handle(
    IPC_CHANNELS.APP_UPDATER_GET_STATE,
    async () => {
      const buildInfo = getAppBuildInfo();
      return appUpdater?.getState() ?? {
        status: "disabled",
        currentVersion: buildInfo.version,
        currentDisplayVersion: buildInfo.displayVersion,
        currentBuildNumber: buildInfo.buildNumber,
        updateSource: "github-releases",
        canCheck: false,
        canDownload: false,
        canInstall: false,
        isPackaged: app.isPackaged,
        message: "Updater is unavailable."
      };
    }
  );
  ipcMain.handle(
    IPC_CHANNELS.APP_UPDATER_CHECK,
    async () => {
      const buildInfo = getAppBuildInfo();
      return appUpdater?.checkForUpdates("manual") ?? {
        status: "disabled",
        currentVersion: buildInfo.version,
        currentDisplayVersion: buildInfo.displayVersion,
        currentBuildNumber: buildInfo.buildNumber,
        updateSource: "github-releases",
        canCheck: false,
        canDownload: false,
        canInstall: false,
        isPackaged: app.isPackaged,
        message: "Updater is unavailable."
      };
    }
  );
  ipcMain.handle(
    IPC_CHANNELS.APP_UPDATER_DOWNLOAD,
    async () => {
      const buildInfo = getAppBuildInfo();
      return appUpdater?.downloadUpdate() ?? {
        status: "disabled",
        currentVersion: buildInfo.version,
        currentDisplayVersion: buildInfo.displayVersion,
        currentBuildNumber: buildInfo.buildNumber,
        updateSource: "github-releases",
        canCheck: false,
        canDownload: false,
        canInstall: false,
        isPackaged: app.isPackaged,
        message: "Updater is unavailable."
      };
    }
  );
  ipcMain.handle(
    IPC_CHANNELS.APP_UPDATER_INSTALL,
    async () => {
      const buildInfo = getAppBuildInfo();
      return appUpdater?.quitAndInstall() ?? {
        status: "disabled",
        currentVersion: buildInfo.version,
        currentDisplayVersion: buildInfo.displayVersion,
        currentBuildNumber: buildInfo.buildNumber,
        updateSource: "github-releases",
        canCheck: false,
        canDownload: false,
        canInstall: false,
        isPackaged: app.isPackaged,
        message: "Updater is unavailable."
      };
    }
  );
  ipcMain.handle(IPC_CHANNELS.SETTINGS_GET, async () => {
    return settingsManager.getAll();
  });
  ipcMain.handle(
    IPC_CHANNELS.SETTINGS_SET,
    async (_event, key, value) => {
      settingsManager.set(key, value);
      const appliedValue = settingsManager.get(key);
      try {
        if (key === "autoCheckForUpdates") {
          appUpdater?.setAutomaticChecksEnabled(appliedValue);
        }
        signalingCoordinator?.applySettingsChange(key, appliedValue);
        if (key === "discordRichPresence") {
          if (appliedValue) {
            void connectDiscordRpc().then(() => discordMonitor.start());
          } else {
            discordMonitor.stop();
            void destroyDiscordRpc();
          }
        }
      } catch (err) {
        console.warn("Failed to apply setting change in main process:", err);
      }
    }
  );
  ipcMain.handle(IPC_CHANNELS.SETTINGS_RESET, async () => {
    const resetSettings = settingsManager.reset();
    appUpdater?.setAutomaticChecksEnabled(resetSettings.autoCheckForUpdates);
    signalingCoordinator?.stopNativeStreamer("settings reset");
    signalingCoordinator?.resetNativeStreamerContext();
    return resetSettings;
  });
  ipcMain.handle(
    IPC_CHANNELS.SETTINGS_SELECT_NATIVE_STREAMER_EXECUTABLE,
    async () => {
      const filters = process.platform === "win32" ? [
        { name: "Executable", extensions: ["exe"] },
        { name: "All Files", extensions: ["*"] }
      ] : [{ name: "All Files", extensions: ["*"] }];
      const options = {
        title: "Select OpenNOW streamer executable",
        properties: ["openFile"],
        filters
      };
      const result = mainWindow && !mainWindow.isDestroyed() ? await dialog.showOpenDialog(mainWindow, options) : await dialog.showOpenDialog(options);
      if (result.canceled || result.filePaths.length === 0) {
        return null;
      }
      return result.filePaths[0] ?? null;
    }
  );
  ipcMain.handle(
    IPC_CHANNELS.MICROPHONE_PERMISSION_GET,
    async () => {
      if (process.platform !== "darwin") {
        return {
          platform: process.platform,
          isMacOs: false,
          status: "not-applicable",
          granted: false,
          canRequest: false,
          shouldUseBrowserApi: true
        };
      }
      const currentStatus = systemPreferences.getMediaAccessStatus("microphone");
      console.log("[Main] macOS microphone permission status:", currentStatus);
      if (currentStatus === "granted") {
        return {
          platform: process.platform,
          isMacOs: true,
          status: "granted",
          granted: true,
          canRequest: false,
          shouldUseBrowserApi: true
        };
      }
      if (currentStatus === "not-determined") {
        const granted = await systemPreferences.askForMediaAccess("microphone");
        const nextStatus = systemPreferences.getMediaAccessStatus("microphone");
        console.log(
          "[Main] Requested macOS microphone permission:",
          granted,
          nextStatus
        );
        return {
          platform: process.platform,
          isMacOs: true,
          status: nextStatus,
          granted,
          canRequest: nextStatus === "not-determined",
          shouldUseBrowserApi: granted
        };
      }
      return {
        platform: process.platform,
        isMacOs: true,
        status: currentStatus,
        granted: false,
        canRequest: false,
        shouldUseBrowserApi: false
      };
    }
  );
  ipcMain.handle(
    IPC_CHANNELS.LOGS_EXPORT,
    async (_event, format = "text") => {
      return exportLogs(format);
    }
  );
  registerMediaIpcHandlers({
    ipcMain,
    dialog,
    shell,
    getMainWindow: () => mainWindow
  });
  ipcMain.handle(IPC_CHANNELS.CACHE_REFRESH_MANUAL, async () => {
    await refreshScheduler.manualRefresh();
  });
  ipcMain.handle(IPC_CHANNELS.CACHE_DELETE_ALL, async () => {
    await cacheManager.deleteAll();
    console.log("[IPC] Cache deletion completed successfully");
  });
  ipcMain.handle(
    IPC_CHANNELS.COMMUNITY_GET_THANKS,
    async () => {
      return fetchThanksData();
    }
  );
  ipcMain.handle(
    IPC_CHANNELS.PING_REGIONS,
    async (_event, regions) => {
      return pingRegions(regions);
    }
  );
  ipcMain.handle(IPC_CHANNELS.PRINTEDWASTE_QUEUE_FETCH, async () => {
    return fetchPrintedWasteQueue(app.getVersion());
  });
  ipcMain.handle(IPC_CHANNELS.PRINTEDWASTE_SERVER_MAPPING_FETCH, async () => {
    return fetchPrintedWasteServerMapping(app.getVersion());
  });
  mainWindow?.on("resize", () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      const [width, height] = mainWindow.getSize();
      settingsManager.set("windowWidth", width);
      settingsManager.set("windowHeight", height);
    }
  });
}
app.whenReady().then(async () => {
  initLogCapture("main");
  await cacheManager.initialize();
  authService = new AuthService(
    join(app.getPath("userData"), "auth-state.json")
  );
  await authService.initialize();
  settingsManager = getSettingsManager();
  appUpdater = createAppUpdaterController({
    onStateChanged: emitUpdaterStateToRenderer,
    automaticChecksEnabled: settingsManager.get("autoCheckForUpdates"),
    onBeforeQuitAndInstall: () => {
      isUpdaterInstallQuitInProgress = true;
      clearExplicitShutdownFallback();
    },
    onQuitAndInstallError: () => {
      isUpdaterInstallQuitInProgress = false;
    }
  });
  if (settingsManager.get("discordRichPresence")) {
    void connectDiscordRpc().then(() => discordMonitor.start());
  }
  session.defaultSession.setPermissionRequestHandler(
    (webContents, permission, callback) => {
      const allowedPermissions = /* @__PURE__ */ new Set([
        "media",
        "microphone",
        "fullscreen",
        "automatic-fullscreen",
        "pointerLock",
        "keyboardLock",
        "speaker-selection"
      ]);
      if (allowedPermissions.has(permission)) {
        callback(true);
        return;
      }
      callback(false);
    }
  );
  session.defaultSession.setPermissionCheckHandler(
    (_webContents, permission, _requestingOrigin) => {
      const allowedPermissions = /* @__PURE__ */ new Set([
        "media",
        "microphone",
        "fullscreen",
        "automatic-fullscreen",
        "pointerLock",
        "keyboardLock",
        "speaker-selection"
      ]);
      return allowedPermissions.has(permission);
    }
  );
  registerOpenNowMediaProtocol();
  registerIpcHandlers();
  refreshScheduler.initialize(
    fetchMainGamesUncached,
    fetchLibraryGamesUncached,
    fetchPublicGamesUncached
  );
  cacheEventBus.on("cache:refresh-start", () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send(IPC_CHANNELS.CACHE_STATUS_UPDATE, {
        event: "refresh-start"
      });
    }
  });
  cacheEventBus.on("cache:refresh-success", () => {
    if (mainWindow && !mainWindow.isDestroyed()) {
      mainWindow.webContents.send(IPC_CHANNELS.CACHE_STATUS_UPDATE, {
        event: "refresh-success"
      });
    }
  });
  cacheEventBus.on(
    "cache:refresh-error",
    (details) => {
      if (mainWindow && !mainWindow.isDestroyed()) {
        mainWindow.webContents.send(IPC_CHANNELS.CACHE_STATUS_UPDATE, {
          event: "refresh-error",
          ...details
        });
      }
    }
  );
  refreshScheduler.start();
  await createMainWindow();
  appUpdater.initialize();
  app.on("activate", async () => {
    if (isShutdownRequested) {
      return;
    }
    if (BrowserWindow.getAllWindows().length === 0) {
      await createMainWindow();
    }
  });
});
app.on("window-all-closed", () => {
  requestAppShutdown({ reason: "window-all-closed" });
});
app.on("before-quit", () => {
  isShutdownRequested = true;
  runShutdownCleanup(
    isUpdaterInstallQuitInProgress ? "before-quit-updater-install" : "before-quit"
  );
});
app.on("will-quit", () => {
  clearExplicitShutdownFallback();
});
app.on("quit", () => {
  clearExplicitShutdownFallback();
});
export {
  isSessionConflictError,
  showSessionConflictDialog
};
//# sourceMappingURL=index.js.map
