import ActivityKit
import SwiftUI
import WidgetKit

struct QueueLiveActivityWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: QueueActivityAttributes.self) { context in
            LockScreenQueueLiveActivityView(context: context)
                .activityBackgroundTint(backgroundTint(for: context.state.phase))
                .activitySystemActionForegroundColor(.white)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    VStack(alignment: .leading, spacing: 3) {
                        Text(context.attributes.gameTitle)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                            .minimumScaleFactor(0.72)
                        Text(context.state.headline)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.white.opacity(0.8))
                            .lineLimit(1)
                            .minimumScaleFactor(0.8)
                    }
                }
                DynamicIslandExpandedRegion(.trailing) {
                    QueueBadge(
                        label: expandedBadgeLabel(for: context.state),
                        phase: context.state.phase,
                        size: .expanded
                    )
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack(spacing: 10) {
                        CompactQueueIcon(phase: context.state.phase, size: 20)
                        Text(context.state.detail)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.white.opacity(0.92))
                            .lineLimit(1)
                            .minimumScaleFactor(0.78)
                        Spacer(minLength: 0)
                    }
                }
            } compactLeading: {
                CompactQueueIcon(phase: context.state.phase, size: 22)
            } compactTrailing: {
                QueueBadge(
                    label: compactBadgeLabel(for: context.state),
                    phase: context.state.phase,
                    size: .compact
                )
            } minimal: {
                CompactQueueIcon(phase: context.state.phase, size: 22)
            }
            .keylineTint(color(for: context.state.phase))
        }
    }

    private func backgroundTint(for phase: QueueActivityAttributes.ContentState.Phase) -> Color {
        switch phase {
        case .queued:
            return Color(red: 0.14, green: 0.2, blue: 0.27)
        case .waiting:
            return Color(red: 0.22, green: 0.2, blue: 0.12)
        case .ready:
            return Color(red: 0.08, green: 0.29, blue: 0.19)
        }
    }

    private func color(for phase: QueueActivityAttributes.ContentState.Phase) -> Color {
        switch phase {
        case .queued:
            return Color(red: 0.39, green: 0.71, blue: 1.0)
        case .waiting:
            return Color(red: 1.0, green: 0.8, blue: 0.33)
        case .ready:
            return Color(red: 0.45, green: 0.91, blue: 0.62)
        }
    }

    private func expandedBadgeLabel(for state: QueueActivityAttributes.ContentState) -> String {
        switch state.phase {
        case .queued:
            if let queue = state.queuePosition {
                return "#\(queue)"
            }
            return "QUEUE"
        case .waiting:
            return "WAIT"
        case .ready:
            return "READY"
        }
    }

    private func compactBadgeLabel(for state: QueueActivityAttributes.ContentState) -> String {
        switch state.phase {
        case .queued:
            if let queue = state.queuePosition {
                return "\(queue)"
            }
            return "Q"
        case .waiting:
            return "..."
        case .ready:
            return "GO"
        }
    }
}

private struct LockScreenQueueLiveActivityView: View {
    let context: ActivityViewContext<QueueActivityAttributes>

    var body: some View {
        HStack(alignment: .center, spacing: 14) {
            VStack(alignment: .leading, spacing: 6) {
                Text(context.attributes.gameTitle)
                    .font(.headline)
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .minimumScaleFactor(0.75)
                Text(context.state.headline)
                    .font(.subheadline.weight(.semibold))
                    .foregroundStyle(.white.opacity(0.88))
                    .lineLimit(1)
                    .minimumScaleFactor(0.82)
                Text(context.state.detail)
                    .font(.caption)
                    .foregroundStyle(.white.opacity(0.72))
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
            Spacer(minLength: 12)
            QueueBadge(
                label: lockScreenBadgeLabel(for: context.state),
                phase: context.state.phase,
                size: .lockScreen
            )
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 16)
    }

    private func lockScreenBadgeLabel(for state: QueueActivityAttributes.ContentState) -> String {
        switch state.phase {
        case .queued:
            if let queue = state.queuePosition {
                return "#\(queue)"
            }
            return "QUEUE"
        case .waiting:
            return "WAIT"
        case .ready:
            return "READY"
        }
    }
}

private struct QueueBadge: View {
    enum Size {
        case compact
        case expanded
        case lockScreen
    }

    let label: String
    let phase: QueueActivityAttributes.ContentState.Phase
    let size: Size

    var body: some View {
        Text(label)
            .font(font)
            .monospacedDigit()
            .foregroundStyle(.white)
            .lineLimit(1)
            .minimumScaleFactor(0.65)
            .frame(minWidth: minWidth, minHeight: height)
            .padding(.horizontal, horizontalPadding)
            .background(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .fill(color.opacity(0.92))
            )
            .overlay(
                RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
                    .strokeBorder(.white.opacity(0.18), lineWidth: 1)
            )
    }

    private var color: Color {
        switch phase {
        case .queued:
            return Color(red: 0.26, green: 0.5, blue: 0.95)
        case .waiting:
            return Color(red: 0.85, green: 0.58, blue: 0.14)
        case .ready:
            return Color(red: 0.14, green: 0.65, blue: 0.34)
        }
    }

    private var font: Font {
        switch size {
        case .compact:
            return .caption2.weight(.bold)
        case .expanded:
            return .caption.weight(.bold)
        case .lockScreen:
            return .headline.weight(.bold)
        }
    }

    private var minWidth: CGFloat {
        switch size {
        case .compact:
            return 26
        case .expanded:
            return 50
        case .lockScreen:
            return 68
        }
    }

    private var height: CGFloat {
        switch size {
        case .compact:
            return 24
        case .expanded:
            return 30
        case .lockScreen:
            return 40
        }
    }

    private var horizontalPadding: CGFloat {
        switch size {
        case .compact:
            return 6
        case .expanded:
            return 8
        case .lockScreen:
            return 10
        }
    }

    private var cornerRadius: CGFloat {
        switch size {
        case .compact:
            return 8
        case .expanded:
            return 10
        case .lockScreen:
            return 12
        }
    }
}

private struct CompactQueueIcon: View {
    let phase: QueueActivityAttributes.ContentState.Phase
    let size: CGFloat

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.32, style: .continuous)
                .fill(iconColor.opacity(0.9))
                .frame(width: size, height: size)
            Image(systemName: iconName)
                .font(.system(size: size * 0.46, weight: .bold))
                .foregroundStyle(.white)
        }
    }

    private var iconName: String {
        switch phase {
        case .queued:
            return "hourglass"
        case .waiting:
            return "clock"
        case .ready:
            return "play.fill"
        }
    }

    private var iconColor: Color {
        switch phase {
        case .queued:
            return Color(red: 0.26, green: 0.5, blue: 0.95)
        case .waiting:
            return Color(red: 0.85, green: 0.58, blue: 0.14)
        case .ready:
            return Color(red: 0.14, green: 0.65, blue: 0.34)
        }
    }
}
