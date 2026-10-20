import QtQuick
import OpenNOW

Item {
    id: section
    property string label: ""
    property int count: -1
    property string note: ""
    width: parent ? parent.width : 0
    height: DesktopTokens.px(18)
    Accessible.role: Accessible.Heading
    Accessible.name: label
    Row {
        x: DesktopTokens.px(14)
        anchors.verticalCenter: parent.verticalCenter
        spacing: DesktopTokens.px(10)
        DesktopFriendsLabel {
            text: section.label
            size: 12; line: 16; tracking: 0.1
            color: DesktopTokens.textMuted
            font.weight: Font.ExtraBold
            font.capitalization: Font.AllUppercase
        }
        DesktopFriendsLabel {
            visible: section.count >= 0
            text: String(section.count)
            mono: true; size: 11; line: 15; tracking: 0.06
            color: DesktopFriendsFormat.faint
            font.weight: Font.DemiBold
        }
    }
    DesktopFriendsLabel {
        visible: section.note !== ""
        anchors.right: parent.right; anchors.rightMargin: DesktopTokens.px(14)
        anchors.verticalCenter: parent.verticalCenter
        text: section.note
        size: 12; line: 16
        color: DesktopFriendsFormat.faint
    }
}
