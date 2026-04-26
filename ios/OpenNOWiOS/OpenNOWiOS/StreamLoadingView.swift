import AVKit
import SwiftUI

struct StreamLoadingView: View {
  @EnvironmentObject private var store: OpenNOWStore

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
    ("Launching", "wifi"),
  ]

  private var currentPhase: StreamPhase {
    if let adState = store.effectiveAdState,
      store.activeSession?.status == 1,
      adState.sessionAdsRequired ?? adState.isAdsRequired
    {
      return .queue
    }
    guard let session = store.activeSession else { return .queue }
    switch session.status {
    case 2: return .setup
    case 3: return .launching
    case 1:
      if session.seatSetupStep == 1 {
        return .queue
      }
      if let pos = session.queuePosition, pos > 1 {
        return .queue
      }
      return .setup
    default: return .queue
    }
  }

  private var statusMessage: String {
    switch currentPhase {
    case .queue:
      if let adState = store.effectiveAdState, store.activeQueueAd != nil {
        if adState.opportunity?.queuePaused == true || adState.isQueuePaused == true {
          return adState.message ?? "Session queue paused. Resume ad playback to continue."
        }
        if adState.sessionAdsRequired ?? adState.isAdsRequired {
          return adState.message ?? "Watch queue ads to continue."
        }
      }
      if let pos = store.activeSession?.queuePosition {
        return pos == 1 ? "A cloud rig is nearly ready." : "Waiting for a cloud rig to free up."
      }
      return store.isLaunchingSession ? "Starting session..." : "Waiting in queue..."
    case .setup:
      return "Setting up your gaming rig..."
    case .launching:
      if store.streamSession != nil {
        return "Opening stream..."
      }
      if let signalingUrl = store.activeSession?.signalingUrl, !signalingUrl.isEmpty {
        return "Connecting streamer..."
      }
      if let signalingServer = store.activeSession?.signalingServer, !signalingServer.isEmpty {
        return "Connecting streamer..."
      }
      return "Finalizing stream endpoint..."
    }
  }

  var body: some View {
    GeometryReader { proxy in
      let isWide = shouldUseWideLayout(size: proxy.size)
      ZStack {
        #if os(tvOS)
          Color.black
            .ignoresSafeArea()
        #else
          Color(.systemBackground)
            .ignoresSafeArea()
        #endif

        ScrollView {
          Group {
            if isWide {
              HStack(alignment: .center, spacing: 28) {
                gameHeader(isWide: true)
                  .frame(maxWidth: 440)

                queueProgressPanel(isWide: true)
                  .frame(maxWidth: 430)
              }
              .frame(maxWidth: 960)
            } else {
              VStack(spacing: 24) {
                gameHeader(isWide: false)
                queueProgressPanel(isWide: false)
              }
              .frame(maxWidth: 430)
            }
          }
          .padding(.horizontal, isWide ? 32 : 24)
          .padding(.vertical, isWide ? 20 : 28)
          .frame(maxWidth: .infinity)
          .frame(
            minHeight: max(
              0, proxy.size.height - proxy.safeAreaInsets.top - proxy.safeAreaInsets.bottom))
        }
        #if os(iOS)
          .scrollBounceBehavior(.basedOnSize)
        #endif
      }
    }
    .animation(.spring(response: 0.34, dampingFraction: 0.84), value: currentPhase)
    .animation(
      .spring(response: 0.34, dampingFraction: 0.84), value: store.activeSession?.queuePosition
    )
    .animation(.easeInOut(duration: 0.22), value: statusMessage)
  }

  private func shouldUseWideLayout(size: CGSize) -> Bool {
    size.width > size.height && size.width >= 680
  }

  private func gameHeader(isWide: Bool) -> some View {
    VStack(alignment: isWide ? .leading : .center, spacing: 14) {
      gameArtworkHero(isWide: isWide)

      VStack(alignment: isWide ? .leading : .center, spacing: 4) {
        Text(store.activeSession?.game.title ?? "Preparing your game")
          .font(isWide ? .title.bold() : .title2.bold())
          .foregroundStyle(.primary)
          .multilineTextAlignment(isWide ? .leading : .center)
          .lineLimit(3)
          .minimumScaleFactor(0.78)
          .fixedSize(horizontal: false, vertical: true)

        Text(store.activeSession?.game.platform ?? "Cloud Gaming")
          .font(.caption)
          .foregroundStyle(.secondary)
      }
      .frame(maxWidth: .infinity, alignment: isWide ? .leading : .center)
    }
    .frame(maxWidth: .infinity, alignment: isWide ? .leading : .center)
  }

  @ViewBuilder
  private func gameArtworkHero(isWide: Bool) -> some View {
    let cornerRadius: CGFloat = isWide ? 26 : 22
    ZStack {
      if let game = store.activeSession?.game {
        GameArtworkView(game: game, iconSize: isWide ? 72 : 58)
      } else {
        BrandLogoView(size: isWide ? 92 : 78)
          .frame(maxWidth: .infinity, maxHeight: .infinity)
          .background(.regularMaterial)
      }
    }
    .aspectRatio(16.0 / 9.0, contentMode: .fit)
    .frame(maxWidth: isWide ? 440 : 340)
    .clipShape(RoundedRectangle(cornerRadius: cornerRadius, style: .continuous))
    .overlay(
      RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
        .stroke(Color.white.opacity(0.1), lineWidth: 1)
    )
    .shadow(color: .black.opacity(0.18), radius: 18, y: 10)
  }

  private func queueProgressPanel(isWide: Bool) -> some View {
    VStack(spacing: isWide ? 18 : 22) {
      queuePositionBadge

      if let ad = store.activeQueueAd {
        QueueAdPlayerCard(ad: ad)
          .environmentObject(store)
      }

      stepsView

      Text(statusMessage)
        .font(.subheadline)
        .foregroundStyle(.secondary)
        .multilineTextAlignment(.center)
        .numericQueueTransition(value: store.activeSession?.queuePosition ?? -1)

      ProgressView()
        .progressViewStyle(.circular)
        .tint(brandAccent)
        .scaleEffect(1.3)

      HStack(spacing: 12) {
        Button {
          Haptics.light()
          store.minimizeQueueOverlay()
        } label: {
          Label("Minimize", systemImage: "rectangle.compress.vertical")
            .frame(maxWidth: .infinity)
        }
        .streamActionButtonStyle()

        Button(role: .destructive) {
          Haptics.medium()
          Task { await store.endSession() }
        } label: {
          Label("Cancel", systemImage: "xmark")
            .frame(maxWidth: .infinity)
        }
        .streamActionButtonStyle(tint: .red.opacity(0.92))
      }
      .frame(maxWidth: 360)
      .padding(.top, 4)
    }
    .frame(maxWidth: .infinity)
  }

  @ViewBuilder
  private var queuePositionBadge: some View {
    if currentPhase == .queue, let pos = store.activeSession?.queuePosition {
      HStack(spacing: 10) {
        Image(systemName: pos == 1 ? "bolt.fill" : "person.3.fill")
          .foregroundStyle(.orange)
        Text(pos == 1 ? "Next in queue" : "Queue #\(pos)")
          .font(.subheadline.bold())
          .foregroundStyle(.primary)
          .lineLimit(1)
          .minimumScaleFactor(0.82)
      }
      .padding(.horizontal, 14)
      .padding(.vertical, 11)
      .background(
        Group {
          if #available(iOS 26, *) {
            Capsule()
              .fill(.regularMaterial)
              .glassEffect(in: Capsule())
              .overlay(
                Capsule()
                  .stroke(.orange.opacity(0.35), lineWidth: 1)
              )
          } else {
            Capsule()
              .fill(.regularMaterial)
              .overlay(
                Capsule()
                  .fill(.orange.opacity(0.08))
              )
          }
        }
      )
      .frame(maxWidth: 360)
      .numericQueueTransition(value: pos)
      .transition(.scale(scale: 0.92).combined(with: .opacity))
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
            .lineLimit(1)
            .minimumScaleFactor(0.9)
            .foregroundStyle(labelColor(for: stepState(index: index)))
        }
        .frame(width: 88)

        if index < steps.count - 1 {
          Rectangle()
            .fill(connectorGradient(after: index))
            .frame(width: 24, height: 2)
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
        .fill(Color.secondary.opacity(0.12))
        .overlay(
          Circle()
            .stroke(Color.secondary.opacity(0.2), lineWidth: 2)
        )
    case .active:
      TimelineView(.animation(minimumInterval: 1.0 / 30.0, paused: false)) { timeline in
        let phase = timeline.date.timeIntervalSinceReferenceDate.remainder(dividingBy: 0.9) / 0.9
        let pulse = 0.5 - 0.5 * cos(phase * 2 * .pi)
        Group {
          if #available(iOS 26, *) {
            Circle()
              .fill(.regularMaterial)
              .glassEffect(in: Circle())
              .overlay(
                Circle()
                  .stroke(brandAccent.opacity(0.55), lineWidth: 1.5)
              )
          } else {
            Circle()
              .fill(brandAccent)
          }
        }
        .scaleEffect(1.0 + (0.15 * pulse))
        .opacity(1.0 - (0.08 * pulse))
      }
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
      return .secondary.opacity(0.7)
    case .active:
      return brandAccent
    case .completed:
      return .white
    }
  }

  private func labelColor(for state: StepState) -> Color {
    switch state {
    case .pending:
      return .secondary
    case .active:
      return .primary
    case .completed:
      return brandAccent
    }
  }

  private func connectorGradient(after index: Int) -> LinearGradient {
    let startColor: Color =
      stepState(index: index) == .completed ? brandAccent : .secondary.opacity(0.2)
    let endColor: Color
    switch stepState(index: index + 1) {
    case .pending:
      endColor = .secondary.opacity(0.2)
    case .active, .completed:
      endColor = brandAccent
    }
    return LinearGradient(
      colors: [startColor, endColor], startPoint: .leading, endPoint: .trailing)
  }
}

// Bare AVPlayerViewController wrapper — no system transport controls so only
// our custom play/pause button is visible (no ±10s skip buttons).
private struct AdVideoView: UIViewControllerRepresentable {
  let player: AVPlayer

  func makeUIViewController(context: Context) -> AVPlayerViewController {
    let vc = AVPlayerViewController()
    vc.player = player
    vc.showsPlaybackControls = false
    vc.videoGravity = .resizeAspect
    return vc
  }

  func updateUIViewController(_ vc: AVPlayerViewController, context: Context) {
    vc.player = player
  }
}

private struct QueueAdPlayerCard: View {
  @EnvironmentObject private var store: OpenNOWStore
  let ad: SessionAdInfo

  @State private var player = AVPlayer()
  @State private var adDurationObserver: Any?
  @State private var adEndObserver: NSObjectProtocol?
  @State private var currentItemId: String?
  @State private var watchedTimeMs = 0
  @State private var didSendFinish = false
  @State private var hasReportedPlaying = false
  @State private var isPaused = false
  @State private var isMuted = false
  @State private var isPlaying = false

  var body: some View {
    if !didSendFinish {
      VStack(alignment: .leading, spacing: 10) {
        HStack(spacing: 8) {
          Image(systemName: "play.rectangle.fill")
            .foregroundStyle(.orange)
          Text("Ad Queue")
            .font(.caption.bold())
            .foregroundStyle(.secondary)
        }

        Group {
          if let mediaUrl = preferredMediaURLString(for: ad), let url = URL(string: mediaUrl) {
            ZStack(alignment: .bottom) {
              AdVideoView(player: player)
                .frame(height: 150)
                .clipShape(RoundedRectangle(cornerRadius: 12))

              HStack {
                Button {
                  if isPlaying {
                    player.pause()
                  } else {
                    player.play()
                  }
                } label: {
                  Image(systemName: isPlaying ? "pause.fill" : "play.fill")
                    .font(.caption.bold())
                    .foregroundStyle(.white)
                    .padding(8)
                    .background(.ultraThinMaterial, in: Circle())
                }

                Spacer()

                Button {
                  toggleMute()
                } label: {
                  Image(systemName: isMuted ? "speaker.slash.fill" : "speaker.wave.2.fill")
                    .font(.caption.bold())
                    .foregroundStyle(.white)
                    .padding(8)
                    .background(.ultraThinMaterial, in: Circle())
                }
              }
              .padding(8)
            }
            .onAppear {
              configurePlayer(url: url)
            }
            .onChange(of: ad.adId) { _, _ in
              didSendFinish = false
              hasReportedPlaying = false
              isPaused = false
              isPlaying = false
              configurePlayer(url: url)
            }
            .onDisappear {
              teardownPlayer()
            }
          } else {
            RoundedRectangle(cornerRadius: 12)
              .fill(Color.secondary.opacity(0.15))
              .frame(height: 150)
              .overlay(
                VStack(spacing: 6) {
                  Image(systemName: "video.slash.fill")
                    .font(.title3)
                    .foregroundStyle(.secondary)
                  Text("Ad media unavailable")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                }
              )
          }
        }

        if let message = store.effectiveAdState?.message, !message.isEmpty {
          Text(message)
            .font(.caption)
            .foregroundStyle(.secondary)
        }
      }
      .padding(12)
      .background(
        RoundedRectangle(cornerRadius: 14)
          .fill(.regularMaterial)
          .overlay(
            RoundedRectangle(cornerRadius: 14)
              .stroke(.orange.opacity(0.28), lineWidth: 1)
          )
      )
      .frame(maxWidth: 320)
    }
  }

  private func preferredMediaURLString(for ad: SessionAdInfo) -> String? {
    if let firstMedia = ad.adMediaFiles.first(where: { ($0.mediaFileUrl ?? "").isEmpty == false })?
      .mediaFileUrl
    {
      return firstMedia
    }
    if let adUrl = ad.adUrl, !adUrl.isEmpty {
      return adUrl
    }
    if let mediaUrl = ad.mediaUrl, !mediaUrl.isEmpty {
      return mediaUrl
    }
    return nil
  }

  private func configurePlayer(url: URL) {
    guard currentItemId != ad.adId else { return }
    teardownPlayer()
    currentItemId = ad.adId
    watchedTimeMs = 0
    didSendFinish = false
    hasReportedPlaying = false
    isPaused = false
    isPlaying = false

    let item = AVPlayerItem(url: url)
    player.replaceCurrentItem(with: item)
    player.isMuted = isMuted
    player.volume = 0.3
    player.play()

    adDurationObserver = player.addPeriodicTimeObserver(
      forInterval: CMTime(seconds: 0.25, preferredTimescale: 600),
      queue: .main
    ) { _ in
      watchedTimeMs = max(0, Int((player.currentTime().seconds * 1000).rounded()))
      let nowPlaying = player.rate > 0.01
      isPlaying = nowPlaying
      if nowPlaying, !hasReportedPlaying {
        hasReportedPlaying = true
        isPaused = false
        Task { @MainActor in
          store.reportQueueAdStarted(adId: ad.adId)
        }
      } else if !nowPlaying, hasReportedPlaying, !didSendFinish, !isPaused {
        isPaused = true
        Task { @MainActor in
          store.reportQueueAdPaused(adId: ad.adId)
        }
      } else if nowPlaying {
        isPaused = false
      }
    }

    adEndObserver = NotificationCenter.default.addObserver(
      forName: .AVPlayerItemDidPlayToEndTime,
      object: item,
      queue: .main
    ) { _ in
      guard !didSendFinish else { return }
      didSendFinish = true
      isPlaying = false
      Task { @MainActor in
        store.reportQueueAdFinished(adId: ad.adId, watchedTimeInMs: watchedTimeMs)
        await dismissQueueOverlayIfAdsFinished()
      }
    }
  }

  private func teardownPlayer() {
    player.pause()
    if let observer = adDurationObserver {
      player.removeTimeObserver(observer)
      adDurationObserver = nil
    }
    if let observer = adEndObserver {
      NotificationCenter.default.removeObserver(observer)
      adEndObserver = nil
    }
  }

  private func toggleMute() {
    isMuted.toggle()
    player.isMuted = isMuted
  }

  @MainActor
  private func dismissQueueOverlayIfAdsFinished() async {
    for _ in 0..<16 {
      let adsRequired =
        store.effectiveAdState.map { $0.sessionAdsRequired ?? $0.isAdsRequired } ?? false
      let isQueueing = (store.activeSession?.status ?? 0) == 1
      if isQueueing && (!adsRequired || store.activeQueueAd == nil) {
        store.minimizeQueueOverlay()
        return
      }
      try? await Task.sleep(for: .milliseconds(250))
    }
  }
}

private struct StreamActionButtonStyleModifier: ViewModifier {
  let tint: Color

  func body(content: Content) -> some View {
    content
      .font(.subheadline.bold())
      .foregroundStyle(.white.opacity(0.84))
      .padding(.horizontal, 18)
      .padding(.vertical, 11)
      .frame(maxWidth: .infinity)
      .background(
        Group {
          if #available(iOS 26, *) {
            Capsule()
              .fill(tint.opacity(0.14))
              .glassEffect(in: Capsule())
          } else {
            Capsule()
              .fill(.regularMaterial)
              .overlay(Capsule().fill(tint.opacity(0.14)))
          }
        }
      )
  }
}

extension View {
  fileprivate func streamActionButtonStyle(tint: Color = .white) -> some View {
    modifier(StreamActionButtonStyleModifier(tint: tint))
  }
}
