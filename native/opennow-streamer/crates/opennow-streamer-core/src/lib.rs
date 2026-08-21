use std::net::UdpSocket;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use opennow_streamer_platform::{
    EncodedFrame, MediaCodec, MediaFeedback, MediaRuntime, MediaSession, MediaSink, PushOutcome,
    supports_audio_decode, supports_audio_output, video_backends,
};
use opennow_streamer_protocol::{
    Capabilities, Command, PROTOCOL_VERSION, SessionContext, error, event, response,
};
use opennow_streamer_transport::{
    NvstDropReason, NvstReceiveEvent, NvstReceiverState, NvstUdpReceiverSession,
    PreferredVideoTransport, ReservedNvstBundle, TransportControl, TransportEvent,
    TransportSession, negotiate, reserve_nvst_mjolnir_udp_socket,
    select_preferred_video_transport, spawn_nvst_mjolnir_receiver,
    spawn_nvst_udp_receiver_with_socket,
};
use serde_json::{Value, json};

pub use opennow_streamer_transport::{EncodedMediaFrame, MediaConsumer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Idle,
    Prepared,
    Negotiating,
    Connected,
}

const ENCODED_MEDIA_QUEUE_CAPACITY: usize = 8;

pub struct Engine {
    lifecycle: Arc<Mutex<Lifecycle>>,
    transport: Option<TransportSession>,
    nvst_transport: Option<NvstUdpReceiverSession>,
    nvst_mjolnir_transport: Option<NvstUdpReceiverSession>,
    reserved_nvst_bundle: Option<ReservedNvstBundle>,
    nvst_hole_punch_socket: Option<UdpSocket>,
    events: Sender<Value>,
    media_consumer: Option<MediaConsumer>,
    media_runtime: Option<MediaRuntime>,
    media_session: Option<MediaSession>,
    media_worker: Option<JoinHandle<()>>,
    media_feedback: Option<Receiver<MediaFeedback>>,
    feedback_worker: Option<JoinHandle<()>>,
}

#[derive(Debug)]
struct Lifecycle {
    state: State,
    context: Option<SessionContext>,
    generation: u64,
}

impl Engine {
    pub fn new(events: Sender<Value>) -> Self {
        Self {
            lifecycle: Arc::new(Mutex::new(Lifecycle {
                state: State::Idle,
                context: None,
                generation: 0,
            })),
            transport: None,
            nvst_transport: None,
            nvst_mjolnir_transport: None,
            reserved_nvst_bundle: None,
            nvst_hole_punch_socket: None,
            events,
            media_consumer: None,
            media_runtime: None,
            media_session: None,
            media_worker: None,
            media_feedback: None,
            feedback_worker: None,
        }
    }

    pub fn with_media_consumer(events: Sender<Value>, media_consumer: MediaConsumer) -> Self {
        Self {
            lifecycle: Arc::new(Mutex::new(Lifecycle {
                state: State::Idle,
                context: None,
                generation: 0,
            })),
            transport: None,
            nvst_transport: None,
            nvst_mjolnir_transport: None,
            reserved_nvst_bundle: None,
            nvst_hole_punch_socket: None,
            events,
            media_consumer: Some(media_consumer),
            media_runtime: None,
            media_session: None,
            media_worker: None,
            media_feedback: None,
            feedback_worker: None,
        }
    }

    pub fn with_media_runtime(events: Sender<Value>, media_runtime: MediaRuntime) -> Self {
        Self {
            lifecycle: Arc::new(Mutex::new(Lifecycle {
                state: State::Idle,
                context: None,
                generation: 0,
            })),
            transport: None,
            nvst_transport: None,
            nvst_mjolnir_transport: None,
            reserved_nvst_bundle: None,
            nvst_hole_punch_socket: None,
            events,
            media_consumer: None,
            media_runtime: Some(media_runtime),
            media_session: None,
            media_worker: None,
            media_feedback: None,
            feedback_worker: None,
        }
    }

    pub fn handle(&mut self, command: Command) -> (Vec<Value>, bool) {
        let id = command.id.clone();
        let result = match command.kind.as_str() {
            "hello" => self.hello(&command),
            "nvst-bind" => self.nvst_bind(command),
            "nvst-send" => self.nvst_send(command),
            "start" => self.start(command),
            "offer" => self.offer(command),
            "remote-ice" => self.remote_ice(command),
            "input" => self.input(command),
            "input-paused" => self.set_paused(command),
            "surface" => self.update_surface(command),
            "bitrate" | "update-shortcuts" => Err(error(
                Some(&id),
                "unsupported-command",
                format!(
                    "Native streamer v2 cannot apply the {} command",
                    command.kind
                ),
            )),
            "stop" => {
                self.stop(command.reason.as_deref().unwrap_or("stopped"));
                Ok(vec![response(id, "ok")])
            }
            other => Err(error(
                Some(&id),
                "unknown-command",
                format!("Unknown command: {other}"),
            )),
        };

        match result {
            Ok(values) => (values, true),
            Err(value) => (vec![value], true),
        }
    }

    fn hello(&self, command: &Command) -> Result<Vec<Value>, Value> {
        if command.protocol_version != Some(PROTOCOL_VERSION) {
            return Err(error(
                Some(&command.id),
                "protocol-version-mismatch",
                format!("Native streamer v2 requires protocol {PROTOCOL_VERSION}"),
            ));
        }
        let backends = video_backends();
        let media_ready = self.media_runtime.is_some();
        let video_ready = media_ready && backends.iter().any(|backend| backend.available);
        let capabilities = Capabilities {
            protocol_version: PROTOCOL_VERSION,
            backend: "native",
            fallback_reason: (!media_ready)
                .then_some("Native streamer v2 requires an in-process decoded media runtime"),
            supports_offer_answer: media_ready,
            supports_remote_ice: media_ready,
            supports_local_ice: media_ready,
            supports_input: media_ready,
            supports_video_decode: video_ready,
            supports_video_present: video_ready,
            supports_audio_decode: media_ready && supports_audio_decode(),
            supports_audio_output: media_ready && supports_audio_output(),
            video_backends: backends,
        };
        Ok(vec![json!({
            "id": command.id,
            "type": "ready",
            "capabilities": capabilities,
        })])
    }

    fn nvst_bind(&mut self, command: Command) -> Result<Vec<Value>, Value> {
        if self.reserved_nvst_bundle.is_none() {
            let bundle = ReservedNvstBundle::reserve().map_err(|bind_error| {
                error(
                    Some(&command.id),
                    "nvst-bind-failed",
                    format!("failed to reserve NVST UDP socket: {bind_error}"),
                )
            })?;
            eprintln!(
                "NVST reserved video UDP socket on {} (Mjolnir on {})",
                bundle
                    .local_addr()
                    .map(|addr| addr.to_string())
                    .unwrap_or_else(|_| "unknown".to_owned()),
                bundle
                    .mjolnir_local_addr()
                    .map(|addr| addr.to_string())
                    .unwrap_or_else(|_| "unknown".to_owned()),
            );
            self.reserved_nvst_bundle = Some(bundle);
        }
        let bundle = self.reserved_nvst_bundle.as_mut().ok_or_else(|| {
            error(
                Some(&command.id),
                "nvst-bind-failed",
                "reserved NVST UDP socket has no local port",
            )
        })?;
        let local_addr = bundle.local_addr().map_err(|_| {
            error(
                Some(&command.id),
                "nvst-bind-failed",
                "reserved NVST UDP socket has no local port",
            )
        })?;
        let mjolnir_addr = bundle.mjolnir_local_addr().map_err(|_| {
            error(
                Some(&command.id),
                "nvst-bind-failed",
                "reserved NVST Mjolnir UDP socket has no local port",
            )
        })?;
        let port = local_addr.port();
        let local_address = bundle.advertised_local_address();
        let identity = bundle.identity();
        Ok(vec![json!({
            "id": command.id,
            "type": "nvst-bound",
            "port": port,
            "mjolnirPort": mjolnir_addr.port(),
            "localAddress": local_address,
            "iceUsernameFragment": identity.ice_username_fragment,
            "icePassword": identity.ice_password,
            "dtlsFingerprint": identity.dtls_fingerprint,
        })])
    }

    fn nvst_send(&mut self, command: Command) -> Result<Vec<Value>, Value> {
        let host = command.host.ok_or_else(|| {
            error(
                Some(&command.id),
                "nvst-send-failed",
                "nvst-send requires host",
            )
        })?;
        let port = command.port.ok_or_else(|| {
            error(
                Some(&command.id),
                "nvst-send-failed",
                "nvst-send requires port",
            )
        })?;
        let payload = BASE64
            .decode(command.payload_base64.unwrap_or_default())
            .map_err(|decode_error| {
                error(
                    Some(&command.id),
                    "nvst-send-failed",
                    format!("nvst-send payload is not valid base64: {decode_error}"),
                )
            })?;
        let send_result = if let Some(bundle) = self.reserved_nvst_bundle.as_ref() {
            bundle.send_to(&payload, host.as_str(), port)
        } else if let Some(socket) = self.nvst_hole_punch_socket.as_ref() {
            socket.send_to(&payload, (host.as_str(), port))
        } else {
            return Err(error(
                Some(&command.id),
                "nvst-send-failed",
                "NVST UDP socket has not been reserved",
            ));
        };
        send_result.map_err(|send_error| {
            error(
                Some(&command.id),
                "nvst-send-failed",
                format!("failed to send NVST UDP datagram: {send_error}"),
            )
        })?;
        Ok(vec![response(command.id, "ok")])
    }

    fn start(&mut self, command: Command) -> Result<Vec<Value>, Value> {
        let context = parse_context(command.context, &command.id)?;
        validate_context(&context, &command.id)?;
        {
            let lifecycle = lock_lifecycle(&self.lifecycle);
            if lifecycle.state != State::Idle {
                return Err(invalid_state(&command.id, "start", lifecycle.state, "Idle"));
            }
        }
        if self.media_runtime.is_some()
            && context
                .settings
                .get("codec")
                .and_then(Value::as_str)
                .is_some_and(|codec| !codec.eq_ignore_ascii_case("h264"))
        {
            return Err(error(
                Some(&command.id),
                "unsupported-video-codec",
                "Native streamer v2 was built with H.264 decode only",
            ));
        }
        let transport_context = serde_json::to_value(&context).map_err(|context_error| {
            error(
                Some(&command.id),
                "invalid-context",
                format!("Session context is not serializable: {context_error}"),
            )
        })?;
        let (nvst_config, fallback_note) =
            match select_preferred_video_transport(&transport_context) {
                PreferredVideoTransport::Nvst(config) => (Some(config), None),
                PreferredVideoTransport::WebRtcFallback(reason) => (
                    None,
                    Some(format!(
                        "NVST unavailable; using WebRTC fallback: {reason:?}"
                    )),
                ),
            };

        if let Some(transport) = self.transport.take() {
            transport.stop();
        }
        if let Some(transport) = self.nvst_transport.take() {
            transport.stop();
        }
        if let Some(transport) = self.nvst_mjolnir_transport.take() {
            transport.stop();
        }
        self.stop_media_resources();
        if let Some(runtime) = self.media_runtime.clone() {
            let (feedback_sender, feedback_receiver) = std::sync::mpsc::channel();
            let session = runtime
                .start(feedback_sender)
                .map_err(|message| error(Some(&command.id), "media-output-unavailable", message))?;
            let sink = session.sink();
            let (media_consumer, media_receiver) =
                std::sync::mpsc::sync_channel(ENCODED_MEDIA_QUEUE_CAPACITY);
            let output = self.events.clone();
            let media_worker = match thread::Builder::new()
                .name("opennow-media-consumer".to_owned())
                .spawn(move || consume_encoded_media(&output, media_receiver, sink))
            {
                Ok(worker) => worker,
                Err(spawn_error) => {
                    session.stop();
                    return Err(error(
                        Some(&command.id),
                        "media-worker-failed",
                        spawn_error.to_string(),
                    ));
                }
            };
            self.media_consumer = Some(media_consumer);
            self.media_session = Some(session);
            self.media_worker = Some(media_worker);
            self.media_feedback = Some(feedback_receiver);
        }

        let mut nvst_events = None;
        if let Some(config) = nvst_config {
            let Some(media_consumer) = self.media_consumer.clone() else {
                self.stop_media_resources();
                return Err(error(
                    Some(&command.id),
                    "media-consumer-unavailable",
                    "NVST video requires an in-process encoded media consumer",
                ));
            };
            let (event_sender, event_receiver) = std::sync::mpsc::channel();
            let (reserved_socket, reserved_rtc, reserved_mjolnir) =
                match self.reserved_nvst_bundle.take() {
                    Some(bundle) => {
                        self.nvst_hole_punch_socket = bundle.try_clone_socket().ok();
                        let (socket, rtc, mjolnir_socket) = bundle.into_parts();
                        (Some(socket), Some(rtc), Some(mjolnir_socket))
                    }
                    None => (None, None, None),
                };
            let mjolnir_udp_port = config.mjolnir_udp_port();
            let transport = spawn_nvst_udp_receiver_with_socket(
                config.clone(),
                media_consumer.clone(),
                event_sender.clone(),
                reserved_socket,
                reserved_rtc,
            )
            .map_err(|transport_error| {
                self.stop_media_resources();
                error(
                    Some(&command.id),
                    "nvst-start-failed",
                    transport_error.to_string(),
                )
            })?;
            self.nvst_transport = Some(transport);
            if let Some(expected_port) = mjolnir_udp_port {
                // Official two-socket model: video RTP/SRTP arrives on the
                // dedicated NATT-only Mjolnir socket, not on the ICE/DTLS bundle.
                let mjolnir_socket = match reserved_mjolnir {
                    Some(socket) => {
                        let actual_port =
                            socket.local_addr().map(|addr| addr.port()).unwrap_or(0);
                        if actual_port != expected_port {
                            eprintln!(
                                "NVST Mjolnir socket port mismatch: reserved {actual_port}, handoff expects {expected_port}; NATT keepalive determines routing"
                            );
                        }
                        socket
                    }
                    None => {
                        eprintln!(
                            "NVST Mjolnir reservation missing at start; binding a fresh video UDP socket"
                        );
                        reserve_nvst_mjolnir_udp_socket().map_err(|bind_error| {
                            if let Some(transport) = self.nvst_transport.take() {
                                transport.stop();
                            }
                            self.stop_media_resources();
                            error(
                                Some(&command.id),
                                "nvst-start-failed",
                                format!("failed to reserve NVST Mjolnir UDP socket: {bind_error}"),
                            )
                        })?
                    }
                };
                let mjolnir = spawn_nvst_mjolnir_receiver(
                    mjolnir_socket,
                    config,
                    media_consumer,
                    event_sender,
                )
                .map_err(|mjolnir_error| {
                    if let Some(transport) = self.nvst_transport.take() {
                        transport.stop();
                    }
                    self.stop_media_resources();
                    error(
                        Some(&command.id),
                        "nvst-start-failed",
                        mjolnir_error.to_string(),
                    )
                })?;
                self.nvst_mjolnir_transport = Some(mjolnir);
            }
            nvst_events = Some(event_receiver);
        } else {
            self.reserved_nvst_bundle = None;
            self.nvst_hole_punch_socket = None;
        }

        let generation = {
            let mut lifecycle = lock_lifecycle(&self.lifecycle);
            lifecycle.generation = lifecycle.generation.wrapping_add(1);
            lifecycle.context = Some(context);
            lifecycle.state = if nvst_events.is_some() {
                State::Connected
            } else {
                State::Prepared
            };
            lifecycle.generation
        };
        if let Some(nvst_events) = nvst_events {
            let output = self.events.clone();
            let lifecycle = self.lifecycle.clone();
            let media_feedback = self.media_feedback.take();
            self.feedback_worker = thread::Builder::new()
                .name("opennow-nvst-events".to_owned())
                .spawn(move || {
                    forward_nvst_session_events(
                        &output,
                        &lifecycle,
                        generation,
                        nvst_events,
                        media_feedback,
                    );
                })
                .ok();
            if self.feedback_worker.is_none() {
                if let Some(transport) = self.nvst_transport.take() {
                    transport.stop();
                }
                if let Some(transport) = self.nvst_mjolnir_transport.take() {
                    transport.stop();
                }
                self.stop_media_resources();
                let mut lifecycle = lock_lifecycle(&self.lifecycle);
                if lifecycle.generation == generation {
                    lifecycle.context = None;
                    lifecycle.state = State::Idle;
                }
                return Err(error(
                    Some(&command.id),
                    "media-worker-failed",
                    "Failed to start NVST lifecycle worker",
                ));
            }
        }
        let _ = self.events.send(event(
            "status",
            json!({
                "status": "ready",
                "message": if self.nvst_transport.is_some() {
                    "NVST authenticated H.264 receive path initialized"
                } else if self.media_runtime.is_some() {
                    "H.264 video and Opus audio media path initialized"
                } else {
                    "Native WebRTC session prepared"
                }
            }),
        ));
        if let Some(note) = fallback_note {
            let _ = self
                .events
                .send(event("log", json!({ "level": "debug", "message": note })));
        }
        let mut start_response = response(command.id, "ok");
        start_response["transport"] = Value::String(
            if self.nvst_transport.is_some() {
                "nvst"
            } else {
                "webrtc"
            }
            .to_owned(),
        );
        Ok(vec![start_response])
    }

    fn offer(&mut self, command: Command) -> Result<Vec<Value>, Value> {
        if self.nvst_transport.is_some() {
            return Err(error(
                Some(&command.id),
                "nvst-video-active",
                "NVST video is active; do not negotiate a WebRTC media offer for this session",
            ));
        }
        let offer_sdp = command.sdp.as_deref().ok_or_else(|| {
            error(
                Some(&command.id),
                "missing-sdp",
                "Offer command does not include SDP",
            )
        })?;
        let offered_context = command
            .context
            .map(|context| parse_context(Some(context), &command.id))
            .transpose()?;
        if let Some(context) = &offered_context {
            validate_context(context, &command.id)?;
        }
        let (context, generation) = {
            let mut lifecycle = lock_lifecycle(&self.lifecycle);
            if lifecycle.state == State::Idle {
                return Err(error(
                    Some(&command.id),
                    "not-started",
                    "Start must be sent before offer",
                ));
            }
            if lifecycle.state != State::Prepared {
                return Err(invalid_state(
                    &command.id,
                    "offer",
                    lifecycle.state,
                    "Prepared",
                ));
            }
            let Some(stored_context) = lifecycle.context.as_ref() else {
                lifecycle.state = State::Idle;
                return Err(error(
                    Some(&command.id),
                    "invalid-state",
                    "Prepared lifecycle is missing its session context",
                ));
            };
            let stored_session_id = stored_context.session.session_id.clone();
            if let Some(context) = offered_context {
                if context.session.session_id != stored_session_id {
                    return Err(error(
                        Some(&command.id),
                        "session-mismatch",
                        "Offer context does not match the prepared session",
                    ));
                }
                lifecycle.context = Some(context);
            }
            let Some(context) = lifecycle.context.clone() else {
                lifecycle.state = State::Idle;
                return Err(error(
                    Some(&command.id),
                    "invalid-state",
                    "Prepared lifecycle is missing its session context",
                ));
            };
            lifecycle.state = State::Negotiating;
            (context, lifecycle.generation)
        };
        let Some(media_consumer) = self.media_consumer.clone() else {
            let mut lifecycle = lock_lifecycle(&self.lifecycle);
            if lifecycle.generation == generation && lifecycle.state == State::Negotiating {
                lifecycle.state = State::Prepared;
            }
            return Err(error(
                Some(&command.id),
                "media-consumer-unavailable",
                "No in-process encoded media consumer is configured",
            ));
        };
        let threshold = partial_reliable_threshold(offer_sdp).unwrap_or(300);
        let (transport_events, receiver) = std::sync::mpsc::channel();
        let negotiated = negotiate(
            offer_sdp,
            &context.session,
            threshold,
            transport_events,
            media_consumer,
        )
        .map_err(|transport_error| {
            let mut lifecycle = lock_lifecycle(&self.lifecycle);
            if lifecycle.generation == generation && lifecycle.state == State::Negotiating {
                lifecycle.state = State::Prepared;
            }
            error(
                Some(&command.id),
                transport_error.code(),
                transport_error.to_string(),
            )
        })?;
        let output = self.events.clone();
        let lifecycle = self.lifecycle.clone();
        let transport_control = negotiated.session.control();
        let media_feedback = self.media_feedback.take();
        let feedback_worker = thread::Builder::new()
            .name("opennow-media-events".to_owned())
            .spawn(move || {
                forward_session_events(
                    &output,
                    &lifecycle,
                    generation,
                    receiver,
                    media_feedback,
                    transport_control,
                );
            });
        self.feedback_worker = Some(match feedback_worker {
            Ok(worker) => worker,
            Err(spawn_error) => {
                negotiated.session.stop();
                let mut lifecycle = lock_lifecycle(&self.lifecycle);
                if lifecycle.generation == generation && lifecycle.state == State::Negotiating {
                    lifecycle.state = State::Prepared;
                }
                drop(lifecycle);
                self.stop_media_resources();
                return Err(error(
                    Some(&command.id),
                    "media-worker-failed",
                    spawn_error.to_string(),
                ));
            }
        });
        self.transport = Some(negotiated.session);
        let _ = self.events.send(event(
            "local-ice",
            json!({ "candidate": negotiated.local_candidate }),
        ));
        Ok(vec![json!({
            "id": command.id,
            "type": "answer",
            "answer": { "sdp": negotiated.answer_sdp },
        })])
    }

    fn remote_ice(&self, command: Command) -> Result<Vec<Value>, Value> {
        if self.nvst_transport.is_some() {
            return Err(error(
                Some(&command.id),
                "nvst-input-unsupported",
                "NVST video does not implement the WebRTC input/control channel",
            ));
        }
        let state = lock_lifecycle(&self.lifecycle).state;
        if !matches!(state, State::Negotiating | State::Connected) {
            return Err(invalid_state(
                &command.id,
                "remote-ice",
                state,
                "Negotiating or Connected",
            ));
        }
        let transport = self.transport.as_ref().ok_or_else(|| {
            error(
                Some(&command.id),
                "transport-not-ready",
                "No active WebRTC transport",
            )
        })?;
        let candidate = command.candidate.as_ref().ok_or_else(|| {
            error(
                Some(&command.id),
                "missing-candidate",
                "Remote ICE command is empty",
            )
        })?;
        transport
            .add_remote_candidate(candidate)
            .map_err(|transport_error| {
                error(
                    Some(&command.id),
                    transport_error.code(),
                    transport_error.to_string(),
                )
            })?;
        Ok(vec![response(command.id, "ok")])
    }

    fn input(&self, command: Command) -> Result<Vec<Value>, Value> {
        if self.nvst_transport.is_some() {
            return Err(error(
                Some(&command.id),
                "nvst-input-unsupported",
                "NVST video input is not implemented",
            ));
        }
        let state = lock_lifecycle(&self.lifecycle).state;
        if state != State::Connected {
            return Err(invalid_state(
                &command.id,
                "input",
                state,
                "Connected with an initialized input channel",
            ));
        }
        let transport = self.transport.as_ref().ok_or_else(|| {
            error(
                Some(&command.id),
                "transport-not-ready",
                "No active WebRTC transport",
            )
        })?;
        let input = command
            .input
            .as_ref()
            .ok_or_else(|| error(Some(&command.id), "missing-input", "Input command is empty"))?;
        let bytes = BASE64
            .decode(&input.payload_base64)
            .map_err(|decode_error| {
                error(Some(&command.id), "invalid-input", decode_error.to_string())
            })?;
        transport
            .send_input(bytes, input.partially_reliable)
            .map_err(|transport_error| {
                error(
                    Some(&command.id),
                    transport_error.code(),
                    transport_error.to_string(),
                )
            })?;
        Ok(Vec::new())
    }

    fn set_paused(&self, command: Command) -> Result<Vec<Value>, Value> {
        let Some(runtime) = self.media_runtime.as_ref() else {
            return Err(error(
                Some(&command.id),
                "unsupported-command",
                "Native streamer has no media runtime for input-paused",
            ));
        };
        let paused = command.paused.ok_or_else(|| {
            error(
                Some(&command.id),
                "missing-paused",
                "Pause command does not include paused state",
            )
        })?;
        runtime
            .set_paused(paused)
            .map_err(|message| error(Some(&command.id), "media-host-unavailable", message))?;
        if let Some(media) = self.media_session.as_ref() {
            media.set_paused(paused);
        }
        if let Some(transport) = self.nvst_transport.as_ref() {
            let result = if paused {
                transport.pause()
            } else {
                transport.resume()
            };
            result.map_err(|transport_error| {
                error(
                    Some(&command.id),
                    "nvst-control-failed",
                    transport_error.to_string(),
                )
            })?;
        }
        if let Some(transport) = self.nvst_mjolnir_transport.as_ref() {
            let result = if paused {
                transport.pause()
            } else {
                transport.resume()
            };
            result.map_err(|transport_error| {
                error(
                    Some(&command.id),
                    "nvst-control-failed",
                    transport_error.to_string(),
                )
            })?;
        }
        Ok(vec![response(command.id, "ok")])
    }

    fn update_surface(&self, command: Command) -> Result<Vec<Value>, Value> {
        let Some(runtime) = self.media_runtime.as_ref() else {
            return Err(error(
                Some(&command.id),
                "unsupported-command",
                "Native streamer has no media runtime for surface",
            ));
        };
        let surface = command.surface.ok_or_else(|| {
            error(
                Some(&command.id),
                "missing-surface",
                "Surface command does not include a render surface",
            )
        })?;
        runtime
            .update_surface(surface)
            .map_err(|message| error(Some(&command.id), "media-host-unavailable", message))?;
        Ok(vec![response(command.id, "ok")])
    }

    fn stop(&mut self, reason: &str) {
        let was_active = {
            let mut lifecycle = lock_lifecycle(&self.lifecycle);
            let was_active = lifecycle.state != State::Idle;
            lifecycle.generation = lifecycle.generation.wrapping_add(1);
            lifecycle.context = None;
            lifecycle.state = State::Idle;
            was_active
        };
        if let Some(transport) = self.transport.take() {
            transport.stop();
        }
        if let Some(transport) = self.nvst_transport.take() {
            transport.stop();
        }
        if let Some(transport) = self.nvst_mjolnir_transport.take() {
            transport.stop();
        }
        self.reserved_nvst_bundle = None;
        self.nvst_hole_punch_socket = None;
        self.stop_media_resources();
        if was_active {
            let _ = self.events.send(event(
                "status",
                json!({ "status": "stopped", "message": reason }),
            ));
        }
    }

    fn stop_media_resources(&mut self) {
        if self.media_runtime.is_some() {
            self.media_consumer = None;
            if let Some(session) = self.media_session.take() {
                session.stop();
            }
            if let Some(worker) = self.media_worker.take() {
                let _ = worker.join();
            }
            self.media_feedback = None;
        }
        if let Some(worker) = self.feedback_worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.stop("process closed");
    }
}

fn partial_reliable_threshold(sdp: &str) -> Option<u16> {
    sdp.lines().find_map(|line| {
        line.trim()
            .strip_prefix("a=ri.partialReliableThresholdMs:")
            .and_then(|value| value.trim().parse().ok())
    })
}

fn parse_context(context: Option<Value>, id: &str) -> Result<SessionContext, Value> {
    let context = context.ok_or_else(|| {
        error(
            Some(id),
            "missing-context",
            "Command requires session context",
        )
    })?;
    serde_json::from_value(context).map_err(|context_error| {
        error(
            Some(id),
            "invalid-context",
            format!("Invalid session context: {context_error}"),
        )
    })
}

fn validate_context(context: &SessionContext, id: &str) -> Result<(), Value> {
    if context.session.session_id.trim().is_empty() {
        return Err(error(
            Some(id),
            "invalid-context",
            "Session context requires a non-empty sessionId",
        ));
    }
    if context.session.server_ip.trim().is_empty() {
        return Err(error(
            Some(id),
            "invalid-context",
            "Session context requires a non-empty serverIp endpoint",
        ));
    }
    if !context.settings.is_object() || !context.shortcuts.is_object() {
        return Err(error(
            Some(id),
            "invalid-context",
            "Session context settings and shortcuts must be objects",
        ));
    }
    if context
        .session
        .ice_servers
        .iter()
        .any(|server| server.urls.is_empty() || server.urls.iter().any(|url| url.trim().is_empty()))
    {
        return Err(error(
            Some(id),
            "invalid-context",
            "Every ICE server requires at least one non-empty URL",
        ));
    }
    if let Some(endpoint) = &context.session.media_connection_info {
        if endpoint.ip.trim().is_empty() || endpoint.port == 0 || endpoint.port > u16::MAX.into() {
            return Err(error(
                Some(id),
                "invalid-context",
                "mediaConnectionInfo requires a hostname and a port in 1..=65535",
            ));
        }
    }
    if context
        .session
        .connection_info
        .as_ref()
        .is_some_and(|connections| {
            connections.iter().any(|connection| {
                connection.port == 0
                    || connection.port > u16::MAX.into()
                    || connection
                        .ip
                        .as_ref()
                        .is_some_and(|ip| ip.trim().is_empty())
            })
        })
    {
        return Err(error(
            Some(id),
            "invalid-context",
            "connectionInfo requires ports in 1..=65535 and non-empty hostnames when present",
        ));
    }
    serde_json::to_value(context).map_err(|context_error| {
        error(
            Some(id),
            "invalid-context",
            format!("Session context is not serializable: {context_error}"),
        )
    })?;
    Ok(())
}

fn invalid_state(id: &str, command: &str, state: State, required: &str) -> Value {
    error(
        Some(id),
        "invalid-state",
        format!("Cannot apply {command} while lifecycle is {state:?}; required state: {required}"),
    )
}

fn lock_lifecycle(lifecycle: &Mutex<Lifecycle>) -> MutexGuard<'_, Lifecycle> {
    lifecycle
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn forward_transport_event(
    output: &Sender<Value>,
    lifecycle: &Mutex<Lifecycle>,
    generation: u64,
    transport_event: TransportEvent,
) {
    {
        let mut lifecycle = lock_lifecycle(lifecycle);
        if lifecycle.generation != generation {
            return;
        }
        match &transport_event {
            TransportEvent::Connected => lifecycle.state = State::Connected,
            TransportEvent::Disconnected(_) => {
                lifecycle.context = None;
                lifecycle.state = State::Idle;
            }
            _ => {}
        }
    }
    let value = match transport_event {
        TransportEvent::Connected => event(
            "status",
            json!({ "status": "streaming", "message": "ICE, DTLS-SRTP, and RTP connected" }),
        ),
        TransportEvent::Disconnected(message) => {
            event("status", json!({ "status": "stopped", "message": message }))
        }
        TransportEvent::InputReady(protocol_version) => event(
            "input-ready",
            json!({ "protocolVersion": protocol_version }),
        ),
        TransportEvent::Log(message) => {
            event("log", json!({ "level": "warn", "message": message }))
        }
    };
    let _ = output.send(value);
}

fn forward_nvst_session_events(
    output: &Sender<Value>,
    lifecycle: &Mutex<Lifecycle>,
    generation: u64,
    nvst_events: Receiver<NvstReceiveEvent>,
    media_feedback: Option<Receiver<MediaFeedback>>,
) {
    let mut dropped = 0;
    let mut last_drop_report = Instant::now();
    loop {
        if let Some(feedback) = media_feedback.as_ref() {
            while let Ok(feedback) = feedback.try_recv() {
                forward_nvst_media_feedback(
                    output,
                    lifecycle,
                    generation,
                    feedback,
                    &mut dropped,
                    &mut last_drop_report,
                );
            }
        }
        match nvst_events.recv_timeout(Duration::from_millis(5)) {
            Ok(nvst_event) => {
                let terminal = forward_nvst_event(output, lifecycle, generation, nvst_event);
                if terminal {
                    return;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return,
        }
    }
}

fn forward_nvst_event(
    output: &Sender<Value>,
    lifecycle: &Mutex<Lifecycle>,
    generation: u64,
    nvst_event: NvstReceiveEvent,
) -> bool {
    let terminal = matches!(
        &nvst_event,
        NvstReceiveEvent::Lifecycle(NvstReceiverState::Stopped)
            | NvstReceiveEvent::Dropped(NvstDropReason::MediaConsumerBackpressured)
            | NvstReceiveEvent::Dropped(NvstDropReason::MediaConsumerClosed)
    );
    {
        let mut lifecycle = lock_lifecycle(lifecycle);
        if lifecycle.generation != generation {
            return true;
        }
        match &nvst_event {
            NvstReceiveEvent::Lifecycle(NvstReceiverState::Running) => {
                lifecycle.state = State::Connected;
            }
            NvstReceiveEvent::Lifecycle(NvstReceiverState::Stopped)
            | NvstReceiveEvent::Dropped(NvstDropReason::MediaConsumerBackpressured)
            | NvstReceiveEvent::Dropped(NvstDropReason::MediaConsumerClosed) => {
                lifecycle.context = None;
                lifecycle.state = State::Idle;
            }
            _ => {}
        }
    }
    let value = match nvst_event {
        NvstReceiveEvent::Lifecycle(NvstReceiverState::Running) => event(
            "status",
            json!({ "status": "streaming", "message": "NVST SRTP video receiver is running" }),
        ),
        NvstReceiveEvent::Lifecycle(NvstReceiverState::Paused) => event(
            "status",
            json!({ "status": "paused", "message": "NVST SRTP video receiver is paused" }),
        ),
        NvstReceiveEvent::Lifecycle(NvstReceiverState::RecoveryRequired) => event(
            "error",
            json!({
                "code": "nvst-recovery-required",
                "message": "NVST receiver needs an explicit recovery after a timeout"
            }),
        ),
        NvstReceiveEvent::Lifecycle(NvstReceiverState::Stopped) => event(
            "status",
            json!({ "status": "stopped", "message": "NVST receiver stopped" }),
        ),
        NvstReceiveEvent::RecoveryNeeded(recovery) => event(
            "error",
            json!({
                "code": "nvst-recovery-required",
                "message": format!("NVST receive recovery required: {recovery:?}")
            }),
        ),
        NvstReceiveEvent::Dropped(NvstDropReason::MediaConsumerBackpressured) => event(
            "error",
            json!({
                "code": "media-consumer-backpressured",
                "message": "NVST receiver stopped because the decoded media path is backpressured"
            }),
        ),
        NvstReceiveEvent::Dropped(NvstDropReason::MediaConsumerClosed) => event(
            "error",
            json!({
                "code": "media-consumer-closed",
                "message": "NVST receiver stopped because the decoded media path closed"
            }),
        ),
        NvstReceiveEvent::Dropped(reason) => event(
            "log",
            json!({ "level": "debug", "message": format!("Dropped NVST datagram: {reason:?}") }),
        ),
        NvstReceiveEvent::Frame(_) => return terminal,
    };
    let _ = output.send(value);
    terminal
}

fn forward_nvst_media_feedback(
    output: &Sender<Value>,
    lifecycle: &Mutex<Lifecycle>,
    generation: u64,
    feedback: MediaFeedback,
    dropped: &mut usize,
    last_drop_report: &mut Instant,
) {
    if lock_lifecycle(lifecycle).generation != generation {
        return;
    }
    match feedback {
        MediaFeedback::PlaybackStarted { backend } => {
            let _ = output.send(event(
                "log",
                json!({
                    "level": "info",
                    "message": format!("{backend} presented the first video frame")
                }),
            ));
        }
        MediaFeedback::BackendFallback { from, to, reason } => {
            let _ = output.send(event(
                "log",
                json!({
                    "level": "warn",
                    "message": format!("{from} startup failed; using {to}: {reason}")
                }),
            ));
        }
        MediaFeedback::RequestKeyframe { reason, .. } => {
            let _ = output.send(event(
                "error",
                json!({
                    "code": "nvst-keyframe-request-unsupported",
                    "message": format!("NVST media path needs a keyframe ({reason}), but NVST control/NACK is not implemented")
                }),
            ));
        }
        MediaFeedback::DecoderError { codec, message } => {
            let _ = output.send(event(
                "error",
                json!({
                    "code": "media-decode-error",
                    "message": format!("{codec} decoder error: {message}")
                }),
            ));
        }
        MediaFeedback::OutputError { message } => {
            let _ = output.send(event(
                "error",
                json!({ "code": "media-output-error", "message": message }),
            ));
        }
        MediaFeedback::QueueDropped { media, count } => {
            *dropped = dropped.saturating_add(count);
            if last_drop_report.elapsed() >= Duration::from_secs(1) {
                let _ = output.send(event(
                    "log",
                    json!({
                        "level": "debug",
                        "message": format!("Low-latency {media} queues dropped {dropped} stale samples/frames")
                    }),
                ));
                *dropped = 0;
                *last_drop_report = Instant::now();
            }
        }
    }
}

fn consume_encoded_media(
    output: &Sender<Value>,
    receiver: Receiver<EncodedMediaFrame>,
    sink: MediaSink,
) {
    while let Ok(frame) = receiver.recv() {
        let codec = if frame.codec.eq_ignore_ascii_case("h264") {
            MediaCodec::H264
        } else if frame.codec.eq_ignore_ascii_case("opus") {
            MediaCodec::Opus { channels: 2 }
        } else {
            MediaCodec::Unsupported(frame.codec)
        };
        match sink.push(EncodedFrame {
            mid: frame.mid,
            codec,
            data: frame.payload,
            timestamp: frame.rtp_timestamp,
            clock_rate_hz: frame.clock_rate_hz,
            keyframe: frame.keyframe,
            contiguous: frame.contiguous,
        }) {
            PushOutcome::Unsupported => {
                let _ = output.send(event(
                    "log",
                    json!({
                        "level": "warn",
                        "message": "Dropping a frame for a codec not built into native streamer v2"
                    }),
                ));
            }
            PushOutcome::Closed => break,
            PushOutcome::Queued | PushOutcome::DroppedOldest | PushOutcome::Paused => {}
        }
    }
}

fn forward_session_events(
    output: &Sender<Value>,
    lifecycle: &Mutex<Lifecycle>,
    generation: u64,
    transport_events: Receiver<TransportEvent>,
    media_feedback: Option<Receiver<MediaFeedback>>,
    transport: TransportControl,
) {
    let mut dropped = 0;
    let mut last_drop_report = Instant::now();
    loop {
        if let Some(feedback) = media_feedback.as_ref() {
            while let Ok(feedback) = feedback.try_recv() {
                forward_media_feedback(
                    output,
                    lifecycle,
                    generation,
                    &transport,
                    feedback,
                    &mut dropped,
                    &mut last_drop_report,
                );
            }
        }
        match transport_events.recv_timeout(Duration::from_millis(5)) {
            Ok(transport_event) => {
                let disconnected = matches!(transport_event, TransportEvent::Disconnected(_));
                forward_transport_event(output, lifecycle, generation, transport_event);
                if disconnected {
                    break;
                }
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}

fn forward_media_feedback(
    output: &Sender<Value>,
    lifecycle: &Mutex<Lifecycle>,
    generation: u64,
    transport: &TransportControl,
    feedback: MediaFeedback,
    dropped: &mut usize,
    last_drop_report: &mut Instant,
) {
    if lock_lifecycle(lifecycle).generation != generation {
        return;
    }
    match feedback {
        MediaFeedback::PlaybackStarted { backend } => {
            let _ = output.send(event(
                "log",
                json!({
                    "level": "info",
                    "message": format!("{backend} presented the first video frame")
                }),
            ));
        }
        MediaFeedback::BackendFallback { from, to, reason } => {
            let _ = output.send(event(
                "log",
                json!({
                    "level": "warn",
                    "message": format!("{from} startup failed; using {to}: {reason}")
                }),
            ));
        }
        MediaFeedback::RequestKeyframe { mid, reason } => {
            let request_result = transport.request_keyframe(mid);
            let _ = output.send(event(
                "log",
                json!({
                    "level": if request_result.is_ok() { "info" } else { "warn" },
                    "message": format!("Requested a video keyframe: {reason}")
                }),
            ));
        }
        MediaFeedback::DecoderError { codec, message } => {
            let _ = output.send(event(
                "error",
                json!({
                    "code": "media-decode-error",
                    "message": format!("{codec} decoder error: {message}")
                }),
            ));
        }
        MediaFeedback::OutputError { message } => {
            let _ = output.send(event(
                "error",
                json!({ "code": "media-output-error", "message": message }),
            ));
        }
        MediaFeedback::QueueDropped { media, count } => {
            *dropped = dropped.saturating_add(count);
            if last_drop_report.elapsed() >= Duration::from_secs(1) {
                let _ = output.send(event(
                    "log",
                    json!({
                        "level": "debug",
                        "message": format!(
                            "Low-latency {media} queues dropped {dropped} stale samples/frames"
                        )
                    }),
                ));
                *dropped = 0;
                *last_drop_report = Instant::now();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::UdpSocket;
    use std::time::Instant;
    use str0m::media::{Direction, MediaKind};
    use str0m::{Candidate, RtcConfig};

    fn command(value: Value) -> Command {
        serde_json::from_value(value).expect("command")
    }

    fn synthetic_context(session_id: &str, ice_servers: Value) -> Value {
        json!({
            "session": {
                "sessionId": session_id,
                "serverIp": "127-0-0-1.synthetic.invalid",
                "iceServers": ice_servers,
                "mediaConnectionInfo": {
                    "ip": "127-0-0-1.media.synthetic.invalid",
                    "port": 18_784,
                    "usage": 17
                },
                "syntheticExtension": "preserved"
            },
            "settings": { "codec": "H264", "fps": 60 },
            "shortcuts": { "stopStream": "Ctrl+Shift+Q" },
            "syntheticContextExtension": true
        })
    }

    fn synthetic_offer() -> String {
        opennow_streamer_transport::install_crypto();
        let mut offerer = RtcConfig::new().build(Instant::now());
        offerer.add_local_candidate(
            Candidate::host("127.0.0.1:49152".parse().expect("candidate address"), "udp")
                .expect("local candidate"),
        );
        let mut change = offerer.sdp_api();
        change.add_media(MediaKind::Video, Direction::SendOnly, None, None, None);
        let (offer, _pending) = change.apply().expect("synthetic offer");
        offer.to_sdp_string()
    }

    fn lifecycle_state(engine: &Engine) -> State {
        lock_lifecycle(&engine.lifecycle).state
    }

    fn unused_udp_port() -> u16 {
        let socket = UdpSocket::bind("127.0.0.1:0").expect("ephemeral UDP port");
        let port = socket.local_addr().expect("socket address").port();
        drop(socket);
        port
    }

    #[test]
    fn hello_reports_honest_transport_only_capabilities() {
        let (sender, _receiver) = std::sync::mpsc::channel();
        let mut engine = Engine::new(sender);
        let command = command(json!({
            "id": "hello",
            "type": "hello",
            "protocolVersion": PROTOCOL_VERSION,
        }));
        let (responses, _) = engine.handle(command);
        assert_eq!(responses[0]["type"], "ready");
        assert_eq!(responses[0]["capabilities"]["supportsOfferAnswer"], false);
        assert_eq!(responses[0]["capabilities"]["supportsVideoPresent"], false);
    }

    #[test]
    fn extracts_partial_reliable_threshold() {
        assert_eq!(
            partial_reliable_threshold("v=0\r\na=ri.partialReliableThresholdMs:250\r\n"),
            Some(250),
        );
    }

    #[test]
    fn start_validates_stores_context_and_prepares_session() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut engine = Engine::new(sender);
        let context = synthetic_context("synthetic-session", json!([]));
        let start = command(json!({
            "id": "start",
            "type": "start",
            "context": context,
        }));
        let (responses, _) = engine.handle(start);

        assert_eq!(responses[0]["type"], "ok");
        assert_eq!(responses[0]["transport"], "webrtc");
        let lifecycle = lock_lifecycle(&engine.lifecycle);
        assert_eq!(lifecycle.state, State::Prepared);
        let stored = serde_json::to_value(lifecycle.context.as_ref().expect("stored context"))
            .expect("serializable stored context");
        assert_eq!(stored["session"]["sessionId"], "synthetic-session");
        assert_eq!(stored["session"]["syntheticExtension"], "preserved");
        assert_eq!(stored["syntheticContextExtension"], true);
        drop(lifecycle);
        let status = receiver.recv().expect("ready status");
        assert_eq!(status["status"], "ready");
    }

    #[test]
    fn valid_nvst_handoff_starts_udp_video_and_bypasses_webrtc_offer_negotiation() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let (media_sender, _media_receiver) = std::sync::mpsc::sync_channel(4);
        let mut engine = Engine::with_media_consumer(sender, media_sender);
        let mut context = synthetic_context("nvst-session", json!([]));
        context["nvstVideo"] = json!({
            "clientUdpPort": unused_udp_port(),
            "videoPeerIp": "127.0.0.1",
            "videoPeerPort": 5004,
            "srtpAesKeyHex": "000102030405060708090A0B0C0D0E0F101112131415161718191A1B1C1D1E1F",
            "srtpSaltHex": "00000000000000009ECA935E",
            "codec": "H264"
        });
        let (responses, _) = engine.handle(command(json!({
            "id": "start",
            "type": "start",
            "context": context.clone(),
        })));

        assert_eq!(responses[0]["type"], "ok");
        assert_eq!(responses[0]["transport"], "nvst");
        assert_eq!(lifecycle_state(&engine), State::Connected);
        assert!(engine.nvst_transport.is_some());
        assert!(engine.transport.is_none());
        assert!(receiver.try_iter().any(|message| {
            message["type"] == "status"
                && message["message"]
                    .as_str()
                    .is_some_and(|text| text.contains("NVST"))
        }));

        let (responses, _) = engine.handle(command(json!({
            "id": "offer",
            "type": "offer",
            "context": context,
            "sdp": synthetic_offer(),
        })));
        assert_eq!(responses[0]["code"], "nvst-video-active");

        let (responses, _) = engine.handle(command(json!({
            "id": "stop",
            "type": "stop",
            "reason": "test complete",
        })));
        assert_eq!(responses[0]["type"], "ok");
        assert_eq!(lifecycle_state(&engine), State::Idle);
    }

    #[test]
    fn start_rejects_invalid_and_duplicate_sessions() {
        let (sender, _receiver) = std::sync::mpsc::channel();
        let mut engine = Engine::new(sender);
        let invalid = command(json!({
            "id": "invalid",
            "type": "start",
            "context": {
                "session": { "sessionId": "", "serverIp": "host", "iceServers": [] },
                "settings": {},
                "shortcuts": {}
            }
        }));
        let (responses, _) = engine.handle(invalid);
        assert_eq!(responses[0]["code"], "invalid-context");
        assert_eq!(lifecycle_state(&engine), State::Idle);

        for id in ["first", "duplicate"] {
            let start = command(json!({
                "id": id,
                "type": "start",
                "context": synthetic_context("synthetic-session", json!([])),
            }));
            let (responses, _) = engine.handle(start);
            if id == "first" {
                assert_eq!(responses[0]["type"], "ok");
            } else {
                assert_eq!(responses[0]["code"], "invalid-state");
            }
        }
    }

    #[test]
    fn offer_negotiates_directly_with_configured_ice_services() {
        let (sender, _receiver) = std::sync::mpsc::channel();
        let (media_sender, _media_receiver) = std::sync::mpsc::sync_channel(4);
        let mut engine = Engine::with_media_consumer(sender, media_sender);
        let context = synthetic_context(
            "synthetic-session",
            json!([{
                "urls": ["stun:stun.synthetic.invalid:3478", "turn:turn.synthetic.invalid:3478"],
                "username": "synthetic-user",
                "credential": "synthetic-credential"
            }]),
        );
        let (responses, _) = engine.handle(command(json!({
            "id": "start",
            "type": "start",
            "context": context.clone(),
        })));
        assert_eq!(responses[0]["type"], "ok");

        let (responses, _) = engine.handle(command(json!({
            "id": "offer",
            "type": "offer",
            "context": context,
            "sdp": synthetic_offer()
        })));
        assert_eq!(responses[0]["type"], "answer");
        assert!(
            responses[0]["answer"]["sdp"]
                .as_str()
                .is_some_and(|sdp| sdp.contains("m=video") && !sdp.contains("m=video 0"))
        );
        assert_eq!(lifecycle_state(&engine), State::Negotiating);
    }

    #[test]
    fn offer_fails_typed_when_no_in_process_media_consumer_exists() {
        let (sender, _receiver) = std::sync::mpsc::channel();
        let mut engine = Engine::new(sender);
        let context = synthetic_context("synthetic-session", json!([]));
        let (responses, _) = engine.handle(command(json!({
            "id": "start",
            "type": "start",
            "context": context.clone(),
        })));
        assert_eq!(responses[0]["type"], "ok");

        let (responses, _) = engine.handle(command(json!({
            "id": "offer",
            "type": "offer",
            "context": context,
            "sdp": "v=0\r\n"
        })));

        assert_eq!(responses[0]["code"], "media-consumer-unavailable");
        assert_eq!(lifecycle_state(&engine), State::Prepared);
    }

    #[test]
    fn prepared_session_negotiates_synthetic_offer_for_typed_media_consumer() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let (media_sender, _media_receiver) = std::sync::mpsc::sync_channel(4);
        let mut engine = Engine::with_media_consumer(sender, media_sender);
        let context = synthetic_context("synthetic-session", json!([]));
        let (responses, _) = engine.handle(command(json!({
            "id": "start",
            "type": "start",
            "context": context.clone(),
        })));
        assert_eq!(responses[0]["type"], "ok");

        let (responses, _) = engine.handle(command(json!({
            "id": "offer",
            "type": "offer",
            "context": context,
            "sdp": synthetic_offer()
        })));

        assert_eq!(responses[0]["type"], "answer");
        assert!(
            responses[0]["answer"]["sdp"]
                .as_str()
                .is_some_and(|sdp| sdp.contains("m=video") && !sdp.contains("m=video 0"))
        );
        assert_eq!(lifecycle_state(&engine), State::Negotiating);
        assert!(
            receiver
                .try_iter()
                .any(|value| value["type"] == "local-ice")
        );
    }

    #[test]
    fn disconnect_clears_context_and_stale_disconnect_cannot_clear_new_session() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let mut engine = Engine::new(sender.clone());
        let (responses, _) = engine.handle(command(json!({
            "id": "first",
            "type": "start",
            "context": synthetic_context("first-session", json!([])),
        })));
        assert_eq!(responses[0]["type"], "ok");
        let first_generation = lock_lifecycle(&engine.lifecycle).generation;
        forward_transport_event(
            &sender,
            &engine.lifecycle,
            first_generation,
            TransportEvent::Disconnected("synthetic disconnect".to_owned()),
        );
        {
            let lifecycle = lock_lifecycle(&engine.lifecycle);
            assert_eq!(lifecycle.state, State::Idle);
            assert!(lifecycle.context.is_none());
        }

        let (responses, _) = engine.handle(command(json!({
            "id": "second",
            "type": "start",
            "context": synthetic_context("second-session", json!([])),
        })));
        assert_eq!(responses[0]["type"], "ok");
        forward_transport_event(
            &sender,
            &engine.lifecycle,
            first_generation,
            TransportEvent::Disconnected("late stale disconnect".to_owned()),
        );
        let lifecycle = lock_lifecycle(&engine.lifecycle);
        assert_eq!(lifecycle.state, State::Prepared);
        assert_eq!(
            lifecycle
                .context
                .as_ref()
                .map(|value| value.session.session_id.as_str()),
            Some("second-session")
        );
        drop(lifecycle);

        let events = receiver.try_iter().collect::<Vec<_>>();
        assert!(events.iter().any(|value| value["status"] == "stopped"));
        assert!(
            !events
                .iter()
                .any(|value| value["message"] == "late stale disconnect")
        );
    }

    #[test]
    fn encoded_media_consumer_is_typed_in_process_and_preserves_arc_payload() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let (media_sender, media_receiver) = std::sync::mpsc::sync_channel(4);
        let mut engine = Engine::with_media_consumer(sender, media_sender);
        let (responses, _) = engine.handle(command(json!({
            "id": "start",
            "type": "start",
            "context": synthetic_context("synthetic-session", json!([])),
        })));
        assert_eq!(responses[0]["type"], "ok");
        let payload: Arc<[u8]> = Arc::from([1_u8, 2, 3]);
        engine
            .media_consumer
            .as_ref()
            .expect("media consumer")
            .send(EncodedMediaFrame {
                mid: "video-0".to_owned(),
                codec: "H264".to_owned(),
                payload: payload.clone(),
                rtp_timestamp: 90_000,
                clock_rate_hz: 90_000,
                received_at_us: 1_500,
                keyframe: true,
                contiguous: true,
            })
            .expect("frame delivery");

        let frame = media_receiver.recv().expect("encoded frame");
        assert!(Arc::ptr_eq(&frame.payload, &payload));
        assert_eq!(frame.rtp_timestamp, 90_000);
        assert_eq!(frame.clock_rate_hz, 90_000);
        assert_eq!(frame.received_at_us, 1_500);
        assert!(
            receiver
                .try_iter()
                .all(|value| value["type"] != "encoded-media")
        );
    }

    #[test]
    fn unapplied_commands_are_rejected_and_stop_clears_context() {
        let (sender, _receiver) = std::sync::mpsc::channel();
        let mut engine = Engine::new(sender);
        let (responses, _) = engine.handle(command(json!({
            "id": "surface",
            "type": "surface",
            "surface": {}
        })));
        assert_eq!(responses[0]["code"], "unsupported-command");

        let (responses, _) = engine.handle(command(json!({
            "id": "start",
            "type": "start",
            "context": synthetic_context("synthetic-session", json!([])),
        })));
        assert_eq!(responses[0]["type"], "ok");
        let (responses, _) = engine.handle(command(json!({
            "id": "stop",
            "type": "stop",
            "reason": "synthetic test complete"
        })));
        assert_eq!(responses[0]["type"], "ok");
        let lifecycle = lock_lifecycle(&engine.lifecycle);
        assert_eq!(lifecycle.state, State::Idle);
        assert!(lifecycle.context.is_none());
    }
}
