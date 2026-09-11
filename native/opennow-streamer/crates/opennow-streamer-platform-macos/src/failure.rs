use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

const DECODE_LOSS_CAPACITY: usize = 4;
const REPEATED_FAILURE_THRESHOLD: usize = 3;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendSubsystem {
    VideoToolbox,
    Metal,
    AudioWorker,
}

impl BackendSubsystem {
    pub const fn name(self) -> &'static str {
        match self {
            Self::VideoToolbox => "VideoToolbox",
            Self::Metal => "Metal",
            Self::AudioWorker => "audio worker",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendFailure {
    pub subsystem: BackendSubsystem,
    pub message: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VideoDecodeLoss {
    pub status: Option<i32>,
}

#[derive(Default)]
struct FailureState {
    decode_losses: VecDeque<VideoDecodeLoss>,
    fatal: Option<BackendFailure>,
}

#[derive(Default)]
pub(crate) struct FailureReporter {
    state: Mutex<FailureState>,
    video_decode_streak: AtomicUsize,
    metal_streak: AtomicUsize,
    audio_streak: AtomicUsize,
}

impl FailureReporter {
    pub(crate) fn video_decode_succeeded(&self) {
        self.video_decode_streak.store(0, Ordering::Release);
    }

    pub(crate) fn video_decode_failed(&self, status: Option<i32>) {
        {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.decode_losses.len() == DECODE_LOSS_CAPACITY {
                state.decode_losses.pop_front();
            }
            state.decode_losses.push_back(VideoDecodeLoss { status });
        }
        if increment_streak(&self.video_decode_streak) >= REPEATED_FAILURE_THRESHOLD {
            let detail = status.map_or_else(
                || "without an output pixel buffer".to_owned(),
                |status| format!("with OSStatus {status}"),
            );
            self.report_fatal(
                BackendSubsystem::VideoToolbox,
                format!(
                    "VideoToolbox failed to decode {REPEATED_FAILURE_THRESHOLD} consecutive frames; last failure was {detail}"
                ),
            );
        }
    }

    pub(crate) fn metal_succeeded(&self) {
        self.metal_streak.store(0, Ordering::Release);
    }

    pub(crate) fn metal_failed(&self, message: String) -> bool {
        if increment_streak(&self.metal_streak) < REPEATED_FAILURE_THRESHOLD {
            return false;
        }
        self.report_fatal(
            BackendSubsystem::Metal,
            format!(
                "Metal presentation failed {REPEATED_FAILURE_THRESHOLD} consecutive times: {message}"
            ),
        );
        true
    }

    pub(crate) fn audio_succeeded(&self) {
        self.audio_streak.store(0, Ordering::Release);
    }

    pub(crate) fn audio_failed(&self, message: String) -> bool {
        if increment_streak(&self.audio_streak) < REPEATED_FAILURE_THRESHOLD {
            return false;
        }
        self.report_fatal(
            BackendSubsystem::AudioWorker,
            format!(
                "the audio worker failed {REPEATED_FAILURE_THRESHOLD} consecutive times: {message}"
            ),
        );
        true
    }

    pub(crate) fn report_fatal(&self, subsystem: BackendSubsystem, message: String) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.fatal.is_none() {
            state.fatal = Some(BackendFailure { subsystem, message });
        }
    }

    pub(crate) fn pop_video_decode_loss(&self) -> Option<VideoDecodeLoss> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .decode_losses
            .pop_front()
    }

    pub(crate) fn fatal_failure(&self) -> Option<BackendFailure> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .fatal
            .clone()
    }
}

fn increment_streak(streak: &AtomicUsize) -> usize {
    streak
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |value| {
            Some(value.saturating_add(1))
        })
        .unwrap_or(usize::MAX)
        .saturating_add(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_loss_queue_keeps_only_the_bounded_tail() {
        let reporter = FailureReporter::default();
        for status in 1..=6 {
            reporter.video_decode_failed(Some(status));
        }
        let statuses: Vec<_> = std::iter::from_fn(|| reporter.pop_video_decode_loss())
            .map(|loss| loss.status.unwrap())
            .collect();
        assert_eq!(statuses, vec![3, 4, 5, 6]);
    }

    #[test]
    fn successful_work_resets_the_repeated_failure_streak() {
        let reporter = FailureReporter::default();
        reporter.video_decode_failed(Some(-1));
        reporter.video_decode_failed(Some(-1));
        reporter.video_decode_succeeded();
        reporter.video_decode_failed(Some(-1));
        reporter.video_decode_failed(Some(-1));
        assert_eq!(reporter.fatal_failure(), None);

        reporter.video_decode_failed(Some(-2));
        assert_eq!(
            reporter.fatal_failure().unwrap().subsystem,
            BackendSubsystem::VideoToolbox
        );
    }

    #[test]
    fn repeated_metal_and_audio_failures_cross_the_same_bound() {
        let metal = FailureReporter::default();
        assert!(!metal.metal_failed("drawable unavailable".to_owned()));
        assert!(!metal.metal_failed("drawable unavailable".to_owned()));
        assert!(metal.metal_failed("command buffer failed".to_owned()));
        assert_eq!(
            metal.fatal_failure().unwrap().subsystem,
            BackendSubsystem::Metal
        );

        let audio = FailureReporter::default();
        assert!(!audio.audio_failed("invalid packet".to_owned()));
        audio.audio_succeeded();
        assert!(!audio.audio_failed("invalid packet".to_owned()));
        assert!(!audio.audio_failed("invalid packet".to_owned()));
        assert!(audio.audio_failed("decoder stopped".to_owned()));
        assert_eq!(
            audio.fatal_failure().unwrap().subsystem,
            BackendSubsystem::AudioWorker
        );
    }

    #[test]
    fn only_the_first_fatal_failure_is_published() {
        let reporter = FailureReporter::default();
        reporter.report_fatal(BackendSubsystem::Metal, "device removed".to_owned());
        reporter.report_fatal(BackendSubsystem::AudioWorker, "worker stopped".to_owned());
        assert_eq!(
            reporter.fatal_failure(),
            Some(BackendFailure {
                subsystem: BackendSubsystem::Metal,
                message: "device removed".to_owned(),
            })
        );
    }
}
