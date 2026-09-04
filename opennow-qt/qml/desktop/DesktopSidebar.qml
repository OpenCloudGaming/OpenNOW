import QtQuick
import QtQuick.Controls
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    property string currentRoute: "home"
    property bool collapsed: true
    property bool hoverExpanded: false
    readonly property bool overlayOpen: !collapsed || hoverExpanded
    readonly property bool compact: !overlayOpen
    readonly property bool consoleModeOn: DesktopTokens.consoleModeOn(Window.window)
    readonly property bool consoleModePending: DesktopTokens.consoleModePending(Window.window)
    readonly property bool friendsAvailable: Boolean(ShellStore.socialCapabilities && ShellStore.socialCapabilities.friendsAvailable)
    signal routeRequested(string route)
    signal consoleModeRequested()
    signal collapseRequested(bool collapsed)

    x: 0
    width: compact ? DesktopTokens.railCollapsedWidth : DesktopTokens.railWidth
    height: parent ? parent.height : 900
    z: overlayOpen ? 40 : 3

    function closeOverlay() {
        hoverExpanded = false
        if (!collapsed)
            collapseRequested(true)
    }

    Behavior on width {
        NumberAnimation { duration: DesktopTokens.motionDuration; easing.type: Easing.OutCubic }
    }

    readonly property var navItems: [
        { route: "home", icon: "desktop-nav-home.svg", name: qsTr("Home") },
        { route: "library", icon: "desktop-nav-library.svg", name: qsTr("Library") },
        { route: "store", icon: "desktop-nav-store.svg", name: qsTr("Store") },
        { route: "friends", icon: "desktop-nav-friends.svg", name: qsTr("Friends") },
        { route: "settings", icon: "desktop-nav-settings.svg", name: qsTr("Settings") }
    ]

    readonly property var collections: [
        { icon: "desktop-star.svg", name: qsTr("Favourites"), count: String(root.favoriteCount()), filter: "favorites" },
        { icon: "desktop-clock.svg", name: qsTr("Recently played"), count: String(root.recentCount()), filter: "recent" },
        { icon: "desktop-coop.svg", name: qsTr("Co-op with friends"), count: String(root.readyCount()), filter: "ready" },
        { icon: "desktop-rtx.svg", name: qsTr("RTX ready"), count: String(root.rtxCount()), filter: "rtx" }
    ]

    function favoriteCount() {
        const games = ShellStore.catalogGames || []
        let count = 0
        for (let i = 0; i < games.length; ++i) {
            if (ShellStore.isFavorite(games[i]))
                count += 1
        }
        return count
    }
    function recentCount() {
        const games = ShellStore.catalogGames || []
        let count = 0
        for (let i = 0; i < games.length; ++i) {
            if (games[i].lastPlayed)
                count += 1
        }
        return count
    }
    function readyCount() {
        const games = ShellStore.catalogGames || []
        let count = 0
        for (let i = 0; i < games.length; ++i) {
            if (games[i].isInLibrary || games[i].isAvailable)
                count += 1
        }
        return count
    }
    function rtxCount() {
        const games = ShellStore.catalogGames || []
        let count = 0
        for (let i = 0; i < games.length; ++i) {
            const tags = (games[i].genres || []).join(" ").toLowerCase()
            const title = String(games[i].title || "").toLowerCase()
            if (tags.indexOf("rtx") >= 0 || title.indexOf("rtx") >= 0)
                count += 1
        }
        return count
    }

    function liveMembershipTier() {
        // The login claim goes stale (e.g. upgrade after sign-in); the live
        // subscription is authoritative, the cached claim is the fallback.
        if (ShellStore.subscription && ShellStore.subscription.membershipTier)
            return String(ShellStore.subscription.membershipTier).toUpperCase()
        if (ShellStore.signedIn && ShellStore.authSession && ShellStore.authSession.user
                && ShellStore.authSession.user.membershipTier)
            return String(ShellStore.authSession.user.membershipTier).toUpperCase()
        return ShellStore.signedIn ? qsTr("Member").toUpperCase() : qsTr("NOT SIGNED IN")
    }

    function routeSelected(route) {
        if (route === "settings")
            return root.currentRoute.indexOf("settings") === 0
        if (route === "library")
            return root.currentRoute === "library" || root.currentRoute === "game-detail"
        return root.currentRoute === route
    }

    Rectangle {
        anchors.fill: parent
        color: DesktopTokens.rail
        Rectangle {
            anchors.right: parent.right
            width: 1
            height: parent.height
            color: root.overlayOpen ? "#29FFFFFF" : DesktopTokens.seamSoft
        }
    }

    Rectangle {
        visible: root.overlayOpen
        x: root.width
        width: DesktopTokens.px(40)
        height: parent.height
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: "#A8000000" }
            GradientStop { position: 1; color: "#00000000" }
        }
    }

    HoverHandler {
        acceptedDevices: PointerDevice.Mouse
        onHoveredChanged: root.hoverExpanded = root.collapsed && hovered
    }

    Item {
        anchors.fill: parent
        anchors.topMargin: root.compact ? 16 : 20
        anchors.leftMargin: root.compact ? 14 : 12
        anchors.rightMargin: root.compact ? 14 : 12
        anchors.bottomMargin: 12

        Flickable {
            id: railFlick
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: dock.top
            anchors.bottomMargin: 8
            clip: true
            contentWidth: width
            contentHeight: topColumn.implicitHeight
            boundsBehavior: Flickable.StopAtBounds
            flickableDirection: Flickable.VerticalFlick

            Column {
                id: topColumn
                width: railFlick.width
                spacing: root.compact ? 10 : 22

        Item {
            width: parent.width
            height: 28

            Image {
                id: brandMark
                width: root.compact ? 40 : 22
                height: root.compact ? 22 : 12
                anchors.verticalCenter: parent.verticalCenter
                anchors.horizontalCenter: root.compact ? parent.horizontalCenter : undefined
                anchors.left: root.compact ? undefined : parent.left
                anchors.leftMargin: root.compact ? 0 : 10
                source: "qrc:/qt/qml/OpenNOW/res/brand/opennow-mark.png"
                fillMode: Image.PreserveAspectFit
                smooth: false
                sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
            }

            Text {
                visible: !root.compact
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: brandMark.right
                anchors.leftMargin: 10
                text: "OpenNOW"
                color: DesktopTokens.text
                font.family: DesktopTokens.displayFont
                font.pixelSize: DesktopTokens.headingSize
                font.weight: Font.Black
                font.letterSpacing: -0.32
            }

            Rectangle {
                id: collapseButton
                visible: !root.compact
                width: 28
                height: 28
                anchors.verticalCenter: parent.verticalCenter
                anchors.right: parent.right
                radius: 9
                color: collapseHover.hovered || collapseButton.activeFocus ? "#17FFFFFF" : "#0FFFFFFF"
                border.width: 1
                border.color: "#17FFFFFF"
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Collapse sidebar")
                Accessible.onPressAction: {
                    root.hoverExpanded = false
                    root.collapseRequested(true)
                }

                DesktopGlyph {
                    anchors.centerIn: parent
                    width: 13
                    height: 13
                    icon: "desktop-collapse.svg"
                }
                HoverHandler { id: collapseHover; cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    onTapped: {
                        root.hoverExpanded = false
                        root.collapseRequested(true)
                    }
                }
            }
        }

        Item {
            visible: root.compact
            width: parent.width
            height: 28
            Rectangle {
                id: expandButton
                width: 40
                height: 28
                anchors.horizontalCenter: parent.horizontalCenter
                radius: 9
                color: expandHover.hovered || expandButton.activeFocus ? "#17FFFFFF" : "#0FFFFFFF"
                border.width: 1
                border.color: "#17FFFFFF"
                Accessible.role: Accessible.Button
                Accessible.name: qsTr("Expand sidebar")
                Accessible.onPressAction: {
                    root.hoverExpanded = false
                    root.collapseRequested(false)
                }

                DesktopGlyph {
                    anchors.centerIn: parent
                    width: 13
                    height: 13
                    icon: "desktop-expand.svg"
                }
                HoverHandler { id: expandHover; cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    onTapped: {
                        root.hoverExpanded = false
                        root.collapseRequested(false)
                    }
                }
            }
        }

        Item {
            visible: root.compact
            width: parent.width
            height: 1
            Rectangle {
                width: 28
                height: 1
                anchors.horizontalCenter: parent.horizontalCenter
                color: DesktopTokens.seamSoft
            }
        }

        Column {
            width: parent.width
            spacing: 3

            Repeater {
                model: root.navItems
                delegate: ItemDelegate {
                    id: navButton
                    required property var modelData
                    width: parent.width
                    height: root.compact ? 44 : 38
                    padding: 0
                    readonly property bool selected: root.routeSelected(modelData.route)
                    Accessible.name: modelData.name
                    background: Rectangle {
                        radius: 10
                        color: navButton.selected ? "#14FFFFFF"
                            : (navButton.hovered || navButton.activeFocus ? "#0CFFFFFF" : "transparent")
                    }
                    contentItem: Item {
                        DesktopGlyph {
                            x: root.compact ? (parent.width - 18) / 2 : 10
                            anchors.verticalCenter: parent.verticalCenter
                            width: 18
                            height: 18
                            icon: navButton.modelData.icon
                            active: navButton.selected
                        }
                        Text {
                            x: 39
                            anchors.verticalCenter: parent.verticalCenter
                            visible: !root.compact
                            text: navButton.modelData.name
                            color: navButton.selected ? DesktopTokens.text : "#B8FFFFFF"
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: DesktopTokens.px(14)
                            font.weight: navButton.selected ? Font.ExtraBold : Font.DemiBold
                        }
                        Row {
                            anchors.right: parent.right
                            anchors.rightMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            visible: navButton.modelData.route === "friends" && !navButton.selected
                            Rectangle {
                                width: 6
                                height: 6
                                radius: 3
                                anchors.verticalCenter: parent.verticalCenter
                                color: root.friendsAvailable ? DesktopTokens.green : DesktopTokens.textFaint
                            }
                        }
                        Rectangle {
                            anchors.right: parent.right
                            anchors.rightMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            visible: navButton.selected && !root.compact
                            width: 4
                            height: 16
                            radius: 999
                            color: DesktopTokens.focus
                        }
                    }
                    onClicked: root.routeRequested(modelData.route)
                }
            }
        }

        Column {
            width: parent.width
            visible: !root.compact
            spacing: 1

            Rectangle {
                width: parent.width
                height: 1
                color: DesktopTokens.seamSoft
            }

            Item {
                width: parent.width
                height: 20
            }

            Item {
                width: parent.width
                height: 26
                Text {
                    x: 10
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("COLLECTIONS")
                    color: DesktopTokens.textFaint
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: 9
                    font.weight: Font.DemiBold
                    font.letterSpacing: 0.9
                }
                DesktopGlyph {
                    anchors.right: parent.right
                    anchors.rightMargin: 8
                    anchors.verticalCenter: parent.verticalCenter
                    width: 11
                    height: 11
                    icon: "desktop-plus.svg"
                }
            }

            Repeater {
                model: root.collections
                delegate: Item {
                    required property var modelData
                    width: parent.width
                    height: 32
                    DesktopGlyph {
                        x: 10
                        anchors.verticalCenter: parent.verticalCenter
                        width: 14
                        height: 14
                        icon: modelData.icon
                    }
                    Text {
                        x: 34
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.name
                        color: collectionHover.hovered ? "#E0FFFFFF" : "#A3FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                    }
                    Text {
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        text: modelData.count
                        color: "#4DFFFFFF"
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: 10
                        font.weight: Font.DemiBold
                    }
                    HoverHandler { id: collectionHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: root.routeRequested("library") }
                }
            }
        }

        Item {
            visible: root.compact
            width: parent.width
            height: 1
            Rectangle {
                width: 28
                height: 1
                anchors.horizontalCenter: parent.horizontalCenter
                color: DesktopTokens.seamSoft
            }
        }

        Item {
            visible: root.compact
            width: parent.width
            height: 44
            DesktopGlyph {
                anchors.centerIn: parent
                width: 18
                height: 18
                icon: "desktop-star.svg"
            }
        }
            }
        }

        Column {
            id: dock
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            spacing: 4

            Rectangle {
                visible: !root.compact
                width: parent.width
                height: 1
                color: DesktopTokens.seamSoft
            }

            ItemDelegate {
                id: consoleModeButton
                width: parent.width
                height: 44
                padding: 0
                enabled: !root.consoleModePending
                opacity: root.consoleModePending ? 0.7 : 1
                Accessible.name: qsTr("Console mode")
                Accessible.description: root.consoleModePending
                    ? qsTr("Switching surfaces")
                    : (root.consoleModeOn ? qsTr("Console mode is on") : qsTr("Console mode is off"))
                Behavior on opacity { NumberAnimation { duration: DesktopTokens.quickDuration } }
                background: Rectangle {
                    radius: 11
                    color: consoleModeButton.hovered || consoleModeButton.activeFocus ? "#0CFFFFFF" : "transparent"
                }
                contentItem: Item {
                    DesktopGlyph {
                        x: root.compact ? (parent.width - 17) / 2 : 10
                        anchors.verticalCenter: parent.verticalCenter
                        width: 17
                        height: 17
                        icon: "desktop-gamepad.svg"
                    }
                    Column {
                        x: 37
                        anchors.verticalCenter: parent.verticalCenter
                        visible: !root.compact
                        spacing: 2
                        Text {
                            text: root.consoleModePending ? qsTr("Console mode…") : qsTr("Console mode")
                            color: DesktopTokens.textHigh
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 13
                            font.weight: Font.Bold
                        }
                        Row {
                            spacing: 5
                            Rectangle {
                                width: 5
                                height: 5
                                radius: 3
                                color: root.consoleModePending ? DesktopTokens.amber
                                    : root.consoleModeOn ? DesktopTokens.mint : DesktopTokens.ledAmber
                                SequentialAnimation on opacity {
                                    running: root.consoleModePending
                                    loops: Animation.Infinite
                                    NumberAnimation { to: 0.25; duration: 420 }
                                    NumberAnimation { to: 1; duration: 420 }
                                }
                            }
                            Text {
                                text: root.consoleModePending ? qsTr("SWITCHING…")
                                    : root.consoleModeOn ? qsTr("CONSOLE ON") : qsTr("GAMEPAD READY")
                                color: DesktopTokens.textMuted
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 9
                                font.weight: Font.DemiBold
                                font.letterSpacing: 0.36
                            }
                        }
                    }
                    Rectangle {
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        visible: !root.compact
                        width: 32
                        height: 19
                        radius: 999
                        color: root.consoleModeOn ? "#2E6EE7B7" : "#1FFFFFFF"
                        border.width: 1
                        border.color: root.consoleModeOn ? "#526EE7B7" : "#1AFFFFFF"
                        Rectangle {
                            x: root.consoleModeOn ? 17 : 2
                            y: 2
                            width: 13
                            height: 13
                            radius: 999
                            color: root.consoleModeOn ? DesktopTokens.focus : "#CCFFFFFF"
                            Behavior on x { NumberAnimation { duration: DesktopTokens.quickDuration; easing.type: Easing.OutCubic } }
                        }
                    }
                    Rectangle {
                        anchors.right: parent.right
                        anchors.top: parent.top
                        width: 7
                        height: 7
                        radius: 4
                        visible: root.compact
                        color: root.consoleModePending ? DesktopTokens.amber
                            : root.consoleModeOn ? DesktopTokens.mint : DesktopTokens.ledAmber
                    }
                }
                onClicked: root.consoleModeRequested()
            }

            ItemDelegate {
                id: profileButton
                width: parent.width
                height: 44
                padding: 0
                Accessible.name: qsTr("Profile")
                background: Rectangle {
                    radius: 11
                    color: profileButton.hovered || profileButton.activeFocus ? "#0CFFFFFF" : "transparent"
                }
                contentItem: Item {
                    Rectangle {
                        x: root.compact ? (parent.width - (root.compact ? 36 : 28)) / 2 : 10
                        anchors.verticalCenter: parent.verticalCenter
                        width: root.compact ? 36 : 28
                        height: root.compact ? 36 : 28
                        radius: 999
                        color: "#17FFFFFF"
                        border.width: 1
                        border.color: "#29FFFFFF"
                        Text {
                            anchors.centerIn: parent
                            text: ShellStore.signedIn && ShellStore.authSession.user
                                ? String(ShellStore.authSession.user.displayName || "?").charAt(0).toUpperCase()
                                : "Z"
                            color: DesktopTokens.textHigh
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: DesktopTokens.monoSize
                            font.weight: Font.Black
                        }
                    }
                    Column {
                        x: 48
                        anchors.verticalCenter: parent.verticalCenter
                        visible: !root.compact
                        spacing: 2
                        Text {
                            text: ShellStore.signedIn && ShellStore.authSession.user
                                ? ShellStore.authSession.user.displayName
                                : qsTr("Guest")
                            color: DesktopTokens.textHigh
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: DesktopTokens.captionSize
                            font.weight: Font.Bold
                        }
                        Text {
                            text: root.liveMembershipTier()
                            color: DesktopTokens.textFaint
                            font.family: DesktopTokens.monoFont
                            font.pixelSize: DesktopTokens.tinySize
                            font.weight: Font.DemiBold
                            font.letterSpacing: 0.36
                        }
                    }
                    DesktopGlyph {
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        visible: !root.compact
                        width: 10
                        height: 10
                        icon: "desktop-chevron-up.svg"
                    }
                }
                onClicked: root.routeRequested("settings-account")
            }
        }
    }
}
