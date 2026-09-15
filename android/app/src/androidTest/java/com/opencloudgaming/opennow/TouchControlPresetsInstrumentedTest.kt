package com.opencloudgaming.opennow

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.unit.dp
import org.junit.Assert.*
import org.junit.Rule
import org.junit.Test

class TouchControlPresetsInstrumentedTest {
    @get:Rule val compose = createComposeRule()

    @Test fun shapeAndToggleSaveWithAnIcon() {
        var touch by mutableStateOf(AndroidTouchSettings())
        compose.setContent { MaterialTheme { TouchButtonAppearanceEditor(touch) { touch = it } } }
        compose.onNodeWithText("Customize buttons").performClick()
        compose.onNodeWithText("A").performClick()
        compose.onNodeWithText("Square").performScrollTo().performClick()
        compose.onNode(isToggleable()).performScrollTo().performClick()
        compose.onNodeWithContentDescription("Flashlight").performScrollTo().performClick()
        compose.onNodeWithText("Save").performClick()
        compose.runOnIdle {
            assertEquals(TouchButtonAppearance(icon = "Flashlight", shape = TouchButtonShape.Square, toggle = true), touch.buttonAppearances["A"])
        }
    }

    @Test fun saveLoadReplaceAndDeleteGamePreset() {
        var touch by mutableStateOf(AndroidTouchSettings().withOffset("aimzone_landscape", -80f, -20f))
        var presets by mutableStateOf(emptyList<TouchControlPreset>())
        compose.setContent { MaterialTheme { TouchControlPresetEditor(touch, presets, { touch = it }, { presets = it }) } }
        compose.onNodeWithText("Presets and themes").performClick()
        compose.onNode(hasSetTextAction()).performScrollTo().performTextInput("My game")
        compose.onNodeWithText("Save current controls").performScrollTo().performClick()
        compose.runOnIdle { touch = touch.withOffset("aimzone_landscape", 0f, 0f) }
        compose.onNodeWithText("Load").performScrollTo().performClick()
        compose.runOnIdle { assertEquals(TouchOffset(-80f, -20f), touch.getOffset("aimzone_landscape")) }
        compose.runOnIdle { touch = touch.withButtonAppearance("RT", TouchButtonAppearance(toggle = true)) }
        compose.onNodeWithText("Save over").performScrollTo().performClick()
        compose.runOnIdle { assertTrue(presets.single().controls.buttonAppearances["RT"]!!.toggle) }
        compose.onNodeWithText("Delete").performScrollTo().performClick()
        compose.runOnIdle { assertTrue(presets.isEmpty()) }
    }

    @Test fun toggleInputReleasesWhenDisabledReconfiguredOrRemoved() {
        val owner = Any()
        var enabled by mutableStateOf(true)
        var visible by mutableStateOf(true)
        var controlKey by mutableStateOf("A")
        var pressed = false
        compose.setContent {
            if (visible) {
                val onPressed = rememberUpdatedState<(Boolean) -> Unit> { pressed = it }
                Box(Modifier.size(80.dp).testTag("button").virtualPressInput(owner, controlKey, onPressed, true, enabled))
            }
        }
        compose.onNodeWithTag("button").performTouchInput { click() }
        compose.runOnIdle { assertTrue(pressed) }
        compose.onNodeWithTag("button").performTouchInput { click() }
        compose.runOnIdle { assertFalse(pressed) }
        compose.onNodeWithTag("button").performTouchInput { click() }
        compose.runOnIdle { enabled = false }
        compose.runOnIdle { assertFalse(pressed); enabled = true }
        compose.onNodeWithTag("button").performTouchInput { click() }
        compose.runOnIdle { assertTrue(pressed); controlKey = "B" }
        compose.runOnIdle { assertFalse(pressed) }
        compose.onNodeWithTag("button").performTouchInput { click() }
        compose.runOnIdle { assertTrue(pressed); visible = false }
        compose.runOnIdle { assertFalse(pressed) }
    }
}
