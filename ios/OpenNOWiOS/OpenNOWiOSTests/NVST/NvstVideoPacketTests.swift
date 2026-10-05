import Foundation
import Testing
@testable import OpenNOWiOS

@Suite(.serialized)
struct NvstVideoPacketTests {
    static let containsPicData: UInt8 = 0x01
    static let endOfFrame: UInt8 = 0x02
    static let startOfFrame: UInt8 = 0x04

    /// Builds a plaintext RTP packet matching the independently documented GS wire format
    /// (12-byte RTP header + 4-byte extension header + 16-byte GS block + payload).
    static func buildPacket(sequence: UInt16,
                            frameIndex: UInt32,
                            flags: UInt8,
                            media: [UInt8],
                            fecWord: UInt32 = 0,
                            streamSequence: UInt32? = nil) -> Data {
        var packet = Data()
        packet.append(0x90)                  // V=2, X=1
        packet.append(0xe0)                  // M=1, PT=96
        packet.append(UInt8(sequence >> 8))
        packet.append(UInt8(sequence & 0xff))
        packet.append(contentsOf: [0x01, 0x02, 0x03, 0x04]) // timestamp
        packet.append(contentsOf: [0x11, 0x22, 0x33, 0x44]) // ssrc
        packet.append(contentsOf: [0x47, 0x53])              // profile 0x4753 ("GS")
        packet.append(contentsOf: [0x00, 0x04])              // 4 × 32-bit words = 16 bytes
        // GS block: stream packet index (<<8, LE), frame index (LE), flags word (LE), FEC word (LE).
        let sequenceWord = (streamSequence ?? UInt32(sequence)) << 8
        packet.append(contentsOf: withUnsafeBytes(of: sequenceWord.littleEndian) { Array($0) })
        packet.append(contentsOf: withUnsafeBytes(of: frameIndex.littleEndian) { Array($0) })
        packet.append(contentsOf: withUnsafeBytes(of: UInt32(flags).littleEndian) { Array($0) })
        packet.append(contentsOf: withUnsafeBytes(of: fecWord.littleEndian) { Array($0) })
        packet.append(contentsOf: media)
        return packet
    }

    private func buildPacket(sequence: UInt16, frameIndex: UInt32, flags: UInt8, media: [UInt8], fecWord: UInt32 = 0) -> Data {
        Self.buildPacket(sequence: sequence, frameIndex: frameIndex, flags: flags, media: media, fecWord: fecWord)
    }

    @Test func parsesRtpHeaderAndGsExtension() throws {
        let packet = try NvstVideoPacketParser.parse(buildPacket(sequence: 0x1234, frameIndex: 7, flags: 0x05, media: [0x00, 0x00, 0x00, 0x01]))
        #expect(packet.payloadType == 96)
        #expect(packet.sequenceNumber == 0x1234)
        #expect(packet.streamSequence == 0x1234)
        #expect(packet.frameIndex == 7)
        #expect(packet.flags.contains(.startOfFrame))
        #expect(packet.flags.contains(.containsPicData))
        #expect(!packet.isFec)
        #expect(packet.ssrc == 0x1122_3344)
        #expect(packet.payload == Data([0x00, 0x00, 0x00, 0x01]))
    }

    @Test func parsesTheStreamPacketIndexAsATwentyFourBitValue() throws {
        // The wire value is `index << 8`, so only the top 24 bits are meaningful.
        var packet = buildPacket(sequence: 1, frameIndex: 1, flags: 0x01, media: [])
        packet.replaceSubrange(16..<20, with: withUnsafeBytes(of: UInt32(0xffff_ffff).littleEndian) { Data($0) })
        let parsed = try NvstVideoPacketParser.parse(packet)
        #expect(parsed.streamSequence == 0x00ff_ffff)
    }

    @Test func onlyTheLowNibbleOfTheFlagsWordIsRead() throws {
        // The upper bits of the flags word are packet-type metadata, not GS flags.
        let packet = buildPacket(sequence: 1, frameIndex: 1, flags: 0xf5, media: [])
        let parsed = try NvstVideoPacketParser.parse(packet)
        #expect(parsed.flags.rawValue == 0x05)
    }

    @Test func classifiesFecRepairPacketsFromTheGroupCoordinates() throws {
        // FecId=3, SrcPkts=3 with the group marker set: a repair packet.
        let repair = try NvstVideoPacketParser.parse(buildPacket(sequence: 3, frameIndex: 7, flags: 0x01, media: [0xaa], fecWord: 0x00c0_3420))
        #expect(repair.isFec)
        // FecId=2 < SrcPkts=3: a source packet.
        let source = try NvstVideoPacketParser.parse(buildPacket(sequence: 2, frameIndex: 7, flags: 0x01, media: [0xaa], fecWord: 0x00c0_2420))
        #expect(!source.isFec)
    }

    @Test func rejectsNonRtpAndMissingExtension() {
        #expect(throws: NvstRtpParseError.self) { _ = try NvstVideoPacketParser.parse(Data([0x00, 0x00])) }
        var noExt = buildPacket(sequence: 1, frameIndex: 1, flags: 0, media: [])
        noExt[0] = 0x80 // clear the extension bit
        #expect(throws: NvstRtpParseError.self) { _ = try NvstVideoPacketParser.parse(noExt) }
        var wrongProfile = buildPacket(sequence: 1, frameIndex: 1, flags: 0, media: [])
        wrongProfile[12] = 0x00
        #expect(throws: NvstRtpParseError.missingGsExtension) { _ = try NvstVideoPacketParser.parse(wrongProfile) }
    }

    @Test func reassemblesAcrossSofEofBoundariesAndStripsTheGsFrameHeader() throws {
        let assembler = NvstFrameReassembler()
        // A GS frame header precedes the first Annex-B start code on the SOF packet.
        let start = try NvstVideoPacketParser.parse(buildPacket(sequence: 10, frameIndex: 100, flags: 0x05, media: [0x01, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x01, 0x65]))
        let middle = try NvstVideoPacketParser.parse(buildPacket(sequence: 11, frameIndex: 100, flags: 0x01, media: [0x10, 0x20]))
        let end = try NvstVideoPacketParser.parse(buildPacket(sequence: 12, frameIndex: 100, flags: 0x03, media: [0x80, 0x90]))

        #expect(try assembler.push(start) == nil)
        #expect(try assembler.push(middle) == nil)
        let unit = try assembler.push(end)
        #expect(unit?.bytes == Data([0x00, 0x00, 0x00, 0x01, 0x65, 0x10, 0x20, 0x80, 0x90]))
        #expect(unit?.frameIndex == 100)
        #expect(unit?.firstStreamPacketIndex == 10)
        #expect(unit?.rtpTimestamp == 0x0102_0304)
        // NAL type 5 anywhere in the unit makes it a keyframe.
        #expect(unit?.isKeyframe == true)
    }

    /// A source packet lost inside a frame used to be concatenated over, handing VideoToolbox an
    /// access unit it rejected as bad data — measured as `kVTVideoDecoderBadDataErr` (-12909) on a
    /// live seat, on roughly a quarter of all frames.
    @Test func aHoleInsideAFrameAbandonsItInsteadOfConcatenatingAcross() throws {
        let assembler = NvstFrameReassembler()
        let start = try NvstVideoPacketParser.parse(buildPacket(sequence: 20, frameIndex: 5, flags: 0x05, media: [0x00, 0x00, 0x00, 0x01, 0x65]))
        // Sequence 21 never arrives.
        let end = try NvstVideoPacketParser.parse(buildPacket(sequence: 22, frameIndex: 5, flags: 0x03, media: [0xcc]))
        #expect(try assembler.push(start) == nil)
        #expect(throws: NvstReassemblyDrop.sequenceGap(expected: 21, received: 22, frameIndex: 5)) { _ = try assembler.push(end) }

        // The next whole frame still assembles: the gap abandons one frame, not the stream.
        let nextStart = try NvstVideoPacketParser.parse(buildPacket(sequence: 23, frameIndex: 6, flags: 0x05, media: [0x00, 0x00, 0x00, 0x01, 0x65]))
        let nextEnd = try NvstVideoPacketParser.parse(buildPacket(sequence: 24, frameIndex: 6, flags: 0x03, media: [0xdd]))
        #expect(try assembler.push(nextStart) == nil)
        #expect(try assembler.push(nextEnd)?.bytes == Data([0x00, 0x00, 0x00, 0x01, 0x65, 0xdd]))
    }

    /// The AUD's NAL header differs per codec, and reading HEVC with H.264 rules silently never
    /// matches — so the delimiter the decoder trips over survives into the access unit.
    @Test func theAccessUnitDelimiterIsStrippedPerCodec() {
        let payload: [UInt8] = [0x00, 0x00, 0x00, 0x01, 0x26, 0x01, 0xaf]
        let h264AUD = Data(payload + [0x00, 0x00, 0x00, 0x01, 0x09, 0x10])
        let hevcAUD = Data(payload + [0x00, 0x00, 0x00, 0x01, 0x46, 0x01, 0x50])

        #expect(NvstFrameReassembler.strippingTrailingAccessUnitDelimiter(h264AUD, codec: .h264) == Data(payload))
        #expect(NvstFrameReassembler.strippingTrailingAccessUnitDelimiter(hevcAUD, codec: .hevc) == Data(payload))
        // Cross-codec rules must not match, and must not truncate anything either.
        #expect(NvstFrameReassembler.strippingTrailingAccessUnitDelimiter(hevcAUD, codec: .h264) == hevcAUD)
        #expect(NvstFrameReassembler.strippingTrailingAccessUnitDelimiter(h264AUD, codec: .hevc) == h264AUD)
        // AV1 carries no Annex-B delimiter, so nothing is ever removed.
        #expect(NvstFrameReassembler.strippingTrailingAccessUnitDelimiter(hevcAUD, codec: .av1) == hevcAUD)
    }

    /// Abandoning a partial frame used to be silent, so a stream losing most of its frames read
    /// as healthy: `frames` was low but nothing said why.
    @Test func abandonedPartialFramesAreCounted() throws {
        let assembler = NvstFrameReassembler()
        #expect(assembler.abandonedFrameCount == 0)
        let start1 = try NvstVideoPacketParser.parse(buildPacket(sequence: 1, frameIndex: 1, flags: 0x05, media: [0x00, 0x00, 0x00, 0x01, 0x41]))
        #expect(try assembler.push(start1) == nil)
        // A second start-of-frame with the first still assembling.
        let start2 = try NvstVideoPacketParser.parse(buildPacket(sequence: 2, frameIndex: 2, flags: 0x05, media: [0x00, 0x00, 0x00, 0x01, 0x41]))
        #expect(try assembler.push(start2) == nil)
        #expect(assembler.abandonedFrameCount == 1)
        // Completing a frame cleanly does not count as abandonment.
        let end2 = try NvstVideoPacketParser.parse(buildPacket(sequence: 3, frameIndex: 2, flags: 0x03, media: [0x02]))
        #expect(try assembler.push(end2) != nil)
        #expect(assembler.abandonedFrameCount == 1)
    }

    @Test func aNewStartOfFrameAbandonsTheStraggler() throws {
        let assembler = NvstFrameReassembler(maxAccessUnitBytes: 1_000_000)
        let start1 = try NvstVideoPacketParser.parse(buildPacket(sequence: 1, frameIndex: 1, flags: 0x05, media: [0x00, 0x00, 0x00, 0x01, 0x41]))
        let start2 = try NvstVideoPacketParser.parse(buildPacket(sequence: 2, frameIndex: 2, flags: 0x05, media: [0x00, 0x00, 0x00, 0x01, 0x41, 0x02]))
        let end2 = try NvstVideoPacketParser.parse(buildPacket(sequence: 3, frameIndex: 2, flags: 0x03, media: [0x03]))
        #expect(try assembler.push(start1) == nil)
        #expect(try assembler.push(start2) == nil)
        let unit = try assembler.push(end2)
        #expect(unit?.bytes == Data([0x00, 0x00, 0x00, 0x01, 0x41, 0x02, 0x03]))
        // A non-IDR slice is not a keyframe.
        #expect(unit?.isKeyframe == false)
    }

    @Test func onlyFecRepairPacketsAreRejected() throws {
        let assembler = NvstFrameReassembler()
        let repair = try NvstVideoPacketParser.parse(buildPacket(sequence: 1, frameIndex: 1, flags: 0x05, media: [0x00, 0x00, 0x00, 0x01, 0x65], fecWord: 0x00c0_3420))
        #expect(throws: NvstReassemblyDrop.notPictureData) { _ = try assembler.push(repair) }
        // The picture-data flag is NOT a reliable gate: a source packet in a later FEC block of the
        // same frame does not always set it, and rejecting those loses real video.
        let noPictureFlag = try NvstVideoPacketParser.parse(buildPacket(sequence: 2, frameIndex: 1, flags: 0x04, media: [0x00, 0x00, 0x00, 0x01, 0x65]))
        #expect(try assembler.push(noPictureFlag) == nil)
    }

    @Test func startAndEndOfFrameAreQualifiedByTheFecBlockCounters() throws {
        // The multi-FEC-block counters live in the top byte of the flags word: current block in
        // bits 4–5, last block in bits 6–7. A frame spanning blocks only starts on block 0 and only
        // ends on the last block.
        func packet(flags: UInt8, currentBlock: UInt8, lastBlock: UInt8) throws -> NvstRtpVideoPacket {
            let flagsWord = UInt32(flags) | (UInt32((currentBlock << 4) | (lastBlock << 6)) << 24)
            var raw = buildPacket(sequence: 1, frameIndex: 1, flags: 0, media: [0x00, 0x00, 0x00, 0x01, 0x65])
            raw.replaceSubrange(24..<28, with: withUnsafeBytes(of: flagsWord.littleEndian) { Data($0) })
            return try NvstVideoPacketParser.parse(raw)
        }
        #expect(try packet(flags: 0x05, currentBlock: 0, lastBlock: 0).isStartOfFrame)
        // SOF set, but this is the second FEC block of the frame: not a frame start.
        #expect(try !packet(flags: 0x05, currentBlock: 1, lastBlock: 1).isStartOfFrame)
        #expect(try packet(flags: 0x03, currentBlock: 1, lastBlock: 1).isEndOfFrame)
        // EOF set on a block that is not the last: the frame continues.
        #expect(try !packet(flags: 0x03, currentBlock: 0, lastBlock: 1).isEndOfFrame)
    }

    @Test func aTrailingAccessUnitDelimiterIsStripped() {
        // VideoToolbox mis-detects picture boundaries when an AUD trails the access unit.
        let withDelimiter = Data([0x00, 0x00, 0x00, 0x01, 0x65, 0xaa, 0x00, 0x00, 0x00, 0x01, 0x09, 0x10])
        #expect(NvstFrameReassembler.strippingTrailingAccessUnitDelimiter(withDelimiter) == Data([0x00, 0x00, 0x00, 0x01, 0x65, 0xaa]))
        // A three-byte start code before the delimiter is handled too.
        let threeByte = Data([0x00, 0x00, 0x00, 0x01, 0x65, 0xaa, 0x00, 0x00, 0x01, 0x09])
        #expect(NvstFrameReassembler.strippingTrailingAccessUnitDelimiter(threeByte) == Data([0x00, 0x00, 0x00, 0x01, 0x65, 0xaa]))
        // A unit that does not end in a delimiter is untouched.
        let plain = Data([0x00, 0x00, 0x00, 0x01, 0x65, 0xaa])
        #expect(NvstFrameReassembler.strippingTrailingAccessUnitDelimiter(plain) == plain)
    }

    @Test func aStartPacketWithoutNearbyAnnexBVideoIsDropped() throws {
        let assembler = NvstFrameReassembler()
        let noise = [UInt8](repeating: 0x81, count: NvstAnnexB.maxPictureHeaderBytes + 8)
        let start = try NvstVideoPacketParser.parse(buildPacket(sequence: 1, frameIndex: 1, flags: 0x05, media: noise))
        #expect(throws: NvstReassemblyDrop.missingStartCode) { _ = try assembler.push(start) }
    }

    @Test func aMidFrameGapNeverEmitsAnIncompleteAccessUnit() throws {
        let assembler = NvstFrameReassembler()
        // No SOF was seen for frame 5, so the assembler fails closed rather than guessing.
        let orphan = try NvstVideoPacketParser.parse(buildPacket(sequence: 9, frameIndex: 5, flags: 0x03, media: [0xab]))
        #expect(throws: NvstReassemblyDrop.awaitingStartOfFrame) { _ = try assembler.push(orphan) }
    }

    @Test func anOversizedAccessUnitIsDroppedRatherThanGrown() throws {
        let assembler = NvstFrameReassembler(maxAccessUnitBytes: 16)
        let start = try NvstVideoPacketParser.parse(buildPacket(sequence: 1, frameIndex: 1, flags: 0x05, media: [0x00, 0x00, 0x00, 0x01, 0x65]))
        #expect(try assembler.push(start) == nil)
        let huge = try NvstVideoPacketParser.parse(buildPacket(sequence: 2, frameIndex: 1, flags: 0x01, media: [UInt8](repeating: 0x11, count: 64)))
        #expect(throws: NvstReassemblyDrop.accessUnitTooLarge(limit: 16)) { _ = try assembler.push(huge) }
    }

    @Test func resetClearsBytesInFlight() throws {
        let assembler = NvstFrameReassembler()
        let start = try NvstVideoPacketParser.parse(buildPacket(sequence: 5, frameIndex: 9, flags: 0x05, media: [0x00, 0x00, 0x00, 0x01, 0x65]))
        #expect(try assembler.push(start) == nil)
        assembler.reset()
        let end = try NvstVideoPacketParser.parse(buildPacket(sequence: 6, frameIndex: 9, flags: 0x03, media: []))
        #expect(throws: NvstReassemblyDrop.awaitingStartOfFrame) { _ = try assembler.push(end) }
    }
}
