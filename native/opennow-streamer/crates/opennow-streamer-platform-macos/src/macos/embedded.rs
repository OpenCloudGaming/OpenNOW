use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use block2::RcBlock;
use objc2::Message;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::{CFRetained, CFString};
use objc2_core_video::{
    CVMetalTexture, CVMetalTextureCache, CVMetalTextureGetTexture, CVPixelBufferGetHeightOfPlane,
    CVPixelBufferGetIOSurface, CVPixelBufferGetPixelFormatType, CVPixelBufferGetPlaneCount,
    CVPixelBufferGetWidthOfPlane, kCVImageBufferYCbCrMatrix_ITU_R_601_4,
    kCVImageBufferYCbCrMatrix_ITU_R_709_2, kCVImageBufferYCbCrMatrixKey,
    kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,
    kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
};
use objc2_foundation::NSString;
use objc2_metal::{
    MTLCommandBuffer, MTLCommandBufferStatus, MTLCommandEncoder, MTLDevice, MTLLibrary,
    MTLLoadAction, MTLPixelFormat, MTLPrimitiveType, MTLRenderCommandEncoder,
    MTLRenderPassDescriptor, MTLRenderPipelineDescriptor, MTLRenderPipelineState, MTLStorageMode,
    MTLStoreAction, MTLTexture, MTLTextureDescriptor, MTLTextureUsage, MTLViewport,
};

use crate::color::ConversionParameters;
use crate::failure::FailureReporter;
use crate::format::{MetalFrameFormat, VideoBitDepth, VideoColorSpace};
use crate::spatial::SpatialConfig;

use super::mailbox::LatestMailbox;
use super::metalfx::{self, SpatialResources};
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
    float sample_scale;
    float luma_offset;
    float luma_scale;
    float chroma_offset;
    float chroma_scale;
    float red_cr;
    float green_cb;
    float green_cr;
    float blue_cb;
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
    float y = luma.sample(linear_sampler, in.texcoord).r * parameters.sample_scale;
    float2 cbcr = chroma.sample(linear_sampler, in.texcoord).rg * parameters.sample_scale;
    y = (y - parameters.luma_offset) * parameters.luma_scale;
    cbcr = (cbcr - parameters.chroma_offset) * parameters.chroma_scale;
    float3 rgb = float3(
        y + parameters.red_cr * cbcr.y,
        y + parameters.green_cb * cbcr.x + parameters.green_cr * cbcr.y,
        y + parameters.blue_cb * cbcr.x);
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

    const fn output_format(self) -> MetalFrameFormat {
        match self {
            Self::Nv12Video | Self::Nv12Full => MetalFrameFormat::Rgba8Unorm,
            Self::P010Video | Self::P010Full => MetalFrameFormat::Rgb10a2Unorm,
        }
    }

    fn parameters(self, color_space: VideoColorSpace) -> ConversionParameters {
        ConversionParameters::new(
            match self {
                Self::Nv12Video | Self::Nv12Full => VideoBitDepth::Eight,
                Self::P010Video | Self::P010Full => VideoBitDepth::Ten,
            },
            matches!(self, Self::Nv12Full | Self::P010Full),
            color_space,
        )
    }
}

impl MetalFrameFormat {
    pub(super) const fn pixel_format(self) -> MTLPixelFormat {
        match self {
            Self::Rgba8Unorm => MTLPixelFormat::RGBA8Unorm,
            Self::Rgb10a2Unorm => MTLPixelFormat::RGB10A2Unorm,
        }
    }

    const fn pipeline_index(self) -> usize {
        match self {
            Self::Rgba8Unorm => 0,
            Self::Rgb10a2Unorm => 1,
        }
    }
}

fn frame_color_space(frame: &DecodedFrame) -> Result<VideoColorSpace, BackendError> {
    let Some(attachment) = (unsafe {
        frame
            .image
            .attachment(kCVImageBufferYCbCrMatrixKey, ptr::null_mut())
    }) else {
        return Ok(frame.color_space);
    };
    let matrix = attachment.downcast_ref::<CFString>().ok_or_else(|| {
        BackendError::Metal("VideoToolbox returned an invalid YCbCr matrix attachment".into())
    })?;
    if matrix == unsafe { kCVImageBufferYCbCrMatrix_ITU_R_601_4 } {
        Ok(VideoColorSpace::Bt601)
    } else if matrix == unsafe { kCVImageBufferYCbCrMatrix_ITU_R_709_2 } {
        Ok(VideoColorSpace::Bt709)
    } else {
        Err(BackendError::Metal(format!(
            "unsupported VideoToolbox YCbCr matrix: {matrix}"
        )))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct AdoptedMetalContext {
    pub device: *mut c_void,
    pub command_buffer: *mut c_void,
    pub upscale_width: u32,
    pub upscale_height: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MetalRecordedFrame {
    pub texture: *mut c_void,
    pub format: MetalFrameFormat,
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
    failures: Arc<FailureReporter>,
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
            (adopted.upscale_width, adopted.upscale_height),
            Arc::clone(&self.counters),
            Arc::clone(&self.failures),
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
    failures: Arc<FailureReporter>,
}

impl EmbeddedFrameProducer {
    pub(super) fn new(
        mailbox: Arc<LatestMailbox<DecodedFrame>>,
        counters: Arc<Counters>,
        failures: Arc<FailureReporter>,
    ) -> Self {
        Self {
            mailbox,
            state: Arc::new(Mutex::new(None)),
            sequence: Arc::new(AtomicU64::new(0)),
            counters,
            failures,
        }
    }

    pub fn acquire_latest(&self) -> Option<MetalFrame> {
        let frame = self.mailbox.take()?;
        let sequence = self.sequence.fetch_add(1, Ordering::AcqRel) + 1;
        Some(MetalFrame {
            frame,
            state: Arc::clone(&self.state),
            counters: Arc::clone(&self.counters),
            failures: Arc::clone(&self.failures),
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

struct InFlightResources {
    _output: Retained<ProtocolObject<dyn MTLTexture>>,
    _spatial: Option<SpatialResources>,
    _frame: DecodedFrame,
    _luma_cv_texture: CFRetained<CVMetalTexture>,
    _chroma_cv_texture: CFRetained<CVMetalTexture>,
    _luma_texture: Retained<ProtocolObject<dyn MTLTexture>>,
    _chroma_texture: Retained<ProtocolObject<dyn MTLTexture>>,
}

struct MetalState {
    device_identity: usize,
    _device: Retained<ProtocolObject<dyn MTLDevice>>,
    library: Retained<ProtocolObject<dyn MTLLibrary>>,
    pipelines: [Option<Retained<ProtocolObject<dyn MTLRenderPipelineState>>>; 2],
    texture_cache: CFRetained<CVMetalTextureCache>,
    slots: HashMap<u32, Retained<ProtocolObject<dyn MTLTexture>>>,
    spatial_slots: HashMap<u32, (SpatialConfig, Option<SpatialResources>)>,
    spatial_supported: Option<bool>,
    spatial_initialization_reported: bool,
    spatial_failed: Arc<AtomicBool>,
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
            library,
            pipelines: [None, None],
            texture_cache: unsafe { CFRetained::from_raw(cache_ptr) },
            slots: HashMap::with_capacity(3),
            spatial_slots: HashMap::with_capacity(3),
            spatial_supported: None,
            spatial_initialization_reported: false,
            spatial_failed: Arc::new(AtomicBool::new(false)),
            generation: 0,
        })
    }

    fn record(
        &mut self,
        frame: DecodedFrame,
        command_buffer: &ProtocolObject<dyn MTLCommandBuffer>,
        frame_slot: u32,
        upscale_size: (u32, u32),
        counters: Arc<Counters>,
        failures: Arc<FailureReporter>,
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
        let output_format = format.output_format();
        let parameters = format.parameters(frame_color_space(&frame)?);
        self.ensure_pipeline(output_format)?;
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

        let spatial = self.spatial_resources(
            frame_slot,
            SpatialConfig::new(width, height, upscale_size.0, upscale_size.1, output_format),
        );
        let output = if let Some(spatial) = &spatial {
            spatial.input.clone()
        } else if let Some(texture) = self.slots.remove(&frame_slot).filter(|texture| {
            texture.width() == width
                && texture.height() == height
                && texture.pixelFormat() == output_format.pixel_format()
        }) {
            texture
        } else {
            let descriptor = unsafe {
                MTLTextureDescriptor::texture2DDescriptorWithPixelFormat_width_height_mipmapped(
                    output_format.pixel_format(),
                    width,
                    height,
                    false,
                )
            };
            descriptor.setStorageMode(MTLStorageMode::Private);
            descriptor.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
            self._device
                .newTextureWithDescriptor(&descriptor)
                .ok_or_else(|| {
                    BackendError::Metal(format!(
                        "failed to create embedded {output_format:?} texture"
                    ))
                })?
        };

        let render_pass = MTLRenderPassDescriptor::renderPassDescriptor();
        let attachments = render_pass.colorAttachments();
        let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
        attachment.setTexture(Some(&output));
        attachment.setLoadAction(MTLLoadAction::DontCare);
        attachment.setStoreAction(MTLStoreAction::Store);
        let encoder = command_buffer
            .renderCommandEncoderWithDescriptor(&render_pass)
            .ok_or_else(|| BackendError::Metal("failed to create embedded Metal encoder".into()))?;
        encoder.setRenderPipelineState(
            self.pipelines[output_format.pipeline_index()]
                .as_ref()
                .expect("initialized pipeline"),
        );
        unsafe {
            encoder.setFragmentTexture_atIndex(Some(&luma_texture), 0);
            encoder.setFragmentTexture_atIndex(Some(&chroma_texture), 1);
        }
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

        let scaling_attempted = spatial.is_some();
        let scaled = spatial
            .as_ref()
            .is_some_and(|spatial| spatial.encode(command_buffer));
        if scaling_attempted && !scaled {
            self.spatial_failed.store(true, Ordering::Release);
            eprintln!("MetalFX encoding failed; using normal conversion until graphics recreation");
        }
        let sampled_output = if scaled {
            &spatial.as_ref().expect("encoded scaler").output
        } else {
            &output
        };
        let texture = Retained::as_ptr(sampled_output).cast_mut().cast::<c_void>();
        let output_width = sampled_output.width();
        let output_height = sampled_output.height();
        self.slots.insert(frame_slot, output.clone());
        let resources = InFlightResources {
            _output: output,
            _spatial: spatial,
            _frame: frame,
            _luma_cv_texture: luma_cv_texture,
            _chroma_cv_texture: chroma_cv_texture,
            _luma_texture: luma_texture,
            _chroma_texture: chroma_texture,
        };
        let spatial_failed = Arc::clone(&self.spatial_failed);
        let completed = RcBlock::new(
            move |command: NonNull<ProtocolObject<dyn MTLCommandBuffer>>| {
                let _retained_until_completion = &resources;
                let command = unsafe { command.as_ref() };
                if command.status() == MTLCommandBufferStatus::Completed {
                    counters
                        .video_metal_completed
                        .fetch_add(1, Ordering::Relaxed);
                    failures.metal_succeeded();
                } else {
                    counters
                        .video_present_errors
                        .fetch_add(1, Ordering::Relaxed);
                    let message = command.error().map_or_else(
                        || "Qt Metal command buffer failed without an error description".to_owned(),
                        |error| error.localizedDescription().to_string(),
                    );
                    if scaling_attempted {
                        if !spatial_failed.swap(true, Ordering::AcqRel) {
                            eprintln!(
                                "Qt command buffer failed with MetalFX enabled; disabling optional scaling: {message}"
                            );
                        }
                    } else {
                        failures.metal_failed(message);
                    }
                }
            },
        );
        unsafe { command_buffer.addCompletedHandler(RcBlock::as_ptr(&completed)) };
        self.generation = self.generation.wrapping_add(1);
        Ok(MetalRecordedFrame {
            texture,
            format: output_format,
            width: u32::try_from(output_width).unwrap_or(u32::MAX),
            height: u32::try_from(output_height).unwrap_or(u32::MAX),
            frame_slot,
            generation: self.generation,
            presentation_time_ns: 0,
        })
    }

    fn spatial_resources(
        &mut self,
        frame_slot: u32,
        config: Option<SpatialConfig>,
    ) -> Option<SpatialResources> {
        let Some(config) = config else {
            self.spatial_slots.remove(&frame_slot);
            return None;
        };
        if self.spatial_failed.load(Ordering::Acquire) {
            self.spatial_slots.remove(&frame_slot);
            return None;
        }
        if !*self
            .spatial_supported
            .get_or_insert_with(|| {
                let supported = metalfx::supports_device(&self._device);
                if !supported {
                    eprintln!("MetalFX spatial scaling unavailable for this OS/device; using normal conversion");
                }
                supported
            })
        {
            return None;
        }
        let entry = self
            .spatial_slots
            .entry(frame_slot)
            .or_insert_with(|| (config, SpatialResources::new(&self._device, config)));
        if entry.0 != config {
            *entry = (config, SpatialResources::new(&self._device, config));
        }
        if entry.1.is_none() && !self.spatial_initialization_reported {
            self.spatial_initialization_reported = true;
            eprintln!(
                "MetalFX spatial resources unavailable for {config:?}; using normal conversion"
            );
        }
        entry.1.clone()
    }

    fn ensure_pipeline(&mut self, format: MetalFrameFormat) -> Result<(), BackendError> {
        let pipeline = &mut self.pipelines[format.pipeline_index()];
        if pipeline.is_some() {
            return Ok(());
        }
        let vertex = self
            .library
            .newFunctionWithName(&NSString::from_str("embedded_video_vertex"))
            .ok_or_else(|| BackendError::Metal("embedded_video_vertex shader is missing".into()))?;
        let fragment = self
            .library
            .newFunctionWithName(&NSString::from_str("embedded_video_fragment"))
            .ok_or_else(|| {
                BackendError::Metal("embedded_video_fragment shader is missing".into())
            })?;
        let descriptor = MTLRenderPipelineDescriptor::new();
        descriptor.setVertexFunction(Some(&vertex));
        descriptor.setFragmentFunction(Some(&fragment));
        let attachments = descriptor.colorAttachments();
        let attachment = unsafe { attachments.objectAtIndexedSubscript(0) };
        attachment.setPixelFormat(format.pixel_format());
        *pipeline = Some(
            self._device
                .newRenderPipelineStateWithDescriptor_error(&descriptor)
                .map_err(|error| {
                    BackendError::Metal(format!(
                        "failed to create {format:?} pipeline: {}",
                        error.localizedDescription()
                    ))
                })?,
        );
        Ok(())
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
    use crate::format::MetalFrameFormat;
    use objc2_core_video::{
        kCVPixelFormatType_420YpCbCr8BiPlanarFullRange,
        kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange,
        kCVPixelFormatType_420YpCbCr10BiPlanarFullRange,
        kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange,
    };
    use objc2_metal::MTLPixelFormat;

    #[test]
    #[ignore = "requires a Mac with MetalFX spatial scaling"]
    fn spatial_slots_reuse_resources_and_retire_failed_or_changed_configurations() {
        use super::*;
        use objc2_metal::MTLCreateSystemDefaultDevice;

        let device = MTLCreateSystemDefaultDevice().expect("Metal device");
        assert!(metalfx::supports_device(&device));
        let mut state = MetalState::new(&device, 1).unwrap();
        let config = SpatialConfig::new(64, 64, 128, 128, MetalFrameFormat::Rgba8Unorm).unwrap();
        let first = state.spatial_resources(0, Some(config)).unwrap();
        let reused = state.spatial_resources(0, Some(config)).unwrap();
        assert_eq!(
            Retained::as_ptr(&first.output),
            Retained::as_ptr(&reused.output)
        );
        assert_eq!(
            Retained::as_ptr(&first.input),
            Retained::as_ptr(&reused.input)
        );
        for slot in 1..MAX_RETAINED_FRAME_SLOTS as u32 {
            let next = state.spatial_resources(slot, Some(config)).unwrap();
            assert_ne!(
                Retained::as_ptr(&first.output),
                Retained::as_ptr(&next.output)
            );
        }
        assert_eq!(state.spatial_slots.len(), MAX_RETAINED_FRAME_SLOTS);
        let resized = SpatialConfig::new(64, 64, 256, 256, config.format).unwrap();
        let replacement = state.spatial_resources(0, Some(resized)).unwrap();
        assert_ne!(
            Retained::as_ptr(&first.output),
            Retained::as_ptr(&replacement.output)
        );
        assert!(state.spatial_resources(0, None).is_none());
        assert!(!state.spatial_slots.contains_key(&0));
        state.spatial_slots.insert(0, (config, None));
        assert!(state.spatial_resources(0, Some(config)).is_none());
        state.spatial_failed.store(true, Ordering::Release);
        assert!(state.spatial_resources(1, Some(config)).is_none());
        assert!(!state.spatial_slots.contains_key(&1));
        drop(state);
        assert_eq!(first.output.width(), 128);
        assert_eq!(replacement.output.width(), 256);
    }

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

    #[test]
    fn ten_bit_planes_always_select_rgb10a2_texture_and_pipeline() {
        for format in [BiPlanarFormat::Nv12Video, BiPlanarFormat::Nv12Full] {
            assert_eq!(format.output_format(), MetalFrameFormat::Rgba8Unorm);
            assert_eq!(
                format.output_format().pixel_format(),
                MTLPixelFormat::RGBA8Unorm
            );
            assert_eq!(format.output_format().pipeline_index(), 0);
            assert_eq!(
                format.plane_formats(),
                (MTLPixelFormat::R8Unorm, MTLPixelFormat::RG8Unorm)
            );
        }
        for format in [BiPlanarFormat::P010Video, BiPlanarFormat::P010Full] {
            assert_eq!(format.output_format(), MetalFrameFormat::Rgb10a2Unorm);
            assert_eq!(
                format.output_format().pixel_format(),
                MTLPixelFormat::RGB10A2Unorm
            );
            assert_eq!(format.output_format().pipeline_index(), 1);
            assert_eq!(
                format.plane_formats(),
                (MTLPixelFormat::R16Unorm, MTLPixelFormat::RG16Unorm)
            );
        }
    }
}
