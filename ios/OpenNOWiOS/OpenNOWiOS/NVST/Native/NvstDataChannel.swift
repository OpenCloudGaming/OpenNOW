import Foundation

/// How a stream guarantees delivery, in the three shapes the seat's channels use.
public enum NvstSctpReliability: Equatable, Sendable {
    case reliable
    /// Bounded retransmissions: the channel is unreliable after `maximumRetransmits` attempts.
    case partialReliableRetransmits(UInt32)
    /// Bounded lifetime in milliseconds, which is how the vendor names its 300 ms channels.
    case partialReliableTimed(UInt32)
}

/// One of the bundle's SCTP streams. The seat validates both the label and the stream id, so the
/// two are pinned together rather than derived independently.
public struct NvstSctpChannelDefinition: Equatable, Sendable {
    public let label: String
    public let streamID: UInt16
    public let reliability: NvstSctpReliability

    public init(_ label: String, streamID: UInt16, reliability: NvstSctpReliability = .reliable) {
        self.label = label
        self.streamID = streamID
        self.reliability = reliability
    }
}

public enum NvstSctpChannelProfile {
    /// The eight channels the seat expects, in the order that assigns their stream ids.
    ///
    /// As the DTLS client we are handed the even ids in creation order, so creating these in order
    /// reproduces the mapping exactly. The ids are not cosmetic: opening `rtcp_on_sctp_private` on
    /// id 12 instead of 14 made the seat reset every stream and close DTLS, because that id belongs
    /// to the cursor channel.
    public static let official: [NvstSctpChannelDefinition] = [
        NvstSctpChannelDefinition("control_channel_reliable", streamID: 0),
        NvstSctpChannelDefinition("custom_message_on_sctp_private_reliable", streamID: 2),
        NvstSctpChannelDefinition("custom_message_on_sctp_private_partially_reliable", streamID: 4, reliability: .partialReliableTimed(300)),
        NvstSctpChannelDefinition("control_channel_partially_reliable", streamID: 6, reliability: .partialReliableTimed(300)),
        NvstSctpChannelDefinition("control_channel_unreliable", streamID: 8, reliability: .partialReliableRetransmits(0)),
        NvstSctpChannelDefinition("input_channel_partially_reliable", streamID: 10, reliability: .partialReliableTimed(300)),
        NvstSctpChannelDefinition("cursor_channel", streamID: 12),
        NvstSctpChannelDefinition("rtcp_on_sctp_private", streamID: 14),
    ]

    public static func channel(labelled label: String) -> NvstSctpChannelDefinition? {
        official.first { $0.label == label }
    }
}

/// RFC 8832 data channel establishment, the in-band OPEN/ACK exchange WebRTC runs over SCTP.
///
/// The seat does not open our channels for us: it validates the stream each label arrives on, so the
/// OPEN must name the right id and the right reliability or the stream is torn down.
enum NvstDataChannelProtocol {
    /// PPID every DCEP message rides, distinct from the channel payloads themselves. DCEP is 50;
    /// 56 is WebRTC's "binary, empty" payload PPID and is not the establishment protocol.
    static let ppid: UInt32 = 50

    enum MessageType: UInt8 {
        case open = 0x03
        case ack = 0x02
    }

    enum ChannelType: UInt8 {
        case reliable = 0x00
        case partialReliableRetransmits = 0x01
        case partialReliableTimed = 0x02

        static func forReliability(_ reliability: NvstSctpReliability) -> ChannelType {
            switch reliability {
            case .reliable: .reliable
            case .partialReliableRetransmits: .partialReliableRetransmits
            case .partialReliableTimed: .partialReliableTimed
            }
        }
    }

    struct OpenMessage: Equatable, Sendable {
        let channelType: ChannelType
        let priority: UInt16
        let reliabilityParameter: UInt32
        let label: String
        let protocolName: String
    }

    static func encodedOpen(for channel: NvstSctpChannelDefinition, priority: UInt16 = 0) -> Data {
        encodedOpen(channelType: ChannelType.forReliability(channel.reliability),
                    priority: priority,
                    reliabilityParameter: reliabilityParameter(for: channel.reliability),
                    label: channel.label)
    }

    static func encodedAck() -> Data {
        Data([MessageType.ack.rawValue])
    }

    static func reliabilityParameter(for reliability: NvstSctpReliability) -> UInt32 {
        switch reliability {
        case .reliable: 0
        case .partialReliableRetransmits(let retransmits): retransmits
        case .partialReliableTimed(let milliseconds): milliseconds
        }
    }

    private static func encodedOpen(channelType: ChannelType, priority: UInt16, reliabilityParameter: UInt32, label: String, protocolName: String = "") -> Data {
        let labelBytes = Array(label.utf8)
        let protocolBytes = Array(protocolName.utf8)
        var bytes: [UInt8] = [
            MessageType.open.rawValue,
            channelType.rawValue,
        ]
        bytes += be16(priority)
        bytes += be32(reliabilityParameter)
        bytes += be16(UInt16(labelBytes.count))
        bytes += be16(UInt16(protocolBytes.count))
        bytes += labelBytes
        bytes += protocolBytes
        return Data(bytes)
    }

    /// Decodes whichever DCEP message `data` holds. Nil for anything that is not one.
    static func decode(_ data: Data) -> (type: MessageType, open: OpenMessage?)? {
        let bytes = [UInt8](data)
        guard let first = bytes.first, let type = MessageType(rawValue: first) else { return nil }
        guard type == .open else { return bytes.count == 1 ? (type, nil) : nil }
        guard bytes.count >= 12 else { return nil }
        let channelRaw = bytes[1]
        guard let channelType = ChannelType(rawValue: channelRaw) else { return nil }
        let priority = uint16(bytes, 2)
        let reliabilityParameter = uint32(bytes, 4)
        let labelLength = Int(uint16(bytes, 8))
        let protocolLength = Int(uint16(bytes, 10))
        guard bytes.count == 12 + labelLength + protocolLength else { return nil }
        let labelBytes = bytes[12..<(12 + labelLength)]
        let protocolBytes = bytes[(12 + labelLength)..<(12 + labelLength + protocolLength)]
        let open = OpenMessage(
            channelType: channelType,
            priority: priority,
            reliabilityParameter: reliabilityParameter,
            label: String(decoding: labelBytes, as: UTF8.self),
            protocolName: String(decoding: protocolBytes, as: UTF8.self)
        )
        return (type, open)
    }

    private static func be16(_ value: UInt16) -> [UInt8] {
        [UInt8(value >> 8), UInt8(value & 0xFF)]
    }

    private static func be32(_ value: UInt32) -> [UInt8] {
        [UInt8((value >> 24) & 0xFF), UInt8((value >> 16) & 0xFF), UInt8((value >> 8) & 0xFF), UInt8(value & 0xFF)]
    }

    private static func uint16(_ bytes: [UInt8], _ offset: Int) -> UInt16 {
        UInt16(bytes[offset]) << 8 | UInt16(bytes[offset + 1])
    }

    private static func uint32(_ bytes: [UInt8], _ offset: Int) -> UInt32 {
        UInt32(bytes[offset]) << 24 | UInt32(bytes[offset + 1]) << 16 | UInt32(bytes[offset + 2]) << 8 | UInt32(bytes[offset + 3])
    }
}
