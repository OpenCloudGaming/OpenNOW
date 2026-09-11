import QtQuick

QtObject {
    property var settings: ({appTheme: "dark", favoriteGameIds: [], resolution: "1920x1080", fps: 60})
    property string previewThemePack: ""
    property bool ready: true
    property bool signedIn: true
    property var authSession: ({user: {displayName: "Controller test"}})
    property var activeSession: null
    property var regions: []
    property var regionPingResults: ({})
    property var catalogGames: [{id: "fixture-game", launchAppId: "12345", title: "Controller test game",
        availableStores: ["Steam"], variants: [{store: "Steam", inLibrary: true}]}]
    property var selectedGame: catalogGames[0]
    property bool catalogLoading: false
    property string catalogError: ""
    property string catalogSource: "account-library"
    property int catalogTotalCount: 1
    property int launchCount: 0
    property int detailsCount: 0

    function focusIndex(route) { return 0 }
    function rememberFocus(route, index) {}
    function artworkUrl(source) { return source }
    function retainArtwork(source) {}
    function releaseArtwork(source) {}
    function requestArtwork(source) {}
    function selectedLaunchAppId() { return selectedGame.launchAppId }
    function selectGameVariant(index) { selectedGame = Object.assign({}, selectedGame, {selectedVariantIndex: index}) }
    function isFavorite(game) { return game && settings.favoriteGameIds.indexOf(game.id) >= 0 }
    function toggleFavorite(game) {
        const ids = settings.favoriteGameIds.slice()
        const index = ids.indexOf(game.id)
        if (index >= 0) ids.splice(index, 1)
        else ids.push(game.id)
        settings = Object.assign({}, settings, {favoriteGameIds: ids})
    }
    function openGame(game) { selectedGame = game; detailsCount++ }
    function launchSelectedGame() { launchCount++ }
}
