import QtQuick
import QtQuick.Controls
import OpenNOW
AbstractButton {
    id: root
    property string detail: ""
    property bool expanded: false
    width: parent.width
    implicitHeight: DesktopTokens.px(56)
    hoverEnabled: true
    background: Rectangle {
        radius: DesktopTokens.px(16)
        color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, root.hovered ? 0.9 : 0.72)
        border.width: root.activeFocus ? 1 : 0; border.color: Theme.focus
    }
    DesktopSettingsIcon { x: 30; anchors.verticalCenter: parent.verticalCenter; width: 20; height: 20; glyph: "sliders"; ink: Theme.textMuted }
    Text { id: title; x: 76; anchors.verticalCenter: parent.verticalCenter; text: qsTr("Advanced"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(15); font.weight: Font.Bold }
    Text { anchors.left: title.right; anchors.leftMargin: 10; anchors.right: arrow.left; anchors.rightMargin: 16; anchors.verticalCenter: parent.verticalCenter; text: root.detail; elide: Text.ElideRight; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(12.5) }
    DesktopSettingsIcon {
        id: arrow; anchors.right: parent.right; anchors.rightMargin: 29; anchors.verticalCenter: parent.verticalCenter
        width: 14; height: 14; glyph: "chevron"; rotation: root.expanded ? 90 : 0; ink: Theme.textMuted
        Behavior on rotation { enabled: !AppController.reducedMotion; NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
    }
}
