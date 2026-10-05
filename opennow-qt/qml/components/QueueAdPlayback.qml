import QtQuick
import QtMultimedia
import OpenNOW

MediaPlayer {
    id: root
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
    readonly property bool playing: playbackState === MediaPlayer.PlayingState
    property bool started: false
    property bool completed: false

    function toggle() {
        if (root.playing) {
            root.pause()
            return
        }
        root.play()
        if (root.started)
            ShellStore.reportSessionAd("resume", root.ad, root.position, "")
    }

    source: root.mediaUrl
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
    Component.onCompleted: if (root.mediaUrl) root.play()
}
