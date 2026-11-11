package com.opencloudgaming.opennow

import android.os.SystemClock
import androidx.test.ext.junit.runners.AndroidJUnit4
import java.nio.ByteBuffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NvstAudioOutputInstrumentedTest {
    @Test fun finalOpusPacketDrainsWithoutAnotherNetworkPacketAcrossRestart() {
        repeat(3) {
            var decodedBytes = 0
            // Standard 20 ms Opus silence packet. No server, video, or controller is required.
            val output = NvstAudioOutput(lowLatencyGameAudio = false) { decodedBytes += it.remaining() }
            try {
                output.feed(ByteBuffer.wrap(byteArrayOf(0xf8.toByte(), 0xff.toByte(), 0xfe.toByte())), 0, muted = true)
                val deadline = SystemClock.elapsedRealtime() + 2_000
                while (decodedBytes == 0 && SystemClock.elapsedRealtime() < deadline) {
                    SystemClock.sleep(5)
                    output.drain()
                }
                assertTrue("Idle ticks must drain the final packet", decodedBytes > 0)
                assertEquals("PCM must contain complete 16-bit stereo frames", 0, decodedBytes % 4)
            } finally { output.close() }
        }
    }
}
