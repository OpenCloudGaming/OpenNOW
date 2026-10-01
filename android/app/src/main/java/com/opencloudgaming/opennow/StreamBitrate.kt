package com.opencloudgaming.opennow

import kotlinx.serialization.KSerializer
import kotlinx.serialization.SerializationException
import kotlinx.serialization.descriptors.PrimitiveKind
import kotlinx.serialization.descriptors.PrimitiveSerialDescriptor
import kotlinx.serialization.encoding.Decoder
import kotlinx.serialization.encoding.Encoder
import kotlinx.serialization.json.JsonDecoder
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.doubleOrNull
import kotlin.math.roundToInt

internal object StreamBitrate {
    const val DEFAULT_MBPS = 75.0
    const val MIN_KBPS = 220
    const val MIN_MBPS = MIN_KBPS / 1_000.0
    const val SETTINGS_MAX_MBPS = 150.0
    const val TRANSPORT_MAX_KBPS = 200_000
    const val TRANSPORT_MAX_MBPS = TRANSPORT_MAX_KBPS / 1_000.0

    fun normalizedMbps(value: Double, maximum: Double = SETTINGS_MAX_MBPS): Double =
        (if (value.isFinite()) value else DEFAULT_MBPS).coerceIn(MIN_MBPS, maximum)

    fun maximumKbps(value: Double): Int =
        (normalizedMbps(value, TRANSPORT_MAX_MBPS) * 1_000).roundToInt()

    fun jsonMbps(value: Double): JsonPrimitive {
        val normalized = normalizedMbps(value, TRANSPORT_MAX_MBPS)
        return if (normalized % 1.0 == 0.0) JsonPrimitive(normalized.toInt()) else JsonPrimitive(normalized)
    }

    fun formatMbps(value: Double): String = jsonMbps(value).content

    fun formatSliderMbps(value: Float): String = "${value.toString().removeSuffix(".0")} Mbps"

    fun sliderChangeMbps(current: Double, selected: Float): Double? =
        if (selected == current.toFloat()) null else selected.roundToInt().toDouble()
}

internal object StreamBitrateMbpsSerializer : KSerializer<Double> {
    override val descriptor = PrimitiveSerialDescriptor("StreamBitrateMbps", PrimitiveKind.DOUBLE)

    override fun deserialize(decoder: Decoder): Double {
        val value = if (decoder is JsonDecoder) {
            (decoder.decodeJsonElement() as? JsonPrimitive)?.doubleOrNull
                ?: throw SerializationException("Expected numeric maxBitrateMbps")
        } else {
            decoder.decodeDouble()
        }
        return StreamBitrate.normalizedMbps(value, StreamBitrate.TRANSPORT_MAX_MBPS)
    }

    override fun serialize(encoder: Encoder, value: Double) {
        val normalized = StreamBitrate.normalizedMbps(value, StreamBitrate.TRANSPORT_MAX_MBPS)
        if (normalized % 1.0 == 0.0) encoder.encodeInt(normalized.toInt()) else encoder.encodeDouble(normalized)
    }
}
