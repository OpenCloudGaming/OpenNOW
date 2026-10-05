import QtQuick
import OpenNOW

QtObject {
    property int key: 0
    property int modifiers: 0
    property int phase: 0
    property Item screen: null
    property Item rows: null
    property Item sections: null
    property Item themesSection: null

    function check(ok, message) { if (!ok) throw new Error("Console settings: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function press(nextKey, nextPhase) {
        key = nextKey
        modifiers = Qt.NoModifier
        phase = nextPhase
        return 0
    }
    function rowIndex(settingKey) {
        return screen.settingsModel().findIndex(row => row.key === settingKey)
    }
    function sharedResolutionChoices() {
        return ShellStore.resolutionItems().filter(item => item.kind !== "heading")
    }

    function checkResolutionParity() {
        screen.selectedSection = 2
        ShellStore.applySetting("resolution", "1920x1080")
        const shared = sharedResolutionChoices()
        let row = screen.settingsModel().find(item => item.key === "resolution")
        check(row, "the console exposes a resolution row")
        check(JSON.stringify(row.values) === JSON.stringify(shared.map(item => item.value)),
            "console lists every shared resolution in the desktop order")
        check(JSON.stringify(row.disabledValues) === JSON.stringify(shared.filter(item => item.disabled).map(item => item.value)),
            "console shares the desktop entitlement decisions")
        check(JSON.stringify(row.labels) === JSON.stringify(shared.map(item => item.label)),
            "console shows the shared resolution labels")
        ShellStore.applySetting("resolution", "1366x768")
        row = screen.settingsModel().find(item => item.key === "resolution")
        check(row.values[0] === "1366x768" && row.values.length === shared.length + 1,
            "a saved resolution outside the shared list stays selectable")
        ShellStore.applySetting("resolution", "1920x1080")
    }

    function checkAccentSwatches() {
        screen.selectedSection = 5
        ShellStore.applySetting("themeAccentOverride", true)
        for (const accent of ["rose", "violet"]) {
            ShellStore.applySetting("appAccentColor", accent)
            const row = screen.settingsModel().find(item => item.key === "appAccentColor")
            check(row && row.labels[1] === "Sky", "the second swatch is Sky")
            check(Qt.colorEqual(row.colors[1], Theme.accentColor("blue", Theme.lightMode)),
                "Sky keeps its own colour while " + accent + " is the active accent")
            check(Qt.colorEqual(row.colors[3], Theme.accentColor("green", Theme.lightMode)),
                "Mint keeps its own colour while " + accent + " is the active accent")
        }
        ShellStore.applySetting("appAccentColor", "blue")
        ShellStore.applySetting("themeAccentOverride", false)
    }

    function checkRetainedDelegate(section, settingKey, values) {
        screen.selectedSection = section
        const index = rowIndex(settingKey)
        check(index >= 0, settingKey + " row exists")
        rows.currentIndex = index
        rows.positionViewAtIndex(index, ListView.Center)
        const delegate = rows.itemAtIndex(index)
        check(delegate !== null, settingKey + " delegate is created")
        for (const value of values) {
            ShellStore.applySetting(settingKey, value)
            check(rows.itemAtIndex(index) === delegate, settingKey + " delegate survives a settings update")
            check(rows.currentIndex === index, settingKey + " keeps row focus across a settings update")
            check(delegate.rowData.key === settingKey, settingKey + " delegate still shows the same row")
        }
    }

    function checkInsertedRowKeepsFocus() {
        screen.selectedSection = 2
        const index = rowIndex("fps")
        rows.currentIndex = index
        const original = screen.settingsModel()
        screen.rows = [{t:"Capability row inserted before frame rate", info:true}].concat(original)
        check(rows.count === original.length + 1, "the inserted row is shown")
        check(rows.currentIndex === index + 1 && screen.rows[rows.currentIndex].key === "fps",
            "focus follows frame rate when a row is inserted before it")
        screen.rows = Qt.binding(() => screen.settingsModel())
        check(rows.currentIndex === index && screen.rows[rows.currentIndex].key === "fps",
            "focus follows frame rate when the inserted row goes away")
    }

    function run(parent) {
        screen = find(parent, "consoleSettingsScreen")
        check(screen !== null, "the production console settings screen is shown")
        rows = find(screen, "consoleSettingsList")
        sections = find(screen, "consoleSettingsSections")
        check(rows !== null && sections !== null, "rows and sections exist")
        checkResolutionParity()
        checkAccentSwatches()
        const fpsValues = ShellStore.canonicalFpsValues()
        checkRetainedDelegate(2, "fps", [fpsValues[0], fpsValues[1], fpsValues[0]])
        checkRetainedDelegate(1, "codec", ["h264", "auto", "h265"])
        checkInsertedRowKeepsFocus()
        return true
    }

    function advance() {
        if (phase === 0) {
            screen.selectedSection = 5
            rows.forceActiveFocus()
            themesSection = sections.itemAtIndex(5)
            check(themesSection !== null && sections.currentIndex === 5, "the Themes rail item is current")
            ShellStore.applySetting("themeAccentOverride", true)
            ShellStore.applySetting("appAccentColor", "rose")
            check(sections.itemAtIndex(5) === themesSection, "an accent change keeps the rail delegates")
            check(sections.currentIndex === 5 && screen.selectedSection === 5, "an accent change keeps Themes selected")
            return press(Qt.Key_Escape, 1)
        }
        if (phase === 1) {
            check(sections.activeFocus && screen.selectedSection === 5, "B moves focus to the Themes rail item")
            return press(Qt.Key_Up, 2)
        }
        if (phase === 2) {
            check(sections.activeFocus && screen.selectedSection === 4, "D-pad up starts from Themes after B")
            return press(Qt.Key_Down, 3)
        }
        if (phase === 3) {
            check(screen.selectedSection === 5, "D-pad down returns to Themes")
            return press(Qt.Key_Down, 4)
        }
        if (phase === 4) {
            check(screen.selectedSection === 6, "repeated D-pad down keeps moving through sections")
            return press(Qt.Key_Right, 5)
        }
        if (phase === 5) {
            check(rows.activeFocus, "D-pad right returns to the rows")
            ShellStore.applySetting("appAccentColor", "blue")
            ShellStore.applySetting("themeAccentOverride", false)
            return 1
        }
        return 0
    }
}
