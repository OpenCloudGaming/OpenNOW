package com.opencloudgaming.opennow

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.util.Log
import java.util.concurrent.atomic.AtomicLong

internal const val QUEUE_CHANNEL_ID = "opennow_queue_status"
internal const val QUEUE_NOTIFICATION_ID = 4210
private const val QUEUE_ALERT_CHANNEL_ID = "opennow_queue_ready"
private const val QUEUE_ALERT_NOTIFICATION_ID = 4212

private const val QUEUE_SERVICE_ACTION_UPDATE = "com.opencloudgaming.opennow.queue.UPDATE"
private const val QUEUE_SERVICE_ACTION_STOP = "com.opencloudgaming.opennow.queue.STOP"
private const val QUEUE_SERVICE_EXTRA_TITLE = "title"
private const val QUEUE_SERVICE_EXTRA_TEXT = "text"
private const val QUEUE_SERVICE_TAG = "OpenNOWQueueService"
private val QUEUE_NOTIFICATION_SMALL_ICON = R.drawable.ic_tab_stream

class AndroidQueueStatusNotifier(context: Context) {
    private val appContext = context.applicationContext
    private val commandVersion = AtomicLong()
    private val queueReadyTracker = QueueReadyNotificationTracker()
    @Volatile private var serviceStartRequested = false
    private var queueReadyAlertSent = false
    private var activeTitle: String? = null
    private var activeText: String? = null
    private var cancellationApplied = true

    fun update(state: OpenNowUiState) {
        val queueFinished = queueReadyTracker.update(state)
        if (isActivelyQueued(state) && queueReadyAlertSent) {
            queueReadyAlertSent = false
            AndroidServiceCommandDispatcher.dispatch("queue-ready-alert-reset") {
                appContext.getSystemService(NotificationManager::class.java).cancel(QUEUE_ALERT_NOTIFICATION_ID)
            }
        }
        // Send a one-shot high-priority heads-up alert only after an observed queue finishes.
        if (!queueReadyAlertSent && queueFinished) {
            queueReadyAlertSent = true
            val readyTitle = state.streamGame?.title ?: "OpenNOW"
            AndroidServiceCommandDispatcher.dispatch("queue-ready-alert") {
                if (canPostNotifications()) {
                    ensureQueueAlertChannel(appContext)
                    appContext.getSystemService(NotificationManager::class.java).notify(
                        QUEUE_ALERT_NOTIFICATION_ID,
                        buildQueueReadyNotification(appContext, readyTitle),
                    )
                }
            }
        }
        if (!shouldShowQueueLaunchStatus(state)) {
            val preserveReadyAlert = queueReadyAlertSent &&
                (state.streamStatus == "connecting" || state.streamStatus == "streaming")
            cancel(preserveReadyAlert = preserveReadyAlert)
            return
        }
        cancellationApplied = false

        val title = state.streamGame?.title ?: "OpenNOW"
        val text = localizedQueueLaunchStatusText(appContext, state)
        if (serviceStartRequested && activeTitle == title && activeText == text) return
        serviceStartRequested = true
        activeTitle = title
        activeText = text
        val version = commandVersion.incrementAndGet()
        AndroidServiceCommandDispatcher.dispatch("queue-update") {
            if (commandVersion.get() != version) return@dispatch
            runCatching {
                val intent = Intent(appContext, AndroidQueueStatusService::class.java).apply {
                    action = QUEUE_SERVICE_ACTION_UPDATE
                    putExtra(QUEUE_SERVICE_EXTRA_TITLE, title)
                    putExtra(QUEUE_SERVICE_EXTRA_TEXT, text)
                }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    appContext.startForegroundService(intent)
                } else {
                    appContext.startService(intent)
                }
            }.onFailure { error ->
                Log.w(QUEUE_SERVICE_TAG, "Unable to start queue foreground service", error)
                if (commandVersion.get() != version) return@onFailure
                serviceStartRequested = false
                if (canPostNotifications()) {
                    runCatching {
                        ensureQueueNotificationChannel(appContext)
                        appContext.getSystemService(NotificationManager::class.java).notify(
                            QUEUE_NOTIFICATION_ID,
                            buildQueueNotification(appContext, title, text),
                        )
                    }.onFailure { notifyError ->
                        Log.w(QUEUE_SERVICE_TAG, "Unable to post fallback queue notification", notifyError)
                    }
                }
            }
        }
    }

    fun cancel() = cancel(preserveReadyAlert = false)

    private fun cancel(preserveReadyAlert: Boolean) {
        if (!serviceStartRequested && cancellationApplied && (preserveReadyAlert || !queueReadyAlertSent)) return
        val version = commandVersion.incrementAndGet()
        val startWasRequested = serviceStartRequested
        serviceStartRequested = false
        if (!preserveReadyAlert) queueReadyAlertSent = false
        activeTitle = null
        activeText = null
        cancellationApplied = true
        AndroidServiceCommandDispatcher.dispatch("queue-stop") {
            if (commandVersion.get() != version) return@dispatch
            val intent = Intent(appContext, AndroidQueueStatusService::class.java).apply {
                action = QUEUE_SERVICE_ACTION_STOP
            }
            if (startWasRequested) {
                appContext.startService(intent)
            } else {
                appContext.stopService(intent)
            }
            appContext.getSystemService(NotificationManager::class.java).apply {
                cancel(QUEUE_NOTIFICATION_ID)
                if (!preserveReadyAlert) cancel(QUEUE_ALERT_NOTIFICATION_ID)
            }
        }
    }

    private fun canPostNotifications(): Boolean {
        return Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            appContext.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
    }

}

class AndroidQueueStatusService : AndroidForegroundService() {
    override val notificationId: Int get() = QUEUE_NOTIFICATION_ID
    override fun prepareNotificationChannel() = ensureQueueNotificationChannel(this)

    override fun isActiveCommand(intent: Intent?): Boolean = intent?.action == QUEUE_SERVICE_ACTION_UPDATE

    override fun foregroundServiceType(intent: Intent?): Int = ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC

    override fun foregroundNotification(intent: Intent?): Notification {
        val title = intent?.getStringExtra(QUEUE_SERVICE_EXTRA_TITLE) ?: "OpenNOW"
        // Bootstrap promotion uses already-loaded resources, without reading locale preferences.
        val text = intent?.getStringExtra(QUEUE_SERVICE_EXTRA_TEXT) ?: getString(R.string.queue_status)
        return buildQueueNotification(this, title, text)
    }
}

private fun ensureQueueNotificationChannel(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val notificationManager = context.applicationContext.getSystemService(NotificationManager::class.java)
    val channel = NotificationChannel(
        QUEUE_CHANNEL_ID,
        "Queue status",
        NotificationManager.IMPORTANCE_LOW,
    ).apply {
        description = "Shows OpenNOW queue and session startup progress."
        lockscreenVisibility = Notification.VISIBILITY_PUBLIC
        setShowBadge(false)
    }
    notificationManager.createNotificationChannel(channel)
}

private fun buildQueueNotification(context: Context, title: String, text: String): Notification {
    val appContext = context.applicationContext
    val openIntent = Intent(appContext, MainActivity::class.java).apply {
        action = Intent.ACTION_MAIN
        addCategory(Intent.CATEGORY_LAUNCHER)
        flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
    }
    val pendingIntent = PendingIntent.getActivity(
        appContext,
        0,
        openIntent,
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        Notification.Builder(appContext, QUEUE_CHANNEL_ID)
    } else {
        @Suppress("DEPRECATION")
        Notification.Builder(appContext)
    }
    return builder
        .setSmallIcon(QUEUE_NOTIFICATION_SMALL_ICON)
        .setContentTitle(title)
        .setContentText(text)
        .setSubText("OpenNOW")
        .setCategory(Notification.CATEGORY_PROGRESS)
        .setProgress(0, 0, true)
        .setVisibility(Notification.VISIBILITY_PUBLIC)
        .setOngoing(true)
        .setOnlyAlertOnce(true)
        .setShowWhen(false)
        .setContentIntent(pendingIntent)
        .build()
}

private fun ensureQueueAlertChannel(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val notificationManager = context.applicationContext.getSystemService(NotificationManager::class.java)
    val channel = NotificationChannel(
        QUEUE_ALERT_CHANNEL_ID,
        "Queue ready alert",
        NotificationManager.IMPORTANCE_HIGH,
    ).apply {
        description = "Alerts when a GFN queue finishes and the game is about to launch."
        lockscreenVisibility = Notification.VISIBILITY_PUBLIC
        setShowBadge(true)
        enableVibration(true)
    }
    notificationManager.createNotificationChannel(channel)
}

private fun buildQueueReadyNotification(context: Context, gameTitle: String): Notification {
    val appContext = context.applicationContext
    val openIntent = Intent(appContext, MainActivity::class.java).apply {
        action = Intent.ACTION_MAIN
        addCategory(Intent.CATEGORY_LAUNCHER)
        flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
    }
    val pendingIntent = PendingIntent.getActivity(
        appContext,
        2,
        openIntent,
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        Notification.Builder(appContext, QUEUE_ALERT_CHANNEL_ID)
    } else {
        @Suppress("DEPRECATION")
        Notification.Builder(appContext)
    }
    return builder
        .setSmallIcon(QUEUE_NOTIFICATION_SMALL_ICON)
        .setContentTitle(localizedAndroidContext(context).getString(R.string.queue_ready_title, gameTitle))
        .setContentText(localizedAndroidContext(context).getString(R.string.queue_ready_body))
        .setSubText("OpenNOW")
        .setCategory(Notification.CATEGORY_ALARM)
        .setVisibility(Notification.VISIBILITY_PUBLIC)
        .setOngoing(false)
        .setAutoCancel(true)
        .setShowWhen(true)
        .setContentIntent(pendingIntent)
        .build()
}
