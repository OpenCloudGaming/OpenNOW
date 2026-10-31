# Handshake and connection setup

The inspected path uses RTSP signaling to prepare media and has distinct RTC, unified-Mjolnir, and legacy socket branches. An RTSP success is not evidence that decoded video has reached the display. These are static findings in `archive/libBifrost2.dylib`, x86_64; [artifacts.md](artifacts.md) identifies the exact slice.

## Evidence anchors

| Alias | REA evidence ID | Relevant functions |
| --- | --- | --- |
| H1 | `ev_d95fac31d976eea57467f92a5dddf849b00c2a6bb6b93655d61a9f35f86f1e20` | `connectWithRtspHandshake` `0x1197a0`, `setupSessionStreams` `0x1145c0`, unified video setup `0x1130f0`, DTLS readiness `0x1bcda0` |
| H2 | `ev_792c051e5ffcc4bf87adfb785b5fd4963245b02f3a6a7189dd165cbb7e1713e0` | `doAnnounce` `0x11ea20`, `setupWebRtcTransport` `0x11fcd0`, `tlsInfoCallback` `0x19ec00`, `tlsVerifyCallback` `0x19ebf0` |
| H3 | `ev_761f6076e08acc843bc9793dcd243ef776c1f415542c8ddcfdec4afaf2dab740` | `SignalingHandler::startStreamingWithRtsp` `0x121310` |
| H4 | `ev_e9cc7b12d4dfa4a4a755cada4e6483c47e193758e70635018e9ff4c2b7932a47` | `createRtspConnection` `0x122780`, `createRtspSession` `0x11d250` |
| H5 | `ev_d1ab53c09326e0e81c00125c87528abf0c329f85fdad9a680d7a2cd8ae890255` | `DtlsTransport::initDtlsContext` `0x19e400`, DTLS receive `0x19f190`, RTC start `0x1b6430` |
| H6 | `ev_6023f8f0c826d742e45b919f38744ca22b5b218b47d740850e07c2cc63ca0afc` | Incoming references identify `ClientSession::setupSession` as a direct `doAnnounce` caller |

Some C++ virtual calls and recovered argument types remain unresolved. Batch pseudocode is not original source. The large connection function contains compiler-generated string handling and error mappings; no live request ordering was recorded.

## Endpoint and signaling preparation

**D, H1, `0x1197a0`.** The connection function gets its initial port from `ConfigHelper::getRtspHandshakePortNumber`, then passes the supplied address to `getServerIpAndPort`, which can update that port. It creates the RTSP session using the resulting address/port and the locally formatted extension-header string `14.2`. This is not a recovered service-discovery rule or proof that every server uses one fixed RTSP port.

**D, H4, `0x11d250`.** The session factory adds an `x-nv-sessionid` header when its configured string is nonempty, then passes the URL, port, header-version string, settings, and header map to its RTSP client. The actual session-ID value is not part of this documentation. The client's final transport-factory implementation and every scheme-specific TLS option were not recovered.

**D, H4, `0x122780`.** The endpoint-selection path calls `connectWithRtspHandshake` on candidate URLs. In its semicolon-separated candidate loop, result `0x800b1004` permits trying another candidate; success stops the search, and another error stops that loop. These are local result values. This is endpoint retry, not evidence of changing alliance provider, authentication identity, codec, or transport security.

## OPTIONS, DESCRIBE, and stream setup

**D, H1, `0x1197a0`.** The OPTIONS stage requires an existing, valid, non-destroyed RTSP session. On a 200 response it parses the extension-header version and checks the server command list for OPTIONS, DESCRIBE, SETUP, TEARDOWN, ANNOUNCE, and PLAY. Missing required-command handling produces the local option-not-supported path. The presence of TEARDOWN in this check does not mean it is sent during startup.

The DESCRIBE stage parses returned SDP only after a 200 response. It separates optional sections marked by `;;` and `||`, with logs naming new features and an upstream-video offer, then validates the remaining SDP. It parses media blocks and applies post-offer settings. An invalid client override file or inconsistent SDP takes a terminating connection path. The complete optional-section grammar and every SDP field's schema remain **U**.

The successful preparation path calls `setupSessionStreams` and allocates the bundle socket early so its local port can contribute to native ICE candidates. Outgoing SDP construction follows that setup in this function. One conditional path generates AES parameters when the offered data contains `runtime.encryptionKeyId` and configured encryption flags are set. No generated key value was collected or published; the settings-to-flag mapping and complete encryption negotiation remain **U**.

## Three media socket arrangements

**D, H1, `0x1145c0`, and H2, `0x11fcd0`.** Stream setup selects behavior by `RtcCompliantMedia` predicates and configuration state:

- RTC-requested audio and microphone bypass their Mjolnir RTSP SETUP branches and defer to `WebRtcTransport` setup.
- For video not requested through the RTC-compliant path, an enabled unified-socket setting with no more than one configured video stream selects `setupVideoStreamUnifiedSocket`. A multi-video configuration takes the legacy-video branch and logs why unified mode is disabled.
- The legacy branch creates separate stream sockets. Reserved-port binding failure has a separately gated dynamic-port fallback; it is not a universal retry rule.
- When an RTC data channel is requested, the legacy control-stream setup branch is skipped. Other branches obtain the control port and perform their own control setup and optional encryption.

**D, H1, `0x1130f0`.** Unified video setup uses the local `WebRtcTransport` socket port for video SETUP and creates placeholder RTP sources marked as unused. **D, H2, `0x11fcd0`.** The transport setup creates and registers `MjolnirVideoReceiver` early for unified mode. This establishes that a WebRTC-named transport object can participate in native Mjolnir video transport; library naming alone cannot identify the selected media protocol.

The integer settings offsets are recovered object layout, not a public settings ABI. The exact negotiated enum names and defaults for those offsets remain **U**.

## ANNOUNCE, transport readiness, and PLAY

**D, H2, `0x11ea20`.** ANNOUNCE has its own destroyed-session guard and checks the response. Its 200-response path updates control/bundle addressing and conditionally calls `setupWebRtcTransport`. When `RtcCompliantMedia::AnyTypeRequested` is true, it waits on a transport-readiness future until a steady-clock deadline **20 seconds** ahead. A timeout and a nonzero readiness result are different error paths.

**D, H2, `0x11fcd0`.** Before starting the RTC transport, setup supplies remote peer addresses, creates audio sender/receiver objects, sets the expected DTLS fingerprint, and sets STUN credentials. **D, H5, `0x1b6430`.** `WebRtcTransport::Start` constructs a `WebRtcTransportThread` with its receiver pointer and a configured integer, stores it, and releases a previous thread object when one exists. The thread constructor, candidate-gathering, nomination, and relay-selection policy are not recovered.

**D, H3, `0x121310`.** PLAY is a separate start function. A configured play-disabled flag takes an explicit skip branch. Otherwise it checks that the RTSP session still exists and invokes its PLAY operation. Status 200 marks the signaling start flag. A nonzero failure destroys the RTSP session and returns the failure. The full higher-level scheduling between setup, ANNOUNCE, and this start callback is not an observed runtime trace.

**D, H6.** A direct call from `ClientSession::setupSession`, entry `0xfcb40`, to `doAnnounce` is present at `0xfcbab`. This anchors ANNOUNCE to session setup rather than to an assumed monolithic handshake function.

## DTLS peer identity and SRTP key setup

**D, H5, `0x19e400`.** The DTLS context loads a certificate/private-key pair, checks the key, configures a cipher-list string, and offers these SRTP profiles: `SRTP_AEAD_AES_256_GCM`, `SRTP_AEAD_AES_128_GCM`, and `SRTP_AES128_CM_SHA1_80`. Failures take distinct callbacks. This does not establish the profile selected by a live server.

**D, H2, `0x19ebf0` and `0x19ec00`.** The certificate-verification callback returns one, while the handshake-completion callback performs a separate expected-fingerprint comparison. When the expected-fingerprint string is nonempty, it extracts the peer certificate, generates an RFC 4572 fingerprint, compares length and bytes, and records a mismatch error rather than exporting keys. Missing peer certificate, missing selected SRTP profile, unsupported profile, and key-export failure have separate error values.

The absent-expected-fingerprint branch logs and returns without the visible key-export/readiness path. It must not be described as successful unauthenticated readiness. The trust source of the expected fingerprint still depends on the session-description path; this study did not validate an authenticated live offer.

The accepted profile IDs in this callback are **1, 7, and 8**, with key/salt lengths of **16/14, 16/12, and 32/12 bytes**, respectively. The exporter label is `EXTRACTOR-dtls_srtp`. A role branch swaps the directional key/salt buffers before handing them to the transport callback. These are algorithm and layout observations; no key material is included.

**D, H1, `0x1bcda0`.** The completion callback initializes SRTP and, under a separate flag, another SRTP context for Mjolnir video. Only after successful setup does it mark DTLS readiness and publish a transport-ready event. One configured branch enables STUN consent at a five-second interval. That branch-local interval is not the whole session's universal keepalive policy.

## Compatibility implications and remaining gaps

**Recommendation.** Keep service authentication, RTSP signaling, ICE/DTLS readiness, media socket ownership, and successful presentation as separate states in OpenNOW. Preserve the distinction between native Mjolnir over a unified transport and RTC-compliant media. Do not replace these branches with a single guessed WebRTC or UDP path.

**U.** Full SDP schemas, every mode selector, ICE candidate priority/relay policy, RTSP/WebSocket upgrade conditions, exact secure-RTSP certificate policy, server-side timeouts, reconnect/resume signaling, and arm64 branch equivalence remain incomplete. The field-name catalog is a discovery reference, not a negotiation specification.
