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
        webView.loadHTMLString(buildHTML(for: session, settings: settings), baseURL: nil)
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
  <script>
  const cfg = \#(payload);
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
  function log(message) { post("log", message); }
  function fail(message) { post("error", message); }
  function nextAck() { ack += 1; return ack; }
  function send(obj) {
    if (!ws || ws.readyState !== WebSocket.OPEN) return;
    ws.send(JSON.stringify(obj));
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
    pc.ontrack = (event) => {
      if (event.streams && event.streams[0]) {
        video.srcObject = event.streams[0];
      } else {
        const stream = new MediaStream();
        stream.addTrack(event.track);
        video.srcObject = stream;
      }
      video.play().catch(() => {});
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
      post('status', 'Peer: ' + pc.connectionState);
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
      ws.onclose = (event) => post('status', 'Signaling closed (' + event.code + ')');
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
