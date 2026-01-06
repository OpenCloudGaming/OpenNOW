//! V4L2 Video Decoder Support for Linux (Raspberry Pi)
//!
//! This module provides hardware video decoding on Raspberry Pi using V4L2 M2M
//! (Memory-to-Memory) stateful codec interface.
//!
//! Supported hardware:
//! - Raspberry Pi 4: H.264 decode via bcm2835-codec
//! - Raspberry Pi 5: H.264 and HEVC decode via rpivid (stateless, limited FFmpeg support)
//!
//! Note: Pi 5's HEVC decoder uses stateless API which requires special handling.
//! For best compatibility, H.264 is recommended on Raspberry Pi.
//!
//! Flow:
//! 1. FFmpeg v4l2m2m decodes to DMA-BUF backed buffer
//! 2. We extract the DMA-BUF fd from the V4L2 buffer
//! 3. Import into Vulkan/GL via EGL_EXT_image_dma_buf_import
//! 4. Render via wgpu
//!
//! Fallback: If zero-copy fails, we mmap the buffer and copy to CPU memory.

use anyhow::{anyhow, Result};
use log::{debug, info, warn};
use std::os::unix::io::RawFd;
use std::path::Path;

/// V4L2 buffer wrapper from FFmpeg hardware decoder
pub struct V4L2BufferWrapper {
    /// DMA-BUF file descriptor
    dmabuf_fd: RawFd,
    /// Buffer dimensions
    pub width: u32,
    pub height: u32,
    /// Pixel format (NV12 for Pi decoders)
    pub format: V4L2PixelFormat,
    /// Whether we own the fd (should close on drop)
    owns_fd: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum V4L2PixelFormat {
    NV12,
    NV21,
    YUV420,
    Unknown,
}

// Safety: DMA-BUF fds can be shared across threads
unsafe impl Send for V4L2BufferWrapper {}
unsafe impl Sync for V4L2BufferWrapper {}

impl std::fmt::Debug for V4L2BufferWrapper {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("V4L2BufferWrapper")
            .field("dmabuf_fd", &self.dmabuf_fd)
            .field("width", &self.width)
            .field("height", &self.height)
            .field("format", &self.format)
            .finish()
    }
}

impl V4L2BufferWrapper {
    /// Create a wrapper from a DMA-BUF fd
    pub fn new(dmabuf_fd: RawFd, width: u32, height: u32, format: V4L2PixelFormat) -> Self {
        Self {
            dmabuf_fd,
            width,
            height,
            format,
            owns_fd: false, // FFmpeg owns the fd
        }
    }

    /// Get the DMA-BUF fd for import
    pub fn dmabuf_fd(&self) -> RawFd {
        self.dmabuf_fd
    }

    /// Lock the buffer and copy planes to CPU memory (fallback path)
    pub fn lock_and_get_planes(&self) -> Result<LockedPlanes> {
        unsafe {
            // Calculate sizes based on NV12 format
            let y_size = (self.width * self.height) as usize;
            let uv_size = y_size / 2;
            let total_size = y_size + uv_size;

            // mmap the DMA-BUF
            let ptr = libc::mmap(
                std::ptr::null_mut(),
                total_size,
                libc::PROT_READ,
                libc::MAP_SHARED,
                self.dmabuf_fd,
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

impl Drop for V4L2BufferWrapper {
    fn drop(&mut self) {
        if self.owns_fd && self.dmabuf_fd >= 0 {
            unsafe {
                libc::close(self.dmabuf_fd);
            }
        }
    }
}

/// Locked plane data from V4L2 buffer
pub struct LockedPlanes {
    pub y_plane: Vec<u8>,
    pub uv_plane: Vec<u8>,
    pub y_stride: u32,
    pub uv_stride: u32,
    pub width: u32,
    pub height: u32,
}

/// Detect if running on Raspberry Pi
pub fn is_raspberry_pi() -> bool {
    // Check for Pi-specific indicators
    if Path::new("/sys/firmware/devicetree/base/model").exists() {
        if let Ok(model) = std::fs::read_to_string("/sys/firmware/devicetree/base/model") {
            let model_lower = model.to_lowercase();
            return model_lower.contains("raspberry pi");
        }
    }

    // Check for bcm2835/bcm2711/bcm2712 in /proc/cpuinfo
    if let Ok(cpuinfo) = std::fs::read_to_string("/proc/cpuinfo") {
        let cpuinfo_lower = cpuinfo.to_lowercase();
        return cpuinfo_lower.contains("bcm2835")
            || cpuinfo_lower.contains("bcm2711")
            || cpuinfo_lower.contains("bcm2712");
    }

    false
}

/// Get Raspberry Pi model (4, 5, etc.)
pub fn get_pi_model() -> Option<u8> {
    if let Ok(model) = std::fs::read_to_string("/sys/firmware/devicetree/base/model") {
        if model.contains("Raspberry Pi 5") {
            return Some(5);
        } else if model.contains("Raspberry Pi 4") {
            return Some(4);
        } else if model.contains("Raspberry Pi 3") {
            return Some(3);
        } else if model.contains("Raspberry Pi") {
            return Some(0); // Unknown Pi version
        }
    }
    None
}

/// Find the V4L2 M2M decoder device for the given codec
pub fn find_v4l2_decoder_device(codec: V4L2Codec) -> Option<String> {
    // Common V4L2 M2M device paths on Raspberry Pi
    let search_paths = match codec {
        V4L2Codec::H264 => vec![
            "/dev/video10", // bcm2835-codec on Pi 4
            "/dev/video11",
            "/dev/video19", // rpivid on Pi 5
        ],
        V4L2Codec::HEVC => vec![
            "/dev/video19", // rpivid HEVC on Pi 5
            "/dev/video10",
        ],
    };

    for path in search_paths {
        if Path::new(path).exists() {
            // Try to query the device capabilities
            if let Ok(file) = std::fs::File::open(path) {
                use std::os::unix::io::AsRawFd;
                let fd = file.as_raw_fd();

                // Query V4L2 capabilities (simplified check)
                if query_v4l2_caps(fd, codec) {
                    info!("Found V4L2 M2M decoder for {:?} at {}", codec, path);
                    return Some(path.to_string());
                }
            }
        }
    }

    None
}

#[derive(Debug, Clone, Copy)]
pub enum V4L2Codec {
    H264,
    HEVC,
}

/// Query V4L2 device capabilities (simplified)
fn query_v4l2_caps(fd: RawFd, _codec: V4L2Codec) -> bool {
    // V4L2 ioctl numbers
    const VIDIOC_QUERYCAP: libc::c_ulong = 0x80685600;

    #[repr(C)]
    struct v4l2_capability {
        driver: [u8; 16],
        card: [u8; 32],
        bus_info: [u8; 32],
        version: u32,
        capabilities: u32,
        device_caps: u32,
        reserved: [u32; 3],
    }

    const V4L2_CAP_VIDEO_M2M_MPLANE: u32 = 0x00004000;
    const V4L2_CAP_VIDEO_M2M: u32 = 0x00008000;

    unsafe {
        let mut caps: v4l2_capability = std::mem::zeroed();
        let ret = libc::ioctl(fd, VIDIOC_QUERYCAP, &mut caps);

        if ret < 0 {
            return false;
        }

        let device_caps = if caps.device_caps != 0 {
            caps.device_caps
        } else {
            caps.capabilities
        };

        // Check for M2M capability
        let is_m2m = (device_caps & V4L2_CAP_VIDEO_M2M) != 0
            || (device_caps & V4L2_CAP_VIDEO_M2M_MPLANE) != 0;

        if is_m2m {
            let driver = String::from_utf8_lossy(&caps.driver);
            let card = String::from_utf8_lossy(&caps.card);
            debug!(
                "V4L2 device: driver={}, card={}",
                driver.trim_end_matches('\0'),
                card.trim_end_matches('\0')
            );
            return true;
        }
    }

    false
}

/// Manager for V4L2 zero-copy buffers
pub struct V4L2ZeroCopyManager {
    enabled: bool,
    pi_model: Option<u8>,
}

impl V4L2ZeroCopyManager {
    pub fn new() -> Self {
        let pi_model = get_pi_model();
        if let Some(model) = pi_model {
            info!(
                "Raspberry Pi {} detected - V4L2 hardware decoding available",
                model
            );
        }

        Self {
            enabled: pi_model.is_some(),
            pi_model,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn pi_model(&self) -> Option<u8> {
        self.pi_model
    }

    pub fn disable(&mut self) {
        warn!("V4L2 zero-copy disabled, falling back to CPU path");
        self.enabled = false;
    }
}

impl Default for V4L2ZeroCopyManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Check if V4L2 M2M decoding is available for the given codec
pub fn is_v4l2_available(codec: V4L2Codec) -> bool {
    if !is_raspberry_pi() {
        return false;
    }

    find_v4l2_decoder_device(codec).is_some()
}

/// Get recommended video codec for this Raspberry Pi
pub fn get_recommended_codec() -> Option<V4L2Codec> {
    match get_pi_model() {
        Some(5) => {
            // Pi 5 has HEVC hardware decoder (stateless via rpivid)
            // But H.264 is more reliable with FFmpeg
            if is_v4l2_available(V4L2Codec::HEVC) {
                Some(V4L2Codec::HEVC)
            } else if is_v4l2_available(V4L2Codec::H264) {
                Some(V4L2Codec::H264)
            } else {
                None
            }
        }
        Some(4) | Some(3) => {
            // Pi 4/3 only have H.264 hardware decoder
            if is_v4l2_available(V4L2Codec::H264) {
                Some(V4L2Codec::H264)
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pi_detection() {
        // This will only pass on actual Pi hardware
        let is_pi = is_raspberry_pi();
        println!("Is Raspberry Pi: {}", is_pi);

        if is_pi {
            println!("Pi Model: {:?}", get_pi_model());
            println!("H264 available: {}", is_v4l2_available(V4L2Codec::H264));
            println!("HEVC available: {}", is_v4l2_available(V4L2Codec::HEVC));
        }
    }
}
