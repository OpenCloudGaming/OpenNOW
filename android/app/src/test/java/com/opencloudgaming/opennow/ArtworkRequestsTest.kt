package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Test

class ArtworkRequestsTest {
    @Test
    fun legacyArtworkUsesJpegAndSeparateCacheUrlsWhileKeepingTheRequestedSize() {
        val base = "https://img.nvidiagrid.net/apps/123/ZZ/GAME_BOX_ART_01_example.jpg"
        val cachedWebp = "$base;f=webp;w=1024;h=576;dpr=2"
        for (sdk in 24..27) {
            assertEquals("$base;f=jpg;w=272", optimizedNvidiaImageUrl(cachedWebp, 272, sdk))
        }
        for (sdk in listOf(28, 29, 35, 37)) {
            assertEquals("$base;f=webp;w=272", optimizedNvidiaImageUrl(cachedWebp, 272, sdk))
        }
    }

    @Test
    fun otherProvidersKeepTheirOriginalArtworkUrls() {
        val url = "https://example.com/poster.webp?width=512"
        assertEquals(url, optimizedNvidiaImageUrl(url, 272, 24))
        assertEquals(url, optimizedNvidiaImageUrl(url, 272, 37))
    }
}
