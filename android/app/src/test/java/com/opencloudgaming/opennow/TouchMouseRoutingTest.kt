package com.opencloudgaming.opennow

import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Test

class TouchMouseRoutingTest {
    @Test fun combinedSlideDirectClickIsOptInAndPersistsWithoutChangingWithLayouts() {
        val legacy = OpenNowJson.decodeFromString<AppSettings>("""{"androidTouch":{"mouseDirectClick":true}}""")
        assertFalse(legacy.androidTouch.experimentalSlideDirectClick)
        for (enabled in listOf(false, true)) {
            val touch = legacy.androidTouch.copy(experimentalSlideDirectClick = enabled)
            val restored = OpenNowJson.decodeFromString<AppSettings>(OpenNowJson.encodeToString(legacy.copy(androidTouch = touch)))
                .normalizedForAndroid()
            assertEquals(enabled, restored.androidTouch.experimentalSlideDirectClick)
            val preset = newTouchPreset("Layout", touch.copy(experimentalSlideDirectClick = !enabled))
            assertEquals(enabled, touch.applyingTouchPreset(preset).experimentalSlideDirectClick)
        }
    }

    private fun routes(
        touch: AndroidTouchSettings = AndroidTouchSettings(),
        gamepadVisible: Boolean = true,
        ready: Boolean = true,
        touchEnabled: Boolean = true,
        nativeTouch: Boolean = false,
    ) = touchMouseEnabledForStream(touch, ready, touchEnabled, nativeTouch, gamepadVisible)

    @Test fun explicitOptOutBlocksBackgroundMouseOnlyWhileControllerIsVisible() {
        for (directClick in listOf(false, true)) {
            val touch = AndroidTouchSettings(mouseDirectClick = directClick, mouseWithTouchController = false)
            assertTrue(routes(touch, gamepadVisible = false))
            assertFalse(routes(touch, gamepadVisible = true))
            assertTrue(routes(touch, gamepadVisible = false))
        }
    }

    @Test fun defaultsAllowBothWithoutBypassingOtherInputGates() {
        val touch = AndroidTouchSettings()
        assertTrue(routes(touch))
        assertFalse(routes(touch.copy(mousePad = false)))
        assertFalse(routes(touch, ready = false))
        assertFalse(routes(touch, touchEnabled = false))
        assertFalse(routes(touch, nativeTouch = true))
    }

    @Test fun keyboardOverlayKeepsItsBackgroundMouse() {
        assertTrue(routes(AndroidTouchSettings(keyboardModeEnabled = true), gamepadVisible = false))
    }

    @Test fun legacySettingsAllowBackgroundMouseAndExplicitChoicePersists() {
        val legacy = OpenNowJson.decodeFromString<AppSettings>("""{"androidTouch":{"mousePad":true,"mouseDirectClick":true}}""")
        assertTrue(routes(legacy.androidTouch))
        for (allowMouse in listOf(false, true)) {
            val settings = legacy.copy(androidTouch = legacy.androidTouch.copy(mouseWithTouchController = allowMouse))
            val restored = OpenNowJson.decodeFromString<AppSettings>(OpenNowJson.encodeToString(settings)).normalizedForAndroid()
            assertEquals(allowMouse, restored.androidTouch.mouseWithTouchController)
        }
    }

    @Test fun applyingPresetPreservesBackgroundMouseChoice() {
        for (allowMouse in listOf(false, true)) {
            val touch = AndroidTouchSettings(mouseWithTouchController = allowMouse)
            val preset = newTouchPreset("Layout", touch.copy(mouseWithTouchController = !allowMouse))
            assertEquals(allowMouse, touch.applyingTouchPreset(preset).mouseWithTouchController)
        }
    }
}
