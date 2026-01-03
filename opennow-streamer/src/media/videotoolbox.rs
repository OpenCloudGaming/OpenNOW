//! VideoToolbox Zero-Copy Support (macOS only)
//!
//! Provides zero-copy video frame handling by keeping decoded frames on GPU.
//! Instead of copying pixel data from VideoToolbox to CPU memory, we:
//! 1. Extract the CVPixelBuffer from the decoded AVFrame
//! 2. Retain it and pass to the renderer
//! 3. Create Metal textures directly from the IOSurface
//! 4. Use those textures in wgpu for rendering
//!
//! This eliminates ~360MB/sec of memory copies at 1080p@120fps.

#![cfg(target_os = "macos")]

use std::ffi::c_void;
use std::sync::Arc;
use log::{info, debug, warn};
use objc::runtime::{Object, YES};
use objc::{class, msg_send, sel, sel_impl};
use foreign_types::ForeignType;

// Core Video FFI
#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVPixelBufferRetain(buffer: *mut c_void) -> *mut c_void;
    fn CVPixelBufferRelease(buffer: *mut c_void);
    fn CVPixelBufferGetWidth(buffer: *mut c_void) -> usize;
    fn CVPixelBufferGetHeight(buffer: *mut c_void) -> usize;
    fn CVPixelBufferGetPixelFormatType(buffer: *mut c_void) -> u32;
    fn CVPixelBufferGetIOSurface(buffer: *mut c_void) -> *mut c_void;
    fn CVPixelBufferLockBaseAddress(buffer: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferUnlockBaseAddress(buffer: *mut c_void, flags: u64) -> i32;
    fn CVPixelBufferGetPlaneCount(buffer: *mut c_void) -> usize;
    fn CVPixelBufferGetBaseAddressOfPlane(buffer: *mut c_void, plane: usize) -> *mut u8;
    fn CVPixelBufferGetBytesPerRowOfPlane(buffer: *mut c_void, plane: usize) -> usize;
    fn CVPixelBufferGetHeightOfPlane(buffer: *mut c_void, plane: usize) -> usize;
    fn CVPixelBufferGetWidthOfPlane(buffer: *mut c_void, plane: usize) -> usize;
}

// Metal FFI for getting the system default device
#[link(name = "Metal", kind = "framework")]
extern "C" {
    fn MTLCreateSystemDefaultDevice() -> *mut Object;
}

// CoreVideo Metal texture cache FFI - TRUE zero-copy
#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVMetalTextureCacheCreate(
        allocator: *const c_void,
        cache_attributes: *const c_void,
        metal_device: *mut Object,
        texture_attributes: *const c_void,
        cache_out: *mut *mut c_void,
    ) -> i32;

    fn CVMetalTextureCacheCreateTextureFromImage(
        allocator: *const c_void,
        texture_cache: *mut c_void,
        source_image: *mut c_void,  // CVPixelBufferRef
        texture_attributes: *const c_void,
        pixel_format: u64,  // MTLPixelFormat
        width: usize,
        height: usize,
        plane_index: usize,
        texture_out: *mut *mut c_void,
    ) -> i32;

    fn CVMetalTextureGetTexture(texture: *mut c_void) -> *mut Object;  // Returns MTLTexture
    fn CVMetalTextureCacheFlush(texture_cache: *mut c_void, options: u64);
}

// kCVReturn success
const K_CV_RETURN_SUCCESS: i32 = 0;

// MTLPixelFormat values
const MTL_PIXEL_FORMAT_R8_UNORM: u64 = 10;      // For Y plane
const MTL_PIXEL_FORMAT_RG8_UNORM: u64 = 30;     // For UV plane (interleaved)

// Lock flags
const K_CV_PIXEL_BUFFER_LOCK_READ_ONLY: u64 = 0x00000001;

// IOSurface FFI
#[link(name = "IOSurface", kind = "framework")]
extern "C" {
    fn IOSurfaceGetWidth(surface: *mut c_void) -> usize;
    fn IOSurfaceGetHeight(surface: *mut c_void) -> usize;
    fn IOSurfaceIncrementUseCount(surface: *mut c_void);
    fn IOSurfaceDecrementUseCount(surface: *mut c_void);
}

// NV12 format constants
const K_CV_PIXEL_FORMAT_TYPE_420_YP_CB_CR_8_BI_PLANAR_VIDEO_RANGE: u32 = 0x34323076; // '420v'
const K_CV_PIXEL_FORMAT_TYPE_420_YP_CB_CR_8_BI_PLANAR_FULL_RANGE: u32 = 0x34323066; // '420f'

/// Wrapper around CVPixelBuffer that handles retain/release
/// This allows passing the GPU buffer between threads safely
pub struct CVPixelBufferWrapper {
    buffer: *mut c_void,
    width: u32,
    height: u32,
    is_nv12: bool,
}

impl std::fmt::Debug for CVPixelBufferWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CVPixelBufferWrapper")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("is_nv12", &self.is_nv12)
            .finish()
    }
}

// CVPixelBuffer is reference-counted and thread-safe
unsafe impl Send for CVPixelBufferWrapper {}
unsafe impl Sync for CVPixelBufferWrapper {}

impl CVPixelBufferWrapper {
    /// Create a new wrapper, retaining the CVPixelBuffer
    ///
    /// # Safety
    /// The provided pointer must be a valid CVPixelBufferRef
    pub unsafe fn new(buffer: *mut c_void) -> Option<Self> {
        if buffer.is_null() {
            return None;
        }

        // Retain the buffer so it stays valid
        CVPixelBufferRetain(buffer);

        let width = CVPixelBufferGetWidth(buffer) as u32;
        let height = CVPixelBufferGetHeight(buffer) as u32;
        let format = CVPixelBufferGetPixelFormatType(buffer);

        // Check if it's NV12 format (what VideoToolbox typically outputs)
        let is_nv12 = format == K_CV_PIXEL_FORMAT_TYPE_420_YP_CB_CR_8_BI_PLANAR_VIDEO_RANGE
            || format == K_CV_PIXEL_FORMAT_TYPE_420_YP_CB_CR_8_BI_PLANAR_FULL_RANGE;

        if !is_nv12 {
            debug!("CVPixelBuffer format is not NV12: {:#x}", format);
        }

        Some(Self {
            buffer,
            width,
            height,
            is_nv12,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn is_nv12(&self) -> bool {
        self.is_nv12
    }

    /// Get the IOSurface backing this pixel buffer
    /// Returns None if the buffer is not backed by an IOSurface
    pub fn io_surface(&self) -> Option<*mut c_void> {
        unsafe {
            let surface = CVPixelBufferGetIOSurface(self.buffer);
            if surface.is_null() {
                None
            } else {
                Some(surface)
            }
        }
    }

    /// Get the raw CVPixelBufferRef (for FFI)
    pub fn as_raw(&self) -> *mut c_void {
        self.buffer
    }

    /// Lock the pixel buffer and get direct access to plane data
    /// This maps GPU memory to CPU address space WITHOUT copying
    /// Returns (y_data, y_stride, uv_data, uv_stride) for NV12 format
    /// IMPORTANT: Call unlock() when done to release the mapping
    pub fn lock_and_get_planes(&self) -> Option<LockedPlanes> {
        unsafe {
            // Lock for read-only access (faster)
            let result = CVPixelBufferLockBaseAddress(self.buffer, K_CV_PIXEL_BUFFER_LOCK_READ_ONLY);
            if result != 0 {
                warn!("Failed to lock CVPixelBuffer: {}", result);
                return None;
            }

            let plane_count = CVPixelBufferGetPlaneCount(self.buffer);
            if plane_count < 2 {
                warn!("CVPixelBuffer has {} planes, expected 2 for NV12", plane_count);
                CVPixelBufferUnlockBaseAddress(self.buffer, K_CV_PIXEL_BUFFER_LOCK_READ_ONLY);
                return None;
            }

            // Y plane (plane 0)
            let y_ptr = CVPixelBufferGetBaseAddressOfPlane(self.buffer, 0);
            let y_stride = CVPixelBufferGetBytesPerRowOfPlane(self.buffer, 0);
            let y_height = CVPixelBufferGetHeightOfPlane(self.buffer, 0);

            // UV plane (plane 1) - interleaved for NV12
            let uv_ptr = CVPixelBufferGetBaseAddressOfPlane(self.buffer, 1);
            let uv_stride = CVPixelBufferGetBytesPerRowOfPlane(self.buffer, 1);
            let uv_height = CVPixelBufferGetHeightOfPlane(self.buffer, 1);

            if y_ptr.is_null() || uv_ptr.is_null() {
                warn!("CVPixelBuffer plane pointers are null");
                CVPixelBufferUnlockBaseAddress(self.buffer, K_CV_PIXEL_BUFFER_LOCK_READ_ONLY);
                return None;
            }

            Some(LockedPlanes {
                buffer: self.buffer,
                y_data: std::slice::from_raw_parts(y_ptr, y_stride * y_height),
                y_stride: y_stride as u32,
                y_height: y_height as u32,
                uv_data: std::slice::from_raw_parts(uv_ptr, uv_stride * uv_height),
                uv_stride: uv_stride as u32,
                uv_height: uv_height as u32,
            })
        }
    }
}

/// Locked plane data from CVPixelBuffer
/// Automatically unlocks on drop
pub struct LockedPlanes<'a> {
    buffer: *mut c_void,
    pub y_data: &'a [u8],
    pub y_stride: u32,
    pub y_height: u32,
    pub uv_data: &'a [u8],
    pub uv_stride: u32,
    pub uv_height: u32,
}

impl<'a> Drop for LockedPlanes<'a> {
    fn drop(&mut self) {
        unsafe {
            CVPixelBufferUnlockBaseAddress(self.buffer, K_CV_PIXEL_BUFFER_LOCK_READ_ONLY);
        }
    }
}

impl Drop for CVPixelBufferWrapper {
    fn drop(&mut self) {
        unsafe {
            CVPixelBufferRelease(self.buffer);
        }
    }
}

impl Clone for CVPixelBufferWrapper {
    fn clone(&self) -> Self {
        unsafe {
            CVPixelBufferRetain(self.buffer);
        }
        Self {
            buffer: self.buffer,
            width: self.width,
            height: self.height,
            is_nv12: self.is_nv12,
        }
    }
}

/// Extract CVPixelBuffer from an FFmpeg hardware frame
///
/// # Safety
/// The AVFrame must be a VideoToolbox hardware frame (format = AV_PIX_FMT_VIDEOTOOLBOX)
/// The frame_data_3 parameter should be frame.data[3] from the AVFrame
pub unsafe fn extract_cv_pixel_buffer_from_data(frame_data_3: *mut u8) -> Option<CVPixelBufferWrapper> {
    // For VideoToolbox frames, data[3] contains the CVPixelBufferRef
    // This is FFmpeg's convention for VideoToolbox hardware frames
    let cv_buffer = frame_data_3 as *mut c_void;
    CVPixelBufferWrapper::new(cv_buffer)
}

/// Zero-copy video frame that holds GPU buffer reference
#[derive(Clone)]
pub struct ZeroCopyFrame {
    pub buffer: Arc<CVPixelBufferWrapper>,
}

impl ZeroCopyFrame {
    pub fn new(buffer: CVPixelBufferWrapper) -> Self {
        Self {
            buffer: Arc::new(buffer),
        }
    }

    pub fn width(&self) -> u32 {
        self.buffer.width()
    }

    pub fn height(&self) -> u32 {
        self.buffer.height()
    }
}

impl std::fmt::Debug for ZeroCopyFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ZeroCopyFrame")
            .field("width", &self.buffer.width())
            .field("height", &self.buffer.height())
            .field("is_nv12", &self.buffer.is_nv12())
            .finish()
    }
}

/// IOSurface wrapper for safe handling
pub struct IOSurfaceWrapper {
    surface: *mut c_void,
}

unsafe impl Send for IOSurfaceWrapper {}
unsafe impl Sync for IOSurfaceWrapper {}

impl IOSurfaceWrapper {
    pub unsafe fn new(surface: *mut c_void) -> Option<Self> {
        if surface.is_null() {
            return None;
        }
        IOSurfaceIncrementUseCount(surface);
        Some(Self { surface })
    }

    pub fn as_ptr(&self) -> *mut c_void {
        self.surface
    }

    pub fn width(&self) -> u32 {
        unsafe { IOSurfaceGetWidth(self.surface) as u32 }
    }

    pub fn height(&self) -> u32 {
        unsafe { IOSurfaceGetHeight(self.surface) as u32 }
    }
}

impl Drop for IOSurfaceWrapper {
    fn drop(&mut self) {
        unsafe {
            IOSurfaceDecrementUseCount(self.surface);
        }
    }
}

/// Metal texture pair created from IOSurface (Y and UV planes)
/// These textures share memory with the CVPixelBuffer - true zero-copy!
pub struct MetalTexturesFromIOSurface {
    pub y_texture: *mut Object,   // MTLTexture for Y plane
    pub uv_texture: *mut Object,  // MTLTexture for UV plane (interleaved)
    pub width: u32,
    pub height: u32,
    // Keep the CVPixelBuffer alive while textures are in use
    _cv_buffer: Arc<CVPixelBufferWrapper>,
}

unsafe impl Send for MetalTexturesFromIOSurface {}
unsafe impl Sync for MetalTexturesFromIOSurface {}

impl Drop for MetalTexturesFromIOSurface {
    fn drop(&mut self) {
        unsafe {
            if !self.y_texture.is_null() {
                let _: () = msg_send![self.y_texture, release];
            }
            if !self.uv_texture.is_null() {
                let _: () = msg_send![self.uv_texture, release];
            }
        }
    }
}

impl MetalTexturesFromIOSurface {
    /// Create Metal textures directly from CVPixelBuffer's IOSurface
    /// This is TRUE zero-copy - the textures share GPU memory with the decoded frame
    pub fn from_cv_buffer(
        cv_buffer: Arc<CVPixelBufferWrapper>,
        metal_device: *mut Object,
    ) -> Option<Self> {
        if metal_device.is_null() {
            warn!("Metal device is null");
            return None;
        }

        let io_surface = cv_buffer.io_surface()?;
        let width = cv_buffer.width();
        let height = cv_buffer.height();

        unsafe {
            // Create Y texture (plane 0) - R8Unorm format
            let y_texture = Self::create_texture_from_iosurface(
                metal_device,
                io_surface,
                0,  // plane 0 = Y
                width,
                height,
                8,  // MTLPixelFormatR8Unorm
            )?;

            // Create UV texture (plane 1) - RG8Unorm format (interleaved UV)
            let uv_texture = Self::create_texture_from_iosurface(
                metal_device,
                io_surface,
                1,  // plane 1 = UV
                width / 2,
                height / 2,
                30, // MTLPixelFormatRG8Unorm
            )?;

            info!("Created Metal textures from IOSurface: {}x{} (zero-copy)", width, height);

            Some(Self {
                y_texture,
                uv_texture,
                width,
                height,
                _cv_buffer: cv_buffer,
            })
        }
    }

    /// Create a single Metal texture from an IOSurface plane
    unsafe fn create_texture_from_iosurface(
        device: *mut Object,
        io_surface: *mut c_void,
        plane: usize,
        width: u32,
        height: u32,
        pixel_format: u64,
    ) -> Option<*mut Object> {
        // Create MTLTextureDescriptor
        let descriptor: *mut Object = msg_send![class!(MTLTextureDescriptor), new];
        if descriptor.is_null() {
            return None;
        }

        // Configure descriptor
        let _: () = msg_send![descriptor, setTextureType: 2u64]; // MTLTextureType2D
        let _: () = msg_send![descriptor, setPixelFormat: pixel_format];
        let _: () = msg_send![descriptor, setWidth: width as u64];
        let _: () = msg_send![descriptor, setHeight: height as u64];
        let _: () = msg_send![descriptor, setStorageMode: 1u64]; // MTLStorageModeManaged (shared on Apple Silicon)
        let _: () = msg_send![descriptor, setUsage: 1u64]; // MTLTextureUsageShaderRead

        // Create texture from IOSurface
        let texture: *mut Object = msg_send![device, newTextureWithDescriptor:descriptor iosurface:io_surface plane:plane];

        // Release descriptor
        let _: () = msg_send![descriptor, release];

        if texture.is_null() {
            warn!("Failed to create Metal texture from IOSurface plane {}", plane);
            return None;
        }

        Some(texture)
    }
}

/// Manager for zero-copy GPU textures using CVMetalTextureCache
/// This creates Metal textures that share GPU memory with CVPixelBuffer - NO CPU COPY!
pub struct ZeroCopyTextureManager {
    metal_device: *mut Object,
    texture_cache: *mut c_void,
    command_queue: *mut Object,  // Cached command queue for GPU blits
}

unsafe impl Send for ZeroCopyTextureManager {}
unsafe impl Sync for ZeroCopyTextureManager {}

impl ZeroCopyTextureManager {
    /// Create a new texture manager with CVMetalTextureCache
    pub fn new() -> Option<Self> {
        unsafe {
            // Get system default Metal device
            let metal_device = MTLCreateSystemDefaultDevice();
            if metal_device.is_null() {
                warn!("Could not get Metal device");
                return None;
            }

            // Create CVMetalTextureCache
            let mut texture_cache: *mut c_void = std::ptr::null_mut();
            let result = CVMetalTextureCacheCreate(
                std::ptr::null(),  // default allocator
                std::ptr::null(),  // no cache attributes
                metal_device,
                std::ptr::null(),  // no texture attributes
                &mut texture_cache,
            );

            if result != K_CV_RETURN_SUCCESS || texture_cache.is_null() {
                warn!("Failed to create CVMetalTextureCache: {}", result);
                let _: () = msg_send![metal_device, release];
                return None;
            }

            // Create a persistent command queue for GPU blits
            let command_queue: *mut Object = msg_send![metal_device, newCommandQueue];
            if command_queue.is_null() {
                warn!("Failed to create Metal command queue");
                CFRelease(texture_cache);
                let _: () = msg_send![metal_device, release];
                return None;
            }

            info!("ZeroCopyTextureManager: Created with CVMetalTextureCache and command queue (TRUE zero-copy)");
            Some(Self { metal_device, texture_cache, command_queue })
        }
    }

    /// Create Metal textures from CVPixelBuffer - TRUE ZERO-COPY
    /// Returns (y_texture, uv_texture) as raw MTLTexture pointers
    pub fn create_textures_from_cv_buffer(
        &self,
        cv_buffer: &CVPixelBufferWrapper,
    ) -> Option<(CVMetalTexture, CVMetalTexture)> {
        let width = cv_buffer.width() as usize;
        let height = cv_buffer.height() as usize;

        unsafe {
            // Create Y plane texture (plane 0)
            let mut y_cv_texture: *mut c_void = std::ptr::null_mut();
            let result = CVMetalTextureCacheCreateTextureFromImage(
                std::ptr::null(),
                self.texture_cache,
                cv_buffer.as_raw(),
                std::ptr::null(),
                MTL_PIXEL_FORMAT_R8_UNORM,
                width,
                height,
                0,  // plane 0 = Y
                &mut y_cv_texture,
            );

            if result != K_CV_RETURN_SUCCESS || y_cv_texture.is_null() {
                warn!("Failed to create Y texture from CVPixelBuffer: {}", result);
                return None;
            }

            // Create UV plane texture (plane 1)
            let mut uv_cv_texture: *mut c_void = std::ptr::null_mut();
            let result = CVMetalTextureCacheCreateTextureFromImage(
                std::ptr::null(),
                self.texture_cache,
                cv_buffer.as_raw(),
                std::ptr::null(),
                MTL_PIXEL_FORMAT_RG8_UNORM,
                width / 2,
                height / 2,
                1,  // plane 1 = UV
                &mut uv_cv_texture,
            );

            if result != K_CV_RETURN_SUCCESS || uv_cv_texture.is_null() {
                warn!("Failed to create UV texture from CVPixelBuffer: {}", result);
                // Clean up Y texture
                CFRelease(y_cv_texture);
                return None;
            }

            Some((
                CVMetalTexture::new(y_cv_texture, width as u32, height as u32, MTL_PIXEL_FORMAT_R8_UNORM),
                CVMetalTexture::new(uv_cv_texture, (width / 2) as u32, (height / 2) as u32, MTL_PIXEL_FORMAT_RG8_UNORM),
            ))
        }
    }

    /// Flush the texture cache (call periodically to free unused textures)
    pub fn flush(&self) {
        unsafe {
            CVMetalTextureCacheFlush(self.texture_cache, 0);
        }
    }

    /// Get the Metal device pointer
    pub fn metal_device(&self) -> *mut Object {
        self.metal_device
    }

    /// Get the cached command queue for GPU blits
    pub fn command_queue(&self) -> *mut Object {
        self.command_queue
    }
}

impl Default for ZeroCopyTextureManager {
    fn default() -> Self {
        Self::new().expect("Failed to create CVMetalTextureCache")
    }
}

impl Drop for ZeroCopyTextureManager {
    fn drop(&mut self) {
        unsafe {
            if !self.command_queue.is_null() {
                let _: () = msg_send![self.command_queue, release];
            }
            if !self.texture_cache.is_null() {
                CFRelease(self.texture_cache);
            }
            if !self.metal_device.is_null() {
                let _: () = msg_send![self.metal_device, release];
            }
        }
    }
}

// CFRelease for CoreFoundation objects
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *mut c_void);
    fn CFRetain(cf: *mut c_void) -> *mut c_void;
}

/// Wrapper around CVMetalTexture that handles release
pub struct CVMetalTexture {
    cv_texture: *mut c_void,
    width: u32,
    height: u32,
    format: u64,  // MTLPixelFormat
}

unsafe impl Send for CVMetalTexture {}
unsafe impl Sync for CVMetalTexture {}

impl CVMetalTexture {
    fn new(cv_texture: *mut c_void, width: u32, height: u32, format: u64) -> Self {
        Self { cv_texture, width, height, format }
    }

    /// Get the underlying MTLTexture pointer - this shares GPU memory with CVPixelBuffer!
    pub fn metal_texture_ptr(&self) -> *mut Object {
        unsafe { CVMetalTextureGetTexture(self.cv_texture) }
    }

    /// Get as metal-rs Texture type (for wgpu-hal integration)
    /// The returned texture shares GPU memory with the CVPixelBuffer - TRUE ZERO-COPY!
    pub fn as_metal_texture(&self) -> metal::Texture {
        unsafe {
            let ptr = self.metal_texture_ptr();
            // Retain because metal::Texture will release on drop
            let _: () = msg_send![ptr, retain];
            metal::Texture::from_ptr(ptr as *mut _)
        }
    }

    pub fn width(&self) -> u32 { self.width }
    pub fn height(&self) -> u32 { self.height }
    pub fn pixel_format(&self) -> u64 { self.format }

    /// Convert MTLPixelFormat to wgpu TextureFormat
    pub fn wgpu_format(&self) -> wgpu::TextureFormat {
        match self.format {
            10 => wgpu::TextureFormat::R8Unorm,   // MTLPixelFormatR8Unorm (Y plane)
            30 => wgpu::TextureFormat::Rg8Unorm,  // MTLPixelFormatRG8Unorm (UV plane)
            _ => wgpu::TextureFormat::R8Unorm,
        }
    }
}

impl Drop for CVMetalTexture {
    fn drop(&mut self) {
        if !self.cv_texture.is_null() {
            unsafe { CFRelease(self.cv_texture); }
        }
    }
}

/// Metal-based video renderer for TRUE zero-copy rendering
/// Renders NV12 video directly from CVMetalTexture to the screen
pub struct MetalVideoRenderer {
    device: *mut Object,
    command_queue: *mut Object,
    pipeline_state: *mut Object,
    sampler_state: *mut Object,
}

unsafe impl Send for MetalVideoRenderer {}
unsafe impl Sync for MetalVideoRenderer {}

impl MetalVideoRenderer {
    /// Create a new Metal video renderer
    pub fn new(device: *mut Object) -> Option<Self> {
        unsafe {
            if device.is_null() {
                return None;
            }

            // Retain device
            let _: () = msg_send![device, retain];

            // Create command queue
            let command_queue: *mut Object = msg_send![device, newCommandQueue];
            if command_queue.is_null() {
                warn!("Failed to create Metal command queue");
                let _: () = msg_send![device, release];
                return None;
            }

            // Create sampler state
            let sampler_descriptor: *mut Object = msg_send![class!(MTLSamplerDescriptor), new];
            let _: () = msg_send![sampler_descriptor, setMinFilter: 1u64]; // Linear
            let _: () = msg_send![sampler_descriptor, setMagFilter: 1u64]; // Linear
            let _: () = msg_send![sampler_descriptor, setSAddressMode: 0u64]; // ClampToEdge
            let _: () = msg_send![sampler_descriptor, setTAddressMode: 0u64]; // ClampToEdge

            let sampler_state: *mut Object = msg_send![device, newSamplerStateWithDescriptor: sampler_descriptor];
            let _: () = msg_send![sampler_descriptor, release];

            if sampler_state.is_null() {
                warn!("Failed to create Metal sampler state");
                let _: () = msg_send![command_queue, release];
                let _: () = msg_send![device, release];
                return None;
            }

            // Create render pipeline for NV12 to RGB conversion
            let pipeline_state = Self::create_nv12_pipeline(device)?;

            info!("MetalVideoRenderer: Initialized for zero-copy video rendering");

            Some(Self {
                device,
                command_queue,
                pipeline_state,
                sampler_state,
            })
        }
    }

    /// Create the NV12 to RGB render pipeline
    unsafe fn create_nv12_pipeline(device: *mut Object) -> Option<*mut Object> {
        // NV12 to RGB shader source (Metal Shading Language)
        let shader_source = r#"
            #include <metal_stdlib>
            using namespace metal;

            struct VertexOut {
                float4 position [[position]];
                float2 texCoord;
            };

            // Full-screen triangle vertex shader
            vertex VertexOut nv12_vertex(uint vertexID [[vertex_id]]) {
                VertexOut out;
                // Generate full-screen triangle
                float2 positions[3] = {
                    float2(-1.0, -1.0),
                    float2(3.0, -1.0),
                    float2(-1.0, 3.0)
                };
                float2 texCoords[3] = {
                    float2(0.0, 1.0),
                    float2(2.0, 1.0),
                    float2(0.0, -1.0)
                };
                out.position = float4(positions[vertexID], 0.0, 1.0);
                out.texCoord = texCoords[vertexID];
                return out;
            }

            // NV12 to RGB fragment shader (BT.709)
            fragment float4 nv12_fragment(
                VertexOut in [[stage_in]],
                texture2d<float> yTexture [[texture(0)]],
                texture2d<float> uvTexture [[texture(1)]],
                sampler texSampler [[sampler(0)]]
            ) {
                float y = yTexture.sample(texSampler, in.texCoord).r;
                float2 uv = uvTexture.sample(texSampler, in.texCoord).rg;

                // BT.709 YUV to RGB conversion (video range)
                float u = uv.r - 0.5;
                float v = uv.g - 0.5;

                // BT.709 matrix
                float r = y + 1.5748 * v;
                float g = y - 0.1873 * u - 0.4681 * v;
                float b = y + 1.8556 * u;

                return float4(saturate(float3(r, g, b)), 1.0);
            }
        "#;

        // Create shader library
        let source_nsstring = Self::create_nsstring(shader_source);
        if source_nsstring.is_null() {
            return None;
        }

        let mut error: *mut Object = std::ptr::null_mut();
        let library: *mut Object = msg_send![device, newLibraryWithSource: source_nsstring options: std::ptr::null::<Object>() error: &mut error];
        let _: () = msg_send![source_nsstring, release];

        if library.is_null() {
            if !error.is_null() {
                let desc: *mut Object = msg_send![error, localizedDescription];
                let cstr: *const i8 = msg_send![desc, UTF8String];
                if !cstr.is_null() {
                    let err_str = std::ffi::CStr::from_ptr(cstr).to_string_lossy();
                    warn!("Metal shader compilation error: {}", err_str);
                }
            }
            return None;
        }

        // Get vertex and fragment functions
        let vertex_name = Self::create_nsstring("nv12_vertex");
        let fragment_name = Self::create_nsstring("nv12_fragment");

        let vertex_fn: *mut Object = msg_send![library, newFunctionWithName: vertex_name];
        let fragment_fn: *mut Object = msg_send![library, newFunctionWithName: fragment_name];

        let _: () = msg_send![vertex_name, release];
        let _: () = msg_send![fragment_name, release];
        let _: () = msg_send![library, release];

        if vertex_fn.is_null() || fragment_fn.is_null() {
            warn!("Failed to get shader functions");
            return None;
        }

        // Create pipeline descriptor
        let pipeline_desc: *mut Object = msg_send![class!(MTLRenderPipelineDescriptor), new];
        let _: () = msg_send![pipeline_desc, setVertexFunction: vertex_fn];
        let _: () = msg_send![pipeline_desc, setFragmentFunction: fragment_fn];

        // Set color attachment format (BGRA8Unorm for Metal drawable)
        let color_attachments: *mut Object = msg_send![pipeline_desc, colorAttachments];
        let attachment0: *mut Object = msg_send![color_attachments, objectAtIndexedSubscript: 0usize];
        let _: () = msg_send![attachment0, setPixelFormat: 80u64]; // MTLPixelFormatBGRA8Unorm

        // Create pipeline state
        let pipeline_state: *mut Object = msg_send![device, newRenderPipelineStateWithDescriptor: pipeline_desc error: &mut error];

        let _: () = msg_send![pipeline_desc, release];
        let _: () = msg_send![vertex_fn, release];
        let _: () = msg_send![fragment_fn, release];

        if pipeline_state.is_null() {
            warn!("Failed to create render pipeline state");
            return None;
        }

        Some(pipeline_state)
    }

    /// Helper to create NSString
    unsafe fn create_nsstring(s: &str) -> *mut Object {
        let nsstring_class = class!(NSString);
        let bytes = s.as_ptr() as *const i8;
        let len = s.len();
        msg_send![nsstring_class, stringWithUTF8String: bytes]
    }

    /// Render video frame using Metal - TRUE ZERO-COPY
    /// Takes CVMetalTextures and renders directly to the provided drawable
    pub fn render(
        &self,
        y_texture: &CVMetalTexture,
        uv_texture: &CVMetalTexture,
        drawable: *mut Object,  // CAMetalDrawable
    ) -> bool {
        unsafe {
            let y_mtl = y_texture.metal_texture_ptr();
            let uv_mtl = uv_texture.metal_texture_ptr();

            if y_mtl.is_null() || uv_mtl.is_null() || drawable.is_null() {
                return false;
            }

            // Get drawable texture
            let target_texture: *mut Object = msg_send![drawable, texture];
            if target_texture.is_null() {
                return false;
            }

            // Create command buffer
            let command_buffer: *mut Object = msg_send![self.command_queue, commandBuffer];
            if command_buffer.is_null() {
                return false;
            }

            // Create render pass descriptor
            let pass_desc: *mut Object = msg_send![class!(MTLRenderPassDescriptor), renderPassDescriptor];
            let color_attachments: *mut Object = msg_send![pass_desc, colorAttachments];
            let attachment0: *mut Object = msg_send![color_attachments, objectAtIndexedSubscript: 0usize];
            let _: () = msg_send![attachment0, setTexture: target_texture];
            let _: () = msg_send![attachment0, setLoadAction: 2u64]; // Clear
            let _: () = msg_send![attachment0, setStoreAction: 1u64]; // Store

            // Create render encoder
            let encoder: *mut Object = msg_send![command_buffer, renderCommandEncoderWithDescriptor: pass_desc];
            if encoder.is_null() {
                return false;
            }

            // Set pipeline state
            let _: () = msg_send![encoder, setRenderPipelineState: self.pipeline_state];

            // Set textures (Y = 0, UV = 1)
            let _: () = msg_send![encoder, setFragmentTexture: y_mtl atIndex: 0usize];
            let _: () = msg_send![encoder, setFragmentTexture: uv_mtl atIndex: 1usize];
            let _: () = msg_send![encoder, setFragmentSamplerState: self.sampler_state atIndex: 0usize];

            // Draw full-screen triangle
            let _: () = msg_send![encoder, drawPrimitives: 3u64 vertexStart: 0usize vertexCount: 3usize]; // Triangle

            // End encoding
            let _: () = msg_send![encoder, endEncoding];

            // Present and commit
            let _: () = msg_send![command_buffer, presentDrawable: drawable];
            let _: () = msg_send![command_buffer, commit];

            true
        }
    }
}

impl Drop for MetalVideoRenderer {
    fn drop(&mut self) {
        unsafe {
            if !self.sampler_state.is_null() {
                let _: () = msg_send![self.sampler_state, release];
            }
            if !self.pipeline_state.is_null() {
                let _: () = msg_send![self.pipeline_state, release];
            }
            if !self.command_queue.is_null() {
                let _: () = msg_send![self.command_queue, release];
            }
            if !self.device.is_null() {
                let _: () = msg_send![self.device, release];
            }
        }
    }
}
