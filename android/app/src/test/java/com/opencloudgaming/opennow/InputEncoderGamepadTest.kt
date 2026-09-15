package com.opencloudgaming.opennow

import android.view.InputDevice
import android.view.KeyEvent
import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class InputEncoderGamepadTest {
    @Test
    fun encodesGuideButtonMaskForSteamOverlay() {
        val encoder = InputEncoder().apply { setProtocolVersion(2) }
        val payload = encoder.encodeGamepadState(
            controllerId = 0,
            buttons = 0x0400,
            leftTrigger = 0,
            rightTrigger = 0,
            leftStickX = 0,
            leftStickY = 0,
            rightStickX = 0,
            rightStickY = 0,
            bitmap = 0x0101,
            partiallyReliable = false,
            timestampUs = 0L,
        )
        val bytes = ByteBuffer.wrap(payload).order(ByteOrder.LITTLE_ENDIAN)

        assertEquals(InputEncoder.INPUT_GAMEPAD, bytes.getInt(0))
        assertEquals(0x0400, bytes.getShort(12).toInt() and 0xffff)
    }

    @Test
    fun mapsAndroidGamepadButtonsToXinputMasks() {
        assertEquals(GamepadButtonMapping.GUIDE, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BUTTON_MODE))
        assertEquals(GamepadButtonMapping.START, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BUTTON_START))
        assertEquals(GamepadButtonMapping.BACK, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BUTTON_SELECT))
        assertEquals(GamepadButtonMapping.START, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_MENU, controllerActivation = true))
        assertEquals(GamepadButtonMapping.BACK, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BACK, controllerActivation = true))
        assertNull(GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_MENU))
        assertNull(GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BACK))
        assertEquals(GamepadButtonMapping.LEFT_THUMB, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BUTTON_THUMBL))
        assertEquals(GamepadButtonMapping.RIGHT_THUMB, GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_BUTTON_THUMBR))
    }

    @Test
    fun buildsSteamMenuAsGuideHeldWithA() {
        assertEquals(GamepadButtonMapping.GUIDE, SteamMenuChord.buttons(aPressed = false))
        assertEquals(
            GamepadButtonMapping.GUIDE,
            SteamMenuChord.buttons(aPressed = true),
        )
    }

    @Test
    fun viewAndStartChordSendsHomeAWithoutLeakingTopButtons() {
        val chord = SteamOverlayChordState()

        assertFalse(chord.update(GamepadButtonMapping.BACK))
        assertEquals(GamepadButtonMapping.BACK, chord.effectiveButtons(GamepadButtonMapping.BACK))

        val both = GamepadButtonMapping.BACK or GamepadButtonMapping.START
        assertTrue(chord.update(both))
        assertEquals(
            GamepadButtonMapping.GUIDE,
            chord.effectiveButtons(both),
        )
        assertTrue(chord.releaseChord())
        assertEquals(0, chord.effectiveButtons(both))

        assertFalse(chord.update(0))
        assertFalse(chord.update(GamepadButtonMapping.START))
        assertEquals(GamepadButtonMapping.START, chord.effectiveButtons(GamepadButtonMapping.START))
    }

    @Test
    fun classifiesControllerButtonKeyCodesWithoutDependingOnEventSource() {
        assertTrue(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_BUTTON_MODE))
        assertTrue(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_BUTTON_START))
        assertTrue(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_BUTTON_SELECT))
        assertTrue(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_BUTTON_L2))
        assertTrue(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_BUTTON_R2))
        assertFalse(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_DPAD_CENTER))
        assertFalse(GamepadButtonMapping.isControllerButtonKeyCode(KeyEvent.KEYCODE_ENTER))
    }

    @Test
    fun detectsControllerCapableAndroidSources() {
        assertTrue(AndroidControllerInput.hasControllerSource(InputDevice.SOURCE_GAMEPAD or InputDevice.SOURCE_DPAD))
        assertTrue(AndroidControllerInput.hasControllerSource(InputDevice.SOURCE_JOYSTICK or InputDevice.SOURCE_DPAD))
        assertFalse(AndroidControllerInput.hasControllerSource(InputDevice.SOURCE_DPAD))
        assertFalse(AndroidControllerInput.hasControllerSource(InputDevice.SOURCE_KEYBOARD or InputDevice.SOURCE_DPAD))
    }

    @Test
    fun doesNotAdvertiseCompositeKeyboardsAndMiceAsGamepads() {
        val misleadingSources =
            InputDevice.SOURCE_GAMEPAD or
                InputDevice.SOURCE_JOYSTICK or
                InputDevice.SOURCE_KEYBOARD or
                InputDevice.SOURCE_MOUSE

        assertFalse(AndroidControllerInput.isControllerDevice(misleadingSources, "SEMICO USB Keyboard System Control"))
        assertFalse(AndroidControllerInput.isControllerDevice(misleadingSources, "BT5.2 Mouse"))
        assertFalse(AndroidControllerInput.isControllerDevice(misleadingSources, "Gaming KB Gaming KB Keyboard"))
        assertFalse(AndroidControllerInput.isControllerDevice(misleadingSources, "Logitech USB Receiver"))
        assertFalse(AndroidControllerInput.isControllerDevice(misleadingSources, "uinput-goodix"))
        assertFalse(AndroidControllerInput.isControllerDevice(misleadingSources, "uinput-fpc"))
        assertFalse(AndroidControllerInput.isControllerDevice(misleadingSources, "Fingerprint Sensor"))
        assertTrue(AndroidControllerInput.isControllerDevice(misleadingSources, "Xbox Wireless Controller"))
    }

    @Test
    fun syntheticControllerEventsReuseTheLiveAndroidDeviceId() {
        val controllerSlots = linkedMapOf<Int, Int>()
        val assignment = AndroidControllerSlotRegistry.assign(
            controllerSlots = controllerSlots,
            deviceId = -1,
            connectedDeviceIds = setOf(14),
            maxControllers = 4,
        )

        assertEquals(0, assignment.slot)
        assertEquals(mapOf(14 to 0), controllerSlots)
        assertTrue(
            AndroidControllerSlotRegistry.retainConnected(
                controllerSlots = controllerSlots,
                connectedDeviceIds = setOf(14),
            ).isEmpty(),
        )
    }

    @Test
    fun syntheticControllerEventsPreferTheAlreadyAssignedPrimaryController() {
        val controllerSlots = linkedMapOf(14 to 0, 15 to 1)
        val assignment = AndroidControllerSlotRegistry.assign(
            controllerSlots = controllerSlots,
            deviceId = -1,
            connectedDeviceIds = setOf(14, 15),
            maxControllers = 4,
        )

        assertEquals(0, assignment.slot)
        assertTrue(assignment.removedDevices.isEmpty())
        assertEquals(mapOf(14 to 0, 15 to 1), controllerSlots)
    }

    @Test
    fun neutralControllerKeepaliveDoesNotFightFingerMouse() {
        assertFalse(
            shouldSendGamepadKeepalive(
                hasControllerState = true,
                hasActiveControllerInput = false,
                touchMouseEnabled = true,
            ),
        )
        assertTrue(
            shouldSendGamepadKeepalive(
                hasControllerState = true,
                hasActiveControllerInput = true,
                touchMouseEnabled = true,
            ),
        )
        assertTrue(
            shouldSendGamepadKeepalive(
                hasControllerState = true,
                hasActiveControllerInput = false,
                touchMouseEnabled = false,
            ),
        )
    }

    @Test
    fun reusesPrimarySlotWhenAndroidReassignsControllerDeviceId() {
        val controllerSlots = linkedMapOf<Int, Int>()
        val initial = AndroidControllerSlotRegistry.assign(
            controllerSlots = controllerSlots,
            deviceId = 21,
            connectedDeviceIds = setOf(21),
            maxControllers = 4,
        )

        val reconnected = AndroidControllerSlotRegistry.assign(
            controllerSlots = controllerSlots,
            deviceId = 44,
            connectedDeviceIds = setOf(44),
            maxControllers = 4,
        )

        assertEquals(0, initial.slot)
        assertEquals(mapOf(21 to 0), reconnected.removedDevices)
        assertEquals(0, reconnected.slot)
        assertEquals(mapOf(44 to 0), controllerSlots)
    }

    @Test
    fun disconnectScanReleasesControllerSlotBeforeReconnect() {
        val controllerSlots = linkedMapOf(21 to 0)

        val removed = AndroidControllerSlotRegistry.retainConnected(
            controllerSlots = controllerSlots,
            connectedDeviceIds = emptySet(),
        )
        val reconnected = AndroidControllerSlotRegistry.assign(
            controllerSlots = controllerSlots,
            deviceId = 44,
            connectedDeviceIds = setOf(44),
            maxControllers = 4,
        )

        assertEquals(mapOf(21 to 0), removed)
        assertEquals(0, reconnected.slot)
        assertEquals(mapOf(44 to 0), controllerSlots)
    }

    @Test
    fun reconnectReusesOnlyTheSlotVacatedByDisconnectedController() {
        val controllerSlots = linkedMapOf(21 to 0, 32 to 1)

        val reconnected = AndroidControllerSlotRegistry.assign(
            controllerSlots = controllerSlots,
            deviceId = 44,
            connectedDeviceIds = setOf(32, 44),
            maxControllers = 4,
        )

        assertEquals(mapOf(21 to 0), reconnected.removedDevices)
        assertEquals(0, reconnected.slot)
        assertEquals(mapOf(32 to 1, 44 to 0), controllerSlots)
    }

    @Test
    fun recognizesStadiaControllerNamesWithDpadOnlySources() {
        assertTrue(AndroidControllerInput.isKnownControllerName("Stadia Controller rev. A"))
        assertTrue(AndroidControllerInput.isKnownControllerName("Google Stadia Controller"))
        assertTrue(AndroidControllerInput.isControllerDevice(InputDevice.SOURCE_DPAD, "Stadia Controller"))
        assertTrue(AndroidControllerInput.isControllerDevice(InputDevice.SOURCE_DPAD, "DualSense Wireless Controller"))
        assertTrue(AndroidControllerInput.isControllerDevice(InputDevice.SOURCE_DPAD, "Xbox Wireless Controller"))
        assertFalse(AndroidControllerInput.isControllerDevice(InputDevice.SOURCE_DPAD, "TV Remote"))
    }

    @Test
    fun classifiesControllerFamiliesForBackButtonHints() {
        assertEquals(AndroidControllerFamily.Google, AndroidControllerInput.controllerFamily("Chromecast Remote"))
        assertEquals(AndroidControllerFamily.Xbox, AndroidControllerInput.controllerFamily("Xbox Wireless Controller"))
        assertEquals(AndroidControllerFamily.PlayStation, AndroidControllerInput.controllerFamily("DualSense Wireless Controller"))
        assertEquals(AndroidControllerFamily.PlayStation, AndroidControllerInput.controllerFamily("Sony Interactive Entertainment Wireless Controller"))
        assertEquals(AndroidControllerFamily.PlayStation, AndroidControllerInput.controllerFamily("Generic Gamepad", vendorId = 0x054c))
        assertEquals(AndroidControllerFamily.Nintendo, AndroidControllerInput.controllerFamily("Nintendo Switch Pro Controller"))
        assertEquals(AndroidControllerFamily.Generic, AndroidControllerInput.controllerFamily("8BitDo Gamepad"))
    }

    @Test
    fun advertisesPlayStationControllersWithoutTheXinputStyleBit() {
        assertEquals(
            0x0001,
            androidGamepadConnectionBitmap(
                controllerId = 0,
                connected = true,
                physicalControllerFamily = AndroidControllerFamily.PlayStation,
            ),
        )
        assertEquals(
            0x0202,
            androidGamepadConnectionBitmap(
                controllerId = 1,
                connected = true,
                physicalControllerFamily = AndroidControllerFamily.Xbox,
            ),
        )
        assertEquals(
            0x0404,
            androidGamepadConnectionBitmap(
                controllerId = 2,
                connected = true,
                physicalControllerFamily = null,
            ),
        )
        assertEquals(
            0,
            androidGamepadConnectionBitmap(
                controllerId = 0,
                connected = false,
                physicalControllerFamily = AndroidControllerFamily.PlayStation,
            ),
        )
    }

    @Test
    fun forcedControllerRumbleUsesTheXinputCompatiblePlayStationIdentity() {
        assertEquals(
            0x0101,
            androidGamepadConnectionBitmap(
                controllerId = 0,
                connected = true,
                physicalControllerFamily = AndroidControllerFamily.PlayStation,
                playStationRumbleCompatibility = true,
            ),
        )
    }

    @Test
    fun resolvesHatOnlyControllerMotionAsLeftStick() {
        val axes = AndroidGamepadAxisMapping.resolve(
            raw = AndroidGamepadRawAxes(hatX = -1f, hatY = 0.75f),
            available = AndroidGamepadAxisAvailability(
                x = false,
                y = false,
                z = false,
                rz = false,
                rx = false,
                ry = false,
                hatX = true,
                hatY = true,
            ),
        )

        assertEquals(-1f, axes.leftX, 0.0001f)
        assertEquals(0.75f, axes.leftY, 0.0001f)
        assertEquals("hat", axes.leftSource)
        assertTrue(axes.hatUsedAsLeftStick)
    }

    @Test
    fun keepsStandardControllerAxesOnExpectedSticks() {
        val axes = AndroidGamepadAxisMapping.resolve(
            AndroidGamepadRawAxes(x = 0.5f, y = -0.25f, z = 0.6f, rz = -0.7f, rx = -0.2f, ry = 0.1f),
        )

        assertEquals(0.5f, axes.leftX, 0.0001f)
        assertEquals(-0.25f, axes.leftY, 0.0001f)
        assertEquals(0.6f, axes.rightX, 0.0001f)
        assertEquals(-0.7f, axes.rightY, 0.0001f)
        assertEquals("x/y", axes.leftSource)
        assertEquals("z/rz", axes.rightSource)
        assertFalse(axes.hatUsedAsLeftStick)
    }

    @Test
    fun mapsControllerMouseAssistFromRightStick() {
        val delta = requireNotNull(AndroidControllerMouseAssist.mouseDelta(0.75f, -0.5f))

        assertTrue(delta.dx > 0)
        assertTrue(delta.dy < 0)
        assertNull(AndroidControllerMouseAssist.mouseDelta(0f, 0f))
    }

    @Test
    fun controllerMouseAssistDropsNonFiniteAxisValues() {
        assertNull(AndroidControllerMouseAssist.mouseDelta(Float.NaN, 0f))
        assertNull(AndroidControllerMouseAssist.mouseDelta(0f, Float.POSITIVE_INFINITY))
        assertEquals(Pair(0, 0f), AndroidControllerMouseAssist.scrollNotches(Float.NaN, 30, 0f))
        assertEquals(Pair(0, 0f), AndroidControllerMouseAssist.scrollNotches(1f, 30, Float.NaN))
    }

    @Test
    fun mapsControllerMouseClicksWithoutTakingOverOtherGameplayButtons() {
        assertNull(AndroidControllerMouseAssist.mouseButtonForGamepad(GamepadButtonMapping.RIGHT_THUMB))
        assertEquals(1, AndroidControllerMouseAssist.mouseButtonForGamepad(GamepadButtonMapping.A))
        assertEquals(3, AndroidControllerMouseAssist.mouseButtonForGamepad(GamepadButtonMapping.B))
        assertNull(AndroidControllerMouseAssist.mouseButtonForTrigger(left = true))
        assertNull(AndroidControllerMouseAssist.mouseButtonForTrigger(left = false))
    }

    @Test
    fun classifiesMemoryConstrainedTvWithoutDowngradingSameMemoryMobile() {
        val twoGiB = 2L * 1024L * 1024L * 1024L

        assertTrue(isLowPowerStreamingProfile(androidTvProfile = true, renderer = "amlogic", totalMemoryBytes = twoGiB))
        assertFalse(isLowPowerStreamingProfile(androidTvProfile = false, renderer = "adreno", totalMemoryBytes = twoGiB))
        assertFalse(isLowPowerStreamingProfile(androidTvProfile = true, renderer = "adreno", totalMemoryBytes = 4L * 1024L * 1024L * 1024L))
    }

    @Test
    fun classifies32BitPhonesAndMemoryConstrainedTvsAsConstrainedRuntimes() {
        val twoGiB = 2L * 1024L * 1024L * 1024L
        val fourGiB = 4L * 1024L * 1024L * 1024L

        assertTrue(isConstrainedStreamingRuntime(androidTvProfile = false, is64BitRuntime = false, totalMemoryBytes = fourGiB))
        assertTrue(isConstrainedStreamingRuntime(androidTvProfile = true, is64BitRuntime = true, totalMemoryBytes = twoGiB))
        assertFalse(isConstrainedStreamingRuntime(androidTvProfile = false, is64BitRuntime = true, totalMemoryBytes = twoGiB))
        assertTrue(
            isLowPowerStreamingProfile(
                androidTvProfile = false,
                renderer = "adreno",
                totalMemoryBytes = fourGiB,
                is64BitRuntime = false,
            ),
        )
    }

    @Test
    fun mapsControllerActivationKeysToPrimaryGamepadButtonOnlyForControllers() {
        assertEquals(
            GamepadButtonMapping.A,
            GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_DPAD_CENTER, controllerActivation = true),
        )
        assertEquals(
            GamepadButtonMapping.A,
            GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_ENTER, controllerActivation = true),
        )
        assertNull(GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_DPAD_CENTER))
        assertNull(GamepadButtonMapping.maskForKeyCode(KeyEvent.KEYCODE_ENTER))
    }

    @Test
    fun normalizesControllerAForNativeUiActivation() {
        assertEquals(
            KeyEvent.KEYCODE_DPAD_CENTER,
            NativeStreamInputRouter.normalizedAppUiKeyCode(KeyEvent.KEYCODE_BUTTON_A, streamUiActive = false),
        )
    }

    @Test
    fun consumesControllerBAsNativeUiBackNavigation() {
        assertTrue(
            NativeStreamInputRouter.isControllerAppBackKey(
                keyCode = KeyEvent.KEYCODE_BUTTON_B,
                controllerSource = false,
                streamUiActive = false,
            ),
        )
    }

    @Test
    fun streamControlsShortcutIsConfigurableWithoutTakingControllerButtons() {
        assertTrue(
            NativeStreamInputRouter.shouldOpenStreamSystemMenuKey(
                KeyEvent.KEYCODE_G,
                controllerInputDevice = false,
                ctrlPressed = true,
                shiftPressed = true,
            ),
        )
        assertFalse(
            NativeStreamInputRouter.shouldOpenStreamSystemMenuKey(
                KeyEvent.KEYCODE_MENU,
                controllerInputDevice = false,
                shiftPressed = true,
            ),
        )
        assertTrue(
            NativeStreamInputRouter.shouldOpenStreamSystemMenuKey(
                KeyEvent.KEYCODE_MENU,
                controllerInputDevice = false,
                configuredShortcut = "Shift+Menu",
                shiftPressed = true,
            ),
        )
        assertFalse(
            NativeStreamInputRouter.shouldOpenStreamSystemMenuKey(
                KeyEvent.KEYCODE_G,
                controllerInputDevice = true,
                ctrlPressed = true,
                shiftPressed = true,
            ),
        )
        assertFalse(
            NativeStreamInputRouter.shouldOpenStreamSystemMenuKey(
                KeyEvent.KEYCODE_BUTTON_START,
                controllerInputDevice = true,
            ),
        )
    }

    @Test
    fun reservesRemoteBackAliasesForStreamControlsWithoutTakingControllerButtons() {
        assertTrue(
            NativeStreamInputRouter.shouldHandleStreamExitKey(
                KeyEvent.KEYCODE_BACK,
                controllerInputDevice = false,
                hardwareKeyboardSource = false,
            ),
        )
        assertFalse(
            NativeStreamInputRouter.shouldHandleStreamExitKey(
                KeyEvent.KEYCODE_BACK,
                controllerInputDevice = true,
                hardwareKeyboardSource = false,
            ),
        )
        assertTrue(
            NativeStreamInputRouter.shouldHandleStreamExitKey(
                KeyEvent.KEYCODE_BUTTON_B,
                controllerInputDevice = false,
                hardwareKeyboardSource = false,
                androidTvProfile = true,
                dpadSource = true,
            ),
        )
        assertFalse(
            NativeStreamInputRouter.shouldHandleStreamExitKey(
                KeyEvent.KEYCODE_BUTTON_B,
                controllerInputDevice = true,
                hardwareKeyboardSource = false,
                androidTvProfile = true,
                dpadSource = true,
            ),
        )
        assertFalse(
            NativeStreamInputRouter.shouldHandleStreamExitKey(
                KeyEvent.KEYCODE_BUTTON_B,
                controllerInputDevice = false,
                hardwareKeyboardSource = false,
                androidTvProfile = false,
                dpadSource = true,
            ),
        )
        assertFalse(
            NativeStreamInputRouter.shouldHandleStreamExitKey(
                KeyEvent.KEYCODE_BUTTON_SELECT,
                controllerInputDevice = false,
                hardwareKeyboardSource = false,
            ),
        )
        assertFalse(
            NativeStreamInputRouter.shouldHandleStreamExitKey(
                KeyEvent.KEYCODE_BUTTON_B,
                controllerInputDevice = false,
                hardwareKeyboardSource = false,
            ),
        )
    }

    @Test
    fun opensOverlayWithGuideButtonOnAndroidTv() {
        assertTrue(
            NativeStreamInputRouter.shouldOpenStreamSystemMenuKey(
                KeyEvent.KEYCODE_BUTTON_MODE,
                controllerInputDevice = true,
                androidTvProfile = true,
            ),
        )
        assertFalse(
            NativeStreamInputRouter.shouldOpenStreamSystemMenuKey(
                KeyEvent.KEYCODE_BUTTON_MODE,
                controllerInputDevice = true,
            ),
        )
    }

    @Test
    fun controllerBackStaysInGameOnAndroidTvWhileRemoteBackOpensOverlay() {
        assertFalse(
            NativeStreamInputRouter.shouldHandleStreamExitKey(
                KeyEvent.KEYCODE_BACK,
                controllerInputDevice = true,
                hardwareKeyboardSource = false,
                androidTvProfile = true,
            ),
        )
        assertTrue(
            NativeStreamInputRouter.shouldHandleStreamExitKey(
                KeyEvent.KEYCODE_BACK,
                controllerInputDevice = false,
                hardwareKeyboardSource = false,
                androidTvProfile = true,
            ),
        )
        assertFalse(
            NativeStreamInputRouter.shouldHandleStreamExitKey(
                KeyEvent.KEYCODE_BACK,
                controllerInputDevice = true,
                hardwareKeyboardSource = false,
            ),
        )
    }

    @Test
    fun clampsStreamSharpnessShaderStrength() {
        assertEquals(0f, streamSharpnessShaderStrength(enabled = false, amount = 1f), 0.0001f)
        assertEquals(0f, streamSharpnessShaderStrength(enabled = true, amount = -1f), 0.0001f)
        assertEquals(0.28f, streamSharpnessShaderStrength(enabled = true, amount = 2f), 0.0001f)
        assertFalse(streamSharpnessShaderActive(streamSharpnessShaderStrength(enabled = false, amount = 1f)))
        assertFalse(streamSharpnessShaderActive(Float.NaN))
        assertTrue(streamSharpnessShaderActive(streamSharpnessShaderStrength(enabled = true, amount = 1f)))
    }

    @Test
    fun controllerMouseLoopOnlyRunsWhileAMouseModeIsActive() {
        assertFalse(shouldRunControllerMouseLoop(controllerMouseAssistActive = false, controllerMouseEmulationActive = false))
        assertTrue(shouldRunControllerMouseLoop(controllerMouseAssistActive = true, controllerMouseEmulationActive = false))
        assertTrue(shouldRunControllerMouseLoop(controllerMouseAssistActive = false, controllerMouseEmulationActive = true))
    }

    @Test
    fun mapsRightStickDeflectionToScrollNotches() {
        // Deadzone behavior
        assertEquals(Pair(0, 0f), AndroidControllerMouseAssist.scrollNotches(0.05f, 30, 0f))
        assertEquals(Pair(0, 0f), AndroidControllerMouseAssist.scrollNotches(-0.09f, 30, 0f))

        // Pushing UP (negative stickY) should scroll UP (positive notches)
        val (upNotches, _) = AndroidControllerMouseAssist.scrollNotches(-1.0f, 30, 0.9f)
        assertTrue(upNotches > 0)

        // Pushing DOWN (positive stickY) should scroll DOWN (negative notches)
        val (downNotches, _) = AndroidControllerMouseAssist.scrollNotches(1.0f, 30, -0.9f)
        assertTrue(downNotches < 0)

        // Sensitivity modulations
        // High sensitivity (low setting e.g. 10) should scroll faster (generate more/equal notches for the same deflection/accumulator)
        val (fastNotches, _) = AndroidControllerMouseAssist.scrollNotches(-1.0f, 10, 0.9f)
        val (slowNotches, _) = AndroidControllerMouseAssist.scrollNotches(-1.0f, 100, 0.9f)
        assertTrue(fastNotches >= slowNotches)
    }
}
