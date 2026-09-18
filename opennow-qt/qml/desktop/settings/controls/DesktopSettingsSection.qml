import QtQuick
import OpenNOW

Item {
    id: root
    property string text: ""
    property string description: ""
    default property alias actions: actionRow.data
    width: parent.width
    implicitHeight: Math.max(heading.implicitHeight, actionRow.implicitHeight) + DesktopTokens.px(28)

    Column {
        id: heading
        x: DesktopTokens.settingsInset
        y: DesktopTokens.px(14)
        width: Math.max(0, actionRow.x - x - DesktopTokens.px(16))
        spacing: DesktopTokens.px(4)
        Text {
            width: parent.width
            text: root.text
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.px(12)
            font.weight: Font.ExtraBold
            font.letterSpacing: 1.2
            wrapMode: Text.WordWrap
        }
        Text {
            visible: text !== ""
            width: parent.width
            text: root.description
            color: Theme.textMuted
            font.family: Theme.bodyFont
            font.pixelSize: DesktopTokens.captionSize
            wrapMode: Text.WordWrap
        }
    }
    Row {
        id: actionRow
        anchors.right: parent.right
        anchors.rightMargin: DesktopTokens.settingsInset
        anchors.verticalCenter: parent.verticalCenter
        spacing: DesktopTokens.px(8)
    }
}
