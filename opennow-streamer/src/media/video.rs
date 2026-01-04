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
use crate::app::{VideoCodec, SharedFrame, config::VideoDecoderBackend};

extern crate ffmpeg_next as ffmpeg;

use ffmpeg::codec::{decoder, context::Context as CodecContext};
use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::{context::Context as ScalerContext, flag::Flags as ScalerFlags};
use ffmpeg::util::frame::video::Video as FfmpegFrame;
use ffmpeg::Packet;

/// GPU Vendor for decoder optimization
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum GpuVendor {
    Nvidia,
    Intel,
    Amd,
    Apple,
    Broadcom, // Raspberry Pi VideoCore
    Other,
    Unknown,
}

/// Cached GPU vendor
static GPU_VENDOR: std::sync::OnceLock<GpuVendor> = std::sync::OnceLock::new();

/// Detect the primary GPU vendor using wgpu, prioritizing discrete GPUs
pub fn detect_gpu_vendor() -> GpuVendor {
    *GPU_VENDOR.get_or_init(|| {
        // blocked_on because we are in a sync context (VideoDecoder::new)
        // but wgpu adapter request is async
        pollster::block_on(async {
            let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());  // Needs borrow

            // Enumerate all available adapters (wgpu 28 returns a Future)
            let adapters = instance.enumerate_adapters(wgpu::Backends::all()).await;

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
                } else if name.contains("videocore") || name.contains("broadcom") || name.contains("v3d") || name.contains("vc4") {
                    vendor = GpuVendor::Broadcom;
                    score += 30; // Raspberry Pi - low power device
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
                 
                 let adapter_result = instance
                    .request_adapter(&wgpu::RequestAdapterOptions {
                        power_preference: wgpu::PowerPreference::HighPerformance,
                        compatible_surface: None,
                        force_fallback_adapter: false,
                    })
                    .await;

                // Handle Result
                if let Ok(adapter) = adapter_result {
                    let info = adapter.get_info();
                    let name = info.name.to_lowercase();
                    
                     if name.contains("nvidia") { GpuVendor::Nvidia }
                     else if name.contains("intel") { GpuVendor::Intel }
                     else if name.contains("amd") { GpuVendor::Amd }
                     else if name.contains("apple") { GpuVendor::Apple }
                     else if name.contains("videocore") || name.contains("broadcom") || name.contains("v3d") { GpuVendor::Broadcom }
                     else { GpuVendor::Other }
                } else {
                    GpuVendor::Unknown
                }
            }
        })
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

/// Cached supported decoder backends
static SUPPORTED_BACKENDS: std::sync::OnceLock<Vec<VideoDecoderBackend>> = std::sync::OnceLock::new();

/// Get list of supported decoder backends for the current system
pub fn get_supported_decoder_backends() -> Vec<VideoDecoderBackend> {
    SUPPORTED_BACKENDS.get_or_init(|| {
        let mut backends = vec![VideoDecoderBackend::Auto];

        // Always check what's actually available
        #[cfg(target_os = "macos")]
        {
            backends.push(VideoDecoderBackend::VideoToolbox);
        }

        #[cfg(target_os = "windows")]
        {
            let gpu = detect_gpu_vendor();
            let qsv = check_qsv_available();
            
            if gpu == GpuVendor::Nvidia {
                backends.push(VideoDecoderBackend::Cuvid);
            }
            
            if qsv || gpu == GpuVendor::Intel {
                backends.push(VideoDecoderBackend::Qsv);
            }
            
            // DXVA is generally available on Windows
            backends.push(VideoDecoderBackend::Dxva);
        }

        #[cfg(target_os = "linux")]
        {
            let gpu = detect_gpu_vendor();
            let qsv = check_qsv_available();
            
            if gpu == GpuVendor::Nvidia {
                backends.push(VideoDecoderBackend::Cuvid);
            }
            
            if qsv || gpu == GpuVendor::Intel {
                backends.push(VideoDecoderBackend::Qsv);
            }
            
            // VAAPI is generally available on Linux (AMD/Intel)
            backends.push(VideoDecoderBackend::Vaapi);
        }
        
        backends.push(VideoDecoderBackend::Software);
        backends
    }).clone()
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
    pub fn new(codec: VideoCodec, backend: VideoDecoderBackend) -> Result<Self> {
        // Initialize FFmpeg
        ffmpeg::init().map_err(|e| anyhow!("Failed to initialize FFmpeg: {:?}", e))?;

        // Suppress FFmpeg's "no frame" info messages (EAGAIN is normal for H.264)
        unsafe {
            ffmpeg::ffi::av_log_set_level(ffmpeg::ffi::AV_LOG_ERROR as i32);
        }

        info!("Creating FFmpeg video decoder for {:?} (backend: {:?})", codec, backend);

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
        let hw_accel = Self::spawn_decoder_thread(decoder_id, cmd_rx, frame_tx, None, None, backend)?;

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
    pub fn new_async(codec: VideoCodec, backend: VideoDecoderBackend, shared_frame: Arc<SharedFrame>) -> Result<(Self, tokio_mpsc::Receiver<DecodeStats>)> {
        // Initialize FFmpeg
        ffmpeg::init().map_err(|e| anyhow!("Failed to initialize FFmpeg: {:?}", e))?;

        // Suppress FFmpeg's "no frame" info messages (EAGAIN is normal for H.264)
        unsafe {
            ffmpeg::ffi::av_log_set_level(ffmpeg::ffi::AV_LOG_ERROR as i32);
        }

        info!("Creating FFmpeg video decoder (async mode) for {:?} (backend: {:?})", codec, backend);

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
            backend
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
        backend: VideoDecoderBackend,
    ) -> Result<bool> {
        // Create decoder synchronously to report hw_accel status
        info!("Creating decoder for codec {:?}...", codec_id);
        let (decoder, hw_accel) = Self::create_decoder(codec_id, backend)?;
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
                            false, // No recovery tracking for blocking mode
                        );
                        let _ = frame_tx.send(result);
                    }
                    DecoderCommand::DecodeAsync { data, receive_time } => {
                        packets_received += 1;

                        // Check if we're in recovery mode (waiting for keyframe)
                        let in_recovery = consecutive_failures >= KEYFRAME_REQUEST_THRESHOLD;

                        // Non-blocking mode - write directly to SharedFrame
                        let result = Self::decode_frame(
                            &mut decoder,
                            &mut scaler,
                            &mut width,
                            &mut height,
                            &mut frames_decoded,
                            &data,
                            codec_id,
                            in_recovery,
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



    /// FFI Callback for format negotiation (VideoToolbox)
    #[cfg(target_os = "macos")]
    unsafe extern "C" fn get_videotoolbox_format(
        _ctx: *mut ffmpeg::ffi::AVCodecContext,
        mut fmt: *const ffmpeg::ffi::AVPixelFormat,
    ) -> ffmpeg::ffi::AVPixelFormat {
        use ffmpeg::ffi::*;

        // Log all available formats for debugging
        let mut available_formats = Vec::new();
        let mut check_fmt = fmt;
        while *check_fmt != AVPixelFormat::AV_PIX_FMT_NONE {
            available_formats.push(*check_fmt as i32);
            check_fmt = check_fmt.add(1);
        }
        info!("get_format callback: available formats: {:?} (VIDEOTOOLBOX={}, NV12={}, YUV420P={})",
            available_formats,
            AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX as i32,
            AVPixelFormat::AV_PIX_FMT_NV12 as i32,
            AVPixelFormat::AV_PIX_FMT_YUV420P as i32);

        while *fmt != AVPixelFormat::AV_PIX_FMT_NONE {
            if *fmt == AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX {
                info!("get_format: selecting VIDEOTOOLBOX hardware format");
                return AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX;
            }
            fmt = fmt.add(1);
        }

        info!("get_format: VIDEOTOOLBOX not available, falling back to NV12");
        AVPixelFormat::AV_PIX_FMT_NV12
    }

    /// FFI Callback for D3D11VA format negotiation (works on all Windows GPUs)
    /// This produces D3D11 textures that can be shared with wgpu via DXGI handles
    ///
    /// CRITICAL: This callback must set up hw_frames_ctx for D3D11VA to work!
    #[cfg(target_os = "windows")]
    unsafe extern "C" fn get_d3d11va_format(
        ctx: *mut ffmpeg::ffi::AVCodecContext,
        fmt: *const ffmpeg::ffi::AVPixelFormat,
    ) -> ffmpeg::ffi::AVPixelFormat {
        use ffmpeg::ffi::*;

        // Check if D3D11 format is available
        let mut has_d3d11 = false;
        let mut check_fmt = fmt;
        while *check_fmt != AVPixelFormat::AV_PIX_FMT_NONE {
            if *check_fmt == AVPixelFormat::AV_PIX_FMT_D3D11 {
                has_d3d11 = true;
                break;
            }
            check_fmt = check_fmt.add(1);
        }

        if !has_d3d11 {
            warn!("get_format: D3D11 not in available formats list");
            // Return the first available format
            return *fmt;
        }

        // We need hw_device_ctx to create hw_frames_ctx
        if (*ctx).hw_device_ctx.is_null() {
            warn!("get_format: hw_device_ctx is null, cannot use D3D11VA");
            return *fmt;
        }

        // Check if hw_frames_ctx already exists (might be called multiple times)
        if !(*ctx).hw_frames_ctx.is_null() {
            info!("get_format: hw_frames_ctx already set, selecting D3D11");
            return AVPixelFormat::AV_PIX_FMT_D3D11;
        }

        // Allocate hw_frames_ctx from hw_device_ctx
        let hw_frames_ref = av_hwframe_ctx_alloc((*ctx).hw_device_ctx);
        if hw_frames_ref.is_null() {
            warn!("get_format: Failed to allocate hw_frames_ctx");
            return *fmt;
        }

        // Configure the frames context
        let frames_ctx = (*hw_frames_ref).data as *mut AVHWFramesContext;
        (*frames_ctx).format = AVPixelFormat::AV_PIX_FMT_D3D11;

        // Determine sw_format based on codec and bit depth
        // HEVC Main10 profile needs P010 (10-bit), others use NV12 (8-bit)
        let sw_format = if (*ctx).codec_id == AVCodecID::AV_CODEC_ID_HEVC && (*ctx).profile == 2 {
            // Main10 profile
            info!("get_format: HEVC Main10 detected, using P010 format");
            AVPixelFormat::AV_PIX_FMT_P010LE
        } else if (*ctx).pix_fmt == AVPixelFormat::AV_PIX_FMT_YUV420P10LE
               || (*ctx).pix_fmt == AVPixelFormat::AV_PIX_FMT_YUV420P10BE {
            info!("get_format: 10-bit content detected, using P010 format");
            AVPixelFormat::AV_PIX_FMT_P010LE
        } else {
            AVPixelFormat::AV_PIX_FMT_NV12
        };

        (*frames_ctx).sw_format = sw_format;
        (*frames_ctx).width = (*ctx).coded_width;
        (*frames_ctx).height = (*ctx).coded_height;
        (*frames_ctx).initial_pool_size = 20; // Larger pool for smoother decoding

        info!("get_format: Configuring D3D11VA hw_frames_ctx: {}x{}, sw_format={:?}, pool_size=20",
            (*ctx).coded_width, (*ctx).coded_height, sw_format as i32);

        // Initialize the frames context
        let ret = av_hwframe_ctx_init(hw_frames_ref);
        if ret < 0 {
            // Try again with NV12 if P010 failed
            if sw_format != AVPixelFormat::AV_PIX_FMT_NV12 {
                warn!("get_format: P010 failed, trying NV12 fallback");
                (*frames_ctx).sw_format = AVPixelFormat::AV_PIX_FMT_NV12;
                let ret2 = av_hwframe_ctx_init(hw_frames_ref);
                if ret2 >= 0 {
                    (*ctx).hw_frames_ctx = av_buffer_ref(hw_frames_ref);
                    av_buffer_unref(&mut (hw_frames_ref as *mut _));
                    info!("get_format: D3D11VA hw_frames_ctx initialized with NV12 fallback!");
                    return AVPixelFormat::AV_PIX_FMT_D3D11;
                }
            }
            warn!("get_format: Failed to initialize hw_frames_ctx (error {})", ret);
            av_buffer_unref(&mut (hw_frames_ref as *mut _));
            return *fmt;
        }

        // Attach to codec context
        (*ctx).hw_frames_ctx = av_buffer_ref(hw_frames_ref);
        av_buffer_unref(&mut (hw_frames_ref as *mut _));

        info!("get_format: D3D11VA hw_frames_ctx initialized successfully - zero-copy enabled!");
        AVPixelFormat::AV_PIX_FMT_D3D11
    }

    /// FFI Callback for CUDA format negotiation (NVIDIA CUVID)
    #[cfg(target_os = "windows")]
    unsafe extern "C" fn get_cuda_format(
        _ctx: *mut ffmpeg::ffi::AVCodecContext,
        fmt: *const ffmpeg::ffi::AVPixelFormat,
    ) -> ffmpeg::ffi::AVPixelFormat {
        use ffmpeg::ffi::*;

        let mut check_fmt = fmt;
        while *check_fmt != AVPixelFormat::AV_PIX_FMT_NONE {
            if *check_fmt == AVPixelFormat::AV_PIX_FMT_CUDA {
                info!("get_format: selecting CUDA hardware format");
                return AVPixelFormat::AV_PIX_FMT_CUDA;
            }
            check_fmt = check_fmt.add(1);
        }

        // Fallback to NV12
        info!("get_format: CUDA not available, falling back to NV12");
        AVPixelFormat::AV_PIX_FMT_NV12
    }

    /// Create decoder, trying hardware acceleration based on preference
    fn create_decoder(codec_id: ffmpeg::codec::Id, backend: VideoDecoderBackend) -> Result<(decoder::Video, bool)> {
        info!("create_decoder: {:?} with backend preference {:?}", codec_id, backend);

        // On macOS, try VideoToolbox hardware acceleration
        #[cfg(target_os = "macos")]
        {
            if backend == VideoDecoderBackend::Auto || backend == VideoDecoderBackend::VideoToolbox {
                info!("macOS detected - attempting VideoToolbox hardware acceleration");

                // First try to find specific VideoToolbox decoders
                let vt_decoder_name = match codec_id {
                    ffmpeg::codec::Id::AV1 => Some("av1_videotoolbox"),
                    ffmpeg::codec::Id::HEVC => Some("hevc_videotoolbox"),
                    ffmpeg::codec::Id::H264 => Some("h264_videotoolbox"),
                    _ => None,
                };

                if let Some(name) = vt_decoder_name {
                    if let Some(codec) = ffmpeg::codec::decoder::find_by_name(name) {
                        info!("Found specific VideoToolbox decoder: {}", name);
                        
                        // Try to use explicit decoder with hardware context attached
                        // This helps ensure we get VIDEOTOOLBOX frames even without set_get_format
                        let res = unsafe {
                            use ffmpeg::ffi::*;
                            use std::ptr;
                            
                            let mut ctx = CodecContext::new_with_codec(codec);
                            
                            // Create HW device context
                            let mut hw_device_ctx: *mut AVBufferRef = ptr::null_mut();
                            let ret = av_hwdevice_ctx_create(
                                &mut hw_device_ctx,
                                AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX,
                                ptr::null(),
                                ptr::null_mut(),
                                0,
                            );
                            
                            if ret >= 0 && !hw_device_ctx.is_null() {
                                let raw_ctx = ctx.as_mut_ptr();
                                (*raw_ctx).hw_device_ctx = av_buffer_ref(hw_device_ctx);
                                av_buffer_unref(&mut hw_device_ctx);
                                
                                // FORCE VIDEOTOOLBOX FORMAT via callback and simple hint
                                (*raw_ctx).get_format = Some(Self::get_videotoolbox_format);
                                (*raw_ctx).pix_fmt = AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX;
                            }

                            ctx.set_threading(ffmpeg::codec::threading::Config::count(4));
                            ctx.decoder().video()
                        };

                        match res {
                            Ok(decoder) => {
                                info!("Specific VideoToolbox decoder ({}) opened successfully", name);
                                return Ok((decoder, true));
                            }
                            Err(e) => {
                                warn!("Failed to open specific VideoToolbox decoder {}: {:?}", name, e);
                            }
                        }
                    }
                }
                
                // Fallback: Generic decoder with manual hw_device_ctx attachment
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
                        av_buffer_unref(&mut hw_device_ctx);

                        // CRITICAL: Set get_format callback to request VideoToolbox pixel format
                        // Without this, the decoder will output software frames (YUV420P)
                        (*raw_ctx).get_format = Some(Self::get_videotoolbox_format);

                        // Use single thread for lowest latency - multi-threading causes frame reordering delays
                        (*raw_ctx).thread_count = 1;
                        
                        // Low latency flags for streaming (same as Windows D3D11VA)
                        (*raw_ctx).flags |= AV_CODEC_FLAG_LOW_DELAY as i32;
                        (*raw_ctx).flags2 |= AV_CODEC_FLAG2_FAST as i32;

                        match ctx.decoder().video() {
                            Ok(decoder) => {
                                info!("VideoToolbox hardware decoder created successfully (generic + hw_device + get_format)");
                                return Ok((decoder, true));
                            }
                            Err(e) => {
                                warn!("Failed to open VideoToolbox decoder: {:?}", e);
                            }
                        }
                    } else {
                        warn!("Failed to create VideoToolbox device context (error {})", ret);
                    }
                }
            } else {
                info!("VideoToolbox disabled by preference: {:?}", backend);
            }
        }

        // Platform-specific hardware decoders (Windows/Linux)
        #[cfg(not(target_os = "macos"))]
        {
            // Windows hardware decoder selection
            // Priority: CUVID (NVIDIA) > QSV (Intel) > D3D11VA (universal but has driver issues)
            #[cfg(target_os = "windows")]
            if backend != VideoDecoderBackend::Software {
                let gpu_vendor = detect_gpu_vendor();

                // For NVIDIA GPUs, skip D3D11VA and use CUVID directly
                // CUVID is more reliable and has lower latency on NVIDIA hardware
                let try_d3d11va = gpu_vendor != GpuVendor::Nvidia
                    && (backend == VideoDecoderBackend::Auto || backend == VideoDecoderBackend::Dxva);

                // Try D3D11VA for non-NVIDIA GPUs (AMD, Intel)
                if try_d3d11va {
                    info!("Attempting D3D11VA hardware acceleration (for AMD/Intel GPU)");

                    let codec = ffmpeg::codec::decoder::find(codec_id)
                        .ok_or_else(|| anyhow!("Decoder not found for {:?}", codec_id));

                    if let Ok(codec) = codec {
                        let result = unsafe {
                            use ffmpeg::ffi::*;
                            use windows::core::Interface;
                            use windows::Win32::Foundation::HMODULE;
                            use windows::Win32::Graphics::Direct3D::*;
                            use windows::Win32::Graphics::Direct3D11::*;

                            // Create D3D11 device with VIDEO_SUPPORT flag
                            // This is critical for D3D11VA to work properly
                            let mut device: Option<ID3D11Device> = None;
                            let mut context: Option<ID3D11DeviceContext> = None;
                            let mut feature_level = D3D_FEATURE_LEVEL_11_0;

                            let flags = D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT;

                            let hr = D3D11CreateDevice(
                                None, // Default adapter
                                D3D_DRIVER_TYPE_HARDWARE,
                                HMODULE::default(), // No software rasterizer
                                flags,
                                Some(&[D3D_FEATURE_LEVEL_11_1, D3D_FEATURE_LEVEL_11_0]),
                                D3D11_SDK_VERSION,
                                Some(&mut device),
                                Some(&mut feature_level),
                                Some(&mut context),
                            );

                            if hr.is_err() || device.is_none() {
                                warn!("Failed to create D3D11 device with video support: {:?}", hr);
                                // Fall through to CUVID/QSV
                            } else {
                                let device = device.unwrap();
                                info!("Created D3D11 device with VIDEO_SUPPORT flag (feature level: {:?})", feature_level);

                                // Enable multithread protection
                                if let Ok(mt) = device.cast::<ID3D11Multithread>() {
                                    mt.SetMultithreadProtected(true);
                                }

                                // Allocate hw_device_ctx and configure with our device
                                let hw_device_ref = av_hwdevice_ctx_alloc(AVHWDeviceType::AV_HWDEVICE_TYPE_D3D11VA);
                                if !hw_device_ref.is_null() {
                                    // Get the D3D11VA device context and set our device
                                    // AVD3D11VADeviceContext structure: first field is ID3D11Device*
                                    let hw_device_ctx = (*hw_device_ref).data as *mut AVHWDeviceContext;
                                    let d3d11_device_hwctx = (*hw_device_ctx).hwctx as *mut *mut std::ffi::c_void;

                                    // Set the device pointer (first field of AVD3D11VADeviceContext)
                                    *d3d11_device_hwctx = std::mem::transmute_copy(&device);
                                    std::mem::forget(device); // Don't drop, FFmpeg owns it now

                                    // Initialize the device context
                                    let ret = av_hwdevice_ctx_init(hw_device_ref);
                                    if ret >= 0 {
                                        info!("D3D11VA hw_device_ctx initialized with custom video device");

                                        let mut ctx = CodecContext::new_with_codec(codec);
                                        let raw_ctx = ctx.as_mut_ptr();

                                        (*raw_ctx).hw_device_ctx = av_buffer_ref(hw_device_ref);
                                        av_buffer_unref(&mut (hw_device_ref as *mut _));

                                        // Set format callback to select D3D11 pixel format
                                        (*raw_ctx).get_format = Some(Self::get_d3d11va_format);

                                        // Low latency flags for streaming
                                        (*raw_ctx).flags |= AV_CODEC_FLAG_LOW_DELAY as i32;
                                        (*raw_ctx).flags2 |= AV_CODEC_FLAG2_FAST as i32;
                                        (*raw_ctx).thread_count = 1; // Single thread for lowest latency

                                        match ctx.decoder().video() {
                                            Ok(decoder) => {
                                                info!("D3D11VA hardware decoder opened successfully - zero-copy GPU decoding active!");
                                                return Ok((decoder, true));
                                            }
                                            Err(e) => {
                                                warn!("D3D11VA decoder failed to open: {:?}", e);
                                            }
                                        }
                                    } else {
                                        warn!("Failed to initialize D3D11VA device context (error {})", ret);
                                        av_buffer_unref(&mut (hw_device_ref as *mut _));
                                    }
                                } else {
                                    warn!("Failed to allocate D3D11VA device context");
                                }
                            }

                            // D3D11VA failed, return error to try next backend
                            Err(ffmpeg::Error::Bug)
                        };

                        match result {
                            Ok(decoder) => {
                                info!("D3D11VA hardware decoder opened successfully - zero-copy GPU decoding active!");
                                return Ok((decoder, true));
                            }
                            Err(e) => {
                                warn!("D3D11VA decoder failed to open: {:?}, trying other backends...", e);
                            }
                        }
                    }
                }

                // Try dedicated hardware decoders (CUVID/QSV)
                // CUVID for NVIDIA, QSV for Intel - these are the most reliable options
                let qsv_available = check_qsv_available();

                // Don't try NVIDIA CUVID decoders on non-NVIDIA GPUs (causes libnvcuvid load errors)
                let is_nvidia = matches!(gpu_vendor, GpuVendor::Nvidia);
                let is_intel = matches!(gpu_vendor, GpuVendor::Intel);

                // Build prioritized list of hardware decoders to try
                let hw_decoders: Vec<&str> = match codec_id {
                    ffmpeg::codec::Id::H264 => {
                        let mut list = Vec::new();
                        // NVIDIA CUVID first (most reliable for NVIDIA)
                        if gpu_vendor == GpuVendor::Nvidia || backend == VideoDecoderBackend::Cuvid {
                            list.push("h264_cuvid");
                        }
                        // Intel QSV
                        if (gpu_vendor == GpuVendor::Intel && qsv_available) || backend == VideoDecoderBackend::Qsv {
                            list.push("h264_qsv");
                        }
                        // AMD AMF (if available)
                        if gpu_vendor == GpuVendor::Amd {
                            list.push("h264_amf");
                        }
                        // Generic fallbacks - only add CUVID/QSV for appropriate GPU vendors
                        if is_nvidia && !list.contains(&"h264_cuvid") { list.push("h264_cuvid"); }
                        if is_intel && qsv_available && !list.contains(&"h264_qsv") { list.push("h264_qsv"); }
                        list
                    }
                    ffmpeg::codec::Id::HEVC => {
                        let mut list = Vec::new();
                        // NVIDIA CUVID first (most reliable for NVIDIA)
                        if gpu_vendor == GpuVendor::Nvidia || backend == VideoDecoderBackend::Cuvid {
                            list.push("hevc_cuvid");
                        }
                        // Intel QSV
                        if (gpu_vendor == GpuVendor::Intel && qsv_available) || backend == VideoDecoderBackend::Qsv {
                            list.push("hevc_qsv");
                        }
                        // AMD AMF (if available)
                        if gpu_vendor == GpuVendor::Amd {
                            list.push("hevc_amf");
                        }
                        // Generic fallbacks - only add CUVID/QSV for appropriate GPU vendors
                        if is_nvidia && !list.contains(&"hevc_cuvid") { list.push("hevc_cuvid"); }
                        if is_intel && qsv_available && !list.contains(&"hevc_qsv") { list.push("hevc_qsv"); }
                        list
                    }
                    ffmpeg::codec::Id::AV1 => {
                        let mut list = Vec::new();
                        // NVIDIA CUVID first (RTX 30+ series)
                        if gpu_vendor == GpuVendor::Nvidia || backend == VideoDecoderBackend::Cuvid {
                            list.push("av1_cuvid");
                        }
                        // Intel QSV (11th gen+)
                        if (gpu_vendor == GpuVendor::Intel && qsv_available) || backend == VideoDecoderBackend::Qsv {
                            list.push("av1_qsv");
                        }
                        // Generic fallbacks - only add CUVID/QSV for appropriate GPU vendors
                        if is_nvidia && !list.contains(&"av1_cuvid") { list.push("av1_cuvid"); }
                        if is_intel && qsv_available && !list.contains(&"av1_qsv") { list.push("av1_qsv"); }
                        list
                    }
                    _ => vec![],
                };

                info!("Trying hardware decoders for {:?}: {:?} (GPU: {:?})", codec_id, hw_decoders, gpu_vendor);

                // Try each hardware decoder in order
                for decoder_name in &hw_decoders {
                    if let Some(hw_codec) = ffmpeg::codec::decoder::find_by_name(decoder_name) {
                        info!("Found hardware decoder: {}, attempting to open...", decoder_name);

                        // For CUVID decoders, we may need CUDA device context
                        if decoder_name.contains("cuvid") {
                            let result = unsafe {
                                use ffmpeg::ffi::*;
                                use std::ptr;

                                let mut ctx = CodecContext::new_with_codec(hw_codec);
                                let raw_ctx = ctx.as_mut_ptr();

                                // Create CUDA device context for CUVID
                                let mut hw_device_ctx: *mut AVBufferRef = ptr::null_mut();
                                let ret = av_hwdevice_ctx_create(
                                    &mut hw_device_ctx,
                                    AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA,
                                    ptr::null(),
                                    ptr::null_mut(),
                                    0,
                                );

                                if ret >= 0 && !hw_device_ctx.is_null() {
                                    (*raw_ctx).hw_device_ctx = av_buffer_ref(hw_device_ctx);
                                    av_buffer_unref(&mut hw_device_ctx);
                                    (*raw_ctx).get_format = Some(Self::get_cuda_format);
                                }

                                // Set low latency flags for streaming
                                (*raw_ctx).flags |= AV_CODEC_FLAG_LOW_DELAY as i32;
                                (*raw_ctx).flags2 |= AV_CODEC_FLAG2_FAST as i32;

                                ctx.decoder().video()
                            };

                            match result {
                                Ok(decoder) => {
                                    info!("CUVID hardware decoder ({}) opened successfully - GPU decoding active!", decoder_name);
                                    return Ok((decoder, true));
                                }
                                Err(e) => {
                                    warn!("Failed to open CUVID decoder {}: {:?}", decoder_name, e);
                                }
                            }
                        } else {
                            // For QSV and other decoders, just open directly
                            let mut ctx = CodecContext::new_with_codec(hw_codec);

                            unsafe {
                                let raw_ctx = ctx.as_mut_ptr();
                                // Set low latency flags
                                (*raw_ctx).flags |= ffmpeg::ffi::AV_CODEC_FLAG_LOW_DELAY as i32;
                                (*raw_ctx).flags2 |= ffmpeg::ffi::AV_CODEC_FLAG2_FAST as i32;
                            }

                            match ctx.decoder().video() {
                                Ok(decoder) => {
                                    info!("Hardware decoder ({}) opened successfully - GPU decoding active!", decoder_name);
                                    return Ok((decoder, true));
                                }
                                Err(e) => {
                                    warn!("Failed to open hardware decoder {}: {:?}", decoder_name, e);
                                }
                            }
                        }
                    } else {
                        debug!("Hardware decoder not found in FFmpeg: {}", decoder_name);
                    }
                }

                warn!("All hardware decoders failed, will use software decoder");
            }

            // Linux hardware decoder handling
            #[cfg(target_os = "linux")]
            if backend != VideoDecoderBackend::Software {
                let qsv_available = check_qsv_available();
                let gpu_vendor = detect_gpu_vendor();

                // Don't try vendor-specific decoders on wrong GPUs
                // - CUVID is NVIDIA-only (requires libnvcuvid)
                // - QSV is Intel-only (requires Intel Media SDK/OneVPL)
                // - VAAPI works on AMD/Intel but not Raspberry Pi
                let is_nvidia = matches!(gpu_vendor, GpuVendor::Nvidia);
                let is_intel = matches!(gpu_vendor, GpuVendor::Intel);
                let is_raspberry_pi = matches!(gpu_vendor, GpuVendor::Broadcom);

                // Raspberry Pi 5 note:
                // - Only has HEVC hardware decoder (hevc_v4l2m2m)
                // - H.264 HW decoder exists but is slower than software, so not enabled
                // - No AV1 hardware decoder

                let hw_decoder_names: Vec<&str> = match codec_id {
                    ffmpeg::codec::Id::H264 => {
                        let mut decoders = Vec::new();
                        match gpu_vendor {
                            GpuVendor::Nvidia => decoders.push("h264_cuvid"),
                            GpuVendor::Intel if qsv_available => decoders.push("h264_qsv"),
                            GpuVendor::Amd => decoders.push("h264_vaapi"),
                            // Raspberry Pi 5: H.264 HW decoder is slower than software, skip it
                            GpuVendor::Broadcom => {
                                info!("Raspberry Pi detected: H.264 will use software decoder (HW is slower)");
                            }
                            _ => {}
                        }
                        // Only add CUVID fallback on NVIDIA GPUs
                        if is_nvidia && !decoders.contains(&"h264_cuvid") { decoders.push("h264_cuvid"); }
                        // Don't add VAAPI fallback on Raspberry Pi (not supported)
                        if !is_raspberry_pi && !decoders.contains(&"h264_vaapi") { decoders.push("h264_vaapi"); }
                        // QSV is Intel-only - never add as fallback for other GPUs
                        if is_intel && qsv_available && !decoders.contains(&"h264_qsv") { decoders.push("h264_qsv"); }
                        decoders
                    }
                    ffmpeg::codec::Id::HEVC => {
                        let mut decoders = Vec::new();
                        match gpu_vendor {
                            GpuVendor::Nvidia => decoders.push("hevc_cuvid"),
                            GpuVendor::Intel if qsv_available => decoders.push("hevc_qsv"),
                            GpuVendor::Amd => decoders.push("hevc_vaapi"),
                            // Raspberry Pi 5: Has dedicated HEVC hardware decoder
                            GpuVendor::Broadcom => {
                                info!("Raspberry Pi detected: Using V4L2 HEVC hardware decoder");
                                decoders.push("hevc_v4l2m2m");
                            }
                            _ => {}
                        }
                        // Only add CUVID fallback on NVIDIA GPUs
                        if is_nvidia && !decoders.contains(&"hevc_cuvid") { decoders.push("hevc_cuvid"); }
                        // Don't add VAAPI fallback on Raspberry Pi
                        if !is_raspberry_pi && !decoders.contains(&"hevc_vaapi") { decoders.push("hevc_vaapi"); }
                        // QSV is Intel-only
                        if is_intel && qsv_available && !decoders.contains(&"hevc_qsv") { decoders.push("hevc_qsv"); }
                        decoders
                    }
                    ffmpeg::codec::Id::AV1 => {
                        let mut decoders = Vec::new();
                        match gpu_vendor {
                            GpuVendor::Nvidia => decoders.push("av1_cuvid"),
                            GpuVendor::Intel if qsv_available => decoders.push("av1_qsv"),
                            GpuVendor::Amd => decoders.push("av1_vaapi"),
                            // Raspberry Pi 5: No AV1 hardware decoder
                            GpuVendor::Broadcom => {
                                info!("Raspberry Pi detected: AV1 will use software decoder (no HW support)");
                            }
                            _ => {}
                        }
                        // Only add CUVID fallback on NVIDIA GPUs
                        if is_nvidia && !decoders.contains(&"av1_cuvid") { decoders.push("av1_cuvid"); }
                        // Don't add VAAPI fallback on Raspberry Pi
                        if !is_raspberry_pi && !decoders.contains(&"av1_vaapi") { decoders.push("av1_vaapi"); }
                        // QSV is Intel-only
                        if is_intel && qsv_available && !decoders.contains(&"av1_qsv") { decoders.push("av1_qsv"); }
                        decoders
                    }
                    _ => vec![],
                };

                info!("Trying Linux hardware decoders for {:?}: {:?} (GPU: {:?})", codec_id, hw_decoder_names, gpu_vendor);

                for hw_name in &hw_decoder_names {
                    if let Some(hw_codec) = ffmpeg::codec::decoder::find_by_name(hw_name) {
                        info!("Found hardware decoder: {}, attempting to open...", hw_name);
                        let mut ctx = CodecContext::new_with_codec(hw_codec);

                        unsafe {
                            let raw_ctx = ctx.as_mut_ptr();
                            // Set low latency flags
                            (*raw_ctx).flags |= ffmpeg::ffi::AV_CODEC_FLAG_LOW_DELAY as i32;
                            (*raw_ctx).flags2 |= ffmpeg::ffi::AV_CODEC_FLAG2_FAST as i32;
                        }

                        // For V4L2 M2M on Raspberry Pi 5, try common device paths
                        if hw_name.contains("v4l2m2m") && is_raspberry_pi {
                            // Pi 5 HEVC decoder is typically at /dev/video19
                            // Try scanning for the rpivid decoder device
                            let v4l2_devices = ["/dev/video19", "/dev/video10", "/dev/video11", "/dev/video12"];

                            for device_path in v4l2_devices {
                                if std::path::Path::new(device_path).exists() {
                                    info!("Trying V4L2 device: {}", device_path);
                                    // Set device via environment variable (FFmpeg V4L2 M2M respects this)
                                    std::env::set_var("V4L2M2M_DEVICE", device_path);

                                    match ctx.decoder().video() {
                                        Ok(dec) => {
                                            info!("V4L2 hardware decoder opened with device {} - GPU decoding active!", device_path);
                                            return Ok((dec, true));
                                        }
                                        Err(e) => {
                                            debug!("V4L2 device {} failed: {:?}", device_path, e);
                                            // Recreate context for next attempt
                                            ctx = CodecContext::new_with_codec(hw_codec);
                                            unsafe {
                                                let raw_ctx = ctx.as_mut_ptr();
                                                (*raw_ctx).flags |= ffmpeg::ffi::AV_CODEC_FLAG_LOW_DELAY as i32;
                                                (*raw_ctx).flags2 |= ffmpeg::ffi::AV_CODEC_FLAG2_FAST as i32;
                                            }
                                        }
                                    }
                                }
                            }
                            // All V4L2 devices failed, skip to next decoder
                            warn!("V4L2 M2M: No working device found, falling back...");
                            continue;
                        }

                        match ctx.decoder().video() {
                            Ok(dec) => {
                                info!("Hardware decoder ({}) opened successfully - GPU decoding active!", hw_name);
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
                warn!("All Linux hardware decoders failed, will use software decoder");
            }
        }

        // Fall back to software decoder
        info!("Using software decoder for {:?}", codec_id);
        let codec = ffmpeg::codec::decoder::find(codec_id)
            .ok_or_else(|| anyhow!("Decoder not found for {:?}", codec_id))?;
        info!("Found software decoder: {:?}", codec.name());

        let mut ctx = CodecContext::new_with_codec(codec);

        // Use fewer threads on low-power devices to reduce memory usage
        let gpu_vendor = detect_gpu_vendor();
        let thread_count = if matches!(gpu_vendor, GpuVendor::Broadcom) {
            // Raspberry Pi: Use 2 threads to avoid memory overflow
            // Pi 5 has 4 cores but limited RAM bandwidth
            info!("Raspberry Pi detected: Using 2 decoder threads to conserve memory");
            2
        } else {
            // Desktop/laptop: Use 4 threads for better performance
            4
        };
        ctx.set_threading(ffmpeg::codec::threading::Config::count(thread_count));

        let decoder = ctx.decoder().video()?;
        info!("Software decoder opened successfully with {} threads", thread_count);
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

        unsafe {
            use ffmpeg::ffi::*;

            // Create a new frame for the software copy
            let sw_frame_ptr = av_frame_alloc();
            if sw_frame_ptr.is_null() {
                warn!("Failed to allocate software frame");
                return None;
            }

            // Transfer data from hardware frame to software frame
            // This is the main latency source - GPU to CPU copy
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
            Some(FfmpegFrame::wrap(sw_frame_ptr))
        }
    }

    /// Calculate 256-byte aligned stride for GPU compatibility (wgpu/DX12 requirement)
    fn get_aligned_stride(width: u32) -> u32 {
        (width + 255) & !255
    }

    /// Decode a single frame (called in decoder thread)
    /// `in_recovery` suppresses repeated warnings when waiting for keyframe
    fn decode_frame(
        decoder: &mut decoder::Video,
        scaler: &mut Option<ScalerContext>,
        width: &mut u32,
        height: &mut u32,
        frames_decoded: &mut u64,
        data: &[u8],
        codec_id: ffmpeg::codec::Id,
        in_recovery: bool,
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
                _ => {
                    // Suppress repeated warnings during keyframe recovery
                    if in_recovery {
                        debug!("Send packet error (waiting for keyframe): {:?}", e);
                    } else {
                        warn!("Send packet error: {:?}", e);
                    }
                }
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

                // Extract color metadata from original frame
                let color_range = match frame.color_range() {
                    ffmpeg::util::color::range::Range::JPEG => ColorRange::Full,
                    ffmpeg::util::color::range::Range::MPEG => ColorRange::Limited,
                    _ => ColorRange::Limited,
                };

                let color_space = match frame.color_space() {
                    ffmpeg::util::color::space::Space::BT709 => ColorSpace::BT709,
                    ffmpeg::util::color::space::Space::BT470BG => ColorSpace::BT601,
                    ffmpeg::util::color::space::Space::SMPTE170M => ColorSpace::BT601,
                    ffmpeg::util::color::space::Space::BT2020NCL => ColorSpace::BT2020,
                    _ => ColorSpace::BT709,
                };

                // ZERO-COPY PATH: For VideoToolbox, extract CVPixelBuffer directly
                // This skips the expensive GPU->CPU->GPU copy entirely
                #[cfg(target_os = "macos")]
                if format == Pixel::VIDEOTOOLBOX {
                    use crate::media::videotoolbox;
                    use std::sync::Arc;

                    // Extract CVPixelBuffer from frame.data[3] using raw FFmpeg pointer
                    // We use unsafe FFI because the safe wrapper does bounds checking
                    // that doesn't work for hardware frames
                    let cv_buffer = unsafe {
                        let raw_frame = frame.as_ptr();
                        let data_ptr = (*raw_frame).data[3] as *mut u8;
                        if !data_ptr.is_null() {
                            videotoolbox::extract_cv_pixel_buffer_from_data(data_ptr)
                        } else {
                            None
                        }
                    };

                    if let Some(buffer) = cv_buffer {
                        if *frames_decoded == 1 {
                            info!("ZERO-COPY: First frame {}x{} via CVPixelBuffer (no CPU transfer!)", w, h);
                        }

                        *width = w;
                        *height = h;

                        return Some(VideoFrame {
                            width: w,
                            height: h,
                            y_plane: Vec::new(),
                            u_plane: Vec::new(),
                            v_plane: Vec::new(),
                            y_stride: 0,
                            u_stride: 0,
                            v_stride: 0,
                            timestamp_us: 0,
                            format: PixelFormat::NV12,
                            color_range,
                            color_space,
                            gpu_frame: Some(Arc::new(buffer)),
                        });
                    } else {
                        warn!("Failed to extract CVPixelBuffer, falling back to CPU transfer");
                    }
                }

                // ZERO-COPY PATH: For D3D11VA, extract D3D11 texture directly
                // This skips the expensive GPU->CPU->GPU copy entirely
                #[cfg(target_os = "windows")]
                if format == Pixel::D3D11 || format == Pixel::D3D11VA_VLD {
                    use crate::media::d3d11;
                    use std::sync::Arc;

                    // Extract D3D11 texture from frame data
                    // FFmpeg D3D11VA frame layout:
                    // - data[0] = ID3D11Texture2D*
                    // - data[1] = texture array index (as intptr_t)
                    let d3d11_texture = unsafe {
                        let raw_frame = frame.as_ptr();
                        let data0 = (*raw_frame).data[0] as *mut u8;
                        let data1 = (*raw_frame).data[1] as *mut u8;
                        d3d11::extract_d3d11_texture_from_frame(data0, data1)
                    };

                    if let Some(texture) = d3d11_texture {
                        if *frames_decoded == 1 {
                            info!("ZERO-COPY: First frame {}x{} via D3D11 texture (no CPU transfer!)", w, h);
                        }

                        *width = w;
                        *height = h;

                        return Some(VideoFrame {
                            width: w,
                            height: h,
                            y_plane: Vec::new(),
                            u_plane: Vec::new(),
                            v_plane: Vec::new(),
                            y_stride: 0,
                            u_stride: 0,
                            v_stride: 0,
                            timestamp_us: 0,
                            format: PixelFormat::NV12,
                            color_range,
                            color_space,
                            gpu_frame: Some(Arc::new(texture)),
                        });
                    } else {
                        warn!("Failed to extract D3D11 texture, falling back to CPU transfer");
                    }
                }

                // FALLBACK: Transfer hardware frame to CPU memory
                let sw_frame = Self::transfer_hw_frame_if_needed(&frame);
                let frame_to_use = sw_frame.as_ref().unwrap_or(&frame);
                let actual_format = frame_to_use.format();

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

                    let y_data = frame_to_use.data(0);
                    let uv_data = frame_to_use.data(1);

                    // Check if we actually have data
                    if y_data.is_empty() || uv_data.is_empty() || y_stride == 0 {
                        warn!("NV12 frame has empty data: y_len={}, uv_len={}, y_stride={}",
                            y_data.len(), uv_data.len(), y_stride);
                        // Fall through to scaler path
                    } else {
                        // GPU texture upload requires 256-byte aligned rows (wgpu restriction)
                        let aligned_y_stride = Self::get_aligned_stride(w);
                        let aligned_uv_stride = Self::get_aligned_stride(w);

                        if *frames_decoded == 1 {
                            info!("NV12 direct path: {}x{}, y_stride={}, uv_stride={}, y_len={}, uv_len={}",
                                w, h, y_stride, uv_stride, y_data.len(), uv_data.len());
                        }

                        // Optimized copy - fast path when strides match
                        let copy_plane_fast = |src: &[u8], src_stride: u32, dst_stride: u32, copy_width: u32, height: u32| -> Vec<u8> {
                            let total_size = (dst_stride * height) as usize;
                            if src_stride == dst_stride && src.len() >= total_size {
                                // Fast path: single memcpy
                                src[..total_size].to_vec()
                            } else {
                                // Slow path: row-by-row
                                let mut dst = vec![0u8; total_size];
                                for row in 0..height as usize {
                                    let src_start = row * src_stride as usize;
                                    let src_end = src_start + copy_width as usize;
                                    let dst_start = row * dst_stride as usize;
                                    if src_end <= src.len() {
                                        dst[dst_start..dst_start + copy_width as usize]
                                            .copy_from_slice(&src[src_start..src_end]);
                                    }
                                }
                                dst
                            }
                        };

                        let y_plane = copy_plane_fast(y_data, y_stride, aligned_y_stride, w, h);
                        let uv_plane = copy_plane_fast(uv_data, uv_stride, aligned_uv_stride, w, uv_height);

                        if *frames_decoded == 1 {
                            info!("NV12 direct GPU path: {}x{} - bypassing CPU scaler (y={} bytes, uv={} bytes)",
                                w, h, y_plane.len(), uv_plane.len());
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
                            #[cfg(target_os = "macos")]
                            gpu_frame: None,
                            #[cfg(target_os = "windows")]
                            gpu_frame: None,
                        });
                    }
                }

                // For other formats, use scaler to convert to NV12
                // NV12 is more efficient for GPU upload and hardware decoders at high bitrates
                // Use POINT (nearest neighbor) since we're not resizing - just color format conversion
                // This is much faster than BILINEAR for same-size conversion
                if scaler.is_none() || *width != w || *height != h {
                    *width = w;
                    *height = h;

                    info!("Creating scaler: {:?} {}x{} -> NV12 {}x{} (POINT mode)", actual_format, w, h, w, h);

                    match ScalerContext::get(
                        actual_format,
                        w,
                        h,
                        Pixel::NV12,
                        w,
                        h,
                        ScalerFlags::POINT,  // Fastest - no interpolation needed for same-size conversion
                    ) {
                        Ok(s) => *scaler = Some(s),
                        Err(e) => {
                            warn!("Failed to create scaler: {:?}", e);
                            return None;
                        }
                    }
                }

                // Convert to NV12
                // We must allocate the destination frame first!
                let mut nv12_frame = FfmpegFrame::new(Pixel::NV12, w, h);

                if let Some(ref mut s) = scaler {
                    if let Err(e) = s.run(frame_to_use, &mut nv12_frame) {
                        warn!("Scaler run failed: {:?}", e);
                        return None;
                    }
                } else {
                    return None;
                }

                // Extract NV12 planes with alignment
                // NV12: Y plane (full res) + UV plane (half height, interleaved)
                let y_stride = nv12_frame.stride(0) as u32;
                let uv_stride = nv12_frame.stride(1) as u32;

                let aligned_y_stride = Self::get_aligned_stride(w);
                let aligned_uv_stride = Self::get_aligned_stride(w);

                let uv_height = h / 2;

                // Optimized plane copy - use bulk copy when strides match, row-by-row otherwise
                let copy_plane_optimized = |src: &[u8], src_stride: u32, dst_stride: u32, width: u32, height: u32| -> Vec<u8> {
                    let total_size = (dst_stride * height) as usize;

                    // Fast path: if source stride equals destination stride AND covers the data we need,
                    // we can do a single memcpy
                    if src_stride == dst_stride && src.len() >= total_size {
                        src[..total_size].to_vec()
                    } else {
                        // Slow path: row-by-row copy with stride conversion
                        let mut dst = vec![0u8; total_size];
                        let width = width as usize;
                        let src_stride = src_stride as usize;
                        let dst_stride = dst_stride as usize;

                        for row in 0..height as usize {
                            let src_start = row * src_stride;
                            let src_end = src_start + width;
                            let dst_start = row * dst_stride;
                            if src_end <= src.len() {
                                dst[dst_start..dst_start + width].copy_from_slice(&src[src_start..src_end]);
                            }
                        }
                        dst
                    }
                };

                Some(VideoFrame {
                    width: w,
                    height: h,
                    y_plane: copy_plane_optimized(nv12_frame.data(0), y_stride, aligned_y_stride, w, h),
                    u_plane: copy_plane_optimized(nv12_frame.data(1), uv_stride, aligned_uv_stride, w, uv_height),
                    v_plane: Vec::new(),  // NV12 has no separate V plane
                    y_stride: aligned_y_stride,
                    u_stride: aligned_uv_stride,
                    v_stride: 0,
                    timestamp_us: 0,
                    format: PixelFormat::NV12,
                    color_range,
                    color_space,
                    #[cfg(target_os = "macos")]
                    gpu_frame: None,
                    #[cfg(target_os = "windows")]
                    gpu_frame: None,
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
