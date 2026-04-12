import SwiftUI

struct BrowseView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var selectedGenre: String? = nil

    private var genres: [String] {
        Array(Set(store.allGames.map { $0.genre })).sorted()
    }

    private var filtered: [CloudGame] {
        let bySearch = store.filteredCatalogGames
        guard let genre = selectedGenre else { return bySearch }
        return bySearch.filter { $0.genre == genre }
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                if !genres.isEmpty {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 8) {
                            FilterChip(label: "All", isSelected: selectedGenre == nil) {
                                selectedGenre = nil
                            }
                            ForEach(genres, id: \.self) { genre in
                                FilterChip(label: genre, isSelected: selectedGenre == genre) {
                                    selectedGenre = (selectedGenre == genre) ? nil : genre
                                }
                            }
                        }
                        .padding(.horizontal)
                        .padding(.vertical, 10)
                    }
                }

                if filtered.isEmpty {
                    if store.isLoadingGames {
                        ScrollView {
                            let columns = [GridItem(.adaptive(minimum: 150, maximum: 200), spacing: 14)]
                            LazyVGrid(columns: columns, spacing: 14) {
                                ForEach(0..<8, id: \.self) { _ in
                                    GameCardSkeletonView()
                                }
                            }
                            .padding(.horizontal)
                            .padding(.bottom)
                        }
                    } else {
                        emptyState
                    }
                } else {
                    ScrollView {
                        let columns = [GridItem(.adaptive(minimum: 150, maximum: 200), spacing: 14)]
                        LazyVGrid(columns: columns, spacing: 14) {
                            ForEach(filtered) { game in
                                GameCardView(game: game) {
                                    Task { await store.launch(game: game) }
                                }
                            }
                        }
                        .padding(.horizontal)
                        .padding(.bottom)
                    }
                    .refreshable { await store.refreshCatalog() }
                }
            }
            .navigationTitle("Browse")
            .searchable(
                text: $store.searchText,
                placement: .navigationBarDrawer(displayMode: .automatic),
                prompt: "Search games…"
            )
        }
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
            if !store.searchText.isEmpty {
                Button("Clear Search") { store.searchText = "" }
                    .buttonStyle(.bordered)
            }
            Spacer()
        }
    }
}

// MARK: - Filter Chip

private struct FilterChip: View {
    let label: String
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            Text(label)
                .font(.caption.weight(.semibold))
                .padding(.horizontal, 14)
                .padding(.vertical, 7)
                .foregroundStyle(isSelected ? .white : .primary)
                .background(isSelected ? brandAccent : Color(.systemFill), in: Capsule())
        }
        .buttonStyle(.plain)
        .animation(.easeOut(duration: 0.15), value: isSelected)
    }
}
