package com.opencloudgaming.opennow

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioManager
import android.media.MediaPlayer
import android.media.ToneGenerator
import android.os.SystemClock

internal class AndroidNerdAudioController(context: Context) {
    private val appContext = context.applicationContext
    private var introPlayer: MediaPlayer? = null
    private var toneGenerator: ToneGenerator? = null
    private var lastToneAtMs = 0L

    fun startIntro(enabled: Boolean, onPlayingChanged: (Boolean) -> Unit) {
        stopIntro(onPlayingChanged)
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
            player.setVolume(0.72f, 0.72f)
            player.isLooping = false
            player.setOnCompletionListener { completed ->
                if (introPlayer === completed) {
                    introPlayer = null
                }
                completed.release()
                onPlayingChanged(false)
            }
            player.setOnErrorListener { failed, _, _ ->
                if (introPlayer === failed) {
                    introPlayer = null
                }
                failed.release()
                onPlayingChanged(false)
                true
            }
            player.prepare()
            introPlayer = player
            player.start()
            onPlayingChanged(true)
        }.onFailure {
            player.release()
            if (introPlayer === player) {
                introPlayer = null
            }
            onPlayingChanged(false)
        }
    }

    fun toggleIntro(onPlayingChanged: (Boolean) -> Unit) {
        val player = introPlayer
        if (player == null) {
            startIntro(enabled = true, onPlayingChanged = onPlayingChanged)
            return
        }
        runCatching {
            if (player.isPlaying) {
                player.pause()
                onPlayingChanged(false)
            } else {
                player.start()
                onPlayingChanged(true)
            }
        }.onFailure {
            stopIntro(onPlayingChanged)
        }
    }

    fun stopIntro(onPlayingChanged: (Boolean) -> Unit = {}) {
        val player = introPlayer ?: return
        introPlayer = null
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
        stopIntro()
        toneGenerator?.release()
        toneGenerator = null
    }

    private companion object {
        private const val BUTTON_TONE_VOLUME = 34
        private const val BUTTON_TONE_DURATION_MS = 48
        private const val MIN_BUTTON_TONE_INTERVAL_MS = 55L
    }
}
