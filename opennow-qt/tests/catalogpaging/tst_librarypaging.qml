import QtQuick
import QtTest
import "../../qml/state/catalog"

TestCase {
    id: test
    name: "LibraryPaging"
    property string lastError: ""
    property var client: QtObject {
        property int sequence: 0
        property var requests: []
        property var cancelled: []
        property bool rejectRequest: false
        signal responseReceived(string requestId, var result)
        signal requestFailed(string requestId, string code, string message)
        function request(method, params, timeout) {
            if (rejectRequest)
                return ""
            const id = "library-test-" + (++sequence)
            requests.push({id: id, method: method, params: params, timeout: timeout})
            return id
        }
        function cancel(id) {
            cancelled.push(id)
            requestFailed(id, "cancelled", "Cancelled")
            return true
        }
    }
    property CatalogState catalog: CatalogState {
        coreClient: test.client
        appController: null
        ready: false
        signedIn: true
        settings: ({})
        setSetting: function() { return "" }
        applySetting: function() {}
        onErrorReported: message => test.lastError = message
    }
    property Connections responses: Connections {
        target: test.client
        function onResponseReceived(requestId, result) {
            if (requestId !== "" && requestId === test.catalog.catalogRequestId)
                test.catalog.acceptCatalog(result)
        }
        function onRequestFailed(requestId, code, message) {
            if (requestId !== "" && requestId === test.catalog.catalogRequestId)
                test.catalog.failCatalog(message)
        }
    }

    function init() {
        catalog.ready = false
        catalog.resetCatalog()
        catalog.signedIn = true
        client.requests = []
        client.cancelled = []
        client.rejectRequest = false
        lastError = ""
        catalog.ready = true
    }

    function cleanup() {
        catalog.ready = false
    }

    function game(id) {
        return {id: id, uuid: id, title: "Game " + id}
    }

    function page(games, cursor, more) {
        return {games: games, count: games.length, totalCount: 15000,
            source: "account-library", fetchedAt: "2026-09-14T00:00:00Z",
            nextCursor: cursor, hasNextPage: more}
    }

    function respond(result) {
        verify(catalog.catalogRequestId !== "", "No active request")
        client.responseReceived(catalog.catalogRequestId, result)
    }

    function nextRequest() {
        tryVerify(function() { return catalog.catalogRequestId !== "" }, 1000)
        return client.requests[client.requests.length - 1]
    }

    function test_largeLibraryIsProgressiveAndDeduplicated() {
        catalog.refreshCatalog("persistent query")
        for (let index = 0; index < 12; ++index) {
            const request = nextRequest()
            compare(request.method, "catalog.library.list")
            compare(request.params.limit, 100)
            compare(request.params.searchQuery, "persistent query")
            compare(request.params.cursor, index === 0 ? "" : "page-" + index)
            const games = [game("shared")]
            for (let row = 0; row < 99; ++row)
                games.push(game("game-" + index + "-" + row))
            const count = client.requests.length
            respond(page(games, "page-" + (index + 1), index < 11))
            compare(client.requests.length, count, "Continuation must not recurse synchronously")
            compare(catalog.catalogGames.length, 1 + (index + 1) * 99)
            compare(catalog.catalogState, index < 11 ? "loading" : "ready")
            compare(catalog.catalogLoading, index < 11)
            if (index < 11) {
                catalog.refreshCatalog("persistent query")
                compare(client.requests.length, count, "Refresh during timer gap restarted the walk")
            }
        }
        compare(catalog.catalogGames.length, 1189)
    }

    function test_emptyFilteredPagesContinue() {
        catalog.refreshCatalog("rare match")
        respond(page([], "empty-one", true))
        compare(catalog.catalogGames.length, 0)
        compare(catalog.catalogState, "loading")
        compare(nextRequest().params.searchQuery, "rare match")
        respond(page([], "empty-two", true))
        compare(nextRequest().params.cursor, "empty-two")
        respond(page([game("match")], "", false))
        compare(catalog.catalogState, "ready")
        compare(catalog.catalogGames[0].id, "match")
        verify(!catalog.catalogLoading)
    }

    function test_invalidPage_data() {
        return [
            {tag: "missing-cursor", result: {games: [], hasNextPage: true}},
            {tag: "empty-cursor", result: page([], "", true)},
            {tag: "whitespace-cursor", result: page([], "  ", true)},
            {tag: "numeric-cursor", result: page([], 12, true)},
            {tag: "missing-more", result: {games: [], nextCursor: ""}},
            {tag: "string-more", result: page([], "next", "true")},
            {tag: "invalid-games", result: page({}, "", false)},
            {tag: "oversized-page", result: page(Array(101).fill(game("too-many")), "", false)}
        ]
    }

    function test_invalidPage(data) {
        catalog.refreshCatalog("")
        respond(data.result)
        compare(catalog.catalogState, "error")
        verify(catalog.catalogError.indexOf("invalid page") >= 0)
        compare(lastError, catalog.catalogError)
        verify(!catalog.catalogLoading)
        wait(30)
        compare(client.requests.length, 1)
    }

    function test_cyclicCursorRetainsPartialGamesAndReportsError() {
        catalog.refreshCatalog("")
        respond(page([game("first")], "one", true))
        nextRequest()
        respond(page([game("second")], "two", true))
        nextRequest()
        respond(page([game("third")], "one", true))
        compare(catalog.catalogState, "error")
        compare(catalog.catalogGames.length, 2)
        verify(lastError.length > 0)
        verify(!catalog.catalogLoading)
    }

    function test_opaqueCursorsAndIdentitiesDoNotCollideWithObjectPrototype() {
        catalog.refreshCatalog("")
        respond(page([game("__proto__")], "__proto__", true))
        nextRequest()
        respond(page([game("constructor")], "constructor", true))
        nextRequest()
        respond(page([game("toString")], "", false))
        compare(catalog.catalogGames.length, 3)
        compare(catalog.catalogState, "ready")
    }

    function test_pageLimit_data() {
        return [{tag: "exact-limit", more: false}, {tag: "over-limit", more: true}]
    }

    function test_pageLimit(data) {
        catalog.refreshCatalog("")
        for (let index = 0; index < 150; ++index) {
            nextRequest()
            respond(page([game("game-" + index)], "cursor-" + index, index < 149 || data.more))
        }
        compare(client.requests.length, 150)
        compare(catalog.catalogGames.length, 150)
        compare(catalog.catalogState, data.more ? "error" : "ready")
        compare(catalog.catalogError.indexOf("page limit") >= 0, data.more)
        verify(!catalog.catalogLoading)
        wait(30)
        compare(client.requests.length, 150)
    }

    function test_failureThenRetryReplacesSnapshot() {
        catalog.refreshCatalog("query")
        respond(page([game("old")], "next", true))
        const failed = nextRequest()
        client.requestFailed(failed.id, "upstream_error", "Library HTTP 503")
        compare(catalog.catalogGames.length, 1)
        compare(catalog.catalogState, "error")
        compare(catalog.catalogError, "Library HTTP 503")
        compare(lastError, "Library HTTP 503")
        verify(!catalog.catalogLoading)
        catalog.retryCatalog()
        compare(catalog.catalogState, "refreshing")
        compare(catalog.catalogError, "")
        compare(nextRequest().params.cursor, "")
        compare(nextRequest().params.searchQuery, "query")
        respond(page([game("new")], "", false))
        compare(catalog.catalogGames.length, 1)
        compare(catalog.catalogGames[0].id, "new")
        compare(catalog.catalogState, "ready")
    }

    function test_requestCannotStart() {
        client.rejectRequest = true
        catalog.refreshCatalog("")
        compare(catalog.catalogState, "error")
        verify(catalog.catalogError.indexOf("Could not start") >= 0)
        verify(!catalog.catalogLoading)
    }

    function test_newSearchCancelsObsoleteRequest() {
        catalog.refreshCatalog("old")
        const old = catalog.catalogRequestId
        catalog.refreshCatalog("new")
        compare(client.cancelled[0], old)
        const current = catalog.catalogRequestId
        client.responseReceived(old, page([game("stale")], "obsolete", true))
        client.requestFailed(old, "network", "Stale error")
        compare(catalog.catalogRequestId, current)
        compare(catalog.catalogError, "")
        compare(catalog.catalogGames.length, 0)
        respond(page([game("new")], "", false))
        compare(catalog.catalogGames[0].id, "new")
    }

    function test_accountResetDuringTimerGap() {
        catalog.refreshCatalog("account A")
        respond(page([game("private")], "account-a", true))
        catalog.reloadCatalogForSession()
        compare(catalog.catalogGames.length, 0)
        compare(catalog.catalogTotalCount, 0)
        compare(catalog.selectedGame, null)
        compare(nextRequest().params.cursor, "")
        compare(nextRequest().params.searchQuery, "")
        respond(page([game("account-b")], "", false))
        wait(30)
        compare(client.requests.length, 2)
        compare(catalog.catalogGames[0].id, "account-b")
    }

    function test_disconnectCancelsAndClearsAccountData_data() {
        return [{tag: "in-flight", timerGap: false}, {tag: "timer-gap", timerGap: true}]
    }

    function test_disconnectCancelsAndClearsAccountData(data) {
        catalog.refreshCatalog("account")
        respond(page([game("private")], "next", true))
        if (!data.timerGap)
            nextRequest()
        const old = catalog.catalogRequestId
        const count = client.requests.length
        catalog.ready = false
        compare(catalog.catalogGames.length, 0)
        compare(catalog.catalogTotalCount, 0)
        compare(catalog.selectedGame, null)
        compare(catalog.catalogState, "idle")
        verify(!catalog.catalogLoading)
        if (old !== "") {
            verify(client.cancelled.indexOf(old) >= 0)
            client.responseReceived(old, page([game("stale")], "", false))
        }
        wait(30)
        compare(client.requests.length, count)
        catalog.ready = true
        catalog.refreshCatalog("")
        compare(nextRequest().params.cursor, "")
        respond(page([game("restored")], "", false))
        compare(catalog.catalogGames[0].id, "restored")
    }

    function test_signOutStopsContinuationAndPublicRemainsOneShot() {
        catalog.refreshCatalog("")
        respond(page([game("private")], "next", true))
        catalog.signedIn = false
        verify(!catalog.catalogLoading)
        compare(catalog.catalogGames.length, 0)
        catalog.refreshCatalog("public query")
        const request = nextRequest()
        compare(request.method, "catalog.public.list")
        compare(request.params.limit, 360)
        compare(request.params.searchQuery, "public query")
        verify(request.params.cursor === undefined)
        respond({games: [game("public")], totalCount: 360})
        compare(catalog.catalogState, "ready")
        wait(30)
        compare(client.requests.length, 2)
        catalog.refreshCatalog("")
        compare(catalog.catalogState, "refreshing")
        compare(catalog.catalogGames[0].id, "public")
    }

    function test_publicSnapshotSurvivesDisconnect() {
        catalog.signedIn = false
        catalog.refreshCatalog("")
        respond({games: [game("public")], totalCount: 360})
        catalog.ready = false
        compare(catalog.catalogGames[0].id, "public")
        compare(catalog.catalogState, "ready")
        catalog.ready = true
        catalog.refreshCatalog("")
        compare(catalog.catalogState, "refreshing")
        compare(catalog.catalogGames[0].id, "public")
        respond({games: [game("fresh-public")], totalCount: 360})
        compare(catalog.catalogGames[0].id, "fresh-public")
    }
}
