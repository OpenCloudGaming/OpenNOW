# OpenNOW shell/core protocol

The Qt shell and Rust application core communicate over newline-delimited UTF-8
JSON on inherited standard input/output. Standard error is reserved for redacted
diagnostics. A line is limited to 1 MiB. Unknown, malformed or oversized protocol
messages terminate the core connection instead of leaving the shell in an
ambiguous state.

## Handshake

The first shell request is always:

```json
{"type":"request","id":"1","method":"core.hello","params":{"protocolVersion":5,"shell":"qt","shellVersion":"0.5.4"}}
```

Protocol 5 requires an exact selected catalog variant and account scope for fresh
session allocation. It retains the bounded library-page contract introduced in
protocol 4. Older shells are rejected with `incompatible_protocol` during
`core.hello`, and protocol-5 shells reject older cores. The native streamer
protocol remains 7.

The core must return the same protocol version and its capabilities. The shell
does not send product requests before this succeeds. Version mismatches, a
five-second handshake deadline, process exit and invalid data all transition the
transport to `failed` with a credential-free diagnostic.

## Cloud library actions and launch decisions

`catalog.launch.inspect({appId, variantId})` always resolves the exact parent and
store variant. Its response contains `appId`, `variantId`, `game`, `scope`,
`catalogRevision`, `fetchedAt`, `freshness`, and `decision: {status, message}`.
Statuses are `ready`, `ownership_required`, `selection_required`, `link_required`,
`subscription_required`, `patching`, `maintenance`, `unavailable`, and
`metadata_unconfirmed`. Only the selected variant's `MANUAL` or `PLATFORM_SYNC`
library status records ownership. App-wide library flags, Home pins, favorites,
free-game labels, store subscriptions, and ownership of another variant do not.
Store-link requirements use the store definitions and fresh account state.
Recorded store-subscription IDs must match the account's active subscriptions.
App playability, variant readiness, patch metadata, and membership restrictions
remain independent checks. Unknown metadata is not a positive authorization.

All five mutation RPCs require a bounded nonempty parent `appId` and the current
`scope: {generation, userId, providerIdpId}`. Ownership mutations also require the
exact `variantId`. `catalog.ownership.add` requires
`confirmedExistingLicense: true`, the user's assertion that they already own the
selected store license. This operation neither buys nor grants a license.
`catalog.ownership.select` accepts only an owned variant and does not add ownership.

Favorites map to the GraphQL `AddFavoriteApp` and `RemoveFavoriteApp` operations
with `appId` and `locale` variables. Ownership maps to `AddOwnedVariant`,
`RemoveOwnedVariant`, and `SelectOwnedVariant`, with `cmsId` and `locale`
variables. `cmsId` supplies the `variantId` argument. Every mutation selects
`app { id }` and checks the returned parent identity. Mutations share catalog
scheduling and serialize conflicting actions for the same account and parent
app. Fresh allocation holds that same app admission while checking and creating.

Mutation results distinguish `outcome: acknowledged|unconfirmed` from
`reconciliation: confirmed|unconfirmed`. A fresh exact read confirms the desired
current state; acknowledgement alone does not. Results include captured target
IDs, `operation`, `scope`, `catalogRevision`, nullable refreshed `game`, a visible
`message`, and sanitized `error.code`, `error.httpStatus`, and `error.graphql`
code/path details. `reconciliationCode` and `invalidationCode` distinguish read
or cache-invalidation failures from the mutation outcome.
Neither ambiguous failures nor HTTP 401 cause mutation replay. Catalog pages are
invalidated before sending and again after the attempt. Cancellation may prevent
delivery of the outcome without preventing an already-sent upstream mutation.
The shell marks that outcome unconfirmed and refreshes metadata without resending.

`catalog.favorites.list` reads the `FAVORITES` panel using `GetGameSection` with
`panelNames`, `vpcId`, and `locale`; it has no cursor. It preserves all parsed game
items up to the 1,000-game and 768-KiB response bounds, rather than the Store
panel's 24-item display limit. The response contains `games`, section identity
and `seeMoreInfo`, `scope`, `catalogRevision`, and `fetchedAt`. `coverage` is
`unknown` and `complete` is false because the panel does not establish global
completeness. No pagination or see-more request is invented. A failed read keeps
the shell's last usable list, and a specific favorite is reconciled through the
exact app's `library.favorited` value. Existing `favoriteGameIds` remain local
Home pins; pinning, collections, hiding, and reordering do not upload favorites.

## Messages

### Authentication boundary

Protocol 5 auth responses and `auth.session.changed` events use the same envelope.
`session` is either null or an allowlisted object containing `user` and `provider`.
Credentials and issuing-client details remain private to the core. The envelope
includes the core-process account `generation`, `persistence`, `refresh`, `warnings`,
and `deviceIdentity`. Consumers reject older generations and reset their generation
when a new core process completes its handshake. A token refresh does not change
account generation.

`auth.device.start` returns `attemptId`, `userCode`, verification URLs, `qrRows`,
`expiresAt`, and `intervalSeconds`. Poll, complete, and cancel use `attemptId` only.
The core retains the device grant and enforces the deadline, one in-flight poll,
and cumulative five-second `slow_down` increments. Pending poll replies include
`retryAfterMs`; the shell schedules one subsequent poll from that reply.
Cancellation before the completion commit fence prevents persistence and publication.
Once the fence is entered, completion is committed under auth ownership; cancelling
its response does not undo that login. The shell reconciles such cancellation with
`auth.session.get`. Logout or account replacement invalidates pending login/link work.

Persistence intent is independent of outcome. `secure-store` means the session was
written to and verified in the OS credential store and its nonsecret index committed.
`memory-only` never writes a plaintext credential fallback. `migration-pending` means
a recoverable legacy source remains because secure migration could not finish.
`unavailable` means restoration failed; warnings identify deferred cleanup without
containing credentials. An explicitly temporary login suppresses automatic restoration
of older saved grants for that identity. Legacy sources are removed only after verified
secure persistence and metadata commit, or explicit account removal. This is file
cleanup, not guaranteed secure erasure of backups or snapshots.

Logout always ends local auth ownership and reports `remoteRevoke` separately from
`localCleanup`. Revocation is best-effort DELETE of the selected client grant with an
access bearer; all-account revocation has a five-second total network budget. Failed
local deletion remains suppressed in the nonsecret account index and is retried on
startup. A metadata write failure reports pending cleanup and cannot guarantee that
suppression survives restart. Logout still selects the next saved account when available.

The core persists one versioned 64-hex device identity. Upgrades freeze the previously
derived identity using the current environment; if that environment changed before the
first upgraded launch, the former hash cannot be recovered. Corrupt or unwritable identity
storage is not replaced, new login is blocked, and restoration reports unavailable device
identity rather than claiming durable compatibility.

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

The core admits at most eight RPC workers, with at most four background workers
(`catalog.*`, `artwork.*`, and `network.regions.ping`). The remaining capacity
is reserved for other methods, including session/control operations. Excess
requests receive `busy`; duplicate active IDs are also rejected. Cancellation
only tracks active IDs and never frees a worker slot before that worker exits.
The Qt client keeps requests rejected with `busy` pending and retries the same
ID and payload after 100 ms, doubling the delay up to one second. Retries do not
extend the original deadline. Cancellation, shutdown, and process failure discard
pending retries. Other errors are delivered to the caller without retrying.
Cancelled requests suppress their response. Store page retries/cache traversal
and region measurement loops stop at cooperative checkpoints. An already-running
blocking HTTP, DNS, or TCP operation is not forcibly interrupted; its existing
timeout still applies. Other mutating operations already dispatched are not rolled back.

Protocol 5 retains the requirement for the Qt client to acknowledge an accepted successful `session.create`
response with `{"type":"ack","id":"42"}` before delivering that response to QML.
Cancelled or timed-out requests do not acknowledge late responses. The create worker
retains its admission slot for at most ten seconds awaiting acceptance, then the
CloudMatch owner deletes an unaccepted fresh allocation with an eight-second HTTP
deadline. Cancellation before or during the compatibility RESUME uses the same cleanup.
The receipt does not apply to claims of existing sessions. A create response is not
also broadcast as `session.changed`, preventing a late event from reviving a cancelled
launch. Cleanup failure retains the seat and its scoped discovery route for explicit
retry, blocks another fresh allocation, and reports `session_cleanup_pending`.
The failed-cleanup record persists only the seat identity, app/status, trusted control
route, original provider/account identity, and error code in `pending-session-cleanup.json`;
it contains no tokens or signaling secrets. Discovery surfaces it only to that account.
Explicit successful DELETE or not-found clears that exact record, never a different seat.
HTTP-success DELETE responses containing an explicit vendor rejection retain ownership.
The CloudMatch owner reserves fresh-create admission before endpoint resolution or POST.
Concurrent creation, pending handoff, and in-progress cleanup lock contention fail promptly with
`session_update_busy`, not the automatically retried `busy` response. Admission is
released on every pre-ID error; after allocation the exact seat remains reserved until
handoff or compensation completes. Cleanup records are read through a 16-KiB limit plus
one overflow-detection byte before parsing, including for corrupt or oversized files.

Raw CloudMatch status 7 is `phase: "finished"`, with a `termination` object containing
`source: "cloudmatch-session-status"`, `status: 7`, and `resumable: false`. This proves
the seat is terminal, not whether the game exited successfully; no vendor reason is
invented. A targeted GET returning HTTP 404 yields `session: null` and a top-level
termination with `source: "cloudmatch-http"`, `httpStatus: 404`, `sessionId`, and
`resumable: false`. Authentication, invalid payloads, other HTTP failures, empty lists,
and native transport closure are not terminal evidence. Recovery first polls the exact
seat before claiming it. `session.poll` with `recoveryMode: true` does not broadcast a
session change while the recovery owner is deciding whether to claim.

Negotiated profiles retain normalized `bitDepth` (8 or 10), CloudMatch `chromaFormat`
(0 or 1), and per-component `*Source` fields. `request`, `finalized`, `server`, and
`unreported` distinguish present values from omissions. Same-seat partial updates retain
previously known components; explicit finalized invalid values are not replaced with
saved preferences. Incomplete or unsupported accepted color is rejected before native
attachment. The native ABI and protocol version are unchanged; transport terminal
events add `termination: {source: "nvst-transport", code, resumable: null}`, preserving
unknown cloud-session disposition rather than assigning a normal-exit reason to EOF.

## Implemented core methods

### Game artwork

GraphQL-backed game objects expose nullable artwork URL strings: `imageUrl` prefers
`GAME_BOX_ART`, `heroImageUrl` prefers hero/banner imagery, and `keyArtUrl` prefers
NVIDIA `KEY_ART` with `KEY_IMAGE` as a fallback. Console Home uses `keyArtUrl` for
square tiles while retaining hero imagery for wide tiles. If key art is absent,
the shell falls back to `imageUrl`, then `heroImageUrl`. Older cached objects and
public-catalog games may omit `keyArtUrl`; it is an optional, additive field in
protocol 1 and does not change the handshake or existing artwork fields.

### Store pagination (`catalog.storePages.v1`)

The protocol-1 envelope and 1 MiB limit are unchanged. This additive capability
advertises cursor pagination and separate storefront presentation. The existing
`catalog.store.list` method now caps `limit` at 100 (default 100), returning one
complete upstream page instead of aggregating thousands of games.

Fresh upstream pages request up to 100 games. Existing cached pages may contain
fewer games; always follow the returned cursor rather than assuming a full page
has the requested count.

Request: `{ "limit":100, "cursor":"", "searchQuery":"" }`. Cursor is an opaque
string (at most 4096 UTF-8 bytes); search is at most 512 UTF-8 bytes. Response:
`{ "games":[], "count":0, "totalCount":0, "hasNextPage":false,
"nextCursor":"", "source":"store-browse", "fetchedAt":0 }`.
Pass `nextCursor` unchanged to the next call with the same search. A final page
has `hasNextPage:false`; empty non-final pages may advance only when the upstream supplies an advancing cursor. Invalid mapped identities are errors.
Missing, repeated or oversized continuation cursors are errors, not completion.

Each result is limited to 768 KiB after JSON encoding, leaving room for the
envelope. Oversized pages are refetched with a smaller count at the same cursor;
they are never truncated while advancing the cursor. A single oversized game
returns `catalog_response_too_large` without disrupting the core connection.

`catalog.store.presentation` accepts `{ "section":"marquee" }` (also `panels`
or `filters`) and returns `{ "section":"marquee", "items":[] }`, independently
bounded to 768 KiB. Failed/oversized optional sections do not fail game pages.

Store pages and presentation results include `cacheHit` (boolean). Successful
responses are persisted under the core data directory in `store-cache-v1`, with
hashed account/provider/membership/proxy/locale and request keys. Credentials
are not stored. Reads remain bounded to 768 KiB; the cache is limited to 64 MiB
and 512 entries. Missing or corrupt entries refetch normally. There is no timed
catalog invalidation: an optional `refresh:true` on a first-page request clears
that account/context's pages and presentation before fetching. Continuations
must omit it or send false. Other accounts' entries are unaffected.

Concurrent misses for the same cache key and refresh epoch share one successful
fetch. Store network requests (including server metadata, presentation fallbacks,
and oversized-page retries) are serialized with at least 50 ms between them.
HTTP 429 starts a core-wide Store cooldown using `Retry-After` (seconds or HTTP
date), or 60 seconds when it is absent or invalid. Uncached requests during the
cooldown return `rate_limited` without network traffic; cached responses remain
available. The core does not automatically retry a rate-limited request.

Server metadata lookups share one bounded, in-memory cache across Store, library,
and subscription requests. A successful lookup is reused for five minutes, scoped
by provider endpoint, account, and token fingerprint; concurrent callers share
the lookup. Ordinary failures use the same context's last known value (or
`GFN-PC` when none exists) for 30 seconds before retrying. Rate-limit and
cancellation errors propagate instead of becoming cached fallback values.

The shell serializes game requests, merges by stable game identity, and retains
loaded games and the failed cursor on error. Retries resume that page.
Search/account changes cancel requests before clearing state; late responses
are ignored.

### Local Store browsing (`catalog.storeLocal.v1`)

`catalog.store.local` pages and searches the saved catalog, without replaying
every cached page into Qt. It accepts `limit` (capped at 60), `cursor`,
`searchQuery`, and optional `genre`, `store`, and `categoryId` strings (at most
256 UTF-8 bytes each). A cursor belongs to its search/filter context and must
be returned unchanged. The existing 768 KiB result budget still applies.

The result includes `games`, `count`, `totalCount` (matching games),
`catalogTotalCount`, `hasNextPage`, `nextCursor`, `source:"store-local"`,
`cacheHit:true`, and `cacheComplete`. First pages also include `facets` with
all indexed genres, stores, and official categories (`id`, `label`, `count`),
not just those present on the visible page. Subsequent pages omit facet data
by returning `facets:null`. `categoryId:"all"` selects the entire catalog.

The core builds one bounded, account-scoped metadata index from saved pages;
full records remain on disk and only selected results are materialized.
Search ranks exact titles, acronyms, prefixes, reordered words and single
typing errors, with owned games preferred on close matches. Store requests
40 results per page; Ctrl+K uses the same algorithm with a six-result limit
and debounced, cancellable requests.

There is no automatic catalog crawl. Scroll/navigation demand or Load more
requests the next page. A missing/partial cache fetches bounded upstream pages
on demand; `cacheComplete:false` distinguishes this from a complete index.
On this method, `refresh:true` rebuilds the local index without deleting saved
pages. The upstream `catalog.store.list` refresh behavior remains unchanged.

`catalog.store.presentation` also accepts `metadataOnly:true`. For `panels`,
each section returns its title and `totalCount` with an empty `games` array;
the complete panel remains cached in the core. Qt materializes shelf games
and artwork only near the viewport, using the section's local category ID
(`shelf:<panel index>:<section index>`). See all opens that category in Store.

### Method list

- `core.hello`
- `app.status`
- `settings.get`
- `settings.choices.get`
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
- `catalog.library.list` returns one bounded upstream page, not an aggregate library.
- `catalog.game.get`, `catalog.definitions.get`, `catalog.languages.get`
- `catalog.launch.inspect`, `catalog.favorites.list`
- `catalog.favorites.add`, `catalog.favorites.remove`
- `catalog.ownership.add`, `catalog.ownership.remove`, `catalog.ownership.select`
- `catalog.store.list`, `catalog.store.local`, `catalog.store.presentation`
- `network.regions.list`
- `network.regions.ping`
- `account.subscription.get`
- `account.connections.list`, `account.connections.sync`, `account.connections.unlink`
- `account.connections.sync.status`, `account.connections.sync.cancel`
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
- `updater.state.get`, `updater.check`, `updater.download`, `updater.install`, `updater.startup.ack`
- `updater.highlights.get`, `updater.highlights.ack`
- `social.capabilities.get`
- `discord.activity.sync`, `discord.activity.clear`
- `telemetry.sync`, `feedback.submit`, `bug_report.submit`

`diagnostics.export` optionally accepts `embeddedStream.drops` and
`lastSessionReport.drops` from the Qt session owner. Each contains the cumulative
`videoDropCount` (frames), `audioDiscardedMs` (decoded audio duration),
`audioPacketDropCount` (audio packets/PCM blocks with unknown duration),
`callbackDropCount` (Qt callbacks), and `otherQueueDropCount` (unclassified items).
The core copies only bounded, non-negative numeric counters into the export's
`shell` section; it does not export arbitrary caller-provided fields. These
counters survive embedded-runtime stop and remain separate from the core's
process-streamer snapshot and its legacy mixed-unit `queueDropCount`.

`diagnostics.export` also optionally accepts `runtimeCapabilities` from the
in-process Qt streamer. The export's separate `nativeRuntime` section preserves
allowlisted backend and codec availability, HDR support, and bounded, redacted
failure reasons, even before a stream starts. It does not copy arbitrary runtime
fields or treat an empty process-streamer snapshot as the embedded capabilities.

### Session resume and reconnect

`session.remote.list` checks the selected endpoint and the regions advertised by
CloudMatch instead of treating the first empty regional response as authoritative.
Discovery deduplicates session IDs and uses at most four concurrent requests, a
three-second per-request timeout, a 12-second total budget (including region
discovery), and at most 32 regional endpoints. If no sessions were found but a
region failed, returned invalid data, or could not be checked within these bounds,
the request fails with `session_discovery_failed` rather than returning an empty
list. HTTP 401 remains `http_unauthorized` after at most one safe-read renewal;
HTTP 403 remains `authentication_required`. A found session
can be returned even when another region fails; an empty successful list means
all discovered regions were checked successfully. Callers must not create a new
session after a discovery failure.

### Provider routing and account scope

`auth.providers.list` returns `providers`, `defaultProviderIdpId`, `generation`, and
`discovery: {state, message, retryAfterMs}`. Successful discovery is fresh for
15 minutes. A failed refresh retains the last known providers and reports
`state: "degraded"`. Without a discovered list, the response includes the restored
provider, when present, and an explicit NVIDIA fallback. This fallback is not
authoritative discovery. Retries wait at least 30 seconds; HTTP 429 `Retry-After`
can extend the wait up to one hour. Qt performs at most three automatic retries.
An explicit unknown `providerIdpId` fails with `provider_unavailable` rather than
selecting another provider. Endpoint updates reconcile by exact IDP identity.

Authenticated catalog and account reads include
`scope: {generation, providerIdpId, userId}`. Core reads capture private ServiceId
credentials and reject obsolete account/provider generations before publishing.
HTTP 401 permits one renewal and one replay of a safe read for the same owner.
HTTP 403 does not trigger renewal. Create, claim, account sync/unlink, and storage
reset do not replay mutations after an ambiguous response.

Authenticated VPC lookup requires a successful server-info response with a
nonempty `requestStatus.serverId`. Missing or rejected metadata is an error,
not a successful `GFN-PC` library. VPC cache entries are scoped by provider route,
account, credential, generation, and proxy, with coalescing within each scope.
Library and VPC requests use the same selected proxy. The core does not silently
retry a selected proxy directly. Authenticated HTTP clients do not follow redirects.
LCARS retains its configured shared endpoint; an Alliance provider ID does not
select `GFNPartnerJWT` or invent a GraphQL hostname.

`settings.set` for `region` also requires the current `providerIdpId`. The core
atomically updates `region`, `regionProviderIdpId`, and the `providerRegions` map.
The metadata fields cannot be written separately. Existing unscoped preferences
remain eligible only for NVIDIA. Region overrides must occur in the current
provider's server-info list. An unavailable or incompatible override falls back
once to that provider's base without deleting the saved preference. Provider and
region bases must be HTTPS NVIDIA-grid names without userinfo or nonstandard ports.
Arbitrary partner domains require a separate evidenced trust policy.

`session.create` requires `catalogAppId` as the parent LCARS identifier,
`variantId` as the selected positive GraphQL Int identifier encoded as a string,
and `appId` equal to that exact variant ID. `scope` must contain the current
`generation`, `userId`, and `providerIdpId`. The core resolves fresh catalog and
account metadata and applies the same decision as `catalog.launch.inspect`
immediately before allocation. Only `ready` admits a fresh allocation. A decision
from an earlier inspection is not a transferable permission. Missing, foreign,
or mismatched identifiers do not fall back to another variant or a title.
Existing validated seat claims remain separate and never write ownership.

`session.create` acquires a single typed CloudMatch admission guard before provider
discovery, authentication, or route locks. A concurrent create returns
`session_update_busy` immediately instead of waiting to allocate after the first
request fails or is cancelled. Pre-allocation failure releases the guard; after
an allocation exists, the existing receipt and exact-seat cleanup state continue
to prevent another allocation.

CloudMatch requests serialize with account changes. An active seat belongs to
its exact session ID, user ID, and provider IDP, independently of the account
generation. Returning from account A to B to A, clearing caches, or signing in
again as A does not revoke A's ownership. New owned-seat operations publish the
current generation and retain the core-captured control and media endpoints.
An active seat retains its original account for polling and exact-seat cleanup
after a switch or logout.
If that retained credential expires, `session_owner_authentication_required`
asks the user to return to the original account without restarting media.
New-account credentials never authorize requests to the retained seat. When the
selected account matches that owner, explicit cleanup renews expired ServiceId
credentials before issuing DELETE. A rejected DELETE is not automatically replayed.
Session results include `ownerScope` inside `session`; Qt distinguishes updates
for its existing native session from unrelated old-account responses. Ordinary
asynchronous results, discovered seats, and catalog results remain generation
fenced. A new allocation's receipt retains its original generation even when
the active seat is republished under a newer generation; an obsolete receipt
still triggers exact-seat compensation.

Qt accepts authoritative terminal status 7 or HTTP 404 for the exact displayed
session and its user/provider identity even when the result's generation is old.
This exception applies before ordinary response and `session.changed` scope
filters. It does not admit stale nonterminal updates, another account/provider,
another session ID, or a resumable or non-authoritative termination. Duplicate
terminal delivery cannot end a replacement seat.

`streamer.prepare` uses the caller's session ID to resolve the current core-owned
active seat. Caller-supplied connection endpoints are ignored. The response also
returns the owned `session` with its current `ownerScope`, which Qt retains for
later session events. Foreign ownership or an unknown seat fails with
`session_owner_mismatch`, a non-ready seat with
`session_not_ready`, and missing RTSPS endpoints with `session_endpoint_missing`.
The existing negotiated profile checks still run. Preparation does not expose
OAuth tokens to Qt or alter the native `/rtsp` websocket session-ID authentication.
These fields are additive to protocol 5; allocation receipts and exact-seat
compensation remain unchanged.

`session.create` reports an existing-session limit as `session_conflict`, including
CloudMatch status `11`, `SESSION_LIMIT` descriptions, and unified error
`4AF1201E`, including structured session-limit responses with HTTP 403. An
unrecognized HTTP 403 remains `authentication_required`; HTTP 401 is
`http_unauthorized`. Session creation is never automatically replayed. The
rejected request never becomes an active local session. The shell
should call `session.remote.list` once and offer to resume or end the existing
session instead of displaying the raw vendor error or retrying creation. When the
create response supplies usable `otherUserSessions` or `session` details, the core
normalizes them to the ordinary discovery descriptors and hands them to the next
list request without another network lookup. This handoff is account-scoped,
consumed once, and expires after 30 seconds; it is not a persistent discovery
fallback. The code/message error envelope and protocol version are unchanged.

`session.claim` first queries the trusted seat host returned by discovery, including
conflict-response handoffs whose create region may not own the existing session.
If that host fails for a reason other than authentication, it falls back to the regional
endpoint. The shell refreshes the chosen session's descriptor before retrying a failed
claim, without substituting another game or silently creating a new session.

`session.claim` discovers the session's actual control server, sends the minimal
`action: 2, data: "RESUME"` request for a ready, streaming, or paused seat, and returns a session
with `resumePending: true` and `phase: "resuming"`. It preserves the stable device
identity and existing launch mode; it does not renegotiate codec, resolution, FPS,
or bitrate. An initializing (`1`) or resuming (`6`) seat is polled without repeating
the mutation. Discovery includes paused (`4`/`5`) and resuming seats so they remain
available to resume and reconnect. Finished or unknown states reject the claim.

The PUT acknowledgement is not stream readiness. Call `session.poll` until a fresh
GET has a successful CloudMatch request status (`statusCode: 1`), session status
`2` or `3`, and nonempty native RTSPS endpoints. Only then does the core clear
`resumePending` and expose the ready/streaming phase. Transient status `6` continues
to report `resuming`; finished or unknown poll states clear `resumePending` and
report `failed`. A RESUME response of `SESSION_NOT_PAUSED` (`statusCode: 34`)
also proceeds to polling, including on an HTTP error response. Authentication
failures and other transport/API failures remain typed RPC errors.

Qt polls every 1.5 seconds with only one request outstanding, bounded by 60 polls
and a 90-second deadline checked between requests. Native connection recovery first
stops the old media transport, discovers the same session ID through
`session.remote.list`, then claims/polls it and calls `streamer.prepare` for fresh
connection context. It never creates a replacement cloud game or claims a different
session. During recovery, `session.remote.list` accepts the active `sessionId` so the
core can use its remembered regional service rather than the native server IP.
Failed recovery attempts back off up to eight seconds and stop after eight
attempts; only a presented first frame resets this budget. Ending the session cancels
recovery. A native stop stalled for 30 seconds reports an error without launching
another transport over the still-owned resources.

For the embedded Qt client, `session.create` and `streamer.prepare` accept an optional
`runtimeCapabilities` object copied from the in-process streamer's protocol-7 `hello` response.
The core filters its available `videoBackends` by the persisted `nativeVideoBackend` preference
and resolves codec `auto` to AV1, HEVC, then H.264 (subject to requested color mode) before
CloudMatch allocation. This resolution is session-local: the saved preference stays `auto`.
Each codec entry may include `colorQualities`, an authoritative array of supported settings
values such as `8bit_420` and `10bit_420`. An empty array means no supported color modes.
The embedded Linux runtime publishes this field for every codec: Vulkan values come from the
attached device's actual decode profiles, while non-Vulkan Linux paths report only 8-bit 4:2:0.
The core filters codecs by the requested color mode before both Auto and manual selection, so
an HEVC decoder supporting Main but not Main10 cannot allocate a 10-bit session, and the shared
Vulkan path cannot allocate 4:4:4. Missing or malformed advanced-color profile information on
Linux/Vulkan fails closed; other platforms retain their existing behavior when this optional
field is absent. The selected codec and color remain session-local and never downgrade the
user's requested color mode.
Manual unavailable codecs/backends return `streamer_codec_unavailable` or
`streamer_backend_unavailable`, respectively. `streamer.prepare` also validates the negotiated
codec on resume; it never changes the codec of an already allocated stream. Older callers without
this optional object retain the external-streamer probe path. The additive fields do not change
the JSON protocol version or native FFI ABI.

### Windows graphics preference

`settings.get`, `settings.set`, and `settings.reset` expose `windowsGpuDeviceId`.
The default empty string selects Automatic. Explicit values are opaque device
identities, limited to 1024 UTF-8 bytes with no NUL characters. Invalid persisted
values normalize to Automatic; invalid writes are rejected without changing the
saved preference.

Before creating the Qt graphics device, the shell may run:

```text
opennow-core --graphics-preferences
```

This mode emits one JSON line, then exits without initializing account, network,
catalog, telemetry, or streaming services:

```json
{"version":1,"windowsGpuDeviceId":""}
```

It uses the normal data-directory resolution, including `--data-dir`,
`OPENNOW_DATA_DIR`, and legacy-directory discovery. The read is limited to 1 MiB
and never saves migrations, creates directories, or renames corrupt settings.
Missing, corrupt, or oversized input selects Automatic. Qt bounds the subprocess
and its output and also uses Automatic if bootstrap fails.

Qt resolves the saved identity to a current-boot Windows adapter LUID. The same
LUID selects Qt's D3D11 adapter and the native runtime's capability probes; LUIDs
are not persisted. A missing saved GPU falls back for that launch without
erasing the preference. The settings selector appears only with at least two
detected hardware adapters, excluding software adapters. Saving a different GPU
affects the next application launch, not the active graphics device or session.

### Recording and replay capture

The Qt/native recorder and replay exporter preserve the negotiated source video and
Opus game audio in Matroska without decoding or re-encoding. Recording resolution,
frame rate and bitrate follow the stream; the retained legacy `recordingResolution`,
`recordingFps` and `recordingBitrateMbps` preferences do not configure native capture.

`settings.get` / `settings.set` expose `replayBufferEnabled` (default `false`),
`replayBufferSeconds` (default 30, clamped to 15–120), `replayBufferMemoryMiB`
(default 256, clamped to 64–512), and `shortcutSaveClip` (default `Ctrl+F12`).
`streamer.prepare` includes these settings in the session context and maps
`shortcutSaveClip` to `shortcuts.saveClip`. Enabling replay and changing its limits
apply to the next native session. Disabling it sends `replay-stop` immediately,
clears buffered media and cancels an in-progress clip export.

These commands use the embedded streamer's protocol-7 JSON payload without
changing the C ABI:

- The `start` response includes `replayEnabled` for the actual session.
- `clip-save` accepts `id` and an absolute `.mkv` `outputPath` allocated by
  `media.recording.target`. It returns `clip-saving` promptly or a typed error if
  replay is disabled, not yet decodable, or another export is still running.
- A `clip-state` event reports `state` (`saved` or `failed`), `requestId`, `path`
  and `message`. The shell correlates `requestId` with the outstanding export and
  ignores completion from a previous session. Cancelled exports do not publish a
  completed file or a stale success event.
- `replay-stop` returns `replay-stopped`. A new session is required to enable
  buffering again.

Replay retains encoded packets with bounded memory, frame count and duration.
Clips start on a retained video keyframe, so their length may be shorter than the
requested duration. Export transfers the retained buffer to a single worker;
buffering rebuilds from a subsequent keyframe within the remaining memory budget.
There is no assumed periodic-keyframe guarantee: when a whole GOP exceeds the
configured limit, replay returns `replay-not-ready` until a new source keyframe.
Capture does not issue periodic keyframe requests that would change stream traffic.
Discontinuity or producer contention clears replay history rather than blocking
playback or publishing a clip with missing references. There is no additional
video encoder or GPU readback. Packet bookkeeping, container muxing and disk I/O
still consume CPU and bandwidth; zero CPU usage or zero performance impact is not
a supported guarantee.

### HDR session contract

`settings.enableHdr` is a persisted boolean with default `false`; HDR requires explicit user
opt-in. Qt adds `nativeHdrSupported: boolean` to `params.runtimeCapabilities` on each
`session.create` and `streamer.prepare` request. It must describe the actual stream window's
current HDR output, including the active monitor, compositor/OS HDR state, and presentation
surface support, not merely a GPU or decoder capability. Missing, false, or malformed values
deny HDR. This value is transient: `settings.set` rejects `nativeHdrSupported`, the settings
loader discards legacy copies, and the core never saves runtime capability results.

With HDR enabled, the core requires an available hardware HEVC or AV1 decoder whose
`colorQualities` explicitly includes the requested ten-bit profile. The optional per-codec
`hdrSupported: true` permits `10bit_420` when `colorQualities` is absent, but does not establish
4:4:4 support. The optional per-codec `hdrColorQualities` array lists verified HDR profiles.
When present, it must contain the requested profile; empty or malformed values deny HDR.
HDR 4:4:4 requires an explicit `10bit_444` entry in both color-quality arrays.
Missing `hdrColorQualities` retains the existing HDR 4:2:0 checks only. Windows reports
explicit SDR color-quality lists and HDR support from actual decoded profile fixtures and GPU conversion;
it no longer infers advanced formats from an eight-bit codec probe. macOS HEVC HDR and
ten-bit 4:4:4 similarly require hardware-required fixture decode and Metal conversion.
Linux uses exact attached-device profiles for Vulkan Video. An explicit false or
malformed `hdrSupported` denies HDR even if 10-bit profiles exist. A true value never
overrides an explicit empty or incompatible `colorQualities` array. Neither setting
changes SDR capability filtering. Auto prefers HEVC then AV1. HDR constrains the
session-local color quality to ten bits while preserving the selected chroma format and saved
SDR color preference. HEVC supports `10bit_420` and `10bit_444`; AV1 remains limited to `10bit_420`.
Explicit H.264, software decoding, missing output support, and unavailable 10-bit profiles
fail before allocating a seat. Without HDR,
the saved color preference is used, subject to exact hardware profiles and the GFN codec
restrictions below. Callers without embedded runtime
capabilities cannot request HDR through the external-streamer probe path.

CloudMatch receives `sessionRequestData.sdrHdrMode=1`, monitor `sdrHdrMode=1`, and
`requestedStreamingFeatures.trueHdr=true` only for this validated HDR request. CloudMatch
uses bit-depth/chroma enums `1/0` for 10-bit 4:2:0 and `1/1` for 10-bit 4:4:4.
For HDR, monitor `displayData` carries the output's validated HDR static metadata
when the current output reports it. `desiredContentMaxLuminance` and
`desiredContentMinLuminance` come from the Wayland color-management target luminance
range in cd/m²; this mirrors the official client's feature-gated mirroring of its
system display properties into the same fields. `desiredContentMaxFrameAverageLuminance`
is omitted while a validated output snapshot is in use, because the output description
exposes no comparable sustained full-frame value and no permitted capture establishes
that mapping.

Without a validated output snapshot, HDR requests keep the fixed requested-content
defaults of maximum luminance 1000 nits, minimum luminance 0, and maximum frame-average
luminance 400 nits, matching the Mac native session payload. Those defaults are requested
content characteristics rather than measurements of the physical display and are not
presented as calibration. SDR luminance values and all display primaries remain zero
protocol defaults; the official typed `displayData` schema contains no primaries or white
point fields, and no permitted capture establishes a scale for measured chromaticities.

The validated snapshot travels from Qt in `runtimeCapabilities.nativeHdrDisplay` as
`minimumNits` and `maximumNits` in cd/m², omitting either value the output does not
report. The core validates the pair again and drops it when the bounds are not finite,
negative, above 10000 cd/m², or not strictly increasing. The snapshot is transient:
`settings.set` rejects it, the settings loader discards persisted copies, and the core
never saves runtime capability results.

Stale snapshots are discarded rather than reused. Qt injects the snapshot only while the
current output reports a validated range and removes any caller-supplied or previously
injected value when it does not; the core strips any earlier snapshot from the resolved
settings before carrying the current capability, so a display change that invalidates the
output falls back to the requested-content defaults instead of replaying the previous
display's luminance. Claim and resume requests do not carry monitor settings at all, so
they cannot replay one either.

The normalized server response carries `negotiatedStreamProfile.enableHdr: boolean`. It
comes from the returned session's `sdrHdrMode`, then the returned monitor's mode, then the
returned session-request mode; missing or unsupported modes mean SDR. An explicit server
SDR response wins over saved HDR intent. `trueHdr` is not used to infer accepted dynamic
range. Resume preserves that returned mode rather than renegotiating from current settings.
The claim request intentionally omits monitor settings and requested streaming features, so
its only copied dynamic-range field is the accepted session `sdrHdrMode`. The initial
compatibility RESUME carries the full request and updates its session mode, monitor mode,
requested-content luminance, and `trueHdr` consistently when the server has returned a mode.
Attachment revalidates the session's HDR codec/color profile and current window output, so
moving to an SDR display cannot silently resume an HDR stream as SDR.

Color negotiation overlays each returned `finalizedStreamingFeatures` field on the server's
returned `sessionRequestData.requestedStreamingFeatures`. An empty or partial finalized object
must not erase the echoed codec, bit depth, or chroma. Explicit finalized values, including
invalid values, take precedence; missing values never come from current saved preferences.
CloudMatch chroma enums are `0` for 4:2:0 and `1` for 4:4:4; NVST chroma-format IDs `2` and
`3` are not accepted as CloudMatch 4:4:4 values.

When present, `session.negotiatedStreamProfile.codec` takes precedence over the numeric
feature-map codec. H.264/AVC and H.265/HEVC names normalize to `H264` and `H265`;
`AV1` remains unchanged. An explicit null or unsupported codec stays unknown rather
than falling back to a requested codec. When the server omits every codec field, a new
allocation retains the exact codec sent in that allocation's request. Polling, direct-server
responses, claims, and ad updates preserve that evidence only for the same session ID.
`codecSource` distinguishes `request`, `server`, and `unreported`; a reported codec supersedes
the request and remains authoritative in later partial responses. Unknown discovered sessions
never borrow a codec from current saved preferences. Preparation failures log bounded
codec/color evidence and a redacted reason, never the full session or credentials.

Embedded session preflight rejects H.264 with advanced color and AV1 with 4:4:4 before
allocation, even if a decoder capability lists those formats. These combinations are not
requested by the supported GFN wire policy. Auto selects HEVC for 4:4:4 rather than silently
reducing chroma; an explicit incompatible codec remains an error.

The native context preserves the resolved profile and codec provenance, and `MediaStreamConfig.hdr` follows its
`enableHdr` alone (missing means false). Invalid accepted HDR profiles are rejected before
stream startup. NVST ANNOUNCE sends `x-nv-video[0].dynamicRangeMode=1` for HDR and `0` for
SDR, with literal bit depth `10` or `8`. Its `chromaFormat` uses chroma_format_idc (`1` for
4:2:0, `3` for 4:4:4), unlike CloudMatch's enum. The captured Mac native ANNOUNCE confirms
10-bit 4:2:0 as `bitDepth:10` / `chromaFormat:1`; its seat control notification `0x010e`
reports a separate runtime mode and must not be confused with CloudMatch's `trueHdr` field.

After an update check, `updater.highlights.get` returns the latest published
release notes for the selected channel, even when that release is equal to or
older than the installed app. Its `version` and `title` identify that published
release, independently of `updater.state.get.availableVersion`. Historical notes
do not emit `updater.highlights.show` or enable downloading a downgrade. Missing
release bodies and empty channels return an explanatory note instead of the
pre-check placeholder.

Updater preferences are independent. `autoCheckForUpdates` defaults to `true`;
`autoDownloadUpdates` defaults to `false` and requires an explicit opt-in. Neither
preference authorizes installation or application shutdown. New profiles use the
nightly update channel when the embedded application version has a `nightly`
prerelease identifier; existing persisted channel choices are preserved.

`updater.install` requires the boolean parameter `confirmed: true`. The core
serializes update preparation against session creation, claiming, polling, and
streamer startup. It rejects installation while a CloudMatch session exists or
the streamer is starting, negotiating, streaming, or recovering. The shell must
also check its local native-runtime state before requesting installation.

`updater.state.get` is authoritative after an error or request timeout. The core
emits `updater.changed` after update operations even if the original request was
cancelled, because cancellation does not undo an installation already prepared
by the native helper. Clients must not synthesize `canDownload`, `canInstall`, or
`canCheck` from an error message. A failed check or a later release check does not
discard an already verified download.

Inside Flatpak, the updater reports `status: "unsupported"` and
`updateSource: "flatpak"`. `canCheck`, `canDownload`, `canInstall`, and
`exitRequired` are false. `updater.check` returns that state without contacting
GitHub. Download and install requests fail with the package-manager instruction
in `message`. The core does not recover native update transactions inside the
sandbox, and the native update helper rejects execution there. Flatpak owns
package replacement and updates.

The additive updater state fields `exitRequired` and `installVersion` describe
the prepared installation, separately from `availableVersion` and
`downloadedVersion`. The shell may quit for an update only after a confirmed
install request in that same shell process, with authoritative
`status: "awaiting-exit"` and `exitRequired: true`, and while the local session
remains inactive. `preparing`, `applying`, and `restarting` are not permission to
quit. Terminal outcomes are `succeeded`, `rolled-back`, and `failed`;
`managed-pending` and `reboot-required` require native package-manager or operating
system completion. Spawning an installer or the replacement application never
counts as success.

On a later launch, the core reconciles these managed outcomes against the native
package registration and its running version. A known live installer keeps
`managed-pending` active. A completed installer resolves to `succeeded` or
`failed`, and the core clears the persisted active transaction. An MSI reboot
warning remains until Windows' per-boot sequence number changes. Sessions
remain available, but another update must wait for the required reboot so it
cannot overlap pending Windows file replacements. Reopening the app before
reboot does not count as successful installation.

If an installer process cannot be identified, the core keeps the transaction
pending until a recorded boot change proves that the previous installer has
stopped. Legacy transactions without a boot marker establish a baseline and may
require one additional restart rather than guessing that installation finished.

Windows portable replacement requires a volume with persistent ACL support;
FAT/exFAT installations are refused before shutdown because private staging
cannot be enforced there. Preserved portable profile data retains its ownership
and effective access permissions. Preparation also fails before shutdown if the
replacement overlaps user data or exceeds the bounded copy limits. MSI packages
use Windows Installer registration and preserve the registered installation root;
they are not treated as portable directories.

`updater.startup.ack` accepts no parameters and returns `{ "acknowledged": true }`
only for a valid helper-launched update attempt. A normal launch returns
`acknowledged: false`. The shell calls it after its UI and core connection are
ready. The core validates the prepared version, per-attempt nonce, and running
Qt application identity before acknowledging startup to the waiting helper.
These changes are additive within protocol version 1; they do not change the
native streamer ABI.

`updater.highlights.show` announces unread release notes, not a navigation
command. The shell keeps the current stream and its video item alive, defers the
announcement during sessions, and acknowledges the notes only when the user
opens them.

Setting `themePack` applies its default appearance (`light` for Bone/Cobalt, `dark`
for the other built-in packs) and clears `themeAccentOverride` in the same save.
Setting `appAccentColor` enables `themeAccentOverride` in the same save. The
`settings.set` response and `settings.changed` event include these coupled values
in `changes`; clients can still override appearance or restore the pack accent
by setting `appTheme` or `themeAccentOverride` independently.

Each successful settings write publishes `settings.changed` before its own
response. A client that starts its next per-key write from that response has
already consumed the previous event. Other response/event pairs, including
`settings.reset`, retain their existing response-first order.

Settings writes use a temporary file plus recoverable backup and normalize
compatibility-sensitive values. `audioOutputDevice` is an opaque native output identifier
(at most 1024 UTF-8 bytes, without NUL characters); an empty string follows the
system default. It is persisted and passed unchanged in the prepared stream
context for the next session. A missing fixed output fails playback startup
instead of falling back to another device. Setting `launchInConsoleMode=false` atomically
sets `switchToConsoleOnPad=false` too, so a manual desktop choice survives restart.
That response and `settings.changed` event additionally contain
`"changes":{"switchToConsoleOnPad":false}`; consumers apply these coupled values
before the primary key. Automatic console switching defaults off. Existing
pre-opt-in settings receive a one-time reset of automatic switching only;
explicit subsequent opt-ins and the independent startup preference are preserved.
Existing microphone device selections are cleared only when explicitly selecting Open
microphone; that write likewise reports `"changes":{"microphoneDeviceId":""}` so the
shell follows the system-default capture selection.

`onboardingCompleted` is a persisted boolean, defaulting to `false` for a new
profile or an unreadable or malformed settings file. A valid existing settings
JSON object without the key migrates to `true` and is saved during loading, so
upgrades do not trigger first-run setup. Explicit `false` and `true` values survive
reloads and unrelated writes, including startup preferences and window geometry.
Non-boolean values normalize to `false` under the standard settings type rules.
`settings.reset` preserves the current completion value while resetting preferences;
it does not replay onboarding for an existing user. Failed saves leave the previous
in-memory value and persisted settings unchanged.

`gameCollections` defaults to `[]`. `settings.set` replaces the complete ordered
array with at most 100 objects of the form
`{"id":"collection-id","name":"Collection name","gameIds":["game-id"]}`.
Collection IDs are caller-owned stable strings, unique across the array, and
must not be regenerated when renaming a collection. IDs are preserved verbatim,
must contain a non-whitespace character, and are limited to 128 Unicode characters.
Names are trimmed and must contain 1–80 Unicode characters after trimming; names
need not be unique. Each `gameIds` array contains at most 10,000 unique strings
with the same nonempty/128-character ID constraints. Game IDs may appear in
multiple collections, and empty collections are allowed. Array order is preserved.
Malformed types, missing or extra fields, duplicate IDs, or exceeded limits return
an error without modifying the in-memory settings or persisted file. Save failures
also restore the previous in-memory settings, including when resetting settings.
Reloads trim valid persisted names and preserve collections across unrelated writes;
invalid persisted collections cause an `InvalidData` load error before any write,
rather than silently dropping collections or truncating identifiers. A successful
`settings.reset` clears collections along with other preferences.

Provider discovery falls back to NVIDIA's
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

### Catalog page and metadata capabilities

Protocol 4 requires `catalog.libraryPages.v1`, `catalog.metadata.v1`,
`account.syncObservation.v1`, and `catalog.languages.v1` in the core handshake.
The Qt client and both relocated package probes check these capabilities.
The JSON envelope and native streaming ABI do not change.

`catalog.library.list` accepts `limit` from 1 to 100, an opaque `cursor` up to
4,096 bytes, and a `traversalId` up to 256 bytes. Continuations must carry the
`catalogRevision` and opaque `catalogContext` returned by the first page. The context binds the cursor to the provider, account, generation, endpoint, resolved VPC, proxy route and locale. Results contain `games`, `count`,
nullable `totalCount`, `hasNextPage`, `nextCursor`, `fetchedAt`, `freshness`,
`traversalId`, `catalogRevision`, `catalogContext`, and the authenticated `scope`. One result is
at most 768 KiB. Oversized pages are retried at the same cursor with a smaller
count; a record that cannot fit is an error. Invalid identities or missing
pagination state are errors, not an empty complete library.

CatalogState stages these pages and detects cursor cycles. A complete library
means the traversal reached a validated end; it does not imply a transactional
snapshot of concurrently changing vendor data. Refresh preserves the previous
complete snapshot until the new traversal ends. The foreground slice is 30
seconds, with an explicit Continue action. The hard aggregate bounds are 20,000
games and 32 MiB of conservative serialized-size accounting. Partial/error
results retain usable data, the failed cursor where safe, and the error text.
`catalogLastCompleteAt` changes only when the aggregate traversal completes.

`catalog.game.get` accepts exactly one `appId` or `variantId`. The former is a
parent LCARS string; the latter must fit a positive GraphQL `Int`. The request
selects library-aware metadata and verifies the returned identity. It always
revalidates rather than authorizing from cached browse cards. The result has
`game`, `catalogRevision`, `scope`, `fetchedAt`, and `freshness`.

Game variants retain nullable `librarySelected`, `libraryStatus`, `playStatus`,
`installed`, `subscription`, `gfnStatus`, `stateDetails`, `paymentModels`,
`subscriptions`, and `supportedLanguages`. Patch details distinguish automatic
patches, manual patches, maintenance, and unknown types. Games retain app-level
`favorited`, `catalogSkuStrings`, `campaignIds`, and fallback `paymentModels`.
App availability, favorites, payment models, and GFN membership do not establish
ownership of another variant. Visible detail metadata refreshes every 30 seconds;
patch duration history is an estimate, not a promised completion time.

`catalog.definitions.get` returns independently fetched `stores`, `genres`, and
`subscriptions` sections. Each section has `items`, `source`, `status`,
`freshness`, `fetchedAt`, `expiresAt`, and nullable `error`. Static definitions
expire after 24 hours. Store definitions retain the feature union and per-variant
linking metadata. They drive account rows and allowed actions. Unknown stores
are display-only; unavailable definitions use labelled, action-disabled fallback
rows. A current server `supported:false` overrides older capability data.
Store subscription IDs are separate from MES/GFN membership.

`catalog.store.presentation` retains parsed filter expressions and `sortOrders`.
`catalog.store.list` can receive a `filterId` or `sortId`; the core resolves these
to the returned server expressions rather than sending an ID as a filter object.
`revalidate:true` invalidates only the addressed page key; use it for an explicit
search submission rather than `refresh:true`, which resets a browse chain.
Local Store facets remain local. `catalog.store.local` returns
`localHasNextPage`, `cacheComplete`, `upstreamCoverage`, and `facetsSource`
alongside its existing demand-driven cursor. Local exhaustion does not prove
upstream coverage. Browse disk pages expire after 15 minutes, and explicit local
refresh first revalidates the upstream first page before rebuilding the index.
Confirmed account changes persist an increasing catalog revision, so old pages cannot join
a new traversal or reappear after a core restart. Failure to persist the revision is an explicit cache-invalidation error, not completion. Cache identities include provider/account, generation,
endpoint, resolved VPC, proxy route, locale and schema revision.

### Store synchronization observation

`account.connections.sync` sends one ALS POST and accepts only HTTP 202. It
returns `operationId`, `provider`, and a local `phase`. Repeated starts for the
same active provider return the existing operation instead of repeating POST.
There are at most eight account-scoped operations. Mutating requests are never
automatically replayed after a 401, timeout, cancellation, or ambiguous send.

`account.connections.sync.status` accepts `operationId`. Each poll reads fresh
`userAccount` at most once. Calls are spaced at two seconds initially and five
seconds after 20 seconds, with a 120-second observation deadline. The operation
records the baseline date and state. Only a changed valid RFC3339 `syncDate`
with `SYNC_SUCCESS` enters `refreshing_library`; unchanged old success, an
unchanged old failure, and changed counts alone do not prove completion. New
failure, disconnection, and expiry enter `failed`. Missing completion evidence
ends as `timed_out`, meaning completion is unconfirmed.

After `refreshing_library`, Qt starts the final library traversal. Only a
complete traversal displays completion and acknowledges it with
`sync.status({operationId, libraryRefreshed:true})`. Partial refresh retains a
separate incomplete notice. `account.connections.sync.cancel` stops observation,
not accepted remote work. Account/provider generation changes clear pending
work; late results cannot complete another account's operation. Polls do not
sleep across their observation lifetime or hold authentication or scheduler
locks across HTTP. Link callbacks and unlink acknowledgements require a fresh
account read before claiming the connection changed.

### Overall supported game languages

`catalog.languages.get({refresh:false})` works without sign-in. Its anonymous
HTTP document is exactly `{ overallGfnSupportedLanguages { language } }`.
There are no variables and no Authorization header. It returns exact wire IDs
in `languages`, `status` (`success`, `stale`, or `error`), `source`, cache timing,
nullable `error`, and `scopeGeneration`. The bounded cache is shared with the
existing disk-cache owner, keyed separately by public endpoint, provider context,
and proxy route. Its TTL is 14 days. At most 512 safe IDs of 64 bytes are accepted;
duplicate IDs are removed without normalization. Invalid or empty metadata does
not replace a previous good list. Failure with a previous list is explicitly
stale; failure without one has an empty list and error status. This reference
data does not select the interface locale, keyboard map, or a game's preferences.

### Independent language preferences and settings choices

`appLanguage` defaults to `system` and selects a bundled Qt interface locale.
`gameLanguage` defaults to `en_US`; `keyboardLayout` defaults to `en-US`.
Changing one does not change either of the others. Game language identifiers
retain exact case, separators, script, and numeric-region subtags. They have an
ASCII alphabetic first subtag of 2–8 bytes, subsequent alphanumeric subtags of
1–8 bytes separated by `_` or `-`, and a total limit of 64 bytes. `auto` and
`system` are rejected, case-insensitively. Safe future IDs need not appear in
current metadata to remain saved.

`settings.get` returns `keyboardLayouts` beside `settings`, never inside the
persisted values. Each descriptor has `value`, `label`, and `aliases`. The table
in `language.rs` is the authority for the supported Windows-rig keyboard subset
on all Qt platforms. Historical `ja-JP` and `Japanese106` request `ja-106`;
historical `es-ES` requests `es-ES_tradnl`. Valid saved aliases retain their
original spelling on disk. This does not select proprietary Mac keyboard IDs.
New unrecognized keyboard values and malformed game IDs return `invalid_setting`.
Restored strings remain visible, but a shared request resolver replaces corrupt
game or keyboard values with the existing respective defaults before create,
immediate resume, or claim requests. The interface locale never enters these
query parameters.

`settings.choices.get({runtimeCapabilities})` returns `colorQualities` for the current
persisted settings and embedded streamer capability snapshot. Each of the four
descriptors contains `value`, `disabled`, and nullable `reason`. The operation
calls the same profile validator as final launch, including backend, codec,
HDR, and chroma restrictions. Missing or incompatible capabilities do not imply
support. This read does not persist or coerce any setting. Protocol 5 and native
streamer protocol 7 are unchanged.

CoreClient injects the current native window's `nativeHdrSupported` into
`runtimeCapabilities`, overriding any caller-supplied flag, just as it does for
`session.create` and `streamer.prepare`. It injects `nativeHdrDisplay` alongside it only
when the output reported a validated luminance range, so an unavailable snapshot is
omitted rather than sent as zero. SettingsState observes
`HdrOutput.supported` only to cancel and refetch choices when the display changes;
it does not author the wire capability or expose platform handles.

Qt `SettingsState` owns shared desktop/console choices and the single lazy
language request. Settings entry loads idle or expired metadata. Explicit retry
sets `refresh:true`; there is no retry loop. Requests have a 15-second deadline.
Readiness, account/provider generation, or proxy changes clear request ownership
before cancellation and discard old metadata, but preserve saved preferences.
Only a response for the current request and generation is accepted. `cacheHit`,
stale/error status, saved IDs absent from metadata, and local fallback choices
remain distinguishable. Language and profile edits serialize per key and become
selected only after persistence succeeds; older failures cannot roll back a
newer successful edit. These local settings operations do not restart media.
