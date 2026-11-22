//! Reference-picture invalidation (RFI) loss recovery, following the GFN client's
//! `NvstQosManager::updateInvalidationStatus` policy: after unrecoverable loss the
//! receiver reports the lost sender frame range with control command 0x301, withholds
//! every frame that may reference it, and resumes on the first I, intra-refresh, or
//! reference-picture-invalidation frame. An IDR request remains the bounded backstop.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use super::nvst::{EncodedVideoAccessUnit, NvstFeedbackState};

const RFI_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// `GsFrameDescriptor.frameType`, carried in byte 3 of the GameStream frame header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvstFrameType {
    P,
    I,
    Header,
    IntraRefresh,
    RefPicI,
    NonRefP,
    Unknown(u8),
}

impl NvstFrameType {
    pub fn from_wire(value: u8) -> Self {
        match value {
            1 => Self::P,
            2 => Self::I,
            3 => Self::Header,
            4 => Self::IntraRefresh,
            5 => Self::RefPicI,
            6 => Self::NonRefP,
            other => Self::Unknown(other),
        }
    }

    pub(crate) fn clears_invalidation(self) -> bool {
        matches!(self, Self::I | Self::IntraRefresh | Self::RefPicI)
    }
}

#[derive(Debug, Default)]
struct UnsentInvalidation {
    range: Option<(u64, u64)>,
    in_flight: bool,
    last_sent_at: Option<Instant>,
}

#[derive(Debug, Default)]
pub(crate) struct FrameInvalidationFeedback {
    unsent: Mutex<UnsentInvalidation>,
    sent: AtomicU64,
    dropped_frames: AtomicU64,
    recovered_i: AtomicU64,
    recovered_intra_refresh: AtomicU64,
    recovered_ref_pic_i: AtomicU64,
    idr_backstops: AtomicU64,
}

impl FrameInvalidationFeedback {
    fn unsent(&self) -> std::sync::MutexGuard<'_, UnsentInvalidation> {
        self.unsent
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// A newer range always covers the unsent one because ranges only grow by merging.
    pub(crate) fn queue(&self, range: (u64, u64)) {
        self.unsent().range = Some(range);
    }

    pub(crate) fn take(&self) -> Option<(u64, u64)> {
        let mut unsent = self.unsent();
        let range = unsent.range.take();
        unsent.in_flight = range.is_some();
        range
    }

    /// Requeues a rejected send unless recovery discarded the range while it was in flight.
    pub(crate) fn restore(&self, range: (u64, u64)) {
        let mut unsent = self.unsent();
        if std::mem::take(&mut unsent.in_flight) {
            unsent.range.get_or_insert(range);
        }
    }

    pub(crate) fn record_sent(&self, now: Instant) {
        let mut unsent = self.unsent();
        if !std::mem::take(&mut unsent.in_flight) {
            return;
        }
        unsent.last_sent_at = Some(now);
        self.sent.fetch_add(1, Ordering::Relaxed);
    }

    fn last_sent_at(&self) -> Option<Instant> {
        self.unsent().last_sent_at
    }

    fn discard(&self) {
        *self.unsent() = UnsentInvalidation::default();
    }

    pub(crate) fn stats(&self) -> String {
        let load = |counter: &AtomicU64| counter.load(Ordering::Relaxed);
        format!(
            "rfiSent={} rfiDropped={} rfiRecoveredI={} rfiRecoveredIntraRefresh={} rfiRecoveredRefPicI={} rfiIdrBackstop={}",
            load(&self.sent),
            load(&self.dropped_frames),
            load(&self.recovered_i),
            load(&self.recovered_intra_refresh),
            load(&self.recovered_ref_pic_i),
            load(&self.idr_backstops),
        )
    }
}

#[derive(Debug, Default)]
pub(crate) struct FrameInvalidation {
    last_frame: Option<u32>,
    frame_types_known: bool,
    pending: Option<(u64, u64)>,
    active_since: Option<Instant>,
    last_retry_at: Option<Instant>,
    backstop_requested: bool,
    /// The last assembled frame when a request went out ahead of a give-up.
    anticipated: Option<u32>,
}

impl FrameInvalidation {
    /// Whether a loss episode is open, i.e. frames are withheld until a recovery frame.
    pub(crate) fn active(&self) -> bool {
        self.active_since.is_some()
    }

    /// Whether recovery is reported by frame range, so a held gap cannot delay it.
    pub(crate) fn frame_types_known(&self) -> bool {
        self.frame_types_known
    }

    pub(crate) fn reset(&mut self, feedback: &NvstFeedbackState) {
        feedback.frame_invalidation.discard();
        *self = Self::default();
    }

    /// Returns whether invalidation owns recovery for this loss. Streams without a frame
    /// type keep the immediate keyframe request.
    pub(crate) fn lose(&mut self, now: Instant) -> bool {
        if !self.frame_types_known {
            return false;
        }
        self.active_since.get_or_insert(now);
        true
    }

    /// A gap has waited the usual time for its retransmission and is held a little longer.
    /// Report the frames at risk now, when the give-up would have, so the server's recovery
    /// frame is no later than without the extra wait. Nothing is withheld: if the
    /// retransmission completes the gap, its frames are delivered and the server's answer is an
    /// ordinary keyframe. If not, the give-up follows with the full range. One request per stall.
    pub(crate) fn anticipate(&mut self, revealed_by_frame: u32, feedback: &NvstFeedbackState) {
        let Some(last) = self.last_frame else {
            return;
        };
        if !self.frame_types_known || self.active_since.is_some() || self.anticipated == Some(last)
        {
            return;
        }
        self.anticipated = Some(last);
        let first = u64::from(last) + 1;
        let range = (
            first,
            first.max(u64::from(revealed_by_frame).saturating_sub(1)),
        );
        feedback.frame_invalidation.queue(range);
        opennow_streamer_protocol::log::log_async(
            "INFO",
            "nvst-rfi",
            &format!("early request frames={}..={}", range.0, range.1),
        );
    }

    pub(crate) fn poll_backstop(
        &mut self,
        now: Instant,
        cooldown: Duration,
        feedback: &NvstFeedbackState,
    ) {
        if let Some(range) = self.pending
            && self
                .last_retry_at
                .max(feedback.frame_invalidation.last_sent_at())
                .is_none_or(|last| now.saturating_duration_since(last) >= RFI_RETRY_INTERVAL)
        {
            feedback.frame_invalidation.queue(range);
            self.last_retry_at = Some(now);
        }
        if self
            .active_since
            .is_some_and(|since| now.saturating_duration_since(since) >= cooldown)
        {
            self.request_idr_backstop(feedback);
        }
    }

    /// Decides whether an assembled frame may reach the decoder and sets its contiguity.
    /// Every assembled frame counts as received, including frames withheld here.
    pub(crate) fn admit(
        &mut self,
        frame: &mut EncodedVideoAccessUnit,
        loss_before: bool,
        now: Instant,
        feedback: &NvstFeedbackState,
    ) -> bool {
        let previous = self.last_frame.replace(frame.frame_index);
        self.frame_types_known = frame.frame_type.is_some();
        let frame_gap =
            previous.is_some_and(|previous| frame.frame_index > previous.saturating_add(1));
        if frame_gap {
            self.lose(now);
        }
        let invalidating = self.active_since.is_some();
        let Some(frame_type) = frame.frame_type else {
            if invalidating {
                feedback.request_keyframe();
            }
            self.clear(feedback);
            frame.contiguous = !loss_before && !invalidating;
            return true;
        };
        if frame.keyframe || frame_type.clears_invalidation() {
            if invalidating {
                let counters = &feedback.frame_invalidation;
                let recovered = match frame_type {
                    NvstFrameType::IntraRefresh => &counters.recovered_intra_refresh,
                    NvstFrameType::RefPicI => &counters.recovered_ref_pic_i,
                    _ => &counters.recovered_i,
                };
                recovered.fetch_add(1, Ordering::Relaxed);
                opennow_streamer_protocol::log::log_async(
                    "INFO",
                    "nvst-rfi",
                    &format!(
                        "recovered frame={} type={frame_type:?} keyframe={} pending={:?}",
                        frame.frame_index, frame.keyframe, self.pending
                    ),
                );
                self.clear(feedback);
            }
            frame.contiguous = invalidating || !loss_before;
            return true;
        }
        if !invalidating {
            frame.contiguous = !loss_before;
            return true;
        }
        if loss_before || frame_gap {
            self.invalidate(previous, frame.frame_index, now, feedback);
        }
        if frame_type == NvstFrameType::Header {
            frame.contiguous = true;
            return true;
        }
        feedback
            .frame_invalidation
            .dropped_frames
            .fetch_add(1, Ordering::Relaxed);
        false
    }

    fn invalidate(
        &mut self,
        previous: Option<u32>,
        frame_index: u32,
        now: Instant,
        feedback: &NvstFeedbackState,
    ) {
        let Some(previous) = previous.filter(|previous| frame_index > previous.saturating_add(1))
        else {
            self.request_idr_backstop(feedback);
            return;
        };
        let lost = (u64::from(previous) + 1, u64::from(frame_index) - 1);
        let range = self.pending.map_or(lost, |pending| {
            (pending.0.min(lost.0), pending.1.max(lost.1))
        });
        self.pending = Some(range);
        feedback.frame_invalidation.queue(range);
        self.last_retry_at = Some(now);
    }

    fn request_idr_backstop(&mut self, feedback: &NvstFeedbackState) {
        if self.backstop_requested {
            return;
        }
        self.backstop_requested = true;
        feedback
            .frame_invalidation
            .idr_backstops
            .fetch_add(1, Ordering::Relaxed);
        feedback.request_keyframe();
    }

    fn clear(&mut self, feedback: &NvstFeedbackState) {
        feedback.frame_invalidation.discard();
        self.pending = None;
        self.active_since = None;
        self.last_retry_at = None;
        self.backstop_requested = false;
        self.anticipated = None;
    }
}
