import QtQuick
import OpenNOW
import OpenNOW.Acceptance 1.0

QtObject {
    property AcceptanceCoreClient client: AcceptanceCoreClient {
        property string state: "ready"
        property string lastError: ""
        property int sequence: 0
        property var requests: []
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "push-test-" + (++sequence)
            requests.push({id:id, method:method, params:params})
            return id
        }
        function cancel(id) {
            requestFailed(id, "cancelled", "Cancelled")
            return true
        }
    }
    function check(value, message) { if (!value) throw new Error("Push invalidation: " + message) }
    function namedChild(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = namedChild(child, name)
            if (found) return found
        }
        return null
    }
    function findText(item, wanted) {
        if (item.text !== undefined && String(item.text) === wanted && item.visible !== false) return item
        for (const child of item.children || []) {
            const found = findText(child, wanted)
            if (found) return found
        }
        return null
    }
    function game(id, title) {
        return {id:"app-" + id, uuid:"app-" + id, title:title,
            variants:[{id:String(id + 100), store:"STEAM", inLibrary:true, libraryStatus:"MANUAL"}],
            imageUrl:"", availableStores:["STEAM"], genres:["ACTION"], isInLibrary:true}
    }
    function requestsFor(method) {
        return client.requests.filter(item => item.method === method).length
    }
    function deliverLibraryPage() {
        const owner = ShellStore.catalogOwnerState
        client.responseReceived(owner.catalogRequestId, {games:[game(2001, "Push refreshed game")],
            totalCount:1, hasNextPage:false, nextCursor:"", traversalId:owner.catalogTraversalId,
            catalogRevision:0, catalogContext:"fixture-context"})
        owner.libraryPageTimer.stop()
    }
    function settleCatalog() {
        const owner = ShellStore.catalogOwnerState
        owner.cancelCatalogRequest()
        owner.catalogState = "idle"
        owner.catalogError = ""
    }
    function verifyRouting(host) {
        const owner = ShellStore.catalogOwnerState
        const accounts = ShellStore.accountServicesOwnerState
        settleCatalog()
        ShellStore.authSession = null
        client.eventReceived("account.push.changed", {generation:9, kind:"library"})
        check(client.requests.length === 0, "signed-out hint issued a request")
        ShellStore.authGeneration = 9
        ShellStore.authSession = {user:{userId:"push-fixture",displayName:"Push fixture"},provider:{idpId:"nvidia"}}
        check(ShellStore.signedIn && ShellStore.authGeneration === 9, "fixture sign-in failed")
        settleCatalog()
        const staleRequests = client.requests.length
        client.eventReceived("account.push.changed", {generation:8, kind:"library"})
        check(client.requests.length === staleRequests, "stale generation triggered a refresh")
        client.eventReceived("account.push.changed", {generation:9, kind:"unknown-kind"})
        check(client.requests.length === staleRequests, "unknown kind triggered a refresh")
        client.eventReceived("account.push.changed", {generation:9, kind:"favorites"})
        check(requestsFor("catalog.favorites.list") === 1, "favorites hint did not refresh favorites")
        const beforeSubscription = requestsFor("account.subscription.get")
        client.eventReceived("account.push.changed", {generation:9, kind:"subscription"})
        check(requestsFor("account.subscription.get") === beforeSubscription + 1, "subscription hint did not refresh the subscription")
        const beforeAccounts = requestsFor("account.connections.list")
        client.eventReceived("account.push.changed", {generation:9, kind:"subscription"})
        check(requestsFor("account.connections.list") === beforeAccounts,
            "a repeated hint duplicated an in-flight connections request")
        const pending = client.requests.filter(item => item.method === "account.connections.list")
        client.responseReceived(pending[pending.length - 1].id, {accounts:[],subscriptions:[],definitions:({})})
        client.eventReceived("account.push.changed", {generation:9, kind:"linked-account"})
        check(requestsFor("account.connections.list") === beforeAccounts + 1,
            "linked-account hint did not refresh connections after the in-flight request settled")
        const beforeSyncStatus = requestsFor("account.connections.sync.status")
        client.eventReceived("account.push.changed", {generation:9, kind:"platform-sync"})
        check(requestsFor("account.connections.sync.status") === beforeSyncStatus,
            "platform-sync hint polled without a pending observation")
        accounts.syncOperation = {operationId:"push-sync", provider:"STEAM", phase:"waiting_remote"}
        client.eventReceived("account.push.changed", {generation:9, kind:"platform-sync"})
        check(requestsFor("account.connections.sync.status") === beforeSyncStatus + 1,
            "platform-sync hint did not accelerate the pending observation")
        accounts.syncPollTimer.stop()
        accounts.syncOperation = null
        ShellStore.authSession = null
        client.eventReceived("account.push.changed", {generation:9, kind:"platform-sync"})
        check(requestsFor("account.connections.sync.status") === beforeSyncStatus + 1,
            "signed-out hint issued a request")
    }
    function verifyStreamingLibraryRefresh(host) {
        const owner = ShellStore.catalogOwnerState
        ShellStore.authGeneration = 9
        ShellStore.authSession = {user:{userId:"push-fixture",displayName:"Push fixture"},provider:{idpId:"nvidia"}}
        settleCatalog()
        const streamState = ShellStore.streamState
        const streamerSnapshot = JSON.stringify(ShellStore.streamerDetection)
        const before = requestsFor("catalog.library.list")
        client.eventReceived("account.push.changed", {generation:9, kind:"library"})
        const request = client.requests.find(item => item.id === owner.catalogRequestId)
        check(request && request.method === "catalog.library.list",
            "in-stream library hint did not refresh the library")
        check(requestsFor("catalog.library.list") === before + 1, "library hint issued an unexpected request count")
        deliverLibraryPage()
        check(owner.catalogGames.some(item => item.title === "Push refreshed game"),
            "library refresh did not commit the pushed page")
        check(ShellStore.activeSession === null, "push invalidation created a session")
        check(ShellStore.streamState === streamState, "push invalidation changed the stream state")
        check(JSON.stringify(ShellStore.streamerDetection) === streamerSnapshot,
            "push invalidation changed the native streamer snapshot")
    }
    function run(host) {
        const route = AppController.route
        verifyRouting(host)
        if (route !== "library")
            AppController.navigate("library")
        verifyStreamingLibraryRefresh(host)
        return true
    }
    function verifyRendered(host) {
        check(AppController.route === "library", "visual check ran on route " + AppController.route)
        const owner = ShellStore.catalogOwnerState
        check(ShellStore.catalogGames.some(item => item.title === "Push refreshed game"),
            "the committed catalog lost the pushed page before rendering (state=" + owner.catalogState
                + " request=" + owner.catalogRequestId + " games=" + owner.catalogGames.length
                + " error=" + owner.catalogError
                + " catalogRequests=" + JSON.stringify(client.requests.filter(item => item.method.indexOf("catalog.") === 0))
                + ")")
        const screen = namedChild(host, "desktopLibraryScreen")
        check(screen !== null && screen.visible, "library screen is not rendered")
        const title = findText(screen, "Push refreshed game")
        check(title !== null, "the push-refreshed library did not render the refreshed game")
        return true
    }
    function notify(host) {
        return verifyRendered(host)
    }
}
