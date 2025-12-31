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

use super::{VideoFrame, PixelFormat};
use crate::app::{VideoCodec, SharedFrame};

extern crate ffmpeg_next as ffmpeg;

use ffmpeg::codec::{decoder, context::Context as CodecContext};
use ffmpeg::format::Pixel;
use ffmpeg::software::scaling::{context::Context as ScalerContext, flag::Flags as ScalerFlags};
use ffmpeg::util::frame::video::Video as FfmpegFrame;
use ffmpeg::Packet;

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
        let (decoder, hw_accel) = Self::create_decoder(codec_id)?;

        // Spawn thread to handle decoding
        thread::spawn(move || {
            let mut decoder = decoder;
            let mut scaler: Option<ScalerContext> = None;
            let mut width = 0u32;
            let mut height = 0u32;
            let mut frames_decoded = 0u64;
            let mut consecutive_failures = 0u32;
            let mut packets_received = 0u64;
            const KEYFRAME_REQUEST_THRESHOLD: u32 = 10; // Request keyframe after 10 consecutive failures (was 30)

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
                        if let Some(frame) = result {
                            if let Some(ref sf) = shared_frame {
                                sf.write(frame);
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

        // Try hardware decoders in order of preference
        // Platform-specific hardware decoders:
        // - Windows: CUVID (NVIDIA), QSV (Intel), D3D11VA, DXVA2
        // - Linux: CUVID, VAAPI, QSV
        #[cfg(not(target_os = "macos"))]
        let hw_decoder_names: Vec<&str> = match codec_id {
            ffmpeg::codec::Id::H264 => {
                #[cfg(target_os = "windows")]
                {
                    let mut decoders = vec!["h264_cuvid"]; // NVIDIA first
                    if qsv_available {
                        decoders.push("h264_qsv"); // Intel QSV (only if runtime detected)
                    }
                    decoders.push("h264_d3d11va"); // Windows D3D11 (AMD/Intel/NVIDIA)
                    decoders.push("h264_dxva2");   // Windows DXVA2 (older API)
                    decoders
                }
                #[cfg(target_os = "linux")]
                {
                    let mut decoders = vec!["h264_cuvid", "h264_vaapi"];
                    if qsv_available {
                        decoders.push("h264_qsv");
                    }
                    decoders
                }
            }
            ffmpeg::codec::Id::HEVC => {
                #[cfg(target_os = "windows")]
                {
                    let mut decoders = vec!["hevc_cuvid"];
                    if qsv_available {
                        decoders.push("hevc_qsv");
                    }
                    decoders.push("hevc_d3d11va");
                    decoders.push("hevc_dxva2");
                    decoders
                }
                #[cfg(target_os = "linux")]
                {
                    let mut decoders = vec!["hevc_cuvid", "hevc_vaapi"];
                    if qsv_available {
                        decoders.push("hevc_qsv");
                    }
                    decoders
                }
            }
            ffmpeg::codec::Id::AV1 => {
                #[cfg(target_os = "windows")]
                {
                    let mut decoders = vec!["av1_cuvid"];
                    if qsv_available {
                        decoders.push("av1_qsv");
                    }
                    decoders
                }
                #[cfg(target_os = "linux")]
                {
                    let mut decoders = vec!["av1_cuvid", "av1_vaapi"];
                    if qsv_available {
                        decoders.push("av1_qsv");
                    }
                    decoders
                }
            }
            _ => vec![],
        };

        // Try hardware decoders (Windows/Linux)
        #[cfg(not(target_os = "macos"))]
        for hw_name in &hw_decoder_names {
            if let Some(hw_codec) = ffmpeg::codec::decoder::find_by_name(hw_name) {
                // new_with_codec returns Context directly, not Result
                let mut ctx = CodecContext::new_with_codec(hw_codec);
                ctx.set_threading(ffmpeg::codec::threading::Config::count(4));

                match ctx.decoder().video() {
                    Ok(dec) => {
                        info!("Successfully created hardware decoder: {}", hw_name);
                        return Ok((dec, true));
                    }
                    Err(e) => {
                        debug!("Failed to open hardware decoder {}: {:?}", hw_name, e);
                    }
                }
            }
        }

        // Fall back to software decoder
        info!("Using software decoder (hardware acceleration not available)");
        let codec = ffmpeg::codec::decoder::find(codec_id)
            .ok_or_else(|| anyhow!("Decoder not found for {:?}", codec_id))?;

        let mut ctx = CodecContext::new_with_codec(codec);
        ctx.set_threading(ffmpeg::codec::threading::Config::count(4));

        let decoder = ctx.decoder().video()?;
        Ok((decoder, false))
    }

    /// Check if a pixel format is a hardware format
    fn is_hw_pixel_format(format: Pixel) -> bool {
        matches!(
            format,
            Pixel::VIDEOTOOLBOX
                | Pixel::CUDA
                | Pixel::VAAPI
                | Pixel::VDPAU
                | Pixel::QSV
                | Pixel::D3D11
                | Pixel::DXVA2_VLD
                | Pixel::D3D11VA_VLD
                | Pixel::D3D12
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

    /// Decode a single frame (called in decoder thread)
    fn decode_frame(
        decoder: &mut decoder::Video,
        scaler: &mut Option<ScalerContext>,
        width: &mut u32,
        height: &mut u32,
        frames_decoded: &mut u64,
        data: &[u8],
    ) -> Option<VideoFrame> {
        // Ensure data starts with start code
        let data = if data.len() >= 4 && data[0..4] == [0, 0, 0, 1] {
            data.to_vec()
        } else if data.len() >= 3 && data[0..3] == [0, 0, 1] {
            data.to_vec()
        } else {
            // Add start code
            let mut with_start = vec![0, 0, 0, 1];
            with_start.extend_from_slice(data);
            with_start
        };

        // Create packet
        let mut packet = Packet::new(data.len());
        if let Some(pkt_data) = packet.data_mut() {
            pkt_data.copy_from_slice(&data);
        } else {
            return None;
        }

        // Send packet to decoder
        if let Err(e) = decoder.send_packet(&packet) {
            // EAGAIN means we need to receive frames first
            match e {
                ffmpeg::Error::Other { errno } if errno == libc::EAGAIN => {}
                _ => debug!("Send packet error: {:?}", e),
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

                if *frames_decoded == 1 {
                    info!("First decoded frame: {}x{}, format: {:?} (hw: {:?})", w, h, actual_format, format);
                }

                // Check if frame is NV12 - skip CPU scaler and pass directly to GPU
                // NV12 has Y plane (full res) and UV plane (half res, interleaved)
                // GPU shader will deinterleave UV - much faster than CPU scaler
                if actual_format == Pixel::NV12 {
                    *width = w;
                    *height = h;

                    // Extract Y plane (plane 0)
                    let y_plane = frame_to_use.data(0).to_vec();
                    let y_stride = frame_to_use.stride(0) as u32;

                    // Extract interleaved UV plane (plane 1)
                    // NV12: UV plane is half height, full width, 2 bytes per pixel (U, V interleaved)
                    let uv_plane = frame_to_use.data(1).to_vec();
                    let uv_stride = frame_to_use.stride(1) as u32;

                    if *frames_decoded == 1 {
                        info!("NV12 direct path: Y {}x{} stride {}, UV stride {} - GPU will handle conversion",
                            w, h, y_stride, uv_stride);
                    }

                    return Some(VideoFrame {
                        width: w,
                        height: h,
                        y_plane,
                        u_plane: uv_plane, // Interleaved UV data
                        v_plane: Vec::new(), // Empty for NV12
                        y_stride,
                        u_stride: uv_stride,
                        v_stride: 0,
                        timestamp_us: 0,
                        format: PixelFormat::NV12,
                    });
                }

                // For other formats, use scaler to convert to YUV420P
                if scaler.is_none() || *width != w || *height != h {
                    *width = w;
                    *height = h;

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
                let mut yuv_frame = FfmpegFrame::empty();
                if let Some(ref mut s) = scaler {
                    if let Err(e) = s.run(frame_to_use, &mut yuv_frame) {
                        warn!("Scaler run failed: {:?}", e);
                        return None;
                    }
                } else {
                    return None;
                }

                // Extract YUV planes
                let y_plane = yuv_frame.data(0).to_vec();
                let u_plane = yuv_frame.data(1).to_vec();
                let v_plane = yuv_frame.data(2).to_vec();

                let y_stride = yuv_frame.stride(0) as u32;
                let u_stride = yuv_frame.stride(1) as u32;
                let v_stride = yuv_frame.stride(2) as u32;

                Some(VideoFrame {
                    width: w,
                    height: h,
                    y_plane,
                    u_plane,
                    v_plane,
                    y_stride,
                    u_stride,
                    v_stride,
                    timestamp_us: 0,
                    format: PixelFormat::YUV420P,
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

/// Codec type for depacketizer
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DepacketizerCodec {
    H264,
    H265,
}

/// RTP depacketizer supporting H.264 and H.265/HEVC
pub struct RtpDepacketizer {
    codec: DepacketizerCodec,
    buffer: Vec<u8>,
    fragments: Vec<Vec<u8>>,
    in_fragment: bool,
    /// Cached VPS NAL unit (H.265 only)
    vps: Option<Vec<u8>>,
    /// Cached SPS NAL unit
    sps: Option<Vec<u8>>,
    /// Cached PPS NAL unit
    pps: Option<Vec<u8>>,
}

impl RtpDepacketizer {
    pub fn new() -> Self {
        Self::with_codec(DepacketizerCodec::H264)
    }

    pub fn with_codec(codec: DepacketizerCodec) -> Self {
        Self {
            codec,
            buffer: Vec::with_capacity(64 * 1024),
            fragments: Vec::new(),
            in_fragment: false,
            vps: None,
            sps: None,
            pps: None,
        }
    }

    /// Set the codec type
    pub fn set_codec(&mut self, codec: DepacketizerCodec) {
        self.codec = codec;
        // Clear cached parameter sets when codec changes
        self.vps = None;
        self.sps = None;
        self.pps = None;
        self.buffer.clear();
        self.in_fragment = false;
    }

    /// Process an RTP payload and return complete NAL units
    pub fn process(&mut self, payload: &[u8]) -> Vec<Vec<u8>> {
        match self.codec {
            DepacketizerCodec::H264 => self.process_h264(payload),
            DepacketizerCodec::H265 => self.process_h265(payload),
        }
    }

    /// Process H.264 RTP payload
    fn process_h264(&mut self, payload: &[u8]) -> Vec<Vec<u8>> {
        let mut result = Vec::new();

        if payload.is_empty() {
            return result;
        }

        let nal_type = payload[0] & 0x1F;

        match nal_type {
            // Single NAL unit (1-23)
            1..=23 => {
                // Cache SPS/PPS for later use
                if nal_type == 7 {
                    debug!("H264: Caching SPS ({} bytes)", payload.len());
                    self.sps = Some(payload.to_vec());
                } else if nal_type == 8 {
                    debug!("H264: Caching PPS ({} bytes)", payload.len());
                    self.pps = Some(payload.to_vec());
                }
                result.push(payload.to_vec());
            }

            // STAP-A (24) - Single-time aggregation packet
            24 => {
                let mut offset = 1;
                debug!("H264 STAP-A packet: {} bytes total", payload.len());

                while offset + 2 <= payload.len() {
                    let size = u16::from_be_bytes([payload[offset], payload[offset + 1]]) as usize;
                    offset += 2;

                    if offset + size > payload.len() {
                        warn!("H264 STAP-A: invalid size {} at offset {}", size, offset);
                        break;
                    }

                    let nal_data = payload[offset..offset + size].to_vec();
                    let inner_nal_type = nal_data.first().map(|b| b & 0x1F).unwrap_or(0);

                    // Cache SPS/PPS
                    if inner_nal_type == 7 {
                        self.sps = Some(nal_data.clone());
                    } else if inner_nal_type == 8 {
                        self.pps = Some(nal_data.clone());
                    }

                    result.push(nal_data);
                    offset += size;
                }
            }

            // FU-A (28) - Fragmentation unit
            28 => {
                if payload.len() < 2 {
                    return result;
                }

                let fu_header = payload[1];
                let start = (fu_header & 0x80) != 0;
                let end = (fu_header & 0x40) != 0;
                let inner_nal_type = fu_header & 0x1F;

                if start {
                    self.buffer.clear();
                    self.in_fragment = true;
                    let nal_header = (payload[0] & 0xE0) | inner_nal_type;
                    self.buffer.push(nal_header);
                    self.buffer.extend_from_slice(&payload[2..]);
                } else if self.in_fragment {
                    self.buffer.extend_from_slice(&payload[2..]);
                }

                if end && self.in_fragment {
                    self.in_fragment = false;
                    let inner_nal_type = self.buffer.first().map(|b| b & 0x1F).unwrap_or(0);

                    // For IDR frames, prepend SPS/PPS
                    if inner_nal_type == 5 {
                        if let (Some(sps), Some(pps)) = (&self.sps, &self.pps) {
                            result.push(sps.clone());
                            result.push(pps.clone());
                        }
                    }

                    result.push(self.buffer.clone());
                }
            }

            _ => {
                debug!("H264: Unknown NAL type: {}", nal_type);
            }
        }

        result
    }

    /// Process H.265/HEVC RTP payload (RFC 7798)
    fn process_h265(&mut self, payload: &[u8]) -> Vec<Vec<u8>> {
        let mut result = Vec::new();

        if payload.len() < 2 {
            return result;
        }

        // H.265 NAL unit header is 2 bytes
        // Type is in bits 1-6 of first byte: (byte0 >> 1) & 0x3F
        let nal_type = (payload[0] >> 1) & 0x3F;

        match nal_type {
            // Single NAL unit (0-47, but 48 and 49 are special)
            0..=47 => {
                // Cache VPS/SPS/PPS for later use
                match nal_type {
                    32 => {
                        debug!("H265: Caching VPS ({} bytes)", payload.len());
                        self.vps = Some(payload.to_vec());
                    }
                    33 => {
                        debug!("H265: Caching SPS ({} bytes)", payload.len());
                        self.sps = Some(payload.to_vec());
                    }
                    34 => {
                        debug!("H265: Caching PPS ({} bytes)", payload.len());
                        self.pps = Some(payload.to_vec());
                    }
                    _ => {}
                }
                result.push(payload.to_vec());
            }

            // AP (48) - Aggregation Packet
            48 => {
                let mut offset = 2; // Skip the 2-byte NAL unit header
                debug!("H265 AP packet: {} bytes total", payload.len());

                while offset + 2 <= payload.len() {
                    let size = u16::from_be_bytes([payload[offset], payload[offset + 1]]) as usize;
                    offset += 2;

                    if offset + size > payload.len() {
                        warn!("H265 AP: invalid size {} at offset {}", size, offset);
                        break;
                    }

                    let nal_data = payload[offset..offset + size].to_vec();

                    if nal_data.len() >= 2 {
                        let inner_nal_type = (nal_data[0] >> 1) & 0x3F;
                        // Cache VPS/SPS/PPS
                        match inner_nal_type {
                            32 => self.vps = Some(nal_data.clone()),
                            33 => self.sps = Some(nal_data.clone()),
                            34 => self.pps = Some(nal_data.clone()),
                            _ => {}
                        }
                    }

                    result.push(nal_data);
                    offset += size;
                }
            }

            // FU (49) - Fragmentation Unit
            49 => {
                if payload.len() < 3 {
                    return result;
                }

                // FU header is at byte 2
                let fu_header = payload[2];
                let start = (fu_header & 0x80) != 0;
                let end = (fu_header & 0x40) != 0;
                let inner_nal_type = fu_header & 0x3F;

                if start {
                    self.buffer.clear();
                    self.in_fragment = true;

                    // Reconstruct NAL unit header from original header + inner type
                    // H265 NAL header: forbidden_zero_bit(1) | nal_unit_type(6) | nuh_layer_id(6) | nuh_temporal_id_plus1(3)
                    // First byte: (forbidden_zero_bit << 7) | (inner_nal_type << 1) | (layer_id >> 5)
                    // Second byte: (layer_id << 3) | temporal_id
                    let layer_id = payload[0] & 0x01; // lowest bit of first byte
                    let temporal_id = payload[1]; // second byte

                    let nal_header_byte0 = (inner_nal_type << 1) | layer_id;
                    let nal_header_byte1 = temporal_id;

                    self.buffer.push(nal_header_byte0);
                    self.buffer.push(nal_header_byte1);
                    self.buffer.extend_from_slice(&payload[3..]);
                } else if self.in_fragment {
                    self.buffer.extend_from_slice(&payload[3..]);
                }

                if end && self.in_fragment {
                    self.in_fragment = false;

                    if self.buffer.len() >= 2 {
                        let inner_nal_type = (self.buffer[0] >> 1) & 0x3F;

                        // For IDR frames (types 19 and 20), prepend VPS/SPS/PPS
                        if inner_nal_type == 19 || inner_nal_type == 20 {
                            if let Some(vps) = &self.vps {
                                result.push(vps.clone());
                            }
                            if let Some(sps) = &self.sps {
                                result.push(sps.clone());
                            }
                            if let Some(pps) = &self.pps {
                                result.push(pps.clone());
                            }
                        }
                    }

                    result.push(self.buffer.clone());
                }
            }

            _ => {
                debug!("H265: Unknown NAL type: {}", nal_type);
            }
        }

        result
    }
}

impl Default for RtpDepacketizer {
    fn default() -> Self {
        Self::new()
    }
}
