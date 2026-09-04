pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Effects
import OpenNOW

FocusScope {
    id: root

    readonly property string pageTitle: qsTr("Home")
    readonly property string pageSubtitle: ShellStore.catalogTotalCount
        ? qsTr("%1 games").arg(ShellStore.catalogTotalCount)
        : qsTr("Your library")
    property int focusZone: 0
    property int focusIndex: 0
    property bool active: true

    signal routeRequested(string route)
    signal gameRequested(var game)

    anchors.fill: parent
    focus: active
    clip: true
    Accessible.role: Accessible.Pane
    Accessible.name: pageTitle

    readonly property var games: ShellStore.catalogGames || []
    readonly property var sortedRecent: {
        const list = (root.games || []).slice()
        list.sort((left, right) => String(right.lastPlayed || "").localeCompare(String(left.lastPlayed || "")))
        return list
    }
    readonly property var heroGame: root.sortedRecent.length ? root.sortedRecent[0] : null
    readonly property string heroArtwork: DesktopTokens.decodeArtworkUrl(
        DesktopTokens.artworkUrl(root.heroGame, true))
    readonly property var jumpGames: root.takeGames(root.sortedRecent, 1, 12)
    readonly property var favoriteGames: {
        const list = []
        for (let i = 0; i < root.games.length; ++i) {
            if (ShellStore.isFavorite(root.games[i]))
                list.push(root.games[i])
        }
        return list.length ? list : root.takeGames(root.games, 0, 12)
    }
    readonly property var newGames: root.takeGames(root.games, Math.max(0, root.games.length - 12), 12)
    readonly property bool friendsAvailable: Boolean(ShellStore.socialCapabilities && ShellStore.socialCapabilities.friendsAvailable)
    readonly property int railGap: 14
    readonly property int railInnerWidth: Math.max(200, contentFlick.width - 48)
    readonly property int homeTileCount: {
        const fit = Math.max(5, Math.floor((root.railInnerWidth + root.railGap) / (112 + root.railGap)))
        return Math.max(1, Math.min(fit, 10))
    }
    readonly property int homeTileWidth: Math.max(112, Math.floor((root.railInnerWidth - root.railGap * (root.homeTileCount - 1)) / root.homeTileCount))
    readonly property int homeTileHeight: Math.round(root.homeTileWidth * 168 / 112)

    function takeGames(source, start, limit) {
        const list = source || []
        const result = []
        for (let index = start; index < list.length && result.length < limit; ++index)
            result.push(list[index])
        return result
    }

    ArtworkSource {
        id: heroArtworkSource
        sourceUrl: root.heroArtwork
        active: root.active
    }

    function heroMeta() {
        const game = root.heroGame
        if (!game)
            return qsTr("Sign in and sync your library to continue a game.")
        const last = String(game.lastPlayed || "")
        const hours = game.hoursPlayed ? qsTr("%1 h played").arg(game.hoursPlayed) : ""
        if (last !== "" && hours !== "")
            return last + " · " + hours
        if (last !== "")
            return last
        if (hours !== "")
            return hours
        return qsTr("Ready to stream from your library")
    }

    function streamChip() {
        const res = String(ShellStore.settings.resolution || "")
        const fps = Number(ShellStore.settings.fps || 0)
        const codec = String(ShellStore.settings.codec || "auto").toUpperCase()
        const parts = []
        if (res.indexOf("x") > 0)
            parts.push(res.split("x")[1] + "p")
        if (fps > 0)
            parts.push(fps + " fps")
        if (codec !== "")
            parts.push(codec)
        return parts.length ? parts.join(" · ") : qsTr("Stream ready")
    }

    function zoneGames(zone) {
        if (zone === 1) return root.jumpGames
        if (zone === 2) return root.favoriteGames
        return root.newGames
    }

    function setSelection(zone, index) {
        root.focusZone = Math.max(0, Math.min(3, zone))
        const count = root.focusZone === 0 ? 6 : root.zoneGames(root.focusZone).length
        root.focusIndex = Math.max(0, Math.min(Math.max(0, count - 1), index))
        root.ensureSelectionVisible()
    }

    function ensureSelectionVisible() {
        if (root.focusZone <= 1) {
            if (contentFlick.contentY > 32)
                contentFlick.contentY = 0
            return
        }
        const zoneTop = root.focusZone === 2
            ? (heroRow.height + 18 + jumpRail.height + 18)
            : (heroRow.height + 18 + jumpRail.height + 18 + playingRail.height + 18)
        const zoneBottom = zoneTop + 30 + root.homeTileHeight
        if (zoneBottom > contentFlick.contentY + contentFlick.height - 12)
            contentFlick.contentY = Math.min(contentFlick.contentHeight - contentFlick.height,
                                             zoneBottom - contentFlick.height + 12)
        else if (zoneTop < contentFlick.contentY + 12)
            contentFlick.contentY = Math.max(0, zoneTop - 12)
    }

    function moveHorizontal(delta) {
        const count = root.focusZone === 0 ? 6 : root.zoneGames(root.focusZone).length
        if (count <= 0)
            return
        root.setSelection(root.focusZone, Math.max(0, Math.min(count - 1, root.focusIndex + delta)))
    }

    function moveVertical(delta) {
        const nextZone = Math.max(0, Math.min(3, root.focusZone + delta))
        if (nextZone === root.focusZone)
            return
        let nextIndex = root.focusIndex
        if (nextZone === 0)
            nextIndex = Math.min(1, Math.round(root.focusIndex / 4))
        else if (root.focusZone === 0)
            nextIndex = Math.min(8, root.focusIndex * 2)
        root.setSelection(nextZone, nextIndex)
    }

    function openGame(game) {
        if (!game)
            return
        root.gameRequested(game)
    }

    function startHero() {
        if (!root.heroGame)
            return
        ShellStore.selectedGame = root.heroGame
        if (ShellStore.signedIn)
            ShellStore.launchSelectedGame(false)
        else
            AppController.navigate("sign-in")
    }

    function openFriends() {
        AppController.showOverlay("friends")
    }

    function activateSelection() {
        if (root.focusZone === 0) {
            if (root.focusIndex === 0)
                root.startHero()
            else if (root.focusIndex === 1)
                root.openGame(root.heroGame)
            else
                root.openFriends()
            return
        }
        const selectedGames = root.zoneGames(root.focusZone)
        if (selectedGames.length > root.focusIndex)
            root.openGame(selectedGames[root.focusIndex])
    }

    Keys.onPressed: event => {
        if (event.key === Qt.Key_Left) {
            root.moveHorizontal(-1)
        } else if (event.key === Qt.Key_Right) {
            root.moveHorizontal(1)
        } else if (event.key === Qt.Key_Up) {
            root.moveVertical(-1)
        } else if (event.key === Qt.Key_Down) {
            root.moveVertical(1)
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter || event.key === Qt.Key_Space) {
            root.activateSelection()
        } else {
            return
        }
        event.accepted = true
    }

    Flickable {
        id: contentFlick
        anchors.fill: parent
        contentWidth: width
        contentHeight: Math.max(height, homeColumn.implicitHeight + 28)
        clip: true
        interactive: true
        boundsBehavior: Flickable.StopAtBounds
        flickDeceleration: 5200
        maximumFlickVelocity: 2200
        Accessible.role: Accessible.Pane

        Column {
            id: homeColumn
            x: 24
            y: 18
            width: contentFlick.width - 48
            spacing: 18

            Item {
                id: heroRow
                width: parent.width
                height: 262

                Rectangle {
                    id: heroMask
                    anchors.fill: heroCard
                    radius: 16
                    color: "white"
                    visible: false
                    layer.enabled: true
                }

                Item {
                    id: heroCard
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.right: friendsPanel.left
                    anchors.rightMargin: 16
                    height: 262
                    layer.enabled: true
                    layer.smooth: true
                    layer.effect: MultiEffect {
                        maskEnabled: true
                        maskSource: heroMask
                        maskThresholdMin: 0.25
                        maskSpreadAtMin: 0.2
                        shadowEnabled: true
                        shadowColor: "#73000000"
                        shadowBlur: 0.55
                        shadowVerticalOffset: 10
                    }

                    Rectangle { anchors.fill: parent; color: "#171B27" }
                    Image {
                        anchors.fill: parent
                        source: heroArtworkSource.resolvedUrl
                        fillMode: Image.PreserveAspectCrop
                        sourceSize: Qt.size(Math.ceil(width), Math.ceil(height))
                        asynchronous: true
                        cache: true
                    }
                    Rectangle {
                        anchors.fill: parent
                        gradient: Gradient {
                            orientation: Gradient.Horizontal
                            GradientStop { position: 0; color: "#EB060912" }
                            GradientStop { position: 0.62; color: "#4D060912" }
                            GradientStop { position: 1; color: "#1A060912" }
                        }
                    }
                    Rectangle {
                        anchors.fill: parent
                        color: "transparent"
                        border.width: 1
                        border.color: "#29FFFFFF"
                        radius: 16
                    }

                    Column {
                        x: 22
                        anchors.bottom: parent.bottom
                        anchors.bottomMargin: 22
                        spacing: 14

                        Text {
                            text: qsTr("CONTINUE PLAYING")
                            color: DesktopTokens.focus
                            font.family: Theme.monoFont
                            font.pixelSize: 10
                            font.weight: Font.DemiBold
                            font.letterSpacing: 1
                        }

                        Column {
                            spacing: 6
                            Text {
                                text: root.heroGame ? String(root.heroGame.title || qsTr("Game")) : qsTr("No games yet")
                                color: "#FFFFFF"
                                font.family: Theme.displayFont
                                font.pixelSize: 34
                                font.weight: Font.Black
                                font.letterSpacing: -1
                            }
                            Text {
                                text: root.heroMeta()
                                color: "#B8FFFFFF"
                                font.family: Theme.bodyFont
                                font.pixelSize: 13
                                font.weight: Font.DemiBold
                            }
                        }

                        Row {
                            spacing: 9

                            Rectangle {
                                id: startButton
                                width: 158
                                height: 38
                                radius: 10
                                color: startTap.pressed ? "#D9FFFFFF" : "#F2FFFFFF"
                                border.width: root.focusZone === 0 && root.focusIndex === 0 && AppController.inputMode !== "pointer" ? 2 : 0
                                border.color: "#FFFFFF"

                                Row {
                                    anchors.centerIn: parent
                                    spacing: 8
                                    DesktopGlyph { width: 10; height: 12; icon: "desktop-play.svg" }
                                    Text { text: qsTr("Start"); color: "#0B0F1A"; font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.ExtraBold }
                                    Rectangle {
                                        width: 41; height: 19; radius: 5; color: "#1A0B0F1A"
                                        Text { anchors.centerIn: parent; text: qsTr("ENTER"); color: "#B80B0F1A"; font.family: Theme.monoFont; font.pixelSize: 9; font.weight: Font.Bold }
                                    }
                                }
                                HoverHandler { id: startHover; cursorShape: Qt.PointingHandCursor; onHoveredChanged: if (hovered) root.setSelection(0, 0) }
                                TapHandler { id: startTap; onTapped: root.startHero() }
                            }

                            Rectangle {
                                id: detailsButton
                                width: 81
                                height: 38
                                radius: 10
                                color: detailsHover.hovered ? "#2EFFFFFF" : "#1FFFFFFF"
                                border.width: root.focusZone === 0 && root.focusIndex === 1 && AppController.inputMode !== "pointer" ? 2 : 1
                                border.color: root.focusZone === 0 && root.focusIndex === 1 && AppController.inputMode !== "pointer" ? "#FFFFFF" : "#33FFFFFF"
                                Text { anchors.centerIn: parent; text: qsTr("Details"); color: "#FFFFFF"; font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.Bold }
                                HoverHandler { id: detailsHover; cursorShape: Qt.PointingHandCursor; onHoveredChanged: if (hovered) root.setSelection(0, 1) }
                                TapHandler { onTapped: root.openGame(root.heroGame) }
                                Behavior on color { ColorAnimation { duration: AppController.reducedMotion ? 0 : 90 } }
                            }

                            Rectangle {
                                width: 182
                                height: 38
                                radius: 10
                                color: "#59000000"
                                border.width: 1
                                border.color: "#1FFFFFFF"
                                Row {
                                    anchors.centerIn: parent
                                    spacing: 8
                                    Rectangle { width: 6; height: 6; radius: 3; color: "#1DB954" }
                                    Text { text: root.streamChip(); color: "#CCFFFFFF"; font.family: Theme.monoFont; font.pixelSize: 10; font.weight: Font.DemiBold; font.letterSpacing: 0.4 }
                                }
                            }
                        }
                    }
                }

                Rectangle {
                    id: friendsPanel
                    anchors.right: parent.right
                    y: 0
                    width: Math.max(260, Math.min(318, parent.width * 0.28))
                    height: 262
                    radius: 16
                    color: "#C70B0F1A"
                    border.width: 1
                    border.color: "#17FFFFFF"

                    Text {
                        x: 16; y: 16
                        text: qsTr("Friends")
                        color: "#FFFFFF"
                        font.family: Theme.bodyFont
                        font.pixelSize: 14
                        font.weight: Font.ExtraBold
                    }
                    Text {
                        anchors.right: parent.right; anchors.rightMargin: 16; y: 17
                        text: root.friendsAvailable ? qsTr("LIVE") : qsTr("UNAVAILABLE")
                        color: "#80FFFFFF"
                        font.family: Theme.monoFont
                        font.pixelSize: 10
                        font.weight: Font.DemiBold
                        font.letterSpacing: 0.4
                    }

                    Text {
                        x: 16
                        y: 52
                        width: parent.width - 32
                        wrapMode: Text.WordWrap
                        text: root.friendsAvailable
                            ? qsTr("Your provider friends list is ready.")
                            : (ShellStore.socialCapabilities.reason || qsTr("GeForce NOW does not expose a friends API OpenNOW can use."))
                        color: "#99FFFFFF"
                        font.family: Theme.bodyFont
                        font.pixelSize: 12
                        lineHeight: 1.4
                    }

                    Rectangle {
                        id: partyButton
                        x: 16; y: 216
                        width: parent.width - 32; height: 34; radius: 10
                        color: partyHover.hovered ? "#17FFFFFF" : "#0FFFFFFF"
                        border.width: root.focusZone === 0 && root.focusIndex === 5 && AppController.inputMode !== "pointer" ? 2 : 1
                        border.color: root.focusZone === 0 && root.focusIndex === 5 && AppController.inputMode !== "pointer" ? "#FFFFFF" : "#1FFFFFFF"
                        Text {
                            anchors.centerIn: parent
                            text: qsTr("Open friends")
                            color: "#CCFFFFFF"
                            font.family: Theme.bodyFont
                            font.pixelSize: 12
                            font.weight: Font.Bold
                        }
                        HoverHandler { id: partyHover; cursorShape: Qt.PointingHandCursor; onHoveredChanged: if (hovered) root.setSelection(0, 5) }
                        TapHandler { onTapped: root.openFriends() }
                        Behavior on color { ColorAnimation { duration: AppController.reducedMotion ? 0 : 90 } }
                    }
                }
            }

            Item {
                id: jumpRail
                width: parent.width
                height: 30 + root.homeTileHeight
                Text { text: qsTr("Jump back in"); color: "#FFFFFF"; font.family: Theme.displayFont; font.pixelSize: 16; font.weight: Font.Black; font.letterSpacing: -0.2 }
                Text {
                    anchors.right: parent.right; y: 1
                    text: qsTr("See all %1  ›").arg(root.games.length)
                    color: seeJump.hovered ? "#B8FFFFFF" : "#80FFFFFF"
                    font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Bold
                    HoverHandler { id: seeJump; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: root.routeRequested("library") }
                }
                Row {
                    y: 30; width: parent.width; spacing: root.railGap
                    Repeater {
                        model: root.takeGames(root.jumpGames, 0, root.homeTileCount)
                        DesktopHomePoster {
                            required property var modelData
                            required property int index
                            game: modelData
                            tileWidth: root.homeTileWidth
                            tileHeight: root.homeTileHeight
                            current: root.focusZone === 1 && root.focusIndex === index
                            onPointed: root.setSelection(1, index)
                            onActivated: root.openGame(modelData)
                        }
                    }
                }
            }

            Item {
                id: playingRail
                width: parent.width
                height: 30 + root.homeTileHeight
                Text { text: qsTr("Favourites"); color: "#FFFFFF"; font.family: Theme.displayFont; font.pixelSize: 16; font.weight: Font.Black; font.letterSpacing: -0.2 }
                Text {
                    anchors.right: parent.right; y: 1
                    text: qsTr("See all  ›")
                    color: seeFriends.hovered ? "#B8FFFFFF" : "#80FFFFFF"
                    font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Bold
                    HoverHandler { id: seeFriends; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: root.routeRequested("library") }
                }
                Row {
                    y: 30; width: parent.width; spacing: root.railGap
                    Repeater {
                        model: root.takeGames(root.favoriteGames, 0, root.homeTileCount)
                        DesktopHomePoster {
                            required property var modelData
                            required property int index
                            game: modelData
                            tileWidth: root.homeTileWidth
                            tileHeight: root.homeTileHeight
                            current: root.focusZone === 2 && root.focusIndex === index
                            onPointed: root.setSelection(2, index)
                            onActivated: root.openGame(modelData)
                        }
                    }
                }
            }

            Item {
                id: newRail
                width: parent.width
                height: 30 + root.homeTileHeight
                Text { text: qsTr("New in your library"); color: "#FFFFFF"; font.family: Theme.displayFont; font.pixelSize: 16; font.weight: Font.Black; font.letterSpacing: -0.2 }
                Text {
                    anchors.right: parent.right; y: 1
                    text: qsTr("See all  ›")
                    color: seeNew.hovered ? "#B8FFFFFF" : "#80FFFFFF"
                    font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Bold
                    HoverHandler { id: seeNew; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: root.routeRequested("library") }
                }
                Row {
                    y: 30; width: parent.width; spacing: root.railGap
                    Repeater {
                        model: root.takeGames(root.newGames, 0, root.homeTileCount)
                        DesktopHomePoster {
                            required property var modelData
                            required property int index
                            game: modelData
                            tileWidth: root.homeTileWidth
                            tileHeight: root.homeTileHeight
                            current: root.focusZone === 3 && root.focusIndex === index
                            onPointed: root.setSelection(3, index)
                            onActivated: root.openGame(modelData)
                        }
                    }
                }
            }
        }
    }

    Component.onCompleted: {
        root.focusZone = 0
        root.focusIndex = Math.max(0, Math.min(5, ShellStore.focusIndex("desktop-home")))
        if (root.active)
            Qt.callLater(root.forceActiveFocus)
    }
    onFocusIndexChanged: ShellStore.rememberFocus("desktop-home", focusIndex)
}
