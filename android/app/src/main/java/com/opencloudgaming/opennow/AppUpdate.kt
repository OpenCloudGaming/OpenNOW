package com.opencloudgaming.opennow

import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import android.provider.Settings
import androidx.core.content.FileProvider
import com.google.android.play.core.appupdate.AppUpdateInfo
import com.google.android.play.core.appupdate.AppUpdateManagerFactory
import com.google.android.play.core.install.model.UpdateAvailability
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.ensureActive
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.coroutines.withContext
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import okhttp3.HttpUrl
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.security.MessageDigest
import java.util.Locale
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

private const val APK_MIME_TYPE = "application/vnd.android.package-archive"
internal const val ANDROID_UPDATE_SOURCE_URL = "https://api.printedwaste.com/releases/opennow/latest"
internal const val GOOGLE_PLAY_STORE_PACKAGE = "com.android.vending"
internal const val GOOGLE_PLAY_STORE_LISTING_URL = "https://play.google.com/store/apps/details?id=${BuildConfig.APPLICATION_ID}"
private const val UPDATE_FILE_PROVIDER_AUTHORITY_SUFFIX = ".updates"
private val UPDATE_USER_AGENT = "OpenNOW-AndroidUpdater/${BuildConfig.VERSION_NAME}"
private val KNOWN_PACKAGE_INSTALLER_GRANT_TARGETS = setOf(
    "com.android.packageinstaller",
    "com.google.android.packageinstaller",
    "com.android.vending",
)

enum class AndroidUpdateStatus {
    Idle,
    Checking,
    Available,
    NotAvailable,
    Downloading,
    Downloaded,
    Error,
}

data class AndroidAppInstallSource(
    val installerPackageNames: Set<String> = emptySet(),
    val apkUpdatesSupportedByBuild: Boolean = BuildConfig.APK_UPDATES_SUPPORTED,
    val playStoreReleaseBuild: Boolean = BuildConfig.PLAY_STORE_RELEASE,
) {
    val isGooglePlay: Boolean
        get() = installerPackageNames.any(::isGooglePlayInstallerPackage)

    val usesGooglePlayUpdates: Boolean
        get() = playStoreReleaseBuild || isGooglePlay

    val distributionKind: String
        get() = if (isGooglePlay) "play-store" else "apk"

    val buildDistributionKind: String
        get() = if (playStoreReleaseBuild) "play-release" else "apk"

    val allowsApkUpdates: Boolean
        get() = apkUpdatesSupportedByBuild && !usesGooglePlayUpdates

    val displayName: String
        get() = when {
            isGooglePlay -> "Google Play"
            installerPackageNames.isEmpty() -> "Sideloaded"
            else -> installerPackageNames.sorted().joinToString(", ")
        }
}

internal fun AndroidUpdateState.debugHeaderLine(debugBuild: Boolean = BuildConfig.DEBUG): String {
    val variant = if (debugBuild) "debug" else "release"
    return listOf(
        "app.version=$currentVersionName",
        "build=$currentVersionCode",
        "variant=$variant",
        "distribution=${installSource.distributionKind}",
        "installSource=${installSource.displayName}",
        "buildDistribution=${installSource.buildDistributionKind}",
        "apkUpdatesAllowed=$apkUpdatesAllowed",
    ).joinToString(" ")
}

data class AndroidUpdateProgress(
    val percent: Int?,
    val transferredBytes: Long,
    val totalBytes: Long?,
)

data class AndroidUpdateState(
    val status: AndroidUpdateStatus = AndroidUpdateStatus.Idle,
    val currentVersionName: String = BuildConfig.VERSION_NAME,
    val currentVersionCode: Long = BuildConfig.VERSION_CODE.toLong(),
    val sourceUrl: String = ANDROID_UPDATE_SOURCE_URL,
    val installSource: AndroidAppInstallSource = AndroidAppInstallSource(),
    val availableVersionName: String? = null,
    val availableVersionCode: Long? = null,
    val releaseNotes: String? = null,
    val downloadedFileName: String? = null,
    val progress: AndroidUpdateProgress? = null,
    val message: String = "Ready to check for updates.",
    val lastCheckedAt: Long? = null,
) {
    val apkUpdatesAllowed: Boolean
        get() = installSource.allowsApkUpdates

    val updateChecksSupported: Boolean
        get() = apkUpdatesAllowed || installSource.usesGooglePlayUpdates

    val canCheck: Boolean
        get() = updateChecksSupported && status != AndroidUpdateStatus.Checking && status != AndroidUpdateStatus.Downloading

    val canDownload: Boolean
        get() = apkUpdatesAllowed && status == AndroidUpdateStatus.Available

    val canInstall: Boolean
        get() = apkUpdatesAllowed && status == AndroidUpdateStatus.Downloaded

    val canOpenPlayStore: Boolean
        get() = installSource.usesGooglePlayUpdates && status == AndroidUpdateStatus.Available
}

internal fun AndroidUpdateState.shouldRunAutomaticCheck(): Boolean {
    if (!updateChecksSupported) return false
    return when (status) {
        AndroidUpdateStatus.Checking,
        AndroidUpdateStatus.Available,
        AndroidUpdateStatus.Downloading,
        AndroidUpdateStatus.Downloaded -> false
        else -> true
    }
}

internal fun androidUpdateNoticeKey(update: AndroidUpdateState): String? =
    if (!update.updateChecksSupported) {
        null
    } else when (update.status) {
        AndroidUpdateStatus.Available,
        AndroidUpdateStatus.Downloading,
        AndroidUpdateStatus.Downloaded,
        -> listOfNotNull(
            update.availableVersionCode?.let { "code:$it" },
            update.availableVersionName?.takeIf { it.isNotBlank() }?.let { "name:$it" },
        ).takeIf { it.isNotEmpty() }?.joinToString("|")
            ?: update.sourceUrl.takeIf { it.isNotBlank() }?.let { "source:$it" }
        else -> null
    }

internal fun androidUpdateUnavailableMessage(installSource: AndroidAppInstallSource): String =
    when {
        installSource.usesGooglePlayUpdates -> "Ready to check Google Play for updates."
        !installSource.apkUpdatesSupportedByBuild -> "APK self-updates are disabled in this Play release."
        else -> "Ready to check for sideload APK updates."
    }

internal fun AndroidUpdateState.visibleNoticeKey(dismissedKey: String?): String? =
    androidUpdateNoticeKey(this)?.takeUnless { it == dismissedKey }

internal data class AndroidUpdateCandidate(
    val sourceUrl: String,
    val apkUrl: String,
    val versionName: String?,
    val versionCode: Long?,
    val sha256: String?,
    val releaseNotes: String?,
    val fileName: String?,
) {
    val displayVersion: String
        get() = versionName ?: versionCode?.toString() ?: "APK update"
}

class AndroidAppUpdater(
    private val context: Context,
    private val http: OkHttpClient,
) {
    private val appContext = context.applicationContext
    private val installSource = detectAndroidAppInstallSource(appContext)
    private val playAppUpdateManager by lazy { AppUpdateManagerFactory.create(appContext) }
    private val _state = MutableStateFlow(
        AndroidUpdateState(
            installSource = installSource,
            message = androidUpdateUnavailableMessage(installSource),
        ),
    )
    val state: StateFlow<AndroidUpdateState> = _state

    private var latestCandidate: AndroidUpdateCandidate? = null
    private var downloadedApk: File? = null

    suspend fun checkForUpdate(sourceUrl: String = ANDROID_UPDATE_SOURCE_URL) {
        if (_state.value.installSource.usesGooglePlayUpdates) {
            checkForPlayStoreUpdate()
            return
        }
        if (!_state.value.apkUpdatesAllowed) {
            publishApkUpdatesUnavailable()
            return
        }
        val normalizedSourceUrl = runCatching { normalizeAndroidUpdateSourceUrl(sourceUrl) }.getOrElse { error ->
            publishError(sourceUrl, error.message ?: "Update source URL is invalid.")
            return
        }
        withContext(Dispatchers.IO) {
            publish(
                status = AndroidUpdateStatus.Checking,
                sourceUrl = normalizedSourceUrl,
                message = "Checking update source...",
                progress = null,
                clearCandidate = true,
            )
            try {
                val candidate = fetchCandidate(normalizedSourceUrl)
                ensureActive()
                latestCandidate = candidate
                downloadedApk = null
                val checkedAt = System.currentTimeMillis()
                if (candidate.versionCode != null && candidate.versionCode <= BuildConfig.VERSION_CODE.toLong()) {
                    publish(
                        status = AndroidUpdateStatus.NotAvailable,
                        sourceUrl = normalizedSourceUrl,
                        message = "OpenNOW Android is up to date.",
                        availableVersionName = candidate.versionName,
                        availableVersionCode = candidate.versionCode,
                        releaseNotes = candidate.releaseNotes,
                        lastCheckedAt = checkedAt,
                    )
                } else {
                    val compareHint = if (candidate.versionCode == null) {
                        " Version could not be compared, so only download this source if you trust it."
                    } else {
                        ""
                    }
                    publish(
                        status = AndroidUpdateStatus.Available,
                        sourceUrl = normalizedSourceUrl,
                        message = "OpenNOW ${candidate.displayVersion} is available to download.$compareHint",
                        availableVersionName = candidate.versionName,
                        availableVersionCode = candidate.versionCode,
                        releaseNotes = candidate.releaseNotes,
                        lastCheckedAt = checkedAt,
                    )
                }
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                ensureActive()
                publishError(normalizedSourceUrl, error.message ?: "Update check failed.")
            }
        }
    }

    fun markCheckDeferredForStreaming() {
        if (!_state.value.updateChecksSupported) {
            publishApkUpdatesUnavailable()
            return
        }
        val current = _state.value
        when (current.status) {
            AndroidUpdateStatus.Available,
            AndroidUpdateStatus.Downloading,
            AndroidUpdateStatus.Downloaded -> return
            else -> Unit
        }
        _state.value = current.copy(
            status = if (current.status == AndroidUpdateStatus.Checking) AndroidUpdateStatus.Idle else current.status,
            message = "Update checks pause while streaming.",
            progress = null,
        )
    }

    fun openPlayStoreListing() {
        if (!_state.value.installSource.usesGooglePlayUpdates) return
        val marketIntent = Intent(
            Intent.ACTION_VIEW,
            Uri.parse("market://details?id=${BuildConfig.APPLICATION_ID}"),
        ).apply {
            setPackage(GOOGLE_PLAY_STORE_PACKAGE)
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        val webIntent = Intent(Intent.ACTION_VIEW, Uri.parse(GOOGLE_PLAY_STORE_LISTING_URL))
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        try {
            appContext.startActivity(marketIntent)
        } catch (_: ActivityNotFoundException) {
            runCatching { appContext.startActivity(webIntent) }
                .onFailure { error ->
                    publishError(
                        GOOGLE_PLAY_STORE_LISTING_URL,
                        error.message ?: "Google Play could not be opened.",
                    )
                }
        } catch (error: SecurityException) {
            runCatching { appContext.startActivity(webIntent) }
                .onFailure {
                    publishError(
                        GOOGLE_PLAY_STORE_LISTING_URL,
                        error.message ?: "Android blocked access to Google Play.",
                    )
                }
        }
    }

    suspend fun downloadUpdate(sourceUrl: String = ANDROID_UPDATE_SOURCE_URL) {
        if (!_state.value.apkUpdatesAllowed) {
            publishApkUpdatesUnavailable()
            return
        }
        val normalizedSourceUrl = runCatching { normalizeAndroidUpdateSourceUrl(sourceUrl) }.getOrElse { error ->
            publishError(sourceUrl, error.message ?: "Update source URL is invalid.")
            return
        }
        withContext(Dispatchers.IO) {
            val candidate = latestCandidate
                ?.takeIf { it.sourceUrl == normalizedSourceUrl }
                ?: runCatching { fetchCandidate(normalizedSourceUrl) }.getOrElse { error ->
                    publishError(normalizedSourceUrl, error.message ?: "Update check failed.")
                    return@withContext
                }
            latestCandidate = candidate
            publish(
                status = AndroidUpdateStatus.Downloading,
                sourceUrl = normalizedSourceUrl,
                message = "Downloading OpenNOW ${candidate.displayVersion}...",
                availableVersionName = candidate.versionName,
                availableVersionCode = candidate.versionCode,
                releaseNotes = candidate.releaseNotes,
                progress = AndroidUpdateProgress(percent = 0, transferredBytes = 0, totalBytes = null),
            )

            runCatching {
                downloadCandidate(candidate)
            }.onSuccess { apk ->
                downloadedApk = apk
                publish(
                    status = AndroidUpdateStatus.Downloaded,
                    sourceUrl = normalizedSourceUrl,
                    message = "Downloaded ${apk.name}. Android will ask you to confirm the install.",
                    availableVersionName = candidate.versionName,
                    availableVersionCode = candidate.versionCode,
                    releaseNotes = candidate.releaseNotes,
                    downloadedFileName = apk.name,
                    progress = null,
                )
            }.onFailure { error ->
                publishError(normalizedSourceUrl, error.message ?: "Update download failed.")
            }
        }
    }

    @Suppress("DEPRECATION")
    fun installDownloadedUpdate() {
        if (!_state.value.apkUpdatesAllowed) {
            publishApkUpdatesUnavailable()
            return
        }
        val apk = downloadedApk?.takeIf { it.exists() && it.isFile } ?: run {
            publishError(_state.value.sourceUrl, "Downloaded APK is no longer available.")
            return
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O && !appContext.packageManager.canRequestPackageInstalls()) {
            val settingsIntent = Intent(
                Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES,
                Uri.parse("package:${BuildConfig.APPLICATION_ID}"),
            ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            runCatching { appContext.startActivity(settingsIntent) }
            _state.value = _state.value.copy(
                status = AndroidUpdateStatus.Downloaded,
                message = "Allow OpenNOW to install unknown apps, then tap Install again.",
            )
            return
        }

        val uri = FileProvider.getUriForFile(appContext, updateFileProviderAuthority(), apk)
        val installIntent = Intent(Intent.ACTION_INSTALL_PACKAGE)
            .setDataAndType(uri, APK_MIME_TYPE)
            .putExtra(Intent.EXTRA_NOT_UNKNOWN_SOURCE, true)
            .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        installIntent.clipData = ClipData.newRawUri("OpenNOW update", uri)
        try {
            grantInstallUriPermissions(uri, installIntent)
            appContext.startActivity(installIntent)
            _state.value = _state.value.copy(
                status = AndroidUpdateStatus.Downloaded,
                message = "Android package installer opened.",
            )
        } catch (error: ActivityNotFoundException) {
            publishError(_state.value.sourceUrl, error.message ?: "No package installer is available.")
        } catch (error: SecurityException) {
            publishError(_state.value.sourceUrl, error.message ?: "Android blocked package install access.")
        }
    }

    private suspend fun checkForPlayStoreUpdate() {
        publish(
            status = AndroidUpdateStatus.Checking,
            sourceUrl = GOOGLE_PLAY_STORE_LISTING_URL,
            message = "Checking Google Play...",
            progress = null,
            clearCandidate = true,
        )
        try {
            val updateInfo = awaitPlayStoreUpdateInfo()
            val currentBuild = BuildConfig.VERSION_CODE.toLong()
            val availableBuild = playStoreAvailableVersionCode(
                currentVersionCode = currentBuild,
                updateAvailability = updateInfo.updateAvailability(),
                availableVersionCode = updateInfo.availableVersionCode(),
            )
            val checkedAt = System.currentTimeMillis()
            if (availableBuild != null) {
                publish(
                    status = AndroidUpdateStatus.Available,
                    sourceUrl = GOOGLE_PLAY_STORE_LISTING_URL,
                    message = "Installed build $currentBuild is older than Google Play build $availableBuild.",
                    availableVersionCode = availableBuild,
                    lastCheckedAt = checkedAt,
                )
            } else {
                publish(
                    status = AndroidUpdateStatus.NotAvailable,
                    sourceUrl = GOOGLE_PLAY_STORE_LISTING_URL,
                    message = "OpenNOW is up to date on Google Play (build $currentBuild).",
                    availableVersionCode = currentBuild,
                    lastCheckedAt = checkedAt,
                )
            }
        } catch (error: Throwable) {
            if (error is CancellationException) throw error
            publishError(
                GOOGLE_PLAY_STORE_LISTING_URL,
                error.message ?: "Google Play update check failed.",
            )
        }
    }

    private suspend fun awaitPlayStoreUpdateInfo(): AppUpdateInfo =
        suspendCancellableCoroutine { continuation ->
            playAppUpdateManager.appUpdateInfo
                .addOnSuccessListener { updateInfo ->
                    if (continuation.isActive) continuation.resume(updateInfo)
                }
                .addOnFailureListener { error ->
                    if (continuation.isActive) continuation.resumeWithException(error)
                }
        }

    private fun fetchCandidate(sourceUrl: String): AndroidUpdateCandidate {
        val request = Request.Builder()
            .url(sourceUrl)
            .header("User-Agent", UPDATE_USER_AGENT)
            .build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                error("Update source returned HTTP ${response.code}.")
            }
            val contentType = response.header("Content-Type").orEmpty()
            if (looksLikeApk(sourceUrl, contentType)) {
                return directApkCandidate(sourceUrl, response.header("X-OpenNOW-Version-Name"), response.header("X-OpenNOW-Version-Code")?.toLongOrNull(), response.header("X-OpenNOW-SHA256"))
            }
            val body = response.body.string().takeIf { it.isNotBlank() }
                ?: error("Update source returned an empty manifest.")
            return parseAndroidUpdateCandidate(sourceUrl, body)
                ?: error("Update manifest must provide versionCode/versionName and an apkUrl.")
        }
    }

    private fun downloadCandidate(candidate: AndroidUpdateCandidate): File {
        val request = Request.Builder()
            .url(candidate.apkUrl)
            .header("User-Agent", UPDATE_USER_AGENT)
            .build()
        http.newCall(request).execute().use { response ->
            if (!response.isSuccessful) {
                error("APK download returned HTTP ${response.code}.")
            }
            val body = response.body
            val totalBytes = body.contentLength().takeIf { it > 0 }
            val updatesDir = androidUpdateStorageDir(appContext).apply {
                mkdirs()
                listFiles()?.forEach { it.delete() }
            }
            val tmp = File(updatesDir, "opennow-update.tmp")
            val outputName = candidate.safeFileName()
            val outputFile = File(updatesDir, outputName)
            var transferred = 0L
            var lastPercent: Int? = null
            var lastProgressAt = 0L
            body.byteStream().use { input ->
                tmp.outputStream().use { output ->
                    val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
                    while (true) {
                        val read = input.read(buffer)
                        if (read == -1) break
                        output.write(buffer, 0, read)
                        transferred += read
                        val percent = totalBytes?.let { ((transferred * 100) / it).toInt().coerceIn(0, 100) }
                        val now = System.currentTimeMillis()
                        if (percent != lastPercent || now - lastProgressAt > 300L) {
                            lastPercent = percent
                            lastProgressAt = now
                            _state.value = _state.value.copy(
                                progress = AndroidUpdateProgress(percent = percent, transferredBytes = transferred, totalBytes = totalBytes),
                            )
                        }
                    }
                }
            }
            candidate.sha256?.takeIf { it.isNotBlank() }?.let { expected ->
                val actual = tmp.sha256()
                if (!actual.equals(expected.cleanHex(), ignoreCase = true)) {
                    tmp.delete()
                    error("Downloaded APK failed SHA-256 verification.")
                }
            }
            if (outputFile.exists()) outputFile.delete()
            if (!tmp.renameTo(outputFile)) {
                tmp.copyTo(outputFile, overwrite = true)
                tmp.delete()
            }
            return outputFile
        }
    }

    private fun publish(
        status: AndroidUpdateStatus,
        sourceUrl: String,
        message: String,
        availableVersionName: String? = null,
        availableVersionCode: Long? = null,
        releaseNotes: String? = null,
        downloadedFileName: String? = null,
        progress: AndroidUpdateProgress? = null,
        lastCheckedAt: Long? = _state.value.lastCheckedAt,
        clearCandidate: Boolean = false,
    ) {
        if (clearCandidate) {
            latestCandidate = null
            downloadedApk = null
        }
        _state.value = AndroidUpdateState(
            status = status,
            sourceUrl = sourceUrl,
            installSource = _state.value.installSource,
            availableVersionName = availableVersionName,
            availableVersionCode = availableVersionCode,
            releaseNotes = releaseNotes,
            downloadedFileName = downloadedFileName,
            progress = progress,
            message = message,
            lastCheckedAt = lastCheckedAt,
        )
    }

    private fun publishApkUpdatesUnavailable() {
        latestCandidate = null
        downloadedApk = null
        val current = _state.value
        _state.value = current.copy(
            status = AndroidUpdateStatus.Idle,
            availableVersionName = null,
            availableVersionCode = null,
            releaseNotes = null,
            downloadedFileName = null,
            progress = null,
            message = androidUpdateUnavailableMessage(current.installSource),
        )
    }

    private fun publishError(sourceUrl: String, message: String) {
        _state.value = _state.value.copy(
            status = AndroidUpdateStatus.Error,
            sourceUrl = sourceUrl,
            message = message,
            progress = null,
        )
    }

    private fun updateFileProviderAuthority(): String =
        "${BuildConfig.APPLICATION_ID}$UPDATE_FILE_PROVIDER_AUTHORITY_SUFFIX"

    private fun grantInstallUriPermissions(uri: Uri, installIntent: Intent) {
        val packageManager = appContext.packageManager
        val grantTargets = KNOWN_PACKAGE_INSTALLER_GRANT_TARGETS.toMutableSet()
        packageManager.resolveActivity(installIntent, 0)?.activityInfo?.packageName?.let(grantTargets::add)
        packageManager.queryIntentActivities(installIntent, 0)
            .mapNotNullTo(grantTargets) { it.activityInfo?.packageName }
        grantTargets.forEach { packageName ->
            runCatching {
                appContext.grantUriPermission(packageName, uri, Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
        }
    }
}

internal fun playStoreAvailableVersionCode(
    currentVersionCode: Long,
    updateAvailability: Int,
    availableVersionCode: Int,
): Long? {
    val updateAvailable = updateAvailability == UpdateAvailability.UPDATE_AVAILABLE ||
        updateAvailability == UpdateAvailability.DEVELOPER_TRIGGERED_UPDATE_IN_PROGRESS
    return availableVersionCode.toLong().takeIf { updateAvailable && it > currentVersionCode }
}

internal fun androidUpdateStorageDir(context: Context): File =
    File(context.applicationContext.filesDir, "updates")

@Suppress("DEPRECATION")
internal fun detectAndroidAppInstallSource(context: Context): AndroidAppInstallSource {
    val appContext = context.applicationContext
    val packageManager = appContext.packageManager
    val packageNames = linkedSetOf<String>()
    try {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val sourceInfo = packageManager.getInstallSourceInfo(appContext.packageName)
            packageNames.addIfNotBlank(sourceInfo.initiatingPackageName)
            packageNames.addIfNotBlank(sourceInfo.installingPackageName)
            packageNames.addIfNotBlank(sourceInfo.originatingPackageName)
        } else {
            packageNames.addIfNotBlank(packageManager.getInstallerPackageName(appContext.packageName))
        }
    } catch (_: PackageManager.NameNotFoundException) {
        // PackageManager should know this app, but treat lookup failure like a sideload/unknown source.
    }
    return AndroidAppInstallSource(packageNames)
}

internal fun isGooglePlayInstallerPackage(packageName: String?): Boolean =
    packageName?.trim()?.equals(GOOGLE_PLAY_STORE_PACKAGE, ignoreCase = true) == true

private fun MutableSet<String>.addIfNotBlank(value: String?) {
    value?.trim()?.takeIf { it.isNotBlank() }?.let(::add)
}

internal fun normalizeAndroidUpdateSourceUrl(raw: String): String {
    val trimmed = raw.trim()
    require(trimmed.isNotBlank()) { "Add an update source URL first." }
    val withScheme = if (Regex("^[a-z][a-z0-9+.-]*://", RegexOption.IGNORE_CASE).containsMatchIn(trimmed)) {
        trimmed
    } else {
        "https://$trimmed"
    }
    val url = withScheme.toHttpUrlOrNull() ?: error("Update source URL is invalid.")
    if (url.scheme != "https" && !url.isLoopbackHttp()) {
        error("Use HTTPS for update sources. HTTP is only allowed for localhost.")
    }
    return url.toString()
}

internal fun parseAndroidUpdateCandidate(sourceUrl: String, body: String): AndroidUpdateCandidate? {
    val root = runCatching { OpenNowJson.parseToJsonElement(body).jsonObject }.getOrNull() ?: return null
    parseGithubReleaseCandidate(sourceUrl, root)?.let { return it }

    val manifest = root.obj("android") ?: root.obj("androidUpdate") ?: root
    val rawApkUrl = manifest.string("apkUrl", "apk_url", "artifactUrl", "artifact_url", "downloadUrl", "download_url", "url") ?: return null
    val apkUrl = resolveUpdateUrl(sourceUrl, rawApkUrl) ?: return null
    return AndroidUpdateCandidate(
        sourceUrl = sourceUrl,
        apkUrl = apkUrl,
        versionName = manifest.string("versionName", "version_name", "name"),
        versionCode = manifest.long("versionCode", "version_code", "androidVersionCode"),
        sha256 = manifest.string("sha256", "sha256sum", "checksumSha256")?.cleanHex(),
        releaseNotes = normalizeReleaseNotes(manifest.string("releaseNotes", "release_notes", "notes", "body")),
        fileName = manifest.string("fileName", "file_name"),
    )
}

private fun parseGithubReleaseCandidate(sourceUrl: String, root: JsonObject): AndroidUpdateCandidate? {
    val assets = root["assets"] as? JsonArray ?: return null
    val apkAsset = assets.mapNotNull { it as? JsonObject }
        .firstOrNull { asset ->
            val name = asset.string("name").orEmpty()
            val contentType = asset.string("content_type").orEmpty()
            name.endsWith(".apk", ignoreCase = true) || contentType.equals(APK_MIME_TYPE, ignoreCase = true)
        } ?: return null
    val apkUrl = apkAsset.string("browser_download_url", "downloadUrl", "url")?.let { resolveUpdateUrl(sourceUrl, it) } ?: return null
    val versionCode = root.long("versionCode", "version_code", "androidVersionCode")
    return AndroidUpdateCandidate(
        sourceUrl = sourceUrl,
        apkUrl = apkUrl,
        versionName = root.string("tag_name", "name")?.removePrefix("v"),
        versionCode = versionCode,
        sha256 = apkAsset.string("sha256", "digest")?.removePrefix("sha256:")?.cleanHex(),
        releaseNotes = normalizeReleaseNotes(root.string("body")),
        fileName = apkAsset.string("name"),
    )
}

private fun directApkCandidate(sourceUrl: String, versionName: String?, versionCode: Long?, sha256: String?): AndroidUpdateCandidate =
    AndroidUpdateCandidate(
        sourceUrl = sourceUrl,
        apkUrl = sourceUrl,
        versionName = versionName,
        versionCode = versionCode,
        sha256 = sha256?.cleanHex(),
        releaseNotes = null,
        fileName = sourceUrl.toHttpUrlOrNull()?.pathSegments?.lastOrNull(),
    )

private fun resolveUpdateUrl(sourceUrl: String, value: String): String? {
    val trimmed = value.trim()
    val url = trimmed.toHttpUrlOrNull() ?: sourceUrl.toHttpUrlOrNull()?.resolve(trimmed)
    return url?.takeIf { it.scheme == "https" || it.isLoopbackHttp() }?.toString()
}

private fun looksLikeApk(url: String, contentType: String): Boolean =
    url.substringBefore("?").endsWith(".apk", ignoreCase = true) ||
        contentType.substringBefore(";").trim().equals(APK_MIME_TYPE, ignoreCase = true)

private fun HttpUrl.isLoopbackHttp(): Boolean =
    scheme == "http" && host.lowercase(Locale.US) in setOf("localhost", "127.0.0.1", "::1")

private fun AndroidUpdateCandidate.safeFileName(): String {
    val raw = fileName
        ?: apkUrl.toHttpUrlOrNull()?.pathSegments?.lastOrNull()
        ?: "OpenNOW-${versionName ?: versionCode ?: "update"}.apk"
    val normalized = raw.substringBefore("?")
        .replace(Regex("[^A-Za-z0-9._-]"), "_")
        .takeIf { it.endsWith(".apk", ignoreCase = true) }
        ?: "OpenNOW-${versionName ?: versionCode ?: "update"}.apk"
    return normalized
}

private fun File.sha256(): String {
    val digest = MessageDigest.getInstance("SHA-256")
    inputStream().use { input ->
        val buffer = ByteArray(DEFAULT_BUFFER_SIZE)
        while (true) {
            val read = input.read(buffer)
            if (read == -1) break
            digest.update(buffer, 0, read)
        }
    }
    return digest.digest().joinToString("") { "%02x".format(it) }
}

private fun String.cleanHex(): String =
    trim().lowercase(Locale.US).removePrefix("sha256:").filter { it in '0'..'9' || it in 'a'..'f' }

private fun normalizeReleaseNotes(value: String?): String? =
    value
        ?.replace("\\r\\n", "\n")
        ?.replace("\\n", "\n")
        ?.replace("\r\n", "\n")
        ?.replace('\r', '\n')
        ?.takeIf { it.isNotBlank() }

private fun JsonObject.string(vararg keys: String): String? =
    keys.firstNotNullOfOrNull { key ->
        this[key]?.jsonPrimitive?.contentOrNull?.takeIf { it.isNotBlank() }
    }

private fun JsonObject.long(vararg keys: String): Long? =
    keys.firstNotNullOfOrNull { key ->
        this[key]?.jsonPrimitive?.longOrNull
    }

private fun JsonObject.obj(key: String): JsonObject? = this[key] as? JsonObject
