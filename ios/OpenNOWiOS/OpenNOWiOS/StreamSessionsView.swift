import SwiftUI

struct StreamSessionsView: View {
    @ObservedObject var history: StreamSessionHistoryStore

    var body: some View {
        NavigationStack { content }
    }

    private var content: some View {
        Group {
            if history.records.isEmpty {
                VStack(spacing: 12) {
                    Image(systemName: "clock.arrow.circlepath").font(.largeTitle)
                    Text("Your sessions will appear here").font(.headline)
                    Text("See how long you played and how your battery changed. History is saved on this device.")
                        .foregroundStyle(.secondary).multilineTextAlignment(.center)
                }
                .padding(32).frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List {
                    ForEach(history.records) { record in
                        NavigationLink {
                            details(record)
                        } label: {
                            VStack(alignment: .leading, spacing: 5) {
                                Text(record.gameTitle).font(.headline)
                                Text(record.startedAt.formatted(date: .abbreviated, time: .shortened))
                                    .font(.caption).foregroundStyle(.secondary)
                                Text("\(record.durationText) · \(record.battery.changeText)")
                                    .font(.subheadline)
                                if record.endedAt == nil || record.interrupted {
                                    Text(record.status).font(.caption).foregroundStyle(.secondary)
                                }
                            }.padding(.vertical, 4)
                        }
                        .deleteDisabled(record.endedAt == nil)
                    }
                    .onDelete(perform: history.delete)
                }
            }
        }
        .navigationTitle("Sessions")
        .toolbar { if !history.records.isEmpty { EditButton() } }
    }

    private func details(_ record: StreamSessionRecord) -> some View {
        Form {
            Section("Session") {
                LabeledContent("Game", value: record.gameTitle)
                LabeledContent("Started", value: record.startedAt.formatted(date: .abbreviated, time: .shortened))
                if let end = record.endedAt {
                    LabeledContent(record.interrupted ? "Last recorded" : "Ended",
                                   value: end.formatted(date: .abbreviated, time: .shortened))
                }
                LabeledContent("Duration", value: record.durationText)
                LabeledContent("Status", value: record.status)
            }
            Section {
                LabeledContent("At start", value: percent(record.battery.startPercent))
                LabeledContent(record.endedAt == nil ? "Current" : record.interrupted ? "Last recorded" : "At end",
                               value: percent(record.battery.currentPercent))
                LabeledContent("Battery change", value: record.battery.changeText)
                LabeledContent("Charging during session", value: record.battery.includedCharging ? "Yes" : "No")
            } header: { Text("Battery") } footer: {
                Text("Battery change compares the first available percentage with the latest reading. Sessions that include charging show the net change.")
            }
            Section("Requested stream") {
                LabeledContent("Resolution", value: record.resolution)
                LabeledContent("Frame rate", value: "\(record.targetFPS) FPS")
                LabeledContent("Codec", value: record.codec)
                LabeledContent("HDR", value: record.hdr ? "On" : "Off")
                LabeledContent("Connection", value: record.transport)
            }
        }
        .navigationTitle(record.gameTitle)
    }

    private func percent(_ value: Int?) -> String { value.map { "\($0)%" } ?? "Unavailable" }
}
