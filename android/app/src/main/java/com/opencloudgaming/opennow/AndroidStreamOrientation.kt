package com.opencloudgaming.opennow

internal const val PHONE_STREAM_LANDSCAPE_MAX_SMALLEST_WIDTH_DP = 600

internal fun shouldLockPhoneStreamLandscape(
    state: OpenNowUiState,
    smallestScreenWidthDp: Int,
): Boolean =
    state.page == AppPage.Stream &&
        state.streamStatus in phoneStreamLandscapeStatuses &&
        (state.boosteroid.active || state.streamSession?.isReadyForStream() == true) &&
        !(state.androidTvProfile || state.codecReport?.androidTvProfile == true) &&
        isPhoneSizedAndroidDevice(smallestScreenWidthDp)

private val phoneStreamLandscapeStatuses = setOf("connecting", "streaming")

private fun isPhoneSizedAndroidDevice(smallestScreenWidthDp: Int): Boolean =
    smallestScreenWidthDp in 1 until PHONE_STREAM_LANDSCAPE_MAX_SMALLEST_WIDTH_DP
