// Windows keyboard hook to block Escape key during streaming
// This prevents the browser from exiting pointer lock when ESC is pressed

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::sync::OnceLock;

#[cfg(windows)]
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, SetWindowsHookExW, UnhookWindowsHookEx, HHOOK, KBDLLHOOKSTRUCT,
    WH_KEYBOARD_LL, WM_KEYDOWN, WM_SYSKEYDOWN,
};
#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;

#[cfg(windows)]
static ESCAPE_BLOCKED: AtomicBool = AtomicBool::new(false);
#[cfg(windows)]
static HOOK_HANDLE: OnceLock<std::sync::Mutex<Option<HHOOK>>> = OnceLock::new();

#[cfg(windows)]
unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 && ESCAPE_BLOCKED.load(Ordering::SeqCst) {
        let kb_struct = &*(lparam.0 as *const KBDLLHOOKSTRUCT);

        // Check if it's Escape key (VK_ESCAPE = 0x1B = 27)
        if kb_struct.vkCode == VK_ESCAPE.0 as u32 {
            let msg_type = wparam.0 as u32;
            if msg_type == WM_KEYDOWN || msg_type == WM_SYSKEYDOWN {
                log::debug!("Blocking Escape key at OS level");
                // Return 1 to block the key
                return LRESULT(1);
            }
        }
    }

    // Call next hook in chain
    CallNextHookEx(HHOOK::default(), code, wparam, lparam)
}

#[cfg(windows)]
fn ensure_hook_installed() {
    let mutex = HOOK_HANDLE.get_or_init(|| std::sync::Mutex::new(None));
    let mut guard = mutex.lock().unwrap();

    if guard.is_none() {
        unsafe {
            let hook = SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook_proc),
                None,
                0,
            );

            match hook {
                Ok(h) => {
                    log::info!("Keyboard hook installed successfully");
                    *guard = Some(h);
                }
                Err(e) => {
                    log::error!("Failed to install keyboard hook: {:?}", e);
                }
            }
        }
    }
}

#[cfg(windows)]
pub fn block_escape_key(block: bool) {
    ensure_hook_installed();
    ESCAPE_BLOCKED.store(block, Ordering::SeqCst);
    log::info!("Escape key blocking: {}", if block { "enabled" } else { "disabled" });
}

#[cfg(windows)]
pub fn cleanup_hook() {
    if let Some(mutex) = HOOK_HANDLE.get() {
        let mut guard = mutex.lock().unwrap();
        if let Some(hook) = guard.take() {
            unsafe {
                let _ = UnhookWindowsHookEx(hook);
                log::info!("Keyboard hook removed");
            }
        }
    }
    ESCAPE_BLOCKED.store(false, Ordering::SeqCst);
}

// Non-Windows stubs
#[cfg(not(windows))]
pub fn block_escape_key(_block: bool) {
    log::debug!("Escape key blocking not supported on this platform");
}

#[cfg(not(windows))]
pub fn cleanup_hook() {}

// Tauri commands
#[tauri::command]
pub fn set_escape_block(block: bool) {
    block_escape_key(block);
}

#[tauri::command]
pub fn is_escape_blocked() -> bool {
    #[cfg(windows)]
    {
        ESCAPE_BLOCKED.load(Ordering::SeqCst)
    }
    #[cfg(not(windows))]
    {
        false
    }
}
