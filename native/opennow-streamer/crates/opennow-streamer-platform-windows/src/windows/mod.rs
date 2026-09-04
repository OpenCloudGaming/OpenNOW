mod audio;
mod decoder;
mod embedded;
mod graphics;

pub use embedded::{
    AdoptedD3d11Context, D3d11Frame, D3d11FrameProducer, D3d11FrameSubmitter, D3d11RecordedFrame,
    D3d11TextureFormat,
};

use std::collections::VecDeque;
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use ::windows::Win32::Foundation::FreeLibrary;
use ::windows::Win32::Media::MediaFoundation::{MF_VERSION, MFSTARTUP_LITE, MFShutdown, MFStartup};
use ::windows::Win32::Media::{TIMERR_NOERROR, timeBeginPeriod, timeEndPeriod};
use ::windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize};
use ::windows::Win32::System::LibraryLoader::{LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use ::windows::Win32::System::Threading::{
    GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_ABOVE_NORMAL,
};
use ::windows::core::w;

use crate::{
    ADAPTIVE_VIDEO_QUEUE_CAPACITY, BackendConfig, BackendError, BackendEvent, CapabilityProbe,
    Control, LifecycleState, Shared, Subsystem, VideoCodec, WindowsDecoderMode, WindowsGraphicsApi,
};

use self::audio::AudioRenderer;
use self::decoder::{DecodedVideoFrame, Decoder};
use self::graphics::Graphics;

const RECOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const AUDIO_RECOVERY_RETRY_INTERVAL: Duration = Duration::from_millis(250);
const AUDIO_RECOVERY_LOG_INTERVAL: Duration = Duration::from_secs(5);
const PRESENTATION_SPIN_THRESHOLD: Duration = Duration::from_micros(300);
const ARRIVAL_HISTORY_LENGTH: usize = 120;
const QUEUE_HISTORY_LENGTH: usize = 3;
const RENDER_HISTORY_LENGTH: usize = 60;
const PACING_OUTLIER: Duration = Duration::from_millis(75);
const PACING_DIAGNOSTIC_INTERVAL: Duration = Duration::from_secs(5);
const MAX_CATCH_UP_RATE: f64 = 0.10;

/// Spaces decoded frames at the negotiated stream cadence without depending on
/// GFN's RTP timestamps. H.265/AV1 can assign one timestamp to a small group of
/// frames, so presenting immediately on decode makes a 120 fps stream arrive at
/// the display as visible bursts. Slower, game-driven streams remain dynamic:
/// when a frame arrives after its deadline it is presented immediately and the
/// clock is rebased instead of trying to catch up.
struct PresentationClock {
    nominal_interval: Duration,
    arrival_history: VecDeque<Duration>,
    queue_history: VecDeque<usize>,
    render_history: VecDeque<Duration>,
    last_arrival: Option<Instant>,
    last_presented: Option<Instant>,
    next_deadline: Option<Instant>,
    queue_integral: f64,
    last_diagnostics: Instant,
}

impl PresentationClock {
    fn new(frame_duration_100ns: i64) -> Self {
        let nominal_interval = frame_interval(frame_duration_100ns);
        Self {
            nominal_interval,
            arrival_history: VecDeque::with_capacity(ARRIVAL_HISTORY_LENGTH),
            queue_history: VecDeque::with_capacity(QUEUE_HISTORY_LENGTH),
            render_history: VecDeque::with_capacity(RENDER_HISTORY_LENGTH),
            last_arrival: None,
            last_presented: None,
            next_deadline: None,
            queue_integral: 0.0,
            last_diagnostics: Instant::now(),
        }
    }

    fn reset(&mut self, frame_duration_100ns: i64) {
        self.nominal_interval = frame_interval(frame_duration_100ns);
        self.arrival_history.clear();
        self.queue_history.clear();
        self.render_history.clear();
        self.last_arrival = None;
        self.last_presented = None;
        self.next_deadline = None;
        self.queue_integral = 0.0;
        self.last_diagnostics = Instant::now();
    }

    fn observe_decoded_frame(&mut self, now: Instant) {
        if let Some(previous) = self.last_arrival.replace(now) {
            let interval = now.saturating_duration_since(previous);
            if interval <= PACING_OUTLIER {
                push_bounded(&mut self.arrival_history, interval, ARRIVAL_HISTORY_LENGTH);
            } else {
                // A loading screen or recovery gap must not pull the active
                // stream cadence down for the next 120 frames.
                self.arrival_history.clear();
                self.next_deadline = None;
            }
        }
    }

    fn is_due(&self, now: Instant) -> bool {
        self.next_deadline.is_none_or(|deadline| now >= deadline)
    }

    fn mark_presented(&mut self, now: Instant, queued_frames: usize) {
        if let Some(previous) = self.last_presented.replace(now) {
            push_bounded(
                &mut self.render_history,
                now.saturating_duration_since(previous),
                RENDER_HISTORY_LENGTH,
            );
        }
        push_bounded(&mut self.queue_history, queued_frames, QUEUE_HISTORY_LENGTH);
        let queue_average = average_usize(&self.queue_history);
        self.queue_integral = (self.queue_integral * 0.9 + queue_average).clamp(0.0, 12.0);
        let interval = self.controlled_interval(queue_average);
        let next = self
            .next_deadline
            .map_or(now + interval, |deadline| deadline + interval);
        // Preserve the presentation phase while decoded frames are queued. A
        // D3D11 hardware decoder commonly releases AV1/H.265 frames in small
        // bursts; rebasing every slightly-late deadline to `now + interval`
        // makes that harmless lateness permanent and eventually forces the
        // low-latency queue to discard frames. Keeping the old phase lets the
        // worker use the next DXGI-ready slot to catch up. When the queue is
        // empty, rebase so a genuinely dynamic/slow game stream is never
        // chased with synthetic catch-up frames.
        self.next_deadline = Some(if queued_frames == 0 && next <= now {
            now + interval
        } else {
            next
        });

        if now.duration_since(self.last_diagnostics) >= PACING_DIAGNOSTIC_INTERVAL {
            let observed = average_duration(&self.render_history).unwrap_or(Duration::ZERO);
            let jitter = percentile_absolute_deviation(&self.render_history, observed, 0.99);
            eprintln!(
                "Adaptive presentation pacing: target={:.3}ms observed={:.3}ms p99Jitter={:.3}ms queue={queue_average:.2} arrivalSamples={}",
                interval.as_secs_f64() * 1_000.0,
                observed.as_secs_f64() * 1_000.0,
                jitter.as_secs_f64() * 1_000.0,
                self.arrival_history.len(),
            );
            self.last_diagnostics = now;
        }
    }

    fn time_until_due(&self, now: Instant) -> Duration {
        self.next_deadline.map_or(Duration::ZERO, |deadline| {
            deadline.saturating_duration_since(now)
        })
    }

    fn controlled_interval(&self, queue_average: f64) -> Duration {
        let estimated = average_duration(&self.arrival_history)
            .unwrap_or(self.nominal_interval)
            .clamp(
                self.nominal_interval,
                self.nominal_interval.saturating_mul(2),
            );
        // Queue feedback is intentionally conservative. P drains a burst now;
        // I prevents a small persistent backlog, while the negotiated cadence
        // remains the hard upper-FPS limit.
        let correction = (queue_average * 0.10 + self.queue_integral * 0.004).clamp(0.0, 0.30);
        let minimum = if queue_average > 0.0 {
            self.nominal_interval.as_secs_f64() * (1.0 - MAX_CATCH_UP_RATE)
        } else {
            self.nominal_interval.as_secs_f64()
        };
        Duration::from_secs_f64((estimated.as_secs_f64() / (1.0 + correction)).max(minimum))
    }
}

fn frame_interval(frame_duration_100ns: i64) -> Duration {
    Duration::from_nanos(u64::try_from(frame_duration_100ns.max(1)).unwrap_or(1) * 100)
}

fn push_bounded<T>(values: &mut VecDeque<T>, value: T, capacity: usize) {
    if values.len() == capacity {
        values.pop_front();
    }
    values.push_back(value);
}

fn average_duration(values: &VecDeque<Duration>) -> Option<Duration> {
    if values.is_empty() {
        return None;
    }
    let total_ns = values.iter().map(Duration::as_nanos).sum::<u128>();
    Some(Duration::from_nanos(
        u64::try_from(total_ns / values.len() as u128).unwrap_or(u64::MAX),
    ))
}

fn average_usize(values: &VecDeque<usize>) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<usize>() as f64 / values.len() as f64
    }
}

fn percentile_absolute_deviation(
    values: &VecDeque<Duration>,
    center: Duration,
    percentile: f64,
) -> Duration {
    if values.is_empty() {
        return Duration::ZERO;
    }
    let center_ns = center.as_nanos();
    let mut deviations = values
        .iter()
        .map(|value| value.as_nanos().abs_diff(center_ns))
        .collect::<Vec<_>>();
    deviations.sort_unstable();
    let index = ((deviations.len() - 1) as f64 * percentile).round() as usize;
    Duration::from_nanos(u64::try_from(deviations[index]).unwrap_or(u64::MAX))
}

/// Keeps the native media loop out of Windows' coarse default timer cadence.
/// The official GFN renderer uses an accurate-sleep pacing path and elevated
/// RTP/media threads; without this, a nominal 1 ms idle sleep can occasionally
/// become a visible multi-frame stall.
struct LowLatencyThreadGuard {
    timer_resolution_active: bool,
}

impl LowLatencyThreadGuard {
    fn enter() -> Self {
        let timer_resolution_active = unsafe { timeBeginPeriod(1) == TIMERR_NOERROR };
        unsafe {
            let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_ABOVE_NORMAL);
        }
        Self {
            timer_resolution_active,
        }
    }
}

impl Drop for LowLatencyThreadGuard {
    fn drop(&mut self) {
        if self.timer_resolution_active {
            unsafe {
                let _ = timeEndPeriod(1);
            }
        }
    }
}

struct MediaRuntime;

fn ensure_media_foundation_available() -> Result<(), String> {
    unsafe {
        let module = LoadLibraryExW(w!("mfplat.dll"), None, LOAD_LIBRARY_SEARCH_SYSTEM32)
            .map_err(|error| format!("Media Foundation is unavailable: {error}"))?;
        FreeLibrary(module).map_err(|error| format!("FreeLibrary(mfplat.dll): {error}"))
    }
}

impl MediaRuntime {
    fn initialize() -> Result<Self, BackendError> {
        ensure_media_foundation_available().map_err(BackendError::Startup)?;
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(|error| BackendError::Startup(format!("CoInitializeEx: {error}")))?;
            if let Err(error) = MFStartup(MF_VERSION, MFSTARTUP_LITE) {
                CoUninitialize();
                return Err(BackendError::Startup(format!("MFStartup: {error}")));
            }
        }
        Ok(Self)
    }
}

impl Drop for MediaRuntime {
    fn drop(&mut self) {
        unsafe {
            let _ = MFShutdown();
            CoUninitialize();
        }
    }
}

pub(super) fn probe(api: WindowsGraphicsApi) -> CapabilityProbe {
    let _runtime = match MediaRuntime::initialize() {
        Ok(runtime) => runtime,
        Err(error) => {
            return CapabilityProbe {
                available: false,
                h264_hardware_decode: false,
                h265_hardware_decode: false,
                av1_hardware_decode: false,
                h264_software_decode: false,
                h265_software_decode: false,
                av1_software_decode: false,
                d3d11_presentation: false,
                wasapi_render: false,
                reason: Some(error.to_string()),
            };
        }
    };

    let graphics = Graphics::probe(api);
    let h264_decoder = graphics
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|graphics| {
            Decoder::probe(graphics, VideoCodec::H264, WindowsDecoderMode::Hardware)
                .map_err(|error| error.to_string())
        });
    let h265_decoder = graphics
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|graphics| {
            Decoder::probe(graphics, VideoCodec::H265, WindowsDecoderMode::Hardware)
                .map_err(|error| error.to_string())
        });
    let av1_decoder = graphics
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|graphics| {
            Decoder::probe(graphics, VideoCodec::Av1, WindowsDecoderMode::Hardware)
                .map_err(|error| error.to_string())
        });
    let h264_software_decoder = graphics
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|graphics| {
            Decoder::probe(graphics, VideoCodec::H264, WindowsDecoderMode::Software)
                .map_err(|error| error.to_string())
        });
    let h265_software_decoder = graphics
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|graphics| {
            Decoder::probe(graphics, VideoCodec::H265, WindowsDecoderMode::Software)
                .map_err(|error| error.to_string())
        });
    let av1_software_decoder = graphics
        .as_ref()
        .map_err(Clone::clone)
        .and_then(|graphics| {
            Decoder::probe(graphics, VideoCodec::Av1, WindowsDecoderMode::Software)
                .map_err(|error| error.to_string())
        });
    let audio = AudioRenderer::probe().map_err(|error| error.to_string());
    let h264_hardware_decode = h264_decoder.is_ok();
    let h265_hardware_decode = h265_decoder.is_ok();
    let av1_hardware_decode = av1_decoder.is_ok();
    let h264_software_decode = h264_software_decoder.is_ok();
    let h265_software_decode = h265_software_decoder.is_ok();
    let av1_software_decode = av1_software_decoder.is_ok();
    let d3d11_presentation = graphics.is_ok();
    let wasapi_render = audio.is_ok();
    let mut probe = CapabilityProbe {
        available: false,
        h264_hardware_decode,
        h265_hardware_decode,
        av1_hardware_decode,
        h264_software_decode,
        h265_software_decode,
        av1_software_decode,
        d3d11_presentation,
        wasapi_render,
        reason: None,
    };
    probe.available = probe.bundled_backend_available();
    if !probe.available {
        probe.reason = Some(
            [
                graphics.err().map(|error| format!("{api:?}: {error}")),
                h264_decoder.err().map(|error| format!("H.264: {error}")),
                audio.err().map(|error| format!("WASAPI: {error}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join("; "),
        );
    }
    probe
}

pub(super) fn spawn(
    api: WindowsGraphicsApi,
    decoder_mode: WindowsDecoderMode,
    config: BackendConfig,
    shared: Arc<Shared>,
    controls: mpsc::Receiver<Control>,
) -> Result<JoinHandle<()>, BackendError> {
    let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
    let worker_shared = Arc::clone(&shared);
    let worker = thread::Builder::new()
        .name("opennow-windows-media".to_owned())
        .spawn(move || {
            let runtime = match Worker::new(api, decoder_mode, config, worker_shared, controls) {
                Ok(worker) => {
                    let _ = ready_sender.send(Ok(()));
                    worker
                }
                Err(error) => {
                    let _ = ready_sender.send(Err(error));
                    return;
                }
            };
            runtime.run();
        })
        .map_err(|error| BackendError::Startup(format!("failed to spawn media thread: {error}")))?;

    match ready_receiver.recv() {
        Ok(Ok(())) => Ok(worker),
        Ok(Err(error)) => {
            let _ = worker.join();
            Err(error)
        }
        Err(_) => {
            let _ = worker.join();
            Err(BackendError::Startup(
                "media thread exited during initialization".to_owned(),
            ))
        }
    }
}

struct Worker {
    api: WindowsGraphicsApi,
    decoder_mode: WindowsDecoderMode,
    config: BackendConfig,
    shared: Arc<Shared>,
    controls: mpsc::Receiver<Control>,
    graphics: Graphics,
    decoder: Decoder,
    decoded_video: VecDeque<DecodedVideoFrame>,
    presentation_clock: PresentationClock,
    first_frame_presented: bool,
    audio: Option<AudioRenderer>,
    audio_recovery: Option<AudioRecovery>,
    _runtime: MediaRuntime,
}

struct AudioRecovery {
    reason: String,
    next_attempt: Instant,
    next_log: Instant,
}

impl Worker {
    fn new(
        api: WindowsGraphicsApi,
        decoder_mode: WindowsDecoderMode,
        config: BackendConfig,
        shared: Arc<Shared>,
        controls: mpsc::Receiver<Control>,
    ) -> Result<Self, BackendError> {
        let runtime = MediaRuntime::initialize()?;
        let graphics = Graphics::new(api, config.surface, config.video)
            .map_err(|error| BackendError::Startup(format!("{api:?} presentation: {error}")))?;
        let decoder = Decoder::new(&graphics, config.video, decoder_mode).map_err(|error| {
            BackendError::Startup(format!(
                "Media Foundation {} {} decoder: {error}",
                match decoder_mode {
                    WindowsDecoderMode::Hardware => "hardware",
                    WindowsDecoderMode::Software => "software",
                },
                config.video.codec.label()
            ))
        })?;
        let audio = AudioRenderer::new(config.audio)
            .map_err(|error| BackendError::Startup(format!("WASAPI: {error}")))?;
        let presentation_clock = PresentationClock::new(config.video.frame_duration_100ns());
        shared.set_state(LifecycleState::Running);
        Ok(Self {
            api,
            decoder_mode,
            config,
            shared,
            controls,
            graphics,
            decoder,
            decoded_video: VecDeque::with_capacity(ADAPTIVE_VIDEO_QUEUE_CAPACITY),
            presentation_clock,
            first_frame_presented: false,
            audio: Some(audio),
            audio_recovery: None,
            _runtime: runtime,
        })
    }

    fn run(mut self) {
        let _low_latency_thread = LowLatencyThreadGuard::enter();
        let mut stopping = false;
        while !stopping {
            let mut did_work = false;
            self.graphics.pump_window_messages();
            while let Ok(control) = self.controls.try_recv() {
                did_work = true;
                match control {
                    Control::Stop => {
                        stopping = true;
                        break;
                    }
                    Control::ReconfigureVideo(format) => {
                        self.config.video = format;
                        self.shared.video.clear();
                        if let Err(error) = self.rebuild_video(
                            Subsystem::VideoDecode,
                            error_label("video reconfiguration"),
                        ) {
                            self.fail(error);
                            return;
                        }
                    }
                    Control::ReconfigureAudio(format) => {
                        self.config.audio = format;
                        self.begin_audio_recovery(error_label("audio reconfiguration"));
                    }
                    Control::SetSurface(surface) => {
                        self.config.surface = surface;
                        self.decoded_video.clear();
                        self.presentation_clock
                            .reset(self.config.video.frame_duration_100ns());
                        self.first_frame_presented = false;
                        if let Err(error) = self.graphics.set_surface(surface, self.config.video) {
                            if let Err(error) = self.recover_video(
                                Subsystem::VideoPresentation,
                                format!("surface reconfiguration failed: {error}"),
                            ) {
                                self.fail(error);
                                return;
                            }
                        } else {
                            self.shared.set_state(LifecycleState::Running);
                        }
                    }
                    Control::SetPaused(paused) => {
                        self.decoded_video.clear();
                        self.presentation_clock
                            .reset(self.config.video.frame_duration_100ns());
                        self.first_frame_presented = false;
                        let audio_error = self
                            .audio
                            .as_mut()
                            .and_then(|audio| audio.set_paused(paused).err());
                        if let Some(error) = audio_error {
                            self.begin_audio_recovery(format!(
                                "audio {} failed: {error}",
                                if paused { "pause" } else { "resume" }
                            ));
                        }
                        if !paused {
                            if let Err(error) = self
                                .rebuild_video(Subsystem::VideoDecode, "video resume".to_owned())
                            {
                                self.fail(error);
                                return;
                            }
                        }
                    }
                }
            }
            if stopping {
                break;
            }
            let endpoint_result = self
                .audio
                .as_mut()
                .map(AudioRenderer::default_endpoint_changed);
            match endpoint_result {
                Some(Ok(true)) => {
                    self.begin_audio_recovery("default audio endpoint changed".to_owned());
                }
                Some(Ok(false)) | None => {}
                Some(Err(error)) => {
                    self.begin_audio_recovery(format!(
                        "default audio endpoint check failed: {error}"
                    ));
                }
            }
            self.poll_audio_recovery();
            if self
                .shared
                .paused
                .load(std::sync::atomic::Ordering::Acquire)
            {
                thread::sleep(Duration::from_millis(1));
                continue;
            }

            let produced = match self
                .decoder
                .poll_output(&mut self.decoded_video, &self.shared.events)
            {
                Ok(produced) => produced,
                Err(error) => {
                    if let Err(error) = self.rebuild_video(Subsystem::VideoDecode, error) {
                        self.fail(error);
                        return;
                    }
                    continue;
                }
            };
            did_work |= produced > 0;
            if produced > 0 {
                self.presentation_clock
                    .observe_decoded_frame(Instant::now());
            }
            let decoded_format = self.decoder.format();
            if decoded_format != self.config.video {
                if let Err(error) = self.graphics.reconfigure_video(decoded_format) {
                    if let Err(error) = self.recover_video(Subsystem::VideoPresentation, error) {
                        self.fail(error);
                        return;
                    }
                    continue;
                }
                self.config.video = decoded_format;
                self.presentation_clock
                    .reset(decoded_format.frame_duration_100ns());
                *self
                    .shared
                    .video_format
                    .lock()
                    .unwrap_or_else(|error| error.into_inner()) = decoded_format;
            }
            while self.decoder.wants_input() {
                if let Some(frame) = self.shared.video.try_pop() {
                    did_work = true;
                    if frame.reset_decoder
                        && let Err(error) = self.reset_decoder_for_keyframe()
                    {
                        if let Err(error) = self.rebuild_video(Subsystem::VideoDecode, error) {
                            self.fail(error);
                            return;
                        }
                        // rebuild_video deliberately requests another clean
                        // keyframe; do not feed this one to a decoder whose
                        // recovery path failed midway.
                        continue;
                    }
                    if let Err(error) = self.decoder.submit(frame) {
                        if let Err(error) = self.rebuild_video(Subsystem::VideoDecode, error) {
                            self.fail(error);
                            return;
                        }
                        continue;
                    }
                } else {
                    break;
                }
            }
            // DXGI's frame-latency handle behaves like an auto-reset event.
            // Checking it without a frame ready would consume the signal and
            // leave the swap chain waiting forever for a Present that cannot
            // happen. Only consume readiness when presentation can follow.
            let presentation_now = Instant::now();
            if !self.decoded_video.is_empty() && self.presentation_clock.is_due(presentation_now) {
                match self.graphics.is_present_ready() {
                    Ok(true) => {
                        // Keep every decoded frame while the bounded queue has
                        // capacity. GFN's dynamic stream durations describe
                        // game output cadence, not whether an older decoded
                        // surface is stale; comparing them discarded normal
                        // AV1/H.265 decoder bursts. The phase-preserving clock
                        // drains bursts, while process_output's seven-surface
                        // cap still bounds latency during a real overload.
                        let frame = self.decoded_video.pop_front();
                        if let Some(frame) = frame {
                            did_work = true;
                            if let Err(error) = self.graphics.present(
                                &frame.texture,
                                frame.subresource,
                                frame.timestamp_100ns,
                                frame.duration_100ns,
                            ) {
                                if let Err(error) =
                                    self.recover_video(Subsystem::VideoPresentation, error)
                                {
                                    self.fail(error);
                                    return;
                                }
                                continue;
                            }
                            self.shared
                                .presented_frames
                                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                            self.presentation_clock
                                .mark_presented(Instant::now(), self.decoded_video.len());
                            if !self.first_frame_presented && self.graphics.is_visible() {
                                self.first_frame_presented = true;
                                let _ = self.shared.events.push(BackendEvent::FirstFramePresented);
                            }
                        }
                    }
                    Ok(false) => {}
                    Err(error) => {
                        if let Err(error) = self.recover_video(Subsystem::VideoPresentation, error)
                        {
                            self.fail(error);
                            return;
                        }
                        continue;
                    }
                }
            }
            let audio_result = self
                .audio
                .as_mut()
                .map(|audio| audio.render(&self.shared.audio));
            match audio_result {
                Some(Ok(true)) => {
                    did_work = true;
                    let _ = self
                        .shared
                        .events
                        .push(BackendEvent::QueueOverflow(Subsystem::Audio));
                }
                Some(Ok(false)) => {}
                Some(Err(error)) => {
                    self.begin_audio_recovery(error);
                }
                None => {
                    // Audio remains real-time while an endpoint is unavailable.
                    // Discard stale PCM instead of allowing it to backpressure video.
                    self.shared.audio.clear();
                }
            }
            let presentation_wait = if self.decoded_video.is_empty() {
                Duration::ZERO
            } else {
                self.presentation_clock.time_until_due(Instant::now())
            };
            if !presentation_wait.is_zero() && presentation_wait <= PRESENTATION_SPIN_THRESHOLD {
                // Avoid handing the final fraction of an 8.33 ms interval
                // back to the Windows scheduler. The bounded 300 us precision
                // phase costs less than four percent of one core at 120 Hz.
                while !self
                    .presentation_clock
                    .time_until_due(Instant::now())
                    .is_zero()
                {
                    std::hint::spin_loop();
                }
            } else if presentation_wait > PRESENTATION_SPIN_THRESHOLD {
                thread::sleep(
                    presentation_wait
                        .saturating_sub(PRESENTATION_SPIN_THRESHOLD)
                        .min(Duration::from_millis(1)),
                );
            } else if !did_work {
                thread::sleep(Duration::from_millis(1));
            } else {
                thread::yield_now();
            }
        }

        self.decoder.stop();
        if let Some(audio) = self.audio.as_mut() {
            audio.stop();
        }
    }

    fn recover_video(&mut self, subsystem: Subsystem, message: String) -> Result<(), BackendError> {
        let _ = self.shared.events.push(BackendEvent::DeviceLost {
            subsystem,
            message: message.clone(),
        });
        self.shared.set_state(LifecycleState::Recovering);
        self.shared.video.clear();
        self.decoded_video.clear();
        self.presentation_clock
            .reset(self.config.video.frame_duration_100ns());
        self.first_frame_presented = false;
        let start = Instant::now();
        loop {
            if self.shared.state() == LifecycleState::Stopping {
                return Ok(());
            }
            self.config.video = *self
                .shared
                .video_format
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            self.config.surface = *self
                .shared
                .surface
                .lock()
                .unwrap_or_else(|error| error.into_inner());
            match Graphics::new(self.api, self.config.surface, self.config.video).and_then(
                |graphics| {
                    Decoder::new(&graphics, self.config.video, self.decoder_mode)
                        .map(|decoder| (graphics, decoder))
                },
            ) {
                Ok((graphics, decoder)) => {
                    self.graphics = graphics;
                    self.decoder = decoder;
                    let _ = self
                        .shared
                        .events
                        .push(BackendEvent::DeviceRecovered(subsystem));
                    let _ = self.shared.events.push(BackendEvent::KeyFrameRequired);
                    self.shared.set_state(LifecycleState::Running);
                    return Ok(());
                }
                Err(error) if start.elapsed() < RECOVERY_TIMEOUT => {
                    let _ = error;
                    thread::sleep(Duration::from_millis(100));
                }
                Err(error) => {
                    return Err(BackendError::DeviceLost {
                        subsystem,
                        message: format!("{message}; recovery timed out: {error}"),
                    });
                }
            }
        }
    }

    fn rebuild_video(&mut self, subsystem: Subsystem, message: String) -> Result<(), BackendError> {
        self.decoder.stop();
        self.shared.video.clear();
        self.decoded_video.clear();
        self.presentation_clock
            .reset(self.config.video.frame_duration_100ns());
        self.first_frame_presented = false;
        match Decoder::new(&self.graphics, self.config.video, self.decoder_mode) {
            Ok(decoder) => {
                self.decoder = decoder;
                self.graphics
                    .reconfigure_video(self.config.video)
                    .map_err(|error| {
                        BackendError::Reconfigure(format!("D3D11 presenter: {error}"))
                    })?;
                let _ = self.shared.events.push(BackendEvent::KeyFrameRequired);
                self.shared.set_state(LifecycleState::Running);
                Ok(())
            }
            Err(error) => self.recover_video(subsystem, format!("{message}: {error}")),
        }
    }

    fn reset_decoder_for_keyframe(&mut self) -> Result<(), String> {
        self.decoder.stop();
        self.decoded_video.clear();
        self.presentation_clock
            .reset(self.config.video.frame_duration_100ns());
        self.first_frame_presented = false;
        self.decoder = Decoder::new(&self.graphics, self.config.video, self.decoder_mode)
            .map_err(|error| format!("recovery-keyframe decoder reset failed: {error}"))?;
        self.graphics
            .reconfigure_video(self.config.video)
            .map_err(|error| format!("recovery-keyframe presenter reset failed: {error}"))?;
        Ok(())
    }

    fn begin_audio_recovery(&mut self, message: String) {
        if self.audio_recovery.is_none() {
            let _ = self.shared.events.push(BackendEvent::DeviceLost {
                subsystem: Subsystem::Audio,
                message: message.clone(),
            });
        }
        self.shared.audio.clear();
        if let Some(mut audio) = self.audio.take() {
            audio.stop();
        }
        let now = Instant::now();
        self.audio_recovery = Some(AudioRecovery {
            reason: message,
            next_attempt: now,
            next_log: now + AUDIO_RECOVERY_LOG_INTERVAL,
        });
    }

    fn poll_audio_recovery(&mut self) {
        let Some(recovery) = self.audio_recovery.as_mut() else {
            return;
        };
        let now = Instant::now();
        if now < recovery.next_attempt {
            return;
        }
        self.config.audio = *self
            .shared
            .audio_format
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let paused = self
            .shared
            .paused
            .load(std::sync::atomic::Ordering::Acquire);
        match AudioRenderer::new(self.config.audio).and_then(|mut audio| {
            if paused {
                audio.set_paused(true)?;
            }
            Ok(audio)
        }) {
            Ok(audio) => {
                self.audio = Some(audio);
                self.audio_recovery = None;
                let _ = self
                    .shared
                    .events
                    .push(BackendEvent::DeviceRecovered(Subsystem::Audio));
            }
            Err(error) => {
                recovery.next_attempt = now + AUDIO_RECOVERY_RETRY_INTERVAL;
                if now >= recovery.next_log {
                    eprintln!(
                        "WASAPI endpoint still unavailable after {}: {error}; video remains active",
                        recovery.reason
                    );
                    recovery.next_log = now + AUDIO_RECOVERY_LOG_INTERVAL;
                }
            }
        }
    }

    fn fail(&self, error: BackendError) {
        self.shared.set_state(LifecycleState::Failed);
        let _ = self.shared.events.push(BackendEvent::Fatal(error));
        self.shared.video.close();
        self.shared.audio.close();
    }
}

fn error_label(value: &str) -> String {
    value.to_owned()
}

#[cfg(test)]
mod tests {
    use super::PresentationClock;
    use std::time::{Duration, Instant};

    #[test]
    fn presentation_clock_spaces_a_decoded_burst() {
        let start = Instant::now();
        let mut clock = PresentationClock::new(83_333);

        assert!(clock.is_due(start));
        clock.mark_presented(start, 3);
        assert!(!clock.is_due(start + Duration::from_millis(4)));
        assert!(clock.is_due(start + Duration::from_micros(8_334)));
    }

    #[test]
    fn presentation_clock_rebases_after_a_slow_frame() {
        let start = Instant::now();
        let mut clock = PresentationClock::new(83_333);
        clock.mark_presented(start, 0);

        let slow_arrival = start + Duration::from_millis(17);
        assert!(clock.is_due(slow_arrival));
        clock.mark_presented(slow_arrival, 0);
        assert!(!clock.is_due(slow_arrival + Duration::from_millis(4)));
        assert!(clock.is_due(slow_arrival + Duration::from_micros(8_334)));
    }

    #[test]
    fn presentation_clock_preserves_phase_to_drain_a_decoder_burst() {
        let start = Instant::now();
        let mut clock = PresentationClock::new(83_333);
        clock.mark_presented(start, 3);

        let late = start + Duration::from_millis(18);
        assert!(clock.is_due(late));
        clock.mark_presented(late, 2);

        // The next buffered frame is already due because the clock retained
        // the original cadence instead of adding another full frame period to
        // the late presentation time.
        assert!(clock.is_due(late));
    }

    #[test]
    fn presentation_clock_learns_a_dynamic_sixty_fps_source() {
        let start = Instant::now();
        let mut clock = PresentationClock::new(83_333);
        for frame in 0..120 {
            clock.observe_decoded_frame(start + Duration::from_micros(frame * 16_667));
        }

        let interval = clock.controlled_interval(0.0);
        assert!(interval >= Duration::from_micros(16_660));
        assert!(interval <= Duration::from_micros(16_670));
    }

    #[test]
    fn presentation_clock_averages_grouped_frame_arrivals() {
        let start = Instant::now();
        let mut clock = PresentationClock::new(83_333);
        for group in 0..30 {
            let group_start = start + Duration::from_millis(group * 33);
            for frame in 0..4 {
                clock.observe_decoded_frame(group_start + Duration::from_micros(frame * 20));
            }
        }

        let interval = clock.controlled_interval(0.0);
        assert!(interval >= Duration::from_micros(8_200));
        assert!(interval <= Duration::from_micros(8_500));
    }

    #[test]
    fn presentation_clock_uses_a_bounded_catch_up_rate_for_backlog() {
        let clock = PresentationClock::new(83_333);
        let interval = clock.controlled_interval(3.0);

        assert!(interval < Duration::from_nanos(8_333_300));
        assert!(interval >= Duration::from_nanos(7_499_970));
    }
}
