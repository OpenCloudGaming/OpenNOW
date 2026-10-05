import QtQuick
import OpenNOW

ConsoleChoiceSheet {
    id: root
    property var game: null
    property string tileSize: "square"
    property int homePosition: 0
    signal moveRequested()
    signal sizeRequested(string size)
    signal detailsRequested()
    signal removeRequested()
    signal closeRequested()

    eyebrow: qsTr("HOME")
    title: qsTr("Edit Home tile")
    description: root.game
        ? qsTr("%1 · Home position %2").arg(String(root.game.title || qsTr("Game"))).arg(root.homePosition + 1)
        : qsTr("Home tile")
    currentIndex: -1
    options: [
        { label: qsTr("Move tile"), value: "move", detail: qsTr("Pick it up, then use the D-pad to place it") },
        { label: qsTr("Square tile"), value: "square", detail: root.tileSize === "square" ? qsTr("Current size") : "" },
        { label: qsTr("Wide tile"), value: "wide", detail: root.tileSize === "wide" ? qsTr("Current size") : "" },
        { label: qsTr("Game details"), value: "details" },
        { label: qsTr("Remove from Home"), value: "remove", detail: qsTr("Still available in Library") }
    ]

    onChosen: index => {
        const value = root.options[index].value
        if (value === "move")
            root.moveRequested()
        else if (value === "square" || value === "wide")
            root.sizeRequested(value)
        else if (value === "details")
            root.detailsRequested()
        else if (value === "remove")
            root.removeRequested()
    }
    onDismissed: root.closeRequested()
}
