import SwiftUI

struct StreamLoadingView: View {
    @EnvironmentObject private var store: OpenNOWStore
    @State private var pulsing = false

    private enum StreamPhase: Equatable {
        case queue
        case setup
        case launching
    }

    private enum StepState: Equatable {
        case pending
        case active
        case completed
    }

    private let steps: [(title: String, icon: String)] = [
        ("Queue", "person.3.fill"),
        ("Setup", "cpu"),
        ("Launching", "wifi")
    ]

    private var currentPhase: StreamPhase {
        guard let session = store.activeSession else { return .queue }
        switch session.status {
        case 0: return .launching
        case 2: return .setup
        default: return .queue
        }
    }

    private var statusMessage: String {
        switch currentPhase {
        case .queue:
            if let pos = store.activeSession?.queuePosition {
                return "Position #\(pos) in queue"
            }
            return store.isLaunchingSession ? "Starting session..." : "Waiting in queue..."
        case .setup:
            return "Setting up your gaming rig..."
        case .launching:
            return "Connecting to server..."
        }
    }

    var body: some View {
        ZStack {
            Color.black.opacity(0.95)
                .ignoresSafeArea()

            VStack(spacing: 24) {
                Spacer()

                gameHeader

                if currentPhase == .queue, let pos = store.activeSession?.queuePosition {
                    HStack {
                        Image(systemName: "person.3.fill")
                            .foregroundStyle(.orange)
                        Text("Queue position: \(pos)")
                            .font(.subheadline.bold())
                            .foregroundStyle(.orange)
                    }
                    .padding(12)
                    .background(.orange.opacity(0.12), in: RoundedRectangle(cornerRadius: 10))
                    .frame(maxWidth: 320)
                }

                stepsView

                Text(statusMessage)
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.8))
                    .multilineTextAlignment(.center)

                ProgressView()
                    .progressViewStyle(.circular)
                    .tint(brandAccent)
                    .scaleEffect(1.3)

                Button(role: .cancel) {
                    Task { await store.endSession() }
                } label: {
                    Label("Cancel", systemImage: "xmark")
                        .font(.subheadline.bold())
                        .foregroundStyle(.white.opacity(0.6))
                        .padding(.horizontal, 24)
                        .padding(.vertical, 10)
                        .background(Color.white.opacity(0.08), in: Capsule())
                }
                .padding(.top, 8)

                Spacer()
            }
            .padding(.horizontal, 24)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .onAppear {
            pulsing = true
        }
    }

    private var gameHeader: some View {
        VStack(spacing: 14) {
            ZStack {
                Circle()
                    .fill(Color.white.opacity(0.06))
                    .frame(width: 92, height: 92)

                if let session = store.activeSession {
                    Image(systemName: session.game.icon)
                        .font(.system(size: 34, weight: .semibold))
                        .foregroundStyle(gameColor(for: session.game.title))
                } else {
                    Image(systemName: "bolt.fill")
                        .font(.system(size: 34, weight: .semibold))
                        .foregroundStyle(brandGradient)
                }
            }

            VStack(spacing: 4) {
                Text(store.activeSession?.game.title ?? "Preparing your game")
                    .font(.title2.bold())
                    .foregroundStyle(.white)
                    .multilineTextAlignment(.center)

                Text(store.activeSession?.game.platform ?? "Cloud Gaming")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var stepsView: some View {
        HStack(spacing: 0) {
            ForEach(Array(steps.enumerated()), id: \.offset) { index, step in
                VStack(spacing: 10) {
                    ZStack {
                        stepCircle(for: stepState(index: index))

                        if stepState(index: index) == .completed {
                            Image(systemName: "checkmark")
                                .font(.system(size: 16, weight: .bold))
                                .foregroundStyle(.white)
                        } else {
                            Image(systemName: step.icon)
                                .font(.system(size: 16, weight: .semibold))
                                .foregroundStyle(iconColor(for: stepState(index: index)))
                        }
                    }
                    .frame(width: 44, height: 44)

                    Text(step.title)
                        .font(.caption.bold())
                        .foregroundStyle(labelColor(for: stepState(index: index)))
                }

                if index < steps.count - 1 {
                    Rectangle()
                        .fill(connectorColor(after: index))
                        .frame(width: 36, height: 2)
                        .padding(.bottom, 26)
                }
            }
        }
        .frame(maxWidth: 320)
    }

    private func stepState(index: Int) -> StepState {
        let activeIndex: Int
        switch currentPhase {
        case .queue: activeIndex = 0
        case .setup: activeIndex = 1
        case .launching: activeIndex = 2
        }
        if index < activeIndex { return .completed }
        if index == activeIndex { return .active }
        return .pending
    }

    @ViewBuilder
    private func stepCircle(for state: StepState) -> some View {
        switch state {
        case .pending:
            Circle()
                .fill(Color.white.opacity(0.12))
                .overlay(
                    Circle()
                        .stroke(Color.white.opacity(0.2), lineWidth: 2)
                )
        case .active:
            Circle()
                .fill(brandAccent)
                .scaleEffect(pulsing ? 1.15 : 1.0)
                .opacity(pulsing ? 0.92 : 1.0)
                .animation(.easeInOut(duration: 0.9).repeatForever(autoreverses: true), value: pulsing)
        case .completed:
            Circle()
                .fill(brandAccent.opacity(0.35))
                .overlay(
                    Circle()
                        .stroke(brandAccent, lineWidth: 2)
                )
        }
    }

    private func iconColor(for state: StepState) -> Color {
        switch state {
        case .pending:
            return Color.white.opacity(0.4)
        case .active:
            return .white
        case .completed:
            return .white
        }
    }

    private func labelColor(for state: StepState) -> Color {
        switch state {
        case .pending:
            return Color.white.opacity(0.4)
        case .active:
            return .white
        case .completed:
            return brandAccent
        }
    }

    private func connectorColor(after index: Int) -> Color {
        switch stepState(index: index + 1) {
        case .pending:
            return Color.white.opacity(0.15)
        case .active, .completed:
            return brandAccent
        }
    }
}
