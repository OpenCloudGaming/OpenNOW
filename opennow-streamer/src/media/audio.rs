//! Audio Decoder and Player
//!
//! Decode Opus audio using FFmpeg and play through cpal.

use anyhow::{Result, Context, anyhow};
use log::{info, error, debug};
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use parking_lot::Mutex;

extern crate ffmpeg_next as ffmpeg;

use ffmpeg::codec::{decoder, context::Context as CodecContext};
use ffmpeg::Packet;

/// Audio decoder using FFmpeg for Opus
pub struct AudioDecoder {
    cmd_tx: mpsc::Sender<AudioCommand>,
    frame_rx: mpsc::Receiver<Vec<i16>>,
    sample_rate: u32,
    channels: u32,
}

enum AudioCommand {
    Decode(Vec<u8>),
    Stop,
}

impl AudioDecoder {
    /// Create a new Opus audio decoder using FFmpeg
    pub fn new(sample_rate: u32, channels: u32) -> Result<Self> {
        info!("Creating Opus audio decoder: {}Hz, {} channels", sample_rate, channels);

        // Initialize FFmpeg (may already be initialized by video decoder)
        let _ = ffmpeg::init();

        // Create channels for thread communication
        let (cmd_tx, cmd_rx) = mpsc::channel::<AudioCommand>();
        let (frame_tx, frame_rx) = mpsc::channel::<Vec<i16>>();

        // Spawn decoder thread (FFmpeg types are not Send)
        let sample_rate_clone = sample_rate;
        let channels_clone = channels;

        thread::spawn(move || {
            // Find Opus decoder
            let codec = match ffmpeg::codec::decoder::find(ffmpeg::codec::Id::OPUS) {
                Some(c) => c,
                None => {
                    error!("Opus decoder not found in FFmpeg");
                    return;
                }
            };

            let ctx = CodecContext::new_with_codec(codec);

            // Set parameters for Opus
            // Note: FFmpeg Opus decoder auto-detects most parameters from the bitstream

            let mut decoder = match ctx.decoder().audio() {
                Ok(d) => d,
                Err(e) => {
                    error!("Failed to create Opus decoder: {:?}", e);
                    return;
                }
            };

            info!("Opus audio decoder initialized");

            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    AudioCommand::Decode(data) => {
                        let samples = Self::decode_opus_packet(&mut decoder, &data, sample_rate_clone, channels_clone);
                        let _ = frame_tx.send(samples);
                    }
                    AudioCommand::Stop => break,
                }
            }

            debug!("Audio decoder thread stopped");
        });

        Ok(Self {
            cmd_tx,
            frame_rx,
            sample_rate,
            channels,
        })
    }

    /// Decode an Opus packet from RTP payload
    fn decode_opus_packet(
        decoder: &mut decoder::Audio,
        data: &[u8],
        target_sample_rate: u32,
        target_channels: u32,
    ) -> Vec<i16> {
        if data.is_empty() {
            return Vec::new();
        }

        // Create packet from raw Opus data
        let mut packet = Packet::new(data.len());
        if let Some(pkt_data) = packet.data_mut() {
            pkt_data.copy_from_slice(data);
        } else {
            return Vec::new();
        }

        // Send packet to decoder
        if let Err(e) = decoder.send_packet(&packet) {
            match e {
                ffmpeg::Error::Other { errno } if errno == libc::EAGAIN => {}
                _ => debug!("Audio send packet error: {:?}", e),
            }
        }

        // Receive decoded audio frame
        let mut frame = ffmpeg::frame::Audio::empty();
        match decoder.receive_frame(&mut frame) {
            Ok(_) => {
                // Convert frame to i16 samples
                let samples = Self::frame_to_samples(&frame, target_sample_rate, target_channels);
                samples
            }
            Err(ffmpeg::Error::Other { errno }) if errno == libc::EAGAIN => {
                Vec::new()
            }
            Err(e) => {
                debug!("Audio receive frame error: {:?}", e);
                Vec::new()
            }
        }
    }

    /// Convert FFmpeg audio frame to i16 samples
    fn frame_to_samples(
        frame: &ffmpeg::frame::Audio,
        _target_sample_rate: u32,
        target_channels: u32,
    ) -> Vec<i16> {
        use ffmpeg::format::Sample;

        let nb_samples = frame.samples();
        let channels = frame.channels() as usize;

        if nb_samples == 0 || channels == 0 {
            return Vec::new();
        }

        let format = frame.format();
        let mut output = Vec::with_capacity(nb_samples * target_channels as usize);

        // Handle different sample formats
        match format {
            Sample::I16(planar) => {
                if planar == ffmpeg::format::sample::Type::Planar {
                    // Planar format - interleave channels
                    for i in 0..nb_samples {
                        for ch in 0..channels.min(target_channels as usize) {
                            let plane = frame.plane::<i16>(ch);
                            if i < plane.len() {
                                output.push(plane[i]);
                            }
                        }
                        // Fill remaining channels with zeros if needed
                        for _ in channels..target_channels as usize {
                            output.push(0);
                        }
                    }
                } else {
                    // Packed format - already interleaved
                    let data = frame.plane::<i16>(0);
                    output.extend_from_slice(&data[..nb_samples * channels]);
                }
            }
            Sample::F32(planar) => {
                // Convert f32 to i16
                if planar == ffmpeg::format::sample::Type::Planar {
                    for i in 0..nb_samples {
                        for ch in 0..channels.min(target_channels as usize) {
                            let plane = frame.plane::<f32>(ch);
                            if i < plane.len() {
                                let sample = (plane[i] * 32767.0).clamp(-32768.0, 32767.0) as i16;
                                output.push(sample);
                            }
                        }
                        for _ in channels..target_channels as usize {
                            output.push(0);
                        }
                    }
                } else {
                    let data = frame.plane::<f32>(0);
                    for sample in &data[..nb_samples * channels] {
                        let s = (*sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
                        output.push(s);
                    }
                }
            }
            _ => {
                // For other formats, try to get as bytes and convert
                debug!("Unsupported audio format: {:?}, returning silence", format);
                output.resize(nb_samples * target_channels as usize, 0);
            }
        }

        output
    }

    /// Decode an Opus packet (sends to decoder thread)
    pub fn decode(&mut self, data: &[u8]) -> Result<Vec<i16>> {
        self.cmd_tx.send(AudioCommand::Decode(data.to_vec()))
            .map_err(|_| anyhow!("Audio decoder thread closed"))?;

        match self.frame_rx.recv() {
            Ok(samples) => Ok(samples),
            Err(_) => Err(anyhow!("Audio decoder thread closed")),
        }
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

impl Drop for AudioDecoder {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(AudioCommand::Stop);
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
    total_written: u64,
    total_read: u64,
}

impl AudioBuffer {
    fn new(capacity: usize) -> Self {
        Self {
            samples: vec![0i16; capacity],
            read_pos: 0,
            write_pos: 0,
            capacity,
            total_written: 0,
            total_read: 0,
        }
    }

    fn available(&self) -> usize {
        if self.write_pos >= self.read_pos {
            self.write_pos - self.read_pos
        } else {
            self.capacity - self.read_pos + self.write_pos
        }
    }

    fn write(&mut self, data: &[i16]) {
        for &sample in data {
            let next_pos = (self.write_pos + 1) % self.capacity;
            // Don't overwrite unread data (drop samples if buffer is full)
            if next_pos != self.read_pos {
                self.samples[self.write_pos] = sample;
                self.write_pos = next_pos;
                self.total_written += 1;
            }
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
                self.total_read += 1;
            }
        }
        count
    }
}

impl AudioPlayer {
    /// Create a new audio player
    pub fn new(sample_rate: u32, channels: u32) -> Result<Self> {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
        use cpal::SampleFormat;

        info!("Creating audio player: {}Hz, {} channels", sample_rate, channels);

        let host = cpal::default_host();

        let device = host.default_output_device()
            .context("No audio output device found")?;

        info!("Using audio device: {}", device.name().unwrap_or_default());

        // Query supported configurations
        let supported_configs: Vec<_> = device.supported_output_configs()
            .map(|configs| configs.collect())
            .unwrap_or_default();

        if supported_configs.is_empty() {
            return Err(anyhow!("No supported audio configurations found"));
        }

        // Log available configurations for debugging
        for cfg in &supported_configs {
            debug!("Supported config: {:?} channels, {:?}-{:?} Hz, format {:?}",
                cfg.channels(), cfg.min_sample_rate().0, cfg.max_sample_rate().0, cfg.sample_format());
        }

        // Find best matching configuration
        // Prefer: f32 format (most compatible), matching channels, matching sample rate
        let target_rate = cpal::SampleRate(sample_rate);
        let target_channels = channels as u16;

        // Try to find a config that supports our sample rate and channel count
        let mut best_config = None;
        let mut best_score = 0i32;

        for cfg in &supported_configs {
            let mut score = 0i32;

            // Prefer f32 format (most widely supported)
            if cfg.sample_format() == SampleFormat::F32 {
                score += 100;
            } else if cfg.sample_format() == SampleFormat::I16 {
                score += 50;
            }

            // Prefer matching channel count
            if cfg.channels() == target_channels {
                score += 50;
            } else if cfg.channels() >= target_channels {
                score += 25;
            }

            // Check if sample rate is in range
            if target_rate >= cfg.min_sample_rate() && target_rate <= cfg.max_sample_rate() {
                score += 100;
            } else if cfg.max_sample_rate().0 >= 44100 {
                score += 25; // At least supports reasonable rates
            }

            if score > best_score {
                best_score = score;
                best_config = Some(cfg.clone());
            }
        }

        let supported_range = best_config
            .ok_or_else(|| anyhow!("No suitable audio configuration found"))?;

        // Determine actual sample rate to use
        let actual_rate = if target_rate >= supported_range.min_sample_rate()
            && target_rate <= supported_range.max_sample_rate() {
            target_rate
        } else if cpal::SampleRate(48000) >= supported_range.min_sample_rate()
            && cpal::SampleRate(48000) <= supported_range.max_sample_rate() {
            cpal::SampleRate(48000)
        } else if cpal::SampleRate(44100) >= supported_range.min_sample_rate()
            && cpal::SampleRate(44100) <= supported_range.max_sample_rate() {
            cpal::SampleRate(44100)
        } else {
            supported_range.max_sample_rate()
        };

        let actual_channels = supported_range.channels();
        let sample_format = supported_range.sample_format();

        info!("Using audio config: {}Hz, {} channels, format {:?}",
            actual_rate.0, actual_channels, sample_format);

        // Buffer for ~200ms of audio
        let buffer_size = (actual_rate.0 as usize) * (actual_channels as usize) / 5;
        let buffer = Arc::new(Mutex::new(AudioBuffer::new(buffer_size)));

        let config = supported_range.with_sample_rate(actual_rate).into();

        let buffer_clone = buffer.clone();

        // Build stream based on sample format
        let stream = match sample_format {
            SampleFormat::F32 => {
                device.build_output_stream(
                    &config,
                    move |data: &mut [f32], _| {
                        let mut buf = buffer_clone.lock();
                        // Read i16 samples and convert to f32
                        for sample in data.iter_mut() {
                            let mut i16_sample = [0i16; 1];
                            buf.read(&mut i16_sample);
                            *sample = i16_sample[0] as f32 / 32768.0;
                        }
                    },
                    |err| {
                        error!("Audio stream error: {}", err);
                    },
                    None,
                ).context("Failed to create f32 audio stream")?
            }
            SampleFormat::I16 => {
                device.build_output_stream(
                    &config,
                    move |data: &mut [i16], _| {
                        let mut buf = buffer_clone.lock();
                        buf.read(data);
                    },
                    |err| {
                        error!("Audio stream error: {}", err);
                    },
                    None,
                ).context("Failed to create i16 audio stream")?
            }
            _ => {
                // Fallback: try f32 anyway
                device.build_output_stream(
                    &config,
                    move |data: &mut [f32], _| {
                        let mut buf = buffer_clone.lock();
                        for sample in data.iter_mut() {
                            let mut i16_sample = [0i16; 1];
                            buf.read(&mut i16_sample);
                            *sample = i16_sample[0] as f32 / 32768.0;
                        }
                    },
                    |err| {
                        error!("Audio stream error: {}", err);
                    },
                    None,
                ).context("Failed to create audio stream with fallback format")?
            }
        };

        stream.play().context("Failed to start audio playback")?;

        info!("Audio player started successfully");

        Ok(Self {
            sample_rate: actual_rate.0,
            channels: actual_channels as u32,
            buffer,
            _stream: Some(stream),
        })
    }

    /// Push audio samples to the player
    pub fn push_samples(&self, samples: &[i16]) {
        let mut buffer = self.buffer.lock();
        buffer.write(samples);
    }

    /// Get buffer status (for debugging)
    pub fn buffer_status(&self) -> (usize, u64, u64) {
        let buffer = self.buffer.lock();
        (buffer.available(), buffer.total_written, buffer.total_read)
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
