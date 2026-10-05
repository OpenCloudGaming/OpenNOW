import Foundation
import Testing
@testable import OpenNOWiOS

/// Drives a full DTLS handshake between two in-process endpoints, with no socket and no seat.
///
/// This is the gate that makes the live handshake a plumbing exercise: if the state machine cannot
/// agree on keying material with itself, there is no point pointing it at a remote peer.
@Suite struct NvstDtlsHandshakeTests {
    private static let srtpKeyMaterialLength = 88

    /// Alternates flights until both ends finish. Datagrams are consumed as they are handed over, so
    /// nothing is replayed into a state machine that has already seen it.
    @discardableResult
    private func pump(_ client: NvstDtlsHandshake, _ server: NvstDtlsHandshake, rounds: Int = 48) throws -> Bool {
        var toClient: Data?
        var toServer: Data?
        for _ in 0..<rounds {
            if !client.isConnected {
                let outbound = try client.handshakeStep(received: toServer)
                toServer = nil
                if let outbound { toClient = outbound }
            }
            if !server.isConnected {
                let outbound = try server.handshakeStep(received: toClient)
                toClient = nil
                if let outbound { toServer = outbound }
            }
            if client.isConnected && server.isConnected { return true }
        }
        return false
    }

    private func makePair() throws -> (client: NvstDtlsHandshake, server: NvstDtlsHandshake) {
        let clientIdentity = try NvstDtlsIdentity()
        let serverIdentity = try NvstDtlsIdentity()
        let client = try NvstDtlsHandshake(role: .client, identity: clientIdentity, expectedPeerFingerprint: serverIdentity.fingerprint)
        let server = try NvstDtlsHandshake(role: .server, identity: serverIdentity, expectedPeerFingerprint: clientIdentity.fingerprint)
        return (client, server)
    }

    @Test func theHandshakeCompletesBetweenTwoEndpoints() throws {
        let pair = try makePair()
        #expect(try pump(pair.client, pair.server), "the handshake never converged")
        #expect(pair.client.isConnected)
        #expect(pair.server.isConnected)
    }

    @Test func bothEndsDeriveTheSameSrtpKeyingMaterial() throws {
        let pair = try makePair()
        #expect(try pump(pair.client, pair.server))
        let clientKeys = try pair.client.exportKeyingMaterial(length: Self.srtpKeyMaterialLength)
        let serverKeys = try pair.server.exportKeyingMaterial(length: Self.srtpKeyMaterialLength)
        #expect(clientKeys.count == Self.srtpKeyMaterialLength)
        #expect(clientKeys == serverKeys, "the two ends disagree on the SRTP master keys")
        #expect(clientKeys.contains { $0 != 0 }, "exported keying material is all zeroes")
    }

    @Test func eachHandshakeExportsDifferentKeys() throws {
        let first = try makePair()
        #expect(try pump(first.client, first.server))
        let second = try makePair()
        #expect(try pump(second.client, second.server))
        #expect(try first.client.exportKeyingMaterial(length: Self.srtpKeyMaterialLength)
                != (try second.client.exportKeyingMaterial(length: Self.srtpKeyMaterialLength)),
                "two handshakes produced identical keying material")
    }

    @Test func aPeerWhoseCertificateDoesNotMatchIsRejected() throws {
        let clientIdentity = try NvstDtlsIdentity()
        let serverIdentity = try NvstDtlsIdentity()
        // The announced fingerprint is a well-formed value that simply is not the peer's.
        let impostor = String(repeating: "AB:", count: 31) + "AB"
        let client = try NvstDtlsHandshake(role: .client, identity: clientIdentity, expectedPeerFingerprint: impostor)
        let server = try NvstDtlsHandshake(role: .server, identity: serverIdentity, expectedPeerFingerprint: clientIdentity.fingerprint)
        #expect(throws: NvstDtlsHandshake.HandshakeError.self) {
            try pump(client, server)
        }
        #expect(!client.isConnected)
    }

    @Test(arguments: ["SRTP_AEAD_AES_256_GCM", "SRTP_AES128_CM_SHA1_80"])
    func negotiatedProfilesExportUsableDirectionalAudioKeys(_ name: String) throws {
        let clientIdentity = try NvstDtlsIdentity()
        let serverIdentity = try NvstDtlsIdentity()
        let client = try NvstDtlsHandshake(role: .client, identity: clientIdentity, expectedPeerFingerprint: serverIdentity.fingerprint)
        let server = try NvstDtlsHandshake(role: .server, identity: serverIdentity, expectedPeerFingerprint: clientIdentity.fingerprint, srtpProfiles: name)
        try #require(try pump(client, server))
        #expect(client.selectedSrtpProfileName == name)
        let profile = try #require(NvstDtlsTransport.profile(forOpenSSLName: name))
        let length = NvstBundleSrtpKeys.exportedLength(for: profile)
        let clientKeys = try NvstBundleSrtpKeys.split(client.exportKeyingMaterial(length: length), profile: profile)
        let serverKeys = try NvstBundleSrtpKeys.split(server.exportKeyingMaterial(length: length), profile: profile)
        let sender = try NvstAudioSrtp(masterKey: serverKeys.serverMasterKey, masterSalt: serverKeys.serverMasterSalt, profile: profile)
        let receiver = try NvstAudioSrtp(masterKey: clientKeys.serverMasterKey, masterSalt: clientKeys.serverMasterSalt, profile: profile)
        let payload = Data([0x6F, 0x70, 0x75, 0x73])
        let header = NvstAudioRtpPacket.headerBytes(payloadType: 111, marker: false, sequenceNumber: 0, timestamp: 0, ssrc: 1)
        let packet = try sender.protect(header + payload)
        #expect(try receiver.unprotect(packet).payload == payload)
        let wrongDirection = try NvstAudioSrtp(masterKey: clientKeys.clientMasterKey, masterSalt: clientKeys.clientMasterSalt, profile: profile)
        #expect(throws: SrtpCryptoError.authenticationFailed) { try wrongDirection.unprotect(packet) }
    }

    @Test func keyingMaterialIsRefusedBeforeTheHandshakeFinishes() throws {
        let pair = try makePair()
        #expect(throws: NvstDtlsHandshake.HandshakeError.notConnected) {
            try pair.client.exportKeyingMaterial(length: Self.srtpKeyMaterialLength)
        }
    }
}
