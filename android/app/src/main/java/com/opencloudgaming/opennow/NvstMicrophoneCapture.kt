package com.opencloudgaming.opennow

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioRecord
import android.media.MediaRecorder
import android.media.audiofx.AcousticEchoCanceler
import android.media.audiofx.NoiseSuppressor
import androidx.core.content.ContextCompat

/** Capture owns AudioRecord and effects on one worker. Never opens a recorder while muted. */
internal class NvstMicrophoneCapture(
    private val context: Context,
    private val deviceId: String,
    private val ready: () -> Boolean,
    private val send: (ShortArray, Long, Boolean) -> Boolean,
    private val error: (String) -> Unit,
) {
    private val generation = java.util.concurrent.atomic.AtomicInteger()
    @Volatile var enabled = false
        set(value) {
            if (field != value) { generation.incrementAndGet(); field = value }
        }
    @Volatile private var closed = false
    private val worker = Thread(::capture, "OpenNOW-NVST-microphone")

    fun start() = worker.start()
    fun stop() { closed = true; enabled = false }
    fun awaitStopped() = worker.join()

    private fun capture() {
        var timestamp = 0L
        while (!closed) {
            if (!enabled || !ready()) { Thread.sleep(10); continue }
            val captureGeneration = generation.get()
            fun active() = !closed && enabled && ready() && generation.get() == captureGeneration
            var recorder: AudioRecord? = null
            var echo: AcousticEchoCanceler? = null
            var noise: NoiseSuppressor? = null
            try {
                check(ContextCompat.checkSelfPermission(context, Manifest.permission.RECORD_AUDIO) == PackageManager.PERMISSION_GRANTED) {
                    "Microphone permission is unavailable"
                }
                val minimum = AudioRecord.getMinBufferSize(48_000, AudioFormat.CHANNEL_IN_MONO, AudioFormat.ENCODING_PCM_16BIT)
                check(minimum > 0) { "48 kHz microphone capture is unavailable" }
                recorder = AudioRecord.Builder()
                    .setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION)
                    .setAudioFormat(AudioFormat.Builder().setSampleRate(48_000)
                        .setChannelMask(AudioFormat.CHANNEL_IN_MONO).setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
                    .setBufferSizeInBytes(maxOf(minimum, 960 * 2 * 4)).build()
                check(recorder.state == AudioRecord.STATE_INITIALIZED) { "Microphone initialization failed" }
                if (deviceId.isNotBlank()) {
                    val selected = context.getSystemService(AudioManager::class.java)
                        .getDevices(AudioManager.GET_DEVICES_INPUTS).firstOrNull { it.id.toString() == deviceId }
                    check(selected != null && recorder.setPreferredDevice(selected)) { "Selected microphone is unavailable" }
                }
                if (AcousticEchoCanceler.isAvailable()) echo = AcousticEchoCanceler.create(recorder.audioSessionId)?.also { it.enabled = true }
                if (NoiseSuppressor.isAvailable()) noise = NoiseSuppressor.create(recorder.audioSessionId)?.also { it.enabled = true }
                if (!active()) continue
                recorder.startRecording()
                check(recorder.recordingState == AudioRecord.RECORDSTATE_RECORDING) { "Microphone did not start" }
                val samples = ShortArray(960)
                var filled = 0
                var restart = true
                while (active()) {
                    val count = recorder.read(samples, filled, samples.size - filled, AudioRecord.READ_NON_BLOCKING)
                    check(count >= 0) { "Microphone read failed ($count)" }
                    if (count == 0) { Thread.sleep(2); continue }
                    filled += count
                    if (filled == samples.size) {
                        // Recheck after read so muting cannot enqueue newly captured speech.
                        if (!active()) break
                        if (restart) timestamp = android.os.SystemClock.elapsedRealtime() * 48
                        check(send(samples, timestamp, restart)) { "Microphone transport is unavailable" }
                        timestamp += samples.size
                        restart = false
                        filled = 0
                    }
                }
            } catch (failure: Exception) {
                if (active()) {
                    enabled = false
                    error(failure.message ?: "Microphone capture failed")
                }
            } finally {
                runCatching { recorder?.stop() }
                runCatching { echo?.release() }
                runCatching { noise?.release() }
                recorder?.release()
            }
        }
    }
}
