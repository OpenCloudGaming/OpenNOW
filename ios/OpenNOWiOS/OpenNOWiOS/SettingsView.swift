import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var showingResetConfirmation = false

    private let fpsValues = [30, 60, 120]
    private let qualityValues = ["Balanced", "Data Saver", "Quality"]
    private let codecValues = ["Auto", "H264", "H265", "AV1"]
    private let regionValues = ["Auto", "US East", "US West", "Europe", "Asia"]

    var body: some View {
        NavigationStack {
            Form {
                profileSection
                streamingSection
                advancedStreamingSection
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
            .onChange(of: store.settings) { _, _ in
                store.persistSettings()
            }
            .onChange(of: store.settings.queueLiveActivitiesEnabled) { _, _ in
                store.refreshTrackedSessionSurface()
            }
            .onAppear {
                enforceAvailableResolution()
            }
            .onChange(of: store.settings.preferredAspectRatio) { _, _ in
                enforceAvailableResolution()
            }
            .onChange(of: currentMembershipTier ?? "") { _, _ in
                enforceAvailableResolution()
            }
            .confirmationDialog("Reset settings?", isPresented: $showingResetConfirmation, titleVisibility: .visible) {
                Button("Reset Settings", role: .destructive) {
                    store.resetSettings()
                }
                Button("Cancel", role: .cancel) {}
            }
        }
    }

    private var profileSection: some View {
        Section {
            LabeledContent("Account", value: store.user?.displayName ?? "Signed out")
            LabeledContent("Tier", value: store.subscription?.membershipTier ?? store.user?.membershipTier ?? "Unknown")
            LabeledContent("Profile", value: headerSummary)
        }
    }

    private var streamingSection: some View {
        Section {
            Picker("Region", selection: $store.settings.preferredRegion) {
                ForEach(regionValues, id: \.self) { Text($0).tag($0) }
            }

            Picker("Aspect Ratio", selection: $store.settings.preferredAspectRatio) {
                ForEach(StreamSettingsResolver.aspectRatioOptions, id: \.self) { value in
                    Text(value).tag(value)
                }
            }

            Picker("Resolution", selection: $store.settings.preferredResolution) {
                Text("Auto").tag("Auto")
                ForEach(StreamSettingsResolver.choices(forAspectRatio: store.settings.preferredAspectRatio)) { choice in
                    let available = resolutionAvailable(choice)
                    Text(resolutionLabel(for: choice, available: available))
                        .foregroundStyle(available ? .primary : .secondary)
                        .tag(choice.value)
                        .disabled(!available)
                }
            }

            Picker("Target FPS", selection: $store.settings.preferredFPS) {
                ForEach(fpsValues, id: \.self) { Text("\($0) fps").tag($0) }
            }

            Picker("Quality", selection: $store.settings.preferredQuality) {
                ForEach(qualityValues, id: \.self) { Text($0).tag($0) }
            }

            Picker("Codec", selection: $store.settings.preferredCodec) {
                ForEach(codecValues, id: \.self) { Text($0).tag($0) }
            }

            Picker("Color", selection: $store.settings.preferredColorQuality) {
                ForEach(StreamColorQuality.allCases) { color in
                    Text(color.label).tag(color.rawValue)
                }
            }

            Picker("Max Bitrate", selection: $store.settings.maxBitrateMbps) {
                ForEach(StreamSettingsResolver.bitrateOptionsMbps, id: \.self) { value in
                    Text(bitrateLabel(for: value)).tag(value)
                }
            }
        } header: {
            Text("Streaming")
        } footer: {
            Text("Resolutions above your current plan are shown but unavailable.")
        }
    }

    private var advancedStreamingSection: some View {
        Section {
            Toggle("HDR", isOn: $store.settings.hdrEnabled)
                .disabled(!hdrAvailable)

            Toggle("Cloud G-Sync", isOn: $store.settings.enableCloudGsync)

            Toggle("L4S Low Latency", isOn: $store.settings.enableL4S)

            Toggle("Stream Sharpening", isOn: $store.settings.streamSharpeningEnabled)

            if store.settings.streamSharpeningEnabled {
                LabeledContent {
                    Slider(value: $store.settings.streamSharpeningAmount, in: 0...1)
                        .frame(maxWidth: 180)
                } label: {
                    Text("Amount")
                }
            }

            Toggle("Session Proxy", isOn: $store.settings.sessionProxyEnabled)

            if store.settings.sessionProxyEnabled {
                TextField("Proxy URL", text: $store.settings.sessionProxyUrl)
                    .textInputAutocapitalization(.never)
                    .keyboardType(.URL)
                    .autocorrectionDisabled()
            }
        } header: {
            Text("Advanced Streaming")
        } footer: {
            if !hdrAvailable {
                Text("HDR requires an Ultimate-capable account.")
            }
        }
    }

    private var sessionSection: some View {
        Section("Game Session") {
            Picker("Keyboard Layout", selection: $store.settings.keyboardLayout) {
                ForEach(StreamSettingsResolver.keyboardLayoutOptions, id: \.value) { option in
                    Text(option.label).tag(option.value)
                }
            }

            Picker("Game Language", selection: $store.settings.gameLanguage) {
                ForEach(StreamSettingsResolver.gameLanguageOptions, id: \.value) { option in
                    Text(option.label).tag(option.value)
                }
            }
        }
    }

    private var inputSection: some View {
        Section("Input") {
            #if !os(tvOS)
            Toggle("Fortnite Mobile Touch", isOn: $store.settings.fortnitePrefersNativeTouch)
            Toggle("Touch Controller", isOn: $store.settings.streamerPreferences.touchControllerVisible)
            Toggle("Touchscreen Mode", isOn: $store.settings.streamerPreferences.touchscreenModeEnabled)
            #endif

            LabeledContent("Controller", value: "Automatic")
        }
    }

    private var appSection: some View {
        Section("Experience") {
            Toggle("Microphone", isOn: $store.settings.keepMicEnabled)
            Toggle("Stats Overlay", isOn: $store.settings.showStatsOverlay)
            #if !os(tvOS)
            Toggle("Queue Live Activities", isOn: $store.settings.queueLiveActivitiesEnabled)
            #endif
            Toggle("Skip Server Selector", isOn: $store.settings.hideServerSelector)
        }
    }

    private var dataSection: some View {
        Section("Data") {
            Button {
                Task { await store.refreshCatalog() }
            } label: {
                Label(store.isLoadingGames ? "Reloading Catalog" : "Reload Catalog", systemImage: "arrow.clockwise")
            }
            .disabled(store.isLoadingGames)

            Button {
                store.clearImageCache()
            } label: {
                Label("Clear Image Cache", systemImage: "trash")
            }

            if !store.settings.defaultGameVariantIds.isEmpty {
                Button(role: .destructive) {
                    store.clearDefaultGameVariants()
                } label: {
                    Label("Clear Default Launchers", systemImage: "star.slash")
                }
            }

            if !store.settings.favoriteGameIds.isEmpty {
                Button(role: .destructive) {
                    store.clearFavorites()
                } label: {
                    Label("Clear Favorites", systemImage: "heart.slash")
                }
            }

            Button(role: .destructive) {
                showingResetConfirmation = true
            } label: {
                Label("Reset Settings", systemImage: "arrow.counterclockwise")
            }
        }
    }

    private func accountSection(_ user: UserProfile) -> some View {
        Section("Account") {
            LabeledContent("Name", value: user.displayName)
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
            Link("GitHub Repository", destination: URL(string: "https://github.com/OpenCloudGaming/OpenNOW")!)
        }
    }

    private var hdrAvailable: Bool {
        StreamSettingsResolver.isHDRAvailable(
            subscription: store.subscription,
            fallbackMembershipTier: store.user?.membershipTier
        )
    }

    private var currentMembershipTier: String? {
        store.subscription?.membershipTier ?? store.user?.membershipTier
    }

    private var headerSummary: String {
        let profile = StreamSettingsResolver.profile(for: store.settings, membershipTier: currentMembershipTier)
        return "\(profile.width)x\(profile.height) @ \(profile.fps) fps"
    }

    private func bitrateLabel(for value: Int) -> String {
        value == 0 ? "Auto" : "\(value) Mbps"
    }

    private func resolutionAvailable(_ choice: StreamSettingsResolver.StreamResolutionChoice) -> Bool {
        StreamSettingsResolver.isResolutionAvailable(choice, membershipTier: currentMembershipTier)
    }

    private func resolutionLabel(
        for choice: StreamSettingsResolver.StreamResolutionChoice,
        available: Bool
    ) -> String {
        guard !available, let plan = choice.requiredPlan.label else {
            return choice.label
        }
        return "\(choice.label) - \(plan)"
    }

    private func enforceAvailableResolution() {
        guard store.settings.preferredResolution != "Auto",
              let selected = StreamSettingsResolver.resolutionChoice(
                value: store.settings.preferredResolution,
                aspectRatio: store.settings.preferredAspectRatio
              ),
              !resolutionAvailable(selected) else {
            return
        }
        store.settings.preferredResolution = "Auto"
    }
}
