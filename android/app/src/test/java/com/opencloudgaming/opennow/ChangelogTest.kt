package com.opencloudgaming.opennow

import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ChangelogTest {
    @Test
    fun versionCodesSelectOnlyPublishedBuilds() {
        val entries = parseChangelogs(
            """{"159":{"version":"2.0.3","notes":["Old"]},"160":{"version":"2.0.4","notes":["New"]},"161":{"version":"2.0.5","notes":["Future"]},"oops":{"version":"bad","notes":["Bad"]}}""",
            currentVersionCode = 160,
        )
        assertEquals(listOf(160, 159), entries.map(ChangelogEntry::versionCode))
        assertEquals(listOf(160), changelogsForPlayUpdate(entries, 159, 160, true, true).map(ChangelogEntry::versionCode))
        assertEquals(listOf(160), changelogsForPlayUpdate(entries, null, 160, true, true).map(ChangelogEntry::versionCode))
        assertTrue(changelogsForPlayUpdate(entries, null, 160, true, false).isEmpty())
        assertTrue(changelogsForPlayUpdate(entries, 159, 160, false, true).isEmpty())
        assertTrue(changelogsForPlayUpdate(entries, 160, 160, true, true).isEmpty())
    }

    @Test
    fun inlineStylesCanBeCombined() {
        val text = formattedChangelogNote("**bold** *italic* ***__all__***")
        assertEquals("bold italic all", text.text)
        assertTrue(text.spanStyles.any { it.item.fontWeight == FontWeight.Bold && it.start == 0 && it.end == 4 })
        assertTrue(text.spanStyles.any { it.item.fontStyle == FontStyle.Italic && it.start == 5 && it.end == 11 })
        assertTrue(text.spanStyles.any { it.item.fontWeight == FontWeight.Bold && it.item.fontStyle == FontStyle.Italic && it.start == 12 && it.end == 15 })
        assertTrue(text.spanStyles.any { it.item.textDecoration == TextDecoration.Underline && it.start == 12 && it.end == 15 })
        assertFalse(text.text.contains('*'))
    }
}
