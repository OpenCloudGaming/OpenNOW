import QtQuick
import OpenNOW
import OpenNOW.Acceptance 1.0

QtObject {
    id: root
    property AcceptanceCoreClient client: AcceptanceCoreClient {
        property string state: "ready"
        property string lastError: ""
        property int sequence: 0
        property var requests: []
        property var cancelled: []
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "search-fixture-" + (++sequence)
            requests.push({id:id, method:method, params:params, at:Date.now(),
                scope:JSON.parse(JSON.stringify(ShellStore.catalogOwnerState.authScope))})
            return id
        }
        function cancel(id) {
            cancelled.push(id)
            responseReceived(id, root.page([root.game("cancelled")]))
            requestFailed(id, "cancelled", "Synchronous cancellation")
            return true
        }
    }
    property var palette: null
    property var host: null
    property var field: null
    property int key: 0
    property int modifiers: 0
    property int phase: 0
    property double startedAt: 0
    property int beforeCount: 0
    property string inFlight: ""
    property string browsingBefore: ""
    property Component disposable: Component { DesktopCommandPalette {} }
    function check(ok, message) { if (!ok) throw new Error("Command search: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.data || item.children || []) {
            const match = find(child, name)
            if (match) return match
        }
        return null
    }
    function game(id) {
        return {id:id, uuid:id, title:"Synthetic search game " + id, imageUrl:"", heroImageUrl:"",
            selectedVariantIndex:0, availableStores:["STEAM"], variants:[
                {id:"123", store:"STEAM", inLibrary:false, libraryStatus:"NOT_OWNED", gfnStatus:"AVAILABLE", playStatus:"PLAYABLE"}]}
    }
    function page(games, more) {
        return {games:games, scope:ShellStore.catalogOwnerState.authScope,
            catalogRevision:ShellStore.catalogOwnerState.catalogRevision === null ? 0 : ShellStore.catalogOwnerState.catalogRevision,
            hasNextPage:more === true,
            nextCursor:more ? "opaque-next-page" : "", totalCount:null}
    }
    function count(method) { return client.requests.filter(item => item.method === method).length }
    function searches() { return client.requests.filter(item => item.method === "catalog.store.list") }
    function browsing() {
        const owner = ShellStore.catalogOwnerState
        return JSON.stringify([owner.storeGames, owner.storeSearchQuery, owner.storeFilters,
            owner.storeRequestId, owner.storeNextCursor, owner.storeHasMore, owner.storeState, owner.selectedGame])
    }
    function typeQuery(value) { field.text = value }
    function submit(value) {
        typeQuery(value)
        palette.requestGames()
        check(palette.searchRequestId !== "", "query did not submit")
        return palette.searchRequestId
    }
    function stale(id) {
        client.responseReceived(id, page([game("stale")]))
        client.requestFailed(id, "network", "Stale failure")
        check(palette.remoteGames.length === 0 && palette.searchError === "", "stale callback published")
    }
    function invalidate(change, label) {
        const id = submit("context " + label)
        change()
        check(palette.searchRequestId === "" && client.cancelled.indexOf(id) >= 0, label + " did not cancel")
        stale(id)
    }
    function run(windowHost) {
        host = windowHost
        ShellStore.settings = Object.assign({}, ShellStore.settings, {onboardingCompleted:true,
            hiddenGameIds:[], appTheme:Qt.application.arguments.indexOf("--search-scaled-light") >= 0 ? "light" : "dark",
            desktopUiScale:Qt.application.arguments.indexOf("--search-scaled-light") >= 0 ? 1.4 : 1})
        ShellStore.authGeneration = 42
        ShellStore.authSession = {user:{userId:"synthetic-user", displayName:"Search fixture"},
            provider:{idpId:"synthetic-provider", code:"NVIDIA"}}
        ShellStore.nativeRuntimeReady = true
        ShellStore.catalogOwnerState.catalogState = "ready"
        ShellStore.catalogOwnerState.catalogGames = []
        palette = find(host, "desktopCommandPalette")
        check(palette, "production DesktopApp palette is missing")
        field = find(palette, "commandSearchField")
        check(field, "production search input is missing")
        palette.parent.commandOpen = true
        check(find(palette, "commandSearchDelay").interval === 2000, "debounce is not exactly 2000 ms")
        check(find(palette, "commandSearchDelay").timerType === Qt.PreciseTimer, "debounce can fire early")
        return true
    }
    function synchronousCases() {
        const owner = ShellStore.catalogOwnerState
        owner.storeGames = [game("browse-retained")]
        owner.storeSearchQuery = "browse query"
        owner.storeFilters = {genre:"ACTION"}
        owner.storeNextCursor = "synthetic-browse-cursor"
        owner.storeHasMore = true
        const saved = browsing()
        palette.acceptCurrent()
        check(!palette.opened && AppController.route === "home" && searches().length === 0,
            "empty-query Enter did not retain local command behavior")
        palette.parent.commandOpen = true
        const old = submit("first query")
        typeQuery("settings")
        check(palette.actionList.length > 0 && palette.searchState === "waiting" && palette.searchStatus !== "",
            "command matches hid the waiting status")
        check(palette.searchError === "" && palette.remoteGames.length === 0, "synchronous cancellation published")
        palette.acceptCurrent()
        const current = palette.searchRequestId
        const sent = searches().length
        palette.acceptCurrent()
        check(searches().length === sent && palette.searchRequestId === current, "Enter duplicated an in-flight query")
        check(palette.actionList.length > 0 && palette.searchStatus.indexOf("Searching") >= 0, "commands hid search status")
        stale(old)
        client.requestFailed(current, "network", "Synthetic search is temporarily unavailable.")
        check(palette.actionList.length > 0 && palette.searchStatus.indexOf("Synthetic") >= 0, "commands hid search error")
        palette.acceptCurrent()
        client.responseReceived(palette.searchRequestId, page([game("ranked-no-substring")], true))
        check(palette.gameList.length === 1 && palette.searchHasMore && palette.searchStatus !== "", "ranked page or has-more lost")
        client.requestFailed(old, "network", "Old failure after success")
        client.responseReceived(old, page([game("old success")]))
        check(palette.gameList[0].id === "ranked-no-substring" && palette.searchError === "", "out-of-order callback replaced success")
        check(browsing() === saved, "search changed Store browsing or selection")
        check(count("catalog.store.local") === 0, "palette used the local index")

        let id = submit("same owner refresh")
        ShellStore.authSession = Object.assign({}, ShellStore.authSession, {expiresAt:123456})
        check(palette.searchRequestId === id, "token-only same-generation replacement cancelled search")
        client.responseReceived(id, page([game("renewed")]))
        check(palette.gameList.length === 1, "same-owner token renewal lost valid results")
        invalidate(function() { ShellStore.authGeneration++ }, "same-owner generation")
        invalidate(function() { ShellStore.authSession = Object.assign({}, ShellStore.authSession, {user:{userId:"second-synthetic-user",displayName:"Search fixture"}}) }, "account")
        invalidate(function() { ShellStore.authSession = Object.assign({}, ShellStore.authSession, {provider:{idpId:"second-synthetic-provider", code:"NVIDIA"}}) }, "provider")
        invalidate(function() { owner.catalogContext = "synthetic-catalog-context" }, "catalog context")
        invalidate(function() { owner.catalogRevision = 73 }, "catalog revision")
        invalidate(function() { ShellStore.settings = Object.assign({}, ShellStore.settings, {region:"synthetic-region"}) }, "routing settings")
        invalidate(function() { ShellStore.storeSessionReset() }, "catalog reset")
        invalidate(function() { client.state = "starting" }, "readiness loss")
        check(palette.searchStatus.indexOf("reconnects") >= 0, "readiness status is missing")
        typeQuery("Open Library")
        const reconnectingCount = searches().length
        palette.acceptCurrent()
        check(AppController.route === "library" && !palette.opened && searches().length === reconnectingCount,
            "reconnecting Enter did not activate the local command without searching")
        AppController.navigate("home")
        palette.parent.commandOpen = true
        typeQuery("settings")
        client.state = "ready"
        check(palette.searchState === "waiting", "restart did not schedule fresh search")
        invalidate(function() { ShellStore.authSession = null }, "logout")
        typeQuery("settings")
        check(palette.actionList.length > 0 && palette.searchStatus.indexOf("Sign in") >= 0, "commands hid sign-in status")
        const signedOutCount = searches().length
        palette.acceptCurrent()
        check(searches().length === signedOutCount && !palette.opened && AppController.route === "settings",
            "signed-out Enter did not activate the local command without searching")
        AppController.navigate("home")
        palette.parent.commandOpen = true
        ShellStore.authSession = {user:{userId:"synthetic-user",displayName:"Search fixture"},provider:{idpId:"synthetic-provider",code:"NVIDIA"}}
        invalidate(function() { palette.scopeFilter = "actions" }, "actions-only")
        typeQuery("settings")
        check(palette.searchStatus === "" && palette.actionList.length > 0, "actions-only mode retained game status")
        palette.scopeFilter = "all"
        id = submit("hidden")
        ShellStore.settings = Object.assign({}, ShellStore.settings, {hiddenGameIds:["hidden"]})
        const hiddenCount = searches().length
        client.responseReceived(id, page([game("hidden")], true))
        check(palette.gameList.length === 0 && palette.searchStatus.indexOf("only hidden") >= 0
            && searches().length === hiddenCount, "hidden-only page crawled or claimed no matches")
        id = submit("hidden final")
        client.responseReceived(id, page([game("hidden")], false))
        check(palette.searchStatus.indexOf("All matching games are hidden") >= 0, "hidden final page was misrepresented")
        id = submit("no matches")
        client.responseReceived(id, page([], false))
        check(palette.searchStatus.indexOf("No games match") >= 0, "empty result status missing")
        id = submit("wrong response owner")
        const wrong = page([game("wrong")])
        wrong.scope = {generation:ShellStore.authGeneration, userId:"wrong-user", providerIdpId:"synthetic-provider"}
        client.responseReceived(id, wrong)
        check(palette.gameList.length === 0 && palette.searchState === "error" && palette.searchStatus !== ""
            && !find(palette, "commandSearchDelay").active, "foreign response scope did not produce a bounded visible error")
        id = submit("closing")
        palette.parent.commandOpen = false
        stale(id)
        check(palette.remoteGames.length === 0, "close retained results")
        palette.parent.commandOpen = true
        check(palette.query === "" && palette.searchRequestId === "" && palette.searchState === "idle",
            "reopen retained query state: " + JSON.stringify([palette.query,palette.searchRequestId,palette.searchState,palette.opened,AppController.route]))
        const temporary = disposable.createObject(host, {opened:true})
        temporary.query = "destruction"
        temporary.requestGames()
        inFlight = temporary.searchRequestId
        temporary.destroy()
        check(searches().every(item => item.params.limit === 6 && item.params.cursor === "" && item.params.revalidate === true),
            "request page is unbounded or not revalidated")
    }
    function checkRejectedResponse() {
        check(palette.gameList.length === 0 && palette.searchState === "error" && palette.searchStatus !== ""
            && palette.searchRequestId === "" && palette.searchIntent === null
            && !find(palette, "commandSearchDelay").active, "invalid response did not produce a bounded visible error")
    }
    function revisionCases() {
        const owner = ShellStore.catalogOwnerState
        owner.catalogRevision = 73
        const libraryBefore = JSON.stringify([owner.catalogGames,owner.catalogState,owner.catalogRequestId,
            owner.catalogContext,owner.catalogRevision,owner.catalogNextCursor])
        const browseBefore = browsing()
        const libraryRequests = count("catalog.library.list")
        const id = submit("newer revision")
        const fresh = page([game("fresh-74")])
        fresh.catalogRevision = 74
        client.responseReceived(id, fresh)
        check(palette.searchState === "ready" && palette.gameList.length === 1 && palette.gameList[0].id === "fresh-74",
            "fresh revision 74 was rejected against library snapshot 73")
        check(JSON.stringify([owner.catalogGames,owner.catalogState,owner.catalogRequestId,
            owner.catalogContext,owner.catalogRevision,owner.catalogNextCursor]) === libraryBefore
            && browsing() === browseBefore && count("catalog.library.list") === libraryRequests,
            "accepting a newer search revision changed or restarted library/browsing state")
        const invalidRevisions = [undefined,null,-1,0.5,"74",NaN,Infinity,9007199254740992]
        for (let index = 0; index < invalidRevisions.length; ++index) {
            const invalidId = submit("invalid revision " + index)
            const invalid = page([game("invalid")])
            if (index === 0) delete invalid.catalogRevision
            else invalid.catalogRevision = invalidRevisions[index]
            client.responseReceived(invalidId, invalid)
            checkRejectedResponse()
        }
        const olderId = submit("settings")
        const older = page([game("old-72")])
        older.catalogRevision = 72
        client.responseReceived(olderId, older)
        checkRejectedResponse()
        check(palette.actionList.length > 0, "invalid response hid local commands")
        beforeCount = searches().length
        startedAt = Date.now()
    }
    function advance() {
        if (phase === 0) {
            synchronousCases()
            phase = 1
            return 0
        }
        if (phase === 1) {
            check(client.cancelled.indexOf(inFlight) >= 0, "destruction did not cancel the request")
            revisionCases()
            phase = 13
            return 0
        }
        if (phase === 13) {
            checkRejectedResponse()
            check(searches().length === beforeCount, "older revision automatically retried")
            if (Date.now() - startedAt < 2200) return 0
            palette.acceptCurrent()
            check(searches().length === beforeCount + 1 && palette.searchRequestId !== "", "explicit retry did not submit once")
            client.responseReceived(palette.searchRequestId, page([game("retry-success")]))
            check(palette.searchState === "ready", "explicit retry could not recover")
            const id = submit("settings foreign scope")
            const foreign = page([game("foreign")])
            foreign.scope = {generation:ShellStore.authGeneration,userId:"foreign-user",providerIdpId:"synthetic-provider"}
            client.responseReceived(id, foreign)
            checkRejectedResponse()
            beforeCount = searches().length
            startedAt = Date.now()
            phase = 14
            return 0
        }
        if (phase === 14) {
            checkRejectedResponse()
            check(searches().length === beforeCount, "foreign scope automatically retried")
            if (Date.now() - startedAt < 2200) return 0
            field.forceActiveFocus()
            beforeCount = searches().length
            startedAt = Date.now()
            typeQuery("debounce final")
            phase = 2
            return 0
        }
        if (phase === 2) {
            check(searches().length === beforeCount, "query submitted before its debounce")
            if (Date.now() - startedAt < 1000) return 0
            startedAt = Date.now()
            typeQuery("  debounce   final  ")
            phase = 3
            return 0
        }
        if (phase === 3) {
            if (searches().length === beforeCount) {
                check(Date.now() - startedAt < 3000, "debounce never submitted")
                return 0
            }
            const request = searches()[searches().length - 1]
            check(request.at - startedAt >= 2000 && searches().length === beforeCount + 1,
                "trailing debounce fired before 2000 ms or sent twice: "
                    + JSON.stringify([request.at - startedAt,searches().length - beforeCount]))
            check(request.params.searchQuery === "debounce final", "debounce submitted an earlier query")
            client.responseReceived(request.id, page([game("old row")]))
            typeQuery("  settings   query  ")
            check(palette.gameList.length === 0, "old row remains selectable during debounce")
            beforeCount = searches().length
            key = Qt.Key_Return
            phase = 4
            return 0
        }
        if (phase === 4) {
            check(searches().length === beforeCount + 1 && palette.searchRequestId !== "", "keyboard Enter did not flush debounce")
            check(searches()[searches().length - 1].params.searchQuery === "settings query", "submitted query was not normalized")
            check(count("catalog.launch.inspect") === 0 && palette.opened, "Enter pending activated an old result or command")
            inFlight = palette.searchRequestId
            key = Qt.Key_Enter
            phase = 5
            return 0
        }
        if (phase === 5) {
            check(searches().length === beforeCount + 1 && palette.searchRequestId === inFlight, "keypad Enter duplicated flight")
            client.responseReceived(inFlight, page([game("keyboard-result")]))
            check(count("catalog.launch.inspect") === 0 && palette.opened, "results auto-launched")
            key = Qt.Key_Return
            phase = 6
            return 0
        }
        if (phase === 6) {
            check(!palette.opened && ShellStore.selectedGame.id === "keyboard-result", "result Enter did not select through DesktopApp")
            check(count("catalog.launch.inspect") === 1 && count("session.create") === 0 && count("catalog.ownership.add") === 0,
                "remote activation bypassed the common launch guard")
            const inspection = client.requests.find(item => item.id === ShellStore.launchInspectRequestId)
            check(inspection.params.appId === "keyboard-result" && inspection.params.variantId === "123", "launch guard lost exact variant")
            client.responseReceived(inspection.id, {appId:"keyboard-result",variantId:"123",game:game("keyboard-result"),
                scope:ShellStore.catalogOwnerState.authScope,decision:{status:"ownership_required",message:"Synthetic ownership requires confirmation."}})
            check(count("session.create") === 0 && count("catalog.ownership.add") === 0, "unowned result bypassed admission")
            AppController.navigate("home")
            ShellStore.lastError = ""
            ShellStore.streamState = "idle"
            key = Qt.Key_K
            modifiers = Qt.ControlModifier
            phase = 7
            return 0
        }
        if (phase === 7) {
            check(palette.opened && field.activeFocus, "Ctrl+K did not reopen with input focus")
            key = Qt.Key_Tab
            modifiers = 0
            phase = 8
            return 0
        }
        if (phase === 8) {
            check(palette.scopeFilter === "games", "Tab did not cycle scope")
            key = Qt.Key_Escape
            phase = 9
            return 0
        }
        if (phase === 9) {
            check(!palette.opened, "Escape did not close palette")
            palette.parent.commandOpen = true
            palette.scopeFilter = "actions"
            typeQuery("Open Library")
            key = Qt.Key_Return
            phase = 10
            return 0
        }
        if (phase === 10) {
            check(AppController.route === "library" && !palette.opened, "actions-only Enter lost local command behavior")
            AppController.navigate("home")
            phase = 17
            return 0
        }
        if (phase === 17) {
            palette.parent.commandOpen = true
            inFlight = submit("settings")
            key = Qt.Key_Down
            phase = 15
            return 0
        }
        if (phase === 15) {
            check(palette.currentIndex === 1 && palette.actionList[1].route === "settings-streaming",
                "Down did not select the pending local command: "
                    + JSON.stringify([palette.currentIndex,palette.query,palette.scopeFilter,palette.opened,field.activeFocus,palette.actionList]))
            client.responseReceived(inFlight, page([game("first-inserted"),game("second-inserted")]))
            check(palette.currentIndex === palette.gameList.length + 1
                && palette.actionList[palette.currentIndex - palette.gameList.length].route === "settings-streaming",
                "arriving games replaced the selected local command")
            key = Qt.Key_Return
            phase = 16
            return 0
        }
        if (phase === 16) {
            check(AppController.route === "settings-streaming" && !palette.opened
                && ShellStore.selectedGame.id === "keyboard-result" && count("session.create") === 0
                && !client.requests.some(item => item.method === "catalog.launch.inspect"
                    && ["first-inserted","second-inserted"].indexOf(item.params.appId) >= 0),
                "Enter did not activate the preserved local command: "
                    + JSON.stringify([AppController.route,palette.opened,count("catalog.launch.inspect"),count("session.create"),palette.currentIndex,field.activeFocus]))
            AppController.navigate("home")
            phase = 11
            return 0
        }
        if (phase === 11) {
            if (Qt.application.arguments.indexOf("--search-scaled-light") >= 0)
                DesktopTokens.uiScale = 1.4
            palette.parent.commandOpen = true
            typeQuery("settings")
            palette.acceptCurrent()
            if (Qt.application.arguments.indexOf("--search-normal") >= 0)
                client.responseReceived(palette.searchRequestId, page([game("Settings adventure"), game("Settings studio")], true))
            else if (Qt.application.arguments.indexOf("--search-scaled-light") >= 0) {
                const invalid = page([game("rejected")])
                invalid.catalogRevision = -1
                client.responseReceived(palette.searchRequestId, invalid)
            }
            key = Qt.Key_Down
            phase = 12
            return 0
        }
        if (phase === 12) {
            if (Qt.application.arguments.indexOf("--search-scaled-light") >= 0)
                check(Theme.lightMode && DesktopTokens.uiScale === 1.4, "scaled-light fixture did not apply its theme and scale")
            check(palette.currentIndex === palette.gameList.length + 1, "Down key did not preserve the selected action: "
                + JSON.stringify([palette.currentIndex,palette.flatCount,field.activeFocus,palette.opened]))
            const panel = find(palette, "commandPalettePanel")
            const status = find(palette, "commandSearchStatus")
            check(panel.width <= host.width - 32 && panel.y + panel.height <= host.height - 16, "palette is outside the viewport")
            check(status.visible && status.text !== "" && palette.actionList.length > 0, "search status disappeared beside actions")
            const row = palette.currentRow()
            const viewport = find(palette, "commandPaletteResults")
            check(row.y >= viewport.contentY && row.y + row.height <= viewport.contentY + viewport.height + 1,
                "keyboard selection is outside the scrolling viewport")
            console.info("Command search: all lifecycle, debounce, keyboard, isolation and launch checks passed")
            return 1
        }
        return -1
    }
}
