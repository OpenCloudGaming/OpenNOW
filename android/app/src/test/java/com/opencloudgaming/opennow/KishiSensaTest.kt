package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class KishiSensaTest {
    private fun descriptor() = KishiSensaDescriptor(0x1532, 0x0724, 4, 0, 3,
        listOf(Triple(0x84, 3, 64), Triple(4, 3, 64)))

    @Test fun matchesObservedProAndXlButRejectsGuessedInterfaces() {
        assertTrue(descriptor().supported())
        assertTrue(descriptor().copy(product = 0x0727).supported())
        assertFalse(descriptor().copy(product = 0x0037).supported())
        assertTrue(descriptor().copy(product = 0x0037, productName = "Razer Kishi V3 Pro").supported())
        assertFalse(descriptor().copy(product = 0x0037, productName = "Razer Wolverine").supported())
        assertFalse(descriptor().copy(product = 0x0037, productName = "Razer Kishi V3 Pro", interfaceId = 3).supported())
        assertFalse(descriptor().copy(interfaceId = 3).supported())
        assertFalse(descriptor().copy(alternate = 1).supported())
        assertFalse(descriptor().copy(endpoints = listOf(Triple(4, 3, 16))).supported())
        assertFalse(descriptor().copy(endpoints = listOf(Triple(4, 2, 64), Triple(0x84, 3, 64))).supported())
    }

    private fun amplitudes(report: ByteArray): List<List<Int>> {
        var bit = 40 // payload starts at byte 5
        fun read(width: Int): Int {
            var value = 0
            repeat(width) {
                value = value * 2 + ((report[bit / 8].toInt() ushr (7 - bit % 8)) and 1)
                bit++
            }
            return value
        }
        assertEquals(40, read(7))
        return (0..1).map { channel ->
            assertEquals(1, read(1))
            val a = (0..3).map { val amplitude = read(6); assertEquals(if (channel == 0) 24 else 58, read(7)); amplitude }
            assertEquals(0, read(1)); assertEquals(0, read(1))
            a
        }
    }

    @Test fun isolatesStrongAndWeakAndStopsImmediatelyWithoutRamp() {
        val encoder = KishiSensaRumble()
        val left = amplitudes(encoder.frame(65535, 0, 100))
        assertEquals(listOf(16, 32, 47, 63), left[0]); assertEquals(listOf(0, 0, 0, 0), left[1])
        val right = amplitudes(encoder.frame(0, 65535, 100))
        assertEquals(listOf(0, 0, 0, 0), right[0]); assertEquals(63, right[1].last())
        assertTrue(amplitudes(encoder.frame(0, 0, 100)).flatten().all { it == 0 })
    }

    @Test fun clampsMagnitudeAndStrengthAndBoundsReport() {
        val r = KishiSensaRumble().frame(Int.MAX_VALUE, -1, 200)
        assertEquals(64, r.size); assertEquals(2, r[0].toInt()); assertEquals(19, r[1].toInt())
        assertEquals(14, r[4].toInt()); assertEquals(63, amplitudes(r)[0].last())
        assertTrue(r.drop(20).all { it == 0.toByte() })
        assertTrue(amplitudes(KishiSensaRumble().frame(65535, 65535, -1)).flatten().all { it == 0 })
    }

    @Test fun replyValidatesFramingLengthAndCommand() {
        val report = KishiSensaProtocol.report(0x87, byteArrayOf(0))
        val reply = byteArrayOf(1, 5, 0, 1, 0x87.toByte(), 2)
        assertArrayEquals(byteArrayOf(2), KishiSensaProtocol.reply(report, reply))
        assertNull(KishiSensaProtocol.reply(report, reply.copyOf().apply { this[4] = 7 }))
        for (bad in listOf(reply.copyOf(5), reply.copyOf().apply { this[1] = 64 }, reply.copyOf().apply { this[2] = 1 })) {
            assertThrows(IllegalArgumentException::class.java) { KishiSensaProtocol.reply(report, bad) }
        }
    }

    private val metadata = """{"StreamBodyType":1,"Bodypart":[
        {"BodypartID":216,"StreamCharacteristics":{"Bands":3,"Points":4,"Transients":2},"Characteristics":{"ValueReport":[{"FrequencyMin":30,"FrequencyMax":400}]}},
        {"BodypartID":116,"StreamCharacteristics":{"Bands":3,"Points":4,"Transients":2},"Characteristics":{"ValueReport":[{"FrequencyMin":30,"FrequencyMax":400}]}}
    ]}"""

    @Test fun validatesActuatorLayoutBeforeChangingMode() {
        KishiSensaProtocol.validateMetadata(metadata.toByteArray())
        for (bad in listOf(metadata.replace("216", "116"), metadata.replace("\"Bands\":3", "\"Bands\":4"), metadata.replace("400", "500"))) {
            assertThrows(IllegalArgumentException::class.java) { KishiSensaProtocol.validateMetadata(bad.toByteArray()) }
        }
    }

    @Test fun controllerRoutingRejectsMissingOrWrongSlot() {
        assertTrue(kishiOwnsController(38, mapOf(38 to 0, 12 to 1), 0))
        assertFalse(kishiOwnsController(38, mapOf(38 to 0, 12 to 1), 1))
        assertFalse(kishiOwnsController(null, mapOf(38 to 0), 0))
        assertFalse(kishiOwnsController(38, emptyMap(), 0))
    }
}
