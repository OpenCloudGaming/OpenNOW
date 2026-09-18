import QtQuick
import QtQuick.Controls
import OpenNOW

Rectangle {
    id: root
    property string text: ""
    property bool previousEnabled: true
    property bool nextEnabled: true
    signal previous()
    signal next()
    signal openRequested()
    function focusSelector() { selector.forceActiveFocus() }
    implicitWidth: DesktopTokens.px(190)
    implicitHeight: DesktopTokens.px(40)
    radius: height / 2
    color: Theme.lightMode ? Qt.rgba(0,0,0,0.04) : Qt.rgba(0,0,0,0.35)
    border.width: 1; border.color: Theme.seam
    Row {
        anchors.centerIn: parent; spacing: DesktopTokens.px(2)
        AbstractButton {
            width: DesktopTokens.px(30); height: width; enabled: root.previousEnabled
            Accessible.name: qsTr("Previous option"); onClicked: root.previous()
            background: Rectangle { radius: width/2; color: parent.activeFocus || parent.hovered ? DesktopTokens.raised : "transparent" }
            DesktopSettingsIcon { anchors.centerIn: parent; width: DesktopTokens.px(14); height: width; glyph: "chevron"; rotation: 180; ink: Theme.textMuted; opacity: parent.enabled ? 1 : 0.3 }
        }
        AbstractButton {
            id: selector
            width: DesktopTokens.px(112); height: DesktopTokens.px(30)
            Accessible.name: root.text; onClicked: root.openRequested()
            Keys.onLeftPressed: event => { if (root.previousEnabled) root.previous(); event.accepted = true }
            Keys.onRightPressed: event => { if (root.nextEnabled) root.next(); event.accepted = true }
            background: Rectangle { radius: height/2; color: parent.activeFocus || parent.hovered ? DesktopTokens.raised : "transparent" }
            Text { anchors.centerIn: parent; text: root.text; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(14); font.weight: Font.ExtraBold }
        }
        AbstractButton {
            width: DesktopTokens.px(30); height: width; enabled: root.nextEnabled
            Accessible.name: qsTr("Next option"); onClicked: root.next()
            background: Rectangle { radius: width/2; color: parent.activeFocus || parent.hovered ? DesktopTokens.raised : "transparent" }
            DesktopSettingsIcon { anchors.centerIn: parent; width: DesktopTokens.px(14); height: width; glyph: "chevron"; ink: Theme.textMuted; opacity: parent.enabled ? 1 : 0.3 }
        }
    }
}
