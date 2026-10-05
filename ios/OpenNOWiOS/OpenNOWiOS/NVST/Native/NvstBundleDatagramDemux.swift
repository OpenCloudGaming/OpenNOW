import Foundation

/// Splits the bundle socket's traffic into the two things that share it.
///
/// The seat muxes game audio onto the same UDP port as the DTLS association, exactly as a browser
/// does: SRTP is not carried *inside* DTLS, it runs alongside it. The two are told apart by the
/// first byte of each datagram, which is the record content type for DTLS and the RTP version bits
/// for SRTP. Getting this wrong is not subtle — feeding an audio packet to the DTLS record layer
/// stalls the handshake or tears the association down — so the classification is stated once and
/// tested against the boundary values.
enum NvstBundleDatagramDemux {
    enum Datagram: Equatable, Sendable {
        case dtls(Data)
        case srtp(Data)
        /// A STUN message. It shares this socket because the seat's front end routes by it, not
        /// because it is media; it is classified here so it is never fed to the record layer.
        case stun(Data)
        /// Neither: a stray or corrupt packet, to be counted rather than fed anywhere.
        case unknown
    }

    /// DTLS record content types run 20…63 (change-cipher-spec through the DTLS-only range).
    static let dtlsRecordTypes: ClosedRange<UInt8> = 20...63
    /// RTP and RTCP both set the version field to 2, so their first byte is 128…191.
    static let rtpFirstBytes: ClosedRange<UInt8> = 128...191
    static let stunMagicCookie: UInt32 = 0x2112_a442

    static func classify(_ datagram: Data) -> Datagram {
        guard let first = datagram.first else { return .unknown }
        if dtlsRecordTypes.contains(first) { return .dtls(datagram) }
        if rtpFirstBytes.contains(first) { return .srtp(datagram) }
        if isStun(datagram) { return .stun(datagram) }
        return .unknown
    }

    /// A STUN header is 20 bytes, starts with two zero bits, and carries the magic cookie at offset
    /// four. The cookie is what makes this safe to look for on a socket that also carries media.
    static func isStun(_ datagram: Data) -> Bool {
        guard datagram.count >= 20, datagram[datagram.startIndex] & 0xC0 == 0 else { return false }
        let base = datagram.startIndex + 4
        let cookie = UInt32(datagram[base]) << 24
            | UInt32(datagram[base + 1]) << 16
            | UInt32(datagram[base + 2]) << 8
            | UInt32(datagram[base + 3])
        return cookie == stunMagicCookie
    }
}
