# RTSP exchange and x-nv SDP attributes, as logged by the Windows client (S6)

Source: Bifrost `[NVST:RtspSessionPoco*]`, `[NVST:SignalingHandler]`, `[NVST:NvscSerialize]`, and `[NVST:NvscClientConfig]` lines in `geronimo.log.bak` (2026-10-12 19:45:29 to 19:45:31 local). There is one handshake in the upload. Redacted intermediates: `extracted/rtsp_exchange.txt`, `extracted/sdp_server_xnv.tsv` (server attributes), `extracted/nvsc_config.tsv` (final config, 975 keys), and `extracted/xnv_compare.tsv` (OpenNOW comparison). Tags follow COMMON.md. Rows are **L** unless they say otherwise.

## What the log does and does not contain

- **Server DESCRIBE SDP: partial.** The DESCRIBE body is `Content-Length: 219292`, and Bifrost reports `Successfully read 4145 NvscClientConfig attributes from SDP`. Bifrost prints only a `new features:` block: 62 `a=x-nv-*` lines plus the SDP header lines `v=`, `o=NvStreamer <unix-time> 14 IN IPv4 <redacted-ip>...`, `s=NVIDIA Streaming Session`, `k=HMAC:<redacted>`, `t=0 0`, and `m=video 0 RTP/AVP`. It then logs `Successfully read 62 NvscClientConfig attributes from SDP`. **I**: the 62 lines are the attributes this server overrides relative to a baseline, and the other 4083 are not visible.
- **Client ANNOUNCE SDP: not printed.** Bifrost only logs `Successfully appended 163 NvscClientConfig attributes to SDP` and that it skipped `x-nv-general.iceUsernameFragment`, `x-nv-general.iceUsernamePwd`, and `x-nv-general.dtlsFingerprint`. The closest available proxy is the post-ANNOUNCE `NvscClientConfig` dump (975 keys, 19:45:31.258-.265). That dump is the merged config, not the wire set. **U**: which 163 keys went on the wire.
- **Request headers: not printed.** Bifrost logs the request target, for example `WSS Setup: streamid=video/0/0, transport ` (empty client transport header), and the full **response** headers.

## RTSP over WSS sequence (signaling `Using Secure WebSockets for Signaling`)

Signaling connects to `wss` on `<redacted-ip>.cloudmatchbeta.nvidiagrid.net:322`. Bifrost logs `RTSP session requested scheme 'RTSPS' that does not match the expected scheme 'WSS'` and `Establishing RTSP session with protocol TAG 'WSS' and URL 'rtsps://...:322'`. It uses `WSS client using 1-way SSL` and client `Header version: 14.2`. The config holds `general.rtspScheme 3` and server `rtspWebSocketPerConnection:1`.

| # | request | request target | latency | response status line | response headers (values redacted where secret) |
|---|---|---|---|---|---|
| 1 | OPTIONS | `rtsps://<host>:322` | 7 ms | `HTTP/1.0 200 OK` | `Request-Id: 1`, `CSeq: 1`, `X-GS-Version: 14.2`, `Public: OPTIONS,DESCRIBE,ANNOUNCE,SETUP,TEARDOWN,PLAY,PAUSE,X_NV_COMMAND,X_NV_EVENT` |
| 2 | DESCRIBE | `rtsps://<host>:322` | 70 ms | `HTTP/1.0 200 OK` | `Request-Id: 2`, `CSeq: 2`, `X-GS-Version: 14.2`, `Content-Type: application/sdp`, `Session: XNV<redacted>`, `Content-Length: 219292` |
| 3 | SETUP | `streamid=video/0/0`, client transport header empty | 173 ms | `HTTP/1.0 200 OK` | `Request-Id: 3`, `CSeq: 3`, `X-GS-Version: 14.2`, `Transport: unicast;X-GS-ServerPort=5004-5005;source=<redacted-ip>`, `Session: XNV<redacted>`, `X-Nv-Ping-Payload: <redacted>`, `x-nv-ping: 6` |
| 4 | ANNOUNCE | `rtsps://<host>:322` | 381 ms | `HTTP/1.0 200 OK` | `Request-Id: 4`, `CSeq: 4`, `X-GS-Version: 14.2` |
| 5 | PLAY | session `<redacted>` | 114 ms | `HTTP/1.0 200 OK` | `Request-Id: 5`, `CSeq: 5`, `X-GS-Version: 14.2` |

Notes:

- The status line over WSS is `HTTP/1.0 200 OK`, not `RTSP/1.0`. OpenNOW's parser ignores the protocol token (`nvst_rtsp.rs` ~1711-1716), so this is compatible.
- `Request-Id` and `CSeq` move together. OpenNOW sends both (`nvst_rtsp.rs:572`) and checks both (`:1756`).
- Only one SETUP is sent, for video. Audio and mic SETUPs are skipped: `Bypassing 'Mjolnir' SETUP for the Audio stream. Deferring to WebRtcTransport setup`. The control stream is not SETUP either. It is selected as `Final Selected ControlProtocol: udp_ag, controlControlId: streamid=control/10`.
- The SETUP response Transport gives server port 5004. Both the video socket and the bundle (DTLS/SCTP/audio) socket then target `<ip>:5004`. `general.serverEndpoints` lists other port numbers (`control 47995 UDP control 47999 audio 48000 input 47995 bundle 48001 stream 0: video stream 47998`), and so does `general.serverBundlePort 47999`. **I**: these are the seat's internal ports behind a NAT or port-mapping front, so they must not be used as peers.
- Protocols the client advertises after DESCRIBE: `udp`, `udp_ag`, `tcp`, `udp_enc`, `udp_ag_enc`, `tcp_enc`, `channel_encrypt`. It picked `udp_ag (2) - not encrypted`.
- Negotiated version minima: `Blob stats version server: [9], client: [9], final: [9]`, `qosFeedbackVersion 7`, `qosTimingsVersion 5`, and `Parser server RTP extension header version 2`.

## Server DESCRIBE x-nv attributes (all 62 printed lines)

Side: all are server -> client. Generated from `extracted/sdp_server_xnv.tsv`.

<!-- GEN:sdp_server -->
**SDP header lines** (6)

| key | value |
|---|---|
| `v=0` |  |
| `o=NvStreamer 1791827130 14 IN IPv4 <redacted-ip>.cloudmatchbeta.nvidiagrid.net` |  |
| `s=NVIDIA Streaming Session` |  |
| `k=HMAC:<redacted>` |  |
| `t=0 0` |  |
| `m=video 0 RTP/AVP` |  |

**general** (14)

| key | value |
|---|---|
| `general.enetControlChannel.mtuSize` | 1191 |
| `general.customMessageOnCC` | 1 |
| `general.rtspWebSocketPerConnection` | 1 |
| `general.rtcAudioOnNativeBundle` | 1 |
| `general.rtcMicOnNativeBundle` | 1 |
| `general.nativeRtcOnBundlePort` | 1 |
| `general.maxQosMessagesSize` | 1071 |
| `general.pingIntervalBeforeConnectionMs` | 20 |
| `general.pingIntervalAfterConnectionMs` | 100 |
| `general.wifiInfoInterval` | 15 |
| `general.enableUDSWifiCongestion` | 1 |
| `general.enableUDSWifiStandard` | 1 |
| `general.transitSelectionSettings.enableDynamicTransitSelection` | 1 |
| `general.transitSelectionSettings.selectionEnforcementMethod` | 1 |

**video[0]** (12)

| key | value |
|---|---|
| `video[0].vbvMultiplier` | 109 |
| `video[0].videoSplitEncodeStripsPerFrame` | 64 |
| `video[0].enableAv1RcPrecisionFactor` | 1 |
| `video[0].adaptiveQuantization.spatialAQSetting` | 7 |
| `video[0].adaptiveQuantization.temporalAQSetting` | 0 |
| `video[0].adaptiveQuantization.spatialAQStrength` | 12 |
| `video[0].adaptiveQuantization.qpThresholdAdjPercent` | 2 |
| `video[0].adaptiveQuantization.saqAdaptMinQpThresholdPercent` | 40 |
| `video[0].adaptiveQuantization.saqAdaptMaxQpThresholdPercent` | 100 |
| `video[0].adaptiveQuantization.saqAdaptDecayStrengthX100` | 250 |
| `video[0].adaptiveQuantization.perfAdjEnablement` | 1 |
| `video[0].framePacing.pid.minTargetFrameTimeUs` | 7936 |

**vqos[0]** (24)

| key | value |
|---|---|
| `vqos[0].grc.enable` | 0 |
| `vqos[0].qec.featureSetting` | 3 |
| `vqos[0].qec.packetLossConcealmentSetting` | 2 |
| `vqos[0].calculateAvgVideoStreamingBitrate` | 1 |
| `vqos[0].statsProcessorThread.flags` | 1023 |
| `vqos[0].sendEndOfSessionQosTelemetry` | 1 |
| `vqos[0].relaxMaxBitrate.overrideAvgBitrateThresholdPercent` | 4 |
| `vqos[0].relaxMaxBitrate.customAvgBitrateThresholdPercent` | 65 |
| `vqos[0].relaxMaxBitrate.overrideAvgQpThresholdPercent` | 7 |
| `vqos[0].relaxMaxBitrate.customAvgQpThresholdPercent` | 51 |
| `vqos[0].relaxMaxBitrate.iirFilterFactor` | 120 |
| `vqos[0].qpDelta.qpDeltaMaxPercent` | 10 |
| `vqos[0].qpDelta.qpDeltaSurfaceAdjustmentStrengthPercent` | 70 |
| `vqos[0].qpDelta.qpDeltaVbvUsageFactorPercentH264` | 100 |
| `vqos[0].qpDelta.qpDeltaVbvUsageFactorPercentH265` | 100 |
| `vqos[0].qpDelta.qpDeltaVbvUsageFactorPercentAv1` | 100 |
| `vqos[0].qpDelta.qpDeltaMinPercent` | 60 |
| `vqos[0].qpDelta.qpDeltaIirFactor` | 60 |
| `vqos[0].qpDelta.qpDeltaThrottlePercent` | 100 |
| `vqos[0].encoderUtilizationBooster.featureSetting` | 3 |
| `vqos[0].encoderUtilizationBooster.iirFilterFactor` | 30 |
| `vqos[0].encoderUtilizationBooster.underUtilizationUpperLimitThresholdPercent` | 95 |
| `vqos[0].encoderUtilizationBooster.maxAllowedUtilizationPercent` | 100 |
| `vqos[0].encoderUtilizationBooster.bitrateMultiplierUpperLimitPercent` | 15 |

**aqos** (4)

| key | value |
|---|---|
| `aqos.enableRedundancy` | 1 |
| `aqos.redundancyLevel` | 2 |
| `aqos.enableRedundancyForMic` | 1 |
| `aqos.redundancyLevelForMic` | 3 |

**runtime** (3)

| key | value |
|---|---|
| `runtime.maxVerboseEtlSizeMb` | 60 |
| `runtime.audioSrtp` | 0 |
| `runtime.micSrtp` | 0 |

**ri** (2)

| key | value |
|---|---|
| `ri.hidDeviceMask` | 4 |
| `ri.partialReliableThresholdMs` | 300 |

**audio** (2)

| key | value |
|---|---|
| `audio.enableDynamicAudioConfig` | 1 |
| `audio.enableTimestampAudioBuffer` | 1 |

**packetPacing** (1)

| key | value |
|---|---|
| `packetPacing.enableAccurateSleep` | 1 |
<!-- /GEN:sdp_server -->

## Final NvscClientConfig after ANNOUNCE (client-side merged config)

`extracted/nvsc_config.tsv` holds all 975 keys with values. The secret key `runtime.encryptionKeyId` is redacted, and `runtime.encryptionKey` is not dumped by Bifrost. Groups:

<!-- GEN:config_groups -->
| group | keys in final NvscClientConfig dump |
|---|---|
| `vqos` | 456 |
| `video` | 209 |
| `general` | 163 |
| `bwe` | 33 |
| `runtime` | 18 |
| `audioBitrate` | 18 |
| `audio` | 17 |
| `packetPacing` | 16 |
| `mic` | 14 |
| `aqos` | 12 |
| `ri` | 8 |
| `qscore` | 6 |
| `clientPerfBr` | 5 |
| total | 975 |
<!-- /GEN:config_groups -->

Keys that matter for an OpenNOW implementation, with their S6 values:

| key | value | meaning / evidence |
|---|---|---|
| `general.rtspScheme` | 3 | WSS signaling (**I** from the `protocol TAG 'WSS'` line) |
| `general.featureFlags` | 183 | bitfield (**U** bit meanings) |
| `general.useReliableUdp` | 10 | reliable-UDP control version/mode (**U** exact semantics) |
| `general.pingVersion` | 6 | matches the SETUP `x-nv-ping: 6` |
| `general.enableStunConsent` | 0 | no STUN consent freshness |
| `general.enableUnifiedSocket` | 0 | `Unified socket mode disabled (... legacy dedicated sockets)` |
| `general.rtcVideoOnNativeBundle` / `rtcAudioOnNativeBundle` / `rtcMicOnNativeBundle` / `rtcDataChannelOnNativeBundle` / `nativeRtcOnBundlePort` | 0 / 1 / 1 / 1 / 1 | video on its own socket. Audio, mic and SCTP on the bundle |
| `general.serverBundlePort` / `clientBundlePort` / `clientBundlePortUsage` | 47999 / 49006 / 0 | the real peer was 5004 (see above) |
| `general.clientPorts.*` | video/audio/mic/control/bundle/session 0, `useReserved 1`, `fallbackDynamic 1` | `Enable dynamic fallback when using reserved ports`. The actual ports were 49005/49006 |
| `general.rtcpOnSctp` | 1 | RTCP carried over SCTP |
| `general.enableRecoveryMode` / `recoveryModeTriggerThresholdMs` / `enableIntraRefreshInRecoveryMode` | 1 / 6000 / 1 | recovery mode after 6 s of trouble |
| `general.forcedPeriodicSettings.frameInvalidationInterval` / `invalidationFrameRange` / `dumpDroppedFramesDueToInvalidation` | 0 / 0 / 1 | no forced invalidation |
| `general.rtcAppMessageSettings.maxRtpMappingVersion` | 2 | none |
| `video[0].bitDepth` / `chromaFormat` | 10 / 1 | 10-bit 4:4:4 (decoder chroma 3, DXGI Y410) |
| `video[0].clientViewportWd/Ht`, `maxFPS` | 2560 / 1440, 120 | none |
| `video[0].packetSize` | 1408 | `Updating streamer packetSize ... to use MTU value determined by NCT. New packetSize: 1408 bytes, NCT detected packet size: 1432` |
| `video[0].streamRecoveryType` / `refPicInvalidation` / `encoderLtrFeatureSetting` | 2 / 1 | 2 (RFI-capable; QEC disabled because of display-freeze-on-invalid-ref) |
| `video[0].enableRtpNack` / `rtpNackVersion` / `rtpNackQueueLength` / `rtpNackQueueMaxPackets` / `rtpNackMaxPacketCount` | 1 / 2 / 2048 / 1024 / 64 | NACK v2 |
| `video[0].initialBitrateKbps` / `vqos[0].bw.maximumBitrateKbps` / `minimumBitrateKbps` | 12135 / 48543 / 1000 | none |
| `video[0].l4sHandling.*` | `version 1`, `enableEcnFeedback 0`, ... | L4S off |
| `vqos[0].fec.*` | `enable 1`, `repairPercent 20`, `repairMinPercent 5`, `repairMaxPercent 40`, `minRequiredFecPackets 2` | dynamic FEC on |
| `vqos[0].bllFec.enable` | 1 | none |
| `runtime.videoSrtp` / `audioSrtp` / `micSrtp` | 1 / 0 / 0 | video SRTP via the RTSPS-delivered key (`Encryption key in RTSPS`) |
| `audio.surround.enable` / `numChannels` / `channelMask` | 0 / 2 / 3 | stereo |
| `mic.numChannels` / `bitrate` / `useOpusInBandFec` | 1 / 16000 / 0 | none |
| `mic.micSsrcConfig.senderSsrc` / `audio.audioSsrcConfig.senderSsrc` | 5 / 1 | none |
| `packetPacing.version` / `mode` / `numGroups` / `minNumPacketsPerGroup` / `maxDelayUs` | 3 / 1 / 5 / 0 / 1000 | none |
| `ri.enablePartiallyReliableTransferGamepad` / `...Hid` / `partialReliableThresholdMs` / `usePartiallyReliableUdpChannel` | 255 / -1 / 300 / 0 | none |
| `runtime.hapticsState` | 0 at dump, then `Sending haptics state 1` | none |

## Comparison with OpenNOW's ANNOUNCE builder

OpenNOW: frozen `7ce3cd54` after #1162, #1164 and #1165. `compare_opennow.py --ref 7ce3cd54` inventories production `a=x-nv-*` literals, excluding inline and separate tests, and joins them with the printed Windows server lines and final config. These are candidate literals, not proof that every key is emitted for S6's offer or stream settings.

Summary counts: 62 printed server keys, 91 OpenNOW production literal keys, 51 server keys without a matching production literal, and 80 OpenNOW keys not printed in that server section. The earlier 94-key count mistakenly included negative test assertions.

**OpenNOW raw builder defaults that differ from Windows final config** (Windows value -> OpenNOW default, not actual transmitted value):

- `mic.micSsrcConfig.senderSsrc`: 5 -> 1
- `packetPacing.minNumPacketsPerGroup`: 0 -> 15
- `vqos[0].bllFec.enable`: 1 -> 0
- `vqos[0].fec.repairMaxPercent`: 40 -> 35
- `vqos[0].fec.repairMinPercent`: 5 -> 20

Neither column is the captured body. OpenNOW's `omit_server_announce_attributes` at `nvst_rtsp.rs:1378-1459` filters the FEC/pacing defaults when DESCRIBE supplies them. Windows also compares against DESCRIBE ([announce-builder.md](announce-builder.md)). The mic SSRC default and actual RTP sender remain a separate comparison.

**OpenNOW candidate literals absent from the Windows final config dump:** `general.clientPorts.localAddress`, `general.dtlsFingerprintV2`, `general.icePasswordV2`, `general.iceUserNameFragmentV2`, `runtime.encryptionKey`. Windows supports V2 forms and skips nullable V1 fields in its serializer. Their exact S6 body values are not logged and must remain private. `clientSupportHevc`, split-encode strip count and `grc.enable` are not production literals in the frozen builder. `dynamicRangeMode:1` is conditional on HDR, not sent as zero for SDR.

**Printed server keys without a matching OpenNOW production literal.** Lack of an ANNOUNCE echo does not establish whether a client consumes a setting or whether ignoring it is safe. These include `aqos.*`, audio buffer policy, custom-message/transport controls, HID permissions, encoder tuning and statistics settings. The table retains each key for owner-specific follow-up rather than treating all of them as server-only tuning.

Several affect client behavior, including RED levels, HID permissions and transport MTU. OpenNOW already decodes downstream RED and implements DS4 synthesis; absence from this literal inventory is not evidence of absent feature handling.

Full join table (generated):

<!-- GEN:xnv_compare -->
| key | Windows server DESCRIBE | Windows final config | OpenNOW ANNOUNCE literal | OpenNOW source |
|---|---|---|---|---|
| `aqos.enableRedundancy` | 1 | 1 |  |  |
| `aqos.enableRedundancyForMic` | 1 | 1 |  |  |
| `aqos.redundancyLevel` | 2 | 2 |  |  |
| `aqos.redundancyLevelForMic` | 3 | 3 |  |  |
| `audio.enableDynamicAudioConfig` | 1 | 1 |  |  |
| `audio.enableTimestampAudioBuffer` | 1 | 1 |  |  |
| `bwe.useOwdCongestionControl` |  | 1 | 1 | nvst_rtsp.rs:1547 |
| `general.clientBundlePort` |  | 49006 | {fmt} | nvst_rtsp.rs:1567 |
| `general.clientPorts.audio` |  | 0 | 0 | nvst_rtsp.rs:1559 |
| `general.clientPorts.bundle` |  | 0 | 0 | nvst_rtsp.rs:1562 |
| `general.clientPorts.control` |  | 0 | 0 | nvst_rtsp.rs:1561 |
| `general.clientPorts.fallbackDynamic` |  | 1 | 1 | nvst_rtsp.rs:1566 |
| `general.clientPorts.localAddress` |  | (not dumped) | {fmt} | nvst_rtsp.rs:1564 |
| `general.clientPorts.mic` |  | 0 | 0 | nvst_rtsp.rs:1560 |
| `general.clientPorts.session` |  | 0 | 0 | nvst_rtsp.rs:1563 |
| `general.clientPorts.useReserved` |  | 1 | 1 | nvst_rtsp.rs:1565 |
| `general.clientPorts.video` |  | 0 | 0 | nvst_rtsp.rs:1558 |
| `general.customMessageOnCC` | 1 | 1 |  |  |
| `general.dtlsFingerprintV2` |  | (not dumped) | {fmt} | nvst_rtsp.rs:1582 |
| `general.enableUDSWifiCongestion` | 1 | 1 |  |  |
| `general.enableUDSWifiStandard` | 1 | 1 |  |  |
| `general.enableUnifiedSocket` |  | 0 | {fmt} | nvst_rtsp.rs:1573 |
| `general.enetControlChannel.mtuSize` | 1191 | 1191 |  |  |
| `general.icePasswordV2` |  | (not dumped) | {fmt} | nvst_rtsp.rs:1581 |
| `general.iceUserNameFragmentV2` |  | (not dumped) | {fmt} | nvst_rtsp.rs:1580 |
| `general.maxQosMessagesSize` | 1071 | 1071 |  |  |
| `general.nativeRtcOnBundlePort` | 1 | 1 | 1 | nvst_rtsp.rs:1568 |
| `general.pingIntervalAfterConnectionMs` | 100 | 100 | 100 | nvst_rtsp.rs:1550 |
| `general.pingIntervalBeforeConnectionMs` | 20 | 20 | 20 | nvst_rtsp.rs:1549 |
| `general.rtcAudioOnNativeBundle` | 1 | 1 | 1 | nvst_rtsp.rs:1570 |
| `general.rtcDataChannelOnNativeBundle` |  | 1 | 1 | nvst_rtsp.rs:1571 |
| `general.rtcMicOnNativeBundle` | 1 | 1 | 1 | nvst_rtsp.rs:1594 |
| `general.rtcVideoOnNativeBundle` |  | 0 | 0 | nvst_rtsp.rs:1569 |
| `general.rtcpOnSctp` |  | 1 | {fmt} | nvst_rtsp.rs:1577 |
| `general.rtspWebSocketPerConnection` | 1 | 1 | 1 | nvst_rtsp.rs:1548 |
| `general.transitSelectionSettings.enableDynamicTransitSelection` | 1 | 1 |  |  |
| `general.transitSelectionSettings.selectionEnforcementMethod` | 1 | 1 |  |  |
| `general.wifiInfoInterval` | 15 | 15 |  |  |
| `mic.micSsrcConfig.senderSsrc` |  | 5 | 1 | nvst_rtsp.rs:1595 |
| `packetPacing.allowFpsBasedToggle` |  | 1 | 1 | nvst_rtsp.rs:1540 |
| `packetPacing.enableAccurateSleep` | 1 | 1 | 1 | nvst_rtsp.rs:1538 |
| `packetPacing.enableSmoothTransition` |  | 1 | 1 | nvst_rtsp.rs:1539 |
| `packetPacing.maxDelayUs` |  | 1000 | {fmt} | nvst_rtsp.rs:1533 |
| `packetPacing.minNumPacketsFrame` |  | 10 | 10 | nvst_rtsp.rs:1536 |
| `packetPacing.minNumPacketsPerGroup` |  | 0 | 15 | nvst_rtsp.rs:1537 |
| `packetPacing.mode` |  | 1 | 1 | nvst_rtsp.rs:1530 |
| `packetPacing.numGroups` |  | 5 | 5 | nvst_rtsp.rs:1531 |
| `packetPacing.version` |  | 3 | 3 | nvst_rtsp.rs:1529 |
| `ri.enablePartiallyReliableTransferGamepad` |  | 255 | 255 | nvst_rtsp.rs:1545 |
| `ri.enablePartiallyReliableTransferHid` |  | -1 | -1 | nvst_rtsp.rs:1546 |
| `ri.hidDeviceMask` | 4 | 4 |  |  |
| `ri.partialReliableThresholdMs` | 300 | 300 | 300 | nvst_rtsp.rs:1541 |
| `ri.timestampsEnabled` |  | 1 | 1 | nvst_rtsp.rs:1542 |
| `ri.useMultipleGamepads` |  | 1 | 1 | nvst_rtsp.rs:1543 |
| `ri.usePartiallyReliableUdpChannel` |  | 0 | 0 | nvst_rtsp.rs:1544 |
| `runtime.audioSrtp` | 0 | 0 | 0 | nvst_rtsp.rs:1551 |
| `runtime.encryptionKey` |  | (not dumped) | {fmt} | nvst_rtsp.rs:1556 |
| `runtime.encryptionKeyId` |  | <redacted> | {fmt} | nvst_rtsp.rs:1557 |
| `runtime.maxVerboseEtlSizeMb` | 60 | 60 |  |  |
| `runtime.micSrtp` | 0 | 0 | 0 | nvst_rtsp.rs:1552 |
| `runtime.mimicRemoteCursor` |  | 0 | 0 | nvst_rtsp.rs:1554 |
| `runtime.mouseCursorCapture` |  | 3 | 3 | nvst_rtsp.rs:1553 |
| `runtime.videoSrtp` |  | 1 | 1 | nvst_rtsp.rs:1555 |
| `video[0].adaptiveQuantization.perfAdjEnablement` | 1 | 1 |  |  |
| `video[0].adaptiveQuantization.qpThresholdAdjPercent` | 2 | 2 |  |  |
| `video[0].adaptiveQuantization.saqAdaptDecayStrengthX100` | 250 | 250 |  |  |
| `video[0].adaptiveQuantization.saqAdaptMaxQpThresholdPercent` | 100 | 100 |  |  |
| `video[0].adaptiveQuantization.saqAdaptMinQpThresholdPercent` | 40 | 40 |  |  |
| `video[0].adaptiveQuantization.spatialAQSetting` | 7 | 7 |  |  |
| `video[0].adaptiveQuantization.spatialAQStrength` | 12 | 12 |  |  |
| `video[0].adaptiveQuantization.temporalAQSetting` | 0 | 0 |  |  |
| `video[0].bitDepth` |  | 10 | {fmt} | nvst_rtsp_color.rs:18 |
| `video[0].chromaFormat` |  | 1 | {fmt} | nvst_rtsp_color.rs:22 |
| `video[0].clientViewportHt` |  | 1440 | {fmt} | nvst_rtsp.rs:1483 |
| `video[0].clientViewportWd` |  | 2560 | {fmt} | nvst_rtsp.rs:1482 |
| `video[0].dynamicRangeMode` |  | 0 | 1 | nvst_rtsp_color.rs:27 |
| `video[0].enableAv1RcPrecisionFactor` | 1 | 1 |  |  |
| `video[0].enableRtpNack` |  | 1 | 1 | nvst_rtsp.rs:1490 |
| `video[0].encoderCscMode` |  | 2 | 2 | nvst_rtsp.rs:1507 |
| `video[0].encoderHdrCscMode` |  | 4 | 4 | nvst_rtsp.rs:1508 |
| `video[0].framePacing.feedbackMode` |  | 1 | {fmt} | nvst_rtsp.rs:1499 |
| `video[0].framePacing.mode` |  | 1 | {fmt} | nvst_rtsp.rs:1495 |
| `video[0].framePacing.pid.minTargetFrameTimeUs` | 7936 | 7936 |  |  |
| `video[0].initialBitrateKbps` |  | 12135 | {fmt} | nvst_rtsp.rs:1511 |
| `video[0].initialPeakBitrateKbps` |  | 12135 | {fmt} | nvst_rtsp.rs:1512 |
| `video[0].mapRtpTimestampsToFrames` |  | 0 | 0 | nvst_rtsp.rs:1509 |
| `video[0].maxCodecLevel` |  | 51 | 51 | nvst_rtsp.rs:1485 |
| `video[0].maxCodecProfile` |  | 3 | 3 | nvst_rtsp.rs:1484 |
| `video[0].maxFPS` |  | 120 | {fmt} | nvst_rtsp.rs:1510 |
| `video[0].maxH264Level` |  | 51 | 51 | nvst_rtsp.rs:1487 |
| `video[0].maxH264Profile` |  | 3 | 3 | nvst_rtsp.rs:1486 |
| `video[0].maxNumReferenceFrames` |  | 0 | 0 | nvst_rtsp.rs:1502 |
| `video[0].packetSize` |  | 1408 | {fmt} | nvst_rtsp.rs:1489 |
| `video[0].prefilterParams.denoiseLevel` |  | 0 | 0 | nvst_rtsp.rs:1505 |
| `video[0].prefilterParams.prefilterMode` |  | 0 | 0 | nvst_rtsp.rs:1503 |
| `video[0].prefilterParams.prefilterModel` |  | 4 | 4 | nvst_rtsp.rs:1504 |
| `video[0].prefilterParams.sharpnessLevel` |  | 0 | 0 | nvst_rtsp.rs:1506 |
| `video[0].rtpNackMaxPacketCount` |  | 64 | 64 | nvst_rtsp.rs:1493 |
| `video[0].rtpNackQueueLength` |  | 2048 | 2048 | nvst_rtsp.rs:1491 |
| `video[0].rtpNackQueueMaxPackets` |  | 1024 | 1024 | nvst_rtsp.rs:1492 |
| `video[0].updateSplitEncodeStateDynamically` |  | 1 | 1 | nvst_rtsp.rs:1488 |
| `video[0].vbvMultiplier` | 109 | 109 |  |  |
| `video[0].videoSplitEncodeStripsPerFrame` | 64 | 64 |  |  |
| `vqos[0].bitStreamFormat` |  | 1 | {fmt} | nvst_rtsp.rs:1513 |
| `vqos[0].bllFec.enable` |  | 1 | 0 | nvst_rtsp.rs:1520 |
| `vqos[0].bw.maximumBitrateKbps` |  | 48543 | {fmt} | nvst_rtsp.rs:1524 |
| `vqos[0].bw.minimumBitrateKbps` |  | 1000 | {fmt} | nvst_rtsp.rs:1525 |
| `vqos[0].calculateAvgVideoStreamingBitrate` | 1 | 1 | 1 | nvst_rtsp.rs:1523 |
| `vqos[0].dfc.adjustResAndFps` |  | 0 | {fmt} | nvst_rtsp.rs:1522 |
| `vqos[0].drc.bitrateIirFilterFactor` |  | 128 | 128 | nvst_rtsp.rs:1526 |
| `vqos[0].drc.enable` |  | 0 | 0 | nvst_rtsp.rs:1521 |
| `vqos[0].dynamicStreamingMode` |  | 0 | {fmt} | nvst_rtsp.rs:1528 |
| `vqos[0].encoderUtilizationBooster.bitrateMultiplierUpperLimitPercent` | 15 | 15 |  |  |
| `vqos[0].encoderUtilizationBooster.featureSetting` | 3 | 3 |  |  |
| `vqos[0].encoderUtilizationBooster.iirFilterFactor` | 30 | 30 |  |  |
| `vqos[0].encoderUtilizationBooster.maxAllowedUtilizationPercent` | 100 | 100 |  |  |
| `vqos[0].encoderUtilizationBooster.underUtilizationUpperLimitThresholdPercent` | 95 | 95 |  |  |
| `vqos[0].fec.enable` |  | 1 | 1 | nvst_rtsp.rs:1514 |
| `vqos[0].fec.minRequiredFecPackets` |  | 2 | 2 | nvst_rtsp.rs:1516 |
| `vqos[0].fec.rateDropWindow` |  | 10 | 10 | nvst_rtsp.rs:1515 |
| `vqos[0].fec.repairMaxPercent` |  | 40 | 35 | nvst_rtsp.rs:1519 |
| `vqos[0].fec.repairMinPercent` |  | 5 | 20 | nvst_rtsp.rs:1518 |
| `vqos[0].fec.repairPercent` |  | 20 | 20 | nvst_rtsp.rs:1517 |
| `vqos[0].grc.enable` | 0 | 0 |  |  |
| `vqos[0].qec.featureSetting` | 3 | 0 |  |  |
| `vqos[0].qec.packetLossConcealmentSetting` | 2 | 2 |  |  |
| `vqos[0].qpDelta.qpDeltaIirFactor` | 60 | 60 |  |  |
| `vqos[0].qpDelta.qpDeltaMaxPercent` | 10 | 10 |  |  |
| `vqos[0].qpDelta.qpDeltaMinPercent` | 60 | 60 |  |  |
| `vqos[0].qpDelta.qpDeltaSurfaceAdjustmentStrengthPercent` | 70 | 70 |  |  |
| `vqos[0].qpDelta.qpDeltaThrottlePercent` | 100 | 100 |  |  |
| `vqos[0].qpDelta.qpDeltaVbvUsageFactorPercentAv1` | 100 | 100 |  |  |
| `vqos[0].qpDelta.qpDeltaVbvUsageFactorPercentH264` | 100 | 100 |  |  |
| `vqos[0].qpDelta.qpDeltaVbvUsageFactorPercentH265` | 100 | 100 |  |  |
| `vqos[0].relaxMaxBitrate.customAvgBitrateThresholdPercent` | 65 | 65 |  |  |
| `vqos[0].relaxMaxBitrate.customAvgQpThresholdPercent` | 51 | 51 |  |  |
| `vqos[0].relaxMaxBitrate.iirFilterFactor` | 120 | 120 |  |  |
| `vqos[0].relaxMaxBitrate.overrideAvgBitrateThresholdPercent` | 4 | 4 |  |  |
| `vqos[0].relaxMaxBitrate.overrideAvgQpThresholdPercent` | 7 | 7 |  |  |
| `vqos[0].resControl.bitrateIirFilterFactor` |  | 128 | 128 | nvst_rtsp.rs:1527 |
| `vqos[0].sendEndOfSessionQosTelemetry` | 1 | 1 |  |  |
| `vqos[0].statsProcessorThread.flags` | 1023 | 1023 |  |  |
<!-- /GEN:xnv_compare -->
