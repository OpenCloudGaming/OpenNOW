package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LaunchErrorsTest {
    @Test
    fun confirmedTerminalResumeFailureForgetsTheEndedSession() {
        val error = IllegalStateException("Resume failed", TerminalSessionStatusException(7, null))

        assertTrue(shouldForgetActiveSessionAfterResumeFailure(error))
    }

    @Test
    fun temporaryResumeFailureKeepsTheSessionAvailableForRecovery() {
        assertFalse(shouldForgetActiveSessionAfterResumeFailure(java.io.IOException("Connection reset")))
        assertFalse(shouldForgetActiveSessionAfterResumeFailure(SessionClaimNotReadyException(null)))
    }

    @Test
    fun entitlementFailureExplainsProviderRejectionWithoutAssumingPlanIsInactive() {
        val error = CloudMatchRequestStatusException(
            statusCode = 18,
            statusDescription = "ENTITLEMENT_FAILURE_STATUS 8A910006",
            unifiedErrorCode = "-1970208762",
        )

        assertEquals(
            "GeForce NOW rejected this launch because it could not verify a playable membership for the signed-in account. If Free is active, confirm you signed in with the same NVIDIA account, then sign out and back in.",
            normalizeLaunchErrorMessage(error, "Subnautica 2"),
        )
        assertTrue(isMissingGfnPlanError(error))
    }

    @Test
    fun wrappedEntitlementFailureStillExplainsProviderRejection() {
        val providerError = CloudMatchRequestStatusException(
            statusCode = 18,
            statusDescription = "ENTITLEMENT_FAILURE_STATUS",
            unifiedErrorCode = "8A910006",
        )
        val error = IllegalStateException("Upgrade membership", providerError)

        assertEquals(
            "GeForce NOW rejected this launch because it could not verify a playable membership for the signed-in account. If Free is active, confirm you signed in with the same NVIDIA account, then sign out and back in.",
            normalizeLaunchErrorMessage(error, "Subnautica 2"),
        )
        assertTrue(isMissingGfnPlanError(error))
    }

    @Test
    fun status18AloneIsParsedAsEntitlementRejection() {
        val error = CloudMatchRequestStatusException(
            statusCode = 18,
            statusDescription = null,
            unifiedErrorCode = null,
        )

        assertEquals(
            "GeForce NOW rejected this launch because it could not verify a playable membership for the signed-in account. If Free is active, confirm you signed in with the same NVIDIA account, then sign out and back in.",
            normalizeLaunchErrorMessage(error),
        )
        assertTrue(isMissingGfnPlanError(error))
    }

    @Test
    fun limitedModeCloudMatchStatusUsesGameTitle() {
        val error = CloudMatchRequestStatusException(
            statusCode = 81,
            statusDescription = "STREAMING_NOT_ALLOWED_IN_LIMITED_MODE 8A91000D",
            unifiedErrorCode = "-1970208755",
        )

        assertEquals(
            "Subnautica 2 is only available for Priority or Ultimate members",
            normalizeLaunchErrorMessage(error, "Subnautica 2"),
        )
    }

    @Test
    fun limitedModeCloudMatchStatusFallsBackWithoutGameTitle() {
        val error = CloudMatchRequestStatusException(
            statusCode = 81,
            statusDescription = "STREAMING_NOT_ALLOWED_IN_LIMITED_MODE",
            unifiedErrorCode = null,
        )

        assertEquals(
            "This game is only available for Priority or Ultimate members",
            normalizeLaunchErrorMessage(error),
        )
    }

    @Test
    fun unrelatedCloudMatchFailureKeepsItsOwnMessage() {
        val error = CloudMatchRequestStatusException(
            statusCode = 42,
            statusDescription = "CAPACITY_FAILURE_STATUS",
            unifiedErrorCode = "DEADBEEF",
        )

        assertEquals(
            "CloudMatch returned status 42: CAPACITY_FAILURE_STATUS (unified error DEADBEEF)",
            normalizeLaunchErrorMessage(error, "Subnautica 2"),
        )
        assertFalse(isMissingGfnPlanError(error))
    }

    @Test
    fun abandonedQueueIsTerminalEvenWhenProviderUsesHttp503() {
        val error = CloudMatchRequestStatusException(
            statusCode = 69,
            statusDescription = "SESSION_REQUEST_IN_QUEUE_ABANDONED 4A8C300F",
            unifiedErrorCode = "1250701327",
        )

        assertTrue(isAbandonedQueueError(error))
        assertEquals(
            "The cloud provider ended this queue request. Start the game again to join a new queue.",
            normalizeLaunchErrorMessage(error),
        )
        assertFalse(isAbandonedQueueError(CloudMatchRequestStatusException(8, "INTERNAL_ERROR_STATUS", null)))
        assertFalse(isAbandonedQueueError(IllegalStateException("Network unavailable")))
    }

    @Test
    fun entitlementWordsInsideAnUnstructuredErrorAreNotMisclassified() {
        val error = IllegalStateException(
            "Diagnostics mentioned ENTITLEMENT_FAILURE_STATUS, but DNS lookup failed",
        )

        assertEquals(
            "Diagnostics mentioned ENTITLEMENT_FAILURE_STATUS, but DNS lookup failed",
            normalizeLaunchErrorMessage(error, "Subnautica 2"),
        )
    }

    @Test
    fun maintenanceErrorsStillUseFriendlyCopy() {
        val error = IllegalStateException("Game server is under maintenance")

        assertEquals(
            "Game is patching or under maintenance. Try again when NVIDIA finishes updating it.",
            normalizeLaunchErrorMessage(error, "Subnautica 2"),
        )
    }

    @Test
    fun internalCloudMatchFailureOffersOneLowerSettingsRetry() {
        val error = CloudMatchRequestStatusException(
            statusCode = 500,
            statusDescription = "INTERNAL_ERROR_STATUS",
            unifiedErrorCode = "8A8C0000",
        )
        val demanding = StreamSettings(
            resolution = "3840x2160",
            fps = 120,
            maxBitrateMbps = 150,
            codec = VideoCodec.H265,
            colorQuality = ColorQuality.TenBit420,
            hdrEnabled = true,
            enableL4S = true,
            experimentalNvst = true,
        )

        assertTrue(shouldOfferLowerSettingsRetry(error, demanding))
        val lower = demanding.loweredSessionLaunchProfile()
        assertEquals("1920x1080", lower.resolution)
        assertEquals("16:9", lower.aspectRatio)
        assertEquals(60, lower.fps)
        assertEquals(75, lower.maxBitrateMbps)
        assertEquals(VideoCodec.H264, lower.codec)
        assertEquals(ColorQuality.EightBit420, lower.colorQuality)
        assertFalse(lower.hdrEnabled)
        assertFalse(lower.enableL4S)
        assertFalse(lower.experimentalNvst)
        assertFalse(shouldOfferLowerSettingsRetry(error, lower))
        assertFalse(shouldOfferLowerSettingsRetry(IllegalStateException("Network unavailable"), demanding))
    }
}
