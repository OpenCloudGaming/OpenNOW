package com.opencloudgaming.opennow

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class StreamInputModeChoiceTest {
    @Test
    fun savedKeyboardOverlayDoesNotSuppressAProvisionedNativeTouchSession() {
        assertFalse(keyboardOverlayEnabledForStream(true, StreamInputMode.NativeTouch))
        assertTrue(keyboardOverlayEnabledForStream(true, StreamInputMode.KeyboardMouse))
        assertFalse(keyboardOverlayEnabledForStream(false, StreamInputMode.KeyboardMouse))
    }

    @Test
    fun touchCapableGameWaitsForEitherExplicitChoiceBeforeProvisioning() = runBlocking {
        for (choice in StreamInputMode.entries) {
            val answer = CompletableDeferred<StreamInputMode>()
            var provisioned: StreamInputMode? = null
            val launch = async(start = CoroutineStart.UNDISPATCHED) {
                chooseStreamInputModeAtStart(true, false, promptForChoice = true) { answer.await() }
                    .also { provisioned = it }
            }
            assertFalse(launch.isCompleted)
            assertNull(provisioned)
            answer.complete(choice)
            assertEquals(choice, launch.await())
            assertEquals(choice, provisioned)
        }
    }

    @Test
    fun cancellationWhileChoosingDoesNotProvisionAHost() = runBlocking {
        val answer = CompletableDeferred<StreamInputMode>()
        var provisioned = false
        val launch = async(start = CoroutineStart.UNDISPATCHED) {
            chooseStreamInputModeAtStart(true, false, promptForChoice = true) { answer.await() }
            provisioned = true
        }
        launch.cancelAndJoin()
        answer.complete(StreamInputMode.NativeTouch)
        assertTrue(launch.isCancelled)
        assertFalse(provisioned)
    }

    @Test
    fun unavailableTouchDoesNotPrompt() = runBlocking {
        for (touch in listOf(false, true)) {
            for (mouse in listOf(false, true)) {
                val prompt = shouldPromptForLaunchInputMode(
                    nativeTouchAvailable = touch,
                    catalogTouchSupported = false,
                    keyboardMouseConnected = mouse,
                )
                if (prompt) continue
                assertEquals(
                    if (touch) StreamInputMode.NativeTouch else StreamInputMode.KeyboardMouse,
                    chooseStreamInputModeAtStart(touch, mouse, promptForChoice = prompt) {
                        error("Unexpected prompt")
                    },
                )
            }
        }
    }

    @Test
    fun catalogTouchGamePromptsWithoutPhysicalInput() {
        assertTrue(shouldPromptForLaunchInputMode(true, true, false))
        assertTrue(shouldPromptForLaunchInputMode(true, true, true))
        assertTrue(shouldPromptForLaunchInputMode(true, false, true))
        assertFalse(shouldPromptForLaunchInputMode(true, false, false))
        assertFalse(shouldPromptForLaunchInputMode(false, true, true))
    }

    @Test
    fun legacySessionFallbackUsesTheAttachedDevice() {
        assertEquals(
            StreamInputMode.KeyboardMouse,
            streamInputModeAtStart(nativeTouchAvailable = true, keyboardMouseConnected = true),
        )
        assertEquals(
            StreamInputMode.NativeTouch,
            streamInputModeAtStart(nativeTouchAvailable = true, keyboardMouseConnected = false),
        )
    }

    @Test
    fun hotPlugAsksBeforeLeavingNativeTouch() {
        assertEquals(
            StreamInputModePrompt.SwitchToKeyboardMouse,
            streamInputModePromptForConnectionChange(
                currentMode = StreamInputMode.NativeTouch,
                keyboardMouseConnected = true,
                nativeTouchProvisionedForSession = true,
            ),
        )
    }

    @Test
    fun disconnectAsksBeforeReturningToProvisionedNativeTouch() {
        assertEquals(
            StreamInputModePrompt.SwitchToNativeTouch,
            streamInputModePromptForConnectionChange(
                currentMode = StreamInputMode.KeyboardMouse,
                keyboardMouseConnected = false,
                nativeTouchProvisionedForSession = true,
            ),
        )
        assertNull(
            streamInputModePromptForConnectionChange(
                currentMode = StreamInputMode.KeyboardMouse,
                keyboardMouseConnected = false,
                nativeTouchProvisionedForSession = false,
            ),
        )
    }

    @Test
    fun unchangedModeNeedsNoPrompt() {
        assertNull(
            streamInputModePromptForConnectionChange(
                currentMode = StreamInputMode.NativeTouch,
                keyboardMouseConnected = false,
                nativeTouchProvisionedForSession = true,
            ),
        )
        assertNull(
            streamInputModePromptForConnectionChange(
                currentMode = StreamInputMode.KeyboardMouse,
                keyboardMouseConnected = true,
                nativeTouchProvisionedForSession = true,
            ),
        )
    }

    @Test
    fun noisyDeviceChangesPresentEachPromptAtMostOncePerSession() {
        val gate = StreamInputModePromptGate()

        assertTrue(gate.shouldPresent(StreamInputModePrompt.SwitchToKeyboardMouse))
        assertFalse(gate.shouldPresent(StreamInputModePrompt.SwitchToKeyboardMouse))
        assertTrue(gate.shouldPresent(StreamInputModePrompt.SwitchToNativeTouch))
        assertFalse(gate.shouldPresent(StreamInputModePrompt.SwitchToNativeTouch))

        // A new stream gets a new gate and can ask again.
        assertTrue(StreamInputModePromptGate().shouldPresent(StreamInputModePrompt.SwitchToKeyboardMouse))
    }
}
