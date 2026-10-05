import CoreMedia
import Foundation

/// Access is protected by the pipeline's lock. A completion consumes exactly its
/// submission, even when outputs arrive early, fail, or arrive out of order.
struct NvstDecodeCompletionLedger<Value> {
    private var entries: [UInt32: Value] = [:]
    var count: Int { entries.count }
    mutating func register(frameIndex: UInt32, value: Value) { entries[frameIndex] = value }
    mutating func take(frameIndex: UInt32) -> Value? { entries.removeValue(forKey: frameIndex) }
}

/// A bounded compressed-frame queue. If a chain is discarded, dependent frames must
/// wait for a fresh keyframe; decoding them would corrupt the reference chain.
struct NvstDecodeFrameInbox<Value> {
    struct Entry {
        let value: Value
        let enqueuedAt: UInt64
    }
    struct Events {
        var discarded = 0
        var resynchronised = false
        var requestKeyframe = false
    }
    let capacity: Int
    let maximumAgeNanoseconds: UInt64
    private var entries: [Entry] = []
    private(set) var awaitingKeyframe = false
    private var lastRequestAt: UInt64?
    var count: Int { entries.count }

    init(capacity: Int = 32, maximumAgeNanoseconds: UInt64 = 250_000_000) {
        precondition(capacity > 0)
        self.capacity = capacity
        self.maximumAgeNanoseconds = maximumAgeNanoseconds
    }

    mutating func offer(_ value: Value, isKeyframe: Bool, now: UInt64) -> Events {
        var events = Events()
        if !awaitingKeyframe && (entries.count >= capacity || oldestExpired(now: now)) {
            events = discardChain()
        }
        if awaitingKeyframe && !isKeyframe {
            events.discarded += 1
        } else {
            if isKeyframe { awaitingKeyframe = false; lastRequestAt = nil }
            entries.append(Entry(value: value, enqueuedAt: now))
        }
        events.requestKeyframe = shouldRequestKeyframe(now: now)
        return events
    }

    mutating func take(now: UInt64) -> (entry: Entry?, events: Events) {
        var events = Events()
        if oldestExpired(now: now) { events = discardChain() }
        events.requestKeyframe = shouldRequestKeyframe(now: now)
        return (entries.isEmpty ? nil : entries.removeFirst(), events)
    }

    mutating func removeAll() {
        entries.removeAll()
        awaitingKeyframe = false
        lastRequestAt = nil
    }

    private func oldestExpired(now: UInt64) -> Bool {
        guard let first = entries.first, now >= first.enqueuedAt else { return false }
        return now - first.enqueuedAt > maximumAgeNanoseconds
    }

    private mutating func discardChain() -> Events {
        let discarded = entries.count
        entries.removeAll()
        awaitingKeyframe = true
        lastRequestAt = nil
        return Events(discarded: discarded, resynchronised: true)
    }

    private mutating func shouldRequestKeyframe(now: UInt64) -> Bool {
        guard awaitingKeyframe else { return false }
        if let lastRequestAt, now >= lastRequestAt, now - lastRequestAt < 400_000_000 { return false }
        lastRequestAt = now
        return true
    }
}

/// Session-relative time, shared by the transport actor and the video pipeline that runs off it.
///
/// NVST timestamps are session-scale, not epoch-scale — a seat that sanity-checks them against its
/// own session time discards an epoch value — so both sides have to read the same origin.
public final class NvstSessionClock: @unchecked Sendable {
    let lock = NSLock()
    var startedAt: Date?

    public init() {}

    /// Starts the clock. Later calls are ignored, so the origin cannot drift mid-session.
    public func start(at date: Date = Date()) {
        lock.lock()
        if startedAt == nil { startedAt = date }
        lock.unlock()
    }

    public var startDate: Date? {
        lock.lock()
        defer { lock.unlock() }
        return startedAt
    }

    public func elapsedMicroseconds(now: Date = Date()) -> UInt64 {
        guard let start = startDate else { return 0 }
        return UInt64(max(0, now.timeIntervalSince(start)) * 1_000_000)
    }
}

/// Decode and acknowledge video, off the transport actor.
///
/// Every access unit used to be handed to the actor as its own `Task`, which put decode in the same
/// serial queue as QoS reports, keepalives and every input event. Actor execution is one job at a
/// time, so a burst of frames drained one behind another: each frame waited for all the work queued
/// ahead of it, and the wait compounded frame by frame (measured: 46 → 62 → 78 → 92 → 109 → 126 ms
/// across six consecutive frames) until the burst cleared and latency snapped back to ~1 ms. That
/// is the random 500–1000 ms spike, seen from the inside.
///
/// The video path has no reason to be on the actor: the decoder is its own lock-guarded object and
/// the bundle's channel writes are thread-safe. It runs here instead, on one serial queue of its
/// own, so a slow decode delays only the next frame — never input, feedback or keepalives — and
/// nothing else can delay a frame.
@available(iOS 17.0, *)
public final class NvstVideoPipeline: @unchecked Sendable {
    /// One frame's time through the client, in milliseconds.
    public struct StageTimings: Sendable, Equatable {
        /// Receive thread finished the access unit -> this pipeline started on it (queue backlog).
        public var hop = 0.0
        /// Submission-to-output completion, including decoder queueing and any session rebuild.
        public var decode = 0.0
        /// The frame-ack (and pacing report) write onto the SCTP control channel.
        public var ack = 0.0

        public var total: Double { hop + decode + ack }

        mutating func raise(to other: StageTimings) {
            hop = max(hop, other.hop)
            decode = max(decode, other.decode)
            ack = max(ack, other.ack)
        }

        mutating func add(_ other: StageTimings) {
            hop += other.hop
            decode += other.decode
            ack += other.ack
        }
    }

    /// The last `capacity` decode times, frame-counted rather than time-windowed so it covers the
    /// same slice of the stream regardless of frame rate.
    ///
    /// Exists because `Counters.total.decode / framesHandled` is a lifetime mean: one bad frame
    /// (a session rebuild, a stall) stays baked into it forever, and choppiness from motion is a
    /// tail problem a mean cannot show at all — measured, a mean of 15 ms sat on a stream whose
    /// individual frames ranged 6-28 ms with the display-visible hitches at the top of that range.
    public struct RecentDecodeTimes: Sendable, Equatable {
        static let capacity = 256
        private var samples: [Double] = []
        private var writeIndex = 0

        mutating func record(_ milliseconds: Double) {
            if samples.count < Self.capacity {
                samples.append(milliseconds)
            } else {
                samples[writeIndex] = milliseconds
                writeIndex = (writeIndex + 1) % Self.capacity
            }
        }

        /// Linear-interpolated between the two nearest ranks, so `p99` moves smoothly as new
        /// frames arrive rather than jumping a whole sample width at a time.
        func percentile(_ p: Double) -> Double {
            guard !samples.isEmpty else { return -1 }
            let sorted = samples.sorted()
            guard sorted.count > 1 else { return sorted[0] }
            let rank = p * Double(sorted.count - 1)
            let low = Int(rank)
            let high = min(low + 1, sorted.count - 1)
            let fraction = rank - Double(low)
            return sorted[low] + (sorted[high] - sorted[low]) * fraction
        }

        var maximum: Double { samples.max() ?? -1 }
        var sampleCount: Int { samples.count }
    }

    public struct Counters: Sendable {
        public var framesHandled: UInt64 = 0
        public var frameAcksSent = 0
        public var frameAckFailures = 0
        public var pacingReportsSent = 0
        public var pacingReportFailures = 0
        public var missingParameterSetFrames = 0
        public var slowFrames = 0
        /// Times the pipeline fell behind and skipped to a keyframe.
        public var latencyResyncs = 0
        /// Frames dropped while waiting for that keyframe.
        public var framesSkippedForLatency = 0
        public var queuedFrames = 0
        public var peakQueuedFrames = 0
        public var lastDecodeLatencyMilliseconds = 0.0
        public var peak = StageTimings()
        public var total = StageTimings()
        /// Frames still awaiting decode output at each submit, by count. See the submit path.
        public var inFlightHistogram: [Int: Int] = [:]
        /// The last `RecentDecodeTimes.capacity` frames' decode cost, for `decodeP50Milliseconds`
        /// etc. — see that type's doc for why the lifetime mean above isn't enough on its own.
        public var recentDecodeTimes = RecentDecodeTimes()

        public var decodeP50Milliseconds: Double { recentDecodeTimes.percentile(0.5) }
        public var decodeP99Milliseconds: Double { recentDecodeTimes.percentile(0.99) }
        public var decodeMaxMilliseconds: Double { recentDecodeTimes.maximum }

        /// Peak names the worst stall; mean names whether the client keeps up at all; the tail
        /// names whether motion is hitching right now, which peak (session lifetime) and mean
        /// (diluted by however many frames have run since) both hide.
        public var timingSummary: String {
            let frames = Double(max(1, framesHandled))
            let inFlight = inFlightHistogram.sorted { $0.key < $1.key }.map { "\($0.key):\($0.value)" }.joined(separator: ",")
            return String(format: "peak[hop=%.1f decode=%.1f ack=%.1f] mean[hop=%.2f decode=%.2f ack=%.2f]ms"
                          + " tail[p50=%.1f p99=%.1f max=%.1f]ms(n=%d) inFlight=%@ queue[depth=%d peak=%d resync=%d skipped=%d]",
                          peak.hop, peak.decode, peak.ack,
                          total.hop / frames, total.decode / frames, total.ack / frames,
                          decodeP50Milliseconds, decodeP99Milliseconds, decodeMaxMilliseconds, recentDecodeTimes.sampleCount,
                          inFlight, queuedFrames, peakQueuedFrames, latencyResyncs, framesSkippedForLatency)
        }
    }

    /// A frame the client spent longer than this on is a stall, not jitter: at 120 fps the whole
    /// budget is 8.3 ms.
    public static let slowFrameMilliseconds = 50.0
    public static let maximumLoggedSlowFrames = 60
    /// Consecutive hard decode failures before the stream is called unrecoverable.
    public static let fatalDecodeFailureCount = 30
    /// How often to re-ask for the keyframe while waiting.
    public static let keyframeRetryInterval = 0.4

    let decoder: NvstVideoToolboxDecoder
    private let clock: NvstSessionClock
    private let frameTimeMicroseconds: UInt32
    /// The real display's vsync interval, in microseconds. Geronimo's own strings
    /// ("Pace server frames to match client vsync", `CVDisplayLinkGetActualOutputVideoRefreshPeriod`
    /// imports) show the seat's frame pacer targets whatever vsync interval the client reports —
    /// this was hardcoded to a hardware-agnostic 16000 before, meaning we told the seat to pace to
    /// ~62.5 Hz regardless of the client's real display.
    private let displayVsyncMicroseconds: UInt32
    let logger: (@Sendable (String) -> Void)?
    /// The client-facing VSync mode: whether `0x203` pacing reports go out and which vsync
    /// interval they claim. Read and written under `lock` — `applyVsyncMode` runs on the
    /// transport actor while `sendFrameAck` reads it on the decode queue.
    private var vsyncMode: NvstVsyncMode
    /// Hands the raw access unit to whatever else wants it (the media-session stream). Must not
    /// block: it is called on the decode queue.
    private let mediaSink: (@Sendable (NvstAccessUnit) -> Void)?
    private let onKeyframeNeeded: @Sendable () -> Void
    private let onFatalDecodeError: @Sendable (String) -> Void

    /// Below the receive loop's `.userInteractive` deliberately: decode falling a frame behind
    /// costs latency, while the receive loop falling behind costs packets.
    let queue = DispatchQueue(label: "com.opennow.nvst.decode", qos: .userInitiated)
    let lock = NSLock()
    /// The video receiver is armed at SETUP, before the ICE/DTLS bundle exists — the bundle needs
    /// SETUP's own ping payload — so the ack channel arrives later than this object does.
    var bundle: NvstNativeBundle?
    private var counters = Counters()
    private var loggedSlowFrames = 0
    private var frameAckNumber: UInt32 = 0
    private var lastFrameAckAt: Date?
    /// Throttle for `logFeedbackSample`.
    private var lastFeedbackLogAt: Date?
    private var overrunsSinceFeedbackLog = 0
    private var worstOverrunMicroseconds = 0
    /// Session-clock stamp of the previous frame ack, for the measured inter-frame interval.
    private var lastAckElapsedMicroseconds: UInt64?
    /// Frames handled since the last 0x203 report, for that report's `groupCount` — a real
    /// capture (2026-08-28) confirmed this is genuinely "frames since last report", not a fixed
    /// small constant.
    private var framesSincePacingReport = 0
    private var consecutiveDecodeFailures = 0
    private var isStopped = false
    private var inbox = NvstDecodeFrameInbox<NvstAccessUnit>()
    private var workerScheduled = false
    /// Decoder failures already seen by the async-failure check, so each is answered once.
    private var observedDecodeFailures: UInt64 = 0
    private var lastKeyframeRequestAt: UInt64?

    /// Register before calling VideoToolbox: its output callback can run before the
    /// submission returns. Match by wire frame index instead of shifting a FIFO.
    private struct PendingCompletion {
        let unit: NvstAccessUnit
        let hopMilliseconds: Double
        let startedAt: UInt64
    }
    private var pendingCompletions = NvstDecodeCompletionLedger<PendingCompletion>()
    public init(decoder: NvstVideoToolboxDecoder,
                clock: NvstSessionClock,
                frameTimeMicroseconds: UInt32,
                displayVsyncMicroseconds: UInt32,
                vsyncMode: NvstVsyncMode = .adaptive,
                logger: (@Sendable (String) -> Void)?,
                mediaSink: (@Sendable (NvstAccessUnit) -> Void)?,
                onKeyframeNeeded: @escaping @Sendable () -> Void,
                onFatalDecodeError: @escaping @Sendable (String) -> Void) {
        self.decoder = decoder
        self.clock = clock
        self.frameTimeMicroseconds = frameTimeMicroseconds
        self.displayVsyncMicroseconds = displayVsyncMicroseconds
        self.vsyncMode = vsyncMode
        self.logger = logger
        self.mediaSink = mediaSink
        self.onKeyframeNeeded = onKeyframeNeeded
        self.onFatalDecodeError = onFatalDecodeError
        // The ack used to fire right after `decoder.decode(unit)` returned — which is only the
        // synchronous submission accepted by VideoToolbox, not the frame actually finishing
        // decode. That answered the seat's frame pacer before the frame the pacer was asking
        // about even existed. This ties the ack to the real asynchronous completion instead.
        decoder.onDecodeCompleted = { [weak self] frameIndex, success in
            self?.handleDecodeCompleted(frameIndex: frameIndex, success: success)
        }
        decoder.onFatalFormatFailure = onFatalDecodeError
    }

    /// Hands over the channel the frame acks go out on, once the bundle is up.
    public func attach(bundle: NvstNativeBundle?) {
        lock.lock()
        self.bundle = bundle
        lock.unlock()
    }

    public var snapshot: Counters {
        lock.lock()
        defer { lock.unlock() }
        return counters
    }

    /// Called on the receive thread. Stamps the handover so queue backlog stays measurable, then
    /// gets off that thread immediately: draining the media socket must never wait for a decode.
    public func submit(_ unit: NvstAccessUnit) {
        let enqueued = DispatchTime.now().uptimeNanoseconds
        lock.lock()
        guard !isStopped else { lock.unlock(); return }
        let events = inbox.offer(unit, isKeyframe: unit.isKeyframe, now: enqueued)
        updateInboxCounters(events)
        let schedule = !workerScheduled && inbox.count > 0
        if schedule { workerScheduled = true }
        lock.unlock()
        reportInboxEvents(events)
        if schedule { queue.async { [weak self] in self?.drainInbox() } }
    }

    public func stop() {
        lock.lock()
        isStopped = true
        inbox.removeAll()
        counters.queuedFrames = 0
        pendingCompletions = NvstDecodeCompletionLedger()
        lock.unlock()
    }

    private func drainInbox() {
        while true {
            lock.lock()
            guard !isStopped else { workerScheduled = false; lock.unlock(); return }
            let next = inbox.take(now: DispatchTime.now().uptimeNanoseconds)
            updateInboxCounters(next.events)
            if next.entry == nil { workerScheduled = false }
            lock.unlock()
            reportInboxEvents(next.events)
            guard let entry = next.entry else { return }
            process(entry.value, enqueuedAt: entry.enqueuedAt)
        }
    }

    /// Called only under lock. The queue never retains more than its fixed capacity.
    private func updateInboxCounters(_ events: NvstDecodeFrameInbox<NvstAccessUnit>.Events) {
        counters.framesSkippedForLatency += events.discarded
        if events.resynchronised { counters.latencyResyncs += 1 }
        counters.queuedFrames = inbox.count
        counters.peakQueuedFrames = max(counters.peakQueuedFrames, inbox.count)
    }

    private func reportInboxEvents(_ events: NvstDecodeFrameInbox<NvstAccessUnit>.Events) {
        if events.resynchronised {
            logger?("NVST decoder queue exceeded its age/capacity budget; discarded \(events.discarded) queued frames and waiting for a fresh keyframe")
        }
        if events.requestKeyframe { onKeyframeNeeded() }
    }

    private func process(_ unit: NvstAccessUnit, enqueuedAt: UInt64) {
        lock.lock()
        let stopped = isStopped
        lock.unlock()
        guard !stopped else { return }

        // VideoToolbox rejects a frame with a broken reference chain in its asynchronous output
        // handler, not by throwing out of `decode` — the live -12909 bursts never touched the
        // catch below. The failure is visible here one frame later, which at stream rate is
        // milliseconds; ask for the keyframe the chain needs, at the retry cadence.
        let asyncFailures = decoder.failedFrameCount
        if asyncFailures > observedDecodeFailures {
            observedDecodeFailures = asyncFailures
            requestKeyframeThrottled()
        }

        let started = DispatchTime.now().uptimeNanoseconds
        var timings = StageTimings()
        timings.hop = Self.milliseconds(from: enqueuedAt, to: started)
        mediaSink?(unit)

        do {
            // How many earlier submissions VideoToolbox still has not answered when this one goes
            // in. A decoder that holds each frame until the next arrives shows 1 here on nearly
            // every frame; one that returns frames as they finish shows 0.
            lock.lock()
            guard !isStopped else { lock.unlock(); return }
            counters.inFlightHistogram[pendingCompletions.count, default: 0] += 1
            pendingCompletions.register(frameIndex: unit.frameIndex,
                value: PendingCompletion(unit: unit, hopMilliseconds: timings.hop, startedAt: started))
            lock.unlock()
            let accepted = try decoder.decode(unit)
            consecutiveDecodeFailures = 0
            // Decode is asynchronous from here — VideoToolbox has only accepted the submission.
            // `handleDecodeCompleted` fires the ack once the frame is actually decoded (or
            // failed), matched by frame ID even when callbacks arrive out of order.
            if !accepted { discardDecodeCompletion(frameIndex: unit.frameIndex) }
        } catch NvstVideoToolboxDecoder.DecoderError.missingParameterSets {
            discardDecodeCompletion(frameIndex: unit.frameIndex)
            // Normal until the seat answers with a keyframe; nudge it. Silently counting these was
            // hiding a stalled stream: with no feedback channel the nudge never left the client.
            lock.lock()
            counters.missingParameterSetFrames += 1
            lock.unlock()
            onKeyframeNeeded()
            return
        } catch {
            discardDecodeCompletion(frameIndex: unit.frameIndex)
            if decoder.requiresTenBit444, let error = error as? NvstVideoToolboxDecoder.DecoderError {
                switch error {
                case .requested444NotDelivered, .unsupported444Hardware, .output444NotPreserved,
                     .hardwareRequired, .formatDescriptionFailed:
                    onFatalDecodeError(error.localizedDescription)
                    return
                default: break
                }
            }
            consecutiveDecodeFailures += 1
            logger?("NVST decode error: \(error.localizedDescription)")
            // A bad-data rejection means the reference chain is broken, and every following frame
            // fails the same way until a keyframe arrives — nothing else on this path asks for
            // one. Waiting for the fatal threshold cost a measured ~141 consecutive rejected
            // frames (~1.2 s at 120 fps) per loss event, each of them also unacknowledged, which
            // reads to the seat's frame pacer as a client that stopped consuming.
            requestKeyframeThrottled()
            if consecutiveDecodeFailures >= Self.fatalDecodeFailureCount {
                consecutiveDecodeFailures = 0
                onFatalDecodeError(error.localizedDescription)
            }
            return
        }
    }

    private func discardDecodeCompletion(frameIndex: UInt32) {
        lock.lock()
        _ = pendingCompletions.take(frameIndex: frameIndex)
        lock.unlock()
    }

    /// Acknowledges the exact completed frame without blocking the decoder callback.
    private func handleDecodeCompleted(frameIndex: UInt32, success: Bool) {
        lock.lock()
        let entry = pendingCompletions.take(frameIndex: frameIndex)
        lock.unlock()
        guard let entry else { return }
        guard success else { return }
        let decodedAt = DispatchTime.now().uptimeNanoseconds
        var timings = StageTimings()
        timings.hop = entry.hopMilliseconds
        timings.decode = Self.milliseconds(from: entry.startedAt, to: decodedAt)
        sendFrameAck(unit: entry.unit, decodedAt: decodedAt, timings: &timings)
        record(timings, frameNumber: frameAckNumber, unit: entry.unit)
    }

    /// The live half of a VSync change (the HUD tile, mid-session). The seat-facing half —
    /// `framePacing.mode`/`feedbackMode` — was fixed at ANNOUNCE and no control-plane command
    /// moves it, so this switches the client-facing half only: whether `0x203` pacing reports go
    /// out, and which vsync interval they claim. See `NvstVsyncMode`.
    /// Called from the transport actor; `sendFrameAck` reads the mode on the decode queue, so it
    /// is stored under `lock`.
    func applyVsyncMode(_ mode: NvstVsyncMode) {
        lock.lock()
        let previousMode = vsyncMode
        let modeChanged = previousMode != mode
        vsyncMode = mode
        // A paused report stream must not resume with a stale "frames since last report":
        // `groupCount` is the wire's cadence evidence, and a count spanning the pause would
        // claim the client kept reporting while it was not.
        if modeChanged { framesSincePacingReport = 0 }
        lock.unlock()
        guard modeChanged else { return }
        logger?("NVST frame pacing mode \(previousMode.label) -> \(mode.label) (\(mode.isSendingFramePacingReports ? "pacing reports resumed" : "pacing reports paused"); seat-side mode fixed at ANNOUNCE)")
    }

    /// Acknowledges one decoded frame to the seat's frame pacer. `video[0].framePacing.mode:1`
    /// with `framePacing.feedbackMode:1` puts the seat in the pacer that follows the client's own
    /// cadence, and the capture shows the native stack answering every single frame with command
    /// `0x204` — 1790 of them for 1789 frames. Answering none is what held the seat at 8.7 frames
    /// per second against the native stack's 60.05 on the same title: the pacer had no cadence to
    /// open up against.
    private func sendFrameAck(unit: NvstAccessUnit, decodedAt: UInt64, timings: inout StageTimings) {
        let now = Date()
        let nowMicroseconds = clock.elapsedMicroseconds(now: now)
        // The ack bookkeeping moves under the same lock as the channel reference: decode
        // completion callbacks are not guaranteed to arrive serialized, and the frame number and
        // inter-frame baseline are a read-modify-write pair that must not interleave.
        lock.lock()
        let channel = bundle
        frameAckNumber += 1
        lastFrameAckAt = now
        let measuredInterFrame = lastAckElapsedMicroseconds.map { UInt32(clamping: nowMicroseconds &- $0) }
            ?? frameTimeMicroseconds
        lastAckElapsedMicroseconds = nowMicroseconds
        // The VSync mode decides whether the seat gets a pacing report at all (only Adaptive
        // feeds `FRAME_PACING_FEEDBACK_INTERVAL`), and the report's +16 claim: the real display
        // interval when pacing to the display, the stream target otherwise.
        lock.unlock()
        guard let bundle = channel else { return }
        // The capture documents this field as the MEASURED interval since the previous frame —
        // the pacer's view of the cadence the client actually sustains (15905 µs on a 60 fps
        // session, not the nominal 16667). Sending the constant target here claimed a perfect
        // cadence every frame; the matched A/B (vendored 120 vs ours ~108–112, same game, same
        // scene, same bitrate) says the pacer does not open up for that.
        //
        // Tried (2026-08-28): updating this reference every other ack instead of every ack, to
        // match a ~2x-at-120fps shape observed in a captured official-client session. No effect on
        // stream fps in testing, and it makes this diagnostic value less accurate for its own
        // purpose (a real per-ack interval), so reverted to the direct measurement.
        // What we can actually measure: how long this frame spent between leaving the reassembler
        // and finishing decode. The capture's five marks are a rising series from one frame origin,
        // so a single measured latency repeated across them is the honest reading of it.
        let latencyMilliseconds = Float(timings.hop + timings.decode)
        let ack = NvstFrameAck(
            frameNumber: frameAckNumber,
            // Session-relative, not epoch. Only the delta matters to the pacer, and the remote-input
            // path already showed this seat rejecting an epoch-scale timestamp where it expected a
            // session one; the capture's own value is ~20000, which is session scale.
            clientTimeMilliseconds: Double(clock.elapsedMicroseconds(now: now)) / 1000,
            frameBytes: UInt32(truncatingIfNeeded: unit.bytes.count),
            interFrameMicroseconds: measuredInterFrame,
            stageMilliseconds: [latencyMilliseconds],
            auxiliaryMilliseconds: [latencyMilliseconds, latencyMilliseconds, 0, 0, 0, 0]
        )
        // The pacer's target interval rides on 0x203, roughly every 6-8 frames as the capture
        // does — but only while the VSync mode feeds the seat at all (see `NvstVsyncMode`).
        var pacingSent = 0
        var pacingFailed = 0
        if vsyncMode.isSendingFramePacingReports {
            let outcome = sendPacingReportIfDue(
                frameAckNumber: frameAckNumber,
                measuredInterFrame: measuredInterFrame,
                hopMilliseconds: timings.hop,
                decodeMilliseconds: timings.decode,
                claimedVsyncMicroseconds: vsyncMode.isReportingDisplayVsync ? displayVsyncMicroseconds : frameTimeMicroseconds,
                bundle: bundle
            )
            pacingSent = outcome.sent
            pacingFailed = outcome.failed
        }
        let acked = bundle.sendPartiallyReliableControl(ack.command)
        timings.ack = Self.milliseconds(from: decodedAt, to: DispatchTime.now().uptimeNanoseconds)

        lock.lock()
        counters.pacingReportsSent += pacingSent
        counters.pacingReportFailures += pacingFailed
        if acked { counters.frameAcksSent += 1 } else { counters.frameAckFailures += 1 }
        let firstFailure = !acked && counters.frameAckFailures == 1
        counters.lastDecodeLatencyMilliseconds = Double(latencyMilliseconds)
        lock.unlock()
        if firstFailure { logger?("NVST frame ack write failed") }
    }

    /// Emits the `0x203` pacing report when the cadence says one is due, and returns how the
    /// write landed. The `groupCount` counter advances every reported frame, so a mode that
    /// paused reporting (Off/On) resumes with a fresh count rather than one spanning the pause.
    ///
    /// A real plaintext capture of the official client (2026-08-28, see `NvstFramePacingReport`'s
    /// doc) settled the payload: the client sends its RAW measured frame time here, unclamped —
    /// it can and does exceed the target — and the target itself is the session's real negotiated
    /// frame interval, not a fixed value. Both of those were wrong here before: this used to
    /// clamp to at most `target` and hardcode a ~75 fps constant regardless of what was negotiated.
    /// Tried (2026-09-05): sending the decoded-frame interval here instead of hop+decode, on the
    /// reading that the vendor's ~15.9 ms on a 60 fps session is a cadence, not a latency.
    /// Cyberpunk benchmark at 3840x2160, cap 150, same seat class: stream 85–107 fps and 31–71
    /// Mbps with either value (84–107 and 31–67 the run before). The seat's frame controller is
    /// not steering off this field; the plateau is seat-side.
    private func sendPacingReportIfDue(frameAckNumber: UInt32,
                                       measuredInterFrame: UInt32,
                                       hopMilliseconds: Double,
                                       decodeMilliseconds: Double,
                                       claimedVsyncMicroseconds: UInt32,
                                       bundle: NvstNativeBundle) -> (sent: Int, failed: Int) {
        framesSincePacingReport += 1
        guard frameAckNumber % NvstFramePacingReport.framesPerReport == 1 else { return (0, 0) }
        let clientMicroseconds = Int((hopMilliseconds + decodeMilliseconds) * 1000)
        let pacing = NvstFramePacingReport(
            frameNumber: frameAckNumber,
            targetFrameTimeMicroseconds: frameTimeMicroseconds,
            measuredFrameTimeMicroseconds: UInt32(clamping: clientMicroseconds),
            // Adaptive claims the real display interval (the seat paces to the display); On
            // paces to the negotiated stream interval instead. Both were fixed at the mode's
            // `isReportingDisplayVsync` in the ack's critical section.
            displayVsyncMicroseconds: claimedVsyncMicroseconds,
            groupCount: UInt32(clamping: framesSincePacingReport)
        )
        framesSincePacingReport = 0
        let isSent = bundle.sendPartiallyReliableControl(pacing.command)
        logFeedbackSample(interFrame: measuredInterFrame, measuredFrameTimeMicroseconds: clientMicroseconds,
                          clientMicroseconds: clientMicroseconds, hopMs: hopMilliseconds, decodeMs: decodeMilliseconds)
        return isSent ? (1, 0) : (0, 1)
    }

    /// Throttled to 1/s: what we actually sent in 0x204/0x203, next to what it implies about
    /// cadence and where the time went. Correlating this against the periodic `NVST counters`
    /// received-fps series is how a guess about seat pacer behavior gets checked against reality
    /// instead of staying a guess — this is what a vendor capture would otherwise be needed for.
    ///
    /// An overrun used to bypass the throttle, on the reading that a measured time past the seat's
    /// target is the rare, interesting case. At 120 fps it is not rare: the target is 8333 µs and
    /// 5K decode alone measures 7.9–10 ms, so nearly every report overran and this logged ~17
    /// times a second — synchronously, on the decode queue, at roughly half a millisecond a line.
    /// The overruns are still reported, as a count and a worst case on the next throttled line, so
    /// the signal survives without the frame path paying for it.
    private func logFeedbackSample(interFrame: UInt32, measuredFrameTimeMicroseconds: Int,
                                   clientMicroseconds: Int, hopMs: Double, decodeMs: Double) {
        let isOverrun = measuredFrameTimeMicroseconds > Int(frameTimeMicroseconds)
        lock.lock()
        let now = Date()
        if isOverrun {
            overrunsSinceFeedbackLog += 1
            worstOverrunMicroseconds = max(worstOverrunMicroseconds, measuredFrameTimeMicroseconds)
        }
        let shouldLog = lastFeedbackLogAt.map { now.timeIntervalSince($0) >= 1.0 } ?? true
        let overruns = overrunsSinceFeedbackLog
        let worstOverrun = worstOverrunMicroseconds
        if shouldLog {
            lastFeedbackLogAt = now
            overrunsSinceFeedbackLog = 0
            worstOverrunMicroseconds = 0
        }
        lock.unlock()
        guard shouldLog, let logger else { return }
        let impliedFps = interFrame > 0 ? 1_000_000.0 / Double(interFrame) : 0
        let overrunSummary = overruns > 0 ? String(format: " OVERRUN x%d worstUs=%d", overruns, worstOverrun) : ""
        logger(String(format: "NVST feedback%@ ack#=%u interFrameUs=%u impliedFps=%.1f measuredUs=%d"
                       + " clientUs=%d targetUs=%u hop=%.2fms decode=%.2fms",
                       overrunSummary, frameAckNumber, interFrame, impliedFps, measuredFrameTimeMicroseconds,
                       clientMicroseconds, frameTimeMicroseconds, hopMs, decodeMs))
    }

    /// One keyframe repairs the whole run of a broken chain, so requests ride the retry cadence
    /// rather than firing per failed frame.
    private func requestKeyframeThrottled() {
        let now = DispatchTime.now().uptimeNanoseconds
        lock.lock()
        let sinceLastRequest = lastKeyframeRequestAt.map { Self.seconds(from: $0, to: now) } ?? .infinity
        let shouldRequest = sinceLastRequest >= Self.keyframeRetryInterval
        if shouldRequest { lastKeyframeRequestAt = now }
        lock.unlock()
        if shouldRequest { onKeyframeNeeded() }
    }

    private func record(_ timings: StageTimings, frameNumber: UInt32, unit: NvstAccessUnit) {
        lock.lock()
        counters.framesHandled &+= 1
        counters.peak.raise(to: timings)
        counters.total.add(timings)
        counters.recentDecodeTimes.record(timings.decode)
        let isSlow = timings.total >= Self.slowFrameMilliseconds
        if isSlow { counters.slowFrames += 1 }
        let shouldLog = isSlow && loggedSlowFrames < Self.maximumLoggedSlowFrames
        if shouldLog { loggedSlowFrames += 1 }
        lock.unlock()
        guard shouldLog else { return }
        logger?(String(format: "NVST SLOW FRAME #%u total=%.1fms hop=%.1f decode=%.1f ack=%.1f bytes=%d key=%@",
                       frameNumber, timings.total, timings.hop, timings.decode, timings.ack,
                       unit.bytes.count, unit.isKeyframe ? "y" : "n"))
    }

    private static func milliseconds(from start: UInt64, to end: UInt64) -> Double {
        end > start ? Double(end - start) / 1_000_000 : 0
    }

    private static func seconds(from start: UInt64, to end: UInt64) -> Double {
        end > start ? Double(end - start) / 1_000_000_000 : 0
    }
}
