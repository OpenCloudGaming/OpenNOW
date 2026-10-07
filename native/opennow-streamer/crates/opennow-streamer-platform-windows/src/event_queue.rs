use std::collections::VecDeque;
use std::sync::Mutex;

use crate::{BackendEvent, PushOutcome};

#[derive(Debug)]
struct Inner {
    events: VecDeque<BackendEvent>,
    dropped_decoded: u64,
    dropped_control: u64,
}

#[derive(Debug)]
pub(crate) struct EventQueue {
    capacity: usize,
    inner: Mutex<Inner>,
}

impl EventQueue {
    #[cfg(any(windows, test))]
    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            capacity,
            inner: Mutex::new(Inner {
                events: VecDeque::with_capacity(capacity),
                dropped_decoded: 0,
                dropped_control: 0,
            }),
        }
    }

    pub(crate) fn push(&self, event: BackendEvent) -> PushOutcome {
        let mut inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        let outcome = if inner.events.len() < self.capacity {
            PushOutcome::Queued
        } else if let Some(index) = inner
            .events
            .iter()
            .position(|event| matches!(event, BackendEvent::VideoFrameDecoded { .. }))
        {
            inner.events.remove(index);
            inner.dropped_decoded = inner.dropped_decoded.saturating_add(1);
            PushOutcome::DroppedOldest
        } else if matches!(event, BackendEvent::VideoFrameDecoded { .. }) {
            inner.dropped_decoded = inner.dropped_decoded.saturating_add(1);
            PushOutcome::Backpressured
        } else {
            inner.events.pop_front();
            inner.dropped_control = inner.dropped_control.saturating_add(1);
            PushOutcome::DroppedOldest
        };
        if outcome != PushOutcome::Backpressured {
            inner.events.push_back(event);
        }
        let dropped_decoded = inner.dropped_decoded;
        let dropped_control = inner.dropped_control;
        drop(inner);
        if outcome != PushOutcome::Queued {
            opennow_streamer_protocol::log::log_throttled(
                "windows-backend-event-overflow",
                "WARN",
                "decode",
                &format!(
                    "Windows backend event queue overflow: droppedDecoded={dropped_decoded} droppedControl={dropped_control} capacity={}",
                    self.capacity,
                ),
            );
        }
        outcome
    }

    pub(crate) fn try_pop(&self) -> Option<BackendEvent> {
        self.inner
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .events
            .pop_front()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Subsystem;
    use opennow_media_protocol::FrameProvenance;

    fn decoded(track_id: u32) -> BackendEvent {
        BackendEvent::VideoFrameDecoded {
            provenance: FrameProvenance {
                track_id,
                ..Default::default()
            },
        }
    }

    fn failure() -> BackendEvent {
        BackendEvent::DeviceLost {
            subsystem: Subsystem::VideoDecode,
            message: "decoder failed".to_owned(),
        }
    }

    #[test]
    fn decoded_bursts_never_evict_pending_recovery_events() {
        let events = EventQueue::new(64);
        events.push(failure());
        events.push(BackendEvent::KeyFrameRequired);
        for id in 0..1000 {
            events.push(decoded(id));
        }
        {
            let inner = events.inner.lock().unwrap();
            assert_eq!(inner.events.len(), 64);
            assert_eq!(inner.dropped_decoded, 938);
            assert_eq!(inner.dropped_control, 0);
        }
        assert_eq!(events.try_pop(), Some(failure()));
        assert_eq!(events.try_pop(), Some(BackendEvent::KeyFrameRequired));
        for id in 938..1000 {
            assert_eq!(events.try_pop(), Some(decoded(id)));
        }
        assert_eq!(events.try_pop(), None);
    }

    #[test]
    fn new_controls_evict_decoded_notifications_before_older_controls() {
        let events = EventQueue::new(3);
        events.push(failure());
        events.push(decoded(1));
        events.push(decoded(2));
        assert_eq!(
            events.push(BackendEvent::KeyFrameRequired),
            PushOutcome::DroppedOldest
        );
        assert_eq!(events.try_pop(), Some(failure()));
        assert_eq!(events.try_pop(), Some(decoded(2)));
        assert_eq!(events.try_pop(), Some(BackendEvent::KeyFrameRequired));
        assert_eq!(events.try_pop(), None);
    }

    #[test]
    fn control_only_queue_rejects_decoded_events_without_evicting_controls() {
        let events = EventQueue::new(2);
        events.push(failure());
        events.push(BackendEvent::KeyFrameRequired);
        for id in 0..1000 {
            assert_eq!(events.push(decoded(id)), PushOutcome::Backpressured);
        }
        let inner = events.inner.lock().unwrap();
        assert_eq!(inner.events.len(), 2);
        assert_eq!(inner.dropped_decoded, 1000);
        assert_eq!(inner.dropped_control, 0);
        drop(inner);
        assert_eq!(events.try_pop(), Some(failure()));
        assert_eq!(events.try_pop(), Some(BackendEvent::KeyFrameRequired));
        assert_eq!(events.try_pop(), None);
    }

    #[test]
    fn control_only_overflow_remains_bounded_and_counted() {
        let events = EventQueue::new(1);
        events.push(failure());
        assert_eq!(
            events.push(BackendEvent::KeyFrameRequired),
            PushOutcome::DroppedOldest
        );
        let inner = events.inner.lock().unwrap();
        assert_eq!(inner.events.len(), 1);
        assert_eq!(inner.dropped_decoded, 0);
        assert_eq!(inner.dropped_control, 1);
        drop(inner);
        assert_eq!(events.try_pop(), Some(BackendEvent::KeyFrameRequired));
        assert_eq!(events.try_pop(), None);
    }
}
