import SwiftUI
import UIKit
import ImageIO
import GameController
import Combine
import CryptoKit

func catalogStableGameKey(_ game: CloudGame) -> String {
    if let uuid = game.uuid?.trimmingCharacters(in: .whitespacesAndNewlines), !uuid.isEmpty {
        return uuid.lowercased()
    }
    return game.id.lowercased()
}

@MainActor
final class CatalogControllerShortcutCoordinator: ObservableObject {
    @Published private(set) var isEnabled = false
    @Published private(set) var controllerConnected = false

    private struct FocusedActions {
        let owner: UUID
        let favorite: () -> Void
        let play: () -> Void
    }

    private var focusedActions: FocusedActions?
    private var attachedControllers: [GCController] = []
    private var notificationObservers: [NSObjectProtocol] = []

    init() {
        let center = NotificationCenter.default
        notificationObservers = [
            center.addObserver(
                forName: NSNotification.Name.GCControllerDidConnect,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                Task { @MainActor in self?.refreshControllers() }
            },
            center.addObserver(
                forName: NSNotification.Name.GCControllerDidDisconnect,
                object: nil,
                queue: .main
            ) { [weak self] _ in
                Task { @MainActor in self?.refreshControllers() }
            }
        ]
    }

    deinit {
        notificationObservers.forEach(NotificationCenter.default.removeObserver)
    }

    func setEnabled(_ enabled: Bool) {
        guard isEnabled != enabled else { return }
        isEnabled = enabled
        if enabled {
            refreshControllers()
        } else {
            focusedActions = nil
            detachControllerHandlers()
            controllerConnected = false
        }
    }

    func updateFocusedActions(
        owner: UUID,
        isFocused: Bool,
        favorite: @escaping () -> Void,
        play: @escaping () -> Void
    ) {
        if isFocused {
            focusedActions = FocusedActions(owner: owner, favorite: favorite, play: play)
        } else if focusedActions?.owner == owner {
            focusedActions = nil
        }
    }

    func clearFocusedActions(owner: UUID) {
        guard focusedActions?.owner == owner else { return }
        focusedActions = nil
    }

    private func refreshControllers() {
        guard isEnabled else { return }
        detachControllerHandlers()
        attachedControllers = GCController.controllers().filter { $0.extendedGamepad != nil }
        controllerConnected = !attachedControllers.isEmpty

        for controller in attachedControllers {
            controller.extendedGamepad?.buttonX.pressedChangedHandler = { [weak self] _, _, pressed in
                guard !pressed else { return }
                Task { @MainActor in
                    guard let self, self.isEnabled else { return }
                    self.focusedActions?.favorite()
                }
            }
            controller.extendedGamepad?.buttonY.pressedChangedHandler = { [weak self] _, _, pressed in
                guard !pressed else { return }
                Task { @MainActor in
                    guard let self, self.isEnabled else { return }
                    self.focusedActions?.play()
                }
            }
        }
    }

    private func detachControllerHandlers() {
        for controller in attachedControllers {
            controller.extendedGamepad?.buttonX.pressedChangedHandler = nil
            controller.extendedGamepad?.buttonY.pressedChangedHandler = nil
        }
        attachedControllers.removeAll()
    }
}

final class OpenNOWImageCache {
    static let shared = OpenNOWImageCache()

    private let cache = NSCache<NSString, UIImage>()

    private init() {
        cache.countLimit = 240
        cache.totalCostLimit = 96 * 1024 * 1024
    }

    static func configureURLCache() {
        URLCache.shared = URLCache(
            memoryCapacity: 64 * 1024 * 1024,
            diskCapacity: 256 * 1024 * 1024,
            diskPath: "OpenNOWURLCache"
        )
        Task(priority: .utility) {
            await OpenNOWImageDiskCache.prepare()
        }
    }

    func image(for url: URL, targetPixelSize: Int) -> UIImage? {
        cache.object(forKey: cacheKey(url: url, targetPixelSize: targetPixelSize))
    }

    func insert(_ image: UIImage, for url: URL, targetPixelSize: Int, cost: Int) {
        cache.setObject(image, forKey: cacheKey(url: url, targetPixelSize: targetPixelSize), cost: cost)
    }

    func removeAll() {
        cache.removeAllObjects()
    }

    static func removeAllPersistentImages() {
        Task(priority: .utility) {
            await OpenNOWImageDiskCache.removeAll()
        }
    }

    private func cacheKey(url: URL, targetPixelSize: Int) -> NSString {
        "\(url.absoluteString)#\(normalizedImageTargetPixelSize(targetPixelSize))" as NSString
    }
}

/// Artwork bytes on disk, keyed by URL.
///
/// Deliberately **not** an actor. Every method here is a file syscall against a path derived purely
/// from the URL, so there is no shared mutable state to protect — and an actor would have serialised
/// them: a screen of cards would queue its reads behind one another and behind every atomic write,
/// on one executor, while the six-wide network gate sat idle. The only thing that needs guarding is
/// how often the directory gets swept, which `OpenNOWImageDiskCachePruneClock` owns.
private enum OpenNOWImageDiskCache {
    private static let maximumAge: TimeInterval = 30 * 24 * 60 * 60
    private static let maximumBytes = 512 * 1024 * 1024

    private static let directoryURL: URL? = FileManager.default
        .urls(for: .cachesDirectory, in: .userDomainMask).first?
        .appendingPathComponent("OpenNOWArtwork", isDirectory: true)

    static func prepare() async {
        ensureDirectoryExists()
        guard await OpenNOWImageDiskCachePruneClock.shared.claim(force: true) else { return }
        prune()
    }

    /// The file holding this URL's bytes, if it exists and has not aged out. Callers decode from
    /// the file rather than from `Data`, so a cache hit never materialises the encoded image.
    static func freshFileURL(for url: URL) -> URL? {
        guard let fileURL = fileURL(for: url),
              let values = try? fileURL.resourceValues(
                forKeys: [.contentModificationDateKey, .fileSizeKey]
              ),
              (values.fileSize ?? 0) > 0,
              Date().timeIntervalSince(values.contentModificationDate ?? .distantPast) <= maximumAge else {
            return nil
        }
        return fileURL
    }

    static func store(_ data: Data, for url: URL) async {
        guard !data.isEmpty, let fileURL = fileURL(for: url) else { return }
        ensureDirectoryExists()
        do {
            try data.write(to: fileURL, options: .atomic)
        } catch {
            // Artwork can always be fetched again; cache writes must not block rendering.
            return
        }
        guard await OpenNOWImageDiskCachePruneClock.shared.claim(force: false) else { return }
        prune()
    }

    static func remove(for url: URL) {
        guard let fileURL = fileURL(for: url) else { return }
        try? FileManager.default.removeItem(at: fileURL)
    }

    static func removeAll() async {
        guard let directoryURL else { return }
        try? FileManager.default.removeItem(at: directoryURL)
        ensureDirectoryExists()
        await OpenNOWImageDiskCachePruneClock.shared.reset()
    }

    private static func ensureDirectoryExists() {
        guard let directoryURL else { return }
        try? FileManager.default.createDirectory(
            at: directoryURL,
            withIntermediateDirectories: true
        )
    }

    private static func fileURL(for url: URL) -> URL? {
        guard let directoryURL else { return nil }
        let digest = SHA256.hash(data: Data(url.absoluteString.utf8))
            .map { String(format: "%02x", $0) }
            .joined()
        return directoryURL.appendingPathComponent(digest).appendingPathExtension("image")
    }

    /// Only ever reached after the clock has granted a sweep, so the directory enumeration cannot
    /// land on the path that just wrote one file.
    private static func prune() {
        let now = Date()
        guard let directoryURL,
              let files = try? FileManager.default.contentsOfDirectory(
                at: directoryURL,
                includingPropertiesForKeys: [.contentModificationDateKey, .fileSizeKey],
                options: [.skipsHiddenFiles]
              ) else {
            return
        }

        var entries = files.compactMap { fileURL -> (url: URL, date: Date, bytes: Int)? in
            guard let values = try? fileURL.resourceValues(
                forKeys: [.contentModificationDateKey, .fileSizeKey]
            ) else {
                return nil
            }
            return (fileURL, values.contentModificationDate ?? .distantPast, values.fileSize ?? 0)
        }

        for entry in entries where now.timeIntervalSince(entry.date) > maximumAge {
            try? FileManager.default.removeItem(at: entry.url)
        }
        entries.removeAll { now.timeIntervalSince($0.date) > maximumAge }

        var totalBytes = entries.reduce(0) { $0 + $1.bytes }
        guard totalBytes > maximumBytes else { return }
        for entry in entries.sorted(by: { $0.date < $1.date }) where totalBytes > maximumBytes {
            try? FileManager.default.removeItem(at: entry.url)
            totalBytes -= entry.bytes
        }
    }
}

/// Decides when the artwork directory may be swept. Claiming is what makes a sweep exclusive, so a
/// hundred concurrent stores produce at most one enumeration an hour between them.
private actor OpenNOWImageDiskCachePruneClock {
    static let shared = OpenNOWImageDiskCachePruneClock()

    private static let interval: TimeInterval = 60 * 60
    private var lastPruneAt = Date.distantPast

    func claim(force: Bool) -> Bool {
        let now = Date()
        guard force || now.timeIntervalSince(lastPruneAt) >= Self.interval else { return false }
        lastPruneAt = now
        return true
    }

    func reset() {
        lastPruneAt = .distantPast
    }
}

private actor OpenNOWImageLoadGate {
    static let shared = OpenNOWImageLoadGate(limit: 6)

    private let limit: Int
    private var available: Int
    private var waiters: [(
        id: UUID,
        priority: TaskPriority,
        continuation: CheckedContinuation<Void, Error>
    )] = []

    init(limit: Int) {
        self.limit = limit
        self.available = limit
    }

    func acquire() async throws {
        if available > 0 {
            available -= 1
            return
        }

        let id = UUID()
        let priority = Task.currentPriority
        try await withTaskCancellationHandler {
            try await withCheckedThrowingContinuation { continuation in
                waiters.append((id, priority, continuation))
            }
        } onCancel: {
            Task {
                await self.cancelWaiter(id)
            }
        }
    }

    func release() {
        guard !waiters.isEmpty else {
            available = min(available + 1, limit)
            return
        }

        let nextIndex = waiters.indices.max {
            waiters[$0].priority.rawValue < waiters[$1].priority.rawValue
        } ?? waiters.startIndex
        let next = waiters.remove(at: nextIndex)
        next.continuation.resume()
    }

    private func cancelWaiter(_ id: UUID) {
        guard let index = waiters.firstIndex(where: { $0.id == id }) else { return }
        let waiter = waiters.remove(at: index)
        waiter.continuation.resume(throwing: CancellationError())
    }
}

private struct OpenNOWImageLoadRequest: Hashable {
    let url: URL
    let targetPixelSize: Int
}

private struct OpenNOWLoadedImage {
    let image: UIImage
    let cost: Int
    let data: Data
}

private enum OpenNOWImageDecoder {
    static func downsample(data: Data, targetPixelSize: Int) -> UIImage? {
        let options = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let source = CGImageSourceCreateWithData(data as CFData, options) else {
            return UIImage(data: data)
        }
        return downsample(source: source, targetPixelSize: targetPixelSize)
            ?? UIImage(data: data)
    }

    static func downsample(fileURL: URL, targetPixelSize: Int) -> UIImage? {
        let options = [kCGImageSourceShouldCache: false] as CFDictionary
        guard let source = CGImageSourceCreateWithURL(fileURL as CFURL, options) else {
            return UIImage(contentsOfFile: fileURL.path)
        }
        return downsample(source: source, targetPixelSize: targetPixelSize)
            ?? UIImage(contentsOfFile: fileURL.path)
    }

    static func downsampledImage(data: Data, targetPixelSize: Int) async throws -> UIImage {
        try await Task.detached(priority: .utility) {
            try Task.checkCancellation()
            guard let decoded = downsample(data: data, targetPixelSize: targetPixelSize) else {
                throw URLError(.cannotDecodeContentData)
            }
            return decoded
        }.value
    }

    static func downsampledImage(fileURL: URL, targetPixelSize: Int) async throws -> UIImage {
        try await Task.detached(priority: .utility) {
            try Task.checkCancellation()
            guard let decoded = downsample(fileURL: fileURL, targetPixelSize: targetPixelSize) else {
                throw URLError(.cannotDecodeContentData)
            }
            return decoded
        }.value
    }

    private static func downsample(source: CGImageSource, targetPixelSize: Int) -> UIImage? {
        let maxPixelSize = max(160, targetPixelSize)
        let downsampleOptions = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceShouldCacheImmediately: true,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceThumbnailMaxPixelSize: maxPixelSize
        ] as CFDictionary
        guard let cgImage = CGImageSourceCreateThumbnailAtIndex(source, 0, downsampleOptions) else {
            return nil
        }
        return UIImage(cgImage: cgImage, scale: UIScreen.main.scale, orientation: .up)
    }
}

private enum OpenNOWRemoteImageFetcher {
    /// Artwork gets its own session with no `URLCache`. `OpenNOWImageDiskCache` already holds every
    /// byte this fetcher returns, and is consulted before the fetcher runs at all, so the shared
    /// cache was writing a second copy of every image to disk and evicting real API responses to
    /// make room for it.
    private static let session: URLSession = {
        let configuration = URLSessionConfiguration.default
        configuration.urlCache = nil
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        configuration.networkServiceType = .responsiveData
        configuration.httpMaximumConnectionsPerHost = 6
        return URLSession(configuration: configuration)
    }()

    static func load(url: URL, targetPixelSize: Int) async throws -> OpenNOWLoadedImage {
        try Task.checkCancellation()
        try await OpenNOWImageLoadGate.shared.acquire()

        var request = URLRequest(url: url)
        request.timeoutInterval = 15

        let data: Data
        let response: URLResponse
        do {
            (data, response) = try await session.data(for: request)
        } catch {
            await OpenNOWImageLoadGate.shared.release()
            throw error
        }
        // The gate protects scarce network work, not CPU decoding. Returning the slot here lets the
        // next visible card start downloading while ImageIO down-samples this response off-main.
        await OpenNOWImageLoadGate.shared.release()
        try Task.checkCancellation()

        if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            throw URLError(.badServerResponse)
        }

        let image = try await OpenNOWImageDecoder.downsampledImage(
            data: data,
            targetPixelSize: targetPixelSize
        )

        let cost = image.cgImage.map { $0.bytesPerRow * $0.height } ?? data.count
        return OpenNOWLoadedImage(image: image, cost: cost, data: data)
    }
}

private actor OpenNOWRemoteImagePipeline {
    static let shared = OpenNOWRemoteImagePipeline()

    private var inFlight: [OpenNOWImageLoadRequest: Task<UIImage, Error>] = [:]

    func load(_ request: OpenNOWImageLoadRequest) async throws -> UIImage {
        if let cached = OpenNOWImageCache.shared.image(
            for: request.url,
            targetPixelSize: request.targetPixelSize
        ) {
            return cached
        }
        if let existing = inFlight[request] {
            return try await existing.value
        }

        let task = Task(priority: Task.currentPriority) {
            if let diskFile = OpenNOWImageDiskCache.freshFileURL(for: request.url) {
                do {
                    let diskImage = try await OpenNOWImageDecoder.downsampledImage(
                        fileURL: diskFile,
                        targetPixelSize: request.targetPixelSize
                    )
                    let cost = diskImage.cgImage.map { $0.bytesPerRow * $0.height } ?? 0
                    OpenNOWImageCache.shared.insert(
                        diskImage,
                        for: request.url,
                        targetPixelSize: request.targetPixelSize,
                        cost: cost
                    )
                    return diskImage
                } catch {
                    OpenNOWImageDiskCache.remove(for: request.url)
                }
            }

            let loaded = try await OpenNOWRemoteImageFetcher.load(
                url: request.url,
                targetPixelSize: request.targetPixelSize
            )
            OpenNOWImageCache.shared.insert(
                loaded.image,
                for: request.url,
                targetPixelSize: request.targetPixelSize,
                cost: loaded.cost
            )
            // First paint must not wait for an atomic disk write. The decoded image is already in
            // memory, so persist the reusable encoded bytes independently at utility priority.
            Task.detached(priority: .utility) {
                await OpenNOWImageDiskCache.store(loaded.data, for: request.url)
            }
            return loaded.image
        }
        inFlight[request] = task
        do {
            let image = try await task.value
            inFlight[request] = nil
            return image
        } catch {
            inFlight[request] = nil
            throw error
        }
    }
}

@MainActor
private final class CachedRemoteImageLoader: ObservableObject {
    @Published private(set) var image: UIImage?
    @Published private(set) var didFail = false

    private var loadedRequest: OpenNOWImageLoadRequest?

    func load(_ request: OpenNOWImageLoadRequest) async {
        if loadedRequest == request && image != nil { return }

        let previousRequest = loadedRequest
        loadedRequest = request
        didFail = false

        if let cached = OpenNOWImageCache.shared.image(for: request.url, targetPixelSize: request.targetPixelSize) {
            image = cached
            return
        }

        if previousRequest?.url != request.url {
            image = nil
        }

        do {
            let loaded = try await OpenNOWRemoteImagePipeline.shared.load(request)
            guard !Task.isCancelled, loadedRequest == request else { return }
            image = loaded
        } catch is CancellationError {
            if loadedRequest == request {
                didFail = false
            }
        } catch {
            if loadedRequest == request, image == nil {
                didFail = true
            }
        }
    }
}

struct CachedRemoteImage<Content: View, Placeholder: View, Failure: View>: View {
    let url: URL
    let targetPixelSize: Int
    // SwiftUI only creates these tasks for mounted views. Treat visible artwork as user-initiated so
    // it wins the load gate over speculative/off-screen work and appears with the surrounding card.
    var priority: TaskPriority = .userInitiated
    let content: (Image) -> Content
    let placeholder: () -> Placeholder
    let failure: () -> Failure

    @StateObject private var loader = CachedRemoteImageLoader()

    private var request: OpenNOWImageLoadRequest {
        OpenNOWImageLoadRequest(url: url, targetPixelSize: targetPixelSize)
    }

    var body: some View {
        Group {
            if let image = loader.image {
                content(Image(uiImage: image))
            } else if loader.didFail {
                failure()
            } else {
                placeholder()
            }
        }
        .task(id: request, priority: priority) {
            await loader.load(request)
        }
    }
}

struct CatalogWallpaperBackdrop: View {
    let isEnabled: Bool
    let managedFilename: String?
    /// Ignored when a custom image is set — a chosen photo always wins over a preset.
    var preset: CatalogWallpaperPreset = .colorfulAbstract

    @State private var image: UIImage?

    var body: some View {
        GeometryReader { proxy in
            let targetPixelSize = imageTargetPixelSize(for: proxy.size)
            let loadID = "\(isEnabled)-\(managedFilename ?? "gradient")-\((targetPixelSize + 159) / 160)"
            Group {
                if isEnabled {
                    ZStack {
                        if let image {
                            Image(uiImage: image)
                                .resizable()
                                .scaledToFill()
                                .transition(.opacity)
                        } else {
                            presetGradient
                        }

                        LinearGradient(
                            colors: [
                                Color(uiColor: .systemBackground).opacity(0.34),
                                Color(uiColor: .systemBackground).opacity(0.72)
                            ],
                            startPoint: .top,
                            endPoint: .bottom
                        )
                    }
                    .clipped()
                } else {
                    Color(uiColor: .systemBackground)
                }
            }
            .task(id: loadID) {
                await loadManagedWallpaper(targetPixelSize: targetPixelSize)
            }
        }
        .ignoresSafeArea()
        .allowsHitTesting(false)
    }

    /// Three built-in backdrops. They sit behind box art the whole time, so all of them stay
    /// dark and low-contrast — a wallpaper that competes with the artwork is a wallpaper that
    /// makes the catalog harder to scan.
    private var presetGradient: LinearGradient {
        switch preset {
        case .colorfulAbstract:
            return LinearGradient(
                colors: [
                    UIAccent.openNow.onDarkColor.opacity(0.55),
                    Color(hex: 0x1B3A6B).opacity(0.75),
                    Color(hex: 0x0A0E14)
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
        case .original:
            return brandGradient
        case .absoluteCinema:
            return LinearGradient(
                colors: [Color(hex: 0x101215), Color(hex: 0x05070A)],
                startPoint: .top,
                endPoint: .bottom
            )
        }
    }

    @MainActor
    private func loadManagedWallpaper(targetPixelSize: Int) async {
        guard isEnabled,
              let url = CatalogWallpaperStorage.wallpaperURL(for: managedFilename) else {
            image = nil
            return
        }
        let loaded = await Task.detached(priority: .utility) {
            OpenNOWImageDecoder.downsample(
                fileURL: url,
                targetPixelSize: targetPixelSize
            )
        }.value
        guard !Task.isCancelled else { return }
        withAnimation(.easeInOut(duration: 0.2)) {
            image = loaded
        }
    }
}

struct HomeView: View {
    private enum StoreSort: String, CaseIterable {
        case popular, title, lastPlayed

        var label: String {
            switch self {
            case .popular: return "Most Popular"
            case .title: return "Title A–Z"
            case .lastPlayed: return "Last Played"
            }
        }
    }

    @EnvironmentObject private var store: OpenNOWStore
    @State private var pendingLaunchRequest: GameLaunchRequest?
    @State private var selectedGameForDetails: CloudGame?
    @State private var selectedGameForLauncher: CloudGame?
    @State private var isSearchPresented = false
    @State private var sort = StoreSort.popular
    @State private var selectedStores = Set<String>()
    @State private var selectedGenres = Set<String>()

    private var continueCardWidth: CGFloat {
        let baseWidth: CGFloat = store.settings.compactGameCards ? 140 : 160
        let scale = CGFloat(min(max(store.settings.posterSizeScale, 0.75), 1.4))
        return baseWidth * scale
    }

    var body: some View {
        NavigationStack {
            GameCatalogGridView(
                games: homeGridGames,
                isLoading: store.isLoadingGames && store.allGames.isEmpty,
                emptyTitle: homeEmptyTitle,
                emptySystemImage: store.isSearchingCatalog ? "arrow.triangle.2.circlepath"
                    : (isHomeSearchActive ? "magnifyingglass" : "square.grid.2x2"),
                emptyDescription: homeEmptyDescription,
                topContentPadding: isHomeSearchActive ? 12 : 2,
                subtitle: { gameCatalogSubtitle(for: $0) },
                badgeSystemImage: { _ in nil },
                onOpenDetails: { selectedGameForDetails = $0 },
                onPlay: launchFromCard,
                onChooseLauncher: { selectedGameForLauncher = $0 }
            ) {
                homeHeader
            } emptyActions: {
                if (isHomeSearchActive || hasCatalogFilters) && !store.isSearchingCatalog {
                    Button(isHomeSearchActive && hasCatalogFilters ? "Clear Search and Filters" :
                           isHomeSearchActive ? "Clear Search" : "Clear Filters") {
                        store.searchText = ""
                        isSearchPresented = false
                        selectedStores.removeAll()
                        selectedGenres.removeAll()
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(brandAccent)
                }
            }
            .searchableCompat(
                text: $store.searchText,
                isPresented: $isSearchPresented,
                placement: .navigationBarDrawer(displayMode: .automatic),
                prompt: "Search games"
            )
            .refreshable { await store.refreshCatalog() }
            .navigationTitle("Store")
            .navigationBarTitleDisplayMode(.inline)
            .background {
                CatalogWallpaperBackdrop(
                    isEnabled: store.settings.catalogWallpaperEnabled,
                    managedFilename: store.settings.catalogWallpaperFilename,
                    preset: store.settings.catalogWallpaperPreset
                )
            }
        }
        .presentGameDetailsSheet(selectedGame: $selectedGameForDetails, store: store) { game, option in
            pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: option)
        }
        .launcherSelectionModalSheet(selectedGame: $selectedGameForLauncher, store: store) { game, option in
            pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: option)
        }
        .printedWasteLaunchSheet(pendingLaunchRequest: $pendingLaunchRequest)
    }

    private var homeHeader: some View {
        VStack(alignment: .leading, spacing: 22) {
            if !isResultsMode && !newGamesHeroGames.isEmpty {
                newGamesHeroSection
            }

            if let error = store.catalogError {
                ErrorBannerView(
                    message: error,
                    failure: store.lastFailure?.message == error ? store.lastFailure : nil,
                    onRecover: { store.performRecovery($0) },
                    onDismiss: { store.clearCatalogError() }
                )
            }

            if !isResultsMode && jumpBackInHasContent {
                continueSection
            }

            if !isResultsMode && !queueGames.isEmpty {
                CatalogPosterRail(title: "In queue", symbol: "hourglass", games: queueGames,
                    onOpenDetails: resumeQueueGame, onPlay: resumeQueueGame)
            }

            if !isResultsMode && !favoriteGames.isEmpty {
                CatalogPosterRail(title: "Favorites", symbol: "heart.fill", games: favoriteGames,
                    onOpenDetails: { selectedGameForDetails = $0 }, onPlay: launchFromCard,
                    onChooseLauncher: { selectedGameForLauncher = $0 })
            }

            CatalogControlsHeader(
                title: isResultsMode ? "Results" : "Recommendations",
                subtitle: homeHeaderTitle,
                chips: homeActiveFilterChips,
                onClear: isResultsMode ? {
                    store.searchText = ""
                    isSearchPresented = false
                    selectedStores.removeAll()
                    selectedGenres.removeAll()
                } : nil
            ) {
                HStack(spacing: 8) {
                    Menu {
                        ForEach(StoreSort.allCases, id: \.self) { option in
                            Button {
                                sort = option
                            } label: {
                                if sort == option { Label(option.label, systemImage: "checkmark") }
                                else { Text(option.label) }
                            }
                        }
                    } label: {
                        Label("Sort", systemImage: "arrow.up.arrow.down")
                    }

                    Menu {
                        if !availableStoreFilters.isEmpty {
                            Section("Stores") {
                                ForEach(availableStoreFilters, id: \.self) { storeID in
                                    Button {
                                        toggle(storeID, in: &selectedStores)
                                    } label: {
                                        if selectedStores.contains(storeID) {
                                            Label(storeDisplayName(storeID), systemImage: "checkmark")
                                        } else { Text(storeDisplayName(storeID)) }
                                    }
                                }
                            }
                        }
                        if !availableGenreFilters.isEmpty {
                            Section("Genres") {
                                ForEach(availableGenreFilters, id: \.self) { genre in
                                    Button {
                                        toggle(genre, in: &selectedGenres)
                                    } label: {
                                        if selectedGenres.contains(genre) {
                                            Label(genre, systemImage: "checkmark")
                                        } else { Text(genre) }
                                    }
                                }
                            }
                        }
                        if hasCatalogFilters {
                            Button("Clear Filters", systemImage: "xmark.circle") {
                                selectedStores.removeAll()
                                selectedGenres.removeAll()
                            }
                        }
                    } label: {
                        Label(hasCatalogFilters ? "Filter \(selectedStores.count + selectedGenres.count)" : "Filter",
                              systemImage: "line.3.horizontal.decrease")
                    }
                }
                .font(.subheadline)
            }
        }
    }

    private var continueSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Continue playing")
                .font(.title2.weight(.bold))

            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: 12) {
                    ForEach(continueGameItems) { item in
                        GameBannerButton(
                            game: item.game,
                            subtitle: item.subtitle,
                            badgeSystemImage: item.badgeSystemImage,
                            showsTitle: store.settings.showCardTitles
                        ) {
                            item.onSelect()
                        }
                        .frame(width: continueCardWidth)
                    }

                    ForEach(unknownResumableSessions) { candidate in
                        Button {
                            Haptics.light()
                            store.scheduleResume(candidate: candidate)
                        } label: {
                            Label("Cloud Session", systemImage: "arrow.clockwise.circle")
                                .frame(maxWidth: .infinity, alignment: .leading)
                        }
                        .buttonStyle(.bordered)
                        .frame(width: continueCardWidth)
                    }
                }
                .padding(.horizontal, 2)
                .padding(.vertical, 8)
            }
            .accessibilityLabel("Continue games and sessions")
        }
    }

    private var newGamesHeroSection: some View {
        ComingNextCarousel(
            games: newGamesHeroGames,
            isPaused: selectedGameForDetails != nil || selectedGameForLauncher != nil || pendingLaunchRequest != nil,
            onOpenDetails: { selectedGameForDetails = $0 }
        )
    }

    private var newGamesHeroGames: [CloudGame] {
        newlyAddedStoreHeroGames(
            games: store.allGames,
            excludedGameKeys: newGamesExcludedGameKeys
        )
    }

    private var newGamesExcludedGameKeys: Set<String> {
        var keys = Set(continueGameItems.map { catalogStableGameKey($0.game) })
        keys.formUnion(queueGames.map(catalogStableGameKey))
        keys.formUnion(favoriteGames.map(catalogStableGameKey))
        if let activeGame = store.activeSession?.game {
            keys.insert(catalogStableGameKey(activeGame))
        }
        for candidate in store.resumableSessions {
            if let game = store.gameForRemoteSession(candidate) {
                keys.insert(catalogStableGameKey(game))
            }
        }
        return keys
    }

    private var queueGames: [CloudGame] {
        var seen = Set<String>()
        var games: [CloudGame] = []
        if let active = store.activeSession, active.status == 1,
           seen.insert(catalogStableGameKey(active.game)).inserted {
            games.append(active.game)
        }
        for candidate in store.resumableSessions where candidate.status == 1 {
            guard let game = store.gameForRemoteSession(candidate),
                  seen.insert(catalogStableGameKey(game)).inserted else { continue }
            games.append(game)
        }
        return Array(games.prefix(8))
    }

    private func resumeQueueGame(_ game: CloudGame) {
        if let active = store.activeSession, catalogStableGameKey(active.game) == catalogStableGameKey(game) {
            store.jumpBackToSession()
            return
        }
        if let candidate = store.resumableSessions.first(where: {
            $0.status == 1 && store.gameForRemoteSession($0).map(catalogStableGameKey) == catalogStableGameKey(game)
        }) {
            store.scheduleResume(candidate: candidate)
        }
    }

    private var favoriteGames: [CloudGame] {
        let byId = Dictionary((store.allGames + store.libraryGames).map { ($0.id, $0) },
            uniquingKeysWith: { first, _ in first })
        let alreadyShown = Set((continueGameItems.map(\.game) + queueGames).map(catalogStableGameKey))
        return Array(store.settings.favoriteGameIds.compactMap { byId[$0] }
            .filter { !alreadyShown.contains(catalogStableGameKey($0)) }.prefix(14))
    }

    private var homeGridGames: [CloudGame] {
        let source = isHomeSearchActive ? homeSearchResults : store.allGames
        let filtered = source.filter { game in
            let storeMatch = selectedStores.isEmpty || game.launchOptions.contains {
                selectedStores.contains($0.storefront.uppercased())
            }
            let genreMatch = selectedGenres.isEmpty || selectedGenres.contains(game.genre)
            return storeMatch && genreMatch
        }
        switch sort {
        case .popular: return filtered
        case .title:
            return filtered.sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
        case .lastPlayed:
            return filtered.enumerated().sorted { left, right in
                let leftDate = left.element.lastPlayedDate ?? ""
                let rightDate = right.element.lastPlayedDate ?? ""
                return leftDate == rightDate ? left.offset < right.offset : leftDate > rightDate
            }.map(\.element)
        }
    }

    private var hasCatalogFilters: Bool { !selectedStores.isEmpty || !selectedGenres.isEmpty }
    private var isResultsMode: Bool { isHomeSearchActive || hasCatalogFilters }

    private var availableStoreFilters: [String] {
        Array(Set(store.allGames.flatMap { $0.launchOptions.map { $0.storefront.uppercased() } }))
            .filter { !$0.isEmpty && $0 != "AUTO" }.sorted()
    }

    private var availableGenreFilters: [String] {
        Array(Set(store.allGames.map(\.genre)))
            .filter { !$0.isEmpty && $0 != "Cloud Game" }.sorted()
    }

    private func toggle(_ value: String, in selected: inout Set<String>) {
        if !selected.insert(value).inserted { selected.remove(value) }
    }

    /// Searching the server takes a moment; saying so beats showing "No Matches" and then
    /// silently filling the grid a second later.
    private var homeEmptyTitle: String {
        if store.isSearchingCatalog { return "Searching the catalog…" }
        return isResultsMode ? "No Matches" : "No Games"
    }

    private var homeEmptyDescription: String? {
        if store.isSearchingCatalog { return nil }
        guard isResultsMode else { return nil }
        return "No games match the selected search or filters. Try changing them."
    }

    private var homeHeaderTitle: String {
        let count = homeGridGames.count
        if isResultsMode {
            return count == 1 ? "1 Match" : "\(count) Matches"
        }
        return count == 1 ? "1 Game" : "\(count) Games"
    }

    private var homeActiveFilterChips: [CatalogFilterChip] {
        let query = store.searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        var chips: [CatalogFilterChip] = []
        if !query.isEmpty {
            chips.append(CatalogFilterChip(label: "Search: \(query)") {
                store.searchText = ""
                isSearchPresented = false
            })
        }
        for storeID in selectedStores.sorted() {
            chips.append(CatalogFilterChip(label: storeDisplayName(storeID)) {
                selectedStores.remove(storeID)
            })
        }
        for genre in selectedGenres.sorted() {
            chips.append(CatalogFilterChip(label: genre) {
                selectedGenres.remove(genre)
            })
        }
        return chips
    }

    private var jumpBackInHasContent: Bool {
        !continueGameItems.isEmpty || !unknownResumableSessions.isEmpty
    }

    private var isHomeSearchActive: Bool {
        !store.searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var homeSearchResults: [CloudGame] {
        store.filteredCatalogGames
    }

    private func launchFromCard(_ game: CloudGame) {
        switch store.launchChoice(for: game) {
        case .chooseLauncher:
            selectedGameForLauncher = game
        case let .launch(option):
            pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: option)
        }
    }

    private var resumableSessionsExcludingActive: [RemoteSessionCandidate] {
        let activeId = store.activeSession?.id
        return store.resumableSessions.filter { $0.id != activeId }
    }

    private var continueGameItems: [GameBannerActionItem] {
        var items: [GameBannerActionItem] = []
        var seenGameKeys = Set<String>()
        if let active = store.activeSession, active.status != 1 {
            seenGameKeys.insert(catalogStableGameKey(active.game))
            items.append(
                GameBannerActionItem(
                    id: "active-\(active.id)",
                    game: active.game,
                    subtitle: jumpBackInSubtitleActive(active),
                    badgeSystemImage: active.status == 3 ? "play.circle.fill" : "hourglass"
                ) {
                    store.jumpBackToSession()
                }
            )
        }

        for candidate in resumableSessionsExcludingActive.filter({ $0.status != 1 }).prefix(6) {
            guard let game = store.gameForRemoteSession(candidate) else { continue }
            guard seenGameKeys.insert(catalogStableGameKey(game)).inserted else { continue }
            items.append(
                GameBannerActionItem(
                    id: "remote-\(candidate.id)",
                    game: game,
                    subtitle: "Resume session",
                    badgeSystemImage: "arrow.clockwise.circle"
                ) {
                    store.scheduleResume(candidate: candidate)
                }
            )
        }

        let recentGames = (store.libraryGames + store.allGames)
            .filter { $0.lastPlayedDate != nil }
            .sorted { ($0.lastPlayedDate ?? "") > ($1.lastPlayedDate ?? "") }
        for game in recentGames where items.count < 12 {
            guard seenGameKeys.insert(catalogStableGameKey(game)).inserted else { continue }
            items.append(GameBannerActionItem(
                id: "recent-\(catalogStableGameKey(game))",
                game: game,
                subtitle: "Recently played",
                badgeSystemImage: nil
            ) {
                selectedGameForDetails = game
            })
        }

        return items
    }

    private var unknownResumableSessions: [RemoteSessionCandidate] {
        resumableSessionsExcludingActive
            .prefix(6)
            .filter { $0.status != 1 && store.gameForRemoteSession($0) == nil }
    }

    private func jumpBackInSubtitleActive(_ session: ActiveSession) -> String {
        switch session.status {
        case 3:
            guard store.supportsEmbeddedStreamer else { return "Ready on another platform" }
            return store.streamSession == nil ? "Ready to return" : "Streaming"
        case 2:
            return "Connecting"
        default:
            if let queue = session.queuePosition {
                return queue == 1 ? "Next in queue" : "Queue #\(queue)"
            }
            return "Queued"
        }
    }
}

private let gameVerticalBannerAspectRatio: CGFloat = 2.0 / 3.0

struct GameBannerRowGroup: Identifiable {
    let id: String
    let games: [CloudGame]
}

struct GameBannerActionItem: Identifiable {
    let id: String
    let game: CloudGame
    let subtitle: String?
    let badgeSystemImage: String?
    let onSelect: () -> Void
}

func gameBannerRows(for games: [CloudGame]) -> [GameBannerRowGroup] {
    guard !games.isEmpty else { return [] }
    var rows: [GameBannerRowGroup] = []
    rows.reserveCapacity((games.count + 1) / 2)

    var index = 0
    while index < games.count {
        let rowGames = Array(games[index..<min(index + 2, games.count)])
        rows.append(GameBannerRowGroup(id: rowGames.map(\.id).joined(separator: "|"), games: rowGames))
        index += 2
    }

    return rows
}

/// Android's Store hero is the provider's weekly GFN Thursday section, in provider order. Keep
/// that exact contract here: section-title guesses can silently turn a named feed into unrelated
/// games, and alphabetical sorting destroys the weekly order users expect.
func newlyAddedStoreHeroGames(
    games: [CloudGame],
    excludedGameKeys: Set<String> = [],
    limit: Int = 6
) -> [CloudGame] {
    guard limit > 0 else { return [] }
    let sectionIDPrefix = "section-cbc43218-6ad6-4ff3-8538-bc84f90c796c-"
    var seen = Set<String>()
    let providerOrdered = games.filter { game in
        let isWeeklySection = game.catalogSectionTitle?
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .localizedCaseInsensitiveCompare("GFN Thursday") == .orderedSame
        let isWeeklySectionID = game.catalogSectionId?.hasPrefix(sectionIDPrefix) == true
        guard isWeeklySection || isWeeklySectionID else { return false }
        return seen.insert(catalogStableGameKey(game)).inserted
    }
    let nonRepeating = providerOrdered.filter {
        !excludedGameKeys.contains(catalogStableGameKey($0))
    }
    return Array((nonRepeating.isEmpty ? providerOrdered : nonRepeating).prefix(limit))
}

struct CatalogFilterChip: Identifiable {
    let label: String
    let onRemove: () -> Void

    var id: String { label }
}

struct CatalogControlsHeader<Controls: View>: View {
    let title: String
    let subtitle: String?
    let chips: [CatalogFilterChip]
    let onClear: (() -> Void)?
    @ViewBuilder let controls: () -> Controls

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .center, spacing: 10) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title)
                        .font(.headline.weight(.semibold))
                    if let subtitle, !subtitle.isEmpty {
                        Text(subtitle)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.secondary)
                    }
                }
                Spacer(minLength: 8)
                controls()
            }

            if !chips.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(chips) { chip in
                            CatalogFilterChipButton(chip: chip)
                        }
                        if let onClear {
                            Button("Clear") {
                                onClear()
                            }
                            .font(.caption.weight(.semibold))
                            .buttonStyle(.bordered)
                            .controlSize(.small)
                        }
                    }
                    .padding(.vertical, 1)
                }
            }
        }
    }
}

private struct CatalogFilterChipButton: View {
    let chip: CatalogFilterChip

    var body: some View {
        Button {
            chip.onRemove()
        } label: {
            HStack(spacing: 6) {
                Text(chip.label)
                    .lineLimit(1)
                Image(systemName: "xmark")
                    .font(.caption2.weight(.bold))
            }
        }
        .font(.caption.weight(.semibold))
        .buttonStyle(.bordered)
        .controlSize(.small)
        .tint(.secondary)
    }
}

struct GameCatalogGridView<Header: View, EmptyActions: View>: View {
    @EnvironmentObject private var store: OpenNOWStore
    let games: [CloudGame]
    let isLoading: Bool
    let emptyTitle: String
    let emptySystemImage: String
    /// One line under the empty-state title saying what actually happened. Optional because not
    /// every caller has a cause worth naming — but when there is one, it belongs here rather
    /// than being folded into the title.
    var emptyDescription: String? = nil
    var topContentPadding: CGFloat = 12
    let subtitle: (CloudGame) -> String
    let badgeSystemImage: (CloudGame) -> String?
    let onOpenDetails: (CloudGame) -> Void
    let onPlay: (CloudGame) -> Void
    var onChooseLauncher: ((CloudGame) -> Void)? = nil
    @ViewBuilder let header: () -> Header
    @ViewBuilder let emptyActions: () -> EmptyActions

    private var columns: [GridItem] {
        let scale = CGFloat(min(max(store.settings.posterSizeScale, 0.75), 1.4))
        let baseMinimum: CGFloat = store.settings.compactGameCards ? 132 : 154
        let baseMaximum: CGFloat = store.settings.compactGameCards ? 196 : 230
        return [
            GridItem(
                .adaptive(minimum: baseMinimum * scale, maximum: baseMaximum * scale),
                spacing: 10,
                alignment: .top
            )
        ]
    }

    var body: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 12) {
                header()
                    .padding(.horizontal, 14)

                if isLoading && games.isEmpty {
                    LazyVGrid(columns: columns, spacing: 12) {
                        ForEach(0..<8, id: \.self) { _ in
                            GameCatalogGridSkeletonCard()
                        }
                    }
                    .shimmeringSkeleton()
                    .padding(.horizontal, 12)
                } else if games.isEmpty {
                    OpenNOWUnavailableView(emptyTitle, systemImage: emptySystemImage) {
                        if let emptyDescription {
                            Text(emptyDescription)
                        }
                    } actions: {
                        emptyActions()
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.horizontal, 18)
                    .padding(.top, 42)
                } else {
                    LazyVGrid(columns: columns, spacing: 12) {
                        ForEach(games) { game in
                            let favorite = store.isFavorite(game)
                            let canLaunch = OpenNOWPlatform.supportsEmbeddedStreamer
                                && !store.launchOptions(for: game).isEmpty
                            GameCatalogGridCard(
                                game: game,
                                subtitle: store.settings.showGameStoreLabels ? subtitle(game) : nil,
                                badgeSystemImage: badgeSystemImage(game),
                                compact: store.settings.compactGameCards,
                                favorite: favorite,
                                canLaunch: canLaunch,
                                showsTitle: store.settings.showCardTitles,
                                alwaysShowsFavorite: store.settings.showFavoriteIconOnGameCards,
                                onToggleFavorite: { store.toggleFavorite(game) },
                                onOpenDetails: { onOpenDetails(game) },
                                onPlay: { onPlay(game) },
                                onChooseLauncher: onChooseLauncher.map { choose in { choose(game) } }
                            )
                        }
                    }
                    .padding(.horizontal, 12)
                }
            }
            .padding(.top, topContentPadding)
            .padding(.bottom, 12)
        }
        .scrollDismissesKeyboard(.interactively)
    }
}

private struct GameCatalogGridCard: View {
    @EnvironmentObject private var controllerShortcuts: CatalogControllerShortcutCoordinator
    @Environment(\.gameDetailsTransition) private var detailsTransition
    @State private var detailsSourceID = UUID()
    @FocusState private var isPosterFocused: Bool
    @State private var isLegacyPosterFocused = false
    @State private var controllerShortcutOwner = UUID()
    let game: CloudGame
    let subtitle: String?
    let badgeSystemImage: String?
    let compact: Bool
    let favorite: Bool
    let canLaunch: Bool
    /// Title caption under the artwork. Off leaves the grid as pure box art.
    var showsTitle: Bool = true
    /// Whether the heart is on every card. When off it appears only on games already favourited
    /// or on the focused card — the long-press menu still reaches it, so nothing is lost.
    var alwaysShowsFavorite: Bool = true
    let onToggleFavorite: () -> Void
    let onOpenDetails: () -> Void
    let onPlay: () -> Void
    var onChooseLauncher: (() -> Void)? = nil

    private var controlSize: CGFloat {
        compact ? 36 : 42
    }

    private var showsFavoriteControl: Bool {
        alwaysShowsFavorite || favorite || isPosterVisuallyFocused
    }

    private var isPosterVisuallyFocused: Bool {
        isPosterFocused || isLegacyPosterFocused
    }

    private func openDetails() {
        Haptics.light()
        detailsTransition?.selectSource(GameDetailsTransitionOrigin(
            sourceID: detailsSourceID, gameKey: catalogStableGameKey(game)
        ))
        onOpenDetails()
    }

    private func toggleFavorite() {
        Haptics.light()
        onToggleFavorite()
    }

    private func play() {
        guard canLaunch else { return }
        Haptics.medium()
        onPlay()
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            poster
            if showsTitle {
                caption
            }
        }
        // Long-press reaches every action the card offers, including the favourite toggle when
        // the heart is hidden. `preview:` shows the artwork at size rather than a cropped cell.
        .contextMenu {
            if canLaunch {
                Button { play() } label: { Label("Play", systemImage: "play.fill") }
            }
            if canLaunch, let onChooseLauncher, game.launchOptions.count > 1 {
                Button(action: onChooseLauncher) {
                    Label("Choose Launcher", systemImage: "rectangle.stack")
                }
            }
            Button { toggleFavorite() } label: {
                Label(
                    favorite ? "Remove from Favourites" : "Add to Favourites",
                    systemImage: favorite ? "heart.slash" : "heart"
                )
            }
            Button { openDetails() } label: { Label("Details", systemImage: "info.circle") }
        }
        // The buttons on the artwork are already exposed individually; repeating them as custom
        // actions would make VoiceOver read the same card three times.
        .accessibilityElement(children: .contain)
        #if DEBUG
        .gameDetailsVisualQA(game: game, source: "grid", activate: openDetails)
        #endif
    }

    @ViewBuilder
    private var caption: some View {
        VStack(alignment: .leading, spacing: 1) {
            Text(game.title)
                .font(.caption.weight(.semibold))
                .lineLimit(2)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
            if let subtitle, !subtitle.isEmpty {
                Text(subtitle)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.horizontal, 2)
        // The poster button already announces the title; repeating it here would double it.
        .accessibilityHidden(true)
    }

    private var poster: some View {
        ZStack(alignment: .bottom) {
            Button(action: openDetails) {
                GameCatalogPosterContent(
                    game: game,
                    subtitle: subtitle,
                    badgeSystemImage: badgeSystemImage,
                    compact: compact,
                    isFocused: isPosterVisuallyFocused
                )
                .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                .gameDetailsArtworkSource(id: detailsSourceID)
            }
            .buttonStyle(.plain)
            .controllerFocusableCompat(
                fallbackActivation: openDetails,
                onLegacyFocusChange: { isLegacyPosterFocused = $0 }
            )
            .focused($isPosterFocused)
            .scaleEffect(isPosterVisuallyFocused ? 1.025 : 1)
            .animation(.easeOut(duration: 0.16), value: isPosterVisuallyFocused)
            .accessibilityLabel("Open details for \(game.title)")

            HStack(alignment: .bottom) {
                if showsFavoriteControl {
                    Button(action: toggleFavorite) {
                        Image(systemName: favorite ? "heart.fill" : "heart")
                            .font(.headline.weight(.bold))
                            .foregroundStyle(favorite ? Color.red : Color.white)
                            .artworkControlChip(diameter: controlSize)
                    }
                    .buttonStyle(.plain)
                    .controllerFocusableCompat(fallbackActivation: toggleFavorite)
                    .contentShape(Circle())
                    .accessibilityLabel(favorite ? "Remove \(game.title) from favorites" : "Add \(game.title) to favorites")
                }

                Spacer(minLength: 8)

                Button(action: play) {
                    Image(systemName: "play.fill")
                        .font(.headline.weight(.bold))
                        .foregroundStyle(Color.white)
                        .artworkControlChip(
                            diameter: controlSize,
                            fill: brandAccent.opacity(canLaunch ? 0.96 : 0.45)
                        )
                }
                .buttonStyle(.plain)
                .controllerFocusableCompat(fallbackActivation: play)
                .contentShape(Circle())
                .accessibilityLabel("Launch \(game.title)")
                .disabled(!canLaunch)
            }
            .padding(compact ? 6 : 8)
            .zIndex(1)
        }
        .overlay(alignment: .topTrailing) {
            if isPosterVisuallyFocused,
               controllerShortcuts.isEnabled,
               controllerShortcuts.controllerConnected {
                CatalogControllerShortcutHint(
                    favorite: favorite,
                    playEnabled: canLaunch
                )
                .padding(6)
                .transition(.opacity)
            }
        }
        .onAppear {
            updateControllerShortcutRegistration(isPosterVisuallyFocused)
        }
        .onChangeCompat(of: isPosterVisuallyFocused) { focused in
            updateControllerShortcutRegistration(focused)
        }
        .onChangeCompat(of: controllerShortcuts.isEnabled) { enabled in
            updateControllerShortcutRegistration(enabled && isPosterVisuallyFocused)
        }
        .onDisappear {
            controllerShortcuts.clearFocusedActions(owner: controllerShortcutOwner)
        }
        .zIndex(isPosterVisuallyFocused ? 2 : 0)
    }

    private func updateControllerShortcutRegistration(_ focused: Bool) {
        controllerShortcuts.updateFocusedActions(
            owner: controllerShortcutOwner,
            isFocused: focused,
            favorite: { toggleFavorite() },
            play: { play() }
        )
    }
}

private struct CatalogControllerShortcutHint: View {
    let favorite: Bool
    let playEnabled: Bool

    var body: some View {
        HStack(spacing: 5) {
            shortcut(button: "X", systemImage: favorite ? "heart.fill" : "heart")
            shortcut(button: "Y", systemImage: "play.fill")
                .opacity(playEnabled ? 1 : 0.45)
        }
        .padding(5)
        .background(.black.opacity(0.72), in: Capsule())
        .overlay(Capsule().stroke(Color.white.opacity(0.16), lineWidth: 1))
        .allowsHitTesting(false)
        .accessibilityHidden(true)
    }

    private func shortcut(button: String, systemImage: String) -> some View {
        HStack(spacing: 3) {
            Text(button)
                .font(.caption2.bold())
                .foregroundStyle(.white)
                .frame(width: 17, height: 17)
                .background(Color.white.opacity(0.18), in: Circle())
            Image(systemName: systemImage)
                .font(.caption2.weight(.bold))
                .foregroundStyle(.white)
        }
    }
}

private struct GameCatalogPosterContent: View {
    let game: CloudGame
    let subtitle: String?
    let badgeSystemImage: String?
    let compact: Bool
    let isFocused: Bool

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            Color.black

            GameArtworkView(game: game, iconSize: 42)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .aspectRatio(gameVerticalBannerAspectRatio, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(alignment: .topTrailing) {
            if let badgeSystemImage {
                Image(systemName: badgeSystemImage)
                    .font(.caption.weight(.bold))
                    .foregroundStyle(.white)
                    .frame(width: 28, height: 28)
                    .background(badgeBackgroundColor.opacity(0.92), in: Circle())
                    .padding(8)
            }
        }
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(
                    isFocused ? brandAccent : Color.white.opacity(0.10),
                    lineWidth: isFocused ? 2 : 1
                )
        )
        .artworkCardShadow(cornerRadius: 12)
        .accessibilityLabel(game.title)
        .accessibilityValue(subtitle ?? "")
    }

    private var badgeBackgroundColor: Color {
        badgeSystemImage == "heart.fill" ? .red : brandAccent
    }
}

private struct GameCatalogGridSkeletonCard: View {
    var body: some View {
        ZStack(alignment: .bottom) {
            Color.secondary.opacity(0.16)

            HStack {
                Circle()
                    .fill(Color.white.opacity(0.18))
                    .frame(width: 42, height: 42)
                Spacer()
                Circle()
                    .fill(Color.white.opacity(0.18))
                    .frame(width: 42, height: 42)
            }
            .padding(8)
        }
        .aspectRatio(gameVerticalBannerAspectRatio, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(Color.white.opacity(0.08), lineWidth: 1)
        )
        .accessibilityHidden(true)
    }
}

struct GameBannerRowView: View {
    let games: [CloudGame]
    let subtitle: (CloudGame) -> String
    var badgeSystemImage: (CloudGame) -> String? = { _ in nil }
    let onSelect: (CloudGame) -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            ForEach(games) { game in
                GameBannerButton(
                    game: game,
                    subtitle: subtitle(game),
                    badgeSystemImage: badgeSystemImage(game)
                ) {
                    onSelect(game)
                }
                .frame(maxWidth: .infinity)
            }

            if games.count == 1 {
                Color.clear
                    .aspectRatio(gameVerticalBannerAspectRatio, contentMode: .fit)
                    .frame(maxWidth: .infinity)
                    .accessibilityHidden(true)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

struct GameBannerSkeletonRowView: View {
    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            GameBannerSkeletonCard()
                .frame(maxWidth: .infinity)
            GameBannerSkeletonCard()
                .frame(maxWidth: .infinity)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct GameBannerSkeletonCard: View {
    var body: some View {
        ZStack(alignment: .bottomLeading) {
            LinearGradient(
                colors: [
                    Color.secondary.opacity(0.22),
                    Color.secondary.opacity(0.12),
                    Color.black.opacity(0.16)
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )

            VStack(alignment: .leading, spacing: 7) {
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(Color.white.opacity(0.24))
                    .frame(maxWidth: .infinity)
                    .frame(height: 9)
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(Color.white.opacity(0.18))
                    .frame(width: 74, height: 8)
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(Color.white.opacity(0.14))
                    .frame(width: 46, height: 7)
            }
            .padding(10)
        }
        .aspectRatio(gameVerticalBannerAspectRatio, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(Color.white.opacity(0.08), lineWidth: 1)
        )
        .accessibilityHidden(true)
    }
}

struct GameBannerButton: View {
    @FocusState private var isFocused: Bool
    @State private var isLegacyFocused = false
    let game: CloudGame
    let subtitle: String?
    let badgeSystemImage: String?
    var showsTitle = true
    let onSelect: () -> Void

    private func select() {
        Haptics.light()
        onSelect()
    }

    private var isVisuallyFocused: Bool {
        isFocused || isLegacyFocused
    }

    var body: some View {
        Button(action: select) {
            GameVerticalBannerCard(
                game: game,
                subtitle: subtitle,
                badgeSystemImage: badgeSystemImage,
                showsTitle: showsTitle,
                isFocused: isVisuallyFocused
            )
            .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        }
        .buttonStyle(.plain)
        .controllerFocusableCompat(
            fallbackActivation: select,
            onLegacyFocusChange: { isLegacyFocused = $0 }
        )
        .focused($isFocused)
        .accessibilityLabel([game.title, subtitle].compactMap { $0 }.joined(separator: ", "))
        .scaleEffect(isVisuallyFocused ? 1.025 : 1)
        .animation(.easeOut(duration: 0.16), value: isVisuallyFocused)
        .zIndex(isVisuallyFocused ? 2 : 0)
    }
}

struct GameVerticalBannerCard: View {
    let game: CloudGame
    let subtitle: String?
    let badgeSystemImage: String?
    var fitArtwork = false
    var showsTitle = true
    var isFocused = false

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            GameArtworkView(game: game, iconSize: 42, fit: fitArtwork)
                .frame(maxWidth: .infinity, maxHeight: .infinity)

            if showsTitle {
                LinearGradient(
                    colors: [.clear, .black.opacity(0.22), .black.opacity(0.88)],
                    startPoint: .top,
                    endPoint: .bottom
                )
                .allowsHitTesting(false)

                VStack(alignment: .leading, spacing: 4) {
                    Text(game.title)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(Color.white)
                        .lineLimit(3)
                        .minimumScaleFactor(0.78)
                        .fixedSize(horizontal: false, vertical: true)

                    GameCapabilityBadges(labels: game.capabilityBadges)

                    if let subtitle, !subtitle.isEmpty {
                        Text(subtitle)
                            .font(.caption2.weight(.medium))
                            .foregroundStyle(Color.white.opacity(0.82))
                            .lineLimit(1)
                    }
                }
                .padding(10)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .aspectRatio(gameVerticalBannerAspectRatio, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(alignment: .topTrailing) {
            if let badgeSystemImage {
                Image(systemName: badgeSystemImage)
                    .font(.caption.weight(.bold))
                    .foregroundStyle(.white)
                    .frame(width: 28, height: 28)
                    .background(badgeBackgroundColor.opacity(0.92), in: Circle())
                    .padding(8)
            }
        }
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(
                    isFocused ? brandAccent : Color.white.opacity(0.10),
                    lineWidth: isFocused ? 2 : 1
                )
        )
        .artworkCardShadow(cornerRadius: 12)
        .accessibilityElement(children: .combine)
    }

    private var badgeBackgroundColor: Color {
        badgeSystemImage == "heart.fill" ? .red : brandAccent
    }
}

private struct GameLaunchDetailsArtworkCard: View {
    let game: CloudGame
    let subtitle: String?
    let badgeSystemImage: String?

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            GameLaunchDetailsArtwork(game: game)
                .frame(maxWidth: .infinity, maxHeight: .infinity)

            LinearGradient(
                colors: [.clear, .black.opacity(0.20), .black.opacity(0.82)],
                startPoint: .top,
                endPoint: .bottom
            )
            .allowsHitTesting(false)

            VStack(alignment: .leading, spacing: 4) {
                Text(game.title)
                    .font(.system(.title2, design: .rounded, weight: .bold))
                    .foregroundStyle(.white)
                    .lineLimit(2)
                    .minimumScaleFactor(0.82)

                GameCapabilityBadges(labels: game.capabilityBadges)

                if let subtitle, !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.white.opacity(0.82))
                        .lineLimit(1)
                }
            }
            .padding(16)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .aspectRatio(16.0 / 9.0, contentMode: .fit)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(alignment: .topTrailing) {
            if let badgeSystemImage {
                Image(systemName: badgeSystemImage)
                    .font(.caption.weight(.bold))
                    .foregroundStyle(.white)
                    .frame(width: 28, height: 28)
                    .background(badgeBackgroundColor.opacity(0.92), in: Circle())
                    .padding(8)
            }
        }
        .overlay(
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(Color.white.opacity(0.10), lineWidth: 1)
        )
        .accessibilityElement(children: .combine)
    }

    private var badgeBackgroundColor: Color {
        badgeSystemImage == "heart.fill" ? .red : brandAccent
    }
}

private struct GameLaunchDetailsArtwork: View {
    let game: CloudGame

    var body: some View {
        GeometryReader { proxy in
            let targetPixelSize = imageTargetPixelSize(for: proxy.size)
            ZStack {
                RoundedRectangle(cornerRadius: 12, style: .continuous)
                    .fill(gameColor(for: game.title).opacity(0.18))

                GameArtworkView(game: game, iconSize: 42, role: .catalog)
                    .frame(width: proxy.size.width, height: proxy.size.height)

                if let imageUrl = game.detailsArtworkUrl,
                   let url = URL(
                    string: optimizedNvidiaArtworkURL(
                        imageUrl,
                        targetPixelWidth: imageRequestWidth(for: proxy.size)
                    )
                   ) {
                    CachedRemoteImage(
                        url: url,
                        targetPixelSize: targetPixelSize,
                        priority: .userInitiated
                    ) { image in
                        image
                            .resizable()
                            .scaledToFill()
                            .frame(width: proxy.size.width, height: proxy.size.height)
                    } placeholder: {
                        Color.clear
                    } failure: {
                        Color.clear
                    }
                } else {
                    Color.clear
                }
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
            .clipped()
        }
    }

}

private struct GameScreenshotGallery: View {
    let urls: [String]
    @State private var selectedIndex = 0
    @State private var showingViewer = false

    private var screenshotURLs: [URL] {
        var seen = Set<String>()
        return urls.compactMap { raw in
            let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !trimmed.isEmpty, seen.insert(trimmed).inserted else { return nil }
            return URL(string: optimizedNvidiaArtworkURL(trimmed, targetPixelWidth: 960))
        }
    }

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            LazyHStack(spacing: 10) {
                ForEach(Array(screenshotURLs.enumerated()), id: \.offset) { index, url in
                    Button {
                        selectedIndex = index
                        showingViewer = true
                    } label: {
                        screenshot(url: url, pixelSize: 960)
                            .frame(width: 288)
                            .aspectRatio(16.0 / 9.0, contentMode: .fit)
                            .background(Color.black)
                            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
                            .overlay(
                                RoundedRectangle(cornerRadius: 12, style: .continuous)
                                    .stroke(Color.white.opacity(0.10), lineWidth: 1)
                            )
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Open screenshot \(index + 1) of \(screenshotURLs.count)")
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 4)
        }
        .accessibilityLabel("Game screenshots")
        .fullScreenCover(isPresented: $showingViewer) {
            screenshotViewer
        }
    }

    private func screenshot(url: URL, pixelSize: Int) -> some View {
        CachedRemoteImage(url: url, targetPixelSize: pixelSize, priority: .userInitiated) { image in
            image.resizable().scaledToFit()
        } placeholder: {
            GameScreenshotPlaceholder()
        } failure: {
            GameScreenshotPlaceholder(isFailure: true)
        }
    }

    private var screenshotViewer: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            TabView(selection: $selectedIndex) {
                ForEach(Array(screenshotURLs.enumerated()), id: \.offset) { index, url in
                    GeometryReader { proxy in
                        screenshot(url: url, pixelSize: 1600)
                            .frame(width: proxy.size.width, height: proxy.size.height)
                    }
                    .tag(index)
                }
            }
            .tabViewStyle(.page(indexDisplayMode: .never))
        }
        .overlay(alignment: .top) {
            HStack {
                Button("Close", systemImage: "xmark") { showingViewer = false }
                    .labelStyle(.iconOnly)
                Spacer()
                Text("\(selectedIndex + 1) of \(screenshotURLs.count)")
                    .font(.subheadline.weight(.semibold))
            }
            .foregroundStyle(.white)
            .padding(14)
            .background(.black.opacity(0.45))
        }
        .preferredColorScheme(.dark)
    }
}

private struct GameScreenshotPlaceholder: View {
    var isFailure = false

    var body: some View {
        ZStack {
            Color.secondary.opacity(isFailure ? 0.10 : 0.16)
            Image(systemName: isFailure ? "photo.badge.exclamationmark" : "photo")
                .font(.title2.weight(.semibold))
                .foregroundStyle(.secondary)
        }
    }
}

struct GameListRowView: View {
    let game: CloudGame
    var subtitle: String?
    var trailingSystemImage: String?

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            GameArtworkView(game: game, iconSize: 30)
                .frame(maxWidth: .infinity)
                .frame(height: 96)
                .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))

            LinearGradient(
                colors: [.clear, .black.opacity(0.36), .black.opacity(0.86)],
                startPoint: .top,
                endPoint: .bottom
            )
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))

            VStack(alignment: .leading, spacing: 3) {
                Text(game.title)
                    .font(.headline.weight(.semibold))
                    .foregroundStyle(.white)
                    .lineLimit(2)
                    .minimumScaleFactor(0.86)
                if let subtitle, !subtitle.isEmpty {
                    Text(subtitle)
                        .font(.caption.weight(.medium))
                        .foregroundStyle(Color.white.opacity(0.82))
                        .lineLimit(1)
                }
            }
            .padding(12)
            .padding(.trailing, trailingSystemImage == nil ? 0 : 42)

            if let trailingSystemImage {
                VStack {
                    HStack {
                        Spacer()
                        Image(systemName: trailingSystemImage)
                            .font(.body.weight(.semibold))
                            .foregroundStyle(trailingSystemImage == "heart.fill" ? Color.red : Color.white)
                            .artworkControlChip(diameter: 30)
                    }
                    Spacer()
                }
                .padding(10)
            }
        }
        .frame(minHeight: 96)
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }
}

private struct JumpBackInCard: View {
    let title: String
    let subtitle: String
    let game: CloudGame?
    let statusTint: Color
    let onTap: () -> Void

    var body: some View {
        Button(action: {
            Haptics.light()
            onTap()
        }) {
            VStack(alignment: .leading, spacing: 0) {
                Group {
                    if let game {
                        GameArtworkView(game: game, iconSize: 48)
                    } else {
                        ZStack {
                            Color.secondary.opacity(0.18)
                            Image(systemName: "arrow.counterclockwise.circle.fill")
                                .font(.system(size: 40))
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .frame(width: 160, height: 100)
                .clipShape(
                    UnevenRoundedRectangle(
                        topLeadingRadius: 14,
                        bottomLeadingRadius: 0,
                        bottomTrailingRadius: 0,
                        topTrailingRadius: 14
                    )
                )

                VStack(alignment: .leading, spacing: 6) {
                    Text(title)
                        .font(.caption.bold())
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .frame(height: 32, alignment: .top)
                        .foregroundStyle(.primary)
                    Text(subtitle)
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(Color.white)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(statusTint.opacity(0.92), in: Capsule())
                }
                .padding(10)
            }
            .frame(width: 160)
            .glassCard()
            .contentShape(RoundedRectangle(cornerRadius: 16))
        }
        .buttonStyle(.plain)
    }
}

/// A horizontal row of poster cards under a titled header.
///
/// Used for curated sections the catalog itself defines, so a rail looks and behaves the same
/// wherever it appears — which is the point of having rails at all.
private struct CatalogPosterRail: View {
    @EnvironmentObject private var store: OpenNOWStore

    let title: String
    var symbol: String? = nil
    var caption: String? = nil
    let games: [CloudGame]
    let onOpenDetails: (CloudGame) -> Void
    let onPlay: (CloudGame) -> Void
    var onChooseLauncher: ((CloudGame) -> Void)? = nil

    private var cardWidth: CGFloat {
        let baseWidth: CGFloat = store.settings.compactGameCards ? 140 : 160
        let scale = CGFloat(min(max(store.settings.posterSizeScale, 0.75), 1.4))
        return baseWidth * scale
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(alignment: .firstTextBaseline) {
                Group {
                    if let symbol {
                        Label(title, systemImage: symbol)
                    } else {
                        Text(title)
                    }
                }
                .font(.title2.weight(.bold))

                Spacer(minLength: 8)

                if let caption {
                    Text(caption)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }

            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(alignment: .top, spacing: 10) {
                    ForEach(games) { game in
                        let favorite = store.isFavorite(game)
                        let canLaunch = OpenNOWPlatform.supportsEmbeddedStreamer
                            && !store.launchOptions(for: game).isEmpty
                        GameCatalogGridCard(
                            game: game,
                            subtitle: store.settings.showGameStoreLabels ? gameCatalogSubtitle(for: game) : nil,
                            badgeSystemImage: nil,
                            compact: store.settings.compactGameCards,
                            favorite: favorite,
                            canLaunch: canLaunch,
                            showsTitle: store.settings.showCardTitles,
                            alwaysShowsFavorite: store.settings.showFavoriteIconOnGameCards,
                            onToggleFavorite: { store.toggleFavorite(game) },
                            onOpenDetails: { onOpenDetails(game) },
                            onPlay: { onPlay(game) },
                            onChooseLauncher: onChooseLauncher.map { choose in { choose(game) } }
                        )
                        .frame(width: cardWidth)
                    }
                }
                .padding(.horizontal, 2)
                .padding(.vertical, 4)
            }
            .accessibilityElement(children: .contain)
            .accessibilityLabel("\(title) games")
        }
    }
}

private struct ComingNextCarousel: View {
    private let advanceInterval: TimeInterval = 6
    @Environment(\.scenePhase) private var scenePhase
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var selectedPage = 0
    @State private var pageProgress: CGFloat = 0
    @State private var focusedGameID: String?
    @FocusState private var focusedPageIndicator: Int?
    @State private var legacyFocusedPageIndicator: Int?
    @State private var voiceOverRunning = false

    let games: [CloudGame]
    let isPaused: Bool
    let onOpenDetails: (CloudGame) -> Void

    private var gameIDs: [String] {
        games.map(\.id)
    }

    private var shouldAutoAdvance: Bool {
        games.count > 1 &&
            !isPaused &&
            focusedGameID == nil &&
            focusedPageIndicator == nil &&
            legacyFocusedPageIndicator == nil &&
            scenePhase == .active &&
            !reduceMotion &&
            !voiceOverRunning
    }

    private var autoAdvanceID: String {
        [
            gameIDs.joined(separator: "|"),
            String(selectedPage),
            scenePhase == .active ? "active" : "inactive",
            focusedGameID ?? "unfocused",
            focusedPageIndicator.map(String.init) ?? "no-indicator-focus",
            legacyFocusedPageIndicator.map(String.init) ?? "no-legacy-indicator-focus",
            reduceMotion ? "reduce" : "motion",
            voiceOverRunning ? "voiceover" : "standard",
            isPaused ? "paused" : "visible"
        ].joined(separator: "#")
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Label("New games added", systemImage: "sparkles")
                .font(.title2.weight(.bold))

            TabView(selection: $selectedPage) {
                ForEach(Array(games.enumerated()), id: \.element.id) { index, game in
                    ComingNextHeroCard(
                        game: game,
                        onOpenDetails: { onOpenDetails(game) },
                        onFocusChange: { focused in
                            if focused {
                                focusedGameID = game.id
                            } else if focusedGameID == game.id {
                                focusedGameID = nil
                            }
                        }
                    )
                    .padding(.horizontal, 2)
                    .padding(.vertical, 4)
                    .tag(index)
                }
            }
            .frame(height: 218)
            .tabViewStyle(.page(indexDisplayMode: .never))
            .accessibilityLabel("New games added")
            .overlay(alignment: .bottomTrailing) {
                progressIndicator
                    .padding(.trailing, 16)
                    .padding(.bottom, 14)
            }
        }
        .onAppear {
            voiceOverRunning = UIAccessibility.isVoiceOverRunning
            normalizeSelectedPage()
        }
        .onChangeCompat(of: gameIDs) { _ in
            normalizeSelectedPage()
        }
        .onReceive(
            NotificationCenter.default.publisher(
                for: UIAccessibility.voiceOverStatusDidChangeNotification
            )
        ) { _ in
            voiceOverRunning = UIAccessibility.isVoiceOverRunning
        }
        .task(id: autoAdvanceID) {
            #if DEBUG
            if ProcessInfo.processInfo.arguments.contains("--opennow-hero-half-progress-preview") {
                pageProgress = 0.5
                return
            }
            #endif
            // Each new page or pause state starts a fresh, visible six-second countdown.
            withTransaction(Transaction(animation: nil)) {
                pageProgress = shouldAutoAdvance ? 0 : 1
            }
            guard shouldAutoAdvance else { return }
            await Task.yield()
            guard !Task.isCancelled, shouldAutoAdvance else { return }
            withAnimation(.linear(duration: advanceInterval)) {
                pageProgress = 1
            }
            do {
                try await Task.sleep(for: .seconds(advanceInterval))
            } catch {
                return
            }
            guard !Task.isCancelled, shouldAutoAdvance, !games.isEmpty else { return }
            withAnimation(.easeInOut(duration: 0.32)) {
                selectedPage = (selectedPage + 1) % games.count
            }
        }
    }

    private var progressIndicator: some View {
        HStack(spacing: 4) {
            ForEach(games.indices, id: \.self) { index in
                Button {
                    selectPage(index)
                } label: {
                    ZStack(alignment: .leading) {
                        Capsule()
                            .fill(Color.white.opacity(0.35))
                        if index == selectedPage {
                            HeroPageProgressFill(progress: pageProgress)
                                .fill(Color.white)
                        }
                    }
                    .frame(width: index == selectedPage ? 28 : 6, height: 6)
                    .animation(reduceMotion ? nil : .spring(response: 0.3, dampingFraction: 0.85), value: selectedPage)
                    .frame(height: 28)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .controllerFocusableCompat(
                    fallbackActivation: { selectPage(index) },
                    onLegacyFocusChange: { focused in
                        if focused {
                            legacyFocusedPageIndicator = index
                        } else if legacyFocusedPageIndicator == index {
                            legacyFocusedPageIndicator = nil
                        }
                    }
                )
                .focused($focusedPageIndicator, equals: index)
                .accessibilityLabel("Show \(games[index].title)")
                .accessibilityAddTraits(index == selectedPage ? .isSelected : [])
            }
        }
        .padding(.horizontal, 5)
        .padding(.vertical, 2)
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Featured games, page \(selectedPage + 1) of \(games.count)")
    }

    private func normalizeSelectedPage() {
        guard !games.isEmpty else {
            selectedPage = 0
            return
        }
        selectedPage = min(max(selectedPage, 0), games.count - 1)
    }

    private func selectPage(_ index: Int) {
        withAnimation(.easeInOut(duration: 0.28)) {
            selectedPage = index
        }
    }
}

private struct ComingNextHeroCard: View {
    @Environment(\.gameDetailsTransition) private var detailsTransition
    @State private var detailsSourceID = UUID()
    @FocusState private var isFocused: Bool
    @State private var isLegacyFocused = false

    let game: CloudGame
    let onOpenDetails: () -> Void
    let onFocusChange: (Bool) -> Void

    private var isVisuallyFocused: Bool {
        isFocused || isLegacyFocused
    }

    private func openDetails() {
        Haptics.light()
        detailsTransition?.selectSource(GameDetailsTransitionOrigin(
            sourceID: detailsSourceID, gameKey: catalogStableGameKey(game)
        ))
        onOpenDetails()
    }

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: 18, style: .continuous)
        Button(action: openDetails) {
            ZStack(alignment: .bottomLeading) {
                GameArtworkView(game: game, iconSize: 54, role: .details)

                LinearGradient(
                    colors: [.clear, .black.opacity(0.30), .black.opacity(0.92)],
                    startPoint: .top,
                    endPoint: .bottom
                )

                VStack(alignment: .leading, spacing: 5) {
                    Text(game.title)
                        .font(.title3.bold())
                        .foregroundStyle(.white)
                        .lineLimit(2)
                    if let publisher = game.publisher?.trimmingCharacters(in: .whitespacesAndNewlines),
                       !publisher.isEmpty {
                        Text(publisher)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(Color.white.opacity(0.78))
                            .lineLimit(1)
                    }
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .contentShape(shape)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .gameDetailsArtworkSource(id: detailsSourceID, cornerRadius: 18)
        }
        .buttonStyle(.plain)
        .controllerFocusableCompat(
            fallbackActivation: openDetails,
            onLegacyFocusChange: { isLegacyFocused = $0 }
        )
        .focused($isFocused)
        .accessibilityLabel("Open details for \(game.title)")
        .frame(maxWidth: .infinity)
        .frame(height: 210)
        .clipShape(shape)
        .overlay(
            shape.stroke(isVisuallyFocused ? brandAccent : Color.white.opacity(0.12), lineWidth: isVisuallyFocused ? 2 : 1)
        )
        .scaleEffect(isVisuallyFocused ? 1.012 : 1)
        .animation(.easeOut(duration: 0.16), value: isVisuallyFocused)
        .onChangeCompat(of: isVisuallyFocused) { focused in
            onFocusChange(focused)
        }
        .onDisappear {
            onFocusChange(false)
        }
        #if DEBUG
        .gameDetailsVisualQA(game: game, source: "hero", activate: openDetails)
        #endif
    }
}

struct FeaturedGameCard: View {
    @EnvironmentObject private var store: OpenNOWStore
    @Environment(\.gameDetailsTransition) private var detailsTransition
    @State private var detailsSourceID = UUID()
    @FocusState private var isFocused: Bool
    @State private var isLegacyFocused = false
    let game: CloudGame
    let onOpenDetails: () -> Void

    private var cardWidth: CGFloat {
        let baseWidth: CGFloat = store.settings.compactGameCards ? 140 : 160
        let scale = CGFloat(min(max(store.settings.posterSizeScale, 0.75), 1.4))
        return baseWidth * scale
    }

    private func openDetails() {
        Haptics.light()
        detailsTransition?.selectSource(GameDetailsTransitionOrigin(
            sourceID: detailsSourceID, gameKey: catalogStableGameKey(game)
        ))
        onOpenDetails()
    }

    private var isVisuallyFocused: Bool {
        isFocused || isLegacyFocused
    }

    var body: some View {
        Button(action: openDetails) {
            GameVerticalBannerCard(
                game: game,
                subtitle: store.settings.showGameStoreLabels ? gameCatalogSubtitle(for: game) : nil,
                badgeSystemImage: nil,
                isFocused: isVisuallyFocused
            )
            .frame(width: cardWidth)
            .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            .gameDetailsArtworkSource(id: detailsSourceID)
        }
        .buttonStyle(.plain)
        .controllerFocusableCompat(
            fallbackActivation: openDetails,
            onLegacyFocusChange: { isLegacyFocused = $0 }
        )
        .focused($isFocused)
        .scaleEffect(isVisuallyFocused ? 1.025 : 1)
        .animation(.easeOut(duration: 0.16), value: isVisuallyFocused)
        .zIndex(isVisuallyFocused ? 2 : 0)
    }
}

private struct FeaturedGameCardSkeleton: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            RoundedRectangle(cornerRadius: 14)
                .fill(.quaternary.opacity(0.4))
                .frame(width: 160, height: 100)
                .shimmeringSkeleton()
            VStack(alignment: .leading, spacing: 4) {
                RoundedRectangle(cornerRadius: 5)
                    .fill(.quaternary.opacity(0.4))
                    .frame(height: 32)
                RoundedRectangle(cornerRadius: 4)
                    .fill(.quaternary.opacity(0.3))
                    .frame(width: 70, height: 14)
            }
            .padding(10)
        }
        .frame(width: 160)
        .glassCard()
    }
}

/// An error the user can do something about.
///
/// The action is the point. "Session launch failed" on its own leaves someone tapping Play again
/// and getting the same result; "All rigs in this region are busy" next to a Change Server button
/// is a screen they can leave.
struct ErrorBannerView: View {
    let message: String
    var failure: OpenNOWFailure?
    var onRecover: ((OpenNOWFailure.Recovery) -> Void)?
    var onDismiss: (() -> Void)?

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: symbol)
                .foregroundStyle(OpenNOWPalette.statusFair)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 3) {
                Text(message)
                    .font(.footnote)
                    .fixedSize(horizontal: false, vertical: true)
                if let code = failure?.code {
                    // Selectable so it can be pasted into a report, small so it never competes
                    // with the sentence above it.
                    Text(code)
                        .font(.caption2.monospaced())
                        .foregroundStyle(.tertiary)
                        .textSelection(.enabled)
                }
            }

            Spacer(minLength: 8)

            if let failure, let label = failure.recovery.label, let onRecover {
                Button(label) { onRecover(failure.recovery) }
                    .font(.footnote.weight(.semibold))
                    .buttonStyle(.borderless)
            } else if let onDismiss {
                Button {
                    onDismiss()
                } label: {
                    Image(systemName: "xmark")
                        .font(.caption2.weight(.bold))
                        .frame(width: 28, height: 28)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .foregroundStyle(.secondary)
                .accessibilityLabel("Dismiss")
            }
        }
        .padding(12)
        .background(OpenNOWPalette.statusFair.opacity(0.12), in: RoundedRectangle(cornerRadius: 10))
        .accessibilityElement(children: .contain)
    }

    private var symbol: String {
        switch failure?.kind {
        case .offline: return "wifi.slash"
        case .authExpired: return "person.crop.circle.badge.exclamationmark"
        case .capacity: return "server.rack"
        case .maintenance: return "wrench.and.screwdriver"
        case .notEntitled: return "lock"
        default: return "exclamationmark.triangle.fill"
        }
    }
}

private struct GameCapabilityBadges: View {
    let labels: [String]
    var body: some View {
        if !labels.isEmpty {
            ViewThatFits(in: .horizontal) {
                badgeRow(labels)
                VStack(alignment: .leading, spacing: 4) {
                    badgeRow(Array(labels.prefix(2)))
                    badgeRow(Array(labels.dropFirst(2)))
                }
            }
            .accessibilityElement(children: .ignore)
            .accessibilityLabel(labels.joined(separator: ", "))
        }
    }

    private func badgeRow(_ values: [String]) -> some View {
        HStack(spacing: 4) {
            ForEach(values, id: \.self) { label in
                Text(label == "RTX 5080 Ready" ? "5080 Ready" : label)
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(.white)
                    .fixedSize()
                    .padding(.horizontal, 5).padding(.vertical, 3)
                    .background(.black.opacity(0.72), in: Capsule())
            }
        }
    }
}

struct GameCardView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @Environment(\.gameDetailsTransition) private var detailsTransition
    @State private var detailsSourceID = UUID()
    @FocusState private var isFocused: Bool
    @State private var isLegacyFocused = false
    let game: CloudGame
    let onOpenDetails: () -> Void

    private func openDetails() {
        Haptics.light()
        detailsTransition?.selectSource(GameDetailsTransitionOrigin(
            sourceID: detailsSourceID, gameKey: catalogStableGameKey(game)
        ))
        onOpenDetails()
    }

    private var isVisuallyFocused: Bool {
        isFocused || isLegacyFocused
    }

    var body: some View {
        Button(action: openDetails) {
            GameVerticalBannerCard(
                game: game,
                subtitle: store.settings.showGameStoreLabels ? gameCatalogSubtitle(for: game) : nil,
                badgeSystemImage: nil,
                isFocused: isVisuallyFocused
            )
            .frame(maxWidth: .infinity, alignment: .topLeading)
            .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
            .gameDetailsArtworkSource(id: detailsSourceID)
        }
        .buttonStyle(.plain)
        .controllerFocusableCompat(
            fallbackActivation: openDetails,
            onLegacyFocusChange: { isLegacyFocused = $0 }
        )
        .focused($isFocused)
        .scaleEffect(isVisuallyFocused ? 1.025 : 1)
        .animation(.easeOut(duration: 0.16), value: isVisuallyFocused)
        .zIndex(isVisuallyFocused ? 2 : 0)
    }
}

struct GameLaunchDetailsSheet: View {
    let game: CloudGame
    let onLaunch: (GameLaunchOption?) -> Void
    @EnvironmentObject private var store: OpenNOWStore
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL
    @State private var selectedOption: GameLaunchOption?
    @State private var selectedOptionChanged = false
    @State private var showingLauncherPicker = false
    @State private var descriptionExpanded = true
    @State private var launchAlertMessage: String?

    private var launcherOptions: [GameLaunchOption] { store.launchOptions(for: game) }
    private var savedDefault: GameLaunchOption? { store.defaultLaunchOption(for: game) }
    private var isInLibrary: Bool {
        !game.ownedStorefronts.isEmpty || store.libraryGames.contains {
            catalogStableGameKey($0) == catalogStableGameKey(game)
        }
    }
    private var launchUnavailableMessage: String? {
        if !OpenNOWPlatform.supportsEmbeddedStreamer { return OpenNOWPlatform.streamingUnavailableReason }
        if launcherOptions.isEmpty { return "This game doesn't expose launch targets yet." }
        return nil
    }

    var body: some View {
        NavigationStack {
            GeometryReader { proxy in
                let landscape = proxy.size.width > 700 && proxy.size.width > proxy.size.height
                Group {
                    if landscape {
                        HStack(alignment: .top, spacing: 22) {
                            artwork
                                .frame(width: min(proxy.size.width * 0.46, 560))
                            ScrollView {
                                detailContent
                                    .padding(.trailing, 20)
                                    .padding(.bottom, 20)
                            }
                        }
                        .padding(.horizontal, 20)
                        .padding(.top, 12)
                    } else {
                        ScrollView {
                            VStack(alignment: .leading, spacing: 18) {
                                artwork
                                detailContent
                            }
                            .padding(.horizontal, 16)
                            .padding(.top, 10)
                            .padding(.bottom, 24)
                            .frame(maxWidth: 720)
                            .frame(maxWidth: .infinity)
                        }
                    }
                }
            }
            .background(Color(uiColor: .systemGroupedBackground))
            .navigationTitle(game.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Close", systemImage: "xmark") { dismiss() }
                        .labelStyle(.iconOnly)
                }
                ToolbarItem(placement: .topBarTrailing) {
                    if let shareURL = GFNGameImportReference.shareURL(
                        game: game, option: selectedOption ?? savedDefault ?? launcherOptions.first
                    ) {
                        ShareLink(item: shareURL) {
                            Image(systemName: "square.and.arrow.up")
                        }
                        .accessibilityLabel("Share \(game.title)")
                    }
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) { actionBar }
            .sheet(isPresented: $showingLauncherPicker) {
                GameLauncherSelectionSheet(game: game) { option in
                    showingLauncherPicker = false
                    completeLaunch(option)
                }
                .environmentObject(store)
            }
            .alert("Launch Unavailable", isPresented: launchAlertPresented) {
                Button("OK", role: .cancel) { launchAlertMessage = nil }
            } message: {
                Text(launchAlertMessage ?? "")
            }
        }
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
        .onAppear { selectedOption = savedDefault ?? launcherOptions.first }
        #if DEBUG
        .task {
            guard ProcessInfo.processInfo.arguments.contains(where: { $0.hasPrefix("--opennow-zoom-qa-") }) else { return }
            guard !ProcessInfo.processInfo.arguments.contains("--opennow-zoom-qa-hold") else { return }
            try? await Task.sleep(for: .seconds(2))
            guard !Task.isCancelled else { return }
            dismiss()
        }
        #endif
    }

    private var artwork: some View {
        Button(action: primaryPlay) {
            GameLaunchDetailsArtworkCard(
                game: game,
                subtitle: game.publisher,
                badgeSystemImage: nil
            )
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Play \(game.title)")
    }

    private var detailContent: some View {
        VStack(alignment: .leading, spacing: 16) {
            ownershipCard

            if let restriction = store.launchRestrictionMessage(for: game) {
                Label(restriction, systemImage: "lock.fill")
                    .font(.subheadline.weight(.medium))
                    .foregroundStyle(.orange)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(14)
                    .background(.orange.opacity(0.10), in: RoundedRectangle(cornerRadius: 14))
            }

            if let launchUnavailableMessage {
                Label(launchUnavailableMessage, systemImage: "info.circle")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            let genres = gameMetadataDisplayLabels([game.genre] + (game.tags ?? []))
            if !genres.isEmpty {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 7) {
                        ForEach(Array(genres.prefix(6)), id: \.self) { genre in
                            Text(genre)
                                .font(.caption.weight(.medium))
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                                .background(.thinMaterial, in: Capsule())
                        }
                    }
                }
            }

            if let lastPlayed = game.lastPlayedDate {
                Label("Last played \(displayLastPlayed(lastPlayed))", systemImage: "clock.arrow.circlepath")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }

            if let screenshots = game.screenshotUrls, !screenshots.isEmpty {
                detailCard {
                    Text("Screenshots").font(.headline)
                    GameScreenshotGallery(urls: screenshots)
                        .padding(.horizontal, -16)
                }
            }

            detailCard {
                DisclosureGroup(isExpanded: $descriptionExpanded) {
                    Text(summaryText ?? "No description available.")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.top, 10)
                } label: {
                    Text("Description").font(.headline)
                }
            }

            detailCard {
                Text("Game details").font(.headline)
                if let publisher = game.publisher { detailRow("Publisher", publisher) }
                if let developer = game.developer { detailRow("Developer", developer) }
                if let releaseDate = game.releaseDate { detailRow("Release", releaseDate) }
                if let platform = displayMetadataLabel(game.platform) { detailRow("Platform", platform) }
                if let playType = displayMetadataLabel(game.playType) { detailRow("Play type", playType) }
                detailRow("Age rating", GFNContentRatingParser.ageBadge(from: game.contentRatings) ?? "Not rated")
                if let tier = displayMetadataLabel(game.membershipTierLabel) { detailRow("Membership", tier) }
                let controls = Array(Set(launcherOptions.flatMap { $0.supportedControls ?? [] })).sorted()
                if !controls.isEmpty {
                    detailRow("Controls", gameMetadataDisplayLabels(controls).joined(separator: ", "))
                }
                let features = gameMetadataDisplayLabels(game.featureLabels ?? [])
                if !features.isEmpty {
                    detailRow("Features", Array(features.prefix(8)).joined(separator: ", "))
                }
                if !game.capabilityBadges.isEmpty {
                    GameCapabilityBadges(labels: game.capabilityBadges)
                }
                if let appID = game.launchAppId ?? game.uuid {
                    Button {
                        UIPasteboard.general.string = appID
                        Haptics.light()
                    } label: {
                        Label("Copy App ID", systemImage: "doc.on.doc")
                    }
                    .font(.footnote)
                }
            }

            let externalStores = launcherOptions.compactMap { option -> (String, URL)? in
                guard let url = option.externalStoreURL else { return nil }
                return (storeDisplayName(option.storefront), url)
            }.reduce(into: [(String, URL)]()) { stores, entry in
                if !stores.contains(where: { $0.0 == entry.0 && $0.1 == entry.1 }) {
                    stores.append(entry)
                }
            }
            if !externalStores.isEmpty {
                detailCard {
                    Text("Stores").font(.headline)
                    ForEach(Array(externalStores.enumerated()), id: \.offset) { entry in
                        Link(destination: entry.element.1) {
                            HStack {
                                Text(entry.element.0)
                                Spacer()
                                Image(systemName: "arrow.up.right.square")
                            }
                        }
                        .font(.subheadline.weight(.medium))
                    }
                }
            }

            if !launcherOptions.isEmpty {
                detailCard {
                    HStack {
                        Text("Launchers").font(.headline)
                        Spacer()
                        if launcherOptions.count > 1 {
                            Button("Choose") { showingLauncherPicker = true }
                                .font(.subheadline.weight(.semibold))
                        }
                    }
                    ForEach(launcherOptions) { option in
                        Button {
                            selectedOption = option
                            selectedOptionChanged = true
                        } label: {
                            HStack(spacing: 12) {
                                StoreGlyph(store: option.storefront)
                                    .frame(width: 25, height: 25)
                                    .frame(width: 38, height: 38)
                                    .background(launcherBadgeColor(for: option.storefront).opacity(0.18),
                                        in: RoundedRectangle(cornerRadius: 10))
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(storeDisplayName(option.storefront))
                                        .font(.subheadline.weight(.semibold))
                                    Text(option.id == savedDefault?.id ? "Default launcher" :
                                        option.isOwned ? "In your library" : "Available launcher")
                                        .font(.caption)
                                        .foregroundStyle(.secondary)
                                }
                                Spacer()
                                if selectedOption?.id == option.id {
                                    Image(systemName: "checkmark.circle.fill")
                                        .foregroundStyle(.tint)
                                }
                            }
                            .padding(10)
                            .background(option.id == selectedOption?.id ? brandAccent.opacity(0.10) : Color.secondary.opacity(0.06),
                                in: RoundedRectangle(cornerRadius: 12))
                        }
                        .buttonStyle(.plain)
                    }
                    if let selectedOption {
                        Button(selectedOption.id == savedDefault?.id ? "Clear default launcher" : "Set as default launcher") {
                            store.setDefaultGameVariant(game: game,
                                option: selectedOption.id == savedDefault?.id ? nil : selectedOption)
                        }
                        .font(.subheadline)
                    }
                }
            }

            #if os(iOS)
            if GFNGameImportReference.homeScreenSetupURL(
                game: game, option: selectedOption ?? savedDefault ?? launcherOptions.first
            ) != nil {
                Button {
                    openHomeScreenSetup()
                } label: {
                    Label("Add to Home Screen", systemImage: "plus.app")
                }
                .font(.subheadline)
                .padding(.horizontal, 4)
            }
            #endif
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var ownershipCard: some View {
        detailCard {
            if launcherOptions.contains(where: { $0.libraryStatus != nil }) {
                ForEach(launcherOptions.filter { $0.libraryStatus != nil }) { option in
                    Label(
                        "\(option.isOwned ? "Owned" : "Not owned") on \(storeDisplayName(option.storefront))",
                        systemImage: option.isOwned ? "checkmark.circle.fill" : "circle.slash"
                    )
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(option.isOwned ? .green : .secondary)
                }
            } else {
                Label(isInLibrary ? "In your library" : "Available on GeForce NOW",
                      systemImage: isInLibrary ? "checkmark.circle.fill" : "cloud")
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(isInLibrary ? .green : .secondary)
            }
        }
    }

    private var actionBar: some View {
        HStack(spacing: 10) {
            Button { store.toggleFavorite(game) } label: {
                Image(systemName: store.isFavorite(game) ? "heart.fill" : "heart")
                    .font(.title3)
                    .frame(width: 46, height: 46)
            }
            .gameDetailsActionStyle()
            .tint(store.isFavorite(game) ? .red : brandAccent)
            .accessibilityLabel(store.isFavorite(game) ? "Remove from favorites" : "Add to favorites")

            Button(action: primaryPlay) {
                Label(launchUnavailableMessage == nil ? "Play" : "Play unavailable", systemImage: "play.fill")
                    .font(.headline)
                    .frame(maxWidth: .infinity)
                    .frame(height: 46)
            }
            .gameDetailsActionStyle(prominent: true)
            .tint(brandAccent)
            .disabled(launchUnavailableMessage != nil)

            if launcherOptions.count > 1 {
                Button { showingLauncherPicker = true } label: {
                    Image(systemName: "ellipsis")
                        .font(.title3)
                        .frame(width: 46, height: 46)
                }
                .gameDetailsActionStyle()
                .accessibilityLabel("Choose launcher")
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .frame(maxWidth: .infinity)
        .bottomSheetFooterBackground()
    }

    private func detailCard<Content: View>(@ViewBuilder content: () -> Content) -> some View {
        VStack(alignment: .leading, spacing: 12, content: content)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(16)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func detailRow(_ label: String, _ value: String) -> some View {
        HStack(alignment: .firstTextBaseline) {
            Text(label).foregroundStyle(.secondary)
            Spacer(minLength: 12)
            Text(value).multilineTextAlignment(.trailing)
        }
        .font(.subheadline)
    }

    private func primaryPlay() {
        if let restriction = store.launchRestrictionMessage(for: game) {
            launchAlertMessage = restriction
            return
        }
        guard launchUnavailableMessage == nil else { return }
        if launcherOptions.count > 1, savedDefault == nil, !selectedOptionChanged {
            showingLauncherPicker = true
            return
        }
        completeLaunch(selectedOption ?? savedDefault ?? launcherOptions.first)
    }

    private func completeLaunch(_ option: GameLaunchOption?) {
        Haptics.medium()
        onLaunch(option)
        dismiss()
    }

    private func displayLastPlayed(_ raw: String) -> String {
        let parser = ISO8601DateFormatter()
        parser.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let date = parser.date(from: raw) ?? ISO8601DateFormatter().date(from: raw)
        guard let date else { return raw }
        return date.formatted(date: .abbreviated, time: .omitted)
    }

    private var summaryText: String? {
        [game.longDescription, game.summary]
            .compactMap { $0?.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first(where: { !$0.isEmpty })
    }

    private func displayMetadataLabel(_ value: String?) -> String? {
        guard let value else { return nil }
        let label = gameMetadataDisplayLabel(value)
        return label.isEmpty ? nil : label
    }

    private var launchAlertPresented: Binding<Bool> {
        Binding(
            get: { launchAlertMessage != nil },
            set: { if !$0 { launchAlertMessage = nil } }
        )
    }

    #if os(iOS)
    private func openHomeScreenSetup() {
        guard let url = GFNGameImportReference.homeScreenSetupURL(
            game: game, option: selectedOption ?? savedDefault ?? launcherOptions.first
        ) else { return }
        openURL(url)
    }
    #endif
}

struct GameLauncherSelectionSheet: View {
    let game: CloudGame
    let onLaunch: (GameLaunchOption) -> Void

    @EnvironmentObject private var store: OpenNOWStore
    @Environment(\.dismiss) private var dismiss
    @State private var selectedOptionId = ""
    @State private var rememberDefault = false

    private var launcherOptions: [GameLaunchOption] {
        store.launchOptions(for: game)
    }

    private var selectedOption: GameLaunchOption? {
        launcherOptions.first { $0.id == selectedOptionId } ?? launcherOptions.first
    }

    private var defaultOption: GameLaunchOption? {
        store.defaultLaunchOption(for: game)
    }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    HStack(spacing: 12) {
                        GameArtworkView(game: game, iconSize: 26, fit: true)
                            .frame(width: 58, height: 76)
                            .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))

                        VStack(alignment: .leading, spacing: 4) {
                            Text(game.title)
                                .font(.headline)
                                .lineLimit(2)
                            Text("Choose a launcher")
                                .font(.subheadline)
                                .foregroundStyle(.secondary)
                        }
                    }
                    .padding(.vertical, 2)
                }

                Section("Launcher") {
                    ForEach(launcherOptions) { option in
                        Button {
                            selectedOptionId = option.id
                        } label: {
                            LauncherOptionRow(
                                option: option,
                                selected: option.id == selectedOption?.id,
                                savedDefault: option.id == defaultOption?.id
                            )
                        }
                        .buttonStyle(.plain)
                    }
                }

                if !launcherOptions.isEmpty {
                    Section {
                        Toggle("Remember for this game", isOn: $rememberDefault)
                    } footer: {
                        Text("When remembered, the play button launches this game with the selected launcher. Use the launcher badge to change it later.")
                    }
                }
            }
            .navigationTitle("Launch")
            .navigationBarTitleDisplayMode(.inline)
            .scrollContentBackground(.hidden)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Cancel") {
                        dismiss()
                    }
                }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                VStack(spacing: 0) {
                    Button {
                        guard let selectedOption else { return }
                        if rememberDefault || defaultOption != nil {
                            store.setDefaultGameVariant(game: game, option: rememberDefault ? selectedOption : nil)
                        }
                        Haptics.medium()
                        onLaunch(selectedOption)
                        dismiss()
                    } label: {
                        Text("Continue")
                            .font(.headline)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 12)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(brandAccent)
                    .disabled(selectedOption == nil)
                    .padding(.horizontal, 20)
                    .padding(.top, 12)
                    .padding(.bottom, 8)
                }
                .frame(maxWidth: .infinity)
                .bottomSheetFooterBackground()
            }
        }
        .onAppear {
            let defaultOption = defaultOption
            selectedOptionId = defaultOption?.id ?? launcherOptions.first?.id ?? ""
            rememberDefault = defaultOption != nil
        }
    }
}

private struct LauncherOptionRow: View {
    let option: GameLaunchOption
    let selected: Bool
    let savedDefault: Bool

    var body: some View {
        HStack(spacing: 12) {
            StoreGlyph(store: option.storefront)
                .frame(width: 24, height: 24)
                .frame(width: 42, height: 42)
                .background(launcherBadgeColor(for: option.storefront).opacity(0.94), in: RoundedRectangle(cornerRadius: 12, style: .continuous))

            VStack(alignment: .leading, spacing: 3) {
                Text(storeDisplayName(option.storefront))
                    .font(.headline)
                    .foregroundStyle(.primary)
                    .lineLimit(1)
                if !detailText.isEmpty {
                    Text(detailText)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
            }

            Spacer(minLength: 8)

            if selected {
                Image(systemName: "checkmark.circle.fill")
                    .font(.title3)
                    .foregroundStyle(brandAccent)
            }
        }
        .padding(.vertical, 4)
        .contentShape(Rectangle())
    }

    private var detailText: String {
        var parts: [String] = []
        if savedDefault {
            parts.append("Default")
        }
        if let controls = option.supportedControls {
            let labels = gameMetadataDisplayLabels(controls).prefix(3)
            if !labels.isEmpty {
                parts.append(labels.joined(separator: ", "))
            }
        }
        if parts.isEmpty {
            parts.append("App \(option.appId)")
        }
        return parts.joined(separator: " - ")
    }
}

private struct GameArtworkCard: View {
    @EnvironmentObject private var store: OpenNOWStore
    let game: CloudGame
    let artworkHeight: CGFloat
    let titleFont: Font
    let subtitleFont: Font
    let storeBadgeLimit: Int

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            GameArtworkView(game: game, iconSize: 36)
                .frame(maxWidth: .infinity)
                .frame(height: artworkHeight)
                .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))

            LinearGradient(
                colors: [.clear, .black.opacity(0.36), .black.opacity(0.88)],
                startPoint: .top,
                endPoint: .bottom
            )
            .frame(height: min(artworkHeight * 0.74, 170))
            .frame(maxHeight: .infinity, alignment: .bottom)
            .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))

            VStack(alignment: .leading, spacing: 8) {
                Text(game.title)
                    .font(titleFont)
                    .foregroundStyle(Color.white)
                    .lineLimit(3)
                    .minimumScaleFactor(0.86)
                    .fixedSize(horizontal: false, vertical: true)

                Text("\(game.genre) · \(game.platform)")
                    .font(subtitleFont)
                    .foregroundStyle(Color.white.opacity(0.82))
                    .lineLimit(1)

                if !displayStores.isEmpty {
                    HStack(spacing: 8) {
                        ForEach(displayStores, id: \.self) { store in
                            StorePill(store: store, prominent: false)
                        }
                    }
                }
            }
            .padding(14)
            .padding(.top, 26)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background {
                Rectangle()
                    .fill(.ultraThinMaterial.opacity(0.86))
                    .mask(
                        LinearGradient(
                            colors: [.clear, Color.white.opacity(0.35), Color.white],
                            startPoint: .top,
                            endPoint: .bottom
                        )
                    )
                    .allowsHitTesting(false)
            }
            .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
        .overlay(alignment: .topTrailing) {
            if store.isFavorite(game) {
                Image(systemName: "heart.fill")
                    .font(.caption.weight(.bold))
                    .foregroundStyle(.white)
                    .padding(8)
                    .background(.red.opacity(0.88), in: Circle())
                    .padding(10)
            }
        }
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(Color.white.opacity(0.08), lineWidth: 1)
        )
        .artworkCardShadow(cornerRadius: 18, opacity: 0.18, radius: 12, offsetY: 8)
    }

    private var displayStores: [String] {
        guard store.settings.showGameStoreLabels else { return [] }
        return Array(gameResolvedStores(game: game).prefix(storeBadgeLimit))
    }
}

private struct GameMetaCard: View {
    let label: String
    let value: String
    let icon: String

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(label, systemImage: icon)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.primary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, minHeight: 88, alignment: .topLeading)
        .padding(14)
        .glassCard()
    }
}

private struct StorePill: View {
    let store: String
    let prominent: Bool

    var body: some View {
        HStack(spacing: 8) {
            StoreGlyph(store: store)
                .frame(width: prominent ? 28 : 22, height: prominent ? 28 : 22)
            if prominent {
                Text(storeDisplayName(store))
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
            }
        }
        .foregroundColor(prominent ? .primary : .white)
        .padding(.horizontal, prominent ? 12 : 6)
        .padding(.vertical, prominent ? 10 : 6)
        .background(backgroundShape)
    }

    @ViewBuilder
    private var backgroundShape: some View {
        if prominent {
            Capsule()
                .fill(.regularMaterial)
                .overlay(Capsule().stroke(Color.white.opacity(0.12), lineWidth: 1))
        } else {
            Capsule()
                .fill(Color.white.opacity(0.12))
                .overlay(Capsule().stroke(Color.white.opacity(0.14), lineWidth: 1))
        }
    }
}

struct StoreGlyph: View {
    let store: String

    var body: some View {
        ZStack {
            if showsGlyphBackground {
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(glyphBackground)
            }
            if let assetName {
                Image(assetName)
                    .resizable()
                    .renderingMode(.original)
                    .scaledToFit()
                    .padding(imagePadding)
            } else {
                Image(systemName: "bag.fill")
                    .font(.system(size: 12, weight: .bold))
                    .foregroundStyle(Color.white)
            }
        }
    }

    private var normalizedStore: String {
        storeNormalizedKey(store)
    }

    private var glyphBackground: some ShapeStyle {
        switch normalizedStore {
        case "STEAM":
            return AnyShapeStyle(
                LinearGradient(
                    colors: [Color(red: 0.08, green: 0.16, blue: 0.24), Color(red: 0.17, green: 0.42, blue: 0.70)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
        case "EPIC", "EGS", "EPIC_GAMES_STORE":
            return AnyShapeStyle(Color.black)
        case "XBOX", "XBOX_GAME_PASS", "GAME_PASS":
            return AnyShapeStyle(
                LinearGradient(
                    colors: [Color(red: 0.31, green: 0.66, blue: 0.17), Color(red: 0.15, green: 0.48, blue: 0.12)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
        default:
            return AnyShapeStyle(Color.gray.opacity(0.8))
        }
    }

    private var showsGlyphBackground: Bool {
        normalizedStore != "STEAM"
    }

    private var assetName: String? {
        switch normalizedStore {
        case "STEAM":
            return "StoreSteam"
        case "EPIC", "EGS", "EPIC_GAMES_STORE":
            return "StoreEpic"
        case "XBOX", "XBOX_GAME_PASS", "GAME_PASS":
            return "StoreXbox"
        default:
            return nil
        }
    }

    private var imagePadding: CGFloat {
        switch normalizedStore {
        case "STEAM":
            return 0
        case "EPIC", "EGS", "EPIC_GAMES_STORE":
            return 3
        case "XBOX", "XBOX_GAME_PASS", "GAME_PASS":
            return 4
        default:
            return 2
        }
    }
}

struct GameCardSkeletonView: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            RoundedRectangle(cornerRadius: 12)
                .fill(.quaternary.opacity(0.35))
                .aspectRatio(gameVerticalBannerAspectRatio, contentMode: .fit)
                .shimmeringSkeleton()
            RoundedRectangle(cornerRadius: 5)
                .fill(.quaternary.opacity(0.4))
                .frame(height: 12)
            RoundedRectangle(cornerRadius: 4)
                .fill(.quaternary.opacity(0.3))
                .frame(width: 100, height: 10)
            RoundedRectangle(cornerRadius: 7)
                .fill(.quaternary.opacity(0.35))
                .frame(height: 30)
        }
        .padding(10)
        .glassCard()
    }
}

struct GameArtworkView: View {
    enum Role {
        case catalog
        case details
        case queue
    }

    let game: CloudGame
    let iconSize: CGFloat
    var fit = false
    var role: Role = .catalog

    var body: some View {
        GeometryReader { proxy in
            let targetPixelSize = imageTargetPixelSize(for: proxy.size)
            ZStack {
                gameColor(for: game.title).opacity(0.2)
                if let imageUrl = artworkUrl,
                   let url = URL(string: requestArtworkURL(imageUrl, size: proxy.size)) {
                    CachedRemoteImage(url: url, targetPixelSize: targetPixelSize) { image in
                        fittedImage(image, size: proxy.size)
                    } placeholder: {
                        GameArtworkLoadingPlaceholder(game: game, iconSize: iconSize, isFailure: false)
                            .frame(width: proxy.size.width, height: proxy.size.height)
                    } failure: {
                        iconFallback
                            .frame(width: proxy.size.width, height: proxy.size.height)
                    }
                } else {
                    iconFallback
                        .frame(width: proxy.size.width, height: proxy.size.height)
                }
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
            .clipped()
        }
    }

    @ViewBuilder
    private func fittedImage(_ image: Image, size: CGSize) -> some View {
        if fit {
            image
                .resizable()
                .scaledToFit()
                .frame(width: size.width, height: size.height)
        } else {
            image
                .resizable()
                .scaledToFill()
                .frame(width: size.width, height: size.height)
        }
    }

    private var iconFallback: some View {
        GameArtworkLoadingPlaceholder(game: game, iconSize: iconSize, isFailure: true)
    }

    private var artworkUrl: String? {
        switch role {
        case .catalog:
            return game.catalogArtworkUrl
        case .details:
            return game.detailsArtworkUrl
        case .queue:
            return game.queueArtworkUrl
        }
    }

    /// Every role asks the CDN for the width it will draw. The catalog used to be exempt, on the
    /// theory that box art is small — but a page of cards is twenty of them, and each one was the
    /// full-size master. That is the single largest cost in filling a catalog screen.
    private func requestArtworkURL(_ source: String, size: CGSize) -> String {
        optimizedNvidiaArtworkURL(source, targetPixelWidth: imageRequestWidth(for: size))
    }
}

/// What fills a card before artwork arrives, or when it never does.
///
/// Two rules, both taken from how the system's own libraries behave:
///
/// 1. **A tile that is waiting does not perform.** Photos, Music and the App Store all show a plain
///    quiet fill while artwork loads — no sweep, no pulse. The previous version ran a shimmer here,
///    which meant a catalog page mid-load held twenty independent 30 fps timelines, each one
///    compositing offscreen, for decoration standing in for the very images it was delaying. The
///    screen-level skeleton already carries "something is happening" while the catalog itself is
///    empty; once cards exist, the honest signal is stillness.
/// 2. **Missing is not the same as loading.** A permanent absence shows the game's initials over
///    the same ground, with its category glyph beneath. A monogram is specific to the title and
///    recognisable at poster size; a generic photo symbol says nothing about which card you are
///    looking at. Both states share one ground, so a half-loaded grid reads as one surface rather
///    than a patchwork.
///
/// It draws in a single pass with no `GeometryReader` of its own — the caller already knows how big
/// the card is and passes `iconSize`, so measuring it again would add a layout pass to every cell.
struct GameArtworkLoadingPlaceholder: View {
    let game: CloudGame
    let iconSize: CGFloat
    let isFailure: Bool

    var body: some View {
        ground
            .overlay {
                if isFailure {
                    monogram
                }
            }
            .accessibilityHidden(true)
    }

    /// Tinted from the title so a grid of placeholders is not a wall of identical grey, but kept
    /// dark and low-contrast because it sits where box art will be.
    private var ground: some View {
        LinearGradient(
            colors: [
                gameColor(for: game.title).opacity(isFailure ? 0.22 : 0.16),
                OpenNOWPalette.imagePlaceholder
            ],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
    }

    /// Scaled from `iconSize`, which every call site already sizes against its own card, so a
    /// 26-point queue thumbnail and a 54-point detail hero both get proportionate lettering.
    private var monogram: some View {
        VStack(spacing: 3) {
            Text(initials)
                .font(.system(size: max(17, iconSize * 0.62), weight: .semibold, design: .rounded))
                .foregroundStyle(OpenNOWPalette.textOnDark.opacity(0.32))
                .lineLimit(1)
                .minimumScaleFactor(0.5)
            Image(systemName: game.icon)
                .font(.system(size: max(9, iconSize * 0.22), weight: .semibold))
                .foregroundStyle(OpenNOWPalette.textMutedOnDark.opacity(0.55))
        }
        .padding(6)
    }

    /// Up to two initials from the meaningful words in the title. Leading articles and short
    /// connectives are skipped so "The Last of Us" reads LU rather than TL.
    private var initials: String {
        let skipped: Set<String> = ["the", "a", "an", "of", "and", "de", "la", "le"]
        let words = game.title
            .components(separatedBy: CharacterSet.alphanumerics.inverted)
            .filter { !$0.isEmpty && !skipped.contains($0.lowercased()) }
        let letters = words.prefix(2).compactMap { $0.first }.map(String.init)
        if letters.isEmpty {
            return String(game.title.prefix(2)).uppercased()
        }
        return letters.joined().uppercased()
    }
}

func imageTargetPixelSize(for size: CGSize) -> Int {
    let points = max(size.width, size.height)
    let pixels = points * UIScreen.main.scale
    guard pixels.isFinite, pixels > 0 else { return 480 }
    return normalizedImageTargetPixelSize(Int(ceil(pixels)))
}

func normalizedImageTargetPixelSize(_ targetPixelSize: Int) -> Int {
    max(160, ((targetPixelSize + 159) / 160) * 160)
}

/// The width to request artwork at, in pixels, for a view of `size`.
///
/// Deliberately the view's **width** rather than its longest edge: box art is portrait, so sizing a
/// request by the longest edge asks the CDN for a third more pixels than the card can show. Rounded
/// up to the same 160-pixel step as the decode target so a poster-size slider does not invalidate
/// every cached image for a few points of travel.
func imageRequestWidth(for size: CGSize) -> Int {
    let pixels = size.width * UIScreen.main.scale
    guard pixels.isFinite, pixels > 0 else { return 480 }
    return normalizedImageTargetPixelSize(Int(ceil(pixels)))
}

/// The pixel width of the master NVIDIA stores for each kind of artwork, keyed by the marker its
/// filename carries.
///
/// These are measured, not assumed, and they are the whole reason this table exists: the CDN
/// happily serves `;w=` above the master and **upscales**, so a request sized from the screen alone
/// can cost more bytes than the untouched original. A 3x phone asking for a 640-pixel-wide poster
/// would have pulled an upscaled 628-pixel master; asking for 1920 pulls nearly three times the
/// original. Every request is clamped here instead.
///
/// An unrecognised path is left alone rather than guessed at — we only rewrite what we have
/// measured.
private let nvidiaArtworkMasterWidths: [(marker: String, width: Int)] = [
    ("/GAME_BOX_ART_", 628),
    ("/KEY_ART_", 600),
    ("/HERO_IMAGE_", 1_920),
    ("/TV_BANNER_", 1_920),
    ("/SCREENSHOT_", 1_920)
]

/// Rewrites an NVIDIA artwork URL to ask the CDN for a WebP re-encode no wider than we will draw.
///
/// Worth doing even when the width is already the master's: at native size WebP costs roughly a
/// quarter less than the stored JPEG for box art and half for a screenshot, so every artwork
/// request gets smaller, and a catalog card on a tablet gets a great deal smaller than that.
///
/// The sizing parameters are path parameters, not query items, so they have to go on the last path
/// segment — appending them after a query string would make the CDN serve the master again.
func optimizedNvidiaArtworkURL(_ raw: String, targetPixelWidth: Int) -> String {
    guard raw.localizedCaseInsensitiveContains("img.nvidiagrid.net") else { return raw }
    let queryStart = raw.firstIndex(of: "?")
    let path = queryStart.map { String(raw[..<$0]) } ?? raw
    let query = queryStart.map { String(raw[$0...]) } ?? ""
    let markers = [";f=", ";w=", ";h=", ";dpr="]
    let cutoff = markers.compactMap {
        path.range(of: $0, options: .caseInsensitive)?.lowerBound
    }.min()
    let base = cutoff.map { String(path[..<$0]) } ?? path
    guard let master = nvidiaArtworkMasterWidths.first(where: {
        base.range(of: $0.marker, options: .caseInsensitive) != nil
    })?.width else {
        return raw
    }
    let width = min(max(targetPixelWidth, 160), master)
    return "\(base);f=webp;w=\(width)\(query)"
}

func gameColor(for title: String) -> Color {
    let palette: [Color] = [
        Color(red: 0.46, green: 0.72, blue: 0.0),
        Color(red: 0.0, green: 0.72, blue: 0.55),
        Color(red: 0.2, green: 0.5, blue: 1.0),
        Color(red: 0.8, green: 0.3, blue: 0.9),
        Color(red: 1.0, green: 0.6, blue: 0.0),
        Color(red: 0.9, green: 0.2, blue: 0.3),
    ]
    let hash = abs(title.hashValue)
    return palette[hash % palette.count]
}

private final class LegacyControllerFocusButton: UIButton {
    var onActivate: () -> Void = {}
    var onFocusChange: (Bool) -> Void = { _ in }

    private var reportedFocus = false

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        isAccessibilityElement = false
        addTarget(self, action: #selector(activate), for: .primaryActionTriggered)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var canBecomeFocused: Bool {
        isEnabled
    }

    override func didUpdateFocus(
        in context: UIFocusUpdateContext,
        with coordinator: UIFocusAnimationCoordinator
    ) {
        super.didUpdateFocus(in: context, with: coordinator)
        reportFocusIfNeeded(isFocused)
    }

    override func didMoveToWindow() {
        super.didMoveToWindow()
        if window == nil {
            reportFocusIfNeeded(false)
        }
    }

    @objc private func activate() {
        onActivate()
    }

    private func reportFocusIfNeeded(_ focused: Bool) {
        guard reportedFocus != focused else { return }
        reportedFocus = focused
        onFocusChange(focused)
    }
}

private struct LegacyControllerFocusProxy: UIViewRepresentable {
    let isEnabled: Bool
    let onActivate: () -> Void
    let onFocusChange: (Bool) -> Void

    func makeUIView(context: Context) -> LegacyControllerFocusButton {
        let button = LegacyControllerFocusButton(type: .custom)
        configure(button)
        return button
    }

    func updateUIView(_ button: LegacyControllerFocusButton, context: Context) {
        configure(button)
    }

    private func configure(_ button: LegacyControllerFocusButton) {
        button.isEnabled = isEnabled
        button.isUserInteractionEnabled = isEnabled
        button.onActivate = onActivate
        button.onFocusChange = onFocusChange
    }
}

private struct ControllerFocusableCompatModifier: ViewModifier {
    @Environment(\.isEnabled) private var isEnabled

    let fallbackActivation: () -> Void
    let onLegacyFocusChange: (Bool) -> Void

    @ViewBuilder
    func body(content: Content) -> some View {
        if #available(iOS 17.0, *) {
            content.focusable()
        } else {
            content.overlay {
                LegacyControllerFocusProxy(
                    isEnabled: isEnabled,
                    onActivate: fallbackActivation,
                    onFocusChange: onLegacyFocusChange
                )
            }
        }
    }
}

extension View {
    func controllerFocusableCompat(
        fallbackActivation: @escaping () -> Void,
        onLegacyFocusChange: @escaping (Bool) -> Void = { _ in }
    ) -> some View {
        modifier(
            ControllerFocusableCompatModifier(
                fallbackActivation: fallbackActivation,
                onLegacyFocusChange: onLegacyFocusChange
            )
        )
    }

    func glassCard(cornerRadius: CGFloat = 16) -> some View {
        modifier(GlassCardModifier(cornerRadius: cornerRadius))
    }

    @ViewBuilder
    func gameBannerGridListRowStyle() -> some View {
        #if os(iOS)
        self
            .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 6, trailing: 16))
            .listRowSeparator(.hidden)
            .listRowBackground(Color.clear)
        #else
        self
            .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 6, trailing: 16))
            .listRowBackground(Color.clear)
        #endif
    }

    func shimmeringSkeleton() -> some View {
        modifier(SkeletonShimmerModifier())
    }

    func bottomSheetFooterBackground() -> some View {
        background {
            Rectangle()
                .fill(.regularMaterial)
                .ignoresSafeArea(edges: .bottom)
        }
    }

    func presentGameDetailsSheet(
        selectedGame: Binding<CloudGame?>,
        store: OpenNOWStore,
        onLaunch: @escaping (CloudGame, GameLaunchOption?) -> Void
    ) -> some View {
        modifier(GameDetailsPresentationModifier(selectedGame: selectedGame, store: store, onLaunch: onLaunch))
    }

    func launcherSelectionModalSheet(
        selectedGame: Binding<CloudGame?>,
        store: OpenNOWStore,
        onLaunch: @escaping (CloudGame, GameLaunchOption) -> Void
    ) -> some View {
        sheet(item: selectedGame) { game in
            GameLauncherSelectionSheet(game: game) { option in
                selectedGame.wrappedValue = nil
                DispatchQueue.main.async {
                    onLaunch(game, option)
                }
            }
            .environmentObject(store)
        }
    }
}

extension View {
    func opennowBottomSheet<Item: Identifiable, Sheet: View>(
        item: Binding<Item?>,
        heightFraction: CGFloat,
        maxHeight: CGFloat,
        @ViewBuilder content: @escaping (Item) -> Sheet
    ) -> some View {
        fullScreenCover(item: item) { value in
            OpenNOWBottomSheetHost(heightFraction: heightFraction, maxHeight: maxHeight) {
                content(value)
            }
            .presentationBackground(.clear)
        }
    }

}

private struct OpenNOWBottomSheetHost<Content: View>: View {
    @Environment(\.dismiss) private var dismiss
    let heightFraction: CGFloat
    let maxHeight: CGFloat
    let content: Content

    init(heightFraction: CGFloat, maxHeight: CGFloat, @ViewBuilder content: () -> Content) {
        self.heightFraction = heightFraction
        self.maxHeight = maxHeight
        self.content = content()
    }

    var body: some View {
        GeometryReader { proxy in
            ZStack(alignment: .bottom) {
                Color.black.opacity(0.28)
                    .ignoresSafeArea()
                    .onTapGesture { dismiss() }

                content
                    .frame(maxWidth: .infinity)
                    .frame(height: sheetFrameHeight(in: proxy))
                    .modifier(OpenNOWBottomSheetSurfaceModifier(cornerRadius: 28))
                    .shadow(color: .black.opacity(0.22), radius: 18, y: -4)
                    .ignoresSafeArea(edges: .bottom)
            }
        }
        .ignoresSafeArea()
        .background(Color.clear)
    }

    private func sheetFrameHeight(in proxy: GeometryProxy) -> CGFloat {
        min(proxy.size.height, sheetHeight(in: proxy) + proxy.safeAreaInsets.bottom)
    }

    private func sheetHeight(in proxy: GeometryProxy) -> CGFloat {
        min(maxHeight, max(360, proxy.size.height * heightFraction))
    }
}

private struct OpenNOWBottomSheetSurfaceModifier: ViewModifier {
    let cornerRadius: CGFloat

    func body(content: Content) -> some View {
        let shape = OpenNOWBottomSheetShape(cornerRadius: cornerRadius)
        if #available(iOS 26, *) {
            content
                .background(.regularMaterial, in: shape)
                .glassEffect(in: shape)
                .clipShape(shape)
        } else {
            content
                .background(.regularMaterial, in: shape)
                .clipShape(shape)
        }
    }
}

private struct OpenNOWBottomSheetShape: Shape {
    let cornerRadius: CGFloat

    func path(in rect: CGRect) -> Path {
        let radius = min(cornerRadius, rect.width / 2, rect.height / 2)
        var path = Path()
        path.move(to: CGPoint(x: rect.minX, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX, y: rect.minY + radius))
        path.addQuadCurve(
            to: CGPoint(x: rect.minX + radius, y: rect.minY),
            control: CGPoint(x: rect.minX, y: rect.minY)
        )
        path.addLine(to: CGPoint(x: rect.maxX - radius, y: rect.minY))
        path.addQuadCurve(
            to: CGPoint(x: rect.maxX, y: rect.minY + radius),
            control: CGPoint(x: rect.maxX, y: rect.minY)
        )
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.maxY))
        path.closeSubpath()
        return path
    }
}

private func storeNormalizedKey(_ store: String) -> String {
    normalizeGameStore(store)
}

func storeDisplayName(_ store: String) -> String {
    gameStoreDisplayName(store)
}

private func launcherBadgeColor(for store: String) -> Color {
    switch storeNormalizedKey(store) {
    case "STEAM":
        return Color(red: 0.09, green: 0.18, blue: 0.28)
    case "EPIC", "EGS", "EPIC_GAMES_STORE":
        return Color.black
    case "XBOX", "XBOX_GAME_PASS", "GAME_PASS":
        return Color(red: 0.06, green: 0.49, blue: 0.06)
    case "MICROSOFT", "MICROSOFT_STORE":
        return Color(red: 0.0, green: 0.40, blue: 0.72)
    case "UBISOFT", "UBISOFT_CONNECT":
        return Color(red: 0.0, green: 0.43, blue: 0.99)
    case "EA", "EA_APP", "ORIGIN":
        return Color(red: 1.0, green: 0.28, blue: 0.28)
    case "GOG", "GOG_COM":
        return Color(red: 0.42, green: 0.21, blue: 0.66)
    case "BATTLENET", "BATTLE_NET", "BLIZZARD":
        return Color(red: 0.08, green: 0.56, blue: 1.0)
    case "RIOT", "RIOT_CLIENT", "RIOT_GAMES":
        return Color(red: 0.82, green: 0.21, blue: 0.22)
    case "ROCKSTAR", "ROCKSTAR_GAMES", "ROCKSTAR_GAMES_LAUNCHER":
        return Color(red: 1.0, green: 0.77, blue: 0.0)
    case "GOOGLE_PLAY", "PLAY_STORE", "ANDROID":
        return Color(red: 0.06, green: 0.62, blue: 0.35)
    case "AMAZON", "AMAZON_GAMES":
        return Color(red: 1.0, green: 0.60, blue: 0.0)
    default:
        return Color.black.opacity(0.72)
    }
}

func gameResolvedStores(game: CloudGame) -> [String] {
    if let stores = game.stores, !stores.isEmpty {
        return stores
    }
    let derived = Array(Set(game.launchOptions.map(\.storefront))).sorted()
    return derived.isEmpty ? [game.platform] : derived
}

func gameCatalogSubtitle(for game: CloudGame, storeLimit: Int = 3) -> String {
    let stores = gameResolvedStores(game: game)
        .map(storeDisplayName)
        .prefix(storeLimit)
        .joined(separator: ", ")
    if !stores.isEmpty {
        return stores
    }
    return [game.genre, game.platform].filter { !$0.isEmpty }.joined(separator: " · ")
}
