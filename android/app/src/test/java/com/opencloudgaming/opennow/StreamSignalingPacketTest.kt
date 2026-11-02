package com.opencloudgaming.opennow

import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import org.junit.Assert.assertEquals
import org.junit.Test

class StreamSignalingPacketTest {
    private fun client(events: MutableList<SignalingEvent>) = GfnSignalingClient(
        session = SessionInfo("test", 2, serverIp = "example.test", signalingServer = "example.test:443", signalingUrl = "wss://example.test/"),
        settings = StreamSettings(),
        peerName = "local",
        onEvent = events::add,
    )

    @Test fun removedPeerIsReportedBeforeTheSocketCanReplaceItWithGenericEof() {
        val events = mutableListOf<SignalingEvent>()
        client(events).handleMessage("""{"error":"peerRemoved"}""")
        val error = events.filterIsInstance<SignalingEvent.Error>().single()
        assertEquals(SignalingFailureDisposition.RecoverPeer, signalingFailureDisposition(error.message))
    }

    @Test fun nullOrWronglyTypedOptionalFieldsDoNotKillTheWebSocketReader() {
        val events = mutableListOf<SignalingEvent>()
        val client = client(events)
        listOf("null", "[]", "1", "{}", "\"text\"").forEach { value ->
            client.handleMessage("""{"peer_info":$value,"peer_msg":$value,"ackid":$value}""")
            client.handleMessage("""{"peer_info":{"name":$value,"id":$value},"peer_msg":{"from":$value,"msg":$value}}""")
            val payload = """{"type":$value,"sdp":$value,"candidate":$value,"sdpMid":$value,"sdpMLineIndex":$value}"""
            client.handleMessage(buildJsonObject { put("peer_msg", buildJsonObject { put("msg", payload) }) }.toString())
        }
        // The same reader must still accept a good offer after bad packets.
        val offer = """{"type":"offer","sdp":"v=0\\r\\n"}"""
        client.handleMessage(buildJsonObject { put("peer_msg", buildJsonObject { put("msg", offer) }) }.toString())
        assertEquals(1, events.filterIsInstance<SignalingEvent.Offer>().size)
    }

    @Test fun explicitNullMidIsRetainedForExistingIceNormalization() {
        val events = mutableListOf<SignalingEvent>()
        val payload = """{"candidate":"candidate:1 1 udp 123 127.0.0.1 123 typ host","sdpMid":null,"sdpMLineIndex":2}"""
        client(events).handleMessage(buildJsonObject { put("peer_msg", buildJsonObject { put("from", 1); put("msg", payload) }) }.toString())
        assertEquals(2, events.filterIsInstance<SignalingEvent.RemoteIce>().single().candidate.sdpMLineIndex)
    }
}
