package com.opencloudgaming.opennow

import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import org.webrtc.PeerConnection

class StreamSignalingFailureTest {
    @Test
    fun freshSignalingUsesTheExistingProtocolWithoutReconnectMarker() {
        assertEquals(
            "wss://stream.example.test/nvst/sign_in?" +
                "peer_id=peer-fresh&version=2&peer_role=1&pairing_id=session-fresh",
            buildGfnSignInUrl(
                signalingUrl = "wss://stream.example.test/nvst/",
                signalingServer = "ignored.example.test:443",
                peerName = "peer-fresh",
                sessionId = "session-fresh",
                reconnect = false,
            ),
        )
    }

    @Test
    fun reconnectUsesCapturedVersionAndMarkerWithoutDuplicatingSignInPath() {
        assertEquals(
            "wss://66-22-141-142.cloudmatchbeta.nvidiagrid.net/nvst/sign_in?" +
                "peer_id=peer-6258383480&version=3.0&peer_role=1&" +
                "pairing_id=b6039c26-5e1d-44c8-bfdb-fee9bc75c2ab&reconnect=1",
            buildGfnSignInUrl(
                signalingUrl = "wss://66-22-141-142.cloudmatchbeta.nvidiagrid.net/nvst/sign_in?stale=1",
                signalingServer = "ignored.example.test:443",
                peerName = "peer-6258383480",
                sessionId = "b6039c26-5e1d-44c8-bfdb-fee9bc75c2ab",
                reconnect = true,
            ),
        )
    }

    @Test
    fun staleSignalingEndpointRequestsSessionRecovery() {
        assertEquals(
            SignalingFailureDisposition.RecoverSession,
            signalingFailureDisposition("Expected HTTP 101 response but was '404 Not Found' http=404"),
        )
    }

    @Test
    fun onlyExplicitProviderGoneResponseIsTerminal() {
        assertEquals(
            SignalingFailureDisposition.SessionEnded,
            signalingFailureDisposition("http=410 Gone"),
        )
        assertEquals(SignalingFailureDisposition.RetryTransport, signalingFailureDisposition("code=1000"))
    }

    @Test
    fun normalServerCloseAlwaysRetriesInsteadOfEndingSession() {
        assertEquals(
            SignalingFailureDisposition.RetryTransport,
            signalingFailureDisposition("code=1000"),
        )
    }

    @Test
    fun transientSignalingFailureRetriesTransport() {
        assertEquals(
            SignalingFailureDisposition.RetryTransport,
            signalingFailureDisposition("socket timeout"),
        )
    }

    @Test
    fun serviceUnavailableUsesSeparateBoundedSignalingBackoff() {
        assertEquals(
            SignalingFailureDisposition.RetrySignaling,
            signalingFailureDisposition(
                "ProtocolException: Expected HTTP 101 response but was '503 Service Unavailable' http=503",
            ),
        )
        assertEquals(SignalingFailureDisposition.RetrySignaling, signalingFailureDisposition("http=429"))
        assertEquals(1_000L, transientSignalingRetryDelayMs(1))
        assertEquals(2_000L, transientSignalingRetryDelayMs(2))
        assertEquals(4_000L, transientSignalingRetryDelayMs(3))
        assertNull(transientSignalingRetryDelayMs(4))
    }

    @Test
    fun bitrateChangesAreNormalizedBeforeBeingQueuedForTheNextOffer() {
        assertEquals(1_000, normalizedLiveBitrateKbps(0))
        assertEquals(1_000, normalizedLiveBitrateKbps(1_499))
        assertEquals(2_000, normalizedLiveBitrateKbps(1_500))
        assertEquals(75_000, normalizedLiveBitrateKbps(75_000))
    }

    @Test
    fun serverHeartbeatGetsImmediateProtocolReply() {
        assertEquals(
            """{"hb":1}""",
            signalingHeartbeatReply(buildJsonObject { put("hb", 1) }),
        )
        assertNull(signalingHeartbeatReply(buildJsonObject { put("ack", 1) }))
    }

    @Test
    fun activeIceTransportSurvivesTransientSignalingFailure() {
        val transient = SignalingFailureDisposition.RetryTransport

        assertTrue(shouldPreserveMediaAfterSignalingFailure(transient, PeerConnection.IceConnectionState.CHECKING))
        assertTrue(shouldPreserveMediaAfterSignalingFailure(transient, PeerConnection.IceConnectionState.CONNECTED))
        assertTrue(shouldPreserveMediaAfterSignalingFailure(transient, PeerConnection.IceConnectionState.COMPLETED))
        assertTrue(
            shouldPreserveMediaAfterSignalingFailure(
                SignalingFailureDisposition.RetrySignaling,
                PeerConnection.IceConnectionState.CONNECTED,
            ),
        )
        assertFalse(shouldPreserveMediaAfterSignalingFailure(transient, PeerConnection.IceConnectionState.DISCONNECTED))
        assertFalse(shouldPreserveMediaAfterSignalingFailure(transient, PeerConnection.IceConnectionState.FAILED))
        assertFalse(shouldPreserveMediaAfterSignalingFailure(transient, null))
    }

    @Test
    fun terminalAndStaleSignalingFailuresStillTakeTheirNormalRecoveryPaths() {
        assertFalse(
            shouldPreserveMediaAfterSignalingFailure(
                SignalingFailureDisposition.SessionEnded,
                PeerConnection.IceConnectionState.CONNECTED,
            ),
        )
        assertFalse(
            shouldPreserveMediaAfterSignalingFailure(
                SignalingFailureDisposition.RecoverSession,
                PeerConnection.IceConnectionState.CONNECTED,
            ),
        )
    }

    @Test
    fun peerNativeOperationsRequireTheCurrentGenerationAndPeerIdentity() {
        val currentPeer = Any()

        assertTrue(
            isCurrentPeerOperation(
                operationGeneration = 7,
                currentGeneration = 7,
                expectedPeer = currentPeer,
                activePeer = currentPeer,
            ),
        )
        assertFalse(
            isCurrentPeerOperation(
                operationGeneration = 6,
                currentGeneration = 7,
                expectedPeer = currentPeer,
                activePeer = currentPeer,
            ),
        )
        assertFalse(
            isCurrentPeerOperation(
                operationGeneration = 7,
                currentGeneration = 7,
                expectedPeer = Any(),
                activePeer = currentPeer,
            ),
        )
        assertFalse(
            isCurrentPeerOperation(
                operationGeneration = 7,
                currentGeneration = 7,
                expectedPeer = currentPeer,
                activePeer = null,
            ),
        )
    }

    @Test
    fun unboundPeerOperationCanAcquireOnlyTheCurrentActivePeer() {
        val currentPeer = Any()

        assertTrue(
            isCurrentPeerOperation(
                operationGeneration = 12,
                currentGeneration = 12,
                expectedPeer = null,
                activePeer = currentPeer,
            ),
        )
        assertFalse(
            isCurrentPeerOperation(
                operationGeneration = 11,
                currentGeneration = 12,
                expectedPeer = null,
                activePeer = currentPeer,
            ),
        )
    }
}
