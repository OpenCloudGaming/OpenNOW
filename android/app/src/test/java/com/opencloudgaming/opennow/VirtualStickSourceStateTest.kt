package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Test

class VirtualStickSourceStateTest {
    @Test
    fun lastActiveSourceWinsAndReleaseRestoresHeldInput() {
        val state = VirtualStickSourceState()

        assertEquals(StickInput(-1f, 0f), state.update("steer-left", -1f, 0f))
        assertEquals(StickInput(0.4f, -0.2f), state.update("joystick", 0.4f, -0.2f))
        assertEquals(StickInput(-1f, 0f), state.update("joystick", 0f, 0f))
        assertEquals(StickInput(), state.update("steer-left", 0f, 0f))
    }

    @Test
    fun oppositeSteeringButtonRestoresTheOneStillHeld() {
        val state = VirtualStickSourceState()

        state.update("left", -1f, 0f)
        assertEquals(StickInput(1f, 0f), state.update("right", 1f, 0f))
        assertEquals(StickInput(-1f, 0f), state.update("right", 0f, 0f))
    }
}
