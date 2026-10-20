import QtQuick
import QtQuick.Controls
import OpenNOW

// GLT-1: the incoming co-op invite toast and, under it, the compact Invites inbox.
Column {
    id: root
    required property var invite
    required property real now
    readonly property var coop: ShellStore.coop
    readonly property var friends: ShellStore.friends
    readonly property string hostName: friends.displayName(invite.from)
    readonly property real createdMs: Date.parse(String(invite.createdAt || ""))
    readonly property real expiresMs: Date.parse(String(invite.expiresAt || ""))
    readonly property real remainingMs: Math.max(0, expiresMs - now)
    readonly property var inbox: coop.incomingInvites.slice(0, 3)

    width: 384
    spacing: 14

    function clock(ms) {
        const seconds = Math.ceil(Math.max(0, ms) / 1000)
        return "%1:%2".arg(Math.floor(seconds / 60)).arg(String(seconds % 60).padStart(2, "0"))
    }
    function lobbyOf(item) {
        const inviter = item.from ? friends.friendById(item.from.id) : null
        if (inviter && inviter.presence && inviter.presence.lobby)
            return inviter.presence.lobby
        const friend = friends.friends.find(entry => entry.presence && entry.presence.lobby
            && entry.presence.lobby.id === item.lobbyId)
        return friend ? friend.presence.lobby : null
    }
    function playersIn(item) {
        return friends.friends.filter(entry => entry.presence && entry.presence.lobby
            && entry.presence.lobby.id === item.lobbyId && !(item.from && entry.user.id === item.from.id))
            .map(entry => friends.displayName(entry.user))
    }
    function sentText(item) {
        const minutes = Math.floor(Math.max(0, root.now - Date.parse(String(item.createdAt || ""))) / 60000)
        return minutes < 1 ? qsTr("sent just now") : qsTr("sent %1 min ago").arg(minutes)
    }
    function artFor(item) {
        return item.game ? DesktopTokens.decodeArtworkUrl(item.game.artUrl || "") : ""
    }

    Rectangle {
        id: toast
        objectName: "coopInviteToast"
        width: root.width
        height: toastBody.implicitHeight + 36
        radius: 20
        color: "#F00E1018"
        border.width: 1
        border.color: "#24FFFFFF"
        clip: true
        Accessible.role: Accessible.AlertMessage
        Accessible.name: qsTr("%1 invited you to co-op").arg(root.hostName)

        Column {
            id: toastBody
            x: 17
            y: 17
            width: parent.width - 34
            spacing: 12

            Row {
                width: parent.width
                spacing: 12
                DesktopCoopNoticeAvatar { user: root.invite.from }
                Column {
                    width: parent.width - 40 - 24 - 24
                    spacing: 2
                    Text {
                        width: parent.width
                        text: qsTr("%1 invited you to co-op").arg(root.hostName)
                        color: "#FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 15
                        font.weight: Font.ExtraBold
                        height: 18
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                    Text {
                        width: parent.width
                        text: root.now - root.createdMs < 60000 ? qsTr("Just now · from your friends list")
                            : qsTr("%1 min ago · from your friends list").arg(Math.floor((root.now - root.createdMs) / 60000))
                        color: "#66FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 12
                        height: 16
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                }
                AbstractButton {
                    width: 24
                    height: 24
                    focusPolicy: Qt.NoFocus
                    Accessible.role: Accessible.Button
                    Accessible.name: qsTr("Dismiss invite")
                    onClicked: root.coop.dismissInviteToast()
                    Image {
                        anchors.centerIn: parent
                        width: 14
                        height: 14
                        source: "qrc:/qt/qml/OpenNOW/res/icons/desktop-close.svg"
                        sourceSize: Qt.size(28, 28)
                        opacity: 0.5
                    }
                }
            }

            Rectangle {
                width: parent.width
                height: 67
                radius: 12
                color: "#0AFFFFFF"
                border.width: 1
                border.color: "#0FFFFFFF"
                Rectangle {
                    x: 11
                    y: 11
                    width: 80
                    height: 45
                    radius: 8
                    color: "#1A1F2B"
                    clip: true
                    Image {
                        anchors.fill: parent
                        source: root.artFor(root.invite)
                        sourceSize: Qt.size(160, 90)
                        fillMode: Image.PreserveAspectCrop
                        asynchronous: true
                    }
                }
                Column {
                    x: 103
                    anchors.verticalCenter: parent.verticalCenter
                    width: parent.width - x - 11
                    spacing: 3
                    Text {
                        width: parent.width
                        text: root.invite.game ? String(root.invite.game.title || "") : ""
                        color: "#FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 15
                        font.weight: Font.ExtraBold
                        height: 18
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                    }
                    Text {
                        readonly property var summary: root.lobbyOf(root.invite)
                        readonly property var others: root.playersIn(root.invite)
                        width: parent.width
                        text: !summary ? qsTr("Co-op lobby · join from any PC")
                            : others.length > 0
                            ? qsTr("%1 of %2 players · %3 is in · P%4 is yours").arg(summary.size).arg(summary.max).arg(others[0]).arg(Number(summary.size) + 1)
                            : qsTr("%1 of %2 players · P%3 is yours").arg(summary.size).arg(summary.max).arg(Number(summary.size) + 1)
                        color: "#A3FFFFFF"
                        font.family: DesktopTokens.bodyFont
                        font.pixelSize: 12
                        font.weight: Font.DemiBold
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
                    text: qsTr("Join")
                    keys: [qsTr("CTRL"), "⇧", "Y"]
                    onClicked: root.coop.acceptInvite(root.invite.id)
                }
                DesktopCoopToastButton {
                    id: declineButton
                    width: implicitWidth + answers.slack
                    text: qsTr("Not now")
                    keys: [qsTr("CTRL"), "⇧", "N"]
                    onClicked: root.coop.dismissInviteToast()
                }
            }

            Text {
                width: parent.width
                text: qsTr("No GeForce NOW account needed. Expires in %1.").arg(root.clock(root.remainingMs))
                color: "#66FFFFFF"
                font.family: DesktopTokens.bodyFont
                font.pixelSize: 12
                height: 16
                verticalAlignment: Text.AlignVCenter
                elide: Text.ElideRight
            }
        }

        Rectangle {
            x: 12
            anchors.bottom: parent.bottom
            width: 360
            height: 2
            color: "#14FFFFFF"
            Rectangle {
                width: parent.width * Math.max(0, Math.min(1, root.remainingMs / Math.max(1, root.expiresMs - root.createdMs)))
                height: parent.height
                color: "#7FD4FF"
            }
        }
    }

    Column {
        width: root.width
        spacing: 8
        topPadding: 6
        visible: root.inbox.length > 0

        Text {
            text: qsTr("MISSED IT? IT WAITS IN FRIENDS › INVITES")
            color: "#66FFFFFF"
            font.family: DesktopTokens.monoFont
            font.pixelSize: 10
            font.weight: Font.DemiBold
            font.letterSpacing: 1.2
            height: 14
            verticalAlignment: Text.AlignVCenter
        }

        Rectangle {
            objectName: "coopInvitesInbox"
            width: root.width
            height: inboxBody.implicitHeight + 30
            radius: 18
            color: "#E60E1018"
            border.width: 1
            border.color: "#24FFFFFF"
            Accessible.role: Accessible.List
            Accessible.name: qsTr("Co-op invites")

            Column {
                id: inboxBody
                x: 15
                y: 15
                width: parent.width - 30
                spacing: 8

                Item {
                    width: parent.width
                    height: 26
                    Row {
                        x: 2
                        anchors.top: parent.top
                        spacing: 8
                        height: 22
                        Image {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 16
                            height: 16
                            source: "qrc:/qt/qml/OpenNOW/res/icons/desktop-inbox.svg"
                            sourceSize: Qt.size(32, 32)
                        }
                        Text {
                            anchors.verticalCenter: parent.verticalCenter
                            text: qsTr("Invites")
                            color: "#FFFFFF"
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 15
                            font.weight: Font.ExtraBold
                        }
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: newLabel.implicitWidth + 18
                            height: 22
                            radius: 11
                            color: "#7FD4FF"
                            Text {
                                id: newLabel
                                anchors.centerIn: parent
                                text: qsTr("%1 NEW").arg(root.coop.incomingInvites.length)
                                color: "#0B0F1A"
                                font.family: DesktopTokens.monoFont
                                font.pixelSize: 10
                                font.weight: Font.Bold
                                font.letterSpacing: 0.8
                            }
                        }
                    }
                    AbstractButton {
                        anchors.right: parent.right
                        anchors.rightMargin: 2
                        height: 22
                        width: friendsTab.implicitWidth
                        focusPolicy: Qt.NoFocus
                        Accessible.role: Accessible.Link
                        Accessible.name: friendsTab.text
                        onClicked: AppController.navigate("friends-invites")
                        contentItem: Text {
                            id: friendsTab
                            text: qsTr("Friends tab")
                            verticalAlignment: Text.AlignVCenter
                            color: parent.hovered ? "#A3FFFFFF" : "#66FFFFFF"
                            font.family: DesktopTokens.bodyFont
                            font.pixelSize: 12
                            font.weight: Font.Bold
                        }
                    }
                }

                Repeater {
                    model: root.inbox
                    delegate: Rectangle {
                        id: row
                        required property var modelData
                        readonly property bool current: modelData.id === root.invite.id
                        readonly property var summary: root.lobbyOf(modelData)
                        width: inboxBody.width
                        height: 58
                        radius: 12
                        color: "#0AFFFFFF"
                        border.width: 1
                        border.color: "#0FFFFFFF"
                        Accessible.role: Accessible.ListItem
                        Accessible.name: title.text

                        DesktopCoopNoticeAvatar {
                            x: 13
                            anchors.verticalCenter: parent.verticalCenter
                            width: 36
                            user: row.modelData.from
                        }
                        Column {
                            x: 61
                            anchors.verticalCenter: parent.verticalCenter
                            width: thumb.x - x - 12
                            spacing: 2
                            Text {
                                id: title
                                width: parent.width
                                text: qsTr("%1 · %2").arg(root.friends.displayName(row.modelData.from))
                                    .arg(row.modelData.game ? String(row.modelData.game.title || "") : "")
                                color: "#FFFFFF"
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 14
                                font.weight: Font.Bold
                                height: 17
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                            }
                            Text {
                                readonly property string when: row.current
                                    ? qsTr("expires in %1").arg(root.clock(Date.parse(String(row.modelData.expiresAt || "")) - root.now))
                                    : root.sentText(row.modelData)
                                width: parent.width
                                text: row.summary ? qsTr("%1/%2 · %3").arg(row.summary.size).arg(row.summary.max).arg(when) : when
                                color: row.current ? "#7FD4FF" : "#66FFFFFF"
                                font.family: DesktopTokens.bodyFont
                                font.pixelSize: 12
                                font.weight: Font.DemiBold
                                height: 16
                                verticalAlignment: Text.AlignVCenter
                                elide: Text.ElideRight
                            }
                        }
                        Rectangle {
                            id: thumb
                            anchors.right: join.left
                            anchors.rightMargin: 12
                            anchors.verticalCenter: parent.verticalCenter
                            width: 56
                            height: 32
                            radius: 6
                            color: "#1A1F2B"
                            clip: true
                            Image {
                                anchors.fill: parent
                                source: root.artFor(row.modelData)
                                sourceSize: Qt.size(112, 64)
                                fillMode: Image.PreserveAspectCrop
                                asynchronous: true
                            }
                        }
                        DesktopCoopToastButton {
                            id: join
                            anchors.right: parent.right
                            anchors.rightMargin: 13
                            anchors.verticalCenter: parent.verticalCenter
                            compact: true
                            primary: row.current
                            text: qsTr("Join")
                            Accessible.name: qsTr("Join %1").arg(title.text)
                            onClicked: root.coop.acceptInvite(row.modelData.id)
                        }
                    }
                }
            }
        }
    }
}
