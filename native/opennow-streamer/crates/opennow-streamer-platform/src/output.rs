use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use opennow_streamer_protocol::RenderSurface;
use sdl2::audio::{AudioCallback, AudioDevice, AudioSpecDesired};
use sdl2::pixels::{Color, PixelFormatEnum};
use sdl2::rect::Rect;
use sdl2::render::{Texture, WindowCanvas};

use crate::native_surface::NativeSurface;

const AUDIO_SAMPLE_RATE: i32 = 48_000;
const AUDIO_CHANNELS: u8 = 2;
const AUDIO_BUFFER_FRAMES: u16 = 480;
const MAX_AUDIO_LATENCY_MS: usize = 120;

#[derive(Debug)]
pub(crate) struct DecodedVideoFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgb: Vec<u8>,
}

#[derive(Debug)]
pub(crate) struct OutputBuffers {
    video: Mutex<Option<DecodedVideoFrame>>,
    audio: Mutex<VecDeque<f32>>,
    audio_capacity: usize,
}

impl OutputBuffers {
    pub(crate) fn new() -> Self {
        Self {
            video: Mutex::new(None),
            audio: Mutex::new(VecDeque::with_capacity(
                AUDIO_SAMPLE_RATE as usize * AUDIO_CHANNELS as usize * MAX_AUDIO_LATENCY_MS / 1_000,
            )),
            audio_capacity: AUDIO_SAMPLE_RATE as usize
                * AUDIO_CHANNELS as usize
                * MAX_AUDIO_LATENCY_MS
                / 1_000,
        }
    }

    pub(crate) fn replace_video(&self, frame: DecodedVideoFrame) -> bool {
        self.video
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .replace(frame)
            .is_some()
    }

    pub(crate) fn take_video(&self) -> Option<DecodedVideoFrame> {
        self.video
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }

    pub(crate) fn push_audio(&self, samples: &[f32]) -> usize {
        let mut audio = self.audio.lock().unwrap_or_else(|error| error.into_inner());
        let overflow = audio
            .len()
            .saturating_add(samples.len())
            .saturating_sub(self.audio_capacity);
        if overflow > 0 {
            let drain_count = overflow.min(audio.len());
            audio.drain(..drain_count);
        }
        if samples.len() >= self.audio_capacity {
            audio.clear();
            audio.extend(
                samples[samples.len() - self.audio_capacity..]
                    .iter()
                    .copied(),
            );
        } else {
            audio.extend(samples.iter().copied());
        }
        overflow
    }

    pub(crate) fn clear(&self) {
        self.take_video();
        self.audio
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clear();
    }

    fn fill_audio(&self, destination: &mut [f32]) {
        let mut audio = self.audio.lock().unwrap_or_else(|error| error.into_inner());
        for sample in destination {
            *sample = audio.pop_front().unwrap_or(0.0);
        }
    }
}

struct StreamAudioCallback {
    output: Arc<OutputBuffers>,
}

impl AudioCallback for StreamAudioCallback {
    type Channel = f32;

    fn callback(&mut self, output: &mut [f32]) {
        self.output.fill_audio(output);
    }
}

pub(crate) struct SoftwareOutput {
    _sdl: sdl2::Sdl,
    canvas: WindowCanvas,
    texture: Option<Texture>,
    texture_size: Option<(u32, u32)>,
    event_pump: sdl2::EventPump,
    audio: AudioDevice<StreamAudioCallback>,
    output: Arc<OutputBuffers>,
    native_surface: Result<NativeSurface, String>,
    visible: bool,
    paused: bool,
}

impl SoftwareOutput {
    fn initialize(output: Arc<OutputBuffers>) -> Result<Self, String> {
        let sdl = sdl2::init().map_err(|error| format!("SDL initialization failed: {error}"))?;
        let video = sdl
            .video()
            .map_err(|error| format!("SDL video initialization failed: {error}"))?;
        let audio_subsystem = sdl
            .audio()
            .map_err(|error| format!("SDL audio initialization failed: {error}"))?;
        let window = video
            .window("OpenNOW Stream", 1280, 720)
            .position_centered()
            .resizable()
            .borderless()
            .hidden()
            .metal_view()
            .build()
            .map_err(|error| format!("native video window creation failed: {error}"))?;
        let mut canvas = window
            .into_canvas()
            .build()
            .map_err(|error| format!("native video renderer creation failed: {error}"))?;
        canvas.set_draw_color(Color::BLACK);
        canvas.clear();
        canvas.present();

        let desired = AudioSpecDesired {
            freq: Some(AUDIO_SAMPLE_RATE),
            channels: Some(AUDIO_CHANNELS),
            samples: Some(AUDIO_BUFFER_FRAMES),
        };
        let callback_output = Arc::clone(&output);
        let audio = audio_subsystem
            .open_playback(None, &desired, move |_| StreamAudioCallback {
                output: callback_output,
            })
            .map_err(|error| format!("native audio output creation failed: {error}"))?;
        if audio.spec().freq != AUDIO_SAMPLE_RATE || audio.spec().channels != AUDIO_CHANNELS {
            return Err(format!(
                "native audio output returned unsupported format: {} Hz, {} channels",
                audio.spec().freq,
                audio.spec().channels
            ));
        }
        let event_pump = sdl
            .event_pump()
            .map_err(|error| format!("native window event pump creation failed: {error}"))?;
        let native_surface = if video.current_video_driver() == "dummy" {
            Err("dummy SDL video driver has no native presentation handle".to_owned())
        } else {
            NativeSurface::new(canvas.window())
        };

        Ok(Self {
            _sdl: sdl,
            canvas,
            texture: None,
            texture_size: None,
            event_pump,
            audio,
            output,
            native_surface,
            visible: false,
            paused: false,
        })
    }

    fn start(&mut self, surface: Option<&RenderSurface>) -> Result<(), String> {
        self.paused = false;
        self.output.clear();
        self.audio.resume();
        if let Some(surface) = surface {
            self.update_surface(surface)?;
        }
        Ok(())
    }

    fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
        if paused {
            self.audio.pause();
            self.output.clear();
        } else {
            self.audio.resume();
        }
    }

    fn stop(&mut self) {
        self.audio.pause();
        self.output.clear();
        if let Ok(surface) = self.native_surface.as_mut() {
            surface.hide();
        }
        self.visible = false;
        self.paused = false;
        self.texture = None;
        self.texture_size = None;
    }

    fn update_surface(&mut self, surface: &RenderSurface) -> Result<(), String> {
        let Some(rect) = surface.rect.filter(|_| surface.visible) else {
            if let Ok(native_surface) = self.native_surface.as_mut() {
                native_surface.hide();
            }
            self.visible = false;
            return Ok(());
        };
        let parent_handle = surface
            .window_handle
            .as_deref()
            .ok_or_else(|| "visible native surface is missing Electron windowHandle".to_owned())?;
        self.native_surface
            .as_mut()
            .map_err(|error| error.clone())?
            .attach_and_show(
                parent_handle,
                rect,
                surface.screen_rect,
                surface.device_scale_factor,
            )?;
        self.visible = true;
        Ok(())
    }

    fn pump(&mut self) -> Result<(), String> {
        if let Ok(surface) = self.native_surface.as_mut() {
            surface.refresh_ordering()?;
        }
        for event in self.event_pump.poll_iter() {
            if matches!(event, sdl2::event::Event::Quit { .. }) {
                if let Ok(surface) = self.native_surface.as_mut() {
                    surface.hide();
                }
                self.visible = false;
            }
        }
        if self.paused || !self.visible {
            self.output.take_video();
            return Ok(());
        }
        let Some(frame) = self.output.take_video() else {
            return Ok(());
        };
        if self.texture_size != Some((frame.width, frame.height)) {
            self.texture = Some(
                self.canvas
                    .texture_creator()
                    .create_texture_streaming(PixelFormatEnum::RGB24, frame.width, frame.height)
                    .map_err(|error| format!("video texture creation failed: {error}"))?,
            );
            self.texture_size = Some((frame.width, frame.height));
        }
        let texture = self.texture.as_mut().expect("texture was just created");
        texture
            .update(None, &frame.rgb, frame.width as usize * 3)
            .map_err(|error| format!("video texture upload failed: {error}"))?;
        let (output_width, output_height) = self.canvas.output_size()?;
        let target = aspect_fit(frame.width, frame.height, output_width, output_height);
        self.canvas.set_draw_color(Color::BLACK);
        self.canvas.clear();
        self.canvas.copy(texture, None, target)?;
        self.canvas.present();
        Ok(())
    }
}

pub(crate) enum ActiveOutput {
    Software(SoftwareOutput),
    #[cfg(target_os = "macos")]
    Mac(crate::macos_backend::MacOutput),
}

impl ActiveOutput {
    pub(crate) fn initialize(
        output: Arc<OutputBuffers>,
        use_macos_hardware: bool,
    ) -> Result<Self, String> {
        #[cfg(target_os = "macos")]
        if use_macos_hardware {
            return Ok(Self::Mac(crate::macos_backend::MacOutput::initialize()));
        }
        let _ = use_macos_hardware;
        SoftwareOutput::initialize(output).map(Self::Software)
    }

    pub(crate) fn start(&mut self, surface: Option<&RenderSurface>) -> Result<(), String> {
        match self {
            Self::Software(output) => output.start(surface),
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.start(surface),
        }
    }

    pub(crate) fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        match self {
            Self::Software(output) => {
                output.set_paused(paused);
                Ok(())
            }
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.set_paused(paused),
        }
    }

    pub(crate) fn stop(&mut self) {
        match self {
            Self::Software(output) => output.stop(),
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.stop(),
        }
    }

    pub(crate) fn update_surface(&mut self, surface: &RenderSurface) -> Result<(), String> {
        match self {
            Self::Software(output) => output.update_surface(surface),
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.update_surface(surface),
        }
    }

    pub(crate) fn pump(&mut self) -> Result<(), String> {
        match self {
            Self::Software(output) => output.pump(),
            #[cfg(target_os = "macos")]
            Self::Mac(output) => output.pump(),
        }
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn configure_macos_h264(
        &mut self,
        parameter_sets: opennow_streamer_platform_macos::H264ParameterSets,
    ) -> Result<opennow_streamer_platform_macos::StreamSink, String> {
        match self {
            Self::Mac(output) => output.configure_h264(parameter_sets),
            Self::Software(_) => Err("VideoToolbox is not the selected media backend".to_owned()),
        }
    }
}

fn aspect_fit(source_width: u32, source_height: u32, width: u32, height: u32) -> Rect {
    let scale = (width as f64 / source_width as f64).min(height as f64 / source_height as f64);
    let target_width = (source_width as f64 * scale).round() as u32;
    let target_height = (source_height as f64 * scale).round() as u32;
    Rect::new(
        ((width - target_width) / 2) as i32,
        ((height - target_height) / 2) as i32,
        target_width,
        target_height,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_buffer_drops_oldest_samples() {
        let output = OutputBuffers {
            video: Mutex::new(None),
            audio: Mutex::new(VecDeque::new()),
            audio_capacity: 4,
        };
        assert_eq!(output.push_audio(&[1.0, 2.0, 3.0]), 0);
        assert_eq!(output.push_audio(&[4.0, 5.0, 6.0]), 2);
        let mut values = [0.0; 4];
        output.fill_audio(&mut values);
        assert_eq!(values, [3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn video_slot_keeps_only_the_latest_frame() {
        let output = OutputBuffers::new();
        assert!(!output.replace_video(DecodedVideoFrame {
            width: 1,
            height: 1,
            rgb: vec![1, 2, 3],
        }));
        assert!(output.replace_video(DecodedVideoFrame {
            width: 2,
            height: 1,
            rgb: vec![4; 6],
        }));
        assert_eq!(output.take_video().expect("frame").width, 2);
    }

    #[test]
    fn aspect_fit_letterboxes_without_stretching() {
        assert_eq!(
            aspect_fit(1920, 1080, 1000, 1000),
            Rect::new(0, 218, 1000, 563)
        );
    }
}
