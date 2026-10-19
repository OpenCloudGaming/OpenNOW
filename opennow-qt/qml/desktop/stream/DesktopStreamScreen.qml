import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    readonly property bool streamPointerLocked: streamVideo.captureActive && streamVideo.relativeMouse
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Live session")

    signal stopRequested()
    property bool launchCovered: false

    Rectangle { anchors.fill: parent; color: "black"; z: -1 }

    readonly property var session: ShellStore.activeSession || ({})
    readonly property string sessionId: String(session.sessionId || "")
    property string clipboardPasteMessage: ""
    onSessionIdChanged: {
        clipboardPasteMessage = ""
        if (clipboardPasteNotice) clipboardPasteNotice.stop()
    }
    readonly property var profile: session.negotiatedStreamProfile || session.streamProfile || ({})
    readonly property var streamer: ShellStore.streamer || ({})
    readonly property string status: {
        if (ShellStore.streamState === "error")
            return "error"
        if (ShellStore.streamState === "reconnecting"
                || (ShellStore.streamerRestartAttempts > 0 && root.streamer.status !== "streaming"))
            return "reconnecting"
        return String(root.streamer.status || ShellStore.streamState || "starting")
    }
    readonly property bool streaming: root.status === "streaming"
    readonly property bool coopGuest: ShellStore.coop.role === "guest"
        && (ShellStore.coop.guestPhase === "playing" || ShellStore.coop.guestPhase === "reconnecting")
    // Guest co-op panel: a brief peek (no focus, gameplay keeps input) or pinned by Ctrl+G (focus, shell input).
    property bool coopPanelPinned: false
    property bool coopPanelPeeking: false
    property bool coopControllersOpen: false
    readonly property bool coopPanelShown: ShellStore.coop.guestStreaming
        && (coopPanelPinned || coopPanelPeeking || coopControllersOpen)
    readonly property string coopRoster: ShellStore.coop.guestStreaming
        ? ShellStore.coop.members.map(member => member.userId + ":" + member.slot).join(",") : ""
    readonly property bool coopPanelFocused: coopGuestOverlay.activeFocus || coopControllers.activeFocus
    // A guest has no GFN session; its presentation runs on the co-op pipeline instead.
    readonly property bool presentationRunning: root.streaming || root.coopGuest
    // The one gate for gameplay input: this client's presentation runs and no local chrome owns input.
    readonly property bool gameplayInputReady: root.presentationRunning && root.visible
        && !ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay) && !root.coopPanelFocused
    readonly property bool videoSurfaceActive: root.presentationRunning
        || (root.streamer.status === "connecting" && root.status !== "error")
    readonly property bool videoReady: streaming
        && root.streamer.firstFrameLatencyMs !== undefined
        && root.streamer.firstFrameLatencyMs !== null
    property var frameGenerationStats: streamVideo.frameGenerationStats || ({})
    property var swapStats: streamVideo.swapStats || ({})
    property double clockNowMs: Date.now()
    readonly property int clockSeconds: ShellStore.streamStartedAtMs > 0
        ? Math.max(0, Math.floor((clockNowMs - ShellStore.streamStartedAtMs) / 1000)) : 0
    Timer {
        interval: 1000; repeat: true
        running: root.visible && root.streaming
            && (ShellStore.settings.sessionCounterEnabled === true || ShellStore.antiAfkEnabled)
        onTriggered: root.clockNowMs = Date.now()
    }
    MotionProgress {
        id: antiAfkReveal
        shown: root.videoReady && ShellStore.antiAfkIndicatorShown(root.clockSeconds)
            && AppController.overlay.indexOf("stats") < 0
    }
    Rectangle {
        objectName: "desktopAntiAfkIndicator"
        anchors.left: parent.left; anchors.top: parent.top; anchors.margins: 24
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        width: antiAfkLabel.implicitWidth + 28; height: 36; radius: 18; z: 4
        color: Qt.rgba(Theme.shell.r,Theme.shell.g,Theme.shell.b,0.85)
        visible: antiAfkReveal.present
        opacity: antiAfkReveal.progress
        Accessible.role: Accessible.StaticText
        Accessible.name: qsTr("Anti-AFK is enabled")
        Row {
            id: antiAfkLabel; anchors.centerIn: parent; spacing: 8
            Rectangle { width: 8; height: 8; radius: 4; color: DesktopTokens.green; anchors.verticalCenter: parent.verticalCenter }
            Text { text: qsTr("ANTI-AFK ON"); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 12 }
        }
    }
    Rectangle {
        anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 24
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        width: sessionClock.implicitWidth + 24; height: 36; radius: 18; z: 4
        color: Qt.rgba(Theme.shell.r,Theme.shell.g,Theme.shell.b,0.85)
        visible: root.streaming && ShellStore.settings.sessionCounterEnabled === true
            && AppController.overlay.indexOf("stats") < 0
        Text {
            id: sessionClock; anchors.centerIn: parent
            text: Math.floor(root.clockSeconds / 3600) + ":" + String(Math.floor(root.clockSeconds / 60) % 60).padStart(2,"0") + ":" + String(root.clockSeconds % 60).padStart(2,"0")
            color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 12
        }
    }
    function publishCaptureRect() {
        const window = root.Window.window
        if (!window)
            return
        const rect = root.mapToItem(null, 0, 0, root.width, root.height)
        ShellStore.streamCaptureRect = Qt.rect(
            Math.round(window.x + rect.x), Math.round(window.y + rect.y),
            Math.round(rect.width), Math.round(rect.height))
    }
    function resynchronizeStreamInput() {
        root.publishCaptureRect()
        if (!root.visible || !root.presentationRunning || root.launchCovered
                || ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay) || root.coopPanelFocused)
            return
        streamVideo.forceActiveFocus()
        streamVideo.resynchronizeInput()
    }

    onXChanged: publishCaptureRect()
    onYChanged: publishCaptureRect()
    onWidthChanged: publishCaptureRect()
    onHeightChanged: publishCaptureRect()
    Component.onCompleted: publishCaptureRect()

    Connections {
        target: root.Window.window
        function onXChanged() { root.resynchronizeStreamInput() }
        function onYChanged() { root.resynchronizeStreamInput() }
        function onWidthChanged() { Qt.callLater(root.resynchronizeStreamInput) }
        function onHeightChanged() { Qt.callLater(root.resynchronizeStreamInput) }
        function onVisibilityChanged() { Qt.callLater(root.resynchronizeStreamInput) }
        function onActiveChanged() { Qt.callLater(root.resynchronizeStreamInput) }
    }

    StreamVideoItem {
        id: streamVideo
        objectName: "streamSurfaceHost"
        anchors.fill: parent
        visible: root.visible && root.videoSurfaceActive
        enabled: !root.launchCovered
        focus: root.presentationRunning && visible && !root.launchCovered
        inputEnabled: root.gameplayInputReady
        shortcutBindings: ShellStore.streamShortcutBindings()
        clipboardPaste: ShellStore.settings.clipboardPaste === true
        keyboardLayout: String((ShellStore.activeSession || {}).keyboardLayout || "en-US")
        videoSize: Qt.size(Number(root.profile.width || 0), Number(root.profile.height || 0))
        frameGeneration: String(ShellStore.settings.frameGeneration || 'off') === '2x'
        metalFxUpscaling: Qt.platform.os === "osx" && ShellStore.settings.upscaling === "metalfx"
        fsrUpscaling: Qt.platform.os !== "osx" && ShellStore.settings.upscaling === "fsr1"
        upscalingSharpness: Number(ShellStore.settings.upscalingSharpness ?? 10)
        upscalingDenoise: Number(ShellStore.settings.upscalingDenoise ?? 0)
        z: 0
        onLocalShortcutRequested: action => ShellStore.applyStreamShortcutAction(action)
        onClipboardPasteFailed: {
            root.clipboardPasteMessage = ""
            clipboardPasteNotice.restart()
        }
    }

    Timer {
        id: clipboardPasteNotice
        objectName: "streamClipboardPasteNoticeTimer"
        interval: 5000
        onTriggered: root.clipboardPasteMessage = ""
    }

    function toggleCoopPanel() {
        if (!ShellStore.coop.guestStreaming)
            return
        if (!root.coopPanelPinned || !root.coopPanelFocused) {
            root.coopPanelPinned = true
            root.coopPanelPeeking = false
            Qt.callLater(coopGuestOverlay.focusPanel)
        } else {
            root.closeCoopPanel()
        }
    }
    function closeCoopPanel() {
        root.coopControllersOpen = false
        root.coopPanelPinned = false
        root.coopPanelPeeking = false
        Qt.callLater(root.restoreStreamFocus)
    }
    function peekCoopPanel() {
        if (!ShellStore.coop.guestStreaming || root.coopPanelPinned)
            return
        root.coopPanelPeeking = true
        coopPeekTimer.restart()
    }
    onCoopRosterChanged: peekCoopPanel()
    Timer {
        id: coopPeekTimer
        objectName: "coopGuestPeekTimer"
        interval: 5000
        onTriggered: {
            if (coopGuestOverlay.hovered || root.coopPanelFocused || root.coopControllersOpen)
                restart()
            else
                root.coopPanelPeeking = false
        }
    }
    function openCoopControllers() {
        root.coopControllersOpen = true
        Qt.callLater(() => {
            if (coopControllers.item)
                coopControllers.item.focusFirst()
        })
    }
    Connections {
        target: ShellStore.coop
        function onGuestStreamingChanged() {
            root.coopPanelPinned = false
            root.coopPanelPeeking = false
            root.coopControllersOpen = false
            root.peekCoopPanel()
        }
    }
    Binding {
        target: ShellStore.coop
        property: "guestPanelOpen"
        value: root.coopGuest && root.coopPanelFocused
        when: root.visible
    }
    Connections {
        target: streamVideo
        function onLocalShortcutRequested(action) {
            if (root.coopGuest && action === "guide")
                root.toggleCoopPanel()
        }
    }

    DesktopCoopGuestOverlay {
        id: coopGuestOverlay
        anchors.fill: parent
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        revealed: root.coopPanelShown
        opacity: revealed ? 1 : 0
        visible: root.visible && opacity > 0
        Behavior on opacity { NumberAnimation { duration: DesktopTokens.motionDuration; easing.type: Easing.OutCubic } }
        z: 6
        onCloseRequested: root.closeCoopPanel()
        onControllersRequested: root.openCoopControllers()
        onVisibleChanged: if (!visible && activeFocus) Qt.callLater(root.restoreStreamFocus)
    }

    Rectangle {
        id: coopControllersDrawer
        objectName: "coopGuestControllersDrawer"
        visible: root.visible && root.coopControllersOpen && ShellStore.coop.guestStreaming
        x: parent.width - width - 24
        y: 24
        width: 456
        height: parent.height - 172
        radius: 20
        color: Theme.lightMode ? Theme.shell : "#FC0A0E15"
        border.width: 1
        border.color: "#24FFFFFF"
        clip: true
        z: 7
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        MouseArea { anchors.fill: parent; acceptedButtons: Qt.AllButtons }
        Loader {
            id: coopControllers
            anchors.fill: parent
            active: coopControllersDrawer.visible
            focus: active
            sourceComponent: DesktopStreamControllersPage {
                objectName: "coopGuestControllers"
                active: true
                controllerHints: AppController.inputMode === "controller"
                onBackRequested: {
                    root.coopControllersOpen = false
                    Qt.callLater(coopGuestOverlay.focusPanel)
                }
                onResumeRequested: root.closeCoopPanel()
            }
        }
    }

    DesktopCoopGuestReconnectToast {
        anchors.horizontalCenter: parent.horizontalCenter
        y: 70
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        visible: root.visible && ShellStore.coop.role === "guest" && ShellStore.coop.guestPhase === "reconnecting"
        z: 6
    }

    StreamIdleWarning {
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        z: 5
    }

    StreamInputNotice {
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        title: clipboardPasteNotice.running ? qsTr("Text input unavailable") : qsTr("Relative mouse input unavailable")
        message: !root.streaming ? "" : clipboardPasteNotice.running
            ? root.clipboardPasteMessage || qsTr("Clipboard paste failed. Use plain text up to 64 KiB and try again.")
            : streamVideo.relativeMouse ? streamVideo.inputCaptureError : ""
        z: 3
    }

    StreamCaptureStatus {
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        z: 4
    }

    Connections {
        target: ShellStore
        function onPointerLockToggleRequested() {
            streamVideo.togglePointerLock()
        }
        function onTextInputErrorAnnounced(message) {
            root.clipboardPasteMessage = message
            clipboardPasteNotice.restart()
        }
    }

    function restoreStreamFocus() {
        if (!root.visible || root.launchCovered
                || ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay))
            return
        streamVideo.forceActiveFocus()
    }

    onVisibleChanged: {
        publishCaptureRect()
        if (visible)
            Qt.callLater(root.restoreStreamFocus)
    }
    onLaunchCoveredChanged: if (!launchCovered) Qt.callLater(root.resynchronizeStreamInput)

    Keys.onPressed: event => {
        if (event.isAutoRepeat)
            return
        if (root.coopGuest) {
            const modifiers = event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier | Qt.AltModifier | Qt.MetaModifier)
            if ((event.key === Qt.Key_G && modifiers === Qt.ControlModifier)
                    || event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
                root.toggleCoopPanel()
                event.accepted = true
            }
            return
        }
        if (!root.streaming
                && (event.key === Qt.Key_Escape || event.key === Qt.Key_Back)) {
            root.stopRequested()
            event.accepted = true
        }
    }
}
