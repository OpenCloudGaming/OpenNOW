import Foundation
import Testing
@testable import OpenNOWiOS

@Suite(.serialized)
struct SrtpCryptographyTests {

    private func hex(_ text: String) -> Data {
        var data = Data()
        var index = text.startIndex
        while index < text.endIndex {
            let next = text.index(index, offsetBy: 2)
            data.append(UInt8(text[index..<next], radix: 16)!)
            index = next
        }
        return data
    }

    // FIPS-197 block-level vectors verify the AES primitive for both key sizes.
    @Test func aesEcbFips197Vectors() throws {
        let pt = hex("00112233445566778899aabbccddeeff")
        let aes128 = try SrtpCryptor.blockEncrypt(key: hex("000102030405060708090a0b0c0d0e0f"), block: pt)
        #expect(aes128 == hex("69c4e0d86a7b0430d8cdb78070b4c55a"))

        let aes256 = try SrtpCryptor.blockEncrypt(
            key: hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"),
            block: pt
        )
        #expect(aes256 == hex("8ea2b7ca516745bfeafc49904b496089"))
    }

    // GCM-8 reference vectors from python-cryptography (AES-256, 12-byte IV, 8-byte tag).
    @Test func gcmAes256Gcm8ReferenceVectors() throws {
        let key = hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
        let iv = hex("00000000000000009ECA935E")
        let aad = hex("80600031f2be83b111223344")
        let plaintext = hex("00254a6f94b9de082d52779cc1e610355a7fa4c9ee183d6287acd1f620456a8fb4d903284d7297bce10b30557a9fc4e913385d82a7ccf11b40658aaf")
        let ciphertext = hex("641058b33b1b18ac2bc1760a40c1685f8a93f3a6ddbb9b168244ab2a4be750ae7e310142f0d8ff7be1261d75baa26da032bebc61b8b21122b4b703c2")
        let tag8 = hex("058b0220fe5b9918")
        let gcm = try SrtpGcm8(key: key)
        #expect(try gcm.decrypt(iv: iv, aad: aad, ciphertext: ciphertext, authenticationTag: tag8) == plaintext)
    }

    @Test func gcmAes256Gcm8SingleBlockReference() throws {
        let key = hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
        let iv = hex("00000000000000009ECA935E")
        let plaintext = hex("00000000000000000000000000000000")
        let ciphertext = hex("643512dcafa2c6a4069301968127786a")
        let tag8 = hex("0be4de75e4dc8898")
        let gcm = try SrtpGcm8(key: key)
        #expect(try gcm.decrypt(iv: iv, aad: Data(), ciphertext: ciphertext, authenticationTag: tag8) == plaintext)
    }

    @Test func gcmRejectsTamperedPayload() throws {
        let key = hex("000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f")
        let iv = hex("00000000000000009ECA935E")
        let aad = hex("80600031f2be83b111223344")
        let ciphertext = hex("641058b33b1b18ac2bc1760a40c1685f8a93f3a6ddbb9b168244ab2a4be750ae7e310142f0d8ff7be1261d75baa26da032bebc61b8b21122b4b703c2")
        let tag8 = hex("058b0220fe5b9918")
        var tampered = ciphertext
        tampered[0] ^= 0x80
        let gcm = try SrtpGcm8(key: key)
        #expect(throws: SrtpCryptoError.self) { _ = try gcm.decrypt(iv: iv, aad: aad, ciphertext: tampered, authenticationTag: tag8) }
    }

    // RFC 3711 key derivation semantics.
    @Test func srtpKeyDerivationMatchesLabelSemantics() throws {
        let masterKey = hex(String(repeating: "e1", count: 32))
        let salt = hex("0ec675ad498afeebb6960b3aabe6")
        let derived = try SrtpKeyDerivation.derive(key: masterKey, salt: salt, label: 0x00, length: 32)
        #expect(derived.count == 32)
        #expect(try SrtpKeyDerivation.derive(key: masterKey, salt: salt, label: 0x00, length: 32) == derived)
        #expect(try SrtpKeyDerivation.derive(key: masterKey, salt: salt, label: 0x02, length: 12) != derived)
        #expect(try SrtpKeyDerivation.derive(key: masterKey, salt: salt, label: 0x02, length: 12).count == 12)
    }

    @Test func gcmIvConstructionMatchesRfc7714() throws {
        let iv = try SrtpKeyDerivation.gcmIV(sessionSalt: hex("00000000000000009ECA935E"), ssrc: 0x1122_3344, rolloverCounter: 1, sequenceNumber: 0xabcd)
        let bytes = [UInt8](iv)
        #expect(iv.count == 12)
        #expect(bytes[0] == 0 && bytes[1] == 0)
        #expect(bytes[2] == 0x11 && bytes[3] == 0x22 && bytes[4] == 0x33 && bytes[5] == 0x44)
    }

    @Test func replayWindowEstimatesAndRejects() {
        var window = SrtpReplayWindow()
        let first = window.accept(100)
        let second = window.accept(101)
        let replay = window.accept(100)
        let estimated = window.estimatedIndex(for: 0)
        #expect(first)
        #expect(second)
        #expect(!replay)
        // A modest sequence delta stays in the current rollover window.
        #expect(estimated == 0)
    }

    /// A sequence number more than 32,768 above the highest accepted one reads as belonging to the
    /// epoch before the current one — which does not exist while the rollover counter is still
    /// zero, the state the stream is in until its sequence wraps. Estimating into that absent
    /// epoch used to wrap `roc - 1` and return an index whose ROC was `0xffff_ffff_ffff_ffff`;
    /// the receiver's `UInt32` conversion of it trapped, so a single unauthenticated datagram
    /// terminated the process. Roughly half of all sequence numbers are far enough above any given
    /// highest to take that branch.
    @Test func replayWindowHasNoEpochBeforeTheFirstOne() {
        var window = SrtpReplayWindow()
        let accepted = window.accept(100)
        #expect(accepted)

        let estimated = window.estimatedIndex(for: 40_000)
        #expect(estimated == 40_000)
        #expect(UInt32(exactly: estimated >> 16) != nil)

        // The estimate is a guess, not an acceptance: it still passes through the ordinary
        // replay window, and the window does not move until a packet authenticates.
        let acceptable = window.wouldAccept(estimated)
        #expect(acceptable)
        #expect(window.estimatedIndex(for: 0xffff) == 0xffff)
    }

    /// Above the first rollover the previous epoch does exist, and a sequence number far below the
    /// highest must still estimate into it — otherwise a packet that legitimately straddles the
    /// wrap is misplaced by a whole epoch and fails authentication.
    @Test func replayWindowEstimatesIntoThePreviousEpochOnceOneExists() {
        var window = SrtpReplayWindow()
        let accepted = window.accept(0x0001_0000)
        #expect(accepted)

        let estimated = window.estimatedIndex(for: 0xffff)
        #expect(estimated == 0xffff)
        #expect(UInt32(exactly: estimated >> 16) != nil)

        // Wrapping forward stays symmetric: far below the highest means the next epoch.
        var forward = SrtpReplayWindow()
        let forwardAccepted = forward.accept(0xffff)
        #expect(forwardAccepted)
        #expect(forward.estimatedIndex(for: 0) == 0x0001_0000)
    }
}
