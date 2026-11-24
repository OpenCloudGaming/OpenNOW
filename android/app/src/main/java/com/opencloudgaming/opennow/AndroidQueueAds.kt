package com.opencloudgaming.opennow

internal fun isSessionAdsRequired(adState: SessionAdState?): Boolean =
    adState?.sessionAdsRequired ?: (adState?.isAdsRequired == true)

internal fun sessionAdItems(adState: SessionAdState?): List<SessionAdInfo> =
    adState?.sessionAds?.takeIf { it.isNotEmpty() } ?: adState?.ads.orEmpty()

internal fun sessionAdMediaUrls(ad: SessionAdInfo?): List<String> {
    if (ad == null) return emptyList()
    return buildList {
        ad.adMediaFiles.forEach { add(it.mediaFileUrl) }
        add(ad.adUrl)
        add(ad.mediaUrl)
    }.mapNotNull { it?.trim()?.takeIf(String::isNotEmpty) }.distinct()
}

internal fun shouldWaitForQueueAdPlayback(adState: SessionAdState?): Boolean =
    isSessionAdsRequired(adState) && sessionAdItems(adState).isNotEmpty()

internal fun mergeQueueAdState(
    previous: SessionAdState?,
    next: SessionAdState?,
    preserveMissingAdState: Boolean = true,
): SessionAdState? {
    if (next == null) return if (preserveMissingAdState) previous else null
    val shouldRestorePreviousAds =
        preserveMissingAdState &&
            isSessionAdsRequired(next) &&
            next.serverSentEmptyAds &&
            sessionAdItems(next).isEmpty() &&
            sessionAdItems(previous).isNotEmpty()

    return if (shouldRestorePreviousAds) {
        next.copy(
            sessionAds = sessionAdItems(previous),
            ads = previous?.ads?.takeIf { it.isNotEmpty() } ?: sessionAdItems(previous),
        )
    } else {
        next
    }
}

internal fun mergeQueueSessionState(
    previous: SessionInfo,
    next: SessionInfo,
    preserveMissingAdState: Boolean = true,
): SessionInfo {
    val merged = next.copy(assignedZone = next.assignedZone ?: previous.assignedZone)
    if (merged.isReadyForStream()) return merged
    return merged.copy(
        adState = mergeQueueAdState(previous.adState, next.adState, preserveMissingAdState),
        mediaConnectionInfo = next.mediaConnectionInfo ?: previous.mediaConnectionInfo,
    )
}

internal fun removeSessionAdItem(adState: SessionAdState?, adId: String): SessionAdState? {
    if (adState == null) return null
    return adState.copy(
        sessionAds = adState.sessionAds.filterNot { it.adId == adId },
        ads = adState.ads.filterNot { it.adId == adId },
        serverSentEmptyAds = false,
    )
}

internal fun removeSessionAdItem(session: SessionInfo, adId: String): SessionInfo =
    session.copy(adState = removeSessionAdItem(session.adState, adId))

internal fun mergeQueueAdReportResult(
    previous: SessionInfo,
    updated: SessionInfo,
    adId: String,
    terminalAction: Boolean,
): SessionInfo {
    val sanitizedUpdate = if (
        terminalAction && sessionAdItems(updated.adState).any { it.adId == adId }
    ) {
        removeSessionAdItem(updated, adId)
    } else {
        updated
    }
    return mergeQueueSessionState(previous, sanitizedUpdate)
}

internal fun nextSessionAdId(adState: SessionAdState?, completedAdId: String): String? {
    val ads = sessionAdItems(adState)
    val completedIndex = ads.indexOfFirst { it.adId == completedAdId }
    return ads.getOrNull(completedIndex + 1)?.adId
}
