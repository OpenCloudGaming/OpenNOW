use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use objc2_core_foundation::{
    CFBoolean, CFData, CFDictionary, CFMutableDictionary, CFNumber, CFNumberType, CFRetained,
    CFString, kCFBooleanFalse, kCFBooleanTrue, kCFTypeDictionaryKeyCallBacks,
    kCFTypeDictionaryValueCallBacks,
};
use objc2_core_media::{
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMSampleTimingInfo, CMTime,
    CMVideoFormatDescriptionCreate, CMVideoFormatDescriptionCreateFromH264ParameterSets,
    CMVideoFormatDescriptionCreateFromHEVCParameterSets, CMVideoFormatDescriptionGetDimensions,
    kCMFormatDescriptionChromaLocation_Center, kCMFormatDescriptionChromaLocation_Left,
    kCMFormatDescriptionColorPrimaries_ITU_R_709_2, kCMFormatDescriptionExtension_BitsPerComponent,
    kCMFormatDescriptionExtension_ChromaLocationBottomField,
    kCMFormatDescriptionExtension_ChromaLocationTopField,
    kCMFormatDescriptionExtension_ColorPrimaries, kCMFormatDescriptionExtension_FullRangeVideo,
    kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms,
    kCMFormatDescriptionExtension_TransferFunction, kCMFormatDescriptionExtension_YCbCrMatrix,
    kCMFormatDescriptionTransferFunction_ITU_R_709_2, kCMFormatDescriptionTransferFunction_sRGB,
    kCMFormatDescriptionYCbCrMatrix_ITU_R_601_4, kCMFormatDescriptionYCbCrMatrix_ITU_R_709_2,
    kCMTimeInvalid, kCMVideoCodecType_AV1,
};
use objc2_core_video::{
    CVImageBuffer, CVPixelBufferGetPixelFormatType, kCVPixelBufferIOSurfacePropertiesKey,
    kCVPixelBufferMetalCompatibilityKey, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
    kCVPixelFormatType_444YpCbCr10BiPlanarFullRange,
    kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange,
};
use objc2_video_toolbox::{
    VTDecodeFrameFlags, VTDecodeInfoFlags, VTDecompressionOutputCallbackRecord,
    VTDecompressionSession, VTSessionSetProperty, kVTDecompressionPropertyKey_RealTime,
    kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder,
};

use crate::failure::{BackendSubsystem, FailureReporter};
use crate::format::{
    Av1Format, FrameTiming, H264Format, H265Format, VideoBitDepth, VideoChroma, VideoColorSpace,
    VideoFormat, VideoTransfer,
};
use crate::queue::{BoundedQueue, PushResult};

use super::mailbox::LatestMailbox;
use super::{BackendError, Counters};

#[derive(Clone)]
pub(super) struct DecodedFrame {
    pub(super) color: Option<opennow_media_protocol::ColorDescription>,
    pub(super) provenance: opennow_media_protocol::FrameProvenance,
    pub(super) image: CFRetained<CVImageBuffer>,
    pub(super) color_space: VideoColorSpace,
    pub(super) transfer: VideoTransfer,
    pub(super) minimum_frame_duration_seconds: f64,
    pub(super) timestamp_100ns: i64,
}

// The callback retains the CVImageBuffer and no code mutates it after publication to the queue.
unsafe impl Send for DecodedFrame {}
unsafe impl Sync for DecodedFrame {}

#[derive(Clone)]
pub(super) enum DecodedFrameOutput {
    PresentationQueue {
        queue: Arc<BoundedQueue<DecodedFrame>>,
        decoded: Option<Arc<dyn Fn(opennow_media_protocol::FrameProvenance) + Send + Sync>>,
    },
    EmbeddedMailbox {
        mailbox: Arc<LatestMailbox<DecodedFrame>>,
        publish: Option<Arc<dyn Fn(DecodedFrame) -> bool + Send + Sync>>,
    },
}

impl DecodedFrameOutput {
    fn publish(&self, frame: DecodedFrame) -> bool {
        match self {
            Self::PresentationQueue { queue, decoded } => {
                if let Some(decoded) = decoded {
                    decoded(frame.provenance);
                }
                matches!(
                    queue.push_drop_oldest(frame),
                    PushResult::Replaced(_) | PushResult::Closed(_)
                )
            }
            Self::EmbeddedMailbox { mailbox, publish } => {
                if let Some(publish) = publish {
                    publish(frame)
                } else {
                    mailbox.replace(frame)
                }
            }
        }
    }

    pub(super) fn clear(&self) -> usize {
        match self {
            Self::PresentationQueue { queue, .. } => queue.clear(),
            Self::EmbeddedMailbox { mailbox, .. } => usize::from(mailbox.clear()),
        }
    }
}

struct InFlight {
    count: AtomicUsize,
    maximum: usize,
}

impl InFlight {
    fn try_acquire(&self) -> bool {
        let mut count = self.count.load(Ordering::Acquire);
        while let Some(next) = (count < self.maximum).then_some(count + 1) {
            match self
                .count
                .compare_exchange_weak(count, next, Ordering::AcqRel, Ordering::Acquire)
            {
                Ok(_) => return true,
                Err(current) => count = current,
            }
        }
        false
    }

    fn release(&self) {
        let previous = self.count.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
    }
}

struct CallbackContext {
    color: Option<opennow_media_protocol::ColorDescription>,
    provenance: Mutex<crate::provenance::DecoderProvenance>,
    output: DecodedFrameOutput,
    counters: Arc<Counters>,
    failures: Arc<FailureReporter>,
    in_flight: Arc<InFlight>,
    color_space: VideoColorSpace,
    bit_depth: VideoBitDepth,
    chroma: VideoChroma,
    transfer: VideoTransfer,
}

pub(super) struct VideoDecoder {
    session: Option<CFRetained<VTDecompressionSession>>,
    format_description: CFRetained<CMFormatDescription>,
    callback_context: Box<CallbackContext>,
    in_flight: Arc<InFlight>,
}

// VTDecompressionSession has no thread affinity. Shared owns VideoDecoder behind a Mutex, so
// decode, reconfiguration, and invalidation are serialized even when StreamSink moves threads.
unsafe impl Send for VideoDecoder {}

impl VideoDecoder {
    pub(super) fn new(
        format: &VideoFormat,
        output: DecodedFrameOutput,
        counters: Arc<Counters>,
        failures: Arc<FailureReporter>,
        maximum_in_flight: usize,
    ) -> Result<Self, BackendError> {
        let (color_space, transfer) = format.resolved_color()?;
        let format_description = create_format_description(format)?;
        if matches!(&output, DecodedFrameOutput::PresentationQueue { .. })
            && (format.chroma() == VideoChroma::Yuv444 || transfer == VideoTransfer::Pq)
        {
            return Err(BackendError::Metal(
                "HDR and 4:4:4 require embedded Metal presentation".into(),
            ));
        }
        let bitstream_depth =
            unsafe { format_description.extension(kCMFormatDescriptionExtension_BitsPerComponent) }
                .and_then(|value| value.downcast_ref::<CFNumber>().and_then(CFNumber::as_i32));
        let bit_depth = format.destination_bit_depth(bitstream_depth)?;
        if (format.chroma() == VideoChroma::Yuv444 || transfer == VideoTransfer::Pq)
            && bit_depth != VideoBitDepth::Ten
        {
            return Err(BackendError::Metal(
                "4:4:4 and HDR require ten-bit output".into(),
            ));
        }
        if transfer == VideoTransfer::Pq && color_space != VideoColorSpace::Bt2020 {
            return Err(BackendError::Metal(
                "PQ output requires negotiated BT.2020 color".into(),
            ));
        }
        let in_flight = Arc::new(InFlight {
            count: AtomicUsize::new(0),
            maximum: maximum_in_flight,
        });
        let mut callback_context = Box::new(CallbackContext {
            color: format.color(),
            provenance: Mutex::new(crate::provenance::DecoderProvenance::new(maximum_in_flight)),
            output,
            counters,
            failures,
            in_flight: Arc::clone(&in_flight),
            color_space,
            bit_depth,
            chroma: format.chroma(),
            transfer,
        });
        let callback = VTDecompressionOutputCallbackRecord {
            decompressionOutputCallback: Some(decompression_callback),
            decompressionOutputRefCon: (&mut *callback_context as *mut CallbackContext).cast(),
        };

        let full_range = if format.color().is_some() {
            let bitstream_range = unsafe {
                format_description.extension(kCMFormatDescriptionExtension_FullRangeVideo)
            }
            .map(|value| {
                value
                    .downcast_ref::<CFBoolean>()
                    .map(CFBoolean::value)
                    .ok_or_else(|| {
                        BackendError::Metal("invalid CoreMedia full-range extension".into())
                    })
            })
            .transpose()?;
            format.destination_full_range(bitstream_range)
        } else {
            false
        };
        let pixel_format = match (bit_depth, format.chroma(), full_range) {
            (VideoBitDepth::Ten, VideoChroma::Yuv444, true) => {
                kCVPixelFormatType_444YpCbCr10BiPlanarFullRange
            }
            (VideoBitDepth::Ten, VideoChroma::Yuv444, false) => {
                kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange
            }
            (VideoBitDepth::Eight, _, true) => kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
            (VideoBitDepth::Eight, _, false) => kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            (VideoBitDepth::Ten, _, true) => kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,
            (VideoBitDepth::Ten, _, false) => kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
        } as i32;
        let pixel_format_number = unsafe {
            CFNumber::new(
                None,
                CFNumberType::SInt32Type,
                (&pixel_format as *const i32).cast(),
            )
        }
        .ok_or_else(|| BackendError::Metal("failed to create CV pixel format number".into()))?;
        let empty_properties = make_dictionary(&[])?;
        let true_value = unsafe { kCFBooleanTrue }.ok_or_else(|| {
            BackendError::Metal("CoreFoundation true value is unavailable".into())
        })?;
        let pixel_format_key = unsafe { kCVPixelBufferPixelFormatTypeKey };
        let metal_compatibility_key = unsafe { kCVPixelBufferMetalCompatibilityKey };
        let io_surface_properties_key = unsafe { kCVPixelBufferIOSurfacePropertiesKey };
        let hardware_decoder_key =
            unsafe { kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder };
        let mut destination_entries = vec![
            (cf_ptr(metal_compatibility_key), cf_ptr(true_value)),
            (
                cf_ptr(io_surface_properties_key),
                cf_ptr(&*empty_properties),
            ),
        ];
        destination_entries.insert(0, (cf_ptr(pixel_format_key), cf_ptr(&*pixel_format_number)));
        let destination_attributes = make_dictionary(&destination_entries)?;
        let decoder_specification =
            make_dictionary(&[(cf_ptr(hardware_decoder_key), cf_ptr(true_value))])?;

        let mut session_ptr = ptr::null_mut();
        let status = unsafe {
            VTDecompressionSession::create(
                None,
                &format_description,
                Some(&decoder_specification),
                Some(&destination_attributes),
                &callback,
                NonNull::from(&mut session_ptr),
            )
        };
        check_status("VTDecompressionSessionCreate", status)?;
        let session_ptr = NonNull::new(session_ptr).ok_or(BackendError::AppleApi {
            api: "VTDecompressionSessionCreate",
            status: -1,
        })?;
        let session = unsafe { CFRetained::from_raw(session_ptr) };
        let realtime_status = unsafe {
            VTSessionSetProperty(
                session.as_ref(),
                kVTDecompressionPropertyKey_RealTime,
                Some(true_value.as_ref()),
            )
        };
        if realtime_status != 0 {
            eprintln!(
                "VideoToolbox declined the real-time decode hint: OSStatus {realtime_status}"
            );
        }

        Ok(Self {
            session: Some(session),
            format_description,
            callback_context,
            in_flight,
        })
    }

    pub(super) fn submit(
        &self,
        avcc_access_unit: &[u8],
        timing: FrameTiming,
    ) -> Result<bool, BackendError> {
        if !self.in_flight.try_acquire() {
            return Ok(false);
        }
        let token = self
            .callback_context
            .provenance
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(timing.provenance);
        let Some(token) = token else {
            self.in_flight.release();
            return Ok(false);
        };
        let result = self.submit_acquired(avcc_access_unit, timing, token);
        if result.is_err() {
            if self
                .callback_context
                .provenance
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .take(token)
                .is_some()
            {
                self.in_flight.release();
            }
            self.callback_context
                .counters
                .video_decode_errors
                .fetch_add(1, Ordering::Relaxed);
            let status = match &result {
                Err(BackendError::AppleApi { status, .. }) => Some(*status),
                _ => None,
            };
            self.callback_context.failures.video_decode_failed(status);
        }
        result.map(|()| true)
    }

    fn submit_acquired(
        &self,
        avcc_access_unit: &[u8],
        timing: FrameTiming,
        token: usize,
    ) -> Result<(), BackendError> {
        let mut block_ptr = ptr::null_mut();
        let status = unsafe {
            CMBlockBuffer::create_with_memory_block(
                None,
                ptr::null_mut(),
                avcc_access_unit.len(),
                None,
                ptr::null(),
                0,
                avcc_access_unit.len(),
                0,
                NonNull::from(&mut block_ptr),
            )
        };
        check_status("CMBlockBufferCreateWithMemoryBlock", status)?;
        let block_ptr = NonNull::new(block_ptr).ok_or(BackendError::AppleApi {
            api: "CMBlockBufferCreateWithMemoryBlock",
            status: -1,
        })?;
        let block = unsafe { CFRetained::from_raw(block_ptr) };
        let source = NonNull::new(avcc_access_unit.as_ptr().cast_mut().cast::<c_void>())
            .expect("validated AVCC access unit is not empty");
        let status =
            unsafe { CMBlockBuffer::replace_data_bytes(source, &block, 0, avcc_access_unit.len()) };
        check_status("CMBlockBufferReplaceDataBytes", status)?;

        let sample_timing = CMSampleTimingInfo {
            duration: unsafe { CMTime::new(timing.duration_value, timing.timescale) },
            presentationTimeStamp: unsafe {
                CMTime::new(timing.presentation_value, timing.timescale)
            },
            decodeTimeStamp: unsafe { kCMTimeInvalid },
        };
        let sample_size = avcc_access_unit.len();
        let mut sample_ptr = ptr::null_mut();
        let status = unsafe {
            CMSampleBuffer::create_ready(
                None,
                Some(&block),
                Some(&self.format_description),
                1,
                1,
                &sample_timing,
                1,
                &sample_size,
                NonNull::from(&mut sample_ptr),
            )
        };
        check_status("CMSampleBufferCreateReady", status)?;
        let sample_ptr = NonNull::new(sample_ptr).ok_or(BackendError::AppleApi {
            api: "CMSampleBufferCreateReady",
            status: -1,
        })?;
        let sample = unsafe { CFRetained::from_raw(sample_ptr) };
        let session = self.session.as_ref().ok_or(BackendError::Stopped)?;
        let status = unsafe {
            session.decode_frame(
                &sample,
                VTDecodeFrameFlags::Frame_EnableAsynchronousDecompression,
                ptr::without_provenance_mut(token),
                ptr::null_mut(),
            )
        };
        check_status("VTDecompressionSessionDecodeFrame", status)
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        self.callback_context
            .provenance
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .invalidate();
        if let Some(session) = self.session.take() {
            let _ = unsafe { session.wait_for_asynchronous_frames() };
            unsafe { session.invalidate() };
            drop(session);
        }
        debug_assert_eq!(self.in_flight.count.load(Ordering::Acquire), 0);
        let _ = &self.callback_context;
    }
}

unsafe extern "C-unwind" fn decompression_callback(
    output_refcon: *mut c_void,
    source_refcon: *mut c_void,
    status: i32,
    _info_flags: VTDecodeInfoFlags,
    image_buffer: *mut CVImageBuffer,
    presentation_time_stamp: CMTime,
    presentation_duration: CMTime,
) {
    let Some(context) = NonNull::new(output_refcon.cast::<CallbackContext>()) else {
        return;
    };
    let context = unsafe { context.as_ref() };
    let provenance = context
        .provenance
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .take(source_refcon.addr());
    let correlated = provenance.is_some();
    let provenance = provenance.unwrap_or_default();
    if status == 0 {
        if let Some(image_buffer) = NonNull::new(image_buffer) {
            let image = unsafe { CFRetained::retain(image_buffer) };
            let pixel_format = CVPixelBufferGetPixelFormatType(&image);
            let preserves_format = match (context.bit_depth, context.chroma) {
                (VideoBitDepth::Ten, VideoChroma::Yuv444) => {
                    pixel_format == kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange
                        || pixel_format == kCVPixelFormatType_444YpCbCr10BiPlanarFullRange
                }
                (VideoBitDepth::Ten, VideoChroma::Yuv420) => {
                    pixel_format == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange
                        || pixel_format == kCVPixelFormatType_420YpCbCr10BiPlanarFullRange
                }
                (VideoBitDepth::Eight, VideoChroma::Yuv420) => true,
                _ => false,
            };
            if !preserves_format {
                context
                    .counters
                    .video_decode_errors
                    .fetch_add(1, Ordering::Relaxed);
                context.failures.report_fatal(
                    BackendSubsystem::VideoToolbox,
                    format!("VideoToolbox did not preserve negotiated depth/chroma: pixel format {pixel_format:#010x}"),
                );
                if correlated {
                    context.in_flight.release();
                }
                return;
            }
            let frame = DecodedFrame {
                color: context.color,
                provenance,
                image,
                color_space: context.color_space,
                transfer: context.transfer,
                minimum_frame_duration_seconds: frame_duration_seconds(presentation_duration),
                timestamp_100ns: time_to_100ns(presentation_time_stamp),
            };
            context
                .counters
                .video_decoded
                .fetch_add(1, Ordering::Relaxed);
            context.failures.video_decode_succeeded();
            if context.output.publish(frame) {
                context
                    .counters
                    .video_frames_dropped
                    .fetch_add(1, Ordering::Relaxed);
                context
                    .counters
                    .video_decoded_queue_dropped
                    .fetch_add(1, Ordering::Relaxed);
            }
        } else {
            context
                .counters
                .video_decode_errors
                .fetch_add(1, Ordering::Relaxed);
            context.failures.video_decode_failed(None);
        }
    } else {
        context
            .counters
            .video_decode_errors
            .fetch_add(1, Ordering::Relaxed);
        context.failures.video_decode_failed(Some(status));
    }
    if correlated {
        context.in_flight.release();
    }
}

fn frame_duration_seconds(duration: CMTime) -> f64 {
    if duration.value > 0 && duration.timescale > 0 {
        (duration.value as f64 / f64::from(duration.timescale)).clamp(1.0 / 360.0, 1.0 / 24.0)
    } else {
        1.0 / 60.0
    }
}

fn time_to_100ns(time: CMTime) -> i64 {
    if time.timescale <= 0 {
        return 0;
    }
    i128::from(time.value)
        .saturating_mul(10_000_000)
        .checked_div(i128::from(time.timescale))
        .and_then(|value| i64::try_from(value).ok())
        .unwrap_or_else(|| {
            if time.value.is_negative() {
                i64::MIN
            } else {
                i64::MAX
            }
        })
}

fn create_format_description(
    format: &VideoFormat,
) -> Result<CFRetained<CMFormatDescription>, BackendError> {
    format.resolved_color()?;
    let description = match format {
        VideoFormat::H264(format) => create_h264_format_description(format),
        VideoFormat::H265(format) => create_h265_format_description(format),
        VideoFormat::Av1(format) => create_av1_format_description(format),
    }?;
    let Some(color) = format.color() else {
        return Ok(description);
    };
    use opennow_media_protocol::{ChromaLocation, ColorRange, Matrix, Transfer};
    let existing = unsafe { description.extensions() }
        .ok_or_else(|| BackendError::Metal("video format has no codec extensions".into()))?;
    let extensions = unsafe { CFMutableDictionary::new_copy(None, 0, Some(&existing)) }
        .ok_or_else(|| BackendError::Metal("failed to copy video format extensions".into()))?;
    let full_range = unsafe {
        match color.range {
            ColorRange::Full => kCFBooleanTrue,
            ColorRange::Limited => kCFBooleanFalse,
        }
    }
    .ok_or_else(|| BackendError::Metal("CoreFoundation boolean is unavailable".into()))?;
    let transfer = unsafe {
        match color.transfer {
            Transfer::Bt709 => kCMFormatDescriptionTransferFunction_ITU_R_709_2,
            Transfer::Srgb => kCMFormatDescriptionTransferFunction_sRGB,
            _ => return Err(crate::format::FormatError::UnsupportedExplicitColor.into()),
        }
    };
    let matrix = unsafe {
        match color.matrix {
            Matrix::Bt601 => kCMFormatDescriptionYCbCrMatrix_ITU_R_601_4,
            Matrix::Bt709 => kCMFormatDescriptionYCbCrMatrix_ITU_R_709_2,
            Matrix::Bt2020NonConstant => {
                return Err(crate::format::FormatError::UnsupportedExplicitColor.into());
            }
        }
    };
    let chroma = unsafe {
        match color.chroma_location {
            ChromaLocation::Left => kCMFormatDescriptionChromaLocation_Left,
            ChromaLocation::Center => kCMFormatDescriptionChromaLocation_Center,
        }
    };
    let defaults = unsafe {
        [
            (
                kCMFormatDescriptionExtension_ColorPrimaries,
                cf_ptr(kCMFormatDescriptionColorPrimaries_ITU_R_709_2),
            ),
            (
                kCMFormatDescriptionExtension_TransferFunction,
                cf_ptr(transfer),
            ),
            (kCMFormatDescriptionExtension_YCbCrMatrix, cf_ptr(matrix)),
            (
                kCMFormatDescriptionExtension_FullRangeVideo,
                cf_ptr(full_range),
            ),
            (
                kCMFormatDescriptionExtension_ChromaLocationTopField,
                cf_ptr(chroma),
            ),
            (
                kCMFormatDescriptionExtension_ChromaLocationBottomField,
                cf_ptr(chroma),
            ),
        ]
    };
    for (key, value) in defaults {
        unsafe { CFMutableDictionary::add_value(Some(&extensions), cf_ptr(key), value) };
    }
    let dimensions = unsafe { CMVideoFormatDescriptionGetDimensions(&description) };
    let mut updated = ptr::null();
    let status = unsafe {
        CMVideoFormatDescriptionCreate(
            None,
            description.media_sub_type(),
            dimensions.width,
            dimensions.height,
            Some(&extensions),
            NonNull::from(&mut updated),
        )
    };
    check_status("CMVideoFormatDescriptionCreate(color defaults)", status)?;
    let updated = NonNull::new(updated.cast_mut()).ok_or(BackendError::AppleApi {
        api: "CMVideoFormatDescriptionCreate(color defaults)",
        status: -1,
    })?;
    Ok(unsafe { CFRetained::from_raw(updated) })
}

fn create_av1_format_description(
    format: &Av1Format,
) -> Result<CFRetained<CMFormatDescription>, BackendError> {
    let atom_name = CFString::from_static_str("av1C");
    let atom_data = CFData::from_bytes(format.codec_configuration());
    let atoms = make_dictionary(&[(cf_ptr(&*atom_name), cf_ptr(&*atom_data))])?;
    let extensions = make_dictionary(&[(
        cf_ptr(unsafe { kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms }),
        cf_ptr(&*atoms),
    )])?;
    let mut description_ptr: *const CMFormatDescription = ptr::null();
    let status = unsafe {
        CMVideoFormatDescriptionCreate(
            None,
            kCMVideoCodecType_AV1,
            format.width(),
            format.height(),
            Some(&extensions),
            NonNull::from(&mut description_ptr),
        )
    };
    check_status("CMVideoFormatDescriptionCreate(AV1)", status)?;
    let description_ptr =
        NonNull::new(description_ptr.cast_mut()).ok_or(BackendError::AppleApi {
            api: "CMVideoFormatDescriptionCreate(AV1)",
            status: -1,
        })?;
    Ok(unsafe { CFRetained::from_raw(description_ptr) })
}

fn create_h264_format_description(
    format: &H264Format,
) -> Result<CFRetained<CMFormatDescription>, BackendError> {
    let mut pointers = [
        NonNull::new(format.parameter_sets.sequence().as_ptr().cast_mut())
            .expect("validated SPS is non-empty"),
        NonNull::new(format.parameter_sets.picture().as_ptr().cast_mut())
            .expect("validated PPS is non-empty"),
    ];
    let mut sizes = [
        format.parameter_sets.sequence().len(),
        format.parameter_sets.picture().len(),
    ];
    let mut description_ptr: *const CMFormatDescription = ptr::null();
    let status = unsafe {
        CMVideoFormatDescriptionCreateFromH264ParameterSets(
            None,
            pointers.len(),
            NonNull::new(pointers.as_mut_ptr()).expect("parameter set array is non-empty"),
            NonNull::new(sizes.as_mut_ptr()).expect("parameter set size array is non-empty"),
            4,
            NonNull::from(&mut description_ptr),
        )
    };
    check_status(
        "CMVideoFormatDescriptionCreateFromH264ParameterSets",
        status,
    )?;
    let description_ptr =
        NonNull::new(description_ptr.cast_mut()).ok_or(BackendError::AppleApi {
            api: "CMVideoFormatDescriptionCreateFromH264ParameterSets",
            status: -1,
        })?;
    Ok(unsafe { CFRetained::from_raw(description_ptr) })
}

fn create_h265_format_description(
    format: &H265Format,
) -> Result<CFRetained<CMFormatDescription>, BackendError> {
    let mut pointers = [
        NonNull::new(format.parameter_sets.video().as_ptr().cast_mut())
            .expect("validated VPS is non-empty"),
        NonNull::new(format.parameter_sets.sequence().as_ptr().cast_mut())
            .expect("validated SPS is non-empty"),
        NonNull::new(format.parameter_sets.picture().as_ptr().cast_mut())
            .expect("validated PPS is non-empty"),
    ];
    let mut sizes = [
        format.parameter_sets.video().len(),
        format.parameter_sets.sequence().len(),
        format.parameter_sets.picture().len(),
    ];
    let mut description_ptr: *const CMFormatDescription = ptr::null();
    let status = unsafe {
        CMVideoFormatDescriptionCreateFromHEVCParameterSets(
            None,
            pointers.len(),
            NonNull::new(pointers.as_mut_ptr()).expect("parameter set array is non-empty"),
            NonNull::new(sizes.as_mut_ptr()).expect("parameter set size array is non-empty"),
            4,
            None,
            NonNull::from(&mut description_ptr),
        )
    };
    check_status(
        "CMVideoFormatDescriptionCreateFromHEVCParameterSets",
        status,
    )?;
    let description_ptr =
        NonNull::new(description_ptr.cast_mut()).ok_or(BackendError::AppleApi {
            api: "CMVideoFormatDescriptionCreateFromHEVCParameterSets",
            status: -1,
        })?;
    Ok(unsafe { CFRetained::from_raw(description_ptr) })
}

fn make_dictionary(
    entries: &[(*const c_void, *const c_void)],
) -> Result<CFRetained<CFDictionary>, BackendError> {
    let mut keys: Vec<_> = entries.iter().map(|(key, _)| *key).collect();
    let mut values: Vec<_> = entries.iter().map(|(_, value)| *value).collect();
    unsafe {
        CFDictionary::new(
            None,
            keys.as_mut_ptr(),
            values.as_mut_ptr(),
            entries.len() as isize,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
    }
    .ok_or_else(|| BackendError::Metal("failed to create CoreFoundation dictionary".into()))
}

fn cf_ptr<T>(value: &T) -> *const c_void {
    (value as *const T).cast()
}

fn check_status(api: &'static str, status: i32) -> Result<(), BackendError> {
    if status == 0 {
        Ok(())
    } else {
        Err(BackendError::AppleApi { api, status })
    }
}

#[cfg(test)]
mod tests {
    use super::{InFlight, frame_duration_seconds, time_to_100ns};

    #[test]
    fn two_decoded_outputs_publish_exact_provenance_once_without_recording() {
        use super::*;
        use crate::EmbeddedFrameProducer;
        use objc2_core_video::CVPixelBufferCreate;
        use opennow_media_protocol::{FrameProvenance, SourceStamp};

        let mut image = ptr::null_mut();
        assert_eq!(
            unsafe {
                CVPixelBufferCreate(
                    None,
                    16,
                    16,
                    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
                    None,
                    NonNull::from(&mut image),
                )
            },
            0
        );
        let image = unsafe { CFRetained::from_raw(NonNull::new(image).unwrap()) };
        for embedded in [false, true] {
            let received = Arc::new(Mutex::new(Vec::new()));
            let counters = Arc::new(Counters::default());
            let failures = Arc::new(FailureReporter::default());
            let mailbox = Arc::new(LatestMailbox::new());
            let producer = EmbeddedFrameProducer::new(
                Arc::clone(&mailbox),
                Arc::clone(&counters),
                Arc::clone(&failures),
            );
            let feedback = Arc::clone(&received);
            let output = if embedded {
                let publisher = producer.clone();
                DecodedFrameOutput::EmbeddedMailbox {
                    mailbox,
                    publish: Some(Arc::new(move |frame| {
                        let frame = publisher.decoded_frame(frame);
                        feedback.lock().unwrap().push(frame.provenance());
                        false
                    })),
                }
            } else {
                DecodedFrameOutput::PresentationQueue {
                    queue: Arc::new(BoundedQueue::new(1)),
                    decoded: Some(Arc::new(move |provenance| {
                        feedback.lock().unwrap().push(provenance);
                    })),
                }
            };
            let in_flight = Arc::new(InFlight {
                count: AtomicUsize::new(0),
                maximum: 3,
            });
            let mut context = CallbackContext {
                color: None,
                provenance: Mutex::new(crate::provenance::DecoderProvenance::new(3)),
                output,
                counters: Arc::clone(&counters),
                failures,
                in_flight: Arc::clone(&in_flight),
                color_space: VideoColorSpace::Bt709,
                bit_depth: VideoBitDepth::Eight,
                chroma: VideoChroma::Yuv420,
                transfer: VideoTransfer::Sdr,
            };
            let sources = [Some(u64::MAX), Some(0)].map(|sender_frame_id| FrameProvenance {
                attempt_generation: 23,
                track_id: 5,
                source: Some(SourceStamp {
                    sender_frame_id,
                    timestamp: u64::MAX,
                    clock_rate_hz: 90_000,
                    ssrc: Some(0),
                }),
            });
            let tokens = sources.map(|provenance| {
                assert!(in_flight.try_acquire());
                context
                    .provenance
                    .lock()
                    .unwrap()
                    .insert(provenance)
                    .unwrap()
            });
            for index in [1, 0] {
                unsafe {
                    decompression_callback(
                        ptr::from_mut(&mut context).cast(),
                        ptr::without_provenance_mut(tokens[index]),
                        0,
                        VTDecodeInfoFlags(0),
                        ptr::from_ref::<CVImageBuffer>(&image).cast_mut(),
                        CMTime::new(0, 90_000),
                        CMTime::new(1500, 90_000),
                    )
                };
            }
            assert_eq!(*received.lock().unwrap(), [sources[1], sources[0]]);
            assert_eq!(counters.video_decoded.load(Ordering::Relaxed), 2);
            assert_eq!(in_flight.count.load(Ordering::Acquire), 0);
            assert!(producer.acquire_latest().is_none());
            context.output.clear();
            assert_eq!(received.lock().unwrap().len(), 2);
        }
    }

    #[test]
    fn initial_av1_description_contains_explicit_color_defaults_and_codec_atoms() {
        use super::*;
        use opennow_media_protocol::{
            ChromaLocation, ColorDescription, ColorRange, Matrix, Primaries, Transfer,
        };
        let color = ColorDescription {
            range: ColorRange::Full,
            primaries: Primaries::Bt709,
            transfer: Transfer::Srgb,
            matrix: Matrix::Bt601,
            chroma_location: ChromaLocation::Left,
        };
        let format: VideoFormat =
            Av1Format::new([0x81, 0x0d, 0x0c, 0], 1920, 1080, VideoColorSpace::Bt709)
                .unwrap()
                .with_color(Some(color))
                .into();
        let description = create_format_description(&format).unwrap();
        for (key, expected) in unsafe {
            [
                (
                    kCMFormatDescriptionExtension_ColorPrimaries,
                    kCMFormatDescriptionColorPrimaries_ITU_R_709_2,
                ),
                (
                    kCMFormatDescriptionExtension_TransferFunction,
                    kCMFormatDescriptionTransferFunction_sRGB,
                ),
                (
                    kCMFormatDescriptionExtension_YCbCrMatrix,
                    kCMFormatDescriptionYCbCrMatrix_ITU_R_601_4,
                ),
                (
                    kCMFormatDescriptionExtension_ChromaLocationTopField,
                    kCMFormatDescriptionChromaLocation_Left,
                ),
                (
                    kCMFormatDescriptionExtension_ChromaLocationBottomField,
                    kCMFormatDescriptionChromaLocation_Left,
                ),
            ]
        } {
            assert_eq!(
                unsafe { description.extension(key) }
                    .unwrap()
                    .downcast_ref::<CFString>(),
                Some(expected)
            );
        }
        assert!(
            unsafe { description.extension(kCMFormatDescriptionExtension_FullRangeVideo) }
                .unwrap()
                .downcast_ref::<CFBoolean>()
                .unwrap()
                .value()
        );
        assert!(
            unsafe {
                description.extension(kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms)
            }
            .is_some()
        );
    }
    use objc2_core_media::{CMTime, CMTimeFlags};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn in_flight_admission_preserves_zero_limit_and_releases_capacity() {
        let disabled = InFlight {
            count: AtomicUsize::new(0),
            maximum: 0,
        };
        assert!(!disabled.try_acquire());
        assert_eq!(disabled.count.load(Ordering::Acquire), 0);

        let in_flight = InFlight {
            count: AtomicUsize::new(0),
            maximum: 2,
        };
        assert!(in_flight.try_acquire());
        assert!(in_flight.try_acquire());
        assert!(!in_flight.try_acquire());
        in_flight.release();
        assert!(in_flight.try_acquire());
        assert_eq!(in_flight.count.load(Ordering::Acquire), 2);
    }

    #[test]
    fn concurrent_in_flight_admission_never_exceeds_the_limit() {
        let in_flight = InFlight {
            count: AtomicUsize::new(0),
            maximum: 3,
        };
        let acquired = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..16 {
                scope.spawn(|| {
                    if in_flight.try_acquire() {
                        acquired.fetch_add(1, Ordering::Relaxed);
                    }
                });
            }
        });
        assert_eq!(acquired.load(Ordering::Relaxed), 3);
        assert_eq!(in_flight.count.load(Ordering::Acquire), 3);
        for _ in 0..3 {
            in_flight.release();
        }
        assert_eq!(in_flight.count.load(Ordering::Acquire), 0);
    }

    #[test]
    fn converts_120_hz_core_media_duration_to_seconds() {
        let duration = CMTime {
            value: 750,
            timescale: 90_000,
            flags: CMTimeFlags(1),
            epoch: 0,
        };
        assert!((frame_duration_seconds(duration) - 1.0 / 120.0).abs() < f64::EPSILON);
    }

    #[test]
    fn converts_top_tier_core_media_duration_without_clamping_to_240() {
        let duration = CMTime {
            value: 250,
            timescale: 90_000,
            flags: CMTimeFlags(1),
            epoch: 0,
        };
        assert!((frame_duration_seconds(duration) - 1.0 / 360.0).abs() < f64::EPSILON);
    }

    #[test]
    fn converts_core_media_time_to_cross_platform_100ns_units() {
        let time = CMTime {
            value: 90_000,
            timescale: 90_000,
            flags: CMTimeFlags(1),
            epoch: 0,
        };
        assert_eq!(time_to_100ns(time), 10_000_000);
    }

    #[test]
    #[ignore = "requires a Mac with VideoToolbox hardware decode and a Metal device"]
    fn hardware_decode_to_embedded_metal_survives_surface_retirement() {
        use super::*;
        use crate::{AdoptedMetalContext, EmbeddedFrameProducer, H264ParameterSets};
        use objc2::rc::Retained;
        use objc2_metal::{
            MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandQueue,
            MTLCreateSystemDefaultDevice, MTLDevice,
        };

        let sps = [
            0x67, 0x42, 0xc0, 0x0a, 0xd9, 0x04, 0x26, 0xc0, 0x44, 0x00, 0x00, 0x03, 0x00, 0x04,
            0x00, 0x00, 0x03, 0x01, 0xe2, 0x3c, 0x48, 0x99, 0x20,
        ];
        let pps = [0x68, 0xcb, 0x83, 0xcb, 0x20];
        let idr = [
            0x65, 0x88, 0x84, 0x04, 0xbc, 0x98, 0xa0, 0x00, 0x38, 0xa3, 0x27, 0x27, 0x27, 0x5d,
            0x75, 0xd7, 0x5d, 0x75, 0xd7, 0x5d, 0x75, 0xd7, 0x80,
        ];
        let format = H264Format::new(
            H264ParameterSets::new(sps, pps).unwrap(),
            VideoColorSpace::Bt709,
        )
        .into();
        let device = MTLCreateSystemDefaultDevice().expect("Metal device");
        let queue = device.newCommandQueue().expect("Metal command queue");
        let mailbox = Arc::new(LatestMailbox::new());
        let counters = Arc::new(Counters::default());
        let failures = Arc::new(FailureReporter::default());
        let producer = EmbeddedFrameProducer::new(
            Arc::clone(&mailbox),
            Arc::clone(&counters),
            Arc::clone(&failures),
        );
        let decoder = VideoDecoder::new(
            &format,
            DecodedFrameOutput::EmbeddedMailbox {
                mailbox,
                publish: None,
            },
            Arc::clone(&counters),
            Arc::clone(&failures),
            3,
        )
        .expect("hardware VideoToolbox session");
        let mut sample = (idr.len() as u32).to_be_bytes().to_vec();
        sample.extend_from_slice(&idr);
        for sender_frame_id in [Some(u64::MAX), Some(0), None] {
            let provenance = opennow_media_protocol::FrameProvenance {
                attempt_generation: 3,
                track_id: 7,
                source: Some(opennow_media_protocol::SourceStamp {
                    sender_frame_id,
                    timestamp: u64::MAX,
                    clock_rate_hz: 90_000,
                    ssrc: Some(0),
                }),
            };
            assert!(
                decoder
                    .submit(
                        &sample,
                        FrameTiming::from_90khz(90_000, 1500).with_provenance(provenance)
                    )
                    .unwrap()
            );
            assert_eq!(
                unsafe {
                    decoder
                        .session
                        .as_ref()
                        .unwrap()
                        .wait_for_asynchronous_frames()
                },
                0
            );
            let frame = producer.acquire_latest().expect("decoded hardware frame");
            assert_eq!((frame.width(), frame.height()), (64, 64));
            assert_eq!(frame.presentation_time_ns(), 1_000_000_000);
            let command = queue.commandBuffer().expect("Metal command buffer");
            let recorded = unsafe {
                frame.record(
                    AdoptedMetalContext {
                        device: Retained::as_ptr(&device).cast_mut().cast(),
                        command_buffer: Retained::as_ptr(&command).cast_mut().cast(),
                        upscale_width: 0,
                        upscale_height: 0,
                        upscale_sharpness: 10,
                        upscale_denoise: 0,
                    },
                    0,
                )
            }
            .expect("zero-copy IOSurface import and Metal conversion");
            assert_eq!(recorded.provenance, provenance);
            assert!(!recorded.texture.is_null());
            assert_eq!((recorded.width, recorded.height), (64, 64));
            producer.release_graphics_resources();
            drop(frame);
            command.commit();
            command.waitUntilCompleted();
            assert_eq!(command.status(), MTLCommandBufferStatus::Completed);
        }
        assert_eq!(counters.snapshot().video_metal_completed, 3);
        assert_eq!(counters.snapshot().video_present_errors, 0);
        assert!(failures.fatal_failure().is_none());
    }
}
