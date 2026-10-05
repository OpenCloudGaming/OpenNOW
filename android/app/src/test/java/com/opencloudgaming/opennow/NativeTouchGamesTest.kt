package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Native touch reroutes every finger away from the cursor and gamepad paths, so *where* it switches
 * on is a safety question, not a preference one. These tests pin the catalog capability signal
 * used by NVIDIA and the user's explicit mode override.
 */
class NativeTouchGamesTest {

    private fun game(
        id: String = "id-1",
        title: String = "Some Game",
        supportedControls: List<String> = emptyList(),
    ) = GameInfo(
        id = id,
        title = title,
        variants = listOf(GameVariant(id = "v1", store = "STEAM", supportedControls = supportedControls)),
    )

    @Test
    fun theCatalogTouchFlagEnablesAutoMode() {
        val touchGame = game(title = "Honkai: Star Rail", supportedControls = listOf("TOUCHSCREEN"))
        assertTrue(catalogClaimsTouchSupport(touchGame))
        assertTrue(shouldUseNativeTouch(NativeTouchMode.Auto, touchGame))
    }

    @Test
    fun fortniteCanPreferTheVirtualControllerWithoutLosingCatalogTouchSupport() {
        val fortnite = game(title = "Fortnite", supportedControls = listOf("TOUCHSCREEN"))
        val streamSettings = StreamSettings()

        assertTrue(catalogClaimsTouchSupport(fortnite))
        assertTrue(
            shouldUseNativeTouchForStream(
                NativeTouchMode.Auto,
                fortnite,
                streamSettings,
                preferVirtualController = false,
            ),
        )
        assertFalse(
            shouldUseNativeTouchForStream(
                NativeTouchMode.Auto,
                fortnite,
                streamSettings,
                preferVirtualController = true,
            ),
        )
    }

    @Test
    fun keyboardMousePreferenceWinsOverNativeTouchForTheSession() {
        val fortnite = game(title = "Fortnite", supportedControls = listOf("TOUCHSCREEN", "MOUSE"))
        val streamSettings = StreamSettings()

        assertFalse(
            shouldUseNativeTouchForStream(
                NativeTouchMode.Auto,
                fortnite,
                streamSettings,
                preferVirtualController = false,
                preferKeyboardMouse = true,
            ),
        )
        assertFalse(
            shouldUseNativeTouchForStream(
                NativeTouchMode.Always,
                fortnite,
                streamSettings,
                preferVirtualController = false,
                preferKeyboardMouse = true,
            ),
        )
    }

    @Test
    fun savedKeyboardOverlayCannotDisableTouchProvisionedForThisSession() {
        val game = game(title = "Genshin Impact", supportedControls = listOf("TOUCHSCREEN", "KEYBOARD"))
        val settings = AndroidTouchSettings(
            nativeTouchMode = NativeTouchMode.Auto,
            keyboardModeEnabled = true,
        )
        val launchMode = StreamInputMode.NativeTouch

        assertFalse(keyboardOverlayEnabledForStream(settings.keyboardModeEnabled, launchMode))
        assertTrue(
            shouldUseNativeTouchForStream(
                settings.effectiveNativeTouchMode(),
                game,
                StreamSettings(),
                preferVirtualController = false,
                preferKeyboardMouse = launchMode == StreamInputMode.KeyboardMouse,
            ),
        )
    }

    @Test
    fun catalogTouchFlagIsCaseInsensitive() {
        assertTrue(catalogClaimsTouchSupport(game(supportedControls = listOf("Touchscreen"))))
        assertFalse(catalogClaimsTouchSupport(game(supportedControls = listOf("X_INPUT_GAMEPAD"))))
        assertFalse(catalogClaimsTouchSupport(game()))
    }

    // -- Mode ---------------------------------------------------------------------------------

    @Test
    fun autoFollowsTheCatalogCapability() {
        assertTrue(shouldUseNativeTouch(NativeTouchMode.Auto, game(supportedControls = listOf("TOUCHSCREEN"))))
        assertFalse(shouldUseNativeTouch(NativeTouchMode.Auto, game(title = "Cyberpunk 2077")))
    }

    @Test
    fun autoKeepsCatalogTouchAtHighPerformanceStreamAllocation() {
        val touchGame = game(supportedControls = listOf("TOUCHSCREEN"))

        assertTrue(
            shouldUseNativeTouch(
                NativeTouchMode.Auto,
                touchGame,
                StreamSettings(resolution = "2560x1440", fps = 120),
            ),
        )
        assertTrue(
            shouldUseNativeTouch(
                NativeTouchMode.Auto,
                touchGame,
                StreamSettings(resolution = "1920x1080", fps = 60),
            ),
        )
    }

    @Test
    fun alwaysTouchStillOverridesHighPerformanceAllocation() {
        assertTrue(
            shouldUseNativeTouch(
                NativeTouchMode.Always,
                game(supportedControls = listOf("TOUCHSCREEN")),
                StreamSettings(resolution = "2560x1440", fps = 120),
            ),
        )
    }

    @Test
    fun offWinsOverEverything() {
        assertFalse(shouldUseNativeTouch(NativeTouchMode.Off, game(supportedControls = listOf("TOUCHSCREEN"))))
    }

    @Test
    fun alwaysAppliesToUnmarkedGamesToo() {
        assertTrue(shouldUseNativeTouch(NativeTouchMode.Always, game(title = "Cyberpunk 2077")))
    }

    /** No game means no session to route touches into; Auto must not guess. */
    @Test
    fun autoStaysOffWithoutAGame() {
        assertFalse(shouldUseNativeTouch(NativeTouchMode.Auto, null))
    }

    @Test
    fun nativeTouchDefaultsToSupportedCatalogGames() {
        val settings = AndroidTouchSettings()
        assertEquals(NativeTouchMode.Auto, settings.nativeTouchMode)
        assertTrue(settings.nativeTouchOptedIn)
        assertEquals(NativeTouchMode.Auto, settings.effectiveNativeTouchMode())
    }

    @Test
    fun legacyAutoSettingKeepsItsPreviouslySavedBehavior() {
        val legacy = AndroidTouchSettings(
            nativeTouchMode = NativeTouchMode.Auto,
            nativeTouchOptedIn = false,
        )
        assertFalse(legacy.nativeTouchOptedIn)
        assertEquals(NativeTouchMode.Auto, legacy.effectiveNativeTouchMode())
    }

    @Test
    fun release159LegacyAutoStillSelectsFortniteNativeTouch() {
        val release159Settings = AndroidTouchSettings(
            nativeTouchMode = NativeTouchMode.Auto,
            nativeTouchOptedIn = false,
        )
        val fortnite = game(
            title = "Fortnite®",
            supportedControls = listOf("GAMEPAD", "KEYBOARD", "MOUSE", "TOUCHSCREEN"),
        )

        assertTrue(
            shouldUseNativeTouchForStream(
                mode = release159Settings.effectiveNativeTouchMode(),
                game = fortnite,
                streamSettings = StreamSettings(),
                preferVirtualController = false,
                preferKeyboardMouse = false,
            ),
        )
    }

    @Test
    fun choosingNativeTouchExplicitlyEnablesIt() {
        val enabled = AndroidTouchSettings().withNativeTouchMode(NativeTouchMode.Auto)
        assertTrue(enabled.nativeTouchOptedIn)
        assertEquals(NativeTouchMode.Auto, enabled.effectiveNativeTouchMode())

        val disabled = enabled.withNativeTouchMode(NativeTouchMode.Off)
        assertFalse(disabled.nativeTouchOptedIn)
        assertEquals(NativeTouchMode.Off, disabled.effectiveNativeTouchMode())
    }

    @Test
    fun diagnosticsCarryTheCatalogDecision() {
        val line = nativeTouchDiagnostics(
            game(id = "abc123", title = "Genshin Impact", supportedControls = listOf("TOUCHSCREEN", "KEYBOARD")),
            enabled = true,
        )
        assertTrue(line, line.contains("id=abc123"))
        assertTrue(line, line.contains("title=Genshin Impact"))
        assertTrue(line, line.contains("TOUCHSCREEN"))
        assertTrue(line, line.contains("catalogTouch=true"))
    }
}
