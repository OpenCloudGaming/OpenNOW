import CommonCrypto
import Foundation

/// STUN hole-punching for the NVST "Mjolnir" socket, per the independently observed format
/// (`NattHolePunch::SendPing`, pingVersion=6):
/// - Binding request USERNAME is `pingPayload:localUfrag`; MESSAGE-INTEGRITY is HMAC-SHA1
///   keyed by the remote password (the DESCRIBE password for the punch, the local ICE
///   password for validating inbound requests); FINGERPRINT is CRC-32 of the preceding bytes
///   XOR 0x5354554e.
/// - Responses carry XOR-MAPPED-ADDRESS over the STUN magic cookie / transaction id.

public struct NvstSocketEndpoint: Equatable, Sendable {
    public let host: String
    public let port: UInt16
    public init(host: String, port: UInt16) { self.host = host; self.port = port }
}

public enum NvstStunError: LocalizedError, Equatable, Sendable {
    case notStun
    case invalidFingerprint
    case invalidIntegrity
    case missingUsername
    case missingMappedAddress
    case malformed(String)

    public var errorDescription: String? {
        switch self {
        case .notStun: "Not a STUN packet."
        case .invalidFingerprint: "STUN FINGERPRINT mismatch."
        case .invalidIntegrity: "STUN MESSAGE-INTEGRITY mismatch."
        case .missingUsername: "STUN USERNAME attribute missing."
        case .missingMappedAddress: "STUN XOR-MAPPED-ADDRESS attribute missing."
        case .malformed(let reason): "Malformed STUN packet: \(reason)"
        }
    }
}

private enum NvstCrc32 {
    private static let table: [UInt32] = (0..<256).map { index in
        var value = UInt32(index)
        for _ in 0..<8 {
            value = (value & 1) != 0 ? (value >> 1) ^ 0xedb8_8320 : value >> 1
        }
        return value
    }

    static func crc32(_ data: Data) -> UInt32 {
        var crc: UInt32 = 0xffff_ffff
        for byte in data {
            crc = (crc >> 8) ^ table[Int((crc ^ UInt32(byte)) & 0xff)]
        }
        return crc ^ 0xffff_ffff
    }
}

private enum NvstStunAttribute {
    static let username: UInt16 = 0x0006
    static let messageIntegrity: UInt16 = 0x0008
    static let xorMappedAddress: UInt16 = 0x0020
    static let fingerprint: UInt16 = 0x8028
}

private let stunMagicCookie: UInt32 = 0x2112_a442
private let fingerprintXor: UInt32 = 0x5354_554e
private let stunBindingRequest: UInt16 = 0x0001
private let stunBindingSuccessResponse: UInt16 = 0x0101

public enum NvstStunHolePunch {
    /// Builds an authenticated STUN binding request (header + USERNAME + MESSAGE-INTEGRITY + FINGERPRINT).
    public static func buildBindingRequest(transactionID: Data, username: String, integrityKey: Data) -> Data? {
        guard let username = username.data(using: .utf8) else { return nil }
        return buildAuthenticatedPacket(
            messageType: stunBindingRequest,
            transactionID: transactionID,
            attributes: [(NvstStunAttribute.username, username)],
            integrityKey: integrityKey
        )
    }

    /// Official ping-version 6 PONG: a STUN Binding Success answering an inbound request,
    /// authenticated with our own (local) ICE password.
    public static func buildBindingSuccess(transactionID: Data,
                                           mappedHost: String,
                                           mappedPort: UInt16,
                                           integrityKey: Data) -> Data? {
        guard let mapped = xorMappedAddress(host: mappedHost, port: mappedPort, transactionID: transactionID) else {
            return nil
        }
        return buildAuthenticatedPacket(
            messageType: stunBindingSuccessResponse,
            transactionID: transactionID,
            attributes: [(NvstStunAttribute.xorMappedAddress, mapped)],
            integrityKey: integrityKey
        )
    }

    /// Header + attributes + MESSAGE-INTEGRITY + FINGERPRINT, with both length rewrites RFC 5389
    /// requires: the integrity HMAC sees a length that stops after its own attribute, and the
    /// fingerprint CRC sees the final length.
    static func buildAuthenticatedPacket(messageType: UInt16,
                                         transactionID: Data,
                                         attributes: [(UInt16, Data)],
                                         integrityKey: Data) -> Data? {
        guard transactionID.count == 12 else { return nil }
        var headerWriter = NvstByteWriter(capacity: 20)
        headerWriter.u16BE(messageType)
        headerWriter.u16BE(0) // length placeholder
        headerWriter.bytes([0x21, 0x12, 0xa4, 0x42])
        headerWriter.bytes(transactionID)
        var header = headerWriter.data

        var body = Data()
        for (type, value) in attributes {
            appendAttribute(to: &body, type: type, value: value)
        }
        let integrityPlaceholder = [UInt8](repeating: 0, count: 20)
        appendAttribute(to: &body, type: NvstStunAttribute.messageIntegrity, value: Data(integrityPlaceholder))

        header.replaceSubrange(2..<4, with: lengthField(UInt16(body.count)))

        // HMAC covers the message up to the start of MESSAGE-INTEGRITY (its 4-byte header and
        // 20-byte value are both excluded).
        var integrityInput = header
        integrityInput.append(body.prefix(body.count - 24))
        var hmac = [UInt8](repeating: 0, count: 20)
        integrityKey.withUnsafeBytes { keyBytes in
            integrityInput.withUnsafeBytes { inputBytes in
                CCHmac(CCHmacAlgorithm(kCCHmacAlgSHA1), keyBytes.baseAddress, integrityKey.count, inputBytes.baseAddress, integrityInput.count, &hmac)
            }
        }
        body.replaceSubrange((body.count - 20)..<body.count, with: hmac)

        header.replaceSubrange(2..<4, with: lengthField(UInt16(body.count + 8)))
        var fingerprintInput = header
        fingerprintInput.append(body)
        let fingerprint = NvstCrc32.crc32(fingerprintInput) ^ fingerprintXor
        var fingerprintValue = NvstByteWriter(capacity: 4)
        fingerprintValue.u32BE(fingerprint)
        appendAttribute(to: &body, type: NvstStunAttribute.fingerprint, value: fingerprintValue.data)

        var packet = header
        packet.append(body)
        return packet
    }

    /// The 12-byte transaction id of an inbound STUN message, when it is a Binding Request.
    public static func bindingRequestTransactionID(_ packet: Data) -> Data? {
        guard packet.count >= 20 else { return nil }
        var reader = NvstByteReader(packet)
        guard let messageType = try? reader.u16BE(),
              messageType == stunBindingRequest,
              (try? reader.skip(2)) != nil,
              let cookie = try? reader.u32BE(),
              cookie == stunMagicCookie else { return nil }
        return try? reader.bytes(12)
    }

    /// NATT hole punch: USERNAME = `pingPayload:localUfrag`, HMAC keyed by the remote password.
    public static func buildNattHolePunchRequest(localUfrag: String, pingPayload: String, remotePassword: Data, transactionID: Data) -> Data? {
        let username = "\(pingPayload):\(localUfrag)"
        return buildBindingRequest(transactionID: transactionID, username: username, integrityKey: remotePassword)
    }

    /// Validates FINGERPRINT + MESSAGE-INTEGRITY and extracts the XOR-MAPPED-ADDRESS. A
    /// non-success message type returns nil, indistinguishable from unrelated traffic — callers
    /// that need to tell a seat's STUN error from noise should classify with
    /// `validateBindingResponse` first.
    public static func parseBindingResponse(_ packet: Data, integrityKey: Data, transactionID: Data) throws -> NvstSocketEndpoint? {
        guard try isBindingSuccess(packet, transactionID: transactionID) else { return nil }
        // MESSAGE-INTEGRITY covers the message through its own attribute, with length adjusted.
        guard integrityMatches(packet, key: integrityKey) == true else { throw NvstStunError.invalidIntegrity }
        guard let mappedAttribute = attribute(packet, type: NvstStunAttribute.xorMappedAddress) else {
            throw NvstStunError.missingMappedAddress
        }
        return try mappedEndpoint(mappedAttribute.value, transactionID: transactionID)
    }

    /// Header, transaction and FINGERPRINT checks. False means a well-formed non-success response.
    private static func isBindingSuccess(_ packet: Data, transactionID: Data) throws -> Bool {
        guard packet.count >= 20 else { throw NvstStunError.notStun }
        var reader = NvstByteReader(packet)
        guard let messageType = try? reader.u16BE(),
              let declaredLength = try? reader.u16BE(),
              let cookie = try? reader.u32BE() else { throw NvstStunError.notStun }
        guard cookie == stunMagicCookie else { throw NvstStunError.notStun }
        if messageType != stunBindingSuccessResponse { return false }
        guard packet.count == 20 + Int(declaredLength) else { throw NvstStunError.malformed("length") }
        guard let transaction = try? reader.bytes(12), transaction == transactionID else { throw NvstStunError.malformed("transaction id") }
        // FINGERPRINT covers everything before it.
        guard fingerprintMatches(packet) == true else { throw NvstStunError.invalidFingerprint }
        return true
    }

    /// Decodes a XOR-MAPPED-ADDRESS attribute value into an endpoint.
    private static func mappedEndpoint(_ mapped: Data, transactionID: Data) throws -> NvstSocketEndpoint {
        guard mapped.count >= 8 else { throw NvstStunError.missingMappedAddress }
        var reader = NvstByteReader(mapped)
        guard (try? reader.skip(1)) != nil,
              let family = try? reader.u8(),
              let xorPort = try? reader.u16BE() else { throw NvstStunError.missingMappedAddress }
        let port = xorPort ^ UInt16(truncatingIfNeeded: stunMagicCookie >> 16)
        var host = "?"
        if family == 0x01 {
            let address = ((try? reader.u32BE()) ?? 0) ^ stunMagicCookie
            host = "\(address >> 24 & 0xff).\(address >> 16 & 0xff).\(address >> 8 & 0xff).\(address & 0xff)"
        } else if family == 0x02 {
            host = xorMappedIPv6Host(mapped, transactionID: transactionID) ?? "?"
        }
        return NvstSocketEndpoint(host: host, port: port)
    }

    /// Un-XORs the 16-byte IPv6 address of a XOR-MAPPED-ADDRESS value: the first four bytes against
    /// the magic cookie, the remaining twelve against the transaction ID.
    private static func xorMappedIPv6Host(_ mapped: Data, transactionID: Data) -> String? {
        guard mapped.count >= 20, transactionID.count >= 12 else { return nil }
        var addr = [UInt8](repeating: 0, count: 16)
        for index in 0..<4 { addr[index] = mapped[mapped.startIndex + 4 + index] ^ UInt8((stunMagicCookie >> (8 * (3 - index))) & 0xff) }
        for index in 0..<12 { addr[4 + index] = mapped[mapped.startIndex + 8 + index] ^ transactionID[transactionID.startIndex + index] }
        return Self.formatIPv6(addr)
    }

    /// Builds a XOR-MAPPED-ADDRESS STUN value (family byte + x-port + x-address).
    public static func xorMappedAddress(host: String, port: UInt16, transactionID: Data) -> Data? {
        let addr = host.split(separator: ".").compactMap { UInt8($0) }
        guard addr.count == 4 else { return nil }
        var writer = NvstByteWriter(capacity: 12)
        writer.bytes([0x00, 0x01]) // IPv4
        writer.u16BE(port ^ UInt16(truncatingIfNeeded: stunMagicCookie >> 16))
        let address = UInt32(addr[0]) << 24 | UInt32(addr[1]) << 16 | UInt32(addr[2]) << 8 | UInt32(addr[3])
        writer.u32BE(address ^ stunMagicCookie)
        return writer.data
    }

    /// FINGERPRINT validity, or nil when the attribute is absent.
    private static func fingerprintMatches(_ packet: Data) -> Bool? {
        guard let (offset, value) = attribute(packet, type: NvstStunAttribute.fingerprint) else { return nil }
        guard value.count == 4 else { return false }
        let expected = NvstCrc32.crc32(packet[..<offset]) ^ fingerprintXor
        var reader = NvstByteReader(value)
        return (try? reader.u32BE()) == expected
    }

    /// MESSAGE-INTEGRITY validity, or nil when the attribute is absent.
    private static func integrityMatches(_ packet: Data, key: Data) -> Bool? {
        guard let (offset, value) = attribute(packet, type: NvstStunAttribute.messageIntegrity) else { return nil }
        guard value.count == 20 else { return false }
        var adjusted = packet[..<(offset + 24)]
        adjusted.replaceSubrange(2..<4, with: lengthField(UInt16(offset + 24 - 20)))
        var hmac = [UInt8](repeating: 0, count: 20)
        key.withUnsafeBytes { keyBytes in
            adjusted.withUnsafeBytes { inputBytes in
                CCHmac(CCHmacAlgorithm(kCCHmacAlgSHA1), keyBytes.baseAddress, key.count, inputBytes.baseAddress, inputBytes.count, &hmac)
            }
        }
        return hmac.elementsEqual(value)
    }

    /// Validates a Binding Success as far as its own attributes allow: magic cookie, declared
    /// length and transaction ID always; MESSAGE-INTEGRITY and FINGERPRINT whenever present. ICE
    /// mandates integrity on responses, but a seat that omits an optional attribute must not
    /// silence the RTT feed, so absence degrades to the transaction match rather than failing —
    /// while a present-but-wrong attribute still rejects the packet.
    public static func validateBindingResponse(_ packet: Data, integrityKey: Data, transactionID: Data) -> Bool {
        guard packet.count >= 20 else { return false }
        var reader = NvstByteReader(packet)
        guard let messageType = try? reader.u16BE(),
              let declaredLength = try? reader.u16BE(),
              let cookie = try? reader.u32BE() else { return false }
        guard cookie == stunMagicCookie, messageType == stunBindingSuccessResponse else { return false }
        guard packet.count == 20 + Int(declaredLength) else { return false }
        guard let transaction = try? reader.bytes(12), transaction == transactionID else { return false }
        if let valid = fingerprintMatches(packet), !valid { return false }
        if let valid = integrityMatches(packet, key: integrityKey), !valid { return false }
        return true
    }

    private static func attribute(_ packet: Data, type: UInt16) -> (offset: Int, value: Data)? {
        var reader = NvstByteReader(packet)
        guard (try? reader.skip(20)) != nil else { return nil }
        while reader.remaining >= 4 {
            let attributeOffset = packet.count - reader.remaining
            guard let attributeType = try? reader.u16BE(),
                  let length = try? reader.u16BE(),
                  let value = try? reader.bytes(Int(length)) else { return nil }
            if attributeType == type {
                return (attributeOffset, value)
            }
            let padded = (Int(length) + 3) & ~3
            guard (try? reader.skip(padded - Int(length))) != nil else { return nil }
        }
        return nil
    }

    /// One TLV attribute with RFC 5389's 4-byte padding.
    private static func appendAttribute(to data: inout Data, type: UInt16, value: Data) {
        var writer = NvstByteWriter(capacity: 4 + value.count + 3)
        writer.u16BE(type)
        writer.u16BE(UInt16(value.count))
        writer.bytes(value)
        writer.zeroes(((value.count + 3) & ~3) - value.count)
        data.append(writer.data)
    }

    private static func lengthField(_ value: UInt16) -> Data {
        var writer = NvstByteWriter(capacity: 2)
        writer.u16BE(value)
        return writer.data
    }

    private static func formatIPv6(_ octets: [UInt8]) -> String {
        var groups: [String] = []
        for index in stride(from: 0, to: 16, by: 2) {
            groups.append(String(format: "%02x%02x", octets[index], octets[index + 1]))
        }
        return groups.joined(separator: ":")
    }
}
