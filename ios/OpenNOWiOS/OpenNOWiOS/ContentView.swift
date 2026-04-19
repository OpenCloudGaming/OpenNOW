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

    /// Keep the pill at the top on all devices so it does not cover tab content
    /// (e.g. Session “End Session” and scrollable bottom actions on iPad).
    private var queuePillAlignment: Alignment { .top }

    private var queuePillPaddingEdge: Edge.Set { .top }

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
                        .transition(.opacity)
                }
            }
        }
        .animation(.easeInOut(duration: 0.32), value: store.queueOverlayVisible)
        .overlay(alignment: queuePillAlignment) {
            if store.showStreamLoading && !store.queueOverlayVisible {
                QueueStatusPill()
                    .environmentObject(store)
                    .padding(queuePillPaddingEdge, 8)
                    .transition(.opacity)
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
        .onChange(of: store.activeSession?.id) { _ in
            streamerAutoRetryCount = 0
            if store.activeSession == nil {
                presentedStreamerSession = nil
            }
        }
    }
}

private struct QueueStatusPill: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var isPulsing = false

    private var statusColor: Color {
        switch store.activeSession?.status {
        case 3:
            return .green
        case 2:
            return Color(red: 0.84, green: 0.72, blue: 0.12)
        default:
            return .orange
        }
    }

    private var subtitle: String {
        guard let session = store.activeSession else { return "Preparing..." }
        switch session.status {
        case 3:
            return store.streamSession == nil ? "Tap to return" : "Streaming"
        case 2:
            return "Ready to connect"
        default:
            if let queue = session.queuePosition {
                return queue == 1 ? "Next in queue" : "Queue #\(queue)"
            }
            return "Queued"
        }
    }

    var body: some View {
        HStack(spacing: 0) {
            Button {
                Haptics.light()
                if store.canReopenStreamer {
                    store.reopenStreamer()
                } else {
                    store.maximizeQueueOverlay()
                }
            } label: {
                HStack(spacing: 10) {
                    Circle()
                        .fill(statusColor)
                        .frame(width: 10, height: 10)
                        .scaleEffect(isPulsing ? 1.2 : 0.9)
                        .opacity(isPulsing ? 1.0 : 0.7)
                    VStack(alignment: .leading, spacing: 1) {
                        Text(store.activeSession?.game.title ?? "Queue")
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
                .padding(.leading, 14)
                .padding(.trailing, 12)
                .padding(.vertical, 11)
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)

            Divider()
                .frame(height: 20)
                .padding(.vertical, 8)

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
            .buttonStyle(.plain)
        }
        .padding(.leading, 4)
        .padding(.trailing, 2)
        .queuePillBackground()
        .shadow(color: brandAccent.opacity(0.12), radius: 8, y: 2)
        .shadow(color: .black.opacity(0.2), radius: 12, y: 4)
        .padding(.horizontal, 16)
        .onAppear {
            withAnimation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true)) {
                isPulsing = true
            }
        }
    }
}

private struct QueuePillBackgroundModifier: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 26, *) {
            content
                .background(.regularMaterial, in: Capsule())
                .glassEffect(in: Capsule())
        } else {
            content
                .background(.regularMaterial, in: Capsule())
        }
    }
}

private extension View {
    func queuePillBackground() -> some View {
        modifier(QueuePillBackgroundModifier())
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
        Color(.systemBackground)
    }
    .ignoresSafeArea()
}

#Preview {
    ContentView()
        .environmentObject(OpenNOWStore())
}
