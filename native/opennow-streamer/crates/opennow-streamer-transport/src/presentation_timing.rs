use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const CAPACITY: usize = 1024;
const MAX_AGE: Duration = Duration::from_secs(2);
const REPORT_INTERVAL: Duration = crate::nvst_control::FRAME_PACING_INTERVAL;

pub const PRESENTATION_PLATFORM_PRESENTED: u32 = 3;
pub const PRESENTATION_RENDER_DURATION: u32 = 1;
pub const PRESENTATION_COMPLETION_DURATION: u32 = 2;
pub const PRESENTATION_VSYNC_INTERVAL: u32 = 4;
pub const PRESENTATION_COMPLETION_AGE: u32 = 8;
pub const PRESENTATION_REQUIRED_FIELDS: u32 = 15;

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationEstimatorConfig {
    pub jitter_mode: u32,
    pub jitter_n: usize,
    pub jitter_stddevs_x1000: u32,
    pub jitter_outlier_us: u32,
    pub jitter_min_advance_us: u32,
    pub jitter_max_advance_us: u32,
    pub render_mode: u32,
    pub render_n: usize,
    pub render_stddevs_x1000: u32,
    pub render_min_time_us: u32,
    pub render_max_time_us: u32,
}

impl PresentationEstimatorConfig {
    fn supported(&self) -> bool {
        self.jitter_mode == 1
            && self.render_mode == 1
            && (2..=256).contains(&self.jitter_n)
            && (2..=256).contains(&self.render_n)
            && self.jitter_stddevs_x1000 <= 100_000
            && self.render_stddevs_x1000 <= 100_000
            && self.jitter_outlier_us > 0
            && self.jitter_outlier_us <= 2_000_000
            && self.jitter_min_advance_us <= self.jitter_max_advance_us
            && self.jitter_max_advance_us <= 2_000_000
            && self.render_min_time_us <= self.render_max_time_us
            && self.render_max_time_us > 0
            && self.render_max_time_us <= 2_000_000
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PresentationConfig {
    pub generation: u64,
    pub scene_epoch: u64,
    pub active: bool,
    pub fps: u32,
    pub cloud_gsync: bool,
    pub feedback_mode: u32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PresentationSample {
    pub generation: u64,
    pub scene_epoch: u64,
    pub presentation_time_ns: u64,
    pub render_duration_ns: u64,
    pub render_present_to_complete_ns: u64,
    pub completion_age_ns: u64,
    pub vsync_interval_ns: u64,
    pub completion_kind: u32,
    pub valid_fields: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PresentationReport {
    pub stream_index: u32,
    pub frame_count: u32,
    pub first_frame: u32,
    pub processing_budget_us: u32,
    pub present_delay_us: u32,
    pub vsync_interval_us: u32,
}

#[derive(Debug)]
struct Admission {
    timestamp_ns: u64,
    frame: u32,
    at: Instant,
    ambiguous: bool,
    sampled: bool,
}

#[derive(Debug, Clone, Copy)]
struct Measured {
    frame: u32,
    processing_us: u32,
    present_us: u32,
    vsync_us: u32,
    at: Instant,
}

#[derive(Debug, Default)]
struct State {
    config: PresentationConfig,
    windows_pts: bool,
    estimator: Option<PresentationEstimatorConfig>,
    next_estimator: Option<PresentationEstimatorConfig>,
    render_samples: VecDeque<f64>,
    jitter_samples: VecDeque<f64>,
    present_samples: VecDeque<f64>,
    previous: Option<(u64, Instant)>,
    last_admitted_pts: Option<u64>,
    admissions: VecDeque<Admission>,
    pending: VecDeque<(PresentationSample, Instant)>,
    measured: Option<Measured>,
    last_reported: Option<u32>,
    last_send: Option<Instant>,
}

#[derive(Debug, Default)]
pub struct PresentationTimingState {
    state: Mutex<State>,
}

impl PresentationTimingState {
    pub fn config(&self) -> PresentationConfig {
        self.state.lock().unwrap_or_else(|e| e.into_inner()).config
    }

    pub fn activate(
        &self,
        generation: u64,
        fps: u32,
        cloud_gsync: bool,
        mode: u32,
        windows_pts: bool,
    ) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let scene_epoch = state.config.scene_epoch.wrapping_add(1);
        let estimator = state.next_estimator.take();
        *state = State {
            config: PresentationConfig {
                generation,
                scene_epoch,
                active: true,
                fps,
                cloud_gsync,
                feedback_mode: u32::from(mode == 1),
            },
            windows_pts,
            estimator,
            ..State::default()
        };
    }

    pub fn configure_estimator(&self, config: Option<PresentationEstimatorConfig>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        invalidate(&mut state);
        state.next_estimator = config.filter(PresentationEstimatorConfig::supported);
        state.estimator = None;
        if state.next_estimator.is_none() {
            opennow_streamer_protocol::log::log_line(
                "INFO",
                "presentation",
                "timing feedback suppressed: negotiated estimator settings are missing or unsupported",
            );
        }
    }

    pub fn deactivate(&self) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.config.active = false;
        invalidate(&mut state);
    }

    pub fn invalidate_scene(&self) {
        invalidate(&mut self.state.lock().unwrap_or_else(|e| e.into_inner()));
    }

    pub fn set_feedback_mode(&self, mode: u32) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mode = u32::from(mode == 1);
        if state.config.feedback_mode != mode {
            state.config.feedback_mode = mode;
            invalidate(&mut state);
        }
    }

    pub fn admit(
        &self,
        generation: u64,
        frame: u32,
        timestamp: u64,
        clock_rate_hz: u32,
        at: Instant,
    ) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.config.active
            || state.config.generation != generation
            || !state.windows_pts
            || clock_rate_hz == 0
        {
            return;
        }
        let ticks = u128::from(timestamp) * 10_000_000 / u128::from(clock_rate_hz);
        let Ok(ticks) = i64::try_from(ticks) else {
            return;
        };
        let Some(timestamp_ns) = (ticks as u64).checked_mul(100) else {
            return;
        };
        if timestamp_ns == 0 {
            return;
        }
        if let Some(existing) = state
            .admissions
            .iter_mut()
            .find(|entry| entry.timestamp_ns == timestamp_ns)
        {
            existing.ambiguous = true;
            state.measured = None;
            return;
        }
        if state
            .last_admitted_pts
            .is_some_and(|previous| timestamp_ns <= previous)
        {
            invalidate(&mut state);
        }
        state
            .admissions
            .retain(|entry| at.saturating_duration_since(entry.at) <= MAX_AGE);
        if state.admissions.len() >= CAPACITY {
            invalidate(&mut state);
        }
        state.last_admitted_pts = Some(timestamp_ns);
        state.admissions.push_back(Admission {
            timestamp_ns,
            frame,
            at,
            ambiguous: false,
            sampled: false,
        });
        let pending = std::mem::take(&mut state.pending);
        for (sample, received_at) in pending {
            if at.saturating_duration_since(received_at) <= MAX_AGE
                && !apply_sample(&mut state, sample, received_at)
                && state.pending.len() < CAPACITY
            {
                state.pending.push_back((sample, received_at));
            }
        }
    }

    pub fn submit(&self, sample: PresentationSample, now: Instant) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !valid_sample(&state, &sample) {
            return false;
        }
        if !apply_sample(&mut state, sample, now) {
            while state.pending.len() >= CAPACITY {
                state.pending.pop_front();
            }
            state.pending.push_back((sample, now));
        }
        true
    }

    pub fn send_due(&self, now: Instant, send: impl FnOnce(PresentationReport) -> bool) -> bool {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.config.active
            || state.config.feedback_mode != 1
            || state
                .last_send
                .is_some_and(|last| now.saturating_duration_since(last) < REPORT_INTERVAL)
        {
            return false;
        }
        let Some(measured) = state.measured else {
            return false;
        };
        if now.saturating_duration_since(measured.at) > MAX_AGE {
            state.measured = None;
            return false;
        }
        let baseline = state
            .last_reported
            .unwrap_or(measured.frame.wrapping_sub(1));
        let count = measured.frame.wrapping_sub(baseline);
        if count == 0 || count > i32::MAX as u32 {
            return false;
        }
        let report = PresentationReport {
            stream_index: 0,
            frame_count: count,
            first_frame: baseline.wrapping_add(1),
            processing_budget_us: measured.processing_us,
            present_delay_us: measured.present_us,
            vsync_interval_us: measured.vsync_us,
        };
        if !send(report) {
            return false;
        }
        state.last_reported = Some(measured.frame);
        state.last_send = Some(now);
        true
    }
}

fn invalidate(state: &mut State) {
    state.config.scene_epoch = state.config.scene_epoch.wrapping_add(1);
    state.admissions.clear();
    state.pending.clear();
    state.measured = None;
    state.last_reported = None;
    state.last_send = None;
    state.previous = None;
    state.last_admitted_pts = None;
    state.render_samples.clear();
    state.jitter_samples.clear();
    state.present_samples.clear();
}

fn valid_sample(state: &State, sample: &PresentationSample) -> bool {
    state.config.active
        && state.windows_pts
        && state.config.generation == sample.generation
        && state.config.scene_epoch == sample.scene_epoch
        && sample.presentation_time_ns != 0
        && sample.completion_kind == PRESENTATION_PLATFORM_PRESENTED
        && sample.valid_fields == PRESENTATION_REQUIRED_FIELDS
        && sample.render_duration_ns > 0
        && sample.render_duration_ns <= MAX_AGE.as_nanos() as u64
        && sample.render_present_to_complete_ns <= MAX_AGE.as_nanos() as u64
        && sample.completion_age_ns <= MAX_AGE.as_nanos() as u64
        && sample.vsync_interval_ns <= 1_000_000_000
}

fn apply_sample(state: &mut State, sample: PresentationSample, received_at: Instant) -> bool {
    if !valid_sample(state, &sample) {
        return true;
    }
    let Some(admission) = state
        .admissions
        .iter_mut()
        .find(|entry| entry.timestamp_ns == sample.presentation_time_ns)
    else {
        return false;
    };
    if admission.ambiguous || admission.sampled {
        return true;
    }
    let Some(render_present) = received_at
        .checked_sub(Duration::from_nanos(sample.completion_age_ns))
        .and_then(|complete| {
            complete.checked_sub(Duration::from_nanos(sample.render_present_to_complete_ns))
        })
    else {
        return true;
    };
    let Some(since_admission) = render_present.checked_duration_since(admission.at) else {
        return true;
    };
    if since_admission > MAX_AGE
        || since_admission.as_nanos() < u128::from(sample.render_duration_ns)
    {
        return true;
    }
    admission.sampled = true;
    if state
        .measured
        .is_some_and(|last| admission.frame.wrapping_sub(last.frame) > i32::MAX as u32)
    {
        return true;
    }
    let frame = admission.frame;
    let Some(estimator) = state.estimator.as_ref() else {
        return true;
    };
    let previous = state
        .previous
        .replace((sample.presentation_time_ns, render_present));
    let Some((previous_pts, previous_present)) = previous else {
        return true;
    };
    let Some(source_delta) = sample
        .presentation_time_ns
        .checked_sub(previous_pts)
        .filter(|delta| *delta > 0)
    else {
        invalidate(state);
        return true;
    };
    let Some(present_delta) = render_present.checked_duration_since(previous_present) else {
        invalidate(state);
        return true;
    };
    let error_us = present_delta.as_secs_f64() * 1_000_000.0 - source_delta as f64 / 1_000.0;
    push_sample(
        &mut state.jitter_samples,
        error_us.clamp(
            -f64::from(estimator.jitter_outlier_us),
            f64::from(estimator.jitter_outlier_us),
        ),
        estimator.jitter_n,
    );
    push_sample(
        &mut state.render_samples,
        (sample.render_duration_ns as f64 / 1_000.0).min(f64::from(estimator.render_max_time_us)),
        estimator.render_n,
    );
    push_sample(
        &mut state.present_samples,
        sample.render_present_to_complete_ns as f64 / 1_000.0,
        estimator.render_n,
    );
    if state.jitter_samples.len() < 2 || state.render_samples.len() < 2 {
        return true;
    }
    let (_, jitter_variance) = moments(&state.jitter_samples);
    let (render_mean, render_variance) = moments(&state.render_samples);
    let jitter = (jitter_variance.sqrt() * f64::from(estimator.jitter_stddevs_x1000) / 1_000.0)
        .clamp(
            f64::from(estimator.jitter_min_advance_us),
            f64::from(estimator.jitter_max_advance_us),
        );
    let render = (render_mean
        + render_variance.sqrt() * f64::from(estimator.render_stddevs_x1000) / 1_000.0)
        .clamp(
            f64::from(estimator.render_min_time_us),
            f64::from(estimator.render_max_time_us),
        );
    state.measured = Some(Measured {
        frame,
        processing_us: (jitter + render) as u32,
        present_us: moments(&state.present_samples).0 as u32,
        vsync_us: (sample.vsync_interval_ns / 1_000) as u32,
        at: received_at,
    });
    true
}

fn push_sample(samples: &mut VecDeque<f64>, value: f64, capacity: usize) {
    while samples.len() >= capacity {
        samples.pop_front();
    }
    samples.push_back(value);
}

fn moments(samples: &VecDeque<f64>) -> (f64, f64) {
    let mean = samples.iter().sum::<f64>() / samples.len() as f64;
    let variance = samples
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / samples.len() as f64;
    (mean, variance)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collector() -> PresentationTimingState {
        let timing = PresentationTimingState::default();
        timing.configure_estimator(Some(PresentationEstimatorConfig {
            jitter_mode: 1,
            jitter_n: 8,
            jitter_stddevs_x1000: 1_000,
            jitter_outlier_us: 50_000,
            jitter_min_advance_us: 100,
            jitter_max_advance_us: 5_000,
            render_mode: 1,
            render_n: 8,
            render_stddevs_x1000: 1_000,
            render_min_time_us: 500,
            render_max_time_us: 10_000,
        }));
        timing.activate(7, 120, true, 1, true);
        timing
    }

    fn sample(timing: &PresentationTimingState, pts: u64) -> PresentationSample {
        let config = timing.config();
        PresentationSample {
            generation: config.generation,
            scene_epoch: config.scene_epoch,
            presentation_time_ns: pts,
            render_duration_ns: 2_000_000,
            render_present_to_complete_ns: 5_000_000,
            completion_age_ns: 3_000_000,
            vsync_interval_ns: 0,
            completion_kind: PRESENTATION_PLATFORM_PRESENTED,
            valid_fields: PRESENTATION_REQUIRED_FIELDS,
        }
    }

    fn observe(timing: &PresentationTimingState, frame: u32, tick: u64, present: Instant) {
        timing.admit(7, frame, tick, 90_000, present - Duration::from_millis(50));
        let pts = (u128::from(tick) * 10_000_000 / 90_000) as u64 * 100;
        assert!(timing.submit(sample(timing, pts), present + Duration::from_millis(8)));
    }

    fn warm(timing: &PresentationTimingState, frame: u32, now: Instant) {
        for index in 0..3 {
            observe(
                timing,
                frame.wrapping_add(index),
                900 + u64::from(index) * 900,
                now + Duration::from_millis(u64::from(index) * 10),
            );
        }
    }

    #[test]
    fn measured_budget_is_jitter_plus_render_not_admission_or_present_wait() {
        let timing = collector();
        let now = Instant::now();
        warm(&timing, 40, now);
        assert!(timing.send_due(now + Duration::from_millis(30), |report| {
            assert_eq!(
                report,
                PresentationReport {
                    stream_index: 0,
                    frame_count: 1,
                    first_frame: 42,
                    processing_budget_us: 2_100,
                    present_delay_us: 5_000,
                    vsync_interval_us: 0,
                }
            );
            true
        }));
    }

    #[test]
    fn unknown_settings_cadence_or_completion_never_send() {
        let timing = PresentationTimingState::default();
        timing.activate(7, 120, true, 1, true);
        let now = Instant::now();
        warm(&timing, 40, now);
        assert!(
            !timing.send_due(now + Duration::from_millis(30), |_| panic!(
                "unknown estimator"
            ))
        );
        let timing = collector();
        observe(&timing, 40, 900, now);
        assert!(
            !timing.send_due(now + Duration::from_millis(30), |_| panic!(
                "unknown cadence"
            ))
        );
        let mut unsupported = sample(&timing, 10_000_000);
        for kind in [0, 1, 2, 4] {
            unsupported.completion_kind = kind;
            assert!(!timing.submit(unsupported, now));
        }
        unsupported.completion_kind = PRESENTATION_PLATFORM_PRESENTED;
        unsupported.valid_fields &= !PRESENTATION_VSYNC_INTERVAL;
        assert!(!timing.submit(unsupported, now));
    }

    #[test]
    fn reports_preserve_sender_gaps_wrap_and_failed_send_baseline() {
        let timing = collector();
        let now = Instant::now();
        warm(&timing, u32::MAX - 2, now);
        let first_send = now + Duration::from_millis(30);
        assert!(timing.send_due(first_send, |report| {
            assert_eq!(report.first_frame, u32::MAX);
            true
        }));
        observe(&timing, 2, 3600, now + Duration::from_millis(90));
        assert!(
            !timing.send_due(now + Duration::from_millis(100), |report| {
                assert_eq!((report.first_frame, report.frame_count), (0, 3));
                false
            })
        );
        assert!(timing.send_due(now + Duration::from_millis(100), |report| {
            assert_eq!((report.first_frame, report.frame_count), (0, 3));
            true
        }));
        assert!(!timing.send_due(now + Duration::from_millis(200), |_| panic!("duplicate")));
    }

    #[test]
    fn epochs_and_feedback_mode_changes_discard_late_observations() {
        let timing = collector();
        let now = Instant::now();
        warm(&timing, 40, now);
        let old = sample(&timing, 30_000_000);
        timing.invalidate_scene();
        assert!(!timing.submit(old, now));
        assert!(!timing.send_due(now + Duration::from_secs(1), |_| panic!("stale scene")));
        timing.set_feedback_mode(0);
        warm(&timing, 50, now);
        assert!(!timing.send_due(now + Duration::from_millis(30), |_| panic!("disabled")));
        timing.set_feedback_mode(1);
        warm(&timing, 60, now);
        assert!(timing.send_due(now + Duration::from_millis(30), |_| true));
        timing.deactivate();
        assert!(!timing.submit(sample(&timing, 30_000_000), now));
        timing.activate(8, 60, false, 1, true);
        assert!(!timing.submit(old, now));
        assert_eq!(timing.config().generation, 8);
        assert!(!timing.config().cloud_gsync);
    }

    #[test]
    fn exact_windows_rounding_and_ambiguous_timestamp_suppression() {
        let timing = collector();
        let now = Instant::now();
        timing.admit(7, 4, 1, 90_000, now);
        assert_eq!(
            timing
                .state
                .lock()
                .unwrap()
                .admissions
                .front()
                .unwrap()
                .timestamp_ns,
            11_100
        );
        timing.admit(7, 5, 1, 90_000, now);
        assert!(
            timing
                .state
                .lock()
                .unwrap()
                .admissions
                .front()
                .unwrap()
                .ambiguous
        );
        assert!(timing.submit(sample(&timing, 11_100), now + Duration::from_millis(12)));
        assert!(timing.state.lock().unwrap().measured.is_none());
        timing.admit(7, 6, 0, 90_000, now);
        timing.admit(7, 7, 1, 0, now);
        assert_eq!(timing.state.lock().unwrap().admissions.len(), 1);
    }

    #[test]
    fn delayed_admission_matches_original_observation_time() {
        let timing = collector();
        let now = Instant::now();
        for index in 0..3 {
            let present = now + Duration::from_millis(index * 10);
            let pts = 10_000_000 + index * 10_000_000;
            assert!(timing.submit(sample(&timing, pts), present + Duration::from_millis(8)));
            timing.admit(
                7,
                40 + index as u32,
                900 + index * 900,
                90_000,
                present - Duration::from_millis(50),
            );
        }
        assert!(timing.send_due(now + Duration::from_millis(30), |report| {
            assert_eq!(report.processing_budget_us, 2_100);
            true
        }));
    }

    #[test]
    fn bounds_and_stale_data_do_not_manufacture_reports() {
        let timing = collector();
        let now = Instant::now();
        warm(&timing, 40, now);
        assert!(!timing.send_due(now + Duration::from_secs(3), |_| panic!("stale estimate")));
        let mut invalid = sample(&timing, 10_000_000);
        invalid.render_present_to_complete_ns = u64::MAX;
        assert!(!timing.submit(invalid, now));
        for index in 0..(CAPACITY * 2) {
            timing.admit(7, index as u32, 90_000 + index as u64, 90_000, now);
        }
        assert!(timing.state.lock().unwrap().admissions.len() <= CAPACITY);
        assert!(timing.config().scene_epoch > 2);
    }

    #[test]
    fn unsupported_backend_and_quantile_configuration_are_suppressed() {
        let timing = collector();
        timing.activate(7, 120, true, 1, false);
        assert!(!timing.submit(sample(&timing, 1), Instant::now()));
        let mut config = collector().state.lock().unwrap().estimator.clone().unwrap();
        config.jitter_mode = 0;
        timing.configure_estimator(Some(config));
        assert!(timing.state.lock().unwrap().estimator.is_none());
    }

    #[test]
    fn supported_maximum_frame_rate_does_not_periodically_invalidate_observations() {
        let timing = collector();
        let now = Instant::now();
        let epoch = timing.config().scene_epoch;
        for index in 0..3600 {
            timing.admit(
                7,
                index,
                90_000 + u64::from(index) * 250,
                90_000,
                now + Duration::from_nanos(u64::from(index) * 1_000_000_000 / 360),
            );
        }
        assert_eq!(timing.config().scene_epoch, epoch);
        assert!(timing.state.lock().unwrap().admissions.len() <= 721);
    }

    #[test]
    fn new_session_cannot_reuse_previous_negotiated_estimator() {
        let timing = collector();
        assert!(timing.state.lock().unwrap().estimator.is_some());
        timing.activate(8, 120, true, 1, true);
        assert!(timing.state.lock().unwrap().estimator.is_none());
    }

    #[test]
    fn negotiated_variance_weights_and_clamps_apply_to_measured_histories() {
        let timing = collector();
        let now = Instant::now();
        for (index, elapsed_ms, render_ns) in
            [(0, 0, 2_000_000), (1, 30, 1_000_000), (2, 40, 3_000_000)]
        {
            let present = now + Duration::from_millis(elapsed_ms);
            let tick = 900 + index * 900;
            timing.admit(
                7,
                40 + index as u32,
                tick,
                90_000,
                present - Duration::from_millis(50),
            );
            let mut measurement = sample(&timing, (index + 1) * 10_000_000);
            measurement.render_duration_ns = render_ns;
            timing.submit(measurement, present + Duration::from_millis(8));
        }
        assert!(timing.send_due(now + Duration::from_millis(50), |report| {
            assert_eq!(report.processing_budget_us, 8_000);
            true
        }));
    }

    #[test]
    fn reused_pts_after_history_expiry_invalidates_late_draw_epoch() {
        let timing = collector();
        let now = Instant::now();
        timing.admit(7, 40, 900, 90_000, now);
        let old = sample(&timing, 10_000_000);
        timing.admit(7, 41, 1800, 90_000, now + Duration::from_secs(3));
        timing.admit(7, 42, 900, 90_000, now + Duration::from_secs(3));
        assert!(!timing.submit(old, now + Duration::from_secs(3)));
    }
}
