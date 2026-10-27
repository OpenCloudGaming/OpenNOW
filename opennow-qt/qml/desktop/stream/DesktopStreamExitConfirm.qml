import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property bool opened: false
    property bool quittingApplication: false
    property bool endingSessionOnQuit: false
    property string switchTargetName: ""
    readonly property bool switchingAccount: switchTargetName !== ""
    readonly property color onDarkTitle: Theme.lightMode ? "#F6F3EE" : DesktopTokens.text
    readonly property color onDarkBody: Theme.lightMode ? "#C5CDD6" : DesktopTokens.textBody
    readonly property string sessionTitle: {
        const title = ShellStore.sessionGameTitle(ShellStore.activeSession)
        return title || (ShellStore.selectedGame && ShellStore.selectedGame.title ? String(ShellStore.selectedGame.title) : "")
    }
    readonly property bool present: reveal.present
    visible: present
    enabled: opened
    focus: opened
    MotionProgress { id: reveal; shown: root.opened }
    Accessible.role: Accessible.Dialog
    Accessible.name: root.quittingApplication ? qsTr("Quit OpenNOW confirmation")
        : root.switchingAccount ? qsTr("Switch account confirmation") : qsTr("End cloud session confirmation")

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
                text: root.quittingApplication ? qsTr("QUIT OPENNOW")
                    : root.switchingAccount ? qsTr("END SESSION TO SWITCH") : qsTr("END SESSION")
                color: DesktopTokens.danger
                font.family: DesktopTokens.monoFont
                font.pixelSize: 10
                font.weight: Font.Bold
                font.letterSpacing: 1.4
            }
            Text {
                objectName: "accountSwitchTitle"
                width: parent.width
                topPadding: 10
                elide: Text.ElideRight
                text: root.quittingApplication ? qsTr("Quit OpenNOW?")
                    : root.switchingAccount ? qsTr("Switch to %1?").arg(root.switchTargetName)
                    : qsTr("End this cloud session?")
                color: root.onDarkTitle
                font.family: DesktopTokens.displayFont
                font.pixelSize: 28
                font.weight: Font.Black
            }
            Text {
                objectName: "accountSwitchBody"
                textFormat: Text.PlainText
                width: parent.width
                topPadding: 10
                elide: Text.ElideRight
                maximumLineCount: 3
                text: root.quittingApplication && !root.endingSessionOnQuit
                    ? qsTr("OpenNOW will close and disconnect from any active cloud session.")
                    : root.switchingAccount
                    ? (root.sessionTitle !== ""
                        ? qsTr("%1 will close on the remote rig. This session cannot be resumed after it ends.").arg(root.sessionTitle)
                        : qsTr("Your game will close on the remote rig. This session cannot be resumed after it ends."))
                    : AppController.route === "inserting"
                    ? qsTr("Your session request will be cancelled and you will leave the queue.")
                    : qsTr("Your game will close on the remote rig. This session cannot be resumed after it ends.")
                color: root.onDarkBody
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
                    objectName: root.quittingApplication ? "quitConfirmKeepOpen"
                        : root.switchingAccount ? "accountSwitchKeepPlaying" : "streamExitKeepPlaying"
                    text: root.quittingApplication ? qsTr("Keep OpenNOW open")
                        : AppController.route === "inserting" ? qsTr("Keep waiting") : qsTr("Keep playing")
                    shortcutText: qsTr("Esc")
                    primary: true
                    onClicked: root.cancelRequested()
                    KeyNavigation.right: endButton
                }
                DesktopButton {
                    id: endButton
                    objectName: root.quittingApplication ? "quitConfirmQuit"
                        : root.switchingAccount ? "accountSwitchEndSession" : "streamExitEndSession"
                    text: root.quittingApplication ? qsTr("Quit OpenNOW")
                        : root.switchingAccount ? qsTr("End and switch") : qsTr("End session")
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
