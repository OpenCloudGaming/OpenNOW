import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property var appController
    required property bool ready
    required property bool signedIn
    required property var settings
    required property var setSetting
    required property var applySetting
    signal accessibilityAnnounced(string message)
    signal storeSessionReset()
    property var catalogGames: []
    property var selectedGame: null
    property int catalogTotalCount: 0
    property string catalogState: "idle"
    readonly property bool catalogBusy: catalogState === "loading" || catalogState === "refreshing"
    property string catalogSource: "public"
    property string catalogRequestId: ""
    property bool catalogComplete: false
    property string catalogError: ""
    property string catalogNextCursor: ""
    property double catalogLastCompleteAt: 0
    property var catalogRevision: null
    property string catalogContext: ""
    readonly property string requestContextKey: JSON.stringify([settings.sessionProxyEnabled,
        settings.sessionProxyUrl, settings.region, settings.providerRegions])
    onRequestContextKeyChanged: if (ready && signedIn) reloadCatalogForSession()
    property string catalogTraversalId: ""
    property var catalogStaged: []
    property var catalogSeenCursors: Object.create(null)
    property int catalogStagedBytes: 0
    property double catalogSliceStarted: 0
    property var acceptsScope: function(scope) { return true }
    property var authScope: null
    property bool detailVisible: false
    property var definitions: ({})
    property string detailRequestId: ""
    property string detailAppId: ""
    property string detailError: ""
    property string detailState: "idle"
    property string detailVariantId: ""
    property var selectedLaunchDecision: ({status: "metadata_unconfirmed", message: ""})
    readonly property string selectedIdentity: selectedGame ? String(selectedGame.id || "") + ":" + String(((selectedGame.variants || [])[Number(selectedGame.selectedVariantIndex || 0)] || {}).id || "") : ""
    property int actionGeneration: 0
    property var ownershipConfirmation: null
    property string mutationRequestId: ""
    property var mutationTarget: null
    property string mutationState: "idle"
    property string mutationMessage: ""
    readonly property bool mutationBusy: mutationRequestId !== ""
    property var remoteFavorites: []
    property string favoritesRequestId: ""
    property string favoritesState: "idle"
    property string favoritesError: ""
    property var favoritesSections: []
    signal launchContextInvalidated()
    onSelectedIdentityChanged: {
        ownershipConfirmation = null
        selectedLaunchDecision = ({status: "metadata_unconfirmed", message: ""})
        cancelDetailRequest()
        if (detailVisible) Qt.callLater(refreshSelectedMetadata)
    }
    property Timer detailRefreshTimer: Timer {
        interval: 30000
        repeat: true
        running: root.ready && root.signedIn && root.detailVisible
        onTriggered: root.refreshSelectedMetadata()
    }
    onDetailVisibleChanged: {
        if (detailVisible) refreshSelectedMetadata()
        else cancelDetailRequest()
    }
    property Connections detailResponses: Connections {
        target: root.coreClient
        function onResponseReceived(requestId, result) {
            if (requestId === "" || requestId !== root.detailRequestId) return
            root.detailRequestId = ""
            if (!root.acceptsScope(result.scope) || !result.game || !root.selectedGame
                    || String(result.game.id) !== root.detailAppId || String(root.selectedGame.id) !== root.detailAppId) return
            if (result.variantId !== root.detailVariantId) return
            root.adoptGame(result.game)
            root.selectedLaunchDecision = result.decision || ({status:"metadata_unconfirmed", message:""})
            root.detailState = "ready"
            root.detailError = ""
        }
        function onRequestFailed(requestId, code, message) {
            if (requestId === "" || requestId !== root.detailRequestId) return
            root.detailRequestId = ""
            root.detailState = "stale"
            root.detailError = message
            root.selectedLaunchDecision = ({status:"metadata_unconfirmed", message:message})
        }
    }

    function cancelDetailRequest() {
        const id = detailRequestId
        detailRequestId = ""
        if (id !== "") coreClient.cancel(id)
    }

    function refreshSelectedMetadata() {
        if (!ready || !signedIn || !detailVisible || !selectedGame || detailRequestId !== "") return
        detailAppId = String(selectedGame.id || "")
        detailVariantId = String(((selectedGame.variants || [])[Number(selectedGame.selectedVariantIndex || 0)] || {}).id || "")
        if (!detailAppId || !detailVariantId) return
        detailState = "loading"
        detailRequestId = coreClient.request("catalog.launch.inspect", {appId: detailAppId, variantId: detailVariantId}, 30000)
    }

    function adoptGame(game) {
        function replace(items) { return items.map(item => String(item.id) === String(game.id) ? game : item) }
        catalogGames = replace(catalogGames)
        storeGames = replace(storeGames)
        remoteFavorites = game.favorited === false
            ? remoteFavorites.filter(item => String(item.id) !== String(game.id)) : replace(remoteFavorites)
        if (selectedGame && String(selectedGame.id) === String(game.id)) {
            const id = String(((selectedGame.variants || [])[Number(selectedGame.selectedVariantIndex || 0)] || {}).id || "")
            const index = (game.variants || []).findIndex(variant => String(variant.id) === id)
            selectedGame = Object.assign({}, game, {selectedVariantIndex:index})
        }
    }

    function resetCloudActions() {
        actionGeneration++
        ownershipConfirmation = null
        const ids = [mutationRequestId, favoritesRequestId]
        mutationRequestId = ""
        favoritesRequestId = ""
        mutationTarget = null
        mutationState = "idle"
        mutationMessage = ""
        favoritesState = "idle"
        favoritesError = ""
        remoteFavorites = []
        selectedLaunchDecision = ({status:"metadata_unconfirmed", message:""})
        for (const id of ids) if (id !== "") coreClient.cancel(id)
        launchContextInvalidated()
    }

    function refreshFavorites() {
        if (!ready || !signedIn || favoritesRequestId !== "") return
        favoritesState = "loading"
        favoritesError = ""
        favoritesRequestId = coreClient.request("catalog.favorites.list", {}, 30000)
    }

    function isCloudFavorite(game) {
        return game && (game.favorited === true || (game.favorited !== false && remoteFavorites.some(item => String(item.id) === String(game.id))))
    }

    function requestOwnershipConfirmation(action) {
        if (!ready || !signedIn || !selectedGame || mutationBusy) return
        const variant = (selectedGame.variants || [])[Number(selectedGame.selectedVariantIndex || 0)]
        if (!variant || ["add", "remove"].indexOf(action) < 0) return
        ownershipConfirmation = {appId:String(selectedGame.id), variantId:String(variant.id), store:String(variant.store || ""), action:action, generation:actionGeneration, identity:selectedIdentity}
    }

    function confirmOwnership() {
        const target = ownershipConfirmation
        ownershipConfirmation = null
        if (!target || target.generation !== actionGeneration || target.identity !== selectedIdentity) return
        mutateGame("catalog.ownership." + target.action, target.appId, target.variantId, target.action === "add")
    }

    function mutateGame(method, appId, variantId, confirmed) {
        if (!ready || !signedIn || mutationBusy) return
        mutationTarget = {appId:appId, variantId:variantId, generation:actionGeneration}
        mutationState = "updating"
        mutationMessage = qsTr("Updating your GeForce NOW library…")
        selectedLaunchDecision = ({status:"metadata_unconfirmed", message:mutationMessage})
        cancelDetailRequest()
        const favoriteRead = favoritesRequestId
        favoritesRequestId = ""
        if (favoriteRead !== "") coreClient.cancel(favoriteRead)
        launchContextInvalidated()
        mutationRequestId = coreClient.request(method, {appId:appId, variantId:variantId, confirmedExistingLicense:confirmed === true, scope:authScope}, 60000)
    }

    function toggleCloudFavorite(game) {
        if (game) mutateGame(isCloudFavorite(game) ? "catalog.favorites.remove" : "catalog.favorites.add", String(game.id), "", false)
    }

    function selectPreferredVariant() {
        const variant = selectedGame && (selectedGame.variants || [])[Number(selectedGame.selectedVariantIndex || 0)]
        if (variant) mutateGame("catalog.ownership.select", String(selectedGame.id), String(variant.id), false)
    }

    property Connections cloudActionResponses: Connections {
        target: root.coreClient
        function onResponseReceived(id, result) {
            if (id !== "" && id === root.favoritesRequestId) {
                root.favoritesRequestId = ""
                if (!root.acceptsScope(result.scope)) return
                root.remoteFavorites = result.games || []
                root.favoritesSections = result.sections || []
                root.favoritesState = "partial"
            } else if (id !== "" && id === root.mutationRequestId) {
                root.mutationRequestId = ""
                const target = root.mutationTarget
                root.mutationTarget = null
                if (!target || target.generation !== root.actionGeneration || !root.acceptsScope(result.scope)) return
                if (result.appId !== target.appId || result.variantId !== target.variantId) {
                    root.mutationState = "unconfirmed"
                    root.mutationMessage = qsTr("The update returned a different game. Refresh this game before trying again.")
                    root.refreshSelectedMetadata()
                    return
                }
                root.mutationState = result.reconciliation === "confirmed" ? "confirmed" : "unconfirmed"
                root.mutationMessage = result.message || qsTr("Could not confirm the update. Refresh this game before trying again.")
                if (result.game && String(result.game.id) === target.appId) root.adoptGame(result.game)
                root.refreshCatalogAfterAccountChange()
                root.refreshSelectedMetadata()
                root.refreshFavorites()
            }
        }
        function onRequestFailed(id, code, message) {
            if (id !== "" && id === root.favoritesRequestId) {
                root.favoritesRequestId = ""
                root.favoritesState = "error"
                root.favoritesError = message
            } else if (id !== "" && id === root.mutationRequestId) {
                root.mutationRequestId = ""
                root.mutationTarget = null
                root.mutationState = "unconfirmed"
                root.mutationMessage = qsTr("Could not confirm the update. Refresh this game before trying again.") + " " + message
                root.refreshSelectedMetadata()
                root.refreshCatalogAfterAccountChange()
                root.refreshFavorites()
            }
        }
    }

    function genreLabel(genre) {
        const items = definitions.genres && definitions.genres.items || []
        const definition = items.find(item => item.genre === genre)
        return definition ? definition.label : ""
    }

    function readinessNotice(game) {
        if (!game) return ""
        const variant = (game.variants || [])[Number(game.selectedVariantIndex || 0)]
        if (!variant) return ""
        const patch = variant.stateDetails
        let notice = ""
        if (patch && patch.__typename === "VariantGfnMaintenanceMetadata") notice = qsTr("This store version is under maintenance.")
        else if (patch && (patch.__typename === "VariantGfnAutoPatchingMetadata" || patch.__typename === "VariantGfnManualPatchingMetadata")) {
            notice = qsTr("This store version is being patched.")
            if (patch.historicalEtaMins > 0) notice += " " + qsTr("Past patches took about %1 minutes; this is not a completion time.").arg(Math.round(patch.historicalEtaMins))
        } else if (variant.playStatus === "NOT_PLAYABLE") notice = qsTr("This store version is currently unavailable.")
        const subscriptions = definitions.subscriptions && definitions.subscriptions.items || []
        const required = variant.subscriptions || []
        const labels = subscriptions.filter(item => required.indexOf(item.subscription) >= 0).map(item => item.label)
        if (labels.length) notice += (notice ? " " : "") + qsTr("Store subscriptions: %1. A subscription does not confirm ownership of this version.").arg(labels.join(", "))
        if (detailError) notice += (notice ? " " : "") + qsTr("Readiness could not be refreshed: %1").arg(detailError)
        return notice
    }
    signal libraryRefreshFinished(bool complete, string message)
    property Timer libraryPageTimer: Timer {
        interval: 1
        onTriggered: root.requestLibraryPage()
    }
    readonly property var gameCollections: settings.gameCollections || []
    property string activeCollectionId: ""
    readonly property var activeCollection: collectionById(activeCollectionId)
    property string collectionRequestId: ""
    readonly property bool collectionsBusy: collectionRequestId !== ""
    property string collectionError: ""
    property string pendingCollectionId: ""
    signal collectionSaved(string collectionId)

    onGameCollectionsChanged: {
        if (activeCollectionId && !collectionById(activeCollectionId))
            activeCollectionId = ""
    }
    onReadyChanged: {
        if (!ready) {
            resetCloudActions()
            cancelCatalogRequest()
            cancelDetailRequest()
            catalogNextCursor = ""
            catalogStaged = []
            catalogState = catalogGames.length ? "partial" : "idle"
            if (catalogGames.length) {
                catalogError = qsTr("The core restarted. Refresh the library to confirm its current contents.")
            }
        }
        if (!ready && collectionsBusy) {
            collectionRequestId = ""
            collectionError = qsTr("The collection could not be saved. Reconnect and try again.")
        }
    }

    property Connections collectionResponses: Connections {
        target: root.coreClient
        function onResponseReceived(requestId, result) {
            if (!root.collectionRequestId || requestId !== root.collectionRequestId)
                return
            root.applySetting("gameCollections", result.value)
            root.collectionRequestId = ""
            root.collectionError = ""
            root.collectionSaved(root.pendingCollectionId)
        }
        function onRequestFailed(requestId, code, message) {
            if (!root.collectionRequestId || requestId !== root.collectionRequestId)
                return
            root.collectionRequestId = ""
            root.collectionError = message
        }
    }

    function collectionById(id) {
        return gameCollections.find(collection => collection.id === id) || null
    }

    function isInCollection(game, collectionId) {
        const collection = collectionById(collectionId)
        const id = gameIdentity(game)
        return collection !== null && id !== "" && collection.gameIds.indexOf(id) >= 0
    }

    function collectionNameError(name, exceptId) {
        const trimmed = name.trim()
        if (!trimmed)
            return qsTr("Enter a collection name.")
        if (trimmed.length > 80)
            return qsTr("Use 80 characters or fewer.")
        if (gameCollections.some(collection => collection.id !== exceptId
                && collection.name.toLocaleLowerCase() === trimmed.toLocaleLowerCase()))
            return qsTr("A collection with this name already exists.")
        return ""
    }

    function saveCollections(collections, collectionId) {
        if (collectionsBusy)
            return false
        collectionError = ""
        if (!ready) {
            collectionError = qsTr("The OpenNOW core is not ready")
            return false
        }
        pendingCollectionId = collectionId
        collectionRequestId = setSetting("gameCollections", collections)
        return collectionRequestId !== ""
    }

    function createCollection(name, game) {
        collectionError = collectionNameError(name, "")
        if (collectionError)
            return false
        if (gameCollections.length >= 100) {
            collectionError = qsTr("You can create up to 100 collections.")
            return false
        }
        let id = ""
        do {
            id = "collection-" + Date.now().toString(36) + "-" + Math.random().toString(36).slice(2, 12)
        } while (collectionById(id))
        const gameId = gameIdentity(game)
        return saveCollections(gameCollections.concat([{id: id, name: name.trim(), gameIds: gameId ? [gameId] : []}]), id)
    }

    function renameCollection(id, name) {
        collectionError = collectionNameError(name, id)
        if (collectionError || !collectionById(id))
            return false
        return saveCollections(gameCollections.map(collection => collection.id === id
            ? {id: id, name: name.trim(), gameIds: collection.gameIds} : collection), id)
    }

    function deleteCollection(id) {
        if (!collectionById(id))
            return false
        return saveCollections(gameCollections.filter(collection => collection.id !== id), "")
    }

    function toggleCollectionGame(id, game) {
        const collection = collectionById(id)
        const gameId = gameIdentity(game)
        if (!collection || !gameId)
            return false
        const ids = collection.gameIds.indexOf(gameId) >= 0
            ? collection.gameIds.filter(value => value !== gameId) : collection.gameIds.concat([gameId])
        return saveCollections(gameCollections.map(value => value.id === id
            ? {id: id, name: value.name, gameIds: ids} : value), id)
    }

    // Store channel: the full CMS browse catalog (all games) for signed-in
    // users, static public list otherwise. Separate from the library channel
    // so store browsing never disturbs library counts or filters.
    property var storeGames: []
    property var storeFacets: ({genres:[], stores:[], categories:[]})
    property var storeFilters: ({})
    property bool storeUsesLocalIndex: false
    property int storeTotalCount: 0
    property string storeState: "idle"
    property string storeSource: "public"
    property string storeError: ""
    property string storeWarning: ""
    property string storeSearchQuery: ""
    property string storeNextCursor: ""
    property bool storeHasMore: false
    property bool storeReplacePage: true
    property int storePageCount: 0
    property var storeSeenCursors: ({})
    property string storePresentationRequestId: ""
    property int storePresentationIndex: 0

    // One account-scoped browse snapshot, not an unbounded cache of searches.
    // Arrays are shared until the next page replaces them; no deep catalog copy.
    property var storeBrowseCache: null
    property bool storeForceRefresh: false
    property bool storeLastPageCached: false
    property string storeRequestId: ""
    readonly property bool storeLoading: storeRequestId !== "" || storePageTimer.running
    property Timer storePageTimer: Timer {
        interval: root.storeLastPageCached ? 1 : 75
        onTriggered: root.requestStorePage()
    }

    // Storefront chrome from the CMS panels documents: marquee hero slides,
    // official shelves (GFN Thursday, per-store rows…), and filter groups.
    property var storeMarquee: []
    property var storePanels: []
    property var storeFilterGroups: []
    property var storeShelfCache: []
    property int storeShelfEpoch: 0

    function cachedStoreShelf(category, limit) {
        const entries = storeShelfCache.slice()
        const index = entries.findIndex(entry => entry.category === category && entry.limit >= limit)
        if (index < 0) return null
        const entry = entries.splice(index, 1)[0]
        entries.push(entry)
        storeShelfCache = entries
        return entry.games
    }

    function cacheStoreShelf(category, limit, games) {
        const entries = storeShelfCache.filter(entry => entry.category !== category)
        entries.push({category:category, limit:Math.min(60, limit), games:games.slice(0, 60)})
        storeShelfCache = entries.slice(-24)
    }

    function resetStoreShelves() {
        storeShelfCache = []
        storeShelfEpoch++
    }

    function ensureCatalog(searchQuery) {
        if (catalogState === "idle") refreshCatalog(searchQuery)
    }

    function refreshCatalog(searchQuery) {
        if (!ready || catalogBusy)
            return
        catalogState = catalogGames.length > 0 ? "refreshing" : "loading"
        catalogError = ""
        catalogSource = signedIn ? "account-library" : "public"
        if (!signedIn) {
            catalogRequestId = coreClient.request("catalog.public.list", {limit: 360, searchQuery: searchQuery || ""}, 30000)
            return
        }
        libraryPageTimer.stop()
        catalogTraversalId = String(Date.now()) + ":" + String(Math.random())
        catalogStaged = []
        catalogSeenCursors = Object.create(null)
        catalogStagedBytes = 0
        catalogNextCursor = ""
        catalogRevision = null
        catalogContext = ""
        catalogSliceStarted = Date.now()
        requestLibraryPage()
    }

    function requestLibraryPage() {
        if (!ready || !signedIn || catalogRequestId !== "") return
        libraryPageTimer.stop()
        catalogRequestId = coreClient.request("catalog.library.list", {
            limit: 100, cursor: catalogNextCursor, traversalId: catalogTraversalId,
            catalogRevision: catalogRevision, catalogContext: catalogContext
        }, 30000)
        if (catalogRequestId === "") failCatalog(qsTr("The library request could not start. Try again."))
    }

    function continueCatalog() {
        if (!ready || catalogBusy) return
        if (catalogNextCursor === "") {
            refreshCatalog("")
            return
        }
        catalogSliceStarted = Date.now()
        catalogError = ""
        catalogState = "refreshing"
        requestLibraryPage()
    }

    function reloadCatalogForSession() {
        resetCloudActions()
        Qt.callLater(refreshFavorites)
        cancelDetailRequest()
        detailState = "idle"
        detailError = ""
        selectedGame = null
        cancelCatalogRequest()
        catalogGames = []
        catalogComplete = false
        catalogLastCompleteAt = 0
        catalogState = "idle"
        refreshCatalog("")
        reloadStoreForSession()
    }

    function cancelCatalogRequest() {
        libraryPageTimer.stop()
        const previous = catalogRequestId
        catalogRequestId = ""
        if (previous !== "") coreClient.cancel(previous)
    }

    function refreshCatalogAfterAccountChange() {
        cancelDetailRequest()
        selectedLaunchDecision = ({status:"metadata_unconfirmed", message:""})
        ownershipConfirmation = null
        launchContextInvalidated()
        cancelCatalogRequest()
        catalogState = "idle"
        refreshCatalog("")
        reloadStoreForSession()
        Qt.callLater(refreshSelectedMetadata)
    }

    function ensureStore(searchQuery, filters) {
        const query = String(searchQuery || "").trim()
        const nextFilters = filters || ({})
        const sameFilters = JSON.stringify(nextFilters) === JSON.stringify(storeFilters)
        if (query === storeSearchQuery && sameFilters && (storeLoading || storeState !== "idle")) return
        const cached = storeBrowseCache
        if (query === "" && JSON.stringify(nextFilters) === "{}" && cached) {
            cancelStoreRequests()
            storeBrowseCache = null
            storeSearchQuery = ""
            storeFilters = ({})
            storeGames = cached.games
            storeTotalCount = cached.totalCount
            storeState = cached.state
            storeError = cached.error
            storeNextCursor = cached.nextCursor
            storeHasMore = cached.hasMore
            storeReplacePage = false
            storePageCount = cached.pageCount
            storeSeenCursors = cached.seenCursors
            storeForceRefresh = false
            storeLastPageCached = cached.lastPageCached
            requestStorePresentation()
            return
        }
        refreshStore(query, false, nextFilters)
    }

    function refreshStore(searchQuery, forceRefresh, filters) {
        if (!ready)
            return
        const query = String(searchQuery || "").trim()
        const nextFilters = filters || ({})
        const sameFilters = JSON.stringify(nextFilters) === JSON.stringify(storeFilters)
        if (storeLoading && storeSearchQuery === query && sameFilters && !forceRefresh) return
        if (!forceRefresh && storeSearchQuery === "" && JSON.stringify(storeFilters) === "{}" && (query !== "" || !sameFilters) && storePageCount > 0) {
            storeBrowseCache = {games: storeGames, totalCount: storeTotalCount,
                state: storeState, error: storeError, nextCursor: storeNextCursor,
                hasMore: storeHasMore, pageCount: storePageCount,
                seenCursors: storeSeenCursors, lastPageCached: storeLastPageCached, resume: storeLoading}
        }
        cancelStoreRequests()
        if (query !== storeSearchQuery || !sameFilters) {
            storeGames = []
            storeTotalCount = 0
        }
        if (forceRefresh) {
            resetStoreShelves()
            storeBrowseCache = null
            storePresentationIndex = 0
        }
        storeSearchQuery = query
        storeFilters = nextFilters
        storeForceRefresh = forceRefresh === true
        storeLastPageCached = false
        storeNextCursor = ""
        storeHasMore = true
        storeReplacePage = true
        storePageCount = 0
        storeSeenCursors = Object.create(null)
        storeError = ""
        storeWarning = ""
        storeSource = signedIn ? "store-browse" : "public"
        requestStorePage()
    }

    function cancelStoreRequests() {
        storePageTimer.stop()
        const pageId = storeRequestId
        const presentationId = storePresentationRequestId
        storeRequestId = ""
        storePresentationRequestId = ""
        // Clear ownership before cancel() emits its synchronous failure signal.
        if (pageId !== "") coreClient.cancel(pageId)
        if (presentationId !== "") coreClient.cancel(presentationId)
    }

    function requestStorePage() {
        if (!ready || storeRequestId !== "" || !storeHasMore) return
        storePageTimer.stop()
        storeError = ""
        storeState = storeGames.length > 0 ? "refreshing" : "loading"
        storeRequestId = coreClient.request(storeSource === "store-browse" ? "catalog.store.local" : "catalog.public.list", {
            limit: 40,
            cursor: storeNextCursor, searchQuery: storeSearchQuery,
            genre: storeFilters.genre || "", store: storeFilters.store || "", categoryId: storeFilters.categoryId || "",
            refresh: storeForceRefresh && storeNextCursor === ""
        }, 60000)
        if (storeRequestId === "") {
            storeState = "error"
            storeError = qsTr("Could not start the Store request. Try again.")
        }
    }

    function requestStorePresentation() {
        if (!ready || storeSource !== "store-browse" || storePresentationRequestId !== "" || storePresentationIndex >= 3) return
        storePresentationRequestId = coreClient.request("catalog.store.presentation", {
            section: ["marquee", "panels", "filters"][storePresentationIndex], metadataOnly:true
        }, 30000)
    }

    function retryStore() {
        if (storeLoading) return
        if (storeError !== "") requestStorePage()
        if (storeWarning !== "") {
            storeWarning = ""
            storePresentationIndex = 0
            requestStorePresentation()
        }
    }

    function acceptStorePage(result) {
        storeRequestId = ""
        const more = storeSource === "store-browse" && result.hasNextPage === true
        const next = String(result.nextCursor || "")
        if (!Array.isArray(result.games) || (more && (!next || next === storeNextCursor || storeSeenCursors[next]))) {
            storeState = "error"
            storeError = qsTr("The Store returned an invalid page. Try again.")
            return
        }
        const merged = storeReplacePage ? [] : storeGames.slice()
        const seen = Object.create(null)
        const identity = game => String(game.uuid || game.id || game.launchAppId || "")
        for (let i = 0; i < merged.length; ++i) seen[identity(merged[i])] = true
        for (let i = 0; i < result.games.length; ++i) {
            const game = result.games[i]
            const key = identity(game)
            if (key && !seen[key]) { merged.push(game); seen[key] = true }
        }
        storeGames = merged
        storeReplacePage = false
        storeForceRefresh = false
        storeLastPageCached = result.cacheHit === true
        storeUsesLocalIndex = result.source === "store-local"
        if (storeUsesLocalIndex && result.cacheComplete === false)
            storeWarning = qsTr("Browsing saved Store pages. These results and filters cover only the catalog loaded so far.")
        storePageCount += 1
        storeTotalCount = Math.max(merged.length, Number(result.totalCount || 0))
        if (result.facets) storeFacets = result.facets
        storeHasMore = more
        storeNextCursor = next
        if (next) storeSeenCursors[next] = true
        storeState = "ready"
        if (storePageCount === 1) requestStorePresentation()
        // Demand-driven: the viewport or Load more owns continuation. Never
        // enumerate the whole disk catalog just because Store was opened.
    }

    function reloadStoreForSession() {
        resetStoreShelves()
        cancelStoreRequests()
        storeBrowseCache = null
        storeForceRefresh = false
        storeLastPageCached = false
        storeSearchQuery = ""
        storeFilters = ({})
        storeFacets = ({genres:[], stores:[], categories:[]})
        storeUsesLocalIndex = false
        storePageCount = 0
        storeGames = []
        storeTotalCount = 0
        storeMarquee = []
        storePanels = []
        storeFilterGroups = []
        storePresentationIndex = 0
        storeError = ""
        storeWarning = ""
        storeHasMore = false
        storeState = "idle"
        // Store is loaded only when its view is opened, not during login on Home.
        storeSessionReset()
    }

    function openGame(game) {
        cancelDetailRequest()
        detailError = ""
        selectedGame = game
        if (detailVisible) refreshSelectedMetadata()
        appController.navigateFromLastPrimary("game-detail")
    }

    function selectGameVariant(index) {
        if (!selectedGame)
            return
        const variants = selectedGame.variants || []
        if (!variants.length)
            return
        const boundedIndex = Math.max(0, Math.min(variants.length - 1, Number(index)))
        const nextGame = Object.assign({}, selectedGame)
        nextGame.selectedVariantIndex = boundedIndex
        selectedGame = nextGame
        const variant = variants[boundedIndex]
        accessibilityAnnounced(qsTr("%1 platform selected").arg(String(variant.store || qsTr("Unknown")))
            + (["MANUAL", "PLATFORM_SYNC"].indexOf(variant.libraryStatus) >= 0 ? qsTr(" · owned")
                : variant.libraryStatus === "NOT_OWNED" ? qsTr(" · not owned") : " · " + qsTr("Ownership unconfirmed")))
    }

    function gameIdentity(game) {
        if (!game)
            return ""
        return String(game.uuid || game.id || game.launchAppId || "")
    }

    function isFavorite(game) {
        const id = gameIdentity(game)
        return id !== "" && (settings.favoriteGameIds || []).indexOf(id) >= 0
    }

    function toggleFavorite(game) {
        const id = gameIdentity(game)
        if (!id)
            return
        const favorites = (settings.favoriteGameIds || []).slice(0)
        const index = favorites.indexOf(id)
        if (index >= 0) {
            removeFromHome(game)
            return
        }
        addToHome(game)
    }

    function isHidden(game) {
        const id = gameIdentity(game)
        return id !== "" && (settings.hiddenGameIds || []).indexOf(id) >= 0
    }

    function hiddenGameCount() {
        return (settings.hiddenGameIds || []).length
    }

    function toggleHidden(game) {
        const id = gameIdentity(game)
        if (!id)
            return
        const hidden = (settings.hiddenGameIds || []).slice(0)
        const index = hidden.indexOf(id)
        if (index >= 0) {
            hidden.splice(index, 1)
            accessibilityAnnounced(qsTr("Restored to library"))
        } else {
            hidden.push(id)
            accessibilityAnnounced(qsTr("Hidden from library"))
        }
        setSetting("hiddenGameIds", hidden)
    }

    function addToHome(game) {
        const id = gameIdentity(game)
        if (!id)
            return
        const favorites = (settings.favoriteGameIds || []).slice(0)
        if (favorites.indexOf(id) >= 0)
            return
        const firstTile = favorites.length === 0
        favorites.push(id)
        if (firstTile) {
            const sizes = Object.assign({}, settings.homeTileSizes || ({}))
            if (!sizes[id])
                sizes[id] = "wide"
            setSetting("homeTileSizes", sizes)
        }
        setSetting("favoriteGameIds", favorites)
        accessibilityAnnounced(qsTr("Added to Home"))
    }

    function removeFromHome(game) {
        const id = gameIdentity(game)
        if (!id)
            return
        const favorites = (settings.favoriteGameIds || []).slice(0)
        const index = favorites.indexOf(id)
        if (index < 0)
            return
        favorites.splice(index, 1)
        const sizes = Object.assign({}, settings.homeTileSizes || ({}))
        delete sizes[id]
        setSetting("homeTileSizes", sizes)
        setSetting("favoriteGameIds", favorites)
        accessibilityAnnounced(qsTr("Removed from Home"))
    }

    function setHomeOrder(ids) {
        const known = settings.favoriteGameIds || []
        const normalized = []
        for (let index = 0; index < ids.length; ++index) {
            const id = String(ids[index] || "")
            if (id && known.indexOf(id) >= 0 && normalized.indexOf(id) < 0)
                normalized.push(id)
        }
        for (let index = 0; index < known.length; ++index) {
            if (normalized.indexOf(known[index]) < 0)
                normalized.push(known[index])
        }
        setSetting("favoriteGameIds", normalized)
        accessibilityAnnounced(qsTr("Home layout saved"))
    }

    function homeTileSize(game) {
        const id = gameIdentity(game)
        return id && settings.homeTileSizes && settings.homeTileSizes[id] === "wide"
            ? "wide" : "square"
    }

    function setHomeTileSize(game, size) {
        const id = gameIdentity(game)
        if (!id)
            return
        const sizes = Object.assign({}, settings.homeTileSizes || ({}))
        sizes[id] = size === "wide" ? "wide" : "square"
        setSetting("homeTileSizes", sizes)
        accessibilityAnnounced(size === "wide" ? qsTr("Wide Home tile") : qsTr("Square Home tile"))
    }

    function acceptCatalog(result) {
        root.catalogRequestId = ""
        if (catalogSource !== "account-library") {
            catalogGames = result.games || []
            catalogTotalCount = Number(result.totalCount || catalogGames.length)
            catalogState = "ready"
            if (!selectedGame && catalogGames.length) selectedGame = catalogGames[0]
            return
        }
        if (!acceptsScope(result.scope) || result.traversalId !== catalogTraversalId
                || (catalogRevision !== null && result.catalogRevision !== catalogRevision)
                || (catalogContext !== "" && result.catalogContext !== catalogContext)) {
            failCatalog(qsTr("The catalog changed. Restart the library refresh."))
            catalogNextCursor = ""
            return
        }
        if (!Array.isArray(result.games) || typeof result.hasNextPage !== "boolean"
                || !Number.isSafeInteger(result.catalogRevision) || result.catalogRevision < 0
                || typeof result.catalogContext !== "string" || result.catalogContext === ""
                || (result.hasNextPage && (!result.nextCursor || catalogSeenCursors[String(result.nextCursor)]))) {
            failCatalog(qsTr("The library returned an invalid or repeated page. Retry the refresh."))
            catalogNextCursor = ""
            return
        }
        const bytes = JSON.stringify(result.games).length * 3
        if (catalogStaged.length + result.games.length > 20000 || catalogStagedBytes + bytes > 32 * 1024 * 1024) {
            failCatalog(qsTr("The library exceeds this device's refresh budget. The saved library has been kept."))
            catalogNextCursor = ""
            return
        }
        catalogRevision = result.catalogRevision
        catalogContext = result.catalogContext || ""
        const next = catalogStaged.slice()
        const ids = new Map(next.map((game, index) => [String(game.id), index]))
        for (const game of result.games) {
            if (!game.id) {
                failCatalog(qsTr("A library game has no identity. Retry the refresh."))
                catalogNextCursor = ""
                return
            }
            if (ids.has(String(game.id))) {
                const index = ids.get(String(game.id))
                const variants = (next[index].variants || []).slice()
                const selected = (game.variants || [])[Number(game.selectedVariantIndex || 0)]
                for (const variant of game.variants || []) {
                    const at = variants.findIndex(previous => String(previous.id) === String(variant.id))
                    if (at >= 0) variants[at] = variant
                    else variants.push(variant)
                }
                const selectedIndex = selected ? variants.findIndex(variant => String(variant.id) === String(selected.id)) : 0
                next[index] = Object.assign({}, game, {variants: variants, selectedVariantIndex: Math.max(0, selectedIndex),
                    availableStores: variants.map(variant => variant.store), isInLibrary: variants.some(variant => variant.inLibrary === true)})
            }
            else { ids.set(String(game.id), next.length); next.push(game) }
        }
        catalogStaged = next
        catalogStagedBytes += bytes
        catalogNextCursor = result.hasNextPage ? String(result.nextCursor) : ""
        const seen = Object.assign(Object.create(null), catalogSeenCursors)
        if (catalogNextCursor) seen[catalogNextCursor] = true
        catalogSeenCursors = seen
        catalogTotalCount = result.totalCount === null ? next.length : Number(result.totalCount || next.length)
        if (!catalogComplete) catalogGames = next
        if (!result.hasNextPage) {
            catalogGames = next
            catalogComplete = true
            catalogLastCompleteAt = Date.now()
            catalogState = "ready"
            catalogError = ""
            if (!selectedGame && next.length) selectedGame = next[0]
            libraryRefreshFinished(true, "")
        } else if (Date.now() - catalogSliceStarted >= 30000) {
            failCatalog(qsTr("Library refresh paused. Continue to load the remaining games."))
        } else libraryPageTimer.restart()
    }

    function acceptStorePresentation(result) {
        root.storePresentationRequestId = ""
        if (result.section === "marquee") root.storeMarquee = result.items || []
        else if (result.section === "panels") root.storePanels = result.items || []
        else if (result.section === "filters") root.storeFilterGroups = result.items || []
        root.storePresentationIndex += 1
        root.requestStorePresentation()
    }

    function failCatalog(message, code) {
        libraryPageTimer.stop()
        if (code === "catalog_changed" || code === "stale_account") catalogNextCursor = ""
        root.catalogState = catalogGames.length ? "partial" : "error"
        root.catalogError = message
        root.catalogRequestId = ""
        libraryRefreshFinished(false, message)
    }

    function failStore(message) {
        root.storeState = "error"
        root.storeRequestId = ""
        root.storePageTimer.stop()
        root.storeError = message
    }

    function failStorePresentation(message) {
        root.storePresentationRequestId = ""
        root.storeWarning = qsTr("Some storefront sections could not load: %1").arg(message)
        root.storePresentationIndex += 1
        root.requestStorePresentation()
    }
}
