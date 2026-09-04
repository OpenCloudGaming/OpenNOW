pragma Singleton
import QtQuick
import OpenNOW

QtObject {
    readonly property color shell: Theme.shell
    readonly property color rail: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.90)
    readonly property color topBar: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.66)
    readonly property color statusBar: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.78)
    readonly property color surface: Theme.glass
    readonly property color raised: Theme.lightMode ? Qt.rgba(0.04, 0.06, 0.10, 0.08) : "#14FFFFFF"
    readonly property color raisedStrong: Theme.lightMode ? Qt.rgba(0.04, 0.06, 0.10, 0.12) : "#1FFFFFFF"
    readonly property color seam: Theme.seam
    readonly property color seamSoft: Theme.lightMode ? Qt.rgba(0.04, 0.06, 0.10, 0.06) : "#0FFFFFFF"
    readonly property color text: Theme.label
    readonly property color textHigh: Theme.label
    readonly property color textBody: Theme.textMuted
    readonly property color textMuted: Theme.textMuted
    readonly property color textFaint: Theme.lightMode ? Qt.rgba(0.04, 0.06, 0.10, 0.32) : "#52FFFFFF"
    readonly property color focus: Theme.focus
    readonly property color green: "#1DB954"
    readonly property color mint: "#56E6A5"
    readonly property color amber: "#FFD166"
    readonly property color ledAmber: "#F5A623"
    readonly property color danger: "#FF8A80"
    readonly property string displayFont: Theme.displayFont
    readonly property string bodyFont: Theme.bodyFont
    readonly property string monoFont: Theme.monoFont
    property real uiScale: 1
    // Type ramp. Every desktop font size must come from here so text stays
    // proportionate on any display size; raw pixelSize literals drift.
    readonly property int titleSize: px(26)
    readonly property int headingSize: px(17)
    readonly property int bodySize: px(15)
    readonly property int captionSize: px(13)
    readonly property int monoSize: px(12)
    readonly property int smallSize: px(11)
    readonly property int microSize: px(10)
    readonly property int tinySize: px(9)
    readonly property int railWidth: px(232)
    readonly property int railCollapsedWidth: px(72)
    readonly property int topBarHeight: px(64)
    readonly property int statusBarHeight: px(52)
    readonly property int rowHeight: px(76)
    readonly property int controlHeight: px(38)
    readonly property int posterWidth: px(112)
    readonly property int posterHeight: px(168)
    readonly property int libraryCellWidth: px(146)
    readonly property int libraryCellHeight: px(214)
    readonly property int libraryArtWidth: px(132)
    readonly property int libraryArtHeight: px(198)
    readonly property int quickDuration: AppController.reducedMotion ? 0 : 120
    readonly property int motionDuration: AppController.reducedMotion ? 0 : 220
    readonly property int revealDuration: AppController.reducedMotion ? 0 : 320
    readonly property real cardHoverScale: 1.025
    readonly property int cardOutlinePad: 2
    readonly property color cardOutlineIdle: Qt.rgba(1, 1, 1, 0.16)

    function px(value) {
        return Math.max(1, Math.round(Number(value) * uiScale))
    }

    function scaleForWindow(width, height) {
        return 1
    }

    function artworkUrl(game, preferHero) {
        if (!game)
            return ""
        const raw = preferHero
            ? String(game.heroImageUrl || game.imageUrl || game.screenshotUrl || game.boxArtUrl || "")
            : String(game.imageUrl || game.heroImageUrl || game.screenshotUrl || game.boxArtUrl || "")
        return decodeArtworkUrl(raw)
    }

    function decodeArtworkUrl(url) {
        return String(url || "").split(";f=webp").join(";f=jpg")
    }

    function consoleModeOn(win) {
        return win
            ? Boolean(win.forceConsole || win.desktopSurfaceActive === false)
            : ShellStore.settings.launchInConsoleMode === true
    }
    function consoleModeTargetOn(win) {
        return win
            ? !Boolean(win.targetDesktopSurface)
            : ShellStore.settings.launchInConsoleMode === true
    }
    function consoleModePending(win) {
        return ShellStore.consoleSurfaceRequestId !== "" || Boolean(win
            && win.targetDesktopSurface !== undefined
            && win.targetDesktopSurface !== win.desktopSurfaceActive)
    }
}
