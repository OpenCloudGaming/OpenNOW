import QtQuick
import OpenNOW

// MFZ-1: the outcome of an ask to join a full lobby ("full") and the follow-up when a watched
// lobby has room again ("room"). The default answer runs by itself when the bar runs out.
Rectangle {
    id: root
    objectName: "coopAskNotice"
    required property var notice
    readonly property var coop: ShellStore.coop
    readonly property var friends: ShellStore.friends
    readonly property bool full: notice.kind === "full"
    readonly property string name: friends.displayName(notice.user)
    readonly property string gameTitle: notice.game && notice.game.title ? String(notice.game.title) : qsTr("Their game")
    readonly property var friend: notice.user ? friends.friendById(notice.user.id) : null
    readonly property int maxPlayers: friend && friend.presence && friend.presence.lobby ? Number(friend.presence.lobby.max || 0) : 0
    readonly property int lifetimeMs: 20000
    property real remaining: 1

    width: 384
    height: Math.max(40, content.implicitHeight) + 32
    radius: 20
    color: "#F00E1018"
    border.width: 1
    border.color: "#24FFFFFF"
    clip: true
    Accessible.role: Accessible.AlertMessage
    Accessible.name: qsTr("%1. %2").arg(heading.text).arg(detail.text)

    function acceptDefault() {
        if (full)
            coop.watchForRoom(notice.user ? notice.user.id : "")
        else
            coop.dismissAskNotice()
    }

    NumberAnimation on remaining {
        id: countdown
        from: 1
        to: 0
        duration: root.lifetimeMs
        running: true
        onFinished: root.acceptDefault()
    }
    onNoticeChanged: countdown.restart()

    DesktopCoopNoticeAvatar {
        x: 17
        y: 15
        user: root.notice.user
    }
    Column {
        id: content
        x: 69
        y: 15
        width: parent.width - x - 17
        spacing: 3
        Text {
            id: heading
            width: parent.width
            text: root.full ? qsTr("%1’s lobby just filled up").arg(root.name) : qsTr("There’s room in %1’s lobby").arg(root.name)
            color: "#FFFFFF"
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 15
            font.weight: Font.ExtraBold
            height: 18
            verticalAlignment: Text.AlignVCenter
            elide: Text.ElideRight
        }
        Text {
            id: detail
            width: parent.width
            text: !root.full ? qsTr("%1 has a free spot. Ask again before it fills up.").arg(root.gameTitle)
                : root.maxPlayers > 0 ? qsTr("%1 is at %2 of %2. We’ll ping you if a spot opens.").arg(root.gameTitle).arg(root.maxPlayers)
                : qsTr("%1 is full. We’ll ping you if a spot opens.").arg(root.gameTitle)
            color: "#A8FFFFFF"
            font.family: DesktopTokens.bodyFont
            font.pixelSize: 13
            lineHeightMode: Text.FixedHeight
            lineHeight: 18
            wrapMode: Text.WordWrap
        }
        Row {
            topPadding: 8
            spacing: 8
            DesktopCoopToastButton {
                compact: true
                primary: true
                text: root.full ? qsTr("OK") : qsTr("Ask again")
                onClicked: {
                    if (root.full) {
                        root.acceptDefault()
                    } else {
                        if (root.friend)
                            root.coop.askToJoin(root.friend)
                        root.coop.dismissAskNotice()
                    }
                }
            }
            DesktopCoopToastButton {
                compact: true
                quiet: true
                text: root.full ? qsTr("Stop asking") : qsTr("Not now")
                onClicked: {
                    if (root.full) {
                        root.coop.cancelAsk("full-" + root.notice.lobbyId)
                        root.coop.stopWatchingForRoom(root.notice.user ? root.notice.user.id : "")
                    } else {
                        root.coop.dismissAskNotice()
                    }
                }
            }
        }
    }

    Rectangle {
        x: 12
        anchors.bottom: parent.bottom
        width: 360
        height: 2
        color: "#14FFFFFF"
        Rectangle {
            width: parent.width * root.remaining
            height: parent.height
            color: "#F5A623"
        }
    }
}
