package com.opencloudgaming.opennow

import android.media.MediaFormat
import android.os.Build

/** Shared by opaque HDR surfaces and the WebRTC SDR codec wrapper. */
internal enum class DecoderLatencyProfile { FULL, CORE, REALTIME }

internal fun decoderLatencyProfiles(enabled: Boolean): List<DecoderLatencyProfile> =
    if (enabled) listOf(DecoderLatencyProfile.FULL, DecoderLatencyProfile.CORE, DecoderLatencyProfile.REALTIME)
    else listOf(DecoderLatencyProfile.REALTIME)

// Limit maximum-rate requests to modern Snapdragon families. In particular SM7250 / Adreno 620
// must not get this hint. Unknown devices retain their requested FPS until tested.
internal fun supportsQualcommMaxOperatingRate(soc: String): Boolean =
    listOf("SM8450", "SM8475", "SM8550", "SM8635", "SM8650", "SM8750", "SM8850")
        .any { soc.uppercase(java.util.Locale.US).startsWith(it) }

internal fun decoderLatencyOptions(
    codecName: String,
    fps: Int,
    profile: DecoderLatencyProfile,
    maxOperatingRate: Boolean,
    soc: String,
): Map<String, Int> = buildMap {
    val qualcomm = isQualcommMediaCodecDecoder(codecName)
    put("priority", 0)
    put("operating-rate", if (profile == DecoderLatencyProfile.FULL && maxOperatingRate &&
        (!qualcomm || supportsQualcommMaxOperatingRate(soc))) 32767 else fps.coerceAtLeast(1))
    if (profile == DecoderLatencyProfile.REALTIME) return@buildMap
    put("low-latency", 1)
    put("vdec-lowlatency", 1)
    if (qualcomm) {
        put("vendor.qti-ext-dec-low-latency.enable", 1)
        if (profile == DecoderLatencyProfile.FULL) {
            put("vendor.qti-ext-dec-picture-order.enable", 1)
            put("vendor.qti-ext-dec-instant-decode.enable", 1)
            put("vendor.qti-ext-output-sw-fence-enable.value", 1)
            put("vendor.qti-ext-output-fence.enable", 1)
            put("vendor.qti-ext-output-fence.fence_type", 1)
            put("vendor.qti-ext-dec-info-misr.disable", 1)
            put("vendor.qti-ext-dec-error-correction.conceal", 1)
        }
    }
}

internal fun applyDecoderLatencyProfile(format: MediaFormat, name: String, fps: Int,
    profile: DecoderLatencyProfile, maxOperatingRate: Boolean = DECODER_MAX_OPERATING_RATE) {
    val soc = if (Build.VERSION.SDK_INT >= 31) Build.SOC_MODEL else ""
    decoderLatencyOptions(name, fps, profile, maxOperatingRate, soc).forEach { (key, value) ->
        format.setInteger(key, value)
    }
}

// Kept separate from the vendor profile so its benefit can be measured independently.
internal const val DECODER_MAX_OPERATING_RATE = false

/** Each attempt must construct a fresh format and release/reset a rejected codec. */
internal fun <T> tryDecoderLatencyProfiles(profiles: List<DecoderLatencyProfile>,
    rejected: (DecoderLatencyProfile, Exception) -> Unit,
    attempt: (DecoderLatencyProfile) -> T): T {
    require(profiles.isNotEmpty())
    var last: Exception? = null
    for (profile in profiles) {
        try { return attempt(profile) }
        catch (error: Exception) { last = error; rejected(profile, error) }
    }
    throw requireNotNull(last)
}
