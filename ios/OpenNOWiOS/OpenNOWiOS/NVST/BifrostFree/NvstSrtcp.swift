import Foundation

/// SRTCP sealing for the NVST `rtcp1` feedback channel.
///
/// Keys derive from the SRTP master material with labels 0x03 (key) / 0x05 (salt); the GCM
/// nonce is the RFC 7714 §9.2 SRTCP IV. Per §7.2 the GCM AAD is the 8-byte SRTCP header plus
/// the first 8 octets of the *ciphertext* (both parties can reconstruct it), the E/S flag
/// folds into bit 0 of the payload's first byte, an 8-byte tag follows the ciphertext, and
/// the 4-byte SRTCP index trailer is appended last.
public enum NvstSrtcp {
    public static let rtcptKeyLabel: UInt8 = 0x03
    public static let rtcptSaltLabel: UInt8 = 0x05
    public static let srtcpEncryptedFlag: UInt8 = 0x80

    private struct Material {
        let cipher: SrtpGcm8
        let salt: Data
    }

    private static func material(masterKey: Data, masterSalt: Data) throws -> Material {
        let sessionKey = try SrtpKeyDerivation.derive(key: masterKey, salt: masterSalt, label: rtcptKeyLabel, length: 32)
        let sessionSalt = try SrtpKeyDerivation.derive(key: masterKey, salt: masterSalt, label: rtcptSaltLabel, length: 12)
        return Material(cipher: try SrtpGcm8(key: sessionKey), salt: sessionSalt)
    }

    public static func seal(rtcpPacket: Data, masterKey: Data, masterSalt: Data, senderSSRC: UInt32, srtcpIndex: UInt32) throws -> Data {
        guard rtcpPacket.count > 8 else { throw SrtpCryptoError.cryptorError("RTCP packet has no encrypted payload.") }
        let material = try material(masterKey: masterKey, masterSalt: masterSalt)
        let iv = try NvstRtcp.srtcpGcmIV(sessionSalt: material.salt, ssrc: senderSSRC, srtcpIndex: srtcpIndex)

        let header = rtcpPacket.prefix(8)
        var payload = [UInt8](rtcpPacket.dropFirst(8))
        // §7.2's AAD takes the first 8 octets of ciphertext: a shorter encrypted region would
        // silently shorten the AAD and seal a packet the seat cannot open. Only RRs are sealed
        // today, and their encrypted region is well over 8 bytes.
        guard payload.count >= 8 else { throw SrtpCryptoError.cryptorError("SRTCP payload too short for the 8-octet AAD.") }
        payload[0] |= srtcpEncryptedFlag
        let keystream = try material.cipher.mask(iv: iv, length: payload.count)
        let ksBytes = [UInt8](keystream)
        var ciphertext = [UInt8](repeating: 0, count: payload.count)
        for index in ciphertext.indices { ciphertext[index] = payload[index] ^ ksBytes[index] }

        let aad = header + Data(ciphertext.prefix(8))
        let tag16 = try material.cipher.authenticationTag(iv: iv, aad: Data(aad), ciphertext: Data(ciphertext))
        let tag = tag16.prefix(8)

        var packet = Data(header)
        packet.append(contentsOf: ciphertext)
        packet.append(tag)
        packet.append(NvstRtcp.srtcpIndexTrailer(index: srtcpIndex, encrypted: true))
        return packet
    }

    public static func open(srtcpPacket: Data, masterKey: Data, masterSalt: Data, senderSSRC: UInt32) throws -> (rtcp: Data, index: UInt32) {
        guard srtcpPacket.count > 8 + 8 + 4 else { throw SrtpCryptoError.cryptorError("SRTCP packet too short.") }
        let header = srtcpPacket.prefix(8)
        let ciphertext = srtcpPacket.dropFirst(8).dropLast(4).dropLast(8)
        let tag = srtcpPacket.dropFirst(8).dropLast(4).suffix(8)
        var trailer = NvstByteReader(srtcpPacket.suffix(4))
        let index = ((try? trailer.u32BE()) ?? 0) & 0x7fff_ffff

        guard ciphertext.count >= 8 else { throw SrtpCryptoError.cryptorError("SRTCP payload too short for the 8-octet AAD.") }
        let material = try material(masterKey: masterKey, masterSalt: masterSalt)
        let iv = try NvstRtcp.srtcpGcmIV(sessionSalt: material.salt, ssrc: senderSSRC, srtcpIndex: index)
        let aad = header + ciphertext.prefix(8)
        let expectedTag = try material.cipher.authenticationTag(iv: iv, aad: Data(aad), ciphertext: Data(ciphertext)).prefix(8)
        guard expectedTag.elementsEqual(tag) else { throw SrtpCryptoError.authenticationFailed }

        let keystream = try material.cipher.mask(iv: iv, length: ciphertext.count)
        let ciphertextBytes = [UInt8](ciphertext)
        let ksBytes = [UInt8](keystream)
        var payload = [UInt8](repeating: 0, count: ciphertext.count)
        for position in payload.indices { payload[position] = ciphertextBytes[position] ^ ksBytes[position] }
        payload[0] &= ~srtcpEncryptedFlag
        var rtcp = Data(header)
        rtcp.append(contentsOf: payload)
        return (rtcp, index)
    }
}
