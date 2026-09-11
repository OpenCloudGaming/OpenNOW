import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import OpenNOW

Column {
    id: shortcutsPageRoot
    required property real availableWidth
    required property var settingsScreen

    width: shortcutsPageRoot.availableWidth; spacing: 14
    property string shortcutQuery: ""
    property bool confirmReset: false

    function allShortcutGroups() {
        return [
            {h:"APP", rows:[{l:"Command palette",k:"Ctrl  K"},{l:"Search this page",k:"/"},{l:"Collapse or expand the sidebar",k:"Ctrl  B"},{l:"Switch to console mode",k:"F10"},{l:"Settings",k:"Ctrl  ,"},{l:"Quit OpenNOW",k:"Ctrl  Q"}]},
            {h:"IN STREAM", rows:[{l:"Session menu",k:"Ctrl+G"},{l:"Stats overlay",k:String(shortcutsPageRoot.settingsScreen.valueSetting("shortcutToggleStats","Ctrl+N"))},{l:"Toggle fullscreen",k:String(shortcutsPageRoot.settingsScreen.valueSetting("shortcutToggleFullscreen","F11"))},{l:"Grab or release the mouse",k:String(shortcutsPageRoot.settingsScreen.valueSetting("shortcutTogglePointerLock","F8"))},{l:"Screenshot the stream",k:String(shortcutsPageRoot.settingsScreen.valueSetting("shortcutScreenshot","Ctrl+F11"))},{l:qsTr("Toggle recording"),k:String(shortcutsPageRoot.settingsScreen.valueSetting("shortcutToggleRecording","F12")),setting:"shortcutToggleRecording"},{l:qsTr("Save replay clip"),k:String(shortcutsPageRoot.settingsScreen.valueSetting("shortcutSaveClip","Ctrl+F12")),setting:"shortcutSaveClip"},{l:"End the session",k:String(shortcutsPageRoot.settingsScreen.valueSetting("shortcutStopStream","Ctrl+Shift+Q"))}]},
            {h:"LIBRARY AND STORE", rows:[{l:"Move through covers",k:"Arrows"},{l:"Play or resume",k:"Enter"},{l:"Game details",k:"Space"},{l:"Toggle favourite",k:"F"},{l:"Context menu",k:"Shift  F10"}]},
            {h:"GAMEPAD · CONSOLE MODE", rows:[{l:"Select · back",k:"A · B",gamepad:true},{l:"Details · favourite",k:"X · Y",gamepad:true},{l:"Switch tab",k:"LB · RB",gamepad:true},{l:"Stats overlay",k:"Guide",gamepad:true}]}
        ]
    }

    function shortcutGroups() {
        const query = shortcutsPageRoot.shortcutQuery.trim().toLocaleLowerCase()
        const groups = shortcutsPageRoot.allShortcutGroups()
        if (query === "")
            return groups
        const result = []
        for (let i = 0; i < groups.length; ++i) {
            const rows = []
            for (let j = 0; j < groups[i].rows.length; ++j) {
                const row = groups[i].rows[j]
                if (String(row.l).toLocaleLowerCase().indexOf(query) >= 0
                        || String(row.k).toLocaleLowerCase().indexOf(query) >= 0
                        || String(groups[i].h).toLocaleLowerCase().indexOf(query) >= 0)
                    rows.push(row)
            }
            if (rows.length > 0)
                result.push({h: groups[i].h, rows: rows})
        }
        return result
    }

    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection { text: qsTr("SHORTCUTS") }
        Item {
            width: parent.width; height: DesktopTokens.px(64)
            DesktopSettingsField {
                x: 20; anchors.verticalCenter: parent.verticalCenter; width: parent.width-40
                placeholderText: qsTr("Search commands or bindings…")
                Accessible.name: qsTr("Search shortcuts")
                onTextChanged: shortcutsPageRoot.shortcutQuery = text
            }
        }
    }
    Column {
        width: parent.width; spacing: 20
        Repeater {
            model: shortcutsPageRoot.shortcutGroups()
            delegate: DesktopSettingsPanel {
                required property var modelData
                width: parent.width; paperStyle: true
                DesktopSettingsSection { text: modelData.h }
                Repeater {
                    model: modelData.rows
                    delegate: DesktopSettingsRow {
                        required property var modelData
                        required property int index
                        width: parent.width; paperStyle: true; glyph: "keyboard"; title: modelData.l
                        rowHeight: DesktopTokens.px(56)
                        Row {
                            visible: !modelData.setting
                            spacing: 6
                            KeyboardGlyph { visible: !modelData.gamepad; shortcut: modelData.gamepad ? "" : modelData.k; keySize: DesktopTokens.px(26) }
                            Repeater {
                                model: modelData.gamepad ? modelData.k.toUpperCase().split(" · ") : []
                                ControllerGlyph { required property string modelData; glyph: modelData; label: ""; glyphSize: DesktopTokens.px(26) }
                            }
                        }
                        DesktopSettingsButton {
                            visible: Boolean(modelData.setting)
                            text: modelData.k
                            keySequence: text
                            Accessible.name: modelData.l + ": " + text
                            onClicked: shortcutEditor.edit(modelData.setting, modelData.l)
                        }
                    }
                }
            }
        }
    }
    Text {
        visible: shortcutsPageRoot.shortcutQuery !== "" && shortcutsPageRoot.shortcutGroups().length === 0
        width: parent.width; horizontalAlignment: Text.AlignHCenter
        text: qsTr("No bindings match “%1”.").arg(shortcutsPageRoot.shortcutQuery)
        color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize
    }
    DesktopSettingsShortcutEditor { id: shortcutEditor }
    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsRow {
            width: parent.width; paperStyle: true; glyph: "sliders"; title: qsTr("Reset all settings")
            description: shortcutsPageRoot.confirmReset ? qsTr("This resets all preferences, not only shortcuts. Continue?") : qsTr("Restore OpenNOW preferences to their defaults")
            showDivider: false
            DesktopSettingsButton { visible: shortcutsPageRoot.confirmReset; text: qsTr("Cancel"); onClicked: shortcutsPageRoot.confirmReset = false }
            DesktopSettingsButton { text: shortcutsPageRoot.confirmReset ? qsTr("Confirm reset") : qsTr("Reset"); danger: true; onClicked: { if (shortcutsPageRoot.confirmReset) { ShellStore.resetSettings(); shortcutsPageRoot.confirmReset = false } else shortcutsPageRoot.confirmReset = true } }
        }
    }
}
