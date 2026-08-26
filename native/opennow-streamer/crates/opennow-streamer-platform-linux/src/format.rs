use std::fmt;
use std::os::fd::RawFd;
use std::sync::Arc;

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VideoCodec {
    H264,
    H265,
    Av1,
}

impl VideoCodec {
    pub const fn label(self) -> &'static str {
        match self {
            Self::H264 => "h264",
            Self::H265 => "h265",
            Self::Av1 => "av1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PixelFormat {
    Nv12,
    I420,
    Bgra8,
    Rgba8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorRange {
    Limited,
    Full,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMatrix {
    Bt601,
    Bt709,
    Bt2020,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChromaLocation {
    Left,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamFormat {
    pub width: u32,
    pub height: u32,
    pub pixel_format: PixelFormat,
    pub color_range: ColorRange,
    pub color_matrix: ColorMatrix,
    pub chroma_location: ChromaLocation,
}

impl StreamFormat {
    pub fn video_default(width: u32, height: u32) -> Result<Self> {
        let format = Self {
            width,
            height,
            pixel_format: PixelFormat::Nv12,
            color_range: ColorRange::Limited,
            color_matrix: if height > 576 {
                ColorMatrix::Bt709
            } else {
                ColorMatrix::Bt601
            },
            chroma_location: ChromaLocation::Left,
        };
        format.validate()?;
        Ok(format)
    }

    pub fn h264_default(width: u32, height: u32) -> Result<Self> {
        Self::video_default(width, height)
    }

    pub fn validate(&self) -> Result<()> {
        if self.width == 0 || self.height == 0 {
            return Err(Error::InvalidFormat(
                "video dimensions must be non-zero".to_owned(),
            ));
        }
        if self.width > 16_384 || self.height > 16_384 {
            return Err(Error::InvalidFormat(
                "video dimensions exceed the Linux backend limit".to_owned(),
            ));
        }
        if matches!(self.pixel_format, PixelFormat::Nv12 | PixelFormat::I420)
            && (self.width % 2 != 0 || self.height % 2 != 0)
        {
            return Err(Error::InvalidFormat(
                "4:2:0 video dimensions must be even".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct EncodedVideoFrame {
    pub data: Arc<[u8]>,
    pub timestamp_us: u64,
    pub keyframe: bool,
}

impl EncodedVideoFrame {
    pub fn new(data: impl Into<Arc<[u8]>>, timestamp_us: u64, keyframe: bool) -> Result<Self> {
        let data = data.into();
        let frame = Self {
            data,
            timestamp_us,
            keyframe,
        };
        frame.validate()?;
        Ok(frame)
    }

    pub fn validate(&self) -> Result<()> {
        if self.data.is_empty() {
            return Err(Error::InvalidFormat(
                "video access unit is empty".to_owned(),
            ));
        }
        if self.data.len() > 16 * 1024 * 1024 {
            return Err(Error::InvalidFormat(
                "video access unit exceeds 16 MiB".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct FramePlane {
    pub data: Arc<[u8]>,
    pub stride: usize,
    pub rows: usize,
}

#[derive(Debug, Clone)]
pub struct FrameOverlay {
    pub origin_x: u32,
    pub origin_y: u32,
    pub width: u32,
    pub height: u32,
    pub luma: FramePlane,
    pub chroma: FramePlane,
}

impl FramePlane {
    pub fn validate(&self, minimum_row_bytes: usize) -> Result<()> {
        if self.stride < minimum_row_bytes {
            return Err(Error::InvalidFormat(format!(
                "plane stride {} is less than the required {minimum_row_bytes}",
                self.stride
            )));
        }
        let required = self
            .stride
            .checked_mul(self.rows)
            .ok_or_else(|| Error::InvalidFormat("plane size overflow".to_owned()))?;
        if self.data.len() < required {
            return Err(Error::InvalidFormat(format!(
                "plane has {} bytes but requires {required}",
                self.data.len()
            )));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaBufObject {
    pub fd: RawFd,
    pub size: usize,
    pub format_modifier: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DmaBufPlane {
    pub object_index: usize,
    pub offset: usize,
    pub pitch: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DmaBufLayer {
    pub format: u32,
    pub planes: Vec<DmaBufPlane>,
}

/// A decoded hardware frame exported through DRM PRIME. `owner` retains the
/// FFmpeg/VAAPI frame and therefore the dma-buf file descriptors until the
/// Vulkan submission that samples them has completed.
pub struct DmaBufFrame {
    pub objects: Vec<DmaBufObject>,
    pub layers: Vec<DmaBufLayer>,
    owner: Arc<dyn Send + Sync>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VulkanImage {
    pub image: u64,
    pub format: i32,
    pub width: u32,
    pub height: u32,
    pub layout: i32,
    pub access: u64,
    pub semaphore: u64,
    pub semaphore_value: u64,
    pub queue_family: u32,
}

/// A Vulkan Video output surface on the decoder's own Vulkan device. The
/// presenter adopts this device and samples these image handles directly.
pub struct VulkanVideoFrame {
    pub instance: usize,
    pub physical_device: usize,
    pub device: usize,
    pub queue_families: Vec<u32>,
    pub image_usage: u32,
    pub image_flags: u32,
    pub images: Vec<VulkanImage>,
    device_context: usize,
    lock_queue: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32)>,
    unlock_queue: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32)>,
    owner: Arc<dyn Send + Sync>,
}

impl VulkanVideoFrame {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        instance: usize,
        physical_device: usize,
        device: usize,
        queue_families: Vec<u32>,
        image_usage: u32,
        image_flags: u32,
        images: Vec<VulkanImage>,
        device_context: usize,
        lock_queue: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32)>,
        unlock_queue: Option<unsafe extern "C" fn(*mut std::ffi::c_void, u32, u32)>,
        owner: Arc<dyn Send + Sync>,
    ) -> Self {
        Self {
            instance,
            physical_device,
            device,
            queue_families,
            image_usage,
            image_flags,
            images,
            device_context,
            lock_queue,
            unlock_queue,
            owner,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.instance == 0
            || self.physical_device == 0
            || self.device == 0
            || self.images.is_empty()
            || self.images.len() > 2
            || self.queue_families.is_empty()
            || self.images.iter().any(|image| image.image == 0)
        {
            return Err(Error::InvalidFormat(
                "Vulkan Video frame has invalid device or image handles".to_owned(),
            ));
        }
        Ok(())
    }

    pub fn retain_owner(&self) -> &Arc<dyn Send + Sync> {
        &self.owner
    }

    pub(crate) fn lock_presentation_queue(&self, family: u32) {
        if let Some(lock) = self.lock_queue {
            unsafe { lock(self.device_context as *mut std::ffi::c_void, family, 0) };
        }
    }

    pub(crate) fn unlock_presentation_queue(&self, family: u32) {
        if let Some(unlock) = self.unlock_queue {
            unsafe { unlock(self.device_context as *mut std::ffi::c_void, family, 0) };
        }
    }
}

impl fmt::Debug for VulkanVideoFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VulkanVideoFrame")
            .field("instance", &self.instance)
            .field("physical_device", &self.physical_device)
            .field("device", &self.device)
            .field("queue_families", &self.queue_families)
            .field("images", &self.images)
            .finish_non_exhaustive()
    }
}

impl DmaBufFrame {
    pub fn new(
        objects: Vec<DmaBufObject>,
        layers: Vec<DmaBufLayer>,
        owner: Arc<dyn Send + Sync>,
    ) -> Self {
        Self {
            objects,
            layers,
            owner,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.objects.is_empty() || self.layers.is_empty() {
            return Err(Error::InvalidFormat(
                "DMA-BUF frame has no objects or layers".to_owned(),
            ));
        }
        for object in &self.objects {
            if object.fd < 0 || object.size == 0 {
                return Err(Error::InvalidFormat(
                    "DMA-BUF frame has an invalid object".to_owned(),
                ));
            }
        }
        for layer in &self.layers {
            if layer.planes.is_empty()
                || layer
                    .planes
                    .iter()
                    .any(|plane| plane.object_index >= self.objects.len() || plane.pitch == 0)
            {
                return Err(Error::InvalidFormat(
                    "DMA-BUF frame has an invalid layer".to_owned(),
                ));
            }
        }
        Ok(())
    }

    pub fn retain_owner(&self) -> &Arc<dyn Send + Sync> {
        &self.owner
    }
}

impl fmt::Debug for DmaBufFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DmaBufFrame")
            .field("objects", &self.objects)
            .field("layers", &self.layers)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct DecodedVideoFrame {
    pub format: StreamFormat,
    pub planes: Vec<FramePlane>,
    pub dmabuf: Option<Arc<DmaBufFrame>>,
    pub vulkan: Option<Arc<VulkanVideoFrame>>,
    pub overlay: Option<FrameOverlay>,
    pub timestamp_us: u64,
}

impl DecodedVideoFrame {
    pub fn validate(&self) -> Result<()> {
        self.format.validate()?;
        if let Some(overlay) = &self.overlay {
            if overlay.width == 0
                || overlay.height == 0
                || overlay.width % 2 != 0
                || overlay.height % 2 != 0
                || overlay.origin_x.saturating_add(overlay.width) > self.format.width
                || overlay.origin_y.saturating_add(overlay.height) > self.format.height
            {
                return Err(Error::InvalidFormat(
                    "frame overlay has invalid bounds".to_owned(),
                ));
            }
            overlay.luma.validate(overlay.width as usize)?;
            overlay.chroma.validate(overlay.width as usize)?;
        }
        if let Some(vulkan) = &self.vulkan {
            vulkan.validate()?;
            return Ok(());
        }
        if let Some(dmabuf) = &self.dmabuf {
            dmabuf.validate()?;
            return Ok(());
        }
        let width = self.format.width as usize;
        let height = self.format.height as usize;
        match self.format.pixel_format {
            PixelFormat::Nv12 => {
                if self.planes.len() != 2 {
                    return Err(Error::InvalidFormat(
                        "NV12 frames require two planes".to_owned(),
                    ));
                }
                self.planes[0].validate(width)?;
                self.planes[1].validate(width)?;
                if self.planes[0].rows < height || self.planes[1].rows < height / 2 {
                    return Err(Error::InvalidFormat(
                        "NV12 plane height is too small".to_owned(),
                    ));
                }
            }
            PixelFormat::I420 => {
                if self.planes.len() != 3 {
                    return Err(Error::InvalidFormat(
                        "I420 frames require three planes".to_owned(),
                    ));
                }
                self.planes[0].validate(width)?;
                self.planes[1].validate(width / 2)?;
                self.planes[2].validate(width / 2)?;
                if self.planes[0].rows < height
                    || self.planes[1].rows < height / 2
                    || self.planes[2].rows < height / 2
                {
                    return Err(Error::InvalidFormat(
                        "I420 plane height is too small".to_owned(),
                    ));
                }
            }
            PixelFormat::Bgra8 | PixelFormat::Rgba8 => {
                if self.planes.len() != 1 {
                    return Err(Error::InvalidFormat(
                        "packed RGB frames require one plane".to_owned(),
                    ));
                }
                self.planes[0].validate(width * 4)?;
                if self.planes[0].rows < height {
                    return Err(Error::InvalidFormat(
                        "packed RGB plane height is too small".to_owned(),
                    ));
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_padded_nv12_planes() {
        let frame = DecodedVideoFrame {
            format: StreamFormat::h264_default(4, 4).unwrap(),
            planes: vec![
                FramePlane {
                    data: Arc::from(vec![0_u8; 32]),
                    stride: 8,
                    rows: 4,
                },
                FramePlane {
                    data: Arc::from(vec![0_u8; 16]),
                    stride: 8,
                    rows: 2,
                },
            ],
            dmabuf: None,
            vulkan: None,
            overlay: None,
            timestamp_us: 1,
        };
        frame.validate().unwrap();
    }

    #[test]
    fn rejects_odd_420_dimensions_and_short_planes() {
        assert!(StreamFormat::h264_default(1919, 1080).is_err());
        let frame = DecodedVideoFrame {
            format: StreamFormat::h264_default(4, 4).unwrap(),
            planes: vec![
                FramePlane {
                    data: Arc::from(vec![0_u8; 15]),
                    stride: 4,
                    rows: 4,
                },
                FramePlane {
                    data: Arc::from(vec![0_u8; 8]),
                    stride: 4,
                    rows: 2,
                },
            ],
            dmabuf: None,
            vulkan: None,
            overlay: None,
            timestamp_us: 1,
        };
        assert!(frame.validate().is_err());
    }
}
