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
    property bool materialized: true
    property string categoryId: ""
    property int totalCount: 0
    property var localGames: []
    property string requestId: ""
    property string loadError: ""
    property int loadedLimit: 0
    property int requestedLimit: 0
    readonly property var displayGames: categoryId ? localGames : games
    function cancelRequest() {
        loadDelay.stop()
        const pending = requestId
        requestId = ""
        if (pending) CoreClient.cancel(pending)
    }
    function loadVisible() {
        loadDelay.restart()
    }
    Timer { id: loadDelay; interval: 16; onTriggered: root.requestVisible() }
    function requestVisible() {
        if (!materialized || !categoryId || requestId || loadedLimit >= tileCount || !ShellStore.ready) return
        const cached = ShellStore.cachedStoreShelf(categoryId, tileCount)
        if (cached !== null) {
            localGames = cached
            loadedLimit = tileCount
            loadError = ""
            return
        }
        loadError = ""
        requestedLimit = tileCount
        requestId = CoreClient.request("catalog.store.local", {categoryId:categoryId,searchQuery:"",cursor:"",limit:tileCount}, 30000)
    }
    onMaterializedChanged: {
        if (materialized) loadVisible()
        else { cancelRequest(); localGames = []; loadedLimit = 0 }
    }
    onTileCountChanged: if (categoryId && materialized && tileCount > loadedLimit) { cancelRequest(); loadVisible() }
    function invalidate() { cancelRequest(); localGames = []; loadedLimit = 0; loadVisible() }
    onCategoryIdChanged: invalidate()
    Component.onCompleted: loadVisible()
    Component.onDestruction: cancelRequest()
    Connections {
        target: ShellStore
        function onStoreShelfEpochChanged() { root.invalidate() }
    }
    Connections {
        target: CoreClient
        function onResponseReceived(id,result) {
            if (!root.requestId || id !== root.requestId) return
            root.requestId = ""
            root.localGames = (result.games || []).slice(0, root.requestedLimit)
            root.loadedLimit = root.requestedLimit
            ShellStore.cacheStoreShelf(root.categoryId, root.loadedLimit, root.localGames)
        }
        function onRequestFailed(id,code,message) {
            if (!root.requestId || id !== root.requestId) return
            root.requestId = ""
            root.loadError = message
        }
    }

    signal gameActivated(var game)
    signal gamePointed(int index)
    signal seeAllRequested()

    readonly property int railGap: DesktopTokens.px(14)
    readonly property int columnCount: Math.max(1, Math.floor((width + railGap) / (DesktopTokens.libraryArtWidth + railGap)))
    readonly property int tileCount: Math.max(1, Math.min(60, (root.categoryId ? root.totalCount : root.games.length) || 1, columnCount))
    // A short final page keeps the same poster size as a full row.
    readonly property int tileWidth: Math.max(DesktopTokens.libraryArtWidth, Math.floor((width - railGap * (columnCount - 1)) / columnCount))
    readonly property int tileHeight: Math.round(tileWidth * 198 / 132) + DesktopTokens.storeCardInfoHeight

    height: DesktopTokens.px(31) + tileHeight

    Row {
        x: 0
        y: 0
        width: Math.max(0, headerActions.visible ? headerActions.x - DesktopTokens.px(12) : parent.width)
        height: DesktopTokens.px(20)
        spacing: DesktopTokens.px(9)

        Text {
            width: Math.max(0, parent.width - (sectionEyebrow.visible ? sectionEyebrow.implicitWidth + parent.spacing : 0))
            elide: Text.ElideRight
            anchors.baseline: sectionEyebrow.baseline
            text: root.title
            color: DesktopTokens.text
            font.family: Theme.displayFont
            font.pixelSize: DesktopTokens.px(16)
            font.weight: Font.Black
            font.letterSpacing: -0.16
        }

        Text {
            id: sectionEyebrow
            visible: root.eyebrow.length > 0
            text: root.eyebrow
            color: DesktopTokens.textMuted
            font.family: Theme.monoFont
            font.pixelSize: DesktopTokens.microSize
            font.weight: Font.DemiBold
            font.letterSpacing: 0.4
        }
    }

    Row {
        id: headerActions
        visible: root.showSeeAll
        anchors.right: parent.right
        y: 1
        height: DesktopTokens.px(18)
        spacing: DesktopTokens.px(5)

        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: root.seeAllText
            color: DesktopTokens.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.smallSize
            font.weight: Font.Bold
        }
        Text {
            anchors.verticalCenter: parent.verticalCenter
            text: "›"
            color: DesktopTokens.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.bodySize
            font.weight: Font.Bold
        }

        TapHandler { onTapped: root.seeAllRequested() }
    }

    Row {
        y: DesktopTokens.px(31)
        width: parent.width
        spacing: root.railGap

        Repeater {
            model: root.materialized ? Math.min(root.displayGames.length, root.tileCount) : 0

            DesktopStoreCard {
                required property int index
                readonly property var itemGame: root.displayGames[index]

                tileWidth: root.tileWidth
                tileHeight: root.tileHeight
                game: itemGame
                selected: root.active && root.selectedIndex === index
                price: String(itemGame && itemGame.storePrice || "")
                discount: String(itemGame && itemGame.storeDiscount || "")
                owned: Boolean(itemGame && (itemGame.storeOwned || itemGame.isInLibrary))
                freeToPlay: Boolean(itemGame && itemGame.storeFree)
                fallbackColor: root.fallbackFor(itemGame, index)
                onPointed: root.gamePointed(index)
                onActivated: game => root.gameActivated(game)
            }
        }
    }

    Text {
        x: 0; y: DesktopTokens.px(45); width: parent.width
        visible: root.materialized && root.loadError !== ""
        text: root.loadError; textFormat: Text.PlainText; wrapMode: Text.Wrap
        color: DesktopTokens.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.monoSize
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
