import CommonCrypto
import CryptoKit
import Foundation

/// SRTP-AES-GCM primitives for the NVST "Mjolnir" video receive path.
///
/// Wire facts (independently derived; OpenNOW's MIT `native-opennow-streamer` documents the
/// same formats):
/// - NVIDIA's `sec_serv_conf_and_auth` + 256-bit keys map to AES-256-GCM with an **8-byte**
///   authentication tag (NIST GCM truncation, not RFC 7714's 16).
/// - Session key/salt derive from the handoff master key/salt per RFC 3711 (§4.3.1):
///   counter-mode AES over the master key with a 16-byte counter IV = master salt, byte 7
///   XORed with the label (0x00 session key, 0x02 session salt).
/// - The packet nonce is the RFC 7714 GCM IV: 12-byte salt XOR (SSRC at 2..6, ROC at 6..10,
///   sequence at 10..12).
/// - AAD is the RTP header + the 16-byte "GS" extension (everything before ciphertext).
///
/// AES block/CTR use CommonCrypto. The truncated-tag GCM AEAD step lives in `SrtpGcm8`; the
/// `GHash` below is a test-only reference implementation of the GHASH convention.

public enum SrtpCryptoError: LocalizedError, Equatable, Sendable {
    case invalidKeyLength
    case invalidNonce
    case invalidTagLength
    case cryptorError(String)
    case authenticationFailed

    public var errorDescription: String? {
        switch self {
        case .invalidKeyLength: "SRTP key must be 16 or 32 bytes."
        case .invalidNonce: "SRTP nonce must be 12 bytes."
        case .invalidTagLength: "SRTP authentication tag must be 4 to 16 bytes."
        case .cryptorError(let reason): "Crypto error: \(reason)"
        case .authenticationFailed: "SRTP authentication tag mismatch."
        }
    }
}

/// GHASH over GF(2^128), NIST GCM convention (no reflection): bytes are MSB-first
/// (byte 0 = x^120..x^127, byte 15 = x^0..x^7); multiplication by x is a right shift of the
/// big-endian value with reduction `0xe1` XORed into byte 0 when the LSB carries out.
public enum GHash {
    static func hash(key h: Data, aad: Data, ciphertext: Data) -> Data {
        var y = [UInt8](repeating: 0, count: 16)
        for block in Self.blocks(aad) + Self.blocks(ciphertext) + [Self.lengthBlock(aadBytes: aad.count, ciphertextBytes: ciphertext.count)] {
            for index in 0..<16 { y[index] ^= block[index] }
            y = Self.multiply([UInt8](h), y)
        }
        return Data(y)
    }

    static func multiply(_ x: [UInt8], _ y: [UInt8]) -> [UInt8] {
        var z = [UInt8](repeating: 0, count: 16)
        var v = y
        for i in 0..<128 {
            // Bit (127 - i): MSB of byte 0 first, down through the big-endian value.
            let byte = i / 8
            let bit = 7 - (i % 8)
            if (x[byte] >> bit) & 1 == 1 {
                for index in 0..<16 { z[index] ^= v[index] }
            }
            let carry = v[15] & 1
            for index in (0..<16).reversed() {
                let next = index == 0 ? 0 : (v[index - 1] & 1)
                v[index] = (v[index] >> 1) | (next << 7)
            }
            if carry == 1 { v[0] ^= 0xe1 }
        }
        return z
    }

    static func blocks(_ data: Data) -> [[UInt8]] {
        var padded = [UInt8](data)
        let remainder = padded.count % 16
        if remainder != 0 { padded.append(contentsOf: [UInt8](repeating: 0, count: 16 - remainder)) }
        guard !padded.isEmpty else { return [] }
        return stride(from: 0, to: padded.count, by: 16).map { Array(padded[$0..<$0 + 16]) }
    }

    static func lengthBlock(aadBytes: Int, ciphertextBytes: Int) -> [UInt8] {
        var block = [UInt8](repeating: 0, count: 16)
        let aadBits = UInt64(aadBytes) * 8
        let ciphertextBits = UInt64(ciphertextBytes) * 8
        for index in 0..<8 { block[index] = UInt8((aadBits >> (8 * (7 - index))) & 0xff) }
        for index in 0..<8 { block[8 + index] = UInt8((ciphertextBits >> (8 * (7 - index))) & 0xff) }
        return block
    }
}

/// AES-256-GCM with an 8-byte auth tag, the NVIDIA SRTP profile, implemented over
/// CommonCrypto ECB + the GHASH above. Correctness pinned against python-cryptography
/// reference vectors in SrtpCryptographyTests.
public struct SrtpGcm8 {
    private let key: Data
    /// Built once. `SymmetricKey(data:)` allocates secure storage, and doing that per packet was a
    /// measurable part of the receive path's cost.
    private let symmetricKey: SymmetricKey

    public init(key: Data) throws {
        guard key.count == 32 else { throw SrtpCryptoError.invalidKeyLength }
        self.key = key
        self.symmetricKey = SymmetricKey(data: key)
    }

    static func j0(iv: Data) -> Data {
        var block = [UInt8](iv)
        block.append(contentsOf: [0, 0, 0, 1])
        return Data(block)
    }

    /// CTR keystream for the GCM counter starting at J0+1, length bytes.
    ///
    /// Sealing zeroes yields the keystream itself, which keeps this on the CPU's AES instructions
    /// and out of CommonCrypto's per-call cryptor setup.
    public func mask(iv: Data, length: Int) throws -> Data {
        guard iv.count == 12 else { throw SrtpCryptoError.invalidNonce }
        return try hardwareSeal(iv: iv, aad: Data(), plaintext: Data(count: length)).ciphertext
    }

    /// Full 16-byte GCM authentication tag for the given nonce/AAD/ciphertext.
    public func authenticationTag(iv: Data, aad: Data, ciphertext: Data) throws -> Data {
        guard iv.count == 12 else { throw SrtpCryptoError.invalidNonce }
        let plaintext = try decryptWithoutAuthenticating(iv: iv, ciphertext: ciphertext)
        return try hardwareSeal(iv: iv, aad: aad, plaintext: plaintext).tag
    }

    /// GCM's tag and ciphertext from Apple's AES-GCM, which runs on the CPU's AES and carry-less
    /// multiply instructions.
    ///
    /// `GHash` below is the same function in portable Swift, and it is far too slow to stand in a
    /// receive path: it walks 128 bit-positions per 16-byte block, so a 1200-byte packet costs
    /// about 320,000 byte operations plus a heap allocation per block. Measured on the real stream
    /// it took 21.9 ms per packet and consumed 98.5% of the session's wall clock, which held the
    /// receiver to 50 packets a second and delivered 4 seconds of media over 30 seconds — video
    /// that plays in slow motion rather than merely dropping frames.
    ///
    /// CryptoKit only opens a sealed box with GCM's full 16-byte tag, while NVIDIA's Mjolnir
    /// policy truncates to 8. Sealing the recovered plaintext reproduces both the ciphertext and
    /// the whole tag, so the truncated comparison is exact — and the regenerated ciphertext is a
    /// free check that the counter stream agreed.
    private func hardwareSeal(iv: Data, aad: Data, plaintext: Data) throws -> (tag: Data, ciphertext: Data) {
        do {
            let box = try AES.GCM.seal(
                plaintext,
                using: symmetricKey,
                nonce: try AES.GCM.Nonce(data: iv),
                authenticating: aad
            )
            return (box.tag, box.ciphertext)
        } catch {
            throw SrtpCryptoError.cryptorError(String(describing: error))
        }
    }

    /// GCM encrypts with a counter keystream, so applying the same key and nonce to the ciphertext
    /// re-applies the identical keystream and hands back the plaintext: the sealed box's
    /// *ciphertext* is what we want, and its tag (computed over the wrong input) is discarded.
    ///
    /// This replaced a CommonCrypto CTR pass that built and released a cryptor, zero-filled a
    /// keystream buffer, and copied through `[UInt8]` and back for every packet. That cost 129 us
    /// per 1280-byte packet end to end — about 7700 packets/s, which is why the stream lost packets
    /// above ~33 Mbps no matter what bitrate the seat was willing to send. The AAD is irrelevant
    /// here because it only feeds the tag, never the keystream.
    private func decryptWithoutAuthenticating(iv: Data, ciphertext: Data) throws -> Data {
        try hardwareSeal(iv: iv, aad: Data(), plaintext: ciphertext).ciphertext
    }

    /// Encrypts plaintext with the truncated tag appended (tests and diagnostics).
    public func seal(iv: Data, aad: Data, plaintext: Data, tagLength: Int = 8) throws -> Data {
        guard iv.count == 12 else { throw SrtpCryptoError.invalidNonce }
        let sealed = try hardwareSeal(iv: iv, aad: aad, plaintext: plaintext)
        return sealed.ciphertext + sealed.tag.prefix(tagLength)
    }

    /// Decrypts ciphertext (without the tag) and authenticates the truncated tag. NVIDIA's
    /// Mjolnir policy uses an 8-byte tag; a seat that advertises the plain `AEAD_AES_*_GCM`
    /// profile uses RFC 7714's 16.
    ///
    /// The two passes are structural, not laziness: CryptoKit's `AES.GCM.open` demands a full
    /// 16-byte tag, and neither CryptoKit nor any Apple API exposes GHASH or a truncated-tag
    /// open, so authenticating an 8-byte tag means regenerating the tag by re-sealing the
    /// recovered plaintext (one CTR pass per direction). CommonCrypto's one-shot GCM would take
    /// a single pass but measured ~129 µs per packet here — its per-call cryptor setup costs
    /// more than both CryptoKit passes together — and `GHash` in portable Swift is three
    /// orders of magnitude slower still. The regenerated-ciphertext comparison is a redundant
    /// check (the tag already authenticates the ciphertext) but a free one, so it stays.
    public func decrypt(iv: Data, aad: Data, ciphertext: Data, authenticationTag: Data) throws -> Data {
        let tagLength = authenticationTag.count
        guard iv.count == 12 else { throw SrtpCryptoError.invalidNonce }
        guard (4...16).contains(tagLength) else { throw SrtpCryptoError.invalidTagLength }
        let plaintext = try decryptWithoutAuthenticating(iv: iv, ciphertext: ciphertext)
        let sealed = try hardwareSeal(iv: iv, aad: aad, plaintext: plaintext)
        guard sealed.ciphertext == ciphertext,
              sealed.tag.prefix(tagLength).elementsEqual(authenticationTag) else {
            throw SrtpCryptoError.authenticationFailed
        }
        return plaintext
    }
}

public enum SrtpCryptor {
    /// One raw AES block encryption (ECB).
    public static func blockEncrypt(key: Data, block: Data) throws -> Data {
        guard block.count == 16, key.count == 16 || key.count == 32 else {
            throw SrtpCryptoError.invalidKeyLength
        }
        return try withCryptor(op: CCOperation(kCCEncrypt), mode: CCMode(kCCModeECB), iv: nil, key: key) { cryptor in
            try cryption(update: cryptor, input: block, outputLength: 16)
        }
    }

    /// AES-CTR keystream from a 16-byte counter IV, for the RFC 3711 key derivation.
    public static func counterKeystream(key: Data, counter: Data, length: Int) throws -> Data {
        guard key.count == 16 || key.count == 32, counter.count == 16 else {
            throw SrtpCryptoError.invalidKeyLength
        }
        return try withCryptor(op: CCOperation(kCCEncrypt), mode: CCMode(kCCModeCTR), iv: counter, key: key) { cryptor in
            try cryption(update: cryptor, input: Data(count: length), outputLength: length)
        }
    }

    private static func cryption(update cryptor: CCCryptorRef, input: Data, outputLength: Int) throws -> Data {
        var output = Data(count: outputLength)
        var moved = 0
        let status: CCCryptorStatus = input.withUnsafeBytes { inBytes in
            output.withUnsafeMutableBytes { outBytes in
                CCCryptorUpdate(cryptor, inBytes.baseAddress, inBytes.count, outBytes.baseAddress, outputLength, &moved)
            }
        }
        guard status == kCCSuccess else {
            throw SrtpCryptoError.cryptorError("update failed (\(status)).")
        }
        var finalMoved = 0
        guard CCCryptorFinal(cryptor, nil, 0, &finalMoved) == kCCSuccess else {
            throw SrtpCryptoError.cryptorError("final failed.")
        }
        return Data(output.prefix(moved))
    }

    private static func withCryptor(op: CCOperation,
                                    mode: CCMode,
                                    iv: Data?,
                                    key: Data,
                                    _ body: (CCCryptorRef) throws -> Data) throws -> Data {
        var cryptor: CCCryptorRef?
        let status: CCCryptorStatus = key.withUnsafeBytes { keyBytes in
            if let iv {
                return iv.withUnsafeBytes { ivBytes in
                    CCCryptorCreateWithMode(op, mode, CCAlgorithm(kCCAlgorithmAES), CCPadding(ccNoPadding), ivBytes.baseAddress, keyBytes.baseAddress, keyBytes.count, nil, 0, 0, 0, &cryptor)
                }
            }
            return CCCryptorCreateWithMode(op, mode, CCAlgorithm(kCCAlgorithmAES), CCPadding(ccNoPadding), nil, keyBytes.baseAddress, keyBytes.count, nil, 0, 0, 0, &cryptor)
        }
        guard status == kCCSuccess, let cryptorRef = cryptor else {
            throw SrtpCryptoError.cryptorError("cryptor create failed (\(status)).")
        }
        defer { CCCryptorRelease(cryptorRef) }
        return try body(cryptorRef)
    }
}

/// SRTP key derivation (RFC 3711 §4.3.1): Counter-mode AES keyed by the master key with the
/// 16-byte counter IV = master salt (byte 7 XOR label).
public enum SrtpKeyDerivation {
    public static func derive(key masterKey: Data, salt masterSalt: Data, label: UInt8, length: Int) throws -> Data {
        guard masterKey.count == 16 || masterKey.count == 32 else {
            throw SrtpCryptoError.invalidKeyLength
        }
        var iv = [UInt8](repeating: 0, count: 16)
        for index in 0..<min(16, masterSalt.count) { iv[index] = masterSalt[index] }
        iv[7] ^= label
        return try SrtpCryptor.counterKeystream(key: masterKey, counter: Data(iv), length: length)
    }

    /// RFC 7714 §9.1: 12-byte nonce = session salt XOR (SSRC at 2..6, ROC at 6..10, seq at 10..12).
    public static func gcmIV(sessionSalt: Data, ssrc: UInt32, rolloverCounter: UInt32, sequenceNumber: UInt16) throws -> Data {
        guard sessionSalt.count == 12 else { throw SrtpCryptoError.invalidNonce }
        var iv = [UInt8](sessionSalt)
        let ssrcBytes = Self.bigEndianBytes(ssrc)
        let rocBytes = Self.bigEndianBytes(rolloverCounter)
        let seqBytes = Self.bigEndianBytes(sequenceNumber)
        for index in 0..<4 { iv[2 + index] ^= ssrcBytes[index] }
        for index in 0..<4 { iv[6 + index] ^= rocBytes[index] }
        for index in 0..<2 { iv[10 + index] ^= seqBytes[index] }
        return Data(iv)
    }

    private static func bigEndianBytes<T: FixedWidthInteger>(_ value: T) -> [UInt8] {
        var big = value.bigEndian
        return withUnsafeBytes(of: &big) { Array($0) }
    }
}

/// Replay window for the video stream (64 packets, matching the observed vendor behavior).
public struct SrtpReplayWindow {
    private var highestIndex: UInt64?
    private var seen: UInt64 = 0

    public init() {}

    /// The extended index a sequence number most plausibly belongs to (RFC 3711 §3.3.1).
    ///
    /// Total for every input: the previous-epoch branch is skipped while `roc` is zero, because a
    /// stream has no epoch before its first packet. Wrapping `roc - 1` there returned an index
    /// whose ROC was `0xffff_ffff_ffff_ffff`, and the receiver's `UInt32` conversion of it trapped
    /// — one datagram on the video socket whose sequence number sat more than 32,768 above the
    /// highest accepted one terminated the process, before authentication and with no key. Such a
    /// packet has no epoch to belong to, so it estimates into the current one and is then held to
    /// the ordinary replay and authentication checks.
    public func estimatedIndex(for sequenceNumber: UInt16) -> UInt64 {
        guard let highest = highestIndex else { return UInt64(sequenceNumber) }
        let roc = highest >> 16
        let highestSequence = UInt16(truncatingIfNeeded: highest & 0xffff)
        let delta = Int32(sequenceNumber) - Int32(highestSequence)
        if delta < -32_768 {
            return ((roc &+ 1) << 16) | UInt64(sequenceNumber)
        } else if delta > 32_768, roc > 0 {
            return ((roc - 1) << 16) | UInt64(sequenceNumber)
        }
        return (roc << 16) | UInt64(sequenceNumber)
    }

    /// Whether `accept` would take this index, without advancing the window. RFC 3711 §3.3.2:
    /// the replay check runs before decryption so replays never pay the GCM cost, but the list
    /// must not update until the packet authenticates — a forged packet must not be able to
    /// advance the window and age out real ones.
    public func wouldAccept(_ index: UInt64) -> Bool {
        guard let highest = highestIndex else { return true }
        if index > highest { return true }
        let age = highest - index
        return age < 64 && (seen & (1 << age)) == 0
    }

    public mutating func accept(_ index: UInt64) -> Bool {
        guard let highest = highestIndex else {
            highestIndex = index
            seen = 1 // the highest itself is already seen (age 0)
            return true
        }
        if index > highest {
            let delta = index - highest
            // Every known index ages by delta; the previous highest becomes age delta.
            seen = (delta >= 64) ? 0 : (seen << delta) | (1 << delta)
            highestIndex = index
            seen |= 1
            return true
        }
        let age = highest - index
        if age >= 64 || (seen & (1 << age)) != 0 { return false }
        seen |= (1 << age)
        return true
    }
}
