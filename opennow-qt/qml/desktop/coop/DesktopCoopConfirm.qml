import QtQuick
import QtQuick.Controls
import OpenNOW

// Co-op confirmations (overlays coop-end-confirm and coop-leave-confirm), styled like DesktopStreamExitConfirm.
FocusScope {
    id: root
    objectName: leaving ? "coopLeaveConfirm" : "coopEndConfirm"
    property bool leaving: false
    property bool opened: false
    readonly property var coop: ShellStore.coop
    readonly property string gameTitle: coop.game && coop.game.title ? String(coop.game.title) : ""
    readonly property int guestCount: coop.guests.length
    readonly property bool present: reveal.present
    visible: present
    enabled: opened
    focus: opened
    MotionProgress { id: reveal; shown: root.opened }
    Accessible.role: Accessible.Dialog
    Accessible.name: leaving ? qsTr("Leave co-op confirmation") : qsTr("End co-op confirmation")

    function cancel() {
        AppController.showOverlay("")
    }
    function confirm() {
        if (leaving)
            coop.confirmLeave()
        else
            coop.confirmEndCoop()
    }

    Shortcut {
        sequences: ["Return", "Enter"]
        enabled: root.opened
        context: Qt.WindowShortcut
        autoRepeat: false
        onActivated: root.confirm()
    }

    Rectangle {
        anchors.fill: parent
        color: "#A6000000"
        opacity: reveal.progress
        TapHandler { onTapped: root.cancel() }
    }

    Rectangle {
        opacity: reveal.progress
        scale: reveal.zoom
        anchors.centerIn: parent
        width: Math.min(520, root.width - 48)
        height: 268
        radius: 28
        color: "#F50E1018"
        border.width: 1
        border.color: "#24FFFFFF"
        MouseArea { anchors.fill: parent }

        Column {
            anchors.fill: parent
            anchors.margins: 28
            spacing: 0

            Text {
                text: root.leaving ? qsTr("LEAVE CO-OP") : qsTr("END CO-OP")
                color: DesktopTokens.danger
                font.family: DesktopTokens.monoFont
                font.pixelSize: 10
                font.weight: Font.Bold
                font.letterSpacing: 1.4
            }
            Text {
                width: parent.width
                topPadding: 10
                elide: Text.ElideRight
                text: root.leaving
                    ? (root.coop.hostName !== "" ? qsTr("Leave %1's game?").arg(root.coop.hostName) : qsTr("Leave this co-op game?"))
                    : qsTr("End co-op for everyone?")
                color: "#FFFFFF"
                font.family: DesktopTokens.displayFont
                font.pixelSize: 28
                font.weight: Font.Black
            }
            Text {
                width: parent.width
                topPadding: 10
                elide: Text.ElideRight
                maximumLineCount: 3
                text: root.leaving
                    ? (root.coop.hostName !== ""
                        ? qsTr("Your controller stops playing in %1's game. To come back, ask %1 for an invite or the code.").arg(root.coop.hostName)
                        : qsTr("Your controller stops playing in the host's game. To come back, ask for an invite or the code."))
                    : root.guestCount === 0
                    ? qsTr("The code stops working. Your stream keeps running.")
                    : root.guestCount === 1
                    ? qsTr("Your guest leaves and the code stops working. Your stream keeps running.")
                    : qsTr("All %1 guests leave and the code stops working. Your stream keeps running.").arg(root.guestCount)
                color: "#C5CDD6"
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
                    id: keepButton
                    objectName: root.leaving ? "coopLeaveConfirmStay" : "coopEndConfirmKeep"
                    text: root.leaving ? qsTr("Stay in co-op") : qsTr("Keep co-op")
                    shortcutText: qsTr("Esc")
                    primary: true
                    onClicked: root.cancel()
                    KeyNavigation.right: confirmButton
                }
                DesktopButton {
                    id: confirmButton
                    objectName: root.leaving ? "coopLeaveConfirmLeave" : "coopEndConfirmEnd"
                    text: root.leaving ? qsTr("Leave co-op") : qsTr("End co-op")
                    shortcutText: qsTr("Enter")
                    danger: true
                    onClicked: root.confirm()
                    KeyNavigation.left: keepButton
                }
            }
        }
    }

    onOpenedChanged: if (opened) Qt.callLater(() => { if (root.opened) keepButton.forceActiveFocus() })
    Keys.onPressed: event => {
        if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
            root.cancel()
            event.accepted = true
        }
    }
}
