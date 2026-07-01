import SwiftUI

struct SessionView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var endingRemoteSessionId: String?

    var body: some View {
        NavigationStack {
            List {
                if let session = store.activeSession {
                    currentSessionSection(session)
                    streamProfileSection(session)
                    controlsSection
                    endSessionSection
                } else {
                    Section {
                        OpenNOWUnavailableView("No Active Session", systemImage: "dot.radiowaves.left.and.right")
                        if store.canJumpBackToSession {
                            Button {
                    store.jumpBackToSession()
                } label: {
                    Label("Resume Session", systemImage: "arrow.clockwise.circle")
                }
                .buttonStyle(.plain)
            }
        }
                }

                remoteSessionsSection
            }
            .listStyle(.insetGrouped)
            .navigationTitle("Session")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        Task { await store.refreshRemoteSessions() }
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                }
            }
        }
    }

    private func currentSessionSection(_ session: ActiveSession) -> some View {
        Section {
            GameListRowView(
                game: session.game,
                subtitle: statusLabel(session),
                trailingSystemImage: session.status == 3 ? "play.circle.fill" : "hourglass"
            )

            LabeledContent("Elapsed", value: store.formattedSessionElapsed())
            if let queue = session.queuePosition {
                LabeledContent("Queue", value: queue == 1 ? "Next" : "#\(queue)")
            }
            if let gpuType = session.gpuType, !gpuType.isEmpty {
                LabeledContent("GPU", value: gpuType)
            }

            if store.canReopenStreamer {
                Button {
                    store.jumpBackToSession()
                } label: {
                    Label("Return to Stream", systemImage: "play.circle.fill")
                }
            } else if store.showStreamLoading {
                Button {
                    store.jumpBackToSession()
                } label: {
                    Label("Show Queue", systemImage: "list.bullet.rectangle")
                }
            } else {
                Button {
                    store.jumpBackToSession()
                } label: {
                    Label("Resume Session", systemImage: "arrow.clockwise.circle")
                }
            }
        } header: {
            Text("Current")
        }
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
            LabeledContent("G-Sync", value: (session.finalizedStreamingFeatures?.cloudGsync ?? store.settings.enableCloudGsync) ? "On" : "Off")
        }
    }

    private var controlsSection: some View {
        Section("Controls") {
            Toggle(isOn: $store.micEnabled) {
                Label("Microphone", systemImage: store.micEnabled ? "mic.fill" : "mic.slash")
            }
            Toggle(isOn: $store.recordingEnabled) {
                Label("Recording", systemImage: "record.circle")
            }
        }
    }

    private var endSessionSection: some View {
        Section {
            Button(role: .destructive) {
                Task { await store.endSession() }
            } label: {
                Label("End Session", systemImage: "stop.circle")
            }
        }
    }

    private var remoteSessionsSection: some View {
        Section("Active on Account") {
            if store.resumableSessions.isEmpty {
                Text("No resumable sessions found.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(store.resumableSessions) { candidate in
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
                                    Label("Resume", systemImage: "arrow.clockwise.circle")
                                }
                                Button(role: .destructive) {
                                    Task {
                                        endingRemoteSessionId = candidate.id
                                        await store.endRemoteSession(candidate: candidate)
                                        endingRemoteSessionId = nil
                                    }
                                } label: {
                                    Label("End", systemImage: "stop.circle")
                                }
                            } label: {
                                Image(systemName: "ellipsis.circle")
                            }
                        }
                    }
                }
            }
        }
    }

    private func statusLabel(_ session: ActiveSession) -> String {
        switch session.status {
        case 3:
            return store.supportsEmbeddedStreamer ? "Streaming" : "Ready"
        case 2:
            return "Connecting"
        case 1:
            if let queue = session.queuePosition {
                return queue == 1 ? "Next in queue" : "Queue #\(queue)"
            }
            return "Queued"
        default:
            return "Status \(session.status)"
        }
    }

    private func remoteStatus(_ candidate: RemoteSessionCandidate) -> String {
        switch candidate.status {
        case 3:
            return "Ready"
        case 2:
            return "Connecting"
        case 1:
            return "Queued"
        default:
            return "Status \(candidate.status)"
        }
    }
}
