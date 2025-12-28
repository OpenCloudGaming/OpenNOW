use std::fs;
use std::path::PathBuf;

/// Get the application data directory.
/// 
/// This function resolves the standard data directory (using `dirs::data_dir()`)
/// and targets the "opennow" directory.
/// 
/// It also handles migration from the legacy "gfn-client" directory.
pub fn get_app_data_dir() -> PathBuf {
    let data_dir = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."));
    let app_dir = data_dir.join("opennow");

    // Ensure the target directory exists
    if let Err(e) = fs::create_dir_all(&app_dir) {
        eprintln!("Failed to create app data directory: {}", e);
    }

    // Migration logic
    if let Some(config_dir) = dirs::config_dir() {
        let legacy_dir = config_dir.join("gfn-client");
        if legacy_dir.exists() {
            // Copy auth.json if it doesn't exist in the new location
            let legacy_auth = legacy_dir.join("auth.json");
            let new_auth = app_dir.join("auth.json");
            if legacy_auth.exists() && !new_auth.exists() {
                if let Err(e) = fs::copy(&legacy_auth, &new_auth) {
                    eprintln!("Failed to migrate auth.json: {}", e);
                } else {
                    println!("Migrated auth.json from legacy directory");
                }
            }

            // Copy settings.json if it doesn't exist in the new location
            let legacy_settings = legacy_dir.join("settings.json");
            let new_settings = app_dir.join("settings.json");
            if legacy_settings.exists() && !new_settings.exists() {
                if let Err(e) = fs::copy(&legacy_settings, &new_settings) {
                    eprintln!("Failed to migrate settings.json: {}", e);
                } else {
                    println!("Migrated settings.json from legacy directory");
                }
            }
        }
    }

    app_dir
}
