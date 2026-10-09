package com.opencloudgaming.opennow

import android.content.Context
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

internal data class ChangelogEntry(
    val versionCode: Int,
    val version: String,
    val notes: List<String>,
)

/** Bundled release notes are available offline and never fetched during startup. */
internal fun parseChangelogs(json: String, currentVersionCode: Int): List<ChangelogEntry> {
    val root = OpenNowJson.parseToJsonElement(json) as? JsonObject ?: return emptyList()
    return root.mapNotNull { (codeText, value) ->
        val code = codeText.toIntOrNull()?.takeIf { it > 0 && it <= currentVersionCode }
            ?: return@mapNotNull null
        val entry = value as? JsonObject ?: return@mapNotNull null
        val version = (entry["version"] as? JsonPrimitive)?.contentOrNull?.trim()
            ?.takeIf(String::isNotEmpty) ?: return@mapNotNull null
        val notes = (entry["notes"] as? kotlinx.serialization.json.JsonArray)
            ?.mapNotNull { (it as? JsonPrimitive)?.contentOrNull?.trim()?.takeIf(String::isNotEmpty) }
            ?.takeIf(List<String>::isNotEmpty) ?: return@mapNotNull null
        ChangelogEntry(code, version, notes)
    }.sortedByDescending(ChangelogEntry::versionCode)
}

internal fun changelogsForPlayUpdate(
    entries: List<ChangelogEntry>,
    previousVersionCode: Int?,
    currentVersionCode: Int,
    usesGooglePlayUpdates: Boolean,
    wasUpdatedInstall: Boolean,
): List<ChangelogEntry> {
    if (!usesGooglePlayUpdates || currentVersionCode <= (previousVersionCode ?: 0)) return emptyList()
    if (previousVersionCode == null && !wasUpdatedInstall) return emptyList()
    // Older installations did not store a seen version. Show only this build on their first run.
    val lowerBound = previousVersionCode ?: currentVersionCode - 1
    return entries.filter { it.versionCode in (lowerBound + 1)..currentVersionCode }
}

internal class ChangelogRepository(context: Context) {
    private val appContext = context.applicationContext
    private val preferences = appContext.getSharedPreferences("changelogs", Context.MODE_PRIVATE)

    val entries: List<ChangelogEntry> by lazy {
        runCatching {
            appContext.assets.open("changelogs.json").bufferedReader().use { reader ->
                parseChangelogs(reader.readText(), BuildConfig.VERSION_CODE)
            }
        }.getOrDefault(emptyList())
    }

    fun pendingPlayUpdate(installSource: AndroidAppInstallSource): List<ChangelogEntry> {
        val current = BuildConfig.VERSION_CODE
        val previous = if (preferences.contains("seen_version_code")) {
            preferences.getInt("seen_version_code", 0)
        } else null
        val wasUpdatedInstall = runCatching {
            @Suppress("DEPRECATION")
            val packageInfo = appContext.packageManager.getPackageInfo(appContext.packageName, 0)
            packageInfo.lastUpdateTime > packageInfo.firstInstallTime
        }.getOrDefault(false)
        val pending = changelogsForPlayUpdate(
            entries = entries,
            previousVersionCode = previous,
            currentVersionCode = current,
            usesGooglePlayUpdates = installSource.usesGooglePlayUpdates,
            wasUpdatedInstall = wasUpdatedInstall,
        )
        if (pending.isEmpty()) markSeen()
        return pending
    }

    fun markSeen() {
        preferences.edit().putInt("seen_version_code", BuildConfig.VERSION_CODE).apply()
    }
}
