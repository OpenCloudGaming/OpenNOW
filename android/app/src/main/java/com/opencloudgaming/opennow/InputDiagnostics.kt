package com.opencloudgaming.opennow

import android.os.SystemClock
import android.util.Log
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit
import kotlinx.serialization.json.*

/** Best-effort logcat mirroring must never block input or grow an unbounded backlog. */
internal class InputDiagnosticsLogWriter(
    maxPendingLines: Int = 64,
    private val writeLine: (String) -> Unit,
) : AutoCloseable {
    private val executor = ThreadPoolExecutor(
        0, 1, 30L, TimeUnit.SECONDS, ArrayBlockingQueue<Runnable>(maxPendingLines),
        { task -> Thread(task, "OpenNOWInputLog").apply { isDaemon = true } },
        ThreadPoolExecutor.DiscardPolicy(),
    )

    fun offer(message: String) {
        executor.execute {
            try {
                writeLine(message)
            } catch (_: Exception) {
                // The in-memory diagnostic entry is already retained if logcat is unavailable.
            }
        }
    }

    override fun close() { executor.shutdownNow() }
}

internal data class InputDiagnosticEntry(
    val elapsedMs: Long,
    val message: String,
    val count: Long? = null,
    val lastSeenMs: Long = elapsedMs,
) {
    fun format(timestamp: (Long) -> String): String = buildString {
        append(timestamp(elapsedMs))
        count?.let { append(" count=$it") }
        if (lastSeenMs != elapsedMs) append(" lastSeenUptimeMs=$lastSeenMs")
        append(' ')
        append(message)
    }
}

internal data class InputDiagnosticsSnapshot(
    val retained: List<Pair<String, InputDiagnosticEntry>>,
    val recent: List<InputDiagnosticEntry>,
) {
    fun toJson(anchor: DiagnosticTimeAnchor): JsonObject {
        fun InputDiagnosticEntry.json(): JsonObject = buildJsonObject {
            put("timestamp", diagnosticTimestamp(anchor.epochMs + elapsedMs - anchor.elapsedMs))
            put("uptimeMs", elapsedMs)
            put("lastSeenUptimeMs", lastSeenMs)
            count?.let { put("count", it) }
            put("message", message)
        }
        return buildJsonObject {
            put("state", JsonObject(retained.associate { (key, entry) -> key to entry.json() }))
            put("events", JsonArray(recent.map { it.json() }))
        }
    }

    fun format(timestamp: (Long) -> String = { it.toString() }): String {
        if (retained.isEmpty() && recent.isEmpty()) return "input.diagnostics=empty"
        // Retained state already contains these exact events; keep earlier transitions only once.
        val retainedEvents = retained.map { it.second.elapsedMs to it.second.message }.toSet()
        val history = recent.filterNot { (it.elapsedMs to it.message) in retainedEvents }
        return buildString {
            if (retained.isNotEmpty()) {
                appendLine("input.state:")
                retained.forEach { (key, entry) -> appendLine("$key ${entry.format(timestamp)}") }
            }
            if (history.isNotEmpty()) {
                appendLine("input.events:")
                history.forEach { appendLine(it.format(timestamp)) }
            }
        }.trimEnd()
    }
}

internal class InputDiagnosticsBuffer(
    private val maxRecentLines: Int,
    private val maxRetainedLines: Int,
    private val elapsedRealtime: () -> Long,
) {
    private val recentLines = ArrayDeque<InputDiagnosticEntry>()
    private val retainedLines = linkedMapOf<String, InputDiagnosticEntry>()
    private val retainedCounts = mutableMapOf<String, Long>()
    private val lastSeenAtMs = mutableMapOf<String, Long>()
    private val lastResults = mutableMapOf<String, Boolean>()

    init {
        require(maxRecentLines > 0)
        require(maxRetainedLines > 0)
    }

    fun add(message: String) {
        addRecentLine(InputDiagnosticEntry(elapsedRealtime(), message))
    }

    fun addRetained(key: String, message: String) {
        val entry = InputDiagnosticEntry(elapsedRealtime(), message)
        addRecentLine(entry)
        retainLine(key, entry)
    }

    private fun addRecentLine(entry: InputDiagnosticEntry) {
        if (recentLines.size >= maxRecentLines) recentLines.removeFirst()
        recentLines.addLast(entry)
    }

    fun retain(key: String, message: String) {
        retainLine(key, InputDiagnosticEntry(elapsedRealtime(), message))
    }

    fun retainCounted(key: String, message: () -> String) {
        val now = elapsedRealtime()
        if (detailDue(key, now)) retainLine(key, InputDiagnosticEntry(now, message()))
        countEvent(key, now)
    }

    fun retainResult(keyPrefix: String, succeeded: Boolean, message: () -> String) {
        val now = elapsedRealtime()
        val lastKey = "$keyPrefix.last"
        val outcomeKey = "$keyPrefix.${if (succeeded) "success" else "failure"}"
        val changed = lastResults[keyPrefix] != succeeded
        if (changed || detailDue(lastKey, now) || detailDue(outcomeKey, now)) {
            val detail = message()
            retainLine(lastKey, InputDiagnosticEntry(now, "success=$succeeded $detail"))
            retainLine(outcomeKey, InputDiagnosticEntry(now, detail))
        }
        if (lastKey in retainedLines) lastResults[keyPrefix] = succeeded
        countEvent(lastKey, now)
        countEvent(outcomeKey, now)
    }

    fun retainThrottled(key: String, minimumIntervalMs: Long, message: () -> String) {
        require(minimumIntervalMs >= 0)
        val now = elapsedRealtime()
        if (detailDue(key, now, minimumIntervalMs)) {
            retainLine(key, InputDiagnosticEntry(now, message()))
        }
    }

    fun capture(): InputDiagnosticsSnapshot = InputDiagnosticsSnapshot(
        retainedLines.map { (key, entry) ->
            key to entry.copy(count = retainedCounts[key], lastSeenMs = lastSeenAtMs[key] ?: entry.elapsedMs)
        },
        recentLines.toList(),
    )

    fun snapshot(): String = capture().format()

    private fun detailDue(key: String, now: Long, intervalMs: Long = 1_000L): Boolean {
        val previous = retainedLines[key]?.elapsedMs ?: return true
        return now - previous !in 0 until intervalMs
    }

    private fun countEvent(key: String, now: Long) {
        // A very small buffer may evict one of the outcome entries while retaining another.
        if (key !in retainedLines) return
        retainedCounts[key] = (retainedCounts[key] ?: 0L) + 1L
        lastSeenAtMs[key] = now
    }

    private fun retainLine(key: String, entry: InputDiagnosticEntry) {
        if (key !in retainedLines && retainedLines.size >= maxRetainedLines) {
            retainedLines.keys.firstOrNull()?.let { oldestKey ->
                retainedLines.remove(oldestKey)
                retainedCounts.remove(oldestKey)
                lastSeenAtMs.remove(oldestKey)
                if (oldestKey.endsWith(".last")) lastResults.remove(oldestKey.removeSuffix(".last"))
            }
        }
        retainedLines[key] = entry
    }
}

object NativeInputDiagnostics {
    private const val MAX_RECENT_LINES = 240
    private const val MAX_RETAINED_LINES = 48
    private const val TAG = "OpenNOWInput"
    private val logWriter = InputDiagnosticsLogWriter { Log.d(TAG, it) }
    private val buffer = InputDiagnosticsBuffer(
        maxRecentLines = MAX_RECENT_LINES,
        maxRetainedLines = MAX_RETAINED_LINES,
        elapsedRealtime = SystemClock::elapsedRealtime,
    )

    fun add(message: String) {
        synchronized(this) { buffer.add(message) }
        if (BuildConfig.DEBUG) logWriter.offer(message)
    }

    fun addRetained(key: String, message: String) {
        synchronized(this) { buffer.addRetained(key, message) }
        if (BuildConfig.DEBUG) logWriter.offer(message)
    }

    @Synchronized
    fun retain(key: String, message: String) {
        buffer.retain(key, message)
    }

    @Synchronized
    fun retainCounted(key: String, message: () -> String) {
        buffer.retainCounted(key, message)
    }

    @Synchronized
    fun retainThrottled(key: String, minimumIntervalMs: Long, message: () -> String) {
        buffer.retainThrottled(key, minimumIntervalMs, message)
    }

    @Synchronized
    fun retainResult(keyPrefix: String, succeeded: Boolean, message: () -> String) {
        buffer.retainResult(keyPrefix, succeeded, message)
    }

    @Synchronized
    fun retainTouchRoute(key: String, message: () -> String) {
        buffer.retainCounted("touch-route.$key", message)
    }

    internal fun capture(): InputDiagnosticsSnapshot = synchronized(this) { buffer.capture() }

    fun snapshot(): String {
        val anchor = DiagnosticTimeAnchor(System.currentTimeMillis(), SystemClock.elapsedRealtime())
        return capture().format(anchor::formatElapsed)
    }
}
