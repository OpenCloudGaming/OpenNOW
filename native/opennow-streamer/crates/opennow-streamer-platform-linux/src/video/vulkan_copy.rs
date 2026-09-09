use std::sync::Arc;
use std::sync::mpsc::{SyncSender, TrySendError, sync_channel};

use ash::{vk, vk::Handle};
use ffmpeg_next::{ffi, frame};

use crate::{
    DecodedVideoFrame, Error, PixelFormat, Result, SharedVulkanDevice, StreamFormat, Subsystem,
    VulkanImage, VulkanVideoFrame,
};

const MAX_SNAPSHOTS: usize = 12;
const COPY_TIMEOUT_NS: u64 = 1_000_000_000;

struct Snapshot {
    _owner: Arc<SharedVulkanDevice>,
    _entry: Arc<ash::Entry>,
    device: ash::Device,
    images: [vk::Image; 2],
    memory: [vk::DeviceMemory; 2],
    formats: [vk::Format; 2],
    pixel_format: PixelFormat,
    width: u32,
    height: u32,
    initialized: bool,
    semaphore: vk::Semaphore,
    semaphore_value: u64,
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_semaphore(self.semaphore, None);
            for image in self.images {
                self.device.destroy_image(image, None);
            }
            for memory in self.memory {
                self.device.free_memory(memory, None);
            }
        }
    }
}

pub(super) struct VulkanCopyPool {
    owner: Arc<SharedVulkanDevice>,
    entry: Arc<ash::Entry>,
    instance: ash::Instance,
    device: ash::Device,
    queue: vk::Queue,
    queue_family: u32,
    queue_index: u32,
    transfer_granularity: vk::Extent3D,
    command_pool: vk::CommandPool,
    command: vk::CommandBuffer,
    fence: vk::Fence,
    pending: bool,
    failed: bool,
    slots: Vec<Arc<Snapshot>>,
    retirement: SyncSender<Retirement>,
    source: Option<frame::Video>,
}

struct Retirement {
    fence: vk::Fence,
    command_pool: vk::CommandPool,
    pending: bool,
    _slots: Vec<Arc<Snapshot>>,
    _source: Option<frame::Video>,
}

fn retire_resources(
    resources: Retirement,
    mut wait: impl FnMut(vk::Fence) -> std::result::Result<(), vk::Result>,
    destroy: impl FnOnce(vk::Fence, vk::CommandPool),
) {
    if resources.pending {
        loop {
            match wait(resources.fence) {
                Ok(()) | Err(vk::Result::ERROR_DEVICE_LOST) => break,
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(10)),
            }
        }
    }
    destroy(resources.fence, resources.command_pool);
}

impl VulkanCopyPool {
    pub(super) fn new(owner: Arc<SharedVulkanDevice>) -> Result<Self> {
        let result = Self::create_pool(Arc::clone(&owner));
        if matches!(&result, Err(Error::DeviceLost { .. })) {
            owner.invalidate();
        }
        result
    }

    fn create_pool(owner: Arc<SharedVulkanDevice>) -> Result<Self> {
        let info = owner.info();
        let entry = Arc::new(
            unsafe { ash::Entry::load() }
                .map_err(|error| Error::backend(Subsystem::Vulkan, error.to_string()))?,
        );
        let instance = unsafe {
            ash::Instance::load(
                entry.static_fn(),
                vk::Instance::from_raw(info.instance as u64),
            )
        };
        let device = unsafe {
            ash::Device::load(instance.fp_v1_0(), vk::Device::from_raw(info.device as u64))
        };
        let (queue_family, queue_index) = owner.copy_queue();
        let families = unsafe {
            instance.get_physical_device_queue_family_properties(vk::PhysicalDevice::from_raw(
                info.physical_device as u64,
            ))
        };
        let transfer_granularity = families
            .get(queue_family as usize)
            .ok_or_else(|| {
                Error::unavailable(Subsystem::Vulkan, "snapshot copy queue family is invalid")
            })?
            .min_image_transfer_granularity;
        let queue = unsafe { device.get_device_queue(queue_family, queue_index) };
        let command_pool = unsafe {
            device.create_command_pool(
                &vk::CommandPoolCreateInfo::default()
                    .queue_family_index(queue_family)
                    .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER),
                None,
            )
        }
        .map_err(|error| failure("create snapshot command pool", error))?;
        let commands = unsafe {
            device.allocate_command_buffers(
                &vk::CommandBufferAllocateInfo::default()
                    .command_pool(command_pool)
                    .level(vk::CommandBufferLevel::PRIMARY)
                    .command_buffer_count(1),
            )
        };
        let command = match commands {
            Ok(commands) => commands[0],
            Err(error) => {
                unsafe { device.destroy_command_pool(command_pool, None) };
                return Err(failure("allocate snapshot command buffer", error));
            }
        };
        let fence = match unsafe { device.create_fence(&vk::FenceCreateInfo::default(), None) } {
            Ok(fence) => fence,
            Err(error) => {
                unsafe { device.destroy_command_pool(command_pool, None) };
                return Err(failure("create snapshot fence", error));
            }
        };
        let (retirement, retired) = sync_channel::<Retirement>(1);
        let retirement_device = device.clone();
        let retirement_owner = Arc::clone(&owner);
        let retirement_entry = Arc::clone(&entry);
        if let Err(error) = std::thread::Builder::new()
            .name("opennow-vulkan-snapshot-retire".to_owned())
            .spawn(move || {
                if let Ok(resources) = retired.recv() {
                    retire_resources(
                        resources,
                        |fence| unsafe {
                            retirement_device.wait_for_fences(&[fence], true, COPY_TIMEOUT_NS)
                        },
                        |fence, pool| unsafe {
                            retirement_device.destroy_fence(fence, None);
                            retirement_device.destroy_command_pool(pool, None);
                        },
                    );
                }
                drop(retirement_owner);
                drop(retirement_entry);
            })
        {
            unsafe {
                device.destroy_fence(fence, None);
                device.destroy_command_pool(command_pool, None);
            }
            return Err(Error::backend(
                Subsystem::Vulkan,
                format!("start snapshot retirement worker: {error}"),
            ));
        }
        Ok(Self {
            owner,
            entry,
            instance,
            device,
            queue,
            queue_family,
            queue_index,
            transfer_granularity,
            command_pool,
            command,
            fence,
            pending: false,
            failed: false,
            slots: Vec::new(),
            retirement,
            source: None,
        })
    }

    pub(super) fn copy(
        &mut self,
        decoded: &frame::Video,
        fallback_timestamp_us: u64,
    ) -> Result<DecodedVideoFrame> {
        let result = self.copy_frame(decoded, fallback_timestamp_us);
        if matches!(&result, Err(Error::DeviceLost { .. })) {
            self.failed = true;
            self.owner.invalidate();
        }
        result
    }

    fn copy_frame(
        &mut self,
        decoded: &frame::Video,
        fallback_timestamp_us: u64,
    ) -> Result<DecodedVideoFrame> {
        if self.failed {
            return Err(Error::backend(
                Subsystem::Vulkan,
                "Vulkan snapshot device is no longer usable",
            ));
        }
        if decoded.format() != ffmpeg_next::format::Pixel::VULKAN {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                "snapshot source is not a Vulkan frame",
            ));
        }
        let raw = unsafe { decoded.as_ptr().as_ref() }
            .ok_or_else(|| Error::backend(Subsystem::Vulkan, "missing decoded frame"))?;
        let frames = unsafe {
            raw.hw_frames_ctx
                .as_ref()
                .and_then(|buffer| buffer.data.cast::<ffi::AVHWFramesContext>().as_ref())
        }
        .ok_or_else(|| Error::backend(Subsystem::Vulkan, "missing Vulkan frame pool"))?;
        let pixel_format = match frames.sw_format {
            ffi::AVPixelFormat::AV_PIX_FMT_NV12 => PixelFormat::Nv12,
            ffi::AVPixelFormat::AV_PIX_FMT_P010LE => PixelFormat::P010,
            ffi::AVPixelFormat::AV_PIX_FMT_NV24 => PixelFormat::Nv24,
            ffi::AVPixelFormat::AV_PIX_FMT_P410LE => PixelFormat::P410,
            _ => {
                return Err(Error::unavailable(
                    Subsystem::Vulkan,
                    "embedded Vulkan supports only NV12, P010, NV24 and P410 images",
                ));
            }
        };
        let vkframes = unsafe { frames.hwctx.cast::<ffi::AVVulkanFramesContext>().as_ref() }
            .ok_or_else(|| {
                Error::backend(Subsystem::Vulkan, "missing Vulkan frame pool metadata")
            })?;
        let context = unsafe { frames.device_ctx.as_ref() }
            .ok_or_else(|| Error::backend(Subsystem::Vulkan, "missing Vulkan frame device"))?;
        let vkdevice = unsafe { context.hwctx.cast::<ffi::AVVulkanDeviceContext>().as_ref() }
            .ok_or_else(|| Error::backend(Subsystem::Vulkan, "missing Vulkan device metadata"))?;
        let info = self.owner.info();
        if (
            vkdevice.inst as usize,
            vkdevice.phys_dev as usize,
            vkdevice.act_dev as usize,
        ) != (info.instance, info.physical_device, info.device)
        {
            return Err(Error::backend(
                Subsystem::Vulkan,
                "Vulkan decoder replaced the adopted device",
            ));
        }
        let source = raw.data[0].cast::<ffi::AVVkFrame>();
        if source.is_null()
            || vkframes.usage as u32 & vk::ImageUsageFlags::TRANSFER_SRC.as_raw() == 0
        {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                "Vulkan decoder output cannot be copied on the GPU",
            ));
        }
        let (width, height) = (decoded.width(), decoded.height());
        let format = StreamFormat {
            width,
            height,
            pixel_format,
            ..StreamFormat::video_default(2, 2)?
        };
        format.validate()?;
        if frames.width < width as i32 || frames.height < height as i32 {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                "Vulkan output extent exceeds its decoder images",
            ));
        }
        validate_copy_granularity(
            [width, height],
            [frames.width as u32, frames.height as u32],
            self.transfer_granularity,
            pixel_format,
        )?;
        let formats = if pixel_format.is_ten_bit() {
            [vk::Format::R16_UNORM, vk::Format::R16G16_UNORM]
        } else {
            [vk::Format::R8_UNORM, vk::Format::R8G8_UNORM]
        };
        let index = self.slots.iter().position(|slot| {
            Arc::strong_count(slot) == 1
                && slot.width == width
                && slot.height == height
                && slot.formats == formats
                && slot.pixel_format == pixel_format
        });
        let index = match index {
            Some(index) => index,
            None => {
                self.slots.retain(|slot| {
                    Arc::strong_count(slot) > 1
                        || (slot.width == width
                            && slot.height == height
                            && slot.pixel_format == pixel_format)
                });
                if self.slots.len() >= MAX_SNAPSHOTS {
                    return Err(Error::backend(
                        Subsystem::Vulkan,
                        "bounded Vulkan presentation snapshot pool exhausted",
                    ));
                }
                self.slots.push(Arc::new(self.allocate(
                    width,
                    height,
                    formats,
                    pixel_format,
                )?));
                self.slots.len() - 1
            }
        };
        let lock = vkframes
            .lock_frame
            .ok_or_else(|| Error::backend(Subsystem::Vulkan, "FFmpeg frame lock is unavailable"))?;
        let unlock = vkframes.unlock_frame.ok_or_else(|| {
            Error::backend(Subsystem::Vulkan, "FFmpeg frame unlock is unavailable")
        })?;
        let lock_queue = vkdevice
            .lock_queue
            .ok_or_else(|| Error::backend(Subsystem::Vulkan, "FFmpeg queue lock is unavailable"))?;
        let unlock_queue = vkdevice.unlock_queue.ok_or_else(|| {
            Error::backend(Subsystem::Vulkan, "FFmpeg queue unlock is unavailable")
        })?;
        let mut retained = frame::Video::empty();
        if unsafe { ffi::av_frame_ref(retained.as_mut_ptr(), decoded.as_ptr()) } < 0 {
            return Err(Error::backend(
                Subsystem::Vulkan,
                "retain snapshot source failed",
            ));
        }
        unsafe { lock(frames as *const _ as *mut _, source) };
        let result = unsafe {
            self.submit_copy(
                &*source,
                vkframes,
                index,
                context as *const _ as *mut _,
                lock_queue,
                unlock_queue,
            )
        };
        if let Ok(signaled_values) = &result {
            unsafe { update_source_signals(&mut *source, signaled_values) };
        }
        unsafe { unlock(frames as *const _ as *mut _, source) };
        result?;
        self.pending = true;
        self.source = Some(retained);
        if let Err(error) = unsafe {
            self.device
                .wait_for_fences(&[self.fence], true, COPY_TIMEOUT_NS)
        } {
            self.failed = true;
            self.owner.invalidate();
            return Err(failure("wait for GPU presentation snapshot", error));
        }
        self.pending = false;
        self.source = None;
        let snapshot = Arc::get_mut(&mut self.slots[index]).expect("unpublished snapshot");
        snapshot.initialized = true;
        snapshot.semaphore_value += 1;
        let slot = Arc::clone(&self.slots[index]);
        let info = self.owner.info();
        let images = slot
            .images
            .iter()
            .enumerate()
            .map(|(index, image)| VulkanImage {
                image: image.as_raw(),
                format: formats[index].as_raw(),
                width: plane_extent(width, height, index, pixel_format).width,
                height: plane_extent(width, height, index, pixel_format).height,
                layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL.as_raw(),
                access: vk::AccessFlags::SHADER_READ.as_raw() as u64,
                semaphore: slot.semaphore.as_raw(),
                semaphore_value: slot.semaphore_value,
                queue_family: if self.queue_family == info.graphics_queue_family_index {
                    self.queue_family
                } else {
                    vk::QUEUE_FAMILY_IGNORED
                },
            })
            .collect();
        let vulkan = unsafe {
            VulkanVideoFrame::new(
                info.instance,
                info.physical_device,
                info.device,
                snapshot_families(self.queue_family, info.graphics_queue_family_index),
                (vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED).as_raw(),
                0,
                images,
                0,
                None,
                None,
                slot,
            )
            .with_completed_gpu_copy()
        };
        let output = DecodedVideoFrame {
            format,
            planes: Vec::new(),
            dmabuf: None,
            vulkan: Some(Arc::new(vulkan)),
            timestamp_us: decoded
                .pts()
                .and_then(|pts| u64::try_from(pts).ok())
                .unwrap_or(fallback_timestamp_us),
        };
        output.validate()?;
        Ok(output)
    }

    fn allocate(
        &self,
        width: u32,
        height: u32,
        formats: [vk::Format; 2],
        pixel_format: PixelFormat,
    ) -> Result<Snapshot> {
        let mut slot = Snapshot {
            _owner: Arc::clone(&self.owner),
            _entry: Arc::clone(&self.entry),
            device: self.device.clone(),
            images: [vk::Image::null(); 2],
            memory: [vk::DeviceMemory::null(); 2],
            formats,
            pixel_format,
            width,
            height,
            initialized: false,
            semaphore: vk::Semaphore::null(),
            semaphore_value: 0,
        };
        let mut timeline = vk::SemaphoreTypeCreateInfo::default()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(0);
        slot.semaphore = unsafe {
            self.device.create_semaphore(
                &vk::SemaphoreCreateInfo::default().push_next(&mut timeline),
                None,
            )
        }
        .map_err(|error| failure("create snapshot timeline", error))?;
        let memory = unsafe {
            self.instance
                .get_physical_device_memory_properties(vk::PhysicalDevice::from_raw(
                    self.owner.info().physical_device as u64,
                ))
        };
        for (index, format) in formats.into_iter().enumerate() {
            let families = snapshot_families(
                self.queue_family,
                self.owner.info().graphics_queue_family_index,
            );
            let info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(plane_extent(width, height, index, pixel_format))
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED)
                .sharing_mode(if families.len() == 1 {
                    vk::SharingMode::EXCLUSIVE
                } else {
                    vk::SharingMode::CONCURRENT
                })
                .queue_family_indices(&families);
            slot.images[index] = unsafe { self.device.create_image(&info, None) }
                .map_err(|error| failure("allocate GPU snapshot image", error))?;
            let requirements = unsafe {
                self.device
                    .get_image_memory_requirements(slot.images[index])
            };
            let memory_type_index = (0..memory.memory_type_count)
                .find(|index| {
                    requirements.memory_type_bits & (1 << index) != 0
                        && memory.memory_types[*index as usize]
                            .property_flags
                            .contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
                })
                .ok_or_else(|| {
                    Error::unavailable(
                        Subsystem::Vulkan,
                        "GPU snapshot has no device-local memory type",
                    )
                })?;
            slot.memory[index] = unsafe {
                self.device.allocate_memory(
                    &vk::MemoryAllocateInfo::default()
                        .allocation_size(requirements.size)
                        .memory_type_index(memory_type_index),
                    None,
                )
            }
            .map_err(|error| failure("allocate GPU snapshot memory", error))?;
            unsafe {
                self.device
                    .bind_image_memory(slot.images[index], slot.memory[index], 0)
            }
            .map_err(|error| failure("bind GPU snapshot image", error))?;
        }
        Ok(slot)
    }

    unsafe fn submit_copy(
        &self,
        source: &ffi::AVVkFrame,
        frames: &ffi::AVVulkanFramesContext,
        index: usize,
        context: *mut ffi::AVHWDeviceContext,
        lock: unsafe extern "C" fn(*mut ffi::AVHWDeviceContext, u32, u32),
        unlock: unsafe extern "C" fn(*mut ffi::AVHWDeviceContext, u32, u32),
    ) -> Result<Vec<(u64, u64)>> {
        let count = source
            .img
            .iter()
            .position(|image| *image == 0)
            .unwrap_or(source.img.len());
        if count == 0
            || count > 2
            || frames.nb_layers != 1
            || source.img[count..].iter().any(|image| *image != 0)
            || (count == 2 && source.img[0] == source.img[1])
        {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                "unsupported Vulkan decoder image topology",
            ));
        }
        for i in 0..count {
            if source.sem[i] == 0
                || source.sem_value[i] == u64::MAX
                || matches!(
                    vk::ImageLayout::from_raw(source.layout[i]),
                    vk::ImageLayout::UNDEFINED | vk::ImageLayout::PREINITIALIZED
                )
                || (source.queue_family[i] != vk::QUEUE_FAMILY_IGNORED
                    && source.queue_family[i] != self.queue_family)
            {
                return Err(Error::unavailable(
                    Subsystem::Vulkan,
                    "Vulkan decoder image lacks compatible queue sharing or timeline synchronization",
                ));
            }
        }
        let slot = &self.slots[index];
        validate_source_formats(&frames.format[..count], slot.pixel_format)?;
        unsafe {
            self.device
                .reset_fences(&[self.fence])
                .map_err(|error| failure("reset snapshot fence", error))?;
            self.device
                .reset_command_buffer(self.command, vk::CommandBufferResetFlags::empty())
                .map_err(|error| failure("reset snapshot commands", error))?;
            self.device
                .begin_command_buffer(
                    self.command,
                    &vk::CommandBufferBeginInfo::default()
                        .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT),
                )
                .map_err(|error| failure("begin snapshot commands", error))?;
        }
        let range = vk::ImageSubresourceRange::default()
            .aspect_mask(vk::ImageAspectFlags::COLOR)
            .level_count(1)
            .layer_count(1);
        let mut before = Vec::new();
        let mut after = Vec::new();
        for i in 0..count {
            let barrier = vk::ImageMemoryBarrier::default()
                .image(vk::Image::from_raw(source.img[i]))
                .subresource_range(range)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
            before.push(
                barrier
                    .old_layout(vk::ImageLayout::from_raw(source.layout[i]))
                    .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ),
            );
            after.push(
                barrier
                    .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                    .new_layout(vk::ImageLayout::from_raw(source.layout[i]))
                    .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .dst_access_mask(vk::AccessFlags::empty()),
            );
        }
        for i in 0..2 {
            let barrier = vk::ImageMemoryBarrier::default()
                .image(slot.images[i])
                .subresource_range(range)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED);
            before.push(
                barrier
                    .old_layout(if slot.initialized {
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                    } else {
                        vk::ImageLayout::UNDEFINED
                    })
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE),
            );
            after.push(
                barrier
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::empty()),
            );
        }
        unsafe {
            self.device.cmd_pipeline_barrier(
                self.command,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &before,
            )
        };
        for i in 0..2 {
            let aspect = if count == 2 {
                vk::ImageAspectFlags::COLOR
            } else if i == 0 {
                vk::ImageAspectFlags::PLANE_0
            } else {
                vk::ImageAspectFlags::PLANE_1
            };
            let region = vk::ImageCopy::default()
                .src_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(aspect)
                        .layer_count(1),
                )
                .dst_subresource(
                    vk::ImageSubresourceLayers::default()
                        .aspect_mask(vk::ImageAspectFlags::COLOR)
                        .layer_count(1),
                )
                .extent(plane_extent(slot.width, slot.height, i, slot.pixel_format));
            unsafe {
                self.device.cmd_copy_image(
                    self.command,
                    vk::Image::from_raw(source.img[if count == 1 { 0 } else { i }]),
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    slot.images[i],
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[region],
                )
            };
        }
        unsafe {
            self.device.cmd_pipeline_barrier(
                self.command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &after,
            );
            self.device
                .end_command_buffer(self.command)
                .map_err(|error| failure("finish snapshot commands", error))?;
        }
        let wait_values = source_waits(&source.sem[..count], &source.sem_value[..count]);
        let semaphores = wait_values
            .iter()
            .map(|(semaphore, _)| vk::Semaphore::from_raw(*semaphore))
            .collect::<Vec<_>>();
        let waits = wait_values
            .iter()
            .map(|(_, value)| *value)
            .collect::<Vec<_>>();
        let mut signals = waits.iter().map(|value| value + 1).collect::<Vec<_>>();
        let mut signal_semaphores = semaphores.clone();
        signals.push(
            slot.semaphore_value
                .checked_add(1)
                .ok_or_else(|| Error::backend(Subsystem::Vulkan, "snapshot timeline exhausted"))?,
        );
        signal_semaphores.push(slot.semaphore);
        let stages = vec![vk::PipelineStageFlags::ALL_COMMANDS; semaphores.len()];
        let commands = [self.command];
        let mut timeline = vk::TimelineSemaphoreSubmitInfo::default()
            .wait_semaphore_values(&waits)
            .signal_semaphore_values(&signals);
        let submission = vk::SubmitInfo::default()
            .push_next(&mut timeline)
            .wait_semaphores(&semaphores)
            .wait_dst_stage_mask(&stages)
            .signal_semaphores(&signal_semaphores)
            .command_buffers(&commands);
        unsafe { lock(context, self.queue_family, self.queue_index) };
        let result = unsafe {
            self.device
                .queue_submit(self.queue, &[submission], self.fence)
        };
        unsafe { unlock(context, self.queue_family, self.queue_index) };
        result.map_err(|error| failure("submit GPU presentation snapshot", error))?;
        Ok(wait_values
            .into_iter()
            .map(|(semaphore, value)| (semaphore, value + 1))
            .collect())
    }
}

impl Drop for VulkanCopyPool {
    fn drop(&mut self) {
        if let Err(TrySendError::Full(resources) | TrySendError::Disconnected(resources)) =
            self.retirement.try_send(Retirement {
                fence: self.fence,
                command_pool: self.command_pool,
                pending: self.pending,
                _slots: std::mem::take(&mut self.slots),
                _source: self.source.take(),
            })
        {
            self.owner.invalidate();
            eprintln!(
                "OpenNOW: Vulkan snapshot retirement unavailable; device disabled and pending resources quarantined"
            );
            std::mem::forget(resources);
        }
    }
}

fn failure(operation: &str, error: vk::Result) -> Error {
    if error == vk::Result::ERROR_DEVICE_LOST {
        Error::DeviceLost {
            subsystem: Subsystem::Vulkan,
            reason: format!("{operation}: {error:?}"),
        }
    } else {
        Error::backend(Subsystem::Vulkan, format!("{operation}: {error:?}"))
    }
}

fn source_waits(semaphores: &[u64], values: &[u64]) -> Vec<(u64, u64)> {
    let mut waits = std::collections::BTreeMap::<u64, u64>::new();
    for (&semaphore, &value) in semaphores.iter().zip(values) {
        waits
            .entry(semaphore)
            .and_modify(|current| *current = (*current).max(value))
            .or_insert(value);
    }
    waits.into_iter().collect()
}

fn update_source_signals(source: &mut ffi::AVVkFrame, signals: &[(u64, u64)]) {
    for (index, semaphore) in source.sem.iter().enumerate() {
        if let Some((_, value)) = signals.iter().find(|(signal, _)| signal == semaphore) {
            source.sem_value[index] = *value;
        }
    }
}

fn snapshot_families(copy: u32, graphics: u32) -> Vec<u32> {
    if copy == graphics {
        vec![copy]
    } else {
        vec![copy, graphics]
    }
}

fn validate_copy_granularity(
    visible: [u32; 2],
    coded: [u32; 2],
    granularity: vk::Extent3D,
    pixel_format: PixelFormat,
) -> Result<()> {
    for divisor in [1, if pixel_format.is_444() { 1 } else { 2 }] {
        for (axis, granularity) in [granularity.width, granularity.height]
            .into_iter()
            .enumerate()
        {
            let extent = visible[axis] / divisor;
            if extent != coded[axis] / divisor && (granularity == 0 || extent % granularity != 0) {
                return Err(Error::unavailable(
                    Subsystem::Vulkan,
                    "visible Vulkan planes violate copy queue transfer granularity",
                ));
            }
        }
    }
    Ok(())
}

fn plane_extent(width: u32, height: u32, index: usize, pixel_format: PixelFormat) -> vk::Extent3D {
    let divisor = if index == 0 || pixel_format.is_444() {
        1
    } else {
        2
    };
    vk::Extent3D {
        width: width / divisor,
        height: height / divisor,
        depth: 1,
    }
}

fn validate_source_formats(source: &[i32], pixel_format: PixelFormat) -> Result<()> {
    let multiplane = match pixel_format {
        PixelFormat::Nv12 => vk::Format::G8_B8R8_2PLANE_420_UNORM,
        PixelFormat::P010 => vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
        PixelFormat::Nv24 => vk::Format::G8_B8R8_2PLANE_444_UNORM,
        PixelFormat::P410 => vk::Format::G10X6_B10X6R10X6_2PLANE_444_UNORM_3PACK16,
        _ => {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                "unsupported Vulkan snapshot pixel format",
            ));
        }
    };
    let target = if pixel_format.is_ten_bit() {
        [vk::Format::R16_UNORM, vk::Format::R16G16_UNORM]
    } else {
        [vk::Format::R8_UNORM, vk::Format::R8G8_UNORM]
    };
    let packed_ten_bit = pixel_format.is_ten_bit()
        && source
            == [
                vk::Format::R10X6_UNORM_PACK16.as_raw(),
                vk::Format::R10X6G10X6_UNORM_2PACK16.as_raw(),
            ];
    if source == [multiplane.as_raw()]
        || source == target.map(|format| format.as_raw())
        || packed_ten_bit
    {
        Ok(())
    } else {
        Err(Error::unavailable(
            Subsystem::Vulkan,
            "Vulkan source image formats do not match the negotiated depth and chroma planes",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_four_four_snapshots_keep_full_resolution_chroma() {
        for format in [PixelFormat::Nv24, PixelFormat::P410] {
            assert_eq!(
                plane_extent(1919, 1079, 1, format),
                vk::Extent3D {
                    width: 1919,
                    height: 1079,
                    depth: 1
                }
            );
            assert_eq!(
                plane_extent(1920, 1080, 0, format),
                plane_extent(1920, 1080, 1, format)
            );
            assert_eq!(plane_extent(1920, 1080, 1, format).width, 1920);
            assert_eq!(plane_extent(1920, 1080, 1, format).height, 1080);
            let granularity = vk::Extent3D {
                width: 4,
                height: 4,
                depth: 1,
            };
            assert!(
                validate_copy_granularity([1920, 1084], [1920, 1088], granularity, format).is_ok()
            );
            assert!(
                validate_copy_granularity([1920, 1082], [1920, 1088], granularity, format).is_err()
            );
        }
        assert_eq!(plane_extent(1920, 1080, 1, PixelFormat::Nv12).width, 960);
        assert_eq!(plane_extent(1920, 1080, 1, PixelFormat::P010).height, 540);
    }

    #[test]
    fn four_four_four_source_layouts_reject_subsampled_and_wrong_depth_images() {
        for (format, valid, invalid, planes) in [
            (
                PixelFormat::Nv24,
                vk::Format::G8_B8R8_2PLANE_444_UNORM,
                vk::Format::G8_B8R8_2PLANE_420_UNORM,
                [vk::Format::R8_UNORM, vk::Format::R8G8_UNORM],
            ),
            (
                PixelFormat::P410,
                vk::Format::G10X6_B10X6R10X6_2PLANE_444_UNORM_3PACK16,
                vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
                [vk::Format::R16_UNORM, vk::Format::R16G16_UNORM],
            ),
        ] {
            assert!(validate_source_formats(&[valid.as_raw()], format).is_ok());
            assert!(validate_source_formats(&[invalid.as_raw()], format).is_err());
            assert!(validate_source_formats(&planes.map(|plane| plane.as_raw()), format).is_ok());
            let other = if format.is_ten_bit() {
                PixelFormat::Nv24
            } else {
                PixelFormat::P410
            };
            assert!(validate_source_formats(&[valid.as_raw()], other).is_err());
            assert!(validate_source_formats(&planes.map(|plane| plane.as_raw()), other).is_err());
        }
        assert!(
            validate_source_formats(
                &[
                    vk::Format::R10X6_UNORM_PACK16.as_raw(),
                    vk::Format::R10X6G10X6_UNORM_2PACK16.as_raw()
                ],
                PixelFormat::P410
            )
            .is_ok()
        );
    }

    #[test]
    fn snapshot_waits_deduplicate_shared_semaphores() {
        assert_eq!(source_waits(&[9, 9], &[3, 5]), [(9, 5)]);
        assert_eq!(source_waits(&[9, 10], &[3, 5]), [(9, 3), (10, 5)]);
    }

    #[test]
    fn aliased_source_signals_use_one_original_maximum() {
        let mut source: ffi::AVVkFrame = unsafe { std::mem::zeroed() };
        source.img[..2].copy_from_slice(&[1, 2]);
        source.sem[..2].copy_from_slice(&[9, 9]);
        source.sem_value[..2].copy_from_slice(&[3, 5]);
        let signals = source_waits(&source.sem[..2], &source.sem_value[..2])
            .into_iter()
            .map(|(semaphore, value)| (semaphore, value + 1))
            .collect::<Vec<_>>();
        update_source_signals(&mut source, &signals);
        assert_eq!(&source.sem_value[..2], &[6, 6]);
        source.sem[..2].copy_from_slice(&[9, 10]);
        source.sem_value[..2].copy_from_slice(&[3, 5]);
        update_source_signals(&mut source, &[(9, 4), (10, 6)]);
        assert_eq!(&source.sem_value[..2], &[4, 6]);
    }

    #[test]
    fn snapshots_share_both_families_only_when_distinct() {
        assert_eq!(snapshot_families(2, 2), vec![2]);
        assert_eq!(snapshot_families(2, 3), vec![2, 3]);
    }

    #[test]
    fn transfer_queue_granularity_checks_both_visible_planes() {
        let granularity = vk::Extent3D {
            width: 4,
            height: 4,
            depth: 1,
        };
        assert!(
            validate_copy_granularity([1920, 1080], [1920, 1088], granularity, PixelFormat::Nv12)
                .is_ok()
        );
        assert!(
            validate_copy_granularity([1920, 1082], [1920, 1088], granularity, PixelFormat::Nv12)
                .is_err()
        );
        let whole_image = vk::Extent3D::default();
        assert!(
            validate_copy_granularity([1920, 1080], [1920, 1080], whole_image, PixelFormat::Nv12)
                .is_ok()
        );
        assert!(
            validate_copy_granularity([1920, 1080], [1920, 1088], whole_image, PixelFormat::Nv12)
                .is_err()
        );
    }

    #[test]
    fn retirement_retains_source_after_timeouts_until_completion_or_device_loss() {
        for terminal in [Ok(()), Err(vk::Result::ERROR_DEVICE_LOST)] {
            let original = frame::Video::new(ffmpeg_next::format::Pixel::NV12, 2, 2);
            let buffer = unsafe { (*original.as_ptr()).buf[0] };
            assert!(!buffer.is_null());
            let mut retained = frame::Video::empty();
            assert_eq!(
                unsafe { ffi::av_frame_ref(retained.as_mut_ptr(), original.as_ptr()) },
                0
            );
            let resources = Retirement {
                fence: vk::Fence::null(),
                command_pool: vk::CommandPool::null(),
                pending: true,
                _slots: Vec::new(),
                _source: Some(retained),
            };
            let mut waits = 0;
            let destroyed = std::cell::Cell::new(false);
            retire_resources(
                resources,
                |_| {
                    assert_eq!(unsafe { ffi::av_buffer_get_ref_count(buffer) }, 2);
                    assert!(!destroyed.get());
                    waits += 1;
                    if waits <= 2 {
                        Err(vk::Result::TIMEOUT)
                    } else {
                        terminal
                    }
                },
                |_, _| {
                    assert_eq!(unsafe { ffi::av_buffer_get_ref_count(buffer) }, 2);
                    destroyed.set(true);
                },
            );
            assert_eq!(waits, 3);
            assert!(destroyed.get());
            assert_eq!(unsafe { ffi::av_buffer_get_ref_count(buffer) }, 1);
        }
    }

    #[test]
    fn retirement_handoff_does_not_wait_for_the_gpu() {
        let original = frame::Video::new(ffmpeg_next::format::Pixel::NV12, 2, 2);
        let buffer = unsafe { (*original.as_ptr()).buf[0] };
        let mut retained = frame::Video::empty();
        assert_eq!(
            unsafe { ffi::av_frame_ref(retained.as_mut_ptr(), original.as_ptr()) },
            0
        );
        let (sender, receiver) = sync_channel(1);
        let (complete, completion) = sync_channel(1);
        let worker = std::thread::spawn(move || {
            let resources = receiver.recv().unwrap();
            retire_resources(
                resources,
                |_| {
                    completion.recv().unwrap();
                    Ok(())
                },
                |_, _| {},
            );
        });
        assert!(
            sender
                .try_send(Retirement {
                    fence: vk::Fence::null(),
                    command_pool: vk::CommandPool::null(),
                    pending: true,
                    _slots: Vec::new(),
                    _source: Some(retained)
                })
                .is_ok()
        );
        assert_eq!(unsafe { ffi::av_buffer_get_ref_count(buffer) }, 2);
        complete.send(()).unwrap();
        worker.join().unwrap();
        assert_eq!(unsafe { ffi::av_buffer_get_ref_count(buffer) }, 1);
    }

    #[test]
    fn snapshot_rejects_mismatched_plane_depth_or_chroma() {
        let nv12 = PixelFormat::Nv12;
        assert!(
            validate_source_formats(&[vk::Format::G8_B8R8_2PLANE_420_UNORM.as_raw()], nv12).is_ok()
        );
        assert!(
            validate_source_formats(
                &[
                    vk::Format::R16_UNORM.as_raw(),
                    vk::Format::R16G16_UNORM.as_raw()
                ],
                nv12
            )
            .is_err()
        );
        assert!(
            validate_source_formats(&[vk::Format::G8_B8R8_2PLANE_444_UNORM.as_raw()], nv12)
                .is_err()
        );
        let p010 = PixelFormat::P010;
        for source in [
            vec![vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16.as_raw()],
            vec![
                vk::Format::R16_UNORM.as_raw(),
                vk::Format::R16G16_UNORM.as_raw(),
            ],
            vec![
                vk::Format::R10X6_UNORM_PACK16.as_raw(),
                vk::Format::R10X6G10X6_UNORM_2PACK16.as_raw(),
            ],
        ] {
            assert!(validate_source_formats(&source, p010).is_ok());
            assert!(validate_source_formats(&source, nv12).is_err());
        }
        assert!(
            validate_source_formats(
                &[vk::Format::G10X6_B10X6R10X6_2PLANE_444_UNORM_3PACK16.as_raw()],
                p010
            )
            .is_err()
        );
        assert!(
            validate_source_formats(
                &[
                    vk::Format::R16_UNORM.as_raw(),
                    vk::Format::R8G8_UNORM.as_raw()
                ],
                p010
            )
            .is_err()
        );
    }
}
