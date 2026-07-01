import SwiftUI

struct BrowseView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var selectedGenre: String?
    @State private var selectedPlatform: String?
    @State private var selectedStore: String?
    @State private var favoritesOnly = false
    @State private var sortMode: CatalogSortMode = .title
    @State private var pendingLaunchRequest: GameLaunchRequest?
    @State private var selectedGameForDetails: CloudGame?

    var body: some View {
        NavigationStack {
            List {
                if store.isLoadingGames && store.allGames.isEmpty {
                    loadingSection
                } else if filteredGames.isEmpty {
                    emptySection
                } else {
                    Section {
                        ForEach(gameBannerRows(for: filteredGames)) { row in
                            GameBannerRowView(
                                games: row.games,
                                subtitle: { gameCatalogSubtitle(for: $0) },
                                badgeSystemImage: { store.isFavorite($0) ? "heart.fill" : nil }
                            ) { game in
                                selectedGameForDetails = game
                            }
                            .gameBannerGridListRowStyle()
                        }
                    } header: {
                        Text("\(filteredGames.count) Games")
                    }
                }
            }
            .listStyle(.insetGrouped)
            .navigationTitle("Browse")
            .searchable(text: $store.searchText, placement: .navigationBarDrawer(displayMode: .automatic), prompt: "Search games")
            .refreshable { await store.refreshCatalog() }
            .toolbar {
                ToolbarItemGroup(placement: .topBarTrailing) {
                    filterMenu
                    sortMenu
                }
            }
        }
        .presentGameDetailsUIKit(selectedGame: $selectedGameForDetails) { game, option in
            pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: option)
        }
        .printedWasteLaunchSheet(pendingLaunchRequest: $pendingLaunchRequest)
    }

    private var loadingSection: some View {
        Section {
            ForEach(0..<3, id: \.self) { _ in
                GameBannerSkeletonRowView()
                    .gameBannerGridListRowStyle()
            }
        }
    }

    private var emptySection: some View {
        Section {
            OpenNOWUnavailableView(
                hasActiveFilters ? "No Matches" : "No Games",
                systemImage: hasActiveFilters ? "line.3.horizontal.decrease.circle" : "square.grid.2x2"
            )
            if hasActiveFilters {
                Button("Clear Filters") {
                    clearFilters()
                }
            }
        }
    }

    private var filterMenu: some View {
        Menu {
            Toggle("Favorites", isOn: $favoritesOnly)

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
                Button("Clear Filters", role: .destructive) {
                    clearFilters()
                }
            }
        } label: {
            Image(systemName: hasActiveFilters ? "line.3.horizontal.decrease.circle.fill" : "line.3.horizontal.decrease.circle")
        }
        .accessibilityLabel("Filters")
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
        }
        .accessibilityLabel("Sort")
    }

    private var genres: [String] {
        Array(Set(store.allGames.map(\.genre).filter { !$0.isEmpty })).sorted()
    }

    private var platforms: [String] {
        Array(Set(store.allGames.map(\.platform).filter { !$0.isEmpty })).sorted()
    }

    private var stores: [String] {
        Array(Set(store.allGames.flatMap { gameResolvedStores(game: $0) })).sorted()
    }

    private var filteredGames: [CloudGame] {
        let filtered = store.filteredCatalogGames.filter { game in
            let matchesGenre = selectedGenre == nil || game.genre == selectedGenre
            let matchesPlatform = selectedPlatform == nil || game.platform == selectedPlatform
            let matchesStore = selectedStore.map { gameResolvedStores(game: game).contains($0) } ?? true
            let matchesFavorite = !favoritesOnly || store.isFavorite(game)
            return matchesGenre && matchesPlatform && matchesStore && matchesFavorite
        }

        switch sortMode {
        case .title:
            return filtered.sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
        case .genre:
            return filtered.sorted {
                $0.genre == $1.genre
                    ? $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
                    : $0.genre.localizedCaseInsensitiveCompare($1.genre) == .orderedAscending
            }
        case .platform:
            return filtered.sorted {
                $0.platform == $1.platform
                    ? $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
                    : $0.platform.localizedCaseInsensitiveCompare($1.platform) == .orderedAscending
            }
        }
    }

    private var hasActiveFilters: Bool {
        !store.searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
            selectedGenre != nil ||
            selectedPlatform != nil ||
            selectedStore != nil ||
            favoritesOnly ||
            sortMode != .title
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
        sortMode = .title
    }

}

enum CatalogSortMode: String, CaseIterable, Identifiable {
    case title
    case genre
    case platform

    var id: String { rawValue }

    var title: String {
        switch self {
        case .title: return "Title"
        case .genre: return "Genre"
        case .platform: return "Platform"
        }
    }

    var icon: String {
        switch self {
        case .title: return "textformat"
        case .genre: return "square.grid.2x2"
        case .platform: return "gamecontroller"
        }
    }
}
