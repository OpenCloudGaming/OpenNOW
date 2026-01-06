//! GStreamer Video Decoder for Linux
//!
//! Hardware-accelerated video decoding using GStreamer with V4L2.
//! Primarily for Raspberry Pi and other embedded ARM devices.
//!
//! Supported hardware:
//! - Raspberry Pi 4: H.264 via bcm2835-codec
//! - Raspberry Pi 5: H.264/HEVC via rpivid
//! - Other V4L2 M2M devices
//!
//! Pipeline: appsrc -> h264parse -> v4l2h264dec -> appsink

use anyhow::{anyhow, Result};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app::{AppSink, AppSrc};
use gstreamer_video as gst_video;
use log::{debug, info, warn};
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use super::{
    ColorRange, ColorSpace, PixelFormat, TransferFunction, VAAPISurfaceWrapper, VideoFrame,
};

/// GStreamer codec type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GstCodec {
    H264,
    H265,
}

impl GstCodec {
    fn caps_string(&self) -> &'static str {
        match self {
            GstCodec::H264 => "video/x-h264,stream-format=byte-stream,alignment=au",
            GstCodec::H265 => "video/x-h265,stream-format=byte-stream,alignment=au",
        }
    }

    fn parser_element(&self) -> &'static str {
        match self {
            GstCodec::H264 => "h264parse",
            GstCodec::H265 => "h265parse",
        }
    }

    fn v4l2_decoder(&self) -> &'static str {
        match self {
            GstCodec::H264 => "v4l2h264dec",
            GstCodec::H265 => "v4l2h265dec",
        }
    }
}

/// GStreamer decoder configuration
#[derive(Debug, Clone)]
pub struct GstDecoderConfig {
    pub codec: GstCodec,
    pub width: u32,
    pub height: u32,
}

impl Default for GstDecoderConfig {
    fn default() -> Self {
        Self {
            codec: GstCodec::H264,
            width: 1920,
            height: 1080,
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
}

/// GStreamer Video Decoder
///
/// Uses V4L2 hardware decoding on Raspberry Pi and other embedded devices.
pub struct GStreamerDecoder {
    pipeline: gst::Pipeline,
    appsrc: AppSrc,
    appsink: AppSink,
    config: GstDecoderConfig,
    frame_count: u64,
    last_frame: Arc<Mutex<Option<DecodedFrame>>>,
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

        // Initialize GStreamer
        gst::init().map_err(|e| anyhow!("Failed to initialize GStreamer: {}", e))?;

        // Build pipeline string
        // appsrc -> parser -> v4l2dec -> videoconvert -> appsink
        let pipeline_str = format!(
            "appsrc name=src is-live=true format=time do-timestamp=true ! {} ! {} ! videoconvert ! video/x-raw,format=NV12 ! appsink name=sink emit-signals=true sync=false",
            config.codec.parser_element(),
            config.codec.v4l2_decoder()
        );

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

        // Configure appsrc
        let caps = gst::Caps::from_str(config.codec.caps_string())
            .map_err(|e| anyhow!("Failed to create caps: {}", e))?;
        appsrc.set_caps(Some(&caps));
        appsrc.set_format(gst::Format::Time);

        // Get appsink
        let appsink = pipeline
            .by_name("sink")
            .ok_or_else(|| anyhow!("Failed to get appsink"))?
            .downcast::<AppSink>()
            .map_err(|_| anyhow!("Failed to downcast to AppSink"))?;

        // Configure appsink for NV12 output
        let sink_caps = gst::Caps::builder("video/x-raw")
            .field("format", "NV12")
            .build();
        appsink.set_caps(Some(&sink_caps));

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
        })
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

            Ok(Some(VideoFrame {
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
                color_range: ColorRange::Limited,
                color_space: ColorSpace::BT709,
                transfer_function: TransferFunction::SDR,
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

/// Check if GStreamer V4L2 decoding is available
pub fn is_gstreamer_v4l2_available() -> bool {
    // Initialize GStreamer if needed
    if gst::init().is_err() {
        return false;
    }

    // Check for V4L2 H.264 decoder
    let registry = gst::Registry::get();
    let h264_available = registry
        .find_feature("v4l2h264dec", gst::ElementFactory::static_type())
        .is_some();
    let h265_available = registry
        .find_feature("v4l2h265dec", gst::ElementFactory::static_type())
        .is_some();

    if h264_available || h265_available {
        info!(
            "GStreamer V4L2 decoders available: H.264={}, H.265={}",
            h264_available, h265_available
        );
        true
    } else {
        debug!("GStreamer V4L2 decoders not available");
        false
    }
}

/// Check if running on Raspberry Pi
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
    }

    #[test]
    fn test_default_config() {
        let config = GstDecoderConfig::default();
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.codec, GstCodec::H264);
    }
}
