package com.opencloudgaming.opennow

enum class StreamInputMode {
    NativeTouch,
    KeyboardMouse,
}

/** A saved keyboard overlay preference must not override the host's selected touch mode. */
internal fun keyboardOverlayEnabledForStream(
    savedEnabled: Boolean,
    currentMode: StreamInputMode,
): Boolean = savedEnabled && currentMode == StreamInputMode.KeyboardMouse

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

/** Touch-capable games need a host input choice even when no physical device is connected. */
internal fun shouldPromptForLaunchInputMode(
    nativeTouchAvailable: Boolean,
    catalogTouchSupported: Boolean,
    keyboardMouseConnected: Boolean,
): Boolean = nativeTouchAvailable && (catalogTouchSupported || keyboardMouseConnected)

/** Wait for a choice before CloudMatch provisions the session's input devices. */
internal suspend fun chooseStreamInputModeAtStart(
    nativeTouchAvailable: Boolean,
    keyboardMouseConnected: Boolean,
    promptForChoice: Boolean,
    choose: suspend () -> StreamInputMode,
): StreamInputMode = if (promptForChoice) {
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
