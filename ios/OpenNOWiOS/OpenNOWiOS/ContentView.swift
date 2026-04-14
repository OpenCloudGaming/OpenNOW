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

    private var isPad: Bool {
        UIDevice.current.userInterfaceIdiom == .pad
    }
    
    private var queuePillAlignment: Alignment {
        isPad ? .bottom : .top
    }
    
    private var queuePillAlignmentEdge: Edge {
        isPad ? .bottom : .top
    }

    private var queuePillPaddingEdge: Edge.Set {
        isPad ? .bottom : .top
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
                        .transition(
                            .asymmetric(
                                insertion: .scale(scale: 0.96).combined(with: .opacity),
                                removal: .move(edge: .bottom).combined(with: .opacity)
                            )
                        )
                }
            }
        }
        .animation(.spring(response: 0.42, dampingFraction: 0.86), value: store.queueOverlayVisible)
        .overlay(alignment: queuePillAlignment) {
            if store.showStreamLoading && !store.queueOverlayVisible {
                QueueStatusPill()
                    .environmentObject(store)
                    .padding(queuePillPaddingEdge, 8)
                    .transition(.move(edge: queuePillAlignmentEdge).combined(with: .opacity))
            }
        }
        .overlay {
            if let session = presentedStreamerSession {
                StreamerView(
                    session: session,
                    settings: store.settings,
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
        .animation(.spring(response: 0.36, dampingFraction: 0.88), value: store.showStreamLoading && !store.queueOverlayVisible)
        .animation(.easeInOut(duration: 0.2), value: presentedStreamerSession?.id)
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
        HStack(spacing: 10) {
            Button {
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
                .padding(.horizontal, 14)
                .padding(.vertical, 11)
            }
            .buttonStyle(.plain)

            Divider()
                .frame(height: 20)

            Button(role: .destructive) {
                Task { await store.endSession() }
            } label: {
                Image(systemName: "stop.fill")
                    .font(.caption.bold())
                    .frame(width: 28, height: 28)
                    .foregroundStyle(.red)
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 10)
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
