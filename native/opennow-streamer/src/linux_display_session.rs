#[cfg(target_os = "linux")]
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinuxDisplaySession {
    Wayland,
    X11,
    Unknown,
}

#[cfg(target_os = "linux")]
static DETECTED_SESSION: OnceLock<LinuxDisplaySession> = OnceLock::new();

#[cfg(target_os = "linux")]
pub(crate) fn detect_linux_display_session() -> LinuxDisplaySession {
    *DETECTED_SESSION.get_or_init(resolve_linux_display_session)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn detect_linux_display_session() -> LinuxDisplaySession {
    LinuxDisplaySession::Unknown
}

#[cfg(target_os = "linux")]
fn resolve_linux_display_session() -> LinuxDisplaySession {
    let session_type = std::env::var("XDG_SESSION_TYPE")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let has_wayland = std::env::var("WAYLAND_DISPLAY")
        .ok()
        .is_some_and(|value| !value.trim().is_empty());
    let has_x11 = linux_has_x11_display();

    if session_type == "wayland" || has_wayland {
        LinuxDisplaySession::Wayland
    } else if session_type == "x11" || has_x11 {
        LinuxDisplaySession::X11
    } else {
        LinuxDisplaySession::Unknown
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn linux_has_x11_display() -> bool {
    std::env::var("DISPLAY")
        .ok()
        .is_some_and(|value| !value.trim().is_empty())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn linux_has_x11_display() -> bool {
    false
}

pub(crate) fn linux_display_session_label(session: LinuxDisplaySession) -> &'static str {
    match session {
        LinuxDisplaySession::Wayland => "Wayland",
        LinuxDisplaySession::X11 => "X11",
        LinuxDisplaySession::Unknown => "unknown",
    }
}

const LINUX_WAYLAND_NATIVE_VIDEO_SINKS: &[&str] =
    &["waylandsink", "glimagesink", "ximagesink", "xvimagesink", "autovideosink"];
const LINUX_WAYLAND_XWAYLAND_VIDEO_SINKS: &[&str] =
    &["ximagesink", "waylandsink", "glimagesink", "xvimagesink", "autovideosink"];
const LINUX_X11_NATIVE_VIDEO_SINKS: &[&str] =
    &["ximagesink", "glimagesink", "waylandsink", "xvimagesink", "autovideosink"];
const LINUX_UNKNOWN_NATIVE_VIDEO_SINKS: &[&str] =
    &["glimagesink", "waylandsink", "ximagesink", "xvimagesink", "autovideosink"];

pub(crate) fn linux_native_video_sink_preference() -> &'static [&'static str] {
    match detect_linux_display_session() {
        LinuxDisplaySession::Wayland if linux_has_x11_display() => {
            LINUX_WAYLAND_XWAYLAND_VIDEO_SINKS
        }
        LinuxDisplaySession::Wayland => LINUX_WAYLAND_NATIVE_VIDEO_SINKS,
        LinuxDisplaySession::X11 => LINUX_X11_NATIVE_VIDEO_SINKS,
        LinuxDisplaySession::Unknown => {
            if linux_has_x11_display() {
                LINUX_X11_NATIVE_VIDEO_SINKS
            } else {
                LINUX_UNKNOWN_NATIVE_VIDEO_SINKS
            }
        }
    }
}

pub(crate) fn linux_sink_uses_x11_fullscreen_overlay(sink_factory: &str) -> bool {
    if !linux_has_x11_display() {
        return false;
    }

    match detect_linux_display_session() {
        LinuxDisplaySession::Wayland => matches!(sink_factory, "ximagesink" | "xvimagesink"),
        LinuxDisplaySession::X11 | LinuxDisplaySession::Unknown => {
            matches!(
                sink_factory,
                "ximagesink" | "xvimagesink" | "glimagesink"
            )
        }
    }
}

pub(crate) fn linux_sink_uses_wayland_native_fullscreen(sink_factory: &str) -> bool {
    match detect_linux_display_session() {
        LinuxDisplaySession::Wayland => matches!(
            sink_factory,
            "waylandsink" | "glimagesink" | "vulkansink" | "autovideosink"
        ),
        _ => sink_factory == "waylandsink",
    }
}

pub(crate) fn linux_uses_wayland_native_input_refocus() -> bool {
    matches!(
        detect_linux_display_session(),
        LinuxDisplaySession::Wayland
    )
}

/// Kernel EVIOCGRAB breaks Wayland compositor pointer management (Mutter/KWin),
/// leaving the cursor stuck after focus changes. Games use compositor pointer
/// constraints instead; we read evdev without grab on Wayland sessions.
pub(crate) fn linux_uses_evdev_mouse_grab() -> bool {
    !matches!(
        detect_linux_display_session(),
        LinuxDisplaySession::Wayland
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wayland_prefers_native_wayland_sinks() {
        let expected = match detect_linux_display_session() {
            LinuxDisplaySession::Wayland if linux_has_x11_display() => "ximagesink",
            LinuxDisplaySession::Wayland => "waylandsink",
            LinuxDisplaySession::X11 => "ximagesink",
            LinuxDisplaySession::Unknown => "glimagesink",
        };
        assert_eq!(linux_native_video_sink_preference()[0], expected);
    }

    #[test]
    fn x11_overlay_eligibility_follows_session_and_sink() {
        let session = detect_linux_display_session();
        if session == LinuxDisplaySession::Wayland {
            assert!(!linux_sink_uses_x11_fullscreen_overlay("glimagesink"));
            assert!(!linux_sink_uses_x11_fullscreen_overlay("waylandsink"));
        }
    }
}
