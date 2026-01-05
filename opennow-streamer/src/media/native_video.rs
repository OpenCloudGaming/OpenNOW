//! Native Video Decoder Wrapper
//!
//! This module provides a VideoDecoder-compatible interface for the native
//! D3D11 Video decoder (DXVA2), which bypasses FFmpeg entirely.
//!
//! Benefits:
//! - No MAX_SLICES=32 limitation (FFmpeg hardcoded limit)
//! - Native texture array (RTArray) support like NVIDIA's client
//! - Better compatibility with NVIDIA drivers
//! - Zero-copy output to D3D11 textures

use anyhow::{anyhow, Result};
use log::warn;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use tokio::sync::mpsc as tokio_mpsc;

use super::dxva_decoder::{DxvaCodec, DxvaDecoder, DxvaDecoderConfig};
use super::hevc_parser::HevcParser;
use super::{ColorRange, ColorSpace, PixelFormat, TransferFunction, VideoFrame};
use crate::app::{SharedFrame, VideoCodec};

/// Stats from the native decoder thread
#[derive(Debug, Clone)]
pub struct NativeDecodeStats {
    /// Time from packet receive to decode complete (ms)
    pub decode_time_ms: f32,
    /// Whether a frame was produced
    pub frame_produced: bool,
    /// Whether a keyframe is needed
    pub needs_keyframe: bool,
}

/// Commands sent to the native decoder thread
enum NativeDecoderCommand {
    /// Decode a packet (async mode)
    DecodeAsync {
        data: Vec<u8>,
        receive_time: std::time::Instant,
    },
    /// Update decoder configuration (resolution change)
    Configure {
        width: u32,
        height: u32,
        is_hdr: bool,
    },
    /// Stop the decoder
    Stop,
}

/// Native D3D11 Video Decoder wrapper
///
/// Provides the same interface as VideoDecoder but uses native DXVA2
/// instead of FFmpeg, avoiding the MAX_SLICES limitation.
pub struct NativeVideoDecoder {
    cmd_tx: mpsc::Sender<NativeDecoderCommand>,
    frames_decoded: u64,
    shared_frame: Option<Arc<SharedFrame>>,
}

impl NativeVideoDecoder {
    /// Create a new native video decoder for async mode
    pub fn new_async(
        codec: VideoCodec,
        shared_frame: Arc<SharedFrame>,
    ) -> Result<(Self, tokio_mpsc::Receiver<NativeDecodeStats>)> {
        // Only HEVC is fully supported for now
        let dxva_codec = match codec {
            VideoCodec::H265 => DxvaCodec::HEVC,
            VideoCodec::H264 => {
                warn!("Native DXVA H.264 not yet implemented, using HEVC parser");
                return Err(anyhow!("H.264 not yet supported in native DXVA decoder"));
            }
            VideoCodec::AV1 => {
                warn!("Native DXVA AV1 not yet implemented");
                return Err(anyhow!("AV1 not yet supported in native DXVA decoder"));
            }
        };

        // Create channels for communication
        let (cmd_tx, cmd_rx) = mpsc::channel::<NativeDecoderCommand>();
        let (stats_tx, stats_rx) = tokio_mpsc::channel::<NativeDecodeStats>(64);

        // Spawn decoder thread
        let shared_frame_clone = shared_frame.clone();
        Self::spawn_decoder_thread(dxva_codec, cmd_rx, shared_frame_clone, stats_tx)?;

        let decoder = Self {
            cmd_tx,
            frames_decoded: 0,
            shared_frame: Some(shared_frame),
        };

        Ok((decoder, stats_rx))
    }

    /// Spawn the native decoder thread
    fn spawn_decoder_thread(
        codec: DxvaCodec,
        cmd_rx: mpsc::Receiver<NativeDecoderCommand>,
        shared_frame: Arc<SharedFrame>,
        stats_tx: tokio_mpsc::Sender<NativeDecodeStats>,
    ) -> Result<()> {
        thread::spawn(move || {
            // Parser for HEVC NAL units
            let mut parser = HevcParser::new();

            // Decoder will be initialized on first frame when we know dimensions
            let mut decoder: Option<DxvaDecoder> = None;
            let mut current_width = 0u32;
            let mut current_height = 0u32;
            let mut is_hdr = false;

            let mut frames_decoded = 0u64;
            let mut consecutive_failures = 0u32;
            const KEYFRAME_REQUEST_THRESHOLD: u32 = 10;

            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    NativeDecoderCommand::DecodeAsync { data, receive_time } => {
                        // Parse NAL units to extract SPS for dimensions
                        let nals = parser.find_nal_units(&data);

                        // Process parameter sets first
                        for nal in &nals {
                            let _ = parser.process_nal(nal);
                        }

                        // Check if we have SPS and can determine dimensions
                        let (width, height, hdr) = parser.get_dimensions().unwrap_or((0, 0, false));

                        // Initialize or reconfigure decoder if dimensions changed
                        if width > 0 && height > 0 {
                            if decoder.is_none()
                                || width != current_width
                                || height != current_height
                                || hdr != is_hdr
                            {
                                let config = DxvaDecoderConfig {
                                    codec,
                                    width,
                                    height,
                                    is_hdr: hdr,
                                    surface_count: 20,
                                };

                                match DxvaDecoder::new(config) {
                                    Ok(dec) => {
                                        decoder = Some(dec);
                                        current_width = width;
                                        current_height = height;
                                        is_hdr = hdr;
                                    }
                                    Err(_e) => {
                                        // Send stats with failure
                                        let _ = stats_tx.try_send(NativeDecodeStats {
                                            decode_time_ms: receive_time.elapsed().as_secs_f32()
                                                * 1000.0,
                                            frame_produced: false,
                                            needs_keyframe: true,
                                        });
                                        continue;
                                    }
                                }
                            }
                        }

                        // Decode frame if decoder is ready
                        let mut frame_produced = false;
                        let mut needs_keyframe = false;

                        if let Some(ref mut dec) = decoder {
                            match dec.decode_frame(&data, &mut parser) {
                                Ok(decoded) => {
                                    frames_decoded += 1;
                                    frame_produced = true;
                                    consecutive_failures = 0;

                                    // Convert to VideoFrame and write to SharedFrame
                                    // For now, we need to copy from D3D11 texture to CPU
                                    // TODO: Implement zero-copy path with D3D11 texture sharing
                                    let video_frame = Self::convert_decoded_frame(&decoded, is_hdr);
                                    if let Some(frame) = video_frame {
                                        shared_frame.write(frame);
                                    }
                                }
                                Err(_e) => {
                                    consecutive_failures += 1;
                                    if consecutive_failures >= KEYFRAME_REQUEST_THRESHOLD {
                                        needs_keyframe = true;
                                    }
                                }
                            }
                        } else {
                            // Decoder not ready yet (waiting for SPS)
                            consecutive_failures += 1;
                        }

                        // Send stats
                        let _ = stats_tx.try_send(NativeDecodeStats {
                            decode_time_ms: receive_time.elapsed().as_secs_f32() * 1000.0,
                            frame_produced,
                            needs_keyframe,
                        });
                    }

                    NativeDecoderCommand::Configure {
                        width,
                        height,
                        is_hdr: hdr,
                    } => {
                        let config = DxvaDecoderConfig {
                            codec,
                            width,
                            height,
                            is_hdr: hdr,
                            surface_count: 20,
                        };

                        if let Ok(dec) = DxvaDecoder::new(config) {
                            decoder = Some(dec);
                            current_width = width;
                            current_height = height;
                            is_hdr = hdr;
                        }
                    }

                    NativeDecoderCommand::Stop => {
                        break;
                    }
                }
            }
        });

        Ok(())
    }

    /// Convert decoded DXVA frame to VideoFrame (zero-copy)
    ///
    /// The GPU texture is wrapped and passed directly to the renderer.
    /// No CPU copy is performed - the texture stays on GPU.
    fn convert_decoded_frame(
        decoded: &super::dxva_decoder::DxvaDecodedFrame,
        is_hdr: bool,
    ) -> Option<VideoFrame> {
        use super::d3d11::D3D11TextureWrapper;
        use std::sync::Arc;

        // Create a D3D11TextureWrapper for zero-copy GPU rendering
        let gpu_texture =
            D3D11TextureWrapper::from_texture(decoded.texture.clone(), decoded.array_index);

        // Zero-copy: CPU planes are empty, GPU texture is used directly
        Some(VideoFrame {
            width: decoded.width,
            height: decoded.height,
            // CPU planes are empty - GPU texture is used for rendering
            y_plane: Vec::new(),
            u_plane: Vec::new(),
            v_plane: Vec::new(),
            y_stride: 0,
            u_stride: 0,
            v_stride: 0,
            timestamp_us: 0,
            format: if is_hdr {
                PixelFormat::P010
            } else {
                PixelFormat::NV12
            },
            color_range: ColorRange::Limited,
            color_space: if is_hdr {
                ColorSpace::BT2020
            } else {
                ColorSpace::BT709
            },
            transfer_function: if is_hdr {
                TransferFunction::PQ
            } else {
                TransferFunction::SDR
            },
            // Zero-copy: GPU texture passed directly to renderer
            gpu_frame: Some(Arc::new(gpu_texture)),
        })
    }

    /// Send a packet for async decoding
    pub fn decode_async(&self, data: Vec<u8>, receive_time: std::time::Instant) {
        let _ = self
            .cmd_tx
            .send(NativeDecoderCommand::DecodeAsync { data, receive_time });
    }

    /// Get frames decoded count
    pub fn frames_decoded(&self) -> u64 {
        self.frames_decoded
    }

    /// Check if using hardware acceleration (always true for native DXVA)
    pub fn is_hw_accel(&self) -> bool {
        true
    }
}

impl Drop for NativeVideoDecoder {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(NativeDecoderCommand::Stop);
    }
}
