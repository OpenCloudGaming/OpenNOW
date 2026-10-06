# Provider modules and community plugins

OpenNOW separates provider services from the application host. The built-in
GeForce NOW module owns NVIDIA and Alliance authentication, catalogs, account
services, CloudMatch sessions, and provider-specific recovery. Qt still owns the
interface and the native streamer still owns media, input, decoding, and recording.

Community plugin version 1 supports catalog previews. It does not support
third-party authentication, session allocation, or playback. Boosteroid, Xbox
Cloud Gaming, and personal-PC streaming need real integrations and compatible
native transports before they can become playable sources.

## Ownership

The application core composes a source host with the built-in GFN module and the
community plugin manager. Application dispatch, reporting, and update admission
do not receive a concrete `GfnService`.

The source host has two contracts:

- The shared catalog contract supplies validated, bounded pages to the Plugins
  preview. Both GFN and community plugins implement it.
- The built-in module contract retains the existing GFN RPCs and their lifecycle
  obligations. Community plugins cannot register those methods or impersonate
  their events.

This separation is intentional. GFN's existing account and session schemas are
not a universal provider API. Reusing them for other services would make those
services imitate NVIDIA membership, Alliance identities, and CloudMatch state.
The current Qt account, library, and launch flows retain their GFN contracts.

The GFN module owns its push registry, region validation, allocation policy,
session receipts, reporting identity, and stream-preparation authorization. The
host retains settings persistence, application diagnostics, updates, and local
media services. The module receives only the host services it needs, not the
entire application object.

## Existing accounts and sessions

The extraction preserves GFN's existing data paths and credential-store names.
There is no move into a new plugin directory, new account identity, or reset of
saved profiles, PINs, device IDs, or pending session cleanup.

A fresh allocation retains the original service owner until Qt acknowledges
delivery or the receipt expires. Cancellation of the request does not cancel the
obligation to settle that receipt. Failed remote cleanup remains observable and
does not become a successful local termination.

Stream preparation still resolves the retained GFN seat under its ownership
guards. A caller-supplied session cannot replace that seat. Update admission
checks retained allocation and cleanup state rather than treating a different
selected account as proof that the provider is idle.

Provider activity reports come from requests admitted to the module. A request
rejected by the host's update or session gate does not record a provider launch
attempt. Such rejections still return their existing protocol errors.

The shell/core protocol remains version 5, native streamer JSON remains version
7, and the C ABI remains version 11. Plugin management and catalog preview use
separate optional capabilities. Existing GFN request and event shapes remain
unchanged.

## Community execution and trust

A community plugin is a self-contained native executable with a versioned
catalog protocol. The core launches it in a separate process and validates its
messages. The plugin does not load a DLL into OpenNOW, inject QML, create a new
presenter, or send commands to the native streamer.

**A separate process is not a security sandbox.** A plugin runs with the user's
operating-system permissions. It can read user files, access credentials
available to that user, and use the network. This remains true even though
OpenNOW does not pass the plugin its NVIDIA credentials or inherited secret
environment variables. Install and enable only code you trust.

Inside Flatpak, a child inherits OpenNOW's application permissions, including
its `org.freedesktop.secrets` keyring access. The application sandbox does not
isolate a plugin from OpenNOW's own resources.

Inspection and installation do not execute the plugin. Installation requires
explicit consent and leaves the plugin disabled. Enabling starts the inspected
code. Package hashes bind the reviewed bytes and detect changes. They do not
authenticate a publisher, and publisher metadata is self-declared.

The host bounds protocol frames, queued work, calls, and deadlines. It terminates
failed or unresponsive processes instead of retrying them indefinitely. These
are protocol and lifecycle limits, not restrictions on malicious code's CPU,
memory, filesystem, or network access outside the protocol.

Version 1 does not download plugins, automatically update them, or replace an
installed package. Uninstalling a community plugin removes its package and its
private data. GFN is a required built-in module and cannot be uninstalled.

## Why not a universal streaming API yet?

The existing NVST engine implements NVIDIA's protocol. The existing WebRTC
compatibility engine also uses NVIDIA-specific signaling and session metadata.
Neither is an arbitrary WebRTC client.

Plugin version 1 rejects authentication, session, playback, and executable-UI
capabilities. Its catalog results contain plain titles and source-qualified IDs,
not stream URLs or Play actions. That restriction keeps the first community
contract testable without weakening GFN's session ownership checks.

A future streaming contract needs another real provider implementation, typed
engine-specific connection data, allocation receipts, and recovery tests. It
must keep credentials and host media policy out of arbitrary plugin payloads.

## Alternatives considered

In-process native libraries would share crashes, memory corruption, and unsafe
unloading with the application. They are not used.

A WebAssembly sandbox could enforce narrower access than trusted executables.
It would also need a supported runtime and brokered network, storage, and
authentication APIs for provider integrations. This version chooses an explicit
trusted-native tier rather than claiming subprocesses provide those guarantees.

Moving GFN into another executable would add a process-failure boundary to its
existing credential, push, and cleanup lifecycle. Keeping it built in isolates
its ownership without changing that failure model.

For the public protocol and package format, see [Plugin API](plugins.md). For a
standalone authoring example, see [Example catalog plugin](../examples/catalog-plugin/README.md).
