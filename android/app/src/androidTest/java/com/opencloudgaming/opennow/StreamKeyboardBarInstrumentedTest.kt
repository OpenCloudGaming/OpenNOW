package com.opencloudgaming.opennow

import android.content.ClipData
import android.content.ClipboardManager
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.mutableStateOf
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextClearance
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.core.content.getSystemService
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test

class StreamKeyboardBarInstrumentedTest {
    @get:Rule val compose = createComposeRule()

    @Test fun atKeyReplacesTheSelectedTextThroughTheKeyboardEditPath() {
        val draft = mutableStateOf(TextFieldValue("user?example.com", TextRange(4, 5)))
        showKeyboard(draft)
        compose.onNodeWithText("@").performClick()
        compose.runOnIdle {
            assertEquals("user@example.com", draft.value.text)
            assertEquals(TextRange(5), draft.value.selection)
        }
    }

    @Test fun softKeyboardAndClipboardDeliverAllVerificationDigitsToTheEditor() {
        val draft = mutableStateOf(TextFieldValue())
        val context = InstrumentationRegistry.getInstrumentation().targetContext
        val clipboard = checkNotNull(context.getSystemService<ClipboardManager>())
        showKeyboard(draft)

        val editor = compose.onNode(hasSetTextAction())
        val code = "700118"
        editor.performClick()
        code.forEach { editor.performTextInput(it.toString()) }
        compose.runOnIdle { assertEquals(code, draft.value.text) }

        editor.performTextClearance()
        // An IME clipboard tile commits the whole string in one input change.
        editor.performTextInput(code)
        compose.runOnIdle { assertEquals(code, draft.value.text) }

        editor.performTextClearance()
        compose.runOnIdle { clipboard.setPrimaryClip(ClipData.newPlainText("Verification code", code)) }
        editor.performSemanticsAction(SemanticsActions.PasteText)
        compose.runOnIdle { assertEquals(code, draft.value.text) }
    }

    private fun showKeyboard(draft: MutableState<TextFieldValue>) {
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
    }
}
