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
    Component {
        id: libraryHostComponent
        FocusScope {
            property alias screen: library
            property var leakedKeys: []
            width: 1920
            height: 1080
            Keys.onPressed: event => leakedKeys = leakedKeys.concat([event.key])
            LibraryScreen { id: library; anchors.fill: parent; focus: true }
        }
    }
    Component {
        id: homeHostComponent
        FocusScope {
            property alias screen: home
            property var leakedKeys: []
            width: 1920
            height: 1080
            Keys.onPressed: event => leakedKeys = leakedKeys.concat([event.key])
            HomeScreen { id: home; anchors.fill: parent; focus: true }
        }
    }

    property var fixtureCatalog: null

    function initTestCase() {
        fixtureCatalog = ShellStore.catalogGames
    }

    function init() {
        ShellStore.catalogGames = fixtureCatalog
        ShellStore.settings = {appTheme: "dark", favoriteGameIds: [], resolution: "1920x1080", fps: 60}
        ShellStore.launchCount = 0
        ShellStore.detailsCount = 0
        ShellStore.selectedGame = ShellStore.catalogGames[0]
        AppController.showOverlay("")
        AppController.inputMode = "keyboard"
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

    function homeFixtureGames() {
        return ["alpha", "bravo", "charlie", "delta"].map(id => ({id: id, launchAppId: id, title: "Home " + id,
            availableStores: ["Steam"], variants: [{store: "Steam", libraryStatus: "MANUAL"}]}))
    }

    function test_librarySearchActivationDoesNotLaunch() {
        const screen = createTemporaryObject(libraryComponent, testCase)
        const search = findChild(screen, "consoleLibrarySearchField")
        const keyboard = findChild(screen, "consoleLibraryKeyboard")
        search.forceActiveFocus()
        AppController.inputMode = "controller"
        keyClick(Qt.Key_Return)
        tryCompare(keyboard, "presented", true)
        compare(ShellStore.detailsCount, 0)
        compare(ShellStore.launchCount, 0)
        keyClick(Qt.Key_Escape)
        tryCompare(keyboard, "presented", false)
        AppController.inputMode = "keyboard"
        search.forceActiveFocus()
        keyClick(Qt.Key_C)
        compare(screen.searchQuery, "c")
        keyClick(Qt.Key_Return)
        verify(!keyboard.presented)
        compare(ShellStore.detailsCount, 0)
        compare(ShellStore.launchCount, 0)
        verify(!search.activeFocus)
        keyClick(Qt.Key_Return)
        compare(ShellStore.detailsCount, 1)
        compare(ShellStore.launchCount, 0)
    }

    function test_homeResizeKeepsEditSheetAndSelection() {
        ShellStore.catalogGames = homeFixtureGames()
        ShellStore.settings = {appTheme: "dark", favoriteGameIds: ["alpha", "bravo", "charlie", "delta"], homeTileSizes: {}}
        const host = createTemporaryObject(homeHostComponent, testCase)
        const screen = host.screen
        tryVerify(() => screen.selectedGame && screen.selectedGame.id === "alpha" && screen.activeFocus)
        keyClick(Qt.Key_Right)
        tryCompare(screen, "currentIndex", 1)
        keyClick(Qt.Key_X)
        tryCompare(screen, "editMenuOpen", true)
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Down)
        keyClick(Qt.Key_Return)
        compare(ShellStore.homeTileSize({id: "bravo"}), "wide")
        wait(50)
        verify(screen.editMenuOpen)
        compare(screen.selectedGame.id, "bravo")
        keyClick(Qt.Key_Up)
        keyClick(Qt.Key_Return)
        compare(ShellStore.homeTileSize({id: "bravo"}), "square")
        wait(50)
        verify(screen.editMenuOpen)
        compare(screen.selectedGame.id, "bravo")
        keyClick(Qt.Key_Escape)
        tryCompare(screen, "editMenuOpen", false)
        compare(host.leakedKeys.length, 0)
        compare(ShellStore.detailsCount, 0)
        tryVerify(() => screen.activeFocus && screen.selectedGame.id === "bravo")
        keyClick(Qt.Key_Right)
        tryCompare(screen, "currentIndex", 2)
        compare(screen.selectedGame.id, "charlie")
    }

    function test_homeWideResizeOfFirstTileKeepsSelection() {
        ShellStore.catalogGames = homeFixtureGames()
        ShellStore.settings = {appTheme: "dark", favoriteGameIds: ["alpha", "bravo", "charlie", "delta"], homeTileSizes: {}}
        const host = createTemporaryObject(homeHostComponent, testCase)
        const screen = host.screen
        tryVerify(() => screen.selectedGame && screen.selectedGame.id === "alpha" && screen.activeFocus)
        keyClick(Qt.Key_Right)
        keyClick(Qt.Key_Right)
        tryCompare(screen, "currentIndex", 2)
        ShellStore.setHomeTileSize({id: "alpha"}, "wide")
        tryVerify(() => screen.activeFocus && screen.selectedGame.id === "charlie")
        ShellStore.removeFromHome({id: "charlie"})
        tryVerify(() => screen.activeFocus && screen.selectedGame !== null)
        compare(host.leakedKeys.length, 0)
    }

    function test_libraryGameShortcutsStayOnGrid() {
        const host = createTemporaryObject(libraryHostComponent, testCase)
        const screen = host.screen
        const search = findChild(screen, "consoleLibrarySearchField")
        const filter = findChild(screen, "consoleLibraryFilterButton")
        const keyboard = findChild(screen, "consoleLibraryKeyboard")
        AppController.inputMode = "controller"
        filter.forceActiveFocus()
        keyClick(Qt.Key_X)
        keyClick(Qt.Key_Y)
        verify(filter.activeFocus)
        verify(!screen.filterSheetOpen)
        compare(ShellStore.detailsCount, 0)
        compare(ShellStore.launchCount, 0)
        verify(!ShellStore.isFavorite(screen.selectedGame))
        compare(host.leakedKeys.length, 0)
        AppController.inputMode = "keyboard"
        search.forceActiveFocus()
        keyClick(Qt.Key_X)
        keyClick(Qt.Key_Y)
        compare(screen.searchQuery, "xy")
        verify(search.activeFocus)
        compare(ShellStore.detailsCount, 0)
        verify(!ShellStore.isFavorite(ShellStore.catalogGames[0]))
        keyClick(Qt.Key_Escape)
        verify(!search.activeFocus)
        verify(!keyboard.presented)
        compare(host.leakedKeys.length, 0)
        search.text = ""
        screen.searchQuery = ""
        search.forceActiveFocus()
        keyClick(Qt.Key_Back)
        tryCompare(keyboard, "presented", true)
        keyClick(Qt.Key_Escape)
        tryCompare(keyboard, "presented", false)
        compare(host.leakedKeys.length, 0)
        keyClick(Qt.Key_Y)
        verify(ShellStore.isFavorite(screen.selectedGame))
        keyClick(Qt.Key_X)
        compare(ShellStore.detailsCount, 1)
        compare(ShellStore.launchCount, 0)
    }
}
