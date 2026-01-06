//! HEVC/H.265 NAL Unit Parser for DXVA2
//!
//! This module parses HEVC bitstreams and extracts the necessary parameter sets
//! (VPS, SPS, PPS) and slice headers needed for DXVA2 hardware decoding.
//!
//! Based on ITU-T H.265 specification.

use anyhow::{anyhow, Result};
use log::{debug, trace, warn};

/// HEVC NAL unit types (ITU-T H.265 Table 7-1)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HevcNalType {
    TrailN = 0,
    TrailR = 1,
    TsaN = 2,
    TsaR = 3,
    StsaN = 4,
    StsaR = 5,
    RadlN = 6,
    RadlR = 7,
    RaslN = 8,
    RaslR = 9,
    // Reserved 10-15
    BlaWLp = 16,
    BlaWRadl = 17,
    BlaNLp = 18,
    IdrWRadl = 19,
    IdrNLp = 20,
    CraNut = 21,
    // Reserved 22-31
    VpsNut = 32,
    SpsNut = 33,
    PpsNut = 34,
    AudNut = 35,
    EosNut = 36,
    EobNut = 37,
    FdNut = 38,
    PrefixSeiNut = 39,
    SuffixSeiNut = 40,
    // Reserved 41-47
    Unknown = 255,
}

impl From<u8> for HevcNalType {
    fn from(val: u8) -> Self {
        match val {
            0 => Self::TrailN,
            1 => Self::TrailR,
            2 => Self::TsaN,
            3 => Self::TsaR,
            4 => Self::StsaN,
            5 => Self::StsaR,
            6 => Self::RadlN,
            7 => Self::RadlR,
            8 => Self::RaslN,
            9 => Self::RaslR,
            16 => Self::BlaWLp,
            17 => Self::BlaWRadl,
            18 => Self::BlaNLp,
            19 => Self::IdrWRadl,
            20 => Self::IdrNLp,
            21 => Self::CraNut,
            32 => Self::VpsNut,
            33 => Self::SpsNut,
            34 => Self::PpsNut,
            35 => Self::AudNut,
            36 => Self::EosNut,
            37 => Self::EobNut,
            38 => Self::FdNut,
            39 => Self::PrefixSeiNut,
            40 => Self::SuffixSeiNut,
            _ => Self::Unknown,
        }
    }
}

impl HevcNalType {
    /// Check if this NAL type is a VCL (Video Coding Layer) NAL unit
    pub fn is_vcl(&self) -> bool {
        (*self as u8) <= 31
    }

    /// Check if this NAL type is an IDR frame
    pub fn is_idr(&self) -> bool {
        matches!(self, Self::IdrWRadl | Self::IdrNLp)
    }

    /// Check if this NAL type is a BLA (Broken Link Access) frame
    pub fn is_bla(&self) -> bool {
        matches!(self, Self::BlaWLp | Self::BlaWRadl | Self::BlaNLp)
    }

    /// Check if this NAL type is a CRA (Clean Random Access) frame
    pub fn is_cra(&self) -> bool {
        matches!(self, Self::CraNut)
    }

    /// Check if this is a random access point (IDR, BLA, or CRA)
    pub fn is_rap(&self) -> bool {
        self.is_idr() || self.is_bla() || self.is_cra()
    }

    /// Check if this NAL contains a slice
    pub fn is_slice(&self) -> bool {
        self.is_vcl()
    }
}

/// Parsed HEVC NAL unit
#[derive(Debug, Clone)]
pub struct HevcNalUnit {
    /// NAL unit type
    pub nal_type: HevcNalType,
    /// NAL unit header layer ID (nuh_layer_id)
    pub layer_id: u8,
    /// NAL unit header temporal ID plus 1 (nuh_temporal_id_plus1)
    pub temporal_id: u8,
    /// Raw NAL unit data (without start code, with header)
    pub data: Vec<u8>,
    /// Offset in original stream
    pub offset: usize,
}

/// HEVC Video Parameter Set (VPS)
#[derive(Debug, Clone, Default)]
pub struct HevcVps {
    pub vps_id: u8,
    pub max_layers: u8,
    pub max_sub_layers: u8,
    pub temporal_id_nesting: bool,
    // Raw VPS data for DXVA
    pub raw_data: Vec<u8>,
}

/// HEVC Sequence Parameter Set (SPS)
#[derive(Debug, Clone, Default)]
pub struct HevcSps {
    pub sps_id: u8,
    pub vps_id: u8,
    pub max_sub_layers: u8,
    pub chroma_format_idc: u8,
    pub separate_colour_plane: bool,
    pub pic_width: u32,
    pub pic_height: u32,
    pub bit_depth_luma: u8,
    pub bit_depth_chroma: u8,
    pub log2_max_poc_lsb: u8,
    pub num_short_term_ref_pic_sets: u8,
    pub long_term_ref_pics_present: bool,
    pub num_long_term_ref_pics_sps: u8,
    pub temporal_mvp_enabled: bool,
    pub strong_intra_smoothing_enabled: bool,
    pub scaling_list_enabled: bool,
    pub amp_enabled: bool,
    pub sample_adaptive_offset_enabled: bool,
    pub pcm_enabled: bool,
    pub pcm_sample_bit_depth_luma: u8,
    pub pcm_sample_bit_depth_chroma: u8,
    pub log2_min_pcm_luma_coding_block_size: u8,
    pub log2_diff_max_min_pcm_luma_coding_block_size: u8,
    pub pcm_loop_filter_disabled: bool,
    pub log2_min_luma_coding_block_size: u8,
    pub log2_diff_max_min_luma_coding_block_size: u8,
    pub log2_min_luma_transform_block_size: u8,
    pub log2_diff_max_min_luma_transform_block_size: u8,
    pub max_transform_hierarchy_depth_inter: u8,
    pub max_transform_hierarchy_depth_intra: u8,
    // Raw SPS data for DXVA
    pub raw_data: Vec<u8>,
}

/// HEVC Picture Parameter Set (PPS)
#[derive(Debug, Clone, Default)]
pub struct HevcPps {
    pub pps_id: u8,
    pub sps_id: u8,
    pub dependent_slice_segments_enabled: bool,
    pub output_flag_present: bool,
    pub num_extra_slice_header_bits: u8,
    pub sign_data_hiding_enabled: bool,
    pub cabac_init_present: bool,
    pub num_ref_idx_l0_default_active: u8,
    pub num_ref_idx_l1_default_active: u8,
    pub init_qp: i8,
    pub constrained_intra_pred: bool,
    pub transform_skip_enabled: bool,
    pub cu_qp_delta_enabled: bool,
    pub diff_cu_qp_delta_depth: u8,
    pub cb_qp_offset: i8,
    pub cr_qp_offset: i8,
    pub slice_chroma_qp_offsets_present: bool,
    pub weighted_pred: bool,
    pub weighted_bipred: bool,
    pub transquant_bypass_enabled: bool,
    pub tiles_enabled: bool,
    pub entropy_coding_sync_enabled: bool,
    pub num_tile_columns: u16,
    pub num_tile_rows: u16,
    pub uniform_spacing: bool,
    pub loop_filter_across_tiles_enabled: bool,
    pub loop_filter_across_slices_enabled: bool,
    pub deblocking_filter_control_present: bool,
    pub deblocking_filter_override_enabled: bool,
    pub deblocking_filter_disabled: bool,
    pub beta_offset: i8,
    pub tc_offset: i8,
    pub lists_modification_present: bool,
    pub log2_parallel_merge_level: u8,
    pub slice_segment_header_extension_present: bool,
    // Raw PPS data for DXVA
    pub raw_data: Vec<u8>,
}

/// HEVC Slice Header (partial parse for DXVA)
#[derive(Debug, Clone, Default)]
pub struct HevcSliceHeader {
    pub first_slice_in_pic: bool,
    pub no_output_of_prior_pics: bool,
    pub pps_id: u8,
    pub dependent_slice_segment: bool,
    pub slice_segment_address: u32,
    pub slice_type: u8, // 0=B, 1=P, 2=I
    pub pic_output: bool,
    pub colour_plane_id: u8,
    pub pic_order_cnt_lsb: u16,
    pub short_term_ref_pic_set_sps_flag: bool,
    pub short_term_ref_pic_set_idx: u8,
    // Reference picture list modification
    pub num_ref_idx_l0_active: u8,
    pub num_ref_idx_l1_active: u8,
}

/// Exponential-Golomb bit reader
pub struct BitReader<'a> {
    data: &'a [u8],
    byte_pos: usize,
    bit_pos: u8,
}

impl<'a> BitReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            byte_pos: 0,
            bit_pos: 0,
        }
    }

    /// Read a single bit
    pub fn read_bit(&mut self) -> Result<u8> {
        if self.byte_pos >= self.data.len() {
            return Err(anyhow!("BitReader: end of data"));
        }
        let bit = (self.data[self.byte_pos] >> (7 - self.bit_pos)) & 1;
        self.bit_pos += 1;
        if self.bit_pos >= 8 {
            self.bit_pos = 0;
            self.byte_pos += 1;
        }
        Ok(bit)
    }

    /// Read N bits as unsigned
    pub fn read_bits(&mut self, n: u8) -> Result<u32> {
        let mut val = 0u32;
        for _ in 0..n {
            val = (val << 1) | self.read_bit()? as u32;
        }
        Ok(val)
    }

    /// Read unsigned Exp-Golomb coded value
    pub fn read_ue(&mut self) -> Result<u32> {
        let mut leading_zeros = 0u32;
        while self.read_bit()? == 0 {
            leading_zeros += 1;
            if leading_zeros > 31 {
                return Err(anyhow!("BitReader: invalid Exp-Golomb code"));
            }
        }
        if leading_zeros == 0 {
            return Ok(0);
        }
        let val = self.read_bits(leading_zeros as u8)?;
        Ok((1 << leading_zeros) - 1 + val)
    }

    /// Read signed Exp-Golomb coded value
    pub fn read_se(&mut self) -> Result<i32> {
        let ue = self.read_ue()?;
        let sign = if ue & 1 == 1 { 1 } else { -1 };
        Ok(sign * ((ue + 1) / 2) as i32)
    }

    /// Skip N bits
    pub fn skip_bits(&mut self, n: u32) -> Result<()> {
        for _ in 0..n {
            self.read_bit()?;
        }
        Ok(())
    }

    /// Get current bit position
    pub fn position(&self) -> usize {
        self.byte_pos * 8 + self.bit_pos as usize
    }

    /// Check if more data is available
    pub fn has_more_data(&self) -> bool {
        self.byte_pos < self.data.len()
    }
}

/// HEVC Bitstream Parser
pub struct HevcParser {
    /// Current VPS (up to 16)
    pub vps: [Option<HevcVps>; 16],
    /// Current SPS (up to 16)
    pub sps: [Option<HevcSps>; 16],
    /// Current PPS (up to 64)
    pub pps: Vec<Option<HevcPps>>,
}

impl Default for HevcParser {
    fn default() -> Self {
        Self::new()
    }
}

impl HevcParser {
    pub fn new() -> Self {
        Self {
            vps: Default::default(),
            sps: Default::default(),
            pps: vec![None; 64],
        }
    }

    /// Find all NAL units in a bitstream (Annex B format with start codes)
    pub fn find_nal_units(&self, data: &[u8]) -> Vec<HevcNalUnit> {
        let mut nals = Vec::new();
        let mut i = 0;

        while i < data.len() {
            // Look for start code (0x000001 or 0x00000001)
            if i + 3 <= data.len() && data[i] == 0 && data[i + 1] == 0 {
                let start_code_len;
                let nal_start;

                if data[i + 2] == 1 {
                    // 3-byte start code
                    start_code_len = 3;
                    nal_start = i + 3;
                } else if i + 4 <= data.len() && data[i + 2] == 0 && data[i + 3] == 1 {
                    // 4-byte start code
                    start_code_len = 4;
                    nal_start = i + 4;
                } else {
                    i += 1;
                    continue;
                }

                // Find the end of this NAL unit (next start code or end of data)
                let mut nal_end = data.len();
                for j in nal_start..data.len() - 2 {
                    if data[j] == 0
                        && data[j + 1] == 0
                        && (data[j + 2] == 1
                            || (j + 3 < data.len() && data[j + 2] == 0 && data[j + 3] == 1))
                    {
                        nal_end = j;
                        break;
                    }
                }

                if nal_start < nal_end && nal_end - nal_start >= 2 {
                    // Parse NAL header (2 bytes for HEVC)
                    let header0 = data[nal_start];
                    let header1 = data[nal_start + 1];

                    // forbidden_zero_bit (1) | nal_unit_type (6) | nuh_layer_id (6) | nuh_temporal_id_plus1 (3)
                    let nal_type = HevcNalType::from((header0 >> 1) & 0x3F);
                    let layer_id = ((header0 & 1) << 5) | (header1 >> 3);
                    let temporal_id = header1 & 0x07;

                    nals.push(HevcNalUnit {
                        nal_type,
                        layer_id,
                        temporal_id,
                        data: data[nal_start..nal_end].to_vec(),
                        offset: i,
                    });

                    trace!(
                        "Found NAL: type={:?}, layer={}, temporal={}, size={}",
                        nal_type,
                        layer_id,
                        temporal_id,
                        nal_end - nal_start
                    );
                }

                i = nal_end;
            } else {
                i += 1;
            }
        }

        nals
    }

    /// Remove emulation prevention bytes (0x000003 -> 0x0000)
    fn remove_emulation_prevention(data: &[u8]) -> Vec<u8> {
        let mut result = Vec::with_capacity(data.len());
        let mut i = 0;

        while i < data.len() {
            if i + 2 < data.len() && data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 3 {
                result.push(0);
                result.push(0);
                i += 3; // Skip the 0x03 byte
            } else {
                result.push(data[i]);
                i += 1;
            }
        }

        result
    }

    /// Parse VPS NAL unit
    pub fn parse_vps(&mut self, nal: &HevcNalUnit) -> Result<u8> {
        let rbsp = Self::remove_emulation_prevention(&nal.data[2..]); // Skip NAL header
        let mut reader = BitReader::new(&rbsp);

        let vps_id = reader.read_bits(4)? as u8;
        reader.skip_bits(2)?; // vps_base_layer_internal_flag, vps_base_layer_available_flag
        let max_layers = reader.read_bits(6)? as u8 + 1;
        let max_sub_layers = reader.read_bits(3)? as u8 + 1;
        let temporal_id_nesting = reader.read_bit()? != 0;

        let vps = HevcVps {
            vps_id,
            max_layers,
            max_sub_layers,
            temporal_id_nesting,
            raw_data: nal.data.clone(),
        };

        debug!(
            "Parsed VPS {}: max_layers={}, max_sub_layers={}",
            vps_id, max_layers, max_sub_layers
        );
        self.vps[vps_id as usize] = Some(vps);
        Ok(vps_id)
    }

    /// Parse SPS NAL unit
    pub fn parse_sps(&mut self, nal: &HevcNalUnit) -> Result<u8> {
        let rbsp = Self::remove_emulation_prevention(&nal.data[2..]); // Skip NAL header
        let mut reader = BitReader::new(&rbsp);

        let vps_id = reader.read_bits(4)? as u8;
        let max_sub_layers = reader.read_bits(3)? as u8 + 1;
        let _temporal_id_nesting = reader.read_bit()?;

        // Skip profile_tier_level (complex structure)
        self.skip_profile_tier_level(&mut reader, true, max_sub_layers)?;

        let sps_id = reader.read_ue()? as u8;
        let chroma_format_idc = reader.read_ue()? as u8;

        let separate_colour_plane = if chroma_format_idc == 3 {
            reader.read_bit()? != 0
        } else {
            false
        };

        let pic_width = reader.read_ue()?;
        let pic_height = reader.read_ue()?;

        // conformance_window_flag
        let conformance_window = reader.read_bit()? != 0;
        if conformance_window {
            reader.read_ue()?; // left
            reader.read_ue()?; // right
            reader.read_ue()?; // top
            reader.read_ue()?; // bottom
        }

        let bit_depth_luma = reader.read_ue()? as u8 + 8;
        let bit_depth_chroma = reader.read_ue()? as u8 + 8;
        let log2_max_poc_lsb = reader.read_ue()? as u8 + 4;

        // sub_layer_ordering_info_present_flag
        let sub_layer_ordering = reader.read_bit()? != 0;
        let start = if sub_layer_ordering {
            0
        } else {
            max_sub_layers - 1
        };
        for _ in start..max_sub_layers {
            reader.read_ue()?; // max_dec_pic_buffering
            reader.read_ue()?; // max_num_reorder_pics
            reader.read_ue()?; // max_latency_increase
        }

        let log2_min_luma_coding_block_size = reader.read_ue()? as u8 + 3;
        let log2_diff_max_min_luma_coding_block_size = reader.read_ue()? as u8;
        let log2_min_luma_transform_block_size = reader.read_ue()? as u8 + 2;
        let log2_diff_max_min_luma_transform_block_size = reader.read_ue()? as u8;
        let max_transform_hierarchy_depth_inter = reader.read_ue()? as u8;
        let max_transform_hierarchy_depth_intra = reader.read_ue()? as u8;

        let scaling_list_enabled = reader.read_bit()? != 0;
        if scaling_list_enabled {
            let scaling_list_data_present = reader.read_bit()? != 0;
            if scaling_list_data_present {
                self.skip_scaling_list_data(&mut reader)?;
            }
        }

        let amp_enabled = reader.read_bit()? != 0;
        let sample_adaptive_offset_enabled = reader.read_bit()? != 0;

        let pcm_enabled = reader.read_bit()? != 0;
        let (
            pcm_sample_bit_depth_luma,
            pcm_sample_bit_depth_chroma,
            log2_min_pcm_luma_coding_block_size,
            log2_diff_max_min_pcm_luma_coding_block_size,
            pcm_loop_filter_disabled,
        ) = if pcm_enabled {
            let luma = reader.read_bits(4)? as u8 + 1;
            let chroma = reader.read_bits(4)? as u8 + 1;
            let min = reader.read_ue()? as u8 + 3;
            let diff = reader.read_ue()? as u8;
            let disabled = reader.read_bit()? != 0;
            (luma, chroma, min, diff, disabled)
        } else {
            (0, 0, 0, 0, false)
        };

        let num_short_term_ref_pic_sets = reader.read_ue()? as u8;
        // Skip short term ref pic sets (complex)
        for i in 0..num_short_term_ref_pic_sets {
            self.skip_short_term_ref_pic_set(&mut reader, i, num_short_term_ref_pic_sets)?;
        }

        let long_term_ref_pics_present = reader.read_bit()? != 0;
        let num_long_term_ref_pics_sps = if long_term_ref_pics_present {
            let num = reader.read_ue()? as u8;
            for _ in 0..num {
                reader.skip_bits(log2_max_poc_lsb as u32)?; // lt_ref_pic_poc_lsb_sps
                reader.read_bit()?; // used_by_curr_pic_lt_sps_flag
            }
            num
        } else {
            0
        };

        let temporal_mvp_enabled = reader.read_bit()? != 0;
        let strong_intra_smoothing_enabled = reader.read_bit()? != 0;

        let sps = HevcSps {
            sps_id,
            vps_id,
            max_sub_layers,
            chroma_format_idc,
            separate_colour_plane,
            pic_width,
            pic_height,
            bit_depth_luma,
            bit_depth_chroma,
            log2_max_poc_lsb,
            num_short_term_ref_pic_sets,
            long_term_ref_pics_present,
            num_long_term_ref_pics_sps,
            temporal_mvp_enabled,
            strong_intra_smoothing_enabled,
            scaling_list_enabled,
            amp_enabled,
            sample_adaptive_offset_enabled,
            pcm_enabled,
            pcm_sample_bit_depth_luma,
            pcm_sample_bit_depth_chroma,
            log2_min_pcm_luma_coding_block_size,
            log2_diff_max_min_pcm_luma_coding_block_size,
            pcm_loop_filter_disabled,
            log2_min_luma_coding_block_size,
            log2_diff_max_min_luma_coding_block_size,
            log2_min_luma_transform_block_size,
            log2_diff_max_min_luma_transform_block_size,
            max_transform_hierarchy_depth_inter,
            max_transform_hierarchy_depth_intra,
            raw_data: nal.data.clone(),
        };

        debug!(
            "Parsed SPS {}: {}x{}, bit_depth={}/{}",
            sps_id, pic_width, pic_height, bit_depth_luma, bit_depth_chroma
        );
        self.sps[sps_id as usize] = Some(sps);
        Ok(sps_id)
    }

    /// Parse PPS NAL unit
    pub fn parse_pps(&mut self, nal: &HevcNalUnit) -> Result<u8> {
        let rbsp = Self::remove_emulation_prevention(&nal.data[2..]); // Skip NAL header
        let mut reader = BitReader::new(&rbsp);

        let pps_id = reader.read_ue()? as u8;
        let sps_id = reader.read_ue()? as u8;

        let dependent_slice_segments_enabled = reader.read_bit()? != 0;
        let output_flag_present = reader.read_bit()? != 0;
        let num_extra_slice_header_bits = reader.read_bits(3)? as u8;
        let sign_data_hiding_enabled = reader.read_bit()? != 0;
        let cabac_init_present = reader.read_bit()? != 0;

        let num_ref_idx_l0_default_active = reader.read_ue()? as u8 + 1;
        let num_ref_idx_l1_default_active = reader.read_ue()? as u8 + 1;
        let init_qp = reader.read_se()? as i8 + 26;

        let constrained_intra_pred = reader.read_bit()? != 0;
        let transform_skip_enabled = reader.read_bit()? != 0;

        let cu_qp_delta_enabled = reader.read_bit()? != 0;
        let diff_cu_qp_delta_depth = if cu_qp_delta_enabled {
            reader.read_ue()? as u8
        } else {
            0
        };

        let cb_qp_offset = reader.read_se()? as i8;
        let cr_qp_offset = reader.read_se()? as i8;

        let slice_chroma_qp_offsets_present = reader.read_bit()? != 0;
        let weighted_pred = reader.read_bit()? != 0;
        let weighted_bipred = reader.read_bit()? != 0;
        let transquant_bypass_enabled = reader.read_bit()? != 0;
        let tiles_enabled = reader.read_bit()? != 0;
        let entropy_coding_sync_enabled = reader.read_bit()? != 0;

        let (num_tile_columns, num_tile_rows, uniform_spacing, loop_filter_across_tiles_enabled) =
            if tiles_enabled {
                let cols = reader.read_ue()? as u16 + 1;
                let rows = reader.read_ue()? as u16 + 1;
                let uniform = reader.read_bit()? != 0;
                if !uniform {
                    for _ in 0..cols - 1 {
                        reader.read_ue()?;
                    }
                    for _ in 0..rows - 1 {
                        reader.read_ue()?;
                    }
                }
                let across = reader.read_bit()? != 0;
                (cols, rows, uniform, across)
            } else {
                (1, 1, true, false)
            };

        let loop_filter_across_slices_enabled = reader.read_bit()? != 0;

        let deblocking_filter_control_present = reader.read_bit()? != 0;
        let (
            deblocking_filter_override_enabled,
            deblocking_filter_disabled,
            beta_offset,
            tc_offset,
        ) = if deblocking_filter_control_present {
            let override_enabled = reader.read_bit()? != 0;
            let disabled = reader.read_bit()? != 0;
            let (beta, tc) = if !disabled {
                (reader.read_se()? as i8 * 2, reader.read_se()? as i8 * 2)
            } else {
                (0, 0)
            };
            (override_enabled, disabled, beta, tc)
        } else {
            (false, false, 0, 0)
        };

        // scaling_list_data_present_flag
        if self.sps[sps_id as usize]
            .as_ref()
            .map_or(false, |s| s.scaling_list_enabled)
        {
            let scaling_list_data_present = reader.read_bit()? != 0;
            if scaling_list_data_present {
                self.skip_scaling_list_data(&mut reader)?;
            }
        }

        let lists_modification_present = reader.read_bit()? != 0;
        let log2_parallel_merge_level = reader.read_ue()? as u8 + 2;
        let slice_segment_header_extension_present = reader.read_bit()? != 0;

        let pps = HevcPps {
            pps_id,
            sps_id,
            dependent_slice_segments_enabled,
            output_flag_present,
            num_extra_slice_header_bits,
            sign_data_hiding_enabled,
            cabac_init_present,
            num_ref_idx_l0_default_active,
            num_ref_idx_l1_default_active,
            init_qp,
            constrained_intra_pred,
            transform_skip_enabled,
            cu_qp_delta_enabled,
            diff_cu_qp_delta_depth,
            cb_qp_offset,
            cr_qp_offset,
            slice_chroma_qp_offsets_present,
            weighted_pred,
            weighted_bipred,
            transquant_bypass_enabled,
            tiles_enabled,
            entropy_coding_sync_enabled,
            num_tile_columns,
            num_tile_rows,
            uniform_spacing,
            loop_filter_across_tiles_enabled,
            loop_filter_across_slices_enabled,
            deblocking_filter_control_present,
            deblocking_filter_override_enabled,
            deblocking_filter_disabled,
            beta_offset,
            tc_offset,
            lists_modification_present,
            log2_parallel_merge_level,
            slice_segment_header_extension_present,
            raw_data: nal.data.clone(),
        };

        debug!(
            "Parsed PPS {}: sps={}, tiles={}x{}",
            pps_id, sps_id, num_tile_columns, num_tile_rows
        );
        self.pps[pps_id as usize] = Some(pps);
        Ok(pps_id)
    }

    /// Parse slice header (partial, for DXVA)
    pub fn parse_slice_header(&self, nal: &HevcNalUnit) -> Result<HevcSliceHeader> {
        let rbsp = Self::remove_emulation_prevention(&nal.data[2..]); // Skip NAL header
        let mut reader = BitReader::new(&rbsp);

        let first_slice_in_pic = reader.read_bit()? != 0;

        let no_output_of_prior_pics = if nal.nal_type.is_rap() {
            reader.read_bit()? != 0
        } else {
            false
        };

        let pps_id = reader.read_ue()? as u8;
        let pps = self.pps[pps_id as usize]
            .as_ref()
            .ok_or_else(|| anyhow!("PPS {} not found", pps_id))?;
        let sps = self.sps[pps.sps_id as usize]
            .as_ref()
            .ok_or_else(|| anyhow!("SPS {} not found", pps.sps_id))?;

        let mut header = HevcSliceHeader {
            first_slice_in_pic,
            no_output_of_prior_pics,
            pps_id,
            ..Default::default()
        };

        if !first_slice_in_pic {
            if pps.dependent_slice_segments_enabled {
                header.dependent_slice_segment = reader.read_bit()? != 0;
            }
            // Calculate slice_segment_address bits
            let ctb_size = 1u32
                << (sps.log2_min_luma_coding_block_size
                    + sps.log2_diff_max_min_luma_coding_block_size);
            let pic_width_in_ctb = (sps.pic_width + ctb_size - 1) / ctb_size;
            let pic_height_in_ctb = (sps.pic_height + ctb_size - 1) / ctb_size;
            let num_ctb = pic_width_in_ctb * pic_height_in_ctb;
            let address_bits = (32 - num_ctb.leading_zeros()) as u8;
            header.slice_segment_address = reader.read_bits(address_bits)?;
        }

        if !header.dependent_slice_segment {
            // Skip extra slice header bits
            reader.skip_bits(pps.num_extra_slice_header_bits as u32)?;

            header.slice_type = reader.read_ue()? as u8;

            if pps.output_flag_present {
                header.pic_output = reader.read_bit()? != 0;
            } else {
                header.pic_output = true;
            }

            if sps.separate_colour_plane {
                header.colour_plane_id = reader.read_bits(2)? as u8;
            }

            if !nal.nal_type.is_idr() {
                header.pic_order_cnt_lsb = reader.read_bits(sps.log2_max_poc_lsb as u8)? as u16;
                header.short_term_ref_pic_set_sps_flag = reader.read_bit()? != 0;
                // Additional ref pic set parsing would go here
            }

            // Default ref idx counts from PPS
            header.num_ref_idx_l0_active = pps.num_ref_idx_l0_default_active;
            header.num_ref_idx_l1_active = pps.num_ref_idx_l1_default_active;
        }

        Ok(header)
    }

    /// Process a NAL unit (parse if parameter set, return slice info if slice)
    pub fn process_nal(&mut self, nal: &HevcNalUnit) -> Result<()> {
        match nal.nal_type {
            HevcNalType::VpsNut => {
                self.parse_vps(nal)?;
            }
            HevcNalType::SpsNut => {
                self.parse_sps(nal)?;
            }
            HevcNalType::PpsNut => {
                self.parse_pps(nal)?;
            }
            _ => {}
        }
        Ok(())
    }

    // Helper functions for skipping complex structures

    fn skip_profile_tier_level(
        &self,
        reader: &mut BitReader,
        profile_present: bool,
        max_sub_layers: u8,
    ) -> Result<()> {
        if profile_present {
            // general_profile_space (2) + general_tier_flag (1) + general_profile_idc (5) = 8 bits
            // general_profile_compatibility_flag[32] = 32 bits
            // general_progressive_source_flag + general_interlaced_source_flag +
            // general_non_packed_constraint_flag + general_frame_only_constraint_flag = 4 bits
            // general_max_12bit_constraint_flag...general_reserved_zero_34bits = 44 bits
            // Total constraint flags = 48 bits
            // general_level_idc = 8 bits
            // Total = 2 + 1 + 5 + 32 + 48 + 8 = 96 bits
            reader.skip_bits(96)?;
        }

        // sub_layer_profile_present_flag and sub_layer_level_present_flag
        let mut sub_layer_profile_present = vec![false; 8];
        let mut sub_layer_level_present = vec![false; 8];

        for i in 0..(max_sub_layers - 1) as usize {
            sub_layer_profile_present[i] = reader.read_bit()? != 0;
            sub_layer_level_present[i] = reader.read_bit()? != 0;
        }

        // Reserved bits when max_sub_layers - 1 < 8
        if max_sub_layers > 1 && max_sub_layers < 8 {
            for _ in (max_sub_layers - 1)..8 {
                reader.skip_bits(2)?; // reserved_zero_2bits
            }
        }

        // Sub-layer profile/level info
        for i in 0..(max_sub_layers - 1) as usize {
            if sub_layer_profile_present[i] {
                // sub_layer profile: 2 + 1 + 5 + 32 + 48 = 88 bits
                reader.skip_bits(88)?;
            }
            if sub_layer_level_present[i] {
                reader.skip_bits(8)?; // sub_layer_level_idc
            }
        }

        Ok(())
    }

    fn skip_scaling_list_data(&self, reader: &mut BitReader) -> Result<()> {
        for size_id in 0..4 {
            let num_matrix = if size_id == 3 { 2 } else { 6 };
            for matrix_id in 0..num_matrix {
                let scaling_list_pred_mode = reader.read_bit()? != 0;
                if !scaling_list_pred_mode {
                    reader.read_ue()?; // delta
                } else {
                    let coef_num = std::cmp::min(64, 1 << (4 + (size_id << 1)));
                    if size_id > 1 {
                        reader.read_se()?; // dc coef
                    }
                    for _ in 0..coef_num {
                        reader.read_se()?;
                    }
                }
                let _ = matrix_id; // suppress warning
            }
        }
        Ok(())
    }

    fn skip_short_term_ref_pic_set(
        &self,
        reader: &mut BitReader,
        idx: u8,
        num_sets: u8,
    ) -> Result<()> {
        let inter_ref_pic_set_prediction = if idx > 0 {
            reader.read_bit()? != 0
        } else {
            false
        };

        if inter_ref_pic_set_prediction {
            if idx == num_sets {
                reader.read_ue()?; // delta_idx
            }
            reader.read_bit()?; // delta_rps_sign
            reader.read_ue()?; // abs_delta_rps
                               // Would need previous ref pic set info to know how many to skip
                               // For now, we'll rely on the parsing stopping before this gets complex
        } else {
            let num_negative = reader.read_ue()?;
            let num_positive = reader.read_ue()?;
            for _ in 0..num_negative {
                reader.read_ue()?; // delta_poc
                reader.read_bit()?; // used
            }
            for _ in 0..num_positive {
                reader.read_ue()?; // delta_poc
                reader.read_bit()?; // used
            }
        }
        Ok(())
    }

    /// Get active SPS for a PPS
    pub fn get_sps_for_pps(&self, pps_id: u8) -> Option<&HevcSps> {
        let pps = self.pps[pps_id as usize].as_ref()?;
        self.sps[pps.sps_id as usize].as_ref()
    }

    /// Get active VPS for an SPS
    pub fn get_vps_for_sps(&self, sps_id: u8) -> Option<&HevcVps> {
        let sps = self.sps[sps_id as usize].as_ref()?;
        self.vps[sps.vps_id as usize].as_ref()
    }

    /// Get video dimensions and HDR status from the first available SPS
    /// Returns (width, height, is_hdr) or None if no SPS is available
    pub fn get_dimensions(&self) -> Option<(u32, u32, bool)> {
        // Find the first valid SPS
        for sps in self.sps.iter().flatten() {
            let width = sps.pic_width;
            let height = sps.pic_height;
            // HDR is indicated by 10-bit depth (Main10 profile)
            let is_hdr = sps.bit_depth_luma > 8 || sps.bit_depth_chroma > 8;
            return Some((width, height, is_hdr));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nal_type_parsing() {
        assert_eq!(HevcNalType::from(32), HevcNalType::VpsNut);
        assert_eq!(HevcNalType::from(33), HevcNalType::SpsNut);
        assert_eq!(HevcNalType::from(34), HevcNalType::PpsNut);
        assert_eq!(HevcNalType::from(19), HevcNalType::IdrWRadl);
        assert!(HevcNalType::IdrWRadl.is_idr());
        assert!(HevcNalType::IdrWRadl.is_rap());
    }

    #[test]
    fn test_bit_reader() {
        let data = [0b10110100, 0b01100000];
        let mut reader = BitReader::new(&data);

        assert_eq!(reader.read_bit().unwrap(), 1);
        assert_eq!(reader.read_bit().unwrap(), 0);
        assert_eq!(reader.read_bits(4).unwrap(), 0b1101);
    }

    #[test]
    fn test_exp_golomb() {
        // 1 -> 0 (single 1 bit = 0)
        let data = [0b10000000];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_ue().unwrap(), 0);

        // 010 -> 1
        let data = [0b01000000];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_ue().unwrap(), 1);

        // 011 -> 2
        let data = [0b01100000];
        let mut reader = BitReader::new(&data);
        assert_eq!(reader.read_ue().unwrap(), 2);
    }
}
