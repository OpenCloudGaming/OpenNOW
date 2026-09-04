use std::collections::{HashMap, hash_map::Entry as HashMapEntry};
use std::io::Cursor;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use ash::vk::{self, Handle};

use crate::{
    DecodedVideoFrame, DmaBufFrame, DmaBufPlane, Error, FramePlane, PixelFormat, Result, Subsystem,
    VulkanVideoFrame,
};

const DRM_FORMAT_NV12: u32 = u32::from_le_bytes(*b"NV12");
const DRM_FORMAT_R8: u32 = u32::from_le_bytes(*b"R8  ");
const DRM_FORMAT_GR88: u32 = u32::from_le_bytes(*b"GR88");
const DECODE_WAIT_TIMEOUT: Duration = Duration::from_secs(1);
const NV12_VERTEX_SHADER: &[u8] = include_bytes!("../shaders/nv12.vert.spv");
const NV12_FRAGMENT_SHADER: &[u8] = include_bytes!("../shaders/nv12.frag.spv");
const DEFAULT_FRAME_SLOTS: u32 = 3;
const MAX_FRAME_SLOTS: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VulkanRenderDevice {
    pub instance: usize,
    pub physical_device: usize,
    pub device: usize,
    pub queue_family: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecordedGpuFrame {
    pub slot: u32,
    pub generation: u64,
    pub image: u64,
    pub image_view: u64,
    pub width: u32,
    pub height: u32,
    pub timestamp_us: u64,
}

#[derive(Clone)]
pub struct LinuxGpuFrameProducer {
    state: Arc<Mutex<SharedProducerState>>,
    sequence: Arc<AtomicU64>,
    slot_count: u32,
}

struct SharedProducerState {
    render: Option<VulkanRenderDevice>,
    producer: Option<LinuxFrameProducer>,
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
            state: Arc::new(Mutex::new(SharedProducerState {
                render: None,
                producer: None,
            })),
            sequence: Arc::new(AtomicU64::new(0)),
            slot_count,
        })
    }

    pub fn frame(&self, frame: DecodedVideoFrame) -> Result<LinuxGpuFrame> {
        frame.validate()?;
        Ok(LinuxGpuFrame {
            frame,
            producer: self.clone(),
            sequence: self.sequence.fetch_add(1, Ordering::Relaxed) + 1,
        })
    }
}

impl LinuxGpuFrame {
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
    /// [`LinuxFrameProducer::record_frame`].
    pub unsafe fn record(
        &self,
        render: VulkanRenderDevice,
        command_buffer: usize,
        frame_slot: u32,
    ) -> Result<RecordedGpuFrame> {
        let mut state = self
            .producer
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.render != Some(render) {
            state.producer = None;
            state.producer = Some(unsafe {
                LinuxFrameProducer::new_with_slots(render, self.producer.slot_count)?
            });
            state.render = Some(render);
        }
        unsafe {
            state
                .producer
                .as_mut()
                .expect("producer initialized")
                .record_frame(self.frame.clone(), command_buffer, frame_slot)
        }
    }
}

impl VulkanRenderDevice {
    fn validate(self) -> Result<()> {
        if self.instance == 0 || self.physical_device == 0 || self.device == 0 {
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
    Cpu(CpuNv12Frame),
}

impl PreparedLinuxFrame {
    pub fn timestamp_us(&self) -> u64 {
        match self {
            Self::Vulkan(frame) => frame.source.timestamp_us,
            Self::DmaBuf(frame) => frame.source.timestamp_us,
            Self::Cpu(frame) => frame.source.timestamp_us,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct DmaBufKey {
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
    renderer: Option<RendererResources>,
    slots: Vec<Option<FrameSlotResources>>,
    generation: u64,
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
    output: GpuImage,
    output_initialized: bool,
    framebuffer: vk::Framebuffer,
    cpu: Option<CpuUploadResources>,
    owned_input_views: Vec<vk::ImageView>,
    frame: Option<PreparedLinuxFrame>,
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
    /// for the lifetime described above.
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
        let required = [
            ash::khr::external_memory::NAME,
            ash::khr::external_memory_fd::NAME,
            ash::ext::external_memory_dma_buf::NAME,
            ash::ext::image_drm_format_modifier::NAME,
        ];
        let available = unsafe { instance.enumerate_device_extension_properties(physical_device) }
            .map_err(|error| vk_error("enumerate Qt Vulkan device extensions", error))?;
        let dmabuf_import_supported = required.iter().all(|required| {
            available.iter().any(|extension| {
                let name = unsafe { std::ffi::CStr::from_ptr(extension.extension_name.as_ptr()) };
                name == *required
            })
        });
        Ok(Self {
            _entry: entry,
            instance,
            physical_device,
            device,
            render,
            dmabuf_import_supported,
            imported: HashMap::new(),
            renderer: None,
            slots: (0..slot_count).map(|_| None).collect(),
            generation: 0,
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
    /// recording state on the QRhi render thread.
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
        let prepared = self.prepare(frame)?;
        self.ensure_renderer()?;
        self.ensure_slot(slot_index, width, height)?;
        let command_buffer = vk::CommandBuffer::from_raw(command_buffer as u64);
        let (luma_view, chroma_view) =
            self.prepare_input(slot_index, command_buffer, &prepared, width, height)?;
        self.update_descriptors(slot_index, luma_view, chroma_view);
        unsafe {
            self.record_conversion(
                slot_index,
                command_buffer,
                &prepared,
                width,
                height,
                color_matrix,
                full_range,
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
            width,
            height,
            timestamp_us,
        })
    }

    pub fn prepare(&mut self, frame: DecodedVideoFrame) -> Result<PreparedLinuxFrame> {
        frame.validate()?;
        if frame.format.pixel_format != PixelFormat::Nv12 {
            return Err(Error::InvalidFormat(format!(
                "embedded Linux presentation requires NV12, received {:?}",
                frame.format.pixel_format
            )));
        }
        let frame = Arc::new(frame);
        let mut decode_waited = frame.vulkan.is_none();
        if let Some(vulkan) = frame.vulkan.as_ref() {
            decode_waited = self.wait_for_decode(vulkan).is_ok();
            if vulkan.device == self.render.device && decode_waited {
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
        }
        let mut gpu_error = None;
        if decode_waited && self.dmabuf_import_supported && frame.dmabuf.is_some() {
            match self.import_dmabuf(Arc::clone(&frame)) {
                Ok(imported) => return Ok(PreparedLinuxFrame::DmaBuf(imported)),
                Err(error) => gpu_error = Some(error),
            }
        }
        if frame.planes.len() == 2 {
            return Ok(PreparedLinuxFrame::Cpu(CpuNv12Frame {
                luma: frame.planes[0].clone(),
                chroma: frame.planes[1].clone(),
                source: frame,
            }));
        }
        if let Some(vulkan) = frame.vulkan.as_ref() {
            let planes = vulkan.download_nv12()?;
            if planes.len() != 2 {
                return Err(Error::InvalidFormat(
                    "Vulkan CPU fallback did not produce two NV12 planes".to_owned(),
                ));
            }
            return Ok(PreparedLinuxFrame::Cpu(CpuNv12Frame {
                luma: planes[0].clone(),
                chroma: planes[1].clone(),
                source: frame,
            }));
        }
        if let Some(error) = gpu_error {
            return Err(error);
        }
        Err(Error::unavailable(
            Subsystem::Vulkan,
            "decoded frame cannot be synchronized with Qt's Vulkan device and has no CPU NV12 fallback",
        ))
    }

    fn wait_for_decode(&self, frame: &VulkanVideoFrame) -> Result<()> {
        let (semaphores, values): (Vec<_>, Vec<_>) = timeline_waits(frame)?
            .into_iter()
            .map(|(semaphore, value)| (vk::Semaphore::from_raw(semaphore), value))
            .unzip();
        let wait = vk::SemaphoreWaitInfo::default()
            .semaphores(&semaphores)
            .values(&values);
        let source_instance;
        let source_device;
        let device = if frame.device == self.render.device {
            &self.device
        } else {
            source_instance = unsafe {
                ash::Instance::load(
                    self._entry.static_fn(),
                    vk::Instance::from_raw(frame.instance as u64),
                )
            };
            source_device = unsafe {
                ash::Device::load(
                    source_instance.fp_v1_0(),
                    vk::Device::from_raw(frame.device as u64),
                )
            };
            &source_device
        };
        unsafe { device.wait_semaphores(&wait, DECODE_WAIT_TIMEOUT.as_nanos() as u64) }
            .map_err(|error| vk_error("host-wait for Vulkan decoder frame", error))
    }

    fn import_dmabuf(&mut self, source: Arc<DecodedVideoFrame>) -> Result<Arc<ImportedNv12Frame>> {
        let dmabuf = source.dmabuf.as_ref().expect("DMA-BUF checked by caller");
        let (object_index, luma, chroma) = nv12_dmabuf_layout(dmabuf)?;
        let object = dmabuf.objects[object_index];
        let (device, inode) = fd_identity(object.fd)?;
        let key = DmaBufKey {
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

    fn ensure_renderer(&mut self) -> Result<()> {
        if self.renderer.is_some() {
            return Ok(());
        }
        let attachment = [vk::AttachmentDescription::default()
            .format(vk::Format::R8G8B8A8_UNORM)
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
        self.renderer = Some(RendererResources {
            render_pass,
            descriptor_set_layout,
            pipeline_layout,
            pipeline,
            descriptor_pool,
            descriptor_sets,
            sampler,
        });
        Ok(())
    }

    fn ensure_slot(&mut self, slot: usize, width: u32, height: u32) -> Result<()> {
        if self.slots[slot]
            .as_ref()
            .is_some_and(|resources| resources.width == width && resources.height == height)
        {
            return Ok(());
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
            vk::Format::R8G8B8A8_UNORM,
            vk::ImageUsageFlags::COLOR_ATTACHMENT | vk::ImageUsageFlags::SAMPLED,
        )?;
        let renderer = self.renderer.as_ref().expect("renderer initialized");
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
            output,
            output_initialized: false,
            framebuffer,
            cpu: None,
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
        resources.frame = None;
        for view in resources.owned_input_views.drain(..) {
            unsafe { self.device.destroy_image_view(view, None) };
        }
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
        let renderer = self.renderer.as_ref().expect("renderer initialized");
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
    ) -> Result<()> {
        let resources = self.slots[slot].as_mut().expect("slot initialized");
        let renderer = self.renderer.as_ref().expect("renderer initialized");
        let mut acquire = Vec::new();
        match prepared {
            PreparedLinuxFrame::Vulkan(frame) => {
                acquire.extend(frame.images.iter().map(|image| {
                    let source_family = if image.source_queue_family == self.render.queue_family {
                        vk::QUEUE_FAMILY_IGNORED
                    } else {
                        image.source_queue_family
                    };
                    vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::from_raw(image.old_layout))
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .src_queue_family_index(source_family)
                        .dst_queue_family_index(if source_family == vk::QUEUE_FAMILY_IGNORED {
                            vk::QUEUE_FAMILY_IGNORED
                        } else {
                            self.render.queue_family
                        })
                        .src_access_mask(vk::AccessFlags::from_raw(image.old_access as u32))
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
            PreparedLinuxFrame::Cpu(_) => {}
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
                    let destination_family =
                        if image.source_queue_family == self.render.queue_family {
                            vk::QUEUE_FAMILY_IGNORED
                        } else {
                            image.source_queue_family
                        };
                    vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .new_layout(vk::ImageLayout::from_raw(image.old_layout))
                        .src_queue_family_index(if destination_family == vk::QUEUE_FAMILY_IGNORED {
                            vk::QUEUE_FAMILY_IGNORED
                        } else {
                            self.render.queue_family
                        })
                        .dst_queue_family_index(destination_family)
                        .src_access_mask(vk::AccessFlags::SHADER_READ)
                        .dst_access_mask(vk::AccessFlags::MEMORY_READ)
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
            PreparedLinuxFrame::Cpu(_) => {}
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
        resources.frame = None;
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
    }
}

#[repr(C)]
struct ConversionConstants {
    texture_scale: [f32; 2],
    color_matrix: u32,
    full_range: u32,
}

impl Drop for LinuxFrameProducer {
    fn drop(&mut self) {
        for resources in std::mem::take(&mut self.slots).into_iter().flatten() {
            self.destroy_slot(resources);
        }
        if let Some(renderer) = self.renderer.take() {
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
        let luma = create_image_view(
            device,
            image,
            vk::Format::R8_UNORM,
            vk::ImageAspectFlags::PLANE_0,
        )?;
        let chroma = match create_image_view(
            device,
            image,
            vk::Format::R8G8_UNORM,
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
            "Vulkan NV12 frame must contain one multiplanar or two plane images".to_owned(),
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
    let memory_type = find_memory_type(
        instance,
        physical_device,
        requirements.memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )?;
    let memory = unsafe {
        device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .allocation_size(requirements.size)
                .memory_type_index(memory_type),
            None,
        )
    }
    .map_err(|error| vk_error("allocate embedded GPU image", error))?;
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
                device.free_memory(memory, None);
                device.destroy_image(image, None);
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

fn nv12_dmabuf_layout(frame: &DmaBufFrame) -> Result<(usize, DmaBufPlane, DmaBufPlane)> {
    let (luma, chroma) = if frame.layers.len() == 1
        && frame.layers[0].format == DRM_FORMAT_NV12
        && frame.layers[0].planes.len() >= 2
    {
        (frame.layers[0].planes[0], frame.layers[0].planes[1])
    } else if frame.layers.len() >= 2
        && frame.layers[0].format == DRM_FORMAT_R8
        && frame.layers[1].format == DRM_FORMAT_GR88
        && !frame.layers[0].planes.is_empty()
        && !frame.layers[1].planes.is_empty()
    {
        (frame.layers[0].planes[0], frame.layers[1].planes[0])
    } else {
        return Err(Error::unavailable(
            Subsystem::Vulkan,
            "unsupported DMA-BUF NV12 plane layout",
        ));
    };
    if luma.object_index != chroma.object_index {
        return Err(Error::unavailable(
            Subsystem::Vulkan,
            "disjoint multi-object NV12 DMA-BUF import is not supported",
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
        .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
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
        vk::Format::R8_UNORM,
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
        vk::Format::R8G8_UNORM,
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

fn vk_error(context: &'static str, error: vk::Result) -> Error {
    Error::backend(Subsystem::Vulkan, format!("{context}: {error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DmaBufLayer, DmaBufObject, VulkanImage};

    fn vulkan_frame(images: Vec<VulkanImage>) -> VulkanVideoFrame {
        VulkanVideoFrame::new(1, 2, 3, vec![0], 0, 0, images, 0, None, None, Arc::new(()))
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
        assert_eq!(nv12_dmabuf_layout(&packed).unwrap(), (0, luma, chroma));

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
        assert_eq!(nv12_dmabuf_layout(&split).unwrap(), (0, luma, chroma));
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
        assert!(nv12_dmabuf_layout(&frame).is_err());
    }
}
