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
        fn is_keyframe(value: &i32) -> bool {
            *value < 0
        }
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
