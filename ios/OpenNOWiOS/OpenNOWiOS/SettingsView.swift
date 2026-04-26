import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var store: OpenNOWStore

    private let fpsValues = [30, 60, 120]
    private let qualityValues = ["Balanced", "Data Saver", "Quality"]
    private let codecValues = ["Auto", "H264", "H265", "AV1"]
    private let regionValues = ["Auto", "US East", "US West", "Europe", "Asia"]

    var body: some View {
        NavigationStack {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 22) {
                    settingsHeader

                    settingsSection("Streaming") {
                        menuSettingRow(
                            icon: "globe",
                            color: .blue,
                            title: "Region",
                            value: store.settings.preferredRegion,
                            selection: $store.settings.preferredRegion,
                            options: regionValues
                        )
                        settingsDivider()
                        menuSettingRow(
                            icon: "display",
                            color: .cyan,
                            title: "Resolution",
                            value: resolutionLabel(for: store.settings.preferredResolution),
                            selection: $store.settings.preferredResolution,
                            options: StreamSettingsResolver.resolutionOptions.map(\.value),
                            label: resolutionLabel(for:)
                        )
                        settingsDivider()
                        menuSettingRow(
                            icon: "speedometer",
                            color: .green,
                            title: "Target FPS",
                            value: "\(store.settings.preferredFPS) fps",
                            selection: $store.settings.preferredFPS,
                            options: fpsValues,
                            label: { "\($0) fps" }
                        )
                        settingsDivider()
                        menuSettingRow(
                            icon: "slider.horizontal.3",
                            color: .orange,
                            title: "Quality",
                            value: store.settings.preferredQuality,
                            selection: $store.settings.preferredQuality,
                            options: qualityValues
                        )
                        settingsDivider()
                        menuSettingRow(
                            icon: "video.fill",
                            color: .purple,
                            title: "Codec",
                            value: store.settings.preferredCodec,
                            selection: $store.settings.preferredCodec,
                            options: codecValues
                        )
                        settingsDivider()
                        menuSettingRow(
                            icon: "gauge.with.dots.needle.67percent",
                            color: .indigo,
                            title: "Max Bitrate",
                            value: bitrateLabel(for: store.settings.maxBitrateMbps),
                            selection: $store.settings.maxBitrateMbps,
                            options: StreamSettingsResolver.bitrateOptionsMbps,
                            label: bitrateLabel(for:)
                        )
                    } footer: {
                        Text("Streaming changes apply to the next session launch.")
                    }

                    settingsSection("Game Session") {
                        menuSettingRow(
                            icon: "keyboard",
                            color: .teal,
                            title: "Keyboard Layout",
                            value: keyboardLayoutLabel(for: store.settings.keyboardLayout),
                            selection: $store.settings.keyboardLayout,
                            options: StreamSettingsResolver.keyboardLayoutOptions.map(\.value),
                            label: keyboardLayoutLabel(for:)
                        )
                        settingsDivider()
                        menuSettingRow(
                            icon: "character.book.closed",
                            color: .mint,
                            title: "Game Language",
                            value: gameLanguageLabel(for: store.settings.gameLanguage),
                            selection: $store.settings.gameLanguage,
                            options: StreamSettingsResolver.gameLanguageOptions.map(\.value),
                            label: gameLanguageLabel(for:)
                        )
                        settingsDivider()
                        toggleRow(
                            icon: "bolt.horizontal.fill",
                            color: .yellow,
                            title: "Low Latency Mode",
                            subtitle: "Use L4S when the network supports it.",
                            isOn: $store.settings.enableL4S
                        )
                        settingsDivider()
                        toggleRow(
                            icon: "rectangle.on.rectangle.angled",
                            color: .green,
                            title: "Cloud G-Sync",
                            subtitle: "Match stream pacing with supported displays.",
                            isOn: $store.settings.enableCloudGsync
                        )
                    }

                    settingsSection("Experience") {
                        toggleRow(
                            icon: "mic.fill",
                            color: .pink,
                            title: "Keep Microphone On",
                            subtitle: "Preserve mic state when a stream starts.",
                            isOn: $store.settings.keepMicEnabled
                        )
                        settingsDivider()
                        toggleRow(
                            icon: "chart.bar.fill",
                            color: .blue,
                            title: "Stats Overlay",
                            subtitle: "Show stream diagnostics in-game.",
                            isOn: $store.settings.showStatsOverlay
                        )
                        settingsDivider()
                        toggleRow(
                            icon: "hand.tap.fill",
                            color: .orange,
                            title: "Fortnite Mobile Touch",
                            subtitle: "Request the mobile touch profile for Fortnite.",
                            isOn: $store.settings.fortnitePrefersNativeTouch
                        )
                        settingsDivider()
                        toggleRow(
                            icon: "gamecontroller.fill",
                            color: .purple,
                            title: "Bluetooth Controller Passthrough",
                            subtitle: "Send native controller input to the stream.",
                            isOn: $store.settings.streamerPreferences.physicalControllerPassthrough
                        )
                        settingsDivider()
                        toggleRow(
                            icon: "circle.grid.cross.fill",
                            color: .indigo,
                            title: "Show Touch Controller",
                            subtitle: "Keep on-screen controls visible by default.",
                            isOn: $store.settings.streamerPreferences.touchControllerVisible
                        )
                        settingsDivider()
                        toggleRow(
                            icon: "server.rack",
                            color: .gray,
                            title: "Skip Server Selector",
                            subtitle: "Launch with the best available server automatically.",
                            isOn: $store.settings.hideServerSelector
                        )
                    }

                    settingsSection("Data") {
                        actionRow(
                            icon: "arrow.clockwise",
                            color: .blue,
                            title: "Reload Catalog",
                            value: store.isLoadingGames ? "Loading" : nil,
                            isDisabled: store.isLoadingGames
                        ) {
                            Task { await store.refreshCatalog() }
                        }

                        if !store.settings.favoriteGameIds.isEmpty {
                            settingsDivider()
                            actionRow(
                                icon: "heart.slash.fill",
                                color: .red,
                                title: "Clear Favorites",
                                value: "\(store.settings.favoriteGameIds.count)",
                                role: .destructive
                            ) {
                                store.clearFavorites()
                            }
                        }
                    }

                    if let user = store.user {
                        accountSection(user: user)
                    }

                    settingsSection("About") {
                        valueRow(icon: "number", color: .gray, title: "Version", value: "1.0")
                        settingsDivider()
                        valueRow(icon: "iphone", color: .green, title: "Platform", value: OpenNOWPlatform.displayName)
                        settingsDivider()
                        Link(destination: URL(string: "https://github.com/OpenCloudGaming/OpenNOW")!) {
                            rowContent(
                                icon: "chevron.left.forwardslash.chevron.right",
                                color: .indigo,
                                title: "GitHub Repository",
                                trailing: AnyView(chevron)
                            )
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 18)
                .padding(.top, 18)
                .padding(.bottom, 30)
                .frame(maxWidth: 680)
                .frame(maxWidth: .infinity)
            }
            .navigationTitle("Settings")
            .background(appBackground)
            .onChange(of: store.settings) { _, _ in
                store.persistSettings()
            }
        }
    }

    private var settingsHeader: some View {
        HStack(spacing: 14) {
            ZStack {
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .fill(brandAccent.gradient)
                    .frame(width: 58, height: 58)
                Image(systemName: "slider.horizontal.3")
                    .font(.title2.weight(.bold))
                    .foregroundStyle(.white)
            }
            .shadow(color: brandAccent.opacity(0.24), radius: 14, y: 7)

            VStack(alignment: .leading, spacing: 4) {
                Text(store.user?.displayName ?? "OpenNOW")
                    .font(.title3.weight(.semibold))
                    .lineLimit(1)
                Text(headerSummary)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }

            Spacer(minLength: 0)
        }
        .padding(16)
        .glassCard(cornerRadius: 24)
        .overlay(
            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .stroke(Color.white.opacity(0.12), lineWidth: 1)
        )
    }

    private var headerSummary: String {
        let profile = StreamSettingsResolver.profile(for: store.settings)
        return "\(profile.width)x\(profile.height) at \(profile.fps) fps"
    }

    private func accountSection(user: UserProfile) -> some View {
        settingsSection("Account") {
            valueRow(icon: "person.fill", color: .blue, title: "Account", value: user.displayName)
            if let email = user.email {
                settingsDivider()
                valueRow(icon: "envelope.fill", color: .teal, title: "Email", value: email)
            }
            settingsDivider()
            valueRow(icon: "sparkles", color: .purple, title: "Tier", value: user.membershipTier)
            settingsDivider()
            actionRow(
                icon: "rectangle.portrait.and.arrow.right",
                color: .red,
                title: "Sign Out",
                role: .destructive
            ) {
                store.signOut()
            }
        }
    }

    private func settingsSection<Content: View, Footer: View>(
        _ title: String,
        @ViewBuilder content: () -> Content,
        @ViewBuilder footer: () -> Footer
    ) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title.uppercased())
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.horizontal, 16)

            VStack(spacing: 0) {
                content()
            }
            .padding(.vertical, 4)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
            .overlay(
                RoundedRectangle(cornerRadius: 22, style: .continuous)
                    .stroke(Color.white.opacity(0.10), lineWidth: 1)
            )

            footer()
                .font(.footnote)
                .foregroundStyle(.secondary)
                .padding(.horizontal, 16)
                .fixedSize(horizontal: false, vertical: true)
        }
    }

    private func settingsSection<Content: View>(
        _ title: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        settingsSection(title, content: content) {
            EmptyView()
        }
    }

    private func menuSettingRow<Value: Hashable>(
        icon: String,
        color: Color,
        title: String,
        value: String,
        selection: Binding<Value>,
        options: [Value],
        label: @escaping (Value) -> String = { "\($0)" }
    ) -> some View {
        Menu {
            Picker(title, selection: selection) {
                ForEach(options, id: \.self) { option in
                    Text(label(option)).tag(option)
                }
            }
        } label: {
            rowContent(
                icon: icon,
                color: color,
                title: title,
                trailing: AnyView(
                    HStack(spacing: 6) {
                        Text(value)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                            .minimumScaleFactor(0.78)
                        chevron
                    }
                )
            )
        }
        .buttonStyle(.plain)
    }

    private func toggleRow(
        icon: String,
        color: Color,
        title: String,
        subtitle: String,
        isOn: Binding<Bool>
    ) -> some View {
        HStack(spacing: 12) {
            settingsIcon(icon, color: color)

            VStack(alignment: .leading, spacing: 2) {
                Text(title)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.primary)
                    .fixedSize(horizontal: false, vertical: true)
                Text(subtitle)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer(minLength: 12)

            Toggle(title, isOn: isOn)
                .labelsHidden()
                .tint(brandAccent)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .frame(minHeight: 52)
    }

    private func valueRow(icon: String, color: Color, title: String, value: String) -> some View {
        rowContent(
            icon: icon,
            color: color,
            title: title,
            trailing: AnyView(
                Text(value)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.trailing)
                    .lineLimit(2)
                    .minimumScaleFactor(0.78)
            )
        )
    }

    private func actionRow(
        icon: String,
        color: Color,
        title: String,
        value: String? = nil,
        role: ButtonRole? = nil,
        isDisabled: Bool = false,
        action: @escaping () -> Void
    ) -> some View {
        Button(role: role, action: action) {
            rowContent(
                icon: icon,
                color: color,
                title: title,
                titleColor: role == .destructive ? .red : .primary,
                trailing: AnyView(
                    HStack(spacing: 8) {
                        if let value {
                            Text(value)
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                        }
                        if isDisabled {
                            ProgressView()
                                .controlSize(.small)
                        } else {
                            chevron
                        }
                    }
                )
            )
        }
        .buttonStyle(.plain)
        .disabled(isDisabled)
    }

    private func rowContent(
        icon: String,
        color: Color,
        title: String,
        titleColor: Color = .primary,
        trailing: AnyView
    ) -> some View {
        HStack(spacing: 12) {
            settingsIcon(icon, color: color)

            Text(title)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(titleColor)
                .lineLimit(2)
                .minimumScaleFactor(0.82)
                .frame(maxWidth: .infinity, alignment: .leading)

            trailing
                .frame(maxWidth: 220, alignment: .trailing)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .frame(minHeight: 50)
        .contentShape(Rectangle())
    }

    private var chevron: some View {
        Image(systemName: "chevron.right")
            .font(.caption.weight(.semibold))
            .foregroundStyle(.tertiary)
    }

    private func settingsIcon(_ systemName: String, color: Color) -> some View {
        ZStack {
            RoundedRectangle(cornerRadius: 7, style: .continuous)
                .fill(color.gradient)
            Image(systemName: systemName)
                .font(.caption.weight(.bold))
                .foregroundStyle(.white)
        }
        .frame(width: 30, height: 30)
        .shadow(color: color.opacity(0.18), radius: 5, y: 2)
    }

    private func settingsDivider() -> some View {
        Divider()
            .padding(.leading, 56)
    }

    private func resolutionLabel(for value: String) -> String {
        StreamSettingsResolver.resolutionOptions.first(where: { $0.value == value })?.label ?? value
    }

    private func keyboardLayoutLabel(for value: String) -> String {
        StreamSettingsResolver.keyboardLayoutOptions.first(where: { $0.value == value })?.label ?? value
    }

    private func gameLanguageLabel(for value: String) -> String {
        StreamSettingsResolver.gameLanguageOptions.first(where: { $0.value == value })?.label ?? value
    }

    private func bitrateLabel(for value: Int) -> String {
        value == 0 ? "Auto" : "\(value) Mbps"
    }
}
