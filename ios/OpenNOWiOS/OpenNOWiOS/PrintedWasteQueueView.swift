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
    let title: String
    let region: String
    let regionLabel: String
    let queuePosition: Int
    let etaMs: Double?
    let zoneUrl: String
    var pingMs: Int?
    var isMeasuring: Bool
    let regionSuffix: String
    let gpuTier: String?

    init(
        id: String,
        title: String? = nil,
        region: String,
        regionLabel: String? = nil,
        queuePosition: Int,
        etaMs: Double?,
        zoneUrl: String,
        pingMs: Int?,
        isMeasuring: Bool,
        regionSuffix: String,
        gpuTier: String? = nil
    ) {
        self.id = id
        self.title = title ?? id
        self.region = region
        self.regionLabel = regionLabel ?? region
        self.queuePosition = queuePosition
        self.etaMs = etaMs
        self.zoneUrl = zoneUrl
        self.pingMs = pingMs
        self.isMeasuring = isMeasuring
        self.regionSuffix = regionSuffix
        self.gpuTier = gpuTier
    }
}

private struct PrintedWasteLocation: Identifiable {
    let title: String
    let primary: PrintedWasteZone
    let zoneIDs: Set<String>
    let alternateCount: Int
    let gpuTier: String?
    var id: String { title }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}

enum StreamZonePolicy {
    static let blockedZoneIDs: Set<String> = []
    static let blockedZoneMessage = "This server is temporarily unavailable on iOS. Choose another server or Automatic."

    static func isBlocked(_ value: String?) -> Bool {
        guard let zoneID = normalizedZoneID(from: value) else { return false }
        return blockedZoneIDs.contains(zoneID)
    }

    private static func normalizedZoneID(from value: String?) -> String? {
        guard let value else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }

        let host = URLComponents(string: trimmed)?.host
            ?? trimmed
                .replacingOccurrences(of: "https://", with: "", options: [.caseInsensitive, .anchored])
                .replacingOccurrences(of: "http://", with: "", options: [.caseInsensitive, .anchored])
                .split(separator: "/", maxSplits: 1)
                .first
                .map(String.init)
        return host?
            .split(separator: ".", maxSplits: 1)
            .first
            .map { String($0).uppercased() }
    }
}

func recommendedPrintedWasteZone(in zones: [PrintedWasteZone]) -> PrintedWasteZone? {
    let allowedZones = zones.filter {
        !StreamZonePolicy.isBlocked($0.id) && !StreamZonePolicy.isBlocked($0.zoneUrl)
    }
    guard !allowedZones.isEmpty else { return nil }
    let pingedZones = allowedZones.filter { $0.pingMs != nil }
    let candidates = pingedZones.isEmpty ? allowedZones : pingedZones
    let maxPing = max(candidates.compactMap(\.pingMs).max() ?? 1, 1)
    let maxQueue = max(candidates.map(\.queuePosition).max() ?? 1, 1)
    let queueAware = candidates.min { lhs, rhs in
        let lhsScore = printedWasteScore(lhs, maxPing: maxPing, maxQueue: maxQueue)
        let rhsScore = printedWasteScore(rhs, maxPing: maxPing, maxQueue: maxQueue)
        if lhsScore != rhsScore { return lhsScore < rhsScore }
        let lhsPing = lhs.pingMs ?? .max
        let rhsPing = rhs.pingMs ?? .max
        if lhsPing != rhsPing { return lhsPing < rhsPing }
        return lhs.queuePosition < rhs.queuePosition
    }
    if (queueAware?.pingMs ?? 0) <= 100 { return queueAware }
    return candidates.min {
        if $0.pingMs != $1.pingMs { return ($0.pingMs ?? .max) < ($1.pingMs ?? .max) }
        if $0.queuePosition != $1.queuePosition { return $0.queuePosition < $1.queuePosition }
        return $0.id < $1.id
    }
}

private func printedWasteScore(_ zone: PrintedWasteZone, maxPing: Int, maxQueue: Int) -> Double {
    (Double(zone.pingMs ?? maxPing) / Double(maxPing)) * 0.75
        + (Double(zone.queuePosition) / Double(maxQueue)) * 0.25
}

func printedWasteRegionalURL(zoneId: String, title: String?, regions: [StreamRegion]) -> String? {
    guard zoneId.hasPrefix("NP-"), !zoneId.hasPrefix("NPA-"),
          let title = title?.trimmingCharacters(in: .whitespacesAndNewlines),
          !title.isEmpty else { return nil }
    func locationKey(_ value: String) -> String {
        value.trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(
                of: #"\s*(?:\((?:usa|canada)\)|[12])$"#,
                with: "",
                options: [.regularExpression, .caseInsensitive]
            )
            .lowercased()
    }
    let wanted = locationKey(title.caseInsensitiveCompare("Mumbai") == .orderedSame ? "India" : title)
    for region in regions where locationKey(region.name) == wanted {
        guard let url = URLComponents(string: region.url),
              url.scheme?.lowercased() == "https",
              let host = url.host?.lowercased(),
              !host.hasPrefix("np-"),
              host.hasSuffix(".cloudmatchbeta.nvidiagrid.net"),
              url.port == nil || url.port == 443,
              url.path.isEmpty || url.path == "/",
              url.query == nil,
              url.fragment == nil else { continue }
        return "https://\(host)/"
    }
    return nil
}

struct PrintedWasteQueueView: View {
    @Environment(\.dismiss) private var dismiss
    @EnvironmentObject private var store: OpenNOWStore

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
        recommendedPrintedWasteZone(in: zones)
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

    private var groupedZones: [(region: String, locations: [PrintedWasteLocation])] {
        let maxPing = max(zones.compactMap(\.pingMs).max() ?? 1, 1)
        let maxQueue = max(zones.map(\.queuePosition).max() ?? 1, 1)
        let locations = Dictionary(grouping: zones, by: \.title).map { title, variants in
            let ordered = variants.sorted {
                let lhs = printedWasteScore($0, maxPing: maxPing, maxQueue: maxQueue)
                let rhs = printedWasteScore($1, maxPing: maxPing, maxQueue: maxQueue)
                return lhs == rhs ? $0.id < $1.id : lhs < rhs
            }
            let primary = ordered[0]
            return PrintedWasteLocation(
                title: title,
                primary: primary,
                zoneIDs: Set(variants.map(\.id)),
                alternateCount: variants.count - 1,
                gpuTier: ordered.compactMap(\.gpuTier).first
            )
        }
        return Dictionary(grouping: locations, by: { $0.primary.regionLabel })
            .map { region, locations in
                (region: region, locations: locations.sorted {
                    let lhs = printedWasteScore($0.primary, maxPing: maxPing, maxQueue: maxQueue)
                    let rhs = printedWasteScore($1.primary, maxPing: maxPing, maxQueue: maxQueue)
                    return lhs == rhs ? $0.title < $1.title : lhs < rhs
                })
            }
            .sorted {
                let lhs = printedWasteScore($0.locations[0].primary, maxPing: maxPing, maxQueue: maxQueue)
                let rhs = printedWasteScore($1.locations[0].primary, maxPing: maxPing, maxQueue: maxQueue)
                return lhs == rhs ? $0.region < $1.region : lhs < rhs
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
            }
        }
        .interactiveDismissDisabled(isLoading)
        .presentationDragIndicator(.visible)
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
        OpenNOWUnavailableView("Unable to Load Servers", systemImage: "exclamationmark.triangle") {
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
        OpenNOWUnavailableView("No Servers Available", systemImage: "network.slash") {
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
                    if routingPreference == .manual,
                       let selectedPing = selectedRoutingZone.pingMs,
                       let closestPing = zones.compactMap(\.pingMs).min(),
                       selectedPing > closestPing {
                        Label("This server has more measured latency than the closest option.",
                              systemImage: "exclamationmark.triangle")
                            .font(.footnote)
                            .foregroundStyle(.orange)
                    }
                }
            } header: {
                Text("Routing")
            } footer: {
                Text(routingExplanation)
            }

            ForEach(groupedZones, id: \.region) { group in
                Section(group.region) {
                    ForEach(group.locations) { location in
                        Button {
                            routingPreference = .manual
                            selectedZoneId = location.primary.id
                        } label: {
                            ZoneRow(
                                zone: zones.first(where: { $0.id == selectedRoutingZone?.id && location.zoneIDs.contains($0.id) }) ?? location.primary,
                                title: location.title,
                                alternateCount: location.alternateCount,
                                gpuTier: location.gpuTier,
                                isSelected: selectedRoutingZone.map { location.zoneIDs.contains($0.id) } ?? false,
                                isManualSelection: routingPreference == .manual && selectedZoneId.map { location.zoneIDs.contains($0) } == true,
                                isAuto: autoZone.map { location.zoneIDs.contains($0.id) } ?? false,
                                isClosest: closestZone.map { location.zoneIDs.contains($0.id) } ?? false
                            )
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
        .scrollContentBackground(.hidden)
        .refreshable {
            await loadZones()
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
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
            Divider()

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
        .frame(maxWidth: .infinity)
        .bottomSheetFooterBackground()
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
        let ping = zone.pingMs.map { "\($0) milliseconds" } ?? (zone.isMeasuring ? "measuring latency" : "latency unknown")
        let quality = zone.pingMs.map(StreamQuality.serverPing).flatMap { $0 == .good ? nil : $0.label }
        let people = zone.queuePosition == 1 ? "1 person in queue" : "\(zone.queuePosition) people in queue"
        return [zone.title, "in \(zone.regionLabel)", ping, quality, people]
            .compactMap { $0 }
            .joined(separator: ", ")
    }

    private func loadZones() async {
        isLoading = true
        fetchError = nil
        do {
            async let queueResponse = fetchQueueResponse()
            async let mappingResponse = fetchMappingResponse()
            async let regionResponse = store.queueRegions()
            let (queue, mapping, regions) = try await (queueResponse, mappingResponse, regionResponse)
            guard !regions.isEmpty else {
                throw NSError(domain: "PrintedWaste", code: 3,
                    userInfo: [NSLocalizedDescriptionKey: "GeForce NOW regional routes are unavailable. Try again shortly."])
            }
            let previousZonesById = Dictionary(uniqueKeysWithValues: zones.map { ($0.id, $0) })
            let nukedZones = Set(mapping.data.compactMap { entry in
                entry.value.nuked == true ? entry.key : nil
            })

            zones = queue.data
                .filter { zoneId, _ in
                    Self.isStandardZone(zoneId)
                        && !nukedZones.contains(zoneId)
                        && !StreamZonePolicy.isBlocked(zoneId)
                }
                .compactMap { zoneId, zone -> PrintedWasteZone? in
                    guard let routingURL = printedWasteRegionalURL(
                        zoneId: zoneId,
                        title: mapping.data[zoneId]?.title,
                        regions: regions
                    ) else { return nil }
                    let components = zone.Region
                        .split(separator: "-", maxSplits: 1)
                        .map { String($0) }
                    let region = components.first ?? zone.Region
                    let suffix = components.count > 1 ? components[1] : zone.Region
                    return PrintedWasteZone(
                        id: zoneId,
                        title: mapping.data[zoneId]?.title?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty ?? zoneId,
                        region: region,
                        regionLabel: mapping.data[zoneId]?.region?.trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty
                            ?? (Self.regionMeta[region]?.label ?? region),
                        queuePosition: zone.QueuePosition,
                        etaMs: zone.eta,
                        zoneUrl: routingURL,
                        pingMs: previousZonesById[zoneId]?.pingMs,
                        isMeasuring: true,
                        regionSuffix: suffix,
                        gpuTier: mapping.data[zoneId]?.is5080Server == true ? "RTX 5080" :
                            (mapping.data[zoneId]?.is4080Server == true ? "RTX 4080" : nil)
                    )
                }
                .sorted { lhs, rhs in
                    if lhs.region == rhs.region {
                        return lhs.queuePosition < rhs.queuePosition
                    }
                    return lhs.region < rhs.region
                }

            guard !zones.isEmpty else {
                throw NSError(domain: "PrintedWaste", code: 4,
                    userInfo: [NSLocalizedDescriptionKey: "No selectable server has an advertised GeForce NOW route."])
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
    let title: String
    let alternateCount: Int
    let gpuTier: String?
    let isSelected: Bool
    let isManualSelection: Bool
    let isAuto: Bool
    let isClosest: Bool

    var body: some View {
        HStack(spacing: 14) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Text(title)
                        .font(.subheadline.weight(.semibold))
                        .lineLimit(1)
                        .minimumScaleFactor(0.85)
                        .layoutPriority(1)
                    Text(alternateCount > 0 ? "+\(alternateCount) servers" : zone.id)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .minimumScaleFactor(0.85)
                }
                HStack(spacing: 6) {
                    statusText("Queue \(zone.queuePosition)", color: queueColor(zone.queuePosition))
                    if let gpuTier { statusText(gpuTier, color: .secondary) }
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
        HStack(spacing: 3) {
            // Colour is never the only signal.
            if let level = pingLevel, level != .good, let glyph = level.glyph {
                Image(systemName: glyph).font(.caption2)
            }
            Group {
                if zone.isMeasuring {
                    Text("Testing")
                } else if let pingMs = zone.pingMs {
                    Text("\(pingMs) ms").monospacedDigit()
                } else {
                    Text("N/A")
                }
            }
        }
        .font(.caption.weight(.semibold))
        .lineLimit(1)
        .minimumScaleFactor(0.85)
        .foregroundStyle(pingBadgeColor)
    }

    private var pingLevel: StreamQualityLevel? {
        zone.pingMs.map(StreamQuality.serverPing)
    }

    /// Reads from the same ladder the in-stream HUD and the session report use. A latency the
    /// picker calls fine must not be one the HUD calls poor thirty seconds later.
    private var pingBadgeColor: Color {
        guard let level = pingLevel else { return .secondary }
        return level.tint ?? OpenNOWPalette.textPrimary
    }

    private func statusText(_ text: String, color: Color) -> some View {
        Text(text)
            .font(.caption2.weight(.semibold))
            .foregroundStyle(color)
    }

    private func queueColor(_ queue: Int) -> Color {
        if queue <= 15 { return OpenNOWPalette.textPrimary }
        if queue <= 40 { return OpenNOWPalette.statusFair }
        return OpenNOWPalette.statusPoor
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
                            .scaledToFit()
                            .frame(width: proxy.size.width, height: proxy.size.height)
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
            .onChangeCompat(of: pendingLaunchRequest?.id) { _ in
                guard !store.shouldPresentPrintedWasteQueue,
                      let request = pendingLaunchRequest else { return }
                store.scheduleLaunch(game: request.game, zoneUrl: nil, launchOption: request.launchOption)
                pendingLaunchRequest = nil
            }
            .opennowBottomSheet(item: sheetBinding, heightFraction: 0.86, maxHeight: 720) { request in
                PrintedWasteQueueView(game: request.game) { selectedZoneUrl in
                    store.scheduleLaunch(game: request.game, zoneUrl: selectedZoneUrl, launchOption: request.launchOption)
                }
                .environmentObject(store)
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
    let (data, response) = try await DiagnosticsHTTPRecorder.data(
        for: request,
        using: .shared,
        source: "PrintedWasteQueue"
    )
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
    let (data, response) = try await DiagnosticsHTTPRecorder.data(
        for: request,
        using: .shared,
        source: "PrintedWasteMapping"
    )
    guard let http = response as? HTTPURLResponse, http.statusCode == 200 else {
        throw NSError(domain: "PrintedWaste", code: 5, userInfo: [NSLocalizedDescriptionKey: "PrintedWaste mapping request failed"])
    }
    let decoded = try JSONDecoder().decode(PrintedWasteMappingResponse.self, from: data)
    guard decoded.status else {
        throw NSError(domain: "PrintedWaste", code: 6, userInfo: [NSLocalizedDescriptionKey: "PrintedWaste mapping returned status:false"])
    }
    return decoded
}
