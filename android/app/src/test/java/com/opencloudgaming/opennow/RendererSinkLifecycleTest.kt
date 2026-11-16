package com.opencloudgaming.opennow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class RendererSinkLifecycleTest {
    @Test
    fun repeatedSurfaceCallbacksQueueOnlyOneAttachAndDetach() {
        val lifecycle = RendererSinkLifecycle()

        assertTrue(lifecycle.requestAttach { true })
        assertFalse(lifecycle.requestAttach { true })
        assertTrue(lifecycle.isAttachRequested())

        assertTrue(lifecycle.requestDetach())
        assertFalse(lifecycle.requestDetach())
        assertFalse(lifecycle.isAttachRequested())
    }

    @Test
    fun recreatedSurfaceCanAttachAfterDetach() {
        val lifecycle = RendererSinkLifecycle()

        assertTrue(lifecycle.requestAttach { true })
        assertTrue(lifecycle.requestDetach())
        assertTrue(lifecycle.requestAttach { true })
        assertTrue(lifecycle.isAttachRequested())
    }

    @Test
    fun queuedAttachDoesNotReserveSinkForRetiredSurface() {
        val lifecycle = RendererSinkLifecycle()
        assertFalse(lifecycle.requestAttach { false })
        assertFalse(lifecycle.isAttachRequested())
        assertTrue(lifecycle.requestAttach { true })
    }
}
