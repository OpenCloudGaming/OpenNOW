//! Logging Utilities
//!
//! File-based and console logging.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use log::{Log, Metadata, Record, Level, LevelFilter};

/// Get the log file path
pub fn get_log_file_path() -> PathBuf {
    super::get_app_data_dir().join("streamer.log")
}

/// Simple file logger
pub struct FileLogger {
    file: Mutex<Option<File>>,
    console: bool,
}

impl FileLogger {
    pub fn new(console: bool) -> Self {
        let file = Self::open_log_file();
        Self {
            file: Mutex::new(file),
            console,
        }
    }

    fn open_log_file() -> Option<File> {
        let path = get_log_file_path();

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .ok()
    }
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        let target = metadata.target();
        let level = metadata.level();

        // STRICT filtering to prevent log spam from external crates
        // This is CRITICAL for performance - even file I/O has overhead
        
        // Our crate: allow INFO and above (DEBUG only if explicitly needed)
        if target.starts_with("opennow_streamer") {
            level <= Level::Info
        } else {
            // External crates: WARN and ERROR only
            // This silences: webrtc_sctp, webrtc_ice, webrtc, wgpu, wgpu_hal, etc.
            level <= Level::Warn
        }
    }

    fn log(&self, record: &Record) {
        let target = record.target();
        let level = record.level();

        // Double-check filtering (belt and suspenders)
        // External crates are restricted to WARN level
        if !target.starts_with("opennow_streamer") && level > Level::Warn {
            return;
        }

        // Our crate allows DEBUG
        if target.starts_with("opennow_streamer") && level > Level::Debug {
            return;
        }

        let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
        let message = record.args();

        let line = format!("[{}] {} {} - {}\n", timestamp, level, target, message);

        // Write to file
        if let Ok(mut guard) = self.file.lock() {
            if let Some(ref mut file) = *guard {
                let _ = file.write_all(line.as_bytes());
            }
        }

        // Write to console if enabled
        if self.console {
            print!("{}", line);
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

/// Initialize the logging system
/// 
/// Console logging is DISABLED by default for performance.
/// Windows console I/O is blocking and causes severe frame drops when
/// external crates (webrtc_sctp, wgpu, etc.) spam debug messages.
/// All logs are still written to the log file for debugging.
pub fn init_logging() -> Result<(), log::SetLoggerError> {
    // CRITICAL: Console logging disabled for performance
    // External crates spam DEBUG logs on every mouse movement
    // Console I/O on Windows is blocking, causing "20 fps feel"
    let logger = Box::new(FileLogger::new(false));
    log::set_boxed_logger(logger)?;
    // Set global max to Info - we don't need DEBUG from external crates
    // Our crate can still log at any level via the logger's enabled() check
    log::set_max_level(LevelFilter::Info);
    Ok(())
}

/// Initialize logging with console output (for debugging only)
/// WARNING: This will cause performance issues during streaming!
pub fn init_logging_with_console() -> Result<(), log::SetLoggerError> {
    let logger = Box::new(FileLogger::new(true));
    log::set_boxed_logger(logger)?;
    log::set_max_level(LevelFilter::Info);
    Ok(())
}

/// Clear log file
pub fn clear_logs() -> std::io::Result<()> {
    let path = get_log_file_path();
    if path.exists() {
        std::fs::write(&path, "")?;
    }
    Ok(())
}

/// Export logs to a specific path
pub fn export_logs(dest: &PathBuf) -> std::io::Result<()> {
    let src = get_log_file_path();
    if src.exists() {
        std::fs::copy(&src, dest)?;
    }
    Ok(())
}

/// Print a message directly to console (bypasses logger)
/// Use sparingly - only for critical startup info
#[inline]
pub fn console_print(msg: &str) {
    println!("{}", msg);
}
