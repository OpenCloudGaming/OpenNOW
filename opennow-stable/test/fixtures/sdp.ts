export function joinSdpLines(lines: string[], lineEnding: "\n" | "\r\n" = "\r\n"): string {
  return `${lines.join(lineEnding)}${lineEnding}`;
}

export function buildOfferSdp(lineEnding: "\n" | "\r\n" = "\r\n"): string {
  return joinSdpLines(
    [
      "v=0",
      "o=- 0 0 IN IP4 127.0.0.1",
      "s=-",
      "t=0 0",
      "a=ice-ufrag:offerUfrag",
      "a=ice-pwd:offerPwd",
      "a=fingerprint:sha-256 AA:BB:CC:DD",
      "m=audio 9 UDP/TLS/RTP/SAVPF 111",
      "c=IN IP4 0.0.0.0",
      "a=rtpmap:111 opus/48000/2",
      "a=fmtp:111 minptime=10;useinbandfec=1;tier-flag=1;profile-id=1;level-id=93",
      "a=candidate:1 1 udp 2113937151 0.0.0.0 54400 typ host",
      "m=video 9 UDP/TLS/RTP/SAVPF 96 98 100",
      "c=IN IP4 0.0.0.0",
      "a=rtpmap:96 H264/90000",
      "a=fmtp:96 profile-level-id=42e01f;tier-flag=1",
      "a=rtpmap:98 H265/90000",
      "a=fmtp:98 profile-id=1;level-id=120;tier-flag=1",
      "a=rtpmap:100 HEVC/90000",
      "a=fmtp:100 profile-id=2;level-id=150;tier-flag=1",
      "a=candidate:2 1 udp 2113937151 0.0.0.0 54402 typ host",
      "m=application 9 UDP/DTLS/SCTP webrtc-datachannel",
      "c=IN IP4 0.0.0.0",
    ],
    lineEnding,
  );
}
