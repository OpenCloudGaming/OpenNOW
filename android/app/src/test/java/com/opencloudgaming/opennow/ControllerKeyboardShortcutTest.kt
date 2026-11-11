package com.opencloudgaming.opennow

import android.view.KeyEvent
import com.opencloudgaming.opennow.ControllerKeyboardShortcut.Route
import org.junit.Assert.assertEquals
import org.junit.Test

class ControllerKeyboardShortcutTest {
    private val shortcut = ControllerKeyboardShortcut()

    @Test
    fun opensOnceAndConsumesRepeatAndReleaseAfterUiOrModeChanges() {
        assertEquals(Route.OpenKeyboard, route())
        assertEquals(Route.Consume, route(repeat = 1, uiActive = true))
        assertEquals(Route.Consume, route(action = KeyEvent.ACTION_UP, enabled = false, uiActive = true))
        assertEquals(Route.Pass, route(action = KeyEvent.ACTION_UP))
        assertEquals(Route.OpenKeyboard, route())
    }

    @Test
    fun leavesOrdinaryGamepadAssistModeKeyboardAndUiInputAlone() {
        assertEquals(Route.Pass, route(enabled = false))
        assertEquals(Route.Pass, route(controller = false))
        assertEquals(Route.Pass, route(uiActive = true))
        assertEquals(Route.Pass, route(key = KeyEvent.KEYCODE_BUTTON_X))
        assertEquals(Route.Pass, route(repeat = 1))
        assertEquals(Route.Pass, route(action = KeyEvent.ACTION_UP))
    }

    @Test
    fun aLostReleaseDoesNotSwallowTheNextPhysicalPress() {
        assertEquals(Route.OpenKeyboard, route())
        assertEquals(Route.OpenKeyboard, route())
        assertEquals(Route.Consume, route(action = KeyEvent.ACTION_UP, uiActive = true))
    }

    @Test
    fun eachControllerOwnsItsReleaseAndDetachClearsHeldKeys() {
        assertEquals(Route.OpenKeyboard, route(deviceId = 1))
        assertEquals(Route.Pass, route(action = KeyEvent.ACTION_UP, deviceId = 2))
        assertEquals(Route.Consume, route(action = KeyEvent.ACTION_UP, deviceId = 1))
        assertEquals(Route.OpenKeyboard, route(deviceId = 2))
        shortcut.reset()
        assertEquals(Route.Pass, route(action = KeyEvent.ACTION_UP, deviceId = 2))
        assertEquals(Route.OpenKeyboard, route(deviceId = 2))
    }

    private fun route(
        key: Int = KeyEvent.KEYCODE_BUTTON_Y,
        action: Int = KeyEvent.ACTION_DOWN,
        repeat: Int = 0,
        deviceId: Int = 1,
        controller: Boolean = true,
        enabled: Boolean = true,
        uiActive: Boolean = false,
    ) = shortcut.route(key, action, repeat, deviceId, controller, enabled, uiActive)
}
