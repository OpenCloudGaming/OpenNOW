package com.opencloudgaming.opennow

import android.media.AudioAttributes
import org.junit.Assert.assertEquals
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

    @Test
    fun standardMediaAudioDisablesLowLatencyAtEveryApiLevel() {
        assertFalse(shouldUseLowLatencyStreamAudio(25, lowLatencyGameAudio = false))
        assertFalse(shouldUseLowLatencyStreamAudio(29, lowLatencyGameAudio = false))
        assertFalse(shouldUseLowLatencyStreamAudio(36, lowLatencyGameAudio = false))
        assertEquals(AudioAttributes.USAGE_GAME, streamAudioUsage(lowLatencyGameAudio = true))
        assertEquals(AudioAttributes.USAGE_MEDIA, streamAudioUsage(lowLatencyGameAudio = false))
    }
}
