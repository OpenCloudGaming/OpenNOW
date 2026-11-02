package com.opencloudgaming.opennow

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Column
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.SideEffect
import androidx.compose.ui.input.InputMode
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.platform.LocalInputModeManager
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.*
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class SettingsDescriptionInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()
    private var changes = 0

    private fun show(space: Boolean = false) {
        compose.setContent {
            val inputMode = LocalInputModeManager.current
            SideEffect { inputMode.requestInputMode(InputMode.Keyboard) }
            OpenNowTheme(AppSettings(), physicalControllerConnected = true) {
                SpaceUiTheme(enabled = space) {
                    CompositionLocalProvider(LocalSettingsControllerNavigationEnabled provides true) {
                        Column {
                            SettingSwitch("Test setting", checked = false, description = "Switch help") { changes++ }
                            ChoiceRow("Test choice", listOf("One", "Two"), "One", description = "Choice help") {}
                        }
                    }
                }
            }
        }
    }

    private fun holdRowThenClick() {
        val row = compose.onNode(hasText("Test setting") and hasClickAction())
        row.performSemanticsAction(SemanticsActions.RequestFocus) { it() }
        row.assertIsFocused()
        row.performKeyInput { keyDown(Key.DirectionCenter) }
        compose.mainClock.advanceTimeBy(1_000)
        compose.onNodeWithText("Switch help").assertIsDisplayed()
        row.performKeyInput { keyUp(Key.DirectionCenter) }
        compose.runOnIdle { assertEquals(0, changes) }
        row.performKeyInput { pressKey(Key.DirectionCenter) }
        compose.runOnIdle { assertEquals(1, changes) }
    }

    @Test fun defaultLauncherHoldShowsHelpWithoutToggling() {
        show()
        holdRowThenClick()
    }

    @Test fun spaceLauncherHoldShowsHelpWithoutToggling() {
        show(space = true)
        holdRowThenClick()
    }

    @Test fun focusedHelpIconReceivesControllerActivation() {
        show()
        val help = compose.onAllNodesWithContentDescription("Show description")[0]
        help.performSemanticsAction(SemanticsActions.RequestFocus) { it() }
        help.assertIsFocused()
        help.performKeyInput { pressKey(Key.DirectionCenter) }
        compose.onNodeWithText("Switch help").assertIsDisplayed()
        compose.runOnIdle { assertEquals(0, changes) }
    }

    @Test fun focusedChoiceHelpDoesNotOpenSelector() {
        show()
        val help = compose.onAllNodesWithContentDescription("Show description")[1]
        help.performSemanticsAction(SemanticsActions.RequestFocus) { it() }
        help.assertIsFocused()
        help.performKeyInput { pressKey(Key.DirectionCenter) }
        compose.onNodeWithText("Choice help").assertIsDisplayed()
        compose.onNodeWithText("Two").assertDoesNotExist()
    }
}
