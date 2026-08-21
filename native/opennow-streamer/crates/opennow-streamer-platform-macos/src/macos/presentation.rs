use std::collections::VecDeque;
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread::{self, JoinHandle};

use objc2::rc::{Retained, autoreleasepool};
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::CFRetained;
use objc2_core_video::{
    CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBufferGetHeightOfPlane,
    CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount, CVPixelBufferGetWidthOfPlane,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLClearColor, MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue,
    MTLCreateSystemDefaultDevice, MTLDevice, MTLDrawable, MTLLibrary, MTLLoadAction,
    MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder, MTLRenderPassDescriptor,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLStoreAction, MTLTexture, MTLViewport,
};
use objc2_quartz_core::{CAMetalDrawable, CAMetalLayer};

use crate::format::VideoColorSpace;
use crate::queue::BoundedQueue;

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

pub(super) struct PresenterHandle {
    queue: Arc<BoundedQueue<DecodedFrame>>,
    worker: Option<JoinHandle<()>>,
}

impl PresenterHandle {
    pub(super) fn start(
        layer: Retained<CAMetalLayer>,
        visible: Arc<AtomicBool>,
        queue: Arc<BoundedQueue<DecodedFrame>>,
        counters: Arc<Counters>,
    ) -> Result<Self, BackendError> {
        let mut presenter = MetalPresenter::new(layer)?;
        let worker_queue = Arc::clone(&queue);
        let worker = thread::Builder::new()
            .name("opennow-metal-present".into())
            .spawn(move || {
                while let Some(frame) = worker_queue.pop_wait() {
                    if !visible.load(Ordering::Acquire) {
                        counters
                            .video_frames_dropped
                            .fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    let result = autoreleasepool(|_| presenter.present(frame));
                    match result {
                        Ok(()) => {
                            counters.video_presented.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(_) => {
                            counters
                                .video_present_errors
                                .fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
                presenter.finish();
            })
            .map_err(|_| BackendError::Thread("Metal presenter"))?;
        Ok(Self {
            queue,
            worker: Some(worker),
        })
    }

    pub(super) fn stop(mut self) {
        self.queue.close_and_discard();
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for PresenterHandle {
    fn drop(&mut self) {
        self.queue.close_and_discard();
        if let Some(worker) = self.worker.take() {
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

struct MetalPresenter {
    layer: Retained<CAMetalLayer>,
    command_queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    texture_cache: CFRetained<CVMetalTextureCache>,
    pending: VecDeque<PendingFrame>,
}

// Metal objects are thread-safe. AppKit attaches and detaches the layer on the main thread; after
// construction this worker only uses CAMetalLayer's thread-safe drawable API.
unsafe impl Send for MetalPresenter {}

impl MetalPresenter {
    fn new(layer: Retained<CAMetalLayer>) -> Result<Self, BackendError> {
        let device = MTLCreateSystemDefaultDevice()
            .ok_or_else(|| BackendError::Metal("Metal is unavailable on this Mac".into()))?;
        layer.setDevice(Some(&device));
        layer.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
        layer.setFramebufferOnly(true);
        layer.setMaximumDrawableCount(3);
        layer.setPresentsWithTransaction(false);
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
            command_queue,
            pipeline,
            texture_cache,
            pending: VecDeque::with_capacity(3),
        })
    }

    fn present(&mut self, frame: DecodedFrame) -> Result<(), BackendError> {
        if self.pending.len() >= 2 {
            self.wait_for_oldest();
        }
        if CVPixelBufferGetPixelFormatType(&frame.image)
            != kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
            || CVPixelBufferGetPlaneCount(&frame.image) != 2
        {
            return Err(BackendError::Metal(
                "VideoToolbox returned a non-NV12 pixel buffer".into(),
            ));
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
        encoder.setViewport(aspect_fit_viewport(
            width as f64,
            height as f64,
            destination_width,
            destination_height,
        ));
        unsafe { encoder.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Triangle, 0, 3) };
        encoder.endEncoding();
        let drawable_ref: &ProtocolObject<dyn CAMetalDrawable> = &drawable;
        let drawable_as_base: &ProtocolObject<dyn MTLDrawable> = drawable_ref.as_ref();
        command_buffer.presentDrawable(drawable_as_base);
        command_buffer.commit();
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

    fn wait_for_oldest(&mut self) {
        if let Some(frame) = self.pending.pop_front() {
            frame.command_buffer.waitUntilCompleted();
        }
    }

    fn finish(&mut self) {
        while !self.pending.is_empty() {
            self.wait_for_oldest();
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
