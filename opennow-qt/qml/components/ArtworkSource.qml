import QtQuick
import OpenNOW

QtObject {
    id: root
    property string sourceUrl: ""
    property bool active: true
    readonly property string resolvedUrl: active ? ShellStore.artworkUrl(sourceUrl) : ""

    function ensureRequested() {
        if (active && sourceUrl !== "")
            ShellStore.requestArtwork(sourceUrl)
    }

    onSourceUrlChanged: ensureRequested()
    onActiveChanged: ensureRequested()
    Component.onCompleted: ensureRequested()

    property Connections storeConnection: Connections {
        target: ShellStore
        function onReadyChanged() { root.ensureRequested() }
    }
}
