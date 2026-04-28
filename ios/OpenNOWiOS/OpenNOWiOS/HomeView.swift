import SwiftUI
import UIKit

final class OpenNOWImageCache {
    static let shared = OpenNOWImageCache()

    private let cache = NSCache<NSURL, UIImage>()

    private init() {
        cache.countLimit = 240
        cache.totalCostLimit = 96 * 1024 * 1024
    }

    static func configureURLCache() {
        URLCache.shared = URLCache(
            memoryCapacity: 64 * 1024 * 1024,
            diskCapacity: 256 * 1024 * 1024,
            diskPath: "OpenNOWURLCache"
        )
    }

    func image(for url: URL) -> UIImage? {
        cache.object(forKey: url as NSURL)
    }

    func insert(_ image: UIImage, for url: URL, cost: Int) {
        cache.setObject(image, forKey: url as NSURL, cost: cost)
    }

    func removeAll() {
        cache.removeAllObjects()
    }
}

@MainActor
private final class CachedRemoteImageLoader: ObservableObject {
    @Published private(set) var image: UIImage?
    @Published private(set) var didFail = false

    private var loadedURL: URL?

    func load(_ url: URL) async {
        if loadedURL == url && (image != nil || didFail) { return }

        loadedURL = url
        didFail = false

        if let cached = OpenNOWImageCache.shared.image(for: url) {
            image = cached
            return
        }

        image = nil

        do {
            var request = URLRequest(url: url)
            request.cachePolicy = .returnCacheDataElseLoad
            request.timeoutInterval = 15

            let (data, response) = try await URLSession.shared.data(for: request)
            if let http = response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
                didFail = true
                return
            }

            guard let decoded = UIImage(data: data) else {
                didFail = true
                return
            }

            OpenNOWImageCache.shared.insert(decoded, for: url, cost: data.count)
            image = decoded
        } catch {
            didFail = true
        }
    }
}

struct CachedRemoteImage<Content: View, Placeholder: View, Failure: View>: View {
    let url: URL
    let content: (Image) -> Content
    let placeholder: () -> Placeholder
    let failure: () -> Failure

    @StateObject private var loader = CachedRemoteImageLoader()

    var body: some View {
        Group {
            if let image = loader.image {
                content(Image(uiImage: image))
            } else if loader.didFail {
                failure()
            } else {
                placeholder()
            }
        }
        .task(id: url) {
            await loader.load(url)
        }
    }
}

struct HomeView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var pendingLaunchRequest: GameLaunchRequest?
    @State private var selectedGameForDetails: CloudGame?

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

                    if store.user != nil, jumpBackInHasContent {
                        sectionHeader("Jump back in")
                        jumpBackInSection
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
        .presentGameDetailsUIKit(selectedGame: $selectedGameForDetails) { game, option in
            pendingLaunchRequest = GameLaunchRequest(game: game, launchOption: option)
        }
        .printedWasteLaunchSheet(pendingLaunchRequest: $pendingLaunchRequest)
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
                    .foregroundStyle(Color.white)
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
                            selectedGameForDetails = game
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
                        selectedGameForDetails = game
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

    private var jumpBackInHasContent: Bool {
        store.activeSession != nil || !store.resumableSessions.isEmpty
    }

    private var resumableSessionsExcludingActive: [RemoteSessionCandidate] {
        let activeId = store.activeSession?.id
        return store.resumableSessions.filter { $0.id != activeId }
    }

    private func jumpBackInSubtitleActive(_ session: ActiveSession) -> String {
        switch session.status {
        case 3:
            guard store.supportsEmbeddedStreamer else { return "Ready, but tvOS can't stream yet" }
            return store.streamSession == nil ? "Tap to return" : "Streaming"
        case 2:
            return "Connecting"
        default:
            if let queue = session.queuePosition {
                return queue == 1 ? "Next in queue" : "Queue #\(queue)"
            }
            return "In queue"
        }
    }

    private func jumpBackInTintActive(_ session: ActiveSession) -> Color {
        switch session.status {
        case 3: return .green
        case 2: return Color(red: 0.84, green: 0.72, blue: 0.12)
        default: return .orange
        }
    }

    private var jumpBackInSection: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 14) {
                if let active = store.activeSession {
                    JumpBackInCard(
                        title: active.game.title,
                        subtitle: jumpBackInSubtitleActive(active),
                        game: active.game,
                        statusTint: jumpBackInTintActive(active)
                    ) {
                        if store.canReopenStreamer {
                            store.reopenStreamer()
                        } else {
                            store.maximizeQueueOverlay()
                        }
                    }
                }
                ForEach(Array(resumableSessionsExcludingActive.prefix(8))) { candidate in
                    let game = store.gameForRemoteSession(candidate)
                    JumpBackInCard(
                        title: game?.title ?? "Cloud session",
                        subtitle: "Resume",
                        game: game,
                        statusTint: .orange
                    ) {
                        store.scheduleResume(candidate: candidate)
                    }
                }
            }
            .padding(.horizontal)
        }
    }

    private func sectionHeader(_ title: String) -> some View {
        Text(title)
            .font(.title3.bold())
            .padding(.horizontal)
    }
}

private struct JumpBackInCard: View {
    let title: String
    let subtitle: String
    let game: CloudGame?
    let statusTint: Color
    let onTap: () -> Void

    var body: some View {
        Button(action: {
            Haptics.light()
            onTap()
        }) {
            VStack(alignment: .leading, spacing: 0) {
                Group {
                    if let game {
                        GameArtworkView(game: game, iconSize: 48)
                    } else {
                        ZStack {
                            Color.secondary.opacity(0.18)
                            Image(systemName: "arrow.counterclockwise.circle.fill")
                                .font(.system(size: 40))
                                .foregroundStyle(.secondary)
                        }
                    }
                }
                .frame(width: 160, height: 100)
                .clipShape(
                    UnevenRoundedRectangle(
                        topLeadingRadius: 14,
                        bottomLeadingRadius: 0,
                        bottomTrailingRadius: 0,
                        topTrailingRadius: 14
                    )
                )

                VStack(alignment: .leading, spacing: 6) {
                    Text(title)
                        .font(.caption.bold())
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .frame(height: 32, alignment: .top)
                        .foregroundStyle(.primary)
                    Text(subtitle)
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(Color.white)
                        .padding(.horizontal, 8)
                        .padding(.vertical, 4)
                        .background(statusTint.opacity(0.92), in: Capsule())
                }
                .padding(10)
            }
            .frame(width: 160)
            .glassCard()
            .contentShape(RoundedRectangle(cornerRadius: 16))
        }
        .buttonStyle(.plain)
    }
}

struct FeaturedGameCard: View {
    let game: CloudGame
    let onOpenDetails: () -> Void

    var body: some View {
        Button(action: {
            Haptics.light()
            onOpenDetails()
        }) {
            GameArtworkCard(
                game: game,
                artworkHeight: 196,
                titleFont: .headline.bold(),
                subtitleFont: .caption.weight(.medium),
                storeBadgeLimit: 0
            )
            .frame(width: 260)
            .contentShape(RoundedRectangle(cornerRadius: 16))
        }
        .buttonStyle(.plain)
    }
}

private struct FeaturedGameCardSkeleton: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            RoundedRectangle(cornerRadius: 14)
                .fill(.quaternary.opacity(0.4))
                .frame(width: 160, height: 100)
                .shimmeringSkeleton()
            VStack(alignment: .leading, spacing: 4) {
                RoundedRectangle(cornerRadius: 5)
                    .fill(.quaternary.opacity(0.4))
                    .frame(height: 32)
                RoundedRectangle(cornerRadius: 4)
                    .fill(.quaternary.opacity(0.3))
                    .frame(width: 70, height: 14)
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
    let onOpenDetails: () -> Void

    var body: some View {
        Button(action: {
            Haptics.light()
            onOpenDetails()
        }) {
            GameArtworkCard(
                game: game,
                artworkHeight: 236,
                titleFont: .headline.bold(),
                subtitleFont: .caption.weight(.medium),
                storeBadgeLimit: 0
            )
            .frame(maxWidth: .infinity, alignment: .topLeading)
            .contentShape(RoundedRectangle(cornerRadius: 16))
        }
        .buttonStyle(.plain)
    }
}

struct GameLaunchDetailsSheet: View {
    let game: CloudGame
    let onLaunch: (GameLaunchOption?) -> Void
    @Environment(\.dismiss) private var dismiss
    @State private var selectedOption: GameLaunchOption?

    private var launcherOptions: [GameLaunchOption] {
        if game.launchOptions.isEmpty, let launchAppId = game.launchAppId {
            return [GameLaunchOption(storefront: "Auto", appId: launchAppId, supportedControls: nil)]
        }
        return game.launchOptions
    }

    private var launchUnavailableMessage: String? {
        if !OpenNOWPlatform.supportsEmbeddedStreamer {
            return OpenNOWPlatform.streamingUnavailableReason
        }
        if launcherOptions.isEmpty {
            return "This game doesn't expose launch targets yet."
        }
        return nil
    }

    private var tagBackgroundColor: Color {
        #if os(tvOS)
        return Color.white.opacity(0.12)
        #else
        return Color(.systemFill)
        #endif
    }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 20) {
                    detailHero

                    VStack(alignment: .leading, spacing: 12) {
                        Text("Overview")
                            .font(.headline)
                        Text(summaryText)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                            .fixedSize(horizontal: false, vertical: true)
                    }

                    if !resolvedStores.isEmpty {
                        VStack(alignment: .leading, spacing: 12) {
                            Text("Storefronts")
                                .font(.headline)
                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack(spacing: 10) {
                                    ForEach(resolvedStores, id: \.self) { store in
                                        StorePill(store: store, prominent: true)
                                    }
                                }
                            }
                        }
                    }

                    LazyVGrid(columns: detailMetadataColumns, spacing: 12) {
                        if let releaseDate = game.releaseDate {
                            GameMetaCard(label: "Release", value: releaseDate, icon: "calendar")
                        }
                        if let publisher = game.publisher {
                            GameMetaCard(label: "Publisher", value: publisher, icon: "building.2")
                        }
                        if let developer = game.developer {
                            GameMetaCard(label: "Developer", value: developer, icon: "hammer")
                        }
                        GameMetaCard(label: "Genre", value: game.genre, icon: "sparkles.tv")
                        GameMetaCard(label: "Platform", value: game.platform, icon: "gamecontroller")
                        if let playType = game.playType {
                            GameMetaCard(label: "Play Type", value: playType, icon: "play.rectangle")
                        }
                        if let tier = game.membershipTierLabel {
                            GameMetaCard(label: "Membership", value: tier, icon: "person.crop.circle.badge.checkmark")
                        }
                    }

                    if let launchUnavailableMessage {
                        Text(launchUnavailableMessage)
                            .font(.subheadline)
                            .foregroundStyle(.secondary)
                    }

                    if !launcherOptions.isEmpty {
                        VStack(alignment: .leading, spacing: 12) {
                            Text("Launch With")
                                .font(.headline)
                            ForEach(launcherOptions) { option in
                                Button {
                                    Haptics.selection()
                                    selectedOption = option
                                } label: {
                                    HStack(spacing: 12) {
                                        StoreGlyph(store: option.storefront)
                                            .frame(width: 34, height: 34)
                                        VStack(alignment: .leading, spacing: 2) {
                                            Text(option.storefront.capitalized)
                                                .font(.subheadline.weight(.semibold))
                                            Text(launchOptionSubtitle(option))
                                                .font(.caption)
                                                .foregroundStyle(.secondary)
                                                .lineLimit(1)
                                        }
                                        Spacer()
                                        if selectedOption?.id == option.id {
                                            Image(systemName: "checkmark.circle.fill")
                                                .foregroundStyle(brandAccent)
                                        }
                                    }
                                    .padding(14)
                                    .glassCard()
                                }
                                .buttonStyle(.plain)
                            }
                        }
                    }

                    if !detailLabels.isEmpty {
                        VStack(alignment: .leading, spacing: 12) {
                            Text("Features")
                                .font(.headline)
                            ScrollView(.horizontal, showsIndicators: false) {
                                HStack(spacing: 8) {
                                    ForEach(detailLabels.prefix(16), id: \.self) { tag in
                                        Text(tag)
                                            .font(.caption.weight(.semibold))
                                            .padding(.horizontal, 10)
                                            .padding(.vertical, 6)
                                            .background(tagBackgroundColor, in: Capsule())
                                    }
                                }
                            }
                        }
                    }
                }
                .padding(20)
            }
            .navigationTitle("Game Details")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        Haptics.light()
                        dismiss()
                    } label: {
                        Image(systemName: "xmark")
                            .font(.headline.weight(.semibold))
                    }
                }
            }
            .safeAreaInset(edge: .bottom) {
                Button {
                    Haptics.medium()
                    onLaunch(selectedOption ?? launcherOptions.first)
                    dismiss()
                } label: {
                    Text(launchUnavailableMessage == nil ? "Launch" : "Launch Unavailable")
                        .font(.headline)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                }
                .buttonStyle(.borderedProminent)
                .tint(brandAccent)
                .disabled(launchUnavailableMessage != nil)
                .padding(.horizontal, 20)
                .padding(.top, 12)
                .padding(.bottom, 8)
                .background(.regularMaterial)
            }
            .background(appBackground)
        }
        .onAppear {
            selectedOption = launcherOptions.first
        }
    }

    private var resolvedStores: [String] {
        if let stores = game.stores, !stores.isEmpty {
            return stores
        }
        let derived = Array(Set(launcherOptions.map(\.storefront))).sorted()
        return derived
    }

    private var summaryText: String {
        let long = game.longDescription?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if !long.isEmpty {
            return long
        }
        let trimmed = game.summary?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        if !trimmed.isEmpty {
            return trimmed
        }
        return "\(game.title) is available through \(resolvedStores.isEmpty ? game.platform : resolvedStores.joined(separator: ", ")) on OpenNOW."
    }

    private var detailLabels: [String] {
        Array(Set((game.featureLabels ?? []) + (game.tags ?? []))).sorted()
    }

    private func launchOptionSubtitle(_ option: GameLaunchOption) -> String {
        let controls = option.supportedControls?.filter { !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty } ?? []
        if !controls.isEmpty {
            return controls.joined(separator: ", ")
        }
        return "Ready to launch"
    }

    private var detailMetadataColumns: [GridItem] {
        [GridItem(.adaptive(minimum: 150, maximum: 220), spacing: 12)]
    }

    private var detailHero: some View {
        ZStack(alignment: .bottomLeading) {
            GameArtworkView(game: game, iconSize: 44)
                .frame(height: 260)
                .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))

            VStack(alignment: .leading, spacing: 12) {
                HStack(spacing: 8) {
                    Text(game.genre)
                        .font(.caption.weight(.semibold))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(Color.white.opacity(0.16), in: Capsule())

                    Text(game.platform)
                        .font(.caption.weight(.semibold))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(Color.white.opacity(0.12), in: Capsule())
                }

                Text(game.title)
                    .font(.title2.bold())
                    .foregroundStyle(Color.white)
                    .lineLimit(2)

                if !resolvedStores.isEmpty {
                    HStack(spacing: 8) {
                        ForEach(Array(resolvedStores.prefix(3)), id: \.self) { store in
                            StorePill(store: store, prominent: false)
                        }
                    }
                }
            }
            .padding(18)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                LinearGradient(
                    colors: [.clear, .black.opacity(0.15), .black.opacity(0.48)],
                    startPoint: .top,
                    endPoint: .bottom
                )
            )
            .overlay(alignment: .bottomLeading) {
                Rectangle()
                    .fill(.ultraThinMaterial.opacity(0.85))
                    .frame(height: 108)
                    .mask(
                        LinearGradient(
                            colors: [.clear, Color.white.opacity(0.35), Color.white],
                            startPoint: .top,
                            endPoint: .bottom
                        )
                    )
                    .clipShape(
                        UnevenRoundedRectangle(
                            topLeadingRadius: 0,
                            bottomLeadingRadius: 24,
                            bottomTrailingRadius: 24,
                            topTrailingRadius: 0
                        )
                    )
            }
            .clipShape(RoundedRectangle(cornerRadius: 24, style: .continuous))
        }
        .overlay(
            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .stroke(Color.white.opacity(0.08), lineWidth: 1)
        )
    }

}

private struct GameArtworkCard: View {
    let game: CloudGame
    let artworkHeight: CGFloat
    let titleFont: Font
    let subtitleFont: Font
    let storeBadgeLimit: Int

    var body: some View {
        ZStack(alignment: .bottomLeading) {
            GameArtworkView(game: game, iconSize: 36)
                .frame(maxWidth: .infinity)
                .frame(height: artworkHeight)
                .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))

            LinearGradient(
                colors: [.clear, .black.opacity(0.36), .black.opacity(0.88)],
                startPoint: .top,
                endPoint: .bottom
            )
            .frame(height: min(artworkHeight * 0.74, 170))
            .frame(maxHeight: .infinity, alignment: .bottom)
            .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))

            VStack(alignment: .leading, spacing: 8) {
                Text(game.title)
                    .font(titleFont)
                    .foregroundStyle(Color.white)
                    .lineLimit(3)
                    .minimumScaleFactor(0.86)
                    .fixedSize(horizontal: false, vertical: true)

                Text("\(game.genre) · \(game.platform)")
                    .font(subtitleFont)
                    .foregroundStyle(Color.white.opacity(0.82))
                    .lineLimit(1)

                if !displayStores.isEmpty {
                    HStack(spacing: 8) {
                        ForEach(displayStores, id: \.self) { store in
                            StorePill(store: store, prominent: false)
                        }
                    }
                }
            }
            .padding(14)
            .padding(.top, 26)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background {
                Rectangle()
                    .fill(.ultraThinMaterial.opacity(0.86))
                    .mask(
                        LinearGradient(
                            colors: [.clear, Color.white.opacity(0.35), Color.white],
                            startPoint: .top,
                            endPoint: .bottom
                        )
                    )
                    .allowsHitTesting(false)
            }
            .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
        .overlay(
            RoundedRectangle(cornerRadius: 18, style: .continuous)
                .stroke(Color.white.opacity(0.08), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.18), radius: 12, y: 8)
    }

    private var displayStores: [String] {
        Array(gameResolvedStores(game: game).prefix(storeBadgeLimit))
    }
}

private struct GameMetaCard: View {
    let label: String
    let value: String
    let icon: String

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Label(label, systemImage: icon)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
            Text(value)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(.primary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .frame(maxWidth: .infinity, minHeight: 88, alignment: .topLeading)
        .padding(14)
        .glassCard()
    }
}

private struct StorePill: View {
    let store: String
    let prominent: Bool

    var body: some View {
        HStack(spacing: 8) {
            StoreGlyph(store: store)
                .frame(width: prominent ? 28 : 22, height: prominent ? 28 : 22)
            if prominent {
                Text(storeDisplayName(store))
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
            }
        }
        .foregroundColor(prominent ? .primary : .white)
        .padding(.horizontal, prominent ? 12 : 6)
        .padding(.vertical, prominent ? 10 : 6)
        .background(backgroundShape)
    }

    @ViewBuilder
    private var backgroundShape: some View {
        if prominent {
            Capsule()
                .fill(.regularMaterial)
                .overlay(Capsule().stroke(Color.white.opacity(0.12), lineWidth: 1))
        } else {
            Capsule()
                .fill(Color.white.opacity(0.12))
                .overlay(Capsule().stroke(Color.white.opacity(0.14), lineWidth: 1))
        }
    }
}

private struct StoreGlyph: View {
    let store: String

    var body: some View {
        ZStack {
            if showsGlyphBackground {
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(glyphBackground)
            }
            if let assetName {
                Image(assetName)
                    .resizable()
                    .renderingMode(.original)
                    .scaledToFit()
                    .padding(imagePadding)
            } else {
                Image(systemName: "bag.fill")
                    .font(.system(size: 12, weight: .bold))
                    .foregroundStyle(Color.white)
            }
        }
    }

    private var normalizedStore: String {
        storeNormalizedKey(store)
    }

    private var glyphBackground: some ShapeStyle {
        switch normalizedStore {
        case "steam":
            return AnyShapeStyle(
                LinearGradient(
                    colors: [Color(red: 0.08, green: 0.16, blue: 0.24), Color(red: 0.17, green: 0.42, blue: 0.70)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
        case "epic":
            return AnyShapeStyle(Color.black)
        case "xbox":
            return AnyShapeStyle(
                LinearGradient(
                    colors: [Color(red: 0.31, green: 0.66, blue: 0.17), Color(red: 0.15, green: 0.48, blue: 0.12)],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                )
            )
        default:
            return AnyShapeStyle(Color.gray.opacity(0.8))
        }
    }

    private var showsGlyphBackground: Bool {
        normalizedStore != "steam"
    }

    private var assetName: String? {
        switch normalizedStore {
        case "steam":
            return "StoreSteam"
        case "epic":
            return "StoreEpic"
        case "xbox":
            return "StoreXbox"
        default:
            return nil
        }
    }

    private var imagePadding: CGFloat {
        switch normalizedStore {
        case "steam":
            return 0
        case "epic":
            return 3
        case "xbox":
            return 4
        default:
            return 2
        }
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

struct GameArtworkView: View {
    let game: CloudGame
    let iconSize: CGFloat

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                gameColor(for: game.title).opacity(0.2)
                if let imageUrl = game.imageUrl, let url = URL(string: imageUrl) {
                    CachedRemoteImage(url: url) { image in
                        image
                            .resizable()
                            .scaledToFill()
                            .frame(width: proxy.size.width, height: proxy.size.height)
                    } placeholder: {
                        Rectangle()
                            .fill(.quaternary.opacity(0.25))
                            .frame(width: proxy.size.width, height: proxy.size.height)
                            .shimmeringSkeleton()
                    } failure: {
                        iconFallback
                            .frame(width: proxy.size.width, height: proxy.size.height)
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
    let cornerRadius: CGFloat

    func body(content: Content) -> some View {
        if #available(iOS 26, *) {
            content
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: cornerRadius))
                .glassEffect(in: RoundedRectangle(cornerRadius: cornerRadius))
        } else {
            content
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: cornerRadius))
                .shadow(color: .black.opacity(0.08), radius: 4, y: 2)
        }
    }
}

extension View {
    func glassCard(cornerRadius: CGFloat = 16) -> some View {
        modifier(GlassCardModifier(cornerRadius: cornerRadius))
    }

    func shimmeringSkeleton() -> some View {
        modifier(SkeletonShimmerModifier())
    }

    func presentGameDetailsUIKit(
        selectedGame: Binding<CloudGame?>,
        onLaunch: @escaping (CloudGame, GameLaunchOption?) -> Void
    ) -> some View {
        #if os(tvOS)
        sheet(item: selectedGame) { game in
            GameLaunchDetailsSheet(game: game) { option in
                onLaunch(game, option)
                selectedGame.wrappedValue = nil
            }
        }
        #else
        background(
            UIKitGameDetailsPresenter(selectedGame: selectedGame, onLaunch: onLaunch)
                .frame(width: 0, height: 0)
        )
        #endif
    }
}

private func storeNormalizedKey(_ store: String) -> String {
    store.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
}

func storeDisplayName(_ store: String) -> String {
    switch storeNormalizedKey(store) {
    case "epic", "epic games":
        return "Epic"
    case "steam":
        return "Steam"
    default:
        return store.capitalized
    }
}

func gameResolvedStores(game: CloudGame) -> [String] {
    if let stores = game.stores, !stores.isEmpty {
        return stores
    }
    let derived = Array(Set(game.launchOptions.map(\.storefront))).sorted()
    return derived.isEmpty ? [game.platform] : derived
}

#if !os(tvOS)
private struct UIKitGameDetailsPresenter: UIViewControllerRepresentable {
    @Binding var selectedGame: CloudGame?
    let onLaunch: (CloudGame, GameLaunchOption?) -> Void

    func makeUIViewController(context: Context) -> UIViewController {
        let controller = UIViewController()
        controller.view.backgroundColor = .clear
        return controller
    }

    func updateUIViewController(_ uiViewController: UIViewController, context: Context) {
        context.coordinator.parent = self
        if let game = selectedGame {
            if context.coordinator.presentedGameId == game.id {
                return
            }
            guard uiViewController.presentedViewController == nil else {
                return
            }
            let hosted = UIHostingController(
                rootView: GameLaunchDetailsSheet(game: game) { option in
                    onLaunch(game, option)
                    selectedGame = nil
                }
            )
            hosted.modalPresentationStyle = .pageSheet
            if let sheet = hosted.sheetPresentationController {
                sheet.detents = [.large()]
                sheet.prefersGrabberVisible = true
                sheet.preferredCornerRadius = 28
            }
            hosted.presentationController?.delegate = context.coordinator
            context.coordinator.presentedGameId = game.id
            uiViewController.present(hosted, animated: true)
        } else if let presented = uiViewController.presentedViewController {
            context.coordinator.presentedGameId = nil
            presented.dismiss(animated: true)
        }
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    final class Coordinator: NSObject, UIAdaptivePresentationControllerDelegate {
        var parent: UIKitGameDetailsPresenter
        var presentedGameId: String?

        init(parent: UIKitGameDetailsPresenter) {
            self.parent = parent
        }

        func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
            presentedGameId = nil
            parent.selectedGame = nil
        }
    }
}
#endif
