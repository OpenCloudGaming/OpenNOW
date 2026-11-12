# Windows CloudMatch logged exchange and native model

Source: Bifrost's `[BifrostClient: GridServer]` lines in `geronimo.log.bak`. These are the only CloudMatch JSON bodies in the upload, and they all belong to session S6 (2026-10-12 19:45 local). Bifrost logs the full request (`Serialized session request. Size: 1957, Data: '{...}'`) and every response (`Client Request: 11, server response: '{...}'`). Redacted copies are in `extracted/cloudmatch_request.json`, `cloudmatch_response_first.json` and `cloudmatch_response_ready.json`. The flattened catalog is `extracted/cloudmatch_fields.tsv`. Evidence tags follow COMMON.md. All rows below are **L** unless they say otherwise.

## Exchange shape

| step | method + URL | body | timing |
|---|---|---|---|
| create | POST (**D**, Bifrost `0x1807d00d0`) `https://eu-netherlands-north.cloudmatchbeta.nvidiagrid.net:443/v2/session?keyboardLayout=en-US&languageCode=en_US` | `{"sessionRequestData":{...}}` (1957 bytes) | T0 = 19:45:11.639 |
| create response | none | `{"session":{...},"requestStatus":{...},"otherUserSessions":[...]}` | +420 ms |
| poll x18 | GET (**D**, Bifrost `0x1807cc4f0`) `https://np-ams-08.cloudmatchbeta.nvidiagrid.net:443/v2/session/<sessionId>` | same response shape, without `otherUserSessions` content | about every 1.02 s |
| ready | none | `session.status 2` | +17.78 s |

The poll host/port comes from `sessionControlInfo`, but the native poller reconstructs `/v2/session/<redacted>` from its fixed template and cached session ID; it does not consume `sessionControlInfo.resourcePath` for this path (**D**, `0x1807d00d0`, `0x1807cc4f0`). The method is statically POST for creation and GET for polling. The HTTP helper adds `Content-Type: text/plain` and the nonempty authorization header (`0x1807eed10`, `0x1807ee9a0`). No credential values are included. See [cloudmatch-native.md](cloudmatch-native.md) for typed field registrations, ownership and usage conversions.

State progression across the 19 responses: `status 1, seatSetupStep 0, ETA 30000` (first two), then `status 1, seatSetupStep 3, ETA 20000` (16), then `status 2` (ready). `errorCode` is 1 and `requestStatus.statusCode` is 1 (`SUCCESS_STATUS`) throughout. `queuePosition` stayed 0, so there was no queue on this priority account. `gpuType` is `""` until the ready response, which carries `"5080h / B40"`.

## connectionInfo and sessionControlInfo usage numbers

| object | usage | Bifrost name | protocol | appLevelProtocol | port | resourcePath | ip |
|---|---|---|---|---|---|---|---|
| `sessionControlInfo` | 10 | session control (no NVB_PU name logged) | 0 | 5 | 443 | `/v2/session/<sessionId>` | `np-ams-08.cloudmatchbeta.nvidiagrid.net` |
| `connectionInfo[0]` | 14 | `NVB_PU_RTSPS` | 1 | 6 | 322 | null until step 3, then `rtsps://<redacted-ip>.cloudmatchbeta.nvidiagrid.net:322` | null |
| `connectionInfo[1]` | 14 | `NVB_PU_RTSPS` | 1 | 6 | 48322 | null until step 3, then `rtsps://<redacted-ip>.cloudmatchbeta.nvidiagrid.net:48322` | null |

- **L**: `ConnectionInfo[0]: address: rtsps://<redacted-ip>.cloudmatchbeta.nvidiagrid.net:322, port: 322, usage: NVB_PU_RTSPS` (19:45:29.416).
- **L**: `connectionInfoCount is 2 but maxNumOfPorts is 20. Ignoring the remainder.` (19:45:29.423). This message is misleading because nothing was ignored. **I**: it fires whenever count != max.
- Usage 2 (VIDEO), 15 (MEDIA), 16, and 17 (BUNDLE) **did not appear** on this NVIDIA seat. The usage-to-port-type map from macOS Bifrost (PR #1161/#1162: 2 -> VIDEO, 17 -> BUNDLE, 14/16 -> RTSPS, 15 skipped) cannot be checked against Windows logs beyond usage 14 -> `NVB_PU_RTSPS`.
- The final NVSC config records these two as `general.serverEndpoints[0] = 322 4 6` and `[1] = 48322 4 6` (**L** config dump). **I**: the fields are port, an NVB port-type value 4 (RTSPS), and appLevelProtocol 6.
- The client connected only to port 322 (`Connecting to host ..., port 322`). Port 48322 was not used. **I**: it is an alternate RTSPS port. The media port came from the RTSP SETUP `Transport` header (`X-GS-ServerPort=5004-5005`), not from CloudMatch.

## Request fields observed

Monitor, HDR, and codec-related fields. Values are from S6.

- `clientRequestMonitorSettings[0]`: 2560x1440@120, `dpi 100`, `sdrHdrMode 0`, `displayData null`, `hdr10PlusGamingData null`.
- Top-level `sdrHdrMode 0`, `clientDisplayHdrCapabilities null`.
- `requestedStreamingFeatures`: `bitDepth 1` and `chromaFormat 1`, which is 10-bit 4:4:4, confirmed by the decoder (`format 0x65` = Y410, `chroma format changed to: 3`). Also `reflex true`, `enabledL4S false`, `cloudGsync false`, `trueHdr false`, `profile 0`, `prefilterMode 0`, `hudStreamingMode 0`, `qosPolicy 0`, `touchSupport false`, and `dlssOverrides {srMode 0, srModel 0, srScale 100, rrModel 0, fgMode 0, fgModel 0, fgMultiplier 0}`.
- The codec is **not** in the CloudMatch request. Bifrost picks it locally. It logged `Selected HEVC codec from client preferences` at RTSP time.
- `metaData[]` keys: `ClientImeSupport`=`0`, `SubSessionId`=`<uuid>`, `clientPhysicalResolution`=`{"horizontalPixels":2560,"verticalPixels":1440}`, `networkType`=`Ethernet`, `wssignaling`=`1`, `surroundAudioInfo`=`2` (top-level `surroundAudioInfo` stays 0).

## Response-only fields

- `finalizedStreamingFeatures` is null until ready, then `{enabledL4S false, profile 0, bitDepth 1, prefilterMode 2, prefilterSharpness 0, reflex false, mouseMovementFlags 0, chromaFormat 1, trueHdr false, hudStreamingMode 0, cloudGsync false, prefilterNoiseReduction 0}`. **Reflex was requested but finalized false**. The server offered prefilter mode 2, and Bifrost finalized 0 (`Prefilter:: Client request: Mode: 0 ... Server response: Mode: 2 ... Finalized Mode: 0`).
- `monitorSettings` is null until ready, then the same monitor with `dpi 0`.
- `sessionAds null`, `sessionAdsRequired false`, `adProvider 1`. JS also logged `queue: ... isAdsRequired=False`.
- `userIdleWarningTimeoutInMs 0`. The idle warning came over NVST control instead (timer type 4, 60 s).
- `session.sessionRequestData` echoes the request. Differences: `useOps false`, `enablePersistingInGameSettings false`, `preferredController 0`, `audioMode 0`, `userAge 0`, `externalAppId` as a string, `partnerCustomData "***"`, `networkTestSessionId` filled, and `requestedStreamingFeatures null`.
- `otherUserSessions[]` on the first response holds the same pending session. **I**: the server lists all of the user's live sessions there.

## Full field catalog

Generated from `extracted/cloudmatch_fields.tsv` by `render.py`. Secrets are replaced with `<redacted>`, `<redacted-uuid>`, `<redacted-ip>` or `<redacted-b64>`.

<!-- GEN:cloudmatch_fields -->
| dir | field path | type | example (redacted) | use |
|---|---|---|---|---|
| request | `sessionRequestData.appId` | int | 106275040 | CMS app id of the title to launch |
| request | `sessionRequestData.externalAppId` | null | null | store-side app id; null in request, echoed as string in response |
| request | `sessionRequestData.internalTitle` | null | null | internal title name (null) |
| request | `sessionRequestData.availableSupportedControllers[]` | int | 2 | controller types the client supports (2) |
| request | `sessionRequestData.preferredController` | int | 2 | preferred controller type (2); response echoes 0 |
| request | `sessionRequestData.networkTestSessionId` | null | null | id of the pre-launch network test; null in request, filled in response |
| request | `sessionRequestData.parentSessionId` | null | null | parent session for sub-sessions (null) |
| request | `sessionRequestData.clientIdentification` | string | GFN-PC | client family string |
| request | `sessionRequestData.deviceHashId` | string | <redacted> | stable device id (secret) |
| request | `sessionRequestData.clientVersion` | string | 30.0 | CloudMatch client API version |
| request | `sessionRequestData.sdkVersion` | string | 2.0 | Bifrost SDK version |
| request | `sessionRequestData.streamerVersion` | string | 14 | NVST streamer protocol major version (14 => RTSPS/WSS path) |
| request | `sessionRequestData.clientPlatformName` | string | Windows | OS name |
| request | `sessionRequestData.clientRequestMonitorSettings[].monitorId` | int | 0 | monitor index |
| request | `sessionRequestData.clientRequestMonitorSettings[].positionX` | int | 0 | monitor origin X |
| request | `sessionRequestData.clientRequestMonitorSettings[].positionY` | int | 0 | monitor origin Y |
| request | `sessionRequestData.clientRequestMonitorSettings[].widthInPixels` | int | 2560 | requested stream width |
| request | `sessionRequestData.clientRequestMonitorSettings[].heightInPixels` | int | 1440 | requested stream height |
| request | `sessionRequestData.clientRequestMonitorSettings[].dpi` | int | 100 | monitor DPI percent (100 on Windows); response monitorSettings echoes 0 |
| request | `sessionRequestData.clientRequestMonitorSettings[].framesPerSecond` | int | 120 | requested fps |
| request | `sessionRequestData.clientRequestMonitorSettings[].sdrHdrMode` | int | 0 | 0 = SDR, 1 = HDR |
| request | `sessionRequestData.clientRequestMonitorSettings[].displayData` | null | null | HDR display metadata (null for SDR) |
| request | `sessionRequestData.clientRequestMonitorSettings[].hdr10PlusGamingData` | null | null | HDR10+ gaming metadata (null) |
| request | `sessionRequestData.useOps` | bool | true | request OPS (true in request, false echoed) |
| request | `sessionRequestData.audioMode` | int | 2 | 2 = stereo per Geronimo 'AudioMode used for Start stream session: 2' |
| request | `sessionRequestData.metaData[].key` | string | ClientImeSupport \| "SubSessionId" \| "clientPhysicalResolution" \| "networkType" |  |
| request | `sessionRequestData.metaData[].value` | string | 0 \| "<redacted-uuid>" \| "{\"horizontalPixels\":2560,\"verticalPixels\":1440}" \| "Ethernet" |  |
| request | `sessionRequestData.sdrHdrMode` | int | 0 | 0 = SDR, 1 = HDR |
| request | `sessionRequestData.clientDisplayHdrCapabilities` | null | null | HDR caps of the display (null for SDR) |
| request | `sessionRequestData.surroundAudioInfo` | int | 0 | top-level 0; metaData carries surroundAudioInfo=2 |
| request | `sessionRequestData.remoteControllersBitmap` | int | 1 | bitmap of attached remote controllers (1) |
| request | `sessionRequestData.clientTimezoneOffset` | int | 7200000 | ms offset from UTC (UTC+2 here) |
| request | `sessionRequestData.enhancedStreamMode` | int | 0 | 0 on NVIDIA seats |
| request | `sessionRequestData.appLaunchMode` | int | 1 | 1 = DEFAULT ('App Launch Mode: DEFAULT') |
| request | `sessionRequestData.secureRTSPSupported` | bool | true | client can do RTSPS |
| request | `sessionRequestData.partnerCustomData` | null | null | partner blob; null in request, '***' masked in response |
| request | `sessionRequestData.accountLinked` | bool | false | store account linked |
| request | `sessionRequestData.enablePersistingInGameSettings` | bool | true | persist in-game settings (true in request, false echoed) |
| request | `sessionRequestData.requestedAudioFormat` | int | 0 | 0 = default |
| request | `sessionRequestData.userAge` | int | <redacted> | account holder age (PII, redacted) |
| request | `sessionRequestData.requestedStreamingFeatures.reflex` | bool | true | request NVIDIA Reflex |
| request | `sessionRequestData.requestedStreamingFeatures.bitDepth` | int | 1 | 0 = 8-bit, 1 = 10-bit |
| request | `sessionRequestData.requestedStreamingFeatures.cloudGsync` | bool | false | cloud G-SYNC / VRR |
| request | `sessionRequestData.requestedStreamingFeatures.enabledL4S` | bool | false | L4S ECN congestion control |
| request | `sessionRequestData.requestedStreamingFeatures.mouseMovementFlags` | int | 0 | mouse movement mode flags |
| request | `sessionRequestData.requestedStreamingFeatures.trueHdr` | bool | false | RTX HDR (false) |
| request | `sessionRequestData.requestedStreamingFeatures.supportedHidDevices` | int | 0 | HID device bitmap (0) |
| request | `sessionRequestData.requestedStreamingFeatures.profile` | int | 0 | streaming profile id (0) |
| request | `sessionRequestData.requestedStreamingFeatures.fallbackToLogicalResolution` | bool | false | allow logical-resolution fallback |
| request | `sessionRequestData.requestedStreamingFeatures.hidDevices` | null | null | explicit HID list (null) |
| request | `sessionRequestData.requestedStreamingFeatures.chromaFormat` | int | 1 | 0 = 4:2:0, 1 = 4:4:4 (S6 decoded as 4:4:4, DXGI 0x65) |
| request | `sessionRequestData.requestedStreamingFeatures.prefilterMode` | int | 0 | encoder prefilter mode; server proposed 2, client finalized 0 |
| request | `sessionRequestData.requestedStreamingFeatures.prefilterSharpness` | int | 0 | prefilter sharpness |
| request | `sessionRequestData.requestedStreamingFeatures.prefilterNoiseReduction` | int | 0 | prefilter denoise |
| request | `sessionRequestData.requestedStreamingFeatures.hudStreamingMode` | int | 0 | HUD streaming mode |
| request | `sessionRequestData.requestedStreamingFeatures.qosPolicy` | int | 0 | QoS policy (0) |
| request | `sessionRequestData.requestedStreamingFeatures.touchSupport` | bool | false | touch input support |
| request | `sessionRequestData.requestedStreamingFeatures.dlssOverrides.srMode` | int | 0 | DLSS super-resolution override |
| request | `sessionRequestData.requestedStreamingFeatures.dlssOverrides.srModel` | int | 0 | DLSS SR model override |
| request | `sessionRequestData.requestedStreamingFeatures.dlssOverrides.srScale` | int | 100 | DLSS SR scale percent |
| request | `sessionRequestData.requestedStreamingFeatures.dlssOverrides.rrModel` | int | 0 | DLSS ray-reconstruction model |
| request | `sessionRequestData.requestedStreamingFeatures.dlssOverrides.fgMode` | int | 0 | DLSS frame-generation mode |
| request | `sessionRequestData.requestedStreamingFeatures.dlssOverrides.fgModel` | int | 0 | DLSS FG model |
| request | `sessionRequestData.requestedStreamingFeatures.dlssOverrides.fgMultiplier` | int | 0 | DLSS FG multiplier |
| request | `sessionRequestData.transport` | null | null | transport override (null) |
| response | `session.sessionAds` | null | null | ad payload (null) |
| response | `session.sessionControlInfo.protocol` | int | 0 | transport protocol enum (0 for control HTTPS, 1 for RTSPS) |
| response | `session.sessionControlInfo.appLevelProtocol` | int | 5 | 5 = CloudMatch HTTPS session control, 6 = RTSPS |
| response | `session.sessionControlInfo.ip` | string | np-ams-08.cloudmatchbeta.nvidiagrid.net | host; sessionControlInfo.ip is the per-zone CloudMatch host, connectionInfo.ip is null |
| response | `session.sessionControlInfo.resourcePath` | string | /v2/session/<redacted-uuid> | session-control path, or rtsps:// URL once the seat is assigned (null before) |
| response | `session.sessionControlInfo.usage` | int | 10 | JSON port usage: 10 = session control, 14 = SIGNALING; native RTSPS is 10 |
| response | `session.sessionControlInfo.port` | int | 443 | TCP port |
| response | `session.connectionInfo[].protocol` | int | 1 | transport protocol enum (0 for control HTTPS, 1 for RTSPS) |
| response | `session.connectionInfo[].appLevelProtocol` | int | 6 | 5 = CloudMatch HTTPS session control, 6 = RTSPS |
| response | `session.connectionInfo[].ip` | null | null | host; sessionControlInfo.ip is the per-zone CloudMatch host, connectionInfo.ip is null |
| response | `session.connectionInfo[].resourcePath` | null,string | null \| "rtsps://<redacted-ip>.cloudmatchbeta.nvidiagrid.net:322" \| "rtsps://<redacted-ip>. | session-control path, or rtsps:// URL once the seat is assigned (null before) |
| response | `session.connectionInfo[].usage` | int | 14 | JSON port usage: 10 = session control, 14 = SIGNALING; native RTSPS is 10 |
| response | `session.connectionInfo[].port` | int | 322 \| 48322 | TCP port |
| response | `session.sessionRequestData.audioMode` | int | 0 | 2 = stereo per Geronimo 'AudioMode used for Start stream session: 2' |
| response | `session.sessionRequestData.clientDisplayHdrCapabilities` | null | null | HDR caps of the display (null for SDR) |
| response | `session.sessionRequestData.remoteControllersBitmap` | int | 1 | bitmap of attached remote controllers (1) |
| response | `session.sessionRequestData.sdrHdrMode` | int | 0 | 0 = SDR, 1 = HDR |
| response | `session.sessionRequestData.networkTestSessionId` | string | <redacted> | id of the pre-launch network test; null in request, filled in response |
| response | `session.sessionRequestData.transport` | null | null | transport override (null) |
| response | `session.sessionRequestData.availableSupportedControllers` | null | null |  |
| response | `session.sessionRequestData.clientVersion` | string | 30.0 | CloudMatch client API version |
| response | `session.sessionRequestData.partnerCustomData` | string | *** | partner blob; null in request, '***' masked in response |
| response | `session.sessionRequestData.deviceHashId` | string | <redacted> | stable device id (secret) |
| response | `session.sessionRequestData.userAge` | int | <redacted> | account holder age (PII, redacted) |
| response | `session.sessionRequestData.internalTitle` | null | null | internal title name (null) |
| response | `session.sessionRequestData.clientPlatformName` | string | Windows | OS name |
| response | `session.sessionRequestData.rigId` | null | null | rig id (null) |
| response | `session.sessionRequestData.requestedStreamingFeatures` | null | null | echoed as null in response |
| response | `session.sessionRequestData.metaData[].key` | string | ClientImeSupport \| "SubSessionId" \| "clientPhysicalResolution" \| "networkType" |  |
| response | `session.sessionRequestData.metaData[].value` | string | 0 \| "<redacted-uuid>" \| "{\"horizontalPixels\":2560,\"verticalPixels\":1440}" \| "Ethernet" |  |
| response | `session.sessionRequestData.surroundAudioInfo` | int | 0 | top-level 0; metaData carries surroundAudioInfo=2 |
| response | `session.sessionRequestData.enablePersistingInGameSettings` | bool | false | persist in-game settings (true in request, false echoed) |
| response | `session.sessionRequestData.clientTimezoneOffset` | int | 7200000 | ms offset from UTC (UTC+2 here) |
| response | `session.sessionRequestData.preferredController` | int | 0 | preferred controller type (2); response echoes 0 |
| response | `session.sessionRequestData.clientIdentification` | string | GFN-PC | client family string |
| response | `session.sessionRequestData.parentSessionId` | null | null | parent session for sub-sessions (null) |
| response | `session.sessionRequestData.appId` | int | 106275040 | CMS app id of the title to launch |
| response | `session.sessionRequestData.externalAppId` | string | 106275040 | store-side app id; null in request, echoed as string in response |
| response | `session.sessionRequestData.streamerVersion` | string | 14 | NVST streamer protocol major version (14 => RTSPS/WSS path) |
| response | `session.sessionRequestData.clientRequestMonitorSettings[].monitorId` | int | 0 | monitor index |
| response | `session.sessionRequestData.clientRequestMonitorSettings[].positionX` | int | 0 | monitor origin X |
| response | `session.sessionRequestData.clientRequestMonitorSettings[].positionY` | int | 0 | monitor origin Y |
| response | `session.sessionRequestData.clientRequestMonitorSettings[].heightInPixels` | int | 1440 | requested stream height |
| response | `session.sessionRequestData.clientRequestMonitorSettings[].sdrHdrMode` | int | 0 | 0 = SDR, 1 = HDR |
| response | `session.sessionRequestData.clientRequestMonitorSettings[].displayData` | null | null | HDR display metadata (null for SDR) |
| response | `session.sessionRequestData.clientRequestMonitorSettings[].framesPerSecond` | int | 120 | requested fps |
| response | `session.sessionRequestData.clientRequestMonitorSettings[].hdr10PlusGamingData` | null | null | HDR10+ gaming metadata (null) |
| response | `session.sessionRequestData.clientRequestMonitorSettings[].dpi` | int | 100 | monitor DPI percent (100 on Windows); response monitorSettings echoes 0 |
| response | `session.sessionRequestData.clientRequestMonitorSettings[].widthInPixels` | int | 2560 | requested stream width |
| response | `session.sessionRequestData.appLaunchMode` | int | 1 | 1 = DEFAULT ('App Launch Mode: DEFAULT') |
| response | `session.sessionRequestData.sdkVersion` | string | 2.0 | Bifrost SDK version |
| response | `session.sessionRequestData.enhancedStreamMode` | int | 0 | 0 on NVIDIA seats |
| response | `session.sessionRequestData.useOps` | bool | false | request OPS (true in request, false echoed) |
| response | `session.sessionRequestData.secureRTSPSupported` | bool | true | client can do RTSPS |
| response | `session.sessionRequestData.accountLinked` | bool | false | store account linked |
| response | `session.sessionRequestData.requestedAudioFormat` | int | 0 | 0 = default |
| response | `session.finalSelectedScreenResolution` | null | null | null in all responses |
| response | `session.finalizedStreamingFeatures` | null | null | null until the ready response |
| response | `session.adProvider` | int | 1 | ad provider id (1) |
| response | `session.sessionAdsRequired` | bool | false | ads gate (false) |
| response | `session.errorCode` | int | 1 | 1 = success in all responses |
| response | `session.enhancedStreamMode` | int | 0 | 0 on NVIDIA seats |
| response | `session.sessionId` | string | <redacted> | CloudMatch session id (secret) |
| response | `session.seatSetupInfo.queuePosition` | int | 0 | queue position (0) |
| response | `session.seatSetupInfo.seatSetupStep` | int | 0 \| 3 | 0 = allocating, 3 = configuring seat |
| response | `session.seatSetupInfo.seatSetupEta` | int | 30000 \| 20000 | ETA in ms |
| response | `session.status` | int | 1 \| 2 | 1 = setting up, 2 = ready |
| response | `session.clientIp` | string | <redacted-ip> | client public IP (redacted) |
| response | `session.gpuType` | string |  \| "5080h / B40" | seat GPU class, empty until assigned |
| response | `session.monitorSettings` | null | null | null until ready; then the finalized monitor list |
| response | `session.userIdleWarningTimeoutInMs` | int | 0 | idle warning (0; idle timer arrived via NVST control instead) |
| response | `requestStatus.countryCode` | null | null | null |
| response | `requestStatus.unifiedErrorCode` | int | 0 | 0 = no error |
| response | `requestStatus.requestId` | string | <redacted> | per-request id (secret) |
| response | `requestStatus.serverId` | string | <redacted> | zone/server id (NP-AMS-08 style) |
| response | `requestStatus.statusDescription` | string | SUCCESS_STATUS | SUCCESS_STATUS |
| response | `requestStatus.statusCode` | int | 1 | 1 = success |
| response | `otherUserSessions[]` | array | [] |  |
| response | `session.finalizedStreamingFeatures.enabledL4S` | bool | false | L4S ECN congestion control |
| response | `session.finalizedStreamingFeatures.profile` | int | 0 | streaming profile id (0) |
| response | `session.finalizedStreamingFeatures.bitDepth` | int | 1 | 0 = 8-bit, 1 = 10-bit |
| response | `session.finalizedStreamingFeatures.prefilterMode` | int | 2 | encoder prefilter mode; server proposed 2, client finalized 0 |
| response | `session.finalizedStreamingFeatures.prefilterSharpness` | int | 0 | prefilter sharpness |
| response | `session.finalizedStreamingFeatures.reflex` | bool | false | request NVIDIA Reflex |
| response | `session.finalizedStreamingFeatures.mouseMovementFlags` | int | 0 | mouse movement mode flags |
| response | `session.finalizedStreamingFeatures.chromaFormat` | int | 1 | 0 = 4:2:0, 1 = 4:4:4 (S6 decoded as 4:4:4, DXGI 0x65) |
| response | `session.finalizedStreamingFeatures.trueHdr` | bool | false | RTX HDR (false) |
| response | `session.finalizedStreamingFeatures.hudStreamingMode` | int | 0 | HUD streaming mode |
| response | `session.finalizedStreamingFeatures.cloudGsync` | bool | false | cloud G-SYNC / VRR |
| response | `session.finalizedStreamingFeatures.prefilterNoiseReduction` | int | 0 | prefilter denoise |
| response | `session.monitorSettings[].monitorId` | int | 0 | monitor index |
| response | `session.monitorSettings[].positionX` | int | 0 | monitor origin X |
| response | `session.monitorSettings[].positionY` | int | 0 | monitor origin Y |
| response | `session.monitorSettings[].heightInPixels` | int | 1440 | requested stream height |
| response | `session.monitorSettings[].sdrHdrMode` | int | 0 | 0 = SDR, 1 = HDR |
| response | `session.monitorSettings[].displayData` | null | null | HDR display metadata (null for SDR) |
| response | `session.monitorSettings[].framesPerSecond` | int | 120 | requested fps |
| response | `session.monitorSettings[].hdr10PlusGamingData` | null | null | HDR10+ gaming metadata (null) |
| response | `session.monitorSettings[].dpi` | int | 0 | monitor DPI percent (100 on Windows); response monitorSettings echoes 0 |
| response | `session.monitorSettings[].widthInPixels` | int | 2560 | requested stream width |

`otherUserSessions[]` repeats the `session` object shape (71 paths, same values as the pending session); it is omitted above.
<!-- /GEN:cloudmatch_fields -->

## Comparison with OpenNOW (`native/opennow-core/src/cloudmatch.rs`, frozen dev `7ce3cd54`)

What matches: `clientIdentification "GFN-PC"`, `clientVersion "30.0"`, `sdkVersion "2.0"`, `streamerVersion "14"`, `useOps true`, `audioMode 2`, `secureRTSPSupported true`, `metaData` `wssignaling=1` and `surroundAudioInfo=2`, `requestedStreamingFeatures` core keys, `transport null` (`build_create_body`, cloudmatch.rs:1686-1724). The codec is also absent from the request on both sides.

What differs. These are **L** for the Windows side and read from code for the OpenNOW side.

| field | Windows 2.0.89.141 | OpenNOW | impact |
|---|---|---|---|
| `requestedStreamingFeatures.qosPolicy`, `.touchSupport`, `.dlssOverrides{...}` | sent (`0`, `false`, 7 keys) | not sent. The comment at cloudmatch.rs:1661-1666 says QoS policy and touch support are "deliberately absent" | the comment contradicts this Windows build; low risk, but not field-for-field |
| `metaData` `ClientImeSupport`, `clientPhysicalResolution`, `networkType` | sent | only `wssignaling`, `SubSessionId`, `surroundAudioInfo` (cloudmatch.rs:1656-1660) | `clientPhysicalResolution` is filtered out of resume bodies on purpose (cloudmatch.rs:1566) |
| `externalAppId` | `null` present | absent | none known |
| `remoteControllersBitmap` | 1 | 0 (1 only in alliance WebRTC mode, cloudmatch.rs:1712/1731) | **U** |
| `clientRequestMonitorSettings[].dpi` | 100 | 96 (144 on macOS) | **U** |
| `userAge` | real account value (redacted) | constant 25 (cloudmatch.rs:1721) | **U** |
| usage handling | supplied sessions return JSON usage 14 SIGNALING only; media peer from SETUP. Static conversion supports more usages | prefers usage 2/17 for media (cloudmatch.rs:1806, 2010), falls back to usage-14 rtsps -> wss (cloudmatch.rs:1990-2006) | consistent for the observed NVIDIA shape; not proof of all-provider parity |
| polling host/path | returned `sessionControlInfo.ip` and port; fixed `/v2/session/<id>`, not copied `resourcePath` | `session_server_ip` prefers `sessionControlInfo.ip` (cloudmatch.rs:2425-2440) | controlling-host policy agrees; native construction in `cloudmatch-native.md` |
