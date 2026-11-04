# Windows CloudMatch native models and conversion

Static evidence from the separately supplied Bifrost2.dll, `gs_04_92 38967594`, image base `0x180000000`. VAs identify Windows functions; model offsets are private implementation details, not a public ABI. No target executable or server was used. The logged exchange is catalogued separately in [cloudmatch-fields.md](cloudmatch-fields.md).

## Request and response ownership

**D/S.** Bifrost owns typed JSON models, not only raw JSON strings. RTTI identifies `SessionRequestData` at `0x181026458`, `StreamingFeatures` at `0x181026250`, `Session` at `0x181027008`, `ConnectionInfo` at `0x181026e88`, `RequestStatus` at `0x181026be8` and `ServerInfo` at `0x181026cd8`.

The registrations below append records of 24 bytes containing a field-name pointer, a local type tag and an address within the model. These establish the schema-to-member association. Their offsets are model offsets, **not** recovered public `NVbSessionParams_t` ABI offsets.

| Role | Windows VA | Evidence and boundary |
| --- | --- | --- |
| Request population | `0x1807bc110` | `0x1807d0248` calls it with destination `SessionRequestData` in `r9`, session-input address in `rcx`, supported-controller collection in `rdx`, and client-context address in `r8`. Input ownership and selected conversions are traced below. |
| Session-request model registration | `0x1807c2b50` | Registers all 32 request keys listed in the appendix. |
| Requested feature model registration | `0x1807c3760` | Registers `qosPolicy`, `touchSupport`, `dlssOverrides` as well as the earlier features. These are not log-only guesses. |
| Request envelope registration | `0x1807c2af0` | Registers `sessionRequestData` at envelope `+0x8`, type tag 3. |
| Serialize and send session request | `0x1807d00d0` | Allocates/populates a request model, serializes its envelope via `0x180839980` at `0x1807d035e`, and constructs a POST task. |
| Session-response envelope registration | `0x1807c2740` | `session` at `+0x30`, `requestStatus` at `+0x38`, `otherUserSessions` array at `+0x8`. |
| Active-session response registration | `0x1807c2850` | `sessions` array at `+0x8`, `requestStatus` at `+0x30`. |
| Session model registration | `0x1807c1ee0` | Separates echoed `sessionRequestData`, finalized features, final monitors, controlling endpoint, media endpoints, status and session error. |
| CloudMatch poll | `0x1807cc4f0` | Constructs GET tasks, updates cached controlling host/port when forwarded, and handles finished-session and advertisement gates. |
| CloudMatch endpoint conversion | `0x1807c5ea0` | Converts response `connectionInfo` elements into a native endpoint vector, using the explicit usage rules below. |
| Native stream-client setup | `0x1807ec080` | Copies up to 20 converted endpoints, then prepares the stream client. |
| Final session configuration | `0x1807e89c0` | Matches monitors by ID, merges server and client video settings, and initiates streaming. Selected merge rules are traced below. |

**D.** Request population contains a title-specific branch: input `appId == 0x00f4c0af` writes `internalTitle = "desktop_pro"` at `0x1807bc169..0x1807bc17f`. This is a narrow special case, not evidence that every app has a title synthesized from its numeric ID.

**D.** In request population, input `appLaunchMode` at session-input `+0x34` maps 1 to JSON 2, `GAMEPAD_FRIENDLY`; 2 to JSON 3, `TOUCH_FRIENDLY`; every other value to JSON 1, `DEFAULT`. Instructions near `0x1807bc30b..0x1807bc350` and the analyzed function agree.

**D/S.** `partnerCustomData` belongs to the typed session request at `SessionRequestData+0x1f8`. This proves native ownership of the serialized field. It does not establish partner login, the content's semantics, or whether every partner session supplies it.

**D.** Requested and finalized features use separate instances of the **same** `StreamingFeatures` model. `SessionRequestData+0x230` and `Session+0x138` both construct their nested object with `0x1807b6000`; that constructor installs `StreamingFeatures::vftable` and registers `0x1807c3760`. Raw reset paths `0x1807cbba0` and `0x1807cb870`, plus the exported constructor, establish this association.

### Which producer owns a request field

**D.** `0x1807bc110` separates four inputs: the session input, the supported-controller collection, client context, and output JSON model. It does not perform provider discovery or choose a login route. Context offsets below are relative to the third input, not to a public C ABI structure.

| Output | Source in this population path |
| --- | --- |
| `appId` | Session input `+0x0` |
| `preferredController`, `availableSupportedControllers` | Session input `+0x4` and separate supported-controller collection, through controller converter `0x1807bc070` |
| `clientIdentification` | Client context string `+0x0` |
| `clientVersion` | Client context string `+0x20` |
| `deviceHashId` | Client context string `+0x40` |
| `sdkVersion` | Client context string `+0x60` |
| `streamerVersion` | Client context string `+0x80` |
| `clientPlatformName` | Client context string `+0x100` |
| `useOps`, `audioMode`, `secureRTSPSupported` | Bifrost population writes constants `true`, `2`, `true` |
| `remoteControllersBitmap` | Session input `+0x30` |
| `partnerCustomData` | Session input string `+0x90` |
| `accountLinked`, `enablePersistingInGameSettings`, `userAge` | Session input `+0xd0`, `+0xd2`, `+0xd4` |
| `clientTimezoneOffset` | OS timezone information obtained locally, converted to milliseconds with sign reversal and daylight adjustment |
| `metaData` | Copies the incoming session metadata map, then adds `surroundAudioInfo` as a decimal string from session-input byte `+0xd1` |
| Requested monitor/HDR settings | Input stream-setting collection, with a `0x18c`-byte element stride |
| Requested feature settings and DLSS | Input stream-setting fields, plus a separate HID collection; not server response values |

`wssignaling`, `SubSessionId`, `ClientImeSupport`, `networkType`, and `clientPhysicalResolution` are not synthesized by this population function. Their presence in the logged request is consistent with the copied input metadata, not evidence that this native function derives those values.

**D.** Requested features convert native bit depth 10 to JSON `bitDepth=1`, all other native values to 0; native chroma 3 to JSON `chromaFormat=1`, all other values to 0. These conversions occur in `0x1807bc110` while copying the stream-setting fields. They prove separate JSON and native-video enum domains. The final SDP enum is a separate signaling question.

**D.** `qosPolicy=0` and `touchSupport=false` are registered fields with zero defaults in constructor `0x1807b6000`, not copied from the stream-setting input by `0x1807bc110`. The normal fresh-request path allocates that object, populates the copied settings, copies the request into its serialization envelope and serializes it. The logged zero/false values fit this path. This does not establish that every other API path must leave them zero.

**D.** The global requested-feature object is reused while iterating monitors, so its copied per-stream values are overwritten by later elements. By contrast, top-level HDR capability construction is gated on the first stream being HDR. Do not assume one distinct global requested-feature object per monitor.

### Response to stream-setting conversion

**D.** `0x1807c5ea0` converts each server monitor to a native setting with `0x1807ed310`, then overlays non-null `Session.finalizedStreamingFeatures` through `0x1807ed730`. If no server monitors remain, it falls back to the client stream-setting collection. Thus the echoed request is not used as the negotiated feature authority.

`0x1807ed730` maps JSON bit-depth 1 to native depth 10, otherwise 8; JSON chroma 1 to native chroma 3, otherwise 0. It copies server-finalized Reflex, cloud G-SYNC, profile, true HDR, L4S, prefilter, HUD, fallback-to-logical-resolution and DLSS fields into the native setting. It does not copy every registered JSON field.

**D.** `0x1807e89c0` matches a server monitor to a requested monitor by ID. On that matched path it limits native bit depth and chroma to the smaller server/client values. It copies several client-only settings while preserving the server's true-HDR enable byte. Its finalized prefilter mode is `min(serverMode, clientMode)`; if the server's mode is 2, sharpness and denoise come from the client settings. This explains the logged requested mode 0, offered mode 2, finalized mode 0 without a timing inference. HUD has its own branch using server mode, requested mode and profile; the full semantic names for its modes remain unresolved.

## HTTP request and polling construction

**D.** Creation uses **POST**, not merely a REST inference. `0x1807d00d0` constructs the four-byte `POST` string and supplies it to request-task constructor `0x1807b38e0`. Polling uses **GET** in `0x1807cc4f0`; its verb bytes are at `0x181027414`. The request-task constructor stores verb, URL and body separately; `0x1807ac200` hands the verb and URL to `0x1807eed10`, which calls exported `nvbutil::WebRequest::create`.

**D.** The create format is `https://%s:%d/v2/session`, literal `0x181025d18`, copied into global template `0x1814adba0` by initializer `0x1801b4100`. The poll format is `https://%s:%d/v2/session/%s`, literal `0x181025d38`, copied into global template `0x1814adbc0` by initializer `0x1801b4090`. The create function appends locale/keyboard query settings through `0x1807bf230`.

**D.** After creation, `0x1807d00d0` caches the returned session ID and `sessionControlInfo.ip` plus port through `0x1807bfdc0`; absent `sessionControlInfo` falls back to the configured server. Polling looks up that cached server through `0x1807c9050`, then formats the fixed `/v2/session/<sessionId>` path. When a response supplies a different controlling host or port, it updates the cache and rebuilds that URL.

This corrects a limit of the earlier log study: the observed path matched `sessionControlInfo.resourcePath`, but the traced native create/poll path **does not consume that response member to construct the poll URL**. It reconstructs the fixed path from the session ID. `connectionInfo[i].resourcePath` is consumed separately as a stream endpoint address fallback.

**D.** `0x1807eed10` adds `Content-Type: text/plain` and the nonempty authorization header built by `0x1807ee9a0` to its HTTP request header vector. This is a native request-builder fact, not a claim that the JSON body uses a JSON Content-Type. Additional application-supplied headers are not catalogued here.

**D.** The creation retry counter starts at 1 and is bounded by the task's configured `+0x140` field; it is not a universal hardcoded request count. A retry also requires the local result predicate `0x1807a9f70` and a non-cancelled pacing path `0x1807ef770`. That pacing helper sleeps the configured delay and increases it from network configuration. The inner HTTP path has separate SSL-backend and authentication recovery. No fixed retry count or total time budget is inferred from the observed polling interval.

## Different usage/type domains

The response JSON usage, public Bifrost endpoint usage, `nvstConnectToServer` input type and internal NVSC config-dump port type are distinct representations. The earlier Windows log study conflated JSON usage 14 with the converted native RTSPS enum. The table below separates them.

**D.** `0x1807c5ea0` reads each JSON `ConnectionInfo.usage` at `+0x58`. Conversion table `0x1807c6694` is indexed by `usage - 1`. Name table `0x1807c6704` is indexed by the unmodified JSON usage. The resulting native record stores protocol at `+0x400`, Bifrost usage at `+0x404` and 16-bit port at `+0x408`.

| JSON usage | JSON enum name | Native Bifrost usage | Native Bifrost name |
| ---: | --- | ---: | --- |
| 0 | UNKNOWN | not appended | not applicable |
| 1 | CONTROL | 1 | NVB_PU_CONTROL |
| 2 | VIDEO | 2 | NVB_PU_VIDEO |
| 3 | AUDIO | 3 | NVB_PU_AUDIO |
| 4 | INPUT | 4 | NVB_PU_INPUT |
| 5 | CUSTOM | 5 | NVB_PU_CUSTOM |
| 6 | USB | 6 | NVB_PU_USB |
| 7 | RTSP | 7 | NVB_PU_RTSP |
| 8 | GAMESTREAM_CONTROL | not appended | not applicable |
| 9 | GAMESTREAM_SECURE_CONTROL | not appended | not applicable |
| 10 | SESSION_CONTROL | not appended | not applicable |
| 11 | NETWORK_TEST_CONTROL | not appended | not applicable |
| 12 | RTSPRU | 8 | NVB_PU_RTSPRU |
| 13 | AUDIO_INPUT | 9 | NVB_PU_AUDIO_INPUT |
| 14 | SIGNALING | 10, conditional | NVB_PU_RTSPS |
| 15 | MEDIA | silently skipped | not applicable |
| 16 | RTSPS | 10 | NVB_PU_RTSPS |
| 17 | BUNDLE | 11 | NVB_PU_BUNDLE |

**D.** JSON usage 14 is appended only for `appLevelProtocol` 6 or 1. At `0x1807c5f17..0x1807c5f27`, the function tests usage 14 and then app protocol. For other app protocols it skips the element before the conversion switch. Usage 16 does not take that pre-filter. Usage 15 is skipped before the switch at `0x1807c5f2d..0x1807c5f30`. Unsupported usages that reach the switch produce the "Skipping unexpected port usage type" warning at `0x1807c6276`.

**D.** Address selection is nonempty `ConnectionInfo.ip` first, then nonempty `resourcePath`; if neither is nonempty, the zero-initialized destination remains empty. The destination has a 1024-byte address region and byte 1023 is explicitly cleared. The relevant instructions are `0x1807c5f47..0x1807c5f76`.

**D.** The public/native endpoint protocol is `3` when JSON `protocol == 1`, otherwise `2`, from `0x1807c5f8a..0x1807c5f96`. This is the exact recovered numeric conversion; no unsupported protocol family names are assigned here.

**D.** `0x1807ec080` caps native endpoints to `min(connectionInfoCount, 20)` at `0x1807ec38c..0x1807ec3cf`. Its warning is demonstrably inverted: it logs "Ignoring the remainder" when count is **less than** 20 (`cmp; jae skip-warning`), not when greater than 20. The prior docs' inference that it fires whenever count differs from max should be replaced with this narrower decoded condition. Actual copying still uses the minimum, so 2 entries are both retained.

**D.** That setup stage copies each source record of `0x40c` bytes to an internal record of `0x20c` bytes. It preserves native usage and protocol, uses the 16-bit port and truncates the internal address to a terminated 512-byte region. This is separate from CloudMatch JSON layout.

**D.** Raw streamer connect function `0x180829e00`, at `0x18082a46a..0x18082a577`, converts the native endpoint usage to the input endpoint type supplied to the NVST connect operation. Table `0x18082b7f4` yields native usages 1→1, 2→2, 3→3, 4→4, 7→5, 9→6, 10→7 and 11→8; native usages 5, 6, 8 and other values are skipped. This function was inspected by disassembly.

It separately converts native protocol 0→connect protocol 1, 2→3, and 3→5, except native usage 10 / connect endpoint type 7 selects protocol 6. Consequently the logged NVIDIA JSON usage 14, JSON protocol 1, application protocol 6 becomes native usage 10/protocol 3, then NVST connect endpoint type 7/protocol 6. **U.** The further internal conversion to the logged config-dump port type 4 is not traced here. Do not call connect endpoint type 7 the logged config enum value 4.

**Comparison.** The Windows decoded rules independently confirm the macOS study's media usage 2/17, signaling usage 14/16 and usage-15 skip. Windows also establishes the usage-14 application-protocol filter, the distinct native enum and the inverted warning. A second eligible endpoint is retained; endpoint retention alone does not prove when it is retried.

## Server types and provider boundary

**D.** The streamer-side conversion at `0x18081f0d0` maps Bifrost type 1 to NVSC type 1, type 2 to NVSC type 3, and every other input to NVSC type 2. The same inline mapping appears at `0x180829a13..0x180829a5b`.

| Bifrost type | Existing application-layer name | NVSC type from this conversion |
| ---: | --- | ---: |
| 0 | NVB_SERVER_NIMBUS | 2 |
| 1 | NVB_SERVER_GAMESTREAM | 1 |
| 2 | NVB_SERVER_ROAMING_PROXY | 3 |
| 3 | NVB_SERVER_CLOUD_STATION | 2 |
| 4 | NVB_SERVER_QUADRO_STATION | 2 |
| 51 | NVB_SERVER_PASSTHRU | 2, if this function receives it |
| other | no additional name recovered | 2, if this function receives it |

The names in that table come from the earlier GeForceNOW.exe study of `0x1401a2c00`; the NVSC mapping comes from this DLL. The fallback is a numeric conversion, not proof that initialization accepts unknown values or that all named types use one protocol.

**D.** Server-info response conversion at `0x1807c88e5..0x1807c8960` maps received JSON server types 1..5 to native types 0..4, and otherwise stores `-1` after the unexpected-type warning. That endpoint-discovery conversion does **not** recognize JSON 1001 as PASSTHRU in this branch. The application-side explicit server type conversion is a different boundary.

**D.** `0x1807ee9a0` reads native auth type at authorization input `+0x20`. Types 1, 2, 3, 4, 7, 8 and 9 choose `PingFederateOAuth2`, `GoogleOAuth2`, `JanrainOAuth2`, `ActiveDirectory2`, `JarvisAuth`, `GFNPartnerJWT` and `GFNJWT` respectively. Types 5, 6, out-of-range values and zero produce no header in this function. Auth type 8 constructs `Authorization: GFNPartnerJWT auth=<token>`; type 9 constructs `Authorization: GFNJWT <token>`. These are format templates only; no token bytes were read or included.

**D.** `0x1807d1ac0` has a separate no-service-request branch for configured server types `0x33..0x35`. `0x1807ce7c0` can construct a local session/connection result from supplied endpoints, with an RTSP-family fallback when none was supplied. These are bypass/passthrough paths. The earlier application study names type 51 as PASSTHRU; types 52 and 53 are not named by the evidence in this pass. Their existence does not prove an alliance seat chooses them.

**U.** Which Bifrost type a real alliance seat uses, provider discovery, interactive login, refresh endpoints, `partnerCustomData` interpretation and provider-specific route selection remain unverified. All supplied logs describe NVIDIA seats. The recovered field and scheme cannot substitute for a partner-session trace.

## Error codes

**D.** `0x80040010` is now named exactly: `NVST_NETERR_CLIENT_DISCONNECT_UNINTENDED_NO_VIDEO_PACKETS_RECENTLY`.

The exported `nvstResultToString` at `0x1801bccc0` computes the `0x80040001..0x80040018` range's index using `code - 0x80040001` and loads an RVA from jump table `0x1801bf828`. Index 15 for this code selects `0x1801bd07f`; that branch returns the literal at `0x18122c970`. This is direct decoded enum evidence, not an interpretation of disconnect reason 55.

**D.** Video-timeout handler `0x180247e20` assigns the same code at `0x180247ff8`. A distinct flag selects `0x80040017` instead at `0x180247ffd..0x180248005`; that flag's owner and exact semantics are not yet resolved. The handler logs last payload/RTP-ping timing and invokes the disconnect path. The enum name does not establish why packets stopped, whether audio was healthy or whether resume can succeed.

**D.** `0xC0F52159` is now named exactly: `NVB_R_USER_STORAGE_NOT_AVAILABLE`. Result formatter `0x1807aa3b0` uses a compressed two-stage switch: code minus `0xC0F52103` indexes byte table `0x1807aa8bc`, then that selector indexes RVA table `0x1807aa850`. For `0xC0F52159`, range index 86 gives selector 24, branch `0x1807aa4d7` and literal `0x1810252c8`. The adjacent `0xC0F5215A` is `NVB_R_GFN_STORAGE_NOT_AVAILABLE`, a different error.

Service-status converter `0x1807ed4b0` maps an otherwise-unhandled service `statusCode <256` to `0xC0F52100 + statusCode`; service status `0x59`, decimal 89, produces `0xC0F52159`. Generic CloudMatch response handling in `0x1807ac200` invokes this converter with `RequestStatus.statusCode`. This identifies the client-side category, not whether the unavailable storage was caused by a local disk, remote storage service, account state or transient server condition.

**U.** `0xC2EC1025` remains unnamed in the inspected enum formatters. `RequestStatus.unifiedErrorCode` is a response integer at `+0x30`; `0x1807cf640` and `0x1807cc4f0` can promote it to the primary result when the existing primary result is server-internal-error or invalid-service-response and the unified code passes `0x1807ed840`, which tests nonzero bits in mask `0x00ff0000`. `0xC2EC1025` passes that numeric test, so this is a plausible propagation path, **not proof** that the observed resume error came from that field. Its source response and readable meaning remain unknown.

The raw DLL contains neither requested composite code as an exact little-endian 32-bit constant. The recovered `0xC0F52159` name shows why such a negative byte search must not be used to infer that a code is unnamed.

## Still-unknown boundaries

- Complete public session-input ABI, every optional/null serialization policy and upstream construction of the copied input metadata.
- Semantic mode names and complete field-by-field client/server merge policy beyond the traced rules.
- Application-supplied header catalogue, configured retry defaults, total retry budgets and complete auth-refresh scheduling.
- Internal NVSC config-dump port-type map and endpoint failover predicate beyond endpoint retention.
- Alliance login and actual partner-seat endpoint/server-type behavior.
- Exact readable meaning and observed source layer for `0xC2EC1025`; operational cause of the now-named storage error.

## Model field layout appendix

The tables below are generated from the decoded registration instructions. Tags are local registration tags: 1 corresponds to Boolean fields, 3 to nested-object pointers, 4 to arrays, 5 to strings and 6 to integer/enum fields in this set. They are not JSON wire numbers. Names and destination offsets were checked directly from `lea` and descriptor stores, not inferred from proximity between strings. A second pass matched 110 registered names to the available analyzed registration-function exports; remaining registration rows retain raw-instruction evidence.

Reproduce the error anchors with:



Reproduce key registration offsets with `descriptors 1807c2b50 1807c3760 1807c1ee0 1807c07e0`, and the endpoint switch tables with `table 1807c6694 11`, `table 1807c6704 12` and `table 18082b7f4 b`. Arguments are hexadecimal, so counts `11`, `12` and `b` mean 17, 18 and 11 entries. The script's hash check prevents accidentally applying these addresses to a different DLL.

### Registration at `0x1807c2b50`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `appId` | `0x8` | 6 | `0x1807c2b69` |
| `externalAppId` | `0x10` | 5 | `0x1807c2bc4` |
| `internalTitle` | `0x30` | 5 | `0x1807c2c15` |
| `availableSupportedControllers` | `0x50` | 4 | `0x1807c2c66` |
| `preferredController` | `0x78` | 6 | `0x1807c2cb7` |
| `networkTestSessionId` | `0x80` | 5 | `0x1807c2d08` |
| `parentSessionId` | `0xa0` | 5 | `0x1807c2d5c` |
| `clientIdentification` | `0xc0` | 5 | `0x1807c2db0` |
| `deviceHashId` | `0xe0` | 5 | `0x1807c2e04` |
| `clientVersion` | `0x100` | 5 | `0x1807c2e58` |
| `sdkVersion` | `0x120` | 5 | `0x1807c2eac` |
| `streamerVersion` | `0x140` | 5 | `0x1807c2f00` |
| `clientPlatformName` | `0x160` | 5 | `0x1807c2f54` |
| `clientRequestMonitorSettings` | `0x180` | 4 | `0x1807c2fa8` |
| `useOps` | `0x1a8` | 1 | `0x1807c2ffc` |
| `audioMode` | `0x1ac` | 6 | `0x1807c3050` |
| `metaData` | `0x1b0` | 4 | `0x1807c30a4` |
| `sdrHdrMode` | `0x1d8` | 6 | `0x1807c30f8` |
| `clientDisplayHdrCapabilities` | `0x228` | 3 | `0x1807c314c` |
| `surroundAudioInfo` | `0x1dc` | 6 | `0x1807c31a0` |
| `remoteControllersBitmap` | `0x1e0` | 6 | `0x1807c31f4` |
| `clientTimezoneOffset` | `0x1e4` | 6 | `0x1807c3248` |
| `enhancedStreamMode` | `0x1e8` | 6 | `0x1807c329c` |
| `appLaunchMode` | `0x1ec` | 6 | `0x1807c32f0` |
| `secureRTSPSupported` | `0x1f0` | 1 | `0x1807c3344` |
| `partnerCustomData` | `0x1f8` | 5 | `0x1807c3398` |
| `accountLinked` | `0x218` | 1 | `0x1807c33ec` |
| `enablePersistingInGameSettings` | `0x219` | 1 | `0x1807c3440` |
| `requestedAudioFormat` | `0x21c` | 6 | `0x1807c3494` |
| `userAge` | `0x220` | 6 | `0x1807c34e8` |
| `requestedStreamingFeatures` | `0x230` | 3 | `0x1807c353c` |
| `transport` | `0x238` | 3 | `0x1807c3590` |

### Registration at `0x1807c3760`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `reflex` | `0x8` | 1 | `0x1807c3779` |
| `bitDepth` | `0xc` | 6 | `0x1807c37ce` |
| `cloudGsync` | `0x10` | 1 | `0x1807c3820` |
| `enabledL4S` | `0x11` | 1 | `0x1807c3872` |
| `mouseMovementFlags` | `0x14` | 6 | `0x1807c38c4` |
| `trueHdr` | `0x18` | 1 | `0x1807c3916` |
| `supportedHidDevices` | `0x1c` | 6 | `0x1807c3968` |
| `profile` | `0x20` | 6 | `0x1807c39ba` |
| `fallbackToLogicalResolution` | `0x24` | 1 | `0x1807c3a0c` |
| `hidDevices` | `0x28` | 4 | `0x1807c3a5e` |
| `chromaFormat` | `0x50` | 6 | `0x1807c3ab0` |
| `prefilterMode` | `0x54` | 6 | `0x1807c3b02` |
| `prefilterSharpness` | `0x58` | 6 | `0x1807c3b54` |
| `prefilterNoiseReduction` | `0x5c` | 6 | `0x1807c3ba6` |
| `hudStreamingMode` | `0x60` | 6 | `0x1807c3bf8` |
| `qosPolicy` | `0x64` | 6 | `0x1807c3c4a` |
| `touchSupport` | `0x68` | 1 | `0x1807c3c9c` |
| `dlssOverrides` | `0x70` | 3 | `0x1807c3cee` |

### Registration at `0x1807c1ee0`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `sessionId` | `0x8` | 5 | `0x1807c1ef9` |
| `sessionRequestData` | `0x120` | 3 | `0x1807c1f54` |
| `finalSelectedScreenResolution` | `0x28` | 5 | `0x1807c1fa8` |
| `monitorSettings` | `0x48` | 4 | `0x1807c1ff9` |
| `userIdleWarningTimeoutInMs` | `0x70` | 6 | `0x1807c204a` |
| `clientIp` | `0x78` | 5 | `0x1807c209b` |
| `seatSetupInfo` | `0x128` | 3 | `0x1807c20ec` |
| `sessionControlInfo` | `0x130` | 3 | `0x1807c2140` |
| `connectionInfo` | `0x98` | 4 | `0x1807c2194` |
| `gpuType` | `0xc0` | 5 | `0x1807c21e8` |
| `sessionAdsRequired` | `0xe0` | 1 | `0x1807c223c` |
| `sessionAds` | `0xe8` | 4 | `0x1807c2290` |
| `adProvider` | `0x110` | 6 | `0x1807c22e4` |
| `status` | `0x114` | 6 | `0x1807c2338` |
| `enhancedStreamMode` | `0x118` | 6 | `0x1807c238c` |
| `errorCode` | `0x11c` | 6 | `0x1807c23e0` |
| `finalizedStreamingFeatures` | `0x138` | 3 | `0x1807c2434` |

### Registration at `0x1807c1560`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `statusCode` | `0x8` | 6 | `0x1807c1579` |
| `statusDescription` | `0x10` | 5 | `0x1807c15d4` |
| `unifiedErrorCode` | `0x30` | 6 | `0x1807c1625` |
| `requestId` | `0x38` | 5 | `0x1807c1676` |
| `serverId` | `0x58` | 5 | `0x1807c16c7` |
| `countryCode` | `0x78` | 5 | `0x1807c1718` |

### Registration at `0x1807c07e0`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `ip` | `0x8` | 5 | `0x1807c07f6` |
| `port` | `0x28` | 6 | `0x1807c0842` |
| `protocol` | `0x2c` | 6 | `0x1807c088b` |
| `appLevelProtocol` | `0x30` | 6 | `0x1807c08d4` |
| `resourcePath` | `0x38` | 5 | `0x1807c091d` |
| `usage` | `0x58` | 6 | `0x1807c0966` |

### Registration at `0x1807c19f0`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `version` | `0xf8` | 3 | `0x1807c1a09` |
| `metaData` | `0x8` | 4 | `0x1807c1a67` |
| `authType` | `0x30` | 4 | `0x1807c1ab8` |
| `serverType` | `0x58` | 6 | `0x1807c1b09` |
| `serverInstanceId` | `0x60` | 5 | `0x1807c1b5a` |
| `monitorSettings` | `0x80` | 4 | `0x1807c1bab` |
| `serverEncodeCapability` | `0x100` | 3 | `0x1807c1bff` |
| `serviceExtras` | `0xa8` | 4 | `0x1807c1c53` |
| `notificationTopicId` | `0xd0` | 4 | `0x1807c1ca7` |
| `requestStatus` | `0x108` | 3 | `0x1807c1cfb` |

### Registration at `0x1807c1780`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `queuePosition` | `0x8` | 6 | `0x1807c178e` |
| `seatSetupEta` | `0xc` | 6 | `0x1807c17e0` |
| `seatSetupStep` | `0x10` | 6 | `0x1807c182f` |

### Registration at `0x1807c1250`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `monitorId` | `0x8` | 6 | `0x1807c1266` |
| `positionX` | `0xc` | 6 | `0x1807c12b2` |
| `positionY` | `0x10` | 6 | `0x1807c12fb` |
| `widthInPixels` | `0x14` | 6 | `0x1807c1344` |
| `heightInPixels` | `0x18` | 6 | `0x1807c138d` |
| `dpi` | `0x1c` | 6 | `0x1807c13d6` |
| `framesPerSecond` | `0x20` | 6 | `0x1807c141f` |
| `sdrHdrMode` | `0x24` | 6 | `0x1807c1468` |
| `displayData` | `0x28` | 3 | `0x1807c14b1` |
| `hdr10PlusGamingData` | `0x30` | 3 | `0x1807c14fa` |

### Registration at `0x1807c0d30`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `srMode` | `0x8` | 6 | `0x1807c0d46` |
| `srModel` | `0xc` | 6 | `0x1807c0d92` |
| `srScale` | `0x10` | 6 | `0x1807c0ddb` |
| `rrModel` | `0x14` | 6 | `0x1807c0e24` |
| `fgMode` | `0x18` | 6 | `0x1807c0e6d` |
| `fgModel` | `0x1c` | 6 | `0x1807c0eb6` |
| `fgMultiplier` | `0x20` | 6 | `0x1807c0eff` |

### Registration at `0x1807c0680`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `version` | `0x8` | 6 | `0x1807c0696` |
| `hdrEdrSupportedFlagsInUint32` | `0xc` | 6 | `0x1807c06e2` |
| `static_metadata_descriptor_id` | `0x10` | 6 | `0x1807c072b` |
| `display_data` | `0x18` | 3 | `0x1807c0774` |

### Registration at `0x1807c3d60`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `policy` | `0x8` | 6 | `0x1807c3d6e` |
| `relayProtocol` | `0xc` | 6 | `0x1807c3dc0` |
| `relayLocation` | `0x10` | 6 | `0x1807c3e0f` |

### Registration at `0x1807c2740`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `session` | `0x30` | 3 | `0x1807c274e` |
| `requestStatus` | `0x38` | 3 | `0x1807c27a0` |
| `otherUserSessions` | `0x8` | 4 | `0x1807c27ef` |

### Registration at `0x1807c2850`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `sessions` | `0x8` | 4 | `0x1807c285e` |
| `requestStatus` | `0x30` | 3 | `0x1807c28b0` |

### Registration at `0x1807c2af0`

| Field | Model offset | Tag | Key-reference VA |
| --- | ---: | ---: | --- |
| `sessionRequestData` | `0x8` | 3 | `0x1807c2af8` |
