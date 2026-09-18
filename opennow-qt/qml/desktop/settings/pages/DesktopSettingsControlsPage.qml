import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

Column {
    id: controlsRoot
    objectName: "desktopControllerSettings"
    required property real availableWidth
    required property var settingsScreen
    required property Component controllersPageComponent
    required property Component shortcutsPageComponent

    property bool shortcutsOpen: controlsRoot.settingsScreen.selectedSection === 10
    width: controlsRoot.availableWidth; spacing: DesktopTokens.px(20)
    Loader { width: parent.width; sourceComponent: controlsRoot.controllersPageComponent }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("MOUSE & KEYBOARD") }
        DesktopSettingsRow { width: parent.width; paperStyle: true; glyph: "arrows"; title: qsTr("Mouse sensitivity"); description: qsTr("Applied to native relative mouse input")
            Column {
                width: DesktopTokens.settingsControlWidth
                spacing: DesktopTokens.px(6)
                DesktopSettingsSlider { width: parent.width; from: 0.1; to: 3; stepSize: 0.05; decimals: 2; suffix: "×"; value: Number(controlsRoot.settingsScreen.valueSetting("mouseSensitivity",1)); onCommitted: value => controlsRoot.settingsScreen.setSetting("mouseSensitivity",value) }
                Text {
                    width: parent.width
                    text: qsTr("Follows the remote cursor · F8 toggles capture")
                    color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                    wrapMode: Text.WordWrap
                }
            }
        }
        DesktopSettingsChoice {
            objectName: "keyboardLayoutChoice"
            width: parent.width; title: qsTr("Keyboard layout"); glyph: "keyboard"
            description: ShellStore.settingsOwnerState.keyboardLayoutDescription
            items: ShellStore.settingsOwnerState.keyboardLayoutItems
            value: controlsRoot.settingsScreen.valueSetting("keyboardLayout", "en-US")
            onSelected: value => controlsRoot.settingsScreen.setChoice("keyboardLayout", value)
        }
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "keyboard"; title: qsTr("Clipboard paste")
            description: qsTr("Paste local text into the stream with Ctrl+V (Command+V on macOS). Up to 64 KiB per paste. No automatic clipboard sync."); showDivider: false
            DesktopSettingsToggle {
                objectName: "clipboardPasteToggle"
                checked: controlsRoot.settingsScreen.boolSetting("clipboardPaste", false)
                onValueChangedByUser: value => controlsRoot.settingsScreen.setSetting("clipboardPaste", value)
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsChoice {
            objectName: "gameLanguageChoice"
            width: parent.width; title: qsTr("Game language")
            description: ShellStore.settingsOwnerState.gameLanguageDescription
            items: ShellStore.settingsOwnerState.gameLanguageItems
            value: controlsRoot.settingsScreen.valueSetting("gameLanguage", "en_US")
            onSelected: value => controlsRoot.settingsScreen.setChoice("gameLanguage", value)
            showDivider: false
            footer: RowLayout {
                width: parent.width
                visible: ShellStore.settingsOwnerState.languageState !== "success"
                spacing: DesktopTokens.px(12)
                Text {
                    Layout.fillWidth: true
                    text: ShellStore.settingsOwnerState.languageStatusText
                    color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
                    wrapMode: Text.WordWrap
                }
                DesktopSettingsButton {
                    objectName: "retryGameLanguages"
                    text: qsTr("Retry")
                    visible: ["error", "stale"].indexOf(ShellStore.settingsOwnerState.languageState) >= 0
                    enabled: ShellStore.settingsOwnerState.ready
                    onClicked: ShellStore.settingsOwnerState.ensureGameLanguages(true)
                }
            }
        }
    }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "keyboard"; title: qsTr("Shortcuts")
            objectName: "renewShortcutsDisclosure"
            description: qsTr("Local shortcuts are consumed before gameplay input"); showDivider: false; expandable: true
            expanded: controlsRoot.shortcutsOpen
            onExpansionRequested: controlsRoot.shortcutsOpen = !controlsRoot.shortcutsOpen
            Row { spacing: DesktopTokens.px(10)
                DesktopKeyHint { keyText: String(controlsRoot.settingsScreen.valueSetting("shortcutToggleStats","Ctrl+N")); label: qsTr("stats") }
                DesktopKeyHint { keyText: "Ctrl G"; label: qsTr("menu") }
                DesktopKeyHint { keyText: "F11"; label: qsTr("fullscreen") }
            }
        }
    }
    DesktopSettingsDisclosure { objectName: "renewInlineShortcuts"; width: parent.width; expanded: controlsRoot.shortcutsOpen; sourceComponent: controlsRoot.shortcutsPageComponent }
    DesktopSettingsAdvanced { detail: qsTr("Cursor"); expanded: controlsRoot.settingsScreen.advancedOpen; onClicked: controlsRoot.settingsScreen.advancedOpen = !controlsRoot.settingsScreen.advancedOpen }
    DesktopSettingsDisclosure {
        width: parent.width; expanded: controlsRoot.settingsScreen.advancedOpen
        sourceComponent: DesktopSettingsPanel {
            width: controlsRoot.availableWidth; paperStyle: true
            DesktopSettingsSection { text: qsTr("KEYBOARD & CURSOR") }
            DesktopSettingsRow { width: parent.width; paperStyle: true; glyph: "mouse"; title: qsTr("Cursor overlay"); showDivider: false
                DesktopSettingsToggle { checked: controlsRoot.settingsScreen.boolSetting("nativeCursorOverlay",true); onValueChangedByUser: value => controlsRoot.settingsScreen.setSetting("nativeCursorOverlay",value) }
            }
        }
    }
}
