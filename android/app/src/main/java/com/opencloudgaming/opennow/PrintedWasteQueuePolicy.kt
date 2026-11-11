package com.opencloudgaming.opennow

import android.os.SystemClock
import okhttp3.Headers
import java.net.URI

internal const val PRINTEDWASTE_MAX_QUEUE_AGE_SECONDS = 30 * 60L

/** Keep queue timestamps and their clock together; device wall-clock changes must not revive data. */
data class PrintedWasteQueueSnapshot(
    val zones: Map<String, PrintedWasteZone>,
    private val receivedEpochMs: Long,
    private val receivedElapsedMs: Long,
) {
    fun nowEpochMs(elapsedMs: Long = SystemClock.elapsedRealtime()): Long =
        receivedEpochMs + (elapsedMs - receivedElapsedMs).coerceAtLeast(0L)
}

internal fun printedWasteQueueSnapshot(
    zones: Map<String, PrintedWasteZone>,
    headers: Headers,
    receivedWallTimeMs: Long,
    receivedElapsedMs: Long,
): PrintedWasteQueueSnapshot {
    // The HTTPS response's Date shares the queue producer's clock. Never infer "now" from
    // the newest zone: a response containing only old zones must remain expired.
    val serverTimeMs = headers.getDate("Date")?.time?.takeIf { it > 0 }
    val ageSeconds = headers["Age"]?.toLongOrNull()?.takeIf { it in 0..Int.MAX_VALUE.toLong() } ?: 0L
    return PrintedWasteQueueSnapshot(
        zones = zones,
        receivedEpochMs = serverTimeMs?.plus(ageSeconds * 1000) ?: receivedWallTimeMs,
        receivedElapsedMs = receivedElapsedMs,
    )
}

/** PrintedWaste's "Last Updated" is Unix seconds, not the time we fetched the response. */
internal fun isPrintedWasteQueueFresh(lastUpdatedEpochSeconds: Long, nowEpochMs: Long): Boolean {
    val nowSeconds = nowEpochMs / 1000
    return lastUpdatedEpochSeconds > 0 && lastUpdatedEpochSeconds <= nowSeconds &&
        lastUpdatedEpochSeconds >= nowSeconds - PRINTEDWASTE_MAX_QUEUE_AGE_SECONDS
}

/** One policy for recommendations, visible choices, pings, and final launch validation. */
internal fun selectablePrintedWasteRegionalUrl(
    zoneId: String,
    zone: PrintedWasteZone,
    mapping: Map<String, PrintedWasteServerMappingEntry>,
    regions: List<StreamRegion>,
    nowEpochMs: Long,
): String? {
    if (printedWasteQueueUnavailableReason(zone, mapping[zoneId], nowEpochMs) != null) return null
    return printedWasteRegionalUrl(zoneId, mapping, regions)
}

internal enum class PrintedWasteUnavailableReason {
    MarkedUnavailable,
    MissingQueueData,
    InvalidQueueTime,
    StaleQueueData,
    MissingServerMapping,
    MissingRegionalRoute,
}

internal fun printedWasteQueueUnavailableReason(
    zone: PrintedWasteZone?,
    entry: PrintedWasteServerMappingEntry?,
    nowEpochMs: Long,
): PrintedWasteUnavailableReason? = when {
    entry?.nuked == true -> PrintedWasteUnavailableReason.MarkedUnavailable
    zone == null -> PrintedWasteUnavailableReason.MissingQueueData
    zone.LastUpdated <= 0 || zone.LastUpdated > nowEpochMs / 1000 -> PrintedWasteUnavailableReason.InvalidQueueTime
    !isPrintedWasteQueueFresh(zone.LastUpdated, nowEpochMs) -> PrintedWasteUnavailableReason.StaleQueueData
    entry?.title.isNullOrBlank() -> PrintedWasteUnavailableReason.MissingServerMapping
    else -> null
}

internal fun shouldUsePrintedWasteQueue(
    hideServerSelector: Boolean,
    providerCode: String,
    providerStreamingServiceUrl: String,
    effectiveStreamingBaseUrl: String,
    membershipTier: String?,
): Boolean {
    if (hideServerSelector) return false
    if (!providerCode.equals("NVIDIA", ignoreCase = true)) return false
    if (!membershipTier.isNullOrBlank() && !membershipTier.equals("FREE", ignoreCase = true)) return false
    return !isAllianceStreamingServiceUrl(providerStreamingServiceUrl) &&
        !isAllianceStreamingServiceUrl(effectiveStreamingBaseUrl)
}

private fun isAllianceStreamingServiceUrl(streamingServiceUrl: String): Boolean {
    val host = runCatching { URI(streamingServiceUrl).host.orEmpty() }.getOrDefault("")
    return host.isNotBlank() && !host.endsWith(".nvidiagrid.net", ignoreCase = true)
}
