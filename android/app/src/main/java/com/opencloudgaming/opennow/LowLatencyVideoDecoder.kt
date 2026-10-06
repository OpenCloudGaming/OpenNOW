package com.opencloudgaming.opennow

import android.media.MediaFormat
import android.os.Build
import android.util.Log
import org.webrtc.EncodedImage
import org.webrtc.VideoCodecStatus
import org.webrtc.VideoDecoder
import java.lang.reflect.Field
import java.lang.reflect.InvocationHandler
import java.lang.reflect.InvocationTargetException
import java.lang.reflect.Method
import java.lang.reflect.Proxy
import java.util.Locale

class LowLatencyVideoDecoder(
    private val delegate: VideoDecoder,
    private val requestedFps: Int,
    private val lowLatencyEnabled: Boolean,
    private val standardLowLatencyEnabled: Boolean = false,
) : VideoDecoder {

    private var patched = false

    override fun initDecode(settings: VideoDecoder.Settings?, decodeCallback: VideoDecoder.Callback?): VideoCodecStatus {
        NativeInputDiagnostics.add(
            "MediaCodecVideoDecoder initDecode delegate=${delegate.javaClass.name} " +
                "requestedFps=$requestedFps lowLatency=$lowLatencyEnabled " +
                "standardLowLatency=$standardLowLatencyEnabled",
        )
        patchMediaCodecWrapperFactory()
        return delegate.initDecode(settings, decodeCallback)
    }

    override fun release(): VideoCodecStatus {
        return delegate.release()
    }

    override fun decode(frame: EncodedImage?, info: VideoDecoder.DecodeInfo?): VideoCodecStatus {
        return delegate.decode(frame, info)
    }

    override fun getImplementationName(): String {
        val suffix = when {
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

            val factoryInterface = factoryField.type
            val proxyFactory = Proxy.newProxyInstance(
                factoryInterface.classLoader,
                arrayOf(factoryInterface),
                MediaCodecWrapperFactoryHandler(
                    delegateFactory = originalFactory,
                    requestedFps = requestedFps,
                    lowLatencyEnabled = lowLatencyEnabled,
                    standardLowLatencyEnabled = standardLowLatencyEnabled,
                )
            )
            factoryField.set(delegate, proxyFactory)
            val msg = "Successfully patched MediaCodecWrapperFactory on ${delegate.javaClass.name}"
            Log.i(TAG, msg)
            NativeInputDiagnostics.add("LowLatencyVideoDecoder: $msg")
        } catch (tr: Throwable) {
            val msg = "Failed to install low latency MediaCodec wrapper: ${tr.message}"
            Log.w(TAG, msg, tr)
            NativeInputDiagnostics.add("LowLatencyVideoDecoder: $msg")
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

    private class MediaCodecWrapperFactoryHandler(
        private val delegateFactory: Any,
        private val requestedFps: Int,
        private val lowLatencyEnabled: Boolean,
        private val standardLowLatencyEnabled: Boolean,
    ) : InvocationHandler {
        override fun invoke(proxy: Any?, method: Method, args: Array<out Any>?): Any? {
            val originalCodecName = if ("createByCodecName" == method.name && args != null && args.isNotEmpty() && args[0] is String) {
                args[0] as String
            } else {
                ""
            }

            val modifiedCodecName = if (lowLatencyEnabled && originalCodecName.isNotEmpty()) {
                getLowLatencyCodecNameIfApplicable(originalCodecName)
            } else {
                originalCodecName
            }

            val finalArgs = if (modifiedCodecName != originalCodecName && args != null) {
                Array<Any>(args.size) { i ->
                    if (i == 0) modifiedCodecName else args[i]
                }
            } else {
                args
            }

            val result = invokeDelegate(delegateFactory, method, finalArgs)
            if ("createByCodecName" != method.name || result == null) {
                return result
            }

            val codecName = modifiedCodecName
            NativeInputDiagnostics.add("LowLatencyVideoDecoder: createByCodecName called for codecName=$codecName")

            var codecWrapperInterface: Class<*>? = if (result.javaClass.interfaces.isNotEmpty()) {
                result.javaClass.interfaces[0]
            } else {
                null
            }

            if (codecWrapperInterface == null || "org.webrtc.MediaCodecWrapper" != codecWrapperInterface.name) {
                codecWrapperInterface = findInterface(result.javaClass, "org.webrtc.MediaCodecWrapper")
            }

            if (codecWrapperInterface == null) {
                NativeInputDiagnostics.add("LowLatencyVideoDecoder: MediaCodecWrapper interface not found on ${result.javaClass.name}")
                return result
            }

            NativeInputDiagnostics.add("LowLatencyVideoDecoder: Successfully wrapping MediaCodecWrapper of class ${result.javaClass.name}")
            return Proxy.newProxyInstance(
                codecWrapperInterface.classLoader,
                arrayOf(codecWrapperInterface),
                MediaCodecWrapperHandler(
                    delegateCodec = result,
                    codecName = codecName,
                    requestedFps = requestedFps,
                    lowLatencyEnabled = lowLatencyEnabled,
                    standardLowLatencyEnabled = standardLowLatencyEnabled,
                )
            )
        }
    }

    private class MediaCodecWrapperHandler(
        private val delegateCodec: Any,
        private val codecName: String,
        private val requestedFps: Int,
        private val lowLatencyEnabled: Boolean,
        private val standardLowLatencyEnabled: Boolean,
    ) : InvocationHandler {
        private var effectiveLowLatency = lowLatencyEnabled

        override fun invoke(proxy: Any?, method: Method, args: Array<out Any>?): Any? {
            if ("configure" == method.name && lowLatencyEnabled && isQualcommDecoder(codecName) &&
                Build.VERSION.SDK_INT >= 29 && args?.firstOrNull() is MediaFormat) {
                val original = args[0] as MediaFormat
                val field = findMediaCodecField(delegateCodec.javaClass)
                field?.isAccessible = true
                val actualCodec = field?.get(delegateCodec) as? android.media.MediaCodec
                var first = true
                return tryDecoderLatencyProfiles(
                    if (actualCodec != null) decoderLatencyProfiles(true) else listOf(DecoderLatencyProfile.CORE),
                    { profile, error -> NativeInputDiagnostics.addRetained("decoder.configureFallback",
                        "SDR rejected profile=$profile error=${error.javaClass.simpleName}") },
                ) { profile ->
                    if (!first) requireNotNull(actualCodec).reset()
                    first = false
                    val format = MediaFormat(original)
                    applyDecoderPerformanceFormat(format, requestedFps, false, standardLowLatencyEnabled)
                    applyDecoderLatencyProfile(format, codecName, requestedFps, profile)
                    val configuredArgs = Array<Any>(args.size) { i -> if (i == 0) format else args[i] }
                    val result = invokeDelegate(delegateCodec, method, configuredArgs)
                    effectiveLowLatency = profile != DecoderLatencyProfile.REALTIME
                    NativeInputDiagnostics.add("SDR decoder codec=$codecName profile=$profile format=$format")
                    result
                }
            }
            if ("configure" == method.name && args != null && args.isNotEmpty() && args[0] is MediaFormat) {
                val format = args[0] as MediaFormat
                NativeInputDiagnostics.add(
                    "MediaCodecVideoDecoder: configure codec=$codecName requestedFps=$requestedFps " +
                        "lowLatency=$lowLatencyEnabled standardLowLatency=$standardLowLatencyEnabled before=$format",
                )
                applyDecoderPerformanceFormat(
                    format = format,
                    requestedFps = requestedFps,
                    lowLatencyEnabled = lowLatencyEnabled,
                    standardLowLatencyEnabled = standardLowLatencyEnabled,
                )
                if (lowLatencyEnabled) applyLowLatencyFormat(format, codecName, requestedFps)
                NativeInputDiagnostics.add("MediaCodecVideoDecoder: configured format=$format")
            }
            val result = invokeDelegate(delegateCodec, method, args)
            if ("start" == method.name && (effectiveLowLatency || standardLowLatencyEnabled)) {
                NativeInputDiagnostics.add("LowLatencyVideoDecoder: Intercepted start() for codec=$codecName")
                applyLowLatencyParameters(
                    delegateCodec = delegateCodec,
                    standardLowLatencyEnabled = standardLowLatencyEnabled || effectiveLowLatency,
                    vendorLowLatencyEnabled = effectiveLowLatency,
                )
            }
            return result
        }
    }

    companion object {
        private const val TAG = "LowLatencyDecoder"
        private const val OPERATING_RATE = 0x7FFF

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

        private fun findInterface(clazz: Class<*>?, interfaceName: String): Class<*>? {
            var current = clazz
            while (current != null) {
                for (item in current.interfaces) {
                    if (interfaceName == item.name) {
                        return item
                    }
                }
                current = current.superclass
            }
            return null
        }

        private fun invokeDelegate(target: Any, method: Method, args: Array<out Any>?): Any? {
            return try {
                method.isAccessible = true
                if (args == null) {
                    method.invoke(target)
                } else {
                    method.invoke(target, *args)
                }
            } catch (ex: InvocationTargetException) {
                throw ex.cause ?: ex
            } catch (ex: SecurityException) {
                if (args == null) {
                    method.invoke(target)
                } else {
                    method.invoke(target, *args)
                }
            }
        }

        private fun applyLowLatencyFormat(format: MediaFormat, codecName: String, requestedFps: Int) {
            val normalizedCodecName = codecName.lowercase(Locale.US)
            if (isQualcommDecoder(normalizedCodecName)) {
                // Older APIs retain core hints; modern codecs use the fresh-format retry path above.
                applyDecoderLatencyProfile(format, codecName, requestedFps, DecoderLatencyProfile.CORE)
                return
            }
            putInt(format, "low-latency", 1)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                putInt(format, "priority", 0)
                putInt(format, "operating-rate", OPERATING_RATE)
            }
            putInt(format, "allow-frame-drop", 1)
            putInt(format, "vdec-lowlatency", 1)
            putInt(format, "vendor.low-latency.enable", 1)

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
            delegateCodec: Any,
            standardLowLatencyEnabled: Boolean,
            vendorLowLatencyEnabled: Boolean,
        ) {
            try {
                val field = findMediaCodecField(delegateCodec.javaClass) ?: return
                field.isAccessible = true
                val mediaCodec = field.get(delegateCodec) as? android.media.MediaCodec ?: return
                
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

                mediaCodec.setParameters(bundle)
                Log.i(TAG, "LowLatencyVideoDecoder: Successfully set MediaCodec parameters: $bundle")
                NativeInputDiagnostics.add("LowLatencyVideoDecoder: Successfully set MediaCodec parameters: $bundle")
            } catch (tr: Throwable) {
                Log.w(TAG, "Failed to apply dynamic MediaCodec parameters", tr)
                NativeInputDiagnostics.add("LowLatencyVideoDecoder: Failed to apply dynamic MediaCodec parameters: ${tr.message}")
            }
        }

        private fun findMediaCodecField(clazz: Class<*>?): Field? {
            var current = clazz
            while (current != null) {
                for (field in current.declaredFields) {
                    if (field.type == android.media.MediaCodec::class.java) {
                        return field
                    }
                }
                current = current.superclass
            }
            return null
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
