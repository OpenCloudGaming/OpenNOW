import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var store: OpenNOWStore

    private let fpsValues = [30, 60, 120]
    private let qualityValues = ["Balanced", "Data Saver", "Quality"]
    private let codecValues = ["Auto", "H264", "H265", "AV1"]
    private let regionValues = ["Auto", "US East", "US West", "Europe", "Asia"]

    var body: some View {
        NavigationStack {
            Form {
                profileSection
                streamingSection
                sessionSection
                inputSection
                appSection
                dataSection
                if let user = store.user {
                    accountSection(user)
                }
                aboutSection
            }
            .navigationTitle("Settings")
            .tint(.blue)
            .onChange(of: store.settings) { _, _ in
                store.persistSettings()
            }
            .onChange(of: store.settings.queueLiveActivitiesEnabled) { _, _ in
                store.refreshTrackedSessionSurface()
            }
        }
    }

    private var profileSection: some View {
        Section {
            HStack(spacing: 14) {
                Image(systemName: "person.crop.circle.fill")
                    .font(.system(size: 44))
                    .foregroundStyle(.blue)

                VStack(alignment: .leading, spacing: 3) {
                    Text(store.user?.displayName ?? "OpenNOW")
                        .font(.headline)
                    Text(headerSummary)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.vertical, 4)
        }
    }

    private var streamingSection: some View {
        Section {
            Picker(selection: $store.settings.preferredRegion) {
                ForEach(regionValues, id: \.self) { Text($0).tag($0) }
            } label: {
                Label("Region", systemImage: "globe")
            }

            Picker(selection: $store.settings.preferredResolution) {
                ForEach(StreamSettingsResolver.resolutionOptions, id: \.value) { option in
                    Text(option.label).tag(option.value)
                }
            } label: {
                Label("Resolution", systemImage: "display")
            }

            Picker(selection: $store.settings.preferredFPS) {
                ForEach(fpsValues, id: \.self) { Text("\($0) fps").tag($0) }
            } label: {
                Label("Target FPS", systemImage: "speedometer")
            }

            Picker(selection: $store.settings.preferredQuality) {
                ForEach(qualityValues, id: \.self) { Text($0).tag($0) }
            } label: {
                Label("Quality", systemImage: "slider.horizontal.3")
            }

            Picker(selection: $store.settings.preferredCodec) {
                ForEach(codecValues, id: \.self) { Text($0).tag($0) }
            } label: {
                Label("Codec", systemImage: "video")
            }

            Picker(selection: $store.settings.maxBitrateMbps) {
                ForEach(StreamSettingsResolver.bitrateOptionsMbps, id: \.self) { value in
                    Text(bitrateLabel(for: value)).tag(value)
                }
            } label: {
                Label("Max Bitrate", systemImage: "gauge.with.dots.needle.67percent")
            }
        } header: {
            Text("Streaming")
        } footer: {
            Text("Streaming changes apply to the next session launch.")
        }
    }

    private var sessionSection: some View {
        Section("Game Session") {
            Picker(selection: $store.settings.keyboardLayout) {
                ForEach(StreamSettingsResolver.keyboardLayoutOptions, id: \.value) { option in
                    Text(option.label).tag(option.value)
                }
            } label: {
                Label("Keyboard Layout", systemImage: "keyboard")
            }

            Picker(selection: $store.settings.gameLanguage) {
                ForEach(StreamSettingsResolver.gameLanguageOptions, id: \.value) { option in
                    Text(option.label).tag(option.value)
                }
            } label: {
                Label("Game Language", systemImage: "character.book.closed")
            }

            Toggle(isOn: $store.settings.enableL4S) {
                Label("Low Latency Mode", systemImage: "bolt.horizontal")
            }

        }
    }

    private var inputSection: some View {
        Section {
            #if !os(tvOS)
            Toggle(isOn: $store.settings.fortnitePrefersNativeTouch) {
                Label("Fortnite Mobile Touch", systemImage: "hand.tap")
            }

            Toggle(isOn: $store.settings.streamerPreferences.touchControllerVisible) {
                Label("Show Touch Controller", systemImage: "circle.grid.cross")
            }
            #endif

            HStack {
                Label("Controller Passthrough", systemImage: "gamecontroller")
                Spacer()
                Text("Automatic")
                    .foregroundStyle(.secondary)
            }
        } header: {
            Text("Input")
        } footer: {
            #if os(tvOS)
            Text("Connected controllers are passed through using the native gamepad path. Touch overlays are disabled on Apple TV.")
            #else
            Text("Bluetooth controllers are detected automatically and passed through using the native gamepad path.")
            #endif
        }
    }

    private var appSection: some View {
        Section("Experience") {
            Toggle(isOn: $store.settings.keepMicEnabled) {
                Label("Keep Microphone On", systemImage: "mic")
            }

            Toggle(isOn: $store.settings.showStatsOverlay) {
                Label("Stats Overlay", systemImage: "chart.bar")
            }

            #if !os(tvOS)
            Toggle(isOn: $store.settings.queueLiveActivitiesEnabled) {
                Label("Queue Live Activities", systemImage: "livephoto")
            }
            #endif

            Toggle(isOn: $store.settings.hideServerSelector) {
                Label("Skip Server Selector", systemImage: "server.rack")
            }
        }
    }

    private var dataSection: some View {
        Section("Data") {
            Button {
                Task { await store.refreshCatalog() }
            } label: {
                HStack {
                    Label("Reload Catalog", systemImage: "arrow.clockwise")
                    Spacer()
                    if store.isLoadingGames {
                        ProgressView()
                    }
                }
            }
            .disabled(store.isLoadingGames)

            if !store.settings.favoriteGameIds.isEmpty {
                Button(role: .destructive) {
                    store.clearFavorites()
                } label: {
                    HStack {
                        Label("Clear Favorites", systemImage: "heart.slash")
                        Spacer()
                        Text("\(store.settings.favoriteGameIds.count)")
                            .foregroundStyle(.secondary)
                    }
                }
            }
        }
    }

    private func accountSection(_ user: UserProfile) -> some View {
        Section("Account") {
            LabeledContent("Account", value: user.displayName)
            if let email = user.email {
                LabeledContent("Email", value: email)
            }
            LabeledContent("Tier", value: user.membershipTier)
            Button(role: .destructive) {
                store.signOut()
            } label: {
                Text("Sign Out")
            }
        }
    }

    private var aboutSection: some View {
        Section("About") {
            LabeledContent("Version", value: "1.0")
            LabeledContent("Platform", value: OpenNOWPlatform.displayName)
            Link(destination: URL(string: "https://github.com/OpenCloudGaming/OpenNOW")!) {
                Text("GitHub Repository")
            }
        }
    }

    private var headerSummary: String {
        let profile = StreamSettingsResolver.profile(for: store.settings)
        return "\(profile.width)x\(profile.height) at \(profile.fps) fps"
    }

    private func bitrateLabel(for value: Int) -> String {
        value == 0 ? "Auto" : "\(value) Mbps"
    }
}
