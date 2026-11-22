package com.opencloudgaming.opennow

import android.media.MediaFormat
import android.media.MediaCodec
import android.os.Build
import android.util.Log
import org.webrtc.EncodedImage
import org.webrtc.OpenNowMediaCodecTuning
import org.webrtc.VideoCodecStatus
import org.webrtc.VideoDecoder
import org.webrtc.VideoFrame
import java.lang.reflect.Field
import java.util.Locale

private const val ALLWINNER_HEVC_DECODER = "OMX.allwinner.video.decoder.hevc"
private const val HD_WIDTH = 1280
private const val HD_HEIGHT = 720
private const val ALLWINNER_PADDED_HD_HEIGHT = 736

internal fun allwinnerHevcNeeds720pCrop(
    width: Int?, height: Int?, sliceHeight: Int?,
    cropLeft: Int?, cropTop: Int?, cropRight: Int?, cropBottom: Int?,
): Boolean = width == HD_WIDTH && height == ALLWINNER_PADDED_HD_HEIGHT && sliceHeight == height &&
    cropLeft == 0 && cropTop == 0 && cropRight == HD_WIDTH - 1 && cropBottom == height - 1

class LowLatencyVideoDecoder(
    private val delegate: VideoDecoder,
    private val requestedFps: Int,
    private val lowLatencyEnabled: Boolean,
    private val standardLowLatencyEnabled: Boolean = false,
    private val tuningEnabled: Boolean = true,
) : VideoDecoder {

    private var patched = false
    private var wrapperInstalled = false
    private var initializationCleanupFailed = false
    private val bufferProgress = CodecBufferProgress()
    private val allwinnerHevc = delegate.implementationName.equals(ALLWINNER_HEVC_DECODER, ignoreCase = true)
    @Volatile private var allwinnerConfiguredFor720p = false
    @Volatile private var allwinnerOutputIsPadded = false

    internal val canBorrowEncodedInput: Boolean
        get() = delegate.supportsBorrowedNvstInput()

    override fun initDecode(settings: VideoDecoder.Settings?, decodeCallback: VideoDecoder.Callback?): VideoCodecStatus {
        NativeInputDiagnostics.add(
            "MediaCodecVideoDecoder initDecode delegate=${delegate.javaClass.name} " +
                "requestedFps=$requestedFps lowLatency=$lowLatencyEnabled " +
                "standardLowLatency=$standardLowLatencyEnabled tuning=$tuningEnabled",
        )
        patchMediaCodecWrapperFactory()
        val callback = if (allwinnerHevc && decodeCallback != null) {
            VideoDecoder.Callback { frame, decodeTimeMs, qp ->
                val buffer = frame.buffer
                if (allwinnerOutputIsPadded && buffer is VideoFrame.TextureBuffer &&
                    buffer.width == HD_WIDTH && buffer.height == ALLWINNER_PADDED_HD_HEIGHT
                ) {
                    val visible = VideoFrame(
                        buffer.cropAndScale(0, 0, HD_WIDTH, HD_HEIGHT, HD_WIDTH, HD_HEIGHT),
                        frame.rotation,
                        frame.timestampNs,
                    )
                    try {
                        decodeCallback.onDecodedFrame(visible, decodeTimeMs, qp)
                    } finally {
                        visible.release()
                    }
                } else {
                    decodeCallback.onDecodedFrame(frame, decodeTimeMs, qp)
                }
            }
        } else decodeCallback
        return delegate.initDecode(settings, callback)
    }

    override fun release(): VideoCodecStatus {
        val result = delegate.release()
        if (wrapperInstalled) retainBufferProgress(force = true)
        return if (initializationCleanupFailed) VideoCodecStatus.ERROR else result
    }

    override fun decode(frame: EncodedImage?, info: VideoDecoder.DecodeInfo?): VideoCodecStatus {
        return delegate.decode(frame, info)
    }

    override fun getImplementationName(): String {
        val suffix = when {
            !tuningEnabled -> "untuned"
            lowLatencyEnabled -> "low-latency"
            standardLowLatencyEnabled -> "platform-low-latency"
            else -> "performance"
        }
        return delegate.implementationName + "+opennow-$suffix"
    }

    private fun patchMediaCodecWrapperFactory() {
        if (patched) {
            return
        }
        patched = true

        try {
            val factoryField = findMediaCodecWrapperFactoryField(delegate.javaClass)
            if (factoryField == null) {
                val msg = "MediaCodecWrapperFactory field not found on ${delegate.javaClass.name}"
                Log.w(TAG, msg)
                NativeInputDiagnostics.add("LowLatencyVideoDecoder: $msg")
                return
            }

            factoryField.isAccessible = true
            val originalFactory = factoryField.get(delegate)
            if (originalFactory == null) {
                val msg = "MediaCodecWrapperFactory is null on ${delegate.javaClass.name}"
                Log.w(TAG, msg)
                NativeInputDiagnostics.add("LowLatencyVideoDecoder: $msg")
                return
            }

            val tunedFactory = OpenNowMediaCodecTuning.wrapFactory(
                originalFactory,
                object : OpenNowMediaCodecTuning.Tuning {
                    override fun selectCodecName(originalName: String): String =
                        if (tuningEnabled) selectStreamCodecName(originalName, lowLatencyEnabled) else originalName

                    override fun configure(codecName: String, format: MediaFormat) {
                        if (allwinnerHevc) {
                            allwinnerOutputIsPadded = false
                            allwinnerConfiguredFor720p = format.intOrNull(MediaFormat.KEY_WIDTH) == HD_WIDTH &&
                                format.intOrNull(MediaFormat.KEY_HEIGHT) == HD_HEIGHT
                        }
                        NativeInputDiagnostics.add(
                            "MediaCodecVideoDecoder: configure codec=$codecName requestedFps=$requestedFps " +
                                "lowLatency=$lowLatencyEnabled standardLowLatency=$standardLowLatencyEnabled before=$format",
                        )
                        if (tuningEnabled) {
                            applyStreamFormat(format, codecName, requestedFps, lowLatencyEnabled, standardLowLatencyEnabled)
                        }
                        NativeInputDiagnostics.add("MediaCodecVideoDecoder: configured format=$format")
                    }

                    override fun started(codecName: String, parameters: OpenNowMediaCodecTuning.ParameterSetter) {
                        if (tuningEnabled) applyStreamParameters(parameters, lowLatencyEnabled, standardLowLatencyEnabled)
                    }

                    override fun initializationFailed(codecName: String, phase: String, error: RuntimeException) {
                        NativeInputDiagnostics.addRetained("codec-initialization-error",
                            "MediaCodec $phase failed decoder=$codecName ${mediaCodecFailureDetail(error)}")
                    }

                    override fun initializationCleanupFailed(codecName: String, error: RuntimeException) {
                        // A second codec must not start when platform ownership is uncertain.
                        initializationCleanupFailed = true
                    }

                    override fun inputQueued() = bufferProgress.inputQueued()
                    override fun outputDequeued(codecConfig: Boolean) = bufferProgress.outputDequeued(codecConfig)
                    override fun outputReleased(render: Boolean) {
                        bufferProgress.outputReleased(render)
                        retainBufferProgress()
                    }

                    override fun outputFormat(codecName: String, format: MediaFormat) {
                        if (allwinnerHevc) {
                            // This decoder reports the aligned slice as the visible image for 720p.
                            allwinnerOutputIsPadded = allwinnerConfiguredFor720p && allwinnerHevcNeeds720pCrop(
                                format.intOrNull(MediaFormat.KEY_WIDTH), format.intOrNull(MediaFormat.KEY_HEIGHT),
                                format.intOrNull("slice-height"), format.intOrNull("crop-left"),
                                format.intOrNull("crop-top"), format.intOrNull("crop-right"),
                                format.intOrNull("crop-bottom"),
                            )
                        }
                        // Negotiated hdr=false does not establish the actual decoder transfer,
                        // range or standard. Retain output metadata for intermittent washed-out
                        // startup reports, even if the recent-event ring has since rolled over.
                        NativeInputDiagnostics.addRetained(
                            "codec-output-format",
                            "MediaCodec texture output decoder=$codecName hdrRequested=false format=$format",
                        )
                    }
                },
            )
            factoryField.set(delegate, tunedFactory)
            wrapperInstalled = true
            val msg = "Successfully patched MediaCodecWrapperFactory on ${delegate.javaClass.name}"
            Log.i(TAG, msg)
            NativeInputDiagnostics.add("LowLatencyVideoDecoder: $msg")
        } catch (tr: Throwable) {
            val msg = "Failed to install low latency MediaCodec wrapper: ${tr.message}"
            Log.w(TAG, msg, tr)
            NativeInputDiagnostics.add("LowLatencyVideoDecoder: $msg")
        }
    }

    private fun MediaFormat.intOrNull(key: String): Int? = if (containsKey(key)) getInteger(key) else null

    private fun retainBufferProgress(force: Boolean = false) {
        bufferProgress.report(android.os.SystemClock.elapsedRealtime(), force)?.let { progress ->
            NativeInputDiagnostics.addRetained("codec-buffer-progress",
                "MediaCodec texture progress decoder=${delegate.implementationName} $progress")
        }
    }

    private fun findMediaCodecWrapperFactoryField(clazz: Class<*>?): Field? {
        var current = clazz
        while (current != null) {
            for (field in current.declaredFields) {
                if ("org.webrtc.MediaCodecWrapperFactory" == field.type.name ||
                    field.name.lowercase(Locale.US).contains("mediacodecwrapperfactory")
                ) {
                    return field
                }
            }
            current = current.superclass
        }
        return null
    }

    companion object {
        private const val TAG = "LowLatencyDecoder"
        private const val OPERATING_RATE = 0x7FFF

        internal fun selectStreamCodecName(originalName: String, lowLatency: Boolean): String =
            if (lowLatency) getLowLatencyCodecNameIfApplicable(originalName) else originalName

        internal fun applyStreamFormat(format: MediaFormat, codecName: String, fps: Int,
            lowLatency: Boolean, standardLowLatency: Boolean) {
            applyDecoderPerformanceFormat(format, fps, lowLatency, standardLowLatency)
            if (lowLatency) applyLowLatencyFormat(format, codecName, fps)
        }

        internal fun applyStreamParameters(parameters: OpenNowMediaCodecTuning.ParameterSetter,
            lowLatency: Boolean, standardLowLatency: Boolean) {
            if (lowLatency || standardLowLatency) applyLowLatencyParameters(parameters,
                standardLowLatencyEnabled = standardLowLatency || lowLatency, vendorLowLatencyEnabled = lowLatency)
        }

        private fun applyDecoderPerformanceFormat(
            format: MediaFormat,
            requestedFps: Int,
            lowLatencyEnabled: Boolean,
            standardLowLatencyEnabled: Boolean,
        ) {
            val exactTargetFps = mediaCodecPerformanceTargetFps(requestedFps)
            if (exactTargetFps != null) {
                putInt(format, MediaFormat.KEY_FRAME_RATE, exactTargetFps)
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M && (exactTargetFps != null || lowLatencyEnabled)) {
                putInt(format, MediaFormat.KEY_PRIORITY, 0)
                putInt(
                    format,
                    MediaFormat.KEY_OPERATING_RATE,
                    if (lowLatencyEnabled) OPERATING_RATE else exactTargetFps!!,
                )
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && standardLowLatencyEnabled) {
                putInt(format, MediaFormat.KEY_LOW_LATENCY, 1)
            }
        }

        private fun applyLowLatencyFormat(format: MediaFormat, codecName: String, requestedFps: Int) {
            putInt(format, "low-latency", 1)

            val normalizedCodecName = codecName.lowercase(Locale.US)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                putInt(format, "priority", 0)
                // Use Short.MAX_VALUE (0x7FFF) for non-Snapdragon decoders; for Qualcomm,
                // forcing 32767 fps operating rate forces Adreno GPU/VPU clocks to maximum state,
                // causing extreme power drain and overheating. Use 120 (or target FPS) instead.
                val operatingRate = vendorLowLatencyOperatingRate(codecName, requestedFps)
                putInt(format, "operating-rate", operatingRate)
            }
            putInt(format, "allow-frame-drop", 1)
            putInt(format, "vdec-lowlatency", 1)
            putInt(format, "vendor.low-latency.enable", 1)

            if (isQualcommDecoder(normalizedCodecName)) {
                putInt(format, "vendor.qti-ext-dec-picture-order.enable", 1)
                putInt(format, "vendor.qti-ext-dec-low-latency.enable", 1)
                putInt(format, "vendor.rtc-ext-dec-low-latency.enable", 1)
            }

            if (isHiSiliconDecoder(normalizedCodecName)) {
                putInt(format, "vendor.hisi-ext-low-latency-video-dec.video-scene-for-low-latency-req", 1)
                putInt(format, "vendor.hisi-ext-low-latency-video-dec.video-scene-for-low-latency-rdy", -1)
            }

            if (isMediaTekDecoder(normalizedCodecName)) {
                putInt(format, "vendor.mtk-dec-low-latency", 1)
                putInt(format, "vendor.mtk-dec-lowlatency", 1)
                putInt(format, "vendor.mtk-ext-dec-low-latency.enable", 1)
                putInt(format, "vendor.mtk-ext-dec-lowlatency.enable", 1)
                putInt(format, "vendor.mtk-vdec-lowlatency", 1)
                putInt(format, "vendor.mtk-vdec-low-latency", 1)
                putInt(format, "vendor.mtk.vdec.lowlatency", 1)
                putInt(format, "vendor.mtk.vdec.low-latency", 1)
                putInt(format, "vendor.mtk.dec.lowlatency", 1)
                putInt(format, "vendor.mtk.dec.low-latency", 1)
                putInt(format, "vendor.mtk.ext.dec.lowlatency.enable", 1)
            }

            Log.i(TAG, "Applied low latency decoder format for codec=$codecName")
        }

        private fun isQualcommDecoder(codecName: String): Boolean {
            return isQualcommMediaCodecDecoder(codecName)
        }

        private fun isHiSiliconDecoder(codecName: String): Boolean {
            val hardware = (Build.HARDWARE ?: "").lowercase(Locale.US)
            val board = (Build.BOARD ?: "").lowercase(Locale.US)
            val manufacturer = (Build.MANUFACTURER ?: "").lowercase(Locale.US)
            return codecName.contains("hisi") ||
                    codecName.contains("kirin") ||
                    hardware.contains("hisi") ||
                    hardware.contains("kirin") ||
                    board.contains("hisi") ||
                    board.contains("kirin") ||
                    manufacturer.contains("huawei")
        }

        private fun isMediaTekDecoder(codecName: String): Boolean {
            val hardware = (Build.HARDWARE ?: "").lowercase(Locale.US)
            val board = (Build.BOARD ?: "").lowercase(Locale.US)
            val manufacturer = (Build.MANUFACTURER ?: "").lowercase(Locale.US)
            return codecName.contains("mtk") ||
                    codecName.contains("mediatek") ||
                    hardware.contains("mtk") ||
                    hardware.contains("mediatek") ||
                    board.contains("mtk") ||
                    board.contains("mediatek") ||
                    manufacturer.contains("mediatek")
        }

        private fun getLowLatencyCodecNameIfApplicable(codecName: String): String {
            val normalized = codecName.lowercase(Locale.US)
            if (normalized.startsWith("c2.mtk.") && normalized.endsWith(".decoder")) {
                val lowLatencyName = "$codecName.lowlatency"
                if (isCodecSupported(lowLatencyName)) {
                    Log.i(TAG, "LowLatencyVideoDecoder: Found MediaTek low latency variant: $lowLatencyName")
                    return lowLatencyName
                }
            }
            return codecName
        }

        private fun isCodecSupported(name: String): Boolean {
            try {
                val list = android.media.MediaCodecList(android.media.MediaCodecList.ALL_CODECS)
                for (info in list.codecInfos) {
                    if (info.name.equals(name, ignoreCase = true)) {
                        return true
                    }
                }
            } catch (tr: Throwable) {
                Log.w(TAG, "Failed to check if codec is supported", tr)
            }
            return false
        }

        private fun putInt(format: MediaFormat, key: String, value: Int) {
            try {
                format.setInteger(key, value)
            } catch (tr: Throwable) {
                Log.w(TAG, "Failed to set MediaFormat key $key", tr)
            }
        }

        private fun applyLowLatencyParameters(
            parameters: OpenNowMediaCodecTuning.ParameterSetter,
            standardLowLatencyEnabled: Boolean,
            vendorLowLatencyEnabled: Boolean,
        ) {
            try {
                val bundle = android.os.Bundle()
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R && standardLowLatencyEnabled) {
                    bundle.putInt(android.media.MediaCodec.PARAMETER_KEY_LOW_LATENCY, 1)
                }
                if (vendorLowLatencyEnabled) {
                    bundle.putInt("vendor.mtk-dec-low-latency", 1)
                    bundle.putInt("vendor.mtk-dec-lowlatency", 1)
                    bundle.putInt("vendor.mtk-ext-dec-low-latency.enable", 1)
                    bundle.putInt("vendor.mtk-ext-dec-lowlatency.enable", 1)
                    bundle.putInt("vendor.mtk-vdec-lowlatency", 1)
                    bundle.putInt("vendor.mtk-vdec-low-latency", 1)
                    bundle.putInt("vendor.mtk.vdec.lowlatency", 1)
                    bundle.putInt("vendor.mtk.vdec.low-latency", 1)
                    bundle.putInt("vendor.mtk.dec.lowlatency", 1)
                    bundle.putInt("vendor.mtk.dec.low-latency", 1)
                    bundle.putInt("vendor.mtk.ext.dec.lowlatency.enable", 1)
                }

                parameters.setParameters(bundle)
                Log.i(TAG, "LowLatencyVideoDecoder: Successfully set MediaCodec parameters: $bundle")
                NativeInputDiagnostics.add("LowLatencyVideoDecoder: Successfully set MediaCodec parameters: $bundle")
            } catch (tr: Throwable) {
                Log.w(TAG, "Failed to apply dynamic MediaCodec parameters", tr)
                NativeInputDiagnostics.add("LowLatencyVideoDecoder: Failed to apply dynamic MediaCodec parameters: ${tr.message}")
            }
        }
    }
}

internal fun mediaCodecPerformanceTargetFps(requestedFps: Int): Int? =
    requestedFps.takeIf { it >= 60 }

internal fun isQualcommMediaCodecDecoder(codecName: String?): Boolean {
    val normalized = codecName?.lowercase(Locale.US).orEmpty()
    return normalized.contains("qcom") || normalized.contains("qti")
}

internal fun shouldBypassMediaCodecPerformanceTuning(
    codec: VideoCodec?,
    decoderImplementationName: String?,
    requestedFps: Int,
    lowLatencyEnabled: Boolean,
): Boolean =
    !lowLatencyEnabled &&
        codec == VideoCodec.H264 &&
        requestedFps == 60 &&
        isQualcommMediaCodecDecoder(decoderImplementationName)

internal fun shouldUseMediaCodecDecoderTuning(
    selectedDecoder: VideoDecoder?,
    approvedHardwareDecoder: VideoDecoder?,
    requestedFps: Int,
    lowLatencyEnabled: Boolean,
    codec: VideoCodec? = null,
    decoderImplementationName: String? = null,
): Boolean =
    selectedDecoder != null &&
        selectedDecoder === approvedHardwareDecoder &&
        (lowLatencyEnabled || mediaCodecPerformanceTargetFps(requestedFps) != null) &&
        !shouldBypassMediaCodecPerformanceTuning(
            codec = codec,
            decoderImplementationName = decoderImplementationName,
            requestedFps = requestedFps,
            lowLatencyEnabled = lowLatencyEnabled,
        )

// Preserve the existing Qualcomm low-latency floor without limiting 240/360 FPS sessions to 120.
internal fun vendorLowLatencyOperatingRate(codecName: String?, requestedFps: Int): Int =
    if (isQualcommMediaCodecDecoder(codecName)) maxOf(120, requestedFps) else 0x7FFF
