package com.opencloudgaming.opennow

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertEquals
import kotlinx.serialization.json.*
import org.junit.Test

class DiagnosticsSanitizationTest {
    @Test
    fun timestampsSurviveWhileBothIpv6FormsAreRedacted() {
        val raw = "2026-09-13T08:37:17.123Z old=8:37:17 AM ipv6=2001:0db8:0000:0000:0000:0000:0000:1234 compressed=2001:db8::1234"
        val sanitized = sanitizeDiagnosticExport(raw)
        assertTrue(sanitized.contains("2026-09-13T08:37:17.123Z"))
        assertTrue(sanitized.contains("8:37:17 AM"))
        assertFalse(sanitized.contains("2001:"))
        assertEquals("1970-01-01T00:00:01.234Z", diagnosticTimestamp(1234))
        assertEquals("1970-01-01T00:00:09.000Z uptimeMs=4000", DiagnosticTimeAnchor(10000, 5000).formatElapsed(4000))
    }

    @Test
    fun parserBlockRemainsValidJsonAfterRedactingNestedDataAndTruncatedPreviews() {
        val data = buildJsonObject {
            put("schemaVersion", 2)
            put("capturedAt", "2026-09-13T08:37:17.123Z")
            put("device", buildJsonObject { put("model", "SM-A245F"); put("androidSdk", 36) })
            put("api", buildJsonArray { add(buildJsonObject {
                put("password", "secret-value")
                put("sessionId", "private-session")
                put("statusCode", 89)
                put("preview", "{\"accessToken\":\"private-token\",\"deviceId\":\"private-device\",\"password\":\"private-password\"}")
                put("message", "literal </parser> text and ipv6=2001:db8::1234")
            }) })
        }
        val sanitized = sanitizeDiagnosticExport("Report\n" + diagnosticParserBlock(data))
        val parsed = OpenNowJson.parseToJsonElement(sanitized.substringAfter("<parser>\n").substringBefore("\n</parser>")).jsonObject
        assertEquals(2, parsed.getValue("schemaVersion").jsonPrimitive.int)
        assertEquals("SM-A245F", parsed.getValue("device").jsonObject.getValue("model").jsonPrimitive.content)
        assertEquals(89, parsed.getValue("api").jsonArray[0].jsonObject.getValue("statusCode").jsonPrimitive.int)
        assertTrue(sanitized.contains("2026-09-13T08:37:17.123Z"))
        for (secret in listOf("secret-value", "private-session", "private-token", "private-device", "private-password", "2001:db8")) {
            assertFalse("Leaked $secret", sanitized.contains(secret))
        }
        assertEquals(sanitized, sanitizeDiagnosticExport(sanitized))
    }

    @Test
    fun exportKeepsNonUniqueDeviceAndAndroidSupportContext() {
        val raw = """
            device.identity manufacturer=NVIDIA brand=NVIDIA model=SHIELD_Android_TV codename=mdarcy product=mdarcy formFactor=tv emulator=false
            android.os release=11 codename=REL sdk=30 targetSdk=36 securityPatch=2025-04-05
            device.hardware hardware=darcy board=darcy abis=arm64-v8a|armeabi-v7a runtimeBits=64 processors=8 memoryMiB=3072 lowRam=false
            device.display pixels=3840x2160 densityDpi=320 smallestWidthDp=960
        """.trimIndent()

        val sanitized = sanitizeDiagnosticExport(raw)

        assertTrue(sanitized.contains("model=SHIELD_Android_TV"))
        assertTrue(sanitized.contains("sdk=30"))
        assertTrue(sanitized.contains("securityPatch=2025-04-05"))
        assertTrue(sanitized.contains("abis=arm64-v8a|armeabi-v7a"))
        assertTrue(sanitized.contains("pixels=3840x2160"))
    }

    @Test
    fun exportRemovesNamesTokensIdsAndNetworkAddresses() {
        val raw = """
            user=Jane Example tier=FREE provider=NVIDIA
            sessionId=01234567-89ab-4cde-8fab-0123456789ab sessionStatus=READY serverIp=192.168.10.42
            Authorization: Bearer secret.jwt.value
            {"displayName":"Jane Example","email":"jane@example.com","deviceId":"device-secret"}
            ipv6=2001:db8::1234
        """.trimIndent()

        val sanitized = sanitizeDiagnosticExport(raw)

        listOf(
            "Jane Example",
            "jane@example.com",
            "secret.jwt.value",
            "01234567-89ab-4cde-8fab-0123456789ab",
            "192.168.10.42",
            "2001:db8::1234",
            "device-secret",
        ).forEach { sensitive -> assertFalse("Leaked $sensitive in $sanitized", sanitized.contains(sensitive)) }
        assertTrue(sanitized.contains("tier=FREE"))
        assertTrue(sanitized.contains("provider=NVIDIA"))
        assertTrue(sanitized.contains("[redacted]"))
    }
}
