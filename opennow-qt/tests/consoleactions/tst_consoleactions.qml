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
}
