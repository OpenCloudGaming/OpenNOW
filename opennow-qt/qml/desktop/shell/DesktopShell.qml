import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    default property alias contentData: contentHost.data
    property string route: "home"
    readonly property bool settingsPage: route.indexOf("settings") === 0
    readonly property int headerHeight: settingsPage ? DesktopTokens.px(60) : DesktopTokens.topBarHeight
    readonly property int footerHeight: DesktopTokens.px(40)
    property string title: qsTr("Home")
    property string subtitle: qsTr("Your library")
    property bool searchVisible: route !== "settings" && route.indexOf("settings-") !== 0 && route !== "friends"
    property string searchText: ""
    readonly property bool railCollapsed: ShellStore.settings.desktopRailCollapsed !== false
    // A pinned sidebar reserves space; only transient hover expansion overlays.
    readonly property int contentInset: railCollapsed ? DesktopTokens.railCollapsedWidth : DesktopTokens.railWidth
    signal routeRequested(string route)
    signal consoleModeRequested()
    signal commandPaletteRequested()

    anchors.fill: parent
    focus: true

    function persistRailCollapsed(collapsed) {
        ShellStore.applySetting("desktopRailCollapsed", collapsed)
        ShellStore.setSetting("desktopRailCollapsed", collapsed)
    }

    function regionStatusText() {
        const selected = String(ShellStore.settings.region || "")
        if (selected === "")
            return qsTr("AUTO REGION")
        const regions = ShellStore.regions || []
        for (let i = 0; i < regions.length; ++i) {
            if (regions[i].name === selected || regions[i].url === selected) {
                const ping = ShellStore.regionPingResults ? ShellStore.regionPingResults[regions[i].url] : undefined
                const name = String(regions[i].name || selected)
                if (ping === undefined || ping === null || ping === "")
                    return name.toUpperCase()
                return name.toUpperCase() + " · " + ping + " ms"
            }
        }
        return selected.toUpperCase()
    }

    function activeSessionPrompt() {
        const session = ShellStore.resumableSession
        if (!session)
            return ""
        const title = ShellStore.sessionGameTitle(session)
        return title
            ? qsTr("Active session: %1 · Resume?").arg(title)
            : qsTr("Active session running · Resume?")
    }

    DesktopBackdrop { anchors.fill: parent }

    Item {
        id: main
        x: root.contentInset
        width: root.width - root.contentInset
        height: root.height

        Rectangle {
            id: header
            width: parent.width; height: root.headerHeight
            readonly property real availableWidth: Math.max(0, width - DesktopTokens.px(48))
            color: DesktopTokens.topBar
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: DesktopTokens.seam }
            Item {
                id: heading
                objectName: "desktopHeaderHeading"
                x: DesktopTokens.px(24); anchors.verticalCenter: parent.verticalCenter
                width: Math.max(0, (search.visible ? search.x : activeSessionButton.visible ? activeSessionButton.x : parent.width - DesktopTokens.px(14)) - x - DesktopTokens.px(10))
                height: headerTitle.implicitHeight
                Text {
                    id: headerTitle
                    width: Math.min(implicitWidth, parent.width)
                    text: root.title; elide: Text.ElideRight
                    color: DesktopTokens.text; font.family: DesktopTokens.displayFont
                    font.pixelSize: root.settingsPage ? DesktopTokens.px(22) : DesktopTokens.titleSize
                    font.weight: Font.Black; font.letterSpacing: -0.4
                }
                Text {
                    x: headerTitle.width + DesktopTokens.px(10)
                    width: Math.max(0, parent.width - x)
                    visible: width >= DesktopTokens.px(90)
                    anchors.baseline: headerTitle.baseline
                    text: root.subtitle; elide: Text.ElideRight
                    color: DesktopTokens.textMuted; font.family: DesktopTokens.monoFont
                    font.pixelSize: DesktopTokens.captionSize; font.weight: Font.DemiBold; font.letterSpacing: 0.45
                }
            }
            TextField {
                id: search
                objectName: "desktopHeaderSearch"
                visible: root.searchVisible
                anchors.right: activeSessionButton.visible ? activeSessionButton.left : parent.right
                anchors.rightMargin: activeSessionButton.visible ? DesktopTokens.px(10) : DesktopTokens.px(24)
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(DesktopTokens.px(300), header.availableWidth * (activeSessionButton.visible ? 0.34 : 0.48))
                height: DesktopTokens.controlHeight
                leftPadding: DesktopTokens.px(34); rightPadding: DesktopTokens.px(34); topPadding: 0; bottomPadding: 0
                color: DesktopTokens.textBody
                placeholderText: root.route === "friends" ? qsTr("Search friends") : root.route === "store" ? qsTr("Search the store") : qsTr("Search your library")
                placeholderTextColor: DesktopTokens.textMuted
                font.family: DesktopTokens.bodyFont; font.pixelSize: DesktopTokens.bodySize; font.weight: Font.DemiBold
                text: root.searchText
                selectByMouse: true
                background: Rectangle { radius: DesktopTokens.px(10); color: Theme.lightMode ? DesktopTokens.raised : "#59000000"; border.width: 1; border.color: DesktopTokens.seam }
                onTextChanged: root.searchText = text
                DesktopGlyph { x: DesktopTokens.px(11); anchors.verticalCenter: parent.verticalCenter; width: DesktopTokens.px(14); height: DesktopTokens.px(14); icon: "desktop-search.svg" }
                KeyboardGlyph { anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(8); anchors.verticalCenter: parent.verticalCenter; shortcut: "/"; keySize: DesktopTokens.px(20); ink: DesktopTokens.textBody }
            }
            DesktopButton {
                id: activeSessionButton
                objectName: "desktopHeaderResume"
                visible: ShellStore.resumableSession !== null && root.route !== "stream"
                anchors.right: parent.right
                anchors.rightMargin: DesktopTokens.px(24)
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(DesktopTokens.px(260), header.availableWidth * (search.visible ? 0.34 : 0.48))
                height: DesktopTokens.controlHeight
                primary: true
                glyph: "desktop-play.svg"
                glyphSize: DesktopTokens.px(11)
                font.pixelSize: DesktopTokens.smallSize
                text: root.activeSessionPrompt()
                ToolTip.visible: hovered
                ToolTip.text: text
                ToolTip.delay: 700
                contentItem: Item {
                    DesktopGlyph {
                        anchors.left: parent.left; anchors.verticalCenter: parent.verticalCenter
                        width: activeSessionButton.glyphSize; height: width
                        icon: "desktop-play.svg"
                    }
                    Text {
                        x: activeSessionButton.glyphSize + DesktopTokens.px(8)
                        width: Math.max(0, parent.width - x)
                        anchors.verticalCenter: parent.verticalCenter
                        text: activeSessionButton.text
                        elide: Text.ElideRight
                        color: "#0A0D14"
                        font: activeSessionButton.font
                    }
                }
                onClicked: ShellStore.resumeActiveSession()
            }
        }

        Item {
            id: contentHost
            x: 0; y: root.headerHeight
            width: parent.width
            height: parent.height - root.headerHeight - root.footerHeight
            clip: true
        }

        Rectangle {
            id: footer
            anchors.bottom: parent.bottom
            width: parent.width
            height: root.footerHeight
            color: DesktopTokens.statusBar
            clip: true
            readonly property bool compactHints: true
            Rectangle { width: parent.width; height: 1; color: DesktopTokens.seam }
            Row { id: shortcutHints; x: DesktopTokens.px(24); anchors.verticalCenter: parent.verticalCenter; spacing: DesktopTokens.px(footer.compactHints ? 8 : 16)
                DesktopKeyHint { compact: footer.compactHints; keyText: qsTr("Arrows"); shortcut: "Arrows"; label: qsTr("Move") }
                DesktopKeyHint { compact: footer.compactHints; keyText: qsTr("Enter"); shortcut: "Enter"; label: qsTr("Play") }
                DesktopKeyHint { compact: footer.compactHints; keyText: "/"; label: qsTr("Search") }
                DesktopKeyHint { compact: footer.compactHints; keyText: qsTr("Ctrl K"); shortcut: "Ctrl K"; label: qsTr("Commands") }
                DesktopKeyHint { compact: footer.compactHints; keyText: "?"; label: qsTr("All shortcuts") }
            }
            Row { anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(24); anchors.verticalCenter: parent.verticalCenter; spacing: DesktopTokens.px(10)
                visible: x >= shortcutHints.x + shortcutHints.width + DesktopTokens.px(16)
                Rectangle { width: 8; height: 8; radius: 4; color: DesktopTokens.green }
                Text { text: root.regionStatusText(); color: DesktopTokens.textBody; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.px(11); font.weight: Font.DemiBold; font.letterSpacing: 0.4 }
                Rectangle { width: 1; height: DesktopTokens.px(16); color: DesktopTokens.seam }
                Text { text: String(ShellStore.settings.themePack || "nocturne").toUpperCase() + qsTr(" THEME"); color: DesktopTokens.textMuted; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.px(11); font.weight: Font.DemiBold; font.letterSpacing: 0.4 }
            }
        }
    }

    Rectangle {
        id: railScrim
        x: root.contentInset
        width: root.width - root.contentInset
        height: root.height
        z: 20
        color: "transparent"
        visible: sidebar.overlayOpen
        TapHandler { onTapped: sidebar.closeOverlay() }
    }

    DesktopSidebar {
        id: sidebar
        x: 0
        z: sidebar.overlayOpen ? 40 : 3
        currentRoute: root.route
        collapsed: root.railCollapsed
        onRouteRequested: route => root.routeRequested(route)
        onConsoleModeRequested: root.consoleModeRequested()
        onCollapseRequested: collapsed => root.persistRailCollapsed(collapsed)
        onCreateCollectionRequested: {
            sidebar.closeOverlay()
            collectionDialog.open()
        }
    }

    DesktopCollectionDialog {
        id: collectionDialog
        onCollectionOpened: collectionId => {
            ShellStore.activeCollectionId = collectionId
            root.routeRequested("library")
        }
    }

    Keys.onPressed: event => {
        if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_K) {
            root.commandPaletteRequested()
            event.accepted = true
        } else if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_B) {
            root.persistRailCollapsed(!root.railCollapsed)
            event.accepted = true
        } else if ((event.modifiers & Qt.ControlModifier) && event.key === Qt.Key_Comma) {
            root.routeRequested("settings")
            event.accepted = true
        } else if (event.key === Qt.Key_Slash && root.searchVisible) {
            search.forceActiveFocus()
            event.accepted = true
        }
    }
}
