import SwiftUI

struct HomeView: View {
    @EnvironmentObject private var store: OpenNOWStore

    var body: some View {
        NavigationStack {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 24) {
                    if let user = store.user {
                        accountCard(user: user)
                            .padding(.horizontal)
                    }

                    if let error = store.lastError {
                        ErrorBannerView(message: error)
                            .padding(.horizontal)
                    }

                    if !store.featuredGames.isEmpty || store.isLoadingGames {
                        sectionHeader("Featured")
                        featuredSection
                    }

                    if !store.allGames.isEmpty {
                        sectionHeader("All Games (\(store.allGames.count))")
                        gameGrid(games: store.allGames)
                            .padding(.horizontal)
                    } else if store.isLoadingGames {
                        loadingPlaceholder
                    }

                    Spacer(minLength: 20)
                }
                .padding(.top, 8)
            }
            .refreshable { await store.refreshCatalog() }
            .navigationTitle("OpenNOW")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    if store.isLoadingGames {
                        ProgressView()
                    }
                }
            }
        }
    }

    // MARK: - Account Card

    @ViewBuilder
    private func accountCard(user: UserProfile) -> some View {
        HStack(spacing: 16) {
            ZStack {
                Circle()
                    .fill(brandGradient)
                    .frame(width: 44, height: 44)
                Text(String(user.displayName.prefix(1)).uppercased())
                    .font(.headline.bold())
                    .foregroundStyle(.white)
            }

            VStack(alignment: .leading, spacing: 2) {
                Text(user.displayName)
                    .font(.headline)
                    .lineLimit(1)
                if let tier = store.subscription?.membershipTier {
                    Text(tier)
                        .font(.caption)
                        .foregroundStyle(brandAccent)
                        .fontWeight(.semibold)
                }
            }

            Spacer()

            if let sub = store.subscription, !sub.isUnlimited {
                VStack(alignment: .trailing, spacing: 2) {
                    Text(String(format: "%.1f h", sub.remainingHours))
                        .font(.subheadline.monospacedDigit().bold())
                    Text("remaining")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            } else if store.subscription?.isUnlimited == true {
                Label("Unlimited", systemImage: "infinity")
                    .font(.caption.bold())
                    .foregroundStyle(brandAccent)
            }
        }
        .padding(16)
        .glassCard()
    }

    // MARK: - Featured Section (horizontal scroll)

    private var featuredSection: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 14) {
                ForEach(store.featuredGames.prefix(8)) { game in
                    FeaturedGameCard(game: game) {
                        Task { await store.launch(game: game) }
                    }
                }
            }
            .padding(.horizontal)
        }
    }

    // MARK: - Game Grid

    private func gameGrid(games: [CloudGame]) -> some View {
        let columns = [GridItem(.adaptive(minimum: 150, maximum: 200), spacing: 14)]
        return LazyVGrid(columns: columns, spacing: 14) {
            ForEach(games) { game in
                GameCardView(game: game) {
                    Task { await store.launch(game: game) }
                }
            }
        }
    }

    // MARK: - Loading Placeholder

    private var loadingPlaceholder: some View {
        HStack {
            Spacer()
            VStack(spacing: 12) {
                ProgressView()
                Text("Loading games…")
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(.vertical, 60)
    }

    // MARK: - Section Header

    private func sectionHeader(_ title: String) -> some View {
        Text(title)
            .font(.title3.bold())
            .padding(.horizontal)
    }
}

// MARK: - Featured Game Card (larger, horizontal scroll card)

private struct FeaturedGameCard: View {
    let game: CloudGame
    let onLaunch: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ZStack {
                gameColor(for: game.title).opacity(0.25)
                Image(systemName: game.icon)
                    .font(.system(size: 48))
                    .foregroundStyle(gameColor(for: game.title))
            }
            .frame(width: 160, height: 100)
            .clipShape(UnevenRoundedRectangle(topLeadingRadius: 14, bottomLeadingRadius: 0, bottomTrailingRadius: 0, topTrailingRadius: 14))

            VStack(alignment: .leading, spacing: 6) {
                Text(game.title)
                    .font(.caption.bold())
                    .lineLimit(2)
                Text(game.platform)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Button(action: onLaunch) {
                    Label("Play", systemImage: "play.fill")
                        .font(.caption2.bold())
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 7)
                }
                .buttonStyle(.bordered)
                .tint(brandAccent)
                .disabled(game.launchAppId == nil)
            }
            .padding(10)
        }
        .frame(width: 160)
        .glassCard()
    }
}

// MARK: - Error Banner

struct ErrorBannerView: View {
    let message: String

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: "exclamationmark.triangle.fill")
                .foregroundStyle(.orange)
            Text(message)
                .font(.footnote)
                .foregroundStyle(.primary)
            Spacer()
        }
        .padding(12)
        .background(.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 10))
    }
}

// MARK: - Shared Game Card

struct GameCardView: View {
    let game: CloudGame
    let onLaunch: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            ZStack {
                gameColor(for: game.title).opacity(0.2)
                Image(systemName: game.icon)
                    .font(.system(size: 32))
                    .foregroundStyle(gameColor(for: game.title))
            }
            .aspectRatio(4/3, contentMode: .fit)
            .clipShape(RoundedRectangle(cornerRadius: 10))

            VStack(alignment: .leading, spacing: 3) {
                Text(game.title)
                    .font(.caption.bold())
                    .lineLimit(2)
                Text("\(game.genre) · \(game.platform)")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }

            Button(action: onLaunch) {
                HStack(spacing: 4) {
                    Image(systemName: "play.fill")
                    Text("Play")
                }
                .font(.caption.bold())
                .frame(maxWidth: .infinity)
                .padding(.vertical, 7)
            }
            .buttonStyle(.bordered)
            .tint(game.launchAppId != nil ? brandAccent : .secondary)
            .disabled(game.launchAppId == nil)
        }
        .padding(10)
        .glassCard()
    }
}

// MARK: - Color helpers

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

// MARK: - GlassCard ViewModifier

struct GlassCardModifier: ViewModifier {
    func body(content: Content) -> some View {
        if #available(iOS 26, *) {
            content
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
                .glassEffect(in: RoundedRectangle(cornerRadius: 16))
        } else {
            content
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 16))
                .shadow(color: .black.opacity(0.08), radius: 4, y: 2)
        }
    }
}

extension View {
    func glassCard() -> some View {
        modifier(GlassCardModifier())
    }
}
