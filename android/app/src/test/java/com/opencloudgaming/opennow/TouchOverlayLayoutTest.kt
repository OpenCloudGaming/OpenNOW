package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TouchOverlayLayoutTest {
    @Test
    fun keepsLandscapeTopControlsBelowPhoneTopInformationBand() {
        val clearance = landscapeTouchTopControlClearanceDp(viewportHeightDp = 390f, controlScale = 1f)

        assertTrue(clearance >= 40f)
    }

    @Test
    fun scalesLandscapeTopClearanceForLargerControlsWithoutRunningAway() {
        val normal = landscapeTouchTopControlClearanceDp(viewportHeightDp = 430f, controlScale = 1f)
        val large = landscapeTouchTopControlClearanceDp(viewportHeightDp = 800f, controlScale = 1.5f)

        assertTrue(large > normal)
        assertEquals(76f, large, 0.001f)
    }

    @Test
    fun keepsTinyLandscapeScreensUsable() {
        val clearance = landscapeTouchTopControlClearanceDp(viewportHeightDp = 300f, controlScale = 0.6f)

        assertEquals(30f, clearance, 0.001f)
    }

    @Test
    fun touchJoystickDeadZoneKeepsCenterStableAndPreservesFullRange() {
        assertEquals(0f, applyTouchJoystickDeadZone(0.05f, 0.08f), 0.0001f)
        assertEquals(0f, applyTouchJoystickDeadZone(-0.05f, 0.08f), 0.0001f)
        assertEquals(1f, applyTouchJoystickDeadZone(1f, 0.08f), 0.0001f)
        assertEquals(-1f, applyTouchJoystickDeadZone(-1f, 0.08f), 0.0001f)
    }

    @Test
    fun touchJoystickDeadZoneRescalesInputBeyondCenter() {
        assertEquals(0.5f, applyTouchJoystickDeadZone(0.54f, 0.08f), 0.0001f)
        assertEquals(-0.5f, applyTouchJoystickDeadZone(-0.54f, 0.08f), 0.0001f)
    }

    @Test
    fun touchAimZoneMapsFingerTravelToRightStickRange() {
        val halfTravel = touchStickValue(deltaX = 36f, deltaY = -36f, maxTravel = 72f, deadZone = 0f)
        val beyondZone = touchStickValue(deltaX = 144f, deltaY = 0f, maxTravel = 72f, deadZone = 0f)

        assertEquals(0.5f, halfTravel.x, 0.0001f)
        assertEquals(-0.5f, halfTravel.y, 0.0001f)
        assertEquals(1f, beyondZone.x, 0.0001f)
        assertEquals(0f, beyondZone.y, 0.0001f)
    }

    @Test
    fun touchAimZoneSensitivityChangesRequiredFingerTravel() {
        val moreSensitive = touchStickValue(
            deltaX = 36f,
            deltaY = 0f,
            maxTravel = 72f,
            deadZone = 0f,
            sensitivity = 2f,
        )
        val lessSensitive = touchStickValue(
            deltaX = 36f,
            deltaY = 0f,
            maxTravel = 72f,
            deadZone = 0f,
            sensitivity = 0.5f,
        )

        assertEquals(1f, moreSensitive.x, 0.0001f)
        assertEquals(0.25f, lessSensitive.x, 0.0001f)
    }

    @Test
    fun touchAimZoneScaleChangesAndBoundsTheInteractiveFootprint() {
        assertEquals(0.36f, scaledAimZoneFraction(0.48f, 0.75f), 0.0001f)
        assertEquals(0.72f, scaledAimZoneFraction(0.48f, 1.5f), 0.0001f)
        assertEquals(1f, scaledAimZoneFraction(0.72f, 1.5f), 0.0001f)
    }

    @Test
    fun touchAimZoneUsesTheConfiguredDeadZoneAndRejectsInvalidGeometry() {
        val insideDeadZone = touchStickValue(deltaX = 4f, deltaY = 0f, maxTravel = 72f, deadZone = 0.08f)
        val invalid = touchStickValue(deltaX = 20f, deltaY = 0f, maxTravel = 0f, deadZone = 0f)

        assertEquals(0f, insideDeadZone.x, 0.0001f)
        assertEquals(0f, insideDeadZone.y, 0.0001f)
        assertEquals(0f, invalid.x, 0.0001f)
        assertEquals(0f, invalid.y, 0.0001f)
    }

    @Test
    fun builtInControlVisibilityIsIndependent() {
        val customized = AndroidTouchSettings()
            .withControlVisible(TouchControlGroup.Dpad, false)
            .withControlVisible(TouchControlGroup.RightStick, false)

        assertTrue(customized.isControlVisible(TouchControlGroup.FaceButtons))
        assertEquals(false, customized.isControlVisible(TouchControlGroup.Dpad))
        assertEquals(false, customized.isControlVisible(TouchControlGroup.RightStick))
    }

    @Test
    fun programmableSlotsAreFixedWidthAndCycleThroughOff() {
        val customized = AndroidTouchSettings()
            .withExtraButtonAction(7, TouchExtraButtonAction.RightBumperAndRightTrigger)

        assertEquals(TouchExtraButtonAction.RightBumperAndRightTrigger, customized.extraButtonAction(7))
        assertEquals(TouchExtraButtonAction.None, nextTouchExtraButtonAction(TouchExtraButtonAction.LeftAndRightTriggers))
        assertEquals(
            TouchExtraButtonAction.RightBumperAndRightTrigger,
            customized.withExtraButtonAction(9, TouchExtraButtonAction.A).extraButtonAction(7),
        )
    }

    @Test
    fun rightBumperAndTriggerComboDispatchesBothControls() {
        val binding = touchExtraButtonBinding(TouchExtraButtonAction.RightBumperAndRightTrigger)

        assertEquals(listOf(GamepadButtonMapping.RIGHT_SHOULDER), binding.buttonMasks)
        assertEquals(false, binding.leftTrigger)
        assertTrue(binding.rightTrigger)
    }

    @Test
    fun racingSteeringAndKeyboardBindingsStayExplicit() {
        assertEquals(-1f, touchExtraButtonBinding(TouchExtraButtonAction.LeftStickLeft).leftStickX)
        assertEquals(1f, touchExtraButtonBinding(TouchExtraButtonAction.LeftStickRight).leftStickX)
        assertEquals(android.view.KeyEvent.KEYCODE_A, touchExtraButtonBinding(TouchExtraButtonAction.KeyboardA).keyboardKeyCode)
        assertEquals(android.view.KeyEvent.KEYCODE_D, touchExtraButtonBinding(TouchExtraButtonAction.KeyboardD).keyboardKeyCode)
    }

    @Test
    fun freelyConfiguredComboDispatchesEverySelectedInputType() {
        val actions = listOf(
            TouchExtraButtonAction.A,
            TouchExtraButtonAction.RightBumper,
            TouchExtraButtonAction.LeftTrigger,
            TouchExtraButtonAction.LeftStickLeft,
            TouchExtraButtonAction.KeyboardD,
        )
        val settings = AndroidTouchSettings().withExtraButtonCombo(2, actions)
        val binding = touchExtraButtonBinding(settings.extraButtonCombo(2))

        assertEquals(actions, settings.extraButtonCombo(2))
        assertEquals(listOf(GamepadButtonMapping.A, GamepadButtonMapping.RIGHT_SHOULDER), binding.buttonMasks)
        assertTrue(binding.leftTrigger)
        assertEquals(false, binding.rightTrigger)
        assertEquals(-1f, binding.leftStickX)
        assertEquals(listOf(android.view.KeyEvent.KEYCODE_D), binding.keyboardKeyCodes)
    }

    @Test
    fun quickAssignmentReplacesCustomComboAndLegacyCombosRemainCompatible() {
        val custom = AndroidTouchSettings().withExtraButtonCombo(
            0,
            listOf(TouchExtraButtonAction.A, TouchExtraButtonAction.RightTrigger),
        )
        val quick = custom.withExtraButtonAction(0, TouchExtraButtonAction.LeftBumperAndLeftTrigger)

        assertTrue("1" in custom.extraButtonCombos)
        assertFalse("1" in quick.extraButtonCombos)
        assertEquals(
            listOf(TouchExtraButtonAction.LeftBumper, TouchExtraButtonAction.LeftTrigger),
            quick.extraButtonCombo(0),
        )
    }

    @Test
    fun customShoulderShapeShrinksLayoutAndHitboxToVisibleCap() {
        val circle = touchShoulderFaceSize(72f, 32f, TouchButtonShape.Circle)
        val trigger = touchShoulderFaceSize(72f, 32f, TouchButtonShape.Trigger)

        assertEquals(32f, circle.width, 0.001f)
        assertEquals(32f, circle.height, 0.001f)
        assertEquals(72f, trigger.width, 0.001f)
        assertEquals(32f, trigger.height, 0.001f)
    }
}
