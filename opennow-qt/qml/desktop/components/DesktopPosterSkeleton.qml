import QtQuick
import OpenNOW

Item {
    id: root

    property int captionHeight: 0
    property real cornerRadius: 12
    Accessible.ignored: true

    Rectangle {
        width: parent.width
        height: Math.max(0, parent.height - root.captionHeight)
        radius: root.cornerRadius
        color: DesktopTokens.raised
        border.color: DesktopTokens.seam
    }
    Column {
        y: parent.height - root.captionHeight + DesktopTokens.px(8)
        width: parent.width
        spacing: DesktopTokens.px(6)
        visible: root.captionHeight > 0
        Rectangle { width: parent.width * 0.78; height: DesktopTokens.px(8); radius: 3; color: DesktopTokens.raisedStrong }
        Rectangle { width: parent.width * 0.42; height: DesktopTokens.px(7); radius: 3; color: DesktopTokens.raised }
    }
}
