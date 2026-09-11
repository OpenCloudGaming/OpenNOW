package com.opencloudgaming.opennow

import java.nio.ByteBuffer
import java.nio.ByteOrder

/** ServerControl uses little-endian type/length records; never scan arbitrary bytes for rumble. */
internal object NvstHaptics {
    fun parse(bytes: ByteArray): List<GamepadRumbleCommand> {
        if (bytes.isEmpty() || bytes.size > 4096) return emptyList()
        if (bytes[0] == 0x22.toByte()) {
            // NVST registers gamepad descriptor indices 3..6; WebRTC uses 6..9.
            return listOfNotNull(HapticsPacketParser.parse(bytes, controllerBase = 3))
                .filter { it.controllerId in 0..3 }
        }
        val view = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        val commands = mutableListOf<GamepadRumbleCommand>()
        var offset = 0
        fun word(at: Int) = view.getShort(at).toInt() and 0xffff
        while (offset < bytes.size) {
            if (bytes.size - offset < 4) return emptyList()
            val type = word(offset)
            val length = word(offset + 2)
            val start = offset + 4
            if (length > bytes.size - start) return emptyList()
            if (type == 0x010b) {
                // Native rumble payload: reserved u32, controller u16, low/high frequency u16.
                if (length != 10) return emptyList()
                val controller = word(start + 4)
                if (controller !in 0..3) return emptyList()
                commands += GamepadRumbleCommand(controller, word(start + 8), word(start + 6))
            }
            offset = start + length
        }
        return commands
    }
}
