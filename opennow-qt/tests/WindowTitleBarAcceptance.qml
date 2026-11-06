import QtQuick
import QtQuick.Window
import OpenNOW

QtObject {
    property var host: null
    property var controls: []
    property int phase: 0
    property int key: 0
    property int modifiers: 0
    property point click: Qt.point(-1, -1)
    property bool settling: false
    onKeyChanged: if (key !== 0) settling = true
    onClickChanged: if (click.x >= 0) settling = true

    function check(value, message) { if (!value) throw new Error("Window titlebar acceptance: " + message) }
    function namedChild(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = namedChild(child, name)
            if (found) return found
        }
        return null
    }
    function press(value, modifier) {
        modifiers = modifier || Qt.NoModifier
        key = value
        phase++
        return 0
    }
    function run(host) {
        this.host = host
        const window = host.Window.window
        window.header.visible = true
        controls = ["windowMinimizeButton", "windowMaximizeButton", "windowCloseButton"].map(
            name => namedChild(window.header, name))
        check(controls.every(control => control !== null), "missing real titlebar controls")
        const before = controls[0].nextItemInFocusChain(false)
        check(before !== controls[0], "there is no focus entry before Minimize")
        before.forceActiveFocus(Qt.TabFocusReason)
        return true
    }
    function advance() {
        if (settling) { settling = false; return 0 }
        const window = host.Window.window
        const minimize = controls[0]
        const maximize = controls[1]
        const close = controls[2]
        if (phase === 0) return press(Qt.Key_Tab)
        if (phase === 1) {
            check(minimize.activeFocus, "Tab did not reach Minimize")
            const ring = namedChild(minimize, "windowControlFocusRing")
            check(ring && ring.visible && ring.border.width > 0, "the focused control has no visible focus ring")
            return press(Qt.Key_Return)
        }
        if (phase === 2) {
            check(window.visibility === Window.Minimized, "Return did not minimize")
            window.showNormal()
            minimize.forceActiveFocus(Qt.TabFocusReason)
            return press(Qt.Key_Tab)
        }
        if (phase === 3) {
            check(maximize.activeFocus, "Tab did not reach Maximize")
            return press(Qt.Key_Enter)
        }
        if (phase === 4) {
            check(window.visibility === Window.Maximized, "Enter did not maximize")
            check(maximize.Accessible.name === "Restore", "maximizing did not update the accessible action name")
            return press(Qt.Key_Space)
        }
        if (phase === 5) {
            check(window.visibility === Window.Windowed, "Space did not restore")
            maximize.Accessible.pressAction()
            phase++
            return 0
        }
        if (phase === 6) {
            check(window.visibility === Window.Maximized, "the accessible press action did not maximize")
            click = maximize.mapToItem(null, maximize.width / 2, maximize.height / 2)
            phase++
            return 0
        }
        if (phase === 7) {
            check(window.visibility === Window.Windowed, "the pointer tap did not restore")
            maximize.forceActiveFocus(Qt.TabFocusReason)
            return press(Qt.Key_Tab)
        }
        if (phase === 8) {
            check(close.activeFocus, "Tab did not reach Close")
            return press(Qt.Key_Tab, Qt.ShiftModifier)
        }
        if (phase === 9) {
            check(maximize.activeFocus, "Shift+Tab did not return to Maximize")
            close.forceActiveFocus(Qt.TabFocusReason)
            return press(Qt.Key_Space)
        }
        if (phase === 10) {
            check(window.visible && AppController.overlay === "application-quit-confirm",
                "keyboard Close bypassed graceful quit confirmation")
            return press(Qt.Key_Escape)
        }
        if (phase === 11) {
            check(AppController.overlay === "" && window.visible, "Escape did not cancel quit")
            window.header.visible = Qt.binding(() => window.visibility !== Window.FullScreen)
            maximize.forceActiveFocus(Qt.TabFocusReason)
            window.showFullScreen()
            phase++
            return 0
        }
        if (phase === 12) {
            check(!window.header.visible && !minimize.visible && !maximize.visible && !close.visible,
                "fullscreen exposed titlebar controls")
            check(!minimize.activeFocus && !maximize.activeFocus && !close.activeFocus,
                "a hidden fullscreen titlebar control retained keyboard focus")
            return press(Qt.Key_Space)
        }
        if (phase === 13) {
            check(window.visibility === Window.FullScreen, "Space activated a hidden fullscreen titlebar control")
            window.showNormal()
            phase++
            return 0
        }
        if (phase === 14) {
            check(window.header.visible, "restoring from fullscreen lost the titlebar")
            maximize.forceActiveFocus(Qt.TabFocusReason)
            console.log("WINDOW TITLEBAR PASS Tab, Shift+Tab, Return, Enter, Space, pointer, accessibility, quit, fullscreen")
            return 1
        }
        return -1
    }
}
