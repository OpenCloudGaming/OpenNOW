package com.opencloudgaming.opennow

import com.posthog.android.PostHogAndroidConfig
import org.junit.Assert.assertTrue
import org.junit.Test

class OpenNowAnalyticsTest {
    @Test
    fun enablesScreenshotReplayForComposeUi() {
        val config = PostHogAndroidConfig(
            apiKey = "phc_test",
            host = "https://us.i.posthog.com",
        ).apply {
            sessionReplay = false
            sessionReplayConfig.screenshot = false
            sessionReplayConfig.maskAllTextInputs = false
            sessionReplayConfig.maskAllImages = false
        }

        config.applyOpenNowSettings(AppSettings(analyticsOptOut = true))

        assertTrue(config.optOut)
        assertTrue(config.captureApplicationLifecycleEvents)
        assertTrue(config.captureDeepLinks)
        assertTrue(config.captureScreenViews)
        assertTrue(config.sessionReplay)
        assertTrue(config.sessionReplayConfig.screenshot)
        assertTrue(config.sessionReplayConfig.maskAllTextInputs)
        assertTrue(config.sessionReplayConfig.maskAllImages)
        assertTrue(config.errorTrackingConfig.autoCapture)
    }
}
