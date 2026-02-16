//! macOS Raw Input API
//!
//! Provides hardware-level mouse input using Core Graphics event taps.
//! Captures mouse deltas directly for responsive input without OS acceleration effects.
//! Events are coalesced (batched) every 2ms like the official GFN client.
//!
//! Key optimizations:
//! - Lock-free event accumulation using atomics
//! - Local cursor tracking for instant visual feedback

use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, AtomicPtr, Ordering};
use std::ffi::c_void;
use log::{info, error, debug, warn};
use tokio::sync::mpsc;
use parking_lot::Mutex;

use crate::webrtc::InputEvent;
use super::{get_timestamp_us, session_elapsed_us, MOUSE_COALESCE_INTERVAL_US};



// Core Graphics bindings
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    fn CGEventTapCreate(
        tap: CGEventTapLocation,
        place: CGEventTapPlacement,
        options: CGEventTapOptions,
        events_of_interest: CGEventMask,
        callback: CGEventTapCallBack,
        user_info: *mut c_void,
    ) -> CFMachPortRef;

    fn CGEventTapEnable(tap: CFMachPortRef, enable: bool);
    fn CGEventGetIntegerValueField(event: CGEventRef, field: CGEventField) -> i64;
    fn CGEventGetType(event: CGEventRef) -> CGEventType;
    fn CGEventSourceSetLocalEventsSuppressionInterval(source: CGEventSourceRef, seconds: f64);
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFMachPortCreateRunLoopSource(
        allocator: CFAllocatorRef,
        port: CFMachPortRef,
        order: CFIndex,
    ) -> CFRunLoopSourceRef;

    fn CFRunLoopGetCurrent() -> CFRunLoopRef;
    fn CFRunLoopAddSource(rl: CFRunLoopRef, source: CFRunLoopSourceRef, mode: CFStringRef);
    fn CFRunLoopRun();
    fn CFRunLoopStop(rl: CFRunLoopRef);
    fn CFRelease(cf: *const c_void);

    static kCFRunLoopCommonModes: CFStringRef;
    static kCFAllocatorDefault: CFAllocatorRef;
}

// Core Graphics types
type CFMachPortRef = *mut c_void;
type CFRunLoopSourceRef = *mut c_void;
type CFRunLoopRef = *mut c_void;
type CFAllocatorRef = *const c_void;
type CFStringRef = *const c_void;
type CFIndex = isize;
type CGEventRef = *mut c_void;
type CGEventSourceRef = *mut c_void;
type CGEventMask = u64;

type CGEventTapCallBack = extern "C" fn(
    proxy: *mut c_void,
    event_type: CGEventType,
    event: CGEventRef,
    user_info: *mut c_void,
) -> CGEventRef;

#[repr(u32)]
#[derive(Clone, Copy)]
enum CGEventTapLocation {
    HIDEventTap = 0,
    SessionEventTap = 1,
    AnnotatedSessionEventTap = 2,
}

#[repr(u32)]
#[derive(Clone, Copy)]
enum CGEventTapPlacement {
    HeadInsertEventTap = 0,
    TailAppendEventTap = 1,
}

#[repr(u32)]
#[derive(Clone, Copy)]
enum CGEventTapOptions {
    Default = 0,
    ListenOnly = 1,
}

#[repr(u32)]
#[derive(Clone, Copy, PartialEq, Debug)]
enum CGEventType {
    Null = 0,
    LeftMouseDown = 1,
    LeftMouseUp = 2,
    RightMouseDown = 3,
    RightMouseUp = 4,
    MouseMoved = 5,
    LeftMouseDragged = 6,
    RightMouseDragged = 7,
    KeyDown = 10,
    KeyUp = 11,
    FlagsChanged = 12,
    ScrollWheel = 22,
    TabletPointer = 23,
    TabletProximity = 24,
    OtherMouseDown = 25,
    OtherMouseUp = 26,
    OtherMouseDragged = 27,
    TapDisabledByTimeout = 0xFFFFFFFE,
    TapDisabledByUserInput = 0xFFFFFFFF,
}

#[repr(u32)]
#[derive(Clone, Copy)]
enum CGEventField {
    MouseEventDeltaX = 4,
    MouseEventDeltaY = 5,
    ScrollWheelEventDeltaAxis1 = 11,
    KeyboardEventKeycode = 9,
}

// Event masks
const CGMOUSEDOWN_MASK: u64 = (1 << CGEventType::LeftMouseDown as u64)
    | (1 << CGEventType::RightMouseDown as u64)
    | (1 << CGEventType::OtherMouseDown as u64);
const CGMOUSEUP_MASK: u64 = (1 << CGEventType::LeftMouseUp as u64)
    | (1 << CGEventType::RightMouseUp as u64)
    | (1 << CGEventType::OtherMouseUp as u64);
const CGMOUSEMOVED_MASK: u64 = (1 << CGEventType::MouseMoved as u64)
    | (1 << CGEventType::LeftMouseDragged as u64)
    | (1 << CGEventType::RightMouseDragged as u64)
    | (1 << CGEventType::OtherMouseDragged as u64);
const CGSCROLL_MASK: u64 = 1 << CGEventType::ScrollWheel as u64;

// Static state
static RAW_INPUT_REGISTERED: AtomicBool = AtomicBool::new(false);
static RAW_INPUT_ACTIVE: AtomicBool = AtomicBool::new(false);
static ACCUMULATED_DX: AtomicI32 = AtomicI32::new(0);
static ACCUMULATED_DY: AtomicI32 = AtomicI32::new(0);

// Coalescing state
static COALESCE_DX: AtomicI32 = AtomicI32::new(0);
static COALESCE_DY: AtomicI32 = AtomicI32::new(0);
static COALESCE_LAST_SEND_US: AtomicU64 = AtomicU64::new(0);
static COALESCED_EVENT_COUNT: AtomicU64 = AtomicU64::new(0);

// Local cursor tracking
static LOCAL_CURSOR_X: AtomicI32 = AtomicI32::new(960);
static LOCAL_CURSOR_Y: AtomicI32 = AtomicI32::new(540);
static LOCAL_CURSOR_WIDTH: AtomicI32 = AtomicI32::new(1920);
static LOCAL_CURSOR_HEIGHT: AtomicI32 = AtomicI32::new(1080);

// Event sender - use Mutex but minimize lock time
static EVENT_SENDER: Mutex<Option<mpsc::Sender<InputEvent>>> = Mutex::new(None);

// Run loop reference for stopping (use AtomicPtr for thread-safety with raw pointers)
static RUN_LOOP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());
static EVENT_TAP: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());



/// Flush coalesced mouse events
/// Uses blocking lock to ensure events are never dropped (matches Windows behavior)
#[inline]
fn flush_coalesced_events() {
    let dx = COALESCE_DX.swap(0, Ordering::AcqRel);
    let dy = COALESCE_DY.swap(0, Ordering::AcqRel);

    if dx != 0 || dy != 0 {
        let timestamp_us = get_timestamp_us();
        let now_us = session_elapsed_us();
        COALESCE_LAST_SEND_US.store(now_us, Ordering::Release);

        // Log first few flushes to verify input flow
        static FLUSH_LOG_COUNT: AtomicU64 = AtomicU64::new(0);
        let count = FLUSH_LOG_COUNT.fetch_add(1, Ordering::Relaxed);
        if count < 10 {
            info!("Mouse flush #{}: dx={}, dy={}", count, dx, dy);
        }

        // Use blocking lock to match Windows behavior - never drop events
        let guard = EVENT_SENDER.lock();
        if let Some(ref sender) = *guard {
            if sender.try_send(InputEvent::MouseMove {
                dx: dx as i16,
                dy: dy as i16,
                timestamp_us,
            }).is_err() {
                // Channel full - this is a real backpressure situation
                // Log it but don't re-queue (would cause more delays)
                warn!("Input channel full - event dropped");
            }
        } else if count < 5 {
            warn!("EVENT_SENDER is None - raw input sender not configured!");
        }
    }
}

/// Core Graphics event tap callback
extern "C" fn event_tap_callback(
    _proxy: *mut c_void,
    event_type: CGEventType,
    event: CGEventRef,
    _user_info: *mut c_void,
) -> CGEventRef {
    // Handle tap being disabled
    if event_type == CGEventType::TapDisabledByTimeout
        || event_type == CGEventType::TapDisabledByUserInput {
        // Re-enable the tap
        let tap = EVENT_TAP.load(Ordering::Acquire);
        if !tap.is_null() {
            unsafe {
                CGEventTapEnable(tap, true);
            }
        }
        warn!("Event tap was disabled, re-enabling");
        return event;
    }

    if !RAW_INPUT_ACTIVE.load(Ordering::SeqCst) {
        return event;
    }

    unsafe {
        let actual_type = CGEventGetType(event);

        match actual_type {
            CGEventType::MouseMoved
            | CGEventType::LeftMouseDragged
            | CGEventType::RightMouseDragged
            | CGEventType::OtherMouseDragged => {
                // Get raw mouse delta (unaccelerated on modern macOS)
                let dx = CGEventGetIntegerValueField(event, CGEventField::MouseEventDeltaX) as i32;
                let dy = CGEventGetIntegerValueField(event, CGEventField::MouseEventDeltaY) as i32;

                if dx != 0 || dy != 0 {
                    // 1. Update local cursor immediately for visual feedback
                    let width = LOCAL_CURSOR_WIDTH.load(Ordering::Acquire);
                    let height = LOCAL_CURSOR_HEIGHT.load(Ordering::Acquire);
                    let old_x = LOCAL_CURSOR_X.load(Ordering::Acquire);
                    let old_y = LOCAL_CURSOR_Y.load(Ordering::Acquire);
                    LOCAL_CURSOR_X.store((old_x + dx).clamp(0, width), Ordering::Release);
                    LOCAL_CURSOR_Y.store((old_y + dy).clamp(0, height), Ordering::Release);

                    // 2. Accumulate for coalescing
                    COALESCE_DX.fetch_add(dx, Ordering::Relaxed);
                    COALESCE_DY.fetch_add(dy, Ordering::Relaxed);
                    COALESCED_EVENT_COUNT.fetch_add(1, Ordering::Relaxed);

                    // Also accumulate for legacy API
                    ACCUMULATED_DX.fetch_add(dx, Ordering::Relaxed);
                    ACCUMULATED_DY.fetch_add(dy, Ordering::Relaxed);

                    // 3. Check if enough time to send batch
                    let now_us = session_elapsed_us();
                    let last_us = COALESCE_LAST_SEND_US.load(Ordering::Acquire);

                    if now_us.saturating_sub(last_us) >= MOUSE_COALESCE_INTERVAL_US {
                        flush_coalesced_events();
                    }
                }
            }
            CGEventType::ScrollWheel => {
                let delta = CGEventGetIntegerValueField(event, CGEventField::ScrollWheelEventDeltaAxis1) as i16;
                if delta != 0 {
                    let timestamp_us = get_timestamp_us();
                    // Use try_lock to avoid blocking the event tap callback
                    if let Some(guard) = EVENT_SENDER.try_lock() {
                        if let Some(ref sender) = *guard {
                            // macOS scroll is inverted compared to Windows, and uses different scale
                            // Multiply by 120 to match Windows WHEEL_DELTA
                            let _ = sender.try_send(InputEvent::MouseWheel {
                                delta: delta * 120,
                                timestamp_us,
                            });
                        }
                    }
                    // Note: scroll events dropped if lock contention, acceptable for wheel
                }
            }
            _ => {}
        }
    }

    event
}



/// Start raw input capture
pub fn start_raw_input() -> Result<(), String> {
    if RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
        RAW_INPUT_ACTIVE.store(true, Ordering::SeqCst);
        info!("Raw input resumed");
        return Ok(());
    }

    // Spawn thread for event tap run loop
    std::thread::spawn(|| {
        unsafe {
            // Create event tap for mouse events
            let event_mask: CGEventMask = CGMOUSEMOVED_MASK | CGSCROLL_MASK;

            let tap = CGEventTapCreate(
                CGEventTapLocation::HIDEventTap,  // Capture at HID level for raw input
                CGEventTapPlacement::HeadInsertEventTap,
                CGEventTapOptions::ListenOnly,  // Don't modify events
                event_mask,
                event_tap_callback,
                std::ptr::null_mut(),
            );

            if tap.is_null() {
                error!("Failed to create event tap. Make sure Accessibility permissions are granted in System Preferences > Security & Privacy > Privacy > Accessibility");
                return;
            }

            EVENT_TAP.store(tap, Ordering::Release);

            // Create run loop source
            let source = CFMachPortCreateRunLoopSource(
                kCFAllocatorDefault,
                tap,
                0,
            );

            if source.is_null() {
                error!("Failed to create run loop source");
                CFRelease(tap);
                EVENT_TAP.store(std::ptr::null_mut(), Ordering::Release);
                return;
            }

            // Get current run loop and add source
            let run_loop = CFRunLoopGetCurrent();
            RUN_LOOP.store(run_loop, Ordering::Release);

            CFRunLoopAddSource(run_loop, source, kCFRunLoopCommonModes);

            // Enable the tap
            CGEventTapEnable(tap, true);

            RAW_INPUT_REGISTERED.store(true, Ordering::SeqCst);
            RAW_INPUT_ACTIVE.store(true, Ordering::SeqCst);
            info!("Raw input started - capturing mouse events via CGEventTap");

            // Flush timer DISABLED - causes lock contention latency

            // Run the loop (blocks until stopped)
            CFRunLoopRun();

            // Cleanup
            CGEventTapEnable(tap, false);
            CFRelease(source);
            CFRelease(tap);

            RAW_INPUT_REGISTERED.store(false, Ordering::SeqCst);
            RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
            EVENT_TAP.store(std::ptr::null_mut(), Ordering::Release);
            RUN_LOOP.store(std::ptr::null_mut(), Ordering::Release);
            info!("Raw input thread stopped");
        }
    });

    // Wait for initialization
    std::thread::sleep(std::time::Duration::from_millis(100));

    if RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err("Failed to start raw input. Check Accessibility permissions.".to_string())
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
    RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);

    // Stop the run loop
    let run_loop = RUN_LOOP.swap(std::ptr::null_mut(), Ordering::AcqRel);
    if !run_loop.is_null() {
        unsafe {
            CFRunLoopStop(run_loop);
        }
    }

    // Wait for the thread to actually exit (up to 500ms)
    // This prevents race conditions when starting a new session immediately
    let start = std::time::Instant::now();
    while RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
        if start.elapsed() > std::time::Duration::from_millis(500) {
            error!("Raw input thread did not exit in time, forcing reset");
            RAW_INPUT_REGISTERED.store(false, Ordering::SeqCst);
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    // Clear the event sender to avoid stale channel issues
    clear_raw_input_sender();

    info!("Raw input stopped");
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

/// Update center position (no-op on macOS, kept for API compatibility)
pub fn update_raw_input_center() {
    // macOS doesn't need cursor recentering with CGEventTap
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

/// Set local cursor dimensions
pub fn set_local_cursor_dimensions(width: u32, height: u32) {
    LOCAL_CURSOR_WIDTH.store(width as i32, Ordering::Release);
    LOCAL_CURSOR_HEIGHT.store(height as i32, Ordering::Release);
    LOCAL_CURSOR_X.store(width as i32 / 2, Ordering::Release);
    LOCAL_CURSOR_Y.store(height as i32 / 2, Ordering::Release);
    info!("Local cursor dimensions set to {}x{}", width, height);
}

/// Get local cursor position
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

/// Flush pending coalesced mouse events
pub fn flush_pending_mouse_events() {
    flush_coalesced_events();
}

/// Get count of coalesced events
pub fn get_coalesced_event_count() -> u64 {
    COALESCED_EVENT_COUNT.load(Ordering::Relaxed)
}

/// Reset coalescing state
pub fn reset_coalescing() {
    COALESCE_DX.store(0, Ordering::Release);
    COALESCE_DY.store(0, Ordering::Release);
    COALESCE_LAST_SEND_US.store(0, Ordering::Release);
    COALESCED_EVENT_COUNT.store(0, Ordering::Release);
    // Center cursor based on actual dimensions, not hardcoded values
    let width = LOCAL_CURSOR_WIDTH.load(Ordering::Acquire);
    let height = LOCAL_CURSOR_HEIGHT.load(Ordering::Acquire);
    LOCAL_CURSOR_X.store(width / 2, Ordering::Release);
    LOCAL_CURSOR_Y.store(height / 2, Ordering::Release);
}
