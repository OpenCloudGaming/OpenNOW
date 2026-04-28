import SwiftUI

@main
struct OpenNOWMacApp: App {
    private let bridge = ONAppCoreBridge()

    var body: some Scene {
        WindowGroup {
            ContentView(bridge: bridge)
        }
    }
}
