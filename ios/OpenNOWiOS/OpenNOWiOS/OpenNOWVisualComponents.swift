import SwiftUI

/// Animate the path's width, preserving a radius of half its height even during the countdown.
/// Scaling a capsule horizontally also scales its end caps into nearly flat edges.
struct HeroPageProgressFill: Shape {
    var progress: CGFloat
    var animatableData: CGFloat {
        get { progress }
        set { progress = newValue }
    }

    func path(in rect: CGRect) -> Path {
        guard rect.width.isFinite, rect.height.isFinite, rect.width > 0, rect.height > 0 else { return Path() }
        let fraction = progress.isFinite ? min(max(progress, 0), 1) : 0
        let dotWidth = min(rect.height, rect.width)
        let width = dotWidth + (rect.width - dotWidth) * fraction
        return Capsule().path(in: CGRect(x: rect.minX, y: rect.minY, width: width, height: rect.height))
    }
}

/// The single sweep that says "this whole screen is still loading".
///
/// Applied once, to a container — never per cell. The phase comes from the clock rather than a
/// `@State` toggle with `repeatForever`, because every screen that shows a skeleton also observes
/// the store, and each re-render used to re-evaluate the implicit animation and restart the sweep
/// mid-flight. A timeline cannot be interrupted by anything a parent does.
struct SkeletonShimmerModifier: ViewModifier {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private let period: TimeInterval = 1.35

    func body(content: Content) -> some View {
        content
            .overlay {
                if !reduceMotion {
                    GeometryReader { proxy in
                        // Let the display choose its native cadence. Hard-capping this at 30 Hz
                        // made a lightweight layer transform look visibly stepped on ProMotion.
                        TimelineView(.animation(paused: reduceMotion)) { context in
                            let phase = context.date.timeIntervalSinceReferenceDate
                                .truncatingRemainder(dividingBy: period) / period
                            LinearGradient(
                                colors: [
                                    .clear,
                                    Color.white.opacity(0.08),
                                    Color.white.opacity(0.30),
                                    Color.white.opacity(0.08),
                                    .clear
                                ],
                                startPoint: .top,
                                endPoint: .bottom
                            )
                            .rotationEffect(.degrees(18))
                            .frame(width: max(80, proxy.size.width * 0.48))
                            .offset(x: (CGFloat(phase) * 2.4 - 1) * proxy.size.width * 1.7)
                        }
                        // The sweep is decoration laid over a skeleton, so nothing an ancestor
                        // animates should reinterpolate a position already derived from the clock.
                        .transaction { $0.animation = nil }
                    }
                    .allowsHitTesting(false)
                }
            }
            .mask(content)
    }
}

/// The circular chip a control sits on when it sits on box art.
///
/// A material is the obvious choice, and it was the first one — but a material is a backdrop
/// capture, and a catalog page holds one per visible card. Twenty backdrop captures recomposited on
/// every scroll frame was the most expensive thing the grid did, in exchange for a blur that is
/// invisible behind an opaque glyph anyway. An opaque scrim with a hairline reads identically at
/// 30–42 points and costs a single fill, with no offscreen pass and no shadow.
struct ArtworkControlChipModifier: ViewModifier {
    let diameter: CGFloat
    var fill: Color = .black.opacity(0.46)

    func body(content: Content) -> some View {
        content
            .frame(width: diameter, height: diameter)
            .background(fill, in: Circle())
            .overlay {
                Circle().strokeBorder(Color.white.opacity(0.18), lineWidth: 0.5)
            }
    }
}

extension View {
    func artworkControlChip(diameter: CGFloat, fill: Color = .black.opacity(0.46)) -> some View {
        modifier(ArtworkControlChipModifier(diameter: diameter, fill: fill))
    }
}

/// The lift under a piece of artwork, drawn as the shadow of a shape rather than of the card.
///
/// `.shadow` applied to a composed subtree has to rasterise that subtree offscreen to find the
/// alpha it should blur, once per view per frame — a catalog page of twenty cards is twenty
/// offscreen passes on every scroll tick. The cards are opaque and already clipped to a rounded
/// rectangle, so the same shadow can come from an opaque shape sitting behind them, which Core
/// Animation draws from a path with no offscreen pass at all.
///
/// This is the same rule that removed the per-cell shimmer and the per-cell material: anything
/// costing a render pass per visible cell has to earn it, and decoration never does.
struct ArtworkCardShadowModifier: ViewModifier {
    let cornerRadius: CGFloat
    var opacity: Double = 0.14
    var radius: CGFloat = 8
    var offsetY: CGFloat = 4

    func body(content: Content) -> some View {
        content
            .background {
                // The shape has to be opaque for its shadow to match the card's, and it is filled
                // with the same ground the artwork itself uses rather than plain black: a card
                // whose image has not arrived is not fully opaque, and this is the colour its own
                // placeholder gradient resolves to, so the corner it lets through is unchanged.
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(OpenNOWPalette.imagePlaceholder)
                    .shadow(color: .black.opacity(opacity), radius: radius, y: offsetY)
            }
    }
}

extension View {
    func artworkCardShadow(
        cornerRadius: CGFloat,
        opacity: Double = 0.14,
        radius: CGFloat = 8,
        offsetY: CGFloat = 4
    ) -> some View {
        modifier(
            ArtworkCardShadowModifier(
                cornerRadius: cornerRadius,
                opacity: opacity,
                radius: radius,
                offsetY: offsetY
            )
        )
    }
}

struct GlassCardModifier: ViewModifier {
    let cornerRadius: CGFloat

    @ViewBuilder
    func body(content: Content) -> some View {
        if #available(iOS 26, *) {
            content
                .glassEffect(.regular, in: .rect(cornerRadius: cornerRadius))
        } else {
            content
                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                        .stroke(Color.white.opacity(0.10), lineWidth: 1)
                )
                .shadow(color: .black.opacity(0.10), radius: 8, y: 4)
        }
    }
}

private struct AdaptiveGlassButtonModifier: ViewModifier {
    let prominent: Bool

    @ViewBuilder
    func body(content: Content) -> some View {
        if #available(iOS 26, *) {
            if prominent {
                content.buttonStyle(.glassProminent)
            } else {
                content.buttonStyle(.glass)
            }
        } else if prominent {
            content.buttonStyle(.borderedProminent)
        } else {
            content.buttonStyle(.bordered)
        }
    }
}

extension View {
    func adaptiveGlassButton(prominent: Bool = false) -> some View {
        modifier(AdaptiveGlassButtonModifier(prominent: prominent))
    }
}
