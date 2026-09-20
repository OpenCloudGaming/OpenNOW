package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class AppPageMotionTest {
    @Test
    fun browsePagesMoveInNavigationOrder() {
        assertEquals(1f, appPageMotionDirection(AppPage.Home, AppPage.Library))
        assertEquals(1f, appPageMotionDirection(AppPage.Library, AppPage.Settings))
        assertEquals(-1f, appPageMotionDirection(AppPage.Settings, AppPage.Home))
    }

    @Test
    fun browsePageMotionHonorsReducedMotionAndSkipsStream() {
        assertTrue(shouldAnimateAppPageEntrance(AppPage.Home, AppPage.Library, reduceMotion = false))
        assertFalse(shouldAnimateAppPageEntrance(AppPage.Home, AppPage.Library, reduceMotion = true))
        assertFalse(shouldAnimateAppPageEntrance(AppPage.Home, AppPage.Stream, reduceMotion = false))
        assertFalse(shouldAnimateAppPageEntrance(AppPage.Stream, AppPage.Home, reduceMotion = false))
        assertFalse(shouldAnimateAppPageEntrance(AppPage.Home, AppPage.Home, reduceMotion = false))
    }

    @Test
    fun catalogCardLiftFollowsScaleWithoutOvershooting() {
        assertEquals(0f, catalogCardLiftDp(1f))
        assertEquals(-2f, catalogCardLiftDp(1.04f), 0.001f)
        assertEquals(-4f, catalogCardLiftDp(1.20f), 0.001f)
        assertEquals(2f, catalogCardLiftDp(0.90f), 0.001f)
    }
}
