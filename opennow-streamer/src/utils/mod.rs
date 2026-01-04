//! Utility Functions
//!
//! Common utilities used throughout the application.

mod logging;
mod time;

pub use logging::*;
pub use time::*;

use std::path::PathBuf;

/// Get the application data directory
pub fn get_app_data_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opennow-streamer")
}

/// Get the cache directory
pub fn get_cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("opennow-streamer")
}

/// Ensure a directory exists
pub fn ensure_dir(path: &PathBuf) -> std::io::Result<()> {
    if !path.exists() {
        std::fs::create_dir_all(path)?;
    }
    Ok(())
}

/// Generate a random peer ID for signaling
pub fn generate_peer_id() -> String {
    let random: u64 = rand::random::<u64>() % 10_000_000_000;
    format!("peer-{}", random)
}

/// Generate a UUID string
pub fn generate_uuid() -> String {
    uuid::Uuid::new_v4().to_string()
}
