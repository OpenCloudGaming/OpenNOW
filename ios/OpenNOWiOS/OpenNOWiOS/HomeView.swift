import SwiftUI
import UIKit

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
                        .foregroundStyle(.white)
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

private struct FeaturedGameCard: View {
    let game: CloudGame
    let onOpenDetails: () -> Void

    var body: some View {
        Button(action: {
            Haptics.light()
            onOpenDetails()
        }) {
            VStack(alignment: .leading, spacing: 0) {
                GameArtworkView(game: game, iconSize: 48)
                    .frame(width: 160, height: 100)
                    .clipShape(UnevenRoundedRectangle(topLeadingRadius: 14, bottomLeadingRadius: 0, bottomTrailingRadius: 0, topTrailingRadius: 14))

                VStack(alignment: .leading, spacing: 4) {
                    Text(game.title)
                        .font(.caption.bold())
                        .lineLimit(2)
                        .multilineTextAlignment(.leading)
                        .frame(height: 32, alignment: .top)
                    Text(game.platform)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .frame(height: 14, alignment: .top)
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
            VStack(alignment: .leading, spacing: 8) {
                GameArtworkView(game: game, iconSize: 32)
                    .frame(maxWidth: .infinity)
                    .frame(height: 112)
                    .clipShape(RoundedRectangle(cornerRadius: 10))

                VStack(alignment: .leading, spacing: 3) {
                    Text(game.title)
                        .font(.caption.bold())
                        .lineLimit(2)
                        .frame(height: 32, alignment: .topLeading)
                    Text("\(game.genre) · \(game.platform)")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                        .frame(height: 14, alignment: .topLeading)
                }

                Spacer(minLength: 0)
            }
            .padding(10)
            .frame(maxWidth: .infinity, alignment: .topLeading)
            .frame(height: 220)
            .glassCard()
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
            return [GameLaunchOption(storefront: "Auto", appId: launchAppId)]
        }
        return game.launchOptions
    }

    var body: some View {
        NavigationStack {
            VStack(alignment: .leading, spacing: 16) {
                GameArtworkView(game: game, iconSize: 40)
                    .frame(height: 160)
                    .clipShape(RoundedRectangle(cornerRadius: 18))
                    .glassCard()

                VStack(alignment: .leading, spacing: 6) {
                    Text(game.title)
                        .font(.title3.bold())
                    Text("\(game.genre) · \(game.platform)")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }

                if let summary = game.summary, !summary.isEmpty {
                    Text(summary)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                        .padding(.top, 2)
                }

                VStack(spacing: 8) {
                    if let releaseDate = game.releaseDate {
                        metadataRow(label: "Release", value: releaseDate, icon: "calendar")
                    }
                    if let publisher = game.publisher {
                        metadataRow(label: "Publisher", value: publisher, icon: "building.2")
                    }
                    if let developer = game.developer {
                        metadataRow(label: "Developer", value: developer, icon: "hammer")
                    }
                    if let stores = resolvedStores, !stores.isEmpty {
                        metadataRow(label: "Stores", value: stores.joined(separator: ", "), icon: "bag")
                    }
                    if let appId = game.uuid {
                        metadataRow(label: "Game ID", value: appId, icon: "number")
                    }
                }

                if launcherOptions.isEmpty {
                    Text("This game doesn't expose launch targets yet.")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                } else {
                    VStack(alignment: .leading, spacing: 10) {
                        Text("Launch With")
                            .font(.headline)
                        ForEach(launcherOptions) { option in
                            Button {
                                Haptics.selection()
                                selectedOption = option
                            } label: {
                                HStack {
                                    Text(option.storefront.capitalized)
                                        .font(.subheadline.weight(.semibold))
                                    Spacer()
                                    if selectedOption?.id == option.id {
                                        Image(systemName: "checkmark.circle.fill")
                                            .foregroundStyle(brandAccent)
                                    }
                                }
                                .padding(12)
                                .glassCard()
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }

                if let tags = game.tags, !tags.isEmpty {
                    ScrollView(.horizontal, showsIndicators: false) {
                        HStack(spacing: 8) {
                            ForEach(tags.prefix(10), id: \.self) { tag in
                                Text(tag)
                                    .font(.caption.weight(.semibold))
                                    .padding(.horizontal, 10)
                                    .padding(.vertical, 6)
                                    .background(Color(.systemFill), in: Capsule())
                            }
                        }
                    }
                }

                Button {
                    Haptics.medium()
                    onLaunch(selectedOption ?? launcherOptions.first)
                    dismiss()
                } label: {
                    Text("Launch")
                        .font(.headline)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 12)
                }
                .buttonStyle(.borderedProminent)
                .tint(brandAccent)
                .disabled(launcherOptions.isEmpty)

                Spacer(minLength: 0)
            }
            .padding(20)
            .navigationTitle("Game Details")
            .navigationBarTitleDisplayMode(.inline)
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
            .background(appBackground)
        }
        .onAppear {
            selectedOption = launcherOptions.first
        }
    }

    private var resolvedStores: [String]? {
        if let stores = game.stores, !stores.isEmpty {
            return stores
        }
        let derived = Array(Set(launcherOptions.map(\.storefront))).sorted()
        return derived.isEmpty ? nil : derived
    }

    @ViewBuilder
    private func metadataRow(label: String, value: String, icon: String) -> some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: icon)
                .font(.caption)
                .foregroundStyle(.secondary)
                .frame(width: 16, height: 16)
            VStack(alignment: .leading, spacing: 2) {
                Text(label)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
                Text(value)
                    .font(.subheadline)
                    .foregroundStyle(.primary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(Color(.secondarySystemBackground), in: RoundedRectangle(cornerRadius: 10))
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

    func presentGameDetailsUIKit(
        selectedGame: Binding<CloudGame?>,
        onLaunch: @escaping (CloudGame, GameLaunchOption?) -> Void
    ) -> some View {
        background(
            UIKitGameDetailsPresenter(selectedGame: selectedGame, onLaunch: onLaunch)
                .frame(width: 0, height: 0)
        )
    }
}

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
