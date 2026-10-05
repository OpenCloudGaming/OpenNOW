import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    readonly property bool streamPointerLocked: streamVideo.captureActive && streamVideo.relativeMouse
    focus: true
    Accessible.role: Accessible.Pane
    Accessible.name: qsTr("Live session")
    readonly property var game: ShellStore.selectedGame || ({})
    readonly property var session: ShellStore.activeSession || ({})
    readonly property var profile: session.negotiatedStreamProfile || ({})
    readonly property var streamer: ShellStore.streamer || ({})
    readonly property string status: {
        if (ShellStore.streamState === "error")
            return "error"
        if (ShellStore.streamState === "reconnecting"
                || (ShellStore.streamerRestartAttempts > 0 && root.streamer.status !== "streaming"))
            return "reconnecting"
        return String(streamer.status || ShellStore.streamState || "starting")
    }
    readonly property bool streaming: status === "streaming"
    readonly property bool videoReady: streaming
        && root.streamer.firstFrameLatencyMs !== undefined
        && root.streamer.firstFrameLatencyMs !== null
    readonly property string sessionId: String(session.sessionId || "")
    property string presentedSessionId: ""
    readonly property bool presentedOnce: sessionId !== "" && presentedSessionId === sessionId
    readonly property bool launchCoverVisible: failed || (!videoReady && !presentedOnce)
    readonly property bool reconnectBannerVisible: !failed && !videoReady && presentedOnce
    property double reconnectStartedMs: 0
    readonly property int reconnectSeconds: reconnectStartedMs > 0
        ? Math.max(0, Math.floor((nowMs - reconnectStartedMs) / 1000)) : 0
    readonly property var rigRows: {
        const rows = []
        if (root.session.gpuType) rows.push({label: qsTr("GPU"), value: String(root.session.gpuType)})
        if (root.session.serverLocation || root.session.zone)
            rows.push({label: qsTr("Zone"), value: String(root.session.serverLocation || root.session.zone)})
        const settings = ShellStore.settings || ({})
        const requested = [settings.resolution ? String(settings.resolution) : "",
            settings.fps ? qsTr("%1 FPS").arg(settings.fps) : ""].filter(part => part !== "").join(" · ")
        if (requested !== "") rows.push({label: qsTr("Requested"), value: requested})
        const negotiated = [root.profile.resolution ? String(root.profile.resolution) : "",
            root.profile.fps ? qsTr("%1 FPS").arg(root.profile.fps) : "",
            root.profile.codec ? String(root.profile.codec).toUpperCase() : ""].filter(part => part !== "").join(" · ")
        if (negotiated !== "") rows.push({label: qsTr("Negotiated"), value: negotiated})
        return rows
    }
    onVideoReadyChanged: {
        if (!videoReady) {
            if (!failed && visible && AppController.overlay === "")
                Qt.callLater(root.forceActiveFocus)
            return
        }
        presentedSessionId = sessionId
        Qt.callLater(root.resynchronizeStreamInput)
    }
    onReconnectBannerVisibleChanged: {
        reconnectStartedMs = reconnectBannerVisible ? Date.now() : 0
        nowMs = Date.now()
    }
    readonly property bool videoSurfaceActive: root.streaming || (root.streamer.status === "connecting" && root.status !== "error")
    property var frameGenerationStats: streamVideo.frameGenerationStats || ({})
    readonly property bool failed: status === "error"
    property double nowMs: Date.now()
    readonly property int elapsedSeconds: ShellStore.streamStartedAtMs > 0
        ? Math.max(0, Math.floor((nowMs - ShellStore.streamStartedAtMs) / 1000)) : 0
    readonly property int clockDuration: Math.max(1, Number(ShellStore.settings.sessionClockShowDurationSeconds || 30))
    readonly property int clockInterval: Math.max(0, Number(ShellStore.settings.sessionClockShowEveryMinutes || 0) * 60)
    readonly property bool sessionClockVisible: Boolean(ShellStore.settings.sessionCounterEnabled) && videoReady
        && (elapsedSeconds < clockDuration
            || (clockInterval > 0 && elapsedSeconds % clockInterval < clockDuration))
    readonly property int antiAfkReminderDuration: Math.max(1, Number(ShellStore.settings.antiAfkReminderDurationSeconds || 5))
    readonly property int antiAfkReminderInterval: Math.max(0, Number(ShellStore.settings.antiAfkReminderEveryMinutes || 0) * 60)
    readonly property bool antiAfkReminderVisible: ShellStore.antiAfkEnabled && videoReady
        && !Boolean(ShellStore.settings.showAntiAfkIndicator) && antiAfkReminderInterval > 0
        && elapsedSeconds % antiAfkReminderInterval < antiAfkReminderDuration

    function elapsed(value) {
        const hours = Math.floor(value / 3600)
        const minutes = Math.floor((value % 3600) / 60)
        const seconds = value % 60
        return (hours > 0 ? String(hours).padStart(2, "0") + ":" : "")
            + String(minutes).padStart(2, "0") + ":" + String(seconds).padStart(2, "0")
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
        if (!root.visible || !root.videoReady
                || ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay))
            return
        streamVideo.forceActiveFocus()
        streamVideo.resynchronizeInput()
    }

    onXChanged: publishCaptureRect()
    onYChanged: publishCaptureRect()
    onWidthChanged: publishCaptureRect()
    onHeightChanged: publishCaptureRect()
    onVisibleChanged: publishCaptureRect()
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

    Timer { interval: 1000; repeat: true; running: root.streaming || root.reconnectBannerVisible; onTriggered: root.nowMs = Date.now() }

    SessionGlyphs { id: glyphs }

    StreamVideoItem {
        id: streamVideo
        objectName: "streamSurfaceHost"
        anchors.fill: parent
        visible: root.visible && root.videoSurfaceActive
        enabled: root.videoReady
        focus: root.videoReady && visible
        inputEnabled: root.videoReady && visible
            && !ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)
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
        onClipboardPasteFailed: clipboardPasteNotice.restart()
    }

    Timer { id: clipboardPasteNotice; interval: 5000 }

    StreamInputNotice {
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        message: !root.videoReady ? "" : clipboardPasteNotice.running
            ? qsTr("Clipboard paste failed. Use plain text up to 64 KiB and try again.")
            : streamVideo.relativeMouse ? streamVideo.inputCaptureError : ""
        z: 3
    }

    StreamCaptureStatus {
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        z: 12
    }

    Connections {
        target: ShellStore
        function onPointerLockToggleRequested() {
            streamVideo.togglePointerLock()
        }
    }

    ScreenBackground {
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        visible: root.reconnectBannerVisible && !streamVideo.visible
        artwork: DesktopTokens.artworkUrl(root.game, true)
        tint: "#101B2A"
        z: 1
    }

    Rectangle {
        visible: root.reconnectBannerVisible
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        anchors.fill: parent
        color: Qt.rgba(0.02, 0.03, 0.06, 0.32)
        z: 2
    }

    LaunchStage {
        id: launchCover
        objectName: "consoleStreamLaunchCover"
        visible: root.launchCoverVisible
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        anchors.fill: parent
        z: 3
        game: root.game
        tone: root.failed ? Theme.coral : Theme.focus
        statusText: root.failed ? qsTr("Stopped") : qsTr("Connecting")
        eyebrow: root.failed ? qsTr("Stream could not start") : qsTr("Almost there")
        headline: root.failed ? qsTr("That didn't work") : qsTr("Connecting to your game")
        detail: root.failed
            ? I18n.source(root.streamer.message || ShellStore.streamMessage || qsTr("Native media startup failed"), I18n.revision)
            : qsTr("Your rig is ready. OpenNOW is opening the media connection, and your game appears here as soon as the first video frame is ready.")
        railVisible: !root.failed
        activeStep: 4
        activeStepDetail: qsTr("Leaves this screen on the first decoded frame")
        footerVisible: !root.failed
        footerText: qsTr("Cancel session…")
        onFooterRequested: ShellStore.requestStreamExitConfirmation()

        actions: [
            LaunchStage.LaunchAction {
                id: retryMediaButton
                objectName: "streamRetryMediaButton"
                visible: root.failed
                enabled: !ShellStore.streamBusy
                primary: true
                text: qsTr("Retry media")
                detail: qsTr("Reconnects to the same session")
                glyph: glyphs.button("A")
                keyboardGlyph: glyphs.keyboard
                KeyNavigation.down: stopSessionButton
                onClicked: ShellStore.retryNativeStreamer()
            },
            LaunchStage.LaunchAction {
                id: stopSessionButton
                objectName: "streamStopSessionButton"
                visible: root.failed
                text: qsTr("End session…")
                glyph: glyphs.button("B")
                keyboardGlyph: glyphs.keyboard
                KeyNavigation.up: retryMediaButton
                onClicked: ShellStore.requestStreamExitConfirmation()
            }
        ]

        aside: [
            LaunchStage.RigCard {
                x: Math.round((parent.width - width) / 2)
                y: 90
                visible: !root.failed && rows.length > 0
                rows: root.rigRows
            },
            LaunchStage.StopCard {
                x: Math.round((parent.width - width) / 2)
                visible: root.failed
                reachedStep: 4
                code: String(root.streamer.errorCode || "")
            }
        ]
    }

    Rectangle {
        objectName: "streamReconnectBanner"
        visible: root.reconnectBannerVisible
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        z: 12
        anchors.horizontalCenter: parent.horizontalCenter
        y: 56
        width: Math.min(parent.width - 96, reconnectCopy.implicitWidth + 160)
        height: 122
        radius: 32
        color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.94)
        border.width: 1
        border.color: Theme.seam
        Accessible.role: Accessible.AlertMessage
        Accessible.name: qsTr("Reconnecting. Your stream will return when the connection is restored.")

        Canvas {
            id: reconnectArc
            x: 30
            anchors.verticalCenter: parent.verticalCenter
            width: 44; height: 44
            onPaint: {
                const context = getContext("2d")
                context.reset()
                context.lineWidth = 4
                context.lineCap = "round"
                context.strokeStyle = Qt.rgba(1, 1, 1, 0.16)
                context.beginPath()
                context.arc(22, 22, 19, 0, Math.PI * 2)
                context.stroke()
                context.strokeStyle = Theme.yellow
                context.beginPath()
                context.arc(22, 22, 19, -Math.PI / 2, Math.PI * 0.1)
                context.stroke()
            }
            RotationAnimator on rotation {
                from: 0; to: 360; duration: 1200
                loops: Animation.Infinite
                running: root.reconnectBannerVisible && !AppController.reducedMotion
            }
        }

        Column {
            id: reconnectCopy
            x: 96
            anchors.verticalCenter: parent.verticalCenter
            spacing: 4
            Text {
                text: qsTr("Reconnecting · %1").arg(root.elapsed(root.reconnectSeconds)).toUpperCase()
                color: Theme.yellow
                font.family: Theme.monoFont; font.pixelSize: 14; font.weight: Font.Bold; font.letterSpacing: 2.4
            }
            Text {
                text: qsTr("Your stream will return when the connection is restored")
                color: Theme.label
                font.family: Theme.displayFont; font.pixelSize: 26; font.weight: Font.Black
            }
            Text {
                text: qsTr("Your session is still active. Open the session menu for options.")
                color: Theme.textMuted
                font.family: Theme.bodyFont; font.pixelSize: 17
            }
        }
    }

    Rectangle {
        visible: root.reconnectBannerVisible && AppController.overlay === ""
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        z: 12
        anchors.right: parent.right; anchors.rightMargin: 156
        anchors.bottom: parent.bottom; anchors.bottomMargin: 50
        width: reconnectHint.implicitWidth + 36
        height: 56
        radius: 28
        color: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.9)
        border.width: 1
        border.color: Theme.seam
        ControllerGlyph {
            id: reconnectHint
            anchors.centerIn: parent
            glyph: glyphs.button("GUIDE")
            keyboard: glyphs.keyboard
            label: qsTr("Session menu")
        }
        MouseArea {
            anchors.fill: parent
            cursorShape: Qt.PointingHandCursor
            onClicked: AppController.showOverlay("guide-session")
        }
    }

    GlassPanel {
        MotionProgress { id: clockMotion; shown: root.sessionClockVisible }
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        visible: clockMotion.present
        z: 12
        x: 34; y: 34; width: 190; height: 58; panelRadius: 22; strong: true
        Row {
            anchors.centerIn: parent; spacing: 10
            Text { text: "◷"; color: Theme.focus; font.pixelSize: 19; font.weight: Font.Black }
            Text { text: root.elapsed(root.elapsedSeconds); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 17; font.weight: Font.Bold }
        }
        opacity: clockMotion.progress
    }

    GlassPanel {
        visible: root.videoReady && AppController.overlay === ""
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        z: 12
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 34
        width: streamHints.implicitWidth + 34
        height: 52
        panelRadius: 26
        strong: true

        Row {
            id: streamHints
            anchors.centerIn: parent
            spacing: 22
            ControllerGlyph { glyph: glyphs.button("GUIDE"); keyboard: glyphs.keyboard; label: qsTr("Session") }
            ControllerGlyph { visible: ShellStore.settings.shortcutToggleStats !== ""; glyph: String(ShellStore.settings.shortcutToggleStats ?? "Ctrl+N"); keyboard: true; label: qsTr("Stats") }
            ControllerGlyph { visible: String(ShellStore.settings.shortcutToggleFullscreen ?? "F11") !== ""; glyph: String(ShellStore.settings.shortcutToggleFullscreen ?? "F11"); keyboard: true; label: qsTr("Fullscreen") }
        }
    }

    GlassPanel {
        MotionProgress {
            id: afkMotion
            shown: root.videoReady && ShellStore.antiAfkEnabled
                && (Boolean(ShellStore.settings.showAntiAfkIndicator) || root.antiAfkReminderVisible)
        }
        visible: afkMotion.present
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        z: 12
        x: root.width - width - 34; y: 34; width: 180; height: 58; panelRadius: 22; strong: true
        Row {
            anchors.centerIn: parent; spacing: 10
            Rectangle { width: 10; height: 10; radius: 5; color: Theme.mint }
            Text { text: qsTr("ANTI-AFK ON"); color: Theme.label; font.family: Theme.monoFont; font.pixelSize: 13; font.weight: Font.Black; font.letterSpacing: 0.8 }
        }
        opacity: afkMotion.progress
    }

    Keys.onPressed: event => {
        if (event.isAutoRepeat)
            return
        if (!root.videoReady
                && (event.key === Qt.Key_Escape || event.key === Qt.Key_Back)) {
            event.accepted = true
            ShellStore.requestStreamExitConfirmation()
        }
    }

    onFailedChanged: if (failed) Qt.callLater(() => retryMediaButton.forceActiveFocus())

}
