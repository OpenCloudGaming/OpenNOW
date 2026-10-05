import QtQuick
import OpenNOW

FocusScope {
    id: root
    objectName: "consoleJoiningScreen"
    readonly property int connected: ControllerInput.controllers.length
    readonly property bool playerTwoReady: connected >= 2
    readonly property var game: ShellStore.selectedGame || ({ title: qsTr("GeForce NOW session") })

    Component.onCompleted: returnButton.forceActiveFocus()

    SessionGlyphs { id: glyphs }

    LaunchStage {
        anchors.fill: parent
        game: root.game
        tone: root.playerTwoReady ? Theme.mint : Theme.focus
        statusText: qsTr("%1 connected").arg(root.connected)
        eyebrow: qsTr("Local co-op")
        headline: root.playerTwoReady ? qsTr("Player two is ready") : qsTr("Bring player two online")
        detail: root.playerTwoReady
            ? qsTr("Both controllers will be sent with distinct player slots. The Guide button remains reserved for the OpenNOW overlay.")
            : qsTr("OpenNOW forwards up to four standard controllers directly to the active GeForce NOW session. Connect a second controller, then return to the game.")

        actions: [
            LaunchStage.LaunchAction {
                id: returnButton
                objectName: "joiningReturnButton"
                primary: root.playerTwoReady || !ShellStore.activeSession
                text: ShellStore.activeSession ? (root.playerTwoReady ? qsTr("Return to game") : qsTr("Return without player two")) : qsTr("Back to controller settings")
                detail: root.playerTwoReady ? "" : qsTr("This page updates as soon as the second controller appears.")
                glyph: glyphs.button("A")
                keyboardGlyph: glyphs.keyboard
                onClicked: AppController.navigate(ShellStore.activeSession ? "stream" : "controllers")
            }
        ]

        aside: [
            Column {
                x: Math.round((parent.width - width) / 2)
                width: 554
                spacing: 12
                Accessible.role: Accessible.List
                Accessible.name: qsTr("Controller slots")
                Repeater {
                    model: 4
                    Rectangle {
                        id: slotCard
                        required property int index
                        readonly property var controller: ControllerInput.controllers.length > index ? ControllerInput.controllers[index] : null
                        width: parent.width
                        height: 92
                        radius: 24
                        color: controller ? Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.86) : Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.5)
                        border.width: 1
                        border.color: controller ? Theme.seam : Qt.rgba(Theme.face.r, Theme.face.g, Theme.face.b, 0.1)
                        Accessible.role: Accessible.ListItem
                        Accessible.name: controller ? qsTr("Player %1, %2").arg(index + 1).arg(controller.name)
                            : qsTr("Player %1, waiting for controller").arg(index + 1)
                        Rectangle {
                            x: 22
                            anchors.verticalCenter: parent.verticalCenter
                            width: 52; height: 52; radius: 26
                            color: slotCard.controller ? (slotCard.index === 0 ? Theme.mint : Theme.focus) : "transparent"
                            border.width: slotCard.controller ? 0 : 2
                            border.color: Theme.seam
                            Text {
                                anchors.centerIn: parent
                                text: "P" + (slotCard.index + 1)
                                color: slotCard.controller ? Theme.contrastText(parent.color) : Theme.textMuted
                                font.family: Theme.displayFont; font.pixelSize: 18; font.weight: Font.Black
                            }
                        }
                        Column {
                            x: 92
                            anchors.verticalCenter: parent.verticalCenter
                            width: parent.width - 92 - 100
                            Text {
                                width: parent.width
                                elide: Text.ElideRight
                                text: slotCard.controller ? String(slotCard.controller.name) : qsTr("Waiting for controller")
                                color: slotCard.controller ? Theme.label : Theme.textMuted
                                font.family: Theme.displayFont; font.pixelSize: 21; font.weight: Font.Black
                            }
                            Text {
                                width: parent.width
                                elide: Text.ElideRight
                                text: slotCard.controller ? qsTr("Connected and ready") : qsTr("Connect or wake a controller")
                                color: Theme.textMuted
                                font.family: Theme.bodyFont; font.pixelSize: 16; font.weight: Font.DemiBold
                            }
                        }
                        Text {
                            anchors.right: parent.right
                            anchors.rightMargin: 24
                            anchors.verticalCenter: parent.verticalCenter
                            visible: slotCard.controller !== null && Number(slotCard.controller.batteryPercent) >= 0
                            text: slotCard.controller ? qsTr("%1%").arg(slotCard.controller.batteryPercent) : ""
                            color: slotCard.controller && Number(slotCard.controller.batteryPercent) <= 20 ? Theme.yellow : Theme.label
                            font.family: Theme.monoFont; font.pixelSize: 16; font.weight: Font.Bold
                        }
                    }
                }
            }
        ]
    }
}
