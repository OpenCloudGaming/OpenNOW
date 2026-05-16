package com.opencloudgaming.opennow

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build

private const val QUEUE_CHANNEL_ID = "opennow_queue_status"
private const val QUEUE_NOTIFICATION_ID = 4210

class AndroidQueueStatusNotifier(private val context: Context) {
    private val appContext = context.applicationContext
    private val notificationManager = appContext.getSystemService(NotificationManager::class.java)

    fun update(state: OpenNowUiState) {
        if (!shouldShowQueueLaunchStatus(state)) {
            cancel()
            return
        }
        if (!canPostNotifications()) return

        ensureChannel()
        notificationManager.notify(QUEUE_NOTIFICATION_ID, buildNotification(state))
    }

    fun cancel() {
        notificationManager.cancel(QUEUE_NOTIFICATION_ID)
    }

    private fun canPostNotifications(): Boolean {
        return Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            appContext.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
    }

    private fun ensureChannel() {
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

    private fun buildNotification(state: OpenNowUiState): Notification {
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
        val gameTitle = state.streamGame?.title ?: "OpenNOW"
        return Notification.Builder(appContext, QUEUE_CHANNEL_ID)
            .setSmallIcon(R.drawable.opennow_icon)
            .setContentTitle(gameTitle)
            .setContentText(queueLaunchStatusText(state))
            .setSubText("OpenNOW")
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setShowWhen(false)
            .setContentIntent(pendingIntent)
            .build()
    }
}
