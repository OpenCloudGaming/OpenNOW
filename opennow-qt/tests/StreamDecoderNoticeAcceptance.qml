import QtQuick
import OpenNOW

// The decoder-limited notice is arithmetic advice, not a status blip: the instruction is 94
// characters ("This device is dropping frames while decoding. Lower the resolution or frame rate in
// Settings."), so the notice has to wrap like every other long-copy notice in the stack. A notice
// left on the single-line default renders the first half of the sentence and elides the half that
// tells the player what to do.
//
// The gate that decides whether the column may be shown estimates its height from the children's
// real heights, so a wrapped notice is accounted for by construction; these checks pin both the
// wrap and the finished geometry at the audited sizes.
QtObject {
    id: fixture
    property var window: null
    property real sampleTime: 100000
    readonly property string noticeSessionId: "decoder-notice-session"
    readonly property string expectedTitle: "Decoder can't keep up"
    readonly property string expectedSubtitle: "This device is dropping frames while decoding. Lower the resolution or frame rate in Settings."

    function check(ok, message) {
        if (!ok) throw new Error("Decoder notice acceptance: " + message)
    }

    function find(item, name) {
        if (!item) return null
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const result = find(child, name)
            if (result) return result
        }
        return null
    }

    function findText(item, text) {
        if (!item) return null
        if (item.text === text) return item
        for (const child of item.children || []) {
            const result = findText(child, text)
            if (result) return result
        }
        return null
    }

    function column() {
        const item = find(window.contentItem, "desktopStreamToasts")
        check(item !== null, "the stream toast column is in the tree")
        return item
    }

    function notice() {
        const item = find(window.contentItem, "streamDecoderLimitedToast")
        check(item !== null, "the decoder notice is part of the stream toast stack")
        return item
    }

    function beginSession() {
        ShellStore.activeSession = {sessionId: fixture.noticeSessionId, appId: "1001",
            catalogAppId: "doom-eternal", phase: "ready", status: 2}
        ShellStore.streamer = {status: "streaming", sessionId: fixture.noticeSessionId}
        ShellStore.streamState = "streaming"
        ShellStore.streamerStopExpected = false
        ShellStore.streamStopRequestId = ""
        ShellStore.acceptNativeEvent({type: "status", event: "first-frame", status: "streaming"})
    }

    function run(parent) {
        window = parent.Window.window
        ShellStore.streamerStartRequestId = "fixture-blocked"
        ShellStore.streamInputPauseRequestId = "fixture-blocked"
        ShellStore.connectionHealth.clock = () => fixture.sampleTime
        beginSession()
        const stack = column()
        const limited = notice()
        check(stack.active, "the toast column is active on the stream route")
        check(stack.visible, "the toast column fits this window")
        check(!limited.visible, "the notice stays quiet while the decoder keeps up")
        check(!ShellStore.connectionHealth.decoderLimited, "a fresh session is not decoder-limited")
        ShellStore.acceptNativeEvent({type: "log", event: "queue-dropped", media: "video-decode",
            unit: "frames", count: 30})
        check(ShellStore.connectionHealth.decoderLimited,
            "sustained decoder queue drops report a limited decoder")
        check(limited.visible, "the decoder notice is raised once the decoder is limited")
        check(limited.title === expectedTitle, "the notice has its title")
        return true
    }

    function verifyRendered(parent) {
        const stack = column()
        const limited = notice()
        check(ShellStore.connectionHealth.decoderLimited, "the decoder is still limited when composed")
        check(stack.active && stack.visible, "the toast column is still rendered after composition")
        check(limited.visible, "the decoder notice is still rendered after composition")
        check(limited.title === expectedTitle, "the notice keeps its title when rendered")
        check(limited.subtitle === expectedSubtitle, "the notice keeps the full instruction copy")
        const subtitle = findText(limited, expectedSubtitle)
        check(subtitle !== null, "the notice renders its instruction text")
        check(subtitle.truncated === false, "the instruction is not truncated")
        check(subtitle.lineCount >= 2, "the instruction wraps instead of running off one line")
        check(limited.height > 68, "the notice grows for the wrapped instruction")
        check(Math.abs(limited.height - Math.max(68, subtitle.parent.height + 28)) <= 0.5,
            "the notice is exactly as tall as its wrapped contents require")
        check(subtitle.mapToItem(limited, 0, subtitle.paintedHeight).y <= limited.height - 8,
            "the last line of the instruction is inside the notice")
        const origin = limited.mapToItem(window.contentItem, 0, 0)
        check(origin.x >= 0 && origin.y >= 0
            && origin.x + limited.width <= window.contentItem.width + 0.5
            && origin.y + limited.height <= window.contentItem.height + 0.5,
            "the whole notice is inside the window")
        return true
    }
}
