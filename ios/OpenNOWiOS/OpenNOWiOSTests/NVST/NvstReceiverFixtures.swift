//  The handoff, the SRTP sealing and the packet builders the Mjolnir receiver suites share.
//

import Foundation
import Testing
@testable import OpenNOWiOS

enum NvstReceiverFixtures {
    static let mediaSSRC: UInt32 = 0x1122_3344

    static func hex(_ text: String) -> Data {
        var data = Data()
        var index = text.startIndex
        while index < text.endIndex {
            let next = text.index(index, offsetBy: 2)
            data.append(UInt8(text[index..<next], radix: 16)!)
            index = next
        }
        return data
    }

    static func makeHandoff(profile: NVSTSrtpProfile = .aeadAes256Gcm8,
                             reorderWindow: Int = 32,
                             expectedSSRC: UInt32 = NvstReceiverFixtures.mediaSSRC) -> NVSTVideoHandoff {
        NVSTVideoHandoff(
            clientUDPPort: 0,
            videoPeerIP: "192.0.2.20",
            videoPeerPort: 5004,
            srtpProfile: profile,
            srtpAESKey: hex(String(repeating: "ab", count: profile.masterKeyLength)),
            srtpSalt: hex(String(repeating: "9e", count: profile.masterSaltLength)),
            codec: .h264,
            rtpPayloadType: 96,
            rtpSSRC: expectedSSRC,
            reorderWindowPackets: reorderWindow,
            maxAccessUnitBytes: 2 * 1024 * 1024,
            timeoutMilliseconds: 5000,
            pingVersion: nil,
            pingPayload: "PING",
            mjolnirUDPPort: 0,
            iceCredentials: nil
        )
    }

    /// Protects a plaintext GS/RTP packet exactly the way the seat does: AES-GCM over the
    /// payload with the RTP header (extension included) as AAD.
    static func seal(_ packet: Data, sequence: UInt16, handoff: NVSTVideoHandoff, rolloverCounter: UInt32 = 0, ssrc: UInt32 = NvstReceiverFixtures.mediaSSRC) throws -> Data {
        let sessionKey = try SrtpKeyDerivation.derive(key: handoff.srtpAESKey, salt: handoff.srtpSalt, label: 0x00, length: handoff.srtpProfile.masterKeyLength)
        let sessionSalt = try SrtpKeyDerivation.derive(key: handoff.srtpAESKey, salt: handoff.srtpSalt, label: 0x02, length: 12)
        let cipher = try SrtpGcm8(key: sessionKey)
        let headerLength = 32 // 12-byte RTP header + 4-byte extension header + 16-byte GS block
        let aad = packet.prefix(headerLength)
        let plaintext = packet.dropFirst(headerLength)
        let iv = try SrtpKeyDerivation.gcmIV(sessionSalt: sessionSalt, ssrc: ssrc, rolloverCounter: rolloverCounter, sequenceNumber: sequence)
        let sealed = try cipher.seal(iv: iv, aad: Data(aad), plaintext: Data(plaintext), tagLength: handoff.srtpProfile.authenticationTagLength)
        return Data(aad) + sealed
    }

    static func packet(sequence: UInt16, frameIndex: UInt32, flags: UInt8, media: [UInt8],
                        fecWord: UInt32 = 0, streamSequence: UInt32? = nil) -> Data {
        NvstVideoPacketTests.buildPacket(sequence: sequence, frameIndex: frameIndex, flags: flags,
                                         media: media, fecWord: fecWord, streamSequence: streamSequence)
    }

    static func frames(_ events: [NvstReceiveEvent]) -> [NvstAccessUnit] {
        events.compactMap { if case .frame(let unit) = $0 { return unit } else { return nil } }
    }

    static func drops(_ events: [NvstReceiveEvent]) -> [String] {
        events.compactMap { if case .dropped(let reason) = $0 { return reason.description } else { return nil } }
    }

    static func recoveries(_ events: [NvstReceiveEvent]) -> Int {
        events.filter { if case .recoveryNeeded = $0 { return true } else { return false } }.count
    }
}
