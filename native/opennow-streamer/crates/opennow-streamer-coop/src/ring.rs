//! Loss repair between a host and its guests, and the keyframe request budget toward GFN.
//!
//! The host stores every forwarded video packet per guest under the peer-link sequence it
//! assigned. Guests NACK missing peer-link sequences with RTCP application-layer feedback
//! (PSFB, FMT 15) whose payload is 'ONAK' followed by Generic-NACK style (PID, BLP) pairs.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const RING_PACKETS: usize = 256;
pub const NACK_IDENTIFIER: &[u8; 4] = b"ONAK";
pub const MAX_NACK_PAIRS: usize = 32;
/// Matches the guest's NVST reorder wait: a repair later than this cannot be assembled.
pub const NACK_TRACKING_TIMEOUT: Duration = Duration::from_millis(52);
const NACK_EXTRA_WAIT: Duration = Duration::from_millis(4);
const MAX_NACK_SENDS: u32 = 3;
const MAX_TRACKED_MISSING: usize = RING_PACKETS;
pub const KEYFRAME_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Debug, Clone)]
pub struct RingEntry {
    pub seq: u64,
    pub packet: Arc<[u8]>,
    pub rtp_time: u32,
    pub marker: bool,
}

/// Fixed 256-entry retransmission ring indexed by peer-link sequence.
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
    /// clamp(52 ms / RTT, 1, 3) sends, the same budget as GFN's Mjolnir NACK policy.
    pub fn poll(&mut self, now: Instant, rtt: Option<Duration>) -> Vec<u64> {
        let rtt = rtt.unwrap_or(Duration::from_millis(30));
        let cap = if rtt.is_zero() {
            MAX_NACK_SENDS
        } else {
            u32::try_from(NACK_TRACKING_TIMEOUT.as_nanos() / rtt.as_nanos())
                .unwrap_or(MAX_NACK_SENDS)
                .clamp(1, MAX_NACK_SENDS)
        };
        let interval = rtt + NACK_EXTRA_WAIT;
        let mut expired = 0;
        self.missing.retain(|_, missing| {
            let keep = now.saturating_duration_since(missing.first_seen) < NACK_TRACKING_TIMEOUT;
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
    fn ring_answers_only_the_last_256_packets() {
        let mut ring = RetransmitRing::default();
        for seq in 0..300 {
            ring.push(entry(seq));
        }
        assert!(ring.get(43).is_none(), "evicted by 299");
        assert_eq!(ring.get(44).unwrap().seq, 44);
        assert_eq!(ring.get(299).unwrap().seq, 299);
        assert!(ring.get(300).is_none());
        assert_eq!(ring.extend(299), Some(299));
        assert_eq!(ring.extend(44), Some(44));
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
