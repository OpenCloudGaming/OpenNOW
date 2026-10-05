import SwiftUI
import UIKit

final class OpenNOWAppDelegate: NSObject, UIApplicationDelegate {
    override init() {
        NativeStreamWebRTCPolicy.initialize()
        super.init()
    }

    func application(
        _ application: UIApplication,
        didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]? = nil
    ) -> Bool {
        OpenNOWImageCache.configureURLCache()
        if ProcessInfo.processInfo.arguments.contains("--opennow-streamer-self-test") {
            NativeStreamSelfTest.run()
        }
        return true
    }

    func applicationDidReceiveMemoryWarning(_ application: UIApplication) {
        OpenNOWImageCache.shared.removeAll()
    }

}

@main
struct OpenNOWiOSApp: App {
    @UIApplicationDelegateAdaptor(OpenNOWAppDelegate.self) private var appDelegate
    @Environment(\.scenePhase) private var scenePhase
    @StateObject private var store = OpenNOWStore()

    init() { NativeStreamWebRTCPolicy.initialize() }

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(store)
                // One place applies the whole theme: accent tint plus the two appearance
                // switches. Views read `\.openNowAccent` rather than a global constant so a
                // change in Settings propagates without a relaunch.
                .openNowTheme(store.settings)
                .task {
                    store.handleScenePhase(scenePhase)
                }
                .onChangeCompat(of: scenePhase) { newPhase in
                    store.handleScenePhase(newPhase)
                }
                .onOpenURL { url in
                    store.handleIncomingURL(url)
                }
        }
    }
}
