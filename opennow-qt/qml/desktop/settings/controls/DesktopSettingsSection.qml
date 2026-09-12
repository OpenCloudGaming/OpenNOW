import QtQuick
import OpenNOW
Item {
    property string text: ""
    width: parent.width
    height: DesktopTokens.px(42)
    Text {
        x: DesktopTokens.px(20); y: DesktopTokens.px(18)
        text: parent.text; color: Theme.textMuted
        font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(12)
        font.weight: Font.ExtraBold; font.letterSpacing: 1.2
    }
}
