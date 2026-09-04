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
const INITIAL_PREROLL_MS: u32 = 80;
const MIN_PREROLL_MS: u32 = 60;
const MAX_PREROLL_MS: u32 = 200;
const PREROLL_ADJUSTMENT_MS: u32 = 5;
const PREROLL_STABLE_INTERVAL: Duration = Duration::from_secs(20);
const MAX_PENDING_AUDIO_MS: usize = 200;

struct AdaptivePreroll {
    target_ms: u32,
    stable_since: Instant,
    underrun_active: bool,
}

impl AdaptivePreroll {
    fn new(now: Instant) -> Self {
        Self {
            target_ms: INITIAL_PREROLL_MS,
            stable_since: now,
            underrun_active: false,
        }
    }

    fn target_frames(&self, sample_rate: u32) -> usize {
        sample_rate as usize * self.target_ms as usize / 1_000
    }

    fn on_started(&mut self, now: Instant) {
        self.stable_since = now;
        self.underrun_active = false;
    }

    fn on_underrun(&mut self, now: Instant) -> bool {
        if self.underrun_active {
            return false;
        }
        self.underrun_active = true;
        let previous = self.target_ms;
        self.target_ms = self
            .target_ms
            .saturating_add(PREROLL_ADJUSTMENT_MS)
            .min(MAX_PREROLL_MS);
        self.stable_since = now;
        self.target_ms != previous
    }

    fn on_buffered(&mut self) {
        self.underrun_active = false;
    }

    fn observe_stable(&mut self, now: Instant) -> bool {
        if self.target_ms == MIN_PREROLL_MS
            || now.saturating_duration_since(self.stable_since) < PREROLL_STABLE_INTERVAL
        {
            return false;
        }
        self.target_ms = self
            .target_ms
            .saturating_sub(PREROLL_ADJUSTMENT_MS)
            .max(MIN_PREROLL_MS);
        self.stable_since = now;
        true
    }
}

pub(super) struct AudioRenderer {
    enumerator: IMMDeviceEnumerator,
    render: IAudioRenderClient,
    client: IAudioClient,
    buffer_frames: u32,
    format: AudioFormat,
    pending: VecDeque<f32>,
    pending_capacity: usize,
    paused: bool,
    started: bool,
    preroll: AdaptivePreroll,
    endpoint_tracker: DefaultEndpointTracker,
    next_endpoint_check: Instant,
}

impl AudioRenderer {
    pub(super) fn probe() -> Result<(), String> {
        let mut renderer = Self::new(AudioFormat {
            sample_rate: 48_000,
            channels: 2,
        })?;
        unsafe {
            renderer.client.Start().map_err(|error| error.to_string())?;
        }
        renderer.started = true;
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
            let now = Instant::now();
            let pending_frames = (format.sample_rate as usize * MAX_PENDING_AUDIO_MS / 1_000)
                .max(buffer_frames as usize * 4);
            Ok(Self {
                enumerator,
                render,
                client,
                buffer_frames,
                format,
                pending: VecDeque::new(),
                pending_capacity: pending_frames * format.channels as usize,
                paused: false,
                started: false,
                preroll: AdaptivePreroll::new(now),
                endpoint_tracker: DefaultEndpointTracker::new(endpoint_id),
                next_endpoint_check: now + ENDPOINT_CHECK_INTERVAL,
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
        if self.paused {
            return Ok(dropped);
        }

        unsafe {
            let padding = self
                .client
                .GetCurrentPadding()
                .map_err(|error| error.to_string())?;
            if self.started && padding == 0 {
                if self.preroll.on_underrun(Instant::now()) {
                    eprintln!(
                        "WASAPI underrun: increasing audio pre-roll to {}ms",
                        self.preroll.target_ms
                    );
                }
                // Shared-mode WASAPI already renders silence while starved.
                // Keep its clock running and refill it below instead of
                // Stop+Reset, which turns a short network gap into a click and
                // another full pre-roll delay.
            } else if padding != 0 {
                self.preroll.on_buffered();
            }

            let pending_frames = self.pending.len() / self.format.channels as usize;
            if !self.started && pending_frames < self.preroll.target_frames(self.format.sample_rate)
            {
                return Ok(dropped);
            }

            let available_frames = self.buffer_frames.saturating_sub(padding);
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
            if !self.started {
                self.client.Start().map_err(|error| error.to_string())?;
                self.started = true;
                self.preroll.on_started(Instant::now());
            } else if self.preroll.observe_stable(Instant::now()) {
                eprintln!(
                    "WASAPI playback stable: reducing audio pre-roll to {}ms",
                    self.preroll.target_ms
                );
            }
        }
        Ok(dropped)
    }

    pub(super) fn stop(&mut self) {
        unsafe {
            if self.started {
                let _ = self.client.Stop();
            }
        }
        self.pending.clear();
        self.started = false;
        self.paused = true;
    }

    pub(super) fn set_paused(&mut self, paused: bool) -> Result<(), String> {
        if self.paused == paused {
            return Ok(());
        }
        self.pending.clear();
        unsafe {
            if paused {
                if self.started {
                    self.client.Stop().map_err(|error| error.to_string())?;
                }
                self.client.Reset().map_err(|error| error.to_string())?;
            }
        }
        self.started = false;
        if !paused {
            // Playback restarts only after a short pre-roll. Starting an empty
            // shared-mode endpoint is the source of the repeated startup and
            // post-focus audio glitches this path used to produce.
            self.preroll.on_started(Instant::now());
        }
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

#[cfg(test)]
mod tests {
    use super::{
        AdaptivePreroll, INITIAL_PREROLL_MS, MAX_PREROLL_MS, MIN_PREROLL_MS, PREROLL_ADJUSTMENT_MS,
        PREROLL_STABLE_INTERVAL,
    };
    use std::time::{Duration, Instant};

    #[test]
    fn preroll_increases_on_underrun_and_is_bounded() {
        let now = Instant::now();
        let mut preroll = AdaptivePreroll::new(now);
        assert_eq!(preroll.target_frames(48_000), 3_840);
        assert_eq!(preroll.target_ms, INITIAL_PREROLL_MS);

        for step in 1..=32 {
            preroll.on_underrun(now + Duration::from_millis(step));
            preroll.on_buffered();
        }
        assert_eq!(preroll.target_ms, MAX_PREROLL_MS);
    }

    #[test]
    fn one_underrun_episode_only_adjusts_the_target_once() {
        let now = Instant::now();
        let mut preroll = AdaptivePreroll::new(now);

        assert!(preroll.on_underrun(now));
        assert!(!preroll.on_underrun(now + Duration::from_millis(1)));
        assert_eq!(
            preroll.target_ms,
            INITIAL_PREROLL_MS + PREROLL_ADJUSTMENT_MS
        );

        preroll.on_buffered();
        assert!(preroll.on_underrun(now + Duration::from_millis(2)));
    }

    #[test]
    fn stable_playback_returns_to_the_low_latency_target_gradually() {
        let now = Instant::now();
        let mut preroll = AdaptivePreroll::new(now);
        preroll.target_ms = MIN_PREROLL_MS + PREROLL_ADJUSTMENT_MS * 2;

        assert!(!preroll.observe_stable(now + PREROLL_STABLE_INTERVAL / 2));
        assert!(preroll.observe_stable(now + PREROLL_STABLE_INTERVAL));
        assert_eq!(preroll.target_ms, MIN_PREROLL_MS + PREROLL_ADJUSTMENT_MS);
        assert!(preroll.observe_stable(now + PREROLL_STABLE_INTERVAL * 2));
        assert_eq!(preroll.target_ms, MIN_PREROLL_MS);
    }
}
