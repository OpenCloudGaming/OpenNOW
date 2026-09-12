import QtQuick
import QtQuick.Controls
import QtQuick.Window
import OpenNOW

FocusScope {
    id: root
    objectName: "desktopSidebar"
    property string currentRoute: "home"
    property bool collapsed: true
    property bool hoverExpanded: false
    readonly property bool overlayOpen: !collapsed || hoverExpanded
    readonly property bool compact: !overlayOpen
    readonly property real reveal: Math.max(0, Math.min(1, (width - DesktopTokens.railCollapsedWidth) / (DesktopTokens.railWidth - DesktopTokens.railCollapsedWidth)))
    readonly property bool consoleModeOn: DesktopTokens.consoleModeOn(Window.window)
    readonly property bool consoleModePending: DesktopTokens.consoleModePending(Window.window)
    readonly property bool friendsAvailable: Boolean(ShellStore.socialCapabilities && ShellStore.socialCapabilities.friendsAvailable)
    signal routeRequested(string route)
    signal consoleModeRequested()
    signal collapseRequested(bool collapsed)
    signal createCollectionRequested()

    x: 0
    width: overlayOpen ? DesktopTokens.railWidth : DesktopTokens.railCollapsedWidth
    height: parent ? parent.height : 900
    z: overlayOpen ? 40 : 3

    function closeOverlay() {
        hoverExpanded = false
        if (!collapsed)
            collapseRequested(true)
    }

    Behavior on width {
        NumberAnimation { duration: AppController.reducedMotion ? 0 : DesktopTokens.motionDuration; easing.type: Easing.OutCubic }
    }

    readonly property var navItems: [
        { route: "home", icon: "desktop-nav-home.svg", name: qsTr("Home") },
        { route: "library", icon: "desktop-nav-library.svg", name: qsTr("Library") },
        { route: "store", icon: "desktop-nav-store.svg", name: qsTr("Store") },
        { route: "friends", icon: "desktop-nav-friends.svg", name: qsTr("Friends") },
        { route: "settings", icon: "desktop-nav-settings.svg", name: qsTr("Settings") }
    ]

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
        color: DesktopTokens.shell
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
        enabled: !SmokeTestMode && ShellStore.settings.desktopSidebarHover !== false
        onHoveredChanged: root.hoverExpanded = root.collapsed && hovered && ShellStore.settings.desktopSidebarHover !== false
    }

    Item {
        anchors.fill: parent
        clip: true
        anchors.topMargin: 16
        anchors.leftMargin: 14
        anchors.rightMargin: 14
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
                spacing: 10

        Item {
            width: parent.width
            height: 28

            Image {
                id: brandMark
                width: 40
                height: 22
                anchors.verticalCenter: parent.verticalCenter
                x: 2
                source: "qrc:/qt/qml/OpenNOW/res/brand/opennow-mark.png"
                fillMode: Image.PreserveAspectFit
                smooth: false
                sourceSize: Qt.size(Math.ceil(width * Screen.devicePixelRatio), Math.ceil(height * Screen.devicePixelRatio))
            }

            Text {
                visible: root.reveal > 0
                        opacity: root.reveal
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
                visible: false
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
            width: parent.width
            height: 28
            Rectangle {
                id: expandButton
                width: 40
                height: 28
                x: 2
                radius: 9
                color: expandHover.hovered || expandButton.activeFocus ? "#17FFFFFF" : "#0FFFFFFF"
                border.width: 1
                border.color: "#17FFFFFF"
                Accessible.role: Accessible.Button
                Accessible.name: root.overlayOpen ? qsTr("Collapse sidebar") : qsTr("Expand sidebar")
                Accessible.onPressAction: {
                    const closing = root.overlayOpen
                    root.hoverExpanded = false
                    root.collapseRequested(closing)
                }

                DesktopGlyph {
                    anchors.centerIn: parent
                    width: 13
                    height: 13
                    icon: root.overlayOpen ? "desktop-collapse.svg" : "desktop-expand.svg"
                }
                HoverHandler { id: expandHover; cursorShape: Qt.PointingHandCursor }
                TapHandler {
                    onTapped: {
                        const closing = root.overlayOpen
                        root.hoverExpanded = false
                        root.collapseRequested(closing)
                    }
                }
            }
        }

        Item {
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
                    height: 44
                    padding: 0
                    readonly property bool selected: root.routeSelected(modelData.route)
                    Accessible.name: modelData.name
                    background: Rectangle {
                        radius: 10
                        color: navButton.selected ? DesktopTokens.raisedStrong
                            : (navButton.hovered || navButton.activeFocus ? DesktopTokens.raised : "transparent")
                    }
                    contentItem: Item {
                        DesktopGlyph {
                            objectName: "sidebarIcon-" + navButton.modelData.route
                            x: 13
                            anchors.verticalCenter: parent.verticalCenter
                            width: 18
                            height: 18
                            icon: navButton.modelData.icon
                            active: navButton.selected
                        }
                        Text {
                            x: 48
                            anchors.verticalCenter: parent.verticalCenter
                            visible: root.reveal > 0
                        opacity: root.reveal
                            text: navButton.modelData.name
                            color: navButton.selected ? DesktopTokens.text : DesktopTokens.textMuted
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
                    onClicked: {
                        if (modelData.route === "library")
                            ShellStore.activeCollectionId = ""
                        root.routeRequested(modelData.route)
                    }
                }
            }
        }

        Item {
            width: parent.width
            height: 44 + collectionRows.height
            Item {
                width: DesktopTokens.railWidth - 28
                height: 36
                Rectangle {
                    x: 8; width: 28; height: 1
                    anchors.verticalCenter: parent.verticalCenter
                    color: DesktopTokens.seamSoft
                }
                Text {
                    x: 48
                    anchors.verticalCenter: parent.verticalCenter
                    visible: root.reveal > 0
                    opacity: root.reveal
                    text: qsTr("COLLECTIONS")
                    color: DesktopTokens.textFaint
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: 9
                    font.weight: Font.DemiBold
                    font.letterSpacing: 0.9
                }
                Button {
                    id: createCollectionButton
                    objectName: "createCollectionButton"
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    width: 32
                    height: 32
                    visible: root.reveal > 0
                    opacity: root.reveal
                    enabled: root.overlayOpen && !ShellStore.collectionsBusy
                    Accessible.name: qsTr("New collection")
                    ToolTip.visible: hovered
                    ToolTip.text: qsTr("New collection")
                    background: Rectangle { radius: 8; color: createCollectionButton.hovered || createCollectionButton.activeFocus ? DesktopTokens.raised : "transparent" }
                    contentItem: DesktopGlyph {
                        width: createCollectionButton.availableWidth
                        height: createCollectionButton.availableHeight
                        icon: "desktop-plus.svg"
                    }
                    onClicked: root.createCollectionRequested()
                }
            }

            Column {
                id: collectionRows
                y: 40
                width: parent.width
                height: implicitHeight * root.reveal
                clip: true
                spacing: 1
            Repeater {
                model: ShellStore.gameCollections
                delegate: Button {
                    id: collectionRow
                    required property var modelData
                    width: parent.width
                    height: 36
                    opacity: root.reveal
                    enabled: root.overlayOpen
                    clip: true
                    Accessible.name: modelData.name
                    background: Rectangle {
                        radius: 8
                        color: ShellStore.activeCollectionId === collectionRow.modelData.id && root.currentRoute === "library"
                            ? DesktopTokens.raisedStrong : collectionRow.hovered || collectionRow.activeFocus ? DesktopTokens.raised : "transparent"
                    }
                    contentItem: Item {
                    width: DesktopTokens.railWidth - 28
                    height: 36
                    DesktopGlyph {
                        objectName: "sidebarCollectionIcon-" + collectionRow.modelData.id
                        x: 9
                        anchors.verticalCenter: parent.verticalCenter
                        width: 16
                        height: width
                        icon: "desktop-nav-library.svg"
                    }
                    Text {
                        x: 42
                        width: parent.width - x - 44
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.reveal > 0
                        opacity: root.reveal
                        text: collectionRow.modelData.name
                        color: collectionRow.hovered ? DesktopTokens.text : DesktopTokens.textMuted
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 13
                        font.weight: Font.DemiBold
                    }
                    Text {
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.reveal > 0
                        opacity: root.reveal
                        text: collectionRow.modelData.gameIds.length
                        color: DesktopTokens.textFaint
                        font.family: DesktopTokens.monoFont
                        font.pixelSize: 10
                        font.weight: Font.DemiBold
                    }
                    }
                    onClicked: {
                        ShellStore.activeCollectionId = modelData.id
                        root.routeRequested("library")
                        root.closeOverlay()
                    }
                }
            }
                Text {
                    x: 8
                    width: parent.width - 16
                    visible: ShellStore.gameCollections.length === 0
                    text: qsTr("Create your first collection with +")
                    wrapMode: Text.WordWrap
                    color: DesktopTokens.textFaint
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 12
                }
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
                        x: 13.5
                        anchors.verticalCenter: parent.verticalCenter
                        width: 17
                        height: 17
                        icon: "desktop-gamepad.svg"
                    }
                    Column {
                        x: 48
                        width: Math.max(0, parent.width - x - 52)
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.reveal > 0
                        opacity: root.reveal
                        spacing: 2
                        Text {
                            width: parent.width
                            elide: Text.ElideRight
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
                        visible: root.reveal > 0
                        opacity: root.reveal
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
                        x: 4
                        anchors.verticalCenter: parent.verticalCenter
                        width: 36
                        height: 36
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
                        width: Math.max(0, parent.width - x - 28)
                        anchors.verticalCenter: parent.verticalCenter
                        visible: root.reveal > 0
                        opacity: root.reveal
                        spacing: 2
                        Text {
                            width: parent.width
                            elide: Text.ElideRight
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
                            width: parent.width
                            elide: Text.ElideRight
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
                        visible: root.reveal > 0
                        opacity: root.reveal
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
