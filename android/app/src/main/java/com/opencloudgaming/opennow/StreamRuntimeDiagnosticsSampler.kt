package com.opencloudgaming.opennow

import android.os.SystemClock
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * UI-thread owner for periodic stream diagnostics. Android service queries run on a worker,
 * with at most one outstanding sample even when a slow Binder call outlives a session reset.
 */
internal class StreamRuntimeDiagnosticsSampler(
    private val scope: CoroutineScope,
    private val readSnapshot: (includeDevice: Boolean) -> AndroidRuntimeDiagnosticsSnapshot,
    private val elapsedRealtimeMs: () -> Long = SystemClock::elapsedRealtime,
    private val workerDispatcher: CoroutineDispatcher = Dispatchers.IO,
) {
    private var samplingJob: Job? = null
    private var lastNetworkSampleAtMs: Long? = null
    private var lastDeviceSampleAtMs: Long? = null

    fun sampleIfDue(onSample: (AndroidRuntimeDiagnosticsSnapshot, includeDevice: Boolean) -> Unit): Job? {
        if (samplingJob?.isCompleted == false) return null
        val now = elapsedRealtimeMs()
        if (lastNetworkSampleAtMs?.let { now - it < NETWORK_SAMPLE_INTERVAL_MS } == true) return null
        val includeDevice = lastDeviceSampleAtMs?.let { now - it >= DEVICE_SAMPLE_INTERVAL_MS } ?: true
        lastNetworkSampleAtMs = now
        if (includeDevice) lastDeviceSampleAtMs = now
        return scope.launch {
            val snapshot = withContext(workerDispatcher) {
                try {
                    readSnapshot(includeDevice)
                } catch (error: CancellationException) {
                    throw error
                } catch (_: Exception) {
                    // Optional diagnostics must not interrupt streaming if a system service fails.
                    null
                }
            } ?: return@launch
            onSample(snapshot, includeDevice)
        }.also { samplingJob = it }
    }

    fun reset() {
        samplingJob?.cancel()
        // Keep the job until completion so a blocked service cannot accumulate worker requests.
        lastNetworkSampleAtMs = null
        lastDeviceSampleAtMs = null
    }
}

private const val NETWORK_SAMPLE_INTERVAL_MS = 5_000L
private const val DEVICE_SAMPLE_INTERVAL_MS = 30_000L
