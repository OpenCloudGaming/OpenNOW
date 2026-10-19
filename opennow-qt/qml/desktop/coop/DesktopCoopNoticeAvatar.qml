import QtQuick
import OpenNOW

Rectangle {
    id: root
    property var user: null
    property bool online: false
    readonly property string initial: ShellStore.friends.displayName(user).charAt(0).toUpperCase()
    width: 40
    height: width
    radius: width / 2
    color: ShellStore.friends.avatarColor(user)
    Accessible.ignored: true

    Text {
        anchors.centerIn: parent
        text: root.initial
        color: "#0B0F1A"
        font.family: DesktopTokens.bodyFont
        font.pixelSize: Math.round(root.width * 0.42)
        font.weight: Font.Black
    }
    DesktopAvatarPhoto {
        anchors.fill: parent
        url: root.user ? String(root.user.avatarUrl || "") : ""
    }
    Rectangle {
        visible: root.online
        readonly property int ring: root.width >= 40 ? 3 : 2
        width: root.width >= 40 ? 13 : 10
        height: width
        radius: width / 2
        x: root.width - width + 2
        y: root.height - height + 2
        color: "#4ADE80"
        border.width: ring
        border.color: "#0B0F1A"
    }
}
