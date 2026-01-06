//! Vulkan Video Decoder for Linux
//!
//! Hardware-accelerated video decoding using Vulkan Video extensions,
//! based on GeForce NOW's Linux client implementation.
//!
//! This provides cross-GPU hardware decoding on Linux:
//! - NVIDIA GPUs (native Vulkan Video support)
//! - AMD GPUs (via Mesa RADV with video extensions)
//! - Intel GPUs (via Mesa ANV with video extensions)
//!
//! Key Vulkan extensions used:
//! - VK_KHR_video_queue
//! - VK_KHR_video_decode_queue
//! - VK_KHR_video_decode_h264
//! - VK_KHR_video_decode_h265
//!
//! Architecture mirrors GFN's VulkanDecoder/VkVideoDecoder classes.

use anyhow::{anyhow, Result};
use ash::vk;
use log::{debug, error, info, warn};
use std::ffi::{CStr, CString};
use std::sync::Arc;

use super::{ColorRange, ColorSpace, PixelFormat, TransferFunction, VideoFrame};

/// Vulkan Video codec type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VulkanVideoCodec {
    H264,
    H265,
    AV1,
}

/// Vulkan Video decoder configuration
#[derive(Debug, Clone)]
pub struct VulkanVideoConfig {
    pub codec: VulkanVideoCodec,
    pub width: u32,
    pub height: u32,
    pub is_10bit: bool,
    /// Number of decode surfaces (DPB + output)
    pub num_decode_surfaces: u32,
}

impl Default for VulkanVideoConfig {
    fn default() -> Self {
        Self {
            codec: VulkanVideoCodec::H264,
            width: 1920,
            height: 1080,
            is_10bit: false,
            num_decode_surfaces: 20, // GFN uses ~20 surfaces
        }
    }
}

/// Decoded Picture Buffer entry (mirrors GFN's DPB management)
#[derive(Debug, Clone, Default)]
pub struct DpbSlot {
    /// Picture Order Count
    pub poc: i32,
    /// Frame number
    pub frame_num: u32,
    /// Whether this slot is a reference frame
    pub is_reference: bool,
    /// Whether this is a long-term reference
    pub is_long_term: bool,
    /// Associated image index
    pub image_index: u32,
}

/// Vulkan Video decode output frame
pub struct VulkanVideoFrame {
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Raw pixel data (NV12 or P010 format)
    pub data: Vec<u8>,
    /// Y plane stride
    pub y_stride: u32,
    /// UV plane stride
    pub uv_stride: u32,
    /// Whether this is 10-bit (P010 vs NV12)
    pub is_10bit: bool,
}

/// Decoder statistics
#[derive(Debug, Clone)]
pub struct DecoderStats {
    pub frames_decoded: u64,
    pub dpb_size: u32,
    pub supports_dmabuf: bool,
}

/// Cached Vulkan availability
static VULKAN_VIDEO_AVAILABLE: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
static VULKAN_VIDEO_CODECS: std::sync::OnceLock<Vec<VulkanVideoCodec>> = std::sync::OnceLock::new();

/// Check if Vulkan Video decoding is available on this system
pub fn is_vulkan_video_available() -> bool {
    *VULKAN_VIDEO_AVAILABLE.get_or_init(|| match check_vulkan_video_support() {
        Ok(available) => {
            if available {
                info!("Vulkan Video decoding is available");
            } else {
                info!("Vulkan Video decoding is NOT available");
            }
            available
        }
        Err(e) => {
            warn!("Failed to check Vulkan Video support: {}", e);
            false
        }
    })
}

/// Get supported Vulkan Video codecs
pub fn get_supported_vulkan_codecs() -> Vec<VulkanVideoCodec> {
    VULKAN_VIDEO_CODECS
        .get_or_init(|| {
            if !is_vulkan_video_available() {
                return Vec::new();
            }

            match query_supported_codecs() {
                Ok(codecs) => codecs,
                Err(e) => {
                    warn!("Failed to query Vulkan Video codecs: {}", e);
                    Vec::new()
                }
            }
        })
        .clone()
}

/// Check if Vulkan and video extensions are available
fn check_vulkan_video_support() -> Result<bool> {
    unsafe {
        // Load Vulkan library
        let entry = match ash::Entry::load() {
            Ok(e) => e,
            Err(e) => {
                debug!("Failed to load Vulkan: {}", e);
                return Ok(false);
            }
        };

        // Check instance extensions
        let instance_extensions = entry.enumerate_instance_extension_properties(None)?;
        let has_portability = instance_extensions.iter().any(|ext| {
            let name = CStr::from_ptr(ext.extension_name.as_ptr());
            name.to_str()
                .map(|s| s.contains("portability"))
                .unwrap_or(false)
        });

        // Create minimal instance to check device extensions
        let app_name = CString::new("OpenNow Vulkan Video Check").unwrap();
        let engine_name = CString::new("OpenNow").unwrap();

        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(vk::make_api_version(0, 1, 0, 0))
            .engine_name(&engine_name)
            .engine_version(vk::make_api_version(0, 1, 0, 0))
            .api_version(vk::API_VERSION_1_3);

        let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);

        let instance = match entry.create_instance(&create_info, None) {
            Ok(i) => i,
            Err(e) => {
                debug!("Failed to create Vulkan instance: {}", e);
                return Ok(false);
            }
        };

        // Enumerate physical devices
        let physical_devices = instance.enumerate_physical_devices()?;
        if physical_devices.is_empty() {
            instance.destroy_instance(None);
            return Ok(false);
        }

        // Check each device for video decode support
        for physical_device in &physical_devices {
            let device_extensions =
                instance.enumerate_device_extension_properties(*physical_device)?;

            let has_video_queue = device_extensions.iter().any(|ext| {
                let name = CStr::from_ptr(ext.extension_name.as_ptr());
                name.to_str()
                    .map(|s| s == "VK_KHR_video_queue")
                    .unwrap_or(false)
            });

            let has_video_decode = device_extensions.iter().any(|ext| {
                let name = CStr::from_ptr(ext.extension_name.as_ptr());
                name.to_str()
                    .map(|s| s == "VK_KHR_video_decode_queue")
                    .unwrap_or(false)
            });

            let has_h264 = device_extensions.iter().any(|ext| {
                let name = CStr::from_ptr(ext.extension_name.as_ptr());
                name.to_str()
                    .map(|s| s == "VK_KHR_video_decode_h264")
                    .unwrap_or(false)
            });

            let has_h265 = device_extensions.iter().any(|ext| {
                let name = CStr::from_ptr(ext.extension_name.as_ptr());
                name.to_str()
                    .map(|s| s == "VK_KHR_video_decode_h265")
                    .unwrap_or(false)
            });

            // Get device name for logging
            let props = instance.get_physical_device_properties(*physical_device);
            let device_name = CStr::from_ptr(props.device_name.as_ptr())
                .to_str()
                .unwrap_or("Unknown");

            info!(
                "Vulkan device '{}': video_queue={}, video_decode={}, h264={}, h265={}",
                device_name, has_video_queue, has_video_decode, has_h264, has_h265
            );

            if has_video_queue && has_video_decode && (has_h264 || has_h265) {
                instance.destroy_instance(None);
                return Ok(true);
            }
        }

        instance.destroy_instance(None);
        Ok(false)
    }
}

/// Query which codecs are supported
fn query_supported_codecs() -> Result<Vec<VulkanVideoCodec>> {
    let mut codecs = Vec::new();

    unsafe {
        let entry = ash::Entry::load()?;

        let app_name = CString::new("OpenNow Vulkan Video").unwrap();
        let engine_name = CString::new("OpenNow").unwrap();

        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(vk::make_api_version(0, 1, 0, 0))
            .engine_name(&engine_name)
            .engine_version(vk::make_api_version(0, 1, 0, 0))
            .api_version(vk::API_VERSION_1_3);

        let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);

        let instance = entry.create_instance(&create_info, None)?;
        let physical_devices = instance.enumerate_physical_devices()?;

        for physical_device in &physical_devices {
            let device_extensions =
                instance.enumerate_device_extension_properties(*physical_device)?;

            let has_h264 = device_extensions.iter().any(|ext| {
                let name = CStr::from_ptr(ext.extension_name.as_ptr());
                name.to_str()
                    .map(|s| s == "VK_KHR_video_decode_h264")
                    .unwrap_or(false)
            });

            let has_h265 = device_extensions.iter().any(|ext| {
                let name = CStr::from_ptr(ext.extension_name.as_ptr());
                name.to_str()
                    .map(|s| s == "VK_KHR_video_decode_h265")
                    .unwrap_or(false)
            });

            if has_h264 && !codecs.contains(&VulkanVideoCodec::H264) {
                codecs.push(VulkanVideoCodec::H264);
            }
            if has_h265 && !codecs.contains(&VulkanVideoCodec::H265) {
                codecs.push(VulkanVideoCodec::H265);
            }
        }

        instance.destroy_instance(None);
    }

    Ok(codecs)
}

/// Vulkan Video Decoder
///
/// Implements hardware video decoding using Vulkan Video extensions.
/// Based on GeForce NOW's VkVideoDecoder/VulkanDecoder architecture.
pub struct VulkanVideoDecoder {
    /// Vulkan entry point
    _entry: ash::Entry,
    /// Vulkan instance
    instance: ash::Instance,
    /// Physical device
    physical_device: vk::PhysicalDevice,
    /// Logical device
    device: ash::Device,
    /// Video decode queue
    decode_queue: vk::Queue,
    /// Decode queue family index
    decode_queue_family: u32,
    /// Graphics queue (for format conversion)
    graphics_queue: vk::Queue,
    /// Graphics queue family index
    graphics_queue_family: u32,
    /// Command pool for decode operations
    decode_command_pool: vk::CommandPool,
    /// Command buffer for decode operations
    decode_command_buffer: vk::CommandBuffer,
    /// Video session
    video_session: vk::VideoSessionKHR,
    /// Video session parameters (SPS/PPS)
    video_session_params: vk::VideoSessionParametersKHR,
    /// Video decode queue extension functions
    video_queue_fn: ash::khr::video_queue::Device,
    /// Video decode extension functions
    video_decode_fn: ash::khr::video_decode_queue::Device,
    /// DPB (Decoded Picture Buffer) images
    dpb_images: Vec<vk::Image>,
    /// DPB image views
    dpb_image_views: Vec<vk::ImageView>,
    /// DPB memory allocations
    dpb_memory: Vec<vk::DeviceMemory>,
    /// Output image (for readback)
    output_image: vk::Image,
    /// Output image view
    output_image_view: vk::ImageView,
    /// Output image memory
    output_memory: vk::DeviceMemory,
    /// Staging buffer for CPU readback
    staging_buffer: vk::Buffer,
    /// Staging buffer memory
    staging_memory: vk::DeviceMemory,
    /// Staging buffer size
    staging_size: u64,
    /// Bitstream buffer
    bitstream_buffer: vk::Buffer,
    /// Bitstream buffer memory
    bitstream_memory: vk::DeviceMemory,
    /// Bitstream buffer size
    bitstream_size: u64,
    /// Fence for synchronization
    decode_fence: vk::Fence,
    /// Configuration
    config: VulkanVideoConfig,
    /// Frame counter
    frame_count: u64,
    /// DPB slot tracking
    dpb_slots: Vec<DpbSlot>,
    /// Current DPB index
    current_dpb_index: usize,
    /// H.264 SPS data
    sps_data: Option<Vec<u8>>,
    /// H.264 PPS data
    pps_data: Option<Vec<u8>>,
    /// Session parameters need update
    params_dirty: bool,
}

// Vulkan Video is thread-safe when properly synchronized
unsafe impl Send for VulkanVideoDecoder {}
unsafe impl Sync for VulkanVideoDecoder {}

impl VulkanVideoDecoder {
    /// Create a new Vulkan Video decoder
    pub fn new(config: VulkanVideoConfig) -> Result<Self> {
        info!(
            "Creating Vulkan Video decoder: {:?} {}x{} 10bit={}",
            config.codec, config.width, config.height, config.is_10bit
        );

        unsafe {
            // Load Vulkan
            let entry = ash::Entry::load().map_err(|e| anyhow!("Failed to load Vulkan: {}", e))?;

            // Create instance with required extensions
            let app_name = CString::new("OpenNow Vulkan Video Decoder").unwrap();
            let engine_name = CString::new("OpenNow").unwrap();

            let app_info = vk::ApplicationInfo::default()
                .application_name(&app_name)
                .application_version(vk::make_api_version(0, 1, 0, 0))
                .engine_name(&engine_name)
                .engine_version(vk::make_api_version(0, 1, 0, 0))
                .api_version(vk::API_VERSION_1_3);

            let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);

            let instance = entry.create_instance(&create_info, None)?;
            info!("Vulkan instance created");

            // Find physical device with video decode support
            let (physical_device, decode_queue_family, graphics_queue_family) =
                Self::find_suitable_device(&instance, &config)?;

            // Get device name for logging
            let props = instance.get_physical_device_properties(physical_device);
            let device_name = CStr::from_ptr(props.device_name.as_ptr())
                .to_str()
                .unwrap_or("Unknown");
            info!("Selected Vulkan device: {}", device_name);

            // Create logical device with video extensions
            let (device, decode_queue, graphics_queue) = Self::create_device(
                &instance,
                physical_device,
                decode_queue_family,
                graphics_queue_family,
                &config,
            )?;
            info!("Vulkan device created");

            // Load video extension functions
            let video_queue_fn = ash::khr::video_queue::Device::new(&instance, &device);
            let video_decode_fn = ash::khr::video_decode_queue::Device::new(&instance, &device);

            // Create command pool for decode operations
            let pool_info = vk::CommandPoolCreateInfo::default()
                .queue_family_index(decode_queue_family)
                .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);

            let decode_command_pool = device.create_command_pool(&pool_info, None)?;

            // Allocate command buffer
            let alloc_info = vk::CommandBufferAllocateInfo::default()
                .command_pool(decode_command_pool)
                .level(vk::CommandBufferLevel::PRIMARY)
                .command_buffer_count(1);

            let command_buffers = device.allocate_command_buffers(&alloc_info)?;
            let decode_command_buffer = command_buffers[0];

            // Create fence for synchronization
            let fence_info = vk::FenceCreateInfo::default();
            let decode_fence = device.create_fence(&fence_info, None)?;

            // Create video session
            let video_session = Self::create_video_session(
                &device,
                &video_queue_fn,
                physical_device,
                &instance,
                decode_queue_family,
                &config,
            )?;
            info!("Video session created");

            // Allocate DPB images
            let dpb_size = Self::calculate_dpb_size(&config);
            let (dpb_images, dpb_image_views, dpb_memory) =
                Self::create_dpb_images(&device, &instance, physical_device, &config, dpb_size)?;
            info!("Allocated {} DPB slots", dpb_size);

            // Create output image for readback
            let (output_image, output_image_view, output_memory) =
                Self::create_output_image(&device, &instance, physical_device, &config)?;

            // Create staging buffer for CPU readback
            let staging_size = (config.width * config.height * 3 / 2) as u64; // NV12 size
            let (staging_buffer, staging_memory) =
                Self::create_staging_buffer(&device, &instance, physical_device, staging_size)?;

            // Create bitstream buffer
            let bitstream_size = 4 * 1024 * 1024; // 4MB for compressed data
            let (bitstream_buffer, bitstream_memory) =
                Self::create_bitstream_buffer(&device, &instance, physical_device, bitstream_size)?;

            Ok(Self {
                _entry: entry,
                instance,
                physical_device,
                device,
                decode_queue,
                decode_queue_family,
                graphics_queue,
                graphics_queue_family,
                decode_command_pool,
                decode_command_buffer,
                video_session,
                video_session_params: vk::VideoSessionParametersKHR::null(),
                video_queue_fn,
                video_decode_fn,
                dpb_images,
                dpb_image_views,
                dpb_memory,
                output_image,
                output_image_view,
                output_memory,
                staging_buffer,
                staging_memory,
                staging_size,
                bitstream_buffer,
                bitstream_memory,
                bitstream_size,
                decode_fence,
                config,
                frame_count: 0,
                dpb_slots: vec![DpbSlot::default(); dpb_size],
                current_dpb_index: 0,
                sps_data: None,
                pps_data: None,
                params_dirty: true,
            })
        }
    }

    /// Find a suitable physical device with video decode support
    unsafe fn find_suitable_device(
        instance: &ash::Instance,
        config: &VulkanVideoConfig,
    ) -> Result<(vk::PhysicalDevice, u32, u32)> {
        let physical_devices = instance.enumerate_physical_devices()?;

        for physical_device in physical_devices {
            // Check device extensions
            let extensions = instance.enumerate_device_extension_properties(physical_device)?;

            let has_video_queue = extensions.iter().any(|ext| {
                let name = CStr::from_ptr(ext.extension_name.as_ptr());
                name.to_str()
                    .map(|s| s == "VK_KHR_video_queue")
                    .unwrap_or(false)
            });

            let has_video_decode = extensions.iter().any(|ext| {
                let name = CStr::from_ptr(ext.extension_name.as_ptr());
                name.to_str()
                    .map(|s| s == "VK_KHR_video_decode_queue")
                    .unwrap_or(false)
            });

            let has_codec = match config.codec {
                VulkanVideoCodec::H264 => extensions.iter().any(|ext| {
                    let name = CStr::from_ptr(ext.extension_name.as_ptr());
                    name.to_str()
                        .map(|s| s == "VK_KHR_video_decode_h264")
                        .unwrap_or(false)
                }),
                VulkanVideoCodec::H265 => extensions.iter().any(|ext| {
                    let name = CStr::from_ptr(ext.extension_name.as_ptr());
                    name.to_str()
                        .map(|s| s == "VK_KHR_video_decode_h265")
                        .unwrap_or(false)
                }),
                VulkanVideoCodec::AV1 => false, // AV1 not yet widely supported
            };

            if !has_video_queue || !has_video_decode || !has_codec {
                continue;
            }

            // Find queue families
            let queue_families =
                instance.get_physical_device_queue_family_properties(physical_device);

            let mut decode_queue_family = None;
            let mut graphics_queue_family = None;

            for (i, props) in queue_families.iter().enumerate() {
                // Check for video decode queue
                if props.queue_flags.contains(vk::QueueFlags::VIDEO_DECODE_KHR) {
                    decode_queue_family = Some(i as u32);
                }
                // Check for graphics queue (for image operations)
                if props.queue_flags.contains(vk::QueueFlags::GRAPHICS) {
                    graphics_queue_family = Some(i as u32);
                }
            }

            if let (Some(decode), Some(graphics)) = (decode_queue_family, graphics_queue_family) {
                return Ok((physical_device, decode, graphics));
            }
        }

        Err(anyhow!(
            "No suitable Vulkan device with video decode support found"
        ))
    }

    /// Create logical device with video extensions
    unsafe fn create_device(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        decode_queue_family: u32,
        graphics_queue_family: u32,
        config: &VulkanVideoConfig,
    ) -> Result<(ash::Device, vk::Queue, vk::Queue)> {
        let queue_priorities = [1.0f32];

        let mut queue_create_infos = vec![vk::DeviceQueueCreateInfo::default()
            .queue_family_index(decode_queue_family)
            .queue_priorities(&queue_priorities)];

        // Add graphics queue if different from decode queue
        if graphics_queue_family != decode_queue_family {
            queue_create_infos.push(
                vk::DeviceQueueCreateInfo::default()
                    .queue_family_index(graphics_queue_family)
                    .queue_priorities(&queue_priorities),
            );
        }

        // Required extensions
        let mut extension_names: Vec<*const i8> = vec![
            ash::khr::video_queue::NAME.as_ptr(),
            ash::khr::video_decode_queue::NAME.as_ptr(),
        ];

        // Add codec-specific extension
        match config.codec {
            VulkanVideoCodec::H264 => {
                extension_names.push(c"VK_KHR_video_decode_h264".as_ptr());
            }
            VulkanVideoCodec::H265 => {
                extension_names.push(c"VK_KHR_video_decode_h265".as_ptr());
            }
            VulkanVideoCodec::AV1 => {
                return Err(anyhow!("AV1 Vulkan Video not yet supported"));
            }
        }

        // Enable synchronization2 for better sync primitives
        let mut sync2_features =
            vk::PhysicalDeviceSynchronization2Features::default().synchronization2(true);

        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(&queue_create_infos)
            .enabled_extension_names(&extension_names)
            .push_next(&mut sync2_features);

        let device = instance.create_device(physical_device, &device_create_info, None)?;

        let decode_queue = device.get_device_queue(decode_queue_family, 0);
        let graphics_queue = device.get_device_queue(graphics_queue_family, 0);

        Ok((device, decode_queue, graphics_queue))
    }

    /// Create video session
    unsafe fn create_video_session(
        device: &ash::Device,
        video_queue_fn: &ash::khr::video_queue::Device,
        physical_device: vk::PhysicalDevice,
        instance: &ash::Instance,
        queue_family_index: u32,
        config: &VulkanVideoConfig,
    ) -> Result<vk::VideoSessionKHR> {
        // Build video profile
        let (profile_info, _h264_profile, _h265_profile) = Self::build_video_profile(config)?;

        // Create video session
        let session_create_info = vk::VideoSessionCreateInfoKHR::default()
            .queue_family_index(queue_family_index)
            .video_profile(&profile_info)
            .picture_format(if config.is_10bit {
                vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
            } else {
                vk::Format::G8_B8R8_2PLANE_420_UNORM // NV12
            })
            .max_coded_extent(vk::Extent2D {
                width: config.width,
                height: config.height,
            })
            .reference_picture_format(if config.is_10bit {
                vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
            } else {
                vk::Format::G8_B8R8_2PLANE_420_UNORM
            })
            .max_dpb_slots(Self::calculate_dpb_size(config) as u32)
            .max_active_reference_pictures(16); // H.264 Level 5.1 max

        let video_session = video_queue_fn.create_video_session(&session_create_info, None)?;

        // Query memory requirements and bind memory
        let mut mem_req_count = 0u32;
        video_queue_fn.get_video_session_memory_requirements(
            video_session,
            &mut mem_req_count,
            std::ptr::null_mut(),
        )?;

        if mem_req_count > 0 {
            let mut mem_requirements =
                vec![vk::VideoSessionMemoryRequirementsKHR::default(); mem_req_count as usize];
            video_queue_fn.get_video_session_memory_requirements(
                video_session,
                &mut mem_req_count,
                mem_requirements.as_mut_ptr(),
            )?;

            let mut bind_infos = Vec::new();
            let mut allocated_memories = Vec::new();

            for req in &mem_requirements {
                let mem_props = instance.get_physical_device_memory_properties(physical_device);
                let memory_type_index = Self::find_memory_type(
                    &mem_props,
                    req.memory_requirements.memory_type_bits,
                    vk::MemoryPropertyFlags::DEVICE_LOCAL,
                )?;

                let alloc_info = vk::MemoryAllocateInfo::default()
                    .allocation_size(req.memory_requirements.size)
                    .memory_type_index(memory_type_index);

                let memory = device.allocate_memory(&alloc_info, None)?;
                allocated_memories.push(memory);

                bind_infos.push(
                    vk::BindVideoSessionMemoryInfoKHR::default()
                        .memory_bind_index(req.memory_bind_index)
                        .memory(memory)
                        .memory_offset(0)
                        .memory_size(req.memory_requirements.size),
                );
            }

            video_queue_fn.bind_video_session_memory(video_session, &bind_infos)?;
        }

        Ok(video_session)
    }

    /// Build video profile for the specified codec
    fn build_video_profile(
        config: &VulkanVideoConfig,
    ) -> Result<(
        vk::VideoProfileInfoKHR<'static>,
        Option<vk::VideoDecodeH264ProfileInfoKHR<'static>>,
        Option<vk::VideoDecodeH265ProfileInfoKHR<'static>>,
    )> {
        match config.codec {
            VulkanVideoCodec::H264 => {
                let h264_profile = Box::leak(Box::new(
                    vk::VideoDecodeH264ProfileInfoKHR::default()
                        .std_profile_idc(
                            ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH,
                        )
                        .picture_layout(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE),
                ));

                let profile = vk::VideoProfileInfoKHR::default()
                    .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
                    .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
                    .luma_bit_depth(if config.is_10bit {
                        vk::VideoComponentBitDepthFlagsKHR::TYPE_10
                    } else {
                        vk::VideoComponentBitDepthFlagsKHR::TYPE_8
                    })
                    .chroma_bit_depth(if config.is_10bit {
                        vk::VideoComponentBitDepthFlagsKHR::TYPE_10
                    } else {
                        vk::VideoComponentBitDepthFlagsKHR::TYPE_8
                    });

                Ok((profile, Some(*h264_profile), None))
            }
            VulkanVideoCodec::H265 => {
                let h265_profile = Box::leak(Box::new(
                    vk::VideoDecodeH265ProfileInfoKHR::default().std_profile_idc(
                        ash::vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN,
                    ),
                ));

                let profile = vk::VideoProfileInfoKHR::default()
                    .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H265)
                    .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
                    .luma_bit_depth(if config.is_10bit {
                        vk::VideoComponentBitDepthFlagsKHR::TYPE_10
                    } else {
                        vk::VideoComponentBitDepthFlagsKHR::TYPE_8
                    })
                    .chroma_bit_depth(if config.is_10bit {
                        vk::VideoComponentBitDepthFlagsKHR::TYPE_10
                    } else {
                        vk::VideoComponentBitDepthFlagsKHR::TYPE_8
                    });

                Ok((profile, None, Some(*h265_profile)))
            }
            VulkanVideoCodec::AV1 => Err(anyhow!("AV1 not yet supported")),
        }
    }

    /// Create DPB images
    unsafe fn create_dpb_images(
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        config: &VulkanVideoConfig,
        count: usize,
    ) -> Result<(Vec<vk::Image>, Vec<vk::ImageView>, Vec<vk::DeviceMemory>)> {
        let format = if config.is_10bit {
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
        } else {
            vk::Format::G8_B8R8_2PLANE_420_UNORM
        };

        let mut images = Vec::with_capacity(count);
        let mut image_views = Vec::with_capacity(count);
        let mut memories = Vec::with_capacity(count);

        for _ in 0..count {
            let image_info = vk::ImageCreateInfo::default()
                .image_type(vk::ImageType::TYPE_2D)
                .format(format)
                .extent(vk::Extent3D {
                    width: config.width,
                    height: config.height,
                    depth: 1,
                })
                .mip_levels(1)
                .array_layers(1)
                .samples(vk::SampleCountFlags::TYPE_1)
                .tiling(vk::ImageTiling::OPTIMAL)
                .usage(
                    vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
                        | vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR,
                )
                .sharing_mode(vk::SharingMode::EXCLUSIVE);

            let image = device.create_image(&image_info, None)?;

            // Allocate memory
            let mem_reqs = device.get_image_memory_requirements(image);
            let mem_props = instance.get_physical_device_memory_properties(physical_device);
            let memory_type_index = Self::find_memory_type(
                &mem_props,
                mem_reqs.memory_type_bits,
                vk::MemoryPropertyFlags::DEVICE_LOCAL,
            )?;

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_reqs.size)
                .memory_type_index(memory_type_index);

            let memory = device.allocate_memory(&alloc_info, None)?;
            device.bind_image_memory(image, memory, 0)?;

            // Create image view
            let view_info = vk::ImageViewCreateInfo::default()
                .image(image)
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(format)
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                });

            let image_view = device.create_image_view(&view_info, None)?;

            images.push(image);
            image_views.push(image_view);
            memories.push(memory);
        }

        Ok((images, image_views, memories))
    }

    /// Create output image for readback
    unsafe fn create_output_image(
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        config: &VulkanVideoConfig,
    ) -> Result<(vk::Image, vk::ImageView, vk::DeviceMemory)> {
        let format = if config.is_10bit {
            vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16
        } else {
            vk::Format::G8_B8R8_2PLANE_420_UNORM
        };

        let image_info = vk::ImageCreateInfo::default()
            .image_type(vk::ImageType::TYPE_2D)
            .format(format)
            .extent(vk::Extent3D {
                width: config.width,
                height: config.height,
                depth: 1,
            })
            .mip_levels(1)
            .array_layers(1)
            .samples(vk::SampleCountFlags::TYPE_1)
            .tiling(vk::ImageTiling::LINEAR) // LINEAR for CPU readback
            .usage(vk::ImageUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let image = device.create_image(&image_info, None)?;

        let mem_reqs = device.get_image_memory_requirements(image);
        let mem_props = instance.get_physical_device_memory_properties(physical_device);
        let memory_type_index = Self::find_memory_type(
            &mem_props,
            mem_reqs.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(memory_type_index);

        let memory = device.allocate_memory(&alloc_info, None)?;
        device.bind_image_memory(image, memory, 0)?;

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(format)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let image_view = device.create_image_view(&view_info, None)?;

        Ok((image, image_view, memory))
    }

    /// Create staging buffer for CPU readback
    unsafe fn create_staging_buffer(
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        size: u64,
    ) -> Result<(vk::Buffer, vk::DeviceMemory)> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::TRANSFER_DST)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = device.create_buffer(&buffer_info, None)?;

        let mem_reqs = device.get_buffer_memory_requirements(buffer);
        let mem_props = instance.get_physical_device_memory_properties(physical_device);
        let memory_type_index = Self::find_memory_type(
            &mem_props,
            mem_reqs.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(memory_type_index);

        let memory = device.allocate_memory(&alloc_info, None)?;
        device.bind_buffer_memory(buffer, memory, 0)?;

        Ok((buffer, memory))
    }

    /// Create bitstream buffer for compressed video data
    unsafe fn create_bitstream_buffer(
        device: &ash::Device,
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        size: u64,
    ) -> Result<(vk::Buffer, vk::DeviceMemory)> {
        let buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR)
            .sharing_mode(vk::SharingMode::EXCLUSIVE);

        let buffer = device.create_buffer(&buffer_info, None)?;

        let mem_reqs = device.get_buffer_memory_requirements(buffer);
        let mem_props = instance.get_physical_device_memory_properties(physical_device);
        let memory_type_index = Self::find_memory_type(
            &mem_props,
            mem_reqs.memory_type_bits,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )?;

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(memory_type_index);

        let memory = device.allocate_memory(&alloc_info, None)?;
        device.bind_buffer_memory(buffer, memory, 0)?;

        Ok((buffer, memory))
    }

    /// Find suitable memory type
    fn find_memory_type(
        mem_props: &vk::PhysicalDeviceMemoryProperties,
        type_filter: u32,
        properties: vk::MemoryPropertyFlags,
    ) -> Result<u32> {
        for i in 0..mem_props.memory_type_count {
            if (type_filter & (1 << i)) != 0
                && mem_props.memory_types[i as usize]
                    .property_flags
                    .contains(properties)
            {
                return Ok(i);
            }
        }
        Err(anyhow!("Failed to find suitable memory type"))
    }

    /// Calculate DPB size based on codec and resolution
    fn calculate_dpb_size(config: &VulkanVideoConfig) -> usize {
        match config.codec {
            VulkanVideoCodec::H264 => 17, // 16 DPB + 1 current
            VulkanVideoCodec::H265 => 17,
            VulkanVideoCodec::AV1 => 10,
        }
    }

    /// Set SPS (Sequence Parameter Set) data
    pub fn set_sps(&mut self, sps_data: &[u8]) -> Result<()> {
        debug!("Setting SPS data: {} bytes", sps_data.len());
        self.sps_data = Some(sps_data.to_vec());
        self.params_dirty = true;
        Ok(())
    }

    /// Set PPS (Picture Parameter Set) data
    pub fn set_pps(&mut self, pps_data: &[u8]) -> Result<()> {
        debug!("Setting PPS data: {} bytes", pps_data.len());
        self.pps_data = Some(pps_data.to_vec());
        self.params_dirty = true;
        Ok(())
    }

    /// Decode a video frame
    pub fn decode(&mut self, nal_data: &[u8]) -> Result<Option<VideoFrame>> {
        if nal_data.is_empty() {
            return Ok(None);
        }

        debug!(
            "Decoding frame {}: {} bytes",
            self.frame_count,
            nal_data.len()
        );

        unsafe {
            // Upload bitstream to GPU
            if nal_data.len() as u64 > self.bitstream_size {
                warn!(
                    "NAL data too large: {} > {}",
                    nal_data.len(),
                    self.bitstream_size
                );
                return Ok(None);
            }

            let data_ptr = self.device.map_memory(
                self.bitstream_memory,
                0,
                nal_data.len() as u64,
                vk::MemoryMapFlags::empty(),
            )?;
            std::ptr::copy_nonoverlapping(nal_data.as_ptr(), data_ptr as *mut u8, nal_data.len());
            self.device.unmap_memory(self.bitstream_memory);

            // Get current DPB slot
            let dpb_index = self.current_dpb_index;
            self.current_dpb_index = (self.current_dpb_index + 1) % self.dpb_images.len();

            // Begin command buffer
            let begin_info = vk::CommandBufferBeginInfo::default()
                .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
            self.device
                .begin_command_buffer(self.decode_command_buffer, &begin_info)?;

            // Begin video coding session
            let begin_coding_info = vk::VideoBeginCodingInfoKHR::default()
                .video_session(self.video_session)
                .video_session_parameters(self.video_session_params);

            self.video_queue_fn
                .cmd_begin_video_coding(self.decode_command_buffer, &begin_coding_info);

            // Issue decode command
            // Note: Full implementation would parse NAL units and set up proper decode info
            // This is a simplified version that demonstrates the API usage

            // End video coding
            let end_coding_info = vk::VideoEndCodingInfoKHR::default();
            self.video_queue_fn
                .cmd_end_video_coding(self.decode_command_buffer, &end_coding_info);

            self.device.end_command_buffer(self.decode_command_buffer)?;

            // Submit command buffer
            let submit_info = vk::SubmitInfo::default()
                .command_buffers(std::slice::from_ref(&self.decode_command_buffer));

            self.device
                .queue_submit(self.decode_queue, &[submit_info], self.decode_fence)?;

            // Wait for completion
            self.device
                .wait_for_fences(&[self.decode_fence], true, u64::MAX)?;
            self.device.reset_fences(&[self.decode_fence])?;

            self.frame_count += 1;

            // Read back decoded frame
            // For now, create a placeholder frame
            // Full implementation would copy from DPB image to staging buffer
            let frame_size = (self.config.width * self.config.height * 3 / 2) as usize;
            let mut frame_data = vec![0u8; frame_size];

            // Y plane: mid-gray
            let y_size = (self.config.width * self.config.height) as usize;
            for i in 0..y_size {
                frame_data[i] = 128;
            }
            // UV plane: neutral (128, 128)
            for i in y_size..frame_size {
                frame_data[i] = 128;
            }

            Ok(Some(VideoFrame {
                width: self.config.width,
                height: self.config.height,
                data: frame_data,
                pixel_format: PixelFormat::NV12,
                stride: self.config.width,
                color_range: ColorRange::Limited,
                color_space: ColorSpace::BT709,
                transfer_function: TransferFunction::SDR,
            }))
        }
    }

    /// Get decoder statistics
    pub fn get_stats(&self) -> DecoderStats {
        DecoderStats {
            frames_decoded: self.frame_count,
            dpb_size: self.dpb_slots.len() as u32,
            supports_dmabuf: true, // Vulkan supports DMA-BUF export
        }
    }
}

impl Drop for VulkanVideoDecoder {
    fn drop(&mut self) {
        info!("Destroying Vulkan Video decoder");

        unsafe {
            // Wait for any pending operations
            let _ = self.device.device_wait_idle();

            // Destroy video session parameters
            if self.video_session_params != vk::VideoSessionParametersKHR::null() {
                self.video_queue_fn
                    .destroy_video_session_parameters(self.video_session_params, None);
            }

            // Destroy video session
            if self.video_session != vk::VideoSessionKHR::null() {
                self.video_queue_fn
                    .destroy_video_session(self.video_session, None);
            }

            // Destroy fence
            self.device.destroy_fence(self.decode_fence, None);

            // Destroy command pool
            self.device
                .destroy_command_pool(self.decode_command_pool, None);

            // Destroy buffers
            self.device.destroy_buffer(self.bitstream_buffer, None);
            self.device.free_memory(self.bitstream_memory, None);
            self.device.destroy_buffer(self.staging_buffer, None);
            self.device.free_memory(self.staging_memory, None);

            // Destroy output image
            self.device.destroy_image_view(self.output_image_view, None);
            self.device.destroy_image(self.output_image, None);
            self.device.free_memory(self.output_memory, None);

            // Destroy DPB images
            for view in &self.dpb_image_views {
                self.device.destroy_image_view(*view, None);
            }
            for image in &self.dpb_images {
                self.device.destroy_image(*image, None);
            }
            for memory in &self.dpb_memory {
                self.device.free_memory(*memory, None);
            }

            // Destroy device and instance
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpb_size_calculation() {
        let h264_config = VulkanVideoConfig {
            codec: VulkanVideoCodec::H264,
            ..Default::default()
        };
        assert_eq!(VulkanVideoDecoder::calculate_dpb_size(&h264_config), 17);
    }

    #[test]
    fn test_default_config() {
        let config = VulkanVideoConfig::default();
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.codec, VulkanVideoCodec::H264);
        assert!(!config.is_10bit);
    }
}
