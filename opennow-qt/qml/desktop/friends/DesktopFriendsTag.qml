import QtQuick
import OpenNOW

Rectangle {
    id: tag
    property string text: ""
    property color tone: DesktopFriendsFormat.accent
    width: tagLabel.implicitWidth + DesktopTokens.px(18)
    height: DesktopTokens.px(22)
    radius: height / 2
    color: DesktopFriendsFormat.tint(tone, 0.12)
    border.width: 1
    border.color: DesktopFriendsFormat.tint(tone, 0.33)
    Accessible.role: Accessible.StaticText
    Accessible.name: text
    DesktopFriendsLabel {
        id: tagLabel
        anchors.centerIn: parent
        text: tag.text
        mono: true; size: 10; line: 12; tracking: 0.08
        color: tag.tone
        font.weight: Font.Bold
        font.capitalization: Font.AllUppercase
    }
}
