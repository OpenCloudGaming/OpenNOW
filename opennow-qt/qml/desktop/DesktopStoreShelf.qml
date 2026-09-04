import QtQuick
import OpenNOW

Item {
    id: root

    property string title: ""
    property string eyebrow: ""
    property string seeAllText: qsTr("See all")
    property bool showSeeAll: true
    property var games: []
    property int selectedIndex: -1
    property bool active: false

    signal gameActivated(var game)
    signal gamePointed(int index)
    signal seeAllRequested()

    readonly property int railGap: 14
    readonly property int tileCount: Math.max(1, Math.min(root.games.length || 1, Math.floor((width + railGap) / (132 + railGap))))
    readonly property int tileWidth: Math.max(132, Math.floor((width - railGap * (Math.max(1, tileCount) - 1)) / Math.max(1, tileCount)))
    readonly property int tileHeight: Math.round(tileWidth * 250 / 132)

    height: 31 + tileHeight

    Row {
        x: 0
        y: 0
        height: 20
        spacing: 9

        Text {
            anchors.baseline: sectionEyebrow.baseline
            text: root.title
            color: "#FFFFFF"
            font.family: Theme.displayFont
            font.pixelSize: 16
            font.weight: Font.Black
            font.letterSpacing: -0.16
        }

        Text {
            id: sectionEyebrow
            visible: root.eyebrow.length > 0
            text: root.eyebrow
            color: Qt.rgba(1, 1, 1, 0.40)
            font.family: Theme.monoFont
            font.pixelSize: 10
            font.weight: Font.DemiBold
            font.letterSpacing: 0.4
        }
    }

    Row {
        visible: root.showSeeAll
        anchors.right: parent.right
        y: 1
        height: 18
        spacing: 5

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.seeAllText
            color: Qt.rgba(1, 1, 1, 0.50)
            font.family: Theme.bodyFont
            font.pixelSize: 11
            font.weight: Font.Bold
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "›"
            color: Qt.rgba(1, 1, 1, 0.50)
            font.family: Theme.bodyFont
            font.pixelSize: 15
            font.weight: Font.Bold
        }

        TapHandler { onTapped: root.seeAllRequested() }
    }

    Row {
        y: 31
        width: parent.width
        spacing: root.railGap

        Repeater {
            model: Math.min(root.games.length, root.tileCount)

            DesktopStoreCard {
                required property int index
                readonly property var itemGame: root.games[index]

                tileWidth: root.tileWidth
                tileHeight: root.tileHeight
                game: itemGame
                selected: root.active && root.selectedIndex === index
                price: String(itemGame && itemGame.storePrice || qsTr("Available"))
                discount: String(itemGame && itemGame.storeDiscount || "")
                owned: Boolean(itemGame && itemGame.storeOwned)
                freeToPlay: Boolean(itemGame && itemGame.storeFree)
                fallbackColor: root.fallbackFor(itemGame, index)
                onPointed: root.gamePointed(index)
                onActivated: game => root.gameActivated(game)
            }
        }
    }

    function fallbackFor(game, index) {
        const stores = game && game.availableStores || []
        const name = stores.length ? String(stores[0]).toUpperCase() : ""
        if (name.indexOf("EPIC") >= 0) return Theme.cartEpic
        if (name.indexOf("UBISOFT") >= 0) return Theme.cartUbisoft
        if (name.indexOf("XBOX") >= 0) return Theme.cartXbox
        if (name.indexOf("GOG") >= 0) return Theme.cartGog
        if (name.indexOf("BATTLE") >= 0) return Theme.cartBattlenet
        return index % 2 ? Theme.cartSteam : Theme.glassStrong
    }
}
