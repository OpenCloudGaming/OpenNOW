import Foundation
import SwiftUI
#if canImport(UIKit)
import UIKit
#endif
#if os(iOS)
import PhotosUI
import UniformTypeIdentifiers
#endif

private enum SettingsCategory: String, CaseIterable, Hashable, Identifiable {
    case general
    case stream
    case input
    case interface
    case advanced
    case account
    case about

    var id: String { rawValue }

    var title: String {
        switch self {
        case .general: return "General"
        case .stream: return "Stream"
        case .input: return "Input"
        case .interface: return "Interface"
        case .advanced: return "Advanced"
        case .account: return "Account"
        case .about: return "About"
        }
    }

    var summary: String {
        switch self {
        case .general: return "Language, privacy, and app data"
        case .stream: return "Resolution, codec, HDR, region"
        case .input: return "Mouse, keyboard, touch, controller"
        case .interface: return "Accent, cards, stats HUD, sound"
        case .advanced: return "Experimental features and diagnostics"
        case .account: return "Sign-in, storage, connected stores"
        case .about: return "Version, links, and thanks"
        }
    }

    var symbolName: String {
        switch self {
        case .general: return "gearshape"
        case .stream: return "play.rectangle"
        case .input: return "keyboard"
        case .interface: return "paintpalette"
        case .advanced: return "flask"
        case .account: return "person.crop.circle"
        case .about: return "info.circle"
        }
    }

    var keywords: [String] {
        switch self {
        case .general:
            return ["general", "privacy", "analytics", "telemetry", "usage", "cache", "reset", "data", "tutorial", "updates"]
        case .stream:
            return ["stream", "preset", "recommended", "resolution", "aspect ratio", "fps", "frame rate", "bitrate", "data", "codec", "color", "hdr", "sharpening", "sharpness", "region", "server", "session proxy", "proxy", "native decoder", "hud", "gpu", "performance overlay", "metal", "stretch"]
        case .input:
            return ["input", "mouse", "sensitivity", "acceleration", "scroll", "pointer", "keyboard", "layout", "language", "clipboard", "paste", "touch", "native touch", "joystick", "stick", "dead zone", "aim", "controller", "rumble", "haptics", "tutorial", "guide", "replay"]
        case .interface:
            return ["interface", "ui", "accent", "color", "theme", "expressive", "outline", "cards", "titles", "favorites", "favourites", "store labels", "card size", "launch page", "stats", "hud", "metrics", "position", "afk", "idle", "keep awake", "server selector", "queue", "live activities", "sound", "chime", "ready", "catalog", "wallpaper", "background", "photo", "session report", "counter"]
        case .advanced:
            return ["advanced", "experimental", "l4s", "cloud g-sync", "gsync", "diagnostics", "debug", "logs", "codec", "probe", "decoder", "decoders", "hardware", "native", "h264", "h265", "hevc", "av1"]
        case .account:
            return ["account", "login", "logout", "sign in", "saved", "provider", "membership", "subscription", "storage", "hours", "play time", "stores", "steam", "epic", "xbox"]
        case .about:
            return ["about", "version", "build", "github", "repository", "developer", "kiefer", "zortos", "thanks", "credits", "contributors", "community", "licence", "license"]
        }
    }

    init(_ route: SettingsRouteTarget) {
        switch route {
        case .account: self = .account
        case .general: self = .general
        case .stream: self = .stream
        case .input: self = .input
        case .interface: self = .interface
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
    // Match the type-erased path storage used by the surrounding tab/split view.
    // Typed category arrays can trap when SwiftUI compares column paths.
    @State private var path = NavigationPath()
    @State private var bugReportDeck: BugReportPreflightDeck?
    @State private var searchText = ""
    @State private var showingResetConfirmation = false
    @State private var showingResetAppConfirmation = false
    @State private var showingSignOutAllConfirmation = false
    @State private var connectorPendingDisconnect: AccountConnector?
    #if os(iOS)
    @State private var selectedCatalogWallpaperItem: PhotosPickerItem?
    @State private var catalogWallpaperImportInProgress = false
    @State private var catalogWallpaperImportError: String?
    @State private var diagnosticsDocument = DiagnosticsTextDocument(text: "")
    @State private var diagnosticsExportInProgress = false
    @State private var showingDiagnosticsExporter = false
    @State private var diagnosticsExportError: String?
    @State private var diagnosticsExportStatus: String?
    #endif

    private let qualityValues = ["Balanced", "Data Saver", "Quality"]

    private var codecValues: [String] {
        let report = NativeStreamCodecProbe.report()
        var values = ["Auto", "H264"]
        if report.capability(for: .h265)?.launchSafe == true {
            values.append("H265")
        }
        if report.capability(for: .av1)?.launchSafe == true {
            values.append("AV1")
        }
        return values
    }

    var body: some View {
        NavigationStack(path: $path) {
            List {
                accountLandingSection
                settingsCategorySection
            }
            .navigationTitle("Settings")
            .searchable(text: $searchText, prompt: "Search settings")
            .onChangeCompat(of: store.pendingSettingsRoute) { route in
                guard let route else { return }
                // Search filters the category list, so an inbound route has to clear it or the
                // destination the caller asked for may not be reachable.
                searchText = ""
                path = NavigationPath([SettingsCategory(route)])
                store.pendingSettingsRoute = nil
            }
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
                enforceAvailableFPS()
                enforceAvailableHDR()
                enforceAvailableCodec()
            }
            .onChangeCompat(of: store.settings.preferredAspectRatio) { _ in
                enforceAvailableResolution()
            }
            .onChangeCompat(of: currentMembershipTier ?? "") { _ in
                enforceAvailableResolution()
                enforceAvailableFPS()
                enforceAvailableHDR()
            }
            .onChangeCompat(of: store.settings.hdrEnabled) { enabled in
                guard enabled else { return }
                store.settings.preferredColorQuality = StreamColorQuality.tenBit420.rawValue
                if NativeStreamCodecProbe.report().capability(for: .h265)?.launchSafe == true {
                    store.settings.preferredCodec = "H265"
                }
            }
            .confirmationDialog("Reset settings?", isPresented: $showingResetConfirmation, titleVisibility: .visible) {
                Button("Reset Settings", role: .destructive) {
                    store.resetSettings()
                }
                Button("Cancel", role: .cancel) {}
            }
            .confirmationDialog("Reset app?", isPresented: $showingResetAppConfirmation, titleVisibility: .visible) {
                Button("Reset App", role: .destructive) {
                    store.resetApp()
                }
                Button("Cancel", role: .cancel) {}
            } message: {
                Text("This clears local settings, saved accounts, launch choices, favorites, session state, and cached images on this device.")
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
        // Presented from the stack root so both entry points — About and Advanced — reach it.
        .sheet(item: $bugReportDeck) { deck in
            BugReportView(deck: deck) { draft in
                await store.submitBugReport(draft, deck: deck)
            }
            .environmentObject(store)
        }
    }

    @ViewBuilder
    private var accountLandingSection: some View {
        if accountLandingVisible {
            Section {
                NavigationLink(value: SettingsCategory.account) {
                    HStack(spacing: 14) {
                        SettingsAccountAvatar(
                            initial: accountLandingInitial,
                            size: 52,
                            selected: true
                        )

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
            privacySection
            dataSection
        case .stream:
            streamQualitySection
            streamVideoSection
            streamConnectionSection
        case .input:
            inputAudioKeyboardSection
            inputPointerSection
            inputTouchControllerSection
        case .interface:
            interfaceAppearanceSection
            interfaceCatalogSection
            interfaceStatsSection
            interfaceSoundSessionSection
        case .advanced:
            experimentalSection
            reportProblemSection
            advancedSection
        case .account:
            savedAccountsSection
            accountActionsSection
            playTimeSection
            storageAddonSection
            accountConnectorsSection
        case .about:
            aboutSection
            thanksSection
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
                    Task { await store.signIn(forceAccountSelection: true) }
                } label: {
                    Label("Add Account", systemImage: "person.badge.plus")
                }
                .disabled(store.isAuthenticating)

                Spacer()

                if store.isAuthenticating {
                    HStack(spacing: 8) {
                        ProgressView()
                        Text("Waiting for NVIDIA")
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            if store.isAuthenticating {
                Text("Take your time completing sign-in. OpenNOW will wait up to five minutes for NVIDIA to return to the app.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }

            if let error = store.lastError, !error.isEmpty {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .font(.footnote)
                    .foregroundStyle(.red)
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
                Text("Storage purchases and changes are managed outside OpenNOW in your GeForce NOW account.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            } else {
                Text("No persistent storage add-on is active for this account.")
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var playTimeSection: some View {
        Section("Play Time") {
            LabeledContent("Membership", value: currentMembershipTier ?? "Unknown")
            LabeledContent("Session Limit", value: sessionLimitDescription)
            if let subscription = store.subscription, subscription.totalHours > 0 {
                LabeledContent("Monthly Allowance", value: hoursLabel(subscription.totalHours))
                LabeledContent("Monthly Remaining", value: hoursLabel(subscription.remainingHours))
                ProgressView(
                    value: min(max(subscription.totalHours - subscription.remainingHours, 0), subscription.totalHours),
                    total: subscription.totalHours
                )
            }
            if let startedAt = store.activeSession?.startedAt {
                // Ticks locally. The store used to publish a new second to every observer once a
                // second so this one row could count; the system animates this without anyone
                // being told anything.
                LabeledContent("Current Session") {
                    Text(startedAt, style: .timer)
                        .monospacedDigit()
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
                        StoreGlyph(store: connector.store)
                            .frame(width: 30, height: 30)
                            .accessibilityHidden(true)

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

    // MARK: Stream

    private var streamQualitySection: some View {
        Section {
            Picker("Preset", selection: streamPresetBinding) {
                ForEach(StreamPreset.allCases) { preset in
                    Text(preset.label).tag(preset)
                }
            }

            Picker("Aspect Ratio", selection: customStreamBinding(\.preferredAspectRatio)) {
                ForEach(StreamSettingsResolver.aspectRatioOptions, id: \.self) { value in
                    Text(value).tag(value)
                }
            }

            Picker("Resolution", selection: customStreamBinding(\.preferredResolution)) {
                Text("Auto").tag("Auto")
                ForEach(StreamSettingsResolver.choices(forAspectRatio: store.settings.preferredAspectRatio)) { choice in
                    let available = resolutionAvailable(choice)
                    Text(resolutionLabel(for: choice, available: available))
                        .foregroundStyle(available ? .primary : .secondary)
                        .tag(choice.value)
                        .disabled(!available)
                }
            }

            Picker("Frame Rate", selection: customStreamBinding(\.preferredFPS)) {
                ForEach(fpsValues, id: \.self) { Text("\($0) fps").tag($0) }
            }

            Picker("Quality", selection: customStreamBinding(\.preferredQuality)) {
                ForEach(qualityValues, id: \.self) { Text($0).tag($0) }
            }

            Picker("Max Bitrate", selection: customStreamBinding(\.maxBitrateMbps)) {
                ForEach(StreamSettingsResolver.bitrateOptionsMbps, id: \.self) { bitrate in
                    Text(bitrateLabel(for: bitrate)).tag(bitrate)
                }
            }

            Toggle("Stretch to Fill", isOn: $store.settings.streamerPreferences.stretchStreamToFill)
        } header: {
            Text("Quality")
        } footer: {
            VStack(alignment: .leading, spacing: 4) {
                Text(store.settings.streamPreset.detail)
                Text("Streaming at \(headerSummary) uses about \(dataUsageEstimate).")
                Text("Resolutions above your plan stay visible but cannot be selected.")
            }
        }
    }

    private var streamVideoSection: some View {
        Section {
            Picker("Codec", selection: codecSelectionBinding) {
                ForEach(codecValues, id: \.self) { Text($0).tag($0) }
            }

            Picker("Color", selection: customStreamBinding(\.preferredColorQuality)) {
                ForEach([StreamColorQuality.eightBit420, .tenBit420]) { color in
                    Text(color.label).tag(color.rawValue)
                }
            }

            Toggle("HDR", isOn: $store.settings.hdrEnabled)
                .disabled(!hdrAvailable)

            Toggle("Stream Sharpening", isOn: $store.settings.streamSharpeningEnabled)

            if store.settings.streamSharpeningEnabled {
                VStack(alignment: .leading, spacing: 8) {
                    LabeledContent(
                        "Sharpness",
                        value: "\(Int((store.settings.streamSharpeningAmount * 100).rounded()))%"
                    )
                    .monospacedDigit()
                    Slider(value: $store.settings.streamSharpeningAmount, in: 0...1, step: 0.05)
                        .accessibilityLabel("Sharpness amount")
                        .accessibilityValue("\(Int((store.settings.streamSharpeningAmount * 100).rounded())) percent")
                }
            }

            Toggle("Native Low-Latency Decoder", isOn: $store.settings.nativeStreamerEnabled)
            Toggle("Apple Performance HUD", isOn: $store.settings.showMetalPerformanceHUD)
        } header: {
            Text("Video")
        } footer: {
            VStack(alignment: .leading, spacing: 4) {
                if !hdrAvailable {
                    Text(hdrUnavailableReason)
                }
                Text("The native decoder cuts a frame or two of latency. Turn it off if the picture tears or stutters.")
                Text("The Apple performance HUD is the system's own GPU readout drawn over the video. It is off unless you turn it on here — OpenNOW's stats overlay covers what most people need, and the system one sits on top of the game.")
            }
        }
    }

    private var streamConnectionSection: some View {
        Section {
            Picker("Region", selection: $store.settings.preferredRegion) {
                Text("Automatic").tag("")
                if !store.settings.preferredRegion.isEmpty,
                   !store.availableRegions.contains(where: { $0.url == store.settings.preferredRegion }) {
                    Text("Saved Region").tag(store.settings.preferredRegion)
                }
                ForEach(store.availableRegions) { region in
                    Text(region.name).tag(region.url)
                }
            }

            Toggle("Session Proxy", isOn: $store.settings.sessionProxyEnabled)

            if store.settings.sessionProxyEnabled {
                TextField("Proxy URL", text: $store.settings.sessionProxyUrl)
                    .textInputAutocapitalization(.never)
                    .keyboardType(.URL)
                    .autocorrectionDisabled()
                    .accessibilityLabel("Session proxy address")
            }
        } header: {
            Text("Connection")
        } footer: {
            Text("A session proxy routes launch requests through a server you control. Leave it off unless you know you need it — a wrong address stops games from starting.")
        }
    }

    /// Bitrate in megabits per second converted to the number people are actually billed on.
    private var dataUsageEstimate: String {
        let profile = StreamSettingsResolver.profile(for: store.settings, membershipTier: currentMembershipTier)
        let mbps = store.settings.maxBitrateMbps > 0
            ? Double(store.settings.maxBitrateMbps)
            : Double(profile.maxBitrateKbps) / 1_000
        guard mbps > 0 else { return "an automatic amount of data" }
        let gbPerHour = mbps * 3_600 / 8 / 1_000
        return String(format: "%.1f GB per hour", gbPerHour)
    }

    private var hdrUnavailableReason: String {
        if !StreamSettingsResolver.isHDRAvailable(
            subscription: store.subscription,
            fallbackMembershipTier: store.user?.membershipTier
        ) {
            return "HDR needs a Performance or Ultimate membership."
        }
        if NativeStreamCodecProbe.report().capability(for: .h265)?.launchSafe != true {
            return "HDR needs an H.265 stream, and this device cannot decode one safely."
        }
        return "This display cannot show HDR."
    }

    // MARK: Input

    private var inputAudioKeyboardSection: some View {
        Section {
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

            Toggle("Paste Into Games", isOn: $store.settings.clipboardPasteEnabled)
        } header: {
            Text("Keyboard")
        } footer: {
            Text("Paste sends your clipboard to the game as typed text, from the stream controls. The keyboard layout has to match the one the game expects, not the one on your device. Microphone input is not available yet — GeForce NOW's stream offer carries no upstream audio track.")
        }
    }

    private var inputPointerSection: some View {
        Section {
            settingsSlider(
                "Mouse Sensitivity",
                value: $store.settings.mouseSensitivity,
                range: 0.25...3,
                step: 0.05,
                format: { String(format: "%.2f×", $0) }
            )

            Picker("Mouse Acceleration", selection: $store.settings.mouseAcceleration) {
                Text("Off").tag(0)
                Text("Standard").tag(1)
                Text("High").tag(2)
            }

            settingsSlider(
                "Scroll Sensitivity",
                value: Binding(
                    get: { Double(store.settings.mouseScrollSensitivity) },
                    set: { store.settings.mouseScrollSensitivity = Int($0.rounded()) }
                ),
                range: 10...100,
                step: 5,
                format: { _ in scrollSensitivityLabel }
            )

            #if !os(tvOS)
            Toggle("Finger Mouse", isOn: $store.settings.fingerMouseEnabled)

            if store.settings.fingerMouseEnabled {
                Toggle("Tap Clicks Where You Touch", isOn: $store.settings.touch.mouseDirectClick)
            }
            #endif

            Toggle("Controller Mouse Mode", isOn: $store.settings.controllerMouseEmulation)
        } header: {
            Text("Pointer")
        } footer: {
            Text("Controller mouse mode maps the left stick to the cursor, the right stick to scrolling, A to click and B to right-click. You can also switch it on for a single session from the stream controls.")
        }
    }

    private var inputTouchControllerSection: some View {
        Section {
            #if !os(tvOS)
            Picker("Touch Mode", selection: $store.settings.touch.nativeTouchMode) {
                ForEach(NativeTouchMode.allCases) { mode in
                    Text(mode.label).tag(mode)
                }
            }

            if store.settings.touch.nativeTouchMode != .never {
                settingsSlider(
                    "Touch Scroll Speed",
                    value: $store.settings.touch.nativeTouchScrollScale,
                    range: 0.25...2,
                    step: 0.05,
                    format: { _ in touchScrollSpeedLabel }
                )
                settingsSlider(
                    "Tap Stability",
                    value: $store.settings.touch.nativeTouchJitterThreshold,
                    range: 0...24,
                    step: 1,
                    format: { "\(Int($0)) pt" }
                )
            }

            Toggle("Touch Controller", isOn: $store.settings.streamerPreferences.touchControllerVisible)

            if store.settings.streamerPreferences.touchControllerVisible {
                Picker("Style", selection: $store.settings.touch.style) {
                    ForEach(TouchControllerStyle.allCases) { style in
                        Text(style.label).tag(style)
                    }
                }
                Picker("Joystick", selection: $store.settings.touch.joystickMode) {
                    ForEach(TouchJoystickMode.allCases) { mode in
                        Text(mode.label).tag(mode)
                    }
                }
                Picker("Aim Lock", selection: $store.settings.touch.aimMode) {
                    ForEach(TouchAimMode.allCases) { mode in
                        Text(mode.label).tag(mode)
                    }
                }
                settingsSlider(
                    "Dead Zone",
                    value: $store.settings.touch.joystickDeadZone,
                    range: 0...0.3,
                    step: 0.01,
                    format: { String(format: "%.0f%%", $0 * 100) }
                )
                touchLayoutSlider("Layout Scale", keyPath: \.scale, range: 0.6...1.4, step: 0.05)
                touchLayoutSlider("Button Size", keyPath: \.buttonScale, range: 0.65...1.5, step: 0.05)
                touchLayoutSlider("Stick Size", keyPath: \.stickScale, range: 0.65...1.5, step: 0.05)
                touchLayoutSlider("Opacity", keyPath: \.opacity, range: 0.15...1, step: 0.05)
                settingsSlider(
                    "Edge Padding",
                    value: $store.settings.touch.edgePadding,
                    range: 0...72,
                    step: 1,
                    format: { "\(Int($0)) pt" }
                )
                settingsSlider(
                    "Bottom Padding",
                    value: $store.settings.touch.bottomPadding,
                    range: 0...120,
                    step: 1,
                    format: { "\(Int($0)) pt" }
                )
                settingsSlider(
                    "Left Controls Across",
                    value: $store.settings.touch.leftOffsetX,
                    range: -220...220,
                    step: 2,
                    format: { "\(Int($0)) pt" }
                )
                settingsSlider(
                    "Left Controls Up",
                    value: $store.settings.touch.leftOffsetY,
                    range: -160...160,
                    step: 2,
                    format: { "\(Int(-$0)) pt" }
                )
                settingsSlider(
                    "Right Controls Across",
                    value: $store.settings.touch.rightOffsetX,
                    range: -220...220,
                    step: 2,
                    format: { "\(Int($0)) pt" }
                )
                settingsSlider(
                    "Right Controls Up",
                    value: $store.settings.touch.rightOffsetY,
                    range: -160...160,
                    step: 2,
                    format: { "\(Int(-$0)) pt" }
                )
                Button("Reset Touch Layout") {
                    store.settings.touch.resetLayout()
                }
            }

            Toggle("Rumble", isOn: $store.settings.phoneRumbleFallback)
            #endif

            LabeledContent("Physical Controller", value: "Detected automatically")
            Button {
                store.setStreamTutorialCompleted(false)
            } label: {
                Label("Replay Stream Tutorial", systemImage: "questionmark.circle")
            }
            .disabled(!store.settings.streamTutorialCompleted)
        } header: {
            Text("Touch & Controller")
        } footer: {
            VStack(alignment: .leading, spacing: 4) {
                Text("Currently: \(resolvedTouchMode.label).")
                Text("Automatic sends real touch only to games known to support it. Everything else gets a cursor or the on-screen controller.")
                Text("Rumble uses the phone's own motor when a paired controller has none.")
            }
        }
    }

    /// The end state the routing settings actually add up to. Showing this is the difference
    /// between three comprehensible pickers and three confusing ones.
    private var resolvedTouchMode: ResolvedTouchMode {
        switch store.settings.touch.nativeTouchMode {
        case .always:
            return .nativeTouch
        case .automatic, .never:
            if store.settings.fingerMouseEnabled {
                return .trackpadCursor(directClick: store.settings.touch.mouseDirectClick)
            }
            if store.settings.streamerPreferences.touchControllerVisible {
                return .virtualGamepad
            }
            return .inert
        }
    }

    private var scrollSensitivityLabel: String {
        switch store.settings.mouseScrollSensitivity {
        case ...20: return "Very fast"
        case 21...40: return "Standard"
        case 41...60: return "Precise"
        default: return "Slow"
        }
    }

    private var touchScrollSpeedLabel: String {
        switch store.settings.touch.nativeTouchScrollScale {
        case ..<0.5: return "Very slow"
        case 0.5..<0.8: return "Slow"
        case 0.8..<1.2: return "Normal"
        case 1.2..<1.6: return "Fast"
        default: return "Very fast"
        }
    }

    /// One slider row shape, so a label, its value and its accessibility value never drift apart.
    /// Stacks at accessibility sizes rather than letting the label and slider collide.
    private func settingsSlider(
        _ title: String,
        value: Binding<Double>,
        range: ClosedRange<Double>,
        step: Double,
        format: @escaping (Double) -> String
    ) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text(title)
                Spacer(minLength: 12)
                Text(format(value.wrappedValue))
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            Slider(value: value, in: range, step: step)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(title)
        .accessibilityValue(format(value.wrappedValue))
    }

    // MARK: Interface

    private var interfaceAppearanceSection: some View {
        Section {
            Picker("Accent", selection: $store.settings.uiAccent) {
                ForEach(UIAccent.allCases) { accent in
                    Label {
                        Text(accent.label)
                    } icon: {
                        Circle().fill(accent.color)
                    }
                    .tag(accent)
                }
            }

            Toggle("Expressive Surfaces", isOn: $store.settings.expressiveUI)
            Toggle("Animated Selection", isOn: $store.settings.liveSelectedOutlines)
            Toggle("Nerd Mode", isOn: $store.settings.nerdMode)
        } header: {
            Text("Appearance")
        } footer: {
            Text("Expressive surfaces add the gradient washes and softer cards. Turning both off gives a flatter interface that costs less to draw. Nerd Mode reveals the Advanced category and the technical readouts.")
        }
    }

    private var interfaceCatalogSection: some View {
        let hasCustomCatalogWallpaper = store.settings.catalogWallpaperFilename != nil
        return Section {
            Picker("Open App To", selection: $store.settings.launchPage) {
                ForEach(AppLaunchPage.allCases) { page in
                    Text(page.label).tag(page)
                }
            }
            Toggle("Compact Cards", isOn: $store.settings.compactGameCards)
            Toggle("Card Titles", isOn: $store.settings.showCardTitles)
            Toggle("Store Labels", isOn: $store.settings.showGameStoreLabels)
            Toggle("Favourite Badge", isOn: $store.settings.showFavoriteIconOnGameCards)
            settingsSlider(
                "Card Size",
                value: $store.settings.posterSizeScale,
                range: 0.75...1.4,
                step: 0.05,
                format: { String(format: "%.0f%%", $0 * 100) }
            )
            Toggle("Skip Server Selector", isOn: $store.settings.hideServerSelector)

            #if os(iOS)
            Toggle("Catalog Wallpaper", isOn: $store.settings.catalogWallpaperEnabled)
            if store.settings.catalogWallpaperEnabled {
                if !hasCustomCatalogWallpaper {
                    Picker("Wallpaper", selection: $store.settings.catalogWallpaperPreset) {
                        ForEach(CatalogWallpaperPreset.allCases) { preset in
                            Text(preset.label).tag(preset)
                        }
                    }
                } else {
                    LabeledContent("Wallpaper", value: "Custom image")
                }

                PhotosPicker(selection: $selectedCatalogWallpaperItem, matching: .images) {
                    Label(
                        hasCustomCatalogWallpaper ? "Replace Image" : "Choose Image",
                        systemImage: "photo.on.rectangle"
                    )
                }
                .disabled(catalogWallpaperImportInProgress)
                .onChangeCompat(of: selectedCatalogWallpaperItem) { item in
                    guard let item else { return }
                    Task { await importCatalogWallpaper(item) }
                }

                if catalogWallpaperImportInProgress {
                    HStack(spacing: 10) {
                        ProgressView()
                        Text("Preparing wallpaper…").foregroundStyle(.secondary)
                    }
                }

                if hasCustomCatalogWallpaper {
                    Button("Use a Built-in Wallpaper") {
                        resetCatalogWallpaperImage()
                    }
                }

                if let catalogWallpaperImportError {
                    Text(catalogWallpaperImportError)
                        .font(.footnote)
                        .foregroundStyle(OpenNOWPalette.statusPoor)
                }
            }
            #endif
        } header: {
            Text("Catalog")
        } footer: {
            Text("Turning card titles off leaves the grid as pure box art. The favourite badge puts a heart on artwork you have starred.")
        }
    }

    private var interfaceStatsSection: some View {
        Section {
            Toggle("Show Stats While Streaming", isOn: $store.settings.showStatsOverlay)

            if store.settings.showStatsOverlay {
                Picker("Style", selection: $store.settings.streamerPreferences.statsStyle) {
                    ForEach(StreamStatsStyle.allCases) { style in
                        Text(style.label).tag(style)
                    }
                }
                Picker("Position", selection: $store.settings.streamerPreferences.statsPosition) {
                    ForEach(StreamStatsPosition.allCases) { position in
                        Text(position.label).tag(position)
                    }
                }
                NavigationLink {
                    StatsMetricsPicker(metrics: $store.settings.streamStatsMetrics)
                } label: {
                    LabeledContent("Metrics", value: "\(store.settings.streamStatsMetrics.enabledCount) shown")
                }
            }

            Toggle("Keep Session Awake", isOn: $store.settings.showAntiAfkIndicator)
        } header: {
            Text("Stats HUD")
        } footer: {
            Text("Keeping the session awake nudges the cursor by a pixel after two idle minutes so GeForce NOW does not disconnect you mid-cutscene; a dot in the corner shows while it is doing that. The HUD sits over the game, so keep it to the numbers you actually watch. Values in the normal range stay untinted on purpose — amber and red are what should catch your eye.")
        }
    }

    private var interfaceSoundSessionSection: some View {
        Section {
            Toggle("Session Counter", isOn: $store.settings.sessionCounterEnabled)
            Toggle("Session Report", isOn: $store.settings.showSessionReportAfterStream)
            Toggle("Chime When Ready", isOn: $store.settings.queueReadySound)
            #if !os(tvOS)
            Toggle("Queue Live Activities", isOn: $store.settings.queueLiveActivitiesEnabled)
            #endif
        } header: {
            Text("Sessions")
        } footer: {
            Text("The session report appears after a stream ends and scores how the connection held up. The chime plays once when a rig frees up, so you can put the phone down while you wait for a long queue.")
        }
    }

    private var landingCategories: [SettingsCategory] {
        var categories: [SettingsCategory] = [.general, .stream, .input, .interface]
        if store.settings.nerdMode {
            categories.append(.advanced)
        }
        categories.append(.about)
        return categories
    }

    private var privacySection: some View {
        Section {
            Toggle("Share Usage Analytics", isOn: analyticsSharingBinding)
            NavigationLink {
                OpenNOWPrivacyPolicyView()
            } label: {
                Label("Privacy Policy", systemImage: "hand.raised")
            }
        } header: {
            Text("Privacy")
        } footer: {
            Text("Anonymous counts of which screens and stream settings get used. Never game titles, account details, or anything that identifies you. Off by default.")
        }
    }

    private var analyticsSharingBinding: Binding<Bool> {
        Binding(
            get: { store.settings.analyticsConsent.isSharing },
            set: { sharing in
                store.settings.analyticsConsentAsked = true
                store.settings.analyticsOptOut = !sharing
            }
        )
    }

    private var experimentalSection: some View {
        Section {
            Toggle("L4S Low Latency", isOn: $store.settings.enableL4S)
            Toggle("Cloud G-Sync", isOn: $store.settings.enableCloudGsync)
        } header: {
            Text("Experimental")
        } footer: {
            Text("Both are negotiated with the server and may be refused. If a game stops launching after you turn one on, turn it off again first.")
        }
    }

    private var reportProblemSection: some View {
        Section {
            Button {
                bugReportDeck = store.bugReportPreflightDeck()
            } label: {
                Label("Report a Problem", systemImage: "ladybug")
            }
        } footer: {
            Text("Sends what OpenNOW already knows about this device and your last session, so you don't have to remember your settings.")
        }
    }

    private var advancedSection: some View {
        Section {
            NavigationLink {
                CodecDiagnosticsView()
            } label: {
                Label("Decoders", systemImage: "cpu")
            }
            NavigationLink {
                DebugLogView()
            } label: {
                Label("Network Log", systemImage: "list.bullet.rectangle")
            }
            LabeledContent("Preferred Codec", value: store.settings.preferredCodec)
            LabeledContent("Stream Profile", value: headerSummary)
            LabeledContent("Color", value: selectedColorQualityLabel)
            LabeledContent("HDR", value: hdrAvailable ? (store.settings.hdrEnabled ? "On" : "Off") : "Unavailable")
            #if os(iOS)
            Button {
                diagnosticsExportInProgress = true
                diagnosticsExportError = nil
                diagnosticsExportStatus = nil
                Task {
                    defer { diagnosticsExportInProgress = false }
                    do {
                        let pasteURL = try await store.uploadDiagnosticsPaste()
                        UIPasteboard.general.string = store.diagnosticsClipboardSummary(pasteURL: pasteURL)
                        diagnosticsExportStatus = "Copied a compact build and tier summary with the full log paste link at the end."
                    } catch is CancellationError {
                        return
                    } catch {
                        diagnosticsExportError = error.localizedDescription
                    }
                }
            } label: {
                Label("Upload Redacted Logs & Copy Summary", systemImage: "doc.on.clipboard")
            }
            .disabled(diagnosticsExportInProgress)

            Button {
                diagnosticsExportInProgress = true
                diagnosticsExportError = nil
                diagnosticsExportStatus = nil
                Task {
                    diagnosticsDocument = DiagnosticsTextDocument(
                        text: await store.makeDiagnosticsExport()
                    )
                    diagnosticsExportInProgress = false
                    showingDiagnosticsExporter = true
                }
            } label: {
                Label("Export Extensive Logs", systemImage: "square.and.arrow.up")
            }
            .disabled(diagnosticsExportInProgress)
            .fileExporter(
                isPresented: $showingDiagnosticsExporter,
                document: diagnosticsDocument,
                contentType: .plainText,
                defaultFilename: store.diagnosticsExportFileName
            ) { result in
                if case .failure(let error) = result {
                    diagnosticsExportError = error.localizedDescription
                }
            }

            if diagnosticsExportInProgress {
                HStack(spacing: 10) {
                    ProgressView()
                    Text("Sanitizing app, API, queue, and stream logs…")
                        .foregroundStyle(.secondary)
                }
            }
            if let diagnosticsExportStatus {
                Text(diagnosticsExportStatus)
                    .font(.footnote)
                    .foregroundStyle(.green)
            }
            if let diagnosticsExportError {
                Text(diagnosticsExportError)
                    .font(.footnote)
                    .foregroundStyle(.red)
            }
            #else
            ShareLink(item: store.diagnosticsReport) {
                Label("Share Redacted Diagnostics", systemImage: "square.and.arrow.up")
            }
            #endif
        } header: {
            Text("Codec Diagnostics")
        } footer: {
            Text("Includes detailed redacted API requests and responses, launch, queue, stream, codec, device, settings, telemetry, and recent OpenNOW logs. Uploads use an unlisted public paste that expires after 7 days. Credentials, tokens, cookies, OAuth values, email addresses, local user paths, identifiers, IP addresses, and long opaque values are removed or fingerprinted before upload. Only the compact summary and paste link are copied.")
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

            Button(role: .destructive) {
                showingResetAppConfirmation = true
            } label: {
                Label("Reset App", systemImage: "arrow.triangle.2.circlepath")
            }
        }
    }

    private var aboutSection: some View {
        Section("About") {
            Button {
                bugReportDeck = store.bugReportPreflightDeck()
            } label: {
                Label("Report a Problem", systemImage: "ladybug")
            }

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
        }
    }

    private func savedAccountRow(_ account: SavedAccount) -> some View {
        let selected = account.userId == store.user?.userId

        return VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 12) {
                SettingsAccountAvatar(
                    initial: accountInitial(account),
                    size: 38,
                    selected: selected
                )

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

                if store.switchingAccountUserId == account.userId {
                    ProgressView()
                        .accessibilityLabel("Switching account")
                } else if selected {
                    Text("Active")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.green)
                } else {
                    Button("Switch") {
                        Task { await store.switchAccount(to: account.userId) }
                    }
                    .buttonStyle(.bordered)
                    .disabled(store.switchingAccountUserId != nil || store.isAuthenticating)
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
        let tierAllowsHDR = StreamSettingsResolver.isHDRAvailable(
            subscription: store.subscription,
            fallbackMembershipTier: store.user?.membershipTier
        )
        #if canImport(UIKit)
        let displayAllowsHDR = UIScreen.main.potentialEDRHeadroom > 1
        #else
        let displayAllowsHDR = false
        #endif
        let codecAllowsHDR = NativeStreamCodecProbe.report().capability(for: .h265)?.launchSafe == true
        return tierAllowsHDR && displayAllowsHDR && codecAllowsHDR
    }

    private var currentMembershipTier: String? {
        store.subscription?.membershipTier ?? store.user?.membershipTier
    }

    private var fpsValues: [Int] {
        StreamSettingsResolver.plan(for: currentMembershipTier) >= .ultimate
            ? [30, 60, 90, 120]
            : [30, 60]
    }

    private var streamPresetBinding: Binding<StreamPreset> {
        Binding(
            get: { store.settings.streamPreset },
            set: { preset in
                store.settings = StreamSettingsResolver.settings(
                    store.settings,
                    applying: preset,
                    membershipTier: currentMembershipTier
                )
                enforceAvailableResolution()
                enforceAvailableFPS()
            }
        )
    }

    private func customStreamBinding<Value>(
        _ keyPath: WritableKeyPath<AppSettings, Value>
    ) -> Binding<Value> {
        Binding(
            get: { store.settings[keyPath: keyPath] },
            set: { value in
                var settings = store.settings
                settings[keyPath: keyPath] = value
                settings.streamPreset = .custom
                store.settings = settings
            }
        )
    }

    #if os(iOS)
    private var catalogWallpaperDescription: String {
        store.settings.catalogWallpaperFilename == nil ? "OpenNOW Gradient" : "Custom Image"
    }

    @MainActor
    private func importCatalogWallpaper(_ item: PhotosPickerItem) async {
        catalogWallpaperImportInProgress = true
        catalogWallpaperImportError = nil
        defer {
            catalogWallpaperImportInProgress = false
            selectedCatalogWallpaperItem = nil
        }

        do {
            guard let data = try await item.loadTransferable(type: Data.self) else {
                throw CatalogWallpaperStorageError.invalidImage
            }
            let filename = try await Task.detached(priority: .userInitiated) {
                try CatalogWallpaperStorage.storeSelectedImageData(data)
            }.value
            store.settings.catalogWallpaperFilename = filename
            store.settings.catalogWallpaperEnabled = true
            store.persistSettings()
            CatalogWallpaperStorage.pruneManagedWallpapers(keeping: filename)
        } catch is CancellationError {
            return
        } catch {
            catalogWallpaperImportError = error.localizedDescription
        }
    }

    private func resetCatalogWallpaperImage() {
        store.settings.catalogWallpaperFilename = nil
        store.settings.catalogWallpaperEnabled = true
        store.persistSettings()
        CatalogWallpaperStorage.pruneManagedWallpapers(keeping: nil)
        catalogWallpaperImportError = nil
    }
    #endif

    private var sessionLimitDescription: String {
        let tier = (currentMembershipTier ?? "").uppercased()
        if tier.contains("ULTIMATE") || tier.contains("RTX3080") {
            return "8 hours"
        }
        if tier.contains("PERFORMANCE") || tier.contains("PRIORITY") || tier.contains("PREMIUM") || tier.contains("FOUNDERS") {
            return "6 hours"
        }
        return "1 hour"
    }

    private func hoursLabel(_ value: Double) -> String {
        if value == floor(value) {
            return "\(Int(value)) hr"
        }
        return String(format: "%.1f hr", value)
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
        guard store.settings.preferredResolution != "Auto" else { return }
        if let selected = StreamSettingsResolver.resolutionChoice(
                value: store.settings.preferredResolution,
                aspectRatio: store.settings.preferredAspectRatio
              ) {
            if !resolutionAvailable(selected) {
                store.settings.preferredResolution = "Auto"
            }
            return
        }
        store.settings.preferredResolution = "Auto"
    }

    private func enforceAvailableFPS() {
        if !fpsValues.contains(store.settings.preferredFPS) {
            store.settings.preferredFPS = fpsValues.last ?? 60
            store.settings.streamPreset = .custom
        }
    }

    private func enforceAvailableHDR() {
        if store.settings.hdrEnabled && !hdrAvailable {
            store.settings.hdrEnabled = false
        }
    }

    private func enforceAvailableCodec() {
        if !codecValues.contains(store.settings.preferredCodec) {
            store.settings.preferredCodec = "H264"
            store.settings.streamPreset = .custom
        }
    }

    private var codecSelectionBinding: Binding<String> {
        Binding(
            get: {
                codecValues.contains(store.settings.preferredCodec)
                    ? store.settings.preferredCodec
                    : "H264"
            },
            set: { value in
                guard codecValues.contains(value) else { return }
                var settings = store.settings
                settings.preferredCodec = value
                settings.streamPreset = .custom
                store.settings = settings
            }
        )
    }
}

/// Ten toggles for the in-stream HUD. Split into two groups because "how the stream is doing" and
/// "what the stream is" are two different questions, and people want the second one far less often.
private struct StatsMetricsPicker: View {
    @Binding var metrics: StreamStatsMetrics

    var body: some View {
        Form {
            Section("Connection") {
                metricToggle("Frame Rate", \.fps)
                metricToggle("Ping", \.ping)
                metricToggle("Latency", \.latency)
                metricToggle("Bitrate", \.bitrate)
                metricToggle("Packet Loss", \.packetLoss)
            }

            Section {
                metricToggle("Resolution", \.resolution)
                metricToggle("Codec", \.codec)
                metricToggle("Server", \.location)
                metricToggle("Battery", \.battery)
                metricToggle("Network Type", \.connection)
            } header: {
                Text("Session")
            } footer: {
                Text("At least one metric stays on. To hide the HUD entirely, turn off Show Stats While Streaming.")
            }
        }
        .navigationTitle("Stats Metrics")
        .navigationBarTitleDisplayMode(.inline)
    }

    private func metricToggle(_ title: String, _ keyPath: WritableKeyPath<StreamStatsMetrics, Bool>) -> some View {
        let isLastEnabled = metrics[keyPath: keyPath] && metrics.enabledCount == 1
        return Toggle(title, isOn: Binding(
            get: { metrics[keyPath: keyPath] },
            set: { newValue in
                // Never let the HUD empty out — there is no way to recover from inside a stream.
                guard newValue || metrics.enabledCount > 1 else { return }
                metrics[keyPath: keyPath] = newValue
            }
        ))
        .disabled(isLastEnabled)
    }
}

private struct OpenNOWPrivacyPolicyView: View {
    var body: some View {
        List {
            Section("Overview") {
                Text("OpenNOW is a client for a GeForce NOW account you provide. OpenNOW does not sell personal information, serve its own advertising, or run usage analytics.")
            }

            Section("Information on This Device") {
                Text("Account authorization tokens are stored in Apple Keychain. App preferences, a selected catalog wallpaper, a random device identifier, catalog cache, favorites, launcher choices, and resumable-session state are stored locally so the app can work reliably across launches.")
            }

            Section("Services You Contact") {
                Text("When you sign in, browse games, link stores, or start a stream, the app communicates directly with NVIDIA and the selected GeForce NOW alliance provider. Those services receive the account, device, network, and session information needed to provide their service under their own privacy terms.")
                Text("Free-tier server selection can contact PrintedWaste queue endpoints. OpenNOW does not send OAuth passwords to PrintedWaste.")
                Text("If you explicitly choose Upload Redacted Logs, OpenNOW sends a strictly sanitized diagnostic artifact to paste.rtech.support. The unlisted paste expires after seven days. The app does not upload diagnostics automatically.")
            }

            Section("Apple Features") {
                Text("If enabled, queue status can be shown using local notifications and Live Activities. These features are processed by the app and Apple system services on your device.")
            }

            Section("Your Choices") {
                Text("You can sign out one or all accounts, clear cached images and catalog data, reset settings, or reset the entire app from Settings. Resetting removes locally stored OpenNOW data and Keychain credentials.")
            }

            Section("Contact") {
                Link("Open an OpenNOW privacy issue", destination: URL(string: "https://github.com/OpenCloudGaming/OpenNOW/issues")!)
            }
        }
        .navigationTitle("Privacy Policy")
        .navigationBarTitleDisplayMode(.inline)
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

private struct SettingsAccountAvatar: View {
    let initial: String
    let size: CGFloat
    let selected: Bool

    var body: some View {
        ZStack {
            Circle()
                .fill(
                    LinearGradient(
                        colors: [
                            Color(red: 0.08, green: 0.14, blue: 0.10),
                            Color(red: 0.035, green: 0.07, blue: 0.055),
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
            Circle()
                .stroke(
                    selected ? brandAccent.opacity(0.90) : Color.white.opacity(0.18),
                    lineWidth: selected ? 2 : 1
                )
            Text(initial)
                .font(.system(size: size * 0.38, weight: .bold, design: .rounded))
                .foregroundStyle(Color.white)
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}

#if os(iOS)
private struct DiagnosticsTextDocument: FileDocument {
    static var readableContentTypes: [UTType] { [.plainText] }

    var text: String

    init(text: String) {
        self.text = text
    }

    init(configuration: ReadConfiguration) throws {
        guard let data = configuration.file.regularFileContents,
              let text = String(data: data, encoding: .utf8) else {
            throw CocoaError(.fileReadCorruptFile)
        }
        self.text = text
    }

    func fileWrapper(configuration: WriteConfiguration) throws -> FileWrapper {
        FileWrapper(regularFileWithContents: Data(text.utf8))
    }
}
#endif

private extension String {
    var nonEmpty: String? {
        let trimmed = trimmingCharacters(in: .whitespacesAndNewlines)
        return trimmed.isEmpty ? nil : trimmed
    }
}
