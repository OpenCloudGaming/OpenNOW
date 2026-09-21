# Session creation and CloudMatch orchestration

This note records how the official GeForce NOW Linux x86_64 Flatpak creates a cloud seat and hands it to the native streamer, then maps that path onto OpenNOW. The evidence is the unpacked Flatpak commit `235a800084abb2246e4578e5b60c33397fac73152c02a5751e663ddafd00e260`, the CEF tree under `files/cef`, string and symbol extracts under `/workspace/audit/gfn-official/extracts/`, and the OpenNOW owners `native/opennow-core/src/cloudmatch.rs`, `native/opennow-core/src/streamer.rs`, and `docs/core-protocol.md`.

The binaries are stripped ELF objects. Exported C symbols, demangled C++ symbols, and adjacent read-only strings are reliable. Integer enum values are reliable only where OpenNOW already treats them as a live CloudMatch contract. Where a string table is a contiguous `Unknown …` run, this note treats the names as the vendor vocabulary and does not invent indexes from file order.

## Build and process split

Flatpak metadata names the app `com.nvidia.geforcenow`, runtime `org.freedesktop.Platform/x86_64/24.08`, and command `GeForceNOW`. The runtime is granted network, IPC, X11, Wayland, PulseAudio, session and system buses, all devices including DRI, and the Gamescope Vulkan layer. Environment variables `ENABLE_GAMESCOPE=1`, `ENABLE_GAMESCOPE_HDR=1`, and `GAMESCOPE_HDR=1` are set in the manifest, so HDR and Gamescope are part of the packaged Linux session environment rather than a user preference.

The mall config that ships beside the shell, `files/mall/shared/assets/config/config.json`, identifies itself as official release build `2.0.84.127`. The CEF shell string table carries Chromium `128.4.13+ge76af7e+chromium-128.0.6613.138.nv27`, product path `gfn_release/2f4a4c46`, and Jenkins sources under `GFN-cx-lin-official/client/cef`. Geronimo and Bifrost debug paths both sit on Perforce branch `gs_04_87` (`/dvs/p4/build/sw/gcomp/rel/gs_04_87/...`). Geronimo is linked against the Mjolnir vcpkg snapshot dated `2025.04.09` (gRPC 1.71.0, OpenTelemetry C++ 1.16.1).

Sizes and identities:

| Object | Size | Role | Build ID |
| --- | --- | --- | --- |
| `GeForceNOW` | 2.4 MB | CEF shell, links `libcef.so`, `libGeronimo.so`, `libBifrost2.so`, SDL2 | `dad4d41dc959f65e69238bc158fc26fc04ab5d6d` |
| `GeForceNOWContainer` | 861 KB | Separate PIE helper; dynamic section does not list Bifrost or Geronimo | xxHash `7babb5688f7eb958` |
| `libBifrost2.so` | 19 MB | CloudMatch and NVST session SDK, SONAME `libBifrost2.so` | `fa3685038bd71962fe30ad09482bcb0721a54f35` |
| `libGeronimo.so` | 33 MB | Streamer, decode, input, `GridApp`, SONAME `libGeronimo.so`, NEEDED `libBifrost2.so` | `15d0eebc08da503f1f37ea9cae2dbac1d760fea4` |
| `libcef.so` | 220 MB | Chromium 128 embedder | — |

`GeForceNOW` does not talk to CloudMatch itself. It hosts the mall at a relative URL, receives JavaScript queries, and forwards them into `GridApp`. Source paths burned into the shell make that boundary explicit: `client/cef/src/gfn/simple_grid_app.cc`, `simple_grid_app.h`, `gfn_query_handler.cc`, `gfn_sdk_connector.cc`, `gfn/serenity/serenity.cc`, and `gfn/serenity/rtsp_handler.cc`. Shell logs say “Browser app is initializing Geronimo”, “Creating grid app”, “Geronimo GRID Init”, then later “QUERY_GFN_PREPARE values, Server:” and “QUERY_GFN_START values, Server:”.

The live chain is:

1. Mall Angular code in `files/mall/main.*.js` and `923.*.js` builds a start-parameter object from `appConfig.streamer` and the selected game.
2. The page issues a CEF query (`QUERY_GFN_PREPARE`, `QUERY_GFN_START`, `QUERY_GFN_RESUME`, and the rest of the `QUERY_GFN_*` set).
3. `simple_grid_app` calls `GridApp::prepare`, `start`, `resume`, `stop`, `pause`, `cancel`, `getActiveSessions`, or `getSessionInfo`.
4. `GridApp` owns a `SessionControl::SessionController` created by `SessionControl::createSessionController`.
5. The controller calls the exported `nvb*` API in `libBifrost2.so`.
6. `NVB::GridServer` performs HTTPS against `https://<host>:<port>/v2/session` and `/v2/serverInfo`, polls until the seat is usable, then Geronimo calls `nvbStartStreaming` with the connection list.

OpenNOW collapses steps 2 through 6. `opennow-core` speaks CloudMatch over reqwest, normalizes the seat, and `streamer.prepare` / `streamer.start` attach the in-process NVST runtime. There is no CEF query bus and no `nvb*` call.

## libBifrost2 NVB API

`nm -D` on `libBifrost2.so` exports one C session surface plus a small C++ HTTP helper. The C entry points, in the order they appear in the dynamic symbol table, are:

`nvbCancelRequest`, `nvbCollectStatistics`, `nvbConfigureAudioChannelCount`, `nvbCreateClient`, `nvbDestroyClient`, `nvbEnumToString`, `nvbFeatureControl`, `nvbFreeMemory`, `nvbGetActiveSessions`, `nvbGetActiveSessionsTraced`, `nvbGetServerInfo`, `nvbGetSession`, `nvbInitializeClient`, `nvbJoinSession`, `nvbPauseSession`, `nvbPauseStream`, `nvbRegisterCallback`, `nvbResumeSession`, `nvbSendClientInfo`, `nvbSendInputEvent`, `nvbSendMessage`, `nvbSendMicAudioFrame`, `nvbSendStreamStats`, `nvbSetAuthInfo`, `nvbSetAvailableInputDevices`, `nvbSetHttpClient`, `nvbStartSession`, `nvbStartStreaming`, `nvbStopSession`, `nvbStopStreaming`, `nvbTestNetworkAsyncCancel`, `nvbTestNetworkCapability`, `nvbTestNetworkCapabilityAsync`, `nvbTestNetworkLatency`, `nvbTestNetworkLatencyAsync`, `nvbUpdateAdState`, `nvbUpdateDJBState`, `nvbUpdateVideoDecoderState`, `nvbUpdateVideoFrameState`, and `registerTracingCallbacks`.

The C++ helpers are `nvbutil::WebRequest` (`create`, `cancel`, `getResponse`, `getRedirectURL`, `getLocalAddress`, `getError`, `getStatus`, `HttpDecomposeUrl`, `reconfigureHttpClient`, `ErrorToString`) and `nvbutil::Network` (`startup(bool, bool)`, `shutdown`). `WebRequest` can be constructed from either `NVbClientImpl*` or `nvbutil::Network*`. The library’s own NEEDED list is only libpthread, libdl, libatomic, libstdc++, libm, libgcc_s, and libc, with `RUNPATH` `./`. TLS, JSON, and HTTP are compiled in.

Log strings document the argument checks the SDK actually enforces:

- `nvbCreateClient` failure is reported by Geronimo as “Failed to create nvbClient” / `GERONIMO_GRIDAPP_CREATE_CLIENT`.
- `nvbRegisterCallback` rejects a null `NVbCallback.callback`.
- `nvbInitializeClient` rejects an invalid profile, a server type that cannot be initialized synchronously, a bad `NVbClientInitParams`, and a second initialization. Success logs “NVbClient successfully initialized. Server type: %d, supported authentication types: %d”.
- `nvbSetAuthInfo` rejects a null or empty token and an invalid auth type.
- `nvbSetAvailableInputDevices` rejects a null list and any device index at or beyond `NVB_DEVICE_MAX_TYPE`.
- `nvbGetServerInfo` rejects bad `NVbServerParams`.
- `nvbStartSession` requires `NVbSessionParams.streamSettings` to be non-null and `streamSettingsCount >= 1`. A count above what “NVSC supports” is rejected. Resolution and fps must be present (“Missing resolution/fps value in NVbSessionParams”). Keyboard layout and client locale are copied out of the same struct.
- `nvbStartStreaming` applies the same stream-count rule to `NVbStreamingParams`, and additionally requires `connectionInfoCount >= 1` and a non-null `connectionInfo` pointer.
- `nvbJoinSession`, `nvbResumeSession`, `nvbPauseSession`, `nvbPauseStream`, `nvbStopSession`, `nvbStopStreaming`, `nvbGetSession`, `nvbSendInputEvent`, `nvbFeatureControl`, `nvbSendClientInfo`, `nvbCollectStatistics`, and the video-state updates all log the session identifier. Video, decoder, and de-jitter updates also check `streamIndex` against `VIDEO_STREAM_MAX_COUNT`.
- `nvbCancelRequest` refuses request id 0.
- `nvbFeatureControl` checks the type against `NVB_FEATURE_MAX_TYPE`. Named feature ids in the string table include `NVB_FEATURE_NETWORK_CAPTURE_CLIENT`, `NVB_FEATURE_NETWORK_CAPTURE_SERVER`, `NVB_FEATURE_RECORD_STATS_CLIENT`, and `NVB_FEATURE_RECORD_TRACES_SERVER`.
- `nvbSendClientInfo` checks the info type against `NVB_CLIENT_INFO_MAX_TYPE`.
- `nvbTestNetworkLatency` requires a server list. `nvbTestNetworkCapability` requires a server address and logs `NetworkTestProfilesCount`.
- `nvbUpdateAdState` is keyed by session setup request id, and the client refuses the call outside an allowed session-control state.

Callbacks are registered once and then dispatched by type. The string table names three callback classes: `NVB_CB_LOG`, `NVB_CB_EVENT`, and `NVB_CB_SESSIONSETUP_PROGRESS`. “Unknown Callback Type” is the fallback. Several events are synchronous from the SDK’s point of view: the client must return before a logged deadline. The strings name those waits explicitly.

`NVB_EVT_SSL_CERTIFICATE_NOTIFICATION` times out after a logged number of seconds and, on the connection path, rejects the socket if the client does not return. `NVB_EVT_SESSION_INFO` times out per session id, and a client-set `abortSession` flag on that event is ignored. `NVB_EVT_AGENT_STARTED` has the same per-session wait. `NVB_EVT_SESSION_NOTIFICATION` subtypes `NVB_SN_STREAMER_UPSTREAM_READY` and `NVB_SN_STREAMER_CONNECTED` have their own waits. `NVB_SN_SERVER_RESUME`, `NVB_EVT_INTERFACE_CHANGE_EVENT`, `NVB_EVT_HID_OUTPUT_EVENT`, and `NVB_EVT_HID_CHANGE_RESPONSE_EVENT` log that the SDK resumes anyway if the client is late. `NVB_SN_STOPPED_BY_USER` and `NVB_SN_STREAMER_NETWORK_ERROR` also have waits.

The event names present as a single `nvbEnumToString` vocabulary are:

`NVB_EVT_INITIALIZATION_RESULT`, `NVB_EVT_SERVERPLUGIN_RESULT`, `NVB_EVT_SESSION_SETUP_FAILED`, `NVB_EVT_RESUME_FAILURE`, `NVB_EVT_PAUSE_RESULT`, `NVB_EVT_STOP_RESULT`, `NVB_EVT_SESSION_INFO`, `NVB_EVT_SESSION_NOTIFICATION`, `NVB_EVT_JOIN_FAILURE`, `NVB_EVT_ANALYTICS_EVENT`, `NVB_EVT_UPDATE_AUTH_TOKEN`, `NVB_EVT_HAPTIC_EVENT`, `NVB_EVT_CUSTOM_MESSAGE`, `NVB_EVT_AGENT_STARTED`, `NVB_EVT_GETSESSION_RESULT`, `NVB_EVT_DJB_CHANGE_EVENT`, `NVB_EVT_GETSERVERINFO_RESULT`, `NVB_EVT_GETACTIVESESSIONS_RESULT`, `NVB_EVT_SSL_CERTIFICATE_NOTIFICATION`, `NVB_EVT_INTERFACE_CHANGE_EVENT`, and `NVB_EVT_TRUEHDR_CHANGE_EVENT`, plus HDR, L4S, and haptic change events adjacent in the same table.

Session-setup progress names are `NVB_SSS_UNKNOWN`, `NVB_SSS_CONFIGURING`, `NVB_SSS_STARTINGSTREAMER`, `NVB_SSS_SEATREADY`, `NVB_SSS_QUEUEPOSITION`, and `NVB_SSS_PREVIOUS_SESSION_CLEANUP`.

Notification names that matter for seat lifetime are `NVB_SN_STREAMING_PROPERTIES`, `NVB_SN_STREAMING_QUALITY_CHANGED`, `NVB_SN_STREAMING_START_FAILED`, `NVB_SN_STREAMER_CONNECTED`, `NVB_SN_STREAMER_UPSTREAM_READY`, `NVB_SN_STREAMER_NETWORK_ERROR`, `NVB_SN_SERVER_INITIATED_PAUSE`, `NVB_SN_SERVER_INITIATED_RESUME`, `NVB_SN_SERVER_RESUME`, `NVB_SN_PAUSED_BY_USER`, `NVB_SN_STOPPED_BY_USER`, `NVB_SN_STOPPED_UNINTENTIONALLY`, `NVB_SN_FRAME_STATISTICS`, `NVB_SN_SUMMARY_STATISTICS`, `NVB_SN_CURSOR_INFO_CHANGED`, `NVB_SN_CLEAR_IDLE_TIMEOUT`, `NVB_SN_APPROACHING_IDLE_TIMEOUT`, `NVB_SN_APPROACHING_SESSION_MAX_TIMELIMIT`, `NVB_SN_APPROACHING_ENTITLEMENT_TIMEOUT`, `NVB_SN_ENTITLEMENT_TIMEOUT`, `NVB_SN_EXITED_DUE_TO_USER_IDLE_TIMEOUT`, `NVB_SN_EXITED_DUE_TO_SESSION_TIMELIMIT`, and a long termination set: operator, PM, game exited, game not owned, another client, multiple login, windowed mode, full TDR, code integrity, malicious process, miner process, unknown process, and unauthorized activity. One identifier is spelled `NVB_SN_TERMINATED_UNAUTHROIZED_PROCESS_ACCESS` in the binary.

Result codes are equally explicit. Success and local failures include `NVB_R_SUCCESS`, `NVB_R_CANCELLED`, `NVB_R_UNINITIALIZED`, `NVB_R_NOT_SUPPORTED`, `NVB_R_VERSION_MISMATCH`, `NVB_R_INVALID_PARAM`, `NVB_R_INVALID_STREAM_SETTINGS`, `NVB_R_INVALID_AUTH_TYPE`, `NVB_R_INVALID_PROFILE`, `NVB_R_CALLBACKS_NOT_REGISTERED`, and `NVB_R_UNINITIALIZED_AUTHINFO`. Seat and account failures that OpenNOW already classifies in prose include `NVB_R_SESSION_LIMIT_REACHED`, `NVB_R_SESSION_LIMIT_PER_DEVICE_REACHED`, `NVB_R_SESSION_NOT_ACTIVE`, `NVB_R_SESSION_NOT_PAUSED`, `NVB_R_NO_ACTIVE_SESSION_FOUND`, `NVB_R_SESSION_EXPIRED`, `NVB_R_SESSION_SETUP_CANCELLED`, `NVB_R_SESSION_SETUP_CANCELLED_DURING_QUEUING`, `NVB_R_SESSION_IN_QUEUE_ABANDONED`, `NVB_R_SESSION_REMOVED_FROM_QUEUE_MAINTENANCE`, `NVB_R_SERVER_SESSION_QUEUE_LENGTH_EXCEEDED`, `NVB_R_USER_IS_NOT_ENTITLED`, `NVB_R_NO_ENTITLEMENT_TIME_REMAINING`, `NVB_R_GFN_GAME_NOT_OWNED_BY_USER`, `NVB_R_EULA_NOT_ACCEPTED`, `NVB_R_REGION_BANNED`, `NVB_R_REGION_NOT_SUPPORTED_FOR_STREAMING`, `NVB_R_INSUFFICIENT_NETWORK_CAPABILITY`, `NVB_R_MINIMUM_NETWORK_CAPABILITY`, `NVB_R_APPLICATION_PATCHING`, and the auth family `NVB_R_AUTH_ERR_DEFUNCT_TOKEN`, `NVB_R_AUTH_ERR_TOKEN_NOT_UPDATED`, `NVB_R_AUTH_ERR_UNAUTHORIZED_CLIENT`, `NVB_R_AUTH_ERR_UNSUPPORTED_PROTOCOL`, `NVB_R_AUTH_ERR_UNSUPPORTED_TOKEN_FORMAT`. Ads add `NVB_R_SESSION_WAITING_ADS_TIME_EXPIRED`, `NVB_R_USER_CANCELED_WATCHING_ADS`, and `NVB_R_SESSION_INVALID_ADS_STATE_TRANSITION`.

Auth types the SDK will name are `NVB_AUTH_INVALID`, `NVB_AUTH_OAUTH2_PINGFEDERATE`, `NVB_AUTH_OAUTH2_GOOGLE`, `NVB_AUTH_OAUTH2_JANRAIN`, `NVB_AUTH_ACTIVE_DIRECTORY`, `NVB_AUTH_CLIENT_CERTIFICATE`, `NVB_AUTH_SESSION_ID`, `NVB_AUTH_JARVIS`, `NVB_AUTH_JWT`, `NVB_AUTH_JWT_GFN`, and `NVB_AUTH_NONE`. The mall’s `setAuthInfo` path in `main.*.js` maps a Jarvis JWT to `NVB_AUTH_JWT` and passes the token into `QUERY_GFN_SET_AUTH_INFO` / `QUERY_GFN_SET_AUTH_TOKEN`. Bifrost then refreshes that token by raising `NVB_EVT_UPDATE_AUTH_TOKEN`. If the client does not update it, the SDK aborts the web request or the session request and logs the retry count (“Client did not update authentication token. Aborting session request: %d. RetryAttempt: %u”).

Port-usage names compiled next to the streaming path are `NVB_PU_CONTROL`, `NVB_PU_AUDIO`, `NVB_PU_INPUT`, `NVB_PU_CUSTOM`, `NVB_PU_USB`, `NVB_PU_RTSP`, `NVB_PU_RTSPRU`, `NVB_PU_AUDIO_INPUT`, `NVB_PU_RTSPS`, `NVB_PU_BUNDLE`, and `NVB_PU_VIDEO`. A second usage vocabulary used when printing connection info is `GAMESTREAM_CONTROL`, `GAMESTREAM_SECURE_CONTROL`, `SESSION_CONTROL`, `NETWORK_TEST_CONTROL`, `SIGNALING`, and `MEDIA`. The log line is `ConnectionInfo[%d]: address: %s, port: %d, usage: %s, protocol: %d`. Unexpected usages are skipped: “Skipping unexpected port usage type: %s (%d) for port number: %d”.

## GridServer: the CloudMatch state machine inside Bifrost

Demangled symbols place the HTTP client in `NVB::GridServer`, with tasks `ProvisionSeatTask`, `GetSessionTask`, `GetActiveSessionsTask`, `SessionModifyTask`, and `SessionCleanUpTask`. The methods that show up as local symbols are `requestSession`, `sendSessionPUTRequest`, `pollSession`, `getSessionInformation`, `sendRequestToServer`, `sendSyncStopRequest`, `sendSessionSetupProgressEvent`, `getSessionTaskFunc`, and `getActiveSessionsTaskFunc`.

URL formats are literal:

- `%s://%s:%d/v2/serverInfo`
- `https://%s:%d/v2/session`
- `https://%s:%d/v2/session/%s`

Verbs and command names in the same cluster are `POST`, `DELETE`, `RESUME`, `JOIN`, `FORWARD`, `TRANSFER`, `SESSION_RATING`, `AD_UPDATE`, `setup`, `PollSession`, `PostSession`, `DeleteSession`, `GetSessionList`, and `SendStopRequest`. PUT is logged as “PUT request URL: %s”. A create is logged “Session request URL: %s”, “Serialized session request. Size: %d, Data: '%s'”, “Accepted new session request. Request Id: %d”, and “Created session: %s” / “Creating/Resuming session: %s”.

Polling is a first-class loop, not a client timer. Strings say “Polling session. The request url is %s”, “Polling for session %s, request id: %d, QueuePosition: %d, ETA: %d(msec)”, “Received response during polling is: '%s'”, “Unexpected state %s (%d) while polling for session”, and “Session %s state changed to Finished, aborting session poll”. A network error during polling sends DELETE for that session id. A session that was canceled also sends DELETE (“Session %s was canceled, sending DELETE request for session: %s”), including the case “Start session canceled before setting up session on PM.” Delete retries log “Retry Delete Attempt %d”. If the control server is missing, DELETE is refused (“Unable to send session DELETE request to the server. Control server not found”).

Zone forwarding is explicit: “Session: %s is forwarded from zone %s:%d to %s:%d”. Ads pause the poll (“Polling for session %s stopped for ADs flow”) and resume it (“Either ADs flow is completed or the AdsWaitTimeout expired. Restarting polling”). Ad updates carry `AdId`, `AdAction`, queue position, ETA in milliseconds, and an ads count. The SDK caches pause/resume pairs and drops back-to-back duplicates before calling `sendAdupdatesRequestToServer`. `ConvertNvbAdActionToAdState` rejects an unexpected action. State transitions that skip a legal edge are refused.

Server-info handling rejects a payload with no version, an unexpected server type, and an unexpected authorization type. Headers compiled into the client include `x-nv-client-identity` and the `NV-Client-Type` / `NV-Device-Type` / `NV-Client-Stream` family (the last three appear concatenated in one rodata run because they are adjacent C strings). RTSP header names that Bifrost owns for later media, and that OpenNOW already mirrors at ANNOUNCE time, include `x-nv-vqos`, `x-nv-audio`, `x-nv-bwe`, `x-nv-sessionid`, `x-nv-video`, `x-nv-general`, `x-nv-mic`, `x-nv-packet`, `x-nv-ping`, `x-nv-qscore`, `x-nv-ri`, `x-nv-runtime`, `x-nv-aqos`, `x-nv-abtesting`, and `x-nv-cli`.

Session status names in the same binary are `INITIALIZING`, `READY_FOR_CONNECTION`, `PLAYING`, `PAUSED_UNINTENTIONAL`, `PAUSED_INTENTIONAL`, `RESUMING`, `FINISHED`, plus “Unknown Status”. App launch modes include `TOUCH_FRIENDLY`, `GAMEPAD_FRIENDLY`, and `desktop_pro`. A log line prints “App Launch Mode: %s (%d)”. Signaling URLs may be `wss://`. The user-agent fragment `) BifrostClientSDK/` is present, which is the tail of the `GFN-PC/<version> (<os>) BifrostClientSDK/<sdk>` string OpenNOW already sends.

If the returned session has no `sessionControlInfo`, Bifrost falls back: “Session %s does not have valid session Controller information, using default server address” and “Received session %s does not contain information on controlling server.” Active-session listing logs “The number of active sessions is: %d” and “The number of user's other active sessions are: %d”. Termination is summarized as “ExtendedTermination code:: SessionTerminationReason: 0x%x, NvbSessionTerminationType: %d, ExitCode: 0x%x, SessionAlive: %u”.

## Session JSON that Bifrost serializes

The `Session` / `SessionRequest` / `SessionInfo` field names sit in one rodata run inside `libBifrost2.so`. The request object is `sessionRequestData`. Fields that travel with a create or resume include:

`sessionId`, `finalSelectedScreenResolution`, `monitorSettings`, `userIdleWarningTimeoutInMs`, `clientIp`, `seatSetupInfo`, `sessionControlInfo`, `connectionInfo`, `gpuType`, `sessionAdsRequired`, `sessionAds`, `enhancedStreamMode`, `errorCode`, `finalizedStreamingFeatures`, `appLevelProtocol`, `resourcePath`, `metaData`, `adUpdates` (`adId`, `adAction`, `clientTimestamp`, `cancelReason`, `watchedTimeInSeconds`, `watchedTimeInMs`, `pausedTimeInMs`), `appId`, `internalTitle`, `availableSupportedControllers`, `preferredController`, `networkTestSessionId`, `parentSessionId`, `clientIdentification`, `deviceHashId`, `clientVersion`, `sdkVersion`, `streamerVersion`, `clientPlatformName`, `clientRequestMonitorSettings`, `useOps`, `audioMode`, `sdrHdrMode`, `clientDisplayHdrCapabilities`, `surroundAudioInfo`, `remoteControllersBitmap`, `clientTimezoneOffset`, `appLaunchMode`, `secureRTSPSupported`, `partnerCustomData`, `accountLinked`, `requestedAudioFormat`, `userAge`, `requestedStreamingFeatures`.

`requestedStreamingFeatures` itself contains `enablePersistingInGameSettings`, `reflex`, `enabledL4S`, `mouseMovementFlags`, `trueHdr`, `supportedHidDevices`, `fallbackToLogicalResolution`, `hidDevices`, `prefilterMode`, `prefilterSharpness`, `prefilterNoiseReduction`, `hudStreamingMode`. HID entries have `vendorId`, `productId`, `revision`, `customId`. HDR display data has the primary chromaticity pairs `displayPrimaryX0/Y0` through `X2/Y2`, white point, `desiredContentMaxLuminance`, `desiredContentMinLuminance`, `desiredContentMaxFrameAverageLuminance`, plus `peakLuminanceIndex`, `peakFullFrameLuminanceIndex`, `hdrEdrSupportedFlagsInUint32`, `static_metadata_descriptor_id`, and `display_data`.

Monitor entries contain `monitorId`, `positionX`, `positionY`, `widthInPixels`, `heightInPixels`, `framesPerSecond`, `displayData`, `hdr10PlusGamingData`. Response status is `statusCode`, `statusDescription`, `unifiedErrorCode`, `requestId`, `serverId`, `countryCode`, plus server capability fields `maxLumaPixelsH264`, `maxLumaPixelsHEVC`, `serverColorSpaceSupport`, `serverCodecModeSupport`, `buildDateTime`, `buildVersion`, `protocolMajorVersion`, `protocolMinorVersion`, `zoneVersion`, `requestStatus`, `otherUserSessions`. Seat progress is `queuePosition`, `seatSetupEta`, `seatSetupStep`. Connection entries add `relayProtocol` and `relayLocation`.

`serverInfo` objects name `authType`, `serverType`, `serverInstanceId`, `serverEncodeCapability`, `serviceExtras`, `notificationTopicId`. A separate log key `request.status` / `request.id` is used when the JSON uses dotted request status. Bifrost also logs `gameSeatInitializing` and `active.sessions` as request-type tags.

Geronimo deserializes a stricter streaming subset. Failure strings require these members to be strings: `sessionId`, `subSessionId`, `networkSessionId`, `rtspSessionId`, `streamSessionId`. `streamSubSessionIds` must be an array of strings (the binary spells the type error “Arrray”). `getStreamStartParameters` then requires connection info, monitor settings, and streaming features on the `SessionObject`. Missing server address skips that connection. Unknown resume type becomes `NONE`. Unknown port usage is logged and skipped. Unknown connection protocol defaults to UDP. A mode-selection result object is deserialized separately and must contain “required fields”. The normalized Geronimo session record also carries `gpuType`, `zoneAddress`, `zoneName`, `appLaunchMode`, `keyboardLayout`, `selectedVideoMode`, `selectedFeatures`, `selectedEncodeMode`, and feature keys `bitDepth`, `chromaFormat`, `vvsync`, `audioChannelCount`, `hdr10PlusGaming`, `maxBitrateKbps`, `dynamicStreamingMode`, `prefilterParams`, `hudStreamingParams`, `scaleFactor`.

## Geronimo session controller

`libGeronimo.so` exports no `nvb*` symbols of its own; it imports them. The session owner is `GridApp`, constructed with a `SessionControl::SessionController` and a `BifrostSDKExecutor` in front of `NVbClientImpl`. `IOInterface` is constructed from `BifrostSDKExecutor*` and `StatsInterface*`.

`GridApp` methods that form the orchestration surface are:

`prepare(SessionControl::PrepareParameters const&)`, `start(SessionControl::SessionParameters const&, NVbTracingContext_t const&)`, `resume` (two overloads: one with session parameters, one with a tracing context and a string), `stop(char const*, int)`, `pause()`, `cancel()`, `stopStreaming(int)`, `pauseStreaming(int)`, `startStreaming(SessionInfo const&)` and `startStreaming(StreamStartParameters, VideoDecoderInitParams)`, `getActiveSessions`, `getSessionInfo`, `setAuthInfo`, `updateAuthToken`, `updateAdState`, `setNVbSessionParams`, `onPrepareResult`, `onSessionSetupProgress`, `onSessionSetupSuccess`, `onSessionSetUpFailure`, `onSeatInitializing`, `onGetSessionResult`, `onActiveSessions`, `onPauseResult`, `onStopResult`, `onResumeFailure`, `onStreamingTerminated`, `onStreamingWarnings`, `onUpstreamReady`, `identifyNetworkConnectionType`.

Log lines tie those methods to NVB calls:

- “Routing prepare call to SessionController”.
- “Invalid prepare parameters” / `GERONIMO_GRIDAPP_INVALID_PREPARE_PARAMS`.
- “Creating SessionController instance via createSessionController”, “Instantiated SessionControllerImpl”, “Destroyed SessionControllerImpl”, “Failed to create and initialize the SessionController instance”, “Failed to instantiate SessionController!”.
- “nvbInitializeClient failed” / `GERONIMO_GRIDAPP_INITIALIZE_CLIENT`.
- “nvbSetAuthInfo returned success” and “nvbSetAuthInfo failed” / `GERONIMO_GRIDAPP_SET_AUTH_INFO`.
- “nvbStartSession failed” / `GERONIMO_GRIDAPP_START_SESSION`.
- “nvbStartStreaming request accepted successfully.”
- “Processing GridApp::stopStreaming. Initiating nvbStopStreaming.” and “nvbStopStreaming returned success”.
- “Routing stop session call to SessionController”.
- “Failed to stop streaming. Will continue to delete session.”
- “stop. arg: '%s', currentSessionId: '%s', sessionSetupRequestId: '%d', reason: '%d'”.
- “deleteSession called with a valid sessionId while a session setup is in progress. Should not occur.”
- “Successfully cancelled the pending session setup request: %d” and “Failed to cancel the pending request” / `GERONIMO_GRIDAPP_CANCEL_REQUEST`.
- “No session exists, cannot pause” / “No streaming session exits, ignoring pauseStreaming.” / `GERONIMO_GRIDAPP_PAUSE_SESSION`.
- “No session exists, cannot resume” / “Invalid session Id in resume.” / `GERONIMO_GRIDAPP_RESUME_SESSION`.
- “nvbGetActiveSessions failed” / `GERONIMO_GRIDAPP_GET_ACTIVE_SESSIONS`.
- “Error nvbGetSession failed: %s (%d)”.
- “Received session info event: sessionid %s with gpu %s” and “SessionId: %s, SubSessionId: %s” and “Seat: %s (%s) / %s”.
- “Processing NVB_EVT_STOP_RESULT event” and “Issuing onSessionSetUpFailure instead of onStopResult.”
- “NVB_SN_STREAMER_CONNECTED notification received” and “NVB_SN_STREAMER_UPSTREAM_READY notification received”.
- “Resumable: %u, sessionAlive: %u is sent for terminationReason:0x%08x extendedCode:0x%08x”.
- “Fatal error notification! Terminating streaming session if any. Error: 0x%08x”.
- “Session Id is missing. Cannot initiate streaming.”
- “Atleast one down stream video or audio settings are required to establish a streaming session.”
- “Failed to convert the streaming parameters to NVbStreamingParams.”
- “Video recording support is disabled during initiating the session.”
- “GridApp::updateAdState calling nvbUpdateAdState for Session requestId: %d”.

`GeronimoSettingsImpl::overrideNVbSessionParams(NVbSessionParams_t&)` and `overrideCommunicationParams` mean a remote-config document can rewrite the struct after the mall has filled it. `updateRemoteConfig`, `updateFromJSONString`, and `updateRemoteOverridesGxt` are the loaders. Debug overrides live in `GeronimoDebugConfig` (`GeronimoDebugConfig.txt`), with keys such as `force444`, `forceVRR`, `forceSDR10`, `trueHDREnabled`, codec and fps overrides, and generic HID allow-lists. Those are developer overrides, not the shipping `config.json`.

Before streaming, Geronimo also stamps keyboard layout and locale: “KBLayout:: session launch/resume, layout: %s, autoMode: %s” and “Session launch/resume, client locale: %s”. After upstream-ready it compares session, system, and client-requested layouts. `sendCustomMessage` before `onUpstreamReady` is queued rather than dropped.

`BifrostSDKExecutor` is the in-session control plane once media exists: feature control, audio config, mic frames, input events, DJB config, video frame and decoder state, max bitrate, dynamic streaming mode, L4S, true-HDR parameters, prefilter, HUD, window and system state, vsync, and client OS string. That is the same work OpenNOW’s native streamer does over NVST after `streamer.start`, not during CloudMatch POST.

## CEF query bridge

The shell’s query names are the contract the mall actually calls. Session lifecycle queries, in table order, are:

`QUERY_GFN_PREPARE`, `QUERY_GFN_START`, `QUERY_GFN_RESUME`, `QUERY_GFN_STOP`, `QUERY_GFN_CLEANUP`, `QUERY_GFN_CANCEL`, `QUERY_GFN_REGISTER_CALLBACK`, `QUERY_GFN_GET_ACTIVE_SESSIONS`, `QUERY_GFN_GET_SESSION_INFO`, `QUERY_GFN_SET_AUTH_INFO`, `QUERY_GFN_SET_AUTH_TOKEN`, `QUERY_GFN_PAUSE_STREAMING`, `QUERY_GFN_UPDATE_AD_STATE`.

Related queries that mutate an already created seat are `QUERY_GFN_UPDATE_STREAMING_SETTINGS`, `QUERY_GFN_SET_STREAMING_MAX_BITRATE`, `QUERY_GFN_SET_DYNAMIC_STREAMING_MODE_STATE`, `QUERY_GFN_SET_STREAMING_L4S_STATE`, `QUERY_GFN_SET_PRE_FILTER_STATE`, `QUERY_GFN_SET_VSYNC_ENABLED`, `QUERY_GFN_SET_HUD_STATE`, `QUERY_GFN_LATENCY_BASED_ROUTING`, `QUERY_GFN_UPDATE_REMOTE_CONFIG`, `QUERY_GFN_GET_PLATFORM_CLOUD_GSYNC_CAPABILITIES`, and `QUERY_GFN_IS_PLATFORM_SUPPORTS_HDR_STREAMING`.

Prepare is idempotent: “QUERY_GFN_PREPARE already executed. Returning early forcing success”. Prepare completion is asynchronous: “onPrepareResult: Completing prepare callback with success” or “with failure”, and “No prepare callback is currently registered so we cannot complete the promise.” Progress and failure are forwarded as “onSessionSetupProgress(state:” and “onSessionSetUpFailure(result:”. Termination is “onStreamingTerminated(reason:”. Empty auth refresh is “updateAuthToken() token not updated, empty token provided”.

`createStartParameters` in `main.*.js` picks a fixed field list off the streaming-params object and then attaches the CloudMatch session, a tracing span, keyboard layout, content rating, and auth refresh:

`appId`, `appLaunchMode`, `frameStatsEnabled`, `summaryStatsEnabled`, `maxLocalPlayers`, `advancedLatencyOptimization`, `networkPacketCaptureEnabled`, `metaData`, `frameLossWarningTimeout`, `frameLossErrorTimeout`, `address`, `serverType`, `port`, `partnerCustomData`, `streamingProfile`, `locale`, `accountLinked`, `persistingInGameSettings`, `gameShortName`, `networkSessionId`, `audioModeFormat`, `supportedControls`, `heroImage`, `gameDisplayOwnRating`, `storeName`, `appName`, `subscriptionLongDesc`, `providerName`, `zoneName`, `userAge`, `serverLocation`, `gpuNameMap`, `streamingDisplayDataInfo`, plus `keyboardLayout`, `allowKeyboardLayoutChange`, `contentRating`, `refreshAuthToken`, `session`, and `spanData`.

Metadata added in that function is `ClientImeSupport` (from `featureEnablement.serverIME`) and `clientPhysicalResolution` (JSON of the current physical resolution). The streamer chunk `923.*.js` fills the same object from `appConfig.streamer`: `frameStatsEnabled`, `summaryStatsEnabled` forced true at this call site even though config says false, `networkPacketCaptureEnabled` from `streamer.networkCapture`, and both frame-loss timeouts. It also passes `networkSessionId`, `audioModeFormat` from the streaming profile, and `persistingInGameSettings`.

Reconnect is a mall timer, not a Bifrost loop. `streamingService` builds `reconnectTimeout` from `appConfig.streamer.reconnectTimeout` and a refresh interval from `reconnectRefreshInterval`. Network-error detection uses the regex `/^8004([A-F0-9]{4})/`. `featureEnablement.useGridServer2` is false in this config, and the service reads that flag, so this build stays on the `grid.server` v2 host.

Serenity is a CEF-side recorder, not a CloudMatch field. Strings in `GeForceNOW` say “serenity selected. target video codec H264”, “configured H264 record supported by transcode”, “reconfiguring system to use serenity and record to H264”, “configuring serenity for record”, and “NOT configuring serenity for record”. The implementation files are `gfn/serenity/serenity.cc` and `rtsp_handler.cc`. Config key `useSerenity` appears in the shell string table next to `recordCodec`, `frameStatsEnabled`, and `networkPacketCaptureEnabled`.

## config.json streamer settings

The shipping streamer object is small and sits next to the CloudMatch grid endpoint:

```json
"grid": {
  "server": "https://prod.cloudmatchbeta.nvidiagrid.net/",
  "name": "Prod",
  "version": "v2",
  "retryConfig": { "defaultRetries": 2, "defaultTimeout": 2000, "defaultTimeBetweenRetries": 5000 }
},
"streamer": {
  "server": "prod.cloudmatchbeta.nvidiagrid.net",
  "port": 443,
  "useSerenity": true,
  "recordCodec": "h264",
  "frameStatsEnabled": false,
  "summaryStatsEnabled": false,
  "networkCapture": false,
  "frameLossWarningTimeout": 500,
  "frameLossErrorTimeout": 30000,
  "reconnectTimeout": 300000,
  "reconnectRefreshInterval": 5000,
  "analyticsTimeout": 10000
}
```

`grid.server` is the CloudMatch origin. `streamer.server` and `streamer.port` are the host and port Geronimo/Bifrost receive in `QUERY_GFN_PREPARE` (“Pre-NVb-conversion ServerType”). They match the URL shapes `https://%s:%d/v2/session` and `%s://%s:%d/v2/serverInfo`. Retry policy for the grid client is two retries, a 2 second timeout, and 5 seconds between retries. That is separate from Bifrost’s own auth-refresh retry.

`useSerenity: true` with `recordCodec: "h264"` selects the CEF Serenity RTSP recorder and forces an H.264 transcode when the live codec cannot be recorded directly. `frameStatsEnabled` and `summaryStatsEnabled` map onto `nvbCollectStatistics` options; the streamer module overrides summary stats to on when it builds start parameters. `networkCapture: false` maps onto `networkPacketCaptureEnabled` and the shell log “Enabling network packet capture”, which is the `NVB_FEATURE_NETWORK_CAPTURE_*` feature control.

`frameLossWarningTimeout` of 500 ms and `frameLossErrorTimeout` of 30000 ms are mall-side media health timers passed into the native start parameters. They are distinct from Bifrost QoS keys `vqos.intraR.frameLossThreshold` and `frameLossWindowSize`, which are RTSP `x-nv-vqos` fields. `reconnectTimeout` of 300000 ms (five minutes) and `reconnectRefreshInterval` of 5000 ms bound the mall’s connectivity auto-resume. `analyticsTimeout` of 10000 ms is present in config; the minified mall chunks inspected here do not reference that key by name, so its consumer is either a later remote-config overlay or a native reader of the same JSON.

Neighboring keys that change session behavior without living under `streamer`:

- `networkTest`: `delayFactorCapacityFull` 1000, `prefetchStreams` true, `maxZonesPerFingerprint` 3, `useMaxBandwidthForNetworkTest` true. This feeds `nvbTestNetworkLatency` / `nvbTestNetworkCapability` and the `networkTestSessionId` field.
- `lbrConfig`: `isLbrEnabled` true, `cachedLatencyCount` 5, `applyDeterministicServerRouting` true. The shell exposes this as `QUERY_GFN_LATENCY_BASED_ROUTING`.
- `client.clientStreamerClassic`: true. The mall maps that boolean to `ClientStreamer.CLASSIC` versus `WEBRTC`. Combined with `client.userAgent` `GFN-PC` and `client.clientTypeNative` true, this is the classic native NVST client, which is why the HTTP stack sends an NVIDIA-CLASSIC streamer header.
- `client.requireFullscreenInStreaming`: true. Linux overrides in `linuxOSConfig` hide the exit-fullscreen control (`igo.hideExitFullScreenOption`), disable photo mode, game filter, and NVCamera, and turn off Discord rich presence, OSC, OOGG, playtest, wheels, and flight controllers.
- `client.supportedStreamingProfiles`: balanced, data saver, competitive, cinematic, and custom are all enabled. SteamOS disables competitive. Profiles are an input to `ConfigureStreamerVideoSettings`, which logs width, fps, max bitrate, HDR mode, reflex, cloud gsync, bit depth, chroma, prefilter, HUD, and dynamic streaming mode.
- `client.streamingModeResolutionsConfiguration` lists 5K, UW 5K, SUW QHD, 4K, QHD, 3.5K, UW QHD, and UW FHD badges, pixel-count bands HD/FHD/QHD/UHD, and `unsupportedFpsIdentifier` 240. `displayAllDynamicStreamingModes` is true.
- `featureEnablement.serverIME` is true and `clientIME` is false, which is why start metadata sets `ClientImeSupport` from the server-IME flag. `dynamicStreamingResolution` is true. `bypassHevcDecodeSupport` is false. `mirrorClientHDRProperties` is false, so HDR static metadata is gated rather than always copied from the display. `useGridServer2` is false.
- `remoteConfig` points at `https://rconfig.nvidiagrid.net/v2` branch `ebeta` with a 500 ms launch timeout. `gxtRemoteConfig` points at the GX target frontend for product “GFN” / project “My GeForce NOW”, refreshing every 24 hours. Those documents are what `GeronimoSettingsImpl::updateRemoteOverridesGxt` and `QUERY_GFN_UPDATE_REMOTE_CONFIG` apply on top of the baked `config.json`, including `overrideNVbSessionParams`.
- `eagerLoadingConfig.streamerPreloadDuration` is 5 (the route `data.eagerLoadingConfigKey` is that string). The streamer Angular module is preloaded.
- `gamepadConfig.implementationType` is `geronimo`.
- `opportunity` configures ad playback: 10 s force-play timeout, 30 s timeout on ad-start failure, 30 s interval on failure, preferred sort order `mp4deinterlaced720p`, `webm`, `hlsadaptive` on desktop and webm-first on Linux and SteamOS. Linux therefore prefers a software-decodable creative before the deinterlaced MP4.

`Resources/config/product-config.json` is only a debug file logger (two rotations, 50 MB). `Resources/config/ShareServer.json` enables `autoHighlights`. Neither file carries CloudMatch fields. The streamer settings live in the mall config, and the shell reads the same keys as loose strings (`useSerenity`, `recordCodec`, `frameLossWarningTimeout`, `frameLossErrorTimeout`, `streamingProfile`, `maxBitrate`, `hdrStreamingMode`, `reflex`, `cloudGsync`, `bitDepth`, `chromaFormat`, `dynamicStreamingMode`).

## CEF GeForceNOW.json switches

`files/cef/Resources/GeForceNOW.json` is the switch list the shell applies at startup. The full set in this build is:

`nv-no-sandbox`, `nv-use-angle-gl-egl`, `nv-native-window-size`, `nv-gpu-accel=true`, `nv-custom-black-window=true`, `nv-min-window-size=640,360`, `nv-shared-storage-name=GeForceNOW`, `nv-ipc-type=mallclient`, `nv-url-relative=../mall/index.html`, `nv-streamer-url-relative=../mall/index.html`, `nv-plugin-folder-relative=plugins/BackgroundProcess;plugins/Base;plugins/GeForceNOW`, `nv-plugin-dependencies-relative=./dependencies`, `nv-self-update-path=gfnupdate.json`, `nv-background-color=0xFF000000`, `nv-localization-path=../mall/assets/i18n`, `nv-doc-extension=gfnpc`, `nv-startup-autoupdate=true`, `nv-app-url-scheme=geforcenow`, `nv-gfn-streamer=true`, `nv-sdl-vsync=true`, `nv-sdl-resizable=true`, `nv-sdl-hidpi=true`, `nv-cmd-storage-path=/`, `nv-gfn-async-renderer=true`, `nv-app-name=GeForceNOW`, `nv-nvtelemetry-config-path=../nvtelemetry/NvTelemetry.json`, `nv-telemetry-api-binary-path=../nvtelemetry`, `nv-renderer-blocklist=llvmpipe,softpipe,software`.

The shell binary contains a larger switch vocabulary than the JSON enables. Present in the executable and absent from this JSON are `nv-sdl-force-windowed`, `nv-sdl-vsync-adaptive`, `nv-sdl-offscreen`, `nv-sdl-fullscreen-exclusive`, `nv-gfn-stats`, `nv-gfn-encrypt-flags`, `nv-osc`, `nv-shadowplay`, `nv-sandboxed`, `nv-remote-debugging-port`, `nv-proxy-server`, `nv-disable-gpu-compositing`, and `nv-ignore-gpu-blocklist`. The JSON is the policy; the binary is the parser.

Switches that define session architecture:

- `nv-gfn-streamer=true` turns on the native Geronimo path. Combined with `clientStreamerClassic`, the page is a controller, and pixels come from SDL/Vulkan inside Geronimo rather than from a WebRTC video element.
- `nv-ipc-type=mallclient` selects the query channel used by `QUERY_GFN_*`. Plugins loaded from `plugins/BackgroundProcess`, `plugins/Base`, and `plugins/GeForceNOW` include `libBackgroundProcess.so`, `libSystemInfo.so`, `libMessageBusRouter.so`, `libShareServer.so`, and `libNetworkTest.so`. Dependencies include `libNetworkTestSDK.so`, `libnvmessagebus.so`, and `libgfnspfbc.so`.
- `nv-url-relative` and `nv-streamer-url-relative` both point at `../mall/index.html`. The shell also accepts `nv-url-absolute`. The streamer UI and the store UI are the same document; session chrome is a route (`streamerPreloadDuration`), not a second browser.
- `nv-gfn-async-renderer=true` plus `nv-use-angle-gl-egl` and `nv-gpu-accel=true` select the async EGL renderer. The shell logs “Async Render unavailable due to config or platform” and “GPUAccel disabled. Disable async rendering.” when that combination fails. Offscreen Geronimo presentation is compiled (`offscreen_renderer_geronimo.cpp`, `osr_simple_handler.cpp`) and used when the async renderer is active.
- `nv-sdl-vsync=true`, `nv-sdl-resizable=true`, `nv-sdl-hidpi=true`, and `nv-min-window-size=640,360` are the SDL window that Geronimo presents into. `nv-custom-black-window=true` and `nv-background-color=0xFF000000` keep the pre-first-frame surface black, which matches the streamer’s `DefaultStreamerBackgroundArt` asset.
- `nv-renderer-blocklist=llvmpipe,softpipe,software` refuses software GL for the CEF GPU process. That is a shell policy. Geronimo separately refuses to start a session when H.264 or HEVC decode is missing (“ConfigureStreamerVideoSettings:: returing H264_DECODER_NOT_SUPPORTED” / `HEVC_DECODER_NOT_SUPPORTED`) and can fall back to a lower resolution for Serenity transcode.
- `nv-no-sandbox` is set. The Flatpak is the sandbox. `nv-shared-storage-name=GeForceNOW` and `nv-cmd-storage-path=/` feed the JSON storage the shell logs as `/sharedstorage.json` and `/storage.json`.
- `nv-app-url-scheme=geforcenow` matches `uasConfig.homeUrl` `geforcenow://open`. `nv-doc-extension=gfnpc` is the document type. `nv-startup-autoupdate=true` and `nv-self-update-path=gfnupdate.json` run before a session; Linux `selfUpdate.showUpdateInProgressDialog` is true.
- Telemetry switches point at `../nvtelemetry/NvTelemetry.json`. The shell logs “Initializing Geronimo telemetry” and “Telemetry client version”. OpenTelemetry export in mall config goes to `https://prod.otel.kaizen.nvidia.com/traces/otlp/v0.9` with component `gfn-client`, and the API trace regex explicitly includes `/v2/serverInfo`.

`nv-localization-path=../mall/assets/i18n` is how locale reaches “Session launch/resume, client locale”. The locale list in the shell includes `en_US`, `en_GB`, and the same set shipped as hashed JSON under `mall/assets/i18n`.

## Mapping onto OpenNOW

OpenNOW’s protocol 5 session methods are `session.create`, `session.poll`, `session.stop`, `session.active.get`, `session.remote.list`, `session.claim`, `session.ad.report`, then `streamer.detect`, `streamer.prepare`, `streamer.start`, `streamer.status.get`, and `streamer.stop`. That is the same lifecycle as `QUERY_GFN_PREPARE` / `START` / `RESUME` / `STOP` / `GET_ACTIVE_SESSIONS` / `GET_SESSION_INFO` / `UPDATE_AD_STATE`, with the HTTP stack moved into `CloudMatchService`.

`session.create` builds `sessionRequestData` in `build_create_body`. Field for field, the body matches the Bifrost request run for the keys OpenNOW fills: `appId`, `internalTitle`, `availableSupportedControllers` `[2]`, `preferredController` `2`, `networkTestSessionId`, `parentSessionId` null, `clientIdentification` `GFN-PC`, `deviceHashId`, `clientVersion` `30.0`, `sdkVersion` `2.0`, `streamerVersion` `14`, `clientPlatformName`, one `clientRequestMonitorSettings` entry (`monitorId` 0, origin 0,0, width, height, fps, `sdrHdrMode`, `displayData`, null HDR10+ gaming data, dpi 96 on Linux), `useOps` true, `audioMode` 2, metadata, `sdrHdrMode`, null `clientDisplayHdrCapabilities`, `surroundAudioInfo` 0, `remoteControllersBitmap` 0, timezone offset, `enhancedStreamMode` 0, `appLaunchMode`, `secureRTSPSupported` true, null partner data, `accountLinked`, `enablePersistingInGameSettings`, `requestedAudioFormat` 0, `userAge`, `requestedStreamingFeatures`, and null `transport`.

`requestedStreamingFeatures` sends `reflex`, `bitDepth`, `cloudGsync`, `enabledL4S`, `mouseMovementFlags` 0, `trueHdr`, `supportedHidDevices` 0, `profile` 0, `fallbackToLogicalResolution` false, null `hidDevices`, `chromaFormat`, and zeroed prefilter and HUD fields. Codec, max bitrate, and dynamic streaming mode are left out of this object on purpose: Bifrost’s own later path carries them as RTSP `x-nv-vqos`, and OpenNOW’s ANNOUNCE does the same. HDR sets `sdrHdrMode` 1, `trueHdr` true, and bit-depth enum 1, which is the contract in `docs/core-protocol.md`.

Headers in `cloudmatch_headers` line up with the Bifrost header fragments: `Authorization: GFNJWT <token>`, `x-nv-client-identity`, `nv-client-type: NATIVE`, `nv-client-streamer: NVIDIA-CLASSIC`, `nv-client-id` equal to the mall LCARS id `ec7e38d4-03af-4b58-b131-cfb0495903ab`, `x-device-id`, and a user agent `GFN-PC/30.0 (Linux) BifrostClientSDK/4.9 (38495286)`. Content type is `text/plain`. The classic-streamer header is the HTTP form of `clientStreamerClassic: true`.

The POST URL is `v2/session` on `https://prod.cloudmatchbeta.nvidiagrid.net/` unless a strict zone override replaces the origin. That is `grid.server` plus `grid.version`. An optional network-test session id is acquired first and stored on the body, which is the OpenNOW counterpart of `networkTestSessionId` and `nvbTestNetwork*`, limited to a single typed test allocation.

Response normalization in `session_info` keeps `sessionId`, `subSessionId`, numeric `status`, `queuePosition`, `seatSetupStep`, ads, `gpuType`, `appLaunchMode`, `connectionInfo`, derived `rtspsEndpoints`, `iceServers`, signaling URL, and a negotiated profile. RTSPS endpoints are recognized when usage is 16, `appLevelProtocol` is 6, or `resourcePath` starts with `rtsps://` or `rtsp://`. Media candidates use usage 2 or 17. Signaling prefers usage 14. Those integers are OpenNOW’s live decoding of the usage names Bifrost prints (`SIGNALING`, `MEDIA`, `NVB_PU_RTSPS`). `session_phase` maps status 1 to preparing, 2 ready, 3 streaming, 4 and 5 paused, 6 resuming, and 7 finished. `streamer.prepare` and `streamer.start` accept only status 2 or 3, which is `READY_FOR_CONNECTION` or `PLAYING` in the Bifrost names, and they require RTSPS endpoints before attachment. That matches the protocol rule that a RESUME acknowledgement is not stream readiness.

Claim sends `action: 2, data: "RESUME"` and polls. Bifrost’s command list includes `RESUME` as a PUT/modify verb, and Geronimo’s “Unknown resume type … Defaulting to NONE” confirms resume type is part of the session object. OpenNOW treats `SESSION_NOT_PAUSED` (`statusCode` 34) as “poll anyway”, which matches `NVB_R_SESSION_NOT_PAUSED` being a distinct result rather than a fatal create error. Session limit is `statusCode` 11, description containing `SESSION_LIMIT`, and unified error `4AF1201E`, mapped to `session_conflict` so the shell lists seats instead of allocating another. That is `NVB_R_SESSION_LIMIT_REACHED` / `NVB_R_SESSION_LIMIT_PER_DEVICE_REACHED` and the `otherUserSessions` field.

Cancellation parity is structural. Bifrost deletes a session that was canceled before the PM seat exists, deletes on poll network failure, and Geronimo refuses `deleteSession` while setup is still in progress. OpenNOW holds one admission lock, waits for a protocol-5 ack for at most ten seconds, then DELETE with an eight-second timeout, and persists `pending-session-cleanup.json` if DELETE fails. The next create returns `session_cleanup_pending` or `session_update_busy`. That is the same “do not start a second seat while the first is unresolved” rule, implemented as core state instead of `NVB_SSS_PREVIOUS_SESSION_CLEANUP`.

`streamer_context` passes the normalized session plus settings into the in-process streamer and forces `transportMode` `nvst`. Official `nv-gfn-streamer=true` plus `NVIDIA-CLASSIC` is the same choice. Software renderers are refused for HDR and for advanced color on Linux Vulkan, which is the OpenNOW form of `nv-renderer-blocklist` plus Geronimo’s decoder-not-supported returns. Frame-rate ceilings (1920×1080 and 1920×1200 up to 360 fps, everything else 240, and 240 without a confirmed hardware decoder and entitlement) line up with `unsupportedFpsIdentifier: 240` being a UI warning threshold rather than a hard protocol maximum.

## Parity gaps

The HTTP create body and the classic NVST attachment path are the parts that already match. The gaps below are the orchestration behaviors this Flatpak implements and OpenNOW does not yet expose with the same controls.

**Client version advertised to CloudMatch.** This Flatpak’s mall build is `2.0.84.127` on Geronimo branch `gs_04_87`. OpenNOW sends `nv-client-version: 2.0.87.131` with `clientVersion` `30.0`, `sdkVersion` `2.0`, and `streamerVersion` `14`. The user-agent SDK tail matches the Bifrost fragment `BifrostClientSDK/`. The marketing build number is ahead of this Linux snapshot. A parity test against this exact Flatpak should treat `2.0.84.127` as the official Linux build under test and `2.0.87.131` as OpenNOW’s current impersonation, which comes from a later client.

**CEF query bus versus core RPC.** Official prepare/start/resume/stop/cancel/get-session/get-active/set-auth/update-ad are `QUERY_GFN_*` calls into `GridApp`, which owns one `NVbClient`. OpenNOW’s RPC methods cover the same verbs, and the Qt shell never sees OAuth tokens. The gap is everything else on that query list that mutates a live seat through Bifrost: streaming-settings update, max bitrate, dynamic streaming mode, L4S, prefilter, vsync, HUD, cloud-gsync capability, and latency-based routing. OpenNOW applies a subset of those at ANNOUNCE or as settings on the next create. It does not expose a mid-session `nvbFeatureControl` / `nvbSendClientInfo` equivalent in the core protocol.

**Setup progress vocabulary.** Bifrost reports `NVB_SSS_CONFIGURING`, `NVB_SSS_STARTINGSTREAMER`, `NVB_SSS_SEATREADY`, `NVB_SSS_QUEUEPOSITION`, and `NVB_SSS_PREVIOUS_SESSION_CLEANUP`, with queue position and `seatSetupEta` in milliseconds. OpenNOW surfaces numeric `status`, `phase`, `queuePosition`, and `seatSetupStep`. ETA and the named setup states are not part of the normalized session. Mall UI that waits on those names has no direct core field.

**Poll and delete policy.** Bifrost polls inside `GridServer`, pauses the poll for ads, restarts it after `AdsWaitTimeout`, aborts on `FINISHED`, and sends DELETE if the poll itself hits a network error. OpenNOW polls on a 1.5 second Qt timer, at most 60 polls and 90 seconds, and does not treat a transient GET failure as terminal. DELETE runs for ack timeout, user cancel, and explicit stop. A poll network error keeps the seat. That is safer for a flaky link and different from Bifrost’s “polling failed, DELETE” path. Ads do not suspend OpenNOW’s poller; `session.ad.report` and `normalize_ad_state` record the opportunity, without the creative sort order (webm first on Linux), force-play timeout, or grace period from `opportunity`.

**Join, forward, transfer, rating.** Bifrost command names include `JOIN`, `FORWARD`, `TRANSFER`, and `SESSION_RATING` in addition to `POST`, `RESUME`, and `DELETE`. `nvbJoinSession` is an exported symbol with its own invalid-params check. OpenNOW claim is only `RESUME`. There is no join of another user’s seat, no transfer, and no session-rating POST.

**Network test and LBR.** Official code has synchronous and async latency and capability tests, a cancel, `maxZonesPerFingerprint` 3, prefetch, max-bandwidth tests, five cached latency samples, and deterministic server routing behind `QUERY_GFN_LATENCY_BASED_ROUTING`. OpenNOW can attach one `networkTestSessionId` and has a separate zone/queue selector for free-tier NVIDIA launches. It does not run `nvbTestNetworkCapability` profiles or the LBR cache as part of create.

**Auth refresh callback.** Bifrost blocks the request until `NVB_EVT_UPDATE_AUTH_TOKEN` returns a new Jarvis/GFN JWT, then retries. The mall wires `refreshAuthToken` into start parameters. OpenNOW captures the token before POST and can renew ServiceId credentials for cleanup when the original account is selected. It does not implement the in-flight “abort request N, retry with a token the SDK just asked for” callback. Certificate pinning UI is similarly absent: `NVB_EVT_SSL_CERTIFICATE_NOTIFICATION` can reject the connection, while OpenNOW uses the process trust store.

**Session document fields Geronimo requires and OpenNOW does not promote.** The native deserializer insists on `networkSessionId`, `rtspSessionId`, `streamSessionId`, and `streamSubSessionIds`. OpenNOW keeps raw `connectionInfo` and derived RTSPS, ICE, and signaling URLs, and copies `subSessionId`. Those four stream identifiers are not first-class fields on the normalized session. If a seat returns them only inside nested objects, the in-process streamer never sees them as named ids. `parentSessionId` is always null. Mall metadata `ClientImeSupport` and `clientPhysicalResolution` are not in `build_create_body`; OpenNOW metadata is `SubSessionId` plus `surroundAudioInfo=2`. `keyboardLayout` is applied by Geronimo at launch and again at upstream-ready. OpenNOW has language preferences and does not copy a layout string into `NVbSessionParams`.

**userAge and controllers.** OpenNOW sends `userAge` 25. The official start path passes the account’s `userAge` and `contentRating`. Controllers are fixed as available `[2]`, preferred `2`, bitmap 0, and `supportedHidDevices` 0 with null `hidDevices`. Geronimo on this build synthesizes DS4/DS5, honors generic HID vid/pid lists, and can set `remoteControllersBitmap`. Linux config also disables wheels and flight controllers. OpenNOW’s gameplay input path is separate; the create body does not advertise the local HID set the way `nvbSetAvailableInputDevices` does before `nvbStartSession`.

**Remote config overrides.** Official `GeronimoSettingsImpl::overrideNVbSessionParams` can rewrite the session struct from GXT/rconfig after the mall has chosen resolution and features. OpenNOW resolves codec and color from saved settings plus embedded `runtimeCapabilities` before POST, and the server’s `finalizedStreamingFeatures` overlay the request afterwards. There is no second JSON document that patches `NVbSessionParams` between prepare and start.

**Serenity recording versus native recording.** `useSerenity: true` and `recordCodec: "h264"` mean the official Linux client transcodes the seat to H.264 for local record when the hardware decoder cannot transcode the live codec, and it will drop resolution to a supported transcode size. OpenNOW records the negotiated encoded video and Opus into Matroska without a second encoder. Replay is a bounded packet buffer. Highlights, ShareServer `autoHighlights`, and the Serenity RTSP handler have no counterpart. `frameStatsEnabled` / `summaryStatsEnabled` / `networkCapture` map to `nvbCollectStatistics` and network-capture feature flags. OpenNOW has streamer stats and diagnostics stages; it does not toggle `NVB_FEATURE_RECORD_STATS_CLIENT` or server-side trace recording.

**Frame-loss and reconnect timers.** Official start parameters carry 500 ms warning and 30 s error timeouts, and the mall will auto-resume for up to five minutes with a five-second refresh, classifying NVST errors with the `8004xxxx` regex. OpenNOW’s reconnect budget is the poll deadline plus eight recovery attempts with backoff up to eight seconds, reset only by a presented frame, and an exhausted video SETUP (`missing-video-peer`) stops automatic recovery. The 500 ms / 30 s pair is not applied as a client-side frame-loss fault. `analyticsTimeout` 10000 has no OpenNOW setting.

**Termination reasons.** Bifrost emits distinct notifications for idle timeout, entitlement timeout, session time limit, operator kill, windowed mode, TDR, process integrity, and multiple login, and Geronimo forwards `resumable` plus `sessionAlive` with a 32-bit reason and extended code. OpenNOW maps CloudMatch status 7 to `phase: finished` with `termination.source = cloudmatch-session-status` and `resumable: false`, and it refuses to invent a vendor reason. Transport EOF is `source: nvst-transport` with `resumable: null`. The official reason enum is the gap. Idle-warning fields `userIdleWarningTimeoutInMs` and the approaching-timeout notifications are not surfaced.

**Window and renderer policy.** Official streaming wants fullscreen (`requireFullscreenInStreaming`), a black custom window, SDL vsync, HiDPI, a 640×360 minimum, and an async EGL renderer, with software GL blocked. Linux packaging turns on Gamescope HDR. OpenNOW’s Qt shell keeps the stream item alive under overlays, confirms before ending a session, and takes HDR from the current output (`nativeHdrSupported`, `nativeHdrDisplay`) rather than from `ENABLE_GAMESCOPE_HDR`. `mirrorClientHDRProperties` is false in this config; OpenNOW copies validated min/max nits when the compositor reports them and otherwise uses the 1000 / 0 / 400 requested-content defaults. Primary chromaticities stay at protocol defaults on both sides when no capture defines a scale. The behavioral gap is Gamescope-specific HDR and the forced-fullscreen mall policy, not the CloudMatch HDR bits.

**Multi-stream and server maintenance results.** Bifrost allows more than one stream only up to an NVSC maximum and rejects zero. OpenNOW sends one monitor. Result codes for patching, region hold, storage, GES, invitation-only registration, and queue abandonment exist in the SDK and collapse in OpenNOW to generic upstream or HTTP errors unless they match the session-limit or not-paused special cases.

**What already matches and should stay stable.** The create URL family, `GFNJWT` authorization, NVIDIA-CLASSIC and NATIVE client headers, LCARS client id, `secureRTSPSupported`, `useOps`, stereo `audioMode` 2 with `requestedAudioFormat` 0, monitor width/height/fps, HDR trio (`sdrHdrMode`, monitor `sdrHdrMode`, `trueHdr`), bit-depth and chroma enums, omission of codec from the POST body, RTSPS endpoint extraction, status 2/3 as the only attachable phases, status 7 as terminal, RESUME-then-poll, session-limit conflict handling, and single-flight create with DELETE compensation. Those are the parity surface to protect when the gaps above are closed.

Static inspection cannot show timing, exact enum integers for every `NVB_R_*` and `NVB_PU_*` name, or the bytes of a live POST from this Flatpak. It can show the exported calls, the URL templates, the JSON keys, the query names, and the config values the mall passes into `GridApp`. OpenNOW already speaks the subset of that contract that allocates a seat and attaches NVST. The remaining official surface is progress reporting, ads-aware polling, auth-refresh callbacks, LBR and network-capability tests, join/transfer, Serenity H.264 recording, frame-loss timers, and the richer termination enum.