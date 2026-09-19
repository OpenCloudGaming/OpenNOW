package com.opencloudgaming.opennow

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.MultipartBody
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.security.MessageDigest

internal const val ANDROID_BUG_REPORT_ENDPOINT =
    "https://api.printedwaste.com/releases/opennow/bug-reports"
internal const val ANDROID_BUG_REPORT_MAX_FILES = 5
internal const val ANDROID_BUG_REPORT_MAX_FILE_BYTES = 10L * 1024L * 1024L
internal const val ANDROID_BUG_REPORT_REPORTER_ID_PREFIX = "br1_"
internal const val ANDROID_BUG_REPORT_NVST_WARNING =
    "NVST is experimental, so bugs are expected while it is enabled. Bug reports containing rude, insulting, or abusive language will be ignored."

enum class AndroidBugReportVersionCheckStatus {
    NotChecked,
    Checking,
    Current,
    UpdateRequired,
    CheckFailed,
}

data class AndroidBugReportVersionCheckState(
    val status: AndroidBugReportVersionCheckStatus = AndroidBugReportVersionCheckStatus.NotChecked,
    val message: String? = null,
)

internal fun androidBugReportNvstWarning(experimentalNvstEnabled: Boolean): String? =
    ANDROID_BUG_REPORT_NVST_WARNING.takeIf { experimentalNvstEnabled }

internal fun androidBugReportsAllowed(
    update: AndroidUpdateState,
    versionCheck: AndroidBugReportVersionCheckState,
): Boolean {
    if (!update.installSource.isGooglePlay) return true
    return versionCheck.status == AndroidBugReportVersionCheckStatus.Current &&
        update.status == AndroidUpdateStatus.NotAvailable
}

internal fun androidBugReportBlockMessage(
    update: AndroidUpdateState,
    versionCheck: AndroidBugReportVersionCheckState,
): String? {
    if (!update.installSource.isGooglePlay) return null
    return when {
        update.status == AndroidUpdateStatus.Available ||
            versionCheck.status == AndroidBugReportVersionCheckStatus.UpdateRequired ->
            "Update OpenNOW from Google Play before sending a bug report. This keeps reports tied to the latest supported build."
        versionCheck.status == AndroidBugReportVersionCheckStatus.CheckFailed ->
            versionCheck.message ?: "OpenNOW could not verify the latest Google Play version. Retry the check before reporting."
        versionCheck.status == AndroidBugReportVersionCheckStatus.Checking ->
            "Checking Google Play for a newer OpenNOW build before bug reporting is enabled."
        versionCheck.status == AndroidBugReportVersionCheckStatus.Current &&
            update.status == AndroidUpdateStatus.NotAvailable -> null
        versionCheck.status == AndroidBugReportVersionCheckStatus.Current ->
            "OpenNOW could not confirm that this is still the latest Google Play build. Retry the check before reporting."
        else -> "Check Google Play for updates before sending a bug report."
    }
}

internal data class AndroidBugReportAttachment(
    val fileName: String,
    val contentType: String,
    val bytes: ByteArray,
)

internal data class AndroidBugReport(
    val title: String,
    val description: String,
    val versionName: String,
    val versionCode: String,
    val reporterId: String,
    val appLanguageSelectionTag: String,
    val languageCheck: AndroidBugReportLanguageCheck,
    val metadata: String,
    val files: List<AndroidBugReportAttachment>,
    val details: AndroidBugReportDetails = AndroidBugReportDetails(),
)

internal data class AndroidBugReportReceipt(
    val reference: String,
)

internal data class AndroidBugReportServerError(
    val code: String?,
    val message: String,
    val retryable: Boolean?,
)

internal class AndroidBugReportUploadException(
    val serverCode: String?,
    val retryable: Boolean?,
    message: String,
) : IllegalStateException(message)

/**
 * Stable, installation-scoped abuse-prevention key. The raw GFN device ID is deliberately never
 * uploaded: a namespaced SHA-256 digest keeps bug reports unlinkable to the provider credential
 * while still giving the report service a consistent value to rate-limit or block.
 */
internal fun androidBugReportReporterId(stableDeviceId: String): String {
    require(stableDeviceId.isNotBlank()) { "Bug report installation ID is unavailable" }
    val digest = MessageDigest.getInstance("SHA-256")
        .digest("opennow-android-bug-report-v1:$stableDeviceId".toByteArray(Charsets.UTF_8))
    return ANDROID_BUG_REPORT_REPORTER_ID_PREFIX + digest.joinToString("") { "%02x".format(it) }
}

internal fun buildAndroidBugReportMetadata(
    logFileName: String,
    knownIssueOverrideKey: String? = null,
    device: AndroidDeviceDiagnosticsSnapshot? = null,
): String = buildJsonObject {
    put("source", "settings-advanced-debug-logs")
    put("attachment", logFileName)
    device?.let { snapshot ->
        put("device", buildJsonObject {
            put("manufacturer", snapshot.manufacturer)
            put("brand", snapshot.brand)
            put("model", snapshot.model)
            put("codename", snapshot.deviceCodename)
            put("product", snapshot.product)
            put("formFactor", snapshot.formFactor)
            put("emulator", snapshot.emulator)
        })
        put("android", buildJsonObject {
            put("release", snapshot.androidRelease)
            put("codename", snapshot.androidCodename)
            put("sdk", snapshot.androidSdk)
            put("targetSdk", snapshot.targetSdk)
            put("securityPatch", snapshot.securityPatch)
        })
        put("hardware", buildJsonObject {
            put("name", snapshot.hardware)
            put("board", snapshot.board)
            put("supportedAbis", buildJsonArray {
                snapshot.supportedAbis.forEach { add(JsonPrimitive(it)) }
            })
            put("runtimeBits", if (snapshot.is64BitRuntime) 64 else 32)
            put("processorCount", snapshot.processorCount)
            snapshot.totalMemoryMiB?.let { put("totalMemoryMiB", it) }
            snapshot.lowRamDevice?.let { put("lowRamDevice", it) }
        })
        put("display", buildJsonObject {
            put("widthPixels", snapshot.displayWidthPixels)
            put("heightPixels", snapshot.displayHeightPixels)
            put("densityDpi", snapshot.densityDpi)
            put("smallestWidthDp", snapshot.smallestScreenWidthDp)
        })
    }
    knownIssueOverrideKey?.trim()?.takeIf { it.isNotEmpty() }?.let { key ->
        put("knownIssueOverride", true)
        put("knownIssueKey", key)
    }
}.toString()

internal fun buildAndroidBugReportRequest(
    report: AndroidBugReport,
    endpoint: String = ANDROID_BUG_REPORT_ENDPOINT,
): Request {
    val title = report.title.trim()
    val description = report.description.trim()
    androidBugReportTitleError(title)?.let { error -> throw IllegalArgumentException(error) }
    androidBugReportDescriptionError(description)?.let { error -> throw IllegalArgumentException(error) }
    require(androidAppLocaleIsEnglish(report.appLanguageSelectionTag)) {
        "Set the OpenNOW or device language to English before sending a bug report"
    }
    androidBugReportLanguageError(
        listOf(
            AndroidBugReportLanguageCandidate(
                languageTag = report.languageCheck.languageTag,
                confidence = report.languageCheck.confidence,
            ),
        ),
    )?.let { error -> throw IllegalArgumentException(error) }
    require(report.versionName.isNotBlank()) { "App version is unavailable" }
    require(report.versionCode.isNotBlank()) { "App build is unavailable" }
    require(report.reporterId.matches(ANDROID_BUG_REPORT_REPORTER_ID_REGEX)) {
        "Bug report installation ID is invalid"
    }
    require(report.files.size <= ANDROID_BUG_REPORT_MAX_FILES) {
        "Bug reports support up to $ANDROID_BUG_REPORT_MAX_FILES files"
    }
    runCatching { OpenNowJson.parseToJsonElement(report.metadata).jsonObject }
        .getOrElse { throw IllegalArgumentException("Bug report metadata must be a JSON object", it) }

    val multipart = MultipartBody.Builder()
        .setType(MultipartBody.FORM)
        .addFormDataPart("title", title)
        .addFormDataPart("description", description)
        .addFormDataPart("versionName", report.versionName)
        .addFormDataPart("versionCode", report.versionCode)
        .addFormDataPart("platform", "android")
        .addFormDataPart("kind", report.details.kind.wireValue)
        .addFormDataPart("area", report.details.area.wireValue)
        .addFormDataPart("frequency", report.details.frequency.wireValue)
        .addFormDataPart("impact", report.details.impact.wireValue)
        .addFormDataPart("reporterId", report.reporterId)
        .addFormDataPart("termsAccepted", "true")
        .addFormDataPart("termsVersion", ANDROID_BUG_REPORT_TERMS_VERSION)
        .addFormDataPart("metadata", report.metadata)

    report.files.forEach { attachment ->
        require(attachment.fileName.isNotBlank()) { "Bug report files must have a name" }
        require(attachment.bytes.size.toLong() <= ANDROID_BUG_REPORT_MAX_FILE_BYTES) {
            "${attachment.fileName} is larger than 10 MiB"
        }
        val mediaType = attachment.contentType.toMediaType()
        multipart.addFormDataPart(
            "files",
            attachment.fileName,
            attachment.bytes.toRequestBody(mediaType),
        )
    }

    return Request.Builder()
        .url(endpoint)
        .header("Accept", "application/json")
        .post(multipart.build())
        .build()
}

internal suspend fun uploadAndroidBugReport(
    http: OkHttpClient,
    report: AndroidBugReport,
): AndroidBugReportReceipt = withContext(Dispatchers.IO) {
    http.newCall(buildAndroidBugReportRequest(report)).execute().use { response ->
        val body = response.body.string().take(MAX_BUG_REPORT_RESPONSE_CHARS)
        if (!response.isSuccessful) {
            val serverError = parseAndroidBugReportServerError(body, response.code)
            throw AndroidBugReportUploadException(
                serverCode = serverError.code,
                retryable = serverError.retryable,
                message = serverError.message,
            )
        }
        if (androidBugReportResponseExplicitlyRejected(body)) {
            val serverError = parseAndroidBugReportServerError(body, response.code)
            throw AndroidBugReportUploadException(
                serverCode = serverError.code,
                retryable = serverError.retryable,
                message = serverError.message,
            )
        }
        parseAndroidBugReportReceipt(body)
    }
}

internal fun parseAndroidBugReportReceipt(body: String): AndroidBugReportReceipt =
    AndroidBugReportReceipt(
        reference = parseAndroidBugReportReference(body)
            ?: throw AndroidBugReportUploadException(
                serverCode = "INVALID_RESPONSE",
                retryable = false,
                message = "The bug report service did not return a report ID.",
            ),
    )

internal fun parseAndroidBugReportReference(body: String): String? = runCatching {
    val json = OpenNowJson.parseToJsonElement(body).jsonObject
    listOf("id", "reportId", "bugReportId")
        .firstNotNullOfOrNull { key ->
            json[key]?.jsonPrimitive?.contentOrNull
                ?.trim()
                ?.take(MAX_BUG_REPORT_REFERENCE_CHARS)
                ?.takeIf(String::isNotBlank)
        }
}.getOrNull()

internal fun parseAndroidBugReportServerError(
    body: String,
    statusCode: Int,
): AndroidBugReportServerError {
    val root = parseBugReportJsonObject(body)
    val error = root?.get("error")?.let { element ->
        runCatching { element.jsonObject }.getOrNull()
    }
    val payload = error ?: root
    val customMessage = payload?.serverString("message")
        ?.replace(BUG_REPORT_RESPONSE_WHITESPACE, " ")
        ?.trim()
        ?.take(MAX_BUG_REPORT_PUBLIC_MESSAGE_CHARS)
        ?.takeIf(String::isNotBlank)
    return AndroidBugReportServerError(
        code = payload?.serverString("code")?.take(MAX_BUG_REPORT_SERVER_CODE_CHARS),
        message = customMessage ?: when (statusCode) {
            403 -> "Bug reporting is unavailable for this installation."
            429 -> "Too many bug reports were sent. Try again later."
            else -> "Bug report upload failed (HTTP $statusCode)."
        },
        retryable = payload?.get("retryable")?.let { element ->
            runCatching { element.jsonPrimitive.booleanOrNull }.getOrNull()
        },
    )
}

private fun androidBugReportResponseExplicitlyRejected(body: String): Boolean =
    (parseBugReportJsonObject(body)
        ?.get("ok")
        ?.let { element -> runCatching { element.jsonPrimitive.booleanOrNull }.getOrNull() }) == false

private fun parseBugReportJsonObject(body: String): JsonObject? = runCatching {
    OpenNowJson.parseToJsonElement(body).jsonObject
}.getOrNull()

private fun JsonObject.serverString(key: String): String? =
    get(key)?.let { element ->
        runCatching { element.jsonPrimitive.contentOrNull }.getOrNull()
    }?.takeIf(String::isNotBlank)

private const val MAX_BUG_REPORT_RESPONSE_CHARS = 64 * 1024
private const val MAX_BUG_REPORT_PUBLIC_MESSAGE_CHARS = 320
private const val MAX_BUG_REPORT_SERVER_CODE_CHARS = 80
private const val MAX_BUG_REPORT_REFERENCE_CHARS = 160
private val ANDROID_BUG_REPORT_REPORTER_ID_REGEX = Regex("^br1_[0-9a-f]{64}$")
private val BUG_REPORT_RESPONSE_WHITESPACE = Regex("\\s+")
