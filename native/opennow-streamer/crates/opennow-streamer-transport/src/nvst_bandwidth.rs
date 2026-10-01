use std::time::Instant;

const TRANSIT_SAMPLES: usize = 16;

#[derive(Debug, Clone, Copy)]
pub(crate) struct PacketBandwidthSample {
    pub(crate) frame_number: u32,
    pub(crate) sequence: u32,
    pub(crate) timestamp: u32,
    pub(crate) bytes: usize,
    pub(crate) started: bool,
    pub(crate) ended: bool,
    pub(crate) received_at: Instant,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct BandwidthFeedback {
    pub(crate) minimum_server_time: u32,
    pub(crate) median_server_time: u32,
    pub(crate) queue_delay_us: u32,
    pub(crate) jitter_us: u16,
    pub(crate) utilization_percent: u8,
    pub(crate) estimate_kbps: u32,
    pub(crate) lossy_frames: u32,
}

#[derive(Debug, Clone, Copy)]
struct FrameSample {
    number: u32,
    first_at: Instant,
    last_at: Instant,
    first_timestamp: u64,
    last_timestamp: u64,
    last_sequence: u32,
    bytes: u64,
    sender_gap_ms: f64,
    started: bool,
    ended: bool,
    contiguous: bool,
}

#[derive(Debug)]
pub(crate) struct BandwidthEstimator {
    first_arrival: Option<Instant>,
    last_timestamp: Option<(u32, u64)>,
    minimum_transit_ms: Option<f64>,
    previous_transit_ms: Option<f64>,
    jitter_ms: f64,
    transits: [f64; TRANSIT_SAMPLES],
    transit_count: usize,
    transit_next: usize,
    latest_transit_ms: f64,
    active_frame: Option<FrameSample>,
    frames_observed: u32,
    lossy_frames: u32,
    average_frame_ms: Option<f64>,
    average_receive_kbps: Option<f64>,
    average_utilization_percent: Option<f64>,
}

impl Default for BandwidthEstimator {
    fn default() -> Self {
        Self {
            first_arrival: None,
            last_timestamp: None,
            minimum_transit_ms: None,
            previous_transit_ms: None,
            jitter_ms: 0.0,
            transits: [0.0; TRANSIT_SAMPLES],
            transit_count: 0,
            transit_next: 0,
            latest_transit_ms: 0.0,
            active_frame: None,
            frames_observed: 0,
            lossy_frames: 0,
            average_frame_ms: None,
            average_receive_kbps: None,
            average_utilization_percent: None,
        }
    }
}

impl BandwidthEstimator {
    pub(crate) fn observe_parity(&mut self, frame_number: u32, sequence: u32) {
        if let Some(frame) = self.active_frame.as_mut()
            && frame.number == frame_number
            && sequence > frame.last_sequence
        {
            if sequence != frame.last_sequence + 1 {
                frame.contiguous = false;
            }
            frame.last_sequence = sequence;
        }
    }

    pub(crate) fn observe(&mut self, sample: PacketBandwidthSample) {
        let PacketBandwidthSample {
            frame_number,
            sequence,
            timestamp,
            bytes,
            started,
            ended,
            received_at: at,
        } = sample;
        let first = *self.first_arrival.get_or_insert(at);
        let extended = match self.last_timestamp {
            Some((last, extended)) => {
                let delta = timestamp.wrapping_sub(last) as i32;
                if delta < 0 {
                    return;
                }
                extended + delta as u64
            }
            None => u64::from(timestamp),
        };
        self.last_timestamp = Some((timestamp, extended));
        let transit_ms =
            at.saturating_duration_since(first).as_secs_f64() * 1_000.0 - extended as f64 / 90.0;
        let minimum = self.minimum_transit_ms.get_or_insert(transit_ms);
        *minimum = minimum.min(transit_ms);
        self.latest_transit_ms = transit_ms;
        if let Some(previous) = self.previous_transit_ms {
            self.jitter_ms += ((transit_ms - previous).abs() - self.jitter_ms) / 16.0;
        }
        self.previous_transit_ms = Some(transit_ms);
        self.transits[self.transit_next] = transit_ms;
        self.transit_next = (self.transit_next + 1) % TRANSIT_SAMPLES;
        self.transit_count = (self.transit_count + 1).min(TRANSIT_SAMPLES);

        if let Some(frame) = self.active_frame.as_mut()
            && frame.number == frame_number
        {
            if sequence <= frame.last_sequence {
                frame.contiguous = false;
                return;
            }
            if sequence != frame.last_sequence + 1 {
                frame.contiguous = false;
            }
            let sender_gap_ms = extended.saturating_sub(frame.last_timestamp) as f64 / 90.0;
            let receive_gap_ms =
                at.saturating_duration_since(frame.last_at).as_secs_f64() * 1_000.0;
            if sender_gap_ms >= 1.0 {
                frame.sender_gap_ms += sender_gap_ms.min(receive_gap_ms);
            }
            frame.last_at = at;
            frame.last_timestamp = extended;
            frame.last_sequence = sequence;
            frame.bytes = frame.bytes.saturating_add(bytes as u64);
            frame.ended |= ended;
            return;
        }

        if let Some(previous) = self.active_frame.take() {
            let distance = frame_number.wrapping_sub(previous.number);
            if distance == 0 || distance >= 0x8000_0000 {
                self.active_frame = Some(previous);
                return;
            }
            self.lossy_frames = self.lossy_frames.saturating_add(distance.saturating_sub(1));
            if previous.started && previous.ended && previous.contiguous {
                let frame_ms = extended.saturating_sub(previous.first_timestamp) as f64 / 90.0;
                let received_ms = (previous
                    .last_at
                    .saturating_duration_since(previous.first_at)
                    .as_secs_f64()
                    * 1_000.0
                    - previous.sender_gap_ms)
                    .max(0.5);
                if frame_ms > 0.0 && frame_ms < 1_000.0 {
                    self.average_frame_ms = Some(smooth(self.average_frame_ms, frame_ms));
                    self.average_receive_kbps = Some(smooth(
                        self.average_receive_kbps,
                        previous.bytes as f64 * 8.0 / received_ms,
                    ));
                    self.average_utilization_percent = Some(smooth(
                        self.average_utilization_percent,
                        (100.0 * received_ms / self.average_frame_ms.unwrap()).min(100.0),
                    ));
                    self.frames_observed = self.frames_observed.saturating_add(1);
                    if self.frames_observed % 60 == 0 {
                        self.minimum_transit_ms =
                            self.minimum_transit_ms.map(|minimum| minimum + 0.5);
                    }
                }
            } else {
                self.lossy_frames = self.lossy_frames.saturating_add(1);
            }
        }
        self.active_frame = Some(FrameSample {
            number: frame_number,
            first_at: at,
            last_at: at,
            first_timestamp: extended,
            last_timestamp: extended,
            last_sequence: sequence,
            bytes: bytes as u64,
            sender_gap_ms: 0.0,
            started,
            ended,
            contiguous: true,
        });
    }

    pub(crate) fn report(&self, at: Instant) -> BandwidthFeedback {
        let (Some(first), Some(minimum)) = (self.first_arrival, self.minimum_transit_ms) else {
            return BandwidthFeedback::default();
        };
        let mut samples = self.transits;
        samples[..self.transit_count].sort_by(f64::total_cmp);
        let median = samples[(self.transit_count - 1) / 2];
        let now_ms = at.saturating_duration_since(first).as_secs_f64() * 1_000.0;
        let server_time =
            |transit: f64| (((now_ms - transit).max(0.0) * 90.0) as u64 & 0x7fff_ffff) as u32;
        BandwidthFeedback {
            minimum_server_time: server_time(minimum),
            median_server_time: server_time(median),
            queue_delay_us: ((self.latest_transit_ms - minimum).max(0.0) * 1_000.0) as u32,
            jitter_us: (self.jitter_ms * 1_000.0).min(f64::from(u16::MAX)) as u16,
            utilization_percent: if self.frames_observed >= 100 {
                self.average_utilization_percent.unwrap_or_default() as u8
            } else {
                0
            },
            estimate_kbps: if self.frames_observed >= 100 {
                self.average_receive_kbps.unwrap_or_default() as u32
            } else {
                0
            },
            lossy_frames: self.lossy_frames,
        }
    }
}

fn smooth(previous: Option<f64>, value: f64) -> f64 {
    previous.map_or(value, |previous| previous * 0.875 + value * 0.125)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn packet(
        frame_number: u32,
        sequence: u32,
        timestamp: u32,
        bytes: usize,
        flags: (bool, bool),
        received_at: Instant,
    ) -> PacketBandwidthSample {
        PacketBandwidthSample {
            frame_number,
            sequence,
            timestamp,
            bytes,
            started: flags.0,
            ended: flags.1,
            received_at,
        }
    }

    #[test]
    fn packet_train_reports_measured_capacity_after_one_hundred_frames() {
        let mut estimator = BandwidthEstimator::default();
        let origin = Instant::now();
        for frame in 0..=100_u32 {
            let at = origin + Duration::from_millis(u64::from(frame) * 20);
            let timestamp = 90_000 + frame * 1_800;
            for part in 0..3_u32 {
                estimator.observe(packet(
                    frame,
                    frame * 3 + part,
                    timestamp,
                    200,
                    (part == 0, part == 2),
                    at + Duration::from_micros(u64::from(part) * 500),
                ));
            }
        }
        let report = estimator.report(origin + Duration::from_millis(2_001));
        assert_eq!(report.minimum_server_time, 270_090);
        assert_eq!(report.estimate_kbps, 4_800);
        assert_eq!(report.utilization_percent, 5);
        assert_eq!(report.queue_delay_us, 1_000);
        assert!(report.jitter_us > 0);
        assert_eq!(report.lossy_frames, 0);
        assert_ne!(report.minimum_server_time, report.median_server_time);
    }

    #[test]
    fn incomplete_and_reordered_frames_do_not_inflate_the_estimate() {
        let mut estimator = BandwidthEstimator::default();
        let origin = Instant::now();
        estimator.observe(packet(1, 100, 90_000, 1_200, (true, false), origin));
        estimator.observe(packet(
            1,
            102,
            90_000,
            1_200,
            (false, true),
            origin + Duration::from_millis(1),
        ));
        estimator.observe(packet(
            3,
            103,
            93_600,
            1_200,
            (true, true),
            origin + Duration::from_millis(40),
        ));
        estimator.observe(packet(
            2,
            101,
            91_800,
            1_200,
            (true, true),
            origin + Duration::from_millis(41),
        ));
        assert_eq!(
            estimator
                .report(origin + Duration::from_millis(50))
                .lossy_frames,
            2
        );
        assert_eq!(estimator.frames_observed, 0);
        assert!(estimator.average_receive_kbps.is_none());
    }

    #[test]
    fn sender_timestamp_wrap_preserves_estimated_server_clock() {
        let mut estimator = BandwidthEstimator::default();
        let origin = Instant::now();
        estimator.observe(packet(1, 10, u32::MAX - 899, 300, (true, true), origin));
        estimator.observe(packet(
            2,
            11,
            901,
            300,
            (true, true),
            origin + Duration::from_millis(20),
        ));
        let report = estimator.report(origin + Duration::from_millis(20));
        assert!(report.minimum_server_time.abs_diff(900) <= 1);
        assert_eq!(estimator.frames_observed, 1);
    }

    #[test]
    fn parity_between_video_shards_does_not_invalidate_a_frame() {
        let mut estimator = BandwidthEstimator::default();
        let origin = Instant::now();
        estimator.observe(packet(1, 10, 90_000, 400, (true, false), origin));
        estimator.observe_parity(1, 11);
        estimator.observe(packet(
            1,
            12,
            90_000,
            400,
            (false, true),
            origin + Duration::from_millis(1),
        ));
        estimator.observe(packet(
            2,
            13,
            91_800,
            400,
            (true, true),
            origin + Duration::from_millis(20),
        ));
        assert_eq!(estimator.frames_observed, 1);
        assert_eq!(estimator.lossy_frames, 0);
        assert_eq!(estimator.average_receive_kbps, Some(6_400.0));
    }
}
