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
    property Component desktopSettings: Component { DesktopSettingsScreen { visible: false } }
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property var calls: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "resolution-fixture-" + (calls.length + 1)
            calls = calls.concat([{id:id, method:method, params:params}])
            return id
        }
        function cancel(id) { return true }
    }

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
        ShellStore.applySetting("resolution", "1234x800")
        row = screen.settingsModel().find(item => item.key === "resolution")
        check(row.values[0] === "1234x800" && row.values.length === shared.length + 1,
            "a saved resolution outside the shared list stays selectable")
        ShellStore.applySetting("resolution", "1366x768")
        row = screen.settingsModel().find(item => item.key === "resolution")
        check(row.values.filter(value => value === "1366x768").length === 1 && row.values.length === shared.length,
            "1366x768 is a shared preset, not an extra current-resolution choice")
        ShellStore.applySetting("resolution", "1920x1080")
    }

    function checkResolutionPresets(parent) {
        const added = [
            ["1112x834", "4:3"], ["1024x768", "4:3"], ["1366x768", "16:9"],
            ["1280x1024", "5:4"], ["1680x720", "21:9"], ["1600x1200", "4:3"], ["3456x2160", "16:10"],
            ["3640x1080", "CUSTOM"]
        ]
        const existing = ["1280x720", "1600x900", "1920x1080", "2560x1440", "3200x1800", "3840x2160", "5120x2880", "7680x4320",
            "1280x800", "1440x900", "1680x1050", "1920x1200", "2560x1600", "3840x2400",
            "2560x1080", "3440x1440", "3840x1600", "3840x1800", "5120x2160", "3840x1080", "5120x1440"]
        const subscription = ShellStore.subscription
        ShellStore.subscription = null
        const items = ShellStore.resolutionItems()
        const choices = sharedResolutionChoices()
        check(choices.length === existing.length + added.length, "all eight requested sizes are added")
        check(new Set(choices.map(item => item.value)).size === choices.length, "all preset sizes are unique")
        check(JSON.stringify(choices.filter(item => existing.indexOf(item.value) >= 0).map(item => item.value)) === JSON.stringify(existing),
            "existing preset sizes and their order are preserved")
        check(choices.some(item => item.value === "3640x1080"), "the requested 3640x1080 dimensions are preserved literally")
        check(choices.every(item => !item.disabled), "offline choices remain available")
        const desktop = desktopSettings.createObject(parent)
        const picker = find(desktop, "renewResolutionPicker")
        check(picker !== null, "the real desktop resolution picker exists")
        for (const preset of added) {
            const index = items.findIndex(item => item.value === preset[0])
            check(index >= 0, preset[0] + " is a shared choice")
            const heading = items.slice(0, index).filter(item => item.kind === "heading").pop()
            check(heading.label.split(" ")[0] === preset[1], preset[0] + " has the correct aspect heading")
            check(items[index].detail === preset[0].replace("x", "×"), preset[0] + " displays exact dimensions")
        }
        for (const entitlement of [null, {entitledResolutions:[]},
                {entitledResolutions:[{width:7680, height:4320, fps:120}]},
                {entitledResolutions:added.map(preset => {
                    const size = preset[0].split("x").map(Number)
                    return {width:size[0], height:size[1], fps:60}
                })},
                {entitledResolutions:[{width:1366, height:768, fps:29}]}]) {
            ShellStore.subscription = entitlement
            check(JSON.stringify(picker.items) === JSON.stringify(ShellStore.resolutionItems()),
                "desktop uses the shared choices and entitlement flags")
            checkResolutionParity()
            for (const preset of added) {
                const exact = entitlement && entitlement.entitledResolutions.some(item =>
                    item.width + "x" + item.height === preset[0] && item.fps >= 30)
                const option = picker.choices.find(item => item.value === preset[0])
                check(option.disabled === Boolean(entitlement && !exact), preset[0] + " requires an exact usable entitlement when loaded")
                check(picker.available.some(item => item.value === preset[0]) === !option.disabled,
                    preset[0] + " cannot be selected by the desktop stepper when unsupported")
            }
        }
        ShellStore.subscription = null
        const owner = ShellStore.settingsOwnerState
        for (const preset of added) {
            for (const surface of ["desktop", "console"]) {
                const callCount = client.calls.length
                if (surface === "desktop") picker.selected(preset[0])
                else screen.applyChoice("resolution", preset[0])
                const write = client.calls.slice(callCount).find(call => call.method === "settings.set" && call.params.key === "resolution")
                check(write && write.params.value === preset[0], surface + " serializes " + preset[0] + " without changing dimensions")
                client.eventReceived("settings.changed", {key:"resolution", value:preset[0]})
                client.responseReceived(write.id, {key:"resolution", value:preset[0]})
                check(owner.confirmedSettings.resolution === preset[0] && ShellStore.settings.resolution === preset[0],
                    surface + " retains the confirmed saved resolution")
                check(picker.value === preset[0] && screen.resolutionLabel(preset[0]) === picker.current.label + " · " + picker.current.detail,
                    "desktop and console reflect the saved preset")
                ShellStore.applySetting("resolution", "1920x1080")
            }
        }
        ShellStore.subscription = subscription
        desktop.destroy()
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
        checkResolutionPresets(parent)
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
