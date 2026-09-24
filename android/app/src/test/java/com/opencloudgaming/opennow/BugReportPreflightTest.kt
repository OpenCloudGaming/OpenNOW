package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class BugReportPreflightTest {
    private val settings = StreamSettings(
        resolution = "1920x1080",
        aspectRatio = "16:9",
        fps = 60,
        maxBitrateMbps = 35,
        codec = VideoCodec.H264,
    )

    @Test
    fun reportFormRequiresConsentAndAcknowledgementOnlyWhenWarningsExist() {
        val healthy = BugReportPreflightDeck(listOf(
            BugReportPreflightCard(
                area = BugReportPreflightArea.Connection,
                label = "Connection",
                title = "Healthy",
                summary = "No warning",
                facts = emptyList(),
                recommendations = emptyList(),
                tone = BugReportPreflightTone.Healthy,
            ),
        ))
        val warning = BugReportPreflightDeck(healthy.cards + healthy.cards.first().copy(tone = BugReportPreflightTone.Warning))

        assertFalse(healthy.hasWarnings())
        assertFalse(canContinueBugReportPreflight(healthy, consentChecked = false, warningsAcknowledged = false))
        assertTrue(canContinueBugReportPreflight(healthy, consentChecked = true, warningsAcknowledged = false))
        assertTrue(warning.hasWarnings())
        assertFalse(canContinueBugReportPreflight(warning, consentChecked = false, warningsAcknowledged = true))
        assertFalse(canContinueBugReportPreflight(warning, consentChecked = true, warningsAcknowledged = false))
        assertTrue(canContinueBugReportPreflight(warning, consentChecked = true, warningsAcknowledged = true))
        assertFalse(canContinueBugReportPreflight(healthy, consentChecked = true, warningsAcknowledged = false, additionalWarning = true))
        assertTrue(canContinueBugReportPreflight(healthy, consentChecked = true, warningsAcknowledged = true, additionalWarning = true))
    }

    @Test
    fun detectsManualServerFromSelectorOrConfiguredRegion() {
        assertFalse(manuallySelectedServerForReport(null, ""))
        assertFalse(manuallySelectedServerForReport("", ""))
        assertTrue(manuallySelectedServerForReport("https://np-lon-06.example", ""))
        assertTrue(manuallySelectedServerForReport(null, "https://np-ams-06.example"))
    }

    @Test
    fun experimentalNativeStreamerWarningIsFirstAndExplicitlyUnsupported() {
        val deck = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(
                requestedSettings = settings,
                nativeLowLatencyDecoderEnabled = true,
            ),
        )

        val warning = deck.cards.first()
        assertEquals(4, deck.cards.size)
        assertEquals(BugReportPreflightTone.Warning, warning.tone)
        assertEquals("EXPERIMENTAL FEATURE DETECTED", warning.label)
        assertTrue(warning.summary.contains("explicitly acknowledge sending anyway"))
        assertTrue(warning.facts.contains("Native streamer (Experimental): On"))
        assertEquals("Turn it off and reproduce the issue again", warning.recommendations.single().title)
    }

    @Test
    fun standardPreflightDoesNotShowExperimentalNativeStreamerWarning() {
        val deck = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(requestedSettings = settings),
        )

        assertEquals(3, deck.cards.size)
        assertFalse(deck.cards.any { it.label == "EXPERIMENTAL FEATURE DETECTED" })
    }

    @Test
    fun healthySixGhzDoesNotSuggestChangingWifiBand() {
        val deck = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(
                requestedSettings = settings,
                runtimeStats = StreamRuntimeStats(
                    bitrateKbps = 28_000,
                    availableIncomingBitrateKbps = 74_000,
                    pingMs = 24,
                    fps = 60,
                    resolution = "1920x1080",
                    codec = "H264",
                    jitterMs = 3.0,
                    packetLossPct = 0.05,
                ),
                runtimeDiagnostics = AndroidRuntimeDiagnosticsSnapshot(
                    thermalStatus = AndroidThermalStatus.None,
                    networkKind = AndroidNetworkKind.Wifi,
                    networkSignalBars = 4,
                    networkDownstreamKbps = 200_000,
                    wifiFrequencyMhz = 6_115,
                    wifiBand = AndroidWifiBand.SixGhz,
                ),
            ),
        )

        val connection = deck.cards.first()
        assertEquals(BugReportPreflightTone.Healthy, connection.tone)
        assertTrue(connection.facts.contains("6 GHz"))
        assertTrue(connection.recommendations.isEmpty())
        assertFalse(connection.toString().contains("Use 5 GHz"))
        assertFalse(connection.toString().contains("2.4 GHz Wi-Fi"))
        assertEquals(
            null,
            bugReportKnownIssueBlock(
                title = "High ping and lag",
                description = "The game has high latency even though the connection looks normal during this session.",
                deck = deck,
            ),
        )
        val video = deck.cards[1]
        assertTrue(video.facts.contains("Requested max 35 Mbps"))
        assertTrue(video.facts.contains("28 Mbps video"))
        assertTrue(video.facts.contains("74 Mbps WebRTC receive estimate"))
    }

    @Test
    fun runtimeBitrateStatusShowsActualAndRequestedMaximum() {
        assertEquals("28.4 Mbps / 35 Mbps max", formatRuntimeBitrateStatus(28_400, 35))
        assertEquals("-- / 35 Mbps max", formatRuntimeBitrateStatus(null, 35))
    }

    @Test
    fun degradedTwoPointFourGhzShowsOnlyMatchedNetworkActions() {
        val deck = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(
                requestedSettings = settings,
                runtimeStats = StreamRuntimeStats(
                    bitrateKbps = 2_000,
                    pingMs = 155,
                    fps = 42,
                    resolution = "1280x720",
                    codec = "H264",
                    jitterMs = 28.0,
                    packetLossPct = 2.2,
                ),
                runtimeDiagnostics = AndroidRuntimeDiagnosticsSnapshot(
                    thermalStatus = AndroidThermalStatus.None,
                    networkKind = AndroidNetworkKind.Wifi,
                    networkSignalBars = 2,
                    networkDownstreamKbps = 12_000,
                    wifiFrequencyMhz = 2_412,
                    wifiBand = AndroidWifiBand.TwoPointFourGhz,
                ),
            ),
        )

        val connection = deck.cards.first()
        val titles = connection.recommendations.map { it.title }
        assertEquals(BugReportPreflightTone.Warning, connection.tone)
        assertTrue(titles.contains("Use 5 GHz or 6 GHz Wi-Fi"))
        assertTrue(titles.contains("Reduce packet loss"))
        assertTrue(titles.contains("Stabilize latency"))
        assertTrue(titles.contains("Lower the maximum bitrate"))
        val block = requireNotNull(
            bugReportKnownIssueBlock(
                title = "High ping and lag",
                description = "The stream feels delayed and stutters while I am playing over this connection.",
                deck = deck,
            ),
        )
        assertEquals("network-2.4ghz", block.key)
        assertTrue(block.action.contains("5/6 GHz"))
        assertTrue(block.action.contains("cellular"))
        assertTrue(block.action.contains("may result in a bug-reporting ban"))
        assertFalse(bugReportKnownIssueAllowsSubmission(block, null))
        assertTrue(bugReportKnownIssueAllowsSubmission(block, block.key))
        assertEquals(
            null,
            bugReportKnownIssueBlock(
                title = "Flag icon is incorrect",
                description = "The country flag icon has the wrong colors after opening the settings page.",
                deck = deck,
            ),
        )
    }

    @Test
    fun possibleBanWarningRequiresExactLagWordAndMeasuredCause() {
        val degradedDeck = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(
                requestedSettings = settings,
                runtimeStats = StreamRuntimeStats(pingMs = 150, packetLossPct = 3.0),
                runtimeDiagnostics = AndroidRuntimeDiagnosticsSnapshot(
                    networkKind = AndroidNetworkKind.Wifi,
                    wifiBand = AndroidWifiBand.TwoPointFourGhz,
                ),
            ),
        )
        val lagBlock = requireNotNull(
            bugReportKnownIssueBlock("Lag", "The stream has lag.", degradedDeck),
        )
        assertTrue(lagBlock.action.contains("bug-reporting ban"))

        val laggyBlock = requireNotNull(
            bugReportKnownIssueBlock("Laggy stream", "The stream stutters.", degradedDeck),
        )
        assertFalse(laggyBlock.action.contains("bug-reporting ban"))

        val healthyDeck = buildBugReportPreflightDeck(BugReportPreflightEvidence(requestedSettings = settings))
        assertEquals(null, bugReportKnownIssueBlock("Lag", "The stream has lag.", healthyDeck))
    }

    @Test
    fun ethernetSessionNeverGetsWifiAdvice() {
        val deck = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(
                requestedSettings = settings,
                runtimeStats = StreamRuntimeStats(
                    bitrateKbps = 30_000,
                    pingMs = 18,
                    fps = 60,
                    packetLossPct = 0.0,
                ),
                runtimeDiagnostics = AndroidRuntimeDiagnosticsSnapshot(
                    networkKind = AndroidNetworkKind.Ethernet,
                    networkDownstreamKbps = 500_000,
                ),
            ),
        )

        val connection = deck.cards.first()
        assertEquals(BugReportPreflightTone.Healthy, connection.tone)
        assertTrue(connection.facts.contains("LAN"))
        assertTrue(connection.recommendations.isEmpty())
        assertFalse(connection.toString().contains("Wi-Fi"))
    }

    @Test
    fun videoAdviceMatchesActualDecoderAndThermalState() {
        val deck = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(
                requestedSettings = settings.copy(codec = VideoCodec.H265),
                runtimeStats = StreamRuntimeStats(
                    bitrateKbps = 18_000,
                    pingMs = 28,
                    fps = 30,
                    resolution = "1920x1080",
                    codec = "H265",
                    decodeMs = 15.5,
                    packetLossPct = 0.0,
                ),
                runtimeDiagnostics = AndroidRuntimeDiagnosticsSnapshot(
                    thermalStatus = AndroidThermalStatus.Severe,
                    networkKind = AndroidNetworkKind.Ethernet,
                    networkDownstreamKbps = 500_000,
                ),
                codecReport = RuntimeCodecReport(
                    capabilities = listOf(
                        CodecCapability(
                            codec = VideoCodec.H265,
                            decoderAvailable = true,
                            encoderAvailable = false,
                            hardwareDecoder = false,
                            hardwareEncoder = false,
                            webRtcDecoderAvailable = true,
                            webRtcHardwareDecoderAvailable = false,
                        ),
                        CodecCapability(
                            codec = VideoCodec.H264,
                            decoderAvailable = true,
                            encoderAvailable = false,
                            hardwareDecoder = true,
                            hardwareEncoder = false,
                            webRtcDecoderAvailable = true,
                            webRtcHardwareDecoderAvailable = true,
                        ),
                    ),
                    nativeRuntimeSummary = "test",
                    androidTvProfile = false,
                    lowPowerGpuProfile = false,
                ),
            ),
        )

        val video = deck.cards[1]
        val titles = video.recommendations.map { it.title }
        assertEquals(BugReportPreflightTone.Warning, video.tone)
        assertTrue(video.facts.contains("Software decoder"))
        assertTrue(video.facts.contains("Thermal severe"))
        assertTrue(titles.contains("Decoder could not keep up"))
        assertTrue(titles.contains("Let the device cool down"))
        assertTrue(titles.contains("Use a hardware-decoded codec"))
        assertTrue(video.recommendations.any { it.detail.contains("Try H264") })
        assertEquals(
            "video-device-measured",
            bugReportKnownIssueBlock(
                title = "Low FPS and video stutter",
                description = "The video becomes choppy and slow after the phone gets hot during a stream.",
                deck = deck,
            )?.key,
        )
    }

    @Test
    fun lagReportAboveDetectedRecommendationRequiresExplicitOverride() {
        val recommended = settings
        val selected = settings.copy(
            resolution = "2560x1440",
            fps = 120,
            maxBitrateMbps = 75,
        )
        val deck = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(
                requestedSettings = selected,
                recommendedSettings = recommended,
                runtimeStats = StreamRuntimeStats(
                    bitrateKbps = 32_000,
                    pingMs = 20,
                    fps = 72,
                    resolution = "2560x1440",
                    codec = "H264",
                    jitterMs = 2.0,
                    packetLossPct = 0.0,
                ),
                runtimeDiagnostics = AndroidRuntimeDiagnosticsSnapshot(
                    thermalStatus = AndroidThermalStatus.None,
                    networkKind = AndroidNetworkKind.Ethernet,
                    networkDownstreamKbps = 500_000,
                ),
            ),
        )

        val video = deck.cards[1]
        assertEquals(BugReportPreflightTone.Warning, video.tone)
        assertEquals("Selected settings exceed the device recommendation", video.title)
        assertTrue(video.facts.any { it.startsWith("Detected Recommended 1920x1080@60") })
        assertTrue(video.recommendations.any { it.title == "Use the detected Recommended profile" })

        val block = requireNotNull(
            bugReportKnownIssueBlock(
                title = "Lag and low FPS",
                description = "The stream stutters and feels slow while I play.",
                deck = deck,
            ),
        )
        assertEquals("device-profile-override", block.key)
        assertTrue(block.title.contains("exceeds this device's recommendation"))
        assertFalse(bugReportKnownIssueAllowsSubmission(block, null))
        assertTrue(bugReportKnownIssueAllowsSubmission(block, block.key))
        assertEquals(
            null,
            bugReportKnownIssueBlock(
                title = "Wrong game artwork",
                description = "The store card uses the wrong image after refresh.",
                deck = deck,
            ),
        )
    }

    @Test
    fun inputCardDistinguishesCapturedMouseFromMissingEvidence() {
        val missing = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(requestedSettings = settings),
        ).cards.last()
        assertEquals(BugReportPreflightTone.Notice, missing.tone)
        assertEquals("For an input problem, reproduce it once", missing.recommendations.single().title)

        val captured = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(
                requestedSettings = settings,
                inputDiagnostics = """
                    input channel open label=input_channel_v1
                    input channel open label=input_channel_partially_reliable
                    external mouse move sent source=131076 device=7 mode=relative
                """.trimIndent(),
            ),
        ).cards.last()
        assertEquals(BugReportPreflightTone.Healthy, captured.tone)
        assertTrue(captured.facts.any { it.contains("External mouse") })
        assertTrue(captured.facts.contains("Input channels opened"))
        assertTrue(captured.facts.contains("Mouse movement sent"))
        assertTrue(captured.recommendations.isEmpty())
    }

    @Test
    fun disconnectedInputOnlyBlocksMatchingInputReports() {
        val deck = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(
                requestedSettings = settings,
                inputDiagnostics = "external mouse input dropped noOpenChannel",
            ),
        )

        assertEquals(
            "input-measured",
            bugReportKnownIssueBlock(
                title = "Mouse input does not work",
                description = "The cursor stops moving after the stream reconnects and clicks no longer reach the game.",
                deck = deck,
            )?.key,
        )
        assertEquals(
            null,
            bugReportKnownIssueBlock(
                title = "Store artwork is missing",
                description = "Several game cards show a blank image after I return from the library page.",
                deck = deck,
            ),
        )
    }

    @Test
    fun manualServerSelectionWarnsAndGatesMatchingStreamReports() {
        val deck = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(
                requestedSettings = settings,
                runtimeStats = StreamRuntimeStats(
                    pingMs = 24,
                    fps = 60,
                    jitterMs = 1.0,
                    packetLossPct = 0.0,
                ),
                runtimeDiagnostics = AndroidRuntimeDiagnosticsSnapshot(
                    networkKind = AndroidNetworkKind.Ethernet,
                    networkDownstreamKbps = 500_000,
                ),
                serverZone = "np-lon-06",
                manuallySelectedServer = true,
            ),
        )

        val connection = deck.cards.first { it.area == BugReportPreflightArea.Connection }
        assertEquals(BugReportPreflightTone.Warning, connection.tone)
        assertEquals("A manually selected server may explain the issue", connection.title)
        assertTrue(connection.facts.contains("Server np-lon-06"))
        assertTrue(connection.facts.contains("Manual server selection"))
        assertTrue(connection.summary.contains("may not be investigated"))

        val block = requireNotNull(
            bugReportKnownIssueBlock(
                title = "FPS drops to 30",
                description = "The video becomes choppy even though my bandwidth is fast.",
                deck = deck,
            ),
        )
        assertEquals("network-manual-server", block.key)
        assertTrue(block.action.contains("may not be investigated"))
        assertFalse(bugReportKnownIssueAllowsSubmission(block, null))
        assertTrue(bugReportKnownIssueAllowsSubmission(block, block.key))
    }

    @Test
    fun manualServerSelectionDoesNotGateUnrelatedReports() {
        val deck = buildBugReportPreflightDeck(
            BugReportPreflightEvidence(
                requestedSettings = settings,
                manuallySelectedServer = true,
            ),
        )

        assertEquals(
            null,
            bugReportKnownIssueBlock(
                title = "Wrong store artwork",
                description = "The library card displays an image from a different game.",
                deck = deck,
            ),
        )
    }
}
