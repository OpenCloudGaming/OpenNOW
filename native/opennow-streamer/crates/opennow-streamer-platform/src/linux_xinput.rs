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
    // While a button is held this X server may stop delivering raw events, so the
    // pointer position is polled as a fallback source of motion (see
    // reconcile_pressed_buttons).
    let mut drag_last_position: Option<(f64, f64)> = None;
    let mut drag_motion_seen = false;
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
                    &state,
                    &xlib,
                    &xi,
                    display,
                    root,
                    pointer_id,
                    &mut drag_last_position,
                    &mut drag_motion_seen,
                    &mut residual_x,
                    &mut residual_y,
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
            if !cookie.data.is_null() {
                let raw = unsafe { &*(cookie.data.cast::<xinput2::XIRawEvent>()) };
                if process_raw_event(&state, raw, &mut residual_x, &mut residual_y) {
                    drag_motion_seen = true;
                }
            }
            unsafe { (xlib.XFreeEventData)(display, cookie) };
        }
        unsafe {
            reconcile_pressed_buttons(
                &state,
                &xlib,
                &xi,
                display,
                root,
                pointer_id,
                &mut drag_last_position,
                &mut drag_motion_seen,
                &mut residual_x,
                &mut residual_y,
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
        xinput2::XI_RawButtonPress => process_button(state, raw.detail, true),
        xinput2::XI_RawButtonRelease => process_button(state, raw.detail, false),
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

fn process_button(state: &XInputState, detail: libc::c_int, pressed: bool) {
    if pressed {
        match detail {
            4 => {
                state.captured_input.push(CapturedInput::MouseWheel {
                    delta_x: 0,
                    delta_y: 120,
                });
                return;
            }
            5 => {
                state.captured_input.push(CapturedInput::MouseWheel {
                    delta_x: 0,
                    delta_y: -120,
                });
                return;
            }
            _ => {}
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

/// Reads the master pointer's physically pressed buttons (mapped to the stream's button
/// numbers) and its root-window position.
unsafe fn query_pointer(
    xlib: &xlib::Xlib,
    xi: &xinput2::XInput2,
    display: *mut xlib::Display,
    root: xlib::Window,
    pointer_id: libc::c_int,
) -> Option<(HashSet<u8>, f64, f64)> {
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
    Some((down, root_x, root_y))
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
    last_position: &mut Option<(f64, f64)>,
    motion_seen: &mut bool,
    residual_x: &mut f64,
    residual_y: &mut f64,
) {
    if state
        .pressed_buttons
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .is_empty()
    {
        *last_position = None;
        *motion_seen = false;
        return;
    }
    let Some((down, x, y)) = (unsafe { query_pointer(xlib, xi, display, root, pointer_id) })
    else {
        return;
    };
    let mut pressed_buttons = state
        .pressed_buttons
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if !state.enabled.load(Ordering::Acquire) {
        return;
    }
    // Movement first, so it stays ordered before any button change sent below.
    if *motion_seen {
        *motion_seen = false;
        *last_position = Some((x, y));
    } else if let Some((last_x, last_y)) = *last_position {
        *residual_x += x - last_x;
        *residual_y += y - last_y;
        let delta_x = residual_x.trunc() as i32;
        let delta_y = residual_y.trunc() as i32;
        *residual_x -= f64::from(delta_x);
        *residual_y -= f64::from(delta_y);
        push_mouse_delta(&state.captured_input, delta_x, delta_y);
        // The pointer position stops at the screen edge but relative mouse look must not:
        // when it gets close, put the pointer back in the middle of the screen.
        let screen = unsafe { (xlib.XDefaultScreen)(display) };
        let width = f64::from(unsafe { (xlib.XDisplayWidth)(display, screen) });
        let height = f64::from(unsafe { (xlib.XDisplayHeight)(display, screen) });
        const EDGE_MARGIN: f64 = 64.0;
        if x < EDGE_MARGIN || y < EDGE_MARGIN || x > width - EDGE_MARGIN || y > height - EDGE_MARGIN
        {
            let (center_x, center_y) = (width / 2.0, height / 2.0);
            unsafe {
                (xi.XIWarpPointer)(
                    display, pointer_id, 0, root, 0.0, 0.0, 0, 0, center_x, center_y,
                );
                (xlib.XFlush)(display);
            }
            *last_position = Some((center_x, center_y));
        } else {
            *last_position = Some((x, y));
        }
    } else {
        *last_position = Some((x, y));
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
    use super::push_mouse_delta;
    use crate::media::{CapturedInput, CapturedInputQueue};

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
