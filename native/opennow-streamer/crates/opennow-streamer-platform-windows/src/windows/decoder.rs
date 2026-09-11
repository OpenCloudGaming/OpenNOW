use std::collections::VecDeque;
use std::ffi::c_void;
use std::mem::ManuallyDrop;
use std::mem::size_of;
use std::ptr;

use ::windows::Win32::Foundation::{E_NOTIMPL, LUID};
use ::windows::Win32::Graphics::Direct3D11::{
    D3D11_DECODER_PROFILE_AV1_VLD_PROFILE0, D3D11_DECODER_PROFILE_H264_VLD_FGT,
    D3D11_DECODER_PROFILE_H264_VLD_NOFGT, D3D11_DECODER_PROFILE_HEVC_VLD_MAIN,
    D3D11_DECODER_PROFILE_HEVC_VLD_MAIN10, D3D11_TEXTURE2D_DESC, D3D11_VIDEO_DECODER_DESC,
    ID3D11Texture2D, ID3D11VideoDevice,
};
use ::windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_AYUV, DXGI_FORMAT_NV12, DXGI_FORMAT_P010, DXGI_FORMAT_Y410,
};
use ::windows::Win32::Media::MediaFoundation::{
    D3D12_VIDEO_DECODE_PROFILE_HEVC_MAIN_444, D3D12_VIDEO_DECODE_PROFILE_HEVC_MAIN10_444,
    MF_MT_TRANSFER_FUNCTION, MF_MT_VIDEO_CHROMA_SITING, MF_MT_VIDEO_PRIMARIES, MF_MT_YUV_MATRIX,
    MFNominalRange_16_235, MFVideoChromaSubsampling_Cosited, MFVideoChromaSubsampling_MPEG2,
    MFVideoChromaSubsampling_ProgressiveChroma, MFVideoPrimaries_BT709, MFVideoPrimaries_BT2020,
    MFVideoTransFunc_22, MFVideoTransFunc_709, MFVideoTransFunc_2020, MFVideoTransFunc_2084,
    MFVideoTransFunc_HLG, MFVideoTransFunc_sRGB, MFVideoTransferMatrix_BT601,
    MFVideoTransferMatrix_BT709, MFVideoTransferMatrix_BT2020_10,
};
use ::windows::Win32::Media::MediaFoundation::{
    IMFActivate, IMFAttributes, IMFDXGIBuffer, IMFMediaEventGenerator, IMFMediaType, IMFSample,
    IMFTransform, METransformHaveOutput, METransformNeedInput, MF_E_ATTRIBUTENOTFOUND,
    MF_E_NO_EVENTS_AVAILABLE, MF_E_TRANSFORM_NEED_MORE_INPUT, MF_E_TRANSFORM_STREAM_CHANGE,
    MF_EVENT_FLAG_NO_WAIT, MF_LOW_LATENCY, MF_MT_AVG_BITRATE, MF_MT_FRAME_RATE, MF_MT_FRAME_SIZE,
    MF_MT_GEOMETRIC_APERTURE, MF_MT_INTERLACE_MODE, MF_MT_MAJOR_TYPE,
    MF_MT_MINIMUM_DISPLAY_APERTURE, MF_MT_MPEG2_PROFILE, MF_MT_PAN_SCAN_APERTURE,
    MF_MT_PAN_SCAN_ENABLED, MF_MT_PIXEL_ASPECT_RATIO, MF_MT_SUBTYPE, MF_MT_VIDEO_NOMINAL_RANGE,
    MF_SA_D3D11_AWARE, MF_TRANSFORM_ASYNC, MF_TRANSFORM_ASYNC_UNLOCK, MFCreateAttributes,
    MFCreateMediaType, MFCreateMemoryBuffer, MFCreateSample, MFMediaType_Video,
    MFNominalRange_0_255, MFSampleExtension_CleanPoint, MFSampleExtension_FrameCorruption,
    MFT_CATEGORY_VIDEO_DECODER, MFT_ENUM_ADAPTER_LUID, MFT_ENUM_FLAG, MFT_ENUM_FLAG_ASYNCMFT,
    MFT_ENUM_FLAG_HARDWARE, MFT_ENUM_FLAG_SORTANDFILTER, MFT_ENUM_FLAG_SYNCMFT,
    MFT_ENUM_HARDWARE_URL_Attribute, MFT_FRIENDLY_NAME_Attribute, MFT_MESSAGE_COMMAND_DRAIN,
    MFT_MESSAGE_COMMAND_FLUSH, MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
    MFT_MESSAGE_NOTIFY_END_OF_STREAM, MFT_MESSAGE_NOTIFY_END_STREAMING,
    MFT_MESSAGE_NOTIFY_START_OF_STREAM, MFT_MESSAGE_SET_D3D_MANAGER, MFT_OUTPUT_DATA_BUFFER,
    MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES, MFT_OUTPUT_STREAM_PROVIDES_SAMPLES,
    MFT_REGISTER_TYPE_INFO, MFTEnum2, MFVideoArea, MFVideoFormat_AV1, MFVideoFormat_AYUV,
    MFVideoFormat_H264, MFVideoFormat_HEVC, MFVideoFormat_NV12, MFVideoFormat_P010,
    MFVideoFormat_Y410, MFVideoInterlace_MixedInterlaceOrProgressive,
    eAVEncH265VProfile_Main_420_8, eAVEncH265VProfile_Main_420_10, eAVEncH265VProfile_Main_444_8,
    eAVEncH265VProfile_Main_444_10,
};
use ::windows::Win32::System::Com::CoTaskMemFree;
use ::windows::core::Interface;

use crate::aperture::VideoAperture;
use crate::queue::BoundedQueue;
use crate::{
    ADAPTIVE_VIDEO_QUEUE_CAPACITY, BackendEvent, EncodedVideoFrame, Subsystem, VideoChromaFormat,
    VideoChromaSiting, VideoCodec, VideoColorMatrix, VideoColorPrimaries, VideoFormat,
    VideoPixelFormat, VideoTransferFunction, WindowsDecoderMode,
};

pub(super) trait DecoderDevice {
    fn device_manager(&self) -> &::windows::Win32::Media::MediaFoundation::IMFDXGIDeviceManager;
    fn adapter_luid(&self) -> Result<LUID, String>;
    fn video_format(&self) -> VideoFormat;
}

pub(super) struct DecodedVideoFrame {
    pub(super) format: VideoFormat,
    pub(super) aperture: VideoAperture,
    pub(super) texture: ID3D11Texture2D,
    pub(super) subresource: u32,
    pub(super) timestamp_100ns: i64,
    pub(super) duration_100ns: i64,
    // The sample, not the texture's COM reference, leases the decoder's array
    // slice. Releasing it early returns that slice to the MFT's surface pool.
    // Keep it alive through the decoded queue and conversion to Qt's RGB target.
    // See Microsoft's "Supporting Direct3D 11 Video Decoding in Media Foundation",
    // Decoding: the tracked-sample release callback makes the surface reusable.
    _sample: IMFSample,
}

impl DecodedVideoFrame {
    pub(super) fn from_sample(
        sample: IMFSample,
        format: VideoFormat,
        aperture: VideoAperture,
        preferred_pixel_format: VideoPixelFormat,
    ) -> Result<Self, String> {
        match unsafe { sample.GetUINT32(&MFSampleExtension_FrameCorruption) } {
            Ok(corrupted) if corrupted != 0 => {
                return Err("Media Foundation decoder reported a corrupted output frame".to_owned());
            }
            Ok(_) => {}
            Err(error) if error.code() == MF_E_ATTRIBUTENOTFOUND => {}
            Err(error) => return Err(format!("read decoder output corruption flag: {error}")),
        }
        let (texture, subresource) = dxgi_surface(&sample)?;
        let mut description = D3D11_TEXTURE2D_DESC::default();
        unsafe {
            texture.GetDesc(&mut description);
        }
        validate_decoded_output_format(
            description.Format,
            format.pixel_format,
            preferred_pixel_format,
        )?;
        let timestamp_100ns = unsafe { sample.GetSampleTime().unwrap_or(0) };
        let duration_100ns = unsafe {
            sample
                .GetSampleDuration()
                .ok()
                .filter(|duration| *duration > 0)
                .unwrap_or(format.frame_duration_100ns())
        };
        Ok(Self {
            format,
            aperture,
            texture,
            subresource,
            timestamp_100ns,
            duration_100ns,
            _sample: sample,
        })
    }
}

pub(super) struct Decoder {
    events: Option<IMFMediaEventGenerator>,
    transform: IMFTransform,
    activation: IMFActivate,
    input_stream: u32,
    output_stream: u32,
    input_credits: u32,
    format: VideoFormat,
    aperture: VideoAperture,
    negotiated_format: VideoFormat,
    output_validated: bool,
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
        decoder.validate_output()?;
        decoder.stop();
        Ok(())
    }

    pub(super) fn new<G: DecoderDevice>(
        graphics: &G,
        format: VideoFormat,
        mode: WindowsDecoderMode,
    ) -> Result<Self, String> {
        if mode == WindowsDecoderMode::Hardware {
            ensure_hardware_format(graphics, format)?;
        }
        let activations = enumerate_decoders(graphics.adapter_luid()?, format.codec, mode)?;
        let mut failures = Vec::new();
        for activation in activations {
            let friendly_name = activation_string(&activation, &MFT_FRIENDLY_NAME_Attribute)
                .unwrap_or_else(|| "unknown".to_owned());
            match configure_transform(&activation, graphics, format) {
                Ok((
                    transform,
                    events,
                    input_stream,
                    output_stream,
                    output_provides_samples,
                    output_format,
                    aperture,
                )) => {
                    let pixel_format = output_format.pixel_format;
                    let full_range = output_format.full_range;
                    let input_credits = u32::from(events.is_none());
                    let hardware_registered =
                        activation_string(&activation, &MFT_ENUM_HARDWARE_URL_Attribute).is_some();
                    video_log!(
                        "Windows decoder configured codec={} mft={friendly_name:?} hardwareRegistered={hardware_registered} async={} output={pixel_format:?} bitDepth={} chroma={:?} range={} sampleLease=retained-through-video-blit",
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
                        format: output_format,
                        aperture,
                        negotiated_format: format,
                        output_validated: false,
                        output_provides_samples,
                        stopped: false,
                    });
                }
                Err(error) => {
                    failures.push(format!("{friendly_name}: {error}"));
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

    pub(super) fn validate_output(&mut self) -> Result<VideoFormat, String> {
        let (aperture, format) = output_format(
            &self.transform,
            self.output_stream,
            self.negotiated_format,
            false,
        )?;
        validate_decoded_output_format(
            decoder_surface_format(format.pixel_format),
            format.pixel_format,
            self.negotiated_format.pixel_format,
        )?;
        self.aperture = aperture;
        self.format = format;
        self.output_validated = true;
        Ok(format)
    }

    pub(super) fn probe_frame(&mut self, data: &[u8]) -> Result<DecodedVideoFrame, String> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(500);
        let mut frames = VecDeque::new();
        let events = BoundedQueue::new(16);
        let mut submitted = false;
        while std::time::Instant::now() < deadline {
            self.poll_output(&mut frames, &events)?;
            if let Some(frame) = frames.pop_front() {
                return Ok(frame);
            }
            if !submitted && self.wants_input() {
                self.submit(EncodedVideoFrame {
                    codec: self.negotiated_format.codec,
                    data: data.to_vec(),
                    timestamp_100ns: 0,
                    duration_100ns: self.negotiated_format.frame_duration_100ns(),
                    key_frame: true,
                    reset_decoder: false,
                })?;
                unsafe {
                    self.transform
                        .ProcessMessage(MFT_MESSAGE_COMMAND_DRAIN, 0)
                        .map_err(|error| format!("drain decoder probe: {error}"))?;
                }
                submitted = true;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        Err(format!(
            "decoder probe timed out waiting for {:?} output (submitted={submitted})",
            self.negotiated_format.pixel_format
        ))
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
                self.select_video_output_type()?;
                let updated = self.validate_output()?;
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
        if !self.output_validated {
            self.validate_output()?;
        }
        let frame = DecodedVideoFrame::from_sample(
            sample,
            self.format,
            self.aperture,
            self.negotiated_format.pixel_format,
        )?;
        if decoded_frames.len() == ADAPTIVE_VIDEO_QUEUE_CAPACITY {
            decoded_frames.pop_front();
            let _ = event_queue.push(BackendEvent::QueueOverflow(Subsystem::VideoPresentation));
        }
        decoded_frames.push_back(frame);
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
            self.negotiated_format.pixel_format,
        )
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
    VideoFormat,
    VideoAperture,
);

fn configure_transform<G: DecoderDevice>(
    activation: &IMFActivate,
    graphics: &G,
    format: VideoFormat,
) -> Result<ConfiguredTransform, String> {
    unsafe {
        let transform: IMFTransform = activation
            .ActivateObject()
            .map_err(|error| format!("ActivateObject: {error}"))?;
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
            .map_err(|error| format!("MFT_MESSAGE_SET_D3D_MANAGER: {error}"))?;

        let (input_stream, output_stream) = stream_ids(&transform)?;
        let input_type = video_input_type(format)?;
        transform
            .SetInputType(input_stream, &input_type, 0)
            .map_err(|error| format!("SetInputType: {error}"))?;
        select_video_output_type(&transform, output_stream, format.pixel_format)?;
        let (aperture, decoded_format) = output_format(&transform, output_stream, format, true)?;

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
            decoded_format,
            aperture,
        ))
    }
}

fn video_input_type(format: VideoFormat) -> Result<IMFMediaType, String> {
    format.validate_color().map_err(|error| error.to_string())?;
    unsafe {
        let input_type = MFCreateMediaType().map_err(|error| error.to_string())?;
        input_type
            .SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)
            .map_err(|error| error.to_string())?;
        input_type
            .SetGUID(&MF_MT_SUBTYPE, video_subtype(format.codec))
            .map_err(|error| error.to_string())?;
        if format.codec == VideoCodec::H265 {
            let profile = match format.pixel_format {
                VideoPixelFormat::Nv12 => eAVEncH265VProfile_Main_420_8,
                VideoPixelFormat::P010 => eAVEncH265VProfile_Main_420_10,
                VideoPixelFormat::Ayuv => eAVEncH265VProfile_Main_444_8,
                VideoPixelFormat::Y410 => eAVEncH265VProfile_Main_444_10,
            };
            input_type
                .SetUINT32(&MF_MT_MPEG2_PROFILE, profile.0 as u32)
                .map_err(|error| format!("HEVC input profile: {error}"))?;
        }
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
        for (key, value) in [
            (
                MF_MT_VIDEO_CHROMA_SITING,
                match format.chroma_siting {
                    VideoChromaSiting::Left => MFVideoChromaSubsampling_MPEG2.0,
                    VideoChromaSiting::TopLeft => MFVideoChromaSubsampling_Cosited.0,
                },
            ),
            (
                MF_MT_TRANSFER_FUNCTION,
                match format.transfer_function {
                    VideoTransferFunction::Sdr => MFVideoTransFunc_709.0,
                    VideoTransferFunction::Pq => MFVideoTransFunc_2084.0,
                    VideoTransferFunction::Hlg => MFVideoTransFunc_HLG.0,
                },
            ),
            (
                MF_MT_VIDEO_PRIMARIES,
                match format.color_primaries {
                    VideoColorPrimaries::Bt709 => MFVideoPrimaries_BT709.0,
                    VideoColorPrimaries::Bt2020 => MFVideoPrimaries_BT2020.0,
                },
            ),
            (
                MF_MT_YUV_MATRIX,
                match format.color_matrix {
                    VideoColorMatrix::Bt601 => MFVideoTransferMatrix_BT601.0,
                    VideoColorMatrix::Bt709 => MFVideoTransferMatrix_BT709.0,
                    VideoColorMatrix::Bt2020 => MFVideoTransferMatrix_BT2020_10.0,
                },
            ),
            (
                MF_MT_VIDEO_NOMINAL_RANGE,
                if format.full_range {
                    MFNominalRange_0_255.0
                } else {
                    MFNominalRange_16_235.0
                },
            ),
        ] {
            input_type
                .SetUINT32(&key, value as u32)
                .map_err(|error| format!("decoder input color metadata: {error}"))?;
        }
        Ok(input_type)
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

// MFT registration and D3D11 awareness do not prove hardware codec support. In particular,
// an installed AV1 MFT can exist on a pre-AV1 GPU. Check the actual decoder device, including
// the requested depth, chroma and dimensions, before configuring either probe or live decode.
fn hardware_profiles(
    codec: VideoCodec,
    pixel: VideoPixelFormat,
) -> &'static [::windows::core::GUID] {
    match (codec, pixel) {
        (VideoCodec::H264, VideoPixelFormat::Nv12) => &[
            D3D11_DECODER_PROFILE_H264_VLD_NOFGT,
            D3D11_DECODER_PROFILE_H264_VLD_FGT,
        ],
        (VideoCodec::H265, VideoPixelFormat::Nv12) => &[D3D11_DECODER_PROFILE_HEVC_VLD_MAIN],
        (VideoCodec::H265, VideoPixelFormat::P010) => &[D3D11_DECODER_PROFILE_HEVC_VLD_MAIN10],
        // DXVA decode profile GUIDs are shared between the D3D11 and D3D12 APIs.
        (VideoCodec::H265, VideoPixelFormat::Ayuv) => &[D3D12_VIDEO_DECODE_PROFILE_HEVC_MAIN_444],
        (VideoCodec::H265, VideoPixelFormat::Y410) => &[D3D12_VIDEO_DECODE_PROFILE_HEVC_MAIN10_444],
        (VideoCodec::Av1, VideoPixelFormat::Nv12 | VideoPixelFormat::P010) => {
            &[D3D11_DECODER_PROFILE_AV1_VLD_PROFILE0]
        }
        _ => &[],
    }
}

fn ensure_hardware_format<G: DecoderDevice>(
    graphics: &G,
    format: VideoFormat,
) -> Result<(), String> {
    let manager = graphics.device_manager();
    let device = unsafe {
        let handle = manager
            .OpenDeviceHandle()
            .map_err(|error| error.to_string())?;
        let mut service = ptr::null_mut();
        let result = manager.GetVideoService(handle, &ID3D11VideoDevice::IID, &mut service);
        let _ = manager.CloseDeviceHandle(handle);
        result.map_err(|error| format!("D3D11 decoder device: {error}"))?;
        if service.is_null() {
            return Err("D3D11 decoder device is null".to_owned());
        }
        ID3D11VideoDevice::from_raw(service)
    };
    let output = decoder_surface_format(format.pixel_format);
    let candidates = hardware_profiles(format.codec, format.pixel_format);
    unsafe {
        for index in 0..device.GetVideoDecoderProfileCount() {
            let profile = device
                .GetVideoDecoderProfile(index)
                .map_err(|error| error.to_string())?;
            if !candidates.contains(&profile)
                || !device
                    .CheckVideoDecoderFormat(&profile, output)
                    .is_ok_and(|supported| supported.as_bool())
            {
                continue;
            }
            let description = D3D11_VIDEO_DECODER_DESC {
                Guid: profile,
                SampleWidth: format.width,
                SampleHeight: format.height,
                OutputFormat: output,
            };
            if device
                .GetVideoDecoderConfigCount(&description)
                .is_ok_and(|count| count > 0)
            {
                return Ok(());
            }
        }
    }
    Err(format!(
        "The selected GPU does not support hardware {} {:?} decoding at {}x{}. Use Auto codec or a supported color depth/resolution.",
        format.codec.label(),
        format.pixel_format,
        format.width,
        format.height
    ))
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
                        video_log!(
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

fn decoder_surface_format(pixel_format: VideoPixelFormat) -> DXGI_FORMAT {
    match pixel_format {
        VideoPixelFormat::Nv12 => DXGI_FORMAT_NV12,
        VideoPixelFormat::P010 => DXGI_FORMAT_P010,
        VideoPixelFormat::Ayuv => DXGI_FORMAT_AYUV,
        VideoPixelFormat::Y410 => DXGI_FORMAT_Y410,
    }
}

fn validate_decoded_output_format(
    surface_format: DXGI_FORMAT,
    output: VideoPixelFormat,
    preferred: VideoPixelFormat,
) -> Result<(), String> {
    if surface_format != decoder_surface_format(output) {
        return Err(format!(
            "Media Foundation decoder surface format {} does not match output media format {output:?}",
            surface_format.0
        ));
    }
    if output.bit_depth() < preferred.bit_depth()
        || (chroma_format(preferred) == VideoChromaFormat::Cs444
            && chroma_format(output) != VideoChromaFormat::Cs444)
    {
        return Err(format!(
            "Media Foundation decoder produced {output:?} output below negotiated {preferred:?} precision or chroma"
        ));
    }
    Ok(())
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

fn output_format(
    transform: &IMFTransform,
    output_stream: u32,
    fallback: VideoFormat,
    allow_provisional: bool,
) -> Result<(VideoAperture, VideoFormat), String> {
    unsafe {
        let media_type = transform
            .GetOutputCurrentType(output_stream)
            .map_err(|error| error.to_string())?;
        let aperture = output_aperture(&media_type, fallback)?;
        let subtype = media_type
            .GetGUID(&MF_MT_SUBTYPE)
            .map_err(|error| error.to_string())?;
        let pixel_format = pixel_format_from_subtype(subtype)
            .ok_or("decoder output has an unsupported pixel format")?;
        let format = output_media_format(
            &media_type,
            fallback,
            aperture,
            pixel_format,
            allow_provisional,
        )?;
        Ok((aperture, format))
    }
}

fn output_media_format(
    media_type: &IMFMediaType,
    negotiated: VideoFormat,
    aperture: VideoAperture,
    pixel_format: VideoPixelFormat,
    allow_provisional: bool,
) -> Result<VideoFormat, String> {
    let format = VideoFormat {
        width: aperture.width,
        height: aperture.height,
        pixel_format,
        chroma_format: chroma_format(pixel_format),
        ..negotiated
    };
    if allow_provisional && pixel_format != negotiated.pixel_format {
        return Ok(format);
    }
    output_color_format(media_type, format)
}

fn output_color_format(
    media_type: &IMFMediaType,
    fallback: VideoFormat,
) -> Result<VideoFormat, String> {
    let mut format = fallback;
    for key in [
        MF_MT_TRANSFER_FUNCTION,
        MF_MT_VIDEO_PRIMARIES,
        MF_MT_YUV_MATRIX,
        MF_MT_VIDEO_NOMINAL_RANGE,
        MF_MT_VIDEO_CHROMA_SITING,
    ] {
        let value = match unsafe { media_type.GetUINT32(&key) } {
            Ok(0) => continue,
            Ok(value) => value as i32,
            Err(error) if error.code() == MF_E_ATTRIBUTENOTFOUND => continue,
            Err(error) => return Err(format!("decoder output color metadata: {error}")),
        };
        if key == MF_MT_TRANSFER_FUNCTION {
            format.transfer_function = match value {
                value if value == MFVideoTransFunc_2084.0 => VideoTransferFunction::Pq,
                value if value == MFVideoTransFunc_HLG.0 => VideoTransferFunction::Hlg,
                value
                    if [
                        MFVideoTransFunc_22.0,
                        MFVideoTransFunc_709.0,
                        MFVideoTransFunc_sRGB.0,
                        MFVideoTransFunc_2020.0,
                    ]
                    .contains(&value) =>
                {
                    VideoTransferFunction::Sdr
                }
                _ => return Err(format!("unsupported decoder transfer function {value}")),
            };
        } else if key == MF_MT_VIDEO_PRIMARIES {
            format.color_primaries = match value {
                value if value == MFVideoPrimaries_BT709.0 => VideoColorPrimaries::Bt709,
                value if value == MFVideoPrimaries_BT2020.0 => VideoColorPrimaries::Bt2020,
                _ => return Err(format!("unsupported decoder color primaries {value}")),
            };
        } else if key == MF_MT_YUV_MATRIX {
            format.color_matrix = match value {
                value if value == MFVideoTransferMatrix_BT601.0 => VideoColorMatrix::Bt601,
                value if value == MFVideoTransferMatrix_BT709.0 => VideoColorMatrix::Bt709,
                value if value == MFVideoTransferMatrix_BT2020_10.0 => VideoColorMatrix::Bt2020,
                _ => return Err(format!("unsupported decoder YCbCr matrix {value}")),
            };
        } else if key == MF_MT_VIDEO_CHROMA_SITING {
            format.chroma_siting = match value & !MFVideoChromaSubsampling_ProgressiveChroma.0 {
                value if value == MFVideoChromaSubsampling_MPEG2.0 => VideoChromaSiting::Left,
                value if value == MFVideoChromaSubsampling_Cosited.0 => VideoChromaSiting::TopLeft,
                _ => return Err(format!("unsupported decoder chroma siting {value}")),
            };
        } else {
            format.full_range = match value {
                value if value == MFNominalRange_0_255.0 => true,
                value if value == MFNominalRange_16_235.0 => false,
                _ => return Err(format!("unsupported decoder nominal range {value}")),
            };
        }
    }
    if fallback.transfer_function != VideoTransferFunction::Sdr
        && format.transfer_function == VideoTransferFunction::Sdr
    {
        return Err("decoder reported SDR output for a negotiated HDR stream".to_owned());
    }
    format.validate_color().map_err(|error| error.to_string())?;
    Ok(format)
}

fn output_aperture(
    media_type: &IMFMediaType,
    fallback: VideoFormat,
) -> Result<VideoAperture, String> {
    let packed = match unsafe { media_type.GetUINT64(&MF_MT_FRAME_SIZE) } {
        Ok(packed) => packed,
        Err(error) if error.code() == MF_E_ATTRIBUTENOTFOUND => {
            pack_pair(fallback.width, fallback.height)
        }
        Err(error) => return Err(error.to_string()),
    };
    let pan_scan = unsafe { media_type.GetUINT32(&MF_MT_PAN_SCAN_ENABLED).unwrap_or(0) != 0 };
    for key in [
        pan_scan.then_some(&MF_MT_PAN_SCAN_APERTURE),
        Some(&MF_MT_MINIMUM_DISPLAY_APERTURE),
        Some(&MF_MT_GEOMETRIC_APERTURE),
    ]
    .into_iter()
    .flatten()
    {
        let size = match unsafe { media_type.GetBlobSize(key) } {
            Ok(size) => size as usize,
            Err(error) if error.code() == MF_E_ATTRIBUTENOTFOUND => continue,
            Err(error) => return Err(error.to_string()),
        };
        if size != size_of::<MFVideoArea>() {
            return Err(format!("invalid decoder aperture blob size {size}"));
        }
        let mut bytes = [0_u8; size_of::<MFVideoArea>()];
        unsafe { media_type.GetBlob(key, &mut bytes, None) }.map_err(|error| error.to_string())?;
        let area = unsafe { ptr::read_unaligned(bytes.as_ptr().cast::<MFVideoArea>()) };
        if area.OffsetX.fract != 0 || area.OffsetY.fract != 0 {
            return Err("fractional decoder aperture offsets cannot be represented by a D3D11 source rectangle".to_owned());
        }
        let area = (
            i32::from(area.OffsetX.value),
            i32::from(area.OffsetY.value),
            area.Area.cx,
            area.Area.cy,
        );
        if area == (0, 0, 0, 0) {
            continue;
        }
        return VideoAperture::new((packed >> 32) as u32, packed as u32, Some(area));
    }
    VideoAperture::new((packed >> 32) as u32, packed as u32, None)
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

    fn hdr_test_format() -> VideoFormat {
        VideoFormat {
            codec: VideoCodec::H265,
            width: 1920,
            height: 1080,
            frame_rate_numerator: std::num::NonZeroU32::new(60).unwrap(),
            frame_rate_denominator: std::num::NonZeroU32::new(1).unwrap(),
            average_bitrate: 20_000_000,
            pixel_format: VideoPixelFormat::P010,
            chroma_format: VideoChromaFormat::Cs420,
            full_range: false,
            chroma_siting: crate::VideoChromaSiting::Left,
            transfer_function: VideoTransferFunction::Pq,
            color_primaries: VideoColorPrimaries::Bt2020,
            color_matrix: VideoColorMatrix::Bt2020,
        }
    }

    #[test]
    fn provisional_nv12_allows_hdr_startup_but_never_validated_output() {
        let _runtime = super::super::MediaRuntime::initialize().unwrap();
        let negotiated = hdr_test_format();
        let aperture = VideoAperture::new(negotiated.width, negotiated.height, None).unwrap();
        let media_type = video_input_type(negotiated).unwrap();
        let provisional = output_media_format(
            &media_type,
            negotiated,
            aperture,
            VideoPixelFormat::Nv12,
            true,
        )
        .unwrap();
        assert_eq!(provisional.pixel_format, VideoPixelFormat::Nv12);
        assert!(
            output_media_format(
                &media_type,
                negotiated,
                aperture,
                VideoPixelFormat::Nv12,
                false,
            )
            .is_err()
        );
        assert!(
            validate_decoded_output_format(
                DXGI_FORMAT_NV12,
                provisional.pixel_format,
                negotiated.pixel_format,
            )
            .is_err()
        );
        assert_eq!(
            output_media_format(
                &media_type,
                negotiated,
                aperture,
                VideoPixelFormat::P010,
                false,
            )
            .unwrap(),
            negotiated
        );

        unsafe {
            media_type
                .SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32)
                .unwrap();
        }
        assert!(
            output_media_format(
                &media_type,
                negotiated,
                aperture,
                VideoPixelFormat::Nv12,
                true,
            )
            .is_ok()
        );
        assert!(
            output_media_format(
                &media_type,
                negotiated,
                aperture,
                VideoPixelFormat::P010,
                false,
            )
            .is_err()
        );
    }

    #[test]
    fn decoder_color_defaults_and_actual_metadata_are_explicit() {
        let _runtime = super::super::MediaRuntime::initialize().unwrap();
        let format = hdr_test_format();
        let media_type = unsafe { MFCreateMediaType().unwrap() };
        assert_eq!(output_color_format(&media_type, format).unwrap(), format);
        unsafe {
            media_type.SetUINT32(&MF_MT_TRANSFER_FUNCTION, 0).unwrap();
            media_type.SetUINT32(&MF_MT_VIDEO_PRIMARIES, 0).unwrap();
            media_type.SetUINT32(&MF_MT_YUV_MATRIX, 0).unwrap();
            media_type.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, 0).unwrap();
        }
        assert_eq!(output_color_format(&media_type, format).unwrap(), format);
        unsafe {
            media_type
                .SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_HLG.0 as u32)
                .unwrap();
            media_type
                .SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_0_255.0 as u32)
                .unwrap();
        }
        let updated = output_color_format(&media_type, format).unwrap();
        assert_eq!(updated.transfer_function, VideoTransferFunction::Hlg);
        assert!(updated.full_range);
        let sdr_defaults = VideoFormat {
            transfer_function: VideoTransferFunction::Sdr,
            color_primaries: VideoColorPrimaries::Bt709,
            color_matrix: VideoColorMatrix::Bt709,
            ..format
        };
        let actual_hdr_type = video_input_type(format).unwrap();
        assert_eq!(
            output_color_format(&actual_hdr_type, sdr_defaults).unwrap(),
            format
        );
        for transfer_function in [VideoTransferFunction::Pq, VideoTransferFunction::Hlg] {
            let negotiated = VideoFormat {
                transfer_function,
                ..format
            };
            assert_eq!(
                output_color_format(&video_input_type(negotiated).unwrap(), negotiated).unwrap(),
                negotiated
            );
        }
    }

    #[test]
    fn decoder_preserves_actual_chroma_siting_or_rejects_it() {
        let _runtime = super::super::MediaRuntime::initialize().unwrap();
        let format = hdr_test_format();
        let media_type = unsafe { MFCreateMediaType().unwrap() };
        for (value, expected) in [
            (0, VideoChromaSiting::Left),
            (MFVideoChromaSubsampling_MPEG2.0, VideoChromaSiting::Left),
            (
                MFVideoChromaSubsampling_Cosited.0,
                VideoChromaSiting::TopLeft,
            ),
            (
                MFVideoChromaSubsampling_MPEG2.0 | MFVideoChromaSubsampling_ProgressiveChroma.0,
                VideoChromaSiting::Left,
            ),
            (
                MFVideoChromaSubsampling_Cosited.0 | MFVideoChromaSubsampling_ProgressiveChroma.0,
                VideoChromaSiting::TopLeft,
            ),
        ] {
            unsafe {
                media_type
                    .SetUINT32(&MF_MT_VIDEO_CHROMA_SITING, value as u32)
                    .unwrap();
            }
            assert_eq!(
                output_color_format(&media_type, format)
                    .unwrap()
                    .chroma_siting,
                expected
            );
            let negotiated = VideoFormat {
                chroma_siting: expected,
                ..format
            };
            assert_eq!(
                output_color_format(&video_input_type(negotiated).unwrap(), format)
                    .unwrap()
                    .chroma_siting,
                expected
            );
        }
        for value in [1, 2, 4, 6, 8, 999] {
            unsafe {
                media_type
                    .SetUINT32(&MF_MT_VIDEO_CHROMA_SITING, value)
                    .unwrap();
            }
            assert!(output_color_format(&media_type, format).is_err());
        }
    }

    #[test]
    fn decoder_rejects_hdr_downgrades_and_unsupported_metadata() {
        let _runtime = super::super::MediaRuntime::initialize().unwrap();
        let format = hdr_test_format();
        for (key, value) in [
            (MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0),
            (MF_MT_TRANSFER_FUNCTION, 999),
            (MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0),
            (MF_MT_VIDEO_PRIMARIES, 999),
            (MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0),
            (MF_MT_YUV_MATRIX, 999),
            (MF_MT_VIDEO_NOMINAL_RANGE, 3),
        ] {
            let media_type = unsafe { MFCreateMediaType().unwrap() };
            unsafe {
                media_type.SetUINT32(&key, value as u32).unwrap();
            }
            assert!(
                output_color_format(&media_type, format).is_err(),
                "metadata {key:?}={value}"
            );
        }
        let media_type = unsafe { MFCreateMediaType().unwrap() };
        assert!(
            output_color_format(
                &media_type,
                VideoFormat {
                    pixel_format: VideoPixelFormat::Nv12,
                    ..format
                }
            )
            .is_err()
        );
    }

    #[test]
    fn hevc_input_types_set_the_negotiated_profile_before_decoder_configuration() {
        let _runtime = super::super::MediaRuntime::initialize().unwrap();
        for (pixel_format, profile) in [
            (VideoPixelFormat::Nv12, eAVEncH265VProfile_Main_420_8),
            (VideoPixelFormat::P010, eAVEncH265VProfile_Main_420_10),
            (VideoPixelFormat::Ayuv, eAVEncH265VProfile_Main_444_8),
            (VideoPixelFormat::Y410, eAVEncH265VProfile_Main_444_10),
        ] {
            let format = VideoFormat {
                codec: VideoCodec::H265,
                width: 2560,
                height: 1440,
                frame_rate_numerator: std::num::NonZeroU32::new(120).unwrap(),
                frame_rate_denominator: std::num::NonZeroU32::new(1).unwrap(),
                average_bitrate: 75_000_000,
                pixel_format,
                chroma_format: chroma_format(pixel_format),
                full_range: false,
                chroma_siting: crate::VideoChromaSiting::Left,
                transfer_function: crate::VideoTransferFunction::Sdr,
                color_primaries: crate::VideoColorPrimaries::Bt709,
                color_matrix: crate::VideoColorMatrix::Bt709,
            };
            let input_type = video_input_type(format).unwrap();
            unsafe {
                assert_eq!(
                    input_type.GetGUID(&MF_MT_MAJOR_TYPE).unwrap(),
                    MFMediaType_Video
                );
                assert_eq!(
                    input_type.GetGUID(&MF_MT_SUBTYPE).unwrap(),
                    MFVideoFormat_HEVC
                );
                assert_eq!(
                    input_type.GetUINT32(&MF_MT_MPEG2_PROFILE).unwrap(),
                    profile.0 as u32
                );
                assert_eq!(
                    input_type.GetUINT64(&MF_MT_FRAME_SIZE).unwrap(),
                    pack_pair(2560, 1440)
                );
                assert_eq!(
                    input_type.GetUINT64(&MF_MT_FRAME_RATE).unwrap(),
                    pack_pair(120, 1)
                );
                assert_eq!(
                    input_type.GetUINT32(&MF_MT_AVG_BITRATE).unwrap(),
                    75_000_000
                );
                assert_eq!(
                    input_type.GetUINT64(&MF_MT_PIXEL_ASPECT_RATIO).unwrap(),
                    pack_pair(1, 1)
                );
                assert_eq!(
                    input_type.GetUINT32(&MF_MT_INTERLACE_MODE).unwrap(),
                    MFVideoInterlace_MixedInterlaceOrProgressive.0 as u32
                );
            }
            for codec in [VideoCodec::H264, VideoCodec::Av1] {
                let input_type = video_input_type(VideoFormat { codec, ..format }).unwrap();
                unsafe {
                    assert_eq!(
                        input_type.GetGUID(&MF_MT_SUBTYPE).unwrap(),
                        *video_subtype(codec)
                    );
                    assert_eq!(
                        input_type
                            .GetUINT32(&MF_MT_MPEG2_PROFILE)
                            .unwrap_err()
                            .code(),
                        MF_E_ATTRIBUTENOTFOUND
                    );
                }
            }
        }
    }

    #[test]
    fn output_samples_reject_reported_corruption_before_surface_extraction() {
        let _runtime = super::super::MediaRuntime::initialize().unwrap();
        let format = VideoFormat {
            codec: VideoCodec::H264,
            width: 2560,
            height: 1440,
            frame_rate_numerator: std::num::NonZeroU32::new(60).unwrap(),
            frame_rate_denominator: std::num::NonZeroU32::new(1).unwrap(),
            average_bitrate: 78_000_000,
            pixel_format: VideoPixelFormat::Nv12,
            chroma_format: VideoChromaFormat::Cs420,
            full_range: false,
            chroma_siting: crate::VideoChromaSiting::Left,
            transfer_function: crate::VideoTransferFunction::Sdr,
            color_primaries: crate::VideoColorPrimaries::Bt709,
            color_matrix: crate::VideoColorMatrix::Bt709,
        };
        let aperture = VideoAperture::new(format.width, format.height, None).unwrap();
        for (corruption, discontinuity) in [
            (None, false),
            (Some(0), false),
            (None, true),
            (Some(0), true),
            (Some(1), false),
            (Some(1), true),
        ] {
            let sample = unsafe {
                let sample = MFCreateSample().unwrap();
                sample.AddBuffer(&MFCreateMemoryBuffer(1).unwrap()).unwrap();
                if let Some(corruption) = corruption {
                    sample
                        .SetUINT32(&MFSampleExtension_FrameCorruption, corruption)
                        .unwrap();
                }
                if discontinuity {
                    sample
                        .SetUINT32(
                            &::windows::Win32::Media::MediaFoundation::MFSampleExtension_Discontinuity,
                            1,
                        )
                        .unwrap();
                }
                sample
            };
            let error =
                DecodedVideoFrame::from_sample(sample, format, aperture, format.pixel_format)
                    .err()
                    .expect("system-memory output cannot be published as a decoded GPU frame");
            assert_eq!(
                error,
                if corruption == Some(1) {
                    "Media Foundation decoder reported a corrupted output frame"
                } else {
                    "hardware decoder returned a system-memory buffer instead of a D3D11 surface"
                },
                "corruption={corruption:?} discontinuity={discontinuity}"
            );
        }
    }

    #[test]
    fn media_type_apertures_handle_padding_defaults_precedence_and_invalid_offsets() {
        let _runtime = super::super::MediaRuntime::initialize().unwrap();
        let format = VideoFormat {
            codec: VideoCodec::H264,
            width: 1920,
            height: 1080,
            frame_rate_numerator: std::num::NonZeroU32::new(60).unwrap(),
            frame_rate_denominator: std::num::NonZeroU32::new(1).unwrap(),
            average_bitrate: 10_000_000,
            pixel_format: VideoPixelFormat::Nv12,
            chroma_format: VideoChromaFormat::Cs420,
            full_range: true,
            chroma_siting: crate::VideoChromaSiting::Left,
            transfer_function: crate::VideoTransferFunction::Sdr,
            color_primaries: crate::VideoColorPrimaries::Bt709,
            color_matrix: crate::VideoColorMatrix::Bt709,
        };
        let media_type = unsafe { MFCreateMediaType().unwrap() };
        assert_eq!(
            output_aperture(&media_type, format).unwrap(),
            VideoAperture::new(1920, 1080, None).unwrap()
        );
        unsafe {
            media_type
                .SetUINT64(&MF_MT_FRAME_SIZE, pack_pair(1920, 1088))
                .unwrap();
        }
        assert_eq!(output_aperture(&media_type, format).unwrap().height, 1088);
        let set_area = |key, area: MFVideoArea| {
            let bytes = unsafe {
                std::slice::from_raw_parts(
                    (&area as *const MFVideoArea).cast::<u8>(),
                    size_of::<MFVideoArea>(),
                )
            };
            unsafe {
                media_type.SetBlob(key, bytes).unwrap();
            }
        };
        let mut area = MFVideoArea::default();
        set_area(&MF_MT_MINIMUM_DISPLAY_APERTURE, area);
        assert_eq!(output_aperture(&media_type, format).unwrap().height, 1088);
        area.Area.cx = 1904;
        area.Area.cy = 1080;
        area.OffsetX.value = 8;
        area.OffsetY.value = 4;
        set_area(&MF_MT_GEOMETRIC_APERTURE, area);
        let visible = output_aperture(&media_type, format).unwrap();
        assert_eq!(
            visible,
            VideoAperture::new(1920, 1088, Some((8, 4, 1904, 1080))).unwrap()
        );
        area.Area.cy = 1072;
        set_area(&MF_MT_MINIMUM_DISPLAY_APERTURE, area);
        assert_eq!(output_aperture(&media_type, format).unwrap().height, 1072);
        area.Area.cy = 1064;
        set_area(&MF_MT_PAN_SCAN_APERTURE, area);
        assert_eq!(output_aperture(&media_type, format).unwrap().height, 1072);
        unsafe {
            media_type.SetUINT32(&MF_MT_PAN_SCAN_ENABLED, 1).unwrap();
        }
        assert_eq!(output_aperture(&media_type, format).unwrap().height, 1064);
        area.OffsetX.fract = 1;
        set_area(&MF_MT_PAN_SCAN_APERTURE, area);
        assert!(
            output_aperture(&media_type, format)
                .unwrap_err()
                .contains("fractional")
        );
        area.OffsetX.fract = 0;
        area.OffsetX.value = -1;
        set_area(&MF_MT_PAN_SCAN_APERTURE, area);
        assert!(output_aperture(&media_type, format).is_err());
        unsafe {
            media_type
                .SetBlob(&MF_MT_PAN_SCAN_APERTURE, &[0_u8; 1])
                .unwrap();
        }
        assert!(
            output_aperture(&media_type, format)
                .unwrap_err()
                .contains("blob size")
        );
    }

    #[test]
    fn hardware_profiles_do_not_confuse_codec_depth_or_chroma() {
        let legacy_gpu = [
            D3D11_DECODER_PROFILE_H264_VLD_NOFGT,
            D3D11_DECODER_PROFILE_HEVC_VLD_MAIN,
        ];
        assert!(
            hardware_profiles(VideoCodec::H264, VideoPixelFormat::Nv12)
                .iter()
                .any(|p| legacy_gpu.contains(p))
        );
        assert!(
            !hardware_profiles(VideoCodec::Av1, VideoPixelFormat::Nv12)
                .iter()
                .any(|p| legacy_gpu.contains(p))
        );
        assert!(
            !hardware_profiles(VideoCodec::H265, VideoPixelFormat::P010)
                .iter()
                .any(|p| legacy_gpu.contains(p))
        );
        assert!(hardware_profiles(VideoCodec::H264, VideoPixelFormat::P010).is_empty());
        assert!(hardware_profiles(VideoCodec::Av1, VideoPixelFormat::Y410).is_empty());
        assert_ne!(
            hardware_profiles(VideoCodec::H265, VideoPixelFormat::Y410),
            hardware_profiles(VideoCodec::H265, VideoPixelFormat::P010)
        );
    }

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

    #[test]
    fn decoded_output_rejects_depth_or_chroma_loss_after_startup() {
        for (preferred, accepted) in [
            (VideoPixelFormat::Nv12, [true, true, true, true]),
            (VideoPixelFormat::P010, [false, true, false, true]),
            (VideoPixelFormat::Ayuv, [false, false, true, true]),
            (VideoPixelFormat::Y410, [false, false, false, true]),
        ] {
            for (output, accepted) in [
                VideoPixelFormat::Nv12,
                VideoPixelFormat::P010,
                VideoPixelFormat::Ayuv,
                VideoPixelFormat::Y410,
            ]
            .into_iter()
            .zip(accepted)
            {
                let result = validate_decoded_output_format(
                    decoder_surface_format(output),
                    output,
                    preferred,
                );
                assert_eq!(
                    result.is_ok(),
                    accepted,
                    "requested={preferred:?} output={output:?}: {result:?}"
                );
                if !accepted {
                    let error = result.unwrap_err();
                    assert!(error.contains(&format!("produced {output:?}")));
                    assert!(error.contains(&format!("negotiated {preferred:?}")));
                }
            }
        }
    }

    #[test]
    fn decoded_surface_must_match_media_type_before_precision_validation() {
        for surface in [
            DXGI_FORMAT_NV12,
            DXGI_FORMAT_P010,
            DXGI_FORMAT_AYUV,
            DXGI_FORMAT_Y410,
            ::windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_R8G8B8A8_UNORM,
        ] {
            for output in [
                VideoPixelFormat::Nv12,
                VideoPixelFormat::P010,
                VideoPixelFormat::Ayuv,
                VideoPixelFormat::Y410,
            ] {
                let result = validate_decoded_output_format(surface, output, output);
                assert_eq!(result.is_ok(), surface == decoder_surface_format(output));
                if let Err(error) = result {
                    assert!(error.contains("does not match output media format"));
                }
            }
        }
    }
}
