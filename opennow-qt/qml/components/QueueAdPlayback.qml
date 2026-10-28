import QtQuick
import QtMultimedia
import OpenNOW

QtObject {
    id: root
    readonly property string sessionId: String(ShellStore.activeSession && ShellStore.activeSession.sessionId || "")
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
    readonly property string adIdentity: ad ? JSON.stringify([sessionId, String(ad.adId || "")]) : ""
    property var videoOutput: null
    property var player: null
    readonly property int playbackState: player ? player.playbackState : MediaPlayer.StoppedState
    readonly property real position: player ? player.position : 0
    readonly property real duration: player ? player.duration : 0
    readonly property bool playing: playbackState === MediaPlayer.PlayingState
    property bool started: false
    property bool completed: false
    property Timer admissionTimer: Timer {
        interval: 1000
        repeat: true
        onTriggered: root.beginAd()
    }

    function toggle() {
        if (root.playing) {
            root.pause()
            return
        }
        root.play()
    }

    function play() { if (player && !completed) player.play() }
    function pause() { if (player) player.pause() }

    function replaceAd() {
        admissionTimer.stop()
        const previous = player
        player = null
        started = false
        completed = false
        if (previous) {
            previous.stop()
            previous.destroy()
        }
        beginAd()
    }

    function beginAd() {
        if (!ad || !sessionId || player)
            return
        if (!ShellStore.prepareSessionAd(ad, sessionId)) {
            admissionTimer.start()
            return
        }
        admissionTimer.stop()
        player = playerComponent.createObject(root, {
            playbackAd: Object.assign({}, ad), playbackSessionId: sessionId,
            playbackIdentity: adIdentity
        })
        if (player && mediaUrl) player.play()
    }

    property Component playerComponent: Component {
        MediaPlayer {
            id: backend
            required property var playbackAd
            required property string playbackSessionId
            required property string playbackIdentity
            source: ownsPlayback() ? root.mediaUrl : ""
            videoOutput: root.videoOutput
            audioOutput: AudioOutput { volume: 1 }

            function ownsPlayback() {
                return root.player === backend && root.adIdentity === playbackIdentity
            }
            function report(action, watched, reason) {
                return ShellStore.reportSessionAd(action, playbackAd, watched, reason, playbackSessionId)
            }
            function acceptPlaybackState(state) {
                if (!ownsPlayback() || root.completed) return
                if (state === MediaPlayer.PlayingState) {
                    if (!root.started)
                        root.started = report("start", 0, "")
                    else
                        report("resume", position, "")
                } else if (state === MediaPlayer.PausedState && root.started) {
                    report("pause", position, "")
                }
            }
            function acceptMediaStatus(status) {
                if (!ownsPlayback() || root.completed) return
                if (status === MediaPlayer.LoadedMedia)
                    play()
                else if (status === MediaPlayer.EndOfMedia)
                    root.completed = report("finish", duration, "")
                else if (status === MediaPlayer.InvalidMedia)
                    root.completed = report("cancel", position, "error")
            }
            onPlaybackStateChanged: acceptPlaybackState(playbackState)
            onMediaStatusChanged: acceptMediaStatus(mediaStatus)
        }
    }
    onAdIdentityChanged: replaceAd()
    Component.onCompleted: if (!player) replaceAd()
}
