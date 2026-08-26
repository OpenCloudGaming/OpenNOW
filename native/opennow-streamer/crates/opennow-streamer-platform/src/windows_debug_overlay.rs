use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::render::WindowCanvas;
use sdl2::video::{Window, WindowPos};
use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GWLP_HWNDPARENT, GetClientRect, GetWindowLongPtrW, SetWindowLongPtrW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT,
};

use crate::media::MediaStreamConfig;
use crate::native_stats_overlay::{NativeStatsOverlay, OverlayMode};

const EDGE_MARGIN: i32 = 24;

pub(crate) struct NativeDebugOverlay {
    canvas: WindowCanvas,
    parent: HWND,
    stats: NativeStatsOverlay,
}

impl NativeDebugOverlay {
    pub(crate) fn new(
        video: &sdl2::VideoSubsystem,
        parent: isize,
        stream: MediaStreamConfig,
        decoder: &'static str,
    ) -> Result<Self, String> {
        let (width, height) = OverlayMode::Minimal
            .size()
            .expect("minimal overlay has a size");
        let window = video
            .window("OpenNOW Stream Stats", width, height)
            .position(0, 0)
            .borderless()
            .hidden()
            .build()
            .map_err(|error| format!("native stats overlay creation failed: {error}"))?;
        let overlay_handle = hwnd(&window)?;
        let parent = parent as HWND;
        unsafe {
            SetWindowLongPtrW(overlay_handle, GWLP_HWNDPARENT, parent as isize);
            let extended = GetWindowLongPtrW(overlay_handle, GWL_EXSTYLE) as u32;
            SetWindowLongPtrW(
                overlay_handle,
                GWL_EXSTYLE,
                (extended | WS_EX_NOACTIVATE | WS_EX_TRANSPARENT | WS_EX_TOOLWINDOW) as isize,
            );
        }
        let mut canvas = window
            .into_canvas()
            .software()
            .build()
            .map_err(|error| format!("native stats overlay renderer failed: {error}"))?;
        let _ = canvas.window_mut().set_opacity(0.96);
        Ok(Self {
            canvas,
            parent,
            stats: NativeStatsOverlay::new(stream, decoder),
        })
    }

    pub(crate) fn toggle(&mut self) {
        self.stats.toggle();
        if self.stats.mode() == OverlayMode::Hidden {
            self.canvas.window_mut().hide();
        } else {
            self.refresh_position();
            self.draw();
            self.canvas.window_mut().show();
            self.canvas.window_mut().raise();
        }
    }

    pub(crate) fn hide(&mut self) {
        self.canvas.window_mut().hide();
    }

    pub(crate) fn show_if_enabled(&mut self) {
        if self.stats.mode() != OverlayMode::Hidden {
            self.refresh_position();
            self.draw();
            self.canvas.window_mut().show();
        }
    }

    pub(crate) fn update(
        &mut self,
        presented_frames: u64,
        dropped_frames: u64,
        relative_mouse: bool,
    ) {
        if self
            .stats
            .update(presented_frames, dropped_frames, relative_mouse)
            && self.stats.mode() != OverlayMode::Hidden
        {
            self.refresh_position();
            self.draw();
        }
    }

    fn refresh_position(&mut self) {
        let Some((client_x, client_y, client_width, _)) = client_bounds(self.parent) else {
            return;
        };
        let Some((requested_width, requested_height)) = self.stats.mode().size() else {
            return;
        };
        let width = match self.stats.mode() {
            OverlayMode::Full => {
                requested_width.min(client_width.saturating_sub(EDGE_MARGIN as u32 * 2).max(320))
            }
            OverlayMode::Minimal => requested_width,
            OverlayMode::Hidden => return,
        };
        let x = match self.stats.mode() {
            OverlayMode::Minimal => client_x + client_width as i32 - width as i32 - EDGE_MARGIN,
            OverlayMode::Full => client_x + EDGE_MARGIN,
            OverlayMode::Hidden => return,
        };
        let window = self.canvas.window_mut();
        let _ = window.set_size(width, requested_height);
        window.set_position(
            WindowPos::Positioned(x),
            WindowPos::Positioned(client_y + EDGE_MARGIN),
        );
    }

    fn draw(&mut self) {
        let Some(frame) = self.stats.frame() else {
            return;
        };
        self.canvas.set_draw_color(Color::RGB(3, 9, 7));
        self.canvas.clear();
        let texture_creator = self.canvas.texture_creator();
        let Ok(mut texture) = texture_creator.create_texture_streaming(
            PixelFormatEnum::RGB24,
            frame.width,
            frame.height,
        ) else {
            return;
        };
        if texture
            .update(None, &frame.rgb, frame.width as usize * 3)
            .is_err()
        {
            return;
        }
        let _ = self.canvas.copy(&texture, None, None);
        self.canvas.present();
    }
}

fn hwnd(window: &Window) -> Result<HWND, String> {
    match window
        .window_handle()
        .map_err(|error| format!("stats overlay Win32 handle unavailable: {error}"))?
        .as_raw()
    {
        RawWindowHandle::Win32(handle) => Ok(handle.hwnd.get() as HWND),
        _ => Err("stats overlay did not create a Win32 window".to_owned()),
    }
}

fn client_bounds(parent: HWND) -> Option<(i32, i32, u32, u32)> {
    let mut rect = RECT::default();
    let mut origin = POINT { x: 0, y: 0 };
    unsafe {
        if GetClientRect(parent, &mut rect) == 0 || ClientToScreen(parent, &mut origin) == 0 {
            return None;
        }
    }
    Some((
        origin.x,
        origin.y,
        (rect.right - rect.left).max(1) as u32,
        (rect.bottom - rect.top).max(1) as u32,
    ))
}
