package com.opencloudgaming.opennow

import kotlinx.serialization.json.*
import okhttp3.HttpUrl.Companion.toHttpUrlOrNull
import java.net.URI

internal data class GfnSessionConnections(
    val serverIp: String,
    val signalingServer: String,
    val signalingUrl: String,
    val mediaConnectionInfo: MediaConnectionInfo?,
    val bundleConnectionInfo: MediaConnectionInfo?,
    val nativeTransportRequired: Boolean,
)

private fun JsonObject.connectionValue(key: String): String? =
    (get(key) as? JsonPrimitive)?.contentOrNull

/** Match dev's WebRTC endpoint ordering, independently of the native RTSPS/media routes. */
internal fun resolveGfnSessionConnections(
    session: JsonObject,
    reportedNativeTransport: Boolean? = reportedGfnSessionNativeTransport(session),
): GfnSessionConnections? {
    val connections = (session["connectionInfo"] as? JsonArray).orEmpty().mapNotNull { it as? JsonObject }
    fun JsonObject.usage() = connectionValue("usage")?.toIntOrNull()
    val candidates = connections.filter {
        val resource = it.connectionValue("resourcePath").orEmpty()
        !it.isRtspSessionConnection() && (resource.startsWith("wss://") || resource.startsWith("https://") ||
            it.usage() == 14)
    } + connections.filter { it.usage() == 14 } + connections.filter { it.usage() == 16 }
    val endpoint = candidates.firstNotNullOfOrNull { connection ->
        webRtcSignalingUrl(connection)?.let { connection to it }
    } ?: return null
    val (connection, url) = endpoint
    val host = if (':' in url.host) "[${url.host}]" else url.host
    fun media(usages: List<Int>, requireIp: Boolean = false): MediaConnectionInfo? = usages.firstNotNullOfOrNull { usage ->
        connections.filter { it.usage() == usage }.firstNotNullOfOrNull { connection ->
            sessionMediaConnection(connection)?.takeIf { !requireIp || isSessionIpLiteral(it.ip) }
        }
    }
    val selectedMedia = media(listOf(2, 17)) ?: webRtcMediaFallbackConnections(connections)
        .firstNotNullOfOrNull(::sessionMediaConnection)
    return GfnSessionConnections(
        serverIp = connection.connectionValue("ip")?.let(::usableSessionHost) ?: url.host,
        signalingServer = "$host:${url.port}",
        signalingUrl = url.toString().replaceFirst("https://", "wss://"),
        mediaConnectionInfo = selectedMedia,
        // BUNDLE (17) can have a different NAT mapping from VIDEO (2) on alliance seats.
        bundleConnectionInfo = media(listOf(17), requireIp = true),
        nativeTransportRequired = reportedNativeTransport ?: connection.isRtspSessionConnection(),
    )
}

/** Match dev's reported transport for every provider; RTSPS descriptors also occur on WebRTC seats. */
internal fun reportedGfnSessionNativeTransport(session: JsonObject): Boolean? {
    val request = session["sessionRequestData"] as? JsonObject ?: return null
    // Explicit native allocation wins over stale WebRTC metadata.
    if (request.connectionValue("secureRTSPSupported") == "true" ||
        request.connectionValue("streamerVersion") == "14") return true
    val metadata = request["metaData"] as? JsonArray
    if (metadata.orEmpty().any { raw ->
            val entry = raw as? JsonObject
            entry?.connectionValue("key") == "GSStreamerType" &&
                entry.connectionValue("value") == "WebRTC"
        }) return false
    return null
}

/** Sparse poll/resume replies must not change a session's explicitly reported transport. */
internal class GfnSessionConnectionResolver(private val retainedSessionLimit: Int = 32) {
    private val reportedTransports = linkedMapOf<String, Boolean>()

    init { require(retainedSessionLimit > 0) }

    @Synchronized fun resolve(session: JsonObject): GfnSessionConnections? {
        val id = session.connectionValue("sessionId")?.takeIf { it.isNotBlank() }
        val reported = reportedGfnSessionNativeTransport(session)
        if (id != null && reported != null) {
            reportedTransports.remove(id)
            reportedTransports[id] = reported
            while (reportedTransports.size > retainedSessionLimit) {
                reportedTransports.remove(reportedTransports.keys.first())
            }
        }
        return resolveGfnSessionConnections(session, reported ?: id?.let(reportedTransports::get))
    }
}

private fun sessionMediaConnection(connection: JsonObject): MediaConnectionInfo? {
    val rawResource = connection.connectionValue("resourcePath")
    val resource = rawResource?.let {
        it.replaceFirst("rtsps://", "https://").replaceFirst("rtsp://", "http://")
            .replaceFirst("wss://", "https://").toHttpUrlOrNull()
    }
    val ip = connection.connectionValue("ip")?.let(::usableSessionHost) ?: resource?.host ?: return null
    val port = connection.connectionValue("port")?.toIntOrNull()
        ?: rawResource?.let { runCatching { URI(it).port }.getOrNull() } ?: return null
    return port.takeIf { it in 1..65535 }?.let { MediaConnectionInfo(ip, it) }
}

private fun isSessionIpLiteral(host: String): Boolean =
    if (':' in host) {
        "https://[${host.removeSurrounding("[", "]")}]".toHttpUrlOrNull() != null
    } else {
        val octets = host.split('.')
        octets.size == 4 && octets.all { octet ->
            octet.isNotEmpty() && octet.all(Char::isDigit) && octet.toIntOrNull()?.let { it in 0..255 } == true
        }
    }

private fun webRtcSignalingUrl(connection: JsonObject): okhttp3.HttpUrl? {
    val usage = connection.connectionValue("usage")?.toIntOrNull()
    if (usage !in listOf(14, 16)) return null
    val resource = connection.connectionValue("resourcePath")?.takeIf { it.isNotEmpty() } ?: "/nvst/"
    val rtsp = resource.startsWith("rtsp://") || resource.startsWith("rtsps://")
    val absolute = rtsp || resource.startsWith("wss://") || resource.startsWith("https://")
    val builder = if (absolute) {
        val translated = resource.replaceFirst("rtsps://", "https://")
            .replaceFirst("rtsp://", "https://").replaceFirst("wss://", "https://")
        val parsed = translated.toHttpUrlOrNull() ?: return null
        if (parsed.username.isNotEmpty() || parsed.password.isNotEmpty() || parsed.fragment != null) return null
        parsed.newBuilder().apply {
            if (rtsp) port(443).encodedPath("/nvst/").query(null)
        }
    } else {
        if (!resource.startsWith('/') || resource.contains("://")) return null
        val host = connection.connectionValue("ip")?.let(::usableSessionHost) ?: return null
        if (host in listOf("0.0.0.0", "::", "[::]")) return null
        val authority = if (':' in host && !host.startsWith('[')) "[$host]" else host
        val parsed = "https://$authority$resource".toHttpUrlOrNull() ?: return null
        // For a relative path, connectionInfo.port can be the UDP media peer
        // (for example 49003), not the WebSocket service. Keep signaling on 443;
        // a nonstandard signaling port must come from an absolute WSS/HTTPS URL.
        parsed.newBuilder()
    }
    return builder.build()
}
