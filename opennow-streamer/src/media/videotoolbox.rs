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

// Core Video FFI
#[link(name = "CoreVideo", kind = "framework")]
extern "C" {
    fn CVPixelBufferRetain(buffer: *mut c_void) -> *mut c_void;
    fn CVPixelBufferRelease(buffer: *mut c_void);
    fn CVPixelBufferGetWidth(buffer: *mut c_void) -> usize;
    fn CVPixelBufferGetHeight(buffer: *mut c_void) -> usize;
    fn CVPixelBufferGetPixelFormatType(buffer: *mut c_void) -> u32;
    fn CVPixelBufferGetIOSurface(buffer: *mut c_void) -> *mut c_void;
}

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

// Note: Full zero-copy texture integration with wgpu requires using
// Metal's newTextureWithIOSurface API and wgpu's hal layer.
// This is complex due to wgpu's hal API requirements.
// For now, the NV12 direct path (skipping CPU scaler) provides significant savings.
// TODO: Implement ZeroCopyTextureManager using objc/metal directly

/// Placeholder for future zero-copy texture manager
pub struct ZeroCopyTextureManager {
    _initialized: bool,
}

impl ZeroCopyTextureManager {
    /// Create a new texture manager from wgpu device
    /// Returns None if zero-copy is not supported or available
    pub fn new(_wgpu_device: &wgpu::Device) -> Option<Self> {
        // TODO: Implement using Metal API directly via objc
        // For now, return None to use fallback CPU path
        info!("ZeroCopyTextureManager: Not yet implemented, using CPU path");
        None
    }
}
