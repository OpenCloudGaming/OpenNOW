package com.opencloudgaming.opennow

import org.junit.Assert.*
import org.junit.Test

class NvstHapticsTest {
    private fun hex(value: String) = value.chunked(2).map { it.toInt(16).toByte() }.toByteArray()
    private val rumble = hex("0b010a000000000002003412cdab")

    @Test fun nativeLowFrequencyMapsToStrongMotorAndZeroStopsBoth() {
        assertEquals(listOf(GamepadRumbleCommand(2, 0xabcd, 0x1234)), NvstHaptics.parse(rumble))
        assertEquals(listOf(GamepadRumbleCommand(2, 0, 0)), NvstHaptics.parse(hex("0b010a0000000000020000000000")))
    }

    @Test fun completeBatchesSkipUnknownCommandsWithoutScanningTheirPayload() {
        val unknown = hex("ee010e00") + rumble
        assertTrue(NvstHaptics.parse(unknown).isEmpty())
        assertEquals(2, NvstHaptics.parse(unknown + rumble + rumble).size)
        assertTrue(NvstHaptics.parse(rumble + byteArrayOf(0)).isEmpty())
    }

    @Test fun malformedLengthsAndControllerIdsFailClosed() {
        for (length in 0 until rumble.size) assertTrue(NvstHaptics.parse(rumble.copyOf(length)).isEmpty())
        assertTrue(NvstHaptics.parse(rumble.copyOf().also { it[2] = 9 }).isEmpty())
        assertTrue(NvstHaptics.parse(rumble.copyOf().also { it[8] = 4 }).isEmpty())
        assertTrue(NvstHaptics.parse(ByteArray(4097)).isEmpty())
    }

    @Test fun ocReportsUseNativeDescriptorIndicesWithoutChangingWebRtcMapping() {
        for (slot in 0..3) {
            val report = hex("22110000000300000501000012ab").also { it[5] = (slot + 3).toByte() }
            assertEquals(listOf(GamepadRumbleCommand(slot, 0x1200, 0xab00)), NvstHaptics.parse(report))
            report[5] = (slot + 6).toByte()
            assertEquals(GamepadRumbleCommand(slot, 0x1200, 0xab00), HapticsPacketParser.parse(report))
        }
    }
}
