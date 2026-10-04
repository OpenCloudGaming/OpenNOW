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
    property string uiSurface: "desktop"
    property string gameId: ""
    property string decoderBackend: ""
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
    property var streamStats: ({})
    property string startedSessionId: ""
    property string endedSessionId: ""
    property bool appOpened: false

    onSessionIdChanged: {
        frameDropSamples = []
        frameDropsReported = false
        streamStats = ({})
    }

    function setEnabled(value, source) {
        setSetting("automaticBugReports", value ? "enabled" : "disabled", source || "settings")
    }

    function track(event, props) {
        if (!ready || !enabled)
            return ""
        return coreClient.request("analytics.track", {event: event, uiSurface: uiSurface,
            props: props || ({})}, 15000)
    }

    function openApp(runtimeCapabilities) {
        if (appOpened || !ready || !enabled)
            return
        appOpened = true
        coreClient.request("analytics.track", {event: "app_opened", uiSurface: uiSurface,
            runtimeCapabilities: runtimeCapabilities || ({})}, 15000)
    }

    function observeTelemetry(event) {
        if (!streaming || sessionId === "" || (event.sessionId && String(event.sessionId) !== sessionId))
            return
        const next = Object.assign({}, streamStats)
        for (const key of ["framesPerSecond", "pingMs", "packetLossPercent"]) {
            const value = event[key]
            if (value === undefined || value === null || !Number.isFinite(Number(value)) || Number(value) < 0)
                continue
            const total = next[key] || {sum: 0, count: 0}
            next[key] = {sum: total.sum + Number(value), count: total.count + 1}
        }
        streamStats = next
    }

    function average(key) {
        const total = streamStats[key]
        return total && total.count > 0 ? total.sum / total.count : undefined
    }

    function observeFirstFrame(props) {
        if (sessionId === "" || startedSessionId === sessionId)
            return
        startedSessionId = sessionId
        track("session_started", props)
    }

    function observeSessionEnd(endedId, props) {
        if (endedId === "" || endedSessionId === endedId)
            return
        endedSessionId = endedId
        track("session_ended", Object.assign({
            avg_fps: average("framesPerSecond"),
            avg_ping_ms: average("pingMs"),
            avg_packet_loss_pct: average("packetLossPercent")
        }, props))
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
        track("frame_drops_detected", {dropped: dropped, window_s: frameDropWindowMs / 1000,
            game_id: gameId, decoder_backend: decoderBackend})
        report("frame_drops", "sustained_frame_drops",
            dropped + " video frames dropped within a minute",
            {droppedFrames: dropped, windowSeconds: frameDropWindowMs / 1000})
    }

    function normalizedCode(code, fallback) {
        return String(code || fallback).toLowerCase().replace(/[^a-z0-9_]/g, "_").slice(0, 64) || fallback
    }

    function reportStreamError(code, message, stage) {
        const normalized = normalizedCode(code, "native_stream_error")
        track("session_error", {stage: stage || "stream", code: normalized, game_id: gameId})
        report("stream_error", normalized, String(message || ""), {})
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
