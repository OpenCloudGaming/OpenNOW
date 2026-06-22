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

private struct SplashView: View {
    var body: some View {
        ZStack {
            appBackground
            VStack(spacing: 16) {
                BrandLogoView(size: 88)
                Text("OpenNOW")
                    .font(.largeTitle.bold())
                ProgressView()
                    .padding(.top, 8)
            }
        }
        .ignoresSafeArea()
    }
}

struct MainTabView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var streamerAutoRetryCount = 0
    @State private var presentedStreamerSession: ActiveSession?
    private static let maxStreamerAutoRetries = 3

    private var queueSurfaceAnimation: Animation {
        .spring(response: 0.42, dampingFraction: 0.86)
    }

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
        .overlay {
            ZStack {
                if store.queueOverlayVisible {
                    StreamLoadingView()
                        .environmentObject(store)
                        .ignoresSafeArea()
                        .zIndex(1000)
                        .transition(queueOverlayTransition)
                }
            }
        }
        .animation(queueSurfaceAnimation, value: store.queueOverlayVisible)
        .safeAreaInset(edge: .top) {
            if store.canJumpBackToSession && !store.queueOverlayVisible && presentedStreamerSession == nil {
                JumpBackStatusBanner()
                    .environmentObject(store)
                    .padding(.horizontal)
                    .padding(.top, 6)
                    .padding(.bottom, 4)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        .overlay {
            if let session = presentedStreamerSession {
                StreamerView(
                    session: session,
                    settings: store.settings,
                    onTouchLayoutChange: { profile, layout in
                        store.updateTouchControlLayout(layout, profile: profile)
                    },
                    onStreamerPreferencesChange: { preferences in
                        store.updateStreamerPreferences(preferences)
                    },
                    onClose: {
                        presentedStreamerSession = nil
                        streamerAutoRetryCount = 0
                        store.dismissStreamer()
                    },
                    onRetry: streamerAutoRetryCount < Self.maxStreamerAutoRetries ? {
                        presentedStreamerSession = nil
                        streamerAutoRetryCount += 1
                        store.dismissStreamer()
                        store.scheduleStreamerReopen()
                    } : nil
                )
                .ignoresSafeArea()
                .zIndex(3000)
                .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.28), value: store.showStreamLoading && !store.queueOverlayVisible)
        .animation(.easeInOut(duration: 0.2), value: presentedStreamerSession?.id)
        .onAppear {
            // MainTabView can be recreated by upstream auth/bootstrap state updates.
            // Reattach streamer overlay if store already has an active stream session.
            if let activeStream = store.streamSession {
                presentedStreamerSession = activeStream
            }
        }
        .onChange(of: store.streamSession) { _, newValue in
            if let newValue {
                presentedStreamerSession = newValue
            } else if store.activeSession == nil {
                // Session fully ended; allow the cover to close.
                presentedStreamerSession = nil
            }
        }
        .onChange(of: store.activeSession?.id) { _, _ in
            streamerAutoRetryCount = 0
            if store.activeSession == nil {
                presentedStreamerSession = nil
            }
        }
    }

    private var queueOverlayTransition: AnyTransition {
        return .asymmetric(
            insertion: .opacity,
            removal: .opacity
        )
    }

}

private struct JumpBackStatusBanner: View {
    @EnvironmentObject private var store: OpenNOWStore

    private var statusColor: Color {
        switch currentStatus {
        case 3:
            return .green
        case 2:
            return Color(red: 0.84, green: 0.72, blue: 0.12)
        default:
            return .orange
        }
    }

    private var currentStatus: Int {
        store.activeSession?.status ?? store.primaryRemoteJumpBackSession?.status ?? 1
    }

    private var title: String {
        if let active = store.activeSession {
            return active.game.title
        }
        if let candidate = store.primaryRemoteJumpBackSession,
           let game = store.gameForRemoteSession(candidate) {
            return game.title
        }
        return "Cloud session"
    }

    private var subtitle: String {
        if let session = store.activeSession {
            return subtitle(for: session.status, queuePosition: session.queuePosition)
        }
        if let candidate = store.primaryRemoteJumpBackSession {
            return subtitle(for: candidate.status, queuePosition: nil)
        }
        return "Resume"
    }

    private func subtitle(for status: Int, queuePosition: Int?) -> String {
        switch status {
        case 3:
            guard store.supportsEmbeddedStreamer else { return "Ready on another platform" }
            return "Ready to return"
        case 2:
            return "Ready to connect"
        default:
            if let queue = queuePosition {
                return queue == 1 ? "Next in queue" : "Queue #\(queue)"
            }
            return "Queued"
        }
    }

    var body: some View {
        HStack(spacing: 12) {
            Button {
                Haptics.light()
                store.jumpBackToSession()
            } label: {
                HStack(spacing: 10) {
                    Circle()
                        .fill(statusColor)
                        .frame(width: 10, height: 10)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(title)
                            .font(.caption.bold())
                            .lineLimit(1)
                        Text(subtitle)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 0)
                    Image(systemName: "chevron.up")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            if store.activeSession != nil {
                Button(role: .destructive) {
                    Haptics.medium()
                    Task { await store.endSession() }
                } label: {
                    Image(systemName: "stop.fill")
                        .font(.caption.bold())
                        .frame(width: 44, height: 44)
                        .contentShape(Rectangle())
                        .foregroundStyle(.red)
                }
                .buttonStyle(.borderless)
            }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
        .numericTextTransition(value: store.activeSession?.queuePosition ?? -1)
        .animation(.spring(response: 0.34, dampingFraction: 0.8), value: subtitle)
        .animation(.spring(response: 0.34, dampingFraction: 0.8), value: store.activeSession?.status)
    }
}

extension View {
    @ViewBuilder
    func numericTextTransition(value: Int) -> some View {
        if #available(iOS 17, tvOS 17, *) {
            self
                .contentTransition(.numericText())
                .animation(.spring(response: 0.32, dampingFraction: 0.82), value: value)
        } else {
            self
                .animation(.spring(response: 0.32, dampingFraction: 0.82), value: value)
        }
    }

    @ViewBuilder
    func numericQueueTransition(value: Int) -> some View {
        numericTextTransition(value: value)
    }
}

let brandAccent = Color(red: 0.46, green: 0.72, blue: 0.0)

let brandGradient = LinearGradient(
    colors: [Color(red: 0.46, green: 0.72, blue: 0.0), Color(red: 0.0, green: 0.72, blue: 0.55)],
    startPoint: .topLeading,
    endPoint: .bottomTrailing
)

var appBackground: some View {
    ZStack {
        #if os(tvOS)
        Color.black
        #else
        Color(.systemBackground)
        #endif
    }
    .ignoresSafeArea()
}

#Preview {
    ContentView()
        .environmentObject(OpenNOWStore())
}
