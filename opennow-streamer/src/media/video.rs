//! Video Decoder
//!
//! Hardware-accelerated H.264/H.265/AV1 decoding using FFmpeg.
//! 
//! This module provides both blocking and non-blocking decode modes:
//! - Blocking: `decode()` - waits for result (legacy, causes latency)
//! - Non-blocking: `decode_async()` - fire-and-forget, writes to SharedFrame

use anyhow::{Result, anyhow};
use log::{info, debug, warn};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use tokio::sync::mpsc as tokio_mpsc;

#[cfg(target_os = "windows")]
use std::path::Path;

use super::{VideoFrame, PixelFormat, ColorRange, ColorSpace};
use crate::app::{VideoCodec, SharedFrame};

extern crate ffmpeg_next as ffmpeg;

use ffmpeg::codec::{decoder, context::Context as CodecContext};
use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::{context::Context as ScalerContext, flag::Flags as ScalerFlags};
use ffmpeg::util::frame::video::Video as FfmpegFrame;
use ffmpeg::Packet;

/// GPU Vendor for decoder optimization
#[derive(Debug, PartialEq, Clone, Copy)]
enum GpuVendor {
    Nvidia,
    Intel,
    Amd,
    Apple,
    Other,
    Unknown,
}

/// Detect the primary GPU vendor using wgpu, prioritizing discrete GPUs
fn detect_gpu_vendor() -> GpuVendor {
    // blocked_on because we are in a sync context (VideoDecoder::new)
    // but wgpu adapter request is async
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        
        // Enumerate all available adapters
        let adapters = instance.enumerate_adapters(wgpu::Backends::all());
        
        let mut best_score = -1;
        let mut best_vendor = GpuVendor::Unknown;
        
        info!("Available GPU adapters:");
        
        for adapter in adapters {
            let info = adapter.get_info();
            let name = info.name.to_lowercase();
            let mut score = 0;
            let mut vendor = GpuVendor::Other;
            
            // Identify vendor
            if name.contains("nvidia") || name.contains("geforce") || name.contains("quadro") {
                vendor = GpuVendor::Nvidia;
                score += 100;
            } else if name.contains("amd") || name.contains("adeon") || name.contains("ryzen") {
                vendor = GpuVendor::Amd;
                score += 80;
            } else if name.contains("intel") || name.contains("uhd") || name.contains("iris") || name.contains("arc") {
                vendor = GpuVendor::Intel;
                score += 50;
            } else if name.contains("apple") || name.contains("m1") || name.contains("m2") || name.contains("m3") {
                vendor = GpuVendor::Apple;
                score += 90; // Apple Silicon is high perf
            }
            
            // Prioritize discrete GPUs
            match info.device_type {
                wgpu::DeviceType::DiscreteGpu => {
                    score += 50;
                }
                wgpu::DeviceType::IntegratedGpu => {
                    score += 10;
                }
                _ => {}
            }
            
            info!("  - {} ({:?}, Vendor: {:?}, Score: {})", info.name, info.device_type, vendor, score);
            
            if score > best_score {
                best_score = score;
                best_vendor = vendor;
            }
        }
        
        if best_vendor != GpuVendor::Unknown {
            info!("Selected best GPU vendor: {:?}", best_vendor);
            best_vendor
        } else {
            // Fallback to default request if enumeration fails
             warn!("Adapter enumeration yielded no results, trying default request");
             let adapter = instance
                .request_adapter(&wgpu::RequestAdapterOptions {
                    power_preference: wgpu::PowerPreference::HighPerformance,
                    compatible_surface: None,
                    force_fallback_adapter: false,
                })
                .await;

            if let Some(adapter) = adapter {
                let info = adapter.get_info();
                let name = info.name.to_lowercase();
                
                 if name.contains("nvidia") { GpuVendor::Nvidia }
                 else if name.contains("intel") { GpuVendor::Intel }
                 else if name.contains("amd") { GpuVendor::Amd }
                 else if name.contains("apple") { GpuVendor::Apple }
                 else { GpuVendor::Other }
            } else {
                GpuVendor::Unknown
            }
        }
    })
}

/// Check if Intel QSV runtime is available on the system
/// Returns true if the required DLLs are found
#[cfg(target_os = "windows")]
fn is_qsv_runtime_available() -> bool {
    use std::env;

    // Intel Media SDK / oneVPL runtime DLLs to look for
    let runtime_dlls = [
        "libmfx-gen.dll",     // Intel oneVPL runtime (11th gen+, newer)
        "libmfxhw64.dll",     // Intel Media SDK runtime (older)
        "mfxhw64.dll",        // Alternative naming
        "libmfx64.dll",       // Another variant
    ];

    // Check common paths where Intel runtimes are installed
    let search_paths: Vec<std::path::PathBuf> = vec![
        // System32 (most common for driver-installed runtimes)
        env::var("SystemRoot")
            .map(|s| Path::new(&s).join("System32"))
            .unwrap_or_default(),
        // SysWOW64 for 32-bit
        env::var("SystemRoot")
            .map(|s| Path::new(&s).join("SysWOW64"))
            .unwrap_or_default(),
        // Intel Media SDK default install
        Path::new("C:\\Program Files\\Intel\\Media SDK 2023 R1\\Software Development Kit\\bin\\x64").to_path_buf(),
        Path::new("C:\\Program Files\\Intel\\Media SDK\\bin\\x64").to_path_buf(),
        // oneVPL default install
        Path::new("C:\\Program Files (x86)\\Intel\\oneAPI\\vpl\\latest\\bin").to_path_buf(),
        // Application directory (for bundled DLLs)
        env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_default(),
    ];

    for dll in &runtime_dlls {
        for path in &search_paths {
            let full_path = path.join(dll);
            if full_path.exists() {
                info!("Found Intel QSV runtime: {}", full_path.display());
                return true;
            }
        }
    }

    // Also try loading via Windows DLL search path
    // If Intel drivers are installed, the DLLs should be in PATH
    if let Ok(output) = std::process::Command::new("where")
        .arg("libmfx-gen.dll")
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout);
            info!("Found Intel QSV runtime via PATH: {}", path.trim());
            return true;
        }
    }

    debug!("Intel QSV runtime not found - QSV decoder will be skipped");
    false
}

#[cfg(not(target_os = "windows"))]
fn is_qsv_runtime_available() -> bool {
    // On Linux, check for libmfx.so or libvpl.so
    use std::process::Command;

    // QSV is only supported on Intel architectures
    if !cfg!(target_arch = "x86") && !cfg!(target_arch = "x86_64") {
        return false;
    }

    if let Ok(output) = Command::new("ldconfig").arg("-p").output() {
        let libs = String::from_utf8_lossy(&output.stdout);
        if libs.contains("libmfx") || libs.contains("libvpl") {
            info!("Found Intel QSV runtime on Linux");
            return true;
        }
    }

    debug!("Intel QSV runtime not found on Linux");
    false
}

/// Cached QSV availability check (only check once at startup)
static QSV_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

fn check_qsv_available() -> bool {
    *QSV_AVAILABLE.get_or_init(|| {
        let available = is_qsv_runtime_available();
        if available {
            info!("Intel QuickSync Video (QSV) runtime detected - QSV decoding enabled");
        } else {
            info!("Intel QSV runtime not detected - QSV decoding disabled (install Intel GPU drivers for QSV support)");
        }
        available
    })
}

/// Cached AV1 hardware support check
static AV1_HW_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();

/// Check if AV1 hardware decoding is supported on this system
/// Returns true if NVIDIA CUVID (RTX 30+) or Intel QSV (11th gen+) is available
pub fn is_av1_hardware_supported() -> bool {
    *AV1_HW_AVAILABLE.get_or_init(|| {
        // Initialize FFmpeg if not already done
        let _ = ffmpeg::init();

        // Check for NVIDIA CUVID AV1 decoder
        let has_nvidia = ffmpeg::codec::decoder::find_by_name("av1_cuvid").is_some();

        // Check for Intel QSV AV1 decoder (requires QSV runtime)
        let has_intel = check_qsv_available() &&
            ffmpeg::codec::decoder::find_by_name("av1_qsv").is_some();

        // Check for AMD VAAPI (Linux only)
        #[cfg(target_os = "linux")]
        let has_amd = ffmpeg::codec::decoder::find_by_name("av1_vaapi").is_some();
        #[cfg(not(target_os = "linux"))]
        let has_amd = false;

        // Check for VideoToolbox (macOS)
        #[cfg(target_os = "macos")]
        let has_videotoolbox = {
            // VideoToolbox AV1 support was added in macOS 13 Ventura on Apple Silicon
            // Check if the decoder exists in FFmpeg build
            ffmpeg::codec::decoder::find_by_name("av1").map_or(false, |codec| {
                // The standard av1 decoder with VideoToolbox hwaccel
                // This is a heuristic - actual support depends on macOS version and hardware
                true
            })
        };
        #[cfg(not(target_os = "macos"))]
        let has_videotoolbox = false;

        let supported = has_nvidia || has_intel || has_amd || has_videotoolbox;

        if supported {
            let mut sources = Vec::new();
            if has_nvidia { sources.push("NVIDIA NVDEC"); }
            if has_intel { sources.push("Intel QSV"); }
            if has_amd { sources.push("AMD VAAPI"); }
            if has_videotoolbox { sources.push("Apple VideoToolbox"); }
            info!("AV1 hardware decoding available via: {}", sources.join(", "));
        } else {
            info!("AV1 hardware decoding NOT available - will use software decode (slow)");
        }

        supported
    })
}

/// Commands sent to the decoder thread
enum DecoderCommand {
    /// Decode a packet and return result via channel (blocking mode)
    Decode(Vec<u8>),
    /// Decode a packet and write directly to SharedFrame (non-blocking mode)
    DecodeAsync {
        data: Vec<u8>,
        receive_time: std::time::Instant,
    },
    Stop,
}

/// Stats from the decoder thread
#[derive(Debug, Clone)]
pub struct DecodeStats {
    /// Time from packet receive to decode complete (ms)
    pub decode_time_ms: f32,
    /// Whether a frame was produced
    pub frame_produced: bool,
    /// Whether a keyframe is needed (too many consecutive decode failures)
    pub needs_keyframe: bool,
}

/// Video decoder using FFmpeg with hardware acceleration
/// Uses a dedicated thread for decoding since FFmpeg types are not Send
pub struct VideoDecoder {
    cmd_tx: mpsc::Sender<DecoderCommand>,
    frame_rx: mpsc::Receiver<Option<VideoFrame>>,
    /// Stats receiver for non-blocking mode
    stats_rx: Option<tokio_mpsc::Receiver<DecodeStats>>,
    hw_accel: bool,
    frames_decoded: u64,
    /// SharedFrame for non-blocking writes (set via set_shared_frame)
    shared_frame: Option<Arc<SharedFrame>>,
}

impl VideoDecoder {
    /// Create a new video decoder with hardware acceleration
    pub fn new(codec: VideoCodec) -> Result<Self> {
        // Initialize FFmpeg
        ffmpeg::init().map_err(|e| anyhow!("Failed to initialize FFmpeg: {:?}", e))?;

        // Suppress FFmpeg's "no frame" info messages (EAGAIN is normal for H.264)
        unsafe {
            ffmpeg::ffi::av_log_set_level(ffmpeg::ffi::AV_LOG_ERROR as i32);
        }

        info!("Creating FFmpeg video decoder for {:?}", codec);

        // Find the decoder
        let decoder_id = match codec {
            VideoCodec::H264 => ffmpeg::codec::Id::H264,
            VideoCodec::H265 => ffmpeg::codec::Id::HEVC,
            VideoCodec::AV1 => ffmpeg::codec::Id::AV1,
        };

        // Create channels for communication with decoder thread
        let (cmd_tx, cmd_rx) = mpsc::channel::<DecoderCommand>();
        let (frame_tx, frame_rx) = mpsc::channel::<Option<VideoFrame>>();

        // Create decoder in a separate thread (FFmpeg types are not Send)
        let hw_accel = Self::spawn_decoder_thread(decoder_id, cmd_rx, frame_tx, None, None)?;

        if hw_accel {
            info!("Using hardware-accelerated decoder");
        } else {
            info!("Using software decoder (hardware acceleration not available)");
        }

        Ok(Self {
            cmd_tx,
            frame_rx,
            stats_rx: None,
            hw_accel,
            frames_decoded: 0,
            shared_frame: None,
        })
    }

    /// Create a new video decoder configured for non-blocking async mode
    /// Decoded frames are written directly to the SharedFrame
    pub fn new_async(codec: VideoCodec, shared_frame: Arc<SharedFrame>) -> Result<(Self, tokio_mpsc::Receiver<DecodeStats>)> {
        // Initialize FFmpeg
        ffmpeg::init().map_err(|e| anyhow!("Failed to initialize FFmpeg: {:?}", e))?;

        // Suppress FFmpeg's "no frame" info messages (EAGAIN is normal for H.264)
        unsafe {
            ffmpeg::ffi::av_log_set_level(ffmpeg::ffi::AV_LOG_ERROR as i32);
        }

        info!("Creating FFmpeg video decoder (async mode) for {:?}", codec);

        // Find the decoder
        let decoder_id = match codec {
            VideoCodec::H264 => ffmpeg::codec::Id::H264,
            VideoCodec::H265 => ffmpeg::codec::Id::HEVC,
            VideoCodec::AV1 => ffmpeg::codec::Id::AV1,
        };

        // Create channels for communication with decoder thread
        let (cmd_tx, cmd_rx) = mpsc::channel::<DecoderCommand>();
        let (frame_tx, frame_rx) = mpsc::channel::<Option<VideoFrame>>();

        // Stats channel for async mode (non-blocking stats updates)
        let (stats_tx, stats_rx) = tokio_mpsc::channel::<DecodeStats>(64);

        // Create decoder in a separate thread with SharedFrame
        let hw_accel = Self::spawn_decoder_thread(
            decoder_id,
            cmd_rx,
            frame_tx,
            Some(shared_frame.clone()),
            Some(stats_tx),
        )?;

        if hw_accel {
            info!("Using hardware-accelerated decoder (async mode)");
        } else {
            info!("Using software decoder (async mode)");
        }

        let decoder = Self {
            cmd_tx,
            frame_rx,
            stats_rx: None, // Stats come via the returned receiver
            hw_accel,
            frames_decoded: 0,
            shared_frame: Some(shared_frame),
        };

        Ok((decoder, stats_rx))
    }

    /// Spawn a dedicated decoder thread
    fn spawn_decoder_thread(
        codec_id: ffmpeg::codec::Id,
        cmd_rx: mpsc::Receiver<DecoderCommand>,
        frame_tx: mpsc::Sender<Option<VideoFrame>>,
        shared_frame: Option<Arc<SharedFrame>>,
        stats_tx: Option<tokio_mpsc::Sender<DecodeStats>>,
    ) -> Result<bool> {
        // Create decoder synchronously to report hw_accel status
        info!("Creating decoder for codec {:?}...", codec_id);
        let (decoder, hw_accel) = Self::create_decoder(codec_id)?;
        info!("Decoder created, hw_accel={}", hw_accel);

        // Spawn thread to handle decoding
        thread::spawn(move || {
            info!("Decoder thread started for {:?}", codec_id);
            let mut decoder = decoder;
            let mut scaler: Option<ScalerContext> = None;
            let mut width = 0u32;
            let mut height = 0u32;
            let mut frames_decoded = 0u64;
            let mut consecutive_failures = 0u32;
            let mut packets_received = 0u64;
            const KEYFRAME_REQUEST_THRESHOLD: u32 = 10; // Request keyframe after 10 consecutive failures (was 30)
            const FRAMES_TO_SKIP: u64 = 3; // Skip first N frames to let decoder settle with reference frames

            while let Ok(cmd) = cmd_rx.recv() {
                match cmd {
                    DecoderCommand::Decode(data) => {
                        // Blocking mode - send result back via channel
                        let result = Self::decode_frame(
                            &mut decoder,
                            &mut scaler,
                            &mut width,
                            &mut height,
                            &mut frames_decoded,
                            &data,
                            codec_id,
                        );
                        let _ = frame_tx.send(result);
                    }
                    DecoderCommand::DecodeAsync { data, receive_time } => {
                        packets_received += 1;

                        // Non-blocking mode - write directly to SharedFrame
                        let result = Self::decode_frame(
                            &mut decoder,
                            &mut scaler,
                            &mut width,
                            &mut height,
                            &mut frames_decoded,
                            &data,
                            codec_id,
                        );

                        let decode_time_ms = receive_time.elapsed().as_secs_f32() * 1000.0;
                        let frame_produced = result.is_some();

                        // Track consecutive decode failures for PLI request
                        // Note: EAGAIN (no frame) is normal for H.264 - decoder buffers B-frames
                        let needs_keyframe = if frame_produced {
                            // Only log recovery for significant failures (>5), not normal buffering
                            if consecutive_failures > 5 {
                                info!("Decoder: recovered after {} packets without output", consecutive_failures);
                            }
                            consecutive_failures = 0;
                            false
                        } else {
                            consecutive_failures += 1;

                            // Only log at higher thresholds - low counts are normal H.264 buffering
                            if consecutive_failures == 30 {
                                debug!("Decoder: {} packets without frame (packets: {}, decoded: {})",
                                    consecutive_failures, packets_received, frames_decoded);
                            }

                            if consecutive_failures == KEYFRAME_REQUEST_THRESHOLD {
                                warn!("Decoder: {} consecutive frames without output - requesting keyframe (packets: {}, decoded: {})",
                                    consecutive_failures, packets_received, frames_decoded);
                                true
                            } else if consecutive_failures > KEYFRAME_REQUEST_THRESHOLD && consecutive_failures % 20 == 0 {
                                // Keep requesting every 20 frames if still failing (~166ms at 120fps)
                                warn!("Decoder: still failing after {} frames - requesting keyframe again", consecutive_failures);
                                true
                            } else {
                                false
                            }
                        };

                        // Write frame directly to SharedFrame (zero-copy handoff)
                        // Skip first few frames to let decoder settle with proper reference frames
                        // This prevents green/corrupted frames during stream startup
                        if let Some(frame) = result {
                            if frames_decoded > FRAMES_TO_SKIP {
                                if let Some(ref sf) = shared_frame {
                                    sf.write(frame);
                                }
                            } else {
                                debug!("Skipping frame {} (waiting for decoder to settle)", frames_decoded);
                            }
                        }

                        // Send stats update (non-blocking)
                        if let Some(ref tx) = stats_tx {
                            let _ = tx.try_send(DecodeStats {
                                decode_time_ms,
                                frame_produced,
                                needs_keyframe,
                            });
                        }
                    }
                    DecoderCommand::Stop => break,
                }
            }
        });

        Ok(hw_accel)
    }

    /// Create decoder, trying hardware acceleration first
    fn create_decoder(codec_id: ffmpeg::codec::Id) -> Result<(decoder::Video, bool)> {
        // On macOS, try VideoToolbox hardware acceleration
        #[cfg(target_os = "macos")]
        {
            info!("macOS detected - attempting VideoToolbox hardware acceleration");

            // Try to set up VideoToolbox hwaccel using FFmpeg's device API
            unsafe {
                use ffmpeg::ffi::*;
                use std::ptr;

                // Find the standard decoder
                let codec = ffmpeg::codec::decoder::find(codec_id)
                    .ok_or_else(|| anyhow!("Decoder not found for {:?}", codec_id))?;

                let mut ctx = CodecContext::new_with_codec(codec);

                // Get raw pointer to AVCodecContext
                let raw_ctx = ctx.as_mut_ptr();

                // Create VideoToolbox hardware device context
                let mut hw_device_ctx: *mut AVBufferRef = ptr::null_mut();
                let ret = av_hwdevice_ctx_create(
                    &mut hw_device_ctx,
                    AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
                    ptr::null(),
                    ptr::null_mut(),
                    0,
                );

                if ret >= 0 && !hw_device_ctx.is_null() {
                    // Attach hardware device context to codec context
                    (*raw_ctx).hw_device_ctx = av_buffer_ref(hw_device_ctx);

                    // Enable multi-threading
                    (*raw_ctx).thread_count = 4;

                    match ctx.decoder().video() {
                        Ok(decoder) => {
                            info!("VideoToolbox hardware decoder created successfully");
                            // Don't free hw_device_ctx - it's now owned by the codec context
                            return Ok((decoder, true));
                        }
                        Err(e) => {
                            warn!("Failed to open VideoToolbox decoder: {:?}", e);
                            av_buffer_unref(&mut hw_device_ctx);
                        }
                    }
                } else {
                    warn!("Failed to create VideoToolbox device context (error {})", ret);
                }
            }

            // Fall back to software decoder on macOS
            info!("Falling back to software decoder on macOS");
            let codec = ffmpeg::codec::decoder::find(codec_id)
                .ok_or_else(|| anyhow!("Decoder not found for {:?}", codec_id))?;

            let mut ctx = CodecContext::new_with_codec(codec);
            ctx.set_threading(ffmpeg::codec::threading::Config::count(4));

            let decoder = ctx.decoder().video()?;
            return Ok((decoder, false));
        }

        // Check if Intel QSV runtime is available (cached, only checks once)
        #[cfg(not(target_os = "macos"))]
        let qsv_available = check_qsv_available();

        // Detect GPU vendor to prioritize correct decoder
        #[cfg(not(target_os = "macos"))]
        let gpu_vendor = detect_gpu_vendor();

        // Try hardware decoders in order of preference
        // Platform-specific hardware decoders:
        // - Windows: CUVID (NVIDIA), QSV (Intel), D3D11VA, DXVA2
        // - Linux: CUVID, VAAPI, QSV
        #[cfg(not(target_os = "macos"))]
        let hw_decoder_names: Vec<&str> = match codec_id {
            ffmpeg::codec::Id::H264 => {
                #[cfg(target_os = "windows")]
                {
                    // Default priority: D3D11 (safe/modern)
                    let mut decoders = Vec::new();
                    
                    // Prioritize based on vendor
                    match gpu_vendor {
                        GpuVendor::Nvidia => decoders.push("h264_cuvid"),
                        GpuVendor::Intel if qsv_available => decoders.push("h264_qsv"),
                        _ => {}
                    }

                    // Add standard APIs
                    decoders.push("h264_d3d11va");
                    
                    // Add remaining helpers as fallback
                    if gpu_vendor != GpuVendor::Nvidia { decoders.push("h264_cuvid"); } // Try CUDA anyway just in case
                    if gpu_vendor != GpuVendor::Intel && qsv_available { decoders.push("h264_qsv"); }
                    
                    decoders.push("h264_dxva2");
                    decoders
                }
                #[cfg(target_os = "linux")]
                {
                    let mut decoders = Vec::new();
                     match gpu_vendor {
                        GpuVendor::Nvidia => decoders.push("h264_cuvid"),
                        GpuVendor::Intel if qsv_available => decoders.push("h264_qsv"),
                        GpuVendor::Amd => decoders.push("h264_vaapi"),
                        _ => {}
                    }
                    
                    // Fallbacks
                    if !decoders.contains(&"h264_cuvid") { decoders.push("h264_cuvid"); }
                    if !decoders.contains(&"h264_vaapi") { decoders.push("h264_vaapi"); }
                    if !decoders.contains(&"h264_qsv") && qsv_available { decoders.push("h264_qsv"); }
                    
                    decoders
                }
            }
            ffmpeg::codec::Id::HEVC => {
                #[cfg(target_os = "windows")]
                {
                    let mut decoders = Vec::new();
                    match gpu_vendor {
                        GpuVendor::Nvidia => decoders.push("hevc_cuvid"),
                        GpuVendor::Intel if qsv_available => decoders.push("hevc_qsv"),
                        _ => {}
                    }
                    decoders.push("hevc_d3d11va");
                    
                    if gpu_vendor != GpuVendor::Nvidia { decoders.push("hevc_cuvid"); }
                    if gpu_vendor != GpuVendor::Intel && qsv_available { decoders.push("hevc_qsv"); }
                    
                    decoders.push("hevc_dxva2");
                    decoders
                }
                #[cfg(target_os = "linux")]
                {
                    let mut decoders = Vec::new();
                    match gpu_vendor {
                        GpuVendor::Nvidia => decoders.push("hevc_cuvid"),
                        GpuVendor::Intel if qsv_available => decoders.push("hevc_qsv"),
                        GpuVendor::Amd => decoders.push("hevc_vaapi"),
                        _ => {}
                    }
                    
                    if !decoders.contains(&"hevc_cuvid") { decoders.push("hevc_cuvid"); }
                    if !decoders.contains(&"hevc_vaapi") { decoders.push("hevc_vaapi"); }
                    if !decoders.contains(&"hevc_qsv") && qsv_available { decoders.push("hevc_qsv"); }
                    
                    decoders
                }
            }
            ffmpeg::codec::Id::AV1 => {
                #[cfg(target_os = "windows")]
                {
                    let mut decoders = Vec::new();
                    match gpu_vendor {
                        GpuVendor::Nvidia => decoders.push("av1_cuvid"),
                        GpuVendor::Intel if qsv_available => decoders.push("av1_qsv"),
                        _ => {}
                    }
                    // AV1 D3D11 is often "av1_d3d11va" or managed automatically, but FFmpeg naming varies. 
                    // Usually av1_cuvid / av1_qsv are the explicit ones.
                    
                    if gpu_vendor != GpuVendor::Nvidia { decoders.push("av1_cuvid"); }
                    if gpu_vendor != GpuVendor::Intel && qsv_available { decoders.push("av1_qsv"); }
                    
                    decoders
                }
                #[cfg(target_os = "linux")]
                {
                    let mut decoders = Vec::new();
                    match gpu_vendor {
                        GpuVendor::Nvidia => decoders.push("av1_cuvid"),
                        GpuVendor::Intel if qsv_available => decoders.push("av1_qsv"),
                        GpuVendor::Amd => decoders.push("av1_vaapi"),
                        _ => {}
                    }
                    
                     if !decoders.contains(&"av1_cuvid") { decoders.push("av1_cuvid"); }
                     if !decoders.contains(&"av1_vaapi") { decoders.push("av1_vaapi"); }
                     if !decoders.contains(&"av1_qsv") && qsv_available { decoders.push("av1_qsv"); }
                     
                    decoders
                }
            }
            _ => vec![],
        };

        // Try hardware decoders (Windows/Linux)
        #[cfg(not(target_os = "macos"))]
        {
            info!("Attempting hardware decoders for {:?}: {:?}", codec_id, hw_decoder_names);
            for hw_name in &hw_decoder_names {
                if let Some(hw_codec) = ffmpeg::codec::decoder::find_by_name(hw_name) {
                    info!("Found hardware decoder: {}, attempting to open...", hw_name);
                    // new_with_codec returns Context directly, not Result
                    let mut ctx = CodecContext::new_with_codec(hw_codec);
                    ctx.set_threading(ffmpeg::codec::threading::Config::count(4));

                    match ctx.decoder().video() {
                        Ok(dec) => {
                            info!("Successfully created hardware decoder: {}", hw_name);
                            return Ok((dec, true));
                        }
                        Err(e) => {
                            warn!("Failed to open hardware decoder {}: {:?}", hw_name, e);
                        }
                    }
                } else {
                    debug!("Hardware decoder not found: {}", hw_name);
                }
            }
        }

        // Fall back to software decoder
        info!("Using software decoder for {:?}", codec_id);
        let codec = ffmpeg::codec::decoder::find(codec_id)
            .ok_or_else(|| anyhow!("Decoder not found for {:?}", codec_id))?;
        info!("Found software decoder: {:?}", codec.name());

        let mut ctx = CodecContext::new_with_codec(codec);
        ctx.set_threading(ffmpeg::codec::threading::Config::count(4));

        let decoder = ctx.decoder().video()?;
        info!("Software decoder opened successfully");
        Ok((decoder, false))
    }

    /// Check if a pixel format is a hardware format
    fn is_hw_pixel_format(format: Pixel) -> bool {
        matches!(
            format,
            Pixel::VIDEOTOOLBOX
                | Pixel::CUDA
                | Pixel::VDPAU
                | Pixel::QSV
                | Pixel::D3D11
                | Pixel::DXVA2_VLD
                | Pixel::D3D11VA_VLD
                | Pixel::VULKAN
        )
    }

    /// Transfer hardware frame to system memory if needed
    fn transfer_hw_frame_if_needed(frame: &FfmpegFrame) -> Option<FfmpegFrame> {
        let format = frame.format();

        if !Self::is_hw_pixel_format(format) {
            // Not a hardware frame, no transfer needed
            return None;
        }

        debug!("Transferring hardware frame (format: {:?}) to system memory", format);

        unsafe {
            use ffmpeg::ffi::*;

            // Create a new frame for the software copy
            let sw_frame_ptr = av_frame_alloc();
            if sw_frame_ptr.is_null() {
                warn!("Failed to allocate software frame");
                return None;
            }

            // Transfer data from hardware frame to software frame
            let ret = av_hwframe_transfer_data(sw_frame_ptr, frame.as_ptr(), 0);
            if ret < 0 {
                warn!("Failed to transfer hardware frame to software (error {})", ret);
                av_frame_free(&mut (sw_frame_ptr as *mut _));
                return None;
            }

            // Copy frame properties
            (*sw_frame_ptr).width = frame.width() as i32;
            (*sw_frame_ptr).height = frame.height() as i32;

            // Wrap in FFmpeg frame type
            // Note: This creates an owned frame that will be freed when dropped
            Some(FfmpegFrame::wrap(sw_frame_ptr))
        }
    }

    /// Calculate 256-byte aligned stride for GPU compatibility (wgpu/DX12 requirement)
    fn get_aligned_stride(width: u32) -> u32 {
        (width + 255) & !255
    }

    /// Decode a single frame (called in decoder thread)
    fn decode_frame(
        decoder: &mut decoder::Video,
        scaler: &mut Option<ScalerContext>,
        width: &mut u32,
        height: &mut u32,
        frames_decoded: &mut u64,
        data: &[u8],
        codec_id: ffmpeg::codec::Id,
    ) -> Option<VideoFrame> {
        // AV1 uses OBUs directly, no start codes needed
        // H.264/H.265 need Annex B start codes (0x00 0x00 0x00 0x01)
        let data = if codec_id == ffmpeg::codec::Id::AV1 {
            // AV1 - use data as-is (OBU format)
            data.to_vec()
        } else if data.len() >= 4 && data[0..4] == [0, 0, 0, 1] {
            data.to_vec()
        } else if data.len() >= 3 && data[0..3] == [0, 0, 1] {
            data.to_vec()
        } else {
            // Add start code for H.264/H.265
            let mut with_start = vec![0, 0, 0, 1];
            with_start.extend_from_slice(data);
            with_start
        };

        // Create packet
        let mut packet = Packet::new(data.len());
        if let Some(pkt_data) = packet.data_mut() {
            pkt_data.copy_from_slice(&data);
        } else {
            warn!("Failed to allocate packet data");
            return None;
        }

        // Send packet to decoder
        if let Err(e) = decoder.send_packet(&packet) {
            // EAGAIN means we need to receive frames first
            match e {
                ffmpeg::Error::Other { errno } if errno == libc::EAGAIN => {}
                _ => warn!("Send packet error: {:?}", e),
            }
        }

        // Try to receive decoded frame
        let mut frame = FfmpegFrame::empty();
        match decoder.receive_frame(&mut frame) {
            Ok(_) => {
                *frames_decoded += 1;

                let w = frame.width();
                let h = frame.height();
                let format = frame.format();

                // Check if this is a hardware frame (e.g., VideoToolbox, CUDA, etc.)
                // Hardware frames need to be transferred to system memory
                let sw_frame = Self::transfer_hw_frame_if_needed(&frame);
                let frame_to_use = sw_frame.as_ref().unwrap_or(&frame);
                let actual_format = frame_to_use.format();

                // Extract color metadata
                let color_range = match frame_to_use.color_range() {
                    ffmpeg::util::color::range::Range::JPEG => ColorRange::Full,
                    ffmpeg::util::color::range::Range::MPEG => ColorRange::Limited,
                    _ => ColorRange::Limited, // Default to limited if unspecified (safest for video)
                };

                let color_space = match frame_to_use.color_space() {
                    ffmpeg::util::color::space::Space::BT709 => ColorSpace::BT709,
                    ffmpeg::util::color::space::Space::BT470BG => ColorSpace::BT601,
                    ffmpeg::util::color::space::Space::SMPTE170M => ColorSpace::BT601,
                    ffmpeg::util::color::space::Space::BT2020NCL => ColorSpace::BT2020,
                    _ => ColorSpace::BT709, // Default to BT.709 for HD content
                };

                if *frames_decoded == 1 {
                    info!("First decoded frame: {}x{}, format: {:?} (hw: {:?}), range: {:?}, space: {:?}", 
                        w, h, actual_format, format, color_range, color_space);
                }

                // Check if frame is NV12 - skip CPU scaler and pass directly to GPU
                // NV12 has Y plane (full res) and UV plane (half res, interleaved)
                // GPU shader will handle color conversion - much faster than CPU scaler
                if actual_format == Pixel::NV12 {
                    *width = w;
                    *height = h;

                    let y_stride = frame_to_use.stride(0) as u32;
                    let uv_stride = frame_to_use.stride(1) as u32;
                    let uv_height = h / 2;

                    // GPU texture upload requires 256-byte aligned rows (wgpu restriction)
                    let aligned_y_stride = Self::get_aligned_stride(w);
                    let aligned_uv_stride = Self::get_aligned_stride(w);

                    let y_data = frame_to_use.data(0);
                    let uv_data = frame_to_use.data(1);

                    // Copy Y plane with alignment
                    let mut y_plane = vec![0u8; (aligned_y_stride * h) as usize];
                    for row in 0..h {
                        let src_start = (row * y_stride) as usize;
                        let src_end = src_start + w as usize;
                        let dst_start = (row * aligned_y_stride) as usize;
                        if src_end <= y_data.len() {
                            y_plane[dst_start..dst_start + w as usize]
                                .copy_from_slice(&y_data[src_start..src_end]);
                        }
                    }

                    // Copy UV plane with alignment
                    let mut uv_plane = vec![0u8; (aligned_uv_stride * uv_height) as usize];
                    for row in 0..uv_height {
                        let src_start = (row * uv_stride) as usize;
                        let src_end = src_start + w as usize;
                        let dst_start = (row * aligned_uv_stride) as usize;
                        if src_end <= uv_data.len() {
                            uv_plane[dst_start..dst_start + w as usize]
                                .copy_from_slice(&uv_data[src_start..src_end]);
                        }
                    }

                    if *frames_decoded == 1 {
                        info!("NV12 direct GPU path: {}x{} - bypassing CPU scaler", w, h);
                    }

                    return Some(VideoFrame {
                        width: w,
                        height: h,
                        y_plane,
                        u_plane: uv_plane,
                        v_plane: Vec::new(),
                        y_stride: aligned_y_stride,
                        u_stride: aligned_uv_stride,
                        v_stride: 0,
                        timestamp_us: 0,
                        format: PixelFormat::NV12,
                        color_range,
                        color_space,
                    });
                }

                // For other formats, use scaler to convert to YUV420P
                if scaler.is_none() || *width != w || *height != h {
                    *width = w;
                    *height = h;

                    info!("Creating scaler: {:?} {}x{} -> YUV420P {}x{}", actual_format, w, h, w, h);

                    match ScalerContext::get(
                        actual_format,
                        w,
                        h,
                        Pixel::YUV420P,
                        w,
                        h,
                        ScalerFlags::BILINEAR,
                    ) {
                        Ok(s) => *scaler = Some(s),
                        Err(e) => {
                            warn!("Failed to create scaler: {:?}", e);
                            return None;
                        }
                    }
                }

                // Convert to YUV420P
                // We must allocate the destination frame first!
                let mut yuv_frame = FfmpegFrame::new(Pixel::YUV420P, w, h);
                // get_buffer is not exposed/needed in this safe wrapper, FfmpegFrame::new handles structure
                // Ideally we should just verify the scaler works.

                if let Some(ref mut s) = scaler {
                    if let Err(e) = s.run(frame_to_use, &mut yuv_frame) {
                        warn!("Scaler run failed: {:?}", e);
                        return None;
                    }
                } else {
                    return None;
                }

                // Extract YUV planes with alignment
                let y_stride = yuv_frame.stride(0) as u32;
                let u_stride = yuv_frame.stride(1) as u32;
                let v_stride = yuv_frame.stride(2) as u32;

                let aligned_y_stride = Self::get_aligned_stride(w);
                let aligned_u_stride = Self::get_aligned_stride(w / 2);
                let aligned_v_stride = Self::get_aligned_stride(w / 2);

                let y_height = h;
                let uv_height = h / 2;

                let dim_y = w;
                let dim_uv = w / 2;

                // Helper to copy plane with alignment
                let copy_plane = |src: &[u8], src_stride: usize, dst_stride: usize, width: usize, height: usize| -> Vec<u8> {
                    let mut dst = vec![0u8; dst_stride * height];
                    for row in 0..height {
                        let src_start = row * src_stride;
                        let src_end = src_start + width;
                        let dst_start = row * dst_stride;
                        let dst_end = dst_start + width;
                        if src_end <= src.len() {
                            dst[dst_start..dst_end].copy_from_slice(&src[src_start..src_end]);
                        }
                    }
                    dst
                };

                Some(VideoFrame {
                    width: w,
                    height: h,
                    y_plane: copy_plane(yuv_frame.data(0), y_stride as usize, aligned_y_stride as usize, dim_y as usize, y_height as usize),
                    u_plane: copy_plane(yuv_frame.data(1), u_stride as usize, aligned_u_stride as usize, dim_uv as usize, uv_height as usize),
                    v_plane: copy_plane(yuv_frame.data(2), v_stride as usize, aligned_v_stride as usize, dim_uv as usize, uv_height as usize),
                    y_stride: aligned_y_stride,
                    u_stride: aligned_u_stride,
                    v_stride: aligned_v_stride,
                    timestamp_us: 0,
                    format: PixelFormat::YUV420P,
                    color_range,
                    color_space,
                })
            }
            Err(ffmpeg::Error::Other { errno }) if errno == libc::EAGAIN => None,
            Err(e) => {
                debug!("Receive frame error: {:?}", e);
                None
            }
        }
    }

    /// Decode a NAL unit - sends to decoder thread and receives result
    /// WARNING: This is BLOCKING and will stall the calling thread!
    /// For low-latency streaming, use `decode_async()` instead.
    pub fn decode(&mut self, data: &[u8]) -> Result<Option<VideoFrame>> {
        // Send decode command
        self.cmd_tx.send(DecoderCommand::Decode(data.to_vec()))
            .map_err(|_| anyhow!("Decoder thread closed"))?;

        // Receive result (blocking)
        match self.frame_rx.recv() {
            Ok(frame) => {
                if frame.is_some() {
                    self.frames_decoded += 1;
                }
                Ok(frame)
            }
            Err(_) => Err(anyhow!("Decoder thread closed")),
        }
    }

    /// Decode a NAL unit asynchronously - fire and forget
    /// The decoded frame will be written directly to the SharedFrame.
    /// Stats are sent via the stats channel returned from `new_async()`.
    /// 
    /// This method NEVER blocks the calling thread, making it ideal for
    /// the main streaming loop where input responsiveness is critical.
    pub fn decode_async(&mut self, data: &[u8], receive_time: std::time::Instant) -> Result<()> {
        self.cmd_tx.send(DecoderCommand::DecodeAsync {
            data: data.to_vec(),
            receive_time,
        }).map_err(|_| anyhow!("Decoder thread closed"))?;

        self.frames_decoded += 1; // Optimistic count
        Ok(())
    }

    /// Check if using hardware acceleration
    pub fn is_hw_accelerated(&self) -> bool {
        self.hw_accel
    }

    /// Get number of frames decoded
    pub fn frames_decoded(&self) -> u64 {
        self.frames_decoded
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        // Signal decoder thread to stop
        let _ = self.cmd_tx.send(DecoderCommand::Stop);
    }
}
