import Darwin
import Foundation

/// The raw-SRTP "Mjolnir" video socket.
///
/// In the official two-socket cloud model video never touches the ICE/DTLS bundle: it arrives
/// as raw SRTP on a dedicated UDP socket that is kept open by an authenticated STUN (NATT)
/// keepalive. This type owns that socket — bind, keepalive, inbound STUN answers, and the
/// receive loop — and hands whole access units to `onAccessUnit`.
public final class NvstMjolnirReceiver: @unchecked Sendable {
    public enum ReceiverError: LocalizedError, Equatable, Sendable {
        case socket(String)

        public var errorDescription: String? {
            switch self {
            case .socket(let reason): "Mjolnir socket error: \(reason)"
            }
        }
    }

    /// Captured official cadence: five punches ~27 ms apart, then a 100 ms keepalive, and the
    /// socket stops punching altogether ~7 s in (69 packets over 6.7 s of a 25 s session).
    public static let pingIntervalBeforeConnection: TimeInterval = 0.027
    public static let pingIntervalAfterConnection: TimeInterval = 0.100
    public static let punchBurstCount = 5
    public static let punchDuration: TimeInterval = 7

    let handoff: NVSTVideoHandoff
    let receiver: NvstVideoReceiver
    /// Sends SRTCP receiver reports on this socket. Off when the bundle's SCTP feedback channel
    /// carries them instead.
    private let sendsReceiverReports: Bool
    /// User-interactive: this loop is the one piece of work that must never be descheduled — a
    /// late drain overflows the socket buffer and the loss is indistinguishable from the network's.
    let queue = DispatchQueue(label: "com.opennow.nvst.mjolnir", qos: .userInteractive)
    /// Feedback runs on its own queue. Sharing the receive queue meant the drain loop starved it:
    /// a 1 s repeating timer fired once in 30 s, so the sender never heard a receiver report and
    /// backed its bitrate off.
    private let startedAtNanoseconds = DispatchTime.now().uptimeNanoseconds
    private let feedbackQueue = DispatchQueue(label: "com.opennow.nvst.mjolnir.feedback")
    private let callbackLock = NSLock()
    private var socketDescriptor: Int32 = -1
    /// Scratch space for `recvfrom`, hoisted out of `drainSocket`: the read source fires thousands
    /// of times per session, and allocating 64 KB on every wake-up was ~17 MB/s of churn on the
    /// user-interactive queue. The receive queue is serial, so a single reused buffer is safe.
    private var drainBuffer = [UInt8](repeating: 0, count: 65_536)
    private var readSource: DispatchSourceRead?
    private var pingTimer: DispatchSourceTimer?
    private var reportTimer: DispatchSourceTimer?
    private var connected = false
    private var punchStartedAt: Date?
    private var punchCount = 0
    private var isStopped = false
    private var isStopRequested: Bool {
        sendLock.lock()
        defer { sendLock.unlock() }
        return isStopped
    }
    private let adoptedDescriptor: Int32
    private let counterLock = NSLock()
    private var counters = NvstInboundCounters()

    public var onAccessUnit: (@Sendable (NvstAccessUnit) -> Void)?
    /// The reference chain broke. The argument is the frame the damage landed in when the receiver
    /// knows it (a hole inside a frame), nil for a finalized RTP loss whose frame is unknown.
    public var onRecoveryNeeded: (@Sendable (UInt32?) -> Void)?
    public var onDrop: (@Sendable (NvstReceiveDrop) -> Void)?
    /// Diagnostics that would otherwise be invisible, such as a feedback timer producing nothing.
    public var onDiagnostic: (@Sendable (String) -> Void)?

    /// `existingDescriptor` adopts a socket that was already bound (and NAT-punched) during
    /// negotiation. Rebinding the same port instead would drop the seat's NAT mapping.
    public init(handoff: NVSTVideoHandoff, sendsReceiverReports: Bool = false, existingDescriptor: Int32 = -1) throws {
        self.handoff = handoff
        self.receiver = try NvstVideoReceiver(handoff: handoff)
        self.sendsReceiverReports = sendsReceiverReports
        self.adoptedDescriptor = existingDescriptor
    }

    public var stats: NvstReceiverStats { receiver.snapshot }

    /// The bundle feedback path's receiver report block: the same real reception statistics the
    /// raw socket seals into SRTCP, handed over unsealed for plain RTCP over SCTP.
    public func receiverReportBlock(now: Date = Date(),
                                    interval: TimeInterval = NvstMjolnirReceiver.receiverReportInterval) -> NvstRtcpReportBlock? {
        receiver.receiverReportBlock(now: now, interval: interval)
    }
    /// The counters the feedback reports need, without copying the per-second arrays.
    public var feedbackCounters: NvstVideoReceiver.FeedbackCounters { receiver.feedbackCounters }
    /// Where the per-packet receive cost goes: decrypt, parse, or reassembly.
    public var processStageTimings: NvstVideoReceiver.StageNanoseconds { receiver.stageTimings }
    public var fecFindings: NvstFecRecovery.Findings { receiver.fecFindings }
    /// The receive buffer the kernel actually granted, for the heartbeat.
    public private(set) var receiveBufferBytes = 0

    /// Requests the largest receive buffer the kernel will grant, halving from `first` down to
    /// `last`. Returns the size in effect afterwards, whether or not any request succeeded.
    static func growReceiveBuffer(_ descriptor: Int32,
                                  first: Int32 = 8 * 1024 * 1024,
                                  last: Int32 = 256 * 1024) -> Int {
        var requested = first
        while requested >= last {
            var value = requested
            if setsockopt(descriptor, SOL_SOCKET, SO_RCVBUF, &value, socklen_t(MemoryLayout<Int32>.size)) == 0 {
                break
            }
            requested /= 2
        }
        var granted: Int32 = 0
        var length = socklen_t(MemoryLayout<Int32>.size)
        guard getsockopt(descriptor, SOL_SOCKET, SO_RCVBUF, &granted, &length) == 0 else { return 0 }
        return Int(granted)
    }

    /// What arrived on this socket, by datagram class. Separates "nothing reaches us" from
    /// "packets reach us but do not authenticate".
    public var inbound: NvstInboundCounters { counterLock.lock(); defer { counterLock.unlock() }; return counters }

    /// Binds `0.0.0.0:<mjolnir port>` and starts the receive, keepalive and report loops.
    public func start() throws {
        let descriptor: Int32
        if adoptedDescriptor >= 0 {
            descriptor = adoptedDescriptor
        } else {
            let created = socket(AF_INET, SOCK_DGRAM, 0)
            guard created >= 0 else { throw ReceiverError.socket(String(cString: strerror(errno))) }
            var reuse: Int32 = 1
            setsockopt(created, SOL_SOCKET, SO_REUSEADDR, &reuse, socklen_t(MemoryLayout<Int32>.size))
            var address = sockaddr_in()
            address.sin_family = sa_family_t(AF_INET)
            address.sin_port = (handoff.mjolnirUDPPort ?? handoff.clientUDPPort).bigEndian
            address.sin_addr.s_addr = INADDR_ANY
            let bindResult = withUnsafePointer(to: &address) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                    bind(created, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_in>.size))
                }
            }
            guard bindResult == 0 else {
                let reason = String(cString: strerror(errno))
                close(created)
                throw ReceiverError.socket(reason)
            }
            descriptor = created
        }
        // A 5K keyframe arrives as a burst of hundreds of datagrams. On the OS default receive
        // buffer the tail of that burst is dropped by the kernel, which is indistinguishable from
        // network loss and unrecoverable without FEC. NVIDIA's own client sizes this explicitly
        // ("Receive buffer size set to %d [b]") and halves its request until one is accepted.
        receiveBufferBytes = Self.growReceiveBuffer(descriptor)

        var flags = fcntl(descriptor, F_GETFL, 0)
        flags |= O_NONBLOCK
        _ = fcntl(descriptor, F_SETFL, flags)
        socketDescriptor = descriptor

        let source = DispatchSource.makeReadSource(fileDescriptor: descriptor, queue: queue)
        source.setEventHandler { [weak self] in self?.drainSocket() }
        source.setCancelHandler { [weak self] in
            guard let self else { return }
            // The close shares the send lock: a send in flight on another thread could otherwise
            // `sendto` a descriptor number the kernel has already reissued to something else.
            sendLock.lock()
            if socketDescriptor >= 0 {
                close(socketDescriptor)
                socketDescriptor = -1
            }
            sendLock.unlock()
        }
        source.resume()
        readSource = source

        if sendsReceiverReports {
            startReceiverReports()
            counterLock.lock()
            counters.receiverReportTimerArmed = true
            counterLock.unlock()
        }
        startRttProbes()
    }

    /// Starts the NATT punch. Deliberately separate from `start()`: the official client binds this
    /// socket at SETUP but punches only after the ANNOUNCE answer (~130 ms after it in its own
    /// log, DTLS already done on a 5 ms path), and sends PLAY 2 ms after the first punch. The seat
    /// binds the video destination while it services PLAY, so the punch has to be ahead of PLAY:
    /// the transport calls this from the ANNOUNCE-ready hook, before PLAY goes out.
    public func beginHolePunch() {
        guard handoff.iceCredentials != nil else { return }
        queue.async { [weak self] in
            guard let self, pingTimer == nil else { return }
            punchStartedAt = Date()
            startHolePunch(interval: Self.pingIntervalBeforeConnection)
        }
    }

    public func stop() {
        onDiagnostic?("mjolnir receiver stopping")
        // The punch handler runs on `queue` and reinstalls `pingTimer` when the burst gives way to
        // the keepalive; a stop from another thread could nil the slot in between and leave that
        // replacement running. The flag is what the handler checks before installing anything.
        sendLock.lock()
        isStopped = true
        sendLock.unlock()
        pingTimer?.cancel()
        pingTimer = nil
        reportTimer?.cancel()
        reportTimer = nil
        rttTimer?.cancel()
        rttTimer = nil
        readSource?.cancel()
        readSource = nil
    }

    /// Asks the peer for a fresh keyframe on this socket's SRTCP path.
    public func requestKeyframe() {
        guard let ssrc = receiver.feedbackCounters.boundSSRC else { return }
        let pli = NvstRtcp.pictureLossIndication(senderSSRC: NvstVideoReceiver.clientSSRC, mediaSSRC: ssrc)
        guard let sealed = try? NvstSrtcp.seal(
            rtcpPacket: pli,
            masterKey: handoff.srtpAESKey,
            masterSalt: handoff.srtpSalt,
            senderSSRC: NvstVideoReceiver.clientSSRC,
            srtcpIndex: nextSrtcpIndex()
        ) else { return }
        send(sealed)
    }

    /// SRTCP index for feedback sealed under the client SSRC (PLI and NACK share one context, so
    /// they share one counter). Sealing everything at a constant 0 meant the seat's SRTCP replay
    /// window accepted only the first PLI/NACK of the whole session and silently discarded every
    /// one after it — and reused the same AES-GCM nonce for different plaintexts.
    private var srtcpSendIndex: UInt32 = 0
    func nextSrtcpIndex() -> UInt32 {
        counterLock.lock()
        defer { counterLock.unlock() }
        let index = srtcpSendIndex
        srtcpSendIndex &+= 1
        return index
    }

    /// How many SRTCP feedback packets (PLI/NACK) have been sealed, for tests and diagnostics.
    public var srtcpFeedbackIndex: UInt32 {
        counterLock.lock()
        defer { counterLock.unlock() }
        return srtcpSendIndex
    }

    /// Asks the seat to retransmit the packets in `first...last`, rather than the whole picture.
    ///
    /// The seat is told we support this (`video[0].enableRtpNack:1` rides in every ANNOUNCE), so
    /// the only reason a two-packet gap has been costing a full 5K keyframe is that we never asked.
    /// Returns the number of packets named, or 0 when the range is too wide to be worth repairing
    /// packet by packet.
    @discardableResult
    public func requestRetransmission(firstMissing: UInt64, lastMissing: UInt64) -> Int {
        guard let ssrc = receiver.feedbackCounters.boundSSRC, lastMissing >= firstMissing else { return 0 }
        let span = Int(lastMissing - firstMissing) + 1
        // Beyond a burst this size a keyframe arrives sooner than the retransmissions would.
        guard span <= NvstRtcp.maximumNackEntries * 16 else { return 0 }
        let missing = (firstMissing...lastMissing).map { UInt16(truncatingIfNeeded: $0) }
        guard let nack = NvstRtcp.genericNack(senderSSRC: NvstVideoReceiver.clientSSRC,
                                              mediaSSRC: ssrc,
                                              missing: missing) else { return 0 }
        guard let sealed = try? NvstSrtcp.seal(
            rtcpPacket: nack,
            masterKey: handoff.srtpAESKey,
            masterSalt: handoff.srtpSalt,
            senderSSRC: NvstVideoReceiver.clientSSRC,
            srtcpIndex: nextSrtcpIndex()
        ) else { return 0 }
        send(sealed)
        counterLock.lock()
        nacksSent += 1
        nackedPackets += span
        counterLock.unlock()
        return span
    }

    /// NACKs sent, and how many packets they named.
    public var nackSummary: (requests: Int, packets: Int) {
        counterLock.lock()
        defer { counterLock.unlock() }
        return (nacksSent, nackedPackets)
    }
    private var nacksSent = 0
    private var nackedPackets = 0

    // MARK: - Receive

    /// Datagrams handled per wake-up. The loop used to run until the socket ran dry, which on a
    /// busy media socket is never — it monopolised the queue and starved everything else on it.
    /// The read source fires again immediately if more are waiting.
    ///
    /// Raised from 64 because a 5120x2160@120 session hit the cap on 5,201 wake-ups out of 8,100:
    /// each of those left datagrams in an 8 MB kernel buffer that is filling at ~14 MB/s, and what
    /// overflows is lost exactly like network loss. Measured cost is ~8 us per packet, so even a
    /// full batch is ~2 ms of work — and the timers that the old cap protected now live on
    /// `feedbackQueue`, not this one.
    static let maxDatagramsPerWakeUp = 256

    private static let pingPrefix = Data("PING".utf8)
    private static let pongResponse = Data("PONG".utf8)

    /// One report per second, as RTCP prescribes and as upstream's NVST transport uses. Raising it
    /// to 10 Hz moved the frame rate 16.5 -> 17.5 fps, so density is not what the sender waits on.
    public static let receiverReportInterval: TimeInterval = 1

    private func drainSocket() {
        var readThisWakeUp = 0
        let drainStart = DispatchTime.now().uptimeNanoseconds
        defer {
            let elapsed = DispatchTime.now().uptimeNanoseconds - drainStart
            counterLock.lock()
            counters.datagramsPerWakeUp[min(readThisWakeUp, 8), default: 0] += 1
            counters.drainNanoseconds += elapsed
            counterLock.unlock()
        }
        for _ in 0..<Self.maxDatagramsPerWakeUp {
            var peer = sockaddr_in()
            var peerLength = socklen_t(MemoryLayout<sockaddr_in>.size)
            let received = withUnsafeMutablePointer(to: &peer) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                    recvfrom(socketDescriptor, &drainBuffer, drainBuffer.count, 0, sockaddrPointer, &peerLength)
                }
            }
            if received <= 0 { return }
            readThisWakeUp += 1
            let datagram = Data(drainBuffer[0..<received])
            counterLock.lock()
            counters.record(datagram: datagram, at: Int(DispatchTime.now().uptimeNanoseconds &- startedAtNanoseconds) / 1_000_000_000)
            counters.recordPeer("\(Self.dottedQuad(peer.sin_addr.s_addr)):\(UInt16(bigEndian: peer.sin_port))")
            counterLock.unlock()
            // Any inbound datagram means the mapping is established: the official client drops
            // from the 20 ms punch cadence to a 100 ms keepalive at that point, and holding 20 ms
            // for a whole session is ~170 packets/s at a media port.
            markConnected()
            // Bifrost's NattHolePunch verifies a ping-hash as well as STUN
            // (`HandlePingHashLegacy` / `HandlePingHashHmac`), and the official probe answers a
            // literal PING with PONG. The video socket's keepalive is a ping, not necessarily a
            // STUN exchange, so answer both.
            if datagram.count >= 4, datagram.prefix(4) == Self.pingPrefix {
                send(Self.pongResponse)
                counterLock.lock()
                counters.responsesSent += 1
                counterLock.unlock()
                continue
            }
            if NvstDatagramClassifier.looksLikeSTUN(datagram) {
                // Binding Success (type 0x0101) answers one of this socket's own RTT probes;
                // anything else STUN-shaped is a request from the seat to answer.
                if datagram.count >= 2, datagram[0] == 0x01, datagram[1] == 0x01 {
                    handleStunResponse(datagram)
                } else {
                    answerStun(datagram, from: peer)
                }
                continue
            }
            handle(datagram: datagram)
        }
    }

    private func handle(datagram: Data) {
        let processStart = DispatchTime.now().uptimeNanoseconds
        let events = receiver.process(datagram: datagram)
        let handlerStart = DispatchTime.now().uptimeNanoseconds
        counterLock.lock()
        counters.processNanoseconds += handlerStart - processStart
        counterLock.unlock()
        defer {
            let done = DispatchTime.now().uptimeNanoseconds
            counterLock.lock()
            counters.handlerNanoseconds += done - handlerStart
            counterLock.unlock()
        }
        for event in events {
            switch event {
            case .frame(let unit):
                callbackLock.lock()
                let handler = onAccessUnit
                callbackLock.unlock()
                handler?(unit)
            case .recoveryNeeded(let firstMissing, let lastMissing):
                // Ask for the packets, but never rely on it: NACK repair on this path is unproven
                // (every NACK before 2026-08-27 was sealed with SRTCP index 0 and discarded by the
                // seat's replay window after the first, so "small gaps ride on the NACK alone" was
                // never actually exercised). The keyframe request is the recovery that is known to
                // work; the NACK stays as a best-effort repair that can only make the keyframe wait
                // shorter, and recovery events are rare enough (~0.1/s measured) that the keyframe
                // cost is noise.
                requestRetransmission(firstMissing: firstMissing, lastMissing: lastMissing)
                callbackLock.lock()
                let handler = onRecoveryNeeded
                callbackLock.unlock()
                handler?(nil)
            case .chainBroken(let frameIndex):
                // Nothing identifiable to retransmit. The frame with the hole is named so the
                // transport can invalidate it at the seat right away, before the decoder has even
                // seen the damage.
                callbackLock.lock()
                let handler = onRecoveryNeeded
                callbackLock.unlock()
                handler?(frameIndex)
            case .dropped(let reason):
                callbackLock.lock()
                let handler = onDrop
                callbackLock.unlock()
                handler?(reason)
            }
        }
    }

    // MARK: - Round trip

    /// The media path's measured round trip in milliseconds, or -1 before the first sample.
    ///
    /// The seat never answers libwebrtc's ICE connectivity checks on the bundle, so the HUD's
    /// latency was permanently `--`. It does answer ICE-format Binding Requests on the video peer
    /// port (that is how `NvstBundleIceProbe` establishes the bundle), so this socket asks once a
    /// second and times the answer — a round trip measured on the same path the video rides.
    public var roundTripMilliseconds: Double {
        counterLock.lock()
        defer { counterLock.unlock() }
        return lastRoundTripMilliseconds
    }

    var lastRoundTripMilliseconds: Double = -1
    private var pendingRttProbes: [Data: UInt64] = [:]
    private var rttTimer: DispatchSourceTimer?
    static let rttProbeInterval: TimeInterval = 1.0


    private func startReceiverReports() {
        // The official client feeds the seat's rate controller about 18 times a second. One report
        // per second is RTCP's default, but on this protocol it reads as a starved receiver: the
        // stream ramps to a fraction of the negotiated rate and stays there.
        let timer = DispatchSource.makeTimerSource(queue: feedbackQueue)
        timer.schedule(deadline: .now() + Self.receiverReportInterval,
                       repeating: Self.receiverReportInterval,
                       leeway: .milliseconds(10))
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            counterLock.lock()
            counters.receiverReportPolls += 1
            counterLock.unlock()
            guard let report = receiver.pollReceiverReport(interval: Self.receiverReportInterval) else {
                onDiagnostic?("mjolnir receiver-report tick produced nothing (ssrc=\(receiver.snapshot.boundSSRC.map { String(format: "0x%08x", $0) } ?? "unbound"))")
                return
            }
            guard send(report) else { return }
            counterLock.lock()
            counters.receiverReportsSent += 1
            counterLock.unlock()
        }
        timer.resume()
        reportTimer = timer
    }

    // MARK: - Send

    private let sendLock = NSLock()
    private var sendDestination: sockaddr_in?

    /// Builds the peer address once instead of re-parsing the dotted quad on every one of the
    /// ~8,500 sends a second. Callers hold `sendLock`.
    private func resolveSendDestination() -> sockaddr_in? {
        if let sendDestination { return sendDestination }
        guard let address = Self.inetAddr(handoff.videoPeerIP) else { return nil }
        var destination = sockaddr_in()
        destination.sin_family = sa_family_t(AF_INET)
        destination.sin_port = handoff.videoPeerPort.bigEndian
        destination.sin_addr.s_addr = address
        sendDestination = destination
        return destination
    }

    /// Reports whether the datagram actually left the socket. A silent `sendto` failure looks
    /// exactly like a NAT that never answers, which is the wrong diagnosis to chase.
    @discardableResult
    private func send(_ data: Data, toPort port: UInt16? = nil) -> Bool {
        // The send path and the cancel handler's close share a lock: sends come from transport
        // threads while the close happens on the source's queue, and an unlocked `sendto` could
        // hit a descriptor that is already closed — or worse, recycled.
        sendLock.lock()
        defer { sendLock.unlock() }
        guard socketDescriptor >= 0 else { return false }
        guard var destination = resolveSendDestination() else {
            counterLock.lock()
            counters.sendFailures += 1
            counterLock.unlock()
            return false
        }
        if let port { destination.sin_port = port.bigEndian }
        let sent = data.withUnsafeBytes { bytes in
            withUnsafePointer(to: &destination) { pointer in
                pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
                    sendto(socketDescriptor, bytes.baseAddress, bytes.count, 0, sockaddrPointer, socklen_t(MemoryLayout<sockaddr_in>.size))
                }
            }
        }
        guard sent != data.count else { return true }
        let failure = errno
        counterLock.lock()
        counters.sendFailures += 1
        if counters.firstSendError == 0 { counters.firstSendError = failure }
        counterLock.unlock()
        return false
    }

    /// The IPv4 address behind a dotted quad, or nil when it is not exactly four octets of at
    /// most 255 — a malformed handoff address used to silently target a truncated one.
    static func inetAddr(_ dottedQuad: String) -> in_addr_t? {
        let octets = dottedQuad.split(separator: ".").compactMap { UInt32($0) }
        guard octets.count == 4, octets.allSatisfy({ $0 <= 255 }) else { return nil }
        let result = (octets[0] << 24) | (octets[1] << 16) | (octets[2] << 8) | octets[3]
        return in_addr_t(result.bigEndian)
    }

    static func dottedQuad(_ address: in_addr_t) -> String {
        let host = UInt32(bigEndian: address)
        return "\((host >> 24) & 0xff).\((host >> 16) & 0xff).\((host >> 8) & 0xff).\(host & 0xff)"
    }
}

// MARK: - STUN, hole punching and round-trip probing

extension NvstMjolnirReceiver {

    /// A relayed inbound Binding Request is answered with an authenticated Binding Success,
    /// keyed by our own ICE password (ping-version 6 "PONG").
    private func answerStun(_ datagram: Data, from peer: sockaddr_in) {
        guard let credentials = handoff.iceCredentials,
              let transactionID = NvstStunHolePunch.bindingRequestTransactionID(datagram) else { return }
        let host = Self.dottedQuad(peer.sin_addr.s_addr)
        let port = UInt16(bigEndian: peer.sin_port)
        guard let response = NvstStunHolePunch.buildBindingSuccess(
            transactionID: transactionID,
            mappedHost: host,
            mappedPort: port,
            integrityKey: Data(credentials.localPassword.utf8)
        ) else { return }
        send(response)
        counterLock.lock()
        counters.responsesSent += 1
        counterLock.unlock()
    }

    // MARK: - Keepalive

    private func markConnected() {
        guard !connected else { return }
        connected = true
    }

    private func startHolePunch(interval: TimeInterval) {
        guard let credentials = handoff.iceCredentials, !isStopRequested else { return }
        pingTimer?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now(), repeating: interval, leeway: .milliseconds(5))
        timer.setEventHandler { [weak self] in
            self?.sendHolePunch(credentials: credentials)
        }
        timer.resume()
        pingTimer = timer
    }

    private func sendHolePunch(credentials: NVSTHandoffIceCredentials) {
        guard !isStopRequested else { return }
        punchCount += 1
        // The burst gives way to the keepalive, and the keepalive stops once the relay is up: the
        // official client sends nothing on this socket after ~7 s.
        if punchCount == Self.punchBurstCount {
            startHolePunch(interval: Self.pingIntervalAfterConnection)
        }
        if let punchStartedAt, Date().timeIntervalSince(punchStartedAt) > Self.punchDuration {
            pingTimer?.cancel()
            pingTimer = nil
            return
        }

        // Captured from the official client (2026-08-23): the video socket sends STUN Binding
        // Requests whose USERNAME is `<raw ping payload>:<local ufrag>` — the bundle socket is the
        // one that uses `ping+1`. No plaintext PING and no ping-hash datagram appears on either
        // socket, so STUN is the only keepalive form needed here.
        guard let request = NvstStunHolePunch.buildNattHolePunchRequest(
            localUfrag: credentials.localUsernameFragment,
            pingPayload: handoff.pingPayload,
            remotePassword: Data(credentials.remotePassword.utf8),
            transactionID: NvstBundleIceProbe.transactionID()
        ) else { return }
        // Every port of the seat's advertised `X-GS-ServerPort` range, so a sender on the second
        // port is never filtered by a NAT that only saw the first one punched.
        var sentAny = false
        for port in handoff.effectiveVideoPeerPunchPorts {
            if send(request, toPort: port) { sentAny = true }
        }
        guard sentAny else { return }
        counterLock.lock()
        counters.punchesSent += 1
        counterLock.unlock()
    }

    private func startRttProbes() {
        guard let credentials = handoff.iceCredentials else { return }
        let timer = DispatchSource.makeTimerSource(queue: feedbackQueue)
        timer.schedule(deadline: .now() + Self.rttProbeInterval,
                       repeating: Self.rttProbeInterval,
                       leeway: .milliseconds(50))
        timer.setEventHandler { [weak self] in
            self?.sendRttProbe(credentials: credentials)
        }
        timer.resume()
        rttTimer = timer
    }

    private func sendRttProbe(credentials: NVSTHandoffIceCredentials) {
        let transactionID = NvstBundleIceProbe.transactionID()
        guard let request = NvstStunHolePunch.buildBindingRequest(
            transactionID: transactionID,
            username: "\(credentials.remoteUsernameFragment):\(credentials.localUsernameFragment)",
            integrityKey: Data(credentials.remotePassword.utf8)
        ) else { return }
        counterLock.lock()
        pendingRttProbes[transactionID] = DispatchTime.now().uptimeNanoseconds
        // An unanswered probe must not accumulate: keep only the last few in flight.
        if pendingRttProbes.count > 4 {
            let oldest = pendingRttProbes.min { $0.value < $1.value }?.key
            if let oldest { pendingRttProbes.removeValue(forKey: oldest) }
        }
        counterLock.unlock()
        _ = send(request)
    }

    /// Matches a STUN Binding Success against an outstanding probe and records the round trip.
    private func handleStunResponse(_ datagram: Data) {
        guard datagram.count >= 20 else { return }
        guard let credentials = handoff.iceCredentials else { return }
        let transactionID = Data(datagram[8..<20])
        // Authenticate before trusting the timing: cookie, length, transaction ID and — whenever
        // the seat sends them — MESSAGE-INTEGRITY and FINGERPRINT. Without this a misrouted
        // Binding Success with an observed id would write a fake RTT into the HUD.
        guard NvstStunHolePunch.validateBindingResponse(
            datagram,
            integrityKey: Data(credentials.localPassword.utf8),
            transactionID: transactionID
        ) else { return }
        counterLock.lock()
        defer { counterLock.unlock() }
        guard let sentAt = pendingRttProbes.removeValue(forKey: transactionID) else { return }
        let elapsed = DispatchTime.now().uptimeNanoseconds
        guard elapsed > sentAt else { return }
        lastRoundTripMilliseconds = Double(elapsed - sentAt) / 1_000_000
    }
}
