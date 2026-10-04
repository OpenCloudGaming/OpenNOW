package com.opencloudgaming.opennow

import java.util.Locale
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull

private val SESSION_ZONE_ID = Regex("^npa?-[a-z0-9]+(?:-[a-z0-9]+)+$")
private val STANDARD_SESSION_CONTROL_HOST = Regex("^np-[a-z0-9]+(?:-[a-z0-9]+)+\\.(?:cloudmatchbeta|cloudmatch)\\.nvidiagrid\\.net$")

/** Only standard CloudMatch zone hosts on its HTTPS port may become session poll targets. */
internal fun standardCloudMatchSessionControlBaseUrl(rawHost: String?, port: Int?): String? {
    if (port != null && port != 443) return null
    val host = rawHost?.trim()?.trimEnd('.')?.lowercase(Locale.US) ?: return null
    return host.takeIf(STANDARD_SESSION_CONTROL_HOST::matches)?.let { "https://$it" }
}

/** Use the control host returned by this session snapshot for the next queue poll. */
internal fun SessionInfo.sessionControlPollBaseUrl(): String? {
    val launchHost = streamingBaseUrl?.toHttpUrlOrNull()?.host ?: return null
    if (!launchHost.endsWith(".cloudmatchbeta.nvidiagrid.net") &&
        !launchHost.endsWith(".cloudmatch.nvidiagrid.net")) return null
    val controlBase = sessionControlBaseUrl ?: return null
    val controlUrl = controlBase.toHttpUrlOrNull() ?: return null
    if (controlUrl.scheme != "https" || controlUrl.port != 443 || !STANDARD_SESSION_CONTROL_HOST.matches(controlUrl.host)) return null
    return "https://${controlUrl.host}"
}

/**
 * Returns the zone that owns an allocated session, when CloudMatch exposes it in the session
 * control hostname. This is deliberately separate from requestStatus.serverId: that field can
 * continue to identify the zone that handled the request even when Free Tier assigns the rig in
 * another zone.
 */
internal fun assignedSessionZoneFromControlHost(rawHost: String?): String? {
    val host = rawHost
        ?.trim()
        ?.trimEnd('.')
        ?.lowercase(Locale.US)
        ?.takeIf { it.isNotBlank() }
        ?: return null
    val isNvidiaSessionHost = host.endsWith(".cloudmatchbeta.nvidiagrid.net") ||
        host.endsWith(".cloudmatch.nvidiagrid.net") ||
        host.endsWith(".geforcenow.nvidiagrid.net")
    if (!isNvidiaSessionHost) return null

    return host.substringBefore('.')
        .takeIf(SESSION_ZONE_ID::matches)
        ?.uppercase(Locale.US)
}

/** The allocated zone when known, with the requested/routing zone retained as a safe fallback. */
internal fun SessionInfo.reportedServerZone(): String = assignedZone ?: zone
