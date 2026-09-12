use std::collections::HashSet;
use std::ffi::c_void;
use std::mem::{MaybeUninit, size_of};
use std::ptr::{null, null_mut};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::{self, JoinHandle};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
};
use windows_sys::Win32::UI::Input::{
    GetRawInputData, HRAWINPUT, MOUSE_MOVE_ABSOLUTE, RAWINPUT, RAWINPUTDEVICE, RAWINPUTHEADER,
    RID_INPUT, RIDEV_INPUTSINK, RIDEV_REMOVE, RIM_TYPEMOUSE, RegisterRawInputDevices,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA,
    GetForegroundWindow, GetMessageW, GetWindowLongPtrW, HWND_MESSAGE, MSG, PostMessageW,
    PostQuitMessage, RI_MOUSE_BUTTON_4_DOWN, RI_MOUSE_BUTTON_4_UP, RI_MOUSE_BUTTON_5_DOWN,
    RI_MOUSE_BUTTON_5_UP, RI_MOUSE_HWHEEL, RI_MOUSE_LEFT_BUTTON_DOWN, RI_MOUSE_LEFT_BUTTON_UP,
    RI_MOUSE_MIDDLE_BUTTON_DOWN, RI_MOUSE_MIDDLE_BUTTON_UP, RI_MOUSE_RIGHT_BUTTON_DOWN,
    RI_MOUSE_RIGHT_BUTTON_UP, RI_MOUSE_WHEEL, RegisterClassW, SetWindowLongPtrW, TranslateMessage,
    WM_APP, WM_CLOSE, WM_INPUT, WM_NCCREATE, WM_NCDESTROY, WNDCLASSW,
};

use crate::media::{CapturedInput, CapturedInputQueue};

const RAW_INPUT_CLASS: &[u16] = &[
    b'O' as u16,
    b'p' as u16,
    b'e' as u16,
    b'n' as u16,
    b'N' as u16,
    b'O' as u16,
    b'W' as u16,
    b'R' as u16,
    b'a' as u16,
    b'w' as u16,
    b'I' as u16,
    b'n' as u16,
    b'p' as u16,
    b'u' as u16,
    b't' as u16,
    0,
];
const WM_RAW_INPUT_REREGISTER: u32 = WM_APP + 1;

struct RawInputState {
    foreground_owner: AtomicIsize,
    enabled: AtomicBool,
    relative_motion: AtomicBool,
    pressed_buttons: Mutex<HashSet<u8>>,
    captured_input: Arc<CapturedInputQueue>,
}

pub(crate) struct WindowsRawInputController {
    state: Arc<RawInputState>,
    message_window: isize,
    worker: Option<JoinHandle<()>>,
}

impl WindowsRawInputController {
    pub(crate) fn start(
        foreground_owner: isize,
        captured_input: Arc<CapturedInputQueue>,
    ) -> Result<Self, String> {
        let state = Arc::new(RawInputState {
            foreground_owner: AtomicIsize::new(foreground_owner),
            enabled: AtomicBool::new(false),
            relative_motion: AtomicBool::new(false),
            pressed_buttons: Mutex::new(HashSet::new()),
            captured_input,
        });
        let thread_state = Arc::clone(&state);
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let worker = thread::Builder::new()
            .name("opennow-raw-input".to_owned())
            .spawn(move || run_raw_input_thread(thread_state, ready_sender))
            .map_err(|error| format!("failed to spawn Raw Input thread: {error}"))?;
        let message_window = match ready_receiver.recv() {
            Ok(Ok(hwnd)) => hwnd,
            Ok(Err(error)) => {
                let _ = worker.join();
                return Err(error);
            }
            Err(_) => {
                let _ = worker.join();
                return Err("Raw Input thread exited during initialization".to_owned());
            }
        };
        Ok(Self {
            state,
            message_window,
            worker: Some(worker),
        })
    }

    pub(crate) fn set_foreground_owner(&self, foreground_owner: isize) {
        self.state
            .foreground_owner
            .store(foreground_owner, Ordering::Release);
    }

    pub(crate) fn set_capture(&self, enabled: bool, relative_motion: bool) {
        let motion_changed = self
            .state
            .relative_motion
            .swap(relative_motion, Ordering::AcqRel)
            != relative_motion;
        let enabled_changed = self.state.enabled.swap(enabled, Ordering::AcqRel) != enabled;
        if enabled && (enabled_changed || motion_changed) {
            // SDL also uses Raw Input for relative mode. Register our dedicated
            // message window after SDL toggles the mode so motion is delivered
            // directly to this thread rather than SDL's event pump.
            unsafe {
                let _ = PostMessageW(self.message_window as HWND, WM_RAW_INPUT_REREGISTER, 0, 0);
            }
        } else if enabled_changed {
            release_pressed_buttons(&self.state);
        }
    }

    pub(crate) fn release_buttons(&self) {
        release_pressed_buttons(&self.state);
    }
}

impl Drop for WindowsRawInputController {
    fn drop(&mut self) {
        self.state.enabled.store(false, Ordering::Release);
        release_pressed_buttons(&self.state);
        unsafe {
            let _ = PostMessageW(self.message_window as HWND, WM_CLOSE, 0, 0);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn run_raw_input_thread(state: Arc<RawInputState>, ready: mpsc::SyncSender<Result<isize, String>>) {
    unsafe {
        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
        let instance = GetModuleHandleW(null());
        let window_class = WNDCLASSW {
            lpfnWndProc: Some(raw_input_window_proc),
            hInstance: instance,
            lpszClassName: RAW_INPUT_CLASS.as_ptr(),
            ..Default::default()
        };
        let _ = RegisterClassW(&window_class);
        let state_pointer = Arc::as_ptr(&state);
        let hwnd = CreateWindowExW(
            0,
            RAW_INPUT_CLASS.as_ptr(),
            RAW_INPUT_CLASS.as_ptr(),
            0,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut(),
            instance,
            state_pointer.cast::<c_void>(),
        );
        if hwnd.is_null() {
            let _ = ready.send(Err("failed to create Raw Input message window".to_owned()));
            return;
        }
        if !register_raw_mouse(hwnd) {
            let _ = DestroyWindow(hwnd);
            let _ = ready.send(Err("failed to register the Raw Input mouse".to_owned()));
            return;
        }
        let _ = ready.send(Ok(hwnd as isize));
        let mut message = MSG::default();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn raw_input_window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = unsafe { &*(lparam as *const CREATESTRUCTW) };
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize);
        }
        return 1;
    }

    let state_pointer = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const RawInputState;
    match message {
        WM_RAW_INPUT_REREGISTER => {
            // Internal wake after SDL changes relative mode: reclaim the raw
            // mouse registration for this dedicated message thread.
            unsafe {
                let _ = register_raw_mouse(hwnd);
            }
            0
        }
        WM_INPUT => {
            if !state_pointer.is_null() {
                unsafe {
                    process_raw_input(&*state_pointer, lparam as HRAWINPUT);
                }
            }
            0
        }
        WM_CLOSE => {
            unsafe {
                unregister_raw_mouse();
                let _ = DestroyWindow(hwnd);
            }
            0
        }
        WM_NCDESTROY => {
            unsafe {
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

unsafe fn register_raw_mouse(hwnd: HWND) -> bool {
    let device = RAWINPUTDEVICE {
        usUsagePage: 0x01,
        usUsage: 0x02,
        dwFlags: RIDEV_INPUTSINK,
        hwndTarget: hwnd,
    };
    unsafe { RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32) != 0 }
}

unsafe fn unregister_raw_mouse() {
    let device = RAWINPUTDEVICE {
        usUsagePage: 0x01,
        usUsage: 0x02,
        dwFlags: RIDEV_REMOVE,
        hwndTarget: null_mut(),
    };
    unsafe {
        let _ = RegisterRawInputDevices(&device, 1, size_of::<RAWINPUTDEVICE>() as u32);
    }
}

unsafe fn process_raw_input(state: &RawInputState, handle: HRAWINPUT) {
    if !state.enabled.load(Ordering::Acquire) {
        return;
    }
    let mut raw = MaybeUninit::<RAWINPUT>::uninit();
    let mut size = size_of::<RAWINPUT>() as u32;
    let copied = unsafe {
        GetRawInputData(
            handle,
            RID_INPUT,
            raw.as_mut_ptr().cast::<c_void>(),
            &mut size,
            size_of::<RAWINPUTHEADER>() as u32,
        )
    };
    if copied == u32::MAX || copied < size_of::<RAWINPUTHEADER>() as u32 {
        return;
    }
    let raw = unsafe { raw.assume_init() };
    if raw.header.dwType != RIM_TYPEMOUSE {
        return;
    }
    let mouse = unsafe { raw.data.mouse };
    let owns_foreground =
        unsafe { GetForegroundWindow() } as isize == state.foreground_owner.load(Ordering::Acquire);
    if owns_foreground
        && state.relative_motion.load(Ordering::Acquire)
        && mouse.usFlags & MOUSE_MOVE_ABSOLUTE == 0
    {
        // SDL continues to own absolute cursor coordinates. This thread owns raw relative
        // deltas, plus buttons and wheel in both cursor modes.
        push_mouse_delta(&state.captured_input, mouse.lLastX, mouse.lLastY);
    }

    let buttons = unsafe { mouse.Anonymous.Anonymous };
    let button_flags = if owns_foreground {
        buttons.usButtonFlags
    } else {
        // A click can transiently move foreground before WM_INPUT delivers the
        // matching release. Never discard releases for buttons that we already
        // sent down: doing so leaves both the local de-duplicator and host stuck.
        buttons.usButtonFlags & raw_mouse_button_up_mask()
    };
    push_raw_mouse_buttons(state, button_flags);
    if owns_foreground && u32::from(buttons.usButtonFlags) & RI_MOUSE_WHEEL != 0 {
        state.captured_input.push(CapturedInput::MouseWheel {
            delta_x: 0,
            delta_y: buttons.usButtonData as i16,
        });
    }
    if owns_foreground && u32::from(buttons.usButtonFlags) & RI_MOUSE_HWHEEL != 0 {
        state.captured_input.push(CapturedInput::MouseWheel {
            delta_x: buttons.usButtonData as i16,
            delta_y: 0,
        });
    }
}

fn raw_mouse_button_up_mask() -> u16 {
    (RI_MOUSE_LEFT_BUTTON_UP
        | RI_MOUSE_MIDDLE_BUTTON_UP
        | RI_MOUSE_RIGHT_BUTTON_UP
        | RI_MOUSE_BUTTON_4_UP
        | RI_MOUSE_BUTTON_5_UP) as u16
}

fn push_raw_mouse_buttons(state: &RawInputState, flags: u16) {
    const BUTTON_FLAGS: &[(u32, u32, u8)] = &[
        (RI_MOUSE_LEFT_BUTTON_DOWN, RI_MOUSE_LEFT_BUTTON_UP, 1),
        (RI_MOUSE_MIDDLE_BUTTON_DOWN, RI_MOUSE_MIDDLE_BUTTON_UP, 2),
        (RI_MOUSE_RIGHT_BUTTON_DOWN, RI_MOUSE_RIGHT_BUTTON_UP, 3),
        (RI_MOUSE_BUTTON_4_DOWN, RI_MOUSE_BUTTON_4_UP, 4),
        (RI_MOUSE_BUTTON_5_DOWN, RI_MOUSE_BUTTON_5_UP, 5),
    ];
    let flags = u32::from(flags);
    for &(down, up, button) in BUTTON_FLAGS {
        if flags & down != 0 {
            push_mouse_button(state, button, true);
        }
        if flags & up != 0 {
            push_mouse_button(state, button, false);
        }
    }
}

fn push_mouse_button(state: &RawInputState, button: u8, pressed: bool) {
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

fn release_pressed_buttons(state: &RawInputState) {
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
    use std::sync::Arc;
    use std::sync::atomic::Ordering;

    use super::{RawInputState, push_mouse_delta, push_raw_mouse_buttons, release_pressed_buttons};
    use crate::media::{CapturedInput, CapturedInputQueue};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        RI_MOUSE_LEFT_BUTTON_DOWN, RI_MOUSE_LEFT_BUTTON_UP, RI_MOUSE_RIGHT_BUTTON_DOWN,
    };

    fn state() -> RawInputState {
        RawInputState {
            foreground_owner: Default::default(),
            enabled: true.into(),
            relative_motion: false.into(),
            pressed_buttons: Default::default(),
            captured_input: Arc::new(CapturedInputQueue::default()),
        }
    }

    #[test]
    fn large_raw_mouse_deltas_are_split_without_loss() {
        let queue = CapturedInputQueue::default();
        push_mouse_delta(&queue, 40_000, -40_000);

        let mut total_x = 0_i32;
        let mut total_y = 0_i32;
        while let Some(CapturedInput::MouseMove { delta_x, delta_y }) = queue.take() {
            total_x += i32::from(delta_x);
            total_y += i32::from(delta_y);
        }
        assert_eq!((total_x, total_y), (40_000, -40_000));
    }

    #[test]
    fn raw_buttons_keep_one_owner_across_cursor_mode_changes() {
        let state = state();
        push_raw_mouse_buttons(
            &state,
            (RI_MOUSE_LEFT_BUTTON_DOWN | RI_MOUSE_RIGHT_BUTTON_DOWN) as u16,
        );
        // A server cursor update can switch motion between SDL absolute and Raw Input relative,
        // but button ownership and pressed state remain on this controller.
        state.relative_motion.store(true, Ordering::Release);
        push_raw_mouse_buttons(&state, RI_MOUSE_LEFT_BUTTON_DOWN as u16);
        state.relative_motion.store(false, Ordering::Release);
        push_raw_mouse_buttons(&state, RI_MOUSE_LEFT_BUTTON_UP as u16);

        assert_eq!(
            state.captured_input.take(),
            Some(CapturedInput::MouseButton {
                button: 1,
                pressed: true,
            })
        );
        assert_eq!(
            state.captured_input.take(),
            Some(CapturedInput::MouseButton {
                button: 3,
                pressed: true,
            })
        );
        assert_eq!(
            state.captured_input.take(),
            Some(CapturedInput::MouseButton {
                button: 1,
                pressed: false,
            })
        );

        state.enabled.store(false, Ordering::Release);
        release_pressed_buttons(&state);
        assert_eq!(
            state.captured_input.take(),
            Some(CapturedInput::MouseButton {
                button: 3,
                pressed: false,
            })
        );
        assert_eq!(state.captured_input.take(), None);
    }
}
