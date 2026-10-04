import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property var setSetting
    property bool ready: false
    property bool signedIn: false
    property var settings: ({})
    property string sessionId: ""
    property bool streaming: false
    property var clock: () => Date.now()

    readonly property string choice: String(settings.automaticBugReports || "unset")
    readonly property bool enabled: choice !== "disabled"
    readonly property bool noticePending: ready && signedIn && choice === "unset"
    readonly property int frameDropThreshold: 300
    readonly property int frameDropWindowMs: 60000

    property var latest: null
    property int generation: 0
    property var frameDropSamples: []
    property bool frameDropsReported: false

    onSessionIdChanged: {
        frameDropSamples = []
        frameDropsReported = false
    }

    function setEnabled(value) {
        setSetting("automaticBugReports", value ? "enabled" : "disabled")
    }

    function observeFrameDrops(count, eventSessionId) {
        if (!enabled || !streaming || frameDropsReported || sessionId === ""
                || (eventSessionId && String(eventSessionId) !== sessionId)
                || !Number.isSafeInteger(count) || count <= 0)
            return
        const now = clock()
        const samples = frameDropSamples.filter(sample => now - sample.at < frameDropWindowMs)
        samples.push({at: now, count: count})
        frameDropSamples = samples
        const dropped = samples.reduce((total, sample) => total + sample.count, 0)
        if (dropped < frameDropThreshold)
            return
        frameDropsReported = true
        report("frame_drops", "sustained_frame_drops",
            dropped + " video frames dropped within a minute",
            {droppedFrames: dropped, windowSeconds: frameDropWindowMs / 1000})
    }

    function reportStreamError(code, message) {
        const normalized = String(code || "native_stream_error").toLowerCase().replace(/[^a-z0-9_]/g, "_").slice(0, 64)
        report("stream_error", normalized || "native_stream_error", String(message || ""), {})
    }

    function report(kind, code, message, metrics) {
        if (!ready || !signedIn || !enabled)
            return
        coreClient.request("bug_report.incident", {kind: kind, code: code,
            message: String(message).slice(0, 2000), metrics: metrics}, 15000)
    }

    function acceptEvent(payload) {
        if (!payload || ["sending", "sent", "failed"].indexOf(String(payload.state)) < 0)
            return
        latest = payload
        generation += 1
    }

    function dismiss() {
        latest = null
    }
}
