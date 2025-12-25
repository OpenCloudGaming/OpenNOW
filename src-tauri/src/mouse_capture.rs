// Native mouse capture - bypasses browser's pointer lock and its "press Esc" message
// Uses Windows raw input API to capture mouse movements

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::Mutex;

#[cfg(windows)]
use windows::Win32::Foundation::{HWND, POINT, RECT};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    ClipCursor, GetCursorPos, SetCursorPos, ShowCursor, GetForegroundWindow,
    GetWindowRect,
};

#[cfg(windows)]
static MOUSE_CAPTURED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static LAST_CURSOR_POS: Mutex<Option<(i32, i32)>> = Mutex::new(None);
#[cfg(windows)]
static CENTER_POS: Mutex<Option<(i32, i32)>> = Mutex::new(None);

#[cfg(windows)]
pub fn capture_mouse(capture: bool) -> Result<(), String> {
    unsafe {
        if capture {
            // Get the foreground window
            let hwnd: HWND = GetForegroundWindow();
            if hwnd.0.is_null() {
                return Err("No foreground window".to_string());
            }

            // Get window rect
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &mut rect).is_err() {
                return Err("Failed to get window rect".to_string());
            }

            // Calculate center of window
            let center_x = (rect.left + rect.right) / 2;
            let center_y = (rect.top + rect.bottom) / 2;

            // Store center position
            *CENTER_POS.lock().unwrap() = Some((center_x, center_y));

            // Get current cursor position
            let mut pos = POINT::default();
            let _ = GetCursorPos(&mut pos);
            *LAST_CURSOR_POS.lock().unwrap() = Some((pos.x, pos.y));

            // Clip cursor to window
            let _ = ClipCursor(Some(&rect));

            // Hide cursor
            ShowCursor(false);

            // Move cursor to center
            let _ = SetCursorPos(center_x, center_y);

            MOUSE_CAPTURED.store(true, Ordering::SeqCst);
            log::info!("Mouse captured natively, center: ({}, {})", center_x, center_y);
            Ok(())
        } else {
            // Restore cursor
            if let Some((x, y)) = LAST_CURSOR_POS.lock().unwrap().take() {
                let _ = SetCursorPos(x, y);
            }

            // Unclip cursor
            let _ = ClipCursor(None);

            // Show cursor
            ShowCursor(true);

            MOUSE_CAPTURED.store(false, Ordering::SeqCst);
            *CENTER_POS.lock().unwrap() = None;
            log::info!("Mouse released");
            Ok(())
        }
    }
}

#[cfg(windows)]
pub fn get_mouse_delta() -> Option<(i32, i32)> {
    if !MOUSE_CAPTURED.load(Ordering::SeqCst) {
        return None;
    }

    unsafe {
        let center = CENTER_POS.lock().unwrap();
        let (center_x, center_y) = center.as_ref()?;

        let mut pos = POINT::default();
        if GetCursorPos(&mut pos).is_err() {
            return None;
        }

        let delta_x = pos.x - center_x;
        let delta_y = pos.y - center_y;

        // Reset cursor to center if it moved
        if delta_x != 0 || delta_y != 0 {
            let _ = SetCursorPos(*center_x, *center_y);
        }

        Some((delta_x, delta_y))
    }
}

#[cfg(windows)]
pub fn is_mouse_captured() -> bool {
    MOUSE_CAPTURED.load(Ordering::SeqCst)
}

// Non-Windows stubs
#[cfg(not(windows))]
pub fn capture_mouse(_capture: bool) -> Result<(), String> {
    Err("Native mouse capture not supported on this platform".to_string())
}

#[cfg(not(windows))]
pub fn get_mouse_delta() -> Option<(i32, i32)> {
    None
}

#[cfg(not(windows))]
pub fn is_mouse_captured() -> bool {
    false
}

// Tauri commands
#[tauri::command]
pub fn set_mouse_capture(capture: bool) -> Result<(), String> {
    capture_mouse(capture)
}

#[tauri::command]
pub fn get_native_mouse_delta() -> Option<(i32, i32)> {
    get_mouse_delta()
}

#[tauri::command]
pub fn is_native_mouse_captured() -> bool {
    is_mouse_captured()
}
