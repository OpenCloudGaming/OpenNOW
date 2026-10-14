import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property var bridge
    required property bool ready
    required property bool available

    readonly property string gfnId: "org.opennow.geforce-now"
    readonly property int minimumPollMs: 250
    readonly property int maximumPollMs: 3600000

    property var sources: []
    property real generation: -1
    property string selectedSourceId: ""
    property bool loaded: false
    property string error: ""
    property string listRequestId: ""
    property bool refreshQueued: false
    property string selectRequestId: ""

    property var authStates: ({})
    property var authErrors: ({})
    property var sourceGenerations: ({})
    property var accounts: ({})
    property var settingsViews: ({})
    property var settingsErrors: ({})
    property var streamSettingsViews: ({})
    property var streamSettingsErrors: ({})
    property var pending: ({})

    property string authSourceId: ""
    property string authAttempt: ""
    property bool authBusy: false
    property string browserOpenedHandle: ""
    property string browserOpenedAttempt: ""

    readonly property var browserChallenge: {
        const state = authSourceId !== "" ? authStates[authSourceId] : null
        return state && state.state === "pending" && state.challenge && state.challenge.kind === "browser"
            ? state.challenge : null
    }
    readonly property bool canOpenBrowser: browserChallenge !== null && bridge !== null
        && typeof browserChallenge.openHandle === "string" && browserChallenge.openHandle !== ""
        && browserChallenge.openHandle !== browserOpenedHandle

    property Connections bridgeConnections: Connections {
        target: root.bridge
        function onAuthorizationOpenFailed(sourceId, message) {
            root.authErrors = root.withEntry(root.authErrors, sourceId,
                qsTr("OpenNOW could not open the sign-in page in your browser."))
        }
    }

    readonly property var selectedSource: sourceById(selectedSourceId)
    readonly property bool selectedIsGfn: selectedSourceId === "" || selectedSourceId === gfnId
    readonly property bool gfnEnabled: loaded && sources.some(source => source.id === gfnId && source.enabled === true)
    readonly property var playableSources: sources.filter(source => source.enabled === true && source.state === "ready")

    property Timer pollTimer: Timer {
        repeat: false
        onTriggered: root.pollAuth()
    }

    onReadyChanged: if (!ready) reset()
    onAvailableChanged: available ? refresh() : reset()

    function reset() {
        pollTimer.stop()
        sources = []
        generation = -1
        selectedSourceId = ""
        loaded = false
        error = ""
        listRequestId = ""
        refreshQueued = false
        selectRequestId = ""
        authStates = ({})
        authErrors = ({})
        sourceGenerations = ({})
        accounts = ({})
        settingsViews = ({})
        settingsErrors = ({})
        streamSettingsViews = ({})
        streamSettingsErrors = ({})
        pending = ({})
        authSourceId = ""
        authAttempt = ""
        authBusy = false
        browserOpenedHandle = ""
        browserOpenedAttempt = ""
    }

    function validGeneration(value) {
        return Number.isSafeInteger(value) && value >= 0
    }

    function sourceById(id) {
        const value = String(id || "")
        return value === "" ? null : sources.find(source => source.id === value) || null
    }

    function has(source, capability) {
        return source !== null && (source.providerCapabilities || []).indexOf(capability) >= 0
    }

    function authState(id) {
        return authStates[id] || null
    }

    function signedIn(id) {
        const state = authState(id)
        return state !== null && (state.state === "signed-in" || state.state === "not-required")
    }

    function accountScope(id) {
        const state = authState(id)
        return state && state.state === "signed-in" && state.account
            ? {account: state.account.key, revision: state.revision} : null
    }

    function withEntry(map, key, value) {
        const next = Object.assign({}, map)
        if (value === undefined)
            delete next[key]
        else
            next[key] = value
        return next
    }

    function call(method, sourceId, request, purpose, timeoutMs) {
        if (!ready || !available)
            return ""
        const id = coreClient.request(method, {sourceId: sourceId, request: request}, timeoutMs || 30000)
        if (id !== "")
            pending = withEntry(pending, id, {sourceId: sourceId, purpose: purpose})
        return id
    }

    function refresh() {
        if (!ready || !available)
            return
        if (listRequestId !== "") {
            refreshQueued = true
            return
        }
        refreshQueued = false
        listRequestId = coreClient.request("sources.list", {})
    }

    function select(id) {
        const source = sourceById(id)
        if (!ready || !available || source === null || playableSources.indexOf(source) < 0 || selectRequestId !== "")
            return false
        selectRequestId = coreClient.request("sources.select", {sourceId: source.id})
        return selectRequestId !== ""
    }

    function live(source) {
        return source !== null && source.enabled === true && source.state === "ready"
    }

    function evictSource(id) {
        for (const name of ["authStates", "authErrors", "sourceGenerations", "accounts", "settingsViews",
                "settingsErrors", "streamSettingsViews", "streamSettingsErrors"])
            root[name] = withEntry(root[name], id, undefined)
        const stale = Object.keys(pending).filter(requestId => pending[requestId].sourceId === id)
        if (stale.length > 0) {
            const next = Object.assign({}, pending)
            for (const requestId of stale)
                delete next[requestId]
            pending = next
            for (const requestId of stale)
                coreClient.cancel(requestId)
        }
        if (authSourceId === id) {
            pollTimer.stop()
            authSourceId = ""
            authAttempt = ""
            authBusy = false
            browserOpenedHandle = ""
            browserOpenedAttempt = ""
        }
    }

    function refreshAuth(id) {
        const source = sourceById(id)
        if (source === null || source.id === gfnId || !live(source))
            return
        call("sources.auth.state", source.id, {}, "auth")
        if (has(source, "accounts.v2"))
            call("sources.accounts.list", source.id, {}, "accounts")
    }

    function refreshAllAuth() {
        for (const source of sources)
            refreshAuth(source.id)
    }

    function authKinds(id) {
        const source = sourceById(id)
        return source ? (source.authKinds || []).filter(kind => kind !== "anonymous") : []
    }

    function startSignIn(id, kind) {
        const source = sourceById(id)
        if (source === null || authBusy || authKinds(id).indexOf(kind) < 0)
            return false
        cancelSignIn()
        authErrors = withEntry(authErrors, source.id, undefined)
        authSourceId = source.id
        authBusy = true
        return call("sources.auth.start", source.id, {authority: null, kind: kind, remember: true}, "auth") !== ""
    }

    function cancelSignIn() {
        pollTimer.stop()
        browserOpenedHandle = ""
        browserOpenedAttempt = ""
        const id = authSourceId
        const attempt = authAttempt
        authSourceId = ""
        authAttempt = ""
        authBusy = false
        if (id !== "" && attempt !== "")
            call("sources.auth.cancel", id, {attempt: attempt}, "cancel")
        if (id !== "")
            call("sources.auth.state", id, {}, "auth")
    }

    function openBrowser() {
        if (!canOpenBrowser)
            return false
        const handle = browserChallenge.openHandle
        if (!bridge.openAuthorization(authSourceId, handle))
            return false
        browserOpenedHandle = handle
        browserOpenedAttempt = String(browserChallenge.attempt || "")
        authErrors = withEntry(authErrors, authSourceId, undefined)
        return true
    }

    function pollAuth() {
        if (authSourceId === "" || authAttempt === "")
            return
        authBusy = true
        call("sources.auth.poll", authSourceId, {attempt: authAttempt}, "auth")
    }

    function signOut(id) {
        const state = authState(id)
        if (!state || state.state !== "signed-in")
            return false
        return call("sources.auth.logout", id, state.account.key, "auth") !== ""
    }

    function selectAccount(id, key) {
        return call("sources.accounts.select", id, {account: key, pin: null}, "auth") !== ""
    }

    function removeAccount(id, key) {
        return call("sources.accounts.remove", id, key, "accounts") !== ""
    }

    function streamSettingsSupported(source) {
        return source !== null && source.id !== gfnId && source.playback === "media-worker-v1"
    }

    function loadSettings(id) {
        const source = sourceById(id)
        if (!has(source, "settings.v2") || !live(source))
            return false
        const scope = {account: accountScope(source.id)}
        call("sources.providerSettings.get", source.id, scope, "settings")
        if (streamSettingsSupported(source))
            call("sources.settings.get", source.id, scope, "streamSettings")
        return true
    }

    function setSetting(id, key, value, domain) {
        const stream = domain === "stream"
        const view = (stream ? streamSettingsViews : settingsViews)[id]
        if (!view)
            return false
        if (stream)
            streamSettingsErrors = withEntry(streamSettingsErrors, id, undefined)
        else
            settingsErrors = withEntry(settingsErrors, id, undefined)
        return call(stream ? "sources.settings.set" : "sources.providerSettings.set", id, {
            scope: {account: accountScope(id)},
            expectedRevision: view.revision,
            key: key,
            value: value
        }, stream ? "streamSettings" : "settings") !== ""
    }

    function acceptSnapshot(result) {
        if (!result || !Array.isArray(result.sources) || !validGeneration(result.generation))
            return false
        if (result.generation < generation)
            return true
        generation = result.generation
        sources = result.sources.filter(source => source && typeof source.id === "string" && source.id !== ""
            && source.protocolVersion === 2)
        selectedSourceId = typeof result.selectedSourceId === "string" ? result.selectedSourceId : ""
        loaded = true
        const known = new Set(Object.keys(sourceGenerations).concat(Object.keys(authStates), Object.keys(authErrors),
            Object.keys(accounts), Object.keys(settingsViews), Object.keys(streamSettingsViews),
            Object.keys(settingsErrors), Object.keys(streamSettingsErrors),
            Object.keys(pending).map(requestId => pending[requestId].sourceId)))
        if (authSourceId !== "")
            known.add(authSourceId)
        for (const id of known) {
            if (!live(sourceById(id)))
                evictSource(id)
        }
        refreshAllAuth()
        return true
    }

    function acceptAuth(id, state) {
        if (!state || typeof state.state !== "string")
            return
        authStates = withEntry(authStates, id, state)
        if (id !== authSourceId)
            return
        if (state.state === "pending" && state.challenge && typeof state.challenge.attempt === "string") {
            authAttempt = state.challenge.attempt
            authBusy = false
            const delay = Number(state.challenge.pollAfterMs)
            pollTimer.interval = Math.max(minimumPollMs, Math.min(maximumPollMs, Number.isFinite(delay) ? delay : minimumPollMs))
            pollTimer.restart()
            if (state.challenge.kind === "browser" && browserOpenedAttempt !== state.challenge.attempt)
                Qt.callLater(root.openBrowser)
        } else if (state.state === "authorized" && typeof state.attempt === "string") {
            pollTimer.stop()
            authAttempt = state.attempt
            authBusy = true
            call("sources.auth.complete", id, {attempt: state.attempt, proof: null}, "auth")
        } else {
            pollTimer.stop()
            authSourceId = ""
            authAttempt = ""
            authBusy = false
            if (state.state === "signed-in") {
                if (has(sourceById(id), "accounts.v2"))
                    call("sources.accounts.list", id, {}, "accounts")
                loadSettings(id)
            }
        }
    }

    function acceptResponse(requestId, result) {
        if (requestId === "")
            return false
        if (requestId === listRequestId) {
            listRequestId = ""
            if (!acceptSnapshot(result))
                error = qsTr("OpenNOW could not read the list of services.")
            if (refreshQueued)
                refresh()
            return true
        }
        if (requestId === selectRequestId) {
            selectRequestId = ""
            if (!acceptSnapshot(result))
                refresh()
            return true
        }
        const request = pending[requestId]
        if (!request)
            return false
        pending = withEntry(pending, requestId, undefined)
        if (!result || result.sourceId !== request.sourceId || !validGeneration(result.generation))
            return true
        const known = sourceGenerations[request.sourceId]
        if (known !== undefined && result.generation < known)
            return true
        sourceGenerations = withEntry(sourceGenerations, request.sourceId, result.generation)
        const body = result.result
        if (request.purpose === "auth") {
            acceptAuth(request.sourceId, body)
        } else if (request.purpose === "accounts" && body && Array.isArray(body.accounts)) {
            accounts = withEntry(accounts, request.sourceId, body)
        } else if (request.purpose === "settings" && body && Array.isArray(body.settings)) {
            settingsViews = withEntry(settingsViews, request.sourceId, body)
        } else if (request.purpose === "streamSettings" && body && Array.isArray(body.settings)) {
            streamSettingsViews = withEntry(streamSettingsViews, request.sourceId, body)
        }
        return true
    }

    function acceptFailure(requestId, code, message) {
        if (requestId === "")
            return false
        const text = String(message || "")
        if (requestId === listRequestId) {
            listRequestId = ""
            error = text
            if (refreshQueued)
                refresh()
            return true
        }
        if (requestId === selectRequestId) {
            selectRequestId = ""
            error = text
            refresh()
            return true
        }
        const request = pending[requestId]
        if (!request)
            return false
        pending = withEntry(pending, requestId, undefined)
        if (code === "cancelled")
            return true
        if (request.purpose === "auth" && request.sourceId === authSourceId) {
            pollTimer.stop()
            authSourceId = ""
            authAttempt = ""
            authBusy = false
            authErrors = withEntry(authErrors, request.sourceId, text)
            call("sources.auth.state", request.sourceId, {}, "auth")
        } else if (request.purpose === "settings" || request.purpose === "streamSettings") {
            const stream = request.purpose === "streamSettings"
            if (code !== "unsupported_feature") {
                if (stream)
                    streamSettingsErrors = withEntry(streamSettingsErrors, request.sourceId, text)
                else
                    settingsErrors = withEntry(settingsErrors, request.sourceId, text)
            }
            if (code === "stale_source" || code === "stale_settings")
                loadSettings(request.sourceId)
        } else if (code === "stale_source" || code === "scope_changed") {
            refreshAuth(request.sourceId)
        }
        return true
    }

    function acceptChanged(payload) {
        const id = payload && typeof payload.sourceId === "string" ? payload.sourceId : ""
        refresh()
        if (id !== "" && id !== authSourceId)
            refreshAuth(id)
    }
}
