import SwiftUI

struct LibraryView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var pendingLaunchGame: CloudGame?

    var body: some View {
        NavigationStack {
            Group {
                if store.user == nil {
                    signedOutState
                } else if store.libraryGames.isEmpty && !store.isLoadingGames {
                    emptyLibraryState
                } else {
                    gameGrid
                }
            }
            .navigationTitle("Library")
        }
        .printedWasteLaunchSheet(pendingGame: $pendingLaunchGame)
    }

    private var gameGrid: some View {
        ScrollView {
            let columns = [GridItem(.adaptive(minimum: 150, maximum: 200), spacing: 14)]
            LazyVGrid(columns: columns, spacing: 14) {
                if store.libraryGames.isEmpty && store.isLoadingGames {
                    ForEach(0..<8, id: \.self) { _ in
                        GameCardSkeletonView()
                    }
                } else {
                    ForEach(store.libraryGames) { game in
                        GameCardView(game: game) {
                            pendingLaunchGame = game
                        }
                    }
                }
            }
            .padding(.horizontal)
            .padding(.vertical)
        }
        .refreshable { await store.refreshCatalog() }
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
}
