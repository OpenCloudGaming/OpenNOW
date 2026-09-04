use std::collections::HashMap;
use std::ffi::{CStr, c_void};
use std::io::Cursor;
use std::marker::PhantomData;
use std::num::NonZeroU64;
use std::os::fd::RawFd;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;

use ash::vk::Handle;
use ash::{Entry, Instance, vk};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
    XlibDisplayHandle, XlibWindowHandle,
};

use crate::{
    ColorMatrix, ColorRange, DecodedVideoFrame, DmaBufFrame, DmaBufPlane, Error, PixelFormat,
    Result, Subsystem, VulkanVideoFrame,
};

// The official Linux client uses FIFO and allows its render worker to wait for
// swapchain progress; input remains responsive because XInput2 runs on a
// separate thread. Keep finite timeouts for teardown/reconfiguration failures
// instead of treating ordinary FIFO back-pressure as a dropped frame.
const PRESENT_WAIT_NS: u64 = 1_000_000_000;
const ACQUIRE_WAIT_NS: u64 = 50_000_000;
const NV12_VERTEX_SHADER: &[u8] = include_bytes!("../shaders/nv12.vert.spv");
const NV12_FRAGMENT_SHADER: &[u8] = include_bytes!("../shaders/nv12.frag.spv");
const DRM_FORMAT_NV12: u32 = fourcc(b'N', b'V', b'1', b'2');
const DRM_FORMAT_R8: u32 = fourcc(b'R', b'8', b' ', b' ');
const DRM_FORMAT_GR88: u32 = fourcc(b'G', b'R', b'8', b'8');
const MAX_IMPORTED_DMABUF_IMAGES: usize = 32;

const fn fourcc(a: u8, b: u8, c: u8, d: u8) -> u32 {
    a as u32 | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
}

struct GpuImage {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
}

struct Nv12Images {
    width: u32,
    height: u32,
    luma: GpuImage,
    chroma: GpuImage,
    initialized: bool,
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

struct ImportedDmaBufImage {
    image: vk::Image,
    memory: vk::DeviceMemory,
    luma_view: vk::ImageView,
    chroma_view: vk::ImageView,
}

struct DirectVulkanViews {
    luma: vk::ImageView,
    chroma: vk::ImageView,
}

pub enum NativeSurface<'a> {
    X11 {
        display: NonNull<c_void>,
        window: NonZeroU64,
        screen: i32,
        _owner: PhantomData<&'a mut c_void>,
    },
    Wayland {
        display: NonNull<c_void>,
        surface: NonNull<c_void>,
        _owner: PhantomData<&'a mut c_void>,
    },
}

impl<'a> NativeSurface<'a> {
    /// Borrows an Xlib display and window owned by the caller.
    ///
    /// # Safety
    ///
    /// `display` and `window` must identify a live X11 surface, Xlib threading must be initialized
    /// when other threads use the display, and both handles must outlive the returned value and
    /// every presenter borrowing it. The backend never closes or destroys either handle.
    pub unsafe fn borrow_x11(display: NonNull<c_void>, window: NonZeroU64, screen: i32) -> Self {
        Self::X11 {
            display,
            window,
            screen,
            _owner: PhantomData,
        }
    }

    /// Borrows a Wayland display and wl_surface owned by the caller.
    ///
    /// # Safety
    ///
    /// Both pointers must remain live, the surface must be used on its owning Wayland thread, and
    /// they must outlive the returned value and every presenter borrowing it. The backend never
    /// disconnects the display or destroys the wl_surface.
    pub unsafe fn borrow_wayland(display: NonNull<c_void>, surface: NonNull<c_void>) -> Self {
        Self::Wayland {
            display,
            surface,
            _owner: PhantomData,
        }
    }

    fn raw_display_handle(&self) -> RawDisplayHandle {
        match self {
            Self::X11 {
                display, screen, ..
            } => RawDisplayHandle::Xlib(XlibDisplayHandle::new(Some(*display), *screen)),
            Self::Wayland { display, .. } => {
                RawDisplayHandle::Wayland(WaylandDisplayHandle::new(*display))
            }
        }
    }

    fn raw_window_handle(&self) -> RawWindowHandle {
        match self {
            Self::X11 { window, .. } => {
                RawWindowHandle::Xlib(XlibWindowHandle::new(window.get() as libc::c_ulong))
            }
            Self::Wayland { surface, .. } => {
                RawWindowHandle::Wayland(WaylandWindowHandle::new(*surface))
            }
        }
    }
}

pub struct VulkanPresenter {
    _entry: Entry,
    instance: Instance,
    surface_loader: ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    physical_device: vk::PhysicalDevice,
    device: ash::Device,
    owns_device: bool,
    queue: vk::Queue,
    queue_family: u32,
    swapchain_loader: ash::khr::swapchain::Device,
    swapchain: vk::SwapchainKHR,
    images: Vec<vk::Image>,
    image_views: Vec<vk::ImageView>,
    framebuffers: Vec<vk::Framebuffer>,
    surface_format: vk::SurfaceFormatKHR,
    extent: vk::Extent2D,
    render_pass: vk::RenderPass,
    descriptor_set_layout: vk::DescriptorSetLayout,
    pipeline_layout: vk::PipelineLayout,
    pipeline: vk::Pipeline,
    descriptor_pool: vk::DescriptorPool,
    descriptor_set: vk::DescriptorSet,
    sampler: vk::Sampler,
    nv12_images: Option<Nv12Images>,
    dmabuf_import_supported: bool,
    dmabuf_import_reported: bool,
    imported_dmabufs: HashMap<DmaBufKey, ImportedDmaBufImage>,
    in_flight_dmabuf: Option<Arc<DmaBufFrame>>,
    direct_views: HashMap<Vec<u64>, DirectVulkanViews>,
    shared_device_owner: Option<Arc<VulkanVideoFrame>>,
    in_flight_vulkan: Option<Arc<VulkanVideoFrame>>,
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    image_available: vk::Semaphore,
    render_finished: Vec<vk::Semaphore>,
    in_flight: vk::Fence,
    staging_buffer: vk::Buffer,
    staging_memory: vk::DeviceMemory,
    staging_capacity: vk::DeviceSize,
    needs_reconfigure: bool,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl VulkanPresenter {
    pub fn new(target: &NativeSurface<'_>, width: u32, height: u32) -> Result<Self> {
        Self::new_inner(target, width, height, None)
    }

    pub fn new_for_vulkan_video(
        target: &NativeSurface<'_>,
        width: u32,
        height: u32,
        frame: &VulkanVideoFrame,
    ) -> Result<Self> {
        Self::new_inner(target, width, height, Some(frame))
    }

    fn new_inner(
        target: &NativeSurface<'_>,
        width: u32,
        height: u32,
        shared: Option<&VulkanVideoFrame>,
    ) -> Result<Self> {
        validate_presentation_extent(width, height)?;
        let entry = unsafe { Entry::load() }
            .map_err(|error| Error::unavailable(Subsystem::Vulkan, error.to_string()))?;
        let owns_device = shared.is_none();
        let instance = if let Some(shared) = shared {
            unsafe {
                Instance::load(
                    entry.static_fn(),
                    vk::Instance::from_raw(shared.instance as u64),
                )
            }
        } else {
            let application_name = c"OpenNOW";
            let application = vk::ApplicationInfo::default()
                .application_name(application_name)
                .application_version(vk::make_api_version(0, 0, 1, 0))
                .engine_name(application_name)
                .engine_version(vk::make_api_version(0, 0, 1, 0))
                .api_version(vk::API_VERSION_1_1);
            let extensions = ash_window::enumerate_required_extensions(target.raw_display_handle())
                .map_err(|error| vk_error("enumerate WSI extensions", error))?;
            let instance_info = vk::InstanceCreateInfo::default()
                .application_info(&application)
                .enabled_extension_names(extensions);
            unsafe { entry.create_instance(&instance_info, None) }
                .map_err(|error| vk_error("create instance", error))?
        };
        let surface = match unsafe {
            ash_window::create_surface(
                &entry,
                &instance,
                target.raw_display_handle(),
                target.raw_window_handle(),
                None,
            )
        } {
            Ok(surface) => surface,
            Err(error) => {
                if owns_device {
                    unsafe { instance.destroy_instance(None) };
                }
                return Err(vk_error("create native surface", error));
            }
        };
        let surface_loader = ash::khr::surface::Instance::new(&entry, &instance);
        let selected = if let Some(shared) = shared {
            select_shared_queue(&instance, &surface_loader, surface, shared)
        } else {
            select_device(&instance, &surface_loader, surface)
        };
        let (physical_device, queue_family) = match selected {
            Ok(selected) => selected,
            Err(error) => {
                unsafe {
                    surface_loader.destroy_surface(surface, None);
                    if owns_device {
                        instance.destroy_instance(None);
                    }
                }
                return Err(error);
            }
        };
        let dma_buf_extensions = [
            ash::khr::external_memory::NAME,
            ash::khr::external_memory_fd::NAME,
            ash::ext::external_memory_dma_buf::NAME,
            ash::ext::image_drm_format_modifier::NAME,
        ];
        let dmabuf_import_supported =
            supports_device_extensions(&instance, physical_device, &dma_buf_extensions)
                .unwrap_or(false);
        let device = if let Some(shared) = shared {
            unsafe {
                ash::Device::load(
                    instance.fp_v1_0(),
                    vk::Device::from_raw(shared.device as u64),
                )
            }
        } else {
            let priority = [1.0_f32];
            let queue_info = [vk::DeviceQueueCreateInfo::default()
                .queue_family_index(queue_family)
                .queue_priorities(&priority)];
            let mut device_extensions = vec![ash::khr::swapchain::NAME.as_ptr()];
            if dmabuf_import_supported {
                device_extensions.extend(dma_buf_extensions.iter().map(|name| name.as_ptr()));
            }
            let device_info = vk::DeviceCreateInfo::default()
                .queue_create_infos(&queue_info)
                .enabled_extension_names(&device_extensions);
            match unsafe { instance.create_device(physical_device, &device_info, None) } {
                Ok(device) => device,
                Err(error) => {
                    unsafe {
                        surface_loader.destroy_surface(surface, None);
                        instance.destroy_instance(None);
                    }
                    return Err(vk_error("create logical device", error));
                }
            }
        };
        let queue = unsafe { device.get_device_queue(queue_family, 0) };
        let swapchain_loader = ash::khr::swapchain::Device::new(&instance, &device);
        let (command_pool, command_buffer, image_available, in_flight) =
            match create_command_resources(&device, queue_family) {
                Ok(resources) => resources,
                Err(error) => {
                    unsafe {
                        if owns_device {
                            device.destroy_device(None);
                        }
                        surface_loader.destroy_surface(surface, None);
                        if owns_device {
                            instance.destroy_instance(None);
                        }
                    }
                    return Err(error);
                }
            };

        let mut presenter = Self {
            _entry: entry,
            instance,
            surface_loader,
            surface,
            physical_device,
            device,
            owns_device,
            queue,
            queue_family,
            swapchain_loader,
            swapchain: vk::SwapchainKHR::null(),
            images: Vec::new(),
            image_views: Vec::new(),
            framebuffers: Vec::new(),
            surface_format: vk::SurfaceFormatKHR::default(),
            extent: vk::Extent2D { width, height },
            render_pass: vk::RenderPass::null(),
            descriptor_set_layout: vk::DescriptorSetLayout::null(),
            pipeline_layout: vk::PipelineLayout::null(),
            pipeline: vk::Pipeline::null(),
            descriptor_pool: vk::DescriptorPool::null(),
            descriptor_set: vk::DescriptorSet::null(),
            sampler: vk::Sampler::null(),
            nv12_images: None,
            dmabuf_import_supported,
            dmabuf_import_reported: false,
            imported_dmabufs: HashMap::new(),
            in_flight_dmabuf: None,
            direct_views: HashMap::new(),
            shared_device_owner: None,
            in_flight_vulkan: None,
            command_pool,
            command_buffer,
            image_available,
            render_finished: Vec::new(),
            in_flight,
            staging_buffer: vk::Buffer::null(),
            staging_memory: vk::DeviceMemory::null(),
            staging_capacity: 0,
            needs_reconfigure: false,
            _thread_affinity: PhantomData,
        };
        if let Err(error) = presenter.create_swapchain(width, height) {
            drop(presenter);
            return Err(error);
        }
        Ok(presenter)
    }

    pub fn extent(&self) -> (u32, u32) {
        (self.extent.width, self.extent.height)
    }

    pub fn matches_vulkan_video_device(&self, frame: &VulkanVideoFrame) -> bool {
        self.device.handle().as_raw() == frame.device as u64
    }

    pub fn reconfigure(&mut self, width: u32, height: u32) -> Result<()> {
        validate_presentation_extent(width, height)?;
        self.needs_reconfigure = true;
        unsafe { self.device.device_wait_idle() }
            .map_err(|error| vk_error("wait before swapchain rebuild", error))?;
        self.destroy_swapchain();
        self.create_swapchain(width, height)?;
        self.recreate_command_resources()?;
        self.needs_reconfigure = false;
        Ok(())
    }

    /// Presents one frame, returning `false` when the compositor temporarily
    /// has no swapchain image available. A WSI timeout is normal while a
    /// Wayland/X11 surface is being hidden or rearranged and must not tear
    /// down the media session.
    pub fn present(&mut self, frame: &DecodedVideoFrame) -> Result<bool> {
        frame.validate()?;
        if frame.format.pixel_format != PixelFormat::Nv12
            || (frame.vulkan.is_none() && frame.dmabuf.is_none() && frame.planes.len() != 2)
        {
            return Err(Error::InvalidFormat(format!(
                "GPU presentation requires two-plane NV12, received {:?}",
                frame.format.pixel_format
            )));
        }
        if self.needs_reconfigure {
            return Err(Error::backend(
                Subsystem::Vulkan,
                "presentation synchronization is invalid; call VulkanPresenter::reconfigure",
            ));
        }
        match unsafe {
            self.device
                .wait_for_fences(&[self.in_flight], true, PRESENT_WAIT_NS)
        } {
            Ok(()) => {}
            Err(vk::Result::TIMEOUT) | Err(vk::Result::NOT_READY) => return Ok(false),
            Err(error) => return Err(vk_error("wait for presentation", error)),
        }
        // The previous submission no longer references its decoder surface.
        self.in_flight_dmabuf = None;
        self.in_flight_vulkan = None;

        let luma_len = frame.format.width as usize * frame.format.height as usize;
        let direct_vulkan = frame.vulkan.as_ref().map(Arc::clone);
        let direct_image = if let Some(vulkan) = direct_vulkan.as_ref() {
            if self.device.handle().as_raw() != vulkan.device as u64 {
                return Err(Error::backend(
                    Subsystem::Vulkan,
                    "presenter and Vulkan Video decoder do not share a device",
                ));
            }
            let (luma, chroma) = self.ensure_direct_vulkan_views(vulkan)?;
            self.update_descriptors(luma, chroma);
            None
        } else if let Some(dmabuf) = frame.dmabuf.as_ref() {
            if !self.dmabuf_import_supported {
                return Err(Error::unavailable(
                    Subsystem::Vulkan,
                    "Vulkan device lacks DMA-BUF/modifier import extensions",
                ));
            }
            let key =
                self.ensure_imported_dmabuf(dmabuf, frame.format.width, frame.format.height)?;
            let imported = self.imported_dmabufs.get(&key).ok_or_else(|| {
                Error::backend(
                    Subsystem::Vulkan,
                    "DMA-BUF import cache lost the current frame",
                )
            })?;
            let handles = (imported.image, imported.luma_view, imported.chroma_view);
            self.update_descriptors(handles.1, handles.2);
            Some(handles.0)
        } else {
            self.ensure_nv12_images(frame.format.width, frame.format.height)?;
            let chroma_len = luma_len / 2;
            let upload_len = luma_len.checked_add(chroma_len).ok_or_else(|| {
                Error::InvalidFormat("NV12 presentation buffer size overflow".to_owned())
            })?;
            self.ensure_staging(upload_len as vk::DeviceSize)?;
            let mapped = unsafe {
                self.device.map_memory(
                    self.staging_memory,
                    0,
                    upload_len as vk::DeviceSize,
                    vk::MemoryMapFlags::empty(),
                )
            }
            .map_err(|error| vk_error("map staging memory", error))?;
            unsafe {
                copy_plane_rows(
                    &frame.planes[0],
                    mapped.cast(),
                    frame.format.width as usize,
                    frame.format.height as usize,
                );
                copy_plane_rows(
                    &frame.planes[1],
                    mapped.cast::<u8>().add(luma_len),
                    frame.format.width as usize,
                    frame.format.height as usize / 2,
                );
                self.device.unmap_memory(self.staging_memory);
            }
            None
        };
        let (image_index, acquire_suboptimal) = match unsafe {
            self.swapchain_loader.acquire_next_image(
                self.swapchain,
                ACQUIRE_WAIT_NS,
                self.image_available,
                vk::Fence::null(),
            )
        } {
            Ok(result) => result,
            Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                self.needs_reconfigure = true;
                return Err(Error::backend(
                    Subsystem::Vulkan,
                    "surface is out of date; call VulkanPresenter::reconfigure",
                ));
            }
            Err(vk::Result::TIMEOUT) | Err(vk::Result::NOT_READY) => return Ok(false),
            Err(error) => return Err(vk_error("acquire swapchain image", error)),
        };
        let image_index = image_index as usize;
        if image_index >= self.images.len()
            || image_index >= self.framebuffers.len()
            || image_index >= self.render_finished.len()
        {
            self.needs_reconfigure = true;
            return Err(Error::backend(
                Subsystem::Vulkan,
                "Vulkan returned an invalid swapchain image index",
            ));
        }
        unsafe {
            if let Err(error) = self
                .device
                .reset_command_buffer(self.command_buffer, vk::CommandBufferResetFlags::empty())
            {
                self.needs_reconfigure = true;
                return Err(vk_error("reset presentation command buffer", error));
            }
            let begin = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            if let Err(error) = self
                .device
                .begin_command_buffer(self.command_buffer, &begin)
            {
                self.needs_reconfigure = true;
                return Err(vk_error("begin presentation command buffer", error));
            }
            if let Some(vulkan) = direct_vulkan.as_ref() {
                let acquire = vulkan
                    .images
                    .iter()
                    .map(|image| {
                        let source_family = if image.queue_family == vk::QUEUE_FAMILY_IGNORED
                            || image.queue_family == self.queue_family
                        {
                            vk::QUEUE_FAMILY_IGNORED
                        } else {
                            image.queue_family
                        };
                        vk::ImageMemoryBarrier::default()
                            .old_layout(vk::ImageLayout::from_raw(image.layout))
                            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                            .src_queue_family_index(source_family)
                            .dst_queue_family_index(if source_family == vk::QUEUE_FAMILY_IGNORED {
                                vk::QUEUE_FAMILY_IGNORED
                            } else {
                                self.queue_family
                            })
                            .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                            .dst_access_mask(vk::AccessFlags::SHADER_READ)
                            .image(vk::Image::from_raw(image.image))
                            .subresource_range(color_range())
                    })
                    .collect::<Vec<_>>();
                self.device.cmd_pipeline_barrier(
                    self.command_buffer,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &acquire,
                );
            } else if let Some(image) = direct_image {
                let acquire = [vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::GENERAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                    .dst_queue_family_index(self.queue_family)
                    .src_access_mask(vk::AccessFlags::MEMORY_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .image(image)
                    .subresource_range(color_range())];
                self.device.cmd_pipeline_barrier(
                    self.command_buffer,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &acquire,
                );
            } else {
                let nv12 = self.nv12_images.as_ref().ok_or_else(|| {
                    Error::backend(Subsystem::Vulkan, "NV12 GPU images are unavailable")
                })?;
                let source_stage = if nv12.initialized {
                    vk::PipelineStageFlags::FRAGMENT_SHADER
                } else {
                    vk::PipelineStageFlags::TOP_OF_PIPE
                };
                let source_access = if nv12.initialized {
                    vk::AccessFlags::SHADER_READ
                } else {
                    vk::AccessFlags::empty()
                };
                let old_layout = if nv12.initialized {
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                } else {
                    vk::ImageLayout::UNDEFINED
                };
                let to_transfer = [nv12.luma.image, nv12.chroma.image].map(|image| {
                    vk::ImageMemoryBarrier::default()
                        .old_layout(old_layout)
                        .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .src_access_mask(source_access)
                        .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .image(image)
                        .subresource_range(color_range())
                });
                self.device.cmd_pipeline_barrier(
                    self.command_buffer,
                    source_stage,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &to_transfer,
                );
                let luma_copy = vk::BufferImageCopy::default()
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D {
                        width: frame.format.width,
                        height: frame.format.height,
                        depth: 1,
                    });
                let chroma_copy = vk::BufferImageCopy::default()
                    .buffer_offset(luma_len as vk::DeviceSize)
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(vk::Extent3D {
                        width: frame.format.width / 2,
                        height: frame.format.height / 2,
                        depth: 1,
                    });
                self.device.cmd_copy_buffer_to_image(
                    self.command_buffer,
                    self.staging_buffer,
                    nv12.luma.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[luma_copy],
                );
                self.device.cmd_copy_buffer_to_image(
                    self.command_buffer,
                    self.staging_buffer,
                    nv12.chroma.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[chroma_copy],
                );
                let to_sample = [nv12.luma.image, nv12.chroma.image].map(|image| {
                    vk::ImageMemoryBarrier::default()
                        .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                        .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .dst_access_mask(vk::AccessFlags::SHADER_READ)
                        .image(image)
                        .subresource_range(color_range())
                });
                self.device.cmd_pipeline_barrier(
                    self.command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &to_sample,
                );
            }
            let clear_values = [vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 1.0],
                },
            }];
            let render_area = vk::Rect2D::default().extent(self.extent);
            let render_begin = vk::RenderPassBeginInfo::default()
                .render_pass(self.render_pass)
                .framebuffer(self.framebuffers[image_index])
                .render_area(render_area)
                .clear_values(&clear_values);
            self.device.cmd_begin_render_pass(
                self.command_buffer,
                &render_begin,
                vk::SubpassContents::INLINE,
            );
            let viewport = vk::Viewport::default()
                .width(self.extent.width as f32)
                .height(self.extent.height as f32)
                .max_depth(1.0);
            self.device
                .cmd_set_viewport(self.command_buffer, 0, &[viewport]);
            self.device
                .cmd_set_scissor(self.command_buffer, 0, &[render_area]);
            self.device.cmd_bind_pipeline(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline,
            );
            self.device.cmd_bind_descriptor_sets(
                self.command_buffer,
                vk::PipelineBindPoint::GRAPHICS,
                self.pipeline_layout,
                0,
                &[self.descriptor_set],
                &[],
            );
            let constants = conversion_constants(frame, self.extent);
            let constants = std::slice::from_raw_parts(
                (&constants as *const ConversionConstants).cast::<u8>(),
                std::mem::size_of::<ConversionConstants>(),
            );
            self.device.cmd_push_constants(
                self.command_buffer,
                self.pipeline_layout,
                vk::ShaderStageFlags::FRAGMENT,
                0,
                constants,
            );
            self.device.cmd_draw(self.command_buffer, 3, 1, 0, 0);
            self.device.cmd_end_render_pass(self.command_buffer);
            if let Some(vulkan) = direct_vulkan.as_ref() {
                let release = vulkan
                    .images
                    .iter()
                    .map(|image| {
                        let destination_family = if image.queue_family == vk::QUEUE_FAMILY_IGNORED
                            || image.queue_family == self.queue_family
                        {
                            vk::QUEUE_FAMILY_IGNORED
                        } else {
                            image.queue_family
                        };
                        vk::ImageMemoryBarrier::default()
                            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                            .new_layout(vk::ImageLayout::from_raw(image.layout))
                            .src_queue_family_index(
                                if destination_family == vk::QUEUE_FAMILY_IGNORED {
                                    vk::QUEUE_FAMILY_IGNORED
                                } else {
                                    self.queue_family
                                },
                            )
                            .dst_queue_family_index(destination_family)
                            .src_access_mask(vk::AccessFlags::SHADER_READ)
                            .dst_access_mask(vk::AccessFlags::MEMORY_READ)
                            .image(vk::Image::from_raw(image.image))
                            .subresource_range(color_range())
                    })
                    .collect::<Vec<_>>();
                self.device.cmd_pipeline_barrier(
                    self.command_buffer,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &release,
                );
            } else if let Some(image) = direct_image {
                let release = [vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .new_layout(vk::ImageLayout::GENERAL)
                    .src_queue_family_index(self.queue_family)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_EXTERNAL)
                    .src_access_mask(vk::AccessFlags::SHADER_READ)
                    .dst_access_mask(vk::AccessFlags::MEMORY_READ | vk::AccessFlags::MEMORY_WRITE)
                    .image(image)
                    .subresource_range(color_range())];
                self.device.cmd_pipeline_barrier(
                    self.command_buffer,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::PipelineStageFlags::ALL_COMMANDS,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &release,
                );
            } else if let Some(nv12) = self.nv12_images.as_mut() {
                nv12.initialized = true;
            }
            if let Err(error) = self.device.end_command_buffer(self.command_buffer) {
                self.needs_reconfigure = true;
                return Err(vk_error("end presentation command buffer", error));
            }
            let mut wait_semaphores = vec![self.image_available];
            let mut wait_stages = vec![vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT];
            let mut wait_values = vec![0_u64];
            if let Some(vulkan) = direct_vulkan.as_ref() {
                for image in &vulkan.images {
                    if image.semaphore != 0 {
                        wait_semaphores.push(vk::Semaphore::from_raw(image.semaphore));
                        wait_stages.push(vk::PipelineStageFlags::FRAGMENT_SHADER);
                        wait_values.push(image.semaphore_value);
                    }
                }
            }
            let command_buffers = [self.command_buffer];
            let signal_semaphores = [self.render_finished[image_index]];
            let signal_values = [0_u64];
            let mut timeline = vk::TimelineSemaphoreSubmitInfo::default()
                .wait_semaphore_values(&wait_values)
                .signal_semaphore_values(&signal_values);
            let mut submit_info = vk::SubmitInfo::default()
                .wait_semaphores(&wait_semaphores)
                .wait_dst_stage_mask(&wait_stages)
                .command_buffers(&command_buffers)
                .signal_semaphores(&signal_semaphores);
            if direct_vulkan.is_some() {
                submit_info = submit_info.push_next(&mut timeline);
            }
            let submit = [submit_info];
            if let Err(error) = self.device.reset_fences(&[self.in_flight]) {
                self.needs_reconfigure = true;
                return Err(vk_error("reset presentation fence", error));
            }
            if let Some(vulkan) = direct_vulkan.as_ref() {
                vulkan.lock_presentation_queue(self.queue_family);
            }
            let submit_result = self
                .device
                .queue_submit(self.queue, &submit, self.in_flight);
            if let Some(vulkan) = direct_vulkan.as_ref() {
                vulkan.unlock_presentation_queue(self.queue_family);
            }
            if let Err(error) = submit_result {
                self.needs_reconfigure = true;
                return Err(vk_error("submit presentation command buffer", error));
            }
            if let Some(dmabuf) = frame.dmabuf.as_ref() {
                self.in_flight_dmabuf = Some(Arc::clone(dmabuf));
            }
            if let Some(vulkan) = direct_vulkan.as_ref() {
                if self.shared_device_owner.is_none() {
                    self.shared_device_owner = Some(Arc::clone(vulkan));
                }
                self.in_flight_vulkan = Some(Arc::clone(vulkan));
            }
            let swapchains = [self.swapchain];
            let image_indices = [image_index as u32];
            let present = vk::PresentInfoKHR::default()
                .wait_semaphores(&signal_semaphores)
                .swapchains(&swapchains)
                .image_indices(&image_indices);
            if let Some(vulkan) = direct_vulkan.as_ref() {
                vulkan.lock_presentation_queue(self.queue_family);
            }
            let present_result = self.swapchain_loader.queue_present(self.queue, &present);
            if let Some(vulkan) = direct_vulkan.as_ref() {
                vulkan.unlock_presentation_queue(self.queue_family);
            }
            match present_result {
                Ok(false) => {}
                Ok(true) | Err(vk::Result::ERROR_OUT_OF_DATE_KHR) => {
                    self.needs_reconfigure = true;
                    return Err(Error::backend(
                        Subsystem::Vulkan,
                        "surface changed; call VulkanPresenter::reconfigure",
                    ));
                }
                Err(error) => {
                    self.needs_reconfigure = true;
                    return Err(vk_error("present swapchain image", error));
                }
            }
        }
        if acquire_suboptimal {
            self.needs_reconfigure = true;
            return Err(Error::backend(
                Subsystem::Vulkan,
                "surface is suboptimal; call VulkanPresenter::reconfigure",
            ));
        }
        Ok(true)
    }

    fn create_swapchain(&mut self, requested_width: u32, requested_height: u32) -> Result<()> {
        let capabilities = unsafe {
            self.surface_loader
                .get_physical_device_surface_capabilities(self.physical_device, self.surface)
        }
        .map_err(|error| vk_error("query surface capabilities", error))?;
        if !capabilities
            .supported_usage_flags
            .contains(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                "surface swapchain images do not support color-attachment usage",
            ));
        }
        let formats = unsafe {
            self.surface_loader
                .get_physical_device_surface_formats(self.physical_device, self.surface)
        }
        .map_err(|error| vk_error("query surface formats", error))?;
        let present_modes = unsafe {
            self.surface_loader
                .get_physical_device_surface_present_modes(self.physical_device, self.surface)
        }
        .map_err(|error| vk_error("query surface present modes", error))?;
        let present_mode = choose_present_mode(&present_modes);
        self.surface_format = choose_surface_format(&formats)?;
        self.extent = choose_extent(capabilities, requested_width, requested_height);
        validate_presentation_extent(self.extent.width, self.extent.height)?;
        let mut image_count = capabilities.min_image_count.saturating_add(1);
        if capabilities.max_image_count > 0 {
            image_count = image_count.min(capabilities.max_image_count);
        }
        let create = vk::SwapchainCreateInfoKHR::default()
            .surface(self.surface)
            .min_image_count(image_count)
            .image_format(self.surface_format.format)
            .image_color_space(self.surface_format.color_space)
            .image_extent(self.extent)
            .image_array_layers(1)
            .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
            .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
            .pre_transform(capabilities.current_transform)
            .composite_alpha(choose_composite_alpha(
                capabilities.supported_composite_alpha,
            ))
            .present_mode(present_mode)
            .clipped(true);
        self.swapchain = unsafe { self.swapchain_loader.create_swapchain(&create, None) }
            .map_err(|error| vk_error("create swapchain", error))?;
        self.images = unsafe { self.swapchain_loader.get_swapchain_images(self.swapchain) }
            .map_err(|error| vk_error("get swapchain images", error))?;
        eprintln!(
            "Vulkan presenter configured: mode={present_mode:?} images={} extent={}x{}",
            self.images.len(),
            self.extent.width,
            self.extent.height
        );
        let semaphore_info = vk::SemaphoreCreateInfo::default();
        let mut render_finished = Vec::with_capacity(self.images.len());
        for _ in &self.images {
            match unsafe { self.device.create_semaphore(&semaphore_info, None) } {
                Ok(semaphore) => render_finished.push(semaphore),
                Err(error) => {
                    for semaphore in render_finished {
                        unsafe { self.device.destroy_semaphore(semaphore, None) };
                    }
                    return Err(vk_error("create per-image render semaphore", error));
                }
            }
        }
        self.render_finished = render_finished;
        self.create_render_resources()?;
        Ok(())
    }

    fn create_render_resources(&mut self) -> Result<()> {
        let attachment = [vk::AttachmentDescription::default()
            .format(self.surface_format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
            .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)];
        let color_reference = [vk::AttachmentReference::default()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)];
        let subpasses = [vk::SubpassDescription::default()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_reference)];
        let dependencies = [vk::SubpassDependency::default()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(vk::AccessFlags::COLOR_ATTACHMENT_WRITE)];
        let render_pass_info = vk::RenderPassCreateInfo::default()
            .attachments(&attachment)
            .subpasses(&subpasses)
            .dependencies(&dependencies);
        self.render_pass = unsafe { self.device.create_render_pass(&render_pass_info, None) }
            .map_err(|error| vk_error("create NV12 render pass", error))?;

        let descriptor_bindings = [0, 1].map(|binding| {
            vk::DescriptorSetLayoutBinding::default()
                .binding(binding)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .descriptor_count(1)
                .stage_flags(vk::ShaderStageFlags::FRAGMENT)
        });
        let descriptor_layout_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&descriptor_bindings);
        self.descriptor_set_layout = unsafe {
            self.device
                .create_descriptor_set_layout(&descriptor_layout_info, None)
        }
        .map_err(|error| vk_error("create NV12 descriptor layout", error))?;
        let descriptor_layouts = [self.descriptor_set_layout];
        let push_range = [vk::PushConstantRange::default()
            .stage_flags(vk::ShaderStageFlags::FRAGMENT)
            .size(std::mem::size_of::<ConversionConstants>() as u32)];
        let pipeline_layout_info = vk::PipelineLayoutCreateInfo::default()
            .set_layouts(&descriptor_layouts)
            .push_constant_ranges(&push_range);
        self.pipeline_layout = unsafe {
            self.device
                .create_pipeline_layout(&pipeline_layout_info, None)
        }
        .map_err(|error| vk_error("create NV12 pipeline layout", error))?;

        let vertex_words = ash::util::read_spv(&mut Cursor::new(NV12_VERTEX_SHADER))
            .map_err(|error| Error::backend(Subsystem::Vulkan, error.to_string()))?;
        let fragment_words = ash::util::read_spv(&mut Cursor::new(NV12_FRAGMENT_SHADER))
            .map_err(|error| Error::backend(Subsystem::Vulkan, error.to_string()))?;
        let vertex_info = vk::ShaderModuleCreateInfo::default().code(&vertex_words);
        let fragment_info = vk::ShaderModuleCreateInfo::default().code(&fragment_words);
        let vertex_module = unsafe { self.device.create_shader_module(&vertex_info, None) }
            .map_err(|error| vk_error("create NV12 vertex shader", error))?;
        let fragment_module =
            match unsafe { self.device.create_shader_module(&fragment_info, None) } {
                Ok(module) => module,
                Err(error) => {
                    unsafe { self.device.destroy_shader_module(vertex_module, None) };
                    return Err(vk_error("create NV12 fragment shader", error));
                }
            };
        let shader_stages = [
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
        let color_attachment = [vk::PipelineColorBlendAttachmentState::default()
            .color_write_mask(vk::ColorComponentFlags::RGBA)];
        let color_blend =
            vk::PipelineColorBlendStateCreateInfo::default().attachments(&color_attachment);
        let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
        let pipeline_info = [vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stages)
            .vertex_input_state(&vertex_input)
            .input_assembly_state(&input_assembly)
            .viewport_state(&viewport_state)
            .rasterization_state(&rasterization)
            .multisample_state(&multisample)
            .color_blend_state(&color_blend)
            .dynamic_state(&dynamic)
            .layout(self.pipeline_layout)
            .render_pass(self.render_pass)
            .subpass(0)];
        let pipeline_result = unsafe {
            self.device
                .create_graphics_pipelines(vk::PipelineCache::null(), &pipeline_info, None)
        };
        unsafe {
            self.device.destroy_shader_module(fragment_module, None);
            self.device.destroy_shader_module(vertex_module, None);
        }
        self.pipeline = pipeline_result
            .map_err(|(_, error)| vk_error("create NV12 graphics pipeline", error))?[0];

        let sampler_info = vk::SamplerCreateInfo::default()
            .mag_filter(vk::Filter::LINEAR)
            .min_filter(vk::Filter::LINEAR)
            .mipmap_mode(vk::SamplerMipmapMode::NEAREST)
            .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
            .max_lod(0.0);
        self.sampler = unsafe { self.device.create_sampler(&sampler_info, None) }
            .map_err(|error| vk_error("create NV12 sampler", error))?;
        let pool_sizes = [vk::DescriptorPoolSize::default()
            .ty(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(2)];
        let pool_info = vk::DescriptorPoolCreateInfo::default()
            .max_sets(1)
            .pool_sizes(&pool_sizes);
        self.descriptor_pool = unsafe { self.device.create_descriptor_pool(&pool_info, None) }
            .map_err(|error| vk_error("create NV12 descriptor pool", error))?;
        let layouts = [self.descriptor_set_layout];
        let allocate_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.descriptor_pool)
            .set_layouts(&layouts);
        let allocated = unsafe { self.device.allocate_descriptor_sets(&allocate_info) }
            .map_err(|error| vk_error("allocate NV12 descriptor sets", error))?;
        self.descriptor_set = allocated[0];

        for image in &self.images {
            let create = vk::ImageViewCreateInfo::default()
                .image(*image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(self.surface_format.format)
                .subresource_range(color_range());
            let view = unsafe { self.device.create_image_view(&create, None) }
                .map_err(|error| vk_error("create swapchain image view", error))?;
            self.image_views.push(view);
        }
        for view in &self.image_views {
            let attachments = [*view];
            let create = vk::FramebufferCreateInfo::default()
                .render_pass(self.render_pass)
                .attachments(&attachments)
                .width(self.extent.width)
                .height(self.extent.height)
                .layers(1);
            let framebuffer = unsafe { self.device.create_framebuffer(&create, None) }
                .map_err(|error| vk_error("create swapchain framebuffer", error))?;
            self.framebuffers.push(framebuffer);
        }
        if self.nv12_images.is_some() {
            self.update_nv12_descriptors();
        }
        Ok(())
    }

    fn ensure_nv12_images(&mut self, width: u32, height: u32) -> Result<()> {
        if self
            .nv12_images
            .as_ref()
            .is_some_and(|images| images.width == width && images.height == height)
        {
            return Ok(());
        }
        self.destroy_nv12_images();
        let luma = create_sampled_image(
            &self.instance,
            self.physical_device,
            &self.device,
            width,
            height,
            vk::Format::R8_UNORM,
        )?;
        let chroma = match create_sampled_image(
            &self.instance,
            self.physical_device,
            &self.device,
            width / 2,
            height / 2,
            vk::Format::R8G8_UNORM,
        ) {
            Ok(image) => image,
            Err(error) => {
                destroy_gpu_image(&self.device, luma);
                return Err(error);
            }
        };
        self.nv12_images = Some(Nv12Images {
            width,
            height,
            luma,
            chroma,
            initialized: false,
        });
        self.update_nv12_descriptors();
        Ok(())
    }

    fn update_nv12_descriptors(&self) {
        let Some(images) = self.nv12_images.as_ref() else {
            return;
        };
        self.update_descriptors(images.luma.view, images.chroma.view);
    }

    fn update_descriptors(&self, luma_view: vk::ImageView, chroma_view: vk::ImageView) {
        self.update_descriptor_set(self.descriptor_set, luma_view, chroma_view);
    }

    fn update_descriptor_set(
        &self,
        descriptor_set: vk::DescriptorSet,
        luma_view: vk::ImageView,
        chroma_view: vk::ImageView,
    ) {
        if descriptor_set == vk::DescriptorSet::null() {
            return;
        }
        let luma = [vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(luma_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let chroma = [vk::DescriptorImageInfo::default()
            .sampler(self.sampler)
            .image_view(chroma_view)
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&luma),
            vk::WriteDescriptorSet::default()
                .dst_set(descriptor_set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
                .image_info(&chroma),
        ];
        unsafe { self.device.update_descriptor_sets(&writes, &[]) };
    }

    fn ensure_direct_vulkan_views(
        &mut self,
        frame: &VulkanVideoFrame,
    ) -> Result<(vk::ImageView, vk::ImageView)> {
        let key = frame
            .images
            .iter()
            .map(|image| image.image)
            .collect::<Vec<_>>();
        if let Some(views) = self.direct_views.get(&key) {
            return Ok((views.luma, views.chroma));
        }
        if self.direct_views.len() >= MAX_IMPORTED_DMABUF_IMAGES {
            for (_, views) in self.direct_views.drain() {
                unsafe {
                    self.device.destroy_image_view(views.chroma, None);
                    self.device.destroy_image_view(views.luma, None);
                }
            }
        }
        let (luma, chroma) = if frame.images.len() == 1 {
            let image = vk::Image::from_raw(frame.images[0].image);
            (
                create_plane_view(
                    &self.device,
                    image,
                    vk::Format::R8_UNORM,
                    vk::ImageAspectFlags::PLANE_0,
                )?,
                create_plane_view(
                    &self.device,
                    image,
                    vk::Format::R8G8_UNORM,
                    vk::ImageAspectFlags::PLANE_1,
                )?,
            )
        } else {
            let luma = create_plane_view(
                &self.device,
                vk::Image::from_raw(frame.images[0].image),
                vk::Format::from_raw(frame.images[0].format),
                vk::ImageAspectFlags::COLOR,
            )?;
            let chroma = match create_plane_view(
                &self.device,
                vk::Image::from_raw(frame.images[1].image),
                vk::Format::from_raw(frame.images[1].format),
                vk::ImageAspectFlags::COLOR,
            ) {
                Ok(view) => view,
                Err(error) => {
                    unsafe { self.device.destroy_image_view(luma, None) };
                    return Err(error);
                }
            };
            (luma, chroma)
        };
        self.direct_views
            .insert(key, DirectVulkanViews { luma, chroma });
        Ok((luma, chroma))
    }

    fn ensure_imported_dmabuf(
        &mut self,
        frame: &Arc<DmaBufFrame>,
        width: u32,
        height: u32,
    ) -> Result<DmaBufKey> {
        let (object_index, luma, chroma) = nv12_dmabuf_layout(frame)?;
        let object = frame.objects[object_index];
        let (device, inode) = fd_identity(object.fd)?;
        let key = DmaBufKey {
            device,
            inode,
            width,
            height,
            modifier: object.format_modifier,
            luma_offset: luma.offset,
            luma_pitch: luma.pitch,
            chroma_offset: chroma.offset,
            chroma_pitch: chroma.pitch,
        };
        if self.imported_dmabufs.contains_key(&key) {
            return Ok(key);
        }
        if self.imported_dmabufs.len() >= MAX_IMPORTED_DMABUF_IMAGES {
            self.destroy_imported_dmabufs();
        }
        let imported = import_nv12_dmabuf(
            &self.instance,
            self.physical_device,
            &self.device,
            object.fd,
            object.size,
            object.format_modifier,
            luma,
            chroma,
            width,
            height,
        )?;
        self.imported_dmabufs.insert(key, imported);
        if !self.dmabuf_import_reported {
            eprintln!(
                "Vulkan presentation zero-copy enabled: DRM PRIME NV12 DMA-BUF import (modifier {:#x})",
                object.format_modifier
            );
            self.dmabuf_import_reported = true;
        }
        Ok(key)
    }

    fn destroy_imported_dmabufs(&mut self) {
        for (_, imported) in self.imported_dmabufs.drain() {
            destroy_imported_dmabuf(&self.device, imported);
        }
    }

    fn destroy_nv12_images(&mut self) {
        if let Some(images) = self.nv12_images.take() {
            destroy_gpu_image(&self.device, images.chroma);
            destroy_gpu_image(&self.device, images.luma);
        }
    }

    fn recreate_command_resources(&mut self) -> Result<()> {
        unsafe {
            if self.in_flight != vk::Fence::null() {
                self.device.destroy_fence(self.in_flight, None);
            }
            if self.image_available != vk::Semaphore::null() {
                self.device.destroy_semaphore(self.image_available, None);
            }
            if self.command_pool != vk::CommandPool::null() {
                self.device.destroy_command_pool(self.command_pool, None);
            }
        }
        self.command_pool = vk::CommandPool::null();
        self.command_buffer = vk::CommandBuffer::null();
        self.image_available = vk::Semaphore::null();
        self.in_flight = vk::Fence::null();
        let (command_pool, command_buffer, image_available, in_flight) =
            create_command_resources(&self.device, self.queue_family)?;
        self.command_pool = command_pool;
        self.command_buffer = command_buffer;
        self.image_available = image_available;
        self.in_flight = in_flight;
        Ok(())
    }

    fn destroy_swapchain(&mut self) {
        for framebuffer in self.framebuffers.drain(..) {
            unsafe { self.device.destroy_framebuffer(framebuffer, None) };
        }
        for image_view in self.image_views.drain(..) {
            unsafe { self.device.destroy_image_view(image_view, None) };
        }
        unsafe {
            if self.pipeline != vk::Pipeline::null() {
                self.device.destroy_pipeline(self.pipeline, None);
                self.pipeline = vk::Pipeline::null();
            }
            if self.pipeline_layout != vk::PipelineLayout::null() {
                self.device
                    .destroy_pipeline_layout(self.pipeline_layout, None);
                self.pipeline_layout = vk::PipelineLayout::null();
            }
            if self.descriptor_pool != vk::DescriptorPool::null() {
                self.device
                    .destroy_descriptor_pool(self.descriptor_pool, None);
                self.descriptor_pool = vk::DescriptorPool::null();
                self.descriptor_set = vk::DescriptorSet::null();
            }
            if self.descriptor_set_layout != vk::DescriptorSetLayout::null() {
                self.device
                    .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
                self.descriptor_set_layout = vk::DescriptorSetLayout::null();
            }
            if self.sampler != vk::Sampler::null() {
                self.device.destroy_sampler(self.sampler, None);
                self.sampler = vk::Sampler::null();
            }
            if self.render_pass != vk::RenderPass::null() {
                self.device.destroy_render_pass(self.render_pass, None);
                self.render_pass = vk::RenderPass::null();
            }
        }
        for semaphore in self.render_finished.drain(..) {
            unsafe { self.device.destroy_semaphore(semaphore, None) };
        }
        if self.swapchain != vk::SwapchainKHR::null() {
            unsafe {
                self.swapchain_loader
                    .destroy_swapchain(self.swapchain, None)
            };
            self.swapchain = vk::SwapchainKHR::null();
            self.images.clear();
        }
    }

    fn ensure_staging(&mut self, required: vk::DeviceSize) -> Result<()> {
        if required <= self.staging_capacity {
            return Ok(());
        }
        unsafe { self.device.device_wait_idle() }
            .map_err(|error| vk_error("wait before staging resize", error))?;
        if self.staging_buffer != vk::Buffer::null() {
            unsafe {
                self.device.destroy_buffer(self.staging_buffer, None);
                self.device.free_memory(self.staging_memory, None);
            }
            self.staging_buffer = vk::Buffer::null();
            self.staging_memory = vk::DeviceMemory::null();
            self.staging_capacity = 0;
        }
        let buffer_info = vk::BufferCreateInfo::default()
            .size(required)
            .usage(vk::BufferUsageFlags::TRANSFER_SRC)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);
        let staging_buffer = unsafe { self.device.create_buffer(&buffer_info, None) }
            .map_err(|error| vk_error("create staging buffer", error))?;
        let requirements = unsafe { self.device.get_buffer_memory_requirements(staging_buffer) };
        let memory_type = match find_memory_type(
            &self.instance,
            self.physical_device,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        ) {
            Ok(memory_type) => memory_type,
            Err(error) => {
                unsafe { self.device.destroy_buffer(staging_buffer, None) };
                return Err(error);
            }
        };
        let allocation = vk::MemoryAllocateInfo::default()
            .allocation_size(requirements.size)
            .memory_type_index(memory_type);
        let staging_memory = match unsafe { self.device.allocate_memory(&allocation, None) } {
            Ok(memory) => memory,
            Err(error) => {
                unsafe { self.device.destroy_buffer(staging_buffer, None) };
                return Err(vk_error("allocate staging memory", error));
            }
        };
        if let Err(error) = unsafe {
            self.device
                .bind_buffer_memory(staging_buffer, staging_memory, 0)
        } {
            unsafe {
                self.device.destroy_buffer(staging_buffer, None);
                self.device.free_memory(staging_memory, None);
            }
            return Err(vk_error("bind staging memory", error));
        }
        self.staging_buffer = staging_buffer;
        self.staging_memory = staging_memory;
        self.staging_capacity = requirements.size;
        Ok(())
    }
}

#[repr(C)]
struct ConversionConstants {
    texture_scale: [f32; 2],
    color_matrix: u32,
    full_range: u32,
}

fn conversion_constants(frame: &DecodedVideoFrame, extent: vk::Extent2D) -> ConversionConstants {
    let (_, _, fitted_width, fitted_height) = aspect_fit_extent(
        frame.format.width,
        frame.format.height,
        extent.width,
        extent.height,
    );
    ConversionConstants {
        texture_scale: [
            extent.width as f32 / fitted_width.max(1) as f32,
            extent.height as f32 / fitted_height.max(1) as f32,
        ],
        color_matrix: match frame.format.color_matrix {
            ColorMatrix::Bt601 => 0,
            ColorMatrix::Bt709 => 1,
            ColorMatrix::Bt2020 => 2,
        },
        full_range: u32::from(frame.format.color_range == ColorRange::Full),
    }
}

unsafe fn copy_plane_rows(
    plane: &crate::FramePlane,
    destination: *mut u8,
    row_bytes: usize,
    rows: usize,
) {
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
            format!(
                "unsupported DMA-BUF NV12 layout ({} layers: {})",
                frame.layers.len(),
                frame
                    .layers
                    .iter()
                    .map(|layer| format!("{:#010x}/{}", layer.format, layer.planes.len()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
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
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device: &ash::Device,
    fd: RawFd,
    object_size: usize,
    modifier: u64,
    luma: DmaBufPlane,
    chroma: DmaBufPlane,
    width: u32,
    height: u32,
) -> Result<ImportedDmaBufImage> {
    if luma.offset >= object_size || chroma.offset >= object_size {
        return Err(Error::InvalidFormat(
            "DMA-BUF plane offset exceeds its object size".to_owned(),
        ));
    }
    let plane_layouts = [
        vk::SubresourceLayout {
            offset: luma.offset as vk::DeviceSize,
            size: object_size.saturating_sub(luma.offset) as vk::DeviceSize,
            row_pitch: luma.pitch as vk::DeviceSize,
            array_pitch: 0,
            depth_pitch: 0,
        },
        vk::SubresourceLayout {
            offset: chroma.offset as vk::DeviceSize,
            size: object_size.saturating_sub(chroma.offset) as vk::DeviceSize,
            row_pitch: chroma.pitch as vk::DeviceSize,
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
            width,
            height,
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
    let memory_type = match find_memory_type(
        instance,
        physical_device,
        memory_bits,
        vk::MemoryPropertyFlags::empty(),
    ) {
        Ok(index) => index,
        Err(error) => {
            unsafe { device.destroy_image(image, None) };
            return Err(error);
        }
    };
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
        .allocation_size(requirements.size.max(object_size as vk::DeviceSize))
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
    Ok(ImportedDmaBufImage {
        image,
        memory,
        luma_view,
        chroma_view,
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

fn destroy_imported_dmabuf(device: &ash::Device, imported: ImportedDmaBufImage) {
    unsafe {
        device.destroy_image_view(imported.chroma_view, None);
        device.destroy_image_view(imported.luma_view, None);
        device.destroy_image(imported.image, None);
        device.free_memory(imported.memory, None);
    }
}

fn create_sampled_image(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    device: &ash::Device,
    width: u32,
    height: u32,
    format: vk::Format,
) -> Result<GpuImage> {
    let image_info = vk::ImageCreateInfo::default()
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
        .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED);
    let image = unsafe { device.create_image(&image_info, None) }
        .map_err(|error| vk_error("create NV12 plane image", error))?;
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
    let allocation = vk::MemoryAllocateInfo::default()
        .allocation_size(requirements.size)
        .memory_type_index(memory_type);
    let memory = match unsafe { device.allocate_memory(&allocation, None) } {
        Ok(memory) => memory,
        Err(error) => {
            unsafe { device.destroy_image(image, None) };
            return Err(vk_error("allocate NV12 plane memory", error));
        }
    };
    if let Err(error) = unsafe { device.bind_image_memory(image, memory, 0) } {
        unsafe {
            device.free_memory(memory, None);
            device.destroy_image(image, None);
        }
        return Err(vk_error("bind NV12 plane memory", error));
    }
    let view_info = vk::ImageViewCreateInfo::default()
        .image(image)
        .view_type(vk::ImageViewType::TYPE_2D)
        .format(format)
        .subresource_range(color_range());
    let view = match unsafe { device.create_image_view(&view_info, None) } {
        Ok(view) => view,
        Err(error) => {
            unsafe {
                device.free_memory(memory, None);
                device.destroy_image(image, None);
            }
            return Err(vk_error("create NV12 plane view", error));
        }
    };
    Ok(GpuImage {
        image,
        memory,
        view,
    })
}

fn destroy_gpu_image(device: &ash::Device, image: GpuImage) {
    unsafe {
        device.destroy_image_view(image.view, None);
        device.destroy_image(image.image, None);
        device.free_memory(image.memory, None);
    }
}

fn create_command_resources(
    device: &ash::Device,
    queue_family: u32,
) -> Result<(vk::CommandPool, vk::CommandBuffer, vk::Semaphore, vk::Fence)> {
    let command_pool_info = vk::CommandPoolCreateInfo::default()
        .queue_family_index(queue_family)
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
    let command_pool = unsafe { device.create_command_pool(&command_pool_info, None) }
        .map_err(|error| vk_error("create command pool", error))?;
    let command_info = vk::CommandBufferAllocateInfo::default()
        .command_pool(command_pool)
        .level(vk::CommandBufferLevel::PRIMARY)
        .command_buffer_count(1);
    let command_buffer = match unsafe { device.allocate_command_buffers(&command_info) } {
        Ok(buffers) => buffers[0],
        Err(error) => {
            unsafe { device.destroy_command_pool(command_pool, None) };
            return Err(vk_error("allocate command buffer", error));
        }
    };
    let semaphore_info = vk::SemaphoreCreateInfo::default();
    let image_available = match unsafe { device.create_semaphore(&semaphore_info, None) } {
        Ok(semaphore) => semaphore,
        Err(error) => {
            unsafe { device.destroy_command_pool(command_pool, None) };
            return Err(vk_error("create acquire semaphore", error));
        }
    };
    let fence_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);
    let in_flight = match unsafe { device.create_fence(&fence_info, None) } {
        Ok(fence) => fence,
        Err(error) => {
            unsafe {
                device.destroy_semaphore(image_available, None);
                device.destroy_command_pool(command_pool, None);
            }
            return Err(vk_error("create presentation fence", error));
        }
    };
    Ok((command_pool, command_buffer, image_available, in_flight))
}

impl Drop for VulkanPresenter {
    fn drop(&mut self) {
        unsafe {
            let _ = self.device.device_wait_idle();
        }
        self.in_flight_dmabuf = None;
        self.in_flight_vulkan = None;
        self.destroy_swapchain();
        for (_, views) in self.direct_views.drain() {
            unsafe {
                self.device.destroy_image_view(views.chroma, None);
                self.device.destroy_image_view(views.luma, None);
            }
        }
        self.destroy_imported_dmabufs();
        self.destroy_nv12_images();
        unsafe {
            if self.staging_buffer != vk::Buffer::null() {
                self.device.destroy_buffer(self.staging_buffer, None);
                self.device.free_memory(self.staging_memory, None);
            }
            if self.in_flight != vk::Fence::null() {
                self.device.destroy_fence(self.in_flight, None);
            }
            if self.image_available != vk::Semaphore::null() {
                self.device.destroy_semaphore(self.image_available, None);
            }
            if self.command_pool != vk::CommandPool::null() {
                self.device.destroy_command_pool(self.command_pool, None);
            }
            if self.owns_device {
                self.device.destroy_device(None);
            }
            self.surface_loader.destroy_surface(self.surface, None);
            if self.owns_device {
                self.instance.destroy_instance(None);
            }
        }
    }
}

pub(crate) fn probe_vulkan() -> std::result::Result<(Vec<&'static str>, String), String> {
    let entry = unsafe { Entry::load() }.map_err(|error| error.to_string())?;
    let extensions = unsafe { entry.enumerate_instance_extension_properties(None) }
        .map_err(|error| error.to_string())?;
    let has = |name: &CStr| {
        extensions
            .iter()
            .any(|extension| (unsafe { CStr::from_ptr(extension.extension_name.as_ptr()) }) == name)
    };
    if !has(ash::khr::surface::NAME) {
        return Err("Vulkan loader lacks VK_KHR_surface".to_owned());
    }
    let mut window_systems = Vec::new();
    let mut extension_names = vec![ash::khr::surface::NAME.as_ptr()];
    if has(ash::khr::xlib_surface::NAME) {
        window_systems.push("x11");
        extension_names.push(ash::khr::xlib_surface::NAME.as_ptr());
    }
    if has(ash::khr::wayland_surface::NAME) {
        window_systems.push("wayland");
        extension_names.push(ash::khr::wayland_surface::NAME.as_ptr());
    }
    if window_systems.is_empty() {
        return Err("Vulkan loader has neither Xlib nor Wayland WSI".to_owned());
    }
    let create = vk::InstanceCreateInfo::default().enabled_extension_names(&extension_names);
    let instance =
        unsafe { entry.create_instance(&create, None) }.map_err(|error| error.to_string())?;
    let devices = match unsafe { instance.enumerate_physical_devices() } {
        Ok(devices) => devices,
        Err(error) => {
            unsafe { instance.destroy_instance(None) };
            return Err(error.to_string());
        }
    };
    let usable_devices = devices
        .into_iter()
        .filter(|device| {
            supports_swapchain(&instance, *device).unwrap_or(false)
                && unsafe { instance.get_physical_device_queue_family_properties(*device) }
                    .iter()
                    .any(|queue| queue.queue_flags.contains(vk::QueueFlags::GRAPHICS))
        })
        .count();
    unsafe { instance.destroy_instance(None) };
    if usable_devices == 0 {
        Err("Vulkan found no graphics device with VK_KHR_swapchain".to_owned())
    } else {
        let detail = format!(
            "{} Vulkan physical device(s), {} WSI",
            usable_devices,
            window_systems.join(" and ")
        );
        Ok((window_systems, detail))
    }
}

fn select_device(
    instance: &Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
) -> Result<(vk::PhysicalDevice, u32)> {
    let devices = unsafe { instance.enumerate_physical_devices() }
        .map_err(|error| vk_error("enumerate physical devices", error))?;
    for device in devices {
        if !supports_swapchain(instance, device)
            .map_err(|error| vk_error("enumerate device extensions", error))?
        {
            continue;
        }
        let queues = unsafe { instance.get_physical_device_queue_family_properties(device) };
        for (index, properties) in queues.iter().enumerate() {
            if !properties.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                continue;
            }
            let present = unsafe {
                surface_loader.get_physical_device_surface_support(device, index as u32, surface)
            }
            .map_err(|error| vk_error("query queue presentation support", error))?;
            if present {
                return Ok((device, index as u32));
            }
        }
    }
    Err(Error::unavailable(
        Subsystem::Vulkan,
        "no graphics queue can present to the supplied native surface",
    ))
}

fn select_shared_queue(
    instance: &Instance,
    surface_loader: &ash::khr::surface::Instance,
    surface: vk::SurfaceKHR,
    shared: &VulkanVideoFrame,
) -> Result<(vk::PhysicalDevice, u32)> {
    let physical_device = vk::PhysicalDevice::from_raw(shared.physical_device as u64);
    let properties =
        unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
    for queue_family in &shared.queue_families {
        let Some(queue) = properties.get(*queue_family as usize) else {
            continue;
        };
        if !queue.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
            continue;
        }
        let present = unsafe {
            surface_loader.get_physical_device_surface_support(
                physical_device,
                *queue_family,
                surface,
            )
        }
        .map_err(|error| vk_error("query shared queue presentation support", error))?;
        if present {
            return Ok((physical_device, *queue_family));
        }
    }
    Err(Error::unavailable(
        Subsystem::Vulkan,
        "FFmpeg Vulkan device has no enabled graphics queue that can present this surface",
    ))
}

fn supports_swapchain(
    instance: &Instance,
    device: vk::PhysicalDevice,
) -> std::result::Result<bool, vk::Result> {
    let extensions = unsafe { instance.enumerate_device_extension_properties(device) }?;
    Ok(extensions.iter().any(|extension| {
        (unsafe { CStr::from_ptr(extension.extension_name.as_ptr()) }) == ash::khr::swapchain::NAME
    }))
}

fn supports_device_extensions(
    instance: &Instance,
    device: vk::PhysicalDevice,
    required: &[&CStr],
) -> std::result::Result<bool, vk::Result> {
    let extensions = unsafe { instance.enumerate_device_extension_properties(device) }?;
    Ok(required.iter().all(|required| {
        extensions.iter().any(|extension| {
            (unsafe { CStr::from_ptr(extension.extension_name.as_ptr()) }) == *required
        })
    }))
}

fn choose_surface_format(formats: &[vk::SurfaceFormatKHR]) -> Result<vk::SurfaceFormatKHR> {
    for desired in [
        vk::Format::B8G8R8A8_UNORM,
        vk::Format::B8G8R8A8_SRGB,
        vk::Format::R8G8B8A8_UNORM,
        vk::Format::R8G8B8A8_SRGB,
    ] {
        if let Some(format) = formats.iter().find(|format| format.format == desired) {
            return Ok(*format);
        }
    }
    Err(Error::unavailable(
        Subsystem::Vulkan,
        "surface has no 8-bit BGRA or RGBA transfer-destination format",
    ))
}

fn choose_extent(
    capabilities: vk::SurfaceCapabilitiesKHR,
    width: u32,
    height: u32,
) -> vk::Extent2D {
    if capabilities.current_extent.width != u32::MAX {
        return capabilities.current_extent;
    }
    vk::Extent2D {
        width: width.clamp(
            capabilities.min_image_extent.width,
            capabilities.max_image_extent.width,
        ),
        height: height.clamp(
            capabilities.min_image_extent.height,
            capabilities.max_image_extent.height,
        ),
    }
}

fn choose_composite_alpha(supported: vk::CompositeAlphaFlagsKHR) -> vk::CompositeAlphaFlagsKHR {
    [
        vk::CompositeAlphaFlagsKHR::OPAQUE,
        vk::CompositeAlphaFlagsKHR::PRE_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::POST_MULTIPLIED,
        vk::CompositeAlphaFlagsKHR::INHERIT,
    ]
    .into_iter()
    .find(|mode| supported.contains(*mode))
    .unwrap_or(vk::CompositeAlphaFlagsKHR::OPAQUE)
}

/// Match the official Linux client's synchronized swapchain. Frame selection
/// and stale-frame skipping happen before submission, so WSI receives at most
/// one intentional frame per display interval and does not need MAILBOX to
/// hide an upstream backlog.
fn choose_present_mode(supported: &[vk::PresentModeKHR]) -> vk::PresentModeKHR {
    let preferred = [
        vk::PresentModeKHR::FIFO,
        vk::PresentModeKHR::MAILBOX,
        vk::PresentModeKHR::IMMEDIATE,
    ];
    preferred
        .into_iter()
        .find(|mode| supported.contains(mode))
        .or_else(|| supported.first().copied())
        .unwrap_or(vk::PresentModeKHR::FIFO)
}

fn find_memory_type(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
    type_bits: u32,
    required: vk::MemoryPropertyFlags,
) -> Result<u32> {
    let properties = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    (0..properties.memory_type_count)
        .find(|index| {
            type_bits & (1 << index) != 0
                && properties.memory_types[*index as usize]
                    .property_flags
                    .contains(required)
        })
        .ok_or_else(|| {
            Error::unavailable(
                Subsystem::Vulkan,
                "no coherent host-visible memory for the presentation staging buffer",
            )
        })
}

fn color_range() -> vk::ImageSubresourceRange {
    vk::ImageSubresourceRange::default()
        .aspect_mask(vk::ImageAspectFlags::COLOR)
        .level_count(1)
        .layer_count(1)
}

fn aspect_fit_extent(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> (u32, u32, u32, u32) {
    let scale = (target_width as f64 / source_width as f64)
        .min(target_height as f64 / source_height as f64);
    let width = ((source_width as f64 * scale).round().max(1.0) as u32).min(target_width);
    let height = ((source_height as f64 * scale).round().max(1.0) as u32).min(target_height);
    (
        (target_width - width) / 2,
        (target_height - height) / 2,
        width,
        height,
    )
}

#[cfg(test)]
fn yuv_to_rgb(y: u8, u: u8, v: u8, matrix: ColorMatrix, range: ColorRange) -> (u8, u8, u8) {
    let (y, u, v) = match range {
        ColorRange::Limited => (
            ((y as f32 - 16.0) * (255.0 / 219.0)).max(0.0),
            (u as f32 - 128.0) * (255.0 / 224.0),
            (v as f32 - 128.0) * (255.0 / 224.0),
        ),
        ColorRange::Full => (y as f32, u as f32 - 128.0, v as f32 - 128.0),
    };
    let (r, g, b) = match matrix {
        ColorMatrix::Bt601 => (
            y + 1.402 * v,
            y - 0.344_136 * u - 0.714_136 * v,
            y + 1.772 * u,
        ),
        ColorMatrix::Bt709 => (
            y + 1.5748 * v,
            y - 0.187_324 * u - 0.468_124 * v,
            y + 1.8556 * u,
        ),
        ColorMatrix::Bt2020 => (
            y + 1.4746 * v,
            y - 0.164_553 * u - 0.571_353 * v,
            y + 1.8814 * u,
        ),
    };
    (clamp_u8(r), clamp_u8(g), clamp_u8(b))
}

#[cfg(test)]
fn clamp_u8(value: f32) -> u8 {
    value.round().clamp(0.0, 255.0) as u8
}

fn validate_presentation_extent(width: u32, height: u32) -> Result<()> {
    if width == 0 || height == 0 {
        return Err(Error::InvalidFormat(
            "presentation extent must be non-zero".to_owned(),
        ));
    }
    if width > 16_384 || height > 16_384 {
        return Err(Error::InvalidFormat(
            "presentation extent exceeds the backend limit".to_owned(),
        ));
    }
    Ok(())
}

fn vk_error(operation: &str, error: vk::Result) -> Error {
    if error == vk::Result::ERROR_DEVICE_LOST {
        return Error::DeviceLost {
            subsystem: Subsystem::Vulkan,
            reason: format!("{operation}: {error:?}"),
        };
    }
    Error::backend(Subsystem::Vulkan, format!("{operation}: {error:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limited_bt709_black_and_white_are_mapped_correctly() {
        assert_eq!(
            yuv_to_rgb(16, 128, 128, ColorMatrix::Bt709, ColorRange::Limited),
            (0, 0, 0)
        );
        assert_eq!(
            yuv_to_rgb(235, 128, 128, ColorMatrix::Bt709, ColorRange::Limited),
            (255, 255, 255)
        );
    }

    #[test]
    fn presentation_preserves_source_aspect_ratio() {
        assert_eq!(aspect_fit_extent(1920, 1080, 1024, 768), (0, 96, 1024, 576));
        assert_eq!(
            aspect_fit_extent(1024, 768, 1920, 1080),
            (240, 0, 1440, 1080)
        );
    }

    #[test]
    fn gpu_conversion_constants_preserve_range_matrix_and_letterboxing() {
        use std::sync::Arc;

        use crate::{ChromaLocation, FramePlane, StreamFormat};

        let frame = DecodedVideoFrame {
            format: StreamFormat {
                width: 1920,
                height: 1080,
                pixel_format: PixelFormat::Nv12,
                color_range: ColorRange::Full,
                color_matrix: ColorMatrix::Bt2020,
                chroma_location: ChromaLocation::Left,
            },
            planes: vec![
                FramePlane {
                    data: Arc::from(vec![0; 1920 * 1080]),
                    stride: 1920,
                    rows: 1080,
                },
                FramePlane {
                    data: Arc::from(vec![0; 1920 * 540]),
                    stride: 1920,
                    rows: 540,
                },
            ],
            dmabuf: None,
            vulkan: None,
            timestamp_us: 0,
        };
        let constants = conversion_constants(
            &frame,
            vk::Extent2D {
                width: 1024,
                height: 768,
            },
        );
        assert_eq!(constants.texture_scale, [1.0, 768.0 / 576.0]);
        assert_eq!(constants.color_matrix, 2);
        assert_eq!(constants.full_range, 1);
        assert_eq!(std::mem::size_of::<ConversionConstants>(), 16);
    }

    #[test]
    fn present_mode_uses_fifo_vsync_when_supported() {
        assert_eq!(
            choose_present_mode(&[vk::PresentModeKHR::FIFO, vk::PresentModeKHR::MAILBOX]),
            vk::PresentModeKHR::FIFO
        );
        assert_eq!(
            choose_present_mode(&[vk::PresentModeKHR::MAILBOX, vk::PresentModeKHR::IMMEDIATE]),
            vk::PresentModeKHR::MAILBOX
        );
        assert_eq!(
            choose_present_mode(&[vk::PresentModeKHR::FIFO]),
            vk::PresentModeKHR::FIFO
        );
    }
}
