package com.opencloudgaming.opennow

import android.content.ClipData
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.Clipboard

internal suspend fun Clipboard.copyPlainText(value: String) {
    setClipEntry(ClipEntry(ClipData.newPlainText("OpenNOW", value)))
}

internal const val MAX_LOG_CLIPBOARD_CHARS = 96_000

/**
 * Android sends clipboard contents through Binder. Keep log copies comfortably below the
 * transaction limit while preserving both the report header and the newest diagnostic tail.
 * Export logs remains the lossless path.
 */
internal fun boundedLogClipboardText(
    value: String,
    maxChars: Int = MAX_LOG_CLIPBOARD_CHARS,
): String {
    require(maxChars > 0)
    if (value.length <= maxChars) return value

    val marker = "\n\n--- Clipboard copy shortened; use Export logs for the complete report. ---\n\n"
    if (marker.length >= maxChars) return value.take(maxChars)
    val contentBudget = maxChars - marker.length
    val headLength = contentBudget / 3
    val tailLength = contentBudget - headLength
    return value.take(headLength) + marker + value.takeLast(tailLength)
}
