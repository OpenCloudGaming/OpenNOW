# Provider modules and community plugins

OpenNOW separates service integration from its Qt interface and native player.
A provider owns authentication, catalog access, launch authorization, remote
sessions, and its service's transport. OpenNOW owns the window, decoder, audio
output, input capture, overlays, and recording.

Protocol-2 packages contain a control executable and a headless media worker.
A compatible service can implement those roles without adding its authentication
or transport logic to the application core. That does not make an unimplemented
service playable. Xbox Cloud Gaming, Boosteroid, and personal-PC streaming still
need their own working integrations.

The [SDK demo](../examples/provider-plugin/README.md) exercises these contracts
with generated H.264 video, Opus audio, and explicitly simulated sign-in. It does
not authenticate to a commercial service. Protocol-1 catalog plugins remain
preview-only.

## One host and one player

The Rust core composes `SourceHost`, the built-in GFN module, and `PluginManager`.
Application dispatch, reporting, and update admission do not own `GfnService`.
The typed `ProviderSource` contract covers provider operations. The built-in
compatibility contract preserves existing GFN RPCs, but its session operations
enter the same host admission and journal as external providers.

GFN's NVIDIA and Alliance authentication, CloudMatch rules, account services,
push registry, and provider-specific recovery stay in `sources/gfn/`. Generic
providers do not imitate GFN membership tiers, numeric status codes, or catalog
identifiers. The shell retains a source ID with each provider-local game ID and
the complete account-qualified session key.

Qt composes the same `StreamVideoItem` for both execution paths. A provider does
not inject QML, load a presenter DLL, open another player window, or introduce a
browser runtime. Local menus, focus handling, fullscreen, and controller routing
stay with the existing Qt owners. Changing the browsing service does not retarget
an active stream.

## Control and media have different lifetimes

The core supervises the provider's control process. The native runtime owns the
media process, so restarting the core does not end healthy media. Both roles use
the same host-assigned provider data directory. The media worker can validate
private attachment credentials against durable provider state without depending
on the control process remaining alive.

The core resolves the installed media role and constructs a private, host-bound
lease. Providers return only bounded accepted-media data and opaque bootstrap
bytes. They cannot choose executable paths, package manifests, local output
devices, or host graphics handles through that reply.

The native runtime verifies the immutable package and retains its shared pin
until the worker retires. Disable, uninstall, and replacement take exclusive
pins before changing package files or provider data. Pin files live outside the
directories uninstall removes, so another process cannot bypass a held lock by
recreating its pathname.

## Allocation is not acceptance

The host journals the original source, account, and operation before allocation.
A fresh allocation returns a receipt. The host settles that receipt on the
original owner even when the calling request is cancelled or the response
arrives after the control process fails.

Providers persist operation and receipt identities across process restarts.
Recovery distinguishes an accepted session from a fresh allocation still waiting
for acceptance. A pending receipt cannot become playable merely because remote
discovery reports an active seat. Rejection and stop intents persist before
provider I/O, and failed cleanup remains an ownership obligation.

An error name, an empty discovery result, or idle local media does not prove that
no remote allocation occurred. `NotAllocated` requires authoritative evidence
for the original operation. GFN records its actual request stage so failures
before allocation and confirmed cleanup do not leave a false ownership record.
A lost GFN POST response with no session ID remains unknown when the upstream
service cannot correlate the operation. The host does not guess a seat or silently
clear that uncertainty. There is no "Forget unresolved launch" action. New launches
and update installation remain blocked until authoritative recovery resolves the
original allocation.

## Playback preparation is private

`SourceBridge` coordinates the new provider path in C++. QML supplies a launch
intent and receives public session handles and status. Native offers, package
paths, bootstrap bytes, and preparation leases use private callbacks rather than
QML-visible signals or properties.

Browser authentication follows the same boundary. The provider owns OAuth,
PKCE, tokens, and callbacks. The core exposes an expiring, scoped open handle.
C++ resolves that handle privately and opens the HTTPS URL in the system browser.
Human-facing device and pairing codes are intentional display exceptions.

Native initialization is not proof of playback or input readiness. The worker
must complete its authenticated handshake before gameplay input is enabled.
Decoded feedback comes from actual decoder output. Presented feedback comes
from Qt's successful frame-swap path. Sender IDs and timestamps remain distinct
from local publication sequence numbers.

The external media contract currently admits SDR formats supported by the local
native offer. It does not advertise external HDR or microphone support. GFN's
existing private HDR and codec negotiation remain separate.

## Recovery preserves both remote and local ownership

Remote termination and native retirement are separate facts. A remote-ended
session remains occupied while native resources still own its media. Conversely,
a crashed worker does not prove that the remote seat ended.

After a core restart, C++ observes the journal's media revision, queries native
status, and reconciles that exact observation. The core rejects an older idle
observation if a newer preparation changed ownership. A matching healthy lease
reattaches bookkeeping without starting another worker.

Legacy GFN starts retain their validated native compatibility path. Successful
legacy preparation records a separate media obligation. An empty bound-lease
field is not evidence that legacy media is idle. Only confirmed native retirement
clears that local obligation.

GFN retains its existing credential-store names, data paths, device IDs, profiles,
and account identities. It is enabled by default and can be disabled while idle,
but it cannot be uninstalled. Core protocol 5 remains compatible with existing
GFN envelopes. Provider control uses protocol 2, native JSON uses protocol 8,
and the native C ABI is version 12.

## Native plugins are trusted code, not a sandbox

A plugin runs with the user's operating-system permissions. It can access user
files, credentials available to that user, and the network. Clearing inherited
environment variables prevents accidental secret handoff. It does not restrict
what a malicious native executable can access.

Inside Flatpak, children inherit OpenNOW's application permissions, including its
keyring access. The application sandbox does not isolate a plugin from OpenNOW's
own resources. Platform execution policies still apply.

Inspection does not execute either role. Installation requires explicit consent
and leaves the package disabled. Hashes identify the inspected bytes, not the
publisher. Publisher metadata is self-declared. There is no marketplace,
automatic download, or in-place package update in this implementation.

Protocol bounds, deadlines, process cleanup, and package pins protect application
lifecycle behavior. They are not CPU, filesystem, or network restrictions on
trusted native code. Existing GFN reporting consent does not authorize uploading
another provider's account IDs, game IDs, bootstrap data, or input text.

## Why not another player or an in-process library?

The existing NVST and GFN WebRTC paths have NVIDIA-specific negotiation and
input behavior. Changing a signaling URL does not turn them into a generic
service client. A provider media worker owns that service-specific transport and
supplies encoded media through a bounded protocol instead.

This keeps decoding and presentation in the existing player while separating
provider transport failures from the core process. An in-process library would
share memory corruption and unsafe unloading with OpenNOW. A separate presenter
would duplicate focus, overlays, scaling, and input ownership.

A sandboxed runtime would need brokered storage, network, and authentication
APIs. This implementation uses an explicit trusted-native tier rather than
claiming that process separation provides those guarantees.

The [provider reference](provider-plugins.md) describes the contracts. The
[standalone demo](../examples/provider-plugin/README.md) describes authoring and
packaging. The [catalog reference](plugins.md) remains the protocol-1 contract.
