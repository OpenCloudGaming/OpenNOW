use std::ptr;
use std::sync::{Arc, OnceLock};

use ffmpeg::codec;
use ffmpeg::ffi;
use ffmpeg::format::Pixel;
use ffmpeg::frame;
use ffmpeg::software::scaling::{context::Context as Scaler, flag::Flags as ScaleFlags};
use ffmpeg_next as ffmpeg;

use crate::{
    ChromaLocation, ColorMatrix, ColorRange, DecodedVideoFrame, DmaBufFrame, DmaBufLayer,
    DmaBufObject, DmaBufPlane, EncodedVideoFrame, Error, FramePlane, PixelFormat, Result,
    StreamFormat, Subsystem, VideoCodec, VulkanImage, VulkanVideoFrame,
};

use super::VideoDecoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FfmpegMode {
    Vulkan,
    Cuda,
    Software,
}

impl FfmpegMode {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Vulkan => "FFmpeg Vulkan Video",
            Self::Cuda => "FFmpeg CUDA/NVDEC",
            Self::Software => "FFmpeg software",
        }
    }

    const fn device_type(self) -> Option<ffi::AVHWDeviceType> {
        match self {
            Self::Vulkan => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN),
            Self::Cuda => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA),
            Self::Software => None,
        }
    }
}

pub(crate) struct FfmpegDecoder {
    // The decoder must be dropped before `wanted_hw_format`; libavcodec may
    // consult the callback opaque pointer during decoder teardown.
    decoder: ffmpeg::decoder::Video,
    wanted_hw_format: Option<Box<HardwareFormatSelection>>,
    scaler: Option<Scaler>,
    codec: VideoCodec,
    mode: FfmpegMode,
    configured_format: StreamFormat,
    pending_format_change: Option<StreamFormat>,
    last_timestamp_us: u64,
    zero_copy_active: bool,
    zero_copy_unavailable_reported: bool,
}

struct HardwareFormatSelection {
    pixel_format: ffi::AVPixelFormat,
    exportable_vulkan_frames: bool,
}

impl FfmpegDecoder {
    pub(crate) fn open(codec: VideoCodec, format: StreamFormat, mode: FfmpegMode) -> Result<Self> {
        initialize_ffmpeg()?;
        let decoder_definition = if mode == FfmpegMode::Software {
            ffmpeg::decoder::find(codec_id(codec))
        } else {
            ffmpeg::decoder::find_by_name(native_decoder_name(codec))
        }
        .ok_or_else(|| {
            Error::unavailable(
                Subsystem::Ffmpeg,
                format!("FFmpeg was built without the {} decoder", codec.label()),
            )
        })?;
        let mut context = codec::Context::new_with_codec(decoder_definition);
        unsafe {
            let raw = context.as_mut_ptr();
            (*raw).width = format.width as i32;
            (*raw).height = format.height as i32;
            (*raw).thread_count = if mode == FfmpegMode::Software { 0 } else { 1 };
        }

        let mut wanted_hw_format = None;
        if let Some(device_type) = mode.device_type() {
            let pixel_format =
                hardware_pixel_format(decoder_definition, device_type).ok_or_else(|| {
                    Error::unavailable(
                        Subsystem::Ffmpeg,
                        format!(
                            "{} does not expose {} decode through libavcodec",
                            mode.label(),
                            codec.label()
                        ),
                    )
                })?;
            let mut device = ptr::null_mut();
            let mut options = ptr::null_mut();
            if mode == FfmpegMode::Vulkan {
                unsafe {
                    ffi::av_dict_set(
                        &mut options,
                        c"instance_extensions".as_ptr(),
                        c"VK_KHR_surface+VK_KHR_xlib_surface+VK_KHR_wayland_surface".as_ptr(),
                        0,
                    );
                    ffi::av_dict_set(
                        &mut options,
                        c"device_extensions".as_ptr(),
                        c"VK_KHR_swapchain".as_ptr(),
                        0,
                    );
                }
            }
            let create_result = unsafe {
                ffi::av_hwdevice_ctx_create(&mut device, device_type, ptr::null(), options, 0)
            };
            unsafe { ffi::av_dict_free(&mut options) };
            if create_result < 0 || device.is_null() {
                return Err(ffmpeg_error(
                    format!("failed to create the {} device", mode.label()),
                    create_result,
                ));
            }
            let mut selected = Box::new(HardwareFormatSelection {
                pixel_format,
                exportable_vulkan_frames: false,
            });
            unsafe {
                let raw = context.as_mut_ptr();
                (*raw).hw_device_ctx = ffi::av_buffer_ref(device);
                (*raw).get_format = Some(select_hardware_format);
                (*raw).opaque = (&mut *selected as *mut HardwareFormatSelection).cast();
                ffi::av_buffer_unref(&mut device);
                if (*raw).hw_device_ctx.is_null() {
                    return Err(Error::backend(
                        Subsystem::Ffmpeg,
                        format!("failed to retain the {} device", mode.label()),
                    ));
                }
            }
            wanted_hw_format = Some(selected);
        }

        let decoder = context
            .decoder()
            .open_as(decoder_definition)
            .and_then(|opened| opened.video())
            .map_err(|error| {
                Error::backend(
                    Subsystem::Ffmpeg,
                    format!(
                        "{} {} decoder initialization failed: {error}",
                        mode.label(),
                        codec.label()
                    ),
                )
            })?;
        Ok(Self {
            decoder,
            wanted_hw_format,
            scaler: None,
            codec,
            mode,
            configured_format: format,
            pending_format_change: None,
            last_timestamp_us: 0,
            zero_copy_active: false,
            zero_copy_unavailable_reported: false,
        })
    }

    pub(crate) fn probe(
        codec: VideoCodec,
        mode: FfmpegMode,
    ) -> std::result::Result<String, String> {
        let format = StreamFormat::video_default(1920, 1080).map_err(|error| error.to_string())?;
        let decoder = Self::open(codec, format, mode).map_err(|error| error.to_string())?;
        Ok(format!(
            "{} {} via libavcodec {}",
            decoder.mode.label(),
            decoder.codec.label(),
            ffmpeg::codec::version()
        ))
    }

    fn drain(&mut self, draining: bool) -> Result<Vec<DecodedVideoFrame>> {
        let mut frames = Vec::new();
        loop {
            let mut decoded = frame::Video::empty();
            match self.decoder.receive_frame(&mut decoded) {
                Ok(()) => frames.push(self.convert_frame(&decoded)?),
                Err(ffmpeg::Error::Other { errno }) if errno == ffmpeg::error::EAGAIN => break,
                Err(ffmpeg::Error::Eof) if draining => break,
                Err(error) => {
                    return Err(Error::backend(
                        Subsystem::Ffmpeg,
                        format!(
                            "{} {} frame receive failed: {error}",
                            self.mode.label(),
                            self.codec.label()
                        ),
                    ));
                }
            }
        }
        Ok(frames)
    }

    fn convert_frame(&mut self, decoded: &frame::Video) -> Result<DecodedVideoFrame> {
        if self.mode == FfmpegMode::Vulkan
            && self
                .wanted_hw_format
                .as_deref()
                .is_some_and(|selection| Pixel::from(selection.pixel_format) == decoded.format())
        {
            if let Ok(frame) = map_vulkan_frame_direct(decoded, self.last_timestamp_us) {
                if !self.zero_copy_active {
                    eprintln!("Vulkan Video same-device zero-copy enabled");
                    self.zero_copy_active = true;
                }
                return Ok(frame);
            }
            match map_vulkan_frame_to_dmabuf(decoded, self.last_timestamp_us) {
                Ok(frame) => {
                    if !self.zero_copy_active {
                        eprintln!("Vulkan Video zero-copy enabled through DRM PRIME/DMA-BUF");
                        self.zero_copy_active = true;
                    }
                    return Ok(frame);
                }
                Err(error) => {
                    if !self.zero_copy_unavailable_reported {
                        eprintln!(
                            "Vulkan Video DMA-BUF export unavailable; using bounded CPU transfer: {error}"
                        );
                        self.zero_copy_unavailable_reported = true;
                    }
                }
            }
        }
        let software_frame;
        let source = if self
            .wanted_hw_format
            .as_deref()
            .is_some_and(|selection| Pixel::from(selection.pixel_format) == decoded.format())
        {
            let mut transferred = frame::Video::empty();
            let transfer_result = unsafe {
                ffi::av_hwframe_transfer_data(transferred.as_mut_ptr(), decoded.as_ptr(), 0)
            };
            if transfer_result < 0 {
                return Err(ffmpeg_error(
                    format!("{} frame download failed", self.mode.label()),
                    transfer_result,
                ));
            }
            unsafe {
                ffi::av_frame_copy_props(transferred.as_mut_ptr(), decoded.as_ptr());
            }
            software_frame = transferred;
            &software_frame
        } else {
            decoded
        };

        let width = source.width().max(1);
        let height = source.height().max(1);
        // NVDEC and Vulkan Video normally download NV12 already. Avoid a full
        // swscale pass in that hot path; conversion is only needed for formats
        // such as software-decoded YUV420P or 10-bit hardware output.
        let converted;
        let nv12 = if source.format() == Pixel::NV12 {
            source
        } else {
            let scaler_changed = self.scaler.as_ref().is_none_or(|scaler| {
                scaler.input().format != source.format()
                    || scaler.input().width != width
                    || scaler.input().height != height
            });
            if scaler_changed {
                self.scaler = Some(
                    Scaler::get(
                        source.format(),
                        width,
                        height,
                        Pixel::NV12,
                        width,
                        height,
                        ScaleFlags::BILINEAR,
                    )
                    .map_err(|error| {
                        Error::backend(
                            Subsystem::Ffmpeg,
                            format!("NV12 conversion setup failed: {error}"),
                        )
                    })?,
                );
            }
            let mut frame = frame::Video::empty();
            self.scaler
                .as_mut()
                .expect("scaler was initialized")
                .run(source, &mut frame)
                .map_err(|error| {
                    Error::backend(
                        Subsystem::Ffmpeg,
                        format!("NV12 conversion failed: {error}"),
                    )
                })?;
            converted = frame;
            &converted
        };

        let output_format = StreamFormat {
            width,
            height,
            pixel_format: PixelFormat::Nv12,
            color_range: map_color_range(source),
            color_matrix: map_color_matrix(source),
            chroma_location: map_chroma_location(source),
        };
        output_format.validate()?;
        if output_format != self.configured_format {
            self.configured_format = output_format;
            self.pending_format_change = Some(output_format);
        }
        let timestamp_us = decoded
            .pts()
            .and_then(|timestamp| u64::try_from(timestamp).ok())
            .unwrap_or(self.last_timestamp_us);
        Ok(DecodedVideoFrame {
            format: output_format,
            planes: vec![
                FramePlane {
                    data: Arc::from(nv12.data(0).to_vec()),
                    stride: nv12.stride(0),
                    rows: height as usize,
                },
                FramePlane {
                    data: Arc::from(nv12.data(1).to_vec()),
                    stride: nv12.stride(1),
                    rows: height as usize / 2,
                },
            ],
            dmabuf: None,
            vulkan: None,
            overlay: None,
            timestamp_us,
        })
    }
}

fn map_vulkan_frame_direct(
    decoded: &frame::Video,
    fallback_timestamp_us: u64,
) -> Result<DecodedVideoFrame> {
    let (vulkan_frame, vulkan_frames, device_context, vulkan_device) = unsafe {
        let raw = decoded.as_ptr();
        let vulkan_frame = (*raw).data[0].cast::<ffi::AVVkFrame>();
        let frames_ref = (*raw).hw_frames_ctx;
        if vulkan_frame.is_null() || frames_ref.is_null() || (*frames_ref).data.is_null() {
            return Err(Error::backend(
                Subsystem::Ffmpeg,
                "Vulkan Video frame has no hardware context",
            ));
        }
        let frames_context = (*frames_ref).data.cast::<ffi::AVHWFramesContext>();
        let vulkan_frames = (*frames_context).hwctx.cast::<ffi::AVVulkanFramesContext>();
        let device_context = (*frames_context).device_ctx;
        if vulkan_frames.is_null() || device_context.is_null() {
            return Err(Error::backend(
                Subsystem::Ffmpeg,
                "Vulkan Video frame has incomplete hardware metadata",
            ));
        }
        let vulkan_device = (*device_context).hwctx.cast::<ffi::AVVulkanDeviceContext>();
        if vulkan_device.is_null() {
            return Err(Error::backend(
                Subsystem::Ffmpeg,
                "Vulkan Video frame has no Vulkan device",
            ));
        }
        (vulkan_frame, vulkan_frames, device_context, vulkan_device)
    };
    let image_count = unsafe {
        (*vulkan_frame)
            .img
            .iter()
            .position(|image| *image == 0)
            .unwrap_or((*vulkan_frame).img.len())
    };
    if image_count == 0 || image_count > 2 {
        return Err(Error::unavailable(
            Subsystem::Ffmpeg,
            format!("unsupported Vulkan Video image count {image_count}"),
        ));
    }
    let width = decoded.width().max(1);
    let height = decoded.height().max(1);
    let mut images = Vec::with_capacity(image_count);
    for index in 0..image_count {
        let (plane_width, plane_height) = if index == 0 || image_count == 1 {
            (width, height)
        } else {
            (width / 2, height / 2)
        };
        images.push(VulkanImage {
            image: unsafe { (*vulkan_frame).img[index] },
            format: unsafe { (*vulkan_frames).format[index] },
            width: plane_width,
            height: plane_height,
            layout: unsafe { (*vulkan_frame).layout[index] },
            access: unsafe { (*vulkan_frame).access[index] },
            semaphore: unsafe { (*vulkan_frame).sem[index] },
            semaphore_value: unsafe { (*vulkan_frame).sem_value[index] },
            queue_family: unsafe { (*vulkan_frame).queue_family[index] },
        });
    }
    let queue_count = unsafe { (*vulkan_device).nb_qf.max(0) as usize };
    let queue_families =
        unsafe { &(&(*vulkan_device).qf)[..queue_count.min((*vulkan_device).qf.len())] }
            .iter()
            .filter_map(|family| u32::try_from(family.idx).ok())
            .collect::<Vec<_>>();
    let mut retained = frame::Video::empty();
    if unsafe { ffi::av_frame_ref(retained.as_mut_ptr(), decoded.as_ptr()) } < 0 {
        return Err(Error::backend(
            Subsystem::Ffmpeg,
            "failed to retain Vulkan Video frame for presentation",
        ));
    }
    let device = unsafe {
        VulkanVideoFrame::new(
            (*vulkan_device).inst as usize,
            (*vulkan_device).phys_dev as usize,
            (*vulkan_device).act_dev as usize,
            queue_families,
            (*vulkan_frames).usage as u32,
            (*vulkan_frames).img_flags,
            images,
            device_context as usize,
            (*vulkan_device).lock_queue.map(|callback| {
                std::mem::transmute::<
                    unsafe extern "C" fn(*mut ffi::AVHWDeviceContext, u32, u32),
                    unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32),
                >(callback)
            }),
            (*vulkan_device).unlock_queue.map(|callback| {
                std::mem::transmute::<
                    unsafe extern "C" fn(*mut ffi::AVHWDeviceContext, u32, u32),
                    unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32),
                >(callback)
            }),
            Arc::new(retained),
        )
    };
    let output_format = StreamFormat {
        width,
        height,
        pixel_format: PixelFormat::Nv12,
        color_range: map_color_range(decoded),
        color_matrix: map_color_matrix(decoded),
        chroma_location: map_chroma_location(decoded),
    };
    let timestamp_us = decoded
        .pts()
        .and_then(|timestamp| u64::try_from(timestamp).ok())
        .unwrap_or(fallback_timestamp_us);
    let output = DecodedVideoFrame {
        format: output_format,
        planes: Vec::new(),
        dmabuf: None,
        vulkan: Some(Arc::new(device)),
        overlay: None,
        timestamp_us,
    };
    output.validate()?;
    Ok(output)
}

fn map_vulkan_frame_to_dmabuf(
    decoded: &frame::Video,
    fallback_timestamp_us: u64,
) -> Result<DecodedVideoFrame> {
    let mut mapped = frame::Video::empty();
    mapped.set_format(Pixel::DRM_PRIME);
    let map_result = unsafe {
        ffi::av_hwframe_map(
            mapped.as_mut_ptr(),
            decoded.as_ptr(),
            ffi::AV_HWFRAME_MAP_READ as i32,
        )
    };
    if map_result < 0 {
        return Err(ffmpeg_error(
            "Vulkan Video frame DMA-BUF export failed".to_owned(),
            map_result,
        ));
    }
    let descriptor = unsafe {
        let data = (*mapped.as_ptr()).data[0];
        if data.is_null() {
            return Err(Error::backend(
                Subsystem::Ffmpeg,
                "DMA-BUF mapping returned no DRM descriptor",
            ));
        }
        &*data.cast::<ffi::AVDRMFrameDescriptor>()
    };
    let object_count = usize::try_from(descriptor.nb_objects).unwrap_or(usize::MAX);
    let layer_count = usize::try_from(descriptor.nb_layers).unwrap_or(usize::MAX);
    if object_count == 0
        || object_count > descriptor.objects.len()
        || layer_count == 0
        || layer_count > descriptor.layers.len()
    {
        return Err(Error::backend(
            Subsystem::Ffmpeg,
            "DMA-BUF mapping returned invalid object/layer counts",
        ));
    }
    let objects = descriptor.objects[..object_count]
        .iter()
        .map(|object| DmaBufObject {
            fd: object.fd,
            size: object.size,
            format_modifier: object.format_modifier,
        })
        .collect();
    let mut layers = Vec::with_capacity(layer_count);
    for layer in &descriptor.layers[..layer_count] {
        let plane_count = usize::try_from(layer.nb_planes).unwrap_or(usize::MAX);
        if plane_count == 0 || plane_count > layer.planes.len() {
            return Err(Error::backend(
                Subsystem::Ffmpeg,
                "DMA-BUF mapping returned an invalid plane count",
            ));
        }
        let planes = layer.planes[..plane_count]
            .iter()
            .map(|plane| {
                Ok(DmaBufPlane {
                    object_index: usize::try_from(plane.object_index).map_err(|_| {
                        Error::backend(Subsystem::Ffmpeg, "negative DMA-BUF object index")
                    })?,
                    offset: usize::try_from(plane.offset).map_err(|_| {
                        Error::backend(Subsystem::Ffmpeg, "negative DMA-BUF plane offset")
                    })?,
                    pitch: usize::try_from(plane.pitch).map_err(|_| {
                        Error::backend(Subsystem::Ffmpeg, "negative DMA-BUF plane pitch")
                    })?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        layers.push(DmaBufLayer {
            format: layer.format,
            planes,
        });
    }
    let width = decoded.width().max(1);
    let height = decoded.height().max(1);
    let output_format = StreamFormat {
        width,
        height,
        pixel_format: PixelFormat::Nv12,
        color_range: map_color_range(decoded),
        color_matrix: map_color_matrix(decoded),
        chroma_location: map_chroma_location(decoded),
    };
    let timestamp_us = decoded
        .pts()
        .and_then(|timestamp| u64::try_from(timestamp).ok())
        .unwrap_or(fallback_timestamp_us);
    let dmabuf = Arc::new(DmaBufFrame::new(objects, layers, Arc::new(mapped)));
    let frame = DecodedVideoFrame {
        format: output_format,
        planes: Vec::new(),
        dmabuf: Some(dmabuf),
        vulkan: None,
        overlay: None,
        timestamp_us,
    };
    frame.validate()?;
    Ok(frame)
}

impl VideoDecoder for FfmpegDecoder {
    fn decode(&mut self, frame: &EncodedVideoFrame) -> Result<Vec<DecodedVideoFrame>> {
        self.last_timestamp_us = frame.timestamp_us;
        let mut packet = ffmpeg::Packet::copy(&frame.data);
        packet.set_pts(i64::try_from(frame.timestamp_us).ok());
        packet.set_dts(i64::try_from(frame.timestamp_us).ok());
        if frame.keyframe {
            packet.set_flags(ffmpeg::packet::Flags::KEY);
        }
        self.decoder.send_packet(&packet).map_err(|error| {
            Error::backend(
                Subsystem::Ffmpeg,
                format!(
                    "{} {} packet submission failed: {error}",
                    self.mode.label(),
                    self.codec.label()
                ),
            )
        })?;
        self.drain(false)
    }

    fn flush(&mut self) -> Result<Vec<DecodedVideoFrame>> {
        match self.decoder.send_eof() {
            Ok(()) | Err(ffmpeg::Error::Eof) => self.drain(true),
            Err(error) => Err(Error::backend(
                Subsystem::Ffmpeg,
                format!("{} decoder flush failed: {error}", self.mode.label()),
            )),
        }
    }

    fn take_format_change(&mut self) -> Option<StreamFormat> {
        self.pending_format_change.take()
    }
}

fn initialize_ffmpeg() -> Result<()> {
    static INITIALIZED: OnceLock<std::result::Result<(), String>> = OnceLock::new();
    INITIALIZED
        .get_or_init(|| ffmpeg::init().map_err(|error| error.to_string()))
        .clone()
        .map_err(|reason| Error::unavailable(Subsystem::Ffmpeg, reason))
}

fn codec_id(codec: VideoCodec) -> codec::Id {
    match codec {
        VideoCodec::H264 => codec::Id::H264,
        VideoCodec::H265 => codec::Id::HEVC,
        VideoCodec::Av1 => codec::Id::AV1,
    }
}

fn native_decoder_name(codec: VideoCodec) -> &'static str {
    match codec {
        VideoCodec::H264 => "h264",
        VideoCodec::H265 => "hevc",
        VideoCodec::Av1 => "av1",
    }
}

fn hardware_pixel_format(
    codec: ffmpeg::Codec,
    device_type: ffi::AVHWDeviceType,
) -> Option<ffi::AVPixelFormat> {
    let mut index = 0;
    loop {
        let config = unsafe { ffi::avcodec_get_hw_config(codec.as_ptr(), index) };
        if config.is_null() {
            return None;
        }
        let matches = unsafe {
            (*config).device_type == device_type
                && ((*config).methods as u32 & ffi::AV_CODEC_HW_CONFIG_METHOD_HW_DEVICE_CTX as u32)
                    != 0
        };
        if matches {
            return Some(unsafe { (*config).pix_fmt });
        }
        index += 1;
    }
}

unsafe extern "C" fn select_hardware_format(
    context: *mut ffi::AVCodecContext,
    formats: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    if context.is_null() || formats.is_null() || unsafe { (*context).opaque.is_null() } {
        return ffi::AVPixelFormat::AV_PIX_FMT_NONE;
    }
    let selection = unsafe { &*((*context).opaque as *const HardwareFormatSelection) };
    let wanted = selection.pixel_format;
    let mut current = formats;
    while unsafe { *current } != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
        if unsafe { *current } == wanted {
            if selection.exportable_vulkan_frames
                && !unsafe { configure_exportable_vulkan_frames(context, wanted) }
            {
                return ffi::AVPixelFormat::AV_PIX_FMT_NONE;
            }
            return wanted;
        }
        current = unsafe { current.add(1) };
    }
    ffi::AVPixelFormat::AV_PIX_FMT_NONE
}

/// Replaces libavcodec's implicit optimal-tiled Vulkan pool with a
/// DRM-modifier pool. This has to happen inside `get_format`; FFmpeg resets
/// `hw_frames_ctx` immediately before invoking the callback.
unsafe fn configure_exportable_vulkan_frames(
    context: *mut ffi::AVCodecContext,
    pixel_format: ffi::AVPixelFormat,
) -> bool {
    let device = unsafe { (*context).hw_device_ctx };
    if device.is_null() {
        return false;
    }
    let mut frames = ptr::null_mut();
    let result = unsafe {
        ffi::avcodec_get_hw_frames_parameters(context, device, pixel_format, &mut frames)
    };
    if result < 0 || frames.is_null() {
        return false;
    }
    let configured = (|| {
        let frames_context = unsafe { (*frames).data.cast::<ffi::AVHWFramesContext>() };
        if frames_context.is_null() {
            return false;
        }
        let vulkan = unsafe { (*frames_context).hwctx.cast::<ffi::AVVulkanFramesContext>() };
        if vulkan.is_null() {
            return false;
        }
        // VkImageTiling is an i32 in the generated Vulkan ABI bindings.
        // VK_IMAGE_TILING_DRM_FORMAT_MODIFIER_EXT = 1000158000.
        unsafe {
            (*vulkan).tiling = 1_000_158_000;
            // NVIDIA exposes exportable modifiers for the component formats
            // used by NV12, while its video decode profile rejects an
            // exportable multi-planar image. Two images still remain entirely
            // GPU-owned and are described as two DRM PRIME layers.
            (*vulkan).flags = ffi::AVVkFrameFlags::AV_VK_FRAME_FLAG_DISABLE_MULTIPLANE;
        }
        if unsafe { ffi::av_hwframe_ctx_init(frames) } < 0 {
            return false;
        }
        let retained = unsafe { ffi::av_buffer_ref(frames) };
        if retained.is_null() {
            return false;
        }
        unsafe {
            ffi::av_buffer_unref(&mut (*context).hw_frames_ctx);
            (*context).hw_frames_ctx = retained;
        }
        true
    })();
    unsafe { ffi::av_buffer_unref(&mut frames) };
    configured
}

fn ffmpeg_error(operation: String, code: i32) -> Error {
    Error::backend(
        Subsystem::Ffmpeg,
        format!("{operation}: {}", ffmpeg::Error::from(code)),
    )
}

fn map_color_range(frame: &frame::Video) -> ColorRange {
    match frame.color_range() {
        ffmpeg::color::Range::JPEG => ColorRange::Full,
        _ => ColorRange::Limited,
    }
}

fn map_color_matrix(frame: &frame::Video) -> ColorMatrix {
    match frame.color_space() {
        ffmpeg::color::Space::BT2020NCL | ffmpeg::color::Space::BT2020CL => ColorMatrix::Bt2020,
        ffmpeg::color::Space::BT470BG | ffmpeg::color::Space::SMPTE170M => ColorMatrix::Bt601,
        _ => ColorMatrix::Bt709,
    }
}

fn map_chroma_location(frame: &frame::Video) -> ChromaLocation {
    match frame.chroma_location() {
        ffmpeg::chroma::Location::Center => ChromaLocation::Center,
        _ => ChromaLocation::Left,
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    #[test]
    fn all_required_software_decoders_are_linked() {
        initialize_ffmpeg().unwrap();
        for codec in [VideoCodec::H264, VideoCodec::H265, VideoCodec::Av1] {
            assert!(ffmpeg::decoder::find(codec_id(codec)).is_some());
        }
    }

    #[test]
    #[ignore = "requires the FFmpeg CLI and local Vulkan/CUDA video hardware"]
    fn local_hardware_decodes_all_required_codecs() {
        for codec in [VideoCodec::H264, VideoCodec::H265, VideoCodec::Av1] {
            let (encoder, muxer, codec_options): (&str, &str, &[&str]) = match codec {
                VideoCodec::H264 => ("libx264", "h264", &["-tune", "zerolatency"]),
                VideoCodec::H265 => (
                    "libx265",
                    "hevc",
                    &["-x265-params", "log-level=error:pools=1"],
                ),
                VideoCodec::Av1 => ("libsvtav1", "obu", &["-preset", "13"]),
            };
            let mut command = Command::new("ffmpeg");
            command.args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=blue:size=256x144:rate=60",
                "-frames:v",
                "1",
                "-c:v",
                encoder,
            ]);
            command.args(codec_options);
            let encoded = command
                .args(["-f", muxer, "pipe:1"])
                .output()
                .expect("FFmpeg CLI must start");
            assert!(
                encoded.status.success(),
                "sample encode failed for {}: {}",
                codec.label(),
                String::from_utf8_lossy(&encoded.stderr)
            );
            assert!(!encoded.stdout.is_empty());

            for mode in [FfmpegMode::Vulkan, FfmpegMode::Cuda] {
                let frame = (|| {
                    let format = StreamFormat::video_default(256, 144)?;
                    let mut decoder = FfmpegDecoder::open(codec, format, mode)?;
                    let packet = EncodedVideoFrame::new(encoded.stdout.clone(), 1, true)?;
                    let mut frames = decoder.decode(&packet)?;
                    frames.extend(decoder.flush()?);
                    if frames.len() != 1 {
                        return Err(Error::backend(
                            Subsystem::Ffmpeg,
                            format!("expected one frame, received {}", frames.len()),
                        ));
                    }
                    Ok(frames.remove(0))
                })()
                .unwrap_or_else(|error: Error| {
                    panic!("{} failed via {}: {error}", codec.label(), mode.label())
                });
                eprintln!("{} decoded via {}", codec.label(), mode.label());
                frame.validate().unwrap();
                assert_eq!(
                    (frame.format.width, frame.format.height),
                    (256, 144),
                    "{} dimensions changed via {}",
                    codec.label(),
                    mode.label()
                );
            }
        }
    }
}
