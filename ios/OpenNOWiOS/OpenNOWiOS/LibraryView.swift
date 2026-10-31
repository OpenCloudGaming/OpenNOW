import SwiftUI

struct LibraryView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @Environment(\.dynamicTypeSize) private var dynamicTypeSize
    @Environment(\.horizontalSizeClass) private var sizeClass
    @StateObject private var presentation = LibraryPresentationModel()
    @State private var criteria = LibraryScreenCriteria()
    @State private var layout = LibraryLayout.grid
    @State private var showingFilters = false
    @State private var showingAddGame = false
    @State private var pendingLaunchRequest: GameLaunchRequest?
    @State private var selectedGameForDetails: CloudGame?
    @State private var selectedGameForLauncher: CloudGame?

    private var edge: CGFloat { OpenNOWSpacing.screenEdge(sizeClass) }
    private var effectiveLayout: LibraryLayout { dynamicTypeSize.isAccessibilitySize ? .list : layout }

    var body: some View {
        NavigationStack {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: OpenNOWSpacing.xl) {
                    if store.user == nil {
                        OpenNOWUnavailableView("Sign In to See Your Library", systemImage: "person.crop.circle")
                            .frame(maxWidth: .infinity, minHeight: 320)
                    } else {
                        libraryHeader
                        libraryContent
                    }
                }
                .padding(.horizontal, edge)
                .padding(.top, OpenNOWSpacing.sm)
                .padding(.bottom, OpenNOWSpacing.xl)
                .frame(maxWidth: 1120)
                .frame(maxWidth: .infinity)
            }
            .scrollDismissesKeyboard(.interactively)
            .navigationTitle("Library")
            .navigationBarTitleDisplayMode(.large)
            .searchable(text: $criteria.query, placement: .navigationBarDrawer(displayMode: .automatic), prompt: "Search your games")
            .refreshable { await store.refreshCatalog() }
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button { showingAddGame = true } label: {
                        Label("Add Game", systemImage: "plus")
                    }
                    .disabled(store.user == nil)
                }
            }
            .background {
                CatalogWallpaperBackdrop(
                    isEnabled: store.settings.catalogWallpaperEnabled,
                    managedFilename: store.settings.catalogWallpaperFilename,
                    preset: store.settings.catalogWallpaperPreset
                )
            }
            .sheet(isPresented: $showingAddGame) { AddLibraryGameSheet().environmentObject(store) }
            .sheet(isPresented: $showingFilters) { filterSheet }
        }
        .onAppear { updatePresentation() }
        .onChangeCompat(of: store.catalogRevision) { _ in updatePresentation() }
        .onChangeCompat(of: store.user?.userId) { _ in updatePresentation() }
        .onChangeCompat(of: store.settings.favoriteGameIds) { _ in updatePresentation() }
        .onChangeCompat(of: criteria) { _ in updatePresentation() }
        .onDisappear { presentation.cancel() }
        .presentGameDetailsSheet(selectedGame: $selectedGameForDetails, store: store) { game, option in
            pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: option)
        }
        .launcherSelectionModalSheet(selectedGame: $selectedGameForLauncher, store: store) { game, option in
            pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: option)
        }
        .printedWasteLaunchSheet(pendingLaunchRequest: $pendingLaunchRequest)
    }

    private var libraryHeader: some View {
        VStack(alignment: .leading, spacing: OpenNOWSpacing.lg) {
            if let session = store.activeSession {
                currentSession(session)
            }

            if presentation.totalCount > 0, store.catalogError != nil {
                CatalogFailureNotice()
            }

            if dynamicTypeSize.isAccessibilitySize {
                collectionPicker.pickerStyle(.menu)
            } else {
                collectionPicker.pickerStyle(.segmented)
            }

            HStack(spacing: OpenNOWSpacing.sm) {
                VStack(alignment: .leading, spacing: OpenNOWSpacing.xs) {
                    Text(criteria.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                         ? (criteria.favoritesOnly ? "Favorites" : "Your Games") : "Search Results")
                        .font(OpenNOWFont.sectionHead)
                        .foregroundStyle(OpenNOWPalette.textPrimary)
                    Text(presentation.countLabel)
                        .font(.subheadline)
                        .foregroundStyle(OpenNOWPalette.textMuted)
                }

                Spacer(minLength: OpenNOWSpacing.sm)

                if presentation.isUpdating || store.isLoadingGames {
                    ProgressView().controlSize(.small).accessibilityLabel("Updating library")
                }

                Button { showingFilters = true } label: {
                    Image(systemName: criteria.hasFilters ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle")
                        .font(.title3)
                        .minimumHitTarget()
                }
                .accessibilityLabel("Filter Library")
                .accessibilityValue(criteria.hasFilters ? "Filters active" : "No filters")
                libraryOptions
            }

            if presentation.stores.count > 1 || criteria.storefront != nil {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: OpenNOWSpacing.sm) {
                        launcherChip(nil, title: "All Launchers")
                        ForEach(filterOptions(presentation.stores, selected: criteria.storefront), id: \.self) { storefront in
                            launcherChip(storefront, title: storeDisplayName(storefront))
                        }
                    }
                }
                .accessibilityLabel("Filter by launcher")
            }

            if criteria.genre != nil || criteria.platform != nil || criteria.touchOnly {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: OpenNOWSpacing.sm) {
                        if let genre = criteria.genre {
                            filterChip(genre) { criteria.genre = nil }
                        }
                        if let platform = criteria.platform {
                            filterChip(platform) { criteria.platform = nil }
                        }
                        if criteria.touchOnly {
                            filterChip("Touch controls") { criteria.touchOnly = false }
                        }
                        Button("Clear Filters") { criteria.clearFilters() }
                            .font(.subheadline)
                            .minimumHitTarget()
                    }
                }
            }
        }
    }

    private var collectionPicker: some View {
        Picker("Library collection", selection: $criteria.favoritesOnly) {
            Text("All Games").tag(false)
            Text("Favorites").tag(true)
        }
    }

    @ViewBuilder
    private var libraryContent: some View {
        if presentation.totalCount == 0 && (store.isLoadingGames || presentation.isUpdating) {
            loadingLibrary
        } else if presentation.totalCount == 0, let error = store.catalogError {
            OpenNOWUnavailableView("Couldn’t Load Your Library", systemImage: "wifi.exclamationmark") {
                Text(error)
            } actions: {
                Button(recoveryTitle, action: recoverLibrary)
                    .buttonStyle(.borderedProminent)
            }
            .frame(maxWidth: .infinity, minHeight: 300)
        } else if presentation.items.isEmpty {
            emptyLibrary
        } else if effectiveLayout == .grid {
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: sizeClass == .regular ? 160 : 136, maximum: 220), spacing: OpenNOWSpacing.lg, alignment: .top)],
                alignment: .leading,
                spacing: OpenNOWSpacing.xl
            ) {
                ForEach(presentation.items) { item in
                    gameTile(item)
                }
            }
        } else {
            LazyVStack(spacing: 0) {
                ForEach(presentation.items) { item in
                    gameTile(item)
                        .padding(.vertical, OpenNOWSpacing.md)
                    Divider().padding(.leading, 76)
                }
            }
        }
    }

    private func gameTile(_ item: LibraryGameItem) -> some View {
        LibraryGameTile(
            item: item,
            layout: effectiveLayout,
            onDetails: { selectedGameForDetails = item.game },
            onPlay: { launchFromCard(item.game) },
            onFavorite: { store.toggleFavorite(item.game) },
            onChooseLauncher: { selectedGameForLauncher = item.game }
        )
        .equatable()
    }

    private var loadingLibrary: some View {
        VStack(alignment: .leading, spacing: OpenNOWSpacing.lg) {
            Text("Loading your games…")
                .font(.subheadline)
                .foregroundStyle(OpenNOWPalette.textMuted)
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: sizeClass == .regular ? 160 : 136, maximum: 220), spacing: OpenNOWSpacing.lg)],
                spacing: OpenNOWSpacing.xl
            ) {
                ForEach(0..<6, id: \.self) { _ in
                    VStack(alignment: .leading, spacing: OpenNOWSpacing.sm) {
                        RoundedRectangle(cornerRadius: OpenNOWRadius.md)
                            .fill(OpenNOWPalette.surfaceInset)
                            .aspectRatio(2.0 / 3.0, contentMode: .fit)
                        RoundedRectangle(cornerRadius: OpenNOWRadius.xs)
                            .fill(OpenNOWPalette.surfaceInset)
                            .frame(height: 14)
                            .padding(.trailing, 24)
                    }
                    .accessibilityHidden(true)
                }
            }
        }
    }

    private var emptyLibrary: some View {
        OpenNOWUnavailableView(emptyTitle, systemImage: emptySymbol) {
            Text(emptyDescription)
        } actions: {
            if criteria.isNarrowed {
                Button(criteria.favoritesOnly && !criteria.hasFilters && criteria.query.isEmpty ? "Show All Games" : "Clear Search and Filters") {
                    criteria.clearNarrowing()
                }
                .buttonStyle(.borderedProminent)
            } else if store.accountConnectors.contains(where: \.isLinked) {
                Button("Refresh Library") { Task { await store.refreshCatalog() } }
                    .buttonStyle(.borderedProminent)
                Button("Add a Game") { showingAddGame = true }
                    .buttonStyle(.bordered)
            } else {
                Button("Link a Game Store") { store.pendingSettingsRoute = .account }
                    .buttonStyle(.borderedProminent)
                Button("Add a Game") { showingAddGame = true }
                    .buttonStyle(.bordered)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 300)
    }

    private var emptyTitle: String {
        if !criteria.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return "No Games Found" }
        if criteria.hasFilters { return "No Games Match These Filters" }
        if criteria.favoritesOnly { return "No Favorites Yet" }
        return "Make This Library Yours"
    }

    private var emptySymbol: String {
        if !criteria.query.isEmpty { return "magnifyingglass" }
        if criteria.hasFilters { return "line.3.horizontal.decrease.circle" }
        return criteria.favoritesOnly ? "heart" : "gamecontroller"
    }

    private var emptyDescription: String {
        if !criteria.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return "Try another title, or clear your filters to search all the games in your library."
        }
        if criteria.hasFilters { return "Try a different launcher or remove a filter to see more of your games." }
        if criteria.favoritesOnly { return "Add a favorite from a game’s details or its touch-and-hold menu. It’ll be waiting here." }
        if store.accountConnectors.contains(where: \.isLinked) {
            return "Your linked stores haven’t returned any owned games yet. Refresh your library, or add a game manually."
        }
        return "Link your game stores to bring your owned games together, or add a GeForce NOW game manually."
    }

    private var libraryOptions: some View {
        Menu {
            Picker("Sort By", selection: $criteria.order) {
                ForEach(LibraryOrder.allCases) { order in
                    Label(order.title, systemImage: order.symbol).tag(order)
                }
            }
            if !dynamicTypeSize.isAccessibilitySize {
                Picker("View As", selection: $layout) {
                    Label("Grid", systemImage: "square.grid.2x2").tag(LibraryLayout.grid)
                    Label("List", systemImage: "list.bullet").tag(LibraryLayout.list)
                }
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
                .font(.title3)
                .minimumHitTarget()
        }
        .accessibilityLabel("Library Sort and View")
        .accessibilityValue("\(criteria.order.title), \(effectiveLayout == .grid ? "Grid" : "List")")
    }

    private func launcherChip(_ storefront: String?, title: String) -> some View {
        let selected = criteria.storefront == storefront
        return Button {
            criteria.storefront = storefront
        } label: {
            Text(title)
                .font(.subheadline.weight(selected ? .semibold : .regular))
                .foregroundStyle(selected ? OpenNOWPalette.textPrimary : OpenNOWPalette.textMuted)
                .padding(.horizontal, OpenNOWSpacing.md)
                .frame(minHeight: OpenNOWSpacing.hitTarget)
                .background(selected ? OpenNOWPalette.surfaceInset : .clear, in: Capsule())
                .overlay(Capsule().strokeBorder(selected ? OpenNOWPalette.hairline : .clear, lineWidth: 1))
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private func filterChip(_ title: String, remove: @escaping () -> Void) -> some View {
        Button(action: remove) {
            Label(title, systemImage: "xmark.circle.fill")
                .font(.subheadline)
                .minimumHitTarget()
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Remove \(title) filter")
    }

    private var filterSheet: some View {
        NavigationStack {
            Form {
                Section {
                    Picker("Launcher", selection: optionalBinding($criteria.storefront)) {
                        Text("All Launchers").tag("")
                        ForEach(filterOptions(presentation.stores, selected: criteria.storefront), id: \.self) { Text(storeDisplayName($0)).tag($0) }
                    }
                    Toggle("Touch controls", isOn: $criteria.touchOnly)
                } footer: {
                    Text("Touch controls shows games whose catalog lists native touchscreen support.")
                }
                Section("Game Details") {
                    Picker("Genre", selection: optionalBinding($criteria.genre)) {
                        Text("All Genres").tag("")
                        ForEach(filterOptions(presentation.genres, selected: criteria.genre), id: \.self) { Text($0).tag($0) }
                    }
                    if presentation.platforms.count > 1 || criteria.platform != nil {
                        Picker("Platform", selection: optionalBinding($criteria.platform)) {
                            Text("All Platforms").tag("")
                            ForEach(filterOptions(presentation.platforms, selected: criteria.platform), id: \.self) { Text($0).tag($0) }
                        }
                    }
                }
                if criteria.hasFilters {
                    Section { Button("Reset Filters") { criteria.clearFilters() } }
                }
            }
            .navigationTitle("Filter Library")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") { showingFilters = false }
                }
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }

    private func currentSession(_ session: ActiveSession) -> some View {
        let state = SessionScreenPresentation(session: session, availability: store.sessionAvailability, isStreaming: store.streamSession != nil)
        return Button { store.jumpBackToSession() } label: {
            HStack(spacing: OpenNOWSpacing.md) {
                GameArtworkView(game: session.game, iconSize: 20)
                    .frame(width: 46, height: 62)
                    .clipShape(RoundedRectangle(cornerRadius: OpenNOWRadius.sm))
                VStack(alignment: .leading, spacing: OpenNOWSpacing.xs) {
                    Text(session.game.title)
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(OpenNOWPalette.textPrimary)
                        .lineLimit(2)
                    Text(state.status)
                        .font(.caption)
                        .foregroundStyle(OpenNOWPalette.textMuted)
                }
                Spacer(minLength: OpenNOWSpacing.sm)
                if state.isChecking {
                    ProgressView().controlSize(.small)
                } else {
                    Image(systemName: state.symbol).font(.title2)
                }
            }
            .padding(OpenNOWSpacing.md)
            .openNowCard(bordered: true)
        }
        .buttonStyle(.plain)
        .disabled(state.isChecking || store.isEndingSession)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(session.game.title), \(state.status)")
        .accessibilityHint(state.actionTitle)
    }

    private var recoveryTitle: String {
        let failure = store.lastFailure
        return failure?.message == store.catalogError ? failure?.recovery.label ?? "Try Again" : "Try Again"
    }

    private func recoverLibrary() {
        if let failure = store.lastFailure, failure.message == store.catalogError, failure.recovery.label != nil {
            store.performRecovery(failure.recovery)
        } else {
            Task { await store.refreshCatalog() }
        }
    }

    private func optionalBinding(_ value: Binding<String?>) -> Binding<String> {
        Binding(get: { value.wrappedValue ?? "" }, set: { value.wrappedValue = $0.isEmpty ? nil : $0 })
    }

    private func filterOptions(_ options: [String], selected: String?) -> [String] {
        guard let selected, !options.contains(selected) else { return options }
        return options + [selected]
    }

    private func updatePresentation() {
        presentation.update(
            games: store.visibleLibraryGames,
            sourceRevision: store.catalogRevision,
            accountID: store.user?.userId,
            favoriteIDs: store.settings.favoriteGameIds,
            criteria: criteria
        )
    }

    private func launchFromCard(_ game: CloudGame) {
        switch store.launchChoice(for: game) {
        case .chooseLauncher: selectedGameForLauncher = game
        case let .launch(option): pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: option)
        }
    }
}

private enum LibraryLayout: Hashable {
    case grid
    case list
}

enum LibraryOrder: String, CaseIterable, Identifiable, Hashable, Sendable {
    case recent
    case title
    var id: String { rawValue }
    var title: String { self == .recent ? "Recently Played" : "Title" }
    var symbol: String { self == .recent ? "clock.arrow.circlepath" : "textformat" }
}

struct LibraryScreenCriteria: Equatable, Sendable {
    var query = ""
    var storefront: String?
    var genre: String?
    var platform: String?
    var favoritesOnly = false
    var touchOnly = false
    var order = LibraryOrder.recent

    var hasFilters: Bool { storefront != nil || genre != nil || platform != nil || touchOnly }
    var isNarrowed: Bool { hasFilters || favoritesOnly || !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    mutating func clearFilters() {
        storefront = nil
        genre = nil
        platform = nil
        touchOnly = false
    }

    mutating func clearNarrowing() {
        clearFilters()
        query = ""
        favoritesOnly = false
    }
}

struct LibraryCatalogEntry: Equatable, Sendable {
    let game: CloudGame
    let storefronts: [String]
    let subtitle: String
    let searchableText: String
    let supportsTouch: Bool
    let lastPlayed: Date
}

struct LibraryGameItem: Identifiable, Equatable, Sendable {
    let entry: LibraryCatalogEntry
    let isFavorite: Bool
    var id: String { entry.game.id }
    var game: CloudGame { entry.game }
}

struct LibraryCatalogIndex: Sendable {
    let entries: [LibraryCatalogEntry]
    let stores: [String]
    let genres: [String]
    let platforms: [String]

    init(games: [CloudGame]) throws {
        let fractionalDate = ISO8601DateFormatter()
        fractionalDate.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let wholeDate = ISO8601DateFormatter()
        wholeDate.formatOptions = [.withInternetDateTime]
        var entries: [LibraryCatalogEntry] = []
        var stores = Set<String>()
        var genres = Set<String>()
        var platforms = Set<String>()
        entries.reserveCapacity(games.count)
        for game in games {
            try Task.checkCancellation()
            let ownedStores = game.ownedStorefronts
            let storefronts = (ownedStores.isEmpty ? gameResolvedStores(game: game) : ownedStores)
                .map { $0.trimmingCharacters(in: .whitespacesAndNewlines).uppercased() }
                .filter { !$0.isEmpty && $0 != "AUTO" }
            stores.formUnion(storefronts)
            if !game.genre.isEmpty && game.genre != "Cloud Game" { genres.insert(game.genre) }
            if !game.platform.isEmpty { platforms.insert(game.platform) }
            let searchableText = ([game.title, game.genre, game.platform, game.summary, game.longDescription, game.publisher, game.developer]
                .compactMap { $0 } + (game.featureLabels ?? []) + (game.tags ?? []) + (game.stores ?? [])
                + game.launchOptions.map(\.storefront)).joined(separator: " ")
            entries.append(LibraryCatalogEntry(
                game: game, storefronts: storefronts,
                subtitle: storefronts.map(storeDisplayName).joined(separator: " · "),
                searchableText: searchableText,
                supportsTouch: NativeTouchSupport.catalogClaimsTouchSupport(game),
                lastPlayed: game.launchOptions.compactMap { option -> Date? in
                    guard let value = option.lastPlayedDate?.trimmingCharacters(in: .whitespacesAndNewlines) else { return nil }
                    return fractionalDate.date(from: value) ?? wholeDate.date(from: value)
                }.max() ?? .distantPast
            ))
        }
        self.entries = entries
        self.stores = stores.sorted()
        self.genres = genres.sorted()
        self.platforms = platforms.sorted()
    }

    func items(matching criteria: LibraryScreenCriteria, favoriteIDs: Set<String>) throws -> [LibraryGameItem] {
        let terms = criteria.query.split(whereSeparator: \.isWhitespace).map(String.init)
        var matches: [LibraryGameItem] = []
        for entry in entries {
            try Task.checkCancellation()
            let favorite = favoriteIDs.contains(entry.game.id)
            guard !criteria.favoritesOnly || favorite,
                  !criteria.touchOnly || entry.supportsTouch,
                  criteria.storefront.map({ entry.storefronts.contains($0) }) ?? true,
                  criteria.genre.map({ entry.game.genre == $0 }) ?? true,
                  criteria.platform.map({ entry.game.platform == $0 }) ?? true,
                  terms.allSatisfy({ entry.searchableText.localizedCaseInsensitiveContains($0) }) else { continue }
            matches.append(LibraryGameItem(entry: entry, isFavorite: favorite))
        }
        var comparisons = 0
        return try matches.sorted {
            comparisons += 1
            if comparisons.isMultiple(of: 256) { try Task.checkCancellation() }
            if criteria.order == .recent, $0.entry.lastPlayed != $1.entry.lastPlayed {
                return $0.entry.lastPlayed > $1.entry.lastPlayed
            }
            return $0.game.title.localizedCaseInsensitiveCompare($1.game.title) == .orderedAscending
        }
    }
}

@MainActor
private final class LibraryPresentationModel: ObservableObject {
    @Published private(set) var items: [LibraryGameItem] = []
    @Published private(set) var totalCount = 0
    @Published private(set) var stores: [String] = []
    @Published private(set) var genres: [String] = []
    @Published private(set) var platforms: [String] = []
    @Published private(set) var isUpdating = true

    private struct Key: Equatable {
        let sourceRevision: UUID
        let accountID: String?
        let favoriteIDs: [String]
        let criteria: LibraryScreenCriteria
    }

    private struct Calculation: Sendable {
        let index: LibraryCatalogIndex
        let items: [LibraryGameItem]
    }

    private var key: Key?
    private var indexedRevision: UUID?
    private var indexedAccountID: String?
    private var index: LibraryCatalogIndex?
    private var generation = UUID()
    private var computation: Task<Calculation, Error>?

    var countLabel: String {
        if isUpdating && totalCount == 0 { return "Gathering your games" }
        return items.count == 1 ? "1 game" : "\(items.count) games"
    }

    func update(games: [CloudGame], sourceRevision: UUID, accountID: String?, favoriteIDs: [String], criteria: LibraryScreenCriteria) {
        let requested = Key(sourceRevision: sourceRevision, accountID: accountID, favoriteIDs: favoriteIDs, criteria: criteria)
        guard key != requested else { return }
        let accountChanged = (key?.accountID ?? indexedAccountID) != accountID
        computation?.cancel()
        key = requested
        generation = UUID()
        let requestedGeneration = generation
        if accountChanged {
            index = nil
            indexedRevision = nil
            indexedAccountID = accountID
            items = []
            totalCount = 0
            stores = []
            genres = []
            platforms = []
        }
        isUpdating = true
        let reusableIndex = indexedRevision == sourceRevision && indexedAccountID == accountID ? index : nil
        let computation = Task.detached(priority: .userInitiated) {
            let index = try reusableIndex ?? LibraryCatalogIndex(games: games)
            let items = try index.items(matching: criteria, favoriteIDs: Set(favoriteIDs))
            try Task.checkCancellation()
            return Calculation(index: index, items: items)
        }
        self.computation = computation
        Task { [weak self] in
            do {
                let result = try await computation.value
                guard let self, self.generation == requestedGeneration else { return }
                self.index = result.index
                self.indexedRevision = sourceRevision
                self.indexedAccountID = accountID
                self.items = result.items
                self.totalCount = result.index.entries.count
                self.stores = result.index.stores
                self.genres = result.index.genres
                self.platforms = result.index.platforms
                self.isUpdating = false
            } catch {
                guard let self, self.generation == requestedGeneration else { return }
                self.isUpdating = false
            }
        }
    }

    func cancel() {
        computation?.cancel()
        computation = nil
        generation = UUID()
        key = nil
        isUpdating = false
    }

    deinit { computation?.cancel() }
}

private struct LibraryGameTile: View, Equatable {
    @EnvironmentObject private var shortcuts: CatalogControllerShortcutCoordinator
    @Environment(\.gameDetailsTransition) private var transition
    @Environment(\.openNowAccent) private var accent
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @FocusState private var focused: Bool
    @State private var legacyFocused = false
    @State private var sourceID = UUID()
    @State private var shortcutOwner = UUID()
    let item: LibraryGameItem
    let layout: LibraryLayout
    let onDetails: () -> Void
    let onPlay: () -> Void
    let onFavorite: () -> Void
    let onChooseLauncher: () -> Void

    static func == (lhs: Self, rhs: Self) -> Bool { lhs.item == rhs.item && lhs.layout == rhs.layout }

    private var isFocused: Bool { focused || legacyFocused }
    private var canPlay: Bool { OpenNOWPlatform.supportsEmbeddedStreamer && !item.game.launchOptions.isEmpty }

    var body: some View {
        Button(action: openDetails) {
            Group {
                if layout == .grid {
                    VStack(alignment: .leading, spacing: OpenNOWSpacing.sm) {
                        artwork.aspectRatio(2.0 / 3.0, contentMode: .fit)
                        caption
                    }
                } else {
                    HStack(spacing: OpenNOWSpacing.lg) {
                        artwork.frame(width: 60, height: 80)
                        caption
                        Spacer(minLength: OpenNOWSpacing.sm)
                        Image(systemName: "chevron.right")
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(OpenNOWPalette.textFaint)
                    }
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .controllerFocusableCompat(fallbackActivation: openDetails, onLegacyFocusChange: { legacyFocused = $0 })
        .focused($focused)
        .accessibilityLabel(item.game.title)
        .accessibilityValue([item.entry.subtitle, item.isFavorite ? "Favorite" : nil, item.entry.supportsTouch ? "Touch controls" : nil].compactMap { $0 }.joined(separator: ", "))
        .accessibilityHint("Opens game details")
        .contextMenu {
            if canPlay {
                Button(action: onPlay) { Label("Play", systemImage: "play.fill") }
                if item.game.launchOptions.count > 1 {
                    Button(action: onChooseLauncher) { Label("Choose Launcher", systemImage: "rectangle.stack") }
                }
            }
            Button(action: onFavorite) {
                Label(item.isFavorite ? "Remove from Favorites" : "Add to Favorites",
                      systemImage: item.isFavorite ? "heart.slash" : "heart")
            }
            Button(action: openDetails) { Label("Game Details", systemImage: "info.circle") }
        }
        .onChangeCompat(of: isFocused) { registerShortcuts($0) }
        .onChangeCompat(of: shortcuts.isEnabled) { registerShortcuts($0 && isFocused) }
        .onDisappear { shortcuts.clearFocusedActions(owner: shortcutOwner) }
    }

    private var artwork: some View {
        GameArtworkView(game: item.game, iconSize: layout == .grid ? 38 : 20)
            .clipShape(RoundedRectangle(cornerRadius: OpenNOWRadius.md, style: .continuous))
            .gameDetailsArtworkSource(id: sourceID, cornerRadius: OpenNOWRadius.md)
            .overlay(alignment: .topTrailing) {
                if item.isFavorite && layout == .grid {
                    Image(systemName: "heart.fill")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.white)
                        .padding(8)
                        .background(.black.opacity(0.72), in: Circle())
                        .padding(8)
                        .accessibilityHidden(true)
                }
            }
            .overlay {
                RoundedRectangle(cornerRadius: OpenNOWRadius.md, style: .continuous)
                    .strokeBorder(isFocused ? accent.color : OpenNOWPalette.hairline, lineWidth: isFocused ? 2 : 0.5)
            }
            .animation(reduceMotion ? nil : OpenNOWMotion.fast, value: isFocused)
    }

    private var caption: some View {
        VStack(alignment: .leading, spacing: OpenNOWSpacing.xs) {
            Text(item.game.title)
                .font(layout == .grid ? .subheadline.weight(.semibold) : .headline)
                .foregroundStyle(OpenNOWPalette.textPrimary)
                .lineLimit(layout == .grid ? 2 : nil)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
            if !item.entry.subtitle.isEmpty {
                Text(item.entry.subtitle)
                    .font(.caption)
                    .foregroundStyle(OpenNOWPalette.textMuted)
                    .lineLimit(layout == .grid ? 1 : nil)
                    .multilineTextAlignment(.leading)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func openDetails() {
        Haptics.light()
        transition?.selectSource(GameDetailsTransitionOrigin(sourceID: sourceID, gameKey: catalogStableGameKey(item.game)))
        onDetails()
    }

    private func registerShortcuts(_ enabled: Bool) {
        shortcuts.updateFocusedActions(owner: shortcutOwner, isFocused: enabled, favorite: onFavorite, play: {
            if canPlay { onPlay() }
        })
    }
}

private struct AddLibraryGameSheet: View {
    @EnvironmentObject private var store: OpenNOWStore
    @Environment(\.dismiss) private var dismiss
    @State private var input = ""
    @State private var title = ""
    @State private var storefront = "STEAM"
    @State private var results: [CloudGame] = []
    @State private var isLookingUp = false
    @State private var error: String?
    @State private var gameToRemove: CloudGame?

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Game name, GeForce NOW link, or app ID", text: $input)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                    TextField("Name for a manually added app ID (optional)", text: $title)
                    Picker("Store for a manually added app ID", selection: $storefront) {
                        Text("Steam").tag("STEAM")
                        Text("Epic").tag("EPIC")
                        Text("GOG").tag("GOG")
                        Text("Xbox").tag("XBOX")
                        Text("Ubisoft").tag("UPLAY")
                    }
                    Button {
                        Task { await findGames() }
                    } label: {
                        HStack {
                            Text("Find Game")
                            if isLookingUp { Spacer(); ProgressView() }
                        }
                    }
                    .disabled(input.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isLookingUp)
                } header: {
                    Text("Find a Game")
                } footer: {
                    Text("Added games stay in this account’s Library. A GeForce NOW app ID is different from a Steam store ID. NVIDIA still controls game availability.")
                }
                if let error {
                    Section { Text(error).foregroundStyle(.red) }
                }
                if !results.isEmpty {
                    Section("Results") {
                        ForEach(results) { game in
                            Button {
                                store.addImportedGame(game)
                                dismiss()
                            } label: {
                                HStack {
                                    VStack(alignment: .leading) {
                                        Text(game.title).foregroundStyle(.primary)
                                        Text(game.platform).font(.caption).foregroundStyle(.secondary)
                                    }
                                    Spacer()
                                    Image(systemName: "plus.circle")
                                }
                            }
                        }
                    }
                }
                if !store.importedGames.isEmpty {
                    Section("Added Games") {
                        ForEach(store.importedGames) { game in
                            HStack {
                                Text(game.title)
                                Spacer()
                                Button(role: .destructive) { gameToRemove = game } label: {
                                    Image(systemName: "minus.circle")
                                        .minimumHitTarget()
                                }
                                .buttonStyle(.borderless)
                                .accessibilityLabel("Remove \(game.title) from added games")
                            }
                        }
                    }
                }
            }
            .navigationTitle("Add Game")
            .toolbar { ToolbarItem(placement: .cancellationAction) { Button("Done") { dismiss() } } }
            .onChange(of: input) { _ in results = []; error = nil }
            .confirmationDialog(
                "Remove Added Game?",
                isPresented: Binding(
                    get: { gameToRemove != nil },
                    set: { if !$0 { gameToRemove = nil } }
                ),
                titleVisibility: .visible,
                presenting: gameToRemove
            ) { game in
                Button("Remove from Added Games", role: .destructive) {
                    store.removeImportedGame(game)
                    gameToRemove = nil
                }
                Button("Cancel", role: .cancel) { gameToRemove = nil }
            } message: { game in
                Text("Remove \(game.title) from this account’s added games? This won’t change ownership in your game stores or end a cloud session.")
            }
        }
    }

    @MainActor private func findGames() async {
        isLookingUp = true
        error = nil
        let submittedInput = input
        defer { isLookingUp = false }
        do {
            let matches = try await store.lookupGamesToAdd(submittedInput, title: title, storefront: storefront)
            guard input == submittedInput else { return }
            results = matches
            if matches.isEmpty { error = "No matching games were returned. Try the GeForce NOW link or app ID." }
        } catch {
            guard input == submittedInput else { return }
            self.error = error.localizedDescription
        }
    }
}
