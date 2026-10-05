import Foundation

/// AV1 open bitstream units (OBUs) on the GFN cloud NVST wire.
///
/// Unlike the H.264/HEVC paths there is no Annex-B framing: a reassembled access unit is a
/// contiguous low-overhead OBU stream (AV1 spec §5.3.2) in which every OBU carries its own
/// LEB128 size — the same shape an AV1-ISOBMFF sample takes, so the decoder's sample is the
/// access unit minus its temporal delimiters and padding. The reassembler learns the exact
/// access-unit length from the GFN extended frame header, so what arrives here is one complete
/// temporal unit.
public enum NvstAv1Obu {
    /// §6.2.2 `obu_type` values this client acts on.
    public static let sequenceHeaderType: UInt8 = 1
    public static let temporalDelimiterType: UInt8 = 2
    public static let frameHeaderType: UInt8 = 3
    public static let frameType: UInt8 = 6
    public static let redundantFrameHeaderType: UInt8 = 7
    public static let paddingType: UInt8 = 15

    /// One OBU inside an access unit, located without copying it.
    public struct Unit: Equatable, Sendable {
        public var type: UInt8
        /// Offset of the OBU header byte in the access unit.
        public var offset: Int
        /// Payload range within the access unit.
        public var payloadOffset: Int
        public var payloadLength: Int
        /// Header + optional extension + LEB128 size field, in bytes: the whole OBU spans
        /// `offset ..< offset + headerLength + payloadLength`.
        public var headerLength: Int
    }

    /// Walks a low-overhead OBU stream. Returns nil when the bytes are not that stream: an OBU
    /// with the forbidden bit set, without a size field, or whose size runs past the end.
    /// Without size fields the OBU boundaries are unknowable without parsing every payload, so
    /// the caller drops the frame rather than feeding the decoder a misframed unit.
    public static func units(in bytes: Data) -> [Unit]? {
        let buffer = [UInt8](bytes)
        var units: [Unit] = []
        var cursor = 0
        while cursor < buffer.count {
            let header = buffer[cursor]
            guard header & 0x80 == 0 else { return nil }
            let type = (header >> 3) & 0x0f
            let hasExtension = header & 0x04 != 0
            let hasSizeField = header & 0x02 != 0
            var offset = cursor + 1
            if hasExtension {
                guard offset < buffer.count else { return nil }
                offset += 1
            }
            guard hasSizeField else { return nil }
            guard let (size, sizeBytes) = leb128(buffer, at: offset) else { return nil }
            offset += sizeBytes
            guard size <= buffer.count - offset else { return nil }
            units.append(Unit(type: type, offset: cursor, payloadOffset: offset,
                              payloadLength: size, headerLength: offset - cursor))
            cursor = offset + size
        }
        return units
    }

    /// LEB128 (§4.10.5): up to 8 bytes of 7 payload bits, continuation in the top bit. A group
    /// that runs out of bytes or never terminates is malformed. Returns `(value, byteCount)`.
    static func leb128(_ buffer: [UInt8], at offset: Int) -> (Int, Int)? {
        var value = 0
        var index = offset
        for shift in stride(from: 0, to: 56, by: 7) {
            guard index < buffer.count else { return nil }
            let byte = buffer[index]
            value |= Int(byte & 0x7f) << shift
            index += 1
            if byte & 0x80 == 0 { return (value, index - offset) }
        }
        return nil
    }

    /// Whether any frame OBU decodes a fresh keyframe: `show_existing_frame == 0` and
    /// `frame_type == KEY_FRAME` in the uncompressed header (§5.9.2). Those are the first three
    /// payload bits, MSB first, so no full header parse is needed. A frame the seat is re-showing
    /// (`show_existing_frame == 1`) is not a random-access point.
    public static func containsKeyframe(in bytes: Data) -> Bool {
        guard let units = units(in: bytes) else { return false }
        let buffer = [UInt8](bytes)
        for unit in units where unit.type == frameType || unit.type == frameHeaderType || unit.type == redundantFrameHeaderType {
            guard unit.payloadLength > 0 else { continue }
            let first = buffer[unit.payloadOffset]
            if first & 0x80 == 0, (first >> 5) & 0x03 == 0 { return true }
        }
        return false
    }

    /// The sequence header OBU in the form an av1C record's `configOBUs` takes: the whole wire
    /// OBU, LEB128 size field included. AV1-ISOBMFF requires configOBUs to keep
    /// `obu_has_size_field`, and VideoToolbox enforces it — the size-less form builds a format
    /// description but `VTDecompressionSessionCreate` rejects it (-12911, measured 2026-09-07).
    public static func configurationOBU(for unit: Unit, in bytes: Data) -> Data {
        let buffer = [UInt8](bytes)
        return Data(buffer[unit.offset..<(unit.offset + unit.headerLength + unit.payloadLength)])
    }

    /// `bytes=N obus=[type:payloadLength:head, …]` — the AV1 counterpart to the NAL-unit shape the
    /// decoder logs for accepted and rejected access units. `obus=unframed` when the unit is not
    /// a size-fielded OBU stream at all, which is the one AV1 framing failure worth naming.
    public static func accessUnitShape(_ bytes: Data) -> String {
        guard let units = units(in: bytes) else { return "bytes=\(bytes.count) obus=unframed" }
        let buffer = [UInt8](bytes)
        let described = units.prefix(12).map { unit -> String in
            let head = buffer[unit.payloadOffset..<min(unit.payloadOffset + 4, buffer.count)]
                .map { String(format: "%02x", $0) }.joined()
            return "\(unit.type):\(unit.payloadLength):\(head)"
        }
        return "bytes=\(bytes.count) obus=[\(described.joined(separator: ", "))]"
    }

    // MARK: - Sequence header

    /// What the av1C record and the format description need out of a sequence header OBU,
    /// parsed through `color_config` (§5.5.1/§5.5.2) because the record carries its fields.
    public struct SequenceHeader: Equatable, Sendable {
        public var colorPrimaries = 2
        public var transferCharacteristics = 2
        public var matrixCoefficients = 2
        public var fullRange = false
        public var profile = 0
        public var levelIndex = 0
        public var highTier = false
        public var highBitdepth = false
        public var twelveBit = false
        public var monochrome = false
        public var subsamplingX = true
        public var subsamplingY = true
        public var chromaSamplePosition = 0
        public var maxFrameWidth = 0
        public var maxFrameHeight = 0
        /// Operating point 0's `initial_display_delay_minus_1`, when the header carries one.
        public var initialDisplayDelayMinus1: Int?

        public var bitDepth: Int { twelveBit ? 12 : (highBitdepth ? 10 : 8) }
    }

    /// Parses a sequence header OBU payload (header byte excluded). Nil on any truncation: a
    /// partial sequence header is a malformed stream, not a short one.
    public static func parseSequenceHeader(_ payload: Data) -> SequenceHeader? {
        var cursor = BitCursor([UInt8](payload))
        do {
            var header = SequenceHeader()
            header.profile = Int(try cursor.f(3))
            _ = try cursor.flag()  // still_picture
            let reduced = try cursor.flag()  // reduced_still_picture_header
            if reduced {
                // §5.5.1: timing, decoder model and display delay are all fixed "absent", one
                // operating point at tier 0 — only its level is on the wire.
                header.levelIndex = Int(try cursor.f(5))
            } else {
                try parseOperatingPoints(into: &header, cursor: &cursor)
            }
            try parseFrameSize(into: &header, cursor: &cursor, reduced: reduced)
            try parseCodingToolFlags(cursor: &cursor, reduced: reduced)
            try parseColorConfig(into: &header, cursor: &cursor)
            return header
        } catch {
            return nil
        }
    }

    /// Timing info, decoder model and the operating-point loop of the non-reduced header.
    /// `decoder_model_info_present_flag` exists only when timing info is present (§5.5.1) —
    /// reading it unconditionally shifts every later field by one bit on streams without timing
    /// info (checked against FFmpeg's cbs_av1 trace of a real encoder's sequence header).
    private static func parseOperatingPoints(into header: inout SequenceHeader, cursor: inout BitCursor) throws {
        var decoderModelInfoPresent = false
        var bufferDelayLength = 0
        if try cursor.flag() {  // timing_info_present_flag
            _ = try cursor.f(32)  // num_units_in_display_tick
            _ = try cursor.f(32)  // time_scale
            if try cursor.flag() { _ = try cursor.uvlc() }  // equal_picture_interval → num_ticks_per_picture_minus_1
            decoderModelInfoPresent = try cursor.flag()
            if decoderModelInfoPresent {
                bufferDelayLength = Int(try cursor.f(5)) + 1  // buffer_delay_length_minus_1
                _ = try cursor.f(32)  // num_units_in_decoding_tick
                _ = try cursor.f(5)  // buffer_removal_time_length_minus_1
                _ = try cursor.f(5)  // frame_presentation_time_length_minus_1
            }
        }
        let initialDisplayDelayPresent = try cursor.flag()
        let operatingPointsMinus1 = Int(try cursor.f(5))
        for point in 0...operatingPointsMinus1 {
            try parseOperatingPoint(point, into: &header, cursor: &cursor,
                                    bufferDelayLength: bufferDelayLength,
                                    decoderModelInfoPresent: decoderModelInfoPresent,
                                    initialDisplayDelayPresent: initialDisplayDelayPresent)
        }
    }

    /// One iteration of the operating-point loop; only point 0's level, tier and display delay
    /// reach the record.
    private static func parseOperatingPoint(_ point: Int,
                                            into header: inout SequenceHeader,
                                            cursor: inout BitCursor,
                                            bufferDelayLength: Int,
                                            decoderModelInfoPresent: Bool,
                                            initialDisplayDelayPresent: Bool) throws {
        _ = try cursor.f(12)  // operating_point_idc
        let level = Int(try cursor.f(5))
        var tier = false
        if level > 7 { tier = try cursor.flag() }
        if point == 0 { header.levelIndex = level; header.highTier = tier }
        if decoderModelInfoPresent, try cursor.flag() {  // decoder_model_present_for_this_op
            _ = try cursor.f(bufferDelayLength)  // decoder_buffer_delay
            _ = try cursor.f(bufferDelayLength)  // encoder_buffer_delay
            _ = try cursor.flag()  // low_delay_mode_flag
        }
        if initialDisplayDelayPresent, try cursor.flag() {  // initial_display_delay_present_for_this_op
            let delayMinus1 = Int(try cursor.f(4))
            if point == 0 { header.initialDisplayDelayMinus1 = delayMinus1 }
        }
    }

    /// `max_frame_width/height`, plus the frame-id syntax the non-reduced header carries between
    /// them and the coding tools.
    private static func parseFrameSize(into header: inout SequenceHeader, cursor: inout BitCursor, reduced: Bool) throws {
        let widthBits = Int(try cursor.f(4)) + 1  // frame_width_bits_minus_1
        let heightBits = Int(try cursor.f(4)) + 1  // frame_height_bits_minus_1
        header.maxFrameWidth = Int(try cursor.f(widthBits)) + 1
        header.maxFrameHeight = Int(try cursor.f(heightBits)) + 1
        guard !reduced else { return }
        if try cursor.flag() {  // frame_id_numbers_present_flag
            _ = try cursor.f(4)  // delta_frame_id_length_minus_2
            _ = try cursor.f(3)  // additional_frame_id_length_minus_1
        }
    }

    /// The coding-tool block between the frame size and `color_config` — parsed only to land the
    /// cursor on `color_config`; none of it reaches the record. Reduced headers fix the whole
    /// inter-prediction block to "off" (§5.5.1), but `enable_superres`/`enable_cdef`/
    /// `enable_restoration` are on the wire either way.
    private static func parseCodingToolFlags(cursor: inout BitCursor, reduced: Bool) throws {
        _ = try cursor.flag()  // use_128x128_superblock
        _ = try cursor.flag()  // enable_filter_intra
        _ = try cursor.flag()  // enable_intra_edge_filter
        if !reduced {
            _ = try cursor.f(4)  // enable_interintra_compound, enable_masked_compound, enable_warped_motion, enable_dual_filter
            let orderHint = try cursor.flag()  // enable_order_hint
            if orderHint {
                _ = try cursor.flag()  // enable_jnt_comp
                _ = try cursor.flag()  // enable_ref_frame_mvs
            }
            var forceScreenContentTools = 2  // SELECT_SCREEN_CONTENT_TOOLS
            if try !cursor.flag() {  // seq_choose_screen_content_tools
                forceScreenContentTools = Int(try cursor.f(1))
            }
            if forceScreenContentTools > 0, try !cursor.flag() {  // seq_choose_integer_mv
                _ = try cursor.f(1)  // seq_force_integer_mv
            }
            if orderHint { _ = try cursor.f(3) }  // order_hint_bits_minus_1
        }
        _ = try cursor.flag()  // enable_superres
        _ = try cursor.flag()  // enable_cdef
        _ = try cursor.flag()  // enable_restoration
    }

    /// §5.5.2 `color_config` — the av1C depth and chroma fields live here, so this one is kept
    /// rather than skipped.
    private static func parseColorConfig(into header: inout SequenceHeader, cursor: inout BitCursor) throws {
        header.highBitdepth = try cursor.flag()
        if header.profile == 2 && header.highBitdepth {
            header.twelveBit = try cursor.flag()
        }
        if header.profile != 1 {
            header.monochrome = try cursor.flag()
        }
        var colorPrimaries = 2, transferCharacteristics = 2, matrixCoefficients = 2
        if try cursor.flag() {  // color_description_present_flag
            colorPrimaries = Int(try cursor.f(8))
            transferCharacteristics = Int(try cursor.f(8))
            matrixCoefficients = Int(try cursor.f(8))
        }
        header.colorPrimaries = colorPrimaries
        header.transferCharacteristics = transferCharacteristics
        header.matrixCoefficients = matrixCoefficients
        try parseChromaSubsampling(into: &header, cursor: &cursor,
                                   srgbIdentity: colorPrimaries == 1 && transferCharacteristics == 13 && matrixCoefficients == 0)
    }

    /// The subsampling tail of `color_config`. The monochrome branch returns before
    /// `separate_uv_delta_q` in the spec (its value is fixed 0); the sRGB-identity branch fixes
    /// 4:4:4 and full range with no further bits until `film_grain_params_present`.
    private static func parseChromaSubsampling(into header: inout SequenceHeader, cursor: inout BitCursor, srgbIdentity: Bool) throws {
        if header.monochrome {
            header.fullRange = try cursor.flag()  // color_range
            header.subsamplingX = true
            header.subsamplingY = true
            header.chromaSamplePosition = 0
            return
        }
        if srgbIdentity {
            header.fullRange = true
            header.subsamplingX = false
            header.subsamplingY = false
            return
        }
        header.fullRange = try cursor.flag()  // color_range
        switch header.profile {
        case 0:
            header.subsamplingX = true
            header.subsamplingY = true
        case 1:
            header.subsamplingX = false
            header.subsamplingY = false
        default:
            if header.bitDepth == 12 {
                header.subsamplingX = try cursor.flag()
                if header.subsamplingX { header.subsamplingY = try cursor.flag() }
            } else {
                header.subsamplingX = true
                header.subsamplingY = false
            }
        }
        if header.subsamplingX && header.subsamplingY {
            header.chromaSamplePosition = Int(try cursor.f(2))
        }
        _ = try cursor.flag()  // separate_uv_delta_q
    }

    /// The `av1C` codec configuration record (AV1-ISOBMFF §2.3.2) for one sequence header: four
    /// marker/profile/level/flag bytes, then the sequence header OBU in its size-less form.
    public static func codecConfigurationRecord(header: SequenceHeader, configurationOBU: Data) -> Data {
        var writer = NvstByteWriter(capacity: configurationOBU.count + 4)
        writer.u8(0x81)  // marker 1, version 1
        writer.u8(UInt8(header.profile << 5 | header.levelIndex & 0x1f))
        writer.u8(UInt8((header.highTier ? 0x80 : 0)
                        | (header.highBitdepth ? 0x40 : 0)
                        | (header.twelveBit ? 0x20 : 0)
                        | (header.monochrome ? 0x10 : 0)
                        | (header.subsamplingX ? 0x08 : 0)
                        | (header.subsamplingY ? 0x04 : 0)
                        | (header.chromaSamplePosition & 0x03)))
        writer.u8(header.initialDisplayDelayMinus1.map { UInt8(0x10 | ($0 & 0x0f)) } ?? 0)
        writer.bytes(configurationOBU)
        return writer.data
    }

    /// MSB-first bit cursor over one OBU payload. Every read throws past the end, letting the
    /// sequence-header parse fail closed on truncation instead of reading garbage.
    private struct BitCursor {
        enum ReadError: Error { case truncated }

        private let bytes: [UInt8]
        private var position = 0  // in bits

        init(_ bytes: [UInt8]) { self.bytes = bytes }

        /// `f(count)`: the next `count` bits as an unsigned integer (§4.10.2).
        mutating func f(_ count: Int) throws -> UInt32 {
            guard count >= 0, count <= 32, position + count <= bytes.count * 8 else { throw ReadError.truncated }
            var value: UInt32 = 0
            for _ in 0..<count {
                let byte = bytes[position / 8]
                let bit = (byte >> UInt8(7 - (position % 8))) & 1
                value = (value << 1) | UInt32(bit)
                position += 1
            }
            return value
        }

        mutating func flag() throws -> Bool {
            try f(1) != 0
        }

        /// `uvlc()` (§4.10.3): a run of zero bits, then a value biased by the run length. 32
        /// leading zeroes is the escape form that reads a raw 32-bit value.
        mutating func uvlc() throws -> UInt32 {
            var leadingZeros = 0
            while leadingZeros < 32 {
                if try f(1) != 0 { break }
                leadingZeros += 1
            }
            if leadingZeros == 32 { return try f(32) }
            return (UInt32(1) << UInt32(leadingZeros)) - 1 + (try f(leadingZeros))
        }
    }
}
