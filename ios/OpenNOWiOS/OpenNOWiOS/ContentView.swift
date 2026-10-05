import SwiftUI

#if canImport(UIKit)
import UIKit
#endif

struct ContentView: View {
    @EnvironmentObject private var store: OpenNOWStore

    var body: some View {
        Group {
            #if DEBUG
            if let queuePosition = debugQueuePreviewPosition {
                StreamLoadingView(coversBottomBar: true)
                    .task {
                        store.installDebugQueuePreview(position: queuePosition)
                    }
            } else {
                standardContent
            }
            #else
            standardContent
            #endif
        }
        .animation(.easeInOut(duration: 0.35), value: store.isBootstrapping)
        .animation(.easeInOut(duration: 0.35), value: store.user == nil)
        .task {
            #if DEBUG
            guard debugQueuePreviewPosition == nil else { return }
            #endif
            await store.bootstrap()
        }
    }

    @ViewBuilder
    private var standardContent: some View {
        Group {
            if store.isBootstrapping {
                SplashView()
            } else if store.user == nil {
                LoginView()
            } else {
                MainTabView(initialPage: store.settings.launchPage)
            }
        }
    }

    #if DEBUG
    private var debugQueuePreviewPosition: Int? {
        ProcessInfo.processInfo.arguments
            .first(where: { $0.hasPrefix("--opennow-queue-preview=") })
            .flatMap { Int($0.split(separator: "=", maxSplits: 1).last ?? "") }
            .map { max(1, $0) }
    }
    #endif
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
    private enum Tab: String, CaseIterable, Hashable {
        case home
        case browse
        case library
        case settings

        var title: String {
            switch self {
            case .home: return "Home"
            case .browse: return "Browse"
            case .library: return "Library"
            case .settings: return "Settings"
            }
        }

        var symbol: String {
            switch self {
            case .home: return "house.fill"
            case .browse: return "square.grid.2x2.fill"
            case .library: return "books.vertical.fill"
            case .settings: return "slider.horizontal.3"
            }
        }
    }

    @EnvironmentObject private var store: OpenNOWStore
    @StateObject private var catalogControllerShortcuts = CatalogControllerShortcutCoordinator()
    @State private var selectedTab: Tab
    @State private var streamerAutoRetryCount = 0
    @State private var presentedStreamerSession: ActiveSession?
    @State private var bugReportDeck: BugReportPreflightDeck?
    @State private var sidebarVisibility: NavigationSplitViewVisibility = .all
    @Environment(\.horizontalSizeClass) private var horizontalSizeClass
    private static let maxStreamerAutoRetries = 3

    init(initialPage: AppLaunchPage) {
        let initialTab: Tab
        switch initialPage {
        case .store:
            initialTab = .home
        case .library:
            initialTab = .library
        }
        _selectedTab = State(initialValue: initialTab)
    }

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
            synchronizeCatalogControllerShortcuts()
            // MainTabView can be recreated by upstream auth/bootstrap state updates.
            // Reattach streamer overlay if store already has an active stream session.
            if let activeStream = store.streamSession {
                catalogControllerShortcuts.setEnabled(false)
                Self.dismissFocusedInput()
                presentedStreamerSession = activeStream
            }
        }
        .onChangeCompat(of: store.streamSession) { newValue in
            if let newValue {
                catalogControllerShortcuts.setEnabled(false)
                Self.dismissFocusedInput()
                presentedStreamerSession = newValue
            } else if store.activeSession == nil {
                // Session fully ended; allow the cover to close.
                presentedStreamerSession = nil
            }
        }
        .onChangeCompat(of: store.activeSession?.id) { newId in
            streamerAutoRetryCount = 0
            if newId == nil {
                presentedStreamerSession = nil
            }
        }
        .onChangeCompat(of: presentedStreamerSession?.id) { newValue in
            if newValue != nil {
                Self.dismissFocusedInput()
            }
            synchronizeCatalogControllerShortcuts()
        }
        .onChangeCompat(of: selectedTab) { _ in
            synchronizeCatalogControllerShortcuts()
        }
        .onChangeCompat(of: store.queueOverlayVisible) { _ in
            synchronizeCatalogControllerShortcuts()
        }
        .onChangeCompat(of: store.pendingSettingsRoute) { route in
            // Switching the tab is this view's job; pushing to the right page inside Settings is
            // SettingsView's. It clears the request once it has consumed it.
            guard route != nil else { return }
            selectedTab = .settings
        }
        .onDisappear {
            catalogControllerShortcuts.setEnabled(false)
        }
    }

    private var tabSurface: some View {
        // A tab bar is the iPhone idiom and a sidebar is the iPad one. Splitting on size class
        // rather than device also covers Slide Over and Stage Manager, where an iPad is compact.
        Group {
            if horizontalSizeClass == .regular {
                sidebarSurface
            } else {
                tabBarSurface
            }
        }
        .environmentObject(catalogControllerShortcuts)
        .overlay {
            ZStack {
                if store.queueOverlayVisible {
                    StreamLoadingView(coversBottomBar: true)
                        .environmentObject(store)
                        .ignoresSafeArea()
                        .zIndex(1000)
                        .transition(queueOverlayTransition)
                }
            }
        }
        .animation(queueSurfaceAnimation, value: store.queueOverlayVisible)
        .safeAreaInset(edge: .top) {
            if horizontalSizeClass != .regular, store.canJumpBackToSession, !store.queueOverlayVisible {
                JumpBackStatusBanner()
                    .environmentObject(store)
                    .padding(.horizontal)
                    .padding(.top, 6)
                    .padding(.bottom, 4)
                    .transition(.move(edge: .top).combined(with: .opacity))
            }
        }
        // The report is presented over the tab surface rather than the streamer, so it survives
        // the streamer being torn down and never competes with the video layer for the screen.
        // `Text(verbatim:)` because the title is composed from a game name at runtime. Passing a
        // dynamic string as a LocalizedStringKey puts an empty key in the string catalog and
        // gives translators nothing to work with.
        .confirmationDialog(
            Text(verbatim: store.pendingLaunchConflict?.title ?? ""),
            isPresented: Binding(
                get: { store.pendingLaunchConflict != nil },
                set: { if !$0 { store.cancelPendingLaunch() } }
            ),
            titleVisibility: .visible,
            presenting: store.pendingLaunchConflict
        ) { conflict in
            Button("End and Play \(conflict.request.game.title)", role: .destructive) {
                Haptics.medium()
                store.confirmPendingLaunch()
            }
            Button("Keep Playing \(conflict.runningGame.title)", role: .cancel) {
                store.cancelPendingLaunch()
            }
        } message: { conflict in
            Text(conflict.message)
        }
        .sheet(item: Binding(
            get: { store.sessionReport },
            set: { if $0 == nil { store.dismissSessionReport() } }
        )) { report in
            SessionReportView(
                report: report,
                onReportProblem: {
                    // Capture the deck before the report sheet closes: it reads the session the
                    // user is about to complain about, which is gone a moment later.
                    bugReportDeck = store.bugReportPreflightDeck()
                    store.dismissSessionReport()
                },
                onDismiss: { disableFutureReports in
                    store.dismissSessionReport(disableFutureReports: disableFutureReports)
                }
            )
            .environmentObject(store)
            .presentationDetents([.medium, .large])
            .presentationDragIndicator(.visible)
        }
        .sheet(item: $bugReportDeck) { deck in
            BugReportView(deck: deck) { draft in
                await store.submitBugReport(draft, deck: deck)
            }
            .environmentObject(store)
        }
        // Two sheets cannot be presented from one view, so consent waits until nothing else is
        // on screen. It is a one-time prompt; deferring it a launch costs nothing.
        .sheet(isPresented: Binding(
            get: { showAnalyticsConsent },
            set: { if !$0 { store.recordAnalyticsConsent(sharing: false) } }
        )) {
            AnalyticsConsentView { sharing in
                store.recordAnalyticsConsent(sharing: sharing)
            }
        }
    }

    /// Only after sign-in, and only when the screen is otherwise clear. Asking during a queue or
    /// over a session report is asking at the worst possible moment.
    private var showAnalyticsConsent: Bool {
        store.settings.analyticsConsent == .notAsked
            && store.sessionReport == nil
            && store.pendingLaunchConflict == nil
            && !store.queueOverlayVisible
            && presentedStreamerSession == nil
    }

    private var tabBarSurface: some View {
        TabView(selection: $selectedTab) {
            ForEach(Tab.allCases, id: \.self) { tab in
                destination(for: tab)
                    .tabItem { Label(tab.title, systemImage: tab.symbol) }
                    .tag(tab)
            }
        }
        .tint(brandAccent)
    }

    private var sidebarSurface: some View {
        NavigationSplitView(columnVisibility: $sidebarVisibility) {
            // iOS sidebars take an optional selection; a nil write (deselect) is ignored so the
            // detail column can never end up showing nothing.
            List(selection: Binding<Tab?>(
                get: { selectedTab },
                set: { newValue in if let newValue { selectedTab = newValue } }
            )) {
                Section {
                    ForEach(Tab.allCases, id: \.self) { tab in
                        Label(tab.title, systemImage: tab.symbol).tag(tab)
                    }
                }

                // On iPhone the running session lives in a floating banner. Here there is a
                // permanent place for it, which is better: it never covers content and it does
                // not have to compete with the top safe area.
                if store.canJumpBackToSession {
                    Section("Session") {
                        JumpBackStatusBanner()
                            .environmentObject(store)
                            .listRowInsets(EdgeInsets(top: 6, leading: 8, bottom: 6, trailing: 8))
                    }
                }
            }
            .listStyle(.sidebar)
            .navigationTitle("OpenNOW")
        } detail: {
            destination(for: selectedTab)
        }
        .navigationSplitViewStyle(.balanced)
        .tint(brandAccent)
    }

    @ViewBuilder
    private func destination(for tab: Tab) -> some View {
        switch tab {
        case .home: HomeView()
        case .browse: BrowseView()
        case .library: LibraryView()
        case .settings: SettingsView()
        }
    }

    private func streamerSurface(session: ActiveSession) -> some View {
        NativeStreamPresentation {
        StreamerView(
            session: session,
            settings: store.currentStreamerSettings,
            membershipTier: store.subscription?.membershipTier ?? store.user?.membershipTier,
            onTouchLayoutChange: { profile, layout in
                store.updateTouchControlLayout(layout, profile: profile)
            },
            onStreamerPreferencesChange: { preferences in
                store.updateStreamerPreferences(preferences)
            },
            onStreamSharpeningChange: { enabled, amount in
                store.updateStreamSharpening(enabled: enabled, amount: amount)
            },
            onFingerMouseEnabledChange: { enabled in
                store.updateFingerMouseEnabled(enabled)
            },
            onPhoneRumbleFallbackChange: { enabled in
                store.updatePhoneRumbleFallback(enabled)
            },
            onStreamTutorialCompleted: {
                store.setStreamTutorialCompleted(true)
            },
            onControllerTouchPromptDismissed: {
                store.setControllerTouchPromptDismissed(true)
            },
            onStatsOverlayChange: { visible in
                store.updateStreamStatsOverlayVisible(visible)
            },
            onTransportStable: {
                streamerAutoRetryCount = 0
            },
            onSelectedVideoProfileRetry: { reason in
                store.recordStreamRecovery(reason: reason)
            },
            onNativeFallbackRequiresFreshEndpoint: { _ in },
            onRuntimeSample: { sample in
                store.recordStreamRuntimeSample(sample)
            },
            onSettingsChange: { updated in
                store.applyStreamerSettings(updated)
            },
            onBuildBugReportDeck: {
                store.bugReportPreflightDeck()
            },
            onSubmitBugReport: { draft, deck in
                await store.submitBugReport(draft, deck: deck)
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
        .environmentObject(store)
        .openNowTheme(store.settings)
        }
        .ignoresSafeArea()
        .id(session.id)
        .zIndex(3000)
        .onAppear {
            catalogControllerShortcuts.setEnabled(false)
        }
    }

    private func synchronizeCatalogControllerShortcuts() {
        let catalogTabSelected = selectedTab == .home || selectedTab == .browse || selectedTab == .library
        catalogControllerShortcuts.setEnabled(
            catalogTabSelected && presentedStreamerSession == nil && !store.queueOverlayVisible
        )
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

/// The default accent, and the fallback for views that have not yet been migrated to
/// `@Environment(\.openNowAccent)`. Now matches the Android build's default (`#6AF0A0`); the
/// previous olive is still available to users as the "Classic" accent in Settings → Interface.
///
/// New code should read the environment instead, so the user's choice is honoured. The root
/// applies `.tint()` from the same value, which already carries most system controls.
let brandAccent = UIAccent.openNow.color

/// Gradient built from the accent so it tracks the user's choice where it is used dynamically.
func brandGradient(for accent: UIAccent) -> LinearGradient {
    LinearGradient(
        colors: [accent.color, accent.color.opacity(0.55)],
        startPoint: .topLeading,
        endPoint: .bottomTrailing
    )
}

let brandGradient = LinearGradient(
    colors: [UIAccent.openNow.color, Color(hex: 0x00B78C)],
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
