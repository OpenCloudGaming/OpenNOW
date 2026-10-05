#if canImport(ActivityKit)
import ActivityKit
import Foundation

struct QueueActivityAttributes: ActivityAttributes {
    struct ContentState: Codable, Hashable {
        enum Phase: String, Codable, Hashable {
            case queued
            case waiting
            case ready
        }

        let phase: Phase
        let headline: String
        let detail: String
        let queueLabel: String
        let queuePosition: Int?

        /// When this wait began.
        ///
        /// Carried so the widget can render a live-ticking elapsed timer with `Text(style: .timer)`,
        /// which the system animates on its own. It is the one honest thing that can move between
        /// updates: a Live Activity is only redrawn when the app pushes state, so everything else
        /// on it is frozen between polls, and a frozen panel reads as a stuck one. Counting *up*
        /// also avoids promising a finish time — the estimate stays a range for that reason.
        var waitStartedAt: Date?

        /// 0…1 through the wait, or nil when there is nothing honest to draw.
        ///
        /// Computed in the app, not the widget: only the app has seen where the queue started, and
        /// a widget that guessed would draw a bar that jumps backwards when the estimate changes.
        var progress: Double?

        init(
            phase: Phase,
            headline: String,
            detail: String,
            queueLabel: String,
            queuePosition: Int?,
            progress: Double? = nil,
            waitStartedAt: Date? = nil
        ) {
            self.phase = phase
            self.headline = headline
            self.detail = detail
            self.queueLabel = queueLabel
            self.queuePosition = queuePosition
            self.progress = progress
            self.waitStartedAt = waitStartedAt
        }
    }

    let sessionId: String
    let gameTitle: String
    let storeName: String?
}
#else
import Foundation

struct QueueActivityAttributes {
    struct ContentState: Codable, Hashable {
        enum Phase: String, Codable, Hashable {
            case queued
            case waiting
            case ready
        }

        let phase: Phase
        let headline: String
        let detail: String
        let queueLabel: String
        let queuePosition: Int?

        /// When this wait began.
        ///
        /// Carried so the widget can render a live-ticking elapsed timer with `Text(style: .timer)`,
        /// which the system animates on its own. It is the one honest thing that can move between
        /// updates: a Live Activity is only redrawn when the app pushes state, so everything else
        /// on it is frozen between polls, and a frozen panel reads as a stuck one. Counting *up*
        /// also avoids promising a finish time — the estimate stays a range for that reason.
        var waitStartedAt: Date?
        var progress: Double?

        init(
            phase: Phase,
            headline: String,
            detail: String,
            queueLabel: String,
            queuePosition: Int?,
            progress: Double? = nil,
            waitStartedAt: Date? = nil
        ) {
            self.phase = phase
            self.headline = headline
            self.detail = detail
            self.queueLabel = queueLabel
            self.queuePosition = queuePosition
            self.progress = progress
            self.waitStartedAt = waitStartedAt
        }
    }

    let sessionId: String
    let gameTitle: String
    let storeName: String?
}
#endif

/// Applied at rendering time too, since restored activities decode independently of the app.
enum QueueActivityProgress {
    static func normalized(_ progress: Double?) -> Double? {
        guard let progress, progress.isFinite else { return nil }
        return min(max(progress, 0), 1)
    }

    static func barFraction(_ progress: Double?, phase: QueueActivityAttributes.ContentState.Phase) -> Double {
        let progress = normalized(progress)
        switch phase {
        case .ready: return 1
        case .waiting: return max(progress ?? 0, 0.75)
        case .queued: return progress.map { min(max($0, 0.03), 0.95) } ?? 0.06
        }
    }
}
