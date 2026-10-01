import QtQuick
import OpenNOW

QtObject {
    property QtObject client: QtObject {
        property string state: "ready"
        property string lastError: ""
        property int sequence: 0
        property var requests: []
        property var cancellations: []
        property var cancelledResponse: null
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        signal eventReceived(string name, var payload)
        function markUiReady() {}
        function logShellDiagnostic(message) {}
        function request(method, params, timeout) {
            const id = "catalog-test-" + (++sequence)
            requests.push({id:id, method:method, params:params})
            return id
        }
        function cancel(id) {
            cancellations.push({id:id, catalogRequestId:ShellStore.catalogRequestId})
            requestFailed(id, "cancelled", "Cancelled")
            if (cancelledResponse && cancelledResponse.id === id)
                responseReceived(id, cancelledResponse.page)
            return true
        }
    }
    function check(value, message) { if (!value) throw new Error("Catalog sync: " + message) }
    function namedChild(item, name) {
        if (item.objectName === name) return item
        for (const child of item.children || []) {
            const found = namedChild(child, name)
            if (found) return found
        }
        return null
    }
    function game(id) {
        return {id:"app-" + id, uuid:"app-" + id, title:"Library game " + id,
            variants:[{id:String(id + 100), store:"STEAM", inLibrary:true, libraryStatus:"MANUAL"}],
            imageUrl:"", availableStores:["STEAM"], genres:["ACTION"], isInLibrary:true}
    }
    function deliver(owner, games, cursor, more) {
        client.responseReceived(owner.catalogRequestId, {games:games,totalCount:1300,hasNextPage:more,
            nextCursor:cursor,traversalId:owner.catalogTraversalId,catalogRevision:0,catalogContext:"fixture-context"})
        owner.libraryPageTimer.stop()
    }
    function verifyAccountChangeDuringTraversal(owner, accounts) {
        for (const scenario of [
            {kind:"sync",gap:false}, {kind:"sync",gap:true},
            {kind:"link",gap:false}, {kind:"link",gap:true},
            {kind:"unlink",gap:false}, {kind:"unlink",gap:true}
        ]) {
            const gap = scenario.gap
            owner.reloadCatalogForSession()
            check(owner.catalogGames.length === 0 && !owner.catalogComplete && owner.catalogLastCompleteAt === 0,
                "account reset retained the previous account's library")
            deliver(owner, [game(7000)], "", false)
            const completedAt = owner.catalogLastCompleteAt
            owner.refreshCatalog("")
            const oldId = owner.catalogRequestId
            const oldTraversal = owner.catalogTraversalId
            const oldPage = {games:[game(8000)],totalCount:2,hasNextPage:true,nextCursor:"old-next",
                traversalId:oldTraversal,catalogRevision:0,catalogContext:"fixture-context"}
            if (gap) {
                client.responseReceived(oldId, oldPage)
                check(owner.libraryPageTimer.running && owner.catalogRequestId === "", "missing interpage sync fixture")
            }
            client.cancelledResponse = {id:oldId,page:oldPage}
            if (scenario.kind === "sync") {
                accounts.syncGameAccount("STEAM")
                client.responseReceived(accounts.gameAccountActionRequestId, {operationId:"sync-race-" + gap,provider:"STEAM",phase:"waiting_remote"})
                accounts.syncPollTimer.stop()
                accounts.pollSync()
                client.responseReceived(accounts.syncStatusRequestId, {operationId:"sync-race-" + gap,provider:"STEAM",phase:"refreshing_library"})
            } else if (scenario.kind === "link") {
                accounts.accountLinkAttempt = {attemptId:"link-race-" + gap}
                accounts.pollAccountLink()
                client.responseReceived(accounts.accountLinkPollRequestId, {status:"complete"})
            } else {
                accounts.unlinkGameAccount("STEAM")
                client.responseReceived(accounts.gameAccountActionRequestId, {message:"Disconnected"})
            }
            const freshId = owner.catalogRequestId
            check(freshId !== "" && freshId !== oldId && owner.catalogTraversalId !== oldTraversal,
                scenario.kind + " completion did not replace the invalidated traversal")
            check(!owner.libraryPageTimer.running && owner.catalogNextCursor === "" && owner.catalogStaged.length === 0,
                "sync completion retained an old cursor, staged page, or timer")
            check(owner.catalogGames.length === 1 && owner.catalogGames[0].id === "app-7000"
                && owner.catalogLastCompleteAt === completedAt, "sync refresh discarded the last complete library")
            if (!gap) {
                const cancellation = client.cancellations.find(item => item.id === oldId)
                check(cancellation && cancellation.catalogRequestId === "", "old request ownership survived synchronous cancellation")
            }
            client.cancelledResponse = null
            client.responseReceived(oldId, oldPage)
            client.requestFailed(oldId, "catalog_changed", "Old revision was invalidated")
            owner.libraryPageTimer.triggered()
            check(owner.catalogRequestId === freshId && owner.catalogStaged.length === 0 && owner.catalogError === "",
                "late old completion, failure, or timer replaced the new traversal")
            if (scenario.kind === "sync")
                check(accounts.syncOperation.phase === "refreshing_library" && accounts.gameAccountMessage.indexOf("incomplete") < 0,
                    "synchronous cancellation stranded sync completion")
            deliver(owner, [game(9000)], "new-next", true)
            if (scenario.kind === "sync")
                check(accounts.syncOperation.phase === "refreshing_library", "sync completed before the final library page")
            owner.libraryPageTimer.triggered()
            deliver(owner, [game(9001)], "", false)
            check(accounts.syncOperation === null && owner.catalogGames.length === 2
                && owner.catalogGames[0].id === "app-9000", "replacement traversal did not finish sync")
        }
    }
    function verifyDirectLaunchPaging(owner) {
        ShellStore.settings = Object.assign({}, ShellStore.settings, {onboardingCompleted:true})
        owner.reloadCatalogForSession()
        ShellStore.acceptDirectLaunch("missing-fixture-app", "Missing fixture game")
        check(ShellStore.pendingDirectLaunch !== null && !ShellStore.onboardingRequired, "pending direct launch fixture is blocked")
        const traversal = owner.catalogTraversalId
        for (let page = 0; page < 3; ++page) {
            const request = client.requests.find(item => item.id === owner.catalogRequestId)
            check(request.params.traversalId === traversal && request.params.cursor === (page ? "launch-" + (page - 1) : ""),
                "direct launch restarted the traversal instead of continuing its cursor")
            deliver(owner, [game(page)], "launch-" + page, page < 2)
            if (page < 2) {
                check(owner.catalogTraversalId === traversal && owner.catalogRequestId === "" && owner.catalogStaged.length === page + 1
                    && ShellStore.pendingDirectLaunch !== null,
                    "direct launch restarted the traversal in the interpage gap")
                owner.libraryPageTimer.triggered()
            }
        }
        check(owner.catalogGames.length === 3 && owner.catalogComplete && ShellStore.pendingDirectLaunch === null,
            "direct launch resolved before the complete multi-page library")
        owner.refreshCatalog("")
        ShellStore.acceptDirectLaunch("missing-fixture-app", "Missing fixture game")
        const pausedTraversal = owner.catalogTraversalId
        owner.catalogSliceStarted = Date.now() - 31000
        deliver(owner, [game(4)], "paused-next", true)
        check(owner.catalogState === "partial" && owner.catalogNextCursor === "paused-next" && owner.catalogTraversalId === pausedTraversal,
            "pending direct launch restarted a bounded paused traversal")
        owner.continueCatalog()
        check(client.requests.find(item => item.id === owner.catalogRequestId).params.cursor === "paused-next",
            "direct-launch continuation did not resume the bounded cursor")
        client.requestFailed(owner.catalogRequestId, "upstream_error", "Fixture HTTP 503")
        const count = client.requests.length
        for (let attempt = 0; attempt < 3; ++attempt) ShellStore.resolveDirectLaunch()
        check(client.requests.length === count && owner.catalogError.indexOf("503") >= 0,
            "pending direct launch automatically retried a failed library page")
        ShellStore.pendingDirectLaunch = null
        owner.continueCatalog()
        deliver(owner, [game(5)], "", false)
        owner.reloadCatalogForSession()
        ShellStore.acceptDirectLaunch("missing-fixture-app", "Missing fixture game")
        client.requestFailed(owner.catalogRequestId, "upstream_error", "Fixture first-page failure")
        const failedCount = client.requests.length
        for (let attempt = 0; attempt < 3; ++attempt) ShellStore.resolveDirectLaunch()
        check(owner.catalogState === "error" && client.requests.length === failedCount,
            "pending direct launch created a first-page error retry loop")
        ShellStore.pendingDirectLaunch = null
    }
    function verifyAccountLinkReplacement(accounts) {
        accounts.startAccountLink("UPLAY")
        client.responseReceived(accounts.accountLinkStartRequestId,
            {attemptId:"first-link",provider:"UPLAY",loginUrl:""})
        accounts.accountLinkPollTimer.stop()
        accounts.pollAccountLink()
        const oldPoll = accounts.accountLinkPollRequestId
        accounts.startAccountLink("EPIC")
        client.responseReceived(accounts.accountLinkStartRequestId,
            {attemptId:"replacement-link",provider:"EPIC",loginUrl:""})
        accounts.accountLinkPollTimer.stop()
        client.responseReceived(oldPoll, {status:"complete"})
        check(accounts.accountLinkAttempt && accounts.accountLinkAttempt.attemptId === "replacement-link",
            "old link completion discarded the replacement sign-in")
        check(client.cancellations.some(item => item.id === oldPoll), "replacement retained the old link poll")
        accounts.pollAccountLink()
        const newPoll = accounts.accountLinkPollRequestId
        check(client.requests.find(item => item.id === newPoll).params.attemptId === "replacement-link",
            "replacement sign-in polled the old attempt")
        client.requestFailed(oldPoll, "link_attempt_not_found", "Old attempt expired")
        check(accounts.accountLinkPollRequestId === newPoll && accounts.accountLinkAttempt !== null,
            "old link failure stopped the replacement sign-in")
        client.responseReceived(newPoll, {status:"complete"})
        check(accounts.accountLinkAttempt === null && !accounts.accountLinkPollTimer.running,
            "replacement sign-in did not finish")
    }
    function verifyStoreActions(host, accounts) {
        const stores = namedChild(host, "desktopStoresSettings")
        if (!stores) return
        const fixtures = [
            {provider:"STEAM",label:"Fixture Steam",isConnected:true,status:"connected",supportsSync:true,supportsLinking:false},
            {provider:"UPLAY",label:"Fixture Ubisoft",isConnected:false,status:"not_connected",supportsSync:true,supportsLinking:true}
        ]
        accounts.refreshGameAccounts()
        client.responseReceived(accounts.gameAccountsRequestId, {accounts:fixtures,subscriptions:[],definitions:{}})
        for (const account of fixtures) {
            const button = storeActionButton(stores, account.label)
            check(button && button.enabled, "supported store action is disabled or missing")
            button.clicked()
            const id = account.provider === "STEAM" ? accounts.gameAccountActionRequestId : accounts.accountLinkStartRequestId
            const request = client.requests.find(item => item.id === id)
            check(request && request.method === (account.provider === "STEAM" ? "account.connections.sync" : "account.connections.link.start")
                && request.params.provider === account.provider, "store button did not dispatch its core RPC")
            const notice = namedChild(stores, "storeSyncNotice")
            check(notice.text === accounts.gameAccountMessage && notice.text !== "", "store action did not display progress")
            client.requestFailed(id, "upstream_error", "Fixture account action failure")
            check(notice.text === "Fixture account action failure", "store action failure was not displayed")
            button.clicked()
            const retry = account.provider === "STEAM" ? accounts.gameAccountActionRequestId : accounts.accountLinkStartRequestId
            check(retry !== "" && retry !== id, "store action could not retry after a core failure")
            client.requestFailed(retry, "upstream_error", "Fixture retry failure")
        }
    }
    function storeActionButton(item, label) {
        if (item.title === label) {
            for (const child of item.trailing || [])
                if (child.clicked !== undefined && child.text === item.status.action) return child
        }
        for (const child of item.children || []) {
            const found = storeActionButton(child, label)
            if (found) return found
        }
        return null
    }
    function run(host) {
        const route = AppController.route
        ShellStore.authSession = {user:{userId:"catalog-fixture",displayName:"Catalog fixture"},provider:{idpId:"fixture"}}
        const owner = ShellStore.catalogOwnerState
        const accounts = ShellStore.accountServicesOwnerState
        verifyAccountLinkReplacement(accounts)
        verifyStoreActions(host, accounts)
        verifyAccountChangeDuringTraversal(owner, accounts)
        verifyDirectLaunchPaging(owner)
        ShellStore.lastError = ""
        AppController.navigate(route)
        check(accounts.gameAccountAction({provider:"STEAM",isConnected:false,supportsSync:true,supportsLinking:false}) === "sync", "first Steam sync was routed to unsupported linking")
        check(accounts.gameAccountAction({provider:"UPLAY",isConnected:true,supportsSync:true,supportsLinking:true,syncState:"SYNC_DENIED"}) === "link", "denied authorization did not offer reconnect")
        check(accounts.gameAccountAction({provider:"NEW_STORE",isConnected:false,supportsSync:false,supportsLinking:false}) === "none", "unknown store advertised an action")
        owner.reloadCatalogForSession()
        for (let page = 0; page < 13; ++page) {
            if (page) owner.requestLibraryPage()
            const request = client.requests.find(item => item.id === owner.catalogRequestId)
            check(request.method === "catalog.library.list" && request.params.limit === 100, "library page contract")
            const games = []
            for (let index = 0; index < 100; ++index) games.push(game(page * 100 + index))
            deliver(owner, games, "cursor-" + page, page < 12)
            check(owner.catalogComplete === (page === 12), "partial page claimed completion")
        }
        check(owner.catalogGames.length === 1300, "thousand-game ceiling remains")
        const completedAt = owner.catalogLastCompleteAt
        owner.refreshCatalog("")
        deliver(owner, [game(9000)], "refresh-two", true)
        check(owner.catalogGames.length === 1300, "refresh replaced the last complete snapshot early")
        owner.requestLibraryPage()
        client.requestFailed(owner.catalogRequestId, "upstream_error", "Fixture HTTP 503")
        check(owner.catalogGames.length === 1300 && owner.catalogError.indexOf("503") >= 0, "later failure lost usable data or error")
        owner.continueCatalog()
        check(client.requests.find(item => item.id === owner.catalogRequestId).params.cursor === "refresh-two", "retry skipped the failed page")
        deliver(owner, [game(9001)], "", false)
        check(owner.catalogGames.length === 2 && owner.catalogLastCompleteAt >= completedAt, "validated end did not commit removals")
        owner.refreshCatalog("")
        deliver(owner, [game(1)], "cycle-a", true)
        owner.requestLibraryPage()
        deliver(owner, [game(2)], "cycle-b", true)
        owner.requestLibraryPage()
        deliver(owner, [game(3)], "cycle-a", true)
        check(owner.catalogState === "partial" && owner.catalogNextCursor === "", "nonadjacent cursor cycle was accepted")
        owner.refreshCatalog("")
        owner.catalogSliceStarted = Date.now() - 31000
        deliver(owner, [game(1)], "slice-two", true)
        check(owner.catalogState === "partial" && owner.catalogNextCursor === "slice-two", "foreground slice was not resumable")
        owner.continueCatalog()
        deliver(owner, [game(2)], "", false)
        owner.refreshCatalog("")
        deliver(owner, [game(1)], "__proto__", true)
        check(owner.catalogNextCursor === "__proto__" && owner.catalogError === "", "opaque cursor collided with a JavaScript property")
        owner.requestLibraryPage()
        deliver(owner, [game(2)], "", false)
        accounts.syncGameAccount("STEAM")
        const before = client.requests.filter(item => item.method === "catalog.library.list").length
        client.responseReceived(accounts.gameAccountActionRequestId, {operationId:"sync-one",provider:"STEAM",phase:"waiting_remote"})
        check(client.requests.filter(item => item.method === "catalog.library.list").length === before, "202 started the final refresh")
        accounts.syncPollTimer.stop()
        accounts.pollSync()
        client.responseReceived(accounts.syncStatusRequestId, {operationId:"sync-one",provider:"STEAM",phase:"refreshing_library"})
        check(accounts.syncOperation.phase === "refreshing_library", "remote completion skipped refresh phase")
        deliver(owner, [game(1),game(2)], "", false)
        check(accounts.syncOperation === null && accounts.gameAccountMessage.indexOf("completed") >= 0, "complete traversal did not finish sync")
        accounts.syncGameAccount("STEAM")
        client.responseReceived(accounts.gameAccountActionRequestId, {operationId:"sync-two",provider:"STEAM",phase:"waiting_remote"})
        accounts.cancelSyncObservation()
        check(accounts.syncOperation === null && !accounts.syncPollTimer.running, "cancel kept observation alive")
        accounts.invalidateAccount()
        check(accounts.syncStatusRequestId === "" && accounts.gameAccounts.length === 0, "account reset retained state")
        owner.refreshCatalog("")
        owner.catalogStagedBytes = 32 * 1024 * 1024
        deliver(owner, [game(4)], "budget", true)
        check(owner.catalogState === "partial" && owner.catalogNextCursor === "", "resource budget claimed completion")
        owner.catalogGames = [game(1),game(2),game(3),game(4)]
        owner.catalogError = qsTr("Store sync finished, but the library refresh is incomplete. Your saved games are still shown. Retry to load the remaining pages.")
        owner.catalogState = "partial"
        accounts.gameAccountMessage = qsTr("Store sync accepted. Waiting for the store to finish…")
        accounts.syncOperation = {operationId:"visual-sync",provider:"STEAM",phase:"waiting_remote"}
        accounts.gameAccounts = [{provider:"STEAM",label:"Steam",isConnected:true,supportsSync:true,supportsLinking:false,status:"connected",syncedGames:4}]
        if (AppController.route === "game-detail") {
            const patched = game(1)
            patched.variants[0].gfnStatus = "PATCHING"
            patched.variants[0].playStatus = "NOT_PLAYABLE"
            patched.variants[0].stateDetails = {__typename:"VariantGfnAutoPatchingMetadata",historicalEtaMins:18}
            ShellStore.selectedGame = patched
            check(ShellStore.readinessNotice(patched).indexOf("not a completion time") >= 0, "patch estimate was not qualified")
            const modal = namedChild(host, "desktopGameModal")
            check(modal !== null, "production game detail is missing")
            modal.game = patched
            const notice = namedChild(modal, "catalogReadinessNotice")
            check(notice !== null && notice.visible && notice.text.indexOf("not a completion time") >= 0, "production detail did not render the patch notice")
        }
        check(ShellStore.activeSession === null, "catalog work created a session")
        return true
    }
}
