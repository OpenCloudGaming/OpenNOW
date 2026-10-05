import SwiftUI

struct LibraryView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var pendingLaunchRequest: GameLaunchRequest?
    @State private var selectedGameForDetails: CloudGame?
    @State private var searchText = ""
    @State private var selectedGenre: String?
    @State private var selectedPlatform: String?
    @State private var selectedStore: String?
    @State private var favoritesOnly = false
    @State private var sortMode: CatalogSortMode = .title
    @State private var showingAddGame = false
    @State private var selectedGameForLauncher: CloudGame?

    var body: some View {
        NavigationStack {
            Group {
                if store.user == nil {
                    OpenNOWUnavailableView("Signed Out", systemImage: "person.crop.circle.badge.exclamationmark")
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                } else {
                    GameCatalogGridView(
                        games: filteredGames,
                        isLoading: store.visibleLibraryGames.isEmpty && store.isLoadingGames,
                        emptyTitle: emptyState.title,
                        emptySystemImage: emptyState.symbol,
                        emptyDescription: emptyState.detail,
                        subtitle: { gameCatalogSubtitle(for: $0) },
                        badgeSystemImage: { _ in nil },
                        onOpenDetails: { selectedGameForDetails = $0 },
                        onPlay: launchFromCard
                    ) {
                        libraryHeader
                    } emptyActions: {
                        emptyStateActions
                    }
                }
            }
            .navigationTitle("Library")
            .toolbar {
                ToolbarItem(placement: .primaryAction) {
                    Button { showingAddGame = true } label: { Image(systemName: "plus") }
                        .accessibilityLabel("Add Game")
                        .disabled(store.user == nil)
                }
            }
            .sheet(isPresented: $showingAddGame) { AddLibraryGameSheet().environmentObject(store) }
            .searchable(text: $searchText, placement: .navigationBarDrawer(displayMode: .automatic), prompt: "Search library")
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

    // MARK: - Empty states
    //
    // "No games" is not one state. It has three completely different causes and three completely
    // different fixes, and naming the wrong one is why people file "my library is broken" when
    // they simply have not linked Steam yet.

    private enum LibraryEmptyCause {
        case noStoresLinked
        case syncing
        case filteredOut
    }

    private var emptyCause: LibraryEmptyCause {
        if hasActiveFilters { return .filteredOut }
        if store.accountConnectors.contains(where: \.isLinked) { return .syncing }
        return .noStoresLinked
    }

    private var emptyState: (title: String, symbol: String, detail: String) {
        switch emptyCause {
        case .noStoresLinked:
            return (
                "Your library is empty",
                "link.badge.plus",
                "Link a game store and the games you already own show up here automatically."
            )
        case .syncing:
            let names = store.accountConnectors.filter(\.isLinked).map(\.label)
            let subject = names.isEmpty ? "your stores" : ListFormatter.localizedString(byJoining: names)
            return (
                "Nothing synced yet",
                "arrow.triangle.2.circlepath",
                "GeForce NOW has not returned any owned games from \(subject) yet. This usually settles within a minute."
            )
        case .filteredOut:
            let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
            if !query.isEmpty {
                return (
                    "No matches for “\(query)”",
                    "magnifyingglass",
                    "Nothing in your library matches. The full catalog might still have it."
                )
            }
            return (
                "No games match these filters",
                "line.3.horizontal.decrease.circle",
                "Try removing one of the filters above."
            )
        }
    }

    @ViewBuilder
    private var emptyStateActions: some View {
        switch emptyCause {
        case .noStoresLinked:
            Button("Link a Game Store") {
                store.pendingSettingsRoute = .account
            }
            .buttonStyle(.borderedProminent)
        case .syncing:
            Button("Check Again") {
                Task { await store.refreshAccountConnectors() }
            }
            .buttonStyle(.bordered)
        case .filteredOut:
            Button("Clear Filters") { clearFilters() }
                .buttonStyle(.borderedProminent)
        }
    }

    private var libraryHeader: some View {
        VStack(alignment: .leading, spacing: 12) {
            if let active = store.activeSession {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Current Session")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.secondary)

                    GameBannerRowView(
                        games: [active.game],
                        subtitle: { _ in active.status == 3 ? "Streaming" : "Queued" },
                        badgeSystemImage: { _ in active.status == 3 ? "play.circle.fill" : "hourglass" }
                    ) { _ in
                        store.jumpBackToSession()
                    }
                }
            }

            CatalogControlsHeader(
                title: libraryCountTitle,
                subtitle: "Synced library",
                chips: activeFilterChips,
                onClear: hasActiveFilters ? clearFilters : nil
            ) {
                HStack(spacing: 8) {
                    filterMenu
                    sortMenu
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

            Picker("Launcher", selection: binding(for: $selectedStore)) {
                Text("Any Launcher").tag("")
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

    private var filteredGames: [CloudGame] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        let filtered = store.visibleLibraryGames.filter { game in
            let matchesQuery = gameMatchesCatalogSearch(game, query: query)
            let matchesGenre = selectedGenre == nil || game.genre == selectedGenre
            let matchesPlatform = selectedPlatform == nil || game.platform == selectedPlatform
            let matchesStore = selectedStore.map { gameResolvedStores(game: game).contains($0) } ?? true
            let matchesFavorite = !favoritesOnly || store.isFavorite(game)
            return matchesQuery && matchesGenre && matchesPlatform && matchesStore && matchesFavorite
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
        Array(Set(store.visibleLibraryGames.map(\.genre).filter { !$0.isEmpty })).sorted()
    }

    private var platforms: [String] {
        Array(Set(store.visibleLibraryGames.map(\.platform).filter { !$0.isEmpty })).sorted()
    }

    private var stores: [String] {
        Array(Set(store.visibleLibraryGames.flatMap { gameResolvedStores(game: $0) })).sorted()
    }

    private var libraryCountTitle: String {
        if filteredGames.count == store.visibleLibraryGames.count {
            return store.visibleLibraryGames.count == 1 ? "1 Game" : "\(store.visibleLibraryGames.count) Games"
        }
        return "\(filteredGames.count) / \(store.visibleLibraryGames.count) Games"
    }

    private var activeFilterChips: [CatalogFilterChip] {
        var chips: [CatalogFilterChip] = []
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        if !query.isEmpty {
            chips.append(CatalogFilterChip(label: "Search: \(query)") {
                searchText = ""
            })
        }
        if favoritesOnly {
            chips.append(CatalogFilterChip(label: "Favorites") {
                favoritesOnly = false
            })
        }
        if let selectedStore {
            chips.append(CatalogFilterChip(label: storeDisplayName(selectedStore)) {
                self.selectedStore = nil
            })
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
        if sortMode != .title {
            chips.append(CatalogFilterChip(label: "Sort: \(sortMode.title)") {
                sortMode = .title
            })
        }
        return chips
    }

    private var hasActiveFilters: Bool {
        !searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
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
        searchText = ""
        selectedGenre = nil
        selectedPlatform = nil
        selectedStore = nil
        favoritesOnly = false
        sortMode = .title
    }

    private func launchFromCard(_ game: CloudGame) {
        let options = store.launchOptions(for: game)
        if options.count > 1 {
            selectedGameForLauncher = game
            return
        }
        pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: store.defaultLaunchOption(for: game) ?? options.first)
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
                                Button(role: .destructive) { store.removeImportedGame(game) } label: {
                                    Image(systemName: "minus.circle")
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
