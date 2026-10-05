import Foundation

/// Reorders the seat's audio and reports what never arrived.
///
/// The seat sends 5 ms frames at about 200 packets a second, so there is no time to wait for a
/// retransmission: a gap is concealment, not recovery. The buffer therefore holds a fixed depth,
/// emits strictly in sequence order, and names each missing sequence number exactly once so the
/// decoder conceals precisely the frames that were lost.
///
/// Its depth accounting is in packets rather than wall-clock time, which keeps the decision
/// deterministic and testable: it holds `targetDepth` packets behind the newest arrival, and once
/// more than that is held, the next expected packet is either emitted or declared lost.
public struct NvstAudioJitterBuffer: Sendable {
    public enum Emission: Equatable, Sendable {
        case payload(Data)
        /// One sequence number that will never arrive.
        case lost(UInt16)
    }

    private let targetDepth: Int
    private let maximumDepth: Int
    /// Where each packet's arrival time came from. Injected so dwell is testable without waiting.
    private let now: @Sendable () -> TimeInterval
    private var pending: [UInt64: Data] = [:]
    private var arrivalTimes: [UInt64: TimeInterval] = [:]
    private var highestExtended: UInt64?
    private var nextExpected: UInt64?
    private(set) public var lostPacketCount = 0

    /// Cumulative time the emitted packets spent waiting, and how many were emitted. These are the
    /// counters the HUD's A/V reading is computed from — the same two libwebrtc's NetEq reported, so
    /// the metric keeps its meaning rather than becoming a number with no source.
    public private(set) var jitterBufferDelaySeconds: TimeInterval = 0
    public private(set) var jitterBufferEmittedCount: UInt64 = 0

    /// - Parameters:
    ///   - targetDepth: packets held behind the newest arrival, absorbing reordering.
    ///   - maximumDepth: a ceiling, so a burst cannot grow latency without bound. The oldest
    ///     entries are dropped and reported as lost when it is exceeded.
    public init(targetDepth: Int = 3,
                maximumDepth: Int = 50,
                now: @escaping @Sendable () -> TimeInterval = { Date().timeIntervalSinceReferenceDate }) {
        self.targetDepth = max(0, targetDepth)
        self.maximumDepth = max(1, maximumDepth)
        self.now = now
    }

    public var bufferedCount: Int { pending.count }

    /// Takes one in-order packet. Duplicates and packets older than what has already been emitted
    /// are discarded rather than rewinding the stream.
    public mutating func insert(sequenceNumber: UInt16, payload: Data) {
        let extended = extend(sequenceNumber)
        let highest = max(highestExtended ?? extended, extended)
        highestExtended = highest
        let expected = nextExpected ?? extended
        if nextExpected == nil { nextExpected = extended }
        guard extended >= expected, pending[extended] == nil else { return }
        pending[extended] = payload
        arrivalTimes[extended] = now()
        enforceDepthCeiling()
    }

    /// Emits everything that can be emitted in order right now.
    public mutating func advance() -> [Emission] {
        var emitted: [Emission] = []
        while pending.count > targetDepth, let expected = nextExpected {
            emitted.append(emit(expected))
            nextExpected = expected + 1
        }
        return emitted
    }

    /// Emits whatever is left, reporting the gaps between the remaining packets. Called when the
    /// stream ends, where holding the tail would simply lose it.
    public mutating func flush() -> [Emission] {
        var emitted: [Emission] = []
        guard var expected = nextExpected else { return emitted }
        for key in pending.keys.sorted() {
            while expected < key {
                emitted.append(.lost(UInt16(truncatingIfNeeded: expected)))
                lostPacketCount += 1
                expected += 1
            }
            emitted.append(emit(key))
            expected = key + 1
        }
        pending.removeAll()
        nextExpected = expected
        return emitted
    }

    public mutating func reset() {
        pending.removeAll()
        arrivalTimes.removeAll()
        highestExtended = nil
        nextExpected = nil
    }

    private mutating func emit(_ extended: UInt64) -> Emission {
        guard let payload = pending.removeValue(forKey: extended) else {
            lostPacketCount += 1
            return .lost(UInt16(truncatingIfNeeded: extended))
        }
        if let arrival = arrivalTimes.removeValue(forKey: extended) {
            jitterBufferDelaySeconds += max(0, now() - arrival)
            jitterBufferEmittedCount &+= 1
        }
        return .payload(payload)
    }

    /// Drops the oldest entries when the ceiling is exceeded, reporting them as the losses they are.
    private mutating func enforceDepthCeiling() {
        while pending.count > maximumDepth, let oldest = pending.keys.min() {
            pending.removeValue(forKey: oldest)
            arrivalTimes.removeValue(forKey: oldest)
            if let expected = nextExpected, oldest >= expected { nextExpected = oldest + 1 }
            lostPacketCount += 1
        }
    }

    /// Maps a 16-bit sequence number onto an extended one, so a wrap at 65535 does not read as a
    /// jump backwards and flush the buffer.
    private func extend(_ sequenceNumber: UInt16) -> UInt64 {
        let value = UInt64(sequenceNumber)
        guard let highest = highestExtended else { return value }
        var candidate = (highest & ~0xFFFF) | value
        if candidate + 0x8000 < highest { candidate += 0x1_0000 }
        else if candidate > highest + 0x8000 { candidate -= 0x1_0000 }
        return candidate
    }
}
