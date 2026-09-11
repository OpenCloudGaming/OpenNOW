import QtQuick
import OpenNOW

QtObject {
    id: test
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
            const id = "collections-test-" + (++sequence)
            requests.push({id: id, method: method, params: params})
            return id
        }
        function cancel(id) { requestFailed(id, "cancelled", "Cancelled"); return true }
    }

    function check(condition, message) {
        if (!condition)
            throw new Error("Collections acceptance: " + message)
    }

    function find(item, name) {
        if (item.objectName === name)
            return item
        const children = item.children || []
        for (let i = 0; i < children.length; ++i) {
            const found = find(children[i], name)
            if (found)
                return found
        }
        return null
    }

    function complete() {
        const request = client.requests[client.requests.length - 1]
        check(request.method === "settings.set" && request.params.key === "gameCollections", "wrong persistence boundary")
        client.responseReceived(request.id, {key: "gameCollections", value: request.params.value})
        client.eventReceived("settings.changed", {key: "gameCollections", value: request.params.value})
        check(!ShellStore.collectionsBusy, "save did not finish")
    }

    function run(item) {
        const screen = find(item, "desktopLibraryScreen")
        check(screen !== null, "library screen not rendered")
        const originalGames = ShellStore.catalogGames.slice()
        ShellStore.settings = {gameCollections: [], hiddenGameIds: [], favoriteGameIds: [], desktopRailCollapsed: false,
            appTheme: ShellStore.settings.appTheme || "dark"}
        ShellStore.catalogGames = [
            {uuid: "game-a", id: "variant-a", title: "First game", availableStores: ["Steam"]},
            {id: "game-b", title: "Second game", availableStores: ["Epic"]},
            {launchAppId: "game-c", title: "Third game", availableStores: ["Steam"]}
        ]
        check(ShellStore.gameCollections.length === 0 && screen.games.length === 3, "default collections must be empty")
        check(!ShellStore.createCollection("  ", null) && ShellStore.collectionError !== "", "blank name accepted")
        check(!ShellStore.createCollection("x".repeat(81), null), "long name accepted")
        check(ShellStore.createCollection("  Weekend games  ", null), "create rejected")
        check(ShellStore.collectionsBusy && ShellStore.gameCollections.length === 0, "unconfirmed write appeared saved")
        const before = client.requests.length
        check(!ShellStore.createCollection("Concurrent", null) && client.requests.length === before, "concurrent full-list write dispatched")
        complete()
        const first = ShellStore.gameCollections[0].id
        check(ShellStore.gameCollections[0].name === "Weekend games", "name not trimmed")
        check(!ShellStore.createCollection("WEEKEND GAMES", null), "duplicate name accepted")
        ShellStore.activeCollectionId = first
        check(screen.games.length === 0, "empty folder leaked catalog games")
        check(ShellStore.toggleCollectionGame(first, ShellStore.catalogGames[0]), "add rejected")
        complete()
        check(screen.games.length === 1 && screen.games[0].uuid === "game-a", "membership did not reactively filter")
        check(ShellStore.isInCollection({uuid: "game-a", id: "other-variant"}, first), "membership did not use canonical identity")
        check(ShellStore.createCollection("With friends", ShellStore.catalogGames[0]), "create with game rejected")
        complete()
        const second = ShellStore.gameCollections[1].id
        check(ShellStore.isInCollection(ShellStore.catalogGames[0], first)
            && ShellStore.isInCollection(ShellStore.catalogGames[0], second), "multiple membership lost")
        check(ShellStore.renameCollection(first, "Slow Sundays"), "rename rejected")
        complete()
        check(screen.collection.name === "Slow Sundays" && screen.games.length === 1, "rename lost identity or games")
        check(ShellStore.toggleCollectionGame(first, ShellStore.catalogGames[0]), "remove rejected")
        complete()
        check(screen.games.length === 0 && ShellStore.isInCollection(ShellStore.catalogGames[0], second), "remove affected another collection")
        check(ShellStore.toggleCollectionGame(first, ShellStore.catalogGames[1]), "second game rejected")
        complete()
        screen.searchQuery = "First"
        check(screen.games.length === 0, "search ignored collection")
        screen.searchQuery = ""
        screen.activeFilter = "steam"
        check(screen.games.length === 0, "store filter ignored collection")
        screen.activeFilter = "all"
        ShellStore.applySetting("hiddenGameIds", ["game-b"])
        check(screen.games.length === 0, "hidden game leaked into collection")
        screen.activeFilter = "hidden"
        check(screen.games.length === 1, "hidden collection member cannot be recovered")
        ShellStore.applySetting("hiddenGameIds", [])
        screen.activeFilter = "all"
        const persisted = JSON.stringify(ShellStore.gameCollections)
        check(ShellStore.renameCollection(first, "Failed rename"), "failure setup rejected")
        client.requestFailed(client.requests[client.requests.length - 1].id, "io_error", "Disk full")
        check(!ShellStore.collectionsBusy && ShellStore.collectionError === "Disk full"
            && JSON.stringify(ShellStore.gameCollections) === persisted, "failed write lost saved state")
        check(ShellStore.renameCollection(first, "Disconnected"), "disconnect setup rejected")
        client.state = "failed"
        check(!ShellStore.collectionsBusy && ShellStore.collectionError !== "", "disconnect stranded save")
        client.state = "ready"
        check(JSON.stringify(ShellStore.gameCollections) === persisted, "disconnect replaced committed data")
        check(ShellStore.deleteCollection(first), "delete rejected")
        complete()
        check(ShellStore.activeCollectionId === "" && screen.games.length === 3, "delete did not restore all games")
        check(ShellStore.gameCollections.length === 1 && ShellStore.catalogGames.length === 3, "delete removed games")

        screen.editCollection("create", ShellStore.catalogGames[2])
        const field = find(item, "collectionNameField")
        const confirm = find(item, "collectionConfirmButton")
        check(field && confirm, "collection editor not rendered")
        const dialogBackground = find(item, "collectionDialogBackground")
        check(dialogBackground && dialogBackground.color === DesktopTokens.shell
            && field.background.color === DesktopTokens.raised, "collection editor does not follow the active theme")
        field.text = "Controller nights"
        check(confirm.enabled, "valid collection cannot be submitted")
        confirm.clicked()
        complete()
        check(screen.collection.name === "Controller nights" && screen.games.length === 1, "dialog failed to create, add and open collection")

        ShellStore.catalogGames = originalGames
        ShellStore.applySetting("gameCollections", [
            {id: second, name: "With friends", gameIds: originalGames.slice(0, 5).map(game => ShellStore.gameIdentity(game))},
            {id: "weekend", name: "Weekend games", gameIds: originalGames.slice(0, 3).map(game => ShellStore.gameIdentity(game))}
        ])
        ShellStore.activeCollectionId = second
        ShellStore.collectionError = ""
        return true
    }
}
