import QtQuick
import QtQuick.Window
import OpenNOW

QtObject {
    function check(ok, message) { if (!ok) throw new Error("Idle mode: " + message) }
    function run(parent) {
        const window = parent.Window.window
        check(window !== null, "application window exists")
        ShellStore.settings = {}
        check(!window.switchToConsoleOnPad, "automatic console disabled before settings load")
        ShellStore.settings = {launchInConsoleMode:false}
        check(!window.switchToConsoleOnPad, "missing opt-in defaults disabled")
        ShellStore.settings = {launchInConsoleMode:false, switchToConsoleOnPad:true, leaveConsoleOnPointer:true}
        window.applyConsoleSurface(false)
        window.pointerRecentlyActive = false
        for (let i = 0; i < 10; ++i)
            ControllerInput.controllerActivityDetailed("test-controller", "axis:0", 19000)
        check(window.targetDesktopSurface, "explicit desktop choice survives controller activity")

        // A previous startup preference must not resurface when mouse grace expires.
        window.desktopExplicitlySelected = false
        window.startupModeApplied = false
        ShellStore.settings = {launchInConsoleMode:true, switchToConsoleOnPad:true, leaveConsoleOnPointer:true}
        window.initializeStartupMode()
        check(window.startupConsoleRequested, "startup preference applied once")
        window.notePointerInput()
        check(window.targetDesktopSurface, "pointer selects desktop")
        window.pointerRecentlyActive = false // same action as the 30-second timer
        check(window.targetDesktopSurface, "AFK never changes the chosen surface")
        AppController.inputMode = "keyboard"
        window.pointerRecentlyActive = false
        AppController.inputMode = "controller"
        check(window.targetDesktopSurface, "input-label changes cannot select console")
        // Settings refresh cannot reactivate a consumed startup preference.
        ShellStore.settings = Object.assign({}, ShellStore.settings, {windowWidth:1600})
        check(!window.startupConsoleRequested, "refresh cannot reapply startup mode")
        ControllerInput.controllerActivityDetailed("test-controller", "button:0", 1)
        check(!window.targetDesktopSurface, "fresh controller action can select automatic console")

        window.applyConsoleSurface(false)
        window.desktopExplicitlySelected = false
        ShellStore.settings = {launchInConsoleMode:false, switchToConsoleOnPad:false, leaveConsoleOnPointer:true}
        window.pointerRecentlyActive = false
        ControllerInput.controllerActivityDetailed("test-controller", "axis:0", 19000)
        check(window.targetDesktopSurface, "automatic-switch preference is respected")
        window.applyConsoleSurface(true)
        check(!window.targetDesktopSurface, "explicit console toggle still works")
        window.applyConsoleSurface(false)
        check(window.targetDesktopSurface, "explicit desktop toggle still works")
        ShellStore.settings = {launchInConsoleMode:false, switchToConsoleOnPad:true, leaveConsoleOnPointer:true}
        window.pointerRecentlyActive = false
        ControllerInput.controllerActivityDetailed("test-controller", "button:0", 1)
        check(!window.targetDesktopSurface, "deliberate automatic opt-in can override previous desktop hold")
        window.applyConsoleSurface(false)
        return true
    }
}
