mod channel;
#[cfg(test)]
mod tests;
mod worker;

use super::*;
use opennow_media_protocol::lease::{
    HostBoundPreparedLease, LocalMediaPolicy, NativeMediaState, OfferRequest, PreparedMedia,
    StartRequest,
};
use opennow_media_protocol::wire::{
    AckKind, ControlMessage, FrameStage, InputEvent, VIDEO_TRACK_ID, WorkerBootstrap,
};
use opennow_plugin_api::media::{
    AcceptedMedia, AudioCodec, AudioFormat, Chroma, DynamicRange, InputCapabilities, Matrix,
    MediaLimits, NativeOffer, Primaries, Transfer, VideoEncoding, VideoSupport,
};
use opennow_plugin_api::provider::{List, OfferId, SecretBytes};
use opennow_plugin_package::verify;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicU64;
use std::time::{SystemTime, UNIX_EPOCH};
use worker::{WorkerEvent, WorkerSession};

pub(super) struct RetainedOffer {
    offer: NativeOffer,
    policy: LocalMediaPolicy,
}

pub(super) struct ActiveLease {
    pub(super) binding: Value,
    pub(super) start_id: String,
    published_baseline: Option<u64>,
}

pub(super) fn runtime_epoch() -> u64 {
    let mut bytes = [0; 8];
    if getrandom::fill(&mut bytes).is_err() {
        return 0;
    }
    (u64::from_le_bytes(bytes) & 0x001f_ffff_ffff_ffff).max(1)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl Engine {
    pub fn needs_lifecycle_poll(&self) -> bool {
        self.external_session.is_some()
    }

    pub fn poll_lifecycle(&mut self) {
        if self
            .external_session
            .as_ref()
            .is_some_and(ExternalSession::finished)
        {
            self.stop("The provider media worker has retired");
        }
    }
    pub(super) fn native_session_occupied(&self) -> bool {
        self.active_lease.is_some()
            || self.external_session.is_some()
            || self.media_session.is_some()
            || self.media_worker.is_some()
            || self.feedback_worker.is_some()
            || self.webrtc_session.is_some()
            || self.nvst_transport.is_some()
            || self.nvst_mjolnir_transport.is_some()
            || self.nvst_rtsp.is_some()
            || self.reserved_nvst_bundle.is_some()
            || lock_lifecycle(&self.lifecycle).state != State::Idle
    }
    pub(super) fn media_offer(&mut self, command: Command) -> Result<Vec<Value>, Value> {
        if command.protocol_version != Some(PROTOCOL_VERSION) || self.native_epoch == 0 {
            return Err(error(
                Some(&command.id),
                "media-offer-unavailable",
                "A current native protocol handshake is required",
            ));
        }
        let request: OfferRequest = serde_json::from_value(
            command.context.clone().unwrap_or(Value::Null),
        )
        .map_err(|_| {
            error(
                Some(&command.id),
                "invalid-media-policy",
                "Host media policy is invalid",
            )
        })?;
        request.local_policy.validate().map_err(|_| {
            error(
                Some(&command.id),
                "invalid-media-policy",
                "Host media policy is invalid",
            )
        })?;
        let ready = self.hello(&command)?;
        let capabilities = ready[0]["capabilities"].clone();
        let offer = create_offer(
            &capabilities,
            &request.local_policy,
            self.native_epoch,
            now_ms(),
        )
        .map_err(|message| error(Some(&command.id), "media-offer-unavailable", message))?;
        self.cache_native_offer(offer.clone(), request.local_policy, now_ms())
            .map_err(|message| error(Some(&command.id), "native-offer-busy", message))?;
        Ok(vec![
            json!({"id":command.id,"type":"media-offer","offer":offer,"runtimeCapabilities":capabilities}),
        ])
    }

    fn cache_native_offer(
        &mut self,
        offer: NativeOffer,
        policy: LocalMediaPolicy,
        now: u64,
    ) -> Result<(), &'static str> {
        offer.validate().map_err(|_| "Invalid native offer")?;
        policy.validate().map_err(|_| "Invalid host media policy")?;
        if offer.runtime_epoch != self.native_epoch || offer.expires_at_ms <= now {
            return Err("Native offer is stale");
        }
        self.native_offers
            .retain(|_, entry| entry.offer.expires_at_ms > now);
        if self.native_offers.len() >= 4 || self.native_offers.contains_key(offer.offer_id.as_str())
        {
            return Err("Too many native offers are pending");
        }
        self.native_offers.insert(
            offer.offer_id.as_str().into(),
            RetainedOffer { offer, policy },
        );
        Ok(())
    }

    pub(super) fn media_status(&self, command: Command) -> Result<Vec<Value>, Value> {
        let native_idle = !self.native_session_occupied();
        let active = self.active_lease.as_ref().map(|lease| {
            let mut value = lease.binding.clone();
            value["startId"] = json!(lease.start_id);
            let state = if lock_lifecycle(&self.lifecycle).state != State::Connected {
                NativeMediaState::Recovering
            } else if self
                .media_runtime
                .as_ref()
                .and_then(MediaRuntime::published_video_frames)
                .zip(lease.published_baseline)
                .is_some_and(|(current, baseline)| current > baseline)
            {
                NativeMediaState::Streaming
            } else if self
                .external_session
                .as_ref()
                .is_some_and(|session| session.worker.pid().is_none())
            {
                NativeMediaState::Starting
            } else {
                NativeMediaState::Negotiating
            };
            value["state"] = json!(state);
            value
        });
        Ok(vec![
            json!({"id":command.id,"type":"media-status","active":active,"runtimeEpoch":self.native_epoch,
            "nativeIdle":native_idle,"legacyActive":self.active_lease.is_none()&&!native_idle}),
        ])
    }

    pub(super) fn cancel_media_offer(&mut self, command: Command) -> Result<Vec<Value>, Value> {
        let id = command.offer_id.ok_or_else(|| {
            error(
                Some(&command.id),
                "missing-offer",
                "Offer identity is required",
            )
        })?;
        self.native_offers.remove(&id);
        Ok(vec![response(command.id, "ok")])
    }

    pub(super) fn start_prepared(&mut self, mut command: Command) -> Result<Vec<Value>, Value> {
        if self.native_session_occupied() {
            return Err(error(
                Some(&command.id),
                "native-session-busy",
                "Stop the active native session before starting another",
            ));
        }
        let request: StartRequest = serde_json::from_value(
            command.context.take().unwrap_or(Value::Null),
        )
        .map_err(|_| {
            error(
                Some(&command.id),
                "invalid-prepared-lease",
                "The private prepared lease is invalid",
            )
        })?;
        let lease = request.lease;
        let retained = self
            .native_offers
            .remove(lease.offer_id.as_str())
            .ok_or_else(|| {
                error(
                    Some(&command.id),
                    "native-offer-stale",
                    "The native offer is absent, expired or already consumed",
                )
            })?;
        lease
            .validate_against(&retained.offer, now_ms())
            .map_err(|_| {
                error(
                    Some(&command.id),
                    "invalid-prepared-lease",
                    "The private prepared lease does not match the native offer",
                )
            })?;
        let binding = lease.public_binding();
        let id = command.id.clone();
        let published_baseline = self
            .media_runtime
            .as_ref()
            .and_then(MediaRuntime::published_video_frames);
        let result = match &lease.media {
            PreparedMedia::Gfn { context } => {
                command.context = Some(context.clone());
                self.start_gfn(command)
            }
            PreparedMedia::Worker { .. } => self.start_worker(id.clone(), lease, retained),
        };
        match result {
            Ok(mut responses) => {
                self.active_lease = Some(ActiveLease {
                    binding: binding.clone(),
                    start_id: id,
                    published_baseline,
                });
                for message in &mut responses {
                    if message["type"] == "ok" {
                        message["leaseId"] = binding["leaseId"].clone();
                    }
                }
                Ok(responses)
            }
            Err(failure) => {
                self.stop("native prepared session failed");
                Err(failure)
            }
        }
    }

    fn start_worker(
        &mut self,
        id: String,
        lease: HostBoundPreparedLease,
        retained: RetainedOffer,
    ) -> Result<Vec<Value>, Value> {
        let binding = lease.public_binding();
        let worker_binding = lease.worker_binding();
        let PreparedMedia::Worker { package, prepared } = lease.media else {
            return Err(error(
                Some(&id),
                "invalid-prepared-lease",
                "A worker preparation is required",
            ));
        };
        let accepted = prepared.accepted;
        validate_portable_color(&accepted)
            .map_err(|message| error(Some(&id), "unsupported-media-color", message))?;
        let verified = verify(&package.version_root, &package.expected_manifest).map_err(|_| {
            error(
                Some(&id),
                "media-package-invalid",
                "The approved media package could not be verified or pinned",
            )
        })?;
        let policy = retained.policy;
        let limits = retained.offer.limits;
        let runtime = self
            .media_runtime
            .clone()
            .filter(MediaRuntime::is_embedded)
            .ok_or_else(|| {
                error(
                    Some(&id),
                    "embedded-runtime-required",
                    "Provider media requires the existing embedded runtime",
                )
            })?;
        if accepted
            .audio
            .as_ref()
            .is_some_and(|audio| audio.channels != 2)
        {
            return Err(error(
                Some(&id),
                "unsupported-audio-format",
                "This native runtime currently offers stereo Opus only",
            ));
        }
        let stream = stream_config(&accepted, &policy);
        let audio =
            opennow_streamer_protocol::AudioOutputDevice::new(policy.audio_output_device.clone())
                .map_err(|_| {
                error(
                    Some(&id),
                    "invalid-audio-device",
                    "The host audio output is invalid",
                )
            })?;
        let (feedback_tx, feedback) = std::sync::mpsc::channel();
        let session = runtime
            .start_with_audio_device(feedback_tx, stream, &policy.video_backend, audio)
            .map_err(|_| {
                error(
                    Some(&id),
                    "media-output-unavailable",
                    "The accepted media format could not start on the selected host devices",
                )
            })?;
        let generation = {
            let mut lifecycle = lock_lifecycle(&self.lifecycle);
            lifecycle.generation = lifecycle.generation.wrapping_add(1);
            lifecycle.state = State::Connected;
            lifecycle.context = None;
            lifecycle.generation
        };
        let initial = WorkerBootstrap {
            version: 1,
            binding: worker_binding,
            attempt_generation: generation,
            control_port: 0,
            authentication: SecretBytes::new(Vec::new()).map_err(|_| {
                error(
                    Some(&id),
                    "media-bootstrap-invalid",
                    "Worker bootstrap is invalid",
                )
            })?,
            accepted: accepted.clone(),
            limits: limits.clone(),
            provider_bootstrap: prepared.bootstrap,
        };
        let worker = Arc::new(
            WorkerSession::spawn(verified, &package.data_root, initial).map_err(|_| {
                error(
                    Some(&id),
                    "media-worker-start-failed",
                    "The approved native media worker could not start",
                )
            })?,
        );
        session.control().start_replay(
            ReplayBufferConfig {
                enabled: policy.replay_buffer_enabled,
                duration: Duration::from_secs(policy.replay_buffer_seconds.into()),
                memory_bytes: usize::from(policy.replay_buffer_memory_mi_b) * 1024 * 1024,
            },
            Arc::clone(&self.replay_budget),
        );
        let sink = session.sink();
        let control = session.control();
        let input = runtime.captured_input();
        input.clear();
        input.set_text_ready(generation, false);
        let paused = Arc::new(AtomicBool::new(false));
        let running = Arc::new(AtomicBool::new(true));
        let (presented_tx, presented) = std::sync::mpsc::sync_channel(8);
        let presentation_drops = Arc::new(AtomicU64::new(0));
        let bridge = Bridge {
            worker: Arc::clone(&worker),
            runtime,
            sink,
            media: control,
            input,
            feedback,
            presented,
            paused: Arc::clone(&paused),
            running: Arc::clone(&running),
            output: self.events.clone(),
            lifecycle: Arc::clone(&self.lifecycle),
            hid: Arc::clone(&self.hid_runtime),
            generation,
            start_id: id.clone(),
            binding,
            accepted,
            limits,
            input_ready: false,
            last_paused: false,
            pending: BTreeMap::new(),
            next_sequence: 1,
            retired_input: 0,
            text: None,
            started: Instant::now(),
            last_telemetry: Instant::now(),
            accepted_frames: 0,
            accepted_bytes: 0,
            presented_frames: 0,
            started_playback: false,
            presentation_drops: Arc::clone(&presentation_drops),
            last_decoded: None,
            video_recovery: None,
        };
        let thread = thread::Builder::new()
            .name("provider-native-media".into())
            .spawn(move || bridge.run())
            .map_err(|_| {
                error(
                    Some(&id),
                    "media-worker-start-failed",
                    "Native media dispatch could not start",
                )
            })?;
        self.external_session = Some(ExternalSession {
            worker,
            paused,
            running,
            presented: presented_tx,
            thread: Some(thread),
            presentation_drops,
        });
        self.media_session = Some(session);
        Ok(vec![worker_start_response(&id, &policy)])
    }

    pub fn notify_presented(&self, provenance: FrameProvenance) {
        if let Some(session) = self.external_session.as_ref() {
            session.notify_presented(provenance);
        }
    }
}

fn worker_start_response(id: &str, policy: &LocalMediaPolicy) -> Value {
    json!({
        "id":id,"type":"ok","transport":"provider-worker","inputReady":false,
        "replayEnabled":policy.replay_buffer_enabled,
    })
}

fn create_offer(
    capabilities: &Value,
    policy: &LocalMediaPolicy,
    epoch: u64,
    now: u64,
) -> Result<NativeOffer, &'static str> {
    if epoch == 0 || capabilities["supportsVideoDecode"] != true {
        return Err("No native video decoder is available");
    }
    let video_backend =
        opennow_streamer_platform::effective_embedded_video_backend(&policy.video_backend);
    let mut formats = Vec::new();
    for backend in capabilities["videoBackends"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let name = backend["backend"].as_str().unwrap_or("");
        if backend["available"] != true
            || !opennow_streamer_platform::embedded_backend_allowed_by_policy(&video_backend, name)
        {
            continue;
        }
        for codec in backend["codecs"].as_array().into_iter().flatten() {
            if codec["available"] != true {
                continue;
            }
            let encoding = match codec["codec"].as_str() {
                Some("h264") => VideoEncoding::H264AnnexB,
                Some("h265" | "hevc") => VideoEncoding::HevcAnnexB,
                Some("av1") => VideoEncoding::Av1Obu,
                _ => continue,
            };
            let qualities: Vec<&str> = codec["colorQualities"]
                .as_array()
                .map(|items| items.iter().filter_map(Value::as_str).collect())
                .unwrap_or_else(|| vec!["8bit_420"]);
            for quality in qualities {
                let (bit_depth, chroma) = match quality {
                    "8bit_420" => (8, Chroma::Yuv420),
                    "8bit_444" => (8, Chroma::Yuv444),
                    "10bit_420" => (10, Chroma::Yuv420),
                    "10bit_444" => (10, Chroma::Yuv444),
                    _ => continue,
                };
                if (encoding == VideoEncoding::H264AnnexB
                    && (bit_depth != 8 || chroma != Chroma::Yuv420))
                    || (encoding == VideoEncoding::Av1Obu && chroma != Chroma::Yuv420)
                {
                    continue;
                }
                let support = VideoSupport {
                    encoding,
                    bit_depth,
                    chroma,
                    dynamic_range: DynamicRange::Sdr,
                    max_width: 4096,
                    max_height: 2304,
                    max_fps: 360,
                };
                if !formats.contains(&support) {
                    formats.push(support);
                }
            }
        }
    }
    if formats.is_empty() {
        return Err("The selected native backend has no supported worker video format");
    }
    let mut bytes = [0; 16];
    getrandom::fill(&mut bytes).map_err(|_| "Native offer entropy is unavailable")?;
    let id: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    let audio = if capabilities["supportsAudioDecode"] == true {
        vec![AudioFormat {
            codec: AudioCodec::Opus,
            sample_rate: 48000,
            channels: 2,
        }]
    } else {
        vec![]
    };
    Ok(NativeOffer {
        version: 1,
        offer_id: OfferId::new(id).map_err(|_| "Invalid native offer identity")?,
        runtime_epoch: epoch,
        expires_at_ms: now.saturating_add(30_000),
        video_formats: List::new(formats).map_err(|_| "Native format list is too large")?,
        audio_formats: List::new(audio).map_err(|_| "Native audio list is too large")?,
        input: InputCapabilities {
            keyboard: true,
            relative_mouse: true,
            absolute_mouse: true,
            text: true,
            gamepad_slots: 4,
            rumble: true,
        },
        limits: MediaLimits {
            max_video_access_unit_bytes: 4 * 1024 * 1024,
            max_audio_packet_bytes: 1275,
            max_buffered_video_bytes: 16 * 1024 * 1024,
            max_buffered_video_frames: 4,
            max_buffered_audio_ms: 100,
            max_control_message_bytes: 65536,
            max_pending_input_events: 256,
        },
    })
}

fn validate_portable_color(accepted: &AcceptedMedia) -> Result<(), &'static str> {
    if accepted.video.dynamic_range() != DynamicRange::Sdr
        || accepted.video.color.primaries != Primaries::Bt709
        || accepted.video.color.matrix == Matrix::Bt2020NonConstant
        || !matches!(
            accepted.video.color.transfer,
            Transfer::Bt709 | Transfer::Srgb
        )
    {
        return Err("Provider video requires a supported SDR BT.709 color profile");
    }
    Ok(())
}

fn stream_config(accepted: &AcceptedMedia, policy: &LocalMediaPolicy) -> MediaStreamConfig {
    let video = &accepted.video;
    MediaStreamConfig {
        audio_enabled: accepted.audio.is_some(),
        codec: match video.encoding {
            VideoEncoding::H264AnnexB => MediaVideoCodec::H264,
            VideoEncoding::HevcAnnexB => MediaVideoCodec::H265,
            VideoEncoding::Av1Obu => MediaVideoCodec::Av1,
        },
        color_quality: match (video.bit_depth, video.chroma) {
            (10, Chroma::Yuv444) => MediaColorQuality::TenBit444,
            (10, _) => MediaColorQuality::TenBit420,
            (_, Chroma::Yuv444) => MediaColorQuality::EightBit444,
            _ => MediaColorQuality::EightBit420,
        },
        color: Some(video.color),
        hdr: false,
        width: video.width,
        height: video.height,
        fps: video.fps,
        bitrate_bps: (policy.max_bitrate_mbps * 1_000_000.0).round() as u32,
        cloud_gsync: false,
        shortcuts: StreamShortcutBindings::from_json(&json!(policy.shortcuts)),
    }
}

pub(super) struct ExternalSession {
    worker: Arc<WorkerSession>,
    paused: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    presented: SyncSender<FrameProvenance>,
    thread: Option<JoinHandle<()>>,
    presentation_drops: Arc<AtomicU64>,
}

impl ExternalSession {
    fn finished(&self) -> bool {
        self.thread.as_ref().is_none_or(JoinHandle::is_finished)
    }
    pub(super) fn set_paused(&self, paused: bool) {
        self.paused.store(paused, Ordering::Release);
    }
    pub(super) fn request_keyframe(&self) -> Result<(), &'static str> {
        self.worker
            .send(ControlMessage::Keyframe {
                attempt_generation: self.worker.attempt_generation(),
                track_id: VIDEO_TRACK_ID,
            })
            .map_err(|_| "The provider could not accept a recording keyframe request")
    }
    fn notify_presented(&self, source: FrameProvenance) {
        if source.attempt_generation == self.worker.attempt_generation()
            && source.track_id == VIDEO_TRACK_ID
            && source.source.is_some()
            && self.presented.try_send(source).is_err()
        {
            self.presentation_drops.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub(super) fn stop(mut self) {
        self.finish();
    }
    fn finish(&mut self) {
        self.running.store(false, Ordering::Release);
        self.worker.signal_stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Drop for ExternalSession {
    fn drop(&mut self) {
        self.finish();
    }
}

struct PendingInput {
    sent: Instant,
    last_text: bool,
    kind: AckKind,
}
struct Bridge {
    worker: Arc<WorkerSession>,
    runtime: MediaRuntime,
    sink: MediaSink,
    media: MediaControl,
    input: Arc<CapturedInputQueue>,
    feedback: Receiver<MediaFeedback>,
    presented: Receiver<FrameProvenance>,
    paused: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    output: EventSender,
    lifecycle: Arc<Mutex<Lifecycle>>,
    hid: Arc<HidRuntime>,
    generation: u64,
    start_id: String,
    binding: Value,
    accepted: AcceptedMedia,
    limits: MediaLimits,
    input_ready: bool,
    last_paused: bool,
    pending: BTreeMap<u64, PendingInput>,
    next_sequence: u64,
    retired_input: u64,
    text: Option<opennow_streamer_protocol::text_input::UnicodeText>,
    started: Instant,
    last_telemetry: Instant,
    accepted_frames: u64,
    accepted_bytes: u64,
    presented_frames: u64,
    started_playback: bool,
    presentation_drops: Arc<AtomicU64>,
    last_decoded: Option<Instant>,
    video_recovery: Option<Instant>,
}

impl Bridge {
    fn emit(&self, kind: &str, mut fields: Value) {
        if lock_lifecycle(&self.lifecycle).generation != self.generation {
            return;
        }
        fields["startId"] = json!(self.start_id);
        fields["leaseId"] = self.binding["leaseId"].clone();
        let _ = self.output.send(event(kind, fields));
    }
    fn run(mut self) {
        if let Err(reason) = self.pump() {
            if self.running.load(Ordering::Acquire) {
                self.emit(
                    "error",
                    json!({"code":"provider-media-failed","message":reason}),
                );
            }
        }
        self.input.set_text_ready(self.generation, false);
        self.input.clear();
        self.worker.signal_stop();
        self.media.stop();
        let mut lifecycle = lock_lifecycle(&self.lifecycle);
        if lifecycle.generation == self.generation {
            lifecycle.state = State::Idle;
        }
    }
    fn pump(&mut self) -> Result<(), &'static str> {
        while self.running.load(Ordering::Acquire) {
            if lock_lifecycle(&self.lifecycle).generation != self.generation {
                return Ok(());
            }
            for _ in 0..32 {
                let Some(event) = self.worker.recv_control() else {
                    break;
                };
                match event {
                    WorkerEvent::Ready { input } => {
                        if !input.is_subset_of(&self.accepted.input) {
                            return Err("Worker expanded its input capabilities");
                        }
                        self.accepted.input = input;
                        self.input_ready = true;
                        self.input.set_text_ready(
                            self.generation,
                            self.accepted.input.text && !self.paused.load(Ordering::Acquire),
                        );
                        self.emit("input", json!({"ready":true}));
                        self.emit(
                            "transport",
                            json!({"transport":"provider-worker","processId":self.worker.pid()}),
                        );
                    }
                    WorkerEvent::Failed(reason) => return Err(reason),
                    WorkerEvent::Exited => return Err("The provider media connection ended"),
                    WorkerEvent::Discontinuity { track_id } if track_id == VIDEO_TRACK_ID => {
                        self.media.invalidate_video();
                        self.media.cut_recording(RecordingCutReason::Discontinuity);
                    }
                    WorkerEvent::Discontinuity { .. } => {
                        self.emit("telemetry", json!({"audioQueueDiscontinuity":true}))
                    }
                    WorkerEvent::Control(message) => self.control(message)?,
                }
            }
            let paused = self.paused.load(Ordering::Acquire);
            if paused != self.last_paused {
                self.input.clear();
                self.input.set_text_ready(
                    self.generation,
                    self.input_ready && self.accepted.input.text && !paused,
                );
                if paused && self.input_ready {
                    self.neutral()?;
                }
                self.last_paused = paused;
            }
            if self.input.take_overflowed() {
                return Err("Native input queue overflowed");
            }
            for _ in 0..32 {
                let Some(sample) = self.input.take_sample() else {
                    break;
                };
                if paused || !self.input_ready {
                    continue;
                }
                self.input_sample(sample)?;
            }
            if self
                .pending
                .values()
                .any(|pending| pending.sent.elapsed() > Duration::from_secs(2))
            {
                return Err("Provider input acknowledgement timed out");
            }
            for _ in 0..4 {
                let Some(frame) = self.worker.recv_video() else {
                    break;
                };
                self.push(frame, true)?;
            }
            for _ in 0..8 {
                let Some(frame) = self.worker.recv_audio() else {
                    break;
                };
                if self.accepted.audio.is_none() {
                    return Err("Worker emitted unaccepted audio");
                }
                self.push(frame, false)?;
            }
            for _ in 0..128 {
                let Ok(feedback) = self.feedback.try_recv() else {
                    break;
                };
                match feedback {
                    MediaFeedback::VideoFrameDecoded { mut provenance } => {
                        if provenance.attempt_generation != 0
                            && provenance.attempt_generation != self.generation
                        {
                            continue;
                        }
                        provenance.attempt_generation = self.generation;
                        provenance.track_id = VIDEO_TRACK_ID;
                        self.last_decoded = Some(Instant::now());
                        self.video_recovery = None;
                        self.worker
                            .send(ControlMessage::FrameProgress {
                                provenance,
                                stage: FrameStage::Decoded,
                                local_us: self.started.elapsed().as_micros() as u64,
                            })
                            .map_err(|_| "Worker progress queue unavailable")?;
                    }
                    MediaFeedback::VideoFrameAccepted {
                        provenance, bytes, ..
                    } => {
                        self.accepted_frames += 1;
                        self.accepted_bytes += u64::from(bytes);
                        self.worker
                            .send(ControlMessage::FrameProgress {
                                provenance,
                                stage: FrameStage::Accepted,
                                local_us: self.started.elapsed().as_micros() as u64,
                            })
                            .map_err(|_| "Worker progress queue overflowed")?;
                    }
                    MediaFeedback::PlaybackStarted { backend } => {
                        if !self.started_playback {
                            self.started_playback = true;
                            self.emit("status",json!({"status":"streaming","mediaBackend":backend,"transport":"provider-worker","firstFrameLatencyMs":self.started.elapsed().as_millis(),"message":"Native video output is ready"}));
                        }
                    }
                    MediaFeedback::RequestKeyframe { .. } => {
                        self.worker
                            .send(ControlMessage::Keyframe {
                                attempt_generation: self.generation,
                                track_id: VIDEO_TRACK_ID,
                            })
                            .map_err(|_| "Worker feedback queue unavailable")?;
                    }
                    MediaFeedback::DecoderError { .. } => self.request_recovery()?,
                    MediaFeedback::OutputError { .. } => {
                        return Err("The native video output failed");
                    }
                    MediaFeedback::AudioDecoderError { .. }
                    | MediaFeedback::AudioUnavailable { .. } => self.emit(
                        "log",
                        json!({"level":"warn","message":"Native audio is unavailable"}),
                    ),
                    MediaFeedback::DeviceLost {
                        recovered: false, ..
                    } => return Err("A native media device was lost"),
                    MediaFeedback::ColorFormatChanged { .. } => {
                        return Err(
                            "The decoded provider format does not match the accepted format",
                        );
                    }
                    MediaFeedback::BackendFallback { to, .. } => {
                        self.emit("telemetry", json!({"mediaBackend":to}))
                    }
                    MediaFeedback::DecodeTimings(timings) => self.emit(
                        "telemetry",
                        json!({"decodeTimings":decode_timings_event(Some(timings))}),
                    ),
                    MediaFeedback::QueueDropped { media, count } => {
                        self.emit("queue-drop", json!({"media":media,"count":count}))
                    }
                    _ => {}
                }
            }
            for source in self.presented.try_iter().take(8) {
                if source.attempt_generation != self.generation {
                    continue;
                }
                self.presented_frames += 1;
                self.worker
                    .send(ControlMessage::FrameProgress {
                        provenance: source,
                        stage: FrameStage::Presented,
                        local_us: self.started.elapsed().as_micros() as u64,
                    })
                    .map_err(|_| "Worker presentation feedback queue overflowed")?;
            }
            if !self.started_playback && self.started.elapsed() > Duration::from_secs(20) {
                return Err("The provider produced no native video output before the deadline");
            }
            if self.started_playback
                && self
                    .last_decoded
                    .is_some_and(|at| at.elapsed() > Duration::from_secs(8))
            {
                self.request_recovery()?;
            }
            if self
                .video_recovery
                .is_some_and(|at| at.elapsed() > Duration::from_secs(4))
            {
                return Err("Native video output did not recover after a keyframe request");
            }
            if self.last_telemetry.elapsed() >= Duration::from_secs(1) {
                let elapsed = self.last_telemetry.elapsed().as_secs_f64();
                self.emit("telemetry",json!({"transport":"provider-worker","framesPerSecond":self.accepted_frames as f64/elapsed,"bitrateMbps":self.accepted_bytes as f64*8.0/elapsed/1_000_000.0,
                    "presentedFrames":self.presented_frames,"publishedFrames":self.runtime.published_video_frames(),"inputReady":self.input_ready,
                    "presentationFeedbackDrops":self.presentation_drops.load(Ordering::Relaxed),"workerFeedbackCoalesced":self.worker.coalesced_frame_progress()}));
                self.accepted_frames = 0;
                self.accepted_bytes = 0;
                self.last_telemetry = Instant::now();
            }
            thread::sleep(Duration::from_millis(1));
        }
        Ok(())
    }
    fn request_recovery(&mut self) -> Result<(), &'static str> {
        if self.video_recovery.is_none() {
            self.video_recovery = Some(Instant::now());
            self.media.invalidate_video();
            self.media.cut_recording(RecordingCutReason::Discontinuity);
            self.worker
                .send(ControlMessage::Keyframe {
                    attempt_generation: self.generation,
                    track_id: VIDEO_TRACK_ID,
                })
                .map_err(|_| "Worker recovery queue unavailable")?;
        }
        Ok(())
    }
    fn push(&self, frame: worker::MediaFrame, video: bool) -> Result<(), &'static str> {
        let codec = if video {
            match self.accepted.video.encoding {
                VideoEncoding::H264AnnexB => MediaCodec::H264,
                VideoEncoding::HevcAnnexB => MediaCodec::H265,
                VideoEncoding::Av1Obu => MediaCodec::Av1,
            }
        } else {
            MediaCodec::Opus {
                channels: self
                    .accepted
                    .audio
                    .as_ref()
                    .map_or(2, |audio| audio.channels),
            }
        };
        let result = self.sink.push(EncodedFrame {
            provenance: frame.header.provenance(),
            mid: if video { "video" } else { "audio" }.into(),
            codec,
            data: Arc::from(frame.payload),
            frame_index: None,
            timestamp: frame.header.source.timestamp,
            clock_rate_hz: frame.header.source.clock_rate_hz,
            ssrc: None,
            keyframe: frame.header.keyframe,
            contiguous: frame.header.contiguous,
        });
        match result {
            PushOutcome::Closed | PushOutcome::Unsupported | PushOutcome::Paused => {
                Err("The native media sink rejected the accepted stream")
            }
            _ => Ok(()),
        }
    }
    fn neutral(&mut self) -> Result<(), &'static str> {
        self.text = None;
        self.retired_input = self.next_sequence.saturating_sub(1);
        self.pending.clear();
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.worker
            .send(ControlMessage::Neutral {
                attempt_generation: self.generation,
                sequence,
            })
            .map_err(|_| "Worker neutral-state queue unavailable")?;
        self.pending.insert(
            sequence,
            PendingInput {
                sent: Instant::now(),
                last_text: false,
                kind: AckKind::Neutral,
            },
        );
        Ok(())
    }
    fn control(&mut self, message: ControlMessage) -> Result<(), &'static str> {
        match message {
            ControlMessage::Ack {
                attempt_generation,
                sequence,
                kind,
            } if attempt_generation == self.generation => {
                if matches!(kind, AckKind::Input | AckKind::Neutral) {
                    if sequence == 0 {
                        return Err("Invalid worker input acknowledgement");
                    }
                    if let Some(pending) = self.pending.remove(&sequence) {
                        if pending.kind != kind {
                            return Err("Wrong worker acknowledgement kind");
                        }
                        if pending.last_text {
                            self.text = None;
                        }
                    } else if sequence > self.retired_input {
                        return Err("Uncorrelated worker input acknowledgement");
                    }
                }
                Ok(())
            }
            ControlMessage::Rumble {
                attempt_generation,
                controller,
                incarnation,
                low,
                high,
                duration_ms,
            } if attempt_generation == self.generation && self.accepted.input.rumble => {
                if self.input_ready
                    && !self.paused.load(Ordering::Acquire)
                    && self.hid.inventory().incarnation(controller) == Some(incarnation)
                {
                    self.emit("controller-rumble",json!({"controllerId":controller,"sourceIncarnation":incarnation,"lowFrequency":low,"highFrequency":high,"durationMs":duration_ms}));
                }
                Ok(())
            }
            ControlMessage::Ended { attempt_generation }
                if attempt_generation == self.generation =>
            {
                Err("The provider media connection ended")
            }
            _ => Err("Worker sent an unsupported or stale control message"),
        }
    }
    fn send_input(
        &mut self,
        event: InputEvent,
        captured_us: u64,
        last_text: bool,
    ) -> Result<(), &'static str> {
        if self.pending.len() >= usize::from(self.limits.max_pending_input_events) {
            return Err("Worker input queue overflowed");
        }
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        self.worker
            .send(ControlMessage::Input {
                attempt_generation: self.generation,
                sequence,
                captured_us,
                event,
            })
            .map_err(|_| "Worker input queue unavailable")?;
        self.pending.insert(
            sequence,
            PendingInput {
                sent: Instant::now(),
                last_text,
                kind: AckKind::Input,
            },
        );
        Ok(())
    }
    fn input_sample(&mut self, sample: CapturedInputSample) -> Result<(), &'static str> {
        let caps = &self.accepted.input;
        let captured_us = sample
            .captured_at
            .saturating_duration_since(self.started)
            .as_micros() as u64;
        let event = match sample.input {
            CapturedInput::Key {
                virtual_key,
                modifiers,
                pressed,
            } if caps.keyboard => InputEvent::Key {
                virtual_key,
                modifiers,
                pressed,
            },
            CapturedInput::MouseMove { delta_x, delta_y } if caps.relative_mouse => {
                InputEvent::MouseRelative {
                    x: delta_x,
                    y: delta_y,
                }
            }
            CapturedInput::MouseAbsolute {
                x,
                y,
                width,
                height,
            } if caps.absolute_mouse => InputEvent::MouseAbsolute {
                x,
                y,
                width,
                height,
            },
            CapturedInput::MouseButton { button, pressed }
                if caps.relative_mouse || caps.absolute_mouse =>
            {
                InputEvent::MouseButton { button, pressed }
            }
            CapturedInput::MouseWheel { delta_x, delta_y }
                if caps.relative_mouse || caps.absolute_mouse =>
            {
                InputEvent::MouseWheel {
                    x: delta_x,
                    y: delta_y,
                }
            }
            CapturedInput::Gamepad {
                controller_id,
                bitmap,
                buttons,
                left_trigger,
                right_trigger,
                left_stick_x,
                left_stick_y,
                right_stick_x,
                right_stick_y,
            } if controller_id < caps.gamepad_slots => InputEvent::Gamepad {
                controller: controller_id,
                bitmap,
                buttons,
                left_trigger,
                right_trigger,
                left_x: left_stick_x,
                left_y: left_stick_y,
                right_x: right_stick_x,
                right_y: right_stick_y,
                incarnation: self.hid.inventory().incarnation(controller_id).unwrap_or(0),
            },
            CapturedInput::Text(text) if caps.text => {
                if text.is_cancelled() {
                    return Ok(());
                }
                let maximum =
                    (self.limits.max_control_message_bytes as usize).saturating_sub(1024) / 6;
                if maximum == 0 {
                    return Err("Worker control limit cannot carry text");
                }
                let text_value = text.as_str().to_owned();
                let paste_id = self.next_sequence;
                let mut offset = 0;
                self.text = Some(text);
                while offset < text_value.len() {
                    let mut end = (offset + maximum.min(8192)).min(text_value.len());
                    while !text_value.is_char_boundary(end) {
                        end -= 1;
                    }
                    if end == offset {
                        return Err("Worker text chunk cannot be framed");
                    }
                    self.send_input(
                        InputEvent::Text {
                            paste_id,
                            offset: offset as u32,
                            final_chunk: end == text_value.len(),
                            utf8: text_value[offset..end].into(),
                        },
                        captured_us,
                        end == text_value.len(),
                    )?;
                    offset = end;
                }
                return Ok(());
            }
            CapturedInput::Guide => {
                self.emit("overlay-request", json!({"source":"gamepad"}));
                return Ok(());
            }
            CapturedInput::Screenshot => {
                self.emit("screenshot-request", json!({"source":"keyboard"}));
                return Ok(());
            }
            CapturedInput::RecordingToggle => {
                self.emit("recording-toggle-request", json!({"source":"keyboard"}));
                return Ok(());
            }
            CapturedInput::Shortcut(action) => {
                forward_shortcut_action(&self.output, Some(&self.runtime), action);
                return Ok(());
            }
            _ => return Ok(()),
        };
        self.send_input(event, captured_us, false)
    }
}
