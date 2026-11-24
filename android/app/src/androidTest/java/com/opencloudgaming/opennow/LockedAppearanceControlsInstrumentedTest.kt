package com.opencloudgaming.opennow

import androidx.activity.ComponentActivity
import androidx.compose.foundation.layout.Column
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import org.junit.Rule
import org.junit.Test

class LockedAppearanceControlsInstrumentedTest {
    @get:Rule val compose = createAndroidComposeRule<ComponentActivity>()

    @Test
    fun appearanceChoicesStayVisibleAndCannotBeActivatedWhileLocked() {
        val unlocked = mutableStateOf(false)
        compose.setContent {
            MaterialTheme {
                Column {
                    ChoiceRow("Accent", listOf("Violet", "Green"), "Violet", enabled = unlocked.value) {
                        error("A locked control changed the selected accent")
                    }
                    SettingSwitch("Use system colors", checked = true, enabled = unlocked.value) {}
                }
            }
        }

        val accent = compose.onNodeWithText("Violet")
        accent.assertIsDisplayed().assertIsNotEnabled()
        compose.onNodeWithText("Use system colors").assertIsDisplayed().assertIsNotEnabled()
        compose.runOnIdle { unlocked.value = true }
        accent.assertIsEnabled()
        compose.onNodeWithText("Use system colors").assertIsEnabled()
    }
}
