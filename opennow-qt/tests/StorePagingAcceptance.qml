import QtQuick
import OpenNOW

QtObject {
    property Component paletteComponent: Component { DesktopCommandPalette {} }
    property Component hostComponent: Component { Item {} }
    property Component shellComponent: Component { DesktopShell {} }
    property Component cardComponent: Component { DesktopStoreCard {} }
    property Component shelfComponent: Component { DesktopStoreShelf {} }
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property int sequence: 0
        property var requests: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {} // No filesystem writes from the isolated mock.
        function request(method, params, timeout) {
            const id = "store-test-" + (++sequence)
            requests.push({id:id, method:method, params:params})
            return id
        }
        function cancel(id) { requestFailed(id, "cancelled", "Cancelled"); return true }
    }
    function check(ok, message) { if (!ok) throw new Error("Store pagination: " + message) }
    function namedChild(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = namedChild(child, name)
            if (found) return found
        }
        return null
    }
    function game(id) {
        return {id:id, uuid:id, title:"Test game " + id, imageUrl:"", heroImageUrl:"",
            variants:[{id:id, store:"Steam", inLibrary:false}], availableStores:["Steam"], genres:[]}
    }
    function page(games, cursor, more) {
        return {games:games, totalCount:2500, nextCursor:cursor, hasNextPage:more}
    }
    function run(screen, status, retry, emptyStatus) {
        const artworkSources = []
        for (let i = 0; i < 6; ++i) {
            const source = "https://artwork.test/" + i
            artworkSources.push(source)
            ShellStore.retainArtwork(source)
            ShellStore.scheduleArtworkRetry(source)
            ShellStore.artworkRetrySources[source].nextAt = 0
        }
        const beforeArtwork = client.requests.length
        ShellStore.retryVisibleArtwork(Date.now())
        check(client.requests.length === beforeArtwork + 2, "artwork retries were not paced")
        const failedArtwork = client.requests[beforeArtwork]
        client.requestFailed(failedArtwork.id, "network", "Offline")
        check(ShellStore.artworkRetrySources[artworkSources[0]].failures === 2,
            "artwork failure did not increase backoff")
        ShellStore.retainArtwork(artworkSources[0])
        ShellStore.releaseArtwork(artworkSources[0])
        check(ShellStore.artworkInterests[artworkSources[0]] === 1, "shared artwork interest lost")
        for (const source of artworkSources) ShellStore.releaseArtwork(source)
        const afterArtwork = client.requests.length
        ShellStore.retryVisibleArtwork(Date.now() + 3600000)
        check(client.requests.length === afterArtwork && Object.keys(ShellStore.artworkRetrySources).length === 0,
            "hidden artwork kept retrying")
        check(screen.catalogPrice(game("unpriced")) === "", "invented Available price label")
        for (const store of ["STEAM", "EPIC", "XBOX", "UPLAY", "EA_APP", "GOG", "BATTLENET", "GAIJIN", "NVIDIA"])
            check(DesktopTokens.storeIconUrl(store).endsWith(".svg"), "missing store icon " + store)
        check(DesktopTokens.storeLabel("UPLAY") === "Ubisoft Connect", "raw provider label")
        check(DesktopTokens.genreLabel("MASSIVELY_MULTIPLAYER") === "Massively Multiplayer", "raw genre label")
        ShellStore.authSession = {user:{userId:"store-fixture", displayName:"Store Test"}}
        ShellStore.reloadStoreForSession()
        const first = ShellStore.storeRequestId
        check(client.requests.find(r => r.id === first).params.limit === 40, "unbounded page requested")
        check(client.requests.find(r => r.id === first).method === "catalog.store.local", "Store did not use its local index")
        client.responseReceived(first, page([game("a"),game("b")], "cursor-one", true))
        check(ShellStore.storeGames.length === 2 && !ShellStore.storeLoading && ShellStore.storeHasMore, "Store automatically continued instead of waiting for demand")
        check(status.text.indexOf("2") >= 0, "progress not visible")
        ShellStore.requestStorePage()
        const second = ShellStore.storeRequestId
        check(client.requests.find(r => r.id === second).params.cursor === "cursor-one", "cursor not forwarded")
        client.requestFailed(second, "upstream_error", "Store HTTP 503")
        check(ShellStore.storeGames.length === 2 && status.text.indexOf("HTTP 503") >= 0, "partial games/error lost")
        retry.clicked()
        const retryId = ShellStore.storeRequestId
        check(client.requests.find(r => r.id === retryId).params.cursor === "cursor-one", "retry skipped failed page")
        client.responseReceived(retryId, page([game("b"),game("c")], "cursor-two", true))
        check(ShellStore.storeGames.length === 3, "overlapping pages not deduplicated")
        ShellStore.requestStorePage()
        client.responseReceived(ShellStore.storeRequestId, page([], "", false))
        check(!ShellStore.storeHasMore && !ShellStore.storeLoading && ShellStore.storeGames.length === 3, "final page did not settle")
        client.requestFailed("unrelated", "other_error", "Unrelated account error")
        check(ShellStore.storeError === "", "unrelated error leaked into Store")

        const presentation = ShellStore.storePresentationRequestId
        client.requestFailed(presentation, "upstream_error", "Banner unavailable")
        check(ShellStore.storeGames.length === 3 && ShellStore.storeWarning.indexOf("Banner unavailable") >= 0, "chrome failure broke games")

        const cachedCount = client.requests.length
        ShellStore.ensureStore("")
        check(client.requests.length === cachedCount && ShellStore.storeGames.length === 3, "route re-entry reloaded the catalog")
        ShellStore.ensureStore("cached search")
        client.responseReceived(ShellStore.storeRequestId, page([game("search-only")], "", false))
        ShellStore.ensureStore("")
        check(ShellStore.storeGames.length === 3 && ShellStore.storeGames[0].id === "a"
            && !ShellStore.storeLoading, "clearing search did not restore browse cache")
        ShellStore.ensureStore("", {categoryId:"shelf:0:0"})
        client.responseReceived(ShellStore.storeRequestId, page([game("category-only")], "", false))
        const filteredRequestCount = client.requests.filter(r => r.method === "catalog.store.local").length
        ShellStore.ensureStore("", {})
        check(ShellStore.storeGames.length === 3 && ShellStore.storeGames[0].id === "a"
            && client.requests.filter(r => r.method === "catalog.store.local").length === filteredRequestCount, "leaving a category reloaded the browse cache")
        ShellStore.refreshStore("", true)
        check(ShellStore.storeLoading && ShellStore.storeGames.length === 3, "manual refresh did not retain artwork")
        check(client.requests.find(r => r.id === ShellStore.storeRequestId).params.refresh === true, "manual refresh did not request a local index rebuild")
        client.responseReceived(ShellStore.storeRequestId, page([game("resume")], "resume-next", true))
        const inFlightCount = client.requests.length
        ShellStore.ensureStore("")
        check(client.requests.length === inFlightCount, "route re-entry restarted pagination")
        ShellStore.ensureStore("interrupt browse")
        const cancelledSearch = ShellStore.storeRequestId
        ShellStore.ensureStore("")
        client.responseReceived(cancelledSearch, page([game("late-search")], "", false))
        ShellStore.requestStorePage()
        check(client.requests.find(r => r.id === ShellStore.storeRequestId).params.cursor === "resume-next"
            && ShellStore.storeGames[0].id === "resume", "cached partial browse did not resume at its cursor")
        check(client.requests.find(r => r.id === ShellStore.storeRequestId).params.refresh === false, "continuation invalidated disk cache")
        client.responseReceived(ShellStore.storeRequestId, page([], "", false))

        ShellStore.refreshStore("first search")
        const stale = ShellStore.storeRequestId
        const stalePresentation = ShellStore.storePresentationRequestId
        ShellStore.refreshStore("second search")
        client.responseReceived(stale, page([game("stale")], "", false))
        if (stalePresentation) client.responseReceived(stalePresentation, {section:"panels", items:[{id:"stale"}]})
        check(ShellStore.storeGames.length === 0, "cancelled search mixed results")
        const search = ShellStore.storeRequestId
        check(client.requests.find(r => r.id === search).params.searchQuery === "second search", "search context lost")
        client.requestFailed(search, "graphql_error", "GraphQL dependency failed")
        check(emptyStatus.text === "GraphQL dependency failed", "initial failure hidden")
        ShellStore.retryStore()
        client.responseReceived(ShellStore.storeRequestId, page([game("fresh")], "repeat", true))
        ShellStore.requestStorePage()
        client.responseReceived(ShellStore.storeRequestId, page([game("wrong")], "repeat", true))
        check(ShellStore.storeState === "error" && ShellStore.storeGames.length === 1, "repeated cursor continued")

        // Filtering/ranking belongs to the complete local index, not the
        // currently materialized games or a second QML substring matcher.
        ShellStore.ensureStore("fortntie", {genre:"ACTION"})
        check(client.requests.find(r => r.id === ShellStore.storeRequestId).params.genre === "ACTION", "local facet not forwarded")
        const ranked = game("fortnite")
        ranked.title = "Fortnite"
        client.responseReceived(ShellStore.storeRequestId, {source:"store-local",games:[ranked],
            totalCount:1,hasNextPage:false,nextCursor:"",facets:{genres:["ACTION","MUSIC"],
                stores:["EPIC","STEAM"],categories:[{id:"shelf:0:0",label:"All official games",count:90}]}})
        check(screen.filteredCatalog.length === 1, "ranked match was discarded by UI substring filtering")
        check(screen.genreOptions.indexOf("MUSIC") >= 0 && screen.categoryOptions.indexOf("shelf:0:0") >= 0,
            "facets were restricted to visible games")

        const paletteHost = hostComponent.createObject(screen, {width:480, height:320})
        const palette = paletteComponent.createObject(paletteHost, {opened:true})
        check(palette !== null, "palette fixture failed to load")
        palette.query = "fortntie"
        palette.requestGames()
        const paletteFirst = palette.searchRequestId
        check(client.requests.find(r => r.id === paletteFirst).params.limit === 6, "palette search was unbounded")
        palette.query = "cs2"
        palette.requestGames()
        client.responseReceived(paletteFirst, {games:[ranked]})
        check(palette.localGames.length === 0, "stale palette search was accepted")
        client.responseReceived(palette.searchRequestId, {games:[game("cs2")]})
        check(palette.gameList.length === 1 && palette.gameList[0].id === "cs2", "palette ignored ranked local results")
        palette.query = ""
        palette.scopeFilter = "actions"
        palette.ensureCurrentVisible()
        check(palette.panelWidth <= paletteHost.width - 32 && palette.panelTop + palette.panelHeight <= paletteHost.height - 16,
            "command palette extends beyond the window")
        palette.moveCurrent(-1)
        const paletteViewport = namedChild(palette, "commandPaletteResults")
        const lastCommand = palette.currentRow()
        check(paletteViewport && lastCommand && lastCommand.y >= paletteViewport.contentY
            && lastCommand.y + lastCommand.height <= paletteViewport.contentY + paletteViewport.height + 1,
            "keyboard selection is outside the command viewport")
        palette.moveCurrent(1)
        check(palette.currentRow().y >= paletteViewport.contentY, "wrapped selection was not scrolled back")
        palette.opened = false
        palette.destroy()
        paletteHost.destroy()

        const originalSettings = ShellStore.settings
        const originalRemotes = ShellStore.remoteSessions
        ShellStore.applySetting("desktopRailCollapsed", false)
        ShellStore.remoteSessions = [{status:2, appName:"A very long game name: Complete Edition"}]
        const originalScale = DesktopTokens.uiScale
        DesktopTokens.uiScale = 1.4
        const headerHost = hostComponent.createObject(screen, {width:960, height:540})
        const shell = shellComponent.createObject(headerHost, {title:"A long translated library heading", subtitle:"A long translated subtitle"})
        const heading = namedChild(shell, "desktopHeaderHeading")
        const searchField = namedChild(shell, "desktopHeaderSearch")
        const resumeButton = namedChild(shell, "desktopHeaderResume")
        check(resumeButton.visible && heading.x + heading.width <= searchField.x
            && searchField.x + searchField.width <= resumeButton.x
            && resumeButton.x + resumeButton.width <= resumeButton.parent.width,
            "header controls overlap with a pinned sidebar and resumable session")
        headerHost.width = 800
        check(heading.width > 0 && heading.x + heading.width <= searchField.x
            && searchField.x + searchField.width <= resumeButton.x,
            "header did not adapt to a narrower viewport")
        shell.destroy()
        headerHost.destroy()
        ShellStore.settings = originalSettings
        ShellStore.remoteSessions = originalRemotes
        DesktopTokens.uiScale = originalScale

        for (const mode of ["light", "dark"]) {
            ShellStore.applySetting("appTheme", mode)
            for (const scale of [0.9, 1.4]) {
                DesktopTokens.uiScale = scale
                const longGame = game("layout")
                longGame.title = "A much longer game title: Complete Ultimate Edition"
                const card = cardComponent.createObject(screen, {game:longGame, owned:true})
                const title = namedChild(card, "storeCardTitle")
                const metadata = namedChild(card, "storeCardMetadata")
                check(title.color.toString() === Theme.label.toString(), "Store caption ignores theme")
                check(title.y >= card.artHeight && metadata.y >= title.y + title.height
                    && metadata.y + metadata.height <= card.height,
                    "scaled Store metadata extends outside its card")
                card.destroy()
            }
        }
        ShellStore.settings = originalSettings
        DesktopTokens.uiScale = originalScale

        const shelf = shelfComponent.createObject(screen, {width:600,materialized:false,categoryId:"shelf:0:0",totalCount:90})
        const hiddenRequestCount = client.requests.length
        shelf.requestVisible()
        check(client.requests.length === hiddenRequestCount, "offscreen shelf requested games")
        shelf.materialized = true
        shelf.requestVisible()
        check(client.requests.find(r => r.id === shelf.requestId).params.limit === shelf.tileCount,
            "visible shelf requested more than its visible tiles")
        const shelfPending = shelf.requestId
        shelf.materialized = false
        client.responseReceived(shelfPending, {games:[ranked]})
        check(shelf.localGames.length === 0, "hidden shelf retained a cancelled response")
        shelf.materialized = true
        shelf.requestVisible()
        client.responseReceived(shelf.requestId, {games:[ranked]})
        const loadedShelfRequests = client.requests.length
        shelf.materialized = false
        check(shelf.localGames.length === 0, "hidden shelf retained visual model")
        shelf.materialized = true
        shelf.requestVisible()
        check(client.requests.length === loadedShelfRequests && shelf.localGames[0].id === ranked.id,
            "returning to a shelf repeated the core request")
        shelf.width = 400
        shelf.requestVisible()
        check(client.requests.length === loadedShelfRequests, "shrinking a shelf fetched again")
        ShellStore.resetStoreShelves()
        check(shelf.localGames.length === 0 && ShellStore.storeShelfCache.length === 0,
            "refresh retained stale shelf data")
        for (let i = 0; i < 30; ++i) ShellStore.cacheStoreShelf("test" + i, 1, [ranked])
        check(ShellStore.storeShelfCache.length === 24 && ShellStore.cachedStoreShelf("test0", 1) === null,
            "shelf cache was not bounded")
        ShellStore.resetStoreShelves()
        shelf.categoryId = ""
        shelf.width = 600
        shelf.games = [ranked]
        check(shelf.tileWidth < 160, "short final row stretched its posters")
        shelf.destroy()

        // An account switch cancels both channels, and late results cannot leak.
        ShellStore.retryStore()
        const oldAccount = ShellStore.storeRequestId
        const oldChrome = ShellStore.storePresentationRequestId
        ShellStore.authSession = null
        ShellStore.reloadStoreForSession()
        client.responseReceived(oldAccount, page([game("private")], "", false))
        if (oldChrome) client.responseReceived(oldChrome, {section:"marquee", items:[{title:"private"}]})
        check(ShellStore.storeGames.length === 0 && ShellStore.storeMarquee.length === 0, "old account results leaked")
        check(ShellStore.storeBrowseCache === null, "browse cache survived account change")
        check(client.requests.find(r => r.id === ShellStore.storeRequestId).method === "catalog.public.list", "signed-out path changed")
        client.responseReceived(ShellStore.storeRequestId, {games:[], totalCount:0})
        check(ShellStore.storeState === "ready" && !ShellStore.storeLoading, "empty catalog never settled")
        ShellStore.cancelStoreRequests()

        // Leave a partial-failure state for native screenshot/layout acceptance.
        ShellStore.authSession = {user:{userId:"store-fixture", displayName:"Store Test"}}
        ShellStore.reloadStoreForSession()
        const preview = []
        for (let i = 0; i < 24; ++i) preview.push(game(String(i)))
        client.responseReceived(ShellStore.storeRequestId, page(preview, "preview-next", true))
        ShellStore.requestStorePage()
        client.requestFailed(ShellStore.storeRequestId, "upstream_error", "Store HTTP 503 — the next catalog page is temporarily unavailable.")
        ShellStore.cancelStoreRequests()
        check(status.text.indexOf("24") >= 0 && retry.visible, "partial-error feedback is not actionable")
        return true
    }

    function prepareAppearance(screen, genreMenu) {
        const stores = ["STEAM","EPIC","XBOX","NONE","UPLAY","EA_APP","GOG","BATTLENET","GAIJIN"]
        const preview = []
        for (let i = 0; i < 24; ++i) {
            const entry = game(String(i))
            entry.title = "A longer game title: Definitive Edition " + i
            entry.availableStores = [stores[i % stores.length]]
            entry.variants[0].store = stores[i % stores.length]
            entry.genres = ["ACTION", "MASSIVELY_MULTIPLAYER", "ROLE_PLAYING", "ADVENTURE", "SIMULATION", "STRATEGY", "INDIE", "SPORTS"]
            preview.push(entry)
        }
        ShellStore.storeGames = preview
        ShellStore.storeHasMore = false
        ShellStore.storeError = ""
        ShellStore.storeWarning = ""
        ShellStore.applySetting("desktopRailCollapsed", false)
        ShellStore.authSession = {user:{userId:"store-fixture", displayName:"A very long account display name"}}
        check(screen.storeOptions.length === 10, "store choices were truncated")
        screen.openMenu = genreMenu ? "genre" : "store"
        return true
    }
}
