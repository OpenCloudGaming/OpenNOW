import Darwin
import Foundation
import usrsctp

private let opennowSctpOutput: @convention(c) (UnsafeMutableRawPointer?, UnsafeMutableRawPointer?, Int, UInt8, UInt8) -> Int32 = { address, buffer, length, _, _ in
    guard let address, let buffer, length > 0 else { return EINVAL }
    return NvstSctpRuntime.deliver(address: address, packet: Data(bytes: buffer, count: length))
}

enum NvstSctpRuntime {
    private static let lock = NSLock()
    nonisolated(unsafe) private static var isInitialized = false
    nonisolated(unsafe) private static var nextToken: UInt = 0x1000
    nonisolated(unsafe) private static var outputs: [UInt: @Sendable (Data) -> Void] = [:]

    static func register(output: @escaping @Sendable (Data) -> Void) -> UInt {
        lock.lock()
        defer { lock.unlock() }
        if !isInitialized {
            usrsctp_init(0, opennowSctpOutput, nil)
            usrsctp_sysctl_set_sctp_ecn_enable(0)
            isInitialized = true
        }
        nextToken += 1
        let token = nextToken
        outputs[token] = output
        usrsctp_register_address(UnsafeMutableRawPointer(bitPattern: token))
        return token
    }

    static func deregister(_ token: UInt) {
        lock.withLock { outputs[token] = nil }
        usrsctp_deregister_address(UnsafeMutableRawPointer(bitPattern: token))
    }

    static func deliver(address: UnsafeMutableRawPointer, packet: Data) -> Int32 {
        guard let output = lock.withLock({ outputs[UInt(bitPattern: address)] }) else { return ENOTCONN }
        output(packet)
        return 0
    }
}

public final class NvstSctpAssociation: @unchecked Sendable {
    public enum AssociationError: LocalizedError, Equatable {
        case socketUnavailable
        case configurationFailed(String)
        case bindFailed(String)
        case connectionFailed(String)
        case notConnected
        case sendFailed(String)

        public var errorDescription: String? {
            switch self {
            case .socketUnavailable: "usrsctp could not create the SCTP association socket."
            case .configurationFailed(let reason): "usrsctp could not configure the SCTP association: \(reason)"
            case .bindFailed(let reason): "usrsctp could not bind the SCTP association: \(reason)"
            case .connectionFailed(let reason): "usrsctp could not start the SCTP association: \(reason)"
            case .notConnected: "The SCTP association is not established."
            case .sendFailed(let reason): "The SCTP association could not send: \(reason)"
            }
        }
    }

    public enum PPID {
        public static let binary: UInt32 = 53
    }

    public static let port: UInt16 = 5000

    public struct InboundMessage: Sendable {
        public let streamID: UInt16
        public let ppid: UInt32
        public let payload: Data
    }

    public var onInboundMessage: (@Sendable (InboundMessage) -> Void)?
    public var onChannelOpened: (@Sendable (UInt16) -> Void)?

    private let lock = NSRecursiveLock()
    private let token: UInt
    private var socket: OpaquePointer?
    private var openStreams: Set<UInt16> = []
    private var requestedStreams: Set<UInt16> = []
    private var receivedPacketCount: UInt64 = 0
    private var notificationCount: UInt64 = 0
    private var streamResetCount: UInt64 = 0
    private var partialMessages: [UInt16: InboundMessage] = [:]
    private var discardedStreams: Set<UInt16> = []
    private static let maximumMessageBytes = 1_048_576

    public var inboundPackets: UInt64 { lock.withLock { receivedPacketCount } }
    /// SCTP notifications seen (mostly association changes). Read-only; the association subscribes
    /// to nothing beyond what RECVRCVINFO needs, so these stay 0 unless usrsctp delivers them anyway.
    public var notificationsSeen: UInt64 { lock.withLock { notificationCount } }
    /// Stream resets seen. A seat that rejects a channel resets its stream rather than answering the
    /// DCEP OPEN, so this is the one counter that distinguishes "we never sent an OPEN" from "the
    /// seat refused it".
    public var streamResetsSeen: UInt64 { lock.withLock { streamResetCount } }
    public var requestedChannelCount: Int { lock.withLock { requestedStreams.count } }
    public var isEstablished: Bool { lock.withLock { associationState == Int32(SCTP_ESTABLISHED) } }

    public var diagnosticState: String {
        lock.withLock {
            guard socket != nil else { return "closed" }
            switch associationState {
            case Int32(SCTP_ESTABLISHED): return "established"
            case Int32(SCTP_COOKIE_WAIT): return "cookie-wait"
            case Int32(SCTP_COOKIE_ECHOED): return "cookie-echoed"
            default: return "connecting"
            }
        }
    }

    public init(onOutboundPacket: @escaping @Sendable (Data) -> Void) throws {
        token = NvstSctpRuntime.register(output: onOutboundPacket)
        guard let socket = usrsctp_socket(AF_CONN, SOCK_STREAM, 132, nil, nil, 0, nil) else {
            NvstSctpRuntime.deregister(token)
            throw AssociationError.socketUnavailable
        }
        self.socket = socket
        do {
            try Self.configure(socket)
            var address = Self.connectionAddress(for: token)
            let bound = withUnsafeMutablePointer(to: &address) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    usrsctp_bind(socket, $0, socklen_t(MemoryLayout<sockaddr_conn>.size))
                }
            }
            guard bound == 0 else { throw AssociationError.bindFailed(Self.lastError) }
            let connected = withUnsafeMutablePointer(to: &address) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) {
                    usrsctp_connect(socket, $0, socklen_t(MemoryLayout<sockaddr_conn>.size))
                }
            }
            guard connected == 0 || errno == EINPROGRESS else {
                throw AssociationError.connectionFailed(Self.lastError)
            }
        } catch {
            close()
            throw error
        }
    }

    deinit { close() }

    public func close() {
        lock.lock()
        defer { lock.unlock() }
        guard let socket else { return }
        self.socket = nil
        usrsctp_close(socket)
        NvstSctpRuntime.deregister(token)
        openStreams.removeAll()
        requestedStreams.removeAll()
        partialMessages.removeAll()
        discardedStreams.removeAll()
    }

    public func openChannels(_ definitions: [NvstSctpChannelDefinition] = NvstSctpChannelProfile.official) throws {
        lock.lock()
        defer { lock.unlock() }
        guard isEstablished else { throw AssociationError.notConnected }
        for definition in definitions where !requestedStreams.contains(definition.streamID) {
            try send(NvstDataChannelProtocol.encodedOpen(for: definition),
                     streamID: definition.streamID, ppid: NvstDataChannelProtocol.ppid)
            requestedStreams.insert(definition.streamID)
        }
    }

    public func isChannelOpen(_ streamID: UInt16) -> Bool {
        lock.withLock { openStreams.contains(streamID) }
    }

    public func send(_ payload: Data, streamID: UInt16, ppid: UInt32) throws {
        lock.lock()
        defer { lock.unlock() }
        guard let socket, isEstablished else { throw AssociationError.notConnected }
        guard !payload.isEmpty else { return }
        var info = Self.sendParameters(streamID: streamID, ppid: ppid)
        let sent = payload.withUnsafeBytes { bytes in
            usrsctp_sendv(socket, bytes.baseAddress, bytes.count, nil, 0, &info,
                          socklen_t(MemoryLayout<sctp_sendv_spa>.size), UInt32(SCTP_SENDV_SPA), 0)
        }
        guard sent == payload.count else { throw AssociationError.sendFailed(Self.lastError) }
    }

    public func feedInbound(_ packet: Data) {
        lock.lock()
        defer { lock.unlock() }
        guard socket != nil, !packet.isEmpty else { return }
        receivedPacketCount &+= 1
        packet.withUnsafeBytes {
            usrsctp_conninput(UnsafeMutableRawPointer(bitPattern: token), $0.baseAddress, $0.count, 0)
        }
    }

    @discardableResult
    public func drainInbound() -> Int {
        lock.lock()
        defer { lock.unlock() }
        guard let socket else { return 0 }
        var delivered = 0
        var buffer = [UInt8](repeating: 0, count: 65_536)
        while true {
            guard self.socket != nil else { return delivered }
            var info = sctp_rcvinfo()
            var infoLength = socklen_t(MemoryLayout<sctp_rcvinfo>.size)
            var infoType: UInt32 = 0
            var flags: Int32 = 0
            let count = buffer.withUnsafeMutableBytes {
                usrsctp_recvv(socket, $0.baseAddress, $0.count, nil, nil, &info, &infoLength, &infoType, &flags)
            }
            guard count > 0 else { return delivered }
            guard flags & MSG_NOTIFICATION == 0, infoType == SCTP_RECVV_RCVINFO else {
                if flags & MSG_NOTIFICATION != 0 {
                    notificationCount &+= 1
                    // `sn_type` is the first two bytes, host order (little-endian here).
                    if buffer.count >= 2, buffer[0] == 0x09, buffer[1] == 0x00 { streamResetCount &+= 1 }
                }
                continue
            }
            guard let message = assemble(streamID: info.rcv_sid, ppid: UInt32(bigEndian: info.rcv_ppid),
                                         payload: Data(buffer[..<count]), isComplete: flags & MSG_EOR != 0) else { continue }
            handleInbound(message)
            delivered += 1
        }
    }

    private func assemble(streamID: UInt16, ppid: UInt32, payload: Data, isComplete: Bool) -> InboundMessage? {
        let isDiscarded = discardedStreams.contains(streamID)
        if isDiscarded, isComplete { discardedStreams.remove(streamID) }
        guard !isDiscarded else { return nil }
        let previous = partialMessages.removeValue(forKey: streamID)
        let combined = (previous?.payload ?? Data()) + payload
        guard (previous?.ppid ?? ppid) == ppid, combined.count <= Self.maximumMessageBytes else {
            if !isComplete { discardedStreams.insert(streamID) }
            return nil
        }
        let message = InboundMessage(streamID: streamID, ppid: ppid, payload: combined)
        guard isComplete else {
            partialMessages[streamID] = message
            return nil
        }
        return message
    }

    private func handleInbound(_ message: InboundMessage) {
        guard message.ppid == NvstDataChannelProtocol.ppid else {
            guard message.ppid == PPID.binary, requestedStreams.contains(message.streamID) else { return }
            markOpen(message.streamID)
            onInboundMessage?(message)
            return
        }
        guard let decoded = NvstDataChannelProtocol.decode(message.payload) else { return }
        switch decoded.type {
        case .open:
            guard let definition = NvstSctpChannelProfile.official.first(where: { $0.streamID == message.streamID }),
                  decoded.open?.label == definition.label,
                  decoded.open?.channelType == NvstDataChannelProtocol.ChannelType.forReliability(definition.reliability),
                  decoded.open?.reliabilityParameter == NvstDataChannelProtocol.reliabilityParameter(for: definition.reliability),
                  (try? send(NvstDataChannelProtocol.encodedAck(), streamID: message.streamID, ppid: message.ppid)) != nil else { return }
            requestedStreams.insert(message.streamID)
            markOpen(message.streamID)
        case .ack:
            guard requestedStreams.contains(message.streamID) else { return }
            markOpen(message.streamID)
        }
    }

    private func markOpen(_ streamID: UInt16) {
        guard openStreams.insert(streamID).inserted else { return }
        onChannelOpened?(streamID)
    }

    private var associationState: Int32? {
        guard let socket else { return nil }
        var status = sctp_status()
        var length = socklen_t(MemoryLayout<sctp_status>.size)
        guard usrsctp_getsockopt(socket, 132, SCTP_STATUS, &status, &length) == 0 else { return nil }
        return status.sstat_state
    }

    static func sendParameters(streamID: UInt16, ppid: UInt32) -> sctp_sendv_spa {
        var info = sctp_sendv_spa()
        info.sendv_flags = UInt32(SCTP_SEND_SNDINFO_VALID | SCTP_SEND_PRINFO_VALID)
        info.sendv_sndinfo.snd_sid = streamID
        info.sendv_sndinfo.snd_ppid = ppid.bigEndian
        guard ppid != NvstDataChannelProtocol.ppid,
              let definition = NvstSctpChannelProfile.official.first(where: { $0.streamID == streamID }) else { return info }
        switch definition.reliability {
        case .reliable: break
        case .partialReliableTimed(let milliseconds):
            info.sendv_prinfo.pr_policy = UInt16(SCTP_PR_SCTP_TTL)
            info.sendv_prinfo.pr_value = milliseconds
        case .partialReliableRetransmits(let retransmits):
            info.sendv_prinfo.pr_policy = UInt16(SCTP_PR_SCTP_RTX)
            info.sendv_prinfo.pr_value = retransmits
        }
        return info
    }

    static func connectionAddress(for token: UInt) -> sockaddr_conn {
        var address = sockaddr_conn()
        address.sconn_len = UInt8(MemoryLayout<sockaddr_conn>.size)
        address.sconn_family = UInt8(AF_CONN)
        address.sconn_port = port.bigEndian
        address.sconn_addr = UnsafeMutableRawPointer(bitPattern: token)
        return address
    }

    private static func configure(_ socket: OpaquePointer) throws {
        guard usrsctp_set_non_blocking(socket, 1) == 0 else { throw AssociationError.configurationFailed(lastError) }
        try setOption(socket, name: SCTP_NODELAY, value: Int32(1))
        try setOption(socket, name: SCTP_RECVRCVINFO, value: Int32(1))
        var streams = sctp_initmsg()
        streams.sinit_num_ostreams = 32
        streams.sinit_max_instreams = 32
        try setOption(socket, name: SCTP_INITMSG, value: streams)
        var path = sctp_paddrparams()
        path.spp_address.ss_family = UInt8(AF_CONN)
        path.spp_address.ss_len = UInt8(MemoryLayout<sockaddr_conn>.size)
        path.spp_flags = UInt32(SPP_PMTUD_DISABLE)
        path.spp_pathmtu = 1200
        try setOption(socket, name: SCTP_PEER_ADDR_PARAMS, value: path)
        var lingerOption = linger(l_onoff: 1, l_linger: 0)
        guard usrsctp_setsockopt(socket, SOL_SOCKET, SO_LINGER, &lingerOption, socklen_t(MemoryLayout<linger>.size)) == 0 else {
            throw AssociationError.configurationFailed(lastError)
        }
    }

    private static func setOption<Value>(_ socket: OpaquePointer, name: Int32, value: Value) throws {
        let status = withUnsafeBytes(of: value) {
            usrsctp_setsockopt(socket, 132, name, $0.baseAddress, socklen_t($0.count))
        }
        guard status == 0 else { throw AssociationError.configurationFailed("option \(name): \(lastError)") }
    }

    private static var lastError: String { String(cString: strerror(errno)) }
}
