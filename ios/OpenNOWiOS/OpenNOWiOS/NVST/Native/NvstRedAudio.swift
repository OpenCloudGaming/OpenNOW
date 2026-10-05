import Foundation

/// RFC 2198 redundant audio: the seat's game audio can arrive as one or more repeats of earlier
/// frames ahead of the current one, so a lost packet can be recovered from a later one.
///
/// Each redundant block is preceded by a four-octet header that carries its payload type, how far
/// back its timestamp is, and — the part that makes this recoverable at all — its payload length.
/// The final block (the primary, which is the frame actually being sent now) has a one-octet header
/// consisting of just its payload type, and its length is whatever remains.
///
/// This mirrors the browser's own splitter deliberately: the browser is what generated the packets
/// on the seat side, so its reading of the format is the one the wire actually carries.
enum NvstRedAudio {
    struct Block: Equatable, Sendable {
        let payloadType: UInt8
        let timestampOffset: UInt32
        let payload: Data
        /// True for the primary block, which is the frame the packet was sent for.
        let isPrimary: Bool
    }

    static let redundantHeaderBytes = 4
    static let primaryHeaderBytes = 1
    /// Far more blocks than any encoder emits; more than this is a malformed packet.
    static let maximumBlocks = 32

    /// Splits one RED payload into its blocks, oldest first, with the primary last. Nil when the
    /// declared lengths do not add up to the packet, which means it is corrupt rather than merely
    /// carrying nothing.
    static func split(_ payload: Data) -> [Block]? {
        let bytes = [UInt8](payload)
        var headerCursor = 0
        var declaredTotal = 0
        var headers: [(payloadType: UInt8, timestampOffset: UInt32, length: Int)] = []
        var isPrimaryHeader = false

        while !isPrimaryHeader {
            guard headerCursor < bytes.count else { return nil }
            let first = bytes[headerCursor]
            let payloadType = first & 0x7F
            if first & 0x80 == 0 {
                // The last header: its payload is whatever the earlier lengths left behind.
                isPrimaryHeader = true
                declaredTotal += primaryHeaderBytes
                let remaining = bytes.count - declaredTotal
                guard remaining >= 0 else { return nil }
                headers.append((payloadType, 0, remaining))
                headerCursor += primaryHeaderBytes
                break
            }
            guard headerCursor + redundantHeaderBytes <= bytes.count else { return nil }
            let timestampOffset = UInt32(bytes[headerCursor + 1]) << 6 | UInt32(bytes[headerCursor + 2] & 0xFC) >> 2
            let length = Int(bytes[headerCursor + 2] & 0x03) << 8 | Int(bytes[headerCursor + 3])
            declaredTotal += length + redundantHeaderBytes
            headers.append((payloadType, timestampOffset, length))
            headerCursor += redundantHeaderBytes
        }

        guard headers.count <= maximumBlocks else { return nil }
        guard declaredTotal <= bytes.count else { return nil }

        var blocks: [Block] = []
        var cursor = headerCursor
        for (index, header) in headers.enumerated() {
            let isPrimary = index == headers.count - 1
            guard cursor + header.length <= bytes.count else { return nil }
            // Sliced from the normalised bytes, never from the caller's `Data`: a decrypted SRTP
            // payload is a slice of the cipher's output, so its indices do not start at zero.
            blocks.append(Block(payloadType: header.payloadType,
                                timestampOffset: header.timestampOffset,
                                payload: Data(bytes[cursor..<(cursor + header.length)]),
                                isPrimary: isPrimary))
            cursor += header.length
        }
        return blocks
    }

    /// Just the frame this packet was sent for, ignoring the repeats behind it.
    static func primary(in payload: Data) -> Block? {
        split(payload)?.last
    }

    /// The first redundant block that repeats `payloadType`, which is what covers a loss: the
    /// repeats are older frames, so the closest one behind the primary is the most useful.
    static func mostRecentRedundant(in payload: Data, payloadType: UInt8) -> Block? {
        split(payload)?.dropLast().last { $0.payloadType == payloadType }
    }
}
