pragma Singleton
import QtQuick
import OpenNOW

QtObject {
    id: tokens

    function relativeLastPlayed(raw, nowMs) {
        const timestamp = Date.parse(String(raw || ""))
        if (isNaN(timestamp))
            return ""
        const seconds = Math.max(0, Math.floor((nowMs - timestamp) / 1000))
        if (seconds < 1)
            return qsTr("Just now")
        if (seconds < 60)
            return seconds === 1 ? qsTr("1 second ago") : qsTr("%1 seconds ago").arg(seconds)
        const minutes = Math.floor(seconds / 60)
        if (minutes < 60)
            return minutes === 1 ? qsTr("1 minute ago") : qsTr("%1 minutes ago").arg(minutes)
        const hours = Math.floor(minutes / 60)
        if (hours < 24)
            return hours === 1 ? qsTr("1 hour ago") : qsTr("%1 hours ago").arg(hours)
        const days = Math.floor(hours / 24)
        return days === 1 ? qsTr("1 day ago") : qsTr("%1 days ago").arg(days)
    }

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
    property FontMetrics storeTitleMetrics: FontMetrics {
        font.family: Theme.bodyFont
        font.pixelSize: tokens.monoSize
        font.weight: Font.Bold
    }
    readonly property int storeCardInfoHeight: px(8) + Math.ceil(storeTitleMetrics.height) * 2 + px(4) + px(17) + px(4)
    readonly property int quickDuration: AppController.reducedMotion ? 0 : 120
    readonly property int motionDuration: AppController.reducedMotion ? 0 : 220
    readonly property int revealDuration: AppController.reducedMotion ? 0 : 320
    readonly property real cardHoverScale: 1.025
    readonly property int cardOutlinePad: 2
    readonly property color cardOutlineIdle: Theme.seam

    function px(value) {
        return Math.max(1, Math.round(Number(value) * uiScale))
    }

    function scaleForWindow(width, height) {
        // Qt already accounts for display DPI. Keep logical text readable;
        // use reflow, not aggressive downscaling, for smaller windows.
        return Math.max(0.95, Math.min(1.15, Math.min(width / 1440, height / 900)))
    }

    function storeKey(value) {
        const key = String(value || "").toLowerCase()
        if (key.indexOf("steam") >= 0) return "steam"
        if (key.indexOf("epic") >= 0) return "epic"
        if (key.indexOf("ubisoft") >= 0 || key.indexOf("uplay") >= 0) return "ubisoft"
        if (key.indexOf("battle") >= 0) return "battlenet"
        if (key.indexOf("xbox") >= 0) return "xbox"
        if (key.indexOf("gog") >= 0) return "gog"
        if (key.indexOf("gaijin") >= 0) return "gaijin"
        if (key === "nvidia") return "nvidia"
        if (key === "ea" || key === "ea_app" || key === "origin") return "ea"
        return ""
    }
    function storeIconUrl(value) {
        const key = storeKey(value)
        return key ? "qrc:/qt/qml/OpenNOW/res/icons/store-" + key + ".svg" : ""
    }
    function storeLabel(value) {
        const labels = {steam:"Steam", epic:"Epic Games", ubisoft:"Ubisoft Connect", battlenet:"Battle.net",
            xbox:"Xbox", gog:"GOG", gaijin:"Gaijin", ea:"EA app", nvidia:"NVIDIA"}
        return labels[storeKey(value)] || (String(value).toUpperCase() === "NONE" ? qsTr("Direct launch") : String(value))
    }
    function genreLabel(value) {
        return String(value).toLowerCase().replace(/_/g, " ").replace(/\b\w/g, letter => letter.toUpperCase())
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
