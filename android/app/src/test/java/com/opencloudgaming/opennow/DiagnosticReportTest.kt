package com.opencloudgaming.opennow

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class DiagnosticReportTest {
    @Test
    fun readableFailureReportAndMachineDataSurviveExportAndPreviousRunAppend() {
        val now = 1_789_289_837_123L
        val api = DiagnosticApiEntry(
            timestampMs = now, method = "POST", url = "https://prod.cloudmatchbeta.nvidiagrid.net/v2/session",
            statusCode = 500, elapsedMs = 989, requestBytes = 1793, responseChars = 2637,
            request = DiagnosticApiBody("""{"sessionRequestData":{"appId":"18106011","internalTitle":"Trove","deviceHashId":"private-device","clientRequestMonitorSettings":[{"widthInPixels":2340,"heightInPixels":1080,"framesPerSecond":30}]}}"""),
            response = DiagnosticApiBody("""{"requestStatus":{"statusCode":89,"statusDescription":"USER_STORAGE_NOT_AVAILABLE CA8C3011","serverId":"NP-BOM-01","unifiedErrorCode":-896782319}}"""),
        )
        val parser = buildJsonObject {
            put("schemaVersion", 2)
            put("capturedAt", diagnosticTimestamp(now))
            put("device", buildJsonObject {
                put("model", "SM-A245F")
                put("androidSdk", 36)
                put("processorCount", 8)
                put("totalMemoryMiB", 7684)
            })
            put("stream", buildJsonObject {
                put("game", "Trove")
                put("state", "idle")
                put("storage", OpenNowJson.encodeToJsonElement(StorageAddon(regionName = "Bulgaria", regionCode = "NP-SOFMR-DC")))
            })
            put("api", JsonArray(listOf(api.toJson())))
        }
        val human = """
            OpenNOW Android diagnostics | format=2
            Captured: ${diagnosticTimestamp(now)} | timezone=UTC
            Example fixture — not a live device capture

            [Overview]
            Game: Trove | state=idle
            ERROR: USER_STORAGE_NOT_AVAILABLE CA8C3011

            [Device]
            Samsung SM-A245F | Android SDK 36 | 8 cores | 7684 MiB

            [Stream & routing]
            Provider default: https://prod.cloudmatchbeta.nvidiagrid.net/
            Storage: region=Bulgaria metro=NP-SOFMR-DC

            [API — 1 record; payloads in parser block]
            api.1 ${api.summary()}

            [Machine-readable JSON — schema 2]
        """.trimIndent()
        val report = renderDiagnosticReport(human, parser)
        assertTrue(report.contains("providerStatus=USER_STORAGE_NOT_AVAILABLE CA8C3011"))
        assertFalse(report.contains("private-device"))
        assertTrue(report.contains(diagnosticTimestamp(now)))
        val parsed = OpenNowJson.parseToJsonElement(report.substringAfter("<parser>\n").substringBefore("\n</parser>")).jsonObject
        assertEquals(36, parsed.getValue("device").jsonObject.getValue("androidSdk").jsonPrimitive.int)
        assertEquals(89, parsed.getValue("api").jsonArray[0].jsonObject.getValue("response").jsonObject.getValue("requestStatus").jsonObject.getValue("statusCode").jsonPrimitive.int)
        val withHistory = appendPreviousDiagnosticSnapshot(report, PreviousDiagnosticSnapshot(now - 1000, report))
        assertEquals(withHistory, sanitizeDiagnosticExport(withHistory))
    }
}
