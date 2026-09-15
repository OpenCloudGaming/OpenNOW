import QtQuick
import OpenNOW

QtObject {
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property var calls: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "store-launch-" + (calls.length + 1)
            calls = calls.concat([{id:id, method:method, params:params}])
            return id
        }
        function cancel(id) { return true }
    }
    function check(ok, message) { if (!ok) throw new Error("Store launch: " + message) }
    function capturePhase() {
        const index = Qt.application.arguments.indexOf("--store-launch-state")
        return index >= 0 && index + 1 < Qt.application.arguments.length
            ? Qt.application.arguments[index + 1] : ""
    }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function request(id) { return client.calls.find(item => item.id === id) }
    function count(method) { return client.calls.filter(item => item.method === method).length }
    function steamClient() {
        return {id:"platform-steam", title:"Steam", imageUrl:"", selectedVariantIndex:0,
            variants:[{id:"12345", store:"STEAM", libraryStatus:null, inLibrary:false, librarySelected:null,
                gfnStatus:"AVAILABLE", playStatus:null, supportsInGameSettingsPersistence:false}]}
    }
    function inspection(status, message) {
        return {store:"STEAM", appId:"platform-steam", variantId:"12345", game:steamClient(),
            decision:{status:status, message:message}, catalogRevision:3, freshness:"fresh",
            fetchedAt:1, scope:ShellStore.catalogOwnerState.authScope}
    }
    function inspectStore() {
        ShellStore.inspectStoreLaunch()
        const pending = ShellStore.storeLaunchRequestId
        check(pending !== "", "opening persistent storage did not inspect the store launch")
        const sent = request(pending)
        check(sent && sent.method === "catalog.launch.store.inspect", "store launch inspection used the wrong method")
        return sent
    }
    function verifyRendered(host) {
        const button = find(host, "persistentStorageStoreLaunch")
        const status = find(host, "persistentStorageStoreLaunchStatus")
        check(button && button.visible, "rendered store launch action is missing")
        check(status && status.visible, "rendered store launch status is missing")
        const edge = button.mapToItem(host, button.width, button.height)
        check(edge.x <= host.width && edge.y <= host.height, "store launch action exceeds the window")
        const statusEdge = status.mapToItem(host, status.width, status.height)
        check(statusEdge.x <= host.width && statusEdge.y <= host.height, "store launch status exceeds the window")
        return true
    }
    function run(host) {
        ShellStore.settings = Object.assign({}, ShellStore.settings, {
            onboardingCompleted: true, steamBigPictureMode: false, colorQuality: "8bit_420"})
        ShellStore.authGeneration = 7
        ShellStore.authSession = {user:{userId:"fixture-user",displayName:"Storage fixture"},
            provider:{idpId:"fixture-provider",code:"NVIDIA"}}
        ShellStore.subscription = {storageAddon:{regionName:"US East",regionCode:"us-east"},
            serverRegionId:"fixture-vpc", isGamePlayAllowed:true}
        ShellStore.nativeRuntimeReady = true
        AppController.navigate("persistent-storage")

        const storageRequest = client.calls.find(item => item.method === "account.storage.locations")
        check(storageRequest, "persistent storage screen did not refresh locations")
        client.responseReceived(storageRequest.id, {locations:[{code:"us-east",name:"US East",isCurrent:true,isAvailable:true}],
            regions:[], vpcId:"fixture-vpc"})

        const button = find(host, "persistentStorageStoreLaunch")
        const status = find(host, "persistentStorageStoreLaunchStatus")
        check(button, "production store launch action was not created")
        check(status, "production store launch status was not created")
        check(!button.enabled, "store launch action was offered before eligibility was confirmed")

        const inspect = inspectStore()
        client.responseReceived(inspect.id, inspection("ready", "Ready to open the Steam store for persistent game management."))
        check(ShellStore.storeLaunchTarget && ShellStore.storeLaunchTarget.appId === "platform-steam"
            && ShellStore.storeLaunchTarget.variantId === "12345", "ready store target was not adopted")
        check(button.enabled && button.text.indexOf("Steam") >= 0,
            "store launch action did not expose the eligible target")
        check(status.text.indexOf("persistent storage") >= 0, "ready store launch status was not explained")
        if (capturePhase() === "ready") return true

        button.clicked()
        check(ShellStore.pendingLaunchParams && ShellStore.pendingLaunchParams.storeLaunch === true,
            "explicit store launch intent was lost")
        const launchesAfterClick = count("catalog.launch.inspect")
        ShellStore.launchStoreGame()
        check(count("catalog.launch.inspect") === launchesAfterClick,
            "a duplicate activation started another store launch")
        check(ShellStore.pendingLaunchParams.catalogAppId === "platform-steam"
            && String(ShellStore.pendingLaunchParams.variantId) === "12345",
            "store launch did not bind the exact discovered target")
        check(!button.enabled && status.text.indexOf("Starting") >= 0,
            "the storage screen did not show the store launch in progress")
        if (capturePhase() === "pending") return true
        const launch = request(ShellStore.launchInspectRequestId)
        check(launch && launch.method === "catalog.launch.inspect" && launch.params.storeLaunch === true
            && launch.params.appId === "platform-steam" && String(launch.params.variantId) === "12345",
            "guarded launch inspection did not carry the explicit store intent")
        client.responseReceived(launch.id, inspection("subscription_required",
            "Launching the Steam store for persistent game management requires the GeForce NOW persistent storage add-on."))
        check(ShellStore.streamState === "error", "a blocked store launch continued anyway")
        check(ShellStore.streamMessage.indexOf("persistent storage add-on") >= 0,
            "blocked store launch did not explain the entitlement requirement")
        check(count("session.remote.list") === 0 && count("session.create") === 0,
            "a blocked store launch reached session allocation")
        check(AppController.route === "persistent-storage",
            "a blocked store launch left the persistent storage screen")
        check(ShellStore.storeLaunchFailed && ShellStore.storeLaunchTarget === null,
            "a blocked store launch did not surface on the storage screen")
        check(button.text === "Retry" && status.text.indexOf("persistent storage add-on") >= 0,
            "a blocked store launch did not explain the decision on the storage screen")
        if (capturePhase() === "blocked") return true

        const patching = inspectStore()
        client.responseReceived(patching.id, inspection("ready",
            "Ready to open the Steam store for persistent game management."))
        button.clicked()
        const patchingLaunch = request(ShellStore.launchInspectRequestId)
        check(patchingLaunch && patchingLaunch.params.storeLaunch === true,
            "the patching fixture did not carry the store intent")
        client.responseReceived(patchingLaunch.id, inspection("patching",
            "This store version is being patched. Try again after the patch finishes."))
        check(ShellStore.storeLaunchFailed && ShellStore.storeLaunchTarget === null
            && status.text.indexOf("being patched") >= 0 && button.text === "Retry",
            "a patching store launch did not stay visible on the storage screen")
        check(count("session.create") === 0,
            "a patching store launch reached session allocation")

        const recovering = inspectStore()
        client.responseReceived(recovering.id, inspection("ready",
            "Ready to open the Steam store for persistent game management."))
        const sessionsBefore = count("session.remote.list") + count("session.create")
        button.clicked()
        const timedOut = request(ShellStore.launchInspectRequestId)
        check(timedOut && timedOut.method === "catalog.launch.inspect" && timedOut.params.storeLaunch === true,
            "the guarded launch fixture did not start from the storage screen")
        client.requestFailed(timedOut.id, "timeout", "The store launch check timed out.")
        check(AppController.route === "persistent-storage",
            "a timed out store launch left the persistent storage screen")
        check(ShellStore.storeLaunchFailed && ShellStore.storeLaunchTarget === null
            && ShellStore.pendingLaunchParams === null,
            "a timed out store launch stayed pending on the storage screen")
        check(button.text === "Retry" && status.text.indexOf("timed out") >= 0,
            "a timed out store launch did not explain the failure on the storage screen")
        check(count("session.remote.list") + count("session.create") === sessionsBefore,
            "a timed out store launch reached session allocation")
        if (capturePhase() === "timeout") return true

        const createPhase = inspectStore()
        client.responseReceived(createPhase.id, inspection("ready",
            "Ready to open the Steam store for persistent game management."))
        const allocationsBefore = count("session.create")
        button.clicked()
        const discover = request(ShellStore.launchInspectRequestId)
        check(discover && discover.params.storeLaunch === true,
            "the session-start fixture did not carry the store intent")
        client.responseReceived(discover.id, inspection("ready",
            "Ready to open the Steam store for persistent game management."))
        check(ShellStore.pendingLaunchParams && ShellStore.pendingLaunchParams.storeLaunch === true,
            "the approved store launch lost its intent")
        check(AppController.route === "inserting",
            "the approved store launch did not start the session lookup")
        const remoteList = request(ShellStore.remoteSessionsRequestId)
        check(remoteList && remoteList.method === "session.remote.list",
            "the approved store launch did not check remote sessions")
        client.responseReceived(remoteList.id, {sessions:[]})
        const createInspect = request(ShellStore.launchInspectRequestId)
        check(createInspect && createInspect.method === "catalog.launch.inspect",
            "the session lookup did not re-check the store launch")
        client.responseReceived(createInspect.id, inspection("unavailable",
            "This store version is currently unavailable."))
        check(AppController.route === "persistent-storage",
            "a rejected store launch at session start left the storage screen")
        check(count("session.create") === allocationsBefore,
            "a rejected store launch at session start reached session allocation")
        const returnedButton = find(host, "persistentStorageStoreLaunch")
        const returnedStatus = find(host, "persistentStorageStoreLaunchStatus")
        check(returnedButton && returnedStatus, "the returned storage screen lost the store launch action")
        check(ShellStore.storeLaunchFailed && ShellStore.storeLaunchTarget === null,
            "a rejected store launch at session start was not surfaced")
        check(ShellStore.storeLaunchRequestId === "",
            "the returned storage screen re-fetched away the store launch decision")
        check(returnedButton.text === "Retry" && returnedStatus.text.indexOf("currently unavailable") >= 0,
            "a rejected store launch at session start did not keep its decision visible")
        if (capturePhase() === "create-rejected") return true

        ShellStore.pendingLaunchParams = null
        ShellStore.openGame({id:"fixture-game",title:"Fixture game",selectedVariantIndex:0,
            variants:[{id:"555",store:"STEAM",libraryStatus:"MANUAL",inLibrary:true,librarySelected:true,
                gfnStatus:"AVAILABLE",playStatus:"PLAYABLE"}]})
        ShellStore.launchSelectedGame(false)
        check(ShellStore.pendingLaunchParams && ShellStore.pendingLaunchParams.storeLaunch === undefined,
            "an ordinary launch inherited the store intent")
        check(ShellStore.pendingLaunchParams.catalogAppId === "fixture-game"
            && String(ShellStore.pendingLaunchParams.variantId) === "555",
            "ordinary launch did not keep its own target")

        AppController.navigate("persistent-storage")
        const reopened = find(host, "persistentStorageStoreLaunch")
        const reopenedStatus = find(host, "persistentStorageStoreLaunchStatus")
        check(reopened && reopenedStatus, "reopened persistent storage lost the store launch action")
        const second = inspectStore()
        client.responseReceived(second.id, inspection("subscription_required",
            "Launching the Steam store for persistent game management requires the GeForce NOW persistent storage add-on."))
        check(ShellStore.storeLaunchTarget === null, "an ineligible store launch was offered")
        check(!reopened.enabled, "ineligible store launch stayed available")
        check(reopenedStatus.text.indexOf("persistent storage add-on") >= 0,
            "ineligible store launch did not explain why")

        const third = inspectStore()
        client.responseReceived(third.id, inspection("ready",
            "Ready to open the Steam store for persistent game management."))
        check(reopened.enabled && ShellStore.storeLaunchTarget.variantId === "12345",
            "the store launch action did not recover after a fresh eligible inspection")

        ShellStore.inspectStoreLaunch()
        const failing = ShellStore.storeLaunchRequestId
        check(failing !== "", "store launch retry did not start an inspection")
        client.requestFailed(failing, "timeout", "The store launch check timed out.")
        check(ShellStore.storeLaunchRequestId === "" && ShellStore.storeLaunchTarget === null
            && ShellStore.storeLaunchFailed, "a failed store launch inspection stayed pending")
        check(reopened.text === "Retry" && reopened.enabled,
            "a failed store launch inspection did not offer a retry")
        check(reopenedStatus.text.indexOf("timed out") >= 0,
            "a failed store launch inspection did not explain the failure")
        const beforeRetry = count("catalog.launch.store.inspect")
        reopened.clicked()
        check(count("catalog.launch.store.inspect") === beforeRetry + 1,
            "the retry action did not send a new inspection")
        client.responseReceived(ShellStore.storeLaunchRequestId, inspection("ready",
            "Ready to open the Steam store for persistent game management."))
        check(ShellStore.storeLaunchTarget !== null && !ShellStore.storeLaunchFailed
            && reopened.text === "Launch Steam",
            "the retry did not recover the store launch target")

        ShellStore.inspectStoreLaunch()
        const inFlight = ShellStore.storeLaunchRequestId
        check(inFlight !== "", "account change fixture did not start an inspection")
        ShellStore.acceptAuthEnvelope({generation: ShellStore.authGeneration + 1,
            session:{user:{userId:"other-user",displayName:"Other"},provider:{idpId:"other-provider",code:"NVIDIA"}}})
        check(ShellStore.storeLaunchTarget === null && !ShellStore.storeLaunchFailed
            && ShellStore.storeLaunchRequestId !== inFlight,
            "an account change retained the previous store launch state")
        client.responseReceived(inFlight, inspection("ready",
            "Ready to open the Steam store for persistent game management."))
        check(ShellStore.storeLaunchTarget === null,
            "a stale store launch response was adopted after the account changed")

        client.state = "starting"
        check(ShellStore.storeLaunchRequestId === "" && ShellStore.storeLaunchTarget === null,
            "a core restart retained store launch state")
        client.state = "ready"
        const restarted = ShellStore.storeLaunchRequestId
        check(restarted !== "", "becoming ready did not trigger the store launch inspection")
        client.responseReceived(restarted, inspection("ready",
            "Ready to open the Steam store for persistent game management."))
        check(ShellStore.storeLaunchTarget !== null,
            "the store launch target did not resolve after the core restarted")
        reopened.forceActiveFocus()
        return true
    }
}
