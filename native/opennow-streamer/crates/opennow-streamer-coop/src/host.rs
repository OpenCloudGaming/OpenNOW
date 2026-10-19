//! Host fan-out: forwards the decrypted NVST RTP tap to every connected guest, answers
//! guest NACKs from a per-guest ring, rate-limits keyframe requests to GFN, and injects
//! guest gamepad input into GFN controller slots 1..3.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, SyncSender, TrySendError, sync_channel};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use opennow_streamer_transport::{
    CoopMediaKind, CoopRtpPacket, coop_gamepad_record, coop_neutral_record, coop_slots_record,
};
use serde_json::{Value, json};

use crate::config::IceConfig;
use crate::input::InputRecord;
use crate::link::{AUDIO_PT, LinkEvent, NetConfig, Path, PeerLink, ResolvedIce, VIDEO_PT};
use crate::protocol::{ControlMessage, Signal, StreamDescription};
use crate::ring::{KeyframeLimiter, RetransmitRing, RingEntry, decode_nack};
use crate::slots::{InputDrop, MAX_GUESTS, SlotAction, SlotError, SlotTable};
use crate::{CoopError, EventSink, bounded, log};

const COMMAND_CAPACITY: usize = 64;
const COMMAND_REPLY_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_TAP_PACKETS_PER_TURN: usize = 512;
const LOOP_WAIT: Duration = Duration::from_millis(1);
const STATS_INTERVAL: Duration = Duration::from_secs(1);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_PEER_ID_LEN: usize = 128;
const INPUT_LOG_INTERVAL: Duration = Duration::from_secs(10);

/// What the co-op host needs from the running NVST session.
pub trait HostBackend: Send + Sync + 'static {
    fn request_keyframe(&self);
    /// Queues one co-op input record on the NVST input path; false when input is not ready.
    fn send_input(&self, record: Vec<u8>) -> bool;
    fn host_to_gfn_ping_ms(&self) -> Option<f64>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rumble {
    pub low_frequency: u16,
    pub high_frequency: u16,
    pub duration_ms: u16,
}

/// Lets the NVST session loop hand rumble for guest-owned slots to the co-op host instead of
/// the local controllers. Holds at most the latest command per slot.
#[derive(Debug, Default)]
pub struct RumbleRouter {
    guest_mask: AtomicU8,
    pending: Mutex<[Option<Rumble>; 4]>,
}

impl RumbleRouter {
    /// Returns true when 'controller_id' belongs to a guest and the command was taken.
    pub fn route(&self, controller_id: u8, rumble: Rumble) -> bool {
        if controller_id > 3 || self.guest_mask.load(Ordering::Acquire) & (1 << controller_id) == 0
        {
            return false;
        }
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)[usize::from(controller_id)] =
            Some(rumble);
        true
    }

    fn set_mask(&self, mask: u8) {
        self.guest_mask.store(mask, Ordering::Release);
    }

    fn take(&self) -> [Option<Rumble>; 4] {
        std::mem::take(&mut *self.pending.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

#[derive(Debug, Clone)]
pub struct HostConfig {
    pub ice: IceConfig,
    pub max_guests: u8,
    pub net: NetConfig,
    pub stream: StreamDescription,
}

enum HostCommand {
    AddPeer { peer_id: String, slot: u8 },
    Signal { peer_id: String, signal: Signal },
    Slot { peer_id: String, slot: u8 },
    Muted { peer_id: String, muted: bool },
    Remove { peer_id: String, reason: String },
    MaxGuests(u8),
}

type Reply = SyncSender<Result<(), CoopError>>;

pub struct CoopHost {
    commands: Option<SyncSender<(HostCommand, Reply)>>,
    worker: Option<JoinHandle<()>>,
}

fn valid_peer_id(peer_id: &str) -> Result<String, CoopError> {
    if peer_id.is_empty() || peer_id.len() > MAX_PEER_ID_LEN {
        return Err(CoopError::Invalid(
            "peerId must be 1 to 128 bytes".to_owned(),
        ));
    }
    Ok(peer_id.to_owned())
}

fn valid_slot(slot: u8) -> Result<u8, CoopError> {
    (1..=3)
        .contains(&slot)
        .then_some(slot)
        .ok_or_else(|| CoopError::Invalid("slot must be 1, 2 or 3".to_owned()))
}

impl CoopHost {
    pub fn start(
        config: HostConfig,
        tap: Receiver<CoopRtpPacket>,
        backend: Arc<dyn HostBackend>,
        rumble: Arc<RumbleRouter>,
        events: EventSink,
    ) -> Result<Self, CoopError> {
        let max_guests = config.max_guests;
        if !(1..=MAX_GUESTS as u8).contains(&max_guests) {
            return Err(CoopError::Invalid("maxGuests must be 1, 2 or 3".to_owned()));
        }
        let (commands, receiver) = sync_channel(COMMAND_CAPACITY);
        let worker = std::thread::Builder::new()
            .name("opennow-coop-host".to_owned())
            .spawn(move || {
                let ice = Arc::new(ResolvedIce::resolve(&config.ice));
                HostWorker {
                    ice,
                    net: config.net,
                    stream: config.stream,
                    max_guests,
                    backend,
                    rumble,
                    events,
                    peers: Vec::new(),
                    slots: SlotTable::default(),
                    keyframes: KeyframeLimiter::default(),
                    origin: Instant::now(),
                    video_seq: None,
                    audio_seq: None,
                    stream_bytes: 0,
                    stats_at: Instant::now(),
                    input_log_at: Instant::now(),
                    registered: None,
                }
                .run(&receiver, &tap);
            })
            .map_err(|error| CoopError::Invalid(error.to_string()))?;
        Ok(Self {
            commands: Some(commands),
            worker: Some(worker),
        })
    }

    fn request(&self, command: HostCommand) -> Result<(), CoopError> {
        let commands = self.commands.as_ref().ok_or(CoopError::Closed)?;
        let (reply, response) = sync_channel(1);
        commands
            .try_send((command, reply))
            .map_err(|error| match error {
                TrySendError::Full(_) => CoopError::Busy,
                TrySendError::Disconnected(_) => CoopError::Closed,
            })?;
        response
            .recv_timeout(COMMAND_REPLY_TIMEOUT)
            .map_err(|_| CoopError::Closed)?
    }

    pub fn set_max_guests(&self, max_guests: u8) -> Result<(), CoopError> {
        if !(1..=MAX_GUESTS as u8).contains(&max_guests) {
            return Err(CoopError::Invalid("maxGuests must be 1, 2 or 3".to_owned()));
        }
        self.request(HostCommand::MaxGuests(max_guests))
    }

    pub fn add_peer(&self, peer_id: &str, slot: u8) -> Result<(), CoopError> {
        self.request(HostCommand::AddPeer {
            peer_id: valid_peer_id(peer_id)?,
            slot: valid_slot(slot)?,
        })
    }

    pub fn signal(&self, peer_id: &str, payload: &Value) -> Result<(), CoopError> {
        let signal = Signal::parse(payload)
            .ok_or_else(|| CoopError::Invalid("payload is not a co-op signal".to_owned()))?;
        self.request(HostCommand::Signal {
            peer_id: valid_peer_id(peer_id)?,
            signal,
        })
    }

    pub fn set_slot(&self, peer_id: &str, slot: u8) -> Result<(), CoopError> {
        self.request(HostCommand::Slot {
            peer_id: valid_peer_id(peer_id)?,
            slot: valid_slot(slot)?,
        })
    }

    pub fn set_muted(&self, peer_id: &str, muted: bool) -> Result<(), CoopError> {
        self.request(HostCommand::Muted {
            peer_id: valid_peer_id(peer_id)?,
            muted,
        })
    }

    pub fn remove_peer(&self, peer_id: &str, reason: &str) -> Result<(), CoopError> {
        self.request(HostCommand::Remove {
            peer_id: valid_peer_id(peer_id)?,
            reason: reason.chars().take(256).collect(),
        })
    }

    /// Removes every guest with neutral input and joins the worker.
    pub fn stop(&mut self) {
        self.commands = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for CoopHost {
    fn drop(&mut self) {
        self.stop();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PeerState {
    Connecting,
    Connected,
    Reconnecting,
    Disconnected,
    Failed,
}

impl PeerState {
    const fn label(self) -> &'static str {
        match self {
            Self::Connecting => "connecting",
            Self::Connected => "connected",
            Self::Reconnecting => "reconnecting",
            Self::Disconnected => "disconnected",
            Self::Failed => "failed",
        }
    }
}

struct HostPeer {
    id: String,
    link: PeerLink,
    ring: RetransmitRing,
    state: PeerState,
    created_at: Instant,
    sent_bytes_at_stats: u64,
    send_kbps: f64,
    /// Host-extended sequence where this peer's video numbering starts. A peer's SRTP
    /// context starts at rollover counter zero, so each peer counts from the cycle it joined
    /// in while the low 16 bits stay the sender's wire sequence.
    video_base: Option<u64>,
    audio_base: Option<u64>,
    input: PeerInput,
}

/// What became of the input records one guest sent, from receipt to the NVST input path.
#[derive(Debug, Default, Clone, Copy)]
struct PeerInput {
    received: u64,
    injected: u64,
    muted: u64,
    slot_not_owned: u64,
    malformed: u64,
    stale: u64,
    nvst_refused: u64,
}

fn path_fields(path: Option<&Path>) -> (Value, Value) {
    match path {
        None => (Value::Null, Value::Null),
        Some(Path::Direct) => (json!("direct"), Value::Null),
        Some(Path::Relay(id)) => (json!("relay"), json!(id)),
    }
}

/// Extends a 16-bit RTP sequence to the value closest to 'last'. This is the host's own
/// numbering; each peer rebases it to the cycle it joined in.
fn extend_seq(last: &mut Option<u64>, wire: u16) -> u64 {
    let extended = match *last {
        None => u64::from(wire),
        Some(previous) => {
            let delta = i64::from(wire.wrapping_sub(previous as u16) as i16);
            previous.saturating_add_signed(delta)
        }
    };
    if last.is_none_or(|previous| extended > previous) {
        *last = Some(extended);
    }
    extended
}

struct HostWorker {
    ice: Arc<ResolvedIce>,
    net: NetConfig,
    stream: StreamDescription,
    max_guests: u8,
    backend: Arc<dyn HostBackend>,
    rumble: Arc<RumbleRouter>,
    events: EventSink,
    peers: Vec<HostPeer>,
    slots: SlotTable,
    keyframes: KeyframeLimiter,
    origin: Instant,
    video_seq: Option<u64>,
    audio_seq: Option<u64>,
    stream_bytes: u64,
    stats_at: Instant,
    input_log_at: Instant,
    /// Guest slot mask the NVST input path last accepted; re-sent until it matches.
    registered: Option<u8>,
}

impl HostWorker {
    fn run(mut self, commands: &Receiver<(HostCommand, Reply)>, tap: &Receiver<CoopRtpPacket>) {
        loop {
            loop {
                match commands.try_recv() {
                    Ok((command, reply)) => {
                        let result = self.handle(command);
                        let _ = reply.try_send(result);
                    }
                    Err(std::sync::mpsc::TryRecvError::Empty) => break,
                    Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                        self.shutdown();
                        return;
                    }
                }
            }
            match tap.recv_timeout(LOOP_WAIT) {
                Ok(packet) => {
                    self.forward(&packet);
                    for packet in tap.try_iter().take(MAX_TAP_PACKETS_PER_TURN) {
                        self.forward(&packet);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => std::thread::sleep(LOOP_WAIT),
            }
            let now = Instant::now();
            self.service_peers(now);
            let actions = self.slots.poll(now);
            self.apply(&actions);
            let mask = self.slots.mask();
            if self.registered != Some(mask) {
                self.apply(&[SlotAction::Slots(mask)]);
            }
            if self.keyframes.poll(now) {
                self.backend.request_keyframe();
            }
            self.route_rumble();
            if now.saturating_duration_since(self.stats_at) >= STATS_INTERVAL {
                self.emit_stats(now);
            }
        }
    }

    fn emit(&self, value: Value) {
        (self.events)(value);
    }

    fn signal_out(&self, peer_id: &str, signal: &Signal) {
        self.emit(json!({"type":"coop-signal-out","peerId":peer_id,"payload":signal.to_json()}));
    }

    fn emit_state(&self, index: usize) {
        let peer = &self.peers[index];
        let (path, relay_id) = path_fields(peer.link.path());
        log(&format!(
            "host peer={index} state={} path={path} relayId={relay_id}",
            peer.state.label()
        ));
        self.emit(json!({
            "type":"coop-peer-state",
            "peerId":peer.id,
            "state":peer.state.label(),
            "path":path,
            "relayId":relay_id,
        }));
    }

    fn set_state(&mut self, index: usize, state: PeerState) {
        if self.peers[index].state != state {
            self.peers[index].state = state;
            self.emit_state(index);
        }
    }

    fn peer_index(&self, peer_id: &str) -> Result<usize, CoopError> {
        self.peers
            .iter()
            .position(|peer| peer.id == peer_id)
            .ok_or_else(|| CoopError::Invalid(format!("unknown co-op peer {peer_id}")))
    }

    fn handle(&mut self, command: HostCommand) -> Result<(), CoopError> {
        let slot_error = |error: SlotError| CoopError::Invalid(format!("{error:?}"));
        match command {
            HostCommand::MaxGuests(max_guests) => {
                self.max_guests = max_guests;
                Ok(())
            }
            HostCommand::AddPeer { peer_id, slot } => {
                if self.peers.iter().any(|peer| peer.id == peer_id) {
                    return Err(CoopError::Invalid(format!(
                        "co-op peer {peer_id} already exists"
                    )));
                }
                if self.peers.len() >= usize::from(self.max_guests) {
                    return Err(CoopError::Invalid("the lobby is full".to_owned()));
                }
                let actions = self.slots.add(&peer_id, slot).map_err(slot_error)?;
                let now = Instant::now();
                let mut link = match PeerLink::new(Arc::clone(&self.ice), self.net.clone(), now) {
                    Ok(link) => link,
                    Err(error) => {
                        let _ = self.slots.remove(&peer_id);
                        return Err(CoopError::Invalid(format!("co-op sockets: {error}")));
                    }
                };
                let Some(offer) = link.create_offer() else {
                    let _ = self.slots.remove(&peer_id);
                    return Err(CoopError::Invalid(
                        "could not create the co-op offer".to_owned(),
                    ));
                };
                self.apply(&actions);
                self.signal_out(&peer_id, &Signal::Offer(offer));
                self.peers.push(HostPeer {
                    id: peer_id,
                    link,
                    ring: RetransmitRing::default(),
                    state: PeerState::Connecting,
                    created_at: now,
                    sent_bytes_at_stats: 0,
                    send_kbps: 0.0,
                    video_base: None,
                    audio_base: None,
                    input: PeerInput::default(),
                });
                self.emit_state(self.peers.len() - 1);
                Ok(())
            }
            HostCommand::Signal { peer_id, signal } => {
                let index = self.peer_index(&peer_id)?;
                let link = &mut self.peers[index].link;
                match signal {
                    Signal::Answer(sdp) => link.accept_answer(&sdp).map_err(CoopError::Invalid),
                    Signal::Candidate(candidate) => link
                        .add_remote_candidate(&candidate)
                        .map_err(CoopError::Invalid),
                    Signal::EndOfCandidates => Ok(()),
                    Signal::Offer(_) => {
                        Err(CoopError::Invalid("the host is the offerer".to_owned()))
                    }
                    Signal::Bye(reason) => {
                        log(&format!(
                            "host peer={index} bye reason={}",
                            bounded(&reason)
                        ));
                        self.disconnect(index, PeerState::Disconnected);
                        Ok(())
                    }
                }
            }
            HostCommand::Slot { peer_id, slot } => {
                let index = self.peer_index(&peer_id)?;
                let actions = self.slots.move_to(&peer_id, slot).map_err(slot_error)?;
                self.apply(&actions);
                self.send_control(index, &ControlMessage::Slot { slot });
                Ok(())
            }
            HostCommand::Muted { peer_id, muted } => {
                let index = self.peer_index(&peer_id)?;
                let actions = self.slots.set_muted(&peer_id, muted).map_err(slot_error)?;
                self.apply(&actions);
                self.send_control(index, &ControlMessage::InputMuted { muted });
                Ok(())
            }
            HostCommand::Remove { peer_id, reason } => {
                let index = self.peer_index(&peer_id)?;
                self.remove(index, &reason);
                Ok(())
            }
        }
    }

    fn remove(&mut self, index: usize, reason: &str) {
        let peer_id = self.peers[index].id.clone();
        if let Ok(actions) = self.slots.remove(&peer_id) {
            self.apply(&actions);
        }
        self.signal_out(&peer_id, &Signal::Bye(reason.to_owned()));
        let mut peer = self.peers.remove(index);
        peer.link.close();
        peer.link.drive(Instant::now());
        if peer.state != PeerState::Disconnected {
            log(&format!(
                "host peer={index} state=disconnected path=null relayId=null removed={}",
                bounded(reason)
            ));
            self.emit(json!({
                "type":"coop-peer-state","peerId":peer_id,"state":"disconnected","path":null,"relayId":null,
            }));
        }
    }

    fn shutdown(&mut self) {
        while !self.peers.is_empty() {
            self.remove(self.peers.len() - 1, "host-stopped");
        }
        self.rumble.set_mask(0);
    }

    /// The link is gone but the lobby member stays until the shell removes it.
    fn disconnect(&mut self, index: usize, state: PeerState) {
        if let Ok(actions) = self.slots.disconnected(&self.peers[index].id) {
            self.apply(&actions);
        }
        self.peers[index].link.close();
        self.set_state(index, state);
    }

    fn apply(&mut self, actions: &[SlotAction]) {
        for action in actions {
            self.apply_action(*action);
        }
    }

    /// Queues one action on the NVST input path; false when that path refused it.
    fn apply_action(&mut self, action: SlotAction) -> bool {
        let timestamp = u64::try_from(self.origin.elapsed().as_micros()).unwrap_or(u64::MAX);
        let record = match action {
            SlotAction::Slots(mask) => {
                self.rumble.set_mask(mask);
                self.registered = self
                    .backend
                    .send_input(coop_slots_record(mask, timestamp))
                    .then_some(mask);
                return self.registered.is_some();
            }
            SlotAction::Gamepad { slot, state } => coop_gamepad_record(
                slot,
                state.buttons,
                state.left_trigger,
                state.right_trigger,
                (state.left_stick_x, state.left_stick_y),
                (state.right_stick_x, state.right_stick_y),
                timestamp,
            ),
            SlotAction::Neutral(slot) => coop_neutral_record(slot, timestamp),
        };
        self.backend.send_input(record)
    }

    /// One record from a guest's input channel, counted at the stage that consumed it.
    fn guest_input(&mut self, index: usize, data: &[u8], now: Instant) {
        self.peers[index].input.received += 1;
        let Some(record) = InputRecord::decode(data) else {
            self.peers[index].input.malformed += 1;
            return;
        };
        let peer_id = self.peers[index].id.clone();
        let action = match self.slots.input(&peer_id, record, now) {
            Ok(SlotAction::Gamepad { slot, .. })
                if self.registered.is_none_or(|mask| mask & (1 << slot) == 0) =>
            {
                Err(InputDrop::SlotNotOwned)
            }
            outcome => outcome,
        };
        let injected = action.map(|action| self.apply_action(action));
        let input = &mut self.peers[index].input;
        match injected {
            Ok(true) => input.injected += 1,
            Ok(false) => input.nvst_refused += 1,
            Err(InputDrop::SlotNotOwned) => input.slot_not_owned += 1,
            Err(InputDrop::Muted) => input.muted += 1,
            Err(InputDrop::Stale) => input.stale += 1,
        }
    }

    fn send_control(&mut self, index: usize, message: &ControlMessage) {
        let link = &mut self.peers[index].link;
        let control = link.control;
        link.send_channel(control, false, &message.encode());
    }

    fn forward(&mut self, packet: &CoopRtpPacket) {
        let bytes = &packet.packet;
        if bytes.len() < 12 || bytes[0] >> 6 != 2 {
            return;
        }
        self.stream_bytes += bytes.len() as u64;
        let wire = u16::from_be_bytes([bytes[2], bytes[3]]);
        let rtp_time = u32::from_be_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        let marker = bytes[1] & 0x80 != 0;
        let (seq, pt) = match packet.kind {
            CoopMediaKind::Video => (extend_seq(&mut self.video_seq, wire), VIDEO_PT),
            CoopMediaKind::Audio => (extend_seq(&mut self.audio_seq, wire), AUDIO_PT),
        };
        let now = Instant::now();
        for peer in &mut self.peers {
            if !peer.link.is_connected() {
                continue;
            }
            let (mid, base) = match packet.kind {
                CoopMediaKind::Video => (peer.link.video_mid, &mut peer.video_base),
                CoopMediaKind::Audio => (peer.link.audio_mid, &mut peer.audio_base),
            };
            let Some(mid) = mid else {
                continue;
            };
            let Some(peer_seq) = seq.checked_sub(*base.get_or_insert(seq & !0xffff)) else {
                continue;
            };
            let entry = RingEntry {
                seq: peer_seq,
                packet: Arc::clone(bytes),
                rtp_time,
                marker,
            };
            peer.link.write_rtp(mid, pt, &entry, now);
            if packet.kind == CoopMediaKind::Video {
                peer.ring.push(entry);
            }
        }
    }

    fn service_peers(&mut self, now: Instant) {
        let mut index = 0;
        while index < self.peers.len() {
            self.peers[index].link.poll_network(now);
            self.peers[index].link.drive(now);
            let events: Vec<LinkEvent> = self.peers[index].link.take_events().collect();
            for event in events {
                self.handle_link_event(index, event, now);
            }
            let peer = &self.peers[index];
            if peer.state == PeerState::Connecting
                && now.saturating_duration_since(peer.created_at) >= CONNECT_TIMEOUT
            {
                self.disconnect(index, PeerState::Failed);
            }
            index += 1;
        }
    }

    fn handle_link_event(&mut self, index: usize, event: LinkEvent, now: Instant) {
        if matches!(
            self.peers[index].state,
            PeerState::Disconnected | PeerState::Failed
        ) {
            return;
        }
        match event {
            LinkEvent::LocalCandidate(candidate) => {
                let peer_id = self.peers[index].id.clone();
                self.signal_out(&peer_id, &Signal::Candidate(candidate));
            }
            LinkEvent::Connected => {
                let peer_id = self.peers[index].id.clone();
                if let Ok(actions) = self.slots.connected(&peer_id, now) {
                    self.apply(&actions);
                }
                self.set_state(index, PeerState::Connected);
            }
            LinkEvent::Ice(state) => {
                use str0m::IceConnectionState::{Checking, Completed, Connected, Disconnected};
                match state {
                    Disconnected if self.peers[index].state == PeerState::Connected => {
                        self.set_state(index, PeerState::Reconnecting);
                    }
                    Connected | Completed if self.peers[index].state == PeerState::Reconnecting => {
                        self.set_state(index, PeerState::Connected);
                    }
                    Checking | Connected | Completed | Disconnected => {}
                    _ => {}
                }
            }
            LinkEvent::Path(_) => self.emit_state(index),
            LinkEvent::ChannelOpen(id, _) => {
                if self.peers[index].link.control == Some(id) {
                    let peer_id = self.peers[index].id.clone();
                    let description = serde_json::to_vec(&self.stream).unwrap_or_default();
                    let link = &mut self.peers[index].link;
                    link.send_channel(Some(id), false, &description);
                    if let Some(slot) = self.slots.slot_of(&peer_id) {
                        self.send_control(index, &ControlMessage::Slot { slot });
                    }
                    let muted = self.slots.is_muted(&peer_id);
                    self.send_control(index, &ControlMessage::InputMuted { muted });
                }
            }
            LinkEvent::ChannelData(id, data) => {
                let peer = &self.peers[index];
                if peer.link.input == Some(id) {
                    self.guest_input(index, &data, now);
                } else if peer.link.control == Some(id)
                    && ControlMessage::decode(&data) == Some(ControlMessage::KeyframeRequest)
                    && self.keyframes.request(now)
                {
                    self.backend.request_keyframe();
                }
            }
            LinkEvent::KeyframeRequest => {
                if self.keyframes.request(now) {
                    self.backend.request_keyframe();
                }
            }
            LinkEvent::AppFeedback { payload, .. } => {
                let Some(sequences) = decode_nack(&payload) else {
                    return;
                };
                let peer = &mut self.peers[index];
                let Some(mid) = peer.link.video_mid else {
                    return;
                };
                for wire in sequences {
                    let Some(entry) = peer.ring.extend(wire).and_then(|seq| peer.ring.get(seq))
                    else {
                        continue;
                    };
                    peer.link.write_rtp(mid, VIDEO_PT, entry, now);
                }
            }
            LinkEvent::Closed => self.disconnect(index, PeerState::Disconnected),
            LinkEvent::RelayUnavailable { .. }
            | LinkEvent::DirectBlocked
            | LinkEvent::ChannelClosed(_)
            | LinkEvent::Rtp(_) => {}
        }
    }

    fn route_rumble(&mut self) {
        for (slot, rumble) in self.rumble.take().into_iter().enumerate() {
            let Some(rumble) = rumble else {
                continue;
            };
            let Some(owner) = self.slots.owner_of(slot as u8).map(str::to_owned) else {
                continue;
            };
            if let Ok(index) = self.peer_index(&owner) {
                self.send_control(
                    index,
                    &ControlMessage::Rumble {
                        low_frequency: rumble.low_frequency,
                        high_frequency: rumble.high_frequency,
                        duration_ms: rumble.duration_ms,
                    },
                );
            }
        }
    }

    fn emit_stats(&mut self, now: Instant) {
        let seconds = now
            .saturating_duration_since(self.stats_at)
            .as_secs_f64()
            .max(0.001);
        self.stats_at = now;
        let kbps = |bytes: u64| bytes as f64 * 8.0 / 1_000.0 / seconds;
        let stream_kbps = kbps(std::mem::take(&mut self.stream_bytes));
        let ping = self.backend.host_to_gfn_ping_ms();
        let mut peers = Vec::with_capacity(self.peers.len());
        let mut upload_kbps = 0.0;
        let log_input = now.saturating_duration_since(self.input_log_at) >= INPUT_LOG_INTERVAL;
        if log_input {
            self.input_log_at = now;
        }
        for index in 0..self.peers.len() {
            let peer = &mut self.peers[index];
            let sent = peer.link.bytes_sent();
            peer.send_kbps = kbps(sent.saturating_sub(peer.sent_bytes_at_stats));
            peer.sent_bytes_at_stats = sent;
            upload_kbps += peer.send_kbps;
            let (path, relay_id) = path_fields(peer.link.path());
            peers.push(json!({
                "peerId":peer.id,
                "slot":self.slots.slot_of(&peer.id),
                "path":path,
                "relayId":relay_id,
                "peerToHostPingMs":peer.link.rtt().map(|rtt| rtt.as_secs_f64() * 1_000.0),
                "lossPct":peer.link.egress_loss().map(|loss| f64::from(loss) * 100.0),
                "sendKbps":peer.send_kbps,
                "inputMuted":self.slots.is_muted(&peer.id),
                "inputReceived":peer.input.received,
                "inputInjected":peer.input.injected,
                "inputDroppedMuted":peer.input.muted,
                "inputDroppedSlotNotOwned":peer.input.slot_not_owned,
                "inputDroppedMalformed":peer.input.malformed,
                "inputDroppedStale":peer.input.stale,
                "inputDroppedNvstRefused":peer.input.nvst_refused,
            }));
            if log_input {
                let input = peer.input;
                opennow_streamer_protocol::log::log_line(
                    "INFO",
                    "coop",
                    &format!(
                        "host input peer={index} slot={:?} received={} injected={} dropped muted={} slotNotOwned={} malformed={} stale={} nvstRefused={}",
                        self.slots.slot_of(&peer.id),
                        input.received,
                        input.injected,
                        input.muted,
                        input.slot_not_owned,
                        input.malformed,
                        input.stale,
                        input.nvst_refused
                    ),
                );
            }
            if peer.state == PeerState::Connected {
                self.send_control(
                    index,
                    &ControlMessage::Stats {
                        host_to_gfn_ping_ms: ping,
                    },
                );
            }
        }
        self.emit(json!({
            "type":"coop-host-stats",
            "uploadKbps":upload_kbps,
            "capacityKbps":null,
            "streamKbps":stream_kbps,
            "hostToGfnPingMs":ping,
            "peers":peers,
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sequence_extension_follows_wrap_and_reordering() {
        let mut last = None;
        assert_eq!(extend_seq(&mut last, 65_534), 65_534);
        assert_eq!(extend_seq(&mut last, 65_535), 65_535);
        assert_eq!(extend_seq(&mut last, 1), 65_537);
        assert_eq!(extend_seq(&mut last, 0), 65_536);
        assert_eq!(last, Some(65_537));
    }

    #[test]
    fn rumble_routes_only_guest_slots_and_keeps_the_latest() {
        let router = RumbleRouter::default();
        let rumble = |low| Rumble {
            low_frequency: low,
            high_frequency: 0,
            duration_ms: 10,
        };
        assert!(!router.route(1, rumble(1)));
        router.set_mask(0b0010);
        assert!(!router.route(0, rumble(1)));
        assert!(router.route(1, rumble(1)));
        assert!(router.route(1, rumble(2)));
        assert!(!router.route(4, rumble(3)));
        assert_eq!(router.take(), [None, Some(rumble(2)), None, None]);
        assert_eq!(router.take(), [None; 4]);
    }
}
