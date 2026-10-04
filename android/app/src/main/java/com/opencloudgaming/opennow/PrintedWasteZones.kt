package com.opencloudgaming.opennow

import java.util.Locale
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

/**
 * PrintedWaste zone identity, naming, and grouping.
 *
 * Zone id handling used to live in three places — the selector in `OpenNowCatalogControls.kt` and
 * two private copies in `OpenNowViewModel.kt` — each re-deriving the `NP-` prefix rule and the
 * CloudMatch host. This is the one owner; the UI decides presentation, and this decides what a zone
 * *is*.
 *
 * The provider's own mapping (`GFN_SERVERID_TO_REGION_MAPPING`) carries a human `title` and a
 * `region` for every server id, and OpenNOW already fetched both and then showed neither. Zone ids
 * like `NP-LAX-03` mean nothing to a player, and worse, several ids share one physical location:
 * `NP-LAX-02` and `NP-LAX-03` are both Southern California, so the list read as duplicates. Naming
 * and combining them is what these functions exist for.
 */

/** Alliance partner zones (`NPA-`) are not routable through the free queue. */
internal fun isStandardPrintedWasteZone(zoneId: String): Boolean =
    zoneId.startsWith("NP-") && !zoneId.startsWith("NPA-")

private val PROVIDER_REGION_SUFFIX = Regex("\\s*(?:\\((?:usa|canada)\\)|[12])$", RegexOption.IGNORE_CASE)

private fun printedWasteLocationKey(value: String): String =
    value.trim().lowercase(Locale.US).replace(PROVIDER_REGION_SUFFIX, "")

/** Route a chosen location through CloudMatch's advertised regional front door. */
internal fun printedWasteRegionalUrl(
    zoneId: String,
    mapping: Map<String, PrintedWasteServerMappingEntry>,
    regions: List<StreamRegion>,
): String? {
    if (!isStandardPrintedWasteZone(zoneId)) return null
    val title = mapping[zoneId]?.title?.trim()?.takeIf(String::isNotEmpty) ?: return null
    val location = printedWasteLocationKey(if (title.equals("Mumbai", ignoreCase = true)) "India" else title)
    return regions.firstNotNullOfOrNull { region ->
        if (printedWasteLocationKey(region.name) != location) return@firstNotNullOfOrNull null
        val url = region.url.toHttpUrlOrNull() ?: return@firstNotNullOfOrNull null
        val host = url.host.lowercase(Locale.US)
        if (url.scheme != "https" || url.port != 443 || url.encodedPath != "/" ||
            url.query != null || url.fragment != null || host.startsWith("np-") ||
            !host.endsWith(".cloudmatchbeta.nvidiagrid.net")) return@firstNotNullOfOrNull null
        "https://$host/"
    }
}

/** The GPU a zone advertises, when the mapping says. */
internal enum class PrintedWasteGpuTier(val label: String) {
    Rtx5080("RTX 5080"),
    Rtx4080("RTX 4080"),
}

internal fun printedWasteGpuTier(entry: PrintedWasteServerMappingEntry?): PrintedWasteGpuTier? = when {
    entry?.is5080Server == true -> PrintedWasteGpuTier.Rtx5080
    entry?.is4080Server == true -> PrintedWasteGpuTier.Rtx4080
    else -> null
}

/**
 * The name to show for a zone: the mapping's title, or the raw id when the mapping has no entry.
 *
 * Falling back to the id rather than to something like "Unknown" keeps a newly added server
 * selectable and still identifiable the day it appears, before the mapping catches up.
 */
internal fun printedWasteZoneTitle(zoneId: String, entry: PrintedWasteServerMappingEntry?): String =
    entry?.title?.trim()?.takeIf { it.isNotEmpty() } ?: zoneId

/**
 * The broad region heading a zone sits under, e.g. `US Southwest`.
 *
 * Prefers the mapping's `region` over the queue payload's coarser continent code, which is only
 * ever `US`, `EU`, `CA`, and so on — too blunt to group by once names are being shown.
 */
internal fun printedWasteZoneRegion(entry: PrintedWasteServerMappingEntry?, queueRegion: String): String =
    entry?.region?.trim()?.takeIf { it.isNotEmpty() } ?: printedWasteContinentLabel(queueRegion)

internal fun printedWasteContinentLabel(region: String): String = when (region.uppercase()) {
    "US" -> "North America"
    "CA" -> "Canada"
    "EU" -> "Europe"
    "JP" -> "Japan"
    "KR" -> "South Korea"
    "THAI" -> "Southeast Asia"
    "MY" -> "Malaysia"
    else -> region
}

/**
 * One selectable location, standing for every zone id that shares its name.
 *
 * [primary] is the id a launch actually routes to — the best of the group. [alternateCount] is how
 * many others were folded in, shown so the row does not silently hide capacity the player might
 * want to know about.
 */
internal data class PrintedWasteLocation(
    val title: String,
    val region: String,
    val primary: PrintedWasteZoneOption,
    val alternateCount: Int,
    val gpuTier: PrintedWasteGpuTier?,
)

/**
 * Folds zone options into one entry per physical location, best server first within each.
 *
 * "Best" is the same [printedWasteScore] the recommendation uses, so the id a combined row routes
 * to is the one the app would have picked anyway had the list stayed flat.
 */
internal fun printedWasteLocations(
    zones: List<PrintedWasteZoneOption>,
    mapping: Map<String, PrintedWasteServerMappingEntry>,
): List<PrintedWasteLocation> {
    if (zones.isEmpty()) return emptyList()
    val maxPing = zones.mapNotNull { it.pingMs }.maxOrNull()?.coerceAtLeast(1L) ?: 1L
    val maxQueue = zones.maxOfOrNull { it.zone.QueuePosition }?.coerceAtLeast(1) ?: 1
    return zones
        .groupBy { printedWasteZoneTitle(it.zoneId, mapping[it.zoneId]) }
        .map { (title, group) ->
            val ordered = group.sortedWith(
                compareBy<PrintedWasteZoneOption> { printedWasteScore(it, maxPing, maxQueue) }
                    .thenBy { it.zoneId },
            )
            val primary = ordered.first()
            PrintedWasteLocation(
                title = title,
                region = printedWasteZoneRegion(mapping[primary.zoneId], primary.zone.Region),
                primary = primary,
                alternateCount = ordered.size - 1,
                // Report the best GPU anywhere in the group: the row stands for the location, and
                // a 5080 sitting behind a folded id is still what the player can reach from here.
                gpuTier = ordered.firstNotNullOfOrNull { printedWasteGpuTier(mapping[it.zoneId]) },
            )
        }
}

/**
 * Locations grouped under their region heading, both ordered by how good the best option is.
 *
 * Regions sort by their strongest location rather than alphabetically, so the nearest servers stay
 * at the top of a list that is now several headings long.
 */
internal fun printedWasteRegionGroups(
    locations: List<PrintedWasteLocation>,
    maxPing: Long,
    maxQueue: Int,
): List<Pair<String, List<PrintedWasteLocation>>> =
    locations
        .groupBy { it.region }
        .entries
        .map { (region, group) ->
            region to group.sortedWith(
                compareBy<PrintedWasteLocation> { printedWasteScore(it.primary, maxPing, maxQueue) }
                    .thenBy { it.title },
            )
        }
        .sortedWith(
            compareBy<Pair<String, List<PrintedWasteLocation>>> {
                printedWasteScore(it.second.first().primary, maxPing, maxQueue)
            }.thenBy { it.first },
        )

internal data class PrintedWasteZoneOption(
    val zoneId: String,
    val zone: PrintedWasteZone,
    val routingUrl: String,
    val pingMs: Long?,
)

/** Whether [selected] has more measured latency than another selectable server. */
internal fun hasHigherPingThanClosestPrintedWasteZone(
    selected: PrintedWasteZoneOption,
    selectableZones: List<PrintedWasteZoneOption>,
): Boolean {
    val selectedPing = selected.pingMs ?: return false
    val closestPing = selectableZones.mapNotNull { it.pingMs }.minOrNull() ?: return false
    return selectedPing > closestPing
}

internal fun recommendedPrintedWasteZone(zones: List<PrintedWasteZoneOption>): PrintedWasteZoneOption? {
    if (zones.isEmpty()) return null
    val pool = zones.filter { it.pingMs != null }.ifEmpty { zones }
    val maxPing = pool.mapNotNull { it.pingMs }.maxOrNull()?.coerceAtLeast(1L) ?: 1L
    val maxQueue = pool.maxOfOrNull { it.zone.QueuePosition }?.coerceAtLeast(1) ?: 1
    val queueAwareRecommendation = pool.minWithOrNull(
        compareBy<PrintedWasteZoneOption> { printedWasteScore(it, maxPing, maxQueue) }
            .thenBy { it.pingMs ?: Long.MAX_VALUE }
            .thenBy { it.zone.QueuePosition },
    )
    if ((queueAwareRecommendation?.pingMs ?: 0L) <= MAX_QUEUE_AWARE_RECOMMENDED_PING_MS) {
        return queueAwareRecommendation
    }

    return pool.minWithOrNull(
        compareBy<PrintedWasteZoneOption> { it.pingMs ?: Long.MAX_VALUE }
            .thenBy { it.zone.QueuePosition }
            .thenBy { it.zoneId },
    )
}

internal fun printedWasteScore(zone: PrintedWasteZoneOption, maxPing: Long, maxQueue: Int): Double {
    val pingScore = ((zone.pingMs ?: maxPing).toDouble() / maxPing.toDouble()) * 0.75
    val queueScore = (zone.zone.QueuePosition.toDouble() / maxQueue.toDouble()) * 0.25
    return pingScore + queueScore
}

private const val MAX_QUEUE_AWARE_RECOMMENDED_PING_MS = 100L

internal fun formatPrintedWasteWait(etaMs: Long): String {
    val minutes = ((etaMs + 59_999L) / 60_000L).coerceAtLeast(1L)
    return if (minutes < 60L) "${minutes}m" else "${minutes / 60L}h ${minutes % 60L}m"
}
