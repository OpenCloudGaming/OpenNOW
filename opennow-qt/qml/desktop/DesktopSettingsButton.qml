import QtQuick
import QtQuick.Controls
import OpenNOW

Button {
    id: control
    property bool primary: false
    property bool danger: false
    property bool compact: false
    property bool menu: false
    property string suffix: ""

    implicitHeight: compact ? DesktopTokens.px(30) : DesktopTokens.controlHeight
    implicitWidth: Math.max(compact ? 68 : 84, contentRow.implicitWidth + 28)
    hoverEnabled: true
    padding: 0
    leftPadding: 14
    rightPadding: 14
    topPadding: 0
    bottomPadding: 0

    background: Rectangle {
        radius: compact ? 9 : 10
        color: control.primary ? Theme.face
             : control.danger ? Qt.rgba(1, 0.32, 0.32, control.down ? 0.18 : 0.09)
             : Qt.rgba(1, 1, 1, control.down ? 0.16 : control.hovered ? 0.12 : 0.08)
        border.width: control.activeFocus ? 2 : 1
        border.color: control.activeFocus ? DesktopTokens.focus
                    : control.danger ? Qt.rgba(1, 0.48, 0.48, 0.28)
                    : Qt.rgba(1, 1, 1, 0.13)
        Behavior on color { ColorAnimation { duration: Theme.focusDuration } }
    }

    contentItem: Item {
        implicitWidth: contentRow.implicitWidth
        implicitHeight: contentRow.implicitHeight
        Row {
            id: contentRow
            anchors.centerIn: parent
            spacing: 8
            Text {
                id: label
                text: control.text
                color: control.primary ? Theme.faceText : control.danger ? "#FFC2C2" : Theme.label
                font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.smallSize
                font.weight: Font.Bold
                anchors.verticalCenter: parent.verticalCenter
            }
            Rectangle {
                visible: control.suffix !== ""
                width: suffixText.implicitWidth + 10
                height: 18
                radius: 5
                color: control.primary ? Qt.rgba(0.05, 0.07, 0.11, 0.08) : Qt.rgba(1, 1, 1, 0.07)
                anchors.verticalCenter: parent.verticalCenter
                Text {
                    id: suffixText
                    anchors.centerIn: parent
                    text: control.suffix
                    color: control.primary ? Qt.rgba(0.05, 0.07, 0.11, 0.52) : Theme.textMuted
                    font.family: Theme.monoFont
                    font.pixelSize: 9
                    font.weight: Font.Bold
                }
            }
            DesktopGlyph {
                visible: control.menu
                width: 10
                height: 6
                icon: "desktop-chevron-down.svg"
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
}
