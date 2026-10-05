import AudioToolbox
import Darwin
import Foundation
import Testing
@testable import OpenNOWiOS

/// SCTP carried inside DTLS, driven end to end in-process — the boundary a component test cannot
/// see — and the assembled `NvstNativeBundle` against a local seat.
@Suite(.serialized) struct NvstBundleStackTests {
    @Test func allEightChannelsCarryPayloadsBothWaysOverARealDtlsAssociation() throws {
        let clientIdentity = try NvstDtlsIdentity()
        let serverIdentity = try NvstDtlsIdentity()
        let client = try NvstDtlsHandshake(role: .client,
                                           identity: clientIdentity,
                                           expectedPeerFingerprint: serverIdentity.fingerprint)
        let server = try NvstDtlsHandshake(role: .server,
                                           identity: serverIdentity,
                                           expectedPeerFingerprint: clientIdentity.fingerprint)
        let isHandshakeComplete = try pumpHandshake(client, server)
        try #require(isHandshakeComplete, "the DTLS handshake did not complete")

        let wire = DtlsSctpWire(client: client, server: server)
        let association = try NvstSctpAssociation(onOutboundPacket: { wire.clientToServer($0) })
        defer { association.close() }
        let peer = try SctpAcceptingPeer(onOutboundPacket: { wire.serverToClient($0) })
        defer { peer.close() }
        let inbox = SctpInbox()
        association.onInboundMessage = { inbox.append($0) }

        for _ in 0..<1_000 {
            try wire.forward(association: association, peer: peer)
            if association.isEstablished, peer.isConnected { break }
            usleep(2_000)
        }
        #expect(association.diagnosticState == "established", "SCTP stayed \(association.diagnosticState)")
        try #require(peer.isConnected)

        try association.openChannels()
        for _ in 0..<1_000 {
            try wire.forward(association: association, peer: peer)
            if association.isChannelOpen(14) { break }
            usleep(2_000)
        }
        #expect(association.requestedChannelCount == 8)
        let expectedStreams: [UInt16] = [0, 2, 4, 6, 8, 10, 12, 14]
        #expect(expectedStreams.allSatisfy { association.isChannelOpen($0) },
                "open channels: \(expectedStreams.filter { association.isChannelOpen($0) })")
        let opens = peer.messages.filter { $0.ppid == 50 && $0.payload.first == 0x03 }
        #expect(opens.map(\.streamID).sorted() == expectedStreams)

        let command = Data([0x0d, 0x00, 0x06, 0x00, 0xde, 0xad, 0xbe, 0xef, 0x01, 0x02])
        try association.send(command, streamID: 0, ppid: NvstSctpAssociation.PPID.binary)
        try waitForControlPayload(from: peer, matching: NvstSctpAssociation.PPID.binary, association: association, wire: wire)
        let receivedCommand = try #require(peer.messages.first { $0.ppid == NvstSctpAssociation.PPID.binary })
        #expect(receivedCommand.streamID == 0)
        #expect(receivedCommand.payload == command)

        let reply = Data([0x01, 0x02, 0x03])
        try peer.send(reply, streamID: 0, ppid: NvstSctpAssociation.PPID.binary)
        for _ in 0..<500 {
            try wire.forward(association: association, peer: peer)
            if !inbox.messages.isEmpty { break }
            usleep(2_000)
        }
        let deliveredReply = try #require(inbox.messages.first)
        #expect(deliveredReply.streamID == 0)
        #expect(deliveredReply.payload == reply)
    }

    @Test func explicitKeyframeRequestReachesSeatOverReliableDtlsSctpControl() async throws {
        let seat = try await startBundleWithLocalSeat(microphone: nil)
        defer { seat.bundle.close(); seat.transport.close(); seat.peer.close() }
        try await waitUntil(timeout: 15) { seat.bundle.isControlChannelOpen }
        #expect(NativeStreamKeyframeRecovery.request(sendControl: { seat.bundle.sendControl($0) },
            sendUDP: { Issue.record("An established control channel should carry the IDR request") }))
        let expected = Data([0x02, 0x03, 0x02, 0x00, 0x00, 0x00])
        try await waitUntil(timeout: 5) {
            seat.peer.messages.contains { $0.streamID == 0
                && $0.ppid == NvstSctpAssociation.PPID.binary && $0.payload == expected }
        }
        #expect(seat.peer.messages.contains { $0.streamID == 0 && $0.payload == expected })
    }

    @Test func theBundleDecodesSeatAudioOverItsEstablishedChannels() async throws {
        let bundleSeat = try await startBundleWithLocalSeat(microphone: nil)
        defer { bundleSeat.bundle.close(); bundleSeat.transport.close(); bundleSeat.peer.close() }

        try await waitUntil(timeout: 15) { await bundleSeat.bundle.audioReception() != nil }
        try await waitUntil(timeout: 15) { bundleSeat.bundle.isControlChannelOpen }
        #expect(bundleSeat.bundle.isControlChannelOpen, "channels never opened: \(bundleSeat.bundle.diagnosticSummary)")
        #expect(!bundleSeat.bundle.microphoneNegotiation.negotiated, "a microphone was advertised without being requested")
        #expect(bundleSeat.bundle.microphoneNegotiation.senderSsrc == nil)
        #expect(bundleSeat.audioTracks.reportedCount == 2, "the audio-armed callback reported \(String(describing: bundleSeat.audioTracks.reportedCount))")

        try await sendSeatAudio(transport: bundleSeat.transport)
        try await drainUntilDecoded(bundleSeat.bundle)
        let reception = try #require(await bundleSeat.bundle.audioReception())
        #expect(reception.packets > 0, "no packet arrived: \(bundleSeat.bundle.diagnosticSummary)")
        #expect(reception.discarded == 0, "packets failed authentication: \(bundleSeat.bundle.diagnosticSummary)")
        #expect(reception.samples > 0, "nothing decoded: \(bundleSeat.bundle.diagnosticSummary)")
    }

    @Test func theBundleSendsCapturedMicrophoneAudioEncryptedWithItsClientKeys() async throws {
        let microphone = NvstNativeBundle.MicrophoneSetup(volume: 1, initiallyEnabled: true)
        let bundleSeat = try await startBundleWithLocalSeat(microphone: microphone)
        defer { bundleSeat.bundle.close(); bundleSeat.transport.close(); bundleSeat.peer.close() }

        try await waitUntil(timeout: 15) { await bundleSeat.bundle.audioReception() != nil }
        try await waitUntil(timeout: 15) { bundleSeat.bundle.isControlChannelOpen }
        try #require(bundleSeat.bundle.isControlChannelOpen, "channels never opened: \(bundleSeat.bundle.diagnosticSummary)")
        #expect(bundleSeat.bundle.microphoneNegotiation.negotiated)

        let seatAudio = MicrophonePacketSink()
        bundleSeat.transport.onSecureAudio = { seatAudio.append($0) }
        for _ in 0..<12 {
            Self.sendOneMicrophoneCallback(through: bundleSeat.bundle)
            try await Task.sleep(nanoseconds: 5_000_000)
        }

        try await waitUntil(timeout: 5) { seatAudio.packets.count >= 2 }
        let packets = seatAudio.packets
        #expect(packets.count >= 2, "the bundle sent \(packets.count) microphone packet(s)")
        let (keys, profile) = try bundleSeat.transport.exportedKeys()
        // The bundle is the DTLS client, so the seat reads its microphone with the client write keys.
        let reader = try NvstAudioSrtp(masterKey: keys.clientMasterKey, masterSalt: keys.clientMasterSalt, profile: profile)
        let (first, firstPayload) = try reader.unprotect(packets[0])
        let (second, _) = try reader.unprotect(packets[1])
        #expect(first.ssrc == NvstAudioSendPipeline.microphoneSSRC)
        #expect(first.payloadType == NvstAudioSendPipeline.opusPayloadType)
        #expect(!firstPayload.isEmpty)
        // The seat's microphone contract is `x-nv-mic.frameSize:10`: ten milliseconds, or 480 samples
        // at 48 kHz, per packet. A 5 ms clock here is the regression that left the seat's mic meter dead.
        #expect(second.timestamp &- first.timestamp == 480, "the microphone clock advanced \((second.timestamp &- first.timestamp)) samples, not a 10 ms frame")
    }

    @Test func theSeatInputProtocolAnnouncementUnlocksInputWhileCommandsReachTheSeat() async throws {
        let bundleSeat = try await startBundleWithLocalSeat(microphone: nil)
        defer { bundleSeat.bundle.close(); bundleSeat.transport.close(); bundleSeat.peer.close() }
        try await waitUntil(timeout: 15) { bundleSeat.bundle.isControlChannelOpen }
        try #require(bundleSeat.bundle.isControlChannelOpen, "channels never opened: \(bundleSeat.bundle.diagnosticSummary)")
        #expect(bundleSeat.bundle.inputProtocolVersion == nil)
        #expect(!bundleSeat.bundle.isInputReady)

        let protocolVersion = ProtocolVersionBox()
        bundleSeat.bundle.onInputProtocolNegotiated = { protocolVersion.record($0) }
        try bundleSeat.peer.send(Data([0x0e, 0x02, 0x02, 0x00, 0x03, 0x00]),
                                 streamID: 0, ppid: NvstSctpAssociation.PPID.binary)
        try await waitUntil(timeout: 5) { bundleSeat.bundle.inputProtocolVersion == 3 }
        #expect(bundleSeat.bundle.inputProtocolVersion == 3, "input protocol never negotiated: \(bundleSeat.bundle.diagnosticSummary)")
        #expect(bundleSeat.bundle.isInputReady, "input never became ready: \(bundleSeat.bundle.diagnosticSummary)")
        #expect(protocolVersion.negotiated == 3)

        let command = NvstControlCommand(code: .pingBackAck, payload: Data([0x01, 0x00]))
        #expect(bundleSeat.bundle.sendControl(command))
        let inputEvent = Data([0x0e, 0x00, 0x00, 0x00])
        #expect(bundleSeat.bundle.sendInput(inputEvent))
        try await waitUntil(timeout: 5) {
            bundleSeat.peer.messages.contains { $0.streamID == 0 && $0.ppid == NvstSctpAssociation.PPID.binary }
                && bundleSeat.peer.messages.contains { $0.streamID == 10 && $0.ppid == NvstSctpAssociation.PPID.binary }
        }
        let encodedCommand = try command.encoded
        #expect(bundleSeat.peer.messages.contains { $0.streamID == 0 && $0.ppid == NvstSctpAssociation.PPID.binary && $0.payload == encodedCommand },
                "the control command did not reach the seat: \(bundleSeat.bundle.diagnosticSummary)")
        #expect(bundleSeat.peer.messages.contains { $0.streamID == 10 && $0.ppid == NvstSctpAssociation.PPID.binary && $0.payload == inputEvent },
                "the input event did not reach the seat: \(bundleSeat.bundle.diagnosticSummary)")
    }

    @Test func theMicrophoneDecisionIsFinalBeforeTheHandshake() async throws {
        let withMicrophoneSocket = try LoopbackDatagramSocket()
        let withMicrophone = NvstNativeBundle(handoff: Self.seatHandoff(peerPort: withMicrophoneSocket.port), identity: try NvstDtlsIdentity(), startsAudioDevice: false)
        defer { withMicrophone.close() }
        _ = try await withMicrophone.prepare(microphone: .init(volume: 1, initiallyEnabled: true), audioChannelCount: 2)
        #expect(withMicrophone.microphoneNegotiation.negotiated)
        #expect(withMicrophone.microphoneNegotiation.senderSsrc == NvstAudioSendPipeline.microphoneSSRC)

        let withoutMicrophoneSocket = try LoopbackDatagramSocket()
        let withoutMicrophone = NvstNativeBundle(handoff: Self.seatHandoff(peerPort: withoutMicrophoneSocket.port), identity: try NvstDtlsIdentity(), startsAudioDevice: false)
        defer { withoutMicrophone.close() }
        _ = try await withoutMicrophone.prepare(microphone: nil, audioChannelCount: 2)
        #expect(!withoutMicrophone.microphoneNegotiation.negotiated)
        #expect(withoutMicrophone.microphoneNegotiation.senderSsrc == nil)
    }

    private func startBundleWithLocalSeat(microphone: NvstNativeBundle.MicrophoneSetup?) async throws
        -> (bundle: NvstNativeBundle, transport: NvstDtlsTransport, peer: SctpAcceptingPeer, audioTracks: AudioTrackBox) {
        let serverSocket = try LoopbackDatagramSocket()
        let clientIdentity = try NvstDtlsIdentity()
        let bundle = NvstNativeBundle(handoff: Self.seatHandoff(peerPort: serverSocket.port), identity: clientIdentity, startsAudioDevice: false)
        // Set before the handshake so the audio-armed callback is observed deterministically.
        let audioTracks = AudioTrackBox()
        bundle.onRemoteAudio = { audioTracks.record($0) }
        bundle.setRemoteAudioMuted(true)
        let identity = try await bundle.prepare(microphone: microphone, audioChannelCount: 2)
        let seat = try startSeat(socket: serverSocket,
                                 identity: try NvstDtlsIdentity(),
                                 clientFingerprint: clientIdentity.fingerprint,
                                 clientPort: identity.bundlePort)
        return (bundle, seat.transport, seat.peer, audioTracks)
    }

    private static func sendOneMicrophoneCallback(through bundle: NvstNativeBundle) {
        var samples = microphoneTone()
        samples.withUnsafeMutableBytes { storage in
            var list = AudioBufferList(
                mNumberBuffers: 1,
                mBuffers: AudioBuffer(mNumberChannels: 2, mDataByteSize: UInt32(storage.count), mData: storage.baseAddress)
            )
            withUnsafePointer(to: &list) { bundle.sendCapturedMicrophone(pointer: UnsafeRawPointer($0), frames: 240) }
        }
    }

    private static func microphoneTone() -> [Int16] {
        (0..<(240 * 2)).map { index in
            let value = sin(2 * Double.pi * 440 * Double(index / 2) / 48_000) * 0.5
            return Int16(clamping: Int((value * Double(Int16.max)).rounded()))
        }
    }

    private func startSeat(socket: LoopbackDatagramSocket,
                           identity: NvstDtlsIdentity,
                           clientFingerprint: String,
                           clientPort: UInt16) throws -> (transport: NvstDtlsTransport, peer: SctpAcceptingPeer) {
        let transport = try NvstDtlsTransport(
            handshake: try NvstDtlsHandshake(role: .server, identity: identity,
                                             expectedPeerFingerprint: clientFingerprint),
            descriptor: socket.takeDescriptor(),
            localAddress: "127.0.0.1",
            localPort: socket.port,
            peerAddress: "127.0.0.1",
            peerPort: clientPort
        )
        let peer = try SctpAcceptingPeer(onOutboundPacket: { try? transport.sendEncrypted($0) })
        transport.onApplicationData = { payload in
            peer.feed(payload)
            try? peer.receive()
        }
        transport.start()
        return (transport, peer)
    }

    private func sendSeatAudio(transport: NvstDtlsTransport) async throws {
        let (keys, profile) = try transport.exportedKeys()
        let sender = try NvstAudioSrtp(masterKey: keys.serverMasterKey, masterSalt: keys.serverMasterSalt, profile: profile)
        let payload = try Self.opusTone()
        var packetizer = NvstAudioRtpPacketizer(ssrc: 1, payloadType: 111, initialSequenceNumber: 1, initialTimestamp: 0)
        for _ in 0..<16 {
            let rtp = packetizer.packet(payload: payload, framesPerPacket: 240)
            try transport.sendRaw(sender.protect(rtp))
            try await Task.sleep(nanoseconds: 5_000_000)
        }
    }

    /// Drains whatever the render device has not already taken; the decoded-sample counter counts
    /// both paths, so this holds whether or not CoreAudio started on the test machine.
    private func drainUntilDecoded(_ bundle: NvstNativeBundle) async throws {
        try await waitUntil(timeout: 5) { (await bundle.audioReception())?.packets ?? 0 > 0 }
        for _ in 0..<50 {
            _ = bundle.pullAudio()
            if (await bundle.audioReception())?.samples ?? 0 > 0 { return }
            try await Task.sleep(nanoseconds: 20_000_000)
        }
    }

    private static func seatHandoff(peerPort: UInt16) -> NVSTVideoHandoff {
        NVSTVideoHandoff(
            clientUDPPort: 0,
            videoPeerIP: "127.0.0.1",
            videoPeerPort: peerPort,
            srtpProfile: .aeadAes256Gcm8,
            srtpAESKey: Data(repeating: 0x11, count: 32),
            srtpSalt: Data(repeating: 0x22, count: 12),
            codec: .h264,
            rtpPayloadType: 96,
            rtpSSRC: 1,
            reorderWindowPackets: 64,
            maxAccessUnitBytes: 1_048_576,
            timeoutMilliseconds: 8_000,
            pingVersion: nil,
            pingPayload: "",
            mjolnirUDPPort: nil,
            iceCredentials: nil
        )
    }

    private static func opusTone() throws -> Data {
        let encoder = try NvstOpusEncoder(channels: 2, framesPerPacket: 240)
        var tone = [Float](repeating: 0, count: 240 * 2)
        for frame in 0..<240 {
            let value = Float(sin(2 * Double.pi * 440 * Double(frame) / 48_000)) * 0.5
            tone[frame * 2] = value
            tone[frame * 2 + 1] = value
        }
        var encoded: Data?
        for _ in 0..<8 where encoded == nil { encoded = try encoder.encode(tone) }
        return try #require(encoded)
    }

    private func waitUntil(timeout: TimeInterval, _ condition: () async -> Bool) async throws {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if await condition() { return }
            try await Task.sleep(nanoseconds: 20_000_000)
        }
    }

    private func waitForControlPayload(from peer: SctpAcceptingPeer,
                                       matching ppid: UInt32,
                                       association: NvstSctpAssociation,
                                       wire: DtlsSctpWire) throws {
        for _ in 0..<500 {
            try wire.forward(association: association, peer: peer)
            if peer.messages.contains(where: { $0.ppid == ppid }) { return }
            usleep(2_000)
        }
    }

    private func pumpHandshake(_ client: NvstDtlsHandshake, _ server: NvstDtlsHandshake, rounds: Int = 64) throws -> Bool {
        var toClient: Data?
        var toServer: Data?
        for _ in 0..<rounds {
            if !client.isConnected {
                toClient = try client.handshakeStep(received: toServer)
                toServer = nil
            }
            if !server.isConnected {
                toServer = try server.handshakeStep(received: toClient)
                toClient = nil
            }
            if client.isConnected, server.isConnected { return true }
        }
        return false
    }
}

/// A UDP socket on loopback, handing its descriptor to a transport the way the reservation does.
private final class LoopbackDatagramSocket: @unchecked Sendable {
    private var descriptor: Int32
    let port: UInt16

    init() throws {
        let socketDescriptor = socket(AF_INET, SOCK_DGRAM, 0)
        guard socketDescriptor >= 0 else { throw NvstDtlsTransport.TransportError.socketUnavailable(String(cString: strerror(errno))) }
        var address = sockaddr_in()
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = 0
        address.sin_addr.s_addr = inet_addr("127.0.0.1")
        let bound = withUnsafePointer(to: &address) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                bind(socketDescriptor, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
            }
        }
        guard bound == 0 else {
            Darwin.close(socketDescriptor)
            throw NvstDtlsTransport.TransportError.socketUnavailable("bind: \(String(cString: strerror(errno)))")
        }
        var local = sockaddr_in()
        var length = socklen_t(MemoryLayout<sockaddr_in>.size)
        let named = withUnsafeMutablePointer(to: &local) { pointer in
            pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                getsockname(socketDescriptor, $0, &length)
            }
        }
        guard named == 0 else {
            Darwin.close(socketDescriptor)
            throw NvstDtlsTransport.TransportError.socketUnavailable("getsockname: \(String(cString: strerror(errno)))")
        }
        self.descriptor = socketDescriptor
        self.port = UInt16(bigEndian: local.sin_port)
    }

    deinit {
        if descriptor >= 0 { Darwin.close(descriptor) }
    }

    func takeDescriptor() -> Int32 {
        let taken = descriptor
        descriptor = -1
        return taken
    }
}

/// Routes SCTP packets through the two DTLS record layers, so a fault in the boundary — a packet
/// encrypted twice, or ciphertext fed raw — fails here.
private final class DtlsSctpWire: @unchecked Sendable {
    private let lock = NSLock()
    private var toServer: [Data] = []
    private var toClient: [Data] = []
    private let client: NvstDtlsHandshake
    private let server: NvstDtlsHandshake

    init(client: NvstDtlsHandshake, server: NvstDtlsHandshake) {
        self.client = client
        self.server = server
    }

    func clientToServer(_ packet: Data) { lock.withLock { toServer.append(packet) } }
    func serverToClient(_ packet: Data) { lock.withLock { toClient.append(packet) } }

    func forward(association: NvstSctpAssociation, peer: SctpAcceptingPeer) throws {
        for packet in drainToServer() {
            if let record = try client.writeApplicationData(packet) { try server.feedDatagram(record) }
        }
        while let payload = try server.readApplicationData() { peer.feed(payload) }
        try peer.receive()
        for packet in drainToClient() {
            if let record = try server.writeApplicationData(packet) { try client.feedDatagram(record) }
        }
        while let payload = try client.readApplicationData() { association.feedInbound(payload) }
        association.drainInbound()
    }

    private func drainToServer() -> [Data] { lock.withLock { defer { toServer.removeAll() }; return toServer } }
    private func drainToClient() -> [Data] { lock.withLock { defer { toClient.removeAll() }; return toClient } }
}

private final class SctpInbox: @unchecked Sendable {
    private let lock = NSLock()
    private var receivedMessages: [NvstSctpAssociation.InboundMessage] = []
    var messages: [NvstSctpAssociation.InboundMessage] { lock.withLock { receivedMessages } }
    func append(_ message: NvstSctpAssociation.InboundMessage) { lock.withLock { receivedMessages.append(message) } }
}

/// Collects the raw SRTP the seat receives from the bundle's microphone, so the up-path can be
/// authenticated independently of the CoreAudio device.
private final class MicrophonePacketSink: @unchecked Sendable {
    private let lock = NSLock()
    private var receivedPackets: [Data] = []
    var packets: [Data] { lock.withLock { receivedPackets } }
    func append(_ packet: Data) { lock.withLock { receivedPackets.append(packet) } }
}

private final class ProtocolVersionBox: @unchecked Sendable {
    private let lock = NSLock()
    private var negotiatedVersion: UInt16?
    var negotiated: UInt16? { lock.withLock { negotiatedVersion } }
    func record(_ version: UInt16) { lock.withLock { negotiatedVersion = version } }
}

private final class AudioTrackBox: @unchecked Sendable {
    private let lock = NSLock()
    private var trackCount: Int?
    var reportedCount: Int? { lock.withLock { trackCount } }
    func record(_ count: Int) { lock.withLock { trackCount = count } }
}
