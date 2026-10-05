import Darwin
import Foundation

/// Datagram classification for the mjolnir socket (the raw video UDP socket also carries
/// STUN and, transiently, DTLS traffic on the two-socket bundle).
public enum NvstDatagramClassifier {
    private static let stunMagicCookie: Data = Data([0x21, 0x12, 0xa4, 0x42])

    public static func looksLikeSTUN(_ datagram: Data) -> Bool {
        datagram.count >= 20 && datagram[4..<8] == stunMagicCookie
    }

    public static func looksLikeDTLS(_ datagram: Data) -> Bool {
        guard let first = datagram.first else { return false }
        return first >= 20 && first <= 63
    }

    public static func looksLikeRTP(_ datagram: Data) -> Bool {
        datagram.count >= 12 && ((datagram[0] >> 6) == 2) && !looksLikeSTUN(datagram) && !looksLikeDTLS(datagram)
    }

    public static func rtpSSRC(_ datagram: Data) -> UInt32? {
        guard looksLikeRTP(datagram) else { return nil }
        return (UInt32(datagram[8]) << 24) | (UInt32(datagram[9]) << 16) | (UInt32(datagram[10]) << 8) | UInt32(datagram[11])
    }
}

/// Determines the outbound routed IPv4 (the "advertised NIC address") by connecting a UDP
/// socket toward a public probe and reading the local address — the same trick upstream uses
/// for the two-socket model.
public enum NvstRoutedIPv4 {
    public static func discover() -> String? {
        let descriptor = socket(AF_INET, SOCK_DGRAM, 0)
        guard descriptor >= 0 else { return nil }
        var destination = sockaddr_in()
        destination.sin_family = sa_family_t(AF_INET)
        destination.sin_port = UInt16(9).bigEndian
        destination.sin_addr.s_addr = inetAddr("1.1.1.1")
        let connectResult = withUnsafePointer(to: &destination) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                connect(descriptor, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        defer { close(descriptor) }
        guard connectResult == 0 else { return nil }
        var local = sockaddr_in()
        var localLength = socklen_t(MemoryLayout<sockaddr_in>.size)
        let nameResult = withUnsafeMutablePointer(to: &local) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                getsockname(descriptor, $0, &localLength)
            }
        }
        guard nameResult == 0 else { return nil }
        let address = UInt32(bigEndian: local.sin_addr.s_addr)
        let first = address >> 24 & 0xff
        let second = address >> 16 & 0xff
        let third = address >> 8 & 0xff
        let fourth = address & 0xff
        if first == 0 || first == 127 { return nil }
        return "\(first).\(second).\(third).\(fourth)"
    }

    private static func inetAddr(_ dottedQuad: String) -> in_addr_t {
        var result: in_addr_t = 0
        for octet in dottedQuad.split(separator: ".").compactMap({ UInt32($0) }) {
            result = (result << 8) | octet
        }
        return result
    }
}
