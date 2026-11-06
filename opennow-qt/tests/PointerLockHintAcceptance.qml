import QtQuick
import OpenNOW

QtObject {
    property Component pointerLockHintComponent: Component { StreamPointerLockHint {} }
    property var pointerLockHint: null
    property var savedPointerLockHint: null
    property var coveredPointerLockHint: null
    property bool pointerLockTimeoutVerified: false
    property int savedPointerLockHintAppearances: 0
    property int coveredPointerLockHintAppearances: 0

    function check(ok, message) { if (!ok) throw new Error("Pointer lock notice: " + message) }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of [...(item.children || []), ...(item.resources || [])]) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }

    function run(parent) {
        const hint = pointerLockHintComponent.createObject(parent, {
            active:true, streaming:true, sessionId:"pointer-lock-fixture"
        })
        check(hint !== null && !hint.visible, "the default-on preference has no unlocked hint")
        const timer = find(hint, "streamPointerLockNoticeTimer")
        check(timer !== null && timer.interval === 6000, "the unlocked hint times out after six seconds")
        hint.pointerLockEnabled = false
        check(hint.visible && hint.Accessible.role === Accessible.Pane,
            "intentional unlock has a passive explanation, not a capture error")
        const button = find(hint, "enablePointerLockButton")
        check(button && button.visible && button.text === qsTr("Enable pointer lock"),
            "the unlocked hint offers a visible way to enable capture")
        hint.active = false
        check(!hint.visible && !hint.pointerLockEnabled, "an overlay hides the hint without changing the choice")
        hint.active = true
        check(!hint.visible, "closing an overlay does not restore a dismissed hint")
        hint.pointerLockEnabled = true
        hint.pointerLockEnabled = false
        check(hint.visible, "a new on-to-off choice shows the hint again")
        hint.streaming = false
        hint.active = false
        hint.streaming = true
        hint.active = true
        check(!hint.visible, "reconnecting does not restore a dismissed hint")
        hint.pointerLockEnabled = true
        hint.pointerLockEnabled = false
        let requested = 0
        hint.enableRequested.connect(() => { requested += 1 })
        button.clicked()
        check(requested === 1 && !hint.visible, "the enable button immediately dismisses the hint")
        hint.pointerLockEnabled = true
        hint.pointerLockEnabled = false
        check(hint.visible, "unlocking shows a fresh notice")
        hint.pointerLockEnabled = true
        check(!hint.visible, "enabling pointer lock with F8 dismisses the notice")
        hint.pointerLockEnabled = false
        timer.interval = 50
        timer.triggered.connect(() => Qt.callLater(() => {
            check(!hint.visible, "the unlocked hint automatically hides after its timer expires")
            hint.active = false
            hint.active = true
            hint.streaming = false
            hint.streaming = true
            Qt.callLater(() => {
                check(!hint.visible, "overlay close and reconnect cannot restart an expired notice")
                pointerLockTimeoutVerified = true
            })
        }))
        pointerLockHint = hint

        const saved = pointerLockHintComponent.createObject(parent, {
            active:true, pointerLockEnabled:false, sessionId:"saved-pointer-lock-fixture"
        })
        check(saved && !saved.visible, "a saved unlocked preference has no hint before the stream starts")
        const savedTimer = find(saved, "streamPointerLockNoticeTimer")
        check(savedTimer && savedTimer.interval === 6000, "the stream-start hint also uses six seconds")
        savedTimer.interval = 50
        saved.visibleChanged.connect(() => {
            if (saved.visible) savedPointerLockHintAppearances += 1
        })
        saved.streaming = true
        savedPointerLockHint = saved

        const covered = pointerLockHintComponent.createObject(parent, {
            active:false, streaming:true, pointerLockEnabled:false, sessionId:"covered-pointer-lock-fixture"
        })
        check(covered && !covered.visible, "the launch cover suppresses the initial unlocked hint")
        const coveredTimer = find(covered, "streamPointerLockNoticeTimer")
        coveredTimer.interval = 50
        covered.visibleChanged.connect(() => {
            if (covered.visible) coveredPointerLockHintAppearances += 1
        })
        Qt.callLater(() => {
            check(!covered.visible && !covered.streamStarted,
                "the startup hint remains pending while the stream is covered")
            covered.active = true
        })
        coveredPointerLockHint = covered
        return true
    }

    function verifyRendered(parent) {
        check(pointerLockTimeoutVerified, "the unlocked notice completed its real timer and remained dismissed")
        check(savedPointerLockHintAppearances === 1 && !savedPointerLockHint.visible,
            "a saved unlocked preference appears once at stream start, then automatically hides")
        check(coveredPointerLockHintAppearances === 1 && !coveredPointerLockHint.visible,
            "clearing the launch cover shows the saved-off hint once, then it automatically hides")
        pointerLockHint.destroy()
        savedPointerLockHint.destroy()
        coveredPointerLockHint.destroy()
        return true
    }
}
