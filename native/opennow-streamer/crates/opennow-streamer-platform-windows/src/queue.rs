use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
#[cfg(test)]
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushOutcome {
    Queued,
    DroppedOldest,
    Paused,
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
    /// chain. Once full, every pending access unit and the incoming unit are
    /// stale; retaining any of them after dropping an older reference frame can
    /// make the hardware decoder reject the stream. The caller requests a new
    /// keyframe after receiving `DroppedOldest`.
    pub(crate) fn push_or_clear_on_overflow(&self, value: T) -> Result<PushOutcome, T> {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        if inner.closed {
            return Err(value);
        }
        if inner.values.len() == self.capacity {
            inner.values.clear();
            return Ok(PushOutcome::DroppedOldest);
        }
        inner.values.push_back(value);
        self.ready.notify_one();
        Ok(PushOutcome::Queued)
    }

    pub(crate) fn try_pop(&self) -> Option<T> {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values
            .pop_front()
    }

    #[cfg(test)]
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

    pub(crate) fn clear(&self) {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values
            .clear();
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
        assert_eq!(queue.push_or_clear_on_overflow(1), Ok(PushOutcome::Queued));
        assert_eq!(queue.push_or_clear_on_overflow(2), Ok(PushOutcome::Queued));
        assert_eq!(
            queue.push_or_clear_on_overflow(3),
            Ok(PushOutcome::DroppedOldest)
        );
        assert_eq!(queue.try_pop(), None);
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
}
