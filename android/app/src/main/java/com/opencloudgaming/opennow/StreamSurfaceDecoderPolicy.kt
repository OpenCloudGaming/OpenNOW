package com.opencloudgaming.opennow

import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat

/** Shader processing and ten-bit SDR keep the established texture/color path. */
internal fun shouldPreferDirectSdrSurface(
    settings: StreamSettings,
    playStoreRelease: Boolean = BuildConfig.PLAY_STORE_RELEASE,
): Boolean {
    val output = if (settings.videoOutput == StreamVideoOutput.Default) {
        defaultStreamVideoOutput(playStoreRelease)
    } else settings.videoOutput
    return output == StreamVideoOutput.MediaCodecSurface && !settings.hdrEnabled &&
        !settings.usesTenBitStreamProfile() && !settings.streamSharpeningEnabled
}

internal fun sdrSurfaceDecoderConfiguration(
    codec: VideoCodec,
    approvedDecoderName: String,
    width: Int,
    height: Int,
    fps: Int,
    tunePerformance: Boolean,
    lowLatency: Boolean,
    standardLowLatency: Boolean,
): SurfaceDecoderConfiguration? {
    if (width <= 0 || height <= 0 || fps <= 0) return null
    val mime = codec.mediaMimeType()
    val name = LowLatencyVideoDecoder.selectStreamCodecName(approvedDecoderName, lowLatency)
    val supported = runCatching {
        val info = MediaCodecList(MediaCodecList.REGULAR_CODECS).codecInfos.firstOrNull { it.name == name }
            ?: return@runCatching false
        !info.isEncoder && CodecProbe.isOpenNowHardwareDecoderAllowed(info) &&
            info.getCapabilitiesForType(mime).videoCapabilities?.areSizeAndRateSupported(width, height, fps.toDouble()) == true
    }.getOrDefault(false)
    if (!supported) return null
    val format = MediaFormat.createVideoFormat(mime, width, height).apply {
        // Stream FPS is not capped to the display rate. 120/240/360 retain their exact hint.
        setInteger(MediaFormat.KEY_FRAME_RATE, fps)
        if (tunePerformance) LowLatencyVideoDecoder.applyStreamFormat(this, name, fps, lowLatency, standardLowLatency)
    }
    return SurfaceDecoderConfiguration(name, format) { decoder ->
        LowLatencyVideoDecoder.applyStreamParameters({ decoder.setParameters(it) }, lowLatency, standardLowLatency)
    }
}

internal fun hdrSurfaceDecoder(fps: Int, surface: () -> DecoderSurfaceTarget?): MediaCodecSurfaceVideoDecoder =
    MediaCodecSurfaceVideoDecoder(fps, surface, hdr = true) { width, height, rate ->
        StreamHdr.decoderName(width, height, rate)?.let { name ->
            SurfaceDecoderConfiguration(name, StreamHdr.format(width, height, rate))
        }
    }
