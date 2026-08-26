use std::fmt;
use std::os::fd::AsRawFd;
use std::sync::{Arc, Mutex};

use cros_codecs::backend::vaapi::decoder::VaapiBackend;
use cros_codecs::decoder::stateless::h264::H264;
use cros_codecs::decoder::stateless::{DecodeError, StatelessDecoder, StatelessVideoDecoder};
use cros_codecs::decoder::{DecodedHandle, DecoderEvent};
use cros_codecs::libva::{self, Display, DrmPrimeSurfaceDescriptor, Surface, UsageHint};
use cros_codecs::video_frame::{ReadMapping, VideoFrame, WriteMapping};
use cros_codecs::{Fourcc, Resolution};

use super::VideoDecoder;
use crate::{
    ChromaLocation, DecodedVideoFrame, DmaBufFrame, DmaBufLayer, DmaBufObject, DmaBufPlane,
    EncodedVideoFrame, Error, PixelFormat, Result, StreamFormat, Subsystem,
};

const MAX_DECODE_RETRIES: usize = 16;

struct VaFrame {
    visible: Resolution,
    coded: Resolution,
    stride: usize,
    exported: Mutex<Option<Arc<DrmPrimeSurfaceDescriptor>>>,
}

impl VaFrame {
    fn new(visible: Resolution, coded: Resolution) -> Self {
        let coded_width = align(coded.width.max(visible.width), 16);
        let coded_height = align(coded.height.max(visible.height), 16);
        let stride = align(coded_width, 64) as usize;
        Self {
            visible,
            coded: Resolution::from((coded_width, coded_height)),
            stride,
            exported: Mutex::new(None),
        }
    }

    fn exported(&self) -> std::result::Result<Arc<DrmPrimeSurfaceDescriptor>, String> {
        self.exported
            .lock()
            .map_err(|_| "VA-API export state was poisoned".to_owned())?
            .clone()
            .ok_or_else(|| "VA-API frame has no exported DRM PRIME surface".to_owned())
    }
}

impl fmt::Debug for VaFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VaFrame")
            .field("visible", &self.visible)
            .field("coded", &self.coded)
            .field("stride", &self.stride)
            .finish_non_exhaustive()
    }
}

impl VideoFrame for VaFrame {
    type MemDescriptor = ();
    type NativeHandle = Surface<()>;

    fn fourcc(&self) -> Fourcc {
        Fourcc::from(b"NV12")
    }

    fn resolution(&self) -> Resolution {
        self.visible
    }

    fn get_plane_size(&self) -> Vec<usize> {
        vec![
            self.stride * self.coded.height as usize,
            self.stride * self.coded.height as usize / 2,
        ]
    }

    fn get_plane_pitch(&self) -> Vec<usize> {
        vec![self.stride, self.stride]
    }

    fn map<'a>(&'a self) -> std::result::Result<Box<dyn ReadMapping<'a> + 'a>, String> {
        Err("VA-API zero-copy frames are not CPU mappable".to_owned())
    }

    fn map_mut<'a>(&'a mut self) -> std::result::Result<Box<dyn WriteMapping<'a> + 'a>, String> {
        Err("VA-API zero-copy frames are not CPU mappable".to_owned())
    }

    fn to_native_handle(
        &self,
        display: &Arc<Display>,
    ) -> std::result::Result<Self::NativeHandle, String> {
        let surface = display
            .create_surfaces(
                libva::VA_RT_FORMAT_YUV420,
                Some(libva::VA_FOURCC_NV12),
                self.coded.width,
                self.coded.height,
                Some(UsageHint::USAGE_HINT_DECODER | UsageHint::USAGE_HINT_EXPORT),
                vec![()],
            )
            .map_err(|error| error.to_string())?
            .pop()
            .ok_or_else(|| "VA-API did not create a decode surface".to_owned())?;
        let exported = Arc::new(surface.export_prime().map_err(|error| error.to_string())?);
        *self
            .exported
            .lock()
            .map_err(|_| "VA-API export state was poisoned".to_owned())? = Some(exported);
        Ok(surface)
    }
}

pub(crate) struct VaApiDecoder {
    decoder: H264VaapiDecoder,
    requested_format: StreamFormat,
    allocation_visible: Resolution,
    allocation_coded: Resolution,
    pending_format_change: Option<StreamFormat>,
}

impl VaApiDecoder {
    pub fn probe() -> std::result::Result<String, String> {
        let display = Display::open().ok_or_else(|| {
            "no /dev/dri/renderD* or /dev/dri/card* node initialized through VA-API".to_owned()
        })?;
        validate_h264_display(&display)?;
        let probe_frame = VaFrame::new(Resolution::from((16, 16)), Resolution::from((16, 16)));
        drop(
            probe_frame
                .to_native_handle(&display)
                .map_err(|error| format!("VA-API cannot allocate an NV12 user surface: {error}"))?,
        );
        let vendor = display
            .query_vendor_string()
            .unwrap_or_else(|_| "VA-API H.264 device".to_owned());
        drop(open_h264_decoder(Arc::clone(&display))?);
        Ok(vendor)
    }

    pub fn open(format: StreamFormat) -> Result<Self> {
        format.validate()?;
        let display = Display::open().ok_or_else(|| {
            Error::unavailable(
                Subsystem::VaApi,
                "no DRM node could be initialized through VA-API",
            )
        })?;
        validate_h264_display(&display)
            .map_err(|error| Error::unavailable(Subsystem::VaApi, error))?;
        let decoder = open_h264_decoder(display)
            .map_err(|error| Error::unavailable(Subsystem::VaApi, error))?;
        let resolution = Resolution::from((format.width, format.height));
        Ok(Self {
            decoder,
            requested_format: format,
            allocation_visible: resolution,
            allocation_coded: resolution,
            pending_format_change: None,
        })
    }

    fn drain_events(&mut self) -> Result<Vec<DecodedVideoFrame>> {
        let mut frames = Vec::new();
        while let Some(event) = self.decoder.next_event() {
            match event {
                DecoderEvent::FormatChanged => {
                    let info = self.decoder.stream_info().ok_or_else(|| {
                        Error::backend(Subsystem::VaApi, "format change had no stream info")
                    })?;
                    self.allocation_visible = info.display_resolution;
                    self.allocation_coded = info.coded_resolution;
                    self.requested_format.width = info.display_resolution.width;
                    self.requested_format.height = info.display_resolution.height;
                    self.requested_format.pixel_format = PixelFormat::Nv12;
                    self.pending_format_change = Some(self.requested_format);
                }
                DecoderEvent::FrameReady(handle) => {
                    handle
                        .sync()
                        .map_err(|error| Error::backend(Subsystem::VaApi, error.to_string()))?;
                    let timestamp_us = handle.timestamp();
                    let visible = handle.display_resolution();
                    let frame = handle.video_frame();
                    let exported = frame
                        .exported()
                        .map_err(|error| Error::backend(Subsystem::VaApi, error))?;
                    let objects = exported
                        .objects
                        .iter()
                        .map(|object| DmaBufObject {
                            fd: object.fd.as_raw_fd(),
                            size: object.size as usize,
                            format_modifier: object.drm_format_modifier,
                        })
                        .collect();
                    let layers = exported
                        .layers
                        .iter()
                        .map(|layer| DmaBufLayer {
                            format: layer.drm_format,
                            planes: (0..layer.num_planes.min(4) as usize)
                                .map(|index| DmaBufPlane {
                                    object_index: layer.object_index[index] as usize,
                                    offset: layer.offset[index] as usize,
                                    pitch: layer.pitch[index] as usize,
                                })
                                .collect(),
                        })
                        .collect();
                    let dmabuf = DmaBufFrame::new(objects, layers, frame);
                    let decoded = DecodedVideoFrame {
                        format: StreamFormat {
                            width: visible.width,
                            height: visible.height,
                            pixel_format: PixelFormat::Nv12,
                            chroma_location: ChromaLocation::Left,
                            ..self.requested_format
                        },
                        planes: Vec::new(),
                        dmabuf: Some(Arc::new(dmabuf)),
                        vulkan: None,
                        overlay: None,
                        timestamp_us,
                    };
                    decoded.validate()?;
                    frames.push(decoded);
                }
            }
        }
        Ok(frames)
    }
}

type H264VaapiDecoder = StatelessDecoder<H264, VaapiBackend<VaFrame>>;

fn open_h264_decoder(display: Arc<Display>) -> std::result::Result<H264VaapiDecoder, String> {
    // cros-codecs creates a 16x16 context without render targets in its
    // constructor. Some drivers advertise H.264 and allocate surfaces but
    // reject that context shape (notably nvidia-vaapi-driver). Exercise the
    // exact operation first so an incompatible driver becomes a normal
    // capability failure instead of reaching the dependency's `expect`.
    validate_initial_context(&display)?;
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        H264VaapiDecoder::new_vaapi(display, cros_codecs::decoder::BlockingMode::NonBlocking)
    }))
    .map_err(|_| "VA-API decoder initialization panicked".to_owned())?
    .map_err(|error| error.to_string())
}

fn validate_initial_context(display: &Arc<Display>) -> std::result::Result<(), String> {
    let config = display
        .create_config(
            vec![libva::VAConfigAttrib {
                type_: libva::VAConfigAttribType::VAConfigAttribRTFormat,
                value: libva::VA_RT_FORMAT_YUV420,
            }],
            libva::VAProfile::VAProfileH264Main,
            libva::VAEntrypoint::VAEntrypointVLD,
        )
        .map_err(|error| format!("VA-API cannot create the H.264 decoder config: {error}"))?;
    drop(
        display
            .create_context::<()>(&config, 16, 16, None, true)
            .map_err(|error| {
                format!("VA-API cannot create the initial H.264 decoder context: {error}")
            })?,
    );
    Ok(())
}

impl VideoDecoder for VaApiDecoder {
    fn decode(&mut self, frame: &EncodedVideoFrame) -> Result<Vec<DecodedVideoFrame>> {
        let mut offset = 0;
        let mut output = Vec::new();
        let mut retries = 0;
        while offset < frame.data.len() {
            let visible = self.allocation_visible;
            let coded = self.allocation_coded;
            let mut allocate = || Some(VaFrame::new(visible, coded));
            match self
                .decoder
                .decode(frame.timestamp_us, &frame.data[offset..], &mut allocate)
            {
                Ok(consumed) if consumed > 0 => {
                    offset += consumed;
                    retries = 0;
                    output.extend(self.drain_events()?);
                }
                Ok(_) => {
                    return Err(Error::backend(
                        Subsystem::VaApi,
                        "decoder accepted zero bytes",
                    ));
                }
                Err(DecodeError::CheckEvents | DecodeError::NotEnoughOutputBuffers(_)) => {
                    output.extend(self.drain_events()?);
                    retries += 1;
                    if retries > MAX_DECODE_RETRIES {
                        return Err(Error::backend(
                            Subsystem::VaApi,
                            "decoder made no progress after handling pending events",
                        ));
                    }
                }
                Err(error) => {
                    return Err(Error::backend(Subsystem::VaApi, error.to_string()));
                }
            }
        }
        output.extend(self.drain_events()?);
        Ok(output)
    }

    fn flush(&mut self) -> Result<Vec<DecodedVideoFrame>> {
        self.decoder
            .flush()
            .map_err(|error| Error::backend(Subsystem::VaApi, error.to_string()))?;
        self.drain_events()
    }

    fn take_format_change(&mut self) -> Option<StreamFormat> {
        self.pending_format_change.take()
    }
}

const fn align(value: u32, alignment: u32) -> u32 {
    value.div_ceil(alignment) * alignment
}

fn validate_h264_display(display: &Display) -> std::result::Result<(), String> {
    let profiles = display
        .query_config_profiles()
        .map_err(|error| error.to_string())?;
    if !profiles.contains(&libva::VAProfile::VAProfileH264Main) {
        return Err("VA-API device exposes no H.264 Main profile".to_owned());
    }
    let entrypoints = display
        .query_config_entrypoints(libva::VAProfile::VAProfileH264Main)
        .map_err(|error| error.to_string())?;
    if !entrypoints.contains(&libva::VAEntrypoint::VAEntrypointVLD) {
        return Err("VA-API device exposes no H.264 VLD entrypoint".to_owned());
    }
    Ok(())
}
