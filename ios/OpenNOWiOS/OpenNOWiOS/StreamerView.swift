import SwiftUI
import UIKit
import WebKit

struct StreamerView: View {
    let session: ActiveSession
    let settings: AppSettings
    let onClose: () -> Void
    @State private var statusText = "Connecting streamer..."
    @State private var showReconnect = false
    @State private var webView: WKWebView?

    var body: some View {
        ZStack(alignment: .top) {
            StreamerWebView(
                session: session,
                settings: settings,
                onEvent: { event in
                    statusText = event
                    showReconnect = event.contains("Reconnecting")
                        || event.contains("closed")
                        || event.contains("failed")
                        || event.contains("Error")
                },
                onWebViewReady: { readyWebView in
                    webView = readyWebView
                }
            )
            .ignoresSafeArea()

            HStack {
                Text(statusText)
                    .font(.caption)
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(.regularMaterial, in: Capsule())
                    .lineLimit(1)
                Spacer()
                if showReconnect {
                    Button {
                        webView?.evaluateJavaScript("manualReconnect()")
                        showReconnect = false
                    } label: {
                        Image(systemName: "arrow.clockwise.circle.fill")
                            .font(.title2)
                            .symbolRenderingMode(.hierarchical)
                            .foregroundStyle(.yellow)
                    }
                }
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
    let onWebViewReady: (WKWebView) -> Void

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
        webView.loadHTMLString(buildHTML(for: session, settings: settings), baseURL: nil)
        DispatchQueue.main.async {
            onWebViewReady(webView)
        }
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
            height: profile.height
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
  <div id="touchpad" style="position:fixed;inset:0;z-index:10;touch-action:none;"></div>
  <div id="touchHint" style="position:fixed;left:50%;bottom:60px;transform:translateX(-50%);
    color:rgba(255,255,255,0.45);font:11px -apple-system;pointer-events:none;user-select:none;
    text-align:center;transition:opacity 1s;">Drag to move · Tap to click · 2-finger tap to right-click</div>
	  <button id="gpBtn" style="position:fixed;right:72px;bottom:16px;z-index:20;
	    width:48px;height:48px;border-radius:50%;background:rgba(30,30,30,0.75);color:#fff;
	    border:1px solid rgba(255,255,255,0.25);font-size:22px;cursor:pointer;
	    backdrop-filter:blur(8px);-webkit-backdrop-filter:blur(8px);">🎮</button>
	  <button id="kbBtn" style="position:fixed;right:16px;bottom:16px;z-index:20;
	    width:48px;height:48px;border-radius:50%;background:rgba(30,30,30,0.75);color:#fff;
	    border:1px solid rgba(255,255,255,0.25);font-size:22px;cursor:pointer;
	    backdrop-filter:blur(8px);-webkit-backdrop-filter:blur(8px);">⌨</button>
	  <div id="kbBar" style="display:none;position:fixed;bottom:0;left:0;right:0;z-index:30;
	    background:rgba(20,20,20,0.92);backdrop-filter:blur(12px);-webkit-backdrop-filter:blur(12px);
	    padding:8px 12px;border-top:1px solid rgba(255,255,255,0.1);">
	    <div style="display:flex;gap:8px;align-items:center;">
	      <input id="kbInput" type="text" autocomplete="off" autocorrect="off" autocapitalize="none"
	        inputmode="text" spellcheck="false" placeholder="Type here…" readonly
	        style="flex:1;background:#2a2a2a;color:#fff;border:1px solid rgba(255,255,255,0.2);
	          border-radius:8px;padding:8px 12px;font-size:16px;outline:none;">
	      <button id="kbDoneBtn" style="padding:8px 14px;background:#333;color:#fff;
	        border:none;border-radius:8px;font-size:14px;cursor:pointer;">Done</button>
	    </div>
	  </div>
	  <div id="gamepadOverlay" style="display:none;position:fixed;inset:0;z-index:15;pointer-events:none;">
	    <div id="gpLeftStick" style="position:fixed;left:20px;bottom:110px;width:110px;height:110px;border-radius:50%;
	      background:rgba(255,255,255,0.12);border:1px solid rgba(255,255,255,0.18);pointer-events:all;
	      backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">
	      <div id="gpLeftStickDot" style="position:absolute;left:40px;top:40px;width:30px;height:30px;border-radius:50%;
	        background:rgba(255,255,255,0.28);"></div>
	    </div>
	    <div id="gpRightStick" style="position:fixed;right:20px;bottom:110px;width:110px;height:110px;border-radius:50%;
	      background:rgba(255,255,255,0.12);border:1px solid rgba(255,255,255,0.18);pointer-events:all;
	      backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">
	      <div id="gpRightStickDot" style="position:absolute;left:40px;top:40px;width:30px;height:30px;border-radius:50%;
	        background:rgba(255,255,255,0.28);"></div>
	    </div>
	    <button data-gp-button="12" style="position:fixed;left:44px;bottom:238px;width:42px;height:42px;border:none;border-radius:50%;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:18px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">↑</button>
	    <button data-gp-button="13" style="position:fixed;left:44px;bottom:144px;width:42px;height:42px;border:none;border-radius:50%;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:18px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">↓</button>
	    <button data-gp-button="14" style="position:fixed;left:0;bottom:191px;width:42px;height:42px;border:none;border-radius:50%;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:18px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">←</button>
	    <button data-gp-button="15" style="position:fixed;left:88px;bottom:191px;width:42px;height:42px;border:none;border-radius:50%;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:18px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">→</button>
	    <button data-gp-button="3" style="position:fixed;right:56px;bottom:238px;width:44px;height:44px;border:none;border-radius:50%;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:18px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">Y</button>
	    <button data-gp-button="0" style="position:fixed;right:56px;bottom:144px;width:44px;height:44px;border:none;border-radius:50%;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:18px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">A</button>
	    <button data-gp-button="2" style="position:fixed;right:100px;bottom:191px;width:44px;height:44px;border:none;border-radius:50%;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:18px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">X</button>
	    <button data-gp-button="1" style="position:fixed;right:12px;bottom:191px;width:44px;height:44px;border:none;border-radius:50%;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:18px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">B</button>
	    <button data-gp-button="4" style="position:fixed;left:20px;top:18px;width:92px;height:36px;border:none;border-radius:18px;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:15px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">L</button>
	    <button data-gp-button="5" style="position:fixed;right:20px;top:18px;width:92px;height:36px;border:none;border-radius:18px;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:15px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">R</button>
	    <button data-gp-button="8" style="position:fixed;left:calc(50% - 76px);bottom:184px;width:64px;height:34px;border:none;border-radius:17px;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:13px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">Select</button>
	    <button data-gp-button="9" style="position:fixed;left:calc(50% + 12px);bottom:184px;width:64px;height:34px;border:none;border-radius:17px;
	      background:rgba(255,255,255,0.12);color:#fff;font-size:13px;pointer-events:all;backdrop-filter:blur(4px);-webkit-backdrop-filter:blur(4px);">Start</button>
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
	  let reconnectAttempt = 0;
	  let isReconnecting = false;
	  let lastMouseSentAt = 0;
	  let gamepadActive = false;
	  let prevGamepadState = null;
	  const peerId = 2;
	  const peerName = "peer-" + Math.floor(Math.random() * 1e10);

  function post(type, message) {
    try { window.webkit.messageHandlers.opennow.postMessage({ type, message }); } catch (_) {}
  }
  function log(message) { post("log", message); }
  function fail(message) { post("error", message); }
  function nextAck() { ack += 1; return ack; }
  function send(obj) {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    ws.send(JSON.stringify(obj));
  }
  function sendInput(buf) {
    if (reliableCh && reliableCh.readyState === 'open') reliableCh.send(buf);
  }
  function sendPartialInput(buf) {
    if (partialCh && partialCh.readyState === 'open') partialCh.send(buf);
    else sendInput(buf);
  }
  function buildSignInUrl() {
    const base = (cfg.signalingUrl || "").trim() || ("wss://" + cfg.signalingServer + "/nvst/");
    const url = new URL(base);
    url.protocol = "wss:";
    if (!url.pathname.endsWith("/")) url.pathname += "/";
    url.pathname += "sign_in";
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
	  function encodeGamepad(buttons, leftTrigger, rightTrigger, leftX, leftY, rightX, rightY) {
	    const buf = new ArrayBuffer(32);
	    const v = new DataView(buf);
	    v.setUint32(0, 5, true);
	    v.setUint16(4, buttons, true);
	    v.setUint8(6, leftTrigger);
	    v.setUint8(7, rightTrigger);
	    v.setInt16(8, Math.max(-32767, Math.min(32767, leftX)), true);
	    v.setInt16(10, Math.max(-32767, Math.min(32767, leftY)), true);
	    v.setInt16(12, Math.max(-32767, Math.min(32767, rightX)), true);
	    v.setInt16(14, Math.max(-32767, Math.min(32767, rightY)), true);
	    writeTimestampBE(v, 16);
	    return buf;
	  }
	  const GAMEPAD_BUTTON_MAP = [
	    0x1000, 0x2000, 0x4000, 0x8000,
	    0x0100, 0x0200, 0, 0,
	    0x0020, 0x0010, 0x0040, 0x0080,
	    0x0001, 0x0002, 0x0004, 0x0008
	  ];
	  function normalizeCodec(name) {
	    const upper = String(name || '').toUpperCase().trim();
	    if (upper === 'HEVC' || upper === 'H265') return 'H265';
	    if (upper === 'AV1') return 'AV1';
	    if (upper === 'H264') return 'H264';
	    return 'AUTO';
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
	      if (offerHasCodec(offerSdp, 'AV1')) return 'AV1';
	      if (offerHasCodec(offerSdp, 'H265')) return 'H265';
	      return 'H264';
	    }
	    if (!offerHasCodec(offerSdp, preferred)) {
	      post('status', 'Codec ' + preferred + ' unavailable, using H264');
	      return 'H264';
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
	  function cleanupConnection() {
	    inputReady = false;
	    prevGamepadState = null;
	    if (hbInput) { clearInterval(hbInput); hbInput = null; }
	    if (hb) { clearInterval(hb); hb = null; }
	    if (reliableCh) { try { reliableCh.close(); } catch (_) {} reliableCh = null; }
	    if (partialCh) { try { partialCh.close(); } catch (_) {} partialCh = null; }
	    if (pc) {
	      pc.ontrack = null;
	      pc.onicecandidate = null;
	      pc.onconnectionstatechange = null;
	      pc.ondatachannel = null;
	      try { pc.close(); } catch (_) {}
	      pc = null;
	    }
	    if (ws) {
	      ws.onopen = null;
	      ws.onmessage = null;
	      ws.onerror = null;
	      ws.onclose = null;
	      try { ws.close(1000); } catch (_) {}
	      ws = null;
	    }
	  }
	  function scheduleReconnect() {
	    if (reconnectTimer || isReconnecting) return;
	    const delay = Math.min(2000 * Math.pow(1.5, reconnectAttempt), 20000);
	    reconnectAttempt += 1;
	    post('status', 'Reconnecting in ' + Math.round(delay / 1000) + 's... (attempt ' + reconnectAttempt + ')');
	    reconnectTimer = setTimeout(() => {
	      reconnectTimer = null;
	      isReconnecting = true;
	      cleanupConnection();
	      isReconnecting = false;
	      connect();
	    }, delay);
	  }
	  function manualReconnect() {
	    if (reconnectTimer) { clearTimeout(reconnectTimer); reconnectTimer = null; }
	    reconnectAttempt = 0;
	    cleanupConnection();
	    connect();
	  }
	  function sendCurrentGamepadState() {
	    if (!inputReady) return;
	    const buttons = virtualGamepad.buttons.reduce((mask, pressed, index) => {
	      return pressed && GAMEPAD_BUTTON_MAP[index] ? (mask | GAMEPAD_BUTTON_MAP[index]) : mask;
	    }, 0);
	    const leftTrigger = Math.round(Math.max(0, Math.min(1, virtualGamepad.leftTrigger)) * 255);
	    const rightTrigger = Math.round(Math.max(0, Math.min(1, virtualGamepad.rightTrigger)) * 255);
	    const leftX = Math.round(virtualGamepad.leftX * 32767);
	    const leftY = Math.round(virtualGamepad.leftY * 32767);
	    const rightX = Math.round(virtualGamepad.rightX * 32767);
	    const rightY = Math.round(virtualGamepad.rightY * 32767);
	    sendPartialInput(encodeGamepad(buttons, leftTrigger, rightTrigger, leftX, leftY, rightX, rightY));
	  }
	  function pollGamepad() {
	    if (!inputReady || gamepadActive) return;
	    const gamepads = navigator.getGamepads ? navigator.getGamepads() : [];
	    const gp = Array.from(gamepads).find((candidate) => candidate && candidate.connected);
	    if (!gp) return;
	    let buttons = 0;
	    for (let i = 0; i < Math.min(gp.buttons.length, GAMEPAD_BUTTON_MAP.length); i++) {
	      if (GAMEPAD_BUTTON_MAP[i] && gp.buttons[i].pressed) {
	        buttons |= GAMEPAD_BUTTON_MAP[i];
	      }
	    }
	    const leftTrigger = Math.round((gp.buttons[6]?.value ?? 0) * 255);
	    const rightTrigger = Math.round((gp.buttons[7]?.value ?? 0) * 255);
	    const leftX = Math.round((gp.axes[0] ?? 0) * 32767);
	    const leftY = Math.round((gp.axes[1] ?? 0) * -32767);
	    const rightX = Math.round((gp.axes[2] ?? 0) * 32767);
	    const rightY = Math.round((gp.axes[3] ?? 0) * -32767);
	    const state = buttons + ',' + leftTrigger + ',' + rightTrigger + ',' + leftX + ',' + leftY + ',' + rightX + ',' + rightY;
	    if (state !== prevGamepadState) {
	      prevGamepadState = state;
	      sendPartialInput(encodeGamepad(buttons, leftTrigger, rightTrigger, leftX, leftY, rightX, rightY));
	    }
	  }
	  function gameLoop() {
	    pollGamepad();
	    requestAnimationFrame(gameLoop);
	  }
	  function ensurePeerConnection() {
	    if (pc) return pc;
	    const ice = (cfg.iceServers || []).map((server) => ({
	      urls: Array.isArray(server.urls) ? server.urls : [server.urls],
      username: server.username || undefined,
      credential: server.credential || undefined
    }));
    pc = new RTCPeerConnection({ iceServers: ice });
    reliableCh = pc.createDataChannel('input_channel_v1', { ordered: true });
    reliableCh.binaryType = 'arraybuffer';
    reliableCh.onopen = () => {
      inputReady = true;
      post('status', 'Input ready');
      if (hbInput) clearInterval(hbInput);
      hbInput = setInterval(() => {
        if (inputReady) sendInput(encodeHeartbeat());
      }, 2000);
    };
	    reliableCh.onclose = () => {
	      inputReady = false;
	      prevGamepadState = null;
	      if (hbInput) { clearInterval(hbInput); hbInput = null; }
	    };
	    reliableCh.onmessage = () => {};
	    partialCh = pc.createDataChannel('input_channel_partially_reliable', {
	      ordered: false,
	      maxPacketLifeTime: 50
	    });
	    partialCh.binaryType = 'arraybuffer';
	    pc.ontrack = (event) => {
	      const stream = event.streams?.[0] ?? (() => {
	        const fallbackStream = new MediaStream();
	        fallbackStream.addTrack(event.track);
	        return fallbackStream;
	      })();
	      if (video.srcObject !== stream) {
	        video.srcObject = stream;
	      }
	      video.muted = true;
	      const tryPlay = () => video.play().catch(() => setTimeout(tryPlay, 500));
	      tryPlay();
	      post('status', 'Streamer connected');
	    };
    pc.onicecandidate = (event) => {
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
	    pc.onconnectionstatechange = () => {
	      const state = pc.connectionState;
	      post('status', 'Peer: ' + state);
	      if (state === 'failed') {
	        scheduleReconnect();
	      } else if (state === 'disconnected') {
	        setTimeout(() => {
	          if (pc && (pc.connectionState === 'disconnected' || pc.connectionState === 'failed')) {
	            scheduleReconnect();
	          }
	        }, 4000);
	      } else if (state === 'connected') {
	        reconnectAttempt = 0;
	        if (reconnectTimer) {
	          clearTimeout(reconnectTimer);
	          reconnectTimer = null;
	        }
	      }
	    };
	    return pc;
	  }
  async function onOffer(sdp) {
    try {
      const rtc = ensurePeerConnection();
      const fixedOffer = fixServerIp(sdp, cfg.serverIp || cfg.signalingServer || '');
      const serverIceUfrag = extractIceUfragFromOffer(fixedOffer);
      const selectedCodec = resolvePreferredCodec(fixedOffer);
      const filteredOffer = preferCodec(fixedOffer, selectedCodec);
      await rtc.setRemoteDescription({ type: 'offer', sdp: filteredOffer });
      const answer = await rtc.createAnswer();
      answer.sdp = mungeAnswerSdp(answer.sdp || '', cfg.maxBitrateKbps);
      await rtc.setLocalDescription(answer);
	      const finalSdp = (await waitForIceGathering(rtc, 3000)) || rtc.localDescription?.sdp || answer.sdp || '';
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
	  const kbBtn = document.getElementById('kbBtn');
	  const kbDoneBtn = document.getElementById('kbDoneBtn');
	  const gpBtn = document.getElementById('gpBtn');
	  const gamepadOverlay = document.getElementById('gamepadOverlay');
	  const gpLeftStick = document.getElementById('gpLeftStick');
	  const gpLeftStickDot = document.getElementById('gpLeftStickDot');
	  const gpRightStick = document.getElementById('gpRightStick');
	  const gpRightStickDot = document.getElementById('gpRightStickDot');
	  let kbPrevLen = 0;
	  let lastTX = 0, lastTY = 0;
	  let tStartTime = 0, tMoved = false;
	  let twoFingerStart = 0;
	  const touchpad = document.getElementById('touchpad');
	  const touchHint = document.getElementById('touchHint');
	  const virtualGamepad = {
	    buttons: Array(16).fill(false),
	    leftTrigger: 0,
	    rightTrigger: 0,
	    leftX: 0,
	    leftY: 0,
	    rightX: 0,
	    rightY: 0
	  };

	  function toggleKeyboard() {
	    if (kbBar.style.display === 'none') showKeyboard();
	    else hideKeyboard();
	  }
	  function showKeyboard() {
	    kbBar.style.display = 'block';
	    kbInput.removeAttribute('readonly');
	    kbInput.value = '';
	    kbPrevLen = 0;
	    kbInput.focus();
	    kbInput.click();
	  }
	  function hideKeyboard() {
	    kbBar.style.display = 'none';
	    kbInput.setAttribute('readonly', '');
	    kbInput.blur();
	  }
	  function toggleGamepad() {
	    gamepadActive = !gamepadActive;
	    gamepadOverlay.style.display = gamepadActive ? 'block' : 'none';
	    if (!gamepadActive) resetVirtualGamepad();
	  }
	  function resetVirtualGamepad() {
	    virtualGamepad.buttons.fill(false);
	    virtualGamepad.leftTrigger = 0;
	    virtualGamepad.rightTrigger = 0;
	    virtualGamepad.leftX = 0;
	    virtualGamepad.leftY = 0;
	    virtualGamepad.rightX = 0;
	    virtualGamepad.rightY = 0;
	    prevGamepadState = null;
	    gpLeftStickDot.style.transform = 'translate(0px, 0px)';
	    gpRightStickDot.style.transform = 'translate(0px, 0px)';
	    sendCurrentGamepadState();
	  }
	  function setGamepadButton(index, pressed) {
	    if (virtualGamepad.buttons[index] === pressed) return;
	    virtualGamepad.buttons[index] = pressed;
	    sendCurrentGamepadState();
	  }
	  function bindVirtualButton(button) {
	    if (!button) return;
	    const index = Number(button.dataset.gpButton);
	    button.addEventListener('touchstart', (e) => {
	      e.preventDefault();
	      e.stopPropagation();
	      setGamepadButton(index, true);
	    }, { passive: false });
	    const release = (e) => {
	      e.preventDefault();
	      e.stopPropagation();
	      setGamepadButton(index, false);
	    };
	    button.addEventListener('touchend', release, { passive: false });
	    button.addEventListener('touchcancel', release, { passive: false });
	  }
	  function bindStick(area, dot, xKey, yKey) {
	    if (!area || !dot) return;
	    const radius = 40;
	    let activeTouchId = null;
	    function updateFromTouch(touch) {
	      const rect = area.getBoundingClientRect();
	      const centerX = rect.left + (rect.width / 2);
	      const centerY = rect.top + (rect.height / 2);
	      let dx = touch.clientX - centerX;
	      let dy = touch.clientY - centerY;
	      const distance = Math.hypot(dx, dy);
	      if (distance > radius) {
	        const scale = radius / distance;
	        dx *= scale;
	        dy *= scale;
	      }
	      dot.style.transform = 'translate(' + dx + 'px, ' + dy + 'px)';
	      virtualGamepad[xKey] = Math.max(-1, Math.min(1, dx / radius));
	      virtualGamepad[yKey] = Math.max(-1, Math.min(1, dy / radius));
	      sendCurrentGamepadState();
	    }
	    const release = () => {
	      activeTouchId = null;
	      dot.style.transform = 'translate(0px, 0px)';
	      virtualGamepad[xKey] = 0;
	      virtualGamepad[yKey] = 0;
	      sendCurrentGamepadState();
	    };
	    area.addEventListener('touchstart', (e) => {
	      e.preventDefault();
	      e.stopPropagation();
	      const touch = e.changedTouches[0];
	      activeTouchId = touch.identifier;
	      updateFromTouch(touch);
	    }, { passive: false });
	    area.addEventListener('touchmove', (e) => {
	      e.preventDefault();
	      e.stopPropagation();
	      const touch = Array.from(e.touches).find((item) => item.identifier === activeTouchId);
	      if (touch) updateFromTouch(touch);
	    }, { passive: false });
	    area.addEventListener('touchend', (e) => {
	      const touchEnded = Array.from(e.changedTouches).some((item) => item.identifier === activeTouchId);
	      if (!touchEnded) return;
	      e.preventDefault();
	      e.stopPropagation();
	      release();
	    }, { passive: false });
	    area.addEventListener('touchcancel', (e) => {
	      const touchEnded = Array.from(e.changedTouches).some((item) => item.identifier === activeTouchId);
	      if (!touchEnded) return;
	      e.preventDefault();
	      e.stopPropagation();
	      release();
	    }, { passive: false });
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

	  setTimeout(() => { if (touchHint) touchHint.style.opacity = '0'; }, 4000);

	  kbBtn.addEventListener('touchstart', (e) => {
	    e.preventDefault();
	    e.stopPropagation();
	    toggleKeyboard();
	  }, { passive: false });
	  kbDoneBtn.addEventListener('touchstart', (e) => {
	    e.preventDefault();
	    e.stopPropagation();
	    hideKeyboard();
	  }, { passive: false });
	  gpBtn.addEventListener('touchstart', (e) => {
	    e.preventDefault();
	    e.stopPropagation();
	    toggleGamepad();
	  }, { passive: false });
	  document.querySelectorAll('[data-gp-button]').forEach(bindVirtualButton);
	  bindStick(gpLeftStick, gpLeftStickDot, 'leftX', 'leftY');
	  bindStick(gpRightStick, gpRightStickDot, 'rightX', 'rightY');

	  touchpad.addEventListener('touchstart', (e) => {
	    e.preventDefault();
	    const t = e.touches[0];
	    lastTX = t.clientX;
    lastTY = t.clientY;
    tStartTime = Date.now();
    tMoved = false;
    if (e.touches.length === 2) twoFingerStart = Date.now();
  }, { passive: false });

	  touchpad.addEventListener('touchmove', (e) => {
	    e.preventDefault();
	    if (e.touches.length > 1) return;
	    const t = e.touches[0];
	    const now = performance.now();
	    if (now - lastMouseSentAt < 14) return;
	    lastMouseSentAt = now;
	    const dx = Math.round((t.clientX - lastTX) * 2.0);
	    const dy = Math.round((t.clientY - lastTY) * 2.0);
	    lastTX = t.clientX;
	    lastTY = t.clientY;
	    if ((Math.abs(dx) > 0 || Math.abs(dy) > 0) && inputReady) {
	      tMoved = true;
	      sendPartialInput(encodeMouseMove(dx, dy));
    }
  }, { passive: false });

  touchpad.addEventListener('touchend', (e) => {
    e.preventDefault();
    if (!inputReady) return;
    const holdMs = Date.now() - tStartTime;
    if (!tMoved && holdMs < 500) {
      if (e.changedTouches.length === 1 && e.targetTouches.length === 0) {
        sendInput(encodeMouseButton(8, 1));
        setTimeout(() => sendInput(encodeMouseButton(9, 1)), 60);
      }
    }
    if (e.changedTouches.length === 2 && Date.now() - twoFingerStart < 400) {
      sendInput(encodeMouseButton(8, 3));
      setTimeout(() => sendInput(encodeMouseButton(9, 3)), 60);
    }
  }, { passive: false });

  kbInput.addEventListener('input', (e) => {
    const val = kbInput.value;
    if (val.length > kbPrevLen) {
      const added = val.slice(kbPrevLen);
      for (const ch of added) sendChar(ch);
    } else if (val.length < kbPrevLen) {
      if (inputReady) {
        sendInput(encodeKey(3, 0x08, 0x2A, 0));
        sendInput(encodeKey(4, 0x08, 0x2A, 0));
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
	      const signIn = buildSignInUrl();
	      post('status', 'Connecting signaling');
	      ws = new WebSocket(signIn, 'x-nv-sessionid.' + cfg.sessionId);
      ws.onopen = () => {
        sendPeerInfo();
        if (hb) clearInterval(hb);
        hb = setInterval(() => send({ hb: 1 }), 5000);
        post('status', 'Signaling connected');
	      };
	      ws.onmessage = (event) => handle(event.data);
	      ws.onerror = () => fail('Signaling error');
	      ws.onclose = (event) => {
	        post('status', 'Signaling closed (' + event.code + ')');
	        inputReady = false;
	        if (hbInput) { clearInterval(hbInput); hbInput = null; }
	        reliableCh = null;
	        partialCh = null;
	        if (event.code !== 1000 && event.code !== 1001 && !isReconnecting) {
	          scheduleReconnect();
	        }
	      };
	    } catch (error) {
	      fail('Signaling setup failed: ' + String(error));
	    }
	  }
  tap.onclick = async () => {
    video.muted = false;
    tap.style.display = 'none';
    try { await video.play(); } catch (_) {}
  };
	  connect();
	  requestAnimationFrame(gameLoop);
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
        let windowScene = UIApplication.shared.connectedScenes
            .compactMap { $0 as? UIWindowScene }
            .first
        let screenBounds = windowScene?.screen.nativeBounds ?? UIScreen.main.nativeBounds
        let scale = windowScene?.screen.nativeScale ?? UIScreen.main.nativeScale
        let longSide = max(screenBounds.width, screenBounds.height)
        let shortSide = min(screenBounds.width, screenBounds.height)
        let supports1440 = longSide >= 2500 || shortSide >= 1300 || scale >= 3.0
        if settings.preferredFPS >= 120 && supports1440 {
            return StreamProfile(width: 2560, height: 1440, maxBitrateKbps: 50000)
        }
        return StreamProfile(width: 1920, height: 1080, maxBitrateKbps: 30000)
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
