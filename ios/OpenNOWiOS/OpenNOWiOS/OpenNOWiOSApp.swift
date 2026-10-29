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

    func application(_ application: UIApplication,
                     supportedInterfaceOrientationsFor window: UIWindow?) -> UIInterfaceOrientationMask {
        StreamOrientation.supportedOrientations
    }

}

#if os(iOS)
@MainActor
enum StreamOrientation {
    private static var previousOrientation: UIInterfaceOrientationMask?
    private(set) static var isStreaming = false

    static var supportedOrientations: UIInterfaceOrientationMask {
        isStreaming ? .landscape : (UIDevice.current.userInterfaceIdiom == .pad ? .all : .allButUpsideDown)
    }

    static var currentOrientation: UIInterfaceOrientationMask {
        let orientation = UIApplication.shared.connectedScenes
            .compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState == .foregroundActive })?.interfaceOrientation
        switch orientation {
        case .landscapeLeft: return .landscapeLeft
        case .landscapeRight: return .landscapeRight
        case .portraitUpsideDown: return .portraitUpsideDown
        default: return .portrait
        }
    }

    static func setStreaming(_ streaming: Bool, restoring orientation: UIInterfaceOrientationMask? = nil) {
        isStreaming = streaming
        let restoredOrientation = previousOrientation ?? .portrait
        if !streaming { previousOrientation = nil }
        guard let scene = UIApplication.shared.connectedScenes
            .compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState == .foregroundActive }) else { return }
        if streaming && previousOrientation == nil {
            previousOrientation = orientation ?? currentOrientation
        }
        let orientations: UIInterfaceOrientationMask = streaming ? .landscape : restoredOrientation
        for window in scene.windows {
            var controller = window.rootViewController
            while let current = controller {
                current.setNeedsUpdateOfSupportedInterfaceOrientations()
                controller = current.presentedViewController
            }
        }
        scene.requestGeometryUpdate(.iOS(interfaceOrientations: orientations)) { _ in }
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
