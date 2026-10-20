import QtQuick
import OpenNOW

Text {
    id: label
    property int size: 13
    property int line: Math.round(size * 1.3)
    property bool mono: false
    property real tracking: 0
    color: DesktopTokens.text
    font.family: mono ? DesktopTokens.monoFont : DesktopTokens.bodyFont
    font.pixelSize: DesktopTokens.px(size)
    font.weight: Font.Normal
    font.letterSpacing: DesktopTokens.px(size) * tracking
    lineHeightMode: Text.FixedHeight
    lineHeight: DesktopTokens.px(line)
    verticalAlignment: Text.AlignTop
    textFormat: Text.PlainText
    height: Math.max(1, lineCount) * lineHeight + topPadding + bottomPadding
    // FixedHeight lines put the glyph box at the top of each line; CSS centres it (half-leading).
    transform: Translate { y: Math.round((label.lineHeight - metrics.height) / 2) }
    FontMetrics { id: metrics; font: label.font }
}
