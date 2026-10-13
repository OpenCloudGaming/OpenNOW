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

    fun decoderName(width: Int, height: Int, fps: Int, codec: VideoCodec = VideoCodec.H265): String? {
        if (Build.VERSION.SDK_INT < 26 || (codec == VideoCodec.AV1 && Build.VERSION.SDK_INT < 29)) return null
        hdrCodecProfile(codec) ?: return null
        val mime = codec.mediaMimeType()
        return runCatching {
            MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.firstOrNull { info ->
                !info.isEncoder && CodecProbe.isOpenNowHardwareDecoderAllowed(info) &&
                    runCatching {
                        val caps = info.getCapabilitiesForType(mime)
                        caps.profileLevels.any { hdrDecoderProfileSupported(codec, it.profile) } &&
                            caps.isFormatSupported(format(width, height, fps, codec)) &&
                            caps.videoCapabilities?.areSizeAndRateSupported(width, height, fps.toDouble()) == true
                    }.getOrDefault(false)
            }?.name
        }.getOrNull()
    }

    fun format(width: Int, height: Int, fps: Int, codec: VideoCodec = VideoCodec.H265): MediaFormat =
        MediaFormat.createVideoFormat(codec.mediaMimeType(), width, height).apply {
            setInteger(MediaFormat.KEY_PROFILE, requireNotNull(hdrCodecProfile(codec)))
            setInteger(MediaFormat.KEY_COLOR_STANDARD, MediaFormat.COLOR_STANDARD_BT2020)
            setInteger(MediaFormat.KEY_COLOR_TRANSFER, MediaFormat.COLOR_TRANSFER_ST2084)
            setInteger(MediaFormat.KEY_COLOR_RANGE, MediaFormat.COLOR_RANGE_LIMITED)
            setInteger(MediaFormat.KEY_FRAME_RATE, fps)
            // Match the SDR decoder's real-time scheduling hints. In particular, a
            // 120-FPS HDR decoder must not retain the platform's background priority.
            setInteger(MediaFormat.KEY_PRIORITY, 0)
            setInteger(MediaFormat.KEY_OPERATING_RATE, fps)
            // No SDR white-point multiplier, tone-map request, or invented mastering metadata.
            // The bitstream color metadata and opaque decoder surface carry HDR to Android.
        }
}

internal fun hdrCodecProfile(codec: VideoCodec): Int? = when (codec) {
    VideoCodec.H265 -> MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10
    VideoCodec.AV1 -> MediaCodecInfo.CodecProfileLevel.AV1ProfileMain10
    VideoCodec.H264 -> null
}

internal fun hdrDecoderProfileSupported(codec: VideoCodec, profile: Int): Boolean = when (codec) {
    VideoCodec.H265 -> profile in hdrHevcProfiles
    VideoCodec.AV1 -> profile in hdrAv1Profiles
    VideoCodec.H264 -> false
}

private val hdrAv1Profiles = setOf(
    MediaCodecInfo.CodecProfileLevel.AV1ProfileMain10,
    MediaCodecInfo.CodecProfileLevel.AV1ProfileMain10HDR10,
    MediaCodecInfo.CodecProfileLevel.AV1ProfileMain10HDR10Plus,
)

private val hdrHevcProfiles = setOf(
    MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10,
    MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10HDR10,
    MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10HDR10Plus,
)

internal fun StreamSettings.withHdrDeviceSupport(context: Context): StreamSettings {
    if (!hdrEnabled) return this
    if (!ANDROID_HDR_STREAMING_ENABLED) {
        return copy(hdrEnabled = false, hdrDisplay = null).withCodecColorCompatibility()
    }
    val display = StreamHdr.displayProfile(context)
    val (width, height) = streamResolutionPixels(this)
    val supported = hdrAvailableForAndroid(isAndroidTvProfile(context)) && display != null &&
        StreamHdr.decoderName(width, height, fps, codec) != null
    return copy(hdrEnabled = supported, hdrDisplay = if (supported) display else null)
}

/** Missing keys retain the configured PQ/BT.2020 values; explicit SDR output is rejected. */
internal fun hdrOutputColorSupported(standard: Int?, transfer: Int?): Boolean =
    (standard == null || standard == MediaFormat.COLOR_STANDARD_BT2020) &&
        (transfer == null || transfer == MediaFormat.COLOR_TRANSFER_ST2084)
