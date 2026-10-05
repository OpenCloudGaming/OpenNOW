import SwiftUI

#if canImport(UIKit)
import UIKit
#endif

/// The API calls this session has made, readable in the app.
///
/// The export already existed, but it produces a wall of text you have to send somewhere to read.
/// When a launch fails the useful question is usually "which call returned what", and answering it
/// on the device beats round-tripping a paste link. Everything here is already sanitised by
/// `DiagnosticsSanitizer` before it is stored, so nothing sensitive can reach the screen either.
struct DebugLogView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @Environment(\.openNowAccent) private var accent
    @State private var entries: [DiagnosticsTraceEntry] = []
    @State private var lifecycleEvents: [DebugLifecycleEvent] = []
    @State private var query = ""
    @State private var failuresOnly = false
    @State private var loaded = false

    private var visibleEntries: [DiagnosticsTraceEntry] {
        entries
            .filter { !failuresOnly || $0.failed }
            .filter { $0.matches(query) }
    }

    private var visibleLifecycleEvents: [DebugLifecycleEvent] {
        lifecycleEvents.reversed().filter {
            query.isEmpty || $0.category.localizedCaseInsensitiveContains(query)
                || $0.message.localizedCaseInsensitiveContains(query)
        }
    }

    var body: some View {
        List {
            if !failuresOnly && !visibleLifecycleEvents.isEmpty {
                Section("Session Timeline") {
                    ForEach(visibleLifecycleEvents) { event in
                        VStack(alignment: .leading, spacing: 3) {
                            HStack {
                                Text(event.category.uppercased())
                                    .font(.caption2.weight(.bold))
                                    .foregroundStyle(accent.color)
                                Spacer()
                                Text(event.timestamp, style: .time)
                                    .font(.caption2.monospacedDigit())
                                    .foregroundStyle(.secondary)
                            }
                            Text(event.message)
                                .font(.subheadline)
                                .textSelection(.enabled)
                        }
                    }
                }
            }
            if !entries.isEmpty {
                Section {
                    Toggle("Failures only", isOn: $failuresOnly)
                } footer: {
                    Text("\(visibleEntries.count) of \(entries.count) calls. Addresses, tokens and identifiers are already removed.")
                        .monospacedDigit()
                }
            }

            if visibleEntries.isEmpty {
                if visibleLifecycleEvents.isEmpty || failuresOnly {
                Section {
                    if !loaded {
                        HStack(spacing: 10) {
                            ProgressView()
                            Text("Reading log…").foregroundStyle(.secondary)
                        }
                    } else {
                        OpenNOWUnavailableView(
                            entries.isEmpty ? "Nothing logged yet" : "No matching calls",
                            systemImage: entries.isEmpty ? "text.append" : "magnifyingglass"
                        ) {
                            Text(entries.isEmpty
                                 ? "Calls appear here as OpenNOW talks to GeForce NOW. Try loading the catalog or starting a session."
                                 : "Try a different search, or turn off Failures only.")
                        }
                    }
                }
                }
            } else {
                Section("API Calls") {
                    ForEach(visibleEntries) { entry in
                        NavigationLink {
                            DebugLogDetailView(entry: entry)
                        } label: {
                            row(entry)
                        }
                    }
                }
            }
        }
        .searchable(text: $query, prompt: "Filter by source, path or status")
        .navigationTitle("Network Log")
        .navigationBarTitleDisplayMode(.inline)
        .refreshable { await reload() }
        .task { await reload() }
    }

    private func row(_ entry: DiagnosticsTraceEntry) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: OpenNOWSpacing.md) {
            Text(entry.statusCode > 0 ? "\(entry.statusCode)" : "—")
                .font(.caption.monospaced().weight(.semibold))
                .foregroundStyle(entry.failed ? OpenNOWPalette.statusPoor : accent.color)
                .frame(width: 34, alignment: .leading)

            VStack(alignment: .leading, spacing: 1) {
                Text(entry.shortPath)
                    .font(.subheadline)
                    .lineLimit(1)
                    .truncationMode(.head)
                Text("\(entry.method) · \(entry.source)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer(minLength: 8)

            Text("\(entry.durationMs) ms")
                .font(.caption2.monospaced())
                .foregroundStyle(.tertiary)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(entry.method) \(entry.shortPath)")
        .accessibilityValue(
            "\(entry.failed ? "Failed, " : "")status \(entry.statusCode), \(entry.durationMs) milliseconds, from \(entry.source)"
        )
    }

    private func reload() async {
        entries = await DiagnosticsHTTPTraceStore.shared.recentEntries()
        lifecycleEvents = store.debugEventSnapshot()
        loaded = true
    }
}

private struct DebugLogDetailView: View {
    let entry: DiagnosticsTraceEntry

    var body: some View {
        ScrollView([.vertical, .horizontal]) {
            Text(entry.rendered)
                .font(.caption2.monospaced())
                .textSelection(.enabled)
                .padding(OpenNOWSpacing.lg)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .navigationTitle(entry.shortPath)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button {
                    #if canImport(UIKit)
                    UIPasteboard.general.string = entry.rendered
                    #endif
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                }
            }
        }
    }
}
