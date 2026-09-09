use std::collections::{HashMap, hash_map::Entry as HashMapEntry};
use std::io::Cursor;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use ash::vk::{self, Handle};

mod sand;

use crate::{
    DecodedVideoFrame, DmaBufFrame, DmaBufPlane, Error, FramePlane, PixelFormat, Result, Subsystem,
    VulkanVideoFrame,
};

const DRM_FORMAT_NV12: u32 = u32::from_le_bytes(*b"NV12");
const DRM_FORMAT_P010: u32 = u32::from_le_bytes(*b"P010");
const DRM_FORMAT_R16: u32 = u32::from_le_bytes(*b"R16 ");
const DRM_FORMAT_GR1616: u32 = u32::from_le_bytes(*b"GR32");
const DRM_FORMAT_R8: u32 = u32::from_le_bytes(*b"R8  ");
const DRM_FORMAT_GR88: u32 = u32::from_le_bytes(*b"GR88");
const DECODE_WAIT_TIMEOUT: Duration = Duration::from_secs(1);
const NV12_VERTEX_SHADER: &[u8] = include_bytes!("../shaders/nv12.vert.spv");
const NV12_FRAGMENT_SHADER: &[u8] = include_bytes!("../shaders/embedded_yuv.frag.spv");
const DEFAULT_FRAME_SLOTS: u32 = 3;
const MAX_FRAME_SLOTS: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VulkanRenderDevice {
    pub instance: usize,
    pub physical_device: usize,
    pub device: usize,
    pub queue: usize,
    pub queue_family: u32,
    pub dmabuf_import_enabled: bool,
    pub dmabuf_buffer_import_enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinuxTextureColorSpace {
    Sdr709,
    Pq2020,
    Hlg2020,
}

impl LinuxTextureColorSpace {
    fn from_format(format: crate::StreamFormat) -> Result<Self> {
        format.validate()?;
        match (format.color_transfer, format.color_primaries) {
            (crate::ColorTransfer::Sdr, crate::ColorPrimaries::Bt709) => Ok(Self::Sdr709),
            (crate::ColorTransfer::Pq, crate::ColorPrimaries::Bt2020) => Ok(Self::Pq2020),
            (crate::ColorTransfer::Hlg, crate::ColorPrimaries::Bt2020) => Ok(Self::Hlg2020),
            _ => Err(Error::unavailable(
                Subsystem::Vulkan,
                "unsupported source transfer/primaries combination",
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordedGpuFrame {
    pub slot: u32,
    pub generation: u64,
    pub image: u64,
    pub image_view: u64,
    pub texture_format: GpuTextureFormat,
    pub width: u32,
    pub height: u32,
    pub timestamp_us: u64,
    pub color_space: LinuxTextureColorSpace,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GpuTextureFormat {
    Rgba8,
    Rgb10A2,
    Rgba16Float,
}

impl GpuTextureFormat {
    fn for_stream_format(format: crate::StreamFormat) -> Result<Self> {
        match LinuxTextureColorSpace::from_format(format)? {
            LinuxTextureColorSpace::Sdr709 => Self::for_pixel_format(format.pixel_format),
            LinuxTextureColorSpace::Pq2020 | LinuxTextureColorSpace::Hlg2020 => {
                Ok(Self::Rgba16Float)
            }
        }
    }

    fn for_pixel_format(format: PixelFormat) -> Result<Self> {
        match format {
            PixelFormat::Nv12 | PixelFormat::Nv24 => Ok(Self::Rgba8),
            PixelFormat::P010 | PixelFormat::P410 => Ok(Self::Rgb10A2),
            _ => Err(Error::InvalidFormat(format!(
                "unsupported embedded conversion source format {format:?}"
            ))),
        }
    }

    fn vulkan_format(self) -> vk::Format {
        match self {
            Self::Rgba8 => vk::Format::R8G8B8A8_UNORM,
            Self::Rgb10A2 => vk::Format::A2B10G10R10_UNORM_PACK32,
            Self::Rgba16Float => vk::Format::R16G16B16A16_SFLOAT,
        }
    }

    fn validate_features(self, features: vk::FormatFeatureFlags) -> Result<()> {
        let required = vk::FormatFeatureFlags::COLOR_ATTACHMENT
            | vk::FormatFeatureFlags::SAMPLED_IMAGE
            | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
        if !features.contains(required) {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                format!(
                    "embedded {self:?} output ({:?}) requires optimal-tiling color attachment and linearly filtered sampling support; available features: {features:?}",
                    self.vulkan_format()
                ),
            ));
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct LinuxGpuFrameProducer {
    render_resources: Arc<LinuxGpuRenderResources>,
    decode_sync: Arc<Mutex<Option<DecodeSync>>>,
    sequence: Arc<AtomicU64>,
    slot_count: u32,
}

pub struct LinuxGpuRenderResources {
    state: Mutex<SharedProducerState>,
}

impl LinuxGpuRenderResources {
    /// Retires borrowed-device resources on the render thread after the host has
    /// submitted all command buffers referencing them, before destroying its device.
    pub fn retire(&self) -> Result<()> {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut retirement_error = None;
        if let Some(producer) = state.producer.as_ref() {
            let result = unsafe {
                producer
                    .device
                    .queue_wait_idle(vk::Queue::from_raw(producer.render.queue as u64))
            };
            if let Err(error) = result {
                if error != vk::Result::ERROR_DEVICE_LOST {
                    retirement_error = Some(vk_error("retire Qt Vulkan resources", error));
                }
            }
        }
        if let Some(error) = retirement_error {
            if let Some(producer) = state.producer.take() {
                std::mem::forget(producer);
            }
            state.render = None;
            return Err(error);
        }
        state.producer = None;
        state.render = None;
        Ok(())
    }
}

struct SharedProducerState {
    render: Option<VulkanRenderDevice>,
    producer: Option<LinuxFrameProducer>,
}

// Used only by the decoded-frame publisher, never Qt's render thread. Cache the
// dispatch table rather than loading Vulkan functions for every decoded frame.
struct DecodeSync {
    _entry: ash::Entry,
    identity: (usize, usize),
    device: ash::Device,
}
impl DecodeSync {
    fn new(frame: &VulkanVideoFrame) -> Result<Self> {
        let entry = unsafe { ash::Entry::load() }
            .map_err(|error| Error::backend(Subsystem::Vulkan, error.to_string()))?;
        let instance = unsafe {
            ash::Instance::load(
                entry.static_fn(),
                vk::Instance::from_raw(frame.instance as u64),
            )
        };
        let device = unsafe {
            ash::Device::load(
                instance.fp_v1_0(),
                vk::Device::from_raw(frame.device as u64),
            )
        };
        Ok(Self {
            _entry: entry,
            identity: (frame.instance, frame.device),
            device,
        })
    }
    fn wait(&self, frame: &VulkanVideoFrame) -> Result<()> {
        let (semaphores, values): (Vec<_>, Vec<_>) = timeline_waits(frame)?
            .into_iter()
            .map(|(semaphore, value)| (vk::Semaphore::from_raw(semaphore), value))
            .unzip();
        if semaphores.is_empty() {
            return Ok(());
        }
        let wait = vk::SemaphoreWaitInfo::default()
            .semaphores(&semaphores)
            .values(&values);
        unsafe {
            self.device
                .wait_semaphores(&wait, DECODE_WAIT_TIMEOUT.as_nanos() as u64)
        }
        .map_err(|error| vk_error("publisher wait for Vulkan decoder frame", error))
    }
}

pub struct LinuxGpuFrame {
    frame: DecodedVideoFrame,
    producer: LinuxGpuFrameProducer,
    sequence: u64,
}

impl LinuxGpuFrameProducer {
    pub fn new(slot_count: u32) -> Result<Self> {
        if slot_count == 0 || slot_count > MAX_FRAME_SLOTS {
            return Err(Error::InvalidFormat(
                "embedded Vulkan frame slot count must be between 1 and 8".to_owned(),
            ));
        }
        Ok(Self {
            render_resources: Arc::new(LinuxGpuRenderResources {
                state: Mutex::new(SharedProducerState {
                    render: None,
                    producer: None,
                }),
            }),
            sequence: Arc::new(AtomicU64::new(0)),
            decode_sync: Arc::new(Mutex::new(None)),
            slot_count,
        })
    }

    pub fn frame(&self, frame: DecodedVideoFrame) -> Result<LinuxGpuFrame> {
        frame.validate()?;
        if let Some(dmabuf) = frame.dmabuf.as_ref().filter(|frame| sand::is_sand(frame)) {
            sand::wait_ready(dmabuf, DECODE_WAIT_TIMEOUT)?;
        }
        // This method runs on opennow-embedded-linux-frame-publisher. Retain the
        // source owner while waiting, without locking render/presentation state.
        if let Some(vulkan) = frame
            .vulkan
            .as_ref()
            .filter(|vulkan| !vulkan.completed_gpu_copy())
        {
            let mut sync = self.decode_sync.lock().unwrap_or_else(|e| e.into_inner());
            if sync
                .as_ref()
                .is_none_or(|s| s.identity != (vulkan.instance, vulkan.device))
            {
                *sync = Some(DecodeSync::new(vulkan)?);
            }
            sync.as_ref()
                .expect("initialized decoder sync")
                .wait(vulkan)?;
        }
        Ok(LinuxGpuFrame {
            frame,
            producer: self.clone(),
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed) + 1,
        })
    }
}

impl LinuxGpuFrame {
    pub fn render_resources(&self) -> Arc<LinuxGpuRenderResources> {
        Arc::clone(&self.producer.render_resources)
    }

    pub fn width(&self) -> u32 {
        self.frame.format.width
    }

    pub fn height(&self) -> u32 {
        self.frame.format.height
    }

    pub fn sequence(&self) -> u64 {
        self.sequence
    }

    pub fn presentation_time_ns(&self) -> u64 {
        self.frame.timestamp_us.saturating_mul(1_000)
    }

    /// Records conversion before Qt begins its item render pass.
    ///
    /// # Safety
    ///
    /// `render` and `command_buffer` have the requirements documented by
    /// [`LinuxFrameProducer::new_with_slots`] and
    /// [`LinuxFrameProducer::record_frame`]. The host must retain
    /// [`Self::render_resources`] and retire it on the render thread after
    /// submission drains and before destroying the borrowed device.
    pub unsafe fn record(
        &self,
        render: VulkanRenderDevice,
        command_buffer: usize,
        frame_slot: u32,
    ) -> Result<RecordedGpuFrame> {
        let mut state = self
            .producer
            .render_resources
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.render != Some(render) {
            if state.producer.is_some() {
                return Err(Error::InvalidFormat(
                    "previous Qt device must be retired before recording on a new device"
                        .to_owned(),
                ));
            }
            state.render = None;
            state.producer = None;
            state.producer = Some(unsafe {
                LinuxFrameProducer::new_with_slots(render, self.producer.slot_count)?
            });
            state.render = Some(render);
        }
        let producer = state.producer.as_mut().expect("producer initialized");
        let result =
            unsafe { producer.record_frame(self.frame.clone(), command_buffer, frame_slot) };
        producer.device_lost |= matches!(&result, Err(Error::DeviceLost { .. }));
        result
    }
}

impl VulkanRenderDevice {
    fn validate(self) -> Result<()> {
        if self.instance == 0 || self.physical_device == 0 || self.device == 0 || self.queue == 0 {
            return Err(Error::InvalidFormat(
                "Qt Vulkan render device contains a null handle".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreparedVulkanImage {
    pub image: u64,
    pub format: i32,
    pub width: u32,
    pub height: u32,
    pub old_layout: i32,
    pub old_access: u64,
    pub source_queue_family: u32,
    pub render_queue_family: u32,
}

#[derive(Debug)]
pub struct PreparedVulkanFrame {
    pub images: Vec<PreparedVulkanImage>,
    source: Arc<DecodedVideoFrame>,
}

impl PreparedVulkanFrame {
    pub fn retain_source(&self) -> &Arc<DecodedVideoFrame> {
        &self.source
    }
}

#[derive(Debug)]
pub struct CpuNv12Frame {
    pub luma: FramePlane,
    pub chroma: FramePlane,
    source: Arc<DecodedVideoFrame>,
}

impl CpuNv12Frame {
    pub fn retain_source(&self) -> &Arc<DecodedVideoFrame> {
        &self.source
    }
}

#[derive(Debug)]
pub enum PreparedLinuxFrame {
    Vulkan(PreparedVulkanFrame),
    DmaBuf(Arc<ImportedNv12Frame>),
    Sand(sand::ImportedSandFrame),
    Cpu(CpuNv12Frame),
}

impl PreparedLinuxFrame {
    pub fn timestamp_us(&self) -> u64 {
        self.source().timestamp_us
    }

    fn source(&self) -> &DecodedVideoFrame {
        match self {
            Self::Vulkan(frame) => &frame.source,
            Self::DmaBuf(frame) => &frame.source,
            Self::Sand(frame) => &frame.source,
            Self::Cpu(frame) => &frame.source,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DmaBufKey {
    ten_bit: bool,
    device: u64,
    inode: u64,
    width: u32,
    height: u32,
    modifier: u64,
    luma_offset: usize,
    luma_pitch: usize,
    chroma_offset: usize,
    chroma_pitch: usize,
}

pub struct ImportedNv12Frame {
    pub image: u64,
    pub luma_view: u64,
    pub chroma_view: u64,
    pub modifier: u64,
    pub external_queue_family: u32,
    pub render_queue_family: u32,
    image_handle: vk::Image,
    luma_view_handle: vk::ImageView,
    chroma_view_handle: vk::ImageView,
    memory: vk::DeviceMemory,
    device: ash::Device,
    source: Arc<DecodedVideoFrame>,
}

impl ImportedNv12Frame {
    pub fn retain_source(&self) -> &Arc<DecodedVideoFrame> {
        &self.source
    }
}

impl std::fmt::Debug for ImportedNv12Frame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImportedNv12Frame")
            .field("image", &self.image)
            .field("luma_view", &self.luma_view)
            .field("chroma_view", &self.chroma_view)
            .field("modifier", &self.modifier)
            .finish_non_exhaustive()
    }
}

impl Drop for ImportedNv12Frame {
    fn drop(&mut self) {
        unsafe {
            self.device
                .destroy_image_view(self.chroma_view_handle, None);
            self.device.destroy_image_view(self.luma_view_handle, None);
            self.device.destroy_image(self.image_handle, None);
            self.device.free_memory(self.memory, None);
        }
    }
}

pub struct LinuxFrameProducer {
    _entry: ash::Entry,
    instance: ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    render: VulkanRenderDevice,
    dmabuf_import_supported: bool,
    imported: HashMap<DmaBufKey, Weak<ImportedNv12Frame>>,
    renderers: HashMap<GpuTextureFormat, RendererResources>,
    slots: Vec<Option<FrameSlotResources>>,
    generation: u64,
    device_lost: bool,
}

struct RendererResources {
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    descriptor_pool: vk::DescriptorPool,
    descriptor_sets: Vec<vk::DescriptorSet>,
    sampler: vk::Sampler,
}

struct GpuImage {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
}

struct CpuUploadResources {
    luma: GpuImage,
    chroma: GpuImage,
    staging_buffer: vk::Buffer,
    staging_memory: vk::DeviceMemory,
    initialized: bool,
}

struct FrameSlotResources {
    width: u32,
    height: u32,
    texture_format: GpuTextureFormat,
    output: GpuImage,
    output_initialized: bool,
    framebuffer: vk::Framebuffer,
    cpu: Option<CpuUploadResources>,
    sand: Option<sand::SandImages>,
    owned_input_views: Vec<vk::ImageView>,
    frame: Option<PreparedLinuxFrame>,
}

impl FrameSlotResources {
    fn reusable(&self, width: u32, height: u32, texture_format: GpuTextureFormat) -> bool {
        self.width == width && self.height == height && self.texture_format == texture_format
    }
}

impl LinuxFrameProducer {
    /// Adopts Qt's Vulkan handles without taking ownership of them.
    ///
    /// The producer and every prepared frame must be dropped before Qt destroys
    /// the corresponding QRhi. Calls must remain on the QRhi render thread.
    ///
    /// # Safety
    ///
    /// All handles must belong to the same live Vulkan device and remain valid
    /// for the lifetime described above. `render.queue` must be reserved for
    /// Qt, externally synchronized by the calling render thread, and excluded
    /// from FFmpeg's queue table.
    pub unsafe fn new(render: VulkanRenderDevice) -> Result<Self> {
        unsafe { Self::new_with_slots(render, DEFAULT_FRAME_SLOTS) }
    }

    /// # Safety
    ///
    /// This has the same requirements as [`Self::new`]. `slot_count` must be
    /// Qt's maximum number of in-flight QRhi frame slots.
    pub unsafe fn new_with_slots(render: VulkanRenderDevice, slot_count: u32) -> Result<Self> {
        render.validate()?;
        if slot_count == 0 || slot_count > MAX_FRAME_SLOTS {
            return Err(Error::InvalidFormat(
                "embedded Vulkan frame slot count must be between 1 and 8".to_owned(),
            ));
        }
        let entry = unsafe { ash::Entry::load() }.map_err(|error| {
            Error::unavailable(Subsystem::Vulkan, format!("load Vulkan entry: {error}"))
        })?;
        let instance = unsafe {
            ash::Instance::load(
                entry.static_fn(),
                vk::Instance::from_raw(render.instance as u64),
            )
        };
        let physical_device = vk::PhysicalDevice::from_raw(render.physical_device as u64);
        let device = unsafe {
            ash::Device::load(
                instance.fp_v1_0(),
                vk::Device::from_raw(render.device as u64),
            )
        };
        let dmabuf_import_supported = render.dmabuf_import_enabled;
        Ok(Self {
            _entry: entry,
            instance,
            physical_device,
            device,
            render,
            dmabuf_import_supported,
            imported: HashMap::new(),
            renderers: HashMap::new(),
            slots: (0..slot_count).map(|_| None).collect(),
            generation: 0,
            device_lost: false,
        })
    }

    /// Encodes NV12-to-RGBA into Qt's current Vulkan command buffer.
    ///
    /// The command buffer must be recording but outside a render pass. This
    /// method never begins, ends, submits, or waits for Qt's command buffer.
    /// Qt may reuse a slot only after its previous frame has completed.
    ///
    /// # Safety
    ///
    /// `command_buffer` must belong to the adopted device and be in the
    /// recording state on the QRhi render thread. Qt must submit it on the
    /// adopted queue after this call's decoder semaphore wait submission.
    pub unsafe fn record_frame(
        &mut self,
        frame: DecodedVideoFrame,
        command_buffer: usize,
        slot: u32,
    ) -> Result<RecordedGpuFrame> {
        if command_buffer == 0 {
            return Err(Error::InvalidFormat(
                "Qt supplied a null Vulkan command buffer".to_owned(),
            ));
        }
        let slot_index = usize::try_from(slot)
            .ok()
            .filter(|index| *index < self.slots.len())
            .ok_or_else(|| Error::InvalidFormat(format!("invalid QRhi frame slot {slot}")))?;
        let width = frame.format.width;
        let height = frame.format.height;
        let timestamp_us = frame.timestamp_us;
        let color_matrix = frame.format.color_matrix;
        let full_range = frame.format.color_range == crate::ColorRange::Full;
        let chroma_location = frame.format.chroma_location;
        let color_space = LinuxTextureColorSpace::from_format(frame.format)?;
        let texture_format = GpuTextureFormat::for_stream_format(frame.format)?;
        let prepared = self.prepare(frame)?;
        self.ensure_renderer(texture_format)?;
        self.ensure_slot(slot_index, width, height, texture_format)?;
        let command_buffer = vk::CommandBuffer::from_raw(command_buffer as u64);
        let (luma_view, chroma_view) =
            self.prepare_input(slot_index, command_buffer, &prepared, width, height)?;
        self.update_descriptors(slot_index, luma_view, chroma_view);
        if let PreparedLinuxFrame::Vulkan(frame) = &prepared {
            if let Err(error) =
                self.enqueue_decode_wait(frame.source.vulkan.as_ref().expect("Vulkan source"))
            {
                for view in self.slots[slot_index]
                    .as_mut()
                    .expect("frame slot initialized")
                    .owned_input_views
                    .drain(..)
                {
                    unsafe { self.device.destroy_image_view(view, None) };
                }
                return Err(error);
            }
        }
        unsafe {
            self.record_conversion(
                slot_index,
                command_buffer,
                &prepared,
                width,
                height,
                color_matrix,
                full_range,
                chroma_location,
            )?;
        }
        self.generation = self.generation.wrapping_add(1);
        let generation = self.generation;
        let slot_resources = self.slots[slot_index]
            .as_mut()
            .expect("frame slot initialized");
        slot_resources.frame = Some(prepared);
        Ok(RecordedGpuFrame {
            slot,
            generation,
            image: slot_resources.output.image.as_raw(),
            image_view: slot_resources.output.view.as_raw(),
            texture_format: slot_resources.texture_format,
            width,
            height,
            timestamp_us,
            color_space,
        })
    }

    pub fn prepare(&mut self, frame: DecodedVideoFrame) -> Result<PreparedLinuxFrame> {
        frame.validate()?;
        LinuxTextureColorSpace::from_format(frame.format)?;
        if self.device_lost {
            return Err(vk_error(
                "embedded Vulkan device is lost",
                vk::Result::ERROR_DEVICE_LOST,
            ));
        }
        if !matches!(
            frame.format.pixel_format,
            PixelFormat::Nv12 | PixelFormat::P010 | PixelFormat::Nv24 | PixelFormat::P410
        ) {
            return Err(Error::InvalidFormat(format!(
                "embedded Linux presentation requires NV12, P010, NV24 or P410, received {:?}",
                frame.format.pixel_format
            )));
        }
        let frame = Arc::new(frame);
        if let Some(vulkan) = frame.vulkan.as_ref() {
            validate_direct_frame(vulkan, frame.format, self.render)?;
            self.wait_for_decode(vulkan)?;
            let images = vulkan
                .images
                .iter()
                .map(|image| PreparedVulkanImage {
                    image: image.image,
                    format: image.format,
                    width: image.width,
                    height: image.height,
                    old_layout: image.layout,
                    old_access: image.access,
                    source_queue_family: image.queue_family,
                    render_queue_family: self.render.queue_family,
                })
                .collect();
            return Ok(PreparedLinuxFrame::Vulkan(PreparedVulkanFrame {
                images,
                source: frame,
            }));
        }
        let mut gpu_error = None;
        if frame
            .dmabuf
            .as_ref()
            .is_some_and(|frame| sand::is_sand(frame))
        {
            if !self.render.dmabuf_buffer_import_enabled {
                return Err(Error::unavailable(
                    Subsystem::Vulkan,
                    "SAND DMA-BUF requires Qt Vulkan external buffer import and foreign queue ownership support",
                ));
            }
            return sand::import(&self.instance, self.physical_device, &self.device, frame)
                .map(PreparedLinuxFrame::Sand);
        }
        if self.dmabuf_import_supported && frame.dmabuf.is_some() {
            match self.import_dmabuf(Arc::clone(&frame)) {
                Ok(imported) => return Ok(PreparedLinuxFrame::DmaBuf(imported)),
                Err(error) => gpu_error = Some(error),
            }
        }
        // CPU planes are accepted only from an explicitly CPU-backed decoder,
        // never as an automatic substitute for failed native GPU interop.
        if frame.vulkan.is_none()
            && frame.dmabuf.is_none()
            && frame.planes.len() == 2
            && frame.format.pixel_format == PixelFormat::Nv12
            && frame.format.color_transfer == crate::ColorTransfer::Sdr
        {
            return Ok(PreparedLinuxFrame::Cpu(CpuNv12Frame {
                luma: frame.planes[0].clone(),
                chroma: frame.planes[1].clone(),
                source: frame,
            }));
        }
        if let Some(error) = gpu_error {
            return Err(error);
        }
        Err(Error::unavailable(
            Subsystem::Vulkan,
            "decoded GPU frame cannot be imported by Qt's Vulkan device; implicit CPU readback is disabled",
        ))
    }

    fn wait_for_decode(&mut self, frame: &VulkanVideoFrame) -> Result<()> {
        let (semaphores, values): (Vec<_>, Vec<_>) = timeline_waits(frame)?
            .into_iter()
            .map(|(semaphore, value)| (vk::Semaphore::from_raw(semaphore), value))
            .unzip();
        if semaphores.is_empty() {
            return Ok(());
        }
        let wait = vk::SemaphoreWaitInfo::default()
            .semaphores(&semaphores)
            .values(&values);
        // Readiness was awaited by the publisher. Revalidate without waiting if
        // this public preparation API is called with a frame from another owner.
        let result = unsafe { self.device.wait_semaphores(&wait, 0) };
        self.device_lost |= result == Err(vk::Result::ERROR_DEVICE_LOST);
        decode_readiness(result)
    }

    fn enqueue_decode_wait(&mut self, frame: &VulkanVideoFrame) -> Result<()> {
        let (semaphores, values): (Vec<_>, Vec<_>) = timeline_waits(frame)?
            .into_iter()
            .map(|(semaphore, value)| (vk::Semaphore::from_raw(semaphore), value))
            .unzip();
        if semaphores.is_empty() {
            return Ok(());
        }
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; semaphores.len()];
        let mut timeline =
            vk::TimelineSemaphoreSubmitInfo::default().wait_semaphore_values(&values);
        let submits = [vk::SubmitInfo::default()
            .push_next(&mut timeline)
            .wait_semaphores(&semaphores)
            .wait_dst_stage_mask(&stages)];
        let result = unsafe {
            self.device.queue_submit(
                vk::Queue::from_raw(self.render.queue as u64),
                &submits,
                vk::Fence::null(),
            )
        };
        self.device_lost |= result == Err(vk::Result::ERROR_DEVICE_LOST);
        result.map_err(|error| vk_error("enqueue Qt Vulkan decoder timeline wait", error))
    }

    fn import_dmabuf(&mut self, source: Arc<DecodedVideoFrame>) -> Result<Arc<ImportedNv12Frame>> {
        let dmabuf = source.dmabuf.as_ref().expect("DMA-BUF checked by caller");
        let (object_index, luma, chroma) = yuv_dmabuf_layout(dmabuf, source.format.pixel_format)?;
        let object = dmabuf.objects[object_index];
        let (device, inode) = fd_identity(object.fd)?;
        let key = DmaBufKey {
            ten_bit: source.format.pixel_format == PixelFormat::P010,
            device,
            inode,
            width: source.format.width,
            height: source.format.height,
            modifier: object.format_modifier,
            luma_offset: luma.offset,
            luma_pitch: luma.pitch,
            chroma_offset: chroma.offset,
            chroma_pitch: chroma.pitch,
        };
        if let Some(imported) = self.imported.get(&key).and_then(Weak::upgrade) {
            return Ok(imported);
        }
        self.imported
            .retain(|_, imported| imported.strong_count() != 0);
        let imported = Arc::new(import_nv12_dmabuf(
            &self.instance,
            self.physical_device,
            &self.device,
            self.render.queue_family,
            object.fd,
            object.size,
            object.format_modifier,
            luma,
            chroma,
            source,
        )?);
        self.imported.insert(key, Arc::downgrade(&imported));
        Ok(imported)
    }

    fn ensure_renderer(&mut self, texture_format: GpuTextureFormat) -> Result<()> {
        if self.renderers.contains_key(&texture_format) {
            return Ok(());
        }
        let properties = unsafe {
            self.instance.get_physical_device_format_properties(
                self.physical_device,
                texture_format.vulkan_format(),
            )
        };
        texture_format.validate_features(properties.optimal_tiling_features)?;
        let attachment = [vk::AttachmentDescription::default()
            .format(texture_format.vulkan_format())
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let color_reference = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpasses = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_reference)];
        let render_pass = unsafe {
            self.device.create_render_pass(
                &vk::RenderPassCreateInfo::default()
                    .attachments(&attachment)
                    .subpasses(&subpasses),
                None,
            )
        }
        .map_err(|error| vk_error("create embedded NV12 render pass", error))?;
        let bindings = [0, 1].map(|binding| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        });
        let descriptor_set_layout = unsafe {
            self.device.create_descriptor_set_layout(
                &vk::DescriptorSetLayoutCreateInfo::default().bindings(&bindings),
                None,
            )
        }
        .map_err(|error| vk_error("create embedded NV12 descriptor layout", error))?;
        let layouts = [descriptor_set_layout];
        let push_ranges = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .size(size_of::<ConversionConstants>() as u32)];
        let pipeline_layout = unsafe {
            self.device.create_pipeline_layout(
                &vk::PipelineLayoutCreateInfo::default()
                    .set_layouts(&layouts)
                    .push_constant_ranges(&push_ranges),
                None,
            )
        }
        .map_err(|error| vk_error("create embedded NV12 pipeline layout", error))?;
        let vertex_words = ash::util::read_spv(&mut Cursor::new(NV12_VERTEX_SHADER))
            .map_err(|error| Error::backend(Subsystem::Vulkan, error.to_string()))?;
        let fragment_words = ash::util::read_spv(&mut Cursor::new(NV12_FRAGMENT_SHADER))
            .map_err(|error| Error::backend(Subsystem::Vulkan, error.to_string()))?;
        let vertex_module = unsafe {
            self.device.create_shader_module(
                &vk::ShaderModuleCreateInfo::default().code(&vertex_words),
                None,
            )
        }
        .map_err(|error| vk_error("create embedded NV12 vertex shader", error))?;
        let fragment_module = unsafe {
            self.device.create_shader_module(
                &vk::ShaderModuleCreateInfo::default().code(&fragment_words),
                None,
            )
        }
        .map_err(|error| vk_error("create embedded NV12 fragment shader", error))?;
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_module)
                .name(c"main"),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fragment_module)
                .name(c"main"),
        ];
        let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
        let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
            .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
        let viewport_state = vk::PipelineViewportStateCreateInfo::default()
            .viewport_count(1)
            .scissor_count(1);
        let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
            .polygon_mode(vk::PolygonMode::FILL)
            .cull_mode(vk::CullModeFlags::NONE)
            .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
            .line_width(1.0);
        let multisample = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);
        let blend_attachments = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)];
        let blend =
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&blend_attachments);
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let pipeline_info = [vk::GraphicsPipelineCreateInfo::default()
            .stages(&stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .color_blend_state(&blend)
            .dynamic_state(&dynamic)
            .layout(pipeline_layout)
            .render_pass(render_pass)];
        let pipeline = unsafe {
            self.device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
        }
        .map_err(|(_, error)| vk_error("create embedded NV12 pipeline", error))?[0];
        unsafe {
            self.device.destroy_shader_module(fragment_module, None);
            self.device.destroy_shader_module(vertex_module, None);
        }
        let sampler = unsafe {
            self.device.create_sampler(
                &vk::SamplerCreateInfo::default()
                    .mag_filter(vk::Filter::LINEAR)
                    .min_filter(vk::Filter::LINEAR)
                    .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
                    .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE),
                None,
            )
        }
        .map_err(|error| vk_error("create embedded NV12 sampler", error))?;
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(self.slots.len() as u32 * 2)];
        let descriptor_pool = unsafe {
            self.device.create_descriptor_pool(
                &vk::DescriptorPoolCreateInfo::default()
                    .max_sets(self.slots.len() as u32)
                    .pool_sizes(&pool_sizes),
                None,
            )
        }
        .map_err(|error| vk_error("create embedded NV12 descriptor pool", error))?;
        let set_layouts = vec![descriptor_set_layout; self.slots.len()];
        let descriptor_sets = unsafe {
            self.device.allocate_descriptor_sets(
                &vk::DescriptorSetAllocateInfo::default()
                    .descriptor_pool(descriptor_pool)
                    .set_layouts(&set_layouts),
            )
        }
        .map_err(|error| vk_error("allocate embedded NV12 descriptor sets", error))?;
        self.renderers.insert(
            texture_format,
            RendererResources {
                render_pass,
                descriptor_set_layout,
                pipeline_layout,
                pipeline,
                descriptor_pool,
                descriptor_sets,
                sampler,
            },
        );
        Ok(())
    }

    fn ensure_slot(
        &mut self,
        slot: usize,
        width: u32,
        height: u32,
        texture_format: GpuTextureFormat,
    ) -> Result<()> {
        if self.slots[slot]
            .as_ref()
            .is_some_and(|resources| resources.reusable(width, height, texture_format))
        {
            return Ok(());
        }
        let usage = vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED;
        let properties = unsafe {
            self.instance.get_physical_device_image_format_properties(
                self.physical_device,
                texture_format.vulkan_format(),
                vk::ImageType::TYPE_2D,
                vk::ImageTiling::OPTIMAL,
                usage,
                vk::ImageCreateFlags::empty(),
            )
        }
        .map_err(|error| {
            vk_error(
                &format!("query embedded {texture_format:?} output support"),
                error,
            )
        })?;
        if width > properties.max_extent.width
            || height > properties.max_extent.height
            || !properties
                .sample_counts
                .contains(vk::SampleCountFlags::TYPE_1)
        {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                format!(
                    "embedded {texture_format:?} output does not support {width}x{height} single-sample images"
                ),
            ));
        }
        if let Some(resources) = self.slots[slot].take() {
            self.destroy_slot(resources);
        }
        let output = create_gpu_image(
            &self.instance,
            self.physical_device,
            &self.device,
            width,
            height,
            texture_format.vulkan_format(),
            usage,
        )?;
        let renderer = &self.renderers[&texture_format];
        let attachments = [output.view];
        let framebuffer = unsafe {
            self.device.create_framebuffer(
                &vk::FramebufferCreateInfo::default()
                    .render_pass(renderer.render_pass)
                    .attachments(&attachments)
                    .width(width)
                    .height(height)
                    .layers(1),
                None,
            )
        }
        .map_err(|error| vk_error("create embedded RGBA framebuffer", error))?;
        self.slots[slot] = Some(FrameSlotResources {
            width,
            height,
            texture_format,
            output,
            output_initialized: false,
            framebuffer,
            cpu: None,
            sand: None,
            owned_input_views: Vec::new(),
            frame: None,
        });
        Ok(())
    }

    fn prepare_input(
        &mut self,
        slot: usize,
        command_buffer: vk::CommandBuffer,
        prepared: &PreparedLinuxFrame,
        width: u32,
        height: u32,
    ) -> Result<(vk::ImageView, vk::ImageView)> {
        let resources = self.slots[slot].as_mut().expect("slot initialized");
        for view in resources.owned_input_views.drain(..) {
            unsafe { self.device.destroy_image_view(view, None) };
        }
        resources.frame = None;
        match prepared {
            PreparedLinuxFrame::Vulkan(frame) => {
                let (luma, chroma) = direct_image_views(&self.device, &frame.images)?;
                resources.owned_input_views.extend([luma, chroma]);
                Ok((luma, chroma))
            }
            PreparedLinuxFrame::DmaBuf(frame) => Ok((
                vk::ImageView::from_raw(frame.luma_view),
                vk::ImageView::from_raw(frame.chroma_view),
            )),
            PreparedLinuxFrame::Sand(frame) => {
                if resources.sand.is_none() {
                    resources.sand = Some(sand::SandImages::new(
                        &self.instance,
                        self.physical_device,
                        &self.device,
                        width,
                        height,
                    )?);
                }
                let images = resources.sand.as_mut().expect("SAND images initialized");
                images.record(
                    &self.device,
                    command_buffer,
                    self.render.queue_family,
                    frame,
                );
                Ok(images.views())
            }
            PreparedLinuxFrame::Cpu(frame) => {
                if resources.cpu.is_none() {
                    resources.cpu = Some(create_cpu_upload_resources(
                        &self.instance,
                        self.physical_device,
                        &self.device,
                        width,
                        height,
                    )?);
                }
                let cpu = resources.cpu.as_mut().expect("CPU resources initialized");
                upload_cpu_nv12(&self.device, command_buffer, cpu, frame, width, height)?;
                Ok((cpu.luma.view, cpu.chroma.view))
            }
        }
    }

    fn update_descriptors(
        &self,
        slot: usize,
        luma_view: vk::ImageView,
        chroma_view: vk::ImageView,
    ) {
        let resources = self.slots[slot].as_ref().expect("slot initialized");
        let renderer = &self.renderers[&resources.texture_format];
        let luma = [vk::DescriptorImageInfo::default()
            .sampler(renderer.sampler)
            .image_view(luma_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let chroma = [vk::DescriptorImageInfo::default()
            .sampler(renderer.sampler)
            .image_view(chroma_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(renderer.descriptor_sets[slot])
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&luma),
            vk::WriteDescriptorSet::default()
                .dst_set(renderer.descriptor_sets[slot])
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&chroma),
        ];
        unsafe { self.device.update_descriptor_sets(&writes, &[]) };
    }

    #[allow(clippy::too_many_arguments)]
    unsafe fn record_conversion(
        &mut self,
        slot: usize,
        command_buffer: vk::CommandBuffer,
        prepared: &PreparedLinuxFrame,
        width: u32,
        height: u32,
        color_matrix: crate::ColorMatrix,
        full_range: bool,
        chroma_location: crate::ChromaLocation,
    ) -> Result<()> {
        let resources = self.slots[slot].as_mut().expect("slot initialized");
        let renderer = &self.renderers[&resources.texture_format];
        let mut acquire = Vec::new();
        match prepared {
            PreparedLinuxFrame::Vulkan(frame) => {
                acquire.extend(frame.images.iter().map(|image| {
                    vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::from_raw(image.old_layout))
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .src_access_mask(
                            vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
                        )
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .image(vk::Image::from_raw(image.image))
                        .subresource_range(image_color_range())
                }));
            }
            PreparedLinuxFrame::DmaBuf(frame) => {
                acquire.push(
                    vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::GENERAL)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                        .dst_queue_family_index(self.render.queue_family)
                        .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .image(vk::Image::from_raw(frame.image))
                        .subresource_range(image_color_range()),
                );
            }
            PreparedLinuxFrame::Cpu(_) | PreparedLinuxFrame::Sand(_) => {}
        }
        acquire.push(
            vk::ImageMemoryBarrier::default()
                .old_layout(if resources.output_initialized {
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                } else {
                    vk::ImageLayout::UNDEFINED
                })
                .new_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .src_access_mask(if resources.output_initialized {
                    vk::AccessFlags::SHADER_READ
                } else {
                    vk::AccessFlags::empty()
                })
                .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .image(resources.output.image)
                .subresource_range(image_color_range()),
        );
        unsafe {
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::FRAGMENT_SHADER
                    | vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &acquire,
            );
            self.device.cmd_begin_render_pass(
                command_buffer,
                &vk::RenderPassBeginInfo::default()
                    .render_pass(renderer.render_pass)
                    .framebuffer(resources.framebuffer)
                    .render_area(vk::Rect2D::default().extent(vk::Extent2D { width, height })),
                vk::SubpassContents::INLINE,
            );
            self.device.cmd_set_viewport(
                command_buffer,
                0,
                &[vk::Viewport::default()
                    .width(width as f32)
                    .height(height as f32)
                    .max_depth(1.0)],
            );
            self.device.cmd_set_scissor(
                command_buffer,
                0,
                &[vk::Rect2D::default().extent(vk::Extent2D { width, height })],
            );
            self.device.cmd_bind_pipeline(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                renderer.pipeline,
            );
            self.device.cmd_bind_descriptor_sets(
                command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                renderer.pipeline_layout,
                0,
                &[renderer.descriptor_sets[slot]],
                &[],
            );
            let constants = ConversionConstants {
                texture_scale: [1.0, 1.0],
                color_matrix: match color_matrix {
                    crate::ColorMatrix::Bt601 => 0,
                    crate::ColorMatrix::Bt709 => 1,
                    crate::ColorMatrix::Bt2020 => 2,
                },
                full_range: u32::from(full_range),
                sample_bits: match prepared {
                    PreparedLinuxFrame::Vulkan(frame)
                        if frame.source.format.pixel_format.is_ten_bit() =>
                    {
                        if vk::Format::from_raw(frame.images[0].format) == vk::Format::R16_UNORM {
                            16
                        } else {
                            10
                        }
                    }
                    PreparedLinuxFrame::DmaBuf(frame)
                        if frame.source.format.pixel_format == PixelFormat::P010 =>
                    {
                        10
                    }
                    _ => 8,
                },
                chroma_offset_x: if chroma_location == crate::ChromaLocation::Left
                    && !prepared.source().format.pixel_format.is_444()
                {
                    0.5 / width as f32
                } else {
                    0.0
                },
            };
            let constants = std::slice::from_raw_parts(
                (&constants as *const ConversionConstants).cast::<u8>(),
                size_of::<ConversionConstants>(),
            );
            self.device.cmd_push_constants(
                command_buffer,
                renderer.pipeline_layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                constants,
            );
            self.device.cmd_draw(command_buffer, 3, 1, 0, 0);
            self.device.cmd_end_render_pass(command_buffer);
        }
        let mut release = vec![
            vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .image(resources.output.image)
                .subresource_range(image_color_range()),
        ];
        match prepared {
            PreparedLinuxFrame::Vulkan(frame) => {
                release.extend(frame.images.iter().map(|image| {
                    vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .new_layout(vk::ImageLayout::from_raw(image.old_layout))
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .src_access_mask(vk::AccessFlags::SHADER_READ)
                        .dst_access_mask(
                            vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE,
                        )
                        .image(vk::Image::from_raw(image.image))
                        .subresource_range(image_color_range())
                }));
            }
            PreparedLinuxFrame::DmaBuf(frame) => release.push(
                vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .src_queue_family_index(self.render.queue_family)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                    .src_access_mask(vk::AccessFlags::SHADER_READ)
                    .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                    .image(vk::Image::from_raw(frame.image))
                    .subresource_range(image_color_range()),
            ),
            PreparedLinuxFrame::Cpu(_) | PreparedLinuxFrame::Sand(_) => {}
        }
        unsafe {
            self.device.cmd_pipeline_barrier(
                command_buffer,
                vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT
                    | vk::PipelineStageFlags::FRAGMENT_SHADER,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &release,
            );
        }
        resources.output_initialized = true;
        Ok(())
    }

    fn destroy_slot(&self, mut resources: FrameSlotResources) {
        unsafe {
            for view in resources.owned_input_views.drain(..) {
                self.device.destroy_image_view(view, None);
            }
            if let Some(cpu) = resources.cpu.take() {
                self.device.destroy_buffer(cpu.staging_buffer, None);
                self.device.free_memory(cpu.staging_memory, None);
                destroy_gpu_image(&self.device, cpu.chroma);
                destroy_gpu_image(&self.device, cpu.luma);
            }
            self.device.destroy_framebuffer(resources.framebuffer, None);
            destroy_gpu_image(&self.device, resources.output);
        }
        resources.frame = None;
    }
}

#[repr(C)]
struct ConversionConstants {
    texture_scale: [f32; 2],
    color_matrix: u32,
    full_range: u32,
    sample_bits: u32,
    chroma_offset_x: f32,
}

impl Drop for LinuxFrameProducer {
    fn drop(&mut self) {
        for resources in std::mem::take(&mut self.slots).into_iter().flatten() {
            self.destroy_slot(resources);
        }
        for (_, renderer) in self.renderers.drain() {
            unsafe {
                self.device
                    .destroy_descriptor_pool(renderer.descriptor_pool, None);
                self.device.destroy_sampler(renderer.sampler, None);
                self.device.destroy_pipeline(renderer.pipeline, None);
                self.device
                    .destroy_pipeline_layout(renderer.pipeline_layout, None);
                self.device
                    .destroy_descriptor_set_layout(renderer.descriptor_set_layout, None);
                self.device.destroy_render_pass(renderer.render_pass, None);
            }
        }
        self.imported.clear();
    }
}

fn direct_image_views(
    device: &ash::Device,
    images: &[PreparedVulkanImage],
) -> Result<(vk::ImageView, vk::ImageView)> {
    if images.len() == 1 {
        let image = vk::Image::from_raw(images[0].image);
        let ten_bit = matches!(
            vk::Format::from_raw(images[0].format),
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
                | vk::Format::G10X6_B10X6R10X6_2PLANE_444_UNORM_3PACK16
        );
        let luma = create_image_view(
            device,
            image,
            if ten_bit {
                vk::Format::R10X6_UNORM_PACK16
            } else {
                vk::Format::R8_UNORM
            },
            vk::ImageAspectFlags::PLANE_0,
        )?;
        let chroma = match create_image_view(
            device,
            image,
            if ten_bit {
                vk::Format::R10X6G10X6_UNORM_2PACK16
            } else {
                vk::Format::R8G8_UNORM
            },
            vk::ImageAspectFlags::PLANE_1,
        ) {
            Ok(view) => view,
            Err(error) => {
                unsafe { device.destroy_image_view(luma, None) };
                return Err(error);
            }
        };
        return Ok((luma, chroma));
    }
    if images.len() != 2 {
        return Err(Error::InvalidFormat(
            "Vulkan YUV frame must contain one multiplanar or two plane images".to_owned(),
        ));
    }
    let luma = create_image_view(
        device,
        vk::Image::from_raw(images[0].image),
        vk::Format::from_raw(images[0].format),
        vk::ImageAspectFlags::COLOR,
    )?;
    let chroma = match create_image_view(
        device,
        vk::Image::from_raw(images[1].image),
        vk::Format::from_raw(images[1].format),
        vk::ImageAspectFlags::COLOR,
    ) {
        Ok(view) => view,
        Err(error) => {
            unsafe { device.destroy_image_view(luma, None) };
            return Err(error);
        }
    };
    Ok((luma, chroma))
}

fn create_gpu_image(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: &ash::Device,
    width: u32,
    height: u32,
    format: vk::Format,
    usage: vk::ImageUsageFlags,
) -> Result<GpuImage> {
    let image = unsafe {
        device.create_image(
            &vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(usage)
                .sharing_mode(vk::SharingMode::EXCLUSIVE)
                .initial_layout(vk::ImageLayout::UNDEFINED),
            None,
        )
    }
    .map_err(|error| vk_error("create embedded GPU image", error))?;
    let requirements = unsafe { device.get_image_memory_requirements(image) };
    let memory_type = match find_memory_type(
        instance,
        physical_device,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    ) {
        Ok(memory_type) => memory_type,
        Err(error) => {
            unsafe { device.destroy_image(image, None) };
            return Err(error);
        }
    };
    let memory = match unsafe {
        device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type),
            None,
        )
    } {
        Ok(memory) => memory,
        Err(error) => {
            unsafe { device.destroy_image(image, None) };
            return Err(vk_error("allocate embedded GPU image", error));
        }
    };
    if let Err(error) = unsafe { device.bind_image_memory(image, memory, 0) } {
        unsafe {
            device.free_memory(memory, None);
            device.destroy_image(image, None);
        }
        return Err(vk_error("bind embedded GPU image", error));
    }
    let view = match create_image_view(device, image, format, vk::ImageAspectFlags::COLOR) {
        Ok(view) => view,
        Err(error) => {
            unsafe {
                device.destroy_image(image, None);
                device.free_memory(memory, None);
            }
            return Err(error);
        }
    };
    Ok(GpuImage {
        image,
        memory,
        view,
    })
}

fn create_cpu_upload_resources(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: &ash::Device,
    width: u32,
    height: u32,
) -> Result<CpuUploadResources> {
    let luma = create_gpu_image(
        instance,
        physical_device,
        device,
        width,
        height,
        vk::Format::R8_UNORM,
        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
    )?;
    let chroma = create_gpu_image(
        instance,
        physical_device,
        device,
        width / 2,
        height / 2,
        vk::Format::R8G8_UNORM,
        vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
    )?;
    let size = u64::from(width) * u64::from(height) * 3 / 2;
    let buffer = unsafe {
        device.create_buffer(
            &vk::BufferCreateInfo::default()
                .size(size)
                .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            None,
        )
    }
    .map_err(|error| vk_error("create NV12 upload buffer", error))?;
    let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
    let memory_type = find_memory_type(
        instance,
        physical_device,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
    )?;
    let memory = unsafe {
        device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type),
            None,
        )
    }
    .map_err(|error| vk_error("allocate NV12 upload buffer", error))?;
    unsafe { device.bind_buffer_memory(buffer, memory, 0) }
        .map_err(|error| vk_error("bind NV12 upload buffer", error))?;
    Ok(CpuUploadResources {
        luma,
        chroma,
        staging_buffer: buffer,
        staging_memory: memory,
        initialized: false,
    })
}

fn upload_cpu_nv12(
    device: &ash::Device,
    command_buffer: vk::CommandBuffer,
    resources: &mut CpuUploadResources,
    frame: &CpuNv12Frame,
    width: u32,
    height: u32,
) -> Result<()> {
    let luma_len = width as usize * height as usize;
    let total_len = luma_len + luma_len / 2;
    let mapped = unsafe {
        device.map_memory(
            resources.staging_memory,
            0,
            total_len as u64,
            vk::MemoryMapFlags::empty(),
        )
    }
    .map_err(|error| vk_error("map NV12 upload buffer", error))?;
    unsafe {
        copy_plane_rows(&frame.luma, mapped.cast(), width as usize, height as usize);
        copy_plane_rows(
            &frame.chroma,
            mapped.cast::<u8>().add(luma_len),
            width as usize,
            height as usize / 2,
        );
        device.unmap_memory(resources.staging_memory);
    }
    let source_stage = if resources.initialized {
        vk::PipelineStageFlags::FRAGMENT_SHADER
    } else {
        vk::PipelineStageFlags::TOP_OF_PIPE
    };
    let source_access = if resources.initialized {
        vk::AccessFlags::SHADER_READ
    } else {
        vk::AccessFlags::empty()
    };
    let old_layout = if resources.initialized {
        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
    } else {
        vk::ImageLayout::UNDEFINED
    };
    let to_transfer = [resources.luma.image, resources.chroma.image].map(|image| {
        vk::ImageMemoryBarrier::default()
            .old_layout(old_layout)
            .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
            .src_access_mask(source_access)
            .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
            .image(image)
            .subresource_range(image_color_range())
    });
    unsafe {
        device.cmd_pipeline_barrier(
            command_buffer,
            source_stage,
            vk::PipelineStageFlags::TRANSFER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &to_transfer,
        );
        device.cmd_copy_buffer_to_image(
            command_buffer,
            resources.staging_buffer,
            resources.luma.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::BufferImageCopy::default()
                .buffer_offset(0)
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1),
                )
                .image_extent(vk::Extent3D {
                    width,
                    height,
                    depth: 1,
                })],
        );
        device.cmd_copy_buffer_to_image(
            command_buffer,
            resources.staging_buffer,
            resources.chroma.image,
            vk::ImageLayout::TRANSFER_DST_OPTIMAL,
            &[vk::BufferImageCopy::default()
                .buffer_offset(luma_len as u64)
                .image_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1),
                )
                .image_extent(vk::Extent3D {
                    width: width / 2,
                    height: height / 2,
                    depth: 1,
                })],
        );
        let to_sample = [resources.luma.image, resources.chroma.image].map(|image| {
            vk::ImageMemoryBarrier::default()
                .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                .dst_access_mask(vk::AccessFlags::SHADER_READ)
                .image(image)
                .subresource_range(image_color_range())
        });
        device.cmd_pipeline_barrier(
            command_buffer,
            vk::PipelineStageFlags::TRANSFER,
            vk::PipelineStageFlags::FRAGMENT_SHADER,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &to_sample,
        );
    }
    resources.initialized = true;
    Ok(())
}

unsafe fn copy_plane_rows(plane: &FramePlane, destination: *mut u8, row_bytes: usize, rows: usize) {
    for row in 0..rows {
        unsafe {
            std::ptr::copy_nonoverlapping(
                plane.data.as_ptr().add(row * plane.stride),
                destination.add(row * row_bytes),
                row_bytes,
            );
        }
    }
}

fn create_image_view(
    device: &ash::Device,
    image: vk::Image,
    format: vk::Format,
    aspect: vk::ImageAspectFlags,
) -> Result<vk::ImageView> {
    unsafe {
        device.create_image_view(
            &vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(aspect)
                        .level_count(1)
                        .layer_count(1),
                ),
            None,
        )
    }
    .map_err(|error| vk_error("create embedded image view", error))
}

fn destroy_gpu_image(device: &ash::Device, image: GpuImage) {
    unsafe {
        device.destroy_image_view(image.view, None);
        device.destroy_image(image.image, None);
        device.free_memory(image.memory, None);
    }
}

fn find_memory_type(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    memory_type_bits: u32,
    required: vk::MemoryPropertyFlags,
) -> Result<u32> {
    let properties = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    (0..properties.memory_type_count)
        .find(|index| {
            memory_type_bits & (1 << index) != 0
                && properties.memory_types[*index as usize]
                    .property_flags
                    .contains(required)
        })
        .ok_or_else(|| Error::unavailable(Subsystem::Vulkan, "no compatible Vulkan memory type"))
}

fn image_color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}

fn timeline_waits(frame: &VulkanVideoFrame) -> Result<Vec<(u64, u64)>> {
    let mut waits = HashMap::<u64, u64>::new();
    for image in &frame.images {
        if image.semaphore == 0 || image.semaphore_value == 0 {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                "Vulkan decoder frame has no host-waitable timeline semaphore",
            ));
        }
        match waits.entry(image.semaphore) {
            HashMapEntry::Occupied(mut entry) => {
                *entry.get_mut() = (*entry.get()).max(image.semaphore_value);
            }
            HashMapEntry::Vacant(entry) => {
                entry.insert(image.semaphore_value);
            }
        }
    }
    let mut waits = waits.into_iter().collect::<Vec<_>>();
    waits.sort_unstable_by_key(|(semaphore, _)| *semaphore);
    Ok(waits)
}

fn validate_direct_frame(
    frame: &VulkanVideoFrame,
    format: crate::StreamFormat,
    render: VulkanRenderDevice,
) -> Result<()> {
    frame.validate()?;
    if (frame.instance, frame.physical_device, frame.device)
        != (render.instance, render.physical_device, render.device)
    {
        return Err(Error::InvalidFormat(
            "Vulkan decoded frame belongs to a different Qt device".to_owned(),
        ));
    }
    let usage = vk::ImageUsageFlags::from_raw(frame.image_usage);
    if !usage.contains(vk::ImageUsageFlags::SAMPLED)
        || usage.contains(vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR)
    {
        return Err(Error::InvalidFormat(
            "direct Vulkan output must be sampleable and separate from decoder DPB images"
                .to_owned(),
        ));
    }
    if !frame.queue_families.contains(&render.queue_family)
        || frame.images.iter().any(|image| {
            image.queue_family != vk::QUEUE_FAMILY_IGNORED
                && image.queue_family != render.queue_family
        })
    {
        return Err(Error::InvalidFormat(
            "Vulkan output is not concurrently accessible by Qt's graphics family".to_owned(),
        ));
    }
    if frame.images.iter().any(|image| {
        matches!(
            vk::ImageLayout::from_raw(image.layout),
            vk::ImageLayout::UNDEFINED | vk::ImageLayout::PREINITIALIZED
        )
    }) {
        return Err(Error::InvalidFormat(
            "Vulkan output has no initialized image layout".to_owned(),
        ));
    }
    let ten_bit = format.pixel_format.is_ten_bit();
    let chroma_divisor = if format.pixel_format.is_444() { 1 } else { 2 };
    let valid = match frame.images.as_slice() {
        [image] => {
            let expected = match format.pixel_format {
                PixelFormat::Nv12 => vk::Format::G8_B8R8_2PLANE_420_UNORM,
                PixelFormat::P010 => vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
                PixelFormat::Nv24 => vk::Format::G8_B8R8_2PLANE_444_UNORM,
                PixelFormat::P410 => vk::Format::G10X6_B10X6R10X6_2PLANE_444_UNORM_3PACK16,
                _ => {
                    return Err(Error::InvalidFormat(
                        "unsupported Vulkan YUV format".to_owned(),
                    ));
                }
            };
            vk::Format::from_raw(image.format) == expected
                && image.width == format.width
                && image.height == format.height
                && vk::ImageCreateFlags::from_raw(frame.image_flags)
                    .contains(vk::ImageCreateFlags::MUTABLE_FORMAT)
        }
        [luma, chroma] => {
            let formats = (
                vk::Format::from_raw(luma.format),
                vk::Format::from_raw(chroma.format),
            );
            let valid_formats = if ten_bit {
                formats == (vk::Format::R16_UNORM, vk::Format::R16G16_UNORM)
                    || formats
                        == (
                            vk::Format::R10X6_UNORM_PACK16,
                            vk::Format::R10X6G10X6_UNORM_2PACK16,
                        )
            } else {
                formats == (vk::Format::R8_UNORM, vk::Format::R8G8_UNORM)
            };
            valid_formats
                && luma.image != chroma.image
                && luma.width == format.width
                && luma.height == format.height
                && chroma.width == format.width / chroma_divisor
                && chroma.height == format.height / chroma_divisor
        }
        _ => false,
    };
    if !valid {
        return Err(Error::InvalidFormat(
            "unsupported Vulkan YUV image format, extent, or plane layout".to_owned(),
        ));
    }
    timeline_waits(frame)?;
    Ok(())
}

fn yuv_dmabuf_layout(
    frame: &DmaBufFrame,
    pixel_format: PixelFormat,
) -> Result<(usize, DmaBufPlane, DmaBufPlane)> {
    if !matches!(pixel_format, PixelFormat::Nv12 | PixelFormat::P010) {
        return Err(Error::unavailable(
            Subsystem::Vulkan,
            "DMA-BUF import supports only NV12 and P010",
        ));
    }
    let p010 = pixel_format == PixelFormat::P010;
    let (luma, chroma) = if frame.layers.len() == 1
        && frame.layers[0].format
            == if p010 {
                DRM_FORMAT_P010
            } else {
                DRM_FORMAT_NV12
            }
        && frame.layers[0].planes.len() >= 2
    {
        (frame.layers[0].planes[0], frame.layers[0].planes[1])
    } else if frame.layers.len() >= 2
        && frame.layers[0].format == if p010 { DRM_FORMAT_R16 } else { DRM_FORMAT_R8 }
        && frame.layers[1].format
            == if p010 {
                DRM_FORMAT_GR1616
            } else {
                DRM_FORMAT_GR88
            }
        && !frame.layers[0].planes.is_empty()
        && !frame.layers[1].planes.is_empty()
    {
        (frame.layers[0].planes[0], frame.layers[1].planes[0])
    } else {
        return Err(Error::unavailable(
            Subsystem::Vulkan,
            "unsupported DMA-BUF NV12/P010 plane layout",
        ));
    };
    if luma.object_index != chroma.object_index {
        return Err(Error::unavailable(
            Subsystem::Vulkan,
            "disjoint multi-object NV12/P010 DMA-BUF import is not supported",
        ));
    }
    Ok((luma.object_index, luma, chroma))
}

fn fd_identity(fd: RawFd) -> Result<(u64, u64)> {
    let mut metadata = std::mem::MaybeUninit::<libc::stat>::zeroed();
    if unsafe { libc::fstat(fd, metadata.as_mut_ptr()) } != 0 {
        return Err(Error::backend(
            Subsystem::Vulkan,
            format!(
                "fstat on DMA-BUF failed: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    let metadata = unsafe { metadata.assume_init() };
    Ok((metadata.st_dev, metadata.st_ino))
}

#[allow(clippy::too_many_arguments)]
fn import_nv12_dmabuf(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    device: &ash::Device,
    render_queue_family: u32,
    fd: RawFd,
    object_size: usize,
    modifier: u64,
    luma: DmaBufPlane,
    chroma: DmaBufPlane,
    source: Arc<DecodedVideoFrame>,
) -> Result<ImportedNv12Frame> {
    if luma.offset >= object_size || chroma.offset >= object_size {
        return Err(Error::InvalidFormat(
            "DMA-BUF plane offset exceeds its object size".to_owned(),
        ));
    }
    let p010 = source.format.pixel_format == PixelFormat::P010;
    let plane_layouts = [
        vk::SubresourceLayout {
            offset: luma.offset as u64,
            size: object_size.saturating_sub(luma.offset) as u64,
            row_pitch: luma.pitch as u64,
            array_pitch: 0,
            depth_pitch: 0,
        },
        vk::SubresourceLayout {
            offset: chroma.offset as u64,
            size: object_size.saturating_sub(chroma.offset) as u64,
            row_pitch: chroma.pitch as u64,
            array_pitch: 0,
            depth_pitch: 0,
        },
    ];
    let mut external = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let mut drm_modifier = vk::ImageDrmFormatModifierExplicitCreateInfoEXT::default()
        .drm_format_modifier(modifier)
        .plane_layouts(&plane_layouts);
    let image_info = vk::ImageCreateInfo::default()
        .push_next(&mut external)
        .push_next(&mut drm_modifier)
        .flags(vk::ImageCreateFlags::MUTABLE_FORMAT)
        .image_type(vk::ImageType::TYPE_2D)
        .format(if p010 {
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
        } else {
            vk::Format::G8_B8R8_2PLANE_420_UNORM
        })
        .extent(vk::Extent3D {
            width: source.format.width,
            height: source.format.height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::DRM_FORMAT_MODIFIER_EXT)
        .usage(vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let image = unsafe { device.create_image(&image_info, None) }
        .map_err(|error| vk_error("create DMA-BUF NV12 image", error))?;
    let requirements = unsafe { device.get_image_memory_requirements(image) };
    let fd_loader = ash::khr::external_memory_fd::Device::new(instance, device);
    let mut fd_properties = vk::MemoryFdPropertiesKHR::default();
    if let Err(error) = unsafe {
        fd_loader.get_memory_fd_properties(
            vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            fd,
            &mut fd_properties,
        )
    } {
        unsafe { device.destroy_image(image, None) };
        return Err(vk_error("query DMA-BUF memory properties", error));
    }
    let memory_bits = requirements.memory_type_bits & fd_properties.memory_type_bits;
    let properties = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    let memory_type = (0..properties.memory_type_count)
        .find(|index| memory_bits & (1 << index) != 0)
        .ok_or_else(|| {
            Error::unavailable(Subsystem::Vulkan, "no memory type can import DMA-BUF")
        })?;
    let imported_fd = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if imported_fd < 0 {
        unsafe { device.destroy_image(image, None) };
        return Err(Error::backend(
            Subsystem::Vulkan,
            format!(
                "duplicate DMA-BUF failed: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    let mut import = vk::ImportMemoryFdInfoKHR::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
        .fd(imported_fd);
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let allocation = vk::MemoryAllocateInfo::default()
        .push_next(&mut import)
        .push_next(&mut dedicated)
        .allocation_size(requirements.size.max(object_size as u64))
        .memory_type_index(memory_type);
    let memory = match unsafe { device.allocate_memory(&allocation, None) } {
        Ok(memory) => memory,
        Err(error) => {
            unsafe {
                libc::close(imported_fd);
                device.destroy_image(image, None);
            }
            return Err(vk_error("import DMA-BUF memory", error));
        }
    };
    if let Err(error) = unsafe { device.bind_image_memory(image, memory, 0) } {
        unsafe {
            device.free_memory(memory, None);
            device.destroy_image(image, None);
        }
        return Err(vk_error("bind DMA-BUF image memory", error));
    }
    let luma_view = match create_plane_view(
        device,
        image,
        if p010 {
            vk::Format::R10X6_UNORM_PACK16
        } else {
            vk::Format::R8_UNORM
        },
        vk::ImageAspectFlags::PLANE_0,
    ) {
        Ok(view) => view,
        Err(error) => {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            return Err(error);
        }
    };
    let chroma_view = match create_plane_view(
        device,
        image,
        if p010 {
            vk::Format::R10X6G10X6_UNORM_2PACK16
        } else {
            vk::Format::R8G8_UNORM
        },
        vk::ImageAspectFlags::PLANE_1,
    ) {
        Ok(view) => view,
        Err(error) => {
            unsafe {
                device.destroy_image_view(luma_view, None);
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            return Err(error);
        }
    };
    Ok(ImportedNv12Frame {
        image: image.as_raw(),
        luma_view: luma_view.as_raw(),
        chroma_view: chroma_view.as_raw(),
        modifier,
        external_queue_family: vk::QUEUE_FAMILY_EXTERNAL,
        render_queue_family,
        image_handle: image,
        luma_view_handle: luma_view,
        chroma_view_handle: chroma_view,
        memory,
        device: device.clone(),
        source,
    })
}

fn create_plane_view(
    device: &ash::Device,
    image: vk::Image,
    format: vk::Format,
    aspect: vk::ImageAspectFlags,
) -> Result<vk::ImageView> {
    let range = vk::ImageSubresourceRange::default()
        .aspect_mask(aspect)
        .level_count(1)
        .layer_count(1);
    let create = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(range);
    unsafe { device.create_image_view(&create, None) }
        .map_err(|error| vk_error("create DMA-BUF plane image view", error))
}

fn vk_error(context: &str, error: vk::Result) -> Error {
    if error == vk::Result::ERROR_DEVICE_LOST {
        return Error::DeviceLost {
            subsystem: Subsystem::Vulkan,
            reason: context.to_owned(),
        };
    }
    Error::backend(Subsystem::Vulkan, format!("{context}: {error:?}"))
}

fn decode_readiness(result: std::result::Result<(), vk::Result>) -> Result<()> {
    result.map_err(|error| {
        if error == vk::Result::TIMEOUT {
            Error::FrameNotReady
        } else {
            vk_error("check Vulkan decoder readiness", error)
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_output_format_preserves_source_precision() {
        assert_eq!(
            GpuTextureFormat::for_pixel_format(PixelFormat::Nv12).unwrap(),
            GpuTextureFormat::Rgba8
        );
        assert_eq!(
            GpuTextureFormat::for_pixel_format(PixelFormat::P010).unwrap(),
            GpuTextureFormat::Rgb10A2
        );
        assert_eq!(
            GpuTextureFormat::Rgba8.vulkan_format(),
            vk::Format::R8G8B8A8_UNORM
        );
        assert_eq!(
            GpuTextureFormat::Rgb10A2.vulkan_format(),
            vk::Format::A2B10G10R10_UNORM_PACK32
        );
        assert_eq!(
            GpuTextureFormat::Rgba16Float.vulkan_format(),
            vk::Format::R16G16B16A16_SFLOAT
        );
        assert!(GpuTextureFormat::for_pixel_format(PixelFormat::I420).is_err());
    }

    #[test]
    fn embedded_output_requires_attachment_and_filtered_sampling_support() {
        let required = vk::FormatFeatureFlags::COLOR_ATTACHMENT
            | vk::FormatFeatureFlags::SAMPLED_IMAGE
            | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
        for format in [
            GpuTextureFormat::Rgba8,
            GpuTextureFormat::Rgb10A2,
            GpuTextureFormat::Rgba16Float,
        ] {
            assert!(format.validate_features(required).is_ok());
            for missing in [
                vk::FormatFeatureFlags::COLOR_ATTACHMENT,
                vk::FormatFeatureFlags::SAMPLED_IMAGE,
                vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR,
            ] {
                let error = format.validate_features(required & !missing).unwrap_err();
                assert!(error.to_string().contains(&format!("{format:?}")));
            }
        }
    }

    #[test]
    fn embedded_slot_reuse_requires_matching_extent_and_precision() {
        for texture_format in [
            GpuTextureFormat::Rgba8,
            GpuTextureFormat::Rgb10A2,
            GpuTextureFormat::Rgba16Float,
        ] {
            let resources = FrameSlotResources {
                width: 1920,
                height: 1080,
                texture_format,
                output: GpuImage {
                    image: vk::Image::null(),
                    memory: vk::DeviceMemory::null(),
                    view: vk::ImageView::null(),
                },
                output_initialized: false,
                framebuffer: vk::Framebuffer::null(),
                cpu: None,
                sand: None,
                owned_input_views: Vec::new(),
                frame: None,
            };
            assert!(resources.reusable(1920, 1080, texture_format));
            assert!(!resources.reusable(1280, 1080, texture_format));
            assert!(!resources.reusable(1920, 720, texture_format));
            let other_format = match texture_format {
                GpuTextureFormat::Rgba8 => GpuTextureFormat::Rgb10A2,
                GpuTextureFormat::Rgb10A2 | GpuTextureFormat::Rgba16Float => {
                    GpuTextureFormat::Rgba8
                }
            };
            assert!(!resources.reusable(1920, 1080, other_format));
        }
    }

    #[test]
    #[ignore = "requires a Vulkan implementation; runs with Mesa lavapipe without /dev/dri"]
    fn embedded_precision_switch_recreates_only_the_recycled_slot() {
        unsafe {
            let entry = ash::Entry::load().unwrap();
            let application = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);
            let instance = entry
                .create_instance(
                    &vk::InstanceCreateInfo::default().application_info(&application),
                    None,
                )
                .unwrap();
            let physical = instance.enumerate_physical_devices().unwrap()[0];
            let family = instance
                .get_physical_device_queue_family_properties(physical)
                .iter()
                .position(|family| family.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                .unwrap() as u32;
            let priorities = [1.0];
            let queues = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(family)
                .queue_priorities(&priorities)];
            let device = instance
                .create_device(
                    physical,
                    &vk::DeviceCreateInfo::default().queue_create_infos(&queues),
                    None,
                )
                .unwrap();
            let render = VulkanRenderDevice {
                instance: instance.handle().as_raw() as usize,
                physical_device: physical.as_raw() as usize,
                device: device.handle().as_raw() as usize,
                queue: device.get_device_queue(family, 0).as_raw() as usize,
                queue_family: family,
                dmabuf_import_enabled: false,
                dmabuf_buffer_import_enabled: false,
            };
            let mut producer = LinuxFrameProducer::new_with_slots(render, 2).unwrap();
            producer.ensure_renderer(GpuTextureFormat::Rgba8).unwrap();
            for slot in 0..2 {
                producer
                    .ensure_slot(slot, 4, 4, GpuTextureFormat::Rgba8)
                    .unwrap();
                producer.slots[slot].as_mut().unwrap().output_initialized = true;
            }
            let other_image = producer.slots[1].as_ref().unwrap().output.image;
            let rgba8_pipeline = producer.renderers[&GpuTextureFormat::Rgba8].pipeline;
            producer.ensure_renderer(GpuTextureFormat::Rgb10A2).unwrap();
            let rgb10_pipeline = producer.renderers[&GpuTextureFormat::Rgb10A2].pipeline;
            assert_ne!(rgba8_pipeline, rgb10_pipeline);
            producer
                .ensure_renderer(GpuTextureFormat::Rgba16Float)
                .unwrap();
            let rgba16_pipeline = producer.renderers[&GpuTextureFormat::Rgba16Float].pipeline;
            assert_ne!(rgba8_pipeline, rgba16_pipeline);
            assert_ne!(rgb10_pipeline, rgba16_pipeline);
            for texture_format in [
                GpuTextureFormat::Rgb10A2,
                GpuTextureFormat::Rgba16Float,
                GpuTextureFormat::Rgba8,
                GpuTextureFormat::Rgb10A2,
                GpuTextureFormat::Rgba16Float,
            ] {
                producer.ensure_renderer(texture_format).unwrap();
                producer.ensure_slot(0, 4, 4, texture_format).unwrap();
                let slot = producer.slots[0].as_mut().unwrap();
                assert_eq!(slot.texture_format, texture_format);
                assert!(!slot.output_initialized);
                let image = slot.output.image;
                slot.output_initialized = true;
                producer.ensure_slot(0, 4, 4, texture_format).unwrap();
                let slot = producer.slots[0].as_ref().unwrap();
                assert_eq!(slot.output.image, image);
                assert!(slot.output_initialized);
                let other_slot = producer.slots[1].as_ref().unwrap();
                assert_eq!(other_slot.output.image, other_image);
                assert_eq!(other_slot.texture_format, GpuTextureFormat::Rgba8);
                assert!(other_slot.output_initialized);
                assert_eq!(producer.renderers.len(), 3);
                assert_eq!(
                    producer.renderers[&GpuTextureFormat::Rgba8].pipeline,
                    rgba8_pipeline
                );
                assert_eq!(
                    producer.renderers[&GpuTextureFormat::Rgb10A2].pipeline,
                    rgb10_pipeline
                );
                assert_eq!(
                    producer.renderers[&GpuTextureFormat::Rgba16Float].pipeline,
                    rgba16_pipeline
                );
            }
            producer
                .ensure_slot(0, 8, 4, GpuTextureFormat::Rgb10A2)
                .unwrap();
            assert_eq!(producer.slots[0].as_ref().unwrap().width, 8);
            assert!(!producer.slots[0].as_ref().unwrap().output_initialized);
            drop(producer);
            device.destroy_device(None);
            instance.destroy_instance(None);
        }
    }

    #[test]
    #[ignore = "requires a Vulkan implementation; runs with Mesa lavapipe without /dev/dri"]
    fn retired_render_resources_survive_session_drop_and_borrowed_device_destruction() {
        unsafe {
            let entry = ash::Entry::load().unwrap();
            let application = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_1);
            let instance = entry
                .create_instance(
                    &vk::InstanceCreateInfo::default().application_info(&application),
                    None,
                )
                .unwrap();
            let physical = instance.enumerate_physical_devices().unwrap()[0];
            let family = instance
                .get_physical_device_queue_family_properties(physical)
                .iter()
                .position(|family| family.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                .unwrap() as u32;
            let priorities = [1.0];
            let queues = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(family)
                .queue_priorities(&priorities)];
            let device = instance
                .create_device(
                    physical,
                    &vk::DeviceCreateInfo::default().queue_create_infos(&queues),
                    None,
                )
                .unwrap();
            let queue = device.get_device_queue(family, 0);
            let pool = device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default().queue_family_index(family),
                    None,
                )
                .unwrap();
            let commands = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(pool)
                        .level(vk::CommandBufferLevel::PRIMARY)
                        .command_buffer_count(1),
                )
                .unwrap();
            device
                .begin_command_buffer(commands[0], &vk::CommandBufferBeginInfo::default())
                .unwrap();
            let producer = LinuxGpuFrameProducer::new(3).unwrap();
            let frame = producer
                .frame(DecodedVideoFrame {
                    format: crate::StreamFormat::video_default(2, 2).unwrap(),
                    planes: vec![
                        FramePlane {
                            data: vec![16; 4].into(),
                            stride: 2,
                            rows: 2,
                        },
                        FramePlane {
                            data: vec![128; 2].into(),
                            stride: 2,
                            rows: 1,
                        },
                    ],
                    dmabuf: None,
                    vulkan: None,
                    timestamp_us: 1,
                })
                .unwrap();
            frame
                .record(
                    VulkanRenderDevice {
                        instance: instance.handle().as_raw() as usize,
                        physical_device: physical.as_raw() as usize,
                        device: device.handle().as_raw() as usize,
                        queue: queue.as_raw() as usize,
                        queue_family: family,
                        dmabuf_import_enabled: false,
                        dmabuf_buffer_import_enabled: false,
                    },
                    commands[0].as_raw() as usize,
                    0,
                )
                .unwrap();
            device.end_command_buffer(commands[0]).unwrap();
            device
                .queue_submit(
                    queue,
                    &[vk::SubmitInfo::default().command_buffers(&commands)],
                    vk::Fence::null(),
                )
                .unwrap();
            let resources = frame.render_resources();
            std::thread::spawn(move || drop(producer)).join().unwrap();
            assert!(resources.state.lock().unwrap().producer.is_some());
            assert!(
                !resources
                    .state
                    .lock()
                    .unwrap()
                    .producer
                    .as_ref()
                    .unwrap()
                    .dmabuf_import_supported
            );
            resources.retire().unwrap();
            resources.retire().unwrap();
            assert!(resources.state.lock().unwrap().producer.is_none());
            device.destroy_command_pool(pool, None);
            device.destroy_device(None);
            instance.destroy_instance(None);
            drop(frame);
            drop(resources);
        }
    }

    #[test]
    fn failed_device_recreation_clears_the_cached_device_identity() {
        let producer = LinuxGpuFrameProducer::new(3).unwrap();
        let previous = VulkanRenderDevice {
            instance: 0,
            physical_device: 0,
            device: 0,
            queue: 0,
            queue_family: 0,
            dmabuf_import_enabled: false,
            dmabuf_buffer_import_enabled: false,
        };
        producer.render_resources.state.lock().unwrap().render = Some(previous);
        let frame = LinuxGpuFrame {
            frame: DecodedVideoFrame {
                format: crate::StreamFormat::video_default(2, 2).unwrap(),
                planes: Vec::new(),
                dmabuf: None,
                vulkan: None,
                timestamp_us: 0,
            },
            producer: producer.clone(),
            sequence: 0,
        };
        let replacement = VulkanRenderDevice {
            queue_family: 1,
            ..previous
        };
        assert!(matches!(
            unsafe { frame.record(replacement, 0, 0) },
            Err(Error::InvalidFormat(_))
        ));
        let state = producer.render_resources.state.lock().unwrap();
        assert_eq!(state.render, None);
        assert!(state.producer.is_none());
        drop(state);
        assert!(matches!(
            unsafe { frame.record(previous, 0, 0) },
            Err(Error::InvalidFormat(_))
        ));
    }

    #[test]
    fn readiness_is_not_a_render_failure_and_device_loss_is_not_hidden() {
        assert!(decode_readiness(Ok(())).is_ok());
        assert!(matches!(
            decode_readiness(Err(vk::Result::TIMEOUT)),
            Err(Error::FrameNotReady)
        ));
        assert!(matches!(
            decode_readiness(Err(vk::Result::ERROR_DEVICE_LOST)),
            Err(Error::DeviceLost { .. })
        ));
    }
    use crate::{DmaBufLayer, DmaBufObject, VulkanImage};

    fn vulkan_frame(images: Vec<VulkanImage>) -> VulkanVideoFrame {
        VulkanVideoFrame::new(1, 2, 3, vec![0], 0, 0, images, 0, None, None, Arc::new(()))
    }

    fn direct_fixture(
        pixel_format: PixelFormat,
    ) -> (VulkanVideoFrame, crate::StreamFormat, VulkanRenderDevice) {
        let p010 = pixel_format.is_ten_bit();
        let chroma_size = if pixel_format.is_444() { 4 } else { 2 };
        let images = [
            (
                10,
                4,
                4,
                if p010 {
                    vk::Format::R16_UNORM
                } else {
                    vk::Format::R8_UNORM
                },
            ),
            (
                11,
                chroma_size,
                chroma_size,
                if p010 {
                    vk::Format::R16G16_UNORM
                } else {
                    vk::Format::R8G8_UNORM
                },
            ),
        ]
        .map(|(image, width, height, format)| VulkanImage {
            image,
            width,
            height,
            format: format.as_raw(),
            layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL.as_raw(),
            access: vk::AccessFlags2::SHADER_SAMPLED_READ.as_raw(),
            semaphore: 12,
            semaphore_value: 1,
            queue_family: vk::QUEUE_FAMILY_IGNORED,
        })
        .to_vec();
        let mut frame = unsafe { vulkan_frame(images).with_completed_gpu_copy() };
        frame.image_usage = vk::ImageUsageFlags::SAMPLED.as_raw();
        let format = crate::StreamFormat {
            pixel_format,
            ..crate::StreamFormat::video_default(4, 4).unwrap()
        };
        let render = VulkanRenderDevice {
            instance: 1,
            physical_device: 2,
            device: 3,
            queue: 4,
            queue_family: 0,
            dmabuf_import_enabled: false,
            dmabuf_buffer_import_enabled: false,
        };
        (frame, format, render)
    }

    #[test]
    #[ignore = "requires a Vulkan implementation; runs with Mesa lavapipe without /dev/dri"]
    fn direct_gpu_conversion_retains_nv12_and_p010_until_slot_retirement() {
        unsafe {
            let entry = ash::Entry::load().unwrap();
            let application = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_2);
            let instance = entry
                .create_instance(
                    &vk::InstanceCreateInfo::default().application_info(&application),
                    None,
                )
                .unwrap();
            let physical = instance.enumerate_physical_devices().unwrap()[0];
            let family = instance
                .get_physical_device_queue_family_properties(physical)
                .iter()
                .position(|family| family.queue_flags.contains(vk::QueueFlags::GRAPHICS))
                .unwrap() as u32;
            let priorities = [1.0];
            let queues = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(family)
                .queue_priorities(&priorities)];
            let mut features =
                vk::PhysicalDeviceTimelineSemaphoreFeatures::default().timeline_semaphore(true);
            let device = instance
                .create_device(
                    physical,
                    &vk::DeviceCreateInfo::default()
                        .queue_create_infos(&queues)
                        .push_next(&mut features),
                    None,
                )
                .unwrap();
            let queue = device.get_device_queue(family, 0);
            let pool = device
                .create_command_pool(
                    &vk::CommandPoolCreateInfo::default()
                        .queue_family_index(family)
                        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                    None,
                )
                .unwrap();
            let commands = device
                .allocate_command_buffers(
                    &vk::CommandBufferAllocateInfo::default()
                        .command_pool(pool)
                        .command_buffer_count(1),
                )
                .unwrap();
            let command = commands[0];
            let render = VulkanRenderDevice {
                instance: instance.handle().as_raw() as usize,
                physical_device: physical.as_raw() as usize,
                device: device.handle().as_raw() as usize,
                queue: queue.as_raw() as usize,
                queue_family: family,
                dmabuf_import_enabled: false,
                dmabuf_buffer_import_enabled: false,
            };
            for (pixel_format, completed_copy) in [
                (PixelFormat::Nv12, false),
                (PixelFormat::P010, false),
                (PixelFormat::Nv12, true),
                (PixelFormat::P010, true),
                (PixelFormat::Nv24, false),
                (PixelFormat::P410, false),
                (PixelFormat::Nv24, true),
                (PixelFormat::P410, true),
            ] {
                let chroma_size = if pixel_format.is_444() { 4 } else { 2 };
                let formats = if pixel_format.is_ten_bit() {
                    [vk::Format::R16_UNORM, vk::Format::R16G16_UNORM]
                } else {
                    [vk::Format::R8_UNORM, vk::Format::R8G8_UNORM]
                };
                let images = [
                    create_gpu_image(
                        &instance,
                        physical,
                        &device,
                        4,
                        4,
                        formats[0],
                        vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                    )
                    .unwrap(),
                    create_gpu_image(
                        &instance,
                        physical,
                        &device,
                        chroma_size,
                        chroma_size,
                        formats[1],
                        vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                    )
                    .unwrap(),
                ];
                device
                    .reset_command_buffer(command, vk::CommandBufferResetFlags::empty())
                    .unwrap();
                device
                    .begin_command_buffer(command, &vk::CommandBufferBeginInfo::default())
                    .unwrap();
                for (index, image) in images.iter().enumerate() {
                    let barrier = [vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::UNDEFINED)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .image(image.image)
                        .subresource_range(image_color_range())];
                    device.cmd_pipeline_barrier(
                        command,
                        vk::PipelineStageFlags::TOP_OF_PIPE,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &barrier,
                    );
                    device.cmd_clear_color_image(
                        command,
                        image.image,
                        vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                        &vk::ClearColorValue {
                            float32: if index == 0 {
                                [0.5; 4]
                            } else {
                                [128.0 / 255.0; 4]
                            },
                        },
                        &[image_color_range()],
                    );
                    let barrier = [vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .image(image.image)
                        .subresource_range(image_color_range())];
                    device.cmd_pipeline_barrier(
                        command,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &barrier,
                    );
                }
                device.end_command_buffer(command).unwrap();
                let mut semaphore_type = vk::SemaphoreTypeCreateInfo::default()
                    .semaphore_type(vk::SemaphoreType::TIMELINE);
                let semaphore = device
                    .create_semaphore(
                        &vk::SemaphoreCreateInfo::default().push_next(&mut semaphore_type),
                        None,
                    )
                    .unwrap();
                let semaphores = [semaphore];
                let values = [1];
                let mut timeline =
                    vk::TimelineSemaphoreSubmitInfo::default().signal_semaphore_values(&values);
                device
                    .queue_submit(
                        queue,
                        &[vk::SubmitInfo::default()
                            .command_buffers(&commands)
                            .signal_semaphores(&semaphores)
                            .push_next(&mut timeline)],
                        vk::Fence::null(),
                    )
                    .unwrap();
                device.queue_wait_idle(queue).unwrap();
                let owner = Arc::new(());
                let weak = Arc::downgrade(&owner);
                let source_images = images
                    .iter()
                    .enumerate()
                    .map(|(index, image)| VulkanImage {
                        image: image.image.as_raw(),
                        format: formats[index].as_raw(),
                        width: if index == 0 { 4 } else { chroma_size },
                        height: if index == 0 { 4 } else { chroma_size },
                        layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL.as_raw(),
                        access: vk::AccessFlags2::SHADER_SAMPLED_READ.as_raw(),
                        semaphore: semaphore.as_raw(),
                        semaphore_value: 1,
                        queue_family: vk::QUEUE_FAMILY_IGNORED,
                    })
                    .collect();
                let vulkan = VulkanVideoFrame::new(
                    render.instance,
                    render.physical_device,
                    render.device,
                    vec![family],
                    vk::ImageUsageFlags::SAMPLED.as_raw(),
                    0,
                    source_images,
                    0,
                    None,
                    None,
                    owner,
                );
                let vulkan = Arc::new(if completed_copy {
                    vulkan.with_completed_gpu_copy()
                } else {
                    vulkan
                });
                let producer = LinuxGpuFrameProducer::new(2).unwrap();
                let frame = producer
                    .frame(DecodedVideoFrame {
                        format: crate::StreamFormat {
                            pixel_format,
                            ..crate::StreamFormat::video_default(4, 4).unwrap()
                        },
                        planes: Vec::new(),
                        dmabuf: None,
                        vulkan: Some(vulkan),
                        timestamp_us: 5,
                    })
                    .unwrap();
                device
                    .reset_command_buffer(command, vk::CommandBufferResetFlags::empty())
                    .unwrap();
                device
                    .begin_command_buffer(command, &vk::CommandBufferBeginInfo::default())
                    .unwrap();
                let output = frame.record(render, command.as_raw() as usize, 0).unwrap();
                assert_ne!(output.image, 0);
                assert_eq!((output.width, output.height), (4, 4));
                assert_eq!(
                    output.texture_format,
                    GpuTextureFormat::for_pixel_format(pixel_format).unwrap()
                );
                device.end_command_buffer(command).unwrap();
                device
                    .queue_submit(
                        queue,
                        &[vk::SubmitInfo::default().command_buffers(&commands)],
                        vk::Fence::null(),
                    )
                    .unwrap();
                device.queue_wait_idle(queue).unwrap();
                if pixel_format.is_ten_bit() {
                    for (slot, transfer, expected_color, expected_texture) in [
                        (
                            1,
                            crate::ColorTransfer::Pq,
                            LinuxTextureColorSpace::Pq2020,
                            GpuTextureFormat::Rgba16Float,
                        ),
                        (
                            0,
                            crate::ColorTransfer::Hlg,
                            LinuxTextureColorSpace::Hlg2020,
                            GpuTextureFormat::Rgba16Float,
                        ),
                        (
                            1,
                            crate::ColorTransfer::Sdr,
                            LinuxTextureColorSpace::Sdr709,
                            GpuTextureFormat::Rgb10A2,
                        ),
                    ] {
                        let mut decoded = frame.frame.clone();
                        decoded.format.color_transfer = transfer;
                        decoded.format.color_primaries = if transfer == crate::ColorTransfer::Sdr {
                            crate::ColorPrimaries::Bt709
                        } else {
                            crate::ColorPrimaries::Bt2020
                        };
                        decoded.format.color_matrix = if transfer == crate::ColorTransfer::Sdr {
                            crate::ColorMatrix::Bt709
                        } else {
                            crate::ColorMatrix::Bt2020
                        };
                        let transition = producer.frame(decoded).unwrap();
                        device
                            .reset_command_buffer(command, vk::CommandBufferResetFlags::empty())
                            .unwrap();
                        device
                            .begin_command_buffer(command, &vk::CommandBufferBeginInfo::default())
                            .unwrap();
                        let result = transition
                            .record(render, command.as_raw() as usize, slot)
                            .unwrap();
                        assert_eq!(result.color_space, expected_color);
                        assert_eq!(result.texture_format, expected_texture);
                        let state = producer.render_resources.state.lock().unwrap();
                        let resources = state.producer.as_ref().unwrap();
                        assert_eq!(resources.renderers.len(), 2);
                        if transfer == crate::ColorTransfer::Pq {
                            assert_eq!(
                                resources.slots[0].as_ref().unwrap().output.image.as_raw(),
                                output.image
                            );
                        }
                        drop(state);
                        device.end_command_buffer(command).unwrap();
                        device
                            .queue_submit(
                                queue,
                                &[vk::SubmitInfo::default().command_buffers(&commands)],
                                vk::Fence::null(),
                            )
                            .unwrap();
                        device.queue_wait_idle(queue).unwrap();
                    }
                }
                drop(frame);
                assert!(weak.upgrade().is_some());
                producer.render_resources.retire().unwrap();
                assert!(weak.upgrade().is_none());
                device.destroy_semaphore(semaphore, None);
                for image in images {
                    destroy_gpu_image(&device, image);
                }
            }
            device.destroy_command_pool(pool, None);
            device.destroy_device(None);
            instance.destroy_instance(None);
        }
    }

    #[test]
    fn direct_conversion_accepts_only_matching_nv12_and_p010_planes() {
        for pixel_format in [PixelFormat::Nv12, PixelFormat::P010] {
            let (mut frame, format, render) = direct_fixture(pixel_format);
            validate_direct_frame(&frame, format, render).unwrap();
            assert_eq!(timeline_waits(&frame).unwrap(), vec![(12, 1)]);
            frame.images[1].width = format.width;
            assert!(validate_direct_frame(&frame, format, render).is_err());
            frame.images[1].width = format.width / 2;
            frame.images[1].format = vk::Format::R8G8B8A8_UNORM.as_raw();
            assert!(validate_direct_frame(&frame, format, render).is_err());
        }
    }

    #[test]
    fn direct_444_conversion_requires_full_resolution_matching_chroma() {
        for pixel_format in [PixelFormat::Nv24, PixelFormat::P410] {
            let (mut frame, format, render) = direct_fixture(pixel_format);
            assert!(validate_direct_frame(&frame, format, render).is_ok());
            frame.images[1].width /= 2;
            assert!(validate_direct_frame(&frame, format, render).is_err());
            frame.images[1].width *= 2;
            frame.images[1].height /= 2;
            assert!(validate_direct_frame(&frame, format, render).is_err());
            frame.images[1].height *= 2;
            frame.images[1].format = vk::Format::R8G8B8A8_UNORM.as_raw();
            assert!(validate_direct_frame(&frame, format, render).is_err());

            let (mut frame, format, render) = direct_fixture(pixel_format);
            frame.images.truncate(1);
            frame.image_flags = vk::ImageCreateFlags::MUTABLE_FORMAT.as_raw();
            frame.images[0].format = if pixel_format.is_ten_bit() {
                vk::Format::G10X6_B10X6R10X6_2PLANE_444_UNORM_3PACK16
            } else {
                vk::Format::G8_B8R8_2PLANE_444_UNORM
            }
            .as_raw();
            assert!(validate_direct_frame(&frame, format, render).is_ok());
            frame.images[0].format = if pixel_format.is_ten_bit() {
                vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
            } else {
                vk::Format::G8_B8R8_2PLANE_420_UNORM
            }
            .as_raw();
            assert!(validate_direct_frame(&frame, format, render).is_err());
        }
    }

    #[test]
    fn p410_hdr_preserves_transfer_and_output_precision() {
        let mut format = crate::StreamFormat {
            pixel_format: PixelFormat::P410,
            ..crate::StreamFormat::video_default(1920, 1080).unwrap()
        };
        assert_eq!(
            GpuTextureFormat::for_stream_format(format).unwrap(),
            GpuTextureFormat::Rgb10A2
        );
        format.color_primaries = crate::ColorPrimaries::Bt2020;
        format.color_matrix = crate::ColorMatrix::Bt2020;
        for (transfer, expected) in [
            (crate::ColorTransfer::Pq, LinuxTextureColorSpace::Pq2020),
            (crate::ColorTransfer::Hlg, LinuxTextureColorSpace::Hlg2020),
        ] {
            format.color_transfer = transfer;
            assert_eq!(
                LinuxTextureColorSpace::from_format(format).unwrap(),
                expected
            );
            assert_eq!(
                GpuTextureFormat::for_stream_format(format).unwrap(),
                GpuTextureFormat::Rgba16Float
            );
        }
    }

    #[test]
    fn hdr_output_precision_follows_explicit_color_metadata() {
        let mut format = crate::StreamFormat::video_default(3840, 2160).unwrap();
        assert_eq!(
            GpuTextureFormat::for_stream_format(format).unwrap(),
            GpuTextureFormat::Rgba8
        );
        format.pixel_format = PixelFormat::P010;
        assert_eq!(
            GpuTextureFormat::for_stream_format(format).unwrap(),
            GpuTextureFormat::Rgb10A2
        );
        format.color_transfer = crate::ColorTransfer::Pq;
        assert!(LinuxTextureColorSpace::from_format(format).is_err());
        format.color_primaries = crate::ColorPrimaries::Bt2020;
        assert_eq!(
            LinuxTextureColorSpace::from_format(format).unwrap(),
            LinuxTextureColorSpace::Pq2020
        );
        assert_eq!(
            GpuTextureFormat::for_stream_format(format).unwrap(),
            GpuTextureFormat::Rgba16Float
        );
        format.color_transfer = crate::ColorTransfer::Hlg;
        assert_eq!(
            LinuxTextureColorSpace::from_format(format).unwrap(),
            LinuxTextureColorSpace::Hlg2020
        );
        assert_eq!(
            GpuTextureFormat::for_stream_format(format).unwrap(),
            GpuTextureFormat::Rgba16Float
        );
        format.pixel_format = PixelFormat::Nv12;
        assert!(GpuTextureFormat::for_stream_format(format).is_err());
        format.pixel_format = PixelFormat::P010;
        format.color_transfer = crate::ColorTransfer::Sdr;
        assert!(LinuxTextureColorSpace::from_format(format).is_err());
    }

    #[test]
    fn p010_dmabuf_layout_requires_matching_storage_and_one_object() {
        let luma = DmaBufPlane {
            object_index: 0,
            offset: 0,
            pitch: 128,
        };
        let chroma = DmaBufPlane {
            object_index: 0,
            offset: 8192,
            pitch: 128,
        };
        let mut frame = DmaBufFrame::new(
            vec![],
            vec![crate::DmaBufLayer {
                format: DRM_FORMAT_P010,
                planes: vec![luma, chroma],
            }],
            Arc::new(()),
        );
        assert_eq!(
            yuv_dmabuf_layout(&frame, PixelFormat::P010).unwrap(),
            (0, luma, chroma)
        );
        assert!(yuv_dmabuf_layout(&frame, PixelFormat::Nv24).is_err());
        assert!(yuv_dmabuf_layout(&frame, PixelFormat::P410).is_err());
        assert!(yuv_dmabuf_layout(&frame, PixelFormat::Nv12).is_err());
        frame.layers = vec![
            crate::DmaBufLayer {
                format: DRM_FORMAT_R16,
                planes: vec![luma],
            },
            crate::DmaBufLayer {
                format: DRM_FORMAT_GR1616,
                planes: vec![chroma],
            },
        ];
        assert_eq!(
            yuv_dmabuf_layout(&frame, PixelFormat::P010).unwrap(),
            (0, luma, chroma)
        );
        frame.layers[1].planes[0].object_index = 1;
        assert!(yuv_dmabuf_layout(&frame, PixelFormat::P010).is_err());
    }

    #[test]
    fn completed_copy_still_requires_gpu_timeline_handoff() {
        let (mut frame, format, render) = direct_fixture(PixelFormat::Nv12);
        for image in &mut frame.images {
            image.semaphore = 0;
            image.semaphore_value = 0;
        }
        assert!(frame.completed_gpu_copy());
        assert!(validate_direct_frame(&frame, format, render).is_err());
    }

    #[test]
    fn direct_conversion_rejects_foreign_devices_dpb_and_exclusive_families() {
        let (mut frame, format, render) = direct_fixture(PixelFormat::Nv12);
        for foreign in [
            VulkanRenderDevice {
                instance: 9,
                ..render
            },
            VulkanRenderDevice {
                physical_device: 9,
                ..render
            },
            VulkanRenderDevice {
                device: 9,
                ..render
            },
            VulkanRenderDevice {
                queue_family: 9,
                ..render
            },
        ] {
            assert!(validate_direct_frame(&frame, format, foreign).is_err());
        }
        frame.image_usage |= vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR.as_raw();
        assert!(validate_direct_frame(&frame, format, render).is_err());
        frame.image_usage = vk::ImageUsageFlags::SAMPLED.as_raw();
        frame.images[0].queue_family = 1;
        assert!(validate_direct_frame(&frame, format, render).is_err());
        frame.images[0].queue_family = vk::QUEUE_FAMILY_IGNORED;
        frame.images[0].layout = vk::ImageLayout::UNDEFINED.as_raw();
        assert!(validate_direct_frame(&frame, format, render).is_err());
    }

    #[test]
    fn multiplanar_p010_requires_mutable_format_and_matching_depth() {
        let (mut frame, format, render) = direct_fixture(PixelFormat::P010);
        frame.images.truncate(1);
        frame.images[0].format = vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16.as_raw();
        assert!(validate_direct_frame(&frame, format, render).is_err());
        frame.image_flags = vk::ImageCreateFlags::MUTABLE_FORMAT.as_raw();
        validate_direct_frame(&frame, format, render).unwrap();
        let nv12 = crate::StreamFormat {
            pixel_format: PixelFormat::Nv12,
            ..format
        };
        assert!(validate_direct_frame(&frame, nv12, render).is_err());
    }

    #[test]
    fn timeline_waits_deduplicate_images_and_keep_latest_value() {
        let frame = vulkan_frame(vec![
            VulkanImage {
                image: 10,
                format: 1,
                width: 4,
                height: 4,
                layout: 1,
                access: 1,
                semaphore: 22,
                semaphore_value: 7,
                queue_family: 0,
            },
            VulkanImage {
                image: 11,
                format: 2,
                width: 2,
                height: 2,
                layout: 1,
                access: 1,
                semaphore: 22,
                semaphore_value: 9,
                queue_family: 0,
            },
        ]);
        assert_eq!(timeline_waits(&frame).unwrap(), vec![(22, 9)]);
    }

    #[test]
    fn frame_without_timeline_signal_cannot_be_rendered_unsynchronized() {
        let frame = vulkan_frame(vec![VulkanImage {
            image: 10,
            format: 1,
            width: 4,
            height: 4,
            layout: 1,
            access: 1,
            semaphore: 0,
            semaphore_value: 0,
            queue_family: 0,
        }]);
        assert!(timeline_waits(&frame).is_err());
    }

    #[test]
    fn dmabuf_layout_accepts_single_and_split_nv12_layers() {
        let object = DmaBufObject {
            fd: 0,
            size: 4096,
            format_modifier: 12,
        };
        let luma = DmaBufPlane {
            object_index: 0,
            offset: 0,
            pitch: 64,
        };
        let chroma = DmaBufPlane {
            object_index: 0,
            offset: 2048,
            pitch: 64,
        };
        let packed = DmaBufFrame::new(
            vec![object],
            vec![DmaBufLayer {
                format: DRM_FORMAT_NV12,
                planes: vec![luma, chroma],
            }],
            Arc::new(()),
        );
        assert_eq!(
            yuv_dmabuf_layout(&packed, PixelFormat::Nv12).unwrap(),
            (0, luma, chroma)
        );

        let split = DmaBufFrame::new(
            vec![object],
            vec![
                DmaBufLayer {
                    format: DRM_FORMAT_R8,
                    planes: vec![luma],
                },
                DmaBufLayer {
                    format: DRM_FORMAT_GR88,
                    planes: vec![chroma],
                },
            ],
            Arc::new(()),
        );
        assert_eq!(
            yuv_dmabuf_layout(&split, PixelFormat::Nv12).unwrap(),
            (0, luma, chroma)
        );
    }

    #[test]
    fn dmabuf_layout_rejects_disjoint_objects() {
        let frame = DmaBufFrame::new(
            vec![
                DmaBufObject {
                    fd: 0,
                    size: 4096,
                    format_modifier: 0,
                },
                DmaBufObject {
                    fd: 1,
                    size: 4096,
                    format_modifier: 0,
                },
            ],
            vec![DmaBufLayer {
                format: DRM_FORMAT_NV12,
                planes: vec![
                    DmaBufPlane {
                        object_index: 0,
                        offset: 0,
                        pitch: 64,
                    },
                    DmaBufPlane {
                        object_index: 1,
                        offset: 0,
                        pitch: 64,
                    },
                ],
            }],
            Arc::new(()),
        );
        assert!(yuv_dmabuf_layout(&frame, PixelFormat::Nv12).is_err());
    }
}
