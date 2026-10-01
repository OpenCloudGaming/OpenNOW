import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property var appController
    required property bool ready
    required property bool signedIn
    required property var refreshCatalogAfterAccountChange
    required property var refreshAccountServices
    signal accessibilityAnnounced(string message)
    property var subscription: null
    property string subscriptionRequestId: ""
    property var regions: []
    property string regionsVpcId: ""
    property string regionsRequestId: ""
    property var regionPingResults: ({})
    property string regionPingMessage: ""
    property bool regionPingPending: false
    readonly property bool regionPingBusy: regionPingPending || regionPingRequestId !== ""
    property string regionPingRequestId: ""
    property var gameAccounts: []
    property string gameAccountsState: "idle"
    property string gameAccountMessage: ""
    property var accountLinkAttempt: null
    property string gameAccountsRequestId: ""
    property string gameAccountActionRequestId: ""
    property string accountLinkStartRequestId: ""
    property string accountLinkPollRequestId: ""
    property var acceptsScope: function(scope) { return true }
    property var syncOperation: null
    property string syncStatusRequestId: ""
    property string syncCancelRequestId: ""
    property var storeSubscriptions: []
    property var catalogDefinitions: ({})
    onReadyChanged: if (!ready) invalidateAccount()
    function storeSubscriptionLabels(account) {
        const items = catalogDefinitions.subscriptions && catalogDefinitions.subscriptions.items || []
        const active = storeSubscriptions.map(item => item.id)
        return items.filter(item => active.indexOf(item.subscription) >= 0
            && (item.primaryStore === account.provider || (item.additionalStores || []).some(store => store.store === account.provider)))
            .map(item => item.label).join(", ")
    }
    function gameAccountAction(account) {
        if (!account) return "none"
        if (account.supportsLinking && (!account.isConnected || account.status === "expired"
                || (account.syncState === "SYNC_DENIED" && account.provider !== "STEAM"))) return "link"
        if (account.supportsSync) return "sync"
        if (account.isConnected && account.supportsLinking) return "unlink"
        return "none"
    }
    property Timer syncPollTimer: Timer {
        interval: 2000
        repeat: false
        onTriggered: root.pollSync()
    }
    property Connections syncResponses: Connections {
        target: root.coreClient
        function onResponseReceived(requestId, result) {
            if (requestId === root.syncCancelRequestId && requestId !== "") {
                root.syncCancelRequestId = ""
                return
            }
            if (requestId !== root.syncStatusRequestId || requestId === "") return
            root.syncStatusRequestId = ""
            if (!root.acceptsScope(result.scope)) return
            root.acceptSync(result)
        }
        function onRequestFailed(requestId, code, message) {
            if (requestId === root.syncCancelRequestId && requestId !== "") {
                root.syncCancelRequestId = ""
                return
            }
            if (requestId !== root.syncStatusRequestId || requestId === "") return
            root.syncStatusRequestId = ""
            root.gameAccountMessage = qsTr("Could not check sync completion: %1").arg(message)
            if (root.syncOperation && root.signedIn) root.syncPollTimer.restart()
        }
    }
    property Timer accountLinkPollTimer: Timer {
        interval: 1200
        repeat: true
        running: false
        onTriggered: root.pollAccountLink()
    }
    property var storageLocations: []
    property string storageMessage: ""
    property string storageLocationsRequestId: ""
    property string storageResetRequestId: ""

    function invalidateAccount() {
        accountLinkPollTimer.stop()
        syncPollTimer.stop()
        for (const key of ["subscriptionRequestId", "regionsRequestId", "regionPingRequestId",
                "gameAccountsRequestId", "gameAccountActionRequestId", "accountLinkStartRequestId",
                "accountLinkPollRequestId", "syncStatusRequestId", "syncCancelRequestId", "storageLocationsRequestId", "storageResetRequestId"]) {
            const requestId = root[key]
            root[key] = ""
            if (requestId !== "") coreClient.cancel(requestId)
        }
        subscription = null
        regions = []
        regionsVpcId = ""
        regionPingPending = false
        regionPingResults = ({})
        regionPingMessage = ""
        gameAccounts = []
        gameAccountsState = "idle"
        gameAccountMessage = ""
        accountLinkAttempt = null
        syncOperation = null
        storeSubscriptions = []
        catalogDefinitions = ({})
        storageLocations = []
        storageMessage = ""
    }

    function refreshRegions() {
        if (!ready || !signedIn || regionsRequestId !== "")
            return
        regionsRequestId = coreClient.request("network.regions.list", {}, 30000)
    }

    function pingRegions() {
        if (regionPingBusy)
            return
        if (!ready) {
            regionPingMessage = qsTr("The OpenNOW core is not ready. Try again shortly.")
            return
        }
        if (!signedIn) {
            regionPingMessage = qsTr("Sign in to measure region latency.")
            return
        }
        if (regionsRequestId !== "" || regions.length === 0) {
            regionPingPending = true
            regionPingMessage = qsTr("Loading regions before measuring latency…")
            refreshRegions()
            if (regionsRequestId === "") {
                regionPingPending = false
                regionPingMessage = qsTr("Could not load regions. Try again.")
            }
            return
        }
        regionPingResults = ({})
        regionPingMessage = qsTr("Measuring region latency…")
        regionPingRequestId = coreClient.request("network.regions.ping", {
            regions: regions
        }, 20000)
        if (regionPingRequestId === "")
            regionPingMessage = qsTr("Could not start the latency test. Try again.")
    }

    function resetRegionPing() {
        const requestId = regionPingRequestId
        regionPingRequestId = ""
        regionPingPending = false
        regionPingResults = ({})
        regionPingMessage = ""
        if (requestId !== "") coreClient.cancel(requestId)
    }

    function refreshGameAccounts() {
        if (!ready || !signedIn || gameAccountsRequestId !== "")
            return
        gameAccountsState = gameAccounts.length ? "refreshing" : "loading"
        gameAccountsRequestId = coreClient.request("account.connections.list", {}, 30000)
    }

    function startAccountLink(provider) {
        if (!ready || accountLinkStartRequestId !== "")
            return
        gameAccountMessage = qsTr("Opening %1 sign-in…").arg(provider)
        accountLinkStartRequestId = coreClient.request("account.connections.link.start", { provider: provider }, 30000)
    }

    function pollAccountLink() {
        if (!ready || !accountLinkAttempt || accountLinkPollRequestId !== "")
            return
        accountLinkPollRequestId = coreClient.request("account.connections.link.poll", {
            attemptId: accountLinkAttempt.attemptId
        }, 10000)
    }

    function syncGameAccount(provider) {
        if (!ready || gameAccountActionRequestId !== "" || syncOperation)
            return
        gameAccountMessage = qsTr("Starting library sync…")
        gameAccountActionRequestId = coreClient.request("account.connections.sync", { provider: provider }, 30000)
    }

    function pollSync() {
        if (!ready || !signedIn || !syncOperation || syncStatusRequestId !== "") return
        syncStatusRequestId = coreClient.request("account.connections.sync.status", {operationId: syncOperation.operationId}, 30000)
    }

    function cancelSyncObservation() {
        syncPollTimer.stop()
        const id = syncStatusRequestId
        syncStatusRequestId = ""
        if (id !== "") coreClient.cancel(id)
        if (syncOperation && ready)
            syncCancelRequestId = coreClient.request("account.connections.sync.cancel", {operationId: syncOperation.operationId}, 10000)
        syncOperation = null
        gameAccountMessage = qsTr("Stopped waiting. The remote store sync may still finish.")
    }

    function acceptSync(result) {
        syncOperation = result
        if (result.phase === "waiting_remote" || result.phase === "starting") {
            gameAccountMessage = qsTr("Store sync accepted. Waiting for the store to finish…")
            syncPollTimer.interval = Math.max(2000, Number(result.retryAfterMs || 2000))
            syncPollTimer.restart()
        } else if (result.phase === "refreshing_library") {
            gameAccountMessage = qsTr("Store sync finished. Refreshing your library…")
            refreshGameAccounts()
            refreshCatalogAfterAccountChange()
        } else {
            syncOperation = null
            syncPollTimer.stop()
            gameAccountMessage = result.phase === "timed_out"
                ? qsTr("Sync completion could not be confirmed. The store may still finish; refresh or retry later.")
                : qsTr("The store could not complete the sync. Check the connection and try again.")
            refreshGameAccounts()
        }
    }

    function libraryRefreshFinished(complete, message) {
        if (!syncOperation || syncOperation.phase !== "refreshing_library") return
        if (complete) {
            syncCancelRequestId = coreClient.request("account.connections.sync.status", {
                operationId: syncOperation.operationId, libraryRefreshed: true
            }, 10000)
            syncOperation = null
            gameAccountMessage = qsTr("Store sync and library refresh completed.")
        } else gameAccountMessage = qsTr("Store sync finished, but the library refresh is incomplete: %1").arg(message)
    }

    function unlinkGameAccount(provider) {
        if (!ready || gameAccountActionRequestId !== "")
            return
        gameAccountMessage = qsTr("Disconnecting account…")
        gameAccountActionRequestId = coreClient.request("account.connections.unlink", { provider: provider }, 30000)
    }

    function refreshStorageLocations() {
        if (!ready || !signedIn || storageLocationsRequestId !== "")
            return
        const addon = subscription && subscription.storageAddon ? subscription.storageAddon : ({})
        storageMessage = qsTr("Loading storage regions…")
        storageLocationsRequestId = coreClient.request("account.storage.locations", {
            serverRegionId: subscription ? subscription.serverRegionId : "",
            currentRegionCode: addon.regionCode || "",
            currentRegionName: addon.regionName || "",
            locale: "en_US"
        }, 30000)
    }

    function resetPersistentStorage(regionCode) {
        if (!ready || !signedIn || storageResetRequestId !== "")
            return
        storageMessage = qsTr("Resetting persistent storage…")
        storageResetRequestId = coreClient.request("account.storage.reset", {
            storageRegion: regionCode || null,
            confirmed: true
        }, 30000)
    }

    onRegionPingMessageChanged: if (regionPingMessage !== "") accessibilityAnnounced(regionPingMessage)

    function acceptSubscription(result) {
        root.subscription = result.subscription || null
        root.subscriptionRequestId = ""
    }

    function failSubscription(message) {
        root.subscriptionRequestId = ""
    }

    function acceptRegions(result) {
        root.regions = result.regions || []
        root.regionsVpcId = result.vpcId || ""
        root.regionsRequestId = ""
        if (root.regionPingPending) {
            root.regionPingPending = false
            if (root.regions.length > 0) root.pingRegions()
            else root.regionPingMessage = qsTr("No streaming regions are available for this account.")
        }
    }

    function failRegions(message) {
        root.regionsRequestId = ""
        root.regionPingPending = false
        root.regionPingMessage = qsTr("Could not load regions: %1").arg(message)
    }

    function acceptRegionPing(result) {
        root.regionPingRequestId = ""
        const values = {}
        const results = result.results || []
        let bestName = ""
        let bestPing = Number.MAX_VALUE
        for (let index = 0; index < results.length; ++index) {
            const item = results[index]
            const measured = Number(item.pingMs)
            values[String(item.url || "")] = item.pingMs === null || item.pingMs === undefined
                || !Number.isFinite(measured) || measured < 0 ? null : measured
            if (values[String(item.url || "")] !== null && Number(item.pingMs) < bestPing) {
                bestPing = Number(item.pingMs)
                for (let regionIndex = 0; regionIndex < root.regions.length; ++regionIndex) {
                    if (root.regions[regionIndex].url === item.url) {
                        bestName = root.regions[regionIndex].name
                        break
                    }
                }
            }
        }
        root.regionPingResults = values
        root.regionPingMessage = bestName
            ? qsTr("Best: %1 · %2 ms").arg(bestName).arg(bestPing)
            : qsTr("No region responded")
    }

    function failRegionPing(message) {
        root.regionPingRequestId = ""
        root.regionPingMessage = message
    }

    function acceptGameAccounts(result) {
        root.gameAccountsRequestId = ""
        root.gameAccounts = result.accounts || []
        root.gameAccountsState = "ready"
        root.storeSubscriptions = result.subscriptions || []
        root.catalogDefinitions = result.definitions || ({})
    }

    function failGameAccounts(message) {
        root.gameAccountsRequestId = ""
        root.gameAccountsState = "error"
        root.gameAccountMessage = message
    }

    function acceptGameAccountAction(result) {
        root.gameAccountActionRequestId = ""
        if (result.operationId) {
            acceptSync(result)
            return
        }
        root.gameAccountMessage = result.message || qsTr("Account updated")
        root.refreshGameAccounts()
        root.refreshCatalogAfterAccountChange()
    }

    function failGameAccountAction(message) {
        root.gameAccountActionRequestId = ""
        root.gameAccountMessage = message
    }

    function acceptAccountLinkStart(result) {
        root.accountLinkStartRequestId = ""
        accountLinkPollTimer.stop()
        const previousPoll = accountLinkPollRequestId
        accountLinkPollRequestId = ""
        if (previousPoll !== "") coreClient.cancel(previousPoll)
        root.accountLinkAttempt = result
        if (!appController.openExternalUrl(result.loginUrl || "")) {
            root.gameAccountMessage = qsTr("Open the provider sign-in URL in your browser.")
        } else {
            root.gameAccountMessage = qsTr("Finish signing in in your browser…")
        }
        root.accountLinkPollTimer.restart()
    }

    function failAccountLinkStart(message) {
        root.accountLinkStartRequestId = ""
        root.gameAccountMessage = message
    }

    function acceptAccountLinkPoll(result) {
        root.accountLinkPollRequestId = ""
        const status = result.status || "error"
        if (status === "pending") {
            if (result.phase === "verifying_account")
                root.gameAccountMessage = qsTr("Sign-in received. Confirming the store connection…")
            return
        }
        root.accountLinkPollTimer.stop()
        root.accountLinkAttempt = null
        if (status === "complete") {
            root.gameAccountMessage = qsTr("Account connected")
            root.refreshGameAccounts()
            root.refreshCatalogAfterAccountChange()
        } else {
            root.gameAccountMessage = result.message || qsTr("Account linking expired")
        }
    }

    function failAccountLinkPoll(message) {
        root.accountLinkPollRequestId = ""
        root.accountLinkPollTimer.stop()
        root.gameAccountMessage = message
    }

    function acceptStorageLocations(result) {
        root.storageLocationsRequestId = ""
        root.storageLocations = result.locations || []
        root.storageMessage = qsTr("")
    }

    function failStorageLocations(message) {
        root.storageLocationsRequestId = ""
        root.storageMessage = message
    }

    function acceptStorageReset(result) {
        root.storageResetRequestId = ""
        root.storageMessage = result.message || qsTr("Persistent storage was reset successfully.")
        root.refreshAccountServices()
    }

    function failStorageReset(message) {
        root.storageResetRequestId = ""
        root.storageMessage = message
    }
}
