package com.opencloudgaming.opennow

private val READY_SESSION_STATUSES = setOf(2, 3)

internal fun isSessionAdsRequired(adState: SessionAdState?): Boolean =
    adState?.sessionAdsRequired ?: (adState?.isAdsRequired == true)

internal fun sessionAdItems(adState: SessionAdState?): List<SessionAdInfo> =
    adState?.sessionAds?.takeIf { it.isNotEmpty() } ?: adState?.ads.orEmpty()

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
    if (next.status in READY_SESSION_STATUSES) return next
    return next.copy(
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
