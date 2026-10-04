use std::sync::mpsc::{self, Receiver, SyncSender};

use opennow_streamer_protocol::AudioOutputDevice;
use opennow_streamer_transport::webrtc::{self as transport, TransportControl, TransportEvent};

use super::*;

mod input;
mod sdp;
mod signaling;

const MAX_SDP_BYTES: usize = 256 * 1024;
const MAX_PENDING_CANDIDATES: usize = 64;
const STARTUP_TIMEOUT: Duration = Duration::from_secs(20);
const VIDEO_TIMEOUT: Duration = Duration::from_secs(8);
const RECOVERY_GRACE: Duration = Duration::from_secs(4);
const SIGNALING_POLL_INTERVAL: Duration = Duration::from_millis(1);

#[derive(Debug)]
struct Failure {
    code: &'static str,
    message: String,
}

impl Failure {
    fn signaling(message: impl Into<String>) -> Self {
        Self {
            code: "webrtc-signaling-failed",
            message: message.into(),
        }
    }

    fn transport(error: transport::TransportError) -> Self {
        Self {
            code: error.code(),
            message: error.to_string(),
        }
    }
}

pub(super) struct OwnedSession {
    cancelled: Arc<AtomicBool>,
    input_ready: Arc<AtomicBool>,
    commands: SyncSender<SessionCommand>,
    worker: Option<JoinHandle<()>>,
}

enum SessionCommand {
    AntiAfk,
    Pause(bool),
}

impl OwnedSession {
    pub(super) fn stop(mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    pub(super) fn anti_afk(&self) -> Result<(), String> {
        if !self.input_ready.load(Ordering::Acquire) || self.cancelled.load(Ordering::Acquire) {
            return Err("WebRTC input handshake is not ready".to_owned());
        }
        self.commands
            .try_send(SessionCommand::AntiAfk)
            .map_err(|_| "WebRTC input command queue unavailable".to_owned())
    }

    pub(super) fn set_paused(&self, paused: bool) -> Result<(), String> {
        self.commands
            .try_send(SessionCommand::Pause(paused))
            .map_err(|_| "WebRTC input pause queue unavailable".to_owned())
    }
}

impl Drop for OwnedSession {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Engine {
    pub(super) fn start_webrtc(
        &mut self,
        id: String,
        context: SessionContext,
        audio_device: AudioOutputDevice,
    ) -> Result<Vec<Value>, Value> {
        let stream = media_stream_config(&context);
        let accepted = context.session.extra.get("negotiatedStreamProfile");
        let codec = accepted
            .and_then(|profile| profile.get("codec"))
            .and_then(Value::as_str)
            .or_else(|| context.settings.get("codec").and_then(Value::as_str))
            .unwrap_or("H264");
        let color = accepted
            .and_then(|profile| profile.get("colorQuality"))
            .and_then(Value::as_str)
            .or_else(|| context.settings.get("colorQuality").and_then(Value::as_str))
            .unwrap_or("8bit_420");
        if !codec.eq_ignore_ascii_case("H264")
            || color != "8bit_420"
            || stream.codec != MediaVideoCodec::H264
            || stream.color_quality != MediaColorQuality::EightBit420
            || stream.hdr
        {
            return Err(error(
                Some(&id),
                "webrtc-profile-unsupported",
                "WebRTC compatibility requires an accepted H264 8-bit 4:2:0 SDR profile",
            ));
        }
        signaling::sign_in_url(&context.session, "validation")
            .map_err(|failure| error(Some(&id), failure.code, failure.message))?;
        if context.nvst_video.is_some() {
            return Err(error(
                Some(&id),
                "webrtc-context-invalid",
                "A WebRTC context cannot contain an NVST handoff",
            ));
        }
        if let Some(session) = self.webrtc_session.take() {
            session.stop();
        }
        if let Some(transport) = self.nvst_transport.take() {
            transport.stop();
        }
        if let Some(transport) = self.nvst_mjolnir_transport.take() {
            transport.stop();
        }
        if let Some(mut rtsp) = self.nvst_rtsp.take() {
            rtsp.shutdown();
        }
        self.reserved_nvst_bundle = None;
        self.nvst_hole_punch_socket = None;
        self.stop_media_resources();
        if let Some(runtime) = self.media_runtime.clone() {
            let (feedback_sender, feedback_receiver) = mpsc::channel();
            let session = runtime
                .start_with_audio_device(
                    feedback_sender,
                    stream,
                    context
                        .settings
                        .get("nativeVideoBackend")
                        .and_then(Value::as_str)
                        .unwrap_or("auto"),
                    audio_device,
                )
                .map_err(|message| error(Some(&id), "media-output-unavailable", message))?;
            session.control().start_replay(
                ReplayBufferConfig::from_settings(&context.settings),
                Arc::clone(&self.replay_budget),
            );
            let sink = session.sink();
            let (consumer, receiver) = mpsc::sync_channel(ENCODED_MEDIA_QUEUE_CAPACITY);
            let output = self.events.clone();
            let worker = match thread::Builder::new()
                .name("opennow-media-consumer".to_owned())
                .spawn(move || consume_encoded_media(&output, receiver, sink))
            {
                Ok(worker) => worker,
                Err(spawn_error) => {
                    session.stop();
                    return Err(error(
                        Some(&id),
                        "media-worker-failed",
                        spawn_error.to_string(),
                    ));
                }
            };
            self.media_consumer = Some(consumer);
            self.media_session = Some(session);
            self.media_worker = Some(worker);
            self.media_feedback = Some(feedback_receiver);
        }
        let Some(consumer) = self.media_consumer.clone() else {
            self.stop_media_resources();
            return Err(error(
                Some(&id),
                "media-consumer-unavailable",
                "WebRTC requires an in-process encoded media consumer",
            ));
        };
        let generation = {
            let mut lifecycle = lock_lifecycle(&self.lifecycle);
            lifecycle.generation = lifecycle.generation.wrapping_add(1);
            lifecycle.context = Some(context.clone());
            lifecycle.state = State::Connected;
            lifecycle.generation
        };
        let replay_enabled = self.media_session.is_some()
            && ReplayBufferConfig::from_settings(&context.settings).enabled;
        let cancelled = Arc::new(AtomicBool::new(false));
        let input_ready = Arc::new(AtomicBool::new(false));
        let (command_sender, commands) = mpsc::sync_channel(4);
        let resources = Worker {
            context,
            stream,
            start_id: id.clone(),
            lifecycle: self.lifecycle.clone(),
            generation,
            cancelled: cancelled.clone(),
            input_ready: input_ready.clone(),
            output: self.events.clone(),
            feedback: self.media_feedback.take(),
            captured_input: self
                .media_session
                .as_ref()
                .map(MediaSession::captured_input),
            media: self.media_session.as_ref().map(MediaSession::control),
            runtime: self.media_runtime.clone(),
            commands,
        };
        let worker = match thread::Builder::new()
            .name("opennow-webrtc-session".to_owned())
            .spawn(move || {
                let result = resources.run(consumer);
                if let Err(failure) = result {
                    resources.terminal(failure);
                }
                resources.input_ready.store(false, Ordering::Release);
                if let Some(queue) = &resources.captured_input {
                    queue.set_text_ready(generation, false);
                    queue.clear();
                }
                if let Some(media) = &resources.media {
                    media.stop();
                }
            }) {
            Ok(worker) => worker,
            Err(spawn_error) => {
                self.stop("WebRTC worker startup failed");
                return Err(error(
                    Some(&id),
                    "media-worker-failed",
                    spawn_error.to_string(),
                ));
            }
        };
        self.webrtc_session = Some(OwnedSession {
            cancelled,
            input_ready,
            commands: command_sender,
            worker: Some(worker),
        });
        let mut result = response(id, "ok");
        result["transport"] = json!("webrtc");
        result["replayEnabled"] = json!(replay_enabled);
        result["capabilities"] = json!({
            "supportsInput":true,"supportsAudioDecode":supports_audio_decode(),
            "supportsAudioOutput":supports_audio_output(),"supportsMicrophone":false
        });
        Ok(vec![result])
    }
}

struct Worker {
    context: SessionContext,
    stream: MediaStreamConfig,
    start_id: String,
    lifecycle: Arc<Mutex<Lifecycle>>,
    generation: u64,
    cancelled: Arc<AtomicBool>,
    input_ready: Arc<AtomicBool>,
    output: EventSender,
    feedback: Option<Receiver<MediaFeedback>>,
    captured_input: Option<Arc<CapturedInputQueue>>,
    media: Option<MediaControl>,
    runtime: Option<MediaRuntime>,
    commands: Receiver<SessionCommand>,
}

impl Worker {
    fn active(&self) -> bool {
        !self.cancelled.load(Ordering::Acquire)
            && lock_lifecycle(&self.lifecycle).generation == self.generation
    }

    fn emit(&self, name: &str, mut payload: Value) {
        let lifecycle = lock_lifecycle(&self.lifecycle);
        if lifecycle.generation != self.generation
            || lifecycle.state == State::Idle
            || self.cancelled.load(Ordering::Acquire)
        {
            return;
        }
        payload["startId"] = json!(self.start_id);
        payload["transport"] = json!("webrtc");
        let _ = self.output.send(event(name, payload));
    }

    fn terminal(&self, failure: Failure) {
        let mut lifecycle = lock_lifecycle(&self.lifecycle);
        if lifecycle.generation != self.generation
            || lifecycle.state == State::Idle
            || self.cancelled.load(Ordering::Acquire)
        {
            return;
        }
        lifecycle.context = None;
        lifecycle.state = State::Idle;
        let termination = json!({"source":"webrtc-transport","code":failure.code,"resumable":null});
        let _ = self.output.send(event("error", json!({"code":failure.code,"message":failure.message,"termination":termination,"startId":self.start_id,"transport":"webrtc"})));
        let _ = self.output.send(event("status", json!({"status":"stopped","message":failure.message,"termination":termination,"startId":self.start_id,"transport":"webrtc"})));
    }

    fn run(&self, consumer: MediaConsumer) -> Result<(), Failure> {
        let published_baseline = self
            .runtime
            .as_ref()
            .and_then(MediaRuntime::published_video_frames);
        if let Some(queue) = &self.captured_input {
            queue.set_text_ready(self.generation, false);
            queue.clear();
        }
        self.emit(
            "status",
            json!({"status":"connecting","message":"Connecting WebRTC compatibility transport"}),
        );
        self.emit("microphone-state", json!({"state":"unavailable","enabled":false,"message":"WebRTC compatibility does not support microphone audio"}));
        let mut signaling = signaling::Signaling::connect(&self.context.session, &self.cancelled)?;
        if !self.active() {
            return Ok(());
        }
        let peer_info = signaling
            .protocol
            .peer_info(self.stream.width, self.stream.height);
        signaling.send(peer_info)?;
        let deadline = Instant::now() + STARTUP_TIMEOUT;
        let mut candidates = Vec::new();
        let offer = loop {
            if !self.active() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(Failure::signaling("Timed out waiting for the WebRTC offer"));
            }
            match signaling.poll()? {
                Some(signaling::Incoming::Offer(offer)) => break offer,
                Some(signaling::Incoming::Candidate(candidate)) => {
                    if candidates.len() >= MAX_PENDING_CANDIDATES {
                        return Err(Failure::signaling("Too many queued ICE candidates"));
                    }
                    candidates.push(candidate);
                }
                Some(signaling::Incoming::Closed) => {
                    return Err(Failure::signaling("Signaling peer closed before the offer"));
                }
                None => thread::sleep(SIGNALING_POLL_INTERVAL),
            }
        };
        let threshold = sdp::partial_reliability(&offer)?;
        let mut transport_session = self.context.session.clone();
        let host = transport_session
            .media_connection_info
            .as_mut()
            .map(|endpoint| &mut endpoint.ip)
            .unwrap_or(&mut transport_session.server_ip);
        if host.parse::<std::net::IpAddr>().is_err() {
            let addresses = signaling::resolve(host.clone(), 443, &self.cancelled)?;
            let address = addresses
                .iter()
                .find(|address| address.is_ipv4())
                .or_else(|| addresses.first())
                .ok_or_else(|| Failure::signaling("Media endpoint DNS returned no address"))?;
            *host = address.ip().to_string();
        }
        if !self.active() {
            return Ok(());
        }
        let (sender, receiver) = mpsc::sync_channel(64);
        let negotiated = transport::negotiate(
            &offer,
            &transport_session,
            transport::NegotiatedVideoCodec::H264,
            threshold,
            sender,
            consumer,
        )
        .map_err(Failure::transport)?;
        if !self.active() {
            return Ok(());
        }
        let nvst = sdp::nvst_answer(&offer, &negotiated.answer_sdp, self.stream)?;
        let video_mid = negotiated.video_mid.to_string();
        signaling.send_peer(json!({"type":"answer","sdp":negotiated.answer_sdp,"nvstSdp":nvst}))?;
        signaling.send_peer(
            serde_json::to_value(&negotiated.local_candidate)
                .map_err(|_| Failure::signaling("Could not serialize local ICE"))?,
        )?;
        let session = negotiated.session;
        for candidate in candidates {
            session
                .add_remote_candidate(&candidate)
                .map_err(Failure::transport)?;
        }
        let control = session.control();
        let mut progress = Progress::new(Instant::now(), published_baseline);
        let mut input_origin = Instant::now();
        let mut input_state = input::InputState::default();
        let mut drops = QueueDropReports::new();
        let result = (|| -> Result<(), Failure> {
            while self.active() {
                let iteration_started = Instant::now();
                for incoming in receiver.try_iter().take(64) {
                    match incoming {
                    TransportEvent::Connected => self.emit("log", json!({"level":"info","message":"WebRTC transport connected; waiting for decoded media"})),
                    TransportEvent::Disconnected(message) => return Err(Failure { code:"webrtc-disconnected", message }),
                        TransportEvent::InputReady(version) => {
                            if !self.input_ready.swap(true, Ordering::AcqRel) {
                                input_origin = Instant::now();
                            }
                        self.emit("input-ready", json!({"protocolVersion":version}));
                    }
                    TransportEvent::InputUnavailable(message) => return Err(Failure { code:"webrtc-input-unavailable", message }),
                    TransportEvent::Log(message) => self.emit("log", json!({"level":"info","message":message})),
                }
                }
                if let Some(feedback) = &self.feedback {
                    for feedback in feedback.try_iter().take(128) {
                        self.feedback(feedback, &control, &video_mid, &mut progress, &mut drops)?;
                    }
                }
                progress.observe_published(
                    self.runtime
                        .as_ref()
                        .and_then(MediaRuntime::published_video_frames),
                    Instant::now(),
                );
                if progress.poll(Instant::now())? {
                    control
                        .request_keyframe(video_mid.clone())
                        .map_err(Failure::transport)?;
                }
                {
                    let lifecycle = lock_lifecycle(&self.lifecycle);
                    if lifecycle.generation != self.generation {
                        break;
                    }
                    drops.flush(&self.output, Instant::now(), false);
                }
                if let Some(queue) = &self.captured_input {
                    if !self.input_ready.load(Ordering::Acquire) {
                        queue.clear();
                    } else if queue.take_overflowed() {
                        return Err(Failure {
                        code: "webrtc-input-overflow",
                        message:
                            "Captured input queue overflowed; stopping to prevent stuck controls"
                                .to_owned(),
                    });
                    }
                    for _ in 0..128 {
                        let Some(sample) = queue.take_sample() else {
                            break;
                        };
                        if matches!(sample.input, CapturedInput::Guide) {
                            self.emit("overlay-request", json!({"source":"gamepad"}));
                            continue;
                        }
                        if matches!(sample.input, CapturedInput::Screenshot) {
                            self.emit("screenshot-request", json!({"source":"keyboard"}));
                            continue;
                        }
                        if matches!(sample.input, CapturedInput::RecordingToggle) {
                            self.emit("recording-toggle-request", json!({"source":"keyboard"}));
                            continue;
                        }
                        let action = match &sample.input {
                            CapturedInput::Shortcut(action) => Some(*action),
                            _ => None,
                        };
                        if let Some(action) = action {
                            if self.active() {
                                forward_shortcut_action(
                                    &self.output,
                                    self.runtime.as_ref(),
                                    action,
                                );
                            }
                            continue;
                        }
                        if matches!(sample.input, CapturedInput::Text(_)) {
                            return Err(Failure {
                                code: "webrtc-text-unavailable",
                                message: "WebRTC text submission is unavailable".to_owned(),
                            });
                        }
                        if self.input_ready.load(Ordering::Acquire) && !input_state.paused {
                            let timestamp = sample
                                .captured_at
                                .saturating_duration_since(input_origin)
                                .as_micros()
                                .min(u128::from(u64::MAX))
                                as u64;
                            input_state.sent(&sample.input)?;
                            control
                                .send_input(captured_input_packet(sample.input, timestamp), false)
                                .map_err(Failure::transport)?;
                        }
                    }
                }
                for command in self.commands.try_iter().take(4) {
                    let inputs = match command {
                        SessionCommand::Pause(paused) => {
                            if let Some(queue) = &self.captured_input {
                                queue.clear();
                            }
                            input_state.set_paused(paused)
                        }
                        SessionCommand::AntiAfk if !input_state.paused => [true, false]
                            .map(|pressed| CapturedInput::Key {
                                virtual_key: 0x7c,
                                modifiers: 0,
                                pressed,
                            })
                            .to_vec(),
                        SessionCommand::AntiAfk => Vec::new(),
                    };
                    if !self.input_ready.load(Ordering::Acquire) {
                        continue;
                    }
                    let timestamp =
                        input_origin.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
                    input::send_and_flush(&control, inputs, timestamp)
                        .map_err(Failure::transport)?;
                }
                match signaling.poll()? {
                    Some(signaling::Incoming::Candidate(candidate)) => session
                        .add_remote_candidate(&candidate)
                        .map_err(Failure::transport)?,
                    Some(signaling::Incoming::Offer(_)) => {
                        return Err(Failure::signaling(
                            "WebRTC renegotiation requires a fresh session start",
                        ));
                    }
                    Some(signaling::Incoming::Closed) => {
                        return Err(Failure::signaling("Signaling peer closed"));
                    }
                    None => {}
                }
                thread::sleep(SIGNALING_POLL_INTERVAL.saturating_sub(iteration_started.elapsed()));
            }
            Ok(())
        })();
        let neutral = input_state.set_paused(true);
        if !neutral.is_empty() && self.input_ready.load(Ordering::Acquire) {
            let timestamp = input_origin.elapsed().as_micros().min(u128::from(u64::MAX)) as u64;
            let _ = input::send_and_flush(&control, neutral, timestamp);
        }
        session.stop();
        result
    }

    fn feedback(
        &self,
        feedback: MediaFeedback,
        control: &TransportControl,
        video_mid: &str,
        progress: &mut Progress,
        drops: &mut QueueDropReports,
    ) -> Result<(), Failure> {
        progress.observe_feedback(&feedback, Instant::now());
        match feedback {
            MediaFeedback::VideoFrameAccepted { .. } => {}
            MediaFeedback::PlaybackStarted { backend } => {
                self.emit("status", json!({"event":"first-frame","backend":backend,"status":"streaming","message":"WebRTC presented the first video frame"}));
            }
            MediaFeedback::DecodeTimings(timings) => {
                self.emit("telemetry", json!({"decodeTimings":decode_timings_event(Some(timings)),"transport":"webrtc"}));
            }
            MediaFeedback::RequestKeyframe { mid, .. } => control.request_keyframe(mid).map_err(Failure::transport)?,
            MediaFeedback::DecoderError { message, .. } => {
                self.emit("log", json!({"level":"warn","message":message}));
                control.request_keyframe(video_mid).map_err(Failure::transport)?;
            }
            MediaFeedback::OutputError { message } => return Err(Failure { code:"media-output-error", message }),
            MediaFeedback::AudioDecoderError { message, consecutive } => self.emit("log", json!({"level":"warn","message":message,"consecutive":consecutive})),
            MediaFeedback::AudioUnavailable { backend, reason, rejected } => self.emit("log", json!({"level":"warn","backend":backend,"message":reason,"rejected":rejected})),
            MediaFeedback::BackendFallback { from, to, reason } => self.emit("log", json!({"event":"backend-fallback","fromBackend":from,"toBackend":to,"reason":reason})),
            MediaFeedback::ColorFormatChanged { requested, actual } => self.emit("log", json!({"level":"warn","message":format!("Color format changed from {requested:?} to {actual:?}")})),
            MediaFeedback::DeviceLost { subsystem, recovered, message } => {
                self.emit("log", json!({"event":"device-state","subsystem":subsystem,"recovered":recovered,"message":message}));
                if recovered { control.request_keyframe(video_mid).map_err(Failure::transport)?; }
            }
            MediaFeedback::QueueDropped { media, count } => drops.record(media, count),
        }
        Ok(())
    }
}

struct Progress {
    started: Instant,
    presented: bool,
    last_decoded: Option<Instant>,
    requested_keyframe: Option<Instant>,
    published: Option<u64>,
    timings_observed: bool,
}

impl Progress {
    fn new(started: Instant, published: Option<u64>) -> Self {
        Self {
            started,
            presented: false,
            last_decoded: None,
            requested_keyframe: None,
            published,
            timings_observed: false,
        }
    }
    fn observe_published(&mut self, published: Option<u64>, now: Instant) {
        if self
            .published
            .zip(published)
            .is_some_and(|(previous, current)| previous != current)
        {
            self.decoded(now);
        }
        self.published = published;
    }
    fn observe_feedback(&mut self, feedback: &MediaFeedback, now: Instant) {
        match feedback {
            MediaFeedback::PlaybackStarted { .. } => self.presented(now),
            MediaFeedback::DecodeTimings(timings) => {
                self.timings_observed = true;
                if let Some(at) = timings.last_output_at.filter(|at| *at >= self.started) {
                    self.decoded(at);
                }
            }
            _ => {}
        }
    }
    fn presented(&mut self, at: Instant) {
        self.presented = true;
        self.decoded(at);
    }
    fn decoded(&mut self, at: Instant) {
        if self.last_decoded.is_none_or(|previous| at > previous) {
            self.last_decoded = Some(at);
            self.requested_keyframe = None;
        }
    }
    fn poll(&mut self, now: Instant) -> Result<bool, Failure> {
        if !self.presented && now.saturating_duration_since(self.started) >= STARTUP_TIMEOUT {
            return Err(Failure {
                code: "webrtc-video-startup-timeout",
                message: "WebRTC connected but no video frame was presented".to_owned(),
            });
        }
        if (self.published.is_some() || self.timings_observed)
            && let Some(last) = self.last_decoded
            && now.saturating_duration_since(last) >= VIDEO_TIMEOUT
        {
            match self.requested_keyframe {
                None => {
                    self.requested_keyframe = Some(now);
                    return Ok(true);
                }
                Some(requested) if now.saturating_duration_since(requested) >= RECOVERY_GRACE => {
                    return Err(Failure {
                        code: "webrtc-video-stalled",
                        message: "WebRTC video stopped decoding after a keyframe request"
                            .to_owned(),
                    });
                }
                Some(_) => {}
            }
        }
        Ok(false)
    }
}

#[cfg(test)]
mod tests;
