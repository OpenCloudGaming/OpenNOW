package com.opencloudgaming.opennow

import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Test

class TouchControlPresetsTest {
    @Test fun presetsPersistEveryControlOptionAndLoadWithoutChangingInputMode() {
        val controls = AndroidTouchSettings(
            aimMode = TouchAimMode.LockZone,
            buttonAppearances = mapOf("RT" to TouchButtonAppearance("Light", "Flashlight", TouchButtonShape.Square, true)),
            touchSkinTint = ControllerThemeRgb(10, 20, 30),
        ).withOffset("aimzone_landscape", -170f, 20f).withOffset("aimzone_portrait", 15f, -80f)
        val preset = newTouchPreset("My game", controls)
        val settings = AppSettings(touchControlPresets = listOf(preset))
        val restored = OpenNowJson.decodeFromString<AppSettings>(OpenNowJson.encodeToString(settings)).normalizedForAndroid()
        assertEquals(listOf(preset), restored.touchControlPresets)
        val current = AndroidTouchSettings(enabled = false, nativeTouchMode = NativeTouchMode.Off, mousePad = false)
        val loaded = current.applyingTouchPreset(restored.touchControlPresets.single())
        assertEquals(controls.buttonAppearances, loaded.buttonAppearances)
        assertEquals(controls.offsets, loaded.offsets)
        assertEquals(controls.touchSkinTint, loaded.touchSkinTint)
        assertFalse(loaded.enabled)
        assertFalse(loaded.mousePad)
        assertEquals(NativeTouchMode.Off, loaded.nativeTouchMode)
    }

    @Test fun sharedPresetRoundTripsAndGetsANewLibraryIdentity() {
        val preset = newTouchPreset("Game", AndroidTouchSettings().withButtonAppearance("extra1",
            TouchButtonAppearance("Radio", "Communicator", TouchButtonShape.Trigger, true)))
        val imported = importTouchPreset(exportTouchPreset(preset))!!
        assertEquals(preset.name, imported.name)
        assertEquals(preset.controls, imported.controls)
        assertNotEquals(preset.id, imported.id)
    }

    @Test fun invalidImportsAreRejectedAndValidValuesAreNormalized() {
        assertNull(importTouchPreset("{}"))
        assertNull(importTouchPreset("not json"))
        assertNull(importTouchPreset(" ".repeat(MAX_TOUCH_PRESET_CODE + 1)))
        val code = exportTouchPreset(newTouchPreset("Game", AndroidTouchSettings()))
        assertNull(importTouchPreset(code.replace("\"version\":1", "\"version\":2")))
        assertNull(importTouchPreset(code.replace("opennow-touch", "another-format")))
        val imported = importTouchPreset(code.replace("\"aimZoneScale\":1.0", "\"aimZoneScale\":999"))!!
        assertEquals(1.5f, imported.controls.aimZoneScale)
    }

    @Test fun oldSettingsStartWithoutPresetsAndBuiltinsHaveExpectedLabels() {
        assertTrue(OpenNowJson.decodeFromString<AppSettings>("{}").touchControlPresets.isEmpty())
        val presets = builtinTouchPresets()
        assertTrue(presets.first().controls.buttonAppearances.isEmpty())
        val playstation = presets.first { it.id == "playstation" }
        assertEquals("×", playstation.controls.buttonAppearances["A"]?.label)
        assertEquals("L2", playstation.controls.buttonAppearances["LT"]?.label)
        assertTrue(presets.any { it.id == "forza-horizon" })
        assertTrue(presets.any { it.id == "resident-evil" })
        assertTrue(presets.any { it.id == "first-person-shooter" })
        assertTrue(presets.any { it.id == "action-adventure" })
        assertTrue(presets.filter { it.id !in setOf("xbox", "playstation") }
            .all { it.controls.extraButtonActions.size == TOUCH_EXTRA_BUTTON_COUNT })
    }

    @Test fun momentaryAndToggleButtonsReleaseOnCancellation() {
        val held = TouchButtonPressState(false)
        assertTrue(held.down())
        assertFalse(held.up())
        val toggle = TouchButtonPressState(true)
        assertTrue(toggle.down())
        assertTrue(toggle.up())
        assertFalse(toggle.down())
        assertFalse(toggle.up())
        assertTrue(toggle.down())
        assertFalse(toggle.cancel())
        assertTrue(toggle.down())
        assertFalse(toggle.cancel())
        assertFalse(TouchButtonPressState(true).up())
    }

    @Test fun normalizingLibraryBoundsNamesAndCountAndRemovesDuplicateIds() {
        val preset = newTouchPreset("Game", AndroidTouchSettings())
        val settings = AppSettings(touchControlPresets = List(50) { preset.copy(id = "$it", name = "x".repeat(100)) })
            .normalizedForAndroid()
        assertEquals(MAX_TOUCH_PRESETS, settings.touchControlPresets.size)
        assertTrue(settings.touchControlPresets.all { it.name.length == 64 })
        assertEquals(1, AppSettings(touchControlPresets = listOf(preset, preset)).normalizedForAndroid().touchControlPresets.size)
    }
}
