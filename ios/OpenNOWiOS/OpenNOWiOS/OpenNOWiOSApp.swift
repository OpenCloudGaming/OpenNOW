import SwiftUI
import UIKit

final class OpenNOWAppDelegate: NSObject, UIApplicationDelegate {
    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        OpenNOWImageCache.configureURLCache()
        Task { @MainActor in
            await QueueLiveActivityManager.shared.endAll()
        }
        return true
    }

    func applicationDidReceiveMemoryWarning(_ application: UIApplication) {
        OpenNOWImageCache.shared.removeAll()
    }

    func applicationWillTerminate(_ application: UIApplication) {
        Task { @MainActor in
            await QueueLiveActivityManager.shared.endAll()
        }
    }
}

@main
struct OpenNOWiOSApp: App {
    @UIApplicationDelegateAdaptor(OpenNOWAppDelegate.self) private var appDelegate
    @Environment(\.scenePhase) private var scenePhase
    @StateObject private var store = OpenNOWStore()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(store)
                .task {
                    store.handleScenePhase(scenePhase)
                }
                .onChange(of: scenePhase) { _, newPhase in
                    store.handleScenePhase(newPhase)
                }
        }
    }
}
