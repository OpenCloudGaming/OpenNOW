# Alliance account-library audit

This audit compares the Qt/Rust client at `a41fb3d5` with OpenNOW-Mac at
`7327dd7`. It covers provider selection, credentials, catalog routing, ownership,
and delivery to the library UI. The separate RTSPS Transport investigation is
not part of this library change.

## Findings and implementation

### Library delivery can exceed the Qt protocol limit

`GfnService::library_catalog` previously combined up to 2,000 mapped games in
one result, with Qt requesting 1,000. `CoreClient::processStdout` rejects a JSON
line larger than 1 MiB. The Rust RPC log records success before Qt receives that
line, so an `outcome=ok` entry does not establish that the library rendered.
Mac does not have this process-message limit and walks its library pages within
the application.

The core now returns complete cursor pages capped at 100 games and 768 KiB,
using the same bounded-page implementation as Store. Oversized pages are
refetched at the original cursor with a smaller count. Qt appends pages
progressively, retains the search, rejects cursor cycles, and cancels obsolete
requests when the account or core connection changes. A partial failure remains
an error rather than becoming a complete-looking library.

### Provider metadata and catalog requests could use different routes

Mac's `OPNGameService+Provider.swift` uses the configured control-plane session
for `/v2/serverInfo` and prefers top-level `vpcId` or `vpc_id`, then
`requestStatus.serverId`. The core previously read only `serverId`, bypassed the
configured proxy for this lookup, and cached its result without a proxy scope.

The core now follows the Mac field precedence and uses the configured route for
discovery. Its metadata cache separates provider, account, token, and proxy
route. Subscription requests use that route too. The existing server-ID
fallback and endpoint allowlist remain intact.

Public, unauthenticated probes during this audit returned only
`requestStatus.serverId` for NVIDIA, ABYA, and Turkcell. Their service URLs
passed the existing allowlist. Those observations do **not** establish that the
missing top-level VPC support caused this tester's failure, and no provider
hostname restriction was weakened.

### Mapping discarded some server-provided library data

Mac's `OPNGameService+Parsing.swift` accepts integer IDs and treats a selected
variant, or a nonempty status other than `NOT_OWNED`, as owned. The core accepted
only JSON string IDs and three exact ownership status strings.

The shared catalog mapper now preserves integer IDs as strings and matches the
Mac ownership rule, including whitespace/case normalization for the ownership
decision. It retains the original status value. Public unauthenticated NVIDIA
queries returned string IDs; integer-ID handling is compatibility coverage,
not a claim about the tester's unseen payload.

### Library success did not require a valid apps response

Mac's page fetcher fails with `No apps data` when the apps object is missing.
The core silently treated missing/null apps or items as an empty library. It
also treated absent pagination as the end of the result.

The core now requires a games array and valid pagination. Credential-free page
counts distinguish upstream rows, mapped rows, and rows left after search
without logging game titles, queries, cursors, or credentials.

### ID-token expiry was independent of the tracked access-token expiry

Mac validates ID-token expiry independently. The core refreshed only against
its access-token lifetime while preferring the ID token for authenticated
requests. It could also preserve an expired ID token when refresh omitted a
replacement.

The core now includes known ID-token expiry in its refresh decision, avoids
sending an expired ID token when a usable access token remains, and drops an
expired ID token omitted by refresh. Catalog, CloudMatch, linked accounts, and
storage share this token-selection rule. It does not change server-side token
verification or treat decoded JWT claims as authorization.

## Paths that already matched

- Both clients use the same device-code OAuth client and scope, and prefer the
  ID token for GFN catalog authorization.
- Both send `GFNJWT` authorization to `games.geforce.com/graphql`, using the
  same NVIDIA client ID and version.
- Both use the library filter
  `variants.gfn.library.status.notEquals = NOT_OWNED`.
- The library POST does not need an Alliance-specific user ID, external ID, or
  `huId`. No speculative identity headers or public-catalog fallback were added.

The Mac client can reconcile provider identity from returned session claims;
the core retains the provider selected for device authorization. This remains
an audit lead, not a demonstrated mismatch in the supplied logs. Changing
provider identity without an affected session capture would be speculative.

## Verification and remaining limit

`gfn_catalog_tests.rs` exercises a local HTTPS provider and library endpoint
with generated, in-memory certificates. It checks VPC precedence, authenticated
request fields, paging and encoded-size limits, mapping, malformed responses,
proxy isolation, and ID-token refresh. The Qt `qml-library-paging` test covers
progressive loading, empty filtered pages, cancellation, errors, and bounds.

Run the core tests with `cargo test --locked --manifest-path
native/opennow-core/Cargo.toml`; run the registered Qt test with
`ctest --test-dir build/opennow-qt -R qml-library-paging --output-on-failure`.

The tester's authenticated library response has not been captured. These are
demonstrated client defects and reference-parity fixes, not proof that the
affected Alliance account now loads or that its stream can start. Live
validation still requires that account on the tester's machine.
