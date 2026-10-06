package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class KishiUsbPermissionPromptTest {
    private val device = "/dev/bus/usb/001/002"

    @Test fun startupConfigurationCanFollowTheFirstResume() {
        val prompt = KishiUsbPermissionPrompt()
        assertFalse(prompt.request(device, manual = false))
        prompt.onForeground()
        assertTrue(prompt.request(device, manual = false))
        assertFalse(prompt.request(device, manual = false))
    }

    @Test fun denialDoesNotLoopOnResumeOrRepeatedScans() {
        val prompt = KishiUsbPermissionPrompt()
        prompt.onForeground()
        assertTrue(prompt.request(device, manual = false))
        assertTrue(prompt.complete(device))
        prompt.onForeground()
        assertFalse(prompt.request(device, manual = false))
    }

    @Test fun nextForegroundVisitChecksAgainAfterDenial() {
        val prompt = KishiUsbPermissionPrompt()
        prompt.onForeground()
        prompt.request(device, manual = false)
        prompt.complete(device)
        prompt.onBackground()
        assertFalse(prompt.request(device, manual = false))
        prompt.onForeground()
        assertTrue(prompt.request(device, manual = false))
    }

    @Test fun systemPermissionActivityIsNotANewForegroundVisit() {
        val prompt = KishiUsbPermissionPrompt()
        prompt.onForeground()
        prompt.request(device, manual = false)
        prompt.onBackground()
        prompt.complete(device)
        prompt.onForeground()
        assertFalse(prompt.request(device, manual = false))
    }

    @Test fun onlyOneSystemRequestMayBePending() {
        val prompt = KishiUsbPermissionPrompt()
        prompt.onForeground()
        assertTrue(prompt.request(device, manual = false))
        assertFalse(prompt.request(device, manual = true))
        assertFalse(prompt.request("other", manual = true))
        assertFalse(prompt.complete("other"))
        assertFalse(prompt.request(device, manual = true))
        assertTrue(prompt.complete(device))
    }

    @Test fun explicitRetryRemainsAvailableAfterDenial() {
        val prompt = KishiUsbPermissionPrompt()
        prompt.onForeground()
        prompt.request(device, manual = false)
        prompt.complete(device)
        assertFalse(prompt.request(device, manual = false))
        assertTrue(prompt.request(device, manual = true))
    }

    @Test fun unplugReplugCanPromptWithoutRestartingTheApp() {
        val prompt = KishiUsbPermissionPrompt()
        prompt.onForeground()
        prompt.request(device, manual = false)
        prompt.retainDevices(emptySet())
        prompt.retainDevices(setOf(device))
        assertTrue(prompt.request(device, manual = false))
    }

    @Test fun stalePermissionResultCannotClearAReplacementRequest() {
        val prompt = KishiUsbPermissionPrompt()
        prompt.onForeground()
        prompt.request(device, manual = false)
        prompt.retainDevices(setOf("replacement"))
        assertTrue(prompt.request("replacement", manual = false))
        assertFalse(prompt.complete(device))
        assertFalse(prompt.request("replacement", manual = true))
    }

    @Test fun backgroundAttachmentWaitsUntilTheAppOpens() {
        val prompt = KishiUsbPermissionPrompt()
        prompt.retainDevices(setOf(device))
        assertFalse(prompt.request(device, manual = false))
        prompt.onForeground()
        assertTrue(prompt.request(device, manual = false))
    }

    @Test fun disablingAndReenablingDoesNotKeepAnObsoleteRequest() {
        val prompt = KishiUsbPermissionPrompt()
        prompt.onForeground()
        prompt.request(device, manual = false)
        prompt.reset()
        assertTrue(prompt.request(device, manual = false))
    }
}
