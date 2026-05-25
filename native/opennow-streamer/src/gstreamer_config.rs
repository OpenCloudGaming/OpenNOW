pub(crate) const EXTERNAL_RENDERER_ENV: &str = "OPENNOW_NATIVE_EXTERNAL_RENDERER";
pub(crate) const NATIVE_VIDEO_API_ENV: &str = "OPENNOW_NATIVE_VIDEO_API";
pub(crate) const NATIVE_VIDEO_BACKEND_ENV: &str = "OPENNOW_NATIVE_VIDEO_BACKEND";
pub(crate) const NATIVE_ZERO_COPY_ENV: &str = "OPENNOW_NATIVE_ZERO_COPY";
pub(crate) const NATIVE_PRESENT_MAX_FPS_ENV: &str = "OPENNOW_NATIVE_PRESENT_MAX_FPS";
pub(crate) const NATIVE_FULLSCREEN_ENV: &str = "OPENNOW_NATIVE_FULLSCREEN";
pub(crate) const NATIVE_D3D_FULLSCREEN_ENV: &str = "OPENNOW_NATIVE_D3D_FULLSCREEN";
pub(crate) const PRESENT_LIMITER_AUTO_SENTINEL: u32 = u32::MAX;

pub(crate) fn use_external_renderer_window() -> bool {
    std::env::var(EXTERNAL_RENDERER_ENV)
        .map(|value| {
            !matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "0" | "false" | "no" | "off"
            )
        })
        .unwrap_or(true)
}

pub(crate) fn requested_video_backend() -> String {
    std::env::var(NATIVE_VIDEO_BACKEND_ENV)
        .or_else(|_| std::env::var(NATIVE_VIDEO_API_ENV))
        .unwrap_or_else(|_| "auto".to_owned())
        .to_ascii_lowercase()
}

pub(crate) fn zero_copy_requested() -> bool {
    matches!(
        std::env::var(NATIVE_ZERO_COPY_ENV)
            .unwrap_or_else(|_| "auto".to_owned())
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes" | "forced"
    )
}

pub(crate) fn resolve_present_max_fps(requested_fps: u32) -> u32 {
    if let Ok(value) = std::env::var(NATIVE_PRESENT_MAX_FPS_ENV) {
        let value = value.trim().to_ascii_lowercase();
        if value == "0" || value == "off" || value == "false" || value == "unlimited" {
            return 0;
        }
        if value == "auto" {
            return PRESENT_LIMITER_AUTO_SENTINEL;
        }
        if let Ok(fps) = value.parse::<u32>() {
            return fps;
        }
    }
    let _ = requested_fps;
    PRESENT_LIMITER_AUTO_SENTINEL
}

pub(crate) fn automatic_present_max_fps(requested_fps: u32, display_hz: Option<u32>) -> u32 {
    display_hz
        .filter(|display_hz| *display_hz >= 30 && *display_hz < requested_fps)
        .unwrap_or(0)
}

pub(crate) fn resolve_native_fullscreen_sink(cloud_gsync_enabled: bool) -> bool {
    if let Ok(value) = std::env::var(NATIVE_FULLSCREEN_ENV) {
        let value = value.trim().to_ascii_lowercase();
        if value == "1" || value == "on" || value == "true" || value == "yes" {
            return true;
        }
        if value == "0" || value == "off" || value == "false" || value == "no" {
            return false;
        }
    }

    if let Ok(value) = std::env::var(NATIVE_D3D_FULLSCREEN_ENV) {
        let value = value.trim().to_ascii_lowercase();
        if value == "1" || value == "on" || value == "true" || value == "yes" {
            return true;
        }
        if value == "0" || value == "off" || value == "false" || value == "no" {
            return false;
        }
    }

    if cfg!(target_os = "linux") {
        true
    } else {
        cloud_gsync_enabled
    }
}
