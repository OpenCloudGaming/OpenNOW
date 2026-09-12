import QtQuick
import OpenNOW

QtObject {
    property var rootItem
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }

    function run(parent) {
        rootItem = parent
        ShellStore.settings = Object.assign({}, ShellStore.settings, {
            codec:"h265", colorQuality:"8bit_420", suppressTenBitWarning:false,
            appTheme:"dark", themePack:"nocturne", desktopUiScale:1.25
        })
        const desktop = find(parent, "desktopSettingsScreen")
        const console = find(parent, "consoleSettingsScreen")
        if (desktop) {
            desktop.setChoice("colorQuality", "10bit_420")
            if (ShellStore.settings.colorQuality !== "10bit_420")
                throw new Error("10-bit selection was not applied")
        } else if (console) {
            const row = console.settingsModel().find(item => item.key === "colorQuality")
            if (!row) throw new Error("Console color quality selection is missing")
            console.openChoices(row)
            console.commitDropdownChoice(row.values.indexOf("10bit_420"))
        } else {
            throw new Error("Settings screen is missing")
        }
        ShellStore.lastError = ""
        return true
    }

    function verify() {
        const checkbox = find(rootItem, "tenBitWarningDontNotify")
        const dismiss = find(rootItem, "tenBitWarningDismiss")
        if (!checkbox || !checkbox.visible || !dismiss || !dismiss.visible)
            throw new Error("Selecting 10-bit did not show the warning controls")
        return true
    }
}
