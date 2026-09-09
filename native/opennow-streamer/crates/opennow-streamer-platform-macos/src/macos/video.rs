use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use objc2_core_foundation::{
    CFData, CFDictionary, CFNumber, CFNumberType, CFRetained, CFString, kCFBooleanTrue,
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks,
};
use objc2_core_media::{
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMSampleTimingInfo, CMTime,
    CMVideoFormatDescriptionCreate, CMVideoFormatDescriptionCreateFromH264ParameterSets,
    CMVideoFormatDescriptionCreateFromHEVCParameterSets,
    kCMFormatDescriptionExtension_BitsPerComponent,
    kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms, kCMTimeInvalid,
    kCMVideoCodecType_AV1,
};
use objc2_core_video::{
    CVImageBuffer, CVPixelBufferGetPixelFormatType, kCVPixelBufferIOSurfacePropertiesKey,
    kCVPixelBufferMetalCompatibilityKey, kCVPixelBufferPixelFormatTypeKey,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
};
use objc2_video_toolbox::{
    VTDecodeFrameFlags, VTDecodeInfoFlags, VTDecompressionOutputCallbackRecord,
    VTDecompressionSession, VTSessionSetProperty, kVTDecompressionPropertyKey_RealTime,
    kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder,
};

use crate::failure::{BackendSubsystem, FailureReporter};
use crate::format::{
    Av1Format, FrameTiming, H264Format, H265Format, VideoBitDepth, VideoColorSpace, VideoFormat,
};
use crate::queue::{BoundedQueue, PushResult};

use super::mailbox::LatestMailbox;
use super::{BackendError, Counters};

#[derive(Clone)]
pub(super) struct DecodedFrame {
    pub(super) image: CFRetained<CVImageBuffer>,
    pub(super) color_space: VideoColorSpace,
    pub(super) minimum_frame_duration_seconds: f64,
    pub(super) timestamp_100ns: i64,
}

// The callback retains the CVImageBuffer and no code mutates it after publication to the queue.
unsafe impl Send for DecodedFrame {}
unsafe impl Sync for DecodedFrame {}

#[derive(Clone)]
pub(super) enum DecodedFrameOutput {
    PresentationQueue(Arc<BoundedQueue<DecodedFrame>>),
    EmbeddedMailbox {
        mailbox: Arc<LatestMailbox<DecodedFrame>>,
        frame_available: Option<Arc<dyn Fn() + Send + Sync>>,
    },
}

impl DecodedFrameOutput {
    fn publish(&self, frame: DecodedFrame) -> bool {
        match self {
            Self::PresentationQueue(queue) => matches!(
                queue.push_drop_oldest(frame),
                PushResult::Replaced(_) | PushResult::Closed(_)
            ),
            Self::EmbeddedMailbox {
                mailbox,
                frame_available,
            } => {
                let replaced = mailbox.replace(frame);
                if let Some(frame_available) = frame_available {
                    frame_available();
                }
                replaced
            }
        }
    }

    pub(super) fn clear(&self) -> usize {
        match self {
            Self::PresentationQueue(queue) => queue.clear(),
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
        self.count
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < self.maximum).then_some(count + 1)
            })
            .is_ok()
    }

    fn release(&self) {
        let previous = self.count.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0);
    }
}

struct CallbackContext {
    output: DecodedFrameOutput,
    counters: Arc<Counters>,
    failures: Arc<FailureReporter>,
    in_flight: Arc<InFlight>,
    color_space: VideoColorSpace,
    bit_depth: VideoBitDepth,
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
        let format_description = create_format_description(format)?;
        let bitstream_depth =
            unsafe { format_description.extension(kCMFormatDescriptionExtension_BitsPerComponent) }
                .and_then(|value| value.downcast_ref::<CFNumber>().and_then(CFNumber::as_i32));
        let bit_depth = format.destination_bit_depth(bitstream_depth)?;
        let in_flight = Arc::new(InFlight {
            count: AtomicUsize::new(0),
            maximum: maximum_in_flight,
        });
        let mut callback_context = Box::new(CallbackContext {
            output,
            counters,
            failures,
            in_flight: Arc::clone(&in_flight),
            color_space: format.color_space(),
            bit_depth,
        });
        let callback = VTDecompressionOutputCallbackRecord {
            decompressionOutputCallback: Some(decompression_callback),
            decompressionOutputRefCon: (&mut *callback_context as *mut CallbackContext).cast(),
        };

        let pixel_format = match bit_depth {
            VideoBitDepth::Eight => kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
            VideoBitDepth::Ten => kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
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
        let result = self.submit_acquired(avcc_access_unit, timing);
        if result.is_err() {
            self.in_flight.release();
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
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        check_status("VTDecompressionSessionDecodeFrame", status)
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
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
    _source_refcon: *mut c_void,
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
    if status == 0 {
        if let Some(image_buffer) = NonNull::new(image_buffer) {
            let image = unsafe { CFRetained::retain(image_buffer) };
            let pixel_format = CVPixelBufferGetPixelFormatType(&image);
            if context.bit_depth == VideoBitDepth::Ten
                && pixel_format != kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange
                && pixel_format != kCVPixelFormatType_420YpCbCr10BiPlanarFullRange
            {
                context
                    .counters
                    .video_decode_errors
                    .fetch_add(1, Ordering::Relaxed);
                context.failures.report_fatal(
                    BackendSubsystem::VideoToolbox,
                    format!("VideoToolbox did not preserve ten-bit output: pixel format {pixel_format:#010x}"),
                );
                context.in_flight.release();
                return;
            }
            let frame = DecodedFrame {
                image,
                color_space: context.color_space,
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
    context.in_flight.release();
}

fn frame_duration_seconds(duration: CMTime) -> f64 {
    if duration.value > 0 && duration.timescale > 0 {
        (duration.value as f64 / f64::from(duration.timescale)).clamp(1.0 / 240.0, 1.0 / 24.0)
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
    match format {
        VideoFormat::H264(format) => create_h264_format_description(format),
        VideoFormat::H265(format) => create_h265_format_description(format),
        VideoFormat::Av1(format) => create_av1_format_description(format),
    }
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
    use super::{frame_duration_seconds, time_to_100ns};
    use objc2_core_media::{CMTime, CMTimeFlags};

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
                frame_available: None,
            },
            Arc::clone(&counters),
            Arc::clone(&failures),
            3,
        )
        .expect("hardware VideoToolbox session");
        let mut sample = (idr.len() as u32).to_be_bytes().to_vec();
        sample.extend_from_slice(&idr);
        for _ in 0..3 {
            assert!(
                decoder
                    .submit(&sample, FrameTiming::from_90khz(90_000, 1500))
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
