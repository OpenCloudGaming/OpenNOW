import SwiftUI

struct BrowseView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var selectedGenre: String?
    @State private var selectedPlatform: String?
    @State private var selectedStore: String?
    @State private var favoritesOnly = false
    @State private var touchControlsOnly = false
    @State private var sortMode: CatalogSortMode = .title
    @State private var pendingLaunchRequest: GameLaunchRequest?
    @State private var selectedGameForDetails: CloudGame?
    @State private var selectedGameForLauncher: CloudGame?

    var body: some View {
        NavigationStack {
            GameCatalogGridView(
                games: filteredGames,
                isLoading: store.isLoadingGames && store.allGames.isEmpty,
                isSearching: store.isSearchingCatalog,
                emptyTitle: store.catalogError != nil && !hasNarrowingFilters ? "Games Unavailable" : hasNarrowingFilters ? "No Matches" : "No Games",
                emptySystemImage: hasNarrowingFilters ? "line.3.horizontal.decrease.circle" : "square.grid.2x2",
                emptyDescription: hasNarrowingFilters ? "Try another search or remove a filter." : nil,
                subtitle: { gameCatalogSubtitle(for: $0) },
                badgeSystemImage: { _ in nil },
                onOpenDetails: { selectedGameForDetails = $0 },
                onPlay: launchFromCard,
                onChooseLauncher: { selectedGameForLauncher = $0 }
            ) {
                browseHeader
            } emptyActions: {
                if hasActiveFilters {
                    Button("Clear Filters") {
                        clearFilters()
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(brandAccent)
                }
            }
            .navigationTitle("Browse")
            .searchable(text: $store.searchText, placement: .navigationBarDrawer(displayMode: .automatic), prompt: "Search games")
            .refreshable { await store.refreshCatalog() }
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

    private var browseHeader: some View {
        VStack(alignment: .leading, spacing: OpenNOWSpacing.md) {
            CatalogFailureNotice()
            CatalogControlsHeader(
                title: browseCountTitle,
                subtitle: store.isLoadingFullCatalog ? "Loading full NVIDIA catalog…" : store.searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "Store catalog" : "Search results",
                chips: activeFilterChips,
                onClear: hasActiveFilters ? clearFilters : nil
            ) {
                HStack(spacing: 8) {
                    if store.isLoadingFullCatalog { ProgressView().accessibilityLabel("Loading full catalog") }
                    if store.isSearchingCatalog { ProgressView().accessibilityLabel("Searching games") }
                    filterMenu
                    sortMenu
                }
            }
        }
    }

    private var filterMenu: some View {
        Menu {
            Toggle("Favorites", isOn: $favoritesOnly)
            Toggle("Touch controls", isOn: $touchControlsOnly)

            Picker("Platform", selection: binding(for: $selectedPlatform)) {
                Text("Any Platform").tag("")
                ForEach(platforms, id: \.self) { platform in
                    Text(platform).tag(platform)
                }
            }

            Picker("Genre", selection: binding(for: $selectedGenre)) {
                Text("Any Genre").tag("")
                ForEach(genres, id: \.self) { genre in
                    Text(genre).tag(genre)
                }
            }

            Picker("Store", selection: binding(for: $selectedStore)) {
                Text("Any Store").tag("")
                ForEach(stores, id: \.self) { storeName in
                    Text(storeDisplayName(storeName)).tag(storeName)
                }
            }

            if hasActiveFilters {
                Divider()
                Button("Clear Filters") {
                    clearFilters()
                }
            }
        } label: {
            Image(systemName: hasActiveFilters ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle")
                .minimumHitTarget()
        }
        .accessibilityLabel("Filters")
        .accessibilityValue(hasActiveFilters ? "Active" : "None")
    }

    private var sortMenu: some View {
        Menu {
            Picker("Sort", selection: $sortMode) {
                ForEach(CatalogSortMode.allCases) { mode in
                    Label(mode.title, systemImage: mode.icon).tag(mode)
                }
            }
        } label: {
            Image(systemName: "arrow.up.arrow.down")
                .minimumHitTarget()
        }
        .accessibilityLabel("Sort")
        .accessibilityValue(sortMode.title)
    }

    private var browseCountTitle: String {
        if filteredGames.count == store.catalogGames.count {
            return store.catalogGames.count == 1 ? "1 Game" : "\(store.catalogGames.count) Games"
        }
        return "\(filteredGames.count) / \(store.catalogGames.count) Games"
    }

    private var activeFilterChips: [CatalogFilterChip] {
        var chips: [CatalogFilterChip] = []
        let query = store.searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        if !query.isEmpty {
            chips.append(CatalogFilterChip(label: "Search: \(query)") {
                store.searchText = ""
            })
        }
        if favoritesOnly {
            chips.append(CatalogFilterChip(label: "Favorites") {
                favoritesOnly = false
            })
        }
        if touchControlsOnly {
            chips.append(CatalogFilterChip(label: "Touch controls") { touchControlsOnly = false })
        }
        if let selectedPlatform {
            chips.append(CatalogFilterChip(label: selectedPlatform) {
                self.selectedPlatform = nil
            })
        }
        if let selectedGenre {
            chips.append(CatalogFilterChip(label: selectedGenre) {
                self.selectedGenre = nil
            })
        }
        if let selectedStore {
            chips.append(CatalogFilterChip(label: storeDisplayName(selectedStore)) {
                self.selectedStore = nil
            })
        }
        if sortMode != .title {
            chips.append(CatalogFilterChip(label: "Sort: \(sortMode.title)") {
                sortMode = .title
            })
        }
        return chips
    }

    private var genres: [String] {
        Array(Set(store.catalogGames.map(\.genre).filter { !$0.isEmpty })).sorted()
    }

    private var platforms: [String] {
        Array(Set(store.catalogGames.map(\.platform).filter { !$0.isEmpty })).sorted()
    }

    private var stores: [String] {
        Array(Set(store.catalogGames.flatMap { gameResolvedStores(game: $0) })).sorted()
    }

    private var filteredGames: [CloudGame] {
        let filtered = store.filteredCatalogGames.filter { game in
            let matchesGenre = selectedGenre == nil || game.genre == selectedGenre
            let matchesPlatform = selectedPlatform == nil || game.platform == selectedPlatform
            let matchesStore = selectedStore.map { gameResolvedStores(game: game).contains($0) } ?? true
            let matchesFavorite = !favoritesOnly || store.isFavorite(game)
            let matchesTouch = !touchControlsOnly || NativeTouchSupport.catalogClaimsTouchSupport(game)
            return matchesGenre && matchesPlatform && matchesStore && matchesFavorite && matchesTouch
        }

        return sortMode.sorted(filtered)
    }

    private var hasActiveFilters: Bool {
        hasNarrowingFilters || sortMode != .title
    }

    private var hasNarrowingFilters: Bool {
        !store.searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
            selectedGenre != nil ||
            selectedPlatform != nil ||
            selectedStore != nil ||
            favoritesOnly ||
            touchControlsOnly
    }

    private func binding(for optional: Binding<String?>) -> Binding<String> {
        Binding(
            get: { optional.wrappedValue ?? "" },
            set: { optional.wrappedValue = $0.isEmpty ? nil : $0 }
        )
    }

    private func clearFilters() {
        store.searchText = ""
        selectedGenre = nil
        selectedPlatform = nil
        selectedStore = nil
        favoritesOnly = false
        touchControlsOnly = false
        sortMode = .title
    }

    private func launchFromCard(_ game: CloudGame) {
        switch store.launchChoice(for: game) {
        case .chooseLauncher:
            selectedGameForLauncher = game
        case let .launch(option):
            pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: option)
        }
    }

}

enum CatalogSortMode: String, CaseIterable, Identifiable {
    case title
    case lastPlayed
    case genre
    case platform

    var id: String { rawValue }

    var title: String {
        switch self {
        case .title: return "Title"
        case .lastPlayed: return "Recently Played"
        case .genre: return "Genre"
        case .platform: return "Platform"
        }
    }

    var icon: String {
        switch self {
        case .title: return "textformat"
        case .lastPlayed: return "clock.arrow.circlepath"
        case .genre: return "square.grid.2x2"
        case .platform: return "gamecontroller"
        }
    }

    func sorted(_ games: [CloudGame]) -> [CloudGame] {
        switch self {
        case .title:
            return games.sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
        case .lastPlayed:
            let standard = ISO8601DateFormatter()
            let fractional = ISO8601DateFormatter()
            fractional.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
            return games.map { game in
                let latest = game.launchOptions.compactMap { option -> Date? in
                    guard let value = option.lastPlayedDate?.trimmingCharacters(in: .whitespacesAndNewlines) else { return nil }
                    return fractional.date(from: value) ?? standard.date(from: value)
                }.max()
                return (game: game, date: latest ?? .distantPast)
            }
                .sorted {
                    $0.date == $1.date
                        ? $0.game.title.localizedCaseInsensitiveCompare($1.game.title) == .orderedAscending
                        : $0.date > $1.date
                }
                .map(\.game)
        case .genre:
            return games.sorted {
                $0.genre == $1.genre
                    ? $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
                    : $0.genre.localizedCaseInsensitiveCompare($1.genre) == .orderedAscending
            }
        case .platform:
            return games.sorted {
                $0.platform == $1.platform
                    ? $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
                    : $0.platform.localizedCaseInsensitiveCompare($1.platform) == .orderedAscending
            }
        }
    }
}
