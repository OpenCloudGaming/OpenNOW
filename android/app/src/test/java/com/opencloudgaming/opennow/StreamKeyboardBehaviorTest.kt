package com.opencloudgaming.opennow

import android.view.KeyEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamKeyboardBehaviorTest {
    @Test
    fun softKeyboardSpaceUsesAKeyStrokeInsteadOfUnicodeText() {
        assertEquals(
            listOf(StreamKeyboardInputChunk.SpaceKey),
            streamKeyboardInputChunks(" ", "en-US"),
        )
    }

    @Test
    fun unicodeTextRunsStayOrderedAroundSpaceKeys() {
        assertEquals(
            listOf(
                StreamKeyboardInputChunk.Text("hello"),
                StreamKeyboardInputChunk.SpaceKey,
                StreamKeyboardInputChunk.SpaceKey,
                StreamKeyboardInputChunk.Text("🙂world"),
            ),
            streamKeyboardInputChunks("hello  🙂world", "en-US"),
        )
    }

    @Test
    fun usKeyboardSymbolsBecomeKeyStrokesInPasswordText() {
        assertEquals(
            listOf(
                StreamKeyboardInputChunk.Text("pass"),
                StreamKeyboardInputChunk.PrintableKey(InputEncoder.TextKeySpec(0x38, 0x0009, shift = true)),
                StreamKeyboardInputChunk.Text("word"),
                StreamKeyboardInputChunk.PrintableKey(InputEncoder.TextKeySpec(0x31, 0x0002, shift = true)),
            ),
            streamKeyboardInputChunks("pass*word!", "en-US"),
        )
    }

    @Test
    fun usPasswordDotUsesAnUnshiftedPhysicalPeriodKey() {
        assertEquals(
            listOf(
                StreamKeyboardInputChunk.Text("pass"),
                StreamKeyboardInputChunk.PrintableKey(InputEncoder.TextKeySpec(0xbe, 0x0034)),
                StreamKeyboardInputChunk.Text("word"),
            ),
            streamKeyboardInputChunks("pass.word", "en-US"),
        )
    }

    @Test
    fun otherLayoutsAndInternationalCharactersStayOnUnicodePath() {
        assertEquals(
            listOf(StreamKeyboardInputChunk.Text("é*")),
            streamKeyboardInputChunks("é*", "fr-FR"),
        )
        assertEquals(
            listOf(
                StreamKeyboardInputChunk.Text("é"),
                StreamKeyboardInputChunk.PrintableKey(InputEncoder.TextKeySpec(0x38, 0x0009, shift = true)),
            ),
            streamKeyboardInputChunks("é*", "en-US"),
        )
    }

    @Test
    fun emptySoftKeyboardEditProducesNoInputChunks() {
        assertEquals(emptyList<StreamKeyboardInputChunk>(), streamKeyboardInputChunks("", "en-US"))
    }

    @Test
    fun typingAndPastingVerificationDigitsProduceTheSameOrderedKeys() {
        val code = "700118"
        val typed = code.indices.flatMap { index ->
            val edit = streamKeyboardEdit(code.take(index), code.take(index + 1)) as StreamKeyboardEdit.Append
            streamKeyboardInputChunks(edit.text, "en-US")
        }
        val pasted = streamKeyboardEdit(null, code) as StreamKeyboardEdit.Append
        val expected = listOf(7, 0, 0, 1, 1, 8).map { digit ->
            val scanCode = if (digit == 0) 0x0b else digit + 1
            StreamKeyboardInputChunk.PrintableKey(InputEncoder.TextKeySpec(0x30 + digit, scanCode))
        }
        assertEquals(expected, typed)
        assertEquals(expected, streamKeyboardInputChunks(pasted.text, "en-US"))
    }

    @Test
    fun frenchLayoutShiftsTheDigitRowAndKeepsInternationalText() {
        assertEquals(
            listOf(
                StreamKeyboardInputChunk.Text("é"),
                StreamKeyboardInputChunk.PrintableKey(InputEncoder.TextKeySpec(0x31, 0x0002, shift = true)),
            ),
            streamKeyboardInputChunks("é1", "fr-FR"),
        )
    }

    @Test
    fun bulkDigitPastesUseUnicodeToLimitQueuedKeystrokes() {
        val text = "0123456789".repeat(20)
        assertEquals(listOf(StreamKeyboardInputChunk.Text(text)), streamKeyboardInputChunks(text, "en-US"))
    }

    @Test
    fun emptyNewDraftHasNothingToType() {
        assertEquals(StreamKeyboardEdit.None, streamKeyboardEdit(null, ""))
    }

    @Test
    fun newDraftIsAppendedToTheRemoteField() {
        assertEquals(StreamKeyboardEdit.Append("hello"), streamKeyboardEdit(null, "hello"))
    }

    @Test
    fun unchangedMirroredTextIsNotDuplicated() {
        assertEquals(StreamKeyboardEdit.None, streamKeyboardEdit("hello", "hello"))
    }

    @Test
    fun typingAtTheEndOnlyAppendsTheNewSuffix() {
        assertEquals(StreamKeyboardEdit.Append(" there"), streamKeyboardEdit("hello", "hello there"))
    }

    @Test
    fun deletingAtTheEndUsesBackspace() {
        assertEquals(StreamKeyboardEdit.Backspace(2), streamKeyboardEdit("hello", "hel"))
    }

    @Test
    fun deletingAnEmojiUsesOneRemoteBackspace() {
        assertEquals(StreamKeyboardEdit.Backspace(1), streamKeyboardEdit("hello 🙂", "hello "))
    }

    @Test
    fun editingInTheMiddleRewindsOnlyTheChangedSuffix() {
        assertEquals(StreamKeyboardEdit.ReplaceSuffix(4, "allo"), streamKeyboardEdit("hello", "hallo"))
    }

    @Test
    fun imeEmailCorrectionDoesNotSelectAllOrClearTheRemoteField() {
        assertEquals(
            StreamKeyboardEdit.ReplaceSuffix(5, "il.com"),
            streamKeyboardEdit("user@gmal.com", "user@gmail.com"),
        )
    }

    @Test
    fun composingReplacementCountsUnicodeCharactersInsteadOfSurrogateHalves() {
        assertEquals(StreamKeyboardEdit.ReplaceSuffix(2, "🙃x"), streamKeyboardEdit("a🙂x", "a🙃x"))
    }

    @Test
    fun typingAnEmailAndPastingItProduceTheSameRemoteText() {
        val email = "a.b+test@gmail.com"
        val typed = email.indices.joinToString("") { index ->
            (streamKeyboardEdit(email.take(index), email.take(index + 1)) as StreamKeyboardEdit.Append).text
        }
        assertEquals(StreamKeyboardEdit.Append(typed), streamKeyboardEdit(null, email))
    }

    @Test
    fun hardwareKeyboardRepeatsAreSuppressedAfterInitialKeyDown() {
        assertFalse(shouldSuppressHardwareKeyboardRepeat(true, KeyEvent.ACTION_DOWN, repeatCount = 0))
        assertTrue(shouldSuppressHardwareKeyboardRepeat(true, KeyEvent.ACTION_DOWN, repeatCount = 1))
        assertTrue(shouldSuppressHardwareKeyboardRepeat(true, KeyEvent.ACTION_DOWN, repeatCount = 200))
    }

    @Test
    fun keyUpAndNonHardwareSourcesAreNotSuppressed() {
        assertFalse(shouldSuppressHardwareKeyboardRepeat(true, KeyEvent.ACTION_UP, repeatCount = 1))
        assertFalse(shouldSuppressHardwareKeyboardRepeat(false, KeyEvent.ACTION_DOWN, repeatCount = 1))
    }
}
