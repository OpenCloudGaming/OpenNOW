package com.opencloudgaming.opennow

import android.os.Build
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assume.assumeTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class NativeCodecProbeInstrumentedTest {
    @Test
    fun android16MetadataProbeSupportsRepeatedQueriesWithoutAllocatingCodecs() {
        assumeTrue(Build.VERSION.SDK_INT >= 36)
        assertTrue(NativeCodecProbe.nativeRuntimeSummary().contains("\"decoderProbe\":\"metadata\""))
        repeat(20) {
            assertTrue(NativeCodecProbe.nativeDecoderAvailable("video/avc"))
            assertFalse(NativeCodecProbe.nativeDecoderAvailable("video/opennow-unsupported"))
        }
    }
}
