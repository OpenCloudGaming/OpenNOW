package com.opencloudgaming.opennow

import android.view.InputDevice
import android.view.KeyEvent
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamControlsShortcutRoutingTest {
    @Test
    fun backAsEscapeIsOptInAndOnlyOwnsGameplayKeyboardBack() {
        assertTrue(NativeStreamInputRouter.shouldSendBackAsEscape(
            enabled = true, streamUiActive = false, clientAttached = true,
            keyCode = KeyEvent.KEYCODE_BACK, physicalKeyboardType = InputDevice.KEYBOARD_TYPE_ALPHABETIC,
        ))
        assertFalse(NativeStreamInputRouter.shouldSendBackAsEscape(
            enabled = false, streamUiActive = false, clientAttached = true,
            keyCode = KeyEvent.KEYCODE_BACK, physicalKeyboardType = InputDevice.KEYBOARD_TYPE_ALPHABETIC,
        ))
        assertFalse(NativeStreamInputRouter.shouldSendBackAsEscape(
            enabled = true, streamUiActive = true, clientAttached = true,
            keyCode = KeyEvent.KEYCODE_BACK, physicalKeyboardType = InputDevice.KEYBOARD_TYPE_ALPHABETIC,
        ))
        assertFalse(NativeStreamInputRouter.shouldSendBackAsEscape(
            enabled = true, streamUiActive = false, clientAttached = false,
            keyCode = KeyEvent.KEYCODE_BACK, physicalKeyboardType = InputDevice.KEYBOARD_TYPE_ALPHABETIC,
        ))
    }

    @Test
    fun dedicatedBackButtonsStayLocalAndPhysicalEscapeScanCodeStillWorks() {
        assertFalse(NativeStreamInputRouter.shouldSendBackAsEscape(
            enabled = true, streamUiActive = false, clientAttached = true,
            keyCode = KeyEvent.KEYCODE_BACK, physicalKeyboardType = null,
        ))
        assertFalse(NativeStreamInputRouter.shouldSendBackAsEscape(
            enabled = true, streamUiActive = false, clientAttached = true,
            keyCode = KeyEvent.KEYCODE_BACK, physicalKeyboardType = InputDevice.KEYBOARD_TYPE_NON_ALPHABETIC,
            physicalKeyboardScanCode = 158,
        ))
        assertTrue(NativeStreamInputRouter.shouldSendBackAsEscape(
            enabled = true, streamUiActive = false, clientAttached = true,
            keyCode = KeyEvent.KEYCODE_BACK, physicalKeyboardType = InputDevice.KEYBOARD_TYPE_NON_ALPHABETIC,
            physicalKeyboardScanCode = 1,
        ))
    }

    @Test
    fun backAsEscapePreservesControllerSelectMouseRightClickAndTvRemoteAliases() {
        assertFalse(NativeStreamInputRouter.shouldSendBackAsEscape(
            enabled = true, streamUiActive = false, clientAttached = true,
            keyCode = KeyEvent.KEYCODE_BACK, physicalKeyboardType = InputDevice.KEYBOARD_TYPE_ALPHABETIC,
            controllerInputDevice = true,
        ))
        assertFalse(NativeStreamInputRouter.shouldSendBackAsEscape(
            enabled = true, streamUiActive = false, clientAttached = true,
            keyCode = KeyEvent.KEYCODE_BACK, physicalKeyboardType = InputDevice.KEYBOARD_TYPE_ALPHABETIC,
            externalMouseInputDevice = true,
        ))
        for (key in listOf(KeyEvent.KEYCODE_BUTTON_B, KeyEvent.KEYCODE_ESCAPE, KeyEvent.KEYCODE_ENTER)) {
            assertFalse(NativeStreamInputRouter.shouldSendBackAsEscape(
                enabled = true, streamUiActive = false, clientAttached = true, keyCode = key,
                physicalKeyboardType = InputDevice.KEYBOARD_TYPE_ALPHABETIC,
            ))
        }
    }

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
    fun knownTvRemoteOkKeepsItsShortcutWithTheKeyboardSourceBitSet() {
        assertTrue(NativeStreamInputRouter.shouldOpenStreamControlsShortcutKey(
            keyCode = KeyEvent.KEYCODE_DPAD_CENTER,
            streamUiActive = false,
            androidTvProfile = true,
            controllerInputDevice = false,
            hardwareKeyboardSource = true,
            externalMouseInputDevice = false,
            dpadSource = true,
            knownTvRemote = true,
        ))
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
