# Co-op sessions and Friends: cross-component contract

This document is the single contract shared by every co-op component. Each component
implements its side exactly as written here. If a change is needed, update this
document in the same pull request as every producer and consumer it affects.

Components:

| Component | Location | Owns |
|---|---|---|
| Social backend | repo `opencloudgaming/opennow-social` (Railway) | Discord sign-in, OpenNOW accounts, friends, presence fan-out, lobbies, invites, join requests, signaling relay, TURN credential minting, `/join/<code>` landing page |
| Discord bot | repo `opencloudgaming/opennow-discord-bot` | Optional invite DMs, only to members of the opencloudgaming Discord server |
| Relay | repo `opencloudgaming/opennow-relay` (self-operated, `nl` and `ca` sites under zortos.me) | STUN and TURN (coturn, TURN REST shared-secret credentials) |
| Rust core | `native/opennow-core` | Social account token, backend REST and WebSocket client, presence publishing, core-protocol methods/events below |
| Native streamer | `native/opennow-streamer` | Host fan-out of the GFN media to guests, guest receive/decode/present, guest input injection into GFN controller slots, P2P transport |
| Qt shell | `opennow-qt` | Variation C2 UI (Paper file "OpenNOW co-op sessions", page "C2 · Play co-op + Discord"), `FriendsState` and `CoopState`, bridging core events to streamer commands |

User-facing copy never says the relays are self-hosted. Relays are named by region only:
"Netherlands" and "Canada".

## 1. Product model

- One GFN account streams to one client. The host runs the only GFN stream. The host's
  streamer forwards the already-encoded media peer-to-peer to at most 3 guests.
- GFN has controller slots 0..3. The host owns slot 0. Guests own slots 1..3 and are
  gamepad only. Keyboard and mouse stay with the host. Per-slot rumble goes to the slot owner.
  Lobby P1..P4 is GFN controller id 0..3, and the game's player number must match it. See
  [Slot to player number](#slot-to-player-number).
- Connection order per guest: direct ICE using our STUN servers, then TURN relay.
  A relayed guest sees a persistent "You're using relay" banner; the host sees "Relay" on that guest.
- Guests see three latency figures: host to GFN, guest to host, and their sum.
- Each guest costs the host about one stream bitrate of upload. The host sees upload used versus
  measured capacity, and a warning when it cannot sustain every guest.
- Friends is OpenNOW's own friends list. Identity is the user's Discord account (OAuth
  `identify` scope only). Users add each other by typing a Discord username. Discord friends are never imported.
- Invites are delivered inside OpenNOW. The bot additionally DMs the invite when the recipient
  enabled bot DMs and is a member of the opencloudgaming Discord server; otherwise no DM is attempted.
- Joining by code works without a linked Discord account (anonymous guest token).

## 2. Identifiers and shared types

All JSON uses camelCase. Timestamps are RFC 3339 UTC strings. IDs are opaque strings.

```ts
type UserId = string;            // backend user id, e.g. "usr_01J..."
type User = {
  id: UserId;
  discordId: string;
  username: string;              // Discord unique username, lowercase, no discriminator
  displayName: string;           // Discord global_name, falls back to username
  avatarUrl: string | null;      // https://cdn.discordapp.com/... 128px
};
type Game = { appId: string; title: string; artUrl: string | null };

type ActivityState =
  | "online"            // idle in app, no richer state
  | "idle"              // no input for 10 minutes
  | "browsing_library"
  | "browsing_store"
  | "in_settings"
  | "in_queue"          // waiting for a GFN seat
  | "playing"           // in a GFN stream, not hosting co-op
  | "in_lobby";         // hosting or a member of a co-op lobby

type Presence = {
  status: "online" | "offline";
  activity: ActivityState | null;      // null when offline or when the friend hides activity
  game: Game | null;                   // in_queue, playing, in_lobby
  since: string | null;                // when the activity/game started
  queuePosition: number | null;        // in_queue
  lobby: { id: string; size: number; max: number; joinable: boolean } | null; // in_lobby
  lastSeenAt: string | null;           // offline only
};

type SocialSettings = {
  presenceVisibility: "friends" | "nobody";   // who sees you online at all
  shareActivity: boolean;                     // false: friends see only "Online"
  joinRequests: "friends" | "nobody";         // who may ask to join your lobby
  botDmInvites: boolean;                      // default true
};

type Lobby = {
  id: string;
  hostId: UserId;
  code: string;            // "K7Q-4MX": 6 chars from 23456789ABCDEFGHJKLMNPQRSTUVWXYZ, dash after 3
  joinUrl: string;         // https://<backend>/join/K7Q4MX
  codeExpiresAt: string;   // 30 minutes after (re)generation
  game: Game;
  maxGuests: 1 | 2 | 3;
  members: LobbyMember[];  // includes the host at slot 0
  state: "open" | "streaming" | "ended";
};
type LobbyMember = {
  userId: UserId;          // anonymous guests: "anon_..."
  displayName: string;
  avatarUrl: string | null;
  slot: 0 | 1 | 2 | 3;
  anonymous: boolean;
  joinedAt: string;
};
type Invite = {
  id: string; lobbyId: string; from: User; to: User; game: Game;
  state: "pending" | "accepted" | "declined" | "expired" | "cancelled";
  createdAt: string; expiresAt: string;   // 10 minutes
  dm: "sent" | "skipped_not_member" | "skipped_disabled" | "failed" | "pending";
};
type JoinRequest = {
  id: string; lobbyId: string; from: User; game: Game;
  state: "pending" | "approved" | "declined" | "expired" | "cancelled";
  createdAt: string; expiresAt: string;   // 2 minutes
};
type IceServer = { urls: string[]; username?: string; credential?: string };
type RelaySite = { id: "nl" | "ca" | string; name: string; stunUrl: string; turnUrls: string[] };
```

## 3. Social backend HTTP API (`/v1`)

Base URL comes from the core setting `socialBackendUrl` (default the production Railway domain).
Authentication: `Authorization: Bearer <token>`. Tokens are 32 random bytes, base64url, stored hashed (SHA-256).
Errors: HTTP 4xx/5xx with `{"error":{"code":string,"message":string}}`. Codes are stable and listed per route.
Every list is bounded: friends 500, requests 200, invites 100.

Backend clarifications (additive):

- Shared codes on any route: 400 `invalid_request`, 401 `unauthorized`, 403 `guest_not_allowed`, 403 `not_host`,
  404 `not_found`, 413 `payload_too_large`, 429 `rate_limited`, 500 `internal`. Missing resources use
  `lobby_not_found`, `invite_not_found`, `join_request_not_found`, `member_not_found`, `request_not_found` and
  `login_not_found`.
- Further route codes: 409 `already_in_lobby` (also on joins and join requests), 403 `blocked` (joining a host you
  are blocked with), 409 `too_many_guests`, 409 `invite_not_pending`, 409 `friend_limit` / `request_limit` /
  `invite_limit`, 409 `join_request_not_pending`, 409 `join_request_not_approved`, 410 `join_request_expired`,
  400 `self` (kicking yourself).
- Approving a join request restarts its 2-minute window; each approval admits one join. Joining a lobby you are
  already in is idempotent and returns your slot.
- A lobby whose code expires with only the host present ends with `lobby.ended` reason `expired`; a lobby with
  guests keeps running and its stale code returns 410 `code_expired`.
- Presence lobby summaries count the host: `size` includes the host and `max` is `maxGuests + 1`.
- `Invite.dm` is `skipped_disabled` when no bot is configured.

### Sign-in (polling, no local listener)

| Method and path | Body | Response |
|---|---|---|
| POST /v1/auth/discord/start | `{}` | `{loginId, pollSecret, loginUrl, expiresAt, intervalMs}` (10 min, 2000 ms) |
| GET /v1/auth/discord/callback | Discord redirect | HTML "You can return to OpenNOW" page |
| POST /v1/auth/discord/poll | `{loginId, pollSecret}` | `{status:"pending"}` / `{status:"complete", token, user}` / `{status:"denied"|"expired"}` |
| POST /v1/auth/guest | `{displayName}` | `{token, guest:{id, displayName}}` anonymous; may only join by code; 24 h |
| POST /v1/auth/logout | | 204 |
| GET /v1/me | | `{user, settings}` |
| PATCH /v1/me/settings | partial SocialSettings | `{settings}` |
| DELETE /v1/me | | 204; deletes the account, friendships, requests, blocks. `social.unlink` does not use it; it signs out with POST /v1/auth/logout and keeps friends. |

`loginUrl` is Discord's authorize URL with `scope=identify`, `state` bound to the login, and PKCE. The client secret never leaves the backend.

### Friends

| Method and path | Body | Response / errors |
|---|---|---|
| GET /v1/friends | | `{friends:[{user, presence, since}], incoming:[{id, user, createdAt}], outgoing:[{id, user, createdAt}], blocked:[User]}` |
| GET /v1/users/lookup?username= | | `{user, relation:"none"|"friend"|"incoming"|"outgoing"|"self"|"blocked"}`; 404 `user_not_found` |
| POST /v1/friends/requests | `{username}` | 201 `{request}`; 404 `user_not_found`, 409 `already_friends`, 409 `request_pending`, 400 `self`, 403 `blocked`. A request to someone who already requested you accepts it (200 `{friend}`). |
| POST /v1/friends/requests/:id/accept | | `{friend}` |
| POST /v1/friends/requests/:id/decline | | 204 |
| DELETE /v1/friends/requests/:id | | 204 (cancel outgoing) |
| DELETE /v1/friends/:userId | | 204 |
| POST /v1/blocks | `{userId}` | 204; removes friendship and pending requests both ways |
| DELETE /v1/blocks/:userId | | 204 |

Username lookup normalizes to lowercase and strips a leading `@`. Rate limit: 30 lookups and 20 requests per minute per user.

### Lobbies, invites and join requests

| Method and path | Body | Response / errors |
|---|---|---|
| POST /v1/lobbies | `{game, maxGuests}` | 201 `{lobby}`; 409 `already_in_lobby` |
| GET /v1/lobbies/:id | | `{lobby}` |
| PATCH /v1/lobbies/:id | `{state?:"streaming", maxGuests?}` | `{lobby}` host only |
| POST /v1/lobbies/:id/code | | `{lobby}` regenerates the code; old code stops working |
| DELETE /v1/lobbies/:id | | 204; ends co-op, every member receives `lobby.ended {reason:"host_ended"}` |
| POST /v1/lobbies/:id/invites | `{userId}` | 201 `{invite}`; 403 `not_friends`, 409 `lobby_full`, 409 `invite_pending`, 409 `already_member` |
| DELETE /v1/invites/:id | | 204 cancel |
| POST /v1/invites/:id/accept | | `{lobby, slot, iceServers}`; 410 `invite_expired`, 409 `lobby_full`, 410 `lobby_ended` |
| POST /v1/invites/:id/decline | | 204 |
| GET /v1/invites | | `{incoming:[Invite], outgoing:[Invite]}` pending only |
| POST /v1/lobbies/join | `{code}` (dash optional, case-insensitive) | `{lobby, slot, iceServers}`; 404 `invalid_code`, 410 `code_expired`, 409 `lobby_full`, 410 `lobby_ended`; anonymous tokens allowed |
| POST /v1/lobbies/:id/join-requests | | 201 `{joinRequest}`; 403 `join_requests_disabled`, 409 `lobby_full` |
| POST /v1/join-requests/:id/approve | | `{joinRequest}` requester receives `joinRequest.updated` then joins via `POST /v1/join-requests/:id/join` |
| POST /v1/join-requests/:id/join | | `{lobby, slot, iceServers}` requester only, after approval |
| POST /v1/join-requests/:id/decline | | 204 |
| POST /v1/lobbies/:id/members/:userId/kick | | 204; member receives `lobby.kicked` |
| POST /v1/lobbies/:id/members/:userId/slot | `{slot}` | `{lobby}` swaps when the slot is taken |
| POST /v1/lobbies/:id/leave | | 204 |
| POST /v1/lobbies/:id/reports | `{event, path, relayId, codec, decoder, connectMs, firstFrameMs, reason}` | `{ok:true}`; a member's (host or guest, anonymous allowed) connection report: `event` `first_frame`\|`ended`, `path` `direct`\|`relay`\|null, `decoder` `hardware`\|`software`\|null, ids ≤ 64 chars, `reason` ≤ 256 chars, times are integer ms below 600000 or null |
| GET /v1/ice-servers | | `{iceServers:[IceServer], relays:[RelaySite], ttlSeconds}` |

Slots are assigned lowest-free from 1. A lobby with no host WebSocket for 60 seconds ends with
`reason:"host_lost"`. Codes are unique among open lobbies.

### TURN credentials

The backend and every relay share `TURN_SHARED_SECRET`. Credentials follow the TURN REST API
(coturn `use-auth-secret`): `username = "<unixExpiry>:<userId>"`, `credential = base64(HMAC-SHA1(secret, username))`,
TTL 6 hours. `iceServers` lists, per relay site in `RELAYS`: one STUN entry `stun:<host>:3478` without credentials,
then one TURN entry with `turn:<host>:3478?transport=udp`, `turn:<host>:3478?transport=tcp` and `turns:<host>:5349?transport=tcp`.
Relay hosts are configuration (`RELAYS` JSON env), for example `[{"id":"nl","name":"Netherlands","host":"nl-relay.zortos.me"},{"id":"ca","name":"Canada","host":"ca-relay.zortos.me"}]`.
The final subdomains are not decided; nothing may hard-code them.

### Join landing page

`GET /join/:code` serves a small HTML page: game, host display name, "Open in OpenNOW" (`opennow://join/<CODE>`)
and "Get OpenNOW" (the download URL from env `DOWNLOAD_URL`). Invalid or expired codes render a friendly expired state.

## 4. Social backend WebSocket (`GET /v1/ws`)

Authenticate with the first frame `{"type":"auth","token":"..."}` within 5 seconds. Server replies `{"type":"ready","user":User|null,"guest":{...}|null}`.
Heartbeat: client sends `{"type":"ping"}` every 20 s; the server closes after 60 s of silence. Frames are at most 64 KiB.
A bad frame gets `{"type":"error","error":{code,message}}` without closing. Close codes: 4001 no auth within 5 s,
4003 bad auth, 4008 60 s of silence, 4009 more than 8 sockets, 4010 signed out or account deleted (do not reconnect;
the account is unlinked), 1009 frame over 64 KiB, 1012 server restarting (reconnect).
A user may hold several sockets; presence is the most recent `presence.set` across sockets and goes offline 30 s after the last socket closes.

Client to server:

```json
{"type":"presence.set","activity":"playing","game":{...},"since":"...","queuePosition":null,"lobbyId":null}
{"type":"signal","lobbyId":"lob_...","to":"usr_...","payload":{...}}
{"type":"ping"}
```

Server to client (each carries `"type"`):

| Type | Payload |
|---|---|
| `presence.update` | `{userId, presence}` filtered by the subject's privacy settings |
| `friends.changed` | `{}` refetch GET /v1/friends |
| `invite.received` / `invite.updated` | `{invite}` |
| `joinRequest.received` / `joinRequest.updated` | `{joinRequest}` |
| `lobby.updated` | `{lobby}` |
| `lobby.ended` | `{lobbyId, reason:"host_ended"|"host_lost"|"expired"}` |
| `lobby.kicked` | `{lobbyId}` |
| `signal` | `{lobbyId, from, payload}` only between members of the same lobby; payload ≤ 16 KiB, opaque to the backend |
| `pong` | `{}` |

## 5. Backend to Discord bot

The bot exposes `POST /v1/dm-invite` and `GET /healthz`. Requests carry `X-OpenNOW-Timestamp` (unix seconds) and
`X-OpenNOW-Signature: sha256=<hex HMAC-SHA256(BOT_SHARED_SECRET, timestamp + "." + rawBody)>`; the bot rejects skew over 300 s.

```json
{"inviteId":"inv_...","discordUserId":"123","inviter":{"displayName":"Mara","avatarUrl":"..."},
 "game":{"title":"Baldur's Gate 3","artUrl":"..."},"code":"K7Q-4MX","joinUrl":"https://.../join/K7Q4MX","expiresAt":"..."}
```

Response `200 {"result":"sent"|"skipped_not_member"|"failed"}`. The bot checks membership of `GUILD_ID` (the opencloudgaming server)
before DMing and never DMs non-members. The DM is an embed (inviter, game art, code, expiry) with Link buttons
"Join in OpenNOW" (joinUrl) and "Get OpenNOW". The backend calls the bot only when the recipient's `botDmInvites` is true,
and records the result on `invite.dm`.

## 6. Core protocol additions (Qt shell and Rust core)

Capability `social.v1` in `core.hello`. Envelope and protocol version are unchanged (additive). The social token is
stored in the OS credential store like GFN sessions and never crosses to Qt. New core settings: `socialBackendUrl` (string).

Methods (request params, result):

| Method | Params | Result |
|---|---|---|
| `social.state.get` | | `{connection, account:{linked, user, settings}|null, guest:{id, displayName}|null}` |
| `social.link.start` | | `{loginId, loginUrl, expiresAt}`; the core polls and emits `social.account.changed`; Qt opens `loginUrl` |
| `social.link.cancel` | `{loginId}` | `{}` |
| `social.unlink` | | `{}` |
| `social.settings.update` | partial SocialSettings | `{settings}` |
| `social.friends.get` | | GET /v1/friends result |
| `social.users.lookup` | `{username}` | lookup result |
| `social.friends.request` | `{username}` | |
| `social.friends.accept` / `decline` / `cancel` | `{requestId}` | |
| `social.friends.remove` / `block` / `unblock` | `{userId}` | |
| `social.presence.set` | `{activity, game, since, queuePosition, lobbyId}` | `{}`; the core coalesces to at most one send per second and replays it after reconnect |
| `coop.iceServers.get` | | GET /v1/ice-servers result (cached until 10 min before expiry) |
| `coop.lobby.create` | `{game, maxGuests}` | `{lobby}` |
| `coop.lobby.update` | `{lobbyId, state?, maxGuests?}` | `{lobby}` |
| `coop.lobby.regenerateCode` / `close` / `leave` | `{lobbyId}` | |
| `coop.lobby.invite` | `{lobbyId, userId}` | `{invite}` |
| `coop.invite.cancel` / `accept` / `decline` | `{inviteId}` | accept returns `{lobby, slot, iceServers}` |
| `coop.invites.get` | | `{incoming, outgoing}` |
| `coop.join.code` | `{code, guestDisplayName?}` | `{lobby, slot, iceServers}`; when not linked the core first obtains a guest token |
| `coop.join.ask` | `{lobbyId}` | `{joinRequest}` |
| `coop.join.approved` | `{requestId}` | `{lobby, slot, iceServers}` |
| `coop.joinRequest.respond` | `{requestId, approve}` | |
| `coop.member.kick` | `{lobbyId, userId}` | |
| `coop.member.slot` | `{lobbyId, userId, slot}` | `{lobby}` |
| `coop.signal.send` | `{lobbyId, to, payload}` | `{}` |
| `coop.report` | `{lobbyId, event, path, relayId, codec, decoder, connectMs, firstFrameMs, reason}` | `{ok:true}`; POST /v1/lobbies/:id/reports with the same body minus `lobbyId`; absent nullable fields are sent as null |

Errors use the existing error envelope with the backend's stable `code` (for example `user_not_found`) and
`social_offline` / `social_not_linked` from the core.

Events (core to Qt): `social.connection.changed {state:"connected"|"connecting"|"offline"|"unlinked"}`,
`social.account.changed` (same shape as `social.state.get`, plus `link:{loginId, status}` while linking),
`social.friends.changed` (full GET /v1/friends snapshot, sent after every `friends.changed`),
`social.presence.changed {userId, presence}`, `coop.invite.received`, `coop.invite.updated`,
`coop.joinRequest.received`, `coop.joinRequest.updated`, `coop.lobby.updated`, `coop.lobby.ended`,
`coop.lobby.kicked`, `coop.signal {lobbyId, from, payload}`. Payloads match section 4.

Core clarifications (additive; details in `core-protocol.md`, "Social and co-op"):

- `socialBackendUrl` defaults to `""`, meaning the built-in production backend; any other value is an https origin
  (http only for localhost).
- `account.settings` is `null` until the core has loaded `GET /v1/me`.
- `link.status` is `pending`, `complete`, `denied`, `expired` or `cancelled`; the final status is sent once.
- `coop.join.code` uses `guestDisplayName` only when no account is linked; it defaults to `Guest`.
- Besides `social_offline` and `social_not_linked`, the core returns the existing `invalid_params`, `busy`
  (256 signal frames already queued) and `invalid_upstream_response` codes.

Deep link: `opennow://join/<CODE>` is routed by `SingleInstance` to the shell, which opens the C2 join flow with the code filled in.

## 7. Native streamer JSON commands (`opennow_streamer_send`)

No C ABI symbols change. Co-op adds commands and events on the existing JSON channel; the ABI version
is bumped to 16 so Qt can require it. Guest gamepad input uses the existing `opennow_streamer_submit_gamepad`.

### Host (during an active GFN session)

| Command | Fields | Effect |
|---|---|---|
| `coop-host-start` | `iceServers, maxGuests, relays` | Starts the fan-out. Idempotent. |
| `coop-peer-add` | `peerId, slot, displayName` | Creates a peer and registers its slot at once; the host is the offerer and emits `coop-signal-out`. |
| `coop-signal-in` | `peerId, payload` | Delivers an answer or ICE candidate from the guest. |
| `coop-peer-slot` | `peerId, slot` | Moves a guest; releases the old slot with a neutral state first. |
| `coop-peer-input-muted` | `peerId, muted` | Muted guests' input is dropped and their slot gets a neutral state. |
| `coop-peer-remove` | `peerId, reason` | Closes the peer, sends neutral state, unregisters the slot. |
| `coop-host-stop` | | Removes every peer; the host's own stream continues. |

Host events: `coop-signal-out {peerId, payload}`; `coop-peer-state {peerId, state:"connecting"|"connected"|"reconnecting"|"disconnected"|"failed", path:"direct"|"relay"|null, relayId}`;
`coop-host-stats {uploadKbps, capacityKbps, streamKbps, hostToGfnPingMs, peers:[{peerId, slot, path, relayId, peerToHostPingMs, lossPct, sendKbps, inputMuted,
inputReceived, inputInjected, inputDroppedMuted, inputDroppedSlotNotOwned, inputDroppedMalformed, inputDroppedStale, inputDroppedNvstRefused}]}` at 1 Hz.
The input counters are cumulative integers per peer: every record received on `coop-input` is counted once as injected (accepted by
the NVST input path) or under one drop reason. `SlotNotOwned` covers a peer without a connected slot and a slot the NVST input path
has not accepted a registration for; `NvstRefused` means the NVST input path was not ready.

### Guest (no GFN session on this client)

| Command | Fields | Effect |
|---|---|---|
| `coop-guest-start` | `iceServers, relays, hostPeerId, slot` | Starts the guest pipeline; presentation, decode and audio use the normal embedded path. Replaces `start` for guests. |
| `coop-signal-in` | `peerId, payload` | The host's offer and candidates. |
| `coop-guest-stop` | | Leaves. |

Guest events: `coop-signal-out`; `coop-guest-state {state:"negotiating"|"trying_direct"|"direct_blocked"|"relay"|"connected"|"reconnecting"|"ended", path, relayId, reason}`;
`coop-guest-stats {hostToGfnPingMs, guestToHostPingMs, totalMs, quality:"great"|"good"|"poor", lossPct, slot, inputMuted,
inputSamplesPlain, inputSamplesSony, inputStatesSent}` at 1 Hz. The input counters are cumulative integers: local controller samples
taken from plain gamepads and from Sony controllers, and snapshots sent on `coop-input`;
the existing `controller-rumble` for the guest's own slot, mapped to the local controller.

### Signal payloads (opaque to core and backend)

`{"kind":"offer"|"answer","sdp":string}`, `{"kind":"candidate","candidate":string,"mid":string|null}`, `{"kind":"end-of-candidates"}`, `{"kind":"bye","reason":string}`.

### Peer media and data

- Transport: ICE (STUN, then TURN UDP, TURN TCP, TURNS) with DTLS-SRTP via `str0m`. The P2P link uses its own keys; GFN's SRTP keys never leave the host.
- Media: the host taps NVST RTP after SRTP decryption and forwards video and audio packets verbatim (sequence numbers, timestamps,
  NV extension) on two RTP streams. FEC repair packets are not forwarded while OpenNOW cannot use them.
- The host keeps a bounded 256-packet (about 200 ms) retransmission ring per guest and answers guest NACKs locally. Guest keyframe
  requests are forwarded to GFN at most once per second across all guests.
- Data channel `coop-control` (reliable, ordered): first message from the host is the stream description
  `{codec, width, height, fps, bitDepth, chroma, colorRange, colorSpace, hdr, audio:{codec, channels, sampleRate}}`
  taken from the negotiated NVST session; then `{type:"slot"}`, `{type:"input-muted"}`, `{type:"stats", hostToGfnPingMs}` (1 Hz),
  `{type:"rumble"}`, `{type:"keyframe-request"}`.
- Data channel `coop-input` (unordered, maxRetransmits 0): guest gamepad snapshots `{seq:u32, bitmap, buttons, lt, rt, lx, ly, rx, ry, ts}`
  as a fixed 24-byte little-endian binary record; the guest resends the latest snapshot every 50 ms while idle. The host maps
  the guest's forwarded controller to its assigned GFN slot, drops stale sequence numbers and sends neutral state on disconnect,
  mute, slot move and 500 ms without input.

### Slot to player number

Windows on the GFN VM gives each virtual XInput pad the lowest free user index when the pad
appears. The pad appears when its bit first shows up in the `0x020d` registration bitmap. The
player number a game shows therefore depends on the order and gaps of registration, not on the
GFN controller id. The host keeps registration in the shape that makes the two equal:

- A guest's id is registered when `coop-peer-add` arrives, before its link connects. It stays
  registered through link drops and reconnects, mute, and `coop-peer-slot` swaps. Only
  `coop-peer-remove` and `coop-host-stop` release it, and only after a neutral state.
- While any guest holds a slot, every id from 0 through the highest guest slot is registered.
  An id with no guest is a neutral placeholder, so a lone P3 guest is still the third pad.
  When P2 leaves while P3 stays, id 1 stays registered as a placeholder.
- New ids are registered one per `0x020d` registration message in ascending order. Join order,
  connect order and rejoins therefore cannot reorder pads.
- Id 0 belongs to the host. A host pad that SDL places on id 1..3 drives that id only while no
  guest owns it; a guest always wins its id, and the host pad's input on that id is dropped.
  Unplugging such a host pad does not unregister an id inside the co-op range.
- A Sony pad that the host forwards through the rich HID path is a DualShock/DualSense device on
  the VM, not an XInput pad, so its id gets no XInput placeholder and the host is not shown
  twice. The host's Sony device is normally attached when the stream starts, before any guest, so games that
  read Sony and XInput pads together (Steam Input, native DualSense support) see the host first
  and the guests after it in slot order. XInput-only games cannot see a rich host pad at all and
  number the guests from player 1.

Games that ask each player to press a button to join, or that let players pick a controller,
assign players themselves. The lobby number cannot override that.

## 8. Shell activity to presence

The shell sets presence on navigation: Library `browsing_library`, Store `browsing_store`, Settings `in_settings`,
queue `in_queue` with position, streaming `playing` with game and start time, an open lobby `in_lobby`, 10 minutes without
input `idle`, anything else `online`. The backend applies the subject's `shareActivity` and `presenceVisibility` before fan-out.
