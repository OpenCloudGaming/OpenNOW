import QtQuick
import OpenNOW

// H3H-1: a friend asks to join the host's lobby. Two or more asks collapse into one card that
// points at the in-stream drawer, where each ask has its own answer.
Item {
    id: root
    objectName: "coopAskToast"
    required property real now
    readonly property var coop: ShellStore.coop
    readonly property var friends: ShellStore.friends
    readonly property var request: coop.joinRequests.length > 0 ? coop.joinRequests[0] : null
    readonly property bool collapsed: coop.joinRequests.length > 1
    readonly property string name: request ? friends.displayName(request.from) : ""
    readonly property bool codeJoin: request !== null && request.kind === "code"
    readonly property real createdMs: request ? Date.parse(String(request.createdAt || "")) : 0
    readonly property real expiresMs: request ? Date.parse(String(request.expiresAt || "")) : 0
    readonly property real remainingMs: Math.max(0, expiresMs - now)

    width: 384
    implicitHeight: collapsed ? summary.implicitHeight : toast.height

    function activityText() {
        const friend = request && request.from ? friends.friendById(request.from.id) : null
        const presence = friend && friend.presence ? friend.presence : null
        switch (presence ? String(presence.activity || "") : "") {
        case "browsing_store": return qsTr("in the store right now")
        case "browsing_library": return qsTr("in their library")
        case "in_settings": return qsTr("in settings")
        case "playing": return presence.game ? qsTr("playing %1").arg(presence.game.title) : qsTr("playing a game")
        case "in_queue": return qsTr("waiting in a queue")
        case "in_lobby": return qsTr("in another co-op lobby")
        case "idle": return qsTr("away")
        }
        return qsTr("online")
    }

    DesktopCoopNoticeCard {
        id: summary
        visible: root.collapsed
        title: root.coop.joinRequests.every(item => item.kind === "code")
            ? qsTr("%1 guests joined with your code").arg(root.coop.joinRequests.length)
            : qsTr("%1 friends want to join").arg(root.coop.joinRequests.length)
        body: qsTr("Press Ctrl+G to answer them in the drawer")
        trailingIcon: "qrc:/qt/qml/OpenNOW/res/icons/desktop-users-violet.svg"
        onActivated: {
            if (AppController.route === "stream")
                ShellStore.applyStreamShortcutAction("guide")
            else
                AppController.showOverlay("coop-lobby")
        }
    }

    Rectangle {
        id: toast
        visible: !root.collapsed && root.request !== null
        width: root.width
        height: body.implicitHeight + 36
        radius: 20
        color: "#F00E1018"
        border.width: 1
        border.color: "#24FFFFFF"
        clip: true
        Accessible.role: Accessible.AlertMessage
        Accessible.name: root.codeJoin
            ? qsTr("%1 joined with your code. Ctrl+Shift+Y lets them in, Ctrl+Shift+N removes them.").arg(root.name)
            : qsTr("%1 wants to join. Ctrl+Shift+Y lets them in, Ctrl+Shift+N declines.").arg(root.name)

        Column {
            id: body
            x: 17
            y: 17
            width: parent.width - 34
            spacing: 12

            Row {
                width: parent.width
                spacing: 12
                DesktopCoopNoticeAvatar {
                    user: root.request ? root.request.from : null
                    online: true
                }
                Column {
                    width: parent.width - 52
                    spacing: 3
                    Row {
                        spacing: 8
                        height: 22
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: root.codeJoin ? qsTr("%1 joined with your code").arg(root.name)
                                : qsTr("%1 wants to join").arg(root.name)
                            color: "#FFFFFF"
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 15
                            font.weight: Font.ExtraBold
                        }
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: pill.implicitWidth + 18
                            height: 22
                            radius: 11
                            color: "#1FA78BFA"
                            border.width: 1
                            border.color: "#55A78BFA"
                            Text {
                                id: pill
                                anchors.centerIn: parent
                                text: root.codeJoin ? qsTr("CODE") : qsTr("ASK TO JOIN")
                                color: "#A78BFA"
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 10
                                font.weight: Font.Bold
                                font.letterSpacing: 0.8
                            }
                        }
                    }
                    Text {
                        readonly property int slot: root.coop.nextFreeSlot()
                        width: parent.width
                        text: root.codeJoin ? qsTr("Holding P%1 · no stream or controller until you let them in").arg(root.request.slot + 1)
                            : slot > 0 ? qsTr("Friend · %1 · would take P%2").arg(root.activityText()).arg(slot + 1)
                            : qsTr("Friend · %1 · your lobby is full").arg(root.activityText())
                        color: "#A3FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 12
                        height: 16
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                }
            }

            Row {
                id: answers
                readonly property real slack: Math.max(0, width - 8 - acceptButton.implicitWidth - declineButton.implicitWidth) / 2
                width: parent.width
                spacing: 8
                DesktopCoopToastButton {
                    id: acceptButton
                    width: implicitWidth + answers.slack
                    primary: true
                    text: qsTr("Let in")
                    keys: [qsTr("CTRL"), "⇧", "Y"]
                    onClicked: root.coop.respondJoinRequest(root.request.id, true)
                }
                DesktopCoopToastButton {
                    id: declineButton
                    width: implicitWidth + answers.slack
                    text: qsTr("Decline")
                    keys: [qsTr("CTRL"), "⇧", "N"]
                    onClicked: root.coop.respondJoinRequest(root.request.id, false)
                }
            }

            Row {
                spacing: 6
                Image {
                    anchors.verticalCenter: parent.verticalCenter
                    width: 13
                    height: 13
                    source: "qrc:/qt/qml/OpenNOW/res/icons/desktop-clock.svg"
                    sourceSize: Qt.size(26, 26)
                    opacity: 0.5
                }
                Text {
                    readonly property int seconds: Math.ceil(root.remainingMs / 1000)
                    text: root.codeJoin ? qsTr("Waits for your answer · your game keeps running")
                        : qsTr("Declines by itself in %1 · your game keeps running")
                        .arg("%1:%2".arg(Math.floor(seconds / 60)).arg(String(seconds % 60).padStart(2, "0")))
                    color: "#66FFFFFF"
                    font.family: DesktopTokens.bodyFont
                    font.pixelSize: 12
                    height: 16
                    verticalAlignment: Text.AlignVCenter
                }
            }
        }

        Rectangle {
            visible: !root.codeJoin
            x: 12
            anchors.bottom: parent.bottom
            width: 360
            height: 2
            color: "#14FFFFFF"
            Rectangle {
                width: parent.width * Math.max(0, Math.min(1, root.remainingMs / Math.max(1, root.expiresMs - root.createdMs)))
                height: parent.height
                color: "#A78BFA"
            }
        }
    }
}
