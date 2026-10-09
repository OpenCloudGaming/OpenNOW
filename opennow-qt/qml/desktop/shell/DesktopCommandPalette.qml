import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property bool opened: false
    visible: reveal.present
    enabled: opened
    MotionProgress { id: reveal; shown: root.opened }
    property string query: ""
    property string scopeFilter: "all"
    property int currentIndex: 0
    property string searchRequestId: ""
    property var remoteGames: []
    property string searchError: ""
    property string searchState: "idle"
    readonly property bool searching: searchState === "loading"
    property bool searchHasMore: false
    property int queryRevision: 0
    property var searchIntent: null
    readonly property string normalizedQuery: query.trim().replace(/\s+/g, " ")
    readonly property bool gfnGames: !ShellStore.browsingExternalSource
    readonly property bool gamesQuery: gfnGames && normalizedQuery !== "" && scopeFilter !== "actions"
    readonly property string searchContext: JSON.stringify([ShellStore.catalogOwnerState.authScope,
        ShellStore.catalogOwnerState.catalogContext, ShellStore.catalogOwnerState.catalogRevision])
    readonly property string searchStatus: {
        if (!gamesQuery) return ""
        if (!ShellStore.signedIn) return qsTr("Sign in to search games.")
        if (!ShellStore.ready) return qsTr("Game search is unavailable while OpenNOW reconnects.")
        if (searchState === "waiting") return qsTr("Waiting to search… Press Enter to search now.")
        if (searching) return qsTr("Searching GeForce NOW…")
        if (searchState === "error") return searchError
        if (searchState !== "ready") return ""
        if (remoteGames.length > 0 && gameList.length === 0)
            return searchHasMore ? qsTr("This result page contains only hidden games. More matches are available; refine your search.")
                : qsTr("All matching games are hidden.")
        if (searchHasMore) return qsTr("More matches are available. Refine your search to narrow the results.")
        return gameList.length === 0 ? qsTr("No games match “%1”.").arg(normalizedQuery) : ""
    }
    function cancelSearch() {
        searchDelay.stop()
        const id = searchRequestId
        searchRequestId = ""
        searchIntent = null
        searchState = "idle"
        if (id !== "") CoreClient.cancel(id)
    }
    function requestGames() {
        if (searchRequestId !== "" || !opened || !gamesQuery || !ShellStore.ready || !ShellStore.signedIn) return
        searchDelay.stop()
        searchError = ""
        searchState = "loading"
        searchIntent = {revision:queryRevision, query:normalizedQuery, context:searchContext}
        searchRequestId = CoreClient.request("catalog.store.list", {searchQuery:normalizedQuery,limit:6,cursor:"",revalidate:true}, 30000)
        if (!searchRequestId) {
            searchIntent = null
            searchState = "error"
            searchError = qsTr("Game search could not start. Press Enter to retry.")
        }
    }
    function scheduleSearch() {
        queryRevision++
        cancelSearch()
        remoteGames = []
        searchError = ""
        searchHasMore = false
        currentIndex = 0
        if (opened && gfnGames && query.trim() !== "" && scopeFilter !== "actions" && ShellStore.ready && ShellStore.signedIn) {
            searchState = "waiting"
            searchDelay.start()
        }
    }
    function acceptsSearch(id) {
        return id !== "" && id === searchRequestId && searchIntent !== null
            && searchIntent.revision === queryRevision && searchIntent.query === normalizedQuery
            && searchIntent.context === searchContext && opened && gamesQuery
            && ShellStore.ready && ShellStore.signedIn
    }
    function acceptCurrent() {
        if (gamesQuery && ShellStore.ready && ShellStore.signedIn && searchState !== "ready") {
            requestGames()
            return
        }
        activateAt(currentIndex)
    }
    NativeTimer {
        id: searchDelay
        objectName: "commandSearchDelay"
        interval: 2000
        singleShot: true
        timerType: Qt.PreciseTimer
        onTimeout: root.requestGames()
    }
    Connections {
        target: CoreClient
        function onResponseReceived(id, result) {
            if (!root.acceptsSearch(id)) return
            root.searchRequestId = ""
            root.searchIntent = null
            if (!result.scope || !ShellStore.matchesAuthScope(result.scope)
                    || !Number.isSafeInteger(result.catalogRevision) || result.catalogRevision < 0
                    || (ShellStore.catalogOwnerState.catalogRevision !== null
                        && result.catalogRevision < ShellStore.catalogOwnerState.catalogRevision)) {
                root.remoteGames = []
                root.searchHasMore = false
                root.searchState = "error"
                root.searchError = qsTr("Game search returned an invalid or outdated response. Press Enter to retry.")
                return
            }
            const selectedActionIndex = root.currentIndex - root.gameList.length
            const preserveAction = selectedActionIndex >= 0 && selectedActionIndex < root.actionList.length
            root.remoteGames = result.games || []
            if (preserveAction)
                root.currentIndex = root.gameList.length + selectedActionIndex
            root.searchHasMore = result.hasNextPage === true
            root.searchState = "ready"
            root.scheduleCurrentVisibility()
        }
        function onRequestFailed(id, code, message) {
            if (!root.acceptsSearch(id)) return
            root.searchRequestId = ""
            root.searchIntent = null
            root.searchState = "error"
            root.searchError = message || qsTr("Game search failed. Press Enter to retry.")
        }
    }
    Connections {
        target: ShellStore
        function onReadyChanged() { root.scheduleSearch() }
        function onSignedInChanged() { root.scheduleSearch() }
        function onStoreSessionReset() { root.scheduleSearch() }
    }
    Connections {
        target: ShellStore.catalogOwnerState
        function onRequestContextKeyChanged() { root.scheduleSearch() }
    }
    onSearchContextChanged: root.scheduleSearch()
    onGfnGamesChanged: root.scheduleSearch()
    Component.onDestruction: root.cancelSearch()
    signal closeRequested()
    signal routeRequested(string route)
    signal gameRequested(var game)
    anchors.fill: parent
    onOpenedChanged: {
        if (opened) {
            root.query = ""
            root.scopeFilter = "all"
            root.currentIndex = 0
            field.text = ""
            field.forceActiveFocus()
            root.scheduleCurrentVisibility()
        } else root.scheduleSearch()
    }

    readonly property var actions: [
        { icon: "desktop-nav-home.svg", name: qsTr("Go to Home"), detail: qsTr("Open your desktop"), route: "home", key: "1" },
        { icon: "desktop-nav-library.svg", name: qsTr("Open Library"), detail: qsTr("Browse all games"), route: "library", key: "2" },
        { icon: "desktop-nav-store.svg", name: qsTr("Open Store"), detail: qsTr("Discover games"), route: "store", key: "3" },
        { icon: "desktop-nav-friends.svg", name: qsTr("Friends and party"), detail: qsTr("See who is online"), route: "friends", key: "4" },
        { icon: "desktop-nav-settings.svg", name: qsTr("Settings"), detail: qsTr("Configure OpenNOW"), route: "settings", key: "," },
        { icon: "desktop-sliders.svg", name: qsTr("Stream settings"), detail: qsTr("Resolution, frame rate and codec"), route: "settings-streaming", key: "" },
        { icon: "desktop-play-stroke.svg", name: qsTr("Start last game"), detail: qsTr("Resume your previous session"), route: "game-detail", key: "Enter" }
    ]

    function matchedGames() {
        if (root.normalizedQuery !== "")
            return root.remoteGames.filter(game => !ShellStore.isHidden(game))
        const source = ShellStore.catalogGames || []
        const result = []
        for (let index = 0; index < source.length && result.length < 6; ++index) {
            const game = source[index]
            if (!ShellStore.isHidden(game))
                result.push(game)
        }
        return result
    }

    function gameSubtitle(game) {
        if (!game)
            return ""
        if (game.lastPlayed)
            return qsTr("Resume · in your library")
        if (game.isInLibrary)
            return qsTr("In your library")
        const stores = game.availableStores || []
        if (stores.length > 0)
            return stores.join(" · ")
        return qsTr("Available on GeForce NOW")
    }

    function matchedActions() {
        const q = root.query.trim().toLocaleLowerCase()
        return root.actions.filter(item =>
            q === "" || String(item.name + " " + item.detail).toLocaleLowerCase().indexOf(q) >= 0)
    }

    readonly property var gameList: !root.gfnGames || root.scopeFilter === "actions" ? [] : root.matchedGames()
    readonly property var actionList: root.scopeFilter === "games" ? [] : root.matchedActions()
    readonly property int flatCount: gameList.length + actionList.length
    readonly property int resultCount: flatCount

    function clampCurrent() {
        if (root.currentIndex >= root.flatCount)
            root.currentIndex = Math.max(0, root.flatCount - 1)
        if (root.currentIndex < 0)
            root.currentIndex = 0
    }

    function moveCurrent(delta) {
        if (root.flatCount === 0)
            return
        root.currentIndex = (root.currentIndex + delta + root.flatCount) % root.flatCount
        root.ensureCurrentVisible()
    }

    function currentRow() {
        return currentIndex < gameList.length ? gameRows.itemAt(currentIndex)
            : actionRows.itemAt(currentIndex - gameList.length)
    }

    function ensureCurrentVisible() {
        if (!opened || !results || !resultsColumn) return
        resultsColumn.forceLayout()
        const row = root.currentRow()
        if (!row) { results.contentY = 0; return }
        let position = results.contentY
        if (row.y < position) position = row.y
        else if (row.y + row.height > position + results.height)
            position = row.y + row.height - results.height
        results.contentY = Math.max(0, Math.min(position, results.contentHeight - results.height))
    }
    // An owned timer is cancelled with the palette when a surface switch
    // destroys it; a queued JavaScript callback can outlive its methods.
    Timer { id: visibilityTimer; interval: 0; onTriggered: root.ensureCurrentVisible() }
    function scheduleCurrentVisibility() {
        if (opened && visibilityTimer) visibilityTimer.restart()
    }
    onHeightChanged: root.scheduleCurrentVisibility()
    onWidthChanged: root.scheduleCurrentVisibility()

    function cycleScope() {
        root.scopeFilter = root.scopeFilter === "all" ? (root.gfnGames ? "games" : "actions")
            : root.scopeFilter === "games" ? "actions" : "all"
        root.currentIndex = 0
    }

    function activateAt(index) {
        if (index < 0 || index >= root.flatCount)
            return
        if (index < root.gameList.length) {
            const game = root.gameList[index]
            root.gameRequested(game)
            root.closeRequested()
            return
        }
        const action = root.actionList[index - root.gameList.length]
        root.routeRequested(action.route)
        root.closeRequested()
    }

    onQueryChanged: root.scheduleSearch()
    onScopeFilterChanged: { root.currentIndex = 0; root.scheduleSearch() }
    onFlatCountChanged: { root.clampCurrent(); root.scheduleCurrentVisibility() }

    readonly property real contentHeight: resultsColumn.implicitHeight
    readonly property real panelHeight: Math.min(58 + Math.min(root.contentHeight, 428) + 42, Math.max(100, height - 32))
    readonly property real panelWidth: Math.max(0, Math.min(640, width - 32))
    readonly property real panelTop: Math.min(120, Math.max(16, (height - panelHeight) / 2))

    Rectangle { anchors.fill: parent; color: "#A8000000"; opacity: reveal.progress; TapHandler { onTapped: root.closeRequested() } }
    Rectangle {
        objectName: "commandPalettePanel"
        opacity: reveal.progress; scale: reveal.zoom
        transformOrigin: Item.Center
        x: Math.round((parent.width - root.panelWidth) / 2)
        y: root.panelTop
        width: root.panelWidth
        height: root.panelHeight
        radius: 18
        color: DesktopTokens.shell
        border.width: 1
        border.color: DesktopTokens.seam
        TapHandler { }

        Item {
            x: 0; y: 0; width: parent.width; height: 58
            DesktopGlyph { x: 18; anchors.verticalCenter: parent.verticalCenter; width: 17; height: 17; icon: "desktop-search.svg" }
            TextField {
                id: field
                objectName: "commandSearchField"
                x: 47; y: 12; width: parent.width - 47 - 90; height: 34
                leftPadding: 0; rightPadding: 0
                placeholderText: qsTr("Search games, commands and settings…")
                placeholderTextColor: DesktopTokens.textMuted
                color: DesktopTokens.text
                font.family: DesktopTokens.bodyFont
                font.pixelSize: DesktopTokens.headingSize
                font.weight: Font.DemiBold
                background: Item {}
                onTextChanged: root.query = text
                onAccepted: root.acceptCurrent()
                Keys.onTabPressed: event => { root.cycleScope(); event.accepted = true }
            }
            KeyboardGlyph {
                anchors.right: parent.right; anchors.rightMargin: 18; anchors.verticalCenter: parent.verticalCenter
                shortcut: "Ctrl K"; keySize: 22; ink: DesktopTokens.textMuted
                Accessible.name: qsTr("Ctrl K")
            }
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: "#14FFFFFF" }
        }

        Flickable {
            id: results
            objectName: "commandPaletteResults"
            x: 10; y: 58; width: parent.width - 20; height: root.panelHeight - 58 - 42
            contentWidth: width
            contentHeight: resultsColumn.implicitHeight
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            Column {
                id: resultsColumn
                width: parent.width
                spacing: 2
                Item {
                    visible: statusText.text !== ""
                    width: parent.width
                    height: Math.max(56, statusText.implicitHeight + 16)
                    Text {
                        id: statusText
                        objectName: "commandSearchStatus"
                        x: 8; width: parent.width - 16
                        anchors.verticalCenter: parent.verticalCenter
                        text: root.searchStatus
                        textFormat: Text.PlainText
                        wrapMode: Text.Wrap
                        color: DesktopTokens.textMuted
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: DesktopTokens.captionSize
                        Accessible.role: Accessible.StaticText
                        Accessible.name: text
                    }
                }
                Text {
                    visible: root.gameList.length > 0
                    height: 26; leftPadding: 8
                    text: qsTr("GAMES")
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: DesktopTokens.tinySize
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1
                    verticalAlignment: Text.AlignVCenter
                }
                Repeater {
                    id: gameRows
                    model: root.gameList
                    delegate: ItemDelegate {
                        id: gameRow
                        required property var modelData
                        required property int index
                        readonly property bool current: index === root.currentIndex
                        width: resultsColumn.width
                        height: 56
                        padding: 0
                        background: Rectangle {
                            radius: 11
                            color: gameRow.current ? DesktopTokens.raisedStrong : (gameRow.hovered ? DesktopTokens.raised : "transparent")
                            border.width: gameRow.current ? 1 : 0
                            border.color: DesktopTokens.seam
                        }
                        contentItem: Item {
                            RoundedArtwork {
                                x: 8; anchors.verticalCenter: parent.verticalCenter
                                width: 40; height: 40
                                artwork: gameRow.modelData.imageUrl || gameRow.modelData.heroImageUrl || ""
                                cornerRadius: 8
                                fallbackColor: Theme.glassStrong
                            }
                            Column {
                                x: 58; width: parent.width - 58 - (gameRow.current ? 130 : 12)
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 2
                                Text {
                                    width: parent.width
                                    text: gameRow.modelData.title || qsTr("Game")
                                    color: DesktopTokens.textHigh
                                    font.family: DesktopTokens.bodyFont
                                    font.pixelSize: DesktopTokens.captionSize
                                    font.weight: Font.Bold
                                    elide: Text.ElideRight
                                }
                                Text {
                                    width: parent.width
                                    text: root.gameSubtitle(gameRow.modelData)
                                    color: DesktopTokens.textMuted
                                    font.family: DesktopTokens.bodyFont
                                    font.pixelSize: DesktopTokens.microSize
                                    elide: Text.ElideRight
                                }
                            }
                            Row {
                                visible: gameRow.current
                                anchors.right: parent.right
                                anchors.rightMargin: 10
                                anchors.verticalCenter: parent.verticalCenter
                                spacing: 8
                                Text {
                                    anchors.verticalCenter: parent.verticalCenter
                                    text: qsTr("PLAY")
                                    color: DesktopTokens.mint
                                    font.family: DesktopTokens.bodyFont
                                    font.pixelSize: DesktopTokens.microSize
                                    font.weight: Font.Black
                                    font.letterSpacing: 0.6
                                }
                                KeyboardGlyph { shortcut: "Enter"; keySize: 20; ink: DesktopTokens.textMuted; Accessible.name: qsTr("Enter") }
                            }
                        }
                        HoverHandler { cursorShape: Qt.PointingHandCursor }
                        onHoveredChanged: if (hovered) root.currentIndex = index
                        onClicked: { root.currentIndex = index; root.activateAt(index) }
                    }
                }
                Text {
                    visible: root.actionList.length > 0
                    height: 26; leftPadding: 8
                    text: qsTr("ACTIONS")
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: DesktopTokens.tinySize
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1
                    verticalAlignment: Text.AlignVCenter
                }
                Repeater {
                    id: actionRows
                    model: root.actionList
                    delegate: ItemDelegate {
                        id: command
                        required property var modelData
                        required property int index
                        readonly property int flatIndex: root.gameList.length + index
                        readonly property bool current: flatIndex === root.currentIndex
                        width: resultsColumn.width
                        height: 40
                        padding: 0
                        background: Rectangle {
                            radius: 11
                            color: command.current ? DesktopTokens.raisedStrong : (command.hovered ? DesktopTokens.raised : "transparent")
                            border.width: command.current ? 1 : 0
                            border.color: DesktopTokens.seam
                        }
                        contentItem: Item {
                            Rectangle {
                                x: 8; anchors.verticalCenter: parent.verticalCenter
                                width: 20; height: 20; radius: 7; color: DesktopTokens.raised
                                DesktopGlyph { anchors.centerIn: parent; width: 13; height: 13; icon: command.modelData.icon }
                            }
                            Text {
                                x: 40; anchors.verticalCenter: parent.verticalCenter
                                text: command.modelData.name
                                color: DesktopTokens.textHigh
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: DesktopTokens.captionSize
                                font.weight: Font.DemiBold
                            }
                            KeyboardGlyph {
                                visible: command.modelData.key !== ""
                                anchors.right: parent.right; anchors.rightMargin: 10; anchors.verticalCenter: parent.verticalCenter
                                shortcut: command.modelData.key; keySize: 20; ink: DesktopTokens.textMuted
                            }
                        }
                        HoverHandler { cursorShape: Qt.PointingHandCursor }
                        onHoveredChanged: if (hovered) root.currentIndex = flatIndex
                        onClicked: { root.currentIndex = flatIndex; root.activateAt(flatIndex) }
                    }
                }
                Text {
                    visible: root.flatCount === 0 && root.searchStatus === ""
                    width: parent.width
                    height: 56
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                    text: qsTr("No matches for “%1”").arg(root.query)
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                    color: DesktopTokens.textMuted
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: DesktopTokens.captionSize
                }
            }
        }

        Rectangle {
            x: 0; y: parent.height - 42; width: parent.width; height: 42
            color: Theme.lightMode ? DesktopTokens.raised : "#8A04060A"
            Rectangle { width: parent.width; height: 1; color: "#14FFFFFF" }
            Row {
                x: 16; anchors.verticalCenter: parent.verticalCenter; spacing: 15
                DesktopKeyHint { keyText: qsTr("↑ ↓"); shortcut: "↑ ↓"; label: qsTr("Move") }
                DesktopKeyHint { keyText: qsTr("Tab"); shortcut: "Tab"; label: qsTr("Filter type") }
                DesktopKeyHint { keyText: "Esc"; label: qsTr("Close") }
            }
            Text {
                anchors.right: parent.right; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter
                text: root.scopeFilter === "all"
                    ? qsTr("%1 RESULTS").arg(root.resultCount)
                    : qsTr("%1 · %2").arg(root.scopeFilter.toUpperCase()).arg(qsTr("%1 RESULTS").arg(root.resultCount))
                color: DesktopTokens.textFaint
                font.family: DesktopTokens.monoFont
                font.pixelSize: DesktopTokens.microSize
                font.weight: Font.DemiBold
                font.letterSpacing: 0.6
            }
        }
    }
    Keys.onUpPressed: root.moveCurrent(-1)
    Keys.onDownPressed: root.moveCurrent(1)
    Keys.onReturnPressed: root.acceptCurrent()
    Keys.onEnterPressed: root.acceptCurrent()
    Keys.onTabPressed: root.cycleScope()
    Keys.onEscapePressed: root.closeRequested()
}
