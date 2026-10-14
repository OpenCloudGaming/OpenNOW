# OpenNOW SDK demo provider

This example is a complete local provider demonstration, not Xbox Cloud Gaming or a commercial service. It signs into demonstration accounts, browses a paginated catalog, allocates a local session, and authorizes a separate native worker that produces real H.264 video and Opus audio. The existing OpenNOW decoder and video item present those frames. The control process never publishes a fake first-frame or `streaming` event.

The pairing flow is deliberately deterministic: the displayed `SDK-DEMO` code is public demonstration text, not a credential. The first auth poll approves the attempt. A separate `AuthComplete` request commits the account. Nothing is sent to an identity provider, and no real authentication is claimed.

## Build one installable package

Run from the repository root on the same OS and architecture as OpenNOW:

```sh
cargo build --locked --release --manifest-path examples/provider-plugin/Cargo.toml
cargo build --locked --release --manifest-path examples/provider-media-worker/Cargo.toml
examples/provider-plugin/target/release/opennow-sdk-demo --package \
  examples/provider-media-worker/target/release/provider-media-worker \
  "$PWD/opennow-sdk-demo.opennow-plugin"
```

On Windows, use `opennow-sdk-demo.exe` and `provider-media-worker.exe`. If `CARGO_TARGET_DIR` is set, use the binaries in that directory instead. Cross-compiling one role and using a worker for another target is not supported; the package generator verifies both native headers for its host.

The generator reads the worker without executing it. It creates one `.opennow-plugin` ZIP containing the control executable, media executable, license, and a protocol-2 manifest with explicit roles and SHA-256 inventory. No worker download or separate GUI application is needed after installation. Neither role requires FFmpeg, Node, Python, or a shell at runtime. The media worker's Rust/C codec dependencies are built with the worker; ordinary OS runtime libraries still apply.

Open **Settings → Plugins → Install from file**, inspect the package, and accept the unsigned-native-code warning only if you trust these bytes. Installation leaves the provider disabled. Enable **OpenNOW SDK demo**, select it as the source, and choose **Sign in**. The pairing panel shows the demonstration code, then the host completes the authorized attempt. Select any **OpenNOW SDK demo** catalog entry and choose Play.

All 24 labeled entries exercise the same generated fixture; they are not 24 cloud games. The actual accepted video is 320×180, 50 FPS, H.264 Annex B, SDR eight-bit YUV 4:2:0, limited-range BT.709 with left chroma siting. Audio is 48 kHz stereo Opus. This format is checked against the current native offer. The demo rejects incompatible codec, HDR, bit-depth, chroma, or native media support rather than relabeling the encoded bytes. Requested dimensions/rates are preferences; the public accepted format reports the fixture's actual negotiated values.

The worker supports the input capabilities it advertises, currently keyboard and one gamepad. Mouse, text injection, rumble, and microphone are not advertised by this demo. Local Guide/exit/focus handling stays in OpenNOW. End the session using the application's confirmation flow.

## Keep control and media ownership separate

The control role implements the SDK's finite `ProviderRequest` and `ProviderReply` operations. The Qt/native-owned media role uses `opennow-media-protocol::wire::WorkerBootstrap`, supplied privately by the native supervisor. Encoded frames and typed input use the production media bridge, not core NDJSON or QML.

Both roles receive the same host-derived `OPENNOW_PLUGIN_DATA_DIR`. No bootstrap field can choose a data-directory path. The control process takes a single-writer lock and appends fsynced snapshots to `state.ndjson`. The media worker reads complete journal records without taking that writer lock. A partial final append is ignored by readers and truncated by the next control owner; a malformed complete record fails closed.

The demonstration journal stores public demo accounts, original create/stop operation IDs, exact owner/session keys, receipt decisions, and media authorization hashes. A temporary login does not restore sign-in after restart. A retained account remains available for exact-seat cleanup, and removing an account with an active seat is rejected.

Operation and receipt identity survives control-process restart. The host may deliver an old receipt to a new process epoch using the same immutable package and data root. Repeating the same create operation returns the same seat; it never allocates a second one. Repeating the same receipt decision is idempotent, while a conflicting later decision cannot reverse an accepted receipt. A claim has no new allocation receipt. A rejected fresh receipt cannot be prepared for media.

An allocated but unaccepted seat reconciles as `PendingAllocation`, with its original durable ticket, even after a control restart. It is not `Active` merely because the fixture is ready. Only a settled accepted receipt becomes Active. The host keeps the recovery ticket private, records its settlement intent before I/O, and must not reject an already accepted/live native binding. The control provider never decides that host policy from the current browsing account.

Valid create intents rejected before allocation are durably recorded. Reconciliation reports `NotAllocated` only for that exact owner/operation record and only when the request has no known session. Missing journal or discovery records remain `Unknown`; absence is never fabricated as successful cleanup. New attempts need new operation IDs rather than changing parameters on a previously rejected ID.

## Authenticate the media role

On prepare, the control process writes an authorization grant containing the exact accepted media and SHA-256 of a random 32-byte token. It returns private bootstrap bytes with this schema:

```text
{version:1, session:SessionKey, offerId, runtimeEpoch, expiresAtMs, token}
```

The native supervisor passes those bytes unchanged as `WorkerBootstrap.providerBootstrap`. It also supplies a typed, host-derived `WorkerBinding { leaseId, sourceId, session, attemptId }` from the canonical prepared lease and checks the source against the verified package manifest. The worker uses these shared functions before producing media:

```text
authorization::authorize_media(data_directory, provider_bootstrap, accepted)
    → io::Result<SessionKey>
authorization::session_is_active(data_directory, session)
    → io::Result<bool>
```

`authorize_media` verifies an accepted receipt, nonterminal exact seat, offer/epoch/format/input binding, expiry, and constant-time token-hash equality against durable control state. A self-asserted token or format is insufficient. Plaintext attach tokens are not written to the journal or logs.

After authorization, the worker must compare the returned `SessionKey` to `WorkerBinding.session` and require the demo's exact source identity. A valid token for another seat or source cannot authorize this host lease. Lease and attempt identifiers stay host-authored; the provider's private bootstrap cannot replace them. The worker performs these checks before opening its media/control channels or emitting frames.

Expiry applies only when attaching. Once authorized, the worker checks terminal session state independently of token rotation or control PID. Killing and restarting only the control process must not interrupt media. Ending the exact session revokes it; an unrelated account selection cannot retarget the running worker.

This is process authorization, not a sandbox. Both native executables run as the user's account and can access that user's resources. An actual service implementation must keep its OAuth/token exchange and credential storage private; the demo's public deterministic pairing is not a template for securing a remote service.

## Verify behavior

```sh
cargo test --locked --manifest-path examples/provider-plugin/Cargo.toml
cargo clippy --locked --manifest-path examples/provider-plugin/Cargo.toml --all-targets -- -D warnings
```

The control suite tests explicit authorization/commit, account switching, pagination and cursor scope, create/receipt/stop idempotency across restart, exact-owner recovery, no-allocation proofs, journal truncation/corruption, media authorization, and actual subprocess protocol handling. The journal is bounded to 8 MiB, with at most 64 auth attempts, 64 historical seats, 128 stop records, and 128 rejected allocation records. This demonstration fails closed when those bounds are exhausted; it is not a production database design.

To verify playback, install the generated package in the current OpenNOW build and use the ordinary sign-in/library/Play flow. Check actual native decoded/presented frames, audible tone or the explicit null-audio acceptance fixture, input/neutral acknowledgement, and confirmed stop. Kill only the control process and check that the media worker and video item remain the same. Separately crash the media worker and check fenced teardown. A passing control test or archive inspection is not evidence of rendered playback, live Xbox support, hardware decode, or Windows/macOS runtime validation.

## Fixture provenance

The media fixture comes from this project's encoded-media worker proof. Colored RGB patterns are generated by code and converted to limited-range BT.709 before encoding. Audio is a generated 440 Hz stereo sine tone. There is no captured commercial-game footage, downloaded media, or account data. The media worker uses the project's source-built OpenH264/Opus fixture and carries its dependency notices; no runtime `ffmpeg` command generates the stream. See `examples/provider-media-worker` for the exact codec versions and build/license details.

The control provider and authoring tools are MIT-licensed under the repository's license. Publisher metadata is self-declared. Hashes bind installed bytes and do not establish publisher authenticity.
