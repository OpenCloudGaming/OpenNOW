package com.opencloudgaming.opennow

import android.view.KeyEvent
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamControlsShortcutRoutingTest {
    private val activationKeys = listOf(
        KeyEvent.KEYCODE_ENTER,
        KeyEvent.KEYCODE_NUMPAD_ENTER,
        KeyEvent.KEYCODE_DPAD_CENTER,
    )

    @Test
    fun startHoldBelongsOnlyToControllerGameplay() {
        assertTrue(NativeStreamInputRouter.shouldTrackControllerStartHold(
            KeyEvent.KEYCODE_BUTTON_START, controllerInputDevice = true, streamUiActive = false,
        ))
        assertFalse(NativeStreamInputRouter.shouldTrackControllerStartHold(
            KeyEvent.KEYCODE_BUTTON_START, controllerInputDevice = false, streamUiActive = false,
        ))
        assertFalse(NativeStreamInputRouter.shouldTrackControllerStartHold(
            KeyEvent.KEYCODE_BUTTON_START, controllerInputDevice = true, streamUiActive = true,
        ))
        assertFalse(NativeStreamInputRouter.shouldTrackControllerStartHold(
            KeyEvent.KEYCODE_DPAD_UP, controllerInputDevice = true, streamUiActive = false,
        ))
    }

    @Test
    fun tabletDockActivationCannotBypassReboundOrDisabledShortcut() {
        for (shortcut in listOf("F8", DISABLED_ANDROID_STREAM_MENU_SHORTCUT)) {
            for (key in activationKeys + KeyEvent.KEYCODE_DPAD_UP) {
                assertFalse(NativeStreamInputRouter.shouldOpenStreamSystemMenuKey(
                    keyCode = key,
                    controllerInputDevice = false,
                    configuredShortcut = shortcut,
                ))
                // Even a dock that reports only DPAD must not get a tablet menu fallback.
                assertFalse(remoteActivation(key, tv = false))
                assertFalse(remoteActivation(key, tv = false, mouse = true))
                assertFalse(remoteActivation(key, tv = false, keyboard = true))
            }
        }
    }

    @Test
    fun compositeMouseReceiverDoesNotGetTvRemoteMenuFallback() {
        for (key in activationKeys) {
            assertFalse(remoteActivation(key, mouse = true))
            assertFalse(remoteActivation(key, keyboard = true))
            assertFalse(remoteActivation(key, controller = true))
        }
    }

    @Test
    fun genuineTvRemoteKeepsOkShortcutOnlyDuringGameplay() {
        for (key in activationKeys) {
            assertTrue(remoteActivation(key))
            assertFalse(remoteActivation(key, uiActive = true))
            assertFalse(remoteActivation(key, dpad = false))
        }
        assertFalse(remoteActivation(KeyEvent.KEYCODE_DPAD_UP))
    }

    @Test
    fun rebindReplacesOldShortcutAndUpArrowRequiresExplicitBinding() {
        assertTrue(NativeStreamInputRouter.shouldOpenStreamSystemMenuKey(
            keyCode = KeyEvent.KEYCODE_F8,
            controllerInputDevice = false,
            configuredShortcut = "F8",
        ))
        assertFalse(NativeStreamInputRouter.shouldOpenStreamSystemMenuKey(
            keyCode = KeyEvent.KEYCODE_G,
            ctrlPressed = true,
            shiftPressed = true,
            controllerInputDevice = false,
            configuredShortcut = "F8",
        ))
        assertTrue(NativeStreamInputRouter.shouldOpenStreamSystemMenuKey(
            keyCode = KeyEvent.KEYCODE_DPAD_UP,
            controllerInputDevice = false,
            configuredShortcut = "Up",
        ))
    }

    private fun remoteActivation(
        key: Int,
        tv: Boolean = true,
        mouse: Boolean = false,
        keyboard: Boolean = false,
        controller: Boolean = false,
        uiActive: Boolean = false,
        dpad: Boolean = true,
    ): Boolean = NativeStreamInputRouter.shouldOpenStreamControlsShortcutKey(
        keyCode = key,
        streamUiActive = uiActive,
        androidTvProfile = tv,
        controllerInputDevice = controller,
        hardwareKeyboardSource = keyboard,
        externalMouseInputDevice = mouse,
        dpadSource = dpad,
    )
}
