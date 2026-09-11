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
    property string catalogSource: "public"
    property string catalogRequestId: ""
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

    function refreshCatalog(searchQuery) {
        if (!ready || catalogRequestId !== "")
            return
        catalogState = catalogGames.length > 0 ? "refreshing" : "loading"
        catalogSource = signedIn ? "account-library" : "public"
        catalogRequestId = coreClient.request(signedIn ? "catalog.library.list" : "catalog.public.list", {
            limit: signedIn ? 1000 : 360,
            searchQuery: searchQuery || ""
        }, 30000)
    }

    function reloadCatalogForSession() {
        if (catalogRequestId !== "") {
            coreClient.cancel(catalogRequestId)
            catalogRequestId = ""
        }
        catalogGames = []
        catalogState = "idle"
        refreshCatalog("")
        reloadStoreForSession()
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
        storeError = ""
        storeWarning = ""
        storeHasMore = false
        storeState = "idle"
        // Store is loaded only when its view is opened, not during login on Home.
        storeSessionReset()
    }

    function openGame(game) {
        selectedGame = game
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
            + (Boolean(variant.inLibrary) ? qsTr(" · owned") : qsTr(" · not owned")))
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
        root.catalogGames = result.games || []
        root.catalogTotalCount = Number(result.totalCount || root.catalogGames.length)
        if (!root.selectedGame && root.catalogGames.length > 0)
            root.selectedGame = root.catalogGames[0]
        root.catalogState = "ready"
        root.catalogRequestId = ""
    }

    function acceptStorePresentation(result) {
        root.storePresentationRequestId = ""
        if (result.section === "marquee") root.storeMarquee = result.items || []
        else if (result.section === "panels") root.storePanels = result.items || []
        else if (result.section === "filters") root.storeFilterGroups = result.items || []
        root.storePresentationIndex += 1
        root.requestStorePresentation()
    }

    function failCatalog(message) {
        root.catalogState = "error"
        root.catalogRequestId = ""
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
