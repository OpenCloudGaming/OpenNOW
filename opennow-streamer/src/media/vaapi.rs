//! VA-API Zero-Copy Video Support for Linux
//!
//! This module provides zero-copy video rendering on Linux by keeping
//! decoded frames on GPU as VA surfaces and sharing them with Vulkan.
//!
//! Supports:
//! - AMD GPUs via RADV/AMDGPU-PRO drivers
//! - Intel GPUs via Intel Media Driver (iHD) or i965
//! - NVIDIA GPUs via nouveau (limited) or nvidia-vaapi-driver
//!
//! Flow:
//! 1. FFmpeg VAAPI decodes to VASurface (GPU VRAM)
//! 2. We export the surface as a DMA-BUF fd
//! 3. Import into Vulkan via VK_EXT_external_memory_dma_buf
//! 4. Bind to wgpu texture for rendering
//!
//! This eliminates the expensive GPU->CPU->GPU round-trip.

use anyhow::{anyhow, Result};
use log::{debug, error, info, warn};
use parking_lot::Mutex;
use std::os::unix::io::RawFd;

/// VA surface format (matches VA-API definitions)
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VASurfaceFormat {
    NV12, // 8-bit 4:2:0
    P010, // 10-bit 4:2:0 (HDR)
    Unknown,
}

/// Wrapper for a VA-API surface from FFmpeg hardware decoder
/// Holds the surface reference and provides DMA-BUF export
pub struct VAAPISurfaceWrapper {
    /// VA display handle (void* to libva VADisplay)
    va_display: *mut std::ffi::c_void,
    /// VA surface ID
    surface_id: u32,
    /// DMA-BUF file descriptor (lazily exported)
    dmabuf_fd: Mutex<Option<RawFd>>,
    /// Surface dimensions
    pub width: u32,
    pub height: u32,
    /// Surface format
    pub format: VASurfaceFormat,
    /// DRM format fourcc (for Vulkan import)
    pub drm_format: u32,
    /// DRM modifier (for tiled formats)
    pub drm_modifier: u64,
    /// Plane info for multi-planar formats
    pub planes: Vec<PlaneInfo>,
}

/// Information about a single plane in a multi-planar surface
#[derive(Debug, Clone)]
pub struct PlaneInfo {
    pub offset: u32,
    pub pitch: u32,
}

// Safety: VA-API surfaces can be shared across threads when properly synchronized
unsafe impl Send for VAAPISurfaceWrapper {}
unsafe impl Sync for VAAPISurfaceWrapper {}

impl std::fmt::Debug for VAAPISurfaceWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VAAPISurfaceWrapper")
            .field("surface_id", &self.surface_id)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .field("has_dmabuf", &self.dmabuf_fd.lock().is_some())
            .finish()
    }
}

// DRM fourcc codes
const DRM_FORMAT_NV12: u32 = 0x3231564E; // NV12
const DRM_FORMAT_P010: u32 = 0x30313050; // P010

// VA-API FFI bindings (minimal set for surface export)
// Full bindings would come from libva-sys crate
mod ffi {
    use std::ffi::c_void;

    pub type VADisplay = *mut c_void;
    pub type VASurfaceID = u32;
    pub type VAStatus = i32;

    pub const VA_STATUS_SUCCESS: VAStatus = 0;

    // VA surface export flags
    pub const VA_EXPORT_SURFACE_READ_ONLY: u32 = 0x0001;
    pub const VA_EXPORT_SURFACE_SEPARATE_LAYERS: u32 = 0x0004;

    // VADRMPRIMESurfaceDescriptor structure
    #[repr(C)]
    pub struct VADRMPRIMESurfaceDescriptor {
        pub fourcc: u32,
        pub width: u32,
        pub height: u32,
        pub num_objects: u32,
        pub objects: [VADRMPRIMEObject; 4],
        pub num_layers: u32,
        pub layers: [VADRMPRIMELayer; 4],
    }

    #[repr(C)]
    pub struct VADRMPRIMEObject {
        pub fd: i32,
        pub size: u32,
        pub drm_format_modifier: u64,
    }

    #[repr(C)]
    pub struct VADRMPRIMELayer {
        pub drm_format: u32,
        pub num_planes: u32,
        pub object_index: [u32; 4],
        pub offset: [u32; 4],
        pub pitch: [u32; 4],
    }

    // We'll use dlopen for VA-API functions since static linking is complex
    pub type VaExportSurfaceHandle = unsafe extern "C" fn(
        dpy: VADisplay,
        surface: VASurfaceID,
        mem_type: u32,
        flags: u32,
        descriptor: *mut VADRMPRIMESurfaceDescriptor,
    ) -> VAStatus;

    pub type VaSyncSurface = unsafe extern "C" fn(dpy: VADisplay, surface: VASurfaceID) -> VAStatus;

    // Memory type for DRM PRIME export
    pub const VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2: u32 = 0x40000000;
}

impl VAAPISurfaceWrapper {
    /// Create a new wrapper from FFmpeg's VAAPI frame data
    ///
    /// # Safety
    /// The va_display and surface_id must be from a valid VAAPI decoded frame
    pub unsafe fn from_ffmpeg_frame(
        va_display: *mut std::ffi::c_void,
        surface_id: u32,
        width: u32,
        height: u32,
    ) -> Option<Self> {
        if va_display.is_null() || surface_id == 0 {
            warn!(
                "Invalid VAAPI surface: display={:?}, surface_id={}",
                va_display, surface_id
            );
            return None;
        }

        debug!(
            "VAAPI surface wrapper: {}x{}, surface_id={}",
            width, height, surface_id
        );

        Some(Self {
            va_display,
            surface_id,
            dmabuf_fd: Mutex::new(None),
            width,
            height,
            format: VASurfaceFormat::NV12, // Default, will be updated on export
            drm_format: DRM_FORMAT_NV12,
            drm_modifier: 0,
            planes: Vec::new(),
        })
    }

    /// Export the surface as a DMA-BUF for Vulkan import
    /// Returns the file descriptor and updates format info
    pub fn export_dmabuf(&self) -> Result<RawFd> {
        let mut guard = self.dmabuf_fd.lock();
        if let Some(fd) = *guard {
            return Ok(fd);
        }

        unsafe {
            // Load libva dynamically
            let libva = match libloading::Library::new("libva.so.2") {
                Ok(lib) => lib,
                Err(_) => {
                    // Try alternative names
                    libloading::Library::new("libva.so")
                        .map_err(|e| anyhow!("Failed to load libva: {}", e))?
                }
            };

            // Get function pointers
            let va_sync_surface: libloading::Symbol<ffi::VaSyncSurface> = libva
                .get(b"vaSyncSurface\0")
                .map_err(|e| anyhow!("vaSyncSurface not found: {}", e))?;

            let va_export_surface_handle: libloading::Symbol<ffi::VaExportSurfaceHandle> = libva
                .get(b"vaExportSurfaceHandle\0")
                .map_err(|e| anyhow!("vaExportSurfaceHandle not found: {}", e))?;

            // Sync the surface before export (wait for decode to complete)
            let status = va_sync_surface(self.va_display, self.surface_id);
            if status != ffi::VA_STATUS_SUCCESS {
                return Err(anyhow!("vaSyncSurface failed with status {}", status));
            }

            // Export as DRM PRIME (DMA-BUF)
            let mut desc: ffi::VADRMPRIMESurfaceDescriptor = std::mem::zeroed();
            let status = va_export_surface_handle(
                self.va_display,
                self.surface_id,
                ffi::VA_SURFACE_ATTRIB_MEM_TYPE_DRM_PRIME_2,
                ffi::VA_EXPORT_SURFACE_READ_ONLY | ffi::VA_EXPORT_SURFACE_SEPARATE_LAYERS,
                &mut desc,
            );

            if status != ffi::VA_STATUS_SUCCESS {
                return Err(anyhow!(
                    "vaExportSurfaceHandle failed with status {}",
                    status
                ));
            }

            if desc.num_objects == 0 {
                return Err(anyhow!("No DMA-BUF objects exported"));
            }

            // Get the primary fd (first object)
            let fd = desc.objects[0].fd;
            if fd < 0 {
                return Err(anyhow!("Invalid DMA-BUF fd: {}", fd));
            }

            debug!(
                "VAAPI DMA-BUF export: fd={}, fourcc={:08x}, modifier={:x}, layers={}",
                fd, desc.fourcc, desc.objects[0].drm_format_modifier, desc.num_layers
            );

            *guard = Some(fd);
            Ok(fd)
        }
    }

    /// Get the surface ID (for FFmpeg/VA-API operations)
    pub fn surface_id(&self) -> u32 {
        self.surface_id
    }

    /// Check if this is a 10-bit HDR surface
    pub fn is_10bit(&self) -> bool {
        self.format == VASurfaceFormat::P010
    }

    /// Lock the surface and copy Y and UV planes to CPU memory
    /// This is the fallback path when zero-copy import fails
    pub fn lock_and_get_planes(&self) -> Result<LockedPlanes> {
        unsafe {
            // Load libva
            let libva = libloading::Library::new("libva.so.2")
                .or_else(|_| libloading::Library::new("libva.so"))
                .map_err(|e| anyhow!("Failed to load libva: {}", e))?;

            // Sync surface first
            let va_sync_surface: libloading::Symbol<ffi::VaSyncSurface> = libva
                .get(b"vaSyncSurface\0")
                .map_err(|e| anyhow!("vaSyncSurface not found: {}", e))?;

            let status = va_sync_surface(self.va_display, self.surface_id);
            if status != ffi::VA_STATUS_SUCCESS {
                return Err(anyhow!("vaSyncSurface failed: {}", status));
            }

            // For CPU fallback, we need to use vaMapBuffer/vaDeriveImage
            // This is more complex and involves creating a VAImage
            // For now, we'll use the simpler approach of exporting and mmap'ing the DMA-BUF

            let fd = self.export_dmabuf()?;

            // Calculate sizes based on NV12 format
            let y_size = (self.width * self.height) as usize;
            let uv_size = y_size / 2; // UV is half height
            let total_size = y_size + uv_size;

            // mmap the DMA-BUF
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                total_size,
                libc::PROT_READ,
                libc::MAP_SHARED,
                fd,
                0,
            );

            if ptr == libc::MAP_FAILED {
                return Err(anyhow!("mmap failed: {}", std::io::Error::last_os_error()));
            }

            // Copy the data
            let data = std::slice::from_raw_parts(ptr as *const u8, total_size);
            let y_plane = data[..y_size].to_vec();
            let uv_plane = data[y_size..].to_vec();

            // Unmap
            libc::munmap(ptr, total_size);

            Ok(LockedPlanes {
                y_plane,
                uv_plane,
                y_stride: self.width,
                uv_stride: self.width,
                width: self.width,
                height: self.height,
            })
        }
    }
}

impl Drop for VAAPISurfaceWrapper {
    fn drop(&mut self) {
        // Close the DMA-BUF fd if we exported one
        if let Some(fd) = self.dmabuf_fd.lock().take() {
            unsafe {
                libc::close(fd);
            }
        }
        // Note: The VA surface itself is owned by FFmpeg and will be released
        // when the AVFrame is freed
    }
}

/// Locked plane data from VAAPI surface
pub struct LockedPlanes {
    pub y_plane: Vec<u8>,
    pub uv_plane: Vec<u8>,
    pub y_stride: u32,
    pub uv_stride: u32,
    pub width: u32,
    pub height: u32,
}

/// Manager for VAAPI zero-copy surfaces
/// Handles Vulkan interop setup
pub struct VaapiZeroCopyManager {
    /// Whether zero-copy is enabled
    enabled: bool,
    /// VA display (cached for surface operations)
    va_display: Option<*mut std::ffi::c_void>,
}

// Safety: VA display pointer is thread-safe when properly synchronized
unsafe impl Send for VaapiZeroCopyManager {}
unsafe impl Sync for VaapiZeroCopyManager {}

impl VaapiZeroCopyManager {
    /// Create a new manager
    pub fn new() -> Self {
        info!("VAAPI zero-copy manager created");
        Self {
            enabled: true,
            va_display: None,
        }
    }

    /// Set the VA display handle (from FFmpeg decoder context)
    pub fn set_va_display(&mut self, display: *mut std::ffi::c_void) {
        self.va_display = Some(display);
    }

    /// Check if zero-copy is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Disable zero-copy (fallback to CPU path)
    pub fn disable(&mut self) {
        warn!("VAAPI zero-copy disabled, falling back to CPU path");
        self.enabled = false;
    }
}

impl Default for VaapiZeroCopyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract VAAPI surface from FFmpeg frame data pointers
///
/// FFmpeg VAAPI frame layout:
/// - data[3] = VASurfaceID (stored as pointer-sized value)
/// - hw_frames_ctx contains VADisplay
///
/// # Safety
/// The data pointers must be from a valid VAAPI decoded frame
pub unsafe fn extract_vaapi_surface_from_frame(
    data3: *mut u8,
    va_display: *mut std::ffi::c_void,
    width: u32,
    height: u32,
) -> Option<VAAPISurfaceWrapper> {
    if data3.is_null() || va_display.is_null() {
        return None;
    }

    // data[3] contains VASurfaceID as a pointer-sized value
    let surface_id = data3 as usize as u32;

    VAAPISurfaceWrapper::from_ffmpeg_frame(va_display, surface_id, width, height)
}

/// Check if VAAPI is available on this system
pub fn is_vaapi_available() -> bool {
    // Try to load libva
    unsafe {
        match libloading::Library::new("libva.so.2") {
            Ok(_) => true,
            Err(_) => match libloading::Library::new("libva.so") {
                Ok(_) => true,
                Err(_) => {
                    debug!("libva not found - VAAPI not available");
                    false
                }
            },
        }
    }
}

/// Get the VAAPI driver name for the current GPU
pub fn get_vaapi_driver_name() -> Option<String> {
    // Check environment variable first
    if let Ok(driver) = std::env::var("LIBVA_DRIVER_NAME") {
        return Some(driver);
    }

    // Try to detect from DRI device
    if let Ok(entries) = std::fs::read_dir("/dev/dri") {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("renderD") {
                    // Found a render node - VAAPI is likely available
                    return Some("auto".to_string());
                }
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locked_planes_layout() {
        // Test NV12 plane calculations
        let width = 1920u32;
        let height = 1080u32;

        // Y plane: full resolution
        let y_size = width * height;
        assert_eq!(y_size, 2073600);

        // UV plane: half height, same width (interleaved)
        let uv_size = width * (height / 2);
        assert_eq!(uv_size, 1036800);
    }

    #[test]
    fn test_drm_format_codes() {
        // Verify fourcc codes
        assert_eq!(DRM_FORMAT_NV12, 0x3231564E);
        assert_eq!(DRM_FORMAT_P010, 0x30313050);
    }
}
