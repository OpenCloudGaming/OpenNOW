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

    private enum RoutingPreference: Hashable {
        case auto
        case closest
        case manual

        var title: String {
            switch self {
            case .auto: return "Auto"
            case .closest: return "Closest"
            case .manual: return "Manual"
            }
        }
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

    private var selectedRoutingZone: PrintedWasteZone? {
        switch routingPreference {
        case .auto:
            return autoZone
        case .closest:
            return closestZone ?? autoZone
        case .manual:
            return zones.first(where: { $0.id == selectedZoneId }) ?? autoZone
        }
    }

    private var routingExplanation: String {
        switch routingPreference {
        case .auto:
            if let autoZone {
                return "Auto will launch on \(zoneDisplayName(autoZone))."
            }
            return "Auto will pick the best available server once queue data finishes loading."
        case .closest:
            if let closestZone {
                return "Closest will launch on \(zoneDisplayName(closestZone))."
            }
            if let autoZone {
                return "Closest is still measuring; launch will fall back to \(zoneDisplayName(autoZone))."
            }
            return "Closest is measuring network latency."
        case .manual:
            if let selectedRoutingZone {
                return "Manual selection will launch on \(zoneDisplayName(selectedRoutingZone))."
            }
            return "Choose a specific server below."
        }
    }

    var body: some View {
        NavigationStack {
            Group {
                if isLoading {
                    loadingState
                } else if let fetchError {
                    errorState(fetchError)
                } else if zones.isEmpty {
                    emptyState
                } else {
                    zoneList
                }
            }
            .animation(.snappy(duration: 0.25), value: isLoading)
            .animation(.snappy(duration: 0.25), value: routingPreference)
            .navigationTitle("Server")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Cancel") {
                        dismiss()
                    }
                }
                if !isLoading {
                    ToolbarItem(placement: .topBarTrailing) {
                        Button {
                            Task { await loadZones() }
                        } label: {
                            Image(systemName: "arrow.clockwise")
                        }
                        .accessibilityLabel("Reload Servers")
                        .disabled(isTestingPings)
                    }
                }
            }
        }
        .interactiveDismissDisabled(isLoading)
        .presentationDragIndicator(.visible)
        .presentationCornerRadius(28)
        .presentationBackground(.regularMaterial)
        .task {
            await loadZones()
        }
    }

    private var loadingState: some View {
        VStack(spacing: 16) {
            ProgressView()
                .controlSize(.large)
            Text("Checking servers")
                .font(.headline)
            Text("Loading queue position and measuring latency.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
        .padding(32)
    }

    private func errorState(_ message: String) -> some View {
        ContentUnavailableView {
            Label("Unable to Load Servers", systemImage: "exclamationmark.triangle")
        } description: {
            Text(message)
        } actions: {
            Button("Try Again") {
                Task { await loadZones() }
            }
            .buttonStyle(.borderedProminent)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var emptyState: some View {
        ContentUnavailableView {
            Label("No Servers Available", systemImage: "network.slash")
        } description: {
            Text("No routing data is available right now.")
        } actions: {
            Button("Launch Anyway") {
                onConfirm(nil)
                dismiss()
            }
            .buttonStyle(.borderedProminent)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var zoneList: some View {
        List {
            Section {
                launchSummary
            }

            Section {
                Picker("Routing", selection: $routingPreference) {
                    Text("Auto").tag(RoutingPreference.auto)
                    Text("Closest").tag(RoutingPreference.closest)
                    Text("Manual").tag(RoutingPreference.manual)
                }
                .pickerStyle(.segmented)

                if let selectedRoutingZone {
                    SelectedRouteRow(
                        title: routingPreference.title,
                        zone: selectedRoutingZone,
                        isTesting: isTestingPings
                    )
                }
            } header: {
                Text("Routing")
            } footer: {
                Text(routingExplanation)
            }

            ForEach(groupedZones, id: \.region) { group in
                Section(group.label) {
                    ForEach(group.zones) { zone in
                        Button {
                            routingPreference = .manual
                            selectedZoneId = zone.id
                        } label: {
                            ZoneRow(
                                zone: zone,
                                isSelected: selectedRoutingZone?.id == zone.id,
                                isManualSelection: routingPreference == .manual && selectedZoneId == zone.id,
                                isAuto: autoZone?.id == zone.id,
                                isClosest: closestZone?.id == zone.id
                            )
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
        .refreshable {
            await loadZones()
        }
        .safeAreaInset(edge: .bottom) {
            launchFooter
        }
    }

    private var launchSummary: some View {
        HStack(spacing: 14) {
            PrintedWasteArtwork(game: game)
                .frame(width: 56, height: 56)
                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))

            VStack(alignment: .leading, spacing: 4) {
                Text(game.title)
                    .font(.headline)
                    .lineLimit(2)
                Text("Choose your preferred region before launch.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            if let selectedRoutingZone {
                Text(selectedRoutingZone.id)
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(.quaternary, in: Capsule())
            }
        }
    }

    private var launchFooter: some View {
        VStack(spacing: 10) {
            if isTestingPings {
                HStack(spacing: 8) {
                    ProgressView()
                        .controlSize(.small)
                    Text("Measuring latency")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                    Spacer()
                }
            }

            Button {
                onConfirm(selectedZoneUrl)
                dismiss()
            } label: {
                Text(launchButtonTitle)
                    .font(.headline)
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .tint(brandAccent)
            .disabled(selectedZoneUrl == nil)
        }
        .padding(.horizontal, 20)
        .padding(.top, 12)
        .padding(.bottom, 8)
        .background(.regularMaterial)
    }

    private var launchButtonTitle: String {
        guard let zone = selectedRoutingZone else { return "Launch" }
        switch routingPreference {
        case .auto:
            return "Launch with Auto"
        case .closest:
            return "Launch with Closest"
        case .manual:
            return "Launch on \(zone.id)"
        }
    }

    private func zoneDisplayName(_ zone: PrintedWasteZone) -> String {
        let ping = zone.pingMs.map { "\($0) ms" } ?? (zone.isMeasuring ? "measuring" : "ping unknown")
        return "\(zone.id) in \(zone.region) · Q \(zone.queuePosition) · \(ping)"
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
            let completionState = TCPProbeCompletionState()

            @Sendable
            func finish(_ sample: Double?) {
                guard completionState.markFinished() else { return }

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
    let isManualSelection: Bool
    let isAuto: Bool
    let isClosest: Bool

    var body: some View {
        HStack(spacing: 14) {
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
                }
                HStack(spacing: 6) {
                    statusText("Queue \(zone.queuePosition)", color: queueColor(zone.queuePosition))
                    if isAuto {
                        statusText("Auto", color: .green)
                    }
                    if isClosest {
                        statusText("Closest", color: .blue)
                    }
                    if isManualSelection {
                        statusText("Selected", color: brandAccent)
                    }
                }
            }

            Spacer(minLength: 8)

            VStack(alignment: .trailing, spacing: 4) {
                if let etaMs = zone.etaMs {
                    Text(formatWait(etaMs))
                        .font(.subheadline.weight(.semibold))
                }
                pingBadge
            }
            .fixedSize(horizontal: true, vertical: false)

            Image(systemName: isSelected ? "checkmark.circle.fill" : "circle")
                .font(.title3.weight(.semibold))
                .foregroundStyle(isSelected ? brandAccent : Color.secondary.opacity(0.35))
        }
        .padding(.vertical, 4)
        .contentShape(Rectangle())
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
        .foregroundStyle(pingBadgeColor)
    }

    private var pingBadgeColor: Color {
        guard let pingMs = zone.pingMs else { return .secondary }
        if pingMs < 30 { return .green }
        if pingMs < 80 { return Color(red: 0.38, green: 0.62, blue: 0.08) }
        if pingMs < 150 { return .orange }
        return .red
    }

    private func statusText(_ text: String, color: Color) -> some View {
        Text(text)
            .font(.caption2.weight(.semibold))
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

private struct SelectedRouteRow: View {
    let title: String
    let zone: PrintedWasteZone
    let isTesting: Bool

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: iconName)
                .font(.title3.weight(.semibold))
                .foregroundStyle(brandAccent)
                .frame(width: 30)

            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.subheadline.weight(.semibold))
                Text("\(zone.id) · Queue \(zone.queuePosition)")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }

            Spacer(minLength: 8)

            if isTesting && zone.isMeasuring {
                ProgressView()
                    .controlSize(.small)
            } else if let pingMs = zone.pingMs {
                Text("\(pingMs) ms")
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var iconName: String {
        switch title {
        case "Closest": return "location.fill"
        case "Manual": return "hand.point.up.left.fill"
        default: return "sparkles"
        }
    }
}

private struct PrintedWasteArtwork: View {
    let game: CloudGame

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                RoundedRectangle(cornerRadius: 16)
                    .fill(gameColor(for: game.title).opacity(0.18))
                if let imageUrl = game.imageUrl, let url = URL(string: imageUrl) {
                    CachedRemoteImage(url: url, targetPixelSize: imageTargetPixelSize(for: proxy.size)) { image in
                        image
                            .resizable()
                            .scaledToFill()
                    } placeholder: {
                        GameArtworkLoadingPlaceholder(game: game, iconSize: 26, isFailure: false)
                    } failure: {
                        fallbackIcon
                    }
                } else {
                    fallbackIcon
                }
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
            .clipped()
        }
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
}

private struct PrintedWasteLaunchSheetModifier: ViewModifier {
    @EnvironmentObject private var store: OpenNOWStore
    @Binding var pendingLaunchRequest: GameLaunchRequest?

    func body(content: Content) -> some View {
        let sheetBinding = Binding<GameLaunchRequest?>(
            get: {
                store.shouldPresentPrintedWasteQueue ? pendingLaunchRequest : nil
            },
            set: { pendingLaunchRequest = $0 }
        )

        content
            .onChange(of: pendingLaunchRequest?.id) { _, _ in
                guard !store.shouldPresentPrintedWasteQueue,
                      let request = pendingLaunchRequest else { return }
                store.scheduleLaunch(game: request.game, zoneUrl: nil, launchOption: request.launchOption)
                pendingLaunchRequest = nil
            }
            .sheet(item: sheetBinding) { request in
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

private final class TCPProbeCompletionState: @unchecked Sendable {
    private let lock = NSLock()
    private var didFinish = false

    func markFinished() -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard !didFinish else { return false }
        didFinish = true
        return true
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
