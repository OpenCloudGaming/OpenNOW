import QtQuick
import QtCore

// Co-op sessions (docs/coop-sessions.md): lobby, invites, join requests, guest join flow, and the
// bridge between core coop.* methods/events (section 6) and streamer coop-* commands/events
// (section 7). One instance is owned by ShellStore.
QtObject {
    id: root
    required property var coreClient
    required property bool ready
    property var appController: null
    property var friends: null
    // NativeStreamRuntime: emits coopEventReceived(event); coopSupported is the ABI 16 gate.
    property var streamRuntime: null
    // ShellStore.sendNativeCommand(type, params, operation) -> request id or "".
    property var sendNativeCommand: function(type, params, operation) { return "" }
    // ShellStore.applySetting(key, value) for the co-op bitrate preset.
    property var applySetting: function(key, value) {}
    property var currentBitrateMbps: function() { return 0 }
    // ShellStore launch hook: launchGame(game) starts the host's own GFN session.
    property var launchGame: function(game) {}
    // The game the host's running stream plays, for a lobby made from Ctrl+G.
    property var streamGame: null
    // This client's decoder tiers from the streamer hello: {h264, h265, av1}, each "hardware" | "software" | "none".
    property var localDecoders: null
    // The host's running stream codec: "h264" | "h265" | "av1", or "" before it is known.
    property string streamCodec: ""
    // The host's normal codec setting ("auto" or a codec), which a codec preset or restart overrides.
    property string settingsCodec: "auto"
    // The host's session stop is in flight (ShellStore streamStopRequestId).
    property bool hostStopping: false
    property bool hostStreaming: false
    // The host's GFN session exists (it may be restarting); losing it ends co-op.
    property bool hostSessionActive: false
    property bool persistPreferences: true

    readonly property int maxGuestsLimit: 3
    readonly property int maxInvites: 100
    readonly property int maxInviteHistory: 20
    readonly property int maxOutgoingInvites: 32
    readonly property int maxJoinRequests: 8
    readonly property int maxAsks: 16
    readonly property int maxPendingSignals: 64
    readonly property int maxPendingRequests: 48
    readonly property int askAgainDelayMs: 5 * 60 * 1000

    // "none" | "host" | "guest"
    property string role: "none"
    property var lobby: null
    property int slot: 0
    property string selfId: ""
    // Host: peerId -> {peerId, slot, state, path, relayId, peerToHostPingMs, lossPct, sendKbps, inputMuted,
    //   inputReceived, inputInjected, inputDropped* counters}
    property var peers: ({})
    // Host: {uploadKbps, capacityKbps, streamKbps, hostToGfnPingMs}
    property var hostStats: null
    property bool fanoutStarted: false
    property var iceServers: []
    property var relays: []
    property var setupGame: null
    property real restoreBitrateMbps: 0
    property string restoreCodec: ""
    // Host: userId -> decoder tiers from the guest's opennow-capabilities signal.
    property var guestDecoders: ({})
    // Host: "userId:codec" codec prompts answered Not now.
    property var codecPromptsDismissed: []
    // Host: a restart for a codec every member can play, {codec, confirmed}; confirmed once the
    // host ends the session from the confirmation. The relaunch re-creates the lobby and invites its guests.
    property var codecRestart: null
    // Ctrl+G: a lobby for the running stream is being created; friends invited meanwhile wait here.
    property bool creatingLobby: false
    property var queuedInvites: []

    property var incomingInvites: []
    property var inviteHistory: []
    property var outgoingInvites: []
    property string inviteToastId: ""
    // Asks to join, plus guests who joined by code while "Require host approval" is on:
    // {id: "admit-<userId>", kind: "code", lobbyId, from: {id, displayName}, slot}. Those guests get no
    // streamer peer (no media, no input) until the host lets them in.
    property var joinRequests: []
    // Host: guests who get a streamer peer: invited, approved asks, let in after a code join, or joined
    // while approval was off.
    property var admittedIds: []
    // Guest: the in-stream co-op panel or its controllers page has focus, so the shell owns input.
    property bool guestPanelOpen: false
    // Guest: the host has not let this code join in yet.
    property bool admissionPending: false
    // Host notice: {kind: "joining", user, slot} after approving an ask, or {kind: "expired"} when the
    // code ran out before anyone joined (lobby.ended "expired")
    property var hostNotice: null
    property var myAsks: []
    // Guest-side ask outcome: {kind: "declined" | "full" | "room", user, game, lobbyId}
    property var askNotice: null
    property var roomWatch: []

    // Guest flow: "idle" | "entering" | "joining" | "connecting" | "waiting" | "starting" | "playing" |
    // "reconnecting" | "ended" | "error". "starting": connected and the host is streaming, but no
    // frame has been presented yet. The guest is already on the stream route then: the embedded decoder
    // runs on the video item's render path, so the item must render under the waiting overlay.
    property string guestPhase: "idle"
    // Streamer coop-guest-state while connecting
    property string connectStage: ""
    // The streamer presented this pipeline's first frame (coop-guest-state "streaming").
    property bool guestFramePresented: false
    // The first_frame report waits for the next coop-guest-stats, which carries firstFrameMs.
    property bool firstFrameReportPending: false
    readonly property int streamStartTimeoutMs: 15000
    property Timer streamStartTimer: Timer {
        interval: root.streamStartTimeoutMs
        running: root.guestPhase === "starting"
        onTriggered: root.endAsGuest("no_stream")
    }
    property string guestPath: ""
    property string guestRelayId: ""
    property var guestStats: null
    property string joinError: ""
    property string endReason: ""
    // Plain-text platform detail for endReason "media_unavailable" (the guest could not decode the stream).
    property string endDetail: ""
    property string enteredCode: ""
    // {source: "code" | "invite" | "ask", hostName, game, maxPlayers}
    property var joinContext: null
    property string guestStartRequestId: ""
    // A guest whose relay rejects its TURN credentials refetches them and restarts the pipeline once.
    property bool relayRetried: false

    property var pending: ({})
    property var pendingSignals: []

    readonly property bool coopSupported: Boolean(streamRuntime && streamRuntime.coopSupported)
    readonly property bool active: role !== "none"
    readonly property bool isHost: role === "host" && lobby !== null
    readonly property bool lobbyOpen: lobby !== null && lobby.state !== "ended"
    readonly property var members: lobby ? (lobby.members || []).slice().sort((a, b) => a.slot - b.slot) : []
    readonly property var guests: members.filter(member => member.slot > 0)
    readonly property bool guestConnected: guests.some(member => ["direct", "relay"].indexOf(peerLink(member.userId).kind) >= 0)
    readonly property int playerCount: members.length
    readonly property int maxPlayers: lobby ? Number(lobby.maxGuests || 0) + 1 : 0
    readonly property var hostMember: members.find(member => member.slot === 0) || null
    readonly property string hostName: hostMember ? String(hostMember.displayName || "")
        : joinContext ? String(joinContext.hostName || "") : ""
    readonly property var game: lobby ? lobby.game : joinContext ? joinContext.game : setupGame
    readonly property var pendingOutgoing: outgoingInvites.filter(invite => invite.state === "pending")
    readonly property var inviteToast: incomingInvites.find(invite => invite.id === inviteToastId) || null
    readonly property bool guestStreaming: role === "guest" && guestPhase === "playing"
    // Joining, connected or playing as a guest. The account may be the host's own, so a guest never
    // resumes, claims, recovers or stops a GFN session: any session it can see may be the host's stream.
    readonly property bool guestActive: role === "guest"
        || ["joining", "connecting", "waiting", "starting", "playing", "reconnecting"].indexOf(guestPhase) >= 0
    // The C5b join-error card for joinError: {kind: "invalid" | "full" | "ended" | "cant_connect" |
    // "unavailable", eyebrow, title, body}. kind picks the actions: invalid and ended offer a new code,
    // full and cant_connect retry, unavailable only goes back.
    readonly property var joinFailure: joinError === "" ? null : describeJoinFailure(joinError)
    readonly property string guestRelayName: relayName(guestRelayId)
    readonly property int uploadNeededKbps: hostStats ? Number(hostStats.streamKbps || 0) * (guests.length + 1) : 0
    // The streamer has no bandwidth estimate (capacityKbps is null) and OpenNOW stores no measured
    // upload, so capacity is 0 = unknown: show usage only and warn only against a known capacity.
    readonly property int uploadCapacityKbps: hostStats ? Number(hostStats.capacityKbps || 0) : 0
    readonly property bool uploadShort: uploadCapacityKbps > 0 && uploadNeededKbps > uploadCapacityKbps
    readonly property bool promptPending: joinRequests.length > 0 || inviteToast !== null
    // Host: a let-in guest can't decode the running codec: {member, codec, best}; best is "" when no
    // codec works for everyone.
    readonly property var codecPrompt: {
        if (!isHost || streamCodec === "" || codecRestart !== null)
            return null
        const blocked = guests.find(member => isAdmitted(member) && decoderTier(member.userId, streamCodec) === "none"
            && codecPromptsDismissed.indexOf(member.userId + ":" + streamCodec) < 0)
        return blocked ? {member: blocked, codec: streamCodec, best: bestCodec()} : null
    }
    // Host lobby: an open slot whose guest would push upload past the known capacity stays held closed
    // until the host opens it.
    property bool heldSlotOpened: false
    // Online friends who could join: not in this lobby and not in another one.
    // Every friend the host can invite: not in this lobby and not already invited, online first (idle after
    // online), then offline by last seen. Offline friends get the invite as a Discord DM.
    readonly property var invitableFriends: {
        if (!friends || role === "guest")
            return []
        const rank = {browsing_store: 0, browsing_library: 0, in_settings: 0, online: 0, playing: 1, in_queue: 2, in_lobby: 2, idle: 3}
        const invited = outgoingInvites.filter(invite => invite.to && ["pending", "declined", "expired"].indexOf(invite.state) >= 0)
            .map(invite => invite.to.id)
        const seen = friend => Date.parse(String(friend.presence && friend.presence.lastSeenAt || "")) || 0
        return friends.friends.map((friend, order) => ({friend: friend, order: order,
                rank: friends.isOnline(friend) ? (rank[friends.activity(friend)] ?? 0) : 4}))
            .filter(entry => entry.friend.user && invited.indexOf(entry.friend.user.id) < 0
                && !members.some(member => member.userId === entry.friend.user.id))
            .sort((a, b) => a.rank - b.rank || (a.rank === 4 ? seen(b.friend) - seen(a.friend) : 0) || a.order - b.order)
            .slice(0, maxInvites).map(entry => entry.friend)
    }
    readonly property int invitableCount: invitableFriends.length
    // A friend can be invited from anywhere while hosting, or while streaming outside co-op (the
    // invite creates the lobby for the running stream first).
    readonly property bool canInviteFriends: isHost || (role === "none" && coopSupported && hostStreaming)

    // Local co-op preferences (C9 Co-op settings). Persisted with QSettings outside smoke tests.
    property var preferences: ({maxGuests: 3, preset: "same", matchBitrate: true, allowRelay: true,
        requireApproval: false, showLatency: true, guestDisplayName: "", showInStreamMenu: true})
    property Settings preferenceStore: Settings {
        category: "coop"
        property string values: ""
    }

    signal accessibilityAnnounced(string message)
    signal lobbyOverlayRequested()
    // A playing guest pressed the controller's Guide button (or the View+Menu chord).
    signal guestGuideRequested()

    Component.onCompleted: {
        if (!persistPreferences || preferenceStore.values === "")
            return
        try {
            preferences = Object.assign({}, preferences, JSON.parse(preferenceStore.values))
        } catch (error) {
            preferenceStore.values = ""
        }
    }
    onReadyChanged: {
        if (!ready)
            return
        refreshInvites()
        if (guestPhase === "entering" && enteredCode.length === 7)
            joinByCode(enteredCode)
    }

    function setPreference(key, value) {
        const next = Object.assign({}, preferences)
        next[key] = value
        preferences = next
        if (persistPreferences)
            preferenceStore.values = JSON.stringify(next)
    }
    function codecName(codec) {
        return {h264: qsTr("H.264"), h265: qsTr("H.265"), av1: qsTr("AV1")}[codec] || String(codec || "").toUpperCase()
    }
    function decoderTiers(value) {
        const tiers = {}
        for (const codec of ["h264", "h265", "av1"]) {
            const tier = value ? String(value[codec] || "") : ""
            tiers[codec] = tier === "hardware" || tier === "software" ? tier : "none"
        }
        return tiers
    }
    // A guest's tier for codec, or "" while its capabilities are unknown.
    function decoderTier(userId, codec) {
        const tiers = guestDecoders[userId]
        return tiers ? tiers[codec] : ""
    }
    // The best codec every let-in guest and this client decode in hardware, else in software; "" when none.
    function bestCodec(userIds) {
        const ids = userIds || guests.filter(isAdmitted).map(member => member.userId)
        const sets = ids.map(userId => guestDecoders[userId]).filter(Boolean)
        if (localDecoders)
            sets.push(decoderTiers(localDecoders))
        for (const tier of ["hardware", "software"]) {
            const codec = ["av1", "h265", "h264"].find(name => sets.every(tiers => tiers[name] === "hardware" || tiers[name] === tier))
            if (codec)
                return codec
        }
        return ""
    }
    function dismissCodecPrompt() {
        if (codecPrompt)
            codecPromptsDismissed = codecPromptsDismissed.concat([codecPrompt.member.userId + ":" + codecPrompt.codec]).slice(-maxInvites)
    }
    // Restart with codec: the normal end-session confirmation; ending relaunches the game with it.
    function requestCodecRestart(codec) {
        if (!isHost || ["h264", "h265", "av1"].indexOf(codec) < 0 || !appController)
            return
        codecRestart = {codec: codec, confirmed: false}
        appController.showOverlay("desktop-stream-exit-confirm")
    }
    function relayName(relayId) {
        const relay = relays.find(item => item.id === relayId)
        return relay ? String(relay.name || "") : ""
    }
    function memberAt(slotNumber) {
        return members.find(member => member.slot === slotNumber) || null
    }
    function peer(userId) {
        return peers[userId] || null
    }
    // Whole milliseconds for every co-op ping; "–" while there is no measurement.
    function msText(value) {
        return value === undefined || value === null || !isFinite(Number(value)) ? "–" : String(Math.round(Number(value)))
    }
    // Host badge for a guest's link: the streamer's peer state wins, then the path once connected.
    function peerLink(userId) {
        const current = peers[userId] || {}
        const state = String(current.state || "")
        if (state === "failed" || state === "disconnected")
            return {kind: "failed", label: qsTr("CAN'T CONNECT")}
        if (state === "reconnecting")
            return {kind: "reconnecting", label: qsTr("RECONNECTING")}
        if (current.path === "relay")
            return {kind: "relay", label: qsTr("RELAY")}
        if (current.path === "direct")
            return {kind: "direct", label: qsTr("DIRECT")}
        return {kind: "connecting", label: qsTr("CONNECTING")}
    }
    function uploadHolds(card) {
        const streamKbps = hostStats ? Number(hostStats.streamKbps || 0) : 0
        return card.kind === "open" && !heldSlotOpened && uploadCapacityKbps > 0 && streamKbps * card.slot > uploadCapacityKbps
    }
    // One-click invite targets, most likely to join first; friends with an open invite are shown by it instead.
    function inviteFriend(userId) {
        if (isHost)
            invite(userId)
        else if (canInviteFriends)
            hostRunningStream(0, userId)
    }
    // A pending invite's chip state: "dm" when the bot DMed an offline friend, "waits" when it couldn't
    // (DMs off, not in the guild) so the invite waits until they come online, else "pending".
    function inviteChipState(invite) {
        if (invite.state !== "pending")
            return invite.state
        if (friends && friends.isOnline(friends.friendById(invite.to.id)))
            return "pending"
        return invite.dm === "sent" ? "dm" : ["skipped_not_member", "skipped_disabled", "failed"].indexOf(invite.dm) >= 0 ? "waits" : "pending"
    }
    function outgoingInviteFor(userId) {
        return outgoingInvites.find(invite => invite.to && invite.to.id === userId) || null
    }
    // Host slot cards P1..P4: {slot, kind: "host" | "guest" | "invited" | "open" | "closed", member, invite}.
    function slotCards() {
        const cards = []
        const invited = pendingOutgoing.slice().reverse()
        for (let index = 0; index < 4; ++index) {
            const member = memberAt(index)
            if (member) {
                cards.push({slot: index, kind: index === 0 ? "host" : "guest", member: member, invite: null})
            } else if (index >= maxPlayers) {
                cards.push({slot: index, kind: "closed", member: null, invite: null})
            } else if (invited.length > 0) {
                cards.push({slot: index, kind: "invited", member: null, invite: invited.shift()})
            } else {
                cards.push({slot: index, kind: "open", member: null, invite: null})
            }
        }
        return cards
    }

    function take(requestId) {
        const entry = pending[requestId]
        if (entry === undefined)
            return null
        const next = Object.assign({}, pending)
        delete next[requestId]
        pending = next
        return entry
    }
    function call(method, params, entry, timeoutMs) {
        if (!ready || !coreClient)
            return ""
        if (Object.keys(pending).length >= maxPendingRequests)
            return ""
        const requestId = coreClient.request(method, params || {}, timeoutMs || 15000)
        if (requestId === "")
            return ""
        const next = Object.assign({}, pending)
        next[requestId] = Object.assign({method: method}, entry || {})
        pending = next
        return requestId
    }
    function sendStreamer(type, params) {
        return root.sendNativeCommand(type, params || {}, type)
    }

    function refreshInvites() {
        call("coop.invites.get", {}, {})
    }
    function fetchIceServers() {
        call("coop.iceServers.get", {}, {})
    }

    // Host flow ------------------------------------------------------------------------------
    function openSetup(selectedGame) {
        setupGame = selectedGame
        if (appController)
            appController.showOverlay("coop-setup")
    }
    // codec: "h264" | "h265" | "av1" for a restart every member can play; otherwise the co-op preset's
    // codec, and "Same as my stream settings" keeps the host's own. The codec is fixed when the
    // session is created.
    function startHosting(selectedGame, maxGuests, bitrateMbps, codec) {
        const guestsAllowed = Math.max(1, Math.min(maxGuestsLimit, Number(maxGuests || preferences.maxGuests)))
        setupGame = selectedGame
        restoreBitrateMbps = 0
        if (Number(bitrateMbps) > 0 && Number(bitrateMbps) !== Number(currentBitrateMbps())) {
            restoreBitrateMbps = Number(currentBitrateMbps())
            applySetting("maxBitrateMbps", Number(bitrateMbps))
        }
        const presetCodec = String(preferences.preset || "same").split("-")[1] || ""
        const chosen = codec || (["h264", "h265", "av1"].indexOf(presetCodec) >= 0 ? presetCodec : "")
        restoreCodec = ""
        if (chosen !== "" && chosen !== settingsCodec) {
            restoreCodec = settingsCodec
            applySetting("codec", chosen)
        }
        call("coop.lobby.create", {game: gamePayload(selectedGame), maxGuests: guestsAllowed}, {})
        fetchIceServers()
        if (appController)
            appController.showOverlay("")
        launchGame(selectedGame)
    }
    // Ctrl+G on a running stream: the stream becomes the lobby's game, with no relaunch. A friend
    // invited before the lobby exists is invited as soon as it does.
    function hostRunningStream(maxGuests, inviteUserId) {
        if (role !== "none" || !hostStreaming)
            return
        if (inviteUserId && queuedInvites.indexOf(inviteUserId) < 0)
            queuedInvites = queuedInvites.concat([inviteUserId]).slice(0, maxGuestsLimit)
        if (creatingLobby)
            return
        const guestsAllowed = Math.max(1, Math.min(maxGuestsLimit, Number(maxGuests || preferences.maxGuests)))
        setupGame = streamGame
        restoreBitrateMbps = 0
        creatingLobby = call("coop.lobby.create", {game: gamePayload(streamGame), maxGuests: guestsAllowed}, {}) !== ""
        if (!creatingLobby) {
            queuedInvites = []
            announceFailure("social_offline")
            return
        }
        fetchIceServers()
    }
    function gamePayload(value) {
        if (!value)
            return {appId: "", title: "", artUrl: null}
        const artUrl = String(value.artUrl || value.heroImageUrl || value.imageUrl || value.boxArtUrl || "")
        return {appId: String(value.appId || value.launchAppId || value.id || ""),
            title: String(value.title || ""), artUrl: artUrl === "" ? null : artUrl}
    }
    // Allow relay fallback off: no TURN servers or relay sites reach the streamer, so peers stay direct.
    function networkConfig() {
        if (preferences.allowRelay !== false)
            return {iceServers: iceServers, relays: relays}
        const direct = iceServers.map(server => Object.assign({}, server,
            {urls: (server.urls || []).filter(url => !/^turns?:/i.test(String(url)))}))
        return {iceServers: direct.filter(server => server.urls.length > 0), relays: []}
    }
    function startFanout() {
        if (!isHost || fanoutStarted || !hostStreaming || iceServers.length === 0)
            return
        const network = networkConfig()
        if (sendStreamer("coop-host-start", {iceServers: network.iceServers, maxGuests: Number(lobby.maxGuests),
                relays: network.relays}) === "")
            return
        fanoutStarted = true
        for (const member of guests.filter(isAdmitted))
            sendStreamer("coop-peer-add", {peerId: member.userId, slot: member.slot, displayName: member.displayName})
        flushSignals()
    }
    function startPlayingTogether() {
        if (!isHost)
            return
        call("coop.lobby.update", {lobbyId: lobby.id, state: "streaming"}, {})
        if (appController && appController.overlay === "coop-lobby")
            appController.showOverlay("")
    }
    function setMaxGuests(count) {
        if (isHost)
            call("coop.lobby.update", {lobbyId: lobby.id, maxGuests: Math.max(1, Math.min(maxGuestsLimit, count))}, {})
    }
    function regenerateCode() {
        if (isHost)
            call("coop.lobby.regenerateCode", {lobbyId: lobby.id}, {})
    }
    function requestEndCoop() {
        if (isHost && appController)
            appController.showOverlay("coop-end-confirm")
    }
    function confirmEndCoop() {
        if (!isHost)
            return
        call("coop.lobby.close", {lobbyId: lobby.id}, {})
        finishHosting()
        if (appController && appController.overlay === "coop-end-confirm")
            appController.showOverlay("")
        accessibilityAnnounced(qsTr("Co-op ended. Your stream keeps running."))
    }
    function finishHosting() {
        if (fanoutStarted)
            sendStreamer("coop-host-stop", {})
        if (restoreBitrateMbps > 0)
            applySetting("maxBitrateMbps", restoreBitrateMbps)
        restoreBitrateMbps = 0
        if (restoreCodec !== "")
            applySetting("codec", restoreCodec)
        restoreCodec = ""
        guestDecoders = ({})
        codecPromptsDismissed = []
        fanoutStarted = false
        peers = ({})
        hostStats = null
        outgoingInvites = []
        joinRequests = []
        admittedIds = []
        hostNotice = null
        heldSlotOpened = false
        resetLobby()
    }
    function invite(userId) {
        if (!isHost)
            return
        admit(userId)
        call("coop.lobby.invite", {lobbyId: lobby.id, userId: userId}, {userId: userId})
    }
    function cancelInvite(inviteId) {
        call("coop.invite.cancel", {inviteId: inviteId}, {inviteId: inviteId})
    }
    function resendInvite(userId) {
        invite(userId)
    }
    function kick(userId) {
        if (isHost)
            call("coop.member.kick", {lobbyId: lobby.id, userId: userId}, {})
    }
    function moveSlot(userId, slotNumber) {
        if (isHost)
            call("coop.member.slot", {lobbyId: lobby.id, userId: userId, slot: slotNumber}, {})
    }
    function setInputMuted(peerId, muted) {
        if (!isHost)
            return
        sendStreamer("coop-peer-input-muted", {peerId: peerId, muted: muted})
        const current = peers[peerId] || {peerId: peerId}
        updatePeer(peerId, Object.assign({}, current, {inputMuted: muted}))
    }
    function respondJoinRequest(requestId, approve) {
        const request = joinRequests.find(item => item.id === requestId)
        joinRequests = joinRequests.filter(item => item.id !== requestId)
        if (request && request.kind === "code") {
            answerCodeJoin(request, approve)
            return
        }
        call("coop.joinRequest.respond", {requestId: requestId, approve: approve}, {requestId: requestId})
        if (approve && request) {
            admit(request.from.id)
            hostNotice = {kind: "joining", user: request.from, slot: nextFreeSlot(), requestId: requestId}
        }
    }
    function isAdmitted(member) {
        return member.slot === 0 || admittedIds.indexOf(member.userId) >= 0
    }
    // Guests still in the lobby are never evicted; the cap only drops the oldest admissions nobody used.
    function admit(userId) {
        if (!userId || admittedIds.indexOf(userId) >= 0)
            return
        const present = (lobby && lobby.members || []).map(member => member.userId)
        const next = admittedIds.concat([userId])
        const joined = next.filter(id => present.indexOf(id) >= 0)
        admittedIds = joined.concat(next.filter(id => present.indexOf(id) < 0).slice(joined.length - maxOutgoingInvites))
    }
    function sendAdmission(userId, state) {
        call("coop.signal.send", {lobbyId: lobby.id, to: userId, payload: {type: "opennow-admission", state: state}}, {})
    }
    function holdForApproval(member) {
        if (joinRequests.some(item => item.id === "admit-" + member.userId))
            return
        joinRequests = joinRequests.concat([{id: "admit-" + member.userId, kind: "code", lobbyId: lobby.id,
            from: {id: member.userId, displayName: member.displayName, avatarUrl: member.avatarUrl}, slot: member.slot}])
            .slice(-maxJoinRequests)
        sendAdmission(member.userId, "pending")
    }
    function answerCodeJoin(request, approve) {
        const member = memberAt(request.slot)
        if (!approve) {
            call("coop.member.kick", {lobbyId: lobby.id, userId: request.from.id}, {})
            return
        }
        admit(request.from.id)
        sendAdmission(request.from.id, "admitted")
        if (fanoutStarted && member && member.userId === request.from.id)
            sendStreamer("coop-peer-add", {peerId: member.userId, slot: member.slot, displayName: member.displayName})
        hostNotice = {kind: "joining", user: request.from, slot: request.slot}
    }
    function respondToFirstJoinRequest(approve) {
        if (joinRequests.length === 0)
            return false
        respondJoinRequest(joinRequests[0].id, approve)
        return true
    }
    // Ctrl+Shift+Y / Ctrl+Shift+N: the oldest ask to join first, otherwise the invite toast.
    function answerFirstPrompt(accept) {
        if (respondToFirstJoinRequest(accept))
            return true
        if (!inviteToast)
            return false
        if (accept)
            acceptInvite(inviteToast.id)
        else
            dismissInviteToast()
        return true
    }
    function nextFreeSlot() {
        for (let index = 1; index < maxPlayers; ++index) {
            if (!memberAt(index))
                return index
        }
        return 0
    }
    function joinUrl() {
        return lobby ? String(lobby.joinUrl || "") : ""
    }
    function copyLink() {
        if (lobby && appController && appController.writeClipboardText(joinUrl()))
            accessibilityAnnounced(qsTr("Invite link copied"))
    }
    function copyCode() {
        if (lobby && appController && appController.writeClipboardText(String(lobby.code || "")))
            accessibilityAnnounced(qsTr("Co-op code copied"))
    }

    // Guest flow -----------------------------------------------------------------------------
    function openJoin(code) {
        if (role === "host")
            return
        enteredCode = String(code || "")
        joinError = ""
        endReason = ""
        guestPhase = "entering"
        if (appController)
            appController.navigate("coop-guest")
        if (enteredCode.replace("-", "").length === 6)
            joinByCode(enteredCode)
    }
    function joinByCode(code) {
        const compact = String(code || "").toUpperCase().replace(/[^0-9A-Z]/g, "")
        if (compact.length !== 6 || ["joining", "connecting", "waiting", "playing"].indexOf(guestPhase) >= 0)
            return
        enteredCode = compact.slice(0, 3) + "-" + compact.slice(3)
        joinContext = {source: "code", hostName: "", game: null}
        beginJoin("coop.join.code", {code: enteredCode,
            guestDisplayName: String(preferences.guestDisplayName || "") || undefined})
    }
    function acceptInvite(inviteId) {
        const invite = incomingInvites.find(item => item.id === inviteId)
        if (!invite || role === "host")
            return
        joinContext = {source: "invite", hostName: friends ? friends.displayName(invite.from) : "", game: invite.game,
            maxPlayers: presenceLobbyMax(invite.from)}
        if (inviteToastId === inviteId)
            inviteToastId = ""
        if (appController)
            appController.navigate("coop-guest")
        beginJoin("coop.invite.accept", {inviteId: inviteId})
    }
    function declineInvite(inviteId) {
        call("coop.invite.decline", {inviteId: inviteId}, {inviteId: inviteId})
        moveInviteToHistory(inviteId, "declined")
        if (inviteToastId === inviteId)
            inviteToastId = ""
    }
    function dismissInviteToast() {
        inviteToastId = ""
    }
    function removeInviteHistory(inviteId) {
        inviteHistory = inviteHistory.filter(item => item.id !== inviteId)
    }
    function askToJoin(friend) {
        const presence = friend && friend.presence || {}
        if (!presence.lobby || !presence.lobby.id || myAsks.length >= maxAsks)
            return
        call("coop.join.ask", {lobbyId: presence.lobby.id}, {user: friend.user, game: presence.game, lobbyId: presence.lobby.id})
    }
    function cancelAsk(requestId) {
        myAsks = myAsks.filter(ask => ask.id !== requestId)
    }
    function watchForRoom(userId) {
        if (roomWatch.indexOf(userId) < 0 && roomWatch.length < maxAsks)
            roomWatch = roomWatch.concat([userId])
        askNotice = null
    }
    function stopWatchingForRoom(userId) {
        roomWatch = roomWatch.filter(id => id !== userId)
        askNotice = null
    }
    function dismissAskNotice() {
        askNotice = null
    }
    function dismissHostNotice() {
        hostNotice = null
    }
    function beginJoin(method, params) {
        joinError = ""
        endReason = ""
        connectStage = "negotiating"
        if (!ready) {
            guestPhase = "entering"
            return
        }
        guestPhase = "joining"
        relayRetried = false
        if (call(method, params, {join: true}, 30000) === "") {
            failJoin("social_offline")
            return
        }
        fetchIceServers()
    }
    function acceptJoin(result) {
        if (!result || !result.lobby) {
            failJoin("lobby_ended")
            return
        }
        role = "guest"
        lobby = result.lobby
        slot = Number(result.slot || 0)
        const me = (lobby.members || []).find(member => member.slot === slot)
        selfId = me ? String(me.userId || "") : ""
        if (Array.isArray(result.iceServers))
            iceServers = result.iceServers
        if (joinContext)
            joinContext = Object.assign({}, joinContext, {game: lobby.game})
        if (localDecoders)
            call("coop.signal.send", {lobbyId: lobby.id, to: String(lobby.hostId || ""),
                payload: {type: "opennow-capabilities", decoders: decoderTiers(localDecoders)}}, {})
        guestPhase = "connecting"
        startGuestPipeline()
    }
    function startGuestPipeline() {
        if (role !== "guest" || !lobby || guestStartRequestId !== "")
            return
        const network = networkConfig()
        guestStartRequestId = sendStreamer("coop-guest-start", {iceServers: network.iceServers, relays: network.relays,
            hostPeerId: lobby.hostId, slot: slot})
        if (guestStartRequestId === "") {
            failJoin("cant_connect")
            return
        }
        flushSignals()
    }
    function describeJoinFailure(code) {
        const host = hostName
        const named = (withHost, withoutHost) => host !== "" ? withHost.arg(host) : withoutHost
        const maxSlots = joinContext ? Number(joinContext.maxPlayers || 0) : 0
        switch (code) {
        case "invalid_code":
        case "code_expired":
        case "lobby_not_found":
        case "not_found":
            return {kind: "invalid", eyebrow: qsTr("INVALID OR EXPIRED CODE"), title: qsTr("That code doesn't work"),
                body: enteredCode !== ""
                    ? qsTr("%1 doesn't match a lobby. Codes stop working when co-op ends or the host makes a new one.").arg(enteredCode)
                    : qsTr("The code doesn't match a lobby. Codes stop working when co-op ends or the host makes a new one.")}
        case "lobby_full":
        case "too_many_guests":
            return {kind: "full", eyebrow: qsTr("HOST FULL"), title: named(qsTr("%1's game is full"), qsTr("This game is full")),
                body: maxSlots > 0 ? qsTr("All %1 slots are taken. Try again when someone leaves.").arg(maxSlots)
                    : qsTr("Every slot is taken. Try again when someone leaves.")}
        case "lobby_ended":
            return {kind: "ended", eyebrow: qsTr("HOST ENDED"), title: named(qsTr("%1 ended co-op"), qsTr("The host ended co-op")),
                body: named(qsTr("The lobby closed before you got in. Ask %1 for a new code."),
                    qsTr("The lobby closed before you got in. Ask the host for a new code."))}
        case "invite_expired":
        case "invite_not_found":
        case "invite_not_pending":
            return {kind: "ended", eyebrow: qsTr("INVITE EXPIRED"), title: qsTr("That invite expired"),
                body: named(qsTr("Invites last 10 minutes. Ask %1 to invite you again, or use their code."),
                    qsTr("Invites last 10 minutes. Ask the host to invite you again, or use their code."))}
        case "join_request_expired":
        case "join_request_not_found":
        case "join_request_not_pending":
        case "join_request_not_approved":
            return {kind: "ended", eyebrow: qsTr("ASK EXPIRED"), title: qsTr("Your ask to join ran out"),
                body: named(qsTr("An approval lasts 2 minutes and works once. Ask %1 again from Friends."),
                    qsTr("An approval lasts 2 minutes and works once. Ask the host again from Friends."))}
        case "already_in_lobby":
            return {kind: "unavailable", eyebrow: qsTr("ALREADY IN CO-OP"), title: qsTr("You're already in a co-op lobby"),
                body: qsTr("Leave or end that co-op first, then join this one.")}
        case "blocked":
            return {kind: "unavailable", eyebrow: qsTr("CAN'T JOIN"), title: named(qsTr("You can't join %1's game"), qsTr("You can't join this game")),
                body: qsTr("Joining isn't possible between your two accounts.")}
        case "social_not_linked":
            return {kind: "unavailable", eyebrow: qsTr("DISCORD NOT LINKED"), title: qsTr("Your Discord link ended"),
                body: qsTr("Link Discord again in Friends, or join with a code.")}
        case "guest_not_allowed":
        case "join_requests_disabled":
            return {kind: "unavailable", eyebrow: qsTr("DISCORD NOT LINKED"), title: qsTr("Link Discord to join this way"),
                body: qsTr("Joining with a code works without Discord.")}
        case "social_offline":
            return {kind: "cant_connect", eyebrow: qsTr("CAN'T CONNECT"), title: qsTr("Couldn't reach OpenNOW co-op"),
                body: qsTr("Check you're online and try again.")}
        case "rate_limited":
        case "busy":
            return {kind: "cant_connect", eyebrow: qsTr("CAN'T CONNECT"), title: qsTr("Too many tries"),
                body: qsTr("Wait a minute, then try again.")}
        case "cant_connect":
            return {kind: "cant_connect", eyebrow: qsTr("CAN'T CONNECT"), title: named(qsTr("Couldn't reach %1"), qsTr("Couldn't reach the host")),
                body: qsTr("Direct and relay both failed. Check you're online and try again.")}
        }
        return {kind: "cant_connect", eyebrow: qsTr("CAN'T CONNECT"), title: named(qsTr("Couldn't reach %1"), qsTr("Couldn't reach the host")),
            body: qsTr("Something went wrong on the way in. Try again in a moment.")}
    }
    function announceFailure(code) {
        accessibilityAnnounced(friends ? friends.errorText(code) : qsTr("Something went wrong. Try again in a moment."))
    }
    // detail: the streamer's reason, for the co-op report only.
    function failJoin(code, detail) {
        if (joinContext && hostName !== "")
            joinContext = Object.assign({}, joinContext, {hostName: hostName, maxPlayers: maxPlayers || joinContext.maxPlayers})
        joinError = String(code || "cant_connect")
        guestPhase = "error"
        if (role === "guest") {
            reportGuest("ended", detail ? joinError + ": " + detail : joinError)
            if (lobby)
                call("coop.lobby.leave", {lobbyId: lobby.id}, {})
            stopGuestPipeline()
        }
        role = "none"
        lobby = null
    }
    function retryJoin() {
        if (enteredCode !== "" && (!joinContext || joinContext.source === "code"))
            joinByCode(enteredCode)
        else
            guestPhase = "entering"
    }
    function editCode() {
        joinError = ""
        guestPhase = "entering"
    }
    function requestLeave() {
        if (role === "guest" && appController)
            appController.showOverlay("coop-leave-confirm")
    }
    function confirmLeave() {
        if (role !== "guest")
            return
        reportGuest("ended", "left")
        if (lobby)
            call("coop.lobby.leave", {lobbyId: lobby.id}, {})
        stopGuestPipeline()
        resetLobby()
        guestPhase = "idle"
        if (appController) {
            if (appController.overlay === "coop-leave-confirm")
                appController.showOverlay("")
            appController.navigate("home")
        }
    }
    function closeGuestFlow() {
        if (role === "guest") {
            reportGuest("ended", "left")
            if (lobby)
                call("coop.lobby.leave", {lobbyId: lobby.id}, {})
            stopGuestPipeline()
        }
        resetLobby()
        guestPhase = "idle"
        joinError = ""
        endReason = ""
        if (appController)
            appController.navigate("home")
    }
    // Fire-and-forget coop.report: "first_frame" once per pipeline, "ended" once when the guest's co-op ends.
    function reportGuest(event, reason) {
        if (role !== "guest" || !lobby)
            return
        const stats = guestStats || {}
        const ms = value => {
            const number = Math.round(Number(value))
            return value !== null && value !== undefined && number >= 0 && number < 600000 ? number : null
        }
        const text = String(reason || "").replace(/[\u0000-\u001f\u007f]/g, " ").trim().slice(0, 256)
        call("coop.report", {lobbyId: lobby.id, event: event,
            path: guestPath === "direct" || guestPath === "relay" ? guestPath : null,
            relayId: guestPath === "relay" && guestRelayId !== "" ? guestRelayId.slice(0, 64) : null,
            codec: stats.codec ? String(stats.codec).slice(0, 64) : null,
            decoder: stats.decoder === "hardware" || stats.decoder === "software" ? stats.decoder : null,
            connectMs: ms(stats.connectMs), firstFrameMs: ms(stats.firstFrameMs), reason: text !== "" ? text : null}, {})
    }
    function stopGuestPipeline() {
        if (guestStartRequestId !== "")
            sendStreamer("coop-guest-stop", {})
        guestStartRequestId = ""
        guestStats = null
        connectStage = ""
        guestFramePresented = false
        firstFrameReportPending = false
        guestPath = ""
        guestRelayId = ""
    }
    // stillMember: the server still lists this guest (not kicked, lobby not closed), so it leaves the lobby.
    function endAsGuest(reason, detail, stillMember) {
        if (joinContext && lobby)
            joinContext = Object.assign({}, joinContext, {hostName: hostName, hostId: String(lobby.hostId || "")})
        reportGuest("ended", detail ? reason + ": " + detail : reason)
        if (role === "guest" && lobby && stillMember !== false)
            call("coop.lobby.leave", {lobbyId: lobby.id}, {})
        stopGuestPipeline()
        endReason = reason
        endDetail = String(detail || "")
        guestPhase = "ended"
        resetLobby()
        if (appController) {
            if (appController.overlay !== "")
                appController.showOverlay("")
            if (appController.route !== "coop-guest")
                appController.navigate("coop-guest")
        }
    }
    function resetLobby() {
        role = "none"
        lobby = null
        slot = 0
        selfId = ""
        pendingSignals = []
        admissionPending = false
    }

    // Shared event handling -----------------------------------------------------------------
    function acceptLobby(next) {
        if (!next)
            return
        if (role === "none" || !lobby || next.id !== lobby.id)
            return
        const previous = lobby
        lobby = next
        if (role === "host") {
            const before = (previous.members || []).map(member => member.userId)
            const present = (next.members || []).map(member => member.userId)
            joinRequests = joinRequests.filter(item => item.kind !== "code" || present.indexOf(item.from.id) >= 0)
            for (const member of next.members || []) {
                if (before.indexOf(member.userId) >= 0 || isAdmitted(member))
                    continue
                if (preferences.requireApproval === true)
                    holdForApproval(member)
                else
                    admit(member.userId)
            }
            syncPeers(previous, next)
            admittedIds = admittedIds.filter(id => present.indexOf(id) >= 0 || before.indexOf(id) < 0)
            outgoingInvites = outgoingInvites.map(invite => (next.members || []).some(member => invite.to && member.userId === invite.to.id)
                && invite.state === "pending" ? Object.assign({}, invite, {state: "accepted"}) : invite)
        } else {
            const me = (next.members || []).find(member => member.userId === selfId)
            if (me)
                slot = me.slot
            if (next.state === "streaming" && guestPhase === "waiting" && !admissionPending) {
                if (guestFramePresented)
                    enterGuestStream()
                else
                    awaitGuestStream()
            }
        }
    }
    function syncPeers(previous, next) {
        if (!fanoutStarted)
            return
        const before = {}
        for (const member of previous.members || [])
            before[member.userId] = member
        const after = {}
        for (const member of next.members || []) {
            after[member.userId] = member
            if (member.slot === 0)
                continue
            const old = before[member.userId]
            if (!isAdmitted(member))
                continue
            if (!old)
                sendStreamer("coop-peer-add", {peerId: member.userId, slot: member.slot, displayName: member.displayName})
            else if (old.slot !== member.slot)
                sendStreamer("coop-peer-slot", {peerId: member.userId, slot: member.slot})
        }
        for (const userId of Object.keys(before)) {
            if (before[userId].slot !== 0 && !after[userId] && isAdmitted(before[userId])) {
                sendStreamer("coop-peer-remove", {peerId: userId, reason: "left"})
                const next = Object.assign({}, peers)
                delete next[userId]
                peers = next
            }
        }
    }
    // Logs each guest controller ownership change (qt-native.log) with the shell's reason.
    property string guestInputOwner: ""
    onRoleChanged: if (role !== "guest") guestInputOwner = ""
    function noteGuestInputOwner(owner, reason, suspended) {
        const line = owner + " " + reason + " " + suspended
        if (line === guestInputOwner)
            return
        guestInputOwner = line
        if (streamRuntime && typeof streamRuntime.logCoopGuestInput === "function")
            streamRuntime.logCoopGuestInput(owner, reason, suspended)
    }
    function enterGuestStream() {
        guestPhase = "playing"
        if (appController)
            appController.navigate("stream")
    }
    function awaitGuestStream() {
        guestPhase = "starting"
        if (appController)
            appController.navigate("stream")
    }
    function acceptLobbyEnded(payload) {
        if (!lobby || payload.lobbyId !== lobby.id)
            return
        const reason = String(payload.reason || "host_ended")
        if (role === "host") {
            finishHosting()
            if (reason === "expired") {
                hostNotice = {kind: "expired"}
                accessibilityAnnounced(qsTr("Co-op closed because no one joined before the code expired."))
            }
            return
        }
        endAsGuest(reason === "host_lost" ? "host_lost" : "host_ended", "", false)
    }
    function recordInvite(invite, received) {
        if (!invite)
            return
        if (lobby && role === "host" && invite.lobbyId === lobby.id && invite.from && invite.from.id === selfId) {
            const others = outgoingInvites.filter(item => !(item.to && invite.to && item.to.id === invite.to.id))
            outgoingInvites = [invite].concat(others).slice(0, maxOutgoingInvites)
            return
        }
        if (invite.state === "pending") {
            const others = incomingInvites.filter(item => item.id !== invite.id)
            incomingInvites = [invite].concat(others).slice(0, maxInvites)
            if (received && role === "none")
                inviteToastId = invite.id
        } else {
            moveInviteToHistory(invite.id, invite.state, invite)
        }
    }
    function moveInviteToHistory(inviteId, state, updated) {
        const existing = updated || incomingInvites.find(item => item.id === inviteId)
        incomingInvites = incomingInvites.filter(item => item.id !== inviteId)
        if (inviteToastId === inviteId)
            inviteToastId = ""
        if (!existing || state === "accepted" || state === "declined")
            return
        const entry = Object.assign({}, existing, {state: state, endedAt: new Date().toISOString()})
        inviteHistory = [entry].concat(inviteHistory.filter(item => item.id !== inviteId)).slice(0, maxInviteHistory)
    }
    function recordJoinRequest(request, received) {
        if (!request)
            return
        if (lobby && role === "host" && request.lobbyId === lobby.id) {
            const others = joinRequests.filter(item => item.id !== request.id)
            joinRequests = request.state === "pending"
                ? others.concat([request]).slice(-maxJoinRequests) : others
            return
        }
        const ask = myAsks.find(item => item.id === request.id)
        if (!ask)
            return
        if (request.state === "approved") {
            myAsks = myAsks.filter(item => item.id !== request.id)
            joinContext = {source: "ask", hostName: friends ? friends.displayName(ask.host) : "", game: request.game,
                maxPlayers: presenceLobbyMax(ask.host)}
            if (appController)
                appController.navigate("coop-guest")
            beginJoin("coop.join.approved", {requestId: request.id})
        } else if (request.state === "declined") {
            myAsks = myAsks.map(item => item.id === request.id
                ? Object.assign({}, item, {state: "declined", retryAt: Date.now() + askAgainDelayMs}) : item)
            askNotice = {kind: "declined", user: ask.host, game: request.game, lobbyId: request.lobbyId}
        } else {
            myAsks = myAsks.filter(item => item.id !== request.id)
        }
    }
    // Presence lobby summaries count the host: size includes them and max is maxGuests + 1.
    function presenceLobbyMax(user) {
        const friend = friends && user ? friends.friendById(user.id) : null
        const summary = friend && friend.presence ? friend.presence.lobby : null
        return summary ? Number(summary.max || 0) : 0
    }
    function acceptPresenceForRoomWatch(userId, presence) {
        if (roomWatch.indexOf(userId) < 0 || !presence || !presence.lobby || !presence.lobby.joinable)
            return
        roomWatch = roomWatch.filter(id => id !== userId)
        const friend = friends ? friends.friendById(userId) : null
        askNotice = {kind: "room", user: friend ? friend.user : null, game: presence.game, lobbyId: presence.lobby.id}
    }
    function acceptSignal(payload) {
        if (!lobby && guestPhase === "joining") {
            pendingSignals = pendingSignals.concat([payload]).slice(-maxPendingSignals)
            return
        }
        if (!lobby || payload.lobbyId !== lobby.id)
            return
        if (payload.payload && payload.payload.type === "opennow-capabilities") {
            const from = String(payload.from || "")
            if (role === "host" && from !== "" && from !== selfId) {
                const next = {}
                for (const userId of Object.keys(guestDecoders).filter(id => id !== from).slice(-(maxGuestsLimit * 4 - 1)))
                    next[userId] = guestDecoders[userId]
                next[from] = decoderTiers(payload.payload.decoders)
                guestDecoders = next
            }
            return
        }
        if (payload.payload && payload.payload.type === "opennow-admission") {
            if (role === "guest" && String(payload.from || "") === String(lobby.hostId || ""))
                acceptAdmission(String(payload.payload.state || ""))
            return
        }
        const sender = (lobby.members || []).find(member => member.userId === String(payload.from || ""))
        if (role === "host" && sender && !isAdmitted(sender))
            return
        const ready = role === "host" ? fanoutStarted : guestStartRequestId !== ""
        if (!ready) {
            pendingSignals = pendingSignals.concat([payload]).slice(-maxPendingSignals)
            return
        }
        sendStreamer("coop-signal-in", {peerId: String(payload.from || ""), payload: payload.payload})
    }
    // The host holds a code join until it lets the guest in; the pipeline waits so it cannot time out.
    function acceptAdmission(state) {
        if (state === "pending") {
            admissionPending = true
            stopGuestPipeline()
            guestPhase = "waiting"
            if (appController && appController.route !== "coop-guest")
                appController.navigate("coop-guest")
        } else if (state === "admitted" && admissionPending) {
            admissionPending = false
            guestPhase = "connecting"
            startGuestPipeline()
        }
    }
    function flushSignals() {
        const queued = pendingSignals
        pendingSignals = []
        for (const payload of queued)
            acceptSignal(payload)
    }
    function updatePeer(peerId, value) {
        const next = Object.assign({}, peers)
        next[peerId] = value
        peers = next
    }
    function acceptStreamerEvent(event) {
        const type = String(event && event.type || "")
        if (type === "coop-signal-out") {
            if (lobby)
                call("coop.signal.send", {lobbyId: lobby.id, to: String(event.peerId || ""), payload: event.payload}, {})
        } else if (type === "coop-peer-state" && role === "host") {
            const peerId = String(event.peerId || "")
            updatePeer(peerId, Object.assign({}, peers[peerId] || {peerId: peerId},
                {state: String(event.state || ""), path: event.path || null, relayId: event.relayId || null}))
        } else if (type === "coop-host-stats" && role === "host") {
            hostStats = {uploadKbps: Number(event.uploadKbps || 0), capacityKbps: Number(event.capacityKbps || 0),
                streamKbps: Number(event.streamKbps || 0), hostToGfnPingMs: Number(event.hostToGfnPingMs || 0)}
            const next = Object.assign({}, peers)
            for (const item of (event.peers || []).slice(0, maxGuestsLimit))
                next[item.peerId] = Object.assign({}, next[item.peerId] || {}, item)
            peers = next
        } else if (type === "coop-guest-state" && role === "guest") {
            const state = String(event.state || "")
            if (state === "ended") {
                if (String(event.reason || "") === "relay-credentials-rejected" && !relayRetried) {
                    relayRetried = true
                    stopGuestPipeline()
                    guestPhase = "connecting"
                    if (call("coop.iceServers.get", {}, {relayRetry: true}) === "")
                        failJoin("cant_connect")
                    return
                }
                const reason = String(event.reason || "")
                const media = /^media-(unavailable|closed):/.exec(reason)
                if (media)
                    endAsGuest("media_unavailable", reason.slice(media[0].length).replace(/[\u0000-\u001f\u007f<>]/g, " ").trim().slice(0, 256))
                else if (guestPhase === "connecting")
                    failJoin("cant_connect", reason)
                else
                    endAsGuest(reason === "host_stream_lost" ? "stream_lost" : "host_lost", reason)
                return
            }
            guestPath = String(event.path || "")
            guestRelayId = guestPath === "relay" ? String(event.relayId || "") : ""
            if (state === "streaming") {
                if (!guestFramePresented)
                    firstFrameReportPending = true
                guestFramePresented = true
                if (["connecting", "waiting", "starting", "reconnecting"].indexOf(guestPhase) >= 0 && !admissionPending)
                    enterGuestStream()
                return
            }
            connectStage = state
            if (state === "reconnecting") {
                guestPhase = "reconnecting"
            } else if (state === "connected") {
                if (guestPhase === "connecting" || guestPhase === "reconnecting")
                    guestPhase = guestFramePresented ? "playing" : lobby && lobby.state === "streaming" ? "starting" : "waiting"
                const route = guestPhase === "waiting" ? "coop-guest" : "stream"
                if (appController && appController.route !== route)
                    appController.navigate(route)
            }
        } else if (type === "coop-guest-stats" && role === "guest") {
            guestStats = event
            if (firstFrameReportPending) {
                firstFrameReportPending = false
                reportGuest("first_frame", "")
            }
        }
    }

    // Engine::stop destroys the native host with the session; a restarted session needs a fresh fan-out.
    onHostStreamingChanged: {
        if (hostStreaming) {
            startFanout()
        } else if (fanoutStarted) {
            fanoutStarted = false
            peers = ({})
            hostStats = null
        }
    }
    onHostSessionActiveChanged: {
        if (hostSessionActive || !isHost)
            return
        const restart = codecRestart && codecRestart.confirmed ? {codec: codecRestart.codec, game: setupGame || streamGame,
            maxGuests: Number(lobby.maxGuests), bitrateMbps: Number(currentBitrateMbps()),
            invitees: guests.map(member => member.userId)} : null
        codecRestart = null
        call("coop.lobby.close", {lobbyId: lobby.id}, {})
        finishHosting()
        if (!restart) {
            accessibilityAnnounced(qsTr("Your stream ended, so co-op closed for everyone."))
            return
        }
        Qt.callLater(() => {
            queuedInvites = restart.invitees.slice(0, maxGuestsLimit)
            startHosting(restart.game, restart.maxGuests, restart.bitrateMbps, restart.codec)
        })
    }
    // The end-session confirmation closed: ending the session confirms a codec restart, Keep playing drops it.
    property Connections overlayEvents: Connections {
        target: root.appController
        ignoreUnknownSignals: true
        function onOverlayChanged() {
            if (!root.codecRestart || root.codecRestart.confirmed || root.appController.overlay === "desktop-stream-exit-confirm")
                return
            root.codecRestart = root.hostStopping ? Object.assign({}, root.codecRestart, {confirmed: true}) : null
        }
    }

    property Connections streamerEvents: Connections {
        target: root.streamRuntime
        ignoreUnknownSignals: true
        function onCoopEventReceived(event) { root.acceptStreamerEvent(event) }
    }
    property Connections presenceEvents: Connections {
        target: root.coreClient
        ignoreUnknownSignals: true
        function onEventReceived(name, payload) {
            if (name === "coop.invite.received")
                root.recordInvite(payload.invite, true)
            else if (name === "coop.invite.updated")
                root.recordInvite(payload.invite, false)
            else if (name === "coop.joinRequest.received")
                root.recordJoinRequest(payload.joinRequest, true)
            else if (name === "coop.joinRequest.updated")
                root.recordJoinRequest(payload.joinRequest, false)
            else if (name === "coop.lobby.updated")
                root.acceptLobby(payload.lobby)
            else if (name === "coop.lobby.ended")
                root.acceptLobbyEnded(payload)
            else if (name === "coop.lobby.kicked") {
                if (root.lobby && payload.lobbyId === root.lobby.id && root.role === "guest")
                    root.endAsGuest(root.admissionPending ? "declined" : "kicked", "", false)
            } else if (name === "coop.signal")
                root.acceptSignal(payload)
            else if (name === "social.presence.changed")
                root.acceptPresenceForRoomWatch(payload.userId, payload.presence)
        }
        function onResponseReceived(requestId, result) {
            const entry = root.take(requestId)
            if (!entry)
                return
            const method = entry.method
            if (entry.join) {
                if (root.guestPhase === "joining")
                    root.acceptJoin(result)
                else if (result && result.lobby)
                    root.call("coop.lobby.leave", {lobbyId: result.lobby.id}, {})
            } else if (method === "coop.iceServers.get") {
                root.relays = (result.relays || []).slice(0, 8)
                if (root.role !== "guest" || root.iceServers.length === 0 || entry.relayRetry)
                    root.iceServers = (result.iceServers || []).slice(0, 16)
                if (entry.relayRetry)
                    root.startGuestPipeline()
                root.startFanout()
            } else if (method === "coop.lobby.create") {
                root.creatingLobby = false
                root.role = "host"
                root.lobby = result.lobby
                root.selfId = String(result.lobby && result.lobby.hostId || "")
                root.slot = 0
                root.startFanout()
                const queued = root.queuedInvites
                root.queuedInvites = []
                for (const userId of queued)
                    root.invite(userId)
            } else if (method === "coop.lobby.update" || method === "coop.lobby.regenerateCode"
                       || method === "coop.member.slot") {
                if (result && result.lobby)
                    root.acceptLobby(result.lobby)
            } else if (method === "coop.lobby.invite") {
                if (result && result.invite)
                    root.recordInvite(result.invite, false)
            } else if (method === "coop.invite.cancel") {
                root.outgoingInvites = root.outgoingInvites.map(invite => invite.id === entry.inviteId
                    ? Object.assign({}, invite, {state: "cancelled"}) : invite)
            } else if (method === "coop.invites.get") {
                root.incomingInvites = (result.incoming || []).slice(0, root.maxInvites)
                if (root.isHost)
                    root.outgoingInvites = (result.outgoing || []).filter(invite => invite.lobbyId === root.lobby.id)
                        .slice(0, root.maxOutgoingInvites)
            } else if (method === "coop.join.ask") {
                const request = result.joinRequest
                if (request)
                    root.myAsks = [Object.assign({}, request, {host: entry.user, state: "pending"})]
                        .concat(root.myAsks.filter(item => item.id !== request.id)).slice(0, root.maxAsks)
            }
        }
        function onRequestFailed(requestId, code, message) {
            const entry = root.take(requestId)
            if (!entry)
                return
            if (entry.join) {
                if (root.guestPhase === "joining")
                    root.failJoin(code)
            } else if (entry.method === "coop.lobby.create") {
                root.creatingLobby = false
                root.queuedInvites = []
                if (root.restoreBitrateMbps > 0)
                    root.applySetting("maxBitrateMbps", root.restoreBitrateMbps)
                root.restoreBitrateMbps = 0
                if (root.restoreCodec !== "")
                    root.applySetting("codec", root.restoreCodec)
                root.restoreCodec = ""
                root.announceFailure(code)
            } else if (entry.method === "coop.signal.send" || entry.method === "coop.report") {
                return
            } else if (entry.relayRetry) {
                if (root.role === "guest")
                    root.failJoin("cant_connect")
            } else if (entry.method === "coop.join.ask" && (code === "lobby_full" || code === "too_many_guests")) {
                root.myAsks = [{id: "full-" + entry.lobbyId, lobbyId: entry.lobbyId, from: null, game: entry.game,
                    state: "full", host: entry.user}].concat(root.myAsks.filter(item => item.lobbyId !== entry.lobbyId))
                    .slice(0, root.maxAsks)
                root.askNotice = {kind: "full", user: entry.user, game: entry.game, lobbyId: entry.lobbyId}
            } else if (entry.method === "coop.joinRequest.respond") {
                if (root.hostNotice && root.hostNotice.requestId === entry.requestId)
                    root.hostNotice = null
                root.announceFailure(code)
            } else if (entry.method !== "coop.join.ask" && root.friends && root.friends.isNotFound(code)) {
                root.refreshInvites()
            } else if (code !== "social_not_linked") {
                root.announceFailure(code)
            }
        }
    }
}
