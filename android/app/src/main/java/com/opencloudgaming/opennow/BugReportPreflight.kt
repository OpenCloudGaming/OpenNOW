package com.opencloudgaming.opennow

import java.util.Locale
import kotlin.math.roundToInt

internal enum class BugReportPreflightTone {
    Healthy,
    Notice,
    Warning,
}

internal enum class BugReportPreflightArea {
    Experimental,
    Connection,
    VideoDevice,
    Input,
}

internal data class BugReportPreflightCard(
    val area: BugReportPreflightArea,
    val label: String,
    val title: String,
    val summary: String,
    val facts: List<String>,
    val recommendations: List<SessionReportFinding>,
    val tone: BugReportPreflightTone,
)

internal data class BugReportKnownIssueBlock(
    val key: String,
    val title: String,
    val action: String,
)

internal data class BugReportPreflightDeck(
    val cards: List<BugReportPreflightCard>,
) {
    init {
        require(cards.isNotEmpty())
    }
}

internal fun BugReportPreflightDeck.hasWarnings(additionalWarning: Boolean = false): Boolean =
    additionalWarning || cards.any { it.tone == BugReportPreflightTone.Warning }

internal fun canContinueBugReportPreflight(
    deck: BugReportPreflightDeck,
    consentChecked: Boolean,
    warningsAcknowledged: Boolean,
    additionalWarning: Boolean = false,
): Boolean = consentChecked && (!deck.hasWarnings(additionalWarning) || warningsAcknowledged)

internal data class BugReportPreflightEvidence(
    val requestedSettings: StreamSettings,
    val recommendedSettings: StreamSettings? = null,
    val nativeLowLatencyDecoderEnabled: Boolean = false,
    val runtimeStats: StreamRuntimeStats = StreamRuntimeStats(),
    val runtimeDiagnostics: AndroidRuntimeDiagnosticsSnapshot = AndroidRuntimeDiagnosticsSnapshot(),
    val sessionReport: SessionReport? = null,
    val deliveredResolution: String? = null,
    val deliveredCodec: String? = null,
    val codecReport: RuntimeCodecReport? = null,
    val androidTvProfile: Boolean = false,
    val serverZone: String? = null,
    val manuallySelectedServer: Boolean = false,
    val inputDiagnostics: String = "",
)

internal fun buildBugReportPreflightDeck(
    evidence: BugReportPreflightEvidence,
): BugReportPreflightDeck {
    val report = evidence.sessionReport
    val networkKind = report?.networkKind
        ?.takeUnless { it == AndroidNetworkKind.Unknown }
        ?: evidence.runtimeDiagnostics.networkKind
    val wifiBand = report?.wifiBand
        ?.takeUnless { it == AndroidWifiBand.Unknown }
        ?: evidence.runtimeDiagnostics.wifiBand
    val pingMs = report?.averagePingMs ?: evidence.runtimeStats.pingMs
    val packetLossPct = report?.packetLossPct ?: evidence.runtimeStats.packetLossPct
    val averageJitterMs = report?.averageJitterMs ?: evidence.runtimeStats.jitterMs
    val averageFps = report?.averageFps ?: evidence.runtimeStats.fps?.toDouble()
    val averageDecodeMs = report?.averageDecodeMs ?: evidence.runtimeStats.decodeMs
    val averageReceivedFps = evidence.runtimeStats.receivedFps?.toDouble()
    val averageDecodedFps = evidence.runtimeStats.decodedFps?.toDouble()
    val averageBitrateKbps = report?.averageBitrateKbps ?: evidence.runtimeStats.bitrateKbps
    val downstreamKbps = report?.estimatedLinkDownstreamKbps
        ?: evidence.runtimeDiagnostics.networkDownstreamKbps
    val recommendations = report?.recommendations ?: buildSessionRecommendations(
        averagePingMs = pingMs,
        packetLossPct = packetLossPct,
        averageJitterMs = averageJitterMs,
        averageFps = averageFps,
        averageDecodeMs = averageDecodeMs,
        targetFps = evidence.requestedSettings.fps,
        targetBitrateMbps = evidence.requestedSettings.maxBitrateMbps,
        averageBitrateKbps = averageBitrateKbps,
        networkKind = networkKind,
        wifiBand = wifiBand,
        estimatedLinkDownstreamKbps = downstreamKbps,
        lowestNetworkBars = evidence.runtimeDiagnostics.networkSignalBars,
        averageReceivedFps = averageReceivedFps,
        averageDecodedFps = averageDecodedFps,
        decoderOverloadDetected = isDecoderOverloadSample(
            evidence.runtimeStats,
            evidence.requestedSettings.fps,
        ),
    )
    val deviceRecommendationTitles = setOf("Reduce device decode load", "Decoder could not keep up")
    val networkRecommendations = recommendations.filterNot { it.title in deviceRecommendationTitles }
    val deviceRecommendations = recommendations.filter { it.title in deviceRecommendationTitles }

    return BugReportPreflightDeck(
        cards = buildList {
            if (evidence.nativeLowLatencyDecoderEnabled) {
                add(buildExperimentalNativeStreamerPreflightCard())
            }
            add(
                buildConnectionPreflightCard(
                    networkKind = networkKind,
                    wifiBand = wifiBand,
                    signalBars = evidence.runtimeDiagnostics.networkSignalBars,
                    downstreamKbps = downstreamKbps,
                    pingMs = pingMs,
                    packetLossPct = packetLossPct,
                    jitterMs = averageJitterMs,
                    serverZone = evidence.serverZone,
                    manuallySelectedServer = evidence.manuallySelectedServer,
                    recommendations = networkRecommendations,
                ),
            )
            add(
                buildVideoDevicePreflightCard(
                    evidence = evidence,
                    averageFps = averageFps,
                    averageBitrateKbps = averageBitrateKbps,
                    recommendations = deviceRecommendations,
                ),
            )
            add(buildInputPreflightCard(evidence.inputDiagnostics))
        },
    )
}

private fun buildExperimentalNativeStreamerPreflightCard(): BugReportPreflightCard =
    BugReportPreflightCard(
        area = BugReportPreflightArea.Experimental,
        label = "EXPERIMENTAL FEATURE DETECTED",
        title = "Native streamer is enabled",
        summary =
            "Native streamer changes the hardware decoder with experimental vendor settings. " +
                "Turn it off and reproduce first, or explicitly acknowledge sending anyway.",
        facts = listOf(
            "Native streamer (Experimental): On",
            "Unsupported report configuration",
        ),
        recommendations = listOf(
            SessionReportFinding(
                title = "Turn it off and reproduce the issue again",
                detail =
                    "Disable Native streamer (Experimental), restart the stream, and reproduce the problem before sending a bug report.",
                kind = SessionReportFindingKind.Warning,
            ),
        ),
        tone = BugReportPreflightTone.Warning,
    )

private fun buildConnectionPreflightCard(
    networkKind: AndroidNetworkKind,
    wifiBand: AndroidWifiBand,
    signalBars: Int?,
    downstreamKbps: Int?,
    pingMs: Int?,
    packetLossPct: Double?,
    jitterMs: Double?,
    serverZone: String?,
    manuallySelectedServer: Boolean,
    recommendations: List<SessionReportFinding>,
): BugReportPreflightCard {
    val facts = buildList {
        add(
            when (networkKind) {
                AndroidNetworkKind.Wifi -> wifiBand.label
                else -> networkKind.label
            },
        )
        signalBars?.let { add("Signal ${it.coerceIn(0, 4)}/4") }
        pingMs?.takeIf { it >= 0 }?.let { add("$it ms latency") }
        packetLossPct?.takeIf { it >= 0.0 }?.let {
            add("${"%.2f".format(Locale.US, it)}% loss")
        }
        jitterMs?.takeIf { it >= 0.0 }?.let {
            add("${"%.1f".format(Locale.US, it)} ms jitter")
        }
        downstreamKbps?.takeIf { it > 0 }?.let { add("~${formatPreflightMbps(it)} Mbps link") }
        serverZone?.trim()?.takeIf { it.isNotEmpty() }?.let { add("Server $it") }
        if (manuallySelectedServer) add("Manual server selection")
    }
    val warningRecommendations = buildList {
        if (manuallySelectedServer) {
            add(
                SessionReportFinding(
                    title = MANUAL_SERVER_SELECTION_ACTION_TITLE,
                    detail =
                        "Switch Region to Auto, start a new cloud session, and reproduce the issue. " +
                            "Reports captured while a server is manually selected may not be investigated because the selected route can cause the reported symptoms.",
                    kind = SessionReportFindingKind.Warning,
                ),
            )
        }
        addAll(recommendations.filter { it.kind == SessionReportFindingKind.Warning })
    }.distinctBy { it.title }
    val hasMeasurements = facts.size > 1 || networkKind !in setOf(AndroidNetworkKind.Unknown, AndroidNetworkKind.Other)
    val tone = when {
        warningRecommendations.isNotEmpty() || networkKind == AndroidNetworkKind.None -> BugReportPreflightTone.Warning
        hasMeasurements -> BugReportPreflightTone.Healthy
        else -> BugReportPreflightTone.Notice
    }
    val title = when (tone) {
        BugReportPreflightTone.Warning -> if (manuallySelectedServer) {
            "A manually selected server may explain the issue"
        } else {
            "The connection may explain the issue"
        }
        BugReportPreflightTone.Healthy -> "The connection looks healthy"
        BugReportPreflightTone.Notice -> "Connection evidence is limited"
    }
    val summary = when {
        manuallySelectedServer ->
            "Reports from manually selected servers may not be investigated. Reproduce on Auto so server routing is ruled out first."
        warningRecommendations.isNotEmpty() ->
            "These suggestions are based on this session's measured network, not generic Wi-Fi advice."
        networkKind == AndroidNetworkKind.None ->
            "Android did not detect an active connection when this check ran."
        hasMeasurements ->
            "Nothing in the measured connection crossed the current warning thresholds."
        else ->
            "Android did not expose enough live network data to make a specific recommendation."
    }
    return BugReportPreflightCard(
        area = BugReportPreflightArea.Connection,
        label = "CONNECTION CHECK",
        title = title,
        summary = summary,
        facts = facts,
        recommendations = warningRecommendations,
        tone = tone,
    )
}

private fun buildVideoDevicePreflightCard(
    evidence: BugReportPreflightEvidence,
    averageFps: Double?,
    averageBitrateKbps: Int?,
    recommendations: List<SessionReportFinding>,
): BugReportPreflightCard {
    val report = evidence.sessionReport
    val requestedResolution = report?.requestedResolution ?: streamResolutionLabelForPreflight(evidence.requestedSettings)
    val deliveredResolution = report?.deliveredResolution
        ?: evidence.deliveredResolution
        ?: evidence.runtimeStats.resolution
    val deliveredCodec = report?.deliveredCodec
        ?: evidence.deliveredCodec
        ?: evidence.runtimeStats.codec
        ?: evidence.requestedSettings.codec.name
    val deliveredCodecType = deliveredCodec.toVideoCodecOrNull()
    val decoderCapability = deliveredCodecType?.let { codec ->
        evidence.codecReport?.capabilities?.firstOrNull { it.codec == codec }
    }
    val hardwareDecoder = decoderCapability?.streamingHardwareDecoderAvailable()
    val thermalStatus = evidence.runtimeDiagnostics.thermalStatus
    val resolutionChanged = deliveredResolution != null &&
        parseResolutionPixelsOrNull(deliveredResolution) != parseResolutionPixelsOrNull(requestedResolution)
    val codecChanged = deliveredCodecType != null && deliveredCodecType != evidence.requestedSettings.codec
    val thermalWarning = thermalStatus in setOf(
        AndroidThermalStatus.Moderate,
        AndroidThermalStatus.Severe,
        AndroidThermalStatus.Critical,
        AndroidThermalStatus.Emergency,
        AndroidThermalStatus.Shutdown,
    )
    val recommendationOverrides = evidence.requestedSettings.performanceOverridesComparedTo(
        recommended = evidence.recommendedSettings,
        report = evidence.codecReport,
    )
    val videoRecommendations = buildList {
        addAll(recommendations.filter { it.kind == SessionReportFindingKind.Warning })
        if (recommendationOverrides.isNotEmpty()) {
            val recommended = requireNotNull(evidence.recommendedSettings)
            add(
                SessionReportFinding(
                    title = DEVICE_RECOMMENDATION_ACTION_TITLE,
                    detail =
                        "Switch to Recommended (${recommended.recommendationSummary()}), restart the stream, and reproduce the lag before reporting it. " +
                            "This session used ${recommendationOverrides.joinToString()}.",
                    kind = SessionReportFindingKind.Warning,
                ),
            )
        }
        if (thermalWarning) {
            add(
                SessionReportFinding(
                    title = "Let the device cool down",
                    detail = "Android reports ${thermalStatus.logValue} thermal pressure. Heat can reduce decode speed and frame delivery, so retry after the device cools or improve ventilation.",
                    kind = SessionReportFindingKind.Warning,
                ),
            )
        }
        if (hardwareDecoder == false) {
            val hardwareFallback = evidence.codecReport?.capabilities
                ?.firstOrNull { it.streamingHardwareDecoderAvailable() }
                ?.codec
            add(
                SessionReportFinding(
                    title = "Use a hardware-decoded codec",
                    detail = hardwareFallback?.let {
                        "$deliveredCodec is not using a hardware decoder on this device. Try ${it.name} for lower decode load."
                    } ?: "$deliveredCodec is not using a hardware decoder on this device. A lower resolution or frame rate may be more reliable.",
                    kind = SessionReportFindingKind.Warning,
                ),
            )
        }
    }.distinctBy { it.title }
    val facts = buildList {
        add("Requested $requestedResolution@${evidence.requestedSettings.fps}")
        add("Requested max ${evidence.requestedSettings.maxBitrateMbps} Mbps")
        evidence.recommendedSettings?.let { add("Detected Recommended ${it.recommendationSummary()}") }
        if (recommendationOverrides.isNotEmpty()) {
            add("Above recommendation: ${recommendationOverrides.joinToString()}")
        }
        deliveredResolution?.let { add("Delivered $it") }
        add("Codec $deliveredCodec")
        hardwareDecoder?.let { add(if (it) "Hardware decoder" else "Software decoder") }
        averageFps?.let { add("${it.roundToInt()} FPS average") }
        averageBitrateKbps?.takeIf { it >= 0 }?.let { add("${formatPreflightMbps(it)} Mbps video") }
        evidence.runtimeStats.availableIncomingBitrateKbps
            ?.takeIf { it >= 0 }
            ?.let { add("${formatPreflightMbps(it)} Mbps WebRTC receive estimate") }
        when {
            evidence.codecReport?.constrainedRuntimeProfile == true -> add("Constrained device profile")
            evidence.codecReport?.lowPowerGpuProfile == true -> add("Low-power device profile")
            evidence.androidTvProfile -> add("Android TV profile")
            else -> add("Android device profile")
        }
        if (thermalStatus != AndroidThermalStatus.Unknown) add("Thermal ${thermalStatus.logValue}")
    }
    val tone = when {
        videoRecommendations.isNotEmpty() || resolutionChanged || codecChanged -> BugReportPreflightTone.Warning
        deliveredResolution == null && averageFps == null -> BugReportPreflightTone.Notice
        else -> BugReportPreflightTone.Healthy
    }
    val title = when {
        resolutionChanged || codecChanged -> "The delivered stream changed"
        recommendationOverrides.isNotEmpty() -> "Selected settings exceed the device recommendation"
        tone == BugReportPreflightTone.Warning -> "The device or decoder needs attention"
        tone == BugReportPreflightTone.Healthy -> "The video path looks healthy"
        else -> "Video evidence is limited"
    }
    val summary = when {
        resolutionChanged || codecChanged ->
            "The requested and delivered profiles differ. That difference will be included in the report automatically."
        recommendationOverrides.isNotEmpty() ->
            "OpenNOW detected a safer profile for this hardware. Higher settings can add decoder, GPU, or network load, so reproduce with Recommended before assigning the lag to the app."
        videoRecommendations.isNotEmpty() ->
            "Only checks that match the current decoder and thermal state are shown below."
        tone == BugReportPreflightTone.Healthy ->
            "The detected decoder, delivered profile, and device state do not show an obvious local bottleneck."
        else ->
            "Reproduce the visual issue while the stream is active so the report can capture delivery data."
    }
    return BugReportPreflightCard(
        area = BugReportPreflightArea.VideoDevice,
        label = "VIDEO + DEVICE",
        title = title,
        summary = summary,
        facts = facts,
        recommendations = videoRecommendations,
        tone = tone,
    )
}

private fun buildInputPreflightCard(inputDiagnostics: String): BugReportPreflightCard {
    val paths = buildList {
        if (inputDiagnostics.contains("external mouse", ignoreCase = true)) add("External mouse")
        if (inputDiagnostics.contains("hardware keyboard", ignoreCase = true)) add("Hardware keyboard")
        if (
            inputDiagnostics.contains("physical gamepad connected=true", ignoreCase = true) ||
            inputDiagnostics.contains("physical gamepad motion", ignoreCase = true) ||
            inputDiagnostics.contains("physical gamepad axes", ignoreCase = true) ||
            inputDiagnostics.contains("physical gamepad analog", ignoreCase = true) ||
            inputDiagnostics.contains("physical gamepad key", ignoreCase = true)
        ) {
            add("Physical gamepad")
        }
        if (inputDiagnostics.contains("controller mouse", ignoreCase = true)) add("Controller mouse")
        if (inputDiagnostics.contains("touch mouse", ignoreCase = true)) {
            add("Touch / Finger Mouse")
        }
    }.distinct()
    val reliableOpen = inputDiagnostics.contains("input channel open label=input_channel_v1", ignoreCase = true)
    val partialOpen = inputDiagnostics.contains("input channel open label=input_channel_partially_reliable", ignoreCase = true)
    val successfulMouseSend = inputDiagnostics.contains("external mouse move sent", ignoreCase = true) ||
        inputDiagnostics.contains("controller mouse move sent", ignoreCase = true)
    val droppedWithoutChannel = inputDiagnostics.contains("input dropped noOpenChannel", ignoreCase = true)
    val facts = buildList {
        if (paths.isNotEmpty()) add("Detected ${paths.joinToString()}")
        if (reliableOpen && partialOpen) add("Input channels opened")
        else if (reliableOpen) add("Reliable input opened")
        if (successfulMouseSend) add("Mouse movement sent")
        if (droppedWithoutChannel) add("A no-channel drop was recorded")
    }
    val recommendations = when {
        paths.isEmpty() -> listOf(
            SessionReportFinding(
                title = "For an input problem, reproduce it once",
                detail = "No recent mouse, keyboard, gamepad, or touch event is present. If the report is about input, move or press the affected control first so the attached diagnostics identify the real path. For other problems, continue as normal.",
            ),
        )
        droppedWithoutChannel && !reliableOpen -> listOf(
            SessionReportFinding(
                title = "Wait for the input channel to reconnect",
                detail = "The recent input event arrived while no cloud input channel was open. Reproduce after the stream reconnects; if it still fails, continue with the report.",
                kind = SessionReportFindingKind.Warning,
            ),
        )
        else -> emptyList()
    }
    val tone = when {
        recommendations.any { it.kind == SessionReportFindingKind.Warning } -> BugReportPreflightTone.Warning
        paths.isNotEmpty() && (reliableOpen || successfulMouseSend) -> BugReportPreflightTone.Healthy
        else -> BugReportPreflightTone.Notice
    }
    return BugReportPreflightCard(
        area = BugReportPreflightArea.Input,
        label = "INPUT EVIDENCE",
        title = when (tone) {
            BugReportPreflightTone.Warning -> "Capture the affected input first"
            BugReportPreflightTone.Healthy -> "The input path was detected"
            BugReportPreflightTone.Notice -> "No recent input was detected"
        },
        summary = when {
            recommendations.any { it.kind == SessionReportFindingKind.Warning } ->
                "The report is more useful after the exact control has been moved or pressed during the failure."
            successfulMouseSend ->
                "Recent diagnostics show mouse movement reached the cloud input path."
            paths.isNotEmpty() ->
                "The attached diagnostics identify the active control type without guessing."
            else ->
                "This only matters for mouse, keyboard, gamepad, or touch reports; other reports can continue."
        },
        facts = facts,
        recommendations = recommendations,
        tone = tone,
    )
}

internal fun bugReportKnownIssueBlock(
    title: String,
    description: String,
    deck: BugReportPreflightDeck,
): BugReportKnownIssueBlock? {
    val reportText = "$title $description".lowercase(Locale.ROOT)
    val experimental = deck.cards.firstOrNull {
        it.area == BugReportPreflightArea.Experimental && it.tone == BugReportPreflightTone.Warning
    }
    if (experimental != null) {
        return BugReportKnownIssueBlock(
            key = "experimental-native-streamer",
            title = "Turn off Native streamer first",
            action = "Restart the stream with Native streamer off, then reproduce the issue.",
        ).withRepeatedLagReportWarning(reportText)
    }

    val connection = deck.cards.firstOrNull {
        it.area == BugReportPreflightArea.Connection && it.tone == BugReportPreflightTone.Warning
    }
    val manualServerSelection = connection?.recommendations?.any {
        it.title == MANUAL_SERVER_SELECTION_ACTION_TITLE
    } == true
    if (
        connection != null &&
        manualServerSelection &&
        reportText.containsAnyWholeTerm(BUG_REPORT_MANUAL_SERVER_SYMPTOM_PATTERNS)
    ) {
        return BugReportKnownIssueBlock(
            key = "network-manual-server",
            title = "Manual server selection must be ruled out",
            action =
                "Switch Region to Auto, start a new cloud session, and reproduce the issue. " +
                    "Reports from manually selected servers may not be investigated.",
        ).withRepeatedLagReportWarning(reportText)
    }
    if (connection != null && reportText.containsAnyWholeTerm(BUG_REPORT_NETWORK_SYMPTOM_PATTERNS)) {
        val twoPointFourGhz = connection.facts.any { it.contains("2.4 GHz", ignoreCase = true) }
        return BugReportKnownIssueBlock(
            key = if (twoPointFourGhz) "network-2.4ghz" else "network-measured",
            title = if (twoPointFourGhz) "2.4 GHz likely explains this" else "Connection issue detected",
            action = if (twoPointFourGhz) {
                "Use 5/6 GHz Wi-Fi, Ethernet, or stable cellular, then try again."
            } else {
                connection.recommendations.firstOrNull()?.compactPreflightAction()
                    ?: "Fix the measured connection warning, then reproduce the issue."
            },
        ).withRepeatedLagReportWarning(reportText)
    }

    val video = deck.cards.firstOrNull {
        it.area == BugReportPreflightArea.VideoDevice && it.tone == BugReportPreflightTone.Warning
    }
    if (video != null && reportText.containsAnyWholeTerm(BUG_REPORT_VIDEO_SYMPTOM_PATTERNS)) {
        val recommendationOverride = video.recommendations.firstOrNull {
            it.title == DEVICE_RECOMMENDATION_ACTION_TITLE
        }
        return BugReportKnownIssueBlock(
            key = if (recommendationOverride != null) "device-profile-override" else "video-device-measured",
            title = if (recommendationOverride != null) {
                "Selected profile exceeds this device's recommendation"
            } else {
                "Local video issue detected"
            },
            action = recommendationOverride?.compactPreflightAction()
                ?: video.recommendations.firstOrNull()?.compactPreflightAction()
                ?: "Apply the video or device fix shown above, then reproduce the issue.",
        ).withRepeatedLagReportWarning(reportText)
    }

    val input = deck.cards.firstOrNull {
        it.area == BugReportPreflightArea.Input && it.tone == BugReportPreflightTone.Warning
    }
    if (input != null && reportText.containsAnyWholeTerm(BUG_REPORT_INPUT_SYMPTOM_PATTERNS)) {
        return BugReportKnownIssueBlock(
            key = "input-measured",
            title = "Input path issue detected",
            action = input.recommendations.firstOrNull()?.compactPreflightAction()
                ?: "Reconnect the input path and reproduce the issue before reporting.",
        ).withRepeatedLagReportWarning(reportText)
    }
    return null
}

private fun BugReportKnownIssueBlock.withRepeatedLagReportWarning(reportText: String): BugReportKnownIssueBlock {
    if (!reportText.containsAnyWholeTerm(BUG_REPORT_EXACT_LAG_PATTERN)) return this
    return copy(
        action = action.trimEnd() +
            " Repeating a lag report after this cause has already been identified, without first trying the requested step, may result in a bug-reporting ban.",
    )
}

internal fun bugReportKnownIssueAllowsSubmission(
    block: BugReportKnownIssueBlock?,
    acknowledgedBlockKey: String?,
): Boolean = block == null || block.key == acknowledgedBlockKey

private fun String.containsAnyWholeTerm(patterns: List<Regex>): Boolean = patterns.any { it.containsMatchIn(this) }

private fun SessionReportFinding.compactPreflightAction(): String {
    val firstSentence = detail.trim().substringBefore('.').trim()
    return if (firstSentence.isNotEmpty()) "$firstSentence." else title
}

private fun bugReportTermPatterns(vararg terms: String): List<Regex> = terms.map { term ->
    Regex("(^|[^a-z0-9])${Regex.escape(term)}([^a-z0-9]|$)")
}

private val BUG_REPORT_NETWORK_SYMPTOM_PATTERNS = bugReportTermPatterns(
    "lag", "laggy", "latency", "ping", "delay", "delayed", "jitter", "packet loss",
    "buffering", "stutter", "stuttering", "choppy", "pixelated", "blurry", "slow",
)

private val BUG_REPORT_EXACT_LAG_PATTERN = bugReportTermPatterns("lag")

private val BUG_REPORT_VIDEO_SYMPTOM_PATTERNS = bugReportTermPatterns(
    "fps", "frame", "frames", "video", "decoder", "decode", "freeze", "frozen", "blurry",
    "pixelated", "stutter", "stuttering", "choppy", "lag", "laggy", "slow", "overheat", "hot",
)

private val BUG_REPORT_INPUT_SYMPTOM_PATTERNS = bugReportTermPatterns(
    "input", "mouse", "keyboard", "controller", "gamepad", "touch", "button", "joystick",
    "stick", "click", "cursor",
)

private val BUG_REPORT_MANUAL_SERVER_SYMPTOM_PATTERNS =
    BUG_REPORT_NETWORK_SYMPTOM_PATTERNS + BUG_REPORT_VIDEO_SYMPTOM_PATTERNS

private const val MANUAL_SERVER_SELECTION_ACTION_TITLE = "Use Auto server selection and reproduce"
private const val DEVICE_RECOMMENDATION_ACTION_TITLE = "Use the detected Recommended profile"

private fun streamResolutionLabelForPreflight(settings: StreamSettings): String {
    val (width, height) = streamResolutionPixels(settings)
    return "${width}x$height"
}

private fun String.toVideoCodecOrNull(): VideoCodec? {
    val normalized = uppercase(Locale.US).replace(".", "").replace("-", "")
    return when {
        "AV1" in normalized -> VideoCodec.AV1
        "H265" in normalized || "HEVC" in normalized -> VideoCodec.H265
        "H264" in normalized || "AVC" in normalized -> VideoCodec.H264
        else -> null
    }
}

private fun formatPreflightMbps(kbps: Int): String =
    "%.1f".format(Locale.US, kbps / 1000.0).removeSuffix(".0")
