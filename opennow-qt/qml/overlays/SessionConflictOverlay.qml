import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    objectName: "sessionConflictDialog"
    anchors.fill: parent
    focus: true
    Accessible.name: qsTr("Existing GeForce NOW session")
    Accessible.role: Accessible.Dialog

    function sessionDescription() {
        const session = ShellStore.conflictSession
        if (!session)
            return qsTr("OpenNOW found another session on your NVIDIA account.")
        const title = ShellStore.sessionGameTitle(session)
        return title ? qsTr("Still running: %1").arg(title)
                     : qsTr("A game is still running on your GeForce NOW account.")
    }

    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, 0.58)
    }

    GlassPanel {
        id: card
        strong: true
        anchors.centerIn: parent
        width: Math.min(parent.width - 64, 660)
        height: content.height + 64
        // OverlayHost owns the single reveal transform.

        Column {
            id: content
            anchors.centerIn: parent
            width: parent.width - 64
            spacing: 18

            Text {
                text: qsTr("YOUR GAME IS STILL RUNNING")
                color: Theme.mint
                font.family: Theme.monoFont
                font.pixelSize: 13
                font.weight: Font.Bold
                font.letterSpacing: 1.5
            }
            Text {
                width: parent.width
                text: qsTr("Return to your game?")
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 34
                font.weight: Font.Black
                wrapMode: Text.WordWrap
            }
            Text {
                width: parent.width
                text: root.sessionDescription()
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 18
                wrapMode: Text.WordWrap
            }
            Text {
                width: parent.width
                text: ShellStore.streamMessage
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 16
                wrapMode: Text.WordWrap
            }
            Text {
                width: parent.width
                text: ShellStore.pendingLaunchParams
                    ? qsTr("Ending the running game will close it before starting %1. Unsaved progress may be lost.").arg(ShellStore.pendingLaunchParams.title || qsTr("your selected game"))
                    : qsTr("Ending the game will close it. Unsaved progress may be lost.")
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 14
                wrapMode: Text.WordWrap
            }

            Item { width: 1; height: 8 }

            Flow {
                width: parent.width
                spacing: 14
                GlassButton {
                    id: resumeButton
                    text: qsTr("Return to game")
                    glyph: "A"
                    primary: true
                    focus: true
                    onClicked: ShellStore.resolveSessionConflict("resume")
                    KeyNavigation.right: newButton
                }
                GlassButton {
                    id: newButton
                    text: ShellStore.pendingLaunchParams ? qsTr("End game and start new") : qsTr("End game")
                    glyph: "X"
                    danger: true
                    onClicked: ShellStore.resolveSessionConflict("new")
                    KeyNavigation.left: resumeButton
                    KeyNavigation.right: cancelButton
                }
                GlassButton {
                    id: cancelButton
                    text: qsTr("Cancel")
                    glyph: "B"
                    onClicked: ShellStore.resolveSessionConflict("cancel")
                    KeyNavigation.left: newButton
                }
            }
        }
    }

    Component.onCompleted: resumeButton.forceActiveFocus()


}
