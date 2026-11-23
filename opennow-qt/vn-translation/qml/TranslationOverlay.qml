import QtQuick
import OpenNOW

// Translation overlay: renders live EN -> VI translations directly over the
// original English text on the stream using Feathered Dark Inpainting (khảm
// tối mờ biên tàng hình) and outlined game-matched typography.
//
// Performance & Stream Integrity Guarantee (HP EliteBook 840 G1 / HD 4400):
//  - NEVER uses ShaderEffectSource or MultiEffect on StreamVideoItem, so Qt's
//    scene graph never redirects the 60FPS video stream through an offscreen
//    FBO texture. Zero video-pipeline interference, zero bitrate/FPS drop.
//  - Never captures keyboard/mouse/controller input (enabled: false).
Item {
    id: root
    anchors.fill: parent
    z: 6
    enabled: false

    readonly property bool hasEngine: typeof translationEngine !== "undefined" && translationEngine !== null
    readonly property bool engineOn: root.hasEngine && translationEngine.enabled
    readonly property bool hasText: root.hasEngine && translationEngine.hasTranslation
    readonly property var blocks: root.hasEngine ? translationEngine.translationBlocks : []
    readonly property string mode: !root.hasEngine ? "off"
        : String(translationEngine.overlayMode || "inplace")
    readonly property bool parentStreaming: parent && parent.streaming !== undefined
        ? Boolean(parent.streaming) : true
    readonly property bool showing: root.engineOn && root.hasText && root.mode !== "off" && root.parentStreaming

    visible: root.showing

    readonly property int baseFontSize: root.hasEngine ? Number(translationEngine.fontSizePx || 22) : 22
    readonly property bool showOriginal: root.hasEngine && translationEngine.showOriginal === true
    readonly property bool showTimingPill: root.hasEngine && translationEngine.showTimingPill === true

    readonly property string blockTranslatedText: root.blocks.map(function(b) {
        return String(b.translated || "")
    }).join("\n")
    readonly property string blockOriginalText: root.blocks.map(function(b) {
        return String(b.original || "")
    }).join("\n")

    readonly property string timingText: {
        if (!root.hasEngine)
            return ""
        const ocr = Number(translationEngine.lastOcrMs || 0)
        const tr = Number(translationEngine.lastTranslateMs || 0)
        let text = qsTr("OCR %1ms · Dịch %2ms").arg(ocr).arg(tr)
        if (translationEngine.lastChunkHit === true)
            text += " · " + qsTr("Chunk hit")
        else if (translationEngine.lastCacheHit === true)
            text += " · " + qsTr("Cache hit")
        return text
    }

    // Feathered Dark Inpaint block:
    // Replaces the original English glyphs with a soft, borderless dark
    // capsule gradient (alpha 0 at edges -> 0.88 at the glyph centerline)
    // tinted with the local background color sampled from the video frame,
    // and draws outlined Be Vietnam Pro text on top in the original text's
    // sampled color (off-white #F4F6FA or warm gold #FFE066).
    component InpaintBlock: Item {
        id: blockItem
        required property string translated
        required property string original
        required property bool showOriginalText
        required property int targetFontSize
        required property real origPixelW
        required property real origPixelH
        required property string fgColorHex
        required property string bgColorHex

        readonly property color baseBg: Qt.color(blockItem.bgColorHex || "#0B0D14")
        readonly property color baseFg: Qt.color(blockItem.fgColorHex || "#F4F6FA")
        readonly property bool isMultiLine: blockItem.origPixelH > blockItem.targetFontSize * 1.85
            || blockItem.translated.indexOf("\n") >= 0

        // Soft outer feather aura: dissolves any boundary between the game
        // scene and the dark inpaint core so the eye never sees a box edge.
        Rectangle {
            id: outerFeather
            anchors.fill: parent
            anchors.leftMargin: -DesktopTokens.px(10)
            anchors.rightMargin: -DesktopTokens.px(10)
            anchors.topMargin: -DesktopTokens.px(5)
            anchors.bottomMargin: -DesktopTokens.px(5)
            radius: height / 2
            color: "transparent"
            gradient: Gradient {
                orientation: Gradient.Vertical
                GradientStop { position: 0.00; color: Qt.rgba(blockItem.baseBg.r, blockItem.baseBg.g, blockItem.baseBg.b, 0.00) }
                GradientStop { position: 0.25; color: Qt.rgba(blockItem.baseBg.r, blockItem.baseBg.g, blockItem.baseBg.b, 0.34) }
                GradientStop { position: 0.50; color: Qt.rgba(blockItem.baseBg.r, blockItem.baseBg.g, blockItem.baseBg.b, 0.48) }
                GradientStop { position: 0.75; color: Qt.rgba(blockItem.baseBg.r, blockItem.baseBg.g, blockItem.baseBg.b, 0.34) }
                GradientStop { position: 1.00; color: Qt.rgba(blockItem.baseBg.r, blockItem.baseBg.g, blockItem.baseBg.b, 0.00) }
            }
        }

        // Core dark inpaint mask: covers the bright original English glyphs
        // along the text baseline with a smooth vertical gradient and pill
        // ends (never a sharp rectangle).
        Rectangle {
            id: inpaintCore
            anchors.fill: parent
            radius: Math.min(height / 2, DesktopTokens.px(18))
            color: "transparent"
            gradient: Gradient {
                orientation: Gradient.Vertical
                GradientStop { position: 0.00; color: Qt.rgba(blockItem.baseBg.r, blockItem.baseBg.g, blockItem.baseBg.b, 0.08) }
                GradientStop { position: 0.18; color: Qt.rgba(blockItem.baseBg.r, blockItem.baseBg.g, blockItem.baseBg.b, 0.82) }
                GradientStop { position: 0.50; color: Qt.rgba(blockItem.baseBg.r, blockItem.baseBg.g, blockItem.baseBg.b, 0.90) }
                GradientStop { position: 0.82; color: Qt.rgba(blockItem.baseBg.r, blockItem.baseBg.g, blockItem.baseBg.b, 0.82) }
                GradientStop { position: 1.00; color: Qt.rgba(blockItem.baseBg.r, blockItem.baseBg.g, blockItem.baseBg.b, 0.08) }
            }
        }

        Column {
            anchors.fill: parent
            anchors.leftMargin: DesktopTokens.px(6)
            anchors.rightMargin: DesktopTokens.px(6)
            anchors.topMargin: DesktopTokens.px(2)
            anchors.bottomMargin: DesktopTokens.px(2)
            spacing: DesktopTokens.px(2)

            Text {
                id: translatedLabel
                width: parent.width
                height: parent.height - (originalLabel.visible ? originalLabel.height + parent.spacing : 0)
                text: blockItem.translated
                color: blockItem.baseFg
                font.family: "Be Vietnam Pro"
                font.pixelSize: blockItem.targetFontSize
                font.weight: Font.SemiBold
                style: Text.Outline
                styleColor: "#E805060A"
                fontSizeMode: Text.Fit
                minimumPixelSize: 11
                wrapMode: Text.WordWrap
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }

            Text {
                id: originalLabel
                visible: blockItem.showOriginalText && blockItem.original !== ""
                width: parent.width
                text: blockItem.original
                color: Qt.rgba(0.88, 0.90, 0.95, 0.68)
                font.family: "Be Vietnam Pro"
                font.pixelSize: Math.max(10, Math.round(blockItem.targetFontSize * 0.58))
                style: Text.Outline
                styleColor: "#CC05060A"
                wrapMode: Text.WordWrap
                elide: Text.ElideRight
                maximumLineCount: 2
            }
        }
    }

    // (a) In-place: one feathered dark-inpaint Vietnamese block per English
    // line/paragraph, matched to the original English line's position, size,
    // and color.
    Repeater {
        id: blockRepeater
        model: root.showing && root.mode === "inplace" ? root.blocks : []
        delegate: InpaintBlock {
            required property var modelData
            readonly property real rawX: Number(modelData.x || 0) * root.width
            readonly property real rawY: Number(modelData.y || 0) * root.height
            readonly property real rawW: Number(modelData.w || 0) * root.width
            readonly property real rawH: Number(modelData.h || 0) * root.height

            // Automatically match the original English line's pixel height for
            // single-line labels/headers, bounded by baseFontSize so paragraphs
            // stay crisp and readable.
            readonly property int autoFontPx: {
                if (rawH <= 0)
                    return root.baseFontSize
                if (rawH < root.baseFontSize * 1.75) {
                    return Math.round(Math.max(12, Math.min(root.baseFontSize * 1.25, rawH * 0.82)))
                }
                return root.baseFontSize
            }

            readonly property real blockW: Math.max(DesktopTokens.px(44),
                Math.min(rawW + DesktopTokens.px(12), root.width))
            readonly property real blockH: Math.max(autoFontPx + DesktopTokens.px(8),
                Math.min(rawH + DesktopTokens.px(8), root.height))

            x: Math.max(0, Math.min(rawX - DesktopTokens.px(6), root.width - blockW))
            y: Math.max(0, Math.min(rawY - DesktopTokens.px(4), root.height - blockH))
            width: blockW
            height: blockH

            origPixelW: rawW
            origPixelH: rawH
            targetFontSize: autoFontPx
            translated: String(modelData.translated || "")
            original: String(modelData.original || "")
            showOriginalText: root.showOriginal
            fgColorHex: String(modelData.fgColor || "#F4F6FA")
            bgColorHex: String(modelData.bgColor || "#0B0D14")
        }
    }

    readonly property real topBlockY: {
        let top = root.height
        for (let i = 0; i < root.blocks.length; i++) {
            const y = Number(root.blocks[i].y || 0) * root.height
            if (y < top)
                top = y
        }
        return top
    }

    // (b) Subtitle: feathered dark subtitle bar at the bottom of the screen.
    InpaintBlock {
        id: subtitleBox
        visible: root.showing && root.mode === "subtitle"
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        anchors.leftMargin: DesktopTokens.px(32)
        anchors.rightMargin: DesktopTokens.px(32)
        anchors.bottomMargin: DesktopTokens.px(18)
        height: DesktopTokens.px(84)
        origPixelW: width
        origPixelH: height
        targetFontSize: root.baseFontSize
        translated: root.blockTranslatedText
        original: root.blockOriginalText
        showOriginalText: root.showOriginal
        fgColorHex: "#F4F6FA"
        bgColorHex: "#0B0D14"
    }

    // Optional timing pill (OFF by default so gameplay looks 100% native).
    Rectangle {
        id: statusPill
        visible: root.showing && root.showTimingPill
        width: statusLabel.implicitWidth + DesktopTokens.px(18)
        height: DesktopTokens.px(22)
        radius: height / 2
        color: "#CC0B0D14"
        x: Math.max(0, root.width - width - DesktopTokens.px(16))
        y: root.mode === "inplace"
            ? Math.max(0, root.topBlockY - height - DesktopTokens.px(6))
            : Math.max(0, root.height - DesktopTokens.px(84 + 18) - height - DesktopTokens.px(6))
        Text {
            id: statusLabel
            anchors.centerIn: parent
            text: root.timingText
            color: "#D8DCE6"
            font.family: Theme.monoFont
            font.pixelSize: DesktopTokens.px(11)
        }
    }
}
