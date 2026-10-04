package com.opencloudgaming.opennow

import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * `appLaunchMode` decides which virtual input devices the host builds for a session, and it is read
 * once at creation and never revisited. Sending the wrong value is invisible at every layer we can
 * see — the packets still encode, still send, still look right on the wire — and shows up only as a
 * game that ignores your fingers. That is why it is pinned here rather than left to a device test.
 */
class AppLaunchModeTest {

    private fun launchModeOf(body: kotlinx.serialization.json.JsonObject): Int =
        body.getValue("sessionRequestData").jsonObject
            .getValue("appLaunchMode").jsonPrimitive.int

    private fun controllerBitmapOf(body: kotlinx.serialization.json.JsonObject): Int =
        body.getValue("sessionRequestData").jsonObject
            .getValue("remoteControllersBitmap").jsonPrimitive.int

    private fun supportedControllerTypesOf(body: kotlinx.serialization.json.JsonObject): List<Int> =
        body.getValue("sessionRequestData").jsonObject
            .getValue("availableSupportedControllers").jsonArray
            .map { it.jsonPrimitive.int }

    @Test
    fun sessionsDefaultToGamepadFriendly() {
        val body = buildMinimalClaimRequestBody(appId = "123", deviceId = "device")
        assertEquals(GfnAppLaunchMode.GAMEPAD_FRIENDLY, launchModeOf(body))
        assertEquals(1, controllerBitmapOf(body))
        assertEquals(listOf(2), supportedControllerTypesOf(body))
    }

    /** The one value that makes the host present a digitizer. Without it native touch is inert. */
    @Test
    fun aTouchSessionAsksForTouchFriendly() {
        val body = buildMinimalClaimRequestBody(
            appId = "123",
            deviceId = "device",
            appLaunchMode = GfnAppLaunchMode.TOUCH_FRIENDLY,
        )
        assertEquals(GfnAppLaunchMode.TOUCH_FRIENDLY, launchModeOf(body))
        assertEquals(0, controllerBitmapOf(body))
        assertEquals(emptyList<Int>(), supportedControllerTypesOf(body))
    }

    @Test
    fun multipleControllersAreAdvertisedOnlyWhenEnabledForGamepadSessions() {
        val settings = StreamSettings(multiControllerEnabled = true)
        val gamepad = buildMinimalClaimRequestBody(appId = "123", deviceId = "device", settings = settings)
        val touch = buildMinimalClaimRequestBody(
            appId = "123",
            deviceId = "device",
            settings = settings,
            appLaunchMode = GfnAppLaunchMode.TOUCH_FRIENDLY,
        )
        assertEquals(15, controllerBitmapOf(gamepad))
        assertEquals(listOf(2), supportedControllerTypesOf(gamepad))
        assertEquals(0, controllerBitmapOf(touch))
    }

    /**
     * These are protocol constants, not ours to renumber — the host and the official client both
     * read them by value.
     */
    @Test
    fun theProtocolValuesAreWhatTheServerExpects() {
        assertEquals(1, GfnAppLaunchMode.DEFAULT)
        assertEquals(2, GfnAppLaunchMode.GAMEPAD_FRIENDLY)
        assertEquals(3, GfnAppLaunchMode.TOUCH_FRIENDLY)
    }
}
