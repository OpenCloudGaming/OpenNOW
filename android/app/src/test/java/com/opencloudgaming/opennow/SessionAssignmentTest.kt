package com.opencloudgaming.opennow

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class SessionAssignmentTest {
    @Test
    fun `uses standard zone from assigned session control host`() {
        assertEquals(
            "NP-WAW-01",
            assignedSessionZoneFromControlHost("np-waw-01.cloudmatchbeta.nvidiagrid.net"),
        )
    }

    @Test
    fun `uses alliance zone from assigned session control host`() {
        assertEquals(
            "NPA-TKC-IST-01",
            assignedSessionZoneFromControlHost("npa-tkc-ist-01.tkc.geforcenow.nvidiagrid.net"),
        )
    }

    @Test
    fun `ignores transport and untrusted hosts`() {
        assertNull(assignedSessionZoneFromControlHost("85-29-33-38.tkc.geforcenow.nvidiagrid.net"))
        assertNull(assignedSessionZoneFromControlHost("np-waw-01.example.com"))
        assertNull(assignedSessionZoneFromControlHost(null))
    }

    @Test
    fun `queue uses the control host from each session snapshot`() {
        val controlBase = standardCloudMatchSessionControlBaseUrl(
            "NP-LON-08.cloudmatchbeta.nvidiagrid.net",
            443,
        )
        val created = session(status = 1, assignedZone = "NP-LON-08").copy(
            streamingBaseUrl = "https://prod.cloudmatchbeta.nvidiagrid.net",
            sessionControlBaseUrl = controlBase,
        )

        assertEquals("https://np-lon-08.cloudmatchbeta.nvidiagrid.net", created.sessionControlPollBaseUrl())
        assertEquals(
            "https://np-lon-06.cloudmatchbeta.nvidiagrid.net",
            created.copy(sessionControlBaseUrl = "https://np-lon-06.cloudmatchbeta.nvidiagrid.net").sessionControlPollBaseUrl(),
        )
        assertNull(created.copy(streamingBaseUrl = "https://alliance.example.com").sessionControlPollBaseUrl())
        assertNull(created.copy(sessionControlBaseUrl = "https://np-lon-08.example.com").sessionControlPollBaseUrl())
    }

    @Test
    fun `queue rejects unsafe or unsupported control endpoints`() {
        assertNull(standardCloudMatchSessionControlBaseUrl("np-lon-08.example.com", 443))
        assertNull(standardCloudMatchSessionControlBaseUrl("np-lon-08.cloudmatchbeta.nvidiagrid.net", 8443))
        assertNull(standardCloudMatchSessionControlBaseUrl("np-lon-08.cloudmatchbeta.nvidiagrid.net@evil.example", 443))
        assertNull(standardCloudMatchSessionControlBaseUrl("np-lon-08.foo.cloudmatchbeta.nvidiagrid.net", 443))
    }

    @Test
    fun `reported server prefers assignment over request zone`() {
        val session = SessionInfo(
            sessionId = "session-1",
            status = 2,
            zone = "NP-LAX-03",
            assignedZone = "NP-PDX-01",
            serverIp = "203.0.113.10",
            signalingServer = "203.0.113.10:443",
            signalingUrl = "wss://203.0.113.10:443/nvst/",
        )

        assertEquals("NP-PDX-01", session.reportedServerZone())
        assertEquals("NP-LAX-03", session.copy(assignedZone = null).reportedServerZone())
    }

    @Test
    fun `ready session update preserves an earlier assignment when provider omits it`() {
        val previous = session(status = 1, assignedZone = "NP-PDX-01")
        val readyWithoutAssignment = session(status = 2, assignedZone = null)

        assertEquals(
            "NP-PDX-01",
            mergeQueueSessionState(previous, readyWithoutAssignment).assignedZone,
        )
    }

    private fun session(status: Int, assignedZone: String?): SessionInfo = SessionInfo(
        sessionId = "session-1",
        status = status,
        zone = "NP-LAX-03",
        assignedZone = assignedZone,
        serverIp = "203.0.113.10",
        signalingServer = "203.0.113.10:443",
        signalingUrl = "wss://203.0.113.10:443/nvst/",
    )
}
