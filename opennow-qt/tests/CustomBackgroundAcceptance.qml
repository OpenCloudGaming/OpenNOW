import QtQuick
import OpenNOW

QtObject {
    property var background
    property var slider
    property var removeButton
    property var dialog
    property string imageUrl: ""

    function check(ok, message) { if (!ok) throw new Error("Custom background: " + message) }
    function find(item, name) {
        if (!item) return null
        if (item.objectName === name) return item
        for (const child of [...(item.children || []), ...(item.resources || [])]) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }
    function run(parent) {
        background = find(find(parent, "desktopBackdrop"), "customDesktopBackground")
        slider = find(parent, "customBackgroundOpacity")
        removeButton = find(parent, "removeCustomBackground")
        dialog = find(parent, "customBackgroundDialog")
        check(background && slider && removeButton && dialog, "background controls exist")
        check(!slider.enabled, "opacity is disabled without a custom image")
        dialog.selectedFile = imageUrl
        dialog.accepted()
        check(ShellStore.settings.desktopBackground === "custom", "selection activates custom mode")
        check(ShellStore.settings.desktopBackgroundImage === imageUrl, "selection stores the image URL")
        check(background.source.toString() === imageUrl && slider.enabled, "selection reaches the backdrop")
        slider.committed(65)
        check(ShellStore.settings.desktopBackgroundOpacity === 65, "opacity control updates settings")
        dialog.rejected()
        check(ShellStore.settings.desktopBackgroundImage === imageUrl, "cancel preserves the image")
        ShellStore.lastError = ""
        return true
    }
    function ready() {
        check(background.status !== Image.Error, "image failed to load")
        return background.status === Image.Ready
    }
    function verify() {
        check(background.status === Image.Ready, "image loads asynchronously")
        check(background.opacity === 0.65, "image uses the selected opacity")
        slider.committed(0)
        check(background.opacity === 0, "zero opacity hides the image")
        slider.committed(100)
        check(background.opacity === 1, "full opacity shows the unmodified image")
        ShellStore.applySetting("desktopBackground", "solid")
        check(background.source.toString() === "", "other modes release the custom image")
        check(ShellStore.settings.desktopBackgroundImage === imageUrl, "mode changes retain the selection")
        ShellStore.applySetting("desktopBackground", "custom")
        removeButton.clicked()
        check(ShellStore.settings.desktopBackgroundImage === "", "remove clears the image")
        check(ShellStore.settings.desktopBackground === "art", "remove restores game art")
        check(!slider.enabled, "remove disables opacity")
        if (Qt.application.arguments.indexOf("--screenshot") >= 0) {
            dialog.selectedFile = imageUrl
            dialog.accepted()
            slider.committed(65)
        }
        ShellStore.lastError = ""
        return true
    }
}
