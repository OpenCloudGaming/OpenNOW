use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

pub(crate) enum PushResult<T> {
    Pushed,
    Replaced(T),
    Closed(T),
}

pub(crate) enum TryPopResult<T> {
    Value(T),
    Empty,
    Closed,
}

pub(crate) struct BoundedQueue<T> {
    capacity: usize,
    state: Mutex<QueueState<T>>,
    ready: Condvar,
}

struct QueueState<T> {
    values: VecDeque<T>,
    closed: bool,
}

impl<T> BoundedQueue<T> {
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            capacity,
            state: Mutex::new(QueueState {
                values: VecDeque::with_capacity(capacity),
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }

    pub(crate) fn push_drop_oldest(&self, value: T) -> PushResult<T> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.closed {
            return PushResult::Closed(value);
        }
        let replaced = if state.values.len() == self.capacity {
            state.values.pop_front()
        } else {
            None
        };
        state.values.push_back(value);
        self.ready.notify_one();
        replaced.map_or(PushResult::Pushed, PushResult::Replaced)
    }

    pub(crate) fn pop_wait(&self) -> Option<T> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    pub(crate) fn pop_now(&self) -> TryPopResult<T> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(value) = state.values.pop_front() {
            TryPopResult::Value(value)
        } else if state.closed {
            TryPopResult::Closed
        } else {
            TryPopResult::Empty
        }
    }

    pub(crate) fn len(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values
            .len()
    }

    pub(crate) fn close(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.closed = true;
        self.ready.notify_all();
    }

    pub(crate) fn close_and_discard(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.closed = true;
        state.values.clear();
        self.ready.notify_all();
    }

    pub(crate) fn clear(&self) -> usize {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let discarded = state.values.len();
        state.values.clear();
        discarded
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn replaces_oldest_at_capacity() {
        let queue = BoundedQueue::new(2);
        assert!(matches!(queue.push_drop_oldest(1), PushResult::Pushed));
        assert!(matches!(queue.push_drop_oldest(2), PushResult::Pushed));
        assert!(matches!(queue.push_drop_oldest(3), PushResult::Replaced(1)));
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.pop_wait(), Some(2));
        assert_eq!(queue.pop_wait(), Some(3));
    }

    #[test]
    fn close_wakes_waiter_and_rejects_pushes() {
        let queue = Arc::new(BoundedQueue::<u8>::new(1));
        let waiter = {
            let queue = Arc::clone(&queue);
            thread::spawn(move || queue.pop_wait())
        };
        queue.close();
        assert_eq!(waiter.join().unwrap(), None);
        assert!(matches!(queue.push_drop_oldest(7), PushResult::Closed(7)));
    }

    #[test]
    fn discard_close_drops_queued_work() {
        let queue = BoundedQueue::new(2);
        assert!(matches!(queue.push_drop_oldest(1), PushResult::Pushed));
        queue.close_and_discard();
        assert_eq!(queue.pop_wait(), None);
    }

    #[test]
    fn clear_keeps_queue_open() {
        let queue = BoundedQueue::new(2);
        assert!(matches!(queue.push_drop_oldest(1), PushResult::Pushed));
        assert_eq!(queue.clear(), 1);
        assert!(matches!(queue.push_drop_oldest(2), PushResult::Pushed));
        assert_eq!(queue.pop_wait(), Some(2));
    }
}
