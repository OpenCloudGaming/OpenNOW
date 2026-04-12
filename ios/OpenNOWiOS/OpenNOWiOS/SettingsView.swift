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
                            Label("Sign Out", systemImage: "rectangle.portrait.and.arrow.right")
                        }
                    } header: {
                        Label("Account", systemImage: "person.crop.circle")
                    }
                }

                Section {
                    LabeledContent("Version", value: "1.0")
                    LabeledContent("Platform", value: "iOS")
                    Link(destination: URL(string: "https://github.com/OpenCloudGaming/OpenNOW")!) {
                        Label("GitHub Repository", systemImage: "link")
                    }
                } header: {
                    Label("About", systemImage: "info.circle")
                }
            }
            .navigationTitle("Settings")
            .scrollContentBackground(.hidden)
            .background(appBackground)
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
        .listRowBackground(Color(.secondarySystemGroupedBackground).opacity(0.75))
    }
}
