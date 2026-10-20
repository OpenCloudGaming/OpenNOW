import QtQuick
import QtQuick.Effects
import QtQuick.Window
import OpenNOW

// A user's Discord picture, clipped to a circle over the caller's coloured initial. Only
// cdn.discordapp.com URLs load, through the core's artwork cache; the picture appears once it has
// loaded, so the initial below shows while loading, after an error, without a URL and on the
// software renderer, which can't run the circular mask.
Item {
    id: root
    property string url: ""
    readonly property bool loaded: photo.status === Image.Ready
    readonly property int pixels: Math.max(16, Math.ceil(width * Screen.devicePixelRatio))
    readonly property int requestSize: Math.min(4096, Math.pow(2, Math.ceil(Math.log(pixels) / Math.LN2)))
    readonly property string source: url.indexOf("https://cdn.discordapp.com/") === 0 && width > 0
        ? url.split(/[?#]/)[0] + "?size=" + requestSize : ""
    Accessible.ignored: true

    ArtworkSource {
        id: artwork
        sourceUrl: root.source
        active: root.source !== ""
        requestActive: root.visible
    }
    Rectangle {
        id: mask
        anchors.fill: parent
        radius: width / 2
        visible: false
        layer.enabled: true
    }
    Item {
        anchors.fill: parent
        visible: root.loaded && GraphicsInfo.api !== GraphicsInfo.Software
        layer.enabled: true
        layer.smooth: true
        layer.effect: MultiEffect {
            maskEnabled: true
            maskSource: mask
            maskThresholdMin: 0.25
            maskSpreadAtMin: 0.2
        }
        Image {
            id: photo
            objectName: "avatarPhoto"
            anchors.fill: parent
            source: artwork.resolvedUrl
            sourceSize: Qt.size(root.requestSize, root.requestSize)
            fillMode: Image.PreserveAspectCrop
            asynchronous: true
            cache: true
            smooth: true
        }
    }
}
