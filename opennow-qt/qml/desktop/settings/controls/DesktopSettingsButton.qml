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
    property string keySequence: ""

    implicitHeight: menu ? DesktopTokens.px(40) : compact ? DesktopTokens.px(30) : DesktopTokens.controlHeight
    implicitWidth: Math.max(compact ? 68 : 84, (keySequence !== "" ? bindingGlyph.implicitWidth : label.implicitWidth) + 28
        + (menu ? 22 : 0) + (suffix !== "" ? suffixGlyph.implicitWidth + 8 : 0))
    hoverEnabled: true
    padding: 0
    leftPadding: 14
    rightPadding: 14
    topPadding: 0
    bottomPadding: 0

    background: Rectangle {
        radius: control.menu ? height / 2 : compact ? 9 : 10
        color: control.primary ? Theme.focus
             : control.danger ? Qt.rgba(1, 0.32, 0.32, control.down ? 0.18 : 0.09)
             : control.menu ? (Theme.lightMode ? Qt.rgba(0,0,0,0.04) : Qt.rgba(0,0,0,0.35))
             : control.down || control.hovered ? DesktopTokens.raisedStrong : DesktopTokens.raised
        border.width: control.activeFocus ? 2 : 1
        border.color: control.activeFocus ? DesktopTokens.focus
                    : control.danger ? Qt.rgba(1, 0.48, 0.48, 0.28)
                    : Theme.seam
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
                visible: control.keySequence === ""
                text: control.text
                width: Math.max(0, Math.min(implicitWidth, control.availableWidth
                    - (control.menu ? 22 : 0) - (control.suffix !== "" ? suffixGlyph.implicitWidth + 8 : 0)))
                elide: Text.ElideRight
                color: control.primary ? Theme.focusText : control.danger ? (Theme.lightMode ? "#9F1239" : "#FFC2C2") : Theme.label
                font.family: Theme.bodyFont
                font.pixelSize: DesktopTokens.px(13)
                font.weight: Font.Bold
                anchors.verticalCenter: parent.verticalCenter
            }
            KeyboardGlyph {
                id: bindingGlyph
                visible: control.keySequence !== ""
                shortcut: control.keySequence
                keySize: DesktopTokens.px(22)
                ink: control.primary ? Theme.focusText : Theme.label
                anchors.verticalCenter: parent.verticalCenter
            }
            KeyboardGlyph {
                id: suffixGlyph
                visible: control.suffix !== ""
                shortcut: control.suffix
                keySize: 18
                ink: control.primary ? Theme.focusText : Theme.textMuted
                anchors.verticalCenter: parent.verticalCenter
            }
            DesktopSettingsIcon {
                visible: control.menu
                width: 10
                height: 10
                glyph: "chevron"; rotation: 90
                ink: control.primary ? Theme.focusText : Theme.textMuted
                anchors.verticalCenter: parent.verticalCenter
            }
        }
    }
}
