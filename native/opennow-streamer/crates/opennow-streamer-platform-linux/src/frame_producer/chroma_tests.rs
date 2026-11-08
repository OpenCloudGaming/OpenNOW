use super::*;
use crate::{ChromaLocation, ColorRange, StreamFormat, VulkanImage};

#[test]
fn conversion_constants_match_shader_layout() {
    assert_eq!(std::mem::size_of::<ConversionConstants>(), 28);
    assert_eq!(std::mem::offset_of!(ConversionConstants, texture_scale), 0);
    assert_eq!(std::mem::offset_of!(ConversionConstants, chroma_offset), 8);
    assert_eq!(std::mem::offset_of!(ConversionConstants, color_matrix), 16);
    assert_eq!(std::mem::offset_of!(ConversionConstants, full_range), 20);
    assert_eq!(std::mem::offset_of!(ConversionConstants, sample_bits), 24);
}

#[test]
#[ignore = "requires Vulkan; synthetic chroma impulses run with Mesa lavapipe"]
fn chroma_impulse_conversion_distinguishes_left_center_and_top_left() {
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
        let mut semaphore_type = vk::SemaphoreTypeCreateInfo::default()
            .semaphore_type(vk::SemaphoreType::TIMELINE)
            .initial_value(1);
        let semaphore = device
            .create_semaphore(
                &vk::SemaphoreCreateInfo::default().push_next(&mut semaphore_type),
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
        let (upload, upload_memory) = host_buffer(
            &instance,
            physical,
            &device,
            192,
            vk::BufferUsageFlags::TRANSFER_SRC,
        );
        let (readback, readback_memory) = host_buffer(
            &instance,
            physical,
            &device,
            256,
            vk::BufferUsageFlags::TRANSFER_DST,
        );
        for pixel_format in [PixelFormat::Nv12, PixelFormat::Nv24] {
            let chroma_size = if pixel_format.is_444() { 8 } else { 4 };
            let mut pixels = vec![128u8; 64 + chroma_size * chroma_size * 2];
            let impulse = if pixel_format.is_444() { 2 } else { 1 };
            pixels[64 + (impulse * chroma_size + impulse) * 2] = 192;
            let mapped = device
                .map_memory(upload_memory, 0, 192, vk::MemoryMapFlags::empty())
                .unwrap();
            std::ptr::copy_nonoverlapping(pixels.as_ptr(), mapped.cast(), pixels.len());
            device.unmap_memory(upload_memory);
            let images = [
                create_gpu_image(
                    &instance,
                    physical,
                    &device,
                    8,
                    8,
                    vk::Format::R8_UNORM,
                    vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_DST,
                )
                .unwrap(),
                create_gpu_image(
                    &instance,
                    physical,
                    &device,
                    chroma_size as u32,
                    chroma_size as u32,
                    vk::Format::R8G8_UNORM,
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
                image_barrier(
                    &device,
                    command,
                    image.image,
                    vk::ImageLayout::UNDEFINED,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::AccessFlags::empty(),
                    vk::AccessFlags::TRANSFER_WRITE,
                );
                let size = if index == 0 { 8 } else { chroma_size as u32 };
                device.cmd_copy_buffer_to_image(
                    command,
                    upload,
                    image.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[vk::BufferImageCopy::default()
                        .buffer_offset(if index == 0 { 0 } else { 64 })
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .layer_count(1),
                        )
                        .image_extent(vk::Extent3D {
                            width: size,
                            height: size,
                            depth: 1,
                        })],
                );
                image_barrier(
                    &device,
                    command,
                    image.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::AccessFlags::TRANSFER_WRITE,
                    vk::AccessFlags::SHADER_READ,
                );
            }
            device.end_command_buffer(command).unwrap();
            device
                .queue_submit(
                    queue,
                    &[vk::SubmitInfo::default().command_buffers(&commands)],
                    vk::Fence::null(),
                )
                .unwrap();
            device.queue_wait_idle(queue).unwrap();
            let mut producer = LinuxFrameProducer::new_with_slots(render, 1).unwrap();
            producer.ensure_renderer(GpuTextureFormat::Rgba8).unwrap();
            let output = create_gpu_image(
                &instance,
                physical,
                &device,
                8,
                8,
                vk::Format::R8G8B8A8_UNORM,
                vk::ImageUsageFlags::COLOR_ATTACHMENT
                    | vk::ImageUsageFlags::SAMPLED
                    | vk::ImageUsageFlags::TRANSFER_SRC,
            )
            .unwrap();
            let framebuffer = device
                .create_framebuffer(
                    &vk::FramebufferCreateInfo::default()
                        .render_pass(producer.renderers[&GpuTextureFormat::Rgba8].render_pass)
                        .attachments(&[output.view])
                        .width(8)
                        .height(8)
                        .layers(1),
                    None,
                )
                .unwrap();
            producer.slots[0] = Some(FrameSlotResources {
                width: 8,
                height: 8,
                texture_format: GpuTextureFormat::Rgba8,
                output,
                output_initialized: false,
                framebuffer,
                cpu: None,
                sand: None,
                owned_input_views: Vec::new(),
                frame: None,
            });
            let mut converted = Vec::new();
            for (chroma_location, weight) in [
                (ChromaLocation::Center, 0.5625),
                (ChromaLocation::Left, 0.75),
                (ChromaLocation::TopLeft, 1.0),
            ] {
                let frame = DecodedVideoFrame {
                    format: StreamFormat {
                        pixel_format,
                        chroma_location,
                        color_range: ColorRange::Full,
                        ..StreamFormat::video_default(8, 8).unwrap()
                    },
                    planes: Vec::new(),
                    dmabuf: None,
                    vulkan: Some(Arc::new(
                        VulkanVideoFrame::new(
                            render.instance,
                            render.physical_device,
                            render.device,
                            vec![family],
                            vk::ImageUsageFlags::SAMPLED.as_raw(),
                            0,
                            images
                                .iter()
                                .enumerate()
                                .map(|(index, image)| VulkanImage {
                                    image: image.image.as_raw(),
                                    width: if index == 0 { 8 } else { chroma_size as u32 },
                                    height: if index == 0 { 8 } else { chroma_size as u32 },
                                    format: if index == 0 {
                                        vk::Format::R8_UNORM
                                    } else {
                                        vk::Format::R8G8_UNORM
                                    }
                                    .as_raw(),
                                    layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL.as_raw(),
                                    access: vk::AccessFlags2::SHADER_SAMPLED_READ.as_raw(),
                                    semaphore: semaphore.as_raw(),
                                    semaphore_value: 1,
                                    queue_family: vk::QUEUE_FAMILY_IGNORED,
                                })
                                .collect(),
                            0,
                            None,
                            None,
                            Arc::new(()),
                        )
                        .with_completed_gpu_copy(),
                    )),
                    timestamp_us: 0,
                };
                device
                    .reset_command_buffer(command, vk::CommandBufferResetFlags::empty())
                    .unwrap();
                device
                    .begin_command_buffer(command, &vk::CommandBufferBeginInfo::default())
                    .unwrap();
                let output = producer
                    .record_frame(frame, command.as_raw() as usize, 0)
                    .unwrap();
                let output_image = vk::Image::from_raw(output.image);
                image_barrier(
                    &device,
                    command,
                    output_image,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::AccessFlags::MEMORY_WRITE | vk::AccessFlags::SHADER_READ,
                    vk::AccessFlags::TRANSFER_READ,
                );
                device.cmd_copy_image_to_buffer(
                    command,
                    output_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    readback,
                    &[vk::BufferImageCopy::default()
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .layer_count(1),
                        )
                        .image_extent(vk::Extent3D {
                            width: 8,
                            height: 8,
                            depth: 1,
                        })],
                );
                image_barrier(
                    &device,
                    command,
                    output_image,
                    vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                    vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    vk::AccessFlags::TRANSFER_READ,
                    vk::AccessFlags::SHADER_READ,
                );
                device.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::HOST,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[vk::BufferMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .dst_access_mask(vk::AccessFlags::HOST_READ)
                        .buffer(readback)
                        .size(vk::WHOLE_SIZE)],
                    &[],
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
                let mapped = device
                    .map_memory(readback_memory, 0, 256, vk::MemoryMapFlags::empty())
                    .unwrap();
                let rgba = std::slice::from_raw_parts(mapped.cast::<u8>(), 256).to_vec();
                device.unmap_memory(readback_memory);
                let weight = if pixel_format.is_444() { 1.0 } else { weight };
                let expected_blue = (128.0 + 1.8556_f32 * 64.0 * weight).round() as u8;
                assert!(
                    rgba[(2 * 8 + 2) * 4 + 2].abs_diff(expected_blue) <= 1,
                    "{pixel_format:?} {chroma_location:?}: blue={}, expected={expected_blue}",
                    rgba[(2 * 8 + 2) * 4 + 2]
                );
                converted.push(rgba);
            }
            if pixel_format.is_444() {
                assert_eq!(converted[0], converted[1]);
                assert_eq!(converted[1], converted[2]);
            } else {
                assert_ne!(converted[0], converted[1]);
                assert_ne!(converted[1], converted[2]);
            }
            drop(producer);
            for image in images {
                destroy_gpu_image(&device, image);
            }
        }
        device.destroy_buffer(upload, None);
        device.free_memory(upload_memory, None);
        device.destroy_buffer(readback, None);
        device.free_memory(readback_memory, None);
        device.destroy_command_pool(pool, None);
        device.destroy_semaphore(semaphore, None);
        device.destroy_device(None);
        instance.destroy_instance(None);
    }
}

unsafe fn host_buffer(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    size: u64,
    usage: vk::BufferUsageFlags,
) -> (vk::Buffer, vk::DeviceMemory) {
    unsafe {
        let buffer = device
            .create_buffer(
                &vk::BufferCreateInfo::default().size(size).usage(usage),
                None,
            )
            .unwrap();
        let requirements = device.get_buffer_memory_requirements(buffer);
        let memory_type = find_memory_type(
            instance,
            physical,
            requirements.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .unwrap();
        let memory = device
            .allocate_memory(
                &vk::MemoryAllocateInfo::default()
                    .allocation_size(requirements.size)
                    .memory_type_index(memory_type),
                None,
            )
            .unwrap();
        device.bind_buffer_memory(buffer, memory, 0).unwrap();
        (buffer, memory)
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn image_barrier(
    device: &ash::Device,
    command: vk::CommandBuffer,
    image: vk::Image,
    old_layout: vk::ImageLayout,
    new_layout: vk::ImageLayout,
    source: vk::AccessFlags,
    destination: vk::AccessFlags,
) {
    unsafe {
        device.cmd_pipeline_barrier(
            command,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::PipelineStageFlags::ALL_COMMANDS,
            vk::DependencyFlags::empty(),
            &[],
            &[],
            &[vk::ImageMemoryBarrier::default()
                .old_layout(old_layout)
                .new_layout(new_layout)
                .src_access_mask(source)
                .dst_access_mask(destination)
                .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                .image(image)
                .subresource_range(image_color_range())],
        );
    }
}
