package com.opencloudgaming.opennow

import android.app.Notification
import android.app.Service
import android.content.Intent
import android.os.Build
import android.os.IBinder
import android.util.Log
import androidx.core.app.ServiceCompat

/** Owns the foreground-start handshake, including stop commands delivered during a restart. */
abstract class AndroidForegroundService : Service() {
    protected abstract val notificationId: Int
    protected abstract fun prepareNotificationChannel()
    protected abstract fun foregroundNotification(intent: Intent?): Notification
    protected abstract fun foregroundServiceType(intent: Intent?): Int
    protected abstract fun isActiveCommand(intent: Intent?): Boolean
    protected open fun onForegroundStarted() = Unit
    protected open fun onForegroundStopped() = Unit

    final override fun onCreate() {
        super.onCreate()
        // A cold service must acknowledge startForegroundService before processing commands,
        // reading locale preferences, acquiring wake locks, or doing other optional work.
        runCatching {
            prepareNotificationChannel()
            promoteToForeground(null)
        }.onFailure { foregroundFailed(it) }
    }

    final override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        runCatching {
            val active = isActiveCommand(intent)
            // A preceding STOP may have demoted this same instance before Android delivers a
            // newer foreground start. Promote for every command, even STOP/unknown/null intents.
            promoteToForeground(if (active) intent else null)
            if (active) onForegroundStarted() else {
                stopForegroundService()
                stopSelfResult(startId)
            }
        }.onFailure { error ->
            foregroundFailed(error, startId)
        }
        return START_NOT_STICKY
    }

    private fun promoteToForeground(intent: Intent?) {
        val notification = foregroundNotification(intent)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            startForeground(notificationId, notification, foregroundServiceType(intent))
        } else {
            startForeground(notificationId, notification)
        }
    }

    private fun foregroundFailed(error: Throwable, startId: Int? = null) {
        Log.e("OpenNOWForeground", "${javaClass.simpleName} foreground command failed", error)
        runCatching { stopForegroundService() }
        if (startId == null) stopSelf() else stopSelfResult(startId)
    }

    protected fun stopForegroundService() {
        try {
            onForegroundStopped()
        } finally {
            ServiceCompat.stopForeground(this, ServiceCompat.STOP_FOREGROUND_REMOVE)
        }
    }

    override fun onTimeout(startId: Int, fgsType: Int) {
        stopForegroundService()
        stopSelf()
    }

    final override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        onForegroundStopped()
        super.onDestroy()
    }
}
