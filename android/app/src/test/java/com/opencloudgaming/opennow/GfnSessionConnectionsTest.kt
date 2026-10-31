package com.opencloudgaming.opennow

import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class GfnSessionConnectionsTest {
    private val yesBase = "https://my-yes.yes.geforcenow.nvidiagrid.net/"

    // Shape from the YES incident: native-looking control descriptors coexist with
    // an explicitly reported WebRTC allocation and separate VIDEO/BUNDLE mappings.
    private fun partnerSession(requestData: String, status: Int = 2, enhancedStreamMode: Int = 0) =
        OpenNowJson.parseToJsonElement("""{
            "status":$status,"enhancedStreamMode":$enhancedStreamMode,
            "sessionRequestData":$requestData,
            "connectionInfo":[
                {"usage":14,"appLevelProtocol":6,"port":322,"resourcePath":"rtsps://seat.yes.example:322"},
                {"usage":14,"appLevelProtocol":6,"port":48322,"resourcePath":"rtsps://seat.yes.example:48322"},
                {"usage":2,"protocol":2,"ip":"203.0.113.20","port":13334},
                {"usage":17,"protocol":2,"ip":"203.0.113.20","port":18342}
            ]
        }""").jsonObject

    private val webRtcRequest = """{"secureRTSPSupported":false,"streamerVersion":"1",
        "metaData":[{"key":"GSStreamerType","value":"WebRTC"}]}"""

    private fun resolve(raw: String) = resolveGfnSessionConnections(
        buildJsonObject { put("connectionInfo", OpenNowJson.parseToJsonElement(raw)) },
    )

    @Test fun nvidiaWebRtcMetadataOverridesNativeLookingDescriptorsEvenWithToggleOff() {
        // RMX3890 H.264/H.265 reports: NVIDIA returns these descriptors for an allocation
        // whose request explicitly says WebRTC, secureRTSPSupported=false, streamerVersion=1.
        val payload = OpenNowJson.parseToJsonElement("""{
            "sessionId":"nvidia-seat","status":2,"sessionRequestData":$webRtcRequest,
            "connectionInfo":[
                {"usage":14,"appLevelProtocol":6,"port":322,
                 "resourcePath":"rtsps://80-250-96-36.cloudmatchbeta.nvidiagrid.net:322"},
                {"usage":14,"appLevelProtocol":6,"port":48322,
                 "resourcePath":"rtsps://80-250-96-36.cloudmatchbeta.nvidiagrid.net:48322"}
            ]
        }""").jsonObject
        val route = resolveGfnSessionConnections(payload)!!
        assertFalse(route.nativeTransportRequired)
        assertEquals("wss://80-250-96-36.cloudmatchbeta.nvidiagrid.net/nvst/", route.signalingUrl)
        val session = SessionInfo("seat", 2, serverIp = route.serverIp,
            signalingServer = route.signalingServer, signalingUrl = route.signalingUrl,
            nativeTransportRequired = route.nativeTransportRequired)
        assertFalse(session.usesNvstTransport(StreamSettings(experimentalNvst = false)))
        assertTrue(session.usesNvstTransport(StreamSettings(experimentalNvst = true)))
    }

    @Test fun explicitWebSocketWinsOverEarlierNativeDescriptorAndKeepsItsPort() {
        val result = resolve("""[
            {"usage":14,"appLevelProtocol":6,"resourcePath":"rtsps://native.abya.example:322"},
            {"usage":14,"ip":"203.0.113.10","port":444,"resourcePath":"wss://web.abya.example:8443/nvst/?version=1"},
            {"usage":2,"ip":"203.0.113.20","port":49005},
            {"usage":17,"ip":"203.0.113.21","port":13749}
        ]""")!!
        assertFalse(result.nativeTransportRequired)
        assertEquals("wss://web.abya.example:8443/nvst/?version=1", result.signalingUrl)
        assertEquals("web.abya.example:8443", result.signalingServer)
        assertEquals("203.0.113.10", result.serverIp)
        assertEquals(MediaConnectionInfo("203.0.113.20", 49005), result.mediaConnectionInfo)
        assertEquals(MediaConnectionInfo("203.0.113.21", 13749), result.bundleConnectionInfo)
        val session = SessionInfo("id", 2, serverIp = result.serverIp,
            signalingServer = result.signalingServer, signalingUrl = result.signalingUrl,
            mediaConnectionInfo = result.mediaConnectionInfo, bundleConnectionInfo = result.bundleConnectionInfo)
        val context = OpenNowJson.parseToJsonElement(nvstSessionContext(session, StreamSettings())).jsonObject
        val bundle = context["session"]!!.jsonObject["connectionInfo"]!!.jsonArray.single().jsonObject
        assertEquals(17, bundle["usage"]!!.jsonPrimitive.int)
        assertEquals(13749, bundle["port"]!!.jsonPrimitive.int)
    }

    @Test fun nativeOnlySeatsUseWebSocket443AndNeverRtspForUdpMedia() {
        for (usage in listOf(14, 16)) {
            val result = resolve("""[{"usage":$usage,"appLevelProtocol":6,
                "resourcePath":"rtsps://seat.abya.example:48322/rtsp"}]""")!!
            assertEquals("wss://seat.abya.example/nvst/", result.signalingUrl)
            assertEquals("seat.abya.example:443", result.signalingServer)
            assertTrue(result.nativeTransportRequired)
            assertNull(result.mediaConnectionInfo)
            assertNull(result.bundleConnectionInfo)
        }
    }

    @Test fun relativeWebSocketUses443AndKeepsIpv6MediaPortSeparate() {
        val result = resolve("""[{"usage":14,"ip":"2001:db8::10","port":8443,"resourcePath":"/nvst/"}]""")!!
        assertEquals("wss://[2001:db8::10]/nvst/", result.signalingUrl)
        assertEquals("[2001:db8::10]:443", result.signalingServer)
        assertEquals("2001:db8::10", result.serverIp)
        assertEquals(MediaConnectionInfo("2001:db8::10", 8443), result.mediaConnectionInfo)
    }

    @Test fun nvidiaRelativeSignalingDoesNotUseTheAdvertisedUdpMediaPort() {
        for (resource in listOf("null", "\"/nvst/\"", "\"/nvst/?version=1\"")) {
            val result = resolve("""[{"usage":14,"ip":"203.0.113.10","port":49003,
                "resourcePath":$resource}]""")!!
            val path = if (resource.contains("version")) "/nvst/?version=1" else "/nvst/"
            assertEquals("wss://203.0.113.10$path", result.signalingUrl)
            assertEquals("203.0.113.10:443", result.signalingServer)
            assertEquals(MediaConnectionInfo("203.0.113.10", 49003), result.mediaConnectionInfo)
            assertEquals("wss://203.0.113.10/nvst/sign_in?peer_id=peer-test&version=2&peer_role=1&pairing_id=session-test",
                buildGfnSignInUrl(result.signalingUrl, result.signalingServer, "peer-test", "session-test", reconnect = false))
        }
    }

    @Test fun nativeRelativeDescriptorDoesNotReuseRtspPortForWebSocket() {
        val result = resolve("""[{"usage":16,"ip":"seat.abya.example","port":322}]""")!!
        assertEquals("wss://seat.abya.example/nvst/", result.signalingUrl)
    }

    @Test fun invalidCandidatesDoNotHideLaterValidMappings() {
        val result = resolve("""[
            {"usage":14,"resourcePath":"wss://user:password@seat.example/nvst/"},
            {"usage":14,"ip":"0.0.0.0","resourcePath":"/nvst/"},
            {"usage":14,"ip":"","port":70000,"resourcePath":"/nvst/"},
            {"usage":14,"resourcePath":"https://seat.example:9443/nvst/"},
            {"usage":2,"resourcePath":"https://seat.example/media"},
            {"usage":17,"ip":"not-an-ip.example","port":49000},
            {"usage":17,"ip":"203.0.113.10","port":0},
            {"usage":17,"ip":"203.0.113.11","port":49003}
        ]""")!!
        assertEquals("wss://seat.example:9443/nvst/", result.signalingUrl)
        assertEquals(MediaConnectionInfo("203.0.113.11", 49003), result.bundleConnectionInfo)
        assertEquals(MediaConnectionInfo("not-an-ip.example", 49000), result.mediaConnectionInfo)
        assertNull(resolve("""[{"usage":2,"ip":"203.0.113.10","port":49005}]"""))
    }

    @Test fun legacySavedSessionsStillLoadWithoutBundleMapping() {
        val session = OpenNowJson.decodeFromString<SessionInfo>("""{"sessionId":"old","status":2,
            "serverIp":"203.0.113.10","signalingServer":"seat.example:443","signalingUrl":"wss://seat.example/nvst/"}""")
        assertFalse(session.nativeTransportRequired)
        assertNull(session.bundleConnectionInfo)
    }

    @Test fun nativeWebSocketDescriptorDoesNotHideRealWebRtcEndpoint() {
        val result = resolve("""[
            {"usage":14,"appLevelProtocol":6,"resourcePath":"wss://native.example/nvst/"},
            {"usage":14,"appLevelProtocol":5,"resourcePath":"wss://webrtc.example/nvst/"}
        ]""")!!
        assertFalse(result.nativeTransportRequired)
        assertEquals("wss://webrtc.example/nvst/", result.signalingUrl)
    }

    @Test fun nativeAllocationSelectsNvstWithoutPersistingExperimentalOptIn() {
        val settings = StreamSettings(experimentalNvst = false)
        val session = SessionInfo("id", 2, serverIp = "seat.example",
            signalingServer = "seat.example:443", signalingUrl = "wss://seat.example/nvst/",
            rtspsEndpoints = listOf("rtsps://seat.example:322"), nativeTransportRequired = true)
        assertTrue(session.usesNvstTransport(settings))
        assertFalse(settings.experimentalNvst)
        assertFalse(session.copy(nativeTransportRequired = false).usesNvstTransport(settings))
        assertTrue(session.copy(nativeTransportRequired = false).usesNvstTransport(settings.copy(experimentalNvst = true)))
        assertNotEquals(session.nativeStreamTransportIdentity(), session.copy(nativeTransportRequired = false).nativeStreamTransportIdentity())
    }

    @Test fun yesWebRtcAllocationUsesWebRtcEvenWithRtspDescriptorsAndEnhancedModeZero() {
        for ((status, mode) in listOf(1 to 1, 2 to 0, 3 to 0)) {
            val result = resolveGfnSessionConnections(partnerSession(webRtcRequest, status, mode))!!
            assertFalse(result.nativeTransportRequired)
            assertEquals("wss://seat.yes.example/nvst/", result.signalingUrl)
            assertEquals("seat.yes.example:443", result.signalingServer)
            assertEquals(MediaConnectionInfo("203.0.113.20", 13334), result.mediaConnectionInfo)
            assertEquals(MediaConnectionInfo("203.0.113.20", 18342), result.bundleConnectionInfo)
            assertEquals(
                "wss://seat.yes.example/nvst/sign_in?peer_id=peer&version=2&peer_role=1&pairing_id=seat",
                buildGfnSignInUrl(result.signalingUrl, result.signalingServer, "peer", "seat", reconnect = false),
            )
            val session = SessionInfo("seat", status, streamingBaseUrl = yesBase,
                serverIp = result.serverIp, signalingServer = result.signalingServer,
                signalingUrl = result.signalingUrl, nativeTransportRequired = result.nativeTransportRequired,
                rtspsEndpoints = nvstRtspEndpoints(partnerSession(webRtcRequest)["connectionInfo"] as JsonArray))
            assertFalse(session.usesNvstTransport(StreamSettings()))
            assertEquals(2, session.rtspsEndpoints.size)
        }
    }

    @Test fun partnerNativeAllocationWinsOverWebRtcMetadataAndEndpointShape() {
        for (request in listOf(
            """{"secureRTSPSupported":true,"streamerVersion":"14"}""",
            """{"secureRTSPSupported":false,"streamerVersion":14,"metaData":[{"key":"GSStreamerType","value":"WebRTC"}]}""",
            """{"secureRTSPSupported":true,"metaData":[{"key":"GSStreamerType","value":"WebRTC"}]}""",
        )) {
            val session = partnerSession(request)
            assertTrue(resolveGfnSessionConnections(session)!!.nativeTransportRequired)
            val webSocketSession = JsonObject(session + ("connectionInfo" to OpenNowJson.parseToJsonElement(
                """[{"usage":14,"appLevelProtocol":4,"resourcePath":"wss://seat.yes.example/nvst/"}]""")))
            assertTrue(resolveGfnSessionConnections(webSocketSession)!!.nativeTransportRequired)
        }
    }

    @Test fun partnerSessionsWithoutReportedTransportKeepEndpointFallback() {
        for (request in listOf("{}", """{"secureRTSPSupported":false}""",
            """{"metaData":[null,1,{}, {"key":"Other","value":"WebRTC"}]}""")) {
            assertTrue(resolveGfnSessionConnections(partnerSession(request))!!.nativeTransportRequired)
        }
        val session = partnerSession("""{"metaData":[{"key":"GSStreamerType","value":"WebRTC"}]}""")
        assertFalse(resolveGfnSessionConnections(session)!!.nativeTransportRequired)
    }

    @Test fun sessionsWithoutReportedTransportPreserveEndpointBasedSelection() {
        for (request in listOf("{}", """{"secureRTSPSupported":false,"streamerVersion":"1"}""")) {
            val session = partnerSession(request)
            assertTrue(resolveGfnSessionConnections(session)!!.nativeTransportRequired)
            val webSocketSession = JsonObject(session + ("connectionInfo" to OpenNowJson.parseToJsonElement(
                """[{"usage":14,"appLevelProtocol":4,"ip":"203.0.113.10","port":49003,"resourcePath":"/nvst/"}]""")))
            val result = resolveGfnSessionConnections(webSocketSession)!!
            assertFalse(result.nativeTransportRequired)
            assertEquals("wss://203.0.113.10/nvst/", result.signalingUrl)
            assertEquals(MediaConnectionInfo("203.0.113.10", 49003), result.mediaConnectionInfo)
        }
    }

    @Test fun sparsePollAndResumeRepliesRetainExplicitTransportForOnlyTheSameSession() {
        val resolver = GfnSessionConnectionResolver()
        fun seat(id: String, request: String) = JsonObject(partnerSession(request) + ("sessionId" to JsonPrimitive(id)))
        assertFalse(resolver.resolve(seat("webrtc-seat", webRtcRequest))!!.nativeTransportRequired)
        repeat(3) {
            assertFalse(resolver.resolve(seat("webrtc-seat", "{}"))!!.nativeTransportRequired)
        }
        assertTrue(resolver.resolve(seat("different-seat", "{}"))!!.nativeTransportRequired)
        assertTrue(resolver.resolve(seat("webrtc-seat", """{"secureRTSPSupported":true}"""))!!.nativeTransportRequired)
        assertTrue(resolver.resolve(seat("webrtc-seat", "{}"))!!.nativeTransportRequired)
    }

    @Test fun transportIsRememberedBeforeQueueRepliesHaveUsableEndpointsAndRetentionIsBounded() {
        val resolver = GfnSessionConnectionResolver(retainedSessionLimit = 2)
        fun seat(id: String, request: String) = JsonObject(partnerSession(request) + ("sessionId" to JsonPrimitive(id)))
        val queued = JsonObject(seat("first", webRtcRequest) + ("connectionInfo" to JsonArray(emptyList())))
        assertNull(resolver.resolve(queued))
        assertFalse(resolver.resolve(seat("first", "{}"))!!.nativeTransportRequired)
        resolver.resolve(seat("second", webRtcRequest))
        resolver.resolve(seat("third", webRtcRequest))
        assertTrue(resolver.resolve(seat("first", "{}"))!!.nativeTransportRequired) // Evicted; use descriptor fallback.
        assertFalse(resolver.resolve(seat("second", "{}"))!!.nativeTransportRequired)
        assertFalse(resolver.resolve(seat("third", "{}"))!!.nativeTransportRequired)
    }

}
