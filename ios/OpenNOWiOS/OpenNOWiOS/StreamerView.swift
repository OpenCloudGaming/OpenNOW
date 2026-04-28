#if canImport(WebKit)
import SwiftUI
import UIKit
import WebKit
import OSLog
import AVFoundation
import GameController

struct StreamerView: View {
    let session: ActiveSession
    let settings: AppSettings
    let onTouchLayoutChange: (String, TouchControlLayout) -> Void
    let onStreamerPreferencesChange: (StreamerPreferences) -> Void
    let onClose: () -> Void
    var onRetry: (() -> Void)? = nil
    private let logger = Logger(subsystem: "OpenNOWiOS", category: "StreamerView")
    @State private var statusText = ""
    @State private var latestStatusLine = "Initializing streamer..."
    @State private var isPeerConnected = false

    private var isShowingConnectionOverlay: Bool {
        !isPeerConnected
    }

    var body: some View {
        ZStack(alignment: .topTrailing) {
            StreamerWebView(
                session: session,
                settings: settings,
                onTouchLayoutChange: onTouchLayoutChange,
                onStreamerPreferencesChange: onStreamerPreferencesChange
            ) { event in
                logger.info("Streamer event: \(event, privacy: .public)")
                statusText = event
                if event.hasPrefix("Status: ") {
                    latestStatusLine = String(event.dropFirst("Status: ".count))
                    if latestStatusLine.localizedCaseInsensitiveContains("peer: connected") {
                        isPeerConnected = true
                    }
                }
                if event.localizedCaseInsensitiveContains("peer: connected") {
                    isPeerConnected = true
                }
                if event.hasPrefix("Error:") {
                    isPeerConnected = false
                }
            }
            .ignoresSafeArea()

            if isShowingConnectionOverlay {
                ZStack {
                    Color.black.opacity(0.72)
                        .ignoresSafeArea()

                    VStack(spacing: 14) {
                        if statusText.hasPrefix("Error:") {
                            Image(systemName: "exclamationmark.triangle.fill")
                                .font(.system(size: 26, weight: .semibold))
                                .foregroundStyle(.orange)
                        } else {
                            ProgressView()
                                .progressViewStyle(.circular)
                                .scaleEffect(1.25)
                                .tint(.white)
                        }

                        Text(statusText.hasPrefix("Error:") ? "Connection issue" : "Connecting to stream...")
                            .font(.headline.weight(.semibold))
                            .foregroundStyle(.white)

                        Text(statusText.hasPrefix("Error:") ? statusText.replacingOccurrences(of: "Error: ", with: "") : latestStatusLine)
                            .font(.subheadline)
                            .foregroundStyle(.white.opacity(0.85))
                            .multilineTextAlignment(.center)
                            .lineLimit(3)
                            .padding(.horizontal, 12)

                        if statusText.hasPrefix("Error:"), let retry = onRetry {
                            Button("Retry") {
                                retry()
                            }
                            .buttonStyle(.borderedProminent)
                            .tint(.white.opacity(0.22))
                            .foregroundStyle(.white)
                            .padding(.top, 4)
                        }
                    }
                    .padding(.horizontal, 20)
                    .padding(.vertical, 18)
                    .frame(maxWidth: 340)
                    .background(
                        RoundedRectangle(cornerRadius: 16, style: .continuous)
                            .fill(.ultraThinMaterial)
                            .overlay(
                                RoundedRectangle(cornerRadius: 16, style: .continuous)
                                    .stroke(Color.white.opacity(0.15), lineWidth: 1)
                            )
                    )
                    .padding(.horizontal, 20)
                }
                .transition(.opacity)
            }

            Button {
                onClose()
            } label: {
                Image(systemName: "xmark")
                    .font(.headline.weight(.bold))
                    .foregroundStyle(.white.opacity(0.9))
                    .frame(width: 36, height: 36)
                    .background(
                        Group {
                            if #available(iOS 26, *) {
                                Circle()
                                    .fill(.regularMaterial)
                                    .glassEffect(in: Circle())
                            } else {
                                Circle()
                                    .fill(.regularMaterial)
                                    .overlay(
                                        Circle()
                                            .stroke(Color.white.opacity(0.22), lineWidth: 1)
                                    )
                            }
                        }
                    )
            }
            .padding(.top, 12)
            .padding(.trailing, 12)

            if statusText.hasPrefix("Error:") {
                VStack {
                    Spacer()
                    Text(statusText)
                        .font(.caption.weight(.semibold))
                        .padding(.horizontal, 10)
                        .padding(.vertical, 6)
                        .background(Color.red.opacity(0.2), in: Capsule())
                        .overlay(
                            Capsule()
                                .stroke(Color.red.opacity(0.45), lineWidth: 1)
                        )
                        .foregroundStyle(.white)
                        .padding(.bottom, 22)
                }
            }
        }
        .background(Color.black.ignoresSafeArea())
        .onAppear {
            do {
                try AVAudioSession.sharedInstance().setCategory(
                    .playback,
                    mode: .moviePlayback,
                    options: [.mixWithOthers]
                )
                try AVAudioSession.sharedInstance().setActive(true)
            } catch {
            }
        }
        .onDisappear {
            try? AVAudioSession.sharedInstance().setActive(
                false,
                options: .notifyOthersOnDeactivation
            )
        }
    }
}

private struct StreamerWebView: UIViewRepresentable {
    let session: ActiveSession
    let settings: AppSettings
    let onTouchLayoutChange: (String, TouchControlLayout) -> Void
    let onStreamerPreferencesChange: (StreamerPreferences) -> Void
    let onEvent: (String) -> Void
    private static let desktopLikeUserAgent =
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 " +
        "(KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36"

    private struct StreamProfile {
        let width: Int
        let height: Int
        let maxBitrateKbps: Int
    }

    func makeCoordinator() -> Coordinator {
        Coordinator(
            onEvent: onEvent,
            onTouchLayoutChange: onTouchLayoutChange,
            onStreamerPreferencesChange: onStreamerPreferencesChange
        )
    }

    func makeUIView(context: Context) -> WKWebView {
        let config = WKWebViewConfiguration()
        config.allowsInlineMediaPlayback = true
        config.mediaTypesRequiringUserActionForPlayback = []
        config.defaultWebpagePreferences.allowsContentJavaScript = true
        config.userContentController.add(context.coordinator, name: "opennow")
        let webView = WKWebView(frame: .zero, configuration: config)
        webView.navigationDelegate = context.coordinator
        webView.customUserAgent = Self.desktopLikeUserAgent
        webView.isOpaque = false
        webView.backgroundColor = .black
        webView.scrollView.isScrollEnabled = false
        let html = buildHTML(for: session, settings: settings)
        let baseURL = URL(string: "https://play.geforcenow.com")
        context.coordinator.cachedHTML = html
        context.coordinator.cachedBaseURL = baseURL
        context.coordinator.attach(webView: webView)
        webView.loadHTMLString(html, baseURL: baseURL)
        return webView
    }

    func updateUIView(_ uiView: WKWebView, context: Context) {}

    static func dismantleUIView(_ uiView: WKWebView, coordinator: Coordinator) {
        uiView.configuration.userContentController.removeScriptMessageHandler(forName: "opennow")
        coordinator.detach()
    }
    }

    private func buildHTML(for session: ActiveSession, settings: AppSettings) -> String {
        struct Bridge: Encodable {
            let sessionId: String
            let signalingServer: String
            let signalingUrl: String
            let iceServers: [IceServerConfig]
            let serverIp: String
            let mediaIp: String?
            let mediaPort: Int
            let preferredCodec: String
            let fps: Int
            let maxBitrateKbps: Int
            let width: Int
            let height: Int
            let showStatsOverlay: Bool
            let gameTitle: String
            let touchProfile: String
            let touchLayout: TouchControlLayout
            let streamerPreferences: StreamerPreferences
            let allowNativeTouchPassthrough: Bool
        }

        let signalingServer = session.signalingServer ?? session.serverIp ?? URL(string: session.streamingBaseUrl)?.host ?? ""
        let signalingUrl = session.signalingUrl ?? "wss://\(signalingServer):443/nvst/"
        let serverIp = session.serverIp ?? signalingServer
        let profile = Self.streamProfile(for: settings)
        let touchProfile = Self.touchProfile(for: session.game.title)
        let bridge = Bridge(
            sessionId: session.id,
            signalingServer: signalingServer,
            signalingUrl: signalingUrl,
            iceServers: session.iceServers,
            serverIp: serverIp,
            mediaIp: session.mediaIp,
            mediaPort: session.mediaPort,
            preferredCodec: Self.normalizePreferredCodec(settings.preferredCodec),
            fps: min(settings.preferredFPS, 60),
            maxBitrateKbps: profile.maxBitrateKbps,
            width: profile.width,
            height: profile.height,
            showStatsOverlay: settings.showStatsOverlay,
            gameTitle: session.game.title,
            touchProfile: touchProfile,
            touchLayout: settings.touchLayout(for: touchProfile),
            streamerPreferences: settings.streamerPreferences,
            allowNativeTouchPassthrough: touchProfile == "fortnite-mobile" && settings.fortnitePrefersNativeTouch
        )
        let data = (try? JSONEncoder().encode(bridge)) ?? Data("{}".utf8)
        let payload = String(data: data, encoding: .utf8) ?? "{}"
        return #"""
<!doctype html>
<html>
<head>
  <meta name="viewport" content="width=device-width,initial-scale=1,maximum-scale=1,user-scalable=no">
  <style>
    html,body{margin:0;padding:0;background:#000;width:100%;height:100%;overflow:hidden}
    #video{position:fixed;inset:0;width:100%;height:100%;object-fit:contain;background:#000}
    #hudToggle{position:fixed;right:max(6px,calc(env(safe-area-inset-right) + 6px));
      bottom:max(10px,calc(env(safe-area-inset-bottom) + 10px));z-index:36;display:inline-flex;
      align-items:center;justify-content:center;width:44px;height:44px;padding:0;border-radius:999px;
      border:1px solid rgba(255,255,255,0.18);background:rgba(16,16,20,0.72);color:#fff;cursor:pointer;
      font:600 18px -apple-system;backdrop-filter:blur(16px);-webkit-backdrop-filter:blur(16px);
      box-shadow:0 10px 28px rgba(0,0,0,0.34);}
    #hudToggle:hover,#hudToggle:active{background:rgba(22,22,28,0.84);}
    #hudToggleGlyph{display:inline-flex;align-items:center;justify-content:center;width:100%;height:100%;
      font-size:20px;line-height:1;}
    #hudPanel{position:fixed;right:max(10px,calc(env(safe-area-inset-right) + 10px));
      bottom:max(62px,calc(env(safe-area-inset-bottom) + 62px));z-index:35;width:min(332px,calc(100vw - 28px));
      max-height:min(72vh,520px);overflow:auto;padding:14px;border-radius:22px;
      color:#fff;background:rgba(18,18,22,0.74);border:1px solid rgba(255,255,255,0.14);
      font:12px -apple-system;transition:transform .22s ease,opacity .22s ease;opacity:0;
      transform:translateY(20px) scale(0.98);pointer-events:none;
      backdrop-filter:blur(14px);-webkit-backdrop-filter:blur(14px);}
    #hudPanel.open{transform:translateY(0) scale(1);opacity:1;pointer-events:auto;}
    .hudHeader{display:flex;gap:12px;align-items:flex-start;justify-content:space-between;margin-bottom:12px;}
    .hudHeaderTitle{display:flex;flex-direction:column;gap:4px;min-width:0;}
    .hudHeaderTitle strong{font-size:14px;}
    .hudHeaderTitle span{color:rgba(255,255,255,0.64);line-height:1.35;}
    .hudSection{margin-top:12px;padding-top:12px;border-top:1px solid rgba(255,255,255,0.1);}
    .hudSectionTitle{display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;}
    .hudSectionTitle strong{font-size:13px;}
    .hudSectionTitle span{color:rgba(255,255,255,0.55);}
    .toggleList{display:flex;flex-direction:column;gap:8px;}
    .toggleRow{display:flex;align-items:center;justify-content:space-between;gap:12px;width:100%;padding:11px 12px;
      border-radius:14px;border:1px solid rgba(255,255,255,0.14);background:rgba(255,255,255,0.06);color:#fff;
      font:12px -apple-system;cursor:pointer;text-align:left;}
    .toggleRow strong{display:block;font-size:12px;font-weight:600;}
    .toggleRow span{display:block;color:rgba(255,255,255,0.58);margin-top:2px;line-height:1.3;}
    .toggleRow .toggleValue{flex:0 0 auto;margin-top:0;color:rgba(255,255,255,0.92);font-weight:600;}
    .toggleRow.is-active{background:rgba(110,186,255,0.14);border-color:rgba(110,186,255,0.26);}
    .toggleRow.is-disabled{opacity:0.45;}
    .infoAction{width:100%;padding:9px 10px;border-radius:10px;border:1px solid rgba(255,255,255,0.24);
      background:rgba(255,255,255,0.08);color:#fff;font:12px -apple-system;font-weight:600;cursor:pointer;}
    .hudSummary{display:flex;gap:6px;flex-wrap:wrap;}
    .hudBadge{padding:5px 8px;border-radius:999px;background:rgba(255,255,255,0.08);color:rgba(255,255,255,0.72);
      border:1px solid rgba(255,255,255,0.1);font-size:11px;line-height:1;}
    .layoutPanel{margin-top:12px;padding-top:12px;border-top:1px solid rgba(255,255,255,0.1);}
    .layoutPanel label{display:block;margin-top:10px;color:rgba(255,255,255,0.82);}
    .layoutPanel input[type=range]{width:100%;margin-top:6px;}
    .layoutHint{margin-top:8px;line-height:1.4;color:rgba(255,255,255,0.62);}
    #gpPad.layoutEditing .layoutGroup{outline:1px dashed rgba(120,210,255,0.9);background:rgba(120,210,255,0.1);border-radius:18px;}
    #gpPad.layoutEditing .layoutGroup::after{content:'Drag';position:absolute;left:50%;top:-18px;transform:translateX(-50%);
      padding:2px 7px;border-radius:999px;background:rgba(10,10,12,0.84);border:1px solid rgba(120,210,255,0.55);
      color:#c8f2ff;font:600 10px -apple-system;letter-spacing:0.02em;pointer-events:none;}
    .layoutGroup{position:absolute;pointer-events:auto;touch-action:none;transform:translate(-50%,-50%);}
  </style>
</head>
<body>
  <video id="video" playsinline autoplay muted></video>
  <div id="stats" style="position:fixed;left:12px;top:12px;z-index:30;display:flex;flex-direction:column;gap:6px;
    min-width:244px;padding:10px 12px;color:#eef7ee;background:rgba(0,0,0,0.58);border:1px solid rgba(255,255,255,0.15);
    border-radius:14px;font:12px -apple-system;backdrop-filter:blur(10px);-webkit-backdrop-filter:blur(10px);">
    <div id="statsPrimary">FPS -- | Ping -- ms | Loss -- | Rate -- Mbps</div>
    <div id="statsMeta" style="display:none;color:rgba(255,255,255,0.7);font-size:11px;"></div>
  </div>
  <div id="touchpad" style="position:fixed;inset:0;z-index:10;touch-action:none;"></div>
  <div id="touchHint" style="position:fixed;left:50%;bottom:60px;transform:translateX(-50%);
    color:rgba(255,255,255,0.45);font:11px -apple-system;pointer-events:none;user-select:none;
    text-align:center;transition:opacity 1s;">Drag to move · Tap to click · 2-finger tap for right click</div>
  <button id="hudToggle" onclick="toggleHudPanel()" aria-label="Toggle stream controls">
    <span id="hudToggleGlyph">&#9881;</span>
  </button>
  <aside id="hudPanel">
    <div class="hudHeader">
      <div class="hudHeaderTitle">
        <strong>Stream Controls</strong>
        <span>One compact panel for display, input, and overlay tuning.</span>
      </div>
      <button onclick="toggleHudPanel(false)" style="border:none;background:transparent;color:#fff;font-size:16px;line-height:1;cursor:pointer;">×</button>
    </div>
    <div class="hudSummary" id="hudSummary">
      <span class="hudBadge" id="hudStatsBadge">Stats Off</span>
      <span class="hudBadge" id="hudInputBadge">Touchpad</span>
      <span class="hudBadge" id="hudAudioBadge">Audio Muted</span>
    </div>
    <div class="hudSection">
      <div class="hudSectionTitle">
        <strong>Display</strong>
        <span>Overlay and playback</span>
      </div>
      <div class="toggleList">
        <button id="audioBtn" class="toggleRow" onclick="toggleAudio()">
          <div><strong>Audio</strong><span>Mute or unmute the stream audio.</span></div>
          <span class="toggleValue" id="audioValue">Muted</span>
        </button>
        <button id="statsBtn" class="toggleRow" onclick="toggleStatsOverlay()">
          <div><strong>Stream Stats</strong><span>Show FPS, ping, packet loss, bitrate, and optional extras.</span></div>
          <span class="toggleValue" id="statsValue">Off</span>
        </button>
        <button id="clockBtn" class="toggleRow" onclick="toggleStatsClock()">
          <div><strong>Clock</strong><span>Include the current local time in the stats card.</span></div>
          <span class="toggleValue" id="clockValue">Off</span>
        </button>
        <button id="batteryBtn" class="toggleRow" onclick="toggleStatsBattery()">
          <div><strong>iPhone Battery</strong><span>Show current battery level and charging state in the stats card.</span></div>
          <span class="toggleValue" id="batteryValue">Off</span>
        </button>
      </div>
    </div>
    <div class="hudSection">
      <div class="hudSectionTitle">
        <strong>Input</strong>
        <span id="controllerState">Controller: waiting...</span>
      </div>
      <div class="toggleList">
        <button id="kbBtn" class="toggleRow" onclick="toggleKeyboard()">
          <div><strong>Keyboard</strong><span>Open or hide the on-screen keyboard bar.</span></div>
          <span class="toggleValue" id="kbValue">Hidden</span>
        </button>
        <button id="gpBtn" class="toggleRow" onclick="toggleGamepad()">
          <div><strong>Touch Controller</strong><span id="touchModeDescription">Touchpad is active. Optional touch controls stay hidden until needed.</span></div>
          <span class="toggleValue" id="gpValue">Hidden</span>
        </button>
        <button id="physicalControllerBtn" class="toggleRow" onclick="togglePhysicalControllerPassthrough()">
          <div><strong>Bluetooth Controller</strong><span>Pass connected controllers through as native GFN gamepad packets.</span></div>
          <span class="toggleValue" id="physicalControllerValue">On</span>
        </button>
      </div>
    </div>
    <div class="layoutPanel">
      <div class="hudSectionTitle">
        <strong>Touch Layout</strong>
        <button id="gpEditBtn" class="infoAction" style="width:auto;padding:8px 12px;">Edit Layout</button>
      </div>
      <label for="gpScaleRange">Control Size <span id="gpScaleValue">100%</span></label>
      <input id="gpScaleRange" type="range" min="70" max="160" step="1" value="100">
      <label for="gpButtonRange">Button Size <span id="gpButtonValue">100%</span></label>
      <input id="gpButtonRange" type="range" min="75" max="150" step="1" value="100">
      <label for="gpStickRange">Stick Size <span id="gpStickValue">100%</span></label>
      <input id="gpStickRange" type="range" min="75" max="150" step="1" value="100">
      <label for="gpOpacityRange">Control Opacity <span id="gpOpacityValue">58%</span></label>
      <input id="gpOpacityRange" type="range" min="15" max="100" step="1" value="58">
      <div style="display:flex;gap:8px;margin-top:10px;">
        <button id="gpResetBtn" class="infoAction">Reset Layout</button>
      </div>
      <div class="layoutHint" id="layoutHint">Turn on edit mode to drag each touch-control cluster and resize the whole layout.</div>
    </div>
  </aside>
  <div id="kbBar" style="display:none;position:fixed;bottom:0;left:0;right:0;z-index:30;
    background:rgba(20,20,20,0.92);backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);
    padding:8px 12px;border-top:1px solid rgba(255,255,255,0.1);">
    <div style="display:flex;gap:8px;align-items:center;">
      <input id="kbInput" type="text" autocomplete="off" autocorrect="off" autocapitalize="none"
        spellcheck="false" placeholder="Type here…"
        style="flex:1;background:#2a2a2a;color:#fff;border:1px solid rgba(255,255,255,0.2);
          border-radius:8px;padding:8px 12px;font-size:16px;outline:none;">
      <button onclick="hideKeyboard()" style="padding:8px 14px;background:#333;color:#fff;
        border:none;border-radius:8px;font-size:14px;cursor:pointer;">Done</button>
    </div>
  </div>
  <div id="gpPad" style="display:none;position:fixed;left:0;right:0;top:0;bottom:12px;z-index:25;pointer-events:none;">
    <div id="gpTopLeft" class="layoutGroup">
      <div style="display:flex;gap:8px;pointer-events:auto;">
        <button data-mask="256" class="gpAux gpShoulder">LB</button>
        <button data-trigger="left" class="gpAux gpTrigger">LT</button>
      </div>
    </div>
    <div id="gpTopCenter" class="layoutGroup">
      <div style="display:flex;gap:8px;pointer-events:auto;">
        <button data-mask="32" class="gpAux gpSmall">View</button>
        <button data-mask="16" class="gpAux gpSmall">Menu</button>
      </div>
    </div>
    <div id="gpTopRight" class="layoutGroup">
      <div style="display:flex;gap:8px;pointer-events:auto;">
        <button data-trigger="right" class="gpAux gpTrigger">RT</button>
        <button data-mask="512" class="gpAux gpShoulder">RB</button>
      </div>
    </div>
    <div id="gpLeftStick" class="layoutGroup">
      <div style="display:flex;flex-direction:column;align-items:center;gap:8px;pointer-events:auto;">
      <div id="joyBase" style="position:relative;width:146px;height:146px;border-radius:50%;
        touch-action:none;background:rgba(20,20,20,0.72);
        border:1px solid rgba(255,255,255,0.2);backdrop-filter:blur(8px);-webkit-backdrop-filter:blur(8px);
        box-shadow:inset 0 0 0 1px rgba(255,255,255,0.08);">
        <div id="joyStick" style="position:absolute;left:50%;top:50%;width:62px;height:62px;border-radius:50%;
          transform:translate(-50%,-50%);background:rgba(255,255,255,0.18);
          border:1px solid rgba(255,255,255,0.35);box-shadow:0 6px 18px rgba(0,0,0,0.3);"></div>
      </div>
      <button data-mask="64" class="gpAux gpSmall">L3</button>
      </div>
    </div>
    <div id="gpRightCluster" class="layoutGroup">
      <div style="display:flex;align-items:flex-end;gap:18px;pointer-events:auto;">
      <div style="display:flex;flex-direction:column;align-items:center;gap:8px;">
        <button data-mask="1024" class="gpAux gpSmall">Home</button>
        <div id="lookBase" style="position:relative;width:124px;height:124px;border-radius:50%;pointer-events:auto;
          touch-action:none;background:rgba(18,18,20,0.46);border:1px solid rgba(255,255,255,0.16);
          backdrop-filter:blur(8px);-webkit-backdrop-filter:blur(8px);">
          <div id="lookStick" style="position:absolute;left:50%;top:50%;width:48px;height:48px;border-radius:50%;
            transform:translate(-50%,-50%);background:rgba(255,255,255,0.14);
            border:1px solid rgba(255,255,255,0.22);"></div>
        </div>
        <button data-mask="128" class="gpAux gpSmall">R3</button>
      </div>
      <div style="display:grid;grid-template-columns:56px 56px;grid-template-rows:56px 56px;gap:8px;pointer-events:auto;">
        <button data-mask="16384" class="gpKey">X</button>
        <button data-mask="32768" class="gpKey">Y</button>
        <button data-mask="4096" class="gpKey">A</button>
        <button data-mask="8192" class="gpKey">B</button>
      </div>
      </div>
    </div>
    <div id="gpBottomCenter" class="layoutGroup">
      <div style="display:flex;justify-content:center;pointer-events:auto;">
      <button id="gpHide" style="padding:8px 12px;border-radius:999px;background:rgba(20,20,20,0.82);
        color:#fff;border:1px solid rgba(255,255,255,0.25);font-size:12px;">Hide touch controls</button>
      </div>
    </div>
  </div>
  <script>
  const cfg = \#(payload);
  function sanitizeStreamerPreferences(raw) {
    return {
      audioMuted: raw?.audioMuted !== false,
      showStatsClock: raw?.showStatsClock === true,
      showStatsBattery: raw?.showStatsBattery === true,
      touchControllerVisible: raw?.touchControllerVisible === true,
      physicalControllerPassthrough: raw?.physicalControllerPassthrough !== false
    };
  }
  let streamerPreferences = sanitizeStreamerPreferences(cfg.streamerPreferences);
  const video = document.getElementById("video");
  let ws = null;
  let pc = null;
  let ack = 0;
  let hb = null;
  let hbInput = null;
  let reliableCh = null;
  let partialCh = null;
  let inputReady = false;
  let reconnectTimer = null;
  let reconnectAttempts = 0;
  const maxReconnectAttempts = 10;
  let offerTimeoutTimer = null;
  let signalingOpenTimeout = null;
  let statsTimer = null;
  let lastBytesReceived = 0;
  let lastBytesTimestamp = 0;
  let pendingMoveDx = 0;
  let pendingMoveDy = 0;
  let moveFrame = null;
  const INPUT_PROTOCOL_FALLBACK_DELAY_MS = 1500;
  const DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS = 100;
  const DEFAULT_HID_DEVICE_MASK = 0xFFFFFFFF;
  const DEFAULT_PR_GAMEPAD_MASK = 0xF;
  const DEFAULT_PR_HID_MASK = 0xFFFFFFFF;
  const RI_INPUT_MOUSE_REL_MASK = 1 << 7;
  const peerId = 2;
  const peerName = "peer-" + Math.floor(Math.random() * 1e10);
  const statsEl = document.getElementById('stats');
  const statsPrimary = document.getElementById('statsPrimary');
  const statsMeta = document.getElementById('statsMeta');
  const hudPanel = document.getElementById('hudPanel');
  const hudToggle = document.getElementById('hudToggle');
  const hudToggleGlyph = document.getElementById('hudToggleGlyph');
  const hudStatsBadge = document.getElementById('hudStatsBadge');
  const hudInputBadge = document.getElementById('hudInputBadge');
  const hudAudioBadge = document.getElementById('hudAudioBadge');
  const audioBtn = document.getElementById('audioBtn');
  const audioValue = document.getElementById('audioValue');
  const statsBtn = document.getElementById('statsBtn');
  const statsValue = document.getElementById('statsValue');
  const clockBtn = document.getElementById('clockBtn');
  const clockValue = document.getElementById('clockValue');
  const batteryBtn = document.getElementById('batteryBtn');
  const batteryValue = document.getElementById('batteryValue');
  const kbBtn = document.getElementById('kbBtn');
  const kbValue = document.getElementById('kbValue');
  const gpValue = document.getElementById('gpValue');
  const physicalControllerBtn = document.getElementById('physicalControllerBtn');
  const physicalControllerValue = document.getElementById('physicalControllerValue');
  const controllerState = document.getElementById('controllerState');
  const touchModeDescription = document.getElementById('touchModeDescription');
  const gpEditBtn = document.getElementById('gpEditBtn');
  const gpResetBtn = document.getElementById('gpResetBtn');
  const gpScaleRange = document.getElementById('gpScaleRange');
  const gpScaleValue = document.getElementById('gpScaleValue');
  const gpButtonRange = document.getElementById('gpButtonRange');
  const gpButtonValue = document.getElementById('gpButtonValue');
  const gpStickRange = document.getElementById('gpStickRange');
  const gpStickValue = document.getElementById('gpStickValue');
  const gpOpacityRange = document.getElementById('gpOpacityRange');
  const gpOpacityValue = document.getElementById('gpOpacityValue');
  const layoutHint = document.getElementById('layoutHint');
  const remoteMediaStream = new MediaStream();
  let inputProtocolVersion = 2;
  let inputHandshakeComplete = false;
  let inputFallbackTimer = null;
  let partialReliableThresholdMs = DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS;
  let riInputCapabilities = {
    hidDeviceMask: DEFAULT_HID_DEVICE_MASK,
    enablePartiallyReliableTransferGamepad: DEFAULT_PR_GAMEPAD_MASK,
    enablePartiallyReliableTransferHid: DEFAULT_PR_HID_MASK
  };
  let offerAccepted = false;
  let hudPanelOpen = false;
  let audioUnlocked = false;
  let userMuted = streamerPreferences.audioMuted;
  let audioRetryTimer = null;
  let statsVisible = !!cfg.showStatsOverlay;
  let showStatsClock = streamerPreferences.showStatsClock;
  let showStatsBattery = streamerPreferences.showStatsBattery;
  let deviceBatteryPercent = null;
  let deviceBatteryCharging = false;
  let latestStats = { fps: 0, pingMs: 0, packetLossPct: 0, bitrateMbps: 0 };
  let nativeGamepadState = null;
  let lastControllerStatusText = '';
  let lastStatsMarkup = '';
  const GAMEPAD_PACKET_SIZE = 38;
  const GAMEPAD_DEADZONE = 0.15;
  const GAMEPAD_KEEPALIVE_MS = 1000;
  const GAMEPAD_DPAD_UP = 0x0001;
  const GAMEPAD_DPAD_DOWN = 0x0002;
  const GAMEPAD_DPAD_LEFT = 0x0004;
  const GAMEPAD_DPAD_RIGHT = 0x0008;
  const GAMEPAD_START = 0x0010;
  const GAMEPAD_BACK = 0x0020;
  const GAMEPAD_LS = 0x0040;
  const GAMEPAD_RS = 0x0080;
  const GAMEPAD_LB = 0x0100;
  const GAMEPAD_RB = 0x0200;
  const GAMEPAD_GUIDE = 0x0400;
  const GAMEPAD_A = 0x1000;
  const GAMEPAD_B = 0x2000;
  const GAMEPAD_X = 0x4000;
  const GAMEPAD_Y = 0x8000;
  const TOUCH_LAYOUT_SCALE_MIN = 0.7;
  const TOUCH_LAYOUT_SCALE_MAX = 1.6;
  const TOUCH_LAYOUT_BUTTON_SCALE_MIN = 0.75;
  const TOUCH_LAYOUT_BUTTON_SCALE_MAX = 1.5;
  const TOUCH_LAYOUT_STICK_SCALE_MIN = 0.75;
  const TOUCH_LAYOUT_STICK_SCALE_MAX = 1.5;
  const TOUCH_LAYOUT_OPACITY_MIN = 0.15;
  const TOUCH_LAYOUT_OPACITY_MAX = 1.0;
  const FORTNITE_NATIVE_TOUCH = !!cfg.allowNativeTouchPassthrough;

  function post(type, message) {
    try { window.webkit.messageHandlers.opennow.postMessage({ type, message }); } catch (_) {}
  }
  function postPayload(type, payload) {
    try { window.webkit.messageHandlers.opennow.postMessage({ type, payload }); } catch (_) {}
  }
  function log(message) { post("log", message); }
  function fail(message) { post("error", message); }
  function defaultTouchLayoutForProfile(profile) {
    if (profile === 'fortnite-mobile') {
      return {
        scale: 1.05,
        opacity: 0.52,
        buttonScale: 1.05,
        stickScale: 1,
        topLeft: { x: 0.16, y: 0.11 },
        topCenter: { x: 0.50, y: 0.11 },
        topRight: { x: 0.84, y: 0.11 },
        leftStick: { x: 0.17, y: 0.81 },
        rightCluster: { x: 0.84, y: 0.78 },
        bottomCenter: { x: 0.50, y: 0.91 }
      };
    }
    return {
      scale: 1,
      opacity: 0.58,
      buttonScale: 1,
      stickScale: 1,
      topLeft: { x: 0.14, y: 0.12 },
      topCenter: { x: 0.50, y: 0.12 },
      topRight: { x: 0.86, y: 0.12 },
      leftStick: { x: 0.18, y: 0.80 },
      rightCluster: { x: 0.83, y: 0.79 },
      bottomCenter: { x: 0.50, y: 0.92 }
    };
  }
  function clamp01(value, min = 0.08, max = 0.92) {
    return Math.max(min, Math.min(max, Number.isFinite(value) ? value : 0.5));
  }
  function sanitizePoint(raw, fallback, minX = 0.08, maxX = 0.92, minY = 0.08, maxY = 0.94) {
    return {
      x: clamp01(raw?.x ?? fallback.x, minX, maxX),
      y: clamp01(raw?.y ?? fallback.y, minY, maxY)
    };
  }
  function sanitizeTouchLayout(raw) {
    const fallback = defaultTouchLayoutForProfile(cfg.touchProfile);
    return {
      scale: Math.max(TOUCH_LAYOUT_SCALE_MIN, Math.min(TOUCH_LAYOUT_SCALE_MAX, Number(raw?.scale ?? fallback.scale) || fallback.scale)),
      opacity: Math.max(TOUCH_LAYOUT_OPACITY_MIN, Math.min(TOUCH_LAYOUT_OPACITY_MAX, Number(raw?.opacity ?? fallback.opacity) || fallback.opacity)),
      buttonScale: Math.max(TOUCH_LAYOUT_BUTTON_SCALE_MIN, Math.min(TOUCH_LAYOUT_BUTTON_SCALE_MAX, Number(raw?.buttonScale ?? fallback.buttonScale) || fallback.buttonScale)),
      stickScale: Math.max(TOUCH_LAYOUT_STICK_SCALE_MIN, Math.min(TOUCH_LAYOUT_STICK_SCALE_MAX, Number(raw?.stickScale ?? fallback.stickScale) || fallback.stickScale)),
      topLeft: sanitizePoint(raw?.topLeft, fallback.topLeft),
      topCenter: sanitizePoint(raw?.topCenter, fallback.topCenter),
      topRight: sanitizePoint(raw?.topRight, fallback.topRight),
      leftStick: sanitizePoint(raw?.leftStick, fallback.leftStick, 0.10, 0.40, 0.45, 0.92),
      rightCluster: sanitizePoint(raw?.rightCluster, fallback.rightCluster, 0.60, 0.92, 0.42, 0.92),
      bottomCenter: sanitizePoint(raw?.bottomCenter, fallback.bottomCenter, 0.20, 0.80, 0.70, 0.97)
    };
  }
  let touchLayout = sanitizeTouchLayout(cfg.touchLayout);
  let layoutEditing = false;
  let touchLayoutPersistTimer = null;
  let streamerPreferencesPersistTimer = null;
  const touchGroupElements = {
    topLeft: document.getElementById('gpTopLeft'),
    topCenter: document.getElementById('gpTopCenter'),
    topRight: document.getElementById('gpTopRight'),
    leftStick: document.getElementById('gpLeftStick'),
    rightCluster: document.getElementById('gpRightCluster'),
    bottomCenter: document.getElementById('gpBottomCenter')
  };
  const layoutDragState = { key: null, pointerId: null, offsetX: 0, offsetY: 0 };
  function toggleHudPanel(forceOpen) {
    hudPanelOpen = typeof forceOpen === 'boolean' ? forceOpen : !hudPanelOpen;
    if (hudPanel) {
      hudPanel.classList.toggle('open', hudPanelOpen);
    }
    if (hudToggle) {
      hudToggle.setAttribute('aria-expanded', hudPanelOpen ? 'true' : 'false');
      hudToggle.setAttribute('aria-label', hudPanelOpen ? 'Close stream controls' : 'Open stream controls');
    }
    if (hudToggleGlyph) {
      hudToggleGlyph.innerHTML = hudPanelOpen ? '&times;' : '&#9881;';
    }
  }
  function setToggleRowState(element, isActive, isDisabled = false) {
    if (!element) return;
    element.classList.toggle('is-active', !!isActive);
    element.classList.toggle('is-disabled', !!isDisabled);
    if (isDisabled) {
      element.setAttribute('aria-disabled', 'true');
    } else {
      element.removeAttribute('aria-disabled');
    }
  }
  function updateHudSummary() {
    if (hudStatsBadge) {
      hudStatsBadge.textContent = statsVisible ? 'Stats On' : 'Stats Off';
    }
    if (hudInputBadge) {
      hudInputBadge.textContent = gpPad && gpPad.style.display !== 'none' ? 'Touch Controls' : 'Touchpad';
    }
    if (hudAudioBadge) {
      hudAudioBadge.textContent = userMuted || video.muted ? 'Audio Muted' : 'Audio On';
    }
  }
  function updateAudioButton() {
    if (audioValue) {
      audioValue.textContent = userMuted || video.muted ? 'Muted' : 'On';
    }
    setToggleRowState(audioBtn, !(userMuted || video.muted));
    updateHudSummary();
  }
  function updateStatsButton() {
    if (statsValue) {
      statsValue.textContent = statsVisible ? 'On' : 'Off';
    }
    setToggleRowState(statsBtn, statsVisible);
    const extrasDisabled = !statsVisible;
    if (clockValue) {
      clockValue.textContent = showStatsClock ? 'On' : 'Off';
    }
    if (batteryValue) {
      batteryValue.textContent = showStatsBattery ? 'On' : 'Off';
    }
    setToggleRowState(clockBtn, showStatsClock, extrasDisabled);
    setToggleRowState(batteryBtn, showStatsBattery, extrasDisabled);
    updateHudSummary();
  }
  function updateTouchControllerButton() {
    const visible = gpPad && gpPad.style.display !== 'none';
    const profileLabel = cfg.touchProfile === 'fortnite-mobile' ? 'Touch Controller Overlay' : 'Touch Controller';
    if (gpValue) {
      gpValue.textContent = visible ? 'Visible' : 'Hidden';
    }
    setToggleRowState(gpBtn, visible);
    updateHudSummary();
  }
  function updatePhysicalControllerButton() {
    const enabled = streamerPreferences.physicalControllerPassthrough;
    if (physicalControllerValue) {
      physicalControllerValue.textContent = enabled ? 'On' : 'Off';
    }
    setToggleRowState(physicalControllerBtn, enabled);
  }
  function togglePhysicalControllerPassthrough() {
    updateStreamerPreference('physicalControllerPassthrough', !streamerPreferences.physicalControllerPassthrough);
    updatePhysicalControllerButton();
    if (!streamerPreferences.physicalControllerPassthrough && nativeGamepadState?.connected) {
      sendCurrentGamepadState(null);
    }
  }
  function updateTouchModeCopy() {
    if (!touchModeDescription) return;
    if (FORTNITE_NATIVE_TOUCH) {
      touchModeDescription.textContent = 'Fortnite is using native mobile touch, and the overlay stays optional.';
      if (touchHint) {
        touchHint.textContent = 'Fortnite mobile touch mode is active.';
      }
      return;
    }
    touchModeDescription.textContent = 'Touchpad mode uses drag to move, tap to click, and two-finger tap for right click.';
    if (touchHint) {
      touchHint.textContent = 'Drag to move · Tap to click · 2-finger tap for right click';
    }
  }
  function applyTouchpadMode() {
    updateTouchModeCopy();
    if (!touchpad) return;
    if (FORTNITE_NATIVE_TOUCH) {
      touchpad.style.pointerEvents = 'none';
      touchpad.style.display = 'none';
    } else {
      touchpad.style.pointerEvents = 'auto';
      touchpad.style.display = 'block';
    }
  }
  function scheduleTouchLayoutPersist() {
    if (touchLayoutPersistTimer) {
      clearTimeout(touchLayoutPersistTimer);
    }
    touchLayoutPersistTimer = setTimeout(() => {
      touchLayoutPersistTimer = null;
      postPayload('touch-layout', { profile: cfg.touchProfile, layout: touchLayout });
    }, 140);
  }
  function scheduleStreamerPreferencesPersist() {
    if (streamerPreferencesPersistTimer) {
      clearTimeout(streamerPreferencesPersistTimer);
    }
    streamerPreferencesPersistTimer = setTimeout(() => {
      streamerPreferencesPersistTimer = null;
      postPayload('streamer-preferences', streamerPreferences);
    }, 140);
  }
  function updateStreamerPreference(key, value) {
    streamerPreferences = sanitizeStreamerPreferences({
      ...streamerPreferences,
      [key]: value
    });
    scheduleStreamerPreferencesPersist();
  }
  function applyTouchLayout() {
    const scale = touchLayout.scale;
    const buttonScale = touchLayout.buttonScale;
    const stickScale = touchLayout.stickScale;
    const overlayOpacity = layoutEditing ? Math.max(touchLayout.opacity, 0.55) : touchLayout.opacity;
    Object.entries(touchGroupElements).forEach(([key, element]) => {
      if (!element || !touchLayout[key]) return;
      const point = touchLayout[key];
      element.style.left = `${(point.x * 100).toFixed(2)}%`;
      element.style.top = `${(point.y * 100).toFixed(2)}%`;
      element.style.transform = `translate(-50%, -50%) scale(${scale})`;
      element.style.opacity = `${overlayOpacity}`;
    });
    if (gpPad) {
      gpPad.classList.toggle('layoutEditing', layoutEditing);
    }
    if (gpScaleRange) {
      gpScaleRange.value = String(Math.round(scale * 100));
    }
    if (gpScaleValue) {
      gpScaleValue.textContent = `${Math.round(scale * 100)}%`;
    }
    if (gpButtonRange) {
      gpButtonRange.value = String(Math.round(buttonScale * 100));
    }
    if (gpButtonValue) {
      gpButtonValue.textContent = `${Math.round(buttonScale * 100)}%`;
    }
    if (gpStickRange) {
      gpStickRange.value = String(Math.round(stickScale * 100));
    }
    if (gpStickValue) {
      gpStickValue.textContent = `${Math.round(stickScale * 100)}%`;
    }
    if (gpOpacityRange) {
      gpOpacityRange.value = String(Math.round(touchLayout.opacity * 100));
    }
    if (gpOpacityValue) {
      gpOpacityValue.textContent = `${Math.round(touchLayout.opacity * 100)}%`;
    }
    if (gpEditBtn) {
      gpEditBtn.textContent = layoutEditing ? 'Done Editing' : 'Edit Layout';
    }
    if (layoutHint) {
      layoutHint.textContent = layoutEditing
        ? 'Drag any highlighted touch-control cluster, then use the size slider if needed.'
        : 'Turn on edit mode to drag each touch-control cluster and resize the whole layout.';
    }
    applyTouchControlSizing();
  }
  function applyTouchControlSizing() {
    const buttonScale = touchLayout.buttonScale;
    const stickScale = touchLayout.stickScale;
    const keySize = Math.round(56 * buttonScale);
    const auxHeight = Math.round(42 * buttonScale);
    const auxMinWidth = Math.round(58 * buttonScale);
    document.querySelectorAll('.gpKey').forEach((btn) => {
      btn.style.width = `${keySize}px`;
      btn.style.height = `${keySize}px`;
      btn.style.fontSize = `${Math.max(14, Math.round(18 * buttonScale))}px`;
    });
    document.querySelectorAll('.gpAux').forEach((btn) => {
      const small = btn.classList.contains('gpSmall');
      btn.style.minWidth = `${small ? Math.round(48 * buttonScale) : auxMinWidth}px`;
      btn.style.height = `${small ? Math.round(34 * buttonScale) : auxHeight}px`;
      btn.style.padding = `0 ${Math.round((small ? 10 : 14) * buttonScale)}px`;
      btn.style.fontSize = `${Math.max(10, Math.round((small ? 12 : 15) * buttonScale))}px`;
    });
    if (joyBase) {
      joyBase.style.width = `${Math.round(146 * stickScale)}px`;
      joyBase.style.height = `${Math.round(146 * stickScale)}px`;
    }
    if (joyStick) {
      joyStick.style.width = `${Math.round(62 * stickScale)}px`;
      joyStick.style.height = `${Math.round(62 * stickScale)}px`;
    }
    if (lookBase) {
      lookBase.style.width = `${Math.round(124 * stickScale)}px`;
      lookBase.style.height = `${Math.round(124 * stickScale)}px`;
    }
    if (lookStick) {
      lookStick.style.width = `${Math.round(48 * stickScale)}px`;
      lookStick.style.height = `${Math.round(48 * stickScale)}px`;
    }
  }
  function resetTouchLayout() {
    touchLayout = sanitizeTouchLayout(defaultTouchLayoutForProfile(cfg.touchProfile));
    applyTouchLayout();
    scheduleTouchLayoutPersist();
  }
  function setLayoutEditing(nextEditing) {
    layoutEditing = !!nextEditing;
    if (layoutEditing) {
      setGamepadVisible(true);
      unlockAudio();
    }
    applyTouchLayout();
  }
  function applyStatsVisibility() {
    if (statsEl) {
      statsEl.style.display = statsVisible ? 'flex' : 'none';
    }
    updateStatsButton();
    renderStatsOverlay();
    if (statsVisible) {
      ensureStatsTicker();
      samplePeerStats();
    } else {
      stopStatsTicker();
    }
  }
  function toggleStatsOverlay() {
    statsVisible = !statsVisible;
    applyStatsVisibility();
  }
  function toggleStatsClock() {
    if (!statsVisible) return;
    showStatsClock = !showStatsClock;
    updateStreamerPreference('showStatsClock', showStatsClock);
    updateStatsButton();
    renderStatsOverlay();
  }
  function toggleStatsBattery() {
    if (!statsVisible) return;
    showStatsBattery = !showStatsBattery;
    updateStreamerPreference('showStatsBattery', showStatsBattery);
    updateStatsButton();
    renderStatsOverlay();
  }
  async function toggleAudio() {
    userMuted = !userMuted;
    updateStreamerPreference('audioMuted', userMuted);
    if (userMuted) {
      if (audioRetryTimer) {
        clearTimeout(audioRetryTimer);
        audioRetryTimer = null;
      }
      video.muted = true;
      audioUnlocked = false;
      updateAudioButton();
      return;
    }
    await unlockAudio();
  }
  async function unlockAudio() {
    if (userMuted) {
      updateAudioButton();
      return;
    }
    if (!video.muted) {
      updateAudioButton();
      return;
    }
    video.muted = false;
    video.volume = 1;
    try { await video.play(); } catch (_) {}
    audioUnlocked = !video.muted;
    updateAudioButton();
  }
  function scheduleAudioRetry() {
    if (userMuted) return;
    if (audioRetryTimer) return;
    audioRetryTimer = setTimeout(async () => {
      audioRetryTimer = null;
      if (userMuted) {
        updateAudioButton();
        return;
      }
      await unlockAudio();
      if (video.muted && !userMuted) {
        scheduleAudioRetry();
      }
    }, 750);
  }
  async function ensureAudioActive() {
    if (userMuted) {
      updateAudioButton();
      return;
    }
    await unlockAudio();
    if (video.muted) {
      scheduleAudioRetry();
    }
  }
  window.addEventListener('error', (event) => {
    const message = event && event.message ? event.message : 'unknown';
    const source = event && event.filename ? event.filename : 'inline';
    const line = event && event.lineno ? event.lineno : 0;
    fail(`JS runtime error: ${message} @ ${source}:${line}`);
  });
  window.addEventListener('unhandledrejection', (event) => {
    const reason = event && event.reason ? String(event.reason) : 'unknown';
    fail('Unhandled promise rejection: ' + reason);
  });
  function nextAck() { ack += 1; return ack; }
  function scheduleReconnect(reason) {
    if (reconnectAttempts >= maxReconnectAttempts) {
      fail('Reconnect exhausted: ' + reason);
      return;
    }
    if (reconnectTimer) return;
    reconnectAttempts += 1;
    const waitMs = Math.min(1500 * reconnectAttempts, 5000);
    post('status', `Reconnecting (${reconnectAttempts}/${maxReconnectAttempts})...`);
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, waitMs);
  }
  function resetTransport(closeSocket = false) {
    inputReady = false;
    inputProtocolVersion = 2;
    inputHandshakeComplete = false;
    partialReliableThresholdMs = DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS;
    riInputCapabilities = {
      hidDeviceMask: DEFAULT_HID_DEVICE_MASK,
      enablePartiallyReliableTransferGamepad: DEFAULT_PR_GAMEPAD_MASK,
      enablePartiallyReliableTransferHid: DEFAULT_PR_HID_MASK
    };
    offerAccepted = false;
    clearOfferTimeout();
    if (inputFallbackTimer) {
      clearTimeout(inputFallbackTimer);
      inputFallbackTimer = null;
    }
    if (signalingOpenTimeout) {
      clearTimeout(signalingOpenTimeout);
      signalingOpenTimeout = null;
    }
    if (hb) { clearInterval(hb); hb = null; }
    if (hbInput) { clearInterval(hbInput); hbInput = null; }
    if (statsTimer) { clearInterval(statsTimer); statsTimer = null; }
    if (audioRetryTimer) { clearTimeout(audioRetryTimer); audioRetryTimer = null; }
    stopKeyframeTimer();
    lastBytesReceived = 0;
    lastBytesTimestamp = 0;
    for (const track of remoteMediaStream.getTracks()) {
      try { remoteMediaStream.removeTrack(track); } catch (_) {}
    }
    if (reliableCh) { try { reliableCh.close(); } catch (_) {} }
    if (partialCh) { try { partialCh.close(); } catch (_) {} }
    reliableCh = null;
    partialCh = null;
    if (pc) { try { pc.close(); } catch (_) {} }
    pc = null;
    if (closeSocket && ws) {
      try { ws.onclose = null; ws.close(); } catch (_) {}
      ws = null;
    }
  }
  function clearOfferTimeout() {
    if (offerTimeoutTimer) {
      clearTimeout(offerTimeoutTimer);
      offerTimeoutTimer = null;
    }
  }
  function startOfferTimeout() {
    clearOfferTimeout();
    offerTimeoutTimer = setTimeout(() => {
      post('status', 'Offer timeout, retrying signaling');
      if (ws && ws.readyState === WebSocket.OPEN) {
        try { ws.close(); } catch (_) {}
      }
      scheduleReconnect('offer timeout');
    }, 15000);
  }
  function isChannelOpen(channel) {
    return !!channel && channel.readyState === 'open';
  }
  function updateInputReady() {
    inputReady = isChannelOpen(reliableCh) && inputHandshakeComplete;
    if (inputReady) {
      post('status', 'Input ready');
    }
  }
  function shouldKeepPeerAliveOnSignalingClose() {
    if (!offerAccepted || !pc) return false;
    const state = pc.connectionState;
    return state === 'new' || state === 'connecting' || state === 'connected';
  }
  function canUsePartiallyReliableForMouse() {
    if (!isChannelOpen(partialCh)) return false;
    if ((riInputCapabilities.hidDeviceMask & RI_INPUT_MOUSE_REL_MASK) === 0) return false;
    return (riInputCapabilities.enablePartiallyReliableTransferHid & RI_INPUT_MOUSE_REL_MASK) !== 0;
  }
  function send(obj) {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    ws.send(JSON.stringify(obj));
  }
  function sendInput(buf) {
    if (isChannelOpen(reliableCh)) {
      reliableCh.send(buf);
      return;
    }
    if (isChannelOpen(partialCh)) {
      partialCh.send(buf);
    }
  }
  function sendPartialInput(buf) {
    if (canUsePartiallyReliableForMouse()) {
      partialCh.send(buf);
      return;
    }
    sendInput(buf);
  }
  async function toBytes(data) {
    if (data instanceof ArrayBuffer) return new Uint8Array(data);
    if (typeof Blob !== 'undefined' && data instanceof Blob) {
      const buffer = await data.arrayBuffer();
      return new Uint8Array(buffer);
    }
    if (typeof data === 'string') {
      return new TextEncoder().encode(data);
    }
    return new Uint8Array(0);
  }
  function setupInputHeartbeat() {
    if (hbInput) clearInterval(hbInput);
    hbInput = setInterval(() => {
      if (inputReady) sendInput(encodeHeartbeat());
    }, 2000);
  }
  function handleInputHandshakeMessage(bytes) {
    if (!bytes || bytes.length < 2) return;
    const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    const firstWord = view.getUint16(0, true);
    let version = 2;
    if (firstWord === 526) {
      version = bytes.length >= 4 ? view.getUint16(2, true) : 2;
    } else if (bytes[0] === 0x0e) {
      version = firstWord;
    } else {
      return;
    }
    if (inputFallbackTimer) {
      clearTimeout(inputFallbackTimer);
      inputFallbackTimer = null;
    }
    inputProtocolVersion = Math.max(2, Math.floor(version || 2));
    if (!inputHandshakeComplete) {
      inputHandshakeComplete = true;
      updateInputReady();
      setupInputHeartbeat();
      log('Input handshake complete (protocol v' + inputProtocolVersion + ')');
    }
  }
  function renderStatsOverlay() {
    if (!statsEl || !statsPrimary || !statsMeta) return;
    const primaryParts = [
      `FPS ${latestStats.fps > 0 ? Math.round(latestStats.fps) : '--'}`,
      `Ping ${latestStats.pingMs > 0 ? Math.round(latestStats.pingMs) : '--'} ms`,
      `Loss ${latestStats.packetLossPct > 0 ? latestStats.packetLossPct.toFixed(1) : '--'}%`,
      `Rate ${latestStats.bitrateMbps > 0 ? latestStats.bitrateMbps.toFixed(1) : '--'} Mbps`
    ];
    const metaParts = [];
    if (showStatsClock) {
      metaParts.push(new Date().toLocaleTimeString([], { hour: 'numeric', minute: '2-digit' }));
    }
    if (showStatsBattery) {
      if (Number.isFinite(deviceBatteryPercent)) {
        metaParts.push(deviceBatteryCharging ? `Battery ${deviceBatteryPercent}% charging` : `Battery ${deviceBatteryPercent}%`);
      } else {
        metaParts.push('Battery unavailable');
      }
    }
    const nextPrimary = primaryParts.join(' | ');
    const nextMeta = metaParts.join(' | ');
    const combined = `${nextPrimary}\n${nextMeta}`;
    if (combined === lastStatsMarkup) return;
    lastStatsMarkup = combined;
    statsPrimary.textContent = nextPrimary;
    if (nextMeta) {
      statsMeta.style.display = 'block';
      statsMeta.textContent = nextMeta;
    } else {
      statsMeta.style.display = 'none';
      statsMeta.textContent = '';
    }
  }
  async function samplePeerStats() {
    if (!statsVisible) return;
    if (!pc || !statsEl) {
      renderStatsOverlay();
      return;
    }
    try {
      const report = await pc.getStats();
      let fps = 0;
      let pingMs = 0;
      let bitrateMbps = 0;
      let packetLossPct = 0;
      report.forEach((stat) => {
        if (stat.type === 'inbound-rtp' && stat.kind === 'video') {
          if (typeof stat.framesPerSecond === 'number' && stat.framesPerSecond > 0) {
            fps = stat.framesPerSecond;
          }
          if (typeof stat.packetsLost === 'number' && typeof stat.packetsReceived === 'number') {
            const totalPackets = stat.packetsReceived + stat.packetsLost;
            if (totalPackets > 0) {
              packetLossPct = Math.max(packetLossPct, (stat.packetsLost / totalPackets) * 100);
            }
          }
          if (typeof stat.bytesReceived === 'number') {
            if (lastBytesTimestamp > 0 && stat.timestamp > lastBytesTimestamp && stat.bytesReceived >= lastBytesReceived) {
              const bytesDiff = stat.bytesReceived - lastBytesReceived;
              const seconds = (stat.timestamp - lastBytesTimestamp) / 1000;
              if (seconds > 0) {
                bitrateMbps = (bytesDiff * 8) / seconds / 1000000;
              }
            }
            lastBytesReceived = stat.bytesReceived;
            lastBytesTimestamp = stat.timestamp;
          }
        }
        if (stat.type === 'remote-inbound-rtp' && stat.kind === 'video' && typeof stat.roundTripTime === 'number') {
          pingMs = stat.roundTripTime * 1000;
        }
        if (stat.type === 'candidate-pair' && stat.nominated && typeof stat.currentRoundTripTime === 'number') {
          pingMs = Math.max(pingMs, stat.currentRoundTripTime * 1000);
        }
      });
      latestStats = { fps, pingMs, packetLossPct, bitrateMbps };
      renderStatsOverlay();
    } catch (_) {}
  }
  function ensureStatsTicker() {
    if (!statsVisible || statsTimer) return;
    statsTimer = setInterval(samplePeerStats, 1000);
  }
  function stopStatsTicker() {
    if (!statsTimer) return;
    clearInterval(statsTimer);
    statsTimer = null;
  }
  function buildSignInUrl() {
    const signalingServer = (cfg.signalingServer || "").trim();
    const fallbackHost = signalingServer.includes(":") ? signalingServer : (signalingServer ? signalingServer + ":443" : "");
    const base = (cfg.signalingUrl || "").trim() || ("wss://" + fallbackHost + "/nvst/");
    const url = new URL(base);
    url.protocol = "wss:";
    // Append sign_in to existing path (e.g. /nvst/abc/ -> /nvst/abc/sign_in).
    // Do NOT wipe the full path — the session signalingUrl may include a unique subpath.
    url.pathname = url.pathname.replace(/\/?$/, '/') + 'sign_in';
    url.search = "";
    url.searchParams.set("peer_id", peerName);
    url.searchParams.set("version", "2");
    return url.toString();
  }
  function sendPeerInfo() {
    send({
      ackid: nextAck(),
      peer_info: {
        browser: "Chrome",
        browserVersion: "131",
        connected: true,
        id: peerId,
        name: peerName,
        peerRole: 0,
        resolution: `${cfg.width}x${cfg.height}`,
        version: 2
      }
    });
  }
  function extractPublicIp(hostOrIp) {
    if (!hostOrIp) return null;
    if (/^\d{1,3}(\.\d{1,3}){3}$/.test(hostOrIp)) return hostOrIp;
    const first = hostOrIp.split('.')[0] ?? '';
    const parts = first.split('-');
    if (parts.length === 4 && parts.every(p => /^\d{1,3}$/.test(p))) return parts.join('.');
    return null;
  }
  function fixServerIp(sdp, serverIp) {
    const ip = extractPublicIp(serverIp);
    if (!ip) return sdp;
    let fixed = sdp.replace(/c=IN IP4 0\.0\.0\.0/g, `c=IN IP4 ${ip}`);
    fixed = fixed.replace(/(a=candidate:\S+\s+\d+\s+\w+\s+\d+\s+)0\.0\.0\.0(\s+)/g, `$1${ip}$2`);
    return fixed;
  }
  function extractIceUfragFromOffer(sdp) {
    const match = sdp.match(/a=ice-ufrag:([^\r\n]+)/);
    return match?.[1]?.trim() ?? "";
  }
  function extractIceCredentials(sdp) {
    const lines = sdp.split(/\r?\n/);
    const ufrag = lines.find((line) => line.startsWith('a=ice-ufrag:'))?.slice('a=ice-ufrag:'.length).trim() ?? '';
    const pwd = lines.find((line) => line.startsWith('a=ice-pwd:'))?.slice('a=ice-pwd:'.length).trim() ?? '';
    const fingerprint = lines.find((line) => line.startsWith('a=fingerprint:sha-256 '))?.slice('a=fingerprint:sha-256 '.length).trim() ?? '';
    return { ufrag, pwd, fingerprint };
  }
  function parseRiIntegerAttribute(sdp, attribute, fallback) {
    const escapedAttribute = attribute.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    const match = sdp.match(new RegExp(`a=${escapedAttribute}:([^\\r\\n]+)`, 'i'));
    const raw = match?.[1]?.trim();
    if (!raw) return fallback;
    const normalized = raw.toLowerCase();
    const parsed = normalized.startsWith('0x') ? parseInt(normalized.slice(2), 16) : parseInt(normalized, 10);
    return Number.isFinite(parsed) ? parsed : fallback;
  }
  function parsePartialReliableThresholdMs(sdp) {
    const match = sdp.match(/a=ri\.partialReliableThresholdMs:(\d+)/i);
    if (!match?.[1]) return DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS;
    const parsed = Number.parseInt(match[1], 10);
    if (!Number.isFinite(parsed) || parsed <= 0) return DEFAULT_PARTIAL_RELIABLE_THRESHOLD_MS;
    return Math.max(1, Math.min(5000, parsed));
  }
  function parseRiInputCapabilities(sdp) {
    return {
      partialReliableThresholdMs: parsePartialReliableThresholdMs(sdp),
      hidDeviceMask: parseRiIntegerAttribute(sdp, 'ri.hidDeviceMask', DEFAULT_HID_DEVICE_MASK),
      enablePartiallyReliableTransferGamepad: parseRiIntegerAttribute(
        sdp,
        'ri.enablePartiallyReliableTransferGamepad',
        DEFAULT_PR_GAMEPAD_MASK
      ),
      enablePartiallyReliableTransferHid: parseRiIntegerAttribute(
        sdp,
        'ri.enablePartiallyReliableTransferHid',
        DEFAULT_PR_HID_MASK
      )
    };
  }
  function nowBigUs() { return BigInt(Math.round(performance.now() * 1000)); }
  function writeTimestampBE(view, offset) {
    const ts = nowBigUs();
    view.setUint32(offset, Number(ts >> 32n), false);
    view.setUint32(offset + 4, Number(ts & 0xFFFFFFFFn), false);
  }
  function encodeHeartbeat() {
    const buf = new ArrayBuffer(4);
    new DataView(buf).setUint32(0, 2, true);
    return buf;
  }
  function encodeKey(type, keycode, scancode, modifiers) {
    const buf = new ArrayBuffer(18);
    const v = new DataView(buf);
    v.setUint32(0, type, true);
    v.setUint16(4, keycode, false);
    v.setUint16(6, modifiers, false);
    v.setUint16(8, scancode, false);
    writeTimestampBE(v, 10);
    return wrapSingleEvent(buf);
  }
  function encodeMouseMove(dx, dy) {
    const buf = new ArrayBuffer(22);
    const v = new DataView(buf);
    v.setUint32(0, 7, true);
    v.setInt16(4, Math.max(-32768, Math.min(32767, dx)), false);
    v.setInt16(6, Math.max(-32768, Math.min(32767, dy)), false);
    writeTimestampBE(v, 14);
    return wrapMouseMoveEvent(buf);
  }
  function encodeMouseButton(type, button) {
    const buf = new ArrayBuffer(18);
    const v = new DataView(buf);
    v.setUint32(0, type, true);
    v.setUint8(4, button);
    writeTimestampBE(v, 10);
    return wrapSingleEvent(buf);
  }
  function wrapSingleEvent(buf) {
    if (inputProtocolVersion <= 2) return buf;
    const wrapped = new ArrayBuffer(10 + buf.byteLength);
    const view = new DataView(wrapped);
    view.setUint8(0, 0x23);
    writeTimestampBE(view, 1);
    view.setUint8(9, 0x22);
    new Uint8Array(wrapped, 10).set(new Uint8Array(buf));
    return wrapped;
  }
  function wrapMouseMoveEvent(buf) {
    if (inputProtocolVersion <= 2) return buf;
    const wrapped = new ArrayBuffer(12 + buf.byteLength);
    const view = new DataView(wrapped);
    view.setUint8(0, 0x23);
    writeTimestampBE(view, 1);
    view.setUint8(9, 0x21);
    view.setUint16(10, buf.byteLength, false);
    new Uint8Array(wrapped, 12).set(new Uint8Array(buf));
    return wrapped;
  }
  function wrapGamepadReliable(payload) {
    if (inputProtocolVersion <= 2) return payload;
    const wrapped = new Uint8Array(12 + payload.byteLength);
    const view = new DataView(wrapped.buffer);
    wrapped[0] = 0x23;
    writeTimestampBE(view, 1);
    wrapped[9] = 0x21;
    view.setUint16(10, payload.byteLength, false);
    wrapped.set(new Uint8Array(payload), 12);
    return wrapped;
  }
  function wrapGamepadPartiallyReliable(payload, gamepadIndex, sequenceNumber) {
    if (inputProtocolVersion <= 2) return payload;
    const wrapped = new Uint8Array(16 + payload.byteLength);
    const view = new DataView(wrapped.buffer);
    wrapped[0] = 0x23;
    writeTimestampBE(view, 1);
    wrapped[9] = 0x26;
    wrapped[10] = gamepadIndex & 0xFF;
    view.setUint16(11, sequenceNumber & 0xFFFF, false);
    wrapped[13] = 0x21;
    view.setUint16(14, payload.byteLength, false);
    wrapped.set(new Uint8Array(payload), 16);
    return wrapped;
  }
  function nextGamepadSequence(index) {
    if (!window.__opennowGamepadSequence) {
      window.__opennowGamepadSequence = new Map();
    }
    const current = window.__opennowGamepadSequence.get(index) ?? 1;
    window.__opennowGamepadSequence.set(index, (current + 1) % 65536);
    return current;
  }
  function encodeGamepadState(payload, bitmap, usePartiallyReliable) {
    const bytes = new ArrayBuffer(GAMEPAD_PACKET_SIZE);
    const view = new DataView(bytes);
    view.setUint32(0, 12, true);
    view.setUint16(4, 26, true);
    view.setUint16(6, payload.controllerId & 0x03, true);
    view.setUint16(8, bitmap & 0xFFFF, true);
    view.setUint16(10, 20, true);
    view.setUint16(12, payload.buttons & 0xFFFF, true);
    const packedTriggers = (payload.leftTrigger & 0xFF) | ((payload.rightTrigger & 0xFF) << 8);
    view.setUint16(14, packedTriggers, true);
    view.setInt16(16, payload.leftStickX, true);
    view.setInt16(18, payload.leftStickY, true);
    view.setInt16(20, payload.rightStickX, true);
    view.setInt16(22, payload.rightStickY, true);
    view.setUint16(24, 0, true);
    view.setUint16(26, 85, true);
    view.setUint16(28, 0, true);
    const ts = nowBigUs();
    view.setUint32(30, Number(ts & 0xFFFFFFFFn), true);
    view.setUint32(34, Number(ts >> 32n), true);
    if (usePartiallyReliable) {
      return wrapGamepadPartiallyReliable(bytes, payload.controllerId, nextGamepadSequence(payload.controllerId));
    }
    return wrapGamepadReliable(bytes);
  }
  function normalizeCodec(name) {
    const upper = String(name || '').toUpperCase();
    return upper === 'HEVC' ? 'H265' : upper;
  }
  function offerHasCodec(sdp, codec) {
    const target = normalizeCodec(codec);
    let inVideo = false;
    for (const line of sdp.split(/\r?\n/)) {
      if (line.startsWith('m=video')) {
        inVideo = true;
        continue;
      }
      if (line.startsWith('m=') && inVideo) {
        break;
      }
      if (!inVideo || !line.startsWith('a=rtpmap:')) continue;
      const rest = line.slice('a=rtpmap:'.length);
      const [pt, codecPart] = rest.split(/\s+/, 2);
      const codecName = normalizeCodec((codecPart || '').split('/')[0] || '');
      if (pt && codecName === target) return true;
    }
    return false;
  }
  function resolvePreferredCodec(offerSdp) {
    const preferred = normalizeCodec(cfg.preferredCodec || 'Auto');
    if (preferred === 'AUTO') {
      // iOS WKWebView can report a connected peer while hardware decode fails
      // on some HEVC paths (AppleAVD init errors). Prefer H264 for reliability.
      return offerHasCodec(offerSdp, 'H264') ? 'H264' : 'H265';
    }
    return preferred;
  }
  function preferCodec(sdp, codec) {
    const target = normalizeCodec(codec);
    const lineEnding = sdp.includes('\r\n') ? '\r\n' : '\n';
    const lines = sdp.split(/\r?\n/);
    let inVideoSection = false;
    const payloadTypesByCodec = new Map();
    const codecByPayloadType = new Map();
    const rtxAptByPayloadType = new Map();

    for (const line of lines) {
      if (line.startsWith('m=video')) {
        inVideoSection = true;
        continue;
      }
      if (line.startsWith('m=') && inVideoSection) {
        inVideoSection = false;
      }
      if (!inVideoSection || !line.startsWith('a=rtpmap:')) continue;
      const rest = line.slice('a=rtpmap:'.length);
      const [pt, codecPart] = rest.split(/\s+/, 2);
      const codecName = normalizeCodec((codecPart || '').split('/')[0] || '');
      if (!pt || !codecName) continue;
      const list = payloadTypesByCodec.get(codecName) ?? [];
      list.push(pt);
      payloadTypesByCodec.set(codecName, list);
      codecByPayloadType.set(pt, codecName);
    }

    inVideoSection = false;
    for (const line of lines) {
      if (line.startsWith('m=video')) {
        inVideoSection = true;
        continue;
      }
      if (line.startsWith('m=') && inVideoSection) {
        inVideoSection = false;
      }
      if (!inVideoSection || !line.startsWith('a=fmtp:')) continue;
      const rest = line.split(':', 2)[1] ?? '';
      const [pt = '', params = ''] = rest.split(/\s+/, 2);
      if (!pt || !params) continue;
      const aptMatch = params.match(/(?:^|;)\s*apt=(\d+)/i);
      if (aptMatch?.[1]) {
        rtxAptByPayloadType.set(pt, aptMatch[1]);
      }
    }

    const preferredPayloads = payloadTypesByCodec.get(target) ?? [];
    if (preferredPayloads.length === 0) {
      return sdp;
    }

    const preferred = new Set(preferredPayloads);
    const allowed = new Set(preferredPayloads);
    for (const [rtxPt, apt] of rtxAptByPayloadType.entries()) {
      if (preferred.has(apt) && codecByPayloadType.get(rtxPt) === 'RTX') {
        allowed.add(rtxPt);
      }
    }

    const filtered = [];
    inVideoSection = false;
    for (const line of lines) {
      if (line.startsWith('m=video')) {
        inVideoSection = true;
        const parts = line.split(/\s+/);
        const header = parts.slice(0, 3);
        const available = parts.slice(3).filter((pt) => allowed.has(pt));
        const ordered = [];
        for (const pt of preferredPayloads) {
          if (available.includes(pt)) ordered.push(pt);
        }
        for (const pt of available) {
          if (!preferred.has(pt)) ordered.push(pt);
        }
        filtered.push(ordered.length > 0 ? [...header, ...ordered].join(' ') : line);
        continue;
      }
      if (line.startsWith('m=') && inVideoSection) {
        inVideoSection = false;
      }
      if (inVideoSection && (line.startsWith('a=rtpmap:') || line.startsWith('a=fmtp:') || line.startsWith('a=rtcp-fb:'))) {
        const rest = line.split(':', 2)[1] ?? '';
        const [pt = ''] = rest.split(/\s+/, 1);
        if (pt && !allowed.has(pt)) continue;
      }
      filtered.push(line);
    }

    return filtered.join(lineEnding);
  }
  function mungeAnswerSdp(sdp, maxBitrateKbps) {
    const lines = sdp.split(/\r?\n/);
    const out = [];
    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      out.push(line);
      if (line.startsWith('m=video') || line.startsWith('m=audio')) {
        const bw = line.startsWith('m=video') ? maxBitrateKbps : 128;
        if (!(lines[i + 1] ?? '').startsWith('b=')) out.push(`b=AS:${bw}`);
      }
      if (line.startsWith('a=fmtp:') && line.includes('minptime=') && !line.includes('stereo=1')) {
        out[out.length - 1] = line + ';stereo=1';
      }
    }
    return out.join(sdp.includes('\r\n') ? '\r\n' : '\n');
  }
  function detectNegotiatedCodec(sdp) {
    const lines = sdp.split(/\r?\n/);
    let inVideo = false;
    let orderedPayloads = [];
    const codecByPayload = new Map();
    for (const line of lines) {
      if (line.startsWith('m=video')) {
        inVideo = true;
        orderedPayloads = line.split(/\s+/).slice(3);
        continue;
      }
      if (line.startsWith('m=') && inVideo) {
        break;
      }
      if (!inVideo || !line.startsWith('a=rtpmap:')) continue;
      const rest = line.slice('a=rtpmap:'.length);
      const [pt, codecPart] = rest.split(/\s+/, 2);
      if (!pt) continue;
      codecByPayload.set(pt, normalizeCodec((codecPart || '').split('/')[0] || ''));
    }
    for (const pt of orderedPayloads) {
      const codec = codecByPayload.get(pt);
      if (codec && codec !== 'RTX') return codec;
    }
    return '';
  }
  function buildNvstSdp(params) {
    const minBitrate = Math.max(5000, Math.floor(params.maxBitrateKbps * 0.35));
    const initialBitrate = Math.max(minBitrate, Math.floor(params.maxBitrateKbps * 0.7));
    const isHighFps = params.fps >= 90;
    const is120Fps = params.fps === 120;
    const is240Fps = params.fps >= 240;
    const isAv1 = params.codec === 'AV1';
    const bitDepth = params.colorQuality.startsWith('10bit') ? 10 : 8;
    const hidDeviceMask = params.hidDeviceMask ?? 0xFFFFFFFF;
    const enablePartiallyReliableTransferGamepad = params.enablePartiallyReliableTransferGamepad ?? 0xF;
    const enablePartiallyReliableTransferHid = params.enablePartiallyReliableTransferHid ?? hidDeviceMask;
    const lines = [
      'v=0',
      'o=SdpTest test_id_13 14 IN IPv4 127.0.0.1',
      's=-',
      't=0 0',
      `a=general.icePassword:${params.credentials.pwd}`,
      `a=general.iceUserNameFragment:${params.credentials.ufrag}`,
      `a=general.dtlsFingerprint:${params.credentials.fingerprint}`,
      'm=video 0 RTP/AVP',
      'a=msid:fbc-video-0',
      'a=vqos.fec.rateDropWindow:10',
      'a=vqos.fec.minRequiredFecPackets:2',
      'a=vqos.fec.repairMinPercent:5',
      'a=vqos.fec.repairPercent:5',
      'a=vqos.fec.repairMaxPercent:35',
      'a=vqos.drc.enable:0',
      'a=vqos.dfc.enable:0',
      'a=video.dx9EnableNv12:1',
      'a=video.dx9EnableHdr:1',
      'a=vqos.qpg.enable:1',
      'a=vqos.resControl.qp.qpg.featureSetting:7',
      'a=bwe.useOwdCongestionControl:1',
      'a=video.enableRtpNack:1',
      'a=vqos.bw.txRxLag.minFeedbackTxDeltaMs:200',
      'a=vqos.drc.bitrateIirFilterFactor:18',
      'a=video.packetSize:1140',
      'a=packetPacing.minNumPacketsPerGroup:15'
    ];
    if (isHighFps) {
      lines.push(
        'a=bwe.iirFilterFactor:8',
        'a=video.encoderFeatureSetting:47',
        'a=video.encoderPreset:6',
        'a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600',
        'a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9',
        `a=video.fbcDynamicFpsGrabTimeoutMs:${is120Fps ? 6 : 18}`,
        `a=vqos.resControl.cpmRtc.serverResolutionUpdateCoolDownCount:${is120Fps ? 6000 : 12000}`
      );
    }
    if (is240Fps) {
      lines.push(
        'a=video.enableNextCaptureMode:1',
        'a=vqos.maxStreamFpsEstimate:240',
        'a=video.videoSplitEncodeStripsPerFrame:3',
        'a=video.updateSplitEncodeStateDynamically:1'
      );
    }
    lines.push(
      'a=vqos.adjustStreamingFpsDuringOutOfFocus:1',
      'a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1',
      'a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1',
      'a=vqos.resControl.cpmRtc.featureMask:0',
      'a=vqos.resControl.cpmRtc.enable:0',
      'a=vqos.resControl.cpmRtc.minResolutionPercent:100',
      'a=vqos.resControl.cpmRtc.resolutionChangeHoldonMs:999999',
      `a=packetPacing.numGroups:${is120Fps ? 3 : 5}`,
      'a=packetPacing.maxDelayUs:1000',
      'a=packetPacing.minNumPacketsFrame:10',
      'a=video.rtpNackQueueLength:1024',
      'a=video.rtpNackQueueMaxPackets:512',
      'a=video.rtpNackMaxPacketCount:25',
      'a=vqos.drc.qpMaxResThresholdAdj:4',
      'a=vqos.grc.qpMaxResThresholdAdj:4',
      'a=vqos.drc.iirFilterFactor:100'
    );
    if (isAv1) {
      lines.push(
        'a=vqos.drc.minQpHeadroom:20',
        'a=vqos.drc.lowerQpThreshold:100',
        'a=vqos.drc.upperQpThreshold:200',
        'a=vqos.drc.minAdaptiveQpThreshold:180',
        'a=vqos.drc.qpCodecThresholdAdj:0',
        'a=vqos.drc.qpMaxResThresholdAdj:20',
        'a=vqos.dfc.minQpHeadroom:20',
        'a=vqos.dfc.qpLowerLimit:100',
        'a=vqos.dfc.qpMaxUpperLimit:200',
        'a=vqos.dfc.qpMinUpperLimit:180',
        'a=vqos.dfc.qpMaxResThresholdAdj:20',
        'a=vqos.dfc.qpCodecThresholdAdj:0',
        'a=vqos.grc.minQpHeadroom:20',
        'a=vqos.grc.lowerQpThreshold:100',
        'a=vqos.grc.upperQpThreshold:200',
        'a=vqos.grc.minAdaptiveQpThreshold:180',
        'a=vqos.grc.qpMaxResThresholdAdj:20',
        'a=vqos.grc.qpCodecThresholdAdj:0',
        'a=video.minQp:25',
        'a=video.enableAv1RcPrecisionFactor:1'
      );
    }
    lines.push(
      `a=video.clientViewportWd:${params.width}`,
      `a=video.clientViewportHt:${params.height}`,
      `a=video.maxFPS:${params.fps}`,
      `a=video.initialBitrateKbps:${initialBitrate}`,
      `a=video.initialPeakBitrateKbps:${params.maxBitrateKbps}`,
      `a=vqos.bw.maximumBitrateKbps:${params.maxBitrateKbps}`,
      `a=vqos.bw.minimumBitrateKbps:${minBitrate}`,
      `a=vqos.bw.peakBitrateKbps:${params.maxBitrateKbps}`,
      `a=vqos.bw.serverPeakBitrateKbps:${params.maxBitrateKbps}`,
      'a=vqos.bw.enableBandwidthEstimation:1',
      'a=vqos.bw.disableBitrateLimit:0',
      `a=vqos.grc.maximumBitrateKbps:${params.maxBitrateKbps}`,
      'a=vqos.grc.enable:0',
      'a=video.maxNumReferenceFrames:4',
      'a=video.mapRtpTimestampsToFrames:1',
      'a=video.encoderCscMode:3',
      'a=video.dynamicRangeMode:0',
      `a=video.bitDepth:${bitDepth}`,
      `a=video.scalingFeature1:${isAv1 ? 1 : 0}`,
      'a=video.prefilterParams.prefilterModel:0',
      'm=audio 0 RTP/AVP',
      'a=msid:audio',
      'm=mic 0 RTP/AVP',
      'a=msid:mic',
      'a=rtpmap:0 PCMU/8000',
      'm=application 0 RTP/AVP',
      'a=msid:input_1',
      `a=ri.partialReliableThresholdMs:${params.partialReliableThresholdMs}`,
      `a=ri.hidDeviceMask:${hidDeviceMask}`,
      `a=ri.enablePartiallyReliableTransferGamepad:${enablePartiallyReliableTransferGamepad}`,
      `a=ri.enablePartiallyReliableTransferHid:${enablePartiallyReliableTransferHid}`,
      ''
    );
    return lines.join('\n');
  }
  async function waitForIceGathering(rtc, timeoutMs) {
    if (!rtc.localDescription) return '';
    if (rtc.iceGatheringState === 'complete') {
      return rtc.localDescription?.sdp || '';
    }
    return await new Promise((resolve) => {
      const timeout = setTimeout(() => {
        rtc.removeEventListener('icegatheringstatechange', onChange);
        resolve(rtc.localDescription?.sdp || '');
      }, timeoutMs);
      function onChange() {
        if (rtc.iceGatheringState === 'complete') {
          clearTimeout(timeout);
          rtc.removeEventListener('icegatheringstatechange', onChange);
          resolve(rtc.localDescription?.sdp || '');
        }
      }
      rtc.addEventListener('icegatheringstatechange', onChange);
    });
  }
  async function injectManualIce(rtc, ip, port, ufrag) {
    const rawIp = extractPublicIp(ip);
    if (!rawIp || !port) return;
    const candidateStr = `candidate:1 1 udp 2130706431 ${rawIp} ${port} typ host`;
    for (const mid of ['0', '1', '2', '3']) {
      try {
        await rtc.addIceCandidate({ candidate: candidateStr, sdpMid: mid, sdpMLineIndex: parseInt(mid, 10), usernameFragment: ufrag || undefined });
        break;
      } catch (_) {}
    }
  }
  let kfAttempt = 0;
  let kfTimer = null;
  function sendKeyframeRequest() {
    kfAttempt += 1;
    send({
      peer_msg: { from: peerId, to: 1, msg: JSON.stringify({ type: 'request_keyframe', reason: 'decoder-recovery', backlogFrames: 0, attempt: kfAttempt }) },
      ackid: nextAck()
    });
  }
  function startKeyframeTimer() {
    if (kfTimer) return;
    kfAttempt = 0;
    kfTimer = setInterval(sendKeyframeRequest, 5000);
  }
  function stopKeyframeTimer() {
    if (kfTimer) { clearInterval(kfTimer); kfTimer = null; }
    kfAttempt = 0;
  }
  function configureReceiverLowLatency(receiver, kind) {
    try {
      if ('jitterBufferTarget' in receiver) receiver.jitterBufferTarget = 0;
      if ('playoutDelayHint' in receiver) receiver.playoutDelayHint = 0;
      if (kind === 'video' && receiver.track && 'contentHint' in receiver.track) receiver.track.contentHint = 'motion';
    } catch (_) {}
  }
  function ensurePeerConnection() {
    if (pc) return pc;
    const ice = (cfg.iceServers || []).map((server) => ({
      urls: Array.isArray(server.urls) ? server.urls : [server.urls],
      username: server.username || undefined,
      credential: server.credential || undefined
    }));
    pc = new RTCPeerConnection({ iceServers: ice, bundlePolicy: 'max-bundle', rtcpMuxPolicy: 'require' });
    const thisPc = pc;
    reliableCh = thisPc.createDataChannel('input_channel_v1', { ordered: true });
    reliableCh.binaryType = 'arraybuffer';
    reliableCh.onopen = () => {
      updateInputReady();
      if (inputFallbackTimer) clearTimeout(inputFallbackTimer);
      inputFallbackTimer = setTimeout(() => {
        if (!inputHandshakeComplete && isChannelOpen(reliableCh)) {
          inputProtocolVersion = 2;
          inputHandshakeComplete = true;
          updateInputReady();
          setupInputHeartbeat();
          log('Input handshake timeout; falling back to protocol v2');
        }
      }, INPUT_PROTOCOL_FALLBACK_DELAY_MS);
    };
    reliableCh.onclose = () => {
      inputHandshakeComplete = false;
      inputProtocolVersion = 2;
      updateInputReady();
      if (inputFallbackTimer) {
        clearTimeout(inputFallbackTimer);
        inputFallbackTimer = null;
      }
      if (hbInput) { clearInterval(hbInput); hbInput = null; }
    };
    reliableCh.onmessage = async (event) => {
      try {
        const bytes = await toBytes(event.data);
        handleInputHandshakeMessage(bytes);
      } catch (_) {}
    };
    partialCh = thisPc.createDataChannel('input_channel_partially_reliable', {
      ordered: false,
      maxPacketLifeTime: partialReliableThresholdMs
    });
    partialCh.binaryType = 'arraybuffer';
    partialCh.onopen = () => updateInputReady();
    partialCh.onclose = () => updateInputReady();
    thisPc.ondatachannel = (event) => {
      const ch = event.channel;
      if (ch.label !== 'control_channel') return;
      ch.binaryType = 'arraybuffer';
      ch.onmessage = (e) => {
        try {
          const msg = typeof e.data === 'string' ? JSON.parse(e.data) : null;
          if (msg && msg.type === 'time_warning') {
            post('status', 'Time warning: ' + (msg.secondsLeft || '?') + 's left');
          }
        } catch (_) {}
      };
    };
    thisPc.ontrack = (event) => {
      const kind = event.track.kind;
      configureReceiverLowLatency(event.receiver, kind);
      if (!remoteMediaStream.getTracks().some((track) => track.id === event.track.id)) {
        remoteMediaStream.addTrack(event.track);
      }
      if (kind === 'video') {
        video.srcObject = remoteMediaStream;
        video.play().catch(() => {});
        ensureAudioActive();
        post('status', 'Streamer connected');
        ensureStatsTicker();
        startKeyframeTimer();
      } else if (kind === 'audio') {
        video.srcObject = remoteMediaStream;
        ensureAudioActive();
      }
    };
    thisPc.onicecandidate = (event) => {
      if (!event.candidate) return;
      send({
        peer_msg: {
          from: peerId,
          to: 1,
          msg: JSON.stringify({
            candidate: event.candidate.candidate,
            sdpMid: event.candidate.sdpMid,
            sdpMLineIndex: event.candidate.sdpMLineIndex
          })
        },
        ackid: nextAck()
      });
    };
    thisPc.onconnectionstatechange = () => {
      post('status', 'Peer: ' + thisPc.connectionState);
      if (thisPc.connectionState === 'failed' || thisPc.connectionState === 'disconnected') {
        stopKeyframeTimer();
        resetTransport();
        if (offerAccepted) {
          fail('Peer connection lost; reopen the session');
          return;
        }
        scheduleReconnect('peer ' + thisPc.connectionState);
      }
    };
    return pc;
  }
  async function onOffer(sdp) {
    try {
      clearOfferTimeout();
      const fixedOffer = fixServerIp(sdp, cfg.serverIp || cfg.signalingServer || '');
      const parsedRi = parseRiInputCapabilities(fixedOffer);
      partialReliableThresholdMs = parsedRi.partialReliableThresholdMs;
      riInputCapabilities = {
        hidDeviceMask: parsedRi.hidDeviceMask,
        enablePartiallyReliableTransferGamepad: parsedRi.enablePartiallyReliableTransferGamepad,
        enablePartiallyReliableTransferHid: parsedRi.enablePartiallyReliableTransferHid
      };
      const rtc = ensurePeerConnection();
      const serverIceUfrag = extractIceUfragFromOffer(fixedOffer);
      const selectedCodec = resolvePreferredCodec(fixedOffer);
      const filteredOffer = preferCodec(fixedOffer, selectedCodec);
      await rtc.setRemoteDescription({ type: 'offer', sdp: filteredOffer });
      const answer = await rtc.createAnswer();
      answer.sdp = mungeAnswerSdp(answer.sdp || '', cfg.maxBitrateKbps);
      await rtc.setLocalDescription(answer);
      const finalSdp = (await waitForIceGathering(rtc, 5000)) || rtc.localDescription?.sdp || answer.sdp || '';
      const effectiveCodec = detectNegotiatedCodec(finalSdp) || selectedCodec;
      const credentials = extractIceCredentials(finalSdp);
      const nvstSdp = buildNvstSdp({
        width: cfg.width,
        height: cfg.height,
        fps: cfg.fps,
        maxBitrateKbps: cfg.maxBitrateKbps,
        codec: effectiveCodec,
        colorQuality: '8bit',
        partialReliableThresholdMs,
        hidDeviceMask: riInputCapabilities.hidDeviceMask,
        enablePartiallyReliableTransferGamepad: riInputCapabilities.enablePartiallyReliableTransferGamepad,
        enablePartiallyReliableTransferHid: riInputCapabilities.enablePartiallyReliableTransferHid,
        credentials
      });
      send({
        peer_msg: {
          from: peerId,
          to: 1,
          msg: JSON.stringify({ type: 'answer', sdp: finalSdp, nvstSdp })
        },
        ackid: nextAck()
      });
      offerAccepted = true;
      await injectManualIce(rtc, cfg.mediaIp, cfg.mediaPort, serverIceUfrag);
      post('status', 'Offer accepted');
    } catch (error) {
      fail('Offer handling failed: ' + String(error));
    }
  }
  async function onRemoteIce(payload) {
    try {
      const rtc = ensurePeerConnection();
      await rtc.addIceCandidate({
        candidate: payload.candidate,
        sdpMid: payload.sdpMid ?? undefined,
        sdpMLineIndex: payload.sdpMLineIndex ?? undefined,
        usernameFragment: payload.usernameFragment ?? undefined
      });
    } catch (error) {
      log('Remote ICE add failed: ' + String(error));
    }
  }
  function handle(text) {
    let parsed;
    try { parsed = JSON.parse(text); } catch (_) { return; }
    if (parsed.hb) { send({ hb: 1 }); return; }
    if (typeof parsed.ackid === 'number') {
      const src = parsed.peer_info && parsed.peer_info.id;
      if (src !== peerId) send({ ack: parsed.ackid });
    }
    if (!parsed.peer_msg || !parsed.peer_msg.msg) return;
    let msg;
    try { msg = JSON.parse(parsed.peer_msg.msg); } catch (_) { return; }
    if (msg.type === 'offer' && typeof msg.sdp === 'string') {
      onOffer(msg.sdp);
      return;
    }
    if (typeof msg.candidate === 'string') {
      onRemoteIce(msg);
    }
  }
  const kbBar = document.getElementById('kbBar');
  const kbInput = document.getElementById('kbInput');
  const gpPad = document.getElementById('gpPad');
  const gpBtn = document.getElementById('gpBtn');
  const gpHide = document.getElementById('gpHide');
  const joyBase = document.getElementById('joyBase');
  const joyStick = document.getElementById('joyStick');
  const lookBase = document.getElementById('lookBase');
  const lookStick = document.getElementById('lookStick');
  let kbPrevLen = 0;
  let lastTX = 0, lastTY = 0;
  let tapStartX = 0, tapStartY = 0;
  let tStartTime = 0, tMoved = false, activeTouchId = null;
  let mouseGestureActive = false;
  let twoFingerStart = 0;
  let twoFingerTapPending = false;
  const MOVE_CLICK_CANCEL_PX = 8;
  const pointerSpeed = 1.05;
  let zoomScale = 1;
  let zoomTx = 0;
  let zoomTy = 0;
  let pinchStartDistance = 0;
  let pinchStartScale = 1;
  let pinchCenterStartX = 0;
  let pinchCenterStartY = 0;
  let pinchTranslateStartX = 0;
  let pinchTranslateStartY = 0;
  let pinchGestureMoved = false;
  const ZOOM_MIN = 1;
  const ZOOM_MAX = 3;
  const ZOOM_DEFAULT_STEP = 0.3;
  const MAX_MOUSE_DELTA_PER_FRAME = 64;
  const touchpad = document.getElementById('touchpad');
  const touchHint = document.getElementById('touchHint');
  let joystickTouchId = null;
  let joystickWindowCleanup = null;
  const joystickMaxOffset = 42;
  let lookTouchId = null;
  let lookWindowCleanup = null;
  const lookMaxOffset = 34;
  let virtualGamepadButtons = 0;
  let virtualLeftStickX = 0;
  let virtualLeftStickY = 0;
  let virtualRightStickX = 0;
  let virtualRightStickY = 0;
  let virtualLeftTrigger = 0;
  let virtualRightTrigger = 0;
  let gamepadBitmap = 0;
  let lastGamepadSendMs = 0;
  let lastSentGamepadState = null;
  function setGamepadVisible(visible, persist = true) {
    if (!gpPad) return;
    gpPad.style.display = visible ? 'block' : 'none';
    if (persist) {
      updateStreamerPreference('touchControllerVisible', !!visible);
    }
    updateTouchControllerButton();
  }

  function toggleKeyboard() {
    if (kbBar.style.display === 'none') showKeyboard();
    else hideKeyboard();
  }
  function updateKeyboardButton() {
    const keyboardVisible = kbBar && kbBar.style.display !== 'none';
    if (kbValue) {
      kbValue.textContent = keyboardVisible ? 'Visible' : 'Hidden';
    }
    setToggleRowState(kbBtn, keyboardVisible);
  }
  function showKeyboard() {
    unlockAudio();
    kbBar.style.display = 'block';
    kbInput.value = '';
    kbPrevLen = 0;
    setTimeout(() => kbInput.focus(), 80);
    updateKeyboardButton();
  }
  function hideKeyboard() {
    kbBar.style.display = 'none';
    kbInput.blur();
    updateKeyboardButton();
  }
  function toggleGamepad() {
    if (!gpPad) return;
    unlockAudio();
    setGamepadVisible(gpPad.style.display === 'none');
  }
  function hookLayoutDrag(key, element) {
    if (!element) return;
    const stopDrag = () => {
      layoutDragState.key = null;
      layoutDragState.pointerId = null;
    };
    const updateFromTouch = (touch) => {
      if (!touch || layoutDragState.key !== key) return;
      const x = (touch.clientX - layoutDragState.offsetX) / window.innerWidth;
      const y = (touch.clientY - layoutDragState.offsetY) / window.innerHeight;
      const nextLayout = { ...touchLayout };
      const fallback = touchLayout[key];
      nextLayout[key] = sanitizePoint({ x, y }, fallback);
      touchLayout = sanitizeTouchLayout(nextLayout);
      applyTouchLayout();
      scheduleTouchLayoutPersist();
    };
    element.addEventListener('touchstart', (e) => {
      if (!layoutEditing) return;
      const touch = e.changedTouches[0];
      if (!touch) return;
      e.preventDefault();
      e.stopPropagation();
      const rect = element.getBoundingClientRect();
      layoutDragState.key = key;
      layoutDragState.pointerId = touch.identifier;
      layoutDragState.offsetX = touch.clientX - (rect.left + (rect.width / 2));
      layoutDragState.offsetY = touch.clientY - (rect.top + (rect.height / 2));
    }, { passive: false });
    element.addEventListener('touchmove', (e) => {
      if (!layoutEditing || layoutDragState.key !== key) return;
      const touch = Array.from(e.touches).find((item) => item.identifier === layoutDragState.pointerId);
      if (!touch) return;
      e.preventDefault();
      updateFromTouch(touch);
    }, { passive: false });
    element.addEventListener('touchend', (e) => {
      if (!layoutEditing || layoutDragState.key !== key) return;
      const ended = Array.from(e.changedTouches).some((item) => item.identifier === layoutDragState.pointerId);
      if (!ended) return;
      e.preventDefault();
      stopDrag();
    }, { passive: false });
    element.addEventListener('touchcancel', () => {
      if (layoutDragState.key === key) {
        stopDrag();
      }
    }, { passive: false });
  }
  window.__opennowNativeGamepadState = function(state) {
    if (!state) {
      nativeGamepadState = null;
      return;
    }
    if (Array.isArray(state.controllers)) {
      const nextControllers = state.controllers.filter((controller) => controller && controller.connected);
      if (nextControllers.length === 0) {
        nativeGamepadState = null;
        return;
      }
      const preferredIndex = Number.isFinite(state.activeControllerIndex) ? state.activeControllerIndex : nextControllers[0].index;
      const activeController = nextControllers.find((controller) => controller.index === preferredIndex) ?? nextControllers[0];
      nativeGamepadState = {
        connected: true,
        id: activeController.id,
        index: activeController.index,
        buttons: activeController.buttons,
        axes: activeController.axes
      };
      return;
    }
    nativeGamepadState = state && state.connected ? state : null;
  };
  window.__opennowDeviceStatus = function(state) {
    deviceBatteryPercent = Number.isFinite(state?.batteryPercent) ? state.batteryPercent : null;
    deviceBatteryCharging = !!state?.charging;
    renderStatsOverlay();
  };
  function setControllerStatus(text) {
    if (!controllerState || text === lastControllerStatusText) return;
    lastControllerStatusText = text;
    controllerState.textContent = text;
  }
  function clamp(value, min, max) {
    return Math.max(min, Math.min(max, value));
  }
  function pinchDistance(t1, t2) {
    const dx = t2.clientX - t1.clientX;
    const dy = t2.clientY - t1.clientY;
    return Math.hypot(dx, dy);
  }
  function pinchCenter(t1, t2) {
    return { x: (t1.clientX + t2.clientX) / 2, y: (t1.clientY + t2.clientY) / 2 };
  }
  function clampZoomTranslation() {
    if (zoomScale <= 1) {
      zoomTx = 0;
      zoomTy = 0;
      return;
    }
    const maxX = ((window.innerWidth * zoomScale) - window.innerWidth) * 0.5;
    const maxY = ((window.innerHeight * zoomScale) - window.innerHeight) * 0.5;
    zoomTx = clamp(zoomTx, -maxX, maxX);
    zoomTy = clamp(zoomTy, -maxY, maxY);
  }
  function applyVideoTransform() {
    clampZoomTranslation();
    if (Math.abs(zoomScale - 1) < 0.01) {
      zoomScale = 1;
      zoomTx = 0;
      zoomTy = 0;
      video.style.transform = 'none';
      return;
    }
    video.style.transformOrigin = 'center center';
    video.style.transform = `translate(${zoomTx}px, ${zoomTy}px) scale(${zoomScale})`;
  }
  function adjustZoom(step) {
    const prev = zoomScale;
    zoomScale = clamp(zoomScale + step, ZOOM_MIN, ZOOM_MAX);
    if (Math.abs(zoomScale - prev) > 0.001) {
      if (zoomScale <= 1.01) {
        zoomScale = 1;
        zoomTx = 0;
        zoomTy = 0;
      }
      applyVideoTransform();
    }
  }
  function flushPendingMouseMove() {
    moveFrame = null;
    if (!inputReady) {
      pendingMoveDx = 0;
      pendingMoveDy = 0;
      return;
    }
    const dx = Math.round(clamp(pendingMoveDx, -MAX_MOUSE_DELTA_PER_FRAME, MAX_MOUSE_DELTA_PER_FRAME));
    const dy = Math.round(clamp(pendingMoveDy, -MAX_MOUSE_DELTA_PER_FRAME, MAX_MOUSE_DELTA_PER_FRAME));
    pendingMoveDx = 0;
    pendingMoveDy = 0;
    if (dx === 0 && dy === 0) return;
    sendPartialInput(encodeMouseMove(dx, dy));
  }

  const charKeyMap = {
    'a':{vk:0x41,sc:0x04},'b':{vk:0x42,sc:0x05},'c':{vk:0x43,sc:0x06},'d':{vk:0x44,sc:0x07},
    'e':{vk:0x45,sc:0x08},'f':{vk:0x46,sc:0x09},'g':{vk:0x47,sc:0x0a},'h':{vk:0x48,sc:0x0b},
    'i':{vk:0x49,sc:0x0c},'j':{vk:0x4a,sc:0x0d},'k':{vk:0x4b,sc:0x0e},'l':{vk:0x4c,sc:0x0f},
    'm':{vk:0x4d,sc:0x10},'n':{vk:0x4e,sc:0x11},'o':{vk:0x4f,sc:0x12},'p':{vk:0x50,sc:0x13},
    'q':{vk:0x51,sc:0x14},'r':{vk:0x52,sc:0x15},'s':{vk:0x53,sc:0x16},'t':{vk:0x54,sc:0x17},
    'u':{vk:0x55,sc:0x18},'v':{vk:0x56,sc:0x19},'w':{vk:0x57,sc:0x1a},'x':{vk:0x58,sc:0x1b},
    'y':{vk:0x59,sc:0x1c},'z':{vk:0x5a,sc:0x1d},
    '0':{vk:0x30,sc:0x27},'1':{vk:0x31,sc:0x1e},'2':{vk:0x32,sc:0x1f},'3':{vk:0x33,sc:0x20},
    '4':{vk:0x34,sc:0x21},'5':{vk:0x35,sc:0x22},'6':{vk:0x36,sc:0x23},'7':{vk:0x37,sc:0x24},
    '8':{vk:0x38,sc:0x25},'9':{vk:0x39,sc:0x26},
    ' ':{vk:0x20,sc:0x2c},'\n':{vk:0x0d,sc:0x28},'\r':{vk:0x0d,sc:0x28},'\t':{vk:0x09,sc:0x2b},
    '-':{vk:0xbd,sc:0x2d},'=':{vk:0xbb,sc:0x2e},'[':{vk:0xdb,sc:0x2f},']':{vk:0xdd,sc:0x30},
    '\\':{vk:0xdc,sc:0x31},';':{vk:0xba,sc:0x33},"'":{vk:0xde,sc:0x34},'`':{vk:0xc0,sc:0x35},
    ',':{vk:0xbc,sc:0x36},'.':{vk:0xbe,sc:0x37},'/':{vk:0xbf,sc:0x38},
    '!':{vk:0x31,sc:0x1e,sh:true},'@':{vk:0x32,sc:0x1f,sh:true},'#':{vk:0x33,sc:0x20,sh:true},
    '$':{vk:0x34,sc:0x21,sh:true},'%':{vk:0x35,sc:0x22,sh:true},'^':{vk:0x36,sc:0x23,sh:true},
    '&':{vk:0x37,sc:0x24,sh:true},'*':{vk:0x38,sc:0x25,sh:true},'(':{vk:0x39,sc:0x26,sh:true},
    ')':{vk:0x30,sc:0x27,sh:true},'_':{vk:0xbd,sc:0x2d,sh:true},'+':{vk:0xbb,sc:0x2e,sh:true},
    '{':{vk:0xdb,sc:0x2f,sh:true},'}':{vk:0xdd,sc:0x30,sh:true},'|':{vk:0xdc,sc:0x31,sh:true},
    ':':{vk:0xba,sc:0x33,sh:true},'"':{vk:0xde,sc:0x34,sh:true},'~':{vk:0xc0,sc:0x35,sh:true},
    '<':{vk:0xbc,sc:0x36,sh:true},'>':{vk:0xbe,sc:0x37,sh:true},'?':{vk:0xbf,sc:0x38,sh:true},
  };

  function lookupChar(ch) {
    const lower = ch.toLowerCase();
    if (charKeyMap[ch]) return charKeyMap[ch];
    if (charKeyMap[lower]) return { ...charKeyMap[lower], sh: ch !== lower };
    return null;
  }

  function sendChar(ch) {
    if (!inputReady) return;
    const spec = lookupChar(ch);
    if (!spec) return;
    const mods = spec.sh ? 0x01 : 0x00;
    if (spec.sh) sendInput(encodeKey(3, 0xA0, 0x2A, 0));
    sendInput(encodeKey(3, spec.vk, spec.sc, mods));
    sendInput(encodeKey(4, spec.vk, spec.sc, mods));
    if (spec.sh) sendInput(encodeKey(4, 0xA0, 0x2A, 0));
  }
  function sendVirtualKey(key, isDown) {
    const mapped = lookupChar(key);
    if (!mapped || !inputReady) return;
    const mods = mapped.sh ? 0x01 : 0x00;
    if (mapped.sh && isDown) sendInput(encodeKey(3, 0xA0, 0x2A, 0));
    sendInput(encodeKey(isDown ? 3 : 4, mapped.vk, mapped.sc, mods));
    if (mapped.sh && !isDown) sendInput(encodeKey(4, 0xA0, 0x2A, 0));
  }
  function hookVirtualGamepadButtons() {
    const btns = document.querySelectorAll('.gpKey');
    btns.forEach((btn) => {
      const mask = Number(btn.getAttribute('data-mask') || '0');
      const down = (e) => {
        if (layoutEditing) return;
        e.preventDefault();
        unlockAudio();
        btn.style.transform = 'scale(0.95)';
        if (mask) virtualGamepadButtons |= mask;
      };
      const up = (e) => {
        if (layoutEditing) return;
        e.preventDefault();
        btn.style.transform = 'scale(1)';
        if (mask) virtualGamepadButtons &= ~mask;
      };
      btn.addEventListener('touchstart', down, { passive: false });
      btn.addEventListener('touchend', up, { passive: false });
      btn.addEventListener('touchcancel', up, { passive: false });
      btn.style.background = 'rgba(30,30,30,0.72)';
      btn.style.color = '#fff';
      btn.style.border = '1px solid rgba(255,255,255,0.25)';
      btn.style.borderRadius = '14px';
      btn.style.backdropFilter = 'blur(8px)';
      btn.style.webkitBackdropFilter = 'blur(8px)';
      btn.style.fontSize = '18px';
    });
    const auxBtns = document.querySelectorAll('.gpAux');
    auxBtns.forEach((btn) => {
      const mask = Number(btn.getAttribute('data-mask') || '0');
      const trigger = btn.getAttribute('data-trigger');
      const down = (e) => {
        if (layoutEditing) return;
        e.preventDefault();
        unlockAudio();
        btn.style.transform = 'scale(0.95)';
        if (mask) virtualGamepadButtons |= mask;
        if (trigger === 'left') virtualLeftTrigger = 1;
        if (trigger === 'right') virtualRightTrigger = 1;
      };
      const up = (e) => {
        if (layoutEditing) return;
        e.preventDefault();
        btn.style.transform = 'scale(1)';
        if (mask) virtualGamepadButtons &= ~mask;
        if (trigger === 'left') virtualLeftTrigger = 0;
        if (trigger === 'right') virtualRightTrigger = 0;
      };
      btn.addEventListener('touchstart', down, { passive: false });
      btn.addEventListener('touchend', up, { passive: false });
      btn.addEventListener('touchcancel', up, { passive: false });
      btn.style.background = 'rgba(30,30,30,0.72)';
      btn.style.color = '#fff';
      btn.style.border = '1px solid rgba(255,255,255,0.25)';
      btn.style.borderRadius = btn.classList.contains('gpSmall') ? '12px' : '16px';
      btn.style.backdropFilter = 'blur(8px)';
      btn.style.webkitBackdropFilter = 'blur(8px)';
      btn.style.fontSize = btn.classList.contains('gpSmall') ? '12px' : '15px';
    });
  }
  function resetVirtualJoystick() {
    if (joyStick) {
      joyStick.style.transform = 'translate(-50%,-50%)';
    }
    virtualLeftStickX = 0;
    virtualLeftStickY = 0;
  }
  function resetLookStick() {
    if (lookStick) {
      lookStick.style.transform = 'translate(-50%,-50%)';
    }
    virtualRightStickX = 0;
    virtualRightStickY = 0;
  }
  function updateVirtualJoystickFromTouch(touch) {
    if (!joyBase || !touch) return;
    const rect = joyBase.getBoundingClientRect();
    const centerX = rect.left + (rect.width / 2);
    const centerY = rect.top + (rect.height / 2);
    let dx = touch.clientX - centerX;
    let dy = touch.clientY - centerY;
    const distance = Math.hypot(dx, dy);
    if (distance > joystickMaxOffset && distance > 0) {
      const scale = joystickMaxOffset / distance;
      dx *= scale;
      dy *= scale;
    }
    if (joyStick) {
      joyStick.style.transform = `translate(calc(-50% + ${dx}px), calc(-50% + ${dy}px))`;
    }
    virtualLeftStickX = clamp(dx / joystickMaxOffset, -1, 1);
    virtualLeftStickY = clamp(dy / joystickMaxOffset, -1, 1);
  }
  function updateLookStickFromTouch(touch) {
    if (!lookBase || !touch) return;
    const rect = lookBase.getBoundingClientRect();
    const centerX = rect.left + (rect.width / 2);
    const centerY = rect.top + (rect.height / 2);
    let dx = touch.clientX - centerX;
    let dy = touch.clientY - centerY;
    const distance = Math.hypot(dx, dy);
    if (distance > lookMaxOffset && distance > 0) {
      const scale = lookMaxOffset / distance;
      dx *= scale;
      dy *= scale;
    }
    if (lookStick) {
      lookStick.style.transform = `translate(calc(-50% + ${dx}px), calc(-50% + ${dy}px))`;
    }
    virtualRightStickX = clamp(dx / lookMaxOffset, -1, 1);
    virtualRightStickY = clamp(dy / lookMaxOffset, -1, 1);
  }
  function hookVirtualJoystick() {
    if (!joyBase) return;
    const detachWindowListeners = () => {
      if (typeof joystickWindowCleanup === 'function') {
        joystickWindowCleanup();
        joystickWindowCleanup = null;
      }
    };
    const onWindowMove = (e) => {
      if (joystickTouchId == null) return;
      const t = Array.from(e.touches).find((item) => item.identifier === joystickTouchId);
      if (!t) return;
      e.preventDefault();
      updateVirtualJoystickFromTouch(t);
    };
    const onWindowEnd = (e) => {
      if (joystickTouchId == null) return;
      const endedHere = Array.from(e.changedTouches).some((item) => item.identifier === joystickTouchId);
      if (!endedHere) return;
      e.preventDefault();
      joystickTouchId = null;
      detachWindowListeners();
      resetVirtualJoystick();
    };
    const onStart = (e) => {
      if (layoutEditing) return;
      if (joystickTouchId != null) return;
      e.preventDefault();
      e.stopPropagation();
      unlockAudio();
      detachWindowListeners();
      const t = e.changedTouches[0];
      if (!t) return;
      joystickTouchId = t.identifier;
      const opts = { passive: false, capture: true };
      window.addEventListener('touchmove', onWindowMove, opts);
      window.addEventListener('touchend', onWindowEnd, opts);
      window.addEventListener('touchcancel', onWindowEnd, opts);
      joystickWindowCleanup = () => {
        window.removeEventListener('touchmove', onWindowMove, opts);
        window.removeEventListener('touchend', onWindowEnd, opts);
        window.removeEventListener('touchcancel', onWindowEnd, opts);
      };
      updateVirtualJoystickFromTouch(t);
    };
    joyBase.addEventListener('touchstart', onStart, { passive: false, capture: true });
  }
  function hookLookStick() {
    if (!lookBase) return;
    const detachWindowListeners = () => {
      if (typeof lookWindowCleanup === 'function') {
        lookWindowCleanup();
        lookWindowCleanup = null;
      }
    };
    const onWindowMove = (e) => {
      if (lookTouchId == null) return;
      const t = Array.from(e.touches).find((item) => item.identifier === lookTouchId);
      if (!t) return;
      e.preventDefault();
      updateLookStickFromTouch(t);
    };
    const onWindowEnd = (e) => {
      if (lookTouchId == null) return;
      const endedHere = Array.from(e.changedTouches).some((item) => item.identifier === lookTouchId);
      if (!endedHere) return;
      e.preventDefault();
      lookTouchId = null;
      detachWindowListeners();
      resetLookStick();
    };
    const onStart = (e) => {
      if (layoutEditing) return;
      if (lookTouchId != null) return;
      e.preventDefault();
      e.stopPropagation();
      unlockAudio();
      detachWindowListeners();
      const t = e.changedTouches[0];
      if (!t) return;
      lookTouchId = t.identifier;
      const opts = { passive: false, capture: true };
      window.addEventListener('touchmove', onWindowMove, opts);
      window.addEventListener('touchend', onWindowEnd, opts);
      window.addEventListener('touchcancel', onWindowEnd, opts);
      lookWindowCleanup = () => {
        window.removeEventListener('touchmove', onWindowMove, opts);
        window.removeEventListener('touchend', onWindowEnd, opts);
        window.removeEventListener('touchcancel', onWindowEnd, opts);
      };
      updateLookStickFromTouch(t);
    };
    lookBase.addEventListener('touchstart', onStart, { passive: false, capture: true });
  }
  function clearVirtualGamepadState() {
    virtualGamepadButtons = 0;
    virtualLeftStickX = 0;
    virtualLeftStickY = 0;
    virtualRightStickX = 0;
    virtualRightStickY = 0;
    virtualLeftTrigger = 0;
    virtualRightTrigger = 0;
    if (joyStick) {
      joyStick.style.transform = 'translate(-50%,-50%)';
    }
    if (lookStick) {
      lookStick.style.transform = 'translate(-50%,-50%)';
    }
  }
  function normalizeGamepadAxis(value) {
    const clamped = clamp(value || 0, -1, 1);
    if (Math.abs(clamped) < GAMEPAD_DEADZONE) return 0;
    return clamp(clamped < 0 ? Math.round(clamped * 32768) : Math.round(clamped * 32767), -32768, 32767);
  }
  function normalizeTriggerValue(value) {
    return clamp(Math.round(clamp(value || 0, 0, 1) * 255), 0, 255);
  }
  function buttonValue(buttons, index) {
    const raw = buttons?.[index];
    if (raw == null) return 0;
    if (typeof raw === 'number') return raw;
    if (typeof raw.value === 'number') return raw.value;
    return raw.pressed ? 1 : 0;
  }
  function mapGamepadButtons(buttons) {
    let mapped = 0;
    if (buttonValue(buttons, 0) > 0) mapped |= GAMEPAD_A;
    if (buttonValue(buttons, 1) > 0) mapped |= GAMEPAD_B;
    if (buttonValue(buttons, 2) > 0) mapped |= GAMEPAD_X;
    if (buttonValue(buttons, 3) > 0) mapped |= GAMEPAD_Y;
    if (buttonValue(buttons, 4) > 0) mapped |= GAMEPAD_LB;
    if (buttonValue(buttons, 5) > 0) mapped |= GAMEPAD_RB;
    if (buttonValue(buttons, 8) > 0) mapped |= GAMEPAD_BACK;
    if (buttonValue(buttons, 9) > 0) mapped |= GAMEPAD_START;
    if (buttonValue(buttons, 10) > 0) mapped |= GAMEPAD_LS;
    if (buttonValue(buttons, 11) > 0) mapped |= GAMEPAD_RS;
    if (buttonValue(buttons, 12) > 0) mapped |= GAMEPAD_DPAD_UP;
    if (buttonValue(buttons, 13) > 0) mapped |= GAMEPAD_DPAD_DOWN;
    if (buttonValue(buttons, 14) > 0) mapped |= GAMEPAD_DPAD_LEFT;
    if (buttonValue(buttons, 15) > 0) mapped |= GAMEPAD_DPAD_RIGHT;
    if (buttonValue(buttons, 16) > 0) mapped |= GAMEPAD_GUIDE;
    return mapped;
  }
  function buildGamepadInputFromState(state, controllerId = 0) {
    const axes = state?.axes || [0, 0, 0, 0];
    const buttons = state?.buttons || [];
    return {
      controllerId,
      connected: !!state?.connected,
      buttons: mapGamepadButtons(buttons),
      leftTrigger: normalizeTriggerValue(buttonValue(buttons, 6) || state?.leftTrigger),
      rightTrigger: normalizeTriggerValue(buttonValue(buttons, 7) || state?.rightTrigger),
      leftStickX: normalizeGamepadAxis(axes[0] || 0),
      leftStickY: normalizeGamepadAxis(-(axes[1] || 0)),
      rightStickX: normalizeGamepadAxis(axes[2] || 0),
      rightStickY: normalizeGamepadAxis(-(axes[3] || 0))
    };
  }
  function currentVirtualGamepadState() {
    return {
      id: 'OpenNOW Virtual Controller',
      connected: gpPad && gpPad.style.display !== 'none',
      buttons: [
        (virtualGamepadButtons & GAMEPAD_A) ? 1 : 0,
        (virtualGamepadButtons & GAMEPAD_B) ? 1 : 0,
        (virtualGamepadButtons & GAMEPAD_X) ? 1 : 0,
        (virtualGamepadButtons & GAMEPAD_Y) ? 1 : 0,
        (virtualGamepadButtons & GAMEPAD_LB) ? 1 : 0,
        (virtualGamepadButtons & GAMEPAD_RB) ? 1 : 0,
        virtualLeftTrigger, virtualRightTrigger,
        (virtualGamepadButtons & GAMEPAD_BACK) ? 1 : 0,
        (virtualGamepadButtons & GAMEPAD_START) ? 1 : 0,
        (virtualGamepadButtons & GAMEPAD_LS) ? 1 : 0,
        (virtualGamepadButtons & GAMEPAD_RS) ? 1 : 0,
        0, 0, 0, 0,
        (virtualGamepadButtons & GAMEPAD_GUIDE) ? 1 : 0
      ],
      axes: [virtualLeftStickX, virtualLeftStickY, virtualRightStickX, virtualRightStickY],
      leftTrigger: virtualLeftTrigger,
      rightTrigger: virtualRightTrigger
    };
  }
  function selectActiveGamepadSource() {
    if (streamerPreferences.physicalControllerPassthrough && nativeGamepadState?.connected) {
      const nativeIndex = Number.isFinite(nativeGamepadState.index) ? nativeGamepadState.index : 0;
      return { label: nativeGamepadState.id || 'iOS Controller', input: buildGamepadInputFromState(nativeGamepadState, nativeIndex) };
    }
    const pads = navigator.getGamepads ? Array.from(navigator.getGamepads()).filter(Boolean) : [];
    const browserPad = pads.find((pad) => pad && pad.connected);
    if (browserPad) {
      return { label: browserPad.id || 'connected', input: buildGamepadInputFromState(browserPad, browserPad.index & 0x03) };
    }
    const virtualState = currentVirtualGamepadState();
    if (virtualState.connected) {
      return { label: virtualState.id, input: buildGamepadInputFromState(virtualState, 0) };
    }
    return null;
  }
  function gamepadStatesEqual(a, b) {
    return !!a && !!b
      && a.buttons === b.buttons
      && a.leftTrigger === b.leftTrigger
      && a.rightTrigger === b.rightTrigger
      && a.leftStickX === b.leftStickX
      && a.leftStickY === b.leftStickY
      && a.rightStickX === b.rightStickX
      && a.rightStickY === b.rightStickY
      && a.controllerId === b.controllerId
      && a.connected === b.connected;
  }
  function sendCurrentGamepadState(state) {
    const controllerId = connectedControllerId(state);
    const connected = !!state?.connected;
    const bitmap = connected ? (1 << controllerId) : 0x0000;
    const usePR = isChannelOpen(partialCh)
      && (riInputCapabilities.enablePartiallyReliableTransferGamepad & bitmap) !== 0;
    const payload = encodeGamepadState({
      controllerId,
      buttons: state?.buttons ?? 0,
      leftTrigger: state?.leftTrigger ?? 0,
      rightTrigger: state?.rightTrigger ?? 0,
      leftStickX: state?.leftStickX ?? 0,
      leftStickY: state?.leftStickY ?? 0,
      rightStickX: state?.rightStickX ?? 0,
      rightStickY: state?.rightStickY ?? 0
    }, bitmap, usePR);
    if (usePR) partialCh.send(payload);
    else sendInput(payload);
    gamepadBitmap = bitmap;
    lastSentGamepadState = connected ? { ...state } : null;
    lastGamepadSendMs = performance.now();
  }
  function connectedControllerId(state) {
    if (state?.connected && Number.isFinite(state.controllerId)) {
      return state.controllerId & 0x03;
    }
    if (lastSentGamepadState && Number.isFinite(lastSentGamepadState.controllerId)) {
      return lastSentGamepadState.controllerId & 0x03;
    }
    return 0;
  }
  function releaseAllPadKeys() {
    joystickTouchId = null;
    lookTouchId = null;
    if (typeof joystickWindowCleanup === 'function') {
      joystickWindowCleanup();
      joystickWindowCleanup = null;
    }
    if (typeof lookWindowCleanup === 'function') {
      lookWindowCleanup();
      lookWindowCleanup = null;
    }
    clearVirtualGamepadState();
  }
  function pollGamepadState() {
    const activeSource = selectActiveGamepadSource();
    if (!activeSource) {
      clearVirtualGamepadState();
      if (lastSentGamepadState || gamepadBitmap !== 0) {
        sendCurrentGamepadState(null);
      }
      setControllerStatus('Controller: waiting…');
      requestAnimationFrame(pollGamepadState);
      return;
    }
    const nextControllerStatus = `Controller: ${activeSource.label}`;
    const controllerChanged = nextControllerStatus !== lastControllerStatusText;
    setControllerStatus(nextControllerStatus);
    if (controllerChanged && !userMuted) {
      unlockAudio();
    }
    const nextState = activeSource.input;
    const nowMs = performance.now();
    const changed = !gamepadStatesEqual(nextState, lastSentGamepadState);
    const keepalive = !changed && (nowMs - lastGamepadSendMs) >= GAMEPAD_KEEPALIVE_MS;
    if (changed || keepalive) {
      sendCurrentGamepadState(nextState);
    }
    requestAnimationFrame(pollGamepadState);
  }
  window.addEventListener('blur', releaseAllPadKeys);

  setTimeout(() => { if (touchHint) touchHint.style.opacity = '0'; }, 4000);

  touchpad.addEventListener('touchstart', (e) => {
    if (FORTNITE_NATIVE_TOUCH) return;
    e.preventDefault();
    unlockAudio();
    const touches = e.targetTouches;
    if (touches.length === 2) {
      const t1 = touches[0];
      const t2 = touches[1];
      const center = pinchCenter(t1, t2);
      pinchStartDistance = pinchDistance(t1, t2);
      pinchStartScale = zoomScale;
      pinchCenterStartX = center.x;
      pinchCenterStartY = center.y;
      pinchTranslateStartX = zoomTx;
      pinchTranslateStartY = zoomTy;
      pinchGestureMoved = false;
      twoFingerStart = Date.now();
      twoFingerTapPending = !mouseGestureActive;
      activeTouchId = null;
      return;
    }
    const t = touches[0];
    if (!t) return;
    mouseGestureActive = true;
    activeTouchId = t.identifier;
    lastTX = t.clientX;
    lastTY = t.clientY;
    tapStartX = t.clientX;
    tapStartY = t.clientY;
    tStartTime = Date.now();
    tMoved = false;
    twoFingerTapPending = false;
  }, { passive: false });

  touchpad.addEventListener('touchmove', (e) => {
    if (FORTNITE_NATIVE_TOUCH) return;
    e.preventDefault();
    const touches = e.targetTouches;
    if (touches.length === 2) {
      if (mouseGestureActive && zoomScale <= 1.01) {
        twoFingerTapPending = false;
        return;
      }
      const t1 = touches[0];
      const t2 = touches[1];
      const distance = pinchDistance(t1, t2);
      const center = pinchCenter(t1, t2);
      if (pinchStartDistance > 0) {
        const nextScale = clamp((distance / pinchStartDistance) * pinchStartScale, ZOOM_MIN, ZOOM_MAX);
        if (Math.abs(nextScale - zoomScale) > 0.04 || Math.abs(distance - pinchStartDistance) > 10) {
          pinchGestureMoved = true;
        }
        zoomScale = nextScale;
      }
      if (zoomScale > 1.01) {
        zoomTx = pinchTranslateStartX + (center.x - pinchCenterStartX);
        zoomTy = pinchTranslateStartY + (center.y - pinchCenterStartY);
        if (Math.abs(center.x - pinchCenterStartX) > 8 || Math.abs(center.y - pinchCenterStartY) > 8) {
          pinchGestureMoved = true;
        }
      }
      if (pinchGestureMoved) {
        twoFingerTapPending = false;
      }
      applyVideoTransform();
      return;
    }
    const t = Array.from(touches).find((item) => item.identifier === activeTouchId) || touches[0];
    if (!t) return;
    const dxRaw = t.clientX - lastTX;
    const dyRaw = t.clientY - lastTY;
    const dx = dxRaw * pointerSpeed;
    const dy = dyRaw * pointerSpeed;
    lastTX = t.clientX;
    lastTY = t.clientY;
    if (Math.abs(t.clientX - tapStartX) > MOVE_CLICK_CANCEL_PX || Math.abs(t.clientY - tapStartY) > MOVE_CLICK_CANCEL_PX) {
      tMoved = true;
    }
    if ((Math.abs(dxRaw) > 0 || Math.abs(dyRaw) > 0) && inputReady) {
      tMoved = true;
      pendingMoveDx += dx;
      pendingMoveDy += dy;
      if (!moveFrame) {
        moveFrame = requestAnimationFrame(flushPendingMouseMove);
      }
    }
  }, { passive: false });

  touchpad.addEventListener('touchend', (e) => {
    if (FORTNITE_NATIVE_TOUCH) return;
    e.preventDefault();
    const remainingTouches = e.targetTouches;
    if (remainingTouches.length === 0 && zoomScale > 1.01 && pinchGestureMoved) {
      pinchGestureMoved = false;
      activeTouchId = null;
      mouseGestureActive = false;
      twoFingerTapPending = false;
      return;
    }
    if (!inputReady) return;
    if (moveFrame) {
      cancelAnimationFrame(moveFrame);
      moveFrame = null;
    }
    flushPendingMouseMove();
    const holdMs = Date.now() - tStartTime;
    if (!tMoved && holdMs < 500) {
      if (e.changedTouches.length === 1 && remainingTouches.length === 0) {
        sendInput(encodeMouseButton(8, 1));
        setTimeout(() => sendInput(encodeMouseButton(9, 1)), 35);
      }
    }
    if (twoFingerTapPending && zoomScale <= 1.01 && remainingTouches.length === 0 && Date.now() - twoFingerStart < 400) {
      sendInput(encodeMouseButton(8, 3));
      setTimeout(() => sendInput(encodeMouseButton(9, 3)), 35);
    }
    activeTouchId = null;
    mouseGestureActive = false;
    twoFingerTapPending = false;
  }, { passive: false });

  touchpad.addEventListener('dblclick', (e) => {
    if (FORTNITE_NATIVE_TOUCH) return;
    e.preventDefault();
    if (zoomScale > 1.01) {
      zoomScale = 1;
      zoomTx = 0;
      zoomTy = 0;
      applyVideoTransform();
    } else {
      adjustZoom(ZOOM_DEFAULT_STEP);
    }
  }, { passive: false });

  kbInput.addEventListener('input', (e) => {
    const val = kbInput.value;
    if (val.length > kbPrevLen) {
      const added = val.slice(kbPrevLen);
      for (const ch of added) sendChar(ch);
    } else if (val.length < kbPrevLen) {
      if (inputReady) {
        sendInput(encodeKey(3, 0x08, 0x0E, 0));
        sendInput(encodeKey(4, 0x08, 0x0E, 0));
      }
    }
    kbPrevLen = val.length;
  });

  kbInput.addEventListener('keydown', (e) => {
    if (e.key.length === 1 && !e.metaKey && !e.ctrlKey && !e.altKey && !e.isComposing) {
      e.preventDefault();
      sendChar(e.key);
      return;
    }
    if (e.key === 'Backspace') {
      e.preventDefault();
      if (inputReady) {
        sendInput(encodeKey(3, 0x08, 0x0E, 0));
        sendInput(encodeKey(4, 0x08, 0x0E, 0));
      }
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      if (inputReady) {
        sendInput(encodeKey(3, 0x0d, 0x28, 0));
        sendInput(encodeKey(4, 0x0d, 0x28, 0));
      }
      return;
    }
    if (e.key === 'Tab') {
      e.preventDefault();
      if (inputReady) {
        sendInput(encodeKey(3, 0x09, 0x2B, 0));
        sendInput(encodeKey(4, 0x09, 0x2B, 0));
      }
      return;
    }
    if (e.key === 'Escape') hideKeyboard();
  });

  touchpad.addEventListener('wheel', (e) => {
    if (FORTNITE_NATIVE_TOUCH) return;
    e.preventDefault();
    adjustZoom(e.deltaY < 0 ? ZOOM_DEFAULT_STEP : -ZOOM_DEFAULT_STEP);
  }, { passive: false });

  function connect() {
    try {
      resetTransport(true);
      const signIn = buildSignInUrl();
      post('status', 'Connecting signaling');
      ws = new WebSocket(signIn, 'x-nv-sessionid.' + cfg.sessionId);
      signalingOpenTimeout = setTimeout(() => {
        if (!ws || ws.readyState !== WebSocket.OPEN) {
          post('status', 'Signaling connect timeout');
          try { if (ws) ws.close(); } catch (_) {}
          scheduleReconnect('socket timeout');
        }
      }, 8000);
      ws.onopen = () => {
        if (signalingOpenTimeout) {
          clearTimeout(signalingOpenTimeout);
          signalingOpenTimeout = null;
        }
        reconnectAttempts = 0;
        sendPeerInfo();
        if (hb) clearInterval(hb);
        hb = setInterval(() => send({ hb: 1 }), 5000);
        post('status', 'Signaling connected');
        startOfferTimeout();
      };
      ws.onmessage = (event) => handle(event.data);
      ws.onerror = () => {
        if (shouldKeepPeerAliveOnSignalingClose()) {
          post('status', 'Signaling error (ignored after offer)');
          return;
        }
        post('status', 'Signaling error, retrying');
        clearOfferTimeout();
        scheduleReconnect('socket error');
      };
      ws.onclose = (event) => {
        clearOfferTimeout();
        const reason = event && event.reason ? event.reason : 'no reason';
        post('status', 'Signaling closed (' + event.code + '): ' + reason);
        if (shouldKeepPeerAliveOnSignalingClose()) {
          if (hb) { clearInterval(hb); hb = null; }
          ws = null;
          post('status', 'Continuing stream without signaling');
          return;
        }
        resetTransport();
        scheduleReconnect('socket closed');
      };
    } catch (error) {
      fail('Signaling setup failed: ' + String(error));
      scheduleReconnect('setup failed');
    }
  }
  hookVirtualGamepadButtons();
  hookVirtualJoystick();
  hookLookStick();
  Object.entries(touchGroupElements).forEach(([key, element]) => hookLayoutDrag(key, element));
  if (gpHide) {
    gpHide.onclick = () => {
      if (layoutEditing) return;
      clearVirtualGamepadState();
      setGamepadVisible(false);
    };
  }
  if (gpEditBtn) {
    gpEditBtn.onclick = () => {
      setLayoutEditing(!layoutEditing);
    };
  }
  if (gpResetBtn) {
    gpResetBtn.onclick = () => {
      resetTouchLayout();
      setLayoutEditing(true);
    };
  }
  if (gpScaleRange) {
    gpScaleRange.addEventListener('input', (e) => {
      const value = Number(e.target && e.target.value);
      touchLayout = sanitizeTouchLayout({
        ...touchLayout,
        scale: (Number.isFinite(value) ? value : 100) / 100
      });
      applyTouchLayout();
      scheduleTouchLayoutPersist();
    });
  }
  if (gpButtonRange) {
    gpButtonRange.addEventListener('input', (e) => {
      const value = Number(e.target && e.target.value);
      touchLayout = sanitizeTouchLayout({
        ...touchLayout,
        buttonScale: (Number.isFinite(value) ? value : 100) / 100
      });
      applyTouchLayout();
      scheduleTouchLayoutPersist();
    });
  }
  if (gpStickRange) {
    gpStickRange.addEventListener('input', (e) => {
      const value = Number(e.target && e.target.value);
      touchLayout = sanitizeTouchLayout({
        ...touchLayout,
        stickScale: (Number.isFinite(value) ? value : 100) / 100
      });
      applyTouchLayout();
      scheduleTouchLayoutPersist();
    });
  }
  if (gpOpacityRange) {
    gpOpacityRange.addEventListener('input', (e) => {
      const value = Number(e.target && e.target.value);
      touchLayout = sanitizeTouchLayout({
        ...touchLayout,
        opacity: (Number.isFinite(value) ? value : 58) / 100
      });
      applyTouchLayout();
      scheduleTouchLayoutPersist();
    });
  }
  if (hudPanel) {
    hudPanel.addEventListener('touchstart', (e) => e.stopPropagation(), { passive: true });
  }
  document.addEventListener('touchstart', (e) => {
    if (!hudPanelOpen) return;
    const target = e.target;
    if (hudPanel && !hudPanel.contains(target) && hudToggle && !hudToggle.contains(target)) {
      toggleHudPanel(false);
    }
  }, { passive: true });
  setGamepadVisible(streamerPreferences.touchControllerVisible, false);
  applyTouchpadMode();
  applyTouchLayout();
  applyStatsVisibility();
  video.addEventListener('volumechange', updateAudioButton);
  updateAudioButton();
  updateStatsButton();
  updateKeyboardButton();
  updateTouchControllerButton();
  updatePhysicalControllerButton();
  updateHudSummary();
  pollGamepadState();
  // GPU keep-alive: minimal WebGL rAF loop prevents GPUProcess idle-exit during
  // WebRTC negotiation. Runs until the WKWebView is destroyed (streamer dismissed).
  (function() {
    var c = document.createElement('canvas');
    c.width = c.height = 1;
    c.style.cssText = 'position:fixed;width:1px;height:1px;opacity:0;pointer-events:none;z-index:-1;';
    document.body.appendChild(c);
    var gl = c.getContext('webgl') || c.getContext('experimental-webgl');
    if (!gl) { c.remove(); return; }
    (function loop() { gl.clear(gl.COLOR_BUFFER_BIT); requestAnimationFrame(loop); })();
  })();
  // WebContent keep-alive: active media playback prevents iOS from suspending the
  // WebContent process during WebRTC negotiation (no browser-engine entitlements).
  try { video.srcObject = new MediaStream(); video.play().catch(function(){}); } catch(e) {}
  connect();
  </script>
</body>
</html>
"""#
    }

    private static func normalizePreferredCodec(_ codec: String) -> String {
        // iOS WKWebView is currently most reliable with H264 decode path.
        // Force non-H264 selections to H264 to avoid connected-but-black video.
        switch codec.uppercased() {
        case "H264":
            return "H264"
        default:
            return "H264"
        }
    }

    private static func touchProfile(for gameTitle: String) -> String {
        let normalized = gameTitle.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if normalized.contains("fortnite") {
            return "fortnite-mobile"
        }
        return "default"
    }

    private static func streamProfile(for settings: AppSettings) -> StreamProfile {
        let nativeBounds = UIScreen.main.nativeBounds
        let longSide = max(nativeBounds.width, nativeBounds.height)
        let shortSide = min(nativeBounds.width, nativeBounds.height)
        let supports1440 = longSide >= 2500 || shortSide >= 1400 || UIScreen.main.nativeScale >= 3.0
        let effectiveFPS = min(settings.preferredFPS, 60)
        let isPad = UIDevice.current.userInterfaceIdiom == .pad
        let quality = settings.preferredQuality.lowercased()
        let prefersQuality = quality == "quality"
        let prefersDataSaver = quality == "data saver"

        if isPad, prefersQuality, supports1440, effectiveFPS >= 60 {
            return StreamProfile(width: 2560, height: 1440, maxBitrateKbps: 36_000)
        }
        if isPad {
            let bitrate = prefersQuality ? 24_000 : (prefersDataSaver ? 12_000 : 18_000)
            return StreamProfile(width: 1920, height: 1080, maxBitrateKbps: bitrate)
        }
        let bitrate = prefersQuality ? 18_000 : (prefersDataSaver ? 8_500 : 13_000)
        return StreamProfile(width: 1280, height: 720, maxBitrateKbps: bitrate)
    }

    final class Coordinator: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
        private let onEvent: (String) -> Void
        private let onTouchLayoutChange: (String, TouchControlLayout) -> Void
        private let onStreamerPreferencesChange: (StreamerPreferences) -> Void
        var cachedHTML: String = ""
        var cachedBaseURL: URL?
        private var contentProcessRestartCount = 0
        private static let maxContentProcessRestarts = 5
        private let controllerBridge = NativeControllerBridge()
        private let deviceStatusBridge = NativeDeviceStatusBridge()

        init(
            onEvent: @escaping (String) -> Void,
            onTouchLayoutChange: @escaping (String, TouchControlLayout) -> Void,
            onStreamerPreferencesChange: @escaping (StreamerPreferences) -> Void
        ) {
            self.onEvent = onEvent
            self.onTouchLayoutChange = onTouchLayoutChange
            self.onStreamerPreferencesChange = onStreamerPreferencesChange
        }

        func attach(webView: WKWebView) {
            controllerBridge.attach(webView: webView)
            deviceStatusBridge.attach(webView: webView)
        }

        func detach() {
            controllerBridge.detach()
            deviceStatusBridge.detach()
        }

        func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
            guard let body = message.body as? [String: Any] else { return }
            let type = (body["type"] as? String) ?? "log"
            let msg = (body["message"] as? String) ?? ""
            if msg.localizedCaseInsensitiveContains("input handshake complete") || msg == "Input ready" {
                return
            }
            switch type {
            case "status":
                onEvent("Status: \(msg)")
            case "error":
                onEvent("Error: \(msg)")
            case "log":
                onEvent("Log: \(msg)")
            case "touch-layout":
                guard let payload = body["payload"] as? [String: Any],
                      let profile = payload["profile"] as? String,
                      let layoutObject = payload["layout"],
                      JSONSerialization.isValidJSONObject(layoutObject),
                      let layoutData = try? JSONSerialization.data(withJSONObject: layoutObject),
                      let layout = try? JSONDecoder().decode(TouchControlLayout.self, from: layoutData) else {
                    return
                }
                onTouchLayoutChange(profile, layout)
            case "streamer-preferences":
                guard let payload = body["payload"],
                      JSONSerialization.isValidJSONObject(payload),
                      let preferencesData = try? JSONSerialization.data(withJSONObject: payload),
                      let preferences = try? JSONDecoder().decode(StreamerPreferences.self, from: preferencesData) else {
                    return
                }
                onStreamerPreferencesChange(preferences)
            default:
                break
            }
        }

        func webView(
            _ webView: WKWebView,
            didFail navigation: WKNavigation!,
            withError error: Error
        ) {
            onEvent("Error: WebView navigation failed: \(error.localizedDescription)")
        }

        func webView(
            _ webView: WKWebView,
            didFailProvisionalNavigation navigation: WKNavigation!,
            withError error: Error
        ) {
            onEvent("Error: WebView provisional load failed: \(error.localizedDescription)")
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            controllerBridge.attach(webView: webView)
            deviceStatusBridge.attach(webView: webView)
        }

        func webViewWebContentProcessDidTerminate(_ webView: WKWebView) {
            contentProcessRestartCount += 1
            if contentProcessRestartCount <= Self.maxContentProcessRestarts, !cachedHTML.isEmpty {
                onEvent("Status: Reconnecting after process crash (\(contentProcessRestartCount)/\(Self.maxContentProcessRestarts))...")
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.0) { [weak webView] in
                    guard let webView else { return }
                    webView.loadHTMLString(self.cachedHTML, baseURL: self.cachedBaseURL)
                }
            } else {
                onEvent("Error: Stream WebContent process terminated (restart limit reached)")
            }
        }
    }
}

private final class NativeDeviceStatusBridge {
    private weak var webView: WKWebView?
    private var observers: [NSObjectProtocol] = []
    private var refreshTimer: Timer?
    private var wasBatteryMonitoringEnabled = false
    private var lastJSON = ""

    func attach(webView: WKWebView) {
        self.webView = webView
        if observers.isEmpty {
            startMonitoring()
        }
        startRefreshTimer()
        publishState()
    }

    func detach() {
        observers.forEach(NotificationCenter.default.removeObserver)
        observers.removeAll()
        refreshTimer?.invalidate()
        refreshTimer = nil
        webView = nil
        lastJSON = ""
        if !wasBatteryMonitoringEnabled {
            UIDevice.current.isBatteryMonitoringEnabled = false
        }
    }

    private func startMonitoring() {
        let device = UIDevice.current
        wasBatteryMonitoringEnabled = device.isBatteryMonitoringEnabled
        device.isBatteryMonitoringEnabled = true

        let center = NotificationCenter.default
        observers.append(center.addObserver(
            forName: UIDevice.batteryLevelDidChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.publishState()
        })
        observers.append(center.addObserver(
            forName: UIDevice.batteryStateDidChangeNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.publishState()
        })
        observers.append(center.addObserver(
            forName: UIApplication.didBecomeActiveNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.publishState()
        })
    }

    private func startRefreshTimer() {
        guard refreshTimer == nil else { return }
        refreshTimer = Timer.scheduledTimer(withTimeInterval: 30, repeats: true) { [weak self] _ in
            self?.publishState()
        }
    }

    private func publishState() {
        guard let webView else { return }
        let level = UIDevice.current.batteryLevel
        let percent = level >= 0 ? Int((level * 100).rounded()) : nil
        let charging = {
            switch UIDevice.current.batteryState {
            case .charging, .full:
                return true
            default:
                return false
            }
        }()
        let payload: [String: Any] = [
            "batteryPercent": percent ?? NSNull(),
            "charging": charging
        ]
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return }
        guard json != lastJSON else { return }
        lastJSON = json
        webView.evaluateJavaScript("window.__opennowDeviceStatus(\(json))", completionHandler: nil)
    }
}

private final class NativeControllerBridge {
    private weak var webView: WKWebView?
    private var observers: [NSObjectProtocol] = []
    private var controllers: [GCController] = []
    private var activeControllerIndex = 0
    private var publishScheduled = false

    func attach(webView: WKWebView) {
        self.webView = webView
        if observers.isEmpty {
            startMonitoring()
        }
        attachCurrentControllers()
        publishState()
    }

    func detach() {
        observers.forEach(NotificationCenter.default.removeObserver)
        observers.removeAll()
        clearControllerHandlers()
        controllers = []
        activeControllerIndex = 0
        webView = nil
        publishScheduled = false
    }

    private func startMonitoring() {
        let center = NotificationCenter.default
        observers.append(center.addObserver(
            forName: .GCControllerDidConnect,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.attachCurrentControllers()
            self?.publishState()
        })
        observers.append(center.addObserver(
            forName: .GCControllerDidDisconnect,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            self?.attachCurrentControllers()
            self?.publishState()
        })
    }

    private func attachCurrentControllers() {
        let nextControllers = GCController.controllers().filter { $0.extendedGamepad != nil }
        let identityChanged = nextControllers.count != controllers.count
            || zip(nextControllers, controllers).contains(where: { lhs, rhs in lhs !== rhs })
        guard identityChanged else { return }

        clearControllerHandlers()
        controllers = nextControllers
        if controllers.isEmpty {
            activeControllerIndex = 0
            return
        }
        activeControllerIndex = min(activeControllerIndex, controllers.count - 1)
        for (index, controller) in controllers.enumerated() {
            controller.extendedGamepad?.valueChangedHandler = { [weak self] _, _ in
                self?.activeControllerIndex = index
                self?.schedulePublish()
            }
            controller.controllerPausedHandler = { [weak self] _ in
                self?.activeControllerIndex = index
                self?.schedulePublish()
            }
        }
    }

    private func clearControllerHandlers() {
        for controller in controllers {
            controller.extendedGamepad?.valueChangedHandler = nil
            controller.controllerPausedHandler = nil
        }
    }

    private func schedulePublish() {
        guard !publishScheduled else { return }
        publishScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.01) { [weak self] in
            guard let self else { return }
            self.publishScheduled = false
            self.publishState()
        }
    }

    private func publishState() {
        guard let webView else { return }
        let payload = controllerPayload()
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let json = String(data: data, encoding: .utf8)
        else { return }
        webView.evaluateJavaScript("window.__opennowNativeGamepadState(\(json))", completionHandler: nil)
    }

    private func controllerPayload() -> [String: Any] {
        let payloads = controllers.enumerated().compactMap { payload(for: $0.element, index: $0.offset) }
        guard !payloads.isEmpty else {
            return ["connected": false]
        }
        let safeActiveIndex = min(activeControllerIndex, payloads.count - 1)
        let active = payloads[safeActiveIndex]
        return [
            "connected": true,
            "activeControllerIndex": active["index"] as? Int ?? 0,
            "controllers": payloads,
            "id": active["id"] as? String ?? "iOS Controller",
            "index": active["index"] as? Int ?? 0,
            "buttons": active["buttons"] as? [Double] ?? [],
            "axes": active["axes"] as? [Double] ?? []
        ]
    }

    private func payload(for controller: GCController, index: Int) -> [String: Any]? {
        guard let gamepad = controller.extendedGamepad else { return nil }

        var buttons = Array(repeating: 0.0, count: 17)
        buttons[0] = value(for: gamepad.buttonA)
        buttons[1] = value(for: gamepad.buttonB)
        buttons[2] = value(for: gamepad.buttonX)
        buttons[3] = value(for: gamepad.buttonY)
        buttons[4] = value(for: gamepad.leftShoulder)
        buttons[5] = value(for: gamepad.rightShoulder)
        buttons[6] = value(for: gamepad.leftTrigger)
        buttons[7] = value(for: gamepad.rightTrigger)
        buttons[8] = value(for: gamepad.buttonOptions)
        buttons[9] = value(for: gamepad.buttonMenu)
        buttons[10] = value(for: gamepad.leftThumbstickButton)
        buttons[11] = value(for: gamepad.rightThumbstickButton)
        buttons[12] = value(for: gamepad.dpad.up)
        buttons[13] = value(for: gamepad.dpad.down)
        buttons[14] = value(for: gamepad.dpad.left)
        buttons[15] = value(for: gamepad.dpad.right)
        if #available(iOS 14.0, *) {
            buttons[16] = value(for: gamepad.buttonHome)
        }

        let axes: [Double] = [
            clampAxis(gamepad.leftThumbstick.xAxis.value),
            clampAxis(gamepad.leftThumbstick.yAxis.value),
            clampAxis(gamepad.rightThumbstick.xAxis.value),
            clampAxis(gamepad.rightThumbstick.yAxis.value)
        ]

        return [
            "connected": true,
            "id": controller.vendorName ?? "iOS Controller",
            "index": index & 0x03,
            "buttons": buttons,
            "axes": axes
        ]
    }

    private func value(for button: GCControllerButtonInput?) -> Double {
        guard let button else { return 0 }
        return Double(button.value)
    }

    private func clampAxis(_ value: Float) -> Double {
        Double(max(-1, min(1, value)))
    }
}
#else
import SwiftUI

struct StreamerView: View {
    let session: ActiveSession
    let settings: AppSettings
    let onTouchLayoutChange: (String, TouchControlLayout) -> Void
    let onStreamerPreferencesChange: (StreamerPreferences) -> Void
    let onClose: () -> Void
    var onRetry: (() -> Void)? = nil

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()

            VStack(spacing: 18) {
                Image(systemName: "tv.slash")
                    .font(.system(size: 34, weight: .semibold))
                    .foregroundStyle(.orange)

                Text("Streaming Unavailable")
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(.white)

                Text("\(session.game.title) is ready, but Apple TV still needs a native streaming client. The current streamer depends on WebKit and in-page WebRTC, which this target does not ship.")
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.8))
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 420)

                if let retry = onRetry {
                    Button("Retry") {
                        retry()
                    }
                    .buttonStyle(.borderedProminent)
                }

                Button("Close") {
                    onClose()
                }
                .buttonStyle(.bordered)
            }
            .padding(28)
            .background(
                RoundedRectangle(cornerRadius: 20, style: .continuous)
                    .fill(Color.white.opacity(0.08))
                    .overlay(
                        RoundedRectangle(cornerRadius: 20, style: .continuous)
                            .stroke(Color.white.opacity(0.12), lineWidth: 1)
                    )
            )
            .padding(.horizontal, 24)
        }
    }
}
#endif
