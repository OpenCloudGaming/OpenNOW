import QtQuick
import OpenNOW

QtObject {
    function check(ok, message) {
        if (!ok) throw new Error("Theme settings: " + message)
    }

    function find(parent, name) {
        if (parent.objectName === name) return parent
        for (const child of parent.children || []) {
            const match = find(child, name)
            if (match) return match
        }
        return null
    }

    function run(parent) {
        const screen = find(parent, "desktopSettingsScreen")
        check(screen !== null, "desktop settings exists")
        const light = Qt.application.arguments.indexOf("--smoke-light-theme") >= 0
        for (const pack of Theme.packs) {
            screen.setChoice("themePack", pack.id)
            check(Theme.lightMode === (pack.category === "Light"), "pack applies appearance: " + pack.id)
            check(!Theme.accentOverridden, "pack resets override")
            for (const accent of Theme.accentChoices) {
                const swatch = find(parent, "themeAccent-" + accent)
                check(swatch !== null, "accent swatch exists: " + accent)
                swatch.clicked()
                check(swatch.checked && Theme.accentOverridden, "accent selection is active")
                check(Theme.focus === Theme.customAccent, "accent reaches palette")
            }
            const reset = find(parent, "themePackAccent")
            check(reset !== null, "pack accent reset exists")
            reset.clicked()
            check(!Theme.accentOverridden && Theme.focus === Theme.packAccent, "pack accent restored")
        }
        screen.setChoice("themePack", light ? "bone" : "aurora")
        if (!light) find(parent, "themeAccent-violet").clicked()
        ShellStore.lastError = ""
        return true
    }
}
