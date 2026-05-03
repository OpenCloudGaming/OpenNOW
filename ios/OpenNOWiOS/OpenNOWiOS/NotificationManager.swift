#if os(tvOS)
import Foundation

actor NotificationManager {
    static let shared = NotificationManager()

    func requestPermission() async {
    }

    func sendQueueReadyNotification(gameTitle: String) async {
    }

    func sendQueueSetupNotification(gameTitle: String) async {
    }

    func cancelSessionNotifications() {
    }
}
#else
import Foundation
import UserNotifications

actor NotificationManager {
    static let shared = NotificationManager()

    private let readyNotificationId = "com.opencloudgaming.opennow.sessionReady"
    private let setupNotificationId = "com.opencloudgaming.opennow.seatSetup"

    func requestPermission() async {
        _ = try? await UNUserNotificationCenter.current()
            .requestAuthorization(options: [.alert, .sound, .badge])
    }

    func sendQueueReadyNotification(gameTitle: String) async {
        let content = UNMutableNotificationContent()
        content.title = "Session Ready!"
        content.body = "\(gameTitle) is ready to stream. Tap to play."
        content.sound = .default
        content.interruptionLevel = .timeSensitive
        let request = UNNotificationRequest(identifier: readyNotificationId, content: content, trigger: nil)
        await withCheckedContinuation { continuation in
            UNUserNotificationCenter.current().add(request) { _ in
                continuation.resume()
            }
        }
    }

    func sendQueueSetupNotification(gameTitle: String) async {
        let content = UNMutableNotificationContent()
        content.title = "Seat Allocated!"
        content.body = "Setting up your \(gameTitle) session..."
        content.sound = .default
        let request = UNNotificationRequest(identifier: setupNotificationId, content: content, trigger: nil)
        await withCheckedContinuation { continuation in
            UNUserNotificationCenter.current().add(request) { _ in
                continuation.resume()
            }
        }
    }

    func cancelSessionNotifications() {
        UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: [readyNotificationId, setupNotificationId])
        UNUserNotificationCenter.current().removePendingNotificationRequests(withIdentifiers: [readyNotificationId, setupNotificationId])
    }
}
#endif
