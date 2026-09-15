import QtQuick

QtObject {
    id: root
    signal sampleAccepted()
    property bool active: false
    property string sessionId: ""
    property var clock: () => Date.now()
    property string status: "unknown"
    property var lastSampleAt: null
    property var badSince: null
    property var lastLoss: null
    property var history: []
    property bool noticeClaimed: false
    property var lastNoticeAt: null

    onActiveChanged: reset()
    onSessionIdChanged: {
        reset()
        lastNoticeAt = null
    }

    function reset() {
        status = "unknown"
        lastSampleAt = null
        badSince = null
        lastLoss = null
        history = []
        noticeClaimed = false
    }

    function expire(now) {
        if (lastNoticeAt !== null && now < lastNoticeAt)
            lastNoticeAt = null
        if (lastSampleAt !== null && (now < lastSampleAt || now - lastSampleAt >= 5000))
            reset()
    }

    function acceptSample(value) {
        const now = clock()
        expire(now)
        if (!active || typeof value !== "number" || !Number.isFinite(value)
                || value < 0 || value > 100) {
            reset()
            return
        }
        lastSampleAt = now
        lastLoss = value
        history = history.concat([value]).slice(-12)
        if (value < 0.1) {
            badSince = null
            noticeClaimed = false
            status = "stable"
        } else if (value >= 0.5) {
            if (badSince === null) badSince = now
            if (now - badSince >= 2000) status = "unstable"
        } else {
            badSince = null
        }
        sampleAccepted()
    }

    function claimNotice() {
        const now = clock()
        expire(now)
        if (status !== "unstable" || noticeClaimed) return false
        if (lastNoticeAt !== null && now - lastNoticeAt < 30000)
            return false
        noticeClaimed = true
        lastNoticeAt = now
        return true
    }

    property Timer freshnessTimer: Timer {
        interval: 250
        repeat: true
        running: root.active && root.lastSampleAt !== null
        onTriggered: root.expire(root.clock())
    }
}
