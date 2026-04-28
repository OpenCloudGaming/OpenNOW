import SwiftUI

struct ContentView: View {
    let bridge: ONAppCoreBridge
    @State private var selectedSection: MacSection = .home
    @State private var proofStatus = "not started"
    @State private var accountActionStatus = ""

    var body: some View {
        HStack(spacing: 0) {
            SidebarView(selectedSection: $selectedSection, bridge: bridge)

            Divider()

            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    header

                    switch selectedSection {
                    case .account:
                        AccountView(
                            model: bridge.accountViewModel(),
                            authStatus: bridge.nativeAuthStatus(),
                            actionStatus: accountActionStatus,
                            beginLogin: {
                                accountActionStatus = bridge.beginNativeLogin()
                            },
                            clearAuth: {
                                accountActionStatus = bridge.clearNativeAuthState()
                            }
                        )
                    case .home:
                        HomeView(model: bridge.homeViewModel())
                    case .catalog:
                        GameCollectionView(
                            title: "Catalog",
                            subtitle: "Browse layout wired for future C++ catalog data.",
                            games: bridge.catalogViewModels()
                        )
                    case .library:
                        GameCollectionView(
                            title: "Library",
                            subtitle: "Owned games placeholder backed by the bridge.",
                            games: bridge.libraryViewModels()
                        )
                    case .settings:
                        SettingsView(model: bridge.settingsViewModel())
                    case .queue:
                        QueueView(model: bridge.queueViewModel())
                    case .stream:
                        StreamView(
                            model: bridge.streamViewModel(),
                            proofStatus: proofStatus,
                            runProof: {
                                proofStatus = bridge.proofStreamStatus()
                            }
                        )
                    case .stats:
                        StatsOverlayView(model: bridge.statsViewModel())
                    case .logs:
                        LogsStatusView(logs: bridge.statusLogViewModels())
                    }
                }
                .padding(28)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .frame(minWidth: 980, minHeight: 680)
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(selectedSection.title)
                .font(.largeTitle.weight(.semibold))
            Text("OpenNOW native macOS - C++ core \(bridge.coreVersion)")
                .font(.headline)
                .foregroundStyle(.secondary)
        }
    }
}

private enum MacSection: String, CaseIterable, Identifiable {
    case account
    case home
    case catalog
    case library
    case settings
    case queue
    case stream
    case stats
    case logs

    var id: String { rawValue }

    var title: String {
        switch self {
        case .account: return "Login and Account"
        case .home: return "Home"
        case .catalog: return "Catalog"
        case .library: return "Library"
        case .settings: return "Settings"
        case .queue: return "Queue and Loading"
        case .stream: return "Stream View"
        case .stats: return "Stats Overlay"
        case .logs: return "Logs and Status"
        }
    }

    var systemImage: String {
        switch self {
        case .account: return "person.crop.circle"
        case .home: return "house"
        case .catalog: return "rectangle.grid.2x2"
        case .library: return "books.vertical"
        case .settings: return "gearshape"
        case .queue: return "clock"
        case .stream: return "play.rectangle"
        case .stats: return "chart.line.uptrend.xyaxis"
        case .logs: return "list.bullet.rectangle"
        }
    }
}

private struct SidebarView: View {
    @Binding var selectedSection: MacSection
    let bridge: ONAppCoreBridge

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            VStack(alignment: .leading, spacing: 4) {
                Text("OpenNOW")
                    .font(.title2.weight(.bold))
                Text("macOS parity scaffold")
                    .foregroundStyle(.secondary)
            }

            VStack(alignment: .leading, spacing: 6) {
                ForEach(MacSection.allCases) { section in
                    Button {
                        selectedSection = section
                    } label: {
                        Label(section.title, systemImage: section.systemImage)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(SidebarButtonStyle(isSelected: selectedSection == section))
                }
            }

            Spacer()

            VStack(alignment: .leading, spacing: 8) {
                ServiceBadge(title: "Auth", isReady: bridge.hasAuthService)
                ServiceBadge(title: "Catalog", isReady: bridge.hasCatalogService)
                ServiceBadge(title: "Sessions", isReady: bridge.hasSessionService)
            }
        }
        .padding(20)
        .frame(width: 240)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

private struct SidebarButtonStyle: ButtonStyle {
    let isSelected: Bool

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .padding(.vertical, 8)
            .padding(.horizontal, 10)
            .background(isSelected ? Color.accentColor.opacity(0.16) : Color.clear)
            .foregroundStyle(isSelected ? Color.accentColor : Color.primary)
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .opacity(configuration.isPressed ? 0.72 : 1)
    }
}

private struct ServiceBadge: View {
    let title: String
    let isReady: Bool

    var body: some View {
        Label("\(title): \(isReady ? "ready" : "pending")", systemImage: isReady ? "checkmark.circle.fill" : "exclamationmark.triangle.fill")
            .font(.caption.monospaced())
            .foregroundStyle(isReady ? .green : .orange)
    }
}

private struct AccountView: View {
    let model: [String: String]
    let authStatus: [String: String]
    let actionStatus: String
    let beginLogin: () -> Void
    let clearAuth: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Card {
                VStack(alignment: .leading, spacing: 12) {
                    Label(model["status"] ?? "Unknown", systemImage: "person.badge.key")
                        .font(.title3.weight(.semibold))
                    Text(model["title"] ?? "Sign in")
                        .font(.title.weight(.semibold))
                    Text(model["subtitle"] ?? "")
                        .foregroundStyle(.secondary)
                    HStack {
                        Text(model["provider"] ?? "Provider pending")
                            .font(.body.monospaced())
                        Spacer()
                        Button(model["cta"] ?? "Continue") {
                            beginLogin()
                        }
                            .buttonStyle(.borderedProminent)
                    }
                    if !actionStatus.isEmpty {
                        Text(actionStatus)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            DetailGrid(rows: [
                ("State", authStatus["state"] ?? "Unknown"),
                ("Persistence", authStatus["canClear"] == "true" ? "Pending login marker stored in Keychain" : "No pending Keychain auth marker"),
                ("Callback", "OAuth callback exchange not connected yet")
            ])

            HStack {
                Button("Clear Native Auth State") {
                    clearAuth()
                }
                .disabled(authStatus["canClear"] != "true")

                Text(authStatus["detail"] ?? "")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private struct HomeView: View {
    let model: [String: String]

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Card {
                VStack(alignment: .leading, spacing: 12) {
                    Text(model["headline"] ?? "Home")
                        .font(.title.weight(.bold))
                    Text(model["subtitle"] ?? "")
                        .foregroundStyle(.secondary)
                    Text(model["serviceState"] ?? "")
                        .font(.body.monospaced())
                }
            }

            HStack(alignment: .top, spacing: 16) {
                FeatureTile(title: model["featuredTitle"] ?? "Featured", subtitle: model["featuredMeta"] ?? "", systemImage: "sparkles.tv")
                FeatureTile(title: model["recentTitle"] ?? "Recent", subtitle: "Native session history will appear here.", systemImage: "clock.arrow.circlepath")
            }
        }
    }
}

private struct GameCollectionView: View {
    let title: String
    let subtitle: String
    let games: [[String: String]]

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(subtitle)
                .foregroundStyle(.secondary)

            LazyVGrid(columns: [GridItem(.adaptive(minimum: 220), spacing: 16)], spacing: 16) {
                ForEach(games.indices, id: \.self) { index in
                    GameCard(game: games[index])
                }
            }

            if games.isEmpty {
                EmptyStateView(title: "No \(title.lowercased()) items", message: "Connect the matching C++ service to populate this surface.")
            }
        }
    }
}

private struct GameCard: View {
    let game: [String: String]

    var body: some View {
        Card {
            VStack(alignment: .leading, spacing: 10) {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(LinearGradient(colors: [.accentColor.opacity(0.55), .purple.opacity(0.35)], startPoint: .topLeading, endPoint: .bottomTrailing))
                    .frame(height: 116)
                    .overlay(Image(systemName: "gamecontroller.fill").font(.largeTitle).foregroundStyle(.white.opacity(0.8)))

                Text(game["title"] ?? "Untitled")
                    .font(.headline)
                Text(game["subtitle"] ?? "")
                    .foregroundStyle(.secondary)
                Text(game["meta"] ?? "")
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
                Text(game["status"] ?? "")
                    .font(.caption)
                    .padding(.vertical, 4)
                    .padding(.horizontal, 8)
                    .background(Color.secondary.opacity(0.12))
                    .clipShape(Capsule())
            }
        }
    }
}

private struct SettingsView: View {
    let model: [String: String]

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            DetailGrid(rows: [
                ("Resolution", model["resolution"] ?? "Unknown"),
                ("Frame Rate", model["frameRate"] ?? "Unknown"),
                ("Bitrate", model["bitrate"] ?? "Unknown"),
                ("Codec", model["codec"] ?? "Unknown"),
                ("Color Quality", model["colorQuality"] ?? "Unknown"),
                ("Keyboard", model["keyboardLayout"] ?? "Unknown"),
                ("Microphone", model["microphone"] ?? "Unknown")
            ])

            Card {
                Toggle("Show performance overlay in stream", isOn: .constant(true))
                Toggle("Capture keyboard shortcuts in stream", isOn: .constant(true))
                Toggle("Use native full screen handoff", isOn: .constant(false))
            }
        }
    }
}

private struct QueueView: View {
    let model: [String: String]

    var body: some View {
        Card {
            VStack(alignment: .leading, spacing: 16) {
                HStack {
                    ProgressView()
                        .controlSize(.large)
                    VStack(alignment: .leading) {
                        Text(model["title"] ?? "Loading")
                            .font(.title2.weight(.semibold))
                        Text(model["detail"] ?? "")
                            .foregroundStyle(.secondary)
                    }
                }

                DetailGrid(rows: [
                    ("Queue", model["position"] ?? "Unknown"),
                    ("Estimate", model["estimate"] ?? "Unknown"),
                    ("Phase", model["phase"] ?? "Unknown")
                ])
            }
        }
    }
}

private struct StreamView: View {
    let model: [String: String]
    let proofStatus: String
    let runProof: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            ZStack {
                RoundedRectangle(cornerRadius: 18, style: .continuous)
                    .fill(Color.black)
                VStack(spacing: 12) {
                    Image(systemName: "play.rectangle.fill")
                        .font(.system(size: 56))
                    Text(model["title"] ?? "Stream")
                        .font(.title.weight(.bold))
                    Text("Renderer placeholder until native WebRTC video is connected.")
                        .foregroundStyle(.secondary)
                }
                .foregroundStyle(.white)
            }
            .frame(minHeight: 320)

            HStack {
                Button("Run Proof Stream Backend", action: runProof)
                    .buttonStyle(.borderedProminent)
                Text("Last proof: \(proofStatus)")
                    .font(.body.monospaced())
                    .foregroundStyle(.secondary)
            }

            DetailGrid(rows: [
                ("State", model["state"] ?? "Unknown"),
                ("Resolution", model["resolution"] ?? "Unknown"),
                ("Latency", model["latency"] ?? "Unknown"),
                ("Input", model["input"] ?? "Unknown"),
                ("Transport", model["transport"] ?? "Unknown")
            ])
        }
    }
}

private struct StatsOverlayView: View {
    let model: [String: String]

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Card {
                VStack(alignment: .leading, spacing: 12) {
                    Text("In-stream overlay")
                        .font(.title2.weight(.semibold))
                    DetailGrid(rows: [
                        ("FPS", model["fps"] ?? "--"),
                        ("Bitrate", model["bitrate"] ?? "--"),
                        ("Packet Loss", model["packetLoss"] ?? "--"),
                        ("RTT", model["rtt"] ?? "--"),
                        ("Decoder", model["decoder"] ?? "--"),
                        ("Codec", model["codec"] ?? "--")
                    ])
                }
            }
        }
    }
}

private struct LogsStatusView: View {
    let logs: [[String: String]]

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            ForEach(logs.indices, id: \.self) { index in
                let log = logs[index]
                Card {
                    HStack(alignment: .top, spacing: 12) {
                        Image(systemName: iconName(for: log["level"] ?? "info"))
                            .foregroundStyle(color(for: log["level"] ?? "info"))
                        VStack(alignment: .leading, spacing: 4) {
                            Text(log["message"] ?? "Log")
                                .font(.headline)
                            Text(log["detail"] ?? "")
                                .font(.body.monospaced())
                                .foregroundStyle(.secondary)
                        }
                    }
                }
            }
        }
    }

    private func iconName(for level: String) -> String {
        switch level {
        case "ok": return "checkmark.circle.fill"
        case "warn": return "exclamationmark.triangle.fill"
        default: return "info.circle.fill"
        }
    }

    private func color(for level: String) -> Color {
        switch level {
        case "ok": return .green
        case "warn": return .orange
        default: return .blue
        }
    }
}

private struct DetailGrid: View {
    let rows: [(String, String)]

    var body: some View {
        Card {
            Grid(alignment: .leading, horizontalSpacing: 18, verticalSpacing: 10) {
                ForEach(rows.indices, id: \.self) { index in
                    GridRow {
                        Text(rows[index].0)
                            .foregroundStyle(.secondary)
                        Text(rows[index].1)
                            .font(.body.monospaced())
                    }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

private struct FeatureTile: View {
    let title: String
    let subtitle: String
    let systemImage: String

    var body: some View {
        Card {
            VStack(alignment: .leading, spacing: 12) {
                Image(systemName: systemImage)
                    .font(.largeTitle)
                    .foregroundStyle(Color.accentColor)
                Text(title)
                    .font(.title3.weight(.semibold))
                Text(subtitle)
                    .foregroundStyle(.secondary)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct EmptyStateView: View {
    let title: String
    let message: String

    var body: some View {
        Card {
            VStack(alignment: .leading, spacing: 8) {
                Text(title)
                    .font(.headline)
                Text(message)
                    .foregroundStyle(.secondary)
            }
        }
    }
}

private struct Card<Content: View>: View {
    let content: Content

    init(@ViewBuilder content: () -> Content) {
        self.content = content()
    }

    var body: some View {
        content
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(Color(nsColor: .controlBackgroundColor))
            .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
    }
}
