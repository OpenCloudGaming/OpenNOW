import QtQuick
import OpenNOW

FocusScope {
    id: root
    readonly property var capabilities: ShellStore.socialCapabilities || ({})
    readonly property var controllers: ControllerInput.controllers || []
    readonly property bool playerTwoReady: AppController.controllerCount >= 2
    focus: visible
    Accessible.name: qsTr("Friends and local co-op")
    Accessible.role: Accessible.Pane
    onVisibleChanged: if (visible) Qt.callLater(() => localJoin.forceActiveFocus())
    Component.onCompleted: if (visible) Qt.callLater(() => localJoin.forceActiveFocus())

    Keys.onPressed: event => {
        if (event.key === Qt.Key_PageUp || event.key === Qt.Key_PageDown) {
            event.accepted = AppController.cyclePrimaryRoute(event.key === Qt.Key_PageUp ? -1 : 1)
        }
    }

    ScreenBackground { tint: "#1F1A3A" }

    Column {
        id: heroColumn
        x: 120; y: 200
        width: 880
        spacing: 22

        Rectangle {
            width: soonRow.implicitWidth + 34
            height: 40
            radius: 20
            color: Qt.rgba(Theme.violet.r, Theme.violet.g, Theme.violet.b, 0.16)
            border.color: Qt.rgba(Theme.violet.r, Theme.violet.g, Theme.violet.b, 0.5)
            border.width: 1
            Row {
                id: soonRow
                anchors.centerIn: parent
                spacing: 10
                Rectangle { anchors.verticalCenter: parent.verticalCenter; width: 8; height: 8; radius: 4; color: Theme.violet }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("COMING SOON")
                    color: Theme.lightMode ? Qt.darker(Theme.violet, 1.6) : Theme.violet
                    font.family: Theme.monoFont
                    font.pixelSize: 14
                    font.weight: Font.Bold
                    font.letterSpacing: 1.96
                }
            }
        }
        Item {
            width: parent.width
            height: Math.max(1, heroTitle.lineCount) * 88
            Text {
                id: heroTitle
                y: -13
                width: parent.width
                text: qsTr("Friends are on the way")
                wrapMode: Text.WordWrap
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 84
                font.weight: Font.Black
                font.letterSpacing: -2.52
                lineHeightMode: Text.FixedHeight
                lineHeight: 88
                Accessible.role: Accessible.Heading
                Accessible.name: text
            }
        }
        Item {
            width: 760
            height: Math.max(1, heroBody.lineCount) * 33
            Text {
                id: heroBody
                y: 1
                width: parent.width
                text: root.capabilities.reason
                    || qsTr("NVIDIA doesn't offer a supported friends service for GeForce NOW yet. When it does, presence and invites will live here.")
                wrapMode: Text.WordWrap
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 22
                font.weight: Font.DemiBold
                lineHeightMode: Text.FixedHeight
                lineHeight: 33
            }
        }
    }

    Column {
        x: 120
        y: Math.max(600, heroColumn.y + heroColumn.height + 48)
        width: 880
        spacing: 10
        visible: y + height <= root.height - 156
        Repeater {
            model: [[180, 120], [150, 140], [120, 160]]
            Rectangle {
                required property var modelData
                width: 880
                height: 72
                radius: 24
                color: "transparent"
                border.color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.06)
                border.width: 1
                Accessible.ignored: true
                Rectangle { x: 21; anchors.verticalCenter: parent.verticalCenter; width: 44; height: 44; radius: 22; color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.06) }
                Rectangle { x: 81; y: 21; width: parent.modelData[0]; height: 12; radius: 6; color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.08) }
                Rectangle { x: 81; y: 41; width: parent.modelData[1]; height: 10; radius: 5; color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.05) }
            }
        }
    }

    GlassPanel {
        x: parent.width - width - 100
        y: 200
        width: 640
        height: coopColumn.implicitHeight + 74
        panelRadius: 40
        strong: true
        Column {
            id: coopColumn
            x: 37; y: 37
            width: parent.width - 74
            spacing: 20
            Text {
                width: parent.width
                height: 18
                verticalAlignment: Text.AlignVCenter
                text: qsTr("AVAILABLE NOW")
                color: Theme.lightMode ? Qt.darker(Theme.mint, 2.2) : Theme.mint
                font.family: Theme.monoFont
                font.pixelSize: 14
                font.weight: Font.Bold
                font.letterSpacing: 1.96
            }
            Item {
                width: parent.width
                height: Math.max(1, coopTitle.lineCount) * 42
                Text {
                    id: coopTitle
                    y: -5
                    width: parent.width
                    text: qsTr("Play together on this screen")
                    wrapMode: Text.WordWrap
                    color: Theme.label
                    font.family: Theme.displayFont
                    font.pixelSize: 38
                    font.weight: Font.Black
                    font.letterSpacing: -0.76
                    lineHeightMode: Text.FixedHeight
                    lineHeight: 42
                }
            }
            Item {
                width: parent.width
                height: Math.max(1, coopBody.lineCount) * 27
                Text {
                    id: coopBody
                    y: 1
                    width: parent.width
                    text: qsTr("Up to four controllers on this device go straight to your game as separate players.")
                    wrapMode: Text.WordWrap
                    color: Theme.textMuted
                    font.family: Theme.bodyFont
                    font.pixelSize: 18
                    font.weight: Font.DemiBold
                    lineHeightMode: Text.FixedHeight
                    lineHeight: 27
                }
            }
            Grid {
                width: parent.width
                columns: 2
                spacing: 10
                Repeater {
                    model: Math.min(4, root.controllers.length + 1)
                    Rectangle {
                        id: slotCard
                        required property int index
                        readonly property var controller: index < root.controllers.length ? root.controllers[index] : null
                        width: (coopColumn.width - 10) / 2
                        height: 64
                        radius: 20
                        color: controller ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.08) : "transparent"
                        border.color: Theme.seam
                        border.width: controller ? 0 : 1
                        Accessible.role: Accessible.StaticText
                        Accessible.name: qsTr("Player %1: %2").arg(index + 1).arg(controller ? controller.name : qsTr("Waiting"))
                        Row {
                            x: 16 + slotCard.border.width
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 12
                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 38; height: 38; radius: 19
                                color: slotCard.controller ? Theme.mint : "transparent"
                                border.color: slotCard.controller ? "transparent" : Theme.textMuted
                                border.width: slotCard.controller ? 0 : 2
                                Text {
                                    anchors.centerIn: parent
                                    text: qsTr("P%1").arg(slotCard.controller && slotCard.controller.slot !== undefined
                                        ? slotCard.controller.slot : slotCard.index + 1)
                                    color: slotCard.controller ? Theme.contrastText(Theme.mint) : Theme.textMuted
                                    font.family: Theme.displayFont
                                    font.pixelSize: 14
                                    font.weight: Font.Black
                                }
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                width: slotCard.width - 100
                                elide: Text.ElideRight
                                text: slotCard.controller ? String(slotCard.controller.name || qsTr("Controller")) : qsTr("Waiting")
                                color: slotCard.controller ? Theme.label : Theme.textMuted
                                font.family: Theme.displayFont
                                font.pixelSize: 16
                                font.weight: Font.ExtraBold
                            }
                        }
                    }
                }
            }
            ConsoleActionButton {
                id: localJoin
                objectName: "consoleFriendsLocalJoin"
                width: parent.width
                text: root.playerTwoReady ? qsTr("Set up player two") : qsTr("Connect another controller")
                glyph: "A"
                primary: true
                leftPadding: 16
                rightPadding: 26
                labelSize: 21
                KeyNavigation.down: controllerSettings
                onClicked: {
                    AppController.showOverlay("")
                    AppController.navigate("joining")
                }
            }
            ConsoleActionButton {
                id: controllerSettings
                objectName: "consoleFriendsControllerSettings"
                width: parent.width
                height: 64
                cornerRadius: 24
                leftPadding: 25
                labelSize: 19
                labelWeight: Font.ExtraBold
                text: qsTr("Controller settings ›")
                KeyNavigation.up: localJoin
                onClicked: {
                    AppController.showOverlay("")
                    AppController.navigate("controllers")
                }
            }
        }
    }

    AppChrome {
        anchors.fill: parent
        title: qsTr("Friends")
        currentRoute: "friends"
        leftHints: [{glyph: "B", label: qsTr("Back")}]
        rightHints: [{glyph: "A", label: localJoin.activeFocus ? localJoin.text : controllerSettings.text}]
        onRouteRequested: route => {
            AppController.showOverlay("")
            AppController.navigate(route)
        }
    }
}
