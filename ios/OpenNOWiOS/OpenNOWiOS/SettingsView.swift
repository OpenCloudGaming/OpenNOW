import SwiftUI

struct SettingsView: View {
    @EnvironmentObject private var store: OpenNOWStore

    private let fpsValues = [60, 120]
    private let qualityValues = ["Balanced", "Data Saver", "Quality"]
    private let codecValues = ["Auto", "H264", "H265", "AV1"]
    private let regionValues = ["Auto", "US East", "US West", "Europe", "Asia"]

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    settingRow(
                        icon: "globe", color: .blue,
                        title: "Region",
                        picker: Picker("Region", selection: $store.settings.preferredRegion) {
                            ForEach(regionValues, id: \.self) { Text($0).tag($0) }
                        }
                    )
                    settingRow(
                        icon: "speedometer", color: .green,
                        title: "Target FPS",
                        picker: Picker("FPS", selection: $store.settings.preferredFPS) {
                            ForEach(fpsValues, id: \.self) { Text("\($0) fps").tag($0) }
                        }
                    )
                    settingRow(
                        icon: "slider.horizontal.3", color: .orange,
                        title: "Quality",
                        picker: Picker("Quality", selection: $store.settings.preferredQuality) {
                            ForEach(qualityValues, id: \.self) { Text($0).tag($0) }
                        }
                    )
                    settingRow(
                        icon: "video.fill", color: .purple,
                        title: "Codec",
                        picker: Picker("Codec", selection: $store.settings.preferredCodec) {
                            ForEach(codecValues, id: \.self) { Text($0).tag($0) }
                        }
                    )
                } header: {
                    Label("Streaming", systemImage: "dot.radiowaves.left.and.right")
                }

                Section {
                    HStack {
                        Label("Keep Microphone On", systemImage: "mic.fill")
                        Spacer()
                        Toggle("", isOn: $store.settings.keepMicEnabled)
                    }
                    HStack {
                        Label("Stats Overlay", systemImage: "chart.bar.fill")
                        Spacer()
                        Toggle("", isOn: $store.settings.showStatsOverlay)
                    }
                    HStack {
                        Label("Fortnite Mobile Touch", systemImage: "hand.tap.fill")
                        Spacer()
                        Toggle("", isOn: $store.settings.fortnitePrefersNativeTouch)
                    }
                    HStack {
                        Label("Bluetooth Controller Passthrough", systemImage: "gamecontroller.fill")
                        Spacer()
                        Toggle("", isOn: $store.settings.streamerPreferences.physicalControllerPassthrough)
                    }
                    HStack {
                        Label("Show Touch Controller", systemImage: "circle.grid.cross.fill")
                        Spacer()
                        Toggle("", isOn: $store.settings.streamerPreferences.touchControllerVisible)
                    }
                } header: {
                    Label("Experience", systemImage: "star.fill")
                } footer: {
                    Text("Fortnite mobile touch advertises a mobile device profile. Bluetooth passthrough sends connected controllers using the same gamepad packet path as the desktop streamer.")
                }

                Section {
                    Button {
                        Task { await store.refreshCatalog() }
                    } label: {
                        Text("Reload Catalog")
                    }
                    .disabled(store.isLoadingGames)
                } header: {
                    Label("Data", systemImage: "internaldrive.fill")
                }

                if let user = store.user {
                    Section {
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
                    } header: {
                        Label("Account", systemImage: "person.crop.circle")
                    }
                }

                Section {   
                    LabeledContent("Version", value: "1.0")
                    LabeledContent("Platform", value: OpenNOWPlatform.displayName)
                    Link(destination: URL(string: "https://github.com/OpenCloudGaming/OpenNOW")!) {
                        Text("GitHub Repository")
                    }
                } header: {
                    Label("About", systemImage: "info.circle")
                }
            }
            .navigationTitle("Settings")
            .background(appBackground)
            .settingsFormBackground()
            .onChange(of: store.settings) { _, _ in
                store.persistSettings()
            }
        }
    }

    private func settingRow<Content: View>(
        icon: String,
        color: Color,
        title: String,
        picker: Content
    ) -> some View {
        HStack {
            ZStack {
                RoundedRectangle(cornerRadius: 7)
                    .fill(color)
                    .frame(width: 30, height: 30)
                Image(systemName: icon)
                    .font(.caption.bold())
                    .foregroundStyle(.white)
            }
            picker
                .pickerStyle(.menu)
                .tint(.primary)
        }
        .listRowBackground(settingsRowBackground)
    }

    private var settingsRowBackground: Color {
        #if os(tvOS)
        return Color.white.opacity(0.08)
        #else
        return Color(.secondarySystemGroupedBackground).opacity(0.75)
        #endif
    }
}

private extension View {
    @ViewBuilder
    func settingsFormBackground() -> some View {
        #if os(tvOS)
        self
        #else
        self.scrollContentBackground(.hidden)
        #endif
    }
}
