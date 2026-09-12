import QtQuick

QtObject {
    id: root
    required property var coreClient
    required property bool ready
    property var artworkUrls: ({})
    property var artworkPending: ({})
    property var artworkRequestSources: ({})
    property var artworkRetrySources: ({})
    property var artworkInterests: ({})
    property Timer artworkRetryTimer: Timer {
        interval: 1000
        repeat: true
        running: root.ready && Object.keys(root.artworkRetrySources).length > 0
        onTriggered: root.retryVisibleArtwork(Date.now())
    }

    function artworkUrl(sourceUrl) {
        const source = String(sourceUrl || "")
        if (source === "")
            return ""
        if (!/^https?:\/\//i.test(source))
            return source
        const resolved = artworkUrls[source]
        if (resolved !== undefined)
            return String(resolved)
        return ""
    }

    function retainArtwork(source) {
        if (!/^https?:\/\//i.test(source)) return
        const interests = Object.assign({}, artworkInterests)
        interests[source] = (interests[source] || 0) + 1
        artworkInterests = interests
    }

    function releaseArtwork(source) {
        const interests = Object.assign({}, artworkInterests)
        if ((interests[source] || 0) > 1) interests[source]--
        else {
            delete interests[source]
            const retries = Object.assign({}, artworkRetrySources)
            delete retries[source]
            artworkRetrySources = retries
        }
        artworkInterests = interests
    }

    function scheduleArtworkRetry(source) {
        if (!artworkInterests[source]) return
        const retries = Object.assign({}, artworkRetrySources)
        const failures = Math.min(7, (retries[source] ? retries[source].failures : 0) + 1)
        retries[source] = {failures: failures, nextAt: Date.now() + Math.min(1800000, 30000 * Math.pow(2, failures - 1))}
        artworkRetrySources = retries
    }

    function retryVisibleArtwork(now) {
        // Pace retries independently of the number of failed posters on screen.
        let budget = 2
        for (const source of Object.keys(artworkRetrySources)) {
            if (!budget) break
            const retry = artworkRetrySources[source]
            if (!artworkInterests[source] || retry.nextAt > now || artworkPending[source]) continue
            // A pending cache job completes via artwork.resolved; don't repeatedly
            // query it while waiting. A missed event can be retried after backoff.
            retry.nextAt = now + Math.min(1800000, 30000 * Math.pow(2, retry.failures - 1))
            requestArtwork(source, true)
            budget--
        }
    }

    function requestArtwork(sourceUrl, retryDue) {
        const source = String(sourceUrl || "")
        const resolved = artworkUrls[source]
        if (!ready || !/^https?:\/\//i.test(source) || artworkPending[source]
                || (!retryDue && artworkRetrySources[source])
                || (resolved !== undefined && String(resolved) !== source))
            return ""
        const requestId = coreClient.request("artwork.resolve", { sourceUrl: source }, 2000)
        if (requestId === "")
            return ""
        const pending = Object.assign({}, artworkPending)
        const requests = Object.assign({}, artworkRequestSources)
        pending[source] = true
        requests[requestId] = source
        artworkPending = pending
        artworkRequestSources = requests
        return requestId
    }

    function finishArtworkRequest(requestId, result, failed) {
        const source = artworkRequestSources[requestId]
        if (source === undefined)
            return false
        const requests = Object.assign({}, artworkRequestSources)
        const pending = Object.assign({}, artworkPending)
        delete requests[requestId]
        delete pending[source]
        artworkRequestSources = requests
        artworkPending = pending
        const resolved = result && result.cached === true && result.artworkUrl
            ? String(result.artworkUrl) : ""
        if (resolved !== "") {
            const urls = Object.assign({}, artworkUrls)
            urls[source] = resolved
            artworkUrls = urls
            const retries = Object.assign({}, artworkRetrySources)
            delete retries[source]
            artworkRetrySources = retries
        }
        if (failed || (result && result.cached === false)) {
            scheduleArtworkRetry(source)
        }
        return true
    }

    function acceptArtworkResult(payload) {
        const source = String(payload && payload.sourceUrl || "")
        const resolved = payload && payload.cached === true
            ? String(payload.artworkUrl || "") : ""
        if (source === "")
            return
        const pending = Object.assign({}, artworkPending)
        delete pending[source]
        artworkPending = pending
        if (resolved !== "") {
            const urls = Object.assign({}, artworkUrls)
            urls[source] = resolved
            artworkUrls = urls
        }
        const retries = Object.assign({}, artworkRetrySources)
        if (payload.cached === false) {
            scheduleArtworkRetry(source)
        } else {
            delete retries[source]
            artworkRetrySources = retries
        }
    }
}
