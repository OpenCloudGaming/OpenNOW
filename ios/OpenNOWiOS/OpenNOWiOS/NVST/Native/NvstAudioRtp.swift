import Foundation

/// Turns encoded Opus packets into the RTP the microphone stream is carried as.
///
/// The bundle's mic rides SRTP on the same 5 ms grid as everything else, so the timestamp advances
/// by exactly one packet's worth of samples and the sequence number by one. Both have to start
/// somewhere the seat can follow: NVST has no SDP exchange for this stream, so the SSRC is the
/// vendor's deterministic value announced in ANNOUNCE and the sequence starts from a random base.
public struct NvstAudioRtpPacketizer: Sendable {
    /// The deterministic sender SSRC the vendor's transport binds the bundle mic by.
    public static let microphoneSSRC: UInt32 = 1
    public static let opusPayloadType: UInt8 = 111
    public static let clockRate: UInt32 = 48000

    public let ssrc: UInt32
    public let payloadType: UInt8

    private(set) public var sequenceNumber: UInt16
    private(set) public var timestamp: UInt32
    private var isFirstPacket = true

    public init(ssrc: UInt32 = microphoneSSRC,
                payloadType: UInt8 = opusPayloadType,
                initialSequenceNumber: UInt16,
                initialTimestamp: UInt32) {
        self.ssrc = ssrc
        self.payloadType = payloadType
        self.sequenceNumber = initialSequenceNumber
        self.timestamp = initialTimestamp
    }

    /// Framed one Opus packet: a 12-byte RTP header followed by the payload.
    ///
    /// The marker bit is set on the first packet only. For audio the marker marks the start of a
    /// talkspurt rather than a frame boundary, and setting it per packet would have the seat treat
    /// every 5 ms as a new talkspurt.
    public mutating func packet(payload: Data, framesPerPacket: Int) -> Data {
        var header = [UInt8](repeating: 0, count: 12)
        let marker: UInt8 = isFirstPacket ? 0x80 : 0x00
        header[0] = 0x80                    // version 2, no padding, no extension, no CSRC count
        header[1] = marker | (payloadType & 0x7F)
        header[2] = UInt8(sequenceNumber >> 8)
        header[3] = UInt8(sequenceNumber & 0xFF)
        header[4] = UInt8((timestamp >> 24) & 0xFF)
        header[5] = UInt8((timestamp >> 16) & 0xFF)
        header[6] = UInt8((timestamp >> 8) & 0xFF)
        header[7] = UInt8(timestamp & 0xFF)
        header[8] = UInt8((ssrc >> 24) & 0xFF)
        header[9] = UInt8((ssrc >> 16) & 0xFF)
        header[10] = UInt8((ssrc >> 8) & 0xFF)
        header[11] = UInt8(ssrc & 0xFF)

        sequenceNumber &+= 1
        timestamp &+= UInt32(framesPerPacket)
        isFirstPacket = false
        return Data(header) + payload
    }
}

/// The SRTP protection the microphone's packets are encrypted with, once the DTLS handshake has
/// yielded the master values. Kept next to the packetizer because the two are used together and the
/// keys are useless apart from the packets they protect.
enum NvstAudioSrtpDirection: Sendable {
    /// The values this endpoint protects its outbound stream with.
    case outbound(key: Data, salt: Data)
    /// The values it unprotects the peer's stream with.
    case inbound(key: Data, salt: Data)

    var key: Data {
        switch self {
        case .outbound(let key, _), .inbound(let key, _): key
        }
    }

    var salt: Data {
        switch self {
        case .outbound(_, let salt), .inbound(_, let salt): salt
        }
    }

    /// The direction's pair from a completed handshake: we send with the client values and receive
    /// with the server's, which is the ordering RFC 5764 fixes.
    static func directions(from keys: NvstBundleSrtpKeys) -> (outbound: NvstAudioSrtpDirection, inbound: NvstAudioSrtpDirection) {
        (.outbound(key: keys.clientMasterKey, salt: keys.clientMasterSalt),
         .inbound(key: keys.serverMasterKey, salt: keys.serverMasterSalt))
    }
}
