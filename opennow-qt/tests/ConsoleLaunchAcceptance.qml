import QtQuick
import QtQuick.Window
import OpenNOW

QtObject {
    function check(ok, message) { if (!ok) throw new Error("Console launch: " + message) }
    function run(parent) {
        const window = parent.Window.window
        check(window.startupLaunchConsidered, "startup entry was consumed")
        check(window.consoleLaunchInputReady, "startup releases input")
        check(!window.consoleLaunchActiveForSmokeTest, "startup completed")
        AppController.navigate("library")
        AppController.goBack()
        window.considerStartupLaunch()
        ShellStore.settings = Object.assign({}, ShellStore.settings, {uiSoundsEnabled:false})
        check(!window.consoleLaunchActiveForSmokeTest, "routes, Back and settings do not replay startup")

        window.applyConsoleSurface(false)
        AppController.navigate("settings")
        window.applyConsoleSurface(true)
        check(window.consoleLaunchActiveForSmokeTest, "deliberate entry starts intro")
        check(!window.desktopSurfaceActive, "console selection changes the rendered surface synchronously")
        check(window.consoleLaunchVariantForSmokeTest === "quick", "reentry uses quick variant")
        check(AppController.route === "settings", "destination is preserved")
        check(!window.consoleLaunchInputReady, "intro owns input")
        window.notePointerInput()
        check(!window.targetDesktopSurface, "skip pointer cannot change mode")
        AppController.showOverlay("quick-settings")
        check(!window.consoleLaunchActiveForSmokeTest, "modal cancels intro")
        AppController.showOverlay("")
        check(!window.consoleLaunchActiveForSmokeTest, "modal dismissal does not replay intro")

        window.applyConsoleSurface(false)
        ShellStore.pendingDirectLaunch = {appId:"test", title:"test"}
        window.applyConsoleSurface(true)
        check(!window.consoleLaunchActiveForSmokeTest, "direct launch bypasses intro")
        ShellStore.pendingDirectLaunch = null
        check(!window.consoleLaunchActiveForSmokeTest, "finishing direct launch does not replay")

        window.applyConsoleSurface(false)
        ShellStore.sessionRecoveryPending = true
        window.applyConsoleSurface(true)
        check(!window.consoleLaunchActiveForSmokeTest, "session recovery bypasses intro")
        ShellStore.sessionRecoveryPending = false
        check(!window.consoleLaunchActiveForSmokeTest, "reconnect completion does not replay")

        window.applyConsoleSurface(false)
        ShellStore.pendingLaunchParams = {appId:"test"}
        window.applyConsoleSurface(true)
        check(!window.consoleLaunchActiveForSmokeTest, "queued launch bypasses intro")
        ShellStore.pendingLaunchParams = null
        check(!window.consoleLaunchActiveForSmokeTest, "queue completion does not replay")

        window.applyConsoleSurface(false)
        ShellStore.activeSession = {sessionId:"launch-regression", phase:"queued", status:1}
        window.applyConsoleSurface(true)
        check(!window.consoleLaunchActiveForSmokeTest, "existing session bypasses intro")
        ShellStore.activeSession = null
        check(!window.consoleLaunchActiveForSmokeTest, "session end does not replay")

        window.applyConsoleSurface(false)
        AppController.navigate("sign-in")
        window.applyConsoleSurface(true)
        check(AppController.route === "sign-in", "auth destination is not replaced by Home")
        window.applyConsoleSurface(false)
        check(!window.consoleLaunchActiveForSmokeTest, "mode exit cancels intro")
        check(window.desktopSurfaceActive, "mode exit restores the desktop synchronously")
        check(window.consoleLaunchInputReady, "cancel restores neutral input")
        return true
    }
}
