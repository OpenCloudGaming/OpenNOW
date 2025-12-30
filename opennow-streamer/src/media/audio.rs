//! Audio Decoder and Player
//!
//! Decode Opus audio and play through cpal.
//! NOTE: Opus decoding is stubbed - will add proper decoder later.

use anyhow::{Result, Context};
use log::{info, warn, error};
use std::sync::Arc;
use parking_lot::Mutex;

/// Audio decoder (stubbed - no opus decoding yet)
pub struct AudioDecoder {
    sample_rate: u32,
    channels: u32,
}

impl AudioDecoder {
    /// Create a new audio decoder (stubbed)
    pub fn new(sample_rate: u32, channels: u32) -> Result<Self> {
        info!("Creating audio decoder (stubbed): {}Hz, {} channels", sample_rate, channels);
        warn!("Opus decoding not yet implemented - audio will be silent");

        Ok(Self {
            sample_rate,
            channels,
        })
    }

    /// Decode an Opus packet (stubbed - returns silence)
    pub fn decode(&mut self, data: &[u8]) -> Result<Vec<i16>> {
        // Return silence for now
        // Each Opus frame is typically 20ms at 48kHz = 960 samples
        let samples_per_frame = (self.sample_rate / 50) as usize; // 20ms frame
        let total_samples = samples_per_frame * self.channels as usize;
        Ok(vec![0i16; total_samples])
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get channel count
    pub fn channels(&self) -> u32 {
        self.channels
    }
}

/// Audio player using cpal
pub struct AudioPlayer {
    sample_rate: u32,
    channels: u32,
    buffer: Arc<Mutex<AudioBuffer>>,
    _stream: Option<cpal::Stream>,
}

struct AudioBuffer {
    samples: Vec<i16>,
    read_pos: usize,
    write_pos: usize,
    capacity: usize,
}

impl AudioBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            samples: vec![0i16; capacity],
            read_pos: 0,
            write_pos: 0,
            capacity,
        }
    }

    fn write(&mut self, data: &[i16]) {
        for &sample in data {
            self.samples[self.write_pos] = sample;
            self.write_pos = (self.write_pos + 1) % self.capacity;
        }
    }

    fn read(&mut self, out: &mut [i16]) -> usize {
        let mut count = 0;
        for sample in out.iter_mut() {
            if self.read_pos == self.write_pos {
                *sample = 0; // Underrun - output silence
            } else {
                *sample = self.samples[self.read_pos];
                self.read_pos = (self.read_pos + 1) % self.capacity;
                count += 1;
            }
        }
        count
    }
}

impl AudioPlayer {
    /// Create a new audio player
    pub fn new(sample_rate: u32, channels: u32) -> Result<Self> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        info!("Creating audio player: {}Hz, {} channels", sample_rate, channels);

        let host = cpal::default_host();

        let device = host.default_output_device()
            .context("No audio output device found")?;

        info!("Using audio device: {}", device.name().unwrap_or_default());

        // Buffer for ~200ms of audio
        let buffer_size = (sample_rate as usize) * (channels as usize) / 5;
        let buffer = Arc::new(Mutex::new(AudioBuffer::new(buffer_size)));

        let config = cpal::StreamConfig {
            channels: channels as u16,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let buffer_clone = buffer.clone();

        let stream = device.build_output_stream(
            &config,
            move |data: &mut [i16], _| {
                let mut buf = buffer_clone.lock();
                buf.read(data);
            },
            |err| {
                error!("Audio stream error: {}", err);
            },
            None,
        ).context("Failed to create audio stream")?;

        stream.play().context("Failed to start audio playback")?;

        info!("Audio player started");

        Ok(Self {
            sample_rate,
            channels,
            buffer,
            _stream: Some(stream),
        })
    }

    /// Push audio samples to the player
    pub fn push_samples(&self, samples: &[i16]) {
        let mut buffer = self.buffer.lock();
        buffer.write(samples);
    }

    /// Get sample rate
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Get channel count
    pub fn channels(&self) -> u32 {
        self.channels
    }
}
