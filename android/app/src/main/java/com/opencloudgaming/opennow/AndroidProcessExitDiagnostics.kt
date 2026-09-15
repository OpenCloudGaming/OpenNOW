package com.opencloudgaming.opennow

import android.app.ActivityManager
import android.content.Context
import android.os.Build
import kotlinx.serialization.Serializable

@Serializable
internal data class DiagnosticProcessExit(
    val timestamp: String,
    val timestampEpochMs: Long,
    val reason: Int,
    val reasonCode: String,
    val status: Int,
    val description: String?,
    val pssKiB: Long?,
    val rssKiB: Long?,
)

@Serializable
internal data class DiagnosticProcessExitHistory(
    val availability: String,
    val scope: String = "previous_main_processes",
    val records: List<DiagnosticProcessExit> = emptyList(),
)

/** Previous main-process exits cannot change while this process is alive. Read once on export. */
internal object AndroidProcessExitDiagnostics {
    private var cached: DiagnosticProcessExitHistory? = null

    @Synchronized
    fun snapshot(context: Context): DiagnosticProcessExitHistory {
        cached?.let { return it }
        val history = if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
            DiagnosticProcessExitHistory("unsupported_android_version")
        } else {
            runCatching {
                val manager = context.getSystemService(Context.ACTIVITY_SERVICE) as? ActivityManager
                if (manager == null) DiagnosticProcessExitHistory("service_unavailable") else {
                    val records = manager.getHistoricalProcessExitReasons(context.packageName, 0, 8)
                        .filter { it.processName == context.packageName }.take(3).map { exit ->
                            DiagnosticProcessExit(
                                timestamp = diagnosticTimestamp(exit.timestamp), timestampEpochMs = exit.timestamp,
                                reason = exit.reason, reasonCode = diagnosticProcessExitReason(exit.reason),
                                status = exit.status, description = exit.description?.take(1000),
                                pssKiB = exit.pss.takeIf { it > 0 }, rssKiB = exit.rss.takeIf { it > 0 },
                            )
                        }
                    DiagnosticProcessExitHistory("available", records = records)
                }
            }.getOrElse { DiagnosticProcessExitHistory("query_failed") }
        }
        cached = history
        return history
    }
}

// Android ApplicationExitInfo's public reason values; retain the integer for future reasons.
internal fun diagnosticProcessExitReason(reason: Int): String = when (reason) {
    1 -> "exit_self"
    2 -> "signaled"
    3 -> "low_memory"
    4 -> "crash_java"
    5 -> "crash_native"
    6 -> "anr"
    7 -> "initialization_failure"
    8 -> "permission_change"
    9 -> "excessive_resource_usage"
    10 -> "user_requested"
    11 -> "user_stopped"
    12 -> "dependency_died"
    13 -> "other"
    14 -> "freezer"
    15 -> "package_state_change"
    16 -> "package_updated"
    else -> "unknown"
}
