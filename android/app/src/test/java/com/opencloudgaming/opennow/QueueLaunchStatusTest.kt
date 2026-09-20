package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class QueueLaunchStatusTest {
    @Test
    fun seatSetupStepFiveQueuePositionIsNotDisplayed() {
        val session = session(queuePosition = 1, seatSetupStep = 5)
        val state = OpenNowUiState(
            streamStatus = "queue",
            launchPhase = "Queue",
            queuePosition = 1,
            streamSession = session,
        )

        assertNull(queueDisplayPosition(session))
        assertNull(queueDisplayPosition(state))
        assertEquals("Connecting...", queueLaunchStatusText(state))
    }

    @Test
    fun seatSetupStepOneQueuePositionIsDisplayed() {
        val session = session(queuePosition = 40, seatSetupStep = 1)
        val state = OpenNowUiState(
            streamStatus = "queue",
            streamSession = session,
        )

        assertEquals(40, queueDisplayPosition(session))
        assertEquals(40, queueDisplayPosition(state))
        assertEquals("Queue position 40", queueLaunchStatusText(state))
    }

    @Test
    fun queueReadyNotificationFiresOnceWhenObservedQueueStartsConnecting() {
        val tracker = QueueReadyNotificationTracker()
        val queuedSession = session(queuePosition = 12, seatSetupStep = 1)
        val connectingSession = session(queuePosition = null, seatSetupStep = 5)

        assertFalse(
            tracker.update(
                OpenNowUiState(
                    streamStatus = "queue",
                    launchPhase = "Queue",
                    queuePosition = 12,
                    streamSession = queuedSession,
                ),
            ),
        )
        assertTrue(
            tracker.update(
                OpenNowUiState(
                    streamStatus = "connecting",
                    launchPhase = "Connecting stream",
                    streamSession = connectingSession,
                ),
            ),
        )
        assertFalse(
            tracker.update(
                OpenNowUiState(
                    streamStatus = "connecting",
                    launchPhase = "Connecting stream",
                    streamSession = connectingSession,
                ),
            ),
        )
    }

    @Test
    fun queueReadyNotificationDoesNotFireForLaunchWithoutObservedQueue() {
        val tracker = QueueReadyNotificationTracker()
        val launchSession = session(queuePosition = null, seatSetupStep = 5)

        assertFalse(
            tracker.update(
                OpenNowUiState(
                    streamStatus = "queue",
                    launchPhase = "Creating session",
                    streamSession = launchSession,
                ),
            ),
        )
        assertFalse(
            tracker.update(
                OpenNowUiState(
                    streamStatus = "connecting",
                    launchPhase = "Connecting stream",
                    streamSession = launchSession,
                ),
            ),
        )
    }

    @Test
    fun queueReadyNotificationDoesNotLeakAcrossSessionsOrCancelledLaunches() {
        val cancelledTracker = QueueReadyNotificationTracker()

        assertFalse(
            cancelledTracker.update(
                OpenNowUiState(
                    streamStatus = "queue",
                    launchPhase = "Queue",
                    streamSession = session(sessionId = "queued", queuePosition = 4, seatSetupStep = 1),
                ),
            ),
        )
        assertFalse(cancelledTracker.update(OpenNowUiState(streamStatus = "idle")))
        assertFalse(
            cancelledTracker.update(
                OpenNowUiState(
                    streamStatus = "connecting",
                    launchPhase = "Connecting stream",
                    streamSession = session(sessionId = "different", queuePosition = null, seatSetupStep = 5),
                ),
            ),
        )

        val replacedSessionTracker = QueueReadyNotificationTracker()
        assertFalse(
            replacedSessionTracker.update(
                OpenNowUiState(
                    streamStatus = "queue",
                    launchPhase = "Queue",
                    streamSession = session(sessionId = "queued", queuePosition = 4, seatSetupStep = 1),
                ),
            ),
        )
        assertFalse(
            replacedSessionTracker.update(
                OpenNowUiState(
                    streamStatus = "connecting",
                    launchPhase = "Connecting stream",
                    streamSession = session(sessionId = "different", queuePosition = null, seatSetupStep = 5),
                ),
            ),
        )
    }

    private fun session(
        sessionId: String = "session",
        queuePosition: Int?,
        seatSetupStep: Int?,
    ): SessionInfo =
        SessionInfo(
            sessionId = sessionId,
            status = 1,
            queuePosition = queuePosition,
            seatSetupStep = seatSetupStep,
            serverIp = "server",
            signalingServer = "server:443",
            signalingUrl = "wss://server:443/nvst/",
        )
}
