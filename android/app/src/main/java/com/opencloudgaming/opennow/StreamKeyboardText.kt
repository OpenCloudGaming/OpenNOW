package com.opencloudgaming.opennow

import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext

// Leave a synthetic press visible for more than one 60 Hz game-input poll.
internal const val STREAM_KEY_PRESS_DURATION_MS = 32L

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

/**
 * SendUnicode is correct for committed text, but a soft-keyboard Space also needs to behave like
 * the physical key for games. Keep text runs intact and surface literal spaces as held key strokes.
 */
internal sealed interface StreamKeyboardInputChunk {
    data class Text(val value: String) : StreamKeyboardInputChunk
    data object SpaceKey : StreamKeyboardInputChunk
}

internal fun streamKeyboardInputChunks(text: String): List<StreamKeyboardInputChunk> {
    if (text.isEmpty()) return emptyList()
    val chunks = mutableListOf<StreamKeyboardInputChunk>()
    var textStart = 0
    text.forEachIndexed { index, char ->
        if (char != ' ') return@forEachIndexed
        if (textStart < index) {
            chunks += StreamKeyboardInputChunk.Text(text.substring(textStart, index))
        }
        chunks += StreamKeyboardInputChunk.SpaceKey
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
