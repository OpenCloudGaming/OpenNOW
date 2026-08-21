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

    pub(crate) fn refresh_ordering(&mut self) -> Result<(), String> {
        self.inner.refresh_ordering()
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
        .ok_or_else(|| format!("invalid Electron native window handle: {value}"))
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn physical_rect(rect: RenderSurfaceRect, scale: f32) -> (i32, i32, u32, u32) {
    let scale = if scale.is_finite() {
        scale.clamp(0.25, 8.0)
    } else {
        1.0
    };
    (
        (rect.x as f32 * scale).round() as i32,
        (rect.y as f32 * scale).round() as i32,
        ((rect.width as f32 * scale).round() as u32).max(2),
        ((rect.height as f32 * scale).round() as u32).max(2),
    )
}

#[cfg(target_os = "windows")]
mod platform {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::Foundation::{GetLastError, SetLastError};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GWL_EXSTYLE, GWL_STYLE, GetWindowLongPtrW, HWND_BOTTOM, SW_HIDE, SWP_NOACTIVATE,
        SWP_SHOWWINDOW, SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_CHILD,
        WS_DISABLED, WS_EX_NOACTIVATE, WS_EX_TRANSPARENT, WS_VISIBLE,
    };

    use super::*;

    pub(crate) struct Surface {
        child: windows_sys::Win32::Foundation::HWND,
        parent: windows_sys::Win32::Foundation::HWND,
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
            })
        }

        pub(crate) fn attach_and_show(
            &mut self,
            parent_handle: &str,
            rect: RenderSurfaceRect,
            _screen_rect: Option<RenderSurfaceRect>,
            scale: f32,
        ) -> Result<(), String> {
            let parent = parse_handle(parent_handle)? as _;
            let (x, y, width, height) = physical_rect(rect, scale);
            unsafe {
                if self.parent != parent {
                    SetLastError(0);
                    if SetParent(self.child, parent).is_null() && GetLastError() != 0 {
                        return Err("failed to parent SDL video surface to Electron".to_owned());
                    }
                    self.parent = parent;
                    let style = GetWindowLongPtrW(self.child, GWL_STYLE) as u32;
                    SetWindowLongPtrW(
                        self.child,
                        GWL_STYLE,
                        ((style & !WS_VISIBLE) | WS_CHILD | WS_DISABLED) as isize,
                    );
                    let extended = GetWindowLongPtrW(self.child, GWL_EXSTYLE) as u32;
                    SetWindowLongPtrW(
                        self.child,
                        GWL_EXSTYLE,
                        (extended | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT) as isize,
                    );
                }
                if SetWindowPos(
                    self.child,
                    HWND_BOTTOM,
                    x,
                    y,
                    width as i32,
                    height as i32,
                    SWP_NOACTIVATE | SWP_SHOWWINDOW,
                ) == 0
                {
                    return Err("failed to position Electron child video surface".to_owned());
                }
            }
            Ok(())
        }

        pub(crate) fn hide(&mut self) {
            unsafe {
                ShowWindow(self.child, SW_HIDE);
            }
        }

        pub(crate) fn refresh_ordering(&mut self) -> Result<(), String> {
            Ok(())
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    use x11_dl::xlib;

    use super::*;

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
                        "native Electron surface embedding requires an X11/XWayland session"
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
            scale: f32,
        ) -> Result<(), String> {
            let parent = parse_handle(parent_handle)? as xlib::Window;
            let (x, y, width, height) = physical_rect(rect, scale);
            unsafe {
                if self.parent != parent {
                    (self.xlib.XUnmapWindow)(self.display, self.child);
                    (self.xlib.XReparentWindow)(self.display, self.child, parent, x, y);
                    (self.xlib.XSelectInput)(self.display, self.child, xlib::StructureNotifyMask);
                    self.parent = parent;
                }
                (self.xlib.XMoveResizeWindow)(self.display, self.child, x, y, width, height);
                (self.xlib.XLowerWindow)(self.display, self.child);
                (self.xlib.XMapWindow)(self.display, self.child);
                (self.xlib.XFlush)(self.display);
            }
            Ok(())
        }

        pub(crate) fn hide(&mut self) {
            unsafe {
                (self.xlib.XUnmapWindow)(self.display, self.child);
                (self.xlib.XFlush)(self.display);
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
