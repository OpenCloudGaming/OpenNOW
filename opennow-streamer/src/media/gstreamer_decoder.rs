//! GStreamer Video Decoder
//!
//! Hardware-accelerated video decoding using GStreamer.
//!
//! Platform support:
//! - Windows: H.264 via D3D11 hardware acceleration (d3d11h264dec)
//! - Linux (Raspberry Pi): H.264/HEVC via V4L2 (v4l2h264dec, v4l2h265dec)
//! - Linux (Desktop): H.264/HEVC via VA-API (vaapih264dec, vaapih265dec)
//!
//! Pipeline structures:
//! - Windows: appsrc -> h264parse -> d3d11h264dec -> d3d11download -> videoconvert -> appsink
//! - Linux V4L2: appsrc -> h264parse -> v4l2h264dec -> videoconvert -> appsink
//! - Linux VA-API: appsrc -> h264parse -> vaapih264dec -> videoconvert -> appsink
//!
//! ## Windows GStreamer Bundling
//!
//! On Windows, GStreamer can be bundled with the application. The decoder will look for
//! GStreamer in the following locations (in order):
//! 1. `gstreamer/` subdirectory next to the executable
//! 2. System-installed GStreamer (GSTREAMER_1_0_ROOT_MSVC_X86_64 environment variable)
//! 3. Standard GStreamer installation paths

use anyhow::{anyhow, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};
use gstreamer_video as gst_video;
use log::{debug, info, warn};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use super::{ColorRange, ColorSpace, PixelFormat, TransferFunction, VideoFrame};

/// Initialize GStreamer with support for bundled runtime on Windows
/// This function MUST be called before any other GStreamer operations.
/// It sets up the PATH and plugin paths for bundled GStreamer on Windows.
#[cfg(target_os = "windows")]
pub fn init_gstreamer() -> Result<()> {
    use std::env;
    use std::path::PathBuf;
    use std::sync::Once;

    static INIT: Once = Once::new();
    static mut INIT_RESULT: Option<Result<(), String>> = None;

    // Thread-safe one-time initialization
    INIT.call_once(|| {
        // Try to find bundled GStreamer FIRST, before calling gst::init()
        // The DLLs must be in PATH before GStreamer tries to load them
        let exe_dir = env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));

        // GStreamer core DLLs are next to the exe (for load-time linking)
        // Plugins are in lib/gstreamer-1.0/ subdirectory
        let bundled_plugins = exe_dir.join("lib").join("gstreamer-1.0");

        // Check if we have bundled GStreamer (look for a core DLL next to exe)
        let has_bundled_gst = exe_dir.join("gstreamer-1.0-0.dll").exists();

        if has_bundled_gst {
            info!("Found bundled GStreamer DLLs at: {}", exe_dir.display());

            // Set plugin path
            if bundled_plugins.exists() {
                env::set_var("GST_PLUGIN_PATH", bundled_plugins.to_str().unwrap_or(""));
                info!("Set GST_PLUGIN_PATH to: {}", bundled_plugins.display());
            }

            // Disable plugin scanning outside bundled path for faster startup
            env::set_var("GST_PLUGIN_SYSTEM_PATH", "");
        } else {
            // Check for system GStreamer
            if let Ok(gst_root) = env::var("GSTREAMER_1_0_ROOT_MSVC_X86_64") {
                info!("Using system GStreamer from: {}", gst_root);
            } else {
                warn!("GStreamer not found. Please install GStreamer or bundle it with the app.");
                warn!("Download from: https://gstreamer.freedesktop.org/download/");
            }
        }

        // Now initialize GStreamer after PATH is set up
        unsafe {
            INIT_RESULT = Some(gst::init().map_err(|e| e.to_string()));
        }
    });

    // Return cached result
    unsafe {
        match &INIT_RESULT {
            Some(Ok(())) => Ok(()),
            Some(Err(e)) => Err(anyhow!("Failed to initialize GStreamer: {}", e)),
            None => Err(anyhow!("GStreamer initialization not completed")),
        }
    }
}

/// Initialize GStreamer (Linux - straightforward)
#[cfg(target_os = "linux")]
pub fn init_gstreamer() -> Result<()> {
    gst::init().map_err(|e| anyhow!("Failed to initialize GStreamer: {}", e))
}

/// GStreamer codec type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GstCodec {
    H264,
    H265,
    AV1,
}

impl GstCodec {
    fn caps_string(&self) -> &'static str {
        match self {
            GstCodec::H264 => "video/x-h264,stream-format=byte-stream,alignment=au",
            GstCodec::H265 => "video/x-h265,stream-format=byte-stream,alignment=au",
            GstCodec::AV1 => "video/x-av1,stream-format=obu-stream,alignment=tu",
        }
    }

    fn parser_element(&self) -> &'static str {
        match self {
            GstCodec::H264 => "h264parse",
            GstCodec::H265 => "h265parse",
            GstCodec::AV1 => "av1parse",
        }
    }

    /// Get the best available decoder element for this codec on the current platform
    #[cfg(target_os = "windows")]
    fn decoder_element(&self) -> &'static str {
        match self {
            // Windows: Use D3D11 hardware decoder for best performance
            // Falls back to software if D3D11 decoder not available
            GstCodec::H264 => "d3d11h264dec",
            GstCodec::H265 => "d3d11h265dec",
            GstCodec::AV1 => "d3d11av1dec",
        }
    }

    #[cfg(target_os = "linux")]
    fn decoder_element(&self) -> &'static str {
        // Linux: V4L2 for embedded (RPi), otherwise VA-API or software
        match self {
            GstCodec::H264 => "v4l2h264dec",
            GstCodec::H265 => "v4l2h265dec",
            GstCodec::AV1 => "v4l2av1dec", // Raspberry Pi 5 supports AV1
        }
    }

    /// Get fallback software decoder
    fn software_decoder(&self) -> &'static str {
        match self {
            GstCodec::H264 => "avdec_h264",
            GstCodec::H265 => "avdec_h265",
            GstCodec::AV1 => "av1dec", // dav1d-based decoder (preferred) or avdec_av1
        }
    }
}

/// GStreamer decoder configuration
#[derive(Debug, Clone)]
pub struct GstDecoderConfig {
    pub codec: GstCodec,
    pub width: u32,
    pub height: u32,
    /// Enable low latency mode (minimize buffering)
    pub low_latency: bool,
}

impl Default for GstDecoderConfig {
    fn default() -> Self {
        Self {
            codec: GstCodec::H264,
            width: 1920,
            height: 1080,
            low_latency: true, // Default to low latency for streaming
        }
    }
}

/// Decoded frame from GStreamer
struct DecodedFrame {
    width: u32,
    height: u32,
    y_plane: Vec<u8>,
    uv_plane: Vec<u8>,
    y_stride: u32,
    uv_stride: u32,
    /// Timestamp when frame was decoded (for latency tracking)
    decode_time: std::time::Instant,
    /// Color space from GStreamer colorimetry
    color_space: ColorSpace,
    /// Transfer function (SDR/PQ/HLG) from GStreamer colorimetry
    transfer_function: TransferFunction,
    /// Color range (Limited/Full)
    color_range: ColorRange,
}

/// GStreamer Video Decoder
///
/// Cross-platform hardware-accelerated video decoder using GStreamer.
/// - Windows: D3D11 hardware acceleration
/// - Linux: V4L2 (embedded) or VA-API (desktop)
pub struct GStreamerDecoder {
    pipeline: gst::Pipeline,
    appsrc: AppSrc,
    #[allow(dead_code)]
    appsink: AppSink,
    #[allow(dead_code)]
    config: GstDecoderConfig,
    frame_count: u64,
    last_frame: Arc<Mutex<Option<DecodedFrame>>>,
    /// Last logged transfer function (to avoid log spam)
    last_logged_transfer: TransferFunction,
}

// GStreamer is thread-safe
unsafe impl Send for GStreamerDecoder {}
unsafe impl Sync for GStreamerDecoder {}

impl GStreamerDecoder {
    /// Create a new GStreamer decoder
    pub fn new(config: GstDecoderConfig) -> Result<Self> {
        info!(
            "Creating GStreamer decoder: {:?} {}x{}",
            config.codec, config.width, config.height
        );

        // Initialize GStreamer (with bundled DLL support on Windows)
        init_gstreamer()?;

        // Build platform-specific pipeline
        let pipeline_str = Self::build_pipeline_string(&config)?;
        info!("GStreamer pipeline: {}", pipeline_str);

        // Parse and create pipeline
        let pipeline = gst::parse::launch(&pipeline_str)
            .map_err(|e| anyhow!("Failed to create GStreamer pipeline: {}", e))?
            .downcast::<gst::Pipeline>()
            .map_err(|_| anyhow!("Failed to downcast to Pipeline"))?;

        // Get appsrc
        let appsrc = pipeline
            .by_name("src")
            .ok_or_else(|| anyhow!("Failed to get appsrc"))?
            .downcast::<AppSrc>()
            .map_err(|_| anyhow!("Failed to downcast to AppSrc"))?;

        // Configure appsrc for ultra-low latency live streaming
        let caps = gst::Caps::from_str(config.codec.caps_string())
            .map_err(|e| anyhow!("Failed to create caps: {}", e))?;
        appsrc.set_caps(Some(&caps));
        appsrc.set_format(gst::Format::Time);

        // Critical low-latency settings (from GStreamer docs):
        // - stream-type=Stream for live streaming push mode
        // - max-bytes=0 disables internal buffering
        // - block=false prevents blocking when buffer is full
        // - min-latency=0 required when using do-timestamp=true
        //   (appsrc timestamps based on running-time when buffer arrives)
        appsrc.set_stream_type(gstreamer_app::AppStreamType::Stream);
        appsrc.set_max_bytes(0);
        appsrc.set_property("block", false);
        appsrc.set_property("min-latency", 0i64);
        appsrc.set_property("max-latency", 0i64);

        // Get appsink
        let appsink = pipeline
            .by_name("sink")
            .ok_or_else(|| anyhow!("Failed to get appsink"))?
            .downcast::<AppSink>()
            .map_err(|_| anyhow!("Failed to downcast to AppSink"))?;

        // Configure appsink for NV12 output with minimal latency
        let sink_caps = gst::Caps::builder("video/x-raw")
            .field("format", "NV12")
            .build();
        appsink.set_caps(Some(&sink_caps));

        // Ultra-low latency sink settings:
        // - drop=true drops old frames if we can't keep up
        // - max-buffers=1 only keeps the latest frame
        // - sync=false renders immediately without clock sync
        appsink.set_drop(true);
        appsink.set_max_buffers(1);
        appsink.set_sync(false);

        // Set up frame storage
        let last_frame = Arc::new(Mutex::new(None));
        let last_frame_clone = last_frame.clone();

        // Set up new-sample callback
        appsink.set_callbacks(
            gstreamer_app::AppSinkCallbacks::builder()
                .new_sample(move |sink| {
                    match sink.pull_sample() {
                        Ok(sample) => {
                            if let Some(buffer) = sample.buffer() {
                                if let Some(caps) = sample.caps() {
                                    if let Ok(video_info) = gst_video::VideoInfo::from_caps(caps) {
                                        let width = video_info.width();
                                        let height = video_info.height();

                                        // Extract colorimetry info for HDR detection
                                        let colorimetry = video_info.colorimetry();

                                        // Detect transfer function (SDR vs HDR PQ vs HLG)
                                        // Use raw GLib values to detect PQ/HLG without v1_18 feature
                                        // GST_VIDEO_TRANSFER_SMPTE2084 = 14, GST_VIDEO_TRANSFER_ARIB_STD_B67 = 15
                                        use gstreamer::glib::translate::IntoGlib;
                                        let transfer_raw = colorimetry.transfer().into_glib();
                                        let transfer_function = match transfer_raw {
                                            14 => {
                                                // SMPTE ST 2084 = PQ (HDR10)
                                                TransferFunction::PQ
                                            }
                                            15 => {
                                                // ARIB STD-B67 = HLG
                                                TransferFunction::HLG
                                            }
                                            _ => TransferFunction::SDR,
                                        };

                                        // Detect color space (BT.709 vs BT.2020)
                                        let color_space = match colorimetry.matrix() {
                                            gst_video::VideoColorMatrix::Bt2020 => {
                                                ColorSpace::BT2020
                                            }
                                            gst_video::VideoColorMatrix::Bt601 => ColorSpace::BT601,
                                            _ => ColorSpace::BT709,
                                        };

                                        // Detect color range (Limited vs Full)
                                        let color_range = match colorimetry.range() {
                                            gst_video::VideoColorRange::Range0_255 => {
                                                ColorRange::Full
                                            }
                                            _ => ColorRange::Limited,
                                        };

                                        // Map buffer for reading
                                        if let Ok(map) = buffer.map_readable() {
                                            let data = map.as_slice();

                                            // NV12 format: Y plane followed by interleaved UV
                                            let y_stride = video_info.stride()[0] as u32;
                                            let uv_stride = video_info.stride()[1] as u32;
                                            let y_size = (y_stride * height) as usize;
                                            let uv_size = (uv_stride * height / 2) as usize;

                                            if data.len() >= y_size + uv_size {
                                                let y_plane = data[..y_size].to_vec();
                                                let uv_plane =
                                                    data[y_size..y_size + uv_size].to_vec();

                                                let frame = DecodedFrame {
                                                    width,
                                                    height,
                                                    y_plane,
                                                    uv_plane,
                                                    y_stride,
                                                    uv_stride,
                                                    decode_time: std::time::Instant::now(),
                                                    color_space,
                                                    transfer_function,
                                                    color_range,
                                                };

                                                *last_frame_clone.lock().unwrap() = Some(frame);
                                            }
                                        }
                                    }
                                }
                            }
                            Ok(gst::FlowSuccess::Ok)
                        }
                        Err(_) => Err(gst::FlowError::Error),
                    }
                })
                .build(),
        );

        // Set up bus message monitoring for errors and state changes
        let bus = pipeline.bus().expect("Pipeline has no bus");
        std::thread::spawn(move || {
            for msg in bus.iter_timed(gst::ClockTime::NONE) {
                use gst::MessageView;
                match msg.view() {
                    MessageView::Error(err) => {
                        log::error!(
                            "GStreamer Error from {:?}: {} ({:?})",
                            err.src().map(|s| s.path_string()),
                            err.error(),
                            err.debug()
                        );
                    }
                    MessageView::Warning(warn) => {
                        log::warn!(
                            "GStreamer Warning from {:?}: {} ({:?})",
                            warn.src().map(|s| s.path_string()),
                            warn.error(),
                            warn.debug()
                        );
                    }
                    MessageView::StateChanged(state) => {
                        if state
                            .src()
                            .map(|s| s.path_string().contains("pipeline"))
                            .unwrap_or(false)
                        {
                            log::debug!(
                                "GStreamer pipeline state: {:?} -> {:?}",
                                state.old(),
                                state.current()
                            );
                        }
                    }
                    MessageView::Eos(_) => {
                        log::warn!("GStreamer: End of stream received");
                    }
                    _ => {}
                }
            }
        });

        // Start pipeline
        pipeline
            .set_state(gst::State::Playing)
            .map_err(|e| anyhow!("Failed to start pipeline: {:?}", e))?;

        info!("GStreamer decoder initialized successfully");

        Ok(Self {
            pipeline,
            appsrc,
            appsink,
            config,
            frame_count: 0,
            last_frame,
            last_logged_transfer: TransferFunction::SDR,
        })
    }

    /// Build the GStreamer pipeline string for the current platform
    fn build_pipeline_string(config: &GstDecoderConfig) -> Result<String> {
        let parser = config.codec.parser_element();
        let decoder = config.codec.decoder_element();

        // Low latency sink options - critical for streaming
        // sync=false renders frames immediately without clock sync
        // max-buffers=1 drop=true ensures we always get the latest frame
        // wait-on-eos=false prevents blocking on end-of-stream
        let sink_opts = if config.low_latency {
            "max-buffers=1 drop=true sync=false wait-on-eos=false"
        } else {
            "max-buffers=2 drop=false sync=false wait-on-eos=false"
        };

        // Check if the hardware decoder is available
        let registry = gst::Registry::get();
        let hw_decoder_available = registry
            .find_feature(decoder, gst::ElementFactory::static_type())
            .is_some();

        #[cfg(target_os = "windows")]
        {
            if hw_decoder_available {
                // Windows D3D11 hardware decoder pipeline - ULTRA LOW LATENCY
                // d3d11h264dec outputs D3D11 textures, need d3d11download to copy to system memory
                //
                // Key optimizations:
                // - NO queue element (queues add latency for thread sync)
                // - is-live=true on appsrc for real-time behavior
                // - sync=false on appsink to render immediately
                // - videoconvert with n-threads for parallel color conversion
                info!("Using D3D11 hardware decoder: {}", decoder);
                Ok(format!(
                    "appsrc name=src is-live=true format=time do-timestamp=true max-buffers=1 \
                     ! {} \
                     ! {} \
                     ! d3d11download \
                     ! videoconvert n-threads=2 \
                     ! video/x-raw,format=NV12 \
                     ! appsink name=sink emit-signals=true {}",
                    parser, decoder, sink_opts
                ))
            } else {
                // Fallback to software decoder - still optimized for low latency
                let sw_decoder = config.codec.software_decoder();
                warn!(
                    "D3D11 decoder {} not available, falling back to software: {}",
                    decoder, sw_decoder
                );
                Ok(format!(
                    "appsrc name=src is-live=true format=time do-timestamp=true max-buffers=1 \
                     ! {} \
                     ! {} \
                     ! videoconvert n-threads=4 \
                     ! video/x-raw,format=NV12 \
                     ! appsink name=sink emit-signals=true {}",
                    parser, sw_decoder, sink_opts
                ))
            }
        }

        #[cfg(target_os = "linux")]
        {
            // Linux decoder priority (from best to fallback):
            // 1. V4L2 (Raspberry Pi, embedded devices with hardware codec)
            // 2. VA (newer va plugin - vah264dec/vah265dec/vaav1dec) for Intel/AMD
            // 3. VAAPI (legacy vaapi plugin - vaapih264dec/vaapih265dec)
            // 4. Software (avdec_h264/avdec_h265/av1dec)

            // Check for V4L2 decoder (Raspberry Pi - RPi5 supports AV1)
            let v4l2_decoder = match config.codec {
                GstCodec::H264 => "v4l2h264dec",
                GstCodec::H265 => "v4l2h265dec",
                GstCodec::AV1 => "v4l2av1dec",
            };
            let v4l2_available = registry
                .find_feature(v4l2_decoder, gst::ElementFactory::static_type())
                .is_some();

            // Check for new VA plugin decoders (preferred for desktop Linux)
            // Intel Arc, AMD RDNA2+, and modern Intel iGPUs support AV1
            let va_decoder = match config.codec {
                GstCodec::H264 => "vah264dec",
                GstCodec::H265 => "vah265dec",
                GstCodec::AV1 => "vaav1dec",
            };
            let va_available = registry
                .find_feature(va_decoder, gst::ElementFactory::static_type())
                .is_some();

            // Check for legacy VAAPI decoders (fallback for older systems)
            // Note: VAAPI AV1 uses same naming as VA plugin
            let vaapi_decoder = match config.codec {
                GstCodec::H264 => "vaapih264dec",
                GstCodec::H265 => "vaapih265dec",
                GstCodec::AV1 => "vaapiav1dec", // May not exist on all systems
            };
            let vaapi_available = registry
                .find_feature(vaapi_decoder, gst::ElementFactory::static_type())
                .is_some();

            if v4l2_available {
                // Raspberry Pi / embedded V4L2 hardware decoder - ULTRA LOW LATENCY
                // V4L2 decoders output directly to DMA buffers
                info!(
                    "Using V4L2 hardware decoder: {} (Raspberry Pi / embedded)",
                    v4l2_decoder
                );
                Ok(format!(
                    "appsrc name=src is-live=true format=time do-timestamp=true max-buffers=1 \
                     ! {} \
                     ! {} \
                     ! videoconvert n-threads=2 \
                     ! video/x-raw,format=NV12 \
                     ! appsink name=sink emit-signals=true {}",
                    parser, v4l2_decoder, sink_opts
                ))
            } else if va_available {
                // Modern VA plugin (Intel/AMD desktop Linux) - LOW LATENCY
                // va plugin is the newer, preferred method for VAAPI
                info!(
                    "Using VA hardware decoder: {} (Intel/AMD via va plugin)",
                    va_decoder
                );
                Ok(format!(
                    "appsrc name=src is-live=true format=time do-timestamp=true max-buffers=1 \
                     ! {} \
                     ! {} \
                     ! videoconvert n-threads=2 \
                     ! video/x-raw,format=NV12 \
                     ! appsink name=sink emit-signals=true {}",
                    parser, va_decoder, sink_opts
                ))
            } else if vaapi_available {
                // Legacy VAAPI plugin (older systems) - LOW LATENCY
                info!("Using legacy VAAPI hardware decoder: {}", vaapi_decoder);
                Ok(format!(
                    "appsrc name=src is-live=true format=time do-timestamp=true max-buffers=1 \
                     ! {} \
                     ! {} \
                     ! videoconvert n-threads=2 \
                     ! video/x-raw,format=NV12 \
                     ! appsink name=sink emit-signals=true {}",
                    parser, vaapi_decoder, sink_opts
                ))
            } else {
                // Fallback to software decoder
                let sw_decoder = config.codec.software_decoder();
                warn!(
                    "No hardware decoder available for {:?}, falling back to software: {}",
                    config.codec, sw_decoder
                );
                warn!("For hardware acceleration, install: libva (Intel/AMD) or enable V4L2 (Raspberry Pi)");
                Ok(format!(
                    "appsrc name=src is-live=true format=time do-timestamp=true max-buffers=1 \
                     ! {} \
                     ! {} \
                     ! videoconvert n-threads=4 \
                     ! video/x-raw,format=NV12 \
                     ! appsink name=sink emit-signals=true {}",
                    parser, sw_decoder, sink_opts
                ))
            }
        }
    }

    /// Decode a video frame
    pub fn decode(&mut self, nal_data: &[u8]) -> Result<Option<VideoFrame>> {
        if nal_data.is_empty() {
            return Ok(None);
        }

        // Create GStreamer buffer from NAL data
        let mut buffer = gst::Buffer::with_size(nal_data.len())
            .map_err(|e| anyhow!("Failed to create buffer: {}", e))?;

        {
            let buffer_ref = buffer.get_mut().unwrap();
            let mut map = buffer_ref
                .map_writable()
                .map_err(|e| anyhow!("Failed to map buffer: {}", e))?;
            map.copy_from_slice(nal_data);
        }

        // Push buffer to pipeline
        match self.appsrc.push_buffer(buffer) {
            Ok(_) => {}
            Err(e) => {
                warn!("Failed to push buffer: {:?}", e);
                return Ok(None);
            }
        }

        self.frame_count += 1;

        // Check for decoded frame
        let frame = self.last_frame.lock().unwrap().take();

        if let Some(decoded) = frame {
            debug!(
                "Decoded frame {}: {}x{}",
                self.frame_count, decoded.width, decoded.height
            );

            // Log when transfer function changes (HDR/SDR switch)
            if decoded.transfer_function != self.last_logged_transfer {
                log::info!(
                    "Transfer function changed: {:?} -> {:?} ({:?} {:?})",
                    self.last_logged_transfer,
                    decoded.transfer_function,
                    decoded.color_space,
                    decoded.color_range
                );
                self.last_logged_transfer = decoded.transfer_function;
            }

            Ok(Some(VideoFrame {
                frame_id: super::next_frame_id(),
                width: decoded.width,
                height: decoded.height,
                y_plane: decoded.y_plane,
                u_plane: decoded.uv_plane,
                v_plane: Vec::new(), // NV12 has interleaved UV in u_plane
                y_stride: decoded.y_stride,
                u_stride: decoded.uv_stride,
                v_stride: 0,
                timestamp_us: 0,
                format: PixelFormat::NV12,
                color_range: decoded.color_range,
                color_space: decoded.color_space,
                transfer_function: decoded.transfer_function,
                gpu_frame: None,
            }))
        } else {
            Ok(None)
        }
    }

    /// Get frame count
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }
}

impl Drop for GStreamerDecoder {
    fn drop(&mut self) {
        info!("Stopping GStreamer pipeline");
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

/// Check if GStreamer hardware decoding is available
#[cfg(target_os = "windows")]
pub fn is_gstreamer_available() -> bool {
    // Initialize GStreamer (with bundled DLL support)
    if init_gstreamer().is_err() {
        return false;
    }

    // Check for D3D11 hardware decoders
    let registry = gst::Registry::get();
    let d3d11_h264 = registry
        .find_feature("d3d11h264dec", gst::ElementFactory::static_type())
        .is_some();
    let d3d11_h265 = registry
        .find_feature("d3d11h265dec", gst::ElementFactory::static_type())
        .is_some();
    let d3d11_av1 = registry
        .find_feature("d3d11av1dec", gst::ElementFactory::static_type())
        .is_some();
    let avdec_h264 = registry
        .find_feature("avdec_h264", gst::ElementFactory::static_type())
        .is_some();
    let av1dec = registry
        .find_feature("av1dec", gst::ElementFactory::static_type())
        .is_some();

    if d3d11_h264 || d3d11_h265 || d3d11_av1 {
        info!(
            "GStreamer D3D11 decoders available: H.264={}, H.265={}, AV1={}",
            d3d11_h264, d3d11_h265, d3d11_av1
        );
        true
    } else if avdec_h264 || av1dec {
        info!(
            "GStreamer software decoders available: H.264={}, AV1={}",
            avdec_h264, av1dec
        );
        true
    } else {
        debug!("GStreamer decoders not available");
        false
    }
}

/// Check if GStreamer V4L2 decoding is available (Linux - Raspberry Pi)
#[cfg(target_os = "linux")]
pub fn is_gstreamer_v4l2_available() -> bool {
    // Initialize GStreamer if needed
    if init_gstreamer().is_err() {
        return false;
    }

    // Check for V4L2 decoders (RPi5 supports AV1)
    let registry = gst::Registry::get();
    let h264_available = registry
        .find_feature("v4l2h264dec", gst::ElementFactory::static_type())
        .is_some();
    let h265_available = registry
        .find_feature("v4l2h265dec", gst::ElementFactory::static_type())
        .is_some();
    let av1_available = registry
        .find_feature("v4l2av1dec", gst::ElementFactory::static_type())
        .is_some();

    if h264_available || h265_available || av1_available {
        info!(
            "GStreamer V4L2 decoders available: H.264={}, H.265={}, AV1={}",
            h264_available, h265_available, av1_available
        );
        true
    } else {
        debug!("GStreamer V4L2 decoders not available");
        false
    }
}

/// Check if GStreamer VA (VAAPI) decoding is available (Linux - Intel/AMD)
#[cfg(target_os = "linux")]
pub fn is_gstreamer_va_available() -> bool {
    // Initialize GStreamer if needed
    if init_gstreamer().is_err() {
        return false;
    }

    let registry = gst::Registry::get();

    // Check new VA plugin (preferred) - Intel Arc/AMD RDNA2+ support AV1
    let va_h264 = registry
        .find_feature("vah264dec", gst::ElementFactory::static_type())
        .is_some();
    let va_h265 = registry
        .find_feature("vah265dec", gst::ElementFactory::static_type())
        .is_some();
    let va_av1 = registry
        .find_feature("vaav1dec", gst::ElementFactory::static_type())
        .is_some();

    // Check legacy VAAPI plugin (fallback)
    let vaapi_h264 = registry
        .find_feature("vaapih264dec", gst::ElementFactory::static_type())
        .is_some();
    let vaapi_h265 = registry
        .find_feature("vaapih265dec", gst::ElementFactory::static_type())
        .is_some();

    if va_h264 || va_h265 || va_av1 {
        info!(
            "GStreamer VA decoders available: H.264={}, H.265={}, AV1={}",
            va_h264, va_h265, va_av1
        );
        true
    } else if vaapi_h264 || vaapi_h265 {
        info!(
            "GStreamer legacy VAAPI decoders available: H.264={}, H.265={}",
            vaapi_h264, vaapi_h265
        );
        true
    } else {
        debug!("GStreamer VA/VAAPI decoders not available");
        false
    }
}

/// Check if any GStreamer hardware decoding is available (Linux)
#[cfg(target_os = "linux")]
pub fn is_gstreamer_available() -> bool {
    // Initialize GStreamer if needed
    if init_gstreamer().is_err() {
        return false;
    }

    let registry = gst::Registry::get();

    // Check all available decoders (H.264, H.265, AV1)
    let v4l2_h264 = registry
        .find_feature("v4l2h264dec", gst::ElementFactory::static_type())
        .is_some();
    let v4l2_h265 = registry
        .find_feature("v4l2h265dec", gst::ElementFactory::static_type())
        .is_some();
    let v4l2_av1 = registry
        .find_feature("v4l2av1dec", gst::ElementFactory::static_type())
        .is_some();
    let va_h264 = registry
        .find_feature("vah264dec", gst::ElementFactory::static_type())
        .is_some();
    let va_h265 = registry
        .find_feature("vah265dec", gst::ElementFactory::static_type())
        .is_some();
    let va_av1 = registry
        .find_feature("vaav1dec", gst::ElementFactory::static_type())
        .is_some();
    let vaapi_h264 = registry
        .find_feature("vaapih264dec", gst::ElementFactory::static_type())
        .is_some();
    let vaapi_h265 = registry
        .find_feature("vaapih265dec", gst::ElementFactory::static_type())
        .is_some();
    let avdec_h264 = registry
        .find_feature("avdec_h264", gst::ElementFactory::static_type())
        .is_some();
    let avdec_h265 = registry
        .find_feature("avdec_h265", gst::ElementFactory::static_type())
        .is_some();
    let av1dec = registry
        .find_feature("av1dec", gst::ElementFactory::static_type())
        .is_some();

    // Log available decoders
    info!("GStreamer Linux decoders:");
    info!(
        "  V4L2 (Raspberry Pi): H.264={}, H.265={}, AV1={}",
        v4l2_h264, v4l2_h265, v4l2_av1
    );
    info!(
        "  VA (Intel/AMD): H.264={}, H.265={}, AV1={}",
        va_h264, va_h265, va_av1
    );
    info!(
        "  VAAPI (legacy): H.264={}, H.265={}",
        vaapi_h264, vaapi_h265
    );
    info!(
        "  Software: H.264={}, H.265={}, AV1={}",
        avdec_h264, avdec_h265, av1dec
    );

    // Return true if any decoder is available
    v4l2_h264
        || v4l2_h265
        || v4l2_av1
        || va_h264
        || va_h265
        || va_av1
        || vaapi_h264
        || vaapi_h265
        || avdec_h264
        || avdec_h265
        || av1dec
}

/// Check if running on Raspberry Pi
#[cfg(target_os = "linux")]
pub fn is_raspberry_pi() -> bool {
    if let Ok(model) = std::fs::read_to_string("/proc/device-tree/model") {
        model.contains("Raspberry Pi")
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codec_caps() {
        assert!(GstCodec::H264.caps_string().contains("h264"));
        assert!(GstCodec::H265.caps_string().contains("h265"));
        assert!(GstCodec::AV1.caps_string().contains("av1"));
    }

    #[test]
    fn test_default_config() {
        let config = GstDecoderConfig::default();
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.codec, GstCodec::H264);
        assert!(config.low_latency);
    }

    #[test]
    fn test_parser_elements() {
        assert_eq!(GstCodec::H264.parser_element(), "h264parse");
        assert_eq!(GstCodec::H265.parser_element(), "h265parse");
        assert_eq!(GstCodec::AV1.parser_element(), "av1parse");
    }

    #[test]
    fn test_software_decoders() {
        assert_eq!(GstCodec::H264.software_decoder(), "avdec_h264");
        assert_eq!(GstCodec::H265.software_decoder(), "avdec_h265");
        assert_eq!(GstCodec::AV1.software_decoder(), "av1dec");
    }
}
