import SwiftUI

/// The activated artwork, rather than just the game: the same game can appear in several rails.
struct GameDetailsTransitionOrigin {
    let sourceID: UUID
    let gameKey: String
}

struct GameDetailsTransitionContext {
    let namespace: Namespace.ID
    let registry: GameDetailsTransitionRegistry

    @MainActor
    func selectSource(_ origin: GameDetailsTransitionOrigin) { registry.origin = origin }
}

/// Navigation metadata is recorded synchronously before the sheet binding changes. Keeping it
/// in a reference avoids the presentation closure capturing a previous SwiftUI state snapshot.
@MainActor
final class GameDetailsTransitionRegistry: ObservableObject {
    var origin: GameDetailsTransitionOrigin?

    func sourceID(for game: CloudGame) -> UUID? {
        origin?.gameKey == catalogStableGameKey(game) ? origin?.sourceID : nil
    }
}

private struct GameDetailsTransitionContextKey: EnvironmentKey {
    static let defaultValue: GameDetailsTransitionContext? = nil
}

extension EnvironmentValues {
    var gameDetailsTransition: GameDetailsTransitionContext? {
        get { self[GameDetailsTransitionContextKey.self] }
        set { self[GameDetailsTransitionContextKey.self] = newValue }
    }
}

struct GameDetailsPresentationModifier: ViewModifier {
    @Binding var selectedGame: CloudGame?
    let store: OpenNOWStore
    let onLaunch: (CloudGame, GameLaunchOption?) -> Void
    @Namespace private var namespace
    @StateObject private var registry = GameDetailsTransitionRegistry()
    @State private var pendingLaunch: GameLaunchRequest?

    func body(content: Content) -> some View {
        content
            .environment(\.gameDetailsTransition, GameDetailsTransitionContext(namespace: namespace, registry: registry))
            .sheet(item: $selectedGame, onDismiss: {
                registry.origin = nil
                // A second presentation starts only after the native zoom has finished closing.
                if let request = pendingLaunch {
                    pendingLaunch = nil
                    onLaunch(request.game, request.launchOption)
                }
            }) { game in
                GameLaunchDetailsSheet(game: game) { option in
                    pendingLaunch = GameLaunchRequest(game: game, launchOption: option)
                    selectedGame = nil
                }
                .environmentObject(store)
                #if DEBUG
                .onAppear {
                    if ProcessInfo.processInfo.arguments.contains(where: { $0.hasPrefix("--opennow-zoom-qa-") }) {
                        NSLog("[GameDetailsQA] details appeared: %@; matched source: %@", game.id,
                              registry.sourceID(for: game)?.uuidString ?? "none")
                    }
                }
                #endif
                .modifier(GameDetailsZoomDestination(
                    sourceID: registry.sourceID(for: game),
                    namespace: namespace
                ))
            }
    }
}

private struct GameDetailsZoomDestination: ViewModifier {
    let sourceID: UUID?
    let namespace: Namespace.ID
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    @ViewBuilder
    func body(content: Content) -> some View {
        if #available(iOS 18, *), let sourceID, !reduceMotion {
            content.navigationTransition(.zoom(sourceID: sourceID, in: namespace))
        } else {
            content
        }
    }
}

private struct GameDetailsArtworkSource: ViewModifier {
    let id: UUID
    let cornerRadius: CGFloat
    @Environment(\.gameDetailsTransition) private var transition
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    @ViewBuilder
    func body(content: Content) -> some View {
        if #available(iOS 18, *), let transition, !reduceMotion {
            content.matchedTransitionSource(id: id, in: transition.namespace) { source in
                source.clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
            }
        } else {
            content
        }
    }
}

extension View {
    func gameDetailsArtworkSource(id: UUID, cornerRadius: CGFloat = 12) -> some View {
        modifier(GameDetailsArtworkSource(id: id, cornerRadius: cornerRadius))
    }

    @ViewBuilder
    func gameDetailsActionStyle(prominent: Bool = false) -> some View {
        if #available(iOS 26, *) {
            if prominent { buttonStyle(.glassProminent) } else { buttonStyle(.glass) }
        } else {
            if prominent { buttonStyle(.borderedProminent) } else { buttonStyle(.bordered) }
        }
    }
}

#if DEBUG
extension View {
    /// Opt-in simulator fixture: exercise the production artwork action and native return path.
    func gameDetailsVisualQA(game: CloudGame, source: String, activate: @escaping () -> Void) -> some View {
        task {
            guard ProcessInfo.processInfo.arguments.contains("--opennow-zoom-qa-\(source)"),
                  game.id == "debug-store-1" else { return }
            NSLog("[GameDetailsQA] starting source: %@", source)
            do {
                try await Task.sleep(for: .seconds(3))
                let cycles = ProcessInfo.processInfo.arguments.contains("--opennow-zoom-qa-hold") ? 1 : 3
                for _ in 0..<cycles {
                    guard !Task.isCancelled else { return }
                    activate()
                    NSLog("[GameDetailsQA] activated source: %@", source)
                    try await Task.sleep(for: .seconds(4))
                }
            } catch { }
        }
    }
}
#endif
