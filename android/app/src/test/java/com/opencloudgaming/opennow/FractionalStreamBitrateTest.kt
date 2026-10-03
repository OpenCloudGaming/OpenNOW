package com.opencloudgaming.opennow

import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FractionalStreamBitrateTest {
    @Test
    fun wholeBitrateSessionSignaturesKeepTheLegacyIntegerRepresentation() {
        for (mbps in listOf(1, 75, 150)) {
            val settings = StreamSettings(maxBitrateMbps = mbps.toDouble(), colorQuality = ColorQuality.EightBit420)
            assertEquals(
                "opennow-android-stream-v1;res=1920x1080;fps=60;bitrate=$mbps;codec=H264;color=EightBit420;hdr=0;l4s=0;keyboard=en-US;language=en_US",
                streamSettingsSessionSignature(settings),
            )
        }
    }

    @Test
    fun legacyWholeBitrateSessionsStillMatchAndCanBeSelectedForRecovery() {
        for (mbps in listOf(1, 75, 150)) {
            val settings = StreamSettings(maxBitrateMbps = mbps.toDouble(), colorQuality = ColorQuality.EightBit420)
            val legacySignature = "opennow-android-stream-v1;res=1920x1080;fps=60;bitrate=$mbps;codec=H264;color=EightBit420;hdr=0;l4s=0;keyboard=en-US;language=en_US"
            val legacy = ActiveSessionInfo(
                sessionId = "legacy-$mbps",
                appId = 123,
                status = 3,
                serverIp = "streamer.example",
                resolution = "1920x1080",
                fps = 60,
                settingsSignature = legacySignature,
            )
            val wrongBitrate = legacy.copy(sessionId = "wrong", settingsSignature = legacySignature.replace("bitrate=$mbps;", "bitrate=35;"))
            assertTrue(legacy.matchesStreamGeometry(settings))
            assertTrue(legacy.matchesStreamSettings(settings))
            assertFalse(wrongBitrate.matchesStreamSettings(settings))
            assertEquals(legacy, activeSessionRecoveryCandidate(listOf(wrongBitrate, legacy), "missing", 123, settings))
        }
    }

    @Test
    fun fractionalSessionSignaturesRemainDistinctAndMatchOnlyTheConfiguredRate() {
        val signatures = listOf(0.22, 0.8, 1.0).map { mbps ->
            val settings = StreamSettings(maxBitrateMbps = mbps)
            val signature = streamSettingsSessionSignature(settings)
            val active = ActiveSessionInfo(
                sessionId = "fractional-$mbps", appId = 123, status = 3, serverIp = "streamer.example",
                resolution = "1920x1080", fps = 60, settingsSignature = signature,
            )
            assertTrue(signature.contains(";bitrate=${StreamBitrate.formatMbps(mbps)};"))
            assertTrue(active.matchesStreamSettings(settings))
            assertFalse(active.matchesStreamSettings(settings.copy(maxBitrateMbps = 75.0)))
            assertEquals(active, activeSessionRecoveryCandidate(listOf(active), "missing", 123, settings))
            signature
        }
        assertEquals(3, signatures.toSet().size)
    }

    @Test
    fun wholeNumberProfilesRemainIntegerEncodedAndOldIntegersDecodeIdentically() {
        for (mbps in 1..200) {
            val settings = OpenNowJson.decodeFromString<StreamSettings>("""{"maxBitrateMbps":$mbps}""")
            assertEquals(mbps.toDouble(), settings.maxBitrateMbps, 0.0)
            val encoded = OpenNowJson.parseToJsonElement(OpenNowJson.encodeToString(settings)).jsonObject
            assertEquals(mbps.toString(), encoded["maxBitrateMbps"]!!.jsonPrimitive.content)
            assertFalse(encoded["maxBitrateMbps"]!!.jsonPrimitive.isString)
        }
        assertEquals(75.0, loadSettingsWithNvstDefault(null).stream.maxBitrateMbps, 0.0)
        assertEquals(75.0, loadSettingsWithNvstDefault("{}").stream.maxBitrateMbps, 0.0)
        assertEquals("75", StreamBitrate.jsonMbps(StreamSettings().maxBitrateMbps).content)
    }

    @Test
    fun numericBoundariesNormalizeOnlyTheBitrate() {
        for ((token, expected) in listOf(
            "0" to 0.22,
            "-1" to 0.22,
            "0.2199" to 0.22,
            "1e100" to 200.0,
            "1e999" to 75.0,
            "NaN" to 75.0,
            "Infinity" to 75.0,
            "-Infinity" to 75.0,
        )) {
            val raw = """{"stream":{"maxBitrateMbps":$token,"fps":120},"favoriteGameIds":["keep-me"],"nvstOptInVersion":1}"""
            var writes = 0
            val loaded = loadSettingsWithNvstDefault(raw) { writes++ }
            assertEquals(token, expected, loaded.stream.maxBitrateMbps, 0.0)
            assertEquals(120, loaded.stream.fps)
            assertEquals(listOf("keep-me"), loaded.favoriteGameIds)
            assertEquals(0, writes)
        }
        for (invalid in listOf("null", "true", "{}", "\"invalid\"")) {
            val raw = """{"stream":{"maxBitrateMbps":$invalid},"nvstOptInVersion":1}"""
            assertEquals(75.0, loadSettingsWithNvstDefault(raw).stream.maxBitrateMbps, 0.0)
        }
    }

    @Test
    fun finiteNormalizationAndWireRoundingKeepExistingBounds() {
        for (value in listOf(Double.NaN, Double.POSITIVE_INFINITY, Double.NEGATIVE_INFINITY)) {
            assertEquals(75.0, StreamBitrate.normalizedMbps(value), 0.0)
            assertEquals(75000, StreamBitrate.maximumKbps(value))
            assertEquals("75", StreamBitrate.jsonMbps(value).content)
            val encoded = OpenNowJson.parseToJsonElement(OpenNowJson.encodeToString(StreamSettings(maxBitrateMbps = value))).jsonObject
            assertEquals("75", encoded["maxBitrateMbps"]!!.jsonPrimitive.content)
        }
        for ((value, kbps) in listOf(-1.0 to 220, 0.0 to 220, 0.22 to 220, 0.8 to 800, 0.8005 to 801, 75.0 to 75000, 200.0 to 200000, Double.MAX_VALUE to 200000)) {
            assertEquals(kbps, StreamBitrate.maximumKbps(value))
        }
        assertEquals(150.0, StreamBitrate.normalizedMbps(200.0), 0.0)
        assertEquals(150.0, StreamBitrate.normalizedMbps(Double.MAX_VALUE), 0.0)
    }

    @Test
    fun liveCeilingsRetainFractionalSettingsForTheNextOffer() {
        for (kbps in listOf(220, 800, 1000, 1499, 1500, 2200, 75000, 150000, 200000)) {
            val normalized = normalizedLiveBitrateKbps(kbps)
            assertEquals(kbps, normalized)
            val settings = StreamSettings().copy(maxBitrateMbps = normalized / 1000.0)
            assertEquals(kbps, StreamNetworkAdaptation.bitrateRange(settings.maxBitrateMbps).maximumKbps)
            assertEquals(settings.maxBitrateMbps, settings.toActiveStreamTransportProfile().maxBitrateMbps, 0.0)
        }
        assertEquals(220, normalizedLiveBitrateKbps(Int.MIN_VALUE))
        assertEquals(200000, normalizedLiveBitrateKbps(Int.MAX_VALUE))
    }

    @Test
    fun customProfilesAndSliderReadoutsKeepManualFractionalValues() {
        for (mbps in listOf(0.22, 0.8, 75.0)) {
            val settings = StreamSettings(maxBitrateMbps = mbps)
            assertEquals(settings, settings.applyingStreamPreset(StreamPreset.Custom))
            assertEquals(mbps, settings.loweredSessionLaunchProfile().maxBitrateMbps, 0.0)
            assertEquals("${StreamBitrate.formatMbps(mbps)} Mbps", StreamBitrate.formatSliderMbps(mbps.toFloat()))
        }
    }

    @Test
    fun unchangedSliderCallbacksDoNotRoundOrRewriteFractionalProfiles() {
        for (mbps in listOf(0.22, 0.8, 0.8005, 1.0, 75.0, 150.0)) {
            assertNull(StreamBitrate.sliderChangeMbps(mbps, mbps.toFloat()))
        }
        for (mbps in listOf(1, 2, 75, 150)) {
            assertEquals(mbps.toDouble(), StreamBitrate.sliderChangeMbps(0.8, mbps.toFloat())!!, 0.0)
        }
    }

    @Test
    fun manualFractionalProfilesSurviveLoadingWithoutRewritingOtherPreferences() {
        for (token in listOf("0.8", "0.22", "1.0", "75")) {
            val raw = """{"stream":{"maxBitrateMbps":$token,"fps":120},"favoriteGameIds":["keep-me"],"nvstOptInVersion":1}"""
            var writes = 0
            val loaded = loadSettingsWithNvstDefault(raw) { writes++ }
            assertEquals(token.toDouble(), loaded.stream.maxBitrateMbps.toDouble(), 0.0)
            assertEquals(120, loaded.stream.fps)
            assertEquals(listOf("keep-me"), loaded.favoriteGameIds)
            assertEquals(0, writes)
        }
    }

    @Test
    fun migrationWriterRetainsFractionalProfilesAndOtherPreferences() {
        for (token in listOf("0.8", "0.22", "75")) {
            val raw = """{"stream":{"maxBitrateMbps":$token,"fps":120,"experimentalNvst":true},"favoriteGameIds":["keep-me"]}"""
            var disk = raw
            var writes = 0
            val loaded = loadSettingsWithNvstDefault(disk) {
                writes++
                disk = OpenNowJson.encodeToString(it)
            }
            assertEquals(token.toDouble(), loaded.stream.maxBitrateMbps.toDouble(), 0.0)
            assertEquals(120, loaded.stream.fps)
            assertEquals(listOf("keep-me"), loaded.favoriteGameIds)
            assertEquals(1, writes)
            assertEquals(loaded, loadSettingsWithNvstDefault(disk) { writes++ })
            assertEquals(1, writes)
            val encoded = OpenNowJson.parseToJsonElement(disk).jsonObject["stream"]!!.jsonObject
            assertEquals(token, encoded["maxBitrateMbps"]!!.jsonPrimitive.content)
        }
    }

    @Test
    fun fractionalProfilesReachSdpAndNativeContextWithExactKbps() {
        for ((token, kbps) in listOf("0.8" to 800, "0.22" to 220)) {
            val settings = OpenNowJson.decodeFromString<StreamSettings>("""{"maxBitrateMbps":$token}""")
            val range = StreamNetworkAdaptation.bitrateRange(settings.maxBitrateMbps)
            assertEquals(StreamBitrateRange(kbps, kbps, kbps), range)
            val sdp = SdpTools.buildNvstSdp("", settings, "")
            assertTrue(sdp.contains("a=vqos.bw.maximumBitrateKbps:$kbps"))
            assertTrue(sdp.contains("a=vqos.bw.minimumBitrateKbps:$kbps"))
            assertTrue(sdp.contains("a=video.initialBitrateKbps:$kbps"))
            val session = SessionInfo("probe", 3, serverIp = "seat.invalid", signalingServer = "", signalingUrl = "")
            val context = OpenNowJson.parseToJsonElement(nvstSessionContext(session, settings)).jsonObject
            val encoded = context["settings"]!!.jsonObject
            assertEquals(token, encoded["maxBitrateMbps"]!!.jsonPrimitive.content)
            assertEquals(kbps.toString(), encoded["networkAdaptation"]!!.jsonObject["initialBitrateKbps"]!!.jsonPrimitive.content)
        }
    }
}
