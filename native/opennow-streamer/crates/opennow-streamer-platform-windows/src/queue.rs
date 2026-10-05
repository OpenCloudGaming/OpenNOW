use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Queued,
    DroppedOldest,
    /// The incoming delta was dropped and the queue was left unchanged because
    /// it already holds a keyframe. Clearing that keyframe to admit one more
    /// P-frame makes the decoder wait forever for a reference it just lost.
    Backpressured,
    Paused,
}

/// How one compressed access unit is admitted into a bounded decode queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressedAdmit {
    Append,
    ReplaceWithKeyframe { dropped: usize },
    PreserveQueuedKeyframe,
    DiscardChain { dropped: usize },
}

/// A full queue that already holds a keyframe keeps that keyframe. An incoming
/// keyframe still replaces the stale chain. A full queue with no keyframe is
/// discarded, because every remaining delta depends on a frame that will not
/// be decoded.
pub fn admit_compressed_frame(
    len: usize,
    capacity: usize,
    incoming_is_keyframe: bool,
    queue_has_keyframe: bool,
) -> CompressedAdmit {
    if len < capacity {
        return CompressedAdmit::Append;
    }
    if incoming_is_keyframe {
        return CompressedAdmit::ReplaceWithKeyframe { dropped: len };
    }
    if queue_has_keyframe {
        return CompressedAdmit::PreserveQueuedKeyframe;
    }
    CompressedAdmit::DiscardChain {
        dropped: len.saturating_add(1),
    }
}

/// Reference state of a compressed queue after it dropped a delta. Every later
/// delta depends on the dropped frame, so none may be queued until a keyframe
/// arrives.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceGap {
    #[default]
    Intact,
    /// The queue still holds the keyframe in front of the gap. A replacement
    /// requested now arrives before the decoder dequeues that keyframe and
    /// evicts it, so slow decoders would request one keyframe per arrival.
    BehindQueuedKeyframe,
    KeyframeRequested,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GapAdmit {
    Admit,
    Discard,
    DiscardAndRequestKeyframe,
}

impl ReferenceGap {
    pub fn admit(
        &mut self,
        incoming_is_keyframe: bool,
        queue_has_keyframe: impl FnOnce() -> bool,
    ) -> GapAdmit {
        if incoming_is_keyframe {
            *self = Self::Intact;
            return GapAdmit::Admit;
        }
        match self {
            Self::Intact => GapAdmit::Admit,
            Self::BehindQueuedKeyframe if queue_has_keyframe() => GapAdmit::Discard,
            Self::BehindQueuedKeyframe => {
                *self = Self::KeyframeRequested;
                GapAdmit::DiscardAndRequestKeyframe
            }
            Self::KeyframeRequested => GapAdmit::Discard,
        }
    }

    pub fn record(&mut self, admitted: CompressedAdmit) {
        match admitted {
            CompressedAdmit::PreserveQueuedKeyframe => *self = Self::BehindQueuedKeyframe,
            CompressedAdmit::DiscardChain { .. } => *self = Self::KeyframeRequested,
            CompressedAdmit::Append | CompressedAdmit::ReplaceWithKeyframe { .. } => {}
        }
    }
}

#[derive(Debug)]
struct Inner<T> {
    values: VecDeque<T>,
    closed: bool,
}

#[derive(Debug)]
pub(crate) struct BoundedQueue<T> {
    capacity: usize,
    inner: Mutex<Inner<T>>,
    ready: Condvar,
}

#[cfg_attr(not(windows), allow(dead_code))]
impl<T> BoundedQueue<T> {
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "queue capacity must be non-zero");
        Self {
            capacity,
            inner: Mutex::new(Inner {
                values: VecDeque::with_capacity(capacity),
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }

    pub(crate) fn push(&self, value: T) -> Result<PushOutcome, T> {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if inner.closed {
            return Err(value);
        }
        let outcome = if inner.values.len() == self.capacity {
            inner.values.pop_front();
            PushOutcome::DroppedOldest
        } else {
            PushOutcome::Queued
        };
        inner.values.push_back(value);
        self.ready.notify_one();
        Ok(outcome)
    }

    /// Queues compressed inter-frame video without creating a broken reference
    /// chain. A full queue with no keyframe is discarded: retaining a delta
    /// after dropping its reference makes the decoder reject the stream. An
    /// incoming keyframe replaces that stale chain. A full queue that already
    /// holds a keyframe stays intact (`Backpressured`); wiping it to admit one
    /// more delta drops the only frame the decoder can restart from.
    pub(crate) fn push_or_clear_on_overflow(
        &self,
        value: T,
        incoming_is_keyframe: bool,
        queued_is_keyframe: impl Fn(&T) -> bool,
    ) -> Result<PushOutcome, T> {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if inner.closed {
            return Err(value);
        }
        let queue_has_keyframe = inner.values.iter().any(queued_is_keyframe);
        match admit_compressed_frame(
            inner.values.len(),
            self.capacity,
            incoming_is_keyframe,
            queue_has_keyframe,
        ) {
            CompressedAdmit::Append => {
                inner.values.push_back(value);
                self.ready.notify_one();
                Ok(PushOutcome::Queued)
            }
            CompressedAdmit::ReplaceWithKeyframe { .. } => {
                inner.values.clear();
                inner.values.push_back(value);
                self.ready.notify_one();
                Ok(PushOutcome::DroppedOldest)
            }
            CompressedAdmit::PreserveQueuedKeyframe => Ok(PushOutcome::Backpressured),
            CompressedAdmit::DiscardChain { .. } => {
                inner.values.clear();
                Ok(PushOutcome::DroppedOldest)
            }
        }
    }

    pub(crate) fn try_pop(&self) -> Option<T> {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values
            .pop_front()
    }

    pub(crate) fn pop_timeout(&self, timeout: Duration) -> Option<T> {
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let mut inner = self
            .ready
            .wait_timeout_while(inner, timeout, |inner| {
                inner.values.is_empty() && !inner.closed
            })
            .unwrap_or_else(|error| error.into_inner())
            .0;
        inner.values.pop_front()
    }

    /// Waits for actionable input, shutdown, or the next decoder-output poll.
    /// Queued input cannot wake a decoder that has no input credits. In that
    /// case even producer notifications must leave it asleep until the bounded
    /// poll deadline (or shutdown), instead of spinning on a nonempty queue.
    pub(crate) fn wait_for_decoder(&self, timeout: Duration, accepts_input: bool) -> bool {
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let inner = self
            .ready
            .wait_timeout_while(inner, timeout, |inner| {
                (!accepts_input || inner.values.is_empty()) && !inner.closed
            })
            .unwrap_or_else(|error| error.into_inner())
            .0;
        !inner.values.is_empty()
    }

    pub(crate) fn clear(&self) {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values
            .clear();
    }

    pub(crate) fn any(&self, predicate: impl Fn(&T) -> bool) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values
            .iter()
            .any(predicate)
    }

    pub(crate) fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values
            .len()
    }

    pub(crate) fn close(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        inner.closed = true;
        inner.values.clear();
        self.ready.notify_all();
    }

    #[cfg(test)]
    pub(crate) fn is_closed(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_oldest_value_when_full() {
        let queue = BoundedQueue::new(2);
        assert_eq!(queue.push(1), Ok(PushOutcome::Queued));
        assert_eq!(queue.push(2), Ok(PushOutcome::Queued));
        assert_eq!(queue.push(3), Ok(PushOutcome::DroppedOldest));
        assert_eq!(queue.try_pop(), Some(2));
        assert_eq!(queue.try_pop(), Some(3));
    }

    #[test]
    fn video_overflow_discards_the_pending_reference_chain() {
        let queue = BoundedQueue::new(2);
        assert_eq!(
            queue.push_or_clear_on_overflow(1, false, |_| false),
            Ok(PushOutcome::Queued)
        );
        assert_eq!(
            queue.push_or_clear_on_overflow(2, false, |_| false),
            Ok(PushOutcome::Queued)
        );
        assert_eq!(
            queue.push_or_clear_on_overflow(3, false, |_| false),
            Ok(PushOutcome::DroppedOldest)
        );
        assert_eq!(queue.try_pop(), None);
    }

    #[test]
    fn video_overflow_retains_an_incoming_recovery_keyframe() {
        let queue = BoundedQueue::new(2);
        assert_eq!(
            queue.push_or_clear_on_overflow(1, false, |_| false),
            Ok(PushOutcome::Queued)
        );
        assert_eq!(
            queue.push_or_clear_on_overflow(2, false, |_| false),
            Ok(PushOutcome::Queued)
        );
        assert_eq!(
            queue.push_or_clear_on_overflow(3, true, |_| false),
            Ok(PushOutcome::DroppedOldest)
        );
        assert_eq!(queue.try_pop(), Some(3));
        assert_eq!(queue.try_pop(), None);
    }

    #[test]
    fn video_overflow_keeps_a_queued_keyframe_when_a_delta_does_not_fit() {
        let queue = BoundedQueue::new(2);
        assert_eq!(
            queue.push_or_clear_on_overflow(-1, true, is_keyframe),
            Ok(PushOutcome::Queued)
        );
        assert_eq!(
            queue.push_or_clear_on_overflow(2, false, is_keyframe),
            Ok(PushOutcome::Queued)
        );
        assert_eq!(
            queue.push_or_clear_on_overflow(3, false, is_keyframe),
            Ok(PushOutcome::Backpressured)
        );
        assert_eq!(queue.try_pop(), Some(-1));
        assert_eq!(queue.try_pop(), Some(2));
        assert_eq!(queue.try_pop(), None);
    }

    #[test]
    fn admit_compressed_frame_preserves_a_queued_keyframe() {
        assert_eq!(
            admit_compressed_frame(7, 7, false, true),
            CompressedAdmit::PreserveQueuedKeyframe
        );
        assert_eq!(
            admit_compressed_frame(7, 7, true, true),
            CompressedAdmit::ReplaceWithKeyframe { dropped: 7 }
        );
        assert_eq!(
            admit_compressed_frame(7, 7, false, false),
            CompressedAdmit::DiscardChain { dropped: 8 }
        );
        assert_eq!(
            admit_compressed_frame(3, 7, false, false),
            CompressedAdmit::Append
        );
    }

    fn is_keyframe(value: &i32) -> bool {
        *value < 0
    }

    fn submit(queue: &BoundedQueue<i32>, gap: &mut ReferenceGap, value: i32) -> GapAdmit {
        let keyframe = is_keyframe(&value);
        let admitted = gap.admit(keyframe, || queue.any(is_keyframe));
        if admitted == GapAdmit::Admit {
            match queue.push_or_clear_on_overflow(value, keyframe, is_keyframe) {
                Ok(PushOutcome::Backpressured) => {
                    gap.record(CompressedAdmit::PreserveQueuedKeyframe)
                }
                Ok(PushOutcome::DroppedOldest) if !keyframe => {
                    gap.record(CompressedAdmit::DiscardChain { dropped: 0 })
                }
                _ => {}
            }
        }
        admitted
    }

    #[test]
    fn queue_reports_whether_any_value_matches() {
        let queue = BoundedQueue::new(3);
        queue.push(1).unwrap();
        assert!(!queue.any(is_keyframe));
        queue.push(-2).unwrap();
        assert!(queue.any(is_keyframe));
        assert_eq!(queue.try_pop(), Some(1));
        assert_eq!(queue.try_pop(), Some(-2));
        assert!(!queue.any(is_keyframe));
    }

    #[test]
    fn stalled_decoder_does_not_request_a_keyframe_per_arrival() {
        let queue = BoundedQueue::new(3);
        let mut gap = ReferenceGap::default();
        for value in [-1, 2, 3] {
            assert_eq!(submit(&queue, &mut gap, value), GapAdmit::Admit);
        }
        assert_eq!(submit(&queue, &mut gap, 4), GapAdmit::Admit);
        assert_eq!(gap, ReferenceGap::BehindQueuedKeyframe);
        for value in 5..300 {
            assert_eq!(submit(&queue, &mut gap, value), GapAdmit::Discard);
        }
        assert_eq!(queue.try_pop(), Some(-1));
        assert_eq!(
            submit(&queue, &mut gap, 300),
            GapAdmit::DiscardAndRequestKeyframe
        );
        assert_eq!(submit(&queue, &mut gap, 301), GapAdmit::Discard);
        assert_eq!(queue.try_pop(), Some(2));
        assert_eq!(queue.try_pop(), Some(3));
        assert_eq!(queue.try_pop(), None);
        assert_eq!(submit(&queue, &mut gap, 302), GapAdmit::Discard);
        assert_eq!(submit(&queue, &mut gap, -303), GapAdmit::Admit);
        assert_eq!(gap, ReferenceGap::Intact);
        assert_eq!(submit(&queue, &mut gap, 304), GapAdmit::Admit);
        assert_eq!(queue.try_pop(), Some(-303));
        assert_eq!(queue.try_pop(), Some(304));
    }

    #[test]
    fn keyframe_arriving_behind_a_queued_keyframe_closes_the_gap() {
        let queue = BoundedQueue::new(2);
        let mut gap = ReferenceGap::default();
        submit(&queue, &mut gap, -1);
        submit(&queue, &mut gap, 2);
        submit(&queue, &mut gap, 3);
        assert_eq!(gap, ReferenceGap::BehindQueuedKeyframe);
        assert_eq!(submit(&queue, &mut gap, -4), GapAdmit::Admit);
        assert_eq!(gap, ReferenceGap::Intact);
        assert_eq!(queue.try_pop(), Some(-4));
        assert_eq!(queue.try_pop(), None);
    }

    #[test]
    fn discarded_chain_drops_deltas_until_a_keyframe_without_new_requests() {
        let queue = BoundedQueue::new(2);
        let mut gap = ReferenceGap::default();
        submit(&queue, &mut gap, 1);
        submit(&queue, &mut gap, 2);
        assert_eq!(submit(&queue, &mut gap, 3), GapAdmit::Admit);
        assert_eq!(gap, ReferenceGap::KeyframeRequested);
        assert_eq!(queue.try_pop(), None);
        assert_eq!(submit(&queue, &mut gap, 4), GapAdmit::Discard);
        assert_eq!(queue.try_pop(), None);
        assert_eq!(submit(&queue, &mut gap, -5), GapAdmit::Admit);
        assert_eq!(queue.try_pop(), Some(-5));
    }

    #[test]
    fn close_discards_values_and_rejects_writes() {
        let queue = BoundedQueue::new(2);
        queue.push(1).unwrap();
        queue.close();
        assert!(queue.is_closed());
        assert_eq!(queue.try_pop(), None);
        assert_eq!(queue.push(2), Err(2));
    }

    #[test]
    fn timeout_returns_without_a_value() {
        let queue = BoundedQueue::<u8>::new(1);
        assert_eq!(queue.pop_timeout(Duration::from_millis(1)), None);
    }

    #[test]
    fn decoder_wait_does_not_consume_actionable_input() {
        let queue = BoundedQueue::new(1);
        queue.push(7).unwrap();
        assert!(queue.wait_for_decoder(Duration::from_secs(1), true));
        assert_eq!(queue.try_pop(), Some(7));
    }

    #[test]
    fn decoder_backpressure_waits_even_with_queued_input() {
        let queue = BoundedQueue::new(1);
        queue.push(7).unwrap();
        let started = std::time::Instant::now();
        let interval = Duration::from_millis(20);
        assert!(queue.wait_for_decoder(interval, false));
        assert!(started.elapsed() >= interval);
        assert_eq!(queue.try_pop(), Some(7));
    }

    #[test]
    fn closing_queue_interrupts_decoder_backpressure_wait() {
        let queue = std::sync::Arc::new(BoundedQueue::new(1));
        queue.push(7).unwrap();
        let worker_queue = std::sync::Arc::clone(&queue);
        let (sent, received) = std::sync::mpsc::channel();
        let worker = std::thread::spawn(move || {
            worker_queue.wait_for_decoder(Duration::from_secs(30), false);
            sent.send(()).unwrap();
        });
        queue.close();
        received.recv_timeout(Duration::from_secs(2)).unwrap();
        worker.join().unwrap();
    }
}
