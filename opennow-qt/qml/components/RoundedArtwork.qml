import QtQuick
import QtQuick.Effects
import OpenNOW

Item {
    id: root
    property string artwork: ""
    property string fallbackTitle: ""
    property color fallbackColor: Theme.cartSteam
    property real cornerRadius: 34
    property real scrimStart: 0.28

    property bool loadStarted: false
    readonly property string normalizedArtwork: DesktopTokens.decodeArtworkUrl(root.artwork)
    onNormalizedArtworkChanged: if (!visible) loadStarted = false
    // Retained popups keep their resolved artwork between opens, avoiding a
    // fallback flash and another image fade every time visibility changes.
    onVisibleChanged: if (visible) loadStarted = true
    Component.onCompleted: if (visible)
        loadStarted = true

    ArtworkSource {
        id: artworkSource
        sourceUrl: root.normalizedArtwork
        active: root.loadStarted
        requestActive: root.visible
    }

    Rectangle {
        anchors.fill: parent
        radius: root.cornerRadius
        color: root.fallbackColor
    }

    Text {
        anchors.centerIn: parent
        width: Math.max(0, parent.width - 20)
        visible: root.fallbackTitle !== "" && !image.hasArtwork
            && (root.normalizedArtwork === "" || artworkSource.failed || image.status === Image.Error)
        text: root.fallbackTitle
        textFormat: Text.PlainText
        wrapMode: Text.Wrap
        maximumLineCount: 3
        elide: Text.ElideRight
        horizontalAlignment: Text.AlignHCenter
        color: "#F6F7FB"
        font.family: Theme.bodyFont
        font.pixelSize: DesktopTokens.captionSize
        font.weight: Font.DemiBold
        Accessible.ignored: true
    }

    Rectangle {
        id: roundedMask
        anchors.fill: parent
        radius: root.cornerRadius
        color: "white"
        visible: false
        layer.enabled: artworkLayer.visible
    }

    Item {
        id: artworkLayer
        anchors.fill: parent
        visible: opacity > 0
        opacity: image.hasArtwork ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: AppController.reducedMotion ? 0 : Theme.heroDuration } }
        layer.enabled: visible
        layer.smooth: true
        layer.effect: MultiEffect {
            maskEnabled: true
            maskSource: roundedMask
            maskThresholdMin: 0.25
            maskSpreadAtMin: 0.2
        }

        Image {
            id: image
            property url readySource: ""
            readonly property bool hasArtwork: status === Image.Ready
                || (status === Image.Loading && readySource.toString() === source.toString())
            anchors.fill: parent
            source: artworkSource.resolvedUrl
            onStatusChanged: {
                if (status === Image.Ready) readySource = source
                artworkSource.reportImageStatus(status, source)
            }
            fillMode: Image.PreserveAspectCrop
            sourceSize: DesktopTokens.artworkSourceSize(width, height, Screen.devicePixelRatio)
            asynchronous: true
            cache: true
            retainWhileLoading: true
            visible: hasArtwork
        }
        Rectangle {
            anchors.fill: parent
            visible: root.scrimStart < 1
            gradient: Gradient {
                GradientStop { position: Math.max(0, Math.min(0.99, root.scrimStart)); color: "transparent" }
                GradientStop { position: 1; color: Qt.rgba(0, 0, 0, 0.82) }
            }
        }
    }
}
