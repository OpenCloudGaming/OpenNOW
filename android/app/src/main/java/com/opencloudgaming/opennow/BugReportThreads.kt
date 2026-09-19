package com.opencloudgaming.opennow

import androidx.compose.runtime.Immutable
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

internal const val ANDROID_BUG_REPORT_TERMS_VERSION = "2026-09-18"
private const val ANDROID_BUG_REPORT_REPORTER_HEADER = "X-Bug-Report-Reporter-Id"
private const val MAX_BUG_REPORT_THREAD_RESPONSE_CHARS = 512 * 1024

@Immutable
data class AndroidBugReportComment(
    val id: String,
    val authorRole: String,
    val kind: String,
    val body: String,
    val createdAt: Long,
)

@Immutable
data class AndroidBugReportThread(
    val id: String,
    val title: String,
    val description: String,
    val status: String,
    val resolutionNote: String,
    val versionName: String,
    val versionCode: String,
    val kind: String,
    val area: String,
    val frequency: String,
    val impact: String,
    val files: List<String>,
    val createdAt: Long,
    val updatedAt: Long,
    val comments: List<AndroidBugReportComment>,
)

internal fun androidBugReportThreadClosed(status: String): Boolean =
    status in setOf("completed", "not_reproducible", "wont_fix")

@Immutable
data class AndroidBugReportThreadsState(
    val loading: Boolean = false,
    val reports: List<AndroidBugReportThread> = emptyList(),
    val postingReportId: String? = null,
    val error: String? = null,
)

internal suspend fun fetchAndroidBugReportThreads(
    http: OkHttpClient,
    reporterId: String,
    endpoint: String = ANDROID_BUG_REPORT_ENDPOINT,
): List<AndroidBugReportThread> = withContext(Dispatchers.IO) {
    require(reporterId.matches(Regex("^br1_[0-9a-f]{64}$"))) { "Bug report installation ID is invalid" }
    val request = Request.Builder()
        .url(endpoint)
        .header("Accept", "application/json")
        .header(ANDROID_BUG_REPORT_REPORTER_HEADER, reporterId)
        .get()
        .build()
    http.newCall(request).execute().use { response ->
        val body = response.body.string().take(MAX_BUG_REPORT_THREAD_RESPONSE_CHARS)
        if (!response.isSuccessful) {
            val serverError = parseAndroidBugReportServerError(body, response.code)
            throw AndroidBugReportUploadException(serverError.code, serverError.retryable, serverError.message)
        }
        parseAndroidBugReportThreads(body)
    }
}

internal suspend fun postAndroidBugReportComment(
    http: OkHttpClient,
    reporterId: String,
    reportId: String,
    comment: String,
    endpoint: String = ANDROID_BUG_REPORT_ENDPOINT,
): AndroidBugReportThread = withContext(Dispatchers.IO) {
    require(reporterId.matches(Regex("^br1_[0-9a-f]{64}$"))) { "Bug report installation ID is invalid" }
    val cleanReportId = reportId.trim().take(160)
    require(cleanReportId.isNotBlank()) { "Bug report ID is unavailable" }
    val cleanComment = comment.trim()
    require(cleanComment.isNotBlank()) { "Write a comment first" }
    require(cleanComment.length <= 3_000) { "Comments must be 3000 characters or fewer" }
    val url = endpoint.toHttpUrl().newBuilder()
        .addPathSegment(cleanReportId)
        .addPathSegment("comments")
        .build()
    val payload = buildJsonObject {
        put("reporterId", reporterId)
        put("comment", cleanComment)
        put("termsAccepted", true)
        put("termsVersion", ANDROID_BUG_REPORT_TERMS_VERSION)
    }.toString()
    val request = Request.Builder()
        .url(url)
        .header("Accept", "application/json")
        .header(ANDROID_BUG_REPORT_REPORTER_HEADER, reporterId)
        .post(payload.toRequestBody("application/json; charset=utf-8".toMediaType()))
        .build()
    http.newCall(request).execute().use { response ->
        val body = response.body.string().take(MAX_BUG_REPORT_THREAD_RESPONSE_CHARS)
        if (!response.isSuccessful) {
            val serverError = parseAndroidBugReportServerError(body, response.code)
            throw AndroidBugReportUploadException(serverError.code, serverError.retryable, serverError.message)
        }
        parseAndroidBugReportThreadResponse(body)
            ?: throw AndroidBugReportUploadException(
                serverCode = "INVALID_RESPONSE",
                retryable = false,
                message = "The bug report service returned an invalid conversation.",
            )
    }
}

internal fun parseAndroidBugReportThreads(body: String): List<AndroidBugReportThread> {
    try {
        val root = OpenNowJson.parseToJsonElement(body).jsonObject
        val data = root["data"]?.jsonObject
            ?: throw IllegalArgumentException("Missing response data")
        val reports = data["reports"]?.jsonArray
            ?: throw IllegalArgumentException("Missing reports")
        return reports.mapNotNull { element ->
            runCatching { parseAndroidBugReportThread(element.jsonObject) }.getOrNull()
        }
    } catch (error: AndroidBugReportUploadException) {
        throw error
    } catch (error: Throwable) {
        throw AndroidBugReportUploadException(
            serverCode = "INVALID_RESPONSE",
            retryable = true,
            message = "The bug report service returned an invalid inbox.",
        )
    }
}

internal fun parseAndroidBugReportThreadResponse(body: String): AndroidBugReportThread? = runCatching {
    val root = OpenNowJson.parseToJsonElement(body).jsonObject
    root["data"]?.jsonObject?.let(::parseAndroidBugReportThread)
}.getOrNull()

private fun parseAndroidBugReportThread(json: JsonObject): AndroidBugReportThread? {
    val id = json.serverText("id")?.takeIf(String::isNotBlank) ?: return null
    val createdAt = json.serverLong("createdAt") ?: 0L
    return AndroidBugReportThread(
        id = id.take(160),
        title = json.serverText("title").orEmpty().ifBlank { "Bug report" }.take(180),
        description = json.serverText("description").orEmpty().take(8_000),
        status = json.serverText("status").orEmpty().ifBlank { "open" }.take(40),
        resolutionNote = json.serverText("resolutionNote").orEmpty().take(3_000),
        versionName = json.serverText("versionName").orEmpty().take(80),
        versionCode = json.serverText("versionCode").orEmpty().take(40),
        kind = json.serverText("kind").orEmpty().ifBlank { "bug" }.take(40),
        area = json.serverText("area").orEmpty().ifBlank { "other" }.take(40),
        frequency = json.serverText("frequency").orEmpty().ifBlank { "not_sure" }.take(40),
        impact = json.serverText("impact").orEmpty().ifBlank { "normal" }.take(40),
        files = json["files"]?.jsonArray.orEmpty().mapNotNull { element ->
            runCatching { element.jsonObject.serverText("name")?.take(180) }.getOrNull()
        },
        createdAt = createdAt,
        updatedAt = json.serverLong("updatedAt") ?: createdAt,
        comments = json["comments"]?.jsonArray.orEmpty().mapNotNull { element ->
            val comment = runCatching { element.jsonObject }.getOrNull() ?: return@mapNotNull null
            val commentId = comment.serverText("id")?.takeIf(String::isNotBlank) ?: return@mapNotNull null
            val commentBody = comment.serverText("body")?.takeIf(String::isNotBlank) ?: return@mapNotNull null
            AndroidBugReportComment(
                id = commentId.take(160),
                authorRole = comment.serverText("authorRole").orEmpty().ifBlank { "admin" }.take(20),
                kind = comment.serverText("kind").orEmpty().ifBlank { "comment" }.take(20),
                body = commentBody.take(3_000),
                createdAt = comment.serverLong("createdAt") ?: 0L,
            )
        },
    )
}

private fun JsonObject.serverText(key: String): String? =
    get(key)?.let { runCatching { it.jsonPrimitive.contentOrNull }.getOrNull() }?.trim()

private fun JsonObject.serverLong(key: String): Long? =
    get(key)?.let { runCatching { it.jsonPrimitive.longOrNull }.getOrNull() }
