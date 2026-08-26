use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::audio::{AudioSink, OpusDecoder, open_audio_fallback, open_audio_sink};
use crate::queue::{BoundedQueue, QueuePop, QueuePush};
use crate::video::{VideoDecoder, open_v4l2};
use crate::{
    AudioBackend, AudioConfig, AudioPacket, DecodedVideoFrame, EncodedVideoFrame, Error, Result,
    StreamFormat, Subsystem, VideoCodec,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderBackend {
    Vulkan,
    Cuda,
    VaApi,
    V4l2,
    Ffmpeg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoderPreference {
    Automatic,
    VulkanOnly,
    CudaOnly,
    VaApiThenV4l2,
    V4l2ThenVaApi,
    VaApiOnly,
    V4l2Only,
    SoftwareOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleState {
    Starting,
    Running,
    Reconfiguring,
    Stopping,
    Stopped,
    Failed,
}

impl LifecycleState {
    fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (
                Self::Starting,
                Self::Running | Self::Failed | Self::Stopping
            ) | (
                Self::Running,
                Self::Reconfiguring | Self::Stopping | Self::Failed
            ) | (
                Self::Reconfiguring,
                Self::Running | Self::Stopping | Self::Failed
            ) | (Self::Stopping, Self::Stopped | Self::Failed)
                | (Self::Failed, Self::Stopping)
        ) || self == next
    }
}

#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub codec: VideoCodec,
    pub stream_format: StreamFormat,
    pub decoder_preference: DecoderPreference,
    pub v4l2_device: Option<PathBuf>,
    pub encoded_queue_depth: usize,
    pub decoded_queue_depth: usize,
    pub audio: Option<AudioConfig>,
}

impl SessionConfig {
    pub fn new(stream_format: StreamFormat) -> Self {
        Self {
            codec: VideoCodec::H264,
            stream_format,
            decoder_preference: DecoderPreference::Automatic,
            v4l2_device: None,
            // Hardware decoders can briefly stop consuming while the driver
            // retires a large frame or reallocates surfaces. Four frames is
            // too small for a 120 Hz stream and turns a harmless burst into a
            // decoder reset. Keep this bounded, but absorb roughly two frame
            // batches before initiating keyframe recovery.
            encoded_queue_depth: 16,
            decoded_queue_depth: 3,
            audio: Some(AudioConfig::default()),
        }
    }

    fn validate(&self) -> Result<()> {
        self.stream_format.validate()?;
        if self.encoded_queue_depth == 0 || self.encoded_queue_depth > 64 {
            return Err(Error::InvalidFormat(
                "encoded video queue depth must be between 1 and 64".to_owned(),
            ));
        }
        if self.decoded_queue_depth == 0 || self.decoded_queue_depth > 16 {
            return Err(Error::InvalidFormat(
                "decoded video queue depth must be between 1 and 16".to_owned(),
            ));
        }
        if let Some(audio) = &self.audio {
            audio.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Queued,
    DroppedOldest,
    Paused,
}

#[derive(Debug)]
pub enum BackendEvent {
    StateChanged(LifecycleState),
    DecoderSelected(DecoderBackend),
    DecoderChanged {
        from: DecoderBackend,
        to: DecoderBackend,
        reason: String,
    },
    AudioSelected(AudioBackend),
    FormatChanged(StreamFormat),
    NeedKeyframe,
    QueueOverflow {
        media: &'static str,
    },
    DeviceLost {
        subsystem: Subsystem,
        reason: String,
    },
    Error(String),
}

enum VideoCommand {
    Decode {
        frame: EncodedVideoFrame,
        generation: u64,
        reset: bool,
    },
    Reconfigure {
        format: StreamFormat,
        generation: u64,
    },
}

type EventQueue = Arc<BoundedQueue<BackendEvent>>;

pub struct LinuxSession {
    config: SessionConfig,
    state: Arc<Mutex<LifecycleState>>,
    video_commands: Arc<BoundedQueue<VideoCommand>>,
    decoded_frames: Arc<BoundedQueue<DecodedVideoFrame>>,
    audio_packets: Option<Arc<BoundedQueue<AudioPacket>>>,
    events: EventQueue,
    video_generation: AtomicU64,
    video_needs_keyframe: AtomicBool,
    paused: AtomicBool,
    video_submit: Mutex<()>,
    video_worker: Option<JoinHandle<()>>,
    audio_worker: Option<JoinHandle<()>>,
}

impl LinuxSession {
    pub fn start(config: SessionConfig) -> Result<Self> {
        config.validate()?;
        let state = Arc::new(Mutex::new(LifecycleState::Starting));
        let video_commands = Arc::new(BoundedQueue::new(config.encoded_queue_depth));
        let decoded_frames = Arc::new(BoundedQueue::new(config.decoded_queue_depth));
        let events = Arc::new(BoundedQueue::new(64));
        let (startup_tx, startup_rx) = mpsc::sync_channel(1);
        let video_worker = {
            let config = config.clone();
            let state = Arc::clone(&state);
            let commands = Arc::clone(&video_commands);
            let decoded = Arc::clone(&decoded_frames);
            let events = Arc::clone(&events);
            thread::Builder::new()
                .name("opennow-linux-video".to_owned())
                .spawn(move || {
                    run_video_worker(config, state, commands, decoded, events, startup_tx)
                })
                .map_err(|error| Error::io(Subsystem::Session, error))?
        };
        match startup_rx.recv_timeout(Duration::from_secs(3)) {
            Ok(Ok(_)) => {}
            Ok(Err(error)) => {
                video_commands.close();
                let _ = video_worker.join();
                return Err(error);
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                video_commands.close();
                let _ = video_worker.join();
                return Err(Error::WorkerPanic("video startup"));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                video_commands.close();
                let _ = video_worker.join();
                return Err(Error::backend(
                    Subsystem::Session,
                    "video worker startup timed out and was cancelled",
                ));
            }
        }

        let (audio_packets, audio_worker) = if let Some(audio_config) = config.audio.clone() {
            let queue = Arc::new(BoundedQueue::new(audio_config.queue_depth));
            let (audio_start_tx, audio_start_rx) = mpsc::sync_channel(1);
            let worker = {
                let queue = Arc::clone(&queue);
                let events = Arc::clone(&events);
                let state = Arc::clone(&state);
                match thread::Builder::new()
                    .name("opennow-linux-audio".to_owned())
                    .spawn(move || {
                        run_audio_worker(audio_config, queue, state, events, audio_start_tx)
                    }) {
                    Ok(worker) => worker,
                    Err(error) => {
                        video_commands.close();
                        let _ = video_worker.join();
                        return Err(Error::io(Subsystem::Session, error));
                    }
                }
            };
            match audio_start_rx.recv_timeout(Duration::from_secs(3)) {
                Ok(Ok(_)) => (Some(queue), Some(worker)),
                Ok(Err(error)) => {
                    queue.close();
                    let _ = worker.join();
                    video_commands.close();
                    let _ = video_worker.join();
                    return Err(error);
                }
                Err(_) => {
                    queue.close();
                    let _ = worker.join();
                    video_commands.close();
                    let _ = video_worker.join();
                    return Err(Error::backend(
                        Subsystem::Session,
                        "audio worker startup timed out",
                    ));
                }
            }
        } else {
            (None, None)
        };

        Ok(Self {
            config,
            state,
            video_commands,
            decoded_frames,
            audio_packets,
            events,
            video_generation: AtomicU64::new(0),
            video_needs_keyframe: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            video_submit: Mutex::new(()),
            video_worker: Some(video_worker),
            audio_worker,
        })
    }

    pub fn state(&self) -> LifecycleState {
        *self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    pub fn submit_video(&self, frame: EncodedVideoFrame) -> Result<PushOutcome> {
        frame.validate()?;
        if self.state() != LifecycleState::Running {
            return Err(Error::NotRunning);
        }
        if self.paused.load(Ordering::Acquire) {
            return Ok(PushOutcome::Paused);
        }
        let _submission = self
            .video_submit
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        queue_video_command(
            &self.video_commands,
            &self.events,
            &self.video_generation,
            &self.video_needs_keyframe,
            frame,
        )
    }

    pub fn submit_audio(&self, packet: AudioPacket) -> Result<PushOutcome> {
        packet.validate()?;
        if self.state() != LifecycleState::Running {
            return Err(Error::NotRunning);
        }
        if self.paused.load(Ordering::Acquire) {
            return Ok(PushOutcome::Paused);
        }
        let queue = self.audio_packets.as_ref().ok_or_else(|| {
            Error::unavailable(Subsystem::Session, "audio is disabled for this session")
        })?;
        match queue.push_latest(packet) {
            QueuePush::Added => Ok(PushOutcome::Queued),
            QueuePush::DroppedOldest => {
                emit(&self.events, BackendEvent::QueueOverflow { media: "audio" });
                Ok(PushOutcome::DroppedOldest)
            }
            QueuePush::Full => unreachable!(),
            QueuePush::Closed => Err(Error::QueueClosed),
        }
    }

    pub fn reconfigure(&self, format: StreamFormat) -> Result<()> {
        format.validate()?;
        if self.state() != LifecycleState::Running {
            return Err(Error::NotRunning);
        }
        let _submission = self
            .video_submit
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        self.video_needs_keyframe.store(true, Ordering::Release);
        let generation = self.video_generation.fetch_add(1, Ordering::AcqRel) + 1;
        match self
            .video_commands
            .replace(VideoCommand::Reconfigure { format, generation })
        {
            QueuePush::Added | QueuePush::DroppedOldest => Ok(()),
            QueuePush::Full => unreachable!(),
            QueuePush::Closed => Err(Error::QueueClosed),
        }
    }

    pub fn set_paused(&self, paused: bool) -> Result<()> {
        if self.state() != LifecycleState::Running {
            return Err(Error::NotRunning);
        }
        let was_paused = self.paused.swap(paused, Ordering::AcqRel);
        if was_paused == paused {
            return Ok(());
        }
        self.video_commands.clear();
        self.decoded_frames.clear();
        if let Some(audio) = &self.audio_packets {
            audio.clear();
        }
        if !paused {
            self.reconfigure(self.config.stream_format)?;
        }
        Ok(())
    }

    pub fn try_recv_frame(&self) -> Option<DecodedVideoFrame> {
        self.decoded_frames.try_pop()
    }

    pub fn recv_frame_timeout(&self, timeout: Duration) -> Option<DecodedVideoFrame> {
        self.decoded_frames.pop_timeout(timeout)
    }

    pub fn try_recv_event(&self) -> Option<BackendEvent> {
        self.events.try_pop()
    }

    pub fn stop(&mut self) -> Result<()> {
        let current = self.state();
        if matches!(current, LifecycleState::Stopped) {
            return Ok(());
        }
        transition_state(&self.state, LifecycleState::Stopping, &self.events);
        self.video_commands.close();
        if let Some(queue) = &self.audio_packets {
            queue.close();
        }
        let mut failure = None;
        if let Some(worker) = self.video_worker.take() {
            if worker.join().is_err() {
                failure = Some(Error::WorkerPanic("video"));
            }
        }
        if let Some(worker) = self.audio_worker.take() {
            if worker.join().is_err() && failure.is_none() {
                failure = Some(Error::WorkerPanic("audio"));
            }
        }
        self.decoded_frames.close();
        if self.state() == LifecycleState::Failed {
            transition_state(&self.state, LifecycleState::Stopping, &self.events);
        }
        transition_state(&self.state, LifecycleState::Stopped, &self.events);
        failure.map_or(Ok(()), Err)
    }

    pub fn config(&self) -> &SessionConfig {
        &self.config
    }
}

impl Drop for LinuxSession {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn queue_video_command(
    commands: &BoundedQueue<VideoCommand>,
    events: &EventQueue,
    generation: &AtomicU64,
    needs_keyframe: &AtomicBool,
    frame: EncodedVideoFrame,
) -> Result<PushOutcome> {
    // Once an access unit has been dropped, later inter-frames are no longer
    // decodable. Do not let them fill the queue or evict the recovery IDR.
    // The first keyframe atomically replaces only decode work (never control
    // commands) and starts a fresh decoder generation.
    if needs_keyframe.load(Ordering::Acquire) {
        if !frame.keyframe {
            return Ok(PushOutcome::DroppedOldest);
        }
        let generation = generation.fetch_add(1, Ordering::AcqRel) + 1;
        return match commands.replace_where(
            VideoCommand::Decode {
                frame,
                generation,
                reset: true,
            },
            |queued| matches!(queued, VideoCommand::Decode { .. }),
        ) {
            QueuePush::Added => {
                needs_keyframe.store(false, Ordering::Release);
                Ok(PushOutcome::Queued)
            }
            QueuePush::DroppedOldest => {
                needs_keyframe.store(false, Ordering::Release);
                Ok(PushOutcome::DroppedOldest)
            }
            QueuePush::Full => Ok(PushOutcome::DroppedOldest),
            QueuePush::Closed => Err(Error::QueueClosed),
        };
    }

    let active_generation = generation.load(Ordering::Acquire);
    match commands.push(VideoCommand::Decode {
        frame: frame.clone(),
        generation: active_generation,
        reset: false,
    }) {
        QueuePush::Added => Ok(PushOutcome::Queued),
        QueuePush::Full => {
            emit(events, BackendEvent::QueueOverflow { media: "video" });
            needs_keyframe.store(true, Ordering::Release);

            // A keyframe that happens to arrive at the overflow boundary can
            // recover immediately. Non-keyframes are deliberately discarded;
            // preserving the already queued contiguous frames is preferable to
            // repeatedly reopening the decoder with an undecodable P-frame.
            if frame.keyframe {
                let recovery_generation = generation.fetch_add(1, Ordering::AcqRel) + 1;
                match commands.replace_where(
                    VideoCommand::Decode {
                        frame,
                        generation: recovery_generation,
                        reset: true,
                    },
                    |queued| matches!(queued, VideoCommand::Decode { .. }),
                ) {
                    QueuePush::Closed => return Err(Error::QueueClosed),
                    QueuePush::Added | QueuePush::DroppedOldest => {
                        needs_keyframe.store(false, Ordering::Release);
                    }
                    QueuePush::Full => {}
                }
            } else {
                emit(events, BackendEvent::NeedKeyframe);
            }
            Ok(PushOutcome::DroppedOldest)
        }
        QueuePush::DroppedOldest => unreachable!(),
        QueuePush::Closed => Err(Error::QueueClosed),
    }
}

fn run_video_worker(
    config: SessionConfig,
    state: Arc<Mutex<LifecycleState>>,
    commands: Arc<BoundedQueue<VideoCommand>>,
    decoded: Arc<BoundedQueue<DecodedVideoFrame>>,
    events: EventQueue,
    startup: mpsc::SyncSender<Result<DecoderBackend>>,
) {
    let (mut backend, mut decoder) = match open_preferred_decoder(&config, config.stream_format) {
        Ok(opened) => opened,
        Err(error) => {
            let _ = startup.send(Err(error));
            transition_state(&state, LifecycleState::Failed, &events);
            return;
        }
    };
    if startup.send(Ok(backend)).is_err() {
        return;
    }
    emit(&events, BackendEvent::DecoderSelected(backend));
    transition_state(&state, LifecycleState::Running, &events);
    // Never feed a fresh hardware decoder an inter-frame packet. In
    // particular, an AV1 stream can deliver its sequence header separately;
    // treating the following delta frame as startup input caused an avoidable
    // backend fallback and a large visible hitch.
    let mut need_keyframe = true;
    emit(&events, BackendEvent::NeedKeyframe);

    let mut active_format = config.stream_format;
    let mut active_generation = 0;
    loop {
        let command = match commands.wait_pop(Duration::from_millis(100)) {
            QueuePop::Item(command) => command,
            QueuePop::TimedOut => continue,
            QueuePop::Closed => break,
        };
        match command {
            VideoCommand::Reconfigure { format, generation } => {
                if generation < active_generation {
                    continue;
                }
                transition_state(&state, LifecycleState::Reconfiguring, &events);
                if let Ok(frames) = flush_decoder(&mut decoder) {
                    enqueue_frames(&decoded, &events, frames);
                }
                decoded.clear();
                match open_preferred_decoder(&config, format) {
                    Ok((new_backend, new_decoder)) => {
                        if new_backend != backend {
                            emit(
                                &events,
                                BackendEvent::DecoderChanged {
                                    from: backend,
                                    to: new_backend,
                                    reason: "stream format changed".to_owned(),
                                },
                            );
                        }
                        backend = new_backend;
                        decoder = new_decoder;
                        active_format = format;
                        active_generation = generation;
                        need_keyframe = true;
                        emit(&events, BackendEvent::FormatChanged(format));
                        emit(&events, BackendEvent::NeedKeyframe);
                        transition_state(&state, LifecycleState::Running, &events);
                    }
                    Err(error) => {
                        report_worker_error(&state, &events, error);
                        return;
                    }
                }
            }
            VideoCommand::Decode {
                frame,
                generation,
                reset,
            } => {
                if generation < active_generation {
                    continue;
                }
                if reset || generation > active_generation {
                    decoded.clear();
                    match open_preferred_decoder(&config, active_format) {
                        Ok((new_backend, new_decoder)) => {
                            if new_backend != backend {
                                emit(
                                    &events,
                                    BackendEvent::DecoderChanged {
                                        from: backend,
                                        to: new_backend,
                                        reason: "encoded queue discontinuity".to_owned(),
                                    },
                                );
                            }
                            backend = new_backend;
                            decoder = new_decoder;
                            active_generation = generation;
                        }
                        Err(error) => {
                            report_worker_error(&state, &events, error);
                            return;
                        }
                    }
                    need_keyframe = true;
                }
                if need_keyframe && !frame.keyframe {
                    continue;
                }
                match decode_frame(&mut decoder, &frame) {
                    Ok(frames) => {
                        need_keyframe = false;
                        if let Some(format) = decoder.take_format_change() {
                            active_format = format;
                            emit(&events, BackendEvent::FormatChanged(format));
                        }
                        enqueue_frames(&decoded, &events, frames);
                    }
                    Err(error) => match open_fallback_decoder(&config, active_format, backend) {
                        Ok((new_backend, mut new_decoder)) => {
                            let reason = error.to_string();
                            emit(
                                &events,
                                BackendEvent::DecoderChanged {
                                    from: backend,
                                    to: new_backend,
                                    reason,
                                },
                            );
                            backend = new_backend;
                            if frame.keyframe {
                                match decode_frame(&mut new_decoder, &frame) {
                                    Ok(frames) => {
                                        if let Some(format) = new_decoder.take_format_change() {
                                            active_format = format;
                                            emit(&events, BackendEvent::FormatChanged(format));
                                        }
                                        enqueue_frames(&decoded, &events, frames);
                                    }
                                    Err(fallback_error) => {
                                        report_worker_error(&state, &events, fallback_error);
                                        return;
                                    }
                                }
                                need_keyframe = false;
                            } else {
                                need_keyframe = true;
                                emit(&events, BackendEvent::NeedKeyframe);
                            }
                            decoder = new_decoder;
                        }
                        Err(_) => {
                            report_worker_error(&state, &events, error);
                            return;
                        }
                    },
                }
            }
        }
    }
    if let Ok(frames) = flush_decoder(&mut decoder) {
        enqueue_frames(&decoded, &events, frames);
    }
}

fn run_audio_worker(
    config: AudioConfig,
    packets: Arc<BoundedQueue<AudioPacket>>,
    state: Arc<Mutex<LifecycleState>>,
    events: EventQueue,
    startup: mpsc::SyncSender<Result<AudioBackend>>,
) {
    let mut opus = match OpusDecoder::open(&config) {
        Ok(decoder) => decoder,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };
    let mut sink: Box<dyn AudioSink + Send> = match open_audio_sink(&config) {
        Ok(sink) => sink,
        Err(error) => {
            let _ = startup.send(Err(error));
            return;
        }
    };
    let mut backend = sink.backend();
    let _ = startup.send(Ok(backend));
    emit(&events, BackendEvent::AudioSelected(backend));
    loop {
        let packet = match packets.wait_pop(Duration::from_millis(100)) {
            QueuePop::Item(packet) => packet,
            QueuePop::TimedOut => continue,
            QueuePop::Closed => break,
        };
        let pcm = match opus.decode(&packet) {
            Ok(pcm) => pcm,
            Err(error) => {
                packets.close();
                report_worker_error(&state, &events, error);
                return;
            }
        };
        let cancelled = || packets.is_closed();
        if let Err(error) = sink.write(pcm, &cancelled) {
            if packets.is_closed() {
                return;
            }
            match open_audio_fallback(&config, backend) {
                Ok(mut fallback) => {
                    if let Err(fallback_error) = fallback.write(pcm, &cancelled) {
                        if packets.is_closed() {
                            return;
                        }
                        packets.close();
                        report_worker_error(&state, &events, fallback_error);
                        return;
                    }
                    backend = fallback.backend();
                    sink = fallback;
                    emit(&events, BackendEvent::AudioSelected(backend));
                    continue;
                }
                Err(fallback_error) => {
                    emit(
                        &events,
                        BackendEvent::Error(format!("audio fallback failed: {fallback_error}")),
                    );
                }
            }
            packets.close();
            report_worker_error(&state, &events, error);
            return;
        }
    }
}

fn open_preferred_decoder(
    config: &SessionConfig,
    format: StreamFormat,
) -> Result<(DecoderBackend, Box<dyn VideoDecoder>)> {
    let order = decoder_order(config.decoder_preference);
    let mut failures = Vec::new();
    for backend in order {
        match open_decoder(config, format, *backend) {
            Ok(decoder) => return Ok((*backend, decoder)),
            Err(error) => failures.push(error.to_string()),
        }
    }
    Err(Error::unavailable(
        Subsystem::Session,
        format!(
            "no requested {} decoder opened: {}",
            config.codec.label(),
            failures.join("; ")
        ),
    ))
}

fn open_fallback_decoder(
    config: &SessionConfig,
    format: StreamFormat,
    current: DecoderBackend,
) -> Result<(DecoderBackend, Box<dyn VideoDecoder>)> {
    let order = decoder_order(config.decoder_preference);
    let Some(current_index) = order.iter().position(|backend| *backend == current) else {
        return Err(Error::unavailable(
            Subsystem::Session,
            "active decoder is outside the configured fallback order",
        ));
    };
    let mut failures = Vec::new();
    for backend in &order[current_index + 1..] {
        match open_decoder(config, format, *backend) {
            Ok(decoder) => return Ok((*backend, decoder)),
            Err(error) => failures.push(error.to_string()),
        }
    }
    Err(Error::unavailable(
        Subsystem::Session,
        format!("no fallback decoder opened: {}", failures.join("; ")),
    ))
}

fn decoder_order(preference: DecoderPreference) -> &'static [DecoderBackend] {
    match preference {
        DecoderPreference::Automatic => &[
            DecoderBackend::Vulkan,
            DecoderBackend::Cuda,
            DecoderBackend::VaApi,
            DecoderBackend::V4l2,
            DecoderBackend::Ffmpeg,
        ],
        DecoderPreference::VulkanOnly => &[DecoderBackend::Vulkan],
        DecoderPreference::CudaOnly => &[DecoderBackend::Cuda],
        DecoderPreference::VaApiThenV4l2 => &[
            DecoderBackend::VaApi,
            DecoderBackend::V4l2,
            DecoderBackend::Ffmpeg,
        ],
        DecoderPreference::V4l2ThenVaApi => &[
            DecoderBackend::V4l2,
            DecoderBackend::VaApi,
            DecoderBackend::Ffmpeg,
        ],
        DecoderPreference::VaApiOnly => &[DecoderBackend::VaApi],
        DecoderPreference::V4l2Only => &[DecoderBackend::V4l2],
        DecoderPreference::SoftwareOnly => &[DecoderBackend::Ffmpeg],
    }
}

fn open_decoder(
    config: &SessionConfig,
    format: StreamFormat,
    backend: DecoderBackend,
) -> Result<Box<dyn VideoDecoder>> {
    match backend {
        DecoderBackend::Vulkan => open_ffmpeg_decoder(config.codec, format, backend),
        DecoderBackend::Cuda => open_ffmpeg_decoder(config.codec, format, backend),
        DecoderBackend::Ffmpeg => open_ffmpeg_decoder(config.codec, format, backend),
        DecoderBackend::V4l2 if config.codec == VideoCodec::H264 => {
            open_v4l2(format, config.v4l2_device.clone())
        }
        DecoderBackend::V4l2 => Err(Error::unavailable(
            Subsystem::V4l2,
            format!(
                "{} decode is not implemented by the V4L2 backend",
                config.codec.label()
            ),
        )),
        DecoderBackend::VaApi => {
            if config.codec != VideoCodec::H264 {
                return Err(Error::unavailable(
                    Subsystem::VaApi,
                    format!(
                        "{} decode is not implemented by the native VA-API backend",
                        config.codec.label()
                    ),
                ));
            }
            #[cfg(feature = "vaapi")]
            {
                crate::video::open_vaapi(format)
            }
            #[cfg(not(feature = "vaapi"))]
            {
                let _ = format;
                Err(Error::unavailable(
                    Subsystem::VaApi,
                    "crate was built without the vaapi feature",
                ))
            }
        }
    }
}

fn open_ffmpeg_decoder(
    codec: VideoCodec,
    format: StreamFormat,
    backend: DecoderBackend,
) -> Result<Box<dyn VideoDecoder>> {
    #[cfg(feature = "ffmpeg")]
    {
        use crate::video::{FfmpegDecoder, FfmpegMode};

        let mode = match backend {
            DecoderBackend::Vulkan => FfmpegMode::Vulkan,
            DecoderBackend::Cuda => FfmpegMode::Cuda,
            DecoderBackend::Ffmpeg => FfmpegMode::Software,
            _ => unreachable!("non-FFmpeg backend passed to open_ffmpeg_decoder"),
        };
        FfmpegDecoder::open(codec, format, mode)
            .map(|decoder| Box::new(decoder) as Box<dyn VideoDecoder>)
    }
    #[cfg(not(feature = "ffmpeg"))]
    {
        let _ = (codec, format, backend);
        Err(Error::unavailable(
            Subsystem::Ffmpeg,
            "crate was built without the ffmpeg feature",
        ))
    }
}

fn enqueue_frames(
    queue: &BoundedQueue<DecodedVideoFrame>,
    events: &EventQueue,
    frames: Vec<DecodedVideoFrame>,
) {
    for frame in frames {
        if queue.push_latest(frame) == QueuePush::DroppedOldest {
            emit(
                events,
                BackendEvent::QueueOverflow {
                    media: "decoded-video",
                },
            );
        }
    }
}

fn decode_frame(
    decoder: &mut Box<dyn VideoDecoder>,
    frame: &EncodedVideoFrame,
) -> Result<Vec<DecodedVideoFrame>> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decoder.decode(frame))).unwrap_or_else(
        |panic| {
            Err(Error::backend(
                Subsystem::Session,
                format!("decoder panicked: {}", panic_message(panic)),
            ))
        },
    )
}

fn flush_decoder(decoder: &mut Box<dyn VideoDecoder>) -> Result<Vec<DecodedVideoFrame>> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| decoder.flush())).unwrap_or_else(
        |panic| {
            Err(Error::backend(
                Subsystem::Session,
                format!("decoder panicked while flushing: {}", panic_message(panic)),
            ))
        },
    )
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        (*message).to_owned()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_owned()
    }
}

fn transition_state(state: &Mutex<LifecycleState>, next: LifecycleState, events: &EventQueue) {
    let mut current = state.lock().unwrap_or_else(|poison| poison.into_inner());
    if current.can_transition_to(next) && *current != next {
        *current = next;
        emit(events, BackendEvent::StateChanged(next));
    }
}

fn report_worker_error(state: &Mutex<LifecycleState>, events: &EventQueue, error: Error) {
    match error {
        Error::DeviceLost { subsystem, reason } => {
            emit(events, BackendEvent::DeviceLost { subsystem, reason });
        }
        other => {
            emit(events, BackendEvent::Error(other.to_string()));
        }
    }
    transition_state(state, LifecycleState::Failed, events);
}

fn emit(events: &EventQueue, event: BackendEvent) {
    events.push_latest(event);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lifecycle_allows_reconfigure_and_orderly_stop() {
        assert!(LifecycleState::Starting.can_transition_to(LifecycleState::Running));
        assert!(LifecycleState::Running.can_transition_to(LifecycleState::Reconfiguring));
        assert!(LifecycleState::Reconfiguring.can_transition_to(LifecycleState::Running));
        assert!(LifecycleState::Running.can_transition_to(LifecycleState::Stopping));
        assert!(LifecycleState::Stopping.can_transition_to(LifecycleState::Stopped));
    }

    #[test]
    fn lifecycle_rejects_restart_after_stop_or_failure() {
        assert!(!LifecycleState::Stopped.can_transition_to(LifecycleState::Running));
        assert!(!LifecycleState::Failed.can_transition_to(LifecycleState::Running));
        assert!(!LifecycleState::Running.can_transition_to(LifecycleState::Starting));
        assert!(LifecycleState::Failed.can_transition_to(LifecycleState::Stopping));
    }

    #[test]
    fn stop_can_recover_from_a_worker_failure_during_shutdown() {
        assert!(LifecycleState::Stopping.can_transition_to(LifecycleState::Failed));
        assert!(LifecycleState::Failed.can_transition_to(LifecycleState::Stopping));
        assert!(LifecycleState::Stopping.can_transition_to(LifecycleState::Stopped));
    }

    #[test]
    fn overflow_preserves_recovery_keyframe_and_rejects_inter_frames_until_it_arrives() {
        let commands = BoundedQueue::new(2);
        let events = Arc::new(BoundedQueue::new(8));
        let generation = AtomicU64::new(0);
        let needs_keyframe = AtomicBool::new(false);
        let frame = |timestamp_us, keyframe| {
            EncodedVideoFrame::new(vec![timestamp_us as u8 + 1], timestamp_us, keyframe).unwrap()
        };

        assert_eq!(
            queue_video_command(
                &commands,
                &events,
                &generation,
                &needs_keyframe,
                frame(0, false),
            )
            .unwrap(),
            PushOutcome::Queued
        );
        assert_eq!(
            queue_video_command(
                &commands,
                &events,
                &generation,
                &needs_keyframe,
                frame(1, false),
            )
            .unwrap(),
            PushOutcome::Queued
        );
        assert_eq!(
            queue_video_command(
                &commands,
                &events,
                &generation,
                &needs_keyframe,
                frame(2, false),
            )
            .unwrap(),
            PushOutcome::DroppedOldest
        );
        assert!(needs_keyframe.load(Ordering::Acquire));

        // This P-frame is discarded instead of evicting queued recovery work.
        assert_eq!(
            queue_video_command(
                &commands,
                &events,
                &generation,
                &needs_keyframe,
                frame(3, false),
            )
            .unwrap(),
            PushOutcome::DroppedOldest
        );
        assert_eq!(commands.len(), 2);

        // The IDR atomically replaces stale decode work. Its following P-frame
        // is queued behind it with the same generation.
        assert_eq!(
            queue_video_command(
                &commands,
                &events,
                &generation,
                &needs_keyframe,
                frame(4, true),
            )
            .unwrap(),
            PushOutcome::DroppedOldest
        );
        assert!(!needs_keyframe.load(Ordering::Acquire));
        assert_eq!(
            queue_video_command(
                &commands,
                &events,
                &generation,
                &needs_keyframe,
                frame(5, false),
            )
            .unwrap(),
            PushOutcome::Queued
        );

        let QueuePop::Item(VideoCommand::Decode {
            frame,
            generation: keyframe_generation,
            reset,
        }) = commands.wait_pop(Duration::ZERO)
        else {
            panic!("expected queued recovery keyframe");
        };
        assert!(frame.keyframe);
        assert!(reset);
        let QueuePop::Item(VideoCommand::Decode {
            frame,
            generation: inter_generation,
            reset,
        }) = commands.wait_pop(Duration::ZERO)
        else {
            panic!("expected inter-frame after recovery keyframe");
        };
        assert!(!frame.keyframe);
        assert!(!reset);
        assert_eq!(inter_generation, keyframe_generation);
    }
}
