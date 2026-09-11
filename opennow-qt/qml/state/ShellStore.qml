pragma Singleton
import QtQuick
import "catalog"
import "settings"
import "account"

QtObject {
    id: root
    property CatalogState catalogOwnerState: CatalogState {
        id: catalogOwner
        coreClient: CoreClient
        appController: AppController
        ready: root.ready
        signedIn: root.signedIn
        settings: root.settings
        setSetting: root.setSetting
        applySetting: root.applySetting
        onAccessibilityAnnounced: message => root.accessibilityMessage = message
        onStoreSessionReset: root.storeSessionReset()
    }

    property ArtworkState artworkOwnerState: ArtworkState {
        id: artworkOwner
        coreClient: CoreClient
        ready: root.ready
    }

    property SettingsState settingsOwnerState: SettingsState {
        id: settingsOwner
        coreClient: CoreClient
        appController: AppController
        i18n: I18n
        ready: root.ready
        subscription: root.subscription
        nativeRuntimeReady: root.nativeRuntimeReady
        nativeRuntimeCapabilities: root.nativeRuntimeCapabilities
        refreshAccountServices: root.refreshAccountServices
        refreshStreamerDetection: root.refreshStreamerDetection
        syncDiscordPresence: root.syncDiscordPresence
        syncTelemetry: root.syncTelemetry
        lastError: root.lastError
        onConsoleSurfaceRequested: enabled => root.consoleSurfaceRequested(enabled)
        onAccessibilityAnnounced: message => root.accessibilityMessage = message
        onErrorReported: message => root.lastError = message
    }

    property AccountServicesState accountServicesOwnerState: AccountServicesState {
        id: accountServicesOwner
        coreClient: CoreClient
        appController: AppController
        ready: root.ready
        signedIn: root.signedIn
        reloadCatalogForSession: root.reloadCatalogForSession
        refreshAccountServices: root.refreshAccountServices
        onAccessibilityAnnounced: message => root.accessibilityMessage = message
    }

    property alias settings: settingsOwner.settings
    property var onboardingAwdlController: MacAwdl
    readonly property bool onboardingAwdlReady: !onboardingAwdlController.busy
        && [MacAwdlController.Unsupported, MacAwdlController.Unavailable, MacAwdlController.Disabled]
            .indexOf(onboardingAwdlController.state) >= 0
    property OnboardingState onboardingOwnerState: OnboardingState {
        id: onboardingOwner
        coreClient: CoreClient
        persistedSettings: root.settings
        ready: root.ready
        signedIn: root.signedIn
        checkRequirements: root.checkOnboardingRequirements
        replayAllowed: root.ready && !root.activeSession && !root.streamBusy
            && root.activeSessionRequestId === "" && root.sessionClaimRequestId === ""
            && ["idle", "error"].indexOf(root.streamState) >= 0
            && ["starting", "streaming", "stopping"].indexOf(root.streamerStatus) < 0
        onRestartRequested: AppController.restartApplication()
        onRequirementsMissing: root.onboardingRequirementsMissing()
        onSettingSaved: (key, value, changes) => {
            settingsOwner.applyCoupledSettings(changes)
            settingsOwner.applySetting(key, value)
        }
        onCompleted: {
            root.consoleSurfaceRequested(root.settings.launchInConsoleMode === true)
            root.onboardingCompleted()
            Qt.callLater(root.resolveDirectLaunch)
        }
    }
    readonly property bool onboardingRequired: onboardingOwner.needed
    readonly property var onboardingSettings: onboardingOwner.settings
    readonly property bool onboardingSaving: onboardingOwner.saving
    readonly property string onboardingError: onboardingOwner.error
    readonly property bool onboardingReplayAvailable: onboardingOwner.replayAllowed
        && !onboardingOwner.saving && !onboardingOwner.replaying
    readonly property bool onboardingReplaying: onboardingOwner.replaying
    readonly property string onboardingReplayError: onboardingOwner.replayError
    signal onboardingCompleted()
    signal onboardingRequirementsMissing()

    function checkOnboardingRequirements() {
        onboardingAwdlController.refresh()
        if (onboardingAwdlReady)
            return ""
        if (onboardingAwdlController.busy)
            return qsTr("Wait for macOS authorization to finish before continuing setup.")
        if (onboardingAwdlController.state === MacAwdlController.Unknown)
            return qsTr("AWDL status could not be verified. Refresh its status in Boost before continuing setup.")
        return qsTr("Disable AWDL in Boost before continuing setup on this Mac.")
    }

    function verifyOnboardingRequirements() {
        return onboardingOwner.verifyRequirements()
    }

    function setOnboardingSetting(key, value) {
        onboardingOwner.setSetting(key, value)
        if (key === "resolution" || key === "fps")
            onboardingOwner.setSetting("fps", settingsOwner.resolveEntitledFps(
                onboardingSettings.resolution, onboardingSettings.fps))
    }

    function finishOnboarding() {
        onboardingOwner.finish()
    }

    function replayOnboarding() {
        onboardingOwner.replay()
    }

    property alias previewThemePack: settingsOwner.previewThemePack
    property string accessibilityMessage: ""
    property alias settingsRequestId: settingsOwner.settingsRequestId
    property string lastError: ""
    property var focusPositions: ({})
    property var providers: []
    property var authSession: null
    property var authChallenge: null
    property string authState: "idle"
    property string authMessage: ""
    property alias catalogGames: catalogOwner.catalogGames
    readonly property var gameCollections: catalogOwner.gameCollections
    property alias activeCollectionId: catalogOwner.activeCollectionId
    readonly property var activeCollection: catalogOwner.activeCollection
    readonly property bool collectionsBusy: catalogOwner.collectionsBusy
    property alias collectionError: catalogOwner.collectionError
    signal collectionSaved(string collectionId)
    property Connections collectionSignals: Connections {
        target: catalogOwner
        function onCollectionSaved(collectionId) { root.collectionSaved(collectionId) }
    }

    function collectionNameError(name, exceptId) { return catalogOwner.collectionNameError(name, exceptId) }
    function createCollection(name, game) { return catalogOwner.createCollection(name, game) }
    function renameCollection(id, name) { return catalogOwner.renameCollection(id, name) }
    function deleteCollection(id) { return catalogOwner.deleteCollection(id) }
    function toggleCollectionGame(id, game) { return catalogOwner.toggleCollectionGame(id, game) }
    function isInCollection(game, id) { return catalogOwner.isInCollection(game, id) }
    property alias selectedGame: catalogOwner.selectedGame
    property alias catalogTotalCount: catalogOwner.catalogTotalCount
    property alias catalogState: catalogOwner.catalogState
    property alias catalogSource: catalogOwner.catalogSource
    property alias storeGames: catalogOwner.storeGames
    property alias storeFacets: catalogOwner.storeFacets
    property alias storeFilters: catalogOwner.storeFilters
    property alias storeUsesLocalIndex: catalogOwner.storeUsesLocalIndex
    signal storeSessionReset()
    property alias storeTotalCount: catalogOwner.storeTotalCount
    property alias storeState: catalogOwner.storeState
    property alias storeSource: catalogOwner.storeSource
    property alias storeError: catalogOwner.storeError
    property alias storeWarning: catalogOwner.storeWarning
    property alias storeSearchQuery: catalogOwner.storeSearchQuery
    property alias storeNextCursor: catalogOwner.storeNextCursor
    property alias storeHasMore: catalogOwner.storeHasMore
    property alias storeReplacePage: catalogOwner.storeReplacePage
    property alias storePageCount: catalogOwner.storePageCount
    property alias storeSeenCursors: catalogOwner.storeSeenCursors
    property alias storePresentationRequestId: catalogOwner.storePresentationRequestId
    property alias storePresentationIndex: catalogOwner.storePresentationIndex
    property alias storeBrowseCache: catalogOwner.storeBrowseCache
    property alias storeForceRefresh: catalogOwner.storeForceRefresh
    property alias storeLastPageCached: catalogOwner.storeLastPageCached
    readonly property bool storeLoading: catalogOwner.storeLoading
    property alias storePageTimer: catalogOwner.storePageTimer
    property alias storeMarquee: catalogOwner.storeMarquee
    property alias storePanels: catalogOwner.storePanels
    property alias storeFilterGroups: catalogOwner.storeFilterGroups
    property string sessionPersistence: "none"
    property bool authRestorePending: true
    property bool pendingStaySignedIn: true
    signal consoleSurfaceRequested(bool enabled)
    property alias consoleSurfaceRequestId: settingsOwner.consoleSurfaceRequestId
    property alias consoleSurfaceConfirmedValue: settingsOwner.consoleSurfaceConfirmedValue
    property alias consoleSurfaceDesiredValue: settingsOwner.consoleSurfaceDesiredValue
    property alias consoleSurfaceRequestValue: settingsOwner.consoleSurfaceRequestValue
    property alias consoleSurfaceInitialized: settingsOwner.consoleSurfaceInitialized
    property alias consoleSurfaceError: settingsOwner.consoleSurfaceError
    property bool desktopUiActive: false
    property alias subscription: accountServicesOwner.subscription
    property alias regions: accountServicesOwner.regions
    property alias regionPingResults: accountServicesOwner.regionPingResults
    property alias regionPingMessage: accountServicesOwner.regionPingMessage
    property alias regionPingPending: accountServicesOwner.regionPingPending
    readonly property bool regionPingBusy: accountServicesOwner.regionPingBusy
    property var savedAccounts: []
    property alias gameAccounts: accountServicesOwner.gameAccounts
    property alias gameAccountsState: accountServicesOwner.gameAccountsState
    property alias gameAccountMessage: accountServicesOwner.gameAccountMessage
    property alias accountLinkAttempt: accountServicesOwner.accountLinkAttempt
    property alias storageLocations: accountServicesOwner.storageLocations
    property alias storageMessage: accountServicesOwner.storageMessage
    property var mediaItems: []
    property string mediaRootPath: ""
    property string mediaState: "idle"
    property string mediaMessage: ""
    property var diagnostics: ({entries: []})
    property string diagnosticsMessage: ""
    property var updaterState: ({status: "idle", currentVersion: Qt.application.version, canCheck: true})
    property var releaseHighlights: ({})
    property var socialCapabilities: ({
        friendsAvailable: false,
        presenceAvailable: false,
        invitesAvailable: false,
        localControllerJoin: true,
        reason: qsTr("Checking provider social capabilities…")
    })
    property string reportingMessage: ""
    property string reportingState: "idle"
    property string pinMode: "unlock"
    property string pinTargetUserId: ""
    property string pinTargetName: qsTr("Profile")
    property string pinMessage: ""
    property var activeSession: null
    property string dropSessionId: ""
    property string sessionReportDropId: ""
    property var streamDropCounts: ({})
    onActiveSessionChanged: {
        const sessionId = String(activeSession && activeSession.sessionId || "")
        if (sessionId !== "" && sessionId !== dropSessionId) {
            dropSessionId = sessionId
            streamDropCounts = {videoDropCount: 0, audioDiscardedMs: 0,
                audioPacketDropCount: 0, callbackDropCount: 0, otherQueueDropCount: 0}
        }
    }
    onStreamDropCountsChanged: {
        if (lastSessionReport && sessionReportDropId !== "" && sessionReportDropId === dropSessionId)
            lastSessionReport = Object.assign({}, lastSessionReport, {drops: streamDropCounts})
    }
    property var remoteSessions: []
    property var pendingLaunchParams: null
    property var pendingDirectLaunch: null
    property var conflictSession: null
    property bool launchConflictDetected: false
    property bool forceNewAfterStop: false
    property var streamer: null
    property var streamerDetection: ({available: false, availableCodecs: [], capabilities: ({})})
    property string streamerDetectionMessage: qsTr("Checking native codec support…")
    property string streamState: "idle"
    property string streamMessage: ""
    property int streamerRestartAttempts: 0
    property bool streamerRecoveryExhausted: false
    property int sessionReconnectAttempts: 0
    readonly property int maximumSessionReconnectAttempts: 8
    property bool sessionRecoveryPending: false
    property string recoverySessionId: ""
    property string recoveryDiscoveryRequestId: ""
    property int resumePollAttempts: 0
    property double resumePollDeadlineMs: 0
    property int streamerRestartRecoveryCount: 0
    property int sessionRecoveryCount: 0
    property var guidePagesVisited: []
    property alias regionsVpcId: accountServicesOwner.regionsVpcId
    property string providersRequestId: ""
    property string authSessionRequestId: ""
    property string activeSessionRequestId: ""
    property string remoteSessionDiscoveryRequestId: ""
    property string remoteSessionsRequestId: ""
    property string sessionClaimRequestId: ""
    property bool sessionClaimIsRecovery: false
    property string streamCreateRequestId: ""
    property string streamPollRequestId: ""
    property string streamStopRequestId: ""
    property string streamerStartRequestId: ""
    property string streamerPrepareRequestId: ""
    property string streamerStopRequestId: ""
    property bool streamerStopExpected: false
    property alias artworkUrls: artworkOwner.artworkUrls
    property alias artworkPending: artworkOwner.artworkPending
    property alias artworkRequestSources: artworkOwner.artworkRequestSources
    property alias artworkRetrySources: artworkOwner.artworkRetrySources
    property alias artworkInterests: artworkOwner.artworkInterests
    property alias storeShelfCache: catalogOwner.storeShelfCache
    property alias storeShelfEpoch: catalogOwner.storeShelfEpoch

    function cachedStoreShelf(category, limit) {
        return catalogOwner.cachedStoreShelf(category, limit)
    }

    function cacheStoreShelf(category, limit, games) {
        return catalogOwner.cacheStoreShelf(category, limit, games)
    }

    function resetStoreShelves() {
        return catalogOwner.resetStoreShelves()
    }
    property alias catalogRequestId: catalogOwner.catalogRequestId
    property alias storeRequestId: catalogOwner.storeRequestId
    property string deviceStartRequestId: ""
    property string devicePollRequestId: ""
    property string deviceCompleteRequestId: ""
    property string logoutRequestId: ""
    property string logoutAllRequestId: ""
    property alias subscriptionRequestId: accountServicesOwner.subscriptionRequestId
    property alias regionsRequestId: accountServicesOwner.regionsRequestId
    property alias regionPingRequestId: accountServicesOwner.regionPingRequestId
    property string accountsRequestId: ""
    property string accountSwitchRequestId: ""
    property string accountRemoveRequestId: ""
    property string pinRequestId: ""
    property alias gameAccountsRequestId: accountServicesOwner.gameAccountsRequestId
    property alias gameAccountActionRequestId: accountServicesOwner.gameAccountActionRequestId
    property alias accountLinkStartRequestId: accountServicesOwner.accountLinkStartRequestId
    property alias accountLinkPollRequestId: accountServicesOwner.accountLinkPollRequestId
    property alias storageLocationsRequestId: accountServicesOwner.storageLocationsRequestId
    property alias storageResetRequestId: accountServicesOwner.storageResetRequestId
    property string sessionAdRequestId: ""
    property string mediaRequestId: ""
    property string mediaDeleteRequestId: ""
    property string mediaRecordingTargetRequestId: ""
    property string streamRecordingStartRequestId: ""
    property string streamRecordingStopRequestId: ""
    property string pendingRecordingPath: ""
    property string pendingRecordingThumbnailPath: ""
    property string mediaClipTargetRequestId: ""
    property string streamClipRequestId: ""
    readonly property bool streamClipBusy: mediaClipTargetRequestId !== "" || streamClipRequestId !== ""
    property bool streamReplayEnabled: false
    readonly property bool replayBufferRequested: settings.replayBufferEnabled === true
    onReplayBufferRequestedChanged: {
        if (!replayBufferRequested)
            disableStreamReplay()
    }
    property bool streamRecordingActive: false
    property double streamRecordingElapsedMs: 0
    property double streamRecordingStartedAtMs: 0
    property string diagnosticsRequestId: ""
    property string diagnosticsExportRequestId: ""
    property string acceptanceExportRequestId: ""
    property string updaterStateRequestId: ""
    property string updaterCheckRequestId: ""
    property string updaterHighlightsRequestId: ""
    property string updaterDownloadRequestId: ""
    property string updaterInstallRequestId: ""
    property string socialCapabilitiesRequestId: ""
    property string discordRequestId: ""
    property double streamStartedAtMs: 0
    property string telemetryRequestId: ""
    property string feedbackRequestId: ""
    property string bugReportRequestId: ""
    property string streamInputPauseRequestId: ""
    property string streamControlRequestId: ""
    property string streamControlAction: ""
    property string streamControlMessage: ""
    property rect streamCaptureRect: Qt.rect(0, 0, 0, 0)
    property bool nativeRuntimeReady: false
    readonly property int nativeProtocolVersion: 6
    property var nativeRuntimeCapabilities: ({})
    property var audioOutputDevices: []
    property bool audioOutputDevicesBusy: false
    property string audioOutputDevicesError: ""
    property string audioOutputDevicesRequestId: ""
    property Timer audioOutputDevicesTimeout: Timer {
        interval: 10000
        onTriggered: {
            root.takeNativeRequest(root.audioOutputDevicesRequestId)
            root.audioOutputDevicesRequestId = ""
            root.audioOutputDevicesBusy = false
            root.audioOutputDevices = []
            root.audioOutputDevicesError = qsTr("Audio device discovery timed out. Try refreshing again.")
        }
    }
    property int nativeRequestSequence: 0
    property var nativeRequests: ({})
    property int overlayRequestGeneration: 0
    property int screenshotRequestGeneration: 0
    property int recordingToggleRequestGeneration: 0
    property int shortcutActionGeneration: 0
    property var runtimeStreamProfile: ({})
    property bool antiAfkEnabled: false
    property var lastSessionReport: null
    property bool desiredStreamInputPaused: false
    property bool currentStreamInputPaused: false
    property bool streamInputStateKnown: false
    readonly property bool ready: CoreClient.state === "ready"
    readonly property bool signedIn: authSession !== null
    readonly property string sessionStatus: activeSession
        ? String(activeSession.phase || activeSession.status || "requesting") : "idle"
    readonly property string streamerStatus: streamer ? String(streamer.status || "unknown") : "stopped"
    readonly property var negotiatedStreamProfile: activeSession && activeSession.negotiatedStreamProfile
        ? activeSession.negotiatedStreamProfile : ({})
    readonly property int streamerRecoveryCount: streamerRestartRecoveryCount
        + Number(streamer && streamer.deviceRecoveryCount || 0)
    readonly property string sessionPersistenceMessage: {
        if (sessionPersistence === "unavailable")
            return qsTr("A saved NVIDIA session could not be opened. Sign in once more to store it on this PC.")
        if (sessionPersistence === "memory-only")
            return qsTr("This session is memory-only and will not last after you quit.")
        return ""
    }
    readonly property bool streamBusy: streamCreateRequestId !== "" || streamStopRequestId !== ""
        || remoteSessionsRequestId !== "" || sessionClaimRequestId !== ""

    signal fullscreenToggleRequested()
    signal pointerLockToggleRequested()
    signal streamCaptureAnnounced(string message)
    readonly property var resumableSession: {
        if (root.activeSession) {
            const localStatus = Number(root.activeSession.status || 0)
            const localPhase = String(root.activeSession.phase || "").toLowerCase()
            if ((localStatus >= 2 && localStatus <= 5)
                    || localPhase === "ready" || localPhase === "streaming")
                return root.activeSession
        }
        const sessions = root.remoteSessions || []
        for (let index = 0; index < sessions.length; ++index) {
            const status = Number(sessions[index] && sessions[index].status || 0)
            if (status >= 2 && status <= 5)
                return sessions[index]
        }
        return null
    }
    property string sessionMicrophoneMode: "disabled"
    property string microphoneRequestId: ""
    property string microphoneRecoverySessionId: ""
    property bool microphoneRecoveryEnabled: false
    readonly property bool microphoneCaptureSupported: nativeRuntimeReady
        && nativeRuntimeCapabilities.supportsMicrophone === true
    readonly property bool microphoneSessionActive: Boolean(activeSession && streamer
        && streamer.status === "streaming")
    readonly property bool microphoneSessionSupported: microphoneSessionActive
        && Boolean(streamer.capabilities && streamer.capabilities.supportsMicrophone === true)
    readonly property bool microphoneToggleAvailable: microphoneSessionSupported
        && sessionMicrophoneMode === "voice-activity"
    readonly property bool microphoneCanToggle: microphoneToggleAvailable && microphoneRequestId === ""
    readonly property string microphoneState: !microphoneSessionActive ? "disabled"
        : String(streamer.microphoneState || (sessionMicrophoneMode === "disabled" ? "disabled"
            : microphoneSessionSupported ? "muted" : "unavailable"))
    readonly property bool microphoneEnabled: microphoneSessionActive
        && microphoneState === "ready" && streamer.microphoneEnabled === true
    readonly property string microphoneLabel: microphoneState === "ready" && microphoneEnabled
        ? qsTr("Open microphone") : microphoneState === "muted" ? qsTr("Muted")
        : microphoneState === "error" ? qsTr("Microphone error")
        : microphoneState === "unavailable" ? qsTr("Unavailable") : qsTr("Disabled")
    readonly property string microphoneActionLabel: microphoneEnabled
        ? qsTr("Mute microphone") : qsTr("Unmute microphone")
    readonly property string microphoneDescription: microphoneSessionActive && streamer.microphoneMessage
        ? String(streamer.microphoneMessage)
        : qsTr("Uses the system default microphone. Open microphone sends audio continuously during supported sessions. Changes apply to the next session.")

    property Timer devicePollTimer: Timer {
        interval: root.authChallenge ? Math.max(1000, Number(root.authChallenge.intervalSeconds || 5) * 1000) : 5000
        repeat: true
        running: false
        onTriggered: root.pollDeviceLogin()
    }

    property Timer streamPollTimer: Timer {
        interval: 1500
        repeat: true
        running: false
        onTriggered: root.pollStreamingSession()
    }

    property Timer remoteSessionRefreshTimer: Timer {
        interval: 30000
        repeat: true
        running: root.signedIn && AppController.route !== "stream"
        onTriggered: root.refreshRemoteSessions()
    }

    property Timer antiAfkPulseTimer: Timer {
        interval: 240000
        repeat: true
        running: root.antiAfkEnabled && root.activeSession && root.streamer
            && root.streamer.status === "streaming"
        onTriggered: root.controlStream("anti-afk-pulse")
    }

    property alias artworkRetryTimer: artworkOwner.artworkRetryTimer

    property Timer streamRecordingTimer: Timer {
        interval: 250
        repeat: true
        running: root.streamRecordingActive
        onTriggered: root.streamRecordingElapsedMs = Math.max(0, Date.now() - root.streamRecordingStartedAtMs)
    }

    property alias accountLinkPollTimer: accountServicesOwner.accountLinkPollTimer

    property Timer streamerRestartTimer: Timer {
        interval: Math.min(8000, 1000 * Math.pow(2, Math.min(3, root.sessionReconnectAttempts)))
        repeat: false
        onTriggered: root.recoverStreamingSession(root.streamMessage)
    }

    property Timer recoveryStopTimer: Timer {
        interval: 30000
        repeat: false
        running: root.sessionRecoveryPending && root.streamerStopRequestId !== ""
        onTriggered: {
            // Keep the outstanding stop ownership: starting another connection
            // while a decoder/transport worker is still alive is unsafe.
            root.sessionRecoveryPending = false
            root.streamState = "error"
            root.streamMessage = qsTr("The previous native stream is still stopping. Retry after cleanup finishes, or restart OpenNOW.")
            root.lastError = root.streamMessage
        }
    }

    property Timer autoUpdateCheckTimer: Timer {
        interval: 15000
        repeat: false
        onTriggered: root.checkForUpdates()
    }

    function refreshSettings() {
        return settingsOwner.refreshSettings()
    }

    function initializeServices() {
        if (!ready)
            return
        if (!signedIn)
            authRestorePending = true
        refreshSettings()
        providersRequestId = CoreClient.request("auth.providers.list", {}, 25000)
        authSessionRequestId = CoreClient.request("auth.session.get", {})
        activeSessionRequestId = CoreClient.request("session.active.get", {})
        ensureNativeRuntimeReady()
        updaterStateRequestId = CoreClient.request("updater.state.get", {})
        socialCapabilitiesRequestId = CoreClient.request("social.capabilities.get", {})
        refreshCatalog()
    }

    function refreshStreamerDetection() {
        streamerDetectionMessage = qsTr("Checking native codec support…")
        ensureNativeRuntimeReady()
        if (nativeRuntimeReady)
            acceptNativeCapabilities(nativeRuntimeCapabilities)
    }

    function refreshRemoteSessions() {
        if (!ready || !signedIn || activeSession || remoteSessionDiscoveryRequestId !== ""
                || remoteSessionsRequestId !== "")
            return
        remoteSessionDiscoveryRequestId = CoreClient.request("session.remote.list", {}, 30000)
    }

    function sessionGameTitle(session) {
        if (!session)
            return ""
        const appId = String(session.appId || "")
        const games = catalogGames || []
        for (let gameIndex = 0; gameIndex < games.length; ++gameIndex) {
            const game = games[gameIndex]
            if (String(game.launchAppId || "") === appId)
                return String(game.title || "")
            const variants = game.variants || []
            for (let variantIndex = 0; variantIndex < variants.length; ++variantIndex) {
                if (String(variants[variantIndex] && variants[variantIndex].id || "") === appId)
                    return String(game.title || "")
            }
        }
        return ""
    }

    function selectGameForSession(session) {
        if (!session)
            return
        const title = sessionGameTitle(session)
        if (!title) {
            selectedGame = {title: qsTr("Your running game"), launchAppId: String(session.appId || "")}
            return
        }
        const games = catalogGames || []
        for (let index = 0; index < games.length; ++index) {
            if (String(games[index].title || "") === title) {
                selectedGame = games[index]
                return
            }
        }
    }

    function resumeActiveSession() {
        const session = resumableSession
        if (!session || sessionClaimRequestId !== "")
            return
        selectGameForSession(session)
        if (activeSession && String(activeSession.sessionId || "") === String(session.sessionId || "")) {
            if (AppController.route !== "stream")
                AppController.navigate("stream")
            if (!streamer || streamer.status === "stopped" || streamer.status === "error")
                recoverStreamingSession(qsTr("Resuming your active GeForce NOW session…"))
            return
        }
        conflictSession = session
        streamState = "resuming"
        streamMessage = qsTr("Resuming your active GeForce NOW session…")
        sessionClaimIsRecovery = false
        sessionClaimRequestId = CoreClient.request("session.claim", {
            sessionId: session.sessionId,
            streamingBaseUrl: session.streamingBaseUrl,
            appId: String(session.appId || "0"),
            recoveryMode: true
        }, 35000)
        AppController.navigate("inserting")
    }

    function codecNamesFromCapabilities(capabilities) {
        return settingsOwner.codecNamesFromCapabilities(capabilities)
    }

    function acceptNativeCapabilities(capabilities) {
        nativeRuntimeCapabilities = capabilities || ({})
        const codecs = codecNamesFromCapabilities(nativeRuntimeCapabilities)
        streamerDetection = {
            available: Boolean(nativeRuntimeCapabilities.supportsVideoDecode
                && nativeRuntimeCapabilities.supportsVideoPresent),
            capabilities: nativeRuntimeCapabilities,
            availableCodecs: codecs
        }
        streamerDetectionMessage = codecs.length
            ? qsTr("Available: ") + codecs.map(codec => String(codec).toUpperCase()).join(", ")
            : qsTr("No native video decoder is available")
    }

    function sendNativeCommand(type, params, operation) {
        if (!NativeStreamRuntime.running && !NativeStreamRuntime.start()) {
            const message = NativeStreamRuntime.lastError || qsTr("The embedded media runtime could not start")
            lastError = message
            return ""
        }
        nativeRequestSequence += 1
        const requestId = "qt-" + nativeRequestSequence
        const command = Object.assign({id: requestId, type: type}, params || ({}))
        if (!NativeStreamRuntime.send(command)) {
            lastError = NativeStreamRuntime.lastError || qsTr("The embedded media runtime rejected a command")
            return ""
        }
        const requests = Object.assign({}, nativeRequests)
        requests[requestId] = Object.assign({operation: operation || type}, params || ({}))
        nativeRequests = requests
        return requestId
    }

    function takeNativeRequest(requestId) {
        const pending = nativeRequests[requestId]
        if (pending === undefined)
            return null
        const requests = Object.assign({}, nativeRequests)
        delete requests[requestId]
        nativeRequests = requests
        return pending
    }

    function refreshAudioOutputDevices() {
        if (audioOutputDevicesBusy || !nativeRuntimeReady)
            return
        audioOutputDevicesError = ""
        audioOutputDevicesBusy = true
        audioOutputDevicesRequestId = sendNativeCommand("audioDevices", {}, "audioDevices")
        if (audioOutputDevicesRequestId === "") {
            audioOutputDevicesBusy = false
            audioOutputDevicesError = lastError
        } else {
            audioOutputDevicesTimeout.restart()
        }
    }

    function ensureNativeRuntimeReady() {
        if (nativeRuntimeReady)
            return
        const requests = nativeRequests || ({})
        for (const requestId in requests) {
            if (requests[requestId].operation === "hello")
                return
        }
        const requestId = sendNativeCommand("hello", {
            protocolVersion: nativeProtocolVersion
        }, "hello")
        if (requestId === "" && streamer && streamer.status === "starting")
            updateStreamerFields({status: "error",
                message: lastError || qsTr("The embedded media runtime could not start"),
                errorCode: "streamer_start_failed"})
    }

    function codecAvailable(codec) {
        return settingsOwner.codecAvailable(codec)
    }

    function hdrDecoderAvailable() {
        return settingsOwner.hdrDecoderAvailable()
    }

    function availableCodecValues() {
        return settingsOwner.availableCodecValues()
    }

    function availableCodecLabels() {
        return settingsOwner.availableCodecLabels()
    }

    function canonicalFpsValues() {
        return settingsOwner.canonicalFpsValues()
    }

    function presetFpsForResolution(width, height) {
        return settingsOwner.presetFpsForResolution(width, height)
    }

    function isFpsCoveredByEntitlement(width, height, fps) {
        return settingsOwner.isFpsCoveredByEntitlement(width, height, fps)
    }

    function entitledFpsForResolution(resolution) {
        return settingsOwner.entitledFpsForResolution(resolution)
    }

    function unentitledFpsValues(resolution) {
        return settingsOwner.unentitledFpsValues(resolution)
    }

    function resolveEntitledFps(resolution, requested) {
        return settingsOwner.resolveEntitledFps(resolution, requested)
    }

    function clampFpsToEntitlement() {
        return settingsOwner.clampFpsToEntitlement()
    }

    function refreshCatalog(searchQuery) {
        return catalogOwner.refreshCatalog(searchQuery)
    }

    function reloadCatalogForSession() {
        return catalogOwner.reloadCatalogForSession()
    }

    function ensureStore(searchQuery, filters) {
        return catalogOwner.ensureStore(searchQuery, filters)
    }

    function videoBackendItems() {
        return settingsOwner.videoBackendItems()
    }

    function resolutionItems() {
        return settingsOwner.resolutionItems()
    }

    function refreshStore(searchQuery, forceRefresh, filters) {
        return catalogOwner.refreshStore(searchQuery, forceRefresh, filters)
    }

    function cancelStoreRequests() {
        return catalogOwner.cancelStoreRequests()
    }

    function requestStorePage() {
        return catalogOwner.requestStorePage()
    }

    function requestStorePresentation() {
        return catalogOwner.requestStorePresentation()
    }

    function retryStore() {
        return catalogOwner.retryStore()
    }

    function acceptStorePage(result) {
        return catalogOwner.acceptStorePage(result)
    }

    function reloadStoreForSession() {
        return catalogOwner.reloadStoreForSession()
    }

    function refreshAccountServices() {
        if (!ready || !signedIn)
            return
        if (subscriptionRequestId === "")
            subscriptionRequestId = CoreClient.request("account.subscription.get", {}, 30000)
        if (regionsRequestId === "")
            regionsRequestId = CoreClient.request("network.regions.list", {}, 30000)
        if (accountsRequestId === "")
            accountsRequestId = CoreClient.request("auth.accounts.list", {})
        if (gameAccountsRequestId === "") {
            gameAccountsState = gameAccounts.length ? "refreshing" : "loading"
            gameAccountsRequestId = CoreClient.request("account.connections.list", {}, 30000)
        }
        if (remoteSessionsRequestId === "" && !activeSession && !pendingLaunchParams)
            remoteSessionsRequestId = CoreClient.request("session.remote.list", {}, 30000)
    }

    function refreshRegions() {
        return accountServicesOwner.refreshRegions()
    }

    function pingRegions() {
        return accountServicesOwner.pingRegions()
    }

    function resetRegionPing() {
        return accountServicesOwner.resetRegionPing()
    }

    function refreshGameAccounts() {
        return accountServicesOwner.refreshGameAccounts()
    }

    function startAccountLink(provider) {
        return accountServicesOwner.startAccountLink(provider)
    }

    function pollAccountLink() {
        return accountServicesOwner.pollAccountLink()
    }

    function syncGameAccount(provider) {
        return accountServicesOwner.syncGameAccount(provider)
    }

    function unlinkGameAccount(provider) {
        return accountServicesOwner.unlinkGameAccount(provider)
    }

    function refreshStorageLocations() {
        return accountServicesOwner.refreshStorageLocations()
    }

    function resetPersistentStorage(regionCode) {
        return accountServicesOwner.resetPersistentStorage(regionCode)
    }

    function refreshMedia() {
        if (!ready || mediaRequestId !== "")
            return
        mediaState = mediaItems.length ? "refreshing" : "loading"
        mediaRequestId = CoreClient.request("media.list", {}, 15000)
    }

    function deleteMedia(item) {
        if (!ready || !item || mediaDeleteRequestId !== "")
            return
        mediaMessage = qsTr("Deleting ") + item.fileName + "…"
        mediaDeleteRequestId = CoreClient.request("media.delete", {
            kind: item.kind,
            id: item.id,
            confirmed: true
        }, 15000)
    }

    function refreshDiagnostics() {
        if (!ready || diagnosticsRequestId !== "")
            return
        diagnosticsMessage = qsTr("Loading redacted diagnostics…")
        diagnosticsRequestId = CoreClient.request("diagnostics.snapshot", {})
    }

    function exportDiagnostics() {
        if (!ready || diagnosticsExportRequestId !== "")
            return
        diagnosticsMessage = qsTr("Creating redacted diagnostic export…")
        diagnosticsExportRequestId = CoreClient.request("diagnostics.export", {
            runtimeCapabilities: nativeRuntimeCapabilities,
            embeddedStream: {drops: streamDropCounts},
            lastSessionReport: lastSessionReport ? {drops: lastSessionReport.drops} : null
        }, 15000)
    }

    function recordGuidePage(page) {
        const allowed = ["guide-session", "guide-controls", "guide-media", "guide-shortcuts"]
        if (allowed.indexOf(page) < 0 || guidePagesVisited.indexOf(page) >= 0)
            return
        guidePagesVisited = guidePagesVisited.concat([page])
    }

    function exportAcceptanceEvidence() {
        if (!ready || acceptanceExportRequestId !== "")
            return
        diagnosticsMessage = qsTr("Creating machine-verifiable live evidence…")
        acceptanceExportRequestId = CoreClient.request("acceptance.export", {
            windowSystem: Qt.platform.pluginName,
            shell: {
                streamerRecoveryCount: streamerRecoveryCount,
                sessionRecoveryCount: sessionRecoveryCount,
                guidePagesVisited: guidePagesVisited
            }
        }, 120000)
    }

    function checkForUpdates() {
        if (!ready || updaterCheckRequestId !== "")
            return
        updaterState = Object.assign({}, updaterState, {
            status: "checking",
            message: qsTr("Checking GitHub Releases…"),
            canCheck: false
        })
        updaterCheckRequestId = CoreClient.request("updater.check", {
            channel: settings.updateChannel || "stable"
        }, 30000)
    }

    function downloadUpdate() {
        if (!ready || updaterDownloadRequestId !== "" || !updaterState.canDownload)
            return
        updaterDownloadRequestId = CoreClient.request("updater.download", {}, 300000)
        updaterState = Object.assign({}, updaterState, {
            status: "downloading",
            message: qsTr("Downloading and verifying the signed update…"),
            canDownload: false,
            canCheck: false
        })
    }

    function installUpdate() {
        if (!ready || updaterInstallRequestId !== "" || !updaterState.canInstall)
            return
        updaterInstallRequestId = CoreClient.request("updater.install", {confirmed: true}, 30000)
    }

    function syncTelemetry() {
        if (!ready || telemetryRequestId !== "")
            return
        telemetryRequestId = CoreClient.request("telemetry.sync", {}, 30000)
    }

    function submitFeedback(category, message) {
        if (!ready || feedbackRequestId !== "")
            return
        reportingState = "submitting"
        reportingMessage = qsTr("Sending feedback…")
        feedbackRequestId = CoreClient.request("feedback.submit", {
            category: category,
            message: message,
            includeSystemInfo: true
        }, 30000)
    }

    function submitBugReport(title, description, includeDiagnostics) {
        if (!ready || bugReportRequestId !== "")
            return
        reportingState = "submitting"
        reportingMessage = qsTr("Uploading bug report…")
        bugReportRequestId = CoreClient.request("bug_report.submit", {
            title: title,
            description: description,
            includeDiagnostics: includeDiagnostics
        }, 60000)
    }

    function switchAccount(userId, pin) {
        if (ready && accountSwitchRequestId === "")
            accountSwitchRequestId = CoreClient.request("auth.accounts.switch", {
                userId: userId,
                pin: pin || ""
            }, 30000)
    }

    function removeAccount(userId) {
        if (ready && accountRemoveRequestId === "")
            accountRemoveRequestId = CoreClient.request("auth.accounts.remove", { userId: userId })
    }

    function openPin(mode, account) {
        pinMode = mode
        pinTargetUserId = account && account.userId ? account.userId : (authSession ? authSession.user.userId : "")
        pinTargetName = account && account.displayName ? account.displayName : (authSession ? authSession.user.displayName : "Profile")
        pinMessage = qsTr("")
        AppController.navigate("profile-pin")
    }

    function submitPin(pin) {
        if (!ready || pinRequestId !== "" || !pinTargetUserId)
            return
        pinMessage = qsTr("Checking PIN…")
        if (pinMode === "unlock") {
            switchAccount(pinTargetUserId, pin)
            return
        }
        const method = pinMode === "clear" ? "auth.pin.clear" : "auth.pin.set"
        const params = pinMode === "clear"
            ? { userId: pinTargetUserId, currentPin: pin }
            : { userId: pinTargetUserId, pin: pin }
        pinRequestId = CoreClient.request(method, params, 30000)
    }

    function openGame(game) {
        return catalogOwner.openGame(game)
    }

    function selectGameVariant(index) {
        return catalogOwner.selectGameVariant(index)
    }

    function gameIdentity(game) {
        return catalogOwner.gameIdentity(game)
    }

    function isFavorite(game) {
        return catalogOwner.isFavorite(game)
    }

    function toggleFavorite(game) {
        return catalogOwner.toggleFavorite(game)
    }

    function isHidden(game) {
        return catalogOwner.isHidden(game)
    }

    function hiddenGameCount() {
        return catalogOwner.hiddenGameCount()
    }

    function toggleHidden(game) {
        return catalogOwner.toggleHidden(game)
    }

    function addToHome(game) {
        return catalogOwner.addToHome(game)
    }

    function removeFromHome(game) {
        return catalogOwner.removeFromHome(game)
    }

    function setHomeOrder(ids) {
        return catalogOwner.setHomeOrder(ids)
    }

    function homeTileSize(game) {
        return catalogOwner.homeTileSize(game)
    }

    function setHomeTileSize(game, size) {
        return catalogOwner.setHomeTileSize(game, size)
    }

    function acceptDirectLaunch(appId, title) {
        pendingDirectLaunch = {
            appId: String(appId || ""),
            title: String(title || "").trim()
        }
        resolveDirectLaunch()
    }

    function gameMatchesDirectLaunch(game, request) {
        const requestedId = String(request.appId || "")
        if (requestedId !== "") {
            if (String(game.launchAppId || "") === requestedId
                    || String(game.appId || "") === requestedId
                    || String(game.id || "") === requestedId)
                return true
            const variants = game.variants || []
            for (let index = 0; index < variants.length; ++index) {
                if (String(variants[index].id || "") === requestedId)
                    return true
            }
        }
        return request.title !== ""
            && String(game.title || "").toLocaleLowerCase() === request.title.toLocaleLowerCase()
    }

    function resolveDirectLaunch() {
        if (!pendingDirectLaunch)
            return
        if (Object.keys(settings).length === 0 || onboardingRequired
                || onboardingSaving || onboardingReplaying || onboardingError !== "")
            return
        if (catalogState !== "ready") {
            if (ready && catalogRequestId === "")
                refreshCatalog(pendingDirectLaunch.title)
            return
        }
        let match = null
        for (let index = 0; index < catalogGames.length; ++index) {
            if (gameMatchesDirectLaunch(catalogGames[index], pendingDirectLaunch)) {
                match = catalogGames[index]
                break
            }
        }
        if (!match) {
            lastError = qsTr("The requested GeForce NOW game was not found in the current catalog.")
            pendingDirectLaunch = null
            AppController.navigate("library")
            return
        }
        selectedGame = match
        if (!signedIn) {
            AppController.navigate("sign-in")
            return
        }
        pendingDirectLaunch = null
        launchSelectedGame(true)
    }

    function selectedLaunchAppId() {
        if (!selectedGame)
            return ""
        const variants = selectedGame.variants || []
        const index = Math.max(0, Number(selectedGame.selectedVariantIndex || 0))
        const variantId = variants.length > index ? String(variants[index].id || "") : ""
        if (/^\d+$/.test(variantId))
            return variantId
        const launchId = String(selectedGame.launchAppId || "")
        return /^\d+$/.test(launchId) ? launchId : ""
    }

    function selectedGameMembershipError() {
        const requiredTier = String(selectedGame.membershipTierLabel || "").trim()
        if (requiredTier === "" || /^free(?: tier|-tier)?$/i.test(requiredTier))
            return ""
        const accountTier = String((subscription && subscription.membershipTier) || "").trim()
        if (accountTier === "") {
            if (subscriptionRequestId === "")
                subscriptionRequestId = CoreClient.request("account.subscription.get", {}, 30000)
            return qsTr("Membership details unavailable. Please try again.")
        }
        if (/^free(?: tier|-tier)?$/i.test(accountTier))
            return qsTr("This game requires a paid GeForce NOW membership.")
        return ""
    }

    function launchSelectedGame(directConsoleMode) {
        if (!signedIn) {
            AppController.navigate("sign-in")
            return
        }
        const appId = selectedLaunchAppId()
        if (!appId) {
            streamState = "error"
            streamMessage = qsTr("This game does not expose a numeric GeForce NOW launch ID.")
            lastError = streamMessage
            return
        }
        if (!ready || streamBusy || onboardingReplaying)
            return
        const membershipError = selectedGameMembershipError()
        if (membershipError !== "") {
            streamState = "error"
            streamMessage = membershipError
            lastError = membershipError
            AppController.navigate("inserting")
            return
        }
        streamerRestartAttempts = 0
        streamerRecoveryExhausted = false
        sessionReconnectAttempts = 0
        streamerRestartRecoveryCount = 0
        sessionRecoveryCount = 0
        guidePagesVisited = []
        antiAfkEnabled = false
        const variants = selectedGame.variants || []
        const selectedVariantIndex = Math.max(0, Number(selectedGame.selectedVariantIndex || 0))
        const selectedVariant = variants.length > selectedVariantIndex ? variants[selectedVariantIndex] : null
        const params = {
            appId: appId,
            title: selectedGame.title || "GeForce NOW game",
            supportsInGameSettingsPersistence: Boolean(selectedVariant && selectedVariant.supportsInGameSettingsPersistence),
            accountLinked: Boolean(selectedVariant && selectedVariant.inLibrary),
            appLaunchMode: settings.steamBigPictureMode === true
                ? "gamepadFriendly" : "default"
        }
        const configuredRegion = String(settings.region || "")
        for (let index = 0; index < regions.length; ++index) {
            const region = regions[index]
            if (configuredRegion === region.name || configuredRegion === region.url) {
                params.zone = region.name
                params.streamingBaseUrl = region.url
                break
            }
        }
        pendingLaunchParams = params
        launchConflictDetected = false
        conflictSession = null
        streamState = "checking"
        streamMessage = qsTr("Looking for your game on GeForce NOW…")
        lastError = qsTr("")
        remoteSessionsRequestId = CoreClient.request("session.remote.list", params, 30000)
        AppController.navigate("inserting")
    }

    function checkLaunchSessions() {
        if (!ready || !pendingLaunchParams || streamBusy)
            return
        streamState = "checking"
        streamMessage = qsTr("Looking for your game on GeForce NOW…")
        lastError = qsTr("")
        remoteSessionsRequestId = CoreClient.request("session.remote.list", pendingLaunchParams, 30000)
    }

    function retrySessionLaunch() {
        if (streamBusy)
            return
        if (activeSession) {
            retryNativeStreamer()
            return
        }
        if (conflictSession) {
            resolveSessionConflict("resume")
            return
        }
        checkLaunchSessions()
    }

    function handleSessionCreateFailure(code, message) {
        streamPollTimer.stop()
        if (code === "session_conflict") {
            launchConflictDetected = true
            checkLaunchSessions()
            return
        }
        streamState = "error"
        streamMessage = message
    }

    function createPendingSession() {
        if (!pendingLaunchParams || !ready || streamCreateRequestId !== "" || onboardingReplaying)
            return
        streamState = "requesting"
        streamMessage = qsTr("Requesting a cloud gaming seat…")
        if (!nativeRuntimeReady) {
            ensureNativeRuntimeReady()
            streamState = "error"
            streamMessage = qsTr("The native decoder is still being detected. Please try again in a moment.")
            lastError = streamMessage
            return
        }
        streamCreateRequestId = CoreClient.request("session.create",
            Object.assign({}, pendingLaunchParams, {runtimeCapabilities: nativeRuntimeCapabilities}), 35000)
    }

    function resolveSessionConflict(choice) {
        if (streamBusy)
            return
        AppController.showOverlay("")
        if (choice === "cancel") {
            pendingLaunchParams = null
            conflictSession = null
            launchConflictDetected = false
            streamState = "idle"
            streamMessage = qsTr("")
            AppController.navigateFromLastPrimary("game-detail")
            return
        }
        if (choice === "resume") {
            if (!conflictSession)
                return
            streamState = "resuming"
            streamMessage = qsTr("Reconnecting to your running game. You don't need to start again.")
            sessionClaimIsRecovery = false
            sessionClaimRequestId = CoreClient.request("session.claim", {
                sessionId: conflictSession.sessionId,
                streamingBaseUrl: conflictSession.streamingBaseUrl,
                appId: String(conflictSession.appId || "0"),
                recoveryMode: true
            }, 35000)
            return
        }
        if (choice === "new") {
            if (!conflictSession) {
                createPendingSession()
                return
            }
            forceNewAfterStop = true
            streamState = "stopping"
            streamMessage = qsTr("Closing the previous cloud session…")
            streamStopRequestId = CoreClient.request("session.stop", {
                sessionId: conflictSession.sessionId,
                streamingBaseUrl: conflictSession.streamingBaseUrl,
                serverIp: conflictSession.serverIp || ""
            }, 35000)
        }
    }

    function inspectRemoteSessions(result) {
        remoteSessions = result.sessions || []
        if (!pendingLaunchParams)
            return
        if (remoteSessions.length === 0) {
            if (launchConflictDetected) {
                streamState = "error"
                streamMessage = qsTr("GeForce NOW says a game is still running, but it isn't available to reconnect yet. Wait a moment, then try again. If you were playing on another device, disconnect there first.")
                return
            }
            createPendingSession()
            return
        }
        const wantedAppId = pendingLaunchParams ? Number(pendingLaunchParams.appId || 0) : 0
        conflictSession = remoteSessions[0]
        for (let index = 0; index < remoteSessions.length; ++index) {
            if (wantedAppId > 0 && Number(remoteSessions[index].appId || 0) === wantedAppId) {
                conflictSession = remoteSessions[index]
                resolveSessionConflict("resume")
                return
            }
        }
        streamState = "conflict"
        streamMessage = qsTr("Another game is still running on GeForce NOW. You can return to it, or end it to play this game.")
        AppController.showOverlay("session-conflict")
    }

    function normalizedStreamingSession(session) {
        if (!session)
            return null
        const normalized = Object.assign({}, session)
        if (session.negotiatedStreamProfile) {
            const profile = Object.assign({}, session.negotiatedStreamProfile)
            const resolution = String(profile.resolution || "").split("x")
            if (resolution.length === 2) {
                const width = Number(resolution[0])
                const height = Number(resolution[1])
                if (Number.isFinite(width) && width > 0)
                    profile.width = width
                if (Number.isFinite(height) && height > 0)
                    profile.height = height
            }
            normalized.negotiatedStreamProfile = profile
        }
        return normalized
    }

    function acceptStreamingSession(session) {
        const previousSession = activeSession
        activeSession = normalizedStreamingSession(session)
        if (!activeSession || !previousSession || previousSession.sessionId !== activeSession.sessionId) {
            streamerRestartTimer.stop()
            streamerRestartAttempts = 0
            sessionReconnectAttempts = 0
            streamerRecoveryExhausted = false
        }
        if (!activeSession) {
            cancelSessionRecovery()
            runtimeStreamProfile = ({})
            if (previousSession && streamer && streamer.status !== "stopped")
                stopNativeStreamer("The remote session ended")
            streamRecordingActive = false
            streamRecordingElapsedMs = 0
            pendingRecordingPath = ""
            pendingRecordingThumbnailPath = ""
            resetStreamReplay()
            streamState = "idle"
            streamMessage = qsTr("")
            streamPollTimer.stop()
            streamStartedAtMs = 0
            syncDiscordPresence()
            return
        }
        // A seat still being ready does not mean its media path recovered.
        // Polls and claim replies must not restart an exhausted media episode.
        if (streamerRecoveryExhausted)
            return
        streamState = activeSession.phase || (Number(activeSession.status) >= 2 ? "ready" : "preparing")
        const adState = activeSession.adState || ({})
        const ads = adState.sessionAds || adState.ads || []
        if ((adState.sessionAdsRequired || adState.isAdsRequired) && ads.length > 0
                && AppController.overlay !== "queue-ad")
            AppController.showOverlay("queue-ad")
        if (streamState === "ready" || streamState === "streaming") {
            if (streamStartedAtMs === 0)
                streamStartedAtMs = Date.now()
            streamMessage = qsTr("Your GeForce NOW seat is ready.")
            streamPollTimer.stop()
            if (AppController.route !== "stream")
                AppController.navigate("stream")
            startNativeStreamer()
        } else if (streamState === "failed") {
            streamMessage = qsTr("GeForce NOW could not prepare this session.")
            streamPollTimer.stop()
        } else {
            const position = Number(activeSession.queuePosition || 0)
            streamMessage = activeSession.resumePending
                ? qsTr("Reconnecting to your running game. You don't need to start again.")
                : position > 0
                ? qsTr("Queue position %1").arg(position)
                : qsTr("Preparing your cloud gaming seat…")
            streamPollTimer.restart()
        }
        syncDiscordPresence()
    }

    function syncDiscordPresence() {
        if (!ready || discordRequestId !== "")
            return
        if (!Boolean(settings.discordRichPresence) || !activeSession) {
            discordRequestId = CoreClient.request("discord.activity.clear", {}, 5000)
            return
        }
        const status = Number(activeSession.status || 0)
        const kind = status >= 3 ? "streaming"
            : Number(activeSession.queuePosition || 0) > 0 ? "queued" : "starting"
        discordRequestId = CoreClient.request("discord.activity.sync", {
            enabled: true,
            gameName: selectedGame ? selectedGame.title : String(activeSession.appId || "GeForce NOW"),
            gameImageUrl: selectedGame ? (selectedGame.boxArtUrl || selectedGame.imageUrl || "") : "",
            kind: kind,
            queuePosition: Number(activeSession.queuePosition || 0),
            startTimestampMs: streamStartedAtMs
        }, 5000)
    }

    function reportSessionAd(action, ad, watchedTimeMs, cancelReason) {
        if (!ready || !activeSession || !ad || sessionAdRequestId !== "")
            return
        sessionAdRequestId = CoreClient.request("session.ad.report", {
            sessionId: activeSession.sessionId,
            adId: ad.adId,
            action: action,
            watchedTimeInMs: Math.max(0, Math.round(watchedTimeMs || 0)),
            pausedTimeInMs: 0,
            cancelReason: cancelReason || undefined
        }, 30000)
    }

    function startNativeStreamer() {
        if (!ready || !activeSession || sessionRecoveryPending || activeSession.resumePending
                || streamerStopRequestId !== "" || streamerStartRequestId !== ""
                || streamerPrepareRequestId !== "" || sessionClaimRequestId !== ""
                || streamerRestartTimer.running || streamerRecoveryExhausted)
            return
        if (streamer && streamer.sessionId === activeSession.sessionId
                && streamer.status !== "stopped" && streamer.status !== "error"
                && streamer.status !== "starting")
            return
        streamer = {
            status: "starting",
            message: qsTr("Preparing the embedded native media runtime…"),
            sessionId: activeSession.sessionId,
            sessionStartedAtMs: String(Date.now()),
            inputReady: false,
            inputPauseCount: 0,
            inputResumeCount: 0,
            recordingStartCount: 0,
            recordingStopCount: 0,
            queueDropCount: 0
        }
        microphoneRequestId = ""
        sessionMicrophoneMode = "disabled"
        streamInputStateKnown = false
        streamerStopExpected = false
        if (!nativeRuntimeReady) {
            ensureNativeRuntimeReady()
            return
        }
        streamerPrepareRequestId = CoreClient.request("streamer.prepare", {
            session: activeSession,
            runtimeCapabilities: nativeRuntimeCapabilities
        }, 15000)
        if (streamerPrepareRequestId === "")
            acceptStreamerSnapshot(Object.assign({}, streamer, {
                status: "error",
                message: qsTr("The core could not prepare the native stream context"),
                errorCode: "core_not_ready"
            }))
    }

    function retryNativeStreamer() {
        streamerRestartTimer.stop()
        streamerRestartAttempts = 0
        sessionReconnectAttempts = 0
        streamerRecoveryExhausted = false
        recoverStreamingSession(streamMessage)
    }

    function acceptStreamerSnapshot(snapshot) {
        // One failure produces both error and stopped, plus late input replies.
        // Count it once and preserve the original, actionable error message.
        const wasTerminal = streamer && (streamer.status === "error" || streamer.status === "stopped")
        const isTerminal = snapshot && (snapshot.status === "error" || snapshot.status === "stopped")
        if (wasTerminal && isTerminal)
            return
        streamer = snapshot ? Object.assign({}, snapshot, {
            codec: snapshot.codec || runtimeStreamProfile.codec || "",
            outputWidth: snapshot.outputWidth || runtimeStreamProfile.width || 0,
            outputHeight: snapshot.outputHeight || runtimeStreamProfile.height || 0,
            outputFps: snapshot.outputFps || runtimeStreamProfile.fps || 0
        }) : null
        if (!streamer) {
            return
        }
        inspectStreamerOverlayRequest(streamer)
        inspectStreamerScreenshotRequest(streamer)
        inspectStreamerRecordingRequest(streamer)
        inspectStreamerShortcutAction(streamer)

        const startedAt = Number(streamer.sessionStartedAtMs || 0)
        if (Number.isFinite(startedAt) && startedAt > 0)
            streamStartedAtMs = startedAt

        const status = String(streamer.status || "unknown")
        if (activeSession && !sessionRecoveryPending && status !== "stopped" && status !== "error") {
            streamState = status
            streamMessage = streamer.message || streamMessage
            setStreamInputPaused(desiredStreamInputPaused)
        }
        if (status === "streaming") {
            return
        }
        if (status !== "error" && status !== "stopped")
            return

        streamInputStateKnown = false
        streamReplayEnabled = false
        mediaClipTargetRequestId = ""
        streamClipRequestId = ""
        if (streamRecordingActive) {
            streamRecordingActive = false
            streamRecordingElapsedMs = 0
            pendingRecordingPath = ""
            pendingRecordingThumbnailPath = ""
            refreshMedia()
        }
        if (status === "stopped" && (streamerStopExpected || !activeSession)) {
            streamInputStateKnown = false
            return
        }
        streamMessage = streamer.message || (status === "error"
            ? qsTr("Native media startup failed") : qsTr("The native media runtime stopped unexpectedly"))
        if (!activeSession)
            return
        if (sessionRecoveryPending || streamerRestartTimer.running)
            return
        scheduleSessionRecovery(streamMessage)
    }

    function recoverStreamingSession(reason) {
        if (!activeSession || sessionClaimRequestId !== "" || recoveryDiscoveryRequestId !== ""
                || streamerStopRequestId !== "")
            return
        if (!ready) {
            streamerRestartTimer.restart()
            return
        }
        if (sessionReconnectAttempts >= maximumSessionReconnectAttempts) {
            streamerRecoveryExhausted = true
            streamerRestartTimer.stop()
            streamState = "error"
            streamMessage = reason || qsTr("The streaming session could not be recovered")
            lastError = streamMessage
            return
        }
        sessionReconnectAttempts += 1
        streamerRestartTimer.stop()
        sessionRecoveryPending = true
        recoverySessionId = String(activeSession.sessionId || "")
        streamState = "reconnecting"
        streamMessage = qsTr("Reconnecting to the GeForce NOW session…")
        streamPollTimer.stop()
        for (const id of [streamerPrepareRequestId, streamPollRequestId])
            if (id !== "") CoreClient.cancel(id)
        streamerPrepareRequestId = ""
        streamPollRequestId = ""
        // Stop/reap the previous native connection before claiming fresh keys.
        // This is NOT session.stop: the cloud game must remain alive.
        if (NativeStreamRuntime.running && streamer && streamer.status !== "stopped") {
            stopNativeStreamer("Reconnecting the native media transport")
            if (streamerStopRequestId === "")
                scheduleSessionRecovery(lastError || qsTr("Could not stop the previous native connection"))
            return
        }
        discoverRecoverySession()
    }

    function scheduleSessionRecovery(reason) {
        sessionRecoveryPending = false
        streamMessage = reason || qsTr("Connection lost. Waiting to reconnect…")
        if (!activeSession || streamStopRequestId !== "") return
        if (sessionReconnectAttempts >= maximumSessionReconnectAttempts) {
            streamerRecoveryExhausted = true
            streamerRestartTimer.stop()
            streamState = "error"
            lastError = streamMessage
            return
        }
        streamState = "reconnecting"
        streamerRestartTimer.restart()
    }

    function discoverRecoverySession() {
        if (!sessionRecoveryPending || !activeSession
                || String(activeSession.sessionId) !== recoverySessionId) return
        recoveryDiscoveryRequestId = CoreClient.request("session.remote.list", {
            sessionId: recoverySessionId,
            streamingBaseUrl: activeSession.streamingBaseUrl
        }, 30000)
        if (recoveryDiscoveryRequestId === "") scheduleSessionRecovery(qsTr("Waiting for the connection…"))
    }

    function acceptRecoverySessions(result) {
        if (!sessionRecoveryPending || !activeSession
                || String(activeSession.sessionId) !== recoverySessionId) return
        const existing = (result.sessions || []).find(session => String(session.sessionId) === recoverySessionId)
        if (!existing) {
            scheduleSessionRecovery(qsTr("The previous session is not available yet. Retrying…"))
            return
        }
        sessionClaimIsRecovery = true
        sessionClaimRequestId = CoreClient.request("session.claim", {
            sessionId: recoverySessionId,
            streamingBaseUrl: existing.streamingBaseUrl || activeSession.streamingBaseUrl,
            appId: String(activeSession.appId || "0"),
            recoveryMode: true
        }, 35000)
        if (sessionClaimRequestId === "") scheduleSessionRecovery(qsTr("Waiting to resume the session…"))
    }

    function cancelSessionRecovery() {
        streamerRestartTimer.stop()
        const requestIds = [recoveryDiscoveryRequestId, sessionClaimRequestId]
        recoveryDiscoveryRequestId = ""
        sessionClaimRequestId = ""
        sessionRecoveryPending = false
        sessionClaimIsRecovery = false
        recoverySessionId = ""
        resumePollAttempts = 0
        resumePollDeadlineMs = 0
        for (const id of requestIds)
            if (id !== "") CoreClient.cancel(id)
    }

    function artworkUrl(sourceUrl) {
        return artworkOwner.artworkUrl(sourceUrl)
    }

    function retainArtwork(source) {
        return artworkOwner.retainArtwork(source)
    }

    function releaseArtwork(source) {
        return artworkOwner.releaseArtwork(source)
    }

    function scheduleArtworkRetry(source) {
        return artworkOwner.scheduleArtworkRetry(source)
    }

    function retryVisibleArtwork(now) {
        return artworkOwner.retryVisibleArtwork(now)
    }

    function requestArtwork(sourceUrl, retryDue) {
        return artworkOwner.requestArtwork(sourceUrl, retryDue)
    }

    function finishArtworkRequest(requestId, result, failed) {
        return artworkOwner.finishArtworkRequest(requestId, result, failed)
    }

    function acceptArtworkResult(payload) {
        return artworkOwner.acceptArtworkResult(payload)
    }

    function stopNativeStreamer(reason) {
        streamerRestartTimer.stop()
        if (streamerStopRequestId !== "")
            return
        // A runtime that never started, or that already reported "stopped", has
        // nothing left to reap. Without this a repeated stop request during a
        // failing session could issue multiple native stop commands.
        const status = streamer ? String(streamer.status || "") : ""
        if (status === "" || status === "stopped")
            return
        streamerStopExpected = true
        streamerStopRequestId = sendNativeCommand("stop", {
            reason: reason || "User stopped the session"
        }, "stop")
        if (streamerStopRequestId === "")
            streamerStopExpected = false
    }

    function setStreamInputPaused(paused) {
        desiredStreamInputPaused = Boolean(paused)
        if (streamInputPauseRequestId !== "" || !streamer
                || streamer.status === "stopped" || streamer.status === "error")
            return
        if (streamInputStateKnown && currentStreamInputPaused === desiredStreamInputPaused)
            return
        streamInputPauseRequestId = sendNativeCommand("input-paused", {
            paused: desiredStreamInputPaused
        }, "input")
    }

    function controlStream(action) {
        if (streamControlRequestId !== "")
            return
        if (!activeSession || !streamer || streamer.status !== "streaming") {
            streamControlMessage = qsTr("Stream controls are available once the session is live.")
            return
        }
        streamControlMessage = qsTr("")
        streamControlAction = action
        const commandTypes = {
            "toggle-fullscreen": "fullscreen-toggle",
            "anti-afk-pulse": "anti-afk-pulse"
        }
        const commandType = commandTypes[action]
        if (!commandType) {
            streamControlAction = ""
            streamControlMessage = qsTr("This stream control is unavailable")
            return
        }
        streamControlRequestId = sendNativeCommand(commandType, {}, "control")
        if (streamControlRequestId === "") {
            streamControlAction = ""
            streamControlMessage = lastError
        }
    }

    function setMicrophoneEnabled(enabled) {
        if (!microphoneCanToggle)
            return
        if (!enabled)
            microphoneRecoveryEnabled = false
        microphoneRequestId = sendNativeCommand("microphone-set", {
            enabled: Boolean(enabled)
        }, "microphone")
        if (microphoneRequestId === "") {
            streamControlMessage = lastError
            accessibilityMessage = lastError
        }
    }

    function toggleMicrophone() {
        setMicrophoneEnabled(!microphoneEnabled)
    }

    function prepareMicrophoneStart(sessionId, mode) {
        sessionMicrophoneMode = String(mode || "disabled")
        if (microphoneRecoverySessionId !== String(sessionId)) {
            microphoneRecoverySessionId = String(sessionId)
            microphoneRecoveryEnabled = sessionMicrophoneMode === "voice-activity"
        }
        if (sessionMicrophoneMode !== "voice-activity")
            microphoneRecoveryEnabled = false
        return microphoneRecoveryEnabled
    }

    function inspectStreamerOverlayRequest(value) {
        const generation = Number(value && value.overlayRequestGeneration || 0)
        if (generation <= overlayRequestGeneration)
            return
        overlayRequestGeneration = generation
        AppController.showOverlay(desktopUiActive ? "desktop-stream-menu" : "guide-session")
    }

    function inspectStreamerScreenshotRequest(value) {
        const generation = Number(value && value.screenshotRequestGeneration || 0)
        if (generation <= screenshotRequestGeneration)
            return
        screenshotRequestGeneration = generation
        captureStreamScreenshot()
    }

    function captureStreamScreenshot() {
        const rect = streamCaptureRect
        const title = selectedGame && selectedGame.title ? selectedGame.title : "OpenNOW"
        const path = AppController.captureScreenRegion(
            Number(rect.x || 0), Number(rect.y || 0),
            Number(rect.width || 0), Number(rect.height || 0), title)
        if (path) {
            mediaMessage = qsTr("Screenshot saved")
            accessibilityMessage = qsTr("Screenshot saved to %1").arg(path)
            refreshMedia()
        } else {
            mediaMessage = qsTr("Screenshot capture failed")
            lastError = qsTr("The desktop compositor did not allow OpenNOW to capture the stream.")
            accessibilityMessage = lastError
        }
    }

    function inspectStreamerRecordingRequest(value) {
        const generation = Number(value && value.recordingToggleRequestGeneration || 0)
        if (generation <= recordingToggleRequestGeneration)
            return
        recordingToggleRequestGeneration = generation
        toggleStreamRecording()
    }

    function streamShortcutBindings() {
        return {
            "guide": ["Ctrl+G"],
            "toggle-pointer-lock": [String(settings.shortcutTogglePointerLock || "F8")],
            "toggle-fullscreen": [String(settings.shortcutToggleFullscreen || "F11")],
            "stop-stream": [String(settings.shortcutStopStream || "Ctrl+Shift+Q")],
            "toggle-anti-afk": [String(settings.shortcutToggleAntiAfk || "Ctrl+Shift+K")],
            "toggle-microphone": microphoneToggleAvailable
                ? [String(settings.shortcutToggleMicrophone || "Ctrl+Shift+M")] : [],
            "screenshot": [String(settings.shortcutScreenshot || "Ctrl+F11")],
            "toggle-recording": [String(settings.shortcutToggleRecording || "F12")],
            "save-clip": [String(settings.shortcutSaveClip || "Ctrl+F12")]
        }
    }

    function isStreamStatsOverlay(overlay) {
        return ["desktop-stream-stats", "desktop-stream-stats-expanded",
                "stream-stats", "stream-stats-expanded"]
            .indexOf(String(overlay || "")) >= 0
    }

    // Statistics are a heads-up display, not a shell modal. Gameplay input must
    // stay owned by StreamVideoItem while the panel is visible.
    function streamOverlayBlocksGameplayInput(overlay) {
        const name = String(overlay || "")
        return name !== "" && !isStreamStatsOverlay(name)
    }

    function requestStreamExitConfirmation() {
        if (AppController.route !== "stream" && AppController.route !== "inserting")
            return
        if (AppController.route === "inserting" && (streamState === "error" || streamState === "checking")
                && !activeSession && streamCreateRequestId === "" && sessionClaimRequestId === ""
                && streamStopRequestId === "") {
            stopStreamingSession()
            return
        }
        AppController.showOverlay("desktop-stream-exit-confirm")
        accessibilityMessage = qsTr("Confirm ending the cloud session")
    }

    function confirmStreamExit() {
        stopStreamingSession()
        AppController.showOverlay("")
    }

    function applyStreamShortcutAction(action) {
        action = String(action || "")
        if (action === "guide") {
            AppController.showOverlay(desktopUiActive ? "desktop-stream-menu" : "guide-session")
        } else if (action === "request-exit" || action === "stop-stream") {
            requestStreamExitConfirmation()
        } else if (action === "toggle-anti-afk") {
            antiAfkEnabled = !antiAfkEnabled
            streamControlMessage = antiAfkEnabled ? qsTr("Anti-AFK on") : qsTr("Anti-AFK off")
            accessibilityMessage = streamControlMessage
        } else if (action === "toggle-stats") {
            const compact = desktopUiActive ? "desktop-stream-stats" : "stream-stats"
            const expanded = desktopUiActive
                ? "desktop-stream-stats-expanded" : "stream-stats-expanded"
            if (AppController.overlay === compact)
                AppController.showOverlay(expanded)
            else if (AppController.overlay === expanded)
                AppController.showOverlay("")
            else
                AppController.showOverlay(compact)
        } else if (action === "toggle-fullscreen") {
            fullscreenToggleRequested()
        } else if (action === "toggle-pointer-lock") {
            pointerLockToggleRequested()
        } else if (action === "screenshot") {
            captureStreamScreenshot()
        } else if (action === "toggle-recording") {
            toggleStreamRecording()
        } else if (action === "save-clip") {
            saveStreamClip()
        } else if (action === "toggle-microphone") {
            toggleMicrophone()
        }
    }

    function inspectStreamerShortcutAction(value) {
        const generation = Number(value && value.shortcutActionGeneration || 0)
        if (generation <= shortcutActionGeneration)
            return
        shortcutActionGeneration = generation
        applyStreamShortcutAction(value.shortcutAction)
    }

    function resetStreamReplay() {
        streamReplayEnabled = false
        mediaClipTargetRequestId = ""
        streamClipRequestId = ""
    }

    function disableStreamReplay() {
        if (!streamReplayEnabled)
            return
        if (NativeStreamRuntime.running) {
            const requestId = sendNativeCommand("replay-stop", {}, "replay-stop")
            if (requestId === "") {
                mediaMessage = lastError
                accessibilityMessage = mediaMessage
                streamCaptureAnnounced(mediaMessage)
                return
            }
        }
        streamReplayEnabled = false
        mediaClipTargetRequestId = ""
        streamClipRequestId = ""
    }

    function saveStreamClip() {
        if (streamClipBusy)
            return
        if (!activeSession || !streamer || streamer.status !== "streaming") {
            mediaMessage = qsTr("Start a native stream before saving a clip")
        } else if (!streamReplayEnabled || !replayBufferRequested) {
            mediaMessage = qsTr("Enable replay buffering in Recording settings before starting a session")
        } else {
            const title = selectedGame && selectedGame.title ? selectedGame.title : "OpenNOW"
            mediaClipTargetRequestId = CoreClient.request("media.recording.target", {
                gameTitle: title + "-clip"
            }, 5000)
            mediaMessage = qsTr("Preparing clip…")
        }
        accessibilityMessage = mediaMessage
        streamCaptureAnnounced(mediaMessage)
    }

    function toggleStreamRecording() {
        if (streamRecordingStartRequestId !== "" || streamRecordingStopRequestId !== ""
                || mediaRecordingTargetRequestId !== "")
            return
        if (streamRecordingActive) {
            streamRecordingStopRequestId = sendNativeCommand("recording-stop", {}, "recording-stop")
            mediaMessage = streamRecordingStopRequestId === ""
                ? lastError : qsTr("Finalizing recording…")
            accessibilityMessage = mediaMessage
            return
        }
        if (!activeSession || !streamer || streamer.status !== "streaming") {
            mediaMessage = qsTr("Start a native stream before recording")
            accessibilityMessage = mediaMessage
            return
        }
        const title = selectedGame && selectedGame.title ? selectedGame.title : "OpenNOW"
        AppController.showOverlay("")
        mediaRecordingTargetRequestId = CoreClient.request("media.recording.target", {
            gameTitle: title
        }, 5000)
        mediaMessage = qsTr("Preparing source-quality recording…")
        accessibilityMessage = mediaMessage
    }

    function pollStreamingSession() {
        if (!ready || !activeSession || streamPollRequestId !== "" || streamStopRequestId !== "")
            return
        if (activeSession.resumePending && (++resumePollAttempts > 60
                || (resumePollDeadlineMs > 0 && Date.now() >= resumePollDeadlineMs))) {
            streamPollTimer.stop()
            scheduleSessionRecovery(qsTr("The resumed session did not become ready. Retrying…"))
            return
        }
        streamPollRequestId = CoreClient.request("session.poll", {
            sessionId: activeSession.sessionId,
            streamingBaseUrl: activeSession.streamingBaseUrl
        }, 35000)
    }

    function stopStreamingSession() {
        const discoveryRequestId = remoteSessionsRequestId
        const createRequestId = streamCreateRequestId
        remoteSessionsRequestId = ""
        streamCreateRequestId = ""
        pendingLaunchParams = null
        conflictSession = null
        launchConflictDetected = false
        forceNewAfterStop = false
        if (discoveryRequestId !== "")
            CoreClient.cancel(discoveryRequestId)
        if (createRequestId !== "")
            CoreClient.cancel(createRequestId)
        cancelSessionRecovery()
        if (activeSession && streamStartedAtMs > 0) {
            const snapshot = streamer || ({})
            sessionReportDropId = dropSessionId
            lastSessionReport = {
                gameTitle: selectedGame && selectedGame.title ? selectedGame.title : "GeForce NOW",
                durationMs: Math.max(0, Date.now() - streamStartedAtMs),
                transport: snapshot.transport ? String(snapshot.transport).toUpperCase() : "",
                mediaBackend: snapshot.mediaBackend ? String(snapshot.mediaBackend) : "",
                firstFrameLatencyMs: snapshot.firstFrameLatencyMs !== undefined
                    && snapshot.firstFrameLatencyMs !== null
                    ? Number(snapshot.firstFrameLatencyMs) : null,
                recoveries: Number(streamerRecoveryCount || 0) + Number(sessionRecoveryCount || 0),
                decoderErrors: snapshot.decoderErrorCount !== undefined
                    && snapshot.decoderErrorCount !== null
                    ? Number(snapshot.decoderErrorCount) : null,
                outputErrors: snapshot.outputErrorCount !== undefined
                    && snapshot.outputErrorCount !== null
                    ? Number(snapshot.outputErrorCount) : null,
                queueDrops: snapshot.queueDropCount !== undefined
                    && snapshot.queueDropCount !== null
                    ? Number(snapshot.queueDropCount) : null,
                drops: streamDropCounts,
                recordingCount: snapshot.recordingStopCount !== undefined
                    && snapshot.recordingStopCount !== null
                    ? Number(snapshot.recordingStopCount) : null
            }
        }
        streamPollTimer.stop()
        stopNativeStreamer("User stopped the session")
        if (streamPollRequestId !== "") {
            CoreClient.cancel(streamPollRequestId)
            streamPollRequestId = ""
        }
        if (!activeSession) {
            streamState = "idle"
            AppController.navigateFromLastPrimary("game-detail")
            return
        }
        if (!ready || streamStopRequestId !== "")
            return
        streamState = "stopping"
        streamMessage = qsTr("Closing the remote session…")
        streamStopRequestId = CoreClient.request("session.stop", {
            sessionId: activeSession.sessionId,
            streamingBaseUrl: activeSession.streamingBaseUrl
        }, 35000)
    }

    function startDeviceLogin(providerIdpId, staySignedIn) {
        if (!ready || deviceStartRequestId !== "")
            return
        pendingStaySignedIn = staySignedIn !== false
        cancelDeviceLogin()
        lastError = qsTr("")
        authMessage = qsTr("Contacting NVIDIA…")
        authState = "starting"
        const params = providerIdpId ? { providerIdpId: providerIdpId } : {}
        deviceStartRequestId = CoreClient.request("auth.device.start", params, 30000)
    }

    function pollDeviceLogin() {
        if (!ready || !authChallenge || devicePollRequestId !== "")
            return
        devicePollRequestId = CoreClient.request("auth.device.poll", {
            attemptId: authChallenge.attemptId,
            deviceCode: authChallenge.deviceCode
        }, 30000)
    }

    function cancelDeviceLogin() {
        devicePollTimer.stop()
        if (devicePollRequestId !== "") {
            CoreClient.cancel(devicePollRequestId)
            devicePollRequestId = ""
        }
        if (authChallenge && ready)
            CoreClient.request("auth.device.cancel", { attemptId: authChallenge.attemptId })
        authChallenge = null
        if (!signedIn)
            authState = "idle"
    }

    function logout() {
        if (ready && logoutRequestId === "")
            logoutRequestId = CoreClient.request("auth.logout", {})
    }

    function logoutAll() {
        if (ready && logoutAllRequestId === "")
            logoutAllRequestId = CoreClient.request("auth.accounts.logoutAll", {})
    }

    function requestConsoleSurface(enabled) {
        return settingsOwner.requestConsoleSurface(enabled)
    }

    function beginConsoleSurfacePersistence() {
        return settingsOwner.beginConsoleSurfacePersistence()
    }

    function setSetting(key, value) {
        return settingsOwner.setSetting(key, value)
    }

    function resetSettings() {
        return settingsOwner.resetSettings()
    }

    function applyCoupledSettings(changes) {
        return settingsOwner.applyCoupledSettings(changes)
    }

    function applySetting(key, value) {
        return settingsOwner.applySetting(key, value)
    }

    function focusIndex(route) {
        return focusPositions[route] === undefined ? 0 : focusPositions[route]
    }

    function rememberFocus(route, index) {
        const updated = Object.assign({}, focusPositions)
        updated[route] = index
        focusPositions = updated
    }

    function updateStreamerFields(fields) {
        acceptStreamerSnapshot(Object.assign({}, streamer || ({}), fields || ({})))
    }

    function acceptNativeResponse(response) {
        const requestId = String(response && response.id || "")
        const pending = takeNativeRequest(requestId)
        if (!pending)
            return
        if (pending.operation === "clip-save" && streamClipRequestId !== requestId)
            return
        const responseType = String(response.type || "")
        if (pending.operation === "audioDevices") {
            audioOutputDevicesTimeout.stop()
            audioOutputDevicesRequestId = ""
            audioOutputDevicesBusy = false
            if (responseType === "audioDevices" && Array.isArray(response.devices)) {
                audioOutputDevices = response.devices
                audioOutputDevicesError = ""
            } else {
                audioOutputDevices = []
                audioOutputDevicesError = String(response.message || qsTr("Could not list audio output devices"))
            }
            return
        }
        if (responseType === "error") {
            const message = String(response.message || qsTr("The embedded media runtime rejected a command"))
            if (pending.operation === "hello") {
                nativeRuntimeReady = false
                streamerDetection = {available: false, availableCodecs: [], capabilities: ({})}
                streamerDetectionMessage = message
            } else if (pending.operation === "start") {
                streamerStartRequestId = ""
                updateStreamerFields({status: "error", message: message,
                                      errorCode: String(response.code || "native_stream_error")})
            } else if (pending.operation === "stop") {
                streamerStopRequestId = ""
                lastError = message
                if (sessionRecoveryPending) scheduleSessionRecovery(message)
            } else if (pending.operation === "input") {
                streamInputPauseRequestId = ""
            } else if (pending.operation === "control") {
                streamControlRequestId = ""
                streamControlAction = ""
                streamControlMessage = message
            } else if (pending.operation === "microphone") {
                if (requestId === microphoneRequestId) {
                    microphoneRequestId = ""
                    microphoneRecoveryEnabled = false
                }
                streamControlMessage = message
                accessibilityMessage = message
            } else if (pending.operation === "recording-start") {
                streamRecordingStartRequestId = ""
                mediaMessage = message
            } else if (pending.operation === "recording-stop") {
                streamRecordingStopRequestId = ""
                mediaMessage = message
            } else if (pending.operation === "clip-save") {
                streamClipRequestId = ""
                mediaMessage = message
                accessibilityMessage = message
                streamCaptureAnnounced(message)
            } else if (pending.operation === "replay-stop") {
                mediaMessage = message
                accessibilityMessage = message
                streamCaptureAnnounced(message)
            }
            lastError = message
            return
        }

        if (pending.operation === "hello") {
            const protocolVersion = Number(response.capabilities
                && response.capabilities.protocolVersion || 0)
            nativeRuntimeReady = responseType === "ready"
                && protocolVersion === nativeProtocolVersion
            if (!nativeRuntimeReady) {
                lastError = qsTr("The embedded media runtime returned an invalid handshake")
                streamerDetection = {available: false, availableCodecs: [], capabilities: ({})}
                streamerDetectionMessage = lastError
                return
            }
            acceptNativeCapabilities(response.capabilities || ({}))
            if (activeSession && (!streamer || streamer.status === "starting"))
                Qt.callLater(() => root.startNativeStreamer())
        } else if (pending.operation === "start") {
            streamerStartRequestId = ""
            if (sessionRecoveryPending) return
            streamReplayEnabled = response.replayEnabled === true
            if (!replayBufferRequested)
                disableStreamReplay()
            updateStreamerFields({
                status: "streaming",
                message: qsTr("Native-owned NVST media transport is active"),
                transport: String(response.transport || "nvst"),
                capabilities: Object.assign({}, nativeRuntimeCapabilities,
                                            response.capabilities || ({}),
                                            {supportsMicrophone: Boolean(response.capabilities
                                                && response.capabilities.supportsMicrophone === true)}),
                errorCode: null
            })
        } else if (pending.operation === "stop") {
            streamerStopRequestId = ""
            microphoneRequestId = ""
            sessionMicrophoneMode = "disabled"
            updateStreamerFields({status: "stopped", message: pending.reason || qsTr("Stream stopped"),
                                  sessionId: null, transport: null, inputReady: false,
                                  microphoneState: "disabled", microphoneEnabled: false, microphoneMessage: ""})
            if (sessionRecoveryPending) {
                streamerStartRequestId = ""
                discoverRecoverySession()
            }
        } else if (pending.operation === "input") {
            streamInputPauseRequestId = ""
            currentStreamInputPaused = Boolean(pending.paused)
            streamInputStateKnown = true
            const counter = currentStreamInputPaused ? "inputPauseCount" : "inputResumeCount"
            const values = {}
            values[counter] = Number(streamer && streamer[counter] || 0) + 1
            updateStreamerFields(values)
            if (currentStreamInputPaused !== desiredStreamInputPaused)
                Qt.callLater(() => root.setStreamInputPaused(root.desiredStreamInputPaused))
        } else if (pending.operation === "control") {
            streamControlRequestId = ""
            streamControlAction = ""
            streamControlMessage = qsTr("Stream control applied")
        } else if (pending.operation === "microphone") {
            if (requestId === microphoneRequestId) {
                microphoneRequestId = ""
                microphoneRecoveryEnabled = pending.enabled === true && pending.microphoneFailed !== true
            }
        } else if (pending.operation === "clip-save") {
            mediaMessage = qsTr("Saving clip…")
            accessibilityMessage = mediaMessage
            streamCaptureAnnounced(mediaMessage)
        } else if (pending.operation === "recording-start") {
            streamRecordingStartRequestId = ""
            streamRecordingActive = true
            streamRecordingStartedAtMs = Date.now()
            streamRecordingElapsedMs = 0
            updateStreamerFields({recordingStartCount: Number(streamer && streamer.recordingStartCount || 0) + 1})
            const rect = streamCaptureRect
            if (pendingRecordingThumbnailPath) {
                AppController.captureScreenRegionTo(
                    Number(rect.x || 0), Number(rect.y || 0),
                    Number(rect.width || 0), Number(rect.height || 0),
                    pendingRecordingThumbnailPath)
            }
            mediaMessage = qsTr("Recording source video + stream audio")
            accessibilityMessage = qsTr("Recording started")
        } else if (pending.operation === "recording-stop") {
            streamRecordingStopRequestId = ""
            streamRecordingActive = false
            streamRecordingElapsedMs = 0
            pendingRecordingPath = ""
            pendingRecordingThumbnailPath = ""
            updateStreamerFields({recordingStopCount: Number(streamer && streamer.recordingStopCount || 0) + 1})
            mediaMessage = response.path ? qsTr("Recording saved") : qsTr("Recording stopped")
            accessibilityMessage = response.path
                ? qsTr("Recording saved to %1").arg(response.path) : mediaMessage
            refreshMedia()
        }
    }

    function recordQueueDrop(event) {
        const count = Number(event.count)
        if (dropSessionId === "" || !Number.isSafeInteger(count) || count <= 0)
            return
        let field = "otherQueueDropCount"
        let amount = count
        if (event.unit === "frames") {
            field = "videoDropCount"
        } else if (event.unit === "packets") {
            field = "audioPacketDropCount"
        } else if (event.unit === "callbacks") {
            field = "callbackDropCount"
        } else if (event.unit === "samples") {
            const rate = Number(event.sampleRate)
            const channels = Number(event.channels)
            if (Number.isSafeInteger(rate) && rate > 0 && rate <= 384000
                    && Number.isSafeInteger(channels) && channels > 0 && channels <= 32) {
                field = "audioDiscardedMs"
                amount = count / rate / channels * 1000
            }
        }
        const next = Object.assign({}, streamDropCounts)
        next[field] = Math.min(Number.MAX_SAFE_INTEGER, Number(next[field] || 0) + amount)
        streamDropCounts = next
    }

    function acceptNativeEvent(event) {
        const type = String(event && event.type || "")
        const fields = {}
        if (event.framesPerSecond !== undefined)
            fields.framesPerSecond = Number(event.framesPerSecond)
        if (event.bitrateMbps !== undefined)
            fields.bitrateMbps = Number(event.bitrateMbps)
        if (event.peakBitrateMbps !== undefined)
            fields.peakBitrateMbps = Number(event.peakBitrateMbps)
        // Keep missing measurements unavailable instead of converting null to 0.
        for (const key of ["pingMs", "jitterMs", "packetLossPercent", "decodeTimeMs", "latencyMs"]) {
            if (event[key] !== undefined)
                fields[key] = event[key] === null || !Number.isFinite(Number(event[key]))
                    ? null : Number(event[key])
        }
        if (event.event === "first-frame") {
            if (!activeSession || !streamer || streamer.status === "error" || streamer.status === "stopped")
                return
            // Only real video progress closes the recovery episode. A ready
            // seat, successful PLAY, or audio/control traffic is insufficient.
            streamerRestartRecoveryCount += streamerRestartAttempts
            if (sessionReconnectAttempts > 0)
                sessionRecoveryCount += 1
            streamerRestartAttempts = 0
            sessionReconnectAttempts = 0
            streamerRecoveryExhausted = false
            streamerRestartTimer.stop()
            if (streamer && (streamer.firstFrameLatencyMs === undefined
                    || streamer.firstFrameLatencyMs === null)) {
                fields.firstFrameLatencyMs = Math.max(0,
                    Date.now() - Number(streamer.sessionStartedAtMs || Date.now()))
            }
            fields.mediaBackend = String(event.backend || "")
        } else if (event.event === "backend-fallback") {
            fields.backendFallbackCount = Number(streamer && streamer.backendFallbackCount || 0) + 1
        } else if (event.event === "decoder-error") {
            fields.decoderErrorCount = Number(streamer && streamer.decoderErrorCount || 0) + 1
        } else if (event.event === "output-error") {
            fields.outputErrorCount = Number(streamer && streamer.outputErrorCount || 0) + 1
        } else if (event.event === "device-state") {
            const key = event.recovered ? "deviceRecoveryCount" : "deviceLossCount"
            fields[key] = Number(streamer && streamer[key] || 0) + 1
        } else if (event.event === "queue-dropped") {
            if (!Number.isSafeInteger(Number(event.count)) || Number(event.count) <= 0)
                return
            recordQueueDrop(event)
            fields.queueDropCount = Number(streamer && streamer.queueDropCount || 0)
                + Number(event.count || 0)
        }

        if (type === "status") {
            fields.status = event.status === "ready" ? "streaming" : String(event.status || "streaming")
            fields.message = String(event.message || streamMessage)
        } else if (type === "error") {
            fields.status = "error"
            fields.message = String(event.message || qsTr("Native media runtime failed"))
            fields.errorCode = String(event.code || "native_stream_error")
        } else if (type === "input-ready") {
            fields.inputReady = true
            fields.inputUnavailableReason = null
        } else if (type === "input-unavailable") {
            fields.inputReady = false
            fields.inputUnavailableReason = String(event.reason || "")
        } else if (type === "microphone-state") {
            fields.microphoneState = ["disabled", "muted", "ready", "unavailable", "error"]
                .indexOf(String(event.state)) >= 0 ? String(event.state) : "error"
            fields.microphoneEnabled = fields.microphoneState === "ready" && event.enabled === true
            fields.microphoneMessage = String(event.message || "")
            if (["muted", "unavailable", "error"].indexOf(fields.microphoneState) >= 0)
                microphoneRecoveryEnabled = false
            if (["unavailable", "error"].indexOf(fields.microphoneState) >= 0
                    && nativeRequests[microphoneRequestId]) {
                const requests = Object.assign({}, nativeRequests)
                requests[microphoneRequestId] = Object.assign({}, requests[microphoneRequestId],
                    {microphoneFailed: true})
                nativeRequests = requests
            }
            accessibilityMessage = fields.microphoneMessage || qsTr("Microphone state changed")
            if (fields.microphoneState === "error" || fields.microphoneState === "unavailable")
                streamControlMessage = fields.microphoneMessage || qsTr("Microphone error")
        } else if (type === "clip-state") {
            if (streamClipRequestId === "" || String(event.requestId || "") !== streamClipRequestId)
                return
            if (event.state === "saved" || event.state === "failed") {
                streamClipRequestId = ""
                mediaMessage = event.state === "saved" ? qsTr("Clip saved")
                    : String(event.message || qsTr("Clip failed"))
                accessibilityMessage = event.state === "saved"
                    ? qsTr("Clip saved to %1").arg(String(event.path || "")) : mediaMessage
                streamCaptureAnnounced(mediaMessage)
                if (event.state === "failed")
                    lastError = mediaMessage
                refreshMedia()
            }
        } else if (type === "recording-state") {
            if (event.state === "saved" || event.state === "failed") {
                streamRecordingActive = false
                streamRecordingElapsedMs = 0
                mediaMessage = event.state === "saved"
                    ? qsTr("Recording saved")
                    : String(event.message || qsTr("Recording failed"))
                if (event.state === "failed")
                    lastError = mediaMessage
                refreshMedia()
            }
        } else if (type === "overlay-request") {
            fields.overlayRequestGeneration = Number(streamer && streamer.overlayRequestGeneration || 0) + 1
        } else if (type === "screenshot-request") {
            fields.screenshotRequestGeneration = Number(streamer && streamer.screenshotRequestGeneration || 0) + 1
        } else if (type === "recording-toggle-request") {
            fields.recordingToggleRequestGeneration = Number(streamer && streamer.recordingToggleRequestGeneration || 0) + 1
        } else if (type === "shortcut-action") {
            fields.shortcutActionGeneration = Number(streamer && streamer.shortcutActionGeneration || 0) + 1
            fields.shortcutAction = String(event.action || "")
            if (fields.shortcutAction === "toggle-stats")
                fields.statsToggleCount = Number(streamer && streamer.statsToggleCount || 0) + 1
            else if (fields.shortcutAction === "toggle-fullscreen")
                fields.fullscreenToggleCount = Number(streamer && streamer.fullscreenToggleCount || 0) + 1
        }
        updateStreamerFields(fields)
    }

    property Connections nativeRuntimeConnections: Connections {
        target: NativeStreamRuntime
        function onPresentationError(message) {
            root.lastError = message
            if (root.activeSession)
                root.updateStreamerFields({status: "error", message: message,
                    errorCode: "streamer_presentation_failed"})
        }
        function onResponseReceived(response) { root.acceptNativeResponse(response) }
        function onEventReceived(event) { root.acceptNativeEvent(event) }
        function onCallbacksDropped(count) {
            root.recordQueueDrop({unit: "callbacks", count: count})
            root.updateStreamerFields({queueDropCount:
                Number(root.streamer && root.streamer.queueDropCount || 0) + Number(count || 0)})
        }
        function onRunningChanged() {
            if (NativeStreamRuntime.running)
                return
            root.nativeRuntimeReady = false
            root.audioOutputDevices = []
            root.audioOutputDevicesBusy = false
            root.audioOutputDevicesError = ""
            root.audioOutputDevicesRequestId = ""
            root.audioOutputDevicesTimeout.stop()
            root.nativeRequests = ({})
            root.streamerStartRequestId = ""
            root.streamerStopRequestId = ""
            root.streamInputPauseRequestId = ""
            root.streamControlRequestId = ""
            root.microphoneRequestId = ""
            root.sessionMicrophoneMode = "disabled"
            root.streamRecordingStartRequestId = ""
            root.streamRecordingStopRequestId = ""
            root.resetStreamReplay()
            if (root.streamer && root.streamer.status !== "stopped"
                    && root.streamer.status !== "error")
                root.updateStreamerFields({status: "error",
                    message: NativeStreamRuntime.lastError || qsTr("The embedded media runtime stopped"),
                    errorCode: "streamer_closed"})
        }
    }

    property Connections coreConnections: Connections {
        target: CoreClient
        function onStateChanged() {
            if (CoreClient.state === "ready")
                root.initializeServices()
            else if (CoreClient.state === "failed") {
                root.lastError = CoreClient.lastError
                root.authRestorePending = false
            }
        }
        function onResponseReceived(requestId, result) {
            if (onboardingOwner.acceptResponse(requestId, result)) {
                return
            } else if (root.finishArtworkRequest(requestId, result, false)) {
                return
            } else if (requestId === root.settingsRequestId && result.settings) {
                settingsOwner.acceptSettings(result)
                root.resolveDirectLaunch()
                root.syncTelemetry()
                root.syncDiscordPresence()
                root.refreshStreamerDetection()
                if (result.settings.autoCheckForUpdates && root.updaterCheckRequestId === "")
                    root.autoUpdateCheckTimer.restart()
            } else if (requestId === root.consoleSurfaceRequestId) {
                settingsOwner.acceptConsoleSurface(result)
            } else if (requestId === root.providersRequestId) {
                root.providers = result.providers || []
                root.providersRequestId = ""
            } else if (requestId === root.authSessionRequestId) {
                root.authSession = result.session || null
                root.sessionPersistence = result.persistence || "none"
                root.authState = root.authSession ? "signed-in" : "idle"
                root.authSessionRequestId = ""
                root.authRestorePending = false
                if (root.sessionPersistenceMessage !== "")
                    root.accessibilityMessage = root.sessionPersistenceMessage
                if (root.authSession && root.catalogSource !== "account-library")
                    root.reloadCatalogForSession()
                if (root.authSession)
                    root.refreshAccountServices()
                if (root.authSession)
                    root.refreshRemoteSessions()
                else
                    root.remoteSessions = []
                root.resolveDirectLaunch()
            } else if (requestId === root.catalogRequestId) {
                catalogOwner.acceptCatalog(result)
                root.resolveDirectLaunch()
            } else if (requestId === root.storeRequestId) {
                root.acceptStorePage(result)
            } else if (requestId === root.storePresentationRequestId) {
                catalogOwner.acceptStorePresentation(result)
            } else if (requestId === root.deviceStartRequestId) {
                root.authChallenge = result
                root.authState = "waiting"
                root.authMessage = qsTr("Scan the QR code or enter %1").arg(result.userCode)
                root.deviceStartRequestId = ""
                root.devicePollTimer.restart()
            } else if (requestId === root.devicePollRequestId) {
                root.devicePollRequestId = ""
                const status = result.status || "error"
                if (status === "authorized") {
                    root.devicePollTimer.stop()
                    root.authState = "completing"
                    root.authMessage = qsTr("Signed in. Loading your profile…")
                    root.deviceCompleteRequestId = CoreClient.request("auth.device.complete", {
                        attemptId: root.authChallenge.attemptId,
                        staySignedIn: root.pendingStaySignedIn
                    }, 30000)
                } else if (status === "pending") {
                    root.authState = "waiting"
                } else if (status === "slow_down") {
                    root.authChallenge.intervalSeconds = Number(result.intervalSeconds || 10)
                    root.devicePollTimer.restart()
                } else {
                    root.devicePollTimer.stop()
                    root.authState = "error"
                    root.authMessage = result.error || qsTr("Device sign-in failed")
                }
            } else if (requestId === root.deviceCompleteRequestId) {
                root.authSession = result.session || null
                root.sessionPersistence = result.persistence || "memory-only"
                root.authChallenge = null
                root.authState = root.authSession ? "signed-in" : "error"
                root.authMessage = root.authSession
                    ? (root.sessionPersistenceMessage !== ""
                        ? qsTr("Welcome, %1. %2").arg(root.authSession.user.displayName).arg(root.sessionPersistenceMessage)
                        : qsTr("Welcome, %1").arg(root.authSession.user.displayName))
                    : qsTr("Sign-in did not return a session")
                root.deviceCompleteRequestId = ""
                if (root.sessionPersistenceMessage !== "")
                    root.accessibilityMessage = root.sessionPersistenceMessage
                if (root.authSession)
                    root.reloadCatalogForSession()
                if (root.authSession)
                    root.refreshAccountServices()
                root.resolveDirectLaunch()
            } else if (requestId === root.logoutRequestId) {
                root.authSession = result.session || null
                if (!root.authSession)
                    root.sessionPersistence = "none"
                root.authState = root.authSession ? "signed-in" : "idle"
                root.authMessage = qsTr("")
                root.logoutRequestId = ""
                root.subscription = null
                root.regions = []
                root.resetRegionPing()
                root.reloadCatalogForSession()
                if (root.authSession)
                    root.refreshAccountServices()
            } else if (requestId === root.logoutAllRequestId) {
                root.logoutAllRequestId = ""
                root.authSession = null
                root.sessionPersistence = "none"
                root.authState = "idle"
                root.authMessage = qsTr("")
                root.savedAccounts = []
                root.subscription = null
                root.regions = []
                root.resetRegionPing()
                root.reloadCatalogForSession()
                root.accessibilityMessage = qsTr("All saved accounts signed out")
            } else if (requestId === root.subscriptionRequestId) {
                accountServicesOwner.acceptSubscription(result)
            } else if (requestId === root.regionsRequestId) {
                accountServicesOwner.acceptRegions(result)
            } else if (requestId === root.regionPingRequestId) {
                accountServicesOwner.acceptRegionPing(result)
            } else if (requestId === root.accountsRequestId) {
                root.savedAccounts = result.accounts || []
                root.accountsRequestId = ""
            } else if (requestId === root.accountSwitchRequestId) {
                root.resetRegionPing()
                root.regions = []
                root.authSession = result.session || null
                root.sessionPersistence = result.persistence || "os-credential-store"
                root.authState = root.authSession ? "signed-in" : "error"
                root.accountSwitchRequestId = ""
                if (root.sessionPersistenceMessage !== "")
                    root.accessibilityMessage = root.sessionPersistenceMessage
                root.reloadCatalogForSession()
                root.refreshAccountServices()
                root.pinMessage = qsTr("")
                if (AppController.route === "profile-pin")
                    AppController.navigate("accounts")
            } else if (requestId === root.accountRemoveRequestId) {
                root.accountRemoveRequestId = ""
                if (root.accountsRequestId === "")
                    root.accountsRequestId = CoreClient.request("auth.accounts.list", {})
                root.authSessionRequestId = CoreClient.request("auth.session.get", {})
            } else if (requestId === root.pinRequestId) {
                root.pinRequestId = ""
                if (result.ok) {
                    root.pinMessage = qsTr("")
                    if (root.accountsRequestId === "")
                        root.accountsRequestId = CoreClient.request("auth.accounts.list", {})
                    AppController.navigate("accounts")
                } else {
                    root.pinMessage = result.reason === "locked_out"
                        ? qsTr("Too many attempts. This profile is temporarily locked.")
                        : result.reason === "invalid_pin"
                            ? qsTr("That PIN is incorrect.")
                            : qsTr("Enter exactly four digits.")
                }
            } else if (requestId === root.gameAccountsRequestId) {
                accountServicesOwner.acceptGameAccounts(result)
            } else if (requestId === root.gameAccountActionRequestId) {
                accountServicesOwner.acceptGameAccountAction(result)
            } else if (requestId === root.accountLinkStartRequestId) {
                accountServicesOwner.acceptAccountLinkStart(result)
            } else if (requestId === root.accountLinkPollRequestId) {
                accountServicesOwner.acceptAccountLinkPoll(result)
            } else if (requestId === root.storageLocationsRequestId) {
                accountServicesOwner.acceptStorageLocations(result)
            } else if (requestId === root.storageResetRequestId) {
                accountServicesOwner.acceptStorageReset(result)
            } else if (requestId === root.mediaRequestId) {
                root.mediaRequestId = ""
                root.mediaItems = result.items || []
                root.mediaRootPath = result.rootPath || ""
                root.mediaState = "ready"
                root.mediaMessage = root.mediaItems.length
                    ? qsTr("%1 screenshots · %2 recordings").arg(result.screenshots).arg(result.recordings)
                    : qsTr("No captures yet")
            } else if (requestId === root.mediaDeleteRequestId) {
                root.mediaDeleteRequestId = ""
                root.mediaMessage = qsTr("Capture deleted")
                root.refreshMedia()
            } else if (requestId === root.mediaClipTargetRequestId && requestId !== "") {
                root.mediaClipTargetRequestId = ""
                if (!root.streamReplayEnabled || !root.replayBufferRequested
                        || !root.activeSession || !root.streamer || root.streamer.status !== "streaming")
                    return
                root.streamClipRequestId = root.sendNativeCommand("clip-save", {
                    outputPath: String(result.path || "")
                }, "clip-save")
                if (root.streamClipRequestId === "") {
                    root.mediaMessage = root.lastError
                    root.accessibilityMessage = root.mediaMessage
                    root.streamCaptureAnnounced(root.mediaMessage)
                }
            } else if (requestId === root.mediaRecordingTargetRequestId) {
                root.mediaRecordingTargetRequestId = ""
                root.pendingRecordingPath = String(result.path || "")
                root.pendingRecordingThumbnailPath = String(result.thumbnailPath || "")
                root.streamRecordingStartRequestId = root.sendNativeCommand("recording-start", {
                    outputPath: root.pendingRecordingPath
                }, "recording-start")
                if (root.streamRecordingStartRequestId === "") {
                    root.pendingRecordingPath = ""
                    root.pendingRecordingThumbnailPath = ""
                    root.mediaMessage = root.lastError
                }
            } else if (requestId === root.diagnosticsRequestId) {
                root.diagnosticsRequestId = ""
                root.diagnostics = result
                root.diagnosticsMessage = result.entries && result.entries.length
                    ? qsTr("%1 recent redacted events").arg(result.entries.length)
                    : qsTr("No diagnostic events recorded")
            } else if (requestId === root.diagnosticsExportRequestId) {
                root.diagnosticsExportRequestId = ""
                root.diagnosticsMessage = qsTr("Saved redacted report to %1").arg(result.path)
                AppController.openLocalPath(result.path, true)
            } else if (requestId === root.acceptanceExportRequestId) {
                root.acceptanceExportRequestId = ""
                root.diagnosticsMessage = qsTr("Saved live acceptance evidence to %1").arg(result.path)
                AppController.openLocalPath(result.path, true)
            } else if (requestId === root.updaterStateRequestId) {
                root.updaterStateRequestId = ""
                root.updaterState = result
            } else if (requestId === root.updaterCheckRequestId) {
                root.updaterCheckRequestId = ""
                root.updaterState = result
                root.updaterHighlightsRequestId = CoreClient.request("updater.highlights.get", {})
            } else if (requestId === root.updaterHighlightsRequestId) {
                root.updaterHighlightsRequestId = ""
                root.releaseHighlights = result
            } else if (requestId === root.updaterDownloadRequestId) {
                root.updaterDownloadRequestId = ""
                root.updaterState = result
                root.accessibilityMessage = qsTr("Signed update downloaded and verified")
            } else if (requestId === root.updaterInstallRequestId) {
                root.updaterInstallRequestId = ""
                root.updaterState = result
                root.accessibilityMessage = qsTr("Verified update installer launched")
                Qt.callLater(() => AppController.quitApplication())
            } else if (requestId === root.socialCapabilitiesRequestId) {
                root.socialCapabilitiesRequestId = ""
                root.socialCapabilities = result
            } else if (requestId === root.discordRequestId) {
                root.discordRequestId = ""
            } else if (requestId === root.telemetryRequestId) {
                root.telemetryRequestId = ""
            } else if (requestId === root.feedbackRequestId) {
                root.feedbackRequestId = ""
                root.reportingState = "sent"
                root.reportingMessage = result.message || qsTr("Thanks — your feedback was sent.")
            } else if (requestId === root.bugReportRequestId) {
                root.bugReportRequestId = ""
                root.reportingState = "sent"
                root.reportingMessage = result.reference
                    ? qsTr("Bug report sent · reference %1").arg(result.reference)
                    : qsTr("Bug report sent successfully.")
            } else if (requestId === root.sessionAdRequestId) {
                root.sessionAdRequestId = ""
                root.acceptStreamingSession(result.session || root.activeSession)
                const state = root.activeSession ? (root.activeSession.adState || ({})) : ({})
                const ads = state.sessionAds || state.ads || []
                if (!(state.sessionAdsRequired || state.isAdsRequired) || ads.length === 0)
                    AppController.showOverlay("")
            } else if (requestId === root.activeSessionRequestId) {
                root.activeSessionRequestId = ""
                root.acceptStreamingSession(result.session || null)
            } else if (requestId === root.remoteSessionDiscoveryRequestId) {
                root.remoteSessionDiscoveryRequestId = ""
                root.remoteSessions = result.sessions || []
            } else if (requestId === root.remoteSessionsRequestId) {
                root.remoteSessionsRequestId = ""
                root.inspectRemoteSessions(result)
            } else if (requestId === root.recoveryDiscoveryRequestId) {
                root.recoveryDiscoveryRequestId = ""
                root.acceptRecoverySessions(result)
            } else if (requestId === root.sessionClaimRequestId) {
                const recovering = root.sessionClaimIsRecovery
                root.sessionClaimRequestId = ""
                root.sessionClaimIsRecovery = false
                if (recovering && root.sessionRecoveryPending
                        && (!root.activeSession || String(root.activeSession.sessionId) !== root.recoverySessionId)) return
                root.sessionRecoveryPending = false
                root.resumePollAttempts = 0
                root.resumePollDeadlineMs = Date.now() + 90000
                root.streamer = null
                root.selectGameForSession(result.session)
                root.pendingLaunchParams = null
                root.conflictSession = null
                root.launchConflictDetected = false
                root.remoteSessions = []
                root.acceptStreamingSession(result.session || null)
            } else if (requestId === root.streamCreateRequestId) {
                root.streamCreateRequestId = ""
                root.launchConflictDetected = false
                root.acceptStreamingSession(result.session || null)
            } else if (requestId === root.streamPollRequestId) {
                root.streamPollRequestId = ""
                root.acceptStreamingSession(result.session || null)
            } else if (requestId === root.streamStopRequestId) {
                root.streamStopRequestId = ""
                const wasForceNewAfterStop = root.forceNewAfterStop
                root.remoteSessions = []
                root.acceptStreamingSession(null)
                if (root.forceNewAfterStop) {
                    root.forceNewAfterStop = false
                    root.conflictSession = null
                    if (root.pendingLaunchParams)
                        root.createPendingSession()
                    else if (AppController.route === "inserting")
                        AppController.navigate("home")
                } else if (AppController.route !== "game-detail")
                    AppController.navigateFromLastPrimary("game-detail")
                if (!wasForceNewAfterStop && Boolean(root.settings.showSessionReport)
                        && root.lastSessionReport)
                    AppController.showOverlay("session-report")
            } else if (requestId === root.streamerPrepareRequestId) {
                root.streamerPrepareRequestId = ""
                if (!result.context) {
                    root.acceptStreamerSnapshot(Object.assign({}, root.streamer || ({}), {
                        status: "error",
                        message: qsTr("The core returned an invalid embedded stream context"),
                        errorCode: "invalid_stream_context"
                    }))
                    return
                }
                const preparedSettings = result.context.settings || ({})
                const initialMicrophoneEnabled = root.prepareMicrophoneStart(
                    root.activeSession.sessionId, preparedSettings.microphoneMode)
                root.runtimeStreamProfile = {
                    codec: String(preparedSettings.codec || "").toUpperCase(),
                    width: Number(preparedSettings.width || root.negotiatedStreamProfile.width || 0),
                    height: Number(preparedSettings.height || root.negotiatedStreamProfile.height || 0),
                    maxBitrateMbps: Number(preparedSettings.maxBitrateMbps || 0),
                    fps: Number(preparedSettings.fps || root.negotiatedStreamProfile.fps || 0)
                }
                root.acceptStreamerSnapshot(Object.assign({}, root.streamer || ({}), {
                    codec: root.runtimeStreamProfile.codec,
                    outputWidth: root.runtimeStreamProfile.width,
                    outputHeight: root.runtimeStreamProfile.height,
                    outputFps: root.runtimeStreamProfile.fps
                }))
                root.streamerStartRequestId = root.sendNativeCommand("start", {
                    context: result.context,
                    microphoneEnabled: initialMicrophoneEnabled
                }, "start")
                if (root.streamerStartRequestId === "")
                    root.acceptStreamerSnapshot(Object.assign({}, root.streamer || ({}), {
                        status: "error",
                        message: NativeStreamRuntime.lastError || qsTr("The embedded media runtime could not start the stream"),
                        errorCode: "streamer_start_failed"
                    }))
            }
        }
        function onRequestFailed(requestId, code, message) {
            if (onboardingOwner.acceptFailure(requestId, message)) {
                return
            } else if (requestId === root.storePresentationRequestId && requestId !== "") {
                catalogOwner.failStorePresentation(message)
                return
            }
            if (root.finishArtworkRequest(requestId, null, true))
                return
            if (requestId === root.remoteSessionDiscoveryRequestId) {
                root.remoteSessionDiscoveryRequestId = ""
                root.remoteSessions = []
                return
            }
            root.lastError = message
            if (requestId === root.consoleSurfaceRequestId) {
                settingsOwner.failConsoleSurface(message)
            } else if (requestId === root.catalogRequestId) {
                catalogOwner.failCatalog(message)
            } else if (requestId === root.storeRequestId) {
                catalogOwner.failStore(message)
            } else if (requestId === root.providersRequestId) {
                root.providersRequestId = ""
            } else if (requestId === root.authSessionRequestId) {
                root.authSessionRequestId = ""
                root.authRestorePending = false
                root.authState = root.authSession ? "signed-in" : "idle"
                if (code !== "cancelled")
                    root.authMessage = message
            } else if (requestId === root.deviceStartRequestId
                       || requestId === root.devicePollRequestId
                       || requestId === root.deviceCompleteRequestId) {
                root.devicePollTimer.stop()
                root.authState = code === "cancelled" ? "idle" : "error"
                root.authMessage = code === "cancelled" ? "" : message
                root.deviceStartRequestId = ""
                root.devicePollRequestId = ""
                root.deviceCompleteRequestId = ""
            } else if (requestId === root.logoutRequestId) {
                root.logoutRequestId = ""
                root.authMessage = message
            } else if (requestId === root.logoutAllRequestId) {
                root.logoutAllRequestId = ""
                root.authMessage = message
            } else if (requestId === root.subscriptionRequestId) {
                accountServicesOwner.failSubscription(message)
            } else if (requestId === root.regionsRequestId) {
                accountServicesOwner.failRegions(message)
            } else if (requestId === root.regionPingRequestId) {
                accountServicesOwner.failRegionPing(message)
            } else if (requestId === root.accountsRequestId) {
                root.accountsRequestId = ""
            } else if (requestId === root.accountSwitchRequestId) {
                root.accountSwitchRequestId = ""
                root.pinMessage = message
            } else if (requestId === root.accountRemoveRequestId) {
                root.accountRemoveRequestId = ""
            } else if (requestId === root.pinRequestId) {
                root.pinRequestId = ""
                root.pinMessage = message
            } else if (requestId === root.gameAccountsRequestId) {
                accountServicesOwner.failGameAccounts(message)
            } else if (requestId === root.gameAccountActionRequestId) {
                accountServicesOwner.failGameAccountAction(message)
            } else if (requestId === root.accountLinkStartRequestId) {
                accountServicesOwner.failAccountLinkStart(message)
            } else if (requestId === root.accountLinkPollRequestId) {
                accountServicesOwner.failAccountLinkPoll(message)
            } else if (requestId === root.storageLocationsRequestId) {
                accountServicesOwner.failStorageLocations(message)
            } else if (requestId === root.storageResetRequestId) {
                accountServicesOwner.failStorageReset(message)
            } else if (requestId === root.mediaRequestId) {
                root.mediaRequestId = ""
                root.mediaState = "error"
                root.mediaMessage = message
            } else if (requestId === root.mediaDeleteRequestId) {
                root.mediaDeleteRequestId = ""
                root.mediaMessage = message
            } else if (requestId === root.mediaClipTargetRequestId && requestId !== "") {
                root.mediaClipTargetRequestId = ""
                root.mediaMessage = message
                root.accessibilityMessage = message
                root.streamCaptureAnnounced(message)
            } else if (requestId === root.mediaRecordingTargetRequestId) {
                root.mediaRecordingTargetRequestId = ""
                root.pendingRecordingPath = ""
                root.pendingRecordingThumbnailPath = ""
                root.mediaMessage = message
            } else if (requestId === root.diagnosticsRequestId) {
                root.diagnosticsRequestId = ""
                root.diagnosticsMessage = message
            } else if (requestId === root.diagnosticsExportRequestId) {
                root.diagnosticsExportRequestId = ""
                root.diagnosticsMessage = message
            } else if (requestId === root.acceptanceExportRequestId) {
                root.acceptanceExportRequestId = ""
                root.diagnosticsMessage = message
            } else if (requestId === root.updaterStateRequestId) {
                root.updaterStateRequestId = ""
            } else if (requestId === root.updaterCheckRequestId) {
                root.updaterCheckRequestId = ""
                root.updaterState = Object.assign({}, root.updaterState, {
                    status: "error",
                    message: message,
                    canCheck: true
                })
            } else if (requestId === root.updaterHighlightsRequestId) {
                root.updaterHighlightsRequestId = ""
            } else if (requestId === root.updaterDownloadRequestId) {
                root.updaterDownloadRequestId = ""
                root.updaterState = Object.assign({}, root.updaterState, {
                    status: "available", message: message, canCheck: true
                })
            } else if (requestId === root.updaterInstallRequestId) {
                root.updaterInstallRequestId = ""
                root.updaterState = Object.assign({}, root.updaterState, {
                    status: "downloaded", message: message, canInstall: true, canCheck: true
                })
            } else if (requestId === root.socialCapabilitiesRequestId) {
                root.socialCapabilitiesRequestId = ""
                root.socialCapabilities = Object.assign({}, root.socialCapabilities, {
                    reason: qsTr("Provider social capabilities could not be checked.")
                })
            } else if (requestId === root.discordRequestId) {
                root.discordRequestId = ""
            } else if (requestId === root.telemetryRequestId) {
                root.telemetryRequestId = ""
            } else if (requestId === root.feedbackRequestId) {
                root.feedbackRequestId = ""
                root.reportingState = "error"
                root.reportingMessage = message
            } else if (requestId === root.bugReportRequestId) {
                root.bugReportRequestId = ""
                root.reportingState = "error"
                root.reportingMessage = message
            } else if (requestId === root.sessionAdRequestId) {
                root.sessionAdRequestId = ""
                root.streamMessage = message
            } else if (requestId === root.activeSessionRequestId) {
                root.activeSessionRequestId = ""
            } else if (requestId === root.remoteSessionsRequestId) {
                root.remoteSessionsRequestId = ""
                root.streamState = "error"
                root.streamMessage = qsTr("We couldn't check whether your game is still running. Check your connection, then try again.")
                root.lastError = root.streamMessage
            } else if (requestId === root.recoveryDiscoveryRequestId) {
                root.recoveryDiscoveryRequestId = ""
                root.scheduleSessionRecovery(message)
            } else if (requestId === root.sessionClaimRequestId) {
                const recovering = root.sessionClaimIsRecovery
                root.sessionClaimRequestId = ""
                root.sessionClaimIsRecovery = false
                if (recovering) {
                    root.scheduleSessionRecovery(message)
                } else {
                    root.streamState = "conflict"
                    root.streamMessage = qsTr("We couldn't reconnect to your game. Try returning to it again, or end it and start over. Ending it may lose unsaved progress.")
                    root.lastError = root.streamMessage
                    AppController.showOverlay("session-conflict")
                }
            } else if (requestId === root.streamCreateRequestId) {
                root.streamCreateRequestId = ""
                root.handleSessionCreateFailure(code, message)
            } else if (requestId === root.streamPollRequestId) {
                root.streamPollRequestId = ""
                if (root.activeSession && root.activeSession.resumePending) {
                    root.streamMessage = qsTr("Waiting for the resumed session…")
                    root.streamPollTimer.restart()
                    return
                }
                root.sessionReconnectAttempts += 1
                root.streamState = root.activeSession && root.sessionReconnectAttempts <= 8 ? "reconnecting" : "error"
                root.streamMessage = root.streamState === "reconnecting"
                    ? qsTr("Connection interrupted. Retrying…")
                    : message
                if (root.streamState === "reconnecting")
                    root.streamPollTimer.restart()
            } else if (requestId === root.streamStopRequestId) {
                root.streamStopRequestId = ""
                root.forceNewAfterStop = false
                root.streamState = root.conflictSession ? "conflict" : "error"
                root.streamMessage = qsTr("The previous game hasn't closed. Try again in a moment. We won't start another game until it has ended.")
                root.lastError = root.streamMessage
                if (root.conflictSession)
                    AppController.showOverlay("session-conflict")
            } else if (requestId === root.streamerPrepareRequestId) {
                root.streamerPrepareRequestId = ""
                root.acceptStreamerSnapshot({ status: "error", message: message, errorCode: code })
            }
        }
        function onEventReceived(name, payload) {
            if (name === "settings.changed") {
                settingsOwner.acceptSettingsChange(payload)
            } else if (name === "settings.reset")
                root.refreshSettings()
            else if (name === "auth.session.changed") {
                root.authSession = payload.session || null
                root.authState = root.authSession ? "signed-in" : "idle"
                if (root.authSession)
                    root.refreshRemoteSessions()
                else
                    root.remoteSessions = []
            } else if (name === "session.changed")
                root.acceptStreamingSession(payload.session || null)
            else if (name === "streamer.changed")
                root.acceptStreamerSnapshot(payload.streamer || payload || null)
            else if (name === "artwork.ready")
                root.acceptArtworkResult(payload)
            else if (name === "updater.changed")
                root.updaterState = payload
            else if (name === "updater.highlights.show") {
                root.releaseHighlights = payload
                AppController.navigate("updates")
                CoreClient.request("updater.highlights.ack", {version: payload.version || ""})
            }
        }
    }
}
