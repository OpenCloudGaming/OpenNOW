package com.opencloudgaming.opennow

import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import com.opencloudgaming.opennow.ui.theme.LocalReduceControllerFocusMotion
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class SelectionEffectColorsInstrumentedTest {
    @get:Rule val compose = createComposeRule()

    @Test fun validatesSavesCancelsAndRestoresThemeColors() {
        var settings by mutableStateOf(AppSettings(uiAccent = UiAccent.Violet))
        compose.setContent {
            MaterialTheme {
                CompositionLocalProvider(LocalReduceControllerFocusMotion provides true) {
                    SelectionEffectColorSetting(settings) { settings = settings.copy(selectionEffectColors = it) }
                }
            }
        }
        compose.onNodeWithText("Effect colors · Follow theme").performClick()
        compose.onNodeWithText("First · #RRGGBB").performScrollTo().performTextReplacement("#GG0000")
        compose.onNodeWithText("Save colors").assertIsNotEnabled()
        compose.onNodeWithText("First · #RRGGBB").performTextReplacement("#FF2255")
        compose.onNodeWithText("Second · #RRGGBB").performTextReplacement("#33CCFF")
        compose.onNodeWithText("Live preview").performScrollTo()
        compose.waitForIdle()
        val instrumentation = androidx.test.platform.app.InstrumentationRegistry.getInstrumentation()
        val screenshot = instrumentation.uiAutomation.takeScreenshot()
        val output = java.io.File(instrumentation.targetContext.getExternalFilesDir(null), "effect-color-picker.png")
        output.outputStream().use { screenshot.compress(android.graphics.Bitmap.CompressFormat.PNG, 100, it) }
        screenshot.recycle()
        compose.onNodeWithText("Save colors").performClick()
        compose.runOnIdle {
            assertEquals(SelectionEffectColors(0xFF2255, 0x33CCFF), settings.selectionEffectColors)
            assertEquals(UiAccent.Violet, settings.uiAccent)
            assertFalse(settings.absoluteCinemaEffects)
        }
        compose.onNodeWithText("Effect colors · Custom").performClick()
        compose.onNodeWithText("First · #RRGGBB").performScrollTo().performTextReplacement("#FFFFFF")
        compose.onNodeWithText("Cancel").performClick()
        compose.runOnIdle { assertEquals(0xFF2255, settings.selectionEffectColors!!.firstRgb) }
        compose.onNodeWithText("Effect colors · Custom").performClick()
        compose.onNodeWithText("Follow theme colors").performScrollTo().performClick()
        compose.runOnIdle { assertNull(settings.selectionEffectColors) }
    }
}
