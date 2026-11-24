package com.opencloudgaming.opennow

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Process
import android.os.SystemClock
import android.util.Log
import java.io.File
import kotlin.system.exitProcess

private const val APP_DATA_RESET_LOG_TAG = "OpenNOW.AppDataReset"
private const val APP_DATA_RESET_RELAUNCH_DELAY_MS = 500L
private const val APP_DATA_RESET_RELAUNCH_REQUEST_CODE = 1007

internal fun wipeAppDataAndRelaunch(context: Context): Nothing {
    val appContext = context.applicationContext
    val relaunchIntent = buildAppRelaunchIntent(appContext)
    val relaunchPendingIntent = PendingIntent.getActivity(
        appContext,
        APP_DATA_RESET_RELAUNCH_REQUEST_CODE,
        relaunchIntent,
        PendingIntent.FLAG_CANCEL_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    val alarmManager = appContext.getSystemService(AlarmManager::class.java)
    alarmManager?.set(
        AlarmManager.ELAPSED_REALTIME,
        SystemClock.elapsedRealtime() + APP_DATA_RESET_RELAUNCH_DELAY_MS,
        relaunchPendingIntent,
    )
    AppMonetizationPreferences(appContext).flushBeforeReset()
    clearAppDataDirectories(appContext)
    Process.killProcess(Process.myPid())
    exitProcess(0)
}

private fun buildAppRelaunchIntent(context: Context): Intent =
    (context.packageManager.getLaunchIntentForPackage(context.packageName) ?: Intent(context, MainActivity::class.java)).apply {
        action = Intent.ACTION_MAIN
        addCategory(Intent.CATEGORY_LAUNCHER)
        flags = Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_CLEAR_TASK
    }

/**
 * [Context.getDataDir] is API 24. On API 23 the same directory is the parent of `filesDir`, which
 * keeps the destructive reset path from throwing NoSuchMethodError instead of clearing data.
 */
private val Context.appDataDirCompat: File
    get() = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
        dataDir
    } else {
        filesDir.parentFile ?: filesDir
    }

private fun clearAppDataDirectories(context: Context) {
    val dataDir = context.appDataDirCompat
    val children = dataDir.listFiles().orEmpty()
    if (children.isEmpty()) {
        deleteKnownAppDataDirectories(context)
        return
    }
    children.forEach { child ->
        when (child.name) {
            "lib" -> Unit
            "shared_prefs" -> deleteResettablePreferences(child)
            else -> deleteAppDataPath(child)
        }
    }
}

private fun deleteKnownAppDataDirectories(context: Context) {
    listOf(
        context.filesDir,
        context.cacheDir,
        context.codeCacheDir,
        context.noBackupFilesDir,
        File(context.appDataDirCompat, "databases"),
    ).forEach(::deleteAppDataPath)
    deleteResettablePreferences(File(context.appDataDirCompat, "shared_prefs"))
}

private fun deleteResettablePreferences(directory: File) {
    directory.listFiles().orEmpty().forEach { file ->
        if (file.name != "$APP_MONETIZATION_PREFS_NAME.xml" && file.name != "$APP_MONETIZATION_PREFS_NAME.xml.bak") {
            deleteAppDataPath(file)
        }
    }
}

private fun deleteAppDataPath(path: File?) {
    if (path == null || !path.exists()) return
    val deleted = runCatching { path.deleteRecursively() }
        .onFailure { error -> Log.w(APP_DATA_RESET_LOG_TAG, "Failed to delete ${path.name}", error) }
        .getOrDefault(false)
    if (!deleted && path.exists()) {
        Log.w(APP_DATA_RESET_LOG_TAG, "Failed to delete ${path.name}")
    }
}
