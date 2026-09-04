use std::collections::VecDeque;
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::mem::size_of;
use std::ptr;

use ::windows::Win32::Foundation::{E_NOTIMPL, LUID};
use ::windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use ::windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFAttributes, IMFDXGIBuffer, IMFMediaEventGenerator, IMFSample, IMFTransform,
    METransformHaveOutput, METransformNeedInput, MF_E_NO_EVENTS_AVAILABLE,
    MF_E_TRANSFORM_NEED_MORE_INPUT, MF_E_TRANSFORM_STREAM_CHANGE, MF_EVENT_FLAG_NO_WAIT,
    MF_LOW_LATENCY, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE, MF_MT_INTERLACE_MODE,
    MF_MT_MAJOR_TYPE, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_MT_VIDEO_NOMINAL_RANGE,
    MF_SA_D3D11_AWARE, MF_TRANSFORM_ASYNC, MF_TRANSFORM_ASYNC_UNLOCK, MFCreateAttributes,
    MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Video,
    MFNominalRange_0_255, MFSampleExtension_CleanPoint, MFT_CATEGORY_VIDEO_DECODER,
    MFT_ENUM_ADAPTER_LUID, MFT_ENUM_FLAG, MFT_ENUM_FLAG_ASYNCMFT, MFT_ENUM_FLAG_HARDWARE,
    MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_FLAG_SYNCMFT, MFT_ENUM_HARDWARE_URL_Attribute,
    MFT_FRIENDLY_NAME_Attribute, MFT_MESSAGE_COMMAND_FLUSH, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_END_OF_STREAM, MFT_MESSAGE_NOTIFY_END_STREAMING,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_MESSAGE_SET_D3D_MANAGER, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES, MFT_OUTPUT_STREAM_PROVIDES_SAMPLES,
    MFT_REGISTER_TYPE_INFO, MFTEnum2, MFVideoFormat_AV1, MFVideoFormat_AYUV, MFVideoFormat_H264,
    MFVideoFormat_HEVC, MFVideoFormat_NV12, MFVideoFormat_P010, MFVideoFormat_Y410,
    MFVideoInterlace_MixedInterlaceOrProgressive,
};
use ::windows::Win32::System::Com::CoTaskMemFree;
use ::windows::core::Interface;

use crate::queue::BoundedQueue;
use crate::{
    ADAPTIVE_VIDEO_QUEUE_CAPACITY, BackendEvent, EncodedVideoFrame, Subsystem, VideoChromaFormat,
    VideoCodec, VideoFormat, VideoPixelFormat, WindowsDecoderMode,
};

pub(super) trait DecoderDevice {
    fn device_manager(&self) -> &::windows::Win32::Media::MediaFoundation::IMFDXGIDeviceManager;
    fn adapter_luid(&self) -> Result<LUID, String>;
    fn video_format(&self) -> VideoFormat;
}

pub(super) struct DecodedVideoFrame {
    pub(super) texture: ID3D11Texture2D,
    pub(super) subresource: u32,
    pub(super) timestamp_100ns: i64,
    pub(super) duration_100ns: i64,
}

pub(super) struct Decoder {
    events: Option<IMFMediaEventGenerator>,
    transform: IMFTransform,
    activation: IMFActivate,
    input_stream: u32,
    output_stream: u32,
    input_credits: u32,
    format: VideoFormat,
    preferred_pixel_format: VideoPixelFormat,
    output_provides_samples: bool,
    stopped: bool,
}

impl Decoder {
    pub(super) fn probe<G: DecoderDevice>(
        graphics: &G,
        codec: VideoCodec,
        mode: WindowsDecoderMode,
    ) -> Result<(), String> {
        let mut decoder = Self::new(
            graphics,
            VideoFormat {
                codec,
                ..graphics.video_format()
            },
            mode,
        )?;
        decoder.stop();
        Ok(())
    }

    pub(super) fn new<G: DecoderDevice>(
        graphics: &G,
        format: VideoFormat,
        mode: WindowsDecoderMode,
    ) -> Result<Self, String> {
        let activations = enumerate_decoders(graphics.adapter_luid()?, format.codec, mode)?;
        let mut failures = Vec::new();
        for activation in activations {
            match configure_transform(&activation, graphics, format) {
                Ok((
                    transform,
                    events,
                    input_stream,
                    output_stream,
                    output_provides_samples,
                    pixel_format,
                    full_range,
                )) => {
                    let input_credits = u32::from(events.is_none());
                    let friendly_name =
                        activation_string(&activation, &MFT_FRIENDLY_NAME_Attribute)
                            .unwrap_or_else(|| "unknown".to_owned());
                    let hardware_registered =
                        activation_string(&activation, &MFT_ENUM_HARDWARE_URL_Attribute).is_some();
                    eprintln!(
                        "Windows decoder configured codec={} mft={friendly_name:?} hardwareRegistered={hardware_registered} async={} output={pixel_format:?} bitDepth={} chroma={:?} range={}",
                        format.codec.label(),
                        events.is_some(),
                        pixel_format.bit_depth(),
                        chroma_format(pixel_format),
                        if full_range { "full" } else { "limited" },
                    );
                    return Ok(Self {
                        events,
                        transform,
                        activation,
                        input_stream,
                        output_stream,
                        input_credits,
                        format: VideoFormat {
                            pixel_format,
                            chroma_format: chroma_format(pixel_format),
                            full_range,
                            ..format
                        },
                        preferred_pixel_format: format.pixel_format,
                        output_provides_samples,
                        stopped: false,
                    });
                }
                Err(error) => {
                    failures.push(error);
                    unsafe {
                        let _ = activation.ShutdownObject();
                    }
                }
            }
        }
        Err(if failures.is_empty() {
            format!(
                "no D3D11-aware {} decoder MFT is registered",
                format.codec.label()
            )
        } else {
            format!(
                "registered {} decoders rejected the D3D11 configuration: {}",
                format.codec.label(),
                failures.join("; ")
            )
        })
    }

    pub(super) fn wants_input(&self) -> bool {
        self.input_credits > 0 && !self.stopped
    }

    pub(super) fn format(&self) -> VideoFormat {
        self.format
    }

    pub(super) fn submit(&mut self, frame: EncodedVideoFrame) -> Result<(), String> {
        if self.input_credits == 0 {
            return Err("decoder input was submitted without a NeedInput credit".to_owned());
        }
        unsafe {
            if frame.codec != self.format.codec {
                return Err(format!(
                    "{} decoder received a {} access unit",
                    self.format.codec.label(),
                    frame.codec.label()
                ));
            }
            let buffer =
                MFCreateMemoryBuffer(frame.data.len() as u32).map_err(|error| error.to_string())?;
            let mut destination = ptr::null_mut();
            buffer
                .Lock(&mut destination, None, None)
                .map_err(|error| error.to_string())?;
            ptr::copy_nonoverlapping(frame.data.as_ptr(), destination, frame.data.len());
            let unlock_result = buffer.Unlock();
            unlock_result.map_err(|error| error.to_string())?;
            buffer
                .SetCurrentLength(frame.data.len() as u32)
                .map_err(|error| error.to_string())?;
            let sample = MFCreateSample().map_err(|error| error.to_string())?;
            sample
                .AddBuffer(&buffer)
                .map_err(|error| error.to_string())?;
            sample
                .SetSampleTime(frame.timestamp_100ns)
                .map_err(|error| error.to_string())?;
            sample
                .SetSampleDuration(frame.duration_100ns)
                .map_err(|error| error.to_string())?;
            if frame.key_frame {
                sample
                    .SetUINT32(&MFSampleExtension_CleanPoint, 1)
                    .map_err(|error| error.to_string())?;
            }
            self.transform
                .ProcessInput(self.input_stream, &sample, 0)
                .map_err(|error| error.to_string())?;
        }
        self.input_credits -= 1;
        Ok(())
    }

    pub(super) fn poll_output(
        &mut self,
        decoded_frames: &mut VecDeque<DecodedVideoFrame>,
        event_queue: &BoundedQueue<BackendEvent>,
    ) -> Result<usize, String> {
        let mut produced = 0;
        if let Some(events) = self.events.clone() {
            loop {
                let event = match unsafe { events.GetEvent(MF_EVENT_FLAG_NO_WAIT) } {
                    Ok(event) => event,
                    Err(error) if error.code() == MF_E_NO_EVENTS_AVAILABLE => break,
                    Err(error) => return Err(error.to_string()),
                };
                let status = unsafe { event.GetStatus().map_err(|error| error.to_string())? };
                status.ok().map_err(|error| error.to_string())?;
                let event_type = unsafe { event.GetType().map_err(|error| error.to_string())? };
                if event_type == METransformNeedInput.0 as u32 {
                    self.input_credits = self.input_credits.saturating_add(1);
                } else if event_type == METransformHaveOutput.0 as u32 {
                    if self.process_output(decoded_frames, event_queue)? == OutputPoll::Produced {
                        produced += 1;
                    }
                    // Keep decode and presentation interleaved on the shared
                    // media worker. Draining every pending MFT output here can
                    // create an artificial burst that looks stale even when
                    // the source cadence and display cadence are healthy.
                    break;
                }
            }
        } else {
            if self.process_output(decoded_frames, event_queue)? == OutputPoll::Produced {
                produced += 1;
            }
            self.input_credits = 1;
        }
        Ok(produced)
    }

    fn process_output(
        &mut self,
        decoded_frames: &mut VecDeque<DecodedVideoFrame>,
        event_queue: &BoundedQueue<BackendEvent>,
    ) -> Result<OutputPoll, String> {
        let mut output = MFT_OUTPUT_DATA_BUFFER {
            dwStreamID: self.output_stream,
            ..Default::default()
        };
        let mut status = 0;
        let result = unsafe {
            self.transform
                .ProcessOutput(0, std::slice::from_mut(&mut output), &mut status)
        };
        let sample = unsafe { ManuallyDrop::take(&mut output.pSample) };
        let output_events = unsafe { ManuallyDrop::take(&mut output.pEvents) };
        drop(output_events);

        if let Err(error) = result {
            drop(sample);
            if error.code() == MF_E_TRANSFORM_NEED_MORE_INPUT {
                return Ok(OutputPoll::NeedsInput);
            }
            if error.code() == MF_E_TRANSFORM_STREAM_CHANGE {
                let pixel_format = self.select_video_output_type()?;
                let (dimensions, full_range) = self.output_format()?;
                let updated = VideoFormat {
                    width: dimensions.0,
                    height: dimensions.1,
                    pixel_format,
                    chroma_format: chroma_format(pixel_format),
                    full_range,
                    ..self.format
                };
                self.format = updated;
                let _ = event_queue.push(BackendEvent::VideoFormatChanged(updated));
                return Ok(OutputPoll::Produced);
            }
            return Err(error.to_string());
        }

        let sample = sample.ok_or_else(|| {
            if self.output_provides_samples {
                "hardware decoder reported output without a sample".to_owned()
            } else {
                "hardware decoder requires caller-allocated output samples".to_owned()
            }
        })?;
        let (texture, subresource) = dxgi_surface(&sample)?;
        let timestamp_100ns = unsafe { sample.GetSampleTime().unwrap_or(0) };
        let duration_100ns = unsafe {
            sample
                .GetSampleDuration()
                .ok()
                .filter(|duration| *duration > 0)
                .unwrap_or_else(|| self.format.frame_duration_100ns())
        };
        if decoded_frames.len() == ADAPTIVE_VIDEO_QUEUE_CAPACITY {
            decoded_frames.pop_front();
            let _ = event_queue.push(BackendEvent::QueueOverflow(Subsystem::VideoPresentation));
        }
        decoded_frames.push_back(DecodedVideoFrame {
            texture,
            subresource,
            timestamp_100ns,
            duration_100ns,
        });
        Ok(OutputPoll::Produced)
    }

    pub(super) fn stop(&mut self) {
        if self.stopped {
            return;
        }
        unsafe {
            let _ = self
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self.transform.ProcessMessage(MFT_MESSAGE_COMMAND_FLUSH, 0);
            let _ = self
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            let _ = self.activation.ShutdownObject();
        }
        self.input_credits = 0;
        self.stopped = true;
    }

    fn select_video_output_type(&self) -> Result<VideoPixelFormat, String> {
        select_video_output_type(
            &self.transform,
            self.output_stream,
            self.preferred_pixel_format,
        )
    }

    fn output_format(&self) -> Result<((u32, u32), bool), String> {
        unsafe {
            let media_type = self
                .transform
                .GetOutputCurrentType(self.output_stream)
                .map_err(|error| error.to_string())?;
            let packed = media_type
                .GetUINT64(&MF_MT_FRAME_SIZE)
                .map_err(|error| error.to_string())?;
            let nominal_range = media_type
                .GetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE)
                .unwrap_or_default();
            Ok((
                ((packed >> 32) as u32, packed as u32),
                nominal_range == MFNominalRange_0_255.0 as u32,
            ))
        }
    }
}

fn activation_string(activation: &IMFActivate, key: &::windows::core::GUID) -> Option<String> {
    unsafe {
        let length = activation.GetStringLength(key).ok()? as usize;
        let mut value = vec![0_u16; length.saturating_add(1)];
        activation.GetString(key, &mut value, None).ok()?;
        String::from_utf16(&value[..length]).ok()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum OutputPoll {
    Produced,
    NeedsInput,
}

impl Drop for Decoder {
    fn drop(&mut self) {
        self.stop();
    }
}

type ConfiguredTransform = (
    IMFTransform,
    Option<IMFMediaEventGenerator>,
    u32,
    u32,
    bool,
    VideoPixelFormat,
    bool,
);

fn configure_transform<G: DecoderDevice>(
    activation: &IMFActivate,
    graphics: &G,
    format: VideoFormat,
) -> Result<ConfiguredTransform, String> {
    unsafe {
        let transform: IMFTransform = activation
            .ActivateObject()
            .map_err(|error| error.to_string())?;
        let attributes = transform
            .GetAttributes()
            .map_err(|error| error.to_string())?;
        let asynchronous = attributes.GetUINT32(&MF_TRANSFORM_ASYNC).unwrap_or(0) != 0;
        let d3d11_aware = attributes.GetUINT32(&MF_SA_D3D11_AWARE).unwrap_or(0) != 0;
        if !d3d11_aware {
            return Err("MFT is not D3D11-aware".to_owned());
        }
        attributes
            .SetUINT32(&MF_LOW_LATENCY, 1)
            .map_err(|error| format!("decoder low-latency mode: {error}"))?;
        if asynchronous {
            attributes
                .SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)
                .map_err(|error| error.to_string())?;
        }
        transform
            .ProcessMessage(
                MFT_MESSAGE_SET_D3D_MANAGER,
                graphics.device_manager().as_raw() as usize,
            )
            .map_err(|error| error.to_string())?;

        let (input_stream, output_stream) = stream_ids(&transform)?;
        let input_type = MFCreateMediaType().map_err(|error| error.to_string())?;
        input_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(|error| error.to_string())?;
        input_type
            .SetGUID(&MF_MT_SUBTYPE, video_subtype(format.codec))
            .map_err(|error| error.to_string())?;
        input_type
            .SetUINT64(&MF_MT_FRAME_SIZE, pack_pair(format.width, format.height))
            .map_err(|error| error.to_string())?;
        input_type
            .SetUINT64(
                &MF_MT_FRAME_RATE,
                pack_pair(
                    format.frame_rate_numerator.get(),
                    format.frame_rate_denominator.get(),
                ),
            )
            .map_err(|error| error.to_string())?;
        input_type
            .SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack_pair(1, 1))
            .map_err(|error| error.to_string())?;
        input_type
            .SetUINT32(
                &MF_MT_INTERLACE_MODE,
                MFVideoInterlace_MixedInterlaceOrProgressive.0 as u32,
            )
            .map_err(|error| error.to_string())?;
        input_type
            .SetUINT32(&MF_MT_AVG_BITRATE, format.average_bitrate)
            .map_err(|error| error.to_string())?;
        transform
            .SetInputType(input_stream, &input_type, 0)
            .map_err(|error| error.to_string())?;
        let pixel_format =
            select_video_output_type(&transform, output_stream, format.pixel_format)?;
        let full_range = output_full_range(&transform, output_stream);

        let stream_info = transform
            .GetOutputStreamInfo(output_stream)
            .map_err(|error| error.to_string())?;
        let provider_flags = MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 as u32
            | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0 as u32;
        if stream_info.dwFlags & provider_flags == 0 {
            return Err("MFT cannot allocate D3D11 output samples".to_owned());
        }
        let events = asynchronous
            .then(|| transform.cast::<IMFMediaEventGenerator>())
            .transpose()
            .map_err(|error| error.to_string())?;
        transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_BEGIN_STREAMING, 0)
            .map_err(|error| error.to_string())?;
        transform
            .ProcessMessage(MFT_MESSAGE_NOTIFY_START_OF_STREAM, 0)
            .map_err(|error| error.to_string())?;
        Ok((
            transform,
            events,
            input_stream,
            output_stream,
            true,
            pixel_format,
            full_range,
        ))
    }
}

fn enumerate_decoders(
    adapter_luid: LUID,
    codec: VideoCodec,
    mode: WindowsDecoderMode,
) -> Result<Vec<IMFActivate>, String> {
    let registered = MFT_ENUM_FLAG(
        MFT_ENUM_FLAG_SYNCMFT.0 | MFT_ENUM_FLAG_ASYNCMFT.0 | MFT_ENUM_FLAG_SORTANDFILTER.0,
    );
    unsafe {
        match mode {
            WindowsDecoderMode::Hardware => {
                let mut attributes = None;
                MFCreateAttributes(&mut attributes, 1).map_err(|error| error.to_string())?;
                let attributes =
                    attributes.ok_or("MFCreateAttributes returned no attribute store")?;
                let luid_bytes = std::slice::from_raw_parts(
                    (&adapter_luid as *const LUID).cast::<u8>(),
                    size_of::<LUID>(),
                );
                attributes
                    .SetBlob(&MFT_ENUM_ADAPTER_LUID, luid_bytes)
                    .map_err(|error| error.to_string())?;
                let mut activations = enumerate_matching_decoders(
                    MFT_ENUM_FLAG(MFT_ENUM_FLAG_HARDWARE.0 | MFT_ENUM_FLAG_SORTANDFILTER.0),
                    Some(&attributes),
                    codec,
                )?;
                // NVIDIA and AMD drive decode through DXVA2 instead of registering
                // a hardware-flagged MFT, so on those adapters the enumeration above
                // is empty and the D3D11-aware Microsoft MFT is the GPU path. Keep it
                // as a fallback rather than reporting no hardware decode at all;
                // configure_transform still rejects any MFT that cannot accept our
                // D3D manager, so a genuinely software-only MFT never gets through.
                activations.extend(enumerate_matching_decoders(registered, None, codec)?);
                Ok(activations)
            }
            WindowsDecoderMode::Software => enumerate_matching_decoders(registered, None, codec),
        }
    }
}

unsafe fn enumerate_matching_decoders(
    flags: MFT_ENUM_FLAG,
    attributes: Option<&IMFAttributes>,
    codec: VideoCodec,
) -> Result<Vec<IMFActivate>, String> {
    let input = MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: *video_subtype(codec),
    };
    let mut entries: *mut Option<IMFActivate> = ptr::null_mut();
    let mut count = 0;
    unsafe {
        MFTEnum2(
            MFT_CATEGORY_VIDEO_DECODER,
            flags,
            Some(&input),
            None,
            attributes,
            &mut entries,
            &mut count,
        )
        .map_err(|error| error.to_string())?;
    }
    let mut activations = Vec::with_capacity(count as usize);
    if !entries.is_null() {
        unsafe {
            for index in 0..count as usize {
                if let Some(activation) = (*entries.add(index)).take() {
                    activations.push(activation);
                }
            }
            CoTaskMemFree(Some(entries as *const c_void));
        }
    }
    Ok(activations)
}

fn stream_ids(transform: &IMFTransform) -> Result<(u32, u32), String> {
    unsafe {
        let mut input_count = 0;
        let mut output_count = 0;
        transform
            .GetStreamCount(&mut input_count, &mut output_count)
            .map_err(|error| error.to_string())?;
        if input_count != 1 || output_count != 1 {
            return Err(format!(
                "expected one input and output stream, got {input_count} and {output_count}"
            ));
        }
        let mut input = [0];
        let mut output = [0];
        if let Err(error) = transform.GetStreamIDs(&mut input, &mut output) {
            if error.code() == E_NOTIMPL {
                return Ok((0, 0));
            }
            return Err(error.to_string());
        }
        Ok((input[0], output[0]))
    }
}

fn select_video_output_type(
    transform: &IMFTransform,
    output_stream: u32,
    preferred: VideoPixelFormat,
) -> Result<VideoPixelFormat, String> {
    let mut supported = Vec::new();
    for index in 0..64 {
        let media_type = match unsafe { transform.GetOutputAvailableType(output_stream, index) } {
            Ok(media_type) => media_type,
            Err(_) if !supported.is_empty() => break,
            Err(error) => return Err(format!("video output type is unavailable: {error}")),
        };
        let subtype = unsafe {
            media_type
                .GetGUID(&MF_MT_SUBTYPE)
                .map_err(|error| error.to_string())?
        };
        if let Some(pixel_format) = pixel_format_from_subtype(subtype) {
            supported.push((pixel_format, media_type));
        }
    }
    let exposed = supported
        .iter()
        .map(|(format, _)| format!("{format:?}"))
        .collect::<Vec<_>>()
        .join(", ");
    let mut rejected = Vec::new();
    for candidate in output_format_preferences(preferred) {
        for (pixel_format, media_type) in &supported {
            if pixel_format != candidate {
                continue;
            }
            match unsafe { transform.SetOutputType(output_stream, media_type, 0) } {
                Ok(()) => {
                    if *pixel_format != preferred {
                        eprintln!(
                            "Windows decoder requested {preferred:?}, temporarily using compatible {pixel_format:?} output"
                        );
                    }
                    return Ok(*pixel_format);
                }
                Err(error) => rejected.push(format!("{pixel_format:?}: {error}")),
            }
        }
    }
    let suffix = if rejected.is_empty() {
        String::new()
    } else {
        format!("; rejected types: {}", rejected.join("; "))
    };
    Err(format!(
        "hardware decoder could not configure requested {preferred:?} output (exposed: {exposed}){suffix}"
    ))
}

fn output_format_preferences(preferred: VideoPixelFormat) -> &'static [VideoPixelFormat] {
    match preferred {
        VideoPixelFormat::Nv12 => &[VideoPixelFormat::Nv12],
        // Some HEVC/AV1 MFTs advertise NV12 until the first sequence header is
        // parsed, then raise MF_E_TRANSFORM_STREAM_CHANGE and expose P010. Let
        // startup proceed in NV12 and keep preferring P010 on every reselection.
        VideoPixelFormat::P010 => &[VideoPixelFormat::P010, VideoPixelFormat::Nv12],
        VideoPixelFormat::Ayuv => &[VideoPixelFormat::Ayuv, VideoPixelFormat::Nv12],
        VideoPixelFormat::Y410 => &[
            VideoPixelFormat::Y410,
            VideoPixelFormat::P010,
            VideoPixelFormat::Ayuv,
            VideoPixelFormat::Nv12,
        ],
    }
}

fn pixel_format_from_subtype(subtype: ::windows::core::GUID) -> Option<VideoPixelFormat> {
    if subtype == MFVideoFormat_NV12 {
        Some(VideoPixelFormat::Nv12)
    } else if subtype == MFVideoFormat_P010 {
        Some(VideoPixelFormat::P010)
    } else if subtype == MFVideoFormat_AYUV {
        Some(VideoPixelFormat::Ayuv)
    } else if subtype == MFVideoFormat_Y410 {
        Some(VideoPixelFormat::Y410)
    } else {
        None
    }
}

fn chroma_format(pixel_format: VideoPixelFormat) -> VideoChromaFormat {
    match pixel_format {
        VideoPixelFormat::Nv12 | VideoPixelFormat::P010 => VideoChromaFormat::Cs420,
        VideoPixelFormat::Ayuv | VideoPixelFormat::Y410 => VideoChromaFormat::Cs444,
    }
}

fn output_full_range(transform: &IMFTransform, output_stream: u32) -> bool {
    unsafe {
        transform
            .GetOutputCurrentType(output_stream)
            .ok()
            .and_then(|media_type| media_type.GetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE).ok())
            .is_some_and(|range| range == MFNominalRange_0_255.0 as u32)
    }
}

fn video_subtype(codec: VideoCodec) -> &'static ::windows::core::GUID {
    match codec {
        VideoCodec::H264 => &MFVideoFormat_H264,
        VideoCodec::H265 => &MFVideoFormat_HEVC,
        VideoCodec::Av1 => &MFVideoFormat_AV1,
    }
}

fn dxgi_surface(sample: &IMFSample) -> Result<(ID3D11Texture2D, u32), String> {
    unsafe {
        let buffer = sample
            .GetBufferByIndex(0)
            .map_err(|error| error.to_string())?;
        let dxgi_buffer: IMFDXGIBuffer = buffer.cast().map_err(|_| {
            "hardware decoder returned a system-memory buffer instead of a D3D11 surface".to_owned()
        })?;
        let mut resource = ptr::null_mut();
        dxgi_buffer
            .GetResource(&ID3D11Texture2D::IID, &mut resource)
            .map_err(|error| error.to_string())?;
        if resource.is_null() {
            return Err("decoder returned a null D3D11 texture".to_owned());
        }
        let texture = ID3D11Texture2D::from_raw(resource);
        let subresource = dxgi_buffer
            .GetSubresourceIndex()
            .map_err(|error| error.to_string())?;
        Ok((texture, subresource))
    }
}

fn pack_pair(high: u32, low: u32) -> u64 {
    ((high as u64) << 32) | low as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_foundation_subtypes_preserve_depth_and_chroma() {
        let cases = [
            (
                MFVideoFormat_NV12,
                VideoPixelFormat::Nv12,
                VideoChromaFormat::Cs420,
            ),
            (
                MFVideoFormat_P010,
                VideoPixelFormat::P010,
                VideoChromaFormat::Cs420,
            ),
            (
                MFVideoFormat_AYUV,
                VideoPixelFormat::Ayuv,
                VideoChromaFormat::Cs444,
            ),
            (
                MFVideoFormat_Y410,
                VideoPixelFormat::Y410,
                VideoChromaFormat::Cs444,
            ),
        ];
        for (subtype, expected_pixel, expected_chroma) in cases {
            let pixel = pixel_format_from_subtype(subtype).expect("supported subtype");
            assert_eq!(pixel, expected_pixel);
            assert_eq!(chroma_format(pixel), expected_chroma);
        }
        assert_eq!(VideoPixelFormat::Y410.bit_depth(), 10);
    }

    #[test]
    fn ten_bit_output_preferences_allow_safe_decoder_startup_fallbacks() {
        assert_eq!(
            output_format_preferences(VideoPixelFormat::P010),
            &[VideoPixelFormat::P010, VideoPixelFormat::Nv12]
        );
        assert_eq!(
            output_format_preferences(VideoPixelFormat::Y410),
            &[
                VideoPixelFormat::Y410,
                VideoPixelFormat::P010,
                VideoPixelFormat::Ayuv,
                VideoPixelFormat::Nv12,
            ]
        );
        assert_eq!(
            output_format_preferences(VideoPixelFormat::Nv12),
            &[VideoPixelFormat::Nv12]
        );
    }
}
