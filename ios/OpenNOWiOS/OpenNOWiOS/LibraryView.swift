import SwiftUI

struct LibraryView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var pendingLaunchRequest: GameLaunchRequest?
    @State private var selectedGameForDetails: CloudGame?
    @State private var searchText = ""
    @State private var selectedGenre: String?
    @State private var selectedPlatform: String?
    @State private var favoritesOnly = false
    @State private var sortMode: CatalogSortMode = .title

    var body: some View {
        NavigationStack {
            List {
                if store.user == nil {
                    Section {
                        OpenNOWUnavailableView("Signed Out", systemImage: "person.crop.circle.badge.exclamationmark")
                    }
                } else if store.libraryGames.isEmpty && store.isLoadingGames {
                    Section {
                        ForEach(0..<3, id: \.self) { _ in
                            GameBannerSkeletonRowView()
                                .gameBannerGridListRowStyle()
                        }
                    } header: {
                        Text("Loading Library")
                    }
                } else if store.libraryGames.isEmpty && !store.isLoadingGames {
                    Section {
                        OpenNOWUnavailableView("Library Empty", systemImage: "books.vertical")
                    }
                } else if filteredGames.isEmpty {
                    Section {
                        OpenNOWUnavailableView("No Matches", systemImage: "magnifyingglass")
                        if hasActiveFilters {
                            Button("Clear Filters") {
                                clearFilters()
                            }
                        }
                    }
                } else {
                    if let active = store.activeSession {
                        Section("Current Session") {
                            GameBannerRowView(
                                games: [active.game],
                                subtitle: { _ in active.status == 3 ? "Streaming" : "Queued" },
                                badgeSystemImage: { _ in active.status == 3 ? "play.circle.fill" : "hourglass" }
                            ) { _ in
                                store.jumpBackToSession()
                            }
                            .gameBannerGridListRowStyle()
                        }
                    }

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
            .navigationTitle("Library")
            .searchable(text: $searchText, placement: .navigationBarDrawer(displayMode: .automatic), prompt: "Search library")
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

    private var filteredGames: [CloudGame] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        let filtered = store.libraryGames.filter { game in
            let matchesQuery = query.isEmpty ||
                game.title.localizedCaseInsensitiveContains(query) ||
                game.genre.localizedCaseInsensitiveContains(query) ||
                game.platform.localizedCaseInsensitiveContains(query) ||
                (game.publisher?.localizedCaseInsensitiveContains(query) ?? false) ||
                (game.developer?.localizedCaseInsensitiveContains(query) ?? false)
            let matchesGenre = selectedGenre == nil || game.genre == selectedGenre
            let matchesPlatform = selectedPlatform == nil || game.platform == selectedPlatform
            let matchesFavorite = !favoritesOnly || store.isFavorite(game)
            return matchesQuery && matchesGenre && matchesPlatform && matchesFavorite
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

    private var genres: [String] {
        Array(Set(store.libraryGames.map(\.genre).filter { !$0.isEmpty })).sorted()
    }

    private var platforms: [String] {
        Array(Set(store.libraryGames.map(\.platform).filter { !$0.isEmpty })).sorted()
    }

    private var hasActiveFilters: Bool {
        !searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
            selectedGenre != nil ||
            selectedPlatform != nil ||
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
        searchText = ""
        selectedGenre = nil
        selectedPlatform = nil
        favoritesOnly = false
        sortMode = .title
    }

}
