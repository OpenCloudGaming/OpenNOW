package com.opencloudgaming.opennow

import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

// Leave a synthetic press visible for more than one 60 Hz game-input poll.
internal const val STREAM_KEY_PRESS_DURATION_MS = 32L
private const val MAX_PACED_DIGITS_PER_EDIT = 16

/** Always release an accepted synthetic press, including when its caller is cancelled. */
internal suspend fun sendStreamKeyboardKeyStroke(send: suspend (pressed: Boolean) -> Boolean): Boolean {
    if (!send(true)) return false
    val released: Boolean
    try {
        delay(STREAM_KEY_PRESS_DURATION_MS)
    } finally {
        released = withContext(NonCancellable) { send(false) }
    }
    // Repeated Backspace presses also need a visible release between them.
    if (released) delay(STREAM_KEY_PRESS_DURATION_MS)
    return released
}

/** Hold Shift across a printable key and release it even if the key send is cancelled. */
internal suspend fun sendStreamKeyboardPrintableKeyStroke(
    spec: InputEncoder.TextKeySpec,
    send: suspend (InputEncoder.KeyboardPayload, Boolean) -> Boolean,
): Boolean {
    val key = spec.toKeyboardPayload(if (spec.shift) 0x01 else 0)
    if (!spec.shift) return sendStreamKeyboardKeyStroke { pressed -> send(key, pressed) }
    if (!send(InputEncoder.shiftLeftPayload(0x01), true)) return false
    val keySent: Boolean
    val shiftReleased: Boolean
    try {
        keySent = sendStreamKeyboardKeyStroke { pressed -> send(key, pressed) }
    } finally {
        shiftReleased = withContext(NonCancellable) { send(InputEncoder.shiftLeftPayload(0), false) }
    }
    return keySent && shiftReleased
}

/**
 * Some game fields expect keystrokes for spaces, verification digits, and US-layout symbols.
 * Keep other text in Unicode runs and pace short digit sequences so repeated keys register.
 */
internal sealed interface StreamKeyboardInputChunk {
    data class Text(val value: String) : StreamKeyboardInputChunk
    data object SpaceKey : StreamKeyboardInputChunk
    data class PrintableKey(val spec: InputEncoder.TextKeySpec) : StreamKeyboardInputChunk
}

internal fun streamKeyboardInputChunks(text: String, keyboardLayout: String): List<StreamKeyboardInputChunk> {
    if (text.isEmpty()) return emptyList()
    val physicalSymbols = keyboardLayout == "en-US"
    // Avoid turning large clipboard pastes into seconds of queued keystrokes.
    val physicalDigits = text.length <= MAX_PACED_DIGITS_PER_EDIT ||
        text.count { it in '0'..'9' } <= MAX_PACED_DIGITS_PER_EDIT
    val chunks = mutableListOf<StreamKeyboardInputChunk>()
    var textStart = 0
    text.forEachIndexed { index, char ->
        val space = char == ' '
        val digit = physicalDigits && char in '0'..'9'
        val symbol = physicalSymbols && (char in '!'..'/' || char in ':'..'@' || char in '['..'`' || char in '{'..'~')
        val key = if (digit || symbol) InputEncoder.mapTextCharToKeySpec(char) else null
        val chunk = when {
            space -> StreamKeyboardInputChunk.SpaceKey
            key != null -> StreamKeyboardInputChunk.PrintableKey(
                if (digit && keyboardLayout == "fr-FR") key.copy(shift = true) else key,
            )
            else -> return@forEachIndexed
        }
        if (textStart < index) {
            chunks += StreamKeyboardInputChunk.Text(text.substring(textStart, index))
        }
        chunks += chunk
        textStart = index + 1
    }
    if (textStart < text.length) {
        chunks += StreamKeyboardInputChunk.Text(text.substring(textStart))
    }
    return chunks
}

/** A minimal remote edit that keeps the host field aligned with the locally mirrored draft. */
internal sealed interface StreamKeyboardEdit {
    data object None : StreamKeyboardEdit
    data class Append(val text: String) : StreamKeyboardEdit
    data class Backspace(val count: Int) : StreamKeyboardEdit
    data class ReplaceSuffix(val backspaces: Int, val text: String) : StreamKeyboardEdit
}

internal fun streamKeyboardEdit(syncedText: String?, draft: String): StreamKeyboardEdit {
    val previous = syncedText.orEmpty()
    return when {
        previous == draft -> StreamKeyboardEdit.None
        draft.startsWith(previous) -> StreamKeyboardEdit.Append(draft.removePrefix(previous))
        previous.startsWith(draft) -> StreamKeyboardEdit.Backspace(
            previous.codePointCount(draft.length, previous.length),
        )
        else -> {
            // IMEs revise composing words while typing. Ctrl+A/Delete would clear the entire
            // host field (including text we never entered), and many game fields ignore Ctrl+A.
            // The remote caret is at the end of our mirrored draft: rewind only the changed tail.
            var prefix = 0
            while (prefix < previous.length && prefix < draft.length) {
                val before = previous.codePointAt(prefix)
                if (before != draft.codePointAt(prefix)) break
                prefix += Character.charCount(before)
            }
            StreamKeyboardEdit.ReplaceSuffix(
                previous.codePointCount(prefix, previous.length),
                draft.substring(prefix),
            )
        }
    }
}
