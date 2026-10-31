#if canImport(ActivityKit)
import ActivityKit
import Foundation
import OSLog

@MainActor
final class QueueLiveActivityManager {
    static let shared = QueueLiveActivityManager()

    private let logger = Logger(subsystem: "OpenNOWiOS", category: "QueueLiveActivity")
    private var revision = UUID()

    private init() {}

    func sync(
        sessionId: String?,
        gameTitle: String?,
        storeName: String?,
        state: QueueActivityAttributes.ContentState?
    ) async {
        let requestedRevision = UUID()
        revision = requestedRevision
        guard ActivityAuthorizationInfo().areActivitiesEnabled else { return }

        let activities = Activity<QueueActivityAttributes>.activities
        guard let sessionId, let gameTitle, let state else {
            await endAll(activities: activities)
            return
        }

        let content = ActivityContent(
            state: state,
            staleDate: Date().addingTimeInterval(300)
        )

        if let existing = activities.first(where: { $0.attributes.sessionId == sessionId }) {
            await existing.update(content)
            guard revision == requestedRevision else { return }
            for activity in activities where activity.id != existing.id {
                await activity.end(nil, dismissalPolicy: .immediate)
            }
            return
        }

        await endAll(activities: activities)
        guard revision == requestedRevision else { return }

        do {
            _ = try Activity.request(
                attributes: QueueActivityAttributes(sessionId: sessionId, gameTitle: gameTitle, storeName: storeName),
                content: content
            )
        } catch {
            logger.error("Live Activity request failed error=\(error.localizedDescription, privacy: .public)")
        }
    }

    func endAll() async {
        revision = UUID()
        await endAll(activities: Activity<QueueActivityAttributes>.activities)
    }

    private func endAll(activities: [Activity<QueueActivityAttributes>]) async {
        for activity in activities {
            await activity.end(nil, dismissalPolicy: .immediate)
        }
    }
}
#else
import Foundation

@MainActor
final class QueueLiveActivityManager {
    static let shared = QueueLiveActivityManager()

    private init() {}

    func sync(
        sessionId: String?,
        gameTitle: String?,
        storeName: String?,
        state: QueueActivityAttributes.ContentState?
    ) async {
    }

    func endAll() async {
    }
}
#endif
