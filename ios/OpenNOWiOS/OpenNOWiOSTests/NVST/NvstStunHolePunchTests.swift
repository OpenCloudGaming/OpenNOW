import Foundation
import Testing
@testable import OpenNOWiOS

@Suite(.serialized)
struct NvstStunHolePunchTests {

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

    // Known answer from upstream's version_six_binding_request test.
    @Test func versionSixBindingRequestMatchesKnownAnswer() throws {
        let packet = NvstStunHolePunch.buildBindingRequest(
            transactionID: Data(Array(0...11)),
            username: "remote01:loc1",
            integrityKey: Data("remote-password-with-36-byte-value-001".utf8)
        )
        let expected = hex("000100342112A442000102030405060708090A0B0006000D72656D6F746530313A6C6F633100000000080014B276DC1C7949494C7EF7EB226BE8BB5E0EE5AABD802800045A8349EF")
        #expect(packet == expected)
    }

    @Test func nattHolePunchUsesSetupPingPayloadAsUsername() throws {
        let packet = NvstStunHolePunch.buildNattHolePunchRequest(
            localUfrag: "loc1",
            pingPayload: "srv1",
            remotePassword: Data("remote-password-with-36-byte-value-001".utf8),
            transactionID: Data(Array(0...11))
        )
        let unwrapped = try #require(packet)
        let length = Int(unwrapped[22]) << 8 | Int(unwrapped[23])
        let username = String(decoding: unwrapped[24..<(24 + length)], as: UTF8.self)
        #expect(username == "srv1:loc1")
    }

    @Test func xorMappedAddressBuildsExpectedXor() {
        let transactionID = Data(Array(0...11))
        let built = NvstStunHolePunch.xorMappedAddress(host: "192.0.2.20", port: 0xd78a ^ 0x2112, transactionID: transactionID)
        let unwrapped = built
        #expect(unwrapped != nil)
        guard let unwrapped else { return }
        #expect(unwrapped[0] == 0x00 && unwrapped[1] == 0x01)
        let address = UInt32(192) << 24 | UInt32(2) << 8 | UInt32(20)
        let expectedXor = address ^ 0x2112_a442
        let actual = UInt32(unwrapped[4]) << 24 | UInt32(unwrapped[5]) << 16 | UInt32(unwrapped[6]) << 8 | UInt32(unwrapped[7])
        #expect(actual == expectedXor)
    }

    @Test func fingerprintIsLastAttribute() {
        let packet = NvstStunHolePunch.buildBindingRequest(transactionID: Data(Array(0...11)), username: "a:b", integrityKey: Data("k".utf8))
        guard let packet else {
            Issue.record("binding request build failed")
            return
        }
        // header(20) + username "a:b"(4+1pad) + integrity(4+20) + fingerprint(4+4) = 60
        #expect(packet.count == 60)
        #expect(UInt16(packet[packet.count - 8]) << 8 | UInt16(packet[packet.count - 7]) == 0x8028)
    }
}
