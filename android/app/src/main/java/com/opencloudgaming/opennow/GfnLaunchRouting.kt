package com.opencloudgaming.opennow

internal data class GfnLaunchRoute(
    val baseUrl: String,
    val storageRegion: String? = null,
)

/** Persistent Install-to-Play storage belongs to a region, independent of the server picker. */
internal suspend fun resolveGfnLaunchRoute(
    game: GameInfo,
    providerCode: String,
    subscription: SubscriptionInfo?,
    defaultBaseUrl: String,
    regions: List<StreamRegion>,
    refreshRegions: suspend () -> List<StreamRegion>,
): GfnLaunchRoute {
    val storage = subscription?.storageAddon?.takeIf {
        providerCode.equals("NVIDIA", ignoreCase = true) &&
            game.playType.equals("INSTALL_TO_PLAY", ignoreCase = true) &&
            it.status.equals("OK", ignoreCase = true) &&
            it.subType.equals("PERMANENT_STORAGE", ignoreCase = true)
    } ?: return GfnLaunchRoute(defaultBaseUrl)

    // Match provider-advertised region names/codes; a storage metro code is not a hostname.
    val aliases = listOfNotNull(storage.regionName, storage.regionCode)
        .map(String::trim).filter(String::isNotEmpty)
    fun matchingUrl(candidates: List<StreamRegion>): String? = candidates
        .filter { region -> aliases.any { it.equals(region.name.trim(), ignoreCase = true) } }
        .mapNotNull { normalizeStreamingServiceUrl(it.url) }
        .distinct()
        .singleOrNull()

    val url = matchingUrl(regions) ?: matchingUrl(refreshRegions())
    val regionLabel = storage.regionName?.trim()?.takeIf(String::isNotEmpty)
        ?: storage.regionCode?.trim()?.takeIf(String::isNotEmpty)
        ?: "the account's storage region"
    check(url != null) {
        "Could not resolve the GeForce NOW storage server for $regionLabel. " +
            "This Install-to-Play game must launch where your storage is located. Try again."
    }
    return GfnLaunchRoute(url, storageRegion = regionLabel)
}
