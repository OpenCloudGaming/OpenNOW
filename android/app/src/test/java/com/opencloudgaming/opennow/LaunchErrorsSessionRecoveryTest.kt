package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Test

class LaunchErrorsSessionRecoveryTest {
    @Test
    fun terminalSessionMessageDoesNotAssumeWhoStoppedIt() {
        val message = normalizeLaunchErrorMessage(
            TerminalSessionStatusException(status = 7, latestSession = null),
        )

        assertEquals(
            "The cloud session is no longer available (status 7). " +
                "Start the game again to open a new session.",
            message,
        )
    }
}
