package com.opencloudgaming.opennow

import kotlin.math.roundToInt

enum class SessionReportRating(val label: String) {
    Excellent("Excellent"),
    Good("Good"),
    Fair("Fair"),
    Poor("Needs work"),
}

enum class SessionReportFindingKind {
    Info,
    Warning,
}

data class SessionReportFinding(
    val title: String,
    val detail: String,
    val kind: SessionReportFindingKind = SessionReportFindingKind.Info,
)

internal data class StreamReportLaunchProfile(
    val gameTitle: String,
    val selectedSettings: StreamSettings,
    val eligibleSettings: StreamSettings,
    val initialSettings: StreamSettings,
)

data class SessionReport(
    val gameTitle: String,
    val score: Int,
    val rating: SessionReportRating,
    val durationSeconds: Int,
    val sampleCount: Int,
    val limitedData: Boolean,
    val averagePingMs: Int?,
    val peakPingMs: Int?,
    val averageBitrateKbps: Int?,
    val peakBitrateKbps: Int?,
    val packetLossPct: Double?,
    val averageJitterMs: Double?,
    val averageFps: Double?,
    val targetFps: Int,
    val averageDecodeMs: Double?,
    val requestedResolution: String,
    val deliveredResolution: String?,
    val requestedCodec: VideoCodec,
    val deliveredCodec: String?,
    val networkKind: AndroidNetworkKind,
    val wifiBand: AndroidWifiBand,
    val estimatedLinkDownstreamKbps: Int?,
    val downgrades: List<SessionReportFinding>,
    val recommendations: List<SessionReportFinding>,
)

internal class StreamSessionReportAccumulator(
    private val launchProfile: StreamReportLaunchProfile,
    private val startedAtMs: Long,
) {
    private var sampleCount = 0
    private var pingCount = 0
    private var pingTotal = 0L
    private var peakPingMs: Int? = null
    private var bitrateCount = 0
    private var bitrateTotal = 0L
    private var peakBitrateKbps: Int? = null
    private var jitterCount = 0
    private var jitterTotal = 0.0
    private var fpsCount = 0
    private var fpsTotal = 0L
    private var receivedFpsCount = 0
    private var receivedFpsTotal = 0L
    private var decodedFpsCount = 0
    private var decodedFpsTotal = 0L
    private var decodeCount = 0
    private var decodeTotal = 0.0
    private var consecutiveDecoderOverloadSamples = 0
    private var decoderOverloadDetected = false
    private var packetLossSampleCount = 0
    private var packetLossSampleTotal = 0.0
    private var packetsLost = 0L
    private var packetsReceived = 0L
    private var hasPacketDeltas = false
    private var lastResolution: String? = null
    private var lastCodec: String? = null
    private val networkKindCounts = mutableMapOf<AndroidNetworkKind, Int>()
    private val wifiBandCounts = mutableMapOf<AndroidWifiBand, Int>()
    private var linkEstimateCount = 0
    private var linkEstimateTotal = 0L
    private var lowestNetworkBars: Int? = null
    private var finalSettings = launchProfile.initialSettings
    private var recoveryReason: String? = null
    private var activeMode: ActiveStreamModeStatus? = null

    fun record(stats: StreamRuntimeStats, network: AndroidRuntimeDiagnosticsSnapshot? = null) {
        if (stats.hasSessionReportValues()) {
            sampleCount += 1
        }
        stats.pingMs?.takeIf { it >= 0 }?.let { value ->
            pingCount += 1
            pingTotal += value
            peakPingMs = maxOf(peakPingMs ?: value, value)
        }
        stats.bitrateKbps?.takeIf { it >= 0 }?.let { value ->
            bitrateCount += 1
            bitrateTotal += value
            peakBitrateKbps = maxOf(peakBitrateKbps ?: value, value)
        }
        stats.jitterMs?.takeIf { it >= 0.0 }?.let { value ->
            jitterCount += 1
            jitterTotal += value
        }
        stats.fps?.takeIf { it > 0 }?.let { value ->
            fpsCount += 1
            fpsTotal += value
        }
        stats.receivedFps?.takeIf { it > 0 }?.let { value ->
            receivedFpsCount += 1
            receivedFpsTotal += value
        }
        stats.decodedFps?.takeIf { it > 0 }?.let { value ->
            decodedFpsCount += 1
            decodedFpsTotal += value
        }
        stats.decodeMs?.takeIf { it >= 0.0 }?.let { value ->
            decodeCount += 1
            decodeTotal += value
        }
        consecutiveDecoderOverloadSamples = if (
            isDecoderOverloadSample(stats, launchProfile.initialSettings.fps)
        ) {
            consecutiveDecoderOverloadSamples + 1
        } else {
            0
        }
        if (consecutiveDecoderOverloadSamples >= SESSION_REPORT_DECODER_OVERLOAD_SAMPLES) {
            decoderOverloadDetected = true
        }
        val lostDelta = stats.packetsLostDelta
        val receivedDelta = stats.packetsReceivedDelta
        if (lostDelta != null && receivedDelta != null && lostDelta >= 0L && receivedDelta >= 0L) {
            hasPacketDeltas = true
            packetsLost += lostDelta
            packetsReceived += receivedDelta
        } else {
            stats.packetLossPct?.takeIf { it >= 0.0 }?.let { value ->
                packetLossSampleCount += 1
                packetLossSampleTotal += value
            }
        }
        stats.resolution?.takeIf { parseResolutionPixelsOrNull(it) != null }?.let { lastResolution = it }
        stats.codec?.takeIf { it.isNotBlank() }?.let { lastCodec = it }
        network?.let(::recordNetwork)
    }

    fun recordRecovery(reason: String, settings: StreamSettings) {
        recoveryReason = reason.trim().takeIf { it.isNotEmpty() }
        finalSettings = settings
    }

    fun recordActiveMode(status: ActiveStreamModeStatus) {
        activeMode = status
        if (status.safeVideoRecoveryActive) {
            finalSettings = finalSettings.copy(codec = status.transportCodec)
        }
    }

    fun finish(finishedAtMs: Long): SessionReport? {
        if (sampleCount == 0) return null
        val averagePingMs = averageLong(pingTotal, pingCount)?.roundToInt()
        val averageBitrateKbps = averageLong(bitrateTotal, bitrateCount)?.roundToInt()
        val averageJitterMs = averageDouble(jitterTotal, jitterCount)
        val averageFps = averageLong(fpsTotal, fpsCount)
        val averageReceivedFps = averageLong(receivedFpsTotal, receivedFpsCount)
        val averageDecodedFps = averageLong(decodedFpsTotal, decodedFpsCount)
        val averageDecodeMs = averageDouble(decodeTotal, decodeCount)
        val packetLossPct = if (hasPacketDeltas && packetsLost + packetsReceived > 0L) {
            packetsLost.toDouble() / (packetsLost + packetsReceived).toDouble() * 100.0
        } else {
            averageDouble(packetLossSampleTotal, packetLossSampleCount)
        }
        val networkKind = dominantValue(networkKindCounts, AndroidNetworkKind.Unknown)
        val wifiBand = dominantValue(wifiBandCounts, AndroidWifiBand.Unknown)
        val estimatedLinkDownstreamKbps = averageLong(linkEstimateTotal, linkEstimateCount)?.roundToInt()
        val mode = activeMode
        val deliveredResolution = mode?.displayedResolution ?: lastResolution
        val deliveredCodec = lastCodec ?: finalSettings.codec.name
        val score = sessionQualityScore(
            averagePingMs = averagePingMs,
            packetLossPct = packetLossPct,
            averageJitterMs = averageJitterMs,
            averageFps = averageFps,
            targetFps = launchProfile.initialSettings.fps,
            averageDecodeMs = averageDecodeMs,
        )
        val downgrades = buildSessionDowngrades(
            launchProfile = launchProfile,
            finalSettings = finalSettings,
            deliveredResolution = deliveredResolution,
            deliveredCodec = deliveredCodec,
            activeMode = mode,
            recoveryReason = recoveryReason,
        )
        val recommendations = buildSessionRecommendations(
            averagePingMs = averagePingMs,
            packetLossPct = packetLossPct,
            averageJitterMs = averageJitterMs,
            averageFps = averageFps,
            averageDecodeMs = averageDecodeMs,
            targetFps = launchProfile.initialSettings.fps,
            targetBitrateMbps = launchProfile.initialSettings.maxBitrateMbps,
            averageBitrateKbps = averageBitrateKbps,
            networkKind = networkKind,
            wifiBand = wifiBand,
            estimatedLinkDownstreamKbps = estimatedLinkDownstreamKbps,
            lowestNetworkBars = lowestNetworkBars,
            averageReceivedFps = averageReceivedFps,
            averageDecodedFps = averageDecodedFps,
            decoderOverloadDetected = decoderOverloadDetected,
        )
        return SessionReport(
            gameTitle = launchProfile.gameTitle.ifBlank { "Cloud session" },
            score = score,
            rating = sessionReportRating(score),
            durationSeconds = ((finishedAtMs - startedAtMs).coerceAtLeast(0L) / 1000L)
                .coerceAtMost(Int.MAX_VALUE.toLong())
                .toInt(),
            sampleCount = sampleCount,
            limitedData = sampleCount < MIN_CONFIDENT_SESSION_REPORT_SAMPLES,
            averagePingMs = averagePingMs,
            peakPingMs = peakPingMs,
            averageBitrateKbps = averageBitrateKbps,
            peakBitrateKbps = peakBitrateKbps,
            packetLossPct = packetLossPct,
            averageJitterMs = averageJitterMs,
            averageFps = averageFps,
            targetFps = launchProfile.initialSettings.fps,
            averageDecodeMs = averageDecodeMs,
            requestedResolution = streamResolutionLabel(launchProfile.initialSettings),
            deliveredResolution = deliveredResolution,
            requestedCodec = launchProfile.initialSettings.codec,
            deliveredCodec = deliveredCodec,
            networkKind = networkKind,
            wifiBand = wifiBand,
            estimatedLinkDownstreamKbps = estimatedLinkDownstreamKbps,
            downgrades = downgrades,
            recommendations = recommendations,
        )
    }

    /** Records a completed service query without advancing media counts or overload detection. */
    fun recordNetwork(network: AndroidRuntimeDiagnosticsSnapshot) {
        networkKindCounts[network.networkKind] = (networkKindCounts[network.networkKind] ?: 0) + 1
        if (network.networkKind == AndroidNetworkKind.Wifi) {
            wifiBandCounts[network.wifiBand] = (wifiBandCounts[network.wifiBand] ?: 0) + 1
        }
        network.networkDownstreamKbps?.takeIf { it > 0 }?.let { value ->
            linkEstimateCount += 1
            linkEstimateTotal += value
        }
        network.networkSignalBars?.let { value ->
            lowestNetworkBars = minOf(lowestNetworkBars ?: value, value)
        }
    }
}

internal fun sessionQualityScore(
    averagePingMs: Int?,
    packetLossPct: Double?,
    averageJitterMs: Double?,
    averageFps: Double?,
    targetFps: Int,
    averageDecodeMs: Double?,
): Int {
    val components = buildList {
        averagePingMs?.let { add(weightedScore(latencyScore(it), 35)) }
        packetLossPct?.let { add(weightedScore(packetLossScore(it), 30)) }
        averageJitterMs?.let { add(weightedScore(jitterScore(it), 15)) }
        averageFps?.let { add(weightedScore(frameRateScore(it, targetFps), 15)) }
        averageDecodeMs?.let { add(weightedScore(decodeScore(it, targetFps, averageFps), 5)) }
    }
    if (components.isEmpty()) return 50
    val weightedTotal = components.sumOf { it.first }
    val availableWeight = components.sumOf { it.second }
    return (weightedTotal / availableWeight.toDouble()).roundToInt().coerceIn(0, 100)
}

internal fun sessionReportRating(score: Int): SessionReportRating = when {
    score >= 90 -> SessionReportRating.Excellent
    score >= 75 -> SessionReportRating.Good
    score >= 60 -> SessionReportRating.Fair
    else -> SessionReportRating.Poor
}

/**
 * A three-step reading of any single metric, coarse enough to drive a colour.
 *
 * The in-stream stats pill used to carry its own inline thresholds (ping >= 100 red, >= 50 orange;
 * loss > 1.0 red) which disagreed with the ladders below — the pill would call a session bad while
 * the report that followed it called the same session Good. There is now one opinion, expressed
 * once, here.
 */
enum class StreamQualityLevel { Good, Fair, Poor }

internal fun qualityLevelOf(score: Int): StreamQualityLevel = when {
    score >= 85 -> StreamQualityLevel.Good
    score >= 55 -> StreamQualityLevel.Fair
    else -> StreamQualityLevel.Poor
}

/** Per-metric quality readings, derived from the same ladders the session score is built from. */
object StreamQuality {
    fun latency(ms: Int): StreamQualityLevel = qualityLevelOf(latencyScore(ms))
    fun packetLoss(pct: Double): StreamQualityLevel = qualityLevelOf(packetLossScore(pct))
    fun jitter(ms: Double): StreamQualityLevel = qualityLevelOf(jitterScore(ms))
    fun decode(ms: Double, targetFps: Int, actualFps: Double? = null): StreamQualityLevel =
        qualityLevelOf(decodeScore(ms, targetFps, actualFps))
    fun frameRate(fps: Double, targetFps: Int): StreamQualityLevel = qualityLevelOf(frameRateScore(fps, targetFps))
}

internal fun latencyScore(value: Int): Int = when {
    value <= 30 -> 100
    value <= 50 -> 92
    value <= 80 -> 80
    value <= 120 -> 60
    value <= 180 -> 35
    else -> 10
}

internal fun packetLossScore(value: Double): Int = when {
    value <= 0.1 -> 100
    value <= 0.5 -> 90
    value <= 1.0 -> 75
    value <= 2.0 -> 55
    value <= 5.0 -> 25
    else -> 5
}

internal fun jitterScore(value: Double): Int = when {
    value <= 5.0 -> 100
    value <= 10.0 -> 90
    value <= 20.0 -> 70
    value <= 30.0 -> 50
    value <= 50.0 -> 25
    else -> 5
}

internal fun frameRateScore(value: Double, targetFps: Int): Int {
    val ratio = value / targetFps.coerceAtLeast(1).toDouble()
    return when {
        ratio >= 0.98 -> 100
        ratio >= 0.95 -> 95
        ratio >= 0.90 -> 82
        ratio >= 0.80 -> 60
        ratio >= 0.65 -> 35
        else -> 10
    }
}

internal fun decodeScore(value: Double, targetFps: Int, actualFps: Double? = null): Int {
    val frameBudgetMs = 1000.0 / targetFps.coerceAtLeast(1).toDouble()
    val ratio = value / frameBudgetMs
    val latencyScore = when {
        ratio <= 0.50 -> 100
        ratio <= 0.75 -> 90
        ratio <= 1.00 -> 75
        ratio <= 1.50 -> 45
        else -> 15
    }
    val throughputRatio = actualFps?.div(targetFps.coerceAtLeast(1).toDouble()) ?: return latencyScore
    // totalDecodeTime is input-to-output latency, not serial decoder throughput. Hardware decoders
    // can pipeline frames, so latency may exceed one display interval while cadence stays healthy.
    return when {
        throughputRatio >= 0.98 -> maxOf(latencyScore, 75)
        throughputRatio >= 0.90 -> maxOf(latencyScore, 55)
        else -> latencyScore
    }
}

private fun buildSessionDowngrades(
    launchProfile: StreamReportLaunchProfile,
    finalSettings: StreamSettings,
    deliveredResolution: String?,
    deliveredCodec: String?,
    activeMode: ActiveStreamModeStatus?,
    recoveryReason: String?,
): List<SessionReportFinding> = buildList {
    val selected = launchProfile.selectedSettings
    val eligible = launchProfile.eligibleSettings
    val initial = launchProfile.initialSettings
    if (
        selected.resolution != eligible.resolution ||
        selected.fps != eligible.fps ||
        selected.hdrEnabled != eligible.hdrEnabled
    ) {
        add(
            SessionReportFinding(
                title = "Account or session limit",
                detail = "Your saved ${profileSummary(selected)} profile was limited to ${profileSummary(eligible)} before launch based on the features available to this session.",
                kind = SessionReportFindingKind.Warning,
            ),
        )
    }
    if (selected.codec != eligible.codec || selected.colorQuality != eligible.colorQuality) {
        add(
            SessionReportFinding(
                title = "Android format compatibility",
                detail = "The selected ${selected.codec.name}/${selected.colorQuality.name} format was normalized to ${eligible.codec.name}/${eligible.colorQuality.name} so Android and WebRTC could decode it reliably.",
                kind = SessionReportFindingKind.Warning,
            ),
        )
    }
    if (!eligible.hasSameSessionReportProfile(initial)) {
        add(
            SessionReportFinding(
                title = "Device compatibility adjustment",
                detail = "The device probe changed ${profileSummary(eligible)} to ${profileSummary(initial)} to stay within the detected decoder and performance limits.",
                kind = SessionReportFindingKind.Warning,
            ),
        )
    }
    if (recoveryReason != null || !finalSettings.hasSameSessionReportProfile(initial)) {
        add(
            SessionReportFinding(
                title = "Safe video recovery",
                detail = buildString {
                    append("OpenNOW changed the live transport from ${profileSummary(initial)} to ${profileSummary(finalSettings)} to keep the session connected")
                    recoveryReason?.let { append(". Reason: ${it.trimEnd('.')}.") } ?: append(".")
                },
                kind = SessionReportFindingKind.Warning,
            ),
        )
    }
    val normalizedDeliveredResolution = deliveredResolution?.let(::normalizeResolutionLabel)
    val initialResolution = normalizeResolutionLabel(streamResolutionLabel(initial))
    if (
        normalizedDeliveredResolution != null &&
        normalizedDeliveredResolution != initialResolution &&
        none { it.title == "Safe video recovery" && finalSettings.resolution != initial.resolution }
    ) {
        val source = when (activeMode?.resolutionSource) {
            StreamResolutionChangeSource.ServerNegotiatedFallback -> "The cloud server negotiated"
            StreamResolutionChangeSource.ProviderOrGameModeChange -> "The provider or game switched to"
            null -> "The delivered stream used"
        }
        add(
            SessionReportFinding(
                title = "Delivered resolution changed",
                detail = "$source $normalizedDeliveredResolution instead of the requested $initialResolution. This reflects the cloud/game runtime mode, not a silent change to your saved setting.",
                kind = SessionReportFindingKind.Warning,
            ),
        )
    }
    val deliveredCodecName = deliveredCodec?.substringAfterLast('/')?.uppercase(java.util.Locale.US)
    if (
        deliveredCodecName != null &&
        !deliveredCodecName.contains(finalSettings.codec.name) &&
        recoveryReason == null
    ) {
        add(
            SessionReportFinding(
                title = "Delivered codec changed",
                detail = "WebRTC reported $deliveredCodec instead of the requested ${finalSettings.codec.name}. The negotiated transport codec determines what the device actually decoded.",
                kind = SessionReportFindingKind.Warning,
            ),
        )
    }
}

internal fun buildSessionRecommendations(
    averagePingMs: Int?,
    packetLossPct: Double?,
    averageJitterMs: Double?,
    averageFps: Double?,
    averageDecodeMs: Double?,
    targetFps: Int,
    targetBitrateMbps: Int,
    averageBitrateKbps: Int?,
    networkKind: AndroidNetworkKind,
    wifiBand: AndroidWifiBand,
    estimatedLinkDownstreamKbps: Int?,
    lowestNetworkBars: Int?,
    averageReceivedFps: Double? = null,
    averageDecodedFps: Double? = null,
    decoderOverloadDetected: Boolean = false,
): List<SessionReportFinding> = buildList {
    when {
        networkKind == AndroidNetworkKind.Wifi && wifiBand == AndroidWifiBand.TwoPointFourGhz -> add(
            SessionReportFinding(
                title = "Use 5 GHz or 6 GHz Wi-Fi",
                detail = "This session used 2.4 GHz Wi-Fi, which is usually busier and more prone to interference. Use 5/6 GHz when you are near the router; Ethernet is the most consistent option.",
                kind = SessionReportFindingKind.Warning,
            ),
        )
        networkKind == AndroidNetworkKind.Wifi &&
            wifiBand in setOf(AndroidWifiBand.FiveGhz, AndroidWifiBand.SixGhz) &&
            lowestNetworkBars != null && lowestNetworkBars <= 2 -> add(
                SessionReportFinding(
                    title = "Move closer to the Wi-Fi access point",
                    detail = "5/6 GHz can provide lower latency and more capacity, but its range is shorter. The session saw a weak signal, so reducing walls and distance may help.",
                    kind = SessionReportFindingKind.Warning,
                ),
            )
        networkKind == AndroidNetworkKind.Wifi && wifiBand == AndroidWifiBand.Unknown &&
            ((averagePingMs ?: 0) > 60 || (packetLossPct ?: 0.0) > 0.5) -> add(
                SessionReportFinding(
                    title = "Check your Wi-Fi band",
                    detail = "Android did not expose the current band. When you are near the router, prefer 5 GHz or 6 GHz over 2.4 GHz; use Ethernet for the most predictable latency.",
                    kind = SessionReportFindingKind.Warning,
                ),
            )
        networkKind == AndroidNetworkKind.Cellular -> add(
            SessionReportFinding(
                title = "Prefer Wi-Fi or Ethernet",
                detail = "Cellular latency and capacity can change quickly as signal and tower load vary. Stable 5/6 GHz Wi-Fi or Ethernet is usually better for cloud gaming.",
                kind = SessionReportFindingKind.Warning,
            ),
        )
    }
    if ((packetLossPct ?: 0.0) > 1.0) {
        add(
            SessionReportFinding(
                title = "Reduce packet loss",
                detail = "Packet loss above 1% can cause blur, stutter, or recovery events. Pause competing uploads, reduce wireless interference, or try Ethernet.",
                kind = SessionReportFindingKind.Warning,
            ),
        )
    }
    if ((averagePingMs ?: 0) > 80 || (averageJitterMs ?: 0.0) > 20.0) {
        add(
            SessionReportFinding(
                title = "Stabilize latency",
                detail = "Choose the closest available server, disable VPN routing, and pause background downloads. Consistent latency matters as much as raw download speed.",
                kind = SessionReportFindingKind.Warning,
            ),
        )
    }
    if (
        estimatedLinkDownstreamKbps != null &&
        estimatedLinkDownstreamKbps < targetBitrateMbps * STREAM_NETWORK_HEADROOM_KBPS_PER_MBPS
    ) {
        val actual = averageBitrateKbps?.let { " The stream averaged ${formatMbps(it)} Mbps." }.orEmpty()
        add(
            SessionReportFinding(
                title = "Lower the maximum bitrate",
                detail = "Android estimated about ${formatMbps(estimatedLinkDownstreamKbps)} Mbps of link capacity for a $targetBitrateMbps Mbps profile.$actual Leave headroom for network variation.",
                kind = SessionReportFindingKind.Warning,
            ),
        )
    }
    val frameBudgetMs = 1000.0 / targetFps.coerceAtLeast(1)
    val averageDecoderDeficit = averageReceivedFps != null &&
        averageDecodedFps != null &&
        averageReceivedFps >= targetFps * 0.85 &&
        averageDecodedFps <= averageReceivedFps * 0.80
    if (
        decoderOverloadDetected ||
        averageDecoderDeficit ||
        (
            (averageFps != null && averageFps < targetFps * 0.85) &&
                (averageDecodeMs ?: 0.0) > frameBudgetMs * 0.85
        )
    ) {
        add(
            SessionReportFinding(
                title = "Decoder could not keep up",
                detail = buildString {
                    append("OpenNOW detected a sustained local decoder bottleneck")
                    if (averageReceivedFps != null && averageDecodedFps != null) {
                        append(
                            ": the stream delivered ${"%.1f".format(java.util.Locale.US, averageReceivedFps)} FPS " +
                                "while the decoder produced ${"%.1f".format(java.util.Locale.US, averageDecodedFps)} FPS",
                        )
                    }
                    append(". Use H264 or the Recommended profile; this is device decode load, not the cloud game's frame rate.")
                },
                kind = SessionReportFindingKind.Warning,
            ),
        )
    }
    if (isEmpty()) {
        add(
            SessionReportFinding(
                title = "Connection looked healthy",
                detail = "No specific network or decoder issue crossed the report thresholds. Keep the same server and network setup for similarly consistent sessions.",
            ),
        )
    }
}.sortedBy { finding ->
    if (finding.title == "Decoder could not keep up") 0 else 1
}.take(MAX_SESSION_REPORT_RECOMMENDATIONS)

private const val SESSION_REPORT_DECODER_OVERLOAD_SAMPLES = 3

private fun StreamRuntimeStats.hasSessionReportValues(): Boolean =
    pingMs != null ||
        bitrateKbps != null ||
        fps != null ||
        decodeMs != null ||
        jitterMs != null ||
        packetLossPct != null

private fun weightedScore(score: Int, weight: Int): Pair<Double, Int> = score * weight.toDouble() to weight

private fun averageLong(total: Long, count: Int): Double? =
    if (count > 0) total.toDouble() / count.toDouble() else null

private fun averageDouble(total: Double, count: Int): Double? =
    if (count > 0) total / count.toDouble() else null

private fun <T> dominantValue(counts: Map<T, Int>, fallback: T): T =
    counts.maxByOrNull { it.value }?.key ?: fallback

private fun streamResolutionLabel(settings: StreamSettings): String {
    val pixels = streamResolutionPixels(settings)
    return "${pixels.first}x${pixels.second}"
}

private fun normalizeResolutionLabel(value: String): String =
    parseResolutionPixelsOrNull(value)?.let { "${it.first}x${it.second}" } ?: value

private fun profileSummary(settings: StreamSettings): String =
    buildString {
        append("${streamResolutionLabel(settings)}@${settings.fps} ${settings.codec.name}/${settings.colorQuality.name}")
        append(" ${settings.maxBitrateMbps} Mbps")
        if (settings.hdrEnabled) append(" HDR")
    }

private fun StreamSettings.hasSameSessionReportProfile(other: StreamSettings): Boolean =
    resolution == other.resolution &&
        aspectRatio == other.aspectRatio &&
        fps == other.fps &&
        maxBitrateMbps == other.maxBitrateMbps &&
        codec == other.codec &&
        colorQuality == other.colorQuality &&
        hdrEnabled == other.hdrEnabled

private fun formatMbps(kbps: Int): String =
    "%.1f".format(java.util.Locale.US, kbps.coerceAtLeast(0) / 1000.0)

private const val MIN_CONFIDENT_SESSION_REPORT_SAMPLES = 10
private const val MAX_SESSION_REPORT_RECOMMENDATIONS = 4
private const val STREAM_NETWORK_HEADROOM_KBPS_PER_MBPS = 1_200
