package com.opencloudgaming.opennow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamAudioPolicyTest {
    @Test
    fun lowLatencyStartsAtAndroidOreo() {
        assertFalse(shouldUseLowLatencyStreamAudio(23))
        assertFalse(shouldUseLowLatencyStreamAudio(25))
        assertTrue(shouldUseLowLatencyStreamAudio(26))
        assertTrue(shouldUseLowLatencyStreamAudio(36))
    }
}
