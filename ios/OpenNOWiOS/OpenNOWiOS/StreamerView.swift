import SwiftUI
import UIKit
import WebKit

struct StreamerView: View {
    let session: ActiveSession
    let settings: AppSettings
    let onClose: () -> Void
    @State private var statusText = "Connecting streamer..."

    var body: some View {
        ZStack(alignment: .top) {
            StreamerWebView(session: session, settings: settings) { event in
                statusText = event
            }
            .ignoresSafeArea()

            HStack {
                Text(statusText)
                    .font(.caption)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(.regularMaterial, in: Capsule())
                    .lineLimit(1)
                Spacer()
                Button {
                    onClose()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.title2)
                        .symbolRenderingMode(.hierarchical)
                }
            }
            .padding()
        }
        .background(Color.black.ignoresSafeArea())
    }
}

private struct StreamerWebView: UIViewRepresentable {
    let session: ActiveSession
    let settings: AppSettings
    let onEvent: (String) -> Void

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
        webView.isOpaque = false
        webView.backgroundColor = .black
        webView.scrollView.isScrollEnabled = false
        webView.loadHTMLString(
            buildHTML(for: session, settings: settings),
            baseURL: URL(string: "https://play.geforcenow.com")
        )
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
    text-align:center;transition:opacity 1s;">Drag to move · Tap to click · 2-finger tap to right-click</div>
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
  const maxReconnectAttempts = 5;
  let offerTimeoutTimer = null;
  let signalingOpenTimeout = null;
  let statsTimer = null;
  let lastBytesReceived = 0;
  let lastBytesTimestamp = 0;
  let pendingMoveDx = 0;
  let pendingMoveDy = 0;
  let moveFrame = null;
  const peerId = 2;
  const peerName = "peer-" + Math.floor(Math.random() * 1e10);
  const statsEl = document.getElementById('stats');

  function post(type, message) {
    try { window.webkit.messageHandlers.opennow.postMessage({ type, message }); } catch (_) {}
  }
  function log(message) { post("log", message); }
  function fail(message) { post("error", message); }
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
    clearOfferTimeout();
    if (signalingOpenTimeout) {
      clearTimeout(signalingOpenTimeout);
      signalingOpenTimeout = null;
    }
    if (hb) { clearInterval(hb); hb = null; }
    if (hbInput) { clearInterval(hbInput); hbInput = null; }
    if (statsTimer) { clearInterval(statsTimer); statsTimer = null; }
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
    }, 9000);
  }
  function isChannelOpen(channel) {
    return !!channel && channel.readyState === 'open';
  }
  function updateInputReady() {
    inputReady = isChannelOpen(reliableCh) || isChannelOpen(partialCh);
    if (inputReady) {
      post('status', 'Input ready');
    }
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
    if (isChannelOpen(partialCh)) {
      partialCh.send(buf);
      return;
    }
    sendInput(buf);
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
    const base = (cfg.signalingUrl || "").trim() || ("wss://" + cfg.signalingServer + "/nvst/");
    const url = new URL(base);
    url.protocol = "wss:";
    // CloudMatch signaling URLs often point at /nvst/, but websocket login is /sign_in.
    url.pathname = "/sign_in";
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
    return buf;
  }
  function encodeMouseMove(dx, dy) {
    const buf = new ArrayBuffer(22);
    const v = new DataView(buf);
    v.setUint32(0, 7, true);
    v.setInt16(4, Math.max(-32768, Math.min(32767, dx)), false);
    v.setInt16(6, Math.max(-32768, Math.min(32767, dy)), false);
    writeTimestampBE(v, 14);
    return buf;
  }
  function encodeMouseButton(type, button) {
    const buf = new ArrayBuffer(18);
    const v = new DataView(buf);
    v.setUint32(0, type, true);
    v.setUint8(4, button);
    writeTimestampBE(v, 10);
    return buf;
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
  function ensurePeerConnection() {
    if (pc) return pc;
    const ice = (cfg.iceServers || []).map((server) => ({
      urls: Array.isArray(server.urls) ? server.urls : [server.urls],
      username: server.username || undefined,
      credential: server.credential || undefined
    }));
    pc = new RTCPeerConnection({ iceServers: ice });
    const thisPc = pc;
    reliableCh = thisPc.createDataChannel('input_channel_v1', { ordered: true });
    reliableCh.binaryType = 'arraybuffer';
    reliableCh.onopen = () => {
      updateInputReady();
      if (hbInput) clearInterval(hbInput);
      hbInput = setInterval(() => {
        if (inputReady) sendInput(encodeHeartbeat());
      }, 2000);
    };
    reliableCh.onclose = () => {
      updateInputReady();
      if (hbInput) { clearInterval(hbInput); hbInput = null; }
    };
    reliableCh.onmessage = () => {};
    partialCh = thisPc.createDataChannel('input_channel_partially_reliable', {
      ordered: false,
      maxPacketLifeTime: 100
    });
    partialCh.binaryType = 'arraybuffer';
    partialCh.onopen = () => updateInputReady();
    partialCh.onclose = () => updateInputReady();
    thisPc.ontrack = (event) => {
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
      const rtc = ensurePeerConnection();
      const fixedOffer = fixServerIp(sdp, cfg.serverIp || cfg.signalingServer || '');
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
        partialReliableThresholdMs: 100,
        hidDeviceMask: 0xFFFFFFFF,
        enablePartiallyReliableTransferGamepad: 0xF,
        enablePartiallyReliableTransferHid: 0xFFFFFFFF,
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
        sdpMid: payload.sdpMid ?? null,
        sdpMLineIndex: payload.sdpMLineIndex ?? null
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
  let tStartTime = 0, tMoved = false, activeTouchId = null;
  let twoFingerStart = 0;
  let twoFingerTapPending = false;
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
    const t = e.touches[0];
    if (!t) return;
    activeTouchId = t.identifier;
    lastTX = t.clientX;
    lastTY = t.clientY;
    tStartTime = Date.now();
    tMoved = false;
    if (e.touches.length === 2) {
      twoFingerStart = Date.now();
      twoFingerTapPending = true;
    } else {
      twoFingerTapPending = false;
    }
  }, { passive: false });

  touchpad.addEventListener('touchmove', (e) => {
    e.preventDefault();
    if (e.touches.length > 1) {
      twoFingerTapPending = false;
      return;
    }
    const t = Array.from(e.touches).find((item) => item.identifier === activeTouchId) || e.touches[0];
    if (!t) return;
    const dx = (t.clientX - lastTX) * 1.6;
    const dy = (t.clientY - lastTY) * 1.6;
    lastTX = t.clientX;
    lastTY = t.clientY;
    if ((Math.abs(dx) > 0 || Math.abs(dy) > 0) && inputReady) {
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
        setTimeout(() => sendInput(encodeMouseButton(9, 1)), 60);
      }
    }
    if (twoFingerTapPending && e.touches.length === 0 && Date.now() - twoFingerStart < 400) {
      sendInput(encodeMouseButton(8, 3));
      setTimeout(() => sendInput(encodeMouseButton(9, 3)), 60);
    }
    activeTouchId = null;
    twoFingerTapPending = false;
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
    if (e.key === 'Enter') {
      e.preventDefault();
      if (inputReady) {
        sendInput(encodeKey(3, 0x0d, 0x28, 0));
        sendInput(encodeKey(4, 0x0d, 0x28, 0));
      }
    }
    if (e.key === 'Escape') hideKeyboard();
  });

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
        fail('Signaling error');
        clearOfferTimeout();
        scheduleReconnect('socket error');
      };
      ws.onclose = (event) => {
        clearOfferTimeout();
        post('status', 'Signaling closed (' + event.code + ')');
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
        if settings.preferredFPS >= 120 && supports1440 {
            return StreamProfile(width: 2560, height: 1440, maxBitrateKbps: 35000)
        }
        return StreamProfile(width: 1920, height: 1080, maxBitrateKbps: 22000)
    }

    final class Coordinator: NSObject, WKScriptMessageHandler {
        private let onEvent: (String) -> Void

        init(onEvent: @escaping (String) -> Void) {
            self.onEvent = onEvent
        }

        func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
            guard let body = message.body as? [String: Any] else { return }
            let type = (body["type"] as? String) ?? "log"
            let msg = (body["message"] as? String) ?? ""
            switch type {
            case "status":
                onEvent(msg)
            case "error":
                onEvent("Error: \(msg)")
            default:
                if !msg.isEmpty {
                    onEvent(msg)
                }
            }
        }
    }
}
