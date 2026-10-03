package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test
import androidx.compose.ui.graphics.Color

class AppMessagesTest {
    @Test fun formatsAnnouncementMarkersWithoutChangingPlainText() {
        val rendered = formattedAppMessageBody(
            "Line one\n**Important** <red>Warning</red> [details](https://example.com)",
            Color.Blue,
            Color.Red,
        )
        assertEquals("Line one\nImportant Warning details", rendered.text)
        assertEquals(3, rendered.spanStyles.size)
        assertEquals("Literal http://example.com", formattedAppMessageBody("Literal http://example.com", Color.Blue, Color.Red).text)
    }
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
        val ordinary = StreamSettings(resolution = "1920x1080", fps = 60, maxBitrateMbps = 75.0)
        assertFalse(ordinary.shouldWarnAboutHighSettings())
        assertFalse(ordinary.copy(resolution = "2560x1440", fps = 120, maxBitrateMbps = 100.0).shouldWarnAboutHighSettings())
        assertTrue(ordinary.copy(resolution = "3840x2160").shouldWarnAboutHighSettings())
        assertTrue(ordinary.copy(fps = 240).shouldWarnAboutHighSettings())
        assertTrue(ordinary.copy(maxBitrateMbps = 200.0).shouldWarnAboutHighSettings())
        assertEquals(60, ordinary.fps)
    }
}
