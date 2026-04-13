import SwiftUI

struct HomeView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var pendingLaunchGame: CloudGame?

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
        .printedWasteLaunchSheet(pendingGame: $pendingLaunchGame)
    }

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

    private var featuredSection: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 14) {
                if store.featuredGames.isEmpty && store.isLoadingGames {
                    ForEach(0..<6, id: \.self) { _ in
                        FeaturedGameCardSkeleton()
                    }
                } else {
                    ForEach(store.featuredGames.prefix(8)) { game in
                        FeaturedGameCard(game: game) {
                            pendingLaunchGame = game
                        }
                    }
                }
            }
            .padding(.horizontal)
        }
    }

    private func gameGrid(games: [CloudGame]) -> some View {
        let columns = [GridItem(.adaptive(minimum: 150, maximum: 200), spacing: 14)]
        return LazyVGrid(columns: columns, spacing: 14) {
            if games.isEmpty && store.isLoadingGames {
                ForEach(0..<8, id: \.self) { _ in
                    GameCardSkeletonView()
                }
            } else {
                ForEach(games) { game in
                    GameCardView(game: game) {
                        pendingLaunchGame = game
                    }
                }
            }
        }
    }

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

    private func sectionHeader(_ title: String) -> some View {
        Text(title)
            .font(.title3.bold())
            .padding(.horizontal)
    }
}

private struct FeaturedGameCard: View {
    let game: CloudGame
    let onLaunch: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            GameArtworkView(game: game, iconSize: 48)
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

private struct FeaturedGameCardSkeleton: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            RoundedRectangle(cornerRadius: 14)
                .fill(.quaternary.opacity(0.4))
                .frame(width: 160, height: 100)
                .shimmeringSkeleton()
            VStack(alignment: .leading, spacing: 8) {
                RoundedRectangle(cornerRadius: 5)
                    .fill(.quaternary.opacity(0.4))
                    .frame(height: 12)
                RoundedRectangle(cornerRadius: 4)
                    .fill(.quaternary.opacity(0.3))
                    .frame(width: 70, height: 10)
                RoundedRectangle(cornerRadius: 7)
                    .fill(.quaternary.opacity(0.35))
                    .frame(height: 28)
            }
            .padding(10)
        }
        .frame(width: 160)
        .glassCard()
    }
}

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

struct GameCardView: View {
    let game: CloudGame
    let onLaunch: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            GameArtworkView(game: game, iconSize: 32)
            .frame(maxWidth: .infinity)
            .frame(height: 112)
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

struct GameCardSkeletonView: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            RoundedRectangle(cornerRadius: 10)
                .fill(.quaternary.opacity(0.35))
                .aspectRatio(4/3, contentMode: .fit)
                .shimmeringSkeleton()
            RoundedRectangle(cornerRadius: 5)
                .fill(.quaternary.opacity(0.4))
                .frame(height: 12)
            RoundedRectangle(cornerRadius: 4)
                .fill(.quaternary.opacity(0.3))
                .frame(width: 100, height: 10)
            RoundedRectangle(cornerRadius: 7)
                .fill(.quaternary.opacity(0.35))
                .frame(height: 30)
        }
        .padding(10)
        .glassCard()
    }
}

private struct GameArtworkView: View {
    let game: CloudGame
    let iconSize: CGFloat

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                gameColor(for: game.title).opacity(0.2)
                if let imageUrl = game.imageUrl, let url = URL(string: imageUrl) {
                    AsyncImage(url: url) { phase in
                        switch phase {
                        case .success(let image):
                            image
                                .resizable()
                                .scaledToFill()
                                .frame(width: proxy.size.width, height: proxy.size.height)
                        case .empty:
                            Rectangle()
                                .fill(.quaternary.opacity(0.25))
                                .frame(width: proxy.size.width, height: proxy.size.height)
                                .shimmeringSkeleton()
                        default:
                            iconFallback
                                .frame(width: proxy.size.width, height: proxy.size.height)
                        }
                    }
                } else {
                    iconFallback
                        .frame(width: proxy.size.width, height: proxy.size.height)
                }
            }
            .frame(width: proxy.size.width, height: proxy.size.height)
            .clipped()
        }
    }

    private var iconFallback: some View {
        Image(systemName: game.icon)
            .font(.system(size: iconSize))
            .foregroundStyle(gameColor(for: game.title))
    }
}

private struct SkeletonShimmerModifier: ViewModifier {
    @State private var phase: CGFloat = -0.6

    func body(content: Content) -> some View {
        content
            .overlay(
                LinearGradient(
                    colors: [
                        .clear,
                        Color.white.opacity(0.25),
                        .clear
                    ],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
                .rotationEffect(.degrees(12))
                .offset(x: phase * 220)
                .blendMode(.screen)
            )
            .mask(content)
            .onAppear {
                withAnimation(.linear(duration: 1.1).repeatForever(autoreverses: false)) {
                    phase = 1.2
                }
            }
    }
}

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

    func shimmeringSkeleton() -> some View {
        modifier(SkeletonShimmerModifier())
    }
}
