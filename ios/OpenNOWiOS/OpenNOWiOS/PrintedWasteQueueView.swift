import SwiftUI
import Network

struct GameLaunchRequest: Identifiable, Equatable {
    let game: CloudGame
    let launchOption: GameLaunchOption?

    var id: String {
        "\(game.id)-\(launchOption?.id ?? "auto")"
    }
}

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
    @State private var lastAutoZoneId: String?
    @State private var lastClosestZoneId: String?

    private enum RoutingPreference: Equatable {
        case auto
        case closest
        case manual
    }

    private var isTestingPings: Bool {
        zones.contains(where: \.isMeasuring)
    }

    private var computedAutoZone: PrintedWasteZone? {
        guard !zones.isEmpty else { return nil }
        let maxPing = max(zones.compactMap { $0.pingMs }.max() ?? 1, 1)
        let maxQueue = max(zones.map(\.queuePosition).max() ?? 1, 1)
        return zones.min { lhs, rhs in
            let lhsScore = (Double(lhs.pingMs ?? maxPing) / Double(maxPing)) * 0.4 + (Double(lhs.queuePosition) / Double(maxQueue)) * 0.6
            let rhsScore = (Double(rhs.pingMs ?? maxPing) / Double(maxPing)) * 0.4 + (Double(rhs.queuePosition) / Double(maxQueue)) * 0.6
            return lhsScore < rhsScore
        }
    }

    private var autoZone: PrintedWasteZone? {
        if isTestingPings,
           let lastAutoZoneId,
           let savedAutoZone = zones.first(where: { $0.id == lastAutoZoneId }) {
            return savedAutoZone
        }
        return computedAutoZone
    }

    private var computedClosestZone: PrintedWasteZone? {
        zones
            .filter { $0.pingMs != nil }
            .min { ($0.pingMs ?? .max) < ($1.pingMs ?? .max) }
    }

    private var closestZone: PrintedWasteZone? {
        if isTestingPings,
           let lastClosestZoneId,
           let savedClosestZone = zones.first(where: { $0.id == lastClosestZoneId }) {
            return savedClosestZone
        }
        return computedClosestZone
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
                    ScrollView {
                        LazyVStack(alignment: .leading, spacing: 18) {
                            header
                                .printedWasteGlassSurface()

                            VStack(alignment: .leading, spacing: 10) {
                                Text("Routing")
                                    .font(.headline)
                                    .foregroundStyle(.secondary)
                                routingRow
                            }
                            .padding(14)
                            .printedWasteGlassSurface()

                            ForEach(groupedZones, id: \.region) { group in
                                VStack(alignment: .leading, spacing: 10) {
                                    Text("\(group.flag) \(group.label)")
                                        .font(.headline)
                                        .foregroundStyle(.secondary)
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
                                        .padding(12)
                                        .background(Color.primary.opacity(0.045), in: RoundedRectangle(cornerRadius: 14))
                                        .overlay(
                                            RoundedRectangle(cornerRadius: 14)
                                                .stroke(.white.opacity(0.14), lineWidth: 1)
                                        )
                                    }
                                }
                            }
                        }
                        .padding(.horizontal, 16)
                        .padding(.vertical, 12)
                    }
                }
            }
            .animation(.spring(response: 0.35), value: isLoading)
            .navigationTitle("Choose Server")
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
                Text("Choose your preferred region before launch.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if let autoZone {
                VStack(alignment: .trailing, spacing: 2) {
                    Text("Best")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.secondary)
                    Text(autoZone.id)
                        .font(.caption.weight(.bold))
                }
            }
        }
        .padding(14)
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
                        Capsule()
                            .fill(.clear)
                            .glassEffect(in: Capsule())
                            .overlay(
                                Capsule()
                                    .stroke(isSelected ? brandAccent.opacity(0.5) : .white.opacity(0.2), lineWidth: 1)
                            )
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
            let previousZonesById = Dictionary(uniqueKeysWithValues: zones.map { ($0.id, $0) })
            let nukedZones = Set(mapping.data.compactMap { entry in
                entry.value.nuked == true ? entry.key : nil
            })

            zones = queue.data
                .filter { zoneId, _ in
                    Self.isStandardZone(zoneId) && !nukedZones.contains(zoneId)
                }
                .map { zoneId, zone in
                    let components = zone.Region
                        .split(separator: "-", maxSplits: 1)
                        .map { String($0) }
                    let region = components.first ?? zone.Region
                    let suffix = components.count > 1 ? components[1] : zone.Region
                    return PrintedWasteZone(
                        id: zoneId,
                        region: region,
                        queuePosition: zone.QueuePosition,
                        etaMs: zone.eta,
                        zoneUrl: Self.constructZoneUrl(zoneId),
                        pingMs: previousZonesById[zoneId]?.pingMs,
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
        } catch is CancellationError {
            isLoading = false
            return
        } catch let nsError as NSError
            where nsError.domain == NSURLErrorDomain && nsError.code == NSURLErrorCancelled {
            isLoading = false
            return
        } catch {
            isLoading = false
            fetchError = error.localizedDescription
        }
    }

    private func measurePings() async {
        let maxConcurrentPings = 16
        guard !zones.isEmpty else { return }

        for start in stride(from: 0, to: zones.count, by: maxConcurrentPings) {
            let end = min(start + maxConcurrentPings, zones.count)
            let batch = Array(zones[start..<end])

            await withTaskGroup(of: (String, Int?).self) { group in
                for zone in batch {
                    group.addTask {
                        let ping = await Self.measurePing(to: zone.zoneUrl)
                        return (zone.id, ping)
                    }
                }

                for await (zoneId, pingMs) in group {
                    if Task.isCancelled {
                        group.cancelAll()
                        break
                    }
                    if let index = zones.firstIndex(where: { $0.id == zoneId }) {
                        zones[index].pingMs = pingMs
                        zones[index].isMeasuring = false
                    }
                }
            }

            if Task.isCancelled {
                return
            }
        }
        persistRoutingRecommendations()
    }

    private func persistRoutingRecommendations() {
        guard !isTestingPings else { return }
        lastAutoZoneId = computedAutoZone?.id
        lastClosestZoneId = computedClosestZone?.id
    }

    private static func measurePing(to zoneUrl: String) async -> Int? {
        guard let url = URL(string: zoneUrl),
              let host = url.host(),
              let port = NWEndpoint.Port(rawValue: UInt16(url.port ?? (url.scheme == "https" ? 443 : 80))) else {
            return nil
        }

        _ = await tcpProbe(host: host, port: port, timeout: 3)

        var samples: [Double] = []
        for sampleIndex in 0..<3 {
            if sampleIndex > 0 {
                try? await Task.sleep(nanoseconds: 100_000_000)
            }
            if let sample = await tcpProbe(host: host, port: port, timeout: 3) {
                samples.append(sample)
            }
        }

        guard !samples.isEmpty else { return nil }
        let average = samples.reduce(0, +) / Double(samples.count)
        return Int(average.rounded())
    }

    private static func tcpProbe(host: String, port: NWEndpoint.Port, timeout: TimeInterval) async -> Double? {
        await withCheckedContinuation { continuation in
            let connection = NWConnection(host: NWEndpoint.Host(host), port: port, using: .tcp)
            let start = Date()
            let lock = NSLock()
            var didFinish = false

            func finish(_ sample: Double?) {
                lock.lock()
                guard !didFinish else {
                    lock.unlock()
                    return
                }
                didFinish = true
                lock.unlock()

                connection.stateUpdateHandler = nil
                connection.cancel()
                continuation.resume(returning: sample)
            }

            connection.stateUpdateHandler = { state in
                switch state {
                case .ready:
                    finish(Date().timeIntervalSince(start) * 1000)
                case .failed, .cancelled:
                    finish(nil)
                default:
                    break
                }
            }

            connection.start(queue: .global(qos: .utility))
            DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + timeout) {
                finish(nil)
            }
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
                        .layoutPriority(1)
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
                    ZStack {
                        Circle()
                            .fill(brandAccent)
                            .frame(width: 22, height: 22)
                        Image(systemName: "checkmark")
                            .font(.caption2.bold())
                            .foregroundStyle(.white)
                    }
                }
            }
            .fixedSize(horizontal: true, vertical: false)
        }
        .padding(.vertical, 2)
    }

    private var queueBadge: some View {
        Text("Q \(zone.queuePosition)")
            .font(.caption.weight(.bold))
            .foregroundStyle(queueColor(zone.queuePosition))
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .liquidBadgeBackground(tint: queueColor(zone.queuePosition).opacity(0.18), cornerRadius: 8)
    }

    private var pingBadge: some View {
        Group {
            if zone.isMeasuring {
                Text("Testing")
            } else if let pingMs = zone.pingMs {
                Text("\(pingMs) ms")
            } else {
                Text("N/A")
            }
        }
        .font(.caption.weight(.semibold))
        .lineLimit(1)
        .minimumScaleFactor(0.85)
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .liquidBadgeBackground(tint: pingBadgeColor, cornerRadius: 999)
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
            .liquidBadgeBackground(tint: color.opacity(0.2), cornerRadius: 999)
    }

    private func smallIconBadge(icon: String, color: Color) -> some View {
        Image(systemName: icon)
            .font(.caption2.weight(.bold))
        .lineLimit(1)
        .minimumScaleFactor(0.85)
        .padding(.horizontal, 8)
        .padding(.vertical, 4)
        .liquidBadgeBackground(tint: color.opacity(0.2), cornerRadius: 999)
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
                CachedRemoteImage(url: url) { image in
                    image
                        .resizable()
                        .scaledToFill()
                } placeholder: {
                    ProgressView()
                } failure: {
                    fallbackIcon
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
    func printedWasteLaunchSheet(pendingLaunchRequest: Binding<GameLaunchRequest?>) -> some View {
        modifier(PrintedWasteLaunchSheetModifier(pendingLaunchRequest: pendingLaunchRequest))
    }

    fileprivate func printedWasteGlassSurface(cornerRadius: CGFloat = 16) -> some View {
        modifier(PrintedWasteGlassSurfaceModifier(cornerRadius: cornerRadius))
    }

    fileprivate func liquidBadgeBackground(tint: Color, cornerRadius: CGFloat) -> some View {
        modifier(LiquidBadgeBackgroundModifier(tint: tint, cornerRadius: cornerRadius))
    }
}

private struct PrintedWasteGlassSurfaceModifier: ViewModifier {
    let cornerRadius: CGFloat

    func body(content: Content) -> some View {
        if #available(iOS 26, *) {
            content
                .background(
                    RoundedRectangle(cornerRadius: cornerRadius)
                        .fill(.clear)
                        .glassEffect(in: RoundedRectangle(cornerRadius: cornerRadius))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: cornerRadius)
                        .stroke(.white.opacity(0.16), lineWidth: 1)
                )
        } else {
            content
                .glassCard()
        }
    }
}

private struct LiquidBadgeBackgroundModifier: ViewModifier {
    let tint: Color
    let cornerRadius: CGFloat

    func body(content: Content) -> some View {
        content
            .background(tint, in: RoundedRectangle(cornerRadius: cornerRadius))
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius)
                    .stroke(tint.opacity(0.32), lineWidth: 1)
            )
    }
}

private struct PrintedWasteLaunchSheetModifier: ViewModifier {
    @EnvironmentObject private var store: OpenNOWStore
    @Binding var pendingLaunchRequest: GameLaunchRequest?

    func body(content: Content) -> some View {
        let sheetBinding = Binding<GameLaunchRequest?>(
            get: {
                store.shouldUsePrintedWasteQueue ? pendingLaunchRequest : nil
            },
            set: { pendingLaunchRequest = $0 }
        )

        content
            .onChange(of: pendingLaunchRequest?.id) { _, _ in
                guard !store.shouldUsePrintedWasteQueue,
                      let request = pendingLaunchRequest else { return }
                store.scheduleLaunch(game: request.game, zoneUrl: nil, launchOption: request.launchOption)
                pendingLaunchRequest = nil
            }
            .sheet(item: sheetBinding) { request in
                if #available(iOS 26, *) {
                    PrintedWasteQueueView(game: request.game) { selectedZoneUrl in
                        store.scheduleLaunch(game: request.game, zoneUrl: selectedZoneUrl, launchOption: request.launchOption)
                    }
                    .environmentObject(store)
                    .presentationDetents([.medium, .large])
                    .presentationDragIndicator(.visible)
                    .presentationBackground(.clear)
                } else {
                    PrintedWasteQueueView(game: request.game) { selectedZoneUrl in
                        store.scheduleLaunch(game: request.game, zoneUrl: selectedZoneUrl, launchOption: request.launchOption)
                    }
                    .environmentObject(store)
                    .presentationDetents([.medium, .large])
                    .presentationDragIndicator(.visible)
                    .presentationBackground(.regularMaterial)
                }
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
