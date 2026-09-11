package com.opencloudgaming.opennow

import kotlinx.serialization.encodeToString
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Settings persistence moved off the caller's thread. The encode is the expensive half, so this
 * pins that the payload really is large enough to be worth keeping off the main thread, and that
 * the round trip is lossless now that the write is conflated.
 */
class SettingsPersistenceTest {
    @Test
    fun `system wallpaper normalization disables OpenNOW backgrounds`() {
        val normalized = AppSettings(
            nerdCatalogBackground = true,
            ambientBackgroundEnabled = true,
            systemWallpaperBackground = true,
        ).normalizedForAndroid()

        assertTrue(normalized.systemWallpaperBackground)
        assertFalse(normalized.nerdCatalogBackground)
        assertFalse(normalized.ambientBackgroundEnabled)
    }

    @Test
    fun aRealisticSettingsObjectIsNotACheapEncode() {
        val settings = AppSettings(
            favoriteGameIds = (1..200).map { "game-$it" },
            localAppPackageNames = (1..40).map { "com.example.app$it" },
            defaultGameVariantIds = (1..200).associate { "game-$it" to "variant-$it" },
        )
        val encoded = OpenNowJson.encodeToString(settings)
        // Every favourite tap used to serialize all of this on the main thread.
        assertTrue("encoded ${encoded.length} chars", encoded.length > 10_000)
    }

    @Test
    fun conflatedWritesStillRoundTripTheLatestValue() {
        // Only the newest value reaches disk; it must decode back to exactly what was set.
        val latest = AppSettings(
            favoriteGameIds = listOf("a", "b"),
            localAppsCollapsed = true,
            hapticsOutput = HapticsOutputPreference.Device,
            androidTouch = AndroidTouchSettings(
                touchControllerStyle = TouchControllerStyle.Neon,
                touchButtonLabels = false,
                faceButtonScale = 1.25f,
                rightStickScale = 0.85f,
                stickKnobScale = 0.58f,
                visibleControlGroups = TouchControlGroup.entries.toSet() - TouchControlGroup.Dpad,
                extraButtonActions = listOf(
                    TouchExtraButtonAction.Guide,
                    TouchExtraButtonAction.RightTrigger,
                    TouchExtraButtonAction.A,
                    TouchExtraButtonAction.None,
                ),
                extraButtonScale = 1.3f,
                gyroscopeEnabled = true,
                gyroscopeSensitivity = 1.4f,
                gyroscopeInvertVertical = true,
            ),
        )
        val decoded = OpenNowJson.decodeFromString<AppSettings>(OpenNowJson.encodeToString(latest))
        assertEquals(latest, decoded)
    }

    @Test
    fun normalizationIsStableSoRepeatedWritesDoNotOscillate() {
        // update() normalizes before storing; a normalize that changed its own output would emit
        // forever under a conflated collector.
        val once = AppSettings().normalizedForAndroid()
        assertEquals(once, once.normalizedForAndroid())
    }

    @Test
    fun programmableButtonsNormalizeToFourSafeSlots() {
        val normalized = AppSettings(
            androidTouch = AndroidTouchSettings(
                extraButtonActions = listOf(TouchExtraButtonAction.A),
                extraButtonScale = Float.POSITIVE_INFINITY,
            ),
        ).normalizedForAndroid().androidTouch

        assertEquals(TOUCH_EXTRA_BUTTON_COUNT, normalized.extraButtonActions.size)
        assertEquals(TouchExtraButtonAction.A, normalized.extraButtonAction(0))
        assertEquals(TouchExtraButtonAction.None, normalized.extraButtonAction(3))
        assertEquals(AndroidTouchSettings().extraButtonScale, normalized.extraButtonScale, 0.0001f)
    }
}
