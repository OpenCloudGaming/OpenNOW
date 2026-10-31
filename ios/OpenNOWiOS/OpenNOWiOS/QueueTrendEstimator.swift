import Foundation

/// Turns a sequence of observed queue positions into something a person can act on.
///
/// The rule this whole type exists to enforce: **never invent an ETA.** Every number below is
/// derived from movement that actually happened, expressed as a range rather than a countdown, and
/// withdrawn the moment the queue stops moving. A fabricated countdown that runs backwards
/// destroys trust in every other number the app shows, and the queue screen is the one people
/// stare at longest.
struct QueueTrendEstimator: Equatable {

    /// A queue that has not moved for this long is reported as holding, with no estimate.
    static let stallThreshold: TimeInterval = 90

    /// Movement older than this is dropped: an estimate built from what happened twenty minutes
    /// ago describes a queue that no longer exists.
    static let windowDuration: TimeInterval = 10 * 60

    /// Below this, one lucky jump would produce a confident-looking lie.
    static let minimumSamples = 3

    /// And below this much wall-clock the rate is dominated by sampling noise.
    static let minimumObservation: TimeInterval = 45

    struct Sample: Equatable {
        let time: Date
        let position: Int
    }

    enum Trend: Equatable {
        /// Positions are being consumed. `perMinute` is how many, averaged over the window.
        case moving(perMinute: Double)
        /// Position has not changed for longer than `stallThreshold`.
        case holding
        /// The queue went backwards. Happens when a rig is reclaimed or a zone is rebalanced.
        case slipped
        /// Not enough observation yet to say anything honest.
        case unknown
    }

    /// The estimate, in whole minutes. Always a range — a single number reads as a promise.
    struct Estimate: Equatable {
        let lowMinutes: Int
        let highMinutes: Int

        var label: String {
            if lowMinutes > 60 {
                return "over an hour left"
            }
            if lowMinutes == highMinutes {
                return lowMinutes <= 1 ? "about a minute left" : "about \(lowMinutes) minutes left"
            }
            if highMinutes > 60 {
                return "about \(lowMinutes)–60+ minutes left"
            }
            return "about \(lowMinutes)–\(highMinutes) minutes left"
        }
    }

    private(set) var samples: [Sample] = []

    init() {}

    /// Records a position. Repeated identical positions are kept, because "it has not moved" is
    /// exactly the signal the stall detector needs.
    mutating func record(position: Int, at time: Date = Date()) {
        guard position >= 0, time.timeIntervalSinceReferenceDate.isFinite else { return }
        if let last = samples.last, time <= last.time { return }
        if let last = samples.last, last.position == position, time.timeIntervalSince(last.time) < 5 {
            // Poll cadence can be faster than the queue changes. Collapse the noise.
            return
        }
        samples.append(Sample(time: time, position: position))
        prune(now: time)
    }

    mutating func reset() {
        samples.removeAll(keepingCapacity: true)
    }

    private mutating func prune(now: Date) {
        samples.removeAll { now.timeIntervalSince($0.time) > Self.windowDuration }
    }

    // MARK: Derived

    func trend(now: Date = Date()) -> Trend {
        guard let first = samples.first, let last = samples.last else { return .unknown }
        let age = now.timeIntervalSince(last.time)
        guard age.isFinite, age >= 0, age <= Self.stallThreshold else { return .unknown }
        guard samples.count >= Self.minimumSamples else { return .unknown }

        if last.position > first.position || last.position > samples[samples.count - 2].position { return .slipped }

        // Stalled is judged from the last time the number actually changed, not from the last
        // poll — otherwise a healthy 1 Hz poll makes every queue look like it is moving.
        if let lastChange = lastPositionChange(), now.timeIntervalSince(lastChange) > Self.stallThreshold {
            return .holding
        }

        let elapsed = last.time.timeIntervalSince(first.time)
        guard elapsed >= Self.minimumObservation else { return .unknown }
        let consumed = first.position - last.position
        guard consumed > 0 else { return .holding }

        return .moving(perMinute: Double(consumed) / (elapsed / 60))
    }

    /// The remaining-time band, or nil when the data does not support one.
    func estimate(now: Date = Date()) -> Estimate? {
        guard case .moving(let perMinute) = trend(now: now), perMinute > 0 else { return nil }
        guard let last = samples.last, last.position > 0 else { return nil }

        let centreMinutes = Double(last.position) / perMinute
        guard centreMinutes.isFinite, centreMinutes * 0.7 <= 60 else {
            return Estimate(lowMinutes: 61, highMinutes: 61)
        }
        // ±40 %. Queue rates are not stable enough to justify anything tighter, and a range the
        // user beats feels better than one they miss.
        let low = max(1, Int((centreMinutes * 0.7).rounded(.down)))
        let high = max(low, Int(min(61, (centreMinutes * 1.4).rounded(.up))))
        return Estimate(lowMinutes: low, highMinutes: high)
    }

    /// One line for under the position numeral. Nil when there is nothing honest to say yet.
    func supportLine(now: Date = Date()) -> String? {
        switch trend(now: now) {
        case .unknown:
            return nil
        case .holding:
            return "Queue is holding"
        case .slipped:
            return "Position moved back — wait estimate unavailable"
        case .moving:
            guard let estimate = estimate(now: now) else { return "Moving" }
            return "Moving — \(estimate.label)"
        }
    }

    /// How far through the wait the queue has come, 0…1, or nil when there is nothing honest to
    /// report. Measured against the worst position actually seen, so it can only move forward as
    /// the queue drains — a bar computed from a guessed starting point jumps backwards the first
    /// time the guess is wrong, which is worse than no bar.
    func progress() -> Double? {
        guard let worst = samples.map(\.position).max(),
              let current = samples.last?.position,
              worst > 0,
              samples.count >= Self.minimumSamples else { return nil }
        guard worst > current else { return nil }
        return min(max(Double(worst - current) / Double(worst), 0), 1)
    }

    /// Normalised 0...1 series, oldest first. Empty when there is nothing to draw.
    func sparkline() -> [Double] {
        guard samples.count >= Self.minimumSamples else { return [] }
        let positions = samples.map { Double($0.position) }
        guard let minimum = positions.min(), let maximum = positions.max(), maximum > minimum else {
            return positions.map { _ in 0.5 }
        }
        return positions.map { ($0 - minimum) / (maximum - minimum) }
    }

    private func lastPositionChange() -> Date? {
        guard samples.count >= 2 else { return samples.first?.time }
        for index in stride(from: samples.count - 1, to: 0, by: -1) where samples[index].position != samples[index - 1].position {
            return samples[index].time
        }
        return samples.first?.time
    }
}

/// Copy for the queue's hero line, so the phase names live in one place rather than being
/// reconstructed from status integers at three different call sites.
enum QueuePhaseCopy {
    static func heroSupport(position: Int?, seatSetupStep: Int?, status: Int, isLaunching: Bool) -> String {
        if let step = seatSetupStep, status == 2 {
            return "Preparing your rig — step \(step) of 4"
        }
        switch status {
        case 2: return "Preparing your rig"
        case 3: return "Connecting"
        default: break
        }
        guard let position else {
            return isLaunching ? "Asking for a rig" : "Waiting in queue"
        }
        switch position {
        case 1: return "You're next"
        case 2...5: return "Almost there"
        case 6...20: return "Nearly there"
        default: return "In the queue"
        }
    }
}
