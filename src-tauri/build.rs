fn main() {
    // Only run tauri build when building the Tauri app
    #[cfg(feature = "tauri-app")]
    tauri_build::build();
}
