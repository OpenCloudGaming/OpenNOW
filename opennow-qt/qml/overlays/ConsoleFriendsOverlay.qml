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
        x: 120; y: 200
        width: 880
        spacing: 24

        Rectangle {
            width: soonRow.implicitWidth + 32
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
                    text: qsTr("COMING SOON")
                    color: Theme.lightMode ? Qt.darker(Theme.violet, 1.6) : Theme.violet
                    font.family: Theme.monoFont
                    font.pixelSize: 15
                    font.weight: Font.Bold
                    font.letterSpacing: 2
                }
            }
        }
        Text {
            width: parent.width
            text: qsTr("Friends are on the way")
            wrapMode: Text.WordWrap
            color: Theme.label
            font.family: Theme.displayFont
            font.pixelSize: 76
            font.weight: Font.Black
            font.letterSpacing: -1.5
            Accessible.role: Accessible.Heading
            Accessible.name: text
        }
        Text {
            width: 760
            text: root.capabilities.reason
                || qsTr("NVIDIA doesn't offer a supported friends service for GeForce NOW yet. When it does, presence and invites will live here.")
            wrapMode: Text.WordWrap
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: 22
            lineHeight: 1.25
        }
        Item { width: 1; height: 120 }
        Repeater {
            model: [180, 150, 120]
            Rectangle {
                required property int modelData
                width: 880
                height: 70
                radius: 22
                color: "transparent"
                border.color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.06)
                border.width: 1
                Accessible.ignored: true
                Rectangle { x: 20; anchors.verticalCenter: parent.verticalCenter; width: 44; height: 44; radius: 22; color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.06) }
                Rectangle { x: 80; y: 20; width: parent.modelData; height: 12; radius: 6; color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.08) }
                Rectangle { x: 80; y: 40; width: parent.modelData * 0.66; height: 9; radius: 5; color: Qt.rgba(Theme.label.r, Theme.label.g, Theme.label.b, 0.05) }
            }
        }
    }

    GlassPanel {
        x: parent.width - width - 100
        y: 200
        width: 640
        height: coopColumn.implicitHeight + 72
        panelRadius: 40
        strong: true
        Column {
            id: coopColumn
            x: 36; y: 36
            width: parent.width - 72
            spacing: 22
            Text {
                text: qsTr("AVAILABLE NOW")
                color: Theme.lightMode ? Qt.darker(Theme.mint, 2.2) : Theme.mint
                font.family: Theme.monoFont
                font.pixelSize: 15
                font.weight: Font.Bold
                font.letterSpacing: 2
            }
            Text {
                width: parent.width
                text: qsTr("Play together on this screen")
                wrapMode: Text.WordWrap
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 36
                font.weight: Font.Black
            }
            Text {
                width: parent.width
                text: qsTr("Up to four controllers on this device go straight to your game as separate players.")
                wrapMode: Text.WordWrap
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 18
                lineHeight: 1.25
            }
            Grid {
                width: parent.width
                columns: 2
                spacing: 12
                Repeater {
                    model: Math.min(4, root.controllers.length + 1)
                    Rectangle {
                        id: slotCard
                        required property int index
                        readonly property var controller: index < root.controllers.length ? root.controllers[index] : null
                        width: (coopColumn.width - 12) / 2
                        height: 64
                        radius: 20
                        color: controller ? Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.07) : "transparent"
                        border.color: Theme.seam
                        border.width: 1
                        Accessible.role: Accessible.StaticText
                        Accessible.name: qsTr("Player %1: %2").arg(index + 1).arg(controller ? controller.name : qsTr("Waiting"))
                        Row {
                            x: 16
                            anchors.verticalCenter: parent.verticalCenter
                            spacing: 12
                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 36; height: 36; radius: 18
                                color: slotCard.controller ? Theme.mint : "transparent"
                                border.color: slotCard.controller ? "transparent" : Theme.textMuted
                                border.width: 1
                                Text {
                                    anchors.centerIn: parent
                                    text: qsTr("P%1").arg(slotCard.controller && slotCard.controller.slot !== undefined
                                        ? slotCard.controller.slot : slotCard.index + 1)
                                    color: slotCard.controller ? Theme.contrastText(Theme.mint) : Theme.textMuted
                                    font.family: Theme.monoFont
                                    font.pixelSize: 13
                                    font.weight: Font.Bold
                                }
                            }
                            Text {
                                anchors.verticalCenter: parent.verticalCenter
                                width: slotCard.width - 110
                                elide: Text.ElideRight
                                text: slotCard.controller ? String(slotCard.controller.name || qsTr("Controller")) : qsTr("Waiting")
                                color: slotCard.controller ? Theme.label : Theme.textMuted
                                font.family: Theme.bodyFont
                                font.pixelSize: 17
                                font.weight: Font.Bold
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
