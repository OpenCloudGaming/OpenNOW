# NVST handshake on Windows (GFN 2.0.89.141, Geronimo gs_04_92 38967594)

The separately supplied `Bifrost2.dll`, `gs_04_92 38967594`, now supplies Windows static evidence for RTSP/WSS, sockets, DTLS/SRTP and SCTP. The S6 timeline below remains observed log evidence. The static sections distinguish supported branches from the one selected session.

Evidence tags: **S** static, **D** decompiled/disassembled (Geronimo.dll base 0x180000000, GeForceNOW.exe base 0x140000000), **L** log line (file, local UTC+2 time), **I** inferred, **U** unknown. Unless noted, log lines are from `geronimo.log.bak`, session started 2026-10-12 19:45 local (S6) (the second session in that file; the first ended at 19:33:41).

## 1. Layers and who owns what

| Layer | Binary | Evidence |
|---|---|---|
| UI, CloudMatch query bridge (`QUERY_GFN_PREPARE`, `QUERY_GFN_START`, `QUERY_GFN_RESUME`) | GeForceNOW.exe | **S** command list from 0x14028d820; **D** handler 0x1401a69c0 reads `address`, `serverType`, `partnerCustomData`, `deviceId`, `token`, `tokenType`, `clientHeaders` |
| Session orchestration, decode, render, input; caller of 31 `nvb*` imports | Geronimo.dll | **S** IAT; **D** `BifrostSDKExecutor` vtable 0x18141c1c0 |
| CloudMatch HTTP, RTSPS/WSS, ICE/DTLS/SRTP, SCTP, Mjolnir, NACK, QoS | Bifrost2.dll | **D/L**: tags `[BifrostClient: GridServer]`, `[BifrostClient: Streamer]`, `[NVST:*]` |

`nvbCreateClient` callers: 0x1800c8e20, 0x18012e310, 0x180139ac0. `nvbStartSession`: 0x180130170, 0x180130250. `nvbStartStreaming`: 0x1800cdcc0, 0x1800ce3e0, 0x18013bf50. `nvbPauseSession` 0x18012e680, `nvbResumeSession` 0x18012f740/0x18012faf0, `nvbSetAuthInfo` 0x18012fce0, `nvbStopSession` 0x1801303a0 (**S/D**, IAT caller scan).

## 2. Server type conversion

GeForceNOW.exe 0x1401a2c00 converts the JS `serverType` into a Bifrost server type (**D**, log literals at 0x1402a12c8..0x1402a1490):

| JS value | Returned | Name |
|---|---|---|
| 1 | 0 | `NVB_SERVER_NIMBUS` |
| 2 | 1 | `NVB_SERVER_GAMESTREAM` |
| 3 | 2 | `NVB_SERVER_ROAMING_PROXY` |
| 4 | 3 | `NVB_SERVER_CLOUD_STATION` |
| 5 | 4 | `NVB_SERVER_QUADRO_STATION` |
| 1001 (0x3e9) | 0x33 (51) | `NVB_SERVER_PASSTHRU` (branch 0x1401a2ddf, `mov eax,0x33` at 0x1401a2e24) |
| 0, 6, other | -1 | `NVB_SERVER_UNKNOWN` |

Logs: console.log `streamingService prepare params` has `"serverType": 1` (**L**); debug.log has `Pre-NVb-conversion ServerType : 1` (**L**); Bifrost then logs `Converted Nvb server type 0 to nvsc Server type 2` (19:45:11, **L**). So NIMBUS (Bifrost 0) maps to NVSC server type 2. Bifrost `0x18081f0d0` maps native type 1 to NVSC 1, type 2 to NVSC 3, and every other type to NVSC 2 (**D**). Server-info JSON types and explicit app types are separate conversions; see [cloudmatch-native.md](cloudmatch-native.md).

## 3. CloudMatch session request and response

Bifrost logs the serialized request (`Serialized session request. Size: 1957`, 19:45:11, **L**). Field names (values redacted where identifying):

- Identity of client: `clientIdentification` "GFN-PC", `clientVersion` "30.0", `sdkVersion` "2.0", `streamerVersion` "14", `clientPlatformName` "Windows", `deviceHashId` <redacted>.
- Session: `appId`, `externalAppId`, `internalTitle`, `appLaunchMode` 1, `parentSessionId`, `networkTestSessionId`, `useOps` true, `accountLinked` false, `userAge`, `enablePersistingInGameSettings`, `clientTimezoneOffset`.
- Transport hints: `secureRTSPSupported` true, `transport` null, `partnerCustomData` null.
- Display/audio: `clientRequestMonitorSettings[]` (monitorId, position, size, dpi, framesPerSecond, sdrHdrMode, displayData, hdr10PlusGamingData), `sdrHdrMode`, `clientDisplayHdrCapabilities`, `audioMode` 2, `surroundAudioInfo`, `requestedAudioFormat`.
- `requestedStreamingFeatures`: reflex, bitDepth, cloudGsync, enabledL4S, mouseMovementFlags, trueHdr, supportedHidDevices, profile, fallbackToLogicalResolution, hidDevices, chromaFormat, prefilterMode/Sharpness/NoiseReduction, hudStreamingMode, qosPolicy, touchSupport, dlssOverrides{srMode,srModel,srScale,rrModel,fgMode,fgModel,fgMultiplier}.
- `metaData[]` keys: `ClientImeSupport`, `SubSessionId`, `clientPhysicalResolution`, `networkType`, `wssignaling` (value 1), `surroundAudioInfo`.

The response (`Client Request: 11, server response:`, 19 polls, **L**) carries `session.status` 1 for 18 polls and 2 once, `seatSetupInfo{queuePosition,seatSetupStep,seatSetupEta}`, `finalizedStreamingFeatures`, `userIdleWarningTimeoutInMs` (0 here), `gpuType`, `clientIp` <redacted>, `requestStatus{statusCode,unifiedErrorCode,serverId,...}`.

### Connection info and port usage

In this NVIDIA (NIMBUS) session the response had exactly these endpoints (all 19 polls, **L**):

| Object | usage | protocol | appLevelProtocol | port | resourcePath |
|---|---|---|---|---|---|
| `sessionControlInfo` | 10 | 0 | 5 | 443 | `/v2/session/<redacted>` |
| `connectionInfo[0]` | 14 | 1 | 6 | 322 | `rtsps://<redacted-ip>.cloudmatchbeta.nvidiagrid.net:322` |
| `connectionInfo[1]` | 14 | 1 | 6 | 48322 | `rtsps://<redacted-ip>.cloudmatchbeta.nvidiagrid.net:48322` |

The log names the converted endpoints `NVB_PU_RTSPS`. The DLL proves that JSON usage 14 is SIGNALING, mapped to native RTSPS 10 only for app protocol 1 or 6; JSON usage 16 also maps to native RTSPS 10. JSON BUNDLE 17 maps to native BUNDLE 11. Usage 15 is skipped. See [cloudmatch-native.md](cloudmatch-native.md) for the full table and the next NVST conversion. No JSON VIDEO 2 or BUNDLE 17 entry appeared in the supplied sessions.

Geronimo has its own usage map in `Nsk::getStreamStartParameters` 0x18012af30 (jump table at RVA 0x12bbf4, **D**): 1→1, 2→2, 3→3, 4→4, 5→2, 7→5, 12→5, 13→6, 14→5, 16→7, 17→8; 6, 8–11 and 15 log `Unknown port usage detected in the SessionObject`. This path belongs to the Nsk/SessionObject API (no direct callers found; its log line `Populating connection info: ... Port Usage: %d` at 0x18013f040 never appears in the logs), so use by this PC session is unproved. The meaning of internal values 1–8 is **U**. This table is separate from Bifrost's JSON-to-native table and does not map 14 and 16 to the same value.

## 4. Signaling: RTSP over secure WebSocket

1. `Using Secure WebSockets for Signaling` (`[BifrostClient: Streamer]`, 19:45:11, **L**), consistent with request `metaData` `wssignaling`=1 and `general.rtspWebSocketPerConnection`=1 (NvscClientConfig, **L**).
2. `WSS Options: rtsps://<redacted-ip>.cloudmatchbeta.nvidiagrid.net:322` (19:45:30.560). The server answers `X-GS-Version: 14.2` and `Public: OPTIONS,DESCRIBE,ANNOUNCE,SETUP,TEARDOWN,PLAY,PAUSE,X_NV_COMMAND,X_NV_EVENT` (**L**).
3. DESCRIBE: `application/sdp`, `Content-Length: 219292`, `WSS Describe: latency is: 69.9 ms` (**L**). Bifrost prints only the server's "new features" SDP subset (`SignalingHandler new features:`; 62 `a=x-nv-*` keys such as `general.customMessageOnCC`, `vqos[0].qec.featureSetting`, `packetPacing.enableAccurateSleep`). The SDP origin line carries `k=HMAC:<redacted>`. Then `SDP: Received QosFeedback version from server: [7]`, `QosTimings version ... [5]`, `Blob stats version server: [9], client: [9]` (19:45:30.643, **L**).
4. Control protocol negotiation: `Adding support for protocol` udp, udp_ag, tcp, udp_enc, udp_ag_enc, tcp_enc, channel_encrypt; `updateControlProtocol has picked udp_ag (2) - not encrypted`; `Final Selected ControlProtocol: udp_ag, controlControlId: streamid=control/10` (19:45:30.643, **L**). `ServerControlRUA` later logs `Instanstiated ServerControlReliableUdpAggregated` (19:45:31.337).
5. SETUP for video only: `WSS Setup: streamid=video/0/0`; response `Transport: unicast;X-GS-ServerPort=5004-5005;source=<redacted-ip>`, `X-Nv-Ping-Payload: <redacted>`, `x-nv-ping: 6`, latency 173 ms (19:45:30.827, **L**). The video RTP source is bound to local port 49005 (`UDP RTP Source: bound to <redacted-ip>:49005`).
6. Audio and mic skip RTSP SETUP: `Bypassing 'Mjolnir' SETUP for the Audio stream. Deferring to WebRtcTransport setup` (same for Mic, 19:45:30.654, **L**). This matches NvscClientConfig `rtcAudioOnNativeBundle`=1, `rtcMicOnNativeBundle`=1, `rtcVideoOnNativeBundle`=0.
7. `Encryption key in RTSPS`; `setEncryptionKey` for audio and video with a single key ID (value <redacted>) (19:45:30.828, **L**). The media key travels inside the RTSPS exchange.
8. ANNOUNCE body: `AppendNvscConfigToSdp` → `Successfully appended 163 NvscClientConfig attributes to SDP`. Bifrost skips `x-nv-general.iceUsernameFragment`, `x-nv-general.iceUsernamePwd` and `x-nv-general.dtlsFingerprint` (`addSdpAttribute: Skipping field`, **L**). OpenNOW's `build_announce` matches this: it writes only `x-nv-general.iceUserNameFragmentV2`, `icePasswordV2` and `dtlsFingerprintV2`, plus the standard `a=ice-ufrag`, `a=ice-pwd` and `a=fingerprint:sha-256` lines (`nvst_rtsp.rs:1580-1586` on dev `7ce3cd54`). Windows supports and conditionally serializes RFI/recovery attributes, comparing against DESCRIBE; see [announce-builder.md](announce-builder.md). The 163 client attributes themselves are not logged; the effective config is the 973-row `NvscClientConfig` dump (see configuration-fields.md §11).
9. `WSS Announce: rtsps://...:322` (19:45:30.876), `RtspAnnounce duration greater than 300 ms` (19:45:31.258, ~382 ms). ICE/DTLS runs during ANNOUNCE (section 5).
10. After ANNOUNCE, `Signaling Server Ip: rtsps://<redacted-ip>.cloudmatchbeta.nvidiagrid.net:48322` and `general.serverEndpoints: RTSP hanshake 322 control 47995 UDP control 47999 audio 48000 input 47995 bundle 48001 ...` (19:45:31.258, **L**).
11. `WSS Play : sessionId : <redacted>`, `WSS Play: latency is: 113.7 ms with code 200:200` (19:45:31.480, **L**).

## 5. NAT punch, ICE, DTLS, SRTP

- `Set ping-version: 6` and `Client early ICE socket bound to port 49006` (19:45:30.828, **L**). `NattHolePunch` sends `Send src ping version 6 on port 49006 to dest <redacted-ip>:5004: Success` (30 lines) and the same from port 49005 (10 lines), then `Setting <redacted-ip>:5004 as valid destination` (19:45:31.323). Port 49006 is `general.clientBundlePort`; 49005 is the video socket.
- `No need to capture candidates for cloud server` (**L**): no host-candidate gathering for NIMBUS.
- `DTLS handshake done.` (19:45:31.337) followed by one `Retrying DTLS handshake after timeout, retry count 1` from a second `DtlsTransport` thread (19:45:31.354), and a telemetry event `NvstDebug_Event DTLSHandshake Succeed` (**L**).
- `OpenSSLCertUtils` logs 37 `Certificate bag` lines, `PKCS7 Encrypted data`, and root subjects (GlobalSign, VeriSign Universal Root) (**L**). These are signaling trust-store logs. DTLS separately compares the SHA-256 peer-certificate fingerprint before exporting keys (`0x18043ba90`, **D**); signaling TLS-chain/hostname verification remains a separate boundary.
- SRTP: `runtime.srtpReplayWindowSize: 1024`; at session end `SRTP encrypt failures: total=0 replay=0 cipher=0 other=0` and `RtpStats percPacketsDroppedByReplayProtectionx1e6 0.000000, percPacketsDroppedByAuthFailx1e6 0.000000` (19:33:41, **L**).

## 6. Pause, resume and session reuse

Geronimo calls `nvbPauseSession` from 0x18012e680 and `nvbResumeSession` from 0x18012f740 and 0x18012faf0 (**D**). `QUERY_GFN_RESUME` is one of the commands handled by GeForceNOW.exe 0x1401a69c0 (**S/D**). Neither call appears in the supplied logs, so the wire effect of pause/resume is **U**. Resumability after termination is decided by Geronimo helper 0x180142a70; see transport-feedback.md §8.

## 7. Comparison with macOS Bifrost and OpenNOW

- macOS Bifrost (PR #1161) describes three socket arrangements: RTC-compliant, unified Mjolnir, and legacy dedicated sockets. The Windows NIMBUS session used the legacy dedicated video socket: `Unified socket mode disabled (enableUnifiedSocket=0, using legacy dedicated sockets)` (19:45:31.364, **L**), with audio/mic/data channels on the native bundle. This is consistent with macOS.
- OpenNOW `native/opennow-core/src/cloudmatch.rs` picks a media endpoint from usage 2 or 17 when present. In this Windows NVIDIA session no such entry exists, so OpenNOW (`nvst_rtsp.rs`, `parse_video_peer` and the bundle-peer fallback after `setup_video`) takes the video peer from the SETUP `Transport` `source`/`X-GS-ServerPort` and reuses it as the bundle peer. That is the same peer both Windows sockets used (server port 5004). OpenNOW does not use `general.serverBundlePort` as a peer; it uses it only to build the NAT ping username (`bundle_natt_username`) (read from code on dev `7ce3cd54`). The two clients therefore agree on NVIDIA seats; the ping-username rule has no Windows evidence either way (**U**).
- OpenNOW `build_announce` (`nvst_rtsp.rs:1580-1586`) does not emit the V1 keys `general.iceUsernameFragment`, `general.iceUsernamePwd` or `general.dtlsFingerprint`. It emits `general.iceUserNameFragmentV2`, `general.icePasswordV2` and `general.dtlsFingerprintV2`, plus the standard `a=ice-ufrag`, `a=ice-pwd` and `a=fingerprint:sha-256` lines. Windows Bifrost skips the same three V1 fields (**L**), so the two clients agree. OpenNOW still reads the V1 names from the server's DESCRIBE SDP as a fallback (`nvst_rtsp.rs:1079-1084`), which is parsing, not sending.
- OpenNOW's request builder (`cloudmatch.rs` ~1656) sends `metaData` `wssignaling`, `SubSessionId` and `surroundAudioInfo`, like Windows. Windows also sends `ClientImeSupport`, `clientPhysicalResolution` and `networkType`; OpenNOW deliberately omits all three (test `session_metadata_contains_only_owned_values` in `cloudmatch.rs`). Their server-side effect is **U**.

## 8. Windows static routing and retry

| Function | Evidence and consequence |
| --- | --- |
| `0x1803babe0` | `D`, Gets the configured handshake port through `0x18026cba0`, then calls `0x1803af070` and creates the session through `0x1803c32f0`. The extension version is formatted from 14 and 2. The handshake port is not globally fixed to 322. |
| `0x1803af070` | `D`, Recognizes `rtsp://`, `rtsps://`, `ws://`, and `wss://` case-insensitively, then extracts an address and decimal port. The parser overwrites the supplied port. Its delimiter is an unresolved global string, so IPv6 and absent-port behavior are not proved. Recognizing a scheme is not proof of the final transport factory's security policy. |
| `0x1803c32f0` | `D`, 101-140. Adds nonempty `x-nv-sessionid`, forwards header version, URL, port, configuration, and callbacks into `0x1804acd90`. The downstream WebSocket-versus-legacy switch is proved below. |
| `0x1803c15c0` | `D`. Splits candidate URLs using `0x3b`, the semicolon. A successful `0x1803babe0` stops the loop. Only error `0x800b1004` permits the next candidate. Another error or the disconnect flag ends the attempt. This matches macOS `0x122780`; it is endpoint retry, not provider or authentication fallback. |
| `0x1803b1880` | `D`, 386-415. `doRtspSetup` serializes a vector of transport descriptors with stride `0x50`, calls the RTSP session, accepts status 200, and parses the returned Transport string via `0x180486020`. It brackets the request with `0x1803ae4e0(...,20000,20000)` and `0x1803ae570`; those helpers need decoding before naming both values as network timeouts. |
| `0x1803b63f0` | `D`, 246-286. Legacy video uses a dedicated `NvSocketUdp`. Config `+0x202` selects reserved binding. The fallback reserved port is `0xbf6d`, decimal 49005, overridable by config `+0x20a`. After failure, only config `+0x203 == 1` retries setup without the explicit reserved address. Dynamic fallback is gated, not universal. |

The SETUP descriptor is `0x50` bytes. `0x180486bf0` proves a standard-vs-NVIDIA port-name flag at `+0`, multicast flag at `+1`, server port at `+2`, server/source address string at `+8`, client port at `+0x28`, and client/destination address string at `+0x30`. It emits `unicast` or `multicast`, client and server port pairs `p-p+1`, `X-GS-ClientPort`/`X-GS-ServerPort` when the first flag is zero, or `client_port`/`server_port` otherwise. Address suffixes are `destination` and `source`.

`0x180486020` splits response transport alternatives on comma and calls `0x180486300` for each. Invalid alternatives are logged and skipped. `0x1803b4ce0`, prefers each response's server port/address; zero port gets the preselected configured port and server IP, while an empty source address gets the configured server IP. Thus SETUP can override the media peer, with explicit fallback when a field is absent. Exact endpoint lookup enums and multi-address preference still need deeper helpers.

`0x180486300`, recognizes both standard and NVIDIA port names and rejects parsed client/server ports below 1024. It stores `source` at descriptor `+8` and `destination` at `+0x30`. `0x18026cdc0` supplies the fallback signaling IP from the last config endpoint with internal usage 4 or 6. These are internal config usages, not CloudMatch's public usage numbers.

### WSS selection

`0x1804acd90` preserves explicit RTSP, RTSPS, WS, and WSS URLs. RTSPS and WSS set its secure flag. A scheme-less address gets an `rtsp://` prefix and a secure flag only when the passed mode equals 3. Missing or unusable URL port takes the caller's default-port argument. Both branches construct `RtspSessionPoco` through `0x1806f1710`, forwarding the secure flag and signaling mode into `0x18072d820`.

`0x18072d820`, stores secure flag at owner `+8` and signaling mode at `+0x138`, then calls `0x18072d470`. That factory selects a WebSocket object at `0x180753460` exactly when mode is 2 or 3, passing argument 0 for unencrypted WS and argument 1 for secure WSS according to the secure flag. Other modes select plain POCO RTSP constructor `0x180ccf7e0` or secure constructor `0x1808d6f70`. Thus a TLS-flagged RTSPS URL plus WebSocket mode reaches secure WSS, explaining the S6 log without assuming its URL had to begin `wss://`.

`0x18072cfa0` names mode 2 `WS` and mode 3 `WSS`. A mismatch between requested URL scheme and expected mode scheme logs a warning rather than stopping construction. `0x18072db50` installs legacy-socket connect/send/receive timeout objects only when mode is not 2 or 3. Its literal values are 6,000,000 and 20,000,000/20,000,000; interpreting the duration unit requires the POCO duration helper. The constructor likewise applies the legacy secure/plain connect operation only outside modes 2/3.

The secure branch calls `0x180758880` before constructing WSS or secure RTSP. Its indexed literals mention SSL context configuration, default CAs, and `Pinning Snoopy CA`, but its body and the POCO chain/hostname verifier were not traced. Those HTTPS/TLS details remain unknown. They are separate from the proved DTLS SHA-256 fingerprint gate below. No static WSS-selection discrepancy is established against OpenNOW's S6-compatible WSS path.

### Socket modes

`0x1803b7300`, initial lines 156-169, 461-483, 636-655, uses the following config bytes from the same config object:

| Offset | Branch meaning |
| --- | --- |
| `+0x2cdb` | Nonzero skips Mjolnir audio SETUP. |
| `+0x2cdc` | Nonzero skips Mjolnir microphone SETUP. |
| `+0x2cd9` | RTC-compliant video. Nonzero skips both native-video SETUP branches. |
| `+0x2cde` | Unified native-video mode request. |
| `+0x3cf5` | Video stream count. More than one forces the dedicated-socket path. |
| `+0x2cd1` | RTC data-channel request. Nonzero skips legacy control setup. |

For non-RTC video, enabled unified mode and at most one stream select `0x1803b5b60`. Otherwise `0x1803b63f0` prepares dedicated video sockets. `0x1803b5b60`, initial lines 84-101, 214-217, 274-285, uses port helper `0x18026cc80`, logs that value as the WebRtcTransport port, passes it in each SETUP client-port field, and creates placeholder RTP sources. It does not bind a live dedicated video socket on this path. `0x18026cc80` searches up to twenty config endpoint entries of stride `0x20c` for category 2 and usage 7, returning that entry's port or fallback `0xbb81`, decimal 48001. Mapping those numeric endpoint enums to client/server names needs the configuration owner, so do not infer them from this helper alone.

`0x1803c1f10`, lines 213-248 and 271-308, repeats the unified eligibility check and creates `MjolnirVideoReceiver` early only for eligible unified native video. `0x18024b570`, initial lines 752-823, then attaches the existing `WebRtcTransport` to the native `NvstRtpStreamReceiver`; it explicitly ignores unified mode for RTC-compliant video. A WebRTC-named owner therefore does not imply RTC packetization.

These branches match macOS `setupSessionStreams 0x1145c0`, unified setup `0x1130f0`, and transport setup `0x11fcd0`. No platform difference is established here. The S6 Windows log's dedicated video plus bundled audio/mic/data is one selected mode, not the only supported mode.

### ICE and transport readiness

`0x1803c1f10`, constructs a fallback DTLS peer address from configured endpoint data when `SignalingHandler +0x590 == 0` and disables ICE on that branch. Otherwise it parses the server transport string and installs the addresses through `0x18036a430`. Near lines 473-483 it copies the supplied fingerprint string into the DTLS owner and calls `0x18038c540` with the local/remote STUN credential fields before starting `WebRtcTransportThread`. Candidate priority, relay selection, nomination, and the exact transport-string address precedence are not yet proved.

`0x18038fdd0`, initializes SRTP through `0x18046d480`, optionally initializes a separate Mjolnir SRTP context, and only then sets readiness and publishes event type `0x1b`. Failure reports `0x80120006`, without the ready path. Lines 53-68 enable STUN consent only when its mode field is zero and its consent owner exists, with a 5-second period. This matches the macOS completion path `0x1bcda0`. It is not a universal 5-second keepalive rule.

## DTLS and SRTP

`0x18043b4f0`, requires a nonempty local private key and certificate, loads both into an OpenSSL context, and checks that the key matches. It installs cipher list `DEFAULT:!NULL:!aNULL:!SHA256:!SHA384:!aECDH:!AESGCM+AES256:!aPSK:!CHACHA20`, followed by SRTP offer order AES-256-GCM, AES-128-GCM, AES128-CM-SHA1-80. Certificate, key, key-check, and SRTP-extension failures invoke callbacks `0x80120008`, `09`, `0a`, and `0b`, respectively. The verify-registration call uses mode 7 and callback `0x18043a820`. The parent's raw disassembly proves this tiny callback returns 1 unconditionally, with return instruction at `0x18043a825`. Trust is enforced by the subsequent fingerprint gate, not a conventional DTLS certificate-chain acceptance callback.

### Peer identity and exported keys

`0x18043ba90` is the completion/info callback. The peer identity check precedes SRTP export:

1. An empty expected-fingerprint string at DTLS owner `+0xd8`, length `+0xe8`, logs `DTLS handshake completed without certificate` and returns. That diagnostic does not prove the peer has no certificate. This branch neither exports keys nor invokes SRTP/readiness.
2. It obtains the peer certificate via `0x1808f4250`, formats its fingerprint through `0x18048d080`, and compares the expected and actual strings with equal length and `memcmp`. Failed formatting or different bytes produces `0x80120002`. A missing peer certificate produces `0x8012000c`.
3. Only a matching fingerprint reaches the selected-SRTP-profile check and exporter. Missing profile produces `0x80120003`, unsupported profile `0x80120004`, and exporter failure `0x80120005`. A close-alert branch records `0x8012000d`. The callback stores errors at owner `+0x27c`; `0x18043bff0` dispatches this pending error rather than application data.

`0x18048d080`, hashes the certificate through `0x1809d3760` using digest factory `0x1809cff10`, then formats colon-separated, zero-padded, uppercase hexadecimal bytes. The factory returns descriptor `0x18105fd00`. The raw descriptor read gives leading dwords 672,668,32,8,1,0. Local `/usr/include/openssl/obj_mac.h` confirms `NID_sha256=672` and `NID_sha256WithRSAEncryption=668`. This proves a SHA-256 certificate fingerprint, not a public-key-only pin.

| Accepted profile ID | Key bytes per direction | Salt bytes per direction | Export length |
| --- | --- | --- | --- |
| 1 | 16 | 14 | 60 |
| 7 | 16 | 12 | 56 |
| 8 | 32 | 12 | 88 |

At `0x18043ba90`, the exporter label is `EXTRACTOR-dtls_srtp`, length 19, with no context. The exported buffer layout is `K0 || K1 || S0 || S1`. The callback assembles contiguous `K0 || S0` and `K1 || S1` buffers. Role field `+0x108 == 1` passes them in that order, while the other role passes them swapped. `0x18046d480` consumes callback argument 2 as inbound and argument 3 as outbound material. Naming role 1 client or server needs its constructor, although the exporter follows the standard client/server split. No key bytes are reproduced here.

The macOS findings at `0x19e400`, `0x19ec00`, and `0x1bcda0` describe the same offer order, accepted IDs, lengths, fingerprint-before-export gate, absent-expected-fingerprint return, and directional swap. No Windows/macOS security difference is established by these paths.

### Retransmission and replay

`0x18043b1f0` increments its timeout retry counter and calls the DTLS timeout-control operation with command `0x4a`. A negative result reports terminal `0x80120001`; otherwise it calls `0x18043b360` to reschedule. The latter asks OpenSSL for its timeout using command `0x49`, arms the timer with that result, or cancels the timer when no timeout remains. The visible code does not impose a local fixed retry count or fixed 100ms retransmission interval. Retry limits may belong to OpenSSL or a higher-level deadline.

`0x18046d480` constructs both RTP and RTCP policies using `0x1808527b0` and `0x1808526e0`. It creates an inbound context with selector 2 and an outbound context with selector 3 through `0x1808522a0`. Both policy structures get the caller's 16-bit replay-window parameter and adjacent 32-bit value 1. The layout is consistent with libsrtp `window_size` and `allow_repeat_tx`, but those names remain inferred until the library consumer is checked. The window value is supplied by the caller, not fixed to 1024 in this initializer. The Windows log reports `runtime.srtpReplayWindowSize=1024`.

`0x1808527b0` and `0x1808526e0` prove the crypto policy mapping. Profile 1 uses cipher ID 1, 30 key+salt bytes, SHA1 auth ID 3, 20 auth-key bytes, and 10 tag bytes. Profile 7 uses cipher ID 6, 28 key+salt bytes, and 16 tag bytes; profile 8 uses cipher ID 7, 44 key+salt bytes, and 16 tag bytes. The DTLS callback accepts only 1/7/8 even though these library policy helpers also implement 2 and 5. `0x1808522a0` confirms selectors 2/3 construct inbound/outbound wildcard templates. Replay-field consumption is delegated deeper, so `allow_repeat_tx` remains a layout inference rather than a proved receiver behavior.

`0x18043bff0`, feeds the packet to the DTLS BIO, calls SSL-read under a lock into a 2048-byte buffer, reports a pending callback error before delivering application data, treats SSL error codes 2 and 3 as retryable, and forwards positive application data to the registered receiver. This is DTLS application-data delivery, not an SCTP framing definition.

## SCTP channel construction

`0x18034b990` builds the control channels in sequence and stores the IDs returned by the channel controller. No fixed numeric stream IDs are assigned in this function.

| Creation order | Label | Stored ID offset | Visible config |
| --- | --- | --- | --- |
| 1 | `control_channel_reliable` | owner `+0x270` | Ordered byte 1, both retry fields zero. |
| 2 | `custom_message_on_sctp_private_reliable` | `+0x272` | Same config. |
| 3 | `custom_message_on_sctp_private_partially_reliable` | `+0x274` | Ordered byte 1. Maximum retransmissions become configured byte at `config+0x27c`, minus 1. |
| 4 | `control_channel_partially_reliable` | `+0x278` | Retains ordered byte 1 and the retransmission count, rather than a lifetime. Priority enum changes from 2 to 0. |
| 5 | `control_channel_unreliable` | `+0x276` | Unordered, retransmission-count field 1, lifetime zero. Its name does not mean a zero-retry DCEP OPEN. |
| Conditional | `rtcp_on_sctp_private` | output parameter | Requires config bytes `+0x2cd9`, `+0x2cd1`, and `+0x2cda` all nonzero. Ordered and uses the configured retransmission count. |
| Conditional | `input_channel_partially_reliable` | `+0x27a` | Requires a nonzero first video-stream setting at stream child `+0x160`, then `+0xc`. Ordered byte 0, config dword `+4` gets that setting, config dword `+8` zero. The log explicitly names `maxRetransmitTime`. |

The config layout is proved jointly by constructor `0x18034b990`, serializer `0x180423790`, and parser `0x180423030`: ordered byte at `+0`, maximum lifetime at `+4`, maximum retransmission count at `+8`, protocol string at `+0x10`, and priority enum at `+0x30`. A nonzero count takes precedence over lifetime. Zero count and zero lifetime select full reliability. Priority enum 0/1/2/3 emits 128/256/512/1024. Thus the first three channels advertise priority 512 and the remaining channels above advertise 128. The protocol string is `control`, including input partial.

The channel-creation wait's deadline is 6,000,000,000 clock ticks after the start, `0x18034b990` lines 582-595. `0x18043ba90` uses the same clock helper and divides elapsed ticks by 1,000,000 for its millisecond metric, so this is a six-second deadline. The decompiler's `short * + 3000000000` expression scales by two bytes.

The existing S6 Windows log records actual SIDs 0,2,4,6,8,10 for reliable control, reliable custom, partial custom, partial control, unreliable control, and partial input. These are observed allocation results. Static universal SID parity, cursor placement, and optional RTCP SID still require the allocator.

### DCEP and application framing

`0x180424d20` sends an OPEN for creator role 0 through `0x180423790`; acceptor role 1 sends an ACK with byte `0x02`. `0x180422fa0` recognizes OPEN byte `0x03`. `0x1804250e0` recognizes explicit ACK byte `0x02` for internal message kind 2, while also accepting the first non-control message as an implicit creation acknowledgment. Kind 0 or 1 is delivered to the application queue. These kinds are internal enums, not proved wire PPIDs.

`0x180423790` emits this DCEP OPEN, and `0x180423030` parses the same layout. Multi-byte fields pass through the Windows host/network conversion helpers `Ordinal_8/9` and `Ordinal_14/15`.

| Wire offset | Field |
| --- | --- |
| 0 | Message type 3. |
| 1 | Channel type: 0 reliable, 1 retransmission-count limited, 2 lifetime limited; add `0x80` for unordered. |
| 2 | Big-endian 16-bit priority. |
| 4 | Big-endian 32-bit count or lifetime, selected by type. |
| 8 | Big-endian 16-bit label byte length. |
| 10 | Big-endian 16-bit protocol byte length. |
| 12 | Label bytes, followed by protocol bytes, without terminators. |

In particular, the unordered control-unreliable OPEN has type `0x81`, parameter 1. The partial-control OPEN has type 1 when its configured count is nonzero, not type `0x82` with 300ms. If configured count becomes zero, it instead advertises full reliable type 0. This is exact serializer behavior, not inferred from labels.

`0x1804253d0` enqueues raw message bytes plus two metadata dwords. It rejects an enqueue with return 2 when adding its byte length would exceed the configured queue-byte capacity. It adds no framing header itself. `0x180425d10` sends DCEP with internal kind 2; blocked sends enter its retry queue.

`0x1804297c0` proves PPID mapping: internal kind 0 maps to text PPID 51, kind 1 to binary PPID 53, and kind 2 to DCEP PPID 50. Empty text/binary messages use PPIDs 56/57 and a single zero byte. It rejects closed sockets, payload lengths above the socket's supported maximum, and null pointers with nonzero length. Socket send result 3 becomes blocked result 2. `0x180425580` selects one of three stored send-option blocks at owner `+0x98`, `+0xc0`, or `+0xe8` according to its per-message mode, then forwards it through `0x1803757f0` to the SCTP socket. Therefore DCEP OPEN reliability does not prove every application packet's retransmission behavior.

Application command framing has two distinct owners. `0x18034dcf0` constructs an internal four-byte header, little-endian `u16 command || u16 payload_length`, then payload. `0x18034da50` asks the virtual prefix helper how many header bytes to transmit. Base `ServerControlReliableUdp` uses `0x18034da40`, which retains only the command, and its receive path `0x18034d0d0` reconstructs length from the SCTP message boundary. `ServerControlReliableUdpAggregated` uses `0x1803d7790`, which retains all four bytes; `0x1803d72d0` iterates commands with explicit lengths inside one message. The vtables `0x18121fa58` and `0x181221080` prove these overrides. S6 selected the aggregated owner, so its four-byte framing matches OpenNOW's `control_command` builder, not the two-byte base form. Commands `0x10a` and `0x113` sent on non-reliable-control lanes bypass this wrapper in `0x18034dcf0`.

## Comparison with OpenNOW dev `7ce3cd54`

Source citations use `SC` for `native/opennow-streamer/crates/opennow-streamer-core/src`, `NT` for the transport crate's `src`, and `V` for `native/opennow-streamer/vendor/str0m/src`. All are read from the frozen commit, not the shared working tree.

- Baseline routing already supports the usage-14-only NVIDIA shape by using SETUP's video peer for the bundle when no explicit BUNDLE mapping exists. See `SC/nvst_rtsp.rs:1128-1166`. Do not label the absent CloudMatch media endpoint a new defect. Windows' SETUP-response source/port override and configured-field fallback are now directly proved above.
- OpenNOW's production bundle requires a remote fingerprint and ICE credentials, parses the fingerprint, installs it, and starts DTLS as client in `NT/nvst.rs:6079-6119`. `V/lib.rs:1660-1680` computes SHA-256 from the peer certificate and disconnects on a mismatch or a missing expected fingerprint. The disabled-verification probe is a separate binary, not the production path.
- `V/crypto/srtp.rs:9-38` offers AES-256-GCM, AES-128-GCM, then AES128-CM-SHA1-80, with key/salt sizes 32/12, 16/12, and 16/14. `V/rtp/srtp.rs:731-749` splits `K0 || K1 || S0 || S1` in the same way. These match the macOS and Windows decompiles. Native Mjolnir video's separate configurable crypto is not the same context as DTLS-derived bundle crypto.
- The native video replay bitmap is 2048 packets, `NT/nvst.rs:213,221-222,3019-3076`, rather than the Windows log's configured 1024. It rejects duplicates and ages beyond its window. This is a proved policy difference, not proof of a protocol incompatibility or disabled replay protection. Bundle audio uses str0m's separate SRTP owner; do not attribute the native-video window to every stream.
- `NT/nvst_input.rs:70-134,298-313` specifies ordered reliable/custom reliable/custom partial, unordered 300ms control partial and input partial, unordered zero-retry control unreliable, and optional reliable cursor/RTCP. Windows instead advertises ordered count-limited partial control and unordered control-unreliable with count 1. Windows RTCP is conditional on RTC video/data/RTCP settings and count-limited, while OpenNOW creates reliable RTCP when bundle video is enabled. These are proved static configuration differences, not measured faults. Their runtime effect remains untested.
- OpenNOW's profile includes SID values 0/2/4/6/8/10/12/14, but `channel_config` passes `negotiated:None` and does not use the `sid` field. Production allocation remains with str0m. The S6 observed IDs agree for the first six labels, but optional-channel allocation is not proved equivalent. OpenNOW advertises an empty protocol string and DCEP priority 0, `V/sctp/mod.rs:1318`; Windows advertises protocol `control` with priorities 512 or 128. These are further wire differences, not evidence that NVIDIA rejects OpenNOW's channels.
