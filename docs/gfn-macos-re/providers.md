# Provider routing and session configuration

This document covers the supplied macOS GFN artifacts, not a verified live service. The supplied helper and streamer `Info.plist` files declare version `2.0.87.131`. Alliance login remains unknown because the full account application/configuration resources are missing. The telemetry JSON/configuration entries are dangling symlink placeholders; their target contents were not supplied. No target executable was run and no GFN connection was made. See [Artifacts](artifacts.md) and [Method](method.md) for provenance and scope.

## Evidence conventions and sources

`D` means a decoded instruction or decompiled call or branch. `S` means a symbol or literal is present. `I` means an inference. `U` means unknown. Unless stated otherwise, every address refers to the x86_64 thin Mach-O slice. A literal's `foff` is its thin-file offset, not its fat-container offset. Bifrost and Geronimo literal addresses happen to equal their thin offsets in the cited sections. The executable uses the `0x100000000` image base.

The inspected identities are listed below. [Artifacts](artifacts.md) describes the original files and slices.

| Short name | Inventory path | x86_64 slice SHA-256 |
| --- | --- | --- |
| Main | `macos/MacOS/GeForceNOW` | `2c7fe76d605315df74fbbc112578a883a6ae69c3f6a002558170d53d390c8b4c` |
| Bifrost | `archive/libBifrost2.dylib` | `e145b333045dd66652ffcc6e75a2d301bff54c74064b1193e57afe9d498971e9` |
| Geronimo | `archive/libGeronimo.dylib` | `f564aa1910f1a366973ce3c0f0e6261122833bc41c530a669ec846a51d50d15a` |
| Network SDK | `archive/dependencies/libNetworkTestSDK.dylib` | `23f7607ee23cf885e268d4b6729734a3ed307425b0bd68f7e839b65fc9bb2842` |
| Agent | `archive/nvc/GFN/libGameStreamClientAgent.dylib` | `0df62e75b1491b78654ab81b7bdf6047326b51cdc908445c36a0a6caac787693` |

The routing/auth batch has evidence ID `ev_df413a2254c91df99ecfc94734391097a195321b85e58195797b3b5dd0242ad2`. The startup/dispatcher batch has evidence ID `ev_7d61adb9093f23fedb850d9bedd6ad247ac77c9c4c6ac8dd786a05d77f7b91cd`. Both batches report `ok` for each requested function. Ghidra leaves many CEF virtual calls unnamed, and the dispatcher warns that type propagation did not settle. Some recovered aggregate arguments have inaccurate-looking types. These inferred signatures are not recovered ABI definitions. Bifrost follow-up findings below use evidence `ev_d1ab53c09326e0e81c00125c87528abf0c329f85fdad9a680d7a2cd8ae890255`. Main queries are replayable with [queries/providers.json](queries/providers.json); Bifrost queries use [queries/transport.json](queries/transport.json).

## Provider identity is distinct from service endpoint selection

**S.** Geronimo contains an agent IPC vocabulary with `USER_INFO_PAYLOAD` at `0xdaf321`, `userId` at `0xdaf333`, `externalUserId` at `0xdaf33a`, and `providerId` at `0xdaf349`. Each literal's `foff` equals its address. The agent contains `providerId` at `0x144a05` and a nonempty-identity check message at `0x13e239`. This proves that the binaries represent a provider identity. It does not prove that the agent selects an alliance or obtains credentials.

**S.** Bifrost contains `GFNPartnerJWT ` at `0xc6e0ca` and auth-type names `NVB_AUTH_JWT` at `0xc69d5c` and `NVB_AUTH_JWT_GFN` at `0xc69d69`. Network SDK contains `NCT_AUTH_JWT_PARTNER` at `0x749bdf`, `NCT_AUTH_JWT_GFN` at `0x749bf4`, and scheme-like literals `GFNPartnerJWT` at `0x74a755` and `GFNJWT` at `0x74a763`. These are authentication vocabulary, not recovered numeric wire enums or an alliance-specific login flow.

**U.** Provider catalog, alliance discovery, OAuth initiation URL, client identifier, redirect validation, token exchange, renewal, provider-specific storage, and country routing are not established. There is no evidence here for inventing a partner login route. `loginids.json` in Main at `0x100100e07`, `foff=0x100e07`, names an unavailable file, not its schema.

## Session orchestration boundary

**S.** Main exposes query names `QUERY_GFN_SET_AUTH_INFO` at `0x1000ffd4c`, `QUERY_GFN_SET_AUTH_TOKEN` at `0x1000ffd64`, and `QUERY_GFN_LOGIN_STATUS` at `0x10010003d`, with thin offsets `0xffd4c`, `0xffd64`, and `0x10003d`. Its native dispatcher is `GFNQueryHandler::OnQueryNative` at `0x1000c3660`. Nearby literals include `address`, `deviceId`, `clientAppVersion`, `clientHeaders`, `headerName`, and `headerValue` at `0x10010991c`, `0x100109929`, `0x100109932`, `0x100109943`, `0x100109951`, and `0x10010995c`. The `serverType` and `serverAddress` literals are at `0x1001099f3` and `0x100109a12`. These are context-field candidates until the dispatcher calls are traced.

**S.** Geronimo names the conversion boundary explicitly. `SessionControl::SessionControllerImpl::prepare` is at `0x94d50`, `setAuthInfo` at `0x957d0`, `setNVbSessionParams` at `0x95880`, and the higher-level `startSession(SessionParameters const&, NVbTracingContext_t const&)` at `0x95fa0`. The raw `startSession(NVbSessionParams_t const&)` overload is at `0x95ef0`. `resumeSession` overloads are at `0x96340` and `0x96410`.

**S.** Bifrost supplies `NVB::BifrostClient::initialize` at `0xf110`, `setAuthInfo` at `0xf8b0`, `startSession` at `0x10240`, `joinSession` at `0x11300`, and `resumeSession` at `0x118a0`. It contains the literal `Reconfiguring server details received via NVbSessionParams` at `0xc688e8`. `GridServer::provisionSeat` at `0x20fc0`, `requestSession` at `0x22a50`, `pollSession` at `0x255f0`, and `PopulateSessionRequestData` at `0x27060` identify the server-request owner candidates.

**D.** Main `GFNQueryHandler::OnQueryNative`, `0x1000c3660`, reads prepare keys including `address`, `port`, `deviceId`, `clientAppVersion`, and `clientHeaders`. The optional header branch reads `headerName` and `headerValue`, and the prepare path calls `GridApp::prepare`. Its auth-info path reads `tokenType` and `token`, then calls `GridApp::setAuthInfo`. Its start and resume paths call `GridApp::start` and `GridApp::resume`. These are direct named calls in REA evidence `ev_7d61adb9093f23fedb850d9bedd6ad247ac77c9c4c6ac8dd786a05d77f7b91cd`. The CEF getters are indirect, and aggregate arguments are imperfectly typed, so the complete serialized schema is not recovered.

**I.** The remaining path is Geronimo session-parameter conversion to Bifrost session provisioning. The symbols make that division plausible, but do not establish every call's order or the full field layout. Neither Main's query dispatcher nor Bifrost's symbols prove that either component performs interactive alliance login.

**S.** Bifrost has endpoint templates `https://%s:%d/v2/session` at `0xc6a2f0` and `https://%s:%d/v2/session/%s` at `0xc6a309`. Network SDK has `/v2/nettestsession` at `0x74a545`. These are literal templates, not proof of an exercised route, a production hostname, HTTP method, or provider-specific endpoint. The template placeholders must remain unresolved until a caller supplies them.

## Endpoint, proxy, and environment configuration

| Evidence | Finding | Limits |
| --- | --- | --- |
| S, Main `BrowserApp::OnBeforeCommandLineProcessing`, `0x10000fa70`; literals `0x1000fe628`, `0x1000fe65a`, `0x1000fe681`, offsets `0xfe628`, `0xfe65a`, `0xfe681` | The executable contains `nv-proxy-server`, `nv-proxy-bypass-list`, and `nv-proxy-pac-url`. | Strings alone do not establish forwarding, precedence, native-media scope, or PAC evaluation. |
| S, Main literals `0x1000fe638`, `0x1000fe64d`, `0x1000fe66f`, `0x1000fe692` | It also contains `no-proxy-server`, `proxy-server`, `proxy-bypass-list`, and `proxy-pac-url`. | These resemble CEF switches. They do not establish Bifrost or NVST proxy behavior. |
| S, Main `0x100101428` and `0x100101434`, offsets `0x101428` and `0x101434` | `config-name` and `config-path` are command-line/configuration vocabulary. | These are not proven process environment variables. |
| S, Main `0x1001023cd` and `0x1001023e2` | `shared/assets/config` and `config.json` are resource-path vocabulary. | The supplied resources do not include their contents. |
| S, Main `0x10010169a`, `0x1001016b5`, `0x1001016d2` | `nv-nvtelemetry-config-path`, `nv-telemetry-api-binary-path`, and `nv-nvtelemetry-config-path-override` name telemetry configuration inputs. | Their consumers and override precedence need call tracing. |
| S, Main `BrowserApp::GeronimoInit`, `0x100014020`; literals `0x1000ff042`, `0x1000ff04a` | `STAGING` and `PRODUCTION` appear next to a tracing-environment log. | This does not prove the session-service environment switches with telemetry. |
| S, Geronimo `0xee9ffb` and `0xeea007`; Agent `0x145ee3` | `environment` and `ephemeralSessionConfiguration` appear as selector/key strings. | The exact configuration schema and lifetime are unresolved. |
| S, Network SDK `0x76b795`, `0x76b7a1`, `0x76b7ac`, `0x76b7b8`, `0x76b7c3`, `0x76b7cc` | `https_proxy`, `http_proxy`, `HTTPS_PROXY`, `HTTP_PROXY`, `no_proxy`, and `NO_PROXY` are present in bundled networking code. | They are not proven GFN application settings. A reachable application call is required before claiming support. |

**S.** Main has server-type conversion log names for Nimbus, GameStream, roaming proxy, cloud station, Quadro station, passthrough, and unknown at `0x10010968c`, `0x1001096bf`, `0x1001096f6`, `0x100109730`, `0x10010976a`, `0x1001097a5`, and `0x1001097da`. These do not recover the input numeric values or prove a routing fallback sequence.

**D.** Main `BrowserApp::OnBeforeCommandLineProcessing`, `0x10000fa70`, branches on command-line virtual slot `0x88` with `nv-proxy-server`. If false, slot `0xa0` receives `no-proxy-server`. Otherwise slot `0x90` retrieves a value. If that value differs from the literal `true`, slot `0xa8` receives `proxy-server` with that value, followed by corresponding `nv-proxy-bypass-list` to `proxy-bypass-list` and `nv-proxy-pac-url` to `proxy-pac-url` calls. If the value equals `true`, the explicit translation block is skipped. This is the observed CEF command-line branch, not proof of native-media proxy support. The virtual methods remain unnamed in Ghidra. Source is evidence `ev_7d61adb9093f23fedb850d9bedd6ad247ac77c9c4c6ac8dd786a05d77f7b91cd`.

**D.** Main `BrowserApp::GeronimoInit`, `0x100014020`, obtains a shared configuration path, constructs `JsonStorage`, and tests `IsValid`. An invalid storage object takes a logging branch before execution continues. It does not immediately abort all startup at that check. The tracing-init branch derives its environment value from a stored byte at offset `0x5a2` of an object reached through `this+0x378`, XORs the value with `1`, logs `STAGING` for zero and `PRODUCTION` otherwise, and calls `TracingDispatcher::Initialize`. The setting's external name is unresolved, so this is not a recovered public environment switch. Source is the same startup evidence.

**D/U.** Main `GetAppConfigurationPath`, `0x1000583d0`, calls `GetAppBundlePath`, builds a `/Contents/Resources/mall/` suffix, and performs subsequent string appends. The decompiler warns that type propagation did not settle and that the calling convention is unknown. The append arguments are missing from the rendered pseudocode, so the full constructed path is not established by this body. `BrowserApp::BeforeContextInitialization`, `0x100013360`, calls `GeronimoInit` and branches on its returned status. Source is REA evidence `ev_838d9102603da8d873957e74590577161515ef564c3c6b661fe4bb4b66eb8a64`.

## Network tests and routing are separate from login

**S.** Main owns named latency-routing adapters `GFNQueryHandler::PerformLatencyBasedRouting` at `0x1000d3980` and `LBRAsyncResultCallback` at `0x1000d6780`. Its result vocabulary includes `results`, `ipAddresses`, `networkSessionId`, `resultCode`, `resultText`, and `cacheResult` at `0x10010abe2`, `0x10010abea`, `0x10010abf6`, `0x10010ac07`, `0x10010ac12`, and `0x10010ac1d`. Those literals' thin offsets are the address minus `0x100000000`.

**S.** Bifrost has `NetworkTestController::runCapabilityTest` at `0x4e7c0`, `executeCapabilityTestFunc` at `0x4ea80`, `runLatencyTest` at `0x4ee10`, `executeLatencyTestFunc` at `0x4f1e0`, and `cancel` at `0x4fcd0`. It also has an internal `NetworkTester` family with `measureRTTLatency` at `0x573900`, `measureBandwidth` at `0x574120`, and `runLatencyTest` at `0x578770`. A second family exists in Network SDK. This does not prove which implementation a configured test selects.

**S.** Network SDK has `SessionRequestTask::runTask` at `0x2fdd0`, `readRequestStatus` at `0x32620`, `readNetTestSession` at `0x334a0`, and `readNetTestThreshold` at `0x352d0`. `NetworkTestTask::runTask` is at `0x206a0`, `fillTestResult` at `0x22250`, and `isLatencyTest` at `0x225f0`. Its literal result fields include `requestStatus` at `0x74a8d4`, `serverId` at `0x74a979`, `netTestSession` at `0x74a9cf`, `sessionId` at `0x74aa24`, `hmacKey` at `0x74aa2e`, `maxBandwidthMbps` at `0x74aa36`, and `connectionInfo` at `0x74aa47`. No field values or credentials were extracted.

**S.** SDK logs distinguish no-stream latency, in-stream latency, downstream bandwidth, upstream bandwidth, loss, packet jitter, and MTU at `0x749811`, `0x749eee`, and `0x749f24`. Profile-limit and recommendation logs occupy `0x74938d` through `0x74946d`. These establish reported metric names and units, not recommendation thresholds or an enforced admission policy.

**D.** Main `PerformLatencyBasedRouting`, `0x1000d3980`, reads `addresses`, `user`, `deviceId`, and `platformId`, accumulates callback consumers, and calls `_nvbTestNetworkLatencyAsync` with `LBRAsyncResultCallback`. A branch with callbacks already present adds another callback without issuing that call again. Source is evidence `ev_df413a2254c91df99ecfc94734391097a195321b85e58195797b3b5dd0242ad2`. The API argument layout is imperfectly typed in the decompiler, and the actual tester implementation remains a Bifrost query.

**D.** Main `LBRAsyncResultCallback`, `0x1000d6780`, starts its minimum at `100` and replaces the selected index only for a strictly smaller unsigned measurement. A `-1` measurement therefore does not win that comparison. Its success branch requires a minimum below `100` and a result code present in a locally constructed lookup table. It returns the selected `url` and `latency`, and sets `cacheResult` only when the result code equals zero. The log calls the measurements milliseconds. Equal measurements leave the earlier index selected. The nonzero recognized-code branch logs partial results. A failure branch calls `CallbackFailure` with local application code `0x50003`. The lookup table's accepted nonzero code meanings were not recovered, and these values are not wire enums. Source is the same routing evidence.

**D.** Bifrost `NetworkTestController::executeLatencyTestFunc`, `0x4f1e0`, constructs its internal `NetworkTester`, attaches cancellation state, and uses `setupUDPSocket` with port value `0x1389`, or 5001, for this test path. It obtains a numeric address, checks `LatencyTestContext::isLatencyTestNecessary`, calls the internal `NetworkTester::runLatencyTest` when needed, and records latency on result zero. Duplicate-address suppression and result/counter branches are separate. This establishes the tester selected by this specific worker, not by every capability-test API or every provider. Source is the Bifrost follow-up evidence. The packet exchange, retry budget, and cancellation callback implementation were not traced.

## Error and fallback ownership

**S.** Bifrost has `requestAuthTokenUpdate` at `0x163d0`, `ShouldUpdateAuthToken` at `0x165a0`, and `GridServer::updateAuthToken` at `0x18e50`. Its logs say `Client did not update authentication token. Aborting web request` at `0xc6a3cd`, `performWebRequest failed with RS_AuthenticationError` at `0xc6c2f1`, and `Retrying the WebRequest with the updated authinfo` at `0xc6c326`. The trigger statuses, retry count, time budget, and callback synchronization remain unknown until these functions are traced.

**D.** Main `SimpleGridApp::updateAuthToken`, `0x1000e4a10`, clears its saved token, emits an application callback containing `event=UPDATE_AUTH_TOKEN` and `spanData`, locks a mutex, and performs one condition-variable timed wait with a 30-second budget. If the elapsed steady-clock interval is shorter than 30 seconds and the saved token is nonempty, it copies the token to the callback's destination and appends a terminator. Otherwise it logs empty-token or expiry failure. The body has no visible predicate-wait loop. `setAuthToken`, `0x1000dc6b0`, assigns the saved string and calls `notify_all`. Its dispatcher caller is the `SetAuthToken` branch in `OnQueryNative`, `0x1000c3660`. Source IDs are the routing and startup evidence above. The waiting thread, destination buffer contract, and race protection outside these bodies remain unknown. This is native-to-application token refresh, not native execution of an alliance login exchange.

**D.** Bifrost `WebRequestTask::performWebRequest`, `0x5b490`, takes the SSL-backend fallback when response error is local value `-15` and a configured HTTP-client object exists. It saves that object's configuration, selects a null/default HTTP-client configuration, reconfigures the request, and repeats its request loop. If the fallback succeeds, the new backend remains selected; if it fails, the saved backend is restored and the request is reconfigured again. Source is the Bifrost follow-up evidence. The log names the path `Retry with fallback to non OS HTTP for SSL error`. This is not proof of disabling certificate verification, downgrading to cleartext, changing alliance identity, or falling back to TCP media. Other retry branches and their scheduling are not collapsed into this backend-swap condition.

**D.** Bifrost `ShouldUpdateAuthToken`, `0x165a0`, is a local result predicate. One branch accepts leading result value `0x1fb`. A separate `0x97` branch requires a nonzero field at byte offset 12 and examines the 32-bit field at byte offset eight: values outside `0x98..0x9c`, or value `0x9b` inside that range, return true. These are recovered result-structure offsets and local numeric tests, not a decoded HTTP-status table or partner-auth wire enum. Source is the Bifrost follow-up evidence. The caller's retry count and all result-field names remain unresolved.

**S.** Main `ConfigureStreamerVideoSettings` at `0x1000cf690` contains a nearby `fallbackToLogicalResolution` literal at `0x10010adbf`. Bifrost also contains that key at `0xc6bd66`. Bifrost's dynamic reserved-port fallback log is at `0xc74e54`, while Main's 4K recording fallback query is at `0x1001001fa`. These are different fallback domains. No evidence supports collapsing them into one general retry setting.

**U.** The traced callbacks and Bifrost follow-ups establish one auth-refresh time budget, conditional partial latency success, a specific HTTP-backend swap, and a local auth-refresh predicate. They do not establish every Bifrost retryability mapping, provider-switch behavior, service-request timeout default, or cross-domain fallback precedence.

## Further native queries

These additional x86_64 queries would resolve remaining context and authentication gaps:

- `0xd520`, `initializeOnServerInfoResponse`, and `0xf110`, `initialize`, for server-type and supported-auth validation.
- `0xf8b0`, `setAuthInfo`, and `0x5ca50`, `createAuthorizationHeader`, for token-type validation and scheme selection without exporting token material.
- `0x163d0`, `requestAuthTokenUpdate`, and `0x18e50`, `GridServer::updateAuthToken`, for the callers of the recovered refresh predicate and synchronization.
- `0x10240`, `BifrostClient::startSession`, `0x27060`, `PopulateSessionRequestData`, `0x22a50`, `requestSession`, and `0x255f0`, `pollSession`, for context serialization, endpoint sources, and setup lifecycle.
- `0x4ee10` and the internal tester called by `0x4f1e0` for full result ordering, packet exchanges, and cancellation.
- `0x19b00`, `PerformWebRequest<ServerInfo>`, and the callers of `0x5b490` for HTTP-backend recovery versus higher-level authentication retry.

Geronimo `0x94d50`, `prepare`, `0x95880`, `setNVbSessionParams`, `0x95fa0`, `startSession`, `0x94130`, `handleNVbCallback`, and `0x94720`, `processEvents` can refine the context/event path. `GeronimoSettingsImpl::updateRemoteConfig`, `0x71620`, and `TracingDispatcher::setStagingEnvironment`, `0x251d10`, can resolve configuration ownership. See [video.md](video.md) for any context conversion actually traced during this study; the remaining functions are proposals, not completed observations.

## Compatibility recommendation

**Recommendation.** Keep provider identity, interactive authentication, service orchestration, endpoint selection, and media-session execution separate. Pass one complete typed session context to the native streaming layer, and preserve explicit auth-refresh and setup-failure notifications. Do not copy CEF into OpenNOW or guess alliance server routes from these strings. Retain separate settings for HTTP-backend recovery, latency routing, media endpoint recovery, resolution fallback, and recording fallback until their conditions are verified.
