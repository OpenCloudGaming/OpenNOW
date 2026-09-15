package com.opencloudgaming.opennow

import kotlinx.serialization.json.*

/** Export-time observations only: an API failure does not establish a process crash. */
internal fun diagnosticFailureAssessment(
    api: List<DiagnosticApiEntry>,
    streamError: String?,
    sessionReport: SessionReport?,
    previousExits: List<DiagnosticProcessExit> = emptyList(),
): JsonObject {
    val findings = buildList {
        previousExits.forEachIndexed { index, exit ->
            if (exit.reason !in setOf(3, 4, 5, 6, 7, 9, 12)) return@forEachIndexed
            add(buildJsonObject {
                put("reasonCode", "previous_process_${exit.reasonCode}")
                put("category", "previous_process")
                put("confidence", "os_recorded_exit")
                put("possibleReason", "Android recorded ${exit.reasonCode} for a previous app process. Correlate its timestamp with the previous-run log; it does not establish a crash in the currently running session.")
                put("timestamp", exit.timestamp)
                put("evidencePaths", JsonArray(listOf(JsonPrimitive("/processExitHistory/records/$index"))))
            })
        }
        api.forEachIndexed { index, entry ->
            if (entry.statusCode in 200..299 && entry.error == null && !entry.response.applicationFailure) return@forEachIndexed
            val response = entry.response.json
            val status = (response as? JsonObject)?.get("requestStatus") as? JsonObject
            val description = (status?.get("statusDescription") as? JsonPrimitive)?.contentOrNull.orEmpty()
            val error = entry.error.orEmpty()
            val code: String
            val possibleReason: String
            when {
                description.contains("USER_STORAGE_NOT_AVAILABLE", ignoreCase = true) -> {
                    code = "provider_storage_unavailable"
                    possibleReason = "The provider rejected session creation because game storage was unavailable. Compare the storage region with the responding server; a routing mismatch is one possible explanation."
                }
                error.contains("UnknownHostException") -> {
                    code = "dns_resolution_failed"
                    possibleReason = "The API hostname could not be resolved. Connectivity or DNS may have interrupted launch or recovery."
                }
                entry.statusCode == 401 || entry.statusCode == 403 -> {
                    code = "api_access_rejected"
                    possibleReason = "The API rejected access. Authentication, entitlement or provider policy may be involved."
                }
                error.contains("Timeout", ignoreCase = true) -> {
                    code = "api_timeout"
                    possibleReason = "The API call timed out. Network delay or an unresponsive provider may be involved."
                }
                entry.statusCode != null && entry.statusCode >= 500 -> {
                    code = "provider_http_failure"
                    possibleReason = "The provider returned a server error. The retained response may explain why the operation failed."
                }
                entry.response.applicationFailure -> {
                    code = "provider_application_failure"
                    possibleReason = "The API response contains a provider error despite its HTTP transport status."
                }
                else -> {
                    code = "api_call_failed"
                    possibleReason = "An API call failed; inspect its response and surrounding timeline to determine whether it affected the session."
                }
            }
            add(buildJsonObject {
                put("reasonCode", code)
                put("category", "api")
                put("confidence", "observed_failure")
                put("possibleReason", possibleReason)
                put("timestamp", diagnosticTimestamp(entry.timestampMs))
                put("evidencePaths", JsonArray(listOf(JsonPrimitive("/api/$index"))))
                put("httpStatus", entry.statusCode?.let(::JsonPrimitive) ?: JsonNull)
                status?.let { put("providerStatus", it) }
                entry.error?.let { put("error", it) }
            })
        }
        streamError?.takeIf { it.isNotBlank() }?.let {
            add(buildJsonObject {
                put("reasonCode", "stream_error_reported")
                put("category", "stream")
                put("confidence", "observed_failure")
                put("possibleReason", "OpenNOW reported a stream error; this message alone does not identify a process crash or root cause.")
                put("error", it)
                put("evidencePaths", JsonArray(listOf(JsonPrimitive("/stream/error"))))
            })
        }
        sessionReport?.recommendations?.forEachIndexed { index, finding ->
            if (finding.kind != SessionReportFindingKind.Warning) return@forEachIndexed
            add(buildJsonObject {
                put("reasonCode", finding.reasonCode)
                put("category", "session_quality")
                put("confidence", "metric_based_hint")
                put("possibleReason", finding.title)
                put("evidencePaths", JsonArray(listOf(JsonPrimitive("/sessionScore/report/recommendations/$index"))))
            })
        }
    }
    return buildJsonObject {
        put("rulesVersion", 1)
        put("crashConclusion", "not_established")
        put("crashEvidence", "No current-process crash is established. Previous OS exit reasons, when available, are separate historical evidence; API and quality failures do not prove an app crash.")
        put("scope", "retained_run_records")
        put("limitations", "Bounded logs can omit earlier events. Findings are observations and possible explanations, not confirmed crash causes; compare timestamps with the reported incident.")
        put("findings", JsonArray(findings))
    }
}

internal fun diagnosticSessionScore(report: SessionReport?, phase: String, showAfterStream: Boolean): JsonObject = buildJsonObject {
    put("available", report != null)
    put("phase", if (report == null) "unavailable" else phase)
    put("showAfterStream", showAfterStream)
    put("scale", 100)
    put("algorithmVersion", 1)
    if (report == null) {
        put("unavailableReason", "No measured stream samples for this session; launch failures may have no score.")
    } else {
        put("gaugeFraction", report.score.coerceIn(0, 100) / 100.0)
        put("report", OpenNowJson.encodeToJsonElement(report))
        put("components", buildJsonObject {
            fun component(name: String, score: Int?, weight: Int) {
                put(name, buildJsonObject {
                    put("available", score != null)
                    put("score", score?.let(::JsonPrimitive) ?: JsonNull)
                    put("weight", weight)
                })
            }
            component("latency", report.averagePingMs?.let(::latencyScore), 35)
            component("packetLoss", report.packetLossPct?.let(::packetLossScore), 30)
            component("jitter", report.averageJitterMs?.let(::jitterScore), 15)
            component("frameRate", report.averageFps?.let { frameRateScore(it, report.targetFps) }, 15)
            component("decode", report.averageDecodeMs?.let { decodeScore(it, report.targetFps, report.averageFps) }, 5)
        })
    }
}
