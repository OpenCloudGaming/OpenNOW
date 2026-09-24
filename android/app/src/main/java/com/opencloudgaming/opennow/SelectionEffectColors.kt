package com.opencloudgaming.opennow

import kotlinx.serialization.Serializable
import java.util.Locale

/** Null in AppSettings preserves the existing theme palette; a saved pair is theme-independent. */
@Serializable
data class SelectionEffectColors(val firstRgb: Int, val secondRgb: Int) {
    fun normalized() = copy(firstRgb = firstRgb and 0xFFFFFF, secondRgb = secondRgb and 0xFFFFFF)
}

internal fun parseEffectColor(value: String): Int? {
    val hex = value.trim().removePrefix("#")
    return hex.takeIf { it.length == 6 && it.all { digit -> digit in '0'..'9' || digit.lowercaseChar() in 'a'..'f' } }?.toIntOrNull(16)
}

internal fun effectColorHex(rgb: Int): String = "#%06X".format(Locale.ROOT, rgb and 0xFFFFFF)
