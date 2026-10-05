import Foundation
import Testing
@testable import OpenNOWiOS

/// AV1 OBU parsing, keyframe detection and av1C record building, against sequence headers built
/// bit-by-bit in the test so the parse is checked against the spec rather than a captured blob.
@Suite(.serialized)
struct NvstAv1ObuTests {
    /// MSB-first bit writer — the encode-side counterpart of the cursor `NvstAv1Obu` parses with.
    private struct BitWriter {
        private(set) var bytes: [UInt8] = []
        private var current: UInt8 = 0
        private var used = 0

        mutating func f(_ value: UInt32, _ count: Int) {
            for shift in stride(from: count - 1, through: 0, by: -1) {
                current = (current << 1) | UInt8((value >> UInt32(shift)) & 1)
                used += 1
                if used == 8 {
                    bytes.append(current)
                    current = 0
                    used = 0
                }
            }
        }

        mutating func flag(_ value: Bool) { f(value ? 1 : 0, 1) }

        mutating func finish() -> [UInt8] {
            if used > 0 { bytes.append(current << UInt8(8 - used)) }
            return bytes
        }
    }

    static func leb128(_ value: Int) -> [UInt8] {
        var bytes: [UInt8] = []
        var remaining = value
        repeat {
            var byte = UInt8(remaining & 0x7f)
            remaining >>= 7
            if remaining > 0 { byte |= 0x80 }
            bytes.append(byte)
        } while remaining > 0
        return bytes
    }

    /// One low-overhead OBU: header with the size-field bit set, LEB128 size, payload.
    static func obu(type: UInt8, payload: [UInt8]) -> Data {
        Data([type << 3 | 0x02] + leb128(payload.count) + payload)
    }

    /// Profile 0, level 4.0 (seq_level_idx 8 — past 7 so the tier bit is on the wire), 10-bit
    /// 4:2:0, 1920x1080, order hints on, colour described as BT.709.
    static func sequenceHeaderPayload() -> Data {
        var writer = BitWriter()
        writer.f(0, 3)  // seq_profile
        writer.flag(false)  // still_picture
        writer.flag(false)  // reduced_still_picture_header
        writer.flag(false)  // timing_info_present_flag (decoder_model_info only exists with timing info)
        writer.flag(false)  // initial_display_delay_present_flag
        writer.f(0, 5)  // operating_points_cnt_minus_1
        writer.f(0, 12)  // operating_point_idc[0]
        writer.f(8, 5)  // seq_level_idx[0]
        writer.flag(false)  // seq_tier[0] (level > 7 puts it on the wire)
        writer.f(10, 4)  // frame_width_bits_minus_1 → 11-bit widths
        writer.f(10, 4)  // frame_height_bits_minus_1
        writer.f(1919, 11)  // max_frame_width_minus_1 → 1920
        writer.f(1079, 11)  // max_frame_height_minus_1 → 1080
        writer.flag(false)  // frame_id_numbers_present_flag
        writer.flag(false)  // use_128x128_superblock
        writer.flag(false)  // enable_filter_intra
        writer.flag(false)  // enable_intra_edge_filter
        writer.f(0, 4)  // interintra / masked / warped / dual-filter enables
        writer.flag(true)  // enable_order_hint
        writer.flag(false)  // enable_jnt_comp
        writer.flag(false)  // enable_ref_frame_mvs
        writer.flag(true)  // seq_choose_screen_content_tools → SELECT
        writer.flag(true)  // seq_choose_integer_mv → SELECT (read whenever force > 0)
        writer.f(6, 3)  // order_hint_bits_minus_1
        writer.flag(false)  // enable_superres
        writer.flag(true)  // enable_cdef
        writer.flag(false)  // enable_restoration
        writer.flag(true)  // high_bitdepth → 10-bit on profile 0
        writer.flag(false)  // mono_chrome
        writer.flag(true)  // color_description_present_flag
        writer.f(1, 8)  // color_primaries BT.709
        writer.f(1, 8)  // transfer_characteristics BT.709
        writer.f(1, 8)  // matrix_coefficients BT.709 (not identity → subsampling is signalled)
        writer.flag(false)  // color_range
        writer.f(0, 2)  // chroma_sample_position
        writer.flag(false)  // separate_uv_delta_q
        return Data(writer.finish())
    }

    /// Reduced still-picture header, profile 2, 12-bit 4:2:2, 3840x2160 — the branch where the
    /// twelve-bit and subsampling flags are on the wire.
    static func reducedSequenceHeaderPayload() -> Data {
        var writer = BitWriter()
        writer.f(2, 3)  // seq_profile
        writer.flag(true)  // still_picture
        writer.flag(true)  // reduced_still_picture_header
        writer.f(13, 5)  // seq_level_idx[0] (no tier bit in the reduced header)
        writer.f(11, 4)  // frame_width_bits_minus_1 → 12-bit widths
        writer.f(11, 4)  // frame_height_bits_minus_1
        writer.f(3839, 12)  // max_frame_width_minus_1 → 3840
        writer.f(2159, 12)  // max_frame_height_minus_1 → 2160
        writer.flag(false)  // use_128x128_superblock
        writer.flag(false)  // enable_filter_intra
        writer.flag(false)  // enable_intra_edge_filter
        writer.flag(false)  // enable_superres
        writer.flag(false)  // enable_cdef
        writer.flag(false)  // enable_restoration
        writer.flag(true)  // high_bitdepth
        writer.flag(true)  // twelve_bit (profile 2 only)
        writer.flag(false)  // mono_chrome
        writer.flag(false)  // color_description_present_flag
        writer.flag(true)  // color_range
        writer.flag(true)  // subsampling_x (12-bit profile 2 signals it)
        writer.flag(false)  // subsampling_y → 4:2:2
        writer.flag(false)  // separate_uv_delta_q
        return Data(writer.finish())
    }

    static var keyframeAccessUnit: Data {
        Self.obu(type: NvstAv1Obu.temporalDelimiterType, payload: [])
            + Self.obu(type: NvstAv1Obu.sequenceHeaderType, payload: [UInt8](Self.sequenceHeaderPayload()))
            + Self.obu(type: NvstAv1Obu.frameType, payload: [0x00, 0xde, 0xad])
    }

    static var interFrameAccessUnit: Data {
        Self.obu(type: NvstAv1Obu.frameType, payload: [0x20, 0xbe, 0xef])
    }

    // MARK: - OBU walking

    @Test func unitsWalkEveryObuWithItsSize() {
        let stream = Self.keyframeAccessUnit
        let units = NvstAv1Obu.units(in: stream)
        #expect(units?.count == 3)
        #expect(units?[0].type == NvstAv1Obu.temporalDelimiterType)
        #expect(units?[0].payloadLength == 0)
        #expect(units?[1].type == NvstAv1Obu.sequenceHeaderType)
        #expect(units?[1].payloadLength == Self.sequenceHeaderPayload().count)
        #expect(units?[2].type == NvstAv1Obu.frameType)
        #expect(units?[2].payloadLength == 3)
        // Offsets must tile the whole buffer exactly.
        var cursor = 0
        for unit in units ?? [] {
            #expect(unit.offset == cursor)
            cursor = unit.payloadOffset + unit.payloadLength
        }
        #expect(cursor == stream.count)
    }

    @Test func unitsAcceptMultiByteLeb128Sizes() {
        let payload = [UInt8](repeating: 0xa5, count: 300)
        let stream = Self.obu(type: NvstAv1Obu.frameType, payload: payload)
        let units = NvstAv1Obu.units(in: stream)
        #expect(units?.count == 1)
        #expect(units?.first?.payloadLength == 300)
        #expect(units?.first?.headerLength == 3)
        #expect(Self.leb128(300) == [0xac, 0x02])
    }

    @Test func unitsRejectStreamsWithoutSizeFieldsOrPastTheEnd() {
        // Size-field bit clear: OBU boundaries are unknowable, so the walk fails closed.
        #expect(NvstAv1Obu.units(in: Data([0x08, 0x01, 0x02])) == nil)
        // Forbidden bit set.
        #expect(NvstAv1Obu.units(in: Data([0x8a, 0x00])) == nil)
        // Advertised size runs past the end.
        #expect(NvstAv1Obu.units(in: Data([0x0a, 0x05, 0x01])) == nil)
        // LEB128 group never terminates.
        #expect(NvstAv1Obu.units(in: Data([0x0a, 0x80])) == nil)
        // Empty input is an empty stream, not a malformed one.
        #expect(NvstAv1Obu.units(in: Data()) == [])
    }

    // MARK: - Keyframes

    @Test func keyframeDetectionReadsTheFrameHeaderBits() {
        // 0x00: show_existing_frame 0, frame_type 0 (KEY_FRAME).
        #expect(NvstAv1Obu.containsKeyframe(in: Self.keyframeAccessUnit))
        // 0x20: show_existing_frame 0, frame_type 1 (INTER_FRAME).
        #expect(!NvstAv1Obu.containsKeyframe(in: Self.interFrameAccessUnit))
        // 0x80: show_existing_frame 1 — a re-shown frame is not a random-access point.
        let reshown = Self.obu(type: NvstAv1Obu.frameType, payload: [0x80, 0x00])
        #expect(!NvstAv1Obu.containsKeyframe(in: reshown))
        // A frame-header OBU (rather than a full frame OBU) answers the same way.
        let headerOnly = Self.obu(type: NvstAv1Obu.frameHeaderType, payload: [0x00])
        #expect(NvstAv1Obu.containsKeyframe(in: headerOnly))
        // The reassembler's one-pass scan must agree, since it is what marks the access unit.
        #expect(NvstAnnexB.scan(Self.keyframeAccessUnit, codec: .av1).isKeyframe)
        #expect(!NvstAnnexB.scan(Self.interFrameAccessUnit, codec: .av1).isKeyframe)
    }

    // MARK: - Sequence header

    @Test func sequenceHeaderParsesEveryFieldTheRecordNeeds() throws {
        let header = try #require(NvstAv1Obu.parseSequenceHeader(Self.sequenceHeaderPayload()))
        #expect(header.profile == 0)
        #expect(header.levelIndex == 8)
        #expect(!header.highTier)
        #expect(header.highBitdepth)
        #expect(!header.twelveBit)
        #expect(header.bitDepth == 10)
        #expect(!header.monochrome)
        #expect(header.subsamplingX)
        #expect(header.subsamplingY)
        #expect(header.chromaSamplePosition == 0)
        #expect(header.maxFrameWidth == 1920)
        #expect(header.maxFrameHeight == 1080)
        #expect(header.initialDisplayDelayMinus1 == nil)
    }

    @Test func reducedSequenceHeaderParsesTheTwelveBitBranch() throws {
        let header = try #require(NvstAv1Obu.parseSequenceHeader(Self.reducedSequenceHeaderPayload()))
        #expect(header.profile == 2)
        #expect(header.levelIndex == 13)
        #expect(header.twelveBit)
        #expect(header.bitDepth == 12)
        #expect(header.subsamplingX)
        #expect(!header.subsamplingY)
        #expect(header.maxFrameWidth == 3840)
        #expect(header.maxFrameHeight == 2160)
    }

    @Test func truncatedSequenceHeadersFailClosed() {
        let payload = Self.sequenceHeaderPayload()
        for length in [0, 1, 2, payload.count / 2, payload.count - 1] {
            #expect(NvstAv1Obu.parseSequenceHeader(payload.prefix(length)) == nil, "length \(length) must not parse")
        }
    }

    // MARK: - av1C record

    @Test func codecConfigurationRecordPacksTheHeaderFields() throws {
        let header = try #require(NvstAv1Obu.parseSequenceHeader(Self.sequenceHeaderPayload()))
        let payload = Self.sequenceHeaderPayload()
        let units = try #require(NvstAv1Obu.units(in: Self.keyframeAccessUnit))
        let sh = try #require(units.first { $0.type == NvstAv1Obu.sequenceHeaderType })
        let configuration = NvstAv1Obu.configurationOBU(for: sh, in: Self.keyframeAccessUnit)
        // The configOBUs form keeps the wire OBU verbatim, size field included: AV1-ISOBMFF
        // requires obu_has_size_field in configOBUs and VideoToolbox enforces it.
        #expect(configuration == Self.obu(type: NvstAv1Obu.sequenceHeaderType, payload: [UInt8](payload)))

        let record = NvstAv1Obu.codecConfigurationRecord(header: header, configurationOBU: configuration)
        #expect(record.prefix(4) == Data([0x81, 0x08, 0x4c, 0x00]))
        #expect(record.dropFirst(4) == configuration)
    }

    // MARK: - Elementary-stream integration

    @Test func prepareStripsTemporalDelimitersAndHarvestsTheSequenceHeader() {
        let stream = Self.keyframeAccessUnit
        let prepared = NvstElementaryStream.prepare(stream, codec: .av1)
        let shWire = Self.obu(type: NvstAv1Obu.sequenceHeaderType, payload: [UInt8](Self.sequenceHeaderPayload()))
        let frameWire = Self.obu(type: NvstAv1Obu.frameType, payload: [0x00, 0xde, 0xad])
        // The sample keeps every OBU but the temporal delimiter, size fields intact.
        #expect(prepared.sample == shWire + frameWire)
        // The parameter set is the sequence header in its wire form (size field included).
        #expect(prepared.parameterSets.sequenceParameterSets == [shWire])
        #expect(prepared.parameterSets.isComplete(for: .av1))
        #expect(!prepared.parameterSets.isComplete)
        // A delta frame carries no sequence header; the decoder's cached set keeps decoding.
        let delta = NvstElementaryStream.prepare(Self.interFrameAccessUnit, codec: .av1)
        #expect(!delta.sample.isEmpty)
        #expect(!delta.parameterSets.isComplete(for: .av1))
    }

    @Test func prepareDropsAnUnframedAccessUnit() {
        // Not a size-fielded OBU stream: no sample, no parameter sets — the decoder logs and
        // asks the seat for a fresh keyframe rather than decoding garbage.
        let prepared = NvstElementaryStream.prepare(Data([0x00, 0x01, 0x02, 0x03]), codec: .av1)
        #expect(prepared.sample.isEmpty)
        #expect(prepared.parameterSets.sequenceParameterSets.isEmpty)
    }
}
