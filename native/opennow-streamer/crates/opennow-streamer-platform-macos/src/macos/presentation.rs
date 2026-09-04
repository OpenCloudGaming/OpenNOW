use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use block2::RcBlock;
use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::CFRetained;
use objc2_core_video::{
    CVDisplayLink, CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVOptionFlags,
    CVPixelBufferGetHeightOfPlane, CVPixelBufferGetIOSurface, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetPlaneCount, CVPixelBufferGetWidthOfPlane, CVTimeStamp,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandEncoder, MTLCommandQueue,
    MTLCreateSystemDefaultDevice, MTLDevice, MTLDrawable, MTLLibrary, MTLLoadAction,
    MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder, MTLRenderPassDescriptor,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLStoreAction, MTLTexture, MTLViewport,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};

use crate::failure::{BackendSubsystem, FailureReporter};
use crate::format::VideoColorSpace;
use crate::queue::{BoundedQueue, TryPopResult};

use super::video::DecodedFrame;
use super::{BackendError, Counters};

const SHADER_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct VertexOut {
    float4 position [[position]];
    float2 texcoord;
};

vertex VertexOut video_vertex(uint vertex_id [[vertex_id]]) {
    const float2 positions[3] = { float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0) };
    const float2 texcoords[3] = { float2(0.0, 1.0), float2(2.0, 1.0), float2(0.0, -1.0) };
    VertexOut out;
    out.position = float4(positions[vertex_id], 0.0, 1.0);
    out.texcoord = texcoords[vertex_id];
    return out;
}

fragment float4 video_fragment(
    VertexOut in [[stage_in]],
    texture2d<float> luma [[texture(0)]],
    texture2d<float> chroma [[texture(1)]],
    constant uint &color_space [[buffer(0)]]) {
    constexpr sampler linear_sampler(coord::normalized, address::clamp_to_edge, filter::linear);
    float y = (luma.sample(linear_sampler, in.texcoord).r - (16.0 / 255.0)) * (255.0 / 219.0);
    float2 cbcr = chroma.sample(linear_sampler, in.texcoord).rg - float2(0.5);
    float3 rgb;
    if (color_space == 0) {
        rgb = float3(
            y + 1.596027 * cbcr.y,
            y - 0.391762 * cbcr.x - 0.812968 * cbcr.y,
            y + 2.017232 * cbcr.x);
    } else {
        rgb = float3(
            y + 1.792741 * cbcr.y,
            y - 0.213249 * cbcr.x - 0.532909 * cbcr.y,
            y + 2.112402 * cbcr.x);
    }
    return float4(saturate(rgb), 1.0);
}
"#;

type PresentedHandler = RcBlock<dyn Fn(NonNull<ProtocolObject<dyn MTLDrawable>>)>;

pub(super) struct PresenterHandle {
    queue: Arc<BoundedQueue<DecodedFrame>>,
    worker: Option<JoinHandle<()>>,
    diagnostics_stop: Arc<AtomicBool>,
    diagnostics_worker: Option<JoinHandle<()>>,
}

impl PresenterHandle {
    pub(super) fn start(
        layer: Retained<CAMetalLayer>,
        visible: Arc<AtomicBool>,
        queue: Arc<BoundedQueue<DecodedFrame>>,
        counters: Arc<Counters>,
        failures: Arc<FailureReporter>,
    ) -> Result<Self, BackendError> {
        let mut presenter = MetalPresenter::new(layer, Arc::clone(&counters))?;
        let telemetry = Arc::clone(&presenter.telemetry);
        let display_clock = DisplayClock::start()?;
        let worker_queue = Arc::clone(&queue);
        let worker_failures = Arc::clone(&failures);
        let worker_counters = Arc::clone(&counters);
        let worker = thread::Builder::new()
            .name("opennow-metal-present".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let mut tick = 0;
                    let mut primed = false;
                    loop {
                        if !display_clock.wait_next(&mut tick) {
                            break;
                        }
                        // Prime one complete measured decode burst before starting scanout.
                        if !primed && worker_queue.len() < 8 {
                            continue;
                        }
                        let frame = match worker_queue.pop_now() {
                            TryPopResult::Value(frame) => frame,
                            TryPopResult::Empty => {
                                worker_counters
                                    .video_display_underflows
                                    .fetch_add(1, Ordering::Relaxed);
                                continue;
                            }
                            TryPopResult::Closed => break,
                        };
                        primed = true;
                        if !visible.load(Ordering::Acquire) {
                            worker_counters
                                .video_frames_dropped
                                .fetch_add(1, Ordering::Relaxed);
                            continue;
                        }
                        let result = autoreleasepool(|_| presenter.present(frame));
                        match result {
                            Ok(()) => {
                                failures.metal_succeeded();
                            }
                            Err(error) => {
                                worker_counters
                                    .video_present_errors
                                    .fetch_add(1, Ordering::Relaxed);
                                if failures.metal_failed(error.to_string()) {
                                    break;
                                }
                            }
                        }
                    }
                    presenter.finish();
                }));
                if result.is_err() {
                    worker_failures.report_fatal(
                        BackendSubsystem::Metal,
                        "the Metal presentation worker stopped unexpectedly".to_owned(),
                    );
                }
            })
            .map_err(|_| BackendError::Thread("Metal presenter"))?;
        let diagnostics_stop = Arc::new(AtomicBool::new(false));
        let diagnostics_should_stop = Arc::clone(&diagnostics_stop);
        let diagnostics_worker = thread::Builder::new()
            .name("opennow-video-diagnostics".into())
            .spawn(move || {
                let mut previous = PipelineCounterSnapshot::default();
                while !diagnostics_should_stop.load(Ordering::Acquire) {
                    thread::sleep(Duration::from_secs(1));
                    if diagnostics_should_stop.load(Ordering::Acquire) {
                        break;
                    }
                    let current = PipelineCounterSnapshot::read(&counters);
                    let timing = telemetry.take_timing();
                    eprintln!(
                        "macOS video pipeline: encodedSubmitted={} (+{}) decoded={} (+{}) decodedQueueDropped={} (+{}) metalSubmitted={} (+{}) displayed={} (+{}) displayUnderflows={} (+{}) scanoutSkipped={} (+{}) totalDropped={} (+{}) backpressured={} decodeErrors={} presentErrors={} scanoutAverageMs={:.3} scanoutMaximumMs={:.3} missedRefreshes={}",
                        current.encoded_submitted,
                        current.encoded_submitted.saturating_sub(previous.encoded_submitted),
                        current.decoded,
                        current.decoded.saturating_sub(previous.decoded),
                        current.decoded_queue_dropped,
                        current.decoded_queue_dropped.saturating_sub(previous.decoded_queue_dropped),
                        current.metal_submitted,
                        current.metal_submitted.saturating_sub(previous.metal_submitted),
                        current.displayed,
                        current.displayed.saturating_sub(previous.displayed),
                        current.display_underflows,
                        current.display_underflows.saturating_sub(previous.display_underflows),
                        current.scanout_skipped,
                        current.scanout_skipped.saturating_sub(previous.scanout_skipped),
                        current.total_dropped,
                        current.total_dropped.saturating_sub(previous.total_dropped),
                        counters.video_backpressured.load(Ordering::Relaxed),
                        counters.video_decode_errors.load(Ordering::Relaxed),
                        counters.video_present_errors.load(Ordering::Relaxed),
                        timing.average_interval_seconds * 1_000.0,
                        timing.maximum_interval_seconds * 1_000.0,
                        timing.missed_refreshes,
                    );
                    previous = current;
                }
            })
            .map_err(|_| BackendError::Thread("video diagnostics"))?;
        Ok(Self {
            queue,
            worker: Some(worker),
            diagnostics_stop,
            diagnostics_worker: Some(diagnostics_worker),
        })
    }

    pub(super) fn stop(mut self) {
        self.diagnostics_stop.store(true, Ordering::Release);
        self.queue.close_and_discard();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.diagnostics_worker.take() {
            let _ = worker.join();
        }
    }
}

#[derive(Clone, Copy, Default)]
struct PipelineCounterSnapshot {
    encoded_submitted: u64,
    decoded: u64,
    decoded_queue_dropped: u64,
    metal_submitted: u64,
    displayed: u64,
    display_underflows: u64,
    scanout_skipped: u64,
    total_dropped: u64,
}

impl PipelineCounterSnapshot {
    fn read(counters: &Counters) -> Self {
        Self {
            encoded_submitted: counters.video_submitted.load(Ordering::Relaxed),
            decoded: counters.video_decoded.load(Ordering::Relaxed),
            decoded_queue_dropped: counters.video_decoded_queue_dropped.load(Ordering::Relaxed),
            metal_submitted: counters.video_metal_submitted.load(Ordering::Relaxed),
            displayed: counters.video_presented.load(Ordering::Relaxed),
            display_underflows: counters.video_display_underflows.load(Ordering::Relaxed),
            scanout_skipped: counters.video_scanout_skipped.load(Ordering::Relaxed),
            total_dropped: counters.video_frames_dropped.load(Ordering::Relaxed),
        }
    }
}

struct DisplayClockState {
    generation: AtomicU64,
    stopped: AtomicBool,
    wait_lock: Mutex<()>,
    ready: Condvar,
}

struct DisplayClock {
    link: CFRetained<CVDisplayLink>,
    state: Arc<DisplayClockState>,
}

// CVDisplayLink is explicitly designed to invoke a callback from its own high-priority thread.
unsafe impl Send for DisplayClock {}

impl DisplayClock {
    fn start() -> Result<Self, BackendError> {
        let state = Arc::new(DisplayClockState {
            generation: AtomicU64::new(0),
            stopped: AtomicBool::new(false),
            wait_lock: Mutex::new(()),
            ready: Condvar::new(),
        });
        let mut link_ptr = ptr::null_mut();
        #[allow(deprecated)]
        let status =
            unsafe { CVDisplayLink::create_with_active_cg_displays(NonNull::from(&mut link_ptr)) };
        if status != 0 {
            return Err(BackendError::AppleApi {
                api: "CVDisplayLinkCreateWithActiveCGDisplays",
                status,
            });
        }
        let link_ptr = NonNull::new(link_ptr).ok_or(BackendError::AppleApi {
            api: "CVDisplayLinkCreateWithActiveCGDisplays",
            status: -1,
        })?;
        let link = unsafe { CFRetained::from_raw(link_ptr) };
        #[allow(deprecated)]
        let status = unsafe {
            link.set_output_callback(
                Some(display_link_callback),
                Arc::as_ptr(&state).cast_mut().cast::<c_void>(),
            )
        };
        if status != 0 {
            return Err(BackendError::AppleApi {
                api: "CVDisplayLinkSetOutputCallback",
                status,
            });
        }
        #[allow(deprecated)]
        let status = link.start();
        if status != 0 {
            return Err(BackendError::AppleApi {
                api: "CVDisplayLinkStart",
                status,
            });
        }
        eprintln!("macOS AsyncFrameQueue display clock active");
        Ok(Self { link, state })
    }

    fn wait_next(&self, previous_generation: &mut u64) -> bool {
        let mut guard = self
            .state
            .wait_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        loop {
            if self.state.stopped.load(Ordering::Acquire) {
                return false;
            }
            let generation = self.state.generation.load(Ordering::Acquire);
            if generation != *previous_generation {
                *previous_generation = generation;
                return true;
            }
            guard = self
                .state
                .ready
                .wait(guard)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

impl Drop for DisplayClock {
    fn drop(&mut self) {
        self.state.stopped.store(true, Ordering::Release);
        self.state.ready.notify_all();
        #[allow(deprecated)]
        let _ = self.link.stop();
    }
}

unsafe extern "C-unwind" fn display_link_callback(
    _display_link: NonNull<CVDisplayLink>,
    _now: NonNull<CVTimeStamp>,
    _output_time: NonNull<CVTimeStamp>,
    _flags_in: CVOptionFlags,
    _flags_out: NonNull<CVOptionFlags>,
    user_info: *mut c_void,
) -> i32 {
    let Some(state) = NonNull::new(user_info.cast::<DisplayClockState>()) else {
        return 0;
    };
    let state = unsafe { state.as_ref() };
    state.generation.fetch_add(1, Ordering::AcqRel);
    state.ready.notify_one();
    0
}

impl Drop for PresenterHandle {
    fn drop(&mut self) {
        self.diagnostics_stop.store(true, Ordering::Release);
        self.queue.close_and_discard();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Some(worker) = self.diagnostics_worker.take() {
            let _ = worker.join();
        }
    }
}

struct PendingFrame {
    command_buffer: Retained<ProtocolObject<dyn MTLCommandBuffer>>,
    _luma_cv_texture: CFRetained<CVMetalTexture>,
    _chroma_cv_texture: CFRetained<CVMetalTexture>,
    _luma_texture: Retained<ProtocolObject<dyn MTLTexture>>,
    _chroma_texture: Retained<ProtocolObject<dyn MTLTexture>>,
}

unsafe impl Send for PendingFrame {}

struct MetalPresenter {
    layer: Retained<CAMetalLayer>,
    _device: Retained<ProtocolObject<dyn MTLDevice>>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    texture_cache: CFRetained<CVMetalTextureCache>,
    pending: VecDeque<PendingFrame>,
    telemetry: Arc<PresentationTelemetry>,
    zero_copy_confirmed: bool,
    pacing_confirmed: bool,
}

#[derive(Default)]
struct PresentationTiming {
    previous_time: Option<f64>,
    interval_samples: u64,
    interval_sum_seconds: f64,
    maximum_interval_seconds: f64,
    missed_refreshes: u64,
}

struct PresentationTelemetry {
    counters: Arc<Counters>,
    timing: Mutex<PresentationTiming>,
}

#[derive(Clone, Copy, Default)]
struct PresentationTimingSnapshot {
    average_interval_seconds: f64,
    maximum_interval_seconds: f64,
    missed_refreshes: u64,
}

impl PresentationTelemetry {
    fn record(&self, presented_time: f64, expected_period: f64) {
        if !presented_time.is_finite() || presented_time <= 0.0 {
            self.counters
                .video_frames_dropped
                .fetch_add(1, Ordering::Relaxed);
            self.counters
                .video_scanout_skipped
                .fetch_add(1, Ordering::Relaxed);
            return;
        }

        self.counters
            .video_presented
            .fetch_add(1, Ordering::Relaxed);
        let mut timing = self
            .timing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(previous_time) = timing.previous_time {
            let interval = presented_time - previous_time;
            if interval.is_finite() && interval > 0.0 {
                timing.interval_samples += 1;
                timing.interval_sum_seconds += interval;
                timing.maximum_interval_seconds = timing.maximum_interval_seconds.max(interval);
                if expected_period.is_finite()
                    && expected_period > 0.0
                    && interval > expected_period * 1.25
                {
                    timing.missed_refreshes +=
                        (interval / expected_period).round().max(1.0) as u64 - 1;
                }
            }
        }
        timing.previous_time = Some(presented_time);
    }

    fn take_timing(&self) -> PresentationTimingSnapshot {
        let mut timing = self
            .timing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let snapshot = PresentationTimingSnapshot {
            average_interval_seconds: if timing.interval_samples == 0 {
                0.0
            } else {
                timing.interval_sum_seconds / timing.interval_samples as f64
            },
            maximum_interval_seconds: timing.maximum_interval_seconds,
            missed_refreshes: timing.missed_refreshes,
        };
        timing.interval_samples = 0;
        timing.interval_sum_seconds = 0.0;
        timing.maximum_interval_seconds = 0.0;
        timing.missed_refreshes = 0;
        snapshot
    }
}

// Metal objects are thread-safe. AppKit attaches and detaches the layer on the main thread; after
// construction this worker only uses CAMetalLayer's thread-safe drawable API.
unsafe impl Send for MetalPresenter {}

impl MetalPresenter {
    fn new(layer: Retained<CAMetalLayer>, counters: Arc<Counters>) -> Result<Self, BackendError> {
        let device = MTLCreateSystemDefaultDevice()
            .ok_or_else(|| BackendError::Metal("Metal is unavailable on this Mac".into()))?;
        layer.setDevice(Some(&device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        layer.setFramebufferOnly(true);
        // Match the native NVIDIA renderer's normal three-drawable asynchronous path. The queue
        // remains bounded, but a transient command-buffer completion delay can no longer force a
        // synchronous wait every other 120 Hz frame.
        layer.setMaximumDrawableCount(3);
        layer.setPresentsWithTransaction(false);
        // Fixed-rate presentation is synchronized by CAMetalLayer. NVIDIA's fixed path submits a
        // plain presentDrawable and reserves explicit target/duration scheduling for adaptive
        // modes; doing the same avoids slipping past a ProMotion refresh boundary once per beat.
        layer.setDisplaySyncEnabled(true);
        layer.setAllowsNextDrawableTimeout(true);

        let command_queue = device
            .newCommandQueue()
            .ok_or_else(|| BackendError::Metal("failed to create Metal command queue".into()))?;
        let source = NSString::from_str(SHADER_SOURCE);
        let library = device
            .newLibraryWithSource_options_error(&source, None)
            .map_err(|error| BackendError::Metal(error.localizedDescription().to_string()))?;
        let vertex = library
            .newFunctionWithName(&NSString::from_str("video_vertex"))
            .ok_or_else(|| BackendError::Metal("video_vertex shader is missing".into()))?;
        let fragment = library
            .newFunctionWithName(&NSString::from_str("video_fragment"))
            .ok_or_else(|| BackendError::Metal("video_fragment shader is missing".into()))?;
        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setVertexFunction(Some(&vertex));
        descriptor.setFragmentFunction(Some(&fragment));
        let attachments = descriptor.colorAttachments();
        let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
        attachment.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        let pipeline = device
            .newRenderPipelineStateWithDescriptor_error(&descriptor)
            .map_err(|error| BackendError::Metal(error.localizedDescription().to_string()))?;

        let mut cache_ptr = ptr::null_mut();
        let status = unsafe {
            CVMetalTextureCache::create(None, None, &device, None, NonNull::from(&mut cache_ptr))
        };
        if status != 0 {
            return Err(BackendError::AppleApi {
                api: "CVMetalTextureCacheCreate",
                status,
            });
        }
        let cache_ptr = NonNull::new(cache_ptr).ok_or(BackendError::AppleApi {
            api: "CVMetalTextureCacheCreate",
            status: -1,
        })?;
        let texture_cache = unsafe { CFRetained::from_raw(cache_ptr) };
        Ok(Self {
            layer,
            _device: device,
            command_queue,
            pipeline,
            texture_cache,
            pending: VecDeque::with_capacity(3),
            telemetry: Arc::new(PresentationTelemetry {
                counters,
                timing: Mutex::new(PresentationTiming::default()),
            }),
            zero_copy_confirmed: false,
            pacing_confirmed: false,
        })
    }

    fn present(&mut self, frame: DecodedFrame) -> Result<(), BackendError> {
        if self.pending.len() >= 3 {
            self.wait_for_oldest()?;
        }
        if CVPixelBufferGetPixelFormatType(&frame.image)
            != kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
            || CVPixelBufferGetPlaneCount(&frame.image) != 2
        {
            return Err(BackendError::Metal(
                "VideoToolbox returned a non-NV12 pixel buffer".into(),
            ));
        }
        if CVPixelBufferGetIOSurface(Some(&frame.image)).is_none() {
            return Err(BackendError::Metal(
                "VideoToolbox returned a pixel buffer without IOSurface backing".into(),
            ));
        }
        if !self.zero_copy_confirmed {
            self.zero_copy_confirmed = true;
            eprintln!(
                "macOS zero-copy video active: VideoToolbox IOSurface -> CVMetalTexture -> CAMetalLayer"
            );
        }

        let width = CVPixelBufferGetWidthOfPlane(&frame.image, 0);
        let height = CVPixelBufferGetHeightOfPlane(&frame.image, 0);
        let chroma_width = CVPixelBufferGetWidthOfPlane(&frame.image, 1);
        let chroma_height = CVPixelBufferGetHeightOfPlane(&frame.image, 1);
        let luma_cv_texture =
            self.make_texture(&frame, MTLPixelFormat::R8Unorm, width, height, 0)?;
        let chroma_cv_texture = self.make_texture(
            &frame,
            MTLPixelFormat::RG8Unorm,
            chroma_width,
            chroma_height,
            1,
        )?;
        let luma_texture = CVMetalTextureGetTexture(&luma_cv_texture)
            .ok_or_else(|| BackendError::Metal("failed to get Metal luma texture".into()))?;
        let chroma_texture = CVMetalTextureGetTexture(&chroma_cv_texture)
            .ok_or_else(|| BackendError::Metal("failed to get Metal chroma texture".into()))?;
        let drawable = self
            .layer
            .nextDrawable()
            .ok_or_else(|| BackendError::Metal("CAMetalLayer has no drawable".into()))?;
        let drawable_texture = drawable.texture();

        let render_pass = MTLRenderPassDescriptor::renderPassDescriptor();
        let attachments = render_pass.colorAttachments();
        let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
        attachment.setTexture(Some(&drawable_texture));
        attachment.setLoadAction(MTLLoadAction::Clear);
        attachment.setStoreAction(MTLStoreAction::Store);
        attachment.setClearColor(MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        });

        let command_buffer = self
            .command_queue
            .commandBuffer()
            .ok_or_else(|| BackendError::Metal("failed to create Metal command buffer".into()))?;
        let encoder = command_buffer
            .renderCommandEncoderWithDescriptor(&render_pass)
            .ok_or_else(|| BackendError::Metal("failed to create Metal render encoder".into()))?;
        encoder.setRenderPipelineState(&self.pipeline);
        unsafe {
            encoder.setFragmentTexture_atIndex(Some(&luma_texture), 0);
            encoder.setFragmentTexture_atIndex(Some(&chroma_texture), 1);
        }
        let color_space = match frame.color_space {
            VideoColorSpace::Bt601 => 0u32,
            VideoColorSpace::Bt709 => 1u32,
        };
        unsafe {
            encoder.setFragmentBytes_length_atIndex(
                NonNull::from(&color_space).cast::<c_void>(),
                std::mem::size_of_val(&color_space),
                0,
            )
        };
        let destination_width = drawable_texture.width() as f64;
        let destination_height = drawable_texture.height() as f64;
        let viewport = aspect_fit_viewport(
            width as f64,
            height as f64,
            destination_width,
            destination_height,
        );
        encoder.setViewport(viewport);
        unsafe { encoder.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Triangle, 0, 3) };
        encoder.endEncoding();
        let drawable_ref: &ProtocolObject<dyn CAMetalDrawable> = &drawable;
        let drawable_as_base: &ProtocolObject<dyn MTLDrawable> = drawable_ref.as_ref();
        if !self.pacing_confirmed {
            self.pacing_confirmed = true;
            eprintln!(
                "macOS Metal fixed-rate presentation active: sourcePeriodMs={:.3} displaySync=true drawables=3",
                frame.minimum_frame_duration_seconds * 1_000.0,
            );
        }
        let telemetry = Arc::clone(&self.telemetry);
        let expected_period = frame.minimum_frame_duration_seconds;
        let presented_handler: PresentedHandler =
            RcBlock::new(move |drawable: NonNull<ProtocolObject<dyn MTLDrawable>>| {
                // SAFETY: Metal supplies a valid drawable pointer for the duration of this block.
                let presented_time = unsafe { drawable.as_ref() }.presentedTime();
                telemetry.record(presented_time, expected_period);
            });
        // SAFETY: `presented_handler` is a valid Objective-C block. Metal copies the escaping
        // handler and invokes it after the drawable reaches scanout.
        unsafe {
            drawable_as_base.addPresentedHandler(RcBlock::as_ptr(&presented_handler));
        }
        command_buffer.presentDrawable(drawable_as_base);
        command_buffer.commit();
        self.telemetry
            .counters
            .video_metal_submitted
            .fetch_add(1, Ordering::Relaxed);
        self.pending.push_back(PendingFrame {
            command_buffer,
            _luma_cv_texture: luma_cv_texture,
            _chroma_cv_texture: chroma_cv_texture,
            _luma_texture: luma_texture,
            _chroma_texture: chroma_texture,
        });
        Ok(())
    }

    fn make_texture(
        &self,
        frame: &DecodedFrame,
        pixel_format: MTLPixelFormat,
        width: usize,
        height: usize,
        plane: usize,
    ) -> Result<CFRetained<CVMetalTexture>, BackendError> {
        let mut texture_ptr = ptr::null_mut();
        let status = unsafe {
            CVMetalTextureCache::create_texture_from_image(
                None,
                &self.texture_cache,
                &frame.image,
                None,
                pixel_format,
                width,
                height,
                plane,
                NonNull::from(&mut texture_ptr),
            )
        };
        if status != 0 {
            return Err(BackendError::AppleApi {
                api: "CVMetalTextureCacheCreateTextureFromImage",
                status,
            });
        }
        let texture_ptr = NonNull::new(texture_ptr).ok_or(BackendError::AppleApi {
            api: "CVMetalTextureCacheCreateTextureFromImage",
            status: -1,
        })?;
        Ok(unsafe { CFRetained::from_raw(texture_ptr) })
    }

    fn wait_for_oldest(&mut self) -> Result<(), BackendError> {
        if let Some(frame) = self.pending.pop_front() {
            frame.command_buffer.waitUntilCompleted();
            if frame.command_buffer.status() == MTLCommandBufferStatus::Error {
                let message = frame.command_buffer.error().map_or_else(
                    || "Metal command buffer failed without an error description".to_owned(),
                    |error| error.localizedDescription().to_string(),
                );
                return Err(BackendError::Metal(message));
            }
        }
        Ok(())
    }

    fn finish(&mut self) {
        while !self.pending.is_empty() {
            let _ = self.wait_for_oldest();
        }
        self.texture_cache.flush(0);
    }
}

impl Drop for MetalPresenter {
    fn drop(&mut self) {
        self.finish();
    }
}

fn aspect_fit_viewport(
    source_width: f64,
    source_height: f64,
    destination_width: f64,
    destination_height: f64,
) -> MTLViewport {
    let source_aspect = source_width / source_height;
    let destination_aspect = destination_width / destination_height;
    let (width, height) = if source_aspect > destination_aspect {
        (destination_width, destination_width / source_aspect)
    } else {
        (destination_height * source_aspect, destination_height)
    };
    MTLViewport {
        originX: (destination_width - width) * 0.5,
        originY: (destination_height - height) * 0.5,
        width,
        height,
        znear: 0.0,
        zfar: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aspect_fit_letterboxes_without_distortion() {
        let viewport = aspect_fit_viewport(1920.0, 1080.0, 1024.0, 768.0);
        assert_eq!(viewport.width, 1024.0);
        assert_eq!(viewport.height, 576.0);
        assert_eq!(viewport.originY, 96.0);

        let portrait = aspect_fit_viewport(1080.0, 1920.0, 1024.0, 768.0);
        assert_eq!(portrait.height, 768.0);
        assert_eq!(portrait.width, 432.0);
        assert_eq!(portrait.originX, 296.0);
    }
}
