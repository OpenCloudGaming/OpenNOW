import SwiftUI
import WebKit

struct StreamerView: View {
    let session: ActiveSession
    let onClose: () -> Void
    @State private var statusText = "Connecting streamer..."

    var body: some View {
        ZStack(alignment: .top) {
            StreamerWebView(session: session) { event in
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
    let onEvent: (String) -> Void

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
        webView.loadHTMLString(buildHTML(for: session), baseURL: nil)
        return webView
    }

    func updateUIView(_ uiView: WKWebView, context: Context) {}

    private func buildHTML(for session: ActiveSession) -> String {
        struct Bridge: Encodable {
            let sessionId: String
            let signalingServer: String
            let signalingUrl: String
            let iceServers: [IceServerConfig]
        }
        let signalingServer = session.signalingServer ?? session.serverIp ?? URL(string: session.streamingBaseUrl)?.host ?? ""
        let signalingUrl = session.signalingUrl ?? "wss://\(signalingServer):443/nvst/"
        let bridge = Bridge(
            sessionId: session.id,
            signalingServer: signalingServer,
            signalingUrl: signalingUrl,
            iceServers: session.iceServers
        )
        let data = (try? JSONEncoder().encode(bridge)) ?? Data("{}".utf8)
        let payload = String(data: data, encoding: .utf8) ?? "{}"
        return """
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
  <script>
  const cfg = \(payload);
  const video = document.getElementById("video");
  const tap = document.getElementById("tap");
  let ws = null;
  let pc = null;
  let ack = 0;
  let hb = null;
  const peerId = 2;
  const peerName = "peer-" + Math.floor(Math.random() * 1e10);

  function post(type, message) {
    try { window.webkit.messageHandlers.opennow.postMessage({ type, message }); } catch (_) {}
  }
  function log(m) { post("log", m); }
  function fail(m) { post("error", m); }
  function nextAck() { ack += 1; return ack; }
  function send(obj) {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    ws.send(JSON.stringify(obj));
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
        resolution: "1920x1080",
        version: 2
      }
    });
  }
  function buildSignInUrl() {
    const base = (cfg.signalingUrl || "").trim() || ("wss://" + cfg.signalingServer + "/nvst/");
    const u = new URL(base);
    u.protocol = "wss:";
    if (!u.pathname.endsWith("/")) u.pathname += "/";
    u.pathname += "sign_in";
    u.search = "";
    u.searchParams.set("peer_id", peerName);
    u.searchParams.set("version", "2");
    return u.toString();
  }
  function ensurePeerConnection() {
    if (pc) return pc;
    const ice = (cfg.iceServers || []).map((s) => ({
      urls: Array.isArray(s.urls) ? s.urls : [s.urls],
      username: s.username || undefined,
      credential: s.credential || undefined
    }));
    pc = new RTCPeerConnection({ iceServers: ice });
    pc.ontrack = (ev) => {
      if (ev.streams && ev.streams[0]) {
        video.srcObject = ev.streams[0];
      } else {
        const ms = new MediaStream();
        ms.addTrack(ev.track);
        video.srcObject = ms;
      }
      video.play().catch(() => {});
      post("status", "Streamer connected");
    };
    pc.onicecandidate = (ev) => {
      if (!ev.candidate) return;
      send({
        peer_msg: {
          from: peerId,
          to: 1,
          msg: JSON.stringify({
            candidate: ev.candidate.candidate,
            sdpMid: ev.candidate.sdpMid,
            sdpMLineIndex: ev.candidate.sdpMLineIndex
          })
        },
        ackid: nextAck()
      });
    };
    pc.onconnectionstatechange = () => {
      post("status", "Peer: " + pc.connectionState);
    };
    return pc;
  }
  async function onOffer(sdp) {
    try {
      const rtc = ensurePeerConnection();
      await rtc.setRemoteDescription({ type: "offer", sdp });
      const answer = await rtc.createAnswer();
      await rtc.setLocalDescription(answer);
      send({
        peer_msg: {
          from: peerId,
          to: 1,
          msg: JSON.stringify({ type: "answer", sdp: answer.sdp || "" })
        },
        ackid: nextAck()
      });
      post("status", "Offer accepted");
    } catch (e) {
      fail("Offer handling failed: " + String(e));
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
    } catch (e) {
      log("Remote ICE add failed: " + String(e));
    }
  }
  function handle(text) {
    let parsed;
    try { parsed = JSON.parse(text); } catch (_) { return; }
    if (parsed.hb) { send({ hb: 1 }); return; }
    if (typeof parsed.ackid === "number") {
      const src = parsed.peer_info && parsed.peer_info.id;
      if (src !== peerId) send({ ack: parsed.ackid });
    }
    if (!parsed.peer_msg || !parsed.peer_msg.msg) return;
    let msg;
    try { msg = JSON.parse(parsed.peer_msg.msg); } catch (_) { return; }
    if (msg.type === "offer" && typeof msg.sdp === "string") {
      onOffer(msg.sdp);
      return;
    }
    if (typeof msg.candidate === "string") {
      onRemoteIce(msg);
    }
  }
  function connect() {
    try {
      const signIn = buildSignInUrl();
      post("status", "Connecting signaling");
      ws = new WebSocket(signIn, "x-nv-sessionid." + cfg.sessionId);
      ws.onopen = () => {
        sendPeerInfo();
        if (hb) clearInterval(hb);
        hb = setInterval(() => send({ hb: 1 }), 5000);
        post("status", "Signaling connected");
      };
      ws.onmessage = (ev) => handle(ev.data);
      ws.onerror = () => fail("Signaling error");
      ws.onclose = (ev) => post("status", "Signaling closed (" + ev.code + ")");
    } catch (e) {
      fail("Signaling setup failed: " + String(e));
    }
  }
  tap.onclick = async () => {
    video.muted = false;
    tap.style.display = "none";
    try { await video.play(); } catch (_) {}
  };
  connect();
  </script>
</body>
</html>
"""
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
