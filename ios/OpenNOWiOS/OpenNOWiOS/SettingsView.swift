import Foundation
import SwiftUI

private enum SettingsCategory: String, CaseIterable, Hashable, Identifiable {
    case general
    case stream
    case input
    case interface
    case advanced
    case account

    var id: String { rawValue }

    var title: String {
        switch self {
        case .general: return "General"
        case .stream: return "Stream"
        case .input: return "Input"
        case .interface: return "Interface"
        case .advanced: return "Advanced"
        case .account: return "Account"
        }
    }

    var summary: String {
        switch self {
        case .general: return "Updates, privacy, cache, and reset"
        case .stream: return "Resolution, FPS, codec, HDR, proxy"
        case .input: return "Mouse, keyboard, touch controls, rumble"
        case .interface: return "Color, cards, stats, controller UI"
        case .advanced: return "Diagnostics, debug logs, nerd tools"
        case .account: return "Sign-in, storage, connected stores"
        }
    }

    var symbolName: String {
        switch self {
        case .general: return "gearshape"
        case .stream: return "play.rectangle"
        case .input: return "keyboard"
        case .interface: return "rectangle.grid.2x2"
        case .advanced: return "stethoscope"
        case .account: return "person.crop.circle"
        }
    }

    var keywords: [String] {
        switch self {
        case .general:
            return ["general", "updates", "privacy", "cache", "reset", "data"]
        case .stream:
            return ["stream", "resolution", "aspect ratio", "fps", "bitrate", "codec", "color", "hdr", "sharpening", "region", "session proxy", "proxy", "l4s", "cloud g-sync", "native"]
        case .input:
            return ["input", "mouse", "keyboard", "layout", "language", "touch", "controller", "rumble"]
        case .interface:
            return ["interface", "ui", "stats", "server selector", "microphone", "queue", "live activities"]
        case .advanced:
            return ["advanced", "diagnostics", "debug", "logs", "codec", "native", "h264", "h265", "hevc", "av1"]
        case .account:
            return ["account", "login", "logout", "sign in", "saved", "provider", "membership", "subscription", "storage", "stores"]
        }
    }

    func matches(_ query: String) -> Bool {
        let tokens = query
            .lowercased()
            .split(whereSeparator: \.isWhitespace)
            .map(String.init)
        guard !tokens.isEmpty else { return true }
        let haystack = ([title, summary] + keywords).joined(separator: " ").lowercased()
        return tokens.allSatisfy { haystack.contains($0) }
    }
}

struct SettingsView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @Environment(\.openURL) private var openURL
    @State private var searchText = ""
    @State private var showingResetConfirmation = false
    @State private var showingSignOutAllConfirmation = false
    @State private var connectorPendingDisconnect: AccountConnector?

    private let fpsValues = [30, 60, 120]
    private let qualityValues = ["Balanced", "Data Saver", "Quality"]
    private let codecValues = ["Auto", "H264", "H265", "AV1"]
    private let regionValues = ["Auto", "US East", "US West", "Europe", "Asia"]

    var body: some View {
        NavigationStack {
            List {
                accountLandingSection
                settingsCategorySection
                aboutSection
                thanksSection
            }
            .navigationTitle("Settings")
            .searchable(text: $searchText, prompt: "Search settings")
            .navigationDestination(for: SettingsCategory.self) { category in
                Form {
                    settingsDetailContent(for: category)
                }
                .navigationTitle(category.title)
                .navigationBarTitleDisplayMode(.inline)
            }
            .onChangeCompat(of: store.settings) { _ in
                store.persistSettings()
            }
            .onChangeCompat(of: store.settings.queueLiveActivitiesEnabled) { _ in
                store.refreshTrackedSessionSurface()
            }
            .onAppear {
                enforceAvailableResolution()
            }
            .onChangeCompat(of: store.settings.preferredAspectRatio) { _ in
                enforceAvailableResolution()
            }
            .onChangeCompat(of: currentMembershipTier ?? "") { _ in
                enforceAvailableResolution()
            }
            .confirmationDialog("Reset settings?", isPresented: $showingResetConfirmation, titleVisibility: .visible) {
                Button("Reset Settings", role: .destructive) {
                    store.resetSettings()
                }
                Button("Cancel", role: .cancel) {}
            }
            .confirmationDialog("Sign out all accounts?", isPresented: $showingSignOutAllConfirmation, titleVisibility: .visible) {
                Button("Sign Out All Accounts", role: .destructive) {
                    store.signOutAll()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This removes every saved GeForce NOW account from this device.")
            }
            .alert(
                "Disconnect account?",
                isPresented: Binding(
                    get: { connectorPendingDisconnect != nil },
                    set: { isPresented in
                        if !isPresented {
                            connectorPendingDisconnect = nil
                        }
                    }
                )
            ) {
                Button("Disconnect", role: .destructive) {
                    if let connector = connectorPendingDisconnect {
                        Task { await store.disconnectAccountConnector(connector) }
                    }
                    connectorPendingDisconnect = nil
                }
                Button("Cancel", role: .cancel) {
                    connectorPendingDisconnect = nil
                }
            } message: {
                Text("This removes the linked account from GeForce NOW. You can connect it again later.")
            }
            .task {
                await store.refreshAccountConnectors()
            }
        }
    }

    @ViewBuilder
    private var accountLandingSection: some View {
        if accountLandingVisible {
            Section {
                NavigationLink(value: SettingsCategory.account) {
                    HStack(spacing: 14) {
                        ZStack {
                            Circle()
                                .fill(Color.accentColor)
                            Text(accountLandingInitial)
                                .font(.title3.weight(.bold))
                                .foregroundStyle(.white)
                        }
                        .frame(width: 52, height: 52)

                        VStack(alignment: .leading, spacing: 3) {
                            Text(accountLandingTitle)
                                .font(.body.weight(.semibold))
                                .lineLimit(1)
                            Text(accountLandingDetail)
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                    .padding(.vertical, 4)
                }
            }
        }
    }

    private var settingsCategorySection: some View {
        Section {
            ForEach(visibleLandingCategories) { category in
                NavigationLink(value: category) {
                    SettingsCategoryLabel(category: category)
                }
            }
        }
    }

    @ViewBuilder
    private func settingsDetailContent(for category: SettingsCategory) -> some View {
        switch category {
        case .general:
            appUpdatesSection
            privacySection
            dataSection
        case .stream:
            streamingSection
        case .input:
            inputSection
        case .interface:
            interfaceSection
        case .advanced:
            advancedSection
        case .account:
            savedAccountsSection
            accountActionsSection
            storageAddonSection
            accountConnectorsSection
        }
    }

    private var visibleLandingCategories: [SettingsCategory] {
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return landingCategories }
        return landingCategories.filter { $0.matches(trimmed) }
    }

    private var accountLandingVisible: Bool {
        let trimmed = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty || SettingsCategory.account.matches(trimmed)
    }

    private var savedAccountsSection: some View {
        Section {
            if accountRows.isEmpty {
                Text("No saved accounts on this device.")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(accountRows) { account in
                    savedAccountRow(account)
                }
            }
        } header: {
            Text("Saved Accounts")
        } footer: {
            Text("Switching accounts updates launches, library sync, cloud storage, and store connections.")
        }
    }

    private var accountActionsSection: some View {
        Section("Account Actions") {
            HStack {
                Button {
                    Task { await store.signIn() }
                } label: {
                    Label("Add Account", systemImage: "person.badge.plus")
                }
                .disabled(store.isAuthenticating)

                Spacer()

                if store.isAuthenticating {
                    ProgressView()
                }
            }

            if store.user != nil {
                Button(role: .destructive) {
                    store.signOut()
                } label: {
                    Label("Sign Out Current Account", systemImage: "rectangle.portrait.and.arrow.right")
                }
            }

            if store.savedAccounts.count > 1 {
                Button(role: .destructive) {
                    showingSignOutAllConfirmation = true
                } label: {
                    Label("Sign Out All Accounts", systemImage: "person.2.slash")
                }
            }
        }
    }

    private var storageAddonSection: some View {
        Section("Cloud Storage") {
            if let storage = store.subscription?.storageAddon {
                if let size = storage.sizeGb {
                    LabeledContent("Total", value: formatStorageGb(size))
                }
                if let used = storage.usedGb {
                    LabeledContent("Used", value: formatStorageGb(used))
                }
                if let size = storage.sizeGb, let used = storage.usedGb {
                    LabeledContent("Available", value: formatStorageGb(max(size - used, 0)))
                }
                if let usage = storage.usageFraction {
                    VStack(alignment: .leading, spacing: 6) {
                        HStack {
                            Text("Storage usage")
                            Spacer()
                            Text(formatStoragePercent(usage))
                                .foregroundStyle(.secondary)
                        }
                        ProgressView(value: usage)
                            .tint(storageUsageTint(usage))
                    }
                }
                if let region = storage.regionName, !region.isEmpty {
                    LabeledContent("Location", value: region)
                }
                Link(destination: store.cloudStorageManagementURL) {
                    Label("Manage Storage", systemImage: "externaldrive")
                }
                Link(destination: store.cloudStorageResetURL) {
                    Label("Reset Storage", systemImage: "arrow.counterclockwise")
                }
                Link(destination: store.cloudStorageManagementURL) {
                    Label("Change Storage Location", systemImage: "location")
                }
            } else {
                Text("No persistent storage add-on is active for this account.")
                    .foregroundStyle(.secondary)
                Link(destination: store.cloudStorageAddURL) {
                    Label("Add Storage", systemImage: "plus.circle")
                }
            }
        }
    }

    private var accountConnectorsSection: some View {
        Section("Game Store Connections") {
            if store.loadingAccountConnectors && store.accountConnectors.isEmpty {
                HStack {
                    ProgressView()
                    Text("Loading connections")
                }
            } else if store.accountConnectors.isEmpty {
                OpenNOWUnavailableView(
                    "No Connections Found",
                    systemImage: "link.badge.plus"
                ) {
                    Text("Refresh to load supported store connections for this account.")
                }
                .frame(maxWidth: .infinity)
            } else {
                ForEach(store.accountConnectors.prefix(8)) { connector in
                    HStack(spacing: 12) {
                        Image(systemName: connector.isLinked ? "checkmark.circle.fill" : "link.circle")
                            .foregroundStyle(connector.isLinked ? .green : .secondary)
                            .frame(width: 24)

                        VStack(alignment: .leading, spacing: 2) {
                            Text(connector.label)
                                .font(.body.weight(.semibold))
                            Text(connectorStatusText(connector))
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .truncationMode(.middle)
                        }

                        Spacer(minLength: 8)

                        if store.connectorActionStore == connector.store {
                            ProgressView()
                        } else if connector.isLinked {
                            Button("Disconnect", role: .destructive) {
                                connectorPendingDisconnect = connector
                            }
                            .buttonStyle(.bordered)
                        } else if connector.supported {
                            Button("Connect") {
                                Task {
                                    await store.connectAccountConnector(connector) { url in
                                        openURL(url)
                                    }
                                }
                            }
                            .buttonStyle(.borderedProminent)
                        }
                    }
                    .padding(.vertical, 3)
                }
            }

            Button {
                Task { await store.refreshAccountConnectors() }
            } label: {
                Label(store.loadingAccountConnectors ? "Refreshing" : "Refresh Connections", systemImage: "arrow.clockwise")
            }
            .disabled(store.loadingAccountConnectors)

            Link(destination: store.accountHelpURL) {
                Label("Account Help", systemImage: "questionmark.circle")
            }
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

            Toggle("HDR", isOn: $store.settings.hdrEnabled)
                .disabled(!hdrAvailable)

            Toggle("Cloud G-Sync", isOn: $store.settings.enableCloudGsync)

            Toggle("L4S Low Latency", isOn: $store.settings.enableL4S)

            Toggle("Native Streamer", isOn: $store.settings.nativeStreamerEnabled)

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
            Text("Stream")
        } footer: {
            VStack(alignment: .leading, spacing: 4) {
                Text("Resolutions above your current plan are shown but unavailable.")
                if !hdrAvailable {
                    Text("HDR requires an Ultimate-capable account.")
                } else if store.settings.nativeStreamerEnabled {
                    Text("Native streaming is experimental. Turn it off if stream setup fails.")
                }
            }
        }
    }

    private var inputSection: some View {
        Section("Input") {
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

            #if !os(tvOS)
            Toggle("Fortnite Mobile Touch", isOn: $store.settings.fortnitePrefersNativeTouch)
            Toggle("Touch Controller", isOn: $store.settings.streamerPreferences.touchControllerVisible)
            Toggle("Touchscreen Mode", isOn: $store.settings.streamerPreferences.touchscreenModeEnabled)
            touchLayoutSlider("Touch Layout Scale", keyPath: \.scale, range: 0.6...1.4, step: 0.05)
            touchLayoutSlider("Touch Button Size", keyPath: \.buttonScale, range: 0.65...1.5, step: 0.05)
            touchLayoutSlider("Touch Stick Size", keyPath: \.stickScale, range: 0.65...1.5, step: 0.05)
            touchLayoutSlider("Touch Opacity", keyPath: \.opacity, range: 0.15...1, step: 0.05)
            #endif

            LabeledContent("Controller", value: "Automatic")
        }
    }

    private var interfaceSection: some View {
        Section("Interface") {
            Toggle("Nerd Mode", isOn: $store.settings.nerdMode)
            Toggle("Microphone", isOn: $store.settings.keepMicEnabled)
            Toggle("Stats Overlay", isOn: $store.settings.showStatsOverlay)
            #if !os(tvOS)
            Toggle("Queue Live Activities", isOn: $store.settings.queueLiveActivitiesEnabled)
            #endif
            Toggle("Skip Server Selector", isOn: $store.settings.hideServerSelector)
        }
    }

    private var landingCategories: [SettingsCategory] {
        var categories: [SettingsCategory] = [.general, .stream, .input, .interface]
        if store.settings.nerdMode {
            categories.append(.advanced)
        }
        return categories
    }

    private var appUpdatesSection: some View {
        Section("App Updates") {
            LabeledContent("Updates", value: "Managed by iOS")
            LabeledContent("Version", value: appVersion)
            LabeledContent("Build", value: buildNumber)
        }
    }

    private var privacySection: some View {
        Section("Privacy") {
            LabeledContent("Usage Analytics", value: "Not Collected")
        }
    }

    private var advancedSection: some View {
        Section("Codec Diagnostics") {
            LabeledContent("Preferred Codec", value: store.settings.preferredCodec)
            LabeledContent("Stream Profile", value: headerSummary)
            LabeledContent("Color", value: selectedColorQualityLabel)
            LabeledContent("HDR", value: hdrAvailable ? (store.settings.hdrEnabled ? "On" : "Off") : "Unavailable")
            LabeledContent("Native Streamer", value: store.settings.nativeStreamerEnabled ? "On" : "Off")
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

    private var aboutSection: some View {
        Section("About") {
            LabeledContent("OpenNOW iOS", value: "Version \(appVersion)")
            LabeledContent("Build", value: buildNumber)
            LabeledContent("Platform", value: OpenNOWPlatform.displayName)
            Link(destination: URL(string: "https://github.com/OpenCloudGaming/OpenNOW")!) {
                Label("OpenNOW Repository", systemImage: "chevron.left.forwardslash.chevron.right")
            }
            Link(destination: URL(string: "https://github.com/Kief5555")!) {
                Label("Kiefer", systemImage: "person.crop.circle")
            }
            Link(destination: URL(string: "https://github.com/zortos293")!) {
                Label("Zortos", systemImage: "person.crop.circle")
            }
        }
    }

    private var thanksSection: some View {
        Section("Thanks") {
            Text("Thanks to the people helping improve OpenNOW for everyone.")
                .foregroundStyle(.secondary)
            LabeledContent("DarkevilPT", value: "Community support")
            Link(destination: URL(string: "https://paypal.me/PrintedWaste")!) {
                Label("Donate with PayPal", systemImage: "heart")
            }
        }
    }

    private func savedAccountRow(_ account: SavedAccount) -> some View {
        let selected = account.userId == store.user?.userId

        return VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 12) {
                ZStack {
                    Circle()
                        .fill(selected ? Color.accentColor : Color.secondary.opacity(0.24))
                    Text(accountInitial(account))
                        .font(.headline.weight(.bold))
                        .foregroundStyle(selected ? .white : .primary)
                }
                .frame(width: 38, height: 38)

                VStack(alignment: .leading, spacing: 2) {
                    Text(account.displayName.isEmpty ? "NVIDIA Account" : account.displayName)
                        .font(.body.weight(.semibold))
                        .lineLimit(1)
                    Text(accountSubtitle(account))
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }

                Spacer(minLength: 8)

                if selected {
                    Text("Active")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.green)
                } else {
                    Button("Switch") {
                        Task { await store.switchAccount(to: account.userId) }
                    }
                    .buttonStyle(.bordered)
                }
            }
        }
        .padding(.vertical, 3)
    }

    private func touchLayoutSlider(
        _ title: String,
        keyPath: WritableKeyPath<TouchControlLayout, Double>,
        range: ClosedRange<Double>,
        step: Double
    ) -> some View {
        let value = store.settings.touchLayout(for: "default")[keyPath: keyPath]

        return VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(title)
                Spacer()
                Text(percentLabel(value))
                    .foregroundStyle(.secondary)
            }
            Slider(value: touchLayoutBinding(keyPath), in: range, step: step)
        }
    }

    private func touchLayoutBinding(
        _ keyPath: WritableKeyPath<TouchControlLayout, Double>,
        profile: String = "default"
    ) -> Binding<Double> {
        Binding(
            get: {
                store.settings.touchLayout(for: profile)[keyPath: keyPath]
            },
            set: { newValue in
                var settings = store.settings
                var layout = settings.touchLayout(for: profile)
                layout[keyPath: keyPath] = newValue
                settings.touchControlLayouts[profile] = layout
                store.settings = settings
            }
        )
    }

    private var accountRows: [SavedAccount] {
        if !store.savedAccounts.isEmpty {
            return store.savedAccounts
        }
        guard let user = store.user else { return [] }
        let fallbackProvider = store.authProviderCode ?? "NVIDIA"
        return [
            SavedAccount(
                userId: user.userId,
                displayName: user.displayName,
                email: user.email,
                membershipTier: store.subscription?.membershipTier ?? user.membershipTier,
                providerCode: fallbackProvider
            )
        ]
    }

    private var landingAccount: SavedAccount? {
        accountRows.first { $0.userId == store.user?.userId } ?? accountRows.first
    }

    private var accountLandingTitle: String {
        landingAccount?.displayName.nonEmpty
            ?? store.user?.displayName.nonEmpty
            ?? "NVIDIA Account"
    }

    private var accountLandingDetail: String {
        let email = landingAccount?.email?.nonEmpty ?? store.user?.email?.nonEmpty
        let tier = store.subscription?.membershipTier.nonEmpty
            ?? landingAccount?.membershipTier.nonEmpty
            ?? store.user?.membershipTier.nonEmpty
        let detail = [email, tier].compactMap { $0 }.joined(separator: " - ")
        if !detail.isEmpty {
            return detail
        }
        return store.user == nil && landingAccount == nil ? "Sign in to sync your GeForce NOW account" : "Manage account"
    }

    private var accountLandingInitial: String {
        if let first = accountLandingTitle.first {
            return String(first).uppercased()
        }
        return "N"
    }

    private func accountInitial(_ account: SavedAccount) -> String {
        if let first = account.displayName.first ?? account.email?.first {
            return String(first).uppercased()
        }
        return "N"
    }

    private func accountSubtitle(_ account: SavedAccount) -> String {
        [account.email, account.providerCode, account.membershipTier]
            .compactMap { value in
                let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                return trimmed.isEmpty ? nil : trimmed
            }
            .joined(separator: " - ")
    }

    private func formatStorageGb(_ value: Double) -> String {
        value == floor(value) ? "\(Int(value)) GB" : String(format: "%.1f GB", value)
    }

    private func formatStoragePercent(_ value: Double) -> String {
        "\(Int((value * 100).rounded()))% used"
    }

    private func percentLabel(_ value: Double) -> String {
        "\(Int((value * 100).rounded()))%"
    }

    private func storageUsageTint(_ value: Double) -> Color {
        if value >= 0.9 { return .red }
        if value >= 0.75 { return .orange }
        return .accentColor
    }

    private func connectorStatusText(_ connector: AccountConnector) -> String {
        if connector.isLinked {
            let identity = cleanConnectorIdentity(connector.userDisplayName)
                ?? cleanConnectorIdentity(connector.userIdentifier)
            let sync = connector.syncedGameCount.map { "\($0) synced games" }
                ?? cleanConnectorState(connector.syncState)
            let summary = [identity, sync]
                .compactMap(\.self)
                .joined(separator: " - ")
            return summary.isEmpty ? "Connected" : summary
        }
        if connector.required {
            return "Required for some launches"
        }
        return connector.supported ? "Not connected" : "Unsupported"
    }

    private func cleanConnectorIdentity(_ value: String?) -> String? {
        guard var cleaned = value?.trimmingCharacters(in: .whitespacesAndNewlines), !cleaned.isEmpty else {
            return nil
        }
        cleaned = cleaned
            .replacingOccurrences(of: "[N/A]", with: "")
            .replacingOccurrences(of: "(N/A)", with: "")
            .replacingOccurrences(of: "N/A", with: "")
            .trimmingCharacters(in: CharacterSet(charactersIn: " -_/[]()").union(.whitespacesAndNewlines))
        guard !cleaned.isEmpty, cleaned.lowercased() != "null" else {
            return nil
        }
        return cleaned.contains("@") ? nil : cleaned
    }

    private func cleanConnectorState(_ value: String?) -> String? {
        guard let state = value?.trimmingCharacters(in: .whitespacesAndNewlines), !state.isEmpty else {
            return nil
        }
        switch state.uppercased() {
        case "SYNC_SUCCESS", "SUCCESS", "CONNECTED":
            return "Synced"
        case "SYNC_FAILED", "FAILED":
            return "Sync failed"
        case "SYNCING", "IN_PROGRESS":
            return "Syncing"
        default:
            return state
                .replacingOccurrences(of: "_", with: " ")
                .lowercased()
                .capitalized
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

    private var selectedColorQualityLabel: String {
        StreamColorQuality(rawValue: store.settings.preferredColorQuality)?.label ?? store.settings.preferredColorQuality
    }

    private var appVersion: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "1.0"
    }

    private var buildNumber: String {
        Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String ?? "1"
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

private struct SettingsCategoryLabel: View {
    let category: SettingsCategory

    var body: some View {
        Label {
            VStack(alignment: .leading, spacing: 2) {
                Text(category.title)
                    .font(.body.weight(.medium))
                Text(category.summary)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        } icon: {
            Image(systemName: category.symbolName)
                .frame(width: 28)
        }
        .padding(.vertical, 4)
    }
}

private extension String {
    var nonEmpty: String? {
        let trimmed = trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}
