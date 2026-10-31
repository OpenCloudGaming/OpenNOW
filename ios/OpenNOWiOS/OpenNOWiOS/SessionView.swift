import SwiftUI

struct SessionView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var endingRemoteSessionId: String?
    @State private var pendingEnd: SessionEndTarget?
    @State private var isRefreshing = false

    var body: some View {
        NavigationStack {
            List {
                if let error = store.sessionError {
                    Section {
                        ErrorBannerView(message: error, onDismiss: { store.clearFailure() })
                    }
                }
                if let session = store.activeSession {
                    currentSessionSection(session)
                    streamProfileSection(session)
                    endSessionSection(session)
                } else {
                    Section {
                        OpenNOWUnavailableView("No Active Session", systemImage: "dot.radiowaves.left.and.right")
                        if store.canJumpBackToSession {
                            Button {
                                store.jumpBackToSession()
                            } label: {
                                Label("Check Cloud Session", systemImage: "arrow.clockwise.circle")
                            }
                        }
                    }
                }

                remoteSessionsSection
            }
            .listStyle(.insetGrouped)
            .navigationTitle("Session")
            .refreshable { await refreshSessions() }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        Task { await refreshSessions() }
                    } label: {
                        if isRefreshing {
                            ProgressView()
                        } else {
                            Image(systemName: "arrow.clockwise")
                        }
                    }
                    .accessibilityLabel("Refresh Sessions")
                    .disabled(isRefreshing || isEnding)
                }
            }
            .alert(
                pendingEnd?.title ?? "End Cloud Session?",
                isPresented: Binding(
                    get: { pendingEnd != nil },
                    set: { if !$0 { pendingEnd = nil } }
                ),
                presenting: pendingEnd
            ) { target in
                Button(target.actionTitle, role: .destructive) {
                    confirmEnd(target)
                }
                Button("Keep Session", role: .cancel) { pendingEnd = nil }
            } message: { target in
                Text(target.message)
            }
        }
    }

    private func currentSessionSection(_ session: ActiveSession) -> some View {
        let presentation = SessionScreenPresentation(session: session, availability: store.sessionAvailability, isStreaming: store.streamSession != nil)
        return Section {
            GameListRowView(
                game: session.game,
                subtitle: presentation.status,
                trailingSystemImage: presentation.symbol
            )

            // Ticks on its own, for the same reason as the Settings row: nothing should publish
            // a change through the store once a second just to move a clock.
            LabeledContent("Elapsed") {
                Text(session.startedAt, style: .timer)
                    .monospacedDigit()
            }
            if store.sessionAvailability == .available, let queue = session.queuePosition {
                LabeledContent("Queue", value: queue == 1 ? "Next" : "#\(queue)")
            }
            if let gpuType = session.gpuType, !gpuType.isEmpty {
                LabeledContent("GPU", value: gpuType)
            }

            Button {
                store.jumpBackToSession()
            } label: {
                Label(presentation.actionTitle, systemImage: presentation.symbol)
            }
            .disabled(presentation.isChecking)
        } header: {
            Text("Current")
        }
        .disabled(isEnding)
    }

    private func streamProfileSection(_ session: ActiveSession) -> some View {
        let profile = session.negotiatedStreamProfile
        let configured = StreamSettingsResolver.profile(
            for: store.settings,
            membershipTier: store.subscription?.membershipTier ?? store.user?.membershipTier
        )
        return Section("Stream Profile") {
            LabeledContent("Requested", value: "\(configured.width)x\(configured.height) @ \(configured.fps) fps")
            if let resolution = profile?.resolution {
                LabeledContent("Negotiated", value: profile?.fps.map { "\(resolution) @ \($0) fps" } ?? resolution)
            }
            if let colorQuality = profile?.colorQuality {
                LabeledContent("Color", value: colorQuality.label)
            } else {
                LabeledContent("Color", value: StreamSettingsResolver.colorQuality(for: store.settings).label)
            }
            LabeledContent("HDR", value: (session.finalizedStreamingFeatures?.trueHdr ?? store.settings.hdrEnabled) ? "On" : "Off")
            LabeledContent("L4S", value: (session.finalizedStreamingFeatures?.enabledL4S ?? store.settings.enableL4S) ? "On" : "Off")
        }
    }

    // A "Controls" section used to sit here with Microphone and Recording toggles. Neither had any
    // consumer anywhere in the app: flipping them changed one published boolean and nothing else.
    // The microphone one was worse than useless — GeForce NOW's offer carries no upstream audio
    // track, so it advertised a capability that cannot exist, and left the impression the app had
    // switched the microphone on when iOS showed its in-call indicator for an unrelated reason.

    private func endSessionSection(_ session: ActiveSession) -> some View {
        Section {
            Button(role: .destructive) {
                pendingEnd = .current(session)
            } label: {
                Label(session.status == 1 ? "Leave Queue" : "End Cloud Session", systemImage: "stop.circle")
            }
            .disabled(isEnding)
        } footer: {
            Text("This stops the cloud session, rather than just disconnecting this device.")
        }
    }

    private var remoteSessionsSection: some View {
        Section("Active on Account") {
            if accountSessions.isEmpty {
                Text(isRefreshing ? "Checking for sessions…" : "No other resumable sessions found.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(accountSessions) { candidate in
                    let game = store.gameForRemoteSession(candidate)
                    HStack(spacing: 12) {
                        if let game {
                            GameListRowView(game: game, subtitle: remoteStatus(candidate), trailingSystemImage: "arrow.clockwise.circle")
                        } else {
                            Label(remoteStatus(candidate), systemImage: "cloud")
                        }
                        Spacer(minLength: 8)
                        if endingRemoteSessionId == candidate.id {
                            ProgressView()
                        } else {
                            Menu {
                                Button {
                                    store.scheduleResume(candidate: candidate)
                                } label: {
                                    Label("Check Session", systemImage: "arrow.clockwise.circle")
                                }
                                Button(role: .destructive) {
                                    pendingEnd = .remote(candidate, title: game?.title ?? "Cloud Session")
                                } label: {
                                    Label(candidate.status == 1 ? "Leave Queue" : "End Cloud Session", systemImage: "stop.circle")
                                }
                            } label: {
                                Image(systemName: "ellipsis.circle")
                                    .minimumHitTarget()
                            }
                            .accessibilityLabel("Actions for \(game?.title ?? "cloud session")")
                            .disabled(isEnding)
                        }
                    }
                }
            }
        }
    }

    private var isEnding: Bool { store.isEndingSession || endingRemoteSessionId != nil }

    private var accountSessions: [RemoteSessionCandidate] {
        store.resumableSessions.filter { $0.id != store.activeSession?.id }
    }

    @MainActor
    private func refreshSessions() async {
        guard !isRefreshing, !isEnding else { return }
        isRefreshing = true
        defer { isRefreshing = false }
        await store.refreshRemoteSessions()
    }

    private func confirmEnd(_ target: SessionEndTarget) {
        pendingEnd = nil
        guard !isEnding, target.matches(activeSession: store.activeSession, remoteSessions: store.resumableSessions) else { return }
        switch target {
        case .current:
            Task { @MainActor in
                guard target.matches(activeSession: store.activeSession, remoteSessions: store.resumableSessions) else { return }
                await store.endSession()
            }
        case let .remote(candidate, _):
            endingRemoteSessionId = candidate.id
            Task { @MainActor in
                defer { endingRemoteSessionId = nil }
                guard target.matches(activeSession: store.activeSession, remoteSessions: store.resumableSessions) else { return }
                await store.endRemoteSession(candidate: candidate)
            }
        }
    }

    private func remoteStatus(_ candidate: RemoteSessionCandidate) -> String {
        switch candidate.status {
        case 3:
            return "Check session"
        case 2:
            return "Connecting"
        case 1:
            return "Queued"
        default:
            return "Status \(candidate.status)"
        }
    }
}

struct SessionScreenPresentation {
    let session: ActiveSession
    let availability: SessionAvailability
    let isStreaming: Bool

    var isChecking: Bool { availability == .checking }

    var status: String {
        switch availability {
        case .checking: return "Checking session…"
        case .unverified: return "Session needs checking"
        case .available:
            switch session.status {
            case 3: return isStreaming ? "Streaming" : "Ready to return"
            case 2: return "Connecting"
            case 1:
                guard let queue = session.queuePosition else { return "Queued" }
                return queue == 1 ? "Next in queue" : "Queue #\(queue)"
            default: return "Status \(session.status)"
            }
        }
    }

    var actionTitle: String {
        switch availability {
        case .checking: return "Checking Session…"
        case .unverified: return "Check Session"
        case .available:
            if isStreaming { return "Return to Stream" }
            return session.status == 1 ? "Show Queue" : "Resume Session"
        }
    }

    var symbol: String {
        guard availability == .available else { return "arrow.clockwise.circle" }
        return session.status == 3 ? "play.circle.fill" : "hourglass"
    }
}

enum SessionEndTarget {
    case current(ActiveSession)
    case remote(RemoteSessionCandidate, title: String)

    private var isQueued: Bool {
        switch self {
        case let .current(session): return session.status == 1
        case let .remote(candidate, _): return candidate.status == 1
        }
    }

    private var gameTitle: String {
        switch self {
        case let .current(session): return session.game.title
        case let .remote(_, title): return title
        }
    }

    var title: String { isQueued ? "Leave Queue?" : "End Cloud Session?" }
    var actionTitle: String { isQueued ? "Leave Queue" : "End Cloud Session" }
    var message: String {
        isQueued
            ? "Leave the queue for \(gameTitle)? You’ll lose your place and need to join again to play. If the game has already started, it will close and unsaved progress may be lost."
            : "End the cloud session for \(gameTitle)? The game will close on GeForce NOW and unsaved progress may be lost. This is not just a disconnect."
    }

    func matches(activeSession: ActiveSession?, remoteSessions: [RemoteSessionCandidate]) -> Bool {
        switch self {
        case let .current(session):
            return activeSession?.id == session.id && activeSession?.status == session.status
        case let .remote(candidate, _):
            return remoteSessions.contains { $0.id == candidate.id && $0.status == candidate.status }
        }
    }
}
