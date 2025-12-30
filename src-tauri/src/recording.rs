use std::fs;
use std::path::PathBuf;
use tauri::command;

/// Get the default recordings directory (Videos/OpenNow)
fn get_default_recordings_dir() -> PathBuf {
    dirs::video_dir()
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")))
        .join("OpenNow")
}

/// Get the recordings directory (custom or default)
fn get_recordings_dir_path(custom_dir: Option<String>) -> PathBuf {
    match custom_dir {
        Some(dir) if !dir.is_empty() => PathBuf::from(dir),
        _ => get_default_recordings_dir(),
    }
}

/// Ensure the recordings directory exists
fn ensure_recordings_dir(path: &PathBuf) -> Result<(), String> {
    if !path.exists() {
        fs::create_dir_all(path)
            .map_err(|e| format!("Failed to create recordings directory: {}", e))?;
    }
    Ok(())
}

/// Get the recordings directory path
#[command]
pub async fn get_recordings_dir(custom_dir: Option<String>) -> Result<String, String> {
    let path = get_recordings_dir_path(custom_dir);
    ensure_recordings_dir(&path)?;
    path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Invalid path encoding".to_string())
}

/// Save a recording file (receives raw bytes from frontend)
#[command]
pub async fn save_recording(
    data: Vec<u8>,
    filename: String,
    custom_dir: Option<String>,
) -> Result<String, String> {
    let dir = get_recordings_dir_path(custom_dir);
    ensure_recordings_dir(&dir)?;
    
    let file_path = dir.join(&filename);
    
    fs::write(&file_path, &data)
        .map_err(|e| format!("Failed to save recording: {}", e))?;
    
    log::info!("Recording saved: {:?} ({} bytes)", file_path, data.len());
    
    file_path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Invalid path encoding".to_string())
}

/// Save a screenshot file (receives raw bytes from frontend)
#[command]
pub async fn save_screenshot(
    data: Vec<u8>,
    filename: String,
    custom_dir: Option<String>,
) -> Result<String, String> {
    let dir = get_recordings_dir_path(custom_dir);
    ensure_recordings_dir(&dir)?;
    
    let file_path = dir.join(&filename);
    
    fs::write(&file_path, &data)
        .map_err(|e| format!("Failed to save screenshot: {}", e))?;
    
    log::info!("Screenshot saved: {:?} ({} bytes)", file_path, data.len());
    
    file_path.to_str()
        .map(|s| s.to_string())
        .ok_or_else(|| "Invalid path encoding".to_string())
}

/// Open the recordings folder in the system file explorer
#[command]
pub async fn open_recordings_folder(custom_dir: Option<String>) -> Result<(), String> {
    let dir = get_recordings_dir_path(custom_dir);
    ensure_recordings_dir(&dir)?;
    
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }
    
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }
    
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&dir)
            .spawn()
            .map_err(|e| format!("Failed to open folder: {}", e))?;
    }
    
    Ok(())
}

/// List all recordings in the recordings directory
#[command]
pub async fn list_recordings(custom_dir: Option<String>) -> Result<Vec<RecordingInfo>, String> {
    let dir = get_recordings_dir_path(custom_dir);
    
    if !dir.exists() {
        return Ok(Vec::new());
    }
    
    let mut recordings = Vec::new();
    
    let entries = fs::read_dir(&dir)
        .map_err(|e| format!("Failed to read recordings directory: {}", e))?;
    
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_str = ext.to_str().unwrap_or("");
                if ext_str == "webm" || ext_str == "png" {
                    if let Ok(metadata) = fs::metadata(&path) {
                        recordings.push(RecordingInfo {
                            filename: path.file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("")
                                .to_string(),
                            path: path.to_str().unwrap_or("").to_string(),
                            size_bytes: metadata.len(),
                            is_screenshot: ext_str == "png",
                        });
                    }
                }
            }
        }
    }
    
    // Sort by filename (which includes timestamp, so newest first)
    recordings.sort_by(|a, b| b.filename.cmp(&a.filename));
    
    Ok(recordings)
}

/// Delete a recording file
#[command]
pub async fn delete_recording(filepath: String) -> Result<(), String> {
    let path = PathBuf::from(&filepath);
    
    if !path.exists() {
        return Err("File not found".to_string());
    }
    
    fs::remove_file(&path)
        .map_err(|e| format!("Failed to delete recording: {}", e))?;
    
    log::info!("Recording deleted: {:?}", path);
    Ok(())
}

/// Recording file information
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RecordingInfo {
    pub filename: String,
    pub path: String,
    pub size_bytes: u64,
    pub is_screenshot: bool,
}
