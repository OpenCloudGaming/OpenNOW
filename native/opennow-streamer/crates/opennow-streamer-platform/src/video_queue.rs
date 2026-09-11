//! Compressed video cannot use an audio-style drop-oldest queue: every retained
//! delta may depend on the removed frame. Admission and loss epochs share one lock.
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

use crate::media::EncodedFrame;

pub(crate) struct VideoPacket {
    pub frame: EncodedFrame,
    #[cfg(any(windows, test))]
    pub generation: u64,
    pub reset_decoder: bool,
}

pub(crate) struct VideoPush {
    pub dropped: usize,
    pub request_keyframe: bool,
}

struct State {
    frames: VecDeque<VideoPacket>,
    generation: u64,
    waiting_for_keyframe: bool,
    request_pending: bool,
    closed: bool,
}

pub(crate) struct VideoQueue {
    capacity: usize,
    state: Mutex<State>,
    ready: Condvar,
}

impl VideoQueue {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            capacity,
            state: Mutex::new(State {
                frames: VecDeque::with_capacity(capacity),
                generation: 1,
                waiting_for_keyframe: true,
                request_pending: false,
                closed: false,
            }),
            ready: Condvar::new(),
        }
    }

    fn invalidate_locked(state: &mut State) -> usize {
        let dropped = state.frames.len();
        state.frames.clear();
        state.generation = state.generation.wrapping_add(1);
        state.waiting_for_keyframe = true;
        state.request_pending = false;
        dropped
    }

    pub fn push(&self, frame: EncodedFrame) -> Result<VideoPush, ()> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.closed {
            return Err(());
        }
        let mut dropped = 0;
        if !state.waiting_for_keyframe && (!frame.contiguous || state.frames.len() == self.capacity)
        {
            dropped += Self::invalidate_locked(&mut state);
        }
        if state.waiting_for_keyframe && !frame.keyframe {
            let request_keyframe = !state.request_pending;
            state.request_pending = true;
            return Ok(VideoPush {
                dropped: dropped + 1,
                request_keyframe,
            });
        }
        let reset_decoder = state.waiting_for_keyframe;
        state.waiting_for_keyframe = false;
        state.request_pending = false;
        #[cfg(any(windows, test))]
        let generation = state.generation;
        state.frames.push_back(VideoPacket {
            frame,
            #[cfg(any(windows, test))]
            generation,
            reset_decoder,
        });
        self.ready.notify_one();
        Ok(VideoPush {
            dropped,
            request_keyframe: false,
        })
    }

    pub fn pop_packet(&self) -> Option<VideoPacket> {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        loop {
            if let Some(frame) = state.frames.pop_front() {
                return Some(frame);
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

    // Compatibility for other platform consumers: local queue loss is a decode
    // discontinuity too. Sender IDs/timestamps and recording input stay untouched.
    pub fn pop(&self) -> Option<EncodedFrame> {
        self.pop_packet().map(|packet| {
            let mut frame = packet.frame;
            frame.contiguous &= !packet.reset_decoder;
            frame
        })
    }

    /// The closure may only enqueue compressed data, never run codec/GPU work.
    /// A loss cannot interleave between epoch validation and downstream admission.
    #[cfg(any(windows, test))]
    pub fn submit_if_current<T>(&self, generation: u64, enqueue: impl FnOnce() -> T) -> Option<T> {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        if state.closed || state.generation != generation {
            return None;
        }
        Some(enqueue())
    }

    pub fn clear(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        Self::invalidate_locked(&mut state);
    }

    pub fn close(&self) {
        let mut state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        Self::invalidate_locked(&mut state);
        state.closed = true;
        self.ready.notify_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::media::MediaCodec;
    use std::sync::Arc;

    fn frame(id: u32, keyframe: bool) -> EncodedFrame {
        EncodedFrame {
            mid: "video".into(),
            codec: MediaCodec::H264,
            data: Arc::from([1]),
            frame_index: Some(id),
            timestamp: u64::from(id) * 750,
            clock_rate_hz: 90_000,
            keyframe,
            contiguous: true,
        }
    }

    #[test]
    fn old_keyframe_cannot_reopen_a_newer_missing_reference_chain() {
        let queue = VideoQueue::new(2);
        queue.push(frame(1, true)).unwrap();
        let old_keyframe = queue.pop_packet().unwrap();
        queue.push(frame(2, false)).unwrap();
        queue.push(frame(3, false)).unwrap();
        let loss = queue.push(frame(4, false)).unwrap();
        assert_eq!(loss.dropped, 3);
        assert!(loss.request_keyframe);
        assert!(
            queue
                .submit_if_current(old_keyframe.generation, || panic!("stale submission"))
                .is_none()
        );
        let dependent = queue.push(frame(5, false)).unwrap();
        assert_eq!(dependent.dropped, 1);
        assert!(!dependent.request_keyframe);
        queue.push(frame(6, true)).unwrap();
        queue.push(frame(7, false)).unwrap();
        let fresh = queue.pop_packet().unwrap();
        assert!(fresh.reset_decoder);
        assert_eq!(fresh.frame.frame_index, Some(6));
        assert_eq!(fresh.frame.timestamp, 4500);
        assert_eq!(
            queue.submit_if_current(fresh.generation, || true),
            Some(true)
        );
        let delta = queue.pop_packet().unwrap();
        assert_eq!(delta.frame.frame_index, Some(7));
        assert!(!delta.reset_decoder);
    }

    #[test]
    fn packet_gap_discards_dependents_and_accepts_a_complete_keyframe() {
        let queue = VideoQueue::new(3);
        queue.push(frame(1, true)).unwrap();
        queue.push(frame(2, false)).unwrap();
        let mut recovery = frame(4, true);
        recovery.contiguous = false;
        let result = queue.push(recovery).unwrap();
        assert_eq!(result.dropped, 2);
        assert!(!result.request_keyframe);
        let packet = queue.pop_packet().unwrap();
        assert_eq!(packet.frame.frame_index, Some(4));
        assert!(packet.reset_decoder);
    }

    #[test]
    fn overflow_retains_incoming_keyframe_without_extra_round_trip() {
        let queue = VideoQueue::new(2);
        queue.push(frame(1, true)).unwrap();
        queue.push(frame(2, false)).unwrap();
        let result = queue.push(frame(3, true)).unwrap();
        assert_eq!(result.dropped, 2);
        assert!(!result.request_keyframe);
        assert_eq!(queue.pop_packet().unwrap().frame.frame_index, Some(3));
    }

    #[test]
    fn clear_and_close_invalidate_in_flight_frames() {
        let queue = VideoQueue::new(2);
        queue.push(frame(1, true)).unwrap();
        let old = queue.pop_packet().unwrap();
        queue.clear();
        assert!(queue.submit_if_current(old.generation, || ()).is_none());
        assert!(queue.push(frame(2, false)).unwrap().request_keyframe);
        queue.push(frame(3, true)).unwrap();
        let new = queue.pop_packet().unwrap();
        queue.close();
        assert!(queue.submit_if_current(new.generation, || ()).is_none());
        assert!(queue.pop_packet().is_none());
        assert!(queue.push(frame(4, true)).is_err());
    }

    #[test]
    fn loss_and_downstream_admission_share_the_same_lock() {
        let queue = Arc::new(VideoQueue::new(2));
        queue.push(frame(1, true)).unwrap();
        let packet = queue.pop_packet().unwrap();
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let worker_queue = Arc::clone(&queue);
        let submit = std::thread::spawn(move || {
            worker_queue.submit_if_current(packet.generation, || {
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
        });
        entered_rx.recv().unwrap();
        // Loss cannot become visible in the middle of downstream admission.
        assert!(queue.state.try_lock().is_err());
        release_tx.send(()).unwrap();
        assert_eq!(submit.join().unwrap(), Some(()));
        queue.clear();
        assert!(queue.submit_if_current(packet.generation, || ()).is_none());
    }
}
