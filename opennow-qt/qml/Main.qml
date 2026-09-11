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

    Component { id: hdrPopupEffect; HdrChromeEffect {} }
    Binding { target: window.Overlay.overlay.layer; property: "enabled"; value: HdrOutput.chromeRequired }
    Binding { target: window.Overlay.overlay.layer; property: "effect"; value: hdrPopupEffect }

    property string activeRoute: AppController.route
    readonly property var frameGenerationStats: activeRoute === "stream" && routeLoader.item
        ? (routeLoader.item.frameGenerationStats || ({})) : ({})
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
    property int visibilityBeforeSession: ApplicationWindow.Windowed
    property int fullscreenRestoreVisibilityBeforeSession: ApplicationWindow.Windowed
    readonly property string configuredStatsShortcut: String(
        ShellStore.settings.shortcutToggleStats || "Ctrl+N")
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
        "accounts", "profile-pin", "game-accounts", "persistent-storage", "media",
        "diagnostics", "feedback", "theme-store"].indexOf(activeRoute) < 0
    readonly property bool targetDesktopSurface: streamSurfaceLocked
        ? lockedStreamDesktopSurface : onboardingVisible || (desktopRequested && desktopEligibleRoute)
    readonly property bool streamQmlOverlayActive: (activeRoute === "stream" || activeRoute === "inserting")
        && (AppController.overlay.startsWith("desktop-stream-")
            || AppController.overlay.startsWith("stream-stats"))
    readonly property bool consoleOverlayFallbackActive: desktopSurfaceActive
        && AppController.overlay !== "" && !AppController.overlay.startsWith("desktop-stream-")
    property bool desktopSurfaceActive: targetDesktopSurface
    property bool modeInitialized: false
    readonly property bool settingsLoadedForSmokeTest: settingsLoaded
    readonly property bool consoleModePersistedForSmokeTest:
        ShellStore.settings.launchInConsoleMode === true
    readonly property bool modePersistenceBusyForSmokeTest:
        ShellStore.consoleSurfaceRequestId !== ""
    readonly property string modePersistenceErrorForSmokeTest: ShellStore.lastError
    readonly property var streamerSnapshotForSmokeTest: ShellStore.streamer
    readonly property bool shellCaptureEnabledForSmokeTest: ControllerInput.shellCaptureEnabled

    Timer {
        id: geometrySaveTimer
        interval: 450
        repeat: false
        onTriggered: {
            if (!window.geometryRestored || window.visibility !== ApplicationWindow.Windowed)
                return
            const savedWidth = Number(ShellStore.settings.windowWidth || 0)
            const savedHeight = Number(ShellStore.settings.windowHeight || 0)
            if (Math.round(window.width) !== savedWidth)
                ShellStore.setSetting("windowWidth", Math.round(window.width))
            if (Math.round(window.height) !== savedHeight)
                ShellStore.setSetting("windowHeight", Math.round(window.height))
        }
    }

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
    onActiveChanged: syncInputOwnership()
    onDesktopSurfaceActiveChanged: ShellStore.desktopUiActive = desktopSurfaceActive

    Shortcut {
        objectName: "shellFullscreenShortcut"
        sequence: "F11"
        context: Qt.ApplicationShortcut
        enabled: window.activeRoute !== "stream"
        onActivated: window.toggleFullscreen()
    }

    Connections {
        target: ShellStore
        function onFullscreenToggleRequested() {
            if (window.activeRoute === "stream")
                window.toggleFullscreen()
        }
    }

    function syncInputOwnership() {
        const shellOwnsInput = !window.active || AppController.route !== "stream"
            || ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)
        ControllerInput.inputSuspended = !window.active
            || (shellOwnsInput && ShellStore.settings.controllerMode === false)
        ControllerInput.shellCaptureEnabled = shellOwnsInput
            && ShellStore.settings.controllerMode !== false
    }

    Binding {
        target: ControllerInput
        property: "leftStickDeadzone"
        value: Number(ShellStore.settings.controllerLeftStickDeadzone ?? 24)
    }
    Binding {
        target: ControllerInput
        property: "rightStickDeadzone"
        value: Number(ShellStore.settings.controllerRightStickDeadzone ?? 27)
    }
    Binding {
        target: ControllerInput
        property: "vibrationIntensity"
        value: Number(ShellStore.settings.controllerVibrationIntensity ?? 100)
    }

    // StreamVideoItem normally owns gameplay keys, but fullscreen transitions
    // can briefly leave the Qt focus chain without an active item. Register the
    // shell-owned stats shortcuts at application scope so F3 never leaks to the
    // remote game or depends on item focus.
    Shortcut {
        objectName: "streamStatsShortcut"
        sequence: "F3"
        context: Qt.ApplicationShortcut
        enabled: window.streamStatsShortcutEnabled
        onActivated: ShellStore.applyStreamShortcutAction("toggle-stats")
    }
    Shortcut {
        objectName: "configuredStreamStatsShortcut"
        sequence: window.configuredStatsShortcut
        context: Qt.ApplicationShortcut
        enabled: window.streamStatsShortcutEnabled
            && window.configuredStatsShortcut !== ""
            && window.configuredStatsShortcut.toUpperCase() !== "F3"
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
        sequence: String(ShellStore.settings.shortcutToggleMicrophone || "Ctrl+Shift+M")
        context: Qt.ApplicationShortcut
        autoRepeat: false
        enabled: window.activeRoute === "stream" && ShellStore.microphoneToggleAvailable
        onActivated: ShellStore.toggleMicrophone()
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
        if (ShellStore.signedIn && AppController.route === "sign-in")
            AppController.navigate("home")
        window.synchronizeRenderedSurface()
    }

    function requestConsoleSurface(enabled) {
        ShellStore.requestConsoleSurface(enabled)
    }

    function synchronizeRenderedSurface() {
        if (window.desktopSurfaceActive === window.targetDesktopSurface)
            return
        window.desktopSurfaceActive = window.targetDesktopSurface
        if (modeInitialized)
            modeTransition.restart()
    }

    function notePointerInput() {
        window.pointerRecentlyActive = true
        pointerGrace.restart()
        if (window.leaveConsoleOnPointer && effectiveLaunchMode !== "console" && !window.forceConsole) {
            window.desktopSelectedByPointer = true
            window.consoleHeldByPad = false
            window.startupConsoleRequested = false
        }
    }

    function noteControllerInput(device, control, value) {
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
    }

    function updateStreamSurfaceLock() {
        if (window.activeRoute === "stream") {
            if (!window.streamSurfaceLocked)
                window.lockedStreamDesktopSurface = window.desktopSurfaceActive
            window.streamSurfaceLocked = true
        } else {
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
            sourceComponent: window.onboardingVisible ? onboardingScreen
                : window.desktopSurfaceActive ? desktopAppScreen : window.componentForRoute(window.activeRoute)
            opacity: 1

            onLoaded: {
                if (item) {
                    item.forceActiveFocus()
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
            function onRouteChanged() {
                window.updateSessionWindowMode()
                window.updateStreamSurfaceLock()
                // The desktop shell and embedded video stay opaque. Animate
                // the entering panel, never flash the entire app on navigation.
                window.syncInputOwnership()
                window.streamStatsAutoShown = false
                Qt.callLater(() => window.showConfiguredStreamStats())
            }
            function onOverlayChanged() {
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
                ShellStore.acceptDirectLaunch(appId, title)
            }
        }
        Connections {
            target: ShellStore
            function onStreamerChanged() { window.showConfiguredStreamStats() }
            function onConsoleSurfaceRequested(enabled) { window.applyConsoleSurface(enabled) }
            function onOnboardingCompleted() {
                if (window.activeRoute === "sign-in")
                    AppController.navigate("home")
            }
            function onSettingsChanged() {
                window.initializeStartupMode()
                window.syncInputOwnership()
                if (window.geometryRestored || !ShellStore.settings.windowWidth)
                    return
                window.width = Number(ShellStore.settings.windowWidth)
                window.height = Number(ShellStore.settings.windowHeight)
                window.geometryRestored = true
            }
        }

        Keys.onPressed: event => {
            if (window.onboardingVisible) {
                event.accepted = true
                return
            }
            if (event.key === Qt.Key_F11 && window.activeRoute === "stream") {
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
                event.accepted = AppController.showOverlay(window.desktopSurfaceActive
                    && window.activeRoute === "stream" ? "desktop-stream-menu" : "guide-session")
            } else if (event.key === Qt.Key_Menu) {
                event.accepted = AppController.showOverlay("quick-settings")
            } else if (event.key === Qt.Key_Y) {
                event.accepted = AppController.showOverlay("friends")
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
            ? 1 : Math.min(window.width / 1920, window.height / 1080)
        x: Math.round((window.width - width * consoleScale) / 2)
        y: Math.round((window.height - height * consoleScale) / 2)
        width: desktopSessionConflict ? window.width : 1920
        height: desktopSessionConflict ? window.height : 1080
        scale: consoleScale
        transformOrigin: Item.TopLeft
        overlay: AppController.overlay
        visible: (!window.desktopSurfaceActive || window.consoleOverlayFallbackActive || consoleOverlayHost.present)
            && !window.streamQmlOverlayActive
        z: 1000
    }

    DesktopStreamOverlayHost {
        id: desktopStreamOverlay
        notificationsEnabled: window.desktopSurfaceActive && window.activeRoute === "stream"
        layer.enabled: HdrOutput.chromeRequired
        layer.effect: HdrChromeEffect {}
        anchors.fill: parent
        frameGenerationStats: window.frameGenerationStats
        pointerLocked: window.activeRoute === "stream" && routeLoader.item
            && routeLoader.item.streamPointerLocked === true
        overlay: AppController.overlay
        inputBlocking: ShellStore.streamOverlayBlocksGameplayInput(AppController.overlay)
        visible: window.streamQmlOverlayActive
            || ((window.activeRoute === "stream" || window.activeRoute === "inserting") && desktopStreamOverlay.present)
        z: 1100
        onVisibleChanged: if (visible && inputBlocking) forceActiveFocus()
        onInputBlockingChanged: if (visible && inputBlocking) forceActiveFocus()
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
        ScriptAction {
            script: {
                if (ShellStore.signedIn && AppController.route === "sign-in")
                    AppController.navigate("home")
            }
        }
        NumberAnimation { target: modeCurtain; property: "opacity"; to: 0; duration: AppController.reducedMotion ? 0 : 380; easing.type: Easing.OutCubic }
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
    Component { id: accountsScreen; AccountsScreen {} }
    Component { id: profilePinScreen; ProfilePinScreen {} }
    Component { id: gameAccountsScreen; GameAccountsScreen {} }
    Component { id: persistentStorageScreen; PersistentStorageScreen {} }
    Component { id: mediaScreen; MediaScreen {} }
    Component { id: diagnosticsScreen; DiagnosticsScreen {} }
    Component { id: updateScreen; UpdateScreen {} }
    Component { id: feedbackScreen; FeedbackScreen {} }
}
