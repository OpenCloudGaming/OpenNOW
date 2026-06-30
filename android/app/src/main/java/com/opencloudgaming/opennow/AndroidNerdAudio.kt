package com.opencloudgaming.opennow

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.ToneGenerator
import android.os.Handler
import android.os.Looper
import android.os.SystemClock

internal class AndroidNerdAudioController(context: Context) {
    private val appContext = context.applicationContext
    private val mainHandler = Handler(Looper.getMainLooper())
    private var cuePlayer: MediaPlayer? = null
    private var cuePurpose: MusicCuePurpose? = null
    private var cuePlayingChanged: ((Boolean) -> Unit)? = null
    private var toneGenerator: ToneGenerator? = null
    private var lastToneAtMs = 0L
    private val stopCueRunnable = Runnable {
        stopMusicCue(cuePlayingChanged ?: {})
    }

    fun startIntro(enabled: Boolean, onPlayingChanged: (Boolean) -> Unit) {
        startMusicCue(MusicCuePurpose.Intro, enabled, INTRO_MUSIC_MAX_DURATION_MS, onPlayingChanged)
    }

    fun startQueueReadyReminder(enabled: Boolean, onPlayingChanged: (Boolean) -> Unit = {}) {
        startMusicCue(MusicCuePurpose.QueueReady, enabled, QUEUE_READY_CUE_DURATION_MS, onPlayingChanged)
    }

    private fun startMusicCue(
        purpose: MusicCuePurpose,
        enabled: Boolean,
        maxDurationMs: Long,
        onPlayingChanged: (Boolean) -> Unit,
    ) {
        stopMusicCue(onPlayingChanged)
        if (!enabled) return

        val descriptor = runCatching {
            appContext.resources.openRawResourceFd(R.raw.nerd_stream_intro)
        }.getOrNull() ?: return

        val player = MediaPlayer()
        runCatching {
            descriptor.use {
                player.setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_GAME)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                        .build(),
                )
                player.setDataSource(it.fileDescriptor, it.startOffset, it.length)
            }
            player.setVolume(MUSIC_CUE_VOLUME, MUSIC_CUE_VOLUME)
            player.isLooping = false
            player.setOnCompletionListener { completed ->
                if (cuePlayer === completed) {
                    cuePlayer = null
                    cuePurpose = null
                    cuePlayingChanged = null
                    mainHandler.removeCallbacks(stopCueRunnable)
                }
                completed.release()
                onPlayingChanged(false)
            }
            player.setOnErrorListener { failed, _, _ ->
                if (cuePlayer === failed) {
                    cuePlayer = null
                    cuePurpose = null
                    cuePlayingChanged = null
                    mainHandler.removeCallbacks(stopCueRunnable)
                }
                failed.release()
                onPlayingChanged(false)
                true
            }
            player.prepare()
            cuePlayer = player
            cuePurpose = purpose
            cuePlayingChanged = onPlayingChanged
            player.start()
            mainHandler.postDelayed(stopCueRunnable, maxDurationMs)
            onPlayingChanged(true)
        }.onFailure {
            player.release()
            if (cuePlayer === player) {
                cuePlayer = null
                cuePurpose = null
                cuePlayingChanged = null
                mainHandler.removeCallbacks(stopCueRunnable)
            }
            onPlayingChanged(false)
        }
    }

    fun stopIntro(onPlayingChanged: (Boolean) -> Unit = {}) {
        if (cuePurpose == MusicCuePurpose.Intro) {
            stopMusicCue(onPlayingChanged)
        }
    }

    fun stopQueueReadyReminder(onPlayingChanged: (Boolean) -> Unit = {}) {
        if (cuePurpose == MusicCuePurpose.QueueReady) {
            stopMusicCue(onPlayingChanged)
        }
    }

    fun stopAll(onPlayingChanged: (Boolean) -> Unit = {}) {
        stopMusicCue(onPlayingChanged)
    }

    private fun stopMusicCue(onPlayingChanged: (Boolean) -> Unit = {}) {
        val player = cuePlayer ?: return
        cuePlayer = null
        cuePurpose = null
        cuePlayingChanged = null
        mainHandler.removeCallbacks(stopCueRunnable)
        runCatching { player.stop() }
        player.release()
        onPlayingChanged(false)
    }

    fun playButtonTone(enabled: Boolean) {
        if (!enabled) return
        val now = SystemClock.uptimeMillis()
        if (now - lastToneAtMs < MIN_BUTTON_TONE_INTERVAL_MS) return
        lastToneAtMs = now
        val generator = toneGenerator ?: runCatching {
            ToneGenerator(AudioManager.STREAM_MUSIC, BUTTON_TONE_VOLUME)
        }.getOrNull()?.also {
            toneGenerator = it
        } ?: return
        runCatching {
            generator.startTone(ToneGenerator.TONE_PROP_ACK, BUTTON_TONE_DURATION_MS)
        }
    }

    fun release() {
        stopAll()
        toneGenerator?.release()
        toneGenerator = null
    }

    private enum class MusicCuePurpose {
        Intro,
        QueueReady,
    }

    private companion object {
        private const val MUSIC_CUE_VOLUME = 0.20f
        private const val INTRO_MUSIC_MAX_DURATION_MS = 150_000L
        private const val QUEUE_READY_CUE_DURATION_MS = 7_000L
        private const val BUTTON_TONE_VOLUME = 34
        private const val BUTTON_TONE_DURATION_MS = 48
        private const val MIN_BUTTON_TONE_INTERVAL_MS = 55L
    }
}
