import SwiftUI

struct PrintedWasteZone: Identifiable, Equatable {
    let id: String
    let region: String
    let queuePosition: Int
    let etaMs: Double?
    let zoneUrl: String
    var pingMs: Int?
    var isMeasuring: Bool
    let regionSuffix: String
}

struct PrintedWasteQueueView: View {
    @Environment(\.dismiss) private var dismiss

    let game: CloudGame
    let onConfirm: (String?) -> Void

    @State private var zones: [PrintedWasteZone] = []
    @State private var routingPreference: RoutingPreference = .auto
    @State private var selectedZoneId: String?
    @State private var isLoading = true
    @State private var fetchError: String?

    private enum RoutingPreference: Equatable {
        case auto
        case closest
        case manual
    }

    private var autoZone: PrintedWasteZone? {
        guard !zones.isEmpty else { return nil }
        let hasPendingPings = zones.contains(where: \.isMeasuring)
        if hasPendingPings {
            return zones.min(by: { $0.queuePosition < $1.queuePosition })
        }
        let maxPing = max(zones.compactMap { $0.pingMs }.max() ?? 1, 1)
        let maxQueue = max(zones.map(\.queuePosition).max() ?? 1, 1)
        return zones.min { lhs, rhs in
            let lhsScore = (Double(lhs.pingMs ?? maxPing) / Double(maxPing)) * 0.4 + (Double(lhs.queuePosition) / Double(maxQueue)) * 0.6
            let rhsScore = (Double(rhs.pingMs ?? maxPing) / Double(maxPing)) * 0.4 + (Double(rhs.queuePosition) / Double(maxQueue)) * 0.6
            return lhsScore < rhsScore
        }
    }

    private var closestZone: PrintedWasteZone? {
        zones
            .filter { $0.pingMs != nil }
            .min { ($0.pingMs ?? .max) < ($1.pingMs ?? .max) }
    }

    private var groupedZones: [(region: String, label: String, flag: String, zones: [PrintedWasteZone])] {
        let grouped = Dictionary(grouping: zones) { $0.region }
        let order = ["US", "CA", "EU", "JP", "KR", "THAI", "MY"]
        let sortedRegions = order.filter { grouped[$0] != nil } + grouped.keys.filter { !order.contains($0) }.sorted()
        return sortedRegions.map { region in
            let meta = Self.regionMeta[region] ?? (label: region, flag: "🌐")
            return (
                region,
                meta.label,
                meta.flag,
                grouped[region, default: []].sorted { $0.queuePosition < $1.queuePosition }
            )
        }
    }

    private var selectedZoneUrl: String? {
        switch routingPreference {
        case .auto:
            return autoZone?.zoneUrl
        case .closest:
            return closestZone?.zoneUrl ?? autoZone?.zoneUrl
        case .manual:
            return zones.first(where: { $0.id == selectedZoneId })?.zoneUrl ?? autoZone?.zoneUrl
        }
    }

    @ViewBuilder
    private var glassRowBackground: some View {
        if #available(iOS 26, *) {
            Rectangle()
                .fill(.ultraThinMaterial)
                .glassEffect(in: Rectangle())
        } else {
            Color(.secondarySystemGroupedBackground).opacity(0.7)
        }
    }

    var body: some View {
        NavigationStack {
            Group {
                if isLoading {
                    ProgressView("Loading queue data...")
                        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
                } else if let fetchError {
                    ContentUnavailableView(
                        "Unable to Load Servers",
                        systemImage: "exclamationmark.triangle",
                        description: Text(fetchError)
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else if zones.isEmpty {
                    VStack(spacing: 18) {
                        ContentUnavailableView(
                            "No Servers Available",
                            systemImage: "network.slash",
                            description: Text("No servers are available right now.")
                        )
                        Button("Launch Anyway") {
                            onConfirm(nil)
                            dismiss()
                        }
                        .buttonStyle(.borderedProminent)
                    }
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    List {
                        Section {
                            header
                                .listRowBackground(glassRowBackground)
                        }

                        Section("Routing") {
                            routingRow
                                .listRowBackground(glassRowBackground)
                        }

                        ForEach(groupedZones, id: \.region) { group in
                            Section("\(group.flag) \(group.label)") {
                                ForEach(group.zones) { zone in
                                    Button {
                                        routingPreference = .manual
                                        selectedZoneId = zone.id
                                    } label: {
                                        ZoneRow(
                                            zone: zone,
                                            isSelected: routingPreference == .manual && selectedZoneId == zone.id,
                                            isAuto: autoZone?.id == zone.id,
                                            isClosest: closestZone?.id == zone.id
                                        )
                                    }
                                    .buttonStyle(.plain)
                                    .listRowBackground(glassRowBackground)
                                }
                            }
                        }
                    }
                    .listStyle(.plain)
                    .scrollContentBackground(.hidden)
                }
            }
            .animation(.spring(response: 0.35), value: isLoading)
            .navigationTitle("Choose Server")
            .navigationBarTitleDisplayMode(.inline)
            .toolbarBackground(.ultraThinMaterial, for: .navigationBar)
            .toolbarBackgroundVisibility(.visible, for: .navigationBar)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Cancel") {
                        dismiss()
                    }
                }
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Launch") {
                        onConfirm(selectedZoneUrl)
                        dismiss()
                    }
                    .fontWeight(.semibold)
                    .disabled(isLoading || zones.isEmpty)
                }
            }
        }
        .interactiveDismissDisabled(isLoading)
        .presentationDragIndicator(.visible)
        .presentationCornerRadius(32)
        .presentationBackground(.regularMaterial)
        .task {
            await loadZones()
        }
    }

    private var header: some View {
        HStack(spacing: 14) {
            PrintedWasteArtwork(game: game)
                .frame(width: 68, height: 68)
                .clipShape(RoundedRectangle(cornerRadius: 16))

            VStack(alignment: .leading, spacing: 4) {
                Text(game.title)
                    .font(.headline)
                    .lineLimit(2)
                Text("Pick a zone before launch.")
                    .font(.footnote)
                    .foregroundStyle(brandAccent)
            }
            Spacer()
        }
        .padding(12)
        .glassCard()
        .padding(.vertical, 2)
    }

    private var routingRow: some View {
        HStack(spacing: 10) {
            routingPill(title: "Auto", accessibilityLabel: "Auto", isSelected: routingPreference == .auto, isEnabled: autoZone != nil) {
                routingPreference = .auto
            }
            routingPill(title: "Closest", accessibilityLabel: "Closest", isSelected: routingPreference == .closest, isEnabled: closestZone != nil || zones.contains(where: \.isMeasuring)) {
                routingPreference = .closest
            }
        }
    }

    private func routingPill(title: String, accessibilityLabel: String, isSelected: Bool, isEnabled: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.subheadline.weight(.semibold))
                .lineLimit(1)
                .minimumScaleFactor(0.9)
            .foregroundStyle(isSelected ? brandAccent : .primary)
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity)
            .background(
                Group {
                    if #available(iOS 26, *) {
                        if isSelected {
                            Capsule()
                                .fill(.regularMaterial)
                                .glassEffect(in: Capsule())
                                .overlay(Capsule().stroke(brandAccent.opacity(0.5), lineWidth: 1))
                        } else {
                            Capsule()
                                .fill(.ultraThinMaterial)
                                .glassEffect(in: Capsule())
                        }
                    } else {
                        if isSelected {
                            Capsule()
                                .fill(.regularMaterial)
                                .overlay(Capsule().stroke(brandAccent.opacity(0.45), lineWidth: 1))
                        } else {
                            Capsule()
                                .fill(.ultraThinMaterial)
                        }
                    }
                }
            )
        }
        .buttonStyle(.plain)
        .accessibilityLabel(accessibilityLabel)
        .disabled(!isEnabled)
        .opacity(isEnabled ? 1 : 0.6)
    }

    private func loadZones() async {
        isLoading = true
        fetchError = nil
        do {
            async let queueResponse = fetchQueueResponse()
            async let mappingResponse = fetchMappingResponse()
            let (queue, mapping) = try await (queueResponse, mappingResponse)
            let nukedZones = Set(mapping.data.compactMap { entry in
                entry.value.nuked == true ? entry.key : nil
            })

            zones = queue.data
                .filter { zoneId, _ in
                    Self.isStandardZone(zoneId) && !nukedZones.contains(zoneId)
                }
                .map { zoneId, zone in
                    let components = zone.Region.split(separator: "-", maxSplits: 1).map(String.init)
                    let region = components.first ?? zone.Region
                    let suffix = components.count > 1 ? components[1] : zone.Region
                    return PrintedWasteZone(
                        id: zoneId,
                        region: region,
                        queuePosition: zone.QueuePosition,
                        etaMs: zone.eta,
                        zoneUrl: Self.constructZoneUrl(zoneId),
                        pingMs: nil,
                        isMeasuring: true,
                        regionSuffix: suffix
                    )
                }
                .sorted { lhs, rhs in
                    if lhs.region == rhs.region {
                        return lhs.queuePosition < rhs.queuePosition
                    }
                    return lhs.region < rhs.region
                }

            if selectedZoneId == nil {
                selectedZoneId = autoZone?.id
            }
            isLoading = false
            await measurePings()
        } catch {
            isLoading = false
            fetchError = error.localizedDescription
        }
    }

    private func measurePings() async {
        await withTaskGroup(of: (String, Int?).self) { group in
            for zone in zones {
                let url = zone.zoneUrl
                group.addTask {
                    let ping = await Self.measurePing(to: url)
                    return (zone.id, ping)
                }
            }

            for await (zoneId, pingMs) in group {
                if let index = zones.firstIndex(where: { $0.id == zoneId }) {
                    zones[index].pingMs = pingMs
                    zones[index].isMeasuring = false
                }
            }
        }
    }

    private static func measurePing(to zoneUrl: String) async -> Int? {
        let warmups = 2
        let measurements = 3
        for _ in 0..<warmups {
            _ = await headProbe(urlString: zoneUrl)
        }

        var samples: [Double] = []
        for _ in 0..<measurements {
            if let sample = await headProbe(urlString: zoneUrl) {
                samples.append(sample)
            }
        }

        guard !samples.isEmpty else { return nil }
        let average = samples.reduce(0, +) / Double(samples.count)
        return Int(average.rounded())
    }

    private static func headProbe(urlString: String) async -> Double? {
        guard let url = URL(string: urlString) else { return nil }
        var request = URLRequest(url: url)
        request.httpMethod = "HEAD"
        request.timeoutInterval = 5
        let start = Date()
        do {
            _ = try await URLSession.shared.data(for: request)
            return Date().timeIntervalSince(start) * 1000
        } catch {
            return nil
        }
    }

    private static func isStandardZone(_ zoneId: String) -> Bool {
        zoneId.hasPrefix("NP-") && !zoneId.hasPrefix("NPA-")
    }

    private static func constructZoneUrl(_ zoneId: String) -> String {
        "https://\(zoneId.lowercased()).cloudmatchbeta.nvidiagrid.net/"
    }

    private static let regionMeta: [String: (label: String, flag: String)] = [
        "US": ("North America", "🇺🇸"),
        "EU": ("Europe", "🇪🇺"),
        "JP": ("Japan", "🇯🇵"),
        "KR": ("South Korea", "🇰🇷"),
        "CA": ("Canada", "🇨🇦"),
        "THAI": ("Southeast Asia", "🇹🇭"),
        "MY": ("Malaysia", "🇲🇾")
    ]
}

private struct ZoneRow: View {
    let zone: PrintedWasteZone
    let isSelected: Bool
    let isAuto: Bool
    let isClosest: Bool

    var body: some View {
        HStack(spacing: 12) {
            queueBadge

            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Text(zone.id)
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(1)
                        .minimumScaleFactor(0.85)
                    Text(zone.regionSuffix)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .minimumScaleFactor(0.85)
                    if isAuto {
                        smallIconBadge(icon: "bolt.fill", color: .green)
                    } else if isClosest {
                        smallIconBadge(icon: "location.fill", color: .blue)
                    }
                }
            }

            Spacer(minLength: 8)

            HStack(spacing: 8) {
                if let etaMs = zone.etaMs {
                    metricBadge(label: formatWait(etaMs), color: .blue)
                }
                pingBadge
                if isSelected {
                    selectedBadge
                }
            }
        }
        .padding(.vertical, 2)
        .contentShape(Rectangle())
    }

    private var queueBadge: some View {
        Text("Q \(zone.queuePosition)")
            .font(.caption.weight(.bold))
            .foregroundStyle(queueColor(zone.queuePosition))
            .lineLimit(1)
            .minimumScaleFactor(0.9)
            .frame(minWidth: 42)
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background {
                if #available(iOS 26, *) {
                    RoundedRectangle(cornerRadius: 8)
                        .fill(queueColor(zone.queuePosition).opacity(0.15))
                        .glassEffect(in: RoundedRectangle(cornerRadius: 8))
                } else {
                    RoundedRectangle(cornerRadius: 8)
                        .fill(queueColor(zone.queuePosition).opacity(0.2))
                }
            }
    }

    private var pingBadge: some View {
        Group {
            if zone.isMeasuring {
                HStack(spacing: 6) {
                    ProgressView()
                        .controlSize(.small)
                    Text("--")
                }
            } else if let pingMs = zone.pingMs {
                Text("\(pingMs)")
            } else {
                Text("N/A")
            }
        }
        .font(.caption.weight(.semibold))
        .lineLimit(1)
        .minimumScaleFactor(0.85)
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background {
            if #available(iOS 26, *) {
                Capsule()
                    .fill(pingBadgeColor)
                    .glassEffect(in: Capsule())
            } else {
                Capsule()
                    .fill(pingBadgeColor)
            }
        }
    }

    private var pingBadgeColor: Color {
        guard let pingMs = zone.pingMs else { return .secondary.opacity(0.16) }
        if pingMs < 30 { return .green.opacity(0.18) }
        if pingMs < 80 { return Color(red: 0.52, green: 0.8, blue: 0.13).opacity(0.18) }
        if pingMs < 150 { return .yellow.opacity(0.2) }
        return .red.opacity(0.18)
    }

    private func metricBadge(label: String, color: Color) -> some View {
        Text(label)
            .font(.caption.weight(.semibold))
            .lineLimit(1)
            .minimumScaleFactor(0.85)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background {
                if #available(iOS 26, *) {
                    Capsule()
                        .fill(color.opacity(0.15))
                        .glassEffect(in: Capsule())
                } else {
                    Capsule()
                        .fill(color.opacity(0.18))
                }
            }
    }

    private var selectedBadge: some View {
        ZStack {
            if #available(iOS 26, *) {
                Circle()
                    .fill(.regularMaterial)
                    .glassEffect(in: Circle())
                    .overlay(Circle().stroke(brandAccent, lineWidth: 2))
                    .frame(width: 22, height: 22)
                Image(systemName: "checkmark")
                    .font(.caption2.bold())
                    .foregroundStyle(brandAccent)
            } else {
                Circle()
                    .fill(brandAccent)
                    .frame(width: 22, height: 22)
                Image(systemName: "checkmark")
                    .font(.caption2.bold())
                    .foregroundStyle(.white)
            }
        }
    }

    private func smallIconBadge(icon: String, color: Color) -> some View {
        Image(systemName: icon)
            .font(.caption2.weight(.bold))
        .lineLimit(1)
        .minimumScaleFactor(0.85)
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .background(color.opacity(0.18), in: Capsule())
        .foregroundStyle(color)
    }

    private func queueColor(_ queue: Int) -> Color {
        if queue <= 5 { return .green }
        if queue <= 15 { return Color(red: 0.52, green: 0.8, blue: 0.13) }
        if queue <= 30 { return .yellow }
        return .red
    }

    private func formatWait(_ etaMs: Double) -> String {
        let mins = Int(ceil(etaMs / 60000))
        if mins < 60 { return "\(mins)m" }
        let hours = mins / 60
        let remaining = mins % 60
        return remaining > 0 ? "\(hours)h\(remaining)m" : "\(hours)h"
    }
}

private struct PrintedWasteArtwork: View {
    let game: CloudGame

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 16)
                .fill(gameColor(for: game.title).opacity(0.18))
            if let imageUrl = game.imageUrl, let url = URL(string: imageUrl) {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image
                            .resizable()
                            .scaledToFill()
                    case .empty:
                        ProgressView()
                    default:
                        fallbackIcon
                    }
                }
            } else {
                fallbackIcon
            }
        }
        .clipped()
    }

    private var fallbackIcon: some View {
        Image(systemName: game.icon)
            .font(.system(size: 26, weight: .semibold))
            .foregroundStyle(gameColor(for: game.title))
    }
}

extension View {
    func printedWasteLaunchSheet(pendingGame: Binding<CloudGame?>) -> some View {
        modifier(PrintedWasteLaunchSheetModifier(pendingGame: pendingGame))
    }
}

private struct PrintedWasteLaunchSheetModifier: ViewModifier {
    @EnvironmentObject private var store: OpenNOWStore
    @Binding var pendingGame: CloudGame?

    func body(content: Content) -> some View {
        let sheetBinding = Binding<CloudGame?>(
            get: {
                store.authProviderCode == "BPC" ? nil : pendingGame
            },
            set: { pendingGame = $0 }
        )

        content
            .onChange(of: pendingGame?.id) { _, _ in
                guard store.authProviderCode == "BPC", let game = pendingGame else { return }
                store.scheduleLaunch(game: game, zoneUrl: nil)
                pendingGame = nil
            }
            .sheet(item: sheetBinding) { game in
            PrintedWasteQueueView(game: game) { selectedZoneUrl in
                store.scheduleLaunch(game: game, zoneUrl: selectedZoneUrl)
            }
            .environmentObject(store)
        }
    }
}

private struct PrintedWasteQueueResponse: Decodable {
    let status: Bool
    let data: [String: PrintedWasteQueueAPIEntry]
}

private struct PrintedWasteQueueAPIEntry: Decodable {
    let QueuePosition: Int
    let LastUpdated: TimeInterval
    let Region: String
    let eta: Double?

    enum CodingKeys: String, CodingKey {
        case QueuePosition
        case LastUpdated = "Last Updated"
        case Region
        case eta
    }
}

private struct PrintedWasteMappingResponse: Decodable {
    let status: Bool
    let data: [String: PrintedWasteMappingEntry]
}

private struct PrintedWasteMappingEntry: Decodable {
    let title: String?
    let region: String?
    let is4080Server: Bool?
    let is5080Server: Bool?
    let nuked: Bool?
}

private func fetchQueueResponse() async throws -> PrintedWasteQueueResponse {
    guard let url = URL(string: "https://api.printedwaste.com/gfn/queue/") else {
        throw NSError(domain: "PrintedWaste", code: 1, userInfo: [NSLocalizedDescriptionKey: "Invalid PrintedWaste queue URL"])
    }
    var request = URLRequest(url: url)
    request.setValue("opennow/1.0 iOS", forHTTPHeaderField: "User-Agent")
    request.timeoutInterval = 7
    let (data, response) = try await URLSession.shared.data(for: request)
    guard let http = response as? HTTPURLResponse, http.statusCode == 200 else {
        throw NSError(domain: "PrintedWaste", code: 2, userInfo: [NSLocalizedDescriptionKey: "PrintedWaste queue request failed"])
    }
    let decoded = try JSONDecoder().decode(PrintedWasteQueueResponse.self, from: data)
    guard decoded.status else {
        throw NSError(domain: "PrintedWaste", code: 3, userInfo: [NSLocalizedDescriptionKey: "PrintedWaste queue returned status:false"])
    }
    return decoded
}

private func fetchMappingResponse() async throws -> PrintedWasteMappingResponse {
    guard let url = URL(string: "https://remote.printedwaste.com/config/GFN_SERVERID_TO_REGION_MAPPING") else {
        throw NSError(domain: "PrintedWaste", code: 4, userInfo: [NSLocalizedDescriptionKey: "Invalid PrintedWaste mapping URL"])
    }
    var request = URLRequest(url: url)
    request.setValue("opennow/1.0 iOS", forHTTPHeaderField: "User-Agent")
    request.timeoutInterval = 7
    let (data, response) = try await URLSession.shared.data(for: request)
    guard let http = response as? HTTPURLResponse, http.statusCode == 200 else {
        throw NSError(domain: "PrintedWaste", code: 5, userInfo: [NSLocalizedDescriptionKey: "PrintedWaste mapping request failed"])
    }
    let decoded = try JSONDecoder().decode(PrintedWasteMappingResponse.self, from: data)
    guard decoded.status else {
        throw NSError(domain: "PrintedWaste", code: 6, userInfo: [NSLocalizedDescriptionKey: "PrintedWaste mapping returned status:false"])
    }
    return decoded
}
