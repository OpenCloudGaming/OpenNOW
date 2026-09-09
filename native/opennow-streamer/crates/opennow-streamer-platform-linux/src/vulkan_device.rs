use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::{Error, PixelFormat, Result, Subsystem, VideoCodec};

#[derive(Debug, Clone, Copy)]
pub struct VulkanDeviceInfo {
    pub instance: usize,
    pub physical_device: usize,
    pub device: usize,
    pub graphics_queue: usize,
    pub graphics_queue_family_index: u32,
    pub graphics_queue_index: u32,
    pub api_version: u32,
}

#[derive(Debug)]
pub struct SharedVulkanDevice {
    #[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
    device: *mut ffmpeg_next::ffi::AVBufferRef,
    info: VulkanDeviceInfo,
    profiles: [Option<ProfileLimits>; 12],
    failed: AtomicBool,
    #[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
    copy_queue: (u32, u32),
}

#[derive(Debug, Clone, Copy)]
struct ProfileLimits {
    minimum: [u32; 2],
    maximum: [u32; 2],
}

unsafe impl Send for SharedVulkanDevice {}
unsafe impl Sync for SharedVulkanDevice {}

impl SharedVulkanDevice {
    pub fn create() -> Result<Arc<Self>> {
        #[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
        {
            implementation::create()
        }
        #[cfg(not(all(feature = "ffmpeg", feature = "vulkan")))]
        Err(Error::unavailable(
            Subsystem::Vulkan,
            "shared Vulkan Video requires the ffmpeg and vulkan features",
        ))
    }

    pub fn info(&self) -> VulkanDeviceInfo {
        self.info
    }

    pub fn codec_support(&self, codec: VideoCodec, ten_bit: bool) -> bool {
        self.codec_format_support(
            codec,
            if ten_bit {
                PixelFormat::P010
            } else {
                PixelFormat::Nv12
            },
        )
    }

    pub fn codec_format_support(&self, codec: VideoCodec, pixel_format: PixelFormat) -> bool {
        !self.failed.load(Ordering::Acquire)
            && format_profile_index(codec, pixel_format)
                .is_some_and(|index| self.profiles[index].is_some())
    }

    pub fn supports(&self, codec: VideoCodec, ten_bit: bool, width: u32, height: u32) -> bool {
        self.supports_format(
            codec,
            if ten_bit {
                PixelFormat::P010
            } else {
                PixelFormat::Nv12
            },
            width,
            height,
        )
    }

    pub fn supports_format(
        &self,
        codec: VideoCodec,
        pixel_format: PixelFormat,
        width: u32,
        height: u32,
    ) -> bool {
        !self.failed.load(Ordering::Acquire)
            && format_profile_index(codec, pixel_format)
                .and_then(|index| self.profiles[index])
                .is_some_and(|limits| {
                    width >= limits.minimum[0]
                        && height >= limits.minimum[1]
                        && width <= limits.maximum[0]
                        && height <= limits.maximum[1]
                })
    }

    #[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
    pub(crate) fn invalidate(&self) {
        self.failed.store(true, Ordering::Release);
    }

    #[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
    pub(crate) fn copy_queue(&self) -> (u32, u32) {
        self.copy_queue
    }

    #[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
    pub(crate) fn retain(&self) -> *mut ffmpeg_next::ffi::AVBufferRef {
        unsafe { ffmpeg_next::ffi::av_buffer_ref(self.device) }
    }
}

fn profile_index(codec: VideoCodec, ten_bit: bool) -> usize {
    let codec = match codec {
        VideoCodec::H264 => 0,
        VideoCodec::H265 => 1,
        VideoCodec::Av1 => 2,
    };
    codec * 2 + usize::from(ten_bit)
}

fn format_profile_index(codec: VideoCodec, pixel_format: PixelFormat) -> Option<usize> {
    let offset = match pixel_format {
        PixelFormat::Nv12 | PixelFormat::P010 => 0,
        PixelFormat::Nv24 | PixelFormat::P410 => 6,
        _ => return None,
    };
    Some(profile_index(codec, pixel_format.is_ten_bit()) + offset)
}

#[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
impl Drop for SharedVulkanDevice {
    fn drop(&mut self) {
        unsafe { ffmpeg_next::ffi::av_buffer_unref(&mut self.device) };
    }
}

#[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
mod implementation {
    use super::*;
    use ash::{vk, vk::Handle};
    use ffmpeg_next::ffi;
    use std::{ffi::CStr, ptr};

    pub(super) fn create() -> Result<Arc<SharedVulkanDevice>> {
        ffmpeg_next::init()
            .map_err(|error| Error::backend(Subsystem::Ffmpeg, error.to_string()))?;
        let mut device = ptr::null_mut();
        let mut options = ptr::null_mut();
        let entry = unsafe { ash::Entry::load() }
            .map_err(|error| Error::unavailable(Subsystem::Vulkan, error.to_string()))?;
        let extensions = unsafe { entry.enumerate_instance_extension_properties(None) }
            .map_err(|error| Error::unavailable(Subsystem::Vulkan, error.to_string()))?;
        let hdr_colorspace = extensions.iter().any(|extension| unsafe {
            CStr::from_ptr(extension.extension_name.as_ptr())
                == ash::ext::swapchain_colorspace::NAME
        });
        let instance_extensions = if hdr_colorspace {
            c"VK_KHR_surface+VK_KHR_xlib_surface+VK_KHR_xcb_surface+VK_KHR_wayland_surface+VK_EXT_swapchain_colorspace"
        } else {
            c"VK_KHR_surface+VK_KHR_xlib_surface+VK_KHR_xcb_surface+VK_KHR_wayland_surface"
        };
        unsafe {
            ffi::av_dict_set(
                &mut options,
                c"instance_extensions".as_ptr(),
                instance_extensions.as_ptr(),
                0,
            );
            ffi::av_dict_set(
                &mut options,
                c"device_extensions".as_ptr(),
                c"VK_KHR_swapchain".as_ptr(),
                0,
            );
            ffi::av_dict_set(&mut options, c"limit_queues".as_ptr(), c"2".as_ptr(), 0);
        }
        let result = unsafe {
            ffi::av_hwdevice_ctx_create(
                &mut device,
                ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VULKAN,
                ptr::null(),
                options,
                0,
            )
        };
        unsafe { ffi::av_dict_free(&mut options) };
        if result < 0 || device.is_null() {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                format!(
                    "shared Vulkan device creation failed: {}",
                    ffmpeg_next::Error::from(result)
                ),
            ));
        }
        let initialized = unsafe { initialize(device) };
        match initialized {
            Ok(owner) => Ok(Arc::new(owner)),
            Err(error) => {
                unsafe { ffi::av_buffer_unref(&mut device) };
                Err(error)
            }
        }
    }

    unsafe fn initialize(device: *mut ffi::AVBufferRef) -> Result<SharedVulkanDevice> {
        let context = unsafe { &mut *((*device).data.cast::<ffi::AVHWDeviceContext>()) };
        let vulkan = unsafe { &mut *context.hwctx.cast::<ffi::AVVulkanDeviceContext>() };
        let entry = unsafe { ash::Entry::load() }
            .map_err(|error| Error::unavailable(Subsystem::Vulkan, error.to_string()))?;
        let instance = unsafe {
            ash::Instance::load(
                entry.static_fn(),
                vk::Instance::from_raw(vulkan.inst as u64),
            )
        };
        let physical = vk::PhysicalDevice::from_raw(vulkan.phys_dev as u64);
        let logical = unsafe {
            ash::Device::load(
                instance.fp_v1_0(),
                vk::Device::from_raw(vulkan.act_dev as u64),
            )
        };
        let properties = unsafe { instance.get_physical_device_properties(physical) };
        let adapter = unsafe { CStr::from_ptr(properties.device_name.as_ptr()) }.to_string_lossy();
        eprintln!(
            "embedded Vulkan adapter={adapter} vendor={:#x} device={:#x} driver={} api={}.{}.{}",
            properties.vendor_id,
            properties.device_id,
            properties.driver_version,
            vk::api_version_major(properties.api_version),
            vk::api_version_minor(properties.api_version),
            vk::api_version_patch(properties.api_version)
        );
        let mut families = vulkan.qf[..usize::try_from(vulkan.nb_qf).unwrap_or(0).min(64)].to_vec();
        let (family, reserved, copy_family) = reserve_graphics_queue(&mut families)?;
        vulkan.nb_qf = families.len() as i32;
        vulkan.qf[..families.len()].copy_from_slice(&families);
        eprintln!(
            "embedded Vulkan Qt queue={family}:{reserved}, native copy queue={copy_family}:0, native families={:?}",
            families
                .iter()
                .map(|family| (family.idx, family.num, family.flags, family.video_caps))
                .collect::<Vec<_>>()
        );
        let queue = unsafe { logical.get_device_queue(family, reserved) };
        let extensions = if vulkan.nb_enabled_dev_extensions > 0 {
            unsafe {
                std::slice::from_raw_parts(
                    vulkan.enabled_dev_extensions,
                    vulkan.nb_enabled_dev_extensions as usize,
                )
            }
        } else {
            &[]
        };
        let enabled = |name: &CStr| {
            extensions
                .iter()
                .any(|extension| unsafe { CStr::from_ptr(*extension) == name })
        };
        if !enabled(ash::khr::swapchain::NAME)
            || !enabled(ash::khr::video_queue::NAME)
            || !enabled(ash::khr::video_decode_queue::NAME)
        {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                "shared Vulkan device lacks enabled swapchain/video decode extensions",
            ));
        }
        let video = ash::khr::video_queue::Instance::new(&entry, &instance);
        let mut profiles = [None; 12];
        for codec in [VideoCodec::H264, VideoCodec::H265, VideoCodec::Av1] {
            let extension = match codec {
                VideoCodec::H264 => ash::khr::video_decode_h264::NAME,
                VideoCodec::H265 => ash::khr::video_decode_h265::NAME,
                VideoCodec::Av1 => ash::khr::video_decode_av1::NAME,
            };
            let operation = match codec {
                VideoCodec::H264 => vk::VideoCodecOperationFlagsKHR::DECODE_H264,
                VideoCodec::H265 => vk::VideoCodecOperationFlagsKHR::DECODE_H265,
                VideoCodec::Av1 => vk::VideoCodecOperationFlagsKHR::DECODE_AV1,
            };
            if !enabled(extension)
                || !families
                    .iter()
                    .any(|family| family.video_caps as u32 & operation.as_raw() != 0)
            {
                continue;
            }
            for pixel_format in [
                PixelFormat::Nv12,
                PixelFormat::P010,
                PixelFormat::Nv24,
                PixelFormat::P410,
            ] {
                let index = format_profile_index(codec, pixel_format).unwrap();
                let formats = if pixel_format.is_ten_bit() {
                    [vk::Format::R16_UNORM, vk::Format::R16G16_UNORM]
                } else {
                    [vk::Format::R8_UNORM, vk::Format::R8G8_UNORM]
                };
                let required = vk::FormatFeatureFlags::SAMPLED_IMAGE
                    | vk::FormatFeatureFlags::SAMPLED_IMAGE_FILTER_LINEAR
                    | vk::FormatFeatureFlags::TRANSFER_DST;
                if formats.iter().all(|format| {
                    unsafe { instance.get_physical_device_format_properties(physical, *format) }
                        .optimal_tiling_features
                        .contains(required)
                }) {
                    profiles[index] = query_profile(
                        &video,
                        physical,
                        codec,
                        pixel_format,
                        enabled(ash::khr::video_encode_queue::NAME)
                            && enabled(ash::khr::video_maintenance1::NAME),
                    );
                }
                eprintln!(
                    "embedded Vulkan codec={} format={:?} supported={}",
                    codec.label(),
                    pixel_format,
                    profiles[index].is_some()
                );
            }
        }
        if profiles.iter().all(Option::is_none) {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                "shared Vulkan device has no supported decode profile",
            ));
        }
        let info = VulkanDeviceInfo {
            instance: vulkan.inst as usize,
            physical_device: vulkan.phys_dev as usize,
            device: vulkan.act_dev as usize,
            graphics_queue: queue.as_raw() as usize,
            graphics_queue_family_index: family,
            graphics_queue_index: reserved,
            api_version: vk::API_VERSION_1_3,
        };
        Ok(SharedVulkanDevice {
            device,
            info,
            profiles,
            failed: AtomicBool::new(false),
            copy_queue: (copy_family, 0),
        })
    }

    fn reserve_graphics_queue(
        families: &mut Vec<ffi::AVVulkanDeviceQueueFamily>,
    ) -> Result<(u32, u32, u32)> {
        let family = families
            .iter()
            .find(|family| family.flags & vk::QueueFlags::GRAPHICS.as_raw() != 0)
            .ok_or_else(|| {
                Error::unavailable(
                    Subsystem::Vulkan,
                    "shared Vulkan device has no graphics queue",
                )
            })?;
        let index = family.idx;
        let count = families
            .iter()
            .filter(|family| family.idx == index)
            .map(|family| family.num)
            .min()
            .unwrap_or(0);
        if index < 0 || count < 1 {
            return Err(Error::unavailable(
                Subsystem::Vulkan,
                "shared Vulkan device has no usable graphics queue",
            ));
        }
        if count == 1 {
            let native = families
                .iter()
                .filter(|family| family.idx != index && family.num > 0)
                .collect::<Vec<_>>();
            let compute = native
                .iter()
                .any(|family| family.flags & vk::QueueFlags::COMPUTE.as_raw() != 0);
            let decode = native
                .iter()
                .any(|family| family.flags & vk::QueueFlags::VIDEO_DECODE_KHR.as_raw() != 0);
            let copy = native
                .iter()
                .find(|family| family.flags & vk::QueueFlags::COMPUTE.as_raw() != 0);
            if !compute || !decode || copy.is_none() {
                return Err(Error::unavailable(
                    Subsystem::Vulkan,
                    "cannot isolate the Qt graphics queue from native compute, copy and video decode queues",
                ));
            }
            let copy_family = copy.unwrap().idx as u32;
            families.retain(|family| family.idx != index);
            return Ok((index as u32, 0, copy_family));
        }
        let reserved = count - 1;
        for family in families.iter_mut().filter(|family| family.idx == index) {
            family.num = reserved;
        }
        Ok((index as u32, reserved as u32, index as u32))
    }

    fn query_profile(
        video: &ash::khr::video_queue::Instance,
        physical: vk::PhysicalDevice,
        codec: VideoCodec,
        pixel_format: PixelFormat,
        encode_source: bool,
    ) -> Option<ProfileLimits> {
        let ten_bit = pixel_format.is_ten_bit();
        if codec == VideoCodec::H264 && (ten_bit || pixel_format.is_444()) {
            return None;
        }
        let depth = if ten_bit {
            vk::VideoComponentBitDepthFlagsKHR::TYPE_10
        } else {
            vk::VideoComponentBitDepthFlagsKHR::TYPE_8
        };
        let mut profile = vk::VideoProfileInfoKHR::default()
            .chroma_subsampling(if pixel_format.is_444() {
                vk::VideoChromaSubsamplingFlagsKHR::TYPE_444
            } else {
                vk::VideoChromaSubsamplingFlagsKHR::TYPE_420
            })
            .luma_bit_depth(depth)
            .chroma_bit_depth(depth);
        let mut h264 = vk::VideoDecodeH264ProfileInfoKHR::default()
            .std_profile_idc(vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_HIGH)
            .picture_layout(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE);
        let mut h265 = vk::VideoDecodeH265ProfileInfoKHR::default()
            .std_profile_idc(h265_profile(pixel_format));
        let mut av1 = vk::VideoDecodeAV1ProfileInfoKHR::default()
            .std_profile(if pixel_format.is_444() {
                vk::native::StdVideoAV1Profile_STD_VIDEO_AV1_PROFILE_HIGH
            } else {
                vk::native::StdVideoAV1Profile_STD_VIDEO_AV1_PROFILE_MAIN
            })
            .film_grain_support(true);
        profile = match codec {
            VideoCodec::H264 => profile
                .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
                .push_next(&mut h264),
            VideoCodec::H265 => profile
                .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H265)
                .push_next(&mut h265),
            VideoCodec::Av1 => profile
                .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_AV1)
                .push_next(&mut av1),
        };
        let mut decode = vk::VideoDecodeCapabilitiesKHR::default();
        let mut h264_caps = vk::VideoDecodeH264CapabilitiesKHR::default();
        let mut h265_caps = vk::VideoDecodeH265CapabilitiesKHR::default();
        let mut av1_caps = vk::VideoDecodeAV1CapabilitiesKHR::default();
        let caps = vk::VideoCapabilitiesKHR::default().push_next(&mut decode);
        let mut caps = match codec {
            VideoCodec::H264 => caps.push_next(&mut h264_caps),
            VideoCodec::H265 => caps.push_next(&mut h265_caps),
            VideoCodec::Av1 => caps.push_next(&mut av1_caps),
        };
        unsafe {
            (video.fp().get_physical_device_video_capabilities_khr)(physical, &profile, &mut caps)
        }
        .result()
        .ok()?;
        let limits = ProfileLimits {
            minimum: [caps.min_coded_extent.width, caps.min_coded_extent.height],
            maximum: [caps.max_coded_extent.width, caps.max_coded_extent.height],
        };
        let profiles = [profile];
        let mut list = vk::VideoProfileListInfoKHR::default().profiles(&profiles);
        let (usage, output_usage) = decode_image_usage(decode.flags, encode_source)?;
        let format_info = vk::PhysicalDeviceVideoFormatInfoKHR::default()
            .image_usage(usage)
            .push_next(&mut list);
        let formats = query_formats(video, physical, &format_info)?;
        let expected = decode_output_format(pixel_format)?;
        if !unambiguous_output(&formats, expected) {
            return None;
        }
        if usage != output_usage {
            let output_info = vk::PhysicalDeviceVideoFormatInfoKHR::default()
                .image_usage(output_usage)
                .push_next(&mut list);
            if !query_formats(video, physical, &output_info)?
                .iter()
                .any(|format| {
                    format.format == expected && format.image_tiling == vk::ImageTiling::OPTIMAL
                })
            {
                return None;
            }
        }
        Some(limits)
    }

    fn decode_image_usage(
        flags: vk::VideoDecodeCapabilityFlagsKHR,
        encode_source: bool,
    ) -> Option<(vk::ImageUsageFlags, vk::ImageUsageFlags)> {
        let coincide = flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE);
        if !coincide && !flags.contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_DISTINCT)
        {
            return None;
        }
        let mut output = vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
            | vk::ImageUsageFlags::TRANSFER_SRC
            | vk::ImageUsageFlags::SAMPLED;
        if coincide {
            output |= vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR;
        }
        if encode_source {
            output |= vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR;
        }
        Some((
            if coincide {
                output
            } else {
                vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR
            },
            output,
        ))
    }

    fn unambiguous_output(
        formats: &[vk::VideoFormatPropertiesKHR<'_>],
        expected: vk::Format,
    ) -> bool {
        !formats.is_empty()
            && formats.iter().all(|format| {
                format.format == expected && format.image_tiling == vk::ImageTiling::OPTIMAL
            })
    }

    fn query_formats(
        video: &ash::khr::video_queue::Instance,
        physical: vk::PhysicalDevice,
        format_info: &vk::PhysicalDeviceVideoFormatInfoKHR<'_>,
    ) -> Option<Vec<vk::VideoFormatPropertiesKHR<'static>>> {
        let mut count = 0;
        unsafe {
            (video.fp().get_physical_device_video_format_properties_khr)(
                physical,
                format_info,
                &mut count,
                ptr::null_mut(),
            )
        }
        .result()
        .ok()?;
        if count == 0 || count > 64 {
            return None;
        }
        let mut formats = vec![vk::VideoFormatPropertiesKHR::default(); count as usize];
        unsafe {
            (video.fp().get_physical_device_video_format_properties_khr)(
                physical,
                format_info,
                &mut count,
                formats.as_mut_ptr(),
            )
        }
        .result()
        .ok()?;
        if count as usize > formats.len() {
            return None;
        }
        formats.truncate(count as usize);
        Some(formats)
    }

    fn h265_profile(pixel_format: PixelFormat) -> vk::native::StdVideoH265ProfileIdc {
        if pixel_format.is_444() {
            vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_FORMAT_RANGE_EXTENSIONS
        } else if pixel_format.is_ten_bit() {
            vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN_10
        } else {
            vk::native::StdVideoH265ProfileIdc_STD_VIDEO_H265_PROFILE_IDC_MAIN
        }
    }

    fn decode_output_format(pixel_format: PixelFormat) -> Option<vk::Format> {
        Some(match pixel_format {
            PixelFormat::Nv12 => vk::Format::G8_B8R8_2PLANE_420_UNORM,
            PixelFormat::P010 => vk::Format::G10X6_B10X6R10X6_2PLANE_420_UNORM_3PACK16,
            PixelFormat::Nv24 => vk::Format::G8_B8R8_2PLANE_444_UNORM,
            PixelFormat::P410 => vk::Format::G10X6_B10X6R10X6_2PLANE_444_UNORM_3PACK16,
            _ => return None,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn decoder_output_admission_rejects_competing_planar_formats() {
            for (expected, competing) in [
                (
                    vk::Format::G8_B8R8_2PLANE_444_UNORM,
                    vk::Format::G8_B8_R8_3PLANE_444_UNORM,
                ),
                (
                    vk::Format::G10X6_B10X6R10X6_2PLANE_444_UNORM_3PACK16,
                    vk::Format::G16_B16_R16_3PLANE_444_UNORM,
                ),
            ] {
                let supported = vk::VideoFormatPropertiesKHR::default()
                    .format(expected)
                    .image_tiling(vk::ImageTiling::OPTIMAL);
                let alternative = vk::VideoFormatPropertiesKHR::default()
                    .format(competing)
                    .image_tiling(vk::ImageTiling::OPTIMAL);
                assert!(unambiguous_output(&[supported], expected));
                assert!(unambiguous_output(&[supported, supported], expected));
                assert!(!unambiguous_output(&[supported, alternative], expected));
                assert!(!unambiguous_output(&[alternative, supported], expected));
                assert!(!unambiguous_output(
                    &[supported.image_tiling(vk::ImageTiling::LINEAR)],
                    expected
                ));
                assert!(!unambiguous_output(&[], expected));
            }
        }

        #[test]
        fn format_queries_match_ffmpeg_dpb_mode_and_enabled_encode_usage() {
            let dpb = vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR;
            let output = vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
                | vk::ImageUsageFlags::TRANSFER_SRC
                | vk::ImageUsageFlags::SAMPLED;
            let coincide = vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_COINCIDE;
            let distinct = vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_DISTINCT;
            assert_eq!(
                decode_image_usage(coincide, false),
                Some((dpb | output, dpb | output))
            );
            assert_eq!(decode_image_usage(distinct, false), Some((dpb, output)));
            assert_eq!(
                decode_image_usage(coincide | distinct, false),
                Some((dpb | output, dpb | output))
            );
            let encode = vk::ImageUsageFlags::VIDEO_ENCODE_SRC_KHR;
            assert_eq!(
                decode_image_usage(coincide, true),
                Some((dpb | output | encode, dpb | output | encode))
            );
            assert_eq!(
                decode_image_usage(distinct, true),
                Some((dpb, output | encode))
            );
            assert_eq!(
                decode_image_usage(vk::VideoDecodeCapabilityFlagsKHR::empty(), false),
                None
            );
        }

        #[test]
        fn four_four_four_requires_exact_decode_output_and_hevc_rext() {
            assert_eq!(h265_profile(PixelFormat::Nv24), 4);
            assert_eq!(h265_profile(PixelFormat::P410), 4);
            assert_eq!(h265_profile(PixelFormat::Nv12), 1);
            assert_eq!(h265_profile(PixelFormat::P010), 2);
            assert_eq!(
                decode_output_format(PixelFormat::Nv24),
                Some(vk::Format::G8_B8R8_2PLANE_444_UNORM)
            );
            assert_eq!(
                decode_output_format(PixelFormat::P410),
                Some(vk::Format::G10X6_B10X6R10X6_2PLANE_444_UNORM_3PACK16)
            );
            assert_ne!(
                decode_output_format(PixelFormat::Nv12),
                decode_output_format(PixelFormat::Nv24)
            );
            assert_ne!(
                decode_output_format(PixelFormat::P010),
                decode_output_format(PixelFormat::P410)
            );
            assert_eq!(decode_output_format(PixelFormat::I420), None);
        }

        #[test]
        fn graphics_queue_is_excluded_from_every_alias() {
            let mut families = vec![
                ffi::AVVulkanDeviceQueueFamily {
                    idx: 0,
                    num: 2,
                    flags: vk::QueueFlags::GRAPHICS.as_raw(),
                    video_caps: 0,
                },
                ffi::AVVulkanDeviceQueueFamily {
                    idx: 0,
                    num: 2,
                    flags: vk::QueueFlags::COMPUTE.as_raw(),
                    video_caps: 0,
                },
                ffi::AVVulkanDeviceQueueFamily {
                    idx: 1,
                    num: 2,
                    flags: vk::QueueFlags::VIDEO_DECODE_KHR.as_raw(),
                    video_caps: 0,
                },
            ];
            assert_eq!(reserve_graphics_queue(&mut families).unwrap(), (0, 1, 0));
            assert_eq!(
                families.iter().map(|family| family.num).collect::<Vec<_>>(),
                [1, 1, 2]
            );
        }

        #[test]
        fn single_graphics_queue_is_rejected() {
            let mut families = vec![ffi::AVVulkanDeviceQueueFamily {
                idx: 0,
                num: 1,
                flags: vk::QueueFlags::GRAPHICS.as_raw(),
                video_caps: 0,
            }];
            assert!(reserve_graphics_queue(&mut families).is_err());
        }

        #[test]
        fn single_graphics_queue_is_reserved_when_native_families_are_separate() {
            let mut families = vec![
                ffi::AVVulkanDeviceQueueFamily {
                    idx: 0,
                    num: 1,
                    flags: vk::QueueFlags::GRAPHICS.as_raw(),
                    video_caps: 0,
                },
                ffi::AVVulkanDeviceQueueFamily {
                    idx: 1,
                    num: 2,
                    flags: (vk::QueueFlags::COMPUTE | vk::QueueFlags::TRANSFER).as_raw(),
                    video_caps: 0,
                },
                ffi::AVVulkanDeviceQueueFamily {
                    idx: 2,
                    num: 1,
                    flags: vk::QueueFlags::VIDEO_DECODE_KHR.as_raw(),
                    video_caps: 1,
                },
            ];
            assert_eq!(reserve_graphics_queue(&mut families).unwrap(), (0, 0, 1));
            assert!(families.iter().all(|family| family.idx != 0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chroma_profiles_do_not_alias_and_other_formats_are_not_advertised() {
        let mut indices = Vec::new();
        for codec in [VideoCodec::H264, VideoCodec::H265, VideoCodec::Av1] {
            for format in [
                PixelFormat::Nv12,
                PixelFormat::P010,
                PixelFormat::Nv24,
                PixelFormat::P410,
            ] {
                indices.push(format_profile_index(codec, format).unwrap());
            }
            for format in [PixelFormat::I420, PixelFormat::Bgra8, PixelFormat::Rgba8] {
                assert_eq!(format_profile_index(codec, format), None);
            }
        }
        indices.sort_unstable();
        assert_eq!(indices, (0..12).collect::<Vec<_>>());
    }

    #[test]
    fn profiles_do_not_alias_codecs_or_bit_depths() {
        let mut indices = Vec::new();
        for codec in [VideoCodec::H264, VideoCodec::H265, VideoCodec::Av1] {
            for ten_bit in [false, true] {
                indices.push(profile_index(codec, ten_bit));
            }
        }
        assert_eq!(indices, [0, 1, 2, 3, 4, 5]);
    }

    #[test]
    fn capability_checks_require_the_exact_profile_dimensions_and_live_device() {
        let mut profiles = [None; 12];
        profiles[profile_index(VideoCodec::H265, true)] = Some(ProfileLimits {
            minimum: [64, 64],
            maximum: [3840, 2160],
        });
        profiles[format_profile_index(VideoCodec::H265, PixelFormat::Nv24).unwrap()] =
            Some(ProfileLimits {
                minimum: [128, 128],
                maximum: [1920, 1080],
            });
        let owner = SharedVulkanDevice {
            #[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
            device: std::ptr::null_mut(),
            info: VulkanDeviceInfo {
                instance: 0,
                physical_device: 0,
                device: 0,
                graphics_queue: 0,
                graphics_queue_family_index: 0,
                graphics_queue_index: 0,
                api_version: 0,
            },
            profiles,
            failed: AtomicBool::new(false),
            #[cfg(all(feature = "ffmpeg", feature = "vulkan"))]
            copy_queue: (0, 0),
        };
        assert!(owner.supports(VideoCodec::H265, true, 3840, 2160));
        assert!(!owner.supports(VideoCodec::H265, true, 4096, 2160));
        assert!(!owner.supports(VideoCodec::H265, true, 32, 32));
        assert!(!owner.supports(VideoCodec::H265, false, 1920, 1080));
        assert!(!owner.supports(VideoCodec::H264, true, 1920, 1080));
        assert!(!owner.codec_format_support(VideoCodec::H265, PixelFormat::P410));
        assert!(!owner.supports_format(VideoCodec::H265, PixelFormat::P410, 1920, 1080));
        assert!(!owner.codec_format_support(VideoCodec::H265, PixelFormat::Rgba8));
        assert!(owner.codec_format_support(VideoCodec::H265, PixelFormat::Nv24));
        assert!(owner.supports_format(VideoCodec::H265, PixelFormat::Nv24, 1919, 1079));
        assert!(!owner.supports_format(VideoCodec::H265, PixelFormat::Nv24, 1921, 1080));
        assert!(!owner.supports_format(VideoCodec::H265, PixelFormat::Nv24, 64, 64));
        owner.failed.store(true, Ordering::Release);
        assert!(!owner.codec_format_support(VideoCodec::H265, PixelFormat::Nv24));
        assert!(!owner.supports_format(VideoCodec::H265, PixelFormat::Nv24, 1920, 1080));
        assert!(!owner.codec_support(VideoCodec::H265, true));
        assert!(!owner.supports(VideoCodec::H265, true, 1920, 1080));
    }
}
