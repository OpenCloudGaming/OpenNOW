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

    var body: some View {
        NavigationStack {
            ZStack {
                Color.clear

                VStack(spacing: 18) {
                    header
                    routingRow
                    content
                    footer
                }
                .padding(.horizontal, 18)
                .padding(.top, 18)
                .padding(.bottom, 12)
            }
            .navigationBarTitleDisplayMode(.inline)
        }
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
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
                Text("Choose Server")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(brandAccent)
                Text("Route to the best GeForce NOW zone before launch.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(16)
        .glassCard()
    }

    private var routingRow: some View {
        HStack(spacing: 10) {
            routingPill(title: "Auto", icon: "bolt.fill", isSelected: routingPreference == .auto, isEnabled: autoZone != nil) {
                routingPreference = .auto
            }
            routingPill(title: "Closest", icon: "location.fill", isSelected: routingPreference == .closest, isEnabled: closestZone != nil || zones.contains(where: \.isMeasuring)) {
                routingPreference = .closest
            }
        }
    }

    @ViewBuilder
    private var content: some View {
        if isLoading {
            Spacer()
            ProgressView("Loading queue data...")
            Spacer()
        } else if let fetchError {
            Spacer()
            VStack(spacing: 10) {
                Image(systemName: "exclamationmark.triangle.fill")
                    .font(.title2)
                    .foregroundStyle(.orange)
                Text(fetchError)
                    .font(.subheadline)
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.secondary)
            }
            .padding(20)
            .glassCard()
            Spacer()
        } else if zones.isEmpty {
            Spacer()
            Text("No server data available.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .padding(20)
                .glassCard()
            Spacer()
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 18) {
                    ForEach(groupedZones, id: \.region) { group in
                        VStack(alignment: .leading, spacing: 10) {
                            HStack(spacing: 8) {
                                Text(group.flag)
                                Text(group.label)
                                    .font(.headline)
                            }
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
                            }
                        }
                    }
                }
                .padding(.bottom, 8)
            }
        }
    }

    private var footer: some View {
        HStack(spacing: 12) {
            Link(destination: URL(string: "https://printedwaste.com/gfn")!) {
                Text("Powered by PrintedWaste")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            Spacer()

            Button("Cancel") {
                dismiss()
            }
            .buttonStyle(.bordered)

            Button {
                onConfirm(selectedZoneUrl)
                dismiss()
            } label: {
                Text("Launch →")
                    .fontWeight(.semibold)
            }
            .buttonStyle(.borderedProminent)
            .tint(brandAccent)
            .disabled(isLoading || zones.isEmpty)
        }
        .padding(.top, 4)
    }

    private func routingPill(title: String, icon: String, isSelected: Bool, isEnabled: Bool, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            HStack(spacing: 8) {
                Image(systemName: icon)
                Text(title)
            }
            .font(.subheadline.weight(.semibold))
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
            VStack(alignment: .leading, spacing: 5) {
                HStack(spacing: 6) {
                    Text(zone.id)
                        .font(.subheadline.weight(.semibold))
                    Text(zone.regionSuffix)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    if isAuto {
                        smallBadge(title: "Auto", icon: "bolt.fill", color: .green)
                    } else if isClosest {
                        smallBadge(title: "Closest", icon: "location.fill", color: .blue)
                    }
                }
                Text(zone.zoneUrl.replacingOccurrences(of: "https://", with: "").replacingOccurrences(of: "/", with: ""))
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Spacer(minLength: 8)

            HStack(spacing: 8) {
                metricBadge(label: "Q \(zone.queuePosition)", color: queueColor(zone.queuePosition))
                if let etaMs = zone.etaMs {
                    metricBadge(label: formatWait(etaMs), color: .blue)
                }
                pingBadge
                if isSelected {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(brandAccent)
                }
            }
        }
        .padding(14)
        .background(rowBackground)
        .overlay(
            RoundedRectangle(cornerRadius: 12)
                .stroke(isSelected ? brandAccent.opacity(0.55) : Color.white.opacity(0.06), lineWidth: 1)
        )
    }

    private var pingBadge: some View {
        Group {
            if zone.isMeasuring {
                HStack(spacing: 6) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Ping")
                }
            } else if let pingMs = zone.pingMs {
                Text("\(pingMs) ms")
            } else {
                Text("N/A")
            }
        }
        .font(.caption.weight(.semibold))
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(pingBadgeColor, in: Capsule())
    }

    @ViewBuilder
    private var rowBackground: some View {
        if #available(iOS 26, *) {
            RoundedRectangle(cornerRadius: 12)
                .fill(.regularMaterial)
                .glassEffect(in: RoundedRectangle(cornerRadius: 12))
        } else {
            RoundedRectangle(cornerRadius: 12)
                .fill(.regularMaterial)
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
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(color.opacity(0.18), in: Capsule())
    }

    private func smallBadge(title: String, icon: String, color: Color) -> some View {
        HStack(spacing: 4) {
            Image(systemName: icon)
                .font(.system(size: 9, weight: .bold))
            Text(title)
        }
        .font(.caption2.weight(.bold))
        .padding(.horizontal, 7)
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
        if mins < 60 { return "~\(mins)m" }
        let hours = mins / 60
        let remaining = mins % 60
        return remaining > 0 ? "~\(hours)h \(remaining)m" : "~\(hours)h"
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
        content.sheet(item: $pendingGame) { game in
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
