package com.opencloudgaming.opennow

internal data class DisplayRefreshMode(
    val id: Int,
    val refreshRate: Float,
    val physicalWidth: Int,
    val physicalHeight: Int,
)

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

    return candidates
        .filter { it.refreshRate + STREAM_REFRESH_TOLERANCE_FPS >= target }
        .minByOrNull { it.refreshRate }
        ?: candidates.maxByOrNull { it.refreshRate }
}

internal fun normalizedStreamDisplayFps(requestedFps: Int): Float =
    requestedFps.coerceIn(MIN_STREAM_DISPLAY_FPS, MAX_STREAM_DISPLAY_FPS).toFloat()

private const val MIN_STREAM_DISPLAY_FPS = 30
private const val MAX_STREAM_DISPLAY_FPS = 240
private const val STREAM_REFRESH_TOLERANCE_FPS = 0.5f
