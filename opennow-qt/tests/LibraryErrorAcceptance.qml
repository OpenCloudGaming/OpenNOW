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
            const id = "library-error-test-" + (++sequence)
            requests.push({id: id, method: method, params: params})
            return id
        }
        function cancel(id) { requestFailed(id, "cancelled", "Cancelled"); return true }
    }

    function check(condition, message) {
        if (!condition)
            throw new Error("Library error acceptance: " + message)
    }

    function find(item, name) {
        if (item.objectName === name)
            return item
        for (const child of item.children || []) {
            const found = find(child, name)
            if (found)
                return found
        }
        return null
    }

    function run(root) {
        const status = find(root, "libraryErrorText")
        const retry = find(root, "libraryRetry")
        const grid = find(root, "libraryGameGrid")
        check(status && retry && grid, "Library status controls missing")
        const games = ["First game", "Second game", "Third game"].map((title, index) => ({
            id: "library-game-" + index, uuid: "library-game-" + index, title: title,
            imageUrl: "qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg",
            heroImageUrl: "qrc:/qt/qml/OpenNOW/res/brand/desktop-renew.jpg",
            availableStores: ["Steam"], genres: [],
            variants: [{id: "library-game-" + index, store: "Steam", inLibrary: true}]
        }))
        ShellStore.authSession = {user: {userId: "library-acceptance", displayName: "Library Test"}}
        ShellStore.reloadCatalogForSession()
        ShellStore.refreshCatalog("persistent query")
        client.requestFailed(ShellStore.catalogRequestId, "upstream_error", "Library HTTP 503")
        check(status.visible && retry.visible && status.text.indexOf("HTTP 503") >= 0,
            "Empty library failure hidden")
        retry.clicked()
        const request = client.requests.find(value => value.id === ShellStore.catalogRequestId)
        check(request && request.params.cursor === "" && request.params.searchQuery === "persistent query",
            "Retry did not preserve the query and restart pagination")
        client.responseReceived(ShellStore.catalogRequestId, {
            games: games, totalCount: 1200, hasNextPage: true, nextCursor: "page-one"
        })
        check(!status.visible && ShellStore.catalogGames.length === 3, "Recovery did not display games")
        ShellStore.catalogOwnerState.requestCatalogPage()
        client.responseReceived(ShellStore.catalogRequestId, {
            games: [], totalCount: 1200, hasNextPage: true, nextCursor: "page-one"
        })
        check(ShellStore.catalogState === "error" && status.visible && retry.visible,
            "Partial library protocol failure hidden")
        check(status.text.indexOf("Loaded 3 games.") >= 0 && status.text.indexOf("invalid page") >= 0,
            "Partial progress and error are not both visible")
        check(grid.visible && grid.count === 3 && find(root, "libraryGameGrid") === grid,
            "Error replaced the game grid")
        retry.clicked()
        check(!status.visible && ShellStore.catalogGames.length === 3, "Retry discarded partial games")
        client.requestFailed(ShellStore.catalogRequestId, "upstream_error", "Library HTTP 503. Try again to load the remaining games.")
        check(status.visible && retry.enabled && grid.count === 3,
            "Partial upstream failure is not recoverable")
        return true
    }
}
