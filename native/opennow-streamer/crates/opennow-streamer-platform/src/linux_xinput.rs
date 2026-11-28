use std::collections::HashSet;
use std::ffi::c_void;
use std::ptr::null;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use x11_dl::{xinput2, xlib};

use crate::media::{CapturedInput, CapturedInputQueue};

struct XInputState {
    enabled: AtomicBool,
    pressed_buttons: Mutex<HashSet<u8>>,
    captured_input: Arc<CapturedInputQueue>,
}

/// Dedicated XInput2 raw-mouse controller matching the official Linux GFN
/// client. It owns a separate X connection and blocks in poll independently
/// of Vulkan presentation, so FIFO WSI back-pressure can never delay input.
pub(crate) struct LinuxXInputController {
    state: Arc<XInputState>,
    stop_fd: libc::c_int,
    worker: Option<JoinHandle<()>>,
}

impl LinuxXInputController {
    pub(crate) fn start(captured_input: Arc<CapturedInputQueue>) -> Result<Self, String> {
        let stop_fd = unsafe { libc::eventfd(0, libc::EFD_CLOEXEC | libc::EFD_NONBLOCK) };
        if stop_fd < 0 {
            return Err(format!(
                "failed to create XInput2 wake event: {}",
                std::io::Error::last_os_error()
            ));
        }
        let state = Arc::new(XInputState {
            enabled: AtomicBool::new(false),
            pressed_buttons: Mutex::new(HashSet::new()),
            captured_input,
        });
        let thread_state = Arc::clone(&state);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let worker = match thread::Builder::new()
            .name("opennow-xinput2".to_owned())
            .spawn(move || run_xinput_thread(thread_state, stop_fd, ready_sender))
        {
            Ok(worker) => worker,
            Err(error) => {
                unsafe { libc::close(stop_fd) };
                return Err(format!("failed to spawn XInput2 thread: {error}"));
            }
        };
        match ready_receiver.recv() {
            Ok(Ok(())) => Ok(Self {
                state,
                stop_fd,
                worker: Some(worker),
            }),
            Ok(Err(error)) => {
                let _ = worker.join();
                unsafe { libc::close(stop_fd) };
                Err(error)
            }
            Err(_) => {
                let _ = worker.join();
                unsafe { libc::close(stop_fd) };
                Err("XInput2 thread exited during initialization".to_owned())
            }
        }
    }

    pub(crate) fn set_enabled(&self, enabled: bool) {
        if self.state.enabled.swap(enabled, Ordering::AcqRel) != enabled && !enabled {
            release_pressed_buttons(&self.state);
        }
    }
}

impl Drop for LinuxXInputController {
    fn drop(&mut self) {
        self.state.enabled.store(false, Ordering::Release);
        release_pressed_buttons(&self.state);
        let wake = 1_u64;
        unsafe {
            let _ = libc::write(
                self.stop_fd,
                (&wake as *const u64).cast::<c_void>(),
                std::mem::size_of::<u64>(),
            );
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        unsafe { libc::close(self.stop_fd) };
    }
}

fn run_xinput_thread(
    state: Arc<XInputState>,
    stop_fd: libc::c_int,
    ready: mpsc::SyncSender<Result<(), String>>,
) {
    let xlib = match xlib::Xlib::open() {
        Ok(xlib) => xlib,
        Err(error) => {
            let _ = ready.send(Err(format!(
                "could not load bundled X11 interface: {error}"
            )));
            return;
        }
    };
    let xi = match xinput2::XInput2::open() {
        Ok(xi) => xi,
        Err(error) => {
            let _ = ready.send(Err(format!("could not load XInput2: {error}")));
            return;
        }
    };
    let display = unsafe { (xlib.XOpenDisplay)(null()) };
    if display.is_null() {
        let _ = ready.send(Err("XInput2 could not open DISPLAY".to_owned()));
        return;
    }

    let result = unsafe { initialize_xinput(&xlib, &xi, display) };
    let opcode = match result {
        Ok(opcode) => opcode,
        Err(error) => {
            unsafe { (xlib.XCloseDisplay)(display) };
            let _ = ready.send(Err(error));
            return;
        }
    };
    let connection_fd = unsafe { (xlib.XConnectionNumber)(display) };
    if connection_fd < 0 {
        unsafe { (xlib.XCloseDisplay)(display) };
        let _ = ready.send(Err("XInput2 returned an invalid X11 connection".to_owned()));
        return;
    }
    let _ = ready.send(Ok(()));

    let mut residual_x = 0.0_f64;
    let mut residual_y = 0.0_f64;
    // While a button is held this X server may stop delivering raw events, so the pointer
    // position is polled as a fallback source of motion (see reconcile_pressed_buttons).
    let mut drag = DragFallback::default();
    // Some X servers (seen on FreeBSD) deliver RawButtonPress but not every
    // RawButtonRelease, which leaves a button stuck in the stream. Check the real
    // button state of the master pointer and synthesize any missing release.
    let root = unsafe { (xlib.XDefaultRootWindow)(display) };
    let mut pointer_id = 2;
    let mut client_pointer = 0;
    if unsafe { (xi.XIGetClientPointer)(display, 0, &mut client_pointer) } != 0
        && client_pointer > 0
    {
        pointer_id = client_pointer;
    }
    let mut poll_fds = [
        libc::pollfd {
            fd: connection_fd,
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: stop_fd,
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    loop {
        let buttons_held = state
            .pressed_buttons
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .len()
            > 0;
        // A round trip to the server (see reconcile_pressed_buttons) can move events from
        // the socket into Xlib's own queue, where poll() cannot see them.
        let queued = unsafe { (xlib.XPending)(display) } > 0;
        let poll_timeout = if queued {
            0
        } else if buttons_held {
            4
        } else {
            -1
        };
        let poll_result =
            unsafe { libc::poll(poll_fds.as_mut_ptr(), poll_fds.len() as _, poll_timeout) };
        if poll_result < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            break;
        }
        if poll_fds[1].revents & libc::POLLIN != 0 {
            break;
        }
        let x_revents = poll_fds[0].revents;
        if !queued
            && x_revents & (libc::POLLIN | libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) == 0
        {
            unsafe {
                reconcile_pressed_buttons(
                    &state, &xlib, &xi, display, root, pointer_id, opcode, &mut drag,
                )
            };
            continue;
        }
        while unsafe { (xlib.XPending)(display) } > 0 {
            let mut event = xlib::XEvent { pad: [0; 24] };
            unsafe { (xlib.XNextEvent)(display, &mut event) };
            if event.get_type() != xlib::GenericEvent {
                continue;
            }
            let cookie = unsafe { &mut event.generic_event_cookie };
            if cookie.extension != opcode || unsafe { (xlib.XGetEventData)(display, cookie) } == 0 {
                continue;
            }
            let mut button_event = None;
            if !cookie.data.is_null() {
                let raw = unsafe { &*(cookie.data.cast::<xinput2::XIRawEvent>()) };
                match raw.evtype {
                    xinput2::XI_RawButtonPress => button_event = Some((raw.detail, true)),
                    xinput2::XI_RawButtonRelease => button_event = Some((raw.detail, false)),
                    _ => {
                        if process_raw_event(&state, raw, &mut residual_x, &mut residual_y) {
                            drag.motion_seen = true;
                        }
                    }
                }
            }
            unsafe { (xlib.XFreeEventData)(display, cookie) };
            if let Some((detail, pressed)) = button_event {
                // Raw events carry the physical button while XIQueryPointer reports logical
                // buttons, so both go through the pointer button map before they are compared.
                // Reading the map is a round trip, which is why it happens after the event
                // data has been released.
                let map = unsafe { pointer_button_map(&xlib, display) };
                if state.enabled.load(Ordering::Acquire) {
                    if !pressed {
                        // Movement the pointer made that raw motion did not carry must be sent
                        // before this release clears the drag tracking.
                        unsafe {
                            reconcile_pressed_buttons(
                                &state, &xlib, &xi, display, root, pointer_id, opcode, &mut drag,
                            )
                        };
                    }
                    process_button(&state, logical_button(&map, detail), pressed);
                }
            }
        }
        unsafe {
            reconcile_pressed_buttons(
                &state, &xlib, &xi, display, root, pointer_id, opcode, &mut drag,
            )
        };
        if x_revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
            break;
        }
    }
    unsafe { (xlib.XCloseDisplay)(display) };
}

unsafe fn initialize_xinput(
    xlib: &xlib::Xlib,
    xi: &xinput2::XInput2,
    display: *mut xlib::Display,
) -> Result<libc::c_int, String> {
    let mut opcode = 0;
    let mut first_event = 0;
    let mut first_error = 0;
    if unsafe {
        (xlib.XQueryExtension)(
            display,
            c"XInputExtension".as_ptr(),
            &mut opcode,
            &mut first_event,
            &mut first_error,
        )
    } == 0
    {
        return Err("XInput2 extension is unavailable".to_owned());
    }
    let mut major = 2;
    let mut minor = 0;
    if unsafe { (xi.XIQueryVersion)(display, &mut major, &mut minor) } != 0 || major < 2 {
        return Err(format!(
            "XInput2 2.0 is required (server reported {major}.{minor})"
        ));
    }
    let mut mask = [0_u8; 4];
    xinput2::XISetMask(&mut mask, xinput2::XI_RawButtonPress);
    xinput2::XISetMask(&mut mask, xinput2::XI_RawButtonRelease);
    xinput2::XISetMask(&mut mask, xinput2::XI_RawMotion);
    let mut event_mask = xinput2::XIEventMask {
        deviceid: xinput2::XIAllMasterDevices,
        mask_len: mask.len() as libc::c_int,
        mask: mask.as_mut_ptr(),
    };
    let root = unsafe { (xlib.XDefaultRootWindow)(display) };
    if unsafe { (xi.XISelectEvents)(display, root, &mut event_mask, 1) } != 0 {
        return Err("XInput2 could not select raw mouse events".to_owned());
    }
    unsafe { (xlib.XFlush)(display) };
    eprintln!("Dedicated Linux XInput2 raw mouse thread ready (XI {major}.{minor})");
    Ok(opcode)
}

fn process_raw_event(
    state: &XInputState,
    raw: &xinput2::XIRawEvent,
    residual_x: &mut f64,
    residual_y: &mut f64,
) -> bool {
    if !state.enabled.load(Ordering::Acquire) {
        return false;
    }
    match raw.evtype {
        xinput2::XI_RawMotion => {
            let (x, y) = raw_motion(raw);
            *residual_x += x;
            *residual_y += y;
            let delta_x = residual_x.trunc() as i32;
            let delta_y = residual_y.trunc() as i32;
            *residual_x -= f64::from(delta_x);
            *residual_y -= f64::from(delta_y);
            push_mouse_delta(&state.captured_input, delta_x, delta_y);
            return true;
        }
        // Button events are handled by the event loop (see run_xinput_thread).
        _ => {}
    }
    false
}

fn raw_motion(raw: &xinput2::XIRawEvent) -> (f64, f64) {
    let valuators = raw.valuators;
    if valuators.mask_len <= 0 || valuators.mask.is_null() || raw.raw_values.is_null() {
        return (0.0, 0.0);
    }
    let mask =
        unsafe { std::slice::from_raw_parts(valuators.mask, valuators.mask_len.max(0) as usize) };
    let mut value_index = 0_usize;
    let mut result = (0.0, 0.0);
    for axis in 0..valuators.mask_len.saturating_mul(8) {
        if !xinput2::XIMaskIsSet(mask, axis) {
            continue;
        }
        let value = unsafe { *raw.raw_values.add(value_index) };
        value_index += 1;
        match axis {
            0 => result.0 = value,
            1 => result.1 = value,
            _ => {}
        }
    }
    result
}

/// Wheel clicks arrive as buttons 4 and 5 (vertical) and 6 and 7 (horizontal), press only.
fn wheel_delta(detail: libc::c_int) -> Option<(i16, i16)> {
    match detail {
        4 => Some((0, 120)),
        5 => Some((0, -120)),
        6 => Some((-120, 0)),
        7 => Some((120, 0)),
        _ => None,
    }
}

fn process_button(state: &XInputState, detail: libc::c_int, pressed: bool) {
    if pressed {
        if let Some((delta_x, delta_y)) = wheel_delta(detail) {
            state
                .captured_input
                .push(CapturedInput::MouseWheel { delta_x, delta_y });
            return;
        }
    } else if matches!(detail, 4..=7) {
        return;
    }
    let button = match detail {
        1 => 1,
        2 => 2,
        3 => 3,
        8 => 4,
        9 => 5,
        _ => return,
    };
    let mut pressed_buttons = state
        .pressed_buttons
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !state.enabled.load(Ordering::Acquire) {
        return;
    }
    let changed = if pressed {
        pressed_buttons.insert(button)
    } else {
        pressed_buttons.remove(&button)
    };
    if changed {
        state
            .captured_input
            .push(CapturedInput::MouseButton { button, pressed });
    }
}

/// The core pointer's button map: entry `physical - 1` holds the logical button number.
unsafe fn pointer_button_map(xlib: &xlib::Xlib, display: *mut xlib::Display) -> Vec<u8> {
    let mut map = [0_u8; 256];
    let count =
        unsafe { (xlib.XGetPointerMapping)(display, map.as_mut_ptr(), map.len() as libc::c_int) };
    map[..count.clamp(0, map.len() as libc::c_int) as usize].to_vec()
}

/// Converts the physical button of a raw event to the logical button the server reports in
/// XIQueryPointer. Identity maps and buttons outside the map are unchanged; 0 marks a
/// disabled button.
fn logical_button(map: &[u8], detail: libc::c_int) -> libc::c_int {
    usize::try_from(detail)
        .ok()
        .and_then(|detail| detail.checked_sub(1))
        .and_then(|index| map.get(index))
        .map_or(detail, |logical| libc::c_int::from(*logical))
}

/// What one XIQueryPointer call reports.
struct PointerSample {
    /// Pressed buttons (logical button numbers mapped to the stream's button numbers).
    buttons: HashSet<u8>,
    /// Root-window position.
    x: f64,
    y: f64,
    /// The top-level window under the pointer (0 over the root window).
    child: xlib::Window,
}

/// Reads the master pointer's pressed buttons and position.
unsafe fn query_pointer(
    xlib: &xlib::Xlib,
    xi: &xinput2::XInput2,
    display: *mut xlib::Display,
    root: xlib::Window,
    pointer_id: libc::c_int,
) -> Option<PointerSample> {
    let mut root_return = 0;
    let mut child_return = 0;
    let (mut root_x, mut root_y, mut win_x, mut win_y) = (0.0_f64, 0.0_f64, 0.0_f64, 0.0_f64);
    let mut buttons = xinput2::XIButtonState {
        mask_len: 0,
        mask: std::ptr::null_mut(),
    };
    let mut modifiers = xinput2::XIModifierState::default();
    let mut group = xinput2::XIModifierState::default();
    if unsafe {
        (xi.XIQueryPointer)(
            display,
            pointer_id,
            root,
            &mut root_return,
            &mut child_return,
            &mut root_x,
            &mut root_y,
            &mut win_x,
            &mut win_y,
            &mut buttons,
            &mut modifiers,
            &mut group,
        )
    } == 0
    {
        if !buttons.mask.is_null() {
            unsafe { (xlib.XFree)(buttons.mask.cast()) };
        }
        return None;
    }
    let mut down = HashSet::new();
    if !buttons.mask.is_null() {
        let mask = unsafe {
            std::slice::from_raw_parts(buttons.mask, buttons.mask_len.max(0) as usize)
        };
        // X button number -> the stream's button number, matching process_button.
        for (detail, mapped) in [(1_usize, 1_u8), (2, 2), (3, 3), (8, 4), (9, 5)] {
            if mask
                .get(detail / 8)
                .is_some_and(|byte| byte & (1 << (detail % 8)) != 0)
            {
                down.insert(mapped);
            }
        }
        unsafe { (xlib.XFree)(buttons.mask.cast()) };
    }
    Some(PointerSample {
        buttons: down,
        x: root_x,
        y: root_y,
        child: child_return,
    })
}

/// Root-relative bounds the pointer can move in while a drag is captured.
#[derive(Clone, Copy, Debug, PartialEq)]
struct CaptureBounds {
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
}

impl CaptureBounds {
    const EDGE_MARGIN: f64 = 64.0;

    /// The bounds of `window` (a child of the root) clipped to the screen, or the whole screen
    /// when the window is unknown or too small to recenter inside.
    fn of_window(window: Option<(f64, f64, f64, f64)>, screen_width: f64, screen_height: f64) -> Self {
        let screen = Self {
            left: 0.0,
            top: 0.0,
            right: screen_width,
            bottom: screen_height,
        };
        let Some((x, y, width, height)) = window else {
            return screen;
        };
        let bounds = Self {
            left: x.max(0.0),
            top: y.max(0.0),
            right: (x + width).min(screen_width),
            bottom: (y + height).min(screen_height),
        };
        if bounds.right - bounds.left < 4.0 * Self::EDGE_MARGIN
            || bounds.bottom - bounds.top < 4.0 * Self::EDGE_MARGIN
        {
            screen
        } else {
            bounds
        }
    }

    fn center(&self) -> (f64, f64) {
        ((self.left + self.right) / 2.0, (self.top + self.bottom) / 2.0)
    }

    /// Whether the pointer is close enough to an edge that further movement would be clipped.
    fn near_edge(&self, x: f64, y: f64) -> bool {
        x < self.left + Self::EDGE_MARGIN
            || y < self.top + Self::EDGE_MARGIN
            || x > self.right - Self::EDGE_MARGIN
            || y > self.bottom - Self::EDGE_MARGIN
    }
}

/// The bounds the pointer is captured in: the top-level window under it (the capture window,
/// which also confines the pointer when SDL grabs it), clipped to the screen.
unsafe fn capture_bounds(
    xlib: &xlib::Xlib,
    display: *mut xlib::Display,
    child: xlib::Window,
) -> CaptureBounds {
    let screen = unsafe { (xlib.XDefaultScreen)(display) };
    let screen_width = f64::from(unsafe { (xlib.XDisplayWidth)(display, screen) });
    let screen_height = f64::from(unsafe { (xlib.XDisplayHeight)(display, screen) });
    let mut geometry = None;
    if child != 0 {
        let mut root_return = 0;
        let (mut x, mut y) = (0, 0);
        let (mut width, mut height, mut border, mut depth) = (0_u32, 0_u32, 0_u32, 0_u32);
        if unsafe {
            (xlib.XGetGeometry)(
                display,
                child,
                &mut root_return,
                &mut x,
                &mut y,
                &mut width,
                &mut height,
                &mut border,
                &mut depth,
            )
        } != 0
        {
            geometry = Some((f64::from(x), f64::from(y), f64::from(width), f64::from(height)));
        }
    }
    CaptureBounds::of_window(geometry, screen_width, screen_height)
}

/// State of the pointer-position stand-in for raw events while a button is held.
#[derive(Default)]
struct DragFallback {
    last_position: Option<(f64, f64)>,
    /// Raw motion was handled since the last sample, so it already carries the movement.
    motion_seen: bool,
    // Pointer-position deltas are in screen pixels, raw motion in device counts; the sub-pixel
    // remainders are kept apart from the raw path's.
    residual_x: f64,
    residual_y: f64,
}

/// Where the movement between two pointer samples comes from.
#[derive(Debug, PartialEq)]
enum FallbackMotion {
    /// Raw motion was already handled for it.
    RawCarried,
    /// Raw motion for it is waiting in Xlib's queue and will be handled next.
    RawQueued,
    /// Nothing raw covers it: send the pointer position change as a delta.
    Pointer,
    /// First sample of a drag: nothing to compare against yet.
    Start,
}

fn fallback_motion(motion_seen: bool, raw_motion_queued: bool, tracking: bool) -> FallbackMotion {
    if motion_seen {
        FallbackMotion::RawCarried
    } else if raw_motion_queued {
        FallbackMotion::RawQueued
    } else if tracking {
        FallbackMotion::Pointer
    } else {
        FallbackMotion::Start
    }
}

struct RawMotionScan {
    opcode: libc::c_int,
    found: bool,
}

/// XCheckIfEvent predicate that only looks: it never claims an event, so the queue is untouched.
unsafe extern "C" fn scan_for_raw_motion(
    _display: *mut xlib::Display,
    event: *mut xlib::XEvent,
    arg: *mut libc::c_char,
) -> libc::c_int {
    let scan = unsafe { &mut *arg.cast::<RawMotionScan>() };
    let event = unsafe { &*event };
    if event.get_type() == xlib::GenericEvent {
        let cookie = unsafe { &event.generic_event_cookie };
        if cookie.extension == scan.opcode && cookie.evtype == xinput2::XI_RawMotion {
            scan.found = true;
        }
    }
    0
}

/// Whether raw motion is waiting in Xlib's event queue. Other queued events (a button release,
/// say) say nothing about whether the sampled movement will be reported, so only motion counts.
unsafe fn raw_motion_queued(
    xlib: &xlib::Xlib,
    display: *mut xlib::Display,
    opcode: libc::c_int,
) -> bool {
    let mut scan = RawMotionScan {
        opcode,
        found: false,
    };
    let mut unused = xlib::XEvent { pad: [0; 24] };
    unsafe {
        (xlib.XCheckIfEvent)(
            display,
            &mut unused,
            Some(scan_for_raw_motion),
            (&raw mut scan).cast(),
        )
    };
    scan.found
}

/// Some X servers (seen on FreeBSD) stop delivering raw events while a button is held:
/// releases and further presses never arrive and the pointer appears frozen in the stream.
/// While a button is believed held, ask the server for the real button state, send any
/// missing press or release, and turn pointer movement that produced no raw motion into
/// mouse deltas.
#[allow(clippy::too_many_arguments)]
unsafe fn reconcile_pressed_buttons(
    state: &XInputState,
    xlib: &xlib::Xlib,
    xi: &xinput2::XInput2,
    display: *mut xlib::Display,
    root: xlib::Window,
    pointer_id: libc::c_int,
    opcode: libc::c_int,
    fallback: &mut DragFallback,
) {
    if state
        .pressed_buttons
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_empty()
    {
        *fallback = DragFallback::default();
        return;
    }
    let Some(sample) = (unsafe { query_pointer(xlib, xi, display, root, pointer_id) }) else {
        return;
    };
    let (down, x, y) = (sample.buttons, sample.x, sample.y);
    let mut pressed_buttons = state
        .pressed_buttons
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !state.enabled.load(Ordering::Acquire) {
        return;
    }
    // The query above is a round trip, so raw motion for the movement it sampled may be sitting
    // unprocessed in Xlib's queue. Leave that movement to those events: they will set
    // motion_seen, and otherwise the next sample sends it. Movement is sent first, so it stays
    // ordered before any button change below. The position delta is pointer-accelerated; it is
    // only a stand-in while raw motion is not delivered.
    let queued = unsafe { raw_motion_queued(xlib, display, opcode) };
    match fallback_motion(
        fallback.motion_seen,
        queued,
        fallback.last_position.is_some(),
    ) {
        FallbackMotion::RawCarried => {
            // Raw events carry no pointer coordinates, so the pointer movement made between the
            // last raw event and this sample cannot be established and the position is simply
            // taken as the new baseline. Handing over from raw motion to the pointer position
            // loses at most one sampling interval (about 4 ms) of movement; subtracting raw
            // deltas from the accelerated pointer movement would not be valid.
            fallback.motion_seen = false;
            fallback.last_position = Some((x, y));
        }
        FallbackMotion::RawQueued | FallbackMotion::Start => {
            if fallback.last_position.is_none() {
                fallback.last_position = Some((x, y));
            }
        }
        FallbackMotion::Pointer => {
            let (last_x, last_y) = fallback.last_position.unwrap_or((x, y));
            fallback.residual_x += x - last_x;
            fallback.residual_y += y - last_y;
            let delta_x = fallback.residual_x.trunc() as i32;
            let delta_y = fallback.residual_y.trunc() as i32;
            fallback.residual_x -= f64::from(delta_x);
            fallback.residual_y -= f64::from(delta_y);
            push_mouse_delta(&state.captured_input, delta_x, delta_y);
            // The pointer position stops at the edge of the area it can move in but relative
            // mouse look must not: when it gets close, put the pointer back in the middle of
            // that area. This is the capture window, not the screen, because a grabbed pointer
            // is confined to its window and a warp outside it would be clipped.
            let bounds = unsafe { capture_bounds(xlib, display, sample.child) };
            if bounds.near_edge(x, y) {
                let (center_x, center_y) = bounds.center();
                unsafe {
                    (xi.XIWarpPointer)(
                        display, pointer_id, 0, root, 0.0, 0.0, 0, 0, center_x, center_y,
                    );
                    (xlib.XFlush)(display);
                }
                // Read back where the warp actually landed, so a clipped warp is never taken
                // for user movement on the next sample.
                fallback.last_position =
                    match unsafe { query_pointer(xlib, xi, display, root, pointer_id) } {
                        Some(landed) => Some((landed.x, landed.y)),
                        None => Some((center_x, center_y)),
                    };
            } else {
                fallback.last_position = Some((x, y));
            }
        }
    }
    // A second button pressed while the first is held produces no raw press either.
    for button in down.iter().copied() {
        if pressed_buttons.insert(button) {
            state.captured_input.push(CapturedInput::MouseButton {
                button,
                pressed: true,
            });
        }
    }
    let stale: Vec<u8> = pressed_buttons
        .iter()
        .copied()
        .filter(|button| !down.contains(button))
        .collect();
    for button in stale {
        pressed_buttons.remove(&button);
        state.captured_input.push(CapturedInput::MouseButton {
            button,
            pressed: false,
        });
    }
}

fn release_pressed_buttons(state: &XInputState) {
    let mut pressed_buttons = state
        .pressed_buttons
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    for button in pressed_buttons.drain() {
        state.captured_input.push(CapturedInput::MouseButton {
            button,
            pressed: false,
        });
    }
}

fn push_mouse_delta(queue: &CapturedInputQueue, mut delta_x: i32, mut delta_y: i32) {
    while delta_x != 0 || delta_y != 0 {
        let x = delta_x.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        let y = delta_y.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16;
        queue.push(CapturedInput::MouseMove {
            delta_x: x,
            delta_y: y,
        });
        delta_x -= i32::from(x);
        delta_y -= i32::from(y);
    }
}

#[cfg(test)]
mod tests {
    use super::{logical_button, push_mouse_delta};
    use crate::media::{CapturedInput, CapturedInputQueue};

    #[test]
    fn recentering_uses_the_capture_window_not_the_screen() {
        use super::CaptureBounds;
        // A 800x600 window in the corner of a 1920x1080 screen.
        let window = CaptureBounds::of_window(Some((100.0, 50.0, 800.0, 600.0)), 1920.0, 1080.0);
        assert_eq!(window.center(), (500.0, 350.0));
        // Well inside the window but nowhere near a screen edge: still not an edge.
        assert!(!window.near_edge(500.0, 350.0));
        // Close to the window edge, far from every screen edge: recenter.
        assert!(window.near_edge(130.0, 350.0));
        assert!(window.near_edge(500.0, 620.0));
        // A window running off the screen is clipped to it.
        let clipped = CaptureBounds::of_window(Some((1500.0, 700.0, 800.0, 600.0)), 1920.0, 1080.0);
        assert_eq!((clipped.right, clipped.bottom), (1920.0, 1080.0));
        // Unknown or tiny windows fall back to the screen.
        let screen = CaptureBounds::of_window(None, 1920.0, 1080.0);
        assert_eq!(screen.center(), (960.0, 540.0));
        assert_eq!(
            CaptureBounds::of_window(Some((0.0, 0.0, 100.0, 100.0)), 1920.0, 1080.0),
            screen
        );
    }

    #[test]
    fn wheel_buttons_cover_both_axes() {
        use super::wheel_delta;
        assert_eq!(wheel_delta(4), Some((0, 120)));
        assert_eq!(wheel_delta(5), Some((0, -120)));
        assert_eq!(wheel_delta(6), Some((-120, 0)));
        assert_eq!(wheel_delta(7), Some((120, 0)));
        assert_eq!(wheel_delta(1), None);
        assert_eq!(wheel_delta(8), None);
    }

    #[test]
    fn fallback_motion_only_defers_to_raw_motion() {
        use super::{FallbackMotion, fallback_motion};
        // Movement raw motion already carried, or that raw motion will carry, is never sent as
        // a pointer delta; anything else is, so a pending button event cannot swallow it.
        assert_eq!(fallback_motion(true, false, true), FallbackMotion::RawCarried);
        assert_eq!(fallback_motion(true, true, true), FallbackMotion::RawCarried);
        assert_eq!(fallback_motion(false, true, true), FallbackMotion::RawQueued);
        assert_eq!(fallback_motion(false, false, true), FallbackMotion::Pointer);
        assert_eq!(fallback_motion(false, false, false), FallbackMotion::Start);
    }

    #[test]
    fn raw_buttons_are_converted_to_the_servers_logical_buttons() {
        // Identity map, swapped primary/secondary buttons, a disabled button, and buttons
        // outside the map.
        assert_eq!(logical_button(&[1, 2, 3], 2), 2);
        assert_eq!(logical_button(&[3, 2, 1], 1), 3);
        assert_eq!(logical_button(&[3, 2, 1], 3), 1);
        assert_eq!(logical_button(&[1, 0, 3], 2), 0);
        assert_eq!(logical_button(&[1, 2, 3], 9), 9);
        assert_eq!(logical_button(&[], 1), 1);
        assert_eq!(logical_button(&[1, 2, 3], 0), 0);
    }

    #[test]
    fn large_xinput_deltas_are_split_without_loss() {
        let queue = CapturedInputQueue::default();
        push_mouse_delta(&queue, 40_000, -40_000);
        let mut total = (0_i32, 0_i32);
        while let Some(CapturedInput::MouseMove { delta_x, delta_y }) = queue.take() {
            total.0 += i32::from(delta_x);
            total.1 += i32::from(delta_y);
        }
        assert_eq!(total, (40_000, -40_000));
    }
}
