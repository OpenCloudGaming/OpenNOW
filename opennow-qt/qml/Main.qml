import QtQuick
import QtQuick.Controls
import OpenNOW

ApplicationWindow {
    id: window
    width: 1600
    height: 900
    minimumWidth: 960
    minimumHeight: 540
    visible: true
    visibility: ApplicationWindow.Windowed
    color: "black"
    title: qsTr("OpenNOW")
    WindowTheme {
        id: windowTheme
        targetWindow: window
        darkMode: !Theme.lightMode
    }
    header: WindowTitleBar {
        targetWindow: window
        visible: windowTheme.clientDecorations && window.visibility !== ApplicationWindow.FullScreen
    }
    WindowResizeEdges {
        parent: window.contentItem.parent
        anchors.fill: parent
        z: 10000
        targetWindow: window
        visible: windowTheme.clientDecorations && window.visibility === ApplicationWindow.Windowed
    }
    property bool applicationCloseConfirmed: false
    property bool sessionQuitPending: false
    property bool sessionQuitStopping: false
    signal sessionQuitCancelled()

    function requestSessionQuit() {
        if (window.sessionQuitPending)
            return
        window.sessionQuitPending = true
        AppController.showOverlay("application-quit-confirm")
    }

    function cancelSessionQuit() {
        if (!window.sessionQuitPending)
            return
        window.sessionQuitPending = false
        window.sessionQuitStopping = false
        window.sessionQuitCancelled()
        if (AppController.overlay === "application-quit-confirm")
            AppController.showOverlay("")
    }

    function confirmApplicationQuit() {
        if (window.sessionQuitPending) {
            if (window.sessionQuitStopping)
                return
            window.sessionQuitStopping = true
            AppController.showOverlay("")
            ShellStore.stopStreamingSession()
            window.finishSessionQuit()
            return
        }
        window.applicationCloseConfirmed = true
        window.close()
    }

    function finishSessionQuit() {
        if (!window.sessionQuitStopping || !ShellStore.sessionWorkIdle || CoreClient.allocationsSettled !== true)
            return
        window.applicationCloseConfirmed = true
        window.close()
    }

    property bool librarySearchPending: false
    property string pendingConsoleScreenshotSession: ""
    property int pendingConsoleScreenshotFrames: 0
    readonly property Item consoleModalFocusOwner: consoleSystemWarning.opened ? consoleSystemWarning
        : consoleStreamEndNotice.opened ? consoleStreamEndNotice
        : (!window.desktopSurfaceActive || AppController.overlay === "session-report")
            && consoleOverlayHost.supportedOverlay ? consoleOverlayHost : null
    onConsoleModalFocusOwnerChanged: if (consoleModalFocusOwner) Qt.callLater(window.restoreConsoleModalFocus)
    onClosing: event => {
        if (window.sessionQuitStopping && !applicationCloseConfirmed) {
            event.accepted = false
            return
        }
        if (!applicationCloseConfirmed && ShellStore.applicationQuitNeedsConfirmation) {
            event.accepted = false
            AppController.showOverlay("application-quit-confirm")
        }
    }

    Component { id: hdrPopupEffect; HdrChromeEffect {} }
    Binding { target: window.Overlay.overlay.layer; property: "enabled"; value: HdrOutput.chromeRequired }
    Binding { target: window.Overlay.overlay.layer; property: "effect"; value: hdrPopupEffect }

    property string activeRoute: AppController.route
    readonly property var frameGenerationStats: activeRoute === "stream" && routeLoader.item
        ? (routeLoader.item.frameGenerationStats || ({})) : ({})
    readonly property var swapStats: activeRoute === "stream" && routeLoader.item
        ? (routeLoader.item.swapStats || ({})) : ({})
    readonly property string frameGenerationStatus: String(frameGenerationStats.status || "")
    readonly property string frameGenerationDiagnosticKey: [frameGenerationStatus,
        frameGenerationStats.timingSource || "none", frameGenerationStats.rejectionReason || "none",
        frameGenerationStats.refreshRateHz || 0].join(":")
    onFrameGenerationDiagnosticKeyChanged: {
        if (frameGenerationStatus !== "")
            CoreClient.logShellDiagnostic("frame-generation state=" + frameGenerationStatus
                + " outputFps=" + Number(frameGenerationStats.outputFps || 0).toFixed(1)
                + " timing=" + String(frameGenerationStats.timingSource || "none")
                + " rejection=" + String(frameGenerationStats.rejectionReason || "none")
                + " intervalMs=" + Number(frameGenerationStats.sourceIntervalMs || 0).toFixed(3)
                + " ptsDeltaMs=" + Number(frameGenerationStats.timestampDeltaMs || 0).toFixed(3)
                + " arrivalDeltaMs=" + Number(frameGenerationStats.arrivalDeltaMs || 0).toFixed(3)
                + " sequenceDelta=" + Number(frameGenerationStats.sequenceDelta || 0)
                + " refreshHz=" + Number(frameGenerationStats.refreshRateHz || 0).toFixed(2)
                + " scope=" + (SmokeTestMode ? "acceptance" : "stream"))
    }
    property bool geometryRestored: false
    readonly property bool settingsLoaded: Object.keys(ShellStore.settings || {}).length > 0
    readonly property bool onboardingVisible: ShellStore.signedIn
        && !ShellStore.authRestorePending
        && !ShellStore.onboardingReplaying
        && (ShellStore.onboardingRequired || ShellStore.onboardingSaving
            || ShellStore.onboardingError !== "")
        && !ShellStore.activeSession
        && ["stream", "inserting", "joining"].indexOf(activeRoute) < 0
    property bool consoleHeldByPad: false
    property bool pointerRecentlyActive: false
    property bool desktopSelectedByPointer: false
    property bool desktopExplicitlySelected: false
    property bool startupModeApplied: false
    property bool startupConsoleRequested: false
    property double lastControllerDiagnosticMs: 0
    property bool forceConsole: false
    property bool launchModeOverridden: false
    readonly property string effectiveLaunchMode: launchModeOverridden ? "" : LaunchModeOverride
    property bool streamStatsAutoShown: false
    property bool streamSurfaceLocked: false
    property bool lockedStreamDesktopSurface: true
    property int visibilityBeforeFullscreen: ApplicationWindow.Windowed
    property bool sessionWindowModeSaved: false
    property bool sessionFullscreenApplied: false
    property bool quitAfterSessionArmed: false
    property int visibilityBeforeSession: ApplicationWindow.Windowed
    property int fullscreenRestoreVisibilityBeforeSession: ApplicationWindow.Windowed
    readonly property string configuredStatsShortcut: String(
        ShellStore.settings.shortcutToggleStats ?? "Ctrl+N")
    readonly property bool streamStatsShortcutEnabled: activeRoute === "stream"
        && (AppController.overlay === ""
            || AppController.overlay === "desktop-stream-menu"
            || AppController.overlay === "guide-session"
            || AppController.overlay === "desktop-stream-stats"
            || AppController.overlay === "desktop-stream-stats-expanded"
            || AppController.overlay === "stream-stats"
            || AppController.overlay === "stream-stats-expanded")
    readonly property bool switchToConsoleOnPad: settingsLoaded
        && ShellStore.settings.switchToConsoleOnPad === true
    onSwitchToConsoleOnPadChanged: {
        window.consoleHeldByPad = false
        if (switchToConsoleOnPad)
            window.desktopExplicitlySelected = false // deliberate opt-in overrides a manual hold
    }
    readonly property bool leaveConsoleOnPointer: !settingsLoaded
        || ShellStore.settings.leaveConsoleOnPointer !== false
    readonly property bool pointerHoldsDesktop: leaveConsoleOnPointer && desktopSelectedByPointer
        && effectiveLaunchMode !== "console"
        && !forceConsole
    readonly property bool consolePreferred: effectiveLaunchMode === "console"
        || forceConsole
        || (!desktopExplicitlySelected && !pointerHoldsDesktop && (
            startupConsoleRequested
            || (consoleHeldByPad && switchToConsoleOnPad)))
    // Command-line flags choose the initial shell, not a permanent mode lock.
    readonly property bool desktopRequested: effectiveLaunchMode === "desktop"
        || (effectiveLaunchMode !== "console" && !consolePreferred)
    readonly property bool desktopEligibleRoute: ["joining",
        "profile-pin", "game-accounts", "persistent-storage", "media",
        "diagnostics", "feedback", "theme-store"].indexOf(activeRoute) < 0
    readonly property bool targetDesktopSurface: streamSurfaceLocked
        ? lockedStreamDesktopSurface : onboardingVisible || (desktopRequested && desktopEligibleRoute)
    readonly property bool streamQmlOverlayActive: (activeRoute === "stream" || activeRoute === "inserting")
        && ((window.desktopSurfaceActive && AppController.overlay.startsWith("desktop-stream-"))
            || ShellStore.isStreamStatsOverlay(AppController.overlay))
    property bool desktopSurfaceActive: targetDesktopSurface
    property bool modeInitialized: false
    property bool startupLaunchConsidered: false
    property bool consoleEntryRequested: false
    property bool launchFramePresented: false
    readonly property bool consoleLaunchAllowed: !ShellStore.activeSession && !ShellStore.streamBusy
        && !ShellStore.pendingLaunchParams && !ShellStore.pendingDirectLaunch
        && !ShellStore.sessionRecoveryPending && !ShellStore.sessionRecoveryAwaitingAuth
        && ShellStore.streamState !== "resuming"
        && ["stream", "inserting", "joining"].indexOf(activeRoute) < 0
        && AppController.overlay === "" && consoleModalFocusOwner === null
        && !onboardingVisible
    readonly property bool consoleLaunchInputReady: !consoleLaunch.active
        && !AppController.consoleLaunchInputDraining && !ControllerInput.shellInputDraining
    readonly property bool consoleLaunchActiveForSmokeTest: consoleLaunch.active
    readonly property string consoleLaunchVariantForSmokeTest: consoleLaunch.variant
    onConsoleLaunchAllowedChanged: if (!consoleLaunchAllowed) cancelConsoleLaunch()
    onConsoleLaunchInputReadyChanged: {
        if (consoleLaunchInputReady && routeLoader.item && window.active)
            Qt.callLater(window.restoreShellFocus)
    }
    readonly property bool settingsLoadedForSmokeTest: settingsLoaded
    readonly property bool consoleModePersistedForSmokeTest:
        ShellStore.settings.launchInConsoleMode === true
    readonly property bool modePersistenceBusyForSmokeTest:
        ShellStore.consoleSurfaceRequestId !== ""
    readonly property string modePersistenceErrorForSmokeTest: ShellStore.lastError
    readonly property var streamerSnapshotForSmokeTest: ShellStore.streamer
    readonly property bool shellCaptureEnabledForSmokeTest: ControllerInput.shellCaptureEnabled
    readonly property bool consoleStreamReady: window.activeRoute === "stream"
        && !window.desktopSurfaceActive && routeLoader.item && routeLoader.item.videoReady === true
    onConsoleStreamReadyChanged: {
        if (!consoleStreamReady)
            pendingConsoleScreenshotSession = ""
        window.syncInputOwnership()
        if (consoleStreamReady)
            Qt.callLater(window.restorePassiveStreamInput)
    }

    Timer {
        id: geometrySaveTimer
        interval: 450
        repeat: false
        onTriggered: {
            if (!window.geometryRestored || window.visibility !== ApplicationWindow.Windowed)
                return
            const geometry = {windowX: window.x, windowY: window.y,
                windowWidth: window.width, windowHeight: window.height}
            for (const key in geometry) {
                if (Math.round(geometry[key]) !== ShellStore.settings[key])
                    ShellStore.setSetting(key, Math.round(geometry[key]))
            }
        }
    }

    onXChanged: if (geometryRestored) geometrySaveTimer.restart()
    onYChanged: if (geometryRestored) geometrySaveTimer.restart()

    onWidthChanged: {
        if (geometryRestored)
            geometrySaveTimer.restart()
        if (activeRoute === "stream")
            fullscreenInputSync.restart()
    }
    onHeightChanged: {
        if (geometryRestored)
            geometrySaveTimer.restart()
        if (activeRoute === "stream")
            fullscreenInputSync.restart()
    }
    onVisibilityChanged: if (activeRoute === "stream") fullscreenInputSync.restart()
    onActiveChanged: {
        syncInputOwnership()
        if (!active) {
            cancelConsoleLaunch()
            pendingConsoleScreenshotSession = ""
        } else if (consoleModalFocusOwner)
            Qt.callLater(window.restoreConsoleModalFocus)
    }
    onDesktopSurfaceActiveChanged: ShellStore.desktopUiActive = desktopSurfaceActive

    readonly property bool accountShortcutsEnabled: ShellStore.signedIn && ShellStore.savedAccounts.length > 1
        && ["stream", "inserting", "joining", "sign-in", "profile-pin", "settings-shortcuts"].indexOf(activeRoute) < 0
        && AppController.overlay === "" && !onboardingVisible && consoleModalFocusOwner === null
    Instantiator {
        model: 9
        delegate: Shortcut {
            required property int index
            objectName: "accountSwitchShortcut" + (index + 1)
            sequence: "Ctrl+" + (index + 1)
            context: Qt.WindowShortcut
            autoRepeat: false
            enabled: window.accountShortcutsEnabled && index < ShellStore.savedAccounts.length
            onActivated: ShellStore.chooseAccountAt(index)
        }
    }

    Shortcut {
        objectName: "shellFullscreenShortcut"
        sequence: "F11"
        context: Qt.ApplicationShortcut
        enabled: window.activeRoute !== "stream"
        onActivated: window.toggleFullscreen()
    }

    Connections {
        target: ShellStore
        function onSessionWorkIdleChanged() { Qt.callLater(window.finishSessionQuit) }
        function onActiveSessionChanged() {
            if (!ShellStore.activeSession
                    || String(ShellStore.activeSession.sessionId || "") !== window.pendingConsoleScreenshotSession)
                window.pendingConsoleScreenshotSession = ""
            window.quitIfSessionFinished()
        }
        function onStreamBusyChanged() { window.quitIfSessionFinished() }
        function onSessionRecoveryPendingChanged() { window.quitIfSessionFinished() }
        function onForceNewAfterStopChanged() { window.quitIfSessionFinished() }
        function onLaunchInspectRequestIdChanged() { window.quitIfSessionFinished() }
        function onBackgroundStreamReminderRequested() {
            AppController.requestWindowAttention(window)
        }
        function onFullscreenToggleRequested() {
            if (window.activeRoute === "stream")
                window.toggleFullscreen()
        }
        function onConsoleScreenshotRequested() {
            const session = ShellStore.activeSession
            if (!session || !window.consoleStreamReady || window.pendingConsoleScreenshotSession !== "")
                return
            window.pendingConsoleScreenshotSession = String(session.sessionId || "")
            window.pendingConsoleScreenshotFrames = 0
            AppController.showOverlay("")
            window.update()
        }
    }

    Connections {
        target: CoreClient
        function onAllocationsSettledChanged() { Qt.callLater(window.finishSessionQuit) }
        ignoreUnknownSignals: true
    }

    Connections {
        target: window.pendingConsoleScreenshotSession !== "" ? window : null
        function onFrameSwapped() {
            if (!window.active || !window.consoleStreamReady || AppController.overlay !== ""
                    || !ShellStore.activeSession
                    || String(ShellStore.activeSession.sessionId || "") !== window.pendingConsoleScreenshotSession) {
                window.pendingConsoleScreenshotSession = ""
                return
            }
            if (consoleOverlayHost.present) {
                window.update()
                return
            }
            if (++window.pendingConsoleScreenshotFrames < 2) {
                window.update()
                return
            }
            window.pendingConsoleScreenshotSession = ""
            ShellStore.captureStreamScreenshot()
        }
    }

    function restoreConsoleModalFocus() {
        const owner = window.consoleModalFocusOwner
        if (!window.active || !owner)
            return
        for (let item = window.activeFocusItem; item; item = item.parent) {
            if (item === owner)
                return
        }
        if (owner === consoleSystemWarning)
            consoleSystemWarning.focusSafe()
        else
            owner.forceActiveFocus()
    }

    Connections {
        target: window.consoleModalFocusOwner ? window : null
        function onActiveFocusItemChanged() { Qt.callLater(window.restoreConsoleModalFocus) }
    }

    function syncInputOwnership() {
        const shellOwnsInput = !window.active || AppController.route !== "stream"
            || (!window.desktopSurfaceActive && !window.consoleStreamReady)
            || (window.desktopSurfaceActive && !ShellStore.authRestorePending && !ShellStore.signedIn)
            || ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)
        ControllerInput.inputSuspended = !window.active
            || (shellOwnsInput && ShellStore.settings.controllerMode === false)
        ControllerInput.shellCaptureEnabled = shellOwnsInput
            && ShellStore.settings.controllerMode !== false
    }

    Binding {
        target: ControllerInput
        property: "leftStickDeadzone"
        value: Number(ShellStore.settings.controllerLeftStickDeadzone ?? 5)
    }
    Binding {
        target: ControllerInput
        property: "forwardFlightControls"
        value: ShellStore.settingsOwnerState.forwardFlightControls
    }
    Binding {
        target: ControllerInput
        property: "rightStickDeadzone"
        value: Number(ShellStore.settings.controllerRightStickDeadzone ?? 5)
    }
    Binding {
        target: ControllerInput
        property: "vibrationIntensity"
        value: Number(ShellStore.settings.controllerVibrationIntensity ?? 100)
    }
    Binding {
        target: ControllerInput
        property: "controllerProfiles"
        value: ShellStore.settings.controllerProfiles || ({})
    }

    Shortcut {
        objectName: "configuredStreamStatsShortcut"
        sequence: window.configuredStatsShortcut
        context: Qt.ApplicationShortcut
        enabled: window.streamStatsShortcutEnabled
            && window.configuredStatsShortcut !== ""
        onActivated: ShellStore.applyStreamShortcutAction("toggle-stats")
    }
    Shortcut {
        objectName: "streamStatsCopyShortcut"
        sequence: "Shift+F3"
        context: Qt.ApplicationShortcut
        enabled: window.activeRoute === "stream"
            && ShellStore.isStreamStatsOverlay(AppController.overlay)
        onActivated: desktopStreamOverlay.copyStatsToClipboard()
    }
    Shortcut {
        objectName: "streamMicrophoneShortcut"
        sequence: String(ShellStore.settings.shortcutToggleMicrophone ?? "Ctrl+Shift+M")
        context: Qt.ApplicationShortcut
        autoRepeat: false
        enabled: window.activeRoute === "stream" && ShellStore.microphoneToggleAvailable
            && sequence !== ""
        onActivated: ShellStore.toggleMicrophone()
    }
    Shortcut {
        objectName: "consoleGuideExitShortcut"
        sequence: String(ShellStore.settings.shortcutStopStream ?? "Ctrl+Shift+Q")
        context: Qt.ApplicationShortcut
        autoRepeat: false
        enabled: !window.desktopSurfaceActive && window.activeRoute === "stream"
            && AppController.overlay.startsWith("guide-") && sequence !== ""
        onActivated: ShellStore.requestStreamExitConfirmation()
    }

    function isGuideShortcut(event) {
        const keyboardModifiers = event.modifiers
            & (Qt.ControlModifier | Qt.ShiftModifier | Qt.AltModifier | Qt.MetaModifier)
        return (event.key === Qt.Key_G && keyboardModifiers === Qt.ControlModifier)
            // ControllerInput represents the physical Guide button as a
            // synthetic F1 event. A real keyboard F1 event switches inputMode
            // to "keyboard" before it reaches this handler.
            || (event.key === Qt.Key_F1 && AppController.inputMode === "controller")
    }

    function saveSessionWindowMode() {
        if (window.sessionWindowModeSaved)
            return
        window.visibilityBeforeSession = window.visibility
        window.fullscreenRestoreVisibilityBeforeSession = window.visibilityBeforeFullscreen
        window.sessionWindowModeSaved = true
    }

    function quitIfSessionFinished() {
        if (window.sessionQuitPending)
            return
        if (!QuitAfterSession)
            return
        if (ShellStore.forceNewAfterStop || ShellStore.launchInspectRequestId !== ""
                || ShellStore.pendingDirectLaunch) {
            window.quitAfterSessionArmed = false
            return
        }
        if (ShellStore.activeSession) {
            window.quitAfterSessionArmed = true
            return
        }
        if (window.quitAfterSessionArmed && !ShellStore.streamBusy && !ShellStore.sessionRecoveryPending)
            AppController.quitApplication()
    }

    function updateSessionWindowMode() {
        if (["inserting", "joining", "stream"].indexOf(AppController.route) >= 0) {
            window.saveSessionWindowMode()
            if (AppController.route === "stream" && !window.sessionFullscreenApplied) {
                window.sessionFullscreenApplied = true
                if (ShellStore.settings.autoFullScreen === true
                        && window.visibility !== ApplicationWindow.FullScreen) {
                    window.visibilityBeforeFullscreen = window.visibility
                    window.showFullScreen()
                }
            }
            return
        }
        if (!window.sessionWindowModeSaved)
            return
        window.sessionWindowModeSaved = false
        window.sessionFullscreenApplied = false
        window.visibilityBeforeFullscreen = window.fullscreenRestoreVisibilityBeforeSession
        if (window.visibility === window.visibilityBeforeSession)
            return
        if (window.visibilityBeforeSession === ApplicationWindow.FullScreen)
            window.showFullScreen()
        else if (window.visibilityBeforeSession === ApplicationWindow.Maximized)
            window.showMaximized()
        else
            window.showNormal()
    }

    function toggleFullscreen() {
        const enteringFullscreen = window.visibility !== ApplicationWindow.FullScreen
        if (enteringFullscreen) {
            window.visibilityBeforeFullscreen = window.visibility
            window.showFullScreen()
        } else if (window.visibilityBeforeFullscreen === ApplicationWindow.Maximized) {
            window.showMaximized()
        } else {
            window.showNormal()
        }
        fullscreenInputSync.restart()
        ShellStore.streamControlMessage = enteringFullscreen
            ? qsTr("Fullscreen on") : qsTr("Fullscreen off")
        ShellStore.accessibilityMessage = ShellStore.streamControlMessage
    }

    Timer {
        id: fullscreenInputSync
        interval: 90
        repeat: false
        onTriggered: {
            if (window.activeRoute !== "stream" || !routeLoader.item)
                return
            if (typeof routeLoader.item.resynchronizeStreamInput === "function")
                routeLoader.item.resynchronizeStreamInput()
        }
    }

    function showConfiguredStreamStats() {
        if (window.activeRoute !== "stream" || window.streamStatsAutoShown
                || AppController.overlay !== ""
                || !(ShellStore.settings.showStatsOnLaunch || ShellStore.settings.showNativeStreamerStats)
                || !ShellStore.streamer || ShellStore.streamer.status !== "streaming")
            return
        window.streamStatsAutoShown = true
        AppController.showOverlay(window.desktopSurfaceActive
            ? "desktop-stream-stats" : "stream-stats")
    }

    Timer {
        id: pointerGrace
        interval: 30000
        repeat: false
        onTriggered: window.pointerRecentlyActive = false
    }

    function applyConsoleSurface(enabled) {
        window.consoleEntryRequested = enabled && window.desktopSurfaceActive
        window.startupModeApplied = true
        window.startupConsoleRequested = false
        window.launchModeOverridden = true
        window.desktopExplicitlySelected = !enabled
        window.desktopSelectedByPointer = false
        pointerGrace.stop()
        window.pointerRecentlyActive = false
        window.forceConsole = enabled
        if (!enabled)
            window.consoleHeldByPad = false
        if (window.activeRoute === "stream") {
            // Automatic pointer/controller heuristics are frozen for a live
            // stream, but the explicit Session menu command remains supported.
            window.lockedStreamDesktopSurface = !enabled
            window.streamSurfaceLocked = true
        }
        window.synchronizeRenderedSurface()
    }

    function requestConsoleSurface(enabled) {
        ShellStore.requestConsoleSurface(enabled)
    }

    function synchronizeRenderedSurface() {
        if (window.targetDesktopSurface || !window.consoleLaunchAllowed)
            window.cancelConsoleLaunch()
        if (consoleLaunch.active) {
            window.consoleEntryRequested = false
            return
        }
        if (window.desktopSurfaceActive === window.targetDesktopSurface) {
            window.consoleEntryRequested = false
            return
        }
        if (window.modeInitialized && window.startupLaunchConsidered
                && window.consoleEntryRequested && !window.targetDesktopSurface) {
            window.consoleEntryRequested = false
            if (window.consoleLaunchAllowed) {
                window.startConsoleLaunch("quick")
                return
            }
        }
        window.desktopSurfaceActive = window.targetDesktopSurface
        if (modeInitialized && window.desktopSurfaceActive)
            modeTransition.restart()
    }

    function startConsoleLaunch(variant) {
        modeTransition.stop()
        modeCurtain.opacity = 0
        window.launchFramePresented = false
        consoleLaunch.start(variant)
        window.desktopSurfaceActive = window.targetDesktopSurface
        window.update()
    }

    function cancelConsoleLaunch() {
        if (consoleLaunch && consoleLaunch.active) {
            consoleLaunch.cancel()
            window.consoleEntryRequested = false
            window.desktopSurfaceActive = window.targetDesktopSurface
        }
    }

    function considerStartupLaunch() {
        if (!window.modeInitialized || !window.settingsLoaded || window.startupLaunchConsidered)
            return
        window.startupLaunchConsidered = true
        window.consoleEntryRequested = false
        if (!window.targetDesktopSurface && window.consoleLaunchAllowed)
            window.startConsoleLaunch("cold")
    }

    function restoreShellFocus() {
        if (!window.active || !window.consoleLaunchInputReady)
            return
        if (window.consoleModalFocusOwner)
            window.restoreConsoleModalFocus()
        else if (routeLoader.item)
            routeLoader.item.forceActiveFocus()
    }

    function notePointerInput() {
        if (!window.consoleLaunchInputReady)
            return
        window.pointerRecentlyActive = true
        pointerGrace.restart()
        if (window.leaveConsoleOnPointer && effectiveLaunchMode !== "console" && !window.forceConsole) {
            window.desktopSelectedByPointer = true
            window.consoleHeldByPad = false
            window.startupConsoleRequested = false
        }
    }

    function noteControllerInput(device, control, value) {
        if (!window.consoleLaunchInputReady)
            return
        // Device enumeration is not user intent (virtual/idle pads are common),
        // and a live stream must never swap render trees because input mode
        // changed. Only real controller activity outside a stream selects it.
        const allowed = window.activeRoute !== "stream"
                && window.switchToConsoleOnPad
                && !window.desktopExplicitlySelected
                && !window.pointerRecentlyActive
        const switching = allowed && !window.consoleHeldByPad
        const now = Date.now()
        if (switching || now - window.lastControllerDiagnosticMs >= 1000) {
            window.lastControllerDiagnosticMs = now
            CoreClient.logShellDiagnostic("controller decision=" + (switching ? "console" : "no-switch")
                + " device=" + String(device || "unknown") + " control=" + String(control || "unknown")
                + " value=" + String(value) + " route=" + window.activeRoute
                + " optedIn=" + window.switchToConsoleOnPad
                + " explicitDesktop=" + window.desktopExplicitlySelected
                + " pointerGrace=" + window.pointerRecentlyActive)
        }
        if (allowed) {
            window.consoleEntryRequested = window.desktopSurfaceActive
            window.desktopSelectedByPointer = false
            window.consoleHeldByPad = true
        }
    }

    function initializeStartupMode() {
        if (window.startupModeApplied || !window.settingsLoaded)
            return
        window.startupModeApplied = true
        window.startupConsoleRequested = ShellStore.settings.launchInConsoleMode === true
            && !window.desktopSelectedByPointer && !window.desktopExplicitlySelected
        CoreClient.logShellDiagnostic("startup console=" + window.startupConsoleRequested
            + " autoSwitch=" + window.switchToConsoleOnPad)
        window.considerStartupLaunch()
    }

    function updateStreamSurfaceLock() {
        if (window.activeRoute === "stream") {
            if (!window.streamSurfaceLocked)
                window.lockedStreamDesktopSurface = window.desktopSurfaceActive
            window.streamSurfaceLocked = true
        } else if (!ShellStore.activeSession
                   || ["sign-in", "accounts", "profile-pin"].indexOf(window.activeRoute) < 0) {
            window.streamSurfaceLocked = false
        }
        window.synchronizeRenderedSurface()
    }

    function restorePassiveStreamInput() {
        if (window.activeRoute !== "stream"
                || ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)
                || !routeLoader.item)
            return
        if (typeof routeLoader.item.resynchronizeStreamInput === "function")
            routeLoader.item.resynchronizeStreamInput()
        else
            routeLoader.item.forceActiveFocus()
    }

    Component.onCompleted: {
        Qt.callLater(() => CoreClient.markUiReady())
        initializeStartupMode()
        updateSessionWindowMode()
        updateStreamSurfaceLock()
        modeInitialized = true
        window.considerStartupLaunch()
        window.synchronizeRenderedSurface()
        ShellStore.desktopUiActive = window.desktopSurfaceActive
        syncInputOwnership()
        Qt.callLater(() => window.showConfiguredStreamStats())
        Qt.callLater(() => {
            if (window.streamQmlOverlayActive && desktopStreamOverlay.inputBlocking)
                desktopStreamOverlay.forceActiveFocus()
            else
                window.restorePassiveStreamInput()
        })
    }

    HoverHandler {
        acceptedDevices: PointerDevice.Mouse
        onPointChanged: {
            if (!window.consoleLaunchInputReady)
                return
            AppController.inputMode = "pointer"
            window.notePointerInput()
        }
    }

    function componentForRoute(route) {
        if (route === "library")
            return libraryScreen
        if (route === "store")
            return storeScreen
        if (route === "theme-store")
            return themeStoreScreen
        if (route === "settings" || route === "settings-streaming")
            return settingsStreamingScreen
        if (route === "controllers" || route === "settings-input")
            return settingsInputScreen
        if (route === "settings-account")
            return settingsAccountScreen
        if (route === "settings-video")
            return settingsVideoScreen
        if (route === "settings-video-dropdown")
            return settingsVideoDropdownScreen
        if (route === "settings-network")
            return settingsNetworkScreen
        if (route === "settings-themes")
            return settingsThemesScreen
        if (route === "settings-advanced")
            return settingsAdvancedScreen
        if (route === "settings-advanced-dropdown")
            return settingsAdvancedDropdownScreen
        if (route === "game-detail")
            return gameDetailScreen
        if (route === "game-detail-platform-dropdown")
            return gameDetailPlatformDropdownScreen
        if (route === "sign-in")
            return signInScreen
        if (route === "joining")
            return joiningScreen
        if (route === "inserting")
            return insertingScreen
        if (route === "stream")
            return streamScreen
        if (route === "accounts")
            return accountsScreen
        if (route === "profile-pin")
            return profilePinScreen
        if (route === "game-accounts")
            return gameAccountsScreen
        if (route === "persistent-storage")
            return persistentStorageScreen
        if (route === "media")
            return mediaScreen
        if (route === "diagnostics")
            return diagnosticsScreen
        if (route === "updates")
            return updateScreen
        if (route === "feedback")
            return feedbackScreen
        return homeScreen
    }

    // Full-window console backdrop. The 16:9 stage letterboxes on taller
    // aspects (16:10, Deck 16:10); this fills the bars with theme-aware
    // color so any aspect looks deliberate instead of void-black.
    Rectangle {
        anchors.fill: parent
        visible: !window.desktopSurfaceActive
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        gradient: Gradient {
            orientation: Gradient.Vertical
            GradientStop { position: 0; color: Qt.darker(Theme.shell, 1.12) }
            GradientStop { position: 0.5; color: Theme.shell }
            GradientStop { position: 1; color: Qt.darker(Theme.shell, 1.12) }
        }
        Rectangle {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            height: Math.round(parent.height * 0.28)
            gradient: Gradient {
                orientation: Gradient.Vertical
                GradientStop { position: 0; color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.07) }
                GradientStop { position: 1; color: "transparent" }
            }
        }
    }

    ShellViewport {
        desktopSurfaceActive: window.desktopSurfaceActive
        layer.enabled: HdrOutput.chromeRequired && window.activeRoute !== "stream"
        layer.effect: HdrChromeEffect {}
        focus: true

        Loader {
            id: routeLoader
            objectName: "mainRouteLoader"
            anchors.fill: parent
            enabled: window.consoleModalFocusOwner === null && window.consoleLaunchInputReady
            sourceComponent: window.onboardingVisible ? onboardingScreen
                : window.desktopSurfaceActive ? desktopAppScreen : window.componentForRoute(window.activeRoute)
            opacity: 1
            transform: Scale {
                origin.x: routeLoader.width / 2
                origin.y: routeLoader.height / 2
                xScale: consoleLaunch.active && !window.desktopSurfaceActive ? consoleLaunch.destinationScale : 1
                yScale: xScale
            }

            onLoaded: {
                if (item) {
                    if (routeLoader.enabled)
                        item.forceActiveFocus()
                    if (window.librarySearchPending && window.activeRoute === "library"
                            && typeof item.showSearchKeyboard === "function") {
                        window.librarySearchPending = false
                        Qt.callLater(item.showSearchKeyboard)
                    }
                    if (!window.desktopSurfaceActive
                            && (window.activeRoute === "settings-video-dropdown"
                            || window.activeRoute === "settings-advanced-dropdown")) {
                        item.initialDropdownOpen = true
                        Qt.callLater(item.openInitialDropdown)
                    }
                }
            }

        }

        Connections {
            target: AppController
            function onApplicationExitCommitted() { window.applicationCloseConfirmed = true }
            function onRouteChanged() {
                if (window.activeRoute !== "library")
                    window.librarySearchPending = false
                window.updateSessionWindowMode()
                window.updateStreamSurfaceLock()
                // The desktop shell and embedded video stay opaque. Animate
                // the entering panel, never flash the entire app on navigation.
                window.syncInputOwnership()
                window.streamStatsAutoShown = false
                Qt.callLater(() => window.showConfiguredStreamStats())
            }
            function onOverlayChanged() {
                if (window.sessionQuitPending && !window.sessionQuitStopping
                        && AppController.overlay !== "application-quit-confirm")
                    window.cancelSessionQuit()
                if (AppController.overlay !== "")
                    window.pendingConsoleScreenshotSession = ""
                if (window.desktopSurfaceActive && window.activeRoute === "stream"
                        && AppController.overlay === "guide-session") {
                    Qt.callLater(() => AppController.showOverlay("desktop-stream-menu"))
                    return
                }
                window.syncInputOwnership()
                if (window.activeRoute === "stream"
                        && !ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay))
                    Qt.callLater(() => window.restorePassiveStreamInput())
                else if (AppController.overlay === "" && routeLoader.item)
                    Qt.callLater(() => routeLoader.item.forceActiveFocus())
            }
            function onDirectLaunchRequested(appId, title) {
                window.startupLaunchConsidered = true
                window.cancelConsoleLaunch()
                ShellStore.acceptDirectLaunch(appId, title)
            }
        }
        Connections {
            target: ShellStore
            function onActiveSessionChanged() {
                window.updateStreamSurfaceLock()
                if (!ShellStore.canOpenSessionGuide() && AppController.overlay.startsWith("guide-"))
                    AppController.showOverlay("")
            }
            function onSignedInChanged() { window.syncInputOwnership() }
            function onAuthRestorePendingChanged() { window.syncInputOwnership() }
            function onStreamerChanged() { window.showConfiguredStreamStats() }
            function onConsoleSurfaceRequested(enabled) { window.applyConsoleSurface(enabled) }
            function onOnboardingCompleted() {
                if (window.activeRoute === "sign-in")
                    AppController.navigate("home")
            }
            function onSettingsChanged() {
                window.initializeStartupMode()
                window.considerStartupLaunch()
                window.syncInputOwnership()
                if (window.geometryRestored || !ShellStore.settings.windowWidth
                        || window.visibility === ApplicationWindow.FullScreen)
                    return
                const settings = ShellStore.settings
                const hasPosition = typeof settings.windowX === "number"
                    && typeof settings.windowY === "number"
                AppController.restoreWindowGeometry(window, Qt.rect(
                    hasPosition ? settings.windowX : 0, hasPosition ? settings.windowY : 0,
                    Number(settings.windowWidth), Number(settings.windowHeight)), hasPosition)
                window.geometryRestored = true
            }
        }

        Keys.onPressed: event => {
            if (window.consoleModalFocusOwner) {
                event.accepted = true
                return
            }
            if (window.onboardingVisible) {
                event.accepted = true
                return
            }
            if (event.key === Qt.Key_F11 && window.activeRoute === "stream"
                    && (ShellStore.settings.shortcutToggleFullscreen ?? "F11") === "F11"
                    && (event.modifiers & (Qt.ControlModifier | Qt.ShiftModifier
                        | Qt.AltModifier | Qt.MetaModifier)) === Qt.NoModifier) {
                window.toggleFullscreen()
                event.accepted = true
            } else if (event.key === Qt.Key_F10) {
                window.requestConsoleSurface(window.desktopSurfaceActive)
                event.accepted = true
            } else if ((event.key === Qt.Key_Escape || event.key === Qt.Key_Back)
                    && window.activeRoute === "stream"
                    && !ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)) {
                // The live StreamVideoItem forwards Escape. Do not turn a
                // transient focus gap into route navigation or session exit.
                event.accepted = false
            } else if (event.key === Qt.Key_Escape || event.key === Qt.Key_Back) {
                event.accepted = AppController.goBack()
            } else if (event.key === Qt.Key_PageUp) {
                event.accepted = AppController.overlay.startsWith("guide-")
                    ? AppController.cycleGuidePage(-1)
                    : AppController.cyclePrimaryRoute(-1)
            } else if (event.key === Qt.Key_PageDown) {
                event.accepted = AppController.overlay.startsWith("guide-")
                    ? AppController.cycleGuidePage(1)
                    : AppController.cyclePrimaryRoute(1)
            } else if (window.isGuideShortcut(event)) {
                event.accepted = true
                if (!event.isAutoRepeat) {
                    if (ShellStore.canOpenSessionGuide())
                        ShellStore.openSessionGuide()
                    else
                        AppController.showOverlay(AppController.overlay === "quick-settings" ? "" : "quick-settings")
                }
            } else if (event.key === Qt.Key_Menu) {
                event.accepted = AppController.showOverlay("quick-settings")
            } else if (event.key === Qt.Key_Y) {
                if (!window.desktopSurfaceActive && window.activeRoute === "home" && AppController.overlay === "") {
                    event.accepted = true
                    if (!event.isAutoRepeat && routeLoader.item && routeLoader.item.moveMode !== true) {
                        window.librarySearchPending = true
                        AppController.navigate("library")
                    }
                } else {
                    event.accepted = AppController.showOverlay("friends")
                }
            } else if (event.key === Qt.Key_X
                       && (AppController.route === "home" || AppController.route === "library")) {
                event.accepted = AppController.navigate("game-detail")
            }
        }
    }

    OverlayHost {
        id: consoleOverlayHost
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        readonly property bool desktopSessionConflict: window.desktopSurfaceActive
            && presentedOverlay === "session-conflict"
        readonly property real consoleScale: desktopSessionConflict
            ? 1 : Math.min(parent.width / 1920, parent.height / 1080)
        width: desktopSessionConflict ? parent.width : parent.width / consoleScale
        height: desktopSessionConflict ? parent.height : parent.height / consoleScale
        scale: consoleScale
        transformOrigin: Item.TopLeft
        overlay: AppController.overlay
        visible: consoleOverlayHost.present && !window.streamQmlOverlayActive
        z: 1000
    }

    DesktopStreamOverlayHost {
        id: desktopStreamOverlay
        notificationsEnabled: window.activeRoute === "stream"
        connectionNotificationsEnabled: window.desktopSurfaceActive
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        anchors.fill: parent
        frameGenerationStats: window.frameGenerationStats
        swapStats: window.swapStats
        pointerLocked: window.activeRoute === "stream" && routeLoader.item
            && routeLoader.item.streamPointerLocked === true
        overlay: window.desktopSurfaceActive || ShellStore.isStreamStatsOverlay(AppController.overlay)
            ? AppController.overlay : ""
        inputBlocking: window.streamQmlOverlayActive
            && ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)
        visible: window.streamQmlOverlayActive
            || ((window.activeRoute === "stream" || window.activeRoute === "inserting") && desktopStreamOverlay.present)
        z: 1100
        onVisibleChanged: if (visible && inputBlocking) forceActiveFocus()
        onInputBlockingChanged: if (visible && inputBlocking) forceActiveFocus()
    }

    DesktopStreamExitConfirm {
        objectName: window.desktopSurfaceActive ? "applicationQuitConfirmation" : "desktopApplicationQuitConfirmation"
        anchors.fill: parent
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        quittingApplication: true
        endingSessionOnQuit: window.sessionQuitPending
        opened: window.desktopSurfaceActive && AppController.overlay === "application-quit-confirm"
        z: 1200
        onCancelRequested: {
            window.cancelSessionQuit()
            AppController.showOverlay("")
        }
        onConfirmRequested: window.confirmApplicationQuit()
    }

    DesktopStreamExitConfirm {
        objectName: "accountSwitchConfirmation"
        anchors.fill: parent
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        switchTargetName: ShellStore.accountSwitchConfirmTarget
            ? String(ShellStore.accountSwitchConfirmTarget.displayName || qsTr("this account")) : qsTr("this account")
        opened: window.desktopSurfaceActive && AppController.overlay === "account-switch-confirm"
        z: 1200
        onCancelRequested: ShellStore.cancelAccountSwitch()
        onConfirmRequested: ShellStore.confirmAccountSwitch()
    }

    Item {
        readonly property real consoleScale: Math.min(parent.width / 1920, parent.height / 1080)
        visible: consoleStreamEndNotice.present
        width: parent.width / consoleScale
        height: parent.height / consoleScale
        scale: consoleScale
        transformOrigin: Item.TopLeft
        z: 1150
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}

        // The desktop shell owns its own notice; the console surface shows this
        // one, so a surface switch hands the same pending notice across.
        ConsoleStreamEndNotice {
            id: consoleStreamEndNotice
            anchors.fill: parent
            opened: !window.desktopSurfaceActive && ShellStore.streamEndNotice !== null
                && AppController.overlay === "" && !window.onboardingVisible && window.consoleLaunchInputReady
                && !consoleSystemWarning.opened && ["stream", "inserting", "joining"].indexOf(window.activeRoute) < 0
            onOpenedChanged: if (!opened) Qt.callLater(() => {
                if (window.desktopSurfaceActive || window.consoleModalFocusOwner || !routeLoader.item)
                    return
                routeLoader.item.forceActiveFocus()
            })
        }
    }

    Item {
        readonly property real consoleScale: Math.min(parent.width / 1920, parent.height / 1080)
        visible: consoleSystemWarning.present
        width: parent.width / consoleScale
        height: parent.height / consoleScale
        scale: consoleScale
        transformOrigin: Item.TopLeft
        z: 1200
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}

        ConsoleWarningSheet {
            id: consoleSystemWarning
            anchors.fill: parent
            readonly property bool quittingApplication: AppController.overlay === "application-quit-confirm"
            readonly property bool switchingAccount: AppController.overlay === "account-switch-confirm"
            readonly property string switchTargetName: ShellStore.accountSwitchConfirmTarget
                ? String(ShellStore.accountSwitchConfirmTarget.displayName || "") : ""
            readonly property bool endingSession: AppController.overlay === "desktop-stream-exit-confirm" || switchingAccount
            readonly property bool waitingForSession: window.activeRoute === "inserting"
            objectName: !window.desktopSurfaceActive && quittingApplication
                ? "applicationQuitConfirmation" : "consoleSystemWarning"
            opened: !window.desktopSurfaceActive && (quittingApplication || endingSession
                || (AppController.overlay === "" && ShellStore.updaterSessionSafe
                    && ShellStore.updaterFailureMessage !== ""))
            panelSide: endingSession && !waitingForSession ? "left" : "right"
            panelInset: endingSession && !waitingForSession ? 48 : 0
            eyebrow: quittingApplication ? qsTr("QUIT") : switchingAccount ? qsTr("END SESSION TO SWITCH") : endingSession
                ? (waitingForSession ? qsTr("LEAVE QUEUE") : qsTr("END SESSION")) : qsTr("UPDATE")
            title: quittingApplication ? qsTr("Quit OpenNOW?")
                : switchingAccount ? qsTr("Switch to %1?").arg(switchTargetName) : endingSession
                ? (waitingForSession ? qsTr("Leave this session request?") : qsTr("End this session?"))
                : qsTr("Update could not be completed")
            message: quittingApplication && !window.sessionQuitPending
                ? qsTr("OpenNOW will close and disconnect from any active cloud session.")
                : switchingAccount || quittingApplication
                ? qsTr("Your game will close on the remote rig. This session cannot be resumed after it ends.")
                : endingSession
                ? (waitingForSession ? qsTr("Your session request will be cancelled and you will leave the queue.")
                    : qsTr("Your game will close on the remote rig. Unsaved progress may be lost. This session cannot be resumed after it ends."))
                : ShellStore.updaterFailureMessage
            safeText: quittingApplication ? qsTr("Keep OpenNOW open") : endingSession
                ? (waitingForSession ? qsTr("Keep waiting") : qsTr("Keep playing")) : qsTr("Close")
            actionText: quittingApplication ? qsTr("Quit OpenNOW") : switchingAccount ? qsTr("End and switch") : endingSession
                ? (waitingForSession ? qsTr("Cancel session") : qsTr("End session")) : ""
            danger: quittingApplication || endingSession
            safeButtonObjectName: quittingApplication ? "quitConfirmKeepOpen" : endingSession
                ? "streamExitKeepPlaying" : "consoleUpdateFailureDismiss"
            actionButtonObjectName: quittingApplication ? "quitConfirmQuit" : "streamExitEndSession"
            signal confirmRequested()
            onSafeRequested: {
                if (switchingAccount)
                    ShellStore.cancelAccountSwitch()
                else if (quittingApplication || endingSession)
                    AppController.showOverlay(consoleOverlayHost.retainingQueueAd ? "queue-ad" : "")
                else
                    ShellStore.updaterFailureMessage = ""
            }
            onActionRequested: confirmRequested()
            onOpenedChanged: if (!opened) Qt.callLater(() => {
                if (window.desktopSurfaceActive || consoleSystemWarning.opened
                        || AppController.overlay !== "" || !routeLoader.item)
                    return
                if (window.activeRoute === "stream")
                    window.restorePassiveStreamInput()
                else
                    routeLoader.item.forceActiveFocus()
            })
            onConfirmRequested: {
                if (quittingApplication) {
                    window.confirmApplicationQuit()
                } else if (switchingAccount) {
                    ShellStore.confirmAccountSwitch()
                } else if (endingSession) {
                    ShellStore.confirmStreamExit()
                }
            }
        }
    }

    Rectangle {
        id: modeCurtain
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        anchors.fill: parent
        color: Theme.shell
        opacity: 0
        visible: opacity > 0
        z: 2000
        Column {
            anchors.centerIn: parent
            spacing: 10
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: window.targetDesktopSurface ? qsTr("COMPUTER MODE") : qsTr("CONSOLE MODE")
                color: Theme.label
                font.family: Theme.displayFont
                font.pixelSize: 28
                font.weight: Font.Black
                font.letterSpacing: 1.2
            }
            Text {
                anchors.horizontalCenter: parent.horizontalCenter
                text: ShellStore.signedIn
                    ? qsTr("Keeping you signed in")
                    : qsTr("Switching shell")
                color: Theme.textMuted
                font.family: Theme.bodyFont
                font.pixelSize: 14
                font.weight: Font.DemiBold
            }
        }
    }
    SequentialAnimation {
        id: modeTransition
        NumberAnimation { target: modeCurtain; property: "opacity"; to: 1; duration: AppController.reducedMotion ? 0 : 240; easing.type: Easing.OutCubic }
        PauseAnimation { duration: AppController.reducedMotion ? 0 : 160 }
        NumberAnimation { target: modeCurtain; property: "opacity"; to: 0; duration: AppController.reducedMotion ? 0 : 380; easing.type: Easing.OutCubic }
    }
    Binding { target: AppController; property: "consoleLaunchInputBlocked"; value: !window.consoleLaunchInputReady }
    Binding { target: ControllerInput; property: "shellInputBlocked"; value: !window.consoleLaunchInputReady }
    Binding {
        target: routeLoader.item
        property: "launchChromeProgress"
        when: !window.desktopSurfaceActive && !window.onboardingVisible && window.activeRoute === "home"
            && routeLoader.item !== null && routeLoader.item.launchChromeProgress !== undefined
        value: consoleLaunch.chromeProgress
        restoreMode: Binding.RestoreBindingOrValue
    }
    Binding {
        target: routeLoader.item
        property: "launchFocusHeld"
        when: !window.desktopSurfaceActive && !window.onboardingVisible && window.activeRoute === "home"
            && routeLoader.item !== null && routeLoader.item.launchFocusHeld !== undefined
        value: !window.consoleLaunchInputReady
        restoreMode: Binding.RestoreBindingOrValue
    }
    ConsoleLaunchAnimation {
        id: consoleLaunch
        objectName: "consoleLaunchAnimation"
        anchors.fill: parent
        z: 2001
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        reducedMotion: AppController.reducedMotion
        soundEnabled: window.settingsLoaded && ShellStore.settings.uiSoundsEnabled !== false
        presentationReady: window.launchFramePresented
        destinationReady: !window.desktopSurfaceActive && routeLoader.status === Loader.Ready && routeLoader.item !== null
        onFinished: {
            window.synchronizeRenderedSurface()
            Qt.callLater(window.restoreShellFocus)
        }
    }
    Connections {
        target: consoleLaunch.active && !window.launchFramePresented ? window : null
        function onFrameSwapped() { window.launchFramePresented = true }
    }
    Connections {
        target: AppController
        function onConsoleLaunchSkipRequested() { if (consoleLaunch.active) consoleLaunch.skip() }
    }
    Connections {
        target: ControllerInput
        function onShellInputSkipRequested() { if (consoleLaunch.active) consoleLaunch.skip() }
    }
    onTargetDesktopSurfaceChanged: {
        window.synchronizeRenderedSurface()
    }

    Item {
        id: accessibilityAnnouncer
        x: -10
        width: 1
        height: 1
        opacity: 0.01
        Accessible.name: qsTr("OpenNOW status")
        Accessible.role: Accessible.StaticText

        function routeName(route) {
            const names = {
                "home": qsTr("Home"),
                "library": qsTr("Library"),
                "store": qsTr("Store"),
                "theme-store": qsTr("Theme store"),
                "controllers": qsTr("Controllers"),
                "settings": qsTr("Settings"),
                "settings-account": qsTr("Account settings"),
                "settings-streaming": qsTr("Streaming settings"),
                "settings-video": qsTr("Video settings"),
                "settings-video-dropdown": qsTr("Video setting choices"),
                "game-detail-platform-dropdown": qsTr("Platform choices"),
                "settings-input": qsTr("Input settings"),
                "settings-network": qsTr("Network settings"),
                "settings-themes": qsTr("Theme settings"),
                "settings-advanced": qsTr("Advanced settings"),
                "settings-advanced-dropdown": qsTr("Advanced setting choices"),
                "game-detail": qsTr("Game details"),
                "sign-in": qsTr("Sign in"),
                "joining": qsTr("Controller order"),
                "inserting": qsTr("Preparing session"),
                "stream": qsTr("Stream"),
                "accounts": qsTr("Accounts"),
                "profile-pin": qsTr("Profile security"),
                "account-switch-confirm": qsTr("Switch account confirmation"),
                "game-accounts": qsTr("Game accounts"),
                "persistent-storage": qsTr("Persistent storage"),
                "media": qsTr("Captures"),
                "diagnostics": qsTr("Diagnostics"),
                "updates": qsTr("Updates"),
                "feedback": qsTr("Feedback"),
                "friends": qsTr("Friends"),
                "friend-actions": qsTr("Friend actions"),
                "quick-settings": qsTr("Quick settings"),
                "session-conflict": qsTr("Session conflict"),
                "queue-ad": qsTr("Queue message"),
                "guide-session": qsTr("Session guide"),
                "guide-controls": qsTr("Controls guide"),
                "guide-media": qsTr("Media guide"),
                "guide-shortcuts": qsTr("Shortcuts guide")
            }
            return names[String(route)] || String(route)
        }
        function announce(message, assertive) {
            if (assertive)
                Accessible.announce(message, Accessible.Assertive)
            else
                Accessible.announce(message)
        }

        Connections {
            target: AppController
            function onRouteChanged() { accessibilityAnnouncer.announce(qsTr("Screen: %1").arg(accessibilityAnnouncer.routeName(AppController.route))) }
            function onOverlayChanged() {
                accessibilityAnnouncer.announce(AppController.overlay === ""
                    ? qsTr("Overlay closed")
                    : qsTr("%1 overlay opened").arg(accessibilityAnnouncer.routeName(AppController.overlay)))
            }
            function onControllerCountChanged() {
                accessibilityAnnouncer.announce(AppController.controllerCount === 0
                    ? qsTr("All controllers disconnected")
                    : qsTr("%1 controllers connected").arg(AppController.controllerCount))
            }
            function onInputModeChanged() {
                if (AppController.inputMode === "pointer" || AppController.inputMode === "keyboard")
                    window.notePointerInput()
            }
        }
        Connections {
            target: ControllerInput
            // Only fresh navigation/button edges signal activity. Repeated keys,
            // hotplug, battery polling and input-label changes cannot select a shell.
            function onControllerActivityDetailed(device, control, value) {
                window.noteControllerInput(device, control, value)
            }
        }
        Connections {
            target: ShellStore
            function onAccessibilityMessageChanged() {
                if (ShellStore.accessibilityMessage !== "")
                    accessibilityAnnouncer.announce(ShellStore.accessibilityMessage)
            }
            function onLastErrorChanged() {
                if (ShellStore.lastError !== "")
                    accessibilityAnnouncer.announce(qsTr("Error: %1").arg(ShellStore.lastError), true)
            }
        }
    }

    UpdateFailureDialog {
        parent: Overlay.overlay
        anchors.centerIn: parent
        failureMessage: ShellStore.updaterFailureMessage
        sessionSafe: window.desktopSurfaceActive && ShellStore.updaterSessionSafe
        onDismissed: ShellStore.updaterFailureMessage = ""
    }

    ClockSkewDialog {
        parent: Overlay.overlay
        anchors.centerIn: parent
        message: ShellStore.clockSkewDismissed ? "" : ShellStore.clockSkewMessage
        sessionSafe: window.desktopSurfaceActive && ShellStore.clockSkewSessionSafe
        onDismissed: ShellStore.clockSkewDismissed = true
    }

    Component { id: desktopAppScreen; DesktopApp {} }
    Component { id: onboardingScreen; DesktopOnboardingScreen {} }
    Component { id: homeScreen; HomeScreen {} }
    Component { id: libraryScreen; LibraryScreen {} }
    Component { id: storeScreen; StoreScreen {} }
    Component { id: themeStoreScreen; ThemeStoreScreen {} }
    Component { id: settingsStreamingScreen; SettingsScreen { initialSection: 1 } }
    Component { id: settingsAccountScreen; SettingsScreen { initialSection: 0 } }
    Component { id: settingsVideoScreen; SettingsScreen { initialSection: 2 } }
    Component { id: settingsVideoDropdownScreen; SettingsScreen { initialSection: 2; initialDropdownOpen: true } }
    Component { id: settingsInputScreen; SettingsScreen { initialSection: 3 } }
    Component { id: settingsNetworkScreen; SettingsScreen { initialSection: 4 } }
    Component { id: settingsThemesScreen; SettingsScreen { initialSection: 5 } }
    Component { id: settingsAdvancedScreen; SettingsScreen { initialSection: 6 } }
    Component { id: settingsAdvancedDropdownScreen; SettingsScreen { initialSection: 6; initialDropdownOpen: true } }
    Component { id: gameDetailScreen; GameDetailScreen {} }
    Component {
        id: gameDetailPlatformDropdownScreen
        GameDetailScreen {
            initialPlatformOpen: SmokeTestMode
            previewGame: SmokeTestMode ? SmokeTestGame : null
        }
    }
    Component { id: signInScreen; SignInScreen {} }
    Component { id: joiningScreen; JoiningScreen {} }
    Component { id: insertingScreen; InsertingScreen {} }
    Component { id: streamScreen; StreamScreen {} }
    Component { id: accountsScreen; DesktopAccountPicker { consoleMode: true } }
    Component { id: profilePinScreen; ProfilePinScreen {} }
    Component { id: gameAccountsScreen; GameAccountsScreen {} }
    Component { id: persistentStorageScreen; PersistentStorageScreen {} }
    Component { id: mediaScreen; MediaScreen {} }
    Component { id: diagnosticsScreen; DiagnosticsScreen {} }
    Component { id: updateScreen; UpdateScreen {} }
    Component { id: feedbackScreen; FeedbackScreen {} }
}
