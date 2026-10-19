//! Guest pipeline: one link to the host, forwarded NVST RTP through the normal reorder,
//! assembly and media-consumer path, NACK and keyframe feedback, and local gamepad
//! snapshots sent on 'coop-input'.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError, TrySendError, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use opennow_streamer_transport::{
    MediaConsumer, NvstAudioTrack, NvstCoopGuestReceiver, NvstVideoCodec,
};
use serde_json::{Value, json};
use str0m::rtp::Ssrc;

use crate::config::IceConfig;
use crate::input::{GamepadState, InputRecord, InputSamples};
use crate::link::{AUDIO_PT, LinkEvent, NetConfig, Path, PeerLink, ResolvedIce, VIDEO_PT};
use crate::protocol::{ControlMessage, Signal, StreamDescription};
use crate::ring::{KEYFRAME_INTERVAL, LossCounters, NackTracker, encode_nack};
use crate::{CoopError, EventSink, bounded, log};

const COMMAND_CAPACITY: usize = 32;
const LOOP_WAIT: Duration = Duration::from_millis(1);
const STATS_INTERVAL: Duration = Duration::from_secs(1);
const INPUT_RESEND_INTERVAL: Duration = Duration::from_millis(50);
const INPUT_LOG_INTERVAL: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
/// Keyframe requests a guest sends while its decoder has not presented a frame; one per
/// KEYFRAME_INTERVAL, the first as soon as the decoder starts.
const MAX_FIRST_FRAME_KEYFRAME_REQUESTS: u32 = 5;
const RECONNECT_TIMEOUT: Duration = Duration::from_secs(15);

/// The local media pipeline a guest feeds. Implemented by the engine with the normal
/// embedded decode, presentation and audio path.
pub trait GuestMedia: Send + 'static {
    /// Starts decode and audio for the described stream and returns its bounded consumer.
    fn start(&mut self, description: &StreamDescription) -> Result<MediaConsumer, String>;
    /// The latest local controller snapshot submitted since the previous call.
    fn take_gamepad(&mut self) -> Option<GamepadState>;
    /// Totals of the local controller samples 'take_gamepad' has consumed, by source.
    fn input_samples(&self) -> InputSamples;
    /// True when the decoder asked for a reference frame since the previous call.
    fn take_keyframe_request(&mut self) -> bool;
    /// Some once, after the first decoded video frame was presented: which decoder made it.
    fn take_first_frame(&mut self) -> Option<GuestDecoder>;
    fn stop(&mut self);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuestDecoder {
    Hardware,
    Software,
}

impl GuestDecoder {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Hardware => "hardware",
            Self::Software => "software",
        }
    }
}

#[derive(Debug, Clone)]
pub struct GuestConfig {
    pub ice: IceConfig,
    pub net: NetConfig,
    pub host_peer_id: String,
    pub slot: u8,
}

pub struct CoopGuest {
    host_peer_id: String,
    commands: Option<SyncSender<Signal>>,
    worker: Option<JoinHandle<()>>,
}

impl CoopGuest {
    pub fn start(
        config: GuestConfig,
        media: Box<dyn GuestMedia>,
        events: EventSink,
    ) -> Result<Self, CoopError> {
        if config.host_peer_id.is_empty() || config.host_peer_id.len() > 128 {
            return Err(CoopError::Invalid(
                "hostPeerId must be 1 to 128 bytes".to_owned(),
            ));
        }
        if !(1..=3).contains(&config.slot) {
            return Err(CoopError::Invalid("slot must be 1, 2 or 3".to_owned()));
        }
        let host_peer_id = config.host_peer_id.clone();
        let (commands, receiver) = sync_channel(COMMAND_CAPACITY);
        let worker = std::thread::Builder::new()
            .name("opennow-coop-guest".to_owned())
            .spawn(move || run(config, media, events, &receiver))
            .map_err(|error| CoopError::Invalid(error.to_string()))?;
        Ok(Self {
            host_peer_id,
            commands: Some(commands),
            worker: Some(worker),
        })
    }

    pub fn signal(&self, peer_id: &str, payload: &Value) -> Result<(), CoopError> {
        if peer_id != self.host_peer_id {
            return Err(CoopError::Invalid(
                "signal is not from the co-op host".to_owned(),
            ));
        }
        let signal = Signal::parse(payload)
            .ok_or_else(|| CoopError::Invalid("payload is not a co-op signal".to_owned()))?;
        let commands = self.commands.as_ref().ok_or(CoopError::Closed)?;
        commands.try_send(signal).map_err(|error| match error {
            TrySendError::Full(_) => CoopError::Busy,
            TrySendError::Disconnected(_) => CoopError::Closed,
        })
    }

    /// Leaves the host and stops local media; joins the worker.
    pub fn stop(&mut self) {
        self.commands = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for CoopGuest {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GuestState {
    Negotiating,
    TryingDirect,
    /// No direct pair connected in time; TURN allocations are being tried.
    TryingRelay,
    /// The nominated pair is relayed.
    DirectBlocked,
    Relay,
    Connected,
    /// Connected and the first decoded frame was presented.
    Streaming,
    Reconnecting,
    Ended,
}

impl GuestState {
    const fn label(self) -> &'static str {
        match self {
            Self::Negotiating => "negotiating",
            Self::TryingDirect => "trying_direct",
            Self::TryingRelay => "trying_relay",
            Self::DirectBlocked => "direct_blocked",
            Self::Relay => "relay",
            Self::Connected => "connected",
            Self::Streaming => "streaming",
            Self::Reconnecting => "reconnecting",
            Self::Ended => "ended",
        }
    }
}

fn video_codec(label: &str) -> Option<NvstVideoCodec> {
    match label.to_ascii_uppercase().as_str() {
        "H264" | "AVC" => Some(NvstVideoCodec::H264),
        "H265" | "HEVC" => Some(NvstVideoCodec::H265),
        "AV1" => Some(NvstVideoCodec::Av1),
        _ => None,
    }
}

/// "great" under 60 ms and 1 % loss, "good" under 120 ms and 5 %, otherwise "poor".
fn quality(total_ms: Option<f64>, loss_pct: f64) -> &'static str {
    match total_ms {
        Some(total) if total < 60.0 && loss_pct < 1.0 => "great",
        Some(total) if total < 120.0 && loss_pct < 5.0 => "good",
        _ => "poor",
    }
}

struct GuestWorker {
    link: PeerLink,
    media: Box<dyn GuestMedia>,
    events: EventSink,
    host_peer_id: String,
    state: GuestState,
    state_since: Instant,
    origin: Instant,
    receiver: Option<NvstCoopGuestReceiver>,
    nack: NackTracker,
    video_ssrc: Option<Ssrc>,
    keyframe_sent: Option<Instant>,
    slot: u8,
    input_muted: bool,
    host_to_gfn_ping_ms: Option<f64>,
    input_seq: u32,
    input_state: GamepadState,
    input_sent: Option<Instant>,
    input_states_sent: u64,
    input_log_at: Instant,
    loss_at_stats: LossCounters,
    stats_at: Instant,
    connected_at: Option<Instant>,
    media_started_at: Option<Instant>,
    first_keyframe_at: Option<Instant>,
    first_frame_at: Option<Instant>,
    first_frame_keyframe_requests: u32,
    codec: Option<String>,
    decoder: Option<GuestDecoder>,
}

fn run(
    config: GuestConfig,
    media: Box<dyn GuestMedia>,
    events: EventSink,
    commands: &Receiver<Signal>,
) {
    let now = Instant::now();
    let ice = Arc::new(ResolvedIce::resolve(&config.ice));
    let link = match PeerLink::new(ice, config.net, now) {
        Ok(link) => link,
        Err(error) => {
            events(json!({
                "type":"coop-guest-state","state":"ended","path":null,"relayId":null,
                "reason":format!("co-op sockets: {error}"),
            }));
            return;
        }
    };
    let mut worker = GuestWorker {
        link,
        media,
        events,
        host_peer_id: config.host_peer_id,
        state: GuestState::Negotiating,
        state_since: now,
        origin: now,
        receiver: None,
        nack: NackTracker::default(),
        video_ssrc: None,
        keyframe_sent: None,
        slot: config.slot,
        input_muted: false,
        host_to_gfn_ping_ms: None,
        input_seq: 0,
        input_state: GamepadState::default(),
        input_sent: None,
        input_states_sent: 0,
        input_log_at: now,
        loss_at_stats: LossCounters::default(),
        stats_at: now,
        connected_at: None,
        media_started_at: None,
        first_keyframe_at: None,
        first_frame_at: None,
        first_frame_keyframe_requests: 0,
        codec: None,
        decoder: None,
    };
    worker.emit_state(None);
    let reason = worker.run(commands);
    worker.finish(&reason);
}

impl GuestWorker {
    fn emit(&self, value: Value) {
        (self.events)(value);
    }

    fn emit_state(&self, reason: Option<&str>) {
        let (path, relay_id) = match self.link.path() {
            None => (Value::Null, Value::Null),
            Some(Path::Direct) => (json!("direct"), Value::Null),
            Some(Path::Relay(id)) => (json!("relay"), json!(id)),
        };
        log(&format!(
            "guest state={} path={path} relayId={relay_id} reason={}",
            self.state.label(),
            reason.map_or_else(|| "null".to_owned(), bounded),
        ));
        self.emit(json!({
            "type":"coop-guest-state",
            "state":self.state.label(),
            "path":path,
            "relayId":relay_id,
            "reason":reason,
        }));
    }

    fn set_state(&mut self, state: GuestState, now: Instant) {
        if self.state != state {
            self.state = state;
            self.state_since = now;
            self.emit_state(None);
        }
    }

    fn signal_out(&self, signal: &Signal) {
        self.emit(json!({
            "type":"coop-signal-out","peerId":self.host_peer_id,"payload":signal.to_json(),
        }));
    }

    /// Runs until the guest leaves or the link ends; returns the end reason.
    fn run(&mut self, commands: &Receiver<Signal>) -> String {
        loop {
            loop {
                match commands.try_recv() {
                    Ok(signal) => {
                        if let Err(reason) = self.handle_signal(signal) {
                            return reason;
                        }
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        self.signal_out(&Signal::Bye("left".to_owned()));
                        return "left".to_owned();
                    }
                }
            }
            let now = Instant::now();
            self.link.poll_network(now);
            self.link.drive(now);
            let events: Vec<LinkEvent> = self.link.take_events().collect();
            for event in events {
                if let Err(reason) = self.handle_link_event(event, now) {
                    return reason;
                }
            }
            if let Err(reason) = self.service(now) {
                return reason;
            }
            self.link.drive(now);
            std::thread::sleep(
                self.link
                    .next_timeout()
                    .saturating_duration_since(Instant::now())
                    .min(LOOP_WAIT),
            );
        }
    }

    fn finish(&mut self, reason: &str) {
        if reason.starts_with("media-") {
            self.signal_out(&Signal::Bye(reason.to_owned()));
        }
        self.link.close();
        self.link.drive(Instant::now());
        self.receiver = None;
        self.media.stop();
        self.state = GuestState::Ended;
        self.emit_state(Some(reason));
    }

    fn handle_signal(&mut self, signal: Signal) -> Result<(), String> {
        match signal {
            Signal::Offer(sdp) => {
                let answer = self
                    .link
                    .accept_offer(&sdp)
                    .map_err(|error| format!("invalid offer: {error}"))?;
                self.signal_out(&Signal::Answer(answer));
                self.set_state(GuestState::TryingDirect, Instant::now());
            }
            Signal::Candidate(candidate) => {
                let _ = self.link.add_remote_candidate(&candidate);
            }
            Signal::Answer(_) | Signal::EndOfCandidates => {}
            Signal::Bye(reason) => {
                return Err(if reason.is_empty() {
                    "host-left".to_owned()
                } else {
                    reason
                });
            }
        }
        Ok(())
    }

    fn handle_link_event(&mut self, event: LinkEvent, now: Instant) -> Result<(), String> {
        match event {
            LinkEvent::LocalCandidate(candidate) => self.signal_out(&Signal::Candidate(candidate)),
            LinkEvent::DirectBlocked => self.set_state(GuestState::TryingRelay, now),
            LinkEvent::RelayUnavailable {
                credentials_rejected,
            } if !self.link.is_connected() => {
                return Err(if credentials_rejected {
                    "relay-credentials-rejected"
                } else {
                    "relay-unavailable"
                }
                .to_owned());
            }
            LinkEvent::Connected => {
                if self.connected_at.is_none() {
                    self.connected_at = Some(now);
                    log(&format!(
                        "guest connected connectMs={}",
                        self.since_start(now)
                    ));
                }
                self.set_state(GuestState::Connected, now);
            }
            LinkEvent::Ice(state) => {
                use str0m::IceConnectionState::{Completed, Connected, Disconnected};
                match state {
                    Disconnected if self.is_up() => {
                        self.set_state(GuestState::Reconnecting, now);
                    }
                    Connected | Completed if self.state == GuestState::Reconnecting => {
                        self.set_state(self.up_state(), now);
                    }
                    _ => {}
                }
            }
            LinkEvent::Path(Path::Relay(_))
                if matches!(
                    self.state,
                    GuestState::Negotiating | GuestState::TryingDirect | GuestState::TryingRelay
                ) =>
            {
                self.set_state(GuestState::DirectBlocked, now);
                self.set_state(GuestState::Relay, now);
            }
            LinkEvent::Path(_) if self.is_up() => self.emit_state(None),
            LinkEvent::ChannelData(id, data) if self.link.control == Some(id) => {
                self.handle_control(&data)?;
            }
            LinkEvent::Rtp(packet) => {
                let payload = &packet.payload;
                match *packet.header.payload_type {
                    VIDEO_PT => {
                        self.video_ssrc = Some(packet.header.ssrc);
                        self.nack.observe(*packet.seq_no, now);
                        if let Some(receiver) = self.receiver.as_mut() {
                            receiver
                                .process_video(payload, now)
                                .map_err(|error| format!("media-closed: {error}"))?;
                        }
                    }
                    AUDIO_PT => {
                        if let Some(receiver) = self.receiver.as_mut() {
                            receiver
                                .process_audio(payload, now)
                                .map_err(|error| format!("media-closed: {error}"))?;
                        }
                    }
                    _ => {}
                }
            }
            LinkEvent::Closed => return Err("host-disconnected".to_owned()),
            _ => {}
        }
        Ok(())
    }

    fn handle_control(&mut self, data: &[u8]) -> Result<(), String> {
        if let Some(message) = ControlMessage::decode(data) {
            match message {
                ControlMessage::Slot { slot } => self.slot = slot,
                ControlMessage::InputMuted { muted } => self.input_muted = muted,
                ControlMessage::Stats {
                    host_to_gfn_ping_ms,
                } => {
                    self.host_to_gfn_ping_ms = host_to_gfn_ping_ms;
                }
                ControlMessage::Rumble {
                    low_frequency,
                    high_frequency,
                    duration_ms,
                } => {
                    self.emit(json!({
                        "type":"controller-rumble",
                        "controllerId":0,
                        "lowFrequency":low_frequency,
                        "highFrequency":high_frequency,
                        "durationMs":duration_ms,
                    }));
                }
                ControlMessage::KeyframeRequest => {}
            }
            return Ok(());
        }
        if self.receiver.is_some() || data.len() > crate::protocol::MAX_CONTROL_MESSAGE_BYTES {
            return Ok(());
        }
        let Ok(description) = serde_json::from_slice::<StreamDescription>(data) else {
            return Ok(());
        };
        log(&format!(
            "guest stream description codec={} size={}x{} fps={} bitDepth={} chroma={} colorSpace={} hdr={} audio={}",
            bounded(&description.codec),
            description.width,
            description.height,
            description.fps,
            description.bit_depth,
            bounded(&description.chroma),
            description
                .color_space
                .as_deref()
                .map_or("null", |space| space),
            description.hdr,
            description.audio.as_ref().map_or_else(
                || "none".to_owned(),
                |audio| format!(
                    "{}/{}ch/{}Hz/pt{}",
                    bounded(&audio.codec),
                    audio.channels,
                    audio.sample_rate,
                    audio.payload_type
                )
            ),
        ));
        let codec = video_codec(&description.codec).ok_or_else(|| {
            format!(
                "media-unavailable: unsupported co-op codec {}",
                description.codec
            )
        })?;
        let audio_track = description.audio.as_ref().map(|audio| NvstAudioTrack {
            payload_type: audio.payload_type,
            clock_rate_hz: audio.sample_rate,
            channels: audio.channels,
            mid: "coop-audio".to_owned(),
            ssrc: None,
        });
        let consumer = match self.media.start(&description) {
            Ok(consumer) => consumer,
            Err(error) => {
                log(&format!("guest media start failed: {}", bounded(&error)));
                return Err(format!("media-unavailable: {error}"));
            }
        };
        let now = Instant::now();
        log(&format!(
            "guest media start ok mediaStartMs={}",
            self.since_start(now)
        ));
        self.media_started_at = Some(now);
        self.codec = Some(description.codec.clone());
        self.receiver = Some(NvstCoopGuestReceiver::new(codec, audio_track, consumer));
        Ok(())
    }

    fn since_start(&self, now: Instant) -> u128 {
        now.saturating_duration_since(self.origin).as_millis()
    }

    fn is_up(&self) -> bool {
        matches!(self.state, GuestState::Connected | GuestState::Streaming)
    }

    fn up_state(&self) -> GuestState {
        if self.first_frame_at.is_some() {
            GuestState::Streaming
        } else {
            GuestState::Connected
        }
    }

    /// Records the first assembled keyframe and the first presented frame, and moves to
    /// 'streaming' once a frame is on screen.
    fn track_first_frame(&mut self, now: Instant) {
        if self.first_keyframe_at.is_none()
            && self
                .receiver
                .as_ref()
                .is_some_and(|receiver| receiver.stats().keyframes_delivered > 0)
        {
            self.first_keyframe_at = Some(now);
        }
        if self.first_frame_at.is_some() {
            return;
        }
        let Some(decoder) = self.media.take_first_frame() else {
            return;
        };
        self.first_frame_at = Some(now);
        self.decoder = Some(decoder);
        let stats = self.receiver.as_ref().map(NvstCoopGuestReceiver::stats);
        let ms = |at: Option<Instant>| {
            at.map_or_else(|| "null".to_owned(), |at| self.since_start(at).to_string())
        };
        log(&format!(
            "guest first frame firstFrameMs={} connectMs={} mediaStartMs={} firstKeyframeMs={} keyframeRequests={} framesDelivered={} keyframesDelivered={} codec={} decoder={}",
            self.since_start(now),
            ms(self.connected_at),
            ms(self.media_started_at),
            ms(self.first_keyframe_at),
            self.first_frame_keyframe_requests,
            stats.map_or(0, |stats| stats.frames_delivered),
            stats.map_or(0, |stats| stats.keyframes_delivered),
            self.codec
                .as_deref()
                .map_or_else(|| "null".to_owned(), bounded),
            decoder.label(),
        ));
        if self.is_up() {
            self.set_state(GuestState::Streaming, now);
        }
    }

    fn service(&mut self, now: Instant) -> Result<(), String> {
        let timeout = match self.state {
            GuestState::Connected | GuestState::Streaming => None,
            GuestState::Reconnecting => Some(RECONNECT_TIMEOUT),
            _ => Some(CONNECT_TIMEOUT),
        };
        if timeout.is_some_and(|timeout| {
            now.saturating_duration_since(if self.state == GuestState::Reconnecting {
                self.state_since
            } else {
                self.origin
            }) >= timeout
        }) {
            return Err("connect-timeout".to_owned());
        }
        if let Some(receiver) = self.receiver.as_mut() {
            receiver
                .poll(now)
                .map_err(|error| format!("media-closed: {error}"))?;
        }
        if let Some(ssrc) = self.video_ssrc {
            let missing = self.nack.poll(now, self.link.rtt());
            if let Some(payload) = encode_nack(&missing) {
                self.link.send_app_feedback(ssrc, payload);
            }
        }
        self.track_first_frame(now);
        let awaiting_first_frame = self.media_started_at.is_some()
            && self.first_frame_at.is_none()
            && self.first_frame_keyframe_requests < MAX_FIRST_FRAME_KEYFRAME_REQUESTS;
        let keyframe_needed = self.media.take_keyframe_request()
            || awaiting_first_frame
            || self
                .receiver
                .as_ref()
                .is_some_and(NvstCoopGuestReceiver::keyframe_needed);
        if keyframe_needed
            && self.link.is_connected()
            && self
                .keyframe_sent
                .is_none_or(|sent| now.saturating_duration_since(sent) >= KEYFRAME_INTERVAL)
        {
            let control = self.link.control;
            if self
                .link
                .send_channel(control, false, &ControlMessage::KeyframeRequest.encode())
            {
                self.keyframe_sent = Some(now);
                if awaiting_first_frame {
                    self.first_frame_keyframe_requests += 1;
                }
            }
        }
        self.send_input(now);
        if now.saturating_duration_since(self.stats_at) >= STATS_INTERVAL {
            self.emit_stats(now);
        }
        Ok(())
    }

    fn send_input(&mut self, now: Instant) {
        let latest = self.media.take_gamepad();
        let changed = latest.is_some_and(|state| state != self.input_state);
        if let Some(state) = latest {
            self.input_state = state;
        }
        let due = self
            .input_sent
            .is_none_or(|sent| now.saturating_duration_since(sent) >= INPUT_RESEND_INTERVAL);
        if !changed && !due {
            return;
        }
        let input = self.link.input;
        if input.is_none() {
            return;
        }
        self.input_seq = self.input_seq.wrapping_add(1);
        let record = InputRecord {
            seq: self.input_seq,
            state: self.input_state,
            ts_ms: u32::try_from(now.saturating_duration_since(self.origin).as_millis())
                .unwrap_or(u32::MAX),
        };
        if self.link.send_channel(input, true, &record.encode()) {
            self.input_sent = Some(now);
            self.input_states_sent += 1;
        }
    }

    fn emit_stats(&mut self, now: Instant) {
        self.stats_at = now;
        let counters = self.nack.counters();
        let received = counters
            .received
            .saturating_sub(self.loss_at_stats.received);
        let lost = counters.lost.saturating_sub(self.loss_at_stats.lost);
        self.loss_at_stats = counters;
        let loss_pct = if received + lost == 0 {
            0.0
        } else {
            lost as f64 * 100.0 / (received + lost) as f64
        };
        let guest_to_host = self.link.rtt().map(|rtt| rtt.as_secs_f64() * 1_000.0);
        let total = guest_to_host
            .zip(self.host_to_gfn_ping_ms)
            .map(|(a, b)| a + b);
        let samples = self.media.input_samples();
        self.emit(json!({
            "type":"coop-guest-stats",
            "hostToGfnPingMs":self.host_to_gfn_ping_ms,
            "guestToHostPingMs":guest_to_host,
            "totalMs":total,
            "quality":quality(total, loss_pct),
            "lossPct":loss_pct,
            "slot":self.slot,
            "inputMuted":self.input_muted,
            "inputSamplesPlain":samples.plain,
            "inputSamplesSony":samples.sony,
            "inputStatesSent":self.input_states_sent,
            "connectMs":self.connected_at.map(|at| self.since_start(at) as u64),
            "firstFrameMs":self.first_frame_at.map(|at| self.since_start(at) as u64),
            "codec":self.codec,
            "decoder":self.decoder.map(GuestDecoder::label),
        }));
        if now.saturating_duration_since(self.input_log_at) >= INPUT_LOG_INTERVAL {
            self.input_log_at = now;
            opennow_streamer_protocol::log::log_line(
                "INFO",
                "coop",
                &format!(
                    "guest input slot={} samples plain={} sony={} statesSent={} muted={}",
                    self.slot,
                    samples.plain,
                    samples.sony,
                    self.input_states_sent,
                    self.input_muted
                ),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_thresholds() {
        assert_eq!(quality(Some(40.0), 0.0), "great");
        assert_eq!(quality(Some(40.0), 2.0), "good");
        assert_eq!(quality(Some(100.0), 0.0), "good");
        assert_eq!(quality(Some(150.0), 0.0), "poor");
        assert_eq!(quality(None, 0.0), "poor");
    }

    #[test]
    fn codec_labels_match_nvst() {
        assert_eq!(video_codec("H264"), Some(NvstVideoCodec::H264));
        assert_eq!(video_codec("hevc"), Some(NvstVideoCodec::H265));
        assert_eq!(video_codec("AV1"), Some(NvstVideoCodec::Av1));
        assert_eq!(video_codec("vp9"), None);
    }
}
