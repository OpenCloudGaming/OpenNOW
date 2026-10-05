import Foundation

/// RTP parsing + NVST "Mjolnir" GS-extension depacketization, per the wire format
/// independently observed and documented by OpenNOW's MIT `native-opennow-streamer`:
/// video packets carry a fixed 16-byte RTP extension block, profile `0x4753` ("GS"), holding
/// a 24-bit stream sequence, a 32-bit frame index, and SOF/EOF/contains-pic-data flags; the
/// RTP payloads reassemble into H.264 Annex-B access units.

public struct NvstRtpVideoPacket: Equatable, Sendable {
    public let payloadType: UInt8
    public let sequenceNumber: UInt16
    public let timestamp: UInt32
    public let ssrc: UInt32
    public let streamSequence: UInt32
    public let frameIndex: UInt32
    public let flags: NVSTVideoFlag
    /// True when the GS extension's FEC group coordinates mark this as a repair packet.
    /// Repair packets carry no picture data and must never reach the assembler.
    public let isFec: Bool
    /// A frame can span several FEC blocks. Start- and end-of-frame only count on the first and
    /// last block, so these qualify the raw SOF/EOF flags.
    public let fecCurrentBlock: UInt8
    public let fecLastBlock: UInt8
    /// This packet's index within its FEC group. Indices below `fecSourcePackets` are source data;
    /// at or above it, repair data.
    public let fecIndex: UInt32
    /// How many source packets the group protects.
    public let fecSourcePackets: UInt32
    /// The group's repair overhead in percent; parity shard count is `ceil(sources * pct / 100)`.
    public let fecPercentage: UInt32
    public let payload: Data
    /// AAD prefix length (12-byte RTP header + 20-byte GS extension), for the SRTP layer.
    public static let headerAndExtensionLength = 32

    public struct NVSTVideoFlag: OptionSet, Equatable, Sendable {
        public let rawValue: UInt8
        public init(rawValue: UInt8) { self.rawValue = rawValue }

        public static let containsPicData = NVSTVideoFlag(rawValue: 0x01)
        public static let endOfFrame = NVSTVideoFlag(rawValue: 0x02)
        public static let startOfFrame = NVSTVideoFlag(rawValue: 0x04)
    }

    /// A frame's first packet: the SOF flag only counts on the first FEC block.
    public var isStartOfFrame: Bool { flags.contains(.startOfFrame) && fecCurrentBlock == 0 }

    /// A frame's last packet: the EOF flag only counts on the last FEC block.
    public var isEndOfFrame: Bool { flags.contains(.endOfFrame) && fecCurrentBlock == fecLastBlock }

    public static let gsExtensionProfile: UInt16 = 0x4753
}

public enum NvstRtpParseError: LocalizedError, Equatable, Sendable {
    case truncated(String)
    case notRtp
    case missingGsExtension
    case badExtensionLength
    /// The replay window has already seen this packet's index: a security decision, kept apart
    /// from parse failures so stats can tell replays from malformed or unauthenticated packets.
    case replayed

    public var errorDescription: String? {
        switch self {
        case .truncated(let reason): "NVST RTP parse truncated: \(reason)"
        case .notRtp: "NVST RTP parse: not an RTP v2 packet."
        case .missingGsExtension: "NVST RTP parse: missing the 0x4753 GS extension."
        case .badExtensionLength: "NVST RTP parse: GS extension length is not 16 bytes."
        case .replayed: "NVST SRTP packet rejected by the replay window."
        }
    }
}

public enum NvstVideoPacketParser {
    /// The RTP wire header every media leg shares. The extension header itself stays unread:
    /// video validates it as the GS extension, SRTP only needs its length to find the payload.
    public struct RtpHeader: Sendable, Equatable {
        public var firstByte: UInt8
        public var payloadType: UInt8
        public var sequenceNumber: UInt16
        public var timestamp: UInt32
        public var ssrc: UInt32
        public var hasExtension: Bool
        /// Byte offset just past the fixed header and CSRC list: the extension header if
        /// `hasExtension`, otherwise the payload.
        public var extensionOffset: Int
    }

    /// Reads the fixed RTP header and skips any CSRC list, leaving the reader at the extension
    /// header (or the payload). Both `NvstVideoReceiver.unprotect` and `parse` consume this, so
    /// a packet is never header-parsed twice.
    public static func readHeader(_ reader: inout NvstByteReader) throws -> RtpHeader {
        guard reader.remaining >= 12 else { throw NvstRtpParseError.truncated("RTP header") }
        let first = try reader.u8()
        guard first & 0xc0 == 0x80 else { throw NvstRtpParseError.notRtp }
        let payloadType = try reader.u8() & 0x7f
        let sequenceNumber = try reader.u16BE()
        let timestamp = try reader.u32BE()
        let ssrc = try reader.u32BE()
        let csrcCount = Int(first & 0x0f)
        guard reader.remaining >= csrcCount * 4 else { throw NvstRtpParseError.truncated("RTP CSRC list") }
        try reader.skip(csrcCount * 4)
        return RtpHeader(firstByte: first, payloadType: payloadType, sequenceNumber: sequenceNumber,
                         timestamp: timestamp, ssrc: ssrc, hasExtension: first & 0x10 != 0,
                         extensionOffset: reader.offset)
    }

    /// Parses one decrypted SRTP RTP datagram (12-byte header + 20-byte GS extension + payload).
    public static func parse(_ packet: Data) throws -> NvstRtpVideoPacket {
        var reader = NvstByteReader(packet)
        let header = try readHeader(&reader)
        return try parseAfterHeader(&reader, header: header)
    }

    /// Parses a packet whose RTP wire header was already read — the position the SRTP receiver
    /// is in after unprotecting, where re-reading the header would be pure waste.
    public static func parse(_ packet: Data, header: RtpHeader) throws -> NvstRtpVideoPacket {
        var reader = NvstByteReader(packet)
        try reader.skip(header.extensionOffset)
        return try parseAfterHeader(&reader, header: header)
    }

    private static func parseAfterHeader(_ reader: inout NvstByteReader, header: RtpHeader) throws -> NvstRtpVideoPacket {
        guard header.hasExtension else { throw NvstRtpParseError.missingGsExtension }

        guard reader.remaining >= 4 else { throw NvstRtpParseError.truncated("GS extension header") }
        let profile = try reader.u16BE()
        let extensionLength = Int(try reader.u16BE()) * 4
        guard profile == NvstRtpVideoPacket.gsExtensionProfile else { throw NvstRtpParseError.missingGsExtension }
        guard extensionLength == 16 else { throw NvstRtpParseError.badExtensionLength }
        guard reader.remaining >= extensionLength else { throw NvstRtpParseError.truncated("GS extension") }

        let sequenceWord = try reader.u32LE()
        let streamSequence = (sequenceWord >> 8) & 0x00ff_ffff
        let frameIndex = try reader.u32LE()
        // Flags live in the low nibble of the little-endian word at extension offset 8; the
        // multi-FEC-block counters share that word's top byte.
        let flagsWord = try reader.u32LE()
        let flags = NvstRtpVideoPacket.NVSTVideoFlag(rawValue: UInt8(flagsWord & 0x0f))
        let multiFecBlocks = UInt8((flagsWord >> 24) & 0xff)
        // FEC group coordinates: a repair packet is one whose index within the group is at or past
        // the source-packet count, while the group carries a non-zero repair percentage.
        let fecWord = try reader.u32LE()
        let fecPercentage = (fecWord >> 4) & 0xff
        let fecIndex = (fecWord >> 12) & 0x3ff
        let fecSourcePackets = (fecWord >> 22) & 0x3ff
        let isFec = fecPercentage != 0 && fecIndex >= fecSourcePackets

        var payload = reader.unread
        // `<=` is deliberate: RFC 3550 lets the padding count equal the whole remaining region,
        // and an all-padding payload then fails the start-code gate below like any empty one.
        if header.firstByte & 0x20 != 0, let padding = payload.last, padding > 0, Int(padding) <= payload.count {
            payload = payload.prefix(payload.count - Int(padding))
        }

        return NvstRtpVideoPacket(
            payloadType: header.payloadType,
            sequenceNumber: header.sequenceNumber,
            timestamp: header.timestamp,
            ssrc: header.ssrc,
            streamSequence: streamSequence,
            frameIndex: frameIndex,
            flags: flags,
            isFec: isFec,
            fecCurrentBlock: (multiFecBlocks >> 4) & 0x03,
            fecLastBlock: (multiFecBlocks >> 6) & 0x03,
            fecIndex: fecIndex,
            fecSourcePackets: fecSourcePackets,
            fecPercentage: fecPercentage,
            payload: Data(payload)
        )
    }
}
