package com.opencloudgaming.opennow

import android.app.ActivityManager
import android.content.Context
import android.content.res.Configuration
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.os.Build
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonObject
import org.webrtc.DefaultVideoDecoderFactory
import org.webrtc.EglBase
import org.webrtc.HardwareVideoDecoderFactory
import org.webrtc.IceCandidate
import org.webrtc.PeerConnection
import org.webrtc.PeerConnectionFactory
import org.webrtc.Predicate
import org.webrtc.RtpCapabilities
import org.webrtc.VideoCodecInfo
import org.webrtc.VideoDecoder
import org.webrtc.VideoDecoderFactory
import java.util.Locale
import java.util.concurrent.ConcurrentHashMap
import kotlin.math.max

object NativeCodecProbe {
    init {
        runCatching { System.loadLibrary("opennow_native") }
    }

    external fun nativeRuntimeSummary(): String
    external fun nativeDecoderAvailable(mimeType: String): Boolean
}

internal object WebRtcRuntime {
    @Volatile
    private var initialized = false

    fun ensureInitialized(context: Context) {
        if (initialized) return
        synchronized(this) {
            if (initialized) return
            PeerConnectionFactory.initialize(
                PeerConnectionFactory.InitializationOptions.builder(context.applicationContext)
                    .setEnableInternalTracer(false)
                    .createInitializationOptions(),
            )
            initialized = true
        }
    }
}

object CodecProbe {
    private data class DecoderLimits(
        val maxSupportedWidth: Int?,
        val maxSupportedHeight: Int?,
    )

    fun report(context: Context): RuntimeCodecReport {
        WebRtcRuntime.ensureInitialized(context)
        val isTv = isAndroidTvProfile(context)
        val renderer = listOf(Build.HARDWARE, Build.BOARD, Build.DEVICE, Build.MODEL, Build.MANUFACTURER)
            .joinToString(" ")
            .lowercase(Locale.US)
        val memoryInfo = ActivityManager.MemoryInfo()
        val totalMemoryBytes = runCatching {
            (context.getSystemService(Context.ACTIVITY_SERVICE) as? ActivityManager)
                ?.getMemoryInfo(memoryInfo)
            memoryInfo.totalMem.takeIf { it > 0L }
        }.getOrNull()
        val is64BitRuntime = android.os.Process.is64Bit()
        val constrainedRuntime = isConstrainedStreamingRuntime(
            androidTvProfile = isTv,
            is64BitRuntime = is64BitRuntime,
            totalMemoryBytes = totalMemoryBytes,
        )
        val lowPower = isLowPowerStreamingProfile(
            androidTvProfile = isTv,
            renderer = renderer,
            totalMemoryBytes = totalMemoryBytes,
            is64BitRuntime = is64BitRuntime,
        )
        val webRtcDecoders = probeWebRtcDecoders()
        val capabilities = VideoCodec.entries.map { codec ->
            val mime = codec.mimeType()
            val decoders = codecInfos(mime, encoder = false)
            val encoders = codecInfos(mime, encoder = true)
            val webRtc = webRtcDecoders[codec]
            val nativeDecoderAvailable = runCatching { NativeCodecProbe.nativeDecoderAvailable(mime) }.getOrNull()
            val preferredDecoder = decoders.firstOrNull(::isHardwareCodec) ?: decoders.firstOrNull()
            val preferredEncoder = encoders.firstOrNull(::isHardwareCodec) ?: encoders.firstOrNull()
            val decoderLimits = decoderLimits(mime, decoders)
            CodecCapability(
                codec = codec,
                decoderAvailable = decoders.isNotEmpty(),
                encoderAvailable = encoders.isNotEmpty(),
                hardwareDecoder = decoders.any(::isHardwareCodec),
                hardwareEncoder = encoders.any(::isHardwareCodec),
                decoderName = preferredDecoder?.name,
                encoderName = preferredEncoder?.name,
                realtimeSafe = decoders.any { isRealtimeSafeDecoder(codec, it) },
                nativeDecoderAvailable = nativeDecoderAvailable,
                webRtcDecoderAvailable = webRtc?.decoderAvailable,
                webRtcHardwareDecoderAvailable = webRtc?.hardwareDecoderAvailable,
                webRtcDecoderName = webRtc?.decoderName,
                webRtcCodecProfiles = webRtc?.profiles.orEmpty(),
                maxSupportedWidth = decoderLimits.maxSupportedWidth,
                maxSupportedHeight = decoderLimits.maxSupportedHeight,
            )
        }
        return RuntimeCodecReport(
            capabilities = capabilities,
            nativeRuntimeSummary = runCatching { NativeCodecProbe.nativeRuntimeSummary() }.getOrElse { "{\"nativeLibrary\":\"unavailable\"}" },
            androidTvProfile = isTv,
            lowPowerGpuProfile = lowPower,
            constrainedRuntimeProfile = constrainedRuntime,
        ).also { report ->
            NativeInputDiagnostics.add(
                "codec probe device=${Build.MANUFACTURER}/${Build.MODEL} hardware=${Build.HARDWARE} tv=$isTv lowPower=$lowPower " +
                    "constrained=$constrainedRuntime runtimeBits=${if (is64BitRuntime) 64 else 32} " +
                    "memoryMiB=${totalMemoryBytes?.div(BYTES_PER_MEBIBYTE) ?: 0L}",
            )
            report.capabilities.forEach { capability ->
                NativeInputDiagnostics.add(
                    "codec probe codec=${capability.codec} platform=${capability.decoderName ?: "none"} " +
                        "platformHw=${capability.hardwareDecoder} native=${capability.nativeDecoderAvailable} " +
                        "webrtc=${capability.webRtcDecoderName ?: "none"} webrtcHw=${capability.webRtcHardwareDecoderAvailable} " +
                    "profiles=${capability.webRtcCodecProfiles.joinToString("|").ifBlank { "none" }} " +
                    "max=${capability.maxSupportedWidth ?: 0}x${capability.maxSupportedHeight ?: 0} " +
                    "launch=${capability.streamingDecoderUsableForLaunch()}",
                )
            }
        }
    }

    private fun decoderLimits(mime: String, decoders: List<MediaCodecInfo>): DecoderLimits {
        val candidates = decoders.filter(::isHardwareCodec).ifEmpty { decoders }
        if (candidates.isEmpty()) return DecoderLimits(null, null)
        val knownResolutions = streamAspectRatioOptions()
            .flatMap(::streamResolutionOptionsForAspect)
            .distinct()
        val supportedPixels = mutableListOf<Pair<Int, Int>>()
        for (resolution in knownResolutions) {
            val (width, height) = parseResolutionPixelsOrNull(resolution) ?: continue
            val sizeSupported = candidates.any { decoder ->
                runCatching {
                    decoder.getCapabilitiesForType(mime).videoCapabilities?.isSizeSupported(width, height) == true
                }.getOrDefault(false)
            }
            if (sizeSupported) supportedPixels += width to height
        }
        return DecoderLimits(
            maxSupportedWidth = supportedPixels.maxOfOrNull { it.first },
            maxSupportedHeight = supportedPixels.maxOfOrNull { it.second },
        )
    }

    private fun codecInfos(mime: String, encoder: Boolean): List<MediaCodecInfo> {
        val list = if (Build.VERSION.SDK_INT >= 21) {
            MediaCodecList(MediaCodecList.ALL_CODECS).codecInfos.toList()
        } else {
            emptyList()
        }
        return list.filter { info ->
            info.isEncoder == encoder && info.supportedTypes.any { it.equals(mime, ignoreCase = true) }
        }
    }

    private data class WebRtcCodecProbe(
        val decoderAvailable: Boolean,
        val hardwareDecoderAvailable: Boolean,
        val decoderName: String?,
        val profiles: List<String>,
    )

    private fun probeWebRtcDecoders(): Map<VideoCodec, WebRtcCodecProbe> {
        val eglBase = runCatching { EglBase.create() }.getOrNull() ?: return emptyMap()
        return try {
            val streamingFactory = OpenNowVideoDecoderFactory(eglBase.eglBaseContext)
            val hardwareFactory = openNowHardwareVideoDecoderFactory(eglBase.eglBaseContext)
            val streamingSupported = streamingFactory.supportedCodecsByVideoCodec()
            val hardwareSupported = hardwareFactory.supportedCodecsByVideoCodec()
            VideoCodec.entries.associateWith { codec ->
                val defaultInfos = streamingSupported[codec].orEmpty()
                val hardwareInfos = hardwareSupported[codec].orEmpty()
                val decoderName = streamingFactory.firstDecoderName(defaultInfos)
                WebRtcCodecProbe(
                    decoderAvailable = decoderName != null,
                    hardwareDecoderAvailable = hardwareFactory.firstDecoderName(hardwareInfos) != null,
                    decoderName = decoderName,
                    profiles = defaultInfos.map(::formatWebRtcCodecInfo).distinct(),
                )
            }
        } catch (_: Throwable) {
            emptyMap()
        } finally {
            eglBase.release()
        }
    }

    private fun VideoDecoderFactory.supportedCodecsByVideoCodec(): Map<VideoCodec, List<VideoCodecInfo>> =
        getSupportedCodecs()
            .groupBy { info -> info.name.toVideoCodec() }
            .mapNotNull { (codec, infos) -> codec?.let { it to infos } }
            .toMap()

    private fun VideoDecoderFactory.firstDecoderName(infos: List<VideoCodecInfo>): String? {
        for (info in infos) {
            val decoder = runCatching { createDecoder(info) }.getOrNull() ?: continue
            return try {
                decoder.getImplementationName()
            } finally {
                runCatching { decoder.release() }
            }
        }
        return null
    }

    private fun String.toVideoCodec(): VideoCodec? =
        when (uppercase(Locale.US)) {
            "AVC", "H264", "H.264" -> VideoCodec.H264
            "HEVC", "H265", "H.265" -> VideoCodec.H265
            "AV01", "AV1" -> VideoCodec.AV1
            else -> null
        }

    private fun formatWebRtcCodecInfo(info: VideoCodecInfo): String {
        val profile = info.params["profile-level-id"]
        val packetization = info.params["packetization-mode"]
        return listOfNotNull(info.name.toVideoCodec()?.name ?: info.name, profile?.let { "profile=$it" }, packetization?.let { "packet=$it" })
            .joinToString(" ")
    }

    private fun isHardwareCodec(info: MediaCodecInfo): Boolean {
        val name = info.name.lowercase(Locale.US)
            if (android.os.Build.VERSION.SDK_INT >= android.os.Build.VERSION_CODES.Q) {
        if (!info.isHardwareAccelerated) return false
    } else {
        if (name.startsWith("omx.google.") || name.startsWith("c2.android.") || name.contains("sw") || name.contains("software")) return false
    }
        if (name.contains(".sw.") || name.contains("software") || name.startsWith("omx.google.") || name.startsWith("c2.android.")) return false
        return if (Build.VERSION.SDK_INT >= 29) {
            info.isHardwareAccelerated
        } else {
            true
        }
    }

    internal fun isOpenNowHardwareDecoderAllowed(info: MediaCodecInfo): Boolean {
        if (!isHardwareCodec(info)) return false
        val name = info.name.lowercase(Locale.US)
        if (name.contains("software") || name.contains(".sw.") || name.startsWith("omx.google.")) return false
        if (name.contains("exynos")) {
            val hevcProfiles = runCatching {
                info.getCapabilitiesForType(HEVC_MIME_TYPE)
                    .profileLevels
                    .map { it.profile }
            }.getOrDefault(emptyList())
            return isSupportedExynosHevcDecoder(
                codecName = info.name,
                sdkInt = Build.VERSION.SDK_INT,
                supportedTypes = info.supportedTypes.toList(),
                hevcProfiles = hevcProfiles,
            )
        }
        return true
    }

    private fun isRealtimeSafeDecoder(codec: VideoCodec, info: MediaCodecInfo): Boolean {
        if (!isHardwareCodec(info)) return false
        val name = info.name.lowercase(Locale.US)
        return when (codec) {
            VideoCodec.H264 -> true
            VideoCodec.H265 -> !name.contains("exynos") || isOpenNowHardwareDecoderAllowed(info)
            VideoCodec.AV1 -> isOpenNowHardwareDecoderAllowed(info)
        }
    }

    private fun VideoCodec.mimeType(): String =
        when (this) {
            VideoCodec.H264 -> "video/avc"
            VideoCodec.H265 -> "video/hevc"
            VideoCodec.AV1 -> "video/av01"
        }
}

internal fun isSupportedExynosHevcDecoder(
    codecName: String,
    sdkInt: Int,
    supportedTypes: Collection<String>,
    hevcProfiles: Collection<Int>,
): Boolean {
    if (!codecName.contains("exynos", ignoreCase = true)) return false
    if (sdkInt < MIN_EXYNOS_HEVC_SDK) return false
    if (supportedTypes.none { it.equals(HEVC_MIME_TYPE, ignoreCase = true) }) return false
    return hevcProfiles.any(SUPPORTED_HEVC_STREAM_PROFILES::contains)
}

private const val MIN_EXYNOS_HEVC_SDK = 36
private const val HEVC_MIME_TYPE = "video/hevc"
private val SUPPORTED_HEVC_STREAM_PROFILES = setOf(
    MediaCodecInfo.CodecProfileLevel.HEVCProfileMain,
    MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10,
    MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10HDR10,
    MediaCodecInfo.CodecProfileLevel.HEVCProfileMain10HDR10Plus,
)

internal fun isAndroidTvProfile(context: Context): Boolean =
    context.packageManager.hasSystemFeature("android.software.leanback") ||
        context.resources.configuration.uiMode and Configuration.UI_MODE_TYPE_MASK == Configuration.UI_MODE_TYPE_TELEVISION

internal fun isLowPowerStreamingProfile(
    androidTvProfile: Boolean,
    renderer: String,
    totalMemoryBytes: Long?,
    is64BitRuntime: Boolean = true,
): Boolean {
    val normalizedRenderer = renderer.lowercase(Locale.US)
    val knownLowPowerGpu =
        normalizedRenderer.contains("powervr") ||
            normalizedRenderer.contains("ge8320") ||
            normalizedRenderer.contains("ge83")
    return knownLowPowerGpu || isConstrainedStreamingRuntime(
        androidTvProfile = androidTvProfile,
        is64BitRuntime = is64BitRuntime,
        totalMemoryBytes = totalMemoryBytes,
    )
}

internal fun isConstrainedStreamingRuntime(
    androidTvProfile: Boolean,
    is64BitRuntime: Boolean,
    totalMemoryBytes: Long?,
): Boolean {
    val constrainedTvMemory = androidTvProfile &&
        totalMemoryBytes != null &&
        totalMemoryBytes in 1..LOW_POWER_TV_MEMORY_LIMIT_BYTES
    return !is64BitRuntime || constrainedTvMemory
}

private fun openNowHardwareVideoDecoderFactory(sharedContext: EglBase.Context): VideoDecoderFactory =
    HardwareVideoDecoderFactory(
        sharedContext,
        Predicate<MediaCodecInfo> { info -> CodecProbe.isOpenNowHardwareDecoderAllowed(info) },
    )

internal class OpenNowVideoDecoderFactory(
    sharedContext: EglBase.Context,
    private val nativeLowLatencyDecoderEnabled: Boolean = false,
    private val requestedFps: () -> Int = { 60 },
    private val hdrEnabled: () -> Boolean = { false },
    private val hdrSurface: () -> HdrSurfaceTarget? = { null },
) : VideoDecoderFactory {
    private val defaultFactory = DefaultVideoDecoderFactory(sharedContext)
    private val hardwareFactory = openNowHardwareVideoDecoderFactory(sharedContext)

    override fun createDecoder(info: VideoCodecInfo): VideoDecoder? {
        val codec = info.name.toOpenNowVideoCodec()
        if (hdrEnabled()) {
            // Never let an HDR session fall through an 8-bit texture or software decoder.
            return if (codec == VideoCodec.H265) HdrSurfaceVideoDecoder(requestedFps(), hdrSurface) else null
        }
        val hardwareDecoder = if (codec != null) hardwareFactory.createDecoder(info) else null
        val decoder = when (codec) {
            VideoCodec.H264 -> hardwareDecoder ?: defaultFactory.createDecoder(info)
            VideoCodec.H265,
            VideoCodec.AV1,
            -> hardwareDecoder
            null -> defaultFactory.createDecoder(info)
        }
        val exactRequestedFps = requestedFps().coerceAtLeast(1)
        val hardwareDecoderImplementation = hardwareDecoder?.getImplementationName()
        val standardLowLatencyAdvertised = codec?.let { selectedCodec ->
            supportsStandardLowLatencyDecoder(
                codecName = hardwareDecoderImplementation,
                mimeType = selectedCodec.mediaMimeType(),
            )
        } == true
        val standardLowLatencyEnabled = shouldEnableMediaTekStandardLowLatency(
            decoderImplementationName = hardwareDecoderImplementation,
            requestedFps = exactRequestedFps,
            featureAdvertised = standardLowLatencyAdvertised,
        )
        val bypassDecoderPerformanceTuning = shouldBypassMediaCodecPerformanceTuning(
            codec = codec,
            decoderImplementationName = hardwareDecoderImplementation,
            requestedFps = exactRequestedFps,
            lowLatencyEnabled = nativeLowLatencyDecoderEnabled,
        )
        val tuneDecoderPerformance =
            mediaCodecPerformanceTargetFps(exactRequestedFps) != null && !bypassDecoderPerformanceTuning
        val tuneSelectedDecoder = shouldUseMediaCodecDecoderTuning(
            selectedDecoder = decoder,
            approvedHardwareDecoder = hardwareDecoder,
            requestedFps = exactRequestedFps,
            lowLatencyEnabled = nativeLowLatencyDecoderEnabled,
            codec = codec,
            decoderImplementationName = hardwareDecoderImplementation,
        )
        if (codec != null && hardwareDecoder != null) {
            NativeInputDiagnostics.add(
                "native MediaCodec decoder selected codec=${codec.name} " +
                    "implementation=$hardwareDecoderImplementation requestedFps=$exactRequestedFps " +
                    "performanceTuning=$tuneDecoderPerformance lowLatency=$nativeLowLatencyDecoderEnabled " +
                    "standardLowLatencyAdvertised=$standardLowLatencyAdvertised " +
                    "standardLowLatency=$standardLowLatencyEnabled " +
                    "qualcommH264Guard=$bypassDecoderPerformanceTuning",
            )
        } else if (codec != null && decoder != null && (nativeLowLatencyDecoderEnabled || tuneDecoderPerformance)) {
            NativeInputDiagnostics.add(
                "MediaCodec tuning skipped codec=${codec.name} decoder=${decoder.javaClass.name} " +
                    "reason=non-approved-hardware-decoder",
            )
        }
        return if (decoder != null && tuneSelectedDecoder) {
            LowLatencyVideoDecoder(
                delegate = decoder,
                requestedFps = exactRequestedFps,
                lowLatencyEnabled = nativeLowLatencyDecoderEnabled,
                standardLowLatencyEnabled = standardLowLatencyEnabled,
            )
        } else {
            decoder
        }
    }

    override fun getSupportedCodecs(): Array<VideoCodecInfo> {
        val defaultCodecs = defaultFactory.getSupportedCodecs()
            .filterNot { it.name.toOpenNowVideoCodec() in ADVANCED_STREAM_CODECS }
        val nativeAdvancedCodecs = hardwareFactory.getSupportedCodecs()
            .filter { it.name.toOpenNowVideoCodec() in ADVANCED_STREAM_CODECS }
        return (defaultCodecs + nativeAdvancedCodecs)
            .distinctBy { it.stableKey() }
            .toTypedArray()
    }

    private fun VideoCodecInfo.stableKey(): String =
        "${name.uppercase(Locale.US)}:${params.toSortedMap()}"

    private companion object {
        private val ADVANCED_STREAM_CODECS = setOf(VideoCodec.H265, VideoCodec.AV1)
    }
}

private fun String.toOpenNowVideoCodec(): VideoCodec? =
    when (uppercase(Locale.US)) {
        "AVC", "H264", "H.264" -> VideoCodec.H264
        "HEVC", "H265", "H.265" -> VideoCodec.H265
        "AV01", "AV1" -> VideoCodec.AV1
        else -> null
    }

private fun VideoCodec.mediaMimeType(): String = when (this) {
    VideoCodec.H264 -> "video/avc"
    VideoCodec.H265 -> "video/hevc"
    VideoCodec.AV1 -> "video/av01"
}

private fun supportsStandardLowLatencyDecoder(codecName: String?, mimeType: String): Boolean {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R || codecName.isNullOrBlank()) return false
    val cacheKey = "${codecName.lowercase(Locale.US)}|${mimeType.lowercase(Locale.US)}"
    return standardLowLatencySupportCache.getOrPut(cacheKey) {
        val codecInfo = runCatching {
            MediaCodecList(MediaCodecList.ALL_CODECS).codecInfos.firstOrNull {
                it.name.equals(codecName, ignoreCase = true)
            }
        }.getOrNull() ?: return@getOrPut false
        runCatching {
            codecInfo.getCapabilitiesForType(mimeType)
                .isFeatureSupported(MediaCodecInfo.CodecCapabilities.FEATURE_LowLatency)
        }.getOrDefault(false)
    }
}

private val standardLowLatencySupportCache = ConcurrentHashMap<String, Boolean>()

internal fun isMediaTekMediaCodecDecoder(codecName: String?): Boolean {
    val normalized = codecName?.lowercase(Locale.US).orEmpty()
    return normalized.contains("mtk") || normalized.contains("mediatek")
}

internal fun shouldEnableMediaTekStandardLowLatency(
    decoderImplementationName: String?,
    requestedFps: Int,
    featureAdvertised: Boolean,
): Boolean =
    requestedFps >= 60 &&
        featureAdvertised &&
        isMediaTekMediaCodecDecoder(decoderImplementationName)

internal fun VideoCodec.webRtcCodecName(): String =
    when (this) {
        VideoCodec.H264 -> "H264"
        VideoCodec.H265 -> "H265"
        VideoCodec.AV1 -> "AV1"
    }

internal fun RtpCapabilities.CodecCapability.openNowCodecName(): String? {
    val fromMime = mimeType
        ?.substringAfter("/", "")
        ?.takeIf { it.isNotBlank() }
        ?.toOpenNowVideoCodec()
        ?.webRtcCodecName()
    if (fromMime != null) return fromMime
    return name?.toOpenNowVideoCodec()?.webRtcCodecName() ?: name?.uppercase(Locale.US)
}

internal fun RtpCapabilities.CodecCapability.codecParameterInt(name: String): Int? =
    parameters
        ?.entries
        ?.firstOrNull { it.key.equals(name, ignoreCase = true) }
        ?.value
        ?.toIntOrNull()

internal fun RtpCapabilities.CodecCapability.h265ProfilePriority(preferTenBit: Boolean): Int {
    val profile = codecParameterInt("profile-id")
    return if (preferTenBit) {
        when (profile) {
            2 -> 0
            1 -> 1
            else -> 2
        }
    } else {
        when (profile) {
            1 -> 0
            null -> 1
            2 -> 2
            else -> 3
        }
    }
}

internal fun RtpCapabilities.CodecCapability.preferenceKey(): String =
    "${openNowCodecName().orEmpty()}:${parameters.orEmpty().toSortedMap()}"

internal fun StreamSettings.prefersTenBitVideo(): Boolean =
    hdrEnabled ||
        colorQuality == ColorQuality.TenBit420 ||
        colorQuality == ColorQuality.TenBit444

internal val WEBRTC_AUXILIARY_VIDEO_CODECS = setOf("RTX", "RED", "ULPFEC", "FLEXFEC-03")

internal fun streamDiagnosticId(value: String?): String {
    val cleaned = value.orEmpty().trim()
    if (cleaned.isBlank()) return "-"
    return if (cleaned.length <= 12) cleaned else "${cleaned.take(4)}...${cleaned.takeLast(6)}"
}

internal fun signalingUrlForDiagnostics(url: String, sessionId: String): String =
    redactDiagnosticUrl(url).replace(sessionId, streamDiagnosticId(sessionId))

internal enum class SignalingFailureDisposition {
    RetryTransport,
    RetrySignaling,
    RecoverSession,
    SessionEnded,
}

internal fun signalingFailureDisposition(message: String): SignalingFailureDisposition = when {
    message.contains("http=410", ignoreCase = true) -> SignalingFailureDisposition.SessionEnded
    message.contains("http=404", ignoreCase = true) ||
        message.contains("Not Found", ignoreCase = true) -> SignalingFailureDisposition.RecoverSession
    isTransientSignalingServiceFailure(message) -> SignalingFailureDisposition.RetrySignaling
    else -> SignalingFailureDisposition.RetryTransport
}

internal fun isTransientSignalingServiceFailure(message: String): Boolean =
    TRANSIENT_SIGNALING_HTTP_STATUS.containsMatchIn(message) ||
        message.contains("Service Unavailable", ignoreCase = true)

internal fun transientSignalingRetryDelayMs(failureCount: Int): Long? = when (failureCount) {
    1 -> 1_000L
    2 -> 2_000L
    3 -> 4_000L
    else -> null
}

internal fun shouldPreserveMediaAfterSignalingFailure(
    disposition: SignalingFailureDisposition,
    iceState: PeerConnection.IceConnectionState?,
): Boolean {
    if (
        disposition != SignalingFailureDisposition.RetryTransport &&
        disposition != SignalingFailureDisposition.RetrySignaling
    ) {
        return false
    }
    return when (iceState) {
        PeerConnection.IceConnectionState.CHECKING,
        PeerConnection.IceConnectionState.CONNECTED,
        PeerConnection.IceConnectionState.COMPLETED,
        -> true
        else -> false
    }
}

private val TRANSIENT_SIGNALING_HTTP_STATUS =
    Regex("""http=(?:429|500|502|503|504)\b""", RegexOption.IGNORE_CASE)

internal fun signalingHeartbeatReply(message: JsonObject): String? =
    if (message["hb"] != null) """{"hb":1}""" else null

internal fun IceCandidate.diagnosticSummary(): String {
    val raw = sdp
    val protocol = Regex("""\s(udp|tcp)\s""", RegexOption.IGNORE_CASE)
        .find(raw)
        ?.value
        ?.trim()
        ?.lowercase(Locale.US)
        ?: "unknown"
    val type = Regex("""\styp\s+([a-z0-9]+)""", RegexOption.IGNORE_CASE)
        .find(raw)
        ?.groupValues
        ?.getOrNull(1)
        ?.lowercase(Locale.US)
        ?: "unknown"
    val address = Regex("""candidate:\S+\s+\d+\s+\S+\s+\d+\s+([^\s]+)\s+(\d+)""")
        .find(raw)
        ?.let { match -> "${match.groupValues[1]}:${match.groupValues[2]}" }
        ?: "unknown"
    return "mid=${sdpMid.orEmpty()} line=$sdpMLineIndex type=$type protocol=$protocol address=$address raw=${raw.take(240)}"
}

internal fun sdpDiagnosticSummary(label: String, sdp: String): String {
    val lines = sdp.split(Regex("\\r?\\n")).filter { it.isNotBlank() }
    val media = lines.filter { it.startsWith("m=") }.joinToString("|").take(180)
    val candidateEndpoints = lines
        .filter { it.startsWith("a=candidate:") }
        .mapNotNull { line ->
            Regex("""a=candidate:\S+\s+\d+\s+\S+\s+\d+\s+([^\s]+)\s+(\d+)""")
                .find(line)
                ?.let { match -> "${match.groupValues[1]}:${match.groupValues[2]}" }
        }
        .distinct()
        .joinToString(limit = 6)
    val codecs = lines
        .filter { it.startsWith("a=rtpmap:") }
        .mapNotNull { line -> line.substringAfter(' ', "").substringBefore('/').takeIf { it.isNotBlank() } }
        .distinct()
        .take(12)
        .joinToString(",")
    val candidates = lines.count { it.startsWith("a=candidate:") }
    val hasIce = lines.any { it.startsWith("a=ice-ufrag:") } && lines.any { it.startsWith("a=ice-pwd:") }
    val hasFingerprint = lines.any { it.startsWith("a=fingerprint:") }
    return "$label lines=${lines.size} media=$media codecs=$codecs candidates=$candidates endpoints=$candidateEndpoints ice=$hasIce fingerprint=$hasFingerprint"
}
