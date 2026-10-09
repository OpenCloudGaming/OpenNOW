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

    function checkSourceDetailsLaunch() {
        const core = Qt.createQmlObject(`import QtQuick
            QtObject {
                property int next: 1
                property var requests: ({})
                function request(method, params) {
                    const id = "detail-" + next++
                    const copy = Object.assign({}, requests)
                    copy[id] = {method: method, params: params}
                    requests = copy
                    return id
                }
                function cancel(id) { return true }
                function idsFor(method) { return Object.keys(requests).filter(id => requests[id].method === method) }
            }`, root)
        const stateComponent = Qt.createQmlObject("import QtQuick; import OpenNOW; Component { SourceState {} }", root, "detailsSourceStub")
        const libraryComponent = Qt.createQmlObject("import QtQuick; import OpenNOW; Component { SourceLibraryState {} }", root, "detailsLibraryStub")
        const modalComponent = Qt.createQmlObject("import QtQuick; import OpenNOW; Component { DesktopSourceGameModal {} }", root, "detailsModalStub")
        const state = stateComponent.createObject(root, {coreClient: core, bridge: null, ready: true, available: true})
        const lib = libraryComponent.createObject(root, {coreClient: core, sources: state})
        let modal = null
        const last = method => core.idsFor(method).slice(-1)[0]
        const reply = (id, body) => ({sourceId: providerId, generation: 1, result: body})
        try {
            check(state.acceptSnapshot({generation: 1, selectedSourceId: providerId, sources: [{id: providerId,
                name: "Example Provider", enabled: true, state: "ready", builtin: false, protocolVersion: 2,
                providerCapabilities: ["auth.deviceCode.v2", "catalog.library.v2", "catalog.details.v2", "launch.v2",
                    "sessions.v2", "settings.v2"], authKinds: ["device-code"], playback: {transport: "provider-worker"}}]}),
                "the playable source snapshot is accepted")
            check(state.acceptResponse(last("sources.auth.state"), reply("", {state: "signed-in",
                account: {key: "account-a", name: "Example player"}, revision: "auth-1"})), "the source account signs in")
            lib.sync()
            check(lib.mode === "library", "the account library is requested")
            lib.acceptResponse(lib.pageRequestId, reply("", {items: [{id: "game-1", title: "Provider <b>game</b>",
                artwork: null, subtitle: "Action", badges: [], availability: "available"}], nextCursor: null, coverage: "complete"}))
            modal = modalComponent.createObject(root, {library: lib, width: 1280, height: 800})
            check(modal.store === state, "the details view uses the library's own source owner")
            check(modal.show(lib.items[0]), "activating a poster requests source details")
            check(modal.requested && modal.game.title === "Provider <b>game</b>", "details open on the activated poster")
            const get = core.requests[lib.detailsRequestId]
            check(get.method === "sources.game.get" && get.params.request.game === "game-1"
                && JSON.stringify(get.params.request.scope) === JSON.stringify({kind: "account", scope: {account: "account-a", revision: "auth-1"}}),
                "details are requested for the selected account")
            lib.acceptResponse(lib.detailsRequestId, reply("", {game: {id: "game-1", title: "Provider <b>game</b>",
                artwork: null, subtitle: "Action", badges: [], availability: "available"}, description: "Plain <i>text</i>.",
                variants: [{id: "variant-a", label: "Windows", availability: "available"}], revision: "catalog-9"}))
            modal.opened = true
            check(find(modal, "sourceDetailsDialog") !== null && find(modal, "sourceDetailsScroll") !== null
                && find(modal, "sourceDetailsClose") !== null, "source details use the shared game details dialog")
            check(find(modal, "gameDetailsSummary") === null && find(modal, "cloudLibraryActions") === null
                && find(modal, "desktopGamePlay") === null, "GeForce NOW-only actions stay out of source details")
            const title = find(modal, "sourceDetailsTitle")
            check(title.text === "Provider <b>game</b>" && title.textFormat === Text.PlainText, "the title stays plain text")
            check(modal.badgeText === "EXAMPLE PROVIDER" && modal.metaText === "Action", "the header names the service and subtitle")
            check(find(modal, "desktopSourceDescription").text === "Plain <i>text</i>."
                && find(modal, "desktopSourceDescription").textFormat === Text.PlainText, "the description stays plain text")
            const variant = find(modal, "sourceDetailsVariant")
            check(variant.visible && variant.title === "Windows" && variant.detail === "Available", "the playable version is shown")
            check(find(modal, "sourceDetailsSettings").visible, "service settings are offered when the source has settings")
            const play = find(modal, "desktopSourcePlay")
            check(play.enabled && play.text === "Play" && find(modal, "desktopSourcePlayNote").text === "",
                "a playable source game offers Play")
            let ready = null
            lib.playReady.connect((details, decision) => ready = {details: details, decision: decision})
            play.clicked()
            const inspect = core.requests[lib.inspectRequestId]
            check(inspect && inspect.method === "sources.launch.inspect"
                && inspect.params.request.target.game === "game-1" && inspect.params.request.target.variant === "variant-a"
                && inspect.params.request.catalogRevision === "catalog-9"
                && inspect.params.request.scope.account === "account-a", "Play inspects the account-scoped launch target")
            check(!play.enabled && play.text === "Checking…", "Play waits for the launch decision")
            lib.acceptResponse(lib.inspectRequestId, reply("", {state: "ready", target: {game: "game-1", variant: "variant-a"},
                revision: "catalog-9"}))
            check(ready !== null && ready.details.id === "game-1" && ready.decision.state === "ready",
                "a ready decision hands the details to the session owner")
            check(core.idsFor("sources.session.create").length === 0, "the details view never allocates a session itself")
            play.clicked()
            lib.acceptResponse(lib.inspectRequestId, reply("", {state: "blocked", reason: "maintenance", message: "Down for maintenance"}))
            check(find(modal, "desktopSourcePlayNote").text === "Down for maintenance", "a blocked launch explains why")
            lib.details = Object.assign({}, lib.details, {variants: [
                {id: "maintenance", label: "Updating", availability: "maintenance"},
                {id: "unknown", label: "Boosteroid", availability: "unknown"}
            ]})
            ready = null
            check(lib.defaultVariant() !== null && lib.defaultVariant().id === "unknown" && play.enabled,
                "an unknown version permits an authoritative launch check without selecting an unavailable version")
            check(variant.visible && variant.title === "Boosteroid" && variant.detail === "",
                "unknown availability is not presented as available or unavailable")
            play.clicked()
            check(core.requests[lib.inspectRequestId].params.request.target.variant === "unknown",
                "an unknown version still goes through launch inspection")
            lib.acceptResponse(lib.inspectRequestId, reply("", {state: "blocked", reason: "subscription-required", message: "Subscription needed"}))
            check(ready === null && core.idsFor("sources.session.create").length === 0
                && find(modal, "desktopSourcePlayNote").text === "Subscription needed",
                "a blocked inspection cannot launch an unknown version")
            lib.details = Object.assign({}, lib.details, {variants: [
                {id: "unknown", label: "Boosteroid", availability: "unknown"},
                {id: "available", label: "Ready", availability: "available"}
            ]})
            check(lib.defaultVariant().id === "available", "an available version is preferred over an unknown version")
            lib.details = Object.assign({}, lib.details, {variants: [
                {id: "maintenance", label: "Updating", availability: "maintenance"}
            ]})
            check(lib.defaultVariant() === null && !play.enabled, "known unavailable versions do not enable Play")
            modal.closeRequested()
            check(lib.details === null && !modal.requested && modal.preview === null, "closing details clears the source details")
        } finally {
            if (modal)
                modal.destroy()
            lib.destroy()
            state.destroy()
            modalComponent.destroy()
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

    function checkRejectedSourceStartOrdering() {
        const owner = ShellStore.sourceSessionOwnerState
        const previousStreamer = ShellStore.streamer
        const previousRequests = ShellStore.nativeRequests
        const previousStartRequest = ShellStore.streamerStartRequestId
        const previousState = ShellStore.streamState
        const previousMessage = ShellStore.streamMessage
        const keepPreview = showcaseStage === "rejected-start"
        try {
            for (const scenario of ["missing-lease", "wrong-lease", "failure-before-response"]) {
                const startId = "rejected-source-start-" + scenario
                owner.sourceId = providerId
                owner.title = "Provider startup failure fixture"
                owner.phase = "connecting"
                owner.startId = startId
                ShellStore.nativeRequests = Object.assign({}, previousRequests, {[startId]: {operation: "source-start"}})
                ShellStore.streamerStartRequestId = startId
                ShellStore.streamer = {status: "starting", message: "Preparing provider playback"}
                ShellStore.streamState = "starting"
                const response = {id: startId, type: "ok", transport: "provider-worker"}
                if (scenario !== "missing-lease")
                    response.leaseId = "wrong-lease"
                const message = "The media runtime accepted a different session lease"
                SourceBridge.failed("unrelated-start", "invalid_source_lease", "Unrelated failure")
                check(ShellStore.streamer.status === "starting" && owner.startId === startId,
                    "an unrelated failure cannot change the current start")
                if (scenario === "failure-before-response") {
                    SourceBridge.failed(startId, "invalid_source_lease", message)
                    ShellStore.acceptNativeResponse(response)
                } else {
                    ShellStore.acceptNativeResponse(response)
                    check(ShellStore.streamer.status === "connecting" && !ShellStore.nativeRequests[startId],
                        "the native response consumes the request before the queued bridge failure")
                    SourceBridge.failed(startId, "invalid_source_lease", message)
                }
                check(ShellStore.streamer.status === "error" && ShellStore.streamState === "error",
                    "a rejected lease leaves both stream state owners in error: " + scenario)
                check(owner.phase === "failed" && owner.startId === "" && owner.message === message,
                    "the source session retains the correlated startup failure")
                check(ShellStore.streamerStartRequestId === "" && !ShellStore.nativeRequests[startId],
                    "the rejected native request is cleared")
                SourceBridge.failed(startId, "duplicate", "Do not replace the original failure")
                check(ShellStore.streamer.message === message && owner.message === message,
                    "duplicate failure delivery preserves the original error")
            }
        } finally {
            if (!keepPreview) {
                owner.reset()
                ShellStore.streamer = previousStreamer
                ShellStore.nativeRequests = previousRequests
                ShellStore.streamerStartRequestId = previousStartRequest
                ShellStore.streamState = previousState
                ShellStore.streamMessage = previousMessage
            }
        }
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
            if (!ShellStore.ready || !store.loaded || !ShellStore.nativeRuntimeReady)
                return 0
            check(CoreClient.capabilities.indexOf("sources.v2") >= 0, "the core advertises sources.v2")
            checkReinstallWithLowerGeneration()
            checkAnonymousLibraryOnly()
            checkSourceDetailsLaunch()
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
            const grid = find(root, "desktopSourceGrid")
            if (grid !== null) {
                const poster = grid.itemAtIndex(0)
                check(poster !== null && poster.game.title === library.items[0].title && poster.playHint
                    && grid.cellHeight === Math.round((grid.cellWidth - 12) * 198 / 132) + 12,
                    "source games use the shared library poster grid")
                check(poster.Accessible.name === library.items[0].title,
                    "shared poster buttons announce their game title")
                poster.clicked()
                const modal = find(root, "desktopSourceDetails")
                check(modal.opened && modal.game.title === library.items[0].title && library.detailsRequestId !== "",
                    "activating a poster opens the shared details dialog")
            } else {
                check(library.openDetails("game-1"), "details open")
            }
            phase = 4
            return 0
        }
        if (phase === 4) {
            if (library.details === null)
                return 0
            check(library.details.description === "A <i>plain</i> description.", "description stays plain text")
            const details = find(root, "desktopSourceDetails")
            if (details !== null)
                check(find(details, "desktopSourceDescription").text === library.details.description
                    && find(details, "desktopSourcePlay").enabled, "the details dialog shows the loaded game")
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
            checkRejectedSourceStartOrdering()
            if (showcaseStage === "rejected-start") {
                AppController.navigate("stream")
                phase = 62
                return 0
            }
            if (showcaseStage === "settings") {
                AppController.navigate("settings-plugins")
                phase = 50
                return 0
            }
            library.closeDetails()
            const closed = find(root, "desktopSourceDetails")
            check(closed === null || !closed.opened, "closing source details dismisses the dialog")
            check(store.select(anonymousId), "the anonymous service is selected")
            phase = 7
            return 0
        }
        if (phase === 62)
            return settle(100)
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
