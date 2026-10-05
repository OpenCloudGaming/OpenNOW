import Darwin
import Foundation
import Testing
import usrsctp
@testable import OpenNOWiOS

@Suite(.serialized) struct NvstSctpAssociationTests {
    @Test func anAcceptingPeerAcknowledgesAllEightChannels() throws {
        let wire = SctpTestWire()
        let peer = try SctpAcceptingPeer(onOutboundPacket: { wire.enqueueForClient($0) })
        let association = try NvstSctpAssociation(onOutboundPacket: { wire.enqueueForPeer($0) })
        defer { association.close() }

        let initial = try #require(wire.peerPackets.first)
        #expect(Array(initial.prefix(4)) == [0x13, 0x88, 0x13, 0x88])
        try connect(association, peer: peer, wire: wire)
        try association.openChannels()
        try association.openChannels()
        for _ in 0..<500 {
            try pump(association, peer: peer, wire: wire)
            if association.isChannelOpen(14) { break }
            usleep(2_000)
        }
        #expect(peer.messages.count == 8)
        #expect(peer.messages.map(\.streamID).sorted() == [0, 2, 4, 6, 8, 10, 12, 14])
        for message in peer.messages {
            #expect(message.ppid == 50)
            #expect(message.payload.first == 0x03)
            #expect(association.isChannelOpen(message.streamID))
        }
        #expect(association.requestedChannelCount == 8)
        #expect(association.diagnosticState == "established")
    }

    @Test func binaryPayloadsKeepTheirStreamsWhileReassemblingInBothDirections() throws {
        let wire = SctpTestWire()
        let peer = try SctpAcceptingPeer(onOutboundPacket: { wire.enqueueForClient($0) })
        let association = try NvstSctpAssociation(onOutboundPacket: { wire.enqueueForPeer($0) })
        defer { association.close() }
        try connect(association, peer: peer, wire: wire)
        try association.openChannels()
        for _ in 0..<100 {
            try pump(association, peer: peer, wire: wire)
            if association.isChannelOpen(10) { break }
            usleep(2_000)
        }
        try #require(association.isChannelOpen(10))
        let input = Data([0x00, 0xFF, 0x80, 0x01])
        try association.send(input, streamID: 10, ppid: 53)
        let reply = Data((0..<80_000).map { UInt8(truncatingIfNeeded: $0) })
        let inbox = SctpTestInbox()
        association.onInboundMessage = { inbox.append($0) }
        try peer.send(reply, streamID: 0, ppid: 53)
        for _ in 0..<1_000 {
            try pump(association, peer: peer, wire: wire)
            if !inbox.messages.isEmpty, peer.messages.contains(where: { $0.ppid == 53 }) { break }
            usleep(2_000)
        }
        let receivedInput = try #require(peer.messages.first { $0.ppid == 53 })
        #expect(receivedInput.streamID == 10)
        #expect(receivedInput.payload == input)
        let receivedReply = try #require(inbox.messages.first)
        #expect(inbox.messages.count == 1)
        #expect(receivedReply.streamID == 0)
        #expect(receivedReply.ppid == 53)
        #expect(receivedReply.payload == reply)
    }

    @Test func messagesUseTheirReliabilityPolicyWhileDcepRemainsReliable() {
        let timed = NvstSctpAssociation.sendParameters(streamID: 10, ppid: 53)
        #expect(timed.sendv_flags & UInt32(SCTP_SEND_SNDINFO_VALID) != 0)
        #expect(timed.sendv_flags & UInt32(SCTP_SEND_PRINFO_VALID) != 0)
        #expect(timed.sendv_sndinfo.snd_ppid == UInt32(53).bigEndian)
        #expect(timed.sendv_prinfo.pr_policy == SCTP_PR_SCTP_TTL)
        #expect(timed.sendv_prinfo.pr_value == 300)
        let unreliable = NvstSctpAssociation.sendParameters(streamID: 8, ppid: 53)
        #expect(unreliable.sendv_prinfo.pr_policy == SCTP_PR_SCTP_RTX)
        #expect(unreliable.sendv_prinfo.pr_value == 0)
        let open = NvstSctpAssociation.sendParameters(streamID: 10, ppid: 50)
        #expect(open.sendv_prinfo.pr_policy == SCTP_PR_SCTP_NONE)
        #expect(open.sendv_sndinfo.snd_flags & UInt16(SCTP_UNORDERED) == 0)
    }

    @Test func aClosedAssociationReleasesItsSocketWhileRejectingFurtherSends() throws {
        let association = try NvstSctpAssociation(onOutboundPacket: { _ in })
        #expect(throws: NvstSctpAssociation.AssociationError.notConnected) { try association.openChannels() }
        association.close()
        association.close()
        #expect(!association.isEstablished)
        #expect(association.diagnosticState == "closed")
        #expect(throws: NvstSctpAssociation.AssociationError.notConnected) {
            try association.send(Data([1]), streamID: 0, ppid: 53)
        }
    }

    private func connect(_ association: NvstSctpAssociation, peer: SctpAcceptingPeer, wire: SctpTestWire) throws {
        for _ in 0..<500 {
            try pump(association, peer: peer, wire: wire)
            if association.isEstablished, peer.isConnected { return }
            usleep(2_000)
        }
        try #require(association.isEstablished, "SCTP stayed \(association.diagnosticState)")
        try #require(peer.isConnected)
    }

    private func pump(_ association: NvstSctpAssociation, peer: SctpAcceptingPeer, wire: SctpTestWire) throws {
        for packet in wire.takePeerPackets() { peer.feed(packet) }
        try peer.receive()
        for packet in wire.takeClientPackets() { association.feedInbound(packet) }
        association.drainInbound()
    }
}

private final class SctpTestWire: @unchecked Sendable {
    private let lock = NSLock()
    private var toPeer: [Data] = []
    private var toClient: [Data] = []

    var peerPackets: [Data] { lock.withLock { toPeer } }
    func enqueueForPeer(_ packet: Data) { lock.withLock { toPeer.append(packet) } }
    func enqueueForClient(_ packet: Data) { lock.withLock { toClient.append(packet) } }

    func takePeerPackets() -> [Data] {
        lock.withLock {
            defer { toPeer.removeAll() }
            return toPeer
        }
    }

    func takeClientPackets() -> [Data] {
        lock.withLock {
            defer { toClient.removeAll() }
            return toClient
        }
    }
}

private final class SctpTestInbox: @unchecked Sendable {
    private let lock = NSLock()
    private var received: [NvstSctpAssociation.InboundMessage] = []
    var messages: [NvstSctpAssociation.InboundMessage] { lock.withLock { received } }
    func append(_ message: NvstSctpAssociation.InboundMessage) { lock.withLock { received.append(message) } }
}

final class SctpAcceptingPeer: @unchecked Sendable {
    private let lock = NSRecursiveLock()
    private let token: UInt
    private var listener: OpaquePointer?
    private var accepted: OpaquePointer?
    private var receivedMessages: [NvstSctpAssociation.InboundMessage] = []
    private var isClosed = false

    var messages: [NvstSctpAssociation.InboundMessage] { lock.withLock { receivedMessages } }
    var isConnected: Bool { lock.withLock { accepted != nil } }

    init(onOutboundPacket: @escaping @Sendable (Data) -> Void) throws {
        token = NvstSctpRuntime.register(output: onOutboundPacket)
        guard let listener = usrsctp_socket(AF_CONN, SOCK_STREAM, 132, nil, nil, 0, nil) else {
            NvstSctpRuntime.deregister(token)
            throw NvstSctpAssociation.AssociationError.socketUnavailable
        }
        self.listener = listener
        do {
            try #require(usrsctp_set_non_blocking(listener, 1) == 0)
            var enabled: Int32 = 1
            try #require(usrsctp_setsockopt(listener, 132, SCTP_RECVRCVINFO, &enabled, 4) == 0)
            try #require(usrsctp_setsockopt(listener, 132, SCTP_NODELAY, &enabled, 4) == 0)
            var streams = sctp_initmsg(sinit_num_ostreams: 32, sinit_max_instreams: 32, sinit_max_attempts: 0, sinit_max_init_timeo: 0)
            try #require(usrsctp_setsockopt(listener, 132, SCTP_INITMSG, &streams, socklen_t(MemoryLayout<sctp_initmsg>.size)) == 0)
            var address = sockaddr_conn()
            address.sconn_len = UInt8(MemoryLayout<sockaddr_conn>.size)
            address.sconn_family = UInt8(AF_CONN)
            address.sconn_port = UInt16(5000).bigEndian
            address.sconn_addr = UnsafeMutableRawPointer(bitPattern: token)
            let result = withUnsafeMutablePointer(to: &address) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    usrsctp_bind(listener, $0, socklen_t(MemoryLayout<sockaddr_conn>.size))
                }
            }
            try #require(result == 0, "bind: \(String(cString: strerror(errno)))")
            try #require(usrsctp_listen(listener, 1) == 0)
        } catch {
            close()
            throw error
        }
    }

    deinit { close() }

    /// Serializes every usrsctp call. The transport's receive queue calls `receive()` from a
    /// background thread while a test's teardown calls `close()`, and closing the socket mid-read is
    /// a use-after-free inside `usrsctp_recvv` that takes the whole test runner down with it.
    func close() {
        lock.lock()
        defer { lock.unlock() }
        guard !isClosed else { return }
        isClosed = true
        if let accepted {
            var option = linger(l_onoff: 1, l_linger: 0)
            usrsctp_setsockopt(accepted, SOL_SOCKET, SO_LINGER, &option, socklen_t(MemoryLayout<linger>.size))
            usrsctp_close(accepted)
            self.accepted = nil
        }
        if let listener {
            usrsctp_close(listener)
            self.listener = nil
        }
        NvstSctpRuntime.deregister(token)
    }

    func feed(_ packet: Data) {
        lock.lock()
        defer { lock.unlock() }
        guard !isClosed else { return }
        packet.withUnsafeBytes {
            usrsctp_conninput(UnsafeMutableRawPointer(bitPattern: token), $0.baseAddress, $0.count, 0)
        }
    }

    func receive() throws {
        lock.lock()
        defer { lock.unlock() }
        guard !isClosed, let listener else { return }
        if accepted == nil { accepted = usrsctp_accept(listener, nil, nil) }
        guard let accepted else { return }
        try #require(usrsctp_set_non_blocking(accepted, 1) == 0)
        var buffer = [UInt8](repeating: 0, count: 65_536)
        while true {
            var info = sctp_rcvinfo()
            var length = socklen_t(MemoryLayout<sctp_rcvinfo>.size)
            var infoType: UInt32 = 0
            var flags: Int32 = 0
            let count = buffer.withUnsafeMutableBytes {
                usrsctp_recvv(accepted, $0.baseAddress, $0.count, nil, nil, &info, &length, &infoType, &flags)
            }
            guard count > 0 else { return }
            try #require(flags & MSG_EOR != 0)
            let ppid = UInt32(bigEndian: info.rcv_ppid)
            receivedMessages.append(.init(streamID: info.rcv_sid, ppid: ppid, payload: Data(buffer[..<count])))
            if ppid == 50, buffer[0] == 0x03 { try send(Data([0x02]), streamID: info.rcv_sid, ppid: 50) }
        }
    }

    func send(_ payload: Data, streamID: UInt16, ppid: UInt32) throws {
        lock.lock()
        defer { lock.unlock() }
        let socket = try #require(accepted)
        var info = sctp_sndinfo()
        info.snd_sid = streamID
        info.snd_ppid = ppid.bigEndian
        let result = payload.withUnsafeBytes {
            usrsctp_sendv(socket, $0.baseAddress, $0.count, nil, 0, &info,
                          socklen_t(MemoryLayout<sctp_sndinfo>.size), UInt32(SCTP_SENDV_SNDINFO), 0)
        }
        try #require(result == payload.count, "send: \(String(cString: strerror(errno)))")
    }
}
