package com.opencloudgaming.opennow

import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

internal fun isNvidiaCloudMatchHost(host: String): Boolean =
    host == "cloudmatchbeta.nvidiagrid.net" ||
        host.endsWith(".cloudmatchbeta.nvidiagrid.net") ||
        host == "cloudmatch.nvidiagrid.net" ||
        host.endsWith(".cloudmatch.nvidiagrid.net")

/** A direct seat IP is not provider identity; keep its existing transport interpretation. */
internal fun isAllianceCloudMatchBase(streamingBaseUrl: String?): Boolean {
    val host = streamingBaseUrl?.toHttpUrlOrNull()?.host ?: return false
    return !isNvidiaCloudMatchHost(host) && host.contains('.') && ':' !in host &&
        host.any(Char::isLetter)
}

/** YES seats work stably with WebRTC 3.0 signaling; NVST RTSP is optional via experimentalNvst. */
internal fun requiresNativeAllianceCloudMatchTransport(streamingBaseUrl: String?): Boolean = false
