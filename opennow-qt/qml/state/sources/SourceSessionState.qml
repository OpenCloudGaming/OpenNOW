import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property var playback
    required property var sources
    required property bool ready

    readonly property int queuePollMs: 2000
    readonly property int streamPollMs: 10000

    property string sourceId: ""
    property string title: ""
    property string artworkUrl: ""
    property var session: null
    property string sessionHandle: ""
    property string phase: ""
    property string message: ""
    property bool adopted: false
    property string createRequestId: ""
    property string pollRequestId: ""
    property string stopRequestId: ""
    property string currentRequestId: ""
    property string startId: ""

    readonly property bool active: phase !== "" && phase !== "ended"
    readonly property bool mediaActive: startId !== ""
    readonly property string remoteState: session && session.state ? String(session.state.state || "") : ""
    readonly property var queuePosition: session && session.state && Number.isSafeInteger(session.state.position)
        ? session.state.position : null
    readonly property var queueWaitSeconds: session && session.state && Number.isSafeInteger(session.state.waitSeconds)
        ? session.state.waitSeconds : null

    signal streamRequested(string startId)
    signal streamStopRequested()
    signal finished(string message)

    property Timer pollTimer: Timer {
        repeat: false
        onTriggered: root.poll()
    }

    onReadyChanged: {
        if (ready)
            refreshCurrent()
        else
            pollTimer.stop()
    }

    function playable(source) {
        return source !== null && source.builtin !== true && source.playback === "media-worker-v1"
            && sources.has(source, "launch.v2") && sources.has(source, "sessions.v2")
            && playback !== null && playback !== undefined
    }

    function reset() {
        pollTimer.stop()
        sourceId = ""
        title = ""
        artworkUrl = ""
        session = null
        sessionHandle = ""
        phase = ""
        message = ""
        adopted = false
        createRequestId = ""
        pollRequestId = ""
        stopRequestId = ""
        startId = ""
    }

    function play(id, details, decision) {
        const source = sources.sourceById(id)
        if (active || !ready || !playable(source) || !details || !decision || decision.state !== "ready"
                || !decision.target || typeof decision.revision !== "string")
            return false
        const requestId = playback.create(id, {
            scope: sources.accountScope(id),
            target: decision.target,
            catalogRevision: decision.revision
        })
        if (requestId === "")
            return false
        reset()
        sourceId = id
        title = String(details.game && details.game.title || "")
        artworkUrl = String(details.game && details.game.imageUrl || "")
        createRequestId = requestId
        phase = "requesting"
        message = qsTr("Requesting a session from %1…").arg(String(source.name || source.id))
        return true
    }

    function sessionView(value) {
        return value && value.key && typeof value.key.remoteId === "string" && value.state
            && typeof value.state.state === "string" ? value : null
    }

    function acceptSession(view) {
        session = view
        const state = remoteState
        if (state === "finished" || state === "failed") {
            end(state === "failed" ? qsTr("The service could not start this game.")
                : qsTr("The session ended."))
            return
        }
        if (phase === "stopping" || phase === "failed")
            return
        if (state === "ready") {
            if (startId === "" && !adopted)
                startStream()
            else if (startId === "") {
                phase = "ready"
                message = qsTr("Your session is still running. Resume to keep playing.")
            }
            schedulePoll(streamPollMs)
            return
        }
        phase = state === "suspended" ? "suspended" : "queued"
        message = state === "suspended" ? qsTr("The session is paused by the service.")
            : queuePosition !== null ? qsTr("Position %1 in queue").arg(queuePosition)
            : qsTr("Waiting for the service to prepare your game…")
        schedulePoll(queuePollMs)
    }

    function schedulePoll(interval) {
        pollTimer.interval = interval
        pollTimer.restart()
    }

    function poll() {
        if (!active || !session || pollRequestId !== "" || stopRequestId !== "")
            return
        pollRequestId = coreClient.request("sources.session.poll", {sourceId: sourceId, request: session.key}, 30000)
        if (pollRequestId === "")
            schedulePoll(queuePollMs)
    }

    function startStream() {
        if (!session || sessionHandle === "" || startId !== "")
            return false
        const id = playback.start(sourceId, session.key, sessionHandle)
        if (id === "") {
            phase = "failed"
            message = qsTr("OpenNOW could not start the media runtime for this game.")
            return false
        }
        startId = id
        phase = "connecting"
        message = qsTr("Connecting to your game…")
        streamRequested(id)
        return true
    }

    function retryStream() {
        if (phase !== "failed" || !session || remoteState !== "ready" || sessionHandle === "")
            return false
        adopted = false
        return startStream()
    }

    function resume() {
        adopted = false
        return remoteState === "ready" && startStream()
    }

    function stop() {
        if (!active)
            return false
        pollTimer.stop()
        if (createRequestId !== "") {
            const id = createRequestId
            createRequestId = ""
            playback.cancel(id)
        }
        if (pollRequestId !== "") {
            const id = pollRequestId
            pollRequestId = ""
            coreClient.cancel(id)
        }
        if (startId !== "") {
            if (!playback.cancel(startId))
                streamStopRequested()
            startId = ""
        }
        if (!session || sessionHandle === "") {
            end("")
            return true
        }
        if (stopRequestId !== "")
            return true
        phase = "stopping"
        message = qsTr("Closing your session…")
        stopRequestId = coreClient.request("sources.session.stop", {
            sourceId: sourceId,
            request: {session: session.key, operation: sessionHandle}
        }, 35000)
        if (stopRequestId === "")
            end(qsTr("OpenNOW could not close the session. It will be cleaned up when the service is available."))
        return true
    }

    function end(text) {
        const note = String(text || "")
        reset()
        finished(note)
    }

    function streamFailed(text) {
        if (!active || startId === "")
            return
        startId = ""
        phase = "failed"
        message = String(text || qsTr("The game stream stopped."))
    }

    function refreshCurrent() {
        if (!ready || active || currentRequestId !== "")
            return
        currentRequestId = coreClient.request("sources.session.current", {})
    }

    function adopt(record) {
        if (active || !record || typeof record.sourceId !== "string" || typeof record.sessionHandle !== "string")
            return
        const source = sources.sourceById(record.sourceId)
        if (!playable(source) || !record.session || typeof record.session.remoteId !== "string")
            return
        sourceId = record.sourceId
        title = String(source.name || source.id)
        sessionHandle = record.sessionHandle
        adopted = true
        phase = "queued"
        message = qsTr("Checking your session on %1…").arg(title)
        session = {key: record.session, target: null, state: {state: "allocating"}}
        poll()
    }

    function acceptCreated(requestId, result) {
        if (requestId === "" || requestId !== createRequestId)
            return false
        createRequestId = ""
        const body = result && result.sourceId === sourceId ? result.result : null
        const view = body ? sessionView(body.session) : null
        if (!view || typeof body.sessionHandle !== "string" || body.sessionHandle === "") {
            phase = "failed"
            message = qsTr("The service returned an invalid session.")
            return true
        }
        sessionHandle = body.sessionHandle
        acceptSession(view)
        return true
    }

    function acceptPlaybackFailure(requestId, code, text) {
        if (requestId === "" || (requestId !== createRequestId && requestId !== startId))
            return false
        if (requestId === startId) {
            streamFailed(text)
            return true
        }
        createRequestId = ""
        if (code === "cancelled")
            return true
        phase = "failed"
        message = String(text || qsTr("The service could not start a session."))
        return true
    }

    function acceptResponse(requestId, result) {
        if (requestId === "")
            return false
        if (requestId === currentRequestId) {
            currentRequestId = ""
            const record = result ? result.session : null
            if (record)
                adopt(record)
            return true
        }
        if (requestId === pollRequestId) {
            pollRequestId = ""
            const view = result && result.sourceId === sourceId ? sessionView(result.result) : null
            if (view && session && view.key.remoteId === session.key.remoteId)
                acceptSession(view)
            else if (active)
                schedulePoll(queuePollMs)
            return true
        }
        if (requestId === stopRequestId) {
            stopRequestId = ""
            const state = result && result.result ? String(result.result.state || "") : ""
            end(state === "unknown" ? qsTr("The service has not confirmed that the session closed.") : "")
            return true
        }
        return false
    }

    function acceptFailure(requestId, code, text) {
        if (requestId === "")
            return false
        if (requestId === currentRequestId) {
            currentRequestId = ""
            return true
        }
        if (requestId === pollRequestId) {
            pollRequestId = ""
            if (code === "cancelled")
                return true
            if (code === "session_owner_mismatch") {
                end(qsTr("This session is no longer available."))
                return true
            }
            schedulePoll(queuePollMs)
            return true
        }
        if (requestId === stopRequestId) {
            stopRequestId = ""
            end(String(text || qsTr("OpenNOW could not close the session.")))
            return true
        }
        return false
    }

    function acceptSessionChanged(payload) {
        if (!active || !payload || payload.sourceId !== sourceId)
            return
        pollTimer.stop()
        poll()
    }

    function acceptCleanup(payload) {
        if (!payload || payload.sourceId !== sourceId)
            return
        message = qsTr("The service has not confirmed that the session closed.")
    }
}
