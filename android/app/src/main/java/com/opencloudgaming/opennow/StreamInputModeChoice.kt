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
 * Prevent a noisy composite or Bluetooth input device from repeatedly presenting the same
 * connection-change prompt. The choice is still asked again in a new stream session, and the
 * opposite transition can still be offered once if the player changes modes during this session.
 */
internal class StreamInputModePromptGate {
    private val presented = mutableSetOf<StreamInputModePrompt>()

    fun shouldPresent(prompt: StreamInputModePrompt): Boolean = presented.add(prompt)
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
