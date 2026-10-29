use std::collections::VecDeque;
use std::time::{Duration, Instant};

use crate::nvst::NvstUdpReceiverError;

pub(crate) const MICROPHONE_QUEUE_CAPACITY: usize = 5;
const MAX_OPUS_BYTES: usize = 1275;
const MAX_FRAME_AGE: Duration = Duration::from_millis(100);

pub(crate) struct MicrophoneFrame {
    pub payload: Vec<u8>,
    pub timestamp: u32,
    pub queued_at: Instant,
}

pub(crate) struct MicrophoneQueue {
    available: bool,
    enabled: bool,
    closed: bool,
    pub generation: u64,
    frames: VecDeque<MicrophoneFrame>,
    dropped: u64,
}

impl MicrophoneQueue {
    pub fn new(available: bool) -> Self {
        Self {
            available,
            enabled: false,
            closed: false,
            generation: 0,
            frames: VecDeque::with_capacity(MICROPHONE_QUEUE_CAPACITY),
            dropped: 0,
        }
    }

    fn clear(&mut self) {
        self.dropped += self.frames.len() as u64;
        self.frames.clear();
    }

    pub fn set_enabled(&mut self, enabled: bool) -> Result<(), NvstUdpReceiverError> {
        self.clear();
        self.generation = self.generation.wrapping_add(1);
        if self.closed {
            return Err(NvstUdpReceiverError::Closed);
        }
        if enabled && !self.available {
            return Err(NvstUdpReceiverError::MicrophoneUnavailable);
        }
        self.enabled = enabled;
        Ok(())
    }

    pub fn push(
        &mut self,
        payload: Vec<u8>,
        timestamp: u32,
        now: Instant,
    ) -> Result<(), NvstUdpReceiverError> {
        if self.closed {
            return Err(NvstUdpReceiverError::Closed);
        }
        if !self.available {
            return Err(NvstUdpReceiverError::MicrophoneUnavailable);
        }
        if !self.enabled {
            return Err(NvstUdpReceiverError::MicrophoneDisabled);
        }
        if payload.is_empty() || payload.len() > MAX_OPUS_BYTES {
            return Err(NvstUdpReceiverError::InvalidMicrophoneFrame);
        }
        if self.frames.len() == MICROPHONE_QUEUE_CAPACITY {
            self.frames.pop_front();
            self.dropped += 1;
        }
        self.frames.push_back(MicrophoneFrame {
            payload,
            timestamp,
            queued_at: now,
        });
        Ok(())
    }

    pub fn pop(&mut self, now: Instant) -> Option<MicrophoneFrame> {
        while let Some(frame) = self.frames.pop_front() {
            if now.saturating_duration_since(frame.queued_at) < MAX_FRAME_AGE {
                return Some(frame);
            }
            self.dropped += 1;
        }
        None
    }

    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    pub fn close(&mut self) {
        self.clear();
        self.enabled = false;
        self.closed = true;
        self.generation = self.generation.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn microphone_queue_bounds_payload_and_drops_oldest() {
        let mut queue = MicrophoneQueue::new(true);
        queue.set_enabled(true).unwrap();
        let now = Instant::now();
        assert!(queue.push(vec![], 0, now).is_err());
        assert!(queue.push(vec![0; MAX_OPUS_BYTES + 1], 0, now).is_err());
        for timestamp in 0..8 {
            queue.push(vec![1], timestamp, now).unwrap();
        }
        assert_eq!(queue.frames.len(), MICROPHONE_QUEUE_CAPACITY);
        assert_eq!(queue.dropped(), 3);
        assert_eq!(queue.pop(now).unwrap().timestamp, 3);
        assert!(queue.pop(now + MAX_FRAME_AGE).is_none());
        assert_eq!(queue.dropped(), 7);
    }

    #[test]
    fn microphone_queue_flushes_every_explicit_toggle_and_shutdown() {
        let mut queue = MicrophoneQueue::new(true);
        let now = Instant::now();
        assert!(matches!(
            queue.push(vec![1], 0, now),
            Err(NvstUdpReceiverError::MicrophoneDisabled)
        ));
        queue.set_enabled(true).unwrap();
        queue.push(vec![1], u32::MAX, now).unwrap();
        queue.set_enabled(true).unwrap();
        assert!(queue.pop(now).is_none());
        queue.push(vec![1], 0, now).unwrap();
        queue.set_enabled(false).unwrap();
        queue.set_enabled(true).unwrap();
        assert!(queue.pop(now).is_none());
        queue.close();
        assert!(matches!(
            queue.set_enabled(true),
            Err(NvstUdpReceiverError::Closed)
        ));
        assert!(matches!(
            queue.push(vec![1], 0, now),
            Err(NvstUdpReceiverError::Closed)
        ));
    }

    #[test]
    fn microphone_frames_expire_at_one_hundred_milliseconds() {
        let mut queue = MicrophoneQueue::new(true);
        queue.set_enabled(true).unwrap();
        let now = Instant::now();
        queue.push(vec![1], 0, now).unwrap();
        assert!(queue.pop(now + Duration::from_millis(99)).is_some());
        queue.push(vec![1], 960, now).unwrap();
        assert!(queue.pop(now + Duration::from_millis(100)).is_none());
        assert_eq!(queue.dropped(), 1);
    }

    #[test]
    fn unsupported_microphone_remains_disabled() {
        let mut queue = MicrophoneQueue::new(false);
        assert!(matches!(
            queue.set_enabled(true),
            Err(NvstUdpReceiverError::MicrophoneUnavailable)
        ));
        queue.set_enabled(false).unwrap();
        assert!(matches!(
            queue.push(vec![1], 0, Instant::now()),
            Err(NvstUdpReceiverError::MicrophoneUnavailable)
        ));
    }
}
