/// <reference types="node" />

import test from "node:test";
import assert from "node:assert/strict";

import {
  buildNvstSdp,
  fixServerIp,
  mungeAnswerSdp,
  preferCodec,
  rewriteH265LevelIdByProfile,
  rewriteH265TierFlag,
} from "./sdp";

test("fixServerIp replaces 0.0.0.0 connection and candidate IPs from GFN dashed hostnames", () => {
  const sdp = [
    "v=0",
    "c=IN IP4 0.0.0.0",
    "a=candidate:1 1 udp 2130706431 0.0.0.0 47998 typ host",
    "a=candidate:2 1 tcp 1 192.168.1.5 9 typ host",
  ].join("\n");

  const fixed = fixServerIp(sdp, "161-248-11-132.bpc.geforcenow.nvidiagrid.net");

  assert.match(fixed, /c=IN IP4 161\.248\.11\.132/);
  assert.match(fixed, /a=candidate:1 1 udp 2130706431 161\.248\.11\.132 47998 typ host/);
  assert.match(fixed, /a=candidate:2 1 tcp 1 192\.168\.1\.5 9 typ host/);
  assert.equal(fixServerIp(sdp, "unparseable.example.com"), sdp);
});

test("preferCodec keeps selected video payloads and RTX apt payloads while leaving audio untouched", () => {
  const sdp = [
    "v=0",
    "m=audio 9 UDP/TLS/RTP/SAVPF 111 0",
    "a=rtpmap:111 opus/48000/2",
    "a=fmtp:111 minptime=10;useinbandfec=1",
    "m=video 9 UDP/TLS/RTP/SAVPF 96 97 98 99 100 101 102",
    "a=rtpmap:96 H264/90000",
    "a=rtcp-fb:96 nack",
    "a=rtpmap:97 rtx/90000",
    "a=fmtp:97 apt=96",
    "a=rtpmap:98 H265/90000",
    "a=fmtp:98 profile-id=1;tier-flag=0;level-id=153",
    "a=rtcp-fb:98 nack pli",
    "a=rtpmap:99 rtx/90000",
    "a=fmtp:99 apt=98",
    "a=rtpmap:100 AV1/90000",
    "a=rtpmap:101 flexfec-03/90000",
    "a=rtpmap:102 ulpfec/90000",
  ].join("\n");

  const filtered = preferCodec(sdp, "H265");

  assert.match(filtered, /m=video 9 UDP\/TLS\/RTP\/SAVPF 98 99/);
  assert.match(filtered, /a=rtpmap:98 H265\/90000/);
  assert.match(filtered, /a=rtpmap:99 rtx\/90000/);
  assert.match(filtered, /a=fmtp:99 apt=98/);
  assert.doesNotMatch(filtered, /a=rtpmap:96 H264/);
  assert.doesNotMatch(filtered, /a=rtpmap:100 AV1/);
  assert.doesNotMatch(filtered, /flexfec|ulpfec/);
  assert.match(filtered, /m=audio 9 UDP\/TLS\/RTP\/SAVPF 111 0/);
  assert.match(filtered, /a=fmtp:111 minptime=10;useinbandfec=1/);
});

test("rewrites H265 tier flag and clamps oversized level by profile", () => {
  const sdp = [
    "m=video 9 UDP/TLS/RTP/SAVPF 98 99",
    "a=rtpmap:98 H265/90000",
    "a=fmtp:98 profile-id=1;tier-flag=1;level-id=186",
    "a=rtpmap:99 H265/90000",
    "a=fmtp:99 profile-id=2;tier-flag=1;level-id=255",
  ].join("\n");

  const tier = rewriteH265TierFlag(sdp, 0);
  const level = rewriteH265LevelIdByProfile(tier.sdp, { 1: 153, 2: 186 });

  assert.equal(tier.replacements, 2);
  assert.equal(level.replacements, 2);
  assert.match(level.sdp, /a=fmtp:98 profile-id=1;tier-flag=0;level-id=153/);
  assert.match(level.sdp, /a=fmtp:99 profile-id=2;tier-flag=0;level-id=186/);
});

test("mungeAnswerSdp injects bitrate lines and appends opus stereo once", () => {
  const sdp = [
    "m=video 9 UDP/TLS/RTP/SAVPF 98",
    "c=IN IP4 127.0.0.1",
    "m=audio 9 UDP/TLS/RTP/SAVPF 111",
    "a=fmtp:111 minptime=10;useinbandfec=1",
  ].join("\n");

  const munged = mungeAnswerSdp(sdp, 50000);
  assert.match(munged, /m=video.*\nb=AS:50000\n/);
  assert.match(munged, /m=audio.*\nb=AS:128\n/);
  assert.match(munged, /a=fmtp:111 minptime=10;useinbandfec=1;stereo=1/);

  const alreadyStereo = mungeAnswerSdp("m=audio 9 UDP/TLS/RTP/SAVPF 111\nb=AS:128\na=fmtp:111 minptime=10;stereo=1", 50000);
  assert.equal((alreadyStereo.match(/stereo=1/g) ?? []).length, 1);
  assert.equal((alreadyStereo.match(/b=AS:128/g) ?? []).length, 1);
});

test("buildNvstSdp includes stream quality and partially reliable input parameters", () => {
  const sdp = buildNvstSdp({
    width: 2560,
    height: 1440,
    fps: 120,
    maxBitrateKbps: 80000,
    partialReliableThresholdMs: 16,
    codec: "AV1",
    colorQuality: "10bit_444",
    credentials: {
      ufrag: "ufrag-test",
      pwd: "password-test",
      fingerprint: "AA:BB:CC",
    },
    hidDeviceMask: 128,
    enablePartiallyReliableTransferGamepad: 15,
    enablePartiallyReliableTransferHid: 128,
  });

  for (const line of [
    "a=video.clientViewportWd:2560",
    "a=video.clientViewportHt:1440",
    "a=video.maxFPS:120",
    "a=video.initialPeakBitrateKbps:80000",
    "a=vqos.bw.maximumBitrateKbps:80000",
    "a=vqos.bw.peakBitrateKbps:80000",
    "a=video.bitDepth:10",
    "a=vqos.drc.enable:0",
    "a=vqos.dfc.enable:0",
    "a=vqos.resControl.cpmRtc.enable:0",
    "a=ri.partialReliableThresholdMs:16",
    "a=ri.hidDeviceMask:128",
    "a=ri.enablePartiallyReliableTransferGamepad:15",
    "a=ri.enablePartiallyReliableTransferHid:128",
  ]) {
    assert.match(sdp, new RegExp(line.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  }
});
