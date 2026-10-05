import AVKit
import SwiftUI

struct StreamLoadingView: View {
  @EnvironmentObject private var store: OpenNOWStore
  var coversBottomBar = false

  private var queueStatusText: String {
    if let position = store.displayQueuePosition {
      return "Queue position \(position)"
    }
    if let adState = store.effectiveAdState, store.activeQueueAd != nil {
      if adState.opportunity?.queuePaused == true || adState.isQueuePaused == true {
        return adState.message ?? "Queue paused"
      }
      if adState.sessionAdsRequired ?? adState.isAdsRequired {
        return adState.message ?? "Advertisement required"
      }
    }
    guard let session = store.activeSession else {
      return store.isLaunchingSession ? "Starting session" : "Waiting in queue"
    }
    switch session.status {
    case 2: return "Setting up gaming rig"
    case 3: return "Launching stream"
    default: return store.isLaunchingSession ? "Starting session" : "Waiting in queue"
    }
  }

  var body: some View {
    GeometryReader { proxy in
      ZStack {
        // Deliberately no longer passed the queue position: it changed on every poll and
        // invalidated a full-screen gradient for a difference nobody could see.
        QueueAmbientBackdrop(accent: brandAccent)
          .ignoresSafeArea(edges: ignoredBackgroundEdges)

        let topInset = max(18, proxy.safeAreaInsets.top + 10)
        let bottomInset = max(18, proxy.safeAreaInsets.bottom + 10)
        ScrollView {
          Group {
            if let ad = store.activeQueueAd {
              queueAdPanel(ad: ad, isWide: proxy.size.width > proxy.size.height)
            } else {
              queueStatusPanel
            }
          }
          .padding(18)
          .padding(.top, topInset)
          .padding(.bottom, bottomInset)
          .frame(maxWidth: .infinity)
          .frame(minHeight: max(0, proxy.size.height - topInset - bottomInset))
          // Scoped to the panel rather than the whole screen. An implicit animation applies to
          // everything beneath it, and at the root it was reaching the ambient backdrop and the
          // progress bar — both of which derive their position from the clock and stutter the
          // moment something re-interpolates it.
          .animation(
            .spring(response: 0.34, dampingFraction: 0.84),
            value: store.displayQueuePosition
          )
          .animation(.easeInOut(duration: 0.22), value: queueStatusText)
        }
        #if os(iOS)
          .scrollBounceBehavior(.basedOnSize)
        #endif
      }
    }
    .environment(\.colorScheme, .dark)
    .task(id: store.activeSession?.id) {
      guard store.activeSession != nil else { return }
      await NotificationManager.shared.requestPermission()
    }
  }

  private var ignoredBackgroundEdges: Edge.Set {
    coversBottomBar ? .all : [.top, .leading, .trailing]
  }

  private var queueStatusPanel: some View {
    VStack(spacing: 0) {
      queueGameArtwork
      Spacer().frame(height: 16)
      Text(store.activeSession?.game.title ?? "Starting stream")
        .font(.title3.weight(.bold))
        .foregroundStyle(.primary)
        .multilineTextAlignment(.center)
        .lineLimit(2)
        .minimumScaleFactor(0.78)

      AndroidQueueStatusText(
        text: queueStatusText,
        position: store.displayQueuePosition,
        compact: false
      )

      QueueTrendStrip(trend: store.queueTrend)
        .padding(.top, 10)

      Spacer().frame(height: 18)
      OscillatingQueueProgressView()
        .frame(maxWidth: 430)
      Spacer().frame(height: 12)
      actionButtons
        .frame(maxWidth: 430)
      queueError
    }
    .frame(maxWidth: 620)
  }

  private func queueAdPanel(ad: SessionAdInfo, isWide: Bool) -> some View {
    VStack(spacing: isWide ? 10 : 12) {
      HStack(spacing: 12) {
        VStack(alignment: .leading, spacing: 2) {
          Text("Advertisement")
            .font(.caption.weight(.bold))
            .foregroundStyle(.secondary)
          Text(store.activeSession?.game.title ?? "Starting stream")
            .font((isWide ? Font.headline : .title3).weight(.bold))
            .lineLimit(1)
            .minimumScaleFactor(0.75)
        }
        Spacer(minLength: 8)
        if let position = store.displayQueuePosition {
          Text(String(position))
            .font(.title2.weight(.black))
            .monospacedDigit()
            .foregroundStyle(queueUrgencyColor(position))
            .numericQueueTransition(value: position)
        }
      }

      QueueAdPlayerCard(ad: ad, compact: isWide)
        .environmentObject(store)

      actionButtons
      queueError
    }
    .padding(isWide ? 14 : 16)
    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    .overlay {
      RoundedRectangle(cornerRadius: 18, style: .continuous)
        .stroke(Color.white.opacity(0.10), lineWidth: 1)
    }
    .frame(maxWidth: isWide ? 720 : 620)
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
    .frame(width: 220)
    .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
    .overlay(
      RoundedRectangle(cornerRadius: 14, style: .continuous)
        .stroke(Color.white.opacity(0.1), lineWidth: 1)
    )
  }

  private var actionButtons: some View {
    HStack(spacing: 10) {
      Button {
        Haptics.light()
        store.minimizeQueueOverlay()
      } label: {
        Text("Minimize").frame(maxWidth: .infinity)
      }
      .buttonStyle(.bordered)
      .buttonBorderShape(.roundedRectangle(radius: 12))

      Button(role: .destructive) {
        Haptics.medium()
        Task { await store.endSession() }
      } label: {
        Text("Cancel").frame(maxWidth: .infinity)
      }
      .buttonStyle(.bordered)
      .buttonBorderShape(.roundedRectangle(radius: 12))
      .tint(.red)
    }
  }

  @ViewBuilder private var queueError: some View {
    if let error = store.sessionError, !error.isEmpty {
      Text(error)
        .font(.footnote)
        .foregroundStyle(Color(red: 1, green: 0.62, blue: 0.62))
        .multilineTextAlignment(.center)
        .padding(.top, 12)
    }
  }

  private func queueUrgencyColor(_ position: Int) -> Color {
    let heat = position >= 10 ? 0 : Double(10 - max(1, position)) / 9
    guard heat > 0 else { return .secondary }
    return Color(
      red: 1,
      green: max(0.06, 0.57 - 0.49 * heat),
      blue: max(0.08, 0.25 - 0.17 * heat)
    )
  }
}

/// One line under the queue numeral saying whether the queue is moving and roughly how long is
/// left. There was a sparkline here too; it was noise — the sentence is the part that answers
/// "should I wait?", and a twenty-two point chart of six samples was decoration.
///
/// Renders nothing until the estimator has seen enough to be honest. An empty line for the first
/// minute is better than a confident wrong number, and the layout does not shift when it appears
/// because the numeral above it is the anchor.
private struct QueueTrendStrip: View {
  let trend: QueueTrendEstimator

  @Environment(\.openNowAccent) private var accent
  @Environment(\.accessibilityReduceMotion) private var reduceMotion

  private var supportLine: String? { trend.supportLine() }

  var body: some View {
    if let supportLine {
      Text(supportLine)
        .font(.footnote.weight(.medium))
        .foregroundStyle(tint)
        .monospacedDigit()
        .multilineTextAlignment(.center)
        .frame(maxWidth: .infinity)
      .accessibilityElement(children: .ignore)
      .accessibilityLabel("Queue trend")
      .accessibilityValue(supportLine)
      .transition(reduceMotion ? .opacity : .opacity.combined(with: .move(edge: .top)))
      .animation(.easeInOut(duration: 0.24), value: supportLine)
    }
  }

  private var tint: Color {
    switch trend.trend() {
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

private struct QueueAdVideoFrame: ViewModifier {
  let compact: Bool

  @ViewBuilder
  func body(content: Content) -> some View {
    if compact {
      content.aspectRatio(16.0 / 9.0, contentMode: .fit)
    } else {
      content.frame(height: 220)
    }
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
          Text("Ad Queue")
            .font((compact ? Font.caption2 : Font.caption).bold())
            .foregroundStyle(.secondary)

          Spacer(minLength: 8)

          Button {
            isShowingReportConfirmation = true
          } label: {
            Label("Report Ad", systemImage: "exclamationmark.bubble")
              .font((compact ? Font.caption2 : Font.caption).weight(.semibold))
              .lineLimit(1)
          }
          .buttonStyle(.plain)
          .foregroundStyle(.secondary)
          .accessibilityHint("Opens NVIDIA Support after confirmation")
        }

        Group {
          if let mediaUrl = preferredMediaURLString(for: ad), let url = URL(string: mediaUrl) {
            ZStack(alignment: .bottom) {
              AdVideoView(player: player)
                .modifier(QueueAdVideoFrame(compact: compact))
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
