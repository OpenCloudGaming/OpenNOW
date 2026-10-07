import QtQuick
import OpenNOW

QtObject {
    property int key: 0
    property int modifiers: 0
    property int phase: 0
    property Item root: null
    readonly property var store: ShellStore.sourceOwnerState
    readonly property var library: ShellStore.sourceLibraryOwnerState
    readonly property string providerId: "org.opennow.example.provider"
    readonly property string anonymousId: "org.opennow.example.anonymous"
    readonly property bool sourcesOnly: Qt.application.arguments.indexOf("--sources-only") >= 0
    readonly property bool showcase: Qt.application.arguments.indexOf("--sources-showcase") >= 0
    readonly property string showcaseStage: showcase
        ? String(Qt.application.arguments[Qt.application.arguments.indexOf("--sources-showcase") + 1] || "library") : ""
    property int ticks: 0

    function check(ok, message) { if (!ok) throw new Error("Sources: " + message) }
    function find(item, name, visited) {
        const seen = visited || new Set()
        if (!item || seen.has(item))
            return null
        seen.add(item)
        if (item.objectName === name)
            return item
        for (const child of (item.children || []).concat(item.data || [])) {
            const found = find(child, name, seen)
            if (found)
                return found
        }
        return null
    }
    function screen() { return find(root, "desktopSourceLibrary") || find(root, "consoleSourceLibrary") }
    function provider() { return store.authState(providerId) }

    function checkReinstallWithLowerGeneration() {
        const core = Qt.createQmlObject(`import QtQuick
            QtObject {
                property int next: 1
                property var requests: ({})
                property var cancelled: []
                function request(method, params) {
                    const id = "stub-" + next++
                    const copy = Object.assign({}, requests)
                    copy[id] = {method: method, params: params}
                    requests = copy
                    return id
                }
                function cancel(id) { cancelled = cancelled.concat([id]); return true }
                function idsFor(method) { return Object.keys(requests).filter(id => requests[id].method === method) }
            }`, root)
        const stateComponent = Qt.createQmlObject("import QtQuick; import OpenNOW; Component { SourceState {} }", root, "sourceStub")
        const libraryComponent = Qt.createQmlObject("import QtQuick; import OpenNOW; Component { SourceLibraryState {} }", root, "libraryStub")
        const state = stateComponent.createObject(root, {coreClient: core, bridge: null, ready: false, available: false})
        const lib = libraryComponent.createObject(root, {coreClient: core, sources: state})
        check(state !== null && lib !== null, "the stub source owners are created")
        const row = (enabled, state) => ({id: providerId, name: "Example Provider", enabled: enabled, state: state, builtin: false,
            protocolVersion: 2, providerCapabilities: ["auth.deviceCode.v2", "catalog.public.v2", "settings.v2"],
            authKinds: ["device-code"], playback: null})
        const snapshot = (generation, rows) => ({generation: generation, selectedSourceId: providerId, sources: rows})
        const reply = (id, generation, body) => state.acceptResponse(id, {sourceId: providerId, generation: generation, result: body})
        const last = method => core.idsFor(method).slice(-1)[0]
        try {
            state.ready = true
            state.available = true
            check(state.acceptSnapshot(snapshot(1, [row(true, "ready")])), "the stub snapshot is accepted")
            const firstAuth = last("sources.auth.state")
            check(reply(firstAuth, 7, {state: "not-required"}), "the first incarnation answers")
            check(state.sourceGenerations[providerId] === 7 && state.authState(providerId).state === "not-required",
                "the source generation is recorded")
            state.loadSettings(providerId)
            const oldSettings = last("sources.providerSettings.get")
            lib.sync()
            const oldPage = lib.pageRequestId
            check(oldPage !== "", "the library requests a page for the live source")
            check(state.acceptSnapshot(snapshot(2, [])), "the removal snapshot is accepted")
            check(state.sourceGenerations[providerId] === undefined && state.authState(providerId) === null,
                "removing the source evicts its generation and auth state")
            check(core.cancelled.indexOf(oldSettings) >= 0, "removing the source cancels its pending requests")
            check(!reply(oldSettings, 7, {revision: 1, settings: [{key: "old"}]}) && state.settingsViews[providerId] === undefined,
                "a delayed reply from the removed source cannot repopulate state")
            check(lib.pageRequestId === "" && core.cancelled.indexOf(oldPage) >= 0 && lib.items.length === 0,
                "the library cancels and clears the removed source")
            check(state.acceptSnapshot(snapshot(3, [row(false, "disabled")])), "the reinstalled source starts disabled")
            check(state.acceptSnapshot(snapshot(4, [row(true, "ready")])), "the reinstalled source becomes ready")
            const freshAuth = last("sources.auth.state")
            check(freshAuth !== firstAuth && reply(freshAuth, 1, {state: "not-required"}), "the fresh incarnation answers")
            check(state.sourceGenerations[providerId] === 1 && state.authState(providerId).state === "not-required",
                "a reinstalled source with a lower fresh generation is accepted")
            state.loadSettings(providerId)
            check(reply(last("sources.providerSettings.get"), 1, {revision: 1, settings: []})
                && state.settingsViews[providerId] !== undefined, "settings from the fresh incarnation are accepted")
            lib.sync()
            check(lib.pageRequestId !== "" && lib.pageRequestId !== oldPage, "the library reloads for the fresh incarnation")
            lib.acceptResponse(lib.pageRequestId, {sourceId: providerId, generation: 1, result: {items: [
                {id: "fresh", title: "Fresh title", availability: "available"}], nextCursor: null, coverage: "complete"}})
            check(lib.items.length === 1 && lib.items[0].title === "Fresh title", "the fresh library page is shown")
        } finally {
            lib.destroy()
            state.destroy()
            stateComponent.destroy()
            libraryComponent.destroy()
            core.destroy()
        }
    }

    function checkAnonymousLibraryOnly() {
        const core = Qt.createQmlObject(`import QtQuick
            QtObject {
                property int next: 1
                property var requests: ({})
                function request(method, params) {
                    const id = "anon-" + next++
                    const copy = Object.assign({}, requests)
                    copy[id] = {method: method, params: params}
                    requests = copy
                    return id
                }
                function cancel(id) { return true }
                function idsFor(method) { return Object.keys(requests).filter(id => requests[id].method === method) }
            }`, root)
        const stateComponent = Qt.createQmlObject("import QtQuick; import OpenNOW; Component { SourceState {} }", root, "anonymousSourceStub")
        const libraryComponent = Qt.createQmlObject("import QtQuick; import OpenNOW; Component { SourceLibraryState {} }", root, "anonymousLibraryStub")
        const state = stateComponent.createObject(root, {coreClient: core, bridge: null, ready: true, available: true})
        const lib = libraryComponent.createObject(root, {coreClient: core, sources: state})
        const last = method => core.idsFor(method).slice(-1)[0]
        try {
            check(state.acceptSnapshot({generation: 1, selectedSourceId: providerId, sources: [{id: providerId,
                name: "Anonymous Provider", enabled: true, state: "ready", builtin: false, protocolVersion: 2,
                providerCapabilities: ["catalog.library.v2", "catalog.details.v2", "launch.v2"], authKinds: [], playback: null}]}),
                "the anonymous library-only snapshot is accepted")
            check(state.acceptResponse(last("sources.auth.state"), {sourceId: providerId, generation: 1, result: {state: "not-required"}}),
                "the anonymous source reports that sign-in is not required")
            check(state.accountScope(providerId) === null, "no account is fabricated for the anonymous source")
            lib.sync()
            check(lib.desiredMode === "library" && !lib.needsSignIn, "an anonymous library-only source opens its library without sign-in")
            const page = core.requests[lib.pageRequestId]
            check(page !== undefined && page.method === "sources.library.page"
                && JSON.stringify(page.params.request.scope) === JSON.stringify({kind: "public"}),
                "the library page uses the public catalog scope")
            lib.acceptResponse(lib.pageRequestId, {sourceId: providerId, generation: 1, result: {items: [
                {id: "anon-game", title: "Anonymous title", availability: "available"}], nextCursor: null, coverage: "complete"}})
            check(lib.items.length === 1 && lib.items[0].title === "Anonymous title", "the anonymous library page is shown")
        } finally {
            lib.destroy()
            state.destroy()
            stateComponent.destroy()
            libraryComponent.destroy()
            core.destroy()
        }
    }

    function checkPaletteGames(external) {
        const palette = Qt.createQmlObject("import OpenNOW; DesktopCommandPalette { visible: false }", root)
        try {
            palette.query = "game"
            check(palette.gfnGames === !external && (external ? !palette.gamesQuery && palette.gameList.length === 0 : palette.gamesQuery),
                external ? "the command palette hides GeForce NOW games while another service is browsed"
                    : "the command palette searches GeForce NOW games when it is browsed")
            palette.query = ""
            check(palette.actionList.length > 0, "the command palette keeps its generic actions")
            palette.cycleScope()
            check(palette.scopeFilter === (external ? "actions" : "games"), "the palette scope filter skips unavailable game results")
        } finally {
            palette.destroy()
        }
    }

    function checkSourceStreamOverlays() {
        const owner = ShellStore.sourceSessionOwnerState
        check(!ShellStore.sourceStreamActive && ShellStore.sourceStreamGame === null, "no source stream is active yet")
        owner.sourceId = providerId
        owner.title = "Provider <b>game</b>"
        owner.artworkUrl = ""
        owner.startId = "acceptance-source-start"
        owner.phase = "connecting"
        try {
            check(ShellStore.sourceStreamActive, "a source stream is the active stream")
            check(ShellStore.streamOwnerSignedIn, "the source stream owns gameplay input without a GeForce NOW sign-in")
            check(ShellStore.sourceStreamGame.title === "Provider <b>game</b>"
                && ShellStore.sourceStreamGame.sourceName === "Example Provider",
                "the active stream view names the source game and service")
            const guide = Qt.createQmlObject("import OpenNOW; GuideOverlay { visible: false }", root)
            const stats = Qt.createQmlObject("import OpenNOW; DesktopStreamStats { visible: false }", root)
            try {
                check(guide.game.title === "Provider <b>game</b>", "the Guide shows the source game, not the selected GFN game")
                check(guide.subtitle === "Example Provider", "the Guide subtitle names the service, not a GFN store or region")
                check(stats.sourcePlayback === true && stats.shown("Region") === false,
                    "stream stats hide the GFN region for a source stream")
                check(["Ping", "Jitter", "PacketLoss", "Receive"].every(key => stats.unmeasuredKeys.indexOf(key) >= 0),
                    "stream stats hide unmeasured GFN transport metrics for a source stream")
                check(ShellStore.activeStreamId === "source:" + providerId + ":acceptance-source-start",
                    "stream actions are fenced by the source media start")
                check(ShellStore.activeStreamTitle === "Provider <b>game</b>", "captures are named after the source game")
                check(Object.keys(ShellStore.activeStreamProfile).length === 0,
                    "a source stream exposes no media profile until native accepts that exact start")
                const antiAfk = find(guide, "guideAntiAfkTile")
                check(antiAfk !== null && antiAfk.enabled === false, "the unsupported Anti-AFK action is disabled for a source stream")
                ShellStore.applyStreamShortcutAction("toggle-anti-afk")
                check(ShellStore.antiAfkEnabled === false, "the Anti-AFK shortcut cannot enable an unsupported source action")
                const previousStreamer = ShellStore.streamer
                ShellStore.streamer = {status: "streaming"}
                try {
                    ShellStore.toggleStreamRecording()
                    check(ShellStore.mediaRecordingTargetRequestId !== ""
                        && ShellStore.mediaRecordingTargetSessionId === ShellStore.activeStreamId,
                        "recording a source stream requests a target fenced to that stream")
                    ShellStore.cancelRecordingTarget()
                } finally {
                    ShellStore.streamer = previousStreamer
                }
            } finally {
                guide.destroy()
                stats.destroy()
            }
        } finally {
            owner.reset()
        }
        check(!ShellStore.sourceStreamActive, "the source stream view clears with the session")
        check(ShellStore.streamOwnerSignedIn === ShellStore.signedIn, "input ownership returns to the GeForce NOW sign-in state")
    }

    function run(parent) {
        root = parent
        return true
    }

    function advance() {
        try {
            return step()
        } catch (error) {
            console.error(error.message)
            return 2
        }
    }

    function settle(next) {
        ticks += 1
        if (ticks < 8)
            return 0
        ticks = 0
        phase = next
        return 0
    }

    function step() {
        if (phase === 0) {
            if (!ShellStore.ready || !store.loaded)
                return 0
            check(CoreClient.capabilities.indexOf("sources.v2") >= 0, "the core advertises sources.v2")
            checkReinstallWithLowerGeneration()
            checkAnonymousLibraryOnly()
            if (sourcesOnly)
                check(CoreClient.capabilities.indexOf("queue.servers.v1") < 0, "a sources-only core connects without GeForce NOW capabilities")
            check(store.sources.length === 3 && store.selectedIsGfn, "three services are listed and GeForce NOW is selected")
            check(!ShellStore.browsingExternalSource, "GeForce NOW browsing stays on the existing screens")
            check(store.select(providerId), "selecting another service starts")
            phase = 1
            return 0
        }
        if (phase === 1) {
            if (store.selectedSourceId !== providerId || screen() === null || library.loading || library.items.length === 0)
                return 0
            check(ShellStore.browsingExternalSource, "the selected service drives browsing")
            checkPaletteGames(true)
            check(library.mode === "public" && library.items.length === 30, "the public catalog loads before sign-in")
            check(library.items[0].title === "Provider game 1" && library.items[0].ref.sourceId === providerId,
                  "items keep their source reference")
            const signIn = find(root, "desktopSignInScreen")
            check(signIn === null || !signIn.visible, "GeForce NOW sign-in doesn't block another service")
            check(store.startSignIn(providerId, "device-code"), "device-code sign-in starts")
            phase = 2
            return 0
        }
        if (phase === 2) {
            const state = provider()
            if (!state || state.state !== "pending")
                return 0
            check(state.challenge.kind === "device-code" && state.challenge.userCode === "WXYZ-1234", "the device code is shown")
            check(store.pollTimer.interval >= store.minimumPollMs && store.pollTimer.running, "polling waits for the provider delay")
            if (showcaseStage === "device-code")
                return settle(100)
            phase = 3
            return 0
        }
        if (phase === 3) {
            const state = provider()
            if (!state || state.state !== "signed-in" || library.mode !== "library" || library.loading || library.items.length === 0)
                return 0
            check(state.account.name === "Example player", "sign-in completes only after explicit completion")
            check(store.accountScope(providerId).revision === state.revision, "the account scope follows the signed-in revision")
            check(library.items.length === 30, "the account library replaces the public catalog")
            check(library.openDetails("game-1"), "details open")
            phase = 4
            return 0
        }
        if (phase === 4) {
            if (library.details === null)
                return 0
            check(library.details.description === "A <i>plain</i> description.", "description stays plain text")
            if (showcaseStage === "library")
                return settle(100)
            check(store.loadSettings(providerId), "service settings load")
            phase = 5
            return 0
        }
        if (phase === 5) {
            const view = store.settingsViews[providerId]
            if (!view)
                return 0
            check(view.settings.length === 2 && view.revision === 1, "service settings are listed")
            check(store.setSetting(providerId, "hdr", {kind: "boolean", value: true}), "a service setting is written")
            phase = 6
            return 0
        }
        if (phase === 6) {
            const view = store.settingsViews[providerId]
            if (!view || view.revision !== 2)
                return 0
            check(view.settings[0].value.value === true, "the service returns the updated setting")
            const stream = store.streamSettingsViews[providerId]
            check(stream && stream.settings.length === 1 && stream.settings[0].key === "stream.codec",
                "host stream overrides load separately from provider settings")
            check(store.setSetting(providerId, "stream.codec", {kind: "choice", value: "h264"}, "stream"),
                "a host stream override is written")
            phase = 61
            return 0
        }
        if (phase === 61) {
            const stream = store.streamSettingsViews[providerId]
            if (!stream || stream.revision !== 2)
                return 0
            check(stream.settings[0].value.value === "h264", "the host stream override is applied")
            check(store.settingsViews[providerId].revision === 2, "provider settings keep their own revision")
            check(library.playbackSupported(), "the provider advertises playback")
            check(library.defaultVariant() !== null, "a playable variant is chosen")
            checkSourceStreamOverlays()
            if (showcaseStage === "settings") {
                AppController.navigate("settings-plugins")
                phase = 50
                return 0
            }
            library.closeDetails()
            check(store.select(anonymousId), "the anonymous service is selected")
            phase = 7
            return 0
        }
        if (phase === 7) {
            if (store.selectedSourceId !== anonymousId || library.sourceId !== anonymousId || library.loading
                    || library.items.length === 0)
                return 0
            check(store.authState(anonymousId).state === "not-required", "an anonymous service needs no account")
            check(library.items.length === 4 && library.items[0].title === "Free title 1", "the anonymous catalog loads")
            check(store.signOut(providerId), "sign-out starts")
            phase = 8
            return 0
        }
        if (phase === 8) {
            const state = provider()
            if (!state || state.state !== "signed-out")
                return 0
            check(store.select(store.gfnId), "GeForce NOW can be selected again")
            phase = 9
            return 0
        }
        if (phase === 9) {
            if (!store.selectedIsGfn || ShellStore.browsingExternalSource)
                return 0
            check(library.items.length === 0 && library.sourceId === "", "the external library is cleared")
            checkPaletteGames(false)
            return 1
        }
        if (phase === 50) {
            const page = find(root, "desktopPluginsSettings")
            const console = find(root, "consoleSettingsScreen")
            if (page !== null) {
                page.expandedId = providerId
                return settle(100)
            }
            if (console !== null) {
                console.openPluginSheet(providerId)
                return settle(100)
            }
            return 0
        }
        if (phase === 100)
            return 1
        return 2
    }
}
