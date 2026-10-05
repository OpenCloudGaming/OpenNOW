import QtQuick
import OpenNOW

Item {
    id: root
    property string title: qsTr("My games")
    property string currentRoute: "home"
    property bool bottomVisible: true
    property bool navVisible: true
    property var leftHints: []
    property var rightHints: []
    property real entranceProgress: 1
    property date now: new Date()
    readonly property var profile: ShellStore.authSession && ShellStore.authSession.user
        ? ShellStore.authSession.user : null
    readonly property string displayName: profile && profile.displayName
        ? String(profile.displayName) : qsTr("Guest")
    readonly property string profileInitial: displayName.length > 0
        ? displayName.slice(0, 1).toUpperCase() : "O"
    readonly property string membershipTier: {
        const subscription = ShellStore.subscription || ({})
        const tier = subscription.membershipTier || (profile && profile.membershipTier) || ""
        return String(tier).toUpperCase()
    }
    readonly property var primaryController: ControllerInput.controllers.length > 0
        ? ControllerInput.controllers[0] : null
    readonly property bool batteryKnown: primaryController !== null
        && Number(primaryController.batteryPercent) >= 0
    signal routeRequested(string route)

    function regionPing() {
        const name = regionName()
        for (let index = 0; index < ShellStore.regions.length; ++index) {
            const region = ShellStore.regions[index]
            if (name && region.name === name) {
                const ping = ShellStore.regionPingResults[region.url]
                return ping === undefined ? null : ping
            }
        }
        return null
    }

    function regionName() {
        const session = ShellStore.activeSession || ({})
        return String(ShellStore.selectedRegion || "") || String(session.zone || session.serverLocation || "")
    }

    function regionStatus() {
        const name = regionName()
        const ping = regionPing()
        if (name && ping !== null)
            return qsTr("%1 · %2 ms").arg(name).arg(ping)
        return name || qsTr("Automatic region")
    }

    Timer {
        interval: 30000
        repeat: true
        running: root.visible
        onTriggered: root.now = new Date()
    }

    GlassPanel {
        id: profilePanel
        opacity: root.entranceProgress
        transform: Translate { y: -16 * (1 - root.entranceProgress) }
        x: 64; y: 36
        width: Math.min(460, profileRow.implicitWidth + 40); height: 60
        panelRadius: 30
        strong: true
        Accessible.role: Accessible.StaticText
        Accessible.name: root.membershipTier !== "" ? root.displayName + ", " + root.membershipTier : root.displayName
        Row {
            id: profileRow
            x: 10
            anchors.verticalCenter: parent.verticalCenter
            spacing: 12
            Rectangle {
                width: 40; height: 40; radius: 20
                color: Theme.violet
                border.color: Qt.rgba(1, 1, 1, 0.7); border.width: 2
                Text {
                    anchors.centerIn: parent
                    text: root.profileInitial
                    color: Theme.contrastText(Theme.violet)
                    font.family: Theme.displayFont
                    font.pixelSize: 17
                    font.weight: Font.Black
                }
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
                width: Math.min(200, implicitWidth)
                text: root.displayName
                color: Theme.label
                font.family: Theme.bodyFont
                font.pixelSize: 19
                font.weight: Font.Bold
                elide: Text.ElideRight
            }
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                readonly property string badge: root.membershipTier !== "" ? root.membershipTier
                    : ShellStore.signedIn ? "" : qsTr("SIGNED OUT")
                visible: badge !== ""
                width: tierText.implicitWidth + 20; height: 26; radius: 13
                color: Qt.rgba(Theme.violet.r, Theme.violet.g, Theme.violet.b, 0.18)
                Text {
                    id: tierText
                    anchors.centerIn: parent
                    text: parent.badge
                    color: Theme.lightMode ? Qt.darker(Theme.violet, 1.6) : Theme.violet
                    font.family: Theme.monoFont
                    font.pixelSize: 13
                    font.weight: Font.Bold
                    font.letterSpacing: 1.2
                }
            }
        }
    }

    Text {
        id: titleText
        opacity: root.entranceProgress
        transform: Translate { y: -16 * (1 - root.entranceProgress) }
        x: Math.round((parent.width - width) / 2)
        y: 66 - Math.round(height / 2)
        width: Math.min(implicitWidth, statusPanel.x - profilePanel.x - profilePanel.width - 80)
        visible: text !== ""
        horizontalAlignment: Text.AlignHCenter
        elide: Text.ElideRight
        text: root.title
        color: Theme.label
        font.family: Theme.displayFont
        font.pixelSize: 24
        font.weight: Font.Black
        Accessible.role: Accessible.Heading
        Accessible.name: text
    }

    GlassPanel {
        id: statusPanel
        objectName: "consoleStatusPanel"
        opacity: root.entranceProgress
        transform: Translate { y: -16 * (1 - root.entranceProgress) }
        x: parent.width - width - 64; y: 36
        width: statusRow.implicitWidth + 48; height: 60
        panelRadius: 30
        strong: true
        Row {
            id: statusRow
            anchors.centerIn: parent
            anchors.alignWhenCentered: false
            spacing: 18
            Row {
                spacing: 9; anchors.verticalCenter: parent.verticalCenter
                Rectangle {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 8; height: 8; radius: 4
                    color: root.regionPing() !== null ? Theme.mint : Theme.textMuted
                }
                Text {
                    text: root.regionStatus()
                    color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 16; font.weight: Font.Bold
                }
            }
            Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 1; height: 22; color: Theme.seam }
            Text {
                objectName: "consoleClock"
                anchors.verticalCenter: parent.verticalCenter
                anchors.alignWhenCentered: false
                text: Qt.formatDateTime(root.now, "hh:mm")
                color: Theme.label
                font.family: Theme.monoFont
                font.pixelSize: 16
                font.weight: Font.Bold
            }
            Rectangle {
                visible: root.batteryKnown
                anchors.verticalCenter: parent.verticalCenter; width: 1; height: 22; color: Theme.seam
            }
            Row {
                visible: root.batteryKnown
                spacing: 8; anchors.verticalCenter: parent.verticalCenter
                Accessible.role: Accessible.StaticText
                Accessible.name: qsTr("Controller battery %1%").arg(root.batteryKnown ? root.primaryController.batteryPercent : 0)
                Item {
                    width: 24; height: 14; anchors.verticalCenter: parent.verticalCenter
                    Rectangle {
                        x: 0; y: 0; width: 21; height: 14; radius: 3; color: "transparent"
                        border.color: Theme.label; border.width: 2
                        Rectangle {
                            x: 3; y: 3; height: 8; radius: 1
                            width: Math.max(2, 15 * Math.min(100, Number(root.batteryKnown ? root.primaryController.batteryPercent : 0)) / 100)
                            color: Number(root.batteryKnown ? root.primaryController.batteryPercent : 0) <= 15 ? Theme.coral : Theme.mint
                        }
                    }
                    Rectangle { x: 22; y: 4; width: 2; height: 6; radius: 1; color: Theme.label }
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("%1%").arg(root.batteryKnown ? root.primaryController.batteryPercent : 0)
                    color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 16; font.weight: Font.Bold
                }
            }
        }
    }

    Row {
        id: leftHintRow
        opacity: root.entranceProgress
        transform: Translate { y: 16 * (1 - root.entranceProgress) }
        visible: root.bottomVisible && root.leftHints.length > 0
        x: 64
        y: parent.height - 97 - Math.round(height / 2)
        spacing: 28
        Accessible.ignored: true
        Repeater {
            model: root.leftHints
            ControllerGlyph {
                required property var modelData
                glyph: modelData.glyph
                keyboard: Boolean(modelData.keyboard)
                label: modelData.label
                glyphSize: 30
            }
        }
    }

    NavPill {
        opacity: root.entranceProgress
        transform: Translate { y: 16 * (1 - root.entranceProgress) }
        visible: root.bottomVisible && root.navVisible
        anchors.horizontalCenter: parent.horizontalCenter
        y: parent.height - height - 60
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

    Row {
        id: rightHintRow
        opacity: root.entranceProgress
        transform: Translate { y: 16 * (1 - root.entranceProgress) }
        visible: root.bottomVisible && root.rightHints.length > 0
        x: parent.width - width - 64
        y: parent.height - 97 - Math.round(height / 2)
        spacing: 28
        Accessible.ignored: true
        Repeater {
            model: root.rightHints
            ControllerGlyph {
                required property var modelData
                glyph: modelData.glyph
                keyboard: Boolean(modelData.keyboard)
                label: modelData.label
                glyphSize: 30
            }
        }
    }
}
