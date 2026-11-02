package com.opencloudgaming.opennow

import android.os.SystemClock
import java.io.IOException
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import okhttp3.OkHttpClient

internal data class GfnServerInfo(
    val vpcId: String,
    val regions: List<StreamRegion>,
    val discoveryBaseUrl: String? = null,
)

internal class GfnServerInfoException(message: String, val retryable: Boolean) : IOException(message)

/** One provider-scoped owner for catalog identity and advertised routes, including outage backoff. */
class GfnServerInfoRepository internal constructor(
    private val fetch: suspend (token: String?, base: String) -> GfnServerInfo,
    private val elapsedMs: () -> Long = SystemClock::elapsedRealtime,
    private val fallbackBases: List<String> = emptyList(),
) {
    constructor(http: OkHttpClient) : this(
        fetch = { token, base -> requestGfnServerInfo(http, token, base) },
        fallbackBases = NVIDIA_DISCOVERY_BOOTSTRAP_BASES,
    )

    private data class Entry(
        val value: GfnServerInfo? = null,
        val fetchedAt: Long = 0,
        val retryAt: Long = 0,
        val failure: IOException? = null,
    )

    private val mutex = Mutex()
    private val entries = mutableMapOf<String, Entry>()

    internal suspend fun get(token: String?, streamingBaseUrl: String): GfnServerInfo {
        val base = normalizeStreamingServiceUrl(streamingBaseUrl)
            ?: throw GfnServerInfoException("Invalid GeForce NOW server URL", retryable = false)
        return mutex.withLock {
            currentCoroutineContext().ensureActive()
            val now = elapsedMs()
            val previous = entries[base]
            val usable = previous?.value?.takeIf { now - previous.fetchedAt < STALE_TTL_MS }
            if (previous?.failure != null && now < previous.retryAt) {
                return@withLock usable ?: throw previous.failure
            }
            if (previous?.value != null && now - previous.fetchedAt < FRESH_TTL_MS) {
                return@withLock previous.value
            }
            try {
                val info = discover(token, base)
                currentCoroutineContext().ensureActive()
                entries[base] = Entry(value = info, fetchedAt = elapsedMs())
                info
            } catch (error: CancellationException) {
                throw error
            } catch (error: IOException) {
                currentCoroutineContext().ensureActive()
                // An auth rejection must remain visible and must not poison a refreshed token.
                if (error is GfnServerInfoException && !error.retryable) {
                    entries.remove(base)
                    throw error
                }
                val failure = if (error is GfnServerInfoException) error else {
                    GfnServerInfoException("Could not reach GeForce NOW server discovery. Try again shortly.", true)
                }
                entries[base] = Entry(usable, previous?.fetchedAt ?: 0, elapsedMs() + RETRY_DELAY_MS, failure)
                usable ?: throw failure
            }
        }
    }

    /** Only automatic NVIDIA routing may follow discovery failover; explicit servers stay fixed. */
    internal suspend fun streamingBaseUrl(token: String, streamingBaseUrl: String): String {
        val base = normalizeStreamingServiceUrl(streamingBaseUrl) ?: return streamingBaseUrl
        return if (isNvidiaAutoDiscoveryBase(base)) get(token, base).discoveryBaseUrl ?: base else base
    }

    private suspend fun discover(token: String?, base: String): GfnServerInfo {
        try {
            return fetch(token, base)
        } catch (error: IOException) {
            currentCoroutineContext().ensureActive()
            if (!isNvidiaAutoDiscoveryBase(base) || fallbackBases.isEmpty() ||
                (error is GfnServerInfoException && !error.retryable)) throw error

            // The geo-routed prod endpoint can return 503 while regional front doors are healthy.
            // Obtain a real identity AND route from a live response, including on a cold start.
            return coroutineScope {
                val results = Channel<Result<GfnServerInfo>>(fallbackBases.size)
                val probes = fallbackBases.map { candidate ->
                    launch {
                        val result = try {
                            Result.success(fetch(token, candidate).copy(discoveryBaseUrl = candidate))
                        } catch (cancelled: CancellationException) {
                            throw cancelled
                        } catch (failure: IOException) {
                            Result.failure(failure)
                        }
                        results.send(result)
                    }
                }
                try {
                    repeat(probes.size) {
                        val result = results.receive()
                        result.getOrNull()?.let { return@coroutineScope it }
                        val failure = result.exceptionOrNull()
                        if (failure is GfnServerInfoException && !failure.retryable) throw failure
                    }
                    throw error
                } finally {
                    probes.forEach { it.cancel() }
                    results.cancel()
                }
            }
        }
    }

    internal suspend fun regions(token: String?, streamingBaseUrl: String): List<StreamRegion> = try {
        get(token, streamingBaseUrl).regions
    } catch (error: IOException) {
        emptyList()
    }

    private companion object {
        const val FRESH_TTL_MS = 5 * 60 * 1_000L
        const val STALE_TTL_MS = 30 * 60 * 1_000L
        const val RETRY_DELAY_MS = 5_000L
    }
}

private fun isNvidiaAutoDiscoveryBase(base: String): Boolean =
    base == "https://prod.cloudmatchbeta.nvidiagrid.net/"

// Regional front doors advertised by NVIDIA's live serverInfo metadata (verified 2026-11-02).
// These bootstrap discovery only after prod fails; server IDs and the full region list stay live.
private val NVIDIA_DISCOVERY_BOOTSTRAP_BASES = listOf(
    "https://ap-japan.cloudmatchbeta.nvidiagrid.net/",
    "https://eu-germany.cloudmatchbeta.nvidiagrid.net/",
    "https://us-oregon.cloudmatchbeta.nvidiagrid.net/",
)

internal fun parseGfnServerInfo(code: Int, text: String): GfnServerInfo {
    if (code !in 200..299) {
        throw GfnServerInfoException(
            "GeForce NOW server discovery failed (HTTP $code). Try again shortly.",
            retryable = code == 408 || code == 429 || code in 500..599,
        )
    }
    val data = runCatching { OpenNowJson.parseToJsonElement(text) as? JsonObject }.getOrNull()
    val status = data?.get("requestStatus") as? JsonObject
    val statusCode = (status?.get("statusCode") as? JsonPrimitive)?.intOrNull
    val vpcId = (status?.get("serverId") as? JsonPrimitive)?.takeIf { it.isString }?.contentOrNull?.trim()
    if (statusCode != 1 || vpcId.isNullOrBlank() || vpcId == "GFN-PC") {
        // GFN-PC is a client label, not a discovered server. Querying with it returns a
        // plausible nine-game catalog and an empty library, overwriting healthy saved data.
        throw GfnServerInfoException("GeForce NOW server discovery returned no valid server identity. Try again shortly.",
            retryable = statusCode == null || statusCode == 1)
    }
    val regions = (data["metaData"] as? JsonArray).orEmpty().mapNotNull { raw ->
        val item = raw as? JsonObject ?: return@mapNotNull null
        val key = (item["key"] as? JsonPrimitive)?.contentOrNull ?: return@mapNotNull null
        val value = (item["value"] as? JsonPrimitive)?.contentOrNull ?: return@mapNotNull null
        val url = normalizeStreamingServiceUrl(value) ?: return@mapNotNull null
        if (key == "gfn-regions" || key.startsWith("gfn-")) null else StreamRegion(key, url)
    }.distinct().sortedBy { it.name }
    return GfnServerInfo(vpcId, regions)
}
