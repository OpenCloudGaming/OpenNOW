use std::collections::VecDeque;
use std::time::Instant;

pub(crate) const STAGE_SAMPLE_CAPACITY: usize = 256;

pub(crate) const DELIVERY_STAMP_CAPACITY: usize = 512;

const NANOS_PER_MILLI: f64 = 1_000_000.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageSummary {
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct FrameStageTimings {
    pub delivery_to_admission: Option<StageSummary>,
    pub admission_to_control_queue: Option<StageSummary>,
    pub assembled_to_control_queue: Option<StageSummary>,
    pub delivery_window_samples: usize,
    pub ack_window_samples: usize,
    pub assembled_frames_total: u64,
    pub admitted_frames_total: u64,
    pub queued_ack_frames_total: u64,
    pub undelivered_frames_total: u64,
    pub last_assembled_at: Option<Instant>,
    pub last_admitted_at: Option<Instant>,
    pub last_ack_queued_at: Option<Instant>,
    pub pending_deliveries: usize,
    pub unmatched_deliveries: u64,
    pub unmatched_admissions: u64,
}

impl FrameStageTimings {
    pub fn is_empty(&self) -> bool {
        self.delivery_to_admission.is_none()
            && self.admission_to_control_queue.is_none()
            && self.assembled_to_control_queue.is_none()
    }

    pub fn has_progress(&self) -> bool {
        self.assembled_frames_total > 0
    }

    pub fn log_line(&self) -> String {
        let stage = |summary: Option<StageSummary>| match summary {
            Some(summary) => format!(
                "p50={:.2} p95={:.2} max={:.2}",
                summary.p50_ms, summary.p95_ms, summary.max_ms
            ),
            None => "unavailable".to_owned(),
        };
        format!(
            "deliveryToAdmission[{}]={} admissionToControlQueue[{}]={} assembledToControlQueue[{}]={} assembled={} admitted={} queuedAcks={} undelivered={} pendingDeliveries={} unmatchedDelivery={} unmatchedAdmission={}",
            self.delivery_window_samples,
            stage(self.delivery_to_admission),
            self.ack_window_samples,
            stage(self.admission_to_control_queue),
            self.ack_window_samples,
            stage(self.assembled_to_control_queue),
            self.assembled_frames_total,
            self.admitted_frames_total,
            self.queued_ack_frames_total,
            self.undelivered_frames_total,
            self.pending_deliveries,
            self.unmatched_deliveries,
            self.unmatched_admissions,
        )
    }
}

#[derive(Debug, Default)]
pub(crate) struct FrameStageTimingsAccumulator {
    delivered: VecDeque<(u32, Instant)>,
    delivery_to_admission_us: VecDeque<u64>,
    admission_to_control_queue_us: VecDeque<u64>,
    assembled_to_control_queue_us: VecDeque<u64>,
    assembled_frames_total: u64,
    admitted_frames_total: u64,
    queued_ack_frames_total: u64,
    undelivered_frames_total: u64,
    last_assembled_at: Option<Instant>,
    last_admitted_at: Option<Instant>,
    last_ack_queued_at: Option<Instant>,
    unmatched_deliveries: u64,
    unmatched_admissions: u64,
    epoch_started_at: Option<Instant>,
}

impl FrameStageTimingsAccumulator {
    pub(crate) fn record_assembly(&mut self, frame_index: u32, at: Instant) {
        if let Some(position) = self
            .delivered
            .iter()
            .position(|(delivered, _)| *delivered == frame_index)
        {
            self.delivered.remove(position);
            self.unmatched_deliveries = self.unmatched_deliveries.saturating_add(1);
        }
        while self.delivered.len() >= DELIVERY_STAMP_CAPACITY {
            self.delivered.pop_front();
            self.unmatched_deliveries = self.unmatched_deliveries.saturating_add(1);
        }
        self.delivered.push_back((frame_index, at));
        self.assembled_frames_total = self.assembled_frames_total.saturating_add(1);
        self.last_assembled_at = Some(at);
    }

    pub(crate) fn retire_undelivered(&mut self, frame_index: u32) {
        if let Some(position) = self
            .delivered
            .iter()
            .position(|(delivered, _)| *delivered == frame_index)
        {
            self.delivered.remove(position);
        }
        self.undelivered_frames_total = self.undelivered_frames_total.saturating_add(1);
    }

    pub(crate) fn record_admission(&mut self, frame_index: u32, at: Instant) -> Option<Instant> {
        self.admitted_frames_total = self.admitted_frames_total.saturating_add(1);
        self.last_admitted_at = Some(at);
        let Some(position) = self
            .delivered
            .iter()
            .position(|(delivered, _)| *delivered == frame_index)
        else {
            self.unmatched_admissions = self.unmatched_admissions.saturating_add(1);
            return None;
        };
        let (_, delivered_at) = self.delivered.remove(position).expect("position found");
        push_sample(
            &mut self.delivery_to_admission_us,
            at.saturating_duration_since(delivered_at),
        );
        Some(delivered_at)
    }

    pub(crate) fn record_ack_queued(
        &mut self,
        assembled_at: Option<Instant>,
        admitted_at: Instant,
        at: Instant,
    ) {
        self.queued_ack_frames_total = self.queued_ack_frames_total.saturating_add(1);
        self.last_ack_queued_at = Some(at);
        if self
            .epoch_started_at
            .is_some_and(|epoch_started_at| admitted_at < epoch_started_at)
        {
            return;
        }
        push_sample(
            &mut self.admission_to_control_queue_us,
            at.saturating_duration_since(admitted_at),
        );
        if let Some(assembled_at) = assembled_at {
            push_sample(
                &mut self.assembled_to_control_queue_us,
                at.saturating_duration_since(assembled_at),
            );
        }
    }

    pub(crate) fn reset_epoch(&mut self, at: Instant) {
        let retired = self.delivered.len() as u64;
        self.delivered.clear();
        self.unmatched_deliveries = self.unmatched_deliveries.saturating_add(retired);
        self.epoch_started_at = Some(at);
    }

    pub(crate) fn snapshot(&self) -> FrameStageTimings {
        FrameStageTimings {
            delivery_to_admission: summarize(&self.delivery_to_admission_us),
            admission_to_control_queue: summarize(&self.admission_to_control_queue_us),
            assembled_to_control_queue: summarize(&self.assembled_to_control_queue_us),
            delivery_window_samples: self.delivery_to_admission_us.len(),
            ack_window_samples: self.admission_to_control_queue_us.len(),
            assembled_frames_total: self.assembled_frames_total,
            admitted_frames_total: self.admitted_frames_total,
            queued_ack_frames_total: self.queued_ack_frames_total,
            undelivered_frames_total: self.undelivered_frames_total,
            last_assembled_at: self.last_assembled_at,
            last_admitted_at: self.last_admitted_at,
            last_ack_queued_at: self.last_ack_queued_at,
            pending_deliveries: self.delivered.len(),
            unmatched_deliveries: self.unmatched_deliveries,
            unmatched_admissions: self.unmatched_admissions,
        }
    }
}

fn push_sample(samples: &mut VecDeque<u64>, value: std::time::Duration) {
    while samples.len() >= STAGE_SAMPLE_CAPACITY {
        samples.pop_front();
    }
    samples.push_back(value.as_nanos().try_into().unwrap_or(u64::MAX));
}

fn summarize(samples: &VecDeque<u64>) -> Option<StageSummary> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted: Vec<u64> = samples.iter().copied().collect();
    sorted.sort_unstable();
    Some(StageSummary {
        p50_ms: percentile_ms(&sorted, 50),
        p95_ms: percentile_ms(&sorted, 95),
        max_ms: *sorted.last().expect("non-empty") as f64 / NANOS_PER_MILLI,
    })
}

fn percentile_ms(sorted: &[u64], percentile: u32) -> f64 {
    let rank = (sorted.len() * percentile as usize).div_ceil(100).max(1);
    sorted[rank.min(sorted.len()) - 1] as f64 / NANOS_PER_MILLI
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn at(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    #[test]
    fn empty_accumulator_reports_no_stage_instead_of_zero() {
        let timings = FrameStageTimingsAccumulator::default().snapshot();
        assert!(timings.is_empty());
        assert_eq!(timings.delivery_window_samples, 0);
        assert_eq!(timings.ack_window_samples, 0);
        assert_eq!(timings.assembled_frames_total, 0);
        assert_eq!(timings.admitted_frames_total, 0);
        assert_eq!(timings.queued_ack_frames_total, 0);
        assert!(timings.last_assembled_at.is_none());
        assert!(timings.last_admitted_at.is_none());
        assert!(timings.last_ack_queued_at.is_none());
        assert!(!timings.has_progress());
        assert_eq!(timings.pending_deliveries, 0);
        assert_eq!(timings.unmatched_deliveries, 0);
        assert_eq!(timings.unmatched_admissions, 0);
    }

    #[test]
    fn matched_samples_become_measured_stage_percentiles() {
        let base = Instant::now();
        let mut accumulator = FrameStageTimingsAccumulator::default();
        for frame in 1..=20_u32 {
            accumulator.record_assembly(frame, base);
            let assembled = accumulator
                .record_admission(frame, at(base, u64::from(frame)))
                .expect("delivery stamp");
            assert_eq!(assembled, base);
            accumulator.record_ack_queued(
                Some(assembled),
                at(base, u64::from(frame)),
                at(base, 100),
            );
        }
        let timings = accumulator.snapshot();
        let delivery = timings.delivery_to_admission.expect("delivery samples");
        assert_eq!(delivery.max_ms, 20.0);
        assert_eq!(delivery.p50_ms, 10.0);
        assert_eq!(delivery.p95_ms, 19.0);
        let ack = timings.admission_to_control_queue.expect("ack samples");
        assert_eq!(ack.max_ms, 99.0);
        assert_eq!(ack.p50_ms, 89.0);
        let age = timings.assembled_to_control_queue.expect("age samples");
        assert_eq!(age.max_ms, 100.0);
        assert_eq!(age.p50_ms, 100.0);
        assert_eq!(timings.delivery_window_samples, 20);
        assert_eq!(timings.ack_window_samples, 20);
        assert_eq!(timings.assembled_frames_total, 20);
        assert_eq!(timings.admitted_frames_total, 20);
        assert_eq!(timings.queued_ack_frames_total, 20);
        assert_eq!(timings.pending_deliveries, 0);
        assert_eq!(timings.last_assembled_at, Some(base));
        assert_eq!(timings.last_admitted_at, Some(at(base, 20)));
        assert_eq!(timings.last_ack_queued_at, Some(at(base, 100)));
        assert!(timings.has_progress());
    }

    #[test]
    fn admission_without_a_delivery_stamp_is_counted_not_estimated() {
        let base = Instant::now();
        let mut accumulator = FrameStageTimingsAccumulator::default();
        assert_eq!(accumulator.record_admission(7, base), None);
        let timings = accumulator.snapshot();
        assert!(timings.delivery_to_admission.is_none());
        assert_eq!(timings.unmatched_admissions, 1);
        assert_eq!(timings.pending_deliveries, 0);
    }

    #[test]
    fn evicted_delivery_stamps_are_counted_as_unmatched() {
        let base = Instant::now();
        let mut accumulator = FrameStageTimingsAccumulator::default();
        for frame in 0..DELIVERY_STAMP_CAPACITY as u32 {
            accumulator.record_assembly(frame, base);
        }
        accumulator.record_assembly(DELIVERY_STAMP_CAPACITY as u32, base);
        let timings = accumulator.snapshot();
        assert_eq!(timings.unmatched_deliveries, 1);
        assert_eq!(timings.pending_deliveries, DELIVERY_STAMP_CAPACITY);
        assert!(timings.delivery_to_admission.is_none());
    }

    #[test]
    fn repeated_frame_identifier_retires_the_older_stamp() {
        let base = Instant::now();
        let mut accumulator = FrameStageTimingsAccumulator::default();
        accumulator.record_assembly(5, base);
        accumulator.record_assembly(5, at(base, 40));
        assert_eq!(accumulator.snapshot().unmatched_deliveries, 1);
        let assembled = accumulator
            .record_admission(5, at(base, 50))
            .expect("newest stamp");
        assert_eq!(assembled, at(base, 40));
        let timings = accumulator.snapshot();
        assert_eq!(
            timings.delivery_to_admission.expect("measured").p50_ms,
            10.0
        );
        assert_eq!(timings.pending_deliveries, 0);
    }

    #[test]
    fn epoch_reset_retires_pending_stamps_without_fabricating_a_sample() {
        let base = Instant::now();
        let mut accumulator = FrameStageTimingsAccumulator::default();
        accumulator.record_assembly(7, base);
        accumulator.reset_epoch(at(base, 5_000));
        assert_eq!(accumulator.record_admission(7, at(base, 5_500)), None);
        let after_stale_admission = accumulator.snapshot();
        assert!(
            after_stale_admission.delivery_to_admission.is_none(),
            "a stale admission can never match a retired stamp"
        );
        assert_eq!(after_stale_admission.unmatched_deliveries, 1);
        assert_eq!(after_stale_admission.unmatched_admissions, 1);
        assert_eq!(after_stale_admission.pending_deliveries, 0);
        accumulator.record_assembly(7, at(base, 6_000));
        let assembled = accumulator
            .record_admission(7, at(base, 6_100))
            .expect("epoch-local stamp");
        assert_eq!(assembled, at(base, 6_000));
        let timings = accumulator.snapshot();
        assert_eq!(
            timings.delivery_to_admission.expect("measured").p50_ms,
            100.0
        );
        assert_eq!(timings.assembled_frames_total, 2);
        assert_eq!(timings.admitted_frames_total, 2);
    }

    #[test]
    fn acks_for_pre_epoch_admissions_report_no_cross_epoch_sample() {
        let base = Instant::now();
        let mut accumulator = FrameStageTimingsAccumulator::default();
        accumulator.record_assembly(3, base);
        let assembled = accumulator
            .record_admission(3, at(base, 20))
            .expect("delivery stamp");
        accumulator.reset_epoch(at(base, 4_000));
        accumulator.record_ack_queued(Some(assembled), at(base, 20), at(base, 4_100));
        let timings = accumulator.snapshot();
        assert!(
            timings.admission_to_control_queue.is_none(),
            "an ACK that spans the epoch boundary reports no stage sample"
        );
        assert!(timings.assembled_to_control_queue.is_none());
        assert_eq!(timings.queued_ack_frames_total, 1);
        assert_eq!(timings.last_ack_queued_at, Some(at(base, 4_100)));
        accumulator.record_assembly(4, at(base, 4_200));
        let assembled = accumulator
            .record_admission(4, at(base, 4_250))
            .expect("delivery stamp");
        accumulator.record_ack_queued(Some(assembled), at(base, 4_250), at(base, 4_300));
        let timings = accumulator.snapshot();
        assert_eq!(
            timings
                .admission_to_control_queue
                .expect("epoch-local ack")
                .p50_ms,
            50.0
        );
    }

    #[test]
    fn stage_rings_stay_bounded_under_continued_load() {
        let base = Instant::now();
        let mut accumulator = FrameStageTimingsAccumulator::default();
        for frame in 0..(STAGE_SAMPLE_CAPACITY as u32 * 3) {
            accumulator.record_assembly(frame, base);
            let assembled = accumulator
                .record_admission(frame, at(base, 1))
                .expect("delivery stamp");
            accumulator.record_ack_queued(Some(assembled), at(base, 1), at(base, 2));
        }
        let timings = accumulator.snapshot();
        assert_eq!(timings.delivery_window_samples, STAGE_SAMPLE_CAPACITY);
        assert_eq!(timings.ack_window_samples, STAGE_SAMPLE_CAPACITY);
        assert_eq!(timings.pending_deliveries, 0);
        assert_eq!(timings.unmatched_deliveries, 0);
        assert_eq!(
            timings.assembled_frames_total,
            u64::from(STAGE_SAMPLE_CAPACITY as u32 * 3),
            "cumulative totals keep counting after the window saturates"
        );
        assert_eq!(
            timings.queued_ack_frames_total,
            u64::from(STAGE_SAMPLE_CAPACITY as u32 * 3)
        );
    }

    #[test]
    fn ack_samples_require_admission_and_never_use_sender_time() {
        let base = Instant::now();
        let mut accumulator = FrameStageTimingsAccumulator::default();
        accumulator.record_ack_queued(None, base, at(base, 3));
        let timings = accumulator.snapshot();
        assert_eq!(
            timings
                .admission_to_control_queue
                .expect("admission sample")
                .p50_ms,
            3.0
        );
        assert!(timings.assembled_to_control_queue.is_none());
    }

    #[test]
    fn log_line_names_stages_and_reports_unavailable_honestly() {
        let empty = FrameStageTimings::default().log_line();
        assert!(empty.contains("deliveryToAdmission[0]=unavailable"));
        assert!(empty.contains("unmatchedDelivery=0"));

        let base = Instant::now();
        let mut accumulator = FrameStageTimingsAccumulator::default();
        accumulator.record_assembly(1, base);
        accumulator.record_admission(1, at(base, 4));
        accumulator.record_ack_queued(Some(base), at(base, 4), at(base, 9));
        let line = accumulator.snapshot().log_line();
        assert!(line.contains("deliveryToAdmission[1]=p50=4.00"));
        assert!(line.contains("assembledToControlQueue[1]=p50=9.00"));
    }

    #[test]
    fn percentiles_use_nearest_rank_and_never_exceed_the_sample_count() {
        let base = Instant::now();
        let mut accumulator = FrameStageTimingsAccumulator::default();
        accumulator.record_assembly(1, base);
        let assembled = accumulator.record_admission(1, at(base, 2)).expect("stamp");
        accumulator.record_ack_queued(Some(assembled), at(base, 2), at(base, 3));
        let timings = accumulator.snapshot();
        let delivery = timings.delivery_to_admission.expect("single sample");
        assert_eq!(delivery.p50_ms, 2.0);
        assert_eq!(delivery.p95_ms, 2.0);
        assert_eq!(delivery.max_ms, 2.0);
    }
}
