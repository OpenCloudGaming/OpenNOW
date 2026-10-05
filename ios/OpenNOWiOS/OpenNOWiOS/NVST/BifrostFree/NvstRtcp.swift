import Foundation

/// RTCP/SRTCP builders for the NVST `rtcp1` feedback channel.
///
/// Upstream's independently observed rule: the official client sends SRTCP Receiver Reports
/// and PLI over an SCTP data channel on the WebRTC bundle ("RTCP over SCTP is a must for One
/// SDK video to function"). SRTCP reuses the SRTP master key with labels 0x03 (key) / 0x05
/// (salt) and the RFC 7714 §9.2 SRTCP GCM IV.
public struct NvstRtcpReportBlock: Equatable, Sendable {
    public let sourceSSRC: UInt32
    public let fractionLost: UInt8
    public let cumulativeLost: UInt32
    public let extendedHighestSequence: UInt32
    public let interarrivalJitter: UInt32
    public let lastSenderReportTimestamp: UInt32
    public let delaySinceLastSenderReport: UInt32

    public init(sourceSSRC: UInt32,
                fractionLost: UInt8,
                cumulativeLost: UInt32,
                extendedHighestSequence: UInt32,
                interarrivalJitter: UInt32,
                lastSenderReportTimestamp: UInt32 = 0,
                delaySinceLastSenderReport: UInt32 = 0) {
        self.sourceSSRC = sourceSSRC
        self.fractionLost = fractionLost
        self.cumulativeLost = cumulativeLost & 0x00ff_ffff
        self.extendedHighestSequence = extendedHighestSequence
        self.interarrivalJitter = interarrivalJitter
        self.lastSenderReportTimestamp = lastSenderReportTimestamp
        self.delaySinceLastSenderReport = delaySinceLastSenderReport
    }
}

public enum NvstRtcp {
    public static let receiverReportPayloadType: UInt8 = 201
    public static let payloadSpecificFeedbackPayloadType: UInt8 = 206
    /// RFC 4585 transport-layer feedback, which is where Generic NACK lives.
    public static let transportFeedbackPayloadType: UInt8 = 205

    /// RFC 3550 Receiver Report (compound RR with no embedded SDES).
    public static func receiverReport(ssrc: UInt32, blocks: [NvstRtcpReportBlock]) -> Data {
        // The RC field is 5 bits wide, so more than 31 blocks cannot be expressed on the wire.
        let blocks = Array(blocks.prefix(31))
        let wordCount = 2 + blocks.count * 6 // header (2 words incl sender ssrc) + 6 words/block
        var writer = NvstByteWriter(capacity: wordCount * 4)
        writer.u8(0x80 | UInt8(blocks.count))
        writer.u8(receiverReportPayloadType)
        writer.u16BE(UInt16(wordCount - 1))
        writer.u32BE(ssrc)
        for block in blocks {
            writer.u32BE(block.sourceSSRC)
            writer.u32BE((UInt32(block.fractionLost) << 24) | block.cumulativeLost)
            writer.u32BE(block.extendedHighestSequence)
            writer.u32BE(block.interarrivalJitter)
            writer.u32BE(block.lastSenderReportTimestamp)
            writer.u32BE(block.delaySinceLastSenderReport)
        }
        return writer.data
    }

    /// RFC 4585 PT=206 FMT=1 (PLI) feedback packet.
    public static func pictureLossIndication(senderSSRC: UInt32, mediaSSRC: UInt32) -> Data {
        var writer = NvstByteWriter(capacity: 12)
        writer.u8(0x81)
        writer.u8(payloadSpecificFeedbackPayloadType)
        writer.u16BE(2) // length = sender SSRC + media SSRC
        writer.u32BE(senderSSRC)
        writer.u32BE(mediaSSRC)
        return writer.data
    }

    /// RFC 4585 PT=205 FMT=1: Generic NACK, asking the sender to retransmit specific packets.
    ///
    /// We have announced `video[0].enableRtpNack:1` from the captured baseline for as long as this
    /// client has existed and then never sent one, so every gap — however small — cost a full
    /// keyframe instead of the handful of packets actually missing. Each FCI entry names one packet
    /// id plus a 16-bit mask of the packets following it, so a 17-packet run needs one entry.
    ///
    /// `missing` is taken in RTP sequence order; entries are emitted greedily.
    /// RFC 4585 PT=206 FMT=1 (Generic NACK) feedback packet. Duplicate sequence numbers are
    /// collapsed, and losses beyond `maximumNackEntries` are deliberately not named — a burst
    /// that large is what the keyframe path recovers from faster.
    public static func genericNack(senderSSRC: UInt32, mediaSSRC: UInt32, missing: [UInt16]) -> Data? {
        var seen = Set<UInt16>()
        let missing = missing.filter { seen.insert($0).inserted }
        guard !missing.isEmpty else { return nil }
        var entries: [(pid: UInt16, blp: UInt16)] = []
        var index = missing.startIndex
        while index < missing.endIndex, entries.count < maximumNackEntries {
            let pid = missing[index]
            var blp: UInt16 = 0
            var next = missing.index(after: index)
            while next < missing.endIndex {
                let delta = Int(missing[next] &- pid)
                guard delta >= 1, delta <= 16 else { break }
                blp |= UInt16(1) << UInt16(delta - 1)
                next = missing.index(after: next)
            }
            entries.append((pid, blp))
            index = next
        }
        var writer = NvstByteWriter(capacity: 12 + entries.count * 4)
        // V=2, P=0, FMT=1 (Generic NACK).
        writer.u8(0x81)
        writer.u8(transportFeedbackPayloadType)
        // Length in 32-bit words minus one: 2 SSRC words plus one word per FCI entry.
        writer.u16BE(UInt16(2 + entries.count))
        writer.u32BE(senderSSRC)
        writer.u32BE(mediaSSRC)
        for entry in entries {
            writer.u16BE(entry.pid)
            writer.u16BE(entry.blp)
        }
        return writer.data
    }

    /// A NACK naming more than this is a loss burst a keyframe recovers from faster than
    /// retransmission would.
    public static let maximumNackEntries = 8

    /// RFC 7714 §9.2 SRTCP GCM IV: salt XOR (SSRC at bytes 2..6, SRTCP index at 6..10).
    public static func srtcpGcmIV(sessionSalt: Data, ssrc: UInt32, srtcpIndex: UInt32) throws -> Data {
        guard sessionSalt.count == 12 else { throw SrtpCryptoError.invalidNonce }
        var iv = [UInt8](sessionSalt)
        var words = NvstByteWriter(capacity: 8)
        words.u32BE(ssrc)
        words.u32BE(srtcpIndex)
        let wordBytes = [UInt8](words.data)
        for index in 0..<4 { iv[2 + index] ^= wordBytes[index] }
        for index in 0..<4 { iv[6 + index] ^= wordBytes[4 + index] }
        return Data(iv)
    }

    /// RFC 3711 SRTCP index trailer: 4 bytes with the encrypted-flag (`S`) as the MSB and the
    /// 31-bit SRTCP index in the remaining bits.
    public static func srtcpIndexTrailer(index: UInt32, encrypted: Bool) -> Data {
        var value = index & 0x7fff_ffff
        if encrypted { value |= 0x8000_0000 }
        var writer = NvstByteWriter(capacity: 4)
        writer.u32BE(value)
        return writer.data
    }
}
