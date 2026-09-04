import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property int currentIndex: 0
    property bool editMenuOpen: false
    property bool moveMode: false
    property var movePreviewIds: []
    property var editingGame: null
    property string editingGameId: ""
    property string editingTileSize: "square"

    readonly property var homeIds: moveMode
        ? movePreviewIds : (ShellStore.settings.favoriteGameIds || [])
    readonly property var games: {
        const result = []
        for (let idIndex = 0; idIndex < root.homeIds.length; ++idIndex) {
            const wanted = String(root.homeIds[idIndex])
            for (let gameIndex = 0; gameIndex < ShellStore.catalogGames.length; ++gameIndex) {
                const game = ShellStore.catalogGames[gameIndex]
                if (ShellStore.gameIdentity(game) === wanted) {
                    result.push(game)
                    break
                }
            }
        }
        return result
    }
    readonly property var layoutItems: {
        const items = []
        let row = 0
        let column = 0
        for (let index = 0; index < root.games.length; ++index) {
            const game = root.games[index]
            const wide = ShellStore.homeTileSize(game) === "wide"
            const span = wide ? 2 : 1
            if (column + span > 8) {
                ++row
                column = 0
            }
            if (row >= 3)
                break
            items.push({
                game: game,
                gameId: ShellStore.gameIdentity(game),
                add: false,
                wide: wide,
                row: row,
                column: column,
                x: column * 192,
                y: row * 192
            })
            column += span
        }
        if (column >= 8) {
            ++row
            column = 0
        }
        if (row < 3) {
            items.push({
                game: null,
                gameId: "",
                add: true,
                wide: false,
                row: root.games.length === 0 ? 1 : row,
                column: root.games.length === 0 ? 3.5 : column,
                x: root.games.length === 0 ? 672 : column * 192,
                y: (root.games.length === 0 ? 1 : row) * 192
            })
        }
        return items
    }
    readonly property var selectedItem: layoutItems.length
        ? layoutItems[Math.max(0, Math.min(currentIndex, layoutItems.length - 1))] : null
    readonly property var selectedGame: selectedItem && !selectedItem.add ? selectedItem.game : null

    function storeGlyph(game) {
        const store = game && game.availableStores && game.availableStores.length
            ? String(game.availableStores[0]).toUpperCase() : "GFN"
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

    function storeLabel(game) {
        return game && game.availableStores && game.availableStores.length
            ? String(game.availableStores[0]).toUpperCase() : "GEFORCE NOW"
    }

    // Focus is restored once the tile layout has settled. A Timer is used instead
    // of Qt.callLater so a pending restore dies with the screen when the shell
    // swaps surfaces, rather than calling into a half-destroyed object.
    Timer {
        id: deferredFocus
        interval: 0
        repeat: false
        property var indexResolver: null
        function schedule(resolver) {
            indexResolver = resolver || null
            restart()
        }
        onTriggered: {
            const resolver = indexResolver
            indexResolver = null
            if (resolver)
                root.currentIndex = resolver()
            root.focusCurrent()
        }
    }

    function focusCurrent() {
        if (!gameRepeater.count)
            return
        root.currentIndex = Math.max(0, Math.min(root.currentIndex, gameRepeater.count - 1))
        const item = gameRepeater.itemAt(root.currentIndex)
        if (item)
            item.forceActiveFocus()
    }

    function indexForGameId(id) {
        for (let index = 0; index < layoutItems.length; ++index) {
            if (layoutItems[index].gameId === id)
                return index
        }
        return 0
    }

    function directionalIndex(direction) {
        if (!selectedItem)
            return currentIndex
        let bestIndex = -1
        let bestScore = Number.MAX_VALUE
        for (let index = 0; index < layoutItems.length; ++index) {
            if (index === currentIndex)
                continue
            const candidate = layoutItems[index]
            let valid = false
            let primary = 0
            let secondary = 0
            if (direction === "left" || direction === "right") {
                valid = candidate.row === selectedItem.row
                    && (direction === "left" ? candidate.column < selectedItem.column
                                             : candidate.column > selectedItem.column)
                primary = Math.abs(candidate.column - selectedItem.column)
                secondary = 0
            } else {
                valid = direction === "up" ? candidate.row < selectedItem.row
                                            : candidate.row > selectedItem.row
                primary = Math.abs(candidate.row - selectedItem.row)
                secondary = Math.abs(candidate.column - selectedItem.column)
            }
            const score = primary * 100 + secondary
            if (valid && score < bestScore) {
                bestScore = score
                bestIndex = index
            }
        }
        return bestIndex >= 0 ? bestIndex : currentIndex
    }

    function moveSelection(direction) {
        const next = directionalIndex(direction)
        if (next === currentIndex)
            return
        currentIndex = next
        ShellStore.rememberFocus("home", next)
        deferredFocus.schedule(null)
    }

    function reorderPreview(direction) {
        const targetLayoutIndex = directionalIndex(direction)
        if (targetLayoutIndex === currentIndex || layoutItems[targetLayoutIndex].add)
            return
        const sourceId = editingGameId
        const targetId = layoutItems[targetLayoutIndex].gameId
        const updated = movePreviewIds.slice(0)
        const sourceIndex = updated.indexOf(sourceId)
        const targetIndex = updated.indexOf(targetId)
        if (sourceIndex < 0 || targetIndex < 0)
            return
        updated.splice(sourceIndex, 1)
        updated.splice(targetIndex, 0, sourceId)
        movePreviewIds = updated
        deferredFocus.schedule(() => root.indexForGameId(sourceId))
    }

    function openEditMenu(game, index) {
        if (!game || moveMode)
            return
        editingGame = game
        editingGameId = ShellStore.gameIdentity(game)
        editingTileSize = ShellStore.homeTileSize(game)
        currentIndex = index
        editMenuOpen = true
    }

    function beginMove() {
        movePreviewIds = (ShellStore.settings.favoriteGameIds || []).slice(0)
        moveMode = true
        editMenuOpen = false
        currentIndex = indexForGameId(editingGameId)
        deferredFocus.schedule(null)
        Accessible.announce(qsTr("Move mode. Use the D-pad, press A to place, or B to cancel."))
    }

    function commitMove() {
        ShellStore.setHomeOrder(movePreviewIds)
        moveMode = false
        deferredFocus.schedule(() => root.indexForGameId(root.editingGameId))
    }

    function cancelMove() {
        moveMode = false
        deferredFocus.schedule(() => root.indexForGameId(root.editingGameId))
        Accessible.announce(qsTr("Home move cancelled"))
    }

    function handleTileKey(event, game, index) {
        if (moveMode) {
            if (event.key === Qt.Key_Left) root.reorderPreview("left")
            else if (event.key === Qt.Key_Right) root.reorderPreview("right")
            else if (event.key === Qt.Key_Up) root.reorderPreview("up")
            else if (event.key === Qt.Key_Down) root.reorderPreview("down")
            else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space)
                root.commitMove()
            else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back)
                root.cancelMove()
            else return
            event.accepted = true
            return
        }
        if ((event.key === Qt.Key_X || event.key === Qt.Key_Menu) && game) {
            root.openEditMenu(game, index)
            event.accepted = true
            return
        }
        if (event.key === Qt.Key_Left) root.moveSelection("left")
        else if (event.key === Qt.Key_Right) root.moveSelection("right")
        else if (event.key === Qt.Key_Up) root.moveSelection("up")
        else if (event.key === Qt.Key_Down) root.moveSelection("down")
        else return
        event.accepted = true
    }

    ScreenBackground {
        artwork: root.selectedGame ? (root.selectedGame.heroImageUrl || root.selectedGame.imageUrl || "") : ""
        tint: "#15253A"
    }

    GlassPanel {
        x: Math.round((root.width - width) / 2)
        y: 207
        width: 1586
        height: 626
        panelRadius: 42
        color: Qt.rgba(0.055, 0.063, 0.094, 0.58)

        Item {
            x: 33
            y: 33
            width: 1520
            height: 560

            Repeater {
                id: gameRepeater
                model: root.layoutItems
                GameTile {
                    id: tile
                    required property var modelData
                    required property int index
                    readonly property bool isAddTile: modelData.add
                    readonly property var game: modelData.game
                    x: modelData.x
                    y: modelData.y
                    wide: modelData.wide
                    width: modelData.wide ? 368 : 176
                    title: isAddTile ? qsTr("Add a game") : game.title
                    artwork: isAddTile ? "" : (modelData.wide
                        ? (game.heroImageUrl || game.imageUrl || "")
                        : (game.imageUrl || game.heroImageUrl || ""))
                    storeGlyph: isAddTile ? "+" : root.storeGlyph(game)
                    storeColor: isAddTile ? Theme.glassStrong : root.storeColor(storeGlyph)
                    addTile: isAddTile
                    session: index === 0 && !isAddTile
                    eyebrow: index === 0 && !isAddTile ? root.storeLabel(game) + " · RECENT" : ""
                    currentItem: root.currentIndex === index
                    opacity: root.moveMode && modelData.gameId !== root.editingGameId ? 0.58 : 1
                    onActiveFocusChanged: if (activeFocus) root.currentIndex = index
                    onClicked: {
                        if (root.moveMode) {
                            root.commitMove()
                        } else if (isAddTile) {
                            AppController.navigate("library")
                        } else {
                            ShellStore.openGame(game)
                        }
                    }
                    onMenuRequested: root.openEditMenu(game, index)
                    Keys.onPressed: event => root.handleTileKey(event, game, index)
                    Behavior on opacity {
                        NumberAnimation { duration: Theme.focusDuration }
                    }
                }
            }
        }

        Column {
            anchors.horizontalCenter: parent.horizontalCenter
            y: 88
            spacing: 6
            visible: root.games.length === 0
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Your Home is ready")
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 28
                font.weight: Font.Black
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: qsTr("Add games from Library to build your own layout")
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 15
                font.weight: Font.Bold
            }
        }
    }

    GlassPanel {
        visible: root.moveMode
        anchors.horizontalCenter: parent.horizontalCenter
        y: 126
        width: moveLabel.implicitWidth + 54
        height: 52
        panelRadius: 26
        strong: true
        border.color: Theme.focus
        border.width: 3
        Text {
            id: moveLabel
            anchors.centerIn: parent
            text: qsTr("MOVE TILE  ·  D-PAD MOVE  ·  A PLACE  ·  B CANCEL")
            color: Theme.label
            font.family: Theme.bodyFont
            font.pixelSize: 14
            font.weight: Font.Black
            font.letterSpacing: 0.7
        }
    }

    HomeTileMenu {
        anchors.fill: parent
        visible: root.editMenuOpen
        game: root.editingGame
        tileSize: root.editingTileSize
        homePosition: root.currentIndex
        onMoveRequested: root.beginMove()
        onSizeRequested: size => {
            root.editingTileSize = size
            ShellStore.setHomeTileSize(root.editingGame, size)
        }
        onDetailsRequested: {
            root.editMenuOpen = false
            ShellStore.openGame(root.editingGame)
        }
        onRemoveRequested: {
            root.editMenuOpen = false
            ShellStore.removeFromHome(root.editingGame)
            deferredFocus.schedule(null)
        }
        onCloseRequested: {
            root.editMenuOpen = false
            deferredFocus.schedule(null)
        }
    }

    onLayoutItemsChanged: deferredFocus.schedule(null)
    Component.onCompleted: deferredFocus.schedule(
        () => Math.min(ShellStore.focusIndex("home"), Math.max(0, root.layoutItems.length - 1)))

    AppChrome {
        anchors.fill: parent
        title: qsTr("My games")
        currentRoute: "home"
        rightHints: root.moveMode
            ? [{glyph:"A", label:qsTr("Place")}, {glyph:"B", label:qsTr("Cancel")}]
            : root.homeIds.length === 0
                ? [{glyph:"A", label:qsTr("Add game")}]
                : [{glyph:"A", label:qsTr("Play")}, {glyph:"X", label:qsTr("Edit tile")}]
        onRouteRequested: route => AppController.navigate(route)
    }
}
