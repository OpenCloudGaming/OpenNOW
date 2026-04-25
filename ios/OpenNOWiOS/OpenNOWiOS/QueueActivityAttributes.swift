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
    }

    let sessionId: String
    let gameTitle: String
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
    }

    let sessionId: String
    let gameTitle: String
}
#endif
