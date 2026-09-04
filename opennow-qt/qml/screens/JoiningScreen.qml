import QtQuick
import OpenNOW

FocusScope {
    id: root
    readonly property bool playerTwoReady: AppController.controllerCount >= 2
    readonly property var game: ShellStore.selectedGame || ({ title: qsTr("GeForce NOW session") })

    ScreenBackground { artwork: root.game.heroImageUrl || root.game.imageUrl || ""; tint: "#1F2D45" }
    GlassPanel {
        anchors.centerIn: parent
        width: Math.min(1040, parent.width - 180)
        height: 610
        panelRadius: 40
        strong: true
        Column {
            anchors.fill: parent
            anchors.margins: 40
            spacing: 22
            Text { text: qsTr("Bring player two online"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 38; font.weight: Font.Black }
            Text {
                width: parent.width
                text: qsTr("OpenNOW forwards up to four standard controllers directly to the active GeForce NOW session. Connect a second controller, then return to the game.")
                wrapMode: Text.WordWrap
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 18
            }
            Row {
                spacing: 18
                Repeater {
                    model: [0, 1]
                    GlassPanel {
                        id: slotCard
                        required property int modelData
                        readonly property var controller: ControllerInput.controllers.length > modelData ? ControllerInput.controllers[modelData] : null
                        width: 462; height: 112; panelRadius: 24; strong: true
                        border.color: controller ? Theme.mint : Theme.focus
                        border.width: 2
                        Row {
                            anchors.fill: parent; anchors.margins: 20; spacing: 16
                            Rectangle {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 48; height: 48; radius: 24
                                color: slotCard.controller ? Theme.mint : Theme.focus
                                Text { anchors.centerIn: parent; text: qsTr("P") + (slotCard.modelData + 1); color: Theme.contrastText(slotCard.controller ? Theme.mint : Theme.focus); font.weight: Font.Black }
                            }
                            Column {
                                anchors.verticalCenter: parent.verticalCenter
                                width: 350
                                Text {
                                    width: parent.width; elide: Text.ElideRight
                                    text: slotCard.controller ? slotCard.controller.name : qsTr("Waiting for controller")
                                    color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: 17; font.weight: Font.Bold
                                }
                                Text {
                                    text: slotCard.controller ? qsTr("Connected and ready") : qsTr("Connect or wake a controller")
                                    color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 14
                                }
                            }
                        }
                    }
                }
            }
            GlassPanel {
                width: parent.width; height: 142; panelRadius: 24
                Column {
                    anchors.fill: parent; anchors.margins: 20; spacing: 8
                    Text { text: root.playerTwoReady ? qsTr("Player two is ready") : qsTr("Waiting for player two"); color: root.playerTwoReady ? Theme.mint : Theme.focus; font.family: Theme.bodyFont; font.pixelSize: 18; font.weight: Font.Black }
                    Text {
                        width: parent.width
                        text: root.playerTwoReady
                              ? qsTr("Both controllers will be sent with distinct player slots. The Guide button remains reserved for the OpenNOW overlay.")
                              : qsTr("SDL hot-plug detection is active. This page updates as soon as the second controller appears.")
                        wrapMode: Text.WordWrap; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 15
                    }
                }
            }
            GlassButton {
                width: parent.width
                text: ShellStore.activeSession ? (root.playerTwoReady ? qsTr("Return to game") : qsTr("Return without player two")) : qsTr("Back to controller settings")
                glyph: "A"
                primary: root.playerTwoReady
                onClicked: AppController.navigate(ShellStore.activeSession ? "stream" : "controllers")
                Component.onCompleted: forceActiveFocus()
            }
        }
    }
    AppChrome { anchors.fill: parent; title: qsTr("Local co-op · %1").arg(root.game.title); currentRoute: "controllers"; bottomVisible: false }
}
