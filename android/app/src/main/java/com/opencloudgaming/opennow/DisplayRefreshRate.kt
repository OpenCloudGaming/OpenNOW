package com.opencloudgaming.opennow

import kotlin.math.abs
import kotlin.math.roundToInt

internal data class DisplayRefreshMode(
    val id: Int,
    val refreshRate: Float,
    val physicalWidth: Int,
    val physicalHeight: Int,
)

internal object DisplayRefreshDiagnostics {
    @Volatile
    private var latestSnapshot = "display.refresh=unavailable"

    fun update(
        active: Boolean,
        requestedFps: Int,
        currentMode: DisplayRefreshMode?,
        selectedMode: DisplayRefreshMode?,
        supportedModes: List<DisplayRefreshMode>,
        preferredModeId: Int,
        preferredRefreshRate: Float,
        applied: Boolean,
        error: Throwable? = null,
    ) {
        latestSnapshot = buildString {
            appendLine("display.refresh.active=$active requestedFps=$requestedFps applied=$applied")
            appendLine("display.refresh.current=${currentMode.debugLabel()} selected=${selectedMode.debugLabel()}")
            appendLine("display.refresh.preferredModeId=$preferredModeId preferredRefreshRate=${preferredRefreshRate.formatRefreshRate()}")
            appendLine("display.refresh.supported=${supportedModes.supportedModesLabel()}")
            error?.let {
                appendLine("display.refresh.error=${it.javaClass.simpleName}:${it.message.orEmpty().take(120)}")
            }
        }.trimEnd()
    }

    fun snapshot(): String = latestSnapshot
}

internal fun selectStreamDisplayMode(
    supportedModes: List<DisplayRefreshMode>,
    currentMode: DisplayRefreshMode?,
    requestedFps: Int,
): DisplayRefreshMode? {
    if (supportedModes.isEmpty()) return null
    val target = requestedFps.coerceIn(MIN_STREAM_DISPLAY_FPS, MAX_STREAM_DISPLAY_FPS).toFloat()
    val resolutionMatched = currentMode?.let { current ->
        supportedModes.filter { mode ->
            mode.physicalWidth == current.physicalWidth && mode.physicalHeight == current.physicalHeight
        }
    }.orEmpty()
    val candidates = resolutionMatched.ifEmpty { supportedModes }
    val cadenceMatched = candidates.filter { mode ->
        mode.refreshRate + STREAM_REFRESH_TOLERANCE_FPS >= target &&
            streamCadenceError(mode.refreshRate, target) <= STREAM_CADENCE_TOLERANCE
    }
    // Prefer one display refresh per stream frame when available. Retaining 120 Hz for a
    // 60 FPS texture stream lets small arrival-time variations alternate between 8/25 ms
    // presentations instead of a steady 16.7 ms cadence, and keeps the display running faster.
    return cadenceMatched.minByOrNull { it.refreshRate }
        ?: candidates
            .filter { it.refreshRate + STREAM_REFRESH_TOLERANCE_FPS >= target }
            .minByOrNull { it.refreshRate }
        ?: candidates.maxByOrNull { it.refreshRate }
}

private fun streamCadenceError(refreshRate: Float, streamFps: Float): Float {
    if (refreshRate <= 0f || streamFps <= 0f) return Float.MAX_VALUE
    val ratio = refreshRate / streamFps
    return abs(ratio - ratio.roundToInt().coerceAtLeast(1))
}

internal fun normalizedStreamDisplayFps(requestedFps: Int): Float =
    requestedFps.coerceIn(MIN_STREAM_DISPLAY_FPS, MAX_STREAM_DISPLAY_FPS).toFloat()

private fun List<DisplayRefreshMode>.supportedModesLabel(): String =
    if (isEmpty()) {
        "[]"
    } else {
        sortedWith(compareBy<DisplayRefreshMode> { it.physicalWidth * it.physicalHeight }.thenBy { it.refreshRate }.thenBy { it.id })
            .joinToString(prefix = "[", postfix = "]") { it.debugLabel() }
    }

private fun DisplayRefreshMode?.debugLabel(): String =
    this?.let { "id=${it.id}:${it.physicalWidth}x${it.physicalHeight}@${it.refreshRate.formatRefreshRate()}Hz" } ?: "none"

private fun Float.formatRefreshRate(): String {
    val rounded = (this * 100f).roundToInt() / 100f
    return if (rounded % 1f == 0f) rounded.toInt().toString() else rounded.toString()
}

private const val MIN_STREAM_DISPLAY_FPS = 30
private const val MAX_STREAM_DISPLAY_FPS = 360
private const val STREAM_REFRESH_TOLERANCE_FPS = 0.5f
private const val STREAM_CADENCE_TOLERANCE = 0.01f
