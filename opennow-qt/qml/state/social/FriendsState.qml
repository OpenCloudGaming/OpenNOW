import QtQuick
import QtCore

// OpenNOW Friends: Discord-linked account, friends list, requests, presence, and the
// social core-protocol methods/events of docs/coop-sessions.md section 6.
QtObject {
    id: root
    required property var coreClient
    required property bool ready
    property var appController: null
    // Shell activity published as presence while linked: {activity, game, since, queuePosition, lobbyId}
    property var presence: null

    readonly property int maxFriends: 500
    readonly property int maxRequests: 200
    readonly property int maxBlocked: 500
    readonly property int maxPendingRequests: 32
    readonly property int maxRequestResults: 32

    // "connected" | "connecting" | "offline" | "unlinked"
    property string connection: "offline"
    // {linked, user, settings} | null
    property var account: null
    // {id, displayName} | null; the anonymous co-op guest identity
    property var guest: null
    // {loginId, loginUrl, expiresAt, status} while linking; status is "starting", "pending",
    // "denied", "expired" or "error". The core's terminal "complete" and "cancelled" clear it.
    property var link: null
    // The linked-success screen (C2 2b) stays until the user adds a friend or dismisses it.
    property bool welcomePending: false
    // Discord user ids that dismissed the early-access card, persisted with QSettings outside smoke tests.
    property var earlyAccessSeen: []
    property bool persistNotices: true
    property Settings noticeStore: Settings {
        category: "friends"
        property string earlyAccessSeen: ""
    }
    property bool loaded: false
    property var friends: []
    property var incoming: []
    property var outgoing: []
    property var blocked: []
    // {username, state: "searching" | "done" | "error", user, relation, code}
    property var lookup: null
    // username -> {state: "sending" | "sent" | "accepted" | "error", code}
    property var requestResults: ({})
    property string lastError: ""
    property string lastErrorCode: ""
    property var pending: ({})
    property string lastPresence: ""

    readonly property bool linked: Boolean(account && account.linked && account.user)
    readonly property var user: linked ? account.user : null
    // account.settings stays null until the core has loaded GET /v1/me.
    readonly property bool settingsLoaded: linked && Boolean(account.settings)
    readonly property var settings: Object.assign({presenceVisibility: "friends", shareActivity: true,
        joinRequests: "friends", botDmInvites: true}, account && account.settings || {})
    readonly property string linkStatus: link ? String(link.status || "") : ""
    readonly property bool linking: linkStatus === "starting" || linkStatus === "pending"
    readonly property int requestCount: incoming.length
    readonly property int onlineCount: friends.filter(friend => isOnline(friend)).length
    readonly property bool earlyAccessPending: linked && earlyAccessSeen.indexOf(String(user.id || "")) < 0

    signal accessibilityAnnounced(string message)

    Component.onCompleted: {
        if (!persistNotices || noticeStore.earlyAccessSeen === "")
            return
        try {
            const seen = JSON.parse(noticeStore.earlyAccessSeen)
            earlyAccessSeen = Array.isArray(seen) ? seen.map(String).slice(-16) : []
        } catch (error) {
            noticeStore.earlyAccessSeen = ""
        }
    }
    function dismissEarlyAccess() {
        if (!earlyAccessPending)
            return
        earlyAccessSeen = earlyAccessSeen.concat([String(user.id)]).slice(-16)
        if (persistNotices)
            noticeStore.earlyAccessSeen = JSON.stringify(earlyAccessSeen)
    }

    onReadyChanged: if (ready) refresh()
    onPresenceChanged: publishPresence(presence)
    onLinkedChanged: publishPresence(presence)

    // User-facing copy for the core's and backend's stable error codes (docs/core-protocol.md,
    // "Social and co-op"). Screens with a dedicated C2 state read the code itself.
    function errorText(code) {
        switch (String(code || "")) {
        case "social_offline":
            return qsTr("OpenNOW can't reach Friends right now. Check you're online and try again.")
        case "social_not_linked":
            return qsTr("Your Discord link ended. Link Discord again to use Friends.")
        case "rate_limited":
        case "busy":
            return qsTr("That's a lot at once. Wait a moment, then try again.")
        case "self":
            return qsTr("That's you.")
        case "user_not_found":
            return qsTr("No OpenNOW user has that name.")
        case "blocked":
            return qsTr("You can't do that with this person.")
        case "already_friends":
            return qsTr("You're already friends.")
        case "request_pending":
            return qsTr("A friend request is already waiting.")
        case "friend_limit":
            return qsTr("You've reached the friend limit. Remove a friend to add someone new.")
        case "request_limit":
            return qsTr("Too many requests are waiting. Cancel some, then try again.")
        case "invite_limit":
            return qsTr("You've sent a lot of invites. Wait for answers, then try again.")
        case "not_friends":
            return qsTr("You can only invite friends. Share the code or link instead.")
        case "lobby_full":
        case "too_many_guests":
            return qsTr("The lobby is full.")
        case "invite_pending":
            return qsTr("They already have an invite waiting.")
        case "already_member":
            return qsTr("They're already in your lobby.")
        case "already_in_lobby":
            return qsTr("You're already in a co-op lobby. Leave it first.")
        case "invite_expired":
        case "invite_not_pending":
            return qsTr("That invite is no longer open.")
        case "lobby_ended":
            return qsTr("That co-op has ended.")
        case "invalid_code":
        case "code_expired":
            return qsTr("That code doesn't work.")
        case "join_requests_disabled":
            return qsTr("This host isn't taking asks to join.")
        case "join_request_expired":
        case "join_request_not_pending":
        case "join_request_not_approved":
            return qsTr("That ask to join is no longer open.")
        case "guest_not_allowed":
            return qsTr("Link Discord to do that. Joining with a code works without it.")
        case "not_host":
            return qsTr("Only the host can do that.")
        case "payload_too_large":
            return qsTr("That was too large to send.")
        }
        return isNotFound(code) ? qsTr("That's no longer there.") : qsTr("Something went wrong. Try again in a moment.")
    }
    function isNotFound(code) {
        return /(^|_)not_found$/.test(String(code || "")) && code !== "user_not_found"
    }
    function linkErrorText() {
        if (linkStatus === "denied")
            return qsTr("Discord sign-in was cancelled in the browser.")
        if (linkStatus === "expired")
            return qsTr("Discord sign-in timed out. Try again.")
        return linkStatus === "error" ? errorText(lastErrorCode) : ""
    }
    function isOnline(friend) {
        return Boolean(friend && friend.presence && friend.presence.status === "online")
    }
    function activity(friend) {
        return isOnline(friend) ? String(friend.presence.activity || "online") : "offline"
    }
    function friendById(userId) {
        return friends.find(friend => friend.user && friend.user.id === userId) || null
    }
    function displayName(user) {
        return user ? String(user.displayName || user.username || "") : ""
    }
    readonly property var avatarPalette: ["#FCD34D", "#7FD4FF", "#93C5FD", "#C4B5FD", "#F9A8D4", "#86EFAC", "#A5F3FC", "#FDBA74"]
    function avatarColor(user) {
        const key = user ? String(user.username || user.id || "") : ""
        let sum = 0
        for (let index = 0; index < key.length; ++index)
            sum += key.charCodeAt(index)
        return avatarPalette[sum % avatarPalette.length]
    }
    // The friends-list sections of C2 3, filtered by the top-bar chips and the search box.
    function sections(filter, query) {
        const needle = String(query || "").trim().toLowerCase().replace(/^@/, "")
        const matches = friend => !needle || displayName(friend.user).toLowerCase().indexOf(needle) >= 0
            || String(friend.user.username || "").toLowerCase().indexOf(needle) >= 0
        const visible = friends.filter(matches)
        // Online sections keep the backend's snapshot order; offline friends sort by last seen.
        const inCoop = visible.filter(friend => activity(friend) === "in_lobby")
        const playing = visible.filter(friend => ["playing", "in_queue"].indexOf(activity(friend)) >= 0)
        const online = visible.filter(friend => isOnline(friend)
            && ["in_lobby", "playing", "in_queue"].indexOf(activity(friend)) < 0)
        const offline = visible.filter(friend => !isOnline(friend)).sort((a, b) =>
            String(b.presence && b.presence.lastSeenAt || "").localeCompare(String(a.presence && a.presence.lastSeenAt || "")))
        if (filter === "playing")
            return {inCoop: inCoop, playing: playing, online: [], offline: []}
        if (filter === "online")
            return {inCoop: inCoop, playing: playing, online: online, offline: []}
        return {inCoop: inCoop, playing: playing, online: online, offline: offline}
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
        if (Object.keys(pending).length >= maxPendingRequests) {
            lastError = qsTr("OpenNOW is still finishing earlier requests. Try again in a moment.")
            return ""
        }
        const requestId = coreClient.request(method, params || {}, timeoutMs || 15000)
        if (requestId === "")
            return ""
        const next = Object.assign({}, pending)
        next[requestId] = Object.assign({method: method}, entry || {})
        pending = next
        return requestId
    }

    function refresh() {
        call("social.state.get", {}, {})
    }
    function refreshFriends() {
        if (linked)
            call("social.friends.get", {}, {})
    }
    function startLink() {
        if (linking)
            return
        link = {loginId: "", loginUrl: "", expiresAt: "", status: "starting"}
        call("social.link.start", {}, {}, 30000)
    }
    function reopenLinkPage() {
        if (link && link.loginUrl && appController)
            appController.openExternalUrl(link.loginUrl)
    }
    function cancelLink() {
        if (link && link.loginId)
            call("social.link.cancel", {loginId: link.loginId}, {})
        link = null
    }
    function unlink() {
        call("social.unlink", {}, {})
    }
    function dismissWelcome() {
        welcomePending = false
    }
    function updateSettings(partial) {
        if (!settingsLoaded)
            return
        account = Object.assign({}, account, {settings: Object.assign({}, settings, partial)})
        call("social.settings.update", partial, {})
    }
    function normalizeUsername(username) {
        return String(username || "").trim().replace(/^@/, "").toLowerCase().slice(0, 32)
    }
    function lookupUser(username) {
        const name = normalizeUsername(username)
        if (name === "") {
            lookup = null
            return
        }
        lookup = {username: name, state: "searching", user: null, relation: "", code: ""}
        call("social.users.lookup", {username: name}, {username: name})
    }
    function setRequestResult(username, result) {
        const next = Object.assign({}, requestResults)
        next[username] = result
        const keys = Object.keys(next)
        if (keys.length > maxRequestResults)
            delete next[keys[0]]
        requestResults = next
    }
    function sendRequest(username) {
        const name = normalizeUsername(username)
        if (name === "")
            return
        setRequestResult(name, {state: "sending", code: ""})
        call("social.friends.request", {username: name}, {username: name})
        welcomePending = false
    }
    function acceptRequest(requestId) { call("social.friends.accept", {requestId: requestId}, {}) }
    function declineRequest(requestId) { call("social.friends.decline", {requestId: requestId}, {}) }
    function cancelRequest(requestId) { call("social.friends.cancel", {requestId: requestId}, {}) }
    function cancelRequestTo(username) {
        const name = normalizeUsername(username)
        const request = outgoing.find(item => item.user && item.user.username === name)
        if (request)
            cancelRequest(request.id)
        const next = Object.assign({}, requestResults)
        delete next[name]
        requestResults = next
    }
    function removeFriend(userId) { call("social.friends.remove", {userId: userId}, {}) }
    function blockUser(userId) { call("social.friends.block", {userId: userId}, {}) }
    function unblockUser(userId) { call("social.friends.unblock", {userId: userId}, {}) }

    // presence: {activity, game, since, queuePosition, lobbyId} (section 8).
    function publishPresence(presence) {
        if (!ready || !linked || !coreClient || !presence)
            return
        const encoded = JSON.stringify(presence)
        if (encoded === lastPresence)
            return
        lastPresence = encoded
        // The core coalesces to one send per second and replays it after reconnect.
        coreClient.request("social.presence.set", presence, 5000)
    }

    function acceptState(state) {
        if (!state)
            return
        if (state.connection !== undefined)
            connection = String(state.connection || "offline")
        const wasLinked = linked
        account = state.account || null
        guest = state.guest || null
        if (state.link) {
            const status = String(state.link.status || "")
            link = status === "complete" || status === "cancelled" ? null : Object.assign({}, link || {}, state.link)
            if (status === "complete")
                welcomePending = true
        }
        if (!linked) {
            friends = []
            incoming = []
            outgoing = []
            blocked = []
            loaded = false
            welcomePending = false
            lastPresence = ""
        } else if (!wasLinked) {
            link = null
            refreshFriends()
        }
    }
    function acceptFriends(snapshot) {
        if (!snapshot)
            return
        friends = (snapshot.friends || []).slice(0, maxFriends)
        incoming = (snapshot.incoming || []).slice(0, maxRequests)
        outgoing = (snapshot.outgoing || []).slice(0, maxRequests)
        blocked = (snapshot.blocked || []).slice(0, maxBlocked)
        loaded = true
    }
    function acceptPresence(payload) {
        if (!payload || !payload.userId)
            return
        const index = friends.findIndex(friend => friend.user && friend.user.id === payload.userId)
        if (index < 0)
            return
        const next = friends.slice()
        next[index] = Object.assign({}, next[index], {presence: payload.presence})
        friends = next
    }

    property Connections responses: Connections {
        target: root.coreClient
        ignoreUnknownSignals: true
        function onResponseReceived(requestId, result) {
            const entry = root.take(requestId)
            if (!entry)
                return
            root.lastError = ""
            root.lastErrorCode = ""
            const method = entry.method
            if (method === "social.state.get") {
                root.acceptState(result)
                if (!root.linked)
                    root.loaded = true
            } else if (method === "social.link.start") {
                root.link = {loginId: String(result.loginId || ""), loginUrl: String(result.loginUrl || ""),
                    expiresAt: String(result.expiresAt || ""), status: "pending"}
                if (root.link.loginUrl !== "" && root.appController)
                    root.appController.openExternalUrl(root.link.loginUrl)
            } else if (method === "social.unlink") {
                root.acceptState({connection: "unlinked", account: null, guest: root.guest})
                root.accessibilityAnnounced(qsTr("Discord unlinked"))
            } else if (method === "social.settings.update") {
                if (result && result.settings && root.account)
                    root.account = Object.assign({}, root.account, {settings: result.settings})
            } else if (method === "social.friends.get") {
                root.acceptFriends(result)
            } else if (method === "social.users.lookup") {
                if (root.lookup && root.lookup.username === entry.username)
                    root.lookup = {username: entry.username, state: "done", user: result.user || null,
                        relation: String(result.relation || "none"), code: ""}
            } else if (method === "social.friends.request") {
                root.setRequestResult(entry.username, {state: result && result.friend ? "accepted" : "sent", code: ""})
                root.refreshFriends()
            } else if (method !== "social.link.cancel") {
                root.refreshFriends()
            }
        }
        function onRequestFailed(requestId, code, message) {
            const entry = root.take(requestId)
            if (!entry)
                return
            root.lastErrorCode = String(code || "")
            if (entry.method === "social.users.lookup") {
                if (root.lookup && root.lookup.username === entry.username)
                    root.lookup = {username: entry.username, state: code === "user_not_found" ? "done" : "error",
                        user: null, relation: code === "user_not_found" ? "not_found" : "", code: String(code || "")}
                return
            }
            if (entry.method === "social.friends.request") {
                root.setRequestResult(entry.username, {state: "error", code: String(code || "")})
                return
            }
            if (entry.method === "social.link.start")
                root.link = {loginId: "", loginUrl: "", expiresAt: "", status: "error"}
            if (entry.method === "social.settings.update")
                root.refresh()
            if (root.isNotFound(code) || code === "social_not_linked") {
                root.refreshFriends()
                return
            }
            root.lastError = root.errorText(code)
            root.accessibilityAnnounced(root.lastError)
        }
        function onEventReceived(name, payload) {
            if (name === "social.connection.changed")
                root.connection = String(payload.state || "offline")
            else if (name === "social.account.changed")
                root.acceptState(payload)
            else if (name === "social.friends.changed")
                root.acceptFriends(payload)
            else if (name === "social.presence.changed")
                root.acceptPresence(payload)
        }
    }
}
