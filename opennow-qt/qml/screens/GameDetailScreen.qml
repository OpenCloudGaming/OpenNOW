import QtQuick
import QtQuick.Controls
import OpenNOW

FocusScope {
    id: root
    property var previewGame: null
    property bool initialPlatformOpen: false
    readonly property var game: previewGame || ShellStore.selectedGame || ({ title: qsTr("Choose a game"), availableStores: [], variants: [] })
    readonly property string artwork: game.heroImageUrl || game.imageUrl || ""
    readonly property bool canLaunch: Boolean(previewGame) || !ShellStore.signedIn || ShellStore.selectedLaunchAppId() !== ""

    function aspectLabel(resolution) {
        const parts = String(resolution || "").split("x")
        if (parts.length !== 2)
            return ""
        const ratio = Number(parts[0]) / Math.max(1, Number(parts[1]))
        if (Math.abs(ratio - 16 / 9) < 0.05) return "16:9"
        if (Math.abs(ratio - 16 / 10) < 0.05) return "16:10"
        if (Math.abs(ratio - 21 / 9) < 0.08) return "21:9"
        if (Math.abs(ratio - 32 / 9) < 0.08) return "32:9"
        return ""
    }

    function streamQualityLabel() {
        const resolution = String(ShellStore.settings.resolution || "")
        if (!resolution)
            return "—"
        const parts = resolution.split("x")
        const height = parts.length === 2 ? Number(parts[1]) : 1440
        const named = height >= 2160 ? "4K" : height + "p"
        const fps = Number(ShellStore.settings.fps || 0)
        return fps > 0 ? qsTr("%1 · %2 FPS").arg(named).arg(fps) : named
    }

    function codecLabel() {
        const codec = String(ShellStore.settings.codec || "auto").toUpperCase()
        const quality = String(ShellStore.settings.colorQuality || "")
        const bitDepth = quality.indexOf("10bit") === 0 ? qsTr("10-bit") : qsTr("8-bit")
        const chroma = quality.indexOf("444") >= 0 ? "4:4:4" : "4:2:0"
        return qsTr("%1 · %2 · %3").arg(codec).arg(bitDepth).arg(chroma)
    }

    function regionLabel() {
        const selected = String(ShellStore.settings.region || "")
        if (selected.length)
            return selected
        return qsTr("Automatic region")
    }

    function selectVariant(index) {
        if (!previewGame) {
            ShellStore.selectGameVariant(index)
            return
        }
        const nextGame = Object.assign({}, previewGame)
        nextGame.selectedVariantIndex = index
        previewGame = nextGame
    }
    ScreenBackground { artwork: root.artwork; tint: "#263A4A" }

    GlassPanel {
        x: 215; y: 218
        width: 900; height: 620; panelRadius: 40
        RoundedArtwork {
            anchors.fill: parent; anchors.margins: 3
            artwork: root.artwork; fallbackColor: "#33485E"; cornerRadius: 37; scrimStart: 0.35
        }
        Column {
            x: 38; y: parent.height - height - 38; spacing: 8
            Text { text: (root.game.publisherName || root.game.developerName || "").toUpperCase() + (root.game.genres && root.game.genres.length ? (root.game.publisherName || root.game.developerName ? " · " : "") + root.game.genres.join(" · ").toUpperCase() : ""); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 14; font.weight: Font.Black; font.letterSpacing: 1.2 }
            Text { text: root.game.title; color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 52; font.weight: Font.Black }
        }
        Rectangle {
            anchors.fill: parent
            radius: 40
            color: "transparent"
            border.color: Theme.face
            border.width: 3
            z: 30
        }
    }

    GlassPanel {
        x: 1146; y: 297; width: 560; height: 464; panelRadius: 34
        Text {
            x: 28; y: 28; width: 504; height: 52
            wrapMode: Text.WordWrap; elide: Text.ElideRight; maximumLineCount: 2
            text: root.game.longDescription || root.game.description || qsTr("Stream this title from your GeForce NOW library with your controller, keyboard, or mouse.")
            color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 18; font.weight: Font.DemiBold
        }
        Row {
            x: 28; y: 98; width: 504; height: 72; spacing: 10
            Repeater {
                model: [
                    {value: root.game.hoursPlayed === undefined || root.game.hoursPlayed === null ? "—" : root.game.hoursPlayed + " h", label: qsTr("PLAYED")},
                    {value: root.game.sessionCount === undefined || root.game.sessionCount === null ? "—" : root.game.sessionCount, label: qsTr("SESSIONS")},
                    {value: root.game.lastPlayedLabel || "—", label: qsTr("LAST PLAYED")}
                ]
                Rectangle { required property var modelData; width: (504 - 20) / 3; height: 72; radius: 18; color: Theme.glassStrong
                    Column { anchors.left: parent.left; anchors.leftMargin: 16; anchors.verticalCenter: parent.verticalCenter; spacing: 2
                        Text { text: modelData.value; color: modelData.label === qsTr("LAST PLAYED") ? Theme.mint : Theme.label; font.family: Theme.bodyFont; font.pixelSize: 24; font.weight: Font.Black }
                        Text { text: modelData.label; color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Black; font.letterSpacing: 1 }
                    }
                }
            }
        }
        Text { x: 28; y: 185; text: qsTr("PLATFORM"); color: Theme.textMuted; font.family: Theme.bodyFont; font.pixelSize: 11; font.weight: Font.Black; font.letterSpacing: 1 }
        PlatformPicker {
            id: platformPicker
            x: 28; y: 205; width: 504
            variants: root.game.variants || []
            currentIndex: Math.max(0, Number(root.game.selectedVariantIndex || 0))
            KeyNavigation.down: play
            onVariantSelected: index => root.selectVariant(index)
        }
        Row {
            x: 28; y: 277; spacing: 8
            Repeater {
                model: [root.streamQualityLabel(), root.codecLabel(), "● " + root.regionLabel()]
                GlassPanel {
                    required property string modelData
                    width: chipLabel.implicitWidth + 26; height: 34; panelRadius: 17; strong: true
                    Text { id: chipLabel; anchors.centerIn: parent; text: modelData; color: modelData.indexOf("●") === 0 ? Theme.mint : Theme.label; font.family: Theme.bodyFont; font.pixelSize: 13; font.weight: Font.Black }
                }
            }
        }
        Text {
            x: 40; y: 328
            text: qsTr("Change in Settings")
            color: Theme.textMuted
            font.family: Theme.bodyFont; font.pixelSize: 13; font.weight: Font.Bold
            TapHandler { onTapped: AppController.navigate("settings-video") }
        }
        Row {
            x: 28; y: 378; width: 504; spacing: 10
            GlassButton { id: play; width: 215; height: 56; text: ShellStore.signedIn ? (root.canLaunch ? qsTr("Play") : qsTr("Unavailable")) : qsTr("Sign in"); glyph: "A"; primary: true; enabled: root.canLaunch; KeyNavigation.up: platformPicker; onClicked: { if (!root.previewGame) ShellStore.launchSelectedGame() } Component.onCompleted: forceActiveFocus() }
            GlassButton { width: 279; height: 56; text: ShellStore.isFavorite(root.game) ? qsTr("Remove from My games") : qsTr("Add to My games"); glyph: "Y"; onClicked: ShellStore.toggleFavorite(root.game) }
        }
    }
    Component.onCompleted: {
        if (initialPlatformOpen)
            Qt.callLater(platformPicker.openMenu)
    }
    AppChrome { anchors.fill: parent; title: root.game.title; currentRoute: "home"; onRouteRequested: route => AppController.navigate(route) }
}
