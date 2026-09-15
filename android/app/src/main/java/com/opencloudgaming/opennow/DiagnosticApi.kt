package com.opencloudgaming.opennow

import kotlinx.serialization.json.*

private val providerStatusCodePattern = Regex("\"statusCode\"\\s*:\\s*(-?\\d+)")
private val graphQlErrorsPattern = Regex("\"errors\"\\s*:\\s*\\[\\s*\\{")

private fun diagnosticApiApplicationFailure(raw: String): Boolean {
    val statusStart = raw.indexOf("\"requestStatus\"")
    if (statusStart >= 0) {
        val status = raw.substring(statusStart, (statusStart + 1_000).coerceAtMost(raw.length))
            .substringBefore('}')
        val code = providerStatusCodePattern.find(status)?.groupValues?.get(1)?.toIntOrNull()
        if (code != null && code != 1) return true
    }
    return graphQlErrorsPattern.containsMatchIn(raw.take(8_000))
}

internal const val DIAGNOSTIC_API_BODY_MAX_CHARS = 262_144
internal const val DIAGNOSTIC_API_MAX_RECORDS = 256
internal const val DIAGNOSTIC_API_MAX_CAPTURED_CHARS = 2_097_152

/** Keep payload structure intact. Parsing and redaction happen once on the export worker. */
internal class DiagnosticApiBody(raw: String, private val decodeQueryJson: Boolean = false, private val stripCatalogText: Boolean = false) {
    val originalChars = raw.length
    val truncated = raw.length > DIAGNOSTIC_API_BODY_MAX_CHARS
    val capturedChars = minOf(raw.length, DIAGNOSTIC_API_BODY_MAX_CHARS)
    private var captured = if (truncated) raw.take(DIAGNOSTIC_API_BODY_MAX_CHARS - 4096) + raw.takeLast(4096) else raw
    val applicationFailure = diagnosticApiApplicationFailure(captured)

    val json: JsonElement by lazy {
        val result = if (truncated) {
            buildJsonObject {
                put("originalChars", originalChars)
                put("truncated", true)
                put("head", captured.dropLast(4096))
                put("tail", captured.takeLast(4096))
            }
        } else if (captured.isEmpty()) {
            JsonNull
        } else {
            runCatching { OpenNowJson.parseToJsonElement(captured) }.getOrElse { JsonPrimitive(captured) }
        }
        val expanded = if (decodeQueryJson && result is JsonObject) JsonObject(result.mapValues { (key, value) ->
            if (key in setOf("variables", "extensions") && value is JsonArray) JsonArray(value.map { item ->
                val text = (item as? JsonPrimitive)?.contentOrNull
                if (text == null) item else runCatching { OpenNowJson.parseToJsonElement(text) }.getOrDefault(item)
            }) else value
        }) else result
        val compact = if (stripCatalogText) stripVerboseDiagnosticCatalogText(expanded) else expanded
        sanitizeDiagnosticParserJson(compact).also { captured = "" }
    }
}

/** Only promotional text in identifiable game objects; never trim provider error descriptions. */
internal fun stripVerboseDiagnosticCatalogText(element: JsonElement): JsonElement = when (element) {
    is JsonObject -> {
        val isGame = ("id" in element || "appId" in element) && ("title" in element || "name" in element) &&
            element.keys.any { it in setOf("gfn", "variants", "publisherName", "images", "shortDescription", "longDescription") }
        JsonObject(element.mapValues { (key, value) ->
            if (isGame && key in setOf("description", "shortDescription", "longDescription", "synopsis") &&
                value is JsonPrimitive && value.isString && value.content.length > 256) {
                JsonPrimitive("[omitted catalog text: ${value.content.length} characters]")
            } else stripVerboseDiagnosticCatalogText(value)
        })
    }
    is JsonArray -> JsonArray(element.map(::stripVerboseDiagnosticCatalogText))
    else -> element
}

internal data class DiagnosticApiEntry(
    val timestampMs: Long,
    val method: String,
    val url: String,
    val statusCode: Int?,
    val elapsedMs: Long,
    val requestBytes: Long?,
    val responseChars: Int,
    val request: DiagnosticApiBody,
    val response: DiagnosticApiBody,
    val error: String? = null,
    val count: Int = 1,
    val firstTimestampMs: Long = timestampMs,
    val requestQuery: DiagnosticApiBody = DiagnosticApiBody(""),
    val requestHeaders: DiagnosticApiBody = DiagnosticApiBody(""),
    val responseHeaders: DiagnosticApiBody = DiagnosticApiBody(""),
) {
    val capturedChars: Int get() = request.capturedChars + response.capturedChars +
        requestQuery.capturedChars + requestHeaders.capturedChars + responseHeaders.capturedChars
    val truncatedBodies: Int get() = listOf(request, response, requestQuery, requestHeaders, responseHeaders).count { it.truncated }

    fun summary(): String {
        val providerStatus = (response.json as? JsonObject)?.get("requestStatus") as? JsonObject
        val description = (providerStatus?.get("statusDescription") as? JsonPrimitive)?.contentOrNull
        return "${diagnosticTimestamp(timestampMs)} $method $url -> ${statusCode ?: "ERR"} " +
            "${elapsedMs}ms responseChars=$responseChars count=$count" +
            (description?.let { " providerStatus=$it" } ?: "") + (error?.let { " error=$it" } ?: "")
    }

    fun toJson(): JsonObject = buildJsonObject {
        put("timestamp", diagnosticTimestamp(timestampMs))
        put("firstTimestamp", diagnosticTimestamp(firstTimestampMs))
        put("count", count)
        put("method", method)
        put("url", url)
        put("statusCode", statusCode?.let(::JsonPrimitive) ?: JsonNull)
        put("elapsedMs", elapsedMs)
        requestBytes?.let { put("requestBytes", it) }
        put("responseChars", responseChars)
        if (request.originalChars > 0) put("request", request.json)
        if (response.originalChars > 0) put("response", response.json)
        if (requestQuery.json != JsonNull) put("requestQuery", requestQuery.json)
        if (requestHeaders.json != JsonNull) put("requestHeaders", requestHeaders.json)
        if (responseHeaders.json != JsonNull) put("responseHeaders", responseHeaders.json)
        put("truncatedBodies", truncatedBodies)
        error?.let { put("error", it) }
    }
}

internal data class DiagnosticApiSnapshot(
    val entries: List<DiagnosticApiEntry>,
    val totalRecorded: Long,
    val evictedRecords: Long,
    val capturedChars: Long,
    val recordLimit: Int,
    val characterLimit: Int,
) {
    fun retentionJson(): JsonObject = buildJsonObject {
        put("mode", "individual_requests")
        put("totalRecorded", totalRecorded)
        put("retainedRecords", entries.size)
        put("evictedRecords", evictedRecords)
        put("recordLimit", recordLimit)
        put("capturedChars", capturedChars)
        put("characterLimit", characterLimit)
        put("bodyCharacterLimit", DIAGNOSTIC_API_BODY_MAX_CHARS)
        put("truncatedBodies", entries.sumOf { it.truncatedBodies })
    }
}

internal class DiagnosticApiBuffer(
    private val capacity: Int = DIAGNOSTIC_API_MAX_RECORDS,
    private val maxCapturedChars: Int = DIAGNOSTIC_API_MAX_CAPTURED_CHARS,
) {
    private val entries = ArrayDeque<DiagnosticApiEntry>()
    private var capturedChars = 0L
    private var totalRecorded = 0L
    private var evictedRecords = 0L

    init { require(capacity > 0 && maxCapturedChars > 0) }

    @Synchronized
    fun record(entry: DiagnosticApiEntry) {
        // Retain each call and response, including changes in successful polls and their timings.
        entries.addLast(entry)
        totalRecorded++
        capturedChars += entry.capturedChars
        while (entries.size > capacity || capturedChars > maxCapturedChars) {
            // Prefer preserving session traffic and provider failures over routine catalogue calls.
            val routine = entries.firstOrNull {
                it.statusCode in 200..299 && it.error == null && !it.response.applicationFailure && !it.url.contains("/v2/session")
            }
            val removed = routine ?: entries.first()
            entries.remove(removed)
            capturedChars -= removed.capturedChars
            evictedRecords++
        }
    }

    @Synchronized
    fun capture(): List<DiagnosticApiEntry> = entries.toList()

    @Synchronized
    fun snapshot(): DiagnosticApiSnapshot = DiagnosticApiSnapshot(
        entries.toList(), totalRecorded, evictedRecords, capturedChars, capacity, maxCapturedChars,
    )
}
