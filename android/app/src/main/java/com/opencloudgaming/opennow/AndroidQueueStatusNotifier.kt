package com.opencloudgaming.opennow

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.util.Log

internal const val QUEUE_CHANNEL_ID = "opennow_queue_status"
internal const val QUEUE_NOTIFICATION_ID = 4210

private const val QUEUE_SERVICE_ACTION_UPDATE = "com.opencloudgaming.opennow.queue.UPDATE"
private const val QUEUE_SERVICE_ACTION_STOP = "com.opencloudgaming.opennow.queue.STOP"
private const val QUEUE_SERVICE_EXTRA_TITLE = "title"
private const val QUEUE_SERVICE_EXTRA_TEXT = "text"
private const val QUEUE_SERVICE_TAG = "OpenNOWQueueService"

class AndroidQueueStatusNotifier(private val context: Context) {
    private val appContext = context.applicationContext
    private val notificationManager = appContext.getSystemService(NotificationManager::class.java)
    private var serviceStartRequested = false

    fun update(state: OpenNowUiState) {
        if (!shouldShowQueueLaunchStatus(state)) {
            cancel()
            return
        }

        ensureChannel()
        val intent = Intent(appContext, AndroidQueueStatusService::class.java).apply {
            action = QUEUE_SERVICE_ACTION_UPDATE
            putExtra(QUEUE_SERVICE_EXTRA_TITLE, state.streamGame?.title ?: "OpenNOW")
            putExtra(QUEUE_SERVICE_EXTRA_TEXT, queueLaunchStatusText(state))
        }
        runCatching {
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                appContext.startForegroundService(intent)
            } else {
                appContext.startService(intent)
            }
            serviceStartRequested = true
        }.onFailure { error ->
            Log.w(QUEUE_SERVICE_TAG, "Unable to start queue foreground service", error)
            if (canPostNotifications()) {
                notificationManager.notify(QUEUE_NOTIFICATION_ID, buildQueueNotification(appContext, state.streamGame?.title ?: "OpenNOW", queueLaunchStatusText(state)))
            }
        }
    }

    fun cancel() {
        val intent = Intent(appContext, AndroidQueueStatusService::class.java).apply {
            action = QUEUE_SERVICE_ACTION_STOP
        }
        runCatching {
            if (serviceStartRequested) {
                appContext.startService(intent)
            } else {
                appContext.stopService(intent)
            }
        }.onFailure { error ->
            Log.w(QUEUE_SERVICE_TAG, "Unable to stop queue foreground service", error)
        }
        serviceStartRequested = false
        notificationManager.cancel(QUEUE_NOTIFICATION_ID)
    }

    private fun canPostNotifications(): Boolean {
        return Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            appContext.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
    }

    private fun ensureChannel() {
        ensureQueueNotificationChannel(appContext)
    }
}

class AndroidQueueStatusService : Service() {
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            QUEUE_SERVICE_ACTION_STOP -> {
                startQueueForeground("OpenNOW", "Queue status")
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf(startId)
                return START_NOT_STICKY
            }
            QUEUE_SERVICE_ACTION_UPDATE, null -> {
                val title = intent?.getStringExtra(QUEUE_SERVICE_EXTRA_TITLE) ?: "OpenNOW"
                val text = intent?.getStringExtra(QUEUE_SERVICE_EXTRA_TEXT) ?: "Queue status"
                startQueueForeground(title, text)
                return START_NOT_STICKY
            }
            else -> {
                startQueueForeground("OpenNOW", "Queue status")
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf(startId)
                return START_NOT_STICKY
            }
        }
    }

    override fun onBind(intent: Intent?): IBinder? = null

    private fun startQueueForeground(title: String, text: String) {
        ensureQueueNotificationChannel(this)
        val notification = buildQueueNotification(this, title, text)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(QUEUE_NOTIFICATION_ID, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(QUEUE_NOTIFICATION_ID, notification)
        }
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
        .setSmallIcon(R.drawable.opennow_icon)
        .setContentTitle(title)
        .setContentText(text)
        .setSubText("OpenNOW")
        .setOngoing(true)
        .setOnlyAlertOnce(true)
        .setShowWhen(false)
        .setContentIntent(pendingIntent)
        .build()
}
