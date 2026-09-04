pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property string searchQuery: ""
    property string activeFilter: "all"
    property var contextGame: null
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
    readonly property int libraryColumns: Math.max(6, Math.floor((grid.width + 10) / 156))
    readonly property int libraryCellW: Math.max(146, Math.floor(grid.width / libraryColumns))
    readonly property int libraryCellH: Math.round(libraryCellW * 214 / 146)

    Row {
        id: filterRow
        x: 24
        y: 16
        height: 40
        spacing: 8
        Repeater {
            model: [
                {key:"all", label:qsTr("All"), count:ShellStore.catalogTotalCount || root.games.length},
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
                        Rectangle {
                            visible: ["steam","epic","gog"].indexOf(filterButton.modelData.key) >= 0
                            width: 8
                            height: 8
                            radius: filterButton.modelData.key === "gog" ? 2 : 4
                            anchors.verticalCenter: parent.verticalCenter
                            color: filterButton.modelData.key === "gog" ? "#8D52FF" : "transparent"
                            border.width: filterButton.modelData.key === "gog" ? 0 : 1
                            border.color: "#52FFFFFF"
                        }
                        Text {
                            text: filterButton.modelData.label
                            color: root.activeFilter === filterButton.modelData.key ? DesktopTokens.text : DesktopTokens.textMuted
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 14
                            font.weight: Font.Bold
                            verticalAlignment: Text.AlignVCenter
                        }
                        Text {
                            text: filterButton.modelData.count
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
        visible: root.width - 24 - libraryHint.implicitWidth > 40 + filterRow.width
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
        y: 70
        width: parent.width - 36
        height: parent.height - 70
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
        visible: root.contextGame !== null
        z: 41
        color: "#F710131D"
        border.width: 1; border.color: "#29FFFFFF"
        Column {
            x: 8; y: 8; width: 214; spacing: 0
            Text { width: parent.width; height: 26; leftPadding: 8; text: root.contextGame ? String(root.contextGame.title || "").toUpperCase() : ""; color: DesktopTokens.textFaint; elide: Text.ElideRight; verticalAlignment: Text.AlignVCenter; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.DemiBold; font.letterSpacing: 0.6 }
            Rectangle {
                id: playRow
                width: parent.width; height: 36; radius: 9
                color: playHover.hovered ? "#FFFFFF" : "#F2FFFFFF"
                Text { x: 12; anchors.verticalCenter: parent.verticalCenter; text: "▶  " + qsTr("Play"); color: "#0B0F1A"; font.family: DesktopTokens.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Black }
                Text { anchors.right: parent.right; anchors.rightMargin: 12; anchors.verticalCenter: parent.verticalCenter; text: qsTr("Enter"); color: "#5A0B0F1A"; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.Bold }
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
                        Text { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: modelData.key; color: DesktopTokens.textFaint; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.tinySize }
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
        opacity: visible ? 1 : 0
        scale: visible ? 1 : 0.96
        Behavior on opacity { NumberAnimation { duration: DesktopTokens.quickDuration } }
        Behavior on scale { NumberAnimation { duration: DesktopTokens.quickDuration; easing.type: Easing.OutCubic } }
    }
    Rectangle {
        x: root.contextPoint.x + 238; y: root.contextPoint.y + 104
        width: 212; height: 76; radius: 12
        visible: root.contextGame !== null && root.collectionOpen
        z: 42
        color: "#F710131D"
        border.width: 1; border.color: "#29FFFFFF"
        Column {
            x: 8; y: 8; width: 196; spacing: 0
            Text { width: parent.width; height: 24; leftPadding: 8; text: qsTr("COLLECTIONS"); color: DesktopTokens.textFaint; verticalAlignment: Text.AlignVCenter; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.tinySize; font.weight: Font.DemiBold; font.letterSpacing: 0.6 }
            ItemDelegate {
                width: 196; height: 32; padding: 8
                background: Rectangle { radius: 7; color: parent.hovered || parent.activeFocus ? "#14FFFFFF" : "transparent" }
                contentItem: Item {
                    Text { anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter; text: qsTr("★  Favourites"); color: DesktopTokens.textBody; font.family: DesktopTokens.bodyFont; font.pixelSize: DesktopTokens.captionSize }
                    Text { anchors.right: parent.right; anchors.verticalCenter: parent.verticalCenter; text: root.contextGame && ShellStore.isFavorite(root.contextGame) ? "✓" : "+"; color: DesktopTokens.focus; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Bold }
                }
                onClicked: root.activateContext("favorite")
            }
        }
        opacity: visible ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: DesktopTokens.quickDuration } }
    }
}
