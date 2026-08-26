/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  buildAnnounceSdp,
  extractHmacSeed,
  extractNvstIceCredentials,
  extractNvstSdpAttribute,
  extractMediaControl,
  extractNvstOpusAudioTrack,
  extractRuntimeEncryptionKey,
  generateNvstIceCredentials,
  packSrtpMasterKeySalt,
} from "./sdp";

test("extractNvstIceCredentials reads native and standard SDP forms", () => {
  assert.deepEqual(
    extractNvstIceCredentials(
      "a=x-nv-general.iceUsernameFragment:native-user\r\na=x-nv-general.iceUsernamePwd:native-password\r\n",
    ),
    { usernameFragment: "native-user", password: "native-password" },
  );
  assert.deepEqual(
    extractNvstIceCredentials("a=ice-ufrag:standard-user\na=ice-pwd:standard-password\n"),
    { usernameFragment: "standard-user", password: "standard-password" },
  );
  assert.deepEqual(
    extractNvstIceCredentials(
      "a=x-nv-general.iceUserNameFragmentV2:native-v2-user\r\na=x-nv-general.icePasswordV2:native-v2-password\r\n",
    ),
    { usernameFragment: "native-v2-user", password: "native-v2-password" },
  );
  assert.equal(extractNvstIceCredentials("a=ice-ufrag:incomplete\n"), null);
});

test("extractHmacSeed reads DESCRIBE k= line", () => {
  const seed = extractHmacSeed(
    "v=0\r\nk=HMAC:76A28E94D8C07CB67C04C29CFAAAAF64BE4BA0899456217CB73D070E5060965F\r\na=x-nv-general.rtspWebSocketPerConnection:1\r\n",
  );
  assert.equal(seed, "76A28E94D8C07CB67C04C29CFAAAAF64BE4BA0899456217CB73D070E5060965F");
});

test("extractNvstSdpAttribute reads DESCRIBE transport fields with or without x-nv-", () => {
  assert.equal(
    extractNvstSdpAttribute(
      "a=x-nv-general.serverTransport:192.0.2.20:5004\r\na=x-nv-general.useNewIceInfo:0\r\n",
      "general.serverTransport",
    ),
    "192.0.2.20:5004",
  );
  assert.equal(
    extractNvstSdpAttribute("a=general.useNewIceInfo:0\n", "general.useNewIceInfo"),
    "0",
  );
  assert.equal(extractNvstSdpAttribute("a=general.clientTransport:\n", "general.clientTransport"), null);
});

test("buildAnnounceSdp uses Bifrost session and attribute shape", () => {
  const sdp = buildAnnounceSdp({
    resolution: "1920x1080",
    fps: 60,
    maxBitrateKbps: 100_000,
    videoPort: 5004,
    clientPorts: { video: 45000, audio: 45002, control: 45004 },
  });
  assert.match(sdp, /^o=unknown 0 14 IN IPv4 127\.0\.0\.1$/m);
  assert.match(sdp, /a=x-nv-video\[0\]\.clientViewportWd:1920/);
  assert.match(sdp, /a=x-nv-video\[0\]\.videoSplitEncodeStripsPerFrame:64/);
  assert.match(sdp, /a=x-nv-video\[0\]\.packetSize:1280/);
  assert.match(sdp, /a=x-nv-video\[0\]\.framePacing\.mode:1/);
  assert.match(sdp, /a=x-nv-video\[0\]\.framePacing\.feedbackMode:1/);
  assert.match(sdp, /a=x-nv-video\[0\]\.framePacing\.pid\.minTargetFrameTimeUs:7936/);
  assert.match(sdp, /a=x-nv-video\[0\]\.maxFPS:60/);
  assert.match(sdp, /a=x-nv-general\.clientPorts\.video:45000/);
  assert.match(sdp, /a=x-nv-general\.clientPorts\.audio:45002/);
  assert.match(sdp, /a=x-nv-general\.clientPorts\.control:45004/);
  assert.match(sdp, /a=x-nv-video\[0\]\.enableRtpNack:1/);
  assert.match(sdp, /a=x-nv-vqos\[0\]\.fec\.enable:1/);
  assert.match(sdp, /a=x-nv-vqos\[0\]\.fec\.repairPercent:20/);
  assert.match(sdp, /a=x-nv-vqos\[0\]\.fec\.repairMinPercent:20/);
  assert.match(sdp, /a=x-nv-vqos\[0\]\.fec\.repairMaxPercent:35/);
  assert.match(sdp, /a=x-nv-packetPacing\.minNumPacketsPerGroup:15/);
  assert.match(sdp, /a=x-nv-packetPacing\.maxDelayUs:2000/);
  assert.match(sdp, /a=x-nv-vqos\[0\]\.bitStreamFormat:0/);
  assert.match(sdp, /a=x-nv-vqos\[0\]\.bllFec\.enable:0/);
  assert.match(sdp, /a=x-nv-vqos\[0\]\.grc\.enable:7/);
  assert.match(sdp, /a=x-nv-aqos\.enableRedundancy:0/);
  assert.match(sdp, /a=x-nv-video\[0\]\.initialBitrateKbps:100000/);
  assert.match(sdp, /a=x-nv-video\[0\]\.initialPeakBitrateKbps:100000/);
  assert.match(sdp, /a=x-nv-vqos\[0\]\.bw\.maximumBitrateKbps:100000/);
  assert.match(sdp, /a=x-nv-vqos\[0\]\.bw\.minimumBitrateKbps:1000/);
  assert.match(sdp, /a=x-nv-vqos\[0\]\.dynamicStreamingMode:0/);
  assert.match(sdp, /a=x-nv-bwe\.useOwdCongestionControl:1/);
  assert.match(sdp, /a=x-nv-runtime\.mouseCursorCapture:3/);
  assert.match(sdp, /a=x-nv-runtime\.mimicRemoteCursor:0/);
  assert.doesNotMatch(sdp, /a=x-nv-general\.rtcpOnSctp:/);
  assert.match(sdp, /m=video 5004\r\ni=DeviceString, DeviceName/);
  assert.doesNotMatch(sdp, /RTP\/AVP|msid:video_0|clientTransport|nativeRtcOnBundlePort|iceUsernameFragment|dtlsFingerprint|controlProtocol/);
});

test("buildAnnounceSdp advertises the negotiated high-FPS ceiling", () => {
  const sdp = buildAnnounceSdp({ fps: 360 });
  assert.match(sdp, /a=x-nv-video\[0\]\.maxFPS:240/);
  const normalSdp = buildAnnounceSdp({ fps: 120 });
  assert.match(normalSdp, /a=x-nv-video\[0\]\.maxFPS:120/);
  assert.match(normalSdp, /a=x-nv-packetPacing\.maxDelayUs:4000/);
});

test("buildAnnounceSdp selects H.265 and AV1 NVST bitstreams", () => {
  const h265 = buildAnnounceSdp({ codec: "H265" });
  assert.match(h265, /a=x-nv-vqos\[0\]\.bitStreamFormat:1/);
  assert.match(h265, /a=x-nv-clientSupportHevc:1/);

  const av1 = buildAnnounceSdp({ codec: "AV1" });
  assert.match(av1, /a=x-nv-vqos\[0\]\.bitStreamFormat:2/);
  assert.doesNotMatch(av1, /a=x-nv-clientSupportHevc:/);
});

test("buildAnnounceSdp uses the selected ceiling as the native startup rate", () => {
  const sdp = buildAnnounceSdp({ maxBitrateKbps: 80_000 });
  assert.match(sdp, /a=x-nv-video\[0\]\.initialBitrateKbps:80000/);
  assert.match(sdp, /a=x-nv-video\[0\]\.initialPeakBitrateKbps:80000/);
  assert.match(sdp, /a=x-nv-vqos\[0\]\.bw\.maximumBitrateKbps:80000/);
});

test("buildAnnounceSdp advertises RTCP over SCTP only when negotiated", () => {
  const sdp = buildAnnounceSdp({ rtcpOnSctp: true });
  assert.match(sdp, /a=x-nv-general\.rtcpOnSctp:1/);
});

test("buildAnnounceSdp echoes nativeRtcOnBundlePort when the server advertised it", () => {
  const sdp = buildAnnounceSdp({
    videoPort: 5004,
    nativeRtcOnBundlePort: "1",
    clientPorts: { video: 45000, audio: 45000, control: 45000, bundle: 45000 },
  });
  assert.match(sdp, /a=x-nv-general\.nativeRtcOnBundlePort:1/);
  assert.match(sdp, /a=x-nv-general\.clientPorts\.bundle:45000/);
});

test("buildAnnounceSdp marks rtc streams on the native bundle when unified", () => {
  const sdp = buildAnnounceSdp({
    videoPort: 5004,
    nativeRtcOnBundlePort: "1",
    rtcOnNativeBundle: true,
  });
  assert.match(sdp, /a=x-nv-general\.rtcVideoOnNativeBundle:1/);
  assert.match(sdp, /a=x-nv-general\.rtcAudioOnNativeBundle:1/);
  assert.match(sdp, /a=x-nv-general\.rtcDataChannelOnNativeBundle:1/);
});

test("buildAnnounceSdp matches official cloud bundle flags", () => {
  const fingerprint = "00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF";
  const sdp = buildAnnounceSdp({
    videoPort: 5004,
    iceCredentials: { usernameFragment: "Ab1+", password: "pwd0123456789abcdefABCD" },
    includeNvscLegacyIce: false,
    includeNvscLegacyDtls: false,
    dtlsFingerprint: fingerprint,
    clientPorts: {
      video: 0,
      audio: 0,
      mic: 0,
      control: 0,
      bundle: 0,
      session: 0,
      localAddress: "192.0.2.8",
      useReserved: true,
      fallbackDynamic: true,
    },
    clientBundlePort: 49006,
    nativeRtcOnBundlePort: "1",
    rtcVideoOnNativeBundle: false,
    rtcAudioOnNativeBundle: true,
    rtcMicOnNativeBundle: true,
    rtcDataChannelOnNativeBundle: true,
    enableUnifiedSocket: false,
  });
  assert.match(sdp, /a=x-nv-general\.clientPorts\.video:0/);
  assert.match(sdp, /a=x-nv-general\.clientPorts\.useReserved:1/);
  assert.match(sdp, /a=x-nv-general\.clientPorts\.fallbackDynamic:1/);
  assert.match(sdp, /a=x-nv-general\.clientPorts\.audio:0/);
  assert.match(sdp, /a=x-nv-general\.clientPorts\.mic:0/);
  assert.match(sdp, /a=x-nv-general\.clientPorts\.bundle:0/);
  assert.match(sdp, /a=x-nv-general\.clientBundlePort:49006/);
  assert.match(sdp, /a=x-nv-general\.rtcVideoOnNativeBundle:0/);
  assert.match(sdp, /a=x-nv-general\.rtcAudioOnNativeBundle:1/);
  assert.match(sdp, /a=x-nv-general\.rtcMicOnNativeBundle:1/);
  assert.match(sdp, /a=x-nv-general\.enableUnifiedSocket:0/);
  assert.doesNotMatch(sdp, /a=x-nv-general\.iceUsernameFragment:/);
  assert.doesNotMatch(sdp, /a=x-nv-general\.iceUsernamePwd:/);
  assert.doesNotMatch(sdp, /a=x-nv-general\.dtlsFingerprint:/);
  assert.match(sdp, /a=x-nv-general\.iceUserNameFragmentV2:Ab1\+/);
  assert.match(sdp, /a=x-nv-general\.dtlsFingerprintV2:/);
  assert.match(sdp, /^a=ice-ufrag:Ab1\+$/m);
  assert.match(sdp, /^a=candidate:1 1 udp 2122260223 192\.0\.2\.8 49006 typ host$/m);
  assert.doesNotMatch(sdp, /clientTransport/);
});

test("buildAnnounceSdp includes official clientPorts.localAddress when provided", () => {
  const sdp = buildAnnounceSdp({
    videoPort: 5004,
    clientPorts: { video: 45000, localAddress: "192.0.2.8" },
  });
  assert.match(sdp, /a=x-nv-general\.clientPorts\.localAddress:192\.0\.2\.8/);
  assert.match(sdp, /a=x-nv-general\.clientPorts\.video:45000/);
});

test("buildAnnounceSdp includes clientTransport only when provided", () => {
  const sdp = buildAnnounceSdp({
    videoPort: 5004,
    clientTransport: "192.0.2.8:45000",
  });
  assert.match(sdp, /a=x-nv-general\.clientTransport:192\.0\.2\.8:45000/);
});

test("buildAnnounceSdp includes DTLS fingerprint V1 and V2 when provided", () => {
  const fingerprint = "00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF";
  const sdp = buildAnnounceSdp({ dtlsFingerprint: fingerprint });
  assert.ok(sdp.includes(`a=x-nv-general.dtlsFingerprint:${fingerprint}`));
  assert.ok(sdp.includes(`a=x-nv-general.dtlsFingerprintV2:${fingerprint}`));
});

test("buildAnnounceSdp includes generated ICE V2 credentials when negotiated", () => {
  const credentials = generateNvstIceCredentials();
  assert.match(credentials.usernameFragment, /^[A-Za-z0-9+/]{4}$/);
  assert.match(credentials.password, /^[A-Za-z0-9+/]{22}$/);

  const sdp = buildAnnounceSdp({ iceCredentials: credentials });
  assert.ok(sdp.includes(`a=x-nv-general.iceUsernameFragment:${credentials.usernameFragment}`));
  assert.ok(sdp.includes(`a=x-nv-general.iceUsernamePwd:${credentials.password}`));
  assert.ok(sdp.includes(`a=x-nv-general.iceUserNameFragmentV2:${credentials.usernameFragment}`));
  assert.ok(sdp.includes(`a=x-nv-general.icePasswordV2:${credentials.password}`));
  assert.ok(sdp.includes(`a=ice-ufrag:${credentials.usernameFragment}`));
  assert.ok(sdp.includes(`a=ice-pwd:${credentials.password}`));
});

test("buildAnnounceSdp includes official WebRTC ICE/DTLS and host candidate", () => {
  const fingerprint = "00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99:AA:BB:CC:DD:EE:FF";
  const sdp = buildAnnounceSdp({
    videoPort: 5004,
    iceCredentials: { usernameFragment: "Ab1+", password: "pwd0123456789abcdefABCD" },
    dtlsFingerprint: fingerprint,
    clientPorts: { video: 45000, bundle: 45000, localAddress: "192.0.2.8" },
  });
  assert.match(sdp, /^a=ice-options:trickle$/m);
  assert.match(sdp, /^a=ice-ufrag:Ab1\+$/m);
  assert.match(sdp, /^a=ice-pwd:pwd0123456789abcdefABCD$/m);
  assert.ok(sdp.includes(`a=fingerprint:sha-256 ${fingerprint}`));
  assert.match(sdp, /^a=setup:actpass$/m);
  assert.match(sdp, /^a=candidate:1 1 udp 2122260223 192\.0\.2\.8 45000 typ host$/m);
  assert.match(sdp, /^c=IN IP4 0\.0\.0\.0$/m);
  assert.match(sdp, /^m=video 5004$/m);
});

test("packSrtpMasterKeySalt matches geronimo keyId packing", () => {
  const aes = `${"1C98".padEnd(60, "0")}07D2`;
  const packed = packSrtpMasterKeySalt(aes, 2664076126);
  assert.equal(packed.length, 88);
  assert.equal(packed.slice(0, 64), aes.toUpperCase());
  assert.equal(packed.slice(64), "00000000000000009ECA935E");
});

test("extractRuntimeEncryptionKey reads DESCRIBE attrs", () => {
  const sdp = [
    "v=0",
    "a=x-nv-runtime.encryptionKey:AABBCCDDEEFF00112233445566778899AABBCCDDEEFF00112233445566778899",
    "a=x-nv-runtime.encryptionKeyId:-1630891170",
    "",
  ].join("\r\n");
  const parsed = extractRuntimeEncryptionKey(sdp);
  assert.ok(parsed);
  assert.equal(parsed?.keyId, 2664076126);
});

test("extractMediaControl reads the advertised video track instead of session control", () => {
  const sdp = [
    "v=0",
    "a=control:*",
    "m=audio 0 RTP/AVP 97",
    "a=control:streamid=audio/0/0",
    "m=video 0 RTP/AVP 96",
    "a=control:tracks/server-selected-video",
    "",
  ].join("\r\n");

  assert.equal(extractMediaControl(sdp, "video"), "tracks/server-selected-video");
  assert.equal(extractMediaControl(sdp, "audio"), "streamid=audio/0/0");
  assert.equal(extractMediaControl(sdp, "control"), null);
});

test("extractNvstOpusAudioTrack returns only standard negotiated Opus metadata", () => {
  const sdp = [
    "v=0",
    "m=video 0 RTP/AVP 96",
    "a=rtpmap:96 H264/90000",
    "m=audio 0 RTP/AVP 97 0",
    "a=mid:audio-main",
    "a=rtpmap:97 opus/48000/2",
    "a=rtpmap:0 PCMU/8000/1",
    "a=ssrc:123456 cname:audio",
    "",
  ].join("\r\n");

  assert.deepEqual(extractNvstOpusAudioTrack(sdp), {
    payloadType: 97,
    codec: "opus",
    clockRateHz: 48_000,
    channels: 2,
    mid: "audio-main",
    ssrc: 123456,
  });
});

test("extractNvstOpusAudioTrack does not guess omitted or unsupported codec metadata", () => {
  assert.equal(
    extractNvstOpusAudioTrack("m=audio 0 RTP/AVP 97\r\na=control:audio\r\n"),
    null,
  );
  assert.equal(
    extractNvstOpusAudioTrack("m=audio 0 RTP/AVP 97\r\na=rtpmap:97 proprietary/48000/2\r\n"),
    null,
  );
});
