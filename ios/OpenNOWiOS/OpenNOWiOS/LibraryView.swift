import SwiftUI

struct LibraryView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var pendingLaunchRequest: GameLaunchRequest?
    @State private var selectedGameForDetails: CloudGame?
    @State private var searchText = ""
    @State private var selectedGenre: String?
    @State private var selectedPlatform: String?
    @State private var sortMode: LibrarySortMode = .title

    var body: some View {
        NavigationStack {
            Group {
                if store.user == nil {
                    signedOutState
                } else if store.libraryGames.isEmpty && !store.isLoadingGames {
                    emptyLibraryState
                } else {
                    libraryContent
                }
            }
            .navigationTitle("Library")
            .navigationBarTitleDisplayMode(.large)
            .searchable(
                text: $searchText,
                placement: .navigationBarDrawer(displayMode: .automatic),
                prompt: "Search your library…"
            )
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Menu {
                        Picker("Sort", selection: $sortMode) {
                            ForEach(LibrarySortMode.allCases) { mode in
                                Label(mode.title, systemImage: mode.icon).tag(mode)
                            }
                        }
                    } label: {
                        Image(systemName: "arrow.up.arrow.down.circle")
                    }
                    .disabled(store.libraryGames.isEmpty && !store.isLoadingGames)
                }
            }
        }
        .presentGameDetailsUIKit(selectedGame: $selectedGameForDetails) { game, option in
            pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: option)
        }
        .printedWasteLaunchSheet(pendingLaunchRequest: $pendingLaunchRequest)
    }

    private var libraryContent: some View {
        ScrollView {
            LazyVStack(alignment: .leading, spacing: 22) {
                libraryHero
                    .padding(.horizontal)
                    .padding(.top, 8)

                if !quickFilters.isEmpty {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 8) {
                            ForEach(quickFilters) { filter in
                                GameFilterChip(label: filter.label, isSelected: filter.isSelected) {
                                    Haptics.selection()
                                    applyQuickFilter(filter)
                                }
                            }
                        }
                    }
                    .padding(.horizontal)
                }

                if !featuredLibraryGames.isEmpty {
                    sectionHeader("Spotlight")
                        .padding(.horizontal)
                    featuredShelf
                }

                VStack(alignment: .leading, spacing: 14) {
                    HStack(alignment: .firstTextBaseline) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(resultsTitle)
                                .font(.title3.bold())
                            Text(resultsSubtitle)
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }
                        Spacer(minLength: 12)
                        if store.isLoadingGames {
                            ProgressView()
                        }
                    }
                    .padding(.horizontal)

                    if store.libraryGames.isEmpty && store.isLoadingGames {
                        skeletonGrid
                    } else if filteredGames.isEmpty {
                        emptySearchState
                            .padding(.horizontal)
                    } else {
                        gameGrid
                    }
                }
            }
            .padding(.bottom, 24)
        }
        .refreshable { await store.refreshCatalog() }
        .background(appBackground)
    }

    private var emptyLibraryState: some View {
        VStack(spacing: 20) {
            Spacer()
            Image(systemName: "books.vertical")
                .font(.system(size: 56))
                .foregroundStyle(.quaternary)
            Text("Your library is empty")
                .font(.title3.bold())
            Text("Games you own on supported stores will appear here.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, 40)
            Spacer()
        }
    }

    private var skeletonGrid: some View {
        let columns = [GridItem(.adaptive(minimum: 150, maximum: 200), spacing: 14)]
        return LazyVGrid(columns: columns, spacing: 14) {
            ForEach(0..<8, id: \.self) { _ in
                GameCardSkeletonView()
            }
        }
        .padding(.horizontal)
    }

    private var gameGrid: some View {
        let columns = [GridItem(.adaptive(minimum: 150, maximum: 200), spacing: 14)]
        return LazyVGrid(columns: columns, spacing: 14) {
            ForEach(filteredGames) { game in
                GameCardView(game: game) {
                    selectedGameForDetails = game
                }
            }
        }
        .padding(.horizontal)
    }

    private var libraryHero: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("My Library")
                        .font(.title.bold())
                    Text("Search, filter, and jump back into the games you already own.")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
                Spacer(minLength: 16)
                if let active = store.activeSession {
                    LibraryStatusBadge(
                        title: active.status == 3 ? "Live" : "Queue",
                        subtitle: active.game.title,
                        tint: active.status == 3 ? .green : .orange
                    )
                }
            }

            HStack(spacing: 12) {
                LibraryStatTile(title: "Games", value: "\(store.libraryGames.count)", icon: "books.vertical.fill")
                LibraryStatTile(title: "Platforms", value: "\(platforms.count)", icon: "shippingbox.fill")
                LibraryStatTile(title: "Genres", value: "\(genres.count)", icon: "sparkles.tv.fill")
            }

            if let active = store.activeSession {
                Button {
                    Haptics.light()
                    if store.canReopenStreamer {
                        store.reopenStreamer()
                    } else {
                        store.maximizeQueueOverlay()
                    }
                } label: {
                    HStack(spacing: 14) {
                        GameArtworkView(game: active.game, iconSize: 28)
                            .frame(width: 60, height: 60)
                            .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))

                        VStack(alignment: .leading, spacing: 4) {
                            Text("Continue \(active.game.title)")
                                .font(.headline)
                                .lineLimit(1)
                            Text(active.status == 3 ? "Return to your current stream" : "Open queue and session status")
                                .font(.footnote)
                                .foregroundStyle(.secondary)
                        }

                        Spacer(minLength: 12)

                        Image(systemName: "play.fill")
                            .font(.headline.weight(.semibold))
                            .foregroundStyle(brandAccent)
                    }
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .glassCard()
                }
                .buttonStyle(.plain)
            }
        }
    }

    private var featuredShelf: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 14) {
                ForEach(featuredLibraryGames) { game in
                    FeaturedGameCard(game: game) {
                        selectedGameForDetails = game
                    }
                }
            }
            .padding(.horizontal)
        }
    }

    private var emptySearchState: some View {
        VStack(spacing: 14) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 34, weight: .semibold))
                .foregroundStyle(.tertiary)
            Text("No matches found")
                .font(.headline)
            Text("Try a different search, platform, or genre filter.")
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)

            Button("Clear Filters") {
                Haptics.light()
                clearFilters()
            }
            .buttonStyle(.bordered)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 32)
        .glassCard()
    }

    private var signedOutState: some View {
        VStack(spacing: 20) {
            Spacer()
            Image(systemName: "person.crop.circle.badge.questionmark")
                .font(.system(size: 56))
                .foregroundStyle(.quaternary)
            Text("Sign in to see your library")
                .font(.title3.bold())
            Spacer()
        }
    }

    private var genres: [String] {
        Array(Set(store.libraryGames.map(\.genre))).sorted()
    }

    private var platforms: [String] {
        Array(Set(store.libraryGames.map(\.platform))).sorted()
    }

    private var filteredGames: [CloudGame] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        let filtered = store.libraryGames.filter { game in
            let matchesSearch: Bool
            if query.isEmpty {
                matchesSearch = true
            } else {
                matchesSearch =
                    game.title.localizedCaseInsensitiveContains(query) ||
                    game.genre.localizedCaseInsensitiveContains(query) ||
                    game.platform.localizedCaseInsensitiveContains(query) ||
                    (game.publisher?.localizedCaseInsensitiveContains(query) ?? false) ||
                    (game.developer?.localizedCaseInsensitiveContains(query) ?? false) ||
                    (game.tags?.contains(where: { $0.localizedCaseInsensitiveContains(query) }) ?? false) ||
                    (game.stores?.contains(where: { $0.localizedCaseInsensitiveContains(query) }) ?? false)
            }

            let matchesGenre = selectedGenre == nil || game.genre == selectedGenre
            let matchesPlatform = selectedPlatform == nil || game.platform == selectedPlatform
            return matchesSearch && matchesGenre && matchesPlatform
        }

        switch sortMode {
        case .title:
            return filtered.sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
        case .genre:
            return filtered.sorted {
                if $0.genre == $1.genre {
                    return $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
                }
                return $0.genre.localizedCaseInsensitiveCompare($1.genre) == .orderedAscending
            }
        case .platform:
            return filtered.sorted {
                if $0.platform == $1.platform {
                    return $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending
                }
                return $0.platform.localizedCaseInsensitiveCompare($1.platform) == .orderedAscending
            }
        }
    }

    private var featuredLibraryGames: [CloudGame] {
        let featuredIds = Set(store.featuredGames.map(\.id))
        let spotlight = store.libraryGames.filter { featuredIds.contains($0.id) }
        if !spotlight.isEmpty {
            return Array(spotlight.prefix(8))
        }
        return Array(filteredGames.prefix(8))
    }

    private var resultsTitle: String {
        if searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && selectedGenre == nil && selectedPlatform == nil {
            return "All Games"
        }
        return "Filtered Results"
    }

    private var resultsSubtitle: String {
        let count = filteredGames.count
        let total = store.libraryGames.count
        if count == total && searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && selectedGenre == nil && selectedPlatform == nil {
            return "\(total) games ready to launch"
        }
        return "\(count) of \(total) games"
    }

    private var quickFilters: [LibraryQuickFilter] {
        var filters: [LibraryQuickFilter] = [
            .init(label: "All", genre: nil, platform: nil, isSelected: selectedGenre == nil && selectedPlatform == nil)
        ]

        filters.append(contentsOf: platforms.prefix(4).map { platform in
            LibraryQuickFilter(
                label: platform,
                genre: nil,
                platform: platform,
                isSelected: selectedPlatform == platform && selectedGenre == nil
            )
        })

        filters.append(contentsOf: genres.prefix(4).map { genre in
            LibraryQuickFilter(
                label: genre,
                genre: genre,
                platform: nil,
                isSelected: selectedGenre == genre && selectedPlatform == nil
            )
        })

        return filters
    }

    private func applyQuickFilter(_ filter: LibraryQuickFilter) {
        selectedGenre = filter.genre
        selectedPlatform = filter.platform
    }

    private func clearFilters() {
        searchText = ""
        selectedGenre = nil
        selectedPlatform = nil
        sortMode = .title
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title)
            .font(.title3.bold())
    }
}

private enum LibrarySortMode: String, CaseIterable, Identifiable {
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
        case .platform: return "shippingbox"
        }
    }
}

private struct LibraryQuickFilter: Identifiable {
    let label: String
    let genre: String?
    let platform: String?
    let isSelected: Bool

    var id: String { "\(label)|\(genre ?? "-")|\(platform ?? "-")" }
}

private struct LibraryStatTile: View {
    let title: String
    let value: String
    let icon: String

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Image(systemName: icon)
                .font(.headline)
                .foregroundStyle(brandAccent)
            Text(value)
                .font(.title2.bold())
            Text(title)
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .glassCard()
    }
}

private struct LibraryStatusBadge: View {
    let title: String
    let subtitle: String
    let tint: Color

    var body: some View {
        VStack(alignment: .trailing, spacing: 4) {
            HStack(spacing: 6) {
                Circle()
                    .fill(tint)
                    .frame(width: 8, height: 8)
                Text(title)
                    .font(.caption.weight(.semibold))
            }
            Text(subtitle)
                .font(.caption2)
                .foregroundStyle(.secondary)
                .lineLimit(1)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .glassCard()
    }
}
