import Foundation

/// Why one datagram was discarded.
///
/// Deliberately an enum rather than a message: FEC repair packets take the drop path by design —
/// 16,433 of them in a 65 s session alongside 7,711 real drops — and every one of those used to
/// build a localized error string for a handler that discards it (`onDrop = { _ in }`). The text
/// is now produced only where something actually prints it.
public enum NvstReceiveDrop: Sendable, Equatable {
    case reassembly(NvstReassemblyDrop)
    case unprotectFailed
    case replayed
    case malformedPacket
    case unexpectedSSRC(UInt32)
    case staleSequence(UInt64)
    case duplicateSequence(UInt64)

    public var description: String {
        switch self {
        case .reassembly(let drop): drop.localizedDescription
        case .unprotectFailed: "NVST packet failed SRTP authentication."
        case .replayed: "NVST packet was a replay."
        case .malformedPacket: "NVST packet could not be parsed."
        case .unexpectedSSRC(let ssrc): "unexpected SSRC \(ssrc)"
        case .staleSequence(let index): "stale RTP packet \(index)"
        case .duplicateSequence(let index): "duplicate RTP packet \(index)"
        }
    }
}

public enum NvstReceiveEvent: Sendable {
    case frame(NvstAccessUnit)
    case dropped(NvstReceiveDrop)
    /// A sequence gap broke the decoder's reference chain; the feedback plane must ask for a
    /// fresh keyframe. The missing range is in extended RTP sequence space and can be NACKed.
    case recoveryNeeded(firstMissingIndex: UInt64, lastMissingIndex: UInt64)
    /// The reference chain is broken but nothing identifiable is missing in RTP sequence space
    /// (a hole in the GS stream sequence with contiguous RTP delivery — a slot consumed
    /// upstream, not a lost packet). Retransmission cannot repair it; a keyframe can, and so can
    /// telling the seat not to reference `frameIndex` (the frame the hole was in) — the latter
    /// without the keyframe's cost.
    case chainBroken(frameIndex: UInt32)
}

public struct NvstReceiverStats: Equatable, Sendable {
    public var authenticatedPackets: UInt64 = 0
    public var fecPackets: UInt64 = 0
    public var droppedPackets: UInt64 = 0
    /// Packets that arrived ahead of the delivery head — a lower sequence was still missing.
    /// Feeds the client RTP stats report's out-of-order field.
    public var outOfOrderPackets: UInt64 = 0
    /// Packets whose sequence slot was already delivered or finalized when they arrived.
    public var latePackets: UInt64 = 0
    /// Packets received twice; the second copy is dropped.
    public var duplicatePackets: UInt64 = 0
    /// Packets rebuilt by FEC recovery and injected back into the reorder path.
    public var recoveredPackets: UInt64 = 0
    /// The largest single finalized-loss range, in packets.
    public var maxLossBurst: UInt32 = 0
    /// The deepest the reorder buffer has been, in packets.
    public var maxReorderDepth: UInt32 = 0
    public var framesEmitted: UInt64 = 0
    public var keyframesEmitted: UInt64 = 0
    public var recoveries: UInt64 = 0
    /// Extended RTP sequence numbers that aged past the reorder window without arriving — true
    /// inferred wire loss, as opposed to `droppedPackets`, which aggregates local rejections
    /// (authentication, parse, SSRC, stale) that say nothing about the network.
    public var finalizedLossPackets: UInt64 = 0
    public var boundSSRC: UInt32?
    public var highestSequence: UInt32 = 0
    /// Packets whose GS flags claim start-of-frame, versus those the FEC-block gate actually
    /// accepts as one. A gap between the two means the gate is rejecting real frame starts.
    public var startOfFrameFlagged: UInt64 = 0
    public var startOfFrameAccepted: UInt64 = 0
    /// Packets belonging to a frame that spans more than one FEC block.
    public var multiBlockPackets: UInt64 = 0
    /// Highest `fecLastBlock` seen, to show when the seat changes its FEC layout mid-session.
    public var highestFecLastBlock: UInt8 = 0
    /// Frames abandoned mid-assembly when the next start-of-frame arrived.
    public var abandonedFrames: UInt64 = 0
    /// How many times `frameIndex` changed between consecutive accepted packets. The native
    /// stack's own QoS reports say it sees 60 frames per second on the same title where we
    /// assemble 15; this number separates "the seat sends fewer frames" from "we mis-read the
    /// frame boundary".
    public var frameIndexChanges: UInt64 = 0
    /// Assembled frame bytes per whole second of the session. A screen that never changes encodes
    /// to a flat series; a UI reacting to a click spikes. With no view of the remote framebuffer
    /// this is how an input event's effect is observed at all.
    public var frameBytesPerSecond: [Int] = []
    /// The largest single frame in each second. A ten-event input burst lasts a fraction of a
    /// second, so summing a whole second dilutes it about tenfold; the largest frame in the second
    /// keeps the spike intact.
    public var maxFrameBytesPerSecond: [Int] = []
    /// Frames completed in each whole second of the session.
    public var framesPerSecond: [Int] = []
    /// RTP sequence span against packets actually accepted. If the span is far wider than the
    /// count, the seat sent frames that never reached us; if they match, it never sent them.
    public var sequenceSpan: UInt32 = 0
    /// First and last RTP timestamp seen. Their difference over the 90 kHz clock is how many
    /// seconds of media the seat actually sent, which against wall-clock time separates "few
    /// frames per second" from "the whole stream delivered in slow motion".
    public var firstRtpTimestamp: UInt32?
    public var lastRtpTimestamp: UInt32 = 0
    /// Wire bytes accepted, for the QoS report's cumulative byte field.
    public var bytesReceived: UInt64 = 0
    /// What the last receiver report actually told the sender. Its congestion controller acts on
    /// these, so a wrong jitter or loss figure is indistinguishable from a congested path.
    public var receiverReportsSent: UInt64 = 0
    public var lastFractionLost: UInt8 = 0
    public var lastCumulativeLost: UInt32 = 0
    public var lastJitter: UInt32 = 0
    /// Receiver reports that could not be sealed, and why. A dead feedback plane is otherwise
    /// indistinguishable from a sender that simply sends little.
    public var receiverReportFailures: UInt64 = 0
    public var lastReceiverReportFailure: String?
}

/// SRTP/RTP receive pipeline for the Mjolnir video socket: unprotect → reorder → GS parse →
/// access-unit assembly. Socket-free so it can be driven from recorded traffic.
///
/// SRTP profile support is deliberately limited to the AEAD-GCM family, which is what the
/// Mjolnir policy negotiates (`sec_serv_conf_and_auth` + 256-bit keys → AES-256-GCM with an
/// 8-byte tag). The AES-CM/HMAC-SHA1 profiles are rejected rather than silently mis-decrypted.
public final class NvstVideoReceiver: @unchecked Sendable {
    /// The SSRC this client sends its own RTCP under — "ONOW". The bundle's feedback sender already
    /// uses this one; the raw Mjolnir socket was reusing the seat's media SSRC instead.
    public static let clientSSRC: UInt32 = 0x4f4e_4f57

    /// How long an open gap may wait for FEC repair before it is finalized as loss, in packets.
    /// A block's parity follows all of its sources, and the GS FEC index is 10 bits — up to 1023
    /// sources plus parity — so the repair for an early-frame hole can trail it by over a
    /// thousand packets. At the measured ~8,500 packets/s that is ~140 ms of worst-case wait
    /// before an unrepairable gap falls back to the keyframe path.
    public static let fecRepairReorderWindow = 1200

    public enum ReceiverError: LocalizedError, Equatable, Sendable {
        case unsupportedProfile(String)

        public var errorDescription: String? {
            switch self {
            case .unsupportedProfile(let profile): "NVST video SRTP profile \(profile) is not supported on the Mjolnir socket."
            }
        }
    }

    let lock = NSLock()
    let handoff: NVSTVideoHandoff
    private let cipher: SrtpGcm8
    private let sessionSalt: Data
    private let tagLength: Int
    private let srtcpMasterKey: Data
    private let srtcpMasterSalt: Data
    private let reorderWindow: Int
    private let reassembler: NvstFrameReassembler
    private var replay = SrtpReplayWindow()
    private var reorder: [UInt64: NvstRtpVideoPacket] = [:]
    private var nextIndex: UInt64?
    private var boundSSRC: UInt32?
    private var srtcpIndex: UInt32 = 0
    private var reportFailures: UInt64 = 0
    private var baseSequence: UInt32?
    private var lastSeenFrameIndex: UInt32?
    private var lastJitterTimestamp: UInt32?
    private var frameBytesEpoch: Date?
    private var priorExpected: UInt32 = 0
    private var priorReceived: UInt32 = 0
    private func recordFrameBytes(_ count: Int) {
        let now = Date()
        let started = frameBytesEpoch ?? now
        frameBytesEpoch = started
        let bucket = Int(now.timeIntervalSince(started))
        guard bucket >= 0, bucket < 3600 else { return }
        if stats.frameBytesPerSecond.count <= bucket {
            stats.frameBytesPerSecond.append(contentsOf: Array(repeating: 0, count: bucket + 1 - stats.frameBytesPerSecond.count))
        }
        if stats.maxFrameBytesPerSecond.count <= bucket {
            stats.maxFrameBytesPerSecond.append(contentsOf: Array(repeating: 0, count: bucket + 1 - stats.maxFrameBytesPerSecond.count))
        }
        stats.frameBytesPerSecond[bucket] += count
        stats.maxFrameBytesPerSecond[bucket] = max(stats.maxFrameBytesPerSecond[bucket], count)
        // Frames per second alongside bytes per second. A session average cannot tell "steady 120
        // during motion, low on a static menu" from "wandering the whole time", and those want
        // opposite fixes — the first is the encoder legitimately having nothing to send.
        if stats.framesPerSecond.count <= bucket {
            stats.framesPerSecond.append(contentsOf: Array(repeating: 0, count: bucket + 1 - stats.framesPerSecond.count))
        }
        stats.framesPerSecond[bucket] += 1
    }

    /// RFC 3550 interarrival jitter, smoothed by 1/16 as the spec prescribes.
    private var interarrivalJitter: Double = 0
    private var lastTransit: Double?
    private var lastReportFailure: String?
    private var lastReportAt: Date?
    private var stats = NvstReceiverStats()
    private let fecRecovery = NvstFecRecovery()

    /// FEC recovery health: verification against the live parity stream, and repairs performed.
    public var fecFindings: NvstFecRecovery.Findings { fecRecovery.snapshot }

    public init(handoff: NVSTVideoHandoff) throws {
        switch handoff.srtpProfile {
        case .aeadAes128Gcm, .aeadAes128Gcm8, .aeadAes256Gcm, .aeadAes256Gcm8:
            break
        default:
            throw ReceiverError.unsupportedProfile(handoff.srtpProfile.rawValue)
        }
        self.handoff = handoff
        // RFC 3711 session key (label 0x00) and salt (label 0x02) from the handoff master material.
        let sessionKey = try SrtpKeyDerivation.derive(
            key: handoff.srtpAESKey,
            salt: handoff.srtpSalt,
            label: 0x00,
            length: handoff.srtpProfile.masterKeyLength
        )
        self.sessionSalt = try SrtpKeyDerivation.derive(key: handoff.srtpAESKey, salt: handoff.srtpSalt, label: 0x02, length: 12)
        self.cipher = try SrtpGcm8(key: sessionKey)
        self.tagLength = handoff.srtpProfile.authenticationTagLength
        self.srtcpMasterKey = handoff.srtpAESKey
        self.srtcpMasterSalt = handoff.srtpSalt
        self.reorderWindow = max(1, min(handoff.reorderWindowPackets, 128))
        self.reassembler = NvstFrameReassembler(maxAccessUnitBytes: handoff.maxAccessUnitBytes, codec: handoff.codec)
    }

    public var snapshot: NvstReceiverStats { lock.lock(); defer { lock.unlock() }; return stats }

    /// Just the counters the feedback reports need.
    ///
    /// `stats` carries per-second arrays that are appended to on every frame, so snapshotting the
    /// whole struct at the report cadence made copy-on-write duplicate those arrays about sixty
    /// times a second — and they grow with session length.
    public struct FeedbackCounters: Equatable, Sendable {
        public var framesEmitted: UInt64 = 0
        public var bytesReceived: UInt64 = 0
        public var lastRtpTimestamp: UInt32 = 0
        public var boundSSRC: UInt32?
    }

    public var feedbackCounters: FeedbackCounters {
        lock.lock()
        defer { lock.unlock() }
        return FeedbackCounters(framesEmitted: stats.framesEmitted,
                                bytesReceived: stats.bytesReceived,
                                lastRtpTimestamp: stats.lastRtpTimestamp,
                                boundSSRC: boundSSRC)
    }

    public func reset() {
        lock.lock()
        defer { lock.unlock() }
        reorder.removeAll()
        nextIndex = nil
        reassembler.reset()
    }

    /// Thread CPU time. Wall time cannot tell "this code is slow" from "this thread was not
    /// running", and the two demand opposite fixes — optimise the path, or stop starving it.
    @inline(__always)
    static func threadCpuNanoseconds() -> UInt64 {
        clock_gettime_nsec_np(CLOCK_THREAD_CPUTIME_ID)
    }

    /// Where `process` spends its time, in nanoseconds, summed over the session.
    public struct StageNanoseconds: Sendable, Equatable {
        public var unprotect: UInt64 = 0
        public var parse: UInt64 = 0
        public var assemble: UInt64 = 0
        /// Reorder-window bookkeeping only.
        public var reorder: UInt64 = 0
        /// `reassembler.push` for a packet that did NOT complete a frame: the per-packet append.
        public var append: UInt64 = 0
        /// `reassembler.push` for the packet that DID complete a frame: the per-frame Annex-B
        /// scans, which copy the whole access unit to `[UInt8]` more than once.
        public var complete: UInt64 = 0
        public var total: UInt64 = 0
        /// Thread CPU time across the same span as `total`. Wall far above CPU means the receive
        /// thread is being descheduled, not that the receive path is expensive.
        public var cpu: UInt64 = 0

        public func summary(packets: UInt64) -> String {
            let divisor = Double(max(1, packets))
            return String(format: "unprotect=%.1fus parse=%.1fus assemble=%.1fus[reorder=%.1f append=%.1f complete=%.1f] total=%.1fus cpu=%.1fus",
                          Double(unprotect) / divisor / 1000, Double(parse) / divisor / 1000,
                          Double(assemble) / divisor / 1000,
                          Double(reorder) / divisor / 1000, Double(append) / divisor / 1000,
                          Double(complete) / divisor / 1000,
                          Double(total) / divisor / 1000, Double(cpu) / divisor / 1000)
        }
    }

    public var stageTimings: StageNanoseconds {
        lock.lock()
        defer { lock.unlock() }
        return stageNanoseconds
    }
    private var stageNanoseconds = StageNanoseconds()

    /// Unprotects and processes one datagram from the video peer.
    public func process(datagram: Data) -> [NvstReceiveEvent] {
        lock.lock()
        defer { lock.unlock() }

        // Stage timing: the drain loop was measured spending 134 us per packet, and "SRTP is slow"
        // has already been the wrong answer once. Attribute it instead of assuming.
        let stageStart = DispatchTime.now().uptimeNanoseconds
        let cpuStart = Self.threadCpuNanoseconds()
        defer {
            stageNanoseconds.total += DispatchTime.now().uptimeNanoseconds - stageStart
            stageNanoseconds.cpu += Self.threadCpuNanoseconds() - cpuStart
        }

        let unprotected: Unprotected
        do {
            unprotected = try unprotect(datagram)
            stageNanoseconds.unprotect += DispatchTime.now().uptimeNanoseconds - stageStart
        } catch NvstRtpParseError.replayed {
            stats.droppedPackets += 1
            return [.dropped(.replayed)]
        } catch {
            stats.droppedPackets += 1
            return [.dropped(.unprotectFailed)]
        }
        let plaintext = unprotected.plaintext
        let extendedIndex = unprotected.index

        let parseStart = DispatchTime.now().uptimeNanoseconds
        let packet: NvstRtpVideoPacket
        do {
            packet = try NvstVideoPacketParser.parse(plaintext, header: unprotected.header)
            stageNanoseconds.parse += DispatchTime.now().uptimeNanoseconds - parseStart
        } catch {
            stats.droppedPackets += 1
            return [.dropped(.malformedPacket)]
        }

        if let rejection = bindLocked(packet) { return rejection }

        stats.authenticatedPackets += 1
        stats.bytesReceived += UInt64(datagram.count)
        let arrivals = arrivalsLocked(plaintext: plaintext, packet: packet, extendedIndex: extendedIndex)
        recordPacketStatsLocked(packet: packet, extendedIndex: extendedIndex)

        var events: [NvstReceiveEvent] = []
        let assembleStart = DispatchTime.now().uptimeNanoseconds
        defer { stageNanoseconds.assemble += DispatchTime.now().uptimeNanoseconds - assembleStart }
        let ordered = arrivals.flatMap { pushReorder(index: $0.index, packet: $0.packet, events: &events) }
        stageNanoseconds.reorder += DispatchTime.now().uptimeNanoseconds - assembleStart
        for candidate in ordered {
            reassembleLocked(candidate, events: &events)
        }
        // Snapshots of counters owned elsewhere, taken once per batch instead of once per packet:
        // the reassembler's lock and these running totals only change on frame/report boundaries.
        stats.abandonedFrames = reassembler.abandonedFrameCount
        stats.receiverReportFailures = reportFailures
        stats.lastReceiverReportFailure = lastReportFailure
        return events
    }

    /// Binds the receiver to the first SSRC it sees and rejects anything else. Returns the drop
    /// events when the packet does not belong to this stream, `nil` when it is accepted.
    private func bindLocked(_ packet: NvstRtpVideoPacket) -> [NvstReceiveEvent]? {
        if handoff.rtpSSRC != 0, packet.ssrc != handoff.rtpSSRC {
            stats.droppedPackets += 1
            return [.dropped(.unexpectedSSRC(packet.ssrc))]
        }
        if let bound = boundSSRC, packet.ssrc != bound {
            stats.droppedPackets += 1
            return [.dropped(.unexpectedSSRC(packet.ssrc))]
        }
        boundSSRC = boundSSRC ?? packet.ssrc
        return nil
    }

    /// The received packet plus anything FEC could rebuild from it.
    ///
    /// Repairs lost packets from the parity stream the moment enough shards exist — before the
    /// reorder window could ever declare them lost. A reconstructed shard is a complete plaintext
    /// packet (RTP header, GS block and payload), so it joins the stream through the same
    /// parse-and-reorder path as a received one; a late arrival of the real packet then reads as an
    /// ordinary duplicate.
    private func arrivalsLocked(plaintext: Data,
                                packet: NvstRtpVideoPacket,
                                extendedIndex: UInt64) -> [(index: UInt64, packet: NvstRtpVideoPacket)] {
        var arrivals: [(index: UInt64, packet: NvstRtpVideoPacket)] = [(extendedIndex, packet)]
        for recoveredPlaintext in fecRecovery.observe(plaintext: plaintext, packet: packet) {
            guard let repaired = try? NvstVideoPacketParser.parse(recoveredPlaintext),
                  repaired.ssrc == boundSSRC else { continue }
            stats.recoveredPackets += 1
            arrivals.append((replay.estimatedIndex(for: repaired.sequenceNumber), repaired))
        }
        return arrivals
    }

    /// Per-packet counters: frame boundaries, RTP timestamps, sequence span and jitter.
    private func recordPacketStatsLocked(packet: NvstRtpVideoPacket, extendedIndex: UInt64) {
        if lastSeenFrameIndex != packet.frameIndex {
            if lastSeenFrameIndex != nil { stats.frameIndexChanges += 1 }
            lastSeenFrameIndex = packet.frameIndex
        }
        if stats.firstRtpTimestamp == nil { stats.firstRtpTimestamp = packet.timestamp }
        stats.lastRtpTimestamp = packet.timestamp
        stats.boundSSRC = boundSSRC
        let sequence = UInt32(truncatingIfNeeded: extendedIndex)
        if baseSequence == nil { baseSequence = sequence }
        stats.highestSequence = max(stats.highestSequence, sequence)
        stats.sequenceSpan = baseSequence.map { stats.highestSequence >= $0 ? stats.highestSequence - $0 + 1 : 0 } ?? 0
        recordJitter(rtpTimestamp: packet.timestamp)
    }

    /// Feeds one in-order packet to the reassembler, appending whatever it produced to `events`.
    private func reassembleLocked(_ candidate: NvstRtpVideoPacket, events: inout [NvstReceiveEvent]) {
        if candidate.flags.contains(.startOfFrame) { stats.startOfFrameFlagged += 1 }
        if candidate.isStartOfFrame { stats.startOfFrameAccepted += 1 }
        if candidate.fecLastBlock > 0 { stats.multiBlockPackets += 1 }
        stats.highestFecLastBlock = max(stats.highestFecLastBlock, candidate.fecLastBlock)
        do {
            let pushStart = DispatchTime.now().uptimeNanoseconds
            let pushed = try reassembler.push(candidate)
            let pushCost = DispatchTime.now().uptimeNanoseconds - pushStart
            // Split per-packet appends from the frame-completion path: only the latter runs the
            // Annex-B scans, and only one packet in ~60 takes it.
            if pushed == nil { stageNanoseconds.append += pushCost } else { stageNanoseconds.complete += pushCost }
            guard let unit = pushed else { return }
            stats.framesEmitted += 1
            recordFrameBytes(unit.bytes.count)
            if unit.isKeyframe { stats.keyframesEmitted += 1 }
            events.append(.frame(unit))
        } catch let drop as NvstReassemblyDrop {
            recordReassemblyDropLocked(drop, events: &events)
        } catch {
            stats.droppedPackets += 1
            events.append(.dropped(.malformedPacket))
        }
    }

    /// Classifies a reassembly drop. A hole inside a frame leaves the decoder's reference chain
    /// broken, and only a fresh keyframe restores it. `awaitingStartOfFrame` is the consequence of a
    /// loss the reorder window already reported, so it must not ask twice.
    ///
    /// A `sequenceGap` is in GS stream-sequence space while RTP delivery was contiguous, so there is
    /// no RTP sequence number to NACK — the old code fed `streamSequence` into the RTP-space
    /// recovery event, asking the seat to retransmit an arbitrary unrelated packet and (at span 1)
    /// suppressing the keyframe request entirely.
    private func recordReassemblyDropLocked(_ drop: NvstReassemblyDrop, events: inout [NvstReceiveEvent]) {
        if drop == .notPictureData {
            stats.fecPackets += 1
        } else {
            stats.droppedPackets += 1
            if case .sequenceGap(_, _, let frameIndex) = drop {
                stats.recoveries += 1
                events.append(.chainBroken(frameIndex: frameIndex))
            }
        }
        events.append(.dropped(.reassembly(drop)))
    }

    /// RFC 3550 interarrival jitter: the smoothed difference between packet spacing on the sender's
    /// RTP clock and on ours. Reporting a flat zero tells the sender its pacing is perfect no matter
    /// what the network did.
    private func recordJitter(rtpTimestamp: UInt32) {
        // RFC 3550's jitter is only defined between packets with different sending instants, so a
        // repeated RTP timestamp compares a zero sender-side delta against a positive arrival delta
        // and contributes pure inflation.
        //
        // On this stream the guard rarely fires: a later census showed the timestamp changing on
        // almost every packet, most steps being under 100 ticks. An earlier note here claimed it
        // removed three samples in four and explained a 50 ms jitter reading — that was wrong, and
        // the drop measured after adding it is not distinguishable from run-to-run variance.
        guard rtpTimestamp != lastJitterTimestamp else { return }
        lastJitterTimestamp = rtpTimestamp
        let arrivalTicks = Date().timeIntervalSince1970 * 90_000.0
        let transit = arrivalTicks - Double(rtpTimestamp)
        defer { lastTransit = transit }
        guard let lastTransit else { return }
        let difference = abs(transit - lastTransit)
        interarrivalJitter += (difference - interarrivalJitter) / 16
    }

    /// An SRTCP Receiver Report for the raw Mjolnir socket, at most once per interval and only
    /// once the media SSRC is known. This is the feedback path used when the seat is not given
    /// an SCTP feedback channel (`general.rtcpOnSctp:0`).
    /// The RFC 3550 reception statistics both RTCP paths report, computed against the highest
    /// extended sequence received. Reporting zeros describes a receiver that has never seen a
    /// packet, which is not the same thing as a healthy one. Advances the interval window the
    /// fraction-lost figure is computed over; returns nil until an SSRC binds and again until
    /// `interval` elapses. Caller holds the lock.
    private func computeReportBlock(now: Date, interval: TimeInterval) -> NvstRtcpReportBlock? {
        guard let mediaSSRC = boundSSRC else { return nil }
        if let lastReportAt, now.timeIntervalSince(lastReportAt) < interval { return nil }
        let highest = stats.highestSequence
        let expected = baseSequence.map { highest >= $0 ? highest - $0 + 1 : 0 } ?? 0
        let received = UInt32(truncatingIfNeeded: stats.authenticatedPackets)
        let cumulativeLost = expected > received ? expected - received : 0
        let expectedInterval = expected >= priorExpected ? expected - priorExpected : 0
        let receivedInterval = received >= priorReceived ? received - priorReceived : 0
        let lostInterval = expectedInterval > receivedInterval ? expectedInterval - receivedInterval : 0
        let fractionLost: UInt8 = expectedInterval == 0
            ? 0
            : UInt8(min(255, (UInt64(lostInterval) << 8) / UInt64(expectedInterval)))
        priorExpected = expected
        priorReceived = received

        return NvstRtcpReportBlock(
            sourceSSRC: mediaSSRC,
            fractionLost: fractionLost,
            cumulativeLost: cumulativeLost,
            extendedHighestSequence: highest,
            interarrivalJitter: UInt32(interarrivalJitter)
        )
    }

    public func pollReceiverReport(now: Date = Date(), interval: TimeInterval = 1.0) -> Data? {
        lock.lock()
        defer { lock.unlock() }
        guard let block = computeReportBlock(now: now, interval: interval) else { return nil }
        // Our own SSRC, never the sender's. SRTCP derives its context and its replay window from
        // the packet's SSRC, so reporting under the seat's SSRC put our index-0,1,2... reports into
        // the context the seat uses for its *own* outbound RTCP, where they read as replays of
        // packets it already sent. Its congestion controller then hears nothing at all and backs
        // the send rate off — which is what a full session at one eighth real-time looks like.
        let rtcp = NvstRtcp.receiverReport(ssrc: Self.clientSSRC, blocks: [block])
        let sealed: Data
        do {
            sealed = try NvstSrtcp.seal(
                rtcpPacket: rtcp,
                masterKey: srtcpMasterKey,
                masterSalt: srtcpMasterSalt,
                senderSSRC: block.sourceSSRC,
                srtcpIndex: srtcpIndex
            )
        } catch {
            // Swallowing this hid a dead feedback plane behind a stream that merely looked slow:
            // the sender never hears a receiver report and backs its bitrate off.
            lastReportFailure = error.localizedDescription
            reportFailures &+= 1
            return nil
        }
        srtcpIndex &+= 1
        lastReportAt = now
        stats.receiverReportsSent += 1
        stats.lastFractionLost = block.fractionLost
        stats.lastCumulativeLost = block.cumulativeLost
        stats.lastJitter = block.interarrivalJitter
        return sealed
    }

    /// The bundle feedback path's form of the same statistics: the report block itself, for
    /// plain RTCP over SCTP, instead of an SRTCP seal. The two paths share the interval window,
    /// and only one is active in a session.
    public func receiverReportBlock(now: Date = Date(), interval: TimeInterval = 1.0) -> NvstRtcpReportBlock? {
        lock.lock()
        defer { lock.unlock() }
        guard let block = computeReportBlock(now: now, interval: interval) else { return nil }
        lastReportAt = now
        stats.lastFractionLost = block.fractionLost
        stats.lastCumulativeLost = block.cumulativeLost
        stats.lastJitter = block.interarrivalJitter
        return block
    }

    // MARK: - SRTP

    struct Unprotected {
        let plaintext: Data
        let index: UInt64
        let header: NvstVideoPacketParser.RtpHeader
    }

    /// RFC 7714 §9.1: the AAD is the RTP header including its extension, the payload is the
    /// GCM ciphertext, and the tag trails it.
    func unprotect(_ datagram: Data) throws -> Unprotected {
        guard datagram.count >= 12 + tagLength else {
            throw NvstRtpParseError.truncated("SRTP datagram")
        }
        // The header comes from the shared reader and rides along with the result, so the parse
        // step after decryption never reads it a second time. The reader indexes the datagram's
        // own storage: the intermediate `[UInt8](datagram)` this used to build was one
        // full-packet copy on the hottest path in the receiver.
        var reader = NvstByteReader(datagram)
        let header = try NvstVideoPacketParser.readHeader(&reader)
        var payloadOffset = header.extensionOffset
        if header.hasExtension {
            guard reader.remaining >= 4 else { throw NvstRtpParseError.truncated("RTP extension header") }
            try reader.skip(2)
            payloadOffset += 4 + Int(try reader.u16BE()) * 4
        }
        guard datagram.count >= payloadOffset + tagLength else { throw NvstRtpParseError.truncated("SRTP payload") }

        let sequenceNumber = header.sequenceNumber
        let ssrc = header.ssrc
        let extendedIndex = replay.estimatedIndex(for: sequenceNumber)
        // RFC 3711: reject replays before paying the GCM cost. The window only advances after
        // authentication below, so a forged packet cannot age real ones out of the window.
        guard replay.wouldAccept(extendedIndex) else {
            throw NvstRtpParseError.replayed
        }
        // RFC 7714's IV consumes only the low 32 bits of the rollover counter, so truncation is
        // the conversion rather than a narrowing of it — and it keeps an index estimate from
        // trapping here, pre-authentication, where no throw can reach the caller's catch.
        let rolloverCounter = UInt32(truncatingIfNeeded: extendedIndex >> 16)
        let iv = try SrtpKeyDerivation.gcmIV(sessionSalt: sessionSalt, ssrc: ssrc, rolloverCounter: rolloverCounter, sequenceNumber: sequenceNumber)
        // Slices share the datagram's storage, so the AAD, ciphertext and tag reach the cipher
        // without three more per-packet copies; only the re-glued plaintext allocates.
        let base = datagram.startIndex
        let aad = datagram[base ..< base + payloadOffset]
        let tag = datagram[base + datagram.count - tagLength ..< base + datagram.count]
        let ciphertext = datagram[base + payloadOffset ..< base + datagram.count - tagLength]
        let payload = try cipher.decrypt(iv: iv, aad: aad, ciphertext: ciphertext, authenticationTag: tag)
        guard replay.accept(extendedIndex) else {
            throw NvstRtpParseError.replayed
        }
        var plaintext = Data(aad)
        plaintext.append(payload)
        return Unprotected(plaintext: plaintext, index: extendedIndex, header: header)
    }

    // MARK: - Reorder

    private func pushReorder(index: UInt64, packet: NvstRtpVideoPacket, events: inout [NvstReceiveEvent]) -> [NvstRtpVideoPacket] {
        let expected = nextIndex ?? index
        if nextIndex == nil { nextIndex = index }
        if index < expected {
            stats.droppedPackets += 1
            stats.latePackets += 1
            events.append(.dropped(.staleSequence(index)))
            return []
        }
        if reorder[index] != nil {
            stats.droppedPackets += 1
            stats.duplicatePackets += 1
            events.append(.dropped(.duplicateSequence(index)))
            return []
        }
        if index > expected { stats.outOfOrderPackets += 1 }
        if index - expected >= UInt64(reorderWindow),
           // With FEC armed, a gap must outlive the chance of repair before it is loss: the
           // block's parity packets arrive after all of its sources, which at 5K is up to ~1000
           // packets after an early-frame hole — far past the plain reorder window. Holding the
           // gap costs one frame a few milliseconds of delivery delay; finalizing it early costs
           // the frame, a keyframe round trip, and the seat's frame-rate knock. The armed check
           // runs only while a gap is already open, so the hot path never takes the extra lock.
           !(index - expected < UInt64(Self.fecRepairReorderWindow) && fecRecovery.snapshot.isArmed) {
            // The gap has aged past the reorder window: whatever is still missing below the first
            // buffered packet is finalized loss, and only that range is skipped. The buffered
            // packets arrived intact and are delivered below — flushing the whole buffer here
            // turned one lost packet into 32: the up-to-31 successors it head-of-line-blocked were
            // destroyed uncounted, taking their frames (and the decoder's reference chain) with
            // them. A later gap among the survivors finalizes the same way on a later packet.
            let firstAvailable = reorder.keys.min() ?? index
            recordRecovery(first: expected, last: firstAvailable - 1, events: &events)
            nextIndex = firstAvailable
        }
        reorder[index] = packet
        stats.maxReorderDepth = max(stats.maxReorderDepth, UInt32(clamping: reorder.count))

        var ready: [NvstRtpVideoPacket] = []
        while let cursor = nextIndex, let next = reorder.removeValue(forKey: cursor) {
            ready.append(next)
            nextIndex = cursor + 1
        }
        return ready
    }

    private func recordRecovery(first: UInt64, last: UInt64, events: inout [NvstReceiveEvent]) {
        reassembler.reset()
        stats.recoveries += 1
        if last >= first {
            stats.finalizedLossPackets += last - first + 1
            stats.maxLossBurst = max(stats.maxLossBurst, UInt32(clamping: last - first + 1))
        }
        events.append(.recoveryNeeded(firstMissingIndex: first, lastMissingIndex: last))
    }
}
