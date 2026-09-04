import QtQuick
import OpenNOW

Row {
    id: root
    property string keyText: "Enter"
    property string label: "Play"
    spacing: 8

    Rectangle {
        width: keyLabel.implicitWidth + 16
        height: 26
        radius: 7
        color: "#1AFFFFFF"
        border.width: 1
        border.color: DesktopTokens.seam
        Text {
            id: keyLabel
            anchors.centerIn: parent
            text: root.keyText
            color: DesktopTokens.textBody
            font.family: DesktopTokens.monoFont
            font.pixelSize: 12
            font.weight: Font.DemiBold
        }
    }
    Text {
        anchors.verticalCenter: parent.verticalCenter
        text: root.label
        color: DesktopTokens.textMuted
        font.family: DesktopTokens.bodyFont
        font.pixelSize: 14
        font.weight: Font.DemiBold
    }
}
