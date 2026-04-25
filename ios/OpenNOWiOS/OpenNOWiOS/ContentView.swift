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
    @AppStorage("queuePillVerticalEdge") private var queuePillVerticalEdgeRaw = ""
    @State private var streamerAutoRetryCount = 0
    @State private var presentedStreamerSession: ActiveSession?
    private static let maxStreamerAutoRetries = 3

    private enum QueuePillVerticalEdge: String {
        case top
        case bottom

        var alignment: Alignment {
            switch self {
            case .top: return .top
            case .bottom: return .bottom
            }
        }

        var transitionEdge: Edge {
            switch self {
            case .top: return .top
            case .bottom: return .bottom
            }
        }
    }

    private var queuePillEdge: QueuePillVerticalEdge {
        if let stored = QueuePillVerticalEdge(rawValue: queuePillVerticalEdgeRaw) {
            return stored
        }
        #if os(iOS)
        return UIDevice.current.userInterfaceIdiom == .pad ? .bottom : .top
        #else
        return .top
        #endif
    }

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
        .overlay {
            queuePillOverlay
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

    @ViewBuilder
    private var queuePillOverlay: some View {
        if store.showStreamLoading && !store.queueOverlayVisible {
            GeometryReader { proxy in
                VStack {
                    if queuePillEdge == .bottom {
                        Spacer(minLength: 0)
                    }

                    QueueStatusPill(edge: queuePillEdge.transitionEdge)
                        .environmentObject(store)
                        .padding(.top, queuePillEdge == .top ? 8 : 0)
                        .padding(.bottom, queuePillEdge == .bottom ? bottomQueuePillPadding(in: proxy) : 0)
                        .queuePillDrag(
                            edgeRawValue: $queuePillVerticalEdgeRaw,
                            proxy: proxy,
                            animation: queueSurfaceAnimation
                        )
                        .transition(
                            .asymmetric(
                                insertion: .move(edge: queuePillEdge.transitionEdge).combined(with: .opacity),
                                removal: .scale(scale: 0.88, anchor: queuePillEdge == .top ? .top : .bottom).combined(with: .opacity)
                            )
                        )
                        .accessibilityHint("Drag up or down to latch the queue pill to the top or bottom.")

                    if queuePillEdge == .top {
                        Spacer(minLength: 0)
                    }
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            }
            .allowsHitTesting(true)
            .animation(queueSurfaceAnimation, value: queuePillEdge.rawValue)
        }
    }

    private func bottomQueuePillPadding(in proxy: GeometryProxy) -> CGFloat {
        #if os(iOS)
        if UIDevice.current.userInterfaceIdiom == .phone {
            return max(proxy.safeAreaInsets.bottom + 52, 68)
        }
        return max(proxy.safeAreaInsets.bottom + 18, 28)
        #else
        return 12
        #endif
    }
}

private struct QueueStatusPill: View {
    @EnvironmentObject private var store: OpenNOWStore
    let edge: Edge
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
            guard store.supportsEmbeddedStreamer else { return "Ready on another platform" }
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
                    Image(systemName: edge == .top ? "chevron.up" : "chevron.down")
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
        .numericQueueTransition(value: store.activeSession?.queuePosition ?? -1)
        .animation(.spring(response: 0.34, dampingFraction: 0.8), value: subtitle)
        .animation(.spring(response: 0.34, dampingFraction: 0.8), value: store.activeSession?.status)
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

private struct QueuePillDragModifier: ViewModifier {
    @Binding var edgeRawValue: String
    let proxy: GeometryProxy
    let animation: Animation
    @State private var dragOffset: CGFloat = 0
    @State private var latchedDuringDrag = false

    func body(content: Content) -> some View {
        #if os(iOS)
        content
            .offset(y: dragOffset)
            .highPriorityGesture(
                DragGesture(minimumDistance: 3, coordinateSpace: .global)
                    .onChanged { value in
                        guard !latchedDuringDrag else { return }

                        let currentEdge = edgeRawValue.isEmpty ? "top" : edgeRawValue
                        let snapDistance = min(max(proxy.size.height * 0.12, 68), 118)
                        let translation = value.translation.height
                        let nextEdge: String?
                        if currentEdge == "bottom", translation < -snapDistance {
                            nextEdge = "top"
                        } else if currentEdge != "bottom", translation > snapDistance {
                            nextEdge = "bottom"
                        } else {
                            nextEdge = nil
                        }

                        if let nextEdge {
                            latchedDuringDrag = true
                            withAnimation(animation) {
                                edgeRawValue = nextEdge
                                dragOffset = 0
                            }
                        } else {
                            var transaction = Transaction()
                            transaction.animation = nil
                            withTransaction(transaction) {
                                dragOffset = translation
                            }
                        }
                    }
                    .onEnded { value in
                        defer {
                            latchedDuringDrag = false
                        }

                        guard !latchedDuringDrag else {
                            withAnimation(animation) {
                                dragOffset = 0
                            }
                            return
                        }

                        let currentEdge = edgeRawValue.isEmpty ? "top" : edgeRawValue
                        let projectedTranslation = value.translation.height + (value.predictedEndTranslation.height * 0.18)
                        let snapDistance = min(max(proxy.size.height * 0.12, 68), 118)
                        let nextEdge: String?
                        if currentEdge == "bottom", projectedTranslation < -snapDistance {
                            nextEdge = "top"
                        } else if currentEdge != "bottom", projectedTranslation > snapDistance {
                            nextEdge = "bottom"
                        } else {
                            nextEdge = nil
                        }
                        withAnimation(animation) {
                            if let nextEdge {
                                edgeRawValue = nextEdge
                            }
                            dragOffset = 0
                        }
                    }
            )
        #else
        content
        #endif
    }
}

extension View {
    func queuePillBackground() -> some View {
        modifier(QueuePillBackgroundModifier())
    }

    @ViewBuilder
    func numericQueueTransition(value: Int) -> some View {
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
    func queuePillDrag(
        edgeRawValue: Binding<String>,
        proxy: GeometryProxy,
        animation: Animation
    ) -> some View {
        modifier(QueuePillDragModifier(edgeRawValue: edgeRawValue, proxy: proxy, animation: animation))
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
