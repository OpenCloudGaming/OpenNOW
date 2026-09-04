import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    anchors.fill: parent
    focus: true
    Accessible.name: qsTr("Existing GeForce NOW session")
    Accessible.role: Accessible.Dialog

    function sessionDescription() {
        const session = ShellStore.conflictSession
        if (!session)
            return qsTr("OpenNOW found another session on your NVIDIA account.")
        const parts = []
        if (session.appId)
            parts.push(qsTr("App %1").arg(String(session.appId)))
        if (session.resolution)
            parts.push(String(session.resolution))
        if (Number(session.fps || 0) > 0)
            parts.push(qsTr("%1 FPS").arg(Number(session.fps)))
        return parts.length ? parts.join(" · ") : qsTr("Session details unavailable")
    }

    Rectangle {
        anchors.fill: parent
        color: Qt.rgba(0, 0, 0, 0.58)
    }

    GlassPanel {
        id: card
        anchors.centerIn: parent
        width: Math.min(parent.width - 64, 660)
        height: 390
        scale: root.visible ? 1 : 0.94
        Behavior on scale {
            NumberAnimation { duration: Theme.panelDuration; easing.type: Easing.OutBack }
        }

        Column {
            anchors.fill: parent
            anchors.margins: 42
            spacing: 18

            Text {
                text: qsTr("SESSION ALREADY ACTIVE")
                color: Theme.mint
                font.family: Theme.monoFont
                font.pixelSize: 13
                font.weight: Font.Bold
                font.letterSpacing: 1.5
            }
            Text {
                width: parent.width
                text: ShellStore.pendingLaunchParams
                      ? qsTr("Pick up where you left off?")
                      : qsTr("A cloud session is still running")
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
            }

            Item { width: 1; height: 8 }

            Row {
                spacing: 14
                GlassButton {
                    id: resumeButton
                    text: qsTr("Resume")
                    glyph: "A"
                    primary: true
                    focus: true
                    onClicked: ShellStore.resolveSessionConflict("resume")
                    KeyNavigation.right: newButton
                }
                GlassButton {
                    id: newButton
                    text: ShellStore.pendingLaunchParams ? qsTr("Start new") : qsTr("End session")
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

    Behavior on opacity {
        NumberAnimation { duration: Theme.overlayDuration; easing.type: Easing.OutCubic }
    }
}
