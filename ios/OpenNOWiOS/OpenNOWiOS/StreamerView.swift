import SwiftUI
import UIKit
import WebKit
import OSLog
import AVFoundation

struct StreamerView: View {
    let session: ActiveSession
    let settings: AppSettings
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
            StreamerWebView(session: session, settings: settings) { event in
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
                    let isFatal = event.localizedCaseInsensitiveContains("reconnect exhausted")
                        || event.localizedCaseInsensitiveContains("restart limit reached")
                    if isFatal, let retry = onRetry {
                        DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) {
                            retry()
                        }
                    }
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
        Coordinator(onEvent: onEvent)
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
        webView.loadHTMLString(html, baseURL: baseURL)
        return webView
    }

    func updateUIView(_ uiView: WKWebView, context: Context) {}

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
        }

        let signalingServer = session.signalingServer ?? session.serverIp ?? URL(string: session.streamingBaseUrl)?.host ?? ""
        let signalingUrl = session.signalingUrl ?? "wss://\(signalingServer):443/nvst/"
        let serverIp = session.serverIp ?? signalingServer
        let profile = Self.streamProfile(for: settings)
        let bridge = Bridge(
            sessionId: session.id,
            signalingServer: signalingServer,
            signalingUrl: signalingUrl,
            iceServers: session.iceServers,
            serverIp: serverIp,
            mediaIp: session.mediaIp,
            mediaPort: session.mediaPort,
            preferredCodec: Self.normalizePreferredCodec(settings.preferredCodec),
            fps: settings.preferredFPS,
            maxBitrateKbps: profile.maxBitrateKbps,
            width: profile.width,
            height: profile.height,
            showStatsOverlay: settings.showStatsOverlay
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
    #tap{position:fixed;left:50%;bottom:20px;transform:translateX(-50%);padding:8px 12px;
      color:#fff;background:rgba(0,0,0,.5);border-radius:999px;font:12px -apple-system;}
  </style>
</head>
<body>
  <video id="video" playsinline autoplay muted></video>
  <div id="tap">Tap to unmute</div>
  <div id="stats" style="position:fixed;left:12px;top:12px;z-index:30;padding:6px 10px;
    color:#d5ffd5;background:rgba(0,0,0,0.58);border:1px solid rgba(255,255,255,0.15);
    border-radius:10px;font:12px -apple-system;backdrop-filter:blur(8px);-webkit-backdrop-filter:blur(8px);">
    FPS -- | Ping -- ms | -- Mbps
  </div>
  <div id="touchpad" style="position:fixed;inset:0;z-index:10;touch-action:none;"></div>
  <div id="touchHint" style="position:fixed;left:50%;bottom:60px;transform:translateX(-50%);
    color:rgba(255,255,255,0.45);font:11px -apple-system;pointer-events:none;user-select:none;
    text-align:center;transition:opacity 1s;">Drag to move · Tap to click · Pinch to zoom · 2-finger pan when zoomed</div>
  <button id="kbBtn" onclick="toggleKeyboard()" style="position:fixed;right:16px;bottom:16px;z-index:20;
    width:48px;height:48px;border-radius:50%;background:rgba(30,30,30,0.75);color:#fff;
    border:1px solid rgba(255,255,255,0.25);font-size:22px;cursor:pointer;
    backdrop-filter:blur(8px);-webkit-backdrop-filter:blur(8px);">⌨</button>
  <button id="gpBtn" onclick="toggleGamepad()" style="position:fixed;right:16px;bottom:72px;z-index:20;
    width:48px;height:48px;border-radius:50%;background:rgba(30,30,30,0.75);color:#fff;
    border:1px solid rgba(255,255,255,0.25);font-size:22px;cursor:pointer;
    backdrop-filter:blur(8px);-webkit-backdrop-filter:blur(8px);">🎮</button>
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
  <div id="gpPad" style="display:none;position:fixed;left:0;right:0;bottom:12px;z-index:25;pointer-events:none;">
    <div style="display:flex;justify-content:space-between;gap:16px;padding:0 12px;">
      <div style="display:grid;grid-template-columns:56px 56px 56px;grid-template-rows:56px 56px 56px;gap:6px;pointer-events:auto;">
        <button data-key="w" style="grid-column:2;grid-row:1;" class="gpKey">▲</button>
        <button data-key="a" style="grid-column:1;grid-row:2;" class="gpKey">◀</button>
        <button data-key="s" style="grid-column:2;grid-row:3;" class="gpKey">▼</button>
        <button data-key="d" style="grid-column:3;grid-row:2;" class="gpKey">▶</button>
      </div>
      <div style="display:grid;grid-template-columns:56px 56px;grid-template-rows:56px 56px;gap:8px;pointer-events:auto;">
        <button data-key="j" class="gpKey">X</button>
        <button data-key="l" class="gpKey">Y</button>
        <button data-key="k" class="gpKey">A</button>
        <button data-key="i" class="gpKey">B</button>
      </div>
    </div>
    <div style="display:flex;justify-content:center;margin-top:10px;pointer-events:auto;">
      <button id="gpHide" style="padding:8px 12px;border-radius:999px;background:rgba(20,20,20,0.82);
        color:#fff;border:1px solid rgba(255,255,255,0.25);font-size:12px;">Hide gamepad</button>
    </div>
  </div>
  <script>
  const cfg = \#(payload);
  const video = document.getElementById("video");
  const tap = document.getElementById("tap");
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

  function post(type, message) {
    try { window.webkit.messageHandlers.opennow.postMessage({ type, message }); } catch (_) {}
  }
  function log(message) { post("log", message); }
  function fail(message) { post("error", message); }
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
    stopKeyframeTimer();
    lastBytesReceived = 0;
    lastBytesTimestamp = 0;
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
      fail('Offer timeout, retrying signaling');
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
  function updateStatsOverlay(fps, pingMs, bitrateMbps) {
    if (!statsEl) return;
    statsEl.textContent = `FPS ${fps > 0 ? Math.round(fps) : '--'} | Ping ${pingMs > 0 ? Math.round(pingMs) : '--'} ms | ${bitrateMbps > 0 ? bitrateMbps.toFixed(1) : '--'} Mbps`;
  }
  async function samplePeerStats() {
    if (!pc || !statsEl || !cfg.showStatsOverlay) return;
    try {
      const report = await pc.getStats();
      let fps = 0;
      let pingMs = 0;
      let bitrateMbps = 0;
      report.forEach((stat) => {
        if (stat.type === 'inbound-rtp' && stat.kind === 'video') {
          if (typeof stat.framesPerSecond === 'number' && stat.framesPerSecond > 0) {
            fps = stat.framesPerSecond;
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
      updateStatsOverlay(fps, pingMs, bitrateMbps);
    } catch (_) {}
  }
  function ensureStatsTicker() {
    if (!cfg.showStatsOverlay || statsTimer) return;
    statsTimer = setInterval(samplePeerStats, 1000);
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
      return offerHasCodec(offerSdp, 'H265') ? 'H265' : 'H264';
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
      if (kind === 'video') {
        if (event.streams && event.streams[0]) {
          video.srcObject = event.streams[0];
        } else {
          const stream = new MediaStream();
          stream.addTrack(event.track);
          video.srcObject = stream;
        }
        video.play().catch(() => {});
        post('status', 'Streamer connected');
        ensureStatsTicker();
        startKeyframeTimer();
      } else if (kind === 'audio') {
        if (event.streams && event.streams[0] && video.srcObject) {
          for (const t of event.streams[0].getAudioTracks()) {
            video.srcObject.addTrack(t);
          }
        }
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
        if (ws && ws.readyState === WebSocket.OPEN) {
          scheduleReconnect('peer disconnected');
        }
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
  let kbPrevLen = 0;
  let lastTX = 0, lastTY = 0;
  let tapStartX = 0, tapStartY = 0;
  let tStartTime = 0, tMoved = false, activeTouchId = null;
  let twoFingerStart = 0;
  let twoFingerTapPending = false;
  const MOVE_CLICK_CANCEL_PX = 8;
  const pointerSpeed = 1.2;
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
  const touchpad = document.getElementById('touchpad');
  const touchHint = document.getElementById('touchHint');
  if (!cfg.showStatsOverlay && statsEl) {
    statsEl.style.display = 'none';
  }
  function setGamepadVisible(visible) {
    gpPad.style.display = visible ? 'block' : 'none';
    if (gpBtn) {
      gpBtn.style.opacity = visible ? '0.75' : '1';
      gpBtn.textContent = visible ? '🙈' : '🎮';
      gpBtn.title = visible ? 'Hide gamepad' : 'Show gamepad';
    }
  }

  function toggleKeyboard() {
    if (kbBar.style.display === 'none') showKeyboard();
    else hideKeyboard();
  }
  function showKeyboard() {
    kbBar.style.display = 'block';
    kbInput.value = '';
    kbPrevLen = 0;
    setTimeout(() => kbInput.focus(), 80);
  }
  function hideKeyboard() {
    kbBar.style.display = 'none';
    kbInput.blur();
  }
  function toggleGamepad() {
    setGamepadVisible(gpPad.style.display === 'none');
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
    const dx = Math.round(pendingMoveDx);
    const dy = Math.round(pendingMoveDy);
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
      const key = btn.getAttribute('data-key');
      const down = (e) => {
        e.preventDefault();
        btn.style.transform = 'scale(0.95)';
        sendVirtualKey(key, true);
      };
      const up = (e) => {
        e.preventDefault();
        btn.style.transform = 'scale(1)';
        sendVirtualKey(key, false);
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
  }

  setTimeout(() => { if (touchHint) touchHint.style.opacity = '0'; }, 4000);

  touchpad.addEventListener('touchstart', (e) => {
    e.preventDefault();
    if (e.touches.length === 2) {
      const t1 = e.touches[0];
      const t2 = e.touches[1];
      const center = pinchCenter(t1, t2);
      pinchStartDistance = pinchDistance(t1, t2);
      pinchStartScale = zoomScale;
      pinchCenterStartX = center.x;
      pinchCenterStartY = center.y;
      pinchTranslateStartX = zoomTx;
      pinchTranslateStartY = zoomTy;
      pinchGestureMoved = false;
      twoFingerStart = Date.now();
      twoFingerTapPending = true;
      activeTouchId = null;
      return;
    }
    const t = e.touches[0];
    if (!t) return;
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
    e.preventDefault();
    if (e.touches.length === 2) {
      const t1 = e.touches[0];
      const t2 = e.touches[1];
      const distance = pinchDistance(t1, t2);
      const center = pinchCenter(t1, t2);
      if (pinchStartDistance > 0) {
        const nextScale = clamp((distance / pinchStartDistance) * pinchStartScale, ZOOM_MIN, ZOOM_MAX);
        if (Math.abs(nextScale - zoomScale) > 0.01) {
          pinchGestureMoved = true;
        }
        zoomScale = nextScale;
      }
      if (zoomScale > 1.01) {
        zoomTx = pinchTranslateStartX + (center.x - pinchCenterStartX);
        zoomTy = pinchTranslateStartY + (center.y - pinchCenterStartY);
        if (Math.abs(center.x - pinchCenterStartX) > 2 || Math.abs(center.y - pinchCenterStartY) > 2) {
          pinchGestureMoved = true;
        }
      }
      if (pinchGestureMoved) {
        twoFingerTapPending = false;
      }
      applyVideoTransform();
      return;
    }
    const t = Array.from(e.touches).find((item) => item.identifier === activeTouchId) || e.touches[0];
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
    e.preventDefault();
    if (e.touches.length === 0 && zoomScale > 1.01 && pinchGestureMoved) {
      pinchGestureMoved = false;
      activeTouchId = null;
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
      if (e.changedTouches.length === 1 && e.targetTouches.length === 0) {
        sendInput(encodeMouseButton(8, 1));
        setTimeout(() => sendInput(encodeMouseButton(9, 1)), 35);
      }
    }
    if (twoFingerTapPending && zoomScale <= 1.01 && e.touches.length === 0 && Date.now() - twoFingerStart < 400) {
      sendInput(encodeMouseButton(8, 3));
      setTimeout(() => sendInput(encodeMouseButton(9, 3)), 35);
    }
    activeTouchId = null;
    twoFingerTapPending = false;
  }, { passive: false });

  touchpad.addEventListener('dblclick', (e) => {
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
          fail('Signaling connect timeout');
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
        fail('Signaling error');
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
  if (gpHide) {
    gpHide.onclick = () => setGamepadVisible(false);
  }
  setGamepadVisible(false);
  tap.onclick = async () => {
    video.muted = false;
    tap.style.display = 'none';
    try { await video.play(); } catch (_) {}
  };
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
        switch codec.uppercased() {
        case "HEVC", "H265":
            return "H265"
        case "AV1":
            return "AV1"
        case "H264":
            return "H264"
        default:
            return "Auto"
        }
    }

    private static func streamProfile(for settings: AppSettings) -> StreamProfile {
        let nativeBounds = UIScreen.main.nativeBounds
        let longSide = max(nativeBounds.width, nativeBounds.height)
        let shortSide = min(nativeBounds.width, nativeBounds.height)
        let supports1440 = longSide >= 2500 || shortSide >= 1400 || UIScreen.main.nativeScale >= 3.0

        // Pick bitrate based on quality preference to avoid overwhelming mobile links.
        // "Data Saver" targets lower network usage; "Quality" allows higher bitrate;
        // "Balanced" stays in the middle.
        let quality = settings.preferredQuality.lowercased()
        let bitrateFor1080: Int = quality == "quality" ? 75_000 : (quality == "data saver" ? 25_000 : 50_000)
        let bitrateFor1440: Int = quality == "quality" ? 100_000 : (quality == "data saver" ? 35_000 : 65_000)

        // Limit to 1440p only when explicitly requesting high FPS AND device supports it.
        // Keep at 1080p for 60 fps or lower to reduce decode load and lag.
        if settings.preferredFPS >= 120 && supports1440 {
            return StreamProfile(width: 2560, height: 1440, maxBitrateKbps: bitrateFor1440)
        }
        return StreamProfile(width: 1920, height: 1080, maxBitrateKbps: bitrateFor1080)
    }

    final class Coordinator: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
        private let onEvent: (String) -> Void
        var cachedHTML: String = ""
        var cachedBaseURL: URL?
        private var contentProcessRestartCount = 0
        private static let maxContentProcessRestarts = 5

        init(onEvent: @escaping (String) -> Void) {
            self.onEvent = onEvent
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
                if msg.localizedCaseInsensitiveContains("error")
                    || msg.localizedCaseInsensitiveContains("reconnect")
                    || msg.localizedCaseInsensitiveContains("timeout")
                {
                    onEvent("Error: \(msg)")
                }
            case "error":
                onEvent("Error: \(msg)")
            case "log":
                onEvent("Log: \(msg)")
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
