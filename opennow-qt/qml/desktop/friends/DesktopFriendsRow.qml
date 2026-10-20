import QtQuick
import OpenNOW

Rectangle {
    id: row
    property var friend: null
    property bool selected: false
    property real nowMs: Date.now()
    property int actionWidth: 150
    default property alias actions: actionHost.data
    readonly property bool hovered: hover.hovered
    readonly property var user: friend ? friend.user : null
    readonly property var activity: DesktopFriendsFormat.activity(friend, nowMs)
    readonly property var game: DesktopFriendsFormat.presence(friend).game || null
    signal activated()
    signal contextMenuRequested(point position)
    width: parent ? parent.width : 0
    height: DesktopTokens.px(54)
    radius: DesktopTokens.px(12)
    color: selected ? DesktopTokens.raised : "transparent"
    border.width: 1
    border.color: selected ? DesktopTokens.seam : "transparent"
    Accessible.role: Accessible.ListItem
    Accessible.name: qsTr("%1, %2").arg(ShellStore.friends.displayName(user)).arg(activity.text)

    HoverHandler { id: hover }
    TapHandler { onTapped: row.activated() }
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: eventPoint => row.contextMenuRequested(eventPoint.position)
    }

    DesktopFriendsAvatar {
        id: avatar
        x: DesktopTokens.px(14)
        anchors.verticalCenter: parent.verticalCenter
        user: row.user
        friend: row.friend
    }
    Column {
        x: avatar.x + avatar.width + DesktopTokens.px(14)
        width: thumb.x - x - DesktopTokens.px(14)
        anchors.verticalCenter: parent.verticalCenter
        spacing: DesktopTokens.px(2)
        Row {
            spacing: DesktopTokens.px(8)
            DesktopFriendsLabel {
                text: ShellStore.friends.displayName(row.user)
                size: 15; line: 18
                font.weight: Font.ExtraBold
            }
            DesktopFriendsLabel {
                anchors.verticalCenter: parent.verticalCenter
                text: "@" + String(row.user && row.user.username || "")
                mono: true; size: 12; line: 16
                color: DesktopFriendsFormat.faint
            }
        }
        Row {
            spacing: DesktopTokens.px(6)
            Rectangle {
                visible: row.activity.glyph === "dot"
                anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(8); height: width; radius: width / 2
                color: DesktopFriendsFormat.green
            }
            DesktopFriendsIcon {
                visible: row.activity.glyph !== "" && row.activity.glyph !== "dot"
                anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(13); height: width
                glyph: row.activity.glyph || "users"
                ink: row.activity.color
            }
            DesktopFriendsLabel {
                text: row.activity.text
                size: 13; line: 17
                color: row.activity.color
                font.weight: row.activity.strong ? Font.ExtraBold : Font.DemiBold
                elide: Text.ElideRight
            }
            DesktopFriendsLabel {
                visible: row.activity.meta !== ""
                anchors.verticalCenter: parent.verticalCenter
                text: row.activity.meta
                mono: true; size: 10; line: 14; tracking: 0.06
                color: DesktopFriendsFormat.faint
                font.weight: Font.DemiBold
                font.capitalization: Font.AllUppercase
            }
        }
    }
    Rectangle {
        id: thumb
        x: actionArea.x - DesktopTokens.px(14) - width
        anchors.verticalCenter: parent.verticalCenter
        width: DesktopTokens.px(72); height: DesktopTokens.px(40)
        radius: DesktopTokens.px(6)
        color: "#1A1F2B"
        visible: art.source.toString() !== ""
        clip: true
        Image {
            id: art
            anchors.fill: parent
            source: DesktopFriendsFormat.gameArt(row.game)
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            sourceSize: Qt.size(DesktopTokens.px(144), DesktopTokens.px(80))
        }
    }
    Item {
        id: actionArea
        x: parent.width - DesktopTokens.px(14) - width
        width: DesktopTokens.px(row.actionWidth)
        height: parent.height
        Row {
            id: actionHost
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: DesktopTokens.px(8)
        }
    }
}
