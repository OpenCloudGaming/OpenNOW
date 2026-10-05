import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property string searchQuery: ""
    property int platformIndex: 0
    property int genreIndex: 0
    property int sortIndex: 0
    property bool cloudFavoritesOnly: false
    property bool filterSheetOpen: false
    readonly property int columnCount: 7
    readonly property var platformOptions: {
        const values = ["All"]
        for (let gameIndex = 0; gameIndex < ShellStore.catalogGames.length; ++gameIndex) {
            const game = ShellStore.catalogGames[gameIndex]
            const variants = game.variants || []
            const stores = variants.length
                ? variants.map(variant => String(variant.store || ""))
                : (game.availableStores || []).map(store => String(store || ""))
            for (let storeIndex = 0; storeIndex < stores.length; ++storeIndex) {
                const store = stores[storeIndex].trim()
                if (store.length && values.indexOf(store) < 0)
                    values.push(store)
            }
        }
        return values
    }
    readonly property var sortLabels: [qsTr("Popular"), qsTr("Title"), qsTr("Recently played"), qsTr("Store"), qsTr("Home pins first")]
    readonly property var genreOptions: {
        const values = ["All"]
        for (let gameIndex = 0; gameIndex < ShellStore.catalogGames.length; ++gameIndex) {
            const genres = ShellStore.catalogGames[gameIndex].genres || []
            for (let genreIndex = 0; genreIndex < genres.length; ++genreIndex) {
                const genre = String(genres[genreIndex])
                if (genre.length && values.indexOf(genre) < 0 && values.length < 12)
                    values.push(genre)
            }
        }
        return values
    }
    readonly property var games: {
        const query = root.searchQuery.trim().toLowerCase()
        const platform = root.platformOptions[root.platformIndex].toLowerCase()
        const genre = root.genreOptions[Math.min(root.genreIndex, root.genreOptions.length - 1)]
        const filtered = []
        const source = cloudFavoritesOnly ? ShellStore.remoteFavorites : ShellStore.catalogGames
        for (let index = 0; index < source.length; ++index) {
            const game = source[index]
            const searchText = String(game.searchText || game.title || "").toLowerCase()
            const stores = (game.availableStores || []).map(store => String(store).toLowerCase())
            const genres = game.genres || []
            if (query.length && searchText.indexOf(query) < 0)
                continue
            if (platform !== "all" && stores.indexOf(platform) < 0)
                continue
            if (genre !== "All" && genres.indexOf(genre) < 0)
                continue
            filtered.push(game)
        }
        if (root.sortIndex === 0)
            return filtered
        filtered.sort((left, right) => {
            if (root.sortIndex === 2)
                return String(right.lastPlayed || "").localeCompare(String(left.lastPlayed || "")) || String(left.title).localeCompare(String(right.title))
            if (root.sortIndex === 3)
                return root.storeName(left).localeCompare(root.storeName(right)) || String(left.title).localeCompare(String(right.title))
            if (root.sortIndex === 4) {
                const favoriteOrder = Number(ShellStore.isFavorite(right)) - Number(ShellStore.isFavorite(left))
                if (favoriteOrder !== 0)
                    return favoriteOrder
            }
            return String(left.title).localeCompare(String(right.title))
        })
        return filtered
    }
    readonly property var selectedGame: games.length > 0 ? games[Math.max(0, Math.min(games.length - 1, catalog.currentIndex))] : null
    readonly property bool filtersActive: searchQuery.trim() !== "" || platformIndex > 0 || genreIndex > 0 || cloudFavoritesOnly
    readonly property string storeFilterLabel: platformIndex > 0 ? ConsoleStores.label(platformOptions[platformIndex]) : qsTr("All stores")
    readonly property string genreFilterLabel: genreIndex > 0 && genreIndex < genreOptions.length
        ? DesktopTokens.genreLabel(genreOptions[genreIndex]) : qsTr("All genres")
    readonly property string showFilterLabel: cloudFavoritesOnly ? qsTr("GeForce NOW favorites") : qsTr("All library games")
    readonly property var filterSections: [
        {
            title: qsTr("Sort"), value: root.sortLabels[root.sortIndex], currentIndex: root.sortIndex,
            options: root.sortLabels.map(label => ({label: label}))
        },
        {
            title: qsTr("Store"), value: root.storeFilterLabel, currentIndex: root.platformIndex,
            options: root.platformOptions.map((store, index) => index === 0
                ? {label: qsTr("All stores"), icon: "all"}
                : {label: ConsoleStores.label(store), store: store})
        },
        {
            title: qsTr("Genre"), value: root.genreFilterLabel, currentIndex: Math.min(root.genreIndex, root.genreOptions.length - 1),
            options: root.genreOptions.map((genre, index) => ({label: index === 0 ? qsTr("All genres") : DesktopTokens.genreLabel(genre)}))
        },
        {
            title: qsTr("Show"), value: root.showFilterLabel, currentIndex: root.cloudFavoritesOnly ? 1 : 0,
            description: root.cloudFavoritesOnly
                ? (ShellStore.remoteFavoritesError || qsTr("Favorites coverage is partial or unknown. Home pins are separate."))
                : qsTr("GeForce NOW favorites sync with your account. Home pins stay on this device."),
            options: [{label: qsTr("All library games")}, {label: qsTr("GeForce NOW favorites")}]
        }
    ]
    readonly property string countLabel: {
        const total = Number(root.cloudFavoritesOnly ? ShellStore.remoteFavorites.length : ShellStore.catalogTotalCount)
            .toLocaleString(Qt.locale(), "f", 0)
        const base = root.cloudFavoritesOnly ? qsTr("%1 GeForce NOW favorites").arg(total)
            : ShellStore.catalogSource === "account-library" ? qsTr("%1 games in your library").arg(total)
            : qsTr("%1 supported games").arg(total)
        return (root.filtersActive ? qsTr("%1 shown · %2").arg(root.games.length.toLocaleString(Qt.locale(), "f", 0)).arg(base) : base).toUpperCase()
    }

    onPlatformOptionsChanged: platformIndex = Math.max(0, Math.min(platformOptions.length - 1, platformIndex))

    function storeName(game) {
        return game && game.availableStores && game.availableStores.length ? game.availableStores[0] : "GFN"
    }

    function showSearchKeyboard() {
        filterSheetOpen = false
        virtualKeyboard.openKeyboard(root.searchQuery)
    }

    function openSelected() {
        if (root.selectedGame !== null)
            ShellStore.openGame(root.selectedGame)
    }

    function applyFilter(section, option) {
        if (section === 0)
            root.sortIndex = option
        else if (section === 1)
            root.platformIndex = option
        else if (section === 2)
            root.genreIndex = option
        else if (section === 3) {
            root.cloudFavoritesOnly = option === 1
            if (root.cloudFavoritesOnly)
                ShellStore.refreshCloudFavorites()
        }
        catalog.currentIndex = 0
    }

    function resetFilters() {
        root.sortIndex = 0
        root.platformIndex = 0
        root.genreIndex = 0
        root.cloudFavoritesOnly = false
        catalog.currentIndex = 0
    }

    function closeFilterSheet() {
        root.filterSheetOpen = false
        filterButton.forceActiveFocus()
    }

    Keys.onPressed: event => {
        if (virtualKeyboard.presented || root.filterSheetOpen)
            return
        if (event.key === Qt.Key_Back) {
            root.showSearchKeyboard()
        } else if (event.key === Qt.Key_Y) {
            if (!event.isAutoRepeat && root.selectedGame !== null)
                ShellStore.toggleFavorite(root.selectedGame)
        } else if (event.key === Qt.Key_X) {
            if (!event.isAutoRepeat)
                root.openSelected()
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            if (event.isAutoRepeat)
                return
            if (root.selectedGame !== null)
                root.openSelected()
            else if (!root.cloudFavoritesOnly && ShellStore.catalogState === "error")
                ShellStore.refreshCatalog("")
        } else return
        event.accepted = true
    }

    ScreenBackground {
        artwork: root.selectedGame ? (root.selectedGame.heroImageUrl || root.selectedGame.imageUrl || "") : ""
        tint: "#354016"
    }

    TextField {
        id: searchField
        x: 96; y: 126
        width: 440; height: 60
        placeholderText: qsTr("Search GeForce NOW games")
        text: root.searchQuery
        color: Theme.label
        placeholderTextColor: Theme.textMuted
        font.family: Theme.bodyFont; font.pixelSize: 19
        font.weight: Font.DemiBold
        leftPadding: 112
        rightPadding: 24
        Accessible.name: qsTr("Search games")
        KeyNavigation.right: filterButton
        KeyNavigation.down: catalog
        onTextEdited: root.searchQuery = text
        onAccepted: catalog.forceActiveFocus()
        Keys.onEscapePressed: catalog.forceActiveFocus()
        background: Rectangle {
            radius: 30
            color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.62)
            FocusFrame { focused: searchField.activeFocus; frameRadius: 30 }
        }
        ControllerGlyph {
            x: 16; anchors.verticalCenter: parent.verticalCenter
            glyph: "VIEW"; label: ""; glyphSize: 32
            TapHandler { onTapped: root.showSearchKeyboard() }
        }
        Image {
            x: 72; anchors.verticalCenter: parent.verticalCenter
            width: 22; height: 22
            source: "qrc:/qt/qml/OpenNOW/res/icons/desktop-search" + (Theme.lightMode ? "-on-light" : "") + ".svg"
            sourceSize: Qt.size(44, 44)
        }
    }

    Button {
        id: filterButton
        objectName: "consoleLibraryFilterButton"
        x: searchField.x + searchField.width + 16; y: 126
        width: Math.min(620, filterRow.implicitWidth + 48); height: 60
        padding: 0
        focusPolicy: Qt.StrongFocus
        Accessible.name: qsTr("Filter and sort: %1").arg(filterSummary.text)
        KeyNavigation.left: searchField
        KeyNavigation.right: catalogRetry.visible ? catalogRetry : null
        KeyNavigation.down: catalog
        onClicked: root.filterSheetOpen = true
        Keys.onReturnPressed: event => { if (!event.isAutoRepeat) clicked() }
        Keys.onEnterPressed: event => { if (!event.isAutoRepeat) clicked() }
        background: Rectangle {
            radius: 30
            color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.62)
            FocusFrame {
                focused: filterButton.activeFocus || root.filterSheetOpen
                parked: root.filterSheetOpen
                frameRadius: 30
            }
        }
        contentItem: Item {
            Row {
                id: filterRow
                x: 24
                anchors.verticalCenter: parent.verticalCenter
                spacing: 14
                Image {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 22; height: 22
                    source: "qrc:/qt/qml/OpenNOW/res/icons/desktop-sliders" + (Theme.lightMode ? "-on-light" : "") + ".svg"
                    sourceSize: Qt.size(44, 44)
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Filter & sort")
                    color: Theme.label
                    font.family: Theme.displayFont
                    font.pixelSize: 20
                    font.weight: Font.Black
                }
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 1; height: 24; color: Theme.seam }
                Text {
                    id: filterSummary
                    anchors.verticalCenter: parent.verticalCenter
                    width: Math.min(implicitWidth, 360)
                    elide: Text.ElideRight
                    text: (root.cloudFavoritesOnly ? [root.showFilterLabel] : []).concat(
                        [root.storeFilterLabel, root.genreFilterLabel, root.sortLabels[root.sortIndex]]).join(" · ")
                    color: Theme.textMuted
                    font.family: Theme.bodyFont
                    font.pixelSize: 18
                    font.weight: Font.DemiBold
                }
            }
        }
    }

    Row {
        anchors.right: parent.right
        anchors.rightMargin: 96
        y: 126
        height: 60
        spacing: 18
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.countLabel
            color: Theme.textMuted
            font.family: Theme.monoFont
            font.pixelSize: 15
            font.weight: Font.Bold
            font.letterSpacing: 1.4
        }
        ConsoleActionButton {
            id: catalogRetry
            anchors.verticalCenter: parent.verticalCenter
            visible: !root.cloudFavoritesOnly && Boolean(ShellStore.catalogError)
            height: 52
            text: ShellStore.catalogNextCursor ? qsTr("Continue") : qsTr("Retry")
            KeyNavigation.left: filterButton
            KeyNavigation.down: catalog
            onClicked: ShellStore.continueCatalog()
        }
    }

    Item {
        x: 80; y: 214
        width: root.columnCount * 172 + 32
        height: root.height - y - 152
        clip: true
        GridView {
            id: catalog
            x: 16; y: 16
            width: root.columnCount * 172
            height: parent.height - 16
            cellWidth: 172
            cellHeight: 268
            clip: false
            model: root.games
            focus: true
            keyNavigationWraps: false
            highlightFollowsCurrentItem: false
            Component.onCompleted: currentIndex = ShellStore.focusIndex("library")
            onCurrentIndexChanged: {
                ShellStore.rememberFocus("library", currentIndex)
                positionViewAtIndex(currentIndex, GridView.Contain)
            }
            Keys.onUpPressed: event => {
                if (catalog.currentIndex < root.columnCount)
                    filterButton.forceActiveFocus()
                else
                    catalog.moveCurrentIndexUp()
            }
            delegate: Item {
                id: gameDelegate
                required property var modelData
                required property int index
                width: catalog.cellWidth; height: catalog.cellHeight
                PosterTile {
                    width: 156
                    height: 232
                    title: gameDelegate.modelData.title
                    artwork: gameDelegate.modelData.imageUrl || ""
                    showLabel: false
                    focusPolicy: Qt.NoFocus
                    pinned: ShellStore.isFavorite(gameDelegate.modelData)
                    currentItem: catalog.activeFocus && gameDelegate.GridView.isCurrentItem
                    parked: !catalog.activeFocus && gameDelegate.GridView.isCurrentItem
                    onClicked: {
                        catalog.currentIndex = gameDelegate.index
                        catalog.forceActiveFocus()
                        ShellStore.openGame(gameDelegate.modelData)
                    }
                }
            }
        }

        Column {
            anchors.centerIn: parent
            width: parent.width - 120
            spacing: 14
            visible: root.games.length === 0
            Text {
                width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
                text: root.cloudFavoritesOnly
                    ? (ShellStore.remoteFavoritesState === "loading" ? qsTr("Loading GeForce NOW favorites…")
                        : ShellStore.remoteFavoritesState === "error" ? qsTr("Couldn’t load GeForce NOW favorites")
                        : qsTr("No GeForce NOW favorites match"))
                    : ShellStore.catalogGames.length > 0 ? qsTr("No games match these filters")
                    : ShellStore.catalogState === "error" ? qsTr("Couldn’t reach the catalog") : qsTr("Loading GeForce NOW games…")
                color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 30; font.weight: Font.Black
            }
            Text {
                width: parent.width; horizontalAlignment: Text.AlignHCenter; wrapMode: Text.WordWrap
                text: root.cloudFavoritesOnly ? (ShellStore.remoteFavoritesError || "")
                    : ShellStore.catalogGames.length > 0 ? qsTr("Change the search or open Filter & sort to reset.")
                    : ShellStore.catalogState === "error" ? ShellStore.lastError
                    : qsTr("The shell stays responsive while the Rust core fetches NVIDIA’s public list.")
                visible: text !== ""
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 18
            }
            ConsoleActionButton {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: !root.cloudFavoritesOnly && ShellStore.catalogState === "error" && ShellStore.catalogGames.length === 0
                text: qsTr("Try again")
                glyph: "A"
                primary: true
                focusPolicy: Qt.NoFocus
                onClicked: ShellStore.refreshCatalog("")
            }
        }
    }

    GlassPanel {
        id: detailPanel
        x: 1338
        y: 230
        width: root.width - x - 96
        height: root.height - y - 172
        panelRadius: 40
        readonly property var game: root.selectedGame
        readonly property string ownedStore: ConsoleStores.ownedStore(game)
        readonly property var stores: ConsoleStores.stores(game)

        Column {
            x: 36; y: 40
            width: parent.width - 72
            spacing: 14
            Text {
                width: parent.width
                text: detailPanel.game
                    ? [detailPanel.game.publisherName || detailPanel.game.developerName || "",
                       (detailPanel.game.genres || []).slice(0, 1).map(genre => DesktopTokens.genreLabel(genre)).join("")]
                        .filter(Boolean).join(" · ").toUpperCase()
                    : qsTr("GEFORCE NOW LIBRARY")
                visible: text !== ""
                elide: Text.ElideRight
                color: Theme.textMuted
                font.family: Theme.monoFont
                font.pixelSize: 14
                font.weight: Font.Bold
                font.letterSpacing: 1.8
            }
            Text {
                width: parent.width
                text: detailPanel.game ? String(detailPanel.game.title || "") : qsTr("Pick a game")
                wrapMode: Text.WordWrap
                maximumLineCount: 2
                elide: Text.ElideRight
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 40
                font.weight: Font.Black
            }
            Row {
                visible: detailPanel.ownedStore !== ""
                spacing: 10
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 9; height: 9; radius: 5; color: Theme.mint }
                Text {
                    text: qsTr("In your %1 library").arg(ConsoleStores.label(detailPanel.ownedStore))
                    color: Theme.lightMode ? Qt.darker(Theme.mint, 2.2) : Theme.mint
                    font.family: Theme.bodyFont
                    font.pixelSize: 18
                    font.weight: Font.Bold
                }
            }
            Item { width: 1; height: 4; visible: detailPanel.stores.length > 0 }
            Text {
                visible: detailPanel.stores.length > 0
                text: qsTr("AVAILABLE ON")
                color: Theme.textMuted
                font.family: Theme.monoFont
                font.pixelSize: 14
                font.weight: Font.Bold
                font.letterSpacing: 1.8
            }
            Flow {
                width: parent.width
                spacing: 10
                visible: detailPanel.stores.length > 0
                Repeater {
                    model: detailPanel.stores
                    ConsoleStoreChip { required property string modelData; store: modelData }
                }
            }
            Text {
                width: parent.width
                text: detailPanel.game ? String(detailPanel.game.description || detailPanel.game.shortDescription || "") : ""
                visible: text !== ""
                wrapMode: Text.WordWrap
                maximumLineCount: 4
                elide: Text.ElideRight
                textFormat: Text.PlainText
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 18
                lineHeight: 1.2
            }
        }

        Column {
            x: 36
            width: parent.width - 72
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 36
            spacing: 14
            Text {
                width: parent.width
                visible: text !== ""
                text: root.cloudFavoritesOnly
                    ? (ShellStore.remoteFavoritesError || qsTr("Favorites coverage is partial or unknown. Home pins are separate."))
                    : ShellStore.catalogSource === "account-library" && Boolean(ShellStore.catalogState)
                        && ShellStore.catalogState !== "ready"
                        ? (ShellStore.catalogError || qsTr("The library refresh is incomplete. Your available games are still shown."))
                        : ""
                wrapMode: Text.WordWrap
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 15
            }
            Rectangle { width: parent.width; height: 1; color: Theme.seam }
            Repeater {
                model: detailPanel.game ? [
                    {glyph: "A", label: qsTr("Open details & play"), pin: false},
                    {glyph: "Y", label: ShellStore.isFavorite(detailPanel.game) ? qsTr("Remove from Home") : qsTr("Pin to Home"), pin: true}
                ] : []
                ItemDelegate {
                    required property var modelData
                    width: parent.width
                    height: 40
                    padding: 0
                    focusPolicy: Qt.NoFocus
                    Accessible.name: modelData.label
                    Accessible.role: Accessible.Button
                    background: Item {}
                    contentItem: ControllerGlyph { glyph: modelData.glyph; label: modelData.label; glyphSize: 30 }
                    onClicked: modelData.pin ? ShellStore.toggleFavorite(detailPanel.game) : root.openSelected()
                }
            }
        }
    }

    AppChrome {
        anchors.fill: parent
        title: qsTr("Library")
        currentRoute: "library"
        leftHints: [{glyph: "VIEW", label: qsTr("Search")}]
        rightHints: root.selectedGame
            ? [{glyph: "Y", label: ShellStore.isFavorite(root.selectedGame) ? qsTr("Remove from Home") : qsTr("Pin to Home")},
               {glyph: "A", label: qsTr("Details")}]
            : []
        onRouteRequested: route => AppController.navigate(route)
    }

    ConsoleFilterSheet {
        id: filterSheet
        objectName: "consoleLibraryFilterSheet"
        opened: root.filterSheetOpen
        eyebrow: qsTr("LIBRARY")
        title: qsTr("Filter & sort")
        description: qsTr("The grid updates behind the sheet as you choose.")
        sections: root.filterSections
        sectionIndex: 1
        onChosen: (section, option) => root.applyFilter(section, option)
        onResetRequested: root.resetFilters()
        onDismissed: root.closeFilterSheet()
    }

    VirtualKeyboard {
        id: virtualKeyboard
        objectName: "consoleLibraryKeyboard"
        anchors.fill: parent
        onAccepted: value => {
            root.searchQuery = value
            searchField.text = value
            catalog.forceActiveFocus()
        }
        onCanceled: catalog.forceActiveFocus()
    }
}
