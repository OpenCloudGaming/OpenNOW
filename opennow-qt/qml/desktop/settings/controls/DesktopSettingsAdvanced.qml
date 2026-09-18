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
    DesktopSettingsIcon { x: DesktopTokens.px(30); anchors.verticalCenter: parent.verticalCenter; width: DesktopTokens.px(20); height: width; glyph: "sliders"; ink: Theme.textMuted }
    Text { id: title; x: DesktopTokens.settingsLabelInset; anchors.verticalCenter: parent.verticalCenter; text: qsTr("Advanced"); color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(15); font.weight: Font.Bold }
    Text { anchors.left: title.right; anchors.leftMargin: DesktopTokens.px(10); anchors.right: arrow.left; anchors.rightMargin: DesktopTokens.px(16); anchors.verticalCenter: parent.verticalCenter; text: root.detail; elide: Text.ElideRight; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize }
    DesktopSettingsIcon {
        id: arrow; anchors.right: parent.right; anchors.rightMargin: DesktopTokens.settingsInset; anchors.verticalCenter: parent.verticalCenter
        width: DesktopTokens.px(14); height: width; glyph: "chevron"; rotation: root.expanded ? 90 : 0; ink: Theme.textMuted
        Behavior on rotation { enabled: !AppController.reducedMotion; NumberAnimation { duration: 160; easing.type: Easing.OutCubic } }
    }
}
