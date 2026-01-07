//! Vulkan Video Decoder for Linux
//!
//! Hardware-accelerated video decoding using Vulkan Video extensions.
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
//!
//! References:
//! - https://www.khronos.org/blog/an-introduction-to-vulkan-video
//! - https://wickedengine.net/2023/05/vulkan-video-decoding/
//! - https://docs.vulkan.org/spec/latest/chapters/videocoding.html

use anyhow::{anyhow, Result};
use ash::vk;
use log::{debug, error, info, trace, warn};
use std::ffi::{CStr, CString};
use std::ptr;

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
            num_decode_surfaces: 17, // H.264 max: 16 refs + 1 current
        }
    }
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
        let entry = match ash::Entry::load() {
            Ok(e) => e,
            Err(e) => {
                debug!("Failed to load Vulkan: {}", e);
                return Ok(false);
            }
        };

        let app_info = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_3);
        let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);

        let instance = match entry.create_instance(&create_info, None) {
            Ok(i) => i,
            Err(e) => {
                debug!("Failed to create Vulkan instance: {}", e);
                return Ok(false);
            }
        };

        let physical_devices = instance.enumerate_physical_devices()?;
        if physical_devices.is_empty() {
            instance.destroy_instance(None);
            return Ok(false);
        }

        for physical_device in &physical_devices {
            let device_extensions =
                instance.enumerate_device_extension_properties(*physical_device)?;

            let has_video_queue = device_extensions.iter().any(|ext| {
                CStr::from_ptr(ext.extension_name.as_ptr())
                    .to_str()
                    .map(|s| s == "VK_KHR_video_queue")
                    .unwrap_or(false)
            });

            let has_video_decode = device_extensions.iter().any(|ext| {
                CStr::from_ptr(ext.extension_name.as_ptr())
                    .to_str()
                    .map(|s| s == "VK_KHR_video_decode_queue")
                    .unwrap_or(false)
            });

            let has_h264 = device_extensions.iter().any(|ext| {
                CStr::from_ptr(ext.extension_name.as_ptr())
                    .to_str()
                    .map(|s| s == "VK_KHR_video_decode_h264")
                    .unwrap_or(false)
            });

            let has_h265 = device_extensions.iter().any(|ext| {
                CStr::from_ptr(ext.extension_name.as_ptr())
                    .to_str()
                    .map(|s| s == "VK_KHR_video_decode_h265")
                    .unwrap_or(false)
            });

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
        let app_info = vk::ApplicationInfo::default().api_version(vk::API_VERSION_1_3);
        let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);

        let instance = entry.create_instance(&create_info, None)?;
        let physical_devices = instance.enumerate_physical_devices()?;

        for physical_device in &physical_devices {
            let device_extensions =
                instance.enumerate_device_extension_properties(*physical_device)?;

            let has_h264 = device_extensions.iter().any(|ext| {
                CStr::from_ptr(ext.extension_name.as_ptr())
                    .to_str()
                    .map(|s| s == "VK_KHR_video_decode_h264")
                    .unwrap_or(false)
            });

            let has_h265 = device_extensions.iter().any(|ext| {
                CStr::from_ptr(ext.extension_name.as_ptr())
                    .to_str()
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

/// H.264 SPS parsed data
#[derive(Debug, Clone, Default)]
struct H264Sps {
    profile_idc: u8,
    level_idc: u8,
    seq_parameter_set_id: u8,
    chroma_format_idc: u8,
    bit_depth_luma_minus8: u8,
    bit_depth_chroma_minus8: u8,
    log2_max_frame_num_minus4: u8,
    pic_order_cnt_type: u8,
    log2_max_pic_order_cnt_lsb_minus4: u8,
    max_num_ref_frames: u8,
    pic_width_in_mbs_minus1: u16,
    pic_height_in_map_units_minus1: u16,
    frame_mbs_only_flag: bool,
    direct_8x8_inference_flag: bool,
    frame_cropping_flag: bool,
    frame_crop_left_offset: u16,
    frame_crop_right_offset: u16,
    frame_crop_top_offset: u16,
    frame_crop_bottom_offset: u16,
    raw_data: Vec<u8>,
}

/// H.264 PPS parsed data
#[derive(Debug, Clone, Default)]
struct H264Pps {
    pic_parameter_set_id: u8,
    seq_parameter_set_id: u8,
    entropy_coding_mode_flag: bool,
    bottom_field_pic_order_in_frame_present_flag: bool,
    num_slice_groups_minus1: u8,
    num_ref_idx_l0_default_active_minus1: u8,
    num_ref_idx_l1_default_active_minus1: u8,
    weighted_pred_flag: bool,
    weighted_bipred_idc: u8,
    pic_init_qp_minus26: i8,
    pic_init_qs_minus26: i8,
    chroma_qp_index_offset: i8,
    deblocking_filter_control_present_flag: bool,
    constrained_intra_pred_flag: bool,
    redundant_pic_cnt_present_flag: bool,
    transform_8x8_mode_flag: bool,
    second_chroma_qp_index_offset: i8,
    raw_data: Vec<u8>,
}

/// DPB slot state
#[derive(Debug, Clone, Default)]
struct DpbSlotState {
    /// Is this slot currently in use?
    in_use: bool,
    /// Frame number from slice header
    frame_num: u16,
    /// Picture Order Count
    pic_order_cnt: i32,
    /// Is this a reference picture?
    is_reference: bool,
    /// Is this a long-term reference?
    is_long_term: bool,
}

/// DPB image resources
struct DpbImage {
    image: vk::Image,
    memory: vk::DeviceMemory,
    view: vk::ImageView,
}

/// Vulkan Video Decoder
pub struct VulkanVideoDecoder {
    // Vulkan core
    _entry: ash::Entry,
    instance: ash::Instance,
    _physical_device: vk::PhysicalDevice,
    device: ash::Device,

    // Queues
    decode_queue: vk::Queue,
    decode_queue_family: u32,

    // Video session
    video_session: vk::VideoSessionKHR,
    video_session_params: vk::VideoSessionParametersKHR,

    // Extension functions
    video_queue_fn: vk::khr::video_queue::Device,
    video_decode_queue_fn: vk::khr::video_decode_queue::Device,

    // Command buffers
    command_pool: vk::CommandPool,
    command_buffer: vk::CommandBuffer,
    fence: vk::Fence,

    // Bitstream buffer (host visible for data upload)
    bitstream_buffer: vk::Buffer,
    bitstream_memory: vk::DeviceMemory,
    bitstream_mapped: *mut u8,
    bitstream_size: vk::DeviceSize,

    // DPB images
    dpb_images: Vec<DpbImage>,
    dpb_slots: Vec<DpbSlotState>,

    // Staging buffer for readback
    staging_buffer: vk::Buffer,
    staging_memory: vk::DeviceMemory,
    staging_mapped: *mut u8,

    // Query pool
    query_pool: vk::QueryPool,

    // Session state
    session_initialized: bool,
    config: VulkanVideoConfig,
    aligned_width: u32,
    aligned_height: u32,

    // Bitstream alignment requirements from VkVideoCapabilitiesKHR
    min_bitstream_buffer_offset_alignment: vk::DeviceSize,
    min_bitstream_buffer_size_alignment: vk::DeviceSize,

    // H.264 parameters
    sps: Option<H264Sps>,
    pps: Option<H264Pps>,
    params_updated: bool,

    // Frame tracking
    frame_count: u64,
    current_dpb_index: usize,
    prev_frame_num: u16,
    prev_pic_order_cnt_msb: i32,
    prev_pic_order_cnt_lsb: i32,

    // Memory bindings (must be kept alive)
    _session_memory: Vec<vk::DeviceMemory>,
}

unsafe impl Send for VulkanVideoDecoder {}
unsafe impl Sync for VulkanVideoDecoder {}

impl VulkanVideoDecoder {
    /// Create a new Vulkan Video decoder
    pub fn new(config: VulkanVideoConfig) -> Result<Self> {
        info!(
            "Creating Vulkan Video decoder: {:?} {}x{} 10bit={}",
            config.codec, config.width, config.height, config.is_10bit
        );

        unsafe { Self::create_decoder(config) }
    }

    unsafe fn create_decoder(config: VulkanVideoConfig) -> Result<Self> {
        // Load Vulkan
        let entry = ash::Entry::load().map_err(|e| anyhow!("Failed to load Vulkan: {}", e))?;

        // Create instance
        let app_name = CString::new("OpenNow Vulkan Video").unwrap();
        let app_info = vk::ApplicationInfo::default()
            .application_name(&app_name)
            .application_version(vk::make_api_version(0, 1, 0, 0))
            .api_version(vk::API_VERSION_1_3);

        let create_info = vk::InstanceCreateInfo::default().application_info(&app_info);
        let instance = entry.create_instance(&create_info, None)?;
        info!("Vulkan instance created");

        // Find physical device
        let (physical_device, decode_queue_family) = Self::find_device(&instance, &config)?;

        let props = instance.get_physical_device_properties(physical_device);
        let device_name = CStr::from_ptr(props.device_name.as_ptr())
            .to_str()
            .unwrap_or("Unknown");
        info!("Selected Vulkan device: {}", device_name);

        // Create device with video extensions
        let device = Self::create_device(&instance, physical_device, decode_queue_family, &config)?;
        let decode_queue = device.get_device_queue(decode_queue_family, 0);

        // Load extension functions
        let video_queue_fn = vk::khr::video_queue::Device::new(&instance, &device);
        let video_decode_queue_fn = vk::khr::video_decode_queue::Device::new(&instance, &device);

        // Create H.264 profile
        let h264_profile_info = vk::VideoDecodeH264ProfileInfoKHR::default()
            .std_profile_idc(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH)
            .picture_layout(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE);

        let video_profile = vk::VideoProfileInfoKHR::default()
            .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
            .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
            .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
            .push_next(&h264_profile_info as *const _ as *mut _);

        // Query video capabilities
        let mut h264_caps = vk::VideoDecodeH264CapabilitiesKHR::default();
        let mut decode_caps = vk::VideoDecodeCapabilitiesKHR::default().push_next(&mut h264_caps);
        let mut video_caps = vk::VideoCapabilitiesKHR::default().push_next(&mut decode_caps);

        video_queue_fn.get_physical_device_video_capabilities(
            physical_device,
            &video_profile,
            &mut video_caps,
        )?;

        info!(
            "Video capabilities: max={}x{}, dpb_slots={}, active_refs={}",
            video_caps.max_coded_extent.width,
            video_caps.max_coded_extent.height,
            video_caps.max_dpb_slots,
            video_caps.max_active_reference_pictures
        );

        // Store alignment requirements for bitstream buffers
        let min_bitstream_buffer_offset_alignment =
            video_caps.min_bitstream_buffer_offset_alignment;
        let min_bitstream_buffer_size_alignment = video_caps.min_bitstream_buffer_size_alignment;
        info!(
            "Bitstream alignment: offset={}, size={}",
            min_bitstream_buffer_offset_alignment, min_bitstream_buffer_size_alignment
        );

        // Check DPB capability flags
        // SEPARATE_REFERENCE_IMAGES means each DPB slot needs its own VkImage (which we do)
        // If not set, we could use a single image with array layers (optimization for future)
        let separate_ref_images = video_caps
            .flags
            .contains(vk::VideoCapabilityFlagsKHR::SEPARATE_REFERENCE_IMAGES);
        info!(
            "Video capability flags: separate_ref_images={}",
            separate_ref_images
        );

        // Calculate aligned dimensions
        let align_w = video_caps.picture_access_granularity.width.max(1);
        let align_h = video_caps.picture_access_granularity.height.max(1);
        let aligned_width = (config.width + align_w - 1) / align_w * align_w;
        let aligned_height = (config.height + align_h - 1) / align_h * align_h;
        info!("Aligned dimensions: {}x{}", aligned_width, aligned_height);

        // Create video session
        let profile_list =
            vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(&video_profile));

        let session_create_info = vk::VideoSessionCreateInfoKHR::default()
            .queue_family_index(decode_queue_family)
            .video_profile(&video_profile)
            .picture_format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .max_coded_extent(vk::Extent2D {
                width: aligned_width,
                height: aligned_height,
            })
            .reference_picture_format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .max_dpb_slots(17) // H.264 max: 16 refs + 1 current
            .max_active_reference_pictures(16)
            .std_header_version(&video_caps.std_header_version);

        let video_session = video_queue_fn.create_video_session(&session_create_info, None)?;
        info!("Video session created");

        // Bind memory to session
        let session_memory = Self::bind_session_memory(
            &instance,
            &device,
            physical_device,
            &video_queue_fn,
            video_session,
        )?;

        // Create session parameters (empty initially, updated when SPS/PPS received)
        let h264_params_add = vk::VideoDecodeH264SessionParametersAddInfoKHR::default();
        let mut h264_params_create = vk::VideoDecodeH264SessionParametersCreateInfoKHR::default()
            .max_std_sps_count(32)
            .max_std_pps_count(256)
            .parameters_add_info(&h264_params_add);

        let params_create_info = vk::VideoSessionParametersCreateInfoKHR::default()
            .video_session(video_session)
            .push_next(&mut h264_params_create);

        let video_session_params =
            video_queue_fn.create_video_session_parameters(&params_create_info, None)?;
        info!("Video session parameters created");

        // Create command pool and buffer
        let pool_info = vk::CommandPoolCreateInfo::default()
            .queue_family_index(decode_queue_family)
            .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER);
        let command_pool = device.create_command_pool(&pool_info, None)?;

        let alloc_info = vk::CommandBufferAllocateInfo::default()
            .command_pool(command_pool)
            .level(vk::CommandBufferLevel::PRIMARY)
            .command_buffer_count(1);
        let command_buffer = device.allocate_command_buffers(&alloc_info)?[0];

        let fence = device.create_fence(&vk::FenceCreateInfo::default(), None)?;

        // Create bitstream buffer (4MB, host visible)
        // Note: Wicked Engine says host-mapped buffers are required
        let bitstream_size: vk::DeviceSize = 4 * 1024 * 1024;
        let (bitstream_buffer, bitstream_memory, bitstream_mapped) = Self::create_host_buffer(
            &instance,
            &device,
            physical_device,
            bitstream_size,
            vk::BufferUsageFlags::VIDEO_DECODE_SRC_KHR,
            &profile_list,
        )?;

        // Create DPB images (17 slots)
        let dpb_count = 17usize;
        let mut dpb_images = Vec::with_capacity(dpb_count);
        for i in 0..dpb_count {
            let dpb = Self::create_dpb_image(
                &instance,
                &device,
                physical_device,
                aligned_width,
                aligned_height,
                &profile_list,
            )?;
            dpb_images.push(dpb);
            trace!("Created DPB image {}", i);
        }

        let dpb_slots = vec![DpbSlotState::default(); dpb_count];

        // Create staging buffer for CPU readback
        let staging_size = (aligned_width * aligned_height * 3 / 2) as vk::DeviceSize;
        let (staging_buffer, staging_memory, staging_mapped) = Self::create_host_buffer(
            &instance,
            &device,
            physical_device,
            staging_size,
            vk::BufferUsageFlags::TRANSFER_DST,
            &profile_list,
        )?;

        // Create query pool
        let mut profile_list_query =
            vk::VideoProfileListInfoKHR::default().profiles(std::slice::from_ref(&video_profile));
        let query_pool_info = vk::QueryPoolCreateInfo::default()
            .query_type(vk::QueryType::RESULT_STATUS_ONLY_KHR)
            .query_count(1)
            .push_next(&mut profile_list_query);
        let query_pool = device.create_query_pool(&query_pool_info, None)?;

        info!("Vulkan Video decoder initialized successfully");

        Ok(Self {
            _entry: entry,
            instance,
            _physical_device: physical_device,
            device,
            decode_queue,
            decode_queue_family,
            video_session,
            video_session_params,
            video_queue_fn,
            video_decode_queue_fn,
            command_pool,
            command_buffer,
            fence,
            bitstream_buffer,
            bitstream_memory,
            bitstream_mapped,
            bitstream_size,
            dpb_images,
            dpb_slots,
            staging_buffer,
            staging_memory,
            staging_mapped,
            query_pool,
            session_initialized: false,
            config,
            aligned_width,
            aligned_height,
            min_bitstream_buffer_offset_alignment,
            min_bitstream_buffer_size_alignment,
            sps: None,
            pps: None,
            params_updated: false,
            frame_count: 0,
            current_dpb_index: 0,
            prev_frame_num: 0,
            prev_pic_order_cnt_msb: 0,
            prev_pic_order_cnt_lsb: 0,
            _session_memory: session_memory,
        })
    }

    unsafe fn find_device(
        instance: &ash::Instance,
        config: &VulkanVideoConfig,
    ) -> Result<(vk::PhysicalDevice, u32)> {
        let physical_devices = instance.enumerate_physical_devices()?;

        for physical_device in physical_devices {
            let extensions = instance.enumerate_device_extension_properties(physical_device)?;

            let required = [
                "VK_KHR_video_queue",
                "VK_KHR_video_decode_queue",
                match config.codec {
                    VulkanVideoCodec::H264 => "VK_KHR_video_decode_h264",
                    VulkanVideoCodec::H265 => "VK_KHR_video_decode_h265",
                    VulkanVideoCodec::AV1 => return Err(anyhow!("AV1 not yet supported")),
                },
            ];

            let has_all = required.iter().all(|req| {
                extensions.iter().any(|ext| {
                    CStr::from_ptr(ext.extension_name.as_ptr())
                        .to_str()
                        .map(|s| s == *req)
                        .unwrap_or(false)
                })
            });

            if !has_all {
                continue;
            }

            // Find video decode queue family
            let queue_families =
                instance.get_physical_device_queue_family_properties(physical_device);

            for (i, props) in queue_families.iter().enumerate() {
                if props.queue_flags.contains(vk::QueueFlags::VIDEO_DECODE_KHR) {
                    return Ok((physical_device, i as u32));
                }
            }
        }

        Err(anyhow!("No Vulkan device with video decode support found"))
    }

    unsafe fn create_device(
        instance: &ash::Instance,
        physical_device: vk::PhysicalDevice,
        queue_family: u32,
        config: &VulkanVideoConfig,
    ) -> Result<ash::Device> {
        let queue_priorities = [1.0f32];
        let queue_create_info = vk::DeviceQueueCreateInfo::default()
            .queue_family_index(queue_family)
            .queue_priorities(&queue_priorities);

        let extension_names: Vec<CString> = vec![
            CString::new("VK_KHR_video_queue").unwrap(),
            CString::new("VK_KHR_video_decode_queue").unwrap(),
            CString::new(match config.codec {
                VulkanVideoCodec::H264 => "VK_KHR_video_decode_h264",
                VulkanVideoCodec::H265 => "VK_KHR_video_decode_h265",
                VulkanVideoCodec::AV1 => return Err(anyhow!("AV1 not supported")),
            })
            .unwrap(),
            CString::new("VK_KHR_synchronization2").unwrap(),
        ];

        let extension_ptrs: Vec<*const i8> = extension_names.iter().map(|s| s.as_ptr()).collect();

        let mut sync2 =
            vk::PhysicalDeviceSynchronization2Features::default().synchronization2(true);

        let device_create_info = vk::DeviceCreateInfo::default()
            .queue_create_infos(std::slice::from_ref(&queue_create_info))
            .enabled_extension_names(&extension_ptrs)
            .push_next(&mut sync2);

        let device = instance.create_device(physical_device, &device_create_info, None)?;
        Ok(device)
    }

    unsafe fn bind_session_memory(
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        video_queue_fn: &vk::khr::video_queue::Device,
        video_session: vk::VideoSessionKHR,
    ) -> Result<Vec<vk::DeviceMemory>> {
        let mut count = 0u32;
        video_queue_fn.get_video_session_memory_requirements(
            video_session,
            &mut count,
            ptr::null_mut(),
        )?;

        let mut reqs = vec![vk::VideoSessionMemoryRequirementsKHR::default(); count as usize];
        video_queue_fn.get_video_session_memory_requirements(
            video_session,
            &mut count,
            reqs.as_mut_ptr(),
        )?;

        info!("Video session needs {} memory bindings", count);

        let mem_props = instance.get_physical_device_memory_properties(physical_device);
        let mut allocations = Vec::with_capacity(count as usize);
        let mut bind_infos = Vec::with_capacity(count as usize);

        for req in &reqs {
            let mem_req = &req.memory_requirements;

            // Find device local memory type
            let mem_type = (0..mem_props.memory_type_count)
                .find(|i| {
                    let type_bits = 1 << i;
                    let is_supported = (mem_req.memory_type_bits & type_bits) != 0;
                    let props = mem_props.memory_types[*i as usize].property_flags;
                    is_supported && props.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
                })
                .ok_or_else(|| anyhow!("No suitable memory type for video session"))?;

            let alloc_info = vk::MemoryAllocateInfo::default()
                .allocation_size(mem_req.size)
                .memory_type_index(mem_type);

            let memory = device.allocate_memory(&alloc_info, None)?;
            allocations.push(memory);

            bind_infos.push(
                vk::BindVideoSessionMemoryInfoKHR::default()
                    .memory_bind_index(req.memory_bind_index)
                    .memory(memory)
                    .memory_offset(0)
                    .memory_size(mem_req.size),
            );
        }

        video_queue_fn.bind_video_session_memory(video_session, &bind_infos)?;
        info!("Video session memory bound");

        Ok(allocations)
    }

    unsafe fn create_host_buffer(
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        size: vk::DeviceSize,
        usage: vk::BufferUsageFlags,
        profile_list: &vk::VideoProfileListInfoKHR,
    ) -> Result<(vk::Buffer, vk::DeviceMemory, *mut u8)> {
        let mut buffer_info = vk::BufferCreateInfo::default()
            .size(size)
            .usage(usage)
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .push_next(profile_list as *const _ as *mut _);

        let buffer = device.create_buffer(&buffer_info, None)?;
        let mem_reqs = device.get_buffer_memory_requirements(buffer);

        let mem_props = instance.get_physical_device_memory_properties(physical_device);
        let mem_type = (0..mem_props.memory_type_count)
            .find(|i| {
                let type_bits = 1 << i;
                let is_supported = (mem_reqs.memory_type_bits & type_bits) != 0;
                let props = mem_props.memory_types[*i as usize].property_flags;
                is_supported
                    && props.contains(
                        vk::MemoryPropertyFlags::HOST_VISIBLE
                            | vk::MemoryPropertyFlags::HOST_COHERENT,
                    )
            })
            .ok_or_else(|| anyhow!("No host visible memory type"))?;

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(mem_type);

        let memory = device.allocate_memory(&alloc_info, None)?;
        device.bind_buffer_memory(buffer, memory, 0)?;

        let mapped = device.map_memory(memory, 0, size, vk::MemoryMapFlags::empty())? as *mut u8;

        Ok((buffer, memory, mapped))
    }

    unsafe fn create_dpb_image(
        instance: &ash::Instance,
        device: &ash::Device,
        physical_device: vk::PhysicalDevice,
        width: u32,
        height: u32,
        profile_list: &vk::VideoProfileListInfoKHR,
    ) -> Result<DpbImage> {
        // DPB images need both DST (decode target) and DPB (reference) usage
        let mut image_info = vk::ImageCreateInfo::default()
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
            .tiling(vk::ImageTiling::OPTIMAL)
            .usage(
                vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
                    | vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
                    | vk::ImageUsageFlags::VIDEO_DECODE_SRC_KHR
                    | vk::ImageUsageFlags::TRANSFER_SRC,
            )
            .sharing_mode(vk::SharingMode::EXCLUSIVE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .push_next(profile_list as *const _ as *mut _);

        let image = device.create_image(&image_info, None)?;
        let mem_reqs = device.get_image_memory_requirements(image);

        let mem_props = instance.get_physical_device_memory_properties(physical_device);
        let mem_type = (0..mem_props.memory_type_count)
            .find(|i| {
                let type_bits = 1 << i;
                let is_supported = (mem_reqs.memory_type_bits & type_bits) != 0;
                let props = mem_props.memory_types[*i as usize].property_flags;
                is_supported && props.contains(vk::MemoryPropertyFlags::DEVICE_LOCAL)
            })
            .ok_or_else(|| anyhow!("No device local memory for DPB"))?;

        let alloc_info = vk::MemoryAllocateInfo::default()
            .allocation_size(mem_reqs.size)
            .memory_type_index(mem_type);

        let memory = device.allocate_memory(&alloc_info, None)?;
        device.bind_image_memory(image, memory, 0)?;

        let view_info = vk::ImageViewCreateInfo::default()
            .image(image)
            .view_type(vk::ImageViewType::TYPE_2D)
            .format(vk::Format::G8_B8R8_2PLANE_420_UNORM)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let view = device.create_image_view(&view_info, None)?;

        Ok(DpbImage {
            image,
            memory,
            view,
        })
    }

    /// Decode a video frame
    pub fn decode(&mut self, nal_data: &[u8]) -> Result<Option<VideoFrame>> {
        if nal_data.is_empty() {
            return Ok(None);
        }

        // Parse NAL units
        let nal_units = self.parse_nal_units(nal_data)?;
        if nal_units.is_empty() {
            return Ok(None);
        }

        // Check for SPS/PPS
        if self.sps.is_none() || self.pps.is_none() {
            debug!("Waiting for SPS/PPS");
            return Ok(None);
        }

        // Update session parameters if we have new SPS/PPS
        if !self.params_updated {
            unsafe {
                self.update_session_parameters()?;
            }
            self.params_updated = true;
        }

        // Find slice NAL (IDR=5 or non-IDR=1)
        let slice_info = nal_units
            .iter()
            .find(|(nal_type, _, _, _)| *nal_type == 1 || *nal_type == 5);

        if slice_info.is_none() {
            debug!("No slice NAL found");
            return Ok(None);
        }

        let (nal_type, nal_ref_idc, _, _) = slice_info.unwrap();
        let is_idr = *nal_type == 5;
        // nal_ref_idc > 0 means this frame can be used as a reference
        let is_reference = *nal_ref_idc > 0;

        self.frame_count += 1;

        // Decode the frame
        unsafe { self.decode_frame(nal_data, is_idr, is_reference) }
    }

    /// Parse NAL units and extract SPS/PPS
    /// Returns Vec of (nal_type, nal_ref_idc, nal_start, nal_end)
    fn parse_nal_units(&mut self, data: &[u8]) -> Result<Vec<(u8, u8, usize, usize)>> {
        let mut nal_units = Vec::new();
        let mut i = 0;

        while i < data.len() {
            // Find start code
            if i + 3 < data.len() && data[i] == 0 && data[i + 1] == 0 {
                let (start_code_len, nal_start) = if data[i + 2] == 1 {
                    (3, i + 3)
                } else if i + 4 < data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                    (4, i + 4)
                } else {
                    i += 1;
                    continue;
                };

                if nal_start >= data.len() {
                    break;
                }

                // Find end of NAL
                let mut nal_end = data.len();
                for j in nal_start..data.len().saturating_sub(2) {
                    if data[j] == 0
                        && data[j + 1] == 0
                        && (data[j + 2] == 1
                            || (j + 3 < data.len() && data[j + 2] == 0 && data[j + 3] == 1))
                    {
                        nal_end = j;
                        break;
                    }
                }

                let nal_header = data[nal_start];
                let nal_type = nal_header & 0x1F;
                let nal_ref_idc = (nal_header >> 5) & 0x03;
                nal_units.push((nal_type, nal_ref_idc, nal_start, nal_end));

                match nal_type {
                    7 => {
                        // SPS
                        if let Ok(sps) = self.parse_sps(&data[nal_start..nal_end]) {
                            let w = (sps.pic_width_in_mbs_minus1 as u32 + 1) * 16;
                            let h = (sps.pic_height_in_map_units_minus1 as u32 + 1) * 16;
                            debug!("SPS: {}x{}, profile={}", w, h, sps.profile_idc);
                            self.sps = Some(sps);
                            self.params_updated = false;
                        }
                    }
                    8 => {
                        // PPS
                        if let Ok(pps) = self.parse_pps(&data[nal_start..nal_end]) {
                            debug!("PPS: id={}", pps.pic_parameter_set_id);
                            self.pps = Some(pps);
                            self.params_updated = false;
                        }
                    }
                    _ => {}
                }

                i = nal_end;
            } else {
                i += 1;
            }
        }

        Ok(nal_units)
    }

    fn parse_sps(&self, data: &[u8]) -> Result<H264Sps> {
        if data.len() < 4 {
            return Err(anyhow!("SPS too short"));
        }

        let mut sps = H264Sps::default();
        sps.raw_data = data.to_vec();

        let mut reader = BitReader::new(&data[1..]);

        sps.profile_idc = reader.read_bits(8)? as u8;
        let _constraint_flags = reader.read_bits(8)?;
        sps.level_idc = reader.read_bits(8)? as u8;
        sps.seq_parameter_set_id = reader.read_ue()? as u8;

        // High profile extensions
        if [100, 110, 122, 244, 44, 83, 86, 118, 128].contains(&sps.profile_idc) {
            sps.chroma_format_idc = reader.read_ue()? as u8;
            if sps.chroma_format_idc == 3 {
                let _ = reader.read_bits(1)?; // separate_colour_plane_flag
            }
            sps.bit_depth_luma_minus8 = reader.read_ue()? as u8;
            sps.bit_depth_chroma_minus8 = reader.read_ue()? as u8;
            let _ = reader.read_bits(1)?; // qpprime_y_zero_transform_bypass_flag
            let scaling_matrix_present = reader.read_bits(1)?;
            if scaling_matrix_present == 1 {
                let count = if sps.chroma_format_idc != 3 { 8 } else { 12 };
                for _ in 0..count {
                    if reader.read_bits(1)? == 1 {
                        let size = if count < 6 { 16 } else { 64 };
                        for _ in 0..size {
                            let _ = reader.read_se();
                        }
                    }
                }
            }
        } else {
            sps.chroma_format_idc = 1;
        }

        sps.log2_max_frame_num_minus4 = reader.read_ue()? as u8;
        sps.pic_order_cnt_type = reader.read_ue()? as u8;

        if sps.pic_order_cnt_type == 0 {
            sps.log2_max_pic_order_cnt_lsb_minus4 = reader.read_ue()? as u8;
        } else if sps.pic_order_cnt_type == 1 {
            let _ = reader.read_bits(1)?;
            let _ = reader.read_se()?;
            let _ = reader.read_se()?;
            let num = reader.read_ue()?;
            for _ in 0..num {
                let _ = reader.read_se()?;
            }
        }

        sps.max_num_ref_frames = reader.read_ue()? as u8;
        let _ = reader.read_bits(1)?;
        sps.pic_width_in_mbs_minus1 = reader.read_ue()? as u16;
        sps.pic_height_in_map_units_minus1 = reader.read_ue()? as u16;
        sps.frame_mbs_only_flag = reader.read_bits(1)? == 1;

        if !sps.frame_mbs_only_flag {
            let _ = reader.read_bits(1)?;
        }

        sps.direct_8x8_inference_flag = reader.read_bits(1)? == 1;
        sps.frame_cropping_flag = reader.read_bits(1)? == 1;

        if sps.frame_cropping_flag {
            sps.frame_crop_left_offset = reader.read_ue()? as u16;
            sps.frame_crop_right_offset = reader.read_ue()? as u16;
            sps.frame_crop_top_offset = reader.read_ue()? as u16;
            sps.frame_crop_bottom_offset = reader.read_ue()? as u16;
        }

        Ok(sps)
    }

    fn parse_pps(&self, data: &[u8]) -> Result<H264Pps> {
        if data.len() < 2 {
            return Err(anyhow!("PPS too short"));
        }

        let mut pps = H264Pps::default();
        pps.raw_data = data.to_vec();

        let mut reader = BitReader::new(&data[1..]);

        pps.pic_parameter_set_id = reader.read_ue()? as u8;
        pps.seq_parameter_set_id = reader.read_ue()? as u8;
        pps.entropy_coding_mode_flag = reader.read_bits(1)? == 1;
        pps.bottom_field_pic_order_in_frame_present_flag = reader.read_bits(1)? == 1;
        pps.num_slice_groups_minus1 = reader.read_ue()? as u8;

        if pps.num_slice_groups_minus1 > 0 {
            return Err(anyhow!("Slice groups not supported"));
        }

        pps.num_ref_idx_l0_default_active_minus1 = reader.read_ue()? as u8;
        pps.num_ref_idx_l1_default_active_minus1 = reader.read_ue()? as u8;
        pps.weighted_pred_flag = reader.read_bits(1)? == 1;
        pps.weighted_bipred_idc = reader.read_bits(2)? as u8;
        pps.pic_init_qp_minus26 = reader.read_se()? as i8;
        pps.pic_init_qs_minus26 = reader.read_se()? as i8;
        pps.chroma_qp_index_offset = reader.read_se()? as i8;
        pps.deblocking_filter_control_present_flag = reader.read_bits(1)? == 1;
        pps.constrained_intra_pred_flag = reader.read_bits(1)? == 1;
        pps.redundant_pic_cnt_present_flag = reader.read_bits(1)? == 1;

        Ok(pps)
    }

    unsafe fn update_session_parameters(&mut self) -> Result<()> {
        let sps = self.sps.as_ref().ok_or_else(|| anyhow!("No SPS"))?;
        let pps = self.pps.as_ref().ok_or_else(|| anyhow!("No PPS"))?;

        // Build StdVideoH264SequenceParameterSet
        let std_sps = vk::native::StdVideoH264SequenceParameterSet {
            flags: vk::native::StdVideoH264SpsFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoH264SpsFlags::new_bitfield_1(
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    if sps.direct_8x8_inference_flag { 1 } else { 0 },
                    0,
                    if sps.frame_mbs_only_flag { 1 } else { 0 },
                    0,
                    0,
                    0,
                    0,
                    if sps.frame_cropping_flag { 1 } else { 0 },
                    0,
                    0,
                ),
            },
            profile_idc: match sps.profile_idc {
                66 => vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_BASELINE,
                77 => vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN,
                _ => vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH,
            },
            level_idc: sps.level_idc.into(),
            chroma_format_idc: sps.chroma_format_idc.into(),
            seq_parameter_set_id: sps.seq_parameter_set_id,
            bit_depth_luma_minus8: sps.bit_depth_luma_minus8,
            bit_depth_chroma_minus8: sps.bit_depth_chroma_minus8,
            log2_max_frame_num_minus4: sps.log2_max_frame_num_minus4,
            pic_order_cnt_type: sps.pic_order_cnt_type,
            offset_for_non_ref_pic: 0,
            offset_for_top_to_bottom_field: 0,
            log2_max_pic_order_cnt_lsb_minus4: sps.log2_max_pic_order_cnt_lsb_minus4,
            num_ref_frames_in_pic_order_cnt_cycle: 0,
            max_num_ref_frames: sps.max_num_ref_frames,
            reserved1: 0,
            pic_width_in_mbs_minus1: sps.pic_width_in_mbs_minus1 as u32,
            pic_height_in_map_units_minus1: sps.pic_height_in_map_units_minus1 as u32,
            frame_crop_left_offset: sps.frame_crop_left_offset as u32,
            frame_crop_right_offset: sps.frame_crop_right_offset as u32,
            frame_crop_top_offset: sps.frame_crop_top_offset as u32,
            frame_crop_bottom_offset: sps.frame_crop_bottom_offset as u32,
            reserved2: 0,
            pOffsetForRefFrame: ptr::null(),
            pScalingLists: ptr::null(),
            pSequenceParameterSetVui: ptr::null(),
        };

        // Build StdVideoH264PictureParameterSet
        let std_pps = vk::native::StdVideoH264PictureParameterSet {
            flags: vk::native::StdVideoH264PpsFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoH264PpsFlags::new_bitfield_1(
                    0,
                    0,
                    if pps.constrained_intra_pred_flag {
                        1
                    } else {
                        0
                    },
                    if pps.deblocking_filter_control_present_flag {
                        1
                    } else {
                        0
                    },
                    if pps.weighted_pred_flag { 1 } else { 0 },
                    if pps.bottom_field_pic_order_in_frame_present_flag {
                        1
                    } else {
                        0
                    },
                    if pps.entropy_coding_mode_flag { 1 } else { 0 },
                    0,
                ),
            },
            seq_parameter_set_id: pps.seq_parameter_set_id,
            pic_parameter_set_id: pps.pic_parameter_set_id,
            num_ref_idx_l0_default_active_minus1: pps.num_ref_idx_l0_default_active_minus1,
            num_ref_idx_l1_default_active_minus1: pps.num_ref_idx_l1_default_active_minus1,
            weighted_bipred_idc: pps.weighted_bipred_idc.into(),
            pic_init_qp_minus26: pps.pic_init_qp_minus26,
            pic_init_qs_minus26: pps.pic_init_qs_minus26,
            chroma_qp_index_offset: pps.chroma_qp_index_offset,
            second_chroma_qp_index_offset: pps.second_chroma_qp_index_offset,
            pScalingLists: ptr::null(),
        };

        let sps_list = [std_sps];
        let pps_list = [std_pps];

        let h264_add = vk::VideoDecodeH264SessionParametersAddInfoKHR::default()
            .std_sp_ss(&sps_list)
            .std_pp_ss(&pps_list);

        let update_info = vk::VideoSessionParametersUpdateInfoKHR::default()
            .update_sequence_count(1)
            .push_next(&h264_add as *const _ as *mut _);

        self.video_queue_fn
            .update_video_session_parameters(self.video_session_params, &update_info)?;

        info!("Session parameters updated with SPS/PPS");
        Ok(())
    }

    unsafe fn decode_frame(
        &mut self,
        nal_data: &[u8],
        is_idr: bool,
        is_reference: bool,
    ) -> Result<Option<VideoFrame>> {
        // Copy bitstream (must include start codes)
        if nal_data.len() > self.bitstream_size as usize {
            return Err(anyhow!("Bitstream too large"));
        }
        ptr::copy_nonoverlapping(nal_data.as_ptr(), self.bitstream_mapped, nal_data.len());

        // Reset DPB on IDR
        if is_idr {
            for slot in &mut self.dpb_slots {
                slot.in_use = false;
            }
            self.current_dpb_index = 0;
        }

        // Get current DPB slot
        let dst_slot = self.current_dpb_index;
        let dst_image = &self.dpb_images[dst_slot];

        // Calculate POC according to H.264 spec (8.2.1)
        // For poc_type 0, we need to parse pic_order_cnt_lsb from slice header
        // For simplicity, we derive it from frame count but with proper wrapping
        let sps = self.sps.as_ref().unwrap();
        let max_poc_lsb = 1i32 << (sps.log2_max_pic_order_cnt_lsb_minus4 + 4);

        // Derive pic_order_cnt_lsb (simplified - real impl should parse from slice header)
        let pic_order_cnt_lsb = ((self.frame_count as i32 * 2) % max_poc_lsb) as i32;

        // Calculate POC MSB with proper wrapping detection per H.264 spec 8.2.1.1
        let poc_msb = if is_idr {
            // IDR resets POC
            self.prev_pic_order_cnt_msb = 0;
            self.prev_pic_order_cnt_lsb = 0;
            0
        } else {
            let prev_lsb = self.prev_pic_order_cnt_lsb;
            let prev_msb = self.prev_pic_order_cnt_msb;

            if pic_order_cnt_lsb < prev_lsb && (prev_lsb - pic_order_cnt_lsb) >= max_poc_lsb / 2 {
                // Wrapped forward
                prev_msb + max_poc_lsb
            } else if pic_order_cnt_lsb > prev_lsb
                && (pic_order_cnt_lsb - prev_lsb) > max_poc_lsb / 2
            {
                // Wrapped backward
                prev_msb - max_poc_lsb
            } else {
                prev_msb
            }
        };

        let poc = poc_msb + pic_order_cnt_lsb;

        // Update previous POC values for next frame
        self.prev_pic_order_cnt_lsb = pic_order_cnt_lsb;
        self.prev_pic_order_cnt_msb = poc_msb;

        // Reset command buffer
        self.device
            .reset_command_buffer(self.command_buffer, vk::CommandBufferResetFlags::empty())?;

        let begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);
        self.device
            .begin_command_buffer(self.command_buffer, &begin_info)?;

        // Reset query
        self.device
            .cmd_reset_query_pool(self.command_buffer, self.query_pool, 0, 1);

        // Transition image to decode layout
        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::NONE)
            .src_access_mask(vk::AccessFlags2::NONE)
            .dst_stage_mask(vk::PipelineStageFlags2::VIDEO_DECODE_KHR)
            .dst_access_mask(vk::AccessFlags2::VIDEO_DECODE_WRITE_KHR)
            .old_layout(vk::ImageLayout::UNDEFINED)
            .new_layout(vk::ImageLayout::VIDEO_DECODE_DST_KHR)
            .image(dst_image.image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let dep_info =
            vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier));
        self.device
            .cmd_pipeline_barrier2(self.command_buffer, &dep_info);

        // Setup picture resource for decode target
        let dst_picture_resource = vk::VideoPictureResourceInfoKHR::default()
            .coded_offset(vk::Offset2D { x: 0, y: 0 })
            .coded_extent(vk::Extent2D {
                width: self.aligned_width,
                height: self.aligned_height,
            })
            .base_array_layer(0)
            .image_view_binding(dst_image.view);

        // H.264 reference info for current frame (will be setup after decode)
        let std_ref_info = vk::native::StdVideoDecodeH264ReferenceInfo {
            flags: vk::native::StdVideoDecodeH264ReferenceInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoDecodeH264ReferenceInfoFlags::new_bitfield_1(
                    0, // top_field_flag (0 for progressive)
                    0, // bottom_field_flag (0 for progressive)
                    0, // used_for_long_term_reference
                    0, // is_non_existing
                ),
            },
            FrameNum: self.frame_count as u16,
            reserved: 0,
            PicOrderCnt: [poc, 0],
        };

        let mut h264_slot_info =
            vk::VideoDecodeH264DpbSlotInfoKHR::default().std_reference_info(&std_ref_info);

        // The setup reference slot (slotIndex = -1 for current decode target in begin coding,
        // but actual slot index in decode info)
        let setup_slot_info = vk::VideoReferenceSlotInfoKHR::default()
            .slot_index(dst_slot as i32)
            .picture_resource(&dst_picture_resource)
            .push_next(&mut h264_slot_info);

        // Build reference slots for VkVideoBeginCodingInfoKHR
        // Must include: all active reference slots + current decode target with slotIndex = -1
        let mut begin_ref_slots = Vec::new();
        let mut begin_h264_infos = Vec::new();
        let mut begin_pic_resources = Vec::new();

        // Add active reference slots
        for (i, slot) in self.dpb_slots.iter().enumerate() {
            if slot.in_use && i != dst_slot {
                begin_pic_resources.push(
                    vk::VideoPictureResourceInfoKHR::default()
                        .coded_offset(vk::Offset2D { x: 0, y: 0 })
                        .coded_extent(vk::Extent2D {
                            width: self.aligned_width,
                            height: self.aligned_height,
                        })
                        .base_array_layer(0)
                        .image_view_binding(self.dpb_images[i].view),
                );

                begin_h264_infos.push(vk::native::StdVideoDecodeH264ReferenceInfo {
                    flags: vk::native::StdVideoDecodeH264ReferenceInfoFlags {
                        _bitfield_align_1: [],
                        _bitfield_1:
                            vk::native::StdVideoDecodeH264ReferenceInfoFlags::new_bitfield_1(
                                0, 0, 0, 0,
                            ),
                    },
                    FrameNum: slot.frame_num,
                    reserved: 0,
                    PicOrderCnt: [slot.pic_order_cnt, 0],
                });
            }
        }

        // Create the slot infos (need separate allocation for pNext chain)
        let mut begin_dpb_slot_infos: Vec<vk::VideoDecodeH264DpbSlotInfoKHR> = begin_h264_infos
            .iter()
            .map(|info| vk::VideoDecodeH264DpbSlotInfoKHR::default().std_reference_info(info))
            .collect();

        // Now build the reference slot infos
        let mut ref_idx = 0;
        for (i, slot) in self.dpb_slots.iter().enumerate() {
            if slot.in_use && i != dst_slot {
                begin_ref_slots.push(
                    vk::VideoReferenceSlotInfoKHR::default()
                        .slot_index(i as i32)
                        .picture_resource(&begin_pic_resources[ref_idx])
                        .push_next(&mut begin_dpb_slot_infos[ref_idx]),
                );
                ref_idx += 1;
            }
        }

        // Add current decode target with slotIndex = -1
        // This tells Vulkan we're going to set up this slot during decode
        let mut current_h264_info =
            vk::VideoDecodeH264DpbSlotInfoKHR::default().std_reference_info(&std_ref_info);

        begin_ref_slots.push(
            vk::VideoReferenceSlotInfoKHR::default()
                .slot_index(-1) // -1 means "will be set up"
                .picture_resource(&dst_picture_resource)
                .push_next(&mut current_h264_info),
        );

        // Begin video coding scope
        let begin_info = vk::VideoBeginCodingInfoKHR::default()
            .video_session(self.video_session)
            .video_session_parameters(self.video_session_params)
            .reference_slots(&begin_ref_slots);

        self.video_queue_fn
            .cmd_begin_video_coding(self.command_buffer, &begin_info);

        // Reset session on first use (required by spec!)
        if !self.session_initialized {
            let control_info = vk::VideoCodingControlInfoKHR::default()
                .flags(vk::VideoCodingControlFlagsKHR::RESET);
            self.video_queue_fn
                .cmd_control_video_coding(self.command_buffer, &control_info);
            self.session_initialized = true;
            info!("Video session reset");
        }

        // Build reference list for decode (only actual references, not current frame)
        let mut decode_ref_slots = Vec::new();
        let mut decode_h264_infos: Vec<vk::VideoDecodeH264DpbSlotInfoKHR> = Vec::new();
        let mut decode_pic_resources = Vec::new();
        let mut decode_std_infos = Vec::new();

        if !is_idr {
            for (i, slot) in self.dpb_slots.iter().enumerate() {
                if slot.in_use && i != dst_slot {
                    decode_pic_resources.push(
                        vk::VideoPictureResourceInfoKHR::default()
                            .coded_offset(vk::Offset2D { x: 0, y: 0 })
                            .coded_extent(vk::Extent2D {
                                width: self.aligned_width,
                                height: self.aligned_height,
                            })
                            .base_array_layer(0)
                            .image_view_binding(self.dpb_images[i].view),
                    );

                    decode_std_infos.push(vk::native::StdVideoDecodeH264ReferenceInfo {
                        flags: vk::native::StdVideoDecodeH264ReferenceInfoFlags {
                            _bitfield_align_1: [],
                            _bitfield_1:
                                vk::native::StdVideoDecodeH264ReferenceInfoFlags::new_bitfield_1(
                                    0, 0, 0, 0,
                                ),
                        },
                        FrameNum: slot.frame_num,
                        reserved: 0,
                        PicOrderCnt: [slot.pic_order_cnt, 0],
                    });
                }
            }

            for (idx, info) in decode_std_infos.iter().enumerate() {
                decode_h264_infos
                    .push(vk::VideoDecodeH264DpbSlotInfoKHR::default().std_reference_info(info));
            }

            for (idx, _) in decode_std_infos.iter().enumerate() {
                let slot_idx = self
                    .dpb_slots
                    .iter()
                    .enumerate()
                    .filter(|(i, s)| s.in_use && *i != dst_slot)
                    .nth(idx)
                    .map(|(i, _)| i)
                    .unwrap();

                decode_ref_slots.push(
                    vk::VideoReferenceSlotInfoKHR::default()
                        .slot_index(slot_idx as i32)
                        .picture_resource(&decode_pic_resources[idx])
                        .push_next(&mut decode_h264_infos[idx]),
                );
            }
        }

        // H.264 picture info
        let std_pic_info = vk::native::StdVideoDecodeH264PictureInfo {
            flags: vk::native::StdVideoDecodeH264PictureInfoFlags {
                _bitfield_align_1: [],
                _bitfield_1: vk::native::StdVideoDecodeH264PictureInfoFlags::new_bitfield_1(
                    0,                                // field_pic_flag
                    0,                                // is_intra
                    if is_idr { 1 } else { 0 },       // IdrPicFlag
                    0,                                // bottom_field_flag
                    if is_reference { 1 } else { 0 }, // is_reference
                    0,                                // complementary_field_pair
                ),
            },
            seq_parameter_set_id: self.sps.as_ref().unwrap().seq_parameter_set_id,
            pic_parameter_set_id: self.pps.as_ref().unwrap().pic_parameter_set_id,
            reserved1: 0,
            reserved2: 0,
            frame_num: self.frame_count as u16,
            idr_pic_id: if is_idr { 0 } else { 0 },
            PicOrderCnt: [poc, 0],
        };

        // Find slice NAL offsets in the bitstream
        let slice_offsets = self.find_slice_offsets(nal_data);

        let mut h264_pic_info = vk::VideoDecodeH264PictureInfoKHR::default()
            .std_picture_info(&std_pic_info)
            .slice_offsets(&slice_offsets);

        // Align srcBufferRange to minBitstreamBufferSizeAlignment (required by spec)
        let raw_size = nal_data.len() as u64;
        let aligned_size = if self.min_bitstream_buffer_size_alignment > 0 {
            let align = self.min_bitstream_buffer_size_alignment;
            (raw_size + align - 1) / align * align
        } else {
            raw_size
        };

        // Build decode info
        let decode_info = vk::VideoDecodeInfoKHR::default()
            .src_buffer(self.bitstream_buffer)
            .src_buffer_offset(0)
            .src_buffer_range(aligned_size)
            .dst_picture_resource(&dst_picture_resource)
            .setup_reference_slot(&setup_slot_info)
            .reference_slots(&decode_ref_slots)
            .push_next(&mut h264_pic_info);

        // Begin query for decode status
        self.device.cmd_begin_query(
            self.command_buffer,
            self.query_pool,
            0,
            vk::QueryControlFlags::empty(),
        );

        // Decode!
        self.video_decode_queue_fn
            .cmd_decode_video(self.command_buffer, &decode_info);

        // End query
        self.device
            .cmd_end_query(self.command_buffer, self.query_pool, 0);

        // End video coding scope
        let end_info = vk::VideoEndCodingInfoKHR::default();
        self.video_queue_fn
            .cmd_end_video_coding(self.command_buffer, &end_info);

        // Transition for readback
        let barrier = vk::ImageMemoryBarrier2::default()
            .src_stage_mask(vk::PipelineStageFlags2::VIDEO_DECODE_KHR)
            .src_access_mask(vk::AccessFlags2::VIDEO_DECODE_WRITE_KHR)
            .dst_stage_mask(vk::PipelineStageFlags2::TRANSFER)
            .dst_access_mask(vk::AccessFlags2::TRANSFER_READ)
            .old_layout(vk::ImageLayout::VIDEO_DECODE_DST_KHR)
            .new_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .image(dst_image.image)
            .subresource_range(vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                base_mip_level: 0,
                level_count: 1,
                base_array_layer: 0,
                layer_count: 1,
            });

        let dep_info =
            vk::DependencyInfo::default().image_memory_barriers(std::slice::from_ref(&barrier));
        self.device
            .cmd_pipeline_barrier2(self.command_buffer, &dep_info);

        // Copy Y plane
        let y_copy = vk::BufferImageCopy::default()
            .buffer_offset(0)
            .buffer_row_length(self.aligned_width)
            .buffer_image_height(self.aligned_height)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::PLANE_0,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: self.aligned_width,
                height: self.aligned_height,
                depth: 1,
            });

        // Copy UV plane
        let uv_offset = (self.aligned_width * self.aligned_height) as u64;
        let uv_copy = vk::BufferImageCopy::default()
            .buffer_offset(uv_offset)
            .buffer_row_length(self.aligned_width)
            .buffer_image_height(self.aligned_height / 2)
            .image_subresource(vk::ImageSubresourceLayers {
                aspect_mask: vk::ImageAspectFlags::PLANE_1,
                mip_level: 0,
                base_array_layer: 0,
                layer_count: 1,
            })
            .image_offset(vk::Offset3D { x: 0, y: 0, z: 0 })
            .image_extent(vk::Extent3D {
                width: self.aligned_width / 2,
                height: self.aligned_height / 2,
                depth: 1,
            });

        self.device.cmd_copy_image_to_buffer(
            self.command_buffer,
            dst_image.image,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            self.staging_buffer,
            &[y_copy, uv_copy],
        );

        self.device.end_command_buffer(self.command_buffer)?;

        // Submit
        self.device.reset_fences(&[self.fence])?;

        let submit_info =
            vk::SubmitInfo::default().command_buffers(std::slice::from_ref(&self.command_buffer));

        self.device
            .queue_submit(self.decode_queue, &[submit_info], self.fence)?;

        self.device.wait_for_fences(&[self.fence], true, u64::MAX)?;

        // Check decode status from query pool
        let mut query_result: i32 = 0;
        let result = self.device.get_query_pool_results(
            self.query_pool,
            0,
            1,
            std::slice::from_mut(&mut query_result),
            vk::QueryResultFlags::WITH_STATUS_KHR,
        );

        if result.is_err() || query_result != vk::QueryResultStatusKHR::COMPLETE.as_raw() {
            warn!(
                "Video decode query status: result={:?}, status={}",
                result, query_result
            );
        }

        // Update DPB slot state (only mark as in_use if it's a reference frame)
        self.dpb_slots[dst_slot] = DpbSlotState {
            in_use: is_reference,
            frame_num: self.frame_count as u16,
            pic_order_cnt: poc,
            is_reference,
            is_long_term: false,
        };

        // Advance to next DPB slot
        self.current_dpb_index = (self.current_dpb_index + 1) % self.dpb_images.len();

        // Read back frame
        let y_size = (self.aligned_width * self.aligned_height) as usize;
        let uv_size = (self.aligned_width * self.aligned_height / 2) as usize;

        let mut y_plane = vec![0u8; y_size];
        let mut uv_plane = vec![0u8; uv_size];

        ptr::copy_nonoverlapping(self.staging_mapped, y_plane.as_mut_ptr(), y_size);
        ptr::copy_nonoverlapping(
            self.staging_mapped.add(y_size),
            uv_plane.as_mut_ptr(),
            uv_size,
        );

        trace!(
            "Decoded frame {}: {}x{}",
            self.frame_count,
            self.config.width,
            self.config.height
        );

        Ok(Some(VideoFrame {
            width: self.config.width,
            height: self.config.height,
            y_plane,
            u_plane: uv_plane,
            v_plane: Vec::new(),
            y_stride: self.aligned_width,
            u_stride: self.aligned_width,
            v_stride: 0,
            timestamp_us: 0,
            format: PixelFormat::NV12,
            color_range: ColorRange::Limited,
            color_space: ColorSpace::BT709,
            transfer_function: TransferFunction::SDR,
            gpu_frame: None,
        }))
    }

    /// Find slice NAL unit offsets in the bitstream
    /// Returns offsets to the start of each slice NAL (after start code)
    fn find_slice_offsets(&self, data: &[u8]) -> Vec<u32> {
        let mut offsets = Vec::new();
        let mut i = 0;

        while i < data.len().saturating_sub(4) {
            // Find start code (0x000001 or 0x00000001)
            if data[i] == 0 && data[i + 1] == 0 {
                let (start_code_len, nal_start) = if data[i + 2] == 1 {
                    (3, i + 3)
                } else if data[i + 2] == 0 && i + 3 < data.len() && data[i + 3] == 1 {
                    (4, i + 4)
                } else {
                    i += 1;
                    continue;
                };

                if nal_start >= data.len() {
                    break;
                }

                let nal_type = data[nal_start] & 0x1F;

                // Slice NAL types: 1 (non-IDR), 5 (IDR), 19 (auxiliary), 20 (extension), 21 (3D)
                if nal_type == 1 || nal_type == 5 {
                    // Offset should point to the start code, not after it
                    offsets.push(i as u32);
                }

                i = nal_start + 1;
            } else {
                i += 1;
            }
        }

        // Must have at least one slice offset
        if offsets.is_empty() {
            offsets.push(0);
        }

        trace!("Found {} slice offsets: {:?}", offsets.len(), offsets);
        offsets
    }

    /// Get decoder statistics
    pub fn get_stats(&self) -> DecoderStats {
        DecoderStats {
            frames_decoded: self.frame_count,
            dpb_size: self.dpb_slots.len() as u32,
            supports_dmabuf: true,
        }
    }
}

impl Drop for VulkanVideoDecoder {
    fn drop(&mut self) {
        info!("Destroying Vulkan Video decoder");

        unsafe {
            let _ = self.device.device_wait_idle();

            self.device.destroy_query_pool(self.query_pool, None);

            self.device.unmap_memory(self.staging_memory);
            self.device.destroy_buffer(self.staging_buffer, None);
            self.device.free_memory(self.staging_memory, None);

            for dpb in &self.dpb_images {
                self.device.destroy_image_view(dpb.view, None);
                self.device.destroy_image(dpb.image, None);
                self.device.free_memory(dpb.memory, None);
            }

            self.device.unmap_memory(self.bitstream_memory);
            self.device.destroy_buffer(self.bitstream_buffer, None);
            self.device.free_memory(self.bitstream_memory, None);

            self.device.destroy_fence(self.fence, None);
            self.device.destroy_command_pool(self.command_pool, None);

            self.video_queue_fn
                .destroy_video_session_parameters(self.video_session_params, None);
            self.video_queue_fn
                .destroy_video_session(self.video_session, None);

            for mem in &self._session_memory {
                self.device.free_memory(*mem, None);
            }

            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
}

/// Bit reader for NAL parsing
struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    fn read_bits(&mut self, count: u8) -> Result<u32> {
        let mut result = 0u32;
        for _ in 0..count {
            if self.byte_pos >= self.data.len() {
                return Err(anyhow!("End of data"));
            }
            let bit = (self.data[self.byte_pos] >> (7 - self.bit_pos)) & 1;
            result = (result << 1) | bit as u32;
            self.bit_pos += 1;
            if self.bit_pos == 8 {
                self.bit_pos = 0;
                self.byte_pos += 1;
            }
        }
        Ok(result)
    }

    fn read_ue(&mut self) -> Result<u32> {
        let mut zeros = 0u32;
        while self.read_bits(1)? == 0 {
            zeros += 1;
            if zeros > 31 {
                return Err(anyhow!("Invalid exp-golomb"));
            }
        }
        if zeros == 0 {
            return Ok(0);
        }
        let val = self.read_bits(zeros as u8)?;
        Ok((1 << zeros) - 1 + val)
    }

    fn read_se(&mut self) -> Result<i32> {
        let ue = self.read_ue()?;
        let se = if ue & 1 == 1 {
            ((ue + 1) / 2) as i32
        } else {
            -((ue / 2) as i32)
        };
        Ok(se)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = VulkanVideoConfig::default();
        assert_eq!(config.width, 1920);
        assert_eq!(config.height, 1080);
        assert_eq!(config.codec, VulkanVideoCodec::H264);
    }

    #[test]
    fn test_bit_reader() {
        let data = [0b10110100, 0b01010101];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_bits(1).unwrap(), 1);
        assert_eq!(reader.read_bits(1).unwrap(), 0);
        assert_eq!(reader.read_bits(2).unwrap(), 0b11);
    }

    #[test]
    fn test_exp_golomb() {
        // 1 -> 0, 010 -> 1, 011 -> 2
        let data = [0b10100110];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_ue().unwrap(), 0);
        assert_eq!(reader.read_ue().unwrap(), 1);
        assert_eq!(reader.read_ue().unwrap(), 2);
    }
}
