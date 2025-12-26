//! Native cursor/mouse capture for macOS and Windows
//! Uses Core Graphics APIs (macOS) or Win32 APIs (Windows) to properly capture mouse input

use tauri::command;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

static CURSOR_CAPTURED: AtomicBool = AtomicBool::new(false);

// High-frequency mouse polling state
static MOUSE_POLLING_ACTIVE: AtomicBool = AtomicBool::new(false);
static ACCUMULATED_DX: AtomicI32 = AtomicI32::new(0);
static ACCUMULATED_DY: AtomicI32 = AtomicI32::new(0);

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

#[cfg(target_os = "windows")]
mod windows {
    use std::ptr::null_mut;
    use std::mem::zeroed;
    use std::sync::atomic::{AtomicI32, AtomicIsize, AtomicBool, Ordering};

    // Store window center for recentering
    pub static CENTER_X: AtomicI32 = AtomicI32::new(0);
    pub static CENTER_Y: AtomicI32 = AtomicI32::new(0);
    // Store the original cursor to restore later
    pub static ORIGINAL_CURSOR: AtomicIsize = AtomicIsize::new(0);
    // Store original mouse acceleration settings
    pub static ACCEL_DISABLED: AtomicBool = AtomicBool::new(false);
    static mut ORIGINAL_MOUSE_PARAMS: [i32; 3] = [0, 0, 0];

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct POINT {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    #[derive(Copy, Clone)]
    struct RECT {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    type HWND = *mut std::ffi::c_void;
    type HCURSOR = *mut std::ffi::c_void;
    type LONG_PTR = isize;

    const GCLP_HCURSOR: i32 = -12;
    const IDC_ARROW: *const u16 = 32512 as *const u16;

    // SystemParametersInfo constants for mouse acceleration
    const SPI_GETMOUSE: u32 = 0x0003;
    const SPI_SETMOUSE: u32 = 0x0004;
    const SPIF_SENDCHANGE: u32 = 0x0002;

    #[link(name = "user32")]
    extern "system" {
        fn GetCursorPos(lpPoint: *mut POINT) -> i32;
        fn SetCursorPos(x: i32, y: i32) -> i32;
        fn ShowCursor(bShow: i32) -> i32;
        fn ClipCursor(lpRect: *const RECT) -> i32;
        fn GetForegroundWindow() -> HWND;
        fn GetWindowRect(hWnd: HWND, lpRect: *mut RECT) -> i32;
        fn SetCursor(hCursor: HCURSOR) -> HCURSOR;
        fn GetClientRect(hWnd: HWND, lpRect: *mut RECT) -> i32;
        fn ClientToScreen(hWnd: HWND, lpPoint: *mut POINT) -> i32;
        fn GetClassLongPtrW(hWnd: HWND, nIndex: i32) -> LONG_PTR;
        fn SetClassLongPtrW(hWnd: HWND, nIndex: i32, dwNewLong: LONG_PTR) -> LONG_PTR;
        fn LoadCursorW(hInstance: *mut std::ffi::c_void, lpCursorName: *const u16) -> HCURSOR;
        fn SystemParametersInfoW(uiAction: u32, uiParam: u32, pvParam: *mut std::ffi::c_void, fWinIni: u32) -> i32;
    }

    /// Disable Windows mouse acceleration (Enhance pointer precision)
    /// Stores original settings to restore later
    pub fn disable_mouse_acceleration() {
        if ACCEL_DISABLED.load(Ordering::SeqCst) {
            return; // Already disabled
        }

        unsafe {
            // Get current mouse parameters [threshold1, threshold2, acceleration]
            let mut params: [i32; 3] = [0, 0, 0];
            if SystemParametersInfoW(SPI_GETMOUSE, 0, params.as_mut_ptr() as *mut _, 0) != 0 {
                // Save original settings
                ORIGINAL_MOUSE_PARAMS = params;

                // Disable acceleration by setting acceleration to 0
                // params[2] is the acceleration flag (0 = disabled, 1 = enabled)
                if params[2] != 0 {
                    let new_params: [i32; 3] = [0, 0, 0]; // Disable acceleration
                    if SystemParametersInfoW(SPI_SETMOUSE, 0, new_params.as_ptr() as *mut _, SPIF_SENDCHANGE) != 0 {
                        ACCEL_DISABLED.store(true, Ordering::SeqCst);
                        log::info!("Mouse acceleration disabled (was: {:?})", ORIGINAL_MOUSE_PARAMS);
                    }
                } else {
                    log::info!("Mouse acceleration already disabled");
                }
            }
        }
    }

    /// Restore original Windows mouse acceleration settings
    pub fn restore_mouse_acceleration() {
        if !ACCEL_DISABLED.load(Ordering::SeqCst) {
            return; // Not disabled by us
        }

        unsafe {
            if SystemParametersInfoW(SPI_SETMOUSE, 0, ORIGINAL_MOUSE_PARAMS.as_ptr() as *mut _, SPIF_SENDCHANGE) != 0 {
                ACCEL_DISABLED.store(false, Ordering::SeqCst);
                log::info!("Mouse acceleration restored to: {:?}", ORIGINAL_MOUSE_PARAMS);
            }
        }
    }

    /// Hide the cursor completely by setting class cursor to NULL
    pub fn hide_cursor() {
        unsafe {
            let hwnd = GetForegroundWindow();
            if !hwnd.is_null() {
                // Save original cursor
                let original = GetClassLongPtrW(hwnd, GCLP_HCURSOR);
                if original != 0 {
                    ORIGINAL_CURSOR.store(original, Ordering::SeqCst);
                }
                // Set class cursor to NULL - this prevents cursor from flickering back
                SetClassLongPtrW(hwnd, GCLP_HCURSOR, 0);
            }
            // Also set current cursor to NULL
            SetCursor(null_mut());
            // Decrement show counter
            let mut count = ShowCursor(0);
            while count >= 0 {
                count = ShowCursor(0);
            }
        }
    }

    /// Show the cursor by restoring the class cursor
    pub fn show_cursor() {
        unsafe {
            let hwnd = GetForegroundWindow();
            if !hwnd.is_null() {
                // Restore original cursor or use arrow
                let original = ORIGINAL_CURSOR.load(Ordering::SeqCst);
                if original != 0 {
                    SetClassLongPtrW(hwnd, GCLP_HCURSOR, original);
                } else {
                    // Load default arrow cursor
                    let arrow = LoadCursorW(null_mut(), IDC_ARROW);
                    SetClassLongPtrW(hwnd, GCLP_HCURSOR, arrow as LONG_PTR);
                }
            }
            // Increment counter until visible
            let mut count = ShowCursor(1);
            while count < 0 {
                count = ShowCursor(1);
            }
        }
    }

    /// Clip cursor to the foreground window
    pub fn clip_cursor_to_window() -> bool {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return false;
            }
            let mut rect: RECT = zeroed();
            if GetWindowRect(hwnd, &mut rect) == 0 {
                return false;
            }
            ClipCursor(&rect) != 0
        }
    }

    /// Release cursor clipping
    pub fn release_clip() -> bool {
        unsafe {
            ClipCursor(null_mut()) != 0
        }
    }

    /// Get current cursor position
    pub fn get_cursor_pos() -> Option<(i32, i32)> {
        unsafe {
            let mut point: POINT = zeroed();
            if GetCursorPos(&mut point) != 0 {
                Some((point.x, point.y))
            } else {
                None
            }
        }
    }

    /// Set cursor position
    pub fn set_cursor_pos(x: i32, y: i32) -> bool {
        unsafe {
            SetCursorPos(x, y) != 0
        }
    }

    /// Get window client area center (screen coordinates)
    pub fn get_window_center() -> Option<(i32, i32)> {
        unsafe {
            let hwnd = GetForegroundWindow();
            if hwnd.is_null() {
                return None;
            }
            let mut client_rect: RECT = zeroed();
            if GetClientRect(hwnd, &mut client_rect) == 0 {
                return None;
            }
            // Get center of client area
            let mut center = POINT {
                x: client_rect.right / 2,
                y: client_rect.bottom / 2,
            };
            // Convert to screen coordinates
            if ClientToScreen(hwnd, &mut center) == 0 {
                return None;
            }
            Some((center.x, center.y))
        }
    }

    /// Update stored center position
    pub fn update_center() -> bool {
        if let Some((x, y)) = get_window_center() {
            CENTER_X.store(x, Ordering::SeqCst);
            CENTER_Y.store(y, Ordering::SeqCst);
            true
        } else {
            false
        }
    }

    /// Get stored center position
    pub fn get_stored_center() -> (i32, i32) {
        (CENTER_X.load(Ordering::SeqCst), CENTER_Y.load(Ordering::SeqCst))
    }

    /// Center cursor in window
    pub fn center_cursor() -> bool {
        let (cx, cy) = get_stored_center();
        if cx != 0 && cy != 0 {
            set_cursor_pos(cx, cy)
        } else if let Some((x, y)) = get_window_center() {
            CENTER_X.store(x, Ordering::SeqCst);
            CENTER_Y.store(y, Ordering::SeqCst);
            set_cursor_pos(x, y)
        } else {
            false
        }
    }

    /// Get mouse delta from center and recenter cursor
    /// Returns (dx, dy) - the movement since last center
    pub fn get_delta_and_recenter() -> (i32, i32) {
        let (cx, cy) = get_stored_center();
        if cx == 0 && cy == 0 {
            return (0, 0);
        }

        if let Some((x, y)) = get_cursor_pos() {
            let dx = x - cx;
            let dy = y - cy;

            // Only recenter if there was movement
            if dx != 0 || dy != 0 {
                set_cursor_pos(cx, cy);
                // Hide cursor again after repositioning
                unsafe { SetCursor(null_mut()); }
            }

            (dx, dy)
        } else {
            (0, 0)
        }
    }
}

/// Capture the mouse cursor (hide cursor and allow unlimited movement)
/// Uses native OS APIs: Core Graphics on macOS, Win32 on Windows
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

    #[cfg(target_os = "windows")]
    {
        if CURSOR_CAPTURED.load(Ordering::SeqCst) {
            return Ok(true); // Already captured
        }

        // Update and store window center
        if !windows::update_center() {
            return Err("Failed to get window center".to_string());
        }

        // Disable mouse acceleration for 1:1 raw input
        windows::disable_mouse_acceleration();

        // Center the cursor
        windows::center_cursor();

        // Hide the cursor
        windows::hide_cursor();

        // Clip cursor to window to prevent it from going to other monitors
        windows::clip_cursor_to_window();

        CURSOR_CAPTURED.store(true, Ordering::SeqCst);
        log::info!("Cursor captured (Windows native with recentering, acceleration disabled)");
        Ok(true)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        // On other platforms, return false to indicate native capture not available
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

    #[cfg(target_os = "windows")]
    {
        if !CURSOR_CAPTURED.load(Ordering::SeqCst) {
            return Ok(true); // Already released
        }

        // Restore mouse acceleration settings
        windows::restore_mouse_acceleration();

        // Release cursor clipping
        windows::release_clip();

        // Show the cursor
        windows::show_cursor();

        // Center cursor so it appears in a reasonable position
        windows::center_cursor();

        CURSOR_CAPTURED.store(false, Ordering::SeqCst);
        log::info!("Cursor released (Windows native)");
        Ok(true)
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        Ok(true)
    }
}

/// Check if cursor is currently captured
#[command]
pub async fn is_cursor_captured() -> Result<bool, String> {
    Ok(CURSOR_CAPTURED.load(Ordering::SeqCst))
}

/// Get mouse delta from center and recenter cursor (Windows only)
/// Returns (dx, dy) - the movement since cursor was last at center
/// This enables FPS-style infinite mouse movement
#[command]
pub fn get_mouse_delta() -> (i32, i32) {
    #[cfg(target_os = "windows")]
    {
        if !CURSOR_CAPTURED.load(Ordering::SeqCst) {
            return (0, 0);
        }
        windows::get_delta_and_recenter()
    }

    #[cfg(not(target_os = "windows"))]
    {
        (0, 0)
    }
}

/// Recenter cursor without getting delta (useful after window resize)
#[command]
pub fn recenter_cursor() -> bool {
    #[cfg(target_os = "windows")]
    {
        if !CURSOR_CAPTURED.load(Ordering::SeqCst) {
            return false;
        }
        // Update center position (in case window moved/resized)
        windows::update_center();
        windows::center_cursor()
    }

    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Start high-frequency mouse polling (Windows only)
/// Polls at ~1000Hz and accumulates deltas for the frontend to read
#[command]
pub fn start_mouse_polling() -> bool {
    #[cfg(target_os = "windows")]
    {
        if MOUSE_POLLING_ACTIVE.load(Ordering::SeqCst) {
            return true; // Already running
        }
        if !CURSOR_CAPTURED.load(Ordering::SeqCst) {
            return false; // Need cursor captured first
        }

        MOUSE_POLLING_ACTIVE.store(true, Ordering::SeqCst);
        ACCUMULATED_DX.store(0, Ordering::SeqCst);
        ACCUMULATED_DY.store(0, Ordering::SeqCst);

        // Spawn high-frequency polling thread
        std::thread::spawn(|| {
            use std::time::{Duration, Instant};

            // Poll at ~1000Hz (1ms intervals)
            let poll_interval = Duration::from_micros(1000);

            while MOUSE_POLLING_ACTIVE.load(Ordering::SeqCst) &&
                  CURSOR_CAPTURED.load(Ordering::SeqCst) {
                let start = Instant::now();

                // Get delta and recenter
                let (dx, dy) = windows::get_delta_and_recenter();

                // Accumulate deltas
                if dx != 0 {
                    ACCUMULATED_DX.fetch_add(dx, Ordering::SeqCst);
                }
                if dy != 0 {
                    ACCUMULATED_DY.fetch_add(dy, Ordering::SeqCst);
                }

                // Sleep for remaining time in interval
                let elapsed = start.elapsed();
                if elapsed < poll_interval {
                    std::thread::sleep(poll_interval - elapsed);
                }
            }

            MOUSE_POLLING_ACTIVE.store(false, Ordering::SeqCst);
            log::info!("Mouse polling thread stopped");
        });

        log::info!("High-frequency mouse polling started (1000Hz)");
        true
    }

    #[cfg(not(target_os = "windows"))]
    {
        false
    }
}

/// Stop high-frequency mouse polling
#[command]
pub fn stop_mouse_polling() {
    MOUSE_POLLING_ACTIVE.store(false, Ordering::SeqCst);
    ACCUMULATED_DX.store(0, Ordering::SeqCst);
    ACCUMULATED_DY.store(0, Ordering::SeqCst);
}

/// Get accumulated mouse deltas and reset accumulators
/// Returns (dx, dy) accumulated since last call
#[command]
pub fn get_accumulated_mouse_delta() -> (i32, i32) {
    let dx = ACCUMULATED_DX.swap(0, Ordering::SeqCst);
    let dy = ACCUMULATED_DY.swap(0, Ordering::SeqCst);
    (dx, dy)
}

/// Check if mouse polling is active
#[command]
pub fn is_mouse_polling_active() -> bool {
    MOUSE_POLLING_ACTIVE.load(Ordering::SeqCst)
}
