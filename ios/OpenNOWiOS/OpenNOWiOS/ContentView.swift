import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var store: OpenNOWStore

    var body: some View {
        Group {
            if store.isBootstrapping {
                SplashView()
            } else if store.user == nil {
                LoginView()
            } else {
                MainTabView()
            }
        }
        .animation(.easeInOut(duration: 0.35), value: store.isBootstrapping)
        .animation(.easeInOut(duration: 0.35), value: store.user == nil)
        .task {
            await store.bootstrap()
        }
    }
}

// MARK: - Splash View (shown during bootstrap)

private struct SplashView: View {
    var body: some View {
        ZStack {
            appBackground
            VStack(spacing: 16) {
                Image(systemName: "bolt.fill")
                    .font(.system(size: 56, weight: .bold))
                    .foregroundStyle(brandGradient)
                Text("OpenNOW")
                    .font(.largeTitle.bold())
                ProgressView()
                    .padding(.top, 8)
            }
        }
        .ignoresSafeArea()
    }
}

// MARK: - Main Tab View

struct MainTabView: View {
    @EnvironmentObject private var store: OpenNOWStore

    var body: some View {
        TabView {
            HomeView()
                .tabItem { Label("Home", systemImage: "house.fill") }
            BrowseView()
                .tabItem { Label("Browse", systemImage: "square.grid.2x2.fill") }
            LibraryView()
                .tabItem { Label("Library", systemImage: "books.vertical.fill") }
            SessionView()
                .tabItem { Label("Session", systemImage: "dot.radiowaves.left.and.right") }
            SettingsView()
                .tabItem { Label("Settings", systemImage: "slider.horizontal.3") }
        }
        .tint(brandAccent)
        .fullScreenCover(isPresented: Binding(
            get: { store.showStreamLoading },
            set: { if !$0 { Task { await store.endSession() } } }
        )) {
            StreamLoadingView()
                .environmentObject(store)
        }
    }
}

// MARK: - Design Tokens (shared across all views in this file)

let brandAccent = Color(red: 0.46, green: 0.72, blue: 0.0)

let brandGradient = LinearGradient(
    colors: [Color(red: 0.46, green: 0.72, blue: 0.0), Color(red: 0.0, green: 0.72, blue: 0.55)],
    startPoint: .topLeading,
    endPoint: .bottomTrailing
)

var appBackground: some View {
    ZStack {
        Color(.systemBackground)
    }
    .ignoresSafeArea()
}

// MARK: - Preview

#Preview {
    ContentView()
        .environmentObject(OpenNOWStore())
}
