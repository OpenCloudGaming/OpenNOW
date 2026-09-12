import QtQuick
import OpenNOW

QtObject {
    id: root
    property string sourceUrl: ""
    property bool active: true
    property bool requestActive: active
    property string retainedSource: ""
    readonly property string resolvedUrl: active ? ShellStore.artworkUrl(sourceUrl) : ""

    function ensureRequested() {
        const wanted = active && requestActive ? sourceUrl : ""
        if (wanted !== retainedSource) {
            if (retainedSource !== "") ShellStore.releaseArtwork(retainedSource)
            retainedSource = wanted
            if (wanted !== "") ShellStore.retainArtwork(wanted)
        }
        if (wanted !== "")
            ShellStore.requestArtwork(sourceUrl)
    }

    onSourceUrlChanged: ensureRequested()
    onActiveChanged: ensureRequested()
    onRequestActiveChanged: ensureRequested()
    Component.onCompleted: ensureRequested()
    Component.onDestruction: if (retainedSource !== "") ShellStore.releaseArtwork(retainedSource)

    property Connections storeConnection: Connections {
        target: ShellStore
        function onReadyChanged() { root.ensureRequested() }
    }
}
