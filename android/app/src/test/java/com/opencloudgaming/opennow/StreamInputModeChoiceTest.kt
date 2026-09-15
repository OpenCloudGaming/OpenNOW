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
    fun attachedMouseWaitsForEitherExplicitChoiceBeforeProvisioning() = runBlocking {
        for (choice in StreamInputMode.entries) {
            val answer = CompletableDeferred<StreamInputMode>()
            var provisioned: StreamInputMode? = null
            val launch = async(start = CoroutineStart.UNDISPATCHED) {
                chooseStreamInputModeAtStart(true, true) { answer.await() }
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
            chooseStreamInputModeAtStart(true, true) { answer.await() }
            provisioned = true
        }
        launch.cancelAndJoin()
        answer.complete(StreamInputMode.NativeTouch)
        assertTrue(launch.isCancelled)
        assertFalse(provisioned)
    }

    @Test
    fun disconnectedMouseAndUnavailableTouchDoNotPrompt() = runBlocking {
        for (touch in listOf(false, true)) {
            for (mouse in listOf(false, true)) {
                if (touch && mouse) continue
                assertEquals(
                    if (touch) StreamInputMode.NativeTouch else StreamInputMode.KeyboardMouse,
                    chooseStreamInputModeAtStart(touch, mouse) { error("Unexpected prompt") },
                )
            }
        }
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
}
