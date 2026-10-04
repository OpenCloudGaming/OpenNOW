package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class MouseButtonPressSourcesTest {
    @Test
    fun physicalAndTouchMouseHoldSameButtonUntilBothRelease() {
        val sources = MouseButtonPressSources()
        val edges = mutableListOf<Boolean>()
        fun update(source: String, pressed: Boolean) = sources.update(1, source, pressed) {
            edges += it
            true
        }

        assertTrue(update("physical", true))
        assertTrue(update("touch", true))
        assertTrue(update("physical", false))
        assertEquals(listOf(true), edges)
        assertTrue(update("touch", false))
        assertEquals(listOf(true, false), edges)

        assertTrue(update("touch", true))
        assertTrue(update("physical", true))
        assertTrue(update("touch", false))
        assertEquals(listOf(true, false, true), edges)
        assertTrue(update("physical", false))
        assertEquals(listOf(true, false, true, false), edges)
    }

    @Test
    fun failedEdgeDoesNotLeavePhantomSourceOrStaleHold() {
        val sources = MouseButtonPressSources()
        val edges = mutableListOf<Boolean>()
        assertFalse(sources.update(1, "physical", true) { false })
        assertTrue(sources.update(1, "touch", true) { edges += it; true })
        assertEquals(listOf(true), edges)

        assertFalse(sources.update(1, "touch", false) { false })
        assertTrue(sources.update(1, "physical", true) { edges += it; true })
        assertEquals(listOf(true, true), edges)
        assertTrue(sources.update(1, "physical", false) { edges += it; true })
        assertEquals(listOf(true, true, false), edges)
    }
}
