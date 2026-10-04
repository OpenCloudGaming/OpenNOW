package com.opencloudgaming.opennow

import android.content.Context
import android.hardware.input.InputManager
import android.os.Build
import android.view.InputDevice
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext

internal fun isMouseInputSource(sources: Int): Boolean =
    (sources and InputDevice.SOURCE_MOUSE) == InputDevice.SOURCE_MOUSE ||
        (sources and InputDevice.SOURCE_MOUSE_RELATIVE) == InputDevice.SOURCE_MOUSE_RELATIVE

internal fun isConnectedPhysicalInputDevice(isVirtual: Boolean, isExternal: Boolean): Boolean =
    !isVirtual && isExternal

private fun InputDevice.isConnectedPhysicalInputDevice(): Boolean =
    isConnectedPhysicalInputDevice(
        isVirtual = isVirtual,
        // Before Android 10 there is no public external-device flag. Keep the existing
        // behavior there rather than guessing from a device name or source mask.
        isExternal = Build.VERSION.SDK_INT < Build.VERSION_CODES.Q || isExternal,
    )

internal fun isPhysicalMouseDevice(device: InputDevice?): Boolean =
    device != null && device.isConnectedPhysicalInputDevice() && isMouseInputSource(device.sources)

internal fun isKeyboardInputSource(sources: Int, keyboardType: Int): Boolean =
    (sources and InputDevice.SOURCE_KEYBOARD) == InputDevice.SOURCE_KEYBOARD &&
        keyboardType == InputDevice.KEYBOARD_TYPE_ALPHABETIC

internal fun isPhysicalKeyboardDevice(device: InputDevice?): Boolean =
    device != null &&
        device.isConnectedPhysicalInputDevice() &&
        isKeyboardInputSource(device.sources, device.keyboardType)

internal data class PhysicalKeyboardMouseConnection(
    val mouseConnected: Boolean,
    val keyboardConnected: Boolean,
) {
    val connected: Boolean
        get() = mouseConnected || keyboardConnected
}

internal fun connectedPhysicalKeyboardMouse(): PhysicalKeyboardMouseConnection = runCatching {
    val devices = InputDevice.getDeviceIds().map(InputDevice::getDevice).filterNotNull()
    PhysicalKeyboardMouseConnection(
        mouseConnected = devices.any(::isPhysicalMouseDevice),
        keyboardConnected = devices.any(::isPhysicalKeyboardDevice),
    )
}.getOrDefault(PhysicalKeyboardMouseConnection(mouseConnected = false, keyboardConnected = false))

internal fun hasConnectedPhysicalKeyboardOrMouse(): Boolean = connectedPhysicalKeyboardMouse().connected

@Composable
internal fun rememberPhysicalKeyboardMouseConnection(enabled: Boolean): PhysicalKeyboardMouseConnection {
    val context = LocalContext.current.applicationContext
    val disconnected = PhysicalKeyboardMouseConnection(mouseConnected = false, keyboardConnected = false)
    var connection by remember(enabled) {
        mutableStateOf(if (enabled) connectedPhysicalKeyboardMouse() else disconnected)
    }
    DisposableEffect(context, enabled) {
        fun refresh() {
            connection = if (enabled) connectedPhysicalKeyboardMouse() else disconnected
        }
        refresh()
        if (!enabled) {
            onDispose {}
        } else {
            val inputManager = context.getSystemService(Context.INPUT_SERVICE) as? InputManager
            val listener = object : InputManager.InputDeviceListener {
                override fun onInputDeviceAdded(deviceId: Int) = refresh()
                override fun onInputDeviceRemoved(deviceId: Int) = refresh()
                override fun onInputDeviceChanged(deviceId: Int) = refresh()
            }
            inputManager?.registerInputDeviceListener(listener, null)
            onDispose {
                inputManager?.unregisterInputDeviceListener(listener)
            }
        }
    }
    return connection
}
