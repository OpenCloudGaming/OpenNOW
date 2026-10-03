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
    fun rigProvisioningStepHidesStaleQueuePositionAndShowsSetup() {
        val session = session(queuePosition = 0, seatSetupStep = 3)
        val state = OpenNowUiState(
            streamStatus = "queue",
            launchPhase = "Checking queue",
            queuePosition = 1,
            streamSession = session,
        )

        assertNull(queueDisplayPosition(session))
        assertNull(queueDisplayPosition(state))
        assertEquals(QueueLaunchStatusKind.SettingUpRig, queueLaunchStatus(state).kind)
        assertEquals("Setting up rig", queueLaunchStatusText(state))
    }

    @Test
    fun positiveQueuePositionKeepsQueueVisibleIfSetupFieldsDisagree() {
        val session = session(queuePosition = 6, seatSetupStep = 3)
        val state = OpenNowUiState(
            streamStatus = "queue",
            launchPhase = "Queue",
            streamSession = session,
        )

        assertEquals(6, queueDisplayPosition(state))
        assertEquals("Queue position 6", queueLaunchStatusText(state))
    }

    @Test
    fun queueDisplayOnlyMovesLowerForTheSameSession() {
        var state = OpenNowUiState(
            streamStatus = "queue",
            streamSession = session(queuePosition = 25, seatSetupStep = 1),
            queuePosition = 25,
        )
        val reported = listOf(37, 17, 37, 8, 37, 4)
        val displayed = reported.map { position ->
            val updated = session(queuePosition = position, seatSetupStep = 1)
            state = state.copy(
                streamSession = updated,
                queuePosition = stableQueueDisplayPosition(state, updated),
            )
            queueDisplayPosition(state)
        }

        assertEquals(listOf(25, 17, 17, 8, 8, 4), displayed)
        val setup = session(queuePosition = 0, seatSetupStep = 3)
        assertNull(stableQueueDisplayPosition(state, setup))
        assertEquals(
            37,
            stableQueueDisplayPosition(state, session(sessionId = "new-session", queuePosition = 37, seatSetupStep = 1)),
        )
    }

    @Test
    fun rigProvisioningBecomesConnectingWithoutReturningToQueue() {
        val state = OpenNowUiState(
            streamStatus = "connecting",
            launchPhase = "Connecting stream",
            queuePosition = 1,
            streamSession = session(queuePosition = 0, seatSetupStep = 3, status = 2),
        )

        assertNull(queueDisplayPosition(state))
        assertEquals(QueueLaunchStatusKind.ConnectingStream, queueLaunchStatus(state).kind)
    }

    @Test
    fun readyStatusOverridesStaleQueuePositionOne() {
        val session = session(queuePosition = 1, seatSetupStep = 4, status = 3)
        val state = OpenNowUiState(
            streamStatus = "queue",
            launchPhase = "Setting up rig",
            queuePosition = 1,
            streamSession = session,
        )

        assertNull(queueDisplayPosition(session))
        assertNull(queueDisplayPosition(state))
        assertEquals("Connecting...", queueLaunchStatusText(state))
    }

    @Test
    fun connectingPhaseHidesStaleQueuePositionEvenBeforeSessionRefresh() {
        val state = OpenNowUiState(
            streamStatus = "connecting",
            launchPhase = "Connecting stream",
            queuePosition = 1,
            streamSession = session(queuePosition = 1, seatSetupStep = 4),
        )

        assertNull(queueDisplayPosition(state))
        assertEquals("Connecting...", queueLaunchStatusText(state))
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
        status: Int = 1,
    ): SessionInfo =
        SessionInfo(
            sessionId = sessionId,
            status = status,
            queuePosition = queuePosition,
            seatSetupStep = seatSetupStep,
            serverIp = "server",
            signalingServer = "server:443",
            signalingUrl = "wss://server:443/nvst/",
        )
}
