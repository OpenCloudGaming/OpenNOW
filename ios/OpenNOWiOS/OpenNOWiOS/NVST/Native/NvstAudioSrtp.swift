import CryptoKit
import Foundation

/// One SRTP packet as it arrives on the bundle socket, split into the parts the cipher needs.
///
/// The authentication covers the RTP header *including* any CSRC list and header extension the seat
/// chose to send, so the header is captured as bytes rather than as fields: an audio stream that
/// carries an extension would otherwise authenticate a shorter span than the sender did and every
/// packet would fail its tag.
public struct NvstAudioRtpPacket: Equatable, Sendable {
    public let marker: Bool
    public let payloadType: UInt8
    public let sequenceNumber: UInt16
    public let timestamp: UInt32
    public let ssrc: UInt32
    /// Everything the SRTP tag authenticates, up to the payload.
    public let authenticatedHeader: Data
    public let encryptedPayload: Data
    public let authenticationTag: Data

    public static let minimumHeaderBytes = 12

    /// Splits `datagram` into header, ciphertext and tag. Nil when it is not a parseable RTP packet
    /// with a tag of the negotiated length. A `tagLength` of 0 splits an unprotected RTP packet,
    /// which is what the send path starts from.
    public static func parse(_ datagram: Data, tagLength: Int) -> NvstAudioRtpPacket? {
        let bytes = [UInt8](datagram)
        guard tagLength >= 0, bytes.count > minimumHeaderBytes + tagLength else { return nil }
        guard bytes[0] >> 6 == 2 else { return nil }
        let hasPadding = bytes[0] & 0x20 != 0
        let hasExtension = bytes[0] & 0x10 != 0
        let csrcCount = Int(bytes[0] & 0x0F)
        var headerLength = minimumHeaderBytes + csrcCount * 4
        guard headerLength <= bytes.count else { return nil }
        if hasExtension {
            guard headerLength + 4 <= bytes.count else { return nil }
            let words = Int(bytes[headerLength + 2]) << 8 | Int(bytes[headerLength + 3])
            headerLength += 4 + words * 4
            guard headerLength <= bytes.count else { return nil }
        }
        // A padded payload carries its padding length in the final byte; the cipher still covers it,
        // so the split does not need to trim it, but the length byte must exist.
        if hasPadding, bytes.count - headerLength <= tagLength { return nil }
        let payloadEnd = bytes.count - tagLength
        guard payloadEnd >= headerLength else { return nil }
        return NvstAudioRtpPacket(
            marker: bytes[1] & 0x80 != 0,
            payloadType: bytes[1] & 0x7F,
            sequenceNumber: UInt16(bytes[2]) << 8 | UInt16(bytes[3]),
            timestamp: UInt32(bytes[4]) << 24 | UInt32(bytes[5]) << 16 | UInt32(bytes[6]) << 8 | UInt32(bytes[7]),
            ssrc: UInt32(bytes[8]) << 24 | UInt32(bytes[9]) << 16 | UInt32(bytes[10]) << 8 | UInt32(bytes[11]),
            // Built from the normalised bytes: `datagram` may be a slice whose indices do not start
            // at zero, which `subdata(in:)` would read out of range.
            authenticatedHeader: Data(bytes[0..<headerLength]),
            encryptedPayload: Data(bytes[headerLength..<payloadEnd]),
            authenticationTag: Data(bytes[payloadEnd..<bytes.count])
        )
    }

    /// The header with the marker bit cleared, as a packet we are sending carries it.
    public static func headerBytes(payloadType: UInt8,
                                   marker: Bool,
                                   sequenceNumber: UInt16,
                                   timestamp: UInt32,
                                   ssrc: UInt32) -> Data {
        var bytes = [UInt8](repeating: 0, count: minimumHeaderBytes)
        bytes[0] = 0x80
        bytes[1] = (marker ? 0x80 : 0x00) | (payloadType & 0x7F)
        bytes[2] = UInt8(sequenceNumber >> 8)
        bytes[3] = UInt8(sequenceNumber & 0xFF)
        bytes[4] = UInt8((timestamp >> 24) & 0xFF)
        bytes[5] = UInt8((timestamp >> 16) & 0xFF)
        bytes[6] = UInt8((timestamp >> 8) & 0xFF)
        bytes[7] = UInt8(timestamp & 0xFF)
        bytes[8] = UInt8((ssrc >> 24) & 0xFF)
        bytes[9] = UInt8((ssrc >> 16) & 0xFF)
        bytes[10] = UInt8((ssrc >> 8) & 0xFF)
        bytes[11] = UInt8(ssrc & 0xFF)
        return Data(bytes)
    }
}

public struct NvstAudioSrtp {
    private enum Cipher {
        case gcm(SymmetricKey)
        case counterMode(encryptionKey: Data, authenticationKey: SymmetricKey)
    }

    private let cipher: Cipher
    private let sessionSalt: Data
    private let tagLength: Int

    public init(masterKey: Data, masterSalt: Data, profile: NVSTSrtpProfile) throws {
        guard masterKey.count == profile.masterKeyLength else { throw SrtpCryptoError.invalidKeyLength }
        guard masterSalt.count == profile.masterSaltLength else { throw SrtpCryptoError.invalidNonce }
        self.sessionSalt = try SrtpKeyDerivation.derive(key: masterKey, salt: masterSalt, label: 0x02, length: profile.masterSaltLength)
        let key = try SrtpKeyDerivation.derive(key: masterKey, salt: masterSalt, label: 0x00, length: profile.masterKeyLength)
        self.tagLength = profile.authenticationTagLength
        switch profile {
        case .aeadAes128Gcm, .aeadAes256Gcm, .aeadAes128Gcm8, .aeadAes256Gcm8:
            self.cipher = .gcm(SymmetricKey(data: key))
        case .aesCm128HmacSha1_32, .aesCm128HmacSha1_80, .aesCm256HmacSha1_32, .aesCm256HmacSha1_80:
            let authenticationKey = try SrtpKeyDerivation.derive(key: masterKey, salt: masterSalt, label: 0x01, length: 20)
            self.cipher = .counterMode(encryptionKey: key, authenticationKey: SymmetricKey(data: authenticationKey))
        }
    }

    /// Decrypts one packet and returns the RTP payload it carried.
    public func unprotect(_ datagram: Data, rolloverCounter: UInt32 = 0) throws -> (packet: NvstAudioRtpPacket, payload: Data) {
        guard let packet = NvstAudioRtpPacket.parse(datagram, tagLength: tagLength) else {
            throw SrtpCryptoError.invalidTagLength
        }
        let payload: Data
        switch cipher {
        case .gcm(let key):
            payload = try unprotectGcm(packet, key: key, rolloverCounter: rolloverCounter)
        case .counterMode(let encryptionKey, let authenticationKey):
            let tag = counterModeTag(header: packet.authenticatedHeader, ciphertext: packet.encryptedPayload,
                                     key: authenticationKey, rolloverCounter: rolloverCounter)
            guard Self.tagsMatch(tag, packet.authenticationTag) else { throw SrtpCryptoError.authenticationFailed }
            payload = try counterModePayload(packet.encryptedPayload, packet: packet, key: encryptionKey, rolloverCounter: rolloverCounter)
        }
        return (packet, try Self.removePadding(payload, header: packet.authenticatedHeader))
    }

    /// Protects one RTP packet, returning header plus ciphertext plus tag.
    public func protect(_ rtpPacket: Data, rolloverCounter: UInt32 = 0) throws -> Data {
        guard let packet = NvstAudioRtpPacket.parse(rtpPacket, tagLength: 0) else {
            throw SrtpCryptoError.invalidTagLength
        }
        switch cipher {
        case .gcm(let key):
            let sealed = try AES.GCM.seal(packet.encryptedPayload, using: key,
                                          nonce: gcmNonce(packet, rolloverCounter: rolloverCounter),
                                          authenticating: packet.authenticatedHeader)
            return packet.authenticatedHeader + sealed.ciphertext + sealed.tag.prefix(tagLength)
        case .counterMode(let encryptionKey, let authenticationKey):
            let ciphertext = try counterModePayload(packet.encryptedPayload, packet: packet, key: encryptionKey, rolloverCounter: rolloverCounter)
            let tag = counterModeTag(header: packet.authenticatedHeader, ciphertext: ciphertext,
                                     key: authenticationKey, rolloverCounter: rolloverCounter)
            return packet.authenticatedHeader + ciphertext + tag
        }
    }

    public var authenticationTagLength: Int { tagLength }

    private func gcmNonce(_ packet: NvstAudioRtpPacket, rolloverCounter: UInt32) throws -> AES.GCM.Nonce {
        try AES.GCM.Nonce(data: SrtpKeyDerivation.gcmIV(sessionSalt: sessionSalt, ssrc: packet.ssrc,
                                                      rolloverCounter: rolloverCounter, sequenceNumber: packet.sequenceNumber))
    }

    private func unprotectGcm(_ packet: NvstAudioRtpPacket, key: SymmetricKey, rolloverCounter: UInt32) throws -> Data {
        let nonce = try gcmNonce(packet, rolloverCounter: rolloverCounter)
        if tagLength == 16 {
            do {
                let sealed = try AES.GCM.SealedBox(nonce: nonce, ciphertext: packet.encryptedPayload, tag: packet.authenticationTag)
                return try AES.GCM.open(sealed, using: key, authenticating: packet.authenticatedHeader)
            } catch {
                throw SrtpCryptoError.authenticationFailed
            }
        }
        let payload = try AES.GCM.seal(packet.encryptedPayload, using: key, nonce: nonce).ciphertext
        let sealed = try AES.GCM.seal(payload, using: key, nonce: nonce, authenticating: packet.authenticatedHeader)
        guard Self.tagsMatch(Data(sealed.tag.prefix(tagLength)), packet.authenticationTag) else {
            throw SrtpCryptoError.authenticationFailed
        }
        return payload
    }

    private func counterModePayload(_ payload: Data, packet: NvstAudioRtpPacket, key: Data, rolloverCounter: UInt32) throws -> Data {
        var counter = [UInt8](sessionSalt) + [0, 0]
        let ssrc = Self.bigEndianBytes(packet.ssrc)
        let rollover = Self.bigEndianBytes(rolloverCounter)
        for index in 0..<4 {
            counter[4 + index] ^= ssrc[index]
            counter[8 + index] ^= rollover[index]
        }
        counter[12] ^= UInt8(packet.sequenceNumber >> 8)
        counter[13] ^= UInt8(truncatingIfNeeded: packet.sequenceNumber)
        let mask = try SrtpCryptor.counterKeystream(key: key, counter: Data(counter), length: payload.count)
        return Data(zip(payload, mask).map { $0 ^ $1 })
    }

    private func counterModeTag(header: Data, ciphertext: Data, key: SymmetricKey, rolloverCounter: UInt32) -> Data {
        let authenticated = header + ciphertext + Data(Self.bigEndianBytes(rolloverCounter))
        let tag = HMAC<Insecure.SHA1>.authenticationCode(for: authenticated, using: key)
        return Data(tag.prefix(tagLength))
    }

    private static func bigEndianBytes(_ value: UInt32) -> [UInt8] {
        withUnsafeBytes(of: value.bigEndian) { Array($0) }
    }

    private static func tagsMatch(_ expected: Data, _ received: Data) -> Bool {
        guard expected.count == received.count else { return false }
        return zip(expected, received).reduce(UInt8(0)) { $0 | ($1.0 ^ $1.1) } == 0
    }

    private static func removePadding(_ payload: Data, header: Data) throws -> Data {
        guard let first = header.first, first & 0x20 != 0 else { return payload }
        guard let padding = payload.last, padding > 0, Int(padding) <= payload.count else {
            throw SrtpCryptoError.cryptorError("invalid RTP padding")
        }
        return Data(payload.dropLast(Int(padding)))
    }
}
