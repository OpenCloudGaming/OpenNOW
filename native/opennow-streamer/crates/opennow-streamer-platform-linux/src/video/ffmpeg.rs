use std::ptr;
use std::sync::{Arc, Mutex, OnceLock};

use ffmpeg::codec;
use ffmpeg::ffi;
use ffmpeg::format::Pixel;
use ffmpeg::frame;
use ffmpeg::software::scaling::{context::Context as Scaler, flag::Flags as ScaleFlags};
use ffmpeg_next as ffmpeg;

use crate::{
    ChromaLocation, ColorMatrix, ColorPrimaries, ColorRange, ColorTransfer, DecodedVideoFrame,
    DmaBufFrame, DmaBufLayer, DmaBufObject, DmaBufPlane, EncodedVideoFrame, Error, FramePlane,
    PixelFormat, Result, StreamFormat, Subsystem, VideoCodec, VulkanImage, VulkanVideoFrame,
};

use super::VideoDecoder;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FfmpegMode {
    Vulkan,
    Cuda,
    Vaapi,
    V4l2Request,
    Software,
}

impl FfmpegMode {
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Vulkan => "FFmpeg Vulkan Video",
            Self::Cuda => "FFmpeg CUDA/NVDEC",
            Self::Vaapi => "FFmpeg VAAPI",
            Self::V4l2Request => "FFmpeg V4L2 HEVC request",
            Self::Software => "FFmpeg software",
        }
    }

    const fn device_type(self) -> Option<ffi::AVHWDeviceType> {
        match self {
            Self::Vulkan => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN),
            Self::Cuda => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_CUDA),
            Self::Vaapi => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI),
            Self::V4l2Request => Some(ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_DRM),
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
    negotiated_format: StreamFormat,
    pending_format_change: Option<StreamFormat>,
    last_timestamp_us: u64,
    zero_copy_active: bool,
    zero_copy_unavailable_reported: bool,
    #[cfg(feature = "vulkan")]
    shared_device: Option<Arc<crate::SharedVulkanDevice>>,
    #[cfg(feature = "vulkan")]
    snapshot_pool: Option<super::vulkan_copy::VulkanCopyPool>,
}

struct HardwareFormatSelection {
    pixel_format: ffi::AVPixelFormat,
    exportable_vulkan_frames: bool,
}

impl FfmpegDecoder {
    pub(crate) fn supports_vaapi_ten_bit(codec: VideoCodec) -> bool {
        initialize_ffmpeg().is_ok()
            && ffmpeg::decoder::find_by_name(native_decoder_name(codec)).is_some_and(|decoder| {
                hardware_pixel_format(decoder, ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI)
                    .is_some()
            })
            && super::vaapi_probe::ten_bit_device(codec).is_some()
    }

    pub(crate) fn open(codec: VideoCodec, format: StreamFormat, mode: FfmpegMode) -> Result<Self> {
        Self::open_internal(codec, format, mode, None)
    }

    #[cfg(feature = "vulkan")]
    pub(crate) fn open_shared(
        codec: VideoCodec,
        format: StreamFormat,
        device: Arc<crate::SharedVulkanDevice>,
    ) -> Result<Self> {
        if !device.supports(
            codec,
            format.pixel_format == PixelFormat::P010,
            format.width,
            format.height,
        ) {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                "negotiated codec, bit depth or dimensions are unsupported by the shared Vulkan device",
            ));
        }
        Self::open_internal(codec, format, FfmpegMode::Vulkan, Some(device))
    }

    fn open_internal(
        codec: VideoCodec,
        format: StreamFormat,
        mode: FfmpegMode,
        shared_device: Option<Arc<crate::SharedVulkanDevice>>,
    ) -> Result<Self> {
        format.validate()?;
        if mode == FfmpegMode::V4l2Request {
            validate_request_profile(codec, format)?;
            super::v4l2_request::probe()
                .map_err(|reason| Error::unavailable(Subsystem::V4l2, reason))?;
        }
        if (format.color_transfer != ColorTransfer::Sdr || format.pixel_format == PixelFormat::P010)
            && matches!(mode, FfmpegMode::Cuda | FfmpegMode::Software)
        {
            return Err(Error::unavailable(
                Subsystem::Ffmpeg,
                "HDR/10-bit requires Vulkan Video or VAAPI zero-copy decoding",
            ));
        }
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
            if mode == FfmpegMode::V4l2Request {
                (*raw).extra_hw_frames = 6;
            }
        }

        let mut wanted_hw_format = None;
        let vaapi_device = if mode == FfmpegMode::Vaapi && format.pixel_format == PixelFormat::P010
        {
            Some(super::vaapi_probe::ten_bit_device(codec).ok_or_else(|| {
                Error::unavailable(
                    Subsystem::VaApi,
                    "no VAAPI decode profile supports the requested 10-bit 4:2:0 format",
                )
            })?)
        } else {
            None
        };
        let device_path = vaapi_device
            .as_ref()
            .map_or(ptr::null(), |path| path.as_ptr());
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
            if mode == FfmpegMode::V4l2Request {
                unsafe {
                    ffi::av_dict_set(&mut options, c"v4l2fmts".as_ptr(), c"NC12/Nc12".as_ptr(), 0);
                }
            }
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
            #[cfg(feature = "vulkan")]
            let create_result = if let Some(shared) = shared_device.as_ref() {
                device = shared.retain();
                if device.is_null() { -12 } else { 0 }
            } else {
                unsafe {
                    ffi::av_hwdevice_ctx_create(&mut device, device_type, device_path, options, 0)
                }
            };
            #[cfg(not(feature = "vulkan"))]
            let create_result = unsafe {
                ffi::av_hwdevice_ctx_create(&mut device, device_type, device_path, options, 0)
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
            negotiated_format: format,
            pending_format_change: None,
            last_timestamp_us: 0,
            zero_copy_active: false,
            zero_copy_unavailable_reported: false,
            #[cfg(feature = "vulkan")]
            shared_device,
            #[cfg(feature = "vulkan")]
            snapshot_pool: None,
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
                    return Err(self.decode_error("frame receive", error));
                }
            }
        }
        Ok(frames)
    }

    fn decode_error(&self, operation: &str, error: ffmpeg::Error) -> Error {
        let reason = format!(
            "{} {} {operation} failed: {error}",
            self.mode.label(),
            self.codec.label()
        );
        if self.mode == FfmpegMode::V4l2Request && error == ffmpeg::Error::InvalidData {
            Error::ReferenceLost {
                subsystem: Subsystem::V4l2,
                reason,
            }
        } else {
            Error::backend(Subsystem::Ffmpeg, reason)
        }
    }

    fn convert_frame(&mut self, decoded: &frame::Video) -> Result<DecodedVideoFrame> {
        if self.mode == FfmpegMode::V4l2Request {
            let output =
                retain_request_frame(decoded, self.last_timestamp_us, self.negotiated_format)?;
            if output.format != self.configured_format {
                self.configured_format = output.format;
                self.pending_format_change = Some(output.format);
            }
            return Ok(output);
        }
        let metadata = decoded_metadata(decoded, self.negotiated_format)?;
        #[cfg(feature = "vulkan")]
        if let Some(device) = self.shared_device.as_ref() {
            if decoded.format() != Pixel::VULKAN {
                return Err(Error::backend(
                    Subsystem::Vulkan,
                    "embedded Vulkan decoder returned a non-GPU frame",
                ));
            }
            if self.snapshot_pool.is_none() {
                self.snapshot_pool =
                    Some(super::vulkan_copy::VulkanCopyPool::new(Arc::clone(device))?);
            }
            let mut output = self
                .snapshot_pool
                .as_mut()
                .expect("initialized snapshot pool")
                .copy(decoded, self.last_timestamp_us)?;
            if output.format.pixel_format != self.configured_format.pixel_format
                || !device.supports(
                    self.codec,
                    output.format.pixel_format == PixelFormat::P010,
                    output.format.width,
                    output.format.height,
                )
            {
                return Err(Error::unavailable(
                    Subsystem::Vulkan,
                    "decoded Vulkan format differs from the supported negotiated profile",
                ));
            }
            output.format = StreamFormat {
                width: output.format.width,
                height: output.format.height,
                pixel_format: output.format.pixel_format,
                ..metadata
            };
            if output.format != self.configured_format {
                self.configured_format = output.format;
                self.pending_format_change = Some(output.format);
            }
            return Ok(output);
        }
        let selected_hardware = self
            .wanted_hw_format
            .as_deref()
            .is_some_and(|selection| Pixel::from(selection.pixel_format) == decoded.format());
        let actual_pixel = if selected_hardware {
            decoded_hardware_software_format(decoded)?
        } else {
            decoded.format()
        };
        let allow_cpu_conversion = cpu_conversion_allowed(metadata, actual_pixel);
        if matches!(self.mode, FfmpegMode::Vulkan | FfmpegMode::Vaapi) && selected_hardware {
            let output = if self.mode == FfmpegMode::Vulkan && allow_cpu_conversion {
                let direct_vulkan =
                    map_vulkan_frame_direct(decoded, self.last_timestamp_us, metadata)
                        .ok()
                        .and_then(|frame| frame.vulkan);
                match map_hardware_frame_to_dmabuf(decoded, self.last_timestamp_us, metadata) {
                    Ok(mut frame) => {
                        frame.vulkan = direct_vulkan;
                        if !self.zero_copy_active {
                            eprintln!(
                                "Vulkan Video frames expose same-device and DRM PRIME/DMA-BUF paths"
                            );
                            self.zero_copy_active = true;
                        }
                        Some(frame)
                    }
                    Err(error) => {
                        if !self.zero_copy_unavailable_reported {
                            eprintln!(
                                "Vulkan Video DMA-BUF export unavailable; using bounded CPU transfer: {error}"
                            );
                            self.zero_copy_unavailable_reported = true;
                        }
                        None
                    }
                }
            } else if self.mode == FfmpegMode::Vulkan {
                Some(map_vulkan_frame_direct(
                    decoded,
                    self.last_timestamp_us,
                    metadata,
                )?)
            } else {
                Some(map_hardware_frame_to_dmabuf(
                    decoded,
                    self.last_timestamp_us,
                    metadata,
                )?)
            };
            if let Some(output) = output {
                if self.mode == FfmpegMode::Vaapi
                    && output.format.pixel_format != self.negotiated_format.pixel_format
                {
                    return Err(Error::unavailable(
                        Subsystem::VaApi,
                        "VAAPI output depth differs from the negotiated decode format",
                    ));
                }
                output.format.validate()?;
                if output.format != self.configured_format {
                    self.configured_format = output.format;
                    self.pending_format_change = Some(output.format);
                }
                return Ok(output);
            }
        }
        if !allow_cpu_conversion || self.mode == FfmpegMode::Vaapi {
            return Err(Error::unavailable(
                Subsystem::Ffmpeg,
                "HDR/10-bit frame has no supported zero-copy decode path",
            ));
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
            ..metadata
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
            timestamp_us,
        })
    }
}

fn map_vulkan_frame_direct(
    decoded: &frame::Video,
    fallback_timestamp_us: u64,
    metadata: StreamFormat,
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
    let device = if metadata.color_transfer == ColorTransfer::Sdr
        && decoded_hardware_pixel_format(decoded)? == PixelFormat::Nv12
    {
        device.with_cpu_nv12_fallback(vulkan_cpu_nv12_fallback(decoded)?)
    } else {
        device
    };
    let output_format = StreamFormat {
        width,
        height,
        pixel_format: decoded_hardware_pixel_format(decoded)?,
        ..metadata
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
        timestamp_us,
    };
    output.validate()?;
    Ok(output)
}

fn vulkan_cpu_nv12_fallback(
    decoded: &frame::Video,
) -> Result<Arc<dyn Fn() -> Result<Vec<FramePlane>> + Send + Sync>> {
    let mut retained = frame::Video::empty();
    if unsafe { ffi::av_frame_ref(retained.as_mut_ptr(), decoded.as_ptr()) } < 0 {
        return Err(Error::backend(
            Subsystem::Ffmpeg,
            "failed to retain Vulkan frame for CPU fallback",
        ));
    }
    let retained = Mutex::new(retained);
    Ok(Arc::new(move || {
        let source = retained.lock().unwrap_or_else(|poison| poison.into_inner());
        let mut transferred = frame::Video::empty();
        let transfer_result =
            unsafe { ffi::av_hwframe_transfer_data(transferred.as_mut_ptr(), source.as_ptr(), 0) };
        if transfer_result < 0 {
            return Err(ffmpeg_error(
                "Vulkan frame CPU fallback download failed".to_owned(),
                transfer_result,
            ));
        }
        let width = transferred.width().max(1);
        let height = transferred.height().max(1);
        let converted;
        let nv12 = if transferred.format() == Pixel::NV12 {
            &transferred
        } else {
            let mut scaler = Scaler::get(
                transferred.format(),
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
                    format!("Vulkan CPU fallback conversion setup failed: {error}"),
                )
            })?;
            let mut output = frame::Video::empty();
            scaler.run(&transferred, &mut output).map_err(|error| {
                Error::backend(
                    Subsystem::Ffmpeg,
                    format!("Vulkan CPU fallback NV12 conversion failed: {error}"),
                )
            })?;
            converted = output;
            &converted
        };
        Ok(vec![
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
        ])
    }))
}

fn map_hardware_frame_to_dmabuf(
    decoded: &frame::Video,
    fallback_timestamp_us: u64,
    metadata: StreamFormat,
) -> Result<DecodedVideoFrame> {
    let mut mapped = frame::Video::empty();
    mapped.set_format(Pixel::DRM_PRIME);
    let map_result = unsafe {
        ffi::av_hwframe_map(
            mapped.as_mut_ptr(),
            decoded.as_ptr(),
            ffi::AV_HWFRAME_MAP_READ as i32 | ffi::AV_HWFRAME_MAP_DIRECT as i32,
        )
    };
    if map_result < 0 {
        return Err(ffmpeg_error(
            "Vulkan Video frame DMA-BUF export failed".to_owned(),
            map_result,
        ));
    }
    let output_format = StreamFormat {
        width: decoded.width().max(1),
        height: decoded.height().max(1),
        pixel_format: decoded_hardware_pixel_format(decoded)?,
        ..metadata
    };
    let timestamp_us = decoded
        .pts()
        .and_then(|timestamp| u64::try_from(timestamp).ok())
        .unwrap_or(fallback_timestamp_us);
    drm_frame(mapped, timestamp_us, output_format)
}

fn validate_request_profile(codec: VideoCodec, format: StreamFormat) -> Result<()> {
    if codec != VideoCodec::H265
        || format.pixel_format != PixelFormat::Nv12
        || format.color_transfer != ColorTransfer::Sdr
    {
        return Err(Error::unavailable(
            Subsystem::V4l2,
            "V4L2 HEVC request decoding supports only HEVC 8-bit 4:2:0 SDR",
        ));
    }
    Ok(())
}

fn retain_request_frame(
    decoded: &frame::Video,
    fallback_timestamp_us: u64,
    metadata: StreamFormat,
) -> Result<DecodedVideoFrame> {
    if decoded.format() != Pixel::DRM_PRIME {
        return Err(Error::backend(
            Subsystem::V4l2,
            "HEVC request decoder returned a non-DRM frame; CPU fallback is disabled",
        ));
    }
    unsafe {
        let raw = &*decoded.as_ptr();
        if raw.flags & ffi::AV_FRAME_FLAG_CORRUPT != 0 || raw.decode_error_flags != 0 {
            return Err(Error::ReferenceLost {
                subsystem: Subsystem::V4l2,
                reason: "HEVC request decode failed; refusing a corrupt reference frame".to_owned(),
            });
        }
        if raw.crop_left != 0 || raw.crop_top != 0 {
            return Err(Error::unavailable(
                Subsystem::V4l2,
                "SAND presentation does not support a nonzero crop origin",
            ));
        }
    }
    let metadata = decoded_metadata(decoded, metadata)?;
    validate_request_profile(VideoCodec::H265, metadata)?;
    let mut retained = frame::Video::empty();
    let result = unsafe { ffi::av_frame_ref(retained.as_mut_ptr(), decoded.as_ptr()) };
    if result < 0 {
        return Err(ffmpeg_error("retain HEVC request frame".to_owned(), result));
    }
    let timestamp_us = decoded
        .pts()
        .and_then(|timestamp| u64::try_from(timestamp).ok())
        .unwrap_or(fallback_timestamp_us);
    let output = drm_frame(
        retained,
        timestamp_us,
        StreamFormat {
            width: decoded.width(),
            height: decoded.height(),
            pixel_format: PixelFormat::Nv12,
            ..metadata
        },
    )?;
    let dmabuf = output
        .dmabuf
        .as_ref()
        .expect("DRM frame has DMA-BUF backing");
    if dmabuf.layers.len() != 1
        || dmabuf.layers[0].format != u32::from_le_bytes(*b"NV12")
        || dmabuf.layers[0].planes.len() != 2
        || dmabuf
            .objects
            .iter()
            .any(|object| object.format_modifier & 0xff00_0000_0000_00ff != 0x0700_0000_0000_0004)
    {
        return Err(Error::unavailable(
            Subsystem::V4l2,
            "HEVC request output is not SAND128 NV12; packed P030/10-bit is unsupported",
        ));
    }
    Ok(output)
}

fn drm_frame(
    mapped: frame::Video,
    timestamp_us: u64,
    output_format: StreamFormat,
) -> Result<DecodedVideoFrame> {
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
    let dmabuf = Arc::new(DmaBufFrame::new(objects, layers, Arc::new(mapped)));
    let frame = DecodedVideoFrame {
        format: output_format,
        planes: Vec::new(),
        dmabuf: Some(dmabuf),
        vulkan: None,
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
        self.decoder
            .send_packet(&packet)
            .map_err(|error| self.decode_error("packet submission", error))?;
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

fn decoded_hardware_software_format(frame: &frame::Video) -> Result<Pixel> {
    unsafe {
        let frames = (*frame.as_ptr()).hw_frames_ctx;
        if frames.is_null() || (*frames).data.is_null() {
            return Err(Error::backend(
                Subsystem::Ffmpeg,
                "hardware frame has no software format metadata",
            ));
        }
        Ok(Pixel::from(
            (*(*frames).data.cast::<ffi::AVHWFramesContext>()).sw_format,
        ))
    }
}

fn decoded_hardware_pixel_format(frame: &frame::Video) -> Result<PixelFormat> {
    let pixel = decoded_hardware_software_format(frame)?;
    match pixel {
        Pixel::NV12 => Ok(PixelFormat::Nv12),
        Pixel::P010LE => Ok(PixelFormat::P010),
        _ => Err(Error::unavailable(
            Subsystem::Ffmpeg,
            format!("unsupported zero-copy hardware pixel format {pixel:?}"),
        )),
    }
}

fn cpu_conversion_allowed(metadata: StreamFormat, pixel: Pixel) -> bool {
    if metadata.color_transfer != ColorTransfer::Sdr || metadata.pixel_format == PixelFormat::P010 {
        return false;
    }
    let descriptor = unsafe { ffi::av_pix_fmt_desc_get(pixel.into()) };
    !descriptor.is_null()
        && unsafe {
            (*descriptor).nb_components != 0
                && (*descriptor)
                    .comp
                    .iter()
                    .all(|component| component.depth <= 8)
        }
}

fn decoded_metadata(frame: &frame::Video, defaults: StreamFormat) -> Result<StreamFormat> {
    use ffmpeg::color::{Primaries, Range, Space, TransferCharacteristic as Transfer};
    let unsupported =
        || Error::unavailable(Subsystem::Ffmpeg, "unsupported decoded color metadata");
    let color_range = match frame.color_range() {
        Range::Unspecified => defaults.color_range,
        Range::JPEG => ColorRange::Full,
        Range::MPEG => ColorRange::Limited,
    };
    let color_matrix = match frame.color_space() {
        Space::Unspecified => defaults.color_matrix,
        Space::BT709 => ColorMatrix::Bt709,
        Space::BT470BG | Space::SMPTE170M => ColorMatrix::Bt601,
        Space::BT2020NCL => ColorMatrix::Bt2020,
        _ => return Err(unsupported()),
    };
    let color_transfer = match frame.color_transfer_characteristic() {
        Transfer::Unspecified => defaults.color_transfer,
        Transfer::SMPTE2084 => ColorTransfer::Pq,
        Transfer::ARIB_STD_B67 => ColorTransfer::Hlg,
        Transfer::BT709
        | Transfer::SMPTE170M
        | Transfer::IEC61966_2_1
        | Transfer::BT2020_10
        | Transfer::BT2020_12 => ColorTransfer::Sdr,
        _ => return Err(unsupported()),
    };
    let color_primaries = match frame.color_primaries() {
        Primaries::Unspecified => defaults.color_primaries,
        Primaries::BT709 => ColorPrimaries::Bt709,
        Primaries::BT2020 => ColorPrimaries::Bt2020,
        _ => return Err(unsupported()),
    };
    let chroma_location = match frame.chroma_location() {
        ffmpeg::chroma::Location::Unspecified => defaults.chroma_location,
        ffmpeg::chroma::Location::Center => ChromaLocation::Center,
        ffmpeg::chroma::Location::Left => ChromaLocation::Left,
        _ => return Err(unsupported()),
    };
    Ok(StreamFormat {
        color_range,
        color_matrix,
        color_transfer,
        color_primaries,
        chroma_location,
        ..defaults
    })
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::os::fd::AsRawFd;
    use std::process::Command;

    use super::*;

    #[cfg(all(target_arch = "aarch64", feature = "ffmpeg-bundled"))]
    #[test]
    fn bundled_arm64_hevc_exposes_request_hwaccel() {
        initialize_ffmpeg().unwrap();
        let decoder = ffmpeg::decoder::find_by_name("hevc").expect("HEVC decoder must be bundled");
        assert_eq!(
            hardware_pixel_format(decoder, ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_DRM),
            Some(ffi::AVPixelFormat::AV_PIX_FMT_DRM_PRIME),
            "bundled ARM64 HEVC requires a DRM_PRIME hardware config with HW_DEVICE_CTX support",
        );
    }

    fn request_frame_fixture() -> (frame::Video, File) {
        let file = File::open("/dev/zero").unwrap();
        let mut frame = frame::Video::empty();
        frame.set_format(Pixel::DRM_PRIME);
        frame.set_width(256);
        frame.set_height(144);
        frame.set_pts(Some(123_456));
        unsafe {
            let raw = &mut *frame.as_mut_ptr();
            raw.buf[0] = ffi::av_buffer_alloc(std::mem::size_of::<ffi::AVDRMFrameDescriptor>());
            assert!(!raw.buf[0].is_null());
            raw.data[0] = (*raw.buf[0]).data;
            let descriptor = &mut *raw.data[0].cast::<ffi::AVDRMFrameDescriptor>();
            *descriptor = std::mem::zeroed();
            descriptor.nb_objects = 1;
            descriptor.objects[0].fd = file.as_raw_fd();
            descriptor.objects[0].size = 256 * 256;
            descriptor.objects[0].format_modifier = 0x0700_0000_0000_0004 | (256 << 8);
            descriptor.nb_layers = 1;
            descriptor.layers[0].format = u32::from_le_bytes(*b"NV12");
            descriptor.layers[0].nb_planes = 2;
            descriptor.layers[0].planes[0].pitch = 256;
            descriptor.layers[0].planes[1].pitch = 256;
            descriptor.layers[0].planes[1].offset = 144 * 128;
            raw.hw_frames_ctx = ffi::av_buffer_alloc(std::mem::size_of::<ffi::AVHWFramesContext>());
            assert!(!raw.hw_frames_ctx.is_null());
            let hwframes = &mut *(*raw.hw_frames_ctx).data.cast::<ffi::AVHWFramesContext>();
            *hwframes = std::mem::zeroed();
            hwframes.sw_format = ffi::AVPixelFormat::AV_PIX_FMT_NB;
        }
        (frame, file)
    }

    #[test]
    fn request_frame_retains_drm_backing_without_software_pixel_conversion() {
        let (decoded, _file) = request_frame_fixture();
        let metadata = StreamFormat::video_default(256, 144).unwrap();
        let output = retain_request_frame(&decoded, 999, metadata).unwrap();
        assert_eq!(output.timestamp_us, 123_456);
        assert_eq!(output.format, metadata);
        assert!(output.planes.is_empty());
        assert!(output.vulkan.is_none());
        assert_eq!(output.dmabuf.as_ref().unwrap().objects.len(), 1);
        unsafe {
            assert_eq!(ffi::av_buffer_get_ref_count((*decoded.as_ptr()).buf[0]), 2);
        }
        drop(output);
        unsafe {
            assert_eq!(ffi::av_buffer_get_ref_count((*decoded.as_ptr()).buf[0]), 1);
        }
    }

    #[test]
    fn request_frame_rejects_corruption_p030_linear_and_cropped_origins() {
        let metadata = StreamFormat::video_default(256, 144).unwrap();
        let (mut decoded, _file) = request_frame_fixture();
        unsafe { (*decoded.as_mut_ptr()).flags |= ffi::AV_FRAME_FLAG_CORRUPT };
        assert!(matches!(
            retain_request_frame(&decoded, 0, metadata),
            Err(Error::ReferenceLost {
                subsystem: Subsystem::V4l2,
                ..
            })
        ));
        unsafe {
            (*decoded.as_mut_ptr()).flags = 0;
            (*decoded.as_mut_ptr()).crop_left = 2;
        }
        assert!(retain_request_frame(&decoded, 0, metadata).is_err());
        unsafe {
            (*decoded.as_mut_ptr()).crop_left = 0;
            let descriptor =
                &mut *(*decoded.as_mut_ptr()).data[0].cast::<ffi::AVDRMFrameDescriptor>();
            descriptor.layers[0].format = u32::from_le_bytes(*b"P030");
        }
        assert!(retain_request_frame(&decoded, 0, metadata).is_err());
        unsafe {
            let descriptor =
                &mut *(*decoded.as_mut_ptr()).data[0].cast::<ffi::AVDRMFrameDescriptor>();
            descriptor.layers[0].format = u32::from_le_bytes(*b"NV12");
            descriptor.objects[0].format_modifier = 0;
        }
        assert!(retain_request_frame(&decoded, 0, metadata).is_err());
        decoded.set_format(Pixel::NV12);
        assert!(retain_request_frame(&decoded, 0, metadata).is_err());
    }

    #[test]
    fn ffmpeg_applies_request_frame_padding_before_retention() {
        let context = codec::Context::new();
        assert_eq!(unsafe { (*context.as_ptr()).apply_cropping }, 1);
        let (mut decoded, _file) = request_frame_fixture();
        decoded.set_width(1920);
        decoded.set_height(1088);
        unsafe {
            let raw = &mut *decoded.as_mut_ptr();
            raw.crop_bottom = 8;
            let descriptor = &mut *raw.data[0].cast::<ffi::AVDRMFrameDescriptor>();
            descriptor.objects[0].size = 15 * 128 * 1632;
            descriptor.objects[0].format_modifier = 0x0700_0000_0000_0004 | (1632 << 8);
            descriptor.layers[0].planes[0].pitch = 1920;
            descriptor.layers[0].planes[1].pitch = 1920;
            descriptor.layers[0].planes[1].offset = 1088 * 128;
            assert_eq!(ffi::av_frame_apply_cropping(decoded.as_mut_ptr(), 0), 0);
            assert_eq!((*decoded.as_ptr()).crop_bottom, 0);
        }
        let output = retain_request_frame(
            &decoded,
            0,
            StreamFormat::video_default(1920, 1080).unwrap(),
        )
        .unwrap();
        assert_eq!((output.format.width, output.format.height), (1920, 1080));
        assert_eq!(
            output.dmabuf.as_ref().unwrap().layers[0].planes[1].offset,
            1088 * 128,
        );
    }

    #[test]
    fn request_decode_rejects_non_hevc_ten_bit_and_hdr_before_device_probe() {
        let mut format = StreamFormat::video_default(256, 144).unwrap();
        assert!(validate_request_profile(VideoCodec::H265, format).is_ok());
        for codec in [VideoCodec::H264, VideoCodec::Av1] {
            assert!(FfmpegDecoder::open(codec, format, FfmpegMode::V4l2Request).is_err());
        }
        format.pixel_format = PixelFormat::P010;
        assert!(FfmpegDecoder::open(VideoCodec::H265, format, FfmpegMode::V4l2Request).is_err());
        format.pixel_format = PixelFormat::Nv12;
        format.color_transfer = ColorTransfer::Pq;
        assert!(FfmpegDecoder::open(VideoCodec::H265, format, FfmpegMode::V4l2Request).is_err());
    }

    #[test]
    #[ignore = "opt-in: requires Raspberry Pi HEVC request hardware, pinned FFmpeg, and FFmpeg CLI/libx265"]
    fn raspberry_pi_hevc_request_drm_only() {
        for (width, height) in [(256, 144), (640, 360)] {
            let encoded = Command::new("ffmpeg")
                .args([
                    "-hide_banner",
                    "-loglevel",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("color=c=blue:size={width}x{height}:rate=60"),
                    "-frames:v",
                    "1",
                    "-pix_fmt",
                    "yuv420p",
                    "-c:v",
                    "libx265",
                    "-x265-params",
                    "log-level=error:pools=1",
                    "-f",
                    "hevc",
                    "pipe:1",
                ])
                .output()
                .expect("FFmpeg CLI must start");
            assert!(
                encoded.status.success(),
                "{}",
                String::from_utf8_lossy(&encoded.stderr)
            );
            let format = StreamFormat::video_default(width, height).unwrap();
            let mut decoder =
                FfmpegDecoder::open(VideoCodec::H265, format, FfmpegMode::V4l2Request)
                    .expect("Pi HEVC request decoder must open");
            let packet = EncodedVideoFrame::new(encoded.stdout, 1_234_567, true).unwrap();
            let mut frames = decoder.decode(&packet).unwrap();
            frames.extend(decoder.flush().unwrap());
            assert_eq!(frames.len(), 1);
            let frame = frames.remove(0);
            drop(decoder);
            frame.validate().unwrap();
            assert_eq!((frame.format.width, frame.format.height), (width, height));
            assert_eq!(frame.timestamp_us, packet.timestamp_us);
            assert_eq!(frame.format.pixel_format, PixelFormat::Nv12);
            assert!(frame.planes.is_empty());
            assert!(frame.vulkan.is_none());
            assert!(frame.dmabuf.is_some());
            assert!(
                frame
                    .dmabuf
                    .as_ref()
                    .unwrap()
                    .objects
                    .iter()
                    .all(|object| unsafe { libc::fcntl(object.fd, libc::F_GETFD) >= 0 })
            );
        }
    }

    #[test]
    fn unspecified_metadata_uses_negotiation_but_explicit_sdr_overrides_hdr() {
        let defaults = StreamFormat {
            pixel_format: PixelFormat::P010,
            color_transfer: ColorTransfer::Pq,
            color_primaries: ColorPrimaries::Bt2020,
            color_matrix: ColorMatrix::Bt2020,
            color_range: ColorRange::Full,
            ..StreamFormat::video_default(1920, 1080).unwrap()
        };
        let mut frame = frame::Video::new(Pixel::P010LE, 2, 2);
        assert_eq!(decoded_metadata(&frame, defaults).unwrap(), defaults);
        frame.set_color_transfer_characteristic(ffmpeg::color::TransferCharacteristic::BT709);
        frame.set_color_primaries(ffmpeg::color::Primaries::BT709);
        frame.set_color_space(ffmpeg::color::Space::BT709);
        frame.set_color_range(ffmpeg::color::Range::MPEG);
        let metadata = decoded_metadata(&frame, defaults).unwrap();
        assert_eq!(metadata.color_transfer, ColorTransfer::Sdr);
        assert_eq!(metadata.color_primaries, ColorPrimaries::Bt709);
        assert_eq!(metadata.color_matrix, ColorMatrix::Bt709);
        assert_eq!(metadata.color_range, ColorRange::Limited);
        frame
            .set_color_transfer_characteristic(ffmpeg::color::TransferCharacteristic::ARIB_STD_B67);
        frame.set_color_primaries(ffmpeg::color::Primaries::BT2020);
        assert_eq!(
            decoded_metadata(&frame, defaults).unwrap().color_transfer,
            ColorTransfer::Hlg
        );
        frame.set_color_space(ffmpeg::color::Space::BT2020CL);
        assert!(decoded_metadata(&frame, defaults).is_err());
        frame.set_color_space(ffmpeg::color::Space::BT2020NCL);
        unsafe {
            (*frame.as_mut_ptr()).chroma_location = ffmpeg::chroma::Location::TopLeft.into();
        }
        assert!(decoded_metadata(&frame, defaults).is_err());
    }

    #[test]
    fn standalone_cpu_recovery_is_limited_to_actual_eight_bit_sdr() {
        let mut metadata = StreamFormat::video_default(1920, 1080).unwrap();
        assert!(cpu_conversion_allowed(metadata, Pixel::NV12));
        assert!(cpu_conversion_allowed(metadata, Pixel::YUV420P));
        assert!(!cpu_conversion_allowed(metadata, Pixel::P010LE));
        assert!(!cpu_conversion_allowed(metadata, Pixel::YUV420P10LE));
        for transfer in [ColorTransfer::Pq, ColorTransfer::Hlg] {
            metadata.color_transfer = transfer;
            assert!(!cpu_conversion_allowed(metadata, Pixel::NV12));
            assert!(!cpu_conversion_allowed(metadata, Pixel::P010LE));
        }
        metadata.color_transfer = ColorTransfer::Sdr;
        metadata.pixel_format = PixelFormat::P010;
        assert!(!cpu_conversion_allowed(metadata, Pixel::NV12));
    }

    #[test]
    fn hdr_and_ten_bit_never_open_cpu_conversion_decoders() {
        for transfer in [ColorTransfer::Sdr, ColorTransfer::Pq, ColorTransfer::Hlg] {
            let format = StreamFormat {
                pixel_format: PixelFormat::P010,
                color_transfer: transfer,
                ..StreamFormat::video_default(1920, 1080).unwrap()
            };
            for mode in [FfmpegMode::Cuda, FfmpegMode::Software] {
                assert!(FfmpegDecoder::open(VideoCodec::H265, format, mode).is_err());
            }
        }
    }

    #[test]
    fn cpu_output_has_no_foreign_gpu_metadata() {
        let format = StreamFormat::video_default(2, 2).unwrap();
        let mut decoder =
            FfmpegDecoder::open(VideoCodec::H264, format, FfmpegMode::Software).unwrap();
        let decoded = frame::Video::new(Pixel::NV12, 2, 2);
        let output = decoder.convert_frame(&decoded).unwrap();
        assert_eq!(output.planes.len(), 2);
        assert!(output.dmabuf.is_none());
        assert!(output.vulkan.is_none());
        output.validate().unwrap();
    }

    #[test]
    fn all_required_software_decoders_are_linked() {
        initialize_ffmpeg().unwrap();
        for codec in [VideoCodec::H264, VideoCodec::H265, VideoCodec::Av1] {
            assert!(ffmpeg::decoder::find(codec_id(codec)).is_some());
        }
    }

    #[cfg(feature = "vulkan")]
    fn shared_hevc_snapshot(ten_bit: bool) {
        let owner =
            crate::SharedVulkanDevice::create().expect("shared Vulkan device must initialize");
        let mut format = StreamFormat::video_default(256, 144).unwrap();
        if ten_bit {
            format.pixel_format = PixelFormat::P010;
        }
        assert!(
            owner.supports(VideoCodec::H265, ten_bit, 256, 144),
            "requested HEVC profile must be supported"
        );
        let encoded = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=256x144:rate=60",
                "-frames:v",
                "1",
                "-pix_fmt",
                if ten_bit { "yuv420p10le" } else { "yuv420p" },
                "-c:v",
                "libx265",
                "-x265-params",
                "log-level=error:pools=1",
                "-f",
                "hevc",
                "pipe:1",
            ])
            .output()
            .expect("FFmpeg CLI with libx265 must start");
        assert!(
            encoded.status.success(),
            "HEVC sample encode failed: {}",
            String::from_utf8_lossy(&encoded.stderr)
        );
        let mut decoder =
            FfmpegDecoder::open_shared(VideoCodec::H265, format, Arc::clone(&owner)).unwrap();
        let packet = EncodedVideoFrame::new(encoded.stdout, 1, true).unwrap();
        let mut frames = decoder.decode(&packet).unwrap();
        frames.extend(decoder.flush().unwrap());
        assert_eq!(frames.len(), 1);
        let frame = frames.remove(0);
        frame.validate().unwrap();
        assert!(frame.planes.is_empty());
        assert!(frame.dmabuf.is_none());
        assert_eq!(frame.format.pixel_format, format.pixel_format);
        let vulkan = frame.vulkan.as_ref().unwrap();
        assert_eq!(vulkan.device, owner.info().device);
        assert!(vulkan.completed_gpu_copy());
        assert_eq!(vulkan.images.len(), 2);
        assert!(
            vulkan.images.iter().all(
                |image| image.layout == ash::vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL.as_raw()
            )
        );
        assert!(vulkan.download_nv12().is_err());
        drop(decoder);
        let producer = crate::LinuxGpuFrameProducer::new(3).unwrap();
        let retained = producer
            .frame(frame)
            .expect("GPU-only snapshot must publish after decoder destruction");
        drop(owner);
        drop(retained);
    }

    #[cfg(feature = "vulkan")]
    #[test]
    #[ignore = "opt-in: requires a Vulkan Video HEVC GPU with an isolated Qt graphics queue and FFmpeg CLI/libx265"]
    fn shared_vulkan_hevc_nv12_gpu_only() {
        shared_hevc_snapshot(false);
    }

    #[cfg(feature = "vulkan")]
    #[test]
    #[ignore = "opt-in: requires a Vulkan Video HEVC Main10 GPU with an isolated Qt graphics queue and FFmpeg CLI/libx265"]
    fn shared_vulkan_hevc_p010_gpu_only() {
        shared_hevc_snapshot(true);
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
