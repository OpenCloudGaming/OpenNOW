//! Time Utilities
//!
//! High-precision timing for input and frame synchronization.

use std::time::{Duration, Instant};

/// High-precision timer for measuring frame times
pub struct FrameTimer {
    start: Instant,
    last_frame: Instant,
    frame_count: u64,
    frame_times: Vec<Duration>,
}

impl FrameTimer {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            start: now,
            last_frame: now,
            frame_count: 0,
            frame_times: Vec::with_capacity(120),
        }
    }

    /// Mark a new frame and return delta time
    pub fn tick(&mut self) -> Duration {
        let now = Instant::now();
        let delta = now - self.last_frame;
        self.last_frame = now;
        self.frame_count += 1;

        // Keep last 120 frame times for FPS calculation
        self.frame_times.push(delta);
        if self.frame_times.len() > 120 {
            self.frame_times.remove(0);
        }

        delta
    }

    /// Get current FPS based on recent frame times
    pub fn fps(&self) -> f32 {
        if self.frame_times.is_empty() {
            return 0.0;
        }

        let total: Duration = self.frame_times.iter().sum();
        let avg = total.as_secs_f32() / self.frame_times.len() as f32;

        if avg > 0.0 {
            1.0 / avg
        } else {
            0.0
        }
    }

    /// Get total elapsed time since start
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// Get total frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Get average frame time in milliseconds
    pub fn avg_frame_time_ms(&self) -> f32 {
        if self.frame_times.is_empty() {
            return 0.0;
        }

        let total: Duration = self.frame_times.iter().sum();
        total.as_secs_f32() * 1000.0 / self.frame_times.len() as f32
    }
}

impl Default for FrameTimer {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current timestamp in microseconds (for input events)
pub fn timestamp_us() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_micros() as u64)
        .unwrap_or(0)
}

/// Get current timestamp in milliseconds
pub fn timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Relative timestamp from a start time (in microseconds)
pub struct RelativeTimer {
    start: Instant,
}

impl RelativeTimer {
    pub fn new() -> Self {
        Self {
            start: Instant::now(),
        }
    }

    /// Get microseconds since start
    pub fn elapsed_us(&self) -> u64 {
        self.start.elapsed().as_micros() as u64
    }

    /// Get milliseconds since start
    pub fn elapsed_ms(&self) -> u64 {
        self.start.elapsed().as_millis() as u64
    }
}

impl Default for RelativeTimer {
    fn default() -> Self {
        Self::new()
    }
}
