import SwiftUI

@main
struct OpenNOWiOSApp: App {
    @StateObject private var store = OpenNOWStore()

    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(store)
        }
    }
}
