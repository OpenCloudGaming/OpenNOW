//! Bounded stereo playout with gentle recovery from a persistent burst backlog.
//!
//! Ordinary packets are copied unchanged. Only a backlog above 60 ms sustained
//! for 200 ms enables a temporary, at-most-2% catch-up resample. Hysteresis stops
//! recovery at 40 ms. The callback neither allocates nor drops whole chunks of
//! audible PCM; both channels always use the same fractional source position.

use std::collections::VecDeque;

const CHANNELS: usize = 2;
const SAMPLE_RATE: usize = 48_000;
const TARGET_FRAMES: usize = SAMPLE_RATE * 40 / 1_000;
const HIGH_FRAMES: usize = SAMPLE_RATE * 60 / 1_000;
const PERSISTENT_FRAMES: usize = SAMPLE_RATE * 200 / 1_000;

#[derive(Debug)]
pub(crate) struct AudioPlayoutBuffer {
    samples: VecDeque<f32>,
    capacity: usize,
    high_frames: usize,
    recovering: bool,
}

impl AudioPlayoutBuffer {
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    pub(crate) fn new(capacity: usize) -> Self {
        assert!(capacity >= CHANNELS && capacity % CHANNELS == 0);
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
            high_frames: 0,
            recovering: false,
        }
    }

    pub(crate) fn push(&mut self, samples: &[f32]) -> usize {
        // Never allow a partial interleaved frame to shift channel alignment.
        let samples = &samples[..samples.len() / CHANNELS * CHANNELS];
        let overflow = self
            .samples
            .len()
            .saturating_add(samples.len())
            .saturating_sub(self.capacity);
        self.samples.drain(..overflow.min(self.samples.len()));
        self.samples.extend(
            samples[samples.len().saturating_sub(self.capacity)..]
                .iter()
                .copied(),
        );
        overflow
    }

    pub(crate) fn clear(&mut self) {
        self.samples.clear();
        self.high_frames = 0;
        self.recovering = false;
    }

    pub(crate) fn fill(&mut self, destination: &mut [f32]) {
        let output_frames = destination.len() / CHANNELS;
        let available = self.samples.len() / CHANNELS;
        let backlog = available.saturating_sub(output_frames);
        if backlog > HIGH_FRAMES {
            self.high_frames = self.high_frames.saturating_add(output_frames);
            self.recovering |= self.high_frames >= PERSISTENT_FRAMES;
        } else {
            self.high_frames = 0;
        }
        if backlog <= TARGET_FRAMES {
            self.recovering = false;
        }
        let extra = if self.recovering {
            (output_frames / 50).min(backlog.saturating_sub(TARGET_FRAMES))
        } else {
            0
        };
        if extra == 0 {
            for sample in destination.iter_mut().take(output_frames * CHANNELS) {
                *sample = self.samples.pop_front().unwrap_or(0.0);
            }
        } else {
            let consumed = output_frames + extra;
            for frame in 0..output_frames {
                let position = frame as f64 * consumed as f64 / output_frames as f64;
                let first = position as usize;
                let fraction = (position - first as f64) as f32;
                for channel in 0..CHANNELS {
                    let a = self.samples[first * CHANNELS + channel];
                    let b = self.samples[(first + 1) * CHANNELS + channel];
                    destination[frame * CHANNELS + channel] = a + (b - a) * fraction;
                }
            }
            self.samples.drain(..consumed * CHANNELS);
        }
        if destination.len() % CHANNELS != 0 {
            *destination.last_mut().unwrap() = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_audio_is_bit_exact_and_underruns_are_silent() {
        let mut audio = AudioPlayoutBuffer::new(11_520);
        assert_eq!(audio.push(&[0.25, -0.25, 0.5, -0.5]), 0);
        let mut output = [1.0; 6];
        audio.fill(&mut output);
        assert_eq!(output, [0.25, -0.25, 0.5, -0.5, 0.0, 0.0]);
    }

    #[test]
    fn overflow_preserves_newest_complete_stereo_frames() {
        let mut audio = AudioPlayoutBuffer::new(4);
        audio.push(&[1.0, 2.0]);
        assert_eq!(audio.push(&[3.0, 4.0, 5.0, 6.0]), 2);
        let mut output = [0.0; 4];
        audio.fill(&mut output);
        assert_eq!(output, [3.0, 4.0, 5.0, 6.0]);
    }

    #[test]
    fn sustained_backlog_recovers_without_underruns_or_channel_skew() {
        let mut audio = AudioPlayoutBuffer::new(11_520);
        let packet: Vec<_> = (0..480).flat_map(|_| [0.25, -0.5]).collect();
        for _ in 0..11 {
            audio.push(&packet);
        }
        let mut output = [0.0; 960];
        for _ in 0..600 {
            audio.push(&packet);
            audio.fill(&mut output);
            assert!(output.chunks_exact(2).all(|v| v == [0.25, -0.5]));
        }
        assert_eq!(audio.samples.len() / CHANNELS, TARGET_FRAMES);
        assert!(!audio.recovering);
    }

    #[test]
    fn short_burst_does_not_enable_resampling_and_clear_resets_recovery() {
        let mut audio = AudioPlayoutBuffer::new(11_520);
        audio.push(&vec![0.5; 11_520]);
        let mut output = [0.0; 960];
        for _ in 0..10 {
            audio.fill(&mut output);
        }
        assert!(!audio.recovering);
        audio.high_frames = PERSISTENT_FRAMES;
        audio.recovering = true;
        audio.clear();
        assert_eq!(audio.high_frames, 0);
        assert!(!audio.recovering);
    }
}
