import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    focus: true
    property int currentIndex: Math.max(0, Math.min(32, ShellStore.focusIndex("store")))
    readonly property var games: ShellStore.storeGames
    readonly property var selectedGame: gameAt(currentIndex)
    readonly property int columnCount: 11

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
        currentIndex = Math.max(0, Math.min(games.length - 1, currentIndex + delta))
        ShellStore.rememberFocus("store", currentIndex)
        catalogGrid.positionViewAtIndex(currentIndex, GridView.Contain)
    }

    function openSelected() {
        const game = gameAt(currentIndex)
        if (game)
            ShellStore.openGame(game)
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Left) root.moveSelection(-1)
        else if (event.key === Qt.Key_Right) root.moveSelection(1)
        else if (event.key === Qt.Key_Up) root.moveSelection(-root.columnCount)
        else if (event.key === Qt.Key_Down) root.moveSelection(root.columnCount)
        else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space)
            root.openSelected()
        else return
        event.accepted = true
    }

    ScreenBackground {
        artwork: root.selectedGame ? (root.selectedGame.heroImageUrl || root.selectedGame.imageUrl || "") : ""
        tint: "#18230F"
    }

    GlassPanel {
        x: 40
        y: 108
        width: 1840
        height: 848
        panelRadius: 44

        Item {
            anchors.fill: parent
            anchors.margins: 28

            Text {
                text: qsTr("Available games")
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 24
                font.weight: Font.Black
                font.letterSpacing: -0.48
            }

            Text {
                anchors.right: parent.right
                text: ShellStore.storeTotalCount > 0
                    ? qsTr("%1 games").arg(ShellStore.storeTotalCount)
                    : qsTr("%1 loaded").arg(root.games.length)
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 14
                font.weight: Font.Bold
            }

            GridView {
                id: catalogGrid
                x: 0
                y: 40
                width: parent.width
                height: parent.height - 40
                cellWidth: width / root.columnCount
                cellHeight: 250
                model: root.games
                currentIndex: root.currentIndex
                clip: true
                boundsBehavior: Flickable.StopAtBounds
                delegate: ItemDelegate {
                    required property int index
                    required property var modelData
                    readonly property var game: modelData
                    width: GridView.view.cellWidth - 24
                    height: 210
                    padding: 0
                    focusPolicy: Qt.NoFocus
                    Accessible.name: game && game.title ? game.title : qsTr("Game")
                    Accessible.role: Accessible.Button
                    onClicked: {
                        root.currentIndex = index
                        root.forceActiveFocus()
                        root.openSelected()
                    }
                    background: RoundedArtwork {
                        artwork: parent.game ? (parent.game.imageUrl || parent.game.heroImageUrl || "") : ""
                        fallbackColor: Theme.glassStrong
                        cornerRadius: 18
                        scrimStart: 1
                    }
                    contentItem: Item {}
                    FocusFrame {
                        focused: root.currentIndex === parent.index
                        frameRadius: 21
                    }
                    scale: root.currentIndex === index ? 1.045 : 1
                    z: root.currentIndex === index ? 20 : 0
                    Behavior on scale {
                        NumberAnimation {
                            duration: Theme.focusDuration
                            easing.type: Easing.OutCubic
                        }
                    }
                }
            }

            Column {
                anchors.centerIn: parent
                spacing: 12
                visible: root.games.length === 0
                Text {
                    anchors.horizontalCenter: parent.horizontalCenter
                    text: ShellStore.storeState === "error" ? qsTr("Catalog unavailable") : qsTr("Loading the live catalog…")
                    color: Theme.label
                    font.family: Theme.displayFont
                    font.pixelSize: 25
                    font.weight: Font.Black
                }
                GlassButton {
                    anchors.horizontalCenter: parent.horizontalCenter
                    visible: ShellStore.storeState === "error"
                    text: qsTr("Try again")
                    glyph: "A"
                    primary: true
                    onClicked: ShellStore.refreshStore("")
                }
            }
        }
    }

    AppChrome {
        anchors.fill: parent
        title: qsTr("Store")
        currentRoute: "store"
        onRouteRequested: route => AppController.navigate(route)
    }

    Component.onCompleted: ShellStore.refreshStore("")
}
