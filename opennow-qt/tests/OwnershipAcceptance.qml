import QtQuick
import OpenNOW

QtObject {
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property int sequence: 0
        property var requests: []
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "ownership-" + (++sequence)
            requests.push({id:id, method:method, params:params})
            return id
        }
        function cancel(id) { requestFailed(id, "cancelled", "Cancelled"); return true }
    }
    function check(value, message) { if (!value) throw new Error("Ownership: " + message) }
    function game(selected) {
        return {id:"fixture-parent", uuid:"fixture-parent", title:"Store ownership fixture", playabilityState:"PLAYABLE", selectedVariantIndex:selected,
            availableStores:["STEAM", "EPIC"], isInLibrary:true, favorited:false, variants:[
                {id:"123", store:"STEAM", libraryStatus:"MANUAL", inLibrary:true, librarySelected:true, gfnStatus:"AVAILABLE", playStatus:"PLAYABLE"},
                {id:"456", store:"EPIC", libraryStatus:"NOT_OWNED", inLibrary:false, librarySelected:false, gfnStatus:"AVAILABLE", playStatus:"UNKNOWN"}
            ]}
    }
    function request(id) { return client.requests.find(item => item.id === id) }
    function count(method) { return client.requests.filter(item => item.method === method).length }
    function find(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found) return found
        }
        return null
    }
    function replyInspection(status, freshGame) {
        const pending = request(ShellStore.launchInspectRequestId)
        check(pending && pending.method === "catalog.launch.inspect", "missing shared launch inspection")
        client.responseReceived(pending.id, {appId:pending.params.appId, variantId:pending.params.variantId,
            scope:ShellStore.catalogOwnerState.authScope, game:freshGame || ShellStore.selectedGame,
            decision:{status:status,message:status === "ready" ? "Ready to play this store version." : "Ownership of this store version must be confirmed."}})
    }
    function detail(status) {
        const owner = ShellStore.catalogOwnerState
        owner.cancelDetailRequest()
        owner.selectedLaunchDecision = {status:status, message:status === "ownership_required" ? "Confirm that you already own the Epic Games Store license." : "Ready to play this store version."}
    }
    function bindDetail(host) {
        const modal = find(host, "desktopGameModal")
        const primary = find(host, "desktopGamePlay") || find(host, "consolePlayButton")
        check(primary, "production detail action was not created")
        if (modal) modal.game = Qt.binding(function() { return ShellStore.selectedGame })
        else {
            let item = primary
            while (item && item.previewGame === undefined) item = item.parent
            check(item, "production console detail was not created")
            item.previewGame = null
        }
        return primary
    }
    function verifyRendered(host) {
        const primary = find(host, "desktopGamePlay") || find(host, "consolePlayButton")
        const bounds = find(host, "gameDetailsDialog") || host
        check(primary && primary.visible, "rendered primary action is missing")
        const top = primary.mapToItem(bounds, 0, 0)
        const bottom = primary.mapToItem(bounds, primary.width, primary.height)
        check(top.x >= 0 && top.y >= 0 && bottom.x <= bounds.width && bottom.y <= bounds.height,
            "rendered primary action is clipped at this size or scale")
        if (ShellStore.ownershipConfirmation) {
            const confirm = find(host, "cloudOwnershipConfirm")
            check(confirm && confirm.visible, "rendered confirmation action is missing")
            const edge = confirm.mapToItem(host, confirm.width, confirm.height)
            check(edge.x <= host.width && edge.y <= host.height, "confirmation exceeds the window")
        }
        return true
    }
    function run(host) {
        const owner = ShellStore.catalogOwnerState
        ShellStore.settings = Object.assign({}, ShellStore.settings, {onboardingCompleted:true, favoriteGameIds:["retained-home-pin"],
            desktopUiScale:Qt.application.arguments.indexOf("--ownership-scaled") >= 0 ? 1.4 : 1})
        ShellStore.authGeneration = 42
        ShellStore.authSession = {user:{userId:"fixture-user",displayName:"Library fixture"},provider:{idpId:"fixture-provider",code:"NVIDIA"}}
        ShellStore.nativeRuntimeReady = true
        owner.resetCloudActions()
        owner.catalogGames = [game(0)]
        owner.catalogState = "ready"
        owner.catalogComplete = true
        ShellStore.selectedGame = game(1)
        AppController.navigate("game-detail")
        const primary = bindDetail(host)
        detail("ownership_required")
        const initialWrites = count("catalog.ownership.add")
        primary.clicked()
        check(owner.ownershipConfirmation && owner.ownershipConfirmation.variantId === "456", "Own Game did not capture the exact Epic variant")
        check(count("catalog.ownership.add") === initialWrites, "opening confirmation wrote ownership")
        owner.ownershipConfirmation = null
        check(count("catalog.ownership.add") === initialWrites, "cancelling wrote ownership")
        ShellStore.requestOwnershipConfirmation("add")
        ShellStore.selectGameVariant(0)
        ShellStore.confirmOwnership()
        check(count("catalog.ownership.add") === initialWrites, "variant change kept stale confirmation")
        ShellStore.selectGameVariant(1)
        ShellStore.requestOwnershipConfirmation("add")
        const confirm = find(host, "cloudOwnershipConfirm")
        check(confirm, "production ownership confirmation button was not created")
        confirm.clicked()
        const mutation = request(owner.mutationRequestId)
        check(mutation.method === "catalog.ownership.add" && mutation.params.appId === "fixture-parent"
            && mutation.params.variantId === "456" && mutation.params.confirmedExistingLicense === true
            && mutation.params.scope.generation === 42, "ownership mutation target or confirmation was lost")
        const beforeDuplicate = client.requests.length
        ShellStore.confirmOwnership()
        check(client.requests.length === beforeDuplicate, "confirmation sent twice")
        const updated = game(1)
        updated.variants[1] = Object.assign({}, updated.variants[1], {libraryStatus:"MANUAL", inLibrary:true, librarySelected:true, playStatus:"PLAYABLE"})
        client.responseReceived(mutation.id, {appId:"fixture-parent", variantId:"456", scope:owner.authScope,
            outcome:"acknowledged", reconciliation:"confirmed", game:updated, message:"Cloud ownership confirmed."})
        check(owner.mutationState === "confirmed" && ShellStore.selectedGame.variants[1].libraryStatus === "MANUAL", "fresh ownership was not adopted")
        check(count("session.create") === 0 && count("session.remote.list") === 0, "confirming ownership auto-launched")
        ShellStore.selectPreferredVariant()
        const preference = request(owner.mutationRequestId)
        check(preference.method === "catalog.ownership.select" && preference.params.variantId === "456", "preference changed the wrong store")
        client.requestFailed(preference.id, "network_error", "The response was lost")
        check(owner.mutationState === "unconfirmed" && owner.mutationMessage.indexOf("confirm") >= 0, "ambiguous mutation was hidden")
        check(count("catalog.ownership.select") === 1, "ambiguous mutation was retried")
        ShellStore.toggleCloudFavorite(ShellStore.selectedGame)
        const favorite = request(owner.mutationRequestId)
        check(favorite.method === "catalog.favorites.add" && favorite.params.appId === "fixture-parent" && favorite.params.variantId === "", "favorite used a launch ID")
        const pinned = JSON.stringify(ShellStore.settings.favoriteGameIds)
        owner.resetCloudActions()
        client.responseReceived(favorite.id, {appId:"fixture-parent",variantId:"",game:updated,reconciliation:"confirmed"})
        check(owner.mutationState === "idle" && JSON.stringify(ShellStore.settings.favoriteGameIds) === pinned, "stale mutation changed state or Home pins")
        ShellStore.toggleFavorite(ShellStore.selectedGame)
        check(count("catalog.favorites.add") === 1, "Home pin was uploaded as a cloud favorite")
        owner.refreshFavorites()
        client.responseReceived(owner.favoritesRequestId, {games:[updated],sections:[],coverage:"unknown",complete:false,scope:owner.authScope})
        check(owner.remoteFavorites.length === 1 && owner.favoritesState === "partial", "favorites claimed complete coverage")
        owner.refreshFavorites()
        client.requestFailed(owner.favoritesRequestId, "network_error", "Favorites could not be refreshed")
        check(owner.remoteFavorites.length === 1 && owner.favoritesError !== "", "favorite read failure erased last good data")
        owner.adoptGame(Object.assign({}, updated, {favorited:false}))
        check(owner.remoteFavorites.length === 0, "confirmed favorite removal remained in the local remote-favorites view")

        ShellStore.selectedGame = game(1)
        const beforeStops = count("session.stop")
        const beforeCreates = count("session.create")
        ShellStore.activeSession = {sessionId:"existing-seat",appId:"999",status:2}
        ShellStore.launchSelectedGame()
        replyInspection("ownership_required")
        check(count("session.remote.list") === 0 && count("session.stop") === beforeStops && count("session.create") === beforeCreates,
            "unowned launch disturbed the existing game")
        check(ShellStore.activeSession.sessionId === "existing-seat", "failed guard dropped the active seat")
        ShellStore.selectedGame = game(0)
        ShellStore.launchSelectedGame()
        const staleInspection = request(ShellStore.launchInspectRequestId)
        ShellStore.selectGameVariant(1)
        client.responseReceived(staleInspection.id, {appId:"fixture-parent",variantId:"123",scope:owner.authScope,decision:{status:"ready"}})
        check(count("session.remote.list") === 0, "variant change accepted stale ready decision")
        ShellStore.selectedGame = game(0)
        ShellStore.launchSelectedGame()
        replyInspection("ready")
        const discovery = request(ShellStore.remoteSessionsRequestId)
        check(discovery && discovery.params.appId === "123" && discovery.params.catalogAppId === "fixture-parent", "launch boundary conflated parent and variant IDs")
        client.responseReceived(discovery.id, {sessions:[{sessionId:"existing-seat",appId:"999",streamingBaseUrl:"https://fixture.invalid/"}]})
        ShellStore.resolveSessionConflict("new")
        replyInspection("ownership_required")
        check(count("session.stop") === beforeStops && ShellStore.activeSession.sessionId === "existing-seat", "failed replacement guard stopped the existing game")

        ShellStore.activeSession = null
        const staleGame = game(0)
        staleGame.variants[0].inLibrary = false
        ShellStore.selectedGame = staleGame
        ShellStore.launchSelectedGame()
        replyInspection("ready", game(0))
        check(request(ShellStore.remoteSessionsRequestId).params.accountLinked === true, "fresh selected-variant metadata did not replace the stale launch flags")
        client.responseReceived(ShellStore.remoteSessionsRequestId, {sessions:[]})
        check(request(ShellStore.launchInspectRequestId).method === "catalog.launch.inspect", "create continuation bypassed revalidation")
        client.requestFailed(ShellStore.launchInspectRequestId, "upstream_error", "Availability could not be refreshed")
        check(count("session.create") === beforeCreates, "failed create guard allocated a seat")
        ShellStore.createPendingSession()
        replyInspection("ready")
        const created = request(ShellStore.streamCreateRequestId)
        check(created.params.appId === "123" && created.params.variantId === "123" && created.params.scope.generation === 42, "fresh create lost exact selection")
        client.responseReceived(created.id, {session:null})
        ShellStore.pendingLaunchParams = null
        ShellStore.streamState = "idle"
        ShellStore.acceptDirectLaunch("456", "Store ownership fixture")
        const lookup = request(ShellStore.directLookupRequestId)
        check(lookup.method === "catalog.game.get" && lookup.params.variantId === "456", "numeric direct launch did not resolve exact variant")
        client.responseReceived(lookup.id, {game:game(0),scope:owner.authScope})
        check(ShellStore.selectedGame.selectedVariantIndex === 1 && request(ShellStore.launchInspectRequestId).params.variantId === "456", "direct launch silently reused Steam")
        replyInspection("ownership_required")
        check(!ShellStore.gameMatchesDirectLaunch(game(0), {appId:"999",title:"Store ownership fixture"}), "direct ID mismatch fell back to title")
        owner.catalogGames = [game(0)]
        const writesBeforeResume = count("catalog.ownership.add") + count("catalog.ownership.select")
        ShellStore.selectGameForSession({appId:"456"})
        check(ShellStore.selectedGame.selectedVariantIndex === 1 && writesBeforeResume === count("catalog.ownership.add") + count("catalog.ownership.select"),
            "existing-seat selection changed stores or wrote ownership")
        ShellStore.selectedGame = game(1)
        const removedVariant = game(0)
        removedVariant.variants = removedVariant.variants.slice(0, 1)
        owner.adoptGame(removedVariant)
        check(ShellStore.selectedGame.selectedVariantIndex === -1 && ShellStore.selectedLaunchAppId() === "",
            "detail refresh replaced a missing selected variant with another store")

        ShellStore.selectedGame = game(0)
        const createsBeforeOutage = count("session.create")
        ShellStore.launchSelectedGame()
        replyInspection("ready")
        client.requestFailed(ShellStore.remoteSessionsRequestId, "session_discovery_failed", "A region is unavailable")
        check(ShellStore.launchInspectStage === "create" && ShellStore.lastError === "",
            "a regional outage blocked the new launch or bypassed revalidation")
        replyInspection("ready")
        check(count("session.create") === createsBeforeOutage + 1,
            "a new launch did not reach CloudMatch after incomplete discovery")
        client.requestFailed(ShellStore.streamCreateRequestId, "session_conflict", "An existing game is running")
        check(ShellStore.launchConflictDetected, "CloudMatch's existing-session conflict was lost")
        replyInspection("ready")
        client.requestFailed(ShellStore.remoteSessionsRequestId, "session_discovery_failed", "The existing game could not be found")
        check(ShellStore.streamState === "error" && count("session.create") === createsBeforeOutage + 1
            && count("session.stop") === beforeStops,
            "failed conflict discovery created another session or stopped the existing game")
        ShellStore.pendingLaunchParams = null
        ShellStore.launchConflictDetected = false
        ShellStore.streamState = "idle"

        ShellStore.lastError = ""
        owner.mutationMessage = ""
        owner.catalogGames = [game(0)]
        owner.catalogState = "ready"
        ShellStore.selectedGame = game(1)
        detail("ownership_required")
        AppController.navigate("game-detail")
        const currentPrimary = bindDetail(host)
        ShellStore.requestOwnershipConfirmation("add")
        check(currentPrimary.text === "I own this game", "unowned production action was labeled " + currentPrimary.text)
        check(find(host, "cloudOwnershipConfirm"), "final production confirmation is missing")
        if (Qt.application.arguments.indexOf("--ownership-error") >= 0) {
            owner.ownershipConfirmation = null
            owner.mutationState = "unconfirmed"
            owner.mutationMessage = "Could not confirm the update. Refresh this game before trying again; it may already have reached GeForce NOW."
        }
        if (Qt.application.arguments.indexOf("--ownership-scaled") >= 0) DesktopTokens.uiScale = 1.4
        return true
    }
}
