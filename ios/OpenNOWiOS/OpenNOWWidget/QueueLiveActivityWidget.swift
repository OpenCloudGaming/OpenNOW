import ActivityKit
import Foundation
import SwiftUI
import WidgetKit

#if OPENNOW_WIDGET_SNAPSHOT_TESTS
@testable import OpenNOWiOS
#endif

struct QueueLiveActivityWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: QueueActivityAttributes.self) { context in
            let presentation = QueueActivityPresentation(state: context.state, isStale: context.isStale)
            LockScreenQueueLiveActivityView(attributes: context.attributes, presentation: presentation)
                .activityBackgroundTint(QueueActivityPalette(presentation: presentation).background)
                .activitySystemActionForegroundColor(.white)
                .widgetURL(presentation.destination)
        } dynamicIsland: { context in
            let presentation = QueueActivityPresentation(state: context.state, isStale: context.isStale)
            let palette = QueueActivityPalette(presentation: presentation)

            return DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    QueueStatusEmblem(presentation: presentation, diameter: 36)
                        .padding(.leading, 8)
                        .padding(.top, 4)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    QueueValueText(presentation: presentation, compact: false)
                        .padding(.trailing, 8)
                        .padding(.top, 4)
                }
                DynamicIslandExpandedRegion(.center) {
                    VStack(spacing: 3) {
                        Text(verbatim: context.attributes.gameTitle)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        Text(verbatim: presentation.phaseLabel)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(palette.accent)
                            .lineLimit(1)
                    }
                    .padding(.horizontal, 6)
                    .padding(.top, 4)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    VStack(alignment: .leading, spacing: 8) {
                        QueueProgressStatus(presentation: presentation)
                        QueueActivityFooter(presentation: presentation, expandedIsland: true)
                    }
                    .padding(.horizontal, 8)
                    .padding(.top, 6)
                    .padding(.bottom, 6)
                }
            } compactLeading: {
                QueueStatusEmblem(presentation: presentation, diameter: 18)
                    .padding(.leading, 4)
                    .padding(.trailing, 2)
                    .accessibilityLabel(presentation.phaseLabel)
            } compactTrailing: {
                QueueValueText(presentation: presentation, compact: true)
                    .padding(.leading, 2)
                    .padding(.trailing, 4)
            } minimal: {
                QueueMinimalIndicator(presentation: presentation)
                    .padding(.horizontal, 4)
            }
            .keylineTint(palette.accent)
            .widgetURL(presentation.destination)
        }
    }
}

private struct QueueActivityPalette {
    let presentation: QueueActivityPresentation

    var accent: Color {
        if presentation.isStale { return Color(red: 1, green: 0.83, blue: 0.51) }
        switch presentation.state.phase {
        case .queued: return Color(red: 0.55, green: 0.89, blue: 1)
        case .waiting: return Color(red: 0.84, green: 0.76, blue: 1)
        case .ready: return Color(red: 0.61, green: 1, blue: 0.76)
        }
    }

    var background: Color {
        if presentation.isStale { return Color(red: 0.16, green: 0.21, blue: 0.28) }
        switch presentation.state.phase {
        case .queued: return Color(red: 0.06, green: 0.23, blue: 0.38)
        case .waiting: return Color(red: 0.23, green: 0.16, blue: 0.40)
        case .ready: return Color(red: 0.05, green: 0.29, blue: 0.24)
        }
    }

    var highlight: Color {
        if presentation.isStale { return Color(red: 0.25, green: 0.32, blue: 0.40) }
        switch presentation.state.phase {
        case .queued: return Color(red: 0.08, green: 0.37, blue: 0.49)
        case .waiting: return Color(red: 0.36, green: 0.24, blue: 0.51)
        case .ready: return Color(red: 0.08, green: 0.40, blue: 0.31)
        }
    }
}

struct LockScreenQueueLiveActivityView: View {
    let attributes: QueueActivityAttributes
    let presentation: QueueActivityPresentation

    private var palette: QueueActivityPalette { QueueActivityPalette(presentation: presentation) }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 6) {
                Image("LiveActivityAppIcon")
                    .resizable()
                    .scaledToFit()
                    .frame(width: 14, height: 14)
                    .clipShape(RoundedRectangle(cornerRadius: 4))
                    .accessibilityHidden(true)
                Text("OPENNOW")
                    .font(.system(.caption2, design: .rounded, weight: .bold))
                    .tracking(1.4)
                Spacer(minLength: 8)
                Text(verbatim: storeName)
                    .font(.caption2.weight(.medium))
                    .lineLimit(1)
            }
            .foregroundStyle(.white.opacity(0.8))

            HStack(alignment: .center, spacing: 12) {
                QueueStatusEmblem(presentation: presentation, diameter: 42)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 4) {
                    Text(verbatim: attributes.gameTitle)
                        .font(.headline.weight(.bold))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                    Text(verbatim: presentation.headline)
                        .font(.subheadline.weight(.medium))
                        .foregroundStyle(palette.accent)
                        .lineLimit(1)
                }
                .layoutPriority(1)
                Spacer(minLength: 0)
                QueueValueText(presentation: presentation, compact: false)
            }

            QueueProgressStatus(presentation: presentation)
            QueueActivityFooter(presentation: presentation, expandedIsland: false)
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 15)
        .background {
            LinearGradient(
                colors: [palette.highlight, palette.background],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            .overlay(alignment: .top) {
                Rectangle()
                    .fill(.white.opacity(0.15))
                    .frame(height: 1)
            }
        }
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: "OpenNOW. \(attributes.gameTitle). \(presentation.headline)"))
        .accessibilityValue(Text(verbatim: presentation.accessibilityValue))
        .accessibilityHint(presentation.isStale ? "Opens OpenNOW to check the session" : "Opens your session in OpenNOW")
    }

    private var storeName: String {
        let store = attributes.storeName?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        return store.isEmpty ? "GeForce NOW" : store
    }
}

private struct QueueStatusEmblem: View {
    let presentation: QueueActivityPresentation
    let diameter: CGFloat

    private var accent: Color { QueueActivityPalette(presentation: presentation).accent }

    var body: some View {
        ZStack {
            if diameter > 20 {
                Circle().fill(accent.opacity(0.12))
            }
            if let progress = presentation.progress, !presentation.canResume {
                Circle().strokeBorder(accent.opacity(0.22), lineWidth: 2)
                Circle()
                    .inset(by: 1)
                    .trim(from: 0, to: CGFloat(progress))
                    .stroke(accent, style: StrokeStyle(lineWidth: 2, lineCap: .round))
                    .rotationEffect(.degrees(-90))
            }
            Image(systemName: presentation.symbol)
                .font(.system(size: diameter * 0.40, weight: .semibold))
                .foregroundStyle(accent)
        }
        .frame(width: diameter, height: diameter)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(Text(verbatim: presentation.phaseLabel))
    }
}

private struct QueueProgressStatus: View {
    let presentation: QueueActivityPresentation

    private var accent: Color { QueueActivityPalette(presentation: presentation).accent }

    var body: some View {
        Group {
            if let progress = presentation.progress, !presentation.canResume {
                HStack(spacing: 10) {
                    Text(presentation.state.phase == .queued ? "Queue movement" : "Setup progress")
                        .font(.caption2)
                        .foregroundStyle(.white.opacity(0.8))
                        .lineLimit(1)
                    ZStack(alignment: .leading) {
                        Capsule().fill(.white.opacity(0.18))
                        QueueProgressFill(fraction: CGFloat(progress))
                            .fill(accent)
                    }
                    .frame(height: 4)
                    Text(progress, format: .percent.precision(.fractionLength(0)))
                        .font(.caption2.weight(.semibold))
                        .monospacedDigit()
                        .foregroundStyle(accent)
                }
                .accessibilityElement(children: .ignore)
                .accessibilityLabel(presentation.state.phase == .queued ? "Measured queue movement" : "Setup progress")
                .accessibilityValue(Text(progress, format: .percent.precision(.fractionLength(0))))
            } else {
                Rectangle()
                    .fill(.white.opacity(0.18))
                    .frame(height: 1)
                    .accessibilityHidden(true)
            }
        }
    }
}

private struct QueueProgressFill: Shape {
    let fraction: CGFloat

    func path(in rect: CGRect) -> Path {
        guard rect.width.isFinite, rect.height.isFinite, rect.width > 0, rect.height > 0,
              fraction.isFinite, fraction > 0 else { return Path() }
        let width = rect.width * min(fraction, 1)
        return Capsule().path(in: CGRect(x: rect.minX, y: rect.minY, width: width, height: rect.height))
    }
}

private struct QueueActivityFooter: View {
    let presentation: QueueActivityPresentation
    let expandedIsland: Bool

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Text(verbatim: presentation.detail)
                .font(.caption)
                .foregroundStyle(.white.opacity(0.88))
                .lineLimit(expandedIsland ? 2 : 1)
                .layoutPriority(1)
            Spacer(minLength: 0)
            if presentation.canResume {
                Label("Resume", systemImage: "arrow.up.right")
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(QueueActivityPalette(presentation: presentation).accent)
                    .fixedSize()
            } else if !presentation.isStale, let since = presentation.state.waitStartedAt {
                HStack(spacing: 4) {
                    Image(systemName: "clock")
                    Text(since, style: .timer)
                        .monospacedDigit()
                }
                .font(.caption2.weight(.medium))
                .foregroundStyle(.white.opacity(0.8))
                .fixedSize()
                .accessibilityElement(children: .combine)
                .accessibilityLabel("Elapsed wait")
            }
        }
    }
}

private struct QueueValueText: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    let presentation: QueueActivityPresentation
    let compact: Bool

    var body: some View {
        Group {
            if let position = presentation.queuePosition {
                VStack(alignment: .trailing, spacing: 0) {
                    Text(position, format: .number.grouping(.never))
                        .font(compact ? .caption.weight(.bold) : .system(.title, design: .rounded, weight: .bold))
                        .monospacedDigit()
                        .contentTransition(reduceMotion ? .identity : .numericText(countsDown: true))
                    if !compact {
                        Text("in queue")
                            .font(.caption2)
                    }
                }
            } else if compact && presentation.canResume {
                Image(systemName: "checkmark")
                    .font(.caption.weight(.semibold))
            } else {
                Text(presentation.isStale ? "Check" : presentation.canResume ? "Ready" : presentation.state.phase == .waiting ? "Setup" : "Queue")
                    .font(compact ? .caption2.weight(.semibold) : .subheadline.weight(.semibold))
            }
        }
        .foregroundStyle(QueueActivityPalette(presentation: presentation).accent)
        .lineLimit(1)
        .minimumScaleFactor(0.8)
        .accessibilityLabel(Text(verbatim: presentation.accessibilityValue))
    }
}

private struct QueueMinimalIndicator: View {
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    let presentation: QueueActivityPresentation

    var body: some View {
        Group {
            if let position = presentation.queuePosition, position < 100 {
                Text(position, format: .number.grouping(.never))
                    .monospacedDigit()
                    .contentTransition(reduceMotion ? .identity : .numericText(countsDown: true))
            } else {
                Image(systemName: presentation.symbol)
            }
        }
        .font(.caption2.weight(.bold))
        .foregroundStyle(QueueActivityPalette(presentation: presentation).accent)
        .accessibilityLabel(Text(verbatim: presentation.accessibilityValue))
    }
}

#if DEBUG
private struct QueueActivityPreview: View {
    let phase: QueueActivityAttributes.ContentState.Phase
    var isStale = false
    var progress: Double? = nil

    var body: some View {
        LockScreenQueueLiveActivityView(
            attributes: QueueActivityAttributes(sessionId: "preview", gameTitle: "Cyberpunk 2077", storeName: "Steam"),
            presentation: QueueActivityPresentation(
                state: .init(
                    phase: phase,
                    headline: phase == .ready ? "Ready to play" : phase == .waiting ? "Preparing your rig" : "In queue",
                    detail: phase == .ready ? "Your cloud rig is ready" : phase == .waiting ? "Setting up your cloud rig" : "Waiting for a cloud rig",
                    queueLabel: "Queue",
                    queuePosition: phase == .queued ? 24 : nil,
                    progress: progress
                ),
                isStale: isStale
            )
        )
        .clipShape(RoundedRectangle(cornerRadius: 24))
        .padding()
        .background(Color.black)
        .previewLayout(.fixed(width: 393, height: 190))
    }
}

private struct QueueLiveActivityPreviews: PreviewProvider {
    static var previews: some View {
        Group {
            QueueActivityPreview(phase: .queued, progress: 0.4).previewDisplayName("Queue · measured movement")
            QueueActivityPreview(phase: .queued).previewDisplayName("Queue · unknown progress")
            QueueActivityPreview(phase: .waiting).previewDisplayName("Setup · unknown progress")
            QueueActivityPreview(phase: .ready).previewDisplayName("Ready")
            QueueActivityPreview(phase: .ready, isStale: true).previewDisplayName("Stale · previously ready")
        }
    }
}
#endif
