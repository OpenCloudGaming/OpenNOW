import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    default property alias contentData: contentHost.data
    property string route: "home"
    property string title: qsTr("Home")
    property string subtitle: qsTr("Your library")
    property bool searchVisible: route !== "settings" && route.indexOf("settings-") !== 0 && route !== "friends"
    property string searchText: ""
    readonly property bool railCollapsed: ShellStore.settings.desktopRailCollapsed !== false
    readonly property int contentInset: DesktopTokens.railCollapsedWidth
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
            width: parent.width; height: DesktopTokens.topBarHeight
            color: DesktopTokens.topBar
            Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: DesktopTokens.seam }
            Row {
                x: DesktopTokens.px(24); anchors.verticalCenter: parent.verticalCenter; spacing: DesktopTokens.px(10)
                Text { text: root.title; color: DesktopTokens.text; font.family: DesktopTokens.displayFont; font.pixelSize: DesktopTokens.titleSize; font.weight: Font.Black; font.letterSpacing: -0.4 }
                Text { anchors.baseline: parent.children[0].baseline; text: root.subtitle; color: DesktopTokens.textMuted; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.DemiBold; font.letterSpacing: 0.45 }
            }
            TextField {
                id: search
                visible: root.searchVisible
                anchors.right: activeSessionButton.visible ? activeSessionButton.left : parent.right
                anchors.rightMargin: activeSessionButton.visible ? DesktopTokens.px(10) : DesktopTokens.px(24)
                anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(300); height: DesktopTokens.controlHeight
                leftPadding: DesktopTokens.px(34); rightPadding: DesktopTokens.px(34); topPadding: 0; bottomPadding: 0
                color: DesktopTokens.textBody
                placeholderText: root.route === "friends" ? qsTr("Search friends") : root.route === "store" ? qsTr("Search the store") : qsTr("Search your library")
                placeholderTextColor: "#66FFFFFF"
                font.family: DesktopTokens.bodyFont; font.pixelSize: DesktopTokens.bodySize; font.weight: Font.DemiBold
                text: root.searchText
                selectByMouse: true
                background: Rectangle { radius: DesktopTokens.px(10); color: "#59000000"; border.width: 1; border.color: DesktopTokens.seam }
                onTextChanged: root.searchText = text
                DesktopGlyph { x: DesktopTokens.px(11); anchors.verticalCenter: parent.verticalCenter; width: DesktopTokens.px(14); height: DesktopTokens.px(14); icon: "desktop-search.svg" }
                Rectangle { anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(8); anchors.verticalCenter: parent.verticalCenter; width: DesktopTokens.px(20); height: DesktopTokens.px(20); radius: DesktopTokens.px(6); color: "#1AFFFFFF"; border.width: 1; border.color: DesktopTokens.seam; Text { anchors.centerIn: parent; text: "/"; color: DesktopTokens.textBody; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.monoSize } }
            }
            DesktopButton {
                id: activeSessionButton
                visible: ShellStore.resumableSession !== null && root.route !== "stream"
                anchors.right: parent.right
                anchors.rightMargin: DesktopTokens.px(24)
                anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(260)
                height: DesktopTokens.controlHeight
                primary: true
                glyph: "desktop-play.svg"
                glyphSize: DesktopTokens.px(11)
                text: root.activeSessionPrompt()
                onClicked: ShellStore.resumeActiveSession()
            }
        }

        Item {
            id: contentHost
            x: 0; y: DesktopTokens.topBarHeight
            width: parent.width
            height: parent.height - DesktopTokens.topBarHeight - DesktopTokens.statusBarHeight
            clip: true
        }

        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: DesktopTokens.statusBarHeight
            color: DesktopTokens.statusBar
            Rectangle { width: parent.width; height: 1; color: DesktopTokens.seam }
            Row { x: DesktopTokens.px(24); anchors.verticalCenter: parent.verticalCenter; spacing: DesktopTokens.px(16)
                DesktopKeyHint { keyText: qsTr("Arrows"); label: qsTr("Move") }
                DesktopKeyHint { keyText: qsTr("Enter"); label: qsTr("Play") }
                DesktopKeyHint { keyText: "/"; label: qsTr("Search") }
                DesktopKeyHint { keyText: qsTr("Ctrl K"); label: qsTr("Commands") }
                DesktopKeyHint { keyText: "?"; label: qsTr("All shortcuts") }
            }
            Row { anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(24); anchors.verticalCenter: parent.verticalCenter; spacing: DesktopTokens.px(10)
                Rectangle { width: 8; height: 8; radius: 4; color: DesktopTokens.green }
                Text { text: root.regionStatusText(); color: DesktopTokens.textBody; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.monoSize; font.weight: Font.DemiBold; font.letterSpacing: 0.4 }
                Rectangle { width: 1; height: DesktopTokens.px(16); color: DesktopTokens.seam }
                Text { text: String(ShellStore.settings.themePack || "nocturne").toUpperCase() + qsTr(" THEME"); color: DesktopTokens.textMuted; font.family: DesktopTokens.monoFont; font.pixelSize: DesktopTokens.monoSize; font.weight: Font.DemiBold; font.letterSpacing: 0.4 }
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
