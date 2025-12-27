use chrono::Local;
use log::{Level, LevelFilter, Log, Metadata, Record, SetLoggerError};
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

#[cfg(feature = "tauri-app")]
use tauri::command;

// ============================================================================
// Sensitive Data Sanitization
// ============================================================================

/// Sanitize log content by redacting sensitive information
fn sanitize_logs(content: &str) -> String {
    let mut sanitized = content.to_string();

    // Redact JWT tokens (format: xxxxx.xxxxx.xxxxx)
    let jwt_regex = regex_lite::Regex::new(r"eyJ[A-Za-z0-9_-]*\.eyJ[A-Za-z0-9_-]*\.[A-Za-z0-9_-]*").unwrap();
    sanitized = jwt_regex.replace_all(&sanitized, "[REDACTED_JWT]").to_string();

    // Redact Bearer tokens
    let bearer_regex = regex_lite::Regex::new(r"(?i)bearer\s+[A-Za-z0-9_\-\.]+").unwrap();
    sanitized = bearer_regex.replace_all(&sanitized, "Bearer [REDACTED]").to_string();

    // Redact common token patterns in JSON (e.g., "access_token": "value")
    let json_token_regex = regex_lite::Regex::new(
        r#"(?i)["']?(access_token|refresh_token|id_token|auth_token|token|apikey|api_key|secret|password|nvauthtoken)["']?\s*[:=]\s*["']?[A-Za-z0-9_\-\.]+["']?"#
    ).unwrap();
    sanitized = json_token_regex.replace_all(&sanitized, |caps: &regex_lite::Captures| {
        // Extract the key name and redact only the value
        let full_match = caps.get(0).unwrap().as_str();
        if let Some(key) = caps.get(1) {
            format!("\"{}\":\"[REDACTED]\"", key.as_str())
        } else {
            full_match.to_string()
        }
    }).to_string();

    // Redact email addresses
    let email_regex = regex_lite::Regex::new(r"[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}").unwrap();
    sanitized = email_regex.replace_all(&sanitized, "[REDACTED_EMAIL]").to_string();

    // Redact long hex strings (potential tokens/keys, 32+ chars)
    let hex_regex = regex_lite::Regex::new(r"\b[a-fA-F0-9]{32,}\b").unwrap();
    sanitized = hex_regex.replace_all(&sanitized, "[REDACTED_HEX]").to_string();

    // Redact base64-like strings that are long (potential tokens, 40+ chars)
    let base64_regex = regex_lite::Regex::new(r"\b[A-Za-z0-9+/]{40,}={0,2}\b").unwrap();
    sanitized = base64_regex.replace_all(&sanitized, "[REDACTED_TOKEN]").to_string();

    // Redact IP addresses (keep for debugging but anonymize last octet)
    let ip_regex = regex_lite::Regex::new(r"\b(\d{1,3}\.\d{1,3}\.\d{1,3}\.)\d{1,3}\b").unwrap();
    sanitized = ip_regex.replace_all(&sanitized, "${1}xxx").to_string();

    // Add header noting sanitization
    let header = "=== LOGS SANITIZED FOR PRIVACY ===\n\
                  === Sensitive data (tokens, emails, etc.) has been redacted ===\n\n";

    format!("{}{}", header, sanitized)
}

/// Global file logger instance
static FILE_LOGGER: std::sync::OnceLock<FileLogger> = std::sync::OnceLock::new();

/// Get the log file path in the user's data directory
pub fn get_log_path() -> PathBuf {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opennow");

    // Ensure directory exists
    let _ = fs::create_dir_all(&data_dir);

    data_dir.join("opennow.log")
}

/// Custom file logger that writes to both console and file
pub struct FileLogger {
    file: Mutex<Option<File>>,
    log_path: PathBuf,
}

impl FileLogger {
    pub fn new() -> Self {
        let log_path = get_log_path();

        // Clear log if it's too large (> 10MB)
        if let Ok(metadata) = fs::metadata(&log_path) {
            if metadata.len() > 10 * 1024 * 1024 {
                // Clear the file instead of rotating
                let _ = File::create(&log_path);
            }
        }

        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();

        // Write session start marker
        if let Some(ref mut f) = file.as_ref() {
            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(
                &mut f.try_clone().unwrap(),
                "\n========== OpenNOW Session Started at {} ==========\n",
                timestamp
            );
        }

        FileLogger {
            file: Mutex::new(file),
            log_path,
        }
    }

    /// Write a log entry to the file
    pub fn write_to_file(&self, level: &str, target: &str, message: &str) {
        if let Ok(mut guard) = self.file.lock() {
            // Check file size and clear if > 10MB
            if let Ok(metadata) = fs::metadata(&self.log_path) {
                if metadata.len() > 10 * 1024 * 1024 {
                    // Reopen file in truncate mode to clear it
                    if let Ok(new_file) = File::create(&self.log_path) {
                        *guard = Some(new_file);
                        if let Some(ref mut f) = *guard {
                            let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                            let _ = writeln!(f, "=== Log cleared (exceeded 10MB) at {} ===\n", timestamp);
                        }
                    }
                }
            }

            if let Some(ref mut file) = *guard {
                let timestamp = Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
                let _ = writeln!(file, "[{}] [{}] [{}] {}", timestamp, level, target, message);
                let _ = file.flush();
            }
        }
    }

    /// Get the path to the log file
    pub fn path(&self) -> &PathBuf {
        &self.log_path
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Debug
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let level = record.level();
            let target = record.target();
            let message = format!("{}", record.args());

            // Write to file
            self.write_to_file(&level.to_string(), target, &message);

            // Also print to console (stderr for errors/warnings, stdout for others)
            let timestamp = Local::now().format("%H:%M:%S%.3f");
            let formatted = format!("[{}] [{}] [{}] {}", timestamp, level, target, message);

            match level {
                Level::Error | Level::Warn => eprintln!("{}", formatted),
                _ => println!("{}", formatted),
            }
        }
    }

    fn flush(&self) {
        if let Ok(mut guard) = self.file.lock() {
            if let Some(ref mut file) = *guard {
                let _ = file.flush();
            }
        }
    }
}

/// Initialize the custom file logger
pub fn init() -> Result<(), SetLoggerError> {
    let logger = FILE_LOGGER.get_or_init(FileLogger::new);

    log::set_logger(logger)?;
    log::set_max_level(LevelFilter::Debug);

    log::info!("Logging initialized, log file: {:?}", logger.path());

    Ok(())
}

/// Get the global logger instance
pub fn get_logger() -> Option<&'static FileLogger> {
    FILE_LOGGER.get()
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Log a message from the frontend
#[cfg(feature = "tauri-app")]
#[command]
pub fn log_frontend(level: String, message: String) {
    if let Some(logger) = get_logger() {
        logger.write_to_file(&level, "frontend", &message);
    }

    // Also log through the standard log macros for console output
    match level.to_lowercase().as_str() {
        "error" => log::error!(target: "frontend", "{}", message),
        "warn" => log::warn!(target: "frontend", "{}", message),
        "debug" => log::debug!(target: "frontend", "{}", message),
        _ => log::info!(target: "frontend", "{}", message),
    }
}

/// Get the current log file path
#[cfg(feature = "tauri-app")]
#[command]
pub fn get_log_file_path() -> Result<String, String> {
    let path = get_log_path();
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Failed to get log path".to_string())
}

/// Export logs to a user-selected location
#[cfg(feature = "tauri-app")]
#[command]
pub async fn export_logs() -> Result<String, String> {
    let log_path = get_log_path();

    if !log_path.exists() {
        return Err("No log file found".to_string());
    }

    // Generate default filename with timestamp
    let timestamp = Local::now().format("%Y%m%d_%H%M%S");
    let default_name = format!("opennow_logs_{}.log", timestamp);

    // Open save dialog using rfd
    let save_path = rfd::AsyncFileDialog::new()
        .set_title("Export OpenNOW Logs")
        .set_file_name(&default_name)
        .add_filter("Log Files", &["log", "txt"])
        .save_file()
        .await;

    match save_path {
        Some(handle) => {
            let dest_path = handle.path();

            // Read the log file content
            let content = fs::read_to_string(&log_path)
                .map_err(|e| format!("Failed to read log file: {}", e))?;

            // Sanitize sensitive data before exporting
            let sanitized_content = sanitize_logs(&content);

            // Write sanitized content to the selected location
            fs::write(dest_path, sanitized_content)
                .map_err(|e| format!("Failed to write log file: {}", e))?;

            log::info!("Logs exported (sanitized) to: {:?}", dest_path);

            Ok(dest_path.to_string_lossy().to_string())
        }
        None => Err("Export cancelled".to_string()),
    }
}

/// Clear the current log file
#[cfg(feature = "tauri-app")]
#[command]
pub fn clear_logs() -> Result<(), String> {
    let log_path = get_log_path();

    // Truncate the file
    File::create(&log_path).map_err(|e| format!("Failed to clear logs: {}", e))?;

    log::info!("Log file cleared");

    Ok(())
}
