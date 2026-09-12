use opennow_streamer_protocol::RenderSurfaceRect;
use sdl2::video::Window;

pub(crate) struct NativeSurface {
    inner: platform::Surface,
}

impl NativeSurface {
    pub(crate) fn new(window: &Window) -> Result<Self, String> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            platform::Surface::new(window)
        }))
        .map_err(|_| "SDL could not expose a native presentation handle".to_owned())?
        .map(|inner| Self { inner })
    }

    pub(crate) fn attach_and_show(
        &mut self,
        parent_handle: &str,
        rect: RenderSurfaceRect,
        screen_rect: Option<RenderSurfaceRect>,
        scale: f32,
    ) -> Result<(), String> {
        self.inner
            .attach_and_show(parent_handle, rect, screen_rect, scale)
    }

    pub(crate) fn hide(&mut self) {
        self.inner.hide();
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn hide_checked(&mut self) -> Result<(), String> {
        self.inner.hide_checked()
    }

    pub(crate) fn refresh_ordering(&mut self) -> Result<(), String> {
        self.inner.refresh_ordering()
    }

    #[cfg(target_os = "windows")]
    pub(crate) fn window_handle(&self) -> isize {
        self.inner.window_handle()
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn parse_handle(value: &str) -> Result<usize, String> {
    let trimmed = value.trim();
    let parsed = if let Some(hex) = trimmed.strip_prefix("0x") {
        usize::from_str_radix(hex, 16)
    } else {
        trimmed.parse()
    };
    parsed
        .ok()
        .filter(|handle| *handle != 0)
        .ok_or_else(|| format!("invalid shell native window handle: {value}"))
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn physical_rect(rect: RenderSurfaceRect, _scale: f32) -> (i32, i32, u32, u32) {
    (rect.x, rect.y, rect.width.max(2), rect.height.max(2))
}

#[cfg(target_os = "windows")]
mod platform {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::SetFocus;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, HWND_TOP, SW_HIDE, SWP_FRAMECHANGED,
        SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetForegroundWindow,
        SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_CAPTION, WS_CHILD,
        WS_CLIPSIBLINGS, WS_DISABLED, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT,
        WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME, WS_VISIBLE,
    };

    use super::*;

    fn child_style(style: u32) -> u32 {
        (style
            & !(WS_VISIBLE
                | WS_POPUP
                | WS_DISABLED
                | WS_CAPTION
                | WS_THICKFRAME
                | WS_MINIMIZEBOX
                | WS_MAXIMIZEBOX
                | WS_SYSMENU))
            | WS_CHILD
            | WS_CLIPSIBLINGS
    }

    fn child_extended_style(style: u32) -> u32 {
        style & !(WS_EX_APPWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT)
    }

    pub(crate) struct Surface {
        child: windows_sys::Win32::Foundation::HWND,
        parent: windows_sys::Win32::Foundation::HWND,
        standalone_style: u32,
        standalone_extended_style: u32,
        shown: bool,
    }

    impl Surface {
        pub(crate) fn new(window: &Window) -> Result<Self, String> {
            let child = match window
                .window_handle()
                .map_err(|error| format!("SDL Win32 handle unavailable: {error}"))?
                .as_raw()
            {
                RawWindowHandle::Win32(handle) => handle.hwnd.get() as _,
                _ => return Err("SDL did not create a Win32 presentation window".to_owned()),
            };
            Ok(Self {
                child,
                parent: std::ptr::null_mut(),
                standalone_style: unsafe { GetWindowLongPtrW(child, GWL_STYLE) as u32 },
                standalone_extended_style: unsafe { GetWindowLongPtrW(child, GWL_EXSTYLE) as u32 },
                shown: false,
            })
        }

        pub(crate) fn window_handle(&self) -> isize {
            self.child as isize
        }

        pub(crate) fn attach_and_show(
            &mut self,
            parent_handle: &str,
            rect: RenderSurfaceRect,
            _screen_rect: Option<RenderSurfaceRect>,
            _scale: f32,
        ) -> Result<(), String> {
            let parent = parse_handle(parent_handle)? as _;
            let (x, y, width, height) = physical_rect(rect, _scale);
            unsafe {
                SetWindowLongPtrW(
                    self.child,
                    GWL_STYLE,
                    child_style(self.standalone_style) as isize,
                );
                SetWindowLongPtrW(
                    self.child,
                    GWL_EXSTYLE,
                    child_extended_style(self.standalone_extended_style) as isize,
                );
                if self.parent != parent {
                    SetLastError(0);
                    if SetParent(self.child, parent).is_null() && GetLastError() != 0 {
                        SetWindowLongPtrW(
                            self.child,
                            GWL_STYLE,
                            (self.standalone_style & !WS_VISIBLE) as isize,
                        );
                        SetWindowLongPtrW(
                            self.child,
                            GWL_EXSTYLE,
                            self.standalone_extended_style as isize,
                        );
                        return Err(
                            "failed to attach SDL video surface to the Qt window".to_owned()
                        );
                    }
                    self.parent = parent;
                }
                if SetWindowPos(
                    self.child,
                    HWND_TOP,
                    x,
                    y,
                    width as i32,
                    height as i32,
                    SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                ) == 0
                {
                    return Err("failed to position the Qt child video surface".to_owned());
                }
                if !self.shown {
                    let _ = SetForegroundWindow(parent);
                    let _ = SetFocus(self.child);
                    self.shown = true;
                }
            }
            Ok(())
        }

        pub(crate) fn hide_checked(&mut self) -> Result<(), String> {
            unsafe {
                ShowWindow(self.child, SW_HIDE);
                if !self.parent.is_null() {
                    SetLastError(0);
                    if SetParent(self.child, std::ptr::null_mut()).is_null() && GetLastError() != 0
                    {
                        self.shown = false;
                        return Err(
                            "failed to detach SDL video surface from the Qt window".to_owned()
                        );
                    }
                    self.parent = std::ptr::null_mut();
                    SetWindowLongPtrW(
                        self.child,
                        GWL_STYLE,
                        (self.standalone_style & !WS_VISIBLE) as isize,
                    );
                    SetWindowLongPtrW(
                        self.child,
                        GWL_EXSTYLE,
                        self.standalone_extended_style as isize,
                    );
                    let _ = SetWindowPos(
                        self.child,
                        std::ptr::null_mut(),
                        0,
                        0,
                        0,
                        0,
                        SWP_FRAMECHANGED | SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
                    );
                }
            }
            self.shown = false;
            Ok(())
        }

        pub(crate) fn hide(&mut self) {
            let _ = self.hide_checked();
        }

        pub(crate) fn refresh_ordering(&mut self) -> Result<(), String> {
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn embedded_presenter_is_an_enabled_input_capable_child() {
            let style = child_style(
                WS_POPUP
                    | WS_VISIBLE
                    | WS_DISABLED
                    | WS_CAPTION
                    | WS_THICKFRAME
                    | WS_MINIMIZEBOX
                    | WS_MAXIMIZEBOX
                    | WS_SYSMENU,
            );
            assert_ne!(style & WS_CHILD, 0);
            assert_ne!(style & WS_CLIPSIBLINGS, 0);
            assert_eq!(
                style
                    & (WS_POPUP
                        | WS_VISIBLE
                        | WS_DISABLED
                        | WS_CAPTION
                        | WS_THICKFRAME
                        | WS_MINIMIZEBOX
                        | WS_MAXIMIZEBOX
                        | WS_SYSMENU),
                0
            );

            let extended =
                child_extended_style(WS_EX_APPWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT);
            assert_eq!(
                extended & (WS_EX_APPWINDOW | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT),
                0
            );
        }
    }
}

#[cfg(all(test, any(target_os = "windows", target_os = "linux")))]
mod tests {
    use super::*;

    #[test]
    fn renderer_rect_is_already_physical_at_common_dpi_scales() {
        let rect = RenderSurfaceRect {
            x: 120,
            y: 80,
            width: 1280,
            height: 720,
        };

        for scale in [1.0, 1.5, 2.0] {
            assert_eq!(physical_rect(rect, scale), (120, 80, 1280, 720));
        }
    }

    #[test]
    fn physical_rect_only_clamps_empty_dimensions() {
        let rect = RenderSurfaceRect {
            x: -10,
            y: 20,
            width: 0,
            height: 1,
        };

        assert_eq!(physical_rect(rect, 2.0), (-10, 20, 2, 2));
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::sync::atomic::{AtomicI32, Ordering};

    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    use x11_dl::xlib;

    use super::*;

    static X11_ERROR_CODE: AtomicI32 = AtomicI32::new(0);

    unsafe extern "C" fn record_x11_error(
        _display: *mut xlib::Display,
        event: *mut xlib::XErrorEvent,
    ) -> i32 {
        if !event.is_null() {
            X11_ERROR_CODE.store(unsafe { (*event).error_code.into() }, Ordering::Release);
        }
        0
    }

    pub(crate) struct Surface {
        xlib: xlib::Xlib,
        display: *mut xlib::Display,
        child: xlib::Window,
        parent: xlib::Window,
    }

    impl Surface {
        pub(crate) fn new(window: &Window) -> Result<Self, String> {
            let child = match window
                .window_handle()
                .map_err(|error| format!("SDL X11 window handle unavailable: {error}"))?
                .as_raw()
            {
                RawWindowHandle::Xlib(handle) => handle.window,
                RawWindowHandle::Wayland(_) => {
                    return Err(
                        "native shell surface embedding requires an X11/XWayland session"
                            .to_owned(),
                    );
                }
                _ => return Err("SDL did not create an X11 presentation window".to_owned()),
            };
            let display = match window
                .display_handle()
                .map_err(|error| format!("SDL X11 display handle unavailable: {error}"))?
                .as_raw()
            {
                RawDisplayHandle::Xlib(handle) => handle
                    .display
                    .map(|display| display.as_ptr())
                    .unwrap_or(std::ptr::null_mut()),
                _ => std::ptr::null_mut(),
            };
            if display.is_null() {
                return Err("SDL X11 display pointer is null".to_owned());
            }
            Ok(Self {
                xlib: xlib::Xlib::open()
                    .map_err(|error| format!("failed to load X11 embedding API: {error}"))?,
                display: display.cast(),
                child,
                parent: 0,
            })
        }

        pub(crate) fn attach_and_show(
            &mut self,
            parent_handle: &str,
            rect: RenderSurfaceRect,
            _screen_rect: Option<RenderSurfaceRect>,
            _scale: f32,
        ) -> Result<(), String> {
            let parent = parse_handle(parent_handle)? as xlib::Window;
            let (x, y, width, height) = physical_rect(rect, _scale);
            let error_code = unsafe {
                (self.xlib.XSync)(self.display, xlib::False);
                X11_ERROR_CODE.store(0, Ordering::Release);
                let previous = (self.xlib.XSetErrorHandler)(Some(record_x11_error));
                if self.parent != parent {
                    (self.xlib.XUnmapWindow)(self.display, self.child);
                    (self.xlib.XReparentWindow)(self.display, self.child, parent, x, y);
                    (self.xlib.XSelectInput)(self.display, self.child, xlib::StructureNotifyMask);
                    self.parent = parent;
                }
                (self.xlib.XMoveResizeWindow)(self.display, self.child, x, y, width, height);
                (self.xlib.XLowerWindow)(self.display, self.child);
                (self.xlib.XMapWindow)(self.display, self.child);
                (self.xlib.XSync)(self.display, xlib::False);
                (self.xlib.XSetErrorHandler)(previous);
                X11_ERROR_CODE.load(Ordering::Acquire)
            };
            if error_code != 0 {
                self.parent = 0;
                return Err(format!(
                    "X11 could not attach the native child surface (error {error_code})"
                ));
            }
            Ok(())
        }

        pub(crate) fn hide(&mut self) {
            unsafe {
                (self.xlib.XSync)(self.display, xlib::False);
                X11_ERROR_CODE.store(0, Ordering::Release);
                let previous = (self.xlib.XSetErrorHandler)(Some(record_x11_error));
                (self.xlib.XUnmapWindow)(self.display, self.child);
                (self.xlib.XSync)(self.display, xlib::False);
                (self.xlib.XSetErrorHandler)(previous);
            }
        }

        pub(crate) fn refresh_ordering(&mut self) -> Result<(), String> {
            Ok(())
        }
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::time::{Duration, Instant};

    use objc2::rc::Retained;
    use objc2_app_kit::{NSView, NSWindow, NSWorkspace};
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};

    use super::*;

    const ORDERING_POLL_INTERVAL: Duration = Duration::from_millis(100);

    pub(crate) struct Surface {
        child: Retained<NSWindow>,
        raw_window: *mut sdl2::sys::SDL_Window,
        parent_pid: libc::pid_t,
        requested_visible: bool,
        ordered: bool,
        last_ordering_check: Option<Instant>,
    }

    impl Surface {
        pub(crate) fn new(window: &Window) -> Result<Self, String> {
            let child_view = match window
                .window_handle()
                .map_err(|error| format!("SDL AppKit handle unavailable: {error}"))?
                .as_raw()
            {
                RawWindowHandle::AppKit(handle) => unsafe {
                    &*handle.ns_view.as_ptr().cast::<NSView>()
                },
                _ => return Err("SDL did not create an AppKit presentation window".to_owned()),
            };
            let child = child_view
                .window()
                .ok_or_else(|| "SDL AppKit view has no window".to_owned())?;
            child.setIgnoresMouseEvents(true);
            Ok(Self {
                child,
                raw_window: window.raw(),
                parent_pid: unsafe { libc::getppid() },
                requested_visible: false,
                ordered: false,
                last_ordering_check: None,
            })
        }

        pub(crate) fn attach_and_show(
            &mut self,
            _parent_handle: &str,
            _rect: RenderSurfaceRect,
            screen_rect: Option<RenderSurfaceRect>,
            _scale: f32,
        ) -> Result<(), String> {
            let screen_rect = screen_rect.ok_or_else(|| {
                "macOS native surface is missing absolute screen bounds".to_owned()
            })?;
            let width = i32::try_from(screen_rect.width)
                .map_err(|_| "macOS native surface width is out of range".to_owned())?;
            let height = i32::try_from(screen_rect.height)
                .map_err(|_| "macOS native surface height is out of range".to_owned())?;
            unsafe {
                sdl2::sys::SDL_SetWindowPosition(self.raw_window, screen_rect.x, screen_rect.y);
                sdl2::sys::SDL_SetWindowSize(self.raw_window, width, height);
            }
            self.requested_visible = true;
            self.last_ordering_check = None;
            self.refresh_ordering()
        }

        pub(crate) fn hide(&mut self) {
            self.requested_visible = false;
            if self.ordered {
                unsafe {
                    sdl2::sys::SDL_HideWindow(self.raw_window);
                }
                self.ordered = false;
            }
        }

        pub(crate) fn refresh_ordering(&mut self) -> Result<(), String> {
            if self
                .last_ordering_check
                .is_some_and(|last| last.elapsed() < ORDERING_POLL_INTERVAL)
            {
                return Ok(());
            }
            self.last_ordering_check = Some(Instant::now());
            let should_order = self.requested_visible && self.parent_is_frontmost();
            if should_order == self.ordered {
                return Ok(());
            }
            unsafe {
                if should_order {
                    sdl2::sys::SDL_ShowWindow(self.raw_window);
                } else {
                    sdl2::sys::SDL_HideWindow(self.raw_window);
                }
            }
            if should_order {
                self.child.orderFrontRegardless();
            }
            self.ordered = should_order;
            Ok(())
        }

        fn parent_is_frontmost(&self) -> bool {
            NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .is_some_and(|application| application.processIdentifier() == self.parent_pid)
        }
    }
}

#[cfg(not(any(target_os = "windows", target_os = "linux", target_os = "macos")))]
mod platform {
    use super::*;

    pub(crate) struct Surface;

    impl Surface {
        pub(crate) fn new(_window: &Window) -> Result<Self, String> {
            Err("native presentation is unsupported on this operating system".to_owned())
        }

        pub(crate) fn attach_and_show(
            &mut self,
            _parent_handle: &str,
            _rect: RenderSurfaceRect,
            _screen_rect: Option<RenderSurfaceRect>,
            _scale: f32,
        ) -> Result<(), String> {
            Err("native presentation is unsupported on this operating system".to_owned())
        }

        pub(crate) fn hide(&mut self) {}

        pub(crate) fn refresh_ordering(&mut self) -> Result<(), String> {
            Ok(())
        }
    }
}
