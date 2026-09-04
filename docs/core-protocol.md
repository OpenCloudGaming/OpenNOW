# OpenNOW shell/core protocol

The Qt shell and Rust application core communicate over newline-delimited UTF-8
JSON on inherited standard input/output. Standard error is reserved for redacted
diagnostics. A line is limited to 1 MiB. Unknown, malformed or oversized protocol
messages terminate the core connection instead of leaving the shell in an
ambiguous state.

## Handshake

The first shell request is always:

```json
{"type":"request","id":"1","method":"core.hello","params":{"protocolVersion":1,"shell":"qt","shellVersion":"0.5.4"}}
```

The core must return the same protocol version and its capabilities. The shell
does not send product requests before this succeeds. Version mismatches, a
five-second handshake deadline, process exit and invalid data all transition the
transport to `failed` with a credential-free diagnostic.

## Messages

```json
{"type":"request","id":"42","method":"settings.get","params":{}}
{"type":"response","id":"42","ok":true,"result":{"settings":{}}}
{"type":"response","id":"42","ok":false,"error":{"code":"invalid_setting","message":"…"}}
{"type":"event","name":"settings.changed","payload":{"key":"fps","value":120}}
{"type":"cancel","id":"42"}
```

Request IDs are unique within a core process. Every request has a bounded
deadline (100 ms to five minutes). The shell sends cancellation after a timeout
or explicit cancellation. Events are delivered in batches through a queue of at
most 512 items; overflow drops the oldest event and emits a diagnostic counter.

## Implemented core methods

- `core.hello`
- `app.status`
- `settings.get`
- `settings.set`
- `settings.reset`
- `auth.providers.list`
- `auth.session.get`
- `auth.device.start`
- `auth.device.poll`
- `auth.device.complete`
- `auth.device.cancel`
- `auth.logout`
- `auth.accounts.logoutAll`
- `auth.accounts.list`
- `auth.accounts.switch`
- `auth.accounts.remove`
- `auth.pin.status`, `auth.pin.set`, `auth.pin.clear`, `auth.pin.verify`
- `catalog.public.list`
- `catalog.library.list`
- `network.regions.list`
- `network.regions.ping`
- `account.subscription.get`
- `account.connections.list`, `account.connections.sync`, `account.connections.unlink`
- `account.connections.link.start`, `account.connections.link.poll`
- `account.storage.locations`, `account.storage.reset`
- `session.create`
- `session.poll`
- `session.stop`
- `session.active.get`
- `session.remote.list`, `session.claim`, `session.ad.report`
- `streamer.detect`
- `streamer.prepare`
- `streamer.start`
- `streamer.status.get`
- `streamer.stop`
- `streamer.input.pause`, `streamer.control`, `streamer.surface.update`
- `streamer.recording.start`, `streamer.recording.stop`
- `diagnostics.snapshot`, `diagnostics.export`, `acceptance.export`
- `media.root.get`, `media.recording.target`, `media.list`, `media.delete`
- `cache.delete`
- `queue.status.get`, `queue.serverMapping.get`
- `thanks.data.get`, `communityProxy.provision`
- `updater.state.get`, `updater.check`, `updater.download`, `updater.install`
- `updater.highlights.get`, `updater.highlights.ack`
- `social.capabilities.get`
- `discord.activity.sync`, `discord.activity.clear`
- `telemetry.sync`, `feedback.submit`, `bug_report.submit`

Settings writes use a temporary file plus recoverable backup and normalize
compatibility-sensitive values. Provider discovery falls back to NVIDIA's
default service when discovery is unavailable. Device-login tokens are stored
through the OS credential store (DPAPI/Credential Manager, Keychain or Secret
Service), with an explicit memory-only fallback when that facility is
unavailable; the shell never receives a password. Public catalog results are
cached in the core process and bounded per response.

CloudMatch session methods preserve one client/device identity through create,
poll and stop, retain pending queue responses before signaling is available,
and return the complete ordered connection, ICE and negotiated-feature payload
needed by the native streamer. `streamer.prepare` returns the normalized session
context used by the NVST runtime linked into the Qt shell. The in-process runtime
owns secure NVIDIA signaling, ICE/DTLS/SCTP, RTSPS, Mjolnir, RTCP and native
gameplay input, while Qt owns the graphics device, scene graph, video item and
all top-level windows. Legacy streamer lifecycle methods remain protocol
compatibility routes and are not used by the Qt shell.

`acceptance.export` is available only through the Qt shell's Diagnostics screen. It rejects
headless window systems and writes an atomic, redacted `opennow.live-acceptance` JSON file. The
file records machine-observed ten-minute streaming, first-frame, guide/input ownership, surface,
microphone, recording, hashed media, bounded recovery and terminal-error checks. It contains no
account identifiers, session identifiers, URLs, process identifiers, executable paths or local
media paths. A false check is retained as evidence of an incomplete run; it is never promoted to
a pass by the release verifier.

Network work runs outside the protocol reader with a fixed concurrency ceiling,
connect/request deadlines, one serialized output writer and best-effort response
suppression after cancellation. The full Electron API inventory remains tracked
in [the machine-readable parity manifest](../native/opennow-core/contracts/legacy-open-now-api.json),
validated against its [JSON schema](../native/opennow-core/contracts/legacy-open-now-api.schema.json)
and executable golden-fixture tests. A method is not considered ported until its
owner, wire shape, fixtures and replacement disposition are recorded there.
