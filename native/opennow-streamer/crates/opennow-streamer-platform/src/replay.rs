use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use opennow_streamer_protocol::ReplayBufferConfig;

use crate::media::{EncodedFrame, MediaCodec};

const MAX_FRAMES: usize = 32_768;
const FRAME_OVERHEAD: usize = 256;

pub(crate) struct ReplayFrame {
    pub frame: EncodedFrame,
    pub time: Duration,
}

pub struct ReplaySnapshot {
    pub(crate) frames: VecDeque<ReplayFrame>,
    pub(crate) cancelled: Arc<AtomicBool>,
    reserved: Arc<AtomicUsize>,
}

impl Drop for ReplaySnapshot {
    fn drop(&mut self) {
        self.frames.clear();
        self.reserved.store(0, Ordering::Release);
    }
}

struct Clock {
    timestamp: u64,
    rate: u32,
    time: Duration,
    codec: MediaCodec,
}

impl Clock {
    fn advance(&mut self, frame: &EncodedFrame) -> Option<Duration> {
        if frame.clock_rate_hz == 0 || self.rate != frame.clock_rate_hz || self.codec != frame.codec
        {
            return None;
        }
        let delta =
            if self.timestamp <= u64::from(u32::MAX) && frame.timestamp <= u64::from(u32::MAX) {
                u64::from((frame.timestamp as u32).wrapping_sub(self.timestamp as u32))
            } else {
                frame.timestamp.checked_sub(self.timestamp)?
            };
        if delta > u64::from(self.rate) * 10 {
            return None;
        }
        self.time += Duration::from_secs_f64(delta as f64 / f64::from(self.rate));
        self.timestamp = frame.timestamp;
        Some(self.time)
    }
}

struct Buffer {
    config: ReplayBufferConfig,
    frames: VecDeque<ReplayFrame>,
    bytes: usize,
    origin: Instant,
    video: Option<Clock>,
    audio: Option<Clock>,
    latest: Duration,
    cancelled: Arc<AtomicBool>,
    reserved: Arc<AtomicUsize>,
}

impl Buffer {
    fn clear(&mut self) {
        self.frames.clear();
        self.bytes = 0;
        self.video = None;
        self.audio = None;
        self.origin = Instant::now();
        self.latest = Duration::ZERO;
    }

    fn pop_front(&mut self) {
        if let Some(frame) = self.frames.pop_front() {
            self.bytes -= frame.frame.data.len() + frame.frame.mid.len() + FRAME_OVERHEAD;
        }
    }

    fn push(&mut self, frame: &EncodedFrame, reserved: usize, received_at: Instant) {
        if !frame.contiguous {
            self.clear();
            return;
        }
        let video = !matches!(frame.codec, MediaCodec::Opus { .. });
        let clock = if video {
            &mut self.video
        } else {
            &mut self.audio
        };
        let time = if let Some(clock) = clock {
            let Some(time) = clock.advance(frame) else {
                self.clear();
                return;
            };
            time
        } else {
            if frame.clock_rate_hz == 0 {
                return;
            }
            let time = received_at.saturating_duration_since(self.origin);
            *clock = Some(Clock {
                timestamp: frame.timestamp,
                rate: frame.clock_rate_hz,
                time,
                codec: frame.codec.clone(),
            });
            time
        };
        self.latest = self.latest.max(time);
        if self.frames.front().is_some_and(|first| time < first.time) {
            return;
        }
        if self.frames.is_empty() && !(video && frame.keyframe) {
            return;
        }
        let budget = self.config.memory_bytes.saturating_sub(reserved);
        let bytes = frame
            .data
            .len()
            .saturating_add(frame.mid.len())
            .saturating_add(FRAME_OVERHEAD);
        if bytes > budget {
            self.clear();
            return;
        }
        self.frames.push_back(ReplayFrame {
            frame: frame.clone(),
            time,
        });
        self.bytes += bytes;
        while self.bytes > budget
            || self.frames.len() > MAX_FRAMES
            || self
                .frames
                .front()
                .is_some_and(|first| self.latest.saturating_sub(first.time) > self.config.duration)
        {
            self.pop_front();
        }
        while self.frames.front().is_some_and(|first| {
            !first.frame.keyframe || matches!(first.frame.codec, MediaCodec::Opus { .. })
        }) {
            self.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(bytes: usize, seconds: u64) -> ReplayBufferConfig {
        ReplayBufferConfig {
            enabled: true,
            memory_bytes: bytes,
            duration: Duration::from_secs(seconds),
        }
    }

    fn frame(timestamp: u64, keyframe: bool) -> EncodedFrame {
        EncodedFrame {
            mid: "video".to_owned(),
            codec: MediaCodec::H264,
            data: Arc::from([0_u8; 16]),
            frame_index: Some(timestamp as u32),
            timestamp,
            clock_rate_hz: 1,
            keyframe,
            contiguous: true,
        }
    }

    #[test]
    fn long_gop_is_discarded_until_a_new_source_keyframe() {
        let tap = ReplayTap::default();
        tap.start(config(100_000, 2));
        for index in 0..10 {
            tap.publish(&frame(index, index == 0));
        }
        assert_eq!(tap.snapshot().err(), Some("replay-not-ready"));
        tap.publish(&frame(10, true));
        let snapshot = tap.snapshot().unwrap();
        assert_eq!(snapshot.frames.len(), 1);
        assert_eq!(snapshot.frames[0].frame.timestamp, 10);
    }

    #[test]
    fn audio_offset_uses_receive_anchor_then_source_clock_not_arrival_jitter() {
        let tap = ReplayTap::default();
        tap.start(config(100_000, 30));
        let mut guard = tap.buffer.lock().unwrap();
        let buffer = guard.as_mut().unwrap();
        let origin = buffer.origin;
        let mut video = frame(9_000_000, true);
        video.clock_rate_hz = 90_000;
        let mut audio = frame(400_000, false);
        audio.codec = MediaCodec::Opus { channels: 2 };
        audio.clock_rate_hz = 48_000;
        buffer.push(&video, 0, origin + Duration::from_secs(1));
        buffer.push(&audio, 0, origin + Duration::from_millis(1_100));
        audio.timestamp += 960;
        buffer.push(&audio, 0, origin + Duration::from_millis(1_500));
        video.timestamp += 18_000;
        video.keyframe = false;
        buffer.push(&video, 0, origin + Duration::from_millis(1_600));
        assert_eq!(buffer.frames[0].time, Duration::from_secs(1));
        assert_eq!(buffer.frames[1].time, Duration::from_millis(1_100));
        assert_eq!(buffer.frames[2].time, Duration::from_millis(1_120));
        assert_eq!(buffer.frames[3].time, Duration::from_millis(1_200));
        assert_eq!(buffer.frames[1].frame.timestamp, 400_000);
        assert_eq!(buffer.frames[2].frame.timestamp, 400_960);
    }

    #[test]
    fn cancelled_previous_session_keeps_its_reservation_until_worker_releases_it() {
        let budget = Arc::new(AtomicUsize::new(0));
        let old = ReplayTap::default();
        old.start_with_budget(config(16 + FRAME_OVERHEAD + 5, 30), Arc::clone(&budget));
        old.publish(&frame(0, true));
        let snapshot = old.snapshot().unwrap();
        old.stop();
        assert!(snapshot.cancelled.load(Ordering::Acquire));
        let new = ReplayTap::default();
        new.start_with_budget(config(16 + FRAME_OVERHEAD + 5, 30), Arc::clone(&budget));
        new.publish(&frame(0, true));
        assert!(
            new.buffer
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .frames
                .is_empty()
        );
        drop(snapshot);
        new.publish(&frame(1, true));
        assert!(new.snapshot().is_ok());
    }

    #[test]
    fn disabled_replay_allocates_no_buffer_and_retains_no_payload() {
        let tap = ReplayTap::default();
        tap.start(ReplayBufferConfig::from_settings(&serde_json::json!({})));
        let frame = frame(0, true);
        tap.publish(&frame);
        assert!(tap.buffer.lock().unwrap().is_none());
        assert_eq!(Arc::strong_count(&frame.data), 1);
        assert_eq!(tap.snapshot().err(), Some("replay-not-enabled"));
    }

    #[test]
    fn evicts_whole_gops_for_byte_and_source_time_bounds() {
        for cfg in [
            config(3 * (16 + FRAME_OVERHEAD + 5), 100),
            config(100_000, 2),
        ] {
            let tap = ReplayTap::default();
            tap.start(cfg);
            for index in 0..6 {
                tap.publish(&frame(index, index % 2 == 0));
            }
            let snapshot = tap.snapshot().unwrap();
            assert_eq!(snapshot.frames.front().unwrap().frame.timestamp, 4);
            assert_eq!(snapshot.frames.len(), 2);
            assert!(snapshot.frames.front().unwrap().frame.keyframe);
            assert!(snapshot.reserved.load(Ordering::Acquire) <= cfg.memory_bytes);
            assert!(
                snapshot.frames.back().unwrap().time - snapshot.frames.front().unwrap().time
                    <= cfg.duration
            );
        }
    }

    #[test]
    fn discontinuity_and_contention_reset_until_keyframe() {
        let tap = ReplayTap::default();
        tap.start(config(100_000, 30));
        tap.publish(&frame(0, true));
        let mut broken = frame(1, false);
        broken.contiguous = false;
        tap.publish(&broken);
        tap.publish(&frame(2, false));
        assert_eq!(tap.snapshot().err(), Some("replay-not-ready"));
        tap.publish(&frame(3, true));
        let guard = tap.buffer.lock().unwrap();
        tap.publish(&frame(4, false));
        drop(guard);
        assert_eq!(tap.snapshot().err(), Some("replay-not-ready"));
        tap.publish(&frame(5, true));
        assert_eq!(tap.snapshot().unwrap().frames.len(), 1);
    }

    #[test]
    fn export_reserves_shared_budget_and_only_one_snapshot() {
        let tap = ReplayTap::default();
        tap.start(config(2 * (16 + FRAME_OVERHEAD + 5), 30));
        let source = frame(0, true);
        tap.publish(&source);
        let snapshot = tap.snapshot().unwrap();
        assert_eq!(Arc::strong_count(&source.data), 2);
        for index in 1..5 {
            tap.publish(&frame(index, true));
        }
        assert_eq!(tap.snapshot().err(), Some("clip-already-saving"));
        let guard = tap.buffer.lock().unwrap();
        assert!(
            guard.as_ref().unwrap().bytes + snapshot.reserved.load(Ordering::Acquire)
                <= 2 * (16 + FRAME_OVERHEAD + 5)
        );
        drop(guard);
        drop(snapshot);
        assert_eq!(Arc::strong_count(&source.data), 1);
        assert!(tap.snapshot().is_ok());
        tap.stop();
        assert!(tap.buffer.lock().unwrap().is_none());
    }

    #[test]
    fn oversized_frames_clock_changes_and_backward_timestamps_reset() {
        let tap = ReplayTap::default();
        tap.start(config(16 + FRAME_OVERHEAD + 5, 30));
        tap.publish(&frame(10, true));
        tap.publish(&frame(9, false));
        assert_eq!(tap.snapshot().err(), Some("replay-not-ready"));
        tap.publish(&frame(10, true));
        let mut changed = frame(11, false);
        changed.clock_rate_hz = 2;
        tap.publish(&changed);
        assert_eq!(tap.snapshot().err(), Some("replay-not-ready"));
        let mut oversized = frame(0, true);
        oversized.data = Arc::from([0_u8; 17]);
        tap.publish(&oversized);
        assert_eq!(tap.snapshot().err(), Some("replay-not-ready"));
    }

    #[test]
    fn source_clock_wrap_preserves_elapsed_time() {
        let mut clock = Clock {
            timestamp: u64::from(u32::MAX),
            rate: 1,
            time: Duration::from_secs(5),
            codec: MediaCodec::H264,
        };
        assert_eq!(clock.advance(&frame(1, true)), Some(Duration::from_secs(7)));
    }
}

#[derive(Default)]
pub(crate) struct ReplayTap {
    enabled: AtomicBool,
    reset: AtomicBool,
    buffer: Mutex<Option<Buffer>>,
}

impl ReplayTap {
    #[cfg(test)]
    pub fn start(&self, config: ReplayBufferConfig) {
        self.start_with_budget(config, Arc::new(AtomicUsize::new(0)));
    }

    pub fn start_with_budget(&self, config: ReplayBufferConfig, reserved: Arc<AtomicUsize>) {
        self.stop();
        if !config.enabled {
            return;
        }
        *self
            .buffer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(Buffer {
            config,
            frames: VecDeque::new(),
            bytes: 0,
            origin: Instant::now(),
            video: None,
            audio: None,
            latest: Duration::ZERO,
            cancelled: Arc::new(AtomicBool::new(false)),
            reserved,
        });
        self.enabled.store(true, Ordering::Release);
    }

    pub fn stop(&self) {
        self.enabled.store(false, Ordering::Release);
        if let Some(buffer) = self
            .buffer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take()
        {
            buffer.cancelled.store(true, Ordering::Release);
        }
        self.reset.store(false, Ordering::Release);
    }

    pub fn publish(&self, frame: &EncodedFrame) {
        if !self.enabled.load(Ordering::Acquire) {
            return;
        }
        let Ok(mut guard) = self.buffer.try_lock() else {
            self.reset.store(true, Ordering::Release);
            return;
        };
        if let Some(buffer) = guard.as_mut() {
            if self.reset.swap(false, Ordering::AcqRel) {
                buffer.clear();
            }
            buffer.push(
                frame,
                buffer.reserved.load(Ordering::Acquire),
                Instant::now(),
            );
        }
    }

    pub fn snapshot(&self) -> Result<ReplaySnapshot, &'static str> {
        let mut guard = self
            .buffer
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let buffer = guard.as_mut().ok_or("replay-not-enabled")?;
        if self.reset.swap(false, Ordering::AcqRel) {
            buffer.clear();
        }
        if buffer.reserved.load(Ordering::Acquire) != 0 {
            return Err("clip-already-saving");
        }
        if buffer.frames.is_empty() {
            return Err("replay-not-ready");
        }
        buffer.reserved.store(buffer.bytes, Ordering::Release);
        buffer.bytes = 0;
        Ok(ReplaySnapshot {
            frames: std::mem::take(&mut buffer.frames),
            reserved: Arc::clone(&buffer.reserved),
            cancelled: Arc::clone(&buffer.cancelled),
        })
    }
}
