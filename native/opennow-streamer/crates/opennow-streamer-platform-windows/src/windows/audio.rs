use std::collections::VecDeque;
use std::time::{Duration, Instant};

use ::windows::Win32::Media::Audio::{
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator,
    MMDeviceEnumerator, WAVEFORMATEX, eConsole, eRender,
};
use ::windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};

use crate::queue::BoundedQueue;
use crate::{AudioFormat, DefaultEndpointTracker, PcmFrame};

const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
const BUFFER_DURATION_100NS: i64 = 200_000;
const ENDPOINT_CHECK_INTERVAL: Duration = Duration::from_millis(250);

pub(super) struct AudioRenderer {
    enumerator: IMMDeviceEnumerator,
    render: IAudioRenderClient,
    client: IAudioClient,
    buffer_frames: u32,
    format: AudioFormat,
    pending: VecDeque<f32>,
    pending_capacity: usize,
    paused: bool,
    endpoint_tracker: DefaultEndpointTracker,
    next_endpoint_check: Instant,
}

impl AudioRenderer {
    pub(super) fn probe() -> Result<(), String> {
        let mut renderer = Self::new(AudioFormat {
            sample_rate: 48_000,
            channels: 2,
        })?;
        renderer.stop();
        Ok(())
    }

    pub(super) fn new(format: AudioFormat) -> Result<Self, String> {
        let block_align = format
            .channels
            .checked_mul(4)
            .ok_or("PCM block alignment overflow")?;
        let wave_format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
            nChannels: format.channels,
            nSamplesPerSec: format.sample_rate,
            nAvgBytesPerSec: format
                .sample_rate
                .checked_mul(block_align as u32)
                .ok_or("PCM byte rate overflow")?,
            nBlockAlign: block_align,
            wBitsPerSample: 32,
            cbSize: 0,
        };

        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|error| error.to_string())?;
            let device = enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|error| error.to_string())?;
            let endpoint_id = device_id(&device)?;
            let client: IAudioClient = device
                .Activate(CLSCTX_ALL, None)
                .map_err(|error| error.to_string())?;
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    BUFFER_DURATION_100NS,
                    0,
                    &wave_format,
                    None,
                )
                .map_err(|error| error.to_string())?;
            let buffer_frames = client.GetBufferSize().map_err(|error| error.to_string())?;
            let render: IAudioRenderClient =
                client.GetService().map_err(|error| error.to_string())?;
            client.Start().map_err(|error| error.to_string())?;
            Ok(Self {
                enumerator,
                render,
                client,
                buffer_frames,
                format,
                pending: VecDeque::new(),
                pending_capacity: buffer_frames as usize * format.channels as usize * 2,
                paused: false,
                endpoint_tracker: DefaultEndpointTracker::new(endpoint_id),
                next_endpoint_check: Instant::now() + ENDPOINT_CHECK_INTERVAL,
            })
        }
    }

    pub(super) fn default_endpoint_changed(&mut self) -> Result<bool, String> {
        let now = Instant::now();
        if now < self.next_endpoint_check {
            return Ok(false);
        }
        self.next_endpoint_check = now + ENDPOINT_CHECK_INTERVAL;
        let device = unsafe {
            self.enumerator
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|error| error.to_string())?
        };
        let endpoint_id = device_id(&device)?;
        Ok(self.endpoint_tracker.observe(endpoint_id))
    }

    pub(super) fn render(&mut self, queue: &BoundedQueue<PcmFrame>) -> Result<bool, String> {
        let mut dropped = false;
        while let Some(frame) = queue.try_pop() {
            if frame.format != self.format {
                return Err(format!(
                    "PCM packet format {:?} does not match active format {:?}",
                    frame.format, self.format
                ));
            }
            self.pending.extend(frame.samples);
            while self.pending.len() > self.pending_capacity {
                self.pending.pop_front();
                dropped = true;
            }
        }
        if self.pending.is_empty() {
            return Ok(dropped);
        }

        unsafe {
            let padding = self
                .client
                .GetCurrentPadding()
                .map_err(|error| error.to_string())?;
            let available_frames = self.buffer_frames.saturating_sub(padding);
            let pending_frames = self.pending.len() / self.format.channels as usize;
            let write_frames = available_frames.min(pending_frames as u32);
            if write_frames == 0 {
                return Ok(dropped);
            }
            let sample_count = write_frames as usize * self.format.channels as usize;
            let destination = self
                .render
                .GetBuffer(write_frames)
                .map_err(|error| error.to_string())? as *mut f32;
            for index in 0..sample_count {
                destination
                    .add(index)
                    .write(self.pending.pop_front().unwrap_or(0.0));
            }
            self.render
                .ReleaseBuffer(write_frames, 0)
                .map_err(|error| error.to_string())?;
        }
        Ok(dropped)
    }

    pub(super) fn stop(&mut self) {
        unsafe {
            let _ = self.client.Stop();
        }
        self.pending.clear();
        self.paused = true;
    }

    pub(super) fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        if self.paused == paused {
            return Ok(());
        }
        self.pending.clear();
        unsafe {
            if paused {
                self.client.Stop()
            } else {
                self.client.Start()
            }
            .map_err(|error| error.to_string())
        }?;
        self.paused = paused;
        Ok(())
    }
}

impl Drop for AudioRenderer {
    fn drop(&mut self) {
        self.stop();
    }
}

fn device_id(device: &::windows::Win32::Media::Audio::IMMDevice) -> Result<String, String> {
    unsafe {
        let id = device.GetId().map_err(|error| error.to_string())?;
        let result = id.to_string().map_err(|error| error.to_string());
        CoTaskMemFree(Some(id.0.cast()));
        result
    }
}
