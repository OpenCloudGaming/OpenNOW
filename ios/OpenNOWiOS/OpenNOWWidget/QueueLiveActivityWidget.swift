import ActivityKit
import Foundation
import SwiftUI
import WidgetKit

/// The queue, on the lock screen and in the Dynamic Island.
///
/// Three rules shaped this, all from how the surfaces are actually used:
///
/// 1. **The minimal and compact presentations get one value each.** They are twenty points wide
///    and read in under a second, so the app icon is the wrong thing to put there — the island
///    already tells you which app it is. The changing number is the only thing worth the space.
/// 2. **Progress is drawn only when it is real.** The previous version had a three-segment "phase
///    track" that looked like a progress bar and never moved within a phase, which is worse than
///    no bar. The ring and the bar here are driven by measured queue movement, and both fall back
///    to indeterminate when the app has not seen enough to say.
/// 3. **Stale means stale.** ActivityKit keeps showing the last state after updates stop; the
///    views dim and say so rather than presenting a stale number as current.
struct QueueLiveActivityWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: QueueActivityAttributes.self) { context in
            LockScreenQueueLiveActivityView(context: context)
                .activityBackgroundTint(QueuePhaseStyle.background(context.state.phase))
                .activitySystemActionForegroundColor(QueuePhaseStyle.accent(context.state.phase))
                .widgetURL(URL(string: "opennow://resume"))
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    QueueProgressRing(
                        progress: context.state.progress,
                        phase: context.state.phase,
                        diameter: 34
                    ) {
                        LiveActivityAppIcon(size: 22)
                    }
                    .padding(.leading, DynamicIslandInset.expandedEdge)
                    .padding(.vertical, DynamicIslandInset.expandedVertical)
                }

                DynamicIslandExpandedRegion(.trailing) {
                    QueueValueText(state: context.state, size: .expanded)
                        .padding(.trailing, DynamicIslandInset.expandedEdge)
                        .padding(.vertical, DynamicIslandInset.expandedVertical)
                }

                DynamicIslandExpandedRegion(.center) {
                    VStack(spacing: 2) {
                        Text(context.attributes.gameTitle)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                            .minimumScaleFactor(0.72)
                        Text(contextLine(for: context))
                            .font(.caption2)
                            .foregroundStyle(.white.opacity(0.7))
                            .lineLimit(1)
                    }
                    .padding(.horizontal, DynamicIslandInset.expandedEdge)
                    .padding(.top, DynamicIslandInset.expandedVertical)
                }

                DynamicIslandExpandedRegion(.bottom) {
                    VStack(alignment: .leading, spacing: 7) {
                        QueueProgressBar(progress: context.state.progress, phase: context.state.phase)
                        HStack(spacing: 6) {
                            Text(context.isStale ? "Waiting for an update…" : context.state.detail)
                                .font(.caption)
                                .foregroundStyle(.white.opacity(context.isStale ? 0.5 : 0.9))
                                .lineLimit(1)
                                .minimumScaleFactor(0.8)
                            Spacer(minLength: 4)
                            if context.state.phase != .ready, let since = context.state.waitStartedAt {
                                QueueElapsedTimer(since: since, font: .caption2.weight(.medium))
                            }
                            if context.state.phase == .ready {
                                Label("Resume", systemImage: "play.fill")
                                    .font(.caption.weight(.semibold))
                                    .foregroundStyle(QueuePhaseStyle.accent(.ready))
                            }
                        }
                    }
                    .padding(.horizontal, DynamicIslandInset.expandedEdge)
                    .padding(.top, DynamicIslandInset.expandedVertical)
                    .padding(.bottom, DynamicIslandInset.expandedBottom)
                }
            } compactLeading: {
                // A ring rather than a bare icon: it carries progress in a slot that would
                // otherwise repeat what the island already tells you.
                QueueProgressRing(
                    progress: context.state.progress,
                    phase: context.state.phase,
                    diameter: DynamicIslandInset.compactRingDiameter
                ) {
                    LiveActivityAppIcon(size: 11)
                }
                .padding(.leading, DynamicIslandInset.compactOuter)
                .padding(.trailing, DynamicIslandInset.compactInner)
                .padding(.vertical, DynamicIslandInset.compactVertical)
            } compactTrailing: {
                QueueValueText(state: context.state, size: .compact)
                    .padding(.leading, DynamicIslandInset.compactInner)
                    .padding(.trailing, DynamicIslandInset.compactOuter)
                    .padding(.vertical, DynamicIslandInset.compactVertical)
            } minimal: {
                // Shown when another activity is competing for the island. One value only.
                QueueMinimalIndicator(state: context.state)
                    .padding(.horizontal, DynamicIslandInset.compactOuter)
                    .padding(.vertical, DynamicIslandInset.compactVertical)
            }
            .keylineTint(QueuePhaseStyle.accent(context.state.phase))
            .widgetURL(URL(string: "opennow://resume"))
        }
    }

    private func contextLine(for context: ActivityViewContext<QueueActivityAttributes>) -> String {
        let store = context.attributes.storeName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        let base = store.isEmpty ? "GeForce NOW" : store
        switch context.state.phase {
        case .ready: return "\(base) · ready"
        case .waiting: return "\(base) · preparing"
        case .queued: return "\(base) · in queue"
        }
    }
}

// MARK: - Island metrics

/// How far every presentation stays clear of the island's own edge.
///
/// The island is a capsule, so its corners curve away from the content well before the region's
/// nominal bounds end — content flush against a region reads as if it has been cut by the pill. The
/// first version pushed a 20-point ring with a centred 2-point stroke into a compact slot roughly
/// 23 points tall and gave it no inset at all, so it was: the stroke sat half outside the frame it
/// declared, and both ends touched the curve.
///
/// The fix has two halves — a stroke that stays inside its own bounds (see `QueueProgressRing`) and
/// these insets. Outer edges get more than inner ones, because that is where the curve is.
private enum DynamicIslandInset {
    /// Toward the island's rounded end.
    static let compactOuter: CGFloat = 4
    /// Toward the sensor housing in the middle, which is a straight edge and needs less.
    static let compactInner: CGFloat = 2
    static let compactVertical: CGFloat = 1
    /// Leaves the ring's 18-point outer bound sitting inside a slot of roughly 23 points.
    static let compactRingDiameter: CGFloat = 18

    static let expandedEdge: CGFloat = 8
    static let expandedVertical: CGFloat = 2
    static let expandedBottom: CGFloat = 4
}

// MARK: - Phase styling

private enum QueuePhaseStyle {
    /// Kept close to the app's own status palette: the queue is neutral, setup is the "fair"
    /// amber, ready is the accent green. Nothing here is red — waiting is not an error.
    static func accent(_ phase: QueueActivityAttributes.ContentState.Phase) -> Color {
        switch phase {
        case .queued: return Color(red: 0.45, green: 0.82, blue: 1.0)
        case .waiting: return Color(red: 1.0, green: 0.79, blue: 0.35)
        case .ready: return Color(red: 0.42, green: 0.94, blue: 0.63)
        }
    }

    static func background(_ phase: QueueActivityAttributes.ContentState.Phase) -> Color {
        switch phase {
        case .queued: return Color(red: 0.03, green: 0.06, blue: 0.09)
        case .waiting: return Color(red: 0.09, green: 0.075, blue: 0.03)
        case .ready: return Color(red: 0.02, green: 0.11, blue: 0.07)
        }
    }
}

// MARK: - Lock screen

private struct LockScreenQueueLiveActivityView: View {
    let context: ActivityViewContext<QueueActivityAttributes>

    private var accent: Color { QueuePhaseStyle.accent(context.state.phase) }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(alignment: .center, spacing: 13) {
                QueueProgressRing(
                    progress: context.state.progress,
                    phase: context.state.phase,
                    diameter: 46
                ) {
                    LiveActivityAppIcon(size: 30)
                }
                // A soft pool of the phase colour under the ring. The lock screen tints the whole
                // panel one flat shade, and a single light source gives it somewhere to sit.
                .background {
                    Circle()
                        .fill(
                            RadialGradient(
                                colors: [accent.opacity(0.28), .clear],
                                center: .center,
                                startRadius: 4,
                                endRadius: 42
                            )
                        )
                        .frame(width: 84, height: 84)
                        .allowsHitTesting(false)
                }

                VStack(alignment: .leading, spacing: 2) {
                    Text(context.attributes.gameTitle)
                        .font(.headline)
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .minimumScaleFactor(0.78)
                    Text(context.state.headline)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(accent)
                        .lineLimit(1)
                }

                Spacer(minLength: 8)

                QueueValueText(state: context.state, size: .lockScreen)
            }

            QueueProgressBar(progress: context.state.progress, phase: context.state.phase)

            HStack(spacing: 6) {
                Text(context.isStale ? "Waiting for an update from OpenNOW…" : context.state.detail)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(context.isStale ? 0.5 : 0.75))
                    .lineLimit(1)
                    .minimumScaleFactor(0.82)
                Spacer(minLength: 4)
                if context.state.phase != .ready, let since = context.state.waitStartedAt {
                    QueueElapsedTimer(since: since, font: .caption.weight(.medium))
                }
                if context.state.phase == .ready {
                    Label("Tap to resume", systemImage: "play.fill")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(accent)
                        .lineLimit(1)
                }
            }
        }
        // The banner presentation has its own rounded container, so the same rule applies here as
        // in the island: leave room for the corner rather than filling to the nominal edge.
        .padding(.horizontal, 20)
        .padding(.vertical, 17)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(context.attributes.gameTitle), \(context.state.headline)")
        .accessibilityValue(context.state.detail)
    }
}

// MARK: - Progress

/// A ring around the app icon. Determinate when the app has measured enough queue movement to
/// know how far through the wait you are; a slow indeterminate arc otherwise.
private struct QueueProgressRing<Content: View>: View {
    let progress: Double?
    let phase: QueueActivityAttributes.ContentState.Phase
    let diameter: CGFloat
    @ViewBuilder let content: () -> Content

    private var lineWidth: CGFloat { max(2, diameter * 0.085) }

    var body: some View {
        ZStack {
            // `strokeBorder` and the matching inset keep the whole ring inside `diameter`. A plain
            // `stroke` centres the line on the path, so half of it spills outside the frame — which
            // is invisible on the lock screen and clipped by the pill in the compact island.
            Circle()
                .strokeBorder(Color.white.opacity(0.16), lineWidth: lineWidth)
            Circle()
                .inset(by: lineWidth / 2)
                .trim(from: 0, to: trimEnd)
                .stroke(
                    // An angular gradient follows the arc, so the stroke gains depth along its
                    // length instead of reading as a flat wedge of colour.
                    AngularGradient(
                        colors: [
                            QueuePhaseStyle.accent(phase).opacity(0.45),
                            QueuePhaseStyle.accent(phase),
                            QueuePhaseStyle.accent(phase)
                        ],
                        center: .center,
                        angle: .degrees(-90)
                    ),
                    style: StrokeStyle(lineWidth: lineWidth, lineCap: .round)
                )
                .rotationEffect(.degrees(-90))
                // Ready is the one state worth drawing attention to, and a glow says so without
                // taking any more room than the ring already occupies.
                .shadow(
                    color: phase == .ready ? QueuePhaseStyle.accent(phase).opacity(0.55) : .clear,
                    radius: lineWidth
                )
                .animation(.smooth(duration: 0.45), value: trimEnd)
            content()
        }
        .frame(width: diameter, height: diameter)
        .accessibilityHidden(true)
    }

    private var trimEnd: CGFloat {
        if phase == .ready { return 1 }
        // A short fixed arc reads as "working" without pretending to know a percentage.
        guard let progress = QueueActivityProgress.normalized(progress) else { return 0.18 }
        return CGFloat(min(max(progress, 0.03), 1))
    }
}

/// The wide bar. Same rule as the ring: real progress or an honest sliver, never a fake full bar.
private struct QueueProgressBar: View {
    let progress: Double?
    let phase: QueueActivityAttributes.ContentState.Phase

    var body: some View {
        ZStack(alignment: .leading) {
            Capsule().fill(Color.white.opacity(0.14))
            QueueProgressFill(fraction: fraction)
                .fill(LinearGradient(
                    colors: [QueuePhaseStyle.accent(phase).opacity(0.72), QueuePhaseStyle.accent(phase)],
                    startPoint: .leading,
                    endPoint: .trailing
                ))
        }
        .frame(height: 5)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Queue progress")
        .accessibilityValue(accessibilityValue)
    }

    private var fraction: CGFloat {
        CGFloat(QueueActivityProgress.barFraction(progress, phase: phase))
    }

    private var accessibilityValue: String {
        switch phase {
        case .ready: return "Ready"
        case .waiting: return "Preparing your rig"
        case .queued:
            guard let progress = QueueActivityProgress.normalized(progress) else { return "Waiting in queue" }
            return "\(Int((progress * 100).rounded())) percent through the queue"
        }
    }
}

/// Widget snapshots can propose zero/unbounded sizes. Draw within the resolved bounds instead
/// of introducing a GeometryReader child-placement pass (the production crash site).
private struct QueueProgressFill: Shape {
    let fraction: CGFloat

    func path(in rect: CGRect) -> Path {
        guard rect.width.isFinite, rect.height.isFinite, rect.width > 0, rect.height > 0 else { return Path() }
        let width = min(rect.width, max(min(6, rect.width), rect.width * fraction))
        return Capsule().path(in: CGRect(x: rect.minX, y: rect.minY, width: width, height: rect.height))
    }
}

/// How long this wait has been going, ticking on its own.
///
/// `Text(_:style: .timer)` is one of the few things the system keeps animating inside a Live
/// Activity without the app pushing an update, which is what makes the panel read as live between
/// two-second polls. It counts up rather than down on purpose: the queue estimate is a range
/// because queue rates are not stable enough to promise a finish time, and a countdown would
/// quietly turn that range back into a promise.
private struct QueueElapsedTimer: View {
    let since: Date
    let font: Font
    var opacity: Double = 0.62

    var body: some View {
        Text(since, style: .timer)
            .font(font)
            .monospacedDigit()
            .foregroundStyle(.white.opacity(opacity))
            .lineLimit(1)
            .fixedSize()
            .accessibilityLabel("Waiting time")
    }
}

// MARK: - Values

private struct QueueValueText: View {
    enum Size {
        case compact
        case expanded
        case lockScreen
    }

    let state: QueueActivityAttributes.ContentState
    let size: Size

    var body: some View {
        Group {
            if state.phase == .ready {
                Image(systemName: "play.circle.fill")
                    .font(readyFont)
            } else if let position = state.queuePosition {
                Text("\(position)")
                    .font(font)
                    .monospacedDigit()
                    // Apple's signature for a counter that changes in place; without it the
                    // number hard-cuts and reads as a glitch on the lock screen.
                    .contentTransition(.numericText(countsDown: true))
            } else {
                Text(state.phase == .waiting ? "Setup" : "Queue")
                    .font(fallbackFont)
            }
        }
        .foregroundStyle(QueuePhaseStyle.accent(state.phase))
        .lineLimit(1)
        .minimumScaleFactor(0.7)
        .accessibilityLabel(accessibilityLabel)
    }

    private var accessibilityLabel: String {
        switch state.phase {
        case .ready: return "Ready to play"
        case .waiting: return "Preparing"
        case .queued:
            guard let position = state.queuePosition else { return "In queue" }
            return position == 1 ? "Next in queue" : "Queue position \(position)"
        }
    }

    private var font: Font {
        switch size {
        case .compact: return .callout.weight(.bold)
        case .expanded: return .title2.weight(.bold)
        case .lockScreen: return .system(size: 32, weight: .heavy, design: .rounded)
        }
    }

    private var readyFont: Font {
        switch size {
        case .compact: return .callout.weight(.semibold)
        case .expanded: return .title2
        case .lockScreen: return .system(size: 30)
        }
    }

    private var fallbackFont: Font {
        switch size {
        case .compact: return .caption2.weight(.bold)
        case .expanded: return .subheadline.weight(.bold)
        case .lockScreen: return .headline.weight(.bold)
        }
    }
}

/// What shows when another Live Activity is sharing the island. One glyph, one meaning.
private struct QueueMinimalIndicator: View {
    let state: QueueActivityAttributes.ContentState

    var body: some View {
        Group {
            if state.phase == .ready {
                Image(systemName: "play.fill").font(.caption2.weight(.bold))
            } else if let position = state.queuePosition, position < 100 {
                Text("\(position)")
                    .font(.caption2.weight(.bold))
                    .monospacedDigit()
                    .contentTransition(.numericText(countsDown: true))
            } else {
                Image(systemName: "hourglass").font(.caption2.weight(.bold))
            }
        }
        .foregroundStyle(QueuePhaseStyle.accent(state.phase))
        .accessibilityLabel(state.phase == .ready ? "Session ready" : "In queue")
    }
}

private struct LiveActivityAppIcon: View {
    let size: CGFloat

    var body: some View {
        Image("LiveActivityAppIcon")
            .resizable()
            .scaledToFill()
            .frame(width: size, height: size)
            .clipShape(Circle())
            .accessibilityHidden(true)
    }
}
