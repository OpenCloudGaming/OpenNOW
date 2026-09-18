import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

DesktopSettingsPanel {
    id: page
    required property real availableWidth
    required property var settingsScreen

    width: page.availableWidth; paperStyle: true
    DesktopSettingsSection { text: qsTr("INTERFACE") }
    DesktopSettingsChoice {
        objectName: "renewLanguageChoice"
        width: parent.width; glyph: "globe"; title: qsTr("Interface language")
        description: ShellStore.settingsOwnerState.interfaceLanguageDescription
        items: ShellStore.settingsOwnerState.interfaceLanguageItems
        value: page.settingsScreen.valueSetting("appLanguage","system")
        onSelected: value => page.settingsScreen.setChoice("appLanguage",value)
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "grid"; title: qsTr("Interface scale"); showDivider: false
        DesktopSettingsSlider { from: 0.85; to: 1.25; stepSize: 0.05; decimals: 2; suffix: "×"; value: Number(page.settingsScreen.valueSetting("desktopUiScale",1)); onCommitted: value => page.settingsScreen.setSetting("desktopUiScale",value) }
    }
}
