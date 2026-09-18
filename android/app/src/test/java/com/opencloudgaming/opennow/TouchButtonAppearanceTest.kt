package com.opencloudgaming.opennow

import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import org.junit.Assert.*
import org.junit.Test

class TouchButtonAppearanceTest {
    @Test fun customizationRoundTripsAndDoesNotChangeTheAssignedAction() {
        val touch = AndroidTouchSettings()
            .withExtraButtonAction(0, TouchExtraButtonAction.RightTrigger)
            .withButtonAppearance("RT", TouchButtonAppearance("Shoot", "Shoot", sizeScale = 1.65f))
            .withButtonAppearance("extra1", TouchButtonAppearance("Fire"))
        val restored = OpenNowJson.decodeFromString<AndroidTouchSettings>(OpenNowJson.encodeToString(touch))
        assertEquals(touch, restored)
        assertEquals(TouchExtraButtonAction.RightTrigger, restored.extraButtonAction(0))
        assertEquals("Shoot", restored.buttonAppearances["RT"]?.label)
        assertEquals(1.65f, restored.buttonAppearances.getValue("RT").sizeScale, 0.001f)
        assertEquals("Fire", restored.buttonAppearances["extra1"]?.label)
        assertFalse(restored.buttonAppearances.containsKey("LT"))
    }

    @Test fun oldSettingsKeepDefaultLabelsAndResetOnlyChangesOneButton() {
        assertTrue(OpenNowJson.decodeFromString<AndroidTouchSettings>("{}").buttonAppearances.isEmpty())
        val touch = AndroidTouchSettings()
            .withButtonAppearance("RT", TouchButtonAppearance("Shoot"))
            .withButtonAppearance("A", TouchButtonAppearance("Jump"))
            .withButtonAppearance("RT", TouchButtonAppearance())
        assertFalse(touch.buttonAppearances.containsKey("RT"))
        assertEquals("Jump", touch.buttonAppearances["A"]?.label)
        assertEquals(AndroidTouchSettings().offsets, touch.offsets)
    }

    @Test fun normalizationRejectsUnknownIconsKeysAndInvalidCoordinatesWithoutLimitingCrossScreenOffsets() {
        val touch = AndroidTouchSettings(
            buttonAppearances = mapOf(
                "RT" to TouchButtonAppearance("  Shoot\n  ", "unknown"),
                "unknown" to TouchButtonAppearance("bad"),
                "A" to TouchButtonAppearance("1234567890123456"),
            ),
            offsets = mapOf("lt_landscape" to TouchOffset(950f, -610f), "lb_landscape" to TouchOffset(Float.NaN, Float.POSITIVE_INFINITY)),
        )
        val normalized = AppSettings(androidTouch = touch).normalizedForAndroid().androidTouch
        assertEquals(TouchButtonAppearance("Shoot"), normalized.buttonAppearances["RT"])
        assertEquals(12, normalized.buttonAppearances["A"]?.label?.length)
        assertFalse(normalized.buttonAppearances.containsKey("unknown"))
        assertEquals(TouchOffset(950f, -610f), normalized.offsets["lt_landscape"])
        assertEquals(TouchOffset(), normalized.offsets["lb_landscape"])
        val restored = OpenNowJson.decodeFromString<AndroidTouchSettings>(OpenNowJson.encodeToString(normalized))
        assertEquals(normalized.offsets, restored.offsets)
    }

    @Test fun individualSizesNormalizeAndSurvivePresetSharing() {
        assertEquals(1f, OpenNowJson.decodeFromString<TouchButtonAppearance>("{\"label\":\"Jump\"}").sizeScale, 0f)
        listOf(Float.NaN, Float.POSITIVE_INFINITY, Float.NEGATIVE_INFINITY).forEach { invalid ->
            assertEquals(1f, TouchButtonAppearance(sizeScale = invalid).normalized().sizeScale, 0f)
        }
        assertEquals(0.5f, TouchButtonAppearance(sizeScale = -1f).normalized().sizeScale, 0f)
        assertEquals(2f, TouchButtonAppearance(sizeScale = 20f).normalized().sizeScale, 0f)
        val controls = AndroidTouchSettings()
            .withButtonAppearance("A", TouchButtonAppearance(sizeScale = 1.5f))
            .withButtonAppearance("B", TouchButtonAppearance(sizeScale = 0.75f))
        val imported = importTouchPreset(exportTouchPreset(newTouchPreset("Sizes", controls)))!!
        assertEquals(controls.buttonAppearances, imported.controls.buttonAppearances)
        val reset = controls.withButtonAppearance("A", TouchButtonAppearance())
        assertFalse(reset.buttonAppearances.containsKey("A"))
        assertEquals(0.75f, reset.buttonAppearances.getValue("B").sizeScale, 0f)
    }

    @Test fun yamlPresetRoundTripsGenreMetadataAndStillImportsLegacyJson() {
        val preset = newTouchPreset(
            name = "Racing custom",
            controls = AndroidTouchSettings().withExtraButtonCombo(
                0,
                listOf(TouchExtraButtonAction.LeftStickLeft, TouchExtraButtonAction.KeyboardA),
            ),
            genre = TouchPresetGenre.Racing,
            description = "Steering tuned for touch",
        )
        val yaml = exportTouchPreset(preset)
        assertTrue(yaml.contains("format: opennow-touch"))
        assertTrue(yaml.contains("genre: Racing"))
        val restored = importTouchPreset(yaml)!!
        assertEquals(TouchPresetGenre.Racing, restored.genre)
        assertEquals("Steering tuned for touch", restored.description)
        assertEquals(TouchExtraButtonAction.LeftStickLeft, restored.controls.extraButtonAction(0))
        assertEquals(
            listOf(TouchExtraButtonAction.LeftStickLeft, TouchExtraButtonAction.KeyboardA),
            restored.controls.extraButtonCombo(0),
        )
        assertTrue(yaml.contains("extraButtonCombos:"))

        val legacy = importTouchPreset(exportTouchPresetJson(preset))!!
        assertEquals(restored.controls, legacy.controls)
        assertEquals(restored.genre, legacy.genre)
    }

    @Test fun builtInPresetsAreGenreBasedAndRacingHasDirectSteeringButtons() {
        val builtIns = builtinTouchPresets()
        assertEquals(listOf("Racing", "Survival", "Horror", "Action"), builtIns.takeLast(4).map { it.name })
        val racing = builtIns.first { it.genre == TouchPresetGenre.Racing }
        assertEquals(TouchExtraButtonAction.LeftStickLeft, racing.controls.extraButtonAction(0))
        assertEquals(TouchExtraButtonAction.LeftStickRight, racing.controls.extraButtonAction(1))
    }

    @Test fun draggingCrossesBothWaysAndStopsAtTheScreenEdges() {
        assertEquals(800f, draggedTouchOffset(0f, 800f, 40f, 120f, 1000f), 0.001f)
        assertEquals(880f, draggedTouchOffset(0f, 1200f, 40f, 120f, 1000f), 0.001f)
        assertEquals(-800f, draggedTouchOffset(0f, -800f, 880f, 960f, 1000f), 0.001f)
        assertEquals(-880f, draggedTouchOffset(0f, -1200f, 880f, 960f, 1000f), 0.001f)
        assertEquals(110f, draggedTouchOffset(-50f, 999f, 100f, 240f, 400f), 0.001f)
    }
}
