//! Windows Raw Input API for low-latency mouse input
//! Uses WM_INPUT messages to get raw mouse deltas directly from hardware
//! This bypasses the need for cursor recentering and provides true 1:1 input

#[cfg(target_os = "windows")]
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
#[cfg(target_os = "windows")]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
static RAW_INPUT_REGISTERED: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static RAW_INPUT_ACTIVE: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "windows")]
static ACCUMULATED_DX: AtomicI32 = AtomicI32::new(0);
#[cfg(target_os = "windows")]
static ACCUMULATED_DY: AtomicI32 = AtomicI32::new(0);
#[cfg(target_os = "windows")]
static MESSAGE_WINDOW: Mutex<Option<isize>> = Mutex::new(None);

#[cfg(target_os = "windows")]
mod win32 {
    use std::ffi::c_void;
    use std::mem::size_of;

    pub type HWND = isize;
    pub type WPARAM = usize;
    pub type LPARAM = isize;
    pub type LRESULT = isize;
    pub type HINSTANCE = isize;
    pub type ATOM = u16;

    // Window messages
    pub const WM_INPUT: u32 = 0x00FF;
    pub const WM_DESTROY: u32 = 0x0002;

    // Raw input constants
    pub const RIDEV_INPUTSINK: u32 = 0x00000100;
    pub const RIDEV_REMOVE: u32 = 0x00000001;
    pub const RID_INPUT: u32 = 0x10000003;
    pub const RIM_TYPEMOUSE: u32 = 0;
    pub const MOUSE_MOVE_RELATIVE: u16 = 0x00;

    // HID usage page and usage for mouse
    pub const HID_USAGE_PAGE_GENERIC: u16 = 0x01;
    pub const HID_USAGE_GENERIC_MOUSE: u16 = 0x02;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct RAWINPUTDEVICE {
        pub usage_page: u16,
        pub usage: u16,
        pub flags: u32,
        pub hwnd_target: HWND,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct RAWINPUTHEADER {
        pub dw_type: u32,
        pub dw_size: u32,
        pub h_device: *mut c_void,
        pub w_param: WPARAM,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct RAWMOUSE {
        pub flags: u16,
        pub button_flags: u16,
        pub button_data: u16,
        pub raw_buttons: u32,
        pub last_x: i32,
        pub last_y: i32,
        pub extra_information: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub union RAWINPUT_DATA {
        pub mouse: RAWMOUSE,
        pub keyboard: [u8; 24], // RAWKEYBOARD placeholder
        pub hid: [u8; 40],      // RAWHID placeholder
    }

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct RAWINPUT {
        pub header: RAWINPUTHEADER,
        pub data: RAWINPUT_DATA,
    }

    #[repr(C)]
    pub struct WNDCLASSEXW {
        pub cb_size: u32,
        pub style: u32,
        pub lpfn_wnd_proc: Option<unsafe extern "system" fn(HWND, u32, WPARAM, LPARAM) -> LRESULT>,
        pub cb_cls_extra: i32,
        pub cb_wnd_extra: i32,
        pub h_instance: HINSTANCE,
        pub h_icon: *mut c_void,
        pub h_cursor: *mut c_void,
        pub hbr_background: *mut c_void,
        pub lpsz_menu_name: *const u16,
        pub lpsz_class_name: *const u16,
        pub h_icon_sm: *mut c_void,
    }

    #[repr(C)]
    pub struct MSG {
        pub hwnd: HWND,
        pub message: u32,
        pub w_param: WPARAM,
        pub l_param: LPARAM,
        pub time: u32,
        pub pt_x: i32,
        pub pt_y: i32,
    }

    #[link(name = "user32")]
    extern "system" {
        pub fn RegisterRawInputDevices(
            devices: *const RAWINPUTDEVICE,
            num_devices: u32,
            size: u32,
        ) -> i32;

        pub fn GetRawInputData(
            raw_input: *mut c_void,
            command: u32,
            data: *mut c_void,
            size: *mut u32,
            header_size: u32,
        ) -> u32;

        pub fn DefWindowProcW(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;

        pub fn RegisterClassExW(wc: *const WNDCLASSEXW) -> ATOM;

        pub fn CreateWindowExW(
            ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: HWND,
            menu: *mut c_void,
            instance: HINSTANCE,
            param: *mut c_void,
        ) -> HWND;

        pub fn DestroyWindow(hwnd: HWND) -> i32;

        pub fn GetMessageW(msg: *mut MSG, hwnd: HWND, filter_min: u32, filter_max: u32) -> i32;

        pub fn TranslateMessage(msg: *const MSG) -> i32;

        pub fn DispatchMessageW(msg: *const MSG) -> LRESULT;

        pub fn PostMessageW(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> i32;

        pub fn GetModuleHandleW(module_name: *const u16) -> HINSTANCE;

        pub fn PostQuitMessage(exit_code: i32);
    }

    /// Convert a Rust string to a null-terminated wide string
    pub fn to_wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Register for raw mouse input
    pub fn register_raw_mouse(hwnd: HWND) -> bool {
        let device = RAWINPUTDEVICE {
            usage_page: HID_USAGE_PAGE_GENERIC,
            usage: HID_USAGE_GENERIC_MOUSE,
            flags: RIDEV_INPUTSINK, // Receive input even when not focused
            hwnd_target: hwnd,
        };

        unsafe {
            RegisterRawInputDevices(
                &device,
                1,
                size_of::<RAWINPUTDEVICE>() as u32,
            ) != 0
        }
    }

    /// Unregister raw mouse input
    pub fn unregister_raw_mouse() -> bool {
        let device = RAWINPUTDEVICE {
            usage_page: HID_USAGE_PAGE_GENERIC,
            usage: HID_USAGE_GENERIC_MOUSE,
            flags: RIDEV_REMOVE,
            hwnd_target: 0,
        };

        unsafe {
            RegisterRawInputDevices(
                &device,
                1,
                size_of::<RAWINPUTDEVICE>() as u32,
            ) != 0
        }
    }

    /// Process a WM_INPUT message and extract mouse delta
    /// Uses a properly aligned stack buffer to avoid heap allocations
    pub fn process_raw_input(lparam: LPARAM) -> Option<(i32, i32)> {
        unsafe {
            // Use a properly aligned buffer for RAWINPUT struct
            // RAWINPUT contains pointers which need 8-byte alignment on x64
            #[repr(C, align(8))]
            struct AlignedBuffer {
                data: [u8; 64],
            }
            
            let mut buffer = AlignedBuffer { data: [0; 64] };
            let mut size: u32 = buffer.data.len() as u32;

            let result = GetRawInputData(
                lparam as *mut c_void,
                RID_INPUT,
                buffer.data.as_mut_ptr() as *mut c_void,
                &mut size,
                size_of::<RAWINPUTHEADER>() as u32,
            );

            if result == u32::MAX || result == 0 {
                return None;
            }

            // Parse the raw input - buffer is now properly aligned
            let raw = &*(buffer.data.as_ptr() as *const RAWINPUT);

            // Check if it's mouse input
            if raw.header.dw_type != RIM_TYPEMOUSE {
                return None;
            }

            let mouse = &raw.data.mouse;

            // Only process relative mouse movement
            if mouse.flags == MOUSE_MOVE_RELATIVE {
                if mouse.last_x != 0 || mouse.last_y != 0 {
                    return Some((mouse.last_x, mouse.last_y));
                }
            }

            None
        }
    }
}

#[cfg(target_os = "windows")]
use win32::*;

/// Window procedure for the message-only window
#[cfg(target_os = "windows")]
unsafe extern "system" fn raw_input_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_INPUT => {
            if RAW_INPUT_ACTIVE.load(Ordering::SeqCst) {
                if let Some((dx, dy)) = process_raw_input(lparam) {
                    // Accumulate deltas atomically
                    ACCUMULATED_DX.fetch_add(dx, Ordering::SeqCst);
                    ACCUMULATED_DY.fetch_add(dy, Ordering::SeqCst);
                }
            }
            0
        }
        WM_DESTROY => {
            PostQuitMessage(0);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

/// Start raw input capture
/// Creates a message-only window to receive WM_INPUT messages
#[cfg(target_os = "windows")]
pub fn start_raw_input() -> Result<(), String> {
    if RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
        RAW_INPUT_ACTIVE.store(true, Ordering::SeqCst);
        return Ok(());
    }

    // Spawn a thread to handle the message loop
    std::thread::spawn(|| {
        unsafe {
            let class_name = to_wide("OpenNOW_RawInput");
            let h_instance = GetModuleHandleW(std::ptr::null());

            // Register window class
            let wc = WNDCLASSEXW {
                cb_size: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: 0,
                lpfn_wnd_proc: Some(raw_input_wnd_proc),
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance,
                h_icon: std::ptr::null_mut(),
                h_cursor: std::ptr::null_mut(),
                hbr_background: std::ptr::null_mut(),
                lpsz_menu_name: std::ptr::null(),
                lpsz_class_name: class_name.as_ptr(),
                h_icon_sm: std::ptr::null_mut(),
            };

            if RegisterClassExW(&wc) == 0 {
                log::error!("Failed to register raw input window class");
                return;
            }

            // Create message-only window (HWND_MESSAGE = -3)
            let hwnd = CreateWindowExW(
                0,
                class_name.as_ptr(),
                std::ptr::null(),
                0,
                0, 0, 0, 0,
                -3isize, // HWND_MESSAGE - message-only window
                std::ptr::null_mut(),
                h_instance,
                std::ptr::null_mut(),
            );

            if hwnd == 0 {
                log::error!("Failed to create raw input window");
                return;
            }

            // Store window handle
            *MESSAGE_WINDOW.lock().unwrap() = Some(hwnd);

            // Register for raw mouse input
            if !register_raw_mouse(hwnd) {
                log::error!("Failed to register raw mouse input");
                DestroyWindow(hwnd);
                return;
            }

            RAW_INPUT_REGISTERED.store(true, Ordering::SeqCst);
            RAW_INPUT_ACTIVE.store(true, Ordering::SeqCst);
            log::info!("Raw input started - receiving hardware mouse deltas");

            // Message loop
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, 0, 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }

            // Cleanup
            RAW_INPUT_REGISTERED.store(false, Ordering::SeqCst);
            RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
            *MESSAGE_WINDOW.lock().unwrap() = None;
            log::info!("Raw input thread stopped");
        }
    });

    // Wait a bit for the thread to start
    std::thread::sleep(std::time::Duration::from_millis(50));

    if RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err("Failed to start raw input".to_string())
    }
}

/// Stop raw input capture (but keep the window for later reuse)
#[cfg(target_os = "windows")]
pub fn pause_raw_input() {
    RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);
    ACCUMULATED_DX.store(0, Ordering::SeqCst);
    ACCUMULATED_DY.store(0, Ordering::SeqCst);
}

/// Resume raw input capture
#[cfg(target_os = "windows")]
pub fn resume_raw_input() {
    if RAW_INPUT_REGISTERED.load(Ordering::SeqCst) {
        ACCUMULATED_DX.store(0, Ordering::SeqCst);
        ACCUMULATED_DY.store(0, Ordering::SeqCst);
        RAW_INPUT_ACTIVE.store(true, Ordering::SeqCst);
    }
}

/// Stop raw input completely and destroy the window
#[cfg(target_os = "windows")]
pub fn stop_raw_input() {
    RAW_INPUT_ACTIVE.store(false, Ordering::SeqCst);

    // Unregister raw input
    unregister_raw_mouse();

    // Post quit message to stop the message loop
    if let Some(hwnd) = *MESSAGE_WINDOW.lock().unwrap() {
        unsafe {
            PostMessageW(hwnd, WM_DESTROY, 0, 0);
        }
    }
}

/// Get accumulated mouse deltas and reset
#[cfg(target_os = "windows")]
pub fn get_raw_mouse_delta() -> (i32, i32) {
    let dx = ACCUMULATED_DX.swap(0, Ordering::SeqCst);
    let dy = ACCUMULATED_DY.swap(0, Ordering::SeqCst);
    (dx, dy)
}

/// Check if raw input is active
#[cfg(target_os = "windows")]
pub fn is_raw_input_active() -> bool {
    RAW_INPUT_ACTIVE.load(Ordering::SeqCst)
}

// Non-Windows stubs
#[cfg(not(target_os = "windows"))]
pub fn start_raw_input() -> Result<(), String> {
    Err("Raw input only supported on Windows".to_string())
}

#[cfg(not(target_os = "windows"))]
pub fn pause_raw_input() {}

#[cfg(not(target_os = "windows"))]
pub fn resume_raw_input() {}

#[cfg(not(target_os = "windows"))]
pub fn stop_raw_input() {}

#[cfg(not(target_os = "windows"))]
pub fn get_raw_mouse_delta() -> (i32, i32) {
    (0, 0)
}

#[cfg(not(target_os = "windows"))]
pub fn is_raw_input_active() -> bool {
    false
}

// Tauri commands
use tauri::command;

#[command]
pub fn start_raw_mouse_input() -> Result<bool, String> {
    start_raw_input()?;
    Ok(true)
}

#[command]
pub fn stop_raw_mouse_input() {
    stop_raw_input();
}

#[command]
pub fn pause_raw_mouse_input() {
    pause_raw_input();
}

#[command]
pub fn resume_raw_mouse_input() {
    resume_raw_input();
}

#[command]
pub fn get_raw_delta() -> (i32, i32) {
    get_raw_mouse_delta()
}

#[command]
pub fn is_raw_input_running() -> bool {
    is_raw_input_active()
}
