use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PushResult {
    Queued,
    DroppedOldest,
    Closed,
}

#[derive(Debug)]
struct QueueState<T> {
    values: VecDeque<T>,
    closed: bool,
}

#[derive(Debug)]
pub(crate) struct BoundedQueue<T> {
    capacity: usize,
    state: Mutex<QueueState<T>>,
    ready: Condvar,
}

impl<T> BoundedQueue<T> {
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "bounded queue capacity must be non-zero");
        Self {
            capacity,
            state: Mutex::new(QueueState {
                values: VecDeque::with_capacity(capacity),
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }

    pub(crate) fn push(&self, value: T) -> PushResult {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.closed {
            return PushResult::Closed;
        }
        let result = if state.values.len() == self.capacity {
            state.values.pop_front();
            PushResult::DroppedOldest
        } else {
            PushResult::Queued
        };
        state.values.push_back(value);
        self.ready.notify_one();
        result
    }

    pub(crate) fn pop(&self) -> Option<T> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            if let Some(value) = state.values.pop_front() {
                return Some(value);
            }
            if state.closed {
                return None;
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(|error| error.into_inner());
        }
    }

    pub(crate) fn clear(&self) {
        self.state
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .values
            .clear();
    }

    pub(crate) fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        state.closed = true;
        state.values.clear();
        self.ready.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_the_oldest_value_at_capacity() {
        let queue = BoundedQueue::new(2);
        assert_eq!(queue.push(1), PushResult::Queued);
        assert_eq!(queue.push(2), PushResult::Queued);
        assert_eq!(queue.push(3), PushResult::DroppedOldest);
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.pop(), Some(3));
    }

    #[test]
    fn close_wakes_and_rejects_producers() {
        let queue = BoundedQueue::new(1);
        queue.push(1);
        queue.close();
        assert_eq!(queue.pop(), None);
        assert_eq!(queue.push(2), PushResult::Closed);
    }
}
