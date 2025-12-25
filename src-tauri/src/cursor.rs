//! Native cursor/mouse capture for macOS
//! Uses Core Graphics APIs to properly capture mouse input

use tauri::command;

#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(target_os = "macos")]
static CURSOR_CAPTURED: AtomicBool = AtomicBool::new(false);

#[cfg(target_os = "macos")]
mod macos {
    use core_graphics::display::{CGDisplay, CGPoint};
    use core_graphics::event::{CGEvent, CGEventType};
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    #[link(name = "CoreGraphics", kind = "framework")]
    extern "C" {
        fn CGAssociateMouseAndMouseCursorPosition(connected: bool) -> i32;
        fn CGDisplayHideCursor(display: u32) -> i32;
        fn CGDisplayShowCursor(display: u32) -> i32;
        fn CGWarpMouseCursorPosition(point: CGPoint) -> i32;
    }

    /// Disassociate mouse from cursor position (allows unlimited movement)
    pub fn set_mouse_cursor_association(associated: bool) -> bool {
        unsafe {
            CGAssociateMouseAndMouseCursorPosition(associated) == 0
        }
    }

    /// Hide the cursor on the main display
    pub fn hide_cursor() -> bool {
        unsafe {
            CGDisplayHideCursor(CGDisplay::main().id) == 0
        }
    }

    /// Show the cursor on the main display
    pub fn show_cursor() -> bool {
        unsafe {
            CGDisplayShowCursor(CGDisplay::main().id) == 0
        }
    }

    /// Warp cursor to center of main display
    pub fn center_cursor() -> bool {
        let display = CGDisplay::main();
        let bounds = display.bounds();
        let center = CGPoint::new(
            bounds.origin.x + bounds.size.width / 2.0,
            bounds.origin.y + bounds.size.height / 2.0,
        );
        unsafe {
            CGWarpMouseCursorPosition(center) == 0
        }
    }

    /// Warp cursor to a specific position
    pub fn warp_cursor(x: f64, y: f64) -> bool {
        let point = CGPoint::new(x, y);
        unsafe {
            CGWarpMouseCursorPosition(point) == 0
        }
    }
}

/// Capture the mouse cursor (hide cursor and allow unlimited movement)
/// This uses macOS Core Graphics to properly disassociate the mouse from cursor position
#[command]
pub async fn capture_cursor() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        if CURSOR_CAPTURED.load(Ordering::SeqCst) {
            return Ok(true); // Already captured
        }

        // First, center the cursor
        macos::center_cursor();

        // Hide the cursor
        if !macos::hide_cursor() {
            return Err("Failed to hide cursor".to_string());
        }

        // Disassociate mouse from cursor position (this is the key!)
        // This allows the mouse to move infinitely without hitting screen edges
        if !macos::set_mouse_cursor_association(false) {
            macos::show_cursor(); // Restore cursor on failure
            return Err("Failed to disassociate mouse from cursor".to_string());
        }

        CURSOR_CAPTURED.store(true, Ordering::SeqCst);
        log::info!("Cursor captured (macOS native)");
        Ok(true)
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On other platforms, return false to indicate native capture not available
        // The frontend should use browser pointer lock instead
        Ok(false)
    }
}

/// Release the mouse cursor (show cursor and restore normal behavior)
#[command]
pub async fn release_cursor() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        if !CURSOR_CAPTURED.load(Ordering::SeqCst) {
            return Ok(true); // Already released
        }

        // Re-associate mouse with cursor position
        macos::set_mouse_cursor_association(true);

        // Show the cursor
        macos::show_cursor();

        // Center cursor so it appears in a reasonable position
        macos::center_cursor();

        CURSOR_CAPTURED.store(false, Ordering::SeqCst);
        log::info!("Cursor released (macOS native)");
        Ok(true)
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(true)
    }
}

/// Check if cursor is currently captured
#[command]
pub async fn is_cursor_captured() -> Result<bool, String> {
    #[cfg(target_os = "macos")]
    {
        Ok(CURSOR_CAPTURED.load(Ordering::SeqCst))
    }

    #[cfg(not(target_os = "macos"))]
    {
        Ok(false)
    }
}
