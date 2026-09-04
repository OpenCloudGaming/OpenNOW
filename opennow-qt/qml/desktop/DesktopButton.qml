import QtQuick
import QtQuick.Controls
import OpenNOW

Button {
    id: root
    property bool primary: false
    property bool danger: false
    property string shortcutText: ""
    property string glyph: ""
    property int glyphSize: 16
    property int cornerRadius: 10
    height: 36
    implicitWidth: Math.max(68, contentRow.implicitWidth + leftPadding + rightPadding)
    leftPadding: 14
    rightPadding: 14
    focusPolicy: Qt.StrongFocus
    font.family: DesktopTokens.bodyFont
    font.pixelSize: 12
    font.weight: Font.ExtraBold
    background: Rectangle {
        radius: root.cornerRadius
        color: root.primary ? (root.down ? "#D9D9D9" : "#FFFFFFFF")
             : root.danger ? (root.hovered || root.activeFocus ? "#29FF8A80" : "#14FF8A80")
             : (root.hovered || root.activeFocus ? "#1FFFFFFF" : "#0FFFFFFF")
        border.width: root.primary ? 0 : 1
        border.color: root.danger ? "#52FF8A80" : "#1FFFFFFF"
        scale: root.down ? 0.985 : 1
        Behavior on color { ColorAnimation { duration: DesktopTokens.quickDuration } }
        Behavior on scale { NumberAnimation { duration: DesktopTokens.quickDuration; easing.type: Easing.OutCubic } }
        Rectangle {
            anchors.fill: parent
            anchors.margins: -2
            radius: parent.radius + 2
            color: "transparent"
            border.width: 2
            border.color: DesktopTokens.focus
            visible: root.activeFocus
        }
    }
    contentItem: Item {
        implicitWidth: contentRow.implicitWidth
        implicitHeight: contentRow.implicitHeight
        Row {
            id: contentRow
            anchors.centerIn: parent
            spacing: root.glyph !== "" ? 11 : 8
            DesktopGlyph {
                visible: root.glyph !== ""
                anchors.verticalCenter: parent.verticalCenter
                width: root.glyphSize
                height: root.glyphSize
                icon: root.glyph
            }
            Text {
                visible: root.text !== ""
                anchors.verticalCenter: parent.verticalCenter
                text: root.text
                color: root.primary ? "#0A0D14" : root.danger ? "#FFB4AE" : DesktopTokens.textHigh
                font: root.font
            }
            Rectangle {
                visible: root.shortcutText !== ""
                anchors.verticalCenter: parent.verticalCenter
                width: shortcut.implicitWidth + 10
                height: 20
                radius: 5
                color: root.primary ? "#120B0F1A" : "#12FFFFFF"
                Text {
                    id: shortcut
                    anchors.centerIn: parent
                    text: root.shortcutText
                    color: root.primary ? "#990B0F1A" : DesktopTokens.textMuted
                    font.family: DesktopTokens.monoFont
                    font.pixelSize: 9
                    font.weight: Font.DemiBold
                }
            }
        }
    }
}
