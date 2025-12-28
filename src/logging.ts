import { invoke } from "@tauri-apps/api/core";

// Store original console methods
const originalConsole = {
  log: console.log.bind(console),
  warn: console.warn.bind(console),
  error: console.error.bind(console),
  info: console.info.bind(console),
  debug: console.debug.bind(console),
};

// Flag to prevent recursive logging
let isLogging = false;

/**
 * Send a log message to the backend for file logging
 */
async function sendToBackend(level: string, ...args: unknown[]): Promise<void> {
  if (isLogging) return;

  try {
    isLogging = true;
    const message = args
      .map((arg) => {
        if (typeof arg === "object") {
          try {
            return JSON.stringify(arg, null, 2);
          } catch {
            return String(arg);
          }
        }
        return String(arg);
      })
      .join(" ");

    await invoke("log_frontend", { level, message });
  } catch {
    // Silently fail - don't want logging errors to break the app
  } finally {
    isLogging = false;
  }
}

/**
 * Initialize frontend logging
 * 
 * NOTE: Console method overrides have been disabled to prevent memory leaks.
 * The previous implementation sent every console.log/info/warn/error/debug to
 * the backend via IPC, causing excessive memory usage (1GB+) due to:
 * - JSON serialization overhead on every log call
 * - IPC message queue buildup
 * - String allocations that couldn't be GC'd fast enough
 * 
 * Now only critical errors (unhandled exceptions) are sent to the backend.
 * Use logToBackend() explicitly for important messages that need file logging.
 */
export function initLogging(): void {
  // Only capture unhandled errors - these are critical and infrequent
  window.addEventListener("error", (event) => {
    sendToBackend(
      "error",
      `Unhandled error: ${event.message} at ${event.filename}:${event.lineno}:${event.colno}`
    );
  });

  // Capture unhandled promise rejections
  window.addEventListener("unhandledrejection", (event) => {
    sendToBackend("error", `Unhandled promise rejection: ${event.reason}`);
  });

  originalConsole.log("[Logging] Frontend logging initialized (lightweight mode)");
}

/**
 * Explicitly log a message to the backend file log
 * Use this for important messages that should be persisted
 */
export async function logToBackend(level: "info" | "warn" | "error" | "debug", message: string): Promise<void> {
  await sendToBackend(level, message);
}

/**
 * Export logs to a user-selected file
 * @returns The path where logs were saved, or throws on error/cancel
 */
export async function exportLogs(): Promise<string> {
  return await invoke<string>("export_logs");
}

/**
 * Get the current log file path
 */
export async function getLogFilePath(): Promise<string> {
  return await invoke<string>("get_log_file_path");
}

/**
 * Clear the current log file
 */
export async function clearLogs(): Promise<void> {
  await invoke("clear_logs");
}
