package com.opencloudgaming.opennow

import android.app.ActivityManager
import android.content.Context
import android.os.Build
import kotlin.math.abs

internal data class AndroidDeviceRecommendation(
    val stream: StreamSettings,
    val displayWidth: Int,
    val displayHeight: Int,
    val processorCount: Int,
    val totalMemoryMiB: Long?,
    val androidTvProfile: Boolean,
    val lowPowerProfile: Boolean,
) {
    fun debugSummary(): String =
        "display=${displayWidth}x$displayHeight processors=$processorCount memoryMiB=${totalMemoryMiB ?: "unknown"} " +
            "tv=$androidTvProfile lowPower=$lowPowerProfile recommended=${stream.resolution}@${stream.fps} " +
            "codec=${stream.codec} bitrate=${stream.maxBitrateMbps}"
}

internal fun recommendedAndroidStreamProfile(
    context: Context,
    report: RuntimeCodecReport?,
): AndroidDeviceRecommendation {
    val metrics = context.resources.displayMetrics
    val displayWidth = maxOf(metrics.widthPixels, metrics.heightPixels).coerceAtLeast(1)
    val displayHeight = minOf(metrics.widthPixels, metrics.heightPixels).coerceAtLeast(1)
    val processorCount = Runtime.getRuntime().availableProcessors().coerceAtLeast(1)
    val totalMemoryMiB = runCatching {
        val info = ActivityManager.MemoryInfo()
        (context.getSystemService(Context.ACTIVITY_SERVICE) as? ActivityManager)?.getMemoryInfo(info)
        info.totalMem.takeIf { it > 0L }?.div(1024L * 1024L)
    }.getOrNull()
    val tvProfile = report?.androidTvProfile ?: isAndroidTvProfile(context)
    val shieldTv = isNvidiaShieldTvDevice(
        androidTvProfile = tvProfile,
        manufacturer = Build.MANUFACTURER,
        model = Build.MODEL,
    )
    return recommendedAndroidStreamProfile(
        displayWidth = displayWidth,
        displayHeight = displayHeight,
        processorCount = processorCount,
        totalMemoryMiB = totalMemoryMiB,
        androidTvProfile = tvProfile,
        nvidiaShieldTv = shieldTv,
        report = report,
    )
}

internal fun recommendedAndroidStreamProfile(
    displayWidth: Int,
    displayHeight: Int,
    processorCount: Int,
    totalMemoryMiB: Long?,
    androidTvProfile: Boolean,
    nvidiaShieldTv: Boolean = false,
    report: RuntimeCodecReport?,
): AndroidDeviceRecommendation {
    val safeDisplayWidth = displayWidth.coerceAtLeast(1)
    val safeDisplayHeight = displayHeight.coerceAtLeast(1)
    val safeProcessorCount = processorCount.coerceAtLeast(1)
    val noVerifiedHardwareDecoder = report != null && report.capabilities.none { capability ->
        capability.streamingDecoderUsableForLaunch() && capability.streamingHardwareDecoderAvailable()
    }
    val lowPower = report?.lowPowerGpuProfile == true ||
        totalMemoryMiB?.let { it < 3_000L } == true ||
        safeProcessorCount <= 4 ||
        noVerifiedHardwareDecoder

    val maxHeight = when {
        report?.constrainedRuntimeProfile == true -> 720
        lowPower -> 720
        nvidiaShieldTv -> 2160
        androidTvProfile -> 1080
        totalMemoryMiB?.let { it >= 6_000L } == true && safeProcessorCount >= 8 -> 1440
        else -> 1080
    }
    val deviceAspect = safeDisplayWidth.toDouble() / safeDisplayHeight.toDouble()
    val aspectRatio = streamAspectRatioOptions().minByOrNull { option ->
        val parts = option.split(':')
        val optionAspect = parts.getOrNull(0)?.toDoubleOrNull()
            ?.div(parts.getOrNull(1)?.toDoubleOrNull() ?: 1.0)
            ?: (16.0 / 9.0)
        abs(deviceAspect - optionAspect)
    } ?: "16:9"
    val choices = streamResolutionChoicesForAspect(aspectRatio)
    val displaySizedChoices = choices
        .filter { it.width <= safeDisplayWidth && it.height <= safeDisplayHeight && it.height <= maxHeight }
        .ifEmpty { choices.filter { it.height <= maxHeight } }
        .sortedByDescending { it.width * it.height }
    val fallbackChoice = streamResolutionChoicesForAspect("16:9").first()
    val selectedWithCodec = displaySizedChoices.firstNotNullOfOrNull { choice ->
        recommendedCodecForResolution(choice, lowPower, report)?.let { codec -> choice to codec }
    } ?: (displaySizedChoices.firstOrNull() ?: fallbackChoice) to VideoCodec.H264
    val (selected, selectedCodec) = selectedWithCodec

    val fps = if (
        report?.constrainedRuntimeProfile == true ||
        noVerifiedHardwareDecoder ||
        (lowPower && safeProcessorCount <= 4)
    ) {
        30
    } else {
        60
    }
    val bitrate = when {
        lowPower -> if (fps <= 30) 12 else 18
        selected.height >= 2160 -> 75
        selected.height >= 1440 -> 45
        androidTvProfile -> 30
        else -> 35
    }
    val stream = StreamSettings(
        resolution = selected.value,
        aspectRatio = selected.aspectRatio,
        fps = fps,
        maxBitrateMbps = bitrate,
        codec = selectedCodec,
        colorQuality = ColorQuality.EightBit420,
    ).adjustedForDevice(report)

    return AndroidDeviceRecommendation(
        stream = stream,
        displayWidth = safeDisplayWidth,
        displayHeight = safeDisplayHeight,
        processorCount = safeProcessorCount,
        totalMemoryMiB = totalMemoryMiB,
        androidTvProfile = androidTvProfile,
        lowPowerProfile = lowPower,
    )
}

private fun recommendedCodecForResolution(
    resolution: StreamResolutionChoice,
    lowPower: Boolean,
    report: RuntimeCodecReport?,
): VideoCodec? {
    if (report == null) return VideoCodec.H264
    val codecOrder = if (!lowPower && resolution.height >= 1440) {
        listOf(VideoCodec.H265, VideoCodec.H264)
    } else {
        listOf(VideoCodec.H264, VideoCodec.H265)
    }
    return codecOrder.firstOrNull { codec ->
        report.capabilities
            .firstOrNull { it.codec == codec }
            ?.supportsRecommendedResolution(resolution) == true
    }
}

private fun CodecCapability.supportsRecommendedResolution(resolution: StreamResolutionChoice): Boolean {
    if (!streamingDecoderUsableForLaunch() || !streamingHardwareDecoderAvailable()) return false
    val maxWidth = maxSupportedWidth
    val maxHeight = maxSupportedHeight
    return maxWidth == null || maxHeight == null ||
        (resolution.width <= maxWidth && resolution.height <= maxHeight)
}

internal fun StreamSettings.performanceOverridesComparedTo(
    recommended: StreamSettings?,
    report: RuntimeCodecReport?,
): List<String> {
    recommended ?: return emptyList()
    val selectedResolution = normalizeStreamResolutionForAspect(resolution, aspectRatio)
    val recommendedResolution = normalizeStreamResolutionForAspect(recommended.resolution, recommended.aspectRatio)
    val selectedPixels = parseResolutionPixelsOrNull(selectedResolution)
    val recommendedPixels = parseResolutionPixelsOrNull(recommendedResolution)
    val selectedPixelCount = selectedPixels?.let { (width, height) -> width.toLong() * height }
    val recommendedPixelCount = recommendedPixels?.let { (width, height) -> width.toLong() * height }
    val codecCapability = report?.capabilities?.firstOrNull { it.codec == codec }

    return buildList {
        if (
            selectedPixelCount != null && recommendedPixelCount != null &&
            selectedPixelCount > recommendedPixelCount
        ) {
            add("$selectedResolution resolution (recommended $recommendedResolution)")
        }
        if (fps > recommended.fps) add("$fps FPS (recommended ${recommended.fps})")
        if (maxBitrateMbps > recommended.maxBitrateMbps) {
            add("$maxBitrateMbps Mbps bitrate (recommended ${recommended.maxBitrateMbps})")
        }
        if (hdrEnabled && !recommended.hdrEnabled) add("HDR")
        // HDR implies a ten-bit transport, but it already has its own user-facing override label.
        // Only call out 10-bit color when the separate SDR color-quality choice is responsible.
        if (!hdrEnabled && colorQuality.isTenBit() &&
            (recommended.hdrEnabled || !recommended.colorQuality.isTenBit())) add("10-bit color")
        if (streamSharpeningEnabled && !recommended.streamSharpeningEnabled) add("stream sharpening")
        if (
            codec != recommended.codec &&
            codecCapability != null &&
            (!codecCapability.streamingDecoderUsableForLaunch() || !codecCapability.streamingHardwareDecoderAvailable())
        ) {
            add("${codec.name} without a verified real-time hardware decoder")
        }
    }.distinct()
}

internal fun StreamSettings.recommendationSummary(): String =
    "$resolution@$fps ${codec.name}, $maxBitrateMbps Mbps"
