use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueuePush {
    Added,
    DroppedOldest,
    Full,
    Closed,
}

#[derive(Debug, PartialEq, Eq)]
pub enum QueuePop<T> {
    Item(T),
    TimedOut,
    Closed,
}

#[derive(Debug)]
struct State<T> {
    items: VecDeque<T>,
    closed: bool,
}

#[derive(Debug)]
pub struct BoundedQueue<T> {
    capacity: usize,
    state: Mutex<State<T>>,
    ready: Condvar,
}

impl<T> BoundedQueue<T> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "queue capacity must be non-zero");
        Self {
            capacity,
            state: Mutex::new(State {
                items: VecDeque::with_capacity(capacity),
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }

    pub fn push_latest(&self, item: T) -> QueuePush {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.closed {
            return QueuePush::Closed;
        }
        let outcome = if state.items.len() == self.capacity {
            state.items.pop_front();
            QueuePush::DroppedOldest
        } else {
            QueuePush::Added
        };
        state.items.push_back(item);
        self.ready.notify_one();
        outcome
    }

    pub fn push(&self, item: T) -> QueuePush {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.closed {
            return QueuePush::Closed;
        }
        if state.items.len() == self.capacity {
            return QueuePush::Full;
        }
        state.items.push_back(item);
        self.ready.notify_one();
        QueuePush::Added
    }

    pub fn replace(&self, item: T) -> QueuePush {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.closed {
            return QueuePush::Closed;
        }
        let outcome = if state.items.is_empty() {
            QueuePush::Added
        } else {
            QueuePush::DroppedOldest
        };
        state.items.clear();
        state.items.push_back(item);
        self.ready.notify_one();
        outcome
    }

    pub fn replace_where(&self, item: T, mut should_remove: impl FnMut(&T) -> bool) -> QueuePush {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.closed {
            return QueuePush::Closed;
        }
        let original_len = state.items.len();
        state.items.retain(|queued| !should_remove(queued));
        if state.items.len() == self.capacity {
            return QueuePush::Full;
        }
        let outcome = if state.items.len() == original_len {
            QueuePush::Added
        } else {
            QueuePush::DroppedOldest
        };
        state.items.push_back(item);
        self.ready.notify_one();
        outcome
    }

    pub fn try_pop(&self) -> Option<T> {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .items
            .pop_front()
    }

    pub fn wait_pop(&self, timeout: Duration) -> QueuePop<T> {
        let start = Instant::now();
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        loop {
            if let Some(item) = state.items.pop_front() {
                return QueuePop::Item(item);
            }
            if state.closed {
                return QueuePop::Closed;
            }
            let Some(remaining) = timeout.checked_sub(start.elapsed()) else {
                return QueuePop::TimedOut;
            };
            let (next, result) = self
                .ready
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poison| poison.into_inner());
            state = next;
            if result.timed_out() {
                return state
                    .items
                    .pop_front()
                    .map_or(QueuePop::TimedOut, QueuePop::Item);
            }
        }
    }

    pub fn pop_timeout(&self, timeout: Duration) -> Option<T> {
        match self.wait_pop(timeout) {
            QueuePop::Item(item) => Some(item),
            QueuePop::TimedOut | QueuePop::Closed => None,
        }
    }

    pub fn clear(&self) {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .items
            .clear();
    }

    pub fn is_closed(&self) -> bool {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .closed
    }

    pub fn close(&self) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        state.closed = true;
        state.items.clear();
        self.ready.notify_all();
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .items
            .len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_newest_items_at_capacity() {
        let queue = BoundedQueue::new(2);
        assert_eq!(queue.push_latest(1), QueuePush::Added);
        assert_eq!(queue.push_latest(2), QueuePush::Added);
        assert_eq!(queue.push_latest(3), QueuePush::DroppedOldest);
        assert_eq!(queue.len(), 2);
        assert_eq!(queue.try_pop(), Some(2));
        assert_eq!(queue.try_pop(), Some(3));
    }

    #[test]
    fn close_wakes_waiters_and_rejects_pushes() {
        let queue = BoundedQueue::new(1);
        queue.close();
        assert_eq!(queue.wait_pop(Duration::from_secs(1)), QueuePop::Closed);
        assert_eq!(queue.push_latest(1), QueuePush::Closed);
        assert_eq!(queue.push(1), QueuePush::Closed);
    }

    #[test]
    fn timeout_does_not_close_an_idle_queue() {
        let queue = BoundedQueue::new(1);
        assert_eq!(queue.wait_pop(Duration::from_millis(1)), QueuePop::TimedOut);
        assert_eq!(queue.push_latest(7), QueuePush::Added);
        assert_eq!(queue.wait_pop(Duration::from_secs(1)), QueuePop::Item(7));
    }

    #[test]
    fn non_evicting_push_and_atomic_replace_preserve_control() {
        let queue = BoundedQueue::new(1);
        assert_eq!(queue.push(1), QueuePush::Added);
        assert_eq!(queue.push(2), QueuePush::Full);
        assert_eq!(queue.replace(3), QueuePush::DroppedOldest);
        assert_eq!(queue.try_pop(), Some(3));
    }

    #[test]
    fn conditional_replace_never_removes_control() {
        let queue = BoundedQueue::new(2);
        assert_eq!(queue.push((false, 1)), QueuePush::Added);
        assert_eq!(queue.push((true, 2)), QueuePush::Added);
        assert_eq!(
            queue.replace_where((true, 3), |(media, _)| *media),
            QueuePush::DroppedOldest
        );
        assert_eq!(queue.try_pop(), Some((false, 1)));
        assert_eq!(queue.try_pop(), Some((true, 3)));
    }
}
