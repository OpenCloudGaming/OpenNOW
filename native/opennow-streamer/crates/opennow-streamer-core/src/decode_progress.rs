use std::time::{Duration, Instant};

use opennow_streamer_platform::DecodeTimingsReport;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecodeProgressPolicy {
    pub stall: Duration,
    pub keyframe_grace: Duration,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum DecodeProgressStage {
    #[default]
    Tracking,
    KeyframePending,
    RecoveryRequired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeProgressEvent {
    KeyframeRequested {
        idle_for: Duration,
        in_flight: usize,
    },
    RecoveryNeeded {
        idle_for: Duration,
        in_flight: usize,
    },
}

#[derive(Debug, Default)]
pub(crate) struct DecodeProgressWatchdog {
    stage: DecodeProgressStage,
    keyframe_requested_at: Option<Instant>,
    decoder_epoch: Option<u64>,
    unsubmitted_since: Option<Instant>,
}

impl DecodeProgressWatchdog {
    pub(crate) fn stage(&self) -> DecodeProgressStage {
        self.stage
    }

    pub(crate) fn poll(
        &mut self,
        timings: &DecodeTimingsReport,
        upstream_stalled: bool,
        last_assembled_at: Option<Instant>,
        now: Instant,
        policy: DecodeProgressPolicy,
    ) -> Option<DecodeProgressEvent> {
        if self.decoder_epoch != Some(timings.epoch) {
            self.decoder_epoch = Some(timings.epoch);
            self.clear_episode();
            self.unsubmitted_since = None;
        }
        if self.stage == DecodeProgressStage::RecoveryRequired {
            return None;
        }
        if upstream_stalled {
            self.clear_episode();
            self.unsubmitted_since = None;
            return None;
        }
        let reference = if timings.in_flight == 0 {
            if !last_assembled_at.is_some_and(|assembled| {
                now.saturating_duration_since(assembled) < policy.stall
                    && timings
                        .last_output_at
                        .is_none_or(|output| assembled > output)
            }) {
                self.clear_episode();
                self.unsubmitted_since = None;
                return None;
            }
            let first = *self.unsubmitted_since.get_or_insert(now);
            if timings.last_output_at.is_some_and(|output| output >= first) {
                self.clear_episode();
                self.unsubmitted_since = None;
                return None;
            }
            first
        } else {
            self.unsubmitted_since = None;
            progress_reference(timings)?
        };
        let idle_for = now.saturating_duration_since(reference);
        if idle_for < policy.stall {
            self.clear_episode();
            return None;
        }
        match self.stage {
            DecodeProgressStage::Tracking => {
                self.stage = DecodeProgressStage::KeyframePending;
                self.keyframe_requested_at = Some(now);
                Some(DecodeProgressEvent::KeyframeRequested {
                    idle_for,
                    in_flight: timings.in_flight,
                })
            }
            DecodeProgressStage::KeyframePending => {
                let grace_started = self.keyframe_requested_at.unwrap_or(now);
                if now.saturating_duration_since(grace_started) < policy.keyframe_grace {
                    return None;
                }
                self.stage = DecodeProgressStage::RecoveryRequired;
                Some(DecodeProgressEvent::RecoveryNeeded {
                    idle_for,
                    in_flight: timings.in_flight,
                })
            }
            DecodeProgressStage::RecoveryRequired => None,
        }
    }

    fn clear_episode(&mut self) {
        self.stage = DecodeProgressStage::Tracking;
        self.keyframe_requested_at = None;
    }
}

pub(crate) fn progress_reference(timings: &DecodeTimingsReport) -> Option<Instant> {
    let this_epoch_output = timings
        .last_output_at
        .filter(|at| timings.epoch_started_at.is_none_or(|floor| *at >= floor));
    match (this_epoch_output, timings.oldest_in_flight_at) {
        (Some(last), Some(oldest)) => Some(last.max(oldest)),
        (Some(last), None) => Some(last),
        (None, Some(oldest)) => Some(
            timings
                .epoch_started_at
                .map_or(oldest, |floor| oldest.max(floor)),
        ),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn timings(epoch: u64, epoch_started_at: Option<Instant>) -> DecodeTimingsReport {
        DecodeTimingsReport {
            call: None,
            residence: None,
            call_window_samples: 0,
            residence_window_samples: 0,
            submissions_total: 0,
            outputs_total: 0,
            output_calls_total: 0,
            last_submission_at: None,
            last_output_at: None,
            in_flight: 0,
            oldest_in_flight_at: None,
            epoch,
            epoch_started_at,
            unmatched_outputs: 0,
            unmatched_submissions: 0,
        }
    }

    fn policy() -> DecodeProgressPolicy {
        DecodeProgressPolicy {
            stall: Duration::from_secs(8),
            keyframe_grace: Duration::from_secs(4),
        }
    }

    fn single_pending_submission(base: Instant) -> DecodeTimingsReport {
        let mut report = timings(1, Some(base));
        report.submissions_total = 1;
        report.in_flight = 1;
        report.oldest_in_flight_at = Some(base);
        report.last_submission_at = Some(base);
        report
    }

    #[test]
    fn no_outstanding_work_never_reports_a_decode_stall() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let report = timings(1, Some(base));
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(60),
                policy()
            ),
            None
        );
        assert_eq!(watchdog.stage(), DecodeProgressStage::Tracking);
    }

    #[test]
    fn arriving_video_without_decoder_submissions_requests_recovery() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let mut report = timings(1, Some(base));
        report.last_output_at = Some(base);
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                Some(base + Duration::from_secs(1)),
                base + Duration::from_secs(1),
                policy()
            ),
            None
        );
        for second in 2..9 {
            assert_eq!(
                watchdog.poll(
                    &report,
                    false,
                    Some(base + Duration::from_secs(second)),
                    base + Duration::from_secs(second),
                    policy()
                ),
                None
            );
        }
        assert!(matches!(
            watchdog.poll(
                &report,
                false,
                Some(base + Duration::from_secs(9)),
                base + Duration::from_secs(9),
                policy()
            ),
            Some(DecodeProgressEvent::KeyframeRequested { in_flight: 0, .. })
        ));
        assert!(matches!(
            watchdog.poll(
                &report,
                false,
                Some(base + Duration::from_secs(13)),
                base + Duration::from_secs(13),
                policy()
            ),
            Some(DecodeProgressEvent::RecoveryNeeded { in_flight: 0, .. })
        ));
    }

    #[test]
    fn stale_assembly_and_resumed_output_do_not_trigger_recovery() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let mut report = timings(1, Some(base));
        report.last_output_at = Some(base);
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                Some(base + Duration::from_secs(1)),
                base + Duration::from_secs(1),
                policy()
            ),
            None
        );
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                Some(base + Duration::from_secs(1)),
                base + Duration::from_secs(10),
                policy()
            ),
            None
        );
        report.last_output_at = Some(base + Duration::from_secs(11));
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                Some(base + Duration::from_secs(12)),
                base + Duration::from_secs(12),
                policy()
            ),
            None
        );
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                Some(base + Duration::from_secs(19)),
                base + Duration::from_secs(19),
                policy()
            ),
            None
        );
        assert_eq!(watchdog.stage(), DecodeProgressStage::Tracking);
    }

    #[test]
    fn upstream_stall_restarts_the_unsubmitted_frame_grace_period() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let report = timings(1, Some(base));
        assert_eq!(
            watchdog.poll(&report, false, Some(base), base, policy()),
            None
        );
        assert_eq!(
            watchdog.poll(
                &report,
                true,
                Some(base + Duration::from_secs(7)),
                base + Duration::from_secs(7),
                policy()
            ),
            None
        );
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                Some(base + Duration::from_secs(10)),
                base + Duration::from_secs(10),
                policy()
            ),
            None
        );
        assert!(matches!(
            watchdog.poll(
                &report,
                false,
                Some(base + Duration::from_secs(18)),
                base + Duration::from_secs(18),
                policy()
            ),
            Some(DecodeProgressEvent::KeyframeRequested { in_flight: 0, .. })
        ));
    }

    #[test]
    fn one_pending_submission_with_a_stale_assembly_timestamp_still_escalates() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let report = single_pending_submission(base);
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(7),
                policy()
            ),
            None
        );
        assert!(matches!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(8),
                policy()
            ),
            Some(DecodeProgressEvent::KeyframeRequested { in_flight: 1, .. })
        ));
        assert!(matches!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(12),
                policy()
            ),
            Some(DecodeProgressEvent::RecoveryNeeded { in_flight: 1, .. })
        ));
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(60),
                policy()
            ),
            None
        );
    }

    #[test]
    fn fresh_output_clears_a_pending_keyframe_episode() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let mut report = single_pending_submission(base);
        assert!(matches!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(8),
                policy()
            ),
            Some(DecodeProgressEvent::KeyframeRequested { .. })
        ));
        report.last_output_at = Some(base + Duration::from_secs(9));
        report.oldest_in_flight_at = Some(base + Duration::from_secs(9));
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(9),
                policy()
            ),
            None
        );
        assert_eq!(watchdog.stage(), DecodeProgressStage::Tracking);
        for second in 10..40 {
            report.last_output_at = Some(base + Duration::from_secs(second));
            report.oldest_in_flight_at = Some(base + Duration::from_secs(second));
            assert_eq!(
                watchdog.poll(
                    &report,
                    false,
                    None,
                    base + Duration::from_secs(second),
                    policy()
                ),
                None
            );
        }
        assert_eq!(watchdog.stage(), DecodeProgressStage::Tracking);
    }

    #[test]
    fn zero_work_clears_a_pending_episode_and_later_work_starts_fresh() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let mut report = single_pending_submission(base);
        assert!(matches!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(8),
                policy()
            ),
            Some(DecodeProgressEvent::KeyframeRequested { .. })
        ));
        report.in_flight = 0;
        report.oldest_in_flight_at = None;
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(9),
                policy()
            ),
            None
        );
        assert_eq!(watchdog.stage(), DecodeProgressStage::Tracking);

        let later = base + Duration::from_secs(100);
        report.in_flight = 1;
        report.oldest_in_flight_at = Some(later);
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                None,
                later + Duration::from_secs(1),
                policy()
            ),
            None
        );
        assert_eq!(watchdog.stage(), DecodeProgressStage::Tracking);
        assert!(matches!(
            watchdog.poll(
                &report,
                false,
                None,
                later + Duration::from_secs(8),
                policy()
            ),
            Some(DecodeProgressEvent::KeyframeRequested { .. })
        ));
    }

    #[test]
    fn upstream_ownership_suppresses_until_the_transport_stage_resolves() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let report = single_pending_submission(base);
        assert_eq!(
            watchdog.poll(
                &report,
                true,
                None,
                base + Duration::from_secs(30),
                policy()
            ),
            None
        );
        assert_eq!(watchdog.stage(), DecodeProgressStage::Tracking);
        assert!(matches!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(31),
                policy()
            ),
            Some(DecodeProgressEvent::KeyframeRequested { .. })
        ));
    }

    #[test]
    fn idle_outputs_before_the_epoch_do_not_mask_a_stall() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let mut report = timings(2, Some(base + Duration::from_secs(10)));
        report.last_output_at = Some(base);
        report.in_flight = 1;
        report.oldest_in_flight_at = Some(base + Duration::from_secs(10));
        assert!(matches!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(19),
                policy()
            ),
            Some(DecodeProgressEvent::KeyframeRequested { .. })
        ));
    }

    #[test]
    fn decoder_epoch_change_resets_a_terminal_episode() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let report = single_pending_submission(base);
        watchdog.poll(
            &report,
            false,
            None,
            base + Duration::from_secs(8),
            policy(),
        );
        watchdog.poll(
            &report,
            false,
            None,
            base + Duration::from_secs(12),
            policy(),
        );
        assert_eq!(watchdog.stage(), DecodeProgressStage::RecoveryRequired);

        let mut restarted = timings(2, Some(base + Duration::from_secs(20)));
        restarted.in_flight = 1;
        restarted.oldest_in_flight_at = Some(base + Duration::from_secs(20));
        assert_eq!(
            watchdog.poll(
                &restarted,
                false,
                None,
                base + Duration::from_secs(27),
                policy()
            ),
            None
        );
        assert!(matches!(
            watchdog.poll(
                &restarted,
                false,
                None,
                base + Duration::from_secs(28),
                policy()
            ),
            Some(DecodeProgressEvent::KeyframeRequested { .. })
        ));
    }

    #[test]
    fn recovering_epoch_does_not_escalate_from_a_prior_epoch_output() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let mut report = single_pending_submission(base);
        report.last_output_at = Some(base - Duration::from_secs(30));
        report.epoch = 4;
        report.epoch_started_at = Some(base);
        assert_eq!(
            watchdog.poll(
                &report,
                false,
                None,
                base + Duration::from_secs(1),
                policy()
            ),
            None
        );
        assert_eq!(watchdog.stage(), DecodeProgressStage::Tracking);
    }

    #[test]
    fn advancing_decode_output_keeps_the_watchdog_tracking() {
        let mut watchdog = DecodeProgressWatchdog::default();
        let base = Instant::now();
        let mut report = timings(1, Some(base));
        report.in_flight = 2;
        report.oldest_in_flight_at = Some(base);
        report.last_output_at = Some(base);
        for second in 1..20 {
            report.last_output_at = Some(base + Duration::from_secs(second));
            report.oldest_in_flight_at = Some(base + Duration::from_secs(second));
            assert_eq!(
                watchdog.poll(
                    &report,
                    false,
                    None,
                    base + Duration::from_secs(second + 1),
                    policy()
                ),
                None
            );
        }
        assert_eq!(watchdog.stage(), DecodeProgressStage::Tracking);
    }
}
