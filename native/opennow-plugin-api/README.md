# OpenNOW provider SDK

The SDK defines data exchanged with a trusted provider process. It does not start processes, access credentials, implement a service, or decode media. The existing crate-root protocol remains version 1 for catalog-preview packages. `provider` defines control protocol 2; `media` defines offer and accepted-format data for media protocol 1. Neither version is the Qt/core protocol version.

## Decode and answer a typed operation

This complete example handles an anonymous provider's auth-status operation. All playback, catalog, account, and optional service operations have their own request and reply variants.

```rust
use opennow_plugin_api::provider::*;

let bytes = br#"{"v":2,"type":"request","epoch":7,"id":"rpc1","timeoutMs":10000,"request":{"method":"auth.status","params":{}}}"#;
let HostMessageV2::Request(request) = HostMessageV2::decode(bytes)? else {
    return Ok::<(), Box<dyn std::error::Error>>(());
};
assert!(matches!(request.request, ProviderRequest::AuthStatus(_)));

let response = ProviderResponseV2 {
    v: Version2,
    epoch: request.epoch,
    id: request.id.clone(),
    outcome: ProviderOutcome::success(ProviderReply::AuthStatus(AuthState::NotRequired)),
    effects: List::default(),
    allocation: None,
};
response.validate_for(&request)?;
let output = serde_json::to_vec(&PluginMessageV2::Response(response))?;
assert!(output.len() <= opennow_plugin_api::MAX_FRAME_BYTES);
Ok::<(), Box<dyn std::error::Error>>(())
```

The process reads incrementally bounded NDJSON frames. `decode` accepts one frame, rejects unknown fields and versions, and returns static diagnostic text rather than child-supplied content. The transport must reject an unterminated frame at 1 MiB before allocating more bytes. Encoding does not replace an outgoing frame-size check.

`ProviderRequest::permits` checks the negotiated capability set. `ProviderResponseV2::validate_for` checks correlation, reply method, catalog scope, exact session identity, and allocation receipt ownership. `validate_for_at` also checks media-offer expiry using the host's current timestamp. Host code must call these checks before using a decoded reply. Parsing a reply alone cannot prove it belongs to the current request.

## Route the full lifecycle

The typed operation table is in `src/provider/wire.rs`. A normal authenticated launch uses `AuthBegin`, short `AuthPoll` requests, `AuthComplete`, `CatalogLibrary`, `CatalogDetails`, `LaunchInspect`, `SessionCreate`, `SessionPrepare`, and `SessionStop`. `AuthState::Authorized` separates approved authorization from credential persistence; polling does not implicitly complete login. Poll responses may require waiting up to one hour before another poll. An anonymous provider returns `AuthState::NotRequired` and uses an absent account scope. It must not fabricate an account to satisfy the protocol.

`SessionCreate` receives the host's operation ID, exact game and variant, account scope, settings revision, requested preferences, and native offer. A successful response includes `AllocationTicket` separately from its outcome. A failed response may retain that ticket and required cleanup effects. The host retains the original provider instance and settles the ticket after shell acceptance or cancellation. `SessionReconcile` must resolve the original operation or return `Reconciliation::Unknown`; it does not authorize retrying create.

Operation IDs, receipt IDs, their original owners, and settled decisions must survive control-process restart in the same immutable package and provider data root. The host may send an old `SessionResolveAllocation` ticket in a new process epoch. An epoch change must not erase the receipt or authorize a second allocation. Repeated resolution of the same decision is idempotent; a conflicting later decision cannot reverse an accepted receipt.

`Reconciliation::PendingAllocation { session, ticket }` reports a freshly allocated seat whose acceptance receipt is still unresolved. `Active` means no unresolved fresh acceptance remains; discovering a ready seat is not sufficient to claim that state. Pending recovery binds the original operation, account scope, and exact session to its persisted ticket. The host must retain that private ticket, persist its intended settlement before I/O, and settle through the original owner. It must not automatically reject an allocation that already has an accepted/live native binding. Recovery tickets must not enter public QML responses or events.

`Reconciliation::NotAllocated { operation }` is positive proof that this exact operation did not allocate a session. Return it only from a durable pre-allocation rejection record or an authoritative provider answer. A missing discovery result or absent journal entry is not proof and must remain `Unknown`. The host accepts this result only for the matching source, account, and operation with no known session, receipt, or media lease; SDK pairing validation also rejects a different operation or a reconciliation request that already names a session.

`SessionPrepare` returns only `PreparedWorker { accepted, bootstrap }`. There is no plugin-selectable native GFN engine, executable path, graphics handle, QML code, or generic host command. Core resolves the package's approved media role and binds the result to its original source, account, session, and attempt. Qt/native runtime owns the media process so a core restart does not stop a healthy stream.

Remote IDs are bounded, opaque values. They are not globally unique. Child results do not carry a selectable source ID or host instance epoch. The host adds those fields from the process that returned the result. `RemoteSessionState` excludes `streaming`; native frame evidence owns that state.

Public numeric account/settings revisions, expected revisions, and revision effects are limited to `0..=MAX_PUBLIC_REVISION`, where the maximum is `2^53 - 1`. QML can round-trip every integer in that range exactly. The SDK rejects larger values during decoding, direct validation, and serialization rather than clamping them. Opaque textual catalog revisions are unchanged. Private process/media epochs and worker sender identities/timestamps retain their full integer width; they are not public QML revision counters.

## Keep private data off public UI paths

`SecretString` and `SecretBytes` have private storage, explicit `expose_secret` access, redacted `Debug`, and zeroization of their owned buffers on drop. Protocol serialization intentionally reveals their values to the intended private IPC recipient. That does not make serialized JSON safe to log or forward to QML; serializer buffers and caller-made copies have their own lifetimes.

Browser authorization URLs and callback proof belong to private auth IPC. Core/C++ converts navigation into an expiring public open handle; the provider owns OAuth, PKCE, redirect handling, and tokens. `SecretBytes` encodes worker bootstrap as standard padded base64, with a 256 KiB decoded limit. Encoded frames never use this control payload. No API returns generic token maps or another provider's secrets.

Public URLs are credential-free HTTPS, with loopback HTTP allowed for local pairing fixtures. Userinfo, query strings, and fragments are rejected. URL syntax validation cannot prove that a provider did not place a secret in a path; host policy still controls artwork fetching and public projections.

## Negotiate actual media support

`StreamPreferences.video` is `RequestedVideo`, not an accepted stream format. Width and height are required; null encoding and FPS preserve Auto without choosing a codec or rate prematurely. Bit depth, chroma, and HDR are preferences. Requested values contain no sender primaries, transfer, matrix, or color range. Schema validation checks finite bounds and incompatible explicit preferences; the provider/native owner must resolve Auto and intersect service and runtime capabilities before allocation. It must reject an unsupported request rather than present a fabricated accepted format. GFN's native owner retains its existing Auto and HDR resolution behavior.

`NativeOffer.video_formats` contains `VideoSupport` tuples and maximum dimensions/frame rates, not a list of every supported resolution. `AcceptedMedia::validate_against` matches encoding, bit depth, chroma, derived dynamic range, bounds, audio format, input subset, offer identity, runtime epoch, and expiry. Declaring an enum value does not advertise decoder support. The native owner offers only formats it implements and has tested.

`VideoFormat` carries an integer nominal frame rate and explicit color range, primaries, transfer, matrix, and chroma location. HDR mode is derived from transfer rather than a contradictory second flag. External media protocol 1 rejects HDR offers and accepted plans. It has no static-HDR metadata field; adding one requires a shared native representation and consumer. GFN's private native HDR preparation remains separate. Bootstrap and accepted formats are private handoff data; public session state is a separate host projection.

## Describe an installable package

`provider::ProviderManifest` requires schema version 2 and control protocol 2. Each target has explicit `control` and `media` roles. Both paths must exist in the package's SHA-256 file inventory; the same executable may implement both roles. Role selection is host-defined, not a manifest argument string.

```json
{
  "schemaVersion": 2,
  "protocolVersion": 2,
  "id": "org.example.provider",
  "name": "Example provider",
  "version": "1.0.0",
  "publisher": "Example author",
  "description": "A local provider fixture",
  "capabilities": ["auth.anonymous.v2", "catalog.public.v2", "catalog.details.v2", "launch.v2", "sessions.v2", "media.worker.v1"],
  "authKinds": ["anonymous"],
  "entrypoints": {
    "x86_64-unknown-linux-gnu": {"control": "bin/provider", "media": "bin/provider"}
  },
  "files": [{"path": "bin/provider", "sha256": "0000000000000000000000000000000000000000000000000000000000000000"}]
}
```

The illustrated hash is a placeholder for the executable's actual digest, not an installable package. The installer verifies the real bytes and native binary target. V2 requires a usable catalog, details, launch, sessions, media-worker support, and an explicit authentication model. Account features, PINs, favorites, ownership, linked stores, regions, storage, and ads are finite optional capability groups corresponding to existing host UI consumers. Missing capabilities are unsupported, not successful empty results.

The v1 `PluginManifest` remains a distinct catalog-only format. A v1 package does not gain playback by changing its handshake. Both tiers require explicit trusted-native-code consent; neither manifest nor process supervision is an OS sandbox.

## Verify the SDK

From the repository root:

```sh
cargo test --manifest-path native/opennow-plugin-api/Cargo.toml
cargo clippy --manifest-path native/opennow-plugin-api/Cargo.toml --all-targets -- -D warnings
```

These checks prove schema, pairing, bounds, and compatibility behavior. They do not prove provider authentication, media playback, or support for an external commercial service.
