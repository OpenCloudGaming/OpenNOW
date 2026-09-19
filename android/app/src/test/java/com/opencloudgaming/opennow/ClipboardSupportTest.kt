package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ClipboardSupportTest {
    @Test
    fun shortLogCopyIsUnchanged() {
        assertEquals("hello", boundedLogClipboardText("hello", maxChars = 20))
    }

    @Test
    fun oversizedLogCopyStaysInsideBinderSafeLimitAndKeepsNewestTail() {
        val report = "HEADER\n" + "x".repeat(200) + "\nNEWEST"
        val copy = boundedLogClipboardText(report, maxChars = 120)

        assertEquals(120, copy.length)
        assertTrue(copy.startsWith("HEADER"))
        assertTrue(copy.contains("Clipboard copy shortened"))
        assertTrue(copy.endsWith("NEWEST"))
    }
}
