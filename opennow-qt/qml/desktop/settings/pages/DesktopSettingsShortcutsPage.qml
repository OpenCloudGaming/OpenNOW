import QtQuick
import QtQuick.Controls
import OpenNOW

Column {
    id: shortcutsPageRoot
    objectName: "desktopShortcutsPage"
    required property real availableWidth
    required property var settingsScreen

    width: shortcutsPageRoot.availableWidth; spacing: DesktopTokens.px(14)
    property string shortcutQuery: ""
    property string filter: "all"
    property string captureKey: ""
    property bool confirmingReset: false
    property bool resetAllPending: false
    property string resetAllError: ""
    readonly property var changedBindings: shortcutBinding.changedBindings()
    readonly property int changedCount: Object.keys(changedBindings).length
    readonly property int unsetCount: shortcutBinding.commands.filter(command => shortcutBinding.value(command.key) === "").length
    readonly property int totalCount: shortcutBinding.commands.length
        + fixedGroups.reduce((total, group) => total + group.rows.length, 0)
    readonly property var filterValues: ["all", "changed", "unset"]
    readonly property var fixedGroups: [
        {h: qsTr("IN STREAM"), rows: [{l: qsTr("Session menu"), k: "Ctrl+G", locked: true}]},
        {h: qsTr("APP"), rows: [{l: qsTr("Command palette"), k: "Ctrl  K"}, {l: qsTr("Search this page"), k: "/"},
            {l: qsTr("Collapse or expand the sidebar"), k: "Ctrl  B"}, {l: qsTr("Switch to console mode"), k: "F10"},
            {l: qsTr("Settings"), k: "Ctrl  ,"}, {l: qsTr("Quit OpenNOW"), k: "Ctrl  Q"}]},
        {h: qsTr("LIBRARY AND STORE"), rows: [{l: qsTr("Move through covers"), k: "Arrows"}, {l: qsTr("Play or resume"), k: "Enter"},
            {l: qsTr("Game details"), k: "Space"}, {l: qsTr("Toggle favourite"), k: "F"}, {l: qsTr("Context menu"), k: "Shift  F10"}]},
        {h: qsTr("GAMEPAD · CONSOLE MODE"), rows: [{l: qsTr("Select · back"), k: "A · B", gamepad: true},
            {l: qsTr("Details · favourite"), k: "X · Y", gamepad: true}, {l: qsTr("Switch tab"), k: "LB · RB", gamepad: true},
            {l: qsTr("Stats overlay"), k: "Guide", gamepad: true}]}
    ]

    DesktopSettingsShortcutBinding { id: shortcutBinding }

    function displayBinding(key) {
        const value = shortcutBinding.value(key)
        return value === "" ? qsTr("Not set") : value
    }

    function allShortcutGroups() {
        const inStream = fixedGroups[0].rows.concat(shortcutBinding.commands.map(command => ({
            l: command.title, k: displayBinding(command.key), setting: command.key})))
        return [{h: fixedGroups[0].h, rows: inStream}].concat(fixedGroups.slice(1))
    }

    function matches(label, keys, group) {
        const query = shortcutQuery.trim().toLocaleLowerCase()
        return query === "" || [label, keys, group].some(text => String(text).toLocaleLowerCase().indexOf(query) >= 0)
    }

    function fixedRowVisible(row, group) {
        return filter === "all" && matches(row.l, row.k, group)
    }

    function commandVisible(command) {
        const value = shortcutBinding.value(command.key)
        if (filter === "changed" && shortcutBinding.isDefault(command.key))
            return false
        if (filter === "unset" && value !== "")
            return false
        return matches(command.title, value, fixedGroups[0].h)
    }

    function groupVisible(index) {
        if (fixedGroups[index].rows.some(row => fixedRowVisible(row, fixedGroups[index].h)))
            return true
        return index === 0 && shortcutBinding.commands.some(command => commandVisible(command))
    }

    function visibleRows() {
        const rows = []
        for (let i = 0; i < commandRows.count; ++i) {
            const row = commandRows.itemAt(i)
            if (row && row.visible)
                rows.push(row)
        }
        return rows
    }

    function focusRow(from, delta) {
        const rows = visibleRows()
        const next = rows[rows.indexOf(from) + delta]
        if (next)
            next.focusBinding()
        else if (delta < 0)
            searchField.forceActiveFocus()
    }

    function requestResetAll() {
        if (changedCount === 0 || ShellStore.shortcutUpdateRequestId !== "")
            return
        resetAllError = ""
        confirmingReset = true
        ShellStore.accessibilityMessage = resetQuestion.text
        confirmResetButton.forceActiveFocus()
    }

    function confirmResetAll() {
        captureKey = ""
        if (ShellStore.updateShortcuts(changedBindings) === "") {
            resetAllError = ShellStore.shortcutUpdateError !== "" ? ShellStore.shortcutUpdateError
                : qsTr("The shortcut could not be saved.")
            ShellStore.accessibilityMessage = resetAllError
            return
        }
        resetAllPending = true
    }

    function cancelResetAll() {
        confirmingReset = false
        resetAllError = ""
        resetAllButton.forceActiveFocus()
    }

    Connections {
        target: ShellStore
        function onShortcutUpdateRequestIdChanged() {
            if (!shortcutsPageRoot.resetAllPending || ShellStore.shortcutUpdateRequestId !== "")
                return
            shortcutsPageRoot.resetAllPending = false
            if (ShellStore.shortcutUpdateError !== "") {
                shortcutsPageRoot.resetAllError = ShellStore.shortcutUpdateError
                ShellStore.accessibilityMessage = ShellStore.shortcutUpdateError
                return
            }
            shortcutsPageRoot.confirmingReset = false
            ShellStore.accessibilityMessage = qsTr("Shortcuts reset to their defaults")
            searchField.forceActiveFocus()
        }
    }

    DesktopSettingsPanel {
        width: parent.width; paperStyle: true
        DesktopSettingsSection {
            text: qsTr("SHORTCUTS")
            description: qsTr("Local shortcuts are handled before gameplay input. A cleared shortcut sends its key to the game.")
            DesktopSettingsButton {
                id: resetAllButton
                objectName: "shortcutResetAll"
                visible: !shortcutsPageRoot.confirmingReset
                enabled: shortcutsPageRoot.changedCount > 0 && ShellStore.shortcutUpdateRequestId === ""
                text: shortcutsPageRoot.changedCount > 0 ? qsTr("Reset all (%1)").arg(shortcutsPageRoot.changedCount) : qsTr("Reset all")
                Accessible.name: text
                onClicked: shortcutsPageRoot.requestResetAll()
            }
        }
        Item {
            objectName: "shortcutResetConfirmation"
            visible: shortcutsPageRoot.confirmingReset
            width: parent.width
            height: visible ? DesktopTokens.px(56) : 0
            Text {
                id: resetQuestion
                x: DesktopTokens.settingsInset
                anchors.verticalCenter: parent.verticalCenter
                width: parent.width - x - resetActions.width - DesktopTokens.px(36)
                text: shortcutsPageRoot.resetAllError !== "" ? shortcutsPageRoot.resetAllError
                    : shortcutsPageRoot.changedCount === 1 ? qsTr("Reset 1 shortcut to its default?")
                    : qsTr("Reset %1 shortcuts to their defaults?").arg(shortcutsPageRoot.changedCount)
                color: shortcutsPageRoot.resetAllError !== "" ? (Theme.lightMode ? "#9F1239" : "#FFC2C2") : Theme.label
                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.px(14); font.weight: Font.ExtraBold
                wrapMode: Text.WordWrap
                Accessible.role: Accessible.AlertMessage
                Accessible.name: text
            }
            Row {
                id: resetActions
                anchors.right: parent.right; anchors.rightMargin: DesktopTokens.settingsInset
                anchors.verticalCenter: parent.verticalCenter
                spacing: DesktopTokens.px(8)
                DesktopSettingsButton {
                    objectName: "shortcutResetAllCancel"
                    text: qsTr("Cancel")
                    onClicked: shortcutsPageRoot.cancelResetAll()
                    Keys.onEscapePressed: shortcutsPageRoot.cancelResetAll()
                }
                DesktopSettingsButton {
                    id: confirmResetButton
                    objectName: "shortcutResetAllConfirm"
                    primary: true
                    enabled: !shortcutsPageRoot.resetAllPending
                    text: qsTr("Reset")
                    onClicked: shortcutsPageRoot.confirmResetAll()
                    Keys.onEscapePressed: shortcutsPageRoot.cancelResetAll()
                }
            }
        }
        Item {
            width: parent.width; height: DesktopTokens.px(64)
            DesktopSettingsField {
                id: searchField
                objectName: "shortcutSearch"
                x: DesktopTokens.settingsInset; anchors.verticalCenter: parent.verticalCenter
                width: parent.width - x - filterChoice.width - DesktopTokens.px(12) - DesktopTokens.settingsInset
                placeholderText: qsTr("Search commands or bindings…")
                Accessible.name: qsTr("Search shortcuts")
                onTextChanged: shortcutsPageRoot.shortcutQuery = text
                Keys.onEscapePressed: event => {
                    event.accepted = text !== ""
                    text = ""
                }
                Keys.onDownPressed: {
                    const rows = shortcutsPageRoot.visibleRows()
                    if (rows.length > 0)
                        rows[0].focusBinding()
                }
            }
            DesktopSettingsSegmented {
                id: filterChoice
                objectName: "shortcutFilter"
                anchors.right: parent.right; anchors.rightMargin: DesktopTokens.settingsInset
                anchors.verticalCenter: parent.verticalCenter
                selectedIndex: shortcutsPageRoot.filterValues.indexOf(shortcutsPageRoot.filter)
                options: [
                    {label: qsTr("All (%1)").arg(shortcutsPageRoot.totalCount), value: "all", width: 84},
                    {label: qsTr("Changed (%1)").arg(shortcutsPageRoot.changedCount), value: "changed", width: 118},
                    {label: qsTr("Not set (%1)").arg(shortcutsPageRoot.unsetCount), value: "unset", width: 106}
                ]
                onSelected: (index, value) => shortcutsPageRoot.filter = value
            }
        }
    }

    DesktopSettingsPanel {
        objectName: "shortcutGroup-stream"
        width: parent.width; paperStyle: true
        visible: shortcutsPageRoot.groupVisible(0)
        DesktopSettingsSection {
            text: shortcutsPageRoot.fixedGroups[0].h
            Text {
                text: qsTr("Select a binding to change it")
                color: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Bold
            }
        }
        DesktopSettingsRow {
            objectName: "shortcutRow-sessionMenu"
            readonly property var rowData: shortcutsPageRoot.fixedGroups[0].rows[0]
            visible: shortcutsPageRoot.fixedRowVisible(rowData, shortcutsPageRoot.fixedGroups[0].h)
            width: parent.width; paperStyle: true; glyph: "keyboard"
            title: rowData.l
            description: qsTr("Always available in a stream")
            rowHeight: DesktopTokens.px(56)
            Accessible.name: qsTr("%1, %2, reserved").arg(rowData.l).arg(rowData.k)
            Row {
                spacing: DesktopTokens.px(10)
                rightPadding: DesktopTokens.px(80)
                DesktopSettingsIcon {
                    anchors.verticalCenter: parent.verticalCenter
                    width: DesktopTokens.px(14); height: width; glyph: "lock"; ink: Theme.textMuted
                }
                Text {
                    anchors.verticalCenter: parent.verticalCenter
                    text: qsTr("Reserved")
                    color: Theme.textMuted
                    font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Bold
                }
                KeyboardGlyph { anchors.verticalCenter: parent.verticalCenter; shortcut: "Ctrl  G"; keySize: DesktopTokens.px(24) }
            }
        }
        Repeater {
            id: commandRows
            model: shortcutBinding.commands
            delegate: DesktopSettingsShortcutRow {
                id: commandRow
                required property var modelData
                settingKey: modelData.key
                title: modelData.title
                binding: shortcutBinding
                visible: shortcutsPageRoot.commandVisible(modelData)
                capturing: shortcutsPageRoot.captureKey === modelData.key
                onCaptureRequested: shortcutsPageRoot.captureKey = modelData.key
                onCaptureFinished: if (shortcutsPageRoot.captureKey === modelData.key) shortcutsPageRoot.captureKey = ""
                onNavigationRequested: delta => shortcutsPageRoot.focusRow(commandRow, delta)
                onSearchRequested: searchField.forceActiveFocus()
            }
        }
    }

    Repeater {
        model: shortcutsPageRoot.fixedGroups.slice(1)
        delegate: DesktopSettingsPanel {
            id: fixedPanel
            required property var modelData
            required property int index
            width: shortcutsPageRoot.width; paperStyle: true
            visible: shortcutsPageRoot.groupVisible(index + 1)
            DesktopSettingsSection {
                text: fixedPanel.modelData.h
                Text {
                    text: qsTr("Fixed")
                    color: Theme.textMuted
                    font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.captionSize; font.weight: Font.Bold
                }
            }
            Repeater {
                model: fixedPanel.modelData.rows
                delegate: DesktopSettingsRow {
                    required property var modelData
                    visible: shortcutsPageRoot.fixedRowVisible(modelData, fixedPanel.modelData.h)
                    width: parent.width; paperStyle: true; glyph: modelData.gamepad ? "controller" : "keyboard"; title: modelData.l
                    rowHeight: DesktopTokens.px(56)
                    Row {
                        spacing: DesktopTokens.px(6)
                        rightPadding: DesktopTokens.px(80)
                        KeyboardGlyph { visible: !modelData.gamepad; shortcut: modelData.gamepad ? "" : modelData.k; keySize: DesktopTokens.px(26) }
                        Repeater {
                            model: modelData.gamepad ? modelData.k.toUpperCase().split(" · ") : []
                            ControllerGlyph { required property string modelData; glyph: modelData; label: ""; glyphSize: DesktopTokens.px(26) }
                        }
                    }
                }
            }
        }
    }

    Column {
        objectName: "shortcutEmptyState"
        visible: ![0, 1, 2, 3].some(index => shortcutsPageRoot.groupVisible(index))
        width: parent.width
        spacing: DesktopTokens.px(12)
        topPadding: DesktopTokens.px(12)
        Text {
            width: parent.width; horizontalAlignment: Text.AlignHCenter
            text: shortcutsPageRoot.shortcutQuery !== ""
                ? qsTr("No bindings match “%1”.").arg(shortcutsPageRoot.shortcutQuery)
                : shortcutsPageRoot.filter === "changed" ? qsTr("Every shortcut uses its default.") : qsTr("Every shortcut is assigned.")
            color: Theme.label; font.family: Theme.bodyFont; font.pixelSize: DesktopTokens.bodySize; font.weight: Font.ExtraBold
            wrapMode: Text.WordWrap
        }
        DesktopSettingsButton {
            objectName: "shortcutShowAll"
            anchors.horizontalCenter: parent.horizontalCenter
            text: shortcutsPageRoot.shortcutQuery !== "" ? qsTr("Clear search") : qsTr("Show all")
            onClicked: {
                searchField.text = ""
                shortcutsPageRoot.filter = "all"
                searchField.forceActiveFocus()
            }
        }
    }
}
