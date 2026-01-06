//! Linux Raw Input API
//!
//! Provides hardware-level mouse input using evdev (direct device access).
//! Captures mouse deltas directly from input devices for responsive input without
//! desktop acceleration effects. Falls back to X11 XInput2 for Wayland compatibility.
//!
//! Events are coalesced (batched) every 2ms like the official GFN client.
//!
//! Key optimizations:
//! - Lock-free event accumulation using atomics
//! - Local cursor tracking for instant visual feedback
//! - Direct evdev access for lowest latency (requires input group membership)
//! - X11 XInput2 fallback for unprivileged access

use log::{debug, error, info, warn};
use parking_lot::Mutex;
use std::ffi::CString;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use tokio::sync::mpsc;

use crate::input::{get_timestamp_us, session_elapsed_us, MOUSE_COALESCE_INTERVAL_US};
use crate::webrtc::InputEvent;

// evdev bindings
use evdev::{Device, InputEventKind, RelativeAxisType};

// X11 bindings for fallback
use x11::xinput2 as xi2;
use x11::xlib;

// Static state
static RAW_INPUT_REGISTERED: AtomicBool = AtomicBool::new(false);
static RAW_INPUT_ACTIVE: AtomicBool = AtomicBool::new(false);
static ACCUMULATED_DX: AtomicI32 = AtomicI32::new(0);
static ACCUMULATED_DY: AtomicI32 = AtomicI32::new(0);
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

// Coalescing state - accumulates events for 2ms batches (like official GFN client)
static COALESCE_DX: AtomicI32 = AtomicI32::new(0);
static COALESCE_DY: AtomicI32 = AtomicI32::new(0);
static COALESCE_LAST_SEND_US: AtomicU64 = AtomicU64::new(0);
static COALESCED_EVENT_COUNT: AtomicU64 = AtomicU64::new(0);

// Local cursor tracking for instant visual feedback (updated on every event)
static LOCAL_CURSOR_X: AtomicI32 = AtomicI32::new(960);
static LOCAL_CURSOR_Y: AtomicI32 = AtomicI32::new(540);
static LOCAL_CURSOR_WIDTH: AtomicI32 = AtomicI32::new(1920);
static LOCAL_CURSOR_HEIGHT: AtomicI32 = AtomicI32::new(1080);

// Direct event sender for immediate mouse events
static EVENT_SENDER: Mutex<Option<mpsc::Sender<InputEvent>>> = Mutex::new(None);

// Input backend type
#[derive(Debug, Clone, Copy, PartialEq)]
enum InputBackend {
    Evdev,
    X11,
    None,
}

static ACTIVE_BACKEND: Mutex<InputBackend> = Mutex::new(InputBackend::None);

/// Flush coalesced mouse events - sends accumulated deltas if any
#[inline]
fn flush_coalesced_events() {
    let dx = COALESCE_DX.swap(0, Ordering::AcqRel);
    let dy = COALESCE_DY.swap(0, Ordering::AcqRel);

    if dx != 0 || dy != 0 {
        let timestamp_us = get_timestamp_us();
        let now_us = session_elapsed_us();
        COALESCE_LAST_SEND_US.store(now_us, Ordering::Release);

        let guard = EVENT_SENDER.lock();
        if let Some(ref sender) = *guard {
            let _ = sender.try_send(InputEvent::MouseMove {
                dx: dx as i16,
                dy: dy as i16,
                timestamp_us,
            });
        }
    }
}

/// Process mouse delta from any backend
#[inline]
fn process_mouse_delta(dx: i32, dy: i32) {
    if dx == 0 && dy == 0 {
        return;
    }

    // 1. Update local cursor IMMEDIATELY for instant visual feedback
    let width = LOCAL_CURSOR_WIDTH.load(Ordering::Acquire);
    let height = LOCAL_CURSOR_HEIGHT.load(Ordering::Acquire);
    let old_x = LOCAL_CURSOR_X.load(Ordering::Acquire);
    let old_y = LOCAL_CURSOR_Y.load(Ordering::Acquire);
    LOCAL_CURSOR_X.store((old_x + dx).clamp(0, width), Ordering::Release);
    LOCAL_CURSOR_Y.store((old_y + dy).clamp(0, height), Ordering::Release);

    // 2. Accumulate delta for coalescing
    COALESCE_DX.fetch_add(dx, Ordering::Relaxed);
    COALESCE_DY.fetch_add(dy, Ordering::Relaxed);
    COALESCED_EVENT_COUNT.fetch_add(1, Ordering::Relaxed);

    // Also accumulate for legacy API
    ACCUMULATED_DX.fetch_add(dx, Ordering::Relaxed);
    ACCUMULATED_DY.fetch_add(dy, Ordering::Relaxed);

    // 3. Check if enough time has passed to send batch (2ms default)
    let now_us = session_elapsed_us();
    let last_us = COALESCE_LAST_SEND_US.load(Ordering::Acquire);

    if now_us.saturating_sub(last_us) >= MOUSE_COALESCE_INTERVAL_US {
        flush_coalesced_events();
    }
}

/// Process scroll wheel event
fn process_scroll(delta: i32) {
    if delta == 0 {
        return;
    }

    let timestamp_us = get_timestamp_us();
    let guard = EVENT_SENDER.lock();
    if let Some(ref sender) = *guard {
        // Linux scroll is typically 1 unit per notch, Windows uses 120
        // Scale to match Windows WHEEL_DELTA
        let _ = sender.try_send(InputEvent::MouseWheel {
            delta: (delta * 120) as i16,
            timestamp_us,
        });
    }
}

/// Find the primary mouse device in /dev/input/
fn find_mouse_device() -> Option<String> {
    // Try common mouse device paths
    let candidates = [
        "/dev/input/mice",   // Combined mice device
        "/dev/input/mouse0", // First mouse
        "/dev/input/event0", // First event device (may be mouse)
    ];

    // First, try to find a proper mouse via evdev enumeration
    if let Ok(entries) = std::fs::read_dir("/dev/input") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("event") {
                    if let Ok(device) = Device::open(&path) {
                        // Check if this device has relative axes (mouse)
                        if device.supported_relative_axes().map_or(false, |axes| {
                            axes.contains(RelativeAxisType::REL_X)
                                && axes.contains(RelativeAxisType::REL_Y)
                        }) {
                            let device_name = device.name().unwrap_or("Unknown");
                            // Skip virtual/tablet devices
                            let name_lower = device_name.to_lowercase();
                            if !name_lower.contains("tablet")
                                && !name_lower.contains("touch")
                                && !name_lower.contains("wacom")
                            {
                                info!("Found mouse device: {} ({})", path.display(), device_name);
                                return Some(path.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    // Fallback to known paths
    for path in &candidates {
        if Path::new(path).exists() {
            info!("Using fallback mouse device: {}", path);
            return Some(path.to_string());
        }
    }

    None
}

/// evdev input thread - direct device access for lowest latency
fn start_evdev_input(device_path: &str) -> Result<(), String> {
    let device = Device::open(device_path)
        .map_err(|e| format!("Failed to open evdev device {}: {}", device_path, e))?;

    let device_name = device.name().unwrap_or("Unknown").to_string();
    info!("evdev: Opened device '{}' at {}", device_name, device_path);

    // Grab the device for exclusive access (optional - may fail on some systems)
    // This prevents other applications from receiving the events
    if let Err(e) = device.grab() {
        warn!(
            "evdev: Could not grab device exclusively: {} (continuing anyway)",
            e
        );
    }

    // Mark as registered before spawning thread
    RAW_INPUT_REGISTERED.store(true, Ordering::SeqCst);
    RAW_INPUT_ACTIVE.store(true, Ordering::SeqCst);
    *ACTIVE_BACKEND.lock() = InputBackend::Evdev;

    let device_path_owned = device_path.to_string();
    std::thread::spawn(move || {
        info!("evdev input thread started for {}", device_path_owned);

        // Re-open in the thread to avoid Send issues
        let mut device = match Device::open(&device_path_owned) {
            Ok(d) => d,
            Err(e) => {
                error!("evdev: Failed to reopen device: {}", e);
                RAW_INPUT_REGISTERED.store(false, Ordering::SeqCst);
                RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
                return;
            }
        };

        // Event loop
        loop {
            if STOP_REQUESTED.load(Ordering::SeqCst) {
                break;
            }

            // Fetch events (blocking with timeout would be better, but evdev crate
            // doesn't support that directly - we use a small sleep instead)
            match device.fetch_events() {
                Ok(events) => {
                    if !RAW_INPUT_ACTIVE.load(Ordering::SeqCst) {
                        continue;
                    }

                    for event in events {
                        match event.kind() {
                            InputEventKind::RelAxis(axis) => {
                                let value = event.value();
                                match axis {
                                    RelativeAxisType::REL_X => {
                                        process_mouse_delta(value, 0);
                                    }
                                    RelativeAxisType::REL_Y => {
                                        process_mouse_delta(0, value);
                                    }
                                    RelativeAxisType::REL_WHEEL
                                    | RelativeAxisType::REL_WHEEL_HI_RES => {
                                        process_scroll(value);
                                    }
                                    _ => {}
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    // EAGAIN is normal for non-blocking reads
                    if e.raw_os_error() != Some(libc::EAGAIN) {
                        debug!("evdev: Error reading events: {}", e);
                    }
                    // Small sleep to prevent busy-waiting
                    std::thread::sleep(std::time::Duration::from_micros(100));
                }
            }
        }

        // Cleanup
        let _ = device.ungrab();
        RAW_INPUT_REGISTERED.store(false, Ordering::SeqCst);
        RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
        info!("evdev input thread stopped");
    });

    Ok(())
}

/// X11 XInput2 input thread - fallback for when evdev isn't available
fn start_x11_input() -> Result<(), String> {
    unsafe {
        // Open X display
        let display = xlib::XOpenDisplay(std::ptr::null());
        if display.is_null() {
            return Err("Failed to open X11 display".to_string());
        }

        let root = xlib::XDefaultRootWindow(display);

        // Check for XInput2 extension
        let mut xi_opcode = 0;
        let mut event = 0;
        let mut error = 0;
        let xinput_name = CString::new("XInputExtension").unwrap();

        if xlib::XQueryExtension(
            display,
            xinput_name.as_ptr(),
            &mut xi_opcode,
            &mut event,
            &mut error,
        ) == 0
        {
            xlib::XCloseDisplay(display);
            return Err("XInput2 extension not available".to_string());
        }

        // Query XInput2 version (need at least 2.0)
        let mut major = 2;
        let mut minor = 0;
        if xi2::XIQueryVersion(display, &mut major, &mut minor) != xlib::Success as i32 {
            xlib::XCloseDisplay(display);
            return Err(format!("XInput2 version {}.{} not supported", major, minor));
        }

        info!("X11: Using XInput2 version {}.{}", major, minor);

        // Select raw motion events
        let mut mask: [u8; 4] = [0; 4];
        xi2::XISetMask(&mut mask, xi2::XI_RawMotion);
        xi2::XISetMask(&mut mask, xi2::XI_RawButtonPress);
        xi2::XISetMask(&mut mask, xi2::XI_RawButtonRelease);

        let mut evmask = xi2::XIEventMask {
            deviceid: xi2::XIAllMasterDevices,
            mask_len: mask.len() as i32,
            mask: mask.as_mut_ptr(),
        };

        if xi2::XISelectEvents(display, root, &mut evmask, 1) != xlib::Success as i32 {
            xlib::XCloseDisplay(display);
            return Err("Failed to select XInput2 events".to_string());
        }

        // Mark as registered
        RAW_INPUT_REGISTERED.store(true, Ordering::SeqCst);
        RAW_INPUT_ACTIVE.store(true, Ordering::SeqCst);
        *ACTIVE_BACKEND.lock() = InputBackend::X11;

        // Spawn event thread
        std::thread::spawn(move || {
            info!("X11 XInput2 input thread started");

            let xi_opcode = xi_opcode; // Move into closure

            loop {
                if STOP_REQUESTED.load(Ordering::SeqCst) {
                    break;
                }

                // Check for pending events
                while xlib::XPending(display) > 0 {
                    let mut event: xlib::XEvent = std::mem::zeroed();
                    xlib::XNextEvent(display, &mut event);

                    // Check if this is a GenericEvent (XInput2 events)
                    if event.get_type() == xlib::GenericEvent {
                        let cookie = &mut event.generic_event_cookie;

                        if xlib::XGetEventData(display, cookie) != 0 {
                            if cookie.extension == xi_opcode {
                                if RAW_INPUT_ACTIVE.load(Ordering::SeqCst) {
                                    match cookie.evtype {
                                        xi2::XI_RawMotion => {
                                            let raw = cookie.data as *const xi2::XIRawEvent;
                                            if !raw.is_null() {
                                                let raw_event = &*raw;

                                                // Extract raw values (unaccelerated)
                                                let valuators = raw_event.raw_values;
                                                let mask = raw_event.valuators.mask;
                                                let mask_len = raw_event.valuators.mask_len;

                                                let mut dx = 0.0f64;
                                                let mut dy = 0.0f64;
                                                let mut idx = 0;

                                                // Iterate through set bits in mask
                                                for i in 0..(mask_len * 8) {
                                                    let byte_idx = (i / 8) as usize;
                                                    let bit_idx = i % 8;

                                                    if byte_idx < mask_len as usize {
                                                        let mask_byte = *mask.add(byte_idx);
                                                        if (mask_byte & (1 << bit_idx)) != 0 {
                                                            let value = *valuators.add(idx);
                                                            match i {
                                                                0 => dx = value,
                                                                1 => dy = value,
                                                                _ => {}
                                                            }
                                                            idx += 1;
                                                        }
                                                    }
                                                }

                                                if dx != 0.0 || dy != 0.0 {
                                                    process_mouse_delta(dx as i32, dy as i32);
                                                }
                                            }
                                        }
                                        _ => {}
                                    }
                                }
                            }
                            xlib::XFreeEventData(display, cookie);
                        }
                    }
                }

                // Small sleep when no events pending
                std::thread::sleep(std::time::Duration::from_micros(500));
            }

            // Cleanup
            xlib::XCloseDisplay(display);
            RAW_INPUT_REGISTERED.store(false, Ordering::SeqCst);
            RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
            info!("X11 XInput2 input thread stopped");
        });

        Ok(())
    }
}

/// Start raw input capture
/// Tries evdev first (lowest latency), falls back to X11 XInput2
pub fn start_raw_input() -> Result<(), String> {
    // If already registered AND active, just return success
    if RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
        if RAW_INPUT_ACTIVE.load(Ordering::SeqCst) {
            info!("Raw input already active");
            return Ok(());
        }
        // Re-activating existing registration
        ACCUMULATED_DX.store(0, Ordering::SeqCst);
        ACCUMULATED_DY.store(0, Ordering::SeqCst);
        COALESCE_DX.store(0, Ordering::SeqCst);
        COALESCE_DY.store(0, Ordering::SeqCst);
        COALESCE_LAST_SEND_US.store(0, Ordering::SeqCst);
        RAW_INPUT_ACTIVE.store(true, Ordering::SeqCst);
        info!("Raw input resumed with clean state");
        return Ok(());
    }

    // Reset state
    STOP_REQUESTED.store(false, Ordering::SeqCst);
    ACCUMULATED_DX.store(0, Ordering::SeqCst);
    ACCUMULATED_DY.store(0, Ordering::SeqCst);
    COALESCE_DX.store(0, Ordering::SeqCst);
    COALESCE_DY.store(0, Ordering::SeqCst);
    COALESCE_LAST_SEND_US.store(0, Ordering::SeqCst);
    COALESCED_EVENT_COUNT.store(0, Ordering::SeqCst);

    // Try evdev first (requires user to be in 'input' group or root)
    if let Some(device_path) = find_mouse_device() {
        match start_evdev_input(&device_path) {
            Ok(()) => {
                // Wait for thread to start
                std::thread::sleep(std::time::Duration::from_millis(50));
                if RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
                    info!("Raw input started via evdev - lowest latency mode");
                    return Ok(());
                }
            }
            Err(e) => {
                warn!("evdev failed: {} - trying X11 fallback", e);
            }
        }
    } else {
        warn!("No mouse device found for evdev - trying X11 fallback");
    }

    // Fall back to X11 XInput2 (not available on headless Pi)
    match start_x11_input() {
        Ok(()) => {
            std::thread::sleep(std::time::Duration::from_millis(50));
            if RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
                info!("Raw input started via X11 XInput2");
                return Ok(());
            }
            Err("X11 input thread failed to start".to_string())
        }
        Err(e) => {
            // Check if running on Raspberry Pi for better error message
            let is_pi = Path::new("/sys/firmware/devicetree/base/model").exists()
                && std::fs::read_to_string("/sys/firmware/devicetree/base/model")
                    .map(|s| s.to_lowercase().contains("raspberry pi"))
                    .unwrap_or(false);

            if is_pi {
                error!(
                    "Input setup failed on Raspberry Pi. Please add your user to the 'input' group:\n\
                     sudo usermod -aG input $USER\n\
                     Then log out and back in."
                );
            } else {
                error!(
                    "All input backends failed. evdev requires 'input' group membership. X11 error: {}",
                    e
                );
            }
            Err(format!("Failed to start raw input: {}", e))
        }
    }
}

/// Pause raw input capture
pub fn pause_raw_input() {
    RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
    ACCUMULATED_DX.store(0, Ordering::SeqCst);
    ACCUMULATED_DY.store(0, Ordering::SeqCst);
    debug!("Raw input paused");
}

/// Resume raw input capture
pub fn resume_raw_input() {
    if RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
        ACCUMULATED_DX.store(0, Ordering::SeqCst);
        ACCUMULATED_DY.store(0, Ordering::SeqCst);
        RAW_INPUT_ACTIVE.store(true, Ordering::SeqCst);
        debug!("Raw input resumed");
    }
}

/// Stop raw input completely
pub fn stop_raw_input() {
    // Signal thread to stop
    STOP_REQUESTED.store(true, Ordering::SeqCst);
    RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);

    // Clear the event sender
    clear_raw_input_sender();

    // Reset state
    ACCUMULATED_DX.store(0, Ordering::SeqCst);
    ACCUMULATED_DY.store(0, Ordering::SeqCst);
    COALESCE_DX.store(0, Ordering::SeqCst);
    COALESCE_DY.store(0, Ordering::SeqCst);
    COALESCE_LAST_SEND_US.store(0, Ordering::SeqCst);

    // Reset local cursor to center
    let width = LOCAL_CURSOR_WIDTH.load(Ordering::Acquire);
    let height = LOCAL_CURSOR_HEIGHT.load(Ordering::Acquire);
    LOCAL_CURSOR_X.store(width / 2, Ordering::SeqCst);
    LOCAL_CURSOR_Y.store(height / 2, Ordering::SeqCst);

    // Wait for thread to stop
    let start = std::time::Instant::now();
    while RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
        if start.elapsed() > std::time::Duration::from_millis(1000) {
            error!("Raw input thread did not exit in time, forcing reset");
            RAW_INPUT_REGISTERED.store(false, Ordering::SeqCst);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    *ACTIVE_BACKEND.lock() = InputBackend::None;
    std::thread::sleep(std::time::Duration::from_millis(50));
    info!("Raw input stopped and fully cleaned up");
}

/// Get accumulated mouse deltas and reset
pub fn get_raw_mouse_delta() -> (i32, i32) {
    let dx = ACCUMULATED_DX.swap(0, Ordering::SeqCst);
    let dy = ACCUMULATED_DY.swap(0, Ordering::SeqCst);
    (dx, dy)
}

/// Check if raw input is active
pub fn is_raw_input_active() -> bool {
    RAW_INPUT_ACTIVE.load(Ordering::SeqCst)
}

/// Update center position (no-op on Linux with evdev, kept for API compatibility)
pub fn update_raw_input_center() {
    // Linux evdev provides pure relative motion, no recentering needed
}

/// Set the event sender for direct mouse event delivery
pub fn set_raw_input_sender(sender: mpsc::Sender<InputEvent>) {
    let mut guard = EVENT_SENDER.lock();
    *guard = Some(sender);
    info!("Raw input direct sender configured");
}

/// Clear the event sender
pub fn clear_raw_input_sender() {
    let mut guard = EVENT_SENDER.lock();
    *guard = None;
}

/// Set local cursor dimensions (call when stream starts or resolution changes)
pub fn set_local_cursor_dimensions(width: u32, height: u32) {
    LOCAL_CURSOR_WIDTH.store(width as i32, Ordering::Release);
    LOCAL_CURSOR_HEIGHT.store(height as i32, Ordering::Release);
    // Center cursor when dimensions change
    LOCAL_CURSOR_X.store(width as i32 / 2, Ordering::Release);
    LOCAL_CURSOR_Y.store(height as i32 / 2, Ordering::Release);
    info!("Local cursor dimensions set to {}x{}", width, height);
}

/// Get local cursor position (for rendering)
pub fn get_local_cursor_position() -> (i32, i32) {
    (
        LOCAL_CURSOR_X.load(Ordering::Acquire),
        LOCAL_CURSOR_Y.load(Ordering::Acquire),
    )
}

/// Get local cursor position normalized (0.0-1.0)
pub fn get_local_cursor_normalized() -> (f32, f32) {
    let x = LOCAL_CURSOR_X.load(Ordering::Acquire) as f32;
    let y = LOCAL_CURSOR_Y.load(Ordering::Acquire) as f32;
    let w = LOCAL_CURSOR_WIDTH.load(Ordering::Acquire) as f32;
    let h = LOCAL_CURSOR_HEIGHT.load(Ordering::Acquire) as f32;
    (x / w.max(1.0), y / h.max(1.0))
}

/// Flush any pending coalesced mouse events
pub fn flush_pending_mouse_events() {
    flush_coalesced_events();
}

/// Get count of coalesced events (for stats)
pub fn get_coalesced_event_count() -> u64 {
    COALESCED_EVENT_COUNT.load(Ordering::Relaxed)
}

/// Reset coalescing state (call when streaming stops)
pub fn reset_coalescing() {
    COALESCE_DX.store(0, Ordering::Release);
    COALESCE_DY.store(0, Ordering::Release);
    COALESCE_LAST_SEND_US.store(0, Ordering::Release);
    COALESCED_EVENT_COUNT.store(0, Ordering::Release);
    // Center cursor based on actual dimensions
    let width = LOCAL_CURSOR_WIDTH.load(Ordering::Acquire);
    let height = LOCAL_CURSOR_HEIGHT.load(Ordering::Acquire);
    LOCAL_CURSOR_X.store(width / 2, Ordering::Release);
    LOCAL_CURSOR_Y.store(height / 2, Ordering::Release);
}

/// Get the active input backend name (for debugging)
pub fn get_active_backend_name() -> &'static str {
    match *ACTIVE_BACKEND.lock() {
        InputBackend::Evdev => "evdev",
        InputBackend::X11 => "X11 XInput2",
        InputBackend::None => "none",
    }
}
