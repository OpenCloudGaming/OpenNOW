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

#if os(iOS)
@MainActor
enum StreamOrientation {
    private static var previousOrientation: UIInterfaceOrientationMask?

    static func setStreaming(_ streaming: Bool) {
        guard let scene = UIApplication.shared.connectedScenes
            .compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState == .foregroundActive }) else { return }
        if streaming && previousOrientation == nil {
            switch scene.interfaceOrientation {
            case .landscapeLeft: previousOrientation = .landscapeLeft
            case .landscapeRight: previousOrientation = .landscapeRight
            case .portraitUpsideDown: previousOrientation = .portraitUpsideDown
            default: previousOrientation = .portrait
            }
        }
        let orientations: UIInterfaceOrientationMask = streaming ? .landscape : (previousOrientation ?? .portrait)
        scene.requestGeometryUpdate(.iOS(interfaceOrientations: orientations)) { _ in }
        if !streaming { previousOrientation = nil }
    }
}
#endif

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
