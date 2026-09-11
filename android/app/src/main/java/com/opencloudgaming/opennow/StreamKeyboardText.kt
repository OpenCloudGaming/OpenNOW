package com.opencloudgaming.opennow

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
