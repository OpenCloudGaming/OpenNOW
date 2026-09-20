package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LaunchErrorsTest {
    @Test
    fun entitlementFailureExplainsThatNoCurrentPlanWasFound() {
        val error = CloudMatchRequestStatusException(
            statusCode = 18,
            statusDescription = "ENTITLEMENT_FAILURE_STATUS 8A910006",
            unifiedErrorCode = "-1970208762",
        )

        assertEquals(
            "No current GeForce NOW plan was found. Activate the Free plan or choose a paid membership, then try again.",
            normalizeLaunchErrorMessage(error, "Subnautica 2"),
        )
        assertTrue(isMissingGfnPlanError(error))
    }

    @Test
    fun wrappedEntitlementFailureStillPromptsForAPlan() {
        val providerError = CloudMatchRequestStatusException(
            statusCode = 18,
            statusDescription = "ENTITLEMENT_FAILURE_STATUS",
            unifiedErrorCode = "8A910006",
        )
        val error = IllegalStateException("Upgrade membership", providerError)

        assertEquals(
            "No current GeForce NOW plan was found. Activate the Free plan or choose a paid membership, then try again.",
            normalizeLaunchErrorMessage(error, "Subnautica 2"),
        )
        assertTrue(isMissingGfnPlanError(error))
    }

    @Test
    fun status18AloneIsParsedAsNoCurrentPlan() {
        val error = CloudMatchRequestStatusException(
            statusCode = 18,
            statusDescription = null,
            unifiedErrorCode = null,
        )

        assertEquals(
            "No current GeForce NOW plan was found. Activate the Free plan or choose a paid membership, then try again.",
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
