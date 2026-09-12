package com.opencloudgaming.opennow

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.*
import org.junit.Assert.*
import org.junit.Test

class NvstTransportTest {
    @Test fun networkMetricsKeepMeasuredRttIndependentOfVideoArrival() {
        assertEquals(NvstNetworkMetrics(null, null, 23), parseNvstNetworkMetrics(",,22.6"))
        assertEquals(NvstNetworkMetrics(0.4, 0.0, 15), parseNvstNetworkMetrics("0.4,0,15.2"))
        assertEquals(NvstNetworkMetrics(0.4, 0.0, null), parseNvstNetworkMetrics("0.4,0,"))
        assertEquals(NvstNetworkMetrics(null, null, null), parseNvstNetworkMetrics("NaN,101,Infinity"))
        assertEquals(NvstNetworkMetrics(null, null, null), parseNvstNetworkMetrics("-1,-1,-1"))
    }

    @Test fun newAndLegacyProfilesDefaultOff() {
        for (raw in listOf(null, "invalid", "{}", """{"stream":{"fps":120,"experimentalNvst":true}}""")) {
            val loaded = loadSettingsWithNvstDefault(raw)
            assertFalse(loaded.stream.experimentalNvst)
            assertEquals(NVST_OPT_IN_VERSION, loaded.nvstOptInVersion)
        }
    }

    @Test fun upgradePersistsResetWithoutRequiringASettingsEdit() {
        val previous = AppSettings(
            stream = StreamSettings(experimentalNvst = true, fps = 120, maxBitrateMbps = 42),
            favoriteGameIds = listOf("123"),
        )
        var disk = OpenNowJson.encodeToString(previous)
        var writes = 0
        val migrated = loadSettingsWithNvstDefault(disk) {
            writes++
            disk = OpenNowJson.encodeToString(it)
        }
        assertEquals(previous.copy(
            stream = previous.stream.copy(experimentalNvst = false),
            nvstOptInVersion = NVST_OPT_IN_VERSION,
        ), migrated)
        assertEquals(1, writes)
        assertEquals(migrated, loadSettingsWithNvstDefault(disk) { writes++ })
        assertEquals(1, writes)
    }

    @Test fun explicitChoicesAfterMigrationSurviveRestartAndFutureVersions() {
        for (version in listOf(NVST_OPT_IN_VERSION, NVST_OPT_IN_VERSION + 1)) {
            for (choice in listOf(false, true)) {
                val settings = AppSettings(
                    stream = StreamSettings(experimentalNvst = choice), nvstOptInVersion = version,
                )
                assertEquals(settings, loadSettingsWithNvstDefault(OpenNowJson.encodeToString(settings)) {
                    fail("Already migrated settings should not be rewritten")
                })
            }
        }
    }

    @Test fun serverResolutionFallbackCannotRepeatedlyRestartTheDecoder() {
        val session = SessionInfo("id", 3, serverIp = "", signalingServer = "", signalingUrl = "",
            negotiatedStreamProfile = NegotiatedStreamProfile(resolution = "1366x768"))
        val requested = StreamSettings(resolution = "1680x720")
        assertEquals(1366 to 768, nvstDecoderInitialSize(session, requested))
        assertEquals(streamResolutionPixels(requested), nvstDecoderInitialSize(
            session.copy(negotiatedStreamProfile = null), requested))
        var released = 0
        for (keyframe in listOf(true, false, false)) {
            val frame = nvstEncodedImage(java.nio.ByteBuffer.allocateDirect(16), 123L, keyframe) { released++ }
            // AndroidVideoDecoder reinitializes if positive frame dimensions disagree with its
            // decoded dimensions. Unknown access-unit sizes must let the bitstream decide.
            assertFalse(frame.encodedWidth * frame.encodedHeight > 0 &&
                (frame.encodedWidth != 1366 || frame.encodedHeight != 768))
            assertEquals(123L, frame.captureTimeNs)
            frame.release()
        }
        assertEquals(3, released)
    }

    @Test fun nvstClaimRequestsSecureRtspWithoutWebRtcOverride() {
        for (nvst in listOf(false, true)) {
            val settings = StreamSettings(experimentalNvst = nvst)
            val request = buildMinimalClaimRequestBody("123", "device", settings)["sessionRequestData"]!!.jsonObject
            assertEquals(nvst, request["secureRTSPSupported"]!!.jsonPrimitive.boolean)
            assertEquals(if (nvst) "14" else "1", request["streamerVersion"]!!.jsonPrimitive.content)
            assertEquals(if (nvst) "2.0" else "1.0", request["sdkVersion"]!!.jsonPrimitive.content)
            assertEquals(if (nvst) 0 else 1, request["enhancedStreamMode"]!!.jsonPrimitive.int)
            val streamer = request["metaData"]!!.jsonArray.firstOrNull { it.jsonObject["key"]?.jsonPrimitive?.content == "GSStreamerType" }
            assertEquals(!nvst, streamer != null)
            assertEquals(nvst, settings.requiresNativeDesktopCloudMatchMode())
            if (nvst) {
                val features = request["requestedStreamingFeatures"]!!.jsonObject
                assertEquals(1, features["dynamicStreamingMode"]!!.jsonPrimitive.int)
            }
        }
    }

    @Test fun deviceRecommendationsPreserveTransportChoice() {
        val user = StreamSettings(experimentalNvst = true, microphoneMode = MicrophoneMode.Disabled)
        assertTrue(StreamSettings().withUserStreamOptionsFrom(user).experimentalNvst)
        assertFalse(StreamSettings(experimentalNvst = true).withUserStreamOptionsFrom(user.copy(experimentalNvst = false)).experimentalNvst)
    }

    @Test fun capturedRtspControlPortsCannotBecomeMediaPeers() {
        val connections = OpenNowJson.parseToJsonElement("""[
            {"protocol":1,"appLevelProtocol":6,"ip":null,"resourcePath":"rtsps://seat.nvidiagrid.net:322","usage":14,"port":322},
            {"protocol":1,"appLevelProtocol":6,"ip":null,"resourcePath":"rtsps://seat.nvidiagrid.net:48322","usage":14,"port":48322}
        ]""").jsonArray.map { it.jsonObject }
        assertEquals(2, nvstRtspEndpoints(JsonArray(connections)).size)
        assertTrue(webRtcMediaFallbackConnections(connections).isEmpty())
        val webRtc = OpenNowJson.parseToJsonElement("""{"usage":14,"ip":"8.8.8.8","port":49003,"resourcePath":"/nvst/"}""").jsonObject
        assertEquals(listOf(webRtc), webRtcMediaFallbackConnections(connections + webRtc))
        // Some responses identify RTSP only by resourcePath or by application protocol.
        for (connection in connections) {
            assertTrue(webRtcMediaFallbackConnections(listOf(JsonObject(connection - "resourcePath"))).isEmpty())
            assertTrue(webRtcMediaFallbackConnections(listOf(JsonObject(connection - "appLevelProtocol"))).isEmpty())
        }
    }

    @Test fun endpointParsingRetainsRtspServiceAndIpv6Port() {
        val connections = OpenNowJson.parseToJsonElement("""[
          {"usage":14,"ip":"1.2.3.4","port":443,"resourcePath":"/nvst/"},
          {"usage":16,"ip":"2001:4860:4860::8888","port":322},
          {"appLevelProtocol":6,"resourcePath":"rtsps://seat.nvidiagrid.net:48010"},
          {"usage":2,"ip":"1.2.3.4","port":49005}
        ]""").jsonArray
        assertEquals(listOf("rtsps://[2001:4860:4860::8888]:322", "rtsps://seat.nvidiagrid.net:48010"), nvstRtspEndpoints(connections))
        assertTrue(nvstRtspEndpoints(null).isEmpty())
    }

    @Test fun transportChoiceParticipatesInSessionReuseSignature() {
        val webRtc = StreamSettings()
        assertNotEquals(streamSettingsSessionSignature(webRtc), streamSettingsSessionSignature(webRtc.copy(experimentalNvst = true)))
    }

    @Test fun nvstContextPreservesSelectedProfileAndEndpoints() {
        val session = SessionInfo("session", 2, serverIp = "8.8.8.8", signalingServer = "", signalingUrl = "",
            rtspsEndpoints = listOf("rtsps://seat.nvidiagrid.net:322"), mediaConnectionInfo = MediaConnectionInfo("8.8.4.4", 49003))
        val settings = StreamSettings(experimentalNvst = true, fps = 360, resolution = "2560x1440", codec = VideoCodec.H265, colorQuality = ColorQuality.TenBit420)
        val context = OpenNowJson.parseToJsonElement(nvstSessionContext(session, settings)).jsonObject
        assertEquals(49003, context["session"]!!.jsonObject["mediaConnectionInfo"]!!.jsonObject["port"]!!.jsonPrimitive.int)
        assertEquals(360, context["settings"]!!.jsonObject["fps"]!!.jsonPrimitive.int)
        assertEquals("10bit_420", context["settings"]!!.jsonObject["colorQuality"]!!.jsonPrimitive.content)
        val adaptation = context["settings"]!!.jsonObject["networkAdaptation"]!!.jsonObject
        assertEquals(1, adaptation["dynamicStreamingMode"]!!.jsonPrimitive.int)
        assertEquals(1000, adaptation["minimumBitrateKbps"]!!.jsonPrimitive.int)
        assertEquals(18750, adaptation["initialBitrateKbps"]!!.jsonPrimitive.int)
        assertEquals(session.rtspsEndpoints.first(), context["session"]!!.jsonObject["rtspsEndpoints"]!!.jsonArray.first().jsonPrimitive.content)
    }
}
