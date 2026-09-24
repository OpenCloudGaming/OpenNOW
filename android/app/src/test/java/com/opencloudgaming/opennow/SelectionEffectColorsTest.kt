package com.opencloudgaming.opennow

import androidx.compose.ui.graphics.Color
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import org.junit.Assert.*
import org.junit.Test

class SelectionEffectColorsTest {
    @Test fun acceptsOnlyCompleteRgbColors() {
        assertEquals(0x33CCFF, parseEffectColor(" #33ccff "))
        listOf("", "#FFF", "#FF33CCFF", "#GG00FF", "-00001", "12345").forEach { assertNull(parseEffectColor(it)) }
        assertEquals("#0033FF", effectColorHex(0x0033FF))
    }

    @Test fun savedColorsSurviveSerializationAndThemeChanges() {
        val settings = AppSettings(selectionEffectColors = SelectionEffectColors(0xFF2255, 0x33CCFF), absoluteCinemaEffects = true)
        val restored = Json.decodeFromString<AppSettings>(Json.encodeToString(settings)).copy(uiAccent = UiAccent.AbsoluteCinema)
        assertEquals(settings.selectionEffectColors, restored.selectionEffectColors)
        val (first, second) = controllerFocusEnergyColors(true, Color.White, Color.White, restored.selectionEffectColors)
        assertEquals(Color(0xFFFF2255), first)
        assertEquals(Color(0xFF33CCFF), second)
        assertEquals(restored.copy(selectionEffectColors = null).activeSelectionEffectStyle(), restored.activeSelectionEffectStyle())
        assertEquals(settings.absoluteCinemaEffects, restored.absoluteCinemaEffects)
    }

    @Test fun olderSettingsKeepTheirExistingPaletteAndEffectsState() {
        val restored = Json.decodeFromString<AppSettings>("{}")
        assertNull(restored.selectionEffectColors)
        assertFalse(restored.absoluteCinemaEffects)
        assertEquals(Color.Red to Color.Blue, controllerFocusEnergyColors(false, Color.Red, Color.Blue, restored.selectionEffectColors))
        assertEquals(controllerFocusEnergyColors(true, null, null), controllerFocusEnergyColors(true, null, null, restored.selectionEffectColors))
    }

    @Test fun normalizationKeepsColorsOpaqueAndDoesNotChangeThemeOrEnableEffects() {
        val restored = AppSettings(selectionEffectColors = SelectionEffectColors(0xAA33CCFF.toInt(), -1)).normalizedForAndroid()
        assertEquals(SelectionEffectColors(0x33CCFF, 0xFFFFFF), restored.selectionEffectColors)
        assertEquals(UiAccent.OpenNow, restored.uiAccent)
        assertFalse(restored.absoluteCinemaEffects)
    }
}
