import QtQuick
import OpenNOW

Rectangle {
    id: row
    property var user: null
    property string title: ""
    property string subtitle: ""
    property string subtitleGlyph: ""
    property color subtitleColor: DesktopTokens.textMuted
    property bool subtitleStrong: false
    property string handle: ""
    property var game: null
    property bool highlighted: false
    property bool faded: false
    property int rowHeight: 68
    property int avatarSize: 40
    property bool avatarDimmed: false
    property int actionWidth: 0
    default property alias actions: actionRow.data
    width: parent ? parent.width : 0
    height: DesktopTokens.px(rowHeight)
    radius: DesktopTokens.px(14)
    opacity: faded ? 0.7 : 1
    color: highlighted ? DesktopFriendsFormat.tint(DesktopFriendsFormat.accent, 0.07) : DesktopFriendsFormat.rowFill
    border.width: 1
    border.color: highlighted ? DesktopFriendsFormat.tint(DesktopFriendsFormat.accent, 0.3) : DesktopTokens.seamSoft
    Accessible.role: Accessible.ListItem
    Accessible.name: subtitle === "" ? title : qsTr("%1, %2").arg(title).arg(subtitle)

    DesktopFriendsAvatar {
        id: avatar
        x: DesktopTokens.px(14)
        anchors.verticalCenter: parent.verticalCenter
        user: row.user
        size: row.avatarSize
        showPresence: false
        dimmed: row.avatarDimmed
    }
    Column {
        x: avatar.x + avatar.width + DesktopTokens.px(14)
        width: (thumb.visible ? thumb.x : actionArea.x) - x - DesktopTokens.px(14)
        anchors.verticalCenter: parent.verticalCenter
        spacing: DesktopTokens.px(row.handle !== "" ? 2 : 3)
        Row {
            spacing: DesktopTokens.px(8)
            DesktopFriendsLabel {
                text: row.title
                size: 15; line: 18
                font.weight: Font.ExtraBold
            }
            DesktopFriendsLabel {
                visible: row.handle !== ""
                anchors.verticalCenter: parent.verticalCenter
                text: "@" + row.handle
                mono: true; size: 12; line: 16
                color: DesktopFriendsFormat.faint
            }
        }
        Row {
            visible: row.subtitle !== ""
            spacing: DesktopTokens.px(6)
            DesktopFriendsIcon {
                visible: row.subtitleGlyph !== ""
                anchors.verticalCenter: parent.verticalCenter
                width: DesktopTokens.px(13); height: width
                glyph: row.subtitleGlyph || "clock"
                ink: row.subtitleColor
            }
            DesktopFriendsLabel {
                text: row.subtitle
                size: 13; line: 17
                color: row.subtitleColor
                font.weight: row.subtitleStrong ? Font.DemiBold : Font.Normal
                elide: Text.ElideRight
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
        visible: row.game !== null && art.source.toString() !== ""
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
        width: row.actionWidth > 0 ? DesktopTokens.px(row.actionWidth) : actionRow.width
        height: parent.height
        Row {
            id: actionRow
            anchors.right: parent.right
            anchors.verticalCenter: parent.verticalCenter
            spacing: DesktopTokens.px(8)
        }
    }
}
