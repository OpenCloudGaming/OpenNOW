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
import android.os.PowerManager
import android.util.Log
import java.util.concurrent.atomic.AtomicLong

private const val STREAM_CHANNEL_ID = "opennow_active_stream"
private const val STREAM_NOTIFICATION_ID = 4211
private const val STREAM_SERVICE_ACTION_START = "com.opencloudgaming.opennow.stream.START"
private const val STREAM_SERVICE_ACTION_STOP = "com.opencloudgaming.opennow.stream.STOP"
private const val STREAM_SERVICE_EXTRA_TITLE = "title"
private const val STREAM_SERVICE_EXTRA_TEXT = "text"
private const val STREAM_SERVICE_EXTRA_MICROPHONE_CAPTURE = "microphone_capture"
private const val STREAM_SERVICE_TAG = "OpenNOWStreamService"

internal fun shouldKeepAndroidStreamAlive(state: OpenNowUiState): Boolean =
    state.page == AppPage.Stream &&
        state.streamStatus != "idle" &&
        state.streamSession?.isReadyForStream() == true

internal fun androidStreamForegroundServiceType(
    microphoneCaptureActive: Boolean,
    sdkInt: Int,
): Int =
    ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK or
        if (microphoneCaptureActive && sdkInt >= Build.VERSION_CODES.R) {
            ServiceInfo.FOREGROUND_SERVICE_TYPE_MICROPHONE
        } else {
            0
        }

internal fun shouldPrepareAndroidStreamMicrophone(
    state: OpenNowUiState,
    permissionGranted: Boolean,
): Boolean =
    shouldKeepAndroidStreamAlive(state) &&
        (state.activeStreamSettings ?: state.settings.stream).microphoneMode != MicrophoneMode.Disabled &&
        permissionGranted

class AndroidStreamKeepAliveNotifier(context: Context) {
    private val appContext = context.applicationContext
    private val commandVersion = AtomicLong()
    @Volatile private var serviceStartRequested = false
    private var activeTitle: String? = null
    private var activeMicrophoneCapture = false
    private var cancellationApplied = false

    fun update(state: OpenNowUiState) {
        if (!shouldKeepAndroidStreamAlive(state)) {
            cancel()
            return
        }
        cancellationApplied = false

        val title = state.streamGame?.title ?: "OpenNOW"
        val microphoneCaptureActive = shouldPrepareAndroidStreamMicrophone(
            state = state,
            permissionGranted = appContext.checkSelfPermission(Manifest.permission.RECORD_AUDIO) ==
                PackageManager.PERMISSION_GRANTED,
        )
        requestStart(title, microphoneCaptureActive)
    }

    /**
     * Marks the stream as microphone-capable before WebRTC opens AudioRecord. The flag remains set
     * while the user mutes the track because WebRTC can keep the capture device open across mute
     * and transport reconnects.
     */
    fun setMicrophoneCaptureActive(active: Boolean) {
        if (activeMicrophoneCapture == active) return
        val title = activeTitle
        if (!serviceStartRequested || title == null) {
            activeMicrophoneCapture = active
            return
        }
        requestStart(title, active)
    }

    private fun requestStart(
        title: String,
        microphoneCaptureActive: Boolean,
    ) {
        if (
            serviceStartRequested &&
            activeTitle == title &&
            activeMicrophoneCapture == microphoneCaptureActive
        ) return
        serviceStartRequested = true
        activeTitle = title
        activeMicrophoneCapture = microphoneCaptureActive
        val version = commandVersion.incrementAndGet()
        AndroidServiceCommandDispatcher.dispatch("stream-start") {
            if (commandVersion.get() != version) return@dispatch
            runCatching {
                val intent = Intent(appContext, AndroidStreamKeepAliveService::class.java).apply {
                    action = STREAM_SERVICE_ACTION_START
                    putExtra(STREAM_SERVICE_EXTRA_TITLE, title)
                    // Resolve app locale before starting Android's foreground-promotion timer.
                    putExtra(STREAM_SERVICE_EXTRA_TEXT, localizedAndroidContext(appContext)
                        .getString(R.string.stream_background_notification))
                    putExtra(STREAM_SERVICE_EXTRA_MICROPHONE_CAPTURE, microphoneCaptureActive)
                }
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                    appContext.startForegroundService(intent)
                } else {
                    appContext.startService(intent)
                }
            }.onFailure { error ->
                if (commandVersion.get() == version) serviceStartRequested = false
                Log.w(STREAM_SERVICE_TAG, "Unable to start stream foreground service", error)
            }
        }
    }

    fun cancel() {
        if (!serviceStartRequested && cancellationApplied) return
        val version = commandVersion.incrementAndGet()
        val startWasRequested = serviceStartRequested
        serviceStartRequested = false
        activeTitle = null
        activeMicrophoneCapture = false
        cancellationApplied = true
        AndroidServiceCommandDispatcher.dispatch("stream-stop") {
            if (commandVersion.get() != version) return@dispatch
            val intent = Intent(appContext, AndroidStreamKeepAliveService::class.java).apply {
                action = STREAM_SERVICE_ACTION_STOP
            }
            if (startWasRequested) {
                appContext.startService(intent)
            } else {
                appContext.stopService(intent)
            }
            appContext.getSystemService(NotificationManager::class.java).cancel(STREAM_NOTIFICATION_ID)
        }
    }
}

class AndroidStreamKeepAliveService : AndroidForegroundService() {
    private var streamWakeLock: PowerManager.WakeLock? = null
    override val notificationId: Int get() = STREAM_NOTIFICATION_ID
    override fun prepareNotificationChannel() = ensureStreamNotificationChannel(this)

    override fun isActiveCommand(intent: Intent?): Boolean = intent?.action == STREAM_SERVICE_ACTION_START

    override fun foregroundServiceType(intent: Intent?): Int = androidStreamForegroundServiceType(
        microphoneCaptureActive = intent?.getBooleanExtra(STREAM_SERVICE_EXTRA_MICROPHONE_CAPTURE, false) == true,
        sdkInt = Build.VERSION.SDK_INT,
    )

    override fun foregroundNotification(intent: Intent?): Notification {
        return buildStreamNotification(
            this,
            intent?.getStringExtra(STREAM_SERVICE_EXTRA_TITLE) ?: "OpenNOW",
            intent?.getStringExtra(STREAM_SERVICE_EXTRA_TEXT) ?: getString(R.string.stream_background_notification),
        )
    }

    override fun onForegroundStarted() = acquireStreamWakeLock()
    override fun onForegroundStopped() = releaseStreamWakeLock()

    override fun onTaskRemoved(rootIntent: Intent?) {
        // Removing the task releases local resources; the cloud session remains resumable.
        Log.i(STREAM_SERVICE_TAG, "App task removed; preserving cloud session until explicit End")
        stopForegroundService()
        stopSelf()
        super.onTaskRemoved(rootIntent)
    }

    private fun acquireStreamWakeLock() {
        if (streamWakeLock?.isHeld == true) return
        streamWakeLock = getSystemService(PowerManager::class.java)
            .newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "OpenNOW:ActiveStream")
            .apply {
                setReferenceCounted(false)
                acquire()
            }
    }

    private fun releaseStreamWakeLock() {
        streamWakeLock?.takeIf { it.isHeld }?.release()
        streamWakeLock = null
    }
}

private fun ensureStreamNotificationChannel(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val notificationManager = context.applicationContext.getSystemService(NotificationManager::class.java)
    val channel = NotificationChannel(
        STREAM_CHANNEL_ID,
        "Active stream",
        NotificationManager.IMPORTANCE_LOW,
    ).apply {
        description = "Keeps an active OpenNOW stream connected while the screen is off."
        lockscreenVisibility = Notification.VISIBILITY_PUBLIC
        setShowBadge(false)
    }
    notificationManager.createNotificationChannel(channel)
}

private fun buildStreamNotification(context: Context, title: String, text: String): Notification {
    val appContext = context.applicationContext
    val openIntent = Intent(appContext, MainActivity::class.java).apply {
        action = Intent.ACTION_MAIN
        addCategory(Intent.CATEGORY_LAUNCHER)
        flags = Intent.FLAG_ACTIVITY_SINGLE_TOP or Intent.FLAG_ACTIVITY_CLEAR_TOP
    }
    val pendingIntent = PendingIntent.getActivity(
        appContext,
        1,
        openIntent,
        PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
        Notification.Builder(appContext, STREAM_CHANNEL_ID)
    } else {
        @Suppress("DEPRECATION")
        Notification.Builder(appContext)
    }
    return builder
        .setSmallIcon(R.drawable.ic_tab_stream)
        .setContentTitle(title)
        .setContentText(text)
        .setSubText("OpenNOW")
        .setCategory(Notification.CATEGORY_TRANSPORT)
        .setVisibility(Notification.VISIBILITY_PUBLIC)
        .setOngoing(true)
        .setOnlyAlertOnce(true)
        .setShowWhen(false)
        .setContentIntent(pendingIntent)
        .build()
}
