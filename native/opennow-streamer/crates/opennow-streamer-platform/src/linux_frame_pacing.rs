use std::time::{Duration, Instant};

const PACING_OUTLIER: Duration = Duration::from_millis(75);
const MAX_ADAPTIVE_QUEUE_DEPTH: usize = 2;
const JITTER_QUEUE_THRESHOLD: f64 = 0.25;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameSelectionPolicy {
    OldestReady,
    LatestReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PacingDecision {
    pub(crate) selection: FrameSelectionPolicy,
    pub(crate) target_queue_depth: usize,
}

/// Display-clocked frame scheduler modelled after the native GFN client's
/// timestamp/adaptive queue. Decode is allowed to run independently, but only
/// one selected frame is submitted to the FIFO swapchain per presentation
/// interval. Fast streams deliberately sample the newest decoded frame rather
/// than building a backlog that WSI must discard later.
#[derive(Debug)]
pub(crate) struct LinuxFramePacer {
    stream_interval: Duration,
    display_interval: Option<Duration>,
    presentation_interval: Duration,
    fast_stream: bool,
    vrr_enabled: bool,
    last_arrival: Option<(Instant, u64)>,
    jitter_ema_ns: f64,
    jitter_bound_ns: f64,
    next_deadline: Option<Instant>,
}

impl LinuxFramePacer {
    pub(crate) fn new(stream_fps: u32, display_refresh_hz: Option<u32>, vrr_enabled: bool) -> Self {
        let stream_interval = frame_interval(stream_fps);
        let display_interval = display_refresh_hz.map(frame_interval);
        let (presentation_interval, fast_stream) = pacing_mode(stream_interval, display_interval);
        Self {
            stream_interval,
            display_interval,
            presentation_interval,
            fast_stream,
            vrr_enabled,
            last_arrival: None,
            jitter_ema_ns: 0.0,
            jitter_bound_ns: 0.0,
            next_deadline: None,
        }
    }

    pub(crate) fn reconfigure(&mut self, stream_fps: u32, display_refresh_hz: Option<u32>) -> bool {
        let stream_interval = frame_interval(stream_fps);
        let display_interval = display_refresh_hz.map(frame_interval);
        if self.stream_interval == stream_interval && self.display_interval == display_interval {
            return false;
        }
        self.stream_interval = stream_interval;
        self.display_interval = display_interval;
        (self.presentation_interval, self.fast_stream) =
            pacing_mode(stream_interval, display_interval);
        self.reset();
        true
    }

    pub(crate) fn reset(&mut self) {
        self.last_arrival = None;
        self.jitter_ema_ns = 0.0;
        self.jitter_bound_ns = 0.0;
        self.next_deadline = None;
    }

    pub(crate) fn is_due(&self, now: Instant) -> bool {
        if self.vrr_enabled {
            return true;
        }
        self.next_deadline.is_none_or(|deadline| now >= deadline)
    }

    pub(crate) fn decision(&self, now: Instant) -> PacingDecision {
        // With compositor VRR enabled, submit the freshest complete server
        // frame immediately. FIFO provides tear-free scanout while the output
        // refresh follows server arrival timing; replaying a burst after a
        // hitch would defeat Cloud G-SYNC and add input latency.
        if self.vrr_enabled {
            return PacingDecision {
                selection: FrameSelectionPolicy::LatestReady,
                target_queue_depth: 0,
            };
        }
        let stalled = self
            .next_deadline
            .is_some_and(|deadline| now.saturating_duration_since(deadline) > PACING_OUTLIER);
        if self.fast_stream || stalled {
            return PacingDecision {
                selection: FrameSelectionPolicy::LatestReady,
                target_queue_depth: 0,
            };
        }
        PacingDecision {
            selection: FrameSelectionPolicy::OldestReady,
            target_queue_depth: self.adaptive_queue_depth(),
        }
    }

    pub(crate) fn observe_frame(&mut self, arrived_at: Instant, timestamp_us: u64) {
        let Some((previous_arrival, previous_timestamp_us)) =
            self.last_arrival.replace((arrived_at, timestamp_us))
        else {
            return;
        };
        let arrival_interval = arrived_at.saturating_duration_since(previous_arrival);
        let Some(timestamp_delta_us) = timestamp_us.checked_sub(previous_timestamp_us) else {
            self.clear_timing_history();
            return;
        };
        let media_interval = Duration::from_micros(timestamp_delta_us);
        if arrival_interval.is_zero()
            || media_interval.is_zero()
            || arrival_interval > PACING_OUTLIER
            || media_interval > PACING_OUTLIER
        {
            self.clear_timing_history();
            return;
        }

        let jitter_ns = arrival_interval.abs_diff(media_interval).as_nanos() as f64;
        self.jitter_ema_ns = if self.jitter_ema_ns == 0.0 {
            jitter_ns
        } else {
            self.jitter_ema_ns * 0.90 + jitter_ns * 0.10
        };
        // Keep a decaying bound so a transient burst is absorbed, but does not
        // leave the stream permanently buffered after conditions recover.
        self.jitter_bound_ns = (self.jitter_bound_ns * 0.98).max(jitter_ns);
    }

    pub(crate) fn mark_presented(&mut self, now: Instant) {
        if self.vrr_enabled {
            self.next_deadline = None;
            return;
        }
        let next = self
            .next_deadline
            .map_or(now + self.presentation_interval, |deadline| {
                deadline + self.presentation_interval
            });
        // Never replay missed deadlines. A hitch rebases the presentation
        // clock and stale frames are handled by `LatestReady` on the next tick.
        self.next_deadline = Some(if next <= now {
            now + self.presentation_interval
        } else {
            next
        });
    }

    pub(crate) const fn fast_stream(&self) -> bool {
        self.fast_stream
    }

    pub(crate) const fn vrr_enabled(&self) -> bool {
        self.vrr_enabled
    }

    pub(crate) fn presentation_hz(&self) -> f64 {
        1.0 / self.presentation_interval.as_secs_f64()
    }

    fn adaptive_queue_depth(&self) -> usize {
        if self.jitter_bound_ns == 0.0 {
            return 0;
        }
        let interval_ns = self.presentation_interval.as_nanos() as f64;
        let target_ns = (self.jitter_ema_ns + self.jitter_bound_ns * 0.5) * 1.25;
        if target_ns < interval_ns * JITTER_QUEUE_THRESHOLD {
            return 0;
        }
        ((target_ns / interval_ns).ceil() as usize).min(MAX_ADAPTIVE_QUEUE_DEPTH)
    }

    fn clear_timing_history(&mut self) {
        self.last_arrival = None;
        self.jitter_ema_ns = 0.0;
        self.jitter_bound_ns = 0.0;
        self.next_deadline = None;
    }
}

fn frame_interval(fps: u32) -> Duration {
    Duration::from_secs_f64(1.0 / f64::from(fps.max(1)))
}

fn pacing_mode(stream_interval: Duration, display_interval: Option<Duration>) -> (Duration, bool) {
    let Some(display_interval) = display_interval else {
        return (stream_interval, false);
    };
    // Matches the official client's observed mismatch boundary: a server
    // interval below 75% of the display interval is treated as a fast stream.
    let fast_stream = stream_interval.as_secs_f64() < display_interval.as_secs_f64() * 0.75;
    (stream_interval.max(display_interval), fast_stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fast_stream_samples_latest_at_display_rate() {
        let pacer = LinuxFramePacer::new(240, Some(165), false);
        assert!(pacer.fast_stream());
        assert_eq!(
            pacer.decision(Instant::now()).selection,
            FrameSelectionPolicy::LatestReady
        );
        assert!((pacer.presentation_hz() - 165.0).abs() < 0.01);
    }

    #[test]
    fn matched_stream_uses_ordered_adaptive_queue() {
        let pacer = LinuxFramePacer::new(120, Some(165), false);
        assert!(!pacer.fast_stream());
        assert_eq!(
            pacer.decision(Instant::now()),
            PacingDecision {
                selection: FrameSelectionPolicy::OldestReady,
                target_queue_depth: 0,
            }
        );
        assert!((pacer.presentation_hz() - 120.0).abs() < 0.01);
    }

    #[test]
    fn missed_deadline_is_rebased_without_catch_up_burst() {
        let mut pacer = LinuxFramePacer::new(120, Some(120), false);
        let origin = Instant::now();
        pacer.mark_presented(origin);
        let late = origin + Duration::from_millis(20);
        pacer.mark_presented(late);
        assert!(!pacer.is_due(late));
        assert!(pacer.is_due(late + Duration::from_millis(9)));
    }

    #[test]
    fn sustained_arrival_jitter_adds_bounded_queue_depth() {
        let mut pacer = LinuxFramePacer::new(120, Some(165), false);
        let origin = Instant::now();
        pacer.observe_frame(origin, 0);
        for index in 1..20_u64 {
            pacer.observe_frame(
                origin + Duration::from_micros(index * 13_000),
                index * 8_333,
            );
        }
        let decision = pacer.decision(origin + Duration::from_millis(300));
        assert_eq!(decision.selection, FrameSelectionPolicy::OldestReady);
        assert!(decision.target_queue_depth > 0);
        assert!(decision.target_queue_depth <= MAX_ADAPTIVE_QUEUE_DEPTH);
    }

    #[test]
    fn vrr_submits_latest_frame_without_a_fixed_deadline() {
        let mut pacer = LinuxFramePacer::new(120, Some(165), true);
        let now = Instant::now();
        assert!(pacer.is_due(now));
        assert!(pacer.vrr_enabled());
        assert_eq!(
            pacer.decision(now),
            PacingDecision {
                selection: FrameSelectionPolicy::LatestReady,
                target_queue_depth: 0,
            }
        );
        pacer.mark_presented(now);
        assert!(pacer.is_due(now));
    }
}
