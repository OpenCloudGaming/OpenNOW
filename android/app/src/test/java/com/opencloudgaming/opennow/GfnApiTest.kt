package com.opencloudgaming.opennow

import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.HttpUrl.Companion.toHttpUrl
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class GfnApiTest {
    @Test
    fun canonicalizesOldGamesGraphQlHost() {
        val url = canonicalizeGfnRequestUrl(
            "https://games.geforcenow.com/graphql?requestType=panels%2FMainV2".toHttpUrl(),
        )

        assertEquals("https", url.scheme)
        assertEquals("games.geforce.com", url.host)
        assertEquals("/graphql", url.encodedPath)
        assertEquals("panels/MainV2", url.queryParameter("requestType"))
    }

    @Test
    fun leavesCanonicalGamesGraphQlHostUnchanged() {
        val source = "https://games.geforce.com/graphql".toHttpUrl()

        assertEquals(source, canonicalizeGfnRequestUrl(source))
    }

    @Test
    fun claimRequestDoesNotRenegotiateMonitorSettings() {
        val body = buildMinimalClaimRequestBody(appId = "123", deviceId = "device")
        val sessionRequestData = body.getValue("sessionRequestData").jsonObject
        val metadata = sessionRequestData.getValue("metaData").jsonArray

        assertEquals(2, body.getValue("action").jsonPrimitive.int)
        assertEquals(123, sessionRequestData.getValue("appId").jsonPrimitive.int)
        assertFalse(sessionRequestData.containsKey("clientRequestMonitorSettings"))
        assertFalse(sessionRequestData.containsKey("requestedStreamingFeatures"))
        assertTrue(metadata.none { item ->
            item.jsonObject["key"]?.jsonPrimitive?.contentOrNull == "clientPhysicalResolution"
        })
    }

    @Test
    fun claimRequestKeepsOpenNowStreamSettingsSignature() {
        val settings = StreamSettings(
            resolution = "1680x720",
            aspectRatio = "21:9",
            fps = 60,
            maxBitrateMbps = 150,
            codec = VideoCodec.H265,
        )
        val body = buildMinimalClaimRequestBody(appId = "123", deviceId = "device", settings = settings)
        val sessionRequestData = body.getValue("sessionRequestData").jsonObject
        val metadata = sessionRequestData.getValue("metaData").jsonArray
        val signature = metadata.firstNotNullOfOrNull { item ->
            item.jsonObject.takeIf {
                it["key"]?.jsonPrimitive?.contentOrNull == OPENNOW_STREAM_SETTINGS_METADATA_KEY
            }?.get("value")?.jsonPrimitive?.contentOrNull
        }

        assertEquals(streamSettingsSessionSignature(settings), signature)
        assertFalse(sessionRequestData.containsKey("clientRequestMonitorSettings"))
        assertFalse(sessionRequestData.containsKey("requestedStreamingFeatures"))
    }

    @Test
    fun activeSessionMonitorSettingsPreferActualTopLevelMonitor() {
        val session = OpenNowJson.parseToJsonElement(
            """
            {
              "sessionRequestData": {
                "clientRequestMonitorSettings": [
                  { "widthInPixels": 1680, "heightInPixels": 720, "framesPerSecond": 60 }
                ]
              },
              "monitorSettings": [
                { "widthInPixels": 1366, "heightInPixels": 768, "framesPerSecond": 60 }
              ]
            }
            """.trimIndent(),
        ).jsonObject
        val monitor = requireNotNull(activeSessionMonitorSettings(session))

        assertEquals(1366, monitor.getValue("widthInPixels").jsonPrimitive.int)
        assertEquals(768, monitor.getValue("heightInPixels").jsonPrimitive.int)
    }

    @Test
    fun activeSessionMonitorSettingsFallsBackToTopLevelMonitor() {
        val session = OpenNowJson.parseToJsonElement(
            """
            {
              "monitorSettings": [
                { "widthInPixels": 1366, "heightInPixels": 768, "framesPerSecond": 60 }
              ]
            }
            """.trimIndent(),
        ).jsonObject
        val monitor = requireNotNull(activeSessionMonitorSettings(session))

        assertEquals(1366, monitor.getValue("widthInPixels").jsonPrimitive.int)
        assertEquals(768, monitor.getValue("heightInPixels").jsonPrimitive.int)
    }

    @Test
    fun activeSessionSettingsSignatureReadsSessionRequestMetadata() {
        val settings = StreamSettings(resolution = "1680x720", aspectRatio = "21:9", fps = 60, maxBitrateMbps = 150, codec = VideoCodec.H265)
        val signature = streamSettingsSessionSignature(settings)
        val session = OpenNowJson.parseToJsonElement(
            """
            {
              "sessionRequestData": {
                "metaData": [
                  { "key": "$OPENNOW_STREAM_SETTINGS_METADATA_KEY", "value": "$signature" }
                ]
              }
            }
            """.trimIndent(),
        ).jsonObject

        assertEquals(signature, activeSessionSettingsSignature(session))
    }
}
