//! RTP Depacketizer
//!
//! Depacketizes RTP payloads for H.264, H.265/HEVC, and AV1 video codecs.

use log::{debug, info, warn};

/// Codec type for depacketizer
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DepacketizerCodec {
    H264,
    H265,
    AV1,
}

/// RTP depacketizer supporting H.264, H.265/HEVC, and AV1
pub struct RtpDepacketizer {
    codec: DepacketizerCodec,
    buffer: Vec<u8>,
    fragments: Vec<Vec<u8>>,
    in_fragment: bool,
    /// Cached VPS NAL unit (H.265 only)
    vps: Option<Vec<u8>>,
    /// Cached SPS NAL unit
    sps: Option<Vec<u8>>,
    /// Cached PPS NAL unit
    pps: Option<Vec<u8>>,
    /// Accumulated OBUs for current AV1 frame (sent when marker bit is set)
    av1_frame_buffer: Vec<u8>,
    /// Cached AV1 SEQUENCE_HEADER OBU - must be present at start of each frame
    av1_sequence_header: Option<Vec<u8>>,
}

impl RtpDepacketizer {
    pub fn new() -> Self {
        Self::with_codec(DepacketizerCodec::H264)
    }

    pub fn with_codec(codec: DepacketizerCodec) -> Self {
        Self {
            codec,
            buffer: Vec::with_capacity(64 * 1024),
            fragments: Vec::new(),
            in_fragment: false,
            vps: None,
            sps: None,
            pps: None,
            av1_frame_buffer: Vec::with_capacity(256 * 1024),
            av1_sequence_header: None,
        }
    }

    /// Set the codec type
    pub fn set_codec(&mut self, codec: DepacketizerCodec) {
        self.codec = codec;
        // Clear cached parameter sets when codec changes
        self.vps = None;
        self.sps = None;
        self.pps = None;
        self.buffer.clear();
        self.in_fragment = false;
        self.av1_frame_buffer.clear();
        self.av1_sequence_header = None;
    }

    /// Accumulate an OBU for the current AV1 frame
    /// Call take_accumulated_frame() when marker bit is set to get complete frame
    pub fn accumulate_obu(&mut self, obu: Vec<u8>) {
        self.av1_frame_buffer.extend_from_slice(&obu);
    }

    /// Take the accumulated AV1 frame data (all OBUs concatenated)
    /// Returns None if no data accumulated or if frame doesn't contain picture data
    pub fn take_accumulated_frame(&mut self) -> Option<Vec<u8>> {
        if self.av1_frame_buffer.is_empty() {
            return None;
        }
        let mut frame = std::mem::take(&mut self.av1_frame_buffer);
        // Pre-allocate for next frame
        self.av1_frame_buffer = Vec::with_capacity(256 * 1024);

        // Validate that frame contains actual picture data (TILE_GROUP or FRAME OBU)
        // Without this, we'd send headers-only to decoder which can crash CUVID
        if !Self::av1_frame_has_picture_data(&frame) {
            // But still extract and cache SEQUENCE_HEADER if present
            if let Some(seq_hdr) = Self::extract_sequence_header(&frame) {
                info!("AV1: Caching SEQUENCE_HEADER ({} bytes) from header-only packet", seq_hdr.len());
                self.av1_sequence_header = Some(seq_hdr);
            }
            static SKIPPED_LOGGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let logged = SKIPPED_LOGGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if logged < 5 {
                warn!("AV1: Skipping frame without picture data ({} bytes)", frame.len());
            }
            return None;
        }

        // Check if frame already has a SEQUENCE_HEADER
        let has_sequence_header = Self::av1_frame_has_sequence_header(&frame);

        // If frame has SEQUENCE_HEADER, cache it for future frames
        if has_sequence_header {
            if let Some(seq_hdr) = Self::extract_sequence_header(&frame) {
                if self.av1_sequence_header.is_none() {
                    info!("AV1: Caching SEQUENCE_HEADER ({} bytes)", seq_hdr.len());
                }
                self.av1_sequence_header = Some(seq_hdr);
            }
        } else if let Some(ref seq_hdr) = self.av1_sequence_header {
            // Prepend cached SEQUENCE_HEADER to frame
            static PREPEND_LOGGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let logged = PREPEND_LOGGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if logged < 5 {
                info!("AV1: Prepending cached SEQUENCE_HEADER ({} bytes) to frame", seq_hdr.len());
            }
            let mut new_frame = Vec::with_capacity(seq_hdr.len() + frame.len());
            new_frame.extend_from_slice(seq_hdr);
            new_frame.extend_from_slice(&frame);
            frame = new_frame;
        }

        static FRAMES_LOGGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let logged = FRAMES_LOGGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if logged < 10 {
            info!("AV1: Sending complete frame to decoder: {} bytes (has_seq_hdr={})",
                  frame.len(), has_sequence_header || self.av1_sequence_header.is_some());
        }

        Some(frame)
    }

    /// Check if an AV1 frame contains actual picture data (TILE_GROUP or FRAME OBU)
    /// Frames with only SEQUENCE_HEADER, FRAME_HEADER, etc. are not decodable
    fn av1_frame_has_picture_data(data: &[u8]) -> bool {
        Self::av1_find_obu_types(data).iter().any(|&t| t == 4 || t == 6)
    }

    /// Check if an AV1 frame contains a SEQUENCE_HEADER OBU
    fn av1_frame_has_sequence_header(data: &[u8]) -> bool {
        Self::av1_find_obu_types(data).contains(&1)
    }

    /// Find all OBU types in an AV1 bitstream
    fn av1_find_obu_types(data: &[u8]) -> Vec<u8> {
        let mut types = Vec::new();
        let mut offset = 0;

        while offset < data.len() {
            // Parse OBU header
            let header = data[offset];
            let obu_type = (header >> 3) & 0x0F;
            let has_extension = (header & 0x04) != 0;
            let has_size = (header & 0x02) != 0;

            types.push(obu_type);

            // Move to next OBU
            let header_size = if has_extension { 2 } else { 1 };
            offset += header_size;

            if has_size && offset < data.len() {
                let (size, bytes_read) = Self::read_leb128(&data[offset..]);
                offset += bytes_read + size as usize;
            } else {
                // No size field - OBU extends to end of data
                break;
            }
        }
        types
    }

    /// Extract the SEQUENCE_HEADER OBU from an AV1 bitstream
    fn extract_sequence_header(data: &[u8]) -> Option<Vec<u8>> {
        let mut offset = 0;

        while offset < data.len() {
            let start_offset = offset;

            // Parse OBU header
            let header = data[offset];
            let obu_type = (header >> 3) & 0x0F;
            let has_extension = (header & 0x04) != 0;
            let has_size = (header & 0x02) != 0;

            // Move past header
            let header_size = if has_extension { 2 } else { 1 };
            offset += header_size;

            if has_size && offset < data.len() {
                let (size, bytes_read) = Self::read_leb128(&data[offset..]);
                offset += bytes_read;

                // If this is SEQUENCE_HEADER (type 1), extract it
                if obu_type == 1 {
                    let end_offset = offset + size as usize;
                    if end_offset <= data.len() {
                        return Some(data[start_offset..end_offset].to_vec());
                    }
                }

                offset += size as usize;
            } else {
                // No size field - OBU extends to end of data
                if obu_type == 1 {
                    return Some(data[start_offset..].to_vec());
                }
                break;
            }
        }
        None
    }

    /// Process an RTP payload and return complete NAL units (or OBUs for AV1)
    pub fn process(&mut self, payload: &[u8]) -> Vec<Vec<u8>> {
        match self.codec {
            DepacketizerCodec::H264 => self.process_h264(payload),
            DepacketizerCodec::H265 => self.process_h265(payload),
            DepacketizerCodec::AV1 => self.process_av1(payload),
        }
    }

    /// Process H.264 RTP payload
    fn process_h264(&mut self, payload: &[u8]) -> Vec<Vec<u8>> {
        let mut result = Vec::new();

        if payload.is_empty() {
            return result;
        }

        let nal_type = payload[0] & 0x1F;

        match nal_type {
            // Single NAL unit (1-23)
            1..=23 => {
                // Cache SPS/PPS for later use
                if nal_type == 7 {
                    debug!("H264: Caching SPS ({} bytes)", payload.len());
                    self.sps = Some(payload.to_vec());
                } else if nal_type == 8 {
                    debug!("H264: Caching PPS ({} bytes)", payload.len());
                    self.pps = Some(payload.to_vec());
                }
                result.push(payload.to_vec());
            }

            // STAP-A (24) - Single-time aggregation packet
            24 => {
                let mut offset = 1;
                debug!("H264 STAP-A packet: {} bytes total", payload.len());

                while offset + 2 <= payload.len() {
                    let size = u16::from_be_bytes([payload[offset], payload[offset + 1]]) as usize;
                    offset += 2;

                    if offset + size > payload.len() {
                        warn!("H264 STAP-A: invalid size {} at offset {}", size, offset);
                        break;
                    }

                    let nal_data = payload[offset..offset + size].to_vec();
                    let inner_nal_type = nal_data.first().map(|b| b & 0x1F).unwrap_or(0);

                    // Cache SPS/PPS
                    if inner_nal_type == 7 {
                        self.sps = Some(nal_data.clone());
                    } else if inner_nal_type == 8 {
                        self.pps = Some(nal_data.clone());
                    }

                    result.push(nal_data);
                    offset += size;
                }
            }

            // FU-A (28) - Fragmentation unit
            28 => {
                if payload.len() < 2 {
                    return result;
                }

                let fu_header = payload[1];
                let start = (fu_header & 0x80) != 0;
                let end = (fu_header & 0x40) != 0;
                let inner_nal_type = fu_header & 0x1F;

                if start {
                    self.buffer.clear();
                    self.in_fragment = true;
                    let nal_header = (payload[0] & 0xE0) | inner_nal_type;
                    self.buffer.push(nal_header);
                    self.buffer.extend_from_slice(&payload[2..]);
                } else if self.in_fragment {
                    self.buffer.extend_from_slice(&payload[2..]);
                }

                if end && self.in_fragment {
                    self.in_fragment = false;
                    let inner_nal_type = self.buffer.first().map(|b| b & 0x1F).unwrap_or(0);

                    // For IDR frames, prepend SPS/PPS
                    if inner_nal_type == 5 {
                        if let (Some(sps), Some(pps)) = (&self.sps, &self.pps) {
                            result.push(sps.clone());
                            result.push(pps.clone());
                        }
                    }

                    result.push(self.buffer.clone());
                }
            }

            _ => {
                debug!("H264: Unknown NAL type: {}", nal_type);
            }
        }

        result
    }

    /// Process H.265/HEVC RTP payload (RFC 7798)
    fn process_h265(&mut self, payload: &[u8]) -> Vec<Vec<u8>> {
        let mut result = Vec::new();

        if payload.len() < 2 {
            return result;
        }

        // H.265 NAL unit header is 2 bytes
        // Type is in bits 1-6 of first byte: (byte0 >> 1) & 0x3F
        let nal_type = (payload[0] >> 1) & 0x3F;

        match nal_type {
            // Single NAL unit (0-47, but 48 and 49 are special)
            0..=47 => {
                // Cache VPS/SPS/PPS for later use
                match nal_type {
                    32 => {
                        debug!("H265: Caching VPS ({} bytes)", payload.len());
                        self.vps = Some(payload.to_vec());
                    }
                    33 => {
                        debug!("H265: Caching SPS ({} bytes)", payload.len());
                        self.sps = Some(payload.to_vec());
                    }
                    34 => {
                        debug!("H265: Caching PPS ({} bytes)", payload.len());
                        self.pps = Some(payload.to_vec());
                    }
                    _ => {}
                }
                result.push(payload.to_vec());
            }

            // AP (48) - Aggregation Packet
            48 => {
                let mut offset = 2; // Skip the 2-byte NAL unit header
                debug!("H265 AP packet: {} bytes total", payload.len());

                while offset + 2 <= payload.len() {
                    let size = u16::from_be_bytes([payload[offset], payload[offset + 1]]) as usize;
                    offset += 2;

                    if offset + size > payload.len() {
                        warn!("H265 AP: invalid size {} at offset {}", size, offset);
                        break;
                    }

                    let nal_data = payload[offset..offset + size].to_vec();

                    if nal_data.len() >= 2 {
                        let inner_nal_type = (nal_data[0] >> 1) & 0x3F;
                        // Cache VPS/SPS/PPS
                        match inner_nal_type {
                            32 => self.vps = Some(nal_data.clone()),
                            33 => self.sps = Some(nal_data.clone()),
                            34 => self.pps = Some(nal_data.clone()),
                            _ => {}
                        }
                    }

                    result.push(nal_data);
                    offset += size;
                }
            }

            // FU (49) - Fragmentation Unit
            49 => {
                if payload.len() < 3 {
                    return result;
                }

                // FU header is at byte 2
                let fu_header = payload[2];
                let start = (fu_header & 0x80) != 0;
                let end = (fu_header & 0x40) != 0;
                let inner_nal_type = fu_header & 0x3F;

                if start {
                    self.buffer.clear();
                    self.in_fragment = true;

                    // Reconstruct NAL unit header from original header + inner type
                    // H265 NAL header: forbidden_zero_bit(1) | nal_unit_type(6) | nuh_layer_id(6) | nuh_temporal_id_plus1(3)
                    // First byte: (forbidden_zero_bit << 7) | (inner_nal_type << 1) | (layer_id >> 5)
                    // Second byte: (layer_id << 3) | temporal_id
                    let layer_id = payload[0] & 0x01; // lowest bit of first byte
                    let temporal_id = payload[1]; // second byte

                    let nal_header_byte0 = (inner_nal_type << 1) | layer_id;
                    let nal_header_byte1 = temporal_id;

                    self.buffer.push(nal_header_byte0);
                    self.buffer.push(nal_header_byte1);
                    self.buffer.extend_from_slice(&payload[3..]);
                } else if self.in_fragment {
                    self.buffer.extend_from_slice(&payload[3..]);
                }

                if end && self.in_fragment {
                    self.in_fragment = false;

                    if self.buffer.len() >= 2 {
                        let inner_nal_type = (self.buffer[0] >> 1) & 0x3F;

                        // For IDR frames (types 19 and 20), prepend VPS/SPS/PPS
                        if inner_nal_type == 19 || inner_nal_type == 20 {
                            if let Some(vps) = &self.vps {
                                result.push(vps.clone());
                            }
                            if let Some(sps) = &self.sps {
                                result.push(sps.clone());
                            }
                            if let Some(pps) = &self.pps {
                                result.push(pps.clone());
                            }
                        }
                    }

                    result.push(self.buffer.clone());
                }
            }

            _ => {
                debug!("H265: Unknown NAL type: {}", nal_type);
            }
        }

        result
    }

    /// Process AV1 RTP payload (RFC 9000 - RTP Payload Format for AV1)
    /// AV1 uses OBUs (Open Bitstream Units) instead of NAL units
    ///
    /// The RTP payload format omits the obu_size field from OBU headers.
    /// We need to reconstruct full OBUs with size fields for the decoder.
    fn process_av1(&mut self, payload: &[u8]) -> Vec<Vec<u8>> {
        let mut result = Vec::new();

        if payload.is_empty() {
            return result;
        }

        // AV1 aggregation header (first byte)
        // Z (1 bit): Continuation of previous OBU fragment
        // Y (1 bit): Last OBU fragment (or complete OBU)
        // W (2 bits): Number of OBU elements (0=1 OBU, 1-3=W OBUs with sizes)
        // N (1 bit): First packet of a coded video sequence
        // Reserved (3 bits)
        let agg_header = payload[0];
        let z_flag = (agg_header & 0x80) != 0; // continuation
        let y_flag = (agg_header & 0x40) != 0; // last fragment / complete
        let w_field = (agg_header >> 4) & 0x03;
        let n_flag = (agg_header & 0x08) != 0; // new coded video sequence

        // Log first few packets to debug
        static PACKETS_LOGGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let logged = PACKETS_LOGGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if logged < 20 {
            info!("AV1 RTP: len={} Z={} Y={} W={} N={} header=0x{:02x}",
                  payload.len(), z_flag as u8, y_flag as u8, w_field, n_flag as u8, agg_header);
        }

        if n_flag {
            info!("AV1: New coded video sequence");
            // Clear any pending fragments and collected OBUs
            self.buffer.clear();
            self.fragments.clear();
            self.in_fragment = false;
        }

        let mut offset = 1;

        // Handle fragmented OBU (Z=1 means this is a continuation)
        if z_flag {
            if self.in_fragment {
                self.buffer.extend_from_slice(&payload[offset..]);
                if y_flag {
                    // Last fragment - reconstruct full OBU with size field
                    self.in_fragment = false;
                    if let Some(obu) = Self::reconstruct_obu_with_size(&self.buffer) {
                        result.push(obu);
                    }
                    self.buffer.clear();
                }
            }
            return result;
        }

        // Not a continuation - parse new OBU elements
        // W=0: single OBU element occupying the rest of the packet
        // W=1,2,3: W OBU elements, each preceded by LEB128 size (except last)
        let obu_count = if w_field == 0 { 1 } else { w_field as usize };

        for i in 0..obu_count {
            if offset >= payload.len() {
                break;
            }

            // Read OBU size (LEB128) for all but the last element when W > 0
            let obu_size = if w_field > 0 && i < obu_count - 1 {
                let (size, bytes_read) = Self::read_leb128(&payload[offset..]);
                offset += bytes_read;
                size as usize
            } else {
                // Last OBU (or only OBU when W=0) takes remaining bytes
                payload.len() - offset
            };

            if offset + obu_size > payload.len() {
                warn!("AV1: Invalid OBU size {} at offset {} (payload len {})",
                      obu_size, offset, payload.len());
                break;
            }

            let obu_data = &payload[offset..offset + obu_size];

            // Check if this starts a fragmented OBU
            // Y=0 on the last OBU means it's fragmented across packets
            if i == obu_count - 1 && !y_flag {
                self.buffer.clear();
                self.buffer.extend_from_slice(obu_data);
                self.in_fragment = true;
            } else if !obu_data.is_empty() {
                // Complete OBU - reconstruct with proper size field
                if let Some(obu) = Self::reconstruct_obu_with_size(obu_data) {
                    result.push(obu);
                }
            }

            offset += obu_size;
        }

        result
    }

    /// Reconstruct an OBU with the obu_size field included
    /// RTP format strips the size field, but decoders need it
    fn reconstruct_obu_with_size(obu_data: &[u8]) -> Option<Vec<u8>> {
        if obu_data.is_empty() {
            return None;
        }

        // Parse OBU header
        let header = obu_data[0];
        let obu_type = (header >> 3) & 0x0F;
        let has_extension = (header & 0x04) != 0;
        let has_size_field = (header & 0x02) != 0;

        // If it already has a size field, return as-is
        if has_size_field {
            return Some(obu_data.to_vec());
        }

        // Calculate payload size (everything after header and optional extension)
        let header_size = if has_extension { 2 } else { 1 };
        if obu_data.len() < header_size {
            return None;
        }

        let payload_size = obu_data.len() - header_size;

        // Build new OBU with size field
        let mut new_obu = Vec::with_capacity(obu_data.len() + 8);

        // Modified header with has_size_field = 1
        new_obu.push(header | 0x02);

        // Copy extension byte if present
        if has_extension && obu_data.len() > 1 {
            new_obu.push(obu_data[1]);
        }

        // Write payload size as LEB128
        Self::write_leb128(&mut new_obu, payload_size as u64);

        // Copy payload
        new_obu.extend_from_slice(&obu_data[header_size..]);

        // Log OBU types for debugging
        let obu_type_name = match obu_type {
            1 => "SEQUENCE_HEADER",
            2 => "TEMPORAL_DELIMITER",
            3 => "FRAME_HEADER",
            4 => "TILE_GROUP",
            5 => "METADATA",
            6 => "FRAME",
            7 => "REDUNDANT_FRAME_HEADER",
            8 => "TILE_LIST",
            15 => "PADDING",
            _ => "UNKNOWN",
        };

        // Log first few OBUs at info level
        static OBUS_LOGGED: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let logged = OBUS_LOGGED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if logged < 30 {
            info!("AV1: Reconstructed OBU type {} ({}) payload_size={} total_size={}",
                  obu_type, obu_type_name, payload_size, new_obu.len());
        }

        Some(new_obu)
    }

    /// Read LEB128 encoded unsigned integer
    fn read_leb128(data: &[u8]) -> (u64, usize) {
        let mut value: u64 = 0;
        let mut bytes_read = 0;

        for (i, &byte) in data.iter().enumerate().take(8) {
            value |= ((byte & 0x7F) as u64) << (i * 7);
            bytes_read = i + 1;
            if (byte & 0x80) == 0 {
                break;
            }
        }

        (value, bytes_read)
    }

    /// Write LEB128 encoded unsigned integer
    fn write_leb128(output: &mut Vec<u8>, mut value: u64) {
        loop {
            let mut byte = (value & 0x7F) as u8;
            value >>= 7;
            if value != 0 {
                byte |= 0x80; // More bytes follow
            }
            output.push(byte);
            if value == 0 {
                break;
            }
        }
    }
}

impl Default for RtpDepacketizer {
    fn default() -> Self {
        Self::new()
    }
}
