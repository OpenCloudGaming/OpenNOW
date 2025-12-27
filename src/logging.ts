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
 * Initialize frontend logging - intercepts console methods
 * and sends logs to the backend for file storage
 */
export function initLogging(): void {
  // Override console.log
  console.log = (...args: unknown[]) => {
    originalConsole.log(...args);
    sendToBackend("info", ...args);
  };

  // Override console.info
  console.info = (...args: unknown[]) => {
    originalConsole.info(...args);
    sendToBackend("info", ...args);
  };

  // Override console.warn
  console.warn = (...args: unknown[]) => {
    originalConsole.warn(...args);
    sendToBackend("warn", ...args);
  };

  // Override console.error
  console.error = (...args: unknown[]) => {
    originalConsole.error(...args);
    sendToBackend("error", ...args);
  };

  // Override console.debug
  console.debug = (...args: unknown[]) => {
    originalConsole.debug(...args);
    sendToBackend("debug", ...args);
  };

  // Capture unhandled errors
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

  originalConsole.log("[Logging] Frontend logging initialized");
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
