import Darwin
import Foundation

/// STUN-only ICE keepalive on the NVST ICE/DTLS bundle socket.
///
/// The official client brings a full ICE/DTLS/SCTP bundle up on this socket after ANNOUNCE and
/// before PLAY. This probe does the ICE half of that — the authenticated Binding Request bursts
/// and the Binding Success answers — without DTLS. It cannot make the seat's data channels work,
/// but it keeps the NAT mapping alive and, decisively, it reports what the seat sends back: STUN
/// answers prove reachability, and an inbound DTLS ClientHello proves the seat is waiting on a
/// DTLS peer rather than ignoring us.
public final class NvstBundleIceProbe: @unchecked Sendable {
    public enum ProbeError: LocalizedError, Equatable, Sendable {
        case noSocket
        case missingCredentials

        public var errorDescription: String? {
            switch self {
            case .noSocket: "NVST bundle probe has no socket to read."
            case .missingCredentials: "NVST bundle probe needs the version-6 ICE credentials."
            }
        }
    }

    /// Official first burst is three ICE Binding Requests per tick, then the NATT ping-string
    /// keepalive ("Old server only supports PING").
    public static let iceBurstCount = 3
    public static let nattUsername = "PING"

    let handoff: NVSTVideoHandoff
    private let credentials: NVSTHandoffIceCredentials
    let logger: (@Sendable (String) -> Void)?
    let queue = DispatchQueue(label: "com.opennow.nvst.bundle")
    let lock = NSLock()
    private var descriptor: Int32
    private var readSource: DispatchSourceRead?
    private var punchTimer: DispatchSourceTimer?
    private var reportTimer: DispatchSourceTimer?
    private var counters = NvstInboundCounters()

    public init(handoff: NVSTVideoHandoff, descriptor: Int32, logger: (@Sendable (String) -> Void)? = nil) throws {
        guard descriptor >= 0 else { throw ProbeError.noSocket }
        guard let credentials = handoff.iceCredentials else { throw ProbeError.missingCredentials }
        self.handoff = handoff
        self.credentials = credentials
        self.descriptor = descriptor
        self.logger = logger
    }

    public var snapshot: NvstInboundCounters { lock.lock(); defer { lock.unlock() }; return counters }

    public func start() {
        // A second start would leak all three sources and leave the first set running.
        guard readSource == nil else { return }
        var flags = fcntl(descriptor, F_GETFL, 0)
        flags |= O_NONBLOCK
        _ = fcntl(descriptor, F_SETFL, flags)

        let source = DispatchSource.makeReadSource(fileDescriptor: descriptor, queue: queue)
        source.setEventHandler { [weak self] in self?.drain() }
        // The close belongs here, not in `stop()`: `drain` reads the descriptor on `queue`, and
        // closing it from the caller's thread could pull it out from under a `recvfrom` — onto a
        // number the kernel has already handed to something else. Same shape as
        // `NvstMjolnirReceiver`, which is the reference for this.
        source.setCancelHandler { [weak self] in
            guard let self else { return }
            lock.lock()
            let closing = descriptor
            descriptor = -1
            lock.unlock()
            if closing >= 0 { close(closing) }
        }
        source.resume()
        readSource = source

        let punch = DispatchSource.makeTimerSource(queue: queue)
        punch.schedule(deadline: .now(), repeating: NvstMjolnirReceiver.pingIntervalBeforeConnection, leeway: .milliseconds(5))
        punch.setEventHandler { [weak self] in self?.sendPunch() }
        punch.resume()
        punchTimer = punch

        let report = DispatchSource.makeTimerSource(queue: queue)
        report.schedule(deadline: .now() + 2, repeating: 2, leeway: .milliseconds(200))
        report.setEventHandler { [weak self] in
            guard let self else { return }
            logger?("NVST bundle \(snapshot.summary)")
        }
        report.resume()
        reportTimer = report
    }

    public func stop() {
        punchTimer?.cancel()
        punchTimer = nil
        reportTimer?.cancel()
        reportTimer = nil
        // Cancelling runs the cancel handler, which is what closes the socket.
        readSource?.cancel()
        readSource = nil
    }

    // MARK: - Internals

    private func drain() {
        var buffer = [UInt8](repeating: 0, count: 65_536)
        while true {
            var peer = sockaddr_in()
            var peerLength = socklen_t(MemoryLayout<sockaddr_in>.size)
            let received = withUnsafeMutablePointer(to: &peer) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                    recvfrom(descriptor, &buffer, buffer.count, 0, sockaddrPointer, &peerLength)
                }
            }
            if received <= 0 { return }
            let datagram = Data(buffer[0..<received])
            lock.lock()
            counters.record(datagram: datagram)
            lock.unlock()
            answerIfBindingRequest(datagram, from: peer)
        }
    }

    /// A relayed inbound Binding Request is answered with a Binding Success authenticated by our
    /// own ICE password — the version-6 "PONG".
    private func answerIfBindingRequest(_ datagram: Data, from peer: sockaddr_in) {
        guard let transactionID = NvstStunHolePunch.bindingRequestTransactionID(datagram) else { return }
        guard let response = NvstStunHolePunch.buildBindingSuccess(
            transactionID: transactionID,
            mappedHost: NvstMjolnirReceiver.dottedQuad(peer.sin_addr.s_addr),
            mappedPort: UInt16(bigEndian: peer.sin_port),
            integrityKey: Data(credentials.localPassword.utf8)
        ) else { return }
        send(response)
        lock.lock()
        counters.responsesSent += 1
        lock.unlock()
    }

    private func sendPunch() {
        var sent = 0
        let remotePassword = Data(credentials.remotePassword.utf8)
        for _ in 0..<Self.iceBurstCount {
            guard let request = NvstStunHolePunch.buildBindingRequest(
                transactionID: Self.transactionID(),
                username: "\(credentials.remoteUsernameFragment):\(credentials.localUsernameFragment)",
                integrityKey: remotePassword
            ) else { continue }
            send(request)
            sent += 1
        }
        // The NATT keepalive rides the same socket with the ping-string identity.
        if let natt = NvstStunHolePunch.buildBindingRequest(
            transactionID: Self.transactionID(),
            username: "\(Self.nattUsername):\(credentials.localUsernameFragment)",
            integrityKey: remotePassword
        ) {
            send(natt)
            sent += 1
        }
        lock.lock()
        counters.punchesSent += sent
        lock.unlock()
    }

    static func transactionID() -> Data {
        var identifier = Data(count: 12)
        for index in 0..<12 { identifier[index] = UInt8.random(in: 0...255) }
        return identifier
    }

    private func send(_ data: Data) {
        guard descriptor >= 0 else { return }
        guard let address = NvstMjolnirReceiver.inetAddr(handoff.videoPeerIP) else { return }
        var destination = sockaddr_in()
        destination.sin_family = sa_family_t(AF_INET)
        destination.sin_port = handoff.videoPeerPort.bigEndian
        destination.sin_addr.s_addr = address
        data.withUnsafeBytes { bytes in
            withUnsafePointer(to: &destination) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                    _ = sendto(descriptor, bytes.baseAddress, bytes.count, 0, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_in>.size))
                }
            }
        }
    }
}
