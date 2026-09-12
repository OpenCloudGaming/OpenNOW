pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopLibraryScreen"
    property string searchQuery: ""
    property string activeFilter: "all"
    readonly property var collection: ShellStore.activeCollection
    readonly property var collectionGameIds: new Set(root.collection ? root.collection.gameIds : [])
    readonly property string selectedCollectionId: ShellStore.activeCollectionId
    onSelectedCollectionIdChanged: {
        activeFilter = "all"
        closeContext()
    }
    property var contextGame: null
    property var presentedContextGame: null
    onContextGameChanged: if (contextGame) presentedContextGame = contextGame
    MotionProgress { id: contextMotion; shown: root.contextGame !== null; enterDuration: 120; exitDuration: 120 }
    MotionProgress { id: collectionMotion; shown: root.contextGame !== null && root.collectionOpen; enterDuration: 120; exitDuration: 120 }
    property point contextPoint: Qt.point(0, 0)
    signal detailsRequested(var game)
    signal playRequested(var game)

    function storeBlob(game) {
        return (game.availableStores || []).join(" ").toLocaleLowerCase()
    }
    function hasRtx(game) {
        const blob = ((game.genres || []).join(" ") + " " + String(game.title || "") + " " + String(game.nvidiaTech || "")).toLocaleLowerCase()
        return blob.indexOf("rtx") >= 0 || blob.indexOf("ray tracing") >= 0
    }
    function hasController(game) {
        const controls = (game.supportedControls || []).join(" ").toLocaleLowerCase()
        return controls.indexOf("gamepad") >= 0 || controls.indexOf("controller") >= 0
    }
    function isReady(game) {
        return Boolean(game.isInLibrary || game.isAvailable)
    }
    function favoriteLabel() {
        if (root.contextGame && ShellStore.isFavorite(root.contextGame))
            return qsTr("Remove from favourites")
        return qsTr("Add to favourites")
    }
    function hideLabel() {
        if (root.contextGame && ShellStore.isHidden(root.contextGame))
            return qsTr("Unhide from library")
        return qsTr("Hide from library")
    }
    function countHidden() {
        return root.countWhere(root.isHiddenGame)
    }
    function isHiddenGame(game) {
        return ShellStore.isHidden(game)
    }
    function countStore(name) {
        return root.countWhere(function(game) { return root.storeBlob(game).indexOf(name) >= 0 })
    }
    function countWhere(predicate) {
        const source = ShellStore.catalogGames || []
        let count = 0
        for (let i = 0; i < source.length; ++i) {
            if (root.collection && !root.collectionGameIds.has(ShellStore.gameIdentity(source[i])))
                continue
            if (predicate(source[i]))
                count += 1
        }
        return count
    }
    function filteredGames() {
        const query = searchQuery.trim().toLocaleLowerCase()
        const source = ShellStore.catalogGames || []
        const result = []
        for (let index = 0; index < source.length; ++index) {
            const game = source[index]
            if (root.collection && !root.collectionGameIds.has(ShellStore.gameIdentity(game)))
                continue
            if (query !== "" && String(game.title || "").toLocaleLowerCase().indexOf(query) < 0)
                continue
            const hidden = root.isHiddenGame(game)
            if (activeFilter === "hidden") {
                if (!hidden)
                    continue
            } else if (hidden) {
                continue
            }
            const stores = root.storeBlob(game)
            if (activeFilter === "ready" && !root.isReady(game))
                continue
            if (activeFilter === "rtx" && !root.hasRtx(game))
                continue
            if (activeFilter === "controller" && !root.hasController(game))
                continue
            if (activeFilter === "steam" && stores.indexOf("steam") < 0)
                continue
            if (activeFilter === "epic" && stores.indexOf("epic") < 0)
                continue
            if (activeFilter === "gog" && stores.indexOf("gog") < 0)
                continue
            result.push(game)
        }
        return result
    }
    readonly property var games: filteredGames()
    readonly property real tileScale: Math.max(0.75, Math.min(1.5, Number(ShellStore.settings.posterSizeScale || 1.05))) / 1.05
    readonly property int libraryColumns: Math.max(1, Math.floor((grid.width + 10) / (156 * tileScale)))
    readonly property int libraryCellW: Math.max(1, Math.floor(grid.width / libraryColumns))
    readonly property int libraryCellH: Math.round(libraryCellW * 214 / 146)

    function editCollection(mode, game) {
        collectionDialog.mode = mode
        collectionDialog.collectionId = mode === "create" ? "" : root.collection.id
        collectionDialog.initialName = mode === "create" ? "" : root.collection.name
        collectionDialog.game = game || null
        root.closeContext()
        collectionDialog.open()
    }

    DesktopCollectionDialog {
        id: collectionDialog
        onCollectionOpened: collectionId => ShellStore.activeCollectionId = collectionId
    }

    Row {
        id: collectionToolbar
        x: 24; y: 16
        width: parent.width - 48
        height: 36
        spacing: 8
        Text {
            width: Math.max(80, parent.width - collectionActions.width - parent.spacing)
            height: 36
            text: root.collection ? root.collection.name : qsTr("All games")
            textFormat: Text.PlainText
            elide: Text.ElideRight
            verticalAlignment: Text.AlignVCenter
            color: DesktopTokens.text
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 18
            font.bold: true
        }
        Row {
            id: collectionActions
            spacing: 8
            DesktopButton {
                text: qsTr("All games")
                visible: root.collection !== null
                onClicked: ShellStore.activeCollectionId = ""
            }
            DesktopButton {
                text: qsTr("Rename")
                visible: root.collection !== null
                enabled: !ShellStore.collectionsBusy
                onClicked: root.editCollection("rename", null)
            }
            DesktopButton {
                text: qsTr("Delete")
                visible: root.collection !== null
                enabled: !ShellStore.collectionsBusy
                onClicked: root.editCollection("delete", null)
            }
            DesktopButton {
                objectName: "libraryNewCollectionButton"
                text: qsTr("New collection")
                enabled: !ShellStore.collectionsBusy
                onClicked: root.editCollection("create", null)
            }
        }
    }

    Flow {
        id: filterRow
        x: 24
        y: collectionToolbar.y + collectionToolbar.height + 14
        width: parent.width - 48
        spacing: 8
        Repeater {
            model: [
                {key:"all", label:qsTr("All"), count:root.countWhere(function(game) { return !root.isHiddenGame(game) })},
                {key:"ready", label:qsTr("Ready to play"), count:root.countWhere(root.isReady)},
                {key:"rtx", label:"RTX", count:root.countWhere(root.hasRtx)},
                {key:"controller", label:qsTr("Controller"), count:root.countWhere(root.hasController)},
                {key:"steam", label:"Steam", count:root.countStore("steam")},
                {key:"epic", label:"Epic", count:root.countStore("epic")},
                {key:"gog", label:"GOG", count:root.countStore("gog")},
                {key:"hidden", label:qsTr("Hidden"), count:root.countHidden()}
            ]
            delegate: Button {
                id: filterButton
                required property var modelData
                visible: modelData.key !== "hidden" || root.countHidden() > 0
                height: 40
                implicitHeight: 40
                implicitWidth: Math.max(84, chipRow.implicitWidth + 28)
                padding: 0
                leftPadding: 0
                rightPadding: 0
                topPadding: 0
                bottomPadding: 0
                focusPolicy: Qt.StrongFocus
                hoverEnabled: true
                clip: false
                background: Rectangle {
                    radius: 11
                    color: root.activeFilter === filterButton.modelData.key ? "#1AFFFFFF" : (filterButton.hovered ? "#0FFFFFFF" : "transparent")
                    border.width: 1
                    border.color: root.activeFilter === filterButton.modelData.key ? "#3DFFFFFF" : DesktopTokens.seam
                }
                contentItem: Item {
                    implicitWidth: chipRow.implicitWidth
                    implicitHeight: 40
                    Row {
                        id: chipRow
                        anchors.centerIn: parent
                        spacing: 8
                        Image {
                            visible: ["steam","epic","gog"].indexOf(filterButton.modelData.key) >= 0
                            width: 16
                            height: 16
                            anchors.verticalCenter: parent.verticalCenter
                            source: visible ? DesktopTokens.storeIconUrl(filterButton.modelData.key) : ""
                            sourceSize: Qt.size(32, 32)
                            fillMode: Image.PreserveAspectFit
                        }
                        Text {
                            text: filterButton.modelData.label
                            anchors.verticalCenter: parent.verticalCenter
                            color: root.activeFilter === filterButton.modelData.key ? DesktopTokens.text : DesktopTokens.textMuted
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 14
                            font.weight: Font.Bold
                            verticalAlignment: Text.AlignVCenter
                        }
                        Text {
                            text: filterButton.modelData.count
                            anchors.verticalCenter: parent.verticalCenter
                            color: DesktopTokens.textFaint
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: 12
                            font.weight: Font.DemiBold
                            verticalAlignment: Text.AlignVCenter
                        }
                    }
                }
                onClicked: root.activeFilter = modelData.key
            }
        }
    }
    Text {
        id: libraryHint
        anchors.right: parent.right
        anchors.rightMargin: 24
        anchors.verticalCenter: filterRow.verticalCenter
        visible: filterRow.height <= 40 && root.width - 24 - libraryHint.implicitWidth > 40 + filterRow.childrenRect.width
        text: qsTr("RIGHT-CLICK A GAME FOR ACTIONS")
        color: DesktopTokens.textFaint
        font.family: DesktopTokens.monoFont
        font.pixelSize: 10
        font.weight: Font.DemiBold
        font.letterSpacing: 0.7
    }

    GridView {
        id: grid
        // The delegate keeps a six-pixel focus/scale gutter. Offset the view by
        // that gutter so the artwork remains on Paper's 24/64 alignment lane.
        x: 18
        y: filterRow.y + filterRow.height + 14
        width: parent.width - 36
        height: parent.height - y
        clip: true
        cellWidth: root.libraryCellW
        cellHeight: root.libraryCellH
        model: root.games
        focus: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
        delegate: DesktopPoster {
            required property var modelData
            game: modelData
            tileWidth: root.libraryCellW
            tileHeight: root.libraryCellH
            onClicked: {
                ShellStore.selectedGame = modelData
                root.detailsRequested(modelData)
            }
            onDoubleClicked: {
                ShellStore.selectedGame = modelData
                root.playRequested(modelData)
            }
            onContextRequested: (sceneX, sceneY) => {
                root.contextGame = modelData
                const local = root.mapFromItem(null, sceneX, sceneY)
                root.contextPoint = Qt.point(Math.min(root.width - 470, Math.max(16, local.x)), Math.min(root.height - 300, Math.max(16, local.y)))
                root.collectionOpen = false
            }
        }
    }

    Column {
        anchors.centerIn: grid
        width: Math.min(grid.width - 48, 460)
        spacing: 12
        visible: root.games.length === 0
        Text {
            width: parent.width
            text: root.collection ? qsTr("No games in this view") : qsTr("No games found")
            color: DesktopTokens.text
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 22
            font.bold: true
            horizontalAlignment: Text.AlignHCenter
        }
        Text {
            width: parent.width
            text: root.collection
                ? qsTr("Open All games, right-click a game, and choose Add to collection. Games can belong to more than one collection.")
                : qsTr("Try a different search or filter.")
            color: DesktopTokens.textMuted
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 14
            wrapMode: Text.WordWrap
            horizontalAlignment: Text.AlignHCenter
        }
    }

    MouseArea {
        anchors.fill: parent; z: 40
        visible: root.contextGame !== null
        acceptedButtons: Qt.LeftButton | Qt.RightButton
        onClicked: { root.contextGame = null; root.collectionOpen = false }
    }
    property bool collectionOpen: false
    function closeContext() {
        root.contextGame = null
        root.collectionOpen = false
    }
    function activateContext(action) {
        const game = root.contextGame
        if (action === "favorite") {
            ShellStore.toggleFavorite(game)
            return
        }
        if (action === "hide") {
            ShellStore.toggleHidden(game)
            root.closeContext()
            return
        }
        root.closeContext()
        if (action === "play") { ShellStore.selectedGame = game; root.playRequested(game) }
        else if (action === "details") { ShellStore.selectedGame = game; root.detailsRequested(game) }
        else if (action === "settings") AppController.navigate("settings-streaming")
    }
    Rectangle {
        x: root.contextPoint.x; y: root.contextPoint.y
        width: 230; height: 286; radius: 12
        visible: contextMotion.present
        enabled: root.contextGame !== null
        z: 41
        color: DesktopTokens.shell
        border.width: 1; border.color: DesktopTokens.seam
        Column {
            x: 8; y: 8; width: 214; spacing: 0
            Text { width: parent.width; height: 26; leftPadding: 8; text: root.presentedContextGame ? String(root.presentedContextGame.title || "").toUpperCase() : ""; color: DesktopTokens.textFaint; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.DemiBold; font.letterSpacing: 0.6 }
            Rectangle {
                id: playRow
                width: parent.width; height: 36; radius: 9
                color: playHover.hovered ? "#FFFFFF" : "#F2FFFFFF"
                Text { x: 12; anchors.verticalCenter: parent.verticalCenter; text: "▶  " + qsTr("Play"); color: "#0B0F1A"; font.family: DesktopTokens.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Black }
                KeyboardGlyph { anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; shortcut: "Enter"; keySize: 20; ink: "#0B0F1A"; Accessible.name: qsTr("Enter") }
                HoverHandler { id: playHover; cursorShape: Qt.PointingHandCursor }
                TapHandler { onTapped: root.activateContext("play") }
            }
            Item { width: parent.width; height: 6 }
            Repeater {
                model: [
                    {label:qsTr("Details"), key:"Space", action:"details"},
                    {label:root.favoriteLabel(), key:"F", action:"favorite"},
                    {label:qsTr("Add to collection"), key:"›", action:"collection"},
                    {label:qsTr("Stream settings…"), key:"Ctrl ,", action:"settings"},
                    {label:root.hideLabel(), key:"", action:"hide"}
                ]
                delegate: ItemDelegate {
                    required property var modelData
                    width: 214; height: 32; padding: 8
                    highlighted: modelData.action === "collection" && root.collectionOpen
                    background: Rectangle { radius: 7; color: parent.hovered || parent.activeFocus || (modelData.action === "collection" && root.collectionOpen) ? "#14FFFFFF" : "transparent" }
                    contentItem: Item {
                        Text { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; text: modelData.label; color: DesktopTokens.textBody; font.family: DesktopTokens.bodyFont; font.pixelSize: DesktopTokens.captionSize }
                        KeyboardGlyph { visible: modelData.action !== "collection"; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; shortcut: modelData.key; keySize: 18; ink: DesktopTokens.textMuted }
                        Text { visible: modelData.action === "collection"; anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: modelData.key; color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.tinySize }
                    }
                    onClicked: {
                        if (modelData.action === "collection")
                            root.collectionOpen = !root.collectionOpen
                        else
                            root.activateContext(modelData.action)
                    }
                }
            }
        }
        opacity: contextMotion.progress
        scale: contextMotion.zoom
        transformOrigin: Item.TopLeft
    }
    Rectangle {
        x: Math.max(8, Math.min(root.width - width - 8, root.contextPoint.x + 238))
        y: Math.max(8, Math.min(root.height - height - 8, root.contextPoint.y + 104))
        width: 240
        height: Math.min(root.height - 16, 96 + Math.min(6, ShellStore.gameCollections.length) * 36 + (ShellStore.collectionError ? 52 : 0))
        radius: 12
        visible: collectionMotion.present
        enabled: root.contextGame !== null && root.collectionOpen
        z: 42
        color: DesktopTokens.shell
        border.width: 1; border.color: DesktopTokens.seam
        Column {
            x: 8; y: 8; width: parent.width - 16; spacing: 0
            Text { width: parent.width; height: 24; leftPadding: 8; text: qsTr("COLLECTIONS"); color: DesktopTokens.textFaint; verticalAlignment: Text.AlignVCenter; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.DemiBold; font.letterSpacing: 0.6 }
            ListView {
                width: parent.width
                height: Math.min(6, count) * 36
                clip: true
                model: ShellStore.gameCollections
                boundsBehavior: Flickable.StopAtBounds
                ScrollBar.vertical: ScrollBar { }
                delegate: ItemDelegate {
                    id: membershipButton
                    required property var modelData
                    width: ListView.view.width
                    height: 36
                    padding: 8
                    enabled: !ShellStore.collectionsBusy
                    Accessible.name: modelData.name
                    background: Rectangle { radius: 7; color: membershipButton.hovered || membershipButton.activeFocus ? DesktopTokens.raised : "transparent" }
                    contentItem: Text {
                        text: (ShellStore.isInCollection(root.contextGame, membershipButton.modelData.id) ? "✓  " : "+  ") + membershipButton.modelData.name
                        textFormat: Text.PlainText
                        elide: Text.ElideRight
                        verticalAlignment: Text.AlignVCenter
                        color: DesktopTokens.textBody
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: DesktopTokens.captionSize
                    }
                    onClicked: ShellStore.toggleCollectionGame(modelData.id, root.contextGame)
                }
            }
            ItemDelegate {
                id: newCollectionAction
                width: parent.width; height: 40; padding: 8
                enabled: !ShellStore.collectionsBusy
                background: Rectangle { radius: 7; color: newCollectionAction.hovered || newCollectionAction.activeFocus ? DesktopTokens.raised : "transparent" }
                contentItem: Text {
                    text: qsTr("New collection")
                    verticalAlignment: Text.AlignVCenter
                    color: DesktopTokens.textBody
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: DesktopTokens.captionSize
                }
                onClicked: root.editCollection("create", root.contextGame)
            }
            Text {
                width: parent.width
                height: visible ? 52 : 0
                visible: ShellStore.collectionError !== ""
                text: ShellStore.collectionError
                textFormat: Text.PlainText
                wrapMode: Text.WordWrap
                color: DesktopTokens.textMuted
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 12
                }
        }
        opacity: collectionMotion.progress
        scale: collectionMotion.zoom
        transformOrigin: Item.TopLeft
    }
}
