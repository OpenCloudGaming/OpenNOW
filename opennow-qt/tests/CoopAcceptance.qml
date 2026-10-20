import QtQuick
import OpenNOW

// C2 co-op + Friends acceptance. Seeds FriendsState and CoopState through a mock core client and a
// mock streamer (the real request/response and event paths), then walks every C2 scene and checks
// that each scene's screens are on screen. Driven by src/acceptance/CoopAcceptance.cpp.
QtObject {
    id: fixture
    property string scene: ""
    property bool consoleSurface: false
    property bool checkScreens: true
    property int key: 0
    property int modifiers: 0
    property point click: Qt.point(-1, -1)
    property int clickButton: Qt.LeftButton
    property int activations: 0
    property point hover: Qt.point(-1, -1)
    property string captureName: ""
    property bool startFocusedByDefault: false
    property int launches: 0
    property bool reachedLastStep: false
    // userId -> avatarUrl for the avatar scenes; people not listed have no picture.
    property var avatarUrls: ({})
    property var applied: []
    property string invitedFromStream: ""
    property var onlyFriends: null
    // Scene steps keyed by settle tick, for input that has to land after the screen has focus.
    property var steps: ({})
    property string failure: ""
    property Item root: null
    property var window: null
    property int index: -1
    property int tick: 0
    property int settleTicks: 14
    property bool verified: false
    property Item frame: null
    readonly property var walk: scene === "" ? sceneNames : [scene]
    readonly property double now: Date.now()

    // Scene name -> Paper artboard it reproduces (docs: C2 page p-8-1, Variation C page p-4-0).
    readonly property var artboards: ({
        "c2-01-friends-not-linked": "BP4-1",
        "c2-01b-signed-out": "BP4-1",
        "c2-02a-link-browser": "BUP-1",
        "c2-02b-link-success": "C17-1",
        "c2-03-friends-list": "C6Y-1",
        "c2-03b-friends-context-menu": "",
        "c2-03c-friends-avatars": "",
        "c2-04-lobby-invite-picker": "E6X-1",
        "c2-04b-lobby-invite-offline": "",
        "c2-03d-friends-invite-offline": "",
        "c2-11a-early-access": "",
        "c2-11b-early-access-dismissed": "",
        "c-08l-menu-signed-out": "",
        "c-08m-menu-setting-off": "",
        "c-08n-menu-setting-off-coop": "",
        "c-09b-settings-coop-stream-menu": "",
        "c-03h-lobby-avatars": "",
        "c2-08c-settings-discord-avatar": "",
        "c2-05a-invite-toast": "GLT-1",
        "c2-05c-invite-connecting": "H2M-1",
        "c2-06-ask-to-join": "H3H-1",
        "c2-06b-ask-joining": "H3H-1",
        "c2-06c-ask-declined": "H3H-1",
        "c2-06d-two-asks": "H3H-1",
        "c2-07a-invite-edge-host": "EGQ-1",
        "c2-07b-invites-inbox": "MFZ-1",
        "c2-08a-settings-discord": "MQ9-1",
        "c2-08b-unlink-confirm": "N41-1",
        "c2-09-host-panel": "HAF-1",
        "c2-10a-add-friend": "NJ7-1",
        "c2-10a-add-friend-sent": "NJ7-1",
        "c2-10a-add-friend-not-found": "NJ7-1",
        "c2-10a-add-friend-already-friend": "NJ7-1",
        "c2-10a-add-friend-already-asked": "NJ7-1",
        "c2-10a-add-friend-self": "NJ7-1",
        "c2-10b-requests": "NQW-1",
        "c-01-home-play-coop": "4ZT-0",
        "c-02-host-setup": "56C-0",
        "c-03-host-lobby": "5C7-0",
        "c-03b-lobby-expired": "",
        "c-03c-host-session-ended": "",
        "c-03d-code-join-held": "",
        "c-03e-code-join-let-in": "",
        "c-03f-code-join-declined": "",
        "c-03g-peer-failed": "",
        "c-04-join-code": "5GF-0",
        "c-05-connecting": "5JO-0",
        "c-05b-error-invalid": "5KK-0",
        "c-05b-error-full": "5KK-0",
        "c-05b-error-host-ended": "5KK-0",
        "c-05b-error-cant-connect": "5KK-0",
        "c-05b-error-code-expired": "5KK-0",
        "c-05b-error-already-in-lobby": "5KK-0",
        "c-05b-error-ask-expired": "5KK-0",
        "c-05c-relay-retry": "5JO-0",
        "c-05c-relay-retry-failed": "5KK-0",
        "c-05d-relay-disabled": "5JO-0",
        "c-05e-join-cancelled": "",
        "c-06-waiting-room": "5LZ-0",
        "c-06b-waiting-approval": "",
        "c-06c-waiting-stream": "",
        "c-06d-waiting-stream-timeout": "",
        "c-05f-trying-relay": "",
        "c-07h-guest-direct": "",
        "c-07-guest-overlay": "5ON-0",
        "c-07e-guest-peek": "",
        "c-07g-guest-shared-account": "",
        "c-07f-guest-pinned": "",
        "c-07b-guest-input": "",
        "c-07c-guest-panel-input": "",
        "c-07d-guest-controllers": "",
        "c-08-host-panel": "5Q0-0",
        "c-08b-peer-failed": "",
        "c-08c-start-ready": "",
        "c-08d-start-waiting": "",
        "c-08e-start-streaming": "",
        "c-08f-create-from-stream": "",
        "c-08g-created-from-stream": "",
        "c-08h-invite-from-stream": "",
        "c-08o-invite-offline-from-stream": "",
        "c-08p-invite-offline-hosting": "",
        "c-08i-codec-prompt": "",
        "c-08j-codec-restart": "",
        "c-08k-codec-not-now": "",
        "c-02b-setup-h265": "",
        "c-02c-setup-create-failed": "",
        "c-09-settings-coop": "5UT-0",
        "c-10-reconnecting": "62P-0",
        "c-10-host-ended": "62P-0",
        "c-10-kicked": "62P-0",
        "c-10-media-unavailable": "",
        "c-10-media-unavailable-connecting": "",
        "c-10-declined": "",
        "c-10-stream-lost": "62P-0",
        "c-11-end-confirm": "",
        "c-11-leave-confirm": ""
    })
    readonly property var sceneNames: Object.keys(artboards)

    // Each scene's screens must be visible once it settles. Screen owners add their objectNames.
    readonly property var expectations: ({
        "c2-01-friends-not-linked": ["friendsNotLinked", "friendsNotLinkedEarlyAccess"],
        "c2-11a-early-access": ["friendsEarlyAccess", "friendsEarlyAccessTag", "friendsEarlyAccessBody", "friendsEarlyAccessGotIt", "friendsEarlyAccessDiscord"],
        "c2-11b-early-access-dismissed": ["friendsList", "friendsEarlyAccessPill"],
        "c-08l-menu-signed-out": ["streamSurfaceHost"],
        "c-08m-menu-setting-off": ["streamSurfaceHost"],
        "c-08n-menu-setting-off-coop": ["streamCoopSection", "streamCoopEndRow", "streamCoopStart"],
        "c-09b-settings-coop-stream-menu": ["settingsCoop"],
        "c2-01b-signed-out": ["friendsNotLinked"],
        "c2-02a-link-browser": ["friendsLinking"],
        "c2-02b-link-success": ["friendsLinked"],
        "c2-03-friends-list": ["friendsList", "friendsLobbyBanner", "friendsHoverCard"],
        "c2-03c-friends-avatars": ["friendsList"],
        "c-03h-lobby-avatars": ["coopLobby"],
        "c2-08c-settings-discord-avatar": [],
        "c2-03b-friends-context-menu": ["friendsList"],
        "c2-04-lobby-invite-picker": ["coopLobby", "coopLobbyInviteStrip"],
        "c2-04b-lobby-invite-offline": ["coopLobby", "coopLobbyInviteStrip", "coopLobbyInvite-usr_joris", "coopLobbyInvite-usr_anouk"],
        "c2-03d-friends-invite-offline": ["friendsList", "friendsChip-usr_pieter"],
        "c2-05a-invite-toast": ["coopInviteToast", "coopInvitesInbox"],
        "c2-05c-invite-connecting": ["coopConnecting"],
        "c2-06-ask-to-join": ["coopAskToast"],
        "c2-06b-ask-joining": ["coopHostNotice"],
        "c2-06c-ask-declined": ["coopAskNotice"],
        "c2-06d-two-asks": ["coopAskToast"],
        "c2-07a-invite-edge-host": ["coopLobby", "coopInviteLookup"],
        "c2-07b-invites-inbox": ["friendsInvites", "coopAskNotice"],
        "c2-08a-settings-discord": ["settingsDiscord"],
        "c2-08b-unlink-confirm": ["discordUnlinkConfirm"],
        "c2-09-host-panel": ["streamCoopSection"],
        "c2-10a-add-friend": ["friendsAdd"],
        "c2-10a-add-friend-sent": ["friendsAdd"],
        "c2-10a-add-friend-not-found": ["friendsAdd"],
        "c2-10a-add-friend-already-friend": ["friendsAdd"],
        "c2-10a-add-friend-already-asked": ["friendsAdd"],
        "c2-10a-add-friend-self": ["friendsAdd"],
        "c2-10b-requests": ["friendsRequests", "friendsManageMenu"],
        "c-01-home-play-coop": ["homePlayCoop"],
        "c-02-host-setup": ["coopSetup"],
        "c-03-host-lobby": ["coopLobby"],
        "c-03b-lobby-expired": [],
        "c-03c-host-session-ended": [],
        "c-03d-code-join-held": ["coopAskToast"],
        "c-03e-code-join-let-in": ["coopHostNotice"],
        "c-03f-code-join-declined": [],
        "c-03g-peer-failed": ["coopLobby", "coopLobbyPeerStatus-usr_lieke", "coopLobbyPeerStatus-usr_jonas"],
        "c-04-join-code": ["coopJoinCode"],
        "c-05-connecting": ["coopConnecting"],
        "c-05b-error-invalid": ["coopJoinError"],
        "c-05b-error-full": ["coopJoinError"],
        "c-05b-error-host-ended": ["coopJoinError"],
        "c-05b-error-cant-connect": ["coopJoinError"],
        "c-05b-error-code-expired": ["coopJoinError"],
        "c-05b-error-already-in-lobby": ["coopJoinError"],
        "c-05b-error-ask-expired": ["coopJoinError"],
        "c-05c-relay-retry": ["coopConnecting"],
        "c-05c-relay-retry-failed": ["coopJoinError"],
        "c-05d-relay-disabled": ["coopConnecting"],
        "c-05e-join-cancelled": [],
        "c-06-waiting-room": ["coopWaitingRoom"],
        "c-06b-waiting-approval": ["coopWaitingRoom", "coopWaitingTitle"],
        "c-06c-waiting-stream": ["coopWaitingRoom", "coopWaitingTitle"],
        "c-06d-waiting-stream-timeout": ["coopEnded"],
        "c-05f-trying-relay": ["coopConnecting"],
        "c-07h-guest-direct": ["coopGuestOverlay", "coopGuestVideoValue"],
        "c-07-guest-overlay": ["coopGuestOverlay"],
        "c-07e-guest-peek": ["streamSurfaceHost"],
        "c-07g-guest-shared-account": ["coopGuestOverlay"],
        "c-07f-guest-pinned": ["coopGuestOverlay", "streamSurfaceHost"],
        "c-07b-guest-input": ["coopGuestOverlay", "streamSurfaceHost"],
        "c-07c-guest-panel-input": ["streamSurfaceHost"],
        "c-07d-guest-controllers": ["coopGuestOverlay", "coopGuestOverlayControllers", "coopGuestControllers"],
        "c-08-host-panel": ["streamCoopSection"],
        "c-08b-peer-failed": ["streamCoopSection", "coopPeerLink-usr_lieke", "coopPeerLink-usr_jonas"],
        "c-08c-start-ready": ["streamCoopSection", "streamCoopStart", "coopPeerPing-usr_jonas"],
        "c-08d-start-waiting": ["streamCoopSection", "streamCoopStart", "streamCoopStartCaption"],
        "c-08e-start-streaming": ["streamCoopSection"],
        "c-08f-create-from-stream": ["streamCoopSection", "streamCoopCreateSection", "streamCoopCreate", "streamCoopPlayers-3"],
        "c-08g-created-from-stream": ["streamCoopSection", "streamCoopStart", "streamSurfaceHost"],
        "c-08h-invite-from-stream": ["streamCoopSection", "streamCoopStart"],
        "c-08o-invite-offline-from-stream": ["streamCoopSection", "streamCoopStart"],
        "c-08p-invite-offline-hosting": ["streamCoopSection", "streamCoopStart", "streamCoopInvite-usr_pieter"],
        "c-08i-codec-prompt": ["coopCodecPrompt", "coopCodecRestart", "coopCodecNotNow", "streamSurfaceHost"],
        "c-08j-codec-restart": [],
        "c-08k-codec-not-now": ["streamSurfaceHost"],
        "c-02b-setup-h265": [],
        "c-02c-setup-create-failed": [],
        "c-09-settings-coop": ["settingsCoop"],
        "c-10-reconnecting": ["coopReconnectingToast"],
        "c-10-host-ended": ["coopEnded"],
        "c-10-kicked": ["coopEnded"],
        "c-10-media-unavailable": ["coopEnded", "coopEndedDetail"],
        "c-10-media-unavailable-connecting": ["coopEnded", "coopEndedDetail"],
        "c-10-declined": ["coopEnded"],
        "c-10-stream-lost": ["coopEnded"],
        "c-11-end-confirm": ["coopEndConfirm"],
        "c-11-leave-confirm": ["coopLeaveConfirm"]
    })

    // Mock core client: answers social.* and coop.* asynchronously, records every call, and
    // emits core events on behalf of the scene. Other methods stay pending.
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property int serial: 0
        property var calls: []
        property var failures: ({})
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function cancel(id) { return true }
        function request(method, params, timeout) {
            serial += 1
            const id = "coop-fixture-" + serial
            calls = calls.concat([{id: id, method: method, params: params}]).slice(-200)
            queued = queued.concat([{id: id, method: method, params: params || {}}])
            Qt.callLater(flush)
            return id
        }
        // Qt.callLater collapses repeated calls to one function, so replies drain from a queue.
        property var queued: []
        function flush() {
            const replies = queued
            queued = []
            for (const reply of replies)
                fixture.answer(reply.id, reply.method, reply.params)
        }
        function emit(name, payload) { eventReceived(name, payload) }
    }

    // Mock streamer: records coop-* commands and emits coop-* events.
    property QtObject runtime: QtObject {
        property bool running: true
        property bool coopSupported: true
        property int serial: 0
        property var sent: []
        signal coopEventReceived(var event)
        function command(type, params) {
            serial += 1
            sent = sent.concat([Object.assign({id: "coop-native-" + serial, type: type}, params || {})]).slice(-200)
            return "coop-native-" + serial
        }
    }

    function iso(offsetMs) { return new Date(now + offsetMs).toISOString() }
    readonly property var hostSession: ({sessionId: "gfn-host-session", status: 2, appId: "100001",
        streamingBaseUrl: "https://host.example.invalid", serverIp: "10.0.0.1"})
    function art() { return "qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg" }
    function user(id, name, username) {
        return {id: "usr_" + id, discordId: "1000" + id.length, username: username, displayName: name, avatarUrl: null}
    }
    readonly property var mara: user("mara", "Mara", "mara.exe")
    readonly property var lieke: user("lieke", "Lieke", "liekeplays")
    readonly property var jonas: user("jonas", "Jonas", "jonasvb")
    readonly property var noor: user("noor", "Noor", "noorhaddad")
    readonly property var sam: user("sam", "Sam", "samwise_")
    readonly property var ravi: user("ravi", "Ravi", "ravi.k")
    readonly property var tess: user("tess", "Tess", "tessaaa")
    readonly property var daan: user("daan", "Daan", "daan_dev")
    readonly property var mika: user("mika", "Mika", "mika")
    readonly property var elif: user("elif", "Elif", "elifyz")
    readonly property var pieter: user("pieter", "Pieter", "pieter")
    readonly property var anouk: user("anouk", "Anouk", "anouk")
    readonly property var joris: user("joris", "Joris", "joris88")
    readonly property var femke: user("femke", "Femke", "femke.gg")
    readonly property var bas: user("bas", "Bas", "basbas")
    readonly property var nina: user("nina", "Nina", "ninaplays")
    readonly property var bram: user("bram", "Bram", "bram_")
    function game(appId, title) { return {appId: appId, title: title, artUrl: art()} }
    readonly property var bg3: game("100001", "Baldur’s Gate 3")
    readonly property var stardew: game("100002", "Stardew Valley")
    readonly property var cyberpunk: game("100003", "Cyberpunk 2077")
    readonly property var fortnite: game("100004", "Fortnite")
    readonly property var rocket: game("100005", "Rocket League")
    readonly property var deepRock: game("100006", "Deep Rock Galactic")
    readonly property var settingsDefaults: ({presenceVisibility: "friends", shareActivity: true,
        joinRequests: "friends", botDmInvites: true})

    function presence(activity, extra) {
        return Object.assign({status: "online", activity: activity, game: null, since: null, queuePosition: null,
            lobby: null, lastSeenAt: null}, extra || {})
    }
    function offline(agoMs) {
        return {status: "offline", activity: null, game: null, since: null, queuePosition: null, lobby: null,
            lastSeenAt: iso(-agoMs)}
    }
    function pictured(person) {
        return avatarUrls[person.id] !== undefined ? Object.assign({}, person, {avatarUrl: avatarUrls[person.id]}) : person
    }
    function friendsSnapshot(withRequests) {
        const friend = (person, presenceValue, sinceDays) => ({user: pictured(person), presence: presenceValue, since: iso(-sinceDays * 86400000)})
        return {
            friends: [
                friend(noor, presence("in_lobby", {game: stardew, since: iso(-1800000), lobby: {id: "lob_noor", size: 2, max: 4, joinable: true}}), 300),
                friend(jonas, presence("in_lobby", {game: bg3, since: iso(-600000), lobby: {id: "lob_mara", size: 2, max: 4, joinable: true}}), 400),
                friend(sam, presence("playing", {game: cyberpunk, since: iso(-4320000)}), 220),
                friend(ravi, presence("in_queue", {game: fortnite, since: iso(-120000), queuePosition: 14}), 120),
                friend(lieke, presence("browsing_library"), 90),
                friend(tess, presence("browsing_store"), 80),
                friend(daan, presence("in_settings"), 60),
                friend(mika, presence("online"), 40),
                friend(elif, presence("idle", {since: iso(-720000)}), 30),
                friend(pieter, offline(7200000), 20),
                friend(anouk, offline(86400000 + 3600000), 15),
                friend(joris, offline(3 * 86400000), 10)
            ].filter(item => !onlyFriends || onlyFriends.indexOf(item.user.id) >= 0),
            incoming: withRequests ? [{id: "frq_femke", user: femke, createdAt: iso(-300000)},
                {id: "frq_bas", user: bas, createdAt: iso(-86400000)}] : [],
            outgoing: withRequests ? [{id: "frq_nina", user: nina, createdAt: iso(-120000)},
                {id: "frq_bram", user: bram, createdAt: iso(-4 * 86400000)}] : [],
            blocked: []
        }
    }
    function member(person, slotNumber, anonymous) {
        return {userId: person.id, displayName: person.displayName, avatarUrl: pictured(person).avatarUrl, slot: slotNumber,
            anonymous: Boolean(anonymous), joinedAt: iso(-600000 + slotNumber * 1000)}
    }
    function lobby(state, members, maxGuests) {
        return {id: "lob_mara", hostId: mara.id, code: "K7Q-4MX", joinUrl: "https://opennow.zortos.me/join/K7Q4MX",
            codeExpiresAt: iso(1800000), game: bg3, maxGuests: maxGuests || 3, members: members, state: state || "open"}
    }
    function invite(id, from, to, state, ageMs, dm) {
        return {id: id, lobbyId: from.id === mara.id ? "lob_mara" : "lob_" + from.username, from: pictured(from), to: pictured(to),
            game: from.id === noor.id ? stardew : bg3, state: state, createdAt: iso(-ageMs), expiresAt: iso(600000 - ageMs),
            dm: dm || "skipped_disabled"}
    }
    readonly property var iceResult: ({iceServers: [{urls: ["stun:nl-relay.example.invalid:3478"]},
        {urls: ["turn:nl-relay.example.invalid:3478?transport=udp"], username: "fixture", credential: "fixture"}],
        relays: [{id: "nl", name: "Netherlands", stunUrl: "stun:nl-relay.example.invalid:3478", turnUrls: []},
            {id: "ca", name: "Canada", stunUrl: "stun:ca-relay.example.invalid:3478", turnUrls: []}], ttlSeconds: 21600})

    // Core replies. A method listed in client.failures fails with that code instead.
    function answer(id, method, params) {
        const code = client.failures[method]
        if (code) {
            client.requestFailed(id, code, "fixture " + code)
            return
        }
        let result = null
        if (method === "social.state.get") result = {connection: "unlinked", account: null, guest: null}
        else if (method === "social.link.start") result = {loginId: "login_fixture", expiresAt: iso(600000),
            loginUrl: "https://discord.com/oauth2/authorize?client_id=1479944467112001669&scope=identify"}
        else if (method === "social.friends.get") result = friendsSnapshot(true)
        else if (method === "social.users.lookup") result = ({"jonasvb": {user: jonas, relation: "friend"},
            "bram_": {user: bram, relation: "outgoing"}, "mara.exe": {user: mara, relation: "self"}})[params.username]
            || {user: lieke, relation: "none"}
        else if (method === "social.settings.update") result = {settings: Object.assign({}, ShellStore.friends.settings, params)}
        else if (method === "coop.iceServers.get") result = iceResult
        else if (method === "coop.invites.get") result = {incoming: ShellStore.coop.incomingInvites, outgoing: ShellStore.coop.outgoingInvites}
        else if (method === "coop.lobby.create") result = {lobby: lobby("open", [member(mara, 0)], params.maxGuests)}
        else if (method === "coop.lobby.invite") result = {invite: invite("inv_" + params.userId, mara,
            ShellStore.friends.friendById(params.userId).user, "pending", 0, "pending")}
        else if (method === "coop.join.code" || method === "coop.invite.accept" || method === "coop.join.approved")
            result = {lobby: lobby("open", [member(mara, 0), member(jonas, 1), member(lieke, 2)]), slot: 2,
                iceServers: iceResult.iceServers}
        else if (method === "coop.join.ask") result = {joinRequest: {id: "jrq_" + params.lobbyId, lobbyId: params.lobbyId,
            from: lieke, game: stardew, state: "pending", createdAt: iso(0), expiresAt: iso(120000)}}
        else if (method.indexOf("social.") === 0 || method.indexOf("coop.") === 0) result = {}
        if (result !== null)
            client.responseReceived(id, result)
    }

    function check(condition, message) {
        if (!condition)
            throw new Error(message)
    }
    function find(item, name) {
        if (!item)
            return null
        if (item.objectName === name)
            return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found)
                return found
        }
        return null
    }
    function shown(item) {
        for (let current = item; current; current = current.parent) {
            if (!current.visible || current.opacity <= 0.01)
                return false
        }
        const box = item.mapToItem(null, 0, 0, item.width, item.height)
        return item.width > 0 && item.height > 0 && box.x < window.width && box.y < window.height
            && box.x + box.width > 0 && box.y + box.height > 0
    }

    function linkAs(person) {
        client.emit("social.account.changed", {connection: "connected", guest: null,
            account: {linked: true, user: pictured(person), settings: settingsDefaults}})
    }
    function seedFriends(withRequests) {
        client.emit("social.friends.changed", friendsSnapshot(withRequests))
    }
    function seedHome() {
        ShellStore.catalogGames = [{id: bg3.appId, title: bg3.title, launchAppId: bg3.appId, isAvailable: true,
            isInLibrary: true, imageUrl: art(), heroImageUrl: art(), lastPlayed: iso(-3600000)}]
    }
    function fakeStream() {
        ShellStore.selectedGame = {id: bg3.appId, title: bg3.title, launchAppId: bg3.appId, imageUrl: art(), heroImageUrl: art()}
        ShellStore.streamerStartRequestId = "coop-fixture"
        ShellStore.streamInputPauseRequestId = "coop-fixture"
        ShellStore.streamStartedAtMs = now - 2530000
        ShellStore.activeSession = {sessionId: "coop-fixture", phase: "streaming", status: 3, appId: bg3.appId,
            negotiatedStreamProfile: {width: 1920, height: 1080, fps: 60, codec: "AV1"}}
        ShellStore.streamState = "streaming"
        ShellStore.streamer = {status: "streaming", sessionId: "coop-fixture", firstFrameLatencyMs: 37,
            framesPerSecond: 60, bitrateMbps: 15, codec: "AV1", outputWidth: 1920, outputHeight: 1080}
        AppController.navigate("stream")
    }
    function showFrame() {
        const surface = find(window.contentItem, "streamSurfaceHost")
        if (!surface || frame)
            return
        frame = frameComponent.createObject(surface.parent)
    }
    property Component frameComponent: Component {
        Image {
            objectName: "coopFixtureFrame"
            anchors.fill: parent
            z: 0.5
            source: "qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg"
            fillMode: Image.PreserveAspectCrop
        }
    }
    function host(members, invites, maxGuests) {
        const coop = ShellStore.coop
        coop.startHosting(ShellStore.selectedGame, maxGuests || 3, 0)
        Qt.callLater(() => {
            client.emit("coop.lobby.updated", {lobby: lobby("open", members, maxGuests)})
            for (const item of invites || [])
                client.emit("coop.invite.updated", {invite: item})
            runtime.coopEventReceived({type: "coop-host-stats", uploadKbps: 30000, capacityKbps: 40000, streamKbps: 15000,
                hostToGfnPingMs: 18, peers: members.filter(item => item.slot > 0).map(item => ({peerId: item.userId,
                    slot: item.slot, path: item.userId === lieke.id ? "relay" : "direct", relayId: item.userId === lieke.id ? "nl" : null,
                    peerToHostPingMs: item.userId === lieke.id ? 41 : 24, lossPct: 0.1, sendKbps: 15000, inputMuted: false}))})
            for (const item of members.filter(entry => entry.slot > 0))
                runtime.coopEventReceived({type: "coop-peer-state", peerId: item.userId, state: "connected",
                    path: item.userId === lieke.id ? "relay" : "direct", relayId: item.userId === lieke.id ? "nl" : null})
        })
    }
    function joinAsGuest(source, stage) {
        const coop = ShellStore.coop
        linkAs(lieke)
        if (source === "invite") {
            client.emit("coop.invite.received", {invite: invite("inv_mara", mara, lieke, "pending", 20000, "sent")})
            coop.acceptInvite("inv_mara")
        } else {
            coop.openJoin("K7Q-4MX")
        }
        Qt.callLater(() => {
            if (stage)
                runtime.coopEventReceived({type: "coop-guest-state", state: stage, path: stage === "relay" ? "relay" : null,
                    relayId: stage === "relay" ? "nl" : null, reason: null})
        })
    }
    function guestConnected(streaming) {
        joinAsGuest("code", "")
        Qt.callLater(() => {
            runtime.coopEventReceived({type: "coop-guest-state", state: "relay", path: "relay", relayId: "nl", reason: null})
            runtime.coopEventReceived({type: "coop-guest-state", state: "connected", path: "relay", relayId: "nl", reason: null})
            runtime.coopEventReceived({type: "coop-guest-stats", hostToGfnPingMs: 18, guestToHostPingMs: 41, totalMs: 59,
                quality: "good", lossPct: 0.1, slot: 2, inputMuted: false, connectMs: 840, firstFrameMs: null,
                codec: "h265", decoder: "software"})
            if (streaming) {
                client.emit("coop.lobby.updated", {lobby: lobby("streaming", [member(mara, 0), member(jonas, 1), member(lieke, 2)])})
                runtime.coopEventReceived({type: "coop-guest-state", state: "streaming", path: "relay", relayId: "nl", reason: null})
                runtime.coopEventReceived({type: "coop-guest-stats", hostToGfnPingMs: 18, guestToHostPingMs: 41, totalMs: 59,
                    quality: "good", lossPct: 0.1, slot: 2, inputMuted: false, connectMs: 840, firstFrameMs: 1310,
                    codec: "h265", decoder: "software"})
            }
        })
    }

    function reset() {
        const coop = ShellStore.coop
        if (coop.role === "host")
            coop.finishHosting()
        coop.resetLobby()
        coop.guestPhase = "idle"
        coop.joinError = ""
        coop.endReason = ""
        coop.enteredCode = ""
        coop.joinContext = null
        coop.guestStartRequestId = ""
        coop.guestFramePresented = false
        coop.guestPath = ""
        coop.guestRelayId = ""
        coop.guestStats = null
        coop.connectStage = ""
        coop.streamStartTimer.interval = coop.streamStartTimeoutMs
        coop.incomingInvites = []
        coop.inviteHistory = []
        coop.inviteToastId = ""
        coop.joinRequests = []
        coop.myAsks = []
        coop.askNotice = null
        coop.hostNotice = null
        coop.roomWatch = []
        client.failures = ({})
        coop.setPreference("allowRelay", true)
        coop.setPreference("requireApproval", false)
        coop.setPreference("showInStreamMenu", true)
        ShellStore.friends.earlyAccessSeen = [mara.id, lieke.id, jonas.id]
        startFocusedByDefault = false
        launches = 0
        reachedLastStep = false
        applied = []
        avatarUrls = ({})
        settleTicks = 14
        invitedFromStream = ""
        onlyFriends = null
        coop.localDecoders = null
        coop.codecRestart = null
        coop.codecPromptsDismissed = []
        coop.setPreference("preset", "same")
        ShellStore.streamStopRequestId = ""
        ShellStore.artworkUrls = ({})
        steps = ({})
        modifiers = 0
        clickButton = Qt.LeftButton
        activations = 0
        client.calls = []
        runtime.sent = []
        ShellStore.friends.link = null
        ShellStore.friends.welcomePending = false
        ShellStore.friends.lookup = null
        ShellStore.friends.requestResults = ({})
        client.emit("social.account.changed", {connection: "unlinked", account: null, guest: null})
        ShellStore.activeSession = null
        ShellStore.remoteSessions = []
        ShellStore.conflictSession = null
        ShellStore.sessionRecoveryPending = false
        ShellStore.recoverySessionId = ""
        ShellStore.streamState = "idle"
        ShellStore.streamer = null
        ShellStore.streamerStartRequestId = ""
        ShellStore.streamInputPauseRequestId = ""
        if (frame) {
            frame.destroy()
            frame = null
        }
        AppController.showOverlay("")
        AppController.navigate("home")
    }

    function setupScene(name) {
        const coop = ShellStore.coop
        const friends = ShellStore.friends
        const lobbyMembers = [member(mara, 0), member(jonas, 1)]
        if (name === "c2-01-friends-not-linked") {
            AppController.navigate("friends")
        } else if (name === "c2-01b-signed-out") {
            linkAs(mara)
            seedFriends(false)
            AppController.navigate("friends")
            Qt.callLater(() => {
                client.emit("social.connection.changed", {state: "unlinked"})
                client.emit("social.account.changed", {connection: "unlinked", account: null, guest: null})
            })
        } else if (name === "c-03b-lobby-expired") {
            linkAs(mara)
            fakeStream()
            host([member(mara, 0)], [])
            Qt.callLater(() => Qt.callLater(() => client.emit("coop.lobby.ended", {lobbyId: "lob_mara", reason: "expired"})))
        } else if (name === "c2-02a-link-browser") {
            AppController.navigate("friends")
            friends.startLink()
        } else if (name === "c2-02b-link-success") {
            AppController.navigate("friends")
            client.emit("social.account.changed", {connection: "connected", guest: null,
                account: {linked: true, user: mara, settings: settingsDefaults}, link: {loginId: "login_fixture", status: "complete"}})
            client.emit("social.friends.changed", {friends: [], incoming: [], outgoing: [], blocked: []})
        } else if (name === "c2-03c-friends-avatars" || name === "c-03h-lobby-avatars" || name === "c2-08c-settings-discord-avatar") {
            const cdn = index => "https://cdn.discordapp.com/avatars/10000" + index + "/a1b2c3d4e5f6" + index + ".png?size=4096"
            avatarUrls = ({[mara.id]: cdn(0), [jonas.id]: cdn(1), [noor.id]: cdn(2), [ravi.id]: cdn(3), [tess.id]: cdn(4),
                [mika.id]: cdn(5), [elif.id]: cdn(0), [pieter.id]: cdn(1), [anouk.id]: cdn(2), [joris.id]: cdn(3),
                [sam.id]: "https://example.com/avatars/sam.png", [lieke.id]: "https://cdn.discordapp.com/avatars/0/unreachable.png", [daan.id]: null})
            settleTicks = 40
            // The core's artwork cache resolves a Discord picture to a local file; lieke's never resolves.
            const resolve = () => {
                const urls = Object.assign({}, ShellStore.artworkUrls)
                for (const photo of avatarPhotos(window.contentItem)) {
                    const index = /^https:\/\/cdn\.discordapp\.com\/avatars\/10000(\d)\//.exec(photo.source)
                    if (index)
                        urls[photo.source] = "qrc:/acceptance/avatars/avatar" + index[1] + ".jpg"
                }
                ShellStore.artworkUrls = urls
            }
            steps = ({4: resolve, 8: resolve, 14: resolve, 20: resolve})
            linkAs(mara)
            seedFriends(false)
            if (name === "c2-08c-settings-discord-avatar") {
                AppController.navigate("settings-account")
            } else if (name === "c2-03c-friends-avatars") {
                Qt.callLater(() => AppController.navigate("friends"))
            } else {
                fakeStream()
                host([member(mara, 0), member(jonas, 1), member(lieke, 2)], [invite("inv_tess", mara, tess, "pending", 24000, "sent")])
                Qt.callLater(() => AppController.showOverlay("coop-lobby"))
            }
        } else if (name === "c2-11a-early-access" || name === "c2-11b-early-access-dismissed") {
            linkAs(mara)
            seedFriends(false)
            friends.earlyAccessSeen = [lieke.id]
            AppController.navigate("friends")
            if (name === "c2-11b-early-access-dismissed") {
                settleTicks = 20
                steps = ({6: () => {
                    if (consoleSurface) {
                        friends.dismissEarlyAccess()
                        return
                    }
                    check(find(window.contentItem, "friendsEarlyAccessGotIt").activeFocus, "Got it has focus when the card opens")
                    key = Qt.Key_Return
                }, 9: () => {
                    check(friends.earlyAccessSeen.indexOf(mara.id) >= 0 && !friends.earlyAccessPending, "Got it stores the dismissal for this user")
                    if (consoleSurface)
                        return
                    find(window.contentItem, "friendsEarlyAccessPill").clicked()
                }, 13: () => {
                    if (consoleSurface)
                        return
                    const card = find(window.contentItem, "friendsEarlyAccess")
                    check(card.opened && find(window.contentItem, "friendsEarlyAccessGotIt").activeFocus, "the header pill reopens the card with focus")
                    key = Qt.Key_Escape
                }, 17: () => {
                    check(consoleSurface || !find(window.contentItem, "friendsEarlyAccess").opened, "Esc closes the reopened card")
                    check(friends.earlyAccessSeen.filter(id => id === mara.id).length === 1, "the dismissal is stored once")
                    reachedLastStep = true
                }})
            }
        } else if (name === "c-08l-menu-signed-out" || name === "c-08m-menu-setting-off" || name === "c-08n-menu-setting-off-coop") {
            if (name !== "c-08l-menu-signed-out") {
                linkAs(mara)
                seedFriends(false)
                coop.setPreference("showInStreamMenu", false)
            }
            fakeStream()
            if (name === "c-08n-menu-setting-off-coop")
                host([member(mara, 0), member(jonas, 1)], [])
            steps = ({4: () => {
                modifiers = Qt.ControlModifier
                key = Qt.Key_G
            }})
        } else if (name === "c-09b-settings-coop-stream-menu") {
            linkAs(mara)
            AppController.navigate("settings-coop")
            steps = consoleSurface ? ({}) : ({5: () => {
                const row = find(window.contentItem, "settingsCoopStreamMenuRow")
                check(row !== null, "Settings → Co-op has the stream-menu row")
                let flick = row.parent
                while (flick && flick.contentY === undefined)
                    flick = flick.parent
                if (flick)
                    flick.contentY = Math.max(0, Math.min(flick.contentHeight - flick.height, row.mapToItem(flick.contentItem, 0, 0).y - 160))
            }, 8: () => {
                const toggle = find(window.contentItem, "settingsCoopStreamMenu")
                check(toggle.checked, "the stream-menu setting is on by default")
                captureName = name
            }})
        } else if (name === "c2-03-friends-list") {
            linkAs(mara)
            seedFriends(true)
            fakeStream()
            host(lobbyMembers, [invite("inv_lieke", mara, lieke, "pending", 24000, "sent")])
            Qt.callLater(() => {
                AppController.navigate("friends")
                const screen = find(window.contentItem, "desktopFriendsScreen")
                if (screen)
                    screen.selectedUserId = sam.id
            })
        } else if (name === "c2-04-lobby-invite-picker" || name === "c2-07a-invite-edge-host" || name === "c-03-host-lobby") {
            linkAs(mara)
            seedFriends(false)
            fakeStream()
            const invites = name === "c2-07a-invite-edge-host"
                ? [invite("inv_lieke", mara, lieke, "pending", 42000, "sent"), invite("inv_daan", mara, daan, "declined", 60000, "skipped_disabled"),
                   invite("inv_tess", mara, tess, "expired", 600000, "skipped_not_member"), invite("inv_pieter", mara, pieter, "pending", 30000, "sent")]
                : name === "c2-04-lobby-invite-picker" ? [invite("inv_lieke", mara, lieke, "pending", 24000, "sent")] : []
            host(name === "c-03-host-lobby" ? [member(mara, 0), member(jonas, 1), member(lieke, 2)] : lobbyMembers, invites)
            Qt.callLater(() => {
                AppController.showOverlay("coop-lobby")
                if (name === "c2-07a-invite-edge-host") {
                    client.failures = ({"social.users.lookup": "user_not_found"})
                    friends.lookupUser("kees_")
                }
            })
        } else if (name === "c2-05a-invite-toast") {
            linkAs(lieke)
            seedFriends(false)
            seedHome()
            client.emit("coop.invite.received", {invite: invite("inv_noor", noor, lieke, "pending", 360000, "sent")})
            client.emit("coop.invite.received", {invite: invite("inv_mara", mara, lieke, "pending", 20000, "sent")})
        } else if (name === "c2-05c-invite-connecting") {
            joinAsGuest("invite", "relay")
        } else if (name === "c2-06c-ask-declined") {
            linkAs(tess)
            seedFriends(false)
            seedHome()
            coop.askNotice = {kind: "declined", user: mara, game: bg3, lobbyId: "lob_mara"}
            AppController.navigate("home")
        } else if (name.indexOf("c2-06") === 0) {
            linkAs(mara)
            seedFriends(false)
            fakeStream()
            host(lobbyMembers, [])
            Qt.callLater(() => {
                const ask = (id, person) => ({id: id, lobbyId: "lob_mara", from: person, game: bg3, state: "pending",
                    createdAt: iso(-96000), expiresAt: iso(24000)})
                if (name === "c2-06-ask-to-join" || name === "c2-06d-two-asks")
                    client.emit("coop.joinRequest.received", {joinRequest: ask("jrq_tess", tess)})
                if (name === "c2-06d-two-asks")
                    client.emit("coop.joinRequest.received", {joinRequest: ask("jrq_daan", daan)})
                if (name === "c2-06b-ask-joining") {
                    client.emit("coop.joinRequest.received", {joinRequest: ask("jrq_tess", tess)})
                    coop.respondJoinRequest("jrq_tess", true)
                }
            })
        } else if (name === "c2-07b-invites-inbox") {
            linkAs(lieke)
            seedFriends(false)
            client.emit("coop.invite.received", {invite: invite("inv_mara", mara, lieke, "pending", 20000, "sent")})
            client.emit("coop.invite.updated", {invite: invite("inv_ravi", ravi, lieke, "expired", 1800000, "sent")})
            client.emit("coop.invite.updated", {invite: invite("inv_tess", tess, lieke, "cancelled", 2400000, "sent")})
            client.failures = ({"coop.join.ask": "lobby_full"})
            coop.askToJoin(friends.friendById(noor.id))
            coop.inviteToastId = ""
            AppController.navigate("friends-invites")
        } else if (name === "c2-08a-settings-discord" || name === "c2-08b-unlink-confirm") {
            linkAs(mara)
            seedFriends(true)
            AppController.navigate("settings-account")
            if (name === "c2-08b-unlink-confirm")
                Qt.callLater(() => AppController.showOverlay("discord-unlink-confirm"))
        } else if (name === "c2-09-host-panel" || name === "c-08-host-panel") {
            linkAs(mara)
            seedFriends(false)
            fakeStream()
            host(name === "c-08-host-panel" ? [member(mara, 0), member(jonas, 1), member(lieke, 2)] : lobbyMembers,
                [invite("inv_lieke", mara, lieke, "pending", 24000, "sent")])
            Qt.callLater(() => AppController.showOverlay("desktop-stream-menu"))
        } else if (name.indexOf("c2-10a-add-friend") === 0) {
            linkAs(mara)
            seedFriends(true)
            AppController.navigate("friends-add")
            const variant = name.replace("c2-10a-add-friend", "")
            if (variant === "-not-found")
                client.failures = ({"social.users.lookup": "user_not_found"})
            friends.lookupUser(variant === "-not-found" ? "kees_" : variant === "-already-friend" ? "jonasvb"
                : variant === "-already-asked" ? "bram_" : variant === "-self" ? "mara.exe" : "liekeplays")
            if (variant === "-sent")
                Qt.callLater(() => friends.sendRequest("liekeplays"))
        } else if (name === "c2-10b-requests") {
            linkAs(mara)
            seedFriends(true)
            AppController.navigate("friends-requests")
            const screen = find(window.contentItem, "desktopFriendsScreen")
            if (screen)
                screen.openManage(joris.id)
        } else if (name === "c-01-home-play-coop") {
            linkAs(mara)
            seedHome()
            AppController.navigate("home")
        } else if (name === "c-02-host-setup") {
            linkAs(mara)
            AppController.navigate("home")
            coop.openSetup({id: bg3.appId, title: bg3.title, launchAppId: bg3.appId, imageUrl: art()})
        } else if (name === "c-04-join-code") {
            coop.openJoin("")
            coop.enteredCode = "K7Q"
        } else if (name === "c-05-connecting") {
            joinAsGuest("code", "relay")
        } else if (name.indexOf("c-05b-error") === 0) {
            const code = {"c-05b-error-invalid": "invalid_code", "c-05b-error-full": "lobby_full",
                "c-05b-error-host-ended": "lobby_ended", "c-05b-error-cant-connect": "cant_connect",
                "c-05b-error-code-expired": "code_expired", "c-05b-error-already-in-lobby": "already_in_lobby",
                "c-05b-error-ask-expired": "join_request_expired"}[name]
            if (code === "cant_connect") {
                joinAsGuest("code", "")
                Qt.callLater(() => runtime.coopEventReceived({type: "coop-guest-state", state: "ended", path: null, relayId: null, reason: "failed"}))
            } else {
                client.failures = ({"coop.join.code": code})
                coop.openJoin("K7Q-4MX")
            }
        } else if (name === "c-05c-relay-retry" || name === "c-05c-relay-retry-failed") {
            joinAsGuest("code", "")
            const rejected = {type: "coop-guest-state", state: "ended", path: null, relayId: null, reason: "relay-credentials-rejected"}
            Qt.callLater(() => {
                runtime.coopEventReceived(rejected)
                if (name === "c-05c-relay-retry-failed")
                    Qt.callLater(() => Qt.callLater(() => runtime.coopEventReceived(rejected)))
            })
        } else if (name === "c-05d-relay-disabled") {
            coop.setPreference("allowRelay", false)
            joinAsGuest("code", "")
        } else if (name === "c-05e-join-cancelled") {
            coop.openJoin("K7Q-4MX")
            coop.closeGuestFlow()
        } else if (name === "c-03c-host-session-ended") {
            linkAs(mara)
            fakeStream()
            host([member(mara, 0), member(jonas, 1)], [])
            Qt.callLater(() => Qt.callLater(() => {
                ShellStore.streamState = "idle"
                ShellStore.streamer = null
                ShellStore.activeSession = null
            }))
        } else if (name.indexOf("c-03") === 0 && name.indexOf("code-join") > 0) {
            linkAs(mara)
            seedFriends(false)
            fakeStream()
            host([member(mara, 0), member(jonas, 1)], [])
            Qt.callLater(() => Qt.callLater(() => {
                coop.setPreference("requireApproval", true)
                coop.invite(lieke.id)
                client.emit("coop.lobby.updated", {lobby: lobby("open", [member(mara, 0), member(jonas, 1), member(tess, 2),
                    member(lieke, 3)])})
                client.emit("coop.signal", {lobbyId: "lob_mara", from: tess.id, payload: {type: "offer", sdp: "fixture"}})
                if (name === "c-03e-code-join-let-in") {
                    coop.respondToFirstJoinRequest(true)
                    client.emit("coop.lobby.updated", {lobby: lobby("open", [member(mara, 0), member(tess, 2), member(lieke, 3)])})
                }
                else if (name === "c-03f-code-join-declined")
                    coop.respondToFirstJoinRequest(false)
            }))
        } else if (name === "c-03g-peer-failed" || name === "c-08b-peer-failed") {
            linkAs(mara)
            seedFriends(false)
            fakeStream()
            host([member(mara, 0), member(jonas, 1), member(lieke, 2)], [])
            Qt.callLater(() => Qt.callLater(() => {
                runtime.coopEventReceived({type: "coop-peer-state", peerId: lieke.id, state: "failed", path: null, relayId: null})
                runtime.coopEventReceived({type: "coop-peer-state", peerId: jonas.id, state: "reconnecting", path: "direct", relayId: null})
                AppController.showOverlay(name === "c-03g-peer-failed" ? "coop-lobby" : "desktop-stream-menu")
            }))
        } else if (name === "c-08f-create-from-stream" || name === "c-08g-created-from-stream" || name === "c-08h-invite-from-stream") {
            linkAs(mara)
            seedFriends(false)
            fakeStream()
            const press = objectName => () => {
                if (consoleSurface) {
                    if (objectName === "streamCoopCreate")
                        coop.hostRunningStream(0, "")
                    else if (objectName.indexOf("streamCoopCreateInvite-") === 0)
                        coop.hostRunningStream(1, objectName.slice("streamCoopCreateInvite-".length))
                    return
                }
                const button = find(window.contentItem, objectName)
                check(button && shown(button), name + ": " + objectName + " is not on screen")
                button.clicked()
            }
            steps = ({4: () => {
                modifiers = Qt.ControlModifier
                key = Qt.Key_G
            }})
            if (name === "c-08g-created-from-stream")
                steps[8] = press("streamCoopCreate")
            if (name === "c-08h-invite-from-stream") {
                steps[7] = press("streamCoopPlayers-2")
                steps[8] = () => {
                    invitedFromStream = coop.invitableFriends[0].user.id
                    press("streamCoopCreateInvite-" + invitedFromStream)()
                }
            }
        } else if (name === "c-08i-codec-prompt" || name === "c-08j-codec-restart" || name === "c-08k-codec-not-now") {
            linkAs(mara)
            seedFriends(false)
            fakeStream()
            coop.localDecoders = {h264: "hardware", h265: "hardware", av1: "hardware"}
            host([member(mara, 0), member(jonas, 1), member(lieke, 2)], [])
            launches = 0
            client.calls = []
            const capabilities = (person, decoders) => client.emit("coop.signal", {lobbyId: "lob_mara", from: person.id,
                payload: {type: "opennow-capabilities", decoders: decoders}})
            Qt.callLater(() => Qt.callLater(() => {
                capabilities(jonas, {h264: "hardware", h265: "hardware", av1: "software"})
                capabilities(lieke, {h264: "hardware", h265: "hardware", av1: "none", vp9: "hardware"})
            }))
            const click = objectName => {
                if (consoleSurface) {
                    if (objectName === "coopCodecRestart")
                        coop.requestCodecRestart(coop.codecPrompt.best)
                    else
                        coop.dismissCodecPrompt()
                    return
                }
                const button = find(window.contentItem, objectName)
                check(button && shown(button), name + ": " + objectName + " is not on screen")
                button.clicked()
            }
            if (name === "c-08i-codec-prompt")
                steps = ({4: () => AppController.showOverlay("desktop-stream-menu")})
            if (name === "c-08j-codec-restart") {
                steps = ({4: () => click("coopCodecRestart"), 6: () => {
                    check(AppController.overlay === "desktop-stream-exit-confirm" && coop.codecRestart.codec === "h265",
                        "Restart opens the end-session confirmation")
                    const title = find(window.contentItem, "accountSwitchTitle")
                    check(consoleSurface || (title && shown(title) && title.text === "Restart with H.265?"), "the confirmation names the restart codec")
                    captureName = name + "-confirm"
                }, 7: () => {
                    ShellStore.streamStopRequestId = "coop-fixture-stop"
                    AppController.showOverlay("")
                }, 8: () => {
                    check(coop.codecRestart && coop.codecRestart.confirmed, "ending the session confirms the restart")
                    ShellStore.streamStopRequestId = ""
                    ShellStore.activeSession = null
                }})
            }
            if (name === "c-08k-codec-not-now")
                steps = ({4: () => click("coopCodecRestart"), 6: () => AppController.showOverlay(""), 8: () => {
                    check(coop.codecRestart === null && coop.codecPrompt !== null, "Keep playing drops the restart and keeps the prompt")
                    click("coopCodecNotNow")
                }})
        } else if (name === "c-02c-setup-create-failed") {
            linkAs(mara)
            client.failures = ({"coop.lobby.create": "social_offline"})
            coop.setPreference("preset", "1080p60-h265")
            coop.startHosting({id: bg3.appId, title: bg3.title, launchAppId: bg3.appId, imageUrl: art()}, 0, 0)
        } else if (name === "c-02b-setup-h265") {
            linkAs(mara)
            AppController.navigate("home")
            coop.openSetup({id: bg3.appId, title: bg3.title, launchAppId: bg3.appId, imageUrl: art()})
            steps = consoleSurface ? ({4: () => {
                coop.setPreference("preset", "1080p60-h265")
                coop.startHosting(coop.setupGame, 0, 0)
            }}) : ({4: () => {
                const chip = find(window.contentItem, "coopSetupCodec-h265")
                check(chip && shown(chip), "the setup offers H.265")
                chip.clicked()
            }, 6: () => {
                check(coop.preferences.preset === "1080p60-h265", "the H.265 chip picks the H.265 preset")
                find(window.contentItem, "coopSetupStart").clicked()
            }})
        } else if (name === "c-08c-start-ready" || name === "c-08d-start-waiting" || name === "c-08e-start-streaming") {
            linkAs(mara)
            seedFriends(false)
            fakeStream()
            host([member(mara, 0), member(jonas, 1)], [])
            Qt.callLater(() => Qt.callLater(() => {
                runtime.coopEventReceived({type: "coop-host-stats", uploadKbps: 30000, capacityKbps: 40000, streamKbps: 15000,
                    hostToGfnPingMs: 18.4129, peers: [{peerId: jonas.id, slot: 1, path: "direct", relayId: null,
                        peerToHostPingMs: 43.4800, lossPct: 0.1, sendKbps: 15000, inputMuted: false}]})
                if (name === "c-08d-start-waiting")
                    runtime.coopEventReceived({type: "coop-peer-state", peerId: jonas.id, state: "connecting", path: null, relayId: null})
                if (name === "c-08e-start-streaming")
                    client.emit("coop.lobby.updated", {lobby: lobby("streaming", [member(mara, 0), member(jonas, 1)])})
                AppController.showOverlay("desktop-stream-menu")
                if (name === "c-08c-start-ready")
                    Qt.callLater(() => Qt.callLater(() => {
                        const start = find(window.contentItem, "streamCoopStart")
                        startFocusedByDefault = Boolean(start && start.activeFocus)
                        if (start) {
                            start.forceActiveFocus()
                            start.forceActiveFocus()
                        }
                        key = Qt.Key_Return
                    }))
            }))
        } else if (name === "c-06b-waiting-approval" || name === "c-10-declined") {
            const pending = {lobbyId: "lob_mara", from: mara.id, payload: {type: "opennow-admission", state: "pending"}}
            joinAsGuest("code", "")
            if (name === "c-10-declined")
                client.emit("coop.signal", pending)
            Qt.callLater(() => Qt.callLater(() => {
                if (name === "c-06b-waiting-approval")
                    client.emit("coop.signal", pending)
                client.emit("coop.lobby.updated", {lobby: lobby("streaming", [member(mara, 0), member(jonas, 1), member(lieke, 2)])})
                if (name === "c-10-declined")
                    client.emit("coop.lobby.kicked", {lobbyId: "lob_mara"})
            }))
        } else if (name === "c-06-waiting-room") {
            guestConnected(false)
        } else if (name === "c-06c-waiting-stream" || name === "c-06d-waiting-stream-timeout") {
            guestConnected(false)
            if (name === "c-06d-waiting-stream-timeout")
                coop.streamStartTimer.interval = 100
            const started = () => client.emit("coop.lobby.updated", {lobby: lobby("streaming", [member(mara, 0), member(jonas, 1), member(lieke, 2)])})
            steps = name === "c-06c-waiting-stream" ? ({3: started, 6: () => {
                modifiers = Qt.ControlModifier
                key = Qt.Key_G
            }}) : ({3: started})
        } else if (name === "c-05f-trying-relay") {
            joinAsGuest("code", "trying_relay")
        } else if (name === "c-07h-guest-direct") {
            coop.localDecoders = {h264: "hardware", h265: "software", av1: "none", extra: "hardware"}
            joinAsGuest("code", "")
            Qt.callLater(() => {
                runtime.coopEventReceived({type: "coop-guest-state", state: "trying_relay", path: null, relayId: null, reason: null})
                runtime.coopEventReceived({type: "coop-guest-state", state: "connected", path: "direct", relayId: null, reason: null})
                client.emit("coop.lobby.updated", {lobby: lobby("streaming", [member(mara, 0), member(jonas, 1), member(lieke, 2)])})
                runtime.coopEventReceived({type: "coop-guest-state", state: "streaming", path: "direct", relayId: null, reason: null})
                runtime.coopEventReceived({type: "coop-guest-stats", hostToGfnPingMs: 18, guestToHostPingMs: 24, totalMs: 42,
                    quality: "great", lossPct: 0, slot: 2, inputMuted: false, connectMs: 610, firstFrameMs: 380,
                    codec: "h264", decoder: "hardware"})
            })
        } else if (name === "c2-04b-lobby-invite-offline") {
            linkAs(mara)
            seedFriends(false)
            fakeStream()
            host(lobbyMembers, [invite("inv_pieter", mara, pieter, "pending", 30000, "sent"),
                invite("inv_anouk", mara, anouk, "pending", 20000, "skipped_not_member")])
            Qt.callLater(() => AppController.showOverlay("coop-lobby"))
            const chip = userId => find(window.contentItem, "coopLobbyInvite-" + userId)
            steps = ({
                4: () => {
                    const ids = coop.invitableFriends.map(friend => friend.user.id)
                    check(ids.length === 9 && ids[ids.length - 1] === joris.id && ids.indexOf(jonas.id) < 0 && ids.indexOf(pieter.id) < 0
                        && ids.slice(0, 8).every(id => ShellStore.friends.isOnline(ShellStore.friends.friendById(id))),
                        "everyone not in the lobby or invited is invitable, online first, offline last: " + JSON.stringify(ids))
                    if (!checkScreens || consoleSurface)
                        return
                    check(!chip(joris.id), "the compact strip keeps online friends first")
                    const all = find(window.contentItem, "coopLobbyAllFriends")
                    check(all && shown(all) && all.text === "All 9 friends", "the strip offers All 9 friends")
                    all.clicked()
                },
                6: () => {
                    if (checkScreens && !consoleSurface) {
                        check(chip(joris.id) && shown(chip(joris.id)) && chip(joris.id).subtitle === "Offline · gets a Discord DM",
                            "All friends shows an offline friend with the DM note")
                        chip(joris.id).clicked()
                    } else {
                        coop.inviteFriend(joris.id)
                    }
                },
                8: () => client.emit("coop.invite.updated", {invite: invite("inv_" + joris.id, mara, joris, "pending", 0, "sent")})
            })
        } else if (name === "c-08o-invite-offline-from-stream" || name === "c-08p-invite-offline-hosting") {
            linkAs(mara)
            onlyFriends = [lieke.id, pieter.id, joris.id]
            seedFriends(false)
            fakeStream()
            if (name === "c-08p-invite-offline-hosting")
                host([member(mara, 0), member(jonas, 1)], [])
            const target = name === "c-08p-invite-offline-hosting" ? joris : pieter
            const chipName = (name === "c-08p-invite-offline-hosting" ? "streamCoopInvite-" : "streamCoopCreateInvite-") + target.id
            steps = ({4: () => {
                modifiers = Qt.ControlModifier
                key = Qt.Key_G
            }, 8: () => {
                if (consoleSurface || !checkScreens) {
                    coop.inviteFriend(target.id)
                    return
                }
                const chip = find(window.contentItem, chipName)
                check(chip && shown(chip) && chip.subtitle === "Offline · gets a Discord DM", name + ": Ctrl+G lists the offline friend: "
                    + (chip ? [shown(chip), chip.subtitle] : coop.invitableFriends.map(friend => friend.user.id)))
                chip.clicked()
            }})
        } else if (name === "c2-03d-friends-invite-offline") {
            linkAs(mara)
            seedFriends(false)
            fakeStream()
            Qt.callLater(() => AppController.navigate("friends"))
            const center = item => item.mapToItem(null, item.width / 2, item.height / 2)
            const menu = () => find(window.contentItem.parent, "friendsContextMenu")
            steps = !checkScreens || consoleSurface ? ({4: () => coop.inviteFriend(pieter.id)}) : ({
                4: () => {
                    const target = find(window.contentItem, "friendsChip-" + pieter.id)
                    check(target !== null, "the offline friend is listed")
                    clickButton = Qt.RightButton
                    click = center(target)
                },
                6: () => {
                    const entry = menu() ? find(menu(), "friendsManage_invite") : null
                    check(menu() && menu().visible && menu().friend.user.id === pieter.id && menu().canInvite && entry && shown(entry),
                        "an offline friend's menu offers Invite to co-op while streaming")
                    captureName = "c2-03d-friends-invite-offline-menu"
                },
                7: () => {
                    clickButton = Qt.LeftButton
                    click = center(find(menu(), "friendsManage_invite"))
                }
            })
        } else if (name === "c2-03b-friends-context-menu") {
            linkAs(mara)
            seedFriends(true)
            AppController.navigate("friends")
            const center = item => item.mapToItem(null, item.width / 2, item.height / 2)
            const menu = () => find(window.contentItem.parent, "friendsContextMenu")
            const rightClick = objectName => {
                const target = find(window.contentItem, objectName)
                check(target !== null, objectName + " is listed")
                if (objectName.indexOf("friendsRow-") === 0)
                    target.activated.connect(() => activations += 1)
                clickButton = Qt.RightButton
                click = center(target)
            }
            steps = !checkScreens ? ({}) : ({
                2: () => rightClick("friendsRow-" + jonas.id),
                3: () => captureName = "c2-03b-right-click-row",
                4: () => {
                    check(menu() && menu().visible && menu().friend.user.id === jonas.id && activations === 0,
                        "right-click on a row opens its friend's menu without activating the row")
                    check(menu().activeFocus, "the context menu takes keyboard focus")
                    key = Qt.Key_Escape
                },
                6: () => {
                    check(!menu() || !menu().visible, "Esc closes the context menu")
                    rightClick("friendsChip-" + pieter.id)
                },
                7: () => captureName = "c2-03b-right-click-chip",
                8: () => {
                    check(menu() && menu().visible && menu().friend.user.id === pieter.id, "right-click on an offline chip opens its menu")
                    clickButton = Qt.LeftButton
                    click = Qt.point(window.width / 2, 8)
                },
                10: () => {
                    check(!menu() || !menu().visible, "a click outside closes the context menu")
                    find(window.contentItem, "friendsList").forceActiveFocus()
                    find(window.contentItem, "desktopFriendsScreen").selectedUserId = jonas.id
                    key = Qt.Key_Menu
                }
            })
        } else if (name === "c-07g-guest-shared-account") {
            guestConnected(true)
            Qt.callLater(() => Qt.callLater(() => {
                ShellStore.remoteSessions = [hostSession]
                AppController.navigate("library")
            }))
            steps = ({
                6: () => {
                    check(ShellStore.resumableSession === null, "a guest sees no resumable session, even the host's on a shared account")
                    if (checkScreens) {
                        const resume = find(window.contentItem, "desktopHeaderResume")
                        check(resume && shown(resume) && resume.text === "Back to Mara's game",
                            "the guest's Back to game returns to the co-op stream")
                    }
                    ShellStore.resumeActiveSession()
                    check(AppController.route === "stream" && ShellStore.sessionClaimRequestId === "",
                        "Back to game puts the guest back in the co-op stream without claiming a session")
                    ShellStore.conflictSession = hostSession
                    ShellStore.resolveSessionConflict("resume")
                    check(ShellStore.sessionClaimRequestId === "" && ShellStore.conflictSession === null,
                        "a guest cannot resume a conflicting session")
                    ShellStore.activeSession = hostSession
                    ShellStore.recoverySessionId = hostSession.sessionId
                    ShellStore.sessionRecoveryPending = true
                    ShellStore.acceptRecoverySessions({session: hostSession, termination: null})
                    check(ShellStore.sessionClaimRequestId === "" && !ShellStore.sessionRecoveryPending,
                        "session recovery never claims while the user is a guest")
                    ShellStore.stopStreamingSession()
                    check(ShellStore.streamStopRequestId === "", "a guest never sends session.stop")
                    ShellStore.activeSession = null
                    AppController.navigate("stream")
                    ShellStore.requestStreamExitConfirmation()
                    check(AppController.overlay === "coop-leave-confirm", "ending the stream as a guest asks to leave co-op")
                    ShellStore.launchSelectedGame(false)
                    check(ShellStore.streamCreateRequestId === "" && ShellStore.remoteSessionsRequestId === "",
                        "a guest cannot start a game")
                }
            })
        } else if (name === "c-07e-guest-peek" || name === "c-07f-guest-pinned") {
            guestConnected(true)
            const overlay = () => find(window.contentItem, "coopGuestOverlay")
            const peekTimer = () => overlay().parent.data.find(item => item.objectName === "coopGuestPeekTimer")
            const roster = extra => client.emit("coop.lobby.updated", {lobby: lobby("streaming",
                [member(mara, 0), member(jonas, 1), member(lieke, 2)].concat(extra))})
            if (name === "c-07e-guest-peek") {
                steps = ({
                    3: () => {
                        check(overlay().revealed && peekTimer().running, "the panel peeks when the guest's stream starts")
                        checkGuestInput(true, "a peeking panel")
                        peekTimer().triggered()
                    },
                    4: () => {
                        check(!overlay().revealed, "the peek hides after its 5 s timer")
                        roster([member(sam, 3)])
                    },
                    5: () => {
                        check(overlay().revealed && peekTimer().running, "a member joining brings the panel back for a peek")
                        peekTimer().triggered()
                    },
                    6: () => {
                        check(!overlay().revealed, "the join peek hides by itself")
                        roster([])
                    },
                    7: () => check(overlay().revealed, "a member leaving brings the panel back for a peek")
                })
            } else {
                steps = ({
                    3: () => {
                        modifiers = Qt.ControlModifier
                        key = Qt.Key_G
                    },
                    7: () => {
                        check(overlay().revealed && overlay().activeFocus, "Ctrl+G pins the panel with focus")
                        peekTimer().triggered()
                    }
                })
            }
        } else if (name === "c-07-guest-overlay" || name === "c-07b-guest-input") {
            guestConnected(true)
        } else if (name === "c-07c-guest-panel-input" || name === "c-07d-guest-controllers") {
            guestConnected(true)
            const ctrlG = () => {
                modifiers = Qt.ControlModifier
                key = Qt.Key_G
            }
            if (name === "c-07c-guest-panel-input") {
                steps = ({4: ctrlG, 8: () => {
                    checkGuestInput(false, "an open guest panel")
                    modifiers = 0
                    key = Qt.Key_Escape
                }})
            } else {
                steps = ({4: ctrlG, 8: () => {
                    const button = find(window.contentItem, "coopGuestOverlayControllers")
                    if (button)
                        button.clicked()
                }})
            }
        } else if (name === "c-09-settings-coop") {
            linkAs(mara)
            AppController.navigate("settings-coop")
        } else if (name === "c-10-reconnecting") {
            guestConnected(true)
            Qt.callLater(() => runtime.coopEventReceived({type: "coop-guest-state", state: "reconnecting", path: "relay", relayId: "nl", reason: null}))
        } else if (name === "c-10-media-unavailable" || name === "c-10-media-unavailable-connecting") {
            const ended = {type: "coop-guest-state", state: "ended", path: null, relayId: null,
                reason: "media-unavailable: H.264 decoder unavailable (VA-API: no hardware profile)\u0007"}
            if (name === "c-10-media-unavailable") {
                guestConnected(true)
                Qt.callLater(() => Qt.callLater(() => runtime.coopEventReceived(ended)))
            } else {
                joinAsGuest("code", "")
                Qt.callLater(() => Qt.callLater(() => runtime.coopEventReceived(ended)))
            }
        } else if (name === "c-10-host-ended" || name === "c-10-kicked" || name === "c-10-stream-lost") {
            guestConnected(true)
            Qt.callLater(() => {
                if (name === "c-10-host-ended")
                    client.emit("coop.lobby.ended", {lobbyId: "lob_mara", reason: "host_ended"})
                else if (name === "c-10-kicked")
                    client.emit("coop.lobby.kicked", {lobbyId: "lob_mara"})
                else
                    runtime.coopEventReceived({type: "coop-guest-state", state: "ended", path: null, relayId: null, reason: "host_stream_lost"})
            })
        } else if (name === "c-11-end-confirm") {
            linkAs(mara)
            fakeStream()
            host(lobbyMembers, [])
            Qt.callLater(() => coop.requestEndCoop())
        } else if (name === "c-11-leave-confirm") {
            guestConnected(true)
            Qt.callLater(() => coop.requestLeave())
        }
    }

    // A guest's gameplay input: the video item takes it while the stream runs and gives it to the shell
    // (controller navigation included) while the guest's own co-op panel has focus.
    function checkGuestInput(gameplay, what) {
        if (!checkScreens)
            return
        const video = find(window.contentItem, "streamSurfaceHost")
        check(video && video.inputEnabled === gameplay && video.activeFocus === gameplay,
            what + ": the guest's video item " + (gameplay ? "takes" : "releases") + " gameplay input")
        check(ShellStore.coop.guestPanelOpen === !gameplay, what + ": the co-op panel " + (gameplay ? "is passive" : "owns input"))
        check(ControllerInput.shellCaptureEnabled === (!gameplay && ShellStore.settings.controllerMode !== false),
            what + ": the shell " + (gameplay ? "leaves the controller to the stream" : "navigates with the controller"))
    }

    // Scene-specific state assertions on top of the visible screens.
    // Every avatar picture on screen: the DesktopAvatarPhoto items under item.
    function avatarPhotos(item, found) {
        const result = found || []
        if (!item)
            return result
        if (item.objectName === "avatarPhoto" && item.parent && item.parent.parent && item.parent.parent.url !== undefined)
            result.push(item.parent.parent)
        for (const child of item.children || [])
            avatarPhotos(child, result)
        return result
    }
    function reports(event) {
        return client.calls.filter(call => call.method === "coop.report" && call.params.event === event).map(call => call.params)
    }
    function verifyScene(name) {
        const coop = ShellStore.coop
        const friends = ShellStore.friends
        if (name === "c2-02a-link-browser")
            check(friends.linkStatus === "pending" && friends.link.loginUrl.indexOf("scope=identify") > 0, "linking waits on the identify-scope page")
        if (name === "c2-03-friends-list")
            check(friends.onlineCount === 9 && coop.isHost && coop.lobby.code === "K7Q-4MX", "friends list sees 9 online and the open lobby")
        if (name === "c2-04-lobby-invite-picker") {
            check(runtime.sent.some(item => item.type === "coop-host-start" && item.maxGuests === 3 && item.relays.length === 2),
                "the host fan-out starts with the relays")
            check(runtime.sent.some(item => item.type === "coop-peer-add" && item.peerId === jonas.id && item.slot === 1),
                "an existing guest becomes a streamer peer")
        }
        if (name === "c2-05a-invite-toast")
            check(coop.inviteToast && coop.inviteToast.id === "inv_mara" && coop.incomingInvites.length === 2, "the newest invite is toasted")
        if (name === "c2-05c-invite-connecting") {
            check(coop.role === "guest" && coop.guestPhase === "connecting" && coop.connectStage === "relay", "accepting connects through relay")
            check(runtime.sent.some(item => item.type === "coop-guest-start" && item.hostPeerId === mara.id && item.slot === 2),
                "guests start with coop-guest-start")
        }
        if (name === "c2-06b-ask-joining")
            check(client.calls.some(call => call.method === "coop.joinRequest.respond" && call.params.approve === true), "Let in approves")
        if (name === "c2-07b-invites-inbox")
            check(coop.inviteHistory.length === 2 && coop.myAsks.length === 1 && coop.myAsks[0].state === "full", "inbox history and a full lobby ask")
        if (name === "c2-10a-add-friend-sent")
            check(friends.requestResults["liekeplays"] && friends.requestResults["liekeplays"].state === "sent", "request sent")
        if (name === "c-05b-error-invalid")
            check(coop.guestPhase === "error" && coop.joinError === "invalid_code", "invalid code error")
        if (name.indexOf("c-05b-error") === 0) {
            const kind = {"c-05b-error-invalid": "invalid", "c-05b-error-full": "full", "c-05b-error-host-ended": "ended",
                "c-05b-error-cant-connect": "cant_connect", "c-05b-error-code-expired": "invalid",
                "c-05b-error-already-in-lobby": "unavailable", "c-05b-error-ask-expired": "ended"}[name]
            check(coop.joinFailure && coop.joinFailure.kind === kind, name + ": join failure is " + kind)
        }
        if (name === "c-05b-error-cant-connect") {
            check(coop.joinFailure.title === "Couldn't reach Mara", "a failed connection names the host")
            check(coop.role === "none" && client.calls.some(call => call.method === "coop.lobby.leave" && call.params.lobbyId === "lob_mara"),
                "a guest that fails after joining leaves the lobby instead of holding a dead slot")
        }
        if (name.indexOf("c-05b-error") === 0 && name !== "c-05b-error-cant-connect")
            check(!client.calls.some(call => call.method === "coop.lobby.leave"), name + ": a join that never landed has nothing to leave")
        if (name === "c-08c-start-ready" || name === "c-08d-start-waiting" || name === "c-08e-start-streaming") {
            check(coop.msText(61.8929) === "62" && coop.msText(203.417) === "203" && coop.msText(null) === "–", "co-op pings are whole milliseconds")
            check(coop.guestConnected === (name !== "c-08d-start-waiting"), name + ": a guest counts as connected only on a direct or relay link")
            if (checkScreens) {
                const start = find(window.contentItem, "streamCoopStart")
                if (name === "c-08e-start-streaming")
                    check(!start || !shown(start), "the start button goes away once co-op is streaming")
                else
                    check(start.enabled === (name === "c-08c-start-ready"), name + ": Start is enabled only with a connected guest")
                if (name === "c-08c-start-ready") {
                    check(find(window.contentItem, "coopPeerPing-" + jonas.id).text === "62 ms", "the in-stream ping is rounded")
                    check(!startFocusedByDefault, "Start does not take the panel's default focus")
                    check(client.calls.some(call => call.method === "coop.lobby.update" && call.params.state === "streaming"),
                        "Enter on Start playing together starts the lobby streaming")
                }
            }
        }
        if (name === "c-03g-peer-failed" || name === "c-08b-peer-failed") {
            check(coop.peerLink(lieke.id).kind === "failed" && coop.peerLink(jonas.id).kind === "reconnecting",
                "the streamer's peer state drives the host badge")
            if (checkScreens) {
                const prefix = name === "c-03g-peer-failed" ? "coopLobbyPeerLink-" : "coopPeerLink-"
                const label = item => item.label !== undefined ? item.label : item.text
                const failed = find(window.contentItem, prefix + lieke.id)
                const reconnecting = find(window.contentItem, prefix + jonas.id)
                check(label(failed) === "CAN'T CONNECT" && label(reconnecting) === "RECONNECTING",
                    name + ": a failed guest shows CAN'T CONNECT and a reconnecting one RECONNECTING")
                if (name === "c-03g-peer-failed")
                    check(find(window.contentItem, "coopLobbyPeerStatus-" + lieke.id).text === "Can't connect · kick to free the slot"
                        && find(window.contentItem, "coopLobbyPeerStatus-" + jonas.id).text === "Reconnecting…",
                        "the lobby card's status line names the failure at every card width")
            }
        }
        if (name === "c-05c-relay-retry")
            check(coop.guestPhase === "connecting" && runtime.sent.filter(item => item.type === "coop-guest-start").length === 2
                && client.calls.filter(call => call.method === "coop.iceServers.get").length >= 2,
                "rejected relay credentials refetch ICE servers and restart the guest once")
        if (name === "c-05c-relay-retry-failed")
            check(coop.guestPhase === "error" && coop.joinError === "cant_connect", "a second rejection shows Can't connect")
        if (name === "c-05d-relay-disabled") {
            const start = runtime.sent.find(item => item.type === "coop-guest-start")
            check(start && start.relays.length === 0 && start.iceServers.every(server => server.urls.every(url => !/^turns?:/.test(url))),
                "relay fallback off sends no TURN servers or relays")
        }
        if (name === "c-05e-join-cancelled")
            check(coop.guestPhase === "idle" && coop.role === "none"
                && client.calls.some(call => call.method === "coop.lobby.leave" && call.params.lobbyId === "lob_mara"),
                "a join that succeeds after cancelling leaves the lobby")
        if (name === "c-03c-host-session-ended")
            check(coop.role === "none" && !coop.fanoutStarted && client.calls.some(call => call.method === "coop.lobby.close"),
                "ending the host's stream closes co-op")
        if (name.indexOf("code-join") > 0) {
            const peerAdds = id => runtime.sent.filter(item => item.type === "coop-peer-add" && item.peerId === id).length
            const admission = state => client.calls.some(call => call.method === "coop.signal.send" && call.params.to === tess.id
                && call.params.payload.type === "opennow-admission" && call.params.payload.state === state)
            check(peerAdds(lieke.id) === 1, name + ": an invited friend skips approval")
            check(admission("pending"), name + ": the code guest hears it is held")
            check(!runtime.sent.some(item => item.type === "coop-signal-in" && item.peerId === tess.id),
                name + ": a held guest's signalling never reaches the streamer")
            if (name === "c-03d-code-join-held")
                check(peerAdds(tess.id) === 0 && coop.joinRequests.length === 1 && coop.joinRequests[0].kind === "code"
                    && coop.joinRequests[0].from.id === tess.id && coop.promptPending,
                    "a code join waits for the host before it gets the stream")
            if (name === "c-03e-code-join-let-in")
                check(peerAdds(tess.id) === 1 && admission("admitted") && coop.joinRequests.length === 0
                    && coop.hostNotice && coop.hostNotice.kind === "joining", "Let in adds the code guest as a streamer peer")
            if (name === "c-03e-code-join-let-in")
                check(coop.admittedIds.indexOf(jonas.id) < 0 && coop.admittedIds.indexOf(tess.id) >= 0
                    && runtime.sent.some(item => item.type === "coop-peer-remove" && item.peerId === jonas.id),
                    "a guest who leaves gives up their admission after their peer is removed")
            if (name === "c-03f-code-join-declined")
                check(peerAdds(tess.id) === 0 && coop.joinRequests.length === 0
                    && client.calls.some(call => call.method === "coop.member.kick" && call.params.userId === tess.id),
                    "Decline removes the code guest from the lobby")
        }
        if (name === "c-06b-waiting-approval") {
            const title = find(window.contentItem, "coopWaitingTitle")
            check(coop.admissionPending && coop.guestPhase === "waiting" && AppController.route === "coop-guest"
                && runtime.sent.some(item => item.type === "coop-guest-stop"),
                "a held code guest waits even after the host starts")
            check(!checkScreens || (title && title.text === "Waiting for Mara to let you in"), "the waiting room says the host must let you in")
        }
        if (name === "c-10-declined")
            check(!client.calls.some(call => call.method === "coop.lobby.leave") && coop.guestPhase === "ended" && coop.endReason === "declined" && !coop.admissionPending,
                "a held guest who is removed sees they weren't let in")
        if (name === "c-03b-lobby-expired")
            check(coop.role === "none" && coop.hostNotice && coop.hostNotice.kind === "expired"
                && runtime.sent.some(item => item.type === "coop-host-stop"), "an expired empty lobby ends hosting with a notice")
        if (name === "c2-01b-signed-out")
            check(!friends.linked && friends.connection === "unlinked" && friends.friends.length === 0,
                "a 4010 sign-out unlinks and clears friends")
        if (name === "c-06-waiting-room")
            check(coop.guestPhase === "waiting" && coop.guestPath === "relay", "connected guests wait for the host")
        if (name === "c-06c-waiting-stream") {
            check(coop.guestPhase === "starting" && AppController.route === "stream" && !coop.guestStreaming
                && coop.streamStartTimer.running, "a connected guest waits for the first frame on the stream route")
            check(AppController.overlay === "quick-settings", "Ctrl+G still opens quick settings while waiting for the stream")
            const overlay = find(window.contentItem, "coopGuestStartingOverlay")
            const video = find(window.contentItem, "streamSurfaceHost")
            check(!checkScreens || (video && shown(video) && !video.inputEnabled && overlay && shown(overlay)
                && overlay.z > video.z && overlay.parent === video.parent),
                "the video item renders (it drives the decoder) under the waiting overlay, without gameplay input")
            const title = find(overlay, "coopWaitingTitle")
            check(!checkScreens || (title && title.text === "Waiting for Mara's stream…"), "the waiting room names the host's stream")
            coop.requestLeave()
            check(AppController.overlay === "coop-leave-confirm", "Leave still works while waiting for the stream")
        }
        if (name === "c-06d-waiting-stream-timeout") {
            check(coop.guestPhase === "ended" && coop.endReason === "no_stream"
                && client.calls.some(call => call.method === "coop.lobby.leave" && call.params.lobbyId === "lob_mara")
                && runtime.sent.some(item => item.type === "coop-guest-stop"), "no first frame in time ends co-op and leaves the lobby")
            const ended = find(window.contentItem, "coopEnded")
            check(!checkScreens || ended.Accessible.name === "Mara's stream didn't start", "the ended screen says the stream didn't start")
            check(reports("first_frame").length === 0 && reports("ended").length === 1 && reports("ended")[0].reason === "no_stream",
                "a stream that never starts reports no_stream")
        }
        if (name === "c-05f-trying-relay") {
            check(coop.guestPhase === "connecting" && coop.connectStage === "trying_relay" && coop.guestPath === "", "trying a relay is not a relay path")
            const connecting = find(window.contentItem, "coopConnecting")
            check(!checkScreens || connecting.Accessible.name === "Trying a relay…", "trying a relay is a small status")
        }
        if (name === "c-07h-guest-direct") {
            check(coop.guestPhase === "playing" && coop.guestPath === "direct" && coop.guestRelayId === "", "the selected direct pair is the path")
            const chip = find(window.contentItem, "coopGuestRelayChip")
            check(!chip || !shown(chip), "a direct guest sees no relay banner")
            const video = find(window.contentItem, "coopGuestVideoValue")
            check(!checkScreens || (video.text === "H.264 · hardware decode" && Qt.colorEqual(video.color, "#FFFFFF")), "the ping card shows the codec and decoder")
        }
        if (name === "c-07-guest-overlay") {
            check(coop.guestPhase === "playing" && AppController.route === "stream", "the host's start puts guests in the stream")
            const video = find(window.contentItem, "coopGuestVideoValue")
            check(!checkScreens || (video && video.text === "H.265 · software decode" && Qt.colorEqual(video.color, "#FFD166")),
                "software decode is flagged on the ping card")
            const first = reports("first_frame")
            check(first.length === 1 && reports("ended").length === 0 && JSON.stringify(first[0]) === JSON.stringify({lobbyId: "lob_mara",
                event: "first_frame", path: "relay", relayId: "nl", codec: "h265", decoder: "software", connectMs: 840,
                firstFrameMs: 1310, reason: null}), "the first frame sends one co-op report: " + JSON.stringify(first))
        }
        if (name === "c-10-reconnecting")
            check(reports("first_frame").length === 1 && reports("ended").length === 0, "a reconnect sends no co-op report")
        if (name === "c-10-stream-lost") {
            const ended = reports("ended")
            check(reports("first_frame").length === 1 && ended.length === 1 && ended[0].reason === "stream_lost: host_stream_lost"
                && ended[0].path === "relay" && ended[0].firstFrameMs === 1310, "a dropped stream reports once with its reason: " + JSON.stringify(ended))
        }
        if (name === "c-05b-error-cant-connect") {
            const ended = reports("ended")
            check(ended.length === 1 && ended[0].reason === "cant_connect: failed" && ended[0].lobbyId === "lob_mara",
                "a failed connection is reported: " + JSON.stringify(ended))
        }
        if (name === "c-08f-create-from-stream" || name === "c-08g-created-from-stream" || name === "c-08h-invite-from-stream") {
            const creates = client.calls.filter(call => call.method === "coop.lobby.create")
            check(AppController.overlay === (consoleSurface ? "guide-session" : "desktop-stream-menu") && AppController.route === "stream"
                && ShellStore.activeSession && ShellStore.activeSession.sessionId === "coop-fixture", name + ": Ctrl+G stays over the same stream")
            check(launches === 0, name + ": co-op from a running stream never relaunches the game")
            if (name === "c-08f-create-from-stream") {
                check(creates.length === 0 && coop.role === "none", "opening Ctrl+G creates nothing by itself")
            } else {
                check(creates.length === 1 && creates[0].params.game.title === bg3.title && creates[0].params.game.appId === bg3.appId
                    && creates[0].params.maxGuests === (name === "c-08h-invite-from-stream" ? 1 : 3),
                    name + ": one lobby for the running game: " + JSON.stringify(creates.map(call => call.params)))
                check(coop.isHost && !coop.creatingLobby && runtime.sent.some(item => item.type === "coop-host-start"),
                    name + ": the live stream fans out to the lobby")
            }
            if (name === "c-08h-invite-from-stream") {
                const invites = client.calls.filter(call => call.method === "coop.lobby.invite")
                check(invitedFromStream !== "" && invites.length === 1 && invites[0].params.userId === invitedFromStream
                    && invites[0].params.lobbyId === coop.lobby.id && coop.queuedInvites.length === 0,
                    "inviting before co-op creates the lobby, then invites: " + JSON.stringify(invites.map(call => call.params)))
            }
        }
        if (name === "c-08i-codec-prompt") {
            check(coop.streamCodec === "av1" && coop.guestDecoders[lieke.id].av1 === "none" && coop.guestDecoders[lieke.id].vp9 === undefined,
                "the host keeps each guest's three decoder tiers")
            check(coop.codecPrompt && coop.codecPrompt.member.userId === lieke.id && coop.codecPrompt.best === "h265",
                "a guest who can't play AV1 gets the best codec everyone decodes in hardware: " + JSON.stringify(coop.codecPrompt))
            check(!client.calls.some(call => call.method === "session.stream.reconfigure"), "a running session never changes codec")
            if (checkScreens) {
                const body = find(window.contentItem, "coopCodecPromptBody")
                check(body.text === "Restart the game with H.265? Everyone in the lobby can play it, and the game restarts.", "the prompt offers the restart")
                const software = find(window.contentItem, "coopPeerSoftware-" + jonas.id)
                check(software && shown(software), "a guest decoding the stream in software gets a row note")
                const none = find(window.contentItem, "coopPeerSoftware-" + lieke.id)
                check(!none || !shown(none), "only software decode gets the note")
            }
            check(coop.bestCodec([lieke.id, jonas.id]) === "h265" && coop.bestCodec([]) === "av1", "the host's own decoders count")
        }
        if (name === "c-08j-codec-restart") {
            check(client.calls.some(call => call.method === "coop.lobby.close" && call.params.lobbyId === "lob_mara"), "the old lobby closes")
            check(launches === 1 && applied.some(item => item.key === "codec" && item.value === "h265"), "the game relaunches with H.265: " + JSON.stringify(applied))
            const creates = client.calls.filter(call => call.method === "coop.lobby.create")
            const invites = client.calls.filter(call => call.method === "coop.lobby.invite").map(call => call.params.userId).sort()
            check(creates.length === 1 && creates[0].params.maxGuests === 3 && JSON.stringify(invites) === JSON.stringify([jonas.id, lieke.id].sort()),
                "the relaunch re-creates the lobby and invites its players: " + JSON.stringify(invites))
            check(coop.codecRestart === null && coop.restoreCodec === "auto", "the restart is spent and the host's codec comes back after co-op")
        }
        if ((name === "c2-03c-friends-avatars" || name === "c-03h-lobby-avatars" || name === "c2-08c-settings-discord-avatar") && checkScreens) {
            const photos = avatarPhotos(window.contentItem).filter(photo => shown(photo.parent))
            const of = person => photos.filter(photo => photo.url === avatarUrls[person.id])
            const initialShown = photo => (photo.parent.children || []).some(child => child !== photo && child.visible && /^[A-Z]$/.test(String(child.text || "")))
            const own = of(mara)
            check(own.length > 0 && own.every(photo => photo.loaded && photo.source === "https://cdn.discordapp.com/avatars/100000/a1b2c3d4e5f60.png?size="
                + photo.requestSize && photo.requestSize >= photo.pixels && photo.requestSize < 2 * photo.pixels
                && (photo.requestSize & (photo.requestSize - 1)) === 0 && photo.pixels >= Math.ceil(photo.width)),
                name + ": your Discord picture shows, requested at the next power of two: " + own.map(photo => photo.source + " " + photo.pixels + " " + photo.loaded))
            if (name !== "c2-08c-settings-discord-avatar") {
                const unreachable = of(lieke)
                check(unreachable.length > 0 && unreachable.every(photo => photo.source !== "" && !photo.loaded && initialShown(photo)),
                    name + ": an unreachable picture keeps the initial")
            }
            if (name === "c2-03c-friends-avatars") {
                const foreign = of(sam)
                check(foreign.length > 0 && foreign.every(photo => photo.source === "" && !photo.loaded && initialShown(photo)),
                    "a picture outside cdn.discordapp.com is never loaded")
            }
        }
        if (name === "c-08k-codec-not-now")
            check(coop.codecPrompt === null && launches === 0 && !client.calls.some(call => call.method === "coop.lobby.close"),
                "Not now keeps the session and hides the prompt")
        if (name === "c2-11a-early-access") {
            check(friends.earlyAccessPending && friends.earlyAccessSeen.indexOf(mara.id) < 0, "a first visit shows the early-access card")
            if (checkScreens) {
                const card = find(window.contentItem, "friendsEarlyAccess")
                check(card.opened && find(window.contentItem, "friendsEarlyAccessGotIt").activeFocus, "the card takes focus on Got it")
            }
        }
        if (name === "c2-11b-early-access-dismissed")
            check(reachedLastStep, "the scene ran its last step")
        if (name === "c2-11b-early-access-dismissed" && checkScreens) {
            const pill = find(window.contentItem, "friendsEarlyAccessPill")
            check(pill && shown(pill), "the header keeps the Early access pill")
        }
        if (name === "c-08l-menu-signed-out" || name === "c-08m-menu-setting-off" || name === "c-08n-menu-setting-off-coop") {
            check(AppController.overlay === (consoleSurface ? "guide-session" : "desktop-stream-menu"), name + ": Ctrl+G opens the stream menu")
            if (!consoleSurface) {
                const section = find(window.contentItem, "streamCoopSection")
                const create = find(window.contentItem, "streamCoopCreate")
                if (name === "c-08n-menu-setting-off-coop") {
                    check(section && shown(section) && !section.inviteVisible && section.candidates.length === 0,
                        "an active co-op session keeps its controls but hides the friends list")
                } else {
                    check(!section || !shown(section), name + ": no co-op section in the stream menu")
                    check(!create || !shown(create), name + ": no Create co-op session")
                    const invite = find(window.contentItem, "streamInviteRow")
                    check(!invite || !shown(invite), name + ": no Invite a friend row")
                }
            }
        }
        if (name === "c-09b-settings-coop-stream-menu" && !consoleSurface) {
            const toggle = find(window.contentItem, "settingsCoopStreamMenu")
            toggle.valueChangedByUser(false)
            check(coop.preferences.showInStreamMenu === false, "turning the setting off stores it")
        }
        if (name === "c-02c-setup-create-failed")
            check(JSON.stringify(applied) === JSON.stringify([{key: "codec", value: "h265"}, {key: "codec", value: "auto"}])
                && coop.restoreCodec === "" && coop.role === "none", "a failed lobby create gives the host their codec back: " + JSON.stringify(applied))
        if (name === "c-02b-setup-h265")
            check(launches === 1 && applied.some(item => item.key === "codec" && item.value === "h265")
                && client.calls.some(call => call.method === "coop.lobby.create"), "the H.265 preset launches with H.265: " + JSON.stringify(applied))
        if (name === "c-07h-guest-direct") {
            const signal = client.calls.find(call => call.method === "coop.signal.send" && call.params.payload.type === "opennow-capabilities")
            check(signal && signal.params.to === mara.id && JSON.stringify(signal.params.payload.decoders) === JSON.stringify({h264: "hardware", h265: "software", av1: "none"}),
                "a joining guest tells the host its decoders: " + JSON.stringify(signal && signal.params))
            const first = reports("first_frame")
            check(first.length === 1 && first[0].path === "direct" && first[0].relayId === null && first[0].firstFrameMs === 380
                && first[0].decoder === "hardware", "a direct guest reports its path and timings: " + JSON.stringify(first))
        }
        if (name === "c2-04b-lobby-invite-offline") {
            const invites = client.calls.filter(call => call.method === "coop.lobby.invite")
            check(invites.length === 1 && invites[0].params.userId === joris.id, "an offline friend is invited from the lobby: "
                + JSON.stringify(invites.map(call => call.params)))
            const state = userId => coop.inviteChipState(coop.outgoingInviteFor(userId))
            check(state(joris.id) === "dm" && state(pieter.id) === "dm" && state(anouk.id) === "waits",
                "a DMed offline friend shows DM sent, an undeliverable DM waits: " + [state(joris.id), state(pieter.id), state(anouk.id)])
            if (checkScreens && !consoleSurface) {
                const chip = userId => find(window.contentItem, "coopLobbyInvite-" + userId)
                check(chip(joris.id).subtitle === "DM sent" && chip(anouk.id).subtitle === "Invite waits until they're online",
                    "the chips say DM sent and that the invite waits")
            }
        }
        if (name === "c-08o-invite-offline-from-stream" || name === "c-08p-invite-offline-hosting" || name === "c2-03d-friends-invite-offline") {
            const target = name === "c-08p-invite-offline-hosting" ? joris : pieter
            const creates = client.calls.filter(call => call.method === "coop.lobby.create")
            const invites = client.calls.filter(call => call.method === "coop.lobby.invite")
            check((name === "c-08p-invite-offline-hosting" || launches === 0) && creates.length === 1 && coop.isHost,
                name + ": one lobby for the running stream: " + JSON.stringify(creates.map(call => call.params)))
            check(invites.length === 1 && invites[0].params.userId === target.id && invites[0].params.lobbyId === coop.lobby.id
                && coop.queuedInvites.length === 0, name + ": the offline friend is invited: " + JSON.stringify(invites.map(call => call.params)))
        }
        if (name === "c2-03b-friends-context-menu" && checkScreens) {
            const menu = find(window.contentItem.parent, "friendsContextMenu")
            check(menu && shown(menu) && menu.friend.user.id === jonas.id && menu.activeFocus,
                "the Menu key opens the selected friend's menu with focus")
            const ask = find(menu, "friendsManage_ask")
            check(ask && shown(ask), "a friend in a joinable lobby offers Ask to join")
            check(activations === 0, "no right-click activated a row")
        }
        if (name === "c-07e-guest-peek")
            checkGuestInput(true, "a peek after a roster change")
        if (name === "c-07g-guest-shared-account")
            check(coop.guestStreaming && ShellStore.sessionClaimRequestId === "" && ShellStore.streamStopRequestId === "",
                "the guest stays in co-op and never touched the host's session")
        if (name === "c-07f-guest-pinned") {
            check(find(window.contentItem, "coopGuestOverlay").revealed, "a pinned panel is not auto-hidden")
            checkGuestInput(false, "a pinned panel")
        }
        if (name === "c-07b-guest-input" || name === "c-07c-guest-panel-input")
            checkGuestInput(true, name === "c-07b-guest-input" ? "a playing guest" : "a closed guest panel")
        if (name === "c-07d-guest-controllers") {
            check(coop.guestPhase === "playing", "the guest is still playing")
            checkGuestInput(false, "the guest's controllers page")
        }
        if (name === "c-10-media-unavailable" || name === "c-10-media-unavailable-connecting") {
            check(coop.guestPhase === "ended" && coop.endReason === "media_unavailable"
                && coop.endDetail === "H.264 decoder unavailable (VA-API: no hardware profile)",
                name + ": a decode failure ends with its own reason and a plain-text detail")
            check(client.calls.some(call => call.method === "coop.lobby.leave" && call.params.lobbyId === "lob_mara")
                && runtime.sent.some(item => item.type === "coop-guest-stop"), name + ": the guest leaves the lobby and stops")
        }
        if (name === "c-10-stream-lost")
            check(client.calls.some(call => call.method === "coop.lobby.leave" && call.params.lobbyId === "lob_mara"),
                "a guest whose stream drops leaves the lobby instead of staying a member")
        if (name === "c-10-kicked" || name === "c-10-host-ended")
            check(!client.calls.some(call => call.method === "coop.lobby.leave"), name + ": a guest the server already removed sends no leave")
        if (name === "c-10-media-unavailable" || name === "c-10-media-unavailable-connecting") {
            if (checkScreens) {
                const ended = find(window.contentItem, "coopEnded")
                check(ended.Accessible.name === "This device can't play Mara's stream", name + ": the ended screen says the device can't play it")
            }
        }
        if (name === "c-10-kicked")
            check(coop.guestPhase === "ended" && coop.endReason === "kicked" && runtime.sent.some(item => item.type === "coop-guest-stop"),
                "kicked guests stop their pipeline")
    }

    function run(parent) {
        root = parent
        window = parent.Window.window
        const coop = ShellStore.coop
        const friends = ShellStore.friends
        friends.coreClient = client
        friends.ready = true
        coop.coreClient = client
        coop.ready = true
        coop.streamRuntime = runtime
        coop.sendNativeCommand = (type, params) => runtime.command(type, params)
        coop.launchGame = gameValue => { launches += 1 }
        coop.applySetting = (keyName, value) => { applied = applied.concat([{key: keyName, value: value}]) }
        return true
    }

    // 0 = continue, 1 = passed, 2 = failed.
    function advance() {
        try {
            if (index < 0 || tick >= settleTicks) {
                if (index >= 0 && !verified) {
                    const name = walk[index]
                    if (name.indexOf("c2-03") === 0 || name.indexOf("c2-04") === 0 || name.indexOf("c2-07a") === 0
                            || name.indexOf("c2-06") === 0 || name.indexOf("c2-09") === 0 || name.indexOf("c-03") === 0
                            || name.indexOf("c-07") === 0 || name.indexOf("c-08") === 0 || name === "c-10-reconnecting"
                            || name.indexOf("c-11") === 0)
                        showFrame()
                    for (const objectName of checkScreens ? expectations[name] || [] : []) {
                        const item = find(window.contentItem, objectName)
                        check(item !== null, name + ": " + objectName + " is missing")
                        check(shown(item), name + ": " + objectName + " is not on screen")
                    }
                    verifyScene(name)
                    verified = true
                    if (scene === "") {
                        captureName = name
                        return 0
                    }
                }
                verified = false
                index += 1
                tick = 0
                if (index >= walk.length)
                    return 1
                reset()
                setupScene(walk[index])
                return 0
            }
            tick += 1
            if (steps[tick])
                steps[tick]()
            return 0
        } catch (error) {
            failure = String(error && error.message || error)
            console.error("Co-op acceptance: " + failure)
            return 2
        }
    }
}
