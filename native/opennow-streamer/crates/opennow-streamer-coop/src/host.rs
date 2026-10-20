//! Host fan-out: forwards the decrypted NVST RTP tap to every connected guest, answers
//! guest NACKs from a per-guest ring, rate-limits keyframe requests to GFN, and injects
//! guest gamepad input into GFN controller slots 1..3.

use std::collections::VecDeque;
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
use crate::protocol::{ControlMessage, Signal, StreamDescription, generation};
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
const PEER_LOG_INTERVAL: Duration = Duration::from_secs(5);
/// Inbound silence after which a connected guest is shown as reconnecting. A live guest
/// sends input every 50 ms and ICE checks at least every second.
const LIVENESS_TIMEOUT: Duration = Duration::from_millis(2_500);
/// Inbound silence after which the host replaces the guest's link.
const RESTART_AFTER: Duration = Duration::from_secs(4);
/// Time a replacement link gets to connect before the host tries another.
const RESTART_TIMEOUT: Duration = Duration::from_secs(8);
/// Replacement links tried before the guest is failed; with RESTART_TIMEOUT this stays
/// inside the 60 s the guest's reconnect toast counts down.
const MAX_RESTARTS: u32 = 6;
/// Pacing toward each guest: at least 20 Mbps and 2.5x the forwarded stream rate, with a
/// 16 KB burst so ordinary frames leave at once and only large frames are spread out.
const PACE_FLOOR_BPS: f64 = 20_000_000.0;
const PACE_FACTOR: f64 = 2.5;
const PACE_BURST_BYTES: f64 = 16_384.0;
const MAX_PACED_PACKETS: usize = 2_048;

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
    AddPeer {
        peer_id: String,
        slot: u8,
    },
    Signal {
        peer_id: String,
        signal: Signal,
        generation: u32,
    },
    Slot {
        peer_id: String,
        slot: u8,
    },
    Muted {
        peer_id: String,
        muted: bool,
    },
    Remove {
        peer_id: String,
        reason: String,
    },
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
                    peer_log_at: Instant::now(),
                    keyframes_to_gfn: 0,
                    registered: None,
                    stream_bps: 0.0,
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
            generation: generation(payload),
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
    forwarded: PeerForwarded,
    /// The guest's first keyframe request after its link connected skips the shared
    /// limiter once, so a joining guest never waits a limiter interval for its first frame.
    first_keyframe_request: bool,
    /// Link generation: 0 for the first offer, one more for every replacement link.
    generation: u32,
    /// Replacement links tried since the guest was last connected.
    restarts: u32,
    /// When the current replacement link was offered.
    restart_at: Option<Instant>,
    pacer: Pacer,
}

/// Releases one guest's video at a bounded rate. The tap hands the host each frame as a
/// burst (a 50-70 KB IDR arrives as one); written straight out, that burst overflows the
/// host's uplink queue and the IDR is the frame that gets lost. Repairs jump the queue.
#[derive(Debug)]
struct Pacer {
    queue: VecDeque<(RingEntry, bool)>,
    budget: f64,
    refilled_at: Option<Instant>,
    dropped: u64,
}

impl Default for Pacer {
    fn default() -> Self {
        Self {
            queue: VecDeque::new(),
            budget: PACE_BURST_BYTES,
            refilled_at: None,
            dropped: 0,
        }
    }
}

impl Pacer {
    fn push(&mut self, entry: RingEntry) {
        if self.queue.len() == MAX_PACED_PACKETS {
            self.queue.pop_front();
            self.dropped += 1;
        }
        self.queue.push_back((entry, false));
    }

    fn push_repair(&mut self, entry: RingEntry) {
        if self.queue.len() == MAX_PACED_PACKETS {
            self.queue.pop_back();
            self.dropped += 1;
        }
        self.queue.push_front((entry, true));
    }

    /// Packets the budget allows now, oldest first; each is flagged when it is a repair.
    fn release(&mut self, now: Instant, rate_bps: f64) -> Vec<(RingEntry, bool)> {
        let elapsed = self
            .refilled_at
            .map_or(0.0, |at| now.saturating_duration_since(at).as_secs_f64());
        self.refilled_at = Some(now);
        self.budget = (self.budget + elapsed * rate_bps / 8.0).min(PACE_BURST_BYTES);
        let mut released = Vec::new();
        while self.budget > 0.0 {
            let Some(item) = self.queue.pop_front() else {
                break;
            };
            self.budget -= item.0.packet.len() as f64;
            released.push(item);
        }
        released
    }
}

/// Media written to one guest's link: forwarded packets, completed video frames, and
/// packets resent from the ring for its NACKs.
#[derive(Debug, Default, Clone, Copy)]
struct PeerForwarded {
    video: u64,
    audio: u64,
    frames: u64,
    repaired: u64,
    /// RTP timestamp of the last video packet written; a new one starts a frame.
    video_rtp_time: Option<u32>,
    /// Keyframe requests the guest sent on 'coop-control', and RTCP PLIs.
    keyframe_requests: u64,
    plis: u64,
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

fn forwarded_summary(forwarded: &PeerForwarded) -> String {
    format!(
        "forwarded video={} frames={} audio={} repaired={} guestKeyframeRequests={} guestPlis={}",
        forwarded.video,
        forwarded.frames,
        forwarded.audio,
        forwarded.repaired,
        forwarded.keyframe_requests,
        forwarded.plis
    )
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
    peer_log_at: Instant,
    /// GFN keyframe requests this host made on behalf of its guests.
    keyframes_to_gfn: u64,
    /// Guest slot mask the NVST input path last accepted; re-sent until it matches.
    registered: Option<u8>,
    /// Forwarded stream rate: rises with the last second at once and decays slowly.
    stream_bps: f64,
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
                self.request_gfn_keyframe();
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

    fn signal_out(&self, peer_id: &str, signal: &Signal, generation: u32) {
        self.emit(
            json!({"type":"coop-signal-out","peerId":peer_id,"payload":signal.to_json(generation)}),
        );
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
                self.signal_out(&peer_id, &Signal::Offer(offer), 0);
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
                    forwarded: PeerForwarded::default(),
                    first_keyframe_request: false,
                    generation: 0,
                    restarts: 0,
                    restart_at: None,
                    pacer: Pacer::default(),
                });
                log(&format!(
                    "host peer={} added slot={slot} peers={}",
                    self.peers.len() - 1,
                    self.peers.len()
                ));
                self.emit_state(self.peers.len() - 1);
                Ok(())
            }
            HostCommand::Signal {
                peer_id,
                signal,
                generation,
            } => {
                let index = self.peer_index(&peer_id)?;
                if generation != self.peers[index].generation && !matches!(signal, Signal::Bye(_)) {
                    return Ok(());
                }
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
                    Signal::Restart => {
                        if !matches!(
                            self.peers[index].state,
                            PeerState::Disconnected | PeerState::Failed
                        ) {
                            self.restart(index, Instant::now(), "guest-request");
                        }
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
        let generation = self.peers[index].generation;
        self.signal_out(&peer_id, &Signal::Bye(reason.to_owned()), generation);
        let mut peer = self.peers.remove(index);
        log(&format!(
            "host peer={index} removed reason={} {}",
            bounded(reason),
            forwarded_summary(&peer.forwarded)
        ));
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

    /// The guest stopped reaching the host: show it as reconnecting and release its buttons.
    fn link_down(&mut self, index: usize) {
        if let Ok(actions) = self.slots.disconnected(&self.peers[index].id) {
            self.apply(&actions);
        }
        self.set_state(index, PeerState::Reconnecting);
    }

    /// Replaces the guest's link with a fresh one (new sockets, candidates, relays and DTLS)
    /// and offers it through the signal path, which does not depend on the dead link.
    fn restart(&mut self, index: usize, now: Instant, reason: &str) {
        if self.peers[index].restarts >= MAX_RESTARTS {
            log(&format!(
                "host peer={index} restart gave up after {} attempts",
                self.peers[index].restarts
            ));
            self.disconnect(index, PeerState::Failed);
            return;
        }
        let link = PeerLink::new(Arc::clone(&self.ice), self.net.clone(), now);
        let Some((mut link, offer)) = link
            .ok()
            .and_then(|mut link| link.create_offer().map(|offer| (link, offer)))
        else {
            log(&format!("host peer={index} restart could not open a link"));
            self.disconnect(index, PeerState::Failed);
            return;
        };
        link.start_relays_now(now);
        if self.peers[index].state == PeerState::Connected {
            self.link_down(index);
        }
        let peer = &mut self.peers[index];
        std::mem::replace(&mut peer.link, link).abandon();
        peer.generation += 1;
        peer.restarts += 1;
        peer.restart_at = Some(now);
        peer.video_base = None;
        peer.audio_base = None;
        peer.ring = RetransmitRing::default();
        peer.pacer = Pacer::default();
        peer.sent_bytes_at_stats = 0;
        peer.first_keyframe_request = false;
        peer.forwarded.video_rtp_time = None;
        let (id, generation, attempt) = (peer.id.clone(), peer.generation, peer.restarts);
        log(&format!(
            "host peer={index} restart generation={generation} attempt={attempt} reason={reason}"
        ));
        self.set_state(index, PeerState::Reconnecting);
        self.signal_out(&id, &Signal::Offer(offer), generation);
    }

    fn check_liveness(&mut self, index: usize, now: Instant) {
        let peer = &self.peers[index];
        let silence = peer.link.inbound_silence(now);
        match (peer.state, peer.restart_at) {
            (PeerState::Connected, _) if silence >= LIVENESS_TIMEOUT => {
                log(&format!(
                    "host peer={index} silent silenceMs={}",
                    silence.as_millis()
                ));
                self.link_down(index);
            }
            (PeerState::Reconnecting, None) if silence >= RESTART_AFTER => {
                self.restart(index, now, "silence");
            }
            (PeerState::Reconnecting, None) if silence < LIVENESS_TIMEOUT => {
                let _ = self.slots.connected(&self.peers[index].id, now);
                self.set_state(index, PeerState::Connected);
            }
            (PeerState::Reconnecting, Some(at))
                if now.saturating_duration_since(at) >= RESTART_TIMEOUT =>
            {
                self.restart(index, now, "timeout");
            }
            _ => {}
        }
    }

    fn pace_bps(&self) -> f64 {
        (self.stream_bps * PACE_FACTOR).max(PACE_FLOOR_BPS)
    }

    fn release_paced(&mut self, index: usize, now: Instant) {
        let rate = self.pace_bps();
        let peer = &mut self.peers[index];
        let Some(mid) = peer.link.video_mid else {
            return;
        };
        for (entry, repair) in peer.pacer.release(now, rate) {
            if !peer.link.write_rtp(mid, VIDEO_PT, &entry, now) || repair {
                continue;
            }
            peer.forwarded.video += 1;
            if peer.forwarded.video_rtp_time.replace(entry.rtp_time) != Some(entry.rtp_time) {
                peer.forwarded.frames += 1;
            }
        }
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

    fn request_gfn_keyframe(&mut self) {
        self.keyframes_to_gfn += 1;
        self.backend.request_keyframe();
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
            if peer.state != PeerState::Connected || !peer.link.is_connected() {
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
            match packet.kind {
                CoopMediaKind::Video => {
                    peer.ring.push(entry.clone());
                    peer.pacer.push(entry);
                }
                CoopMediaKind::Audio => {
                    if peer.link.write_rtp(mid, pt, &entry, now) {
                        peer.forwarded.audio += 1;
                    }
                }
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
            self.check_liveness(index, now);
            if self.peers[index].state == PeerState::Connected {
                self.release_paced(index, now);
                self.peers[index].link.drive(now);
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
                let generation = self.peers[index].generation;
                self.signal_out(&peer_id, &Signal::Candidate(candidate), generation);
            }
            LinkEvent::Connected => {
                let peer_id = self.peers[index].id.clone();
                let _ = self.slots.connected(&peer_id, now);
                self.set_state(index, PeerState::Connected);
                let peer = &mut self.peers[index];
                peer.restarts = 0;
                peer.restart_at = None;
                peer.first_keyframe_request = true;
                if self.keyframes.request(now) {
                    self.request_gfn_keyframe();
                }
                log(&format!("host peer={index} connected; keyframe requested"));
            }
            LinkEvent::Ice(str0m::IceConnectionState::Disconnected)
                if self.peers[index].link.is_connected()
                    && self.peers[index].restart_at.is_none() =>
            {
                self.restart(index, now, "ice-disconnected");
            }
            LinkEvent::Ice(_) => {}
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
                {
                    self.peers[index].forwarded.keyframe_requests += 1;
                    let first = std::mem::take(&mut self.peers[index].first_keyframe_request);
                    if first {
                        self.keyframes.force(now);
                        self.request_gfn_keyframe();
                        log(&format!(
                            "host peer={index} first guest keyframe request forwarded"
                        ));
                    } else if self.keyframes.request(now) {
                        self.request_gfn_keyframe();
                    }
                }
            }
            LinkEvent::KeyframeRequest => {
                self.peers[index].forwarded.plis += 1;
                if self.keyframes.request(now) {
                    self.request_gfn_keyframe();
                }
            }
            LinkEvent::AppFeedback { payload, .. } => {
                let Some(sequences) = decode_nack(&payload) else {
                    return;
                };
                let peer = &mut self.peers[index];
                if peer.link.video_mid.is_none() {
                    return;
                }
                for wire in sequences.into_iter().rev() {
                    let Some(entry) = peer.ring.extend(wire).and_then(|seq| peer.ring.get(seq))
                    else {
                        continue;
                    };
                    peer.pacer.push_repair(entry.clone());
                    peer.forwarded.repaired += 1;
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
        self.stream_bps = (stream_kbps * 1_000.0).max(self.stream_bps * 0.8);
        let ping = self.backend.host_to_gfn_ping_ms();
        let mut peers = Vec::with_capacity(self.peers.len());
        let mut upload_kbps = 0.0;
        let log_peers = now.saturating_duration_since(self.peer_log_at) >= PEER_LOG_INTERVAL;
        if log_peers {
            self.peer_log_at = now;
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
            if log_peers {
                let input = peer.input;
                log(&format!(
                    "host peer={index} state={} slot={:?} {} sendKbps={:.0} keyframesToGfn={} paceKbps={:.0} pacedQueue={} pacedDropped={} silenceMs={} generation={}",
                    peer.state.label(),
                    self.slots.slot_of(&peer.id),
                    forwarded_summary(&peer.forwarded),
                    peer.send_kbps,
                    self.keyframes_to_gfn,
                    (self.stream_bps * PACE_FACTOR).max(PACE_FLOOR_BPS) / 1_000.0,
                    peer.pacer.queue.len(),
                    peer.pacer.dropped,
                    peer.link.inbound_silence(now).as_millis(),
                    peer.generation
                ));
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

    fn entry(seq: u64, bytes: usize) -> RingEntry {
        RingEntry {
            seq,
            packet: Arc::from(vec![0_u8; bytes]),
            rtp_time: 0,
            marker: false,
        }
    }

    /// A 60 KB IDR handed over in one burst leaves at the pacing rate after a 16 KB burst,
    /// and a repair goes out before the queued video.
    #[test]
    fn the_pacer_spreads_an_idr_burst_and_sends_repairs_first() {
        let start = Instant::now();
        let mut pacer = Pacer::default();
        for seq in 0..50 {
            pacer.push(entry(seq, 1_200));
        }
        let first = pacer.release(start, PACE_FLOOR_BPS);
        assert_eq!(
            first.len(),
            14,
            "the burst allowance plus the packet that crosses it"
        );
        pacer.push_repair(entry(7, 1_200));
        let after_1ms = pacer.release(start + Duration::from_millis(1), PACE_FLOOR_BPS);
        assert!(
            after_1ms[0].1 && after_1ms[0].0.seq == 7,
            "the repair leads"
        );
        assert_eq!(after_1ms.len(), 2, "20 Mbps is 2,500 bytes per ms");
        let mut sent = first.len() + after_1ms.len() - 1;
        let mut elapsed = 1;
        while sent < 50 {
            elapsed += 1;
            sent += pacer
                .release(start + Duration::from_millis(elapsed), PACE_FLOOR_BPS)
                .len();
        }
        assert_eq!(
            elapsed, 18,
            "the 61 KB after the first 16 KB leave at 2.5 KB per ms"
        );
    }

    #[test]
    fn the_pacer_bounds_its_queue() {
        let mut pacer = Pacer::default();
        for seq in 0..MAX_PACED_PACKETS as u64 + 5 {
            pacer.push(entry(seq, 1_200));
        }
        assert_eq!(pacer.queue.len(), MAX_PACED_PACKETS);
        assert_eq!(pacer.dropped, 5);
        assert_eq!(
            pacer.queue.front().unwrap().0.seq,
            5,
            "the oldest packets go first"
        );
    }

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
