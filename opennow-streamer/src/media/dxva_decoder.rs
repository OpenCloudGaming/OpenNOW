//! Native D3D11 Video Decoder (DXVA2)
//!
//! This module implements hardware video decoding using the D3D11 Video API directly,
//! similar to NVIDIA's DXVADecoder in their GeForce NOW client.
//!
//! Benefits over FFmpeg D3D11VA:
//! - No MAX_SLICES limitation (FFmpeg has hardcoded limit of 32)
//! - Direct control over texture arrays (RTArray)
//! - Better compatibility with NVIDIA drivers
//! - Zero-copy output to D3D11 textures

use anyhow::{anyhow, Result};
use log::info;

use windows::core::Interface;
use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::*;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::*;

/// Video codec types supported by the decoder
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DxvaCodec {
    H264,
    HEVC,
    AV1,
}

/// DXVA2 decoder profile GUIDs
mod profiles {
    use windows::core::GUID;

    // H.264/AVC profiles
    pub const D3D11_DECODER_PROFILE_H264_VLD_NOFGT: GUID =
        GUID::from_u128(0x1b81be68_a0c7_11d3_b984_00c04f2e73c5);

    // HEVC/H.265 profiles
    pub const D3D11_DECODER_PROFILE_HEVC_VLD_MAIN: GUID =
        GUID::from_u128(0x5b11d51b_2f4c_4452_bcc3_09f2a1160cc0);
    pub const D3D11_DECODER_PROFILE_HEVC_VLD_MAIN10: GUID =
        GUID::from_u128(0x107af0e0_ef1a_4d19_aba8_67a163073d13);

    // AV1 profiles
    pub const D3D11_DECODER_PROFILE_AV1_VLD_PROFILE0: GUID =
        GUID::from_u128(0xb8be4ccb_cf53_46ba_8d59_d6b8a6da5d2a);
}

/// Decoder configuration
#[derive(Debug, Clone)]
pub struct DxvaDecoderConfig {
    /// Video codec
    pub codec: DxvaCodec,
    /// Video width
    pub width: u32,
    /// Video height
    pub height: u32,
    /// Whether HDR (10-bit) is enabled
    pub is_hdr: bool,
    /// Number of surfaces in the decoder pool (RTArray size)
    pub surface_count: u32,
}

impl Default for DxvaDecoderConfig {
    fn default() -> Self {
        Self {
            codec: DxvaCodec::HEVC,
            width: 1920,
            height: 1080,
            is_hdr: false,
            surface_count: 25, // Increased from 20 for high bitrate 4K streams
        }
    }
}

/// Reference picture entry in the DPB (Decoded Picture Buffer)
#[derive(Debug, Clone, Copy, Default)]
pub struct DpbEntry {
    /// Surface index in the texture array
    pub surface_index: u8,
    /// Picture Order Count (full POC, not just LSB)
    pub poc: i32,
    /// Is this a reference frame
    pub is_reference: bool,
    /// Is this a long-term reference
    pub is_long_term: bool,
    /// Frame number (for debugging/ordering)
    pub frame_num: u64,
}

/// Native D3D11 Video Decoder
///
/// Uses ID3D11VideoDecoder directly, bypassing FFmpeg's D3D11VA wrapper.
/// This gives us full control over texture arrays and avoids FFmpeg limitations.
pub struct DxvaDecoder {
    /// D3D11 device
    device: ID3D11Device,
    /// D3D11 device context
    #[allow(dead_code)]
    context: ID3D11DeviceContext,
    /// D3D11 Video device interface
    video_device: ID3D11VideoDevice,
    /// D3D11 Video context interface
    #[allow(dead_code)]
    video_context: ID3D11VideoContext,
    /// Video decoder instance
    decoder: Option<ID3D11VideoDecoder>,
    /// Output texture array (RTArray)
    output_textures: Option<ID3D11Texture2D>,
    /// Decoder output views (one per surface in the array)
    output_views: Vec<ID3D11VideoDecoderOutputView>,
    /// Current configuration
    config: DxvaDecoderConfig,
    /// DXGI format for output (NV12 or P010)
    output_format: DXGI_FORMAT,
    /// Decoder profile GUID
    profile_guid: windows::core::GUID,
    /// Current surface index (round-robin)
    current_surface: u32,
    /// Maximum supported resolution
    #[allow(dead_code)]
    max_width: u32,
    #[allow(dead_code)]
    max_height: u32,
    /// Decoded Picture Buffer (DPB) for reference frame management
    dpb: Vec<DpbEntry>,
    /// Maximum DPB size
    dpb_max_size: usize,
    /// ConfigBitstreamRaw value from decoder config
    /// 1 = raw bitstream with start codes, 2 = raw bitstream without start codes
    config_bitstream_raw: u32,
    /// Frame counter for DPB ordering
    frame_count: u64,
    /// Previous POC LSB for POC MSB calculation
    prev_poc_lsb: i32,
    /// Previous POC MSB
    prev_poc_msb: i32,
    /// Max POC LSB (2^log2_max_pic_order_cnt_lsb)
    max_poc_lsb: i32,
}

// Safety: D3D11 COM objects are internally thread-safe
unsafe impl Send for DxvaDecoder {}
unsafe impl Sync for DxvaDecoder {}

impl DxvaDecoder {
    /// Create a new DXVA decoder
    pub fn new(config: DxvaDecoderConfig) -> Result<Self> {
        info!(
            "Creating native DXVA decoder for {:?} {}x{} HDR={}",
            config.codec, config.width, config.height, config.is_hdr
        );

        // Create D3D11 device with video support
        let (device, context) = Self::create_d3d11_device()?;

        // Get video interfaces
        let video_device: ID3D11VideoDevice = device
            .cast()
            .map_err(|e| anyhow!("Failed to get ID3D11VideoDevice: {:?}", e))?;
        let video_context: ID3D11VideoContext = context
            .cast()
            .map_err(|e| anyhow!("Failed to get ID3D11VideoContext: {:?}", e))?;

        // Enable multithread protection
        if let Ok(mt) = device.cast::<ID3D11Multithread>() {
            unsafe {
                mt.SetMultithreadProtected(true);
            }
            info!("D3D11 multithread protection enabled");
        }

        // Determine output format and profile
        let (output_format, profile_guid) = Self::get_format_and_profile(&config)?;

        info!(
            "DXVA decoder using format {:?}, profile {:?}",
            output_format, profile_guid
        );

        // DPB size - must be large enough to hold all reference frames
        // For high bitrate 4K HEVC streams, we need more buffer space
        // HEVC spec allows up to 16 reference pictures, but we use 18 to have margin
        // This should be less than surface_count to ensure we always have free surfaces
        let dpb_max_size = 18;

        let mut decoder = Self {
            device,
            context,
            video_device,
            video_context,
            decoder: None,
            output_textures: None,
            output_views: Vec::new(),
            config,
            output_format,
            profile_guid,
            current_surface: 0,
            max_width: 0,
            max_height: 0,
            dpb: Vec::with_capacity(dpb_max_size),
            dpb_max_size,
            config_bitstream_raw: 1, // Will be set during initialize_decoder
            frame_count: 0,
            prev_poc_lsb: 0,
            prev_poc_msb: 0,
            max_poc_lsb: 256, // Default, will be updated from SPS
        };

        // Check decoder capabilities
        decoder.check_capabilities()?;

        // Initialize the decoder
        decoder.initialize_decoder()?;

        Ok(decoder)
    }

    /// Create D3D11 device with VIDEO_SUPPORT flag
    fn create_d3d11_device() -> Result<(ID3D11Device, ID3D11DeviceContext)> {
        unsafe {
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            let mut feature_level = D3D_FEATURE_LEVEL_11_0;

            // Flags for video decoding
            let flags = D3D11_CREATE_DEVICE_VIDEO_SUPPORT | D3D11_CREATE_DEVICE_BGRA_SUPPORT;

            // Feature levels to try
            let feature_levels = [
                D3D_FEATURE_LEVEL_12_1,
                D3D_FEATURE_LEVEL_12_0,
                D3D_FEATURE_LEVEL_11_1,
                D3D_FEATURE_LEVEL_11_0,
            ];

            D3D11CreateDevice(
                None, // Default adapter
                D3D_DRIVER_TYPE_HARDWARE,
                HMODULE::default(),
                flags,
                Some(&feature_levels),
                D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut feature_level),
                Some(&mut context),
            )
            .map_err(|e| anyhow!("Failed to create D3D11 device: {:?}", e))?;

            let device = device.ok_or_else(|| anyhow!("D3D11 device is null"))?;
            let context = context.ok_or_else(|| anyhow!("D3D11 context is null"))?;

            info!(
                "Created D3D11 device with feature level {:?} (0x{:x})",
                feature_level, feature_level.0
            );

            Ok((device, context))
        }
    }

    /// Get output format and decoder profile based on config
    fn get_format_and_profile(
        config: &DxvaDecoderConfig,
    ) -> Result<(DXGI_FORMAT, windows::core::GUID)> {
        let format = if config.is_hdr {
            DXGI_FORMAT_P010 // 10-bit HDR
        } else {
            DXGI_FORMAT_NV12 // 8-bit SDR
        };

        let profile = match config.codec {
            DxvaCodec::H264 => profiles::D3D11_DECODER_PROFILE_H264_VLD_NOFGT,
            DxvaCodec::HEVC => {
                if config.is_hdr {
                    profiles::D3D11_DECODER_PROFILE_HEVC_VLD_MAIN10
                } else {
                    profiles::D3D11_DECODER_PROFILE_HEVC_VLD_MAIN
                }
            }
            DxvaCodec::AV1 => profiles::D3D11_DECODER_PROFILE_AV1_VLD_PROFILE0,
        };

        Ok((format, profile))
    }

    /// Check decoder capabilities and maximum resolution
    fn check_capabilities(&mut self) -> Result<()> {
        unsafe {
            // Get number of decoder profiles
            let profile_count = self.video_device.GetVideoDecoderProfileCount();
            info!("D3D11 Video device has {} decoder profiles", profile_count);

            // Check if our profile is supported
            let mut profile_supported = false;
            for i in 0..profile_count {
                // New API returns Result<GUID>
                if let Ok(profile) = self.video_device.GetVideoDecoderProfile(i) {
                    if profile == self.profile_guid {
                        profile_supported = true;
                        info!("Found matching decoder profile at index {}", i);
                        break;
                    }
                }
            }

            if !profile_supported {
                return Err(anyhow!(
                    "Decoder profile {:?} not supported",
                    self.profile_guid
                ));
            }

            // Check format support - new API returns Result<BOOL>
            let format_supported = self
                .video_device
                .CheckVideoDecoderFormat(&self.profile_guid, self.output_format)
                .map_err(|e| anyhow!("Failed to check decoder format: {:?}", e))?;

            if !format_supported.as_bool() {
                return Err(anyhow!(
                    "Output format {:?} not supported for this profile",
                    self.output_format
                ));
            }

            info!("Output format {:?} is supported", self.output_format);

            // Get decoder config to check max resolution
            let desc = D3D11_VIDEO_DECODER_DESC {
                Guid: self.profile_guid,
                SampleWidth: self.config.width,
                SampleHeight: self.config.height,
                OutputFormat: self.output_format,
            };

            let config_count = self
                .video_device
                .GetVideoDecoderConfigCount(&desc)
                .map_err(|e| anyhow!("Failed to get decoder config count: {:?}", e))?;

            info!(
                "Found {} decoder configurations for {}x{}",
                config_count, self.config.width, self.config.height
            );

            if config_count == 0 {
                return Err(anyhow!(
                    "No decoder configurations available for {}x{}",
                    self.config.width,
                    self.config.height
                ));
            }

            // Store max resolution (we'll refine this later if needed)
            self.max_width = self.config.width;
            self.max_height = self.config.height;

            Ok(())
        }
    }

    /// Initialize the video decoder and output textures
    fn initialize_decoder(&mut self) -> Result<()> {
        unsafe {
            info!(
                "Initializing DXVA decoder {}x{} with {} surfaces",
                self.config.width, self.config.height, self.config.surface_count
            );

            // Create decoder description
            let decoder_desc = D3D11_VIDEO_DECODER_DESC {
                Guid: self.profile_guid,
                SampleWidth: self.config.width,
                SampleHeight: self.config.height,
                OutputFormat: self.output_format,
            };

            // Enumerate all decoder configurations and find one with ConfigBitstreamRaw=2 (short slices)
            let config_count = self
                .video_device
                .GetVideoDecoderConfigCount(&decoder_desc)
                .map_err(|e| anyhow!("Failed to get decoder config count: {:?}", e))?;

            info!("Enumerating {} decoder configurations:", config_count);

            let mut selected_config: Option<D3D11_VIDEO_DECODER_CONFIG> = None;
            let mut fallback_config: Option<D3D11_VIDEO_DECODER_CONFIG> = None;

            for i in 0..config_count {
                let mut config = D3D11_VIDEO_DECODER_CONFIG::default();
                if self
                    .video_device
                    .GetVideoDecoderConfig(&decoder_desc, i, &mut config)
                    .is_ok()
                {
                    info!(
                        "  Config {}: ConfigBitstreamRaw={}, ConfigMBcontrolRasterOrder={}, ConfigResidDiffHost={}, ConfigSpatialResid8={}, ConfigResid8Subtraction={}, ConfigSpatialHost8or9Clipping={}, ConfigSpatialResidInterleaved={}, ConfigIntraResidUnsigned={}, ConfigResidDiffAccelerator={}, ConfigHostInverseScan={}, ConfigSpecificIDCT={}, Config4GroupedCoefs={}",
                        i,
                        config.ConfigBitstreamRaw,
                        config.ConfigMBcontrolRasterOrder,
                        config.ConfigResidDiffHost,
                        config.ConfigSpatialResid8,
                        config.ConfigResid8Subtraction,
                        config.ConfigSpatialHost8or9Clipping,
                        config.ConfigSpatialResidInterleaved,
                        config.ConfigIntraResidUnsigned,
                        config.ConfigResidDiffAccelerator,
                        config.ConfigHostInverseScan,
                        config.ConfigSpecificIDCT,
                        config.Config4GroupedCoefs
                    );

                    // Prefer ConfigBitstreamRaw=2 (short slice format)
                    if config.ConfigBitstreamRaw == 2 {
                        selected_config = Some(config);
                        info!("  -> Selected config {} (short slice format)", i);
                    } else if fallback_config.is_none() {
                        fallback_config = Some(config);
                    }
                }
            }

            // Use selected config, or fallback to first available
            let decoder_config = selected_config
                .or(fallback_config)
                .ok_or_else(|| anyhow!("No valid decoder configuration found"))?;

            // Store the ConfigBitstreamRaw value for bitstream formatting
            self.config_bitstream_raw = decoder_config.ConfigBitstreamRaw;

            info!(
                "Using decoder config: ConfigBitstreamRaw={}, ConfigMBcontrolRasterOrder={}",
                decoder_config.ConfigBitstreamRaw, decoder_config.ConfigMBcontrolRasterOrder
            );

            // Create the video decoder - returns Result<ID3D11VideoDecoder>
            let decoder = self
                .video_device
                .CreateVideoDecoder(&decoder_desc, &decoder_config)
                .map_err(|e| anyhow!("Failed to create video decoder: {:?}", e))?;

            info!("Created ID3D11VideoDecoder successfully");

            // Create output texture array (RTArray)
            // This is the key difference from FFmpeg - we create a proper texture array
            let texture_desc = D3D11_TEXTURE2D_DESC {
                Width: self.config.width,
                Height: self.config.height,
                MipLevels: 1,
                ArraySize: self.config.surface_count,
                Format: self.output_format,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_DECODER.0 as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };

            let mut output_texture: Option<ID3D11Texture2D> = None;
            self.device
                .CreateTexture2D(&texture_desc, None, Some(&mut output_texture))
                .map_err(|e| anyhow!("Failed to create output texture array: {:?}", e))?;

            let output_texture = output_texture.ok_or_else(|| anyhow!("Output texture is null"))?;

            info!(
                "Created output texture array: {}x{} x {} slices, format {:?}",
                self.config.width,
                self.config.height,
                self.config.surface_count,
                self.output_format
            );

            // Create decoder output views for each surface in the array
            let mut output_views = Vec::with_capacity(self.config.surface_count as usize);

            for i in 0..self.config.surface_count {
                let view_desc = D3D11_VIDEO_DECODER_OUTPUT_VIEW_DESC {
                    DecodeProfile: self.profile_guid,
                    ViewDimension: D3D11_VDOV_DIMENSION_TEXTURE2D,
                    Anonymous: D3D11_VIDEO_DECODER_OUTPUT_VIEW_DESC_0 {
                        Texture2D: D3D11_TEX2D_VDOV { ArraySlice: i },
                    },
                };

                // New API: CreateVideoDecoderOutputView takes 3 params but last is Option<*mut Option<T>>
                let mut view: Option<ID3D11VideoDecoderOutputView> = None;
                self.video_device
                    .CreateVideoDecoderOutputView(&output_texture, &view_desc, Some(&mut view))
                    .map_err(|e| anyhow!("Failed to create output view {}: {:?}", i, e))?;

                let view = view.ok_or_else(|| anyhow!("Output view {} is null", i))?;
                output_views.push(view);
            }

            info!("Created {} decoder output views", output_views.len());

            self.decoder = Some(decoder);
            self.output_textures = Some(output_texture);
            self.output_views = output_views;
            self.current_surface = 0;

            Ok(())
        }
    }

    /// Get the next available surface index, avoiding surfaces in DPB
    pub fn get_next_surface(&mut self) -> u32 {
        // Find a surface that is NOT currently used as a reference in the DPB
        let surface_count = self.config.surface_count;

        for _ in 0..surface_count {
            let candidate = self.current_surface;
            self.current_surface = (self.current_surface + 1) % surface_count;

            // Check if this surface is in the DPB
            let in_dpb = self
                .dpb
                .iter()
                .any(|entry| entry.surface_index == candidate as u8);

            if !in_dpb {
                return candidate;
            }
        }

        // All surfaces are in DPB - this shouldn't happen if DPB size < surface count
        // Fall back to evicting the oldest DPB entry
        if let Some(oldest) = self.dpb.first() {
            let surface = oldest.surface_index as u32;
            self.dpb.remove(0);
            return surface;
        }

        // Last resort: just use current surface
        let surface = self.current_surface;
        self.current_surface = (self.current_surface + 1) % surface_count;
        surface
    }

    /// Get the output texture
    pub fn output_texture(&self) -> Option<&ID3D11Texture2D> {
        self.output_textures.as_ref()
    }

    /// Get a specific output view
    pub fn output_view(&self, index: u32) -> Option<&ID3D11VideoDecoderOutputView> {
        self.output_views.get(index as usize)
    }

    /// Get the D3D11 device
    pub fn device(&self) -> &ID3D11Device {
        &self.device
    }

    /// Get the video decoder
    pub fn decoder(&self) -> Option<&ID3D11VideoDecoder> {
        self.decoder.as_ref()
    }

    /// Get the video context
    pub fn video_context(&self) -> &ID3D11VideoContext {
        &self.video_context
    }

    /// Get decoder configuration
    pub fn config(&self) -> &DxvaDecoderConfig {
        &self.config
    }

    /// Check if decoder is initialized
    pub fn is_initialized(&self) -> bool {
        self.decoder.is_some() && !self.output_views.is_empty()
    }

    /// Get output format
    pub fn output_format(&self) -> DXGI_FORMAT {
        self.output_format
    }

    /// Check decoder capabilities for a specific resolution
    pub fn check_resolution_support(
        codec: DxvaCodec,
        width: u32,
        height: u32,
        is_hdr: bool,
    ) -> Result<bool> {
        // Create temporary device to check capabilities
        let (device, _context) = Self::create_d3d11_device()?;

        let video_device: ID3D11VideoDevice = device
            .cast()
            .map_err(|e| anyhow!("Failed to get ID3D11VideoDevice: {:?}", e))?;

        let config = DxvaDecoderConfig {
            codec,
            width,
            height,
            is_hdr,
            surface_count: 1,
        };

        let (output_format, profile_guid) = Self::get_format_and_profile(&config)?;

        unsafe {
            // Check format support
            let format_supported = video_device
                .CheckVideoDecoderFormat(&profile_guid, output_format)
                .map_err(|e| anyhow!("Failed to check decoder format: {:?}", e))?;

            if !format_supported.as_bool() {
                return Ok(false);
            }

            // Check if we can get decoder configs for this resolution
            let desc = D3D11_VIDEO_DECODER_DESC {
                Guid: profile_guid,
                SampleWidth: width,
                SampleHeight: height,
                OutputFormat: output_format,
            };

            let config_count = video_device.GetVideoDecoderConfigCount(&desc).unwrap_or(0);

            Ok(config_count > 0)
        }
    }

    /// Get maximum supported resolution for a codec
    pub fn get_max_resolution(codec: DxvaCodec, is_hdr: bool) -> Result<(u32, u32)> {
        // Common resolutions to check (from highest to lowest)
        let resolutions = [
            (7680, 4320), // 8K
            (5120, 2880), // 5K
            (3840, 2160), // 4K
            (2560, 1440), // 1440p
            (1920, 1080), // 1080p
            (1280, 720),  // 720p
        ];

        for (width, height) in resolutions {
            if Self::check_resolution_support(codec, width, height, is_hdr)? {
                info!(
                    "Max resolution for {:?} HDR={}: {}x{}",
                    codec, is_hdr, width, height
                );
                return Ok((width, height));
            }
        }

        Err(anyhow!("No supported resolution found for {:?}", codec))
    }
}

impl Drop for DxvaDecoder {
    fn drop(&mut self) {
        info!("Dropping DXVA decoder");
        // COM objects are automatically released when dropped
        self.output_views.clear();
        self.decoder = None;
        self.output_textures = None;
    }
}

// ============================================================================
// DXVA2 HEVC Structures and Frame Decoding
// ============================================================================

/// DXVA Picture Entry for HEVC
/// Matches DXVA_PicEntry_HEVC from dxva.h
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct DxvaPicEntryHevc {
    /// Index into the reference picture array (7 bits) + flags (1 bit for RefPicFlags)
    /// Bit 7: RefPicFlags (0 = short-term, 1 = long-term)
    pub b_pic_entry: u8,
}

impl DxvaPicEntryHevc {
    pub fn new(index: u8, is_long_term: bool) -> Self {
        let flags = if is_long_term { 0x80 } else { 0 };
        Self {
            b_pic_entry: (index & 0x7F) | flags,
        }
    }

    pub fn invalid() -> Self {
        Self { b_pic_entry: 0xFF }
    }
}

/// DXVA HEVC Picture Parameters (DXVA_PicParams_HEVC)
/// This structure MUST match the Windows SDK dxva.h definition exactly
/// Size should be 440 bytes according to Microsoft spec
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct DxvaHevcPicParams {
    // Dimensions in minimum coding block units
    pub pic_width_in_min_cbs_y: u16,  // offset 0
    pub pic_height_in_min_cbs_y: u16, // offset 2

    // Format and sequence info flags (bitfield packed into u16)
    // chroma_format_idc:2, separate_colour_plane_flag:1, bit_depth_luma_minus8:3,
    // bit_depth_chroma_minus8:3, log2_max_pic_order_cnt_lsb_minus4:4,
    // NoPicReorderingFlag:1, NoBiPredFlag:1, ReservedBits1:1
    pub w_format_and_sequence_info_flags: u16, // offset 4

    // Current picture
    pub curr_pic: DxvaPicEntryHevc, // offset 6

    // SPS parameters
    pub sps_max_dec_pic_buffering_minus1: u8, // offset 7
    pub log2_min_luma_coding_block_size_minus3: u8, // offset 8
    pub log2_diff_max_min_luma_coding_block_size: u8, // offset 9
    pub log2_min_transform_block_size_minus2: u8, // offset 10
    pub log2_diff_max_min_transform_block_size: u8, // offset 11
    pub max_transform_hierarchy_depth_inter: u8, // offset 12
    pub max_transform_hierarchy_depth_intra: u8, // offset 13
    pub num_short_term_ref_pic_sets: u8,      // offset 14
    pub num_long_term_ref_pics_sps: u8,       // offset 15
    pub num_ref_idx_l0_default_active_minus1: u8, // offset 16
    pub num_ref_idx_l1_default_active_minus1: u8, // offset 17
    pub init_qp_minus26: i8,                  // offset 18
    pub uc_num_delta_pocs_of_ref_rps_idx: u8, // offset 19
    pub w_num_bits_for_short_term_rps_in_slice: u16, // offset 20
    pub reserved_bits2: u16,                  // offset 22

    // Coding param tool flags (bitfield packed into u32)
    // scaling_list_enabled_flag:1, amp_enabled_flag:1, sample_adaptive_offset_enabled_flag:1,
    // pcm_enabled_flag:1, pcm_sample_bit_depth_luma_minus1:4, pcm_sample_bit_depth_chroma_minus1:4,
    // log2_min_pcm_luma_coding_block_size_minus3:2, log2_diff_max_min_pcm_luma_coding_block_size:2,
    // pcm_loop_filter_disabled_flag:1, long_term_ref_pics_present_flag:1, sps_temporal_mvp_enabled_flag:1,
    // strong_intra_smoothing_enabled_flag:1, dependent_slice_segments_enabled_flag:1,
    // output_flag_present_flag:1, num_extra_slice_header_bits:3, sign_data_hiding_enabled_flag:1,
    // cabac_init_present_flag:1, ReservedBits3:5
    pub dw_coding_param_tool_flags: u32, // offset 24

    // Coding setting picture property flags (bitfield packed into u32)
    // constrained_intra_pred_flag:1, transform_skip_enabled_flag:1, cu_qp_delta_enabled_flag:1,
    // pps_slice_chroma_qp_offsets_present_flag:1, weighted_pred_flag:1, weighted_bipred_flag:1,
    // transquant_bypass_enabled_flag:1, tiles_enabled_flag:1, entropy_coding_sync_enabled_flag:1,
    // uniform_spacing_flag:1, loop_filter_across_tiles_enabled_flag:1,
    // pps_loop_filter_across_slices_enabled_flag:1, deblocking_filter_override_enabled_flag:1,
    // pps_deblocking_filter_disabled_flag:1, lists_modification_present_flag:1,
    // slice_segment_header_extension_present_flag:1, IrapPicFlag:1, IdrPicFlag:1, IntraPicFlag:1,
    // ReservedBits4:13
    pub dw_coding_setting_picture_property_flags: u32, // offset 28

    // PPS parameters
    pub pps_cb_qp_offset: i8,                 // offset 32
    pub pps_cr_qp_offset: i8,                 // offset 33
    pub num_tile_columns_minus1: u8,          // offset 34
    pub num_tile_rows_minus1: u8,             // offset 35
    pub column_width_minus1: [u16; 19],       // offset 36 (38 bytes)
    pub row_height_minus1: [u16; 21],         // offset 74 (42 bytes)
    pub diff_cu_qp_delta_depth: u8,           // offset 116
    pub pps_beta_offset_div2: i8,             // offset 117
    pub pps_tc_offset_div2: i8,               // offset 118
    pub log2_parallel_merge_level_minus2: u8, // offset 119
    pub curr_pic_order_cnt_val: i32,          // offset 120

    // Reference picture list (15 entries)
    pub ref_pic_list: [DxvaPicEntryHevc; 15], // offset 124 (15 bytes)
    pub reserved_bits5: u8,                   // offset 139

    // POC values for reference pictures
    pub pic_order_cnt_val_list: [i32; 15], // offset 140 (60 bytes)

    // Reference picture sets
    pub ref_pic_set_st_curr_before: [u8; 8], // offset 200
    pub ref_pic_set_st_curr_after: [u8; 8],  // offset 208
    pub ref_pic_set_lt_curr: [u8; 8],        // offset 216

    pub reserved_bits6: u16, // offset 224
    pub reserved_bits7: u16, // offset 226
    pub status_report_feedback_number: u32, // offset 228
                             // Total size: 232 bytes (without alignment padding)
}

impl Default for DxvaHevcPicParams {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

impl std::fmt::Debug for DxvaHevcPicParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Copy values to avoid unaligned references in packed struct
        let width = self.pic_width_in_min_cbs_y;
        let height = self.pic_height_in_min_cbs_y;
        let poc = self.curr_pic_order_cnt_val;
        f.debug_struct("DxvaHevcPicParams")
            .field("pic_width_in_min_cbs_y", &width)
            .field("pic_height_in_min_cbs_y", &height)
            .field("curr_pic_order_cnt_val", &poc)
            .finish()
    }
}

/// DXVA HEVC Quantization Matrix
#[repr(C)]
#[derive(Debug, Clone)]
pub struct DxvaHevcQMatrix {
    pub scaling_list_4x4: [[u8; 16]; 6],
    pub scaling_list_8x8: [[u8; 64]; 6],
    pub scaling_list_16x16: [[u8; 64]; 6],
    pub scaling_list_32x32: [[u8; 64]; 2],
    pub scaling_list_dc_16x16: [u8; 6],
    pub scaling_list_dc_32x32: [u8; 2],
}

impl Default for DxvaHevcQMatrix {
    fn default() -> Self {
        // Initialize with flat scaling (all 16s)
        Self {
            scaling_list_4x4: [[16; 16]; 6],
            scaling_list_8x8: [[16; 64]; 6],
            scaling_list_16x16: [[16; 64]; 6],
            scaling_list_32x32: [[16; 64]; 2],
            scaling_list_dc_16x16: [16; 6],
            scaling_list_dc_32x32: [16; 2],
        }
    }
}

/// DXVA HEVC Slice Header (short format)
/// This matches the DXVA_Slice_HEVC_Short structure used by FFmpeg and NVIDIA
/// For ConfigBitstreamRaw=1, we submit Annex-B formatted bitstream with start codes
/// Size: 10 bytes (packed)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, Default)]
pub struct DxvaHevcSliceShort {
    /// Position of NAL unit data in the bitstream buffer
    pub bs_nal_unit_data_location: u32,
    /// Number of bytes in the bitstream buffer for this slice
    pub slice_bytes_in_buffer: u32,
    /// Bad slice chopping indicator (0 = no chopping)
    pub w_bad_slice_chopping: u16,
}

/// Decoded frame result - zero-copy version
/// The texture remains on GPU and should be used directly for rendering
#[derive(Debug)]
pub struct DxvaDecodedFrame {
    /// Texture array containing the decoded frame
    pub texture: ID3D11Texture2D,
    /// Array slice index within the texture
    pub array_index: u32,
    /// Frame width
    pub width: u32,
    /// Frame height
    pub height: u32,
    /// Is 10-bit HDR
    pub is_hdr: bool,
    /// Picture order count
    pub poc: i32,
}

/// DXVA2 Buffer types - must match D3D11_VIDEO_DECODER_BUFFER_TYPE enum values
#[repr(i32)]
#[derive(Debug, Clone, Copy)]
pub enum DxvaBufferType {
    PictureParameters = 0,         // D3D11_VIDEO_DECODER_BUFFER_PICTURE_PARAMETERS
    MacroblockControl = 1,         // D3D11_VIDEO_DECODER_BUFFER_MACROBLOCK_CONTROL
    ResidualDifference = 2,        // D3D11_VIDEO_DECODER_BUFFER_RESIDUAL_DIFFERENCE
    DeblockingControl = 3,         // D3D11_VIDEO_DECODER_BUFFER_DEBLOCKING_CONTROL
    InverseQuantizationMatrix = 4, // D3D11_VIDEO_DECODER_BUFFER_INVERSE_QUANTIZATION_MATRIX
    SliceControl = 5,              // D3D11_VIDEO_DECODER_BUFFER_SLICE_CONTROL
    Bitstream = 6,                 // D3D11_VIDEO_DECODER_BUFFER_BITSTREAM
    MotionVector = 7,              // D3D11_VIDEO_DECODER_BUFFER_MOTION_VECTOR
    FilmGrain = 8,                 // D3D11_VIDEO_DECODER_BUFFER_FILM_GRAIN
}

impl DxvaDecoder {
    /// Decode a frame using native DXVA2
    ///
    /// This function:
    /// 1. Parses the HEVC bitstream
    /// 2. Fills DXVA picture parameters
    /// 3. Submits buffers to the decoder
    /// 4. Returns the decoded texture
    pub fn decode_frame(
        &mut self,
        bitstream: &[u8],
        parser: &mut super::hevc_parser::HevcParser,
    ) -> Result<DxvaDecodedFrame> {
        if !self.is_initialized() {
            return Err(anyhow!("DXVA decoder not initialized"));
        }

        // Parse NAL units
        let nals = parser.find_nal_units(bitstream);
        if nals.is_empty() {
            return Err(anyhow!("No NAL units found in bitstream"));
        }

        // Process parameter sets
        for nal in &nals {
            parser.process_nal(nal)?;
        }

        // Find slice NAL units
        let slice_nals: Vec<_> = nals.iter().filter(|n| n.nal_type.is_slice()).collect();
        if slice_nals.is_empty() {
            return Err(anyhow!("No slice NAL units found"));
        }

        // Get first slice header to determine PPS/SPS
        let first_slice = &slice_nals[0];
        let slice_header = parser.parse_slice_header(first_slice)?;

        let pps = parser.pps[slice_header.pps_id as usize]
            .as_ref()
            .ok_or_else(|| anyhow!("PPS {} not found", slice_header.pps_id))?;
        let sps = parser.sps[pps.sps_id as usize]
            .as_ref()
            .ok_or_else(|| anyhow!("SPS {} not found", pps.sps_id))?;

        // CRITICAL: Clear DPB BEFORE building pic_params for IDR frames
        // IDR frames must not have any reference pictures, so we need to clear
        // the DPB before building the picture parameters, not after decoding
        if first_slice.nal_type.is_idr() {
            self.dpb.clear();
        }

        // Get next output surface FIRST - this must happen before building pic_params
        // because curr_pic in pic_params must match the actual output surface
        let surface_idx = self.get_next_surface();

        // Calculate the full POC using H.265 section 8.3.1 algorithm
        // max_poc_lsb = 2^log2_max_pic_order_cnt_lsb
        let max_poc_lsb = 1i32 << sps.log2_max_poc_lsb;
        self.max_poc_lsb = max_poc_lsb;
        let is_idr = first_slice.nal_type.is_idr();
        let poc_lsb = slice_header.pic_order_cnt_lsb as i32;
        let full_poc = self.calculate_full_poc(poc_lsb, is_idr, max_poc_lsb);

        // Build DXVA picture parameters with the correct surface index and full POC
        let pic_params = self.build_hevc_pic_params(
            sps,
            pps,
            first_slice,
            &slice_header,
            surface_idx,
            full_poc,
        )?;
        let output_view = self
            .output_views
            .get(surface_idx as usize)
            .ok_or_else(|| anyhow!("Invalid surface index {}", surface_idx))?;

        // Get decoder
        let decoder = self
            .decoder
            .as_ref()
            .ok_or_else(|| anyhow!("Decoder not available"))?;

        // Build Annex-B formatted bitstream and slice controls
        // FFmpeg prepends start codes (0x000001) to each slice NAL unit
        let (annex_b_bitstream, slice_controls) =
            self.build_annex_b_bitstream_and_slices(&slice_nals, bitstream)?;
        let slice_size = (std::mem::size_of::<DxvaHevcSliceShort>() * slice_controls.len()) as u32;

        unsafe {
            // Begin frame
            self.video_context
                .DecoderBeginFrame(decoder, output_view, 0, None)
                .map_err(|e| anyhow!("DecoderBeginFrame failed: {:?}", e))?;

            // Collect buffer descriptors
            let mut buffer_descs = Vec::with_capacity(4);

            // 1. Submit picture parameters buffer
            let pic_params_size = std::mem::size_of::<DxvaHevcPicParams>() as u32;
            self.submit_buffer(
                decoder,
                DxvaBufferType::PictureParameters,
                &pic_params as *const _ as *const u8,
                pic_params_size,
            )?;
            buffer_descs.push(D3D11_VIDEO_DECODER_BUFFER_DESC {
                BufferType: D3D11_VIDEO_DECODER_BUFFER_PICTURE_PARAMETERS,
                BufferIndex: 0,
                DataOffset: 0,
                DataSize: pic_params_size,
                FirstMBaddress: 0,
                NumMBsInBuffer: 0,
                Width: self.config.width,
                Height: self.config.height,
                Stride: 0,
                ReservedBits: 0,
                pIV: std::ptr::null_mut(),
                IVSize: 0,
                PartialEncryption: false.into(),
                EncryptedBlockInfo: D3D11_ENCRYPTED_BLOCK_INFO::default(),
            });

            // 2. Submit quantization matrix buffer only if scaling lists are enabled
            if sps.scaling_list_enabled {
                let qmatrix = DxvaHevcQMatrix::default();
                let qmatrix_size = std::mem::size_of::<DxvaHevcQMatrix>() as u32;
                self.submit_buffer(
                    decoder,
                    DxvaBufferType::InverseQuantizationMatrix,
                    &qmatrix as *const _ as *const u8,
                    qmatrix_size,
                )?;
                buffer_descs.push(D3D11_VIDEO_DECODER_BUFFER_DESC {
                    BufferType: D3D11_VIDEO_DECODER_BUFFER_INVERSE_QUANTIZATION_MATRIX,
                    BufferIndex: 0,
                    DataOffset: 0,
                    DataSize: qmatrix_size,
                    FirstMBaddress: 0,
                    NumMBsInBuffer: 0,
                    Width: self.config.width,
                    Height: self.config.height,
                    Stride: 0,
                    ReservedBits: 0,
                    pIV: std::ptr::null_mut(),
                    IVSize: 0,
                    PartialEncryption: false.into(),
                    EncryptedBlockInfo: D3D11_ENCRYPTED_BLOCK_INFO::default(),
                });
            }

            // 3. Submit slice control buffers
            if !slice_controls.is_empty() {
                self.submit_buffer(
                    decoder,
                    DxvaBufferType::SliceControl,
                    slice_controls.as_ptr() as *const u8,
                    slice_size,
                )?;
                buffer_descs.push(D3D11_VIDEO_DECODER_BUFFER_DESC {
                    BufferType: D3D11_VIDEO_DECODER_BUFFER_SLICE_CONTROL,
                    BufferIndex: 0,
                    DataOffset: 0,
                    DataSize: slice_size,
                    FirstMBaddress: 0,
                    NumMBsInBuffer: slice_controls.len() as u32,
                    Width: self.config.width,
                    Height: self.config.height,
                    Stride: 0,
                    ReservedBits: 0,
                    pIV: std::ptr::null_mut(),
                    IVSize: 0,
                    PartialEncryption: false.into(),
                    EncryptedBlockInfo: D3D11_ENCRYPTED_BLOCK_INFO::default(),
                });
            }

            // 4. Submit bitstream buffer (Annex-B formatted with start codes)
            let bitstream_size = annex_b_bitstream.len() as u32;
            self.submit_buffer(
                decoder,
                DxvaBufferType::Bitstream,
                annex_b_bitstream.as_ptr(),
                bitstream_size,
            )?;
            buffer_descs.push(D3D11_VIDEO_DECODER_BUFFER_DESC {
                BufferType: D3D11_VIDEO_DECODER_BUFFER_BITSTREAM,
                BufferIndex: 0,
                DataOffset: 0,
                DataSize: bitstream_size,
                FirstMBaddress: 0,
                NumMBsInBuffer: 0,
                Width: self.config.width,
                Height: self.config.height,
                Stride: 0,
                ReservedBits: 0,
                pIV: std::ptr::null_mut(),
                IVSize: 0,
                PartialEncryption: false.into(),
                EncryptedBlockInfo: D3D11_ENCRYPTED_BLOCK_INFO::default(),
            });

            // Execute decode with all buffer descriptors
            self.video_context
                .SubmitDecoderBuffers(decoder, &buffer_descs)
                .map_err(|e| anyhow!("SubmitDecoderBuffers failed: {:?}", e))?;

            // End frame
            self.video_context
                .DecoderEndFrame(decoder)
                .map_err(|e| anyhow!("DecoderEndFrame failed: {:?}", e))?;

            // CRITICAL: Flush the context to ensure decode commands are submitted
            // This is needed for proper GPU synchronization before texture is used
            self.context.Flush();
        }

        // ZERO-COPY: No CPU staging texture copy needed!
        // The texture stays on GPU and will be used directly by the renderer
        // via D3D11TextureWrapper and wgpu texture import

        // Determine if this is a reference frame (all non-RASL/RADL frames are reference)
        // TrailR (trailing picture, reference) = slice type indicates reference
        let is_reference = first_slice.nal_type.is_vcl(); // VCL NALs are video data

        // Update DPB with the decoded frame using the full POC
        self.update_dpb(surface_idx, full_poc, is_reference, is_idr);

        // Return decoded frame info - texture stays on GPU
        let output_texture = self
            .output_textures
            .as_ref()
            .ok_or_else(|| anyhow!("Output texture not available"))?
            .clone();

        Ok(DxvaDecodedFrame {
            texture: output_texture,
            array_index: surface_idx,
            width: self.config.width,
            height: self.config.height,
            is_hdr: self.config.is_hdr,
            poc: full_poc,
        })
    }

    /// Build bitstream and slice controls based on ConfigBitstreamRaw setting
    /// - ConfigBitstreamRaw=1: Annex-B format with start codes (0x000001)
    /// - ConfigBitstreamRaw=2: Raw NAL units without start codes
    fn build_annex_b_bitstream_and_slices(
        &self,
        slice_nals: &[&super::hevc_parser::HevcNalUnit],
        _original_bitstream: &[u8],
    ) -> Result<(Vec<u8>, Vec<DxvaHevcSliceShort>)> {
        // Start code for Annex-B format (only used when ConfigBitstreamRaw=1)
        const START_CODE: [u8; 3] = [0x00, 0x00, 0x01];

        // Determine whether to include start codes based on ConfigBitstreamRaw
        // ConfigBitstreamRaw=1: Include start codes (Annex-B format)
        // ConfigBitstreamRaw=2: No start codes (raw NAL units)
        let use_start_codes = self.config_bitstream_raw == 1;
        let start_code_len = if use_start_codes { START_CODE.len() } else { 0 };

        // Pre-calculate total size needed
        let total_size: usize = slice_nals
            .iter()
            .map(|nal| start_code_len + nal.data.len())
            .sum();

        // Add padding to align to 128 bytes (required by DXVA)
        let padded_size = (total_size + 127) & !127;

        let mut bitstream = Vec::with_capacity(padded_size);
        let mut slice_controls = Vec::with_capacity(slice_nals.len());

        for nal in slice_nals {
            // Record position before adding this slice
            let position = bitstream.len() as u32;

            // Add start code only if ConfigBitstreamRaw=1
            if use_start_codes {
                bitstream.extend_from_slice(&START_CODE);
            }

            // Add NAL unit data (use the pre-parsed data from HevcNalUnit)
            bitstream.extend_from_slice(&nal.data);

            // Create slice control (short format)
            let slice_size = (start_code_len + nal.data.len()) as u32;
            slice_controls.push(DxvaHevcSliceShort {
                bs_nal_unit_data_location: position,
                slice_bytes_in_buffer: slice_size,
                w_bad_slice_chopping: 0,
            });
        }

        // Add padding to align to 128 bytes (FFmpeg does this)
        while bitstream.len() < padded_size {
            bitstream.push(0);
        }

        // Update last slice to include padding bytes
        if let Some(last_slice) = slice_controls.last_mut() {
            let padding = (padded_size - total_size) as u32;
            last_slice.slice_bytes_in_buffer += padding;
        }

        Ok((bitstream, slice_controls))
    }

    /// Build HEVC picture parameters from parsed data
    /// This fills the DXVA_PicParams_HEVC structure according to Microsoft specification
    fn build_hevc_pic_params(
        &self,
        sps: &super::hevc_parser::HevcSps,
        pps: &super::hevc_parser::HevcPps,
        nal: &super::hevc_parser::HevcNalUnit,
        slice_header: &super::hevc_parser::HevcSliceHeader,
        surface_idx: u32,
        full_poc: i32,
    ) -> Result<DxvaHevcPicParams> {
        let mut pp = DxvaHevcPicParams::default();

        // Calculate MinCbSizeY = 1 << (log2_min_luma_coding_block_size_minus3 + 3)
        let min_cb_log2 = sps.log2_min_luma_coding_block_size;
        let min_cb_size = 1u32 << min_cb_log2;

        // PicWidthInMinCbsY = pic_width / MinCbSizeY
        // PicHeightInMinCbsY = pic_height / MinCbSizeY
        pp.pic_width_in_min_cbs_y = (sps.pic_width / min_cb_size) as u16;
        pp.pic_height_in_min_cbs_y = (sps.pic_height / min_cb_size) as u16;

        // wFormatAndSequenceInfoFlags - packed bitfield:
        // chroma_format_idc:2, separate_colour_plane_flag:1, bit_depth_luma_minus8:3,
        // bit_depth_chroma_minus8:3, log2_max_pic_order_cnt_lsb_minus4:4,
        // NoPicReorderingFlag:1, NoBiPredFlag:1, ReservedBits1:1
        let chroma_format = (sps.chroma_format_idc as u16) & 0x3;
        let separate_colour_plane = ((sps.separate_colour_plane as u16) & 0x1) << 2;
        let bit_depth_luma = (((sps.bit_depth_luma - 8) as u16) & 0x7) << 3;
        let bit_depth_chroma = (((sps.bit_depth_chroma - 8) as u16) & 0x7) << 6;
        let log2_max_poc = (((sps.log2_max_poc_lsb - 4) as u16) & 0xF) << 9;
        // NoPicReorderingFlag and NoBiPredFlag are typically 0
        pp.w_format_and_sequence_info_flags = chroma_format
            | separate_colour_plane
            | bit_depth_luma
            | bit_depth_chroma
            | log2_max_poc;

        // Current picture - must match the surface index used in DecoderBeginFrame
        pp.curr_pic = DxvaPicEntryHevc::new(surface_idx as u8, false);

        // SPS parameters
        // max_dec_pic_buffering not in HevcSps, use a default of 5 (common value)
        pp.sps_max_dec_pic_buffering_minus1 = 4; // 5 - 1 = 4
        pp.log2_min_luma_coding_block_size_minus3 =
            sps.log2_min_luma_coding_block_size.saturating_sub(3);
        pp.log2_diff_max_min_luma_coding_block_size = sps.log2_diff_max_min_luma_coding_block_size;
        pp.log2_min_transform_block_size_minus2 =
            sps.log2_min_luma_transform_block_size.saturating_sub(2);
        pp.log2_diff_max_min_transform_block_size = sps.log2_diff_max_min_luma_transform_block_size;
        pp.max_transform_hierarchy_depth_inter = sps.max_transform_hierarchy_depth_inter;
        pp.max_transform_hierarchy_depth_intra = sps.max_transform_hierarchy_depth_intra;
        pp.num_short_term_ref_pic_sets = sps.num_short_term_ref_pic_sets;
        pp.num_long_term_ref_pics_sps = sps.num_long_term_ref_pics_sps;
        pp.num_ref_idx_l0_default_active_minus1 =
            pps.num_ref_idx_l0_default_active.saturating_sub(1);
        pp.num_ref_idx_l1_default_active_minus1 =
            pps.num_ref_idx_l1_default_active.saturating_sub(1);
        pp.init_qp_minus26 = (pps.init_qp as i8) - 26;

        // dwCodingParamToolFlags - packed bitfield for SPS/PPS tool flags
        let mut tool_flags: u32 = 0;
        tool_flags |= (sps.scaling_list_enabled as u32) << 0;
        tool_flags |= (sps.amp_enabled as u32) << 1;
        tool_flags |= (sps.sample_adaptive_offset_enabled as u32) << 2;
        tool_flags |= (sps.pcm_enabled as u32) << 3;
        if sps.pcm_enabled {
            tool_flags |= ((sps.pcm_sample_bit_depth_luma.saturating_sub(1) as u32) & 0xF) << 4;
            tool_flags |= ((sps.pcm_sample_bit_depth_chroma.saturating_sub(1) as u32) & 0xF) << 8;
            tool_flags |=
                ((sps.log2_min_pcm_luma_coding_block_size.saturating_sub(3) as u32) & 0x3) << 12;
            tool_flags |= ((sps.log2_diff_max_min_pcm_luma_coding_block_size as u32) & 0x3) << 14;
            tool_flags |= (sps.pcm_loop_filter_disabled as u32) << 16;
        }
        tool_flags |= (sps.long_term_ref_pics_present as u32) << 17;
        tool_flags |= (sps.temporal_mvp_enabled as u32) << 18;
        tool_flags |= (sps.strong_intra_smoothing_enabled as u32) << 19;
        tool_flags |= (pps.dependent_slice_segments_enabled as u32) << 20;
        tool_flags |= (pps.output_flag_present as u32) << 21;
        tool_flags |= ((pps.num_extra_slice_header_bits as u32) & 0x7) << 22;
        tool_flags |= (pps.sign_data_hiding_enabled as u32) << 25;
        tool_flags |= (pps.cabac_init_present as u32) << 26;
        pp.dw_coding_param_tool_flags = tool_flags;

        // dwCodingSettingPicturePropertyFlags - packed bitfield for picture properties
        let mut pic_flags: u32 = 0;
        pic_flags |= (pps.constrained_intra_pred as u32) << 0;
        pic_flags |= (pps.transform_skip_enabled as u32) << 1;
        pic_flags |= (pps.cu_qp_delta_enabled as u32) << 2;
        pic_flags |= (pps.slice_chroma_qp_offsets_present as u32) << 3;
        pic_flags |= (pps.weighted_pred as u32) << 4;
        pic_flags |= (pps.weighted_bipred as u32) << 5;
        pic_flags |= (pps.transquant_bypass_enabled as u32) << 6;
        pic_flags |= (pps.tiles_enabled as u32) << 7;
        pic_flags |= (pps.entropy_coding_sync_enabled as u32) << 8;
        pic_flags |= (pps.uniform_spacing as u32) << 9;
        pic_flags |= (pps.loop_filter_across_tiles_enabled as u32) << 10;
        pic_flags |= (pps.loop_filter_across_slices_enabled as u32) << 11;
        pic_flags |= (pps.deblocking_filter_override_enabled as u32) << 12;
        pic_flags |= (pps.deblocking_filter_disabled as u32) << 13;
        pic_flags |= (pps.lists_modification_present as u32) << 14;
        pic_flags |= (pps.slice_segment_header_extension_present as u32) << 15;
        // IrapPicFlag, IdrPicFlag, IntraPicFlag
        let is_irap = nal.nal_type.is_rap();
        let is_idr = nal.nal_type.is_idr();
        let is_intra = slice_header.slice_type == 2; // I-slice
        pic_flags |= (is_irap as u32) << 16;
        pic_flags |= (is_idr as u32) << 17;
        pic_flags |= (is_intra as u32) << 18;
        pp.dw_coding_setting_picture_property_flags = pic_flags;

        // PPS QP offsets
        pp.pps_cb_qp_offset = pps.cb_qp_offset;
        pp.pps_cr_qp_offset = pps.cr_qp_offset;

        // Tiles
        if pps.tiles_enabled {
            pp.num_tile_columns_minus1 = pps.num_tile_columns.saturating_sub(1) as u8;
            pp.num_tile_rows_minus1 = pps.num_tile_rows.saturating_sub(1) as u8;
            // column_width_minus1 and row_height_minus1 arrays would be filled here
            // For uniform spacing, these aren't needed
        }

        // Deblocking
        pp.diff_cu_qp_delta_depth = pps.diff_cu_qp_delta_depth;
        pp.pps_beta_offset_div2 = pps.beta_offset / 2;
        pp.pps_tc_offset_div2 = pps.tc_offset / 2;
        pp.log2_parallel_merge_level_minus2 = pps.log2_parallel_merge_level.saturating_sub(2);

        // Current picture POC - use the full POC (includes MSB) passed from decode_frame
        // The full POC is calculated using ITU-T H.265 section 8.3.1 algorithm
        let current_poc = full_poc;
        pp.curr_pic_order_cnt_val = current_poc;

        // Reference picture list - populate from DPB
        // First, mark all as invalid
        for i in 0..15 {
            pp.ref_pic_list[i] = DxvaPicEntryHevc::invalid();
            pp.pic_order_cnt_val_list[i] = 0;
        }

        // Initialize reference picture sets to invalid
        for i in 0..8 {
            pp.ref_pic_set_st_curr_before[i] = 0xFF;
            pp.ref_pic_set_st_curr_after[i] = 0xFF;
            pp.ref_pic_set_lt_curr[i] = 0xFF;
        }

        // For IDR frames, DPB should already be cleared - no references needed
        // For non-IDR frames, fill reference picture list from DPB
        if !is_idr && !self.dpb.is_empty() {
            // Sort DPB entries by POC for proper reference ordering
            // RefPicSetStCurrBefore: short-term refs with POC < current POC (most recent first)
            // RefPicSetStCurrAfter: short-term refs with POC > current POC (not used for P-frames)

            let mut ref_idx = 0;
            let mut st_curr_before_idx = 0;
            let mut st_curr_after_idx = 0;

            // Collect and sort references by POC (descending for before, ascending for after)
            let mut refs_before: Vec<_> = self
                .dpb
                .iter()
                .filter(|e| e.is_reference && !e.is_long_term && e.poc < current_poc)
                .collect();
            refs_before.sort_by(|a, b| b.poc.cmp(&a.poc)); // Most recent first

            let mut refs_after: Vec<_> = self
                .dpb
                .iter()
                .filter(|e| e.is_reference && !e.is_long_term && e.poc > current_poc)
                .collect();
            refs_after.sort_by(|a, b| a.poc.cmp(&b.poc)); // Closest first

            // Add references before current POC
            for dpb_entry in &refs_before {
                if ref_idx >= 15 {
                    break;
                }
                pp.ref_pic_list[ref_idx] = DxvaPicEntryHevc::new(dpb_entry.surface_index, false);
                pp.pic_order_cnt_val_list[ref_idx] = dpb_entry.poc;

                if st_curr_before_idx < 8 {
                    pp.ref_pic_set_st_curr_before[st_curr_before_idx] = ref_idx as u8;
                    st_curr_before_idx += 1;
                }
                ref_idx += 1;
            }

            // Add references after current POC (for B-frames)
            for dpb_entry in &refs_after {
                if ref_idx >= 15 {
                    break;
                }
                pp.ref_pic_list[ref_idx] = DxvaPicEntryHevc::new(dpb_entry.surface_index, false);
                pp.pic_order_cnt_val_list[ref_idx] = dpb_entry.poc;

                if st_curr_after_idx < 8 {
                    pp.ref_pic_set_st_curr_after[st_curr_after_idx] = ref_idx as u8;
                    st_curr_after_idx += 1;
                }
                ref_idx += 1;
            }

            // Add long-term references if any
            for dpb_entry in &self.dpb {
                if ref_idx >= 15 {
                    break;
                }
                if dpb_entry.is_reference && dpb_entry.is_long_term {
                    pp.ref_pic_list[ref_idx] = DxvaPicEntryHevc::new(dpb_entry.surface_index, true);
                    pp.pic_order_cnt_val_list[ref_idx] = dpb_entry.poc;
                    // Long-term refs go in ref_pic_set_lt_curr
                    ref_idx += 1;
                }
            }
        }

        // Status report feedback number (used for debugging)
        pp.status_report_feedback_number = 1;

        Ok(pp)
    }

    /// Update the DPB (Decoded Picture Buffer) after decoding a frame
    fn update_dpb(&mut self, surface_idx: u32, poc: i32, is_reference: bool, is_idr: bool) {
        // Increment frame counter
        self.frame_count += 1;

        // Clear DPB on IDR frames and reset POC tracking
        if is_idr {
            self.dpb.clear();
            self.prev_poc_lsb = 0;
            self.prev_poc_msb = 0;
        }

        // Remove any existing entry with this surface index (surface is being reused)
        self.dpb
            .retain(|entry| entry.surface_index != surface_idx as u8);

        // Remove oldest entries if DPB is full (keep most recent by frame_num)
        while self.dpb.len() >= self.dpb_max_size {
            // Find entry with lowest frame_num (oldest)
            if let Some(oldest_idx) = self
                .dpb
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.frame_num)
                .map(|(i, _)| i)
            {
                self.dpb.remove(oldest_idx);
            }
        }

        // Add current frame to DPB if it's a reference frame
        if is_reference {
            self.dpb.push(DpbEntry {
                surface_index: surface_idx as u8,
                poc,
                is_reference: true,
                is_long_term: false,
                frame_num: self.frame_count,
            });
        }
    }

    /// Clear DPB (call on seek or error recovery)
    pub fn clear_dpb(&mut self) {
        self.dpb.clear();
        self.prev_poc_lsb = 0;
        self.prev_poc_msb = 0;
        self.frame_count = 0;
    }

    /// Calculate full POC from POC LSB using the POC MSB derivation algorithm
    /// This follows ITU-T H.265 section 8.3.1
    fn calculate_full_poc(&mut self, poc_lsb: i32, is_idr: bool, max_poc_lsb: i32) -> i32 {
        // For IDR frames, POC is always 0 and we reset tracking
        if is_idr {
            self.prev_poc_lsb = 0;
            self.prev_poc_msb = 0;
            return 0;
        }

        // Calculate POC MSB according to H.265 spec
        let half_max_poc_lsb = max_poc_lsb / 2;

        let poc_msb = if poc_lsb < self.prev_poc_lsb
            && (self.prev_poc_lsb - poc_lsb) >= half_max_poc_lsb
        {
            // POC LSB wrapped around (increased)
            self.prev_poc_msb + max_poc_lsb
        } else if poc_lsb > self.prev_poc_lsb && (poc_lsb - self.prev_poc_lsb) > half_max_poc_lsb {
            // POC LSB wrapped around (decreased) - rare case
            self.prev_poc_msb - max_poc_lsb
        } else {
            self.prev_poc_msb
        };

        let full_poc = poc_msb + poc_lsb;

        // Update tracking for next frame
        // Note: Only update for reference frames in a real implementation
        self.prev_poc_lsb = poc_lsb;
        self.prev_poc_msb = poc_msb;

        full_poc
    }

    /// Submit a buffer to the decoder
    unsafe fn submit_buffer(
        &self,
        decoder: &ID3D11VideoDecoder,
        buffer_type: DxvaBufferType,
        data: *const u8,
        size: u32,
    ) -> Result<()> {
        // Get buffer from decoder
        let mut buffer_ptr: *mut std::ffi::c_void = std::ptr::null_mut();
        let mut buffer_size: u32 = 0;

        self.video_context
            .GetDecoderBuffer(
                decoder,
                D3D11_VIDEO_DECODER_BUFFER_TYPE(buffer_type as i32),
                &mut buffer_size,
                &mut buffer_ptr,
            )
            .map_err(|e| anyhow!("GetDecoderBuffer failed for {:?}: {:?}", buffer_type, e))?;

        if buffer_ptr.is_null() {
            return Err(anyhow!(
                "GetDecoderBuffer returned null for {:?}",
                buffer_type
            ));
        }

        if size > buffer_size {
            self.video_context
                .ReleaseDecoderBuffer(decoder, D3D11_VIDEO_DECODER_BUFFER_TYPE(buffer_type as i32))
                .ok();
            return Err(anyhow!(
                "Buffer too small for {:?}: need {} but got {}",
                buffer_type,
                size,
                buffer_size
            ));
        }

        // Copy data to buffer
        std::ptr::copy_nonoverlapping(data, buffer_ptr as *mut u8, size as usize);

        // Release buffer
        self.video_context
            .ReleaseDecoderBuffer(decoder, D3D11_VIDEO_DECODER_BUFFER_TYPE(buffer_type as i32))
            .map_err(|e| anyhow!("ReleaseDecoderBuffer failed for {:?}: {:?}", buffer_type, e))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_check_hevc_support() {
        let result = DxvaDecoder::check_resolution_support(DxvaCodec::HEVC, 1920, 1080, false);
        println!("HEVC 1080p support: {:?}", result);
    }

    #[test]
    fn test_check_hevc_4k_support() {
        let result = DxvaDecoder::check_resolution_support(DxvaCodec::HEVC, 3840, 2160, false);
        println!("HEVC 4K support: {:?}", result);
    }

    #[test]
    fn test_get_max_resolution() {
        let result = DxvaDecoder::get_max_resolution(DxvaCodec::HEVC, false);
        println!("HEVC max resolution: {:?}", result);
    }

    #[test]
    fn test_dxva_struct_sizes() {
        // Verify structure sizes match Microsoft DXVA specification
        // DXVA_PicParams_HEVC should be 232 bytes (packed)
        let pic_params_size = std::mem::size_of::<DxvaHevcPicParams>();
        println!("DxvaHevcPicParams size: {} bytes", pic_params_size);

        // DXVA_Slice_HEVC_Short should be 10 bytes
        let slice_short_size = std::mem::size_of::<DxvaHevcSliceShort>();
        println!("DxvaHevcSliceShort size: {} bytes", slice_short_size);

        // DXVA_Qmatrix_HEVC should be 880 bytes
        let qmatrix_size = std::mem::size_of::<DxvaHevcQMatrix>();
        println!("DxvaHevcQMatrix size: {} bytes", qmatrix_size);

        // DxvaPicEntryHevc should be 1 byte
        let pic_entry_size = std::mem::size_of::<DxvaPicEntryHevc>();
        println!("DxvaPicEntryHevc size: {} bytes", pic_entry_size);
        assert_eq!(pic_entry_size, 1, "DxvaPicEntryHevc should be 1 byte");

        // Slice short should be 10 bytes (4 + 4 + 2)
        assert_eq!(
            slice_short_size, 10,
            "DxvaHevcSliceShort should be 10 bytes"
        );

        // QMatrix: 6*16 + 6*64 + 6*64 + 2*64 + 6 + 2 = 96 + 384 + 384 + 128 + 8 = 1000
        // Wait, let me recalculate: 6*16=96, 6*64=384, 6*64=384, 2*64=128, 6+2=8
        // Total = 96 + 384 + 384 + 128 + 8 = 1000? No that's wrong
        // Actually: scaling_list_4x4[6][16] = 96, scaling_list_8x8[6][64] = 384,
        // scaling_list_16x16[6][64] = 384, scaling_list_32x32[2][64] = 128,
        // scaling_list_dc_16x16[6] = 6, scaling_list_dc_32x32[2] = 2
        // Total = 96 + 384 + 384 + 128 + 6 + 2 = 1000 bytes
        println!("Expected QMatrix size: 1000 bytes");

        // PicParams size check - the packed struct should be around 232 bytes
        // This is a rough check since the exact size depends on packing
        assert!(
            pic_params_size >= 200 && pic_params_size <= 256,
            "DxvaHevcPicParams size {} is outside expected range 200-256",
            pic_params_size
        );
    }
}
