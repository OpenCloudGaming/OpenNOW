import SwiftUI
import Charts

struct SessionView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var pingSamples: [Double] = []
    @State private var fpsSamples: [Double] = []
    /// Which resumable row’s “End” is in progress (shows spinner).
    @State private var endingRemoteSessionId: String?

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(spacing: 20) {
                    if let session = store.activeSession {
                        nowPlayingCard(session: session)
                        telemetrySection
                        controlsSection
                        endSessionButton
                    } else {
                        noSessionState
                    }

                    if !store.resumableSessions.isEmpty || store.activeSession == nil {
                        resumableSection
                    }
                }
                .padding(.horizontal)
                .padding(.vertical, 8)
            }
            .navigationTitle("Session")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        Haptics.light()
                        Task { await store.refreshRemoteSessions() }
                    } label: {
                        Image(systemName: "arrow.clockwise")
                    }
                }
            }
        }
        .onChange(of: store.telemetry) { _, new in
            pingSamples.append(Double(new.pingMs))
            fpsSamples.append(Double(new.fps))
            if pingSamples.count > 30 { pingSamples.removeFirst() }
            if fpsSamples.count > 30 { fpsSamples.removeFirst() }
        }
    }

    private func nowPlayingCard(session: ActiveSession) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(spacing: 14) {
                ZStack {
                    RoundedRectangle(cornerRadius: 12)
                        .fill(gameColor(for: session.game.title).opacity(0.2))
                        .frame(width: 56, height: 56)
                    Image(systemName: session.game.icon)
                        .font(.title2)
                        .foregroundStyle(gameColor(for: session.game.title))
                }

                VStack(alignment: .leading, spacing: 3) {
                    Text(session.game.title)
                        .font(.headline)
                    HStack(spacing: 6) {
                        statusDot(status: session.status)
                        Text(statusLabel(session.status))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }

                Spacer()

                VStack(alignment: .trailing, spacing: 2) {
                    Text(store.formattedSessionElapsed())
                        .font(.title3.monospacedDigit().bold())
                    Text("elapsed")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            if let queue = session.queuePosition {
                HStack {
                    Image(systemName: "person.3.fill")
                        .foregroundStyle(.orange)
                    Text("Queue position: \(queue)")
                        .font(.subheadline)
                        .foregroundStyle(.orange)
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(.orange.opacity(0.1), in: RoundedRectangle(cornerRadius: 10))
            }
        }
        .padding(16)
        .glassCard()
    }

    private func statusDot(status: Int) -> some View {
        Circle()
            .fill(statusDotColor(status))
            .frame(width: 8, height: 8)
    }

    private func statusDotColor(_ status: Int) -> Color {
        switch status {
        case 3:
            return .green
        case 2, 1:
            return .orange
        default:
            return .red
        }
    }

    private func statusLabel(_ status: Int) -> String {
        switch status {
        case 3: return store.supportsEmbeddedStreamer ? "Connected" : "Ready"
        case 2: return "Initializing"
        case 1: return "Queued"
        default: return "Status \(status)"
        }
    }

    private var telemetrySection: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Telemetry")
                .font(.headline)

            LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 12) {
                MetricTileView(
                    label: "Ping",
                    value: "\(store.telemetry.pingMs) ms",
                    icon: "antenna.radiowaves.left.and.right",
                    color: pingColor(store.telemetry.pingMs),
                    samples: pingSamples
                )
                MetricTileView(
                    label: "FPS",
                    value: "\(store.telemetry.fps)",
                    icon: "chart.line.uptrend.xyaxis",
                    color: fpsColor(store.telemetry.fps),
                    samples: fpsSamples
                )
                MetricTileView(
                    label: "Packet Loss",
                    value: String(format: "%.2f%%", store.telemetry.packetLossPercent),
                    icon: "exclamationmark.triangle",
                    color: lossColor(store.telemetry.packetLossPercent),
                    samples: []
                )
                MetricTileView(
                    label: "Bitrate",
                    value: String(format: "%.1f Mbps", store.telemetry.bitrateMbps),
                    icon: "waveform",
                    color: .blue,
                    samples: []
                )
            }
        }
    }

    private func pingColor(_ ms: Int) -> Color {
        ms < 30 ? .green : (ms < 60 ? .orange : .red)
    }

    private func fpsColor(_ fps: Int) -> Color {
        fps >= 60 ? .green : (fps >= 30 ? .orange : .red)
    }

    private func lossColor(_ loss: Double) -> Color {
        loss < 0.15 ? .green : (loss < 1.0 ? .orange : .red)
    }

    private var controlsSection: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text("Controls")
                .font(.headline)
                .padding(.bottom, 4)

            Toggle(isOn: $store.micEnabled) {
                Label("Microphone", systemImage: store.micEnabled ? "mic.fill" : "mic.slash.fill")
            }
            .tint(brandAccent)
            .padding(14)
            .glassCard()

            Toggle(isOn: $store.recordingEnabled) {
                Label("Recording", systemImage: "record.circle")
            }
            .tint(.red)
            .padding(14)
            .glassCard()
        }
    }

    private var endSessionButton: some View {
        Button(role: .destructive) {
            Haptics.medium()
            Task { await store.endSession() }
        } label: {
            Label("End Session", systemImage: "stop.circle.fill")
                .frame(maxWidth: .infinity)
                .padding(.vertical, 14)
                .font(.headline)
        }
        .buttonStyle(.bordered)
        .tint(.red)
    }

    private var noSessionState: some View {
        VStack(spacing: 20) {
            Image(systemName: "dot.radiowaves.left.and.right")
                .font(.system(size: 56))
                .foregroundStyle(.quaternary)
                .padding(.top, 40)
            Text("No Active Session")
                .font(.title3.bold())
            Text(
                store.supportsEmbeddedStreamer
                    ? "Launch a game from Home, Browse, or Library to start streaming."
                    : "This Apple TV build can browse session state, but streaming is still disabled."
            )
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 20)
        }
        .frame(maxWidth: .infinity)
        .padding(24)
        .glassCard()
    }

    private var resumableSection: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Active on Account")
                .font(.headline)

            if !store.supportsEmbeddedStreamer {
                Text("Resume is disabled on this Apple TV build until the native streamer lands.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if store.resumableSessions.isEmpty {
                Text("No resumable sessions found.")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .glassCard()
            } else {
                ForEach(store.resumableSessions) { candidate in
                    let isEndingRemote = endingRemoteSessionId == candidate.id
                    HStack {
                        VStack(alignment: .leading, spacing: 3) {
                            Text("Session")
                                .font(.caption.bold())
                            Text(candidate.id)
                                .font(.caption2.monospaced())
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                            if let appId = candidate.appId {
                                Text("App: \(appId)")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }
                        }
                        Spacer()
                        HStack(spacing: 8) {
                            Button("Resume") {
                                Haptics.light()
                                store.scheduleResume(candidate: candidate)
                            }
                            .buttonStyle(.bordered)
                            .tint(brandAccent)
                            .disabled(isEndingRemote || !store.supportsEmbeddedStreamer)

                            Button(role: .destructive) {
                                Haptics.medium()
                                Task { @MainActor in
                                    endingRemoteSessionId = candidate.id
                                    await store.endRemoteSession(candidate: candidate)
                                    if endingRemoteSessionId == candidate.id {
                                        endingRemoteSessionId = nil
                                    }
                                }
                            } label: {
                                Group {
                                    if isEndingRemote {
                                        ProgressView()
                                            .controlSize(.small)
                                            .tint(.red)
                                    } else {
                                        Text("End")
                                    }
                                }
                                .frame(minWidth: 52, minHeight: 20)
                            }
                            .buttonStyle(.bordered)
                            .disabled(isEndingRemote)
                        }
                    }
                    .padding(14)
                    .glassCard()
                }
            }
        }
    }
}

private struct MetricTileView: View {
    let label: String
    let value: String
    let icon: String
    let color: Color
    let samples: [Double]

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Image(systemName: icon)
                    .font(.caption.bold())
                    .foregroundStyle(color)
                Spacer()
            }

            Text(value)
                .font(.title3.monospacedDigit().bold())
                .foregroundStyle(color)

            Text(label)
                .font(.caption2)
                .foregroundStyle(.secondary)

            if samples.count > 2 {
                Chart {
                    ForEach(Array(samples.enumerated()), id: \.offset) { index, sample in
                        LineMark(
                            x: .value("Index", index),
                            y: .value(label, sample)
                        )
                        .foregroundStyle(color)
                        .interpolationMethod(.catmullRom)
                    }
                }
                .chartXAxis(.hidden)
                .chartYAxis(.hidden)
                .frame(height: 40)
            }
        }
        .padding(14)
        .glassCard()
    }
}
