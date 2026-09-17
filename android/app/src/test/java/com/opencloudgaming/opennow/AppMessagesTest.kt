package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class AppMessagesTest {
    @Test fun acknowledgementSuppressesSameAndOlderIdsButAllowsNewIds() {
        val message = AppMessage(5, "News", "Body")
        assertTrue(message.isUnacknowledged(4))
        assertFalse(message.isUnacknowledged(5))
        assertFalse(message.isUnacknowledged(6))
    }
    @Test fun acceptsPublicMessageAndNoMessage() {
        assertEquals(AppMessage(1, "News", "Body"), parseAppMessage("""{"status":true,"data":{"id":1,"title":"News","body":"Body","active":true}}"""))
        assertNull(parseAppMessage("""{"status":true,"data":null}"""))
    }
    @Test fun rejectsMalformedOrInactiveMessages() {
        for (payload in listOf(
            """{"status":false,"data":null}""",
            """{"status":true,"data":{"id":0,"title":"News","body":"Body","active":true}}""",
            """{"status":true,"data":{"id":1,"title":"News","body":"Body","active":false}}""",
            """{"status":true,"data":{}}""",
        )) assertTrue(runCatching { parseAppMessage(payload) }.isFailure)
    }
    @Test fun warnsForDemandingProfilesWithoutChangingSettings() {
        val ordinary = StreamSettings(resolution = "1920x1080", fps = 60, maxBitrateMbps = 75)
        assertFalse(ordinary.shouldWarnAboutHighSettings())
        assertFalse(ordinary.copy(resolution = "2560x1440", fps = 120, maxBitrateMbps = 100).shouldWarnAboutHighSettings())
        assertTrue(ordinary.copy(resolution = "3840x2160").shouldWarnAboutHighSettings())
        assertTrue(ordinary.copy(fps = 240).shouldWarnAboutHighSettings())
        assertTrue(ordinary.copy(maxBitrateMbps = 200).shouldWarnAboutHighSettings())
        assertEquals(60, ordinary.fps)
    }
}
