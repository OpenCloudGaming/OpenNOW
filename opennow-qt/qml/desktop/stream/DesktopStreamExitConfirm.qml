import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property bool opened: false
    readonly property bool present: reveal.present
    visible: present
    enabled: opened
    focus: opened
    MotionProgress { id: reveal; shown: root.opened }
    Accessible.role: Accessible.Dialog
    Accessible.name: qsTr("End cloud session confirmation")

    signal cancelRequested()
    signal confirmRequested()

    Shortcut {
        sequences: ["Return", "Enter"]
        enabled: root.opened
        context: Qt.WindowShortcut
        autoRepeat: false
        onActivated: root.confirmRequested()
    }

    Rectangle {
        anchors.fill: parent
        color: "#A6000000"
        opacity: reveal.progress
        TapHandler { onTapped: root.cancelRequested() }
    }

    Rectangle {
        id: card
        opacity: reveal.progress
        scale: reveal.zoom
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
                text: AppController.route === "inserting"
                    ? qsTr("Your session request will be cancelled and you will leave the queue.")
                    : qsTr("Your game will close on the remote rig. This session cannot be resumed after it ends.")
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
                    objectName: "streamExitKeepPlaying"
                    text: AppController.route === "inserting" ? qsTr("Keep waiting") : qsTr("Keep playing")
                    shortcutText: qsTr("Esc")
                    primary: true
                    onClicked: root.cancelRequested()
                    KeyNavigation.right: endButton
                }
                DesktopButton {
                    id: endButton
                    objectName: "streamExitEndSession"
                    text: qsTr("End session")
                    shortcutText: qsTr("Enter")
                    danger: true
                    onClicked: root.confirmRequested()
                    KeyNavigation.left: keepPlayingButton
                }
            }
        }
    }

    onOpenedChanged: if (opened) Qt.callLater(() => { if (root.opened) keepPlayingButton.forceActiveFocus() })
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.cancelRequested()
            event.accepted = true
        }
    }
}
