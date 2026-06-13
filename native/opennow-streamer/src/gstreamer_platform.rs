#[cfg(target_os = "windows")]
use crate::gstreamer_backend::send_log;
#[cfg(target_os = "windows")]
use crate::protocol::NativeRenderRect;
use crate::protocol::{Event, NativeRenderSurface, NativeStreamerShortcutBindings};
#[cfg(target_os = "windows")]
use gst_video::prelude::*;
use gstreamer as gst;
#[cfg(target_os = "windows")]
use gstreamer_video as gst_video;
#[cfg(target_os = "windows")]
use std::ffi::c_void;
use std::sync::atomic::AtomicBool;
#[cfg(target_os = "windows")]
use std::sync::atomic::Ordering;
use std::sync::mpsc::Sender;
use std::sync::Arc;
#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
use std::time::Duration;

#[cfg(target_os = "windows")]
fn parse_window_handle(value: &str) -> Result<usize, String> {
    let trimmed = value.trim();
    let hex = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"));
    let parsed = if let Some(hex) = hex {
        usize::from_str_radix(hex, 16)
    } else {
        trimmed.parse::<usize>()
    }
    .map_err(|error| format!("Invalid native render window handle {value:?}: {error}"))?;

    if parsed == 0 {
        return Err("Native render window handle is zero.".to_owned());
    }

    Ok(parsed)
}

#[cfg(target_os = "windows")]
fn normalized_render_rect(rect: Option<&NativeRenderRect>) -> NativeRenderRect {
    let Some(rect) = rect else {
        return NativeRenderRect {
            x: 0,
            y: 0,
            width: 2,
            height: 2,
        };
    };

    NativeRenderRect {
        x: rect.x.max(0),
        y: rect.y.max(0),
        width: rect.width.max(2),
        height: rect.height.max(2),
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn start_external_renderer_window_guard(
    event_sender: Option<Sender<Event>>,
    stop: Arc<AtomicBool>,
) {
    thread::spawn(move || {
        let mut logged = false;
        while !stop.load(Ordering::SeqCst) {
            if stop.load(Ordering::SeqCst) {
                break;
            }

            let configured = unsafe { win32_renderer_window::protect_process_renderer_window() };
            if configured && !logged {
                send_log(
                    &event_sender,
                    "info",
                    "Configured external native renderer window for fullscreen DX11 input capture."
                        .to_owned(),
                );
                logged = true;
            }
            thread::sleep(if logged {
                Duration::from_millis(500)
            } else {
                Duration::from_millis(100)
            });
        }
    });
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn start_external_renderer_window_guard(
    _event_sender: Option<Sender<Event>>,
    _stop: Arc<AtomicBool>,
) {
}

#[cfg(target_os = "windows")]
pub(crate) fn set_native_shortcut_bindings(bindings: &NativeStreamerShortcutBindings) {
    unsafe {
        win32_renderer_window::set_shortcut_bindings(bindings.clone());
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn set_native_shortcut_bindings(_bindings: &NativeStreamerShortcutBindings) {}

#[cfg(target_os = "windows")]
pub(crate) fn clear_native_shortcut_bindings() {
    unsafe {
        win32_renderer_window::clear_shortcut_bindings();
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn clear_native_shortcut_bindings() {}

#[cfg(target_os = "windows")]
pub(crate) fn update_external_renderer_surface(surface: &NativeRenderSurface) {
    let target = surface
        .window_handle
        .as_deref()
        .and_then(|window_handle| parse_window_handle(window_handle).ok())
        .and_then(|window_handle| {
            surface
                .visible
                .then_some(())
                .and(surface.rect.as_ref())
                .map(|rect| (window_handle, normalized_render_rect(Some(rect))))
        });

    unsafe {
        win32_renderer_window::set_render_target_surface(target);
    }
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn update_external_renderer_surface(_surface: &NativeRenderSurface) {}

#[cfg(target_os = "windows")]
pub(crate) mod win32_renderer_window {
    use crate::gstreamer_input::NativeWindowInputEvent;
    use crate::protocol::NativeRenderRect;
    use crate::protocol::{NativeStreamerShortcutAction, NativeStreamerShortcutBindings};
    use crate::shortcuts::NativeShortcutMatcher;
    use std::collections::{HashMap, HashSet};
    use std::ffi::c_void;
    use std::ptr::{null, null_mut};
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::mpsc::Sender;
    use std::sync::{Mutex, OnceLock};
    use std::thread;
    use std::time::{Duration, Instant};

    type Bool = i32;
    type Dword = u32;
    type Hcursor = *mut c_void;
    type Hmonitor = *mut c_void;
    type Hrawinput = *mut c_void;
    type Hwnd = *mut c_void;
    type Lparam = isize;
    type Lresult = isize;
    type Uint = u32;
    type Wparam = usize;

    const GWL_STYLE: i32 = -16;
    const GWL_EXSTYLE: i32 = -20;
    const GWLP_WNDPROC: i32 = -4;
    const GW_OWNER: Uint = 4;
    const HTCLIENT: isize = 1;
    const HWND_NOTOPMOST: Hwnd = -2isize as Hwnd;
    const MA_ACTIVATE: isize = 1;
    const MONITOR_DEFAULTTONEAREST: Dword = 0x0000_0002;
    const RID_INPUT: Uint = 0x1000_0003;
    const RIDEV_REMOVE: Dword = 0x0000_0001;
    const RIDEV_NOLEGACY: Dword = 0x0000_0030;
    const RIDEV_CAPTUREMOUSE: Dword = 0x0000_0200;
    const RIM_TYPEMOUSE: Dword = 0;
    const RIM_TYPEKEYBOARD: Dword = 1;
    const RI_KEY_BREAK: u16 = 0x0001;
    const RI_KEY_E0: u16 = 0x0002;
    const RI_KEY_E1: u16 = 0x0004;
    const RI_MOUSE_LEFT_BUTTON_DOWN: u16 = 0x0001;
    const RI_MOUSE_LEFT_BUTTON_UP: u16 = 0x0002;
    const RI_MOUSE_RIGHT_BUTTON_DOWN: u16 = 0x0004;
    const RI_MOUSE_RIGHT_BUTTON_UP: u16 = 0x0008;
    const RI_MOUSE_MIDDLE_BUTTON_DOWN: u16 = 0x0010;
    const RI_MOUSE_MIDDLE_BUTTON_UP: u16 = 0x0020;
    const RI_MOUSE_BUTTON_4_DOWN: u16 = 0x0040;
    const RI_MOUSE_BUTTON_4_UP: u16 = 0x0080;
    const RI_MOUSE_BUTTON_5_DOWN: u16 = 0x0100;
    const RI_MOUSE_BUTTON_5_UP: u16 = 0x0200;
    const RI_MOUSE_WHEEL: u16 = 0x0400;
    const VK_SHIFT: u16 = 0x10;
    const VK_ESCAPE: u16 = 0x1B;
    const VK_TAB: u16 = 0x09;
    const VK_CONTROL: u16 = 0x11;
    const VK_MENU: u16 = 0x12;
    const VK_CAPITAL: i32 = 0x14;
    const VK_NUMLOCK: i32 = 0x90;
    const VK_LSHIFT: u16 = 0xA0;
    const VK_RSHIFT: u16 = 0xA1;
    const VK_LCONTROL: u16 = 0xA2;
    const VK_RCONTROL: u16 = 0xA3;
    const VK_LMENU: u16 = 0xA4;
    const VK_RMENU: u16 = 0xA5;
    const VK_LWIN: u16 = 0x5B;
    const VK_RWIN: u16 = 0x5C;
    const WM_INPUT: Uint = 0x00FF;
    const WM_NCHITTEST: Uint = 0x0084;
    const WM_MOUSEACTIVATE: Uint = 0x0021;
    const WM_SETCURSOR: Uint = 0x0020;
    const WM_KILLFOCUS: Uint = 0x0008;
    const WM_ACTIVATE: Uint = 0x0006;
    const WA_INACTIVE: usize = 0;
    const WM_KEYDOWN: Uint = 0x0100;
    const WM_KEYUP: Uint = 0x0101;
    const WM_SYSKEYDOWN: Uint = 0x0104;
    const WM_SYSKEYUP: Uint = 0x0105;
    const WM_LBUTTONDOWN: Uint = 0x0201;
    const WM_LBUTTONUP: Uint = 0x0202;
    const WM_RBUTTONDOWN: Uint = 0x0204;
    const WM_RBUTTONUP: Uint = 0x0205;
    const WM_MBUTTONDOWN: Uint = 0x0207;
    const WM_MBUTTONUP: Uint = 0x0208;
    const WM_XBUTTONDOWN: Uint = 0x020B;
    const WM_XBUTTONUP: Uint = 0x020C;
    const XBUTTON1: u16 = 0x0001;
    const XBUTTON2: u16 = 0x0002;
    const WS_CAPTION: isize = 0x00C0_0000;
    const WS_MAXIMIZEBOX: isize = 0x0001_0000;
    const WS_MINIMIZEBOX: isize = 0x0002_0000;
    const WS_SYSMENU: isize = 0x0008_0000;
    const WS_THICKFRAME: isize = 0x0004_0000;
    const WS_EX_NOACTIVATE: isize = 0x0800_0000;
    const WS_EX_TOOLWINDOW: isize = 0x0000_0080;
    const WS_EX_TRANSPARENT: isize = 0x0000_0020;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SW_MINIMIZE: i32 = 6;
    const ESCAPE_SCANCODE: u16 = 0x0001;
    const ESCAPE_HOLD_TO_MINIMIZE: Duration = Duration::from_secs(5);

    struct EnumState {
        process_id: u32,
        candidates: Vec<WindowCandidate>,
    }

    #[derive(Clone, Copy)]
    struct WindowCandidate {
        hwnd: Hwnd,
        area: i64,
    }

    #[derive(Clone, Copy)]
    struct RenderTargetSurface {
        hwnd: isize,
        client_rect: Rect,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct MonitorInfo {
        cb_size: Dword,
        rc_monitor: Rect,
        rc_work: Rect,
        dw_flags: Dword,
    }

    #[repr(C)]
    struct RawInputDevice {
        us_usage_page: u16,
        us_usage: u16,
        dw_flags: Dword,
        hwnd_target: Hwnd,
    }

    #[repr(C)]
    struct RawInputHeader {
        dw_type: Dword,
        dw_size: Dword,
        h_device: *mut c_void,
        w_param: Wparam,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RawMouse {
        us_flags: u16,
        buttons: u32,
        ul_raw_buttons: u32,
        l_last_x: i32,
        l_last_y: i32,
        ul_extra_information: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    struct RawKeyboard {
        make_code: u16,
        flags: u16,
        reserved: u16,
        vkey: u16,
        message: Uint,
        extra_information: u32,
    }

    #[derive(Clone, Copy)]
    struct PressedKey {
        keycode: u16,
        scancode: u16,
        suppressed: bool,
    }

    #[derive(Clone, Copy)]
    struct EscapeKeyPress {
        scancode: u16,
        hold_timer_armed: bool,
    }

    static INPUT_EVENT_SENDER: OnceLock<Mutex<Option<Sender<NativeWindowInputEvent>>>> =
        OnceLock::new();
    static ORIGINAL_WNDPROCS: OnceLock<Mutex<HashMap<isize, isize>>> = OnceLock::new();
    static CAPTURED_HWND: OnceLock<Mutex<Option<isize>>> = OnceLock::new();
    static PROTECTED_HWND: OnceLock<Mutex<Option<isize>>> = OnceLock::new();
    static PRESSED_KEYS: OnceLock<Mutex<HashMap<u16, PressedKey>>> = OnceLock::new();
    static LEGACY_SUPPRESSED_KEYS: OnceLock<Mutex<HashSet<u16>>> = OnceLock::new();
    static STARTED_AT: OnceLock<Instant> = OnceLock::new();
    static ESCAPE_HOLD_HWND: OnceLock<Mutex<Option<isize>>> = OnceLock::new();
    static ESCAPE_HOLD_TOKEN: OnceLock<AtomicU64> = OnceLock::new();
    static ESCAPE_KEY_PRESS: OnceLock<Mutex<Option<EscapeKeyPress>>> = OnceLock::new();
    static SHORTCUT_MATCHER: OnceLock<Mutex<NativeShortcutMatcher>> = OnceLock::new();
    static RENDER_TARGET_SURFACE: OnceLock<Mutex<Option<RenderTargetSurface>>> = OnceLock::new();

    #[link(name = "user32")]
    unsafe extern "system" {
        fn CallWindowProcW(
            previous: isize,
            hwnd: Hwnd,
            message: Uint,
            wparam: Wparam,
            lparam: Lparam,
        ) -> Lresult;
        fn ClientToScreen(hwnd: Hwnd, point: *mut Point) -> Bool;
        fn ClipCursor(rect: *const Rect) -> Bool;
        fn DefWindowProcW(hwnd: Hwnd, message: Uint, wparam: Wparam, lparam: Lparam) -> Lresult;
        fn EnumWindows(
            callback: Option<unsafe extern "system" fn(Hwnd, Lparam) -> Bool>,
            lparam: Lparam,
        ) -> Bool;
        fn GetMonitorInfoW(monitor: Hmonitor, info: *mut MonitorInfo) -> Bool;
        fn GetRawInputData(
            raw_input: Hrawinput,
            command: Uint,
            data: *mut c_void,
            size: *mut u32,
            header_size: u32,
        ) -> u32;
        fn GetKeyState(virtual_key: i32) -> i16;
        fn GetWindow(hwnd: Hwnd, command: Uint) -> Hwnd;
        fn GetWindowLongPtrW(hwnd: Hwnd, index: i32) -> isize;
        fn GetWindowRect(hwnd: Hwnd, rect: *mut Rect) -> Bool;
        fn GetWindowThreadProcessId(hwnd: Hwnd, process_id: *mut u32) -> u32;
        fn IsIconic(hwnd: Hwnd) -> Bool;
        fn IsWindowVisible(hwnd: Hwnd) -> Bool;
        fn MonitorFromWindow(hwnd: Hwnd, flags: Dword) -> Hmonitor;
        fn RegisterRawInputDevices(devices: *const RawInputDevice, count: u32, size: u32) -> Bool;
        fn ReleaseCapture() -> Bool;
        fn SetCapture(hwnd: Hwnd) -> Hwnd;
        fn SetCursor(cursor: Hcursor) -> Hcursor;
        fn SetFocus(hwnd: Hwnd) -> Hwnd;
        fn SetForegroundWindow(hwnd: Hwnd) -> Bool;
        fn SetWindowLongPtrW(hwnd: Hwnd, index: i32, new_long: isize) -> isize;
        fn SetWindowPos(
            hwnd: Hwnd,
            insert_after: Hwnd,
            x: i32,
            y: i32,
            cx: i32,
            cy: i32,
            flags: u32,
        ) -> Bool;
        fn ShowWindow(hwnd: Hwnd, command: i32) -> Bool;
        fn ShowCursor(show: Bool) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcessId() -> u32;
    }

    pub unsafe fn set_render_target_surface(target: Option<(usize, NativeRenderRect)>) {
        let target_surface = target.map(|(window_handle, rect)| RenderTargetSurface {
            hwnd: window_handle as isize,
            client_rect: Rect {
                left: rect.x,
                top: rect.y,
                right: rect.x.saturating_add(rect.width.max(2)),
                bottom: rect.y.saturating_add(rect.height.max(2)),
            },
        });
        let slot = RENDER_TARGET_SURFACE.get_or_init(|| Mutex::new(None));
        if let Ok(mut current) = slot.lock() {
            *current = target_surface;
        }
    }

    pub unsafe fn set_input_event_sender(sender: Option<Sender<NativeWindowInputEvent>>) {
        let slot = INPUT_EVENT_SENDER.get_or_init(|| Mutex::new(None));
        if let Ok(mut current) = slot.lock() {
            *current = sender;
        }
    }

    pub unsafe fn set_shortcut_bindings(bindings: NativeStreamerShortcutBindings) {
        let matcher = SHORTCUT_MATCHER.get_or_init(|| Mutex::new(NativeShortcutMatcher::default()));
        if let Ok(mut current) = matcher.lock() {
            *current = NativeShortcutMatcher::from_bindings(&bindings);
        }
    }

    pub unsafe fn clear_shortcut_bindings() {
        let matcher = SHORTCUT_MATCHER.get_or_init(|| Mutex::new(NativeShortcutMatcher::default()));
        if let Ok(mut current) = matcher.lock() {
            *current = NativeShortcutMatcher::default();
        }
    }

    pub unsafe fn release_current_input_capture() {
        let Some(captured) = CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
        else {
            unregister_raw_input_devices();
            return;
        };

        release_input_capture(captured as Hwnd);
    }

    pub unsafe fn protect_process_renderer_window() -> bool {
        let mut state = EnumState {
            process_id: GetCurrentProcessId(),
            candidates: Vec::new(),
        };
        EnumWindows(
            Some(collect_renderer_window_candidate),
            &mut state as *mut EnumState as Lparam,
        );

        let Some(candidate) = state
            .candidates
            .into_iter()
            .max_by_key(|candidate| candidate.area)
        else {
            return false;
        };

        protect_renderer_window(candidate.hwnd)
    }

    unsafe extern "system" fn collect_renderer_window_candidate(
        hwnd: Hwnd,
        lparam: Lparam,
    ) -> Bool {
        let state = &mut *(lparam as *mut EnumState);
        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, &mut process_id);
        if process_id != state.process_id || IsWindowVisible(hwnd) == 0 || IsIconic(hwnd) != 0 {
            return 1;
        }

        if !GetWindow(hwnd, GW_OWNER).is_null() {
            return 1;
        }

        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        if (ex_style & (WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE)) != 0 {
            return 1;
        }

        let mut rect = Rect {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return 1;
        }
        let width = rect.right.saturating_sub(rect.left);
        let height = rect.bottom.saturating_sub(rect.top);
        if width < 320 || height < 180 {
            return 1;
        }

        state.candidates.push(WindowCandidate {
            hwnd,
            area: i64::from(width) * i64::from(height),
        });
        1
    }

    unsafe fn protect_renderer_window(hwnd: Hwnd) -> bool {
        let protected_slot = PROTECTED_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut protected) = protected_slot.lock() {
            *protected = Some(hwnd as isize);
        }

        let mut configured = false;
        let current = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        let desired = current & !(WS_EX_NOACTIVATE | WS_EX_TRANSPARENT);
        if desired != current {
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, desired);
            configured = true;
        }

        let current_style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        let fullscreen_style = current_style
            & !(WS_CAPTION | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_SYSMENU);
        if fullscreen_style != current_style {
            SetWindowLongPtrW(hwnd, GWL_STYLE, fullscreen_style);
            configured = true;
        }

        if install_input_wndproc(hwnd) {
            SetForegroundWindow(hwnd);
            SetFocus(hwnd);
            configured = true;
        }

        if let Some(rect) = target_renderer_rect().or_else(|| monitor_rect_for_window(hwnd)) {
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                rect.left,
                rect.top,
                rect.right.saturating_sub(rect.left).max(2),
                rect.bottom.saturating_sub(rect.top).max(2),
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            configured = true;
        } else {
            SetWindowPos(
                hwnd,
                HWND_NOTOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }

        configured
    }

    unsafe fn render_rect_to_screen_rect(hwnd: Hwnd, rect: Rect) -> Option<Rect> {
        if hwnd.is_null() {
            return None;
        }
        let mut origin = Point {
            x: rect.left,
            y: rect.top,
        };
        if ClientToScreen(hwnd, &mut origin) == 0 {
            return None;
        }
        let width = rect.right.saturating_sub(rect.left).max(2);
        let height = rect.bottom.saturating_sub(rect.top).max(2);

        Some(Rect {
            left: origin.x,
            top: origin.y,
            right: origin.x.saturating_add(width),
            bottom: origin.y.saturating_add(height),
        })
    }

    unsafe fn install_input_wndproc(hwnd: Hwnd) -> bool {
        let key = hwnd as isize;
        let map = ORIGINAL_WNDPROCS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut map) = map.lock() else {
            return false;
        };
        if map.contains_key(&key) {
            return false;
        }

        let previous = SetWindowLongPtrW(hwnd, GWLP_WNDPROC, renderer_window_wndproc as isize);
        if previous == 0 {
            return false;
        }
        map.insert(key, previous);
        true
    }

    unsafe extern "system" fn renderer_window_wndproc(
        hwnd: Hwnd,
        message: Uint,
        wparam: Wparam,
        lparam: Lparam,
    ) -> Lresult {
        if message == WM_NCHITTEST {
            return HTCLIENT;
        }
        if message == WM_MOUSEACTIVATE {
            begin_input_capture(hwnd);
            return MA_ACTIVATE;
        }
        if message == WM_SETCURSOR && is_input_captured(hwnd) {
            SetCursor(null_mut());
            return 1;
        }
        if message == WM_INPUT {
            handle_raw_input(lparam as Hrawinput);
            return 0;
        }
        let handled_legacy_shortcut = is_keyboard_message(message)
            && handle_legacy_shortcut_keyboard(message, wparam, lparam);
        if handled_legacy_shortcut {
            return 0;
        }
        if is_escape_keyboard_message(message, wparam) {
            if !is_input_captured(hwnd) {
                begin_input_capture(hwnd);
            }
            handle_legacy_escape_keyboard(message, lparam);
            return 0;
        }
        if message == WM_KILLFOCUS || (message == WM_ACTIVATE && (wparam & 0xffff) == WA_INACTIVE) {
            release_input_capture(hwnd);
        }
        if let Some((button, pressed)) = legacy_mouse_button(message, wparam) {
            let was_captured = is_input_captured(hwnd);
            if pressed && !was_captured {
                begin_input_capture(hwnd);
                emit_input_event(NativeWindowInputEvent::MouseButton {
                    pressed,
                    button,
                    timestamp_us: timestamp_us(),
                });
            }
        }

        let key = hwnd as isize;
        let previous = ORIGINAL_WNDPROCS
            .get()
            .and_then(|map| map.lock().ok().and_then(|map| map.get(&key).copied()));
        if let Some(previous) = previous {
            return CallWindowProcW(previous, hwnd, message, wparam, lparam);
        }

        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    unsafe fn monitor_rect_for_window(hwnd: Hwnd) -> Option<Rect> {
        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        if monitor.is_null() {
            return None;
        }

        let mut info = MonitorInfo {
            cb_size: std::mem::size_of::<MonitorInfo>() as Dword,
            rc_monitor: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            rc_work: Rect {
                left: 0,
                top: 0,
                right: 0,
                bottom: 0,
            },
            dw_flags: 0,
        };
        if GetMonitorInfoW(monitor, &mut info) == 0 {
            return None;
        }

        Some(info.rc_monitor)
    }

    unsafe fn target_renderer_rect() -> Option<Rect> {
        let target = RENDER_TARGET_SURFACE
            .get()
            .and_then(|surface| surface.lock().ok().and_then(|surface| *surface))?;
        let hwnd = target.hwnd as Hwnd;
        render_rect_to_screen_rect(hwnd, target.client_rect)
            .or_else(|| monitor_rect_for_window(hwnd))
    }

    unsafe fn begin_input_capture(hwnd: Hwnd) {
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);
        SetCapture(hwnd);
        register_raw_input_devices(hwnd);
        if let Some(rect) = target_renderer_rect().or_else(|| monitor_rect_for_window(hwnd)) {
            ClipCursor(&rect);
        }
        hide_cursor();

        let slot = CAPTURED_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut captured) = slot.lock() {
            *captured = Some(hwnd as isize);
        }
    }

    unsafe fn release_input_capture(hwnd: Hwnd) {
        cancel_escape_hold_to_minimize_timer();
        clear_escape_key_press();
        let slot = CAPTURED_HWND.get_or_init(|| Mutex::new(None));
        let mut should_release = false;
        if let Ok(mut captured) = slot.lock() {
            should_release = captured.is_some_and(|captured| captured == hwnd as isize);
            if should_release {
                *captured = None;
            }
        }

        if !should_release {
            return;
        }

        release_pressed_keys();
        ReleaseCapture();
        ClipCursor(null());
        show_cursor();
        unregister_raw_input_devices();
        SetWindowPos(
            hwnd,
            HWND_NOTOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
    }

    fn is_input_captured(hwnd: Hwnd) -> bool {
        CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
            .is_some_and(|captured| captured == hwnd as isize)
    }

    fn captured_hwnd() -> Option<isize> {
        CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
    }

    fn protected_hwnd() -> Option<isize> {
        PROTECTED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
    }

    unsafe fn start_escape_hold_to_minimize_timer() {
        let Some(hwnd) = captured_hwnd() else {
            return;
        };

        let token = ESCAPE_HOLD_TOKEN
            .get_or_init(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst)
            .wrapping_add(1);
        let slot = ESCAPE_HOLD_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut held_hwnd) = slot.lock() {
            *held_hwnd = Some(hwnd);
        }

        thread::spawn(move || {
            thread::sleep(ESCAPE_HOLD_TO_MINIMIZE);
            unsafe {
                minimize_window_if_escape_still_held(hwnd, token);
            }
        });
    }

    fn cancel_escape_hold_to_minimize_timer() {
        ESCAPE_HOLD_TOKEN
            .get_or_init(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::SeqCst);
        let slot = ESCAPE_HOLD_HWND.get_or_init(|| Mutex::new(None));
        if let Ok(mut held_hwnd) = slot.lock() {
            *held_hwnd = None;
        }
    }

    unsafe fn minimize_window_if_escape_still_held(hwnd: isize, token: u64) {
        let current_token = ESCAPE_HOLD_TOKEN
            .get_or_init(|| AtomicU64::new(0))
            .load(Ordering::SeqCst);
        if current_token != token {
            return;
        }

        let still_held = ESCAPE_HOLD_HWND
            .get()
            .and_then(|held_hwnd| held_hwnd.lock().ok().and_then(|held_hwnd| *held_hwnd))
            .is_some_and(|held_hwnd| held_hwnd == hwnd);
        if !still_held {
            return;
        }

        let hwnd = hwnd as Hwnd;
        release_input_capture(hwnd);
        ShowWindow(hwnd, SW_MINIMIZE);
    }

    unsafe fn register_raw_input_devices(hwnd: Hwnd) -> bool {
        let devices = [
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x02,
                dw_flags: RIDEV_NOLEGACY | RIDEV_CAPTUREMOUSE,
                hwnd_target: hwnd,
            },
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x06,
                dw_flags: RIDEV_NOLEGACY,
                hwnd_target: hwnd,
            },
        ];

        RegisterRawInputDevices(
            devices.as_ptr(),
            devices.len() as u32,
            std::mem::size_of::<RawInputDevice>() as u32,
        ) != 0
    }

    unsafe fn unregister_raw_input_devices() -> bool {
        let devices = [
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x02,
                dw_flags: RIDEV_REMOVE,
                hwnd_target: null_mut(),
            },
            RawInputDevice {
                us_usage_page: 0x01,
                us_usage: 0x06,
                dw_flags: RIDEV_REMOVE,
                hwnd_target: null_mut(),
            },
        ];

        RegisterRawInputDevices(
            devices.as_ptr(),
            devices.len() as u32,
            std::mem::size_of::<RawInputDevice>() as u32,
        ) != 0
    }

    unsafe fn handle_raw_input(raw_input: Hrawinput) {
        let mut size = 0u32;
        let header_size = std::mem::size_of::<RawInputHeader>() as u32;
        let query = GetRawInputData(raw_input, RID_INPUT, null_mut(), &mut size, header_size);
        if query == u32::MAX || size < header_size {
            return;
        }

        let mut buffer = vec![0u8; size as usize];
        let read = GetRawInputData(
            raw_input,
            RID_INPUT,
            buffer.as_mut_ptr() as *mut c_void,
            &mut size,
            header_size,
        );
        if read == u32::MAX || read == 0 || buffer.len() < header_size as usize {
            return;
        }

        let header = &*(buffer.as_ptr() as *const RawInputHeader);
        let data = buffer.as_ptr().add(std::mem::size_of::<RawInputHeader>());
        match header.dw_type {
            RIM_TYPEMOUSE => handle_raw_mouse(&*(data as *const RawMouse)),
            RIM_TYPEKEYBOARD => handle_raw_keyboard(&*(data as *const RawKeyboard)),
            _ => {}
        }
    }

    unsafe fn handle_raw_mouse(raw: &RawMouse) {
        if CAPTURED_HWND
            .get()
            .and_then(|captured| captured.lock().ok().and_then(|captured| *captured))
            .is_none()
        {
            return;
        }

        let timestamp_us = timestamp_us();
        let dx = clamp_i32_to_i16(raw.l_last_x);
        let dy = clamp_i32_to_i16(raw.l_last_y);
        if dx != 0 || dy != 0 {
            emit_input_event(NativeWindowInputEvent::MouseMove {
                dx,
                dy,
                timestamp_us,
            });
        }

        let button_flags = (raw.buttons & 0xffff) as u16;
        let button_data = (raw.buttons >> 16) as u16;
        emit_raw_mouse_button_events(button_flags, timestamp_us);
        if (button_flags & RI_MOUSE_WHEEL) != 0 {
            emit_input_event(NativeWindowInputEvent::MouseWheel {
                delta: button_data as i16,
                timestamp_us,
            });
        }
    }

    unsafe fn emit_raw_mouse_button_events(flags: u16, timestamp_us: u64) {
        let pairs = [
            (RI_MOUSE_LEFT_BUTTON_DOWN, 1, true),
            (RI_MOUSE_LEFT_BUTTON_UP, 1, false),
            (RI_MOUSE_MIDDLE_BUTTON_DOWN, 2, true),
            (RI_MOUSE_MIDDLE_BUTTON_UP, 2, false),
            (RI_MOUSE_RIGHT_BUTTON_DOWN, 3, true),
            (RI_MOUSE_RIGHT_BUTTON_UP, 3, false),
            (RI_MOUSE_BUTTON_4_DOWN, 4, true),
            (RI_MOUSE_BUTTON_4_UP, 4, false),
            (RI_MOUSE_BUTTON_5_DOWN, 5, true),
            (RI_MOUSE_BUTTON_5_UP, 5, false),
        ];

        for (flag, button, pressed) in pairs {
            if (flags & flag) != 0 {
                emit_input_event(NativeWindowInputEvent::MouseButton {
                    pressed,
                    button,
                    timestamp_us,
                });
            }
        }
    }

    unsafe fn handle_raw_keyboard(raw: &RawKeyboard) {
        if raw.vkey == 0xff {
            return;
        }

        let pressed = match raw.message {
            WM_KEYDOWN | WM_SYSKEYDOWN => true,
            WM_KEYUP | WM_SYSKEYUP => false,
            _ => (raw.flags & RI_KEY_BREAK) == 0,
        };
        let keycode = normalize_virtual_key(raw.vkey, raw.make_code, raw.flags);
        let mut scancode = normalize_scancode(raw.make_code, raw.flags);
        if keycode == VK_ESCAPE && scancode == 0 {
            scancode = ESCAPE_SCANCODE;
        }
        if keycode == 0 || scancode == 0 {
            return;
        }
        handle_keyboard_state(keycode, scancode, pressed);
    }

    unsafe fn handle_legacy_escape_keyboard(message: Uint, lparam: Lparam) {
        let pressed = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let mut scancode = legacy_keyboard_scancode(lparam);
        if scancode == 0 {
            scancode = ESCAPE_SCANCODE;
        }
        handle_keyboard_state(VK_ESCAPE, scancode, pressed);
    }

    fn is_escape_keyboard_message(message: Uint, wparam: Wparam) -> bool {
        matches!(message, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP)
            && (wparam as u16) == VK_ESCAPE
    }

    fn is_keyboard_message(message: Uint) -> bool {
        matches!(message, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP)
    }

    fn legacy_keyboard_scancode(lparam: Lparam) -> u16 {
        let scancode = ((lparam >> 16) & 0xff) as u16;
        if scancode == 0 {
            return 0;
        }
        if ((lparam >> 24) & 0x01) != 0 {
            0xe000 | scancode
        } else {
            scancode
        }
    }

    unsafe fn handle_legacy_shortcut_keyboard(
        message: Uint,
        wparam: Wparam,
        lparam: Lparam,
    ) -> bool {
        let keycode = wparam as u16;
        if keycode == VK_ESCAPE {
            return false;
        }

        let pressed = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN);
        let scancode = legacy_keyboard_scancode(lparam);
        let key_id = if scancode == 0 { keycode } else { scancode };
        let suppressed_keys = LEGACY_SUPPRESSED_KEYS.get_or_init(|| Mutex::new(HashSet::new()));
        let Ok(mut suppressed_keys) = suppressed_keys.lock() else {
            return false;
        };

        if !pressed {
            return suppressed_keys.remove(&key_id);
        }

        if suppressed_keys.contains(&key_id) {
            return true;
        }

        let modifiers = current_legacy_modifier_flags();
        let Some(action) = shortcut_action_for_keypress(keycode, scancode, modifiers) else {
            return false;
        };

        suppressed_keys.insert(key_id);
        drop(suppressed_keys);
        handle_shortcut_action(action);
        true
    }

    unsafe fn handle_keyboard_state(keycode: u16, scancode: u16, pressed: bool) {
        let keys = PRESSED_KEYS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut keys) = keys.lock() else {
            return;
        };
        if pressed && keycode == VK_TAB && is_alt_modifier_down(&keys) {
            drop(keys);
            release_current_input_capture();
            return;
        }
        if keycode == VK_ESCAPE {
            drop(keys);
            handle_escape_keyboard_state(scancode, pressed);
            return;
        }
        let previous = keys.get(&scancode).copied();
        if pressed {
            if previous.is_some() {
                return;
            }
            let modifiers = current_modifier_flags(&keys);
            if let Some(action) = shortcut_action_for_keypress(keycode, scancode, modifiers) {
                keys.insert(
                    scancode,
                    PressedKey {
                        keycode,
                        scancode,
                        suppressed: true,
                    },
                );
                drop(keys);
                handle_shortcut_action(action);
                return;
            }
            keys.insert(
                scancode,
                PressedKey {
                    keycode,
                    scancode,
                    suppressed: false,
                },
            );
        } else {
            let Some(previous) = keys.remove(&scancode) else {
                return;
            };
            if previous.suppressed {
                return;
            }
        }
        let modifiers = current_modifier_flags(&keys);
        drop(keys);

        emit_input_event(NativeWindowInputEvent::Key {
            pressed,
            keycode,
            scancode,
            modifiers,
            timestamp_us: timestamp_us(),
        });
    }

    unsafe fn handle_escape_keyboard_state(scancode: u16, pressed: bool) {
        let slot = ESCAPE_KEY_PRESS.get_or_init(|| Mutex::new(None));
        let Ok(mut escape_press) = slot.lock() else {
            return;
        };

        if pressed {
            let should_start_hold_timer = if let Some(current) = escape_press.as_mut() {
                let should_start = !current.hold_timer_armed && captured_hwnd().is_some();
                if should_start {
                    current.hold_timer_armed = true;
                }
                should_start
            } else {
                let hold_timer_armed = captured_hwnd().is_some();
                *escape_press = Some(EscapeKeyPress {
                    scancode,
                    hold_timer_armed,
                });
                hold_timer_armed
            };
            drop(escape_press);
            if should_start_hold_timer {
                start_escape_hold_to_minimize_timer();
            }
            return;
        }

        let Some(escape_press) = escape_press.take() else {
            cancel_escape_hold_to_minimize_timer();
            return;
        };
        let scancode = escape_press.scancode;

        cancel_escape_hold_to_minimize_timer();
        send_escape_tap(scancode);
    }

    fn clear_escape_key_press() {
        let slot = ESCAPE_KEY_PRESS.get_or_init(|| Mutex::new(None));
        if let Ok(mut escape_press) = slot.lock() {
            *escape_press = None;
        }
    }

    fn send_escape_tap(scancode: u16) {
        let keydown_timestamp_us = timestamp_us();
        emit_input_event(NativeWindowInputEvent::Key {
            pressed: true,
            keycode: VK_ESCAPE,
            scancode,
            modifiers: 0,
            timestamp_us: keydown_timestamp_us,
        });
        emit_input_event(NativeWindowInputEvent::Key {
            pressed: false,
            keycode: VK_ESCAPE,
            scancode,
            modifiers: 0,
            timestamp_us: timestamp_us(),
        });
    }

    unsafe fn release_pressed_keys() {
        let keys = PRESSED_KEYS.get_or_init(|| Mutex::new(HashMap::new()));
        let Ok(mut keys) = keys.lock() else {
            return;
        };
        let pressed = keys.values().copied().collect::<Vec<_>>();
        keys.clear();
        drop(keys);

        let timestamp_us = timestamp_us();
        for key in pressed {
            if key.suppressed {
                continue;
            }
            emit_input_event(NativeWindowInputEvent::Key {
                pressed: false,
                keycode: key.keycode,
                scancode: key.scancode,
                modifiers: 0,
                timestamp_us,
            });
        }
    }

    fn normalize_virtual_key(vkey: u16, make_code: u16, flags: u16) -> u16 {
        match vkey {
            VK_SHIFT => match make_code {
                0x36 => VK_RSHIFT,
                _ => VK_LSHIFT,
            },
            VK_CONTROL => {
                if (flags & RI_KEY_E0) != 0 {
                    VK_RCONTROL
                } else {
                    VK_LCONTROL
                }
            }
            VK_MENU => {
                if (flags & RI_KEY_E0) != 0 {
                    VK_RMENU
                } else {
                    VK_LMENU
                }
            }
            _ => vkey,
        }
    }

    fn normalize_scancode(make_code: u16, flags: u16) -> u16 {
        if make_code == 0 {
            return 0;
        }
        if (flags & RI_KEY_E0) != 0 {
            0xe000 | make_code
        } else if (flags & RI_KEY_E1) != 0 {
            0xe100 | make_code
        } else {
            make_code
        }
    }

    unsafe fn current_modifier_flags(keys: &HashMap<u16, PressedKey>) -> u16 {
        let mut modifiers = 0u16;
        if keys
            .values()
            .any(|key| matches!(key.keycode, VK_LSHIFT | VK_RSHIFT | VK_SHIFT))
        {
            modifiers |= 0x01;
        }
        if keys
            .values()
            .any(|key| matches!(key.keycode, VK_LCONTROL | VK_RCONTROL | VK_CONTROL))
        {
            modifiers |= 0x02;
        }
        if keys
            .values()
            .any(|key| matches!(key.keycode, VK_LMENU | VK_RMENU | VK_MENU))
        {
            modifiers |= 0x04;
        }
        if keys
            .values()
            .any(|key| matches!(key.keycode, VK_LWIN | VK_RWIN))
        {
            modifiers |= 0x08;
        }
        if (GetKeyState(VK_CAPITAL) & 0x0001) != 0 {
            modifiers |= 0x10;
        }
        if (GetKeyState(VK_NUMLOCK) & 0x0001) != 0 {
            modifiers |= 0x20;
        }
        modifiers
    }

    unsafe fn current_legacy_modifier_flags() -> u16 {
        let mut modifiers = 0u16;
        if is_key_down(VK_SHIFT) || is_key_down(VK_LSHIFT) || is_key_down(VK_RSHIFT) {
            modifiers |= 0x01;
        }
        if is_key_down(VK_CONTROL) || is_key_down(VK_LCONTROL) || is_key_down(VK_RCONTROL) {
            modifiers |= 0x02;
        }
        if is_key_down(VK_MENU) || is_key_down(VK_LMENU) || is_key_down(VK_RMENU) {
            modifiers |= 0x04;
        }
        if is_key_down(VK_LWIN) || is_key_down(VK_RWIN) {
            modifiers |= 0x08;
        }
        if (GetKeyState(VK_CAPITAL) & 0x0001) != 0 {
            modifiers |= 0x10;
        }
        if (GetKeyState(VK_NUMLOCK) & 0x0001) != 0 {
            modifiers |= 0x20;
        }
        modifiers
    }

    unsafe fn is_key_down(keycode: u16) -> bool {
        ((GetKeyState(keycode as i32) as u16) & 0x8000) != 0
    }

    unsafe fn is_alt_modifier_down(keys: &HashMap<u16, PressedKey>) -> bool {
        keys.values()
            .any(|key| matches!(key.keycode, VK_LMENU | VK_RMENU | VK_MENU))
            || ((GetKeyState(VK_MENU as i32) as u16) & 0x8000) != 0
    }

    fn shortcut_action_for_keypress(
        keycode: u16,
        scancode: u16,
        modifiers: u16,
    ) -> Option<NativeStreamerShortcutAction> {
        SHORTCUT_MATCHER
            .get()
            .and_then(|matcher| matcher.lock().ok())
            .and_then(|matcher| matcher.match_keydown(keycode, scancode, modifiers))
    }

    unsafe fn handle_shortcut_action(action: NativeStreamerShortcutAction) {
        match action {
            NativeStreamerShortcutAction::TogglePointerLock => {
                if let Some(hwnd) = captured_hwnd().or_else(protected_hwnd) {
                    let hwnd = hwnd as Hwnd;
                    if is_input_captured(hwnd) {
                        release_input_capture(hwnd);
                    } else {
                        begin_input_capture(hwnd);
                    }
                }
            }
            _ => {
                if shortcut_action_releases_input_capture(action) {
                    release_current_input_capture();
                }
                emit_input_event(NativeWindowInputEvent::Shortcut { action });
            }
        }
    }

    fn shortcut_action_releases_input_capture(action: NativeStreamerShortcutAction) -> bool {
        matches!(
            action,
            NativeStreamerShortcutAction::ToggleFullscreen
                | NativeStreamerShortcutAction::StopStream
        )
    }

    fn legacy_mouse_button(message: Uint, wparam: Wparam) -> Option<(u8, bool)> {
        match message {
            WM_LBUTTONDOWN => Some((1, true)),
            WM_LBUTTONUP => Some((1, false)),
            WM_MBUTTONDOWN => Some((2, true)),
            WM_MBUTTONUP => Some((2, false)),
            WM_RBUTTONDOWN => Some((3, true)),
            WM_RBUTTONUP => Some((3, false)),
            WM_XBUTTONDOWN | WM_XBUTTONUP => {
                let xbutton = ((wparam >> 16) & 0xffff) as u16;
                let button = match xbutton {
                    XBUTTON1 => 4,
                    XBUTTON2 => 5,
                    _ => return None,
                };
                Some((button, message == WM_XBUTTONDOWN))
            }
            _ => None,
        }
    }

    fn emit_input_event(event: NativeWindowInputEvent) {
        let Some(sender) = INPUT_EVENT_SENDER
            .get()
            .and_then(|sender| sender.lock().ok().and_then(|sender| sender.clone()))
        else {
            return;
        };
        let _ = sender.send(event);
    }

    fn clamp_i32_to_i16(value: i32) -> i16 {
        value.clamp(i16::MIN as i32, i16::MAX as i32) as i16
    }

    fn timestamp_us() -> u64 {
        STARTED_AT
            .get_or_init(Instant::now)
            .elapsed()
            .as_micros()
            .min(u128::from(u64::MAX)) as u64
    }

    unsafe fn hide_cursor() {
        while ShowCursor(0) >= 0 {}
    }

    unsafe fn show_cursor() {
        while ShowCursor(1) < 0 {}
    }
}

#[cfg(target_os = "windows")]
pub(crate) fn apply_render_surface_to_video_sink(
    sink: &gst::Element,
    surface: &NativeRenderSurface,
) -> Result<(), String> {
    let Some(window_handle) = surface.window_handle.as_deref() else {
        return Ok(());
    };

    let handle = parse_window_handle(window_handle)?;
    let overlay = sink
        .clone()
        .dynamic_cast::<gst_video::VideoOverlay>()
        .map_err(|_| {
            format!(
                "Native render sink {} does not implement GstVideoOverlay.",
                sink.name()
            )
        })?;
    let rect = normalized_render_rect(surface.visible.then_some(()).and(surface.rect.as_ref()));

    unsafe {
        overlay.set_window_handle(handle);
    }
    overlay.handle_events(false);
    overlay
        .set_render_rectangle(rect.x, rect.y, rect.width, rect.height)
        .map_err(|error| format!("Failed to set native render rectangle: {error}"))?;
    overlay.expose();
    Ok(())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn apply_render_surface_to_video_sink(
    _sink: &gst::Element,
    _surface: &NativeRenderSurface,
) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "windows")]
pub(crate) fn primary_display_refresh_hz() -> Option<u32> {
    const VREFRESH: i32 = 116;

    #[link(name = "user32")]
    extern "system" {
        fn GetDC(hwnd: *mut c_void) -> *mut c_void;
        fn ReleaseDC(hwnd: *mut c_void, hdc: *mut c_void) -> i32;
    }

    #[link(name = "gdi32")]
    extern "system" {
        fn GetDeviceCaps(hdc: *mut c_void, index: i32) -> i32;
    }

    let hdc = unsafe { GetDC(std::ptr::null_mut()) };
    if hdc.is_null() {
        return None;
    }

    let refresh = unsafe { GetDeviceCaps(hdc, VREFRESH) };
    unsafe {
        ReleaseDC(std::ptr::null_mut(), hdc);
    }

    (refresh > 1).then_some(refresh as u32)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn primary_display_refresh_hz() -> Option<u32> {
    None
}
