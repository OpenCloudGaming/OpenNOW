import QtQuick
import OpenNOW

Item {
    id: avatar
    property var user: null
    property var friend: null
    property int size: 38
    property int dotSize: 12
    property int dotBorder: 3
    property bool showPresence: friend !== null
    property bool dimmed: false
    property color fill: user && user.id === (ShellStore.friends.user && ShellStore.friends.user.id)
        ? DesktopFriendsFormat.accent : DesktopFriendsFormat.avatarColor(user)
    width: DesktopTokens.px(size); height: width
    Accessible.ignored: true

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: avatar.dimmed ? DesktopFriendsFormat.tint(avatar.fill, 0.5) : avatar.fill
        DesktopFriendsLabel {
            anchors.centerIn: parent
            text: DesktopFriendsFormat.initial(avatar.user)
            size: Math.round(avatar.size * 0.42)
            color: DesktopFriendsFormat.avatarInk
            font.weight: Font.Black
        }
        DesktopAvatarPhoto {
            anchors.fill: parent
            url: avatar.user ? String(avatar.user.avatarUrl || "") : ""
            opacity: avatar.dimmed ? 0.5 : 1
        }
    }
    Rectangle {
        visible: avatar.showPresence
        readonly property bool offline: ShellStore.friends.activity(avatar.friend) === "offline"
        width: DesktopTokens.px(avatar.dotSize); height: width; radius: width / 2
        x: parent.width - width + DesktopTokens.px(2); y: parent.height - height + DesktopTokens.px(2)
        color: offline ? DesktopFriendsFormat.ring : DesktopFriendsFormat.dotColor(avatar.friend)
        border.width: DesktopTokens.px(offline ? 2 : avatar.dotBorder)
        border.color: offline ? DesktopFriendsFormat.offlineRing : DesktopFriendsFormat.ring
    }
}
