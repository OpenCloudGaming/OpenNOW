import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property string searchQuery: ""
    property int platformIndex: 0
    property int genreIndex: 0
    property int sortIndex: 0
    readonly property real posterFactor: Math.max(0.75, Math.min(1.5,
        Number(ShellStore.settings.posterSizeScale || 1.05))) / 1.05
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
    readonly property var sortOptions: ["Popular", "Title", "Recently played", "Store", "Favorites first"]
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
        for (let index = 0; index < ShellStore.catalogGames.length; ++index) {
            const game = ShellStore.catalogGames[index]
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
            if (root.sortIndex === 0)
                return 0
            return String(left.title).localeCompare(String(right.title))
        })
        return filtered
    }
    readonly property var selectedGame: games.length > 0 ? games[Math.max(0, catalog.currentIndex)] : null

    onPlatformOptionsChanged: platformIndex = Math.max(0, Math.min(platformOptions.length - 1, platformIndex))

    function storeName(game) {
        return game && game.availableStores && game.availableStores.length ? game.availableStores[0] : "GFN"
    }

    function storeGlyph(game) {
        const store = storeName(game).toUpperCase()
        if (store.indexOf("EPIC") >= 0) return "E"
        if (store.indexOf("UBISOFT") >= 0) return "U"
        if (store.indexOf("BATTLE") >= 0) return "B"
        if (store.indexOf("XBOX") >= 0) return "X"
        if (store.indexOf("GOG") >= 0) return "G"
        return "S"
    }

    function storeColor(glyph) {
        if (glyph === "E") return Theme.cartEpic
        if (glyph === "U") return Theme.cartUbisoft
        if (glyph === "B") return Theme.cartBattlenet
        if (glyph === "X") return Theme.cartXbox
        if (glyph === "G") return Theme.cartGog
        return Theme.cartSteam
    }

    function showSearchKeyboard() {
        platformFilter.expanded = false
        genreFilter.expanded = false
        sortFilter.expanded = false
        virtualKeyboard.openKeyboard(root.searchQuery)
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Y && !virtualKeyboard.presented) {
            root.showSearchKeyboard()
            event.accepted = true
        }
    }

    ScreenBackground { tint: "#354016" }

    GlassPanel {
        x: 120; y: 108
        width: 1134
        height: root.height - 232
        panelRadius: 38

        Row {
            id: filters
            x: 28; y: 24; spacing: 12
            z: 200
            TextField {
                id: searchField
                width: 400; height: 52
                placeholderText: qsTr("Search GeForce NOW games")
                text: root.searchQuery
                color: Theme.label
                placeholderTextColor: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: 16
                font.weight: Font.Bold
                leftPadding: 84
                rightPadding: 20
                Accessible.name: qsTr("Search games")
                KeyNavigation.right: platformFilter
                KeyNavigation.down: catalog
                onTextEdited: root.searchQuery = text
                onAccepted: catalog.forceActiveFocus()
                background: Rectangle { radius: 26; color: searchField.activeFocus ? Theme.glassStrong : Qt.rgba(1, 1, 1, 0.10); border.color: searchField.activeFocus ? Theme.focus : Theme.seam; border.width: searchField.activeFocus ? 3 : 1 }
                Rectangle {
                    x: 14; anchors.verticalCenter: parent.verticalCenter
                    width: 26; height: 26; radius: 13
                    color: Theme.face
                    Text { anchors.centerIn: parent; text: "Y"; color: Theme.faceText; font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Black }
                    TapHandler { onTapped: root.showSearchKeyboard() }
                }
                Item {
                    x: 52; anchors.verticalCenter: parent.verticalCenter
                    width: 20; height: 20
                    Rectangle { x: 2; y: 2; width: 13; height: 13; radius: 7; color: "transparent"; border.color: Qt.rgba(1,1,1,0.70); border.width: 2 }
                    Rectangle { x: 14; y: 14; width: 7; height: 2; radius: 1; rotation: 45; color: Qt.rgba(1,1,1,0.70) }
                }
            }
            FilterDropdown {
                id: platformFilter
                prefix: qsTr("Platform")
                options: root.platformOptions
                currentIndex: root.platformIndex
                width: 157; height: 52
                KeyNavigation.left: searchField
                KeyNavigation.right: genreFilter
                KeyNavigation.down: catalog
                onExpandedChanged: if (expanded) { genreFilter.expanded = false; sortFilter.expanded = false }
                onOptionSelected: index => root.platformIndex = index
            }
            FilterDropdown {
                id: genreFilter
                prefix: qsTr("Genre")
                options: root.genreOptions
                currentIndex: Math.min(root.genreIndex, root.genreOptions.length - 1)
                width: 137; height: 52
                KeyNavigation.left: platformFilter
                KeyNavigation.right: sortFilter
                KeyNavigation.down: catalog
                onExpandedChanged: if (expanded) { platformFilter.expanded = false; sortFilter.expanded = false }
                onOptionSelected: index => root.genreIndex = index
            }
            FilterDropdown {
                id: sortFilter
                prefix: qsTr("Sort")
                options: root.sortOptions
                currentIndex: root.sortIndex
                width: 160; height: 52
                KeyNavigation.left: genreFilter
                KeyNavigation.down: catalog
                onExpandedChanged: if (expanded) { platformFilter.expanded = false; genreFilter.expanded = false }
                onOptionSelected: index => root.sortIndex = index
            }
        }

        Item {
            // The focus ring and selected scale both extend outside the tile.
            // Reserve a real gutter inside the clipped viewport for that motion.
            x: 12; y: 76
            width: parent.width - 24
            height: parent.height - 92
            clip: true
            GridView {
                id: catalog
                x: 16; y: 28
                width: 1092
                height: parent.height - 44
                cellWidth: 156
                cellHeight: 226
                clip: false
                model: root.games
                Component.onCompleted: currentIndex = ShellStore.focusIndex("library")
                onCurrentIndexChanged: ShellStore.rememberFocus("library", currentIndex)
                focus: true
                KeyNavigation.up: platformFilter
                keyNavigationWraps: false
                delegate: Item {
                    id: gameDelegate
                    required property var modelData
                    required property int index
                    width: catalog.cellWidth; height: catalog.cellHeight
                    PosterTile {
                        x: 0; y: 0
                        width: 140
                        height: 210
                        title: modelData.title
                        artwork: modelData.imageUrl || ""
                        storeGlyph: root.storeGlyph(modelData)
                        storeColor: root.storeColor(storeGlyph)
                        showStoreBadge: false
                        showLabel: false
                        currentItem: gameDelegate.GridView.isCurrentItem
                        onClicked: ShellStore.openGame(modelData)
                    }
                    Rectangle {
                        visible: ShellStore.isFavorite(modelData)
                        x: 102; y: 10
                        width: 28; height: 28; radius: 14; color: Theme.yellow
                        Text { anchors.centerIn: parent; text: "★"; color: Theme.contrastText(Theme.yellow); font.pixelSize: 15; font.weight: Font.Black }
                    }
                }
            }
        }

        Column {
            anchors.centerIn: parent
            spacing: 12
            visible: root.games.length === 0
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: ShellStore.catalogState === "error" ? qsTr("Couldn’t reach the catalog") : qsTr("Loading GeForce NOW games…"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 24; font.weight: Font.Black }
            Text { anchors.horizontalCenter: parent.horizontalCenter; text: ShellStore.catalogGames.length > 0 ? qsTr("No games match these filters.") : (ShellStore.catalogState === "error" ? ShellStore.lastError : qsTr("The shell stays responsive while the Rust core fetches NVIDIA’s public list.")); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 14 }
            GlassButton { anchors.horizontalCenter: parent.horizontalCenter; visible: ShellStore.catalogState === "error"; text: qsTr("Try again"); glyph: "A"; primary: true; onClicked: ShellStore.refreshCatalog("") }
        }
    }

    GlassPanel {
        x: 1278
        y: 108
        width: root.width - x - 120
        height: root.height - 232
        panelRadius: 38

        Column {
            anchors.fill: parent
            anchors.margins: 28
            spacing: 18
            RoundedArtwork {
                width: parent.width; height: 250
                cornerRadius: 28
                scrimStart: 1
                fallbackColor: root.selectedGame ? root.storeColor(root.storeGlyph(root.selectedGame)) : Theme.cartSteam
                artwork: root.selectedGame ? (root.selectedGame.heroImageUrl || root.selectedGame.imageUrl || "") : ""
            }
            Text { width: parent.width; text: root.selectedGame ? root.selectedGame.title : qsTr("GeForce NOW catalog"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 28; font.weight: Font.Black; elide: Text.ElideRight }
            Text { width: parent.width; text: root.selectedGame ? qsTr("%1 · Available on GeForce NOW").arg(root.storeName(root.selectedGame)) : (ShellStore.catalogSource === "account-library" ? qsTr("%1 games in your library").arg(ShellStore.catalogTotalCount) : qsTr("%1 supported games").arg(ShellStore.catalogTotalCount)); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 15; elide: Text.ElideRight }
            Row {
                spacing: 8
                Repeater {
                    model: root.selectedGame ? [ShellStore.isFavorite(root.selectedGame)
                        ? qsTr("● In your %1 library").arg(root.storeName(root.selectedGame))
                        : qsTr("Available on %1").arg(root.storeName(root.selectedGame))] : []
                    GlassPanel {
                        required property string modelData
                        width: chipText.implicitWidth + 26; height: 36; panelRadius: 18; strong: true
                        Text { id: chipText; anchors.centerIn: parent; text: modelData; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 13; font.weight: Font.Bold }
                    }
                }
            }
            GlassButton {
                width: parent.width
                text: root.selectedGame && ShellStore.isFavorite(root.selectedGame)
                    ? qsTr("Remove from My games") : qsTr("Add to My games")
                glyph: "Y"
                primary: root.selectedGame && !ShellStore.isFavorite(root.selectedGame)
                enabled: root.selectedGame !== null
                onClicked: ShellStore.toggleFavorite(root.selectedGame)
            }
            GlassButton { width: parent.width; text: ShellStore.signedIn ? qsTr("Play now") : qsTr("Sign in to play"); glyph: "X"; enabled: root.selectedGame !== null; onClicked: ShellStore.signedIn ? ShellStore.openGame(root.selectedGame) : AppController.navigate("sign-in") }
        }
    }

    AppChrome { anchors.fill: parent; title: qsTr("GeForce NOW library"); currentRoute: "library"; onRouteRequested: route => AppController.navigate(route) }
    VirtualKeyboard {
        id: virtualKeyboard
        anchors.fill: parent
        onAccepted: value => {
            root.searchQuery = value
            searchField.text = value
            catalog.forceActiveFocus()
        }
        onCanceled: catalog.forceActiveFocus()
    }
}
