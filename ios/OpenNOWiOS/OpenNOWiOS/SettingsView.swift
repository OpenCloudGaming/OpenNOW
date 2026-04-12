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
                    .listRowBackground(glassListRowBackground)
                    HStack {
                        Label("Stats Overlay", systemImage: "chart.bar.fill")
                        Spacer()
                        Toggle("", isOn: $store.settings.showStatsOverlay)
                    }
                    .listRowBackground(glassListRowBackground)
                } header: {
                    Label("Experience", systemImage: "star.fill")
                }

                Section {
                    Button {
                        Task { await store.refreshCatalog() }
                    } label: {
                        Label("Reload Catalog", systemImage: "arrow.clockwise")
                    }
                    .disabled(store.isLoadingGames)
                    .listRowBackground(glassListRowBackground)
                } header: {
                    Label("Data", systemImage: "internaldrive.fill")
                }

                if let user = store.user {
                    Section {
                        LabeledContent("Account", value: user.displayName)
                            .listRowBackground(glassListRowBackground)
                        if let email = user.email {
                            LabeledContent("Email", value: email)
                                .listRowBackground(glassListRowBackground)
                        }
                        LabeledContent("Tier", value: user.membershipTier)
                            .listRowBackground(glassListRowBackground)

                        Button(role: .destructive) {
                            store.signOut()
                        } label: {
                            Label("Sign Out", systemImage: "rectangle.portrait.and.arrow.right")
                        }
                        .listRowBackground(glassListRowBackground)
                    } header: {
                        Label("Account", systemImage: "person.crop.circle")
                    }
                }

                Section {
                    LabeledContent("Version", value: "1.0")
                        .listRowBackground(glassListRowBackground)
                    LabeledContent("Platform", value: "iOS")
                        .listRowBackground(glassListRowBackground)
                    Link(destination: URL(string: "https://github.com/OpenCloudGaming/OpenNOW")!) {
                        Label("GitHub Repository", systemImage: "link")
                    }
                    .listRowBackground(glassListRowBackground)
                } header: {
                    Label("About", systemImage: "info.circle")
                }
            }
            .navigationTitle("Settings")
            .scrollContentBackground(.hidden)
            .background(appBackground)
            .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
            .toolbarBackground(.visible, for: .navigationBar)
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
        .listRowBackground(glassListRowBackground)
    }

    private var glassListRowBackground: some View {
        Group {
            if #available(iOS 26, *) {
                RoundedRectangle(cornerRadius: 10)
                    .fill(.regularMaterial)
                    .glassEffect(in: RoundedRectangle(cornerRadius: 10))
            } else {
                Color(.secondarySystemGroupedBackground).opacity(0.75)
            }
        }
    }
}
