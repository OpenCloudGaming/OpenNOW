package com.opencloudgaming.opennow

internal fun StreamSettings.shouldWarnAboutHighSettings(): Boolean {
    val (width, height) = streamResolutionPixels(this)
    return width.toLong() * height >= 3840L * 2160 || fps > 120 || maxBitrateMbps > 100
}
