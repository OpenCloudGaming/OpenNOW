import QtQuick
import OpenNOW

// Translation overlay: renders the live EN -> VI translation above the stream
// video in the SAME window. It never captures input (enabled: false, no
// MouseArea, no Keys handler) and never touches the stream surface, so the
// session keeps running untouched underneath it.
//
// The C++ translationEngine context property is registered by the translation
// engine (parallel work). Every engine access below is guarded by hasEngine,
// so this overlay stays inert instead of logging errors until the engine
// lands. Visibility derives straight from translationEngine.hasTranslation:
// translationCleared hides it immediately (no fade, no timer) and a new
// translationReady replaces the text in one frame (direct binding, no
// animation), so there is no flicker.
Item {
    id: root
    anchors.fill: parent
    z: 6
    enabled: false

    readonly property bool hasEngine: typeof translationEngine !== "undefined" && translationEngine !== null
    readonly property bool engineOn: root.hasEngine && translationEngine.enabled
    readonly property bool hasText: root.hasEngine && translationEngine.hasTranslation
    readonly property string mode: root.hasEngine ? String(translationEngine.overlayMode || "inplace") : "off"
    // The parent screen (DesktopStreamScreen) exposes `streaming`; fall back
    // to showing on engine state alone when hosted anywhere else.
    readonly property bool parentStreaming: parent && parent.streaming !== undefined
        ? Boolean(parent.streaming) : true
    readonly property bool showing: root.engineOn && root.hasText && root.mode !== "off" && root.parentStreaming

    visible: root.showing

    // Settings -> engine sync (integration fix): QML is the only side that
    // can read ShellStore.settings, so this overlay — instantiated on the
    // stream screen — mirrors "translation.*" into the C++ engine on load
    // and whenever the settings map changes. Without this, the engine
    // would keep its defaults (enabled=false) forever no matter what the
    // Settings page saved. applySetting() replaces the whole settings
    // object (SettingsState.qml), so settingsChanged fires on every save.
    function syncEngineFromSettings() {
        if (!root.hasEngine)
            return
        const s = ShellStore.settings || ({})
        const get = (key, fallback) => {
            const v = s["translation." + key]
            return v === undefined || v === null ? fallback : v
        }
        translationEngine.enabled = get("enabled", false) === true
        translationEngine.sourceLanguage = String(get("sourceLanguage", "en"))
        translationEngine.targetLanguage = String(get("targetLanguage", "vi"))
        translationEngine.ocrEngine = String(get("ocrEngine", "gemini-vision"))
        translationEngine.translationProvider = String(get("translationProvider", "google"))
        translationEngine.overlayMode = String(get("overlayMode", "inplace"))
        translationEngine.stabilityDelayMs = Number(get("stabilityDelayMs", 400))
        translationEngine.fontSizePx = Number(get("fontSizePx", 22))
        translationEngine.showOriginal = get("showOriginal", false) === true
        translationEngine.cacheEnabled = get("cacheEnabled", true) !== false
        translationEngine.setDialoguePreset(String(get("dialoguePreset", "wuthering-waves")))
    }

    Component.onCompleted: root.syncEngineFromSettings()

    Connections {
        target: ShellStore
        function onSettingsChanged() { root.syncEngineFromSettings() }
    }

    // Dialogue area, normalized 0..1, from the engine (default
    // 0.05, 0.72, 0.90, 0.24). Converted to pixels against this item.
    readonly property rect dialogueRect: root.hasEngine
        ? translationEngine.dialogueRect : Qt.rect(0.05, 0.72, 0.90, 0.24)
    readonly property real boxX: Math.max(0, Math.min(root.dialogueRect.x * root.width,
        Math.max(0, root.width - root.dialogueRect.width * root.width)))
    readonly property real boxY: Math.max(0, Math.min(root.dialogueRect.y * root.height,
        Math.max(0, root.height - root.dialogueRect.height * root.height)))
    readonly property real boxW: Math.max(0, Math.min(root.dialogueRect.width * root.width, root.width))
    readonly property real boxH: Math.max(0, Math.min(root.dialogueRect.height * root.height, root.height))

    readonly property int baseFontSize: root.hasEngine ? Number(translationEngine.fontSizePx || 22) : 22
    readonly property bool showOriginal: root.hasEngine && translationEngine.showOriginal === true
    readonly property string translatedText: root.hasEngine ? String(translationEngine.lastTranslatedText || "") : ""
    readonly property string originalText: root.hasEngine ? String(translationEngine.lastOriginalText || "") : ""

    readonly property string timingText: {
        if (!root.hasEngine)
            return ""
        const ocr = Number(translationEngine.lastOcrMs || 0)
        const tr = Number(translationEngine.lastTranslateMs || 0)
        let text = qsTr("OCR %1ms · Dịch %2ms").arg(ocr).arg(tr)
        if (translationEngine.lastCacheHit === true)
            text += " · " + qsTr("Cache hit")
        return text
    }

    // One box shape for both modes: semi-transparent dark background, white
    // wrapped text that shrinks (never overflows the box), optional small
    // original underneath.
    component TranslationBox: Rectangle {
        id: box
        required property string translated
        required property string original
        required property bool showOriginalText
        required property int baseFontSize
        color: "black"
        opacity: 0.85
        radius: DesktopTokens.px(12)
        Column {
            anchors.fill: parent
            anchors.margins: DesktopTokens.px(12)
            spacing: DesktopTokens.px(4)
            Text {
                id: translatedLabel
                width: parent.width
                height: parent.height - (originalLabel.visible ? originalLabel.height + parent.spacing : 0)
                text: box.translated
                color: "white"
                font.family: Theme.bodyFont
                font.pixelSize: box.baseFontSize
                fontSizeMode: Text.Fit
                minimumPixelSize: 12
                wrapMode: Text.WordWrap
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }
            Text {
                id: originalLabel
                visible: box.showOriginalText && box.original !== ""
                width: parent.width
                text: box.original
                color: Qt.rgba(1, 1, 1, 0.65)
                font.family: Theme.bodyFont
                font.pixelSize: Math.max(11, Math.round(box.baseFontSize * 0.6))
                wrapMode: Text.WordWrap
                elide: Text.ElideRight
                maximumLineCount: 2
            }
        }
    }

    // (a) In-place: translated text drawn over the game's dialogue area,
    // covering the original line.
    TranslationBox {
        id: inplaceBox
        visible: root.showing && root.mode === "inplace"
        x: root.boxX
        y: root.boxY
        width: root.boxW
        height: root.boxH
        translated: root.translatedText
        original: root.originalText
        showOriginalText: root.showOriginal
        baseFontSize: root.baseFontSize
    }

    // (b) Subtitle: full-width bar pinned to the bottom of the screen.
    TranslationBox {
        id: subtitleBox
        visible: root.showing && root.mode === "subtitle"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: DesktopTokens.px(16)
        anchors.rightMargin: DesktopTokens.px(16)
        anchors.bottomMargin: DesktopTokens.px(16)
        height: DesktopTokens.px(90)
        translated: root.translatedText
        original: root.originalText
        showOriginalText: root.showOriginal
        baseFontSize: root.baseFontSize
    }

    // Timing pill, only while a translation is shown. Sits just above the
    // active box, right-aligned, clamped inside the screen.
    Rectangle {
        id: statusPill
        visible: root.showing
        width: statusLabel.implicitWidth + DesktopTokens.px(20)
        height: DesktopTokens.px(24)
        radius: height / 2
        color: "black"
        opacity: 0.7
        x: root.mode === "inplace"
            ? Math.max(0, Math.min(root.boxX + root.boxW - width, root.width - width))
            : Math.max(0, root.width - width - DesktopTokens.px(16))
        y: root.mode === "inplace"
            ? Math.max(0, root.boxY - height - DesktopTokens.px(6))
            : Math.max(0, root.height - DesktopTokens.px(90 + 16) - height - DesktopTokens.px(6))
        Text {
            id: statusLabel
            anchors.centerIn: parent
            text: root.timingText
            color: "white"
            font.family: Theme.monoFont
            font.pixelSize: DesktopTokens.px(11)
        }
    }
}
