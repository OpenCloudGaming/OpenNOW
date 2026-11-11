package com.opencloudgaming.opennow

import android.os.Build

/** Shared by catalog posters, TV banners, hero artwork, and screenshot prefetch/display. */
internal fun optimizedNvidiaImageUrl(
    url: String,
    width: Int,
    androidSdk: Int = Build.VERSION.SDK_INT,
): String {
    if (!url.contains("img.nvidiagrid.net")) return url
    val base = url
        .substringBefore(";f=")
        .substringBefore(";w=")
        .substringBefore(";h=")
        .substringBefore(";dpr=")
    // The build-179 native crash enters the legacy SkWebpCodec/EmitFancyRGB path. Request JPEG
    // artwork on pre-Android-9 devices instead of forcing their system WebP decoder. A distinct
    // URL also prevents Coil from reusing previously cached WebP bytes for these requests.
    val format = if (androidSdk < 28) "jpg" else "webp"
    return "$base;f=$format;w=$width"
}
