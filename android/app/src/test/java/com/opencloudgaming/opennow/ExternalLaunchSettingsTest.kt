package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ExternalLaunchSettingsTest {
    private val saved = StreamSettings(resolution = "1920x1080", fps = 60, maxBitrateMbps = 75, codec = VideoCodec.H264)

    @Test fun validValuesOverrideOnlyWhatIsGiven() {
        val result = ExternalLaunchSettings.apply(saved, "2560x1440", "120", null, "av1", null)!!
        assertEquals("2560x1440", result.resolution)
        assertEquals(120, result.fps)
        assertEquals(75, result.maxBitrateMbps)
        assertEquals(VideoCodec.AV1, result.codec)
        assertEquals(saved.hdrEnabled, result.hdrEnabled)
    }

    @Test fun invalidOrMissingValuesKeepTheSavedSettings() {
        assertNull(ExternalLaunchSettings.apply(saved, "huge", "999", "1", "vp9", "maybe"))
        assertNull(ExternalLaunchSettings.apply(saved, null, null, null, null, null))
        assertEquals(VideoCodec.H265, ExternalLaunchSettings.apply(saved, null, null, null, "HEVC", null)!!.codec)
        assertEquals(true, ExternalLaunchSettings.apply(saved, null, null, null, null, "on")!!.hdrEnabled)
    }
}
