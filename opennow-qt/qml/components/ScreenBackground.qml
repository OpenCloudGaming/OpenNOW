import QtQuick
import OpenNOW

Rectangle {
    id: root
    property string artwork: ""
    property color tint: "#111827"
    // Theme-aware scrim: the shell color carries the active theme pack, so
    // one system themes desktop and console together. The per-screen tint
    // survives only as a faint wash for identity — never as a dark crush.
    // Kept translucent enough that artwork breathes instead of drowning.
    readonly property color scrimMid: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.60)
    readonly property color scrimFar: Qt.rgba(Theme.shell.r, Theme.shell.g, Theme.shell.b, 0.88)
    anchors.fill: parent
    color: Theme.shell

    ArtworkSource {
        id: artworkSource
        sourceUrl: DesktopTokens.decodeArtworkUrl(root.artwork)
        active: root.visible
    }

    Image {
        anchors.fill: parent
        source: artworkSource.resolvedUrl
        fillMode: Image.PreserveAspectCrop
        sourceSize: Qt.size(Math.ceil(width), Math.ceil(height))
        asynchronous: true
        opacity: status === Image.Ready ? 0.58 : 0
        Behavior on opacity { NumberAnimation { duration: Theme.enterDuration } }
    }
    Rectangle {
        anchors.fill: parent
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Theme.shell }
            GradientStop { position: 0.48; color: root.scrimMid }
            GradientStop { position: 1; color: root.scrimFar }
        }
    }
    Rectangle {
        anchors.fill: parent
        color: root.tint
        opacity: 0.12
    }
    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.top: parent.top
        height: Math.round(parent.height * 0.4)
        gradient: Gradient {
            orientation: Gradient.Vertical
            GradientStop { position: 0; color: Qt.rgba(Theme.focus.r, Theme.focus.g, Theme.focus.b, 0.07) }
            GradientStop { position: 1; color: "transparent" }
        }
    }
}
