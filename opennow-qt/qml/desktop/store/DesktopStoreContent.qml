import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopStoreContent"

    anchors.fill: parent
    focus: true
    clip: true

    property var storeGames: ShellStore.storeGames
    property string searchText: ""
    property string activeCategory: qsTr("Featured")
    property string activeCategoryId: "featured"
    property string activeGenre: qsTr("All")
    property string activeStore: qsTr("All")
    property string focusZone: "hero"
    property int focusIndex: 0
    property string openMenu: ""
    property string presentedMenu: "store"
    onOpenMenuChanged: if (openMenu !== "") presentedMenu = openMenu
    MotionProgress { id: filterMotion; shown: root.openMenu !== ""; enterDuration: 180; exitDuration: 160 }
    property int menuIndex: 0

    signal gameSelected(var game)
    signal playRequested(var game)
    signal routeRequested(string route)
    signal claimRequested(var games)
    signal searchRequested()
    signal messageRequested(string message)

    readonly property int railInnerWidth: Math.max(200, content.width - 48)
    readonly property int railCount: Math.max(1, Math.floor((root.railInnerWidth + DesktopTokens.px(14)) / (DesktopTokens.libraryArtWidth + DesktopTokens.px(14))))
    readonly property var filteredCatalog: ShellStore.storeUsesLocalIndex ? root.storeGames : root.buildFiltered(root.storeGames, root.searchText, root.activeGenre, root.activeStore, root.activeCategoryId)
    readonly property var heroGames: root.buildHero(root.filteredCatalog)
    readonly property var newGames: root.buildNew(root.filteredCatalog, root.heroGames)
    readonly property var popularGames: root.buildPopular(root.filteredCatalog, root.heroGames, root.newGames)
    readonly property var categories: root.buildCategories(root.storeGames)
    readonly property var genreOptions: root.optionsFor("genre", root.storeGames)
    readonly property var storeOptions: root.optionsFor("store", root.storeGames)
    readonly property var categoryOptions: ["featured"].concat(root.allCategoryMetadata().map(item => item.id))
    function allCategoryMetadata() {
        const values = (ShellStore.storeFacets.categories || []).slice()
        const panels = ShellStore.storePanels || []
        for (let p = 0; p < panels.length; ++p)
            for (let s = 0; s < (panels[p].sections || []).length; ++s) {
                const id = "shelf:" + p + ":" + s
                if (!values.some(item => item.id === id)) values.push({id:id,label:panels[p].sections[s].title})
            }
        return values
    }
    readonly property var currentMenuOptions: openMenu === "category" ? categoryOptions : openMenu === "genre" ? genreOptions : storeOptions
    readonly property int catalogCount: Number(ShellStore.storeTotalCount || (root.storeGames || []).length)
    readonly property string catalogState: ShellStore.storeState
    readonly property string catalogError: ShellStore.storeError
    onSearchTextChanged: storeSearchDelay.restart()
    onActiveGenreChanged: storeSearchDelay.restart()
    onActiveStoreChanged: storeSearchDelay.restart()
    onActiveCategoryIdChanged: storeSearchDelay.restart()
    function queryFilters() {
        const filters = {}
        if (activeGenre !== qsTr("All")) filters.genre = activeGenre
        if (activeStore !== qsTr("All")) filters.store = activeStore
        if (activeCategoryId !== "featured") filters.categoryId = activeCategoryId
        return filters
    }
    function categoryLabel(id) {
        const item = root.allCategoryMetadata().find(item => item.id === id)
        return item ? item.label : qsTr("Featured")
    }
    function refreshResults() { ShellStore.refreshStore(root.searchText, true, root.queryFilters()) }
    Connections {
        target: ShellStore
        function onStoreSessionReset() { if (root.visible) ShellStore.ensureStore(root.searchText,root.queryFilters()) }
    }
    Timer {
        id: storeSearchDelay
        interval: 300
        onTriggered: {
            const filters = root.queryFilters()
            if (root.searchText.trim() !== ShellStore.storeSearchQuery
                || JSON.stringify(filters) !== JSON.stringify(ShellStore.storeFilters)) {
                scrollAnimation.stop()
                content.contentY = 0
                ShellStore.ensureStore(root.searchText, filters)
            }
        }
    }

    function catalogList() {
        return root.storeGames || []
    }

    function isOwned(game) {
        if (Boolean(game && (game.storeOwned || game.isInLibrary)))
            return true
        const variants = game && game.variants || []
        for (let index = 0; index < variants.length; ++index) {
            if (Boolean(variants[index].inLibrary))
                return true
        }
        return false
    }

    function looksFree(game) {
        return Boolean(game && (game.storeFree === true || game.freeToPlay === true || game.isFree === true))
    }

    function catalogHasFreeFlag(games) {
        const source = games || []
        for (let index = 0; index < source.length; ++index) {
            if (root.looksFree(source[index]))
                return true
        }
        return false
    }

    function isRtx(game) {
        const tags = game && game.storeCategories || []
        for (let index = 0; index < tags.length; ++index) {
            if (String(tags[index]).toLowerCase() === "rtx")
                return true
        }
        const title = String(game && game.title || "").toLowerCase()
        if (title.indexOf("rtx") >= 0)
            return true
        const genres = game && game.genres || []
        for (let index = 0; index < genres.length; ++index) {
            if (String(genres[index]).toLowerCase().indexOf("rtx") >= 0)
                return true
        }
        const tech = game && game.nvidiaTech || []
        for (let index = 0; index < tech.length; ++index) {
            if (String(tech[index]).toLowerCase().indexOf("rtx") >= 0)
                return true
        }
        return false
    }

    function looksNew(game) {
        const tags = game && game.storeCategories || []
        for (let index = 0; index < tags.length; ++index) {
            if (String(tags[index]).toLowerCase() === "new")
                return true
        }
        return String(game && game.lastPlayed || "").length === 0
    }

    function looksComing(game) {
        const tags = game && game.storeCategories || []
        for (let index = 0; index < tags.length; ++index) {
            if (String(tags[index]).toLowerCase() === "coming")
                return true
        }
        const state = String(game && game.playabilityState || "").toLowerCase()
        return state.indexOf("coming") >= 0 || state.indexOf("soon") >= 0 || state.indexOf("unreleased") >= 0
    }

    function catalogPrice(game) {
        if (game && game.storePrice !== undefined && game.storePrice !== null && String(game.storePrice).length)
            return String(game.storePrice)
        if (game && game.price !== undefined && game.price !== null && String(game.price).length)
            return String(game.price)
        return ""
    }

    function catalogDiscount(game) {
        if (game && game.storeDiscount !== undefined && game.storeDiscount !== null)
            return String(game.storeDiscount)
        if (game && game.discount !== undefined && game.discount !== null)
            return String(game.discount)
        return ""
    }

    function categoriesFor(game) {
        const values = []
        const tags = game && game.storeCategories || []
        for (let index = 0; index < tags.length; ++index)
            values.push(String(tags[index]))
        if (root.isRtx(game) && values.indexOf("rtx") < 0)
            values.push("rtx")
        if (root.looksFree(game) && values.indexOf("free") < 0)
            values.push("free")
        if (root.looksNew(game) && values.indexOf("new") < 0)
            values.push("new")
        if (root.looksComing(game) && values.indexOf("coming") < 0)
            values.push("coming")
        return values
    }

    function decorate(game) {
        if (!game)
            return game
        return Object.assign({}, game, {
            storePrice: root.catalogPrice(game),
            storeDiscount: root.catalogDiscount(game),
            storeFree: root.looksFree(game),
            storeOwned: root.isOwned(game),
            storeCategories: root.categoriesFor(game)
        })
    }

    function decorateList(games) {
        const result = []
        const source = games || []
        for (let index = 0; index < source.length; ++index)
            result.push(root.decorate(source[index]))
        return result
    }

    function containsIdentity(games, candidate) {
        const identity = String(candidate && (candidate.uuid || candidate.id || candidate.title) || "")
        for (let index = 0; index < games.length; ++index) {
            if (String(games[index] && (games[index].uuid || games[index].id || games[index].title) || "") === identity)
                return true
        }
        return false
    }

    function storesFor(game) {
        const values = []
        const stores = game && game.availableStores || []
        for (let index = 0; index < stores.length; ++index)
            values.push(String(stores[index]))
        const variants = game && game.variants || []
        for (let index = 0; index < variants.length; ++index) {
            const store = String(variants[index].store || "")
            if (store.length && values.indexOf(store) < 0)
                values.push(store)
        }
        return values
    }

    function matchesSearchAndFacets(game, query, genre, store) {
        if (query.length && String(game && game.title || "").toLowerCase().indexOf(query) < 0)
            return false
        if (genre !== qsTr("All")) {
            const genres = game && game.genres || []
            let genreFound = false
            for (let index = 0; index < genres.length; ++index)
                genreFound = genreFound || String(genres[index]).toLowerCase() === genre.toLowerCase()
            if (!genreFound)
                return false
        }
        if (store !== qsTr("All")) {
            const stores = root.storesFor(game)
            let storeFound = false
            for (let index = 0; index < stores.length; ++index)
                storeFound = storeFound || stores[index].toLowerCase() === store.toLowerCase()
            if (!storeFound)
                return false
        }
        return true
    }

    function matchesCategory(game, categoryId) {
        if (categoryId === "featured")
            return true
        if (categoryId === "rtx")
            return root.isRtx(game)
        if (categoryId === "free")
            return root.looksFree(game)
        if (categoryId === "new")
            return root.looksNew(game)
        if (categoryId === "coming")
            return root.looksComing(game)
        return true
    }

    function buildFiltered(games, searchText, genre, store, categoryId) {
        const query = String(searchText || "").trim().toLowerCase()
        const source = games || []
        const result = []
        for (let index = 0; index < source.length; ++index) {
            const game = source[index]
            if (!root.matchesSearchAndFacets(game, query, genre, store))
                continue
            if (!root.matchesCategory(game, categoryId))
                continue
            result.push(game)
        }
        return result
    }

    function takeSlice(source, start, count) {
        const result = []
        const list = source || []
        for (let index = start; index < list.length && result.length < count; ++index)
            result.push(list[index])
        return result
    }

    function takeWithout(source, excluded, count) {
        const result = []
        const list = source || []
        for (let index = 0; index < list.length && result.length < count; ++index) {
            if (root.containsIdentity(excluded, list[index]))
                continue
            result.push(list[index])
        }
        return result
    }

    function buildHero(filtered) {
        return root.decorateList(root.takeSlice(filtered, 0, 4))
    }

    // Marquee slides for the auto-sliding hero: live CMS slides when
    // available, otherwise the top browsed games as game slides.
    function marqueeSlides() {
        const live = ShellStore.storeMarquee || []
        if (live.length > 0)
            return live
        const top = root.takeSlice(root.filteredCatalog, 0, 8)
        const slides = []
        for (let index = 0; index < top.length; ++index) {
            const game = top[index]
            const stores = game && game.availableStores || []
            slides.push({
                kind: "game",
                title: String(game && game.title || ""),
                body: String(game && game.publisherName || stores.join(" · ") || ""),
                image: String((game && (game.heroImageUrl || game.imageUrl)) || ""),
                game: game
            })
        }
        return slides
    }

    function isDefaultBrowse() {
        return root.searchText.trim() === ""
            && root.activeGenre === qsTr("All")
            && root.activeStore === qsTr("All")
            && root.activeCategoryId === "featured"
    }

    // Official CMS shelves (GFN Thursday, per-store rows…) as shelf models.
    // Shown only while browsing unfiltered; search and facets use the
    // computed new/popular shelves below.
    function buildPanelShelves() {
        if (!root.isDefaultBrowse())
            return []
        const result = []
        const panels = ShellStore.storePanels || []
        for (let p = 0; p < panels.length; ++p) {
            const sections = panels[p].sections || []
            for (let s = 0; s < sections.length; ++s) {
                const games = root.decorateList(root.takeSlice(sections[s].games || [], 0, 12))
                const total = Number(sections[s].totalCount || games.length)
                if (total === 0)
                    continue
                result.push({
                    zone: "panel" + result.length,
                    title: String(sections[s].title || panels[p].title || ""),
                    games: games,
                    panel: true, totalCount:total,
                    categoryId: "shelf:" + p + ":" + s, seeAll: true,
                    lazy: sections[s].totalCount !== undefined
                })
            }
        }
        return result
    }

    readonly property var panelShelves: root.buildPanelShelves()
    function pagedRows() {
        const rows = []
        for (let at = 0; at < root.filteredCatalog.length; at += root.railCount)
            rows.push({zone:"results" + at, title: at === 0 ? qsTr("More from the catalog") : "", games:root.decorateList(root.filteredCatalog.slice(at,at+root.railCount)), panel:false, seeAll:false})
        return rows
    }
    readonly property var shelfModels: root.panelShelves.concat(ShellStore.storeUsesLocalIndex ? root.pagedRows() : [
        {zone: "new", title: qsTr("Recently added"), games: root.newGames, panel: false, seeAll: true},
        {zone: "popular", title: qsTr("More from the catalog"), games: root.popularGames, panel: false, seeAll: true}
    ])

    function shelfZoneOrder() {
        const zones = hero.visible ? ["hero", "chips"] : ["chips"]
        const shelves = root.shelfModels || []
        for (let index = 0; index < shelves.length; ++index)
            zones.push(shelves[index].zone)
        return zones
    }

    function shelfModelFor(zone) {
        const shelves = root.shelfModels || []
        for (let index = 0; index < shelves.length; ++index) {
            if (shelves[index].zone === zone)
                return shelves[index]
        }
        return null
    }
    function shelfGamesFor(zone) {
        const index = root.shelfModels.findIndex(shelf => shelf.zone === zone)
        const item = index >= 0 ? shelfRepeater.itemAt(index) : null
        return item ? item.displayGames : []
    }

    function buildNew(filtered, heroGames) {
        const unplayed = []
        const list = filtered || []
        for (let index = 0; index < list.length; ++index) {
            if (String(list[index] && list[index].lastPlayed || "").length === 0)
                unplayed.push(list[index])
        }
        const lastAdded = root.takeSlice(list, Math.max(0, list.length - root.railCount), root.railCount)
        const pool = unplayed.length ? unplayed : lastAdded
        let result = root.takeWithout(pool, heroGames, root.railCount)
        if (!result.length)
            result = root.takeSlice(list, 4, root.railCount)
        if (!result.length)
            result = root.takeSlice(list, 0, root.railCount)
        return root.decorateList(result)
    }

    function buildPopular(filtered, heroGames, newGames) {
        const excluded = (heroGames || []).concat(newGames || [])
        const played = []
        const list = filtered || []
        for (let index = 0; index < list.length; ++index) {
            if (String(list[index] && list[index].lastPlayed || "").length > 0)
                played.push(list[index])
        }
        let result = root.takeWithout(played.length ? played : list, excluded, root.railCount)
        if (!result.length)
            result = root.takeSlice(list, 4 + root.railCount, root.railCount)
        if (!result.length)
            result = root.takeWithout(list, excluded, root.railCount)
        if (!result.length)
            result = root.takeSlice(list, 0, root.railCount)
        return root.decorateList(result)
    }

    function buildCategories(games) {
        const list = [
            {id: "featured", label: qsTr("Featured")},
            {id: "new", label: qsTr("New releases")}
        ]
        if (ShellStore.storeUsesLocalIndex || root.catalogHasFreeFlag(games))
            list.push({id: "free", label: qsTr("Free to play")})
        list.push({id: "rtx", label: qsTr("RTX")})
        list.push({id: "coming", label: qsTr("Coming soon")})
        return list
    }

    function optionsFor(kind, games) {
        const values = [qsTr("All")]
        const globalValues = ShellStore.storeFacets[kind === "genre" ? "genres" : "stores"] || []
        for (const value of globalValues) if (values.indexOf(value) < 0) values.push(value)
        const source = games || []
        for (let gameIndex = 0; gameIndex < source.length; ++gameIndex) {
            const game = source[gameIndex]
            const candidates = kind === "genre" ? (game.genres || []) : root.storesFor(game)
            for (let index = 0; index < candidates.length; ++index) {
                const value = String(candidates[index]).trim()
                if (value.length && values.indexOf(value) < 0)
                    values.push(value)
            }
        }
        return values
    }

    function pointZone(zone, index) {
        // Scrolling moves hit targets underneath a stationary pointer. Hover
        // may highlight a row, but must never start (or redirect) scrolling.
        if (scrollAnimation.running || content.moving) return
        focusZone = zone
        focusIndex = Math.max(0, index)
    }

    function selectZone(zone, index) {
        focusZone = zone
        focusIndex = Math.max(0, index)
        forceActiveFocus()
        if (zone === "hero" || zone === "chips") {
            scrollTo(0)
            return
        }
        const shelfIndex = root.shelfModels.findIndex(shelf => shelf.zone === zone)
        const item = shelfIndex >= 0 ? shelfRepeater.itemAt(shelfIndex) : null
        if (item) {
            const top = shelvesColumn.y + item.y - 20
            const bottom = shelvesColumn.y + item.y + item.height + 20
            // Move only far enough to reveal the selected row. Resolve from
            // the current viewport when an opposite key interrupts a scroll.
            const target = top < content.contentY || bottom - top > content.height ? top
                : bottom > content.contentY + content.height ? bottom - content.height : content.contentY
            scrollTo(target)
        }
    }

    function scrollTo(value) {
        scrollAnimation.stop()
        content.cancelFlick()
        const target = Math.max(0, Math.min(value, content.contentHeight - content.height))
        if (Math.abs(content.contentY - target) < 0.5) return
        scrollAnimation.from = content.contentY
        scrollAnimation.to = target
        scrollAnimation.duration = AppController.reducedMotion ? 0 : 180
        scrollAnimation.start()
    }

    function activateCurrent() {
        if (focusZone === "hero") {
            const game = hero.slideGame
            if (!game)
                return
            if (focusIndex === 0)
                root.playRequested(game)
            else
                root.gameSelected(game)
            return
        }
        if (focusZone === "chips") {
            if (focusIndex < categories.length) {
                activeCategoryId = categories[focusIndex].id
                activeCategory = categories[focusIndex].label
            }
            else if (focusIndex === categories.length + 2) {
                openMenu = openMenu === "category" ? "" : "category"
                menuIndex = 0
            } else if (focusIndex === categories.length + 3) {
                root.refreshResults()
            } else {
                openMenu = focusIndex === categories.length ? (openMenu === "genre" ? "" : "genre")
                                                            : (openMenu === "store" ? "" : "store")
                menuIndex = 0
            }
            return
        }
        const games = root.shelfGamesFor(focusZone)
        if (games.length) gameSelected(games[Math.min(focusIndex, games.length - 1)])
    }

    function syncFreeCategory() {
        if (!ShellStore.storeUsesLocalIndex && root.activeCategoryId === "free" && !root.catalogHasFreeFlag(root.storeGames)) {
            root.activeCategoryId = "featured"
            root.activeCategory = qsTr("Featured")
        }
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_F5) {
            root.refreshResults()
        } else if (root.openMenu.length && (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space)) {
            const option = root.currentMenuOptions[Math.min(root.menuIndex, Math.max(0, root.currentMenuOptions.length - 1))]
            if (root.openMenu === "genre") root.activeGenre = option
            else if (root.openMenu === "category") root.activeCategoryId = option
            else root.activeStore = option
            root.openMenu = ""
        } else if (root.openMenu.length && event.key === Qt.Key_Up) {
            root.menuIndex = Math.max(0, root.menuIndex - 1)
        } else if (root.openMenu.length && event.key === Qt.Key_Down) {
            root.menuIndex = Math.min(root.currentMenuOptions.length - 1, root.menuIndex + 1)
        } else if (event.key === Qt.Key_Slash) {
            root.searchRequested()
        } else if (event.key === Qt.Key_Escape && root.openMenu.length) {
            root.openMenu = ""
        } else if (event.key === Qt.Key_Left) {
            root.focusIndex = Math.max(0, root.focusIndex - 1)
        } else if (event.key === Qt.Key_Right) {
            let count = 2
            if (focusZone === "chips")
                count = root.categories.length + 4
            else {
                const shelf = root.shelfModelFor(focusZone)
                if (shelf)
                    count = Math.max(1, root.shelfGamesFor(focusZone).length)
            }
            root.focusIndex = Math.min(Math.max(0, count - 1), root.focusIndex + 1)
        } else if (event.key === Qt.Key_Up || event.key === Qt.Key_Down) {
            const order = root.shelfZoneOrder()
            const at = order.indexOf(focusZone)
            if (event.key === Qt.Key_Down && at === order.length - 1 && ShellStore.storeHasMore && !ShellStore.storeLoading)
                ShellStore.requestStorePage()
            if (at < 0) {
                root.selectZone(order[0], 0)
            } else {
            const next = event.key === Qt.Key_Up ? Math.max(0, at - 1) : Math.min(order.length - 1, at + 1)
            let count = 2
            const target = order[next]
            if (target === "chips")
                count = root.categories.length + 4
            else {
                const shelf = root.shelfModelFor(target)
                if (shelf)
                    count = Math.max(1, root.shelfGamesFor(target).length)
            }
            root.selectZone(target, Math.min(root.focusIndex, Math.max(0, count - 1)))
            }
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
            root.activateCurrent()
        } else {
            return
        }
        event.accepted = true
    }

    Flickable {
        id: content
        objectName: "storeViewport"
        anchors.fill: parent
        contentWidth: width
        contentHeight: Math.max(height, shelvesColumn.y + shelvesColumn.height + 24)
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.VerticalFlick
        // Loading is tied to movement near the end, never to page arrival:
        // an empty/filtered result cannot accidentally crawl the whole cache.
        onContentYChanged: {
            if (moving && contentY + height >= contentHeight - 240 && !ShellStore.storeLoading
                    && ShellStore.storeHasMore && !ShellStore.storeError) ShellStore.requestStorePage()
        }
        onMovementStarted: scrollAnimation.stop()

        DesktopStoreHero {
            id: hero
            x: 24
            y: 18
            width: root.railInnerWidth
            height: 260
            visible: slides.length > 0
            slides: root.marqueeSlides()
            selectedAction: root.focusZone === "hero" ? root.focusIndex : -1
            onActionPointed: index => root.pointZone("hero", index)
            onPlayRequested: game => root.playRequested(game)
            onDetailsRequested: game => root.gameSelected(game)
        }

        Flow {
            id: chips
            x: 24
            y: hero.visible ? 292 : 18
            width: root.railInnerWidth
            spacing: 8

            Repeater {
                model: root.categories
                DesktopStoreChip {
                    required property int index
                    required property var modelData
                    text: modelData.label
                    selected: root.activeCategoryId === modelData.id
                              || (root.focusZone === "chips" && root.focusIndex === index)
                    onHoveredChanged: if (hovered) root.pointZone("chips", index)
                    onClicked: {
                        root.activeCategoryId = modelData.id
                        root.activeCategory = modelData.label
                        root.openMenu = ""
                    }
                }
            }

            DesktopStoreChip {
                id: genreChip
                text: root.activeGenre === qsTr("All") ? qsTr("Genres") : DesktopTokens.genreLabel(root.activeGenre)
                hasMenu: true
                selected: root.openMenu === "genre"
                          || (root.focusZone === "chips" && root.focusIndex === root.categories.length)
                onHoveredChanged: if (hovered) root.pointZone("chips", root.categories.length)
                onClicked: {
                    root.openMenu = root.openMenu === "genre" ? "" : "genre"
                    root.menuIndex = 0
                }
            }

            DesktopStoreChip {
                id: storeChip
                text: root.activeStore === qsTr("All") ? qsTr("Stores") : DesktopTokens.storeLabel(root.activeStore)
                hasMenu: true
                selected: root.openMenu === "store"
                          || (root.focusZone === "chips" && root.focusIndex === root.categories.length + 1)
                onHoveredChanged: if (hovered) root.pointZone("chips", root.categories.length + 1)
                onClicked: {
                    root.openMenu = root.openMenu === "store" ? "" : "store"
                    root.menuIndex = 0
                }
            }
            DesktopStoreChip {
                id: categoryChip
                text: root.activeCategoryId.indexOf("shelf:") === 0 ? root.categoryLabel(root.activeCategoryId) : qsTr("Categories")
                hasMenu: true
                selected: root.openMenu === "category" || (root.focusZone === "chips" && root.focusIndex === root.categories.length + 2)
                onHoveredChanged: if (hovered) root.pointZone("chips", root.categories.length + 2)
                onClicked: { root.openMenu = root.openMenu === "category" ? "" : "category"; root.menuIndex = 0 }
            }
            DesktopStoreChip {
                objectName: "storeRefreshButton"
                text: qsTr("Refresh")
                selected: root.focusZone === "chips" && root.focusIndex === root.categories.length + 3
                onHoveredChanged: if (hovered) root.pointZone("chips", root.categories.length + 3)
                onClicked: root.refreshResults()
            }
        }

        Rectangle {
            id: storeStatus
            objectName: "storePageStatus"
            x: 24; y: chips.y + chips.height + 16; width: root.railInnerWidth
            visible: root.storeGames.length > 0 && (ShellStore.storeLoading || ShellStore.storeHasMore
                || root.catalogError !== "" || ShellStore.storeWarning !== "")
            height: visible ? Math.max(48, statusText.implicitHeight + 24) : 0
            radius: 12; color: DesktopTokens.raised
            Text {
                id: statusText
                objectName: "storePageStatusText"
                x: 12; anchors.verticalCenter: parent.verticalCenter
                width: parent.width - 24 - (pageAction.visible ? pageAction.width + 12 : 0)
                text: root.catalogError !== "" ? qsTr("Loaded %1 games. %2").arg(root.storeGames.length).arg(root.catalogError)
                    : ShellStore.storeWarning !== "" ? ShellStore.storeWarning
                    : qsTr("Loaded %1 of %2 games").arg(root.storeGames.length).arg(root.catalogCount)
                wrapMode: Text.Wrap; textFormat: Text.PlainText
                color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
            }
            DesktopSettingsButton {
                id: pageAction
                objectName: "storePageRetry"
                anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter
                visible: !ShellStore.storeLoading
                text: root.catalogError !== "" || ShellStore.storeWarning !== "" ? qsTr("Try again") : qsTr("Load more")
                onClicked: root.catalogError !== "" || ShellStore.storeWarning !== "" ? ShellStore.retryStore() : ShellStore.requestStorePage()
            }
        }

        Column {
            id: shelvesColumn
            x: 24
            y: storeStatus.y + (storeStatus.visible ? storeStatus.height + 16 : 0)
            width: root.railInnerWidth
            spacing: 16
            Repeater {
                id: shelfRepeater
                model: root.shelfModels
                delegate: DesktopStoreShelf {
                    required property var modelData
                    objectName: "storeShelf-" + modelData.zone
                    width: shelvesColumn.width
                    title: modelData.title
                    seeAllText: modelData.seeAll
                        ? (modelData.zone === "new" && root.filteredCatalog.length
                            ? qsTr("See all %1").arg(root.filteredCatalog.length) : qsTr("See all"))
                        : ""
                    showSeeAll: Boolean(modelData.seeAll)
                    games: modelData.games
                    categoryId: modelData.lazy ? modelData.categoryId : ""
                    totalCount: Number(modelData.totalCount || modelData.games.length)
                    materialized: root.focusZone === modelData.zone || (shelvesColumn.y + y + height >= content.contentY - 180 && shelvesColumn.y + y <= content.contentY + content.height + 180)
                    active: root.focusZone === modelData.zone
                    selectedIndex: root.focusIndex
                    onGamePointed: index => root.pointZone(modelData.zone, index)
                    onGameActivated: game => root.gameSelected(game)
                    onSeeAllRequested: {
                        if (modelData.categoryId) root.activeCategoryId = modelData.categoryId
                        else { root.activeCategoryId = "all"; storeSearchDelay.restart() }
                    }
                }
            }
            DesktopSettingsButton {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: ShellStore.storeHasMore && !ShellStore.storeLoading
                text: qsTr("Load more")
                onClicked: ShellStore.requestStorePage()
            }
        }
    }

    Rectangle {
        id: filterMenu
        objectName: "storeFilterMenu"
        z: 400
        visible: filterMotion.present
        enabled: root.openMenu.length > 0
        readonly property var anchorChip: root.presentedMenu === "category" ? categoryChip : root.presentedMenu === "genre" ? genreChip : storeChip
        x: Math.max(12, Math.min(chips.x + anchorChip.x, root.width - width - 12))
        y: Math.max(12, Math.min(chips.y + anchorChip.y + anchorChip.height + 6 - content.contentY, root.height - height - 12))
        width: Math.min(DesktopTokens.px(284), root.width - 24)
        height: Math.min(menuColumn.contentHeight + 12, DesktopTokens.px(332), root.height - 24)
        radius: DesktopTokens.px(11)
        color: Theme.lightMode ? "#F7F9FC" : "#0B0F1A"
        border.width: 1
        border.color: DesktopTokens.seam
        opacity: filterMotion.progress
        scale: filterMotion.zoom
        transformOrigin: Item.TopLeft

        ListView {
            id: menuColumn
            anchors.fill: parent
            anchors.margins: 6
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            currentIndex: root.menuIndex
            onCurrentIndexChanged: positionViewAtIndex(currentIndex, ListView.Contain)
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            model: root.presentedMenu === "category" ? root.categoryOptions : root.presentedMenu === "genre" ? root.genreOptions : root.storeOptions
            delegate: Button {
                    id: menuButton
                    required property string modelData
                    required property int index
                    width: menuColumn.width
                    height: Math.max(DesktopTokens.px(36), menuLabel.implicitHeight + DesktopTokens.px(16))
                    padding: 0
                    focusPolicy: Qt.NoFocus
                    hoverEnabled: true
                    background: Rectangle {
                        radius: 7
                        color: menuButton.hovered || root.menuIndex === menuButton.index
                               ? DesktopTokens.raisedStrong : "transparent"
                    }
                    contentItem: Item {
                      Rectangle {
                        visible: storeLogo.visible && Theme.lightMode
                        x: storeLogo.x - DesktopTokens.px(3)
                        anchors.verticalCenter: parent.verticalCenter
                        width: storeLogo.width + DesktopTokens.px(6); height: width
                        radius: DesktopTokens.px(5); color: "#202634"
                      }
                      Image {
                        id: storeLogo
                        x: DesktopTokens.px(9); anchors.verticalCenter: parent.verticalCenter
                        width: DesktopTokens.px(20); height: width
                        source: root.presentedMenu === "store" ? DesktopTokens.storeIconUrl(menuButton.modelData) : ""
                        visible: source.toString() !== ""
                        sourceSize: Qt.size(40,40); fillMode: Image.PreserveAspectFit
                      }
                      Text {
                        id: menuLabel
                        x: DesktopTokens.px(storeLogo.visible ? 39 : 9)
                        width: parent.width - x - DesktopTokens.px(14)
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.presentedMenu === "category" ? root.categoryLabel(menuButton.modelData) : root.presentedMenu === "store" ? DesktopTokens.storeLabel(menuButton.modelData) : DesktopTokens.genreLabel(menuButton.modelData)
                        color: DesktopTokens.text
                        font.family: Theme.bodyFont
                        font.pixelSize: DesktopTokens.monoSize
                        font.weight: Font.DemiBold
                        verticalAlignment: Text.AlignVCenter
                        wrapMode: Text.Wrap
                      }
                    }
                    onClicked: {
                        if (root.openMenu === "genre") root.activeGenre = modelData
                        else if (root.openMenu === "category") root.activeCategoryId = modelData
                        else root.activeStore = modelData
                        root.openMenu = ""
                        root.forceActiveFocus()
                    }
                    onHoveredChanged: if (hovered) root.menuIndex = index
                }
        }
    }

    Column {
        anchors.centerIn: parent
        width: Math.min(root.width - 48, 600)
        spacing: 9
        visible: root.panelShelves.length === 0 && root.newGames.length === 0 && root.popularGames.length === 0
        z: 200

        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: root.catalogList().length === 0
                  ? (root.catalogState === "error" ? qsTr("Store unavailable") : root.catalogState === "ready" ? qsTr("No games match these filters") : qsTr("Loading the store…"))
                  : qsTr("No games match these filters")
            color: DesktopTokens.text
            font.family: Theme.displayFont
            font.pixelSize: DesktopTokens.px(22)
            font.weight: Font.Black
        }
        Text {
            anchors.horizontalCenter: parent.horizontalCenter
            objectName: "storeEmptyStatusText"
            width: parent.width
            horizontalAlignment: Text.AlignHCenter
            wrapMode: Text.Wrap
            textFormat: Text.PlainText
            text: root.catalogList().length === 0
                  ? (root.catalogState === "error" ? root.catalogError : root.catalogState === "ready" ? qsTr("No games match these filters") : qsTr("Browsing the GeForce NOW catalog."))
                  : qsTr("Try another category, genre, store, or search.")
            color: DesktopTokens.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.captionSize
        }
        DesktopSettingsButton {
            anchors.horizontalCenter: parent.horizontalCenter
            visible: root.catalogList().length === 0 && root.catalogState === "error"
            text: qsTr("Try again")
            primary: true
            onClicked: ShellStore.retryStore()
        }
    }

    NumberAnimation {
        id: scrollAnimation
        objectName: "storeScrollAnimation"
        target: content
        property: "contentY"
        easing.type: Easing.OutCubic
    }

    onStoreGamesChanged: root.syncFreeCategory()
    Component.onCompleted: {
        ShellStore.ensureStore(root.searchText, root.queryFilters())
        root.syncFreeCategory()
        root.forceActiveFocus()
    }
}
