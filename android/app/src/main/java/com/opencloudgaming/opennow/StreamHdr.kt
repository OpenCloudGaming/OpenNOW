package com.opencloudgaming.opennow

import android.content.Context
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Build
import android.view.Display
import android.view.WindowManager

/** Runtime display data, never persisted as a property of the user's stream preset. */
data class HdrDisplayProfile(val maxLuminance: Float, val minLuminance: Float, val maxAverageLuminance: Float)

internal fun hdrDisplayProfile(max: Float, min: Float, average: Float): HdrDisplayProfile? {
    // Unknown luminance is reported as -1. Do not advertise a made-up 1000-nit panel.
    if (!max.isFinite() || max <= 0f || !min.isFinite() || min < 0f || min >= max ||
        !average.isFinite() || average <= 0f || average > max) return null
    return HdrDisplayProfile(max, min, average)
}

internal object StreamHdr {
    @Suppress("DEPRECATION")
    fun displayProfile(context: Context): HdrDisplayProfile? {
        if (Build.VERSION.SDK_INT < 26) return null
        val display = (if (Build.VERSION.SDK_INT >= 30) runCatching { context.display }.getOrNull() else null)
            ?: (context.getSystemService(Context.WINDOW_SERVICE) as? WindowManager)?.defaultDisplay
        val capabilities = display?.hdrCapabilities ?: return null
        if (Display.HdrCapabilities.HDR_TYPE_HDR10 !in capabilities.supportedHdrTypes) return null
        return hdrDisplayProfile(capabilities.desiredMaxLuminance, capabilities.desiredMinLuminance,
            capabilities.desiredMaxAverageLuminance)
    }

    fun decoderName(width: Int, height: Int, fps: Int): String? {
        if (Build.VERSION.SDK_INT < 26) return null
        return runCatching {
            MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.firstOrNull { info ->
                !info.isEncoder && CodecProbe.isOpenNowHardwareDecoderAllowed(info) &&
                    runCatching {
                        val caps = info.getCapabilitiesForType("video/hevc")
                        caps.profileLevels.any { it.profile in setOf(
                            MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10,
                            MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10HDR10,
                            MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10HDR10Plus,
                        ) } && (caps.videoCapabilities?.let { it.areSizeAndRateSupported(width, height, fps.toDouble()) || it.isSizeSupported(width, height) } ?: true)
                    }.getOrDefault(false)
            }?.name
        }.getOrNull()
    }

    fun format(width: Int, height: Int, fps: Int): MediaFormat =
        MediaFormat.createVideoFormat("video/hevc", width, height).apply {
            setInteger(MediaFormat.KEY_PROFILE, MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10)
            setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT2020)
            setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_ST2084)
            setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_LIMITED)
            setInteger(MediaFormat.KEY_FRAME_RATE, fps)
            // No SDR white-point multiplier, tone-map request, or invented mastering metadata.
            // The HEVC VUI/SEI and opaque decoder surface carry the source metadata to Android.
        }
}

internal fun StreamSettings.withHdrDeviceSupport(context: Context): StreamSettings {
    if (!hdrEnabled) return this
    val display = StreamHdr.displayProfile(context)
    val (width, height) = streamResolutionPixels(this)
    val supported = hdrAvailableForAndroid(isAndroidTvProfile(context)) && display != null &&
        StreamHdr.decoderName(width, height, fps) != null
    return copy(hdrEnabled = supported, hdrDisplay = if (supported) display else null)
}

/** Missing keys retain the configured PQ/BT.2020 values; explicit SDR output is rejected. */
internal fun hdrOutputColorSupported(standard: Int?, transfer: Int?): Boolean =
    (standard == null || standard == MediaFormat.COLOR_STANDARD_BT2020) &&
        (transfer == null || transfer == MediaFormat.COLOR_TRANSFER_ST2084)
