//! Loss repair between a host and its guests, and the keyframe request budget toward GFN.
//!
//! The host stores every forwarded video packet per guest under the peer-link sequence it
//! assigned. Guests NACK missing peer-link sequences with RTCP application-layer feedback
//! (PSFB, FMT 15) whose payload is 'ONAK' followed by Generic-NACK style (PID, BLP) pairs.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Holds the longest repair window at about 4,000 packets per second.
pub const RING_PACKETS: usize = 1_024;
pub const NACK_IDENTIFIER: &[u8; 4] = b"ONAK";
pub const MAX_NACK_PAIRS: usize = 32;
/// GFN's own reorder wait, the shortest window a guest uses.
const MIN_REPAIR_WINDOW: Duration = Duration::from_millis(52);
const MAX_REPAIR_WINDOW: Duration = Duration::from_millis(250);
const REPAIR_WINDOW_SLACK: Duration = Duration::from_millis(30);
const DEFAULT_RTT: Duration = Duration::from_millis(30);
const NACK_EXTRA_WAIT: Duration = Duration::from_millis(4);
const MAX_NACK_SENDS: u32 = 3;
const MAX_TRACKED_MISSING: usize = RING_PACKETS;
pub const KEYFRAME_INTERVAL: Duration = Duration::from_secs(1);
/// Unanswered keyframe requests space the next one out, up to this interval.
const MAX_KEYFRAME_INTERVAL: Duration = Duration::from_secs(4);

/// How long a guest waits for a repaired packet before it gives the frame up: three NACK
/// round trips to the host plus slack, within 52..=250 ms. The guest's NVST reorder wait and
/// its NACK tracking use the same window, so a gap is never declared lost while a repair can
/// still arrive.
pub fn repair_window(rtt: Option<Duration>) -> Duration {
    (rtt.unwrap_or(DEFAULT_RTT) * 3 + REPAIR_WINDOW_SLACK)
        .clamp(MIN_REPAIR_WINDOW, MAX_REPAIR_WINDOW)
}

#[derive(Debug, Clone)]
pub struct RingEntry {
    pub seq: u64,
    pub packet: Arc<[u8]>,
    pub rtp_time: u32,
    pub marker: bool,
}

/// Fixed-size retransmission ring (RING_PACKETS entries) indexed by peer-link sequence.
#[derive(Debug)]
pub struct RetransmitRing {
    entries: Vec<Option<RingEntry>>,
    highest: Option<u64>,
}

impl Default for RetransmitRing {
    fn default() -> Self {
        Self {
            entries: vec![None; RING_PACKETS],
            highest: None,
        }
    }
}

impl RetransmitRing {
    pub fn push(&mut self, entry: RingEntry) {
        self.highest = Some(
            self.highest
                .map_or(entry.seq, |highest| highest.max(entry.seq)),
        );
        let index = (entry.seq % RING_PACKETS as u64) as usize;
        self.entries[index] = Some(entry);
    }

    pub fn get(&self, seq: u64) -> Option<&RingEntry> {
        self.entries[(seq % RING_PACKETS as u64) as usize]
            .as_ref()
            .filter(|entry| entry.seq == seq)
    }

    /// Extends a 16-bit wire sequence to the closest stored peer-link sequence.
    pub fn extend(&self, wire: u16) -> Option<u64> {
        let highest = self.highest?;
        let behind = (highest as u16).wrapping_sub(wire);
        highest.checked_sub(u64::from(behind))
    }

    pub fn highest(&self) -> Option<u64> {
        self.highest
    }
}

/// Encodes missing sequences as 'ONAK' + (PID, BLP) pairs; returns None for an empty list.
pub fn encode_nack(missing: &[u64]) -> Option<Vec<u8>> {
    let mut sorted: Vec<u16> = missing.iter().map(|seq| *seq as u16).collect();
    sorted.sort_unstable();
    sorted.dedup();
    let start = (0..sorted.len())
        .max_by_key(|&index| {
            let previous = sorted[(index + sorted.len() - 1) % sorted.len()];
            sorted[index].wrapping_sub(previous)
        })
        .unwrap_or(0);
    sorted.rotate_left(start);
    let mut pairs: Vec<(u16, u16)> = Vec::new();
    for seq in sorted {
        if let Some((pid, blp)) = pairs.last_mut() {
            let offset = seq.wrapping_sub(*pid);
            if (1..=16).contains(&offset) {
                *blp |= 1 << (offset - 1);
                continue;
            }
        }
        if pairs.len() == MAX_NACK_PAIRS {
            break;
        }
        pairs.push((seq, 0));
    }
    if pairs.is_empty() {
        return None;
    }
    let mut payload = Vec::with_capacity(4 + pairs.len() * 4);
    payload.extend_from_slice(NACK_IDENTIFIER);
    for (pid, blp) in pairs {
        payload.extend_from_slice(&pid.to_be_bytes());
        payload.extend_from_slice(&blp.to_be_bytes());
    }
    Some(payload)
}

/// Decodes an 'ONAK' payload into 16-bit sequences; malformed or oversized payloads yield None.
pub fn decode_nack(payload: &[u8]) -> Option<Vec<u16>> {
    let body = payload.strip_prefix(NACK_IDENTIFIER.as_slice())?;
    if body.is_empty() || body.len() % 4 != 0 || body.len() / 4 > MAX_NACK_PAIRS {
        return None;
    }
    let mut sequences = Vec::new();
    for pair in body.chunks_exact(4) {
        let pid = u16::from_be_bytes([pair[0], pair[1]]);
        let blp = u16::from_be_bytes([pair[2], pair[3]]);
        sequences.push(pid);
        for bit in 0..16 {
            if blp & (1 << bit) != 0 {
                sequences.push(pid.wrapping_add(bit + 1));
            }
        }
    }
    Some(sequences)
}

#[derive(Debug, Clone, Copy)]
struct Missing {
    first_seen: Instant,
    last_sent: Option<Instant>,
    sends: u32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LossCounters {
    pub received: u64,
    pub repaired: u64,
    pub lost: u64,
}

/// Guest-side gap tracking over the contiguous peer-link sequence of the video stream.
#[derive(Debug, Default)]
pub struct NackTracker {
    highest: Option<u64>,
    missing: BTreeMap<u64, Missing>,
    counters: LossCounters,
}

impl NackTracker {
    pub fn observe(&mut self, seq: u64, now: Instant) {
        self.counters.received += 1;
        let Some(highest) = self.highest else {
            self.highest = Some(seq);
            return;
        };
        if seq <= highest {
            if self.missing.remove(&seq).is_some() {
                self.counters.repaired += 1;
            }
            return;
        }
        let first_missing = (highest + 1).max(seq.saturating_sub(MAX_TRACKED_MISSING as u64));
        self.counters.lost += first_missing - (highest + 1);
        for missing in first_missing..seq {
            self.missing.insert(
                missing,
                Missing {
                    first_seen: now,
                    last_sent: None,
                    sends: 0,
                },
            );
        }
        while self.missing.len() > MAX_TRACKED_MISSING {
            self.missing.pop_first();
            self.counters.lost += 1;
        }
        self.highest = Some(seq);
    }

    /// Sequences due for a (re)send now. Retries follow RTT + 4 ms with at most
    /// clamp(window / RTT, 1, 3) sends inside the repair window.
    pub fn poll(&mut self, now: Instant, rtt: Option<Duration>) -> Vec<u64> {
        let window = repair_window(rtt);
        let rtt = rtt.unwrap_or(DEFAULT_RTT);
        let cap = if rtt.is_zero() {
            MAX_NACK_SENDS
        } else {
            u32::try_from(window.as_nanos() / rtt.as_nanos())
                .unwrap_or(MAX_NACK_SENDS)
                .clamp(1, MAX_NACK_SENDS)
        };
        let interval = rtt + NACK_EXTRA_WAIT;
        let mut expired = 0;
        self.missing.retain(|_, missing| {
            let keep = now.saturating_duration_since(missing.first_seen) < window;
            if !keep {
                expired += 1;
            }
            keep
        });
        self.counters.lost += expired;
        let mut due = Vec::new();
        for (seq, missing) in &mut self.missing {
            if missing.sends >= cap
                || missing
                    .last_sent
                    .is_some_and(|sent| now.saturating_duration_since(sent) < interval)
            {
                continue;
            }
            missing.sends += 1;
            missing.last_sent = Some(now);
            due.push(*seq);
            if due.len() == MAX_NACK_PAIRS * 17 {
                break;
            }
        }
        due
    }

    pub fn counters(&self) -> LossCounters {
        self.counters
    }
}

/// Spacing of one guest's keyframe requests: one per KEYFRAME_INTERVAL while each request
/// is answered with a keyframe, doubling up to MAX_KEYFRAME_INTERVAL while they are not, so
/// a link that keeps losing IDRs stops asking GFN for one every second.
#[derive(Debug)]
pub struct KeyframeBackoff {
    interval: Duration,
    sent_at: Option<Instant>,
    keyframes_at_request: u64,
}

impl Default for KeyframeBackoff {
    fn default() -> Self {
        Self {
            interval: KEYFRAME_INTERVAL,
            sent_at: None,
            keyframes_at_request: 0,
        }
    }
}

impl KeyframeBackoff {
    pub fn due(&self, now: Instant) -> bool {
        self.sent_at
            .is_none_or(|sent| now.saturating_duration_since(sent) >= self.interval)
    }

    /// Records a request sent with 'keyframes_delivered' keyframes assembled so far. With
    /// 'backoff' false the next request stays one interval away.
    pub fn sent(&mut self, now: Instant, keyframes_delivered: u64, backoff: bool) {
        let unanswered = self.sent_at.is_some() && keyframes_delivered == self.keyframes_at_request;
        self.interval = if backoff && unanswered {
            (self.interval * 2).min(MAX_KEYFRAME_INTERVAL)
        } else {
            KEYFRAME_INTERVAL
        };
        self.keyframes_at_request = keyframes_delivered;
        self.sent_at = Some(now);
    }
}

/// Coalesces guest keyframe requests to at most one GFN request per interval. A request
/// arriving inside the interval is remembered and fires when the interval ends.
#[derive(Debug, Default)]
pub struct KeyframeLimiter {
    last_forwarded: Option<Instant>,
    pending: bool,
}

impl KeyframeLimiter {
    pub fn request(&mut self, now: Instant) -> bool {
        self.pending = true;
        self.poll(now)
    }

    /// Records a request forwarded outside the limit; any pending one is satisfied by it.
    pub fn force(&mut self, now: Instant) {
        self.pending = false;
        self.last_forwarded = Some(now);
    }

    pub fn poll(&mut self, now: Instant) -> bool {
        if !self.pending
            || self
                .last_forwarded
                .is_some_and(|last| now.saturating_duration_since(last) < KEYFRAME_INTERVAL)
        {
            return false;
        }
        self.pending = false;
        self.last_forwarded = Some(now);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(seq: u64) -> RingEntry {
        RingEntry {
            seq,
            packet: Arc::from(seq.to_be_bytes().as_slice()),
            rtp_time: seq as u32,
            marker: false,
        }
    }

    #[test]
    fn ring_answers_only_the_last_ring_packets() {
        let mut ring = RetransmitRing::default();
        let last = RING_PACKETS as u64 + 43;
        for seq in 0..=last {
            ring.push(entry(seq));
        }
        assert!(ring.get(43).is_none(), "evicted by {last}");
        assert_eq!(ring.get(44).unwrap().seq, 44);
        assert_eq!(ring.get(last).unwrap().seq, last);
        assert!(ring.get(last + 1).is_none());
        assert_eq!(ring.extend(last as u16), Some(last));
        assert_eq!(ring.extend(44), Some(44));
    }

    #[test]
    fn the_repair_window_covers_three_round_trips_within_bounds() {
        assert_eq!(
            repair_window(Some(Duration::from_millis(5))),
            Duration::from_millis(52)
        );
        assert_eq!(
            repair_window(Some(Duration::from_millis(40))),
            Duration::from_millis(150)
        );
        assert_eq!(
            repair_window(Some(Duration::from_millis(200))),
            Duration::from_millis(250)
        );
        assert_eq!(repair_window(None), Duration::from_millis(120));
    }

    /// At 40 ms RTT GFN's 52 ms budget allowed one NACK that could not return in time; the
    /// co-op window keeps the gap open for three.
    #[test]
    fn a_slow_hop_still_gets_three_nacks_before_the_gap_is_lost() {
        let start = Instant::now();
        let rtt = Some(Duration::from_millis(40));
        let mut tracker = NackTracker::default();
        tracker.observe(1, start);
        tracker.observe(3, start);
        let mut sends = 0;
        for ms in 0..150 {
            sends += tracker.poll(start + Duration::from_millis(ms), rtt).len();
        }
        assert_eq!(sends, 3);
        assert_eq!(tracker.counters().lost, 0, "still inside the window");
        tracker.observe(2, start + Duration::from_millis(140));
        assert_eq!(tracker.counters().repaired, 1);
    }

    #[test]
    fn ring_extends_wire_sequences_across_wrap() {
        let mut ring = RetransmitRing::default();
        for seq in 65_500..65_600 {
            ring.push(entry(seq));
        }
        assert_eq!(ring.extend(65_530), Some(65_530));
        assert_eq!(ring.extend(10), Some(65_546));
        assert_eq!(ring.get(ring.extend(10).unwrap()).unwrap().seq, 65_546);
    }

    #[test]
    fn nack_payload_round_trips_with_bitmask_pairs() {
        let missing = [100, 101, 116, 117, 65_535, 0];
        let payload = encode_nack(&missing).unwrap();
        assert_eq!(&payload[..4], b"ONAK");
        let mut decoded = decode_nack(&payload).unwrap();
        decoded.sort_unstable();
        assert_eq!(decoded, [0, 100, 101, 116, 117, 65_535]);
        assert_eq!(payload.len(), 4 + 3 * 4);
        assert!(encode_nack(&[]).is_none());
        assert!(decode_nack(b"ONAK").is_none());
        assert!(decode_nack(b"REMB\0\0\0\0").is_none());
        assert!(decode_nack(&[b'O', b'N', b'A', b'K', 0, 1, 0]).is_none());
    }

    #[test]
    fn tracker_requests_gaps_retries_on_rtt_and_counts_repairs() {
        let start = Instant::now();
        let mut tracker = NackTracker::default();
        tracker.observe(10, start);
        tracker.observe(13, start);
        let rtt = Some(Duration::from_millis(10));
        assert_eq!(tracker.poll(start, rtt), [11, 12]);
        assert!(
            tracker
                .poll(start + Duration::from_millis(5), rtt)
                .is_empty()
        );
        tracker.observe(11, start + Duration::from_millis(12));
        assert_eq!(tracker.poll(start + Duration::from_millis(14), rtt), [12]);
        assert_eq!(tracker.poll(start + Duration::from_millis(28), rtt), [12]);
        assert!(
            tracker
                .poll(start + Duration::from_millis(42), rtt)
                .is_empty(),
            "send cap of 3"
        );
        tracker.poll(start + Duration::from_millis(60), rtt);
        assert_eq!(
            tracker.counters(),
            LossCounters {
                received: 3,
                repaired: 1,
                lost: 1
            }
        );
    }

    #[test]
    fn tracker_bounds_huge_gaps_to_the_ring() {
        let now = Instant::now();
        let mut tracker = NackTracker::default();
        tracker.observe(0, now);
        tracker.observe(10_000, now);
        let due = tracker.poll(now, Some(Duration::from_millis(5)));
        assert!(due.len() <= RING_PACKETS);
        assert_eq!(*due.first().unwrap(), 10_000 - RING_PACKETS as u64);
        assert_eq!(tracker.counters().lost, 10_000 - 1 - RING_PACKETS as u64);
    }

    #[test]
    fn unanswered_keyframe_requests_back_off_and_an_answer_resets_them() {
        let start = Instant::now();
        let at = |seconds: u64| start + Duration::from_secs(seconds);
        let mut backoff = KeyframeBackoff::default();
        assert!(backoff.due(start));
        backoff.sent(start, 0, true);
        assert!(!backoff.due(at(1) - Duration::from_millis(1)));
        assert!(backoff.due(at(1)));
        backoff.sent(at(1), 0, true);
        assert!(!backoff.due(at(2)), "unanswered: two seconds");
        assert!(backoff.due(at(3)));
        backoff.sent(at(3), 0, true);
        backoff.sent(at(7), 0, true);
        assert!(!backoff.due(at(10)), "capped at four seconds");
        assert!(backoff.due(at(11)));
        backoff.sent(at(11), 1, true);
        assert!(
            backoff.due(at(12)),
            "a keyframe arrived: back to one second"
        );
        backoff.sent(at(12), 1, false);
        assert!(backoff.due(at(13)), "first-frame requests never back off");
    }

    #[test]
    fn keyframe_requests_reach_gfn_at_most_once_per_second() {
        let start = Instant::now();
        let mut limiter = KeyframeLimiter::default();
        assert!(limiter.request(start));
        assert!(!limiter.request(start + Duration::from_millis(10)));
        assert!(!limiter.request(start + Duration::from_millis(900)));
        assert!(!limiter.poll(start + Duration::from_millis(999)));
        assert!(
            limiter.poll(start + Duration::from_secs(1)),
            "coalesced request fires"
        );
        assert!(
            !limiter.poll(start + Duration::from_secs(3)),
            "nothing pending"
        );
        assert!(limiter.request(start + Duration::from_secs(3)));
    }
}
