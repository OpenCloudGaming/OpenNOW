package com.opencloudgaming.opennow

import android.view.MotionEvent
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ExternalMousePointerCaptureTest {
    @Test
    fun enablesCaptureOnlyDuringUnobstructedGameplayWhenMouseLockIsOn() {
        assertTrue(
            shouldEnableExternalMousePointerCapture(
                streamReady = true,
                streamOverlayOpen = false,
                pointerLockEnabled = true,
            ),
        )
        assertFalse(
            shouldEnableExternalMousePointerCapture(
                streamReady = true,
                streamOverlayOpen = true,
                pointerLockEnabled = true,
            ),
        )
        assertFalse(
            shouldEnableExternalMousePointerCapture(
                streamReady = true,
                streamOverlayOpen = false,
                pointerLockEnabled = false,
            ),
        )
    }

    @Test
    fun retriesCaptureForMouseMotionWhenStreamingWithoutAnOverlay() {
        assertTrue(
            shouldRequestAndroidMousePointerCapture(
                streamActive = true,
                captureEnabled = true,
                windowFocused = true,
                hasPointerCapture = false,
                mouseLikePointer = true,
            ),
        )
    }

    @Test
    fun doesNotStealCaptureFromOverlaysOrOtherPointerSources() {
        assertFalse(
            shouldRequestAndroidMousePointerCapture(
                streamActive = true,
                captureEnabled = false,
                windowFocused = true,
                hasPointerCapture = false,
                mouseLikePointer = true,
            ),
        )
        assertFalse(
            shouldRequestAndroidMousePointerCapture(
                streamActive = true,
                captureEnabled = true,
                windowFocused = true,
                hasPointerCapture = false,
                mouseLikePointer = false,
            ),
        )
        assertFalse(
            shouldRequestAndroidMousePointerCapture(
                streamActive = true,
                captureEnabled = true,
                windowFocused = true,
                hasPointerCapture = true,
                mouseLikePointer = true,
            ),
        )
    }

    @Test
    fun routesOnlyCapturedMouseEventsWhileStreaming() {
        assertTrue(
            shouldRouteCapturedAndroidMousePointer(
                streamActive = true,
                mouseLikePointer = true,
            ),
        )
        assertFalse(
            shouldRouteCapturedAndroidMousePointer(
                streamActive = false,
                mouseLikePointer = true,
            ),
        )
        assertFalse(
            shouldRouteCapturedAndroidMousePointer(
                streamActive = true,
                mouseLikePointer = false,
            ),
        )
    }

    @Test
    fun capturedMousePrefersExplicitRelativeAxesAndFallsBackWhenTheyAreEmpty() {
        assertTrue(shouldUseAndroidRelativeMouseAxes(relativeDx = 4f, relativeDy = 0f))
        assertTrue(shouldUseAndroidRelativeMouseAxes(relativeDx = 0f, relativeDy = -3f))
        assertFalse(shouldUseAndroidRelativeMouseAxes(relativeDx = 0f, relativeDy = 0f))
    }

    @Test
    fun capturedMouseKeepsMatchingRelativeAxesAndRepairsOpposingOnes() {
        assertEquals(4f, resolveAndroidCapturedMouseAxis(relativeAxis = 4f, capturedAxis = 3f), 0f)
        assertEquals(-4f, resolveAndroidCapturedMouseAxis(relativeAxis = -4f, capturedAxis = -3f), 0f)
        assertEquals(3f, resolveAndroidCapturedMouseAxis(relativeAxis = -4f, capturedAxis = 3f), 0f)
        assertEquals(-3f, resolveAndroidCapturedMouseAxis(relativeAxis = 4f, capturedAxis = -3f), 0f)
        assertEquals(3f, resolveAndroidCapturedMouseAxis(relativeAxis = 0f, capturedAxis = 3f), 0f)
    }

    @Test
    fun detectsEitherCapturedMouseAxisDirectionConflict() {
        assertTrue(
            androidCapturedMouseAxesConflict(
                relativeDx = -4f,
                relativeDy = 2f,
                capturedDx = 3f,
                capturedDy = 2f,
            ),
        )
        assertFalse(
            androidCapturedMouseAxesConflict(
                relativeDx = 4f,
                relativeDy = -2f,
                capturedDx = 3f,
                capturedDy = -2f,
            ),
        )
    }

    @Test
    fun keepsCapturedAndExplicitRelativeMouseMotionUnbounded() {
        assertTrue(
            shouldSendExternalMouseAsRelative(
                capturedPointer = true,
                hasRelativeAxisMotion = false,
            ),
        )
        assertTrue(
            shouldSendExternalMouseAsRelative(
                capturedPointer = false,
                hasRelativeAxisMotion = true,
            ),
        )
        assertFalse(
            shouldSendExternalMouseAsRelative(
                capturedPointer = false,
                hasRelativeAxisMotion = false,
            ),
        )
    }

    @Test
    fun pointerCaptureCancellationDoesNotReleaseAHeldMouseButton() {
        assertFalse(shouldReleaseExternalMouseButton(MotionEvent.ACTION_CANCEL))
        assertTrue(shouldReleaseExternalMouseButton(MotionEvent.ACTION_UP))
        assertTrue(shouldReleaseExternalMouseButton(MotionEvent.ACTION_BUTTON_RELEASE))
    }
}
