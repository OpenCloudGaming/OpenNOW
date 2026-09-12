package com.opencloudgaming.opennow

import android.os.SystemClock
import android.util.Log
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ThreadPoolExecutor
import java.util.concurrent.TimeUnit

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

internal data class InputDiagnosticsSnapshot(
    val retained: List<Pair<String, String>>,
    val recent: List<String>,
) {
    fun format(): String {
        if (retained.isEmpty() && recent.isEmpty()) return "input.diagnostics=empty"
        return buildString {
            if (retained.isNotEmpty()) {
                appendLine("input.state:")
                retained.forEach { (key, line) -> appendLine("$key $line") }
            }
            if (recent.isNotEmpty()) {
                appendLine("input.diagnostics:")
                recent.forEach { appendLine(it) }
            }
        }.trimEnd()
    }
}

internal class InputDiagnosticsBuffer(
    private val maxRecentLines: Int,
    private val maxRetainedLines: Int,
    private val elapsedRealtime: () -> Long,
) {
    private val recentLines = ArrayDeque<String>()
    private val retainedLines = linkedMapOf<String, String>()
    private val retainedUpdatedAtMs = mutableMapOf<String, Long>()
    private val retainedCounts = mutableMapOf<String, Long>()

    init {
        require(maxRecentLines > 0)
        require(maxRetainedLines > 0)
    }

    fun add(message: String): String {
        val line = formatLine(elapsedRealtime(), message)
        addRecentLine(line)
        return line
    }

    fun addRetained(key: String, message: String): String {
        val now = elapsedRealtime()
        val line = formatLine(now, message)
        addRecentLine(line)
        retainLine(key, now, line)
        return line
    }

    private fun addRecentLine(line: String) {
        if (recentLines.size >= maxRecentLines) {
            recentLines.removeFirst()
        }
        recentLines.addLast(line)
    }

    fun retain(key: String, message: String): String =
        retainAt(key, elapsedRealtime(), message)

    fun retainCounted(key: String, message: () -> String): String {
        return retainCountedAt(key, elapsedRealtime(), message())
    }

    fun retainResult(
        keyPrefix: String,
        succeeded: Boolean,
        message: () -> String,
    ) {
        val now = elapsedRealtime()
        val detail = message()
        retainCountedAt("$keyPrefix.last", now, "success=$succeeded $detail")
        retainCountedAt("$keyPrefix.${if (succeeded) "success" else "failure"}", now, detail)
    }

    fun retainThrottled(
        key: String,
        minimumIntervalMs: Long,
        message: () -> String,
    ): String? {
        require(minimumIntervalMs >= 0)
        val now = elapsedRealtime()
        val lastUpdate = retainedUpdatedAtMs[key]
        if (lastUpdate != null && now - lastUpdate in 0 until minimumIntervalMs) {
            return null
        }
        return retainAt(key, now, message())
    }

    fun capture(): InputDiagnosticsSnapshot =
        InputDiagnosticsSnapshot(retainedLines.toList(), recentLines.toList())

    fun snapshot(): String = capture().format()

    private fun retainAt(key: String, now: Long, message: String): String {
        val line = formatLine(now, message)
        retainLine(key, now, line)
        return line
    }

    private fun retainCountedAt(key: String, now: Long, message: String): String {
        val count = (retainedCounts[key] ?: 0L) + 1L
        retainedCounts[key] = count
        return retainAt(key, now, "count=$count $message")
    }

    private fun retainLine(key: String, now: Long, line: String) {
        if (key !in retainedLines && retainedLines.size >= maxRetainedLines) {
            retainedLines.keys.firstOrNull()?.let { oldestKey ->
                retainedLines.remove(oldestKey)
                retainedUpdatedAtMs.remove(oldestKey)
                retainedCounts.remove(oldestKey)
            }
        }
        retainedLines[key] = line
        retainedUpdatedAtMs[key] = now
    }

    private fun formatLine(now: Long, message: String): String = "$now $message"
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
        logWriter.offer(message)
    }

    fun addRetained(key: String, message: String) {
        synchronized(this) { buffer.addRetained(key, message) }
        logWriter.offer(message)
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

    fun snapshot(): String = synchronized(this) { buffer.capture() }.format()
}
