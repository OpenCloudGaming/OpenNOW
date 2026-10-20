import QtQuick
import OpenNOW

// Text with a CSS-style line box: the glyphs sit centred in lineHeightPx. Qt's FixedHeight puts
// each line's font box at the top of its line, so wrapped text shifts by the half-leading.
Text {
    id: root
    property int size: 14
    property int lineHeightPx: 18
    property bool mono: false
    property real tracking: 0
    readonly property bool wraps: wrapMode !== Text.NoWrap
    readonly property real halfLeading: wraps ? (lineHeightPx - metrics.height) / 2 : 0
    color: "#FFFFFF"
    font.family: mono ? DesktopTokens.monoFont : DesktopTokens.bodyFont
    font.pixelSize: size
    font.weight: Font.DemiBold
    font.letterSpacing: tracking * size
    lineHeightMode: wraps ? Text.FixedHeight : Text.ProportionalHeight
    lineHeight: wraps ? lineHeightPx : 1
    topPadding: halfLeading
    bottomPadding: -halfLeading
    height: wraps ? Math.max(1, lineCount) * lineHeightPx : lineHeightPx
    verticalAlignment: wraps ? Text.AlignTop : Text.AlignVCenter

    FontMetrics { id: metrics; font: root.font }
}
