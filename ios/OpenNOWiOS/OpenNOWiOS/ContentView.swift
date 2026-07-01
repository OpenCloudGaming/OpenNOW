import SwiftUI

#if canImport(UIKit)
import UIKit
#endif

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
    @State private var nativeStreamerBypassSessionIds = Set<String>()
    private static let maxStreamerAutoRetries = 3

    private var queueSurfaceAnimation: Animation {
        .spring(response: 0.42, dampingFraction: 0.86)
    }

    var body: some View {
        ZStack {
            if presentedStreamerSession == nil {
                tabSurface
                    .transition(.opacity)
            }

            if let session = presentedStreamerSession {
                streamerSurface(session: session)
                    .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.28), value: store.showStreamLoading && !store.queueOverlayVisible)
        .animation(.easeInOut(duration: 0.2), value: presentedStreamerSession?.id)
        .onAppear {
            // MainTabView can be recreated by upstream auth/bootstrap state updates.
            // Reattach streamer overlay if store already has an active stream session.
            if let activeStream = store.streamSession {
                Self.dismissFocusedInput()
                presentedStreamerSession = activeStream
            }
        }
        .onChangeCompat(of: store.streamSession) { newValue in
            if let newValue {
                Self.dismissFocusedInput()
                presentedStreamerSession = newValue
            } else if store.activeSession == nil {
                // Session fully ended; allow the cover to close.
                presentedStreamerSession = nil
            }
        }
        .onChangeCompat(of: store.activeSession?.id) { newId in
            streamerAutoRetryCount = 0
            if let newId {
                nativeStreamerBypassSessionIds = Set(nativeStreamerBypassSessionIds.filter { $0 == newId })
            } else {
                nativeStreamerBypassSessionIds.removeAll()
                presentedStreamerSession = nil
            }
        }
        .onChangeCompat(of: presentedStreamerSession?.id) { newValue in
            if newValue != nil {
                Self.dismissFocusedInput()
            }
        }
    }

    private var tabSurface: some View {
        TabView {
            tabContent {
                HomeView()
            }
                .tabItem { Label("Home", systemImage: "house.fill") }
            tabContent {
                BrowseView()
            }
                .tabItem { Label("Browse", systemImage: "square.grid.2x2.fill") }
            tabContent {
                LibraryView()
            }
                .tabItem { Label("Library", systemImage: "books.vertical.fill") }
            tabContent {
                SettingsView()
            }
                .tabItem { Label("Settings", systemImage: "slider.horizontal.3") }
        }
        .tint(brandAccent)
        .safeAreaInset(edge: .top) {
            if store.canJumpBackToSession && !store.queueOverlayVisible {
                JumpBackStatusBanner()
                    .environmentObject(store)
                    .padding(.horizontal)
                    .padding(.top, 6)
                    .padding(.bottom, 4)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
    }

    private func tabContent<Content: View>(@ViewBuilder _ content: () -> Content) -> some View {
        content()
            .overlay {
                if store.queueOverlayVisible {
                    StreamLoadingView()
                        .environmentObject(store)
                        .ignoresSafeArea(edges: [.top, .leading, .trailing])
                        .zIndex(1000)
                        .transition(queueOverlayTransition)
                }
            }
            .animation(queueSurfaceAnimation, value: store.queueOverlayVisible)
    }

    private func streamerSurface(session: ActiveSession) -> some View {
        StreamerView(
            session: session,
            settings: store.settings,
            nativeStreamerEnabled: !nativeStreamerBypassSessionIds.contains(session.id),
            onTouchLayoutChange: { profile, layout in
                store.updateTouchControlLayout(layout, profile: profile)
            },
            onStreamerPreferencesChange: { preferences in
                store.updateStreamerPreferences(preferences)
            },
            onNativeFallbackRequiresFreshEndpoint: { _ in
                nativeStreamerBypassSessionIds.insert(session.id)
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
    }

    private var queueOverlayTransition: AnyTransition {
        return .asymmetric(
            insertion: .opacity,
            removal: .opacity
        )
    }

    private static func dismissFocusedInput() {
        #if canImport(UIKit)
        UIApplication.shared.sendAction(
            #selector(UIResponder.resignFirstResponder),
            to: nil,
            from: nil,
            for: nil
        )
        #endif
    }

}

struct OpenNOWUnavailableView<Description: View, Actions: View>: View {
    private let title: String
    private let systemImage: String
    private let description: Description
    private let actions: Actions

    init(
        _ title: String,
        systemImage: String,
        @ViewBuilder description: () -> Description,
        @ViewBuilder actions: () -> Actions
    ) {
        self.title = title
        self.systemImage = systemImage
        self.description = description()
        self.actions = actions()
    }

    var body: some View {
        if #available(iOS 17.0, tvOS 17.0, macOS 14.0, *) {
            ContentUnavailableView {
                Label(title, systemImage: systemImage)
            } description: {
                description
            } actions: {
                actions
            }
        } else {
            VStack(spacing: 12) {
                Image(systemName: systemImage)
                    .font(.system(size: 42, weight: .semibold))
                    .foregroundStyle(.secondary)

                Text(title)
                    .font(.headline)
                    .multilineTextAlignment(.center)

                description
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)

                actions
                    .padding(.top, 2)
            }
            .frame(maxWidth: .infinity, alignment: .center)
            .padding(.horizontal, 20)
            .padding(.vertical, 28)
        }
    }
}

extension OpenNOWUnavailableView where Description == EmptyView, Actions == EmptyView {
    init(_ title: String, systemImage: String) {
        self.init(title, systemImage: systemImage) {
            EmptyView()
        } actions: {
            EmptyView()
        }
    }
}

extension OpenNOWUnavailableView where Actions == EmptyView {
    init(
        _ title: String,
        systemImage: String,
        @ViewBuilder description: () -> Description
    ) {
        self.init(title, systemImage: systemImage, description: description) {
            EmptyView()
        }
    }
}

extension View {
    @ViewBuilder
    func onChangeCompat<Value: Equatable>(
        of value: Value,
        perform action: @escaping (Value) -> Void
    ) -> some View {
        if #available(iOS 17.0, tvOS 17.0, macOS 14.0, *) {
            self.onChange(of: value) { _, newValue in
                action(newValue)
            }
        } else {
            self.onChange(of: value, perform: action)
        }
    }

    @ViewBuilder
    func searchableCompat(
        text: Binding<String>,
        isPresented: Binding<Bool>,
        placement: SearchFieldPlacement,
        prompt: String
    ) -> some View {
        if #available(iOS 17.0, tvOS 17.0, macOS 14.0, *) {
            self.searchable(
                text: text,
                isPresented: isPresented,
                placement: placement,
                prompt: Text(prompt)
            )
        } else {
            self.searchable(
                text: text,
                placement: placement,
                prompt: Text(prompt)
            )
        }
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
