import Foundation

/// One reassembled elementary-stream access unit.
public struct NvstAccessUnit: Equatable, Sendable {
    public let frameIndex: UInt32
    public let firstStreamPacketIndex: UInt32
    public let rtpTimestamp: UInt32
    public let isKeyframe: Bool
    public let bytes: Data

    public init(frameIndex: UInt32, firstStreamPacketIndex: UInt32, rtpTimestamp: UInt32, isKeyframe: Bool, bytes: Data) {
        self.frameIndex = frameIndex
        self.firstStreamPacketIndex = firstStreamPacketIndex
        self.rtpTimestamp = rtpTimestamp
        self.isKeyframe = isKeyframe
        self.bytes = bytes
    }
}

public enum NvstReassemblyDrop: Equatable, Sendable {
    /// FEC repair packet, or a packet with no picture data.
    case notPictureData
    /// A mid-frame packet arrived before its start-of-frame; the frame cannot be recovered.
    case awaitingStartOfFrame
    /// The start-of-frame payload had no Annex-B start code inside the GS header window.
    case missingStartCode
    /// A source packet is missing from the middle of the frame. Concatenating across the hole
    /// produces an access unit the decoder rejects as bad data, so the frame is abandoned instead.
    /// `frameIndex` names the abandoned frame: it is what the seat has to be told not to reference
    /// (`0x0301` frame invalidation), and knowing it here saves waiting for the decoder to reject
    /// the *next* frame before the invalidation goes out.
    case sequenceGap(expected: UInt32, received: UInt32, frameIndex: UInt32)
    case accessUnitTooLarge(limit: Int)
    /// The AV1 frame header advertised an access-unit length the assembled frame cannot satisfy.
    case invalidAv1AccessUnitLength(reported: Int, available: Int)
}

/// Reassembles GS-framed packets into whole elementary-stream access units.
///
/// The receiver is deliberately fail-closed: a frame is emitted only when its own
/// start-of-frame was seen and its end-of-frame arrives, so a gap never produces a
/// half-decodable access unit — it produces a drop the feedback plane answers with a PLI.
public final class NvstFrameReassembler: @unchecked Sendable {
    let lock = NSLock()
    private let maxAccessUnitBytes: Int
    private let codec: NVSTVideoCodec
    private var currentFrame: UInt32?
    private var firstStreamPacketIndex: UInt32?
    private var lastStreamPacketIndex: UInt32?
    private var abandonedFrames: UInt64 = 0
    private var bytes = Data()

    public init(maxAccessUnitBytes: Int = 2 * 1024 * 1024, codec: NVSTVideoCodec = .h264) {
        self.maxAccessUnitBytes = max(1, maxAccessUnitBytes)
        self.codec = codec
    }

    /// Frames whose assembly was cut short by the next start-of-frame.
    public var abandonedFrameCount: UInt64 {
        lock.lock()
        defer { lock.unlock() }
        return abandonedFrames
    }

    public func reset() {
        lock.lock()
        defer { lock.unlock() }
        resetLocked()
    }

    /// The AV1 access-unit size advertised by the current frame's `0x81` header, if it had one.
    private var expectedAv1AccessUnitLength: Int?

    /// The GFN cloud AV1 extended header (`0x81`) carries the whole access unit's byte count as a
    /// little-endian `UInt32` at bytes 16..<20. The short (`0x01`) header does not.
    static func av1AccessUnitLength(in payload: Data, codec: NVSTVideoCodec) -> Int? {
        guard codec == .av1, payload.first == 0x81, payload.count >= 20 else { return nil }
        let bytes = [UInt8](payload)
        var value: UInt32 = 0
        for index in (16..<20).reversed() { value = (value << 8) | UInt32(bytes[index]) }
        return Int(value)
    }

    /// Submits a parsed packet. Returns the completed access unit on end-of-frame, `nil` while
    /// the frame is still assembling, or throws the drop reason.
    public func push(_ packet: NvstRtpVideoPacket) throws -> NvstAccessUnit? {
        lock.lock()
        defer { lock.unlock() }

        // Only FEC repair packets are rejected. The picture-data flag is not a reliable gate: a
        // source packet in a later FEC block of the same frame does not always carry it.
        guard !packet.isFec else {
            throw NvstReassemblyDrop.notPictureData
        }

        if packet.isStartOfFrame {
            try beginFrameLocked(packet)
        } else if currentFrame != packet.frameIndex {
            resetLocked()
            throw NvstReassemblyDrop.awaitingStartOfFrame
        } else {
            try appendLocked(packet)
        }

        guard packet.isEndOfFrame else { return nil }
        return try completeFrameLocked(packet)
    }

    /// Starts a new access unit. A start-of-frame while a frame is still assembling abandons that
    /// frame — it used to vanish with no counter, so a stream losing two thirds of its frames
    /// looked healthy.
    private func beginFrameLocked(_ packet: NvstRtpVideoPacket) throws {
        if currentFrame != nil { abandonedFrames += 1 }
        resetLocked()
        expectedAv1AccessUnitLength = Self.av1AccessUnitLength(in: packet.payload, codec: codec)
        guard let stripped = NvstAnnexB.picturePayload(packet.payload, codec: codec) else {
            throw NvstReassemblyDrop.missingStartCode
        }
        guard stripped.count <= maxAccessUnitBytes else {
            resetLocked()
            throw NvstReassemblyDrop.accessUnitTooLarge(limit: maxAccessUnitBytes)
        }
        currentFrame = packet.frameIndex
        firstStreamPacketIndex = packet.streamSequence
        lastStreamPacketIndex = packet.streamSequence
        bytes = stripped
    }

    /// Appends a continuation packet of the frame already assembling.
    ///
    /// Repair packets were skipped, so a break in the stream sequence is a real hole in the picture
    /// data. `dropped` counts nothing here because the reorder buffer saw no reordering — the
    /// packet simply never arrived.
    private func appendLocked(_ packet: NvstRtpVideoPacket) throws {
        if let previous = lastStreamPacketIndex {
            // `streamSequence` is a 24-bit GS field, so compare it modulo 2^24: otherwise the
            // 0xff_ffff -> 0x000000 rollover (~every 2^24 packets, ~half an hour of streaming)
            // reads as a gap and forces a needless keyframe.
            let expected = (previous &+ 1) & 0x00ff_ffff
            if packet.streamSequence != expected {
                resetLocked()
                throw NvstReassemblyDrop.sequenceGap(expected: expected, received: packet.streamSequence, frameIndex: packet.frameIndex)
            }
        }
        guard packet.payload.count <= maxAccessUnitBytes - bytes.count else {
            resetLocked()
            throw NvstReassemblyDrop.accessUnitTooLarge(limit: maxAccessUnitBytes)
        }
        lastStreamPacketIndex = packet.streamSequence
        bytes.append(packet.payload)
    }

    /// Finishes the assembled access unit on end-of-frame.
    private func completeFrameLocked(_ packet: NvstRtpVideoPacket) throws -> NvstAccessUnit? {
        // AV1 is not self-delimiting the way Annex-B is, so trailing bytes from the last packet
        // would be fed to the decoder as if they were OBU data. The GFN cloud 0x81 header states
        // the exact access-unit size, so trim to it — and treat a size the frame cannot satisfy as
        // a corrupt frame rather than passing a truncated OBU stream on.
        if let reported = expectedAv1AccessUnitLength {
            guard reported > 0, reported <= bytes.count else {
                let available = bytes.count
                resetLocked()
                throw NvstReassemblyDrop.invalidAv1AccessUnitLength(reported: reported, available: available)
            }
            bytes = Data(bytes.prefix(reported))
        }
        // A trailing access-unit delimiter confuses VideoToolbox's picture boundary detection.
        // One scan answers both questions — trailing delimiter and keyframe — without copying the
        // access unit. See `NvstAnnexB.scan`.
        let scan = NvstAnnexB.scan(bytes, codec: codec)
        let completed = Self.strippingTrailingAccessUnitDelimiter(bytes, codec: codec, scan: scan)
        let firstIndex = firstStreamPacketIndex ?? packet.streamSequence
        resetLocked()
        guard !completed.isEmpty else { return nil }
        return NvstAccessUnit(
            frameIndex: packet.frameIndex,
            firstStreamPacketIndex: firstIndex,
            rtpTimestamp: packet.timestamp,
            // Truncating a trailing delimiter never removes a keyframe NAL, so the scan of the
            // untruncated buffer answers for the truncated one.
            isKeyframe: scan.isKeyframe,
            bytes: completed
        )
    }

    /// Drops a trailing access-unit delimiter from an assembled access unit.
    ///
    /// The NAL header differs per codec, and reading an HEVC stream with H.264 rules silently never
    /// matches: H.264 puts `nal_unit_type` in the low 5 bits of a one-byte header (AUD is 9), while
    /// HEVC uses a two-byte header whose type is bits 1–6 of the first byte (AUD is 35). AV1 has no
    /// Annex-B NAL framing at all.
    static func strippingTrailingAccessUnitDelimiter(_ bytes: Data,
                                                     codec: NVSTVideoCodec = .h264,
                                                     scan: NvstAnnexB.AccessUnitScan? = nil) -> Data {
        guard let delimiterType = accessUnitDelimiterMatcher(for: codec) else { return bytes }
        let scan = scan ?? NvstAnnexB.scan(bytes, codec: codec)
        guard scan.lastUnitOffset != nil, delimiterType(scan.lastUnitHeader) else { return bytes }
        guard let startCodeOffset = scan.lastStartCodeOffset, startCodeOffset > 0 else { return bytes }
        return bytes.prefix(startCodeOffset)
    }

    /// Whether a NAL header's first byte introduces an access-unit delimiter, or `nil` when the
    /// codec has no such NAL.
    private static func accessUnitDelimiterMatcher(for codec: NVSTVideoCodec) -> ((UInt8) -> Bool)? {
        switch codec {
        case .h264: { $0 & 0x1f == 9 }
        case .hevc: { ($0 >> 1) & 0x3f == 35 }
        case .av1: nil
        }
    }

    private func resetLocked() {
        currentFrame = nil
        firstStreamPacketIndex = nil
        lastStreamPacketIndex = nil
        bytes = Data()
        expectedAv1AccessUnitLength = nil
    }
}

extension NvstReassemblyDrop: LocalizedError {
    public var errorDescription: String? {
        switch self {
        case .notPictureData: "NVST video packet carries no picture data (FEC repair or control)."
        case .awaitingStartOfFrame: "NVST video packet arrived before its start-of-frame."
        case .missingStartCode: "NVST start-of-frame payload has no Annex-B start code."
        case .accessUnitTooLarge(let limit): "NVST access unit exceeded \(limit) bytes."
        case .sequenceGap(let expected, let received, let frameIndex):
            "NVST video packet \(received) arrived where \(expected) was expected; frame \(frameIndex) has a hole."
        case .invalidAv1AccessUnitLength(let reported, let available):
            "NVST AV1 frame header reported \(reported) bytes but only \(available) were assembled."
        }
    }
}
