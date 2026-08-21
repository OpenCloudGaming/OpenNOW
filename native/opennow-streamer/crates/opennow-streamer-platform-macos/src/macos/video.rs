use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use objc2_core_foundation::{
    CFDictionary, CFNumber, CFNumberType, CFRetained, kCFBooleanTrue,
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks,
};
use objc2_core_media::{
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMSampleTimingInfo, CMTime,
    CMVideoFormatDescriptionCreateFromH264ParameterSets, kCMTimeInvalid,
};
use objc2_core_video::{
    CVImageBuffer, kCVPixelBufferIOSurfacePropertiesKey, kCVPixelBufferMetalCompatibilityKey,
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
};
use objc2_video_toolbox::{
    VTDecodeFrameFlags, VTDecodeInfoFlags, VTDecompressionOutputCallbackRecord,
    VTDecompressionSession, kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder,
};

use crate::format::{FrameTiming, H264Format, VideoColorSpace};
use crate::queue::{BoundedQueue, PushResult};

use super::{BackendError, Counters};

pub(super) struct DecodedFrame {
    pub(super) image: CFRetained<CVImageBuffer>,
    pub(super) color_space: VideoColorSpace,
}

// The callback retains the CVImageBuffer and no code mutates it after publication to the queue.
unsafe impl Send for DecodedFrame {}

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
    queue: Arc<BoundedQueue<DecodedFrame>>,
    counters: Arc<Counters>,
    in_flight: Arc<InFlight>,
    color_space: VideoColorSpace,
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
        format: &H264Format,
        queue: Arc<BoundedQueue<DecodedFrame>>,
        counters: Arc<Counters>,
        maximum_in_flight: usize,
    ) -> Result<Self, BackendError> {
        let format_description = create_format_description(format)?;
        let in_flight = Arc::new(InFlight {
            count: AtomicUsize::new(0),
            maximum: maximum_in_flight,
        });
        let mut callback_context = Box::new(CallbackContext {
            queue,
            counters,
            in_flight: Arc::clone(&in_flight),
            color_space: format.color_space,
        });
        let callback = VTDecompressionOutputCallbackRecord {
            decompressionOutputCallback: Some(decompression_callback),
            decompressionOutputRefCon: (&mut *callback_context as *mut CallbackContext).cast(),
        };

        let pixel_format = kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange as i32;
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
        let destination_attributes = make_dictionary(&[
            (cf_ptr(pixel_format_key), cf_ptr(&*pixel_format_number)),
            (cf_ptr(metal_compatibility_key), cf_ptr(true_value)),
            (
                cf_ptr(io_surface_properties_key),
                cf_ptr(&*empty_properties),
            ),
        ])?;
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
    _presentation_time_stamp: CMTime,
    _presentation_duration: CMTime,
) {
    let Some(context) = NonNull::new(output_refcon.cast::<CallbackContext>()) else {
        return;
    };
    let context = unsafe { context.as_ref() };
    if status == 0 {
        if let Some(image_buffer) = NonNull::new(image_buffer) {
            let image = unsafe { CFRetained::retain(image_buffer) };
            let frame = DecodedFrame {
                image,
                color_space: context.color_space,
            };
            context
                .counters
                .video_decoded
                .fetch_add(1, Ordering::Relaxed);
            if matches!(
                context.queue.push_drop_oldest(frame),
                PushResult::Replaced(_) | PushResult::Closed(_)
            ) {
                context
                    .counters
                    .video_frames_dropped
                    .fetch_add(1, Ordering::Relaxed);
            }
        } else {
            context
                .counters
                .video_decode_errors
                .fetch_add(1, Ordering::Relaxed);
        }
    } else {
        context
            .counters
            .video_decode_errors
            .fetch_add(1, Ordering::Relaxed);
    }
    context.in_flight.release();
}

fn create_format_description(
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
