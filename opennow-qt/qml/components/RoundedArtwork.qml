import QtQuick
import QtQuick.Effects
import OpenNOW

Item {
    id: root
    property string artwork: ""
    property color fallbackColor: Theme.cartSteam
    property real cornerRadius: 34
    property real scrimStart: 0.28

    property bool loadStarted: false
    readonly property string normalizedArtwork: DesktopTokens.decodeArtworkUrl(root.artwork)
    onVisibleChanged: loadStarted = visible
    Component.onCompleted: if (visible)
        loadStarted = true

    ArtworkSource {
        id: artworkSource
        sourceUrl: root.normalizedArtwork
        active: root.loadStarted
    }

    Rectangle {
        id: roundedMask
        anchors.fill: parent
        radius: root.cornerRadius
        color: "white"
        visible: false
        layer.enabled: true
    }

    Item {
        anchors.fill: parent
        layer.enabled: true
        layer.smooth: true
        layer.effect: MultiEffect {
            maskEnabled: true
            maskSource: roundedMask
            maskThresholdMin: 0.25
            maskSpreadAtMin: 0.2
        }

        Rectangle { anchors.fill: parent; color: root.fallbackColor }
        Image {
            anchors.fill: parent
            source: artworkSource.resolvedUrl
            fillMode: Image.PreserveAspectCrop
            sourceSize: Qt.size(Math.ceil(width), Math.ceil(height))
            asynchronous: true
            cache: true
            opacity: status === Image.Ready ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: Theme.heroDuration } }
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
