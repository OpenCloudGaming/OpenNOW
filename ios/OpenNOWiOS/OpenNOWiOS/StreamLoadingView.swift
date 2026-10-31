import AVKit
import SwiftUI

struct StreamLoadingView: View {
  @EnvironmentObject private var store: OpenNOWStore
  @Environment(\.openNowAccent) private var accent
  @Environment(\.accessibilityReduceMotion) private var reduceMotion
  @Environment(\.dynamicTypeSize) private var dynamicTypeSize
  var coversBottomBar = false

  private var isPastQueue: Bool {
    store.activeSession.map(QueueSessionPhase.isPastQueue) ?? false
  }

  private var visibleQueuePosition: Int? {
    guard !isPastQueue, let position = store.displayQueuePosition, position > 0 else { return nil }
    return position
  }

  private var queueStatusText: String {
    if isPastQueue {
      return store.activeSession?.status == 1 ? "Setting up your rig" : "Connecting stream"
    }
    if let position = visibleQueuePosition {
      return "Queue position \(position)"
    }
    if let adState = store.effectiveAdState {
      if adState.opportunity?.queuePaused == true || adState.isQueuePaused == true {
        return "Queue paused"
      }
    }
    if store.activeSession?.seatSetupStep == 1 {
      return "Waiting for a rig"
    }
    return store.isLaunchingSession ? "Starting session" : "Waiting in queue"
  }

  private var queueAdNotice: String? {
    guard !isPastQueue, let adState = store.effectiveAdState else { return nil }
    if adState.opportunity?.queuePaused == true || adState.isQueuePaused == true {
      return "Queue paused"
    }
    if store.activeQueueAd != nil, adState.sessionAdsRequired ?? adState.isAdsRequired {
      return "Watch ad to continue"
    }
    return nil
  }

  var body: some View {
    GeometryReader { proxy in
      let isWide = proxy.size.width >= 600 && proxy.size.width > proxy.size.height
        && !dynamicTypeSize.isAccessibilitySize
      ZStack {
        QueueAmbientBackdrop(accent: accent.color)
          .ignoresSafeArea(edges: ignoredBackgroundEdges)

        ScrollView {
          Group {
            if let ad = store.activeQueueAd {
              queueAdPanel(ad: ad, isWide: isWide)
            } else {
              queueStatusPanel(isWide: isWide)
            }
          }
          .padding(OpenNOWSpacing.xl)
          .padding(proxy.safeAreaInsets)
          .frame(maxWidth: .infinity)
          .frame(minHeight: proxy.size.height)
        }
        #if os(iOS)
          .scrollBounceBehavior(.basedOnSize)
        #endif
      }
    }
    .environment(\.colorScheme, .dark)
  }

  private var ignoredBackgroundEdges: Edge.Set {
    coversBottomBar ? .all : [.top, .leading, .trailing]
  }

  private func queueStatusPanel(isWide: Bool) -> some View {
    let layout = isWide
      ? AnyLayout(HStackLayout(alignment: .center, spacing: OpenNOWSpacing.xxl))
      : AnyLayout(VStackLayout(spacing: OpenNOWSpacing.xl))
    return layout {
      VStack(spacing: OpenNOWSpacing.md) {
        queueGameArtwork
        gameHeading
      }
      .frame(maxWidth: isWide ? 260 : .infinity)

      queueDetails(compact: isWide)
        .frame(maxWidth: 430)
    }
    .frame(maxWidth: isWide ? 780 : 540)
  }

  private func queueAdPanel(ad: SessionAdInfo, isWide: Bool) -> some View {
    let layout = isWide
      ? AnyLayout(HStackLayout(alignment: .center, spacing: OpenNOWSpacing.lg))
      : AnyLayout(VStackLayout(spacing: OpenNOWSpacing.lg))
    return layout {
      QueueAdPlayerCard(ad: ad, compact: isWide)
        .environmentObject(store)

      VStack(spacing: OpenNOWSpacing.md) {
        gameHeading
        queueDetails(compact: true)
      }
      .frame(maxWidth: isWide ? 300 : .infinity)
    }
    .padding(OpenNOWSpacing.lg)
    .openNowCard(radius: OpenNOWRadius.xl, bordered: true)
    .frame(maxWidth: isWide ? 900 : 620)
  }

  private var gameHeading: some View {
    Text(store.activeSession?.game.title ?? "Starting stream")
      .font(.title3.weight(.bold))
      .foregroundStyle(OpenNOWPalette.textPrimary)
      .fixedSize(horizontal: false, vertical: true)
      .multilineTextAlignment(.center)
  }

  private func queueDetails(compact: Bool) -> some View {
    VStack(spacing: compact ? OpenNOWSpacing.sm : OpenNOWSpacing.lg) {
      AndroidQueueStatusText(text: queueStatusText, position: visibleQueuePosition, compact: compact)
      if visibleQueuePosition != nil {
        QueueTrendStrip(trend: store.queueTrend)
      }

      if isPastQueue {
        SessionSetupProgressView()
      } else {
        OscillatingQueueProgressView()
      }

      if let message = queueAdNotice {
        Label(message, systemImage: "info.circle")
          .font(OpenNOWFont.rowFootnote)
          .foregroundStyle(OpenNOWPalette.statusNotice)
          .fixedSize(horizontal: false, vertical: true)
      }

      actionButtons
      queueError
    }
    .animation(reduceMotion ? nil : OpenNOWMotion.standard, value: isPastQueue)
  }

  private var queueGameArtwork: some View {
    ZStack {
      if let game = store.activeSession?.game {
        GameArtworkView(
          game: game,
          iconSize: 52,
          role: .queue
        )
      } else {
        BrandLogoView(size: 64)
          .frame(maxWidth: .infinity, maxHeight: .infinity)
          .background(.regularMaterial)
      }
    }
    .aspectRatio(16.0 / 9.0, contentMode: .fit)
    .frame(maxWidth: 220)
    .clipShape(RoundedRectangle(cornerRadius: OpenNOWRadius.lg, style: .continuous))
    .overlay(
      RoundedRectangle(cornerRadius: OpenNOWRadius.lg, style: .continuous)
        .stroke(Color.white.opacity(0.1), lineWidth: 1)
    )
  }

  private var actionButtons: some View {
    let layout = dynamicTypeSize.isAccessibilitySize
      ? AnyLayout(VStackLayout(spacing: OpenNOWSpacing.sm))
      : AnyLayout(HStackLayout(spacing: OpenNOWSpacing.sm))
    return layout {
      Button {
        Haptics.light()
        store.minimizeQueueOverlay()
      } label: {
        Text("Minimize")
      }
      .accessibilityHint("Keep waiting while browsing OpenNOW")

      Button(role: .destructive) {
        Haptics.medium()
        Task { await store.endSession() }
      } label: {
        Text("Cancel")
      }
    }
    .buttonStyle(QueueActionButtonStyle())
  }

  @ViewBuilder private var queueError: some View {
    if let error = store.sessionError, !error.isEmpty {
      Text(error)
        .font(.footnote)
        .foregroundStyle(OpenNOWPalette.statusPoor)
        .multilineTextAlignment(.center)
        .padding(.top, 12)
    }
  }

}

struct QueueActionButtonStyle: ButtonStyle {
  @Environment(\.openNowAccent) private var accent

  func makeBody(configuration: Configuration) -> some View {
    let tint = configuration.role == .destructive ? Color.red : accent.color
    configuration.label
      .font(.subheadline.weight(.medium))
      .foregroundStyle(tint)
      .padding(.horizontal, 14)
      .padding(.vertical, 7)
      .background(tint.opacity(configuration.isPressed ? 0.24 : 0.12),
                  in: RoundedRectangle(cornerRadius: 10, style: .continuous))
      .frame(minWidth: OpenNOWSpacing.hitTarget, minHeight: OpenNOWSpacing.hitTarget)
      .contentShape(Rectangle())
  }
}

private struct QueueTrendStrip: View {
  let trend: QueueTrendEstimator

  @Environment(\.openNowAccent) private var accent
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  var body: some View {
    TimelineView(.periodic(from: .now, by: 30)) { context in
      let supportLine = trend.supportLine(now: context.date)
      if let supportLine {
        Text(supportLine)
          .font(.footnote.weight(.medium))
          .foregroundStyle(tint(now: context.date))
          .monospacedDigit()
          .multilineTextAlignment(.center)
          .frame(maxWidth: .infinity)
          .accessibilityElement(children: .ignore)
          .accessibilityLabel("Queue trend")
          .accessibilityValue(supportLine)
          .accessibilityHint("Wait estimates are based on observed queue movement")
          .transition(reduceMotion ? .opacity : .opacity.combined(with: .move(edge: .top)))
          .animation(reduceMotion ? nil : OpenNOWMotion.fast, value: supportLine)
      }
    }
  }

  private func tint(now: Date) -> Color {
    switch trend.trend(now: now) {
    case .moving: return accent.color
    case .holding: return OpenNOWPalette.statusFair
    case .slipped: return OpenNOWPalette.statusPoor
    case .unknown: return .secondary
    }
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
  @Environment(\.openURL) private var openURL
  let ad: SessionAdInfo
  var compact = false

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
  @State private var isShowingReportConfirmation = false

  var body: some View {
    if !didSendFinish {
      VStack(alignment: .leading, spacing: compact ? 7 : 10) {
        HStack(spacing: 8) {
          Image(systemName: "play.rectangle.fill")
            .foregroundStyle(.orange)
          Text("Advertisement")
            .font(OpenNOWFont.overline)
            .foregroundStyle(.secondary)

          Spacer(minLength: 8)

          Button {
            isShowingReportConfirmation = true
          } label: {
            Label("Report Ad", systemImage: "exclamationmark.bubble")
              .font((compact ? Font.caption2 : Font.caption).weight(.semibold))
              .lineLimit(1)
              .minimumHitTarget()
          }
          .buttonStyle(.plain)
          .foregroundStyle(.secondary)
          .accessibilityHint("Opens NVIDIA Support after confirmation")
        }

        Group {
          if let mediaUrl = preferredMediaURLString(for: ad), let url = URL(string: mediaUrl) {
            ZStack(alignment: .bottom) {
              AdVideoView(player: player)
                .aspectRatio(16.0 / 9.0, contentMode: .fit)
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
                    .frame(width: OpenNOWSpacing.hitTarget, height: OpenNOWSpacing.hitTarget)
                    .background(.ultraThinMaterial, in: Circle())
                }
                .accessibilityLabel(isPlaying ? "Pause ad" : "Play ad")

                Spacer()

                Button {
                  toggleMute()
                } label: {
                  Image(systemName: isMuted ? "speaker.slash.fill" : "speaker.wave.2.fill")
                    .font(.caption.bold())
                    .foregroundStyle(.white)
                    .frame(width: OpenNOWSpacing.hitTarget, height: OpenNOWSpacing.hitTarget)
                    .background(.ultraThinMaterial, in: Circle())
                }
                .accessibilityLabel(isMuted ? "Unmute ad" : "Mute ad")
              }
              .padding(8)
            }
            .onAppear {
              configurePlayer(url: url)
            }
            .onChangeCompat(of: ad.adId) { _ in
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
            .font(compact ? .caption2 : .caption)
            .foregroundStyle(.secondary)
            .lineLimit(compact ? 2 : nil)
            .minimumScaleFactor(0.82)
        }
      }
      .padding(compact ? 9 : 12)
      .background(
        RoundedRectangle(cornerRadius: 14)
          .fill(.regularMaterial)
          .overlay(
            RoundedRectangle(cornerRadius: 14)
              .stroke(.orange.opacity(0.28), lineWidth: 1)
          )
      )
      .frame(maxWidth: .infinity)
      .confirmationDialog(
        "Report this ad?",
        isPresented: $isShowingReportConfirmation,
        titleVisibility: .visible
      ) {
        Button("Open NVIDIA Support") {
          if let supportURL = URL(string: "https://www.nvidia.com/en-us/support/consumer/") {
            openURL(supportURL)
          }
        }
        Button("Cancel", role: .cancel) {}
      } message: {
        Text(
          "Open NVIDIA Support to report this queue ad. OpenNOW does not automatically send the ad or your account details."
        )
      }
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

}

private struct StreamActionButtonStyleModifier: ViewModifier {
  let tint: Color

  func body(content: Content) -> some View {
    content
      .font(.subheadline.bold())
      .foregroundStyle(tint)
      .padding(.horizontal, 18)
      .padding(.vertical, 11)
      .frame(maxWidth: .infinity)
      .background(
        Capsule()
          .fill(.regularMaterial)
          .overlay(Capsule().fill(tint.opacity(0.10)))
          .overlay(Capsule().stroke(tint.opacity(0.32), lineWidth: 1))
      )
  }
}

extension View {
  fileprivate func streamActionButtonStyle(tint: Color = .primary) -> some View {
    modifier(StreamActionButtonStyleModifier(tint: tint))
  }
}
