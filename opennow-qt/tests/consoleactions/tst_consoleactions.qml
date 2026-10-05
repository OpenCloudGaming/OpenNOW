import QtQuick
import QtTest
import OpenNOW

TestCase {
    id: testCase
    name: "ConsoleActions"
    when: windowShown
    visible: true
    width: 1920
    height: 1080

    Component { id: detailsComponent; GameDetailScreen { width: 1920; height: 1080 } }
    Component { id: libraryComponent; LibraryScreen { width: 1920; height: 1080 } }

    function init() {
        ShellStore.settings = {appTheme: "dark", favoriteGameIds: [], resolution: "1920x1080", fps: 60}
        ShellStore.launchCount = 0
        ShellStore.detailsCount = 0
        ShellStore.selectedGame = ShellStore.catalogGames[0]
        AppController.showOverlay("")
    }

    function test_detailsPlayAndFavorite() {
        const screen = createTemporaryObject(detailsComponent, testCase)
        verify(screen !== null)
        const play = findChild(screen, "consolePlayButton")
        verify(play !== null)
        play.forceActiveFocus()
        keyClick(Qt.Key_Return)
        compare(ShellStore.launchCount, 1)
        keyClick(Qt.Key_Y)
        verify(ShellStore.isFavorite(ShellStore.selectedGame))
        verify(play.activeFocus)
        compare(ShellStore.launchCount, 1)
        keyClick(Qt.Key_Right)
        const favorite = findChild(screen, "consoleFavoriteButton")
        verify(favorite.activeFocus)
        keyClick(Qt.Key_Return)
        verify(!ShellStore.isFavorite(ShellStore.selectedGame))
        compare(ShellStore.launchCount, 1)
    }

    function test_libraryFavoriteDoesNotOpenSearch() {
        const screen = createTemporaryObject(libraryComponent, testCase)
        verify(screen !== null)
        screen.forceActiveFocus()
        keyClick(Qt.Key_Y)
        verify(ShellStore.isFavorite(screen.selectedGame))
        const keyboard = findChild(screen, "consoleLibraryKeyboard")
        verify(!keyboard.presented)
        keyClick(Qt.Key_Y)
        verify(!ShellStore.isFavorite(screen.selectedGame))
        keyClick(Qt.Key_X)
        compare(ShellStore.detailsCount, 1)
        keyClick(Qt.Key_Back)
        tryCompare(keyboard, "presented", true)
        keyClick(Qt.Key_Escape)
        tryCompare(keyboard, "presented", false)
    }

    function test_storeChipKeepsFocusAcrossSelectionAndRefresh() {
        ShellStore.selectedGame = {id:"fixture-game", launchAppId:"12345", title:"Two stores",
            selectedVariantIndex:0, variants:[
                {store:"Steam", libraryStatus:"MANUAL"},
                {store:"Epic", libraryStatus:"NOT_OWNED"}
            ]}
        const screen = createTemporaryObject(detailsComponent, testCase)
        const epic = findChild(screen, "consolePlatformChip1")
        verify(epic !== null)
        epic.forceActiveFocus()
        keyClick(Qt.Key_Return)
        compare(ShellStore.selectedGame.selectedVariantIndex, 1)
        compare(findChild(screen, "consolePlatformChip1"), epic)
        verify(epic.activeFocus)
        compare(ShellStore.launchCount, 0)
        ShellStore.selectedGame = Object.assign({}, ShellStore.selectedGame, {title:"Refreshed metadata"})
        compare(findChild(screen, "consolePlatformChip1"), epic)
        verify(epic.activeFocus)
        keyClick(Qt.Key_Left)
        const steam = findChild(screen, "consolePlatformChip0")
        verify(steam.activeFocus)
        keyClick(Qt.Key_Return)
        compare(ShellStore.selectedGame.selectedVariantIndex, 0)
        compare(ShellStore.launchCount, 0)
    }

    function test_missingStoreSelectionDoesNotSelectAnotherVersion() {
        ShellStore.selectedGame = {id:"fixture-game", launchAppId:"", title:"Store removed",
            selectedVariantIndex:-1, variants:[{store:"Steam", libraryStatus:"MANUAL"}]}
        const screen = createTemporaryObject(detailsComponent, testCase)
        compare(screen.selectedVariantIndex, -1)
        compare(screen.selectedVariant, null)
        verify(!findChild(screen, "consolePlatformChip0").selectedVariant)
        verify(!findChild(screen, "consolePlayButton").enabled)
        compare(screen.libraryOptions.filter(option => option.value === "remove" || option.value === "select").length, 0)
        screen.forceActiveFocus()
        keyClick(Qt.Key_Return)
        compare(ShellStore.launchCount, 0)
    }
}
