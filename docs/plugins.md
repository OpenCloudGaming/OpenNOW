# Plugin API v1

This reference describes trusted community catalog plugins. It is not a
streaming-provider SDK. The [module architecture](provider-modules.md) explains
the ownership and security boundaries.

## Application capabilities

`core.hello` advertises `plugins.v1` and `sources.catalog.v1` when plugin
management and catalog previews are available. These capabilities are optional.
They do not change shell/core protocol version 5.

The built-in source ID is `org.opennow.geforce-now`. Community manifests cannot
claim that identity. Each community plugin supplies one source in version 1.

## Management RPCs

The methods use the normal core request and response envelope. Registry
generation is process-local. Clients discard it when the core connection
restarts and fetch a new snapshot.

| Method | Parameters | Result |
| --- | --- | --- |
| `plugins.list` | `{}` | `{generation, plugins}` |
| `plugins.install.inspect` | `{path}` | `{generation, inspection:{token, expiresAt, plugin, packageSha256}}` |
| `plugins.install.commit` | `{token, expectedGeneration, consent:true}` | Registry snapshot |
| `plugins.install.cancel` | `{token}` | `{cancelled:true}` |
| `plugins.setEnabled` | `{id, enabled, expectedGeneration}` | Registry snapshot |
| `plugins.uninstall` | `{id, expectedGeneration, confirmed:true}` | Registry snapshot |

`path` is a local absolute path or file URL. Remote package URLs are not accepted.
Inspection stages the package without execution. Its short-lived, single-use
token identifies those staged bytes, not a path that the client can replace
between inspection and consent.

Installation leaves the plugin disabled. An existing installed ID is rejected;
version 1 has no package-replacement operation. Uninstall removes both the
package and its private data. Required built-ins cannot be disabled or removed.

`plugins.changed` carries `{generation}`. Clients fetch `plugins.list` rather
than applying a partial update to their cached descriptor list. A stale mutation
is rejected rather than replayed automatically.

### Plugin descriptor

Descriptors contain `id`, `name`, `version`, `publisher`, `description`,
`builtin`, `required`, `enabled`, `state`, `capabilities`, `trust`, and
`lastError`. State is `disabled`, `starting`, `ready`, or `failed`. Trust is
`builtin` or `unsigned-native`.

`lastError` is null or a host-authored `{code,message}` object. It does not expose
child stderr, malformed message fragments, or executable paths. Name, publisher,
description, and catalog titles are plain text, not HTML or QML.

## Catalog RPC

`sources.catalog.page` takes:

```json
{"sourceId":"org.opennow.example.catalog","query":"","cursor":null,"limit":20}
```

It returns:

```json
{
	"sourceId": "org.opennow.example.catalog",
	"generation": 1,
	"items": [
		{
			"id": {"sourceId": "org.opennow.example.catalog", "localId": "example-1"},
			"title": "Example game"
		}
	],
	"nextCursor": null,
	"coverage": "complete"
}
```

The host supplies source identity and generation. The executable returns only
local IDs. A local ID from one plugin cannot identify an item in another plugin.

Queries are at most 512 UTF-8 bytes. Cursors are at most 4096 bytes. Page limits
range from 1 to 100, with a default of 20. Item IDs and titles are nonempty and at
most 256 bytes. Duplicate item IDs within a page are rejected.

Coverage is `partial`, `complete`, or `unknown`. A null cursor does not by itself
mean the upstream catalog is complete. GFN's preview retains that distinction
and does not invent paging support its public catalog lacks.

Catalog results have no Play action. They cannot supply URLs, native descriptors,
authentication prompts, or host commands. The Plugins UI renders results through
OpenNOW's own components.

## Package and executable contract

A `.opennow-plugin` package is a ZIP archive containing `manifest.json` and its
listed payload files. The manifest declares schema and protocol versions,
identity, version, publisher, description, `catalog.v1`, target-specific
entrypoints, and SHA-256 file hashes.

The manifest fields are:

```json
{
	"schemaVersion": 1,
	"id": "org.example.catalog",
	"name": "Example catalog",
	"version": "1.0.0",
	"publisher": "Example author",
	"description": "A read-only catalog extension.",
	"protocolVersion": 1,
	"capabilities": ["catalog.v1"],
	"entrypoints": {
		"x86_64-unknown-linux-gnu": "bin/catalog-plugin"
	},
	"files": [
		{
			"path": "bin/catalog-plugin",
			"sha256": "0000000000000000000000000000000000000000000000000000000000000000"
		}
	]
}
```

The hash above is a placeholder. Each payload needs its actual lowercase
SHA-256 digest. `manifest.json` is not listed in its own inventory. Entrypoint
keys are Rust target triples, and the installed host must have a matching
entrypoint. The archive contains files, not directory entries or links.

Archives are limited to 64 MiB, with 128 MiB total expanded data, 128 members
including the manifest, and a 64 KiB manifest. Each payload file is at most
64 MiB. Package paths are at most 240 UTF-8 bytes.

The host rejects unsupported capabilities and targets, unlisted payloads, missing
or changed files, path traversal, absolute paths, links, duplicate paths, and
case-colliding paths. There are no installation hooks, executable arguments,
interpreter requirements, or plugin-provided environment variables.

Executables exchange versioned newline-delimited JSON over private standard
input and output. The protocol has hello, catalog-page, cancellation, and
shutdown messages. The plugin echoes the host-issued epoch and request ID.
It cannot send events or call host RPCs. Standard error is discarded, not added
to OpenNOW diagnostics.

The canonical Rust data types are in
[`opennow-plugin-api`](../native/opennow-plugin-api/). The
[standalone example](../examples/catalog-plugin/README.md) documents the build,
package, and request loop without depending on the application core.

### Child messages

Each message occupies one UTF-8 line. Unknown fields, unsupported operations, and
responses with both a result and an error are rejected.

The host starts with a hello request:

```json
{"v":1,"type":"request","epoch":7,"id":"1","op":"plugin.hello","args":{"pluginId":"org.opennow.example.catalog","capabilities":["catalog.v1"]},"timeoutMs":5000}
```

The plugin returns its installed identity and version:

```json
{"v":1,"type":"response","epoch":7,"id":"1","ok":true,"result":{"pluginId":"org.opennow.example.catalog","version":"1.0.0","protocolVersion":1,"capabilities":["catalog.v1"]}}
```

A catalog request and response use the same correlation fields:

```json
{"v":1,"type":"request","epoch":7,"id":"2","op":"catalog.page","args":{"query":"","cursor":null,"limit":20},"timeoutMs":10000}
{"v":1,"type":"response","epoch":7,"id":"2","ok":true,"result":{"items":[{"id":"example-1","title":"Example game"}],"nextCursor":null,"coverage":"complete"}}
```

The host adds the source-qualified item reference after validating that response.
An executable cannot choose another source ID by including it in an item.

Cancellation identifies the outstanding request:

```json
{"v":1,"type":"cancel","epoch":7,"id":"2"}
```

Plugin errors contain a code, not free-form diagnostic text:

```json
{"v":1,"type":"response","epoch":7,"id":"2","ok":false,"error":{"code":"cancelled"}}
```

The other allowed error codes are `invalid_request`, `unsupported_capability`,
and `internal_error`. The host maps them to its own display messages.

Shutdown uses a request with an empty argument object:

```json
{"v":1,"type":"request","epoch":7,"id":"3","op":"plugin.shutdown","args":{},"timeoutMs":2000}
{"v":1,"type":"response","epoch":7,"id":"3","ok":true,"result":{}}
```

The epoch must match the running process instance, and the response ID must
match the outstanding request. A restart does not reuse old requests.

### Deadlines and cancellation

The host permits one outstanding catalog call per executable. Frames are
limited to 1 MiB, and each pipe queue holds one frame. The hello deadline is
five seconds. Catalog requests have a ten-second deadline.

After cancellation, the host allows up to 500 ms for the outstanding request's
correlated response. A cooperative plugin can return `cancelled` and keep
running. If a request already completed, a later Cancel message for that ID
must not produce a second response.

If a cancelled request does not finish within that grace period, the host
terminates its process. It preserves the user's enabled choice, rechecks the
installed package, and attempts one fresh handshake. A failed restart disables
the plugin with an error. There is no restart loop. Clients discard an old
catalog cursor when the source generation changes.

A valid plugin-reported request error does not disable the plugin. Malformed
messages, mismatched response identities, process failure, and expired request
deadlines do.

Explicitly disabling an idle plugin allows up to two seconds for shutdown
before forced cleanup. Core exit instead terminates all plugin processes first,
then uses one shared 500 ms reap budget. It does not wait two seconds for each
plugin before closing OpenNOW.

## Security limits

Native plugins are unsigned, trusted code running as the user. Neither manifest
capabilities nor checksums create a sandbox or authenticate the author. Package
inspection does not execute code, and enabling is separate from installation.

The host clears inherited environment variables and validates package integrity
before launch. This prevents accidental credential handoff through the plugin
API. It does not prevent a malicious native executable from accessing the same
user's files or credentials directly.

Operating-system execution policies still apply. OpenNOW does not disable
Gatekeeper, antivirus, or Flatpak confinement to run a plugin. A package must
contain an executable compatible with the installed OpenNOW platform.
