import QtQuick
import OpenNOW

FocusScope {
    id: root
    focus: true
    property int currentIndex: Math.max(0, Math.min(games.length - 1, ShellStore.focusIndex("store")))
    readonly property var games: ShellStore.storeGames
    readonly property var selectedGame: gameAt(currentIndex)
    readonly property int columnCount: 10
    readonly property bool pageFailed: ShellStore.storeError !== "" || ShellStore.storeWarning !== ""
    onCurrentIndexChanged: Qt.callLater(root.revealSelection)

    function revealSelection() {
        catalogGrid.positionViewAtIndex(root.currentIndex, GridView.Contain)
    }

    function gameAt(index) {
        if (!games.length || index < 0)
            return null
        return index < games.length ? games[index] : null
    }

    function moveSelection(delta) {
        if (!games.length)
            return
        if ((delta === -1 && currentIndex % columnCount === 0)
                || (delta === 1 && currentIndex % columnCount === columnCount - 1))
            return
        if (delta === columnCount && currentIndex + delta >= games.length) {
            if (Math.floor(currentIndex / columnCount) < Math.floor((games.length - 1) / columnCount))
                currentIndex = games.length - 1
            else
                root.continuePaging()
        } else {
            currentIndex = Math.max(0, Math.min(games.length - 1, currentIndex + delta))
        }
        ShellStore.rememberFocus("store", currentIndex)
    }

    function continuePaging() {
        if (ShellStore.storeLoading)
            return
        if (root.pageFailed)
            ShellStore.retryStore()
        else if (ShellStore.storeHasMore)
            ShellStore.requestStorePage()
    }

    function openSelected() {
        const game = gameAt(currentIndex)
        if (game)
            ShellStore.openGame(game)
        else if (ShellStore.storeState === "error")
            ShellStore.retryStore()
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Left) root.moveSelection(-1)
        else if (event.key === Qt.Key_Right) root.moveSelection(1)
        else if (event.key === Qt.Key_Up) root.moveSelection(-root.columnCount)
        else if (event.key === Qt.Key_Down) root.moveSelection(root.columnCount)
        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
            if (!event.isAutoRepeat)
                root.openSelected()
        } else return
        event.accepted = true
    }

    ScreenBackground {
        artwork: root.selectedGame ? (root.selectedGame.heroImageUrl || root.selectedGame.imageUrl || "") : ""
        tint: "#18230F"
    }

    Text {
        id: eyebrow
        x: 96; y: 120
        text: qsTr("GEFORCE NOW CATALOG")
        color: Theme.textMuted
        font.family: Theme.monoFont
        font.pixelSize: 14
        font.weight: Font.DemiBold
        font.letterSpacing: 1.68
    }
    Text {
        x: 96; y: 139
        text: qsTr("Available games")
        color: Theme.label
        font.family: Theme.displayFont
        font.pixelSize: 40
        font.weight: Font.Black
        font.letterSpacing: -0.8
        Accessible.role: Accessible.Heading
        Accessible.name: text
    }
    Text {
        objectName: "consoleStorePageStatus"
        anchors.right: parent.right
        anchors.rightMargin: 96
        y: 163
        text: (ShellStore.storeTotalCount > 0
            ? qsTr("Loaded %1 of %2 games").arg(root.games.length.toLocaleString(Qt.locale(), "f", 0))
                .arg(Number(ShellStore.storeTotalCount).toLocaleString(Qt.locale(), "f", 0))
            : qsTr("%1 loaded").arg(root.games.length.toLocaleString(Qt.locale(), "f", 0))).toUpperCase()
        color: Theme.textMuted
        font.family: Theme.monoFont
        font.pixelSize: 15
        font.weight: Font.DemiBold
        font.letterSpacing: 1.4
    }

    Item {
        x: 80; y: 192
        width: root.width - 160
        height: strip.y - y - 8
        clip: true

        GridView {
            id: catalogGrid
            objectName: "consoleStoreGrid"
            x: 20; y: 16
            width: parent.width - 20
            height: parent.height - 16
            cellWidth: width / root.columnCount
            cellHeight: 256
            model: root.games
            currentIndex: root.currentIndex
            highlightFollowsCurrentItem: false
            boundsBehavior: Flickable.StopAtBounds
            delegate: Item {
                id: posterCell
                required property int index
                required property var modelData
                width: catalogGrid.cellWidth
                height: catalogGrid.cellHeight
                PosterTile {
                    width: Math.min(156, catalogGrid.cellWidth - 18)
                    height: Math.round(width * 234 / 156)
                    title: posterCell.modelData && posterCell.modelData.title ? posterCell.modelData.title : qsTr("Game")
                    artwork: posterCell.modelData ? (posterCell.modelData.imageUrl || posterCell.modelData.heroImageUrl || "") : ""
                    stores: ConsoleStores.stores(posterCell.modelData)
                    showLabel: false
                    focusPolicy: Qt.NoFocus
                    currentItem: root.activeFocus && root.currentIndex === posterCell.index
                    onClicked: {
                        root.currentIndex = posterCell.index
                        root.forceActiveFocus()
                        root.openSelected()
                    }
                }
            }
            footer: Item {
                width: catalogGrid.width
                height: footerRow.visible ? 96 : 0
                Row {
                    id: footerRow
                    anchors.centerIn: parent
                    spacing: 20
                    visible: root.games.length > 0 && (ShellStore.storeLoading || ShellStore.storeHasMore || root.pageFailed)
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        width: Math.min(implicitWidth, 900)
                        text: ShellStore.storeLoading ? qsTr("Loading more games…")
                            : ShellStore.storeError || ShellStore.storeWarning || qsTr("Move down past the last row to load more")
                        color: root.pageFailed ? Theme.coral : Theme.textMuted
                        wrapMode: Text.Wrap
                        textFormat: Text.PlainText
                        font.family: Theme.bodyFont
                        font.pixelSize: 17
                        font.weight: Font.DemiBold
                    }
                    ConsoleActionButton {
                        anchors.verticalCenter: parent.verticalCenter
                        visible: !ShellStore.storeLoading
                        height: 56
                        focusPolicy: Qt.NoFocus
                        text: root.pageFailed ? qsTr("Try again") : qsTr("Load more")
                        onClicked: root.continuePaging()
                    }
                }
            }
        }

        Column {
            anchors.centerIn: parent
            width: Math.min(parent.width - 48, 800)
            spacing: 14
            visible: root.games.length === 0
            Text {
                width: parent.width
                horizontalAlignment: Text.AlignHCenter
                text: ShellStore.storeState === "error" ? qsTr("Catalog unavailable")
                    : ShellStore.storeState === "ready" ? qsTr("No games match these filters") : qsTr("Loading the live catalog…")
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 30
                font.weight: Font.Black
            }
            Text {
                width: parent.width; text: ShellStore.storeError; visible: text !== ""
                horizontalAlignment: Text.AlignHCenter; wrapMode: Text.Wrap; textFormat: Text.PlainText
                color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 18
            }
            ConsoleActionButton {
                anchors.horizontalCenter: parent.horizontalCenter
                visible: ShellStore.storeState === "error"
                text: qsTr("Try again")
                glyph: "A"
                primary: true
                focusPolicy: Qt.NoFocus
                onClicked: ShellStore.retryStore()
            }
        }
    }

    GlassPanel {
        id: strip
        readonly property var game: root.selectedGame
        readonly property var stores: ConsoleStores.stores(game)
        readonly property string ownedStore: ConsoleStores.ownedStore(game)
        x: 96
        y: root.height - 260
        width: root.width - 192
        height: 96
        panelRadius: 32
        strong: true
        visible: game !== null
        Accessible.role: Accessible.StaticText
        Accessible.name: game ? String(game.title || "") : ""

        Column {
            x: 29
            anchors.verticalCenter: parent.verticalCenter
            width: parent.width - stripFacts.width - 80
            spacing: -1
            Text {
                width: parent.width
                text: strip.game ? String(strip.game.title || "") : ""
                elide: Text.ElideRight
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 26
                font.weight: Font.Black
            }
            Text {
                width: parent.width
                text: strip.game
                    ? [strip.game.publisherName || strip.game.developerName || "",
                       (strip.game.genres || []).slice(0, 1).map(genre => DesktopTokens.genreLabel(genre)).join("")]
                        .filter(Boolean).join(" · ")
                    : ""
                visible: text !== ""
                elide: Text.ElideRight
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 16
                font.weight: Font.Bold
            }
        }

        Row {
            id: stripFacts
            anchors.right: parent.right
            anchors.rightMargin: 29
            anchors.verticalCenter: parent.verticalCenter
            spacing: 22
            Text {
                anchors.verticalCenter: parent.verticalCenter
                visible: strip.stores.length > 0
                text: qsTr("AVAILABLE ON")
                color: Theme.textMuted
                font.family: Theme.monoFont
                font.pixelSize: 13
                font.weight: Font.Bold
                font.letterSpacing: 1.56
            }
            Row {
                anchors.verticalCenter: parent.verticalCenter
                visible: strip.stores.length > 0
                spacing: 8
                Repeater {
                    model: strip.stores.slice(0, 4)
                    ConsoleStoreChip {
                        required property string modelData
                        store: modelData
                        markSize: 32
                        inset: 6
                    }
                }
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                visible: strip.ownedStore !== ""
                width: 1; height: 44; color: Theme.seam
            }
            Row {
                anchors.verticalCenter: parent.verticalCenter
                visible: strip.ownedStore !== ""
                spacing: 8
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 9; height: 9; radius: 5; color: Theme.mint }
                Text {
                    text: qsTr("In your %1 library").arg(ConsoleStores.label(strip.ownedStore))
                    color: Theme.lightMode ? Qt.darker(Theme.mint, 2.2) : Theme.mint
                    font.family: Theme.bodyFont
                    font.pixelSize: 17
                    font.weight: Font.ExtraBold
                }
            }
        }
    }

    AppChrome {
        anchors.fill: parent
        title: qsTr("Store")
        currentRoute: "store"
        leftHints: [{glyph: "B", label: qsTr("Back")}]
        rightHints: root.selectedGame ? [{glyph: "A", label: qsTr("Details")}]
            : ShellStore.storeState === "error" ? [{glyph: "A", label: qsTr("Try again")}] : []
        onRouteRequested: route => AppController.navigate(route)
    }

    Connections {
        target: ShellStore
        function onStoreSessionReset() { if (root.visible) ShellStore.ensureStore("") }
    }
    Component.onCompleted: {
        ShellStore.ensureStore("")
        Qt.callLater(root.revealSelection)
    }
}
