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
        width: parent.width; glyph: "globe"; title: qsTr("Language"); description: qsTr("Community translated through Crowdin")
        items: [{label:qsTr("System"),value:"system"},{label:"Deutsch",value:"de"},{label:"English",value:"en"},{label:"Español",value:"es"},{label:"Français",value:"fr"},{label:qsTr("Japanese"),value:"ja"},{label:qsTr("Korean"),value:"ko"},{label:"Nederlands",value:"nl"},{label:"Polski",value:"pl"},{label:"Română",value:"ro"},{label:"Русский",value:"ru"},{label:"Türkçe",value:"tr"},{label:qsTr("Chinese"),value:"zh"}]
        value: page.settingsScreen.valueSetting("appLanguage","en")
        onSelected: value => page.settingsScreen.setChoice("appLanguage",value)
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "sidebar"; title: qsTr("Collapsed sidebar")
        description: qsTr("Show icons only · Ctrl B toggles")
        DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("desktopRailCollapsed",true); onValueChangedByUser: value => page.settingsScreen.setSetting("desktopRailCollapsed",value) }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "wave"; title: qsTr("Reduce motion")
        description: qsTr("Cuts parallax and cover animations · follows your OS by default")
        DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("reducedMotion",false); onValueChangedByUser: value => page.settingsScreen.setSetting("reducedMotion",value) }
    }
    DesktopSettingsRow {
        width: parent.width; paperStyle: true; glyph: "sun"; title: qsTr("Translucent interface")
        description: qsTr("Use translucent shell surfaces when supported"); showDivider: false
        DesktopSettingsToggle { checked: page.settingsScreen.boolSetting("translucentUI",false); onValueChangedByUser: value => page.settingsScreen.setSetting("translucentUI",value) }
    }
}
