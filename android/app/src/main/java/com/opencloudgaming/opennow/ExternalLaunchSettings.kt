package com.opencloudgaming.opennow

import android.content.Intent

/**
 * One-session stream settings a launcher may pass with an external launch, e.g.
 * `opennow://launch?id=<game>&resolution=1920x1080&fps=120&bitrate=50&codec=av1&hdr=false`
 * (or the same names as intent extras). Only these video settings are accepted, each is validated,
 * and anything missing keeps the user's saved setting. The result is still adjusted for the device
 * and membership like any launch, and the saved settings are never changed.
 */
internal object ExternalLaunchSettings {
    const val RESOLUTION = "resolution"
    const val FPS = "fps"
    const val BITRATE_MBPS = "bitrate"
    const val CODEC = "codec"
    const val HDR = "hdr"

    fun fromIntent(intent: Intent, base: StreamSettings): StreamSettings? {
        val uri = intent.data
        fun read(name: String): String? = intent.getStringExtra(name)
            ?: intent.extras?.get(name)?.toString()
            ?: uri?.takeIf { it.isHierarchical }?.let { runCatching { it.getQueryParameter(name) }.getOrNull() }
        return apply(base, read(RESOLUTION), read(FPS), read(BITRATE_MBPS), read(CODEC), read(HDR))
    }

    /** Returns null when nothing valid was given, so the launch keeps the normal path. */
    fun apply(
        base: StreamSettings,
        resolution: String?,
        fps: String?,
        bitrateMbps: String?,
        codec: String?,
        hdr: String?,
    ): StreamSettings? {
        var settings = base
        var changed = false
        resolution?.trim()?.lowercase()?.let { value ->
            val pixels = parseResolutionPixelsOrNull(value)
            if (pixels != null && pixels.first in 640..7680 && pixels.second in 360..4320) {
                settings = settings.copy(resolution = "${pixels.first}x${pixels.second}"); changed = true
            }
        }
        fps?.trim()?.toIntOrNull()?.takeIf { it in 30..240 }?.let { settings = settings.copy(fps = it); changed = true }
        bitrateMbps?.trim()?.toIntOrNull()?.takeIf { it in 5..250 }?.let {
            settings = settings.copy(maxBitrateMbps = it); changed = true
        }
        when (codec?.trim()?.lowercase()) {
            "h264", "avc" -> VideoCodec.H264
            "h265", "hevc" -> VideoCodec.H265
            "av1" -> VideoCodec.AV1
            else -> null
        }?.let { settings = settings.copy(codec = it); changed = true }
        when (hdr?.trim()?.lowercase()) {
            "true", "1", "on" -> true
            "false", "0", "off" -> false
            else -> null
        }?.let { settings = settings.copy(hdrEnabled = it); changed = true }
        return settings.takeIf { changed }
    }
}
