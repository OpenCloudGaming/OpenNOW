import QtQuick
import QtQuick.Layouts
import OpenNOW

Rectangle {
    id: root
    property string glyph: ""
    property string title: ""
    property string detail: ""
    objectName: "gameDetailsSummaryCard"
    Layout.fillWidth: true; Layout.minimumWidth: 0; Layout.preferredHeight: DesktopTokens.px(68)
    Layout.preferredWidth: DesktopTokens.px(180)
    radius: DesktopTokens.px(16); color: DesktopTokens.raised
    RowLayout {
        anchors.fill: parent; anchors.margins: DesktopTokens.px(12); spacing: DesktopTokens.px(12)
        Rectangle {
            Layout.preferredWidth: DesktopTokens.px(36); Layout.preferredHeight: DesktopTokens.px(36)
            radius: DesktopTokens.px(11); color: DesktopTokens.raised
            // Paper's accent icons sit on their own tile. Never use
            // the fixed dark-ink settings SVGs on a dark surface.
            DesktopSettingsIcon {
                anchors.centerIn: parent; width: DesktopTokens.px(18); height: DesktopTokens.px(18)
                glyph: root.glyph
                ink: Theme.lightMode ? Theme.label : Theme.focus
            }
        }
        ColumnLayout {
            Layout.fillWidth: true; Layout.minimumWidth: 0; spacing: DesktopTokens.px(2)
            Text { Layout.fillWidth: true; Layout.minimumWidth: 0; text: root.title; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Bold }
            Text { Layout.fillWidth: true; Layout.minimumWidth: 0; text: root.detail; textFormat: Text.PlainText; elide: Text.ElideRight; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.smallSize }
        }
    }
}
