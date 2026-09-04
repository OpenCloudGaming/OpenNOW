use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::CFRetained;
use objc2_core_video::{
    CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBufferGetHeightOfPlane,
    CVPixelBufferGetIOSurface, CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount,
    CVPixelBufferGetWidthOfPlane, kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLCommandBuffer, MTLCommandEncoder, MTLDevice, MTLLibrary, MTLLoadAction, MTLPixelFormat,
    MTLPrimitiveType, MTLRenderCommandEncoder, MTLRenderPassDescriptor,
    MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLStorageMode, MTLStoreAction,
    MTLTexture, MTLTextureDescriptor, MTLTextureUsage, MTLViewport,
};

use crate::format::VideoColorSpace;

use super::mailbox::LatestMailbox;
use super::video::DecodedFrame;
use super::{BackendError, Counters};

const MAX_RETAINED_FRAME_SLOTS: usize = 8;

fn validate_frame_slot(frame_slot: u32) -> Result<(), BackendError> {
    if frame_slot < MAX_RETAINED_FRAME_SLOTS as u32 {
        Ok(())
    } else {
        Err(BackendError::Metal(format!(
            "Metal frame slot {frame_slot} exceeds the eight-slot limit"
        )))
    }
}

const SHADER_SOURCE: &str = r#"
#include <metal_stdlib>
using namespace metal;

struct VertexOut {
    float4 position [[position]];
    float2 texcoord;
};

struct ConversionParameters {
    uint color_space;
    uint sample_format;
    uint full_range;
};

vertex VertexOut embedded_video_vertex(uint vertex_id [[vertex_id]]) {
    const float2 positions[3] = { float2(-1.0, -1.0), float2(3.0, -1.0), float2(-1.0, 3.0) };
    const float2 texcoords[3] = { float2(0.0, 1.0), float2(2.0, 1.0), float2(0.0, -1.0) };
    VertexOut out;
    out.position = float4(positions[vertex_id], 0.0, 1.0);
    out.texcoord = texcoords[vertex_id];
    return out;
}

fragment float4 embedded_video_fragment(
    VertexOut in [[stage_in]],
    texture2d<float> luma [[texture(0)]],
    texture2d<float> chroma [[texture(1)]],
    constant ConversionParameters &parameters [[buffer(0)]]) {
    constexpr sampler linear_sampler(coord::normalized, address::clamp_to_edge, filter::linear);
    float y = luma.sample(linear_sampler, in.texcoord).r;
    float2 cbcr = chroma.sample(linear_sampler, in.texcoord).rg;
    if (parameters.full_range == 0) {
        if (parameters.sample_format == 0) {
            y = (y - (16.0 / 255.0)) * (255.0 / 219.0);
            cbcr -= float2(0.5);
        } else {
            y = (y - (64.0 / 1023.0)) * (1023.0 / 876.0);
            cbcr -= float2(512.0 / 1023.0);
        }
    } else {
        cbcr -= parameters.sample_format == 0
            ? float2(128.0 / 255.0)
            : float2(512.0 / 1023.0);
    }
    float3 rgb;
    if (parameters.color_space == 0) {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BiPlanarFormat {
    Nv12Video,
    Nv12Full,
    P010Video,
    P010Full,
}

impl BiPlanarFormat {
    fn from_pixel_format(pixel_format: u32) -> Option<Self> {
        if pixel_format == kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange {
            Some(Self::Nv12Video)
        } else if pixel_format == kCVPixelFormatType_420YpCbCr8BiPlanarFullRange {
            Some(Self::Nv12Full)
        } else if pixel_format == kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange {
            Some(Self::P010Video)
        } else if pixel_format == kCVPixelFormatType_420YpCbCr10BiPlanarFullRange {
            Some(Self::P010Full)
        } else {
            None
        }
    }

    const fn plane_formats(self) -> (MTLPixelFormat, MTLPixelFormat) {
        match self {
            Self::Nv12Video | Self::Nv12Full => (MTLPixelFormat::R8Unorm, MTLPixelFormat::RG8Unorm),
            Self::P010Video | Self::P010Full => {
                (MTLPixelFormat::R16Unorm, MTLPixelFormat::RG16Unorm)
            }
        }
    }

    const fn parameters(self, color_space: VideoColorSpace) -> ConversionParameters {
        ConversionParameters {
            color_space: match color_space {
                VideoColorSpace::Bt601 => 0,
                VideoColorSpace::Bt709 => 1,
            },
            sample_format: match self {
                Self::Nv12Video | Self::Nv12Full => 0,
                Self::P010Video | Self::P010Full => 1,
            },
            full_range: match self {
                Self::Nv12Video | Self::P010Video => 0,
                Self::Nv12Full | Self::P010Full => 1,
            },
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ConversionParameters {
    color_space: u32,
    sample_format: u32,
    full_range: u32,
}

#[derive(Clone, Copy, Debug)]
pub struct AdoptedMetalContext {
    pub device: *mut c_void,
    pub command_buffer: *mut c_void,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetalRecordedFrame {
    pub texture: *mut c_void,
    pub width: u32,
    pub height: u32,
    pub frame_slot: u32,
    pub generation: u64,
    pub presentation_time_ns: u64,
}

pub struct MetalFrame {
    frame: DecodedFrame,
    state: Arc<Mutex<Option<MetalState>>>,
    counters: Arc<Counters>,
    sequence: u64,
}

impl MetalFrame {
    pub fn width(&self) -> u32 {
        u32::try_from(CVPixelBufferGetWidthOfPlane(&self.frame.image, 0)).unwrap_or(0)
    }

    pub fn height(&self) -> u32 {
        u32::try_from(CVPixelBufferGetHeightOfPlane(&self.frame.image, 0)).unwrap_or(0)
    }

    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn presentation_time_ns(&self) -> u64 {
        u64::try_from(self.frame.timestamp_100ns.max(0))
            .unwrap_or(0)
            .saturating_mul(100)
    }

    /// Records IOSurface plane sampling and YUV-to-RGBA conversion into a retained slot texture.
    ///
    /// The function creates no command queue and does not commit or wait for the command buffer.
    /// It must run before Qt begins the render pass that samples the returned texture.
    ///
    /// # Safety
    ///
    /// Both pointers must identify live Metal objects from the same Qt QRhi frame, and the command
    /// buffer must be open for encoding. `frame_slot` may be reused only when Qt has retired its
    /// previous GPU work for that slot.
    pub unsafe fn record(
        &self,
        adopted: AdoptedMetalContext,
        frame_slot: u32,
    ) -> Result<MetalRecordedFrame, BackendError> {
        if adopted.device.is_null() || adopted.command_buffer.is_null() {
            return Err(BackendError::Metal(
                "Qt supplied a null Metal device or command buffer".into(),
            ));
        }
        validate_frame_slot(frame_slot)?;
        let device_ptr = NonNull::new(adopted.device)
            .expect("validated device")
            .cast::<ProtocolObject<dyn MTLDevice>>();
        let command_buffer_ptr = NonNull::new(adopted.command_buffer)
            .expect("validated command buffer")
            .cast::<ProtocolObject<dyn MTLCommandBuffer>>();
        let device = unsafe { device_ptr.as_ref() };
        let command_buffer = unsafe { command_buffer_ptr.as_ref() };

        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let device_identity = device as *const _ as *const () as usize;
        if state
            .as_ref()
            .is_none_or(|state| state.device_identity != device_identity)
        {
            *state = Some(MetalState::new(device, device_identity)?);
        }
        let recorded = state.as_mut().expect("initialized Metal state").record(
            self.frame.clone(),
            command_buffer,
            frame_slot,
        )?;
        self.counters
            .video_metal_submitted
            .fetch_add(1, Ordering::Relaxed);
        Ok(MetalRecordedFrame {
            presentation_time_ns: self.presentation_time_ns(),
            ..recorded
        })
    }
}

#[derive(Clone)]
pub struct EmbeddedFrameProducer {
    mailbox: Arc<LatestMailbox<DecodedFrame>>,
    state: Arc<Mutex<Option<MetalState>>>,
    sequence: Arc<AtomicU64>,
    counters: Arc<Counters>,
}

impl EmbeddedFrameProducer {
    pub(super) fn new(mailbox: Arc<LatestMailbox<DecodedFrame>>, counters: Arc<Counters>) -> Self {
        Self {
            mailbox,
            state: Arc::new(Mutex::new(None)),
            sequence: Arc::new(AtomicU64::new(0)),
            counters,
        }
    }

    pub fn acquire_latest(&self) -> Option<MetalFrame> {
        let frame = self.mailbox.take()?;
        let sequence = self.sequence.fetch_add(1, Ordering::AcqRel) + 1;
        Some(MetalFrame {
            frame,
            state: Arc::clone(&self.state),
            counters: Arc::clone(&self.counters),
            sequence,
        })
    }

    pub fn clear(&self) -> bool {
        self.mailbox.clear()
    }

    /// Releases device-bound caches after Qt has invalidated and drained its scene graph.
    pub fn release_graphics_resources(&self) {
        *self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }

    pub(super) fn mailbox(&self) -> &Arc<LatestMailbox<DecodedFrame>> {
        &self.mailbox
    }

    pub(super) fn counters(&self) -> &Arc<Counters> {
        &self.counters
    }
}

struct SlotResources {
    output: Retained<ProtocolObject<dyn MTLTexture>>,
    _frame: DecodedFrame,
    _luma_cv_texture: CFRetained<CVMetalTexture>,
    _chroma_cv_texture: CFRetained<CVMetalTexture>,
    _luma_texture: Retained<ProtocolObject<dyn MTLTexture>>,
    _chroma_texture: Retained<ProtocolObject<dyn MTLTexture>>,
}

struct MetalState {
    device_identity: usize,
    _device: Retained<ProtocolObject<dyn MTLDevice>>,
    pipeline: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
    texture_cache: CFRetained<CVMetalTextureCache>,
    slots: HashMap<u32, SlotResources>,
    generation: u64,
}

unsafe impl Send for MetalState {}

impl MetalState {
    fn new(
        device: &ProtocolObject<dyn MTLDevice>,
        device_identity: usize,
    ) -> Result<Self, BackendError> {
        let device = device.retain();
        let source = NSString::from_str(SHADER_SOURCE);
        let library = device
            .newLibraryWithSource_options_error(&source, None)
            .map_err(|error| BackendError::Metal(error.localizedDescription().to_string()))?;
        let vertex = library
            .newFunctionWithName(&NSString::from_str("embedded_video_vertex"))
            .ok_or_else(|| BackendError::Metal("embedded_video_vertex shader is missing".into()))?;
        let fragment = library
            .newFunctionWithName(&NSString::from_str("embedded_video_fragment"))
            .ok_or_else(|| {
                BackendError::Metal("embedded_video_fragment shader is missing".into())
            })?;
        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setVertexFunction(Some(&vertex));
        descriptor.setFragmentFunction(Some(&fragment));
        let attachments = descriptor.colorAttachments();
        let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
        attachment.setPixelFormat(MTLPixelFormat::RGBA8Unorm);
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
        Ok(Self {
            device_identity,
            _device: device,
            pipeline,
            texture_cache: unsafe { CFRetained::from_raw(cache_ptr) },
            slots: HashMap::with_capacity(3),
            generation: 0,
        })
    }

    fn record(
        &mut self,
        frame: DecodedFrame,
        command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
        frame_slot: u32,
    ) -> Result<MetalRecordedFrame, BackendError> {
        if CVPixelBufferGetPlaneCount(&frame.image) != 2 {
            return Err(BackendError::Metal(
                "VideoToolbox returned a non-bi-planar pixel buffer".into(),
            ));
        }
        if CVPixelBufferGetIOSurface(Some(&frame.image)).is_none() {
            return Err(BackendError::Metal(
                "VideoToolbox returned a pixel buffer without IOSurface backing".into(),
            ));
        }
        let format =
            BiPlanarFormat::from_pixel_format(CVPixelBufferGetPixelFormatType(&frame.image))
                .ok_or_else(|| {
                    BackendError::Metal("VideoToolbox returned neither NV12 nor P010".into())
                })?;
        let width = CVPixelBufferGetWidthOfPlane(&frame.image, 0);
        let height = CVPixelBufferGetHeightOfPlane(&frame.image, 0);
        let chroma_width = CVPixelBufferGetWidthOfPlane(&frame.image, 1);
        let chroma_height = CVPixelBufferGetHeightOfPlane(&frame.image, 1);
        let (luma_format, chroma_format) = format.plane_formats();
        let luma_cv_texture = self.make_plane_texture(&frame, luma_format, width, height, 0)?;
        let chroma_cv_texture =
            self.make_plane_texture(&frame, chroma_format, chroma_width, chroma_height, 1)?;
        let luma_texture = CVMetalTextureGetTexture(&luma_cv_texture)
            .ok_or_else(|| BackendError::Metal("failed to get Metal luma texture".into()))?;
        let chroma_texture = CVMetalTextureGetTexture(&chroma_cv_texture)
            .ok_or_else(|| BackendError::Metal("failed to get Metal chroma texture".into()))?;

        let output = self
            .slots
            .remove(&frame_slot)
            .map(|resources| resources.output)
            .filter(|texture| texture.width() == width && texture.height() == height)
            .map_or_else(
                || {
                    let descriptor = unsafe {
                        MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                            MTLPixelFormat::RGBA8Unorm,
                            width,
                            height,
                            false,
                        )
                    };
                    descriptor.setStorageMode(MTLStorageMode::Private);
                    descriptor.setUsage(
                        MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead,
                    );
                    self._device
                        .newTextureWithDescriptor(&descriptor)
                        .ok_or_else(|| {
                            BackendError::Metal("failed to create embedded RGBA texture".into())
                        })
                },
                Ok,
            )?;

        let render_pass = MTLRenderPassDescriptor::renderPassDescriptor();
        let attachments = render_pass.colorAttachments();
        let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
        attachment.setTexture(Some(&output));
        attachment.setLoadAction(MTLLoadAction::DontCare);
        attachment.setStoreAction(MTLStoreAction::Store);
        let encoder = command_buffer
            .renderCommandEncoderWithDescriptor(&render_pass)
            .ok_or_else(|| BackendError::Metal("failed to create embedded Metal encoder".into()))?;
        encoder.setRenderPipelineState(&self.pipeline);
        unsafe {
            encoder.setFragmentTexture_atIndex(Some(&luma_texture), 0);
            encoder.setFragmentTexture_atIndex(Some(&chroma_texture), 1);
        }
        let parameters = format.parameters(frame.color_space);
        unsafe {
            encoder.setFragmentBytes_length_atIndex(
                NonNull::from(&parameters).cast::<c_void>(),
                std::mem::size_of_val(&parameters),
                0,
            );
        }
        encoder.setViewport(MTLViewport {
            originX: 0.0,
            originY: 0.0,
            width: width as f64,
            height: height as f64,
            znear: 0.0,
            zfar: 1.0,
        });
        unsafe { encoder.drawPrimitives_vertexStart_vertexCount(MTLPrimitiveType::Triangle, 0, 3) };
        encoder.endEncoding();

        let texture = Retained::as_ptr(&output).cast_mut().cast::<c_void>();
        self.slots.insert(
            frame_slot,
            SlotResources {
                output,
                _frame: frame,
                _luma_cv_texture: luma_cv_texture,
                _chroma_cv_texture: chroma_cv_texture,
                _luma_texture: luma_texture,
                _chroma_texture: chroma_texture,
            },
        );
        self.generation = self.generation.wrapping_add(1);
        Ok(MetalRecordedFrame {
            texture,
            width: u32::try_from(width).unwrap_or(u32::MAX),
            height: u32::try_from(height).unwrap_or(u32::MAX),
            frame_slot,
            generation: self.generation,
            presentation_time_ns: 0,
        })
    }

    fn make_plane_texture(
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
}

impl Drop for MetalState {
    fn drop(&mut self) {
        self.slots.clear();
        self.texture_cache.flush(0);
    }
}

#[cfg(test)]
mod tests {
    use super::{BiPlanarFormat, MAX_RETAINED_FRAME_SLOTS, validate_frame_slot};
    use objc2_core_video::{
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
        kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,
        kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
    };

    #[test]
    fn accepts_nv12_and_p010_ranges_only() {
        assert_eq!(
            BiPlanarFormat::from_pixel_format(kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange),
            Some(BiPlanarFormat::Nv12Video)
        );
        assert_eq!(
            BiPlanarFormat::from_pixel_format(kCVPixelFormatType_420YpCbCr8BiPlanarFullRange),
            Some(BiPlanarFormat::Nv12Full)
        );
        assert_eq!(
            BiPlanarFormat::from_pixel_format(kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange),
            Some(BiPlanarFormat::P010Video)
        );
        assert_eq!(
            BiPlanarFormat::from_pixel_format(kCVPixelFormatType_420YpCbCr10BiPlanarFullRange),
            Some(BiPlanarFormat::P010Full)
        );
        assert_eq!(BiPlanarFormat::from_pixel_format(0), None);
    }

    #[test]
    fn retained_slot_bound_matches_public_embedded_limit() {
        assert_eq!(MAX_RETAINED_FRAME_SLOTS, 8);
        assert!(validate_frame_slot(0).is_ok());
        assert!(validate_frame_slot(7).is_ok());
        assert!(validate_frame_slot(8).is_err());
    }
}
