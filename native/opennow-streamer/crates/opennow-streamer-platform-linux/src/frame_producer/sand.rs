use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::time::Instant;

use super::*;

const SAND128: u64 = (0x07 << 56) | 4;
const PARAM_MASK: u64 = (1 << 48) - 1;
const P030: u32 = u32::from_le_bytes(*b"P030");

pub(super) fn is_sand(frame: &DmaBufFrame) -> bool {
    frame
        .objects
        .iter()
        .any(|object| object.format_modifier & !(PARAM_MASK << 8) == SAND128)
        || frame.layers.iter().any(|layer| layer.format == P030)
}

fn invalid(message: &str) -> Error {
    Error::InvalidFormat(format!("SAND128 DMA-BUF: {message}"))
}

#[derive(Debug)]
struct PlaneCopy {
    object: usize,
    regions: Vec<vk::BufferImageCopy>,
}

#[derive(Debug)]
struct SandLayout {
    planes: [PlaneCopy; 2],
}

impl SandLayout {
    fn new(frame: &DmaBufFrame, format: crate::StreamFormat) -> Result<Self> {
        frame.validate()?;
        format.validate()?;
        if frame.layers.iter().any(|layer| layer.format == P030) {
            return Err(invalid(
                "10-bit P030 is unsupported; only 8-bit NV12 is supported",
            ));
        }
        if format.pixel_format != PixelFormat::Nv12
            || format.color_transfer != crate::ColorTransfer::Sdr
            || format.color_primaries != crate::ColorPrimaries::Bt709
        {
            return Err(invalid("only 8-bit SDR NV12 is supported"));
        }
        let [layer] = frame.layers.as_slice() else {
            return Err(invalid("expected one NV12 layer"));
        };
        let [luma, chroma] = layer.planes.as_slice() else {
            return Err(invalid("expected exactly two planes"));
        };
        if layer.format != DRM_FORMAT_NV12 {
            return Err(invalid("expected NV12 fourcc"));
        }
        let heights = [luma, chroma].map(|plane| {
            let modifier = frame.objects[plane.object_index].format_modifier;
            if modifier & !(PARAM_MASK << 8) != SAND128 {
                return Err(invalid("unsupported Broadcom modifier; expected SAND128"));
            }
            Ok((modifier >> 8) & PARAM_MASK)
        });
        let [y_height, uv_height] = heights;
        let (y_height, uv_height) = (y_height?, uv_height?);
        if y_height == 0 && uv_height == 0 {
            if frame.objects.len() != 2 || luma.object_index == chroma.object_index {
                return Err(invalid("zero-height modifier requires split Y/UV objects"));
            }
        } else if y_height == 0
            || y_height != uv_height
            || frame.objects.len() != 1
            || luma.object_index != chroma.object_index
        {
            return Err(invalid(
                "height-bearing modifier requires one shared object",
            ));
        }
        let mut copies = Vec::with_capacity(2);
        let mut plane_ranges = Vec::with_capacity(2);
        for (index, plane) in [luma, chroma].into_iter().enumerate() {
            let object = &frame.objects[plane.object_index];
            let column_height = if y_height == 0 {
                plane.pitch as u64
            } else {
                y_height
            };
            let rows = u64::from(format.height >> index);
            let stride = column_height
                .checked_mul(128)
                .ok_or_else(|| invalid("column stride overflow"))?;
            let plane_start = plane.offset as u64;
            let plane_end = rows
                .checked_mul(128)
                .and_then(|size| plane_start.checked_add(size))
                .ok_or_else(|| invalid("plane size overflow"))?;
            if column_height < rows || plane_start % 128 != 0 || plane_end > stride {
                return Err(invalid(
                    "plane offset or rows exceed column height or alignment",
                ));
            }
            plane_ranges.push((plane_start, plane_end));
            let mut regions = Vec::with_capacity(format.width.div_ceil(128) as usize);
            for column in 0..format.width.div_ceil(128) {
                let offset = u64::from(column)
                    .checked_mul(stride)
                    .and_then(|start| start.checked_add(plane_start))
                    .ok_or_else(|| invalid("column offset overflow"))?;
                let bytes = (format.width - column * 128).min(128);
                let end = (rows - 1)
                    .checked_mul(128)
                    .and_then(|size| size.checked_add(offset))
                    .and_then(|size| size.checked_add(u64::from(bytes)))
                    .ok_or_else(|| invalid("copy range overflow"))?;
                if end > object.size as u64 {
                    return Err(invalid("column copy exceeds DMA-BUF object size"));
                }
                regions.push(
                    vk::BufferImageCopy::default()
                        .buffer_offset(offset)
                        .buffer_row_length(128 >> index)
                        .image_subresource(
                            vk::ImageSubresourceLayers::default()
                                .aspect_mask(vk::ImageAspectFlags::COLOR)
                                .layer_count(1),
                        )
                        .image_offset(vk::Offset3D {
                            x: ((column * 128) >> index) as i32,
                            y: 0,
                            z: 0,
                        })
                        .image_extent(vk::Extent3D {
                            width: bytes >> index,
                            height: rows as u32,
                            depth: 1,
                        }),
                );
            }
            copies.push(PlaneCopy {
                object: plane.object_index,
                regions,
            });
        }
        if luma.object_index == chroma.object_index
            && plane_ranges[0].0 < plane_ranges[1].1
            && plane_ranges[1].0 < plane_ranges[0].1
        {
            return Err(invalid("Y and UV overlap within a column"));
        }
        Ok(Self {
            planes: copies.try_into().expect("two planes"),
        })
    }
}

pub(super) fn wait_ready(frame: &DmaBufFrame, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    for object in &frame.objects {
        loop {
            let mut descriptor = libc::pollfd {
                fd: object.fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let remaining = deadline.saturating_duration_since(Instant::now());
            let millis = remaining.as_millis().min(i32::MAX as u128) as i32;
            let result = unsafe { libc::poll(&mut descriptor, 1, millis) };
            if result < 0 {
                let error = std::io::Error::last_os_error();
                if error.kind() == std::io::ErrorKind::Interrupted && Instant::now() < deadline {
                    continue;
                }
                return Err(Error::backend(
                    Subsystem::Vulkan,
                    format!("SAND DMA-BUF fence poll failed: {error}"),
                ));
            }
            if result == 0 {
                return Err(Error::FrameNotReady);
            }
            if descriptor.revents & libc::POLLIN == 0
                || descriptor.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0
            {
                return Err(Error::unavailable(
                    Subsystem::Vulkan,
                    "SAND DMA-BUF producer write fence poll returned an invalid descriptor state",
                ));
            }
            break;
        }
    }
    Ok(())
}

struct ImportedBuffer {
    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    device: ash::Device,
}

impl Drop for ImportedBuffer {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_buffer(self.buffer, None);
            self.device.free_memory(self.memory, None);
        }
    }
}

pub struct ImportedSandFrame {
    buffers: Vec<ImportedBuffer>,
    layout: SandLayout,
    pub(super) source: Arc<DecodedVideoFrame>,
}

impl std::fmt::Debug for ImportedSandFrame {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ImportedSandFrame")
            .field("layout", &self.layout)
            .finish_non_exhaustive()
    }
}

pub(super) fn import(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    source: Arc<DecodedVideoFrame>,
) -> Result<ImportedSandFrame> {
    let frame = source.dmabuf.as_ref().expect("DMA-BUF checked");
    let layout = SandLayout::new(frame, source.format)?;
    wait_ready(frame, Duration::ZERO)?;
    let mut properties = vk::ExternalBufferProperties::default();
    unsafe {
        instance.get_physical_device_external_buffer_properties(
            physical,
            &vk::PhysicalDeviceExternalBufferInfo::default()
                .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT),
            &mut properties,
        );
    }
    if !properties
        .external_memory_properties
        .external_memory_features
        .contains(vk::ExternalMemoryFeatureFlags::IMPORTABLE)
    {
        return Err(Error::unavailable(
            Subsystem::Vulkan,
            "Vulkan device cannot import SAND DMA-BUF as TRANSFER_SRC buffers",
        ));
    }
    let buffers = frame
        .objects
        .iter()
        .map(|object| import_buffer(instance, physical, device, object))
        .collect::<Result<Vec<_>>>()?;
    Ok(ImportedSandFrame {
        buffers,
        layout,
        source,
    })
}

fn import_buffer(
    instance: &ash::Instance,
    physical: vk::PhysicalDevice,
    device: &ash::Device,
    object: &crate::DmaBufObject,
) -> Result<ImportedBuffer> {
    let mut external = vk::ExternalMemoryBufferCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT);
    let buffer = unsafe {
        device.create_buffer(
            &vk::BufferCreateInfo::default()
                .push_next(&mut external)
                .size(object.size as u64)
                .usage(vk::BufferUsageFlags::TRANSFER_SRC)
                .sharing_mode(vk::SharingMode::EXCLUSIVE),
            None,
        )
    }
    .map_err(|error| vk_error("create SAND DMA-BUF buffer", error))?;
    let mut imported = ImportedBuffer {
        buffer,
        memory: vk::DeviceMemory::null(),
        device: device.clone(),
    };
    let requirements = unsafe { device.get_buffer_memory_requirements(buffer) };
    if requirements.size > object.size as u64 {
        return Err(invalid(
            "DMA-BUF allocation is smaller than Vulkan buffer memory requirements",
        ));
    }
    let loader = ash::khr::external_memory_fd::Device::new(instance, device);
    let mut properties = vk::MemoryFdPropertiesKHR::default();
    unsafe {
        loader.get_memory_fd_properties(
            vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT,
            object.fd,
            &mut properties,
        )
    }
    .map_err(|error| vk_error("query SAND DMA-BUF memory properties", error))?;
    let memory_type = find_memory_type(
        instance,
        physical,
        requirements.memory_type_bits & properties.memory_type_bits,
        vk::MemoryPropertyFlags::empty(),
    )?;
    let fd = unsafe { libc::fcntl(object.fd, libc::F_DUPFD_CLOEXEC, 0) };
    if fd < 0 {
        return Err(Error::backend(
            Subsystem::Vulkan,
            format!(
                "duplicate SAND DMA-BUF failed: {}",
                std::io::Error::last_os_error()
            ),
        ));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(fd) };
    let mut import = vk::ImportMemoryFdInfoKHR::default()
        .handle_type(vk::ExternalMemoryHandleTypeFlags::DMA_BUF_EXT)
        .fd(fd.as_raw_fd());
    let mut dedicated = vk::MemoryDedicatedAllocateInfo::default().buffer(buffer);
    imported.memory = unsafe {
        device.allocate_memory(
            &vk::MemoryAllocateInfo::default()
                .push_next(&mut import)
                .push_next(&mut dedicated)
                .allocation_size(object.size as u64)
                .memory_type_index(memory_type),
            None,
        )
    }
    .map_err(|error| vk_error("import SAND DMA-BUF memory", error))?;
    std::mem::forget(fd);
    unsafe { device.bind_buffer_memory(buffer, imported.memory, 0) }
        .map_err(|error| vk_error("bind SAND DMA-BUF buffer", error))?;
    Ok(imported)
}

pub(super) struct SandImages {
    planes: [GpuImage; 2],
    device: ash::Device,
    initialized: bool,
}

impl SandImages {
    pub(super) fn new(
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
        width: u32,
        height: u32,
    ) -> Result<Self> {
        let mut images = Vec::with_capacity(2);
        for (index, format) in [vk::Format::R8_UNORM, vk::Format::R8G8_UNORM]
            .into_iter()
            .enumerate()
        {
            let result = (|| {
                let properties =
                    unsafe { instance.get_physical_device_format_properties(physical, format) };
                let required = vk::FormatFeatureFlags::TRANSFER_DST
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR;
                if !properties.optimal_tiling_features.contains(required) {
                    return Err(Error::unavailable(
                        Subsystem::Vulkan,
                        format!(
                            "SAND conversion requires {format:?} transfer destination and linear sampling support"
                        ),
                    ));
                }
                create_gpu_image(
                    instance,
                    physical,
                    device,
                    width >> index,
                    height >> index,
                    format,
                    vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
                )
            })();
            match result {
                Ok(image) => images.push(image),
                Err(error) => {
                    for image in images {
                        destroy_gpu_image(device, image);
                    }
                    return Err(error);
                }
            }
        }
        Ok(Self {
            planes: images.try_into().ok().expect("two images"),
            device: device.clone(),
            initialized: false,
        })
    }

    pub(super) fn views(&self) -> (vk::ImageView, vk::ImageView) {
        (self.planes[0].view, self.planes[1].view)
    }

    pub(super) fn record(
        &mut self,
        device: &ash::Device,
        command: vk::CommandBuffer,
        family: u32,
        frame: &ImportedSandFrame,
    ) {
        let acquire = frame
            .buffers
            .iter()
            .map(|buffer| {
                vk::BufferMemoryBarrier::default()
                    .src_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                    .dst_queue_family_index(family)
                    .src_access_mask(vk::AccessFlags::empty())
                    .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .buffer(buffer.buffer)
                    .size(vk::WHOLE_SIZE)
            })
            .collect::<Vec<_>>();
        unsafe {
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &acquire,
                &[],
            );
        }
        self.record_pixels(device, command, frame);
        let release = frame
            .buffers
            .iter()
            .map(|buffer| {
                vk::BufferMemoryBarrier::default()
                    .src_queue_family_index(family)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_FOREIGN_EXT)
                    .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                    .dst_access_mask(vk::AccessFlags::empty())
                    .buffer(buffer.buffer)
                    .size(vk::WHOLE_SIZE)
            })
            .collect::<Vec<_>>();
        unsafe {
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &release,
                &[],
            );
        }
    }

    fn record_pixels(
        &mut self,
        device: &ash::Device,
        command: vk::CommandBuffer,
        frame: &ImportedSandFrame,
    ) {
        let images = self
            .planes
            .iter()
            .map(|image| {
                vk::ImageMemoryBarrier::default()
                    .old_layout(if self.initialized {
                        vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
                    } else {
                        vk::ImageLayout::UNDEFINED
                    })
                    .new_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .src_access_mask(if self.initialized {
                        vk::AccessFlags::SHADER_READ
                    } else {
                        vk::AccessFlags::empty()
                    })
                    .dst_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .image(image.image)
                    .subresource_range(image_color_range())
            })
            .collect::<Vec<_>>();
        unsafe {
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::PipelineStageFlags::TRANSFER,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &images,
            );
            for (image, plane) in self.planes.iter().zip(&frame.layout.planes) {
                device.cmd_copy_buffer_to_image(
                    command,
                    frame.buffers[plane.object].buffer,
                    image.image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &plane.regions,
                );
            }
        }
        let images = self
            .planes
            .iter()
            .map(|image| {
                vk::ImageMemoryBarrier::default()
                    .old_layout(vk::ImageLayout::TRANSFER_DST_OPTIMAL)
                    .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                    .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                    .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                    .dst_access_mask(vk::AccessFlags::SHADER_READ)
                    .image(image.image)
                    .subresource_range(image_color_range())
            })
            .collect::<Vec<_>>();
        unsafe {
            device.cmd_pipeline_barrier(
                command,
                vk::PipelineStageFlags::TRANSFER,
                vk::PipelineStageFlags::ALL_COMMANDS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &images,
            );
        }
        self.initialized = true;
    }
}

impl Drop for SandImages {
    fn drop(&mut self) {
        for image in &self.planes {
            unsafe {
                self.device.destroy_image_view(image.view, None);
                self.device.destroy_image(image.image, None);
                self.device.free_memory(image.memory, None);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DmaBufLayer, DmaBufObject, StreamFormat};

    fn fixture(split: bool, width: u32, height: u32) -> (DmaBufFrame, StreamFormat) {
        let columns = width.div_ceil(128) as usize;
        let rows = height as usize;
        let (objects, chroma, pitch) = if split {
            (
                vec![
                    DmaBufObject {
                        fd: 0,
                        size: columns * 128 * rows,
                        format_modifier: SAND128,
                    },
                    DmaBufObject {
                        fd: 1,
                        size: columns * 128 * rows / 2,
                        format_modifier: SAND128,
                    },
                ],
                DmaBufPlane {
                    object_index: 1,
                    offset: 0,
                    pitch: rows / 2,
                },
                rows,
            )
        } else {
            let column_height = rows * 3 / 2 + 32;
            (
                vec![DmaBufObject {
                    fd: 0,
                    size: columns * 128 * column_height,
                    format_modifier: SAND128 | ((column_height as u64) << 8),
                }],
                DmaBufPlane {
                    object_index: 0,
                    offset: rows * 128,
                    pitch: width as usize,
                },
                width as usize,
            )
        };
        (
            DmaBufFrame::new(
                objects,
                vec![DmaBufLayer {
                    format: DRM_FORMAT_NV12,
                    planes: vec![
                        DmaBufPlane {
                            object_index: 0,
                            offset: 0,
                            pitch,
                        },
                        chroma,
                    ],
                }],
                Arc::new(()),
            ),
            StreamFormat::video_default(width, height).unwrap(),
        )
    }

    #[test]
    fn nc12_and_split_nc12_copy_visible_columns_without_padding() {
        for split in [false, true] {
            for (width, height) in [(2, 2), (128, 64), (130, 66), (1920, 1080), (3840, 2160)] {
                let (frame, format) = fixture(split, width, height);
                let layout = SandLayout::new(&frame, format).unwrap();
                for (index, plane) in layout.planes.iter().enumerate() {
                    let metadata = frame.layers[0].planes[index];
                    let column_height = if split {
                        metadata.pitch as u64
                    } else {
                        (frame.objects[0].format_modifier >> 8) & PARAM_MASK
                    };
                    assert_eq!(plane.object, metadata.object_index);
                    assert_eq!(plane.regions.len(), width.div_ceil(128) as usize);
                    let mut copied = 0;
                    for (column, region) in plane.regions.iter().enumerate() {
                        assert_eq!(
                            region.buffer_offset,
                            metadata.offset as u64 + column as u64 * 128 * column_height
                        );
                        assert_eq!(region.buffer_row_length, 128 >> index);
                        assert_eq!(region.image_offset.x, (column * (128 >> index)) as i32);
                        assert_eq!(region.image_extent.height, height >> index);
                        assert_eq!(region.image_extent.depth, 1);
                        assert_eq!(region.buffer_offset % 4, 0);
                        copied += region.image_extent.width;
                    }
                    assert_eq!(copied, width >> index);
                }
            }
        }
    }

    #[test]
    fn sand_selector_preserves_generic_modifier_imports() {
        let (mut frame, _) = fixture(false, 130, 66);
        for modifier in [SAND128, SAND128 | (128 << 8)] {
            frame.objects[0].format_modifier = modifier;
            assert!(is_sand(&frame));
        }
        for modifier in [
            0,
            (0x07 << 56) | 1,
            (0x07 << 56) | 2,
            (0x07 << 56) | 3,
            (0x07 << 56) | 5,
            (0x07 << 56) | 6,
        ] {
            frame.objects[0].format_modifier = modifier;
            assert!(!is_sand(&frame));
        }
        frame.layers[0].format = P030;
        assert!(is_sand(&frame));
    }

    #[test]
    fn sand_rejects_p030_wrong_modifier_depth_and_hdr() {
        let (mut frame, mut format) = fixture(false, 130, 66);
        frame.layers[0].format = P030;
        assert!(
            SandLayout::new(&frame, format)
                .unwrap_err()
                .to_string()
                .contains("P030")
        );
        frame.layers[0].format = DRM_FORMAT_NV12;
        for modifier in [
            0,
            u64::MAX,
            (0x07 << 56) | 3,
            (0x07 << 56) | 5,
            (0x08 << 56) | 4,
        ] {
            frame.objects[0].format_modifier = modifier;
            assert!(SandLayout::new(&frame, format).is_err());
        }
        let (frame, _) = fixture(false, 130, 66);
        format.pixel_format = PixelFormat::P010;
        assert!(SandLayout::new(&frame, format).is_err());
        format.color_transfer = crate::ColorTransfer::Pq;
        format.color_primaries = crate::ColorPrimaries::Bt2020;
        assert!(SandLayout::new(&frame, format).is_err());
    }

    #[test]
    fn sand_rejects_invalid_object_extents_alignment_overlap_and_overflow() {
        for split in [false, true] {
            let (_, format) = fixture(split, 130, 66);
            for index in 0..2 {
                let (mut short, _) = fixture(split, 130, 66);
                let object = short.layers[0].planes[index].object_index;
                short.objects[object].size = 128;
                assert!(SandLayout::new(&short, format).is_err());
                let (mut unaligned, _) = fixture(split, 130, 66);
                unaligned.layers[0].planes[index].offset += 1;
                assert!(SandLayout::new(&unaligned, format).is_err());
                let (mut overflow, _) = fixture(split, 130, 66);
                overflow.layers[0].planes[index].offset = usize::MAX - 127;
                assert!(SandLayout::new(&overflow, format).is_err());
                let (mut out_of_bounds, _) = fixture(split, 130, 66);
                out_of_bounds.layers[0].planes[index].object_index = 9;
                assert!(SandLayout::new(&out_of_bounds, format).is_err());
            }
        }
        let (mut frame, format) = fixture(false, 130, 66);
        frame.layers[0].planes[1].offset = 128;
        assert!(SandLayout::new(&frame, format).is_err());
        let (mut frame, format) = fixture(true, 130, 66);
        frame.layers[0].planes[0].pitch = usize::MAX;
        assert!(SandLayout::new(&frame, format).is_err());
        frame.layers[0].planes[0].pitch = 65;
        assert!(SandLayout::new(&frame, format).is_err());
    }

    #[test]
    fn split_sand_exception_does_not_accept_generic_disjoint_nv12() {
        let (mut frame, format) = fixture(true, 130, 66);
        assert!(SandLayout::new(&frame, format).is_ok());
        assert!(yuv_dmabuf_layout(&frame, PixelFormat::Nv12).is_err());
        for object in &mut frame.objects {
            object.format_modifier = 0;
        }
        assert!(!is_sand(&frame));
        assert!(SandLayout::new(&frame, format).is_err());
        assert!(yuv_dmabuf_layout(&frame, PixelFormat::Nv12).is_err());
    }

    #[test]
    fn sand_requires_unambiguous_single_or_split_object_layout() {
        let (mut frame, format) = fixture(false, 130, 66);
        frame.objects[0].format_modifier = SAND128;
        assert!(SandLayout::new(&frame, format).is_err());
        let (mut frame, format) = fixture(true, 130, 66);
        frame.objects[1].format_modifier |= 66 << 8;
        assert!(SandLayout::new(&frame, format).is_err());
        let (mut frame, format) = fixture(true, 130, 66);
        frame.layers[0].planes[1].object_index = 0;
        assert!(SandLayout::new(&frame, format).is_err());
    }

    #[test]
    fn readiness_rejects_unsignaled_and_invalid_descriptors() {
        let (mut frame, _) = fixture(false, 130, 66);
        frame.objects[0].fd = i32::MAX;
        assert!(matches!(
            wait_ready(&frame, Duration::ZERO),
            Err(Error::Unavailable { .. })
        ));
        let mut fds = [-1; 2];
        assert_eq!(
            unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC | libc::O_NONBLOCK) },
            0
        );
        let read = unsafe { OwnedFd::from_raw_fd(fds[0]) };
        let write = unsafe { OwnedFd::from_raw_fd(fds[1]) };
        frame.objects[0].fd = read.as_raw_fd();
        assert!(matches!(
            wait_ready(&frame, Duration::ZERO),
            Err(Error::FrameNotReady)
        ));
        assert_eq!(
            unsafe { libc::write(write.as_raw_fd(), [1u8].as_ptr().cast(), 1) },
            1
        );
        wait_ready(&frame, Duration::ZERO).unwrap();
    }

    fn host_buffer(
        instance: &ash::Instance,
        physical: vk::PhysicalDevice,
        device: &ash::Device,
        size: u64,
        usage: vk::BufferUsageFlags,
    ) -> ImportedBuffer {
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
            ImportedBuffer {
                buffer,
                memory,
                device: device.clone(),
            }
        }
    }

    #[test]
    #[ignore = "requires Vulkan; synthetic buffers run with Mesa lavapipe without DMA-BUF import"]
    fn sand_gpu_columns_and_nv12_conversion_match_pixels_across_slot_reuse() {
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
                dmabuf_import_enabled: true,
                dmabuf_buffer_import_enabled: false,
            };
            let (width, height) = (130, 66);
            let pixels = (width * height) as usize;
            let readback_size = pixels * 4 + pixels * 3 / 2;
            let readback = host_buffer(
                &instance,
                physical,
                &device,
                readback_size as u64,
                vk::BufferUsageFlags::TRANSFER_DST,
            );
            let mut producer = LinuxFrameProducer::new_with_slots(render, 1).unwrap();
            let (metadata, format) = fixture(false, width, height);
            let disabled = producer
                .prepare(DecodedVideoFrame {
                    format,
                    planes: Vec::new(),
                    dmabuf: Some(Arc::new(metadata)),
                    vulkan: None,
                    timestamp_us: 0,
                })
                .unwrap_err();
            assert!(disabled.to_string().contains("external buffer import"));
            let normal_images =
                SandImages::new(&instance, physical, &device, width, height).unwrap();
            assert_ne!(normal_images.views().0, vk::ImageView::null());
            assert_ne!(normal_images.views().1, vk::ImageView::null());
            drop(normal_images);
            producer.ensure_renderer(GpuTextureFormat::Rgba8).unwrap();
            let output = create_gpu_image(
                &instance,
                physical,
                &device,
                width,
                height,
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
                        .width(width)
                        .height(height)
                        .layers(1),
                    None,
                )
                .unwrap();
            producer.slots[0] = Some(FrameSlotResources {
                width,
                height,
                texture_format: GpuTextureFormat::Rgba8,
                output,
                output_initialized: false,
                framebuffer,
                cpu: None,
                sand: None,
                owned_input_views: Vec::new(),
                frame: None,
            });
            let mut images = SandImages {
                planes: [vk::Format::R8_UNORM, vk::Format::R8G8_UNORM]
                    .into_iter()
                    .enumerate()
                    .map(|(index, format)| {
                        create_gpu_image(
                            &instance,
                            physical,
                            &device,
                            width >> index,
                            height >> index,
                            format,
                            vk::ImageUsageFlags::TRANSFER_DST
                                | vk::ImageUsageFlags::SAMPLED
                                | vk::ImageUsageFlags::TRANSFER_SRC,
                        )
                        .unwrap()
                    })
                    .collect::<Vec<_>>()
                    .try_into()
                    .ok()
                    .unwrap(),
                device: device.clone(),
                initialized: false,
            };
            let original_views = images.views();
            for (iteration, split) in [false, true, false, true].into_iter().enumerate() {
                let (metadata, format) = fixture(split, width, height);
                let layout = SandLayout::new(&metadata, format).unwrap();
                let buffers = metadata
                    .objects
                    .iter()
                    .map(|object| {
                        host_buffer(
                            &instance,
                            physical,
                            &device,
                            object.size as u64,
                            vk::BufferUsageFlags::TRANSFER_SRC,
                        )
                    })
                    .collect::<Vec<_>>();
                let u = if split { 180u8 } else { 80 };
                let v = if split { 90u8 } else { 170 };
                let y_value =
                    |x: u32, y: u32| ((x * 3 + y * 5 + iteration as u32 * 17) % 180 + 30) as u8;
                for (index, object) in metadata.objects.iter().enumerate() {
                    let pointer = device
                        .map_memory(
                            buffers[index].memory,
                            0,
                            object.size as u64,
                            vk::MemoryMapFlags::empty(),
                        )
                        .unwrap()
                        .cast::<u8>();
                    let bytes = std::slice::from_raw_parts_mut(pointer, object.size);
                    bytes.fill(0xed);
                    for (plane_index, plane) in metadata.layers[0]
                        .planes
                        .iter()
                        .enumerate()
                        .filter(|(_, plane)| plane.object_index == index)
                    {
                        let column_height = if split {
                            plane.pitch
                        } else {
                            ((object.format_modifier >> 8) & PARAM_MASK) as usize
                        };
                        for row in 0..(height >> plane_index) {
                            for x in 0..width {
                                let offset = plane.offset
                                    + (x as usize / 128) * 128 * column_height
                                    + row as usize * 128
                                    + x as usize % 128;
                                bytes[offset] = if plane_index == 0 {
                                    y_value(x, row)
                                } else if x % 2 == 0 {
                                    u
                                } else {
                                    v
                                };
                            }
                        }
                    }
                    device.unmap_memory(buffers[index].memory);
                }
                let source = Arc::new(DecodedVideoFrame {
                    format,
                    planes: Vec::new(),
                    dmabuf: Some(Arc::new(metadata)),
                    vulkan: None,
                    timestamp_us: iteration as u64,
                });
                let retained = Arc::downgrade(&source);
                let frame = ImportedSandFrame {
                    buffers,
                    layout,
                    source,
                };
                device
                    .reset_command_buffer(command, vk::CommandBufferResetFlags::empty())
                    .unwrap();
                device
                    .begin_command_buffer(command, &vk::CommandBufferBeginInfo::default())
                    .unwrap();
                images.record_pixels(&device, command, &frame);
                assert_eq!(images.views(), original_views);
                producer.update_descriptors(0, images.views().0, images.views().1);
                let prepared = PreparedLinuxFrame::Sand(frame);
                producer
                    .record_conversion(
                        0,
                        command,
                        &prepared,
                        width,
                        height,
                        crate::ColorMatrix::Bt709,
                        true,
                        crate::ChromaLocation::Center,
                    )
                    .unwrap();
                let output_image = producer.slots[0].as_ref().unwrap().output.image;
                let sources = [
                    (output_image, 0, width, height),
                    (images.planes[0].image, pixels * 4, width, height),
                    (images.planes[1].image, pixels * 5, width / 2, height / 2),
                ];
                for (image, offset, width, height) in sources {
                    device.cmd_pipeline_barrier(
                        command,
                        vk::PipelineStageFlags::ALL_COMMANDS,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[vk::ImageMemoryBarrier::default()
                            .old_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                            .src_access_mask(
                                vk::AccessFlags::MEMORY_WRITE | vk::AccessFlags::SHADER_READ,
                            )
                            .dst_access_mask(vk::AccessFlags::TRANSFER_READ)
                            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .image(image)
                            .subresource_range(image_color_range())],
                    );
                    device.cmd_copy_image_to_buffer(
                        command,
                        image,
                        vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
                        readback.buffer,
                        &[vk::BufferImageCopy::default()
                            .buffer_offset(offset as u64)
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
                    device.cmd_pipeline_barrier(
                        command,
                        vk::PipelineStageFlags::TRANSFER,
                        vk::PipelineStageFlags::FRAGMENT_SHADER,
                        vk::DependencyFlags::empty(),
                        &[],
                        &[],
                        &[vk::ImageMemoryBarrier::default()
                            .old_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
                            .new_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
                            .src_access_mask(vk::AccessFlags::TRANSFER_READ)
                            .dst_access_mask(vk::AccessFlags::SHADER_READ)
                            .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                            .image(image)
                            .subresource_range(image_color_range())],
                    );
                }
                device.cmd_pipeline_barrier(
                    command,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::HOST,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[vk::BufferMemoryBarrier::default()
                        .src_access_mask(vk::AccessFlags::TRANSFER_WRITE)
                        .dst_access_mask(vk::AccessFlags::HOST_READ)
                        .src_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .dst_queue_family_index(vk::QUEUE_FAMILY_IGNORED)
                        .buffer(readback.buffer)
                        .size(vk::WHOLE_SIZE)],
                    &[],
                );
                device.end_command_buffer(command).unwrap();
                producer.slots[0].as_mut().unwrap().frame = Some(prepared);
                assert!(retained.upgrade().is_some());
                device
                    .queue_submit(
                        queue,
                        &[vk::SubmitInfo::default().command_buffers(&commands)],
                        vk::Fence::null(),
                    )
                    .unwrap();
                device.queue_wait_idle(queue).unwrap();
                let pointer = device
                    .map_memory(
                        readback.memory,
                        0,
                        readback_size as u64,
                        vk::MemoryMapFlags::empty(),
                    )
                    .unwrap()
                    .cast::<u8>();
                let bytes = std::slice::from_raw_parts(pointer, readback_size);
                for y in 0..height {
                    for x in 0..width {
                        let index = (y * width + x) as usize;
                        let luma = y_value(x, y);
                        assert_eq!(
                            bytes[pixels * 4 + index],
                            luma,
                            "luma {split}/{iteration} ({x},{y})"
                        );
                        let expected = [
                            luma as f32 + 1.5748 * (v as f32 - 128.0),
                            luma as f32
                                - 0.187324 * (u as f32 - 128.0)
                                - 0.468124 * (v as f32 - 128.0),
                            luma as f32 + 1.8556 * (u as f32 - 128.0),
                        ];
                        for channel in 0..3 {
                            let wanted = expected[channel].clamp(0.0, 255.0).round() as i16;
                            assert!(
                                (bytes[index * 4 + channel] as i16 - wanted).abs() <= 1,
                                "RGBA {split}/{iteration} ({x},{y}) channel {channel}: {} != {wanted}",
                                bytes[index * 4 + channel]
                            );
                        }
                        assert_eq!(bytes[index * 4 + 3], 255);
                    }
                }
                for pair in bytes[pixels * 5..].chunks_exact(2) {
                    assert_eq!(pair, [u, v]);
                }
                device.unmap_memory(readback.memory);
                producer.slots[0].as_mut().unwrap().frame = None;
                assert!(retained.upgrade().is_none());
            }
            drop(images);
            drop(producer);
            drop(readback);
            device.destroy_command_pool(pool, None);
            device.destroy_device(None);
            instance.destroy_instance(None);
        }
    }
}
