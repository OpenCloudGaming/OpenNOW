pragma ComponentBehavior: Bound
import QtQuick
import QtQuick.Controls
import OpenNOW

GridView {
    id: root
    property bool playHints: true
    property var noteForGame: null
    signal gameActivated(var game)
    signal gamePlayRequested(var game)
    signal gameContextRequested(var game, real sceneX, real sceneY)

    readonly property real tileScale: Math.max(0.75, Math.min(1.5, Number(ShellStore.settings.posterSizeScale || 1.05))) / 1.05
    readonly property int columns: Math.max(1, Math.floor((width + 10) / (156 * tileScale)))
    // The delegate's artwork keeps a 2:3 aspect inside a 6px gutter; derive the
    // cell from that geometry so the outline and overlay never clip at scale.
    cellWidth: Math.max(1, Math.floor(width / columns))
    cellHeight: Math.round((cellWidth - 12) * 198 / 132) + 12
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
    delegate: DesktopPoster {
        required property var modelData
        game: modelData
        tileWidth: root.cellWidth
        tileHeight: root.cellHeight
        playHint: root.playHints
        note: root.noteForGame ? root.noteForGame(modelData) : ""
        onClicked: root.gameActivated(modelData)
        onDoubleClicked: root.gamePlayRequested(modelData)
        onContextRequested: (sceneX, sceneY) => root.gameContextRequested(modelData, sceneX, sceneY)
    }
}
