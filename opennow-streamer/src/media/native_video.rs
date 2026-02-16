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
use log::{info, warn};
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
    ///
    /// Note: Only HEVC (H.265) is supported. H.264 streams should use
    /// FFmpeg-based decoders (D3D11VA, DXVA2) instead.
    pub fn new_async(
        codec: VideoCodec,
        shared_frame: Arc<SharedFrame>,
    ) -> Result<(Self, tokio_mpsc::Receiver<NativeDecodeStats>)> {
        // Only HEVC is supported by the native decoder
        if codec != VideoCodec::H265 {
            return Err(anyhow!(
                "Native DXVA decoder only supports HEVC. Use D3D11VA or DXVA2 for H.264."
            ));
        }

        info!("Creating native DXVA HEVC decoder");
        let dxva_codec = DxvaCodec::HEVC;

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
        _codec: DxvaCodec,
        cmd_rx: mpsc::Receiver<NativeDecoderCommand>,
        shared_frame: Arc<SharedFrame>,
        stats_tx: tokio_mpsc::Sender<NativeDecodeStats>,
    ) -> Result<()> {
        thread::spawn(move || {
            // HEVC NAL unit parser
            let mut hevc_parser = HevcParser::new();

            // Decoder will be initialized on first frame when we know dimensions
            let mut decoder: Option<DxvaDecoder> = None;
            let mut current_width = 0u32;
            let mut current_height = 0u32;
            let mut is_hdr = false;

            let mut frames_decoded = 0u64;
            let mut consecutive_failures = 0u32;
            const KEYFRAME_REQUEST_THRESHOLD: u32 = 3; // Lowered from 10 for faster recovery after focus loss

            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    NativeDecoderCommand::DecodeAsync { data, receive_time } => {
                        // Parse HEVC NAL units to extract SPS for dimensions
                        let nals = hevc_parser.find_nal_units(&data);
                        for nal in &nals {
                            let _ = hevc_parser.process_nal(nal);
                        }
                        let (width, height, hdr) =
                            hevc_parser.get_dimensions().unwrap_or((0, 0, false));

                        // Initialize or reconfigure decoder if dimensions changed
                        if width > 0 && height > 0 {
                            if decoder.is_none()
                                || width != current_width
                                || height != current_height
                                || hdr != is_hdr
                            {
                                let config = DxvaDecoderConfig {
                                    codec: DxvaCodec::HEVC,
                                    width,
                                    height,
                                    is_hdr: hdr,
                                    surface_count: 25, // Increased for high bitrate streams
                                    low_latency: true, // Enable low latency for streaming
                                };

                                match DxvaDecoder::new(config) {
                                    Ok(dec) => {
                                        info!(
                                            "Native DXVA HEVC decoder initialized: {}x{} HDR={}",
                                            width, height, hdr
                                        );
                                        decoder = Some(dec);
                                        current_width = width;
                                        current_height = height;
                                        is_hdr = hdr;
                                    }
                                    Err(e) => {
                                        warn!("Failed to create DXVA decoder: {:?}", e);
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
                            // Decode HEVC frame
                            match dec.decode_frame(&data, &mut hevc_parser) {
                                Ok(decoded) => {
                                    frames_decoded += 1;
                                    frame_produced = true;
                                    consecutive_failures = 0;

                                    // Convert to VideoFrame and write to SharedFrame
                                    // Zero-copy: GPU texture passed directly to renderer
                                    let video_frame = Self::convert_decoded_frame(&decoded, is_hdr);
                                    if let Some(frame) = video_frame {
                                        shared_frame.write(frame);
                                    }
                                }
                                Err(e) => {
                                    consecutive_failures += 1;
                                    // Log first few failures and then periodically
                                    if consecutive_failures <= 5 || consecutive_failures % 100 == 0
                                    {
                                        warn!(
                                            "Native HEVC decode failed (failure #{}): {:?}",
                                            consecutive_failures, e
                                        );
                                    }
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
                            codec: DxvaCodec::HEVC,
                            width,
                            height,
                            is_hdr: hdr,
                            surface_count: 25,
                            low_latency: true, // Enable low latency for streaming
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

    /// Convert decoded DXVA frame to VideoFrame
    ///
    /// CRITICAL: We must copy the frame data IMMEDIATELY after decoding, before
    /// the decoder moves on to the next frame. The DXVA decoder uses a texture
    /// array with surface recycling - if we don't copy now, the surface may be
    /// reused for the next decode before the renderer can read it, causing
    /// frame repetition or corruption.
    fn convert_decoded_frame(
        decoded: &super::dxva_decoder::DxvaDecodedFrame,
        is_hdr: bool,
    ) -> Option<VideoFrame> {
        use super::d3d11::D3D11TextureWrapper;

        info!(
            "Converting decoded frame: {}x{}, array_index={}, poc={}",
            decoded.width, decoded.height, decoded.array_index, decoded.poc
        );

        // Create wrapper to access the texture
        let gpu_texture =
            D3D11TextureWrapper::from_texture(decoded.texture.clone(), decoded.array_index);

        // CRITICAL: Copy frame data NOW before decoder reuses the surface
        // This prevents frame repetition caused by surface recycling
        match gpu_texture.lock_and_get_planes() {
            Ok(planes) => {
                info!(
                    "Frame copied: poc={}, y_size={}, uv_size={}, stride={}",
                    decoded.poc,
                    planes.y_plane.len(),
                    planes.uv_plane.len(),
                    planes.y_stride
                );

                // Return VideoFrame with CPU plane data
                // The renderer will upload this to GPU textures
                Some(VideoFrame {
                    frame_id: super::next_frame_id(),
                    width: decoded.width,
                    height: decoded.height,
                    // NV12 format: Y plane + interleaved UV plane
                    y_plane: planes.y_plane,
                    u_plane: planes.uv_plane, // UV interleaved in NV12
                    v_plane: Vec::new(),      // Empty for NV12 (UV is interleaved)
                    y_stride: planes.y_stride,
                    u_stride: planes.uv_stride,
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
                    // No GPU frame - we've copied to CPU planes
                    gpu_frame: None,
                })
            }
            Err(e) => {
                warn!(
                    "Failed to copy decoded frame (poc={}): {:?}",
                    decoded.poc, e
                );
                None
            }
        }
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
