import SwiftUI

struct BrowseView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var selectedGenre: String? = nil
    @State private var selectedPlatform: String?
    @State private var selectedStore: String?
    @State private var sortMode: CatalogSortMode = .title
    @State private var pendingLaunchRequest: GameLaunchRequest?
    @State private var selectedGameForDetails: CloudGame?
    private let gridColumns = [GridItem(.adaptive(minimum: 150, maximum: 200), spacing: 14)]

    private var genres: [String] {
        Array(Set(store.allGames.map { $0.genre })).sorted()
    }

    private var platforms: [String] {
        Array(Set(store.allGames.map(\.platform))).sorted()
    }

    private var stores: [String] {
        Array(Set(store.allGames.flatMap { gameResolvedStores(game: $0) })).sorted()
    }

    private var filtered: [CloudGame] {
        let filtered = store.filteredCatalogGames.filter { game in
            let matchesGenre = selectedGenre == nil || game.genre == selectedGenre
            let matchesPlatform = selectedPlatform == nil || game.platform == selectedPlatform
            let matchesStore = selectedStore.map { gameResolvedStores(game: game).contains($0) } ?? true
            return matchesGenre && matchesPlatform && matchesStore
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

    private var hasActiveFilters: Bool {
        !store.searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ||
            selectedGenre != nil ||
            selectedPlatform != nil ||
            selectedStore != nil ||
            sortMode != .title
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 16) {
                    browseControls

                    if store.isLoadingGames {
                        skeletonGridContent
                    } else if filtered.isEmpty {
                        emptyState
                            .frame(minHeight: 360)
                    } else {
                        gameGridContent
                    }
                }
                .padding(.top, 8)
                .padding(.bottom)
            }
            .refreshable { await store.refreshCatalog() }
            .navigationTitle("Browse")
        }
        .presentGameDetailsUIKit(selectedGame: $selectedGameForDetails) { game, option in
            pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: option)
        }
        .printedWasteLaunchSheet(pendingLaunchRequest: $pendingLaunchRequest)
    }

    private var browseControls: some View {
        VStack(alignment: .leading, spacing: 12) {
            GameSearchField(text: $store.searchText, prompt: "Search games")
                .padding(.horizontal)

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 10) {
                    CatalogFilterMenu(
                        title: "Platform",
                        value: selectedPlatform ?? "Any",
                        icon: "gamecontroller",
                        values: platforms,
                        selection: $selectedPlatform
                    )

                    CatalogFilterMenu(
                        title: "Genre",
                        value: selectedGenre ?? "Any",
                        icon: "sparkles.tv",
                        values: genres,
                        selection: $selectedGenre
                    )

                    CatalogFilterMenu(
                        title: "Store",
                        value: selectedStore.map(storeDisplayName) ?? "Any",
                        icon: "shippingbox",
                        values: stores,
                        displayName: storeDisplayName,
                        selection: $selectedStore
                    )

                    Menu {
                        Picker("Sort", selection: $sortMode) {
                            ForEach(CatalogSortMode.allCases) { mode in
                                Label(mode.title, systemImage: mode.icon).tag(mode)
                            }
                        }
                    } label: {
                        Label(sortMode.title, systemImage: sortMode.icon)
                            .lineLimit(1)
                    }
                    .buttonStyle(.bordered)

                    if hasActiveFilters {
                        Button {
                            Haptics.light()
                            clearFilters()
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                        }
                        .buttonStyle(.bordered)
                        .accessibilityLabel("Clear filters")
                    }
                }
                .font(.caption.weight(.semibold))
                .padding(.horizontal)
            }
        }
    }

    private func clearFilters() {
        store.searchText = ""
        selectedGenre = nil
        selectedPlatform = nil
        selectedStore = nil
        sortMode = .title
    }

    private var gameGridContent: some View {
        LazyVGrid(columns: gridColumns, spacing: 14) {
            ForEach(filtered) { game in
                GameCardView(game: game) {
                    selectedGameForDetails = game
                }
            }
        }
        .padding(.horizontal)
    }

    private var skeletonGridContent: some View {
        LazyVGrid(columns: gridColumns, spacing: 14) {
            ForEach(0..<8, id: \.self) { _ in
                GameCardSkeletonView()
            }
        }
        .padding(.horizontal)
    }

    private var emptyState: some View {
        VStack(spacing: 16) {
            Spacer()
            Image(systemName: store.searchText.isEmpty ? "square.grid.2x2" : "magnifyingglass")
                .font(.system(size: 44))
                .foregroundStyle(.quaternary)
            Text(store.searchText.isEmpty ? "No games available" : "No results for \"\(store.searchText)\"")
                .font(.headline)
                .foregroundStyle(.secondary)
            if hasActiveFilters {
                Button("Clear Filters") {
                    Haptics.light()
                    clearFilters()
                }
                    .buttonStyle(.bordered)
            }
            Spacer()
        }
    }
}

struct GameSearchField: View {
    @Binding var text: String
    let prompt: String

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            TextField(prompt, text: $text)
                .textInputAutocapitalization(.never)
                .disableAutocorrection(true)
                .submitLabel(.search)
            if !text.isEmpty {
                Button {
                    Haptics.light()
                    text = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
            }
        }
        .font(.subheadline)
        .padding(.horizontal, 14)
        .padding(.vertical, 11)
        .glassCard(cornerRadius: 14)
        .overlay(
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(Color.white.opacity(0.10), lineWidth: 1)
        )
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

struct CatalogFilterMenu: View {
    let title: String
    let value: String
    let icon: String
    let values: [String]
    var displayName: (String) -> String = { $0 }
    @Binding var selection: String?

    var body: some View {
        Menu {
            Button("Any \(title)") {
                Haptics.selection()
                selection = nil
            }
            ForEach(values, id: \.self) { option in
                Button {
                    Haptics.selection()
                    selection = option
                } label: {
                    if selection == option {
                        Label(displayName(option), systemImage: "checkmark")
                    } else {
                        Text(displayName(option))
                    }
                }
            }
        } label: {
            Label(value, systemImage: icon)
                .lineLimit(1)
        }
        .buttonStyle(.bordered)
    }
}

struct GameFilterChip: View {
    let label: String
    let isSelected: Bool
    let action: () -> Void

    private var unselectedBackground: Color {
        #if os(tvOS)
        return Color.white.opacity(0.12)
        #else
        return Color(.systemFill)
        #endif
    }

    var body: some View {
        Button(action: {
            Haptics.selection()
            action()
        }) {
            Text(label)
                .font(.caption.weight(.semibold))
                .padding(.horizontal, 14)
                .padding(.vertical, 7)
                .foregroundStyle(isSelected ? .white : .primary)
                .background(isSelected ? brandAccent : unselectedBackground, in: Capsule())
        }
        .buttonStyle(.plain)
        .animation(.easeOut(duration: 0.15), value: isSelected)
    }
}
