package com.opencloudgaming.opennow

import kotlinx.serialization.json.*
import kotlin.math.roundToInt

/** Wire definitions, independently implemented from observed Sensa HID protocol facts. */
internal object KishiSensaProtocol {
    const val STREAM = 0x0e
    const val MAX_METADATA = 4096

    fun report(command: Int, body: ByteArray): ByteArray {
        require(command in 0..255 && body.size in 1..58)
        val report = ByteArray(64)
        report[0] = 2
        report[1] = (body.size + 4).toByte()
        report[3] = 1
        report[4] = command.toByte()
        body.copyInto(report, destinationOffset = 5)
        return report
    }

    fun reply(report: ByteArray, received: ByteArray): ByteArray? {
        require(report.size == 64)
        require(received.size >= 6 && received[0] == 1.toByte() && received[2] == 0.toByte() && received[3] == 1.toByte()) { "Invalid Sensa reply header" }
        val length = received[1].toInt() and 255
        require(length >= 5 && length < received.size) { "Invalid Sensa reply length" }
        if (received[4] != report[4]) return null
        return received.copyOfRange(5, length + 1)
    }

    fun validateMetadata(bytes: ByteArray) {
        val root = OpenNowJson.parseToJsonElement(bytes.toString(Charsets.UTF_8).trimEnd('\u0000')).jsonObject
        fun JsonObject.number(key: String): Int = getValue(key).jsonPrimitive.int
        require(root.number("StreamBodyType") == 1) { "Unsupported Sensa stream body type" }
        val bodies = root.getValue("Bodypart").jsonArray
        require(bodies.size == 2) { "Expected two Sensa actuators" }
        for ((index, entry) in bodies.withIndex()) {
            val body = entry.jsonObject
            require(body.number("BodypartID") == if (index == 0) 216 else 116) { "Unsupported actuator order" }
            val stream = body.getValue("StreamCharacteristics").jsonObject
            require(stream.number("Bands") == 3 && stream.number("Points") == 4 && stream.number("Transients") == 2) { "Unsupported Sensa envelope" }
            val values = body.getValue("Characteristics").jsonObject.getValue("ValueReport").jsonArray
            require(values.size == 1 && values[0].jsonObject.number("FrequencyMin") == 30 && values[0].jsonObject.number("FrequencyMax") == 400) { "Unsupported Sensa frequency range" }
        }
    }
}

/** One tone per actuator: strong motor on left, weak motor on right; 10 ms frames. */
internal class KishiSensaRumble {
    private val previous = IntArray(2)

    fun frame(strong: Int, weak: Int, strengthPercent: Int): ByteArray {
        val bits = ArrayList<Int>(117)
        fun field(value: Int, width: Int) {
            for (shift in width - 1 downTo 0) bits.add((value ushr shift) and 1)
        }
        field(40, 7) // duration in quarter milliseconds
        val motors = intArrayOf(strong, weak)
        for (channel in 0..1) {
            val target = (motors[channel].coerceIn(0, 65535) / 65535.0 * strengthPercent.coerceIn(0, 100) / 100.0 * 63).roundToInt()
            val start = if (target == 0) 0 else previous[channel]
            previous[channel] = target
            field(1, 1) // one frequency band, four envelope points
            for (point in 1..4) {
                field((start + (target - start) * point / 4.0).roundToInt(), 6)
                field(((if (channel == 0) 100 else 200) - 30) * 127 / 370, 7)
            }
            field(0, 1) // no additional bands
            field(0, 1) // no transients
        }
        val payload = ByteArray((bits.size + 7) / 8)
        bits.forEachIndexed { index, bit ->
            payload[index / 8] = (payload[index / 8].toInt() or (bit shl (7 - index % 8))).toByte()
        }
        return KishiSensaProtocol.report(KishiSensaProtocol.STREAM, payload)
    }
}

internal data class KishiSensaDescriptor(
    val vendor: Int, val product: Int, val interfaceId: Int, val alternate: Int,
    val interfaceClass: Int, val endpoints: List<Triple<Int, Int, Int>>,
    val productName: String? = null,
) {
    fun supported(): Boolean = kishiSensaIdentity(vendor, product, productName) &&
        interfaceId == 4 && alternate == 0 && interfaceClass == 3 &&
        endpoints.sortedBy { it.first } == listOf(Triple(4, 3, 64), Triple(0x84, 3, 64))
}

// 0037 is shared by Razer XInput devices. Require the observed USB product name as well as
// the dedicated interface; firmware metadata is still validated before enabling output.
internal fun kishiSensaIdentity(vendor: Int, product: Int, productName: String?): Boolean =
    vendor == 0x1532 && (product in setOf(0x0724, 0x0727) ||
        (product == 0x0037 && productName == "Razer Kishi V3 Pro"))

internal fun kishiOwnsController(inputDeviceId: Int?, slots: Map<Int, Int>, controllerId: Int): Boolean =
    inputDeviceId != null && slots[inputDeviceId] == controllerId
