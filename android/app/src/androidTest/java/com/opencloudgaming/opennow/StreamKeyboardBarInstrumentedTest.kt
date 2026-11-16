package com.opencloudgaming.opennow

import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class StreamKeyboardBarInstrumentedTest {
    @get:Rule val compose = createComposeRule()

    @Test fun atKeyReplacesTheSelectedTextThroughTheKeyboardEditPath() {
        val draft = mutableStateOf(TextFieldValue("user?example.com", TextRange(4, 5)))
        compose.setContent {
            OpenNowTheme(AppSettings(), physicalControllerConnected = false) {
                StreamKeyboardBar(
                    value = draft.value,
                    onValueChange = { draft.value = it },
                    clearConfirmationEnabled = false,
                    onClear = {},
                    onDisableClearConfirmation = {},
                    onEnter = {},
                    onEsc = {},
                    onDone = {},
                )
            }
        }
        compose.onNodeWithText("@").performClick()
        compose.runOnIdle {
            assertEquals("user@example.com", draft.value.text)
            assertEquals(TextRange(5), draft.value.selection)
        }
    }
}
