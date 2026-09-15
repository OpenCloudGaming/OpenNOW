package com.opencloudgaming.opennow

enum class StreamInputMode {
    NativeTouch,
    KeyboardMouse,
}

internal enum class StreamInputModePrompt {
    SwitchToKeyboardMouse,
    SwitchToNativeTouch,
}

/**
 * Fallback for sessions without a saved launch choice. New launches use
 * [chooseStreamInputModeAtStart] before provisioning the host input devices.
 */
internal fun streamInputModeAtStart(
    nativeTouchAvailable: Boolean,
    keyboardMouseConnected: Boolean,
): StreamInputMode = if (nativeTouchAvailable && !keyboardMouseConnected) {
    StreamInputMode.NativeTouch
} else {
    StreamInputMode.KeyboardMouse
}

/** Wait for a choice before CloudMatch creates a session with a mouse attached. */
internal suspend fun chooseStreamInputModeAtStart(
    nativeTouchAvailable: Boolean,
    keyboardMouseConnected: Boolean,
    choose: suspend () -> StreamInputMode,
): StreamInputMode = if (nativeTouchAvailable && keyboardMouseConnected) {
    choose()
} else {
    streamInputModeAtStart(nativeTouchAvailable, keyboardMouseConnected)
}

internal fun streamInputModePromptForConnectionChange(
    currentMode: StreamInputMode,
    keyboardMouseConnected: Boolean,
    nativeTouchProvisionedForSession: Boolean,
): StreamInputModePrompt? = when {
    keyboardMouseConnected && currentMode == StreamInputMode.NativeTouch ->
        StreamInputModePrompt.SwitchToKeyboardMouse
    !keyboardMouseConnected &&
        currentMode == StreamInputMode.KeyboardMouse &&
        nativeTouchProvisionedForSession -> StreamInputModePrompt.SwitchToNativeTouch
    else -> null
}
