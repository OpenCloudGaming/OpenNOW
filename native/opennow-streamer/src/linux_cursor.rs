#[cfg(target_os = "linux")]
use crate::linux_display_session::linux_has_x11_display;
#[cfg(target_os = "linux")]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(target_os = "linux")]
use std::sync::{Mutex, OnceLock};
#[cfg(target_os = "linux")]
use std::thread::{self, JoinHandle};

#[cfg(target_os = "linux")]
static CURSOR_HIDDEN: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "linux")]
static CURSOR_WORKER: OnceLock<Mutex<Option<JoinHandle<()>>>> = OnceLock::new();
#[cfg(target_os = "linux")]
static CURSOR_RELEASE: OnceLock<AtomicBool> = OnceLock::new();

#[cfg(target_os = "linux")]
pub(crate) fn linux_hide_stream_cursor() {
    if CURSOR_HIDDEN.swap(true, Ordering::SeqCst) {
        return;
    }

    let release = cursor_release_signal();
    release.store(false, Ordering::SeqCst);
    let handle = thread::spawn(move || linux_cursor_worker(release));
    let slot = CURSOR_WORKER.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        if let Some(previous) = guard.take() {
            let _ = previous.join();
        }
        *guard = Some(handle);
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn linux_show_stream_cursor() {
    if !CURSOR_HIDDEN.swap(false, Ordering::SeqCst) {
        return;
    }

    cursor_release_signal().store(true, Ordering::SeqCst);
    let slot = CURSOR_WORKER.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = slot.lock() {
        if let Some(handle) = guard.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn linux_hide_stream_cursor() {}

#[cfg(not(target_os = "linux"))]
pub(crate) fn linux_show_stream_cursor() {}

#[cfg(target_os = "linux")]
fn cursor_release_signal() -> &'static AtomicBool {
    CURSOR_RELEASE.get_or_init(|| AtomicBool::new(false))
}

#[cfg(target_os = "linux")]
fn linux_cursor_worker(release: &'static AtomicBool) {
    if !linux_has_x11_display() {
        while !release.load(Ordering::SeqCst) {
            thread::sleep(std::time::Duration::from_millis(50));
        }
        return;
    }

    type Display = std::ffi::c_void;
    type Window = u64;

    #[link(name = "X11")]
    unsafe extern "C" {
        fn XOpenDisplay(display_name: *const i8) -> *mut Display;
        fn XCloseDisplay(display: *mut Display) -> i32;
        fn XFlush(display: *mut Display) -> i32;
    }

    type XFixesHideCursorFn = unsafe extern "C" fn(*mut Display, Window);
    type XFixesShowCursorFn = unsafe extern "C" fn(*mut Display, Window);

    unsafe fn xfixes_cursor_functions() -> Option<(XFixesHideCursorFn, XFixesShowCursorFn)> {
        let library = libc::dlopen(c"libXfixes.so.3".as_ptr(), libc::RTLD_LAZY);
        if library.is_null() {
            return None;
        }

        let hide_symbol = libc::dlsym(library, c"XFixesHideCursor".as_ptr());
        let show_symbol = libc::dlsym(library, c"XFixesShowCursor".as_ptr());
        if hide_symbol.is_null() || show_symbol.is_null() {
            libc::dlclose(library);
            return None;
        }

        Some((
            std::mem::transmute(hide_symbol),
            std::mem::transmute(show_symbol),
        ))
    }

    unsafe {
        let display = XOpenDisplay(std::ptr::null());
        if display.is_null() {
            while !release.load(Ordering::SeqCst) {
                thread::sleep(std::time::Duration::from_millis(50));
            }
            return;
        }

        let Some((hide_cursor, show_cursor)) = xfixes_cursor_functions() else {
            XCloseDisplay(display);
            while !release.load(Ordering::SeqCst) {
                thread::sleep(std::time::Duration::from_millis(50));
            }
            return;
        };

        let overlay_window = crate::gstreamer_platform::linux_x11_fullscreen_overlay_window();
        if overlay_window != 0 {
            hide_cursor(display, overlay_window);
        }
        XFlush(display);

        while !release.load(Ordering::SeqCst) {
            thread::sleep(std::time::Duration::from_millis(50));
        }

        if overlay_window != 0 {
            show_cursor(display, overlay_window);
        }
        XFlush(display);
        XCloseDisplay(display);
    }
}
