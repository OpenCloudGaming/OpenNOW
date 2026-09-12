pragma Singleton
import QtQuick

QtObject {
    readonly property var packs: [
        {id:"nocturne", name:"Nocturne", author:"OPENNOW", category:"Dark", detail:"SKY GLASS", bg:"#0B0F1A", lightBg:"#EEF3FA", mid:"#252A48", accent:"#7FD4FF", lightAccent:"#12638C"},
        {id:"aurora", name:"Aurora", author:"OPENNOW", category:"Dark", detail:"MINT AURORA", bg:"#071A20", lightBg:"#EAF5F2", mid:"#16434A", accent:"#6EE7B7", lightAccent:"#126B4E"},
        {id:"kraft", name:"Kraft", author:"OPENNOW", category:"Dark", detail:"WARM BRASS", bg:"#211D17", lightBg:"#F4ECDD", mid:"#756346", accent:"#D3A85C", lightAccent:"#796020"},
        {id:"phosphor", name:"Phosphor", author:"OPENNOW", category:"High contrast", detail:"CRT GLOW", bg:"#07120B", lightBg:"#EDF8EA", mid:"#12351D", accent:"#6BFF8A", lightAccent:"#216C31"},
        {id:"bone", name:"Bone", author:"OPENNOW", category:"Light", detail:"WARM MINIMAL", bg:"#EEE7DB", darkBg:"#201D18", mid:"#A89578", accent:"#7F6A4D", darkAccent:"#D3BA91"},
        {id:"cobalt", name:"Cobalt", author:"OPENNOW", category:"Light", detail:"COBALT PAPER", bg:"#F7F9FF", darkBg:"#0E1730", mid:"#ADC4FF", accent:"#245BDB", darkAccent:"#93B5FF"},
        {id:"hibiscus", name:"Hibiscus", author:"OPENNOW", category:"Dark", detail:"ROSE GLASS", bg:"#1F0C18", lightBg:"#FAEDF3", mid:"#642343", accent:"#FF6F9F", lightAccent:"#AA285B"},
        {id:"chapel", name:"Chapel", author:"OPENNOW", category:"Dark", detail:"GOLD GLASS", bg:"#151225", lightBg:"#F3EFF9", mid:"#40355D", accent:"#FFD166", lightAccent:"#806017"}
    ]
    readonly property string mode: String(ShellStore.settings.appTheme || "auto")
    readonly property string themePack: ShellStore.previewThemePack !== ""
                                        ? ShellStore.previewThemePack
                                        : String(ShellStore.settings.themePack || "nocturne")
    readonly property var pack: packs.find(item => item.id === themePack) || packs[0]
    readonly property bool packLight: pack.category === "Light"
    readonly property bool systemLight: Qt.styleHints.colorScheme === Qt.Light
    readonly property bool lightMode: ShellStore.previewThemePack !== "" ? packLight
        : mode === "light" || (mode === "auto" && systemLight)
    readonly property bool translucent: Boolean(ShellStore.settings.translucentUI)
    readonly property string accent: String(ShellStore.settings.appAccentColor || "blue")
    readonly property color shell: lightMode ? (pack.lightBg || pack.bg) : (pack.darkBg || pack.bg)
    readonly property color face: lightMode ? "#111827" : "#FFFFFF"
    readonly property color faceText: lightMode ? "#FFFFFF" : "#111827"
    // Artwork scrims and dark store-color fallbacks always need a light foreground,
    // independently of the shell's light/dark mode.
    readonly property color mediaForeground: "#FFFFFF"
    readonly property color seam: lightMode ? Qt.rgba(0.04, 0.06, 0.10, 0.14) : Qt.rgba(1, 1, 1, 0.14)
    readonly property color label: lightMode ? "#111827" : "#FFFFFF"
    readonly property color textMuted: lightMode ? Qt.rgba(0.04, 0.06, 0.10, 0.64) : Qt.rgba(1, 1, 1, 0.64)
    readonly property var accentChoices: ["green", "blue", "violet", "rose", "coral", "amber", "white"]
    function accentColor(value) {
        const dark = {green:"#6EE7B7", blue:"#7FD4FF", violet:"#A78BFA", rose:"#FF8A9A", coral:"#FF8A80", amber:"#FFD166", white:"#FFFFFF"}
        const light = {green:"#126B4E", blue:"#12638C", violet:"#7042BA", rose:"#AA285B", coral:"#A9362C", amber:"#806017", white:"#374151"}
        return (lightMode ? light : dark)[value] || (lightMode ? light.blue : dark.blue)
    }
    readonly property color customAccent: accentColor(accent)
    readonly property color packAccent: lightMode ? (pack.lightAccent || pack.accent) : (pack.darkAccent || pack.accent)
    readonly property bool accentOverridden: ShellStore.previewThemePack === "" && ShellStore.settings.themeAccentOverride === true
    readonly property color focus: accentOverridden ? customAccent : packAccent
    readonly property color focusText: contrastText(focus)
    readonly property color mint: "#6EE7B7"
    readonly property color violet: "#A78BFA"
    readonly property color yellow: "#FFD166"
    readonly property color coral: "#FF8A80"
    readonly property color glass: Qt.rgba(shell.r, shell.g, shell.b, translucent ? 0.52 : 0.72)
    readonly property color glassStrong: Qt.rgba(shell.r, shell.g, shell.b, translucent ? 0.74 : 0.94)
    readonly property color cartSteam: "#26364A"
    readonly property color cartEpic: "#1B1B1F"
    readonly property color cartUbisoft: "#3A5BD9"
    readonly property color cartXbox: "#107C41"
    readonly property color cartGog: "#7B3FE4"
    readonly property color cartBattlenet: "#2E7CB8"

    function contrastText(background) {
        const linear = value => value <= 0.04045 ? value / 12.92 : Math.pow((value + 0.055) / 1.055, 2.4)
        const luminance = linear(background.r) * 0.2126 + linear(background.g) * 0.7152 + linear(background.b) * 0.0722
        return (luminance + 0.05) / 0.0592 > 1.05 / (luminance + 0.05) ? "#111827" : "#FFFFFF"
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
