# Provider plugin API v2

This reference covers installable playback providers. The canonical public types
are in [`opennow-plugin-api`](../native/opennow-plugin-api/README.md). Private
native types and worker framing are in
[`opennow-media-protocol`](../native/opennow-media-protocol/src/).

The [module architecture](provider-modules.md) explains ownership and trust.
The [SDK demo](../examples/provider-plugin/README.md) provides the build and
package procedure. A working demo is not a commercial-service integration.

## Versions and package roles

The independent version boundaries are:

| Boundary | Version |
| --- | --- |
| Qt shell to application core | Core protocol 5, capability `sources.v2` |
| Provider manifest and control protocol | 2 |
| Encoded-media worker protocol | 1 |
| Qt to native runtime JSON | 8 |
| Native C ABI | 12 |

Catalog-only protocol-1 packages continue to use
[`sources.catalog.page`](plugins.md). They do not become selectable playback
services or gain authentication and session capabilities.

Provider packages use the same `.opennow-plugin` ZIP container, inventory checks,
installation consent, and management RPCs as catalog packages. A protocol-2
manifest declares `authKinds`, typed capabilities, and target-specific roles:

```json
{
	"x86_64-unknown-linux-gnu": {
		"control": "bin/control",
		"media": "bin/media"
	}
}
```

This object is the manifest's `entrypoints` field. Both files are in the hashed
inventory and match the host target. Neither role is executed during inspection
or installation. There are no provider-supplied executable arguments, environment
variables, or installation hooks. The host supplies `OPENNOW_PLUGIN_DATA_DIR`.

`ProviderManifest::validate` defines required playback capabilities and accepted
authentication kinds. SDK enum membership does not mean a capability is available
on a particular provider or native device.

## Control protocol

`HostMessageV2` and `PluginMessageV2` use bounded UTF-8 NDJSON over private pipes.
Hello binds the installed ID, version, capabilities, and host-issued process
epoch. Every response is paired with its original request ID, epoch, and typed
operation. The plugin cannot call arbitrary host RPCs.

Responses contain a typed outcome, bounded effects, and an optional fresh
allocation ticket. A failed outcome can still carry cleanup obligations. The
host retains late effects and tickets after cancellation or EOF rather than
treating failure as evidence that nothing happened.

The supervisor reserves request capacity for receipt and session operations.
Unanswered non-background requests retire control after their deadline and a
bounded grace period. Catalog cancellation alone does not terminate control or
media. Explicit reconcile, stop, and receipt settlement can restart control using
the same package and data directory. Create and arbitrary mutations are not
replayed automatically.

Child stderr and raw child error messages are not forwarded to the shell.
Protocol errors use host-authored messages. `SecretString` and `SecretBytes`
have bounded, redacted representations. Bootstrap bytes are limited to 256 KiB
before base64 encoding.

## Public shell projection

The core's public provider calls use `{sourceId,request:<parameters>}` and return
`{sourceId,generation,result:<body>}`. The body does not include the SDK transport's
additional method tag. Source generation, account revision, and registry
generation have different meanings.

`sources.list` returns `{generation,selectedSourceId,sources}`. Source rows extend
plugin descriptors with `protocolVersion`, `providerCapabilities`, `authKinds`,
and `playback`. `sources.select` changes the persisted browsing source, not the
owner of an active session.

The public method groups are:

| Group | Methods |
| --- | --- |
| Authentication | `sources.auth.authorities`, `.state`, `.start`, `.poll`, `.complete`, `.cancel`, `.logout` |
| Accounts | `sources.accounts.list`, `.select`, `.remove` |
| Catalogs | `sources.public.page`, `sources.library.page`, `sources.store.page`, `sources.game.get` |
| Launch inspection | `sources.launch.inspect` |
| Sessions | `sources.session.current`, `.poll`, `.discover`, `.claim`, `.reconcile`, `.stop` |
| Host stream overrides | `sources.settings.get`, `.set` |
| Provider-defined settings | `sources.providerSettings.get`, `.set` |

Parameters and result types follow the SDK except for host settings and private
data projections. Browser authorization becomes an expiring `openHandle`.
Allocation tickets, package paths, and media bootstrap never enter the public
projection. Human-facing device and pairing codes remain visible.

`authorized` is not a completed login. Authentication requires a separate
`sources.auth.complete` call. Provider polling delays range from 250 to 3,600,000
milliseconds. A client does not shorten that delay to its own retry interval.

Catalog items carry provider-local IDs. An application reference also retains
`sourceId`. A session uses `{sourceId,session:<SessionKey>}`, where `SessionKey`
contains both `account` and `remoteId`. A bare remote ID is not an ownership key.

Host stream overrides and provider-defined settings have separate revision
domains. Host keys include `stream.width`, `stream.height`, `stream.fps`,
`stream.codec`, `stream.colorQuality`, `stream.hdr`, and `stream.bitrateMbps`.
Zero FPS denotes Auto in host settings. The SDK represents unresolved Auto codec
and FPS with `null` in `RequestedVideo`. Requested preferences do not invent
sender colorimetry. `AcceptedMedia` describes the negotiated format.

## Private C++ playback flow

QML supplies `{sourceId,request:{scope,target,catalogRevision}}` to `SourceBridge`.
The C++ coordinator obtains host local policy with `streamer.source.policy`, then
gets a native `media-offer`. Its private `sources.session.create` call adds that
offer and current runtime capabilities. The core supplies the operation ID,
settings revision, and effective preferences.

Create returns a public session view and opaque `sessionHandle`. CoreClient
validates the response and acknowledges it before public delivery. A late
cancelled create is not acknowledged.

Preparation obtains a fresh offer, because an allocation can outlive the offer
used to create it. `streamer.source.prepare` takes
`{sessionHandle,offer,runtimeCapabilities}` and returns `HostBoundPreparedLease`.
The native start context is `{lease:<that result>}`.

The lease binds source, account-qualified session, offer, runtime epoch, attempt,
expiry, and media. A worker lease includes a host-resolved `PackageReference`
with `versionRoot`, `expectedManifest`, and `dataRoot`. A provider cannot supply
those fields. The built-in-only GFN variant retains its owned native context.

C++ acknowledges preparation only after native initialization accepts the exact
lease. That acknowledgement does not assert that a frame was decoded, displayed,
or that input is ready. Rejected preparation releases its preparation pin, not
the remote session.

`streamer.source.release` takes the exact source, session key, and lease ID after
confirmed native retirement. Losing a core connection is not retirement.

Private calls use non-QML callbacks. A signal on a QML-exposed QObject is not a
private channel, even when only C++ is intended to connect to it.

## Native media contract

The native runtime verifies the package, owns the media worker, and sends bounded
`WorkerBootstrap` through private stdin. Its `WorkerBinding` comes from the host
lease. The worker validates that binding against the service authorization before
producing media. The demo checks it against its durable control journal.

Encoded media uses binary `ONW1` frames on stdout. Authenticated control uses a
separate loopback connection with bounded length-prefixed messages. The handshake
is worker Hello, host Attached, then worker Ready. Typed input, neutralization,
keyframe requests, acknowledgements, and frame feedback use this channel.

Worker Ready grants only the accepted input subset. Local shortcuts and focus
ownership remain in Qt. Input neutralization does not pause audio or video.
Decoded and presented feedback are distinct from packet acceptance.

`FrameProvenance` preserves sender timestamps, clock rates, and optional sender
IDs and SSRCs through decoding and presentation. Zero is a valid sender ID.
Absent metadata remains absent. `FrameInfo.sequence` remains a local publication
counter and does not replace a sender ID.

The current external contract is SDR-only. It advertises neither HDR nor a
microphone. Exact codec, color, input, audio, and queue limits come from the
current native offer. Unsupported combinations fail admission rather than being
relabelled as another format. GFN's private HDR path remains available separately.

## Recovery evidence

`PendingAllocation` returns a recovered fresh ticket bound to the original
operation, account, and exact session. It is not an accepted playable session.
The host keeps the ticket private and preserves rejection intent before cleanup.
Conflicting pending evidence cannot authorize destruction of an accepted live
native binding.

`NotAllocated` requires affirmative evidence for the original operation.
Discovery or journal absence alone is insufficient. `Unknown` retains ownership
and never triggers a replacement allocation automatically.

Native status has `runtimeEpoch`, `nativeIdle`, `legacyActive`, and `active`.
Bound states are `starting`, `negotiating`, `streaming`, and `recovering`.
Streaming requires actual current-attempt media output, not a connected socket.

C++ first calls `streamer.source.observe` for `mediaRevision`, then probes native
status. `streamer.source.reconcile` receives `{mediaRevision,status}`. The journal
compares the revision before an idle observation can retire any newer ownership.
A stale observation requires a fresh probe. `active:null` alone is not idle.

Healthy matching leases reattach bookkeeping after a core restart without
another native start. Remote-ended sessions remain occupied while native media
still owns resources. Legacy GFN media has its own persisted obligation rather
than an invented lease.

GFN failures before allocation and confirmed exact-seat cleanup have explicit
stage evidence. A lost allocation response without a session ID can remain
unknown because the upstream API supplies no operation correlation. The host
does not turn that limitation into a false successful cleanup.
