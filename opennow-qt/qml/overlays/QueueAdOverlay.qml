import QtQuick
import QtQuick.Controls
import QtMultimedia
import OpenNOW

FocusScope {
    id: root
    anchors.fill: parent
    focus: true
    readonly property var adState: ShellStore.activeSession ? (ShellStore.activeSession.adState || ({})) : ({})
    readonly property var ads: adState.sessionAds || adState.ads || []
    readonly property var ad: ads.length ? ads[0] : null
    readonly property string mediaUrl: {
        if (!ad) return ""
        const files = ad.adMediaFiles || []
        for (let index = 0; index < files.length; ++index)
            if (files[index].mediaFileUrl) return files[index].mediaFileUrl
        return ad.adUrl || ad.mediaUrl || ""
    }
    property bool started: false
    property bool completed: false

    Rectangle { anchors.fill: parent; color: "#05070D" }
    VideoOutput {
        id: output
        anchors.fill: parent
        fillMode: VideoOutput.PreserveAspectCrop
    }
    MediaPlayer {
        id: player
        source: root.mediaUrl
        videoOutput: output
        audioOutput: AudioOutput { volume: 1 }
        onPlaybackStateChanged: {
            if (playbackState === MediaPlayer.PlayingState && root.ad && !root.started) {
                root.started = true
                ShellStore.reportSessionAd("start", root.ad, 0, "")
            } else if (playbackState === MediaPlayer.PausedState && root.ad && root.started && !root.completed) {
                ShellStore.reportSessionAd("pause", root.ad, position, "")
            }
        }
        onMediaStatusChanged: {
            if (mediaStatus === MediaPlayer.LoadedMedia)
                play()
            else if (mediaStatus === MediaPlayer.EndOfMedia && root.ad && !root.completed) {
                root.completed = true
                ShellStore.reportSessionAd("finish", root.ad, duration, "")
            } else if (mediaStatus === MediaPlayer.InvalidMedia && root.ad && !root.completed) {
                root.completed = true
                ShellStore.reportSessionAd("cancel", root.ad, position, "error")
            }
        }
    }

    Rectangle {
        anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
        height: 190
        gradient: Gradient {
            GradientStop { position: 0; color: "transparent" }
            GradientStop { position: 1; color: Qt.rgba(0,0,0,0.9) }
        }
        Column {
            anchors.left: parent.left; anchors.right: parent.right; anchors.bottom: parent.bottom
            anchors.margins: 42; spacing: 10
            Text { text: qsTr("YOUR RIG IS GETTING READY"); color: Theme.mint; font.family: Theme.monoFont; font.pixelSize: 12; font.weight: Font.Black; font.letterSpacing: 1.5 }
            Text { text: root.ad ? (root.ad.title || root.adState.message || qsTr("A short message while you wait")) : qsTr("Preparing your session"); color: Theme.label; font.family: Theme.displayFont; font.pixelSize: 30; font.weight: Font.Black }
            ProgressBar { width: parent.width; from: 0; to: Math.max(1, player.duration); value: player.position }
        }
    }

    GlassButton {
        anchors.right: parent.right; anchors.top: parent.top; anchors.margins: 34
        text: player.playbackState === MediaPlayer.PlayingState ? qsTr("Pause") : qsTr("Resume"); glyph: "A"
        onClicked: {
            if (player.playbackState === MediaPlayer.PlayingState) player.pause()
            else { player.play(); if (root.started) ShellStore.reportSessionAd("resume", root.ad, player.position, "") }
        }
        Component.onCompleted: forceActiveFocus()
    }

    Component.onCompleted: if (root.mediaUrl) player.play()
}
