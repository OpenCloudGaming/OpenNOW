import Foundation
import Network

/// The RTSP control channel the negotiator drives. Abstracted so the handshake can be
/// exercised against recorded seat responses without a socket.
public protocol NvstRtspControlChannel: Sendable {
    func connect(sessionID: String?) async throws
    @discardableResult
    func request(method: String, uri: String, headers: [(String, String)], body: String) async throws -> NvstRtspResponse
    /// Keeps the control connection alive for the life of the session with periodic RTSP OPTIONS
    /// requests to `uri`, timing each one. A transport that has no such notion can ignore it.
    func startKeepAlive(uri: String, headers: [(String, String)]) async
    /// Stops the keepalive so a TEARDOWN never collides with an in-flight OPTIONS.
    func stopKeepAlive() async
    /// Latest keepalive round trip in milliseconds; -1 when the channel cannot measure one.
    /// A protocol requirement, not just an extension default: the callers hold existentials, and
    /// an extension-only method dispatches statically to the default, silently shadowing the
    /// real implementation.
    func measuredRoundTripMilliseconds() async -> Double
    /// Keepalive ping/pong counters for the diagnostic log.
    func keepAliveSummary() async -> String
    func close() async
}

public extension NvstRtspControlChannel {
    func startKeepAlive(uri: String, headers: [(String, String)]) async {}
    func stopKeepAlive() async {}
    /// Latest keepalive round trip in milliseconds; -1 when the channel cannot measure one.
    func measuredRoundTripMilliseconds() async -> Double { -1 }
    func keepAliveSummary() async -> String { "" }
}

public extension NvstRtspControlChannel {
    @discardableResult
    func request(method: String, uri: String, headers: [(String, String)] = []) async throws -> NvstRtspResponse {
        try await request(method: method, uri: uri, headers: headers, body: "")
    }
}

/// RTSP-over-WSS control connection to the NVST seat (`:322`).
///
/// A stock HTTP client cannot be used: the seat only accepts the exact Poco-shaped upgrade
/// (`GET /rtsp`, `Content-Length: 0`, `x-nv-sessionid`) and answers anything else with HTTP 400.
/// So the TLS socket is driven directly and the WebSocket framing is done here.
public actor NvstRtspConnection: NvstRtspControlChannel {
    public enum ConnectionError: LocalizedError, Equatable, Sendable {
        case connectFailed(String)
        case upgradeRejected(Int)
        case invalidAccept
        case closed
        case timedOut(String)
        case overlappingRequest

        public var errorDescription: String? {
            switch self {
            case .connectFailed(let reason): "NVST RTSPS connect failed: \(reason)"
            case .upgradeRejected(let code): "NVST RTSPS WebSocket upgrade rejected with HTTP \(code)."
            case .invalidAccept: "NVST RTSPS WebSocket upgrade returned an invalid Sec-WebSocket-Accept."
            case .closed: "The NVST RTSPS control connection is closed."
            case .timedOut(let method): "NVST RTSPS \(method) timed out."
            case .overlappingRequest: "Overlapping NVST RTSPS requests are not supported."
            }
        }
    }

    private let target: NvstRtspEndpoints.Target
    private let timeout: Duration
    let logger: (@Sendable (String) -> Void)?
    private var connection: NWConnection?
    private var frameReader = NvstWebSocketFrameReader()
    private var responseBuffer = Data()
    private var pendingResponses: [NvstRtspResponse] = []
    private var responseWaiter: CheckedContinuation<NvstRtspResponse, Error>?
    private var failure: Error?
    private var cseq = 0
    private var keepAliveTask: Task<Void, Never>?
    private var keepAliveGeneration: UInt64 = 0
    private var keepAlivesSent = 0
    private var keepAlivesAnswered = 0
    private var lastKeepAliveStatus = 0

    public init(target: NvstRtspEndpoints.Target,
                timeout: Duration = .seconds(20),
                logger: (@Sendable (String) -> Void)? = nil) {
        self.target = target
        self.timeout = timeout
        self.logger = logger
    }

    // MARK: - Lifecycle

    public func connect(sessionID: String?) async throws {
        // Per-socket, like the connection itself. `sendKeepAlive` and every request guard on it, so
        // a connection reused after a failure answered nothing: the keepalive loop could re-arm and
        // then no-op every two seconds for the rest of the session.
        failure = nil
        let parameters = NWParameters(tls: tlsOptions(), tcp: NWProtocolTCP.Options())
        let endpoint = NWEndpoint.hostPort(host: .init(target.host), port: .init(integerLiteral: target.port))
        let connection = NWConnection(to: endpoint, using: parameters)
        self.connection = connection
        // NWConnection has no connect deadline of its own: an unroutable seat would sit in
        // `.waiting` forever. Cancelling on the deadline is what makes the state handler fire —
        // racing the continuation against a sleep would leave the continuation unresumed.
        let timedOut = OneShotFlag()
        let timeoutTask = Task { [timeout] in
            try? await Task.sleep(for: timeout)
            guard !Task.isCancelled else { return }
            timedOut.set()
            connection.cancel()
        }
        do {
            try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
                let resumed = OneShot(continuation)
                connection.stateUpdateHandler = { state in
                    switch state {
                    case .ready: resumed.resume(.success(()))
                    case .failed(let error): resumed.resume(.failure(ConnectionError.connectFailed(error.localizedDescription)))
                    case .cancelled: resumed.resume(.failure(timedOut.isSet ? ConnectionError.timedOut("CONNECT") : ConnectionError.closed))
                    default: break
                    }
                }
                connection.start(queue: Self.queue)
            }
            timeoutTask.cancel()
        } catch {
            timeoutTask.cancel()
            connection.cancel()
            self.connection = nil
            throw error
        }
        logger?("NVST RTSPS TLS connected to \(target.host):\(target.port)")
        try await performUpgrade(connection: connection, sessionID: sessionID)
        receiveLoop(connection: connection)
    }

    public func close() {
        keepAliveTask?.cancel()
        keepAliveTask = nil
        connection?.cancel()
        connection = nil
        if let responseWaiter {
            self.responseWaiter = nil
            responseWaiter.resume(throwing: ConnectionError.closed)
        }
    }

    // MARK: - Requests

    @discardableResult
    public func request(method: String,
                        uri: String,
                        headers: [(String, String)],
                        body: String) async throws -> NvstRtspResponse {
        guard let connection else { throw ConnectionError.closed }
        if let failure { throw failure }
        guard responseWaiter == nil else { throw ConnectionError.overlappingRequest }
        cseq += 1
        let message = NvstRtspMessage.buildRequest(method: method, uri: uri, headers: headers, body: body, cseq: cseq)
        let sentAt = DispatchTime.now().uptimeNanoseconds
        try await send(connection: connection, data: NvstWebSocketFrame.encodeText(Data(message.utf8)))
        let response = try await nextResponse(method: method)
        noteRequestRoundTrip(method: method, sentAt: sentAt)
        return response
    }

    /// Keeps the RTSPS control connection alive with an RTSP request every `interval`, and times it.
    ///
    /// What the seat tolerates after PLAY, measured live 2026-09-04, one run each:
    /// - a client WebSocket ping: TCP close within 7 ms (`ping #1 sent` 14:52:32.555, FIN .562);
    /// - `OPTIONS` on the handshake URI, no Session header: `400`, then TCP close 7 s later;
    /// - `GET_PARAMETER` with the Session header: answered `551 Option Not Supported` every 2 s for
    ///   the whole session, socket never closed, 4.6–5.3 ms turnaround. That is a keepalive: the
    ///   seat parses it as a well-formed request for this session and declines the method, and
    ///   the connection is what we needed, not the parameter.
    /// An RTSP request is what this channel exists to carry, and its turnaround is a real round trip
    /// on the control path, which nothing else on this transport measures — the seat answers
    /// neither STUN on the video socket nor ICE checks on the bundle. Any RTSP answer counts as
    /// the seat alive; only a dead socket ends the loop.
    public static let keepAliveMethod = "GET_PARAMETER"

    public func startKeepAlive(uri: String, headers: [(String, String)]) {
        startKeepAlive(uri: uri, headers: headers, interval: Self.keepAliveInterval)
    }

    public func startKeepAlive(uri: String, headers: [(String, String)], interval: TimeInterval) {
        guard keepAliveTask == nil else { return }
        keepAliveGeneration &+= 1
        let generation = keepAliveGeneration
        keepAliveTask = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(interval))
                guard !Task.isCancelled, let self else { return }
                // A non-200 is logged and the cadence continues; only a dead socket ends the loop.
                _ = await self.sendKeepAlive(uri: uri, headers: headers)
                if await self.failure != nil { break }
            }
            await self?.clearFinishedKeepAlive(generation: generation)
        }
    }

    /// The `keepAliveTask == nil` check above is what makes a second `startKeepAlive` a no-op, so a
    /// loop that ends on its own — a dead socket, not a cancellation — has to release the slot.
    /// Left held, a connection that recovered its socket ran on with no keepalive at all.
    private func clearFinishedKeepAlive(generation: UInt64) {
        // Only the loop that still owns the slot may release it: a stop-then-start in the meantime
        // has already installed a newer task, and clearing that one would let a third loop start
        // alongside it.
        guard generation == keepAliveGeneration else { return }
        keepAliveTask = nil
    }

    public func stopKeepAlive() {
        keepAliveTask?.cancel()
        keepAliveTask = nil
    }

    /// The vendor's control-channel cadence: a message every couple of seconds.
    public static let keepAliveInterval: TimeInterval = 2

    @discardableResult
    func sendKeepAlive(uri: String, headers: [(String, String)]) async -> Bool {
        guard connection != nil, failure == nil, responseWaiter == nil else { return false }
        keepAlivesSent += 1
        let sentAt = DispatchTime.now().uptimeNanoseconds
        do {
            let response = try await request(method: Self.keepAliveMethod, uri: uri, headers: headers)
            let now = DispatchTime.now().uptimeNanoseconds
            // Any RTSP answer is a round trip and proof the seat still holds the session; the live
            // seat says 551 to this method and keeps the socket, which is all the keepalive needs.
            if now > sentAt { lastRoundTripMilliseconds = Double(now - sentAt) / 1_000_000 }
            keepAlivesAnswered += 1
            // The first few are logged so a socket the seat closes can be timed against them, and
            // any change in the seat's answer after that.
            if keepAlivesSent <= 3 || response.statusCode != lastKeepAliveStatus {
                logger?("NVST RTSPS keepalive \(Self.keepAliveMethod) #\(keepAlivesSent) -> \(response.statusCode) rtt=\(String(format: "%.1f", lastRoundTripMilliseconds))ms")
            }
            lastKeepAliveStatus = response.statusCode
            return true
        } catch {
            logger?("NVST RTSPS keepalive \(Self.keepAliveMethod) #\(keepAlivesSent) failed: \(error.localizedDescription)")
            return false
        }
    }

    public var keepAliveRequestsSent: Int { keepAlivesSent }

    /// Round trip of the most recent keepalive OPTIONS on this control connection, in
    /// milliseconds, or -1 before the first answer.
    public var roundTripMilliseconds: Double { lastRoundTripMilliseconds }
    /// The live keepalive round trip when the socket survives, else the handshake's fastest turnaround.
    public func measuredRoundTripMilliseconds() -> Double {
        lastRoundTripMilliseconds >= 0 ? lastRoundTripMilliseconds : handshakeRoundTripMilliseconds
    }
    /// Keepalive health for the diagnostic log: whether pings go out and pongs come back at all.
    public func keepAliveSummary() -> String {
        "keepalives=\(keepAlivesSent)/\(keepAlivesAnswered) seatPingsAnswered=\(pongsAnswered) pongsSeen=\(frameReader.pongsSeen)"
            + " failed=\(failure.map { _ in 1 } ?? 0) rtt=\(String(format: "%.1f", lastRoundTripMilliseconds))"
            + " handshakeRtt=\(String(format: "%.1f", handshakeRoundTripMilliseconds))"
            + (failure.map { " failure=\($0.localizedDescription)" } ?? "")
    }
    var lastRoundTripMilliseconds: Double = -1
    /// Fastest RTSP request/response turnaround seen during the handshake, in milliseconds, or -1.
    ///
    /// The keepalive OPTIONS above measures the same thing live; this is the value from before the
    /// first one answers, and the fallback should the seat ever stop answering.
    var handshakeRoundTripMilliseconds: Double = -1

    private func noteRequestRoundTrip(method: String, sentAt: UInt64) {
        // TEARDOWN happens during teardown; its timing describes nothing the HUD should show.
        guard method != "TEARDOWN" else { return }
        let now = DispatchTime.now().uptimeNanoseconds
        guard now > sentAt else { return }
        let milliseconds = Double(now - sentAt) / 1_000_000
        if handshakeRoundTripMilliseconds < 0 || milliseconds < handshakeRoundTripMilliseconds {
            handshakeRoundTripMilliseconds = milliseconds
        }
    }

    private func nextResponse(method: String) async throws -> NvstRtspResponse {
        if !pendingResponses.isEmpty { return pendingResponses.removeFirst() }
        if let failure { throw failure }
        let deadline = timeout
        return try await withThrowingTaskGroup(of: NvstRtspResponse.self) { group in
            group.addTask { [weak self] in
                guard let self else { throw ConnectionError.closed }
                return try await self.awaitResponse()
            }
            group.addTask { [weak self] in
                try await Task.sleep(for: deadline)
                // Resume the waiting continuation too: leaving it pending would deadlock the
                // group, which cannot return until every child task finishes.
                await self?.failPending(ConnectionError.timedOut(method))
                throw ConnectionError.timedOut(method)
            }
            guard let response = try await group.next() else {
                throw ConnectionError.timedOut(method)
            }
            group.cancelAll()
            return response
        }
    }

    /// Fails a pending request without tearing down the connection.
    private func failPending(_ error: Error) {
        guard let waiter = responseWaiter else { return }
        responseWaiter = nil
        waiter.resume(throwing: error)
    }

    private func awaitResponse() async throws -> NvstRtspResponse {
        try await withCheckedThrowingContinuation { continuation in
            if !pendingResponses.isEmpty {
                continuation.resume(returning: pendingResponses.removeFirst())
                return
            }
            if let failure {
                continuation.resume(throwing: failure)
                return
            }
            responseWaiter = continuation
        }
    }

    // MARK: - Internals

    static let queue = DispatchQueue(label: "com.opennow.nvst.rtsp")

    private func tlsOptions() -> NWProtocolTLS.Options {
        let options = NWProtocolTLS.Options()
        sec_protocol_options_set_tls_server_name(options.securityProtocolOptions, target.host)
        sec_protocol_options_set_min_tls_protocol_version(options.securityProtocolOptions, .TLSv12)
        return options
    }

    private func performUpgrade(connection: NWConnection, sessionID: String?) async throws {
        let key = NvstWebSocketUpgrade.generateKey()
        let request = NvstWebSocketUpgrade.request(host: target.host, port: target.port, secWebSocketKey: key, sessionID: sessionID)
        try await send(connection: connection, data: Data(request.utf8))
        var buffer = Data()
        while true {
            let chunk = try await receive(connection: connection)
            buffer.append(chunk)
            guard let response = NvstWebSocketUpgrade.parseResponse(buffer) else {
                guard buffer.count <= 16_384 else { throw ConnectionError.upgradeRejected(0) }
                continue
            }
            guard response.statusCode == 101 else { throw ConnectionError.upgradeRejected(response.statusCode) }
            guard response.headers["sec-websocket-accept"] == NvstWebSocketUpgrade.expectedAccept(for: key) else {
                throw ConnectionError.invalidAccept
            }
            if !response.leftover.isEmpty {
                ingest(response.leftover)
            }
            logger?("NVST RTSPS WebSocket upgraded (GET \(NvstWebSocketUpgrade.requestTarget))")
            return
        }
    }

    private func receiveLoop(connection: NWConnection) {
        connection.receive(minimumIncompleteLength: 1, maximumLength: 65_536) { [weak self] content, _, isComplete, error in
            guard let self else { return }
            Task {
                if let content, !content.isEmpty {
                    await self.ingest(content)
                }
                if let error {
                    await self.fail(ConnectionError.connectFailed(error.localizedDescription))
                    return
                }
                if isComplete {
                    await self.fail(ConnectionError.closed)
                    return
                }
                await self.resumeReceiveLoop()
            }
        }
    }

    private func resumeReceiveLoop() {
        guard let connection else { return }
        receiveLoop(connection: connection)
    }

    private func ingest(_ chunk: Data) {
        do {
            for payload in try frameReader.push(chunk) {
                responseBuffer.append(payload)
                while let response = try NvstRtspMessage.extractResponse(from: &responseBuffer) {
                    deliver(response)
                }
            }
            // The seat pings this connection and closes it when no pong comes back — Poco's
            // client answered these automatically, so never answering was invisible until the
            // keepalive tried to use the socket the seat had already abandoned.
            let pings = frameReader.takePings()
            if !pings.isEmpty, let connection {
                pongsAnswered += pings.count
                for ping in pings {
                    Task { try? await self.send(connection: connection, data: NvstWebSocketFrame.encodePong(payload: ping)) }
                }
            }
        } catch {
            fail(error)
        }
    }
    private var pongsAnswered = 0

    private func deliver(_ response: NvstRtspResponse) {
        if let responseWaiter {
            self.responseWaiter = nil
            responseWaiter.resume(returning: response)
        } else {
            pendingResponses.append(response)
        }
    }

    private func fail(_ error: Error) {
        // Logged because the HUD's latency fallback chain depends on knowing whether this socket
        // is alive; a silent failure here read as "no RTT source at all" for weeks.
        if failure == nil {
            logger?("NVST RTSPS control connection failed after \(cseq) requests, \(keepAlivesSent) keepalives: \(error.localizedDescription)")
        }
        failure = error
        if let responseWaiter {
            self.responseWaiter = nil
            responseWaiter.resume(throwing: error)
        }
    }

    private func send(connection: NWConnection, data: Data) async throws {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            let resumed = OneShot(continuation)
            connection.send(content: data, completion: .contentProcessed { error in
                if let error {
                    resumed.resume(.failure(ConnectionError.connectFailed(error.localizedDescription)))
                } else {
                    resumed.resume(.success(()))
                }
            })
        }
    }

    private func receive(connection: NWConnection) async throws -> Data {
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Data, Error>) in
            let resumed = OneShot(continuation)
            connection.receive(minimumIncompleteLength: 1, maximumLength: 65_536) { content, _, isComplete, error in
                if let error {
                    resumed.resume(.failure(ConnectionError.connectFailed(error.localizedDescription)))
                    return
                }
                if let content, !content.isEmpty {
                    resumed.resume(.success(content))
                    return
                }
                resumed.resume(.failure(isComplete ? ConnectionError.closed : ConnectionError.connectFailed("empty read")))
            }
        }
    }
}

/// A set-once flag shared with a timeout task.
private final class OneShotFlag: @unchecked Sendable {
    let lock = NSLock()
    private var value = false

    func set() { lock.lock(); value = true; lock.unlock() }
    var isSet: Bool { lock.lock(); defer { lock.unlock() }; return value }
}

/// Guards a `CheckedContinuation` that Network.framework callbacks may invoke more than once.
private final class OneShot<Value: Sendable>: @unchecked Sendable {
    let lock = NSLock()
    private var continuation: CheckedContinuation<Value, Error>?

    init(_ continuation: CheckedContinuation<Value, Error>) {
        self.continuation = continuation
    }

    func resume(_ result: Result<Value, Error>) {
        lock.lock()
        let pending = continuation
        continuation = nil
        lock.unlock()
        guard let pending else { return }
        pending.resume(with: result)
    }
}
