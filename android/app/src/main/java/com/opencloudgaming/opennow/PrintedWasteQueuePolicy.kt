package com.opencloudgaming.opennow

import java.net.URI

internal fun shouldUsePrintedWasteQueue(
    hideServerSelector: Boolean,
    providerCode: String,
    providerStreamingServiceUrl: String,
    effectiveStreamingBaseUrl: String,
    membershipTier: String?,
): Boolean {
    if (hideServerSelector) return false
    if (!providerCode.equals("NVIDIA", ignoreCase = true)) return false
    if (!membershipTier.isNullOrBlank() && !membershipTier.equals("FREE", ignoreCase = true)) return false
    return !isAllianceStreamingServiceUrl(providerStreamingServiceUrl) &&
        !isAllianceStreamingServiceUrl(effectiveStreamingBaseUrl)
}

private fun isAllianceStreamingServiceUrl(streamingServiceUrl: String): Boolean {
    val host = runCatching { URI(streamingServiceUrl).host.orEmpty() }.getOrDefault("")
    return host.isNotBlank() && !host.endsWith(".nvidiagrid.net", ignoreCase = true)
}
