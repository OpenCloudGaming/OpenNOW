package com.opencloudgaming.opennow

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioTrack
import android.media.MediaCodec
import android.media.MediaFormat
import java.nio.ByteBuffer
import java.nio.ByteOrder

/** Android's Opus decoder; partial writes retain PCM within a bounded 100 ms queue. */
internal class NvstAudioOutput(
    private val lowLatencyGameAudio: Boolean,
    private val recordingAudio: (ByteBuffer) -> Unit,
) {
    private val codec = MediaCodec.createDecoderByType(MediaFormat.MIMETYPE_AUDIO_OPUS)
    private val track: AudioTrack
    private val info = MediaCodec.BufferInfo()
    private val pending = java.util.ArrayDeque<ByteBuffer>()
    private var pendingBytes = 0
    private var appliedMute: Boolean? = null

    init {
        var openedTrack: AudioTrack? = null
        try {
            val format = MediaFormat.createAudioFormat(MediaFormat.MIMETYPE_AUDIO_OPUS, 48000, 2)
            val head = ByteBuffer.allocate(19).order(ByteOrder.LITTLE_ENDIAN)
                .put("OpusHead".toByteArray(Charsets.US_ASCII)).put(1).put(2).putShort(0).putInt(48000).putShort(0).put(0)
            format.setByteBuffer("csd-0", ByteBuffer.wrap(head.array()))
            format.setByteBuffer("csd-1", ByteBuffer.allocate(8).order(ByteOrder.nativeOrder()).putLong(0).apply { flip() })
            format.setByteBuffer("csd-2", ByteBuffer.allocate(8).order(ByteOrder.nativeOrder()).putLong(80_000_000).apply { flip() })
            codec.configure(format, null, null, 0)
            codec.start()
            openedTrack = AudioTrack.Builder()
                .setAudioAttributes(AudioAttributes.Builder().setUsage(streamAudioUsage(lowLatencyGameAudio)).setContentType(AudioAttributes.CONTENT_TYPE_MOVIE).build())
                .setAudioFormat(AudioFormat.Builder().setSampleRate(48000).setChannelMask(AudioFormat.CHANNEL_OUT_STEREO).setEncoding(AudioFormat.ENCODING_PCM_16BIT).build())
                .setBufferSizeInBytes(maxOf(3840, AudioTrack.getMinBufferSize(48000, AudioFormat.CHANNEL_OUT_STEREO, AudioFormat.ENCODING_PCM_16BIT)))
                .setTransferMode(AudioTrack.MODE_STREAM)
                .apply {
                    if (shouldUseLowLatencyStreamAudio(android.os.Build.VERSION.SDK_INT, lowLatencyGameAudio)) {
                        setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
                    }
                }
                .build()
            check(openedTrack.state == AudioTrack.STATE_INITIALIZED) { "NVST audio output unavailable" }
            openedTrack.play()
            track = openedTrack
        } catch (error: Exception) {
            openedTrack?.release()
            codec.release()
            throw error
        }
    }

    fun feed(bytes: ByteBuffer, ptsUs: Long, muted: Boolean) {
        if (appliedMute != muted) {
            if (track.setVolume(if (muted) 0f else 1f) == AudioTrack.SUCCESS) appliedMute = muted
        }
        drain()
        // This dedicated audio worker can wait briefly without holding up video or input.
        val index = codec.dequeueInputBuffer(5_000)
        if (index >= 0) {
            val size = bytes.remaining()
            codec.getInputBuffer(index)!!.apply { clear(); put(bytes) }
            codec.queueInputBuffer(index, 0, size, ptsUs, 0)
        } else {
            NativeInputDiagnostics.retainThrottled("nvst.audio-input", 1_000) {
                "NVST audio decoder input unavailable; packet dropped"
            }
        }
        drain()
    }

    fun drain() {
        drainPcm()
        repeat(8) {
            val output = codec.dequeueOutputBuffer(info, 0)
            if (output == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED || output == MediaCodec.INFO_OUTPUT_BUFFERS_CHANGED) return@repeat
            if (output < 0) return
            try {
                codec.getOutputBuffer(output)?.let { pcm ->
                    pcm.position(info.offset)
                    pcm.limit(info.offset + info.size)
                    if (pcm.hasRemaining()) {
                        runCatching {
                            recordingAudio(pcm.duplicate().apply {
                                position(info.offset)
                                limit(info.offset + info.size)
                            })
                        }
                    }
                    if (pending.isEmpty()) writePcm(pcm)
                    if (pcm.hasRemaining()) {
                        // A stalled output cannot retain unbounded audio or block the video worker.
                        while (pending.isNotEmpty() && pendingBytes + pcm.remaining() > 19200) {
                            pendingBytes -= pending.removeFirst().remaining()
                        }
                        if (pcm.remaining() <= 19200) {
                            val retained = ByteBuffer.allocateDirect(pcm.remaining()).apply { put(pcm); flip() }
                            pendingBytes += retained.remaining()
                            pending.addLast(retained)
                        }
                    }
                }
            } finally { codec.releaseOutputBuffer(output, false) }
        }
    }

    private fun drainPcm() {
        while (pending.isNotEmpty()) {
            val pcm = pending.first()
            val written = writePcm(pcm)
            if (written <= 0) return
            pendingBytes -= written
            if (pcm.hasRemaining()) return
            pending.removeFirst()
        }
    }

    private fun writePcm(pcm: ByteBuffer): Int =
        track.write(pcm, pcm.remaining(), AudioTrack.WRITE_NON_BLOCKING).also {
            check(it >= 0) { "NVST audio output write failed: $it" }
        }

    fun close() {
        try { codec.stop() } finally {
            try { codec.release() } finally { track.release() }
        }
    }
}
