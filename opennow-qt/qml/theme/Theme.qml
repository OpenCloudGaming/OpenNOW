pragma Singleton
import QtQuick

QtObject {
    readonly property string mode: String(ShellStore.settings.appTheme || "auto")
    readonly property string themePack: ShellStore.previewThemePack !== ""
                                        ? ShellStore.previewThemePack
                                        : String(ShellStore.settings.themePack || "nocturne")
    readonly property bool packLight: themePack === "bone" || themePack === "cobalt"
    readonly property bool systemLight: Qt.styleHints.colorScheme === Qt.Light
    readonly property bool lightMode: mode === "light" || (mode === "auto" && (packLight || systemLight))
    readonly property bool translucent: Boolean(ShellStore.settings.translucentUI)
    readonly property string accent: String(ShellStore.settings.appAccentColor || "blue")
    readonly property color shell: themePack === "nocturne" ? "#0B0F1A"
                                  : themePack === "aurora" ? "#071A20"
                                  : themePack === "kraft" ? "#211D17"
                                  : themePack === "phosphor" ? "#07120B"
                                  : themePack === "bone" ? "#EEE7DB"
                                  : themePack === "cobalt" ? "#F7F9FF"
                                  : themePack === "hibiscus" ? "#1F0C18"
                                  : themePack === "chapel" ? "#151225"
                                  : lightMode ? "#F2F5FA" : "#0B0F1A"
    readonly property color face: lightMode ? "#111827" : "#FFFFFF"
    readonly property color faceText: lightMode ? "#FFFFFF" : "#111827"
    // Artwork scrims and dark store-color fallbacks always need a light foreground,
    // independently of the shell's light/dark mode.
    readonly property color mediaForeground: "#FFFFFF"
    readonly property color seam: lightMode ? Qt.rgba(0.04, 0.06, 0.10, 0.14) : Qt.rgba(1, 1, 1, 0.14)
    readonly property color label: lightMode ? "#111827" : "#FFFFFF"
    readonly property color textMuted: lightMode ? Qt.rgba(0.04, 0.06, 0.10, 0.64) : Qt.rgba(1, 1, 1, 0.64)
    readonly property color focus: themePack === "nocturne" ? "#7FD4FF"
                                  : themePack === "aurora" ? "#6EE7B7"
                                  : themePack === "kraft" ? "#D3A85C"
                                  : themePack === "phosphor" ? "#6BFF8A"
                                  : themePack === "bone" ? "#7F6A4D"
                                  : themePack === "cobalt" ? "#245BDB"
                                  : themePack === "hibiscus" ? "#FF6F9F"
                                  : themePack === "chapel" ? "#FFD166"
                                  : accent === "green" ? "#6EE7B7"
                                  : accent === "violet" ? "#A78BFA"
                                  : accent === "amber" ? "#FFD166"
                                  : accent === "rose" ? "#FF8A9A"
                                  : accent === "coral" ? "#FF8A80"
                                  : accent === "white" ? "#FFFFFF"
                                  : "#7FD4FF"
    readonly property color mint: "#6EE7B7"
    readonly property color violet: "#A78BFA"
    readonly property color yellow: "#FFD166"
    readonly property color coral: "#FF8A80"
    readonly property color glass: lightMode ? Qt.rgba(1, 1, 1, translucent ? 0.52 : 0.68) : Qt.rgba(0.055, 0.063, 0.094, translucent ? 0.46 : 0.62)
    readonly property color glassStrong: lightMode ? Qt.rgba(1, 1, 1, translucent ? 0.74 : 0.9) : Qt.rgba(0.055, 0.063, 0.094, translucent ? 0.66 : 0.82)
    readonly property color cartSteam: "#26364A"
    readonly property color cartEpic: "#1B1B1F"
    readonly property color cartUbisoft: "#3A5BD9"
    readonly property color cartXbox: "#107C41"
    readonly property color cartGog: "#7B3FE4"
    readonly property color cartBattlenet: "#2E7CB8"

    function contrastText(background) {
        const brightness = background.r * 0.299 + background.g * 0.587 + background.b * 0.114
        return brightness > 0.58 ? "#111827" : "#FFFFFF"
    }

    readonly property string displayFont: "Nunito"
    readonly property string bodyFont: "Nunito"
    readonly property string monoFont: "IBM Plex Mono"

    readonly property int focusDuration: AppController.reducedMotion ? 0 : 140
    readonly property int enterDuration: AppController.reducedMotion ? 0 : 260
    readonly property int heroDuration: AppController.reducedMotion ? 0 : 200
    readonly property int overlayDuration: AppController.reducedMotion ? 0 : 180
    readonly property int panelDuration: AppController.reducedMotion ? 0 : 220
    readonly property var easeOut: [0.16, 1.0, 0.3, 1.0]
    readonly property var easeEmphasized: [0.2, 0.9, 0.1, 1.0]

    function unit(windowWidth, windowHeight) {
        return Math.min(windowWidth / 100, windowHeight / 56.25)
    }
}
