package com.opencloudgaming.opennow

import java.io.File
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class DiagnosticAssessmentTest {
    private val now = 1_789_289_837_123L
    private fun api(status: Int? = 500, body: String = "", error: String? = null) = DiagnosticApiEntry(
        timestampMs = now, method = "POST", url = "https://example.invalid/v2/session",
        statusCode = status, elapsedMs = 989, requestBytes = 0, responseChars = body.length,
        request = DiagnosticApiBody(""), response = DiagnosticApiBody(body), error = error,
    )
    private fun accumulator(): StreamSessionReportAccumulator {
        val settings = StreamSettings(resolution = "1920x1080", fps = 60, maxBitrateMbps = 30)
        return StreamSessionReportAccumulator(StreamReportLaunchProfile("Preview game", settings, settings, settings), now - 60_000).also { accumulator ->
            repeat(10) {
                accumulator.record(StreamRuntimeStats(pingMs = 95, jitterMs = 22.0, packetLossPct = 2.0,
                    fps = 58, decodeMs = 5.0, bitrateKbps = 28_000, resolution = "1920x1080", codec = "H264"),
                    AndroidRuntimeDiagnosticsSnapshot(networkKind = AndroidNetworkKind.Wifi, wifiBand = AndroidWifiBand.TwoPointFourGhz))
            }
        }
    }

    @Test
    fun providerFailureLinksToResponseWithoutClaimingCrash() {
        val api = api(body = """{"requestStatus":{"statusCode":89,"statusDescription":"USER_STORAGE_NOT_AVAILABLE CA8C3011","serverId":"NP-BOM-01"}}""")
        val result = sanitizeDiagnosticParserJson(diagnosticFailureAssessment(listOf(api), null, null)).jsonObject
        assertEquals("not_established", result.getValue("crashConclusion").jsonPrimitive.content)
        val finding = result.getValue("findings").jsonArray.single().jsonObject
        assertEquals("provider_storage_unavailable", finding.getValue("reasonCode").jsonPrimitive.content)
        assertEquals("/api/0", finding.getValue("evidencePaths").jsonArray.single().jsonPrimitive.content)
        assertEquals(89, finding.getValue("providerStatus").jsonObject.getValue("statusCode").jsonPrimitive.int)
    }

    @Test
    fun priorNativeCrashRemainsHistoricalAndUserStopsAreNotCrashes() {
        val exits = listOf(5, 10).map { reason -> DiagnosticProcessExit(diagnosticTimestamp(now - 1000),
            now - 1000, reason, diagnosticProcessExitReason(reason), 0, null, null, null) }
        val result = diagnosticFailureAssessment(emptyList(), null, null, exits)
        val finding = result.getValue("findings").jsonArray.single().jsonObject
        assertEquals("previous_process_crash_native", finding.getValue("reasonCode").jsonPrimitive.content)
        assertEquals("/processExitHistory/records/0", finding.getValue("evidencePaths").jsonArray.single().jsonPrimitive.content)
        assertEquals("os_recorded_exit", finding.getValue("confidence").jsonPrimitive.content)
        assertEquals("not_established", result.getValue("crashConclusion").jsonPrimitive.content)
    }

    @Test
    fun dnsFailureAndHealthyCallAreDistinguished() {
        val result = diagnosticFailureAssessment(listOf(api(status = 200), api(null, error = "UnknownHostException: provider")), null, null)
        val finding = result.getValue("findings").jsonArray.single().jsonObject
        assertEquals("dns_resolution_failed", finding.getValue("reasonCode").jsonPrimitive.content)
        assertEquals("/api/1", finding.getValue("evidencePaths").jsonArray.single().jsonPrimitive.content)
    }

    @Test
    fun scoreSnapshotPreservesAccumulatorAndExactAppRecommendations() {
        val accumulator = accumulator()
        val report = accumulator.finish(now)!!
        val snapshot = sanitizeDiagnosticParserJson(diagnosticSessionScore(report, "active", true)).jsonObject
        assertEquals(64, report.score)
        assertEquals(0.64, snapshot.getValue("gaugeFraction").jsonPrimitive.double, 0.0001)
        val restored = OpenNowJson.decodeFromJsonElement<SessionReport>(snapshot.getValue("report"))
        assertEquals(report.recommendations, restored.recommendations)
        assertTrue(restored.recommendations.all { it.reasonCode != "unspecified" })
        accumulator.record(StreamRuntimeStats(pingMs = 20))
        assertEquals(11, accumulator.finish(now + 1000)!!.sampleCount)
    }

    @Test
    fun noSamplesMeansNoScoreAndNoInventedDiagnosis() {
        val score = diagnosticSessionScore(null, "active", true)
        assertFalse(score.getValue("available").jsonPrimitive.boolean)
        assertFalse(score.containsKey("report"))
        assertTrue(diagnosticFailureAssessment(emptyList(), null, null).getValue("findings").jsonArray.isEmpty())
    }

    @Test
    fun nonFiniteMediaDoesNotBreakReportJson() {
        val accumulator = accumulator()
        accumulator.record(StreamRuntimeStats(decodeMs = Double.POSITIVE_INFINITY,
            jitterMs = Double.NaN, packetLossPct = Double.POSITIVE_INFINITY))
        val text = diagnosticSessionScore(accumulator.finish(now), "completed", true).toString()
        assertFalse(text.contains("Infinity"))
        assertFalse(text.contains("NaN"))
    }

    @Test
    fun halfCircleReflectsScoreAndClampsRange() {
        assertEquals(0f, sessionScoreSweepDegrees(-1), 0.001f)
        assertEquals(90f, sessionScoreSweepDegrees(50), 0.001f)
        assertEquals(180f, sessionScoreSweepDegrees(101), 0.001f)
    }

    @Test
    fun previewUsesProductionScoreAssessmentAndSanitization() {
        val report = accumulator().finish(now)!!
        val calls = listOf(api(503, """{"requestStatus":{"statusCode":-1,"statusDescription":"SERVICE_UNAVAILABLE"},"access_token":"private-preview-token"}"""))
        val score = diagnosticSessionScore(report, "completed", true)
        val assessment = diagnosticFailureAssessment(calls, "Provider request failed during the session", report)
        val settings = StreamSettings(resolution = "1920x1080", fps = 60, maxBitrateMbps = 30)
        val parser = buildJsonObject {
            put("schemaVersion", 2)
            put("retention", buildJsonObject {
                val buffer = DiagnosticApiBuffer(); calls.forEach(buffer::record)
                put("api", buffer.snapshot().retentionJson())
                put("stream", buildJsonObject {
                    put("sampleLimit", DIAGNOSTIC_STREAM_SAMPLE_LIMIT); put("totalSamples", 0)
                    put("retainedSamples", 0); put("evictedSamples", 0)
                })
            })
            put("capturedAt", diagnosticTimestamp(now)); put("capturedAtEpochMs", now); put("uptimeMs", 60_000); put("timezone", "UTC")
            put("app", buildJsonObject {
                put("version", "preview"); put("build", 0); put("debug", true); put("distribution", "fixture")
                put("provider", "NVIDIA"); put("membershipTier", "example")
            })
            put("device", OpenNowJson.encodeToJsonElement(AndroidDeviceDiagnosticsSnapshot(
                manufacturer = "Example", brand = "Example", model = "Preview phone", deviceCodename = "preview",
                product = "preview", hardware = "example", board = "example", androidRelease = "15",
                androidCodename = "REL", androidSdk = 35, targetSdk = 35, securityPatch = "2026-09-01",
                supportedAbis = listOf("arm64-v8a"), is64BitRuntime = true, processorCount = 8,
                totalMemoryMiB = 8192, lowRamDevice = false, displayWidthPixels = 2400, displayHeightPixels = 1080,
                densityDpi = 420, smallestScreenWidthDp = 411, formFactor = "phone", emulator = false)))
            put("deviceRuntime", OpenNowJson.encodeToJsonElement(AndroidRuntimeDiagnosticsSnapshot(
                batteryPercent = 72, batteryTemperatureC = 35f, networkKind = AndroidNetworkKind.Wifi,
                wifiBand = AndroidWifiBand.TwoPointFourGhz, thermalStatus = AndroidThermalStatus.None)))
            put("stream", buildJsonObject {
                put("samples", JsonArray(emptyList()))
                put("state", "idle"); put("game", report.gameTitle); put("launchPhase", ""); put("queuePosition", JsonNull)
                put("error", "Provider request failed during the session"); put("providerDefaultUrl", "https://example.invalid/")
                put("sessionBaseUrl", JsonNull); put("settings", OpenNowJson.encodeToJsonElement(settings))
            })
            put("inputSettings", buildJsonObject { put("mouseLock", false); put("touch", OpenNowJson.encodeToJsonElement(AndroidTouchSettings())) })
            put("cpuBuckets", JsonArray(emptyList()))
            put("cpuSamples", JsonArray(emptyList()))
            put("input", InputDiagnosticsSnapshot(emptyList(), emptyList()).toJson(DiagnosticTimeAnchor(now, 60_000)))
            put("events", JsonArray(emptyList())); put("api", JsonArray(calls.map { it.toJson() }))
            put("sessionScore", score); put("failureAssessment", assessment)
            put("processExitHistory", OpenNowJson.encodeToJsonElement(DiagnosticProcessExitHistory("available")))
        }
        val human = buildString {
            appendLine("OpenNOW Android diagnostics | format=2")
            appendLine("PREVIEW ONLY — synthetic values, not a device capture")
            appendLine("Captured: ${diagnosticTimestamp(now)} | timezone=UTC | uptimeMs=60000")
            appendLine("\n[Overview]\nGame: Preview game | state=idle")
            appendLine("\n[Device]\nPreview phone | Android 15 | 8 cores | 8192 MiB | Wi-Fi 2.4 GHz")
            appendLine("\n[Session score]\n${report.score}/100 ${report.rating.label} | phase=completed | samples=${report.sampleCount} | limitedData=${report.limitedData}")
            report.recommendations.forEach { appendLine("${it.reasonCode}: ${it.title} — ${it.detail}") }
            appendLine("\n[Failure clues — not a confirmed crash diagnosis]")
            assessment.getValue("findings").jsonArray.filter { it.jsonObject["category"]?.jsonPrimitive?.content != "session_quality" }.forEach {
                val finding = it.jsonObject
                appendLine("${finding.getValue("reasonCode").jsonPrimitive.content}: ${finding.getValue("possibleReason").jsonPrimitive.content} | evidence=${finding.getValue("evidencePaths")}")
            }
            appendLine("\n[API — 1 record; payloads in parser block]\napi.1 ${calls.single().summary()}")
            appendLine("\n[Machine-readable JSON — schema 2]")
        }
        val preview = renderDiagnosticReport(human, parser)
        assertFalse(preview.contains("private-preview-token"))
        val restored = OpenNowJson.parseToJsonElement(preview.substringAfter("<parser>\n").substringBefore("\n</parser>"))
        assertEquals("packet_loss", restored.jsonObject.getValue("sessionScore").jsonObject.getValue("report").jsonObject
            .getValue("recommendations").jsonArray[1].jsonObject.getValue("reasonCode").jsonPrimitive.content)
        if (System.getenv("OPENNOW_UPDATE_DIAGNOSTIC_SCHEMA") == "1") File("../docs/diagnostics-preview.txt").writeText(preview)
    }
}
