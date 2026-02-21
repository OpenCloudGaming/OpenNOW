/**
 * Shared logger utilities for log capture and privacy-safe export
 * Handles redaction of sensitive information like emails, passwords, tokens, etc.
 */

export interface LogEntry {
  timestamp: number;
  level: "log" | "error" | "warn" | "info" | "debug";
  prefix: string;
  message: string;
  args: unknown[];
}

/** Maximum number of log entries to keep in memory */
const MAX_LOG_ENTRIES = 5000;

/** Patterns for sensitive data redaction */
const SENSITIVE_PATTERNS = [
  // Email addresses
  { pattern: /[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}/g, replacement: "[Redacted for privacy]" },
  // Authorization tokens (GFNJWT, Bearer, etc.)
  { pattern: /Authorization["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]+\s+[a-zA-Z0-9_\-]+/gi, replacement: 'Authorization: [Redacted for privacy]' },
  // JWT tokens (three base64url parts separated by dots)
  { pattern: /[a-zA-Z0-9_\-]{10,}\.[a-zA-Z0-9_\-]{10,}\.[a-zA-Z0-9_\-]{10,}/g, replacement: "[Redacted for privacy]" },
  // Client tokens, access tokens
  { pattern: /client[_-]?token["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]{20,}/gi, replacement: 'client_token: [Redacted for privacy]' },
  { pattern: /access[_-]?token["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]{20,}/gi, replacement: 'access_token: [Redacted for privacy]' },
  { pattern: /refresh[_-]?token["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]{20,}/gi, replacement: 'refresh_token: [Redacted for privacy]' },
  // Session IDs (UUID-like)
  { pattern: /session[_-]?id["']?\s*[:=]\s*["']?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, replacement: 'session_id: [Redacted for privacy]' },
  // Passwords
  { pattern: /password["']?\s*[:=]\s*["']?[^\s"']{4,}/gi, replacement: 'password: [Redacted for privacy]' },
  // API keys
  { pattern: /api[_-]?key["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]{16,}/gi, replacement: 'api_key: [Redacted for privacy]' },
  // Credential/secret
  { pattern: /credential["']?\s*[:=]\s*["']?[^\s"']{8,}/gi, replacement: 'credential: [Redacted for privacy]' },
  { pattern: /secret["']?\s*[:=]\s*["']?[^\s"']{8,}/gi, replacement: 'secret: [Redacted for privacy]' },
  // IP addresses (might be sensitive in some contexts)
  { pattern: /\b(?:[0-9]{1,3}\.){3}[0-9]{1,3}\b/g, replacement: "[Redacted IP]" },
  // Device IDs, client IDs (UUID-like patterns)
  { pattern: /device[_-]?id["']?\s*[:=]\s*["']?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, replacement: 'device_id: [Redacted for privacy]' },
  { pattern: /client[_-]?id["']?\s*[:=]\s*["']?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, replacement: 'client_id: [Redacted for privacy]' },
  // User IDs that look like UUIDs
  { pattern: /user[_-]?id["']?\s*[:=]\s*["']?[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/gi, replacement: 'user_id: [Redacted for privacy]' },
  // OAuth codes
  { pattern: /code["']?\s*[:=]\s*["']?[a-zA-Z0-9_\-]{20,}/gi, replacement: 'code: [Redacted for privacy]' },
  // Peer names/IDs in signaling
  { pattern: /peer[_-]?name["']?\s*[:=]\s*["']?peer-\d+/gi, replacement: 'peer_name: [Redacted for privacy]' },
];

/**
 * Redact sensitive information from a string
 */
export function redactSensitiveData(text: string): string {
  let redacted = text;
  for (const { pattern, replacement } of SENSITIVE_PATTERNS) {
    redacted = redacted.replace(pattern, replacement);
  }
  return redacted;
}

/**
 * Format a log entry to string
 */
export function formatLogEntry(entry: LogEntry): string {
  const date = new Date(entry.timestamp);
  const timeStr = date.toISOString();
  const levelStr = entry.level.toUpperCase().padStart(5);
  const prefixStr = entry.prefix ? `[${entry.prefix}] ` : "";
  const argsStr = entry.args.length > 0
    ? " " + entry.args.map(arg => {
        try {
          if (typeof arg === "object" && arg !== null) {
            return JSON.stringify(arg);
          }
          return String(arg);
        } catch {
          return "[Object]";
        }
      }).join(" ")
    : "";

  return `${timeStr} ${levelStr} ${prefixStr}${entry.message}${argsStr}`;
}

/**
 * Create redacted log export
 */
export function createRedactedLogExport(entries: LogEntry[]): string {
  const lines = entries.map(entry => {
    const formatted = formatLogEntry(entry);
    return redactSensitiveData(formatted);
  });

  return lines.join("\n");
}

/**
 * Logger class that captures console output
 */
export class LogCapture {
  private entries: LogEntry[] = [];
  private originalConsole: Partial<typeof console> | null = null;
  private processName: string;

  constructor(processName: string) {
    this.processName = processName;
  }

  /**
   * Get all captured log entries
   */
  getEntries(): LogEntry[] {
    return [...this.entries];
  }

  /**
   * Clear all log entries
   */
  clear(): void {
    this.entries = [];
  }

  /**
   * Get count of captured entries
   */
  getCount(): number {
    return this.entries.length;
  }

  /**
   * Add a log entry directly
   */
  addEntry(level: LogEntry["level"], prefix: string, message: string, args: unknown[]): void {
    const entry: LogEntry = {
      timestamp: Date.now(),
      level,
      prefix,
      message,
      args,
    };

    this.entries.push(entry);

    // Keep log size bounded
    if (this.entries.length > MAX_LOG_ENTRIES) {
      this.entries.shift();
    }
  }

  /**
   * Intercept console methods to capture logs
   */
  interceptConsole(): void {
    if (this.originalConsole) {
      return; // Already intercepted
    }

    this.originalConsole = {
      log: console.log,
      error: console.error,
      warn: console.warn,
      info: console.info,
      debug: console.debug,
    };

    // Extract prefix from first argument if it's a string like "[Module] message"
    const extractPrefix = (args: unknown[]): { prefix: string; message: string; rest: unknown[] } => {
      if (args.length > 0 && typeof args[0] === "string") {
        const match = args[0].match(/^\[([^\]]+)\]\s*(.*)$/);
        if (match) {
          return {
            prefix: match[1],
            message: match[2],
            rest: args.slice(1),
          };
        }
      }
      return {
        prefix: this.processName,
        message: args.length > 0 ? String(args[0]) : "",
        rest: args.slice(1),
      };
    };

    console.log = (...args: unknown[]) => {
      const { prefix, message, rest } = extractPrefix(args);
      this.addEntry("log", prefix, message, rest);
      this.originalConsole?.log?.apply(console, args);
    };

    console.error = (...args: unknown[]) => {
      const { prefix, message, rest } = extractPrefix(args);
      this.addEntry("error", prefix, message, rest);
      this.originalConsole?.error?.apply(console, args);
    };

    console.warn = (...args: unknown[]) => {
      const { prefix, message, rest } = extractPrefix(args);
      this.addEntry("warn", prefix, message, rest);
      this.originalConsole?.warn?.apply(console, args);
    };

    console.info = (...args: unknown[]) => {
      const { prefix, message, rest } = extractPrefix(args);
      this.addEntry("info", prefix, message, rest);
      this.originalConsole?.info?.apply(console, args);
    };

    console.debug = (...args: unknown[]) => {
      const { prefix, message, rest } = extractPrefix(args);
      this.addEntry("debug", prefix, message, rest);
      this.originalConsole?.debug?.apply(console, args);
    };
  }

  /**
   * Restore original console methods
   */
  restoreConsole(): void {
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
  exportRedacted(): string {
    const header = `OpenNOW Logs Export\nGenerated: ${new Date().toISOString()}\nSource: ${this.processName}\nTotal Entries: ${this.entries.length}\n${"=".repeat(60)}\n\n`;
    const redactedLogs = createRedactedLogExport(this.entries);
    return header + redactedLogs;
  }

  /**
   * Export logs as JSON (for programmatic use)
   */
  exportJSON(): string {
    const exportData = {
      source: this.processName,
      generatedAt: Date.now(),
      entryCount: this.entries.length,
      entries: this.entries.map(entry => ({
        ...entry,
        // Redact sensitive data in messages and args
        message: redactSensitiveData(entry.message),
        args: entry.args.map(arg => {
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
        }),
      })),
    };
    return JSON.stringify(exportData, null, 2);
  }
}

// Global log capture instance (used in main process or renderer)
let globalLogCapture: LogCapture | null = null;

/**
 * Initialize global log capture
 */
export function initLogCapture(processName: string): LogCapture {
  if (!globalLogCapture) {
    globalLogCapture = new LogCapture(processName);
    globalLogCapture.interceptConsole();
  }
  return globalLogCapture;
}

/**
 * Get global log capture instance
 */
export function getLogCapture(): LogCapture | null {
  return globalLogCapture;
}

/**
 * Export logs from main process (to be called via IPC)
 */
export function exportLogs(format: "text" | "json" = "text"): string {
  if (!globalLogCapture) {
    return "No logs captured";
  }
  return format === "json" ? globalLogCapture.exportJSON() : globalLogCapture.exportRedacted();
}
