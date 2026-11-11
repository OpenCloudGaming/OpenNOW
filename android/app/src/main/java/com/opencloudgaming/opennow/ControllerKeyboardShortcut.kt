package com.opencloudgaming.opennow

import android.view.KeyEvent

/** Own the complete physical press, even after opening the keyboard changes UI routing. */
internal class ControllerKeyboardShortcut {
    enum class Route { Pass, Consume, OpenKeyboard }

    private val heldDevices = mutableSetOf<Int>()

    fun route(
        keyCode: Int,
        action: Int,
        repeatCount: Int,
        deviceId: Int,
        controllerInputDevice: Boolean,
        enabled: Boolean,
        streamUiActive: Boolean,
    ): Route {
        if (keyCode != KeyEvent.KEYCODE_BUTTON_Y) return Route.Pass
        if (deviceId in heldDevices) {
            // Focus loss or controller disconnect can drop UP. A fresh physical DOWN starts a
            // new press; otherwise one lost release would swallow the next keyboard shortcut.
            if (action == KeyEvent.ACTION_DOWN && repeatCount == 0) {
                heldDevices.remove(deviceId)
            } else {
                if (action == KeyEvent.ACTION_UP) heldDevices.remove(deviceId)
                return Route.Consume
            }
        }
        if (!enabled || streamUiActive || !controllerInputDevice ||
            action != KeyEvent.ACTION_DOWN || repeatCount != 0
        ) return Route.Pass
        heldDevices += deviceId
        return Route.OpenKeyboard
    }

    fun reset() = heldDevices.clear()
}
