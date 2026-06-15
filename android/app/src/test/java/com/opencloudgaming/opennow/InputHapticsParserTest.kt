package com.opencloudgaming.opennow

import java.nio.ByteBuffer
import java.nio.ByteOrder
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class InputHapticsParserTest {
    @Test
    fun parsesLegacyHapticPacket() {
        val packet = ByteBuffer.allocate(12).order(ByteOrder.LITTLE_ENDIAN).apply {
            putShort(267.toShort())
            putShort(1.toShort())
            putShort(6.toShort())
            putShort(2.toShort())
            putShort(0x4000.toShort())
            putShort(0x7fff.toShort())
        }.array()

        val command = HapticsPacketParser.parse(packet)

        assertEquals(2, command?.controllerId)
        assertEquals(0x4000, command?.weakMagnitude)
        assertEquals(0x7fff, command?.strongMagnitude)
    }

    @Test
    fun parsesWrappedOcHapticPacket() {
        val packet = ByteArray(14)
        packet[0] = 34
        ByteBuffer.wrap(packet).order(ByteOrder.LITTLE_ENDIAN).putInt(1, 17)
        packet[5] = 7
        packet[8] = 5
        packet[9] = 1
        packet[12] = 0x20
        packet[13] = 0x60

        val command = HapticsPacketParser.parse(packet)

        assertEquals(1, command?.controllerId)
        assertEquals(0x2000, command?.weakMagnitude)
        assertEquals(0x6000, command?.strongMagnitude)
    }

    @Test
    fun ignoresHandshakeAndInputWrappers() {
        assertNull(HapticsPacketParser.parse(byteArrayOf(0x0e, 0x02, 0x03, 0x00)))
        assertNull(HapticsPacketParser.parse(byteArrayOf(33, 0, 0, 0)))
    }
}
