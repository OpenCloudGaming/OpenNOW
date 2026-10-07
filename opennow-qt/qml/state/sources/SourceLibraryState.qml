import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property var sources

    readonly property int pageLimit: 40
    readonly property int maximumQueryBytes: 512

    property string sourceId: ""
    property string mode: ""
    property string query: ""
    property var items: []
    property var nextCursor: null
    property string coverage: ""
    property string error: ""
    property real generation: -1
    property string pageRequestId: ""
    property bool appending: false
    property string scopeKey: ""

    property var details: null
    property string detailsError: ""
    property string detailsRequestId: ""
    property var launchDecision: null
    property string inspectRequestId: ""
    property bool playRequested: false

    signal playReady(var details, var decision)

    readonly property bool loading: pageRequestId !== ""
    readonly property var source: {
        const value = sources.sourceById(sourceId)
        return sources.live(value) ? value : null
    }
    readonly property bool signedIn: sources.signedIn(sourceId)
    readonly property bool needsSignIn: source !== null && !signedIn && !sources.has(source, "catalog.public.v2")
        && sources.authKinds(sourceId).length > 0
    readonly property string desiredMode: source === null ? ""
        : signedIn && sources.has(source, "catalog.library.v2") ? "library"
        : sources.has(source, "catalog.public.v2") ? "public" : ""
    readonly property string desiredScopeKey: desiredMode === "library"
        ? JSON.stringify(sources.accountScope(sourceId)) : desiredMode
    readonly property string targetSourceId: sources.selectedIsGfn ? "" : sources.selectedSourceId

    onTargetSourceIdChanged: Qt.callLater(sync)
    onSourceChanged: if (source === null && sourceId !== "") clearContent()
    onDesiredScopeKeyChanged: Qt.callLater(sync)

    function sync() {
        if (sourceId !== targetSourceId) {
            clear()
            sourceId = targetSourceId
        }
        if (sourceId === "" || desiredMode === "") {
            cancelPage()
            items = []
            nextCursor = null
            mode = ""
            return
        }
        if (mode !== desiredMode || scopeKey !== desiredScopeKey)
            load(false)
    }

    function clear() {
        clearContent()
        sourceId = ""
        query = ""
    }

    function clearContent() {
        cancelPage()
        cancelDetails()
        mode = ""
        items = []
        nextCursor = null
        coverage = ""
        error = ""
        generation = -1
        scopeKey = ""
    }

    function cancelPage() {
        const id = pageRequestId
        pageRequestId = ""
        appending = false
        if (id !== "")
            coreClient.cancel(id)
    }

    function cancelDetails() {
        const detailsId = detailsRequestId
        const inspectId = inspectRequestId
        detailsRequestId = ""
        inspectRequestId = ""
        details = null
        detailsError = ""
        launchDecision = null
        playRequested = false
        if (detailsId !== "")
            coreClient.cancel(detailsId)
        if (inspectId !== "")
            coreClient.cancel(inspectId)
    }

    function scope() {
        const account = mode === "library" ? sources.accountScope(sourceId) : null
        return account ? {kind: "account", scope: account} : {kind: "public"}
    }

    function utf8Prefix(value, maximumBytes) {
        let bytes = 0
        let end = 0
        for (const character of String(value || "")) {
            const point = character.codePointAt(0)
            const width = point < 0x80 ? 1 : point < 0x800 ? 2 : point < 0x10000 ? 3 : 4
            if (bytes + width > maximumBytes)
                break
            bytes += width
            end += character.length
        }
        return String(value || "").slice(0, end)
    }

    function load(append) {
        cancelPage()
        if (!append) {
            mode = desiredMode
            scopeKey = desiredScopeKey
            items = []
            nextCursor = null
            coverage = ""
            generation = -1
        }
        error = ""
        appending = append
        pageRequestId = coreClient.request(mode === "library" ? "sources.library.page" : "sources.public.page", {
            sourceId: sourceId,
            request: {scope: scope(), query: {query: query, cursor: append ? nextCursor : null, limit: pageLimit}}
        }, 30000)
    }

    function search(text) {
        if (sourceId === "" || mode === "")
            return
        query = utf8Prefix(String(text || "").trim(), maximumQueryBytes)
        load(false)
    }

    function loadMore() {
        if (sourceId !== "" && !loading && nextCursor !== null)
            load(true)
    }

    function viewItem(summary) {
        return {
            ref: {sourceId: sourceId, localId: summary.id},
            title: summary.title,
            imageUrl: typeof summary.artwork === "string" && summary.artwork.indexOf("https://") === 0 ? summary.artwork : "",
            subtitle: typeof summary.subtitle === "string" ? summary.subtitle : "",
            badges: Array.isArray(summary.badges) ? summary.badges.filter(badge => typeof badge === "string") : [],
            availability: typeof summary.availability === "string" ? summary.availability : "unknown"
        }
    }

    function openDetails(localId) {
        cancelDetails()
        if (sourceId === "" || !sources.has(source, "catalog.details.v2"))
            return false
        detailsRequestId = coreClient.request("sources.game.get", {
            sourceId: sourceId, request: {scope: scope(), game: localId}
        }, 30000)
        return detailsRequestId !== ""
    }

    function closeDetails() {
        cancelDetails()
    }

    function playbackSupported() {
        return source !== null && source.playback !== null && source.playback !== undefined
            && sources.has(source, "launch.v2") && sources.has(source, "sessions.v2")
    }

    function defaultVariant() {
        return details ? details.variants.find(variant => variant.availability === "available") || null : null
    }

    function play() {
        const variant = defaultVariant()
        if (!variant || playRequested)
            return false
        playRequested = inspect(variant.id)
        return playRequested
    }

    function inspect(variant) {
        if (!details || !playbackSupported() || inspectRequestId !== "")
            return false
        launchDecision = null
        inspectRequestId = coreClient.request("sources.launch.inspect", {
            sourceId: sourceId,
            request: {scope: sources.accountScope(sourceId), target: {game: details.id, variant: variant},
                catalogRevision: details.revision}
        }, 30000)
        return inspectRequestId !== ""
    }

    function acceptPage(result) {
        const append = appending
        pageRequestId = ""
        appending = false
        const body = result ? result.result : null
        if (!result || result.sourceId !== sourceId || !sources.validGeneration(result.generation)
                || !body || !Array.isArray(body.items)) {
            error = qsTr("The service returned an invalid page.")
            return
        }
        if (append && result.generation !== generation) {
            load(false)
            return
        }
        const merged = append ? items.slice() : []
        const seen = new Set(merged.map(item => item.ref.localId))
        for (const summary of body.items) {
            if (!summary || typeof summary.id !== "string" || summary.id === "" || typeof summary.title !== "string"
                    || summary.title === "" || seen.has(summary.id))
                continue
            seen.add(summary.id)
            merged.push(viewItem(summary))
        }
        items = merged
        generation = result.generation
        nextCursor = typeof body.nextCursor === "string" && body.nextCursor !== "" ? body.nextCursor : null
        coverage = ["partial", "complete", "unknown"].indexOf(body.coverage) >= 0 ? body.coverage : "unknown"
    }

    function acceptResponse(id, result) {
        if (id === "")
            return false
        if (id === pageRequestId) {
            acceptPage(result)
            return true
        }
        if (id === detailsRequestId) {
            detailsRequestId = ""
            const body = result ? result.result : null
            if (result && result.sourceId === sourceId && body && body.game && Array.isArray(body.variants))
                details = {
                    game: viewItem(body.game),
                    id: body.game.id,
                    description: typeof body.description === "string" ? body.description : "",
                    variants: body.variants.filter(variant => variant && typeof variant.id === "string"),
                    revision: body.revision
                }
            else
                detailsError = qsTr("The service returned invalid game details.")
            return true
        }
        if (id === inspectRequestId) {
            inspectRequestId = ""
            const body = result ? result.result : null
            launchDecision = result && result.sourceId === sourceId && body && typeof body.state === "string" ? body : null
            const wanted = playRequested
            playRequested = false
            if (wanted && launchDecision && launchDecision.state === "ready")
                playReady(details, launchDecision)
            return true
        }
        return false
    }

    function acceptFailure(id, code, message) {
        if (id === "")
            return false
        const text = String(message || "")
        if (id === pageRequestId) {
            pageRequestId = ""
            appending = false
            if (code === "stale_source" || code === "scope_changed") {
                sources.refreshAuth(sourceId)
                scopeKey = ""
            } else if (code !== "cancelled") {
                error = text
            }
            return true
        }
        if (id === detailsRequestId) {
            detailsRequestId = ""
            if (code !== "cancelled")
                detailsError = text
            return true
        }
        if (id === inspectRequestId) {
            inspectRequestId = ""
            playRequested = false
            if (code !== "cancelled")
                launchDecision = {state: "blocked", reason: "unknown", message: text}
            return true
        }
        return false
    }
}
