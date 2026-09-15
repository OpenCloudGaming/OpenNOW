package com.opencloudgaming.opennow

import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.TimeZone

// Thread-local because exports and crash-history writes can run concurrently; supports API 23.
private val diagnosticTimeFormatter = object : ThreadLocal<SimpleDateFormat>() {
    override fun initialValue() = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss.SSS'Z'", Locale.US).apply {
        timeZone = TimeZone.getTimeZone("UTC")
    }
}

internal fun diagnosticTimestamp(epochMs: Long): String =
    diagnosticTimeFormatter.get()!!.format(Date(epochMs))

/** Convert monotonic event times only at export, keeping clock formatting off input threads. */
internal data class DiagnosticTimeAnchor(val epochMs: Long, val elapsedMs: Long) {
    fun formatElapsed(eventElapsedMs: Long): String =
        "${diagnosticTimestamp(epochMs + eventElapsedMs - elapsedMs)} uptimeMs=$eventElapsedMs"
}
