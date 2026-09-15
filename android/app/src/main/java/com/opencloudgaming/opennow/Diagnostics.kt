package com.opencloudgaming.opennow

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.Request
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.RequestBody.Companion.toRequestBody
import okio.Buffer
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

internal const val OPENNOW_DEBUG_LOG_TAG = "OpenNOWDebug"

private const val DIAGNOSTIC_PAYLOAD_BODY_LIMIT = 20_000
private const val HTTP_DIAGNOSTIC_MAX_REQUEST_CAPTURE_BYTES = 262_144L

private val DIAGNOSTIC_SENSITIVE_TEXT_PATTERN = Regex(
    """(?i)\b(authorization|access[_-]?token|id[_-]?token|refresh[_-]?token|client[_-]?token|device[_-]?code|user[_-]?code|verification[_-]?uri[_-]?complete|credential|password|secret|cookie|code|sub)(\s*[=:]\s*)([^\s,;&]+)""",
)
private val DIAGNOSTIC_BEARER_PATTERN = Regex("""(?i)\bBearer\s+[A-Za-z0-9._~+/=-]+""")
private val DIAGNOSTIC_JSON_IDENTITY_PATTERN = Regex(
    """(?i)([\"']?(?:email|user(?:[_-]?id|[_-]?name)?|display[_-]?name|account[_-]?id|profile[_-]?id|session[_-]?id|server[_-]?ip|device[_-]?id|device[_-]?name|ip[_-]?address)[\"']?\s*:\s*)(\"(?:\\.|[^\"])*\"|'(?:\\.|[^'])*'|[^,}\r\n]+)""",
)
private val DIAGNOSTIC_LINE_IDENTITY_PATTERN = Regex(
    """(?im)\b(email|user|user[_-]?id|user[_-]?name|display[_-]?name|account|account[_-]?id|profile[_-]?id|session|session[_-]?id|server|server[_-]?ip|device|device[_-]?id|device[_-]?name|ip[_-]?address)(\s*[=:]\s*)(.*?)(?=\s+[a-z][a-z0-9_.-]*\s*[=:]|$)""",
)
private val DIAGNOSTIC_EMAIL_PATTERN = Regex(
    """\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b""",
    RegexOption.IGNORE_CASE,
)
private val DIAGNOSTIC_IPV4_PATTERN = Regex("""\b(?:\d{1,3}\.){3}\d{1,3}\b""")
private val DIAGNOSTIC_IPV6_FULL_PATTERN = Regex(
    """(?i)(?<![A-F0-9:])(?:[A-F0-9]{1,4}:){7}[A-F0-9]{1,4}(?![A-F0-9:])""",
)
private val DIAGNOSTIC_IPV6_COMPRESSED_PATTERN = Regex(
    """(?i)(?<![A-F0-9:])(?:(?:[A-F0-9]{1,4}:){1,7}:(?:[A-F0-9]{1,4}(?::[A-F0-9]{1,4}){0,6})?|::(?:[A-F0-9]{1,4}(?::[A-F0-9]{1,4}){0,6})?)(?![A-F0-9:])""",
)
private val DIAGNOSTIC_UUID_PATTERN = Regex(
    """\b[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[1-5][0-9a-fA-F]{3}-[89abAB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}\b""",
)

private val DebugPayloadJson = Json {
    prettyPrint = true
    ignoreUnknownKeys = true
    explicitNulls = false
    isLenient = true
    encodeDefaults = true
}

internal object OpenNowHttpDiagnostics {
    private val buffer = DiagnosticApiBuffer()

    fun record(
        request: Request,
        requestBody: String,
        statusCode: Int?,
        responseBody: String,
        elapsedMs: Long,
        error: Throwable? = null,
        responseHeaders: okhttp3.Headers? = null,
    ) {
        val entry = DiagnosticApiEntry(
            timestampMs = System.currentTimeMillis(),
            method = request.method,
            url = request.url.newBuilder().username("").password("").query(null).build().toString(),
            requestQuery = DiagnosticApiBody(diagnosticRequestQuery(request), decodeQueryJson = true),
            requestHeaders = DiagnosticApiBody(diagnosticHeaders(request.headers)),
            responseHeaders = DiagnosticApiBody(responseHeaders?.let(::diagnosticHeaders).orEmpty()),
            statusCode = statusCode,
            elapsedMs = elapsedMs,
            requestBytes = request.body?.safeContentLength()?.takeIf { it >= 0 },
            responseChars = responseBody.length,
            request = DiagnosticApiBody(requestBody),
            response = DiagnosticApiBody(responseBody, stripCatalogText = true),
            error = error?.let { "${it.javaClass.simpleName}: ${it.message.orEmpty()}".take(320) },
        )
        buffer.record(entry)
    }

    fun captureRequestBody(request: Request): String {
        val body = request.body ?: return ""
        val contentLength = body.safeContentLength()
        if (contentLength < 0 || contentLength > HTTP_DIAGNOSTIC_MAX_REQUEST_CAPTURE_BYTES) {
            return "(request body omitted: unknown or excessive size)"
        }
        if (body.isDuplex()) return "(duplex request body omitted)"
        if (body.isOneShot()) return "(one-shot request body omitted)"
        return runCatching {
            val buffer = Buffer()
            body.writeTo(buffer)
            buffer.readUtf8()
        }.getOrElse { error -> "(request body unavailable ${error.javaClass.simpleName})" }
    }

    fun capture(): List<DiagnosticApiEntry> = buffer.capture()
    fun snapshot(): DiagnosticApiSnapshot = buffer.snapshot()
}

internal fun diagnosticRequestQuery(request: Request): String = buildJsonObject {
    request.url.queryParameterNames.forEach { name ->
        put(name, JsonArray(request.url.queryParameterValues(name).map { value ->
            if (value == null) kotlinx.serialization.json.JsonNull else JsonPrimitive(value)
        }))
    }
}.toString()

internal fun diagnosticHeaders(headers: okhttp3.Headers): String = buildJsonObject {
    headers.names().forEach { name -> put(name, JsonArray(headers.values(name).map(::JsonPrimitive))) }
}.toString()

internal fun sanitizeDiagnosticLogPayload(
    raw: String,
    limit: Int = DIAGNOSTIC_PAYLOAD_BODY_LIMIT,
): String {
    val trimmed = raw.trim()
    if (trimmed.isBlank()) return "(empty)"
    val formatted = runCatching {
        val sanitized = redactDiagnosticJsonElement(OpenNowJson.parseToJsonElement(trimmed))
        DebugPayloadJson.encodeToString(JsonElement.serializer(), sanitized)
    }.getOrElse {
        redactDiagnosticText(trimmed)
    }
    val redacted = redactDiagnosticText(formatted)
    return if (redacted.length <= limit) {
        redacted
    } else {
        redacted.take(limit) + "\n... truncated ${redacted.length - limit} chars ..."
    }
}

internal fun redactDiagnosticUrl(raw: String): String {
    val parsed = raw.toHttpUrlOrNull() ?: return redactDiagnosticText(raw)
    val redactedNames = (0 until parsed.querySize)
        .map { parsed.queryParameterName(it) }
        .filter(::shouldRedactDiagnosticKey)
        .distinct()
    if (redactedNames.isEmpty()) return raw
    val builder = parsed.newBuilder()
    redactedNames.forEach { name -> builder.setQueryParameter(name, "[redacted]") }
    return builder.build().toString()
}

internal fun redactDiagnosticJsonElement(element: JsonElement, keyHint: String? = null): JsonElement =
    when {
        keyHint != null && shouldRedactDiagnosticKey(keyHint) -> JsonPrimitive("[redacted]")
        element is JsonObject -> JsonObject(element.mapValues { (key, value) -> redactDiagnosticJsonElement(value, key) })
        element is JsonArray -> JsonArray(element.map { redactDiagnosticJsonElement(it) })
        else -> element
    }

private fun shouldRedactDiagnosticKey(key: String): Boolean {
    val normalized = key.lowercase(Locale.US).filter(Char::isLetterOrDigit)
    return normalized.contains("authorization") ||
        normalized.contains("token") ||
        normalized.contains("credential") ||
        normalized.contains("password") ||
        normalized.contains("secret") ||
        normalized.contains("cookie") ||
        normalized == "code" ||
        normalized == "devicecode" ||
        normalized == "usercode" ||
        normalized == "verificationuricomplete" ||
        normalized == "deviceid" ||
        normalized == "devicehashid" ||
        normalized == "sub" ||
        normalized == "email" ||
        normalized == "userid"
}

private fun redactDiagnosticText(text: String): String {
    return DIAGNOSTIC_SENSITIVE_TEXT_PATTERN.replace(text) { match ->
        "${match.groupValues[1]}${match.groupValues[2]}[redacted]"
    }
}

private val DIAGNOSTIC_PARSER_BLOCK = Regex("(?ms)^<parser>\\r?\\n(.*?)^</parser>$")
private val DIAGNOSTIC_JSON_SECRET_PATTERN = Regex(
    """(?i)(["'](?:authorization|access[_-]?token|id[_-]?token|refresh[_-]?token|client[_-]?token|credential|password|secret|cookie|device[_-]?code|user[_-]?code|verification[_-]?uri[_-]?complete)["']\s*:\s*)("(?:\\.|[^"\\])*(?:"|$)|[^,}\r\n]+)""",
)

/** Sanitize JSON structurally so redaction can never break a parser block's quoting. */
internal fun sanitizeDiagnosticExport(raw: String): String {
    val blocks = DIAGNOSTIC_PARSER_BLOCK.findAll(raw).toList()
    if (blocks.isEmpty()) return sanitizeDiagnosticText(raw)
    return buildString {
        var offset = 0
        for (block in blocks) {
            append(sanitizeDiagnosticText(raw.substring(offset, block.range.first)))
            val data = runCatching { OpenNowJson.parseToJsonElement(block.groupValues[1]) }
                .getOrElse { buildJsonObject { put("error", "Invalid diagnostic parser block omitted") } }
            append(diagnosticParserBlock(sanitizeDiagnosticParserJson(data)))
            offset = block.range.last + 1
        }
        append(sanitizeDiagnosticText(raw.substring(offset)))
    }
}

internal fun sanitizeDiagnosticParserJson(element: JsonElement): JsonElement = when (element) {
    is JsonObject -> JsonObject(element.mapValues { (key, value) ->
        val normalized = key.lowercase(Locale.US).filter(Char::isLetterOrDigit)
        if (shouldRedactDiagnosticKey(key) || normalized in setOf(
                "sessionid", "serverip", "displayname", "username", "accountid", "profileid", "ipaddress", "devicename",
            )) JsonPrimitive("[redacted]") else sanitizeDiagnosticParserJson(value)
    })
    is JsonArray -> JsonArray(element.map(::sanitizeDiagnosticParserJson))
    is JsonPrimitive -> if (element.isString) JsonPrimitive(sanitizeDiagnosticText(element.content)) else element
    else -> element
}

internal fun diagnosticParserBlock(data: JsonElement): String =
    "<parser>\n${data.toString().replace("<", "\\u003c")}\n</parser>"

private fun sanitizeDiagnosticText(raw: String): String {
    var sanitized = DIAGNOSTIC_BEARER_PATTERN.replace(raw, "Bearer [redacted]")
    sanitized = DIAGNOSTIC_JSON_SECRET_PATTERN.replace(sanitized) { "${it.groupValues[1]}\"[redacted]\"" }
    sanitized = redactDiagnosticText(sanitized)
    sanitized = DIAGNOSTIC_JSON_IDENTITY_PATTERN.replace(sanitized) { match ->
        "${match.groupValues[1]}\"[redacted]\""
    }
    sanitized = DIAGNOSTIC_LINE_IDENTITY_PATTERN.replace(sanitized) { match ->
        "${match.groupValues[1]}${match.groupValues[2]}[redacted]"
    }
    sanitized = DIAGNOSTIC_EMAIL_PATTERN.replace(sanitized, "[redacted-email]")
    sanitized = DIAGNOSTIC_IPV4_PATTERN.replace(sanitized, "[redacted-ip]")
    sanitized = DIAGNOSTIC_IPV6_FULL_PATTERN.replace(sanitized, "[redacted-ip]")
    sanitized = DIAGNOSTIC_IPV6_COMPRESSED_PATTERN.replace(sanitized, "[redacted-ip]")
    sanitized = DIAGNOSTIC_UUID_PATTERN.replace(sanitized, "[redacted-id]")
    return sanitized
}

private const val ANDROID_DIAGNOSTIC_PASTE_URL =
    "https://paste.rtech.support/upload/opennow-android-diagnostics.txt"
private const val ANDROID_DIAGNOSTIC_PASTE_EXPIRY_SECONDS = 86_400

internal suspend fun uploadAndroidDiagnosticPaste(
    http: OkHttpClient,
    sanitizedText: String,
): String = withContext(Dispatchers.IO) {
    val request = Request.Builder()
        .url(ANDROID_DIAGNOSTIC_PASTE_URL)
        .header("Accept", "application/json")
        .header("Linx-Randomize", "yes")
        .header("Linx-Expiry", ANDROID_DIAGNOSTIC_PASTE_EXPIRY_SECONDS.toString())
        .put(sanitizedText.toRequestBody("text/plain; charset=utf-8".toMediaType()))
        .build()
    http.newCall(request).execute().use { response ->
        val body = response.body.string().trim()
        if (!response.isSuccessful) {
            error("Diagnostics upload failed (HTTP ${response.code})")
        }
        val jsonUrl = runCatching {
            OpenNowJson.parseToJsonElement(body).jsonObject["url"]?.jsonPrimitive?.content
        }.getOrNull()
        (jsonUrl ?: body.lineSequence().firstOrNull { it.startsWith("https://") })
            ?.trim()
            ?.takeIf { it.startsWith("https://paste.rtech.support/") }
            ?: error("Diagnostics upload returned no paste URL")
    }
}

private fun okhttp3.RequestBody.safeContentLength(): Long =
    runCatching { contentLength() }.getOrDefault(-1L)

/** Render from the existing JSON tree; do not serialize then parse it again just to redact it. */
internal fun renderDiagnosticReport(humanText: String, data: JsonObject): String =
    sanitizeDiagnosticText(humanText).trimEnd() + "\n\n" + diagnosticParserBlock(sanitizeDiagnosticParserJson(data))
