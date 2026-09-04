import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    focus: true
    Accessible.role: Accessible.Dialog
    Accessible.name: qsTr("End cloud session confirmation")

    signal cancelRequested()
    signal confirmRequested()

    Rectangle {
        anchors.fill: parent
        color: "#A6000000"
        TapHandler { onTapped: root.cancelRequested() }
    }

    Rectangle {
        id: card
        anchors.centerIn: parent
        width: Math.min(520, root.width - 48)
        height: 268
        radius: 28
        color: "#F50E1018"
        border.width: 1
        border.color: DesktopTokens.seam

        Column {
            anchors.fill: parent
            anchors.margins: 28
            spacing: 0

            Text {
                text: qsTr("END SESSION")
                color: DesktopTokens.danger
                font.family: DesktopTokens.monoFont
                font.pixelSize: 10
                font.weight: Font.Bold
                font.letterSpacing: 1.4
            }
            Text {
                width: parent.width
                topPadding: 10
                text: qsTr("End this cloud session?")
                color: DesktopTokens.text
                font.family: DesktopTokens.displayFont
                font.pixelSize: 28
                font.weight: Font.Black
            }
            Text {
                width: parent.width
                topPadding: 10
                text: qsTr("Your game will close on the remote rig. This session cannot be resumed after it ends.")
                color: DesktopTokens.textBody
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 14
                font.weight: Font.Medium
                wrapMode: Text.WordWrap
                lineHeight: 1.25
            }

            Item { width: 1; height: 22 }
            Row {
                anchors.right: parent.right
                spacing: 10
                DesktopButton {
                    id: keepPlayingButton
                    text: qsTr("Keep playing")
                    shortcutText: qsTr("Esc")
                    primary: true
                    onClicked: root.cancelRequested()
                    KeyNavigation.right: endButton
                }
                DesktopButton {
                    id: endButton
                    text: qsTr("End session")
                    shortcutText: qsTr("Enter")
                    danger: true
                    onClicked: root.confirmRequested()
                    KeyNavigation.left: keepPlayingButton
                }
            }
        }
    }

    onVisibleChanged: if (visible) Qt.callLater(keepPlayingButton.forceActiveFocus)
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.cancelRequested()
            event.accepted = true
        }
    }
}
