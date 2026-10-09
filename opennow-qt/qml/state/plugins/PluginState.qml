import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property bool ready
    required property bool available
    required property bool catalogAvailable

    readonly property int previewLimit: 20
    readonly property int maximumQueryBytes: 512

    property var plugins: []
    property real generation: -1
    property bool loaded: false
    property string error: ""
    property string busyId: ""
    property string listRequestId: ""
    property bool refreshQueued: false
    property string mutationRequestId: ""

    property var inspection: null
    property string inspectRequestId: ""
    property string commitRequestId: ""
    property string installError: ""

    property string previewSourceId: ""
    property string previewQuery: ""
    property var previewItems: []
    property var previewNextCursor: null
    property string previewCoverage: ""
    readonly property string previewSummary: {
        const count = previewItems.length
        if (count === 0)
            return qsTr("No titles match.")
        if (previewCoverage === "complete" && previewNextCursor === null)
            return count === 1 ? qsTr("Showing 1 title.") : qsTr("Showing all %1 titles.").arg(count)
        return count === 1 ? qsTr("Showing 1 title. The plugin may have more.")
            : qsTr("Showing %1 titles. The plugin may have more.").arg(count)
    }
    property string previewError: ""
    property real previewGeneration: -1
    property real previewRegistryGeneration: -1
    property string previewRequestId: ""
    property bool previewAppending: false
    property bool previewWaiting: false
    readonly property bool previewLoading: previewRequestId !== ""
    readonly property var previewPlugin: pluginById(previewSourceId)

    readonly property bool loading: listRequestId !== "" && !loaded
    readonly property bool inspecting: inspectRequestId !== ""
    readonly property bool committing: commitRequestId !== ""

    onReadyChanged: if (!ready) reset()
    onAvailableChanged: available ? refresh() : reset()

    function reset() {
        closePreview()
        plugins = []
        generation = -1
        loaded = false
        error = ""
        busyId = ""
        listRequestId = ""
        refreshQueued = false
        mutationRequestId = ""
        inspection = null
        inspectRequestId = ""
        commitRequestId = ""
        installError = ""
    }

    function validGeneration(value) {
        return Number.isSafeInteger(value) && value >= 0
    }

    function pluginById(id) {
        const value = String(id || "")
        return value === "" ? null : plugins.find(plugin => plugin.id === value) || null
    }

    function catalogReady(plugin) {
        return plugin !== null && plugin.enabled === true && plugin.state === "ready"
            && (plugin.capabilities || []).indexOf("catalog.v1") >= 0
    }

    function refresh() {
        if (!ready || !available)
            return
        if (listRequestId !== "") {
            refreshQueued = true
            return
        }
        refreshQueued = false
        listRequestId = coreClient.request("plugins.list", {})
    }

    function setEnabled(id, enabled) {
        const plugin = pluginById(id)
        if (!ready || !available || mutationRequestId !== "" || plugin === null || plugin.required === true)
            return false
        error = ""
        busyId = plugin.id
        mutationRequestId = coreClient.request("plugins.setEnabled",
            {id: plugin.id, enabled: enabled === true, expectedGeneration: generation}, 30000)
        if (mutationRequestId === "")
            busyId = ""
        return mutationRequestId !== ""
    }

    function uninstall(id) {
        const plugin = pluginById(id)
        if (!ready || !available || mutationRequestId !== "" || plugin === null
                || plugin.builtin === true || plugin.required === true)
            return false
        error = ""
        busyId = plugin.id
        if (previewSourceId === plugin.id)
            closePreview()
        mutationRequestId = coreClient.request("plugins.uninstall",
            {id: plugin.id, expectedGeneration: generation, confirmed: true}, 30000)
        if (mutationRequestId === "")
            busyId = ""
        return mutationRequestId !== ""
    }

    function inspectPackage(fileUrl) {
        const path = String(fileUrl || "")
        if (!ready || !available || path === "" || committing)
            return false
        cancelInstall()
        installError = ""
        inspectRequestId = coreClient.request("plugins.install.inspect", {path: path}, 60000)
        return inspectRequestId !== ""
    }

    function commitInstall() {
        if (!ready || !available || inspection === null || committing)
            return false
        installError = ""
        commitRequestId = coreClient.request("plugins.install.commit",
            {token: inspection.token, expectedGeneration: inspection.generation, consent: true}, 60000)
        return commitRequestId !== ""
    }

    function cancelInstall() {
        const pendingInspect = inspectRequestId
        inspectRequestId = ""
        if (pendingInspect !== "")
            coreClient.cancel(pendingInspect)
        const staged = inspection
        inspection = null
        if (staged !== null && !committing && ready)
            coreClient.request("plugins.install.cancel", {token: staged.token})
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

    function openPreview(id) {
        const plugin = pluginById(id)
        if (!catalogAvailable || !catalogReady(plugin))
            return false
        closePreview()
        previewSourceId = plugin.id
        loadPreview(false)
        return true
    }

    function searchPreview(query) {
        if (previewSourceId === "")
            return
        previewQuery = utf8Prefix(String(query || "").trim(), maximumQueryBytes)
        loadPreview(false)
    }

    function loadMorePreview() {
        if (previewSourceId === "" || previewLoading || previewNextCursor === null)
            return
        loadPreview(true)
    }

    function cancelPreviewRequest() {
        const pending = previewRequestId
        previewRequestId = ""
        if (pending !== "")
            coreClient.cancel(pending)
    }

    function closePreview() {
        cancelPreviewRequest()
        previewSourceId = ""
        previewQuery = ""
        previewItems = []
        previewNextCursor = null
        previewCoverage = ""
        previewError = ""
        previewGeneration = -1
        previewRegistryGeneration = -1
        previewAppending = false
        previewWaiting = false
    }

    function syncPreview() {
        if (previewSourceId === "")
            return
        const plugin = pluginById(previewSourceId)
        if (plugin === null || plugin.enabled !== true || plugin.state === "failed"
                || (plugin.capabilities || []).indexOf("catalog.v1") < 0) {
            closePreview()
            return
        }
        if (plugin.state !== "ready") {
            cancelPreviewRequest()
            previewAppending = false
            previewError = ""
            previewWaiting = true
            return
        }
        if (previewWaiting) {
            previewWaiting = false
            loadPreview(false)
        }
    }

    function loadPreview(append) {
        cancelPreviewRequest()
        if (!append) {
            previewItems = []
            previewNextCursor = null
            previewCoverage = ""
            previewGeneration = -1
        }
        previewError = ""
        previewAppending = append
        previewRegistryGeneration = generation
        previewRequestId = coreClient.request("sources.catalog.page", {
            sourceId: previewSourceId,
            query: previewQuery,
            cursor: append ? previewNextCursor : null,
            limit: previewLimit
        }, 30000)
    }

    function acceptSnapshot(result) {
        if (!result || !Array.isArray(result.plugins) || !validGeneration(result.generation))
            return false
        if (result.generation < generation)
            return true
        generation = result.generation
        plugins = result.plugins.filter(plugin => plugin && typeof plugin.id === "string" && plugin.id !== "")
        loaded = true
        syncPreview()
        if (inspection !== null && inspection.generation !== generation && !committing)
            inspection = Object.assign({}, inspection, {stale: true})
        return true
    }

    function acceptPreviewPage(result) {
        const append = previewAppending
        previewRequestId = ""
        previewAppending = false
        if (previewRegistryGeneration !== generation) {
            syncPreview()
            if (previewSourceId !== "" && !previewWaiting)
                loadPreview(false)
            return
        }
        if (!result || result.sourceId !== previewSourceId || !validGeneration(result.generation)) {
            previewError = qsTr("The plugin returned an invalid catalog page.")
            return
        }
        if (append && result.generation !== previewGeneration) {
            loadPreview(false)
            return
        }
        const existing = append ? previewItems.slice() : []
        const seen = new Set(existing.map(item => item.localId))
        for (const item of Array.isArray(result.items) ? result.items : []) {
            if (!item || !item.id || item.id.sourceId !== previewSourceId
                    || typeof item.id.localId !== "string" || item.id.localId === ""
                    || typeof item.title !== "string" || item.title === "" || seen.has(item.id.localId))
                continue
            seen.add(item.id.localId)
            existing.push({localId: item.id.localId, title: item.title})
        }
        previewItems = existing
        previewGeneration = result.generation
        previewNextCursor = typeof result.nextCursor === "string" && result.nextCursor !== "" ? result.nextCursor : null
        previewCoverage = ["partial", "complete", "unknown"].indexOf(result.coverage) >= 0 ? result.coverage : "unknown"
    }

    function acceptResponse(id, result) {
        if (id === "")
            return false
        if (id === listRequestId) {
            listRequestId = ""
            if (!acceptSnapshot(result))
                error = qsTr("OpenNOW could not read the plugin list.")
            if (refreshQueued)
                refresh()
            return true
        }
        if (id === mutationRequestId) {
            mutationRequestId = ""
            busyId = ""
            if (!acceptSnapshot(result))
                refresh()
            return true
        }
        if (id === inspectRequestId) {
            inspectRequestId = ""
            if (result && result.inspection && typeof result.inspection.token === "string"
                    && result.inspection.plugin && validGeneration(result.generation))
                inspection = Object.assign({}, result.inspection, {generation: result.generation})
            else
                installError = qsTr("OpenNOW could not read this plugin package.")
            return true
        }
        if (id === commitRequestId) {
            commitRequestId = ""
            inspection = null
            if (!acceptSnapshot(result))
                refresh()
            return true
        }
        if (id === previewRequestId) {
            acceptPreviewPage(result)
            return true
        }
        return false
    }

    function acceptFailure(id, code, message) {
        if (id === "")
            return false
        const text = String(message || "")
        if (id === listRequestId) {
            listRequestId = ""
            error = text
            if (refreshQueued)
                refresh()
            return true
        }
        if (id === mutationRequestId) {
            mutationRequestId = ""
            busyId = ""
            error = text
            refresh()
            return true
        }
        if (id === inspectRequestId) {
            inspectRequestId = ""
            installError = text
            return true
        }
        if (id === commitRequestId) {
            commitRequestId = ""
            inspection = null
            installError = text
            refresh()
            return true
        }
        if (id === previewRequestId) {
            previewRequestId = ""
            previewAppending = false
            if (code === "stale_source") {
                previewItems = []
                previewNextCursor = null
                previewWaiting = true
                refresh()
            } else if (code !== "cancelled") {
                previewError = text
            }
            return true
        }
        return false
    }

    function acceptChanged(payload) {
        if (!payload || !validGeneration(payload.generation) || payload.generation !== generation)
            refresh()
    }
}
