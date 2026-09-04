import QtQuick
import OpenNOW

Item {
    id: root
    property string title: qsTr("My games")
    property string currentRoute: "home"
    property bool bottomVisible: true
    property var leftHints: [
        { glyph: "Y", label: qsTr("Search") },
        { glyph: "−", label: qsTr("Details") }
    ]
    property var rightHints: [
        { glyph: "A", label: qsTr("Play") },
        { glyph: "+", label: qsTr("Menu") }
    ]
    property date now: new Date()
    readonly property var profile: ShellStore.authSession && ShellStore.authSession.user
        ? ShellStore.authSession.user : null
    readonly property string displayName: profile && profile.displayName
        ? String(profile.displayName) : qsTr("Guest")
    readonly property string profileInitial: displayName.length > 0
        ? displayName.slice(0, 1).toUpperCase() : "O"
    readonly property var primaryController: ControllerInput.controllers.length > 0
        ? ControllerInput.controllers[0] : null
    signal routeRequested(string route)

    function regionStatus() {
        const selected = String(ShellStore.settings.region || "")
        const session = ShellStore.activeSession || ({})
        const name = selected || String(session.zone || session.serverLocation || "")
        let ping = null
        for (let index = 0; index < ShellStore.regions.length; ++index) {
            const region = ShellStore.regions[index]
            if (name && region.name === name) {
                ping = ShellStore.regionPingResults[region.url]
                break
            }
        }
        if (name && ping !== null && ping !== undefined)
            return qsTr("%1 · %2 ms").arg(name).arg(ping)
        return name || qsTr("Automatic region")
    }

    function controllerStatus() {
        if (root.primaryController && Number(root.primaryController.batteryPercent) >= 0)
            return qsTr("%1%").arg(root.primaryController.batteryPercent)
        return qsTr("No pad")
    }

    Timer {
        interval: 30000
        repeat: true
        running: root.visible
        onTriggered: root.now = new Date()
    }

    GlassPanel {
        id: profilePanel
        x: 40; y: 28
        width: Math.max(273, Math.min(430, profileRow.implicitWidth + 28)); height: 56
        panelRadius: 28
        strong: true
        Row {
            id: profileRow
            anchors.centerIn: parent
            spacing: 12
            Rectangle {
                width: 40; height: 40; radius: 20
                color: Theme.violet
                border.color: Theme.face; border.width: 2
                Text {
                    anchors.centerIn: parent
                    text: root.profileInitial
                    color: Theme.contrastText(Theme.violet)
                    font.family: Theme.displayFont
                    font.pixelSize: 16
                    font.weight: Font.Black
                }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(180, implicitWidth)
                text: root.displayName
                color: Theme.label
                font.family: Theme.bodyFont
                font.pixelSize: 16
                font.weight: Font.Bold
                elide: Text.ElideRight
            }
            Rectangle { width: 1; height: 20; color: Theme.seam }
            Row {
                anchors.verticalCenter: parent.verticalCenter
                spacing: 6
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 8; height: 8; radius: 4; color: ShellStore.signedIn ? Theme.mint : Theme.textMuted }
                Text {
                    text: ShellStore.signedIn ? qsTr("Connected") : qsTr("Offline")
                    color: Theme.label
                    font.family: Theme.bodyFont
                    font.pixelSize: 16
                    font.weight: Font.Bold
                }
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 29; height: 24; radius: 7; color: Theme.face
                Text { anchors.centerIn: parent; text: "LT"; color: Theme.faceText; font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Black }
            }
        }
    }

    GlassPanel {
        id: titlePanel
        x: Math.round((profilePanel.x + profilePanel.width + statusPanel.x - width) / 2)
        y: 28
        width: Math.max(156, titleText.implicitWidth + 98)
        height: 56
        panelRadius: 28
        strong: true
        Text {
            id: titleText
            anchors.centerIn: parent
            width: parent.width - 36
            horizontalAlignment: Text.AlignHCenter
            elide: Text.ElideRight
            text: root.title
            color: Theme.label
            font.family: Theme.displayFont
            font.pixelSize: 20
            font.weight: Font.Black
        }
    }

    GlassPanel {
        id: statusPanel
        x: parent.width - width - 40; y: 28
        width: Math.max(446, statusRow.implicitWidth + 40); height: 56
        panelRadius: 28
        strong: true
        Row {
            id: statusRow
            anchors.centerIn: parent
            spacing: 16
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 31; height: 24; radius: 7; color: Theme.face
                Text { anchors.centerIn: parent; text: "RT"; color: Theme.faceText; font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Black }
            }
            Row {
                spacing: 6; anchors.verticalCenter: parent.verticalCenter
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 8; height: 8; radius: 4; color: Theme.mint }
                Text {
                    text: root.regionStatus()
                    color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 16; font.weight: Font.Bold
                }
            }
            Rectangle { width: 1; height: 20; color: Theme.seam }
            Text {
                text: Qt.formatDateTime(root.now, "hh:mm | MM/dd")
                color: Theme.label
                font.family: Theme.bodyFont
                font.pixelSize: 16
                font.weight: Font.Bold
            }
            Rectangle { width: 1; height: 20; color: Theme.seam }
            Row {
                spacing: 6; anchors.verticalCenter: parent.verticalCenter
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: root.controllerStatus()
                    color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 16; font.weight: Font.Black
                }
                Item {
                    visible: root.primaryController && Number(root.primaryController.batteryPercent) >= 0
                    width: 26; height: 14; anchors.verticalCenter: parent.verticalCenter
                    Rectangle { x: 0; y: 1; width: 22; height: 12; radius: 3; color: "transparent"; border.color: Theme.label; border.width: 2
                        Rectangle { x: 3; y: 3; width: Math.max(2, 14 * Math.min(100, Number(root.primaryController ? root.primaryController.batteryPercent : 0)) / 100); height: 6; radius: 1; color: Theme.label }
                    }
                    Rectangle { x: 23; y: 4; width: 3; height: 6; radius: 1; color: Theme.label }
                }
            }
        }
    }

    HintBar {
        visible: root.bottomVisible
        x: 40
        y: parent.height - height - 14
        hints: root.leftHints
    }

    NavPill {
        visible: root.bottomVisible
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height - height - 26
        currentRoute: root.currentRoute
        onRouteRequested: route => {
            if (route === "friends")
                AppController.showOverlay("friends")
            else if (route === "computer")
                ShellStore.requestConsoleSurface(false)
            else
                root.routeRequested(route)
        }
    }

    HintBar {
        visible: root.bottomVisible
        x: parent.width - width - 40
        y: parent.height - height - 14
        hints: root.rightHints
    }
}
