import QtQuick
import OpenNOW

Row {
    id: root
    property string keyText: "Enter"
    property string shortcut: keyText
    property string label: "Play"
    property bool compact: false
    spacing: compact ? 6 : 8

    KeyboardGlyph {
        shortcut: root.shortcut
        keySize: root.compact ? 18 : 24
        ink: DesktopTokens.textBody
        Accessible.name: root.keyText
    }
    Text {
        anchors.verticalCenter: parent.verticalCenter
        text: root.label
        color: DesktopTokens.textMuted
        font.family: DesktopTokens.bodyFont
        font.pixelSize: root.compact ? 11 : 14
        font.weight: Font.DemiBold
    }
}
