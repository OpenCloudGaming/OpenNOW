import Darwin
import Foundation

/// A DTLS association carried on a UDP socket, driven continuously rather than once.
///
/// The seat cannot answer DTLS until ANNOUNCE has reached it — that is where it learns this
/// endpoint's bundle port, fingerprint and ICE credentials. The handshake therefore cannot be
/// completed inside the bring-up that *produces* those values: it runs here, and finishes whenever
/// the seat starts talking back. Waiting for it synchronously is a deadlock with a ten-second
/// timeout, which is exactly how it first failed against a live seat.
///
/// The socket carries two planes. SCTP rides *inside* DTLS, so its packets are written through the
/// record layer. SRTP audio runs *alongside* DTLS on the same port, already protected, so it is sent
/// raw and classified on the way in. Getting that backwards would encrypt an already-encrypted
/// stream, or hand ciphertext to the record layer.
///
/// Two drivers exist over the same primitives. `start()` is the production one: a receive loop on
/// its own queue. `completeHandshake(timeout:)` and `receive(timeout:)` drive the same state machine
/// inline, which is how two endpoints are tested against each other on loopback sockets. A transport
/// must use one or the other, never both.
public final class NvstDtlsTransport: @unchecked Sendable {
    public enum TransportError: LocalizedError, Equatable {
        case socketUnavailable(String)
        case peerUnresolvable(String)
        case sendFailed(String)
        case receiveFailed(String)
        case handshakeFailed(String)
        case handshakeTimedOut

        public var errorDescription: String? {
            switch self {
            case .socketUnavailable(let reason): "The DTLS bundle socket is unusable: \(reason)"
            case .peerUnresolvable(let address): "The DTLS peer address \(address) is not a valid IPv4 literal."
            case .sendFailed(let reason): "The DTLS bundle could not send a datagram: \(reason)"
            case .receiveFailed(let reason): "The DTLS bundle could not receive a datagram: \(reason)"
            case .handshakeFailed(let reason): "The DTLS handshake failed: \(reason)"
            case .handshakeTimedOut: "The DTLS handshake did not complete before its deadline."
            }
        }
    }

    public static func profile(forOpenSSLName name: String) -> NVSTSrtpProfile? {
        switch name.uppercased() {
        case "SRTP_AEAD_AES_256_GCM": return .aeadAes256Gcm
        case "SRTP_AEAD_AES_128_GCM": return .aeadAes128Gcm
        case "SRTP_AES128_CM_SHA1_80": return .aesCm128HmacSha1_80
        case "SRTP_AES128_CM_SHA1_32": return .aesCm128HmacSha1_32
        default: return nil
        }
    }

    /// The bundle socket's STUN identity, without which the seat's front end has no route to its
    /// bundle service and never answers DTLS.
    public struct NattIdentity: Sendable {
        public let remoteUfrag: String
        public let localUfrag: String
        public let integrityKey: Data

        public init(remoteUfrag: String, localUfrag: String, integrityKey: Data) {
            self.remoteUfrag = remoteUfrag
            self.localUfrag = localUfrag
            self.integrityKey = integrityKey
        }
    }

    public var natt: NattIdentity?

    public let localAddress: String?
    public let localPort: UInt16

    /// Decrypted DTLS application data — the SCTP association.
    public var onApplicationData: (@Sendable (Data) -> Void)?
    /// SRTP audio that arrived alongside DTLS, still protected.
    public var onSecureAudio: (@Sendable (Data) -> Void)?
    public var onHandshakeComplete: (@Sendable () -> Void)?
    public var onHandshakeFailure: (@Sendable (Error) -> Void)?

    private let handshake: NvstDtlsHandshake
    private let descriptor: Int32
    private let peer: sockaddr_in
    private let lock = NSLock()
    private let handshakeLock = NSLock()
    private let queue = DispatchQueue(label: "io.opencg.opennow.nvst.bundle-socket")
    private var isClosed = false
    private var hasReportedCompletion = false
    private var hasFailed = false
    private var nextPunchAt = DispatchTime.now()
    private var _applicationDatagrams: UInt64 = 0

    /// DTLS application records delivered after the handshake — every one of these is an SCTP
    /// packet. Zero means the association is not exchanging anything, whatever its state says.
    public var applicationDatagrams: UInt64 {
        lock.lock(); defer { lock.unlock() }
        return _applicationDatagrams
    }

    /// The official client sends a Binding Request, then the ClientHello about 37 ms later; the
    /// seat's front end needs the punch to have established the route first. Before the handshake
    /// completes the ping is the ICE cadence, after it the slower keepalive.
    private static let punchInterval: TimeInterval = 0.027
    private static let keepAliveInterval: TimeInterval = 0.100
    private static let clientHelloDelay: TimeInterval = 0.035
    private static let punchBurst = 3

    public var isHandshakeComplete: Bool { handshakeLock.withLock { handshake.isConnected } }

    /// Takes ownership of `descriptor`, which the caller must not close afterwards.
    public init(handshake: NvstDtlsHandshake,
                descriptor: Int32,
                localAddress: String?,
                localPort: UInt16,
                peerAddress: String,
                peerPort: UInt16) throws {
        guard descriptor >= 0 else { throw TransportError.socketUnavailable("no descriptor") }
        var address = sockaddr_in()
        address.sin_family = sa_family_t(AF_INET)
        address.sin_port = peerPort.bigEndian
        guard inet_pton(AF_INET, peerAddress, &address.sin_addr) == 1 else {
            Darwin.close(descriptor)
            throw TransportError.peerUnresolvable(peerAddress)
        }
        self.handshake = handshake
        self.descriptor = descriptor
        self.peer = address
        self.localAddress = localAddress
        self.localPort = localPort
    }

    deinit { close() }

    /// Begins reading the socket. The handshake is driven from here, so it can complete the moment
    /// the seat starts answering — which is only after ANNOUNCE has gone out.
    public func start() {
        queue.async { [weak self] in
            guard let self else { return }
            sendPunch()
            // The route the punch opens is what lets the ClientHello be answered, so the handshake
            // is deliberately held back for the interval the official client uses.
            queue.asyncAfter(deadline: .now() + Self.clientHelloDelay) { [weak self] in
                guard let self, !isClosed else { return }
                do {
                    try advanceHandshake(received: nil)
                    runReceiveLoop()
                } catch {
                    reportFailure(error)
                }
            }
        }
    }

    /// The keying material, valid once the handshake has completed.
    public func exportedKeys() throws -> (keys: NvstBundleSrtpKeys, profile: NVSTSrtpProfile) {
        handshakeLock.lock()
        defer { handshakeLock.unlock() }
        guard handshake.isConnected else { throw TransportError.handshakeFailed("not connected") }
        guard let name = handshake.selectedSrtpProfileName else {
            throw TransportError.handshakeFailed("no SRTP profile was selected")
        }
        guard let profile = Self.profile(forOpenSSLName: name) else {
            throw TransportError.handshakeFailed("unrecognised SRTP profile \(name)")
        }
        let exported = try handshake.exportKeyingMaterial(length: NvstBundleSrtpKeys.exportedLength(for: profile))
        return (try NvstBundleSrtpKeys.split(exported, profile: profile), profile)
    }

    /// Writes through the record layer: SCTP rides inside DTLS.
    public func sendEncrypted(_ payload: Data) throws {
        guard let datagram = try handshakeLock.withLock({ try handshake.writeApplicationData(payload) }) else { return }
        try sendDatagram(datagram)
    }

    /// Writes straight to the socket: SRTP audio is already protected and runs beside DTLS.
    public func sendRaw(_ datagram: Data) throws {
        try sendDatagram(datagram)
    }

    public func close() {
        lock.lock()
        let wasClosed = isClosed
        isClosed = true
        lock.unlock()
        guard !wasClosed else { return }
        Darwin.close(descriptor)
    }

    // MARK: - Synchronous driver (loopback endpoints, and the handshake's own tests)

    /// Drives the handshake to completion, returning the SRTP keys both ends must agree on.
    public func completeHandshake(timeout: TimeInterval) throws -> NvstBundleSrtpKeys {
        let deadline = Date().addingTimeInterval(timeout)
        try advanceHandshake(received: nil)
        while !isHandshakeComplete {
            guard Date() < deadline else { throw TransportError.handshakeTimedOut }
            if let flight = handshakeLock.withLock({ handshake.retransmitWhenDue() }) { try sendDatagram(flight) }
            if let datagram = try receiveDatagram(timeout: 0.05) {
                try advanceHandshake(received: datagram)
            } else {
                try advanceHandshake(received: nil)
            }
        }
        return try exportedKeys().keys
    }

    /// Reads one application datagram, feeding the record layer whatever arrives first.
    public func receive(timeout: TimeInterval) throws -> Data? {
        let deadline = Date().addingTimeInterval(timeout)
        while Date() < deadline {
            if let datagram = try receiveDatagram(timeout: 0.05) {
                try deliver(datagram)
            } else if !isHandshakeComplete {
                try advanceHandshake(received: nil)
            }
            if isHandshakeComplete, let payload = try drainApplicationData() { return payload }
        }
        return nil
    }

    // MARK: - Receive loop

    private func runReceiveLoop() {
        while !isClosed {
            if failure() { return }
            // Retransmit on the DTLS clock even when nothing arrives: a lost flight is the common
            // case on a fresh NAT mapping and the timer is the only thing that recovers it.
            if !isHandshakeComplete, let flight = handshakeLock.withLock({ handshake.retransmitWhenDue() }) {
                try? sendDatagram(flight)
            }
            punchWhenDue()
            do {
                if let datagram = try receiveDatagram(timeout: 0.02) {
                    try deliver(datagram)
                } else if !isHandshakeComplete {
                    try advanceHandshake(received: nil)
                }
                if isHandshakeComplete { try drainApplicationData() }
            } catch {
                reportFailure(error)
                return
            }
        }
    }

    /// Classifies one datagram and routes it to the plane that owns it.
    private func deliver(_ datagram: Data) throws {
        switch NvstBundleDatagramDemux.classify(datagram) {
        case .dtls:
            guard !isHandshakeComplete else {
                try handshakeLock.withLock { try handshake.feedDatagram(datagram) }
                return
            }
            try advanceHandshake(received: datagram)
            guard isHandshakeComplete else { return }
            reportCompletionOnce()
        case .srtp(let audio):
            onSecureAudio?(audio)
        case .stun:
            // The seat never pings back, so there is nothing to answer; the classification exists
            // so this is never handed to the record layer.
            break
        case .unknown:
            break
        }
    }

    /// Sends the burst when its cadence is due. Runs on the receive queue, so it shares the socket
    /// with the handshake without a separate timer.
    private func punchWhenDue() {
        guard natt != nil else { return }
        let now = DispatchTime.now()
        guard now >= nextPunchAt else { return }
        sendPunch()
        nextPunchAt = now + DispatchTimeInterval.nanoseconds(Int((isHandshakeComplete ? Self.keepAliveInterval : Self.punchInterval) * 1_000_000_000))
    }

    private func sendPunch() {
        guard let natt else { return }
        for _ in 0..<Self.punchBurst {
            guard let request = NvstBundleNattPunch.request(remoteUfrag: natt.remoteUfrag,
                                                            localUfrag: natt.localUfrag,
                                                            remotePassword: natt.integrityKey,
                                                            transactionID: NvstBundleNattPunch.transactionID()) else { continue }
            try? sendDatagram(request)
        }
    }

    /// Drives the handshake and sends every flight it produces.
    ///
    /// `hasPendingInbound` is the whole reason this is a loop: a datagram carries a flight, OpenSSL
    /// consumes one record per call, and calling in once leaves the handshake one record short of
    /// finishing — a stall with nothing in the logs to explain it.
    private func advanceHandshake(received datagram: Data?) throws {
        handshakeLock.lock()
        defer { handshakeLock.unlock() }
        var flight = try handshake.handshakeStep(received: datagram)
        var iterations = 0
        while true {
            if let flight { try sendDatagram(flight) }
            guard !handshake.isConnected, handshake.hasPendingInbound, iterations < 64 else { return }
            iterations += 1
            flight = try handshake.handshakeStep(received: nil)
        }
    }

    /// Reads every application record available, forwarding each to `onApplicationData` and
    /// returning the first so the synchronous driver has a payload to hand back.
    @discardableResult
    private func drainApplicationData() throws -> Data? {
        var first: Data?
        while let payload = try handshakeLock.withLock({ try handshake.readApplicationData() }) {
            guard !payload.isEmpty else { continue }
            lock.lock()
            _applicationDatagrams &+= 1
            lock.unlock()
            if first == nil { first = payload }
            onApplicationData?(payload)
        }
        return first
    }

    private func failure() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        return hasFailed
    }

    private func reportCompletionOnce() {
        lock.lock()
        let shouldReport = !hasReportedCompletion
        if shouldReport { hasReportedCompletion = true }
        lock.unlock()
        guard shouldReport else { return }
        onHandshakeComplete?()
    }

    private func reportFailure(_ error: Error) {
        lock.lock()
        let firstFailure = !hasFailed
        hasFailed = true
        lock.unlock()
        guard firstFailure else { return }
        onHandshakeFailure?(error)
    }

    // MARK: - Socket

    private func sendDatagram(_ datagram: Data) throws {
        let sent = datagram.withUnsafeBytes { bytes in
            withUnsafePointer(to: peer) { peerPointer in
                peerPointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { address in
                    sendto(descriptor, bytes.baseAddress, bytes.count, 0, address, socklen_t(MemoryLayout<sockaddr_in>.size))
                }
            }
        }
        guard sent == datagram.count else {
            throw TransportError.sendFailed(String(cString: strerror(errno)))
        }
    }

    private func receiveDatagram(timeout: TimeInterval) throws -> Data? {
        var wait = timeval(tv_sec: Int(timeout), tv_usec: Int32((timeout - Double(Int(timeout))) * 1_000_000))
        setsockopt(descriptor, SOL_SOCKET, SO_RCVTIMEO, &wait, socklen_t(MemoryLayout<timeval>.size))
        var buffer = [UInt8](repeating: 0, count: 4096)
        let read = buffer.withUnsafeMutableBytes { bytes in
            recv(descriptor, bytes.baseAddress, bytes.count, 0)
        }
        if read > 0 { return Data(buffer[0..<read]) }
        if read == 0 { return nil }
        guard errno == EAGAIN || errno == EWOULDBLOCK || errno == EINTR else {
            throw TransportError.receiveFailed(String(cString: strerror(errno)))
        }
        return nil
    }
}
