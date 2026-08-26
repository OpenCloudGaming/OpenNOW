use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::media::MediaStreamConfig;

const FULL_SIZE: (u32, u32) = (480, 470);
const MINIMAL_SIZE: (u32, u32) = (180, 40);
#[cfg(any(target_os = "linux", test))]
const EDGE_MARGIN: u32 = 24;
const SAMPLE_INTERVAL: Duration = Duration::from_millis(250);
#[cfg(any(target_os = "linux", test))]
const OVERLAY_ALPHA: u16 = 245;

type Rgb = [u8; 3];

const BACKGROUND: Rgb = [3, 9, 7];
const BORDER: Rgb = [28, 55, 40];
const GREEN: Rgb = [47, 229, 119];
const MUTED: Rgb = [102, 116, 108];
const TEXT: Rgb = [216, 226, 220];
const AMBER: Rgb = [255, 184, 0];
const GRAPH: Rgb = [48, 111, 76];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OverlayMode {
    Hidden,
    Minimal,
    Full,
}

impl OverlayMode {
    fn next(self) -> Self {
        match self {
            Self::Hidden => Self::Minimal,
            Self::Minimal => Self::Full,
            Self::Full => Self::Hidden,
        }
    }

    pub(crate) const fn size(self) -> Option<(u32, u32)> {
        match self {
            Self::Hidden => None,
            Self::Minimal => Some(MINIMAL_SIZE),
            Self::Full => Some(FULL_SIZE),
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct OverlayFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgb: Vec<u8>,
}

pub(crate) struct NativeStatsOverlay {
    mode: OverlayMode,
    stream: MediaStreamConfig,
    decoder: &'static str,
    last_presented: u64,
    last_sample: Instant,
    measured_fps: f32,
    last_received_video_bytes: u64,
    measured_bitrate_bps: f32,
    fps_history: VecDeque<f32>,
    dropped_frames: u64,
    presentation_skips: u64,
    relative_mouse: bool,
    rendered: Option<OverlayFrame>,
}

impl NativeStatsOverlay {
    pub(crate) fn new(stream: MediaStreamConfig, decoder: &'static str) -> Self {
        Self {
            mode: OverlayMode::Hidden,
            stream,
            decoder,
            last_presented: 0,
            last_sample: Instant::now(),
            measured_fps: 0.0,
            last_received_video_bytes: 0,
            measured_bitrate_bps: 0.0,
            fps_history: VecDeque::with_capacity(18),
            dropped_frames: 0,
            presentation_skips: 0,
            relative_mouse: false,
            rendered: None,
        }
    }

    pub(crate) fn toggle(&mut self) {
        self.mode = self.mode.next();
        self.render();
        eprintln!("Native F3 stream stats overlay: {:?}", self.mode);
    }

    #[cfg(any(target_os = "windows", target_os = "macos"))]
    pub(crate) const fn mode(&self) -> OverlayMode {
        self.mode
    }

    pub(crate) fn update(
        &mut self,
        presented_frames: u64,
        dropped_frames: u64,
        relative_mouse: bool,
        received_video_bytes: u64,
    ) -> bool {
        self.dropped_frames = dropped_frames;
        self.relative_mouse = relative_mouse;
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_sample);
        if elapsed < SAMPLE_INTERVAL {
            return false;
        }
        self.measured_fps = presented_frames.saturating_sub(self.last_presented) as f32
            / elapsed.as_secs_f32().max(0.001);
        let bitrate_bps = measured_bitrate_bps(
            received_video_bytes.saturating_sub(self.last_received_video_bytes),
            elapsed,
        );
        self.measured_bitrate_bps = if self.measured_bitrate_bps <= 0.0 {
            bitrate_bps
        } else {
            self.measured_bitrate_bps * 0.65 + bitrate_bps * 0.35
        };
        self.last_presented = presented_frames;
        self.last_received_video_bytes = received_video_bytes;
        self.last_sample = now;
        if self.fps_history.len() == 18 {
            self.fps_history.pop_front();
        }
        self.fps_history.push_back(self.measured_fps);
        self.render();
        true
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn set_presentation_skips(&mut self, presentation_skips: u64) {
        self.presentation_skips = presentation_skips;
    }

    pub(crate) fn frame(&self) -> Option<&OverlayFrame> {
        self.rendered.as_ref()
    }

    #[cfg(any(target_os = "linux", test))]
    pub(crate) fn composite_rgb24(
        &self,
        destination: &mut [u8],
        width: u32,
        height: u32,
        stride: usize,
    ) {
        let Some(source) = self.frame() else {
            return;
        };
        let Some((origin_x, origin_y)) = overlay_origin(self.mode, width, height, source) else {
            return;
        };
        let copy_width = source.width.min(width.saturating_sub(origin_x));
        let copy_height = source.height.min(height.saturating_sub(origin_y));
        for row in 0..copy_height as usize {
            let source_start = row * source.width as usize * 3;
            let destination_start = (origin_y as usize + row) * stride + origin_x as usize * 3;
            let bytes = copy_width as usize * 3;
            let Some(source_row) = source.rgb.get(source_start..source_start + bytes) else {
                return;
            };
            let Some(destination_row) =
                destination.get_mut(destination_start..destination_start + bytes)
            else {
                return;
            };
            for (destination, source) in destination_row.iter_mut().zip(source_row) {
                *destination = blend(*destination, *source);
            }
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn composite_linux_frame(
        &self,
        frame: &mut opennow_streamer_platform_linux::DecodedVideoFrame,
    ) {
        use opennow_streamer_platform_linux::PixelFormat;

        if frame.format.pixel_format != PixelFormat::Nv12
            || (frame.vulkan.is_none() && frame.dmabuf.is_none() && frame.planes.len() != 2)
        {
            return;
        }
        let Some(source) = self.frame() else {
            return;
        };
        let Some((origin_x, origin_y)) =
            overlay_origin(self.mode, frame.format.width, frame.format.height, source)
        else {
            return;
        };
        let origin_x = origin_x & !1;
        let origin_y = origin_y & !1;
        let copy_width = source
            .width
            .min(frame.format.width.saturating_sub(origin_x))
            & !1;
        let copy_height = source
            .height
            .min(frame.format.height.saturating_sub(origin_y))
            & !1;
        if frame.vulkan.is_some() || frame.dmabuf.is_some() {
            let mut luma = vec![0_u8; (copy_width * copy_height) as usize];
            let mut chroma = vec![0_u8; (copy_width * copy_height / 2) as usize];
            for row in 0..copy_height as usize {
                for column in 0..copy_width as usize {
                    let source_offset = (row * source.width as usize + column) * 3;
                    let rgb = &source.rgb[source_offset..source_offset + 3];
                    let (y, _, _) = rgb_to_limited_bt709(rgb[0], rgb[1], rgb[2]);
                    luma[row * copy_width as usize + column] = y;
                }
            }
            for row in (0..copy_height as usize).step_by(2) {
                for column in (0..copy_width as usize).step_by(2) {
                    let mut rgb = [0_u32; 3];
                    for sample_y in row..row + 2 {
                        for sample_x in column..column + 2 {
                            let offset = (sample_y * source.width as usize + sample_x) * 3;
                            rgb[0] += u32::from(source.rgb[offset]);
                            rgb[1] += u32::from(source.rgb[offset + 1]);
                            rgb[2] += u32::from(source.rgb[offset + 2]);
                        }
                    }
                    let (_, u, v) = rgb_to_limited_bt709(
                        (rgb[0] / 4) as u8,
                        (rgb[1] / 4) as u8,
                        (rgb[2] / 4) as u8,
                    );
                    let offset = row / 2 * copy_width as usize + column;
                    chroma[offset] = u;
                    chroma[offset + 1] = v;
                }
            }
            frame.overlay = Some(opennow_streamer_platform_linux::FrameOverlay {
                origin_x,
                origin_y,
                width: copy_width,
                height: copy_height,
                luma: opennow_streamer_platform_linux::FramePlane {
                    data: std::sync::Arc::from(luma),
                    stride: copy_width as usize,
                    rows: copy_height as usize,
                },
                chroma: opennow_streamer_platform_linux::FramePlane {
                    data: std::sync::Arc::from(chroma),
                    stride: copy_width as usize,
                    rows: copy_height as usize / 2,
                },
            });
            return;
        }
        let (y_planes, uv_planes) = frame.planes.split_at_mut(1);
        let y_stride = y_planes[0].stride;
        let uv_stride = uv_planes[0].stride;
        let y_plane = std::sync::Arc::make_mut(&mut y_planes[0].data);
        let uv_plane = std::sync::Arc::make_mut(&mut uv_planes[0].data);

        for row in 0..copy_height as usize {
            for column in 0..copy_width as usize {
                let source_offset = (row * source.width as usize + column) * 3;
                let Some(rgb) = source.rgb.get(source_offset..source_offset + 3) else {
                    return;
                };
                let (y, _, _) = rgb_to_limited_bt709(rgb[0], rgb[1], rgb[2]);
                let destination_offset =
                    (origin_y as usize + row) * y_stride + origin_x as usize + column;
                let Some(destination) = y_plane.get_mut(destination_offset) else {
                    return;
                };
                *destination = blend(*destination, y);
            }
        }

        for row in (0..copy_height as usize).step_by(2) {
            for column in (0..copy_width as usize).step_by(2) {
                let mut red = 0_u32;
                let mut green = 0_u32;
                let mut blue = 0_u32;
                let mut samples = 0_u32;
                for sample_y in row..(row + 2).min(copy_height as usize) {
                    for sample_x in column..(column + 2).min(copy_width as usize) {
                        let offset = (sample_y * source.width as usize + sample_x) * 3;
                        let Some(rgb) = source.rgb.get(offset..offset + 3) else {
                            return;
                        };
                        red += u32::from(rgb[0]);
                        green += u32::from(rgb[1]);
                        blue += u32::from(rgb[2]);
                        samples += 1;
                    }
                }
                let samples = samples.max(1);
                let (_, u, v) = rgb_to_limited_bt709(
                    (red / samples) as u8,
                    (green / samples) as u8,
                    (blue / samples) as u8,
                );
                let destination_offset =
                    (origin_y as usize / 2 + row / 2) * uv_stride + origin_x as usize + column;
                let Some(destination) =
                    uv_plane.get_mut(destination_offset..destination_offset + 2)
                else {
                    return;
                };
                destination[0] = blend(destination[0], u);
                destination[1] = blend(destination[1], v);
            }
        }
    }

    fn render(&mut self) {
        let Some((width, height)) = self.mode.size() else {
            self.rendered = None;
            return;
        };
        let mut canvas = Raster::new(width, height, BACKGROUND);
        canvas.draw_rect(0, 0, width, height, BORDER);
        match self.mode {
            OverlayMode::Hidden => {}
            OverlayMode::Minimal => self.draw_minimal(&mut canvas),
            OverlayMode::Full => self.draw_full(&mut canvas),
        }
        self.rendered = Some(OverlayFrame {
            width,
            height,
            rgb: canvas.pixels,
        });
    }

    fn draw_minimal(&self, canvas: &mut Raster) {
        let fps = display_fps(self.measured_fps, self.stream.fps);
        canvas.draw_text(14, 10, 3, GREEN, &fps.to_string());
        canvas.draw_text(76, 15, 1, MUTED, "FPS");
        canvas.draw_text(105, 14, 2, MUTED, "|");
        let frame_time = if self.measured_fps > 1.0 {
            format!("{:.1} MS/F", 1000.0 / self.measured_fps)
        } else {
            "-- MS/F".to_owned()
        };
        canvas.draw_text(120, 15, 1, TEXT, &frame_time);
    }

    fn draw_full(&self, canvas: &mut Raster) {
        canvas.draw_text(20, 22, 2, GREEN, "NVST DEBUG");
        canvas.tag(328, 18, 64, "NTFK ON", GREEN);
        canvas.tag(400, 18, 62, "F3 OFF", MUTED);

        let fps = display_fps(self.measured_fps, self.stream.fps);
        let fps_text = fps.to_string();
        canvas.draw_text(20, 58, 7, GREEN, &fps_text);
        let fps_label_x = 20 + text_width(&fps_text, 7) as i32 + 10;
        canvas.draw_text(fps_label_x, 77, 2, TEXT, "FPS");
        let frame_ms = if self.measured_fps > 1.0 {
            format!("{:.1} MS / FRAME", 1000.0 / self.measured_fps)
        } else {
            "-- MS / FRAME".to_owned()
        };
        canvas.draw_text(fps_label_x, 99, 1, MUTED, &frame_ms);
        self.draw_graph(canvas, 220, 58, 242, 52);

        canvas.section(20, 132, "VIDEO");
        canvas.row(
            20,
            154,
            "CODEC",
            460,
            &self.stream.codec.label().to_ascii_uppercase(),
            TEXT,
        );
        canvas.row(20, 178, "DECODER", 460, self.decoder, TEXT);
        canvas.row(
            20,
            202,
            "STREAM",
            460,
            &format!(
                "{}X{} @ {}",
                self.stream.width, self.stream.height, self.stream.fps
            ),
            TEXT,
        );
        let pacing = if self.stream.cloud_gsync {
            format!("{frame_ms} / CLOUD VRR")
        } else {
            frame_ms.clone()
        };
        canvas.row(20, 226, "FRAME PERIOD", 460, &pacing, GREEN);
        canvas.row(
            20,
            250,
            "DROPS / SKIPS",
            460,
            &format!("{} / {}", self.dropped_frames, self.presentation_skips),
            if self.dropped_frames == 0 {
                TEXT
            } else {
                AMBER
            },
        );
        canvas.row(
            20,
            274,
            "CURSOR / INPUT",
            460,
            if self.relative_mouse {
                "LOCKED / RAW"
            } else {
                "ABSOLUTE / RAW"
            },
            GREEN,
        );
        canvas.divider(20, 299, 440);

        canvas.section(20, 317, "NETWORK");
        canvas.row(20, 339, "TRANSPORT", 460, "NVST / UDP", GREEN);
        canvas.row(20, 363, "ROUND TRIP / JITTER", 460, "-- / --", MUTED);
        canvas.row(20, 387, "PACKET LOSS", 460, "--", MUTED);
        canvas.row(
            20,
            411,
            "BITRATE NOW / MAX",
            460,
            &format!(
                "{:.1} / {:.1} MBPS",
                self.measured_bitrate_bps / 1_000_000.0,
                self.stream.bitrate_bps as f32 / 1_000_000.0,
            ),
            TEXT,
        );
        canvas.draw_text(20, 448, 1, MUTED, "F3: MINIMAL  /  FULL  /  OFF");
    }

    fn draw_graph(&self, canvas: &mut Raster, x: i32, y: i32, width: u32, height: u32) {
        canvas.fill_rect(x, y + height as i32, width, 1, BORDER);
        let target = self.stream.fps.max(1) as f32;
        for (index, sample) in self.fps_history.iter().enumerate() {
            let bar_height = ((*sample / target).clamp(0.05, 1.2) * height as f32) as u32;
            canvas.fill_rect(
                x + index as i32 * 14,
                y + height as i32 - bar_height as i32,
                10,
                bar_height,
                if *sample < target * 0.8 { AMBER } else { GRAPH },
            );
        }
    }
}

fn measured_bitrate_bps(bytes: u64, elapsed: Duration) -> f32 {
    bytes as f32 * 8.0 / elapsed.as_secs_f32().max(0.001)
}

#[cfg(any(target_os = "linux", test))]
fn overlay_origin(
    mode: OverlayMode,
    width: u32,
    height: u32,
    frame: &OverlayFrame,
) -> Option<(u32, u32)> {
    mode.size()?;
    let max_x = width.saturating_sub(frame.width);
    let max_y = height.saturating_sub(frame.height);
    Some(match mode {
        OverlayMode::Hidden => return None,
        OverlayMode::Minimal => (max_x.saturating_sub(EDGE_MARGIN), EDGE_MARGIN.min(max_y)),
        OverlayMode::Full => (EDGE_MARGIN.min(max_x), EDGE_MARGIN.min(max_y)),
    })
}

#[cfg(any(target_os = "linux", test))]
fn blend(destination: u8, source: u8) -> u8 {
    let inverse = 255 - OVERLAY_ALPHA;
    ((u16::from(source) * OVERLAY_ALPHA + u16::from(destination) * inverse + 127) / 255) as u8
}

#[cfg(target_os = "linux")]
fn rgb_to_limited_bt709(red: u8, green: u8, blue: u8) -> (u8, u8, u8) {
    let red = i32::from(red);
    let green = i32::from(green);
    let blue = i32::from(blue);
    let y = ((47 * red + 157 * green + 16 * blue + 128) >> 8) + 16;
    let u = ((-26 * red - 87 * green + 112 * blue + 128) >> 8) + 128;
    let v = ((112 * red - 102 * green - 10 * blue + 128) >> 8) + 128;
    (clamp_byte(y), clamp_byte(u), clamp_byte(v))
}

#[cfg(target_os = "linux")]
fn clamp_byte(value: i32) -> u8 {
    value.clamp(0, 255) as u8
}

struct Raster {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}

impl Raster {
    fn new(width: u32, height: u32, color: Rgb) -> Self {
        let mut pixels = vec![0_u8; width as usize * height as usize * 3];
        for pixel in pixels.chunks_exact_mut(3) {
            pixel.copy_from_slice(&color);
        }
        Self {
            width,
            height,
            pixels,
        }
    }

    fn fill_rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: Rgb) {
        let start_x = x.max(0) as u32;
        let start_y = y.max(0) as u32;
        let end_x = x.saturating_add_unsigned(width).max(0) as u32;
        let end_y = y.saturating_add_unsigned(height).max(0) as u32;
        for row in start_y.min(self.height)..end_y.min(self.height) {
            for column in start_x.min(self.width)..end_x.min(self.width) {
                let offset = (row as usize * self.width as usize + column as usize) * 3;
                self.pixels[offset..offset + 3].copy_from_slice(&color);
            }
        }
    }

    fn draw_rect(&mut self, x: i32, y: i32, width: u32, height: u32, color: Rgb) {
        if width == 0 || height == 0 {
            return;
        }
        self.fill_rect(x, y, width, 1, color);
        self.fill_rect(x, y + height as i32 - 1, width, 1, color);
        self.fill_rect(x, y, 1, height, color);
        self.fill_rect(x + width as i32 - 1, y, 1, height, color);
    }

    fn section(&mut self, x: i32, y: i32, label: &str) {
        self.draw_text(x, y, 1, MUTED, label);
    }

    fn divider(&mut self, x: i32, y: i32, width: u32) {
        self.fill_rect(x, y, width, 1, BORDER);
    }

    fn row(&mut self, x: i32, y: i32, label: &str, right: i32, value: &str, color: Rgb) {
        self.draw_text(x, y, 2, MUTED, label);
        self.draw_text(right - text_width(value, 2) as i32, y, 2, color, value);
    }

    fn tag(&mut self, x: i32, y: i32, width: u32, label: &str, color: Rgb) {
        self.draw_rect(x, y, width, 25, BORDER);
        self.draw_text(x + 8, y + 8, 1, color, label);
    }

    fn draw_text(&mut self, x: i32, y: i32, scale: u32, color: Rgb, text: &str) {
        let mut cursor_x = x;
        for character in text.to_ascii_uppercase().chars() {
            for (row, bits) in glyph(character).iter().enumerate() {
                for column in 0..5 {
                    if bits & (1 << (4 - column)) != 0 {
                        self.fill_rect(
                            cursor_x + column * scale as i32,
                            y + row as i32 * scale as i32,
                            scale,
                            scale,
                            color,
                        );
                    }
                }
            }
            cursor_x += 6 * scale as i32;
        }
    }
}

fn display_fps(measured: f32, target: u32) -> u32 {
    if measured > 1.0 {
        measured.round() as u32
    } else {
        target
    }
}

fn text_width(text: &str, scale: u32) -> u32 {
    text.chars().count() as u32 * 6 * scale
}

fn glyph(character: char) -> [u8; 7] {
    match character {
        'A' => [14, 17, 17, 31, 17, 17, 17],
        'B' => [30, 17, 17, 30, 17, 17, 30],
        'C' => [14, 17, 16, 16, 16, 17, 14],
        'D' => [30, 17, 17, 17, 17, 17, 30],
        'E' => [31, 16, 16, 30, 16, 16, 31],
        'F' => [31, 16, 16, 30, 16, 16, 16],
        'G' => [14, 17, 16, 23, 17, 17, 15],
        'H' => [17, 17, 17, 31, 17, 17, 17],
        'I' => [14, 4, 4, 4, 4, 4, 14],
        'J' => [7, 2, 2, 2, 18, 18, 12],
        'K' => [17, 18, 20, 24, 20, 18, 17],
        'L' => [16, 16, 16, 16, 16, 16, 31],
        'M' => [17, 27, 21, 21, 17, 17, 17],
        'N' => [17, 25, 21, 19, 17, 17, 17],
        'O' => [14, 17, 17, 17, 17, 17, 14],
        'P' => [30, 17, 17, 30, 16, 16, 16],
        'Q' => [14, 17, 17, 17, 21, 18, 13],
        'R' => [30, 17, 17, 30, 20, 18, 17],
        'S' => [15, 16, 16, 14, 1, 1, 30],
        'T' => [31, 4, 4, 4, 4, 4, 4],
        'U' => [17, 17, 17, 17, 17, 17, 14],
        'V' => [17, 17, 17, 17, 17, 10, 4],
        'W' => [17, 17, 17, 21, 21, 21, 10],
        'X' => [17, 17, 10, 4, 10, 17, 17],
        'Y' => [17, 17, 10, 4, 4, 4, 4],
        'Z' => [31, 1, 2, 4, 8, 16, 31],
        '0' => [14, 17, 19, 21, 25, 17, 14],
        '1' => [4, 12, 4, 4, 4, 4, 14],
        '2' => [14, 17, 1, 2, 4, 8, 31],
        '3' => [30, 1, 1, 14, 1, 1, 30],
        '4' => [2, 6, 10, 18, 31, 2, 2],
        '5' => [31, 16, 16, 30, 1, 1, 30],
        '6' => [14, 16, 16, 30, 17, 17, 14],
        '7' => [31, 1, 2, 4, 8, 8, 8],
        '8' => [14, 17, 17, 14, 17, 17, 14],
        '9' => [14, 17, 17, 15, 1, 1, 14],
        '.' => [0, 0, 0, 0, 0, 12, 12],
        ':' => [0, 12, 12, 0, 12, 12, 0],
        '/' => [1, 2, 2, 4, 8, 8, 16],
        '-' => [0, 0, 0, 31, 0, 0, 0],
        '|' => [4, 4, 4, 4, 4, 4, 4],
        '@' => [14, 17, 23, 21, 23, 16, 14],
        '%' => [17, 2, 4, 8, 16, 17, 0],
        _ => [0; 7],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlay_modes_cycle_predictably() {
        assert_eq!(OverlayMode::Hidden.next(), OverlayMode::Minimal);
        assert_eq!(OverlayMode::Minimal.next(), OverlayMode::Full);
        assert_eq!(OverlayMode::Full.next(), OverlayMode::Hidden);
    }

    #[test]
    fn fps_uses_target_until_live_sample_exists() {
        assert_eq!(display_fps(0.0, 120), 120);
        assert_eq!(display_fps(117.4, 120), 117);
    }

    #[test]
    fn measured_bitrate_uses_encoded_video_bytes_over_sample_time() {
        assert_eq!(
            measured_bitrate_bps(1_000_000, Duration::from_secs(1)),
            8_000_000.0,
        );
        assert_eq!(
            measured_bitrate_bps(500_000, Duration::from_millis(250)),
            16_000_000.0,
        );
    }

    #[test]
    fn rgb_overlay_is_composited_in_the_expected_corner() {
        let mut overlay = NativeStatsOverlay::new(MediaStreamConfig::default(), "TEST");
        overlay.toggle();
        let mut destination = vec![0_u8; 640 * 360 * 3];
        overlay.composite_rgb24(&mut destination, 640, 360, 640 * 3);
        let origin = (24 * 640 + (640 - MINIMAL_SIZE.0 - EDGE_MARGIN)) as usize * 3;
        assert_ne!(&destination[origin..origin + 3], &[0, 0, 0]);
        assert_eq!(&destination[..3], &[0, 0, 0]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn nv12_overlay_updates_luma_and_chroma_without_touching_other_pixels() {
        use std::sync::Arc;

        use opennow_streamer_platform_linux::{DecodedVideoFrame, FramePlane, StreamFormat};

        let mut overlay = NativeStatsOverlay::new(MediaStreamConfig::default(), "TEST");
        overlay.toggle();
        let mut frame = DecodedVideoFrame {
            format: StreamFormat::h264_default(640, 360).unwrap(),
            planes: vec![
                FramePlane {
                    data: Arc::from(vec![16_u8; 640 * 360]),
                    stride: 640,
                    rows: 360,
                },
                FramePlane {
                    data: Arc::from(vec![128_u8; 640 * 180]),
                    stride: 640,
                    rows: 180,
                },
            ],
            dmabuf: None,
            vulkan: None,
            overlay: None,
            timestamp_us: 0,
        };

        overlay.composite_linux_frame(&mut frame);

        let origin_x = 640 - MINIMAL_SIZE.0 as usize - EDGE_MARGIN as usize;
        let luma_origin = 24 * 640 + origin_x;
        let chroma_origin = 12 * 640 + origin_x;
        assert_ne!(frame.planes[0].data[luma_origin], 16);
        assert_ne!(
            &frame.planes[1].data[chroma_origin..chroma_origin + 2],
            &[128, 128]
        );
        assert_eq!(frame.planes[0].data[0], 16);
        assert_eq!(&frame.planes[1].data[..2], &[128, 128]);
    }
}
