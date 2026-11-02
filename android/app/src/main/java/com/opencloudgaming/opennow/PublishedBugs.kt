package com.opencloudgaming.opennow

import androidx.compose.runtime.Immutable
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import kotlinx.serialization.json.put
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody

internal const val ANDROID_PUBLISHED_BUGS_ENDPOINT = "https://api.printedwaste.com/releases/opennow/published-bugs"

@Immutable
data class AndroidPublishedBug(
    val id: String,
    val title: String,
    val body: String,
    val status: String,
    val progress: Int,
    val fixedVersion: String,
    val votes: Int,
    val voted: Boolean,
    val logCount: Int,
    val updatedAt: Long,
    val acceptingLogs: Boolean = true,
)

internal data class AndroidPublishedBugPage(val bugs: List<AndroidPublishedBug>, val nextCursor: String?)

@Immutable
data class AndroidPublishedBugsState(
    val bugs: List<AndroidPublishedBug> = emptyList(),
    val loading: Boolean = false,
    val nextCursor: String? = null,
    val actingBugId: String? = null,
    val uploadingLogs: Boolean = false,
    val error: String? = null,
    val uploadedBugId: String? = null,
)

internal fun parseAndroidPublishedBug(json: JsonObject): AndroidPublishedBug {
    fun text(key: String) = json[key]?.jsonPrimitive?.contentOrNull.orEmpty()
    val id = text("id")
    require(id.isNotBlank() && id.length <= 160)
    return AndroidPublishedBug(
        id = id,
        title = text("title").take(180),
        body = text("body").take(4000),
        status = text("status").take(40),
        progress = (json["progress"]?.jsonPrimitive?.intOrNull ?: 0).coerceIn(0, 100),
        fixedVersion = text("fixedVersion").take(40),
        votes = (json["votes"]?.jsonPrimitive?.intOrNull ?: 0).coerceAtLeast(0),
        voted = json["voted"]?.jsonPrimitive?.booleanOrNull == true,
        logCount = (json["logCount"]?.jsonPrimitive?.intOrNull ?: 0).coerceAtLeast(0),
        updatedAt = json["updatedAt"]?.jsonPrimitive?.longOrNull ?: 0,
        acceptingLogs = json["acceptingLogs"]?.jsonPrimitive?.booleanOrNull != false,
    )
}

internal fun parseAndroidPublishedBugPage(body: String): AndroidPublishedBugPage {
    try {
        val data = OpenNowJson.parseToJsonElement(body).jsonObject["data"]!!.jsonObject
        val bugs = data["bugs"]!!.jsonArray.map { parseAndroidPublishedBug(it.jsonObject) }
        return AndroidPublishedBugPage(bugs, data["nextCursor"]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() })
    } catch (_: Exception) {
        throw AndroidBugReportUploadException("INVALID_RESPONSE", true, "Could not load published bugs. Please try again.")
    }
}

private fun publishedBugRequest(reporterId: String, endpoint: String): Request.Builder {
    require(reporterId.matches(Regex("^br1_[0-9a-f]{64}$"))) { "Bug report installation ID is invalid" }
    return Request.Builder().url(endpoint).header("Accept", "application/json")
        .header(ANDROID_BUG_REPORT_REPORTER_HEADER, reporterId)
}

private suspend fun publishedBugResponse(http: OkHttpClient, request: Request): String = withContext(Dispatchers.IO) {
    http.newCall(request).execute().use { response ->
        val source = response.body.source()
        val limit = 512L * 1024
        source.request(limit + 1)
        if (source.buffer.size > limit) throw AndroidBugReportUploadException("INVALID_RESPONSE", false, "The published bugs response was too large.")
        val body = source.readUtf8()
        if (!response.isSuccessful) {
            val error = parseAndroidBugReportServerError(body, response.code)
            throw AndroidBugReportUploadException(error.code, error.retryable, error.message)
        }
        val root = runCatching { OpenNowJson.parseToJsonElement(body).jsonObject }.getOrNull()
        if (root?.get("ok")?.jsonPrimitive?.booleanOrNull != true) {
            throw AndroidBugReportUploadException("INVALID_RESPONSE", true, "Could not complete the request. Please try again.")
        }
        body
    }
}

internal suspend fun fetchAndroidPublishedBugs(http: OkHttpClient, reporterId: String, cursor: String? = null, endpoint: String = ANDROID_PUBLISHED_BUGS_ENDPOINT): AndroidPublishedBugPage {
    val url = endpoint.toHttpUrl().newBuilder().apply { if (cursor != null) addQueryParameter("cursor", cursor) }.build()
    return parseAndroidPublishedBugPage(publishedBugResponse(http, publishedBugRequest(reporterId, url.toString()).get().build()))
}

internal suspend fun voteAndroidPublishedBug(http: OkHttpClient, reporterId: String, bugId: String, voted: Boolean, endpoint: String = ANDROID_PUBLISHED_BUGS_ENDPOINT): AndroidPublishedBug {
    val url = endpoint.toHttpUrl().newBuilder().addPathSegment(bugId).addPathSegment("vote").build()
    val body = buildJsonObject { put("voted", voted) }.toString().toRequestBody("application/json".toMediaType())
    val response = publishedBugResponse(http, publishedBugRequest(reporterId, url.toString()).put(body).build())
    return parseAndroidPublishedBug(OpenNowJson.parseToJsonElement(response).jsonObject["data"]!!.jsonObject)
}

internal fun buildPublishedBugLogRequest(reporterId: String, bugId: String, submissionId: String, file: AndroidBugReportAttachment, endpoint: String = ANDROID_PUBLISHED_BUGS_ENDPOINT): Request {
    require(file.bytes.isNotEmpty() && file.bytes.size <= ANDROID_BUG_REPORT_MAX_FILE_BYTES) { "Diagnostics must be between 1 byte and 10 MB" }
    val url = endpoint.toHttpUrl().newBuilder().addPathSegment(bugId).addPathSegment("logs").build()
    val body = MultipartBody.Builder().setType(MultipartBody.FORM)
        .addFormDataPart("submissionId", submissionId)
        .addFormDataPart("termsAccepted", "true")
        .addBugReportAttachments(listOf(file)).build()
    return publishedBugRequest(reporterId, url.toString()).post(body).build()
}

internal suspend fun uploadAndroidPublishedBugLog(http: OkHttpClient, reporterId: String, bugId: String, submissionId: String, file: AndroidBugReportAttachment) {
    publishedBugResponse(http, buildPublishedBugLogRequest(reporterId, bugId, submissionId, file))
}
