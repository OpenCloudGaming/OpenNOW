import SwiftUI

struct ContentView: View {
    @EnvironmentObject private var store: OpenNOWStore

    var body: some View {
        TabView {
            HomeTabView()
                .tabItem {
                    Label("Home", systemImage: "house.fill")
                }

            BrowseTabView()
                .tabItem {
                    Label("Browse", systemImage: "square.grid.2x2.fill")
                }

            LibraryTabView()
                .tabItem {
                    Label("Library", systemImage: "books.vertical")
                }

            SessionTabView()
                .tabItem {
                    Label("Session", systemImage: "dot.radiowaves.left.and.right")
                }

            SettingsTabView()
                .tabItem {
                    Label("Settings", systemImage: "slider.horizontal.3")
                }
        }
        .tint(.accentColor)
        .task {
            await store.bootstrap()
        }
    }
}

private struct HomeTabView: View {
    @EnvironmentObject private var store: OpenNOWStore

    var body: some View {
        NavigationStack {
            List {
                Section("Account") {
                    if store.user == nil {
                        Picker("Provider", selection: $store.settings.selectedProviderIdpId) {
                            ForEach(store.providers) { provider in
                                Text(provider.displayName).tag(provider.idpId)
                            }
                        }

                        Button {
                            Task { await store.signIn() }
                        } label: {
                            if store.isAuthenticating {
                                ProgressView()
                            } else {
                                Text("Sign In with NVIDIA")
                            }
                        }
                    } else if let user = store.user {
                        LabeledContent("Signed In As", value: user.displayName)
                        if let email = user.email {
                            LabeledContent("Email", value: email)
                        }
                        LabeledContent("Tier", value: user.membershipTier)
                        Button("Sign Out", role: .destructive) {
                            store.signOut()
                        }
                    }
                }

                if let sub = store.subscription {
                    Section("Subscription") {
                        LabeledContent("Membership", value: sub.membershipTier)
                        LabeledContent("Gameplay Allowed", value: sub.isGamePlayAllowed ? "Yes" : "No")
                        if sub.isUnlimited {
                            LabeledContent("Session Time", value: "Unlimited")
                        } else {
                            LabeledContent("Remaining", value: String(format: "%.1f h", sub.remainingHours))
                            LabeledContent("Total", value: String(format: "%.1f h", sub.totalHours))
                        }
                    }
                }

                Section("Featured") {
                    if store.isLoadingGames && store.featuredGames.isEmpty {
                        HStack {
                            Spacer()
                            ProgressView("Loading games")
                            Spacer()
                        }
                    } else {
                        ForEach(store.featuredGames) { game in
                            GameRowView(game: game) {
                                Task { await store.launch(game: game) }
                            }
                        }
                    }
                }

                if let message = store.lastError {
                    Section {
                        Text(message)
                            .foregroundStyle(.red)
                    }
                }
            }
            .refreshable {
                await store.refreshCatalog()
            }
            .navigationTitle("OpenNOW")
        }
    }
}

private struct BrowseTabView: View {
    @EnvironmentObject private var store: OpenNOWStore

    var body: some View {
        NavigationStack {
            List {
                ForEach(store.filteredCatalogGames) { game in
                    GameRowView(game: game) {
                        Task { await store.launch(game: game) }
                    }
                }
            }
            .searchable(text: $store.searchText, placement: .navigationBarDrawer(displayMode: .automatic), prompt: "Search by title, genre, or platform")
            .refreshable {
                await store.refreshCatalog()
            }
            .navigationTitle("Browse")
        }
    }
}

private struct LibraryTabView: View {
    @EnvironmentObject private var store: OpenNOWStore

    var body: some View {
        NavigationStack {
            List {
                if store.libraryGames.isEmpty {
                    Text("Your library is empty on this account.")
                        .foregroundStyle(.secondary)
                } else {
                    ForEach(store.libraryGames) { game in
                        GameRowView(game: game) {
                            Task { await store.launch(game: game) }
                        }
                    }
                }
            }
            .navigationTitle("Library")
        }
    }
}

private struct SessionTabView: View {
    @EnvironmentObject private var store: OpenNOWStore

    var body: some View {
        NavigationStack {
            List {
                if let session = store.activeSession {
                    Section("Now Playing") {
                        LabeledContent("Title", value: session.game.title)
                        LabeledContent("Genre", value: session.game.genre)
                        LabeledContent("Platform", value: session.game.platform)
                        LabeledContent("Session ID", value: session.id)
                        LabeledContent("Status", value: String(session.status))
                        if let queue = session.queuePosition {
                            LabeledContent("Queue Position", value: String(queue))
                        }
                        if let serverIp = session.serverIp {
                            LabeledContent("Server", value: serverIp)
                        }
                        LabeledContent("Elapsed", value: store.formattedSessionElapsed())
                        Button("End Session", role: .destructive) {
                            Task { await store.endSession() }
                        }
                    }

                    Section("Telemetry") {
                        MetricRow(label: "Ping", value: "\(store.telemetry.pingMs) ms")
                        MetricRow(label: "FPS", value: "\(store.telemetry.fps)")
                        MetricRow(label: "Packet Loss", value: String(format: "%.2f%%", store.telemetry.packetLossPercent))
                        MetricRow(label: "Bitrate", value: String(format: "%.1f Mbps", store.telemetry.bitrateMbps))
                    }

                    Section("Controls") {
                        Toggle("Microphone", isOn: $store.micEnabled)
                        Toggle("Recording", isOn: $store.recordingEnabled)
                        Toggle("Controller Connected", isOn: $store.controllerConnected)
                    }
                } else {
                    Section {
                        Text("No active session. Launch a game from Home, Browse, or Library.")
                            .foregroundStyle(.secondary)
                    }
                }

                Section("Active Sessions On Account") {
                    if store.resumableSessions.isEmpty {
                        Text("No resumable sessions found.")
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(store.resumableSessions) { candidate in
                            HStack {
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(candidate.id)
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                    Text("Status \(candidate.status)")
                                        .font(.footnote)
                                    if let appId = candidate.appId {
                                        Text("App \(appId)")
                                            .font(.caption2)
                                            .foregroundStyle(.secondary)
                                    }
                                }
                                Spacer()
                                Button("Resume") {
                                    Task { await store.resumeSession(candidate: candidate) }
                                }
                                .buttonStyle(.bordered)
                            }
                        }
                    }
                    Button("Refresh Active Sessions") {
                        Task { await store.refreshRemoteSessions() }
                    }
                }
            }
            .navigationTitle("Session")
        }
    }
}

private struct SettingsTabView: View {
    @EnvironmentObject private var store: OpenNOWStore

    private let fpsValues = [60, 120]
    private let qualityValues = ["Balanced", "Data Saver", "Quality"]
    private let codecValues = ["Auto", "H264", "HEVC", "AV1"]
    private let regionValues = ["Auto", "US East", "US West", "Europe", "Asia"]

    var body: some View {
        NavigationStack {
            Form {
                Section("Streaming") {
                    settingPicker(title: "Region", selection: $store.settings.preferredRegion, values: regionValues)
                    settingPicker(title: "FPS", selection: $store.settings.preferredFPS, values: fpsValues)
                    settingPicker(title: "Quality", selection: $store.settings.preferredQuality, values: qualityValues)
                    settingPicker(title: "Codec", selection: $store.settings.preferredCodec, values: codecValues)
                }

                Section("Experience") {
                    Toggle("Keep microphone enabled", isOn: $store.settings.keepMicEnabled)
                    Toggle("Show telemetry overlay", isOn: $store.settings.showStatsOverlay)
                }

                Section("Data") {
                    Button("Reload Catalog") {
                        Task { await store.refreshCatalog() }
                    }
                }
            }
            .navigationTitle("Settings")
            .onChange(of: store.settings) { _, _ in
                store.persistSettings()
            }
        }
    }

    private func settingPicker<T: Hashable>(title: String, selection: Binding<T>, values: [T]) -> some View {
        HStack {
            Text(title)
            Spacer()
            Picker(title, selection: selection) {
                ForEach(values, id: \.self) { value in
                    Text(String(describing: value))
                        .tag(value)
                }
            }
            .pickerStyle(.menu)
        }
    }
}

private struct MetricRow: View {
    let label: String
    let value: String

    var body: some View {
        LabeledContent(label, value: value)
    }
}

private struct GameRowView: View {
    let game: CloudGame
    let onLaunch: () -> Void

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            Image(systemName: game.icon)
                .font(.title3)
                .foregroundStyle(.tint)
                .frame(width: 30)

            VStack(alignment: .leading, spacing: 2) {
                Text(game.title)
                    .font(.body.weight(.semibold))
                Text("\(game.genre) - \(game.platform)")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
                if game.launchAppId == nil {
                    Text("Not directly launchable")
                        .font(.caption2)
                        .foregroundStyle(.orange)
                }
            }
            Spacer()
            Button("Launch", action: onLaunch)
                .disabled(game.launchAppId == nil)
        }.contentShape(Rectangle())
            .onTapGesture {
                if game.launchAppId != nil {
                    onLaunch()
                }
            }
    }
}

#Preview {
    ContentView()
        .environmentObject(OpenNOWStore())
}
