use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const STAGE_SAMPLE_CAPACITY: usize = 256;

pub const IN_FLIGHT_CAPACITY: usize = 64;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DecodeStagePercentiles {
    pub p50_us: u64,
    pub p95_us: u64,
    pub max_us: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct DecodeTimings {
    pub call: Option<DecodeStagePercentiles>,
    pub residence: Option<DecodeStagePercentiles>,
    pub call_window_samples: usize,
    pub residence_window_samples: usize,
    pub submissions_total: u64,
    pub outputs_total: u64,
    pub output_calls_total: u64,
    pub last_submission_at: Option<Instant>,
    pub last_output_at: Option<Instant>,
    pub in_flight: usize,
    pub oldest_in_flight_at: Option<Instant>,
    pub epoch: u64,
    pub epoch_started_at: Option<Instant>,
    pub unmatched_outputs: u64,
    pub unmatched_submissions: u64,
    pub duplicate_timestamps: u64,
}

impl DecodeTimings {
    pub fn is_empty(&self) -> bool {
        self.call.is_none() && self.residence.is_none()
    }

    pub fn has_progress(&self) -> bool {
        self.outputs_total > 0
    }

    pub fn has_observable_state(&self) -> bool {
        !self.is_empty() || self.submissions_total > 0 || self.outputs_total > 0
    }

    pub fn progress_reference(&self) -> Option<Instant> {
        let this_epoch_output = self
            .last_output_at
            .filter(|at| self.epoch_started_at.is_none_or(|floor| *at >= floor));
        let outstanding = self.oldest_in_flight_at;
        match (this_epoch_output, outstanding) {
            (Some(last), Some(oldest)) => Some(last.max(oldest)),
            (Some(last), None) => Some(last),
            (None, Some(oldest)) => Some(
                self.epoch_started_at
                    .map_or(oldest, |floor| oldest.max(floor)),
            ),
            (None, None) => None,
        }
    }
}

#[derive(Clone, Default)]
pub struct DecodeTimingProbe {
    inner: Arc<Mutex<DecodeTimingAccumulator>>,
}

impl DecodeTimingProbe {
    fn with<R>(&self, op: impl FnOnce(&mut DecodeTimingAccumulator) -> R) -> R {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        op(&mut inner)
    }

    pub fn record_submission(&self, timestamp_us: u64) {
        self.with(|inner| inner.record_submission(timestamp_us, Instant::now()));
    }

    pub fn record_output(&self, timestamp_us: u64) {
        self.with(|inner| inner.record_output(timestamp_us, Instant::now()));
    }

    pub fn record_call(&self, duration: Duration, produced_output: bool) {
        self.with(|inner| inner.record_call(duration, produced_output));
    }

    pub fn clear(&self) {
        self.with(|inner| inner.clear());
    }

    pub fn snapshot(&self) -> DecodeTimings {
        self.with(|inner| inner.snapshot())
    }
}

#[derive(Debug, Default)]
struct DecodeTimingAccumulator {
    in_flight: VecDeque<(u64, Instant)>,
    call_us: VecDeque<u64>,
    residence_us: VecDeque<u64>,
    submissions_total: u64,
    outputs_total: u64,
    output_calls_total: u64,
    last_submission_at: Option<Instant>,
    last_output_at: Option<Instant>,
    unmatched_outputs: u64,
    unmatched_submissions: u64,
    duplicate_timestamps: u64,
    epoch: u64,
    epoch_started_at: Option<Instant>,
}

impl DecodeTimingAccumulator {
    fn clear(&mut self) {
        let retired = self.in_flight.len() as u64;
        self.in_flight.clear();
        self.unmatched_submissions = self.unmatched_submissions.saturating_add(retired);
        self.epoch = self.epoch.saturating_add(1);
        self.epoch_started_at = Some(Instant::now());
    }

    fn record_submission(&mut self, timestamp_us: u64, at: Instant) {
        if self
            .in_flight
            .iter()
            .any(|(queued, _)| *queued == timestamp_us)
        {
            self.duplicate_timestamps = self.duplicate_timestamps.saturating_add(1);
        }
        while self.in_flight.len() >= IN_FLIGHT_CAPACITY {
            self.in_flight.pop_front();
            self.unmatched_submissions = self.unmatched_submissions.saturating_add(1);
        }
        self.in_flight.push_back((timestamp_us, at));
        self.submissions_total = self.submissions_total.saturating_add(1);
        self.last_submission_at = Some(at);
    }

    fn record_output(&mut self, timestamp_us: u64, at: Instant) {
        self.outputs_total = self.outputs_total.saturating_add(1);
        self.last_output_at = Some(at);
        let Some(position) = self
            .in_flight
            .iter()
            .position(|(queued, _)| *queued == timestamp_us)
        else {
            self.unmatched_outputs = self.unmatched_outputs.saturating_add(1);
            return;
        };
        let (_, submitted_at) = self.in_flight.remove(position).expect("position found");
        push_sample(
            &mut self.residence_us,
            at.saturating_duration_since(submitted_at),
        );
    }

    fn record_call(&mut self, duration: Duration, produced_output: bool) {
        if !produced_output {
            return;
        }
        self.output_calls_total = self.output_calls_total.saturating_add(1);
        push_sample(&mut self.call_us, duration);
    }

    fn snapshot(&self) -> DecodeTimings {
        DecodeTimings {
            call: summarize(&self.call_us),
            residence: summarize(&self.residence_us),
            call_window_samples: self.call_us.len(),
            residence_window_samples: self.residence_us.len(),
            submissions_total: self.submissions_total,
            outputs_total: self.outputs_total,
            output_calls_total: self.output_calls_total,
            last_submission_at: self.last_submission_at,
            last_output_at: self.last_output_at,
            in_flight: self.in_flight.len(),
            oldest_in_flight_at: self.in_flight.front().map(|(_, at)| *at),
            epoch: self.epoch,
            epoch_started_at: self.epoch_started_at,
            unmatched_outputs: self.unmatched_outputs,
            unmatched_submissions: self.unmatched_submissions,
            duplicate_timestamps: self.duplicate_timestamps,
        }
    }
}

fn push_sample(samples: &mut VecDeque<u64>, value: Duration) {
    while samples.len() >= STAGE_SAMPLE_CAPACITY {
        samples.pop_front();
    }
    samples.push_back(value.as_nanos().try_into().unwrap_or(u64::MAX));
}

fn summarize(samples: &VecDeque<u64>) -> Option<DecodeStagePercentiles> {
    if samples.is_empty() {
        return None;
    }
    let mut sorted: Vec<u64> = samples.iter().copied().collect();
    sorted.sort_unstable();
    Some(DecodeStagePercentiles {
        p50_us: percentile_us(&sorted, 50),
        p95_us: percentile_us(&sorted, 95),
        max_us: *sorted.last().expect("non-empty") / 1_000,
    })
}

fn percentile_us(sorted: &[u64], percentile: u32) -> u64 {
    let rank = (sorted.len() * percentile as usize).div_ceil(100).max(1);
    sorted[rank.min(sorted.len()) - 1] / 1_000
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(base: Instant, millis: u64) -> Instant {
        base + Duration::from_millis(millis)
    }

    #[test]
    fn empty_probe_reports_no_measured_stage() {
        let timings = DecodeTimingProbe::default().snapshot();
        assert!(timings.is_empty());
        assert!(!timings.has_progress());
        assert_eq!(timings.call_window_samples, 0);
        assert_eq!(timings.residence_window_samples, 0);
        assert_eq!(timings.submissions_total, 0);
        assert_eq!(timings.outputs_total, 0);
        assert_eq!(timings.output_calls_total, 0);
        assert!(timings.last_submission_at.is_none());
        assert!(timings.last_output_at.is_none());
        assert_eq!(timings.in_flight, 0);
        assert_eq!(timings.unmatched_outputs, 0);
        assert_eq!(timings.unmatched_submissions, 0);
        assert_eq!(timings.duplicate_timestamps, 0);
    }

    #[test]
    fn matched_submission_and_output_measure_residence() {
        let base = Instant::now();
        let mut accumulator = DecodeTimingAccumulator::default();
        accumulator.record_submission(4_500, base);
        accumulator.record_output(4_500, at(base, 7));
        let timings = accumulator.snapshot();
        let residence = timings.residence.expect("residence sample");
        assert_eq!(residence.p50_us, 7_000);
        assert_eq!(residence.p95_us, 7_000);
        assert_eq!(residence.max_us, 7_000);
        assert_eq!(timings.in_flight, 0);
        assert_eq!(timings.unmatched_outputs, 0);
        assert_eq!(timings.submissions_total, 1);
        assert_eq!(timings.outputs_total, 1);
        assert_eq!(timings.last_submission_at, Some(base));
        assert_eq!(timings.last_output_at, Some(at(base, 7)));
        assert!(timings.has_progress());
    }

    #[test]
    fn submit_only_calls_are_not_reported_as_decode_time() {
        let mut accumulator = DecodeTimingAccumulator::default();
        accumulator.record_call(Duration::from_micros(300), false);
        accumulator.record_call(Duration::from_micros(2_500), true);
        let timings = accumulator.snapshot();
        let call = timings.call.expect("call sample");
        assert_eq!(call.p50_us, 2_500);
        assert_eq!(timings.call_window_samples, 1);
        assert_eq!(timings.output_calls_total, 1);
    }

    #[test]
    fn output_without_a_recorded_submission_is_counted_not_estimated() {
        let mut accumulator = DecodeTimingAccumulator::default();
        accumulator.record_output(9_000, Instant::now());
        let timings = accumulator.snapshot();
        assert!(timings.residence.is_none());
        assert_eq!(timings.unmatched_outputs, 1);
    }

    #[test]
    fn earlier_in_flight_submissions_survive_a_later_output() {
        let base = Instant::now();
        let mut accumulator = DecodeTimingAccumulator::default();
        accumulator.record_submission(1_000, base);
        accumulator.record_submission(2_000, at(base, 1));
        accumulator.record_output(2_000, at(base, 5));
        let timings = accumulator.snapshot();
        assert_eq!(
            timings.residence.expect("residence").p50_us,
            4_000,
            "the matched submission is the one for this output"
        );
        assert_eq!(
            timings.in_flight, 1,
            "an earlier submission is not retired by a later output"
        );
        assert_eq!(timings.unmatched_submissions, 0);
        accumulator.record_output(1_000, at(base, 30));
        let timings = accumulator.snapshot();
        assert_eq!(
            timings.residence.expect("residences").max_us,
            30_000,
            "the earlier submission still produces its own residence sample"
        );
        assert_eq!(timings.in_flight, 0);
        assert_eq!(timings.outputs_total, 2);
    }

    #[test]
    fn out_of_order_outputs_match_their_own_submissions() {
        let base = Instant::now();
        let mut accumulator = DecodeTimingAccumulator::default();
        for timestamp in [1_000, 2_000, 3_000] {
            accumulator.record_submission(timestamp, base);
        }
        accumulator.record_output(3_000, at(base, 10));
        accumulator.record_output(1_000, at(base, 20));
        accumulator.record_output(2_000, at(base, 30));
        let timings = accumulator.snapshot();
        assert_eq!(timings.in_flight, 0);
        assert_eq!(timings.unmatched_outputs, 0);
        assert_eq!(timings.outputs_total, 3);
        let residence = timings.residence.expect("residence samples");
        assert_eq!(residence.max_us, 30_000);
        assert_eq!(residence.p50_us, 20_000);
        assert_eq!(residence.p95_us, 30_000);
    }

    #[test]
    fn unmatched_outputs_still_count_as_decoder_progress() {
        let base = Instant::now();
        let mut accumulator = DecodeTimingAccumulator::default();
        accumulator.record_output(9_000, base);
        let timings = accumulator.snapshot();
        assert_eq!(timings.unmatched_outputs, 1);
        assert_eq!(timings.outputs_total, 1);
        assert_eq!(timings.last_output_at, Some(base));
        assert!(timings.residence.is_none());
        assert!(timings.has_progress());
    }

    #[test]
    fn duplicate_identifiers_are_reported_and_matched_oldest_first() {
        let base = Instant::now();
        let mut accumulator = DecodeTimingAccumulator::default();
        accumulator.record_submission(3_000, base);
        accumulator.record_submission(3_000, at(base, 10));
        accumulator.record_output(3_000, at(base, 20));
        let timings = accumulator.snapshot();
        assert_eq!(timings.duplicate_timestamps, 1);
        assert_eq!(
            timings.residence.expect("residence").p50_us,
            20_000,
            "the oldest in-flight submission owns the earlier frame"
        );
        assert_eq!(timings.in_flight, 1);
    }

    #[test]
    fn in_flight_queue_stays_bounded_and_reports_evictions() {
        let mut accumulator = DecodeTimingAccumulator::default();
        for timestamp in 0..(IN_FLIGHT_CAPACITY as u64 + 4) {
            accumulator.record_submission(timestamp, Instant::now());
        }
        let timings = accumulator.snapshot();
        assert_eq!(timings.in_flight, IN_FLIGHT_CAPACITY);
        assert_eq!(timings.unmatched_submissions, 4);
    }

    #[test]
    fn clear_starts_a_new_epoch_and_accounts_retired_submissions() {
        let base = Instant::now();
        let mut accumulator = DecodeTimingAccumulator::default();
        accumulator.record_submission(700, base);
        accumulator.record_output(700, at(base, 3));
        accumulator.record_call(Duration::from_millis(2), true);
        accumulator.record_submission(800, base);
        assert_eq!(accumulator.snapshot().epoch, 0);
        accumulator.clear();
        let timings = accumulator.snapshot();
        assert_eq!(timings.in_flight, 0);
        assert_eq!(timings.oldest_in_flight_at, None);
        assert_eq!(timings.epoch, 1);
        assert!(timings.epoch_started_at.is_some());
        assert_eq!(
            timings.unmatched_submissions, 1,
            "the dropped in-flight submission is accounted, not silently forgotten"
        );
        assert_eq!(timings.call_window_samples, 1);
        assert_eq!(timings.residence_window_samples, 1);
        assert_eq!(timings.outputs_total, 1);
        accumulator.record_output(800, at(base, 9));
        assert_eq!(accumulator.snapshot().unmatched_outputs, 1);
    }

    #[test]
    fn a_decoder_hung_on_its_first_submission_is_still_observable() {
        let base = Instant::now();
        let mut accumulator = DecodeTimingAccumulator::default();
        accumulator.record_submission(1_000, base);
        let timings = accumulator.snapshot();
        assert!(timings.is_empty());
        assert!(!timings.has_progress());
        assert!(
            timings.has_observable_state(),
            "input with no output yet is decoder work Task3 must be able to see"
        );
        assert_eq!(timings.submissions_total, 1);
        assert_eq!(timings.outputs_total, 0);
        assert_eq!(timings.in_flight, 1);
        assert_eq!(timings.progress_reference(), Some(base));
    }

    #[test]
    fn progress_reference_ignores_outputs_from_before_the_epoch() {
        let base = Instant::now();
        let mut accumulator = DecodeTimingAccumulator::default();
        accumulator.record_submission(1_000, base);
        accumulator.record_output(1_000, at(base, 50));
        accumulator.clear();
        let cleared = accumulator.snapshot();
        let epoch_started = cleared.epoch_started_at.expect("epoch boundary");
        accumulator.record_submission(2_000, at(base, 60));
        let timings = accumulator.snapshot();
        let reference = timings.progress_reference().expect("outstanding work");
        assert!(
            reference >= epoch_started,
            "a stale pre-epoch output must not make the decoder look like it just progressed"
        );
        assert_eq!(reference, at(base, 60));
    }

    #[test]
    fn progress_reference_is_absent_without_outputs_or_outstanding_work() {
        let accumulator = DecodeTimingAccumulator::default();
        assert_eq!(accumulator.snapshot().progress_reference(), None);
    }

    #[test]
    fn stage_rings_stay_bounded_under_continued_load() {
        let base = Instant::now();
        let mut accumulator = DecodeTimingAccumulator::default();
        for timestamp in 0..(STAGE_SAMPLE_CAPACITY as u64 * 2) {
            accumulator.record_submission(timestamp, base);
            accumulator.record_output(timestamp, at(base, 1));
            accumulator.record_call(Duration::from_micros(500), true);
        }
        let timings = accumulator.snapshot();
        assert_eq!(timings.call_window_samples, STAGE_SAMPLE_CAPACITY);
        assert_eq!(timings.residence_window_samples, STAGE_SAMPLE_CAPACITY);
        assert_eq!(timings.unmatched_outputs, 0);
        assert_eq!(timings.unmatched_submissions, 0);
        assert_eq!(
            timings.outputs_total,
            STAGE_SAMPLE_CAPACITY as u64 * 2,
            "cumulative output totals keep counting after the window saturates"
        );
        assert_eq!(timings.submissions_total, STAGE_SAMPLE_CAPACITY as u64 * 2);
    }

    #[test]
    fn percentiles_use_nearest_rank_measured_values() {
        let mut accumulator = DecodeTimingAccumulator::default();
        for millis in 1..=20 {
            accumulator.record_call(Duration::from_millis(millis), true);
        }
        let call = accumulator.snapshot().call.expect("call samples");
        assert_eq!(call.p50_us, 10_000);
        assert_eq!(call.p95_us, 19_000);
        assert_eq!(call.max_us, 20_000);
    }
}
