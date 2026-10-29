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
    readonly property int decoderDropWindowMs: 5000
    readonly property int decoderDropThreshold: 30
    property var decoderDrops: []
    property bool decoderLimited: false

    onActiveChanged: {
        reset()
        decoderDrops = []
        decoderLimited = false
    }
    onSessionIdChanged: {
        reset()
        decoderDrops = []
        decoderLimited = false
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
        const recentDrops = decoderDrops.filter(drop => drop.at <= now && now - drop.at < decoderDropWindowMs)
        if (recentDrops.length !== decoderDrops.length)
            decoderDrops = recentDrops
        decoderLimited = recentDrops.reduce((total, drop) => total + drop.count, 0) >= decoderDropThreshold
    }

    function acceptDecoderDrops(count) {
        if (!active || !Number.isSafeInteger(count) || count <= 0)
            return
        const now = clock()
        decoderDrops = decoderDrops.concat([{at: now, count: count}])
        expire(now)
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

    function canClaimNotice() {
        const now = clock()
        expire(now)
        return status === "unstable" && !noticeClaimed
            && (lastNoticeAt === null || now - lastNoticeAt >= 30000)
    }

    function claimNotice() {
        if (!canClaimNotice()) return false
        noticeClaimed = true
        lastNoticeAt = clock()
        return true
    }

    property Timer freshnessTimer: Timer {
        interval: 250
        repeat: true
        running: root.active && (root.lastSampleAt !== null || root.decoderDrops.length > 0)
        onTriggered: root.expire(root.clock())
    }
}
