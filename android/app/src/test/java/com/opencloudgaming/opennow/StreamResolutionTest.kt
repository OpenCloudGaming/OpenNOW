package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Test

class StreamResolutionTest {
    @Test
    fun streamResolutionPixelsKeepsSelected1080pFor16By9() {
        val settings = StreamSettings(resolution = "1920x1080", aspectRatio = "16:9")

        assertEquals(1920 to 1080, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsMaps1080pTierToUltrawideMode() {
        val settings = StreamSettings(resolution = "1920x1080", aspectRatio = "21:9")

        assertEquals(2560 to 1080, streamResolutionPixels(settings))
    }

    @Test
    fun freePlanResolutionNormalizationUses720pUltrawideFallback() {
        val freeSubscription = SubscriptionInfo(membershipTier = "FREE")

        assertEquals(
            "1680x720",
            normalizeStreamResolutionForAspectAndPlan("1920x1080", "21:9", freeSubscription, null),
        )
        assertEquals(
            "1680x720",
            StreamSettings(resolution = "2560x1080", aspectRatio = "21:9")
                .withResolutionAllowed(freeSubscription, null)
                .resolution,
        )
    }

    @Test
    fun freePlanResolutionGuardFallsBackWhenAspectHasNoAvailableMode() {
        val adjusted = StreamSettings(resolution = "3840x1080", aspectRatio = "32:9")
            .withResolutionAllowed(SubscriptionInfo(membershipTier = "FREE"), null)

        assertEquals("1920x1080", adjusted.resolution)
        assertEquals("16:9", adjusted.aspectRatio)
    }

    @Test
    fun streamResolutionPixelsMaps1440pTierToUltrawideMode() {
        val settings = StreamSettings(resolution = "2560x1440", aspectRatio = "21:9")

        assertEquals(3440 to 1440, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsKeepsSelected4kFor16By9() {
        val settings = StreamSettings(resolution = "3840x2160", aspectRatio = "16:9")

        assertEquals(3840 to 2160, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsMapsSelectedTierForTallerAspectRatio() {
        val settings = StreamSettings(resolution = "1920x1080", aspectRatio = "16:10")

        assertEquals(1920 to 1200, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionPixelsKeepsExactStoredUltrawideMode() {
        val settings = StreamSettings(resolution = "3440x1440", aspectRatio = "21:9")

        assertEquals(3440 to 1440, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionOptionsIncludeAndroidSupportedModes() {
        assertEquals(
            listOf("1280x720", "1366x768", "1600x900", "1920x1080", "2560x1440", "3840x2160", "5120x2880"),
            streamResolutionOptionsForAspect("16:9"),
        )
        assertEquals(listOf("1024x768", "1112x834", "1600x1200"), streamResolutionOptionsForAspect("4:3"))
        assertEquals(listOf("1280x1024"), streamResolutionOptionsForAspect("5:4"))
        assertEquals(listOf("1680x720", "2560x1080", "3440x1440", "5120x2160"), streamResolutionOptionsForAspect("21:9"))
        assertEquals(listOf("3840x1080", "5120x1440"), streamResolutionOptionsForAspect("32:9"))
    }

    @Test
    fun streamResolutionPixelsMaps4kTierToSupported16By10Mode() {
        val settings = StreamSettings(resolution = "3840x2160", aspectRatio = "16:10")

        assertEquals(3456 to 2160, streamResolutionPixels(settings))
    }

    @Test
    fun streamResolutionChoicesGatePriorityAndUltimateModes() {
        val freeSubscription = SubscriptionInfo(membershipTier = "FREE")
        val prioritySubscription = SubscriptionInfo(membershipTier = "PRIORITY")
        val ultimateSubscription = SubscriptionInfo(membershipTier = "ULTIMATE")
        val fhd = streamResolutionChoicesForAspect("16:9").first { it.value == "1920x1080" }
        val whd = streamResolutionChoicesForAspect("21:9").first { it.value == "1680x720" }
        val wfhd = streamResolutionChoicesForAspect("21:9").first { it.value == "2560x1080" }
        val qhd = streamResolutionChoicesForAspect("16:9").first { it.value == "2560x1440" }
        val fourK = streamResolutionChoicesForAspect("16:9").first { it.value == "3840x2160" }
        val fiveK = streamResolutionChoicesForAspect("16:9").first { it.value == "5120x2880" }

        assertEquals(true, fhd.isAvailableFor(freeSubscription, null))
        assertEquals(true, whd.isAvailableFor(freeSubscription, null))
        assertEquals(false, wfhd.isAvailableFor(freeSubscription, null))
        assertEquals(false, qhd.isAvailableFor(freeSubscription, null))
        assertEquals(true, fhd.isAvailableFor(prioritySubscription, null))
        assertEquals(true, whd.isAvailableFor(prioritySubscription, null))
        assertEquals(true, wfhd.isAvailableFor(prioritySubscription, null))
        assertEquals(true, qhd.isAvailableFor(prioritySubscription, null))
        assertEquals(false, fourK.isAvailableFor(prioritySubscription, null))
        assertEquals(false, fiveK.isAvailableFor(prioritySubscription, null))
        assertEquals(true, fourK.isAvailableFor(ultimateSubscription, null))
        assertEquals(true, fiveK.isAvailableFor(ultimateSubscription, null))
    }

    @Test
    fun entitledResolutionDoesNotBypassMembershipPlanGate() {
        val subscription = SubscriptionInfo(
            membershipTier = "FREE",
            entitledResolutions = listOf(EntitledResolution(width = 3840, height = 2160, fps = 60)),
        )
        val fourK = streamResolutionChoicesForAspect("16:9").first { it.value == "3840x2160" }

        assertEquals(false, fourK.isAvailableFor(subscription, null))
    }
}
