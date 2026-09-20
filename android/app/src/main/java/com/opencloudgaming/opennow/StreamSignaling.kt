package com.opencloudgaming.opennow

import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.intOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put
import okhttp3.ConnectionSpec
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.TlsVersion
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.webrtc.IceCandidate
import java.net.URLEncoder
import java.util.concurrent.TimeUnit

sealed interface SignalingEvent {
    data object Connected : SignalingEvent
    data class Disconnected(val reason: String) : SignalingEvent
    data class Offer(val sdp: String) : SignalingEvent
    data class RemoteIce(val candidate: IceCandidate) : SignalingEvent
    data class Error(val message: String) : SignalingEvent
    data class Log(val message: String) : SignalingEvent
}

internal fun newGfnSignalingPeerName(): String =
    "peer-${java.util.UUID.randomUUID().toString().replace("-", "").take(12)}"

internal fun buildGfnSignInUrl(
    signalingUrl: String,
    signalingServer: String,
    peerName: String,
    sessionId: String,
    reconnect: Boolean,
): String {
    val fallbackHost = if (signalingServer.contains(":")) signalingServer else "$signalingServer:443"
    val suppliedBase = signalingUrl.trim().ifBlank { "wss://$fallbackHost/nvst/" }
    val authorityAndPath = suppliedBase
        .substringBefore('#')
        .substringBefore('?')
        .substringAfter("://", suppliedBase)
        .trimStart('/')
        .trimEnd('/')
    val signInBase = if (authorityAndPath.endsWith("/sign_in")) {
        "wss://$authorityAndPath"
    } else {
        "wss://$authorityAndPath/sign_in"
    }
    fun queryValue(value: String): String = URLEncoder.encode(value, Charsets.UTF_8.name())
    val version = if (reconnect) "3.0" else "2"
    return buildString {
        append(signInBase)
        append("?peer_id=")
        append(queryValue(peerName))
        append("&version=")
        append(version)
        append("&peer_role=1&pairing_id=")
        append(queryValue(sessionId))
        if (reconnect) append("&reconnect=1")
    }
}

class GfnSignalingClient(
    private val session: SessionInfo,
    private val settings: StreamSettings,
    private val reconnect: Boolean = false,
    private val peerName: String = newGfnSignalingPeerName(),
    private val http: OkHttpClient = defaultHttpClient(),
    private val onEvent: (SignalingEvent) -> Unit,
) {
    private val signalingHttp = signalingWebSocketHttpClient(http)
    private var webSocket: WebSocket? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var heartbeatJob: Job? = null
    private var peerId = 0
    private var remotePeerId = 1
    private var ackCounter = 0

    fun connect() {
        val url = buildSignInUrl()
        val host = url.removePrefix("wss://").substringBefore("/")
        onEvent(SignalingEvent.Log("Signaling connecting url=${signalingUrlForDiagnostics(url, session.sessionId)} session=${streamDiagnosticId(session.sessionId)}"))
        val request = Request.Builder()
            .url(url)
            .header("Sec-WebSocket-Protocol", "x-nv-sessionid.${session.sessionId}")
            .header("Host", host)
            .header("Origin", "https://play.geforcenow.com")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36")
            .build()
        webSocket = signalingHttp.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    onEvent(
                        SignalingEvent.Log(
                            "Signaling open http=${response.code} tls=${response.handshake?.tlsVersion?.javaName ?: "unknown"} " +
                                "protocol=${response.header("Sec-WebSocket-Protocol").orEmpty().replace(session.sessionId, streamDiagnosticId(session.sessionId))}",
                        ),
                    )
                    sendPeerInfo()
                    heartbeatJob?.cancel()
                    NativeInputDiagnostics.retain(
                        "heartbeat.signaling.lifecycle",
                        "signaling heartbeat active intervalMs=5000 session=${streamDiagnosticId(session.sessionId)}",
                    )
                    heartbeatJob = scope.launch {
                        while (true) {
                            delay(5000)
                            val sent = sendJson("""{"hb":1}""")
                            NativeInputDiagnostics.retainResult("heartbeat.signaling.send", sent) {
                                "client heartbeat session=${streamDiagnosticId(session.sessionId)}"
                            }
                        }
                    }
                    onEvent(SignalingEvent.Connected)
                }

                override fun onMessage(webSocket: WebSocket, text: String) = handleMessage(text)
                override fun onMessage(webSocket: WebSocket, bytes: ByteString) = handleMessage(bytes.utf8())

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    heartbeatJob?.cancel()
                    NativeInputDiagnostics.retain(
                        "heartbeat.signaling.lifecycle",
                        "signaling heartbeat stopped socketClosed=$code session=${streamDiagnosticId(session.sessionId)}",
                    )
                    onEvent(SignalingEvent.Disconnected("socket closed code=$code reason=${reason.ifBlank { "none" }}"))
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    heartbeatJob?.cancel()
                    NativeInputDiagnostics.retain(
                        "heartbeat.signaling.lifecycle",
                        "signaling heartbeat stopped failure=${t.javaClass.simpleName} session=${streamDiagnosticId(session.sessionId)}",
                    )
                    val responseText = response?.let { " http=${it.code} message=${it.message}" }.orEmpty()
                    onEvent(SignalingEvent.Error("${t.javaClass.simpleName}: ${t.message ?: "Signaling failed"}$responseText"))
                }
            },
        )
    }

    fun sendAnswer(sdp: String, nvstSdp: String?) {
        onEvent(SignalingEvent.Log(sdpDiagnosticSummary("Sending answer", sdp)))
        if (!nvstSdp.isNullOrBlank()) {
            onEvent(SignalingEvent.Log("Sending NVST SDP lines=${nvstSdp.lineSequence().count()} bytes=${nvstSdp.length}"))
        }
        val msg = buildJsonObject {
            put("type", "answer")
            put("sdp", sdp)
            if (nvstSdp != null) put("nvstSdp", nvstSdp)
        }.toString()
        sendPeerMessage(msg)
    }

    fun sendIceCandidate(candidate: IceCandidate) {
        if (candidate.sdp.contains(" tcp ", ignoreCase = true)) {
            onEvent(SignalingEvent.Log("Dropping TCP local ICE candidate ${candidate.diagnosticSummary()}"))
            return
        }
        onEvent(SignalingEvent.Log("Sending local ICE candidate ${candidate.diagnosticSummary()}"))
        val msg = buildJsonObject {
            put("candidate", candidate.sdp)
            put("sdpMid", candidate.sdpMid)
            put("sdpMLineIndex", candidate.sdpMLineIndex)
        }.toString()
        sendPeerMessage(msg)
    }

    fun requestKeyframe(reason: String, backlogFrames: Int, attempt: Int) {
        val msg = buildJsonObject {
            put("type", "request_keyframe")
            put("reason", reason)
            put("backlogFrames", backlogFrames)
            put("attempt", attempt)
        }.toString()
        sendPeerMessage(msg)
    }

    fun disconnect() {
        heartbeatJob?.cancel()
        if (webSocket != null) {
            NativeInputDiagnostics.retain(
                "heartbeat.signaling.lifecycle",
                "signaling heartbeat stopped clientDisconnect session=${streamDiagnosticId(session.sessionId)}",
            )
        }
        webSocket?.close(1000, "closed")
        webSocket = null
    }

    private fun buildSignInUrl(): String {
        return buildGfnSignInUrl(
            signalingUrl = session.signalingUrl,
            signalingServer = session.signalingServer,
            peerName = peerName,
            sessionId = session.sessionId,
            reconnect = reconnect,
        )
    }

    private fun handleMessage(text: String) {
        val parsed = runCatching { OpenNowJson.parseToJsonElement(text).jsonObject }.getOrNull()
        if (parsed == null) {
            onEvent(SignalingEvent.Log("Ignoring non-JSON signaling packet"))
            return
        }
        parsed["peer_info"]?.jsonObject?.let { info ->
            if (info["name"]?.jsonPrimitive?.contentOrNull == peerName) {
                peerId = info["id"]?.jsonPrimitive?.intOrNull ?: peerId
            }
        }
        parsed["ackid"]?.jsonPrimitive?.intOrNull?.let { ack ->
            val shouldAck = parsed["peer_info"]?.jsonObject?.get("id")?.jsonPrimitive?.intOrNull != peerId
            if (shouldAck) sendJson("""{"ack":$ack}""")
        }
        signalingHeartbeatReply(parsed)?.let { reply ->
            // Match the desktop client and acknowledge server-driven
            // heartbeats immediately. The periodic client heartbeat remains
            // a separate keepalive when the server does not initiate one.
            val sent = sendJson(reply)
            NativeInputDiagnostics.retainResult("heartbeat.signaling.reply", sent) {
                "server heartbeat reply session=${streamDiagnosticId(session.sessionId)}"
            }
            return
        }
        val peerMsg = parsed["peer_msg"]?.jsonObject ?: return
        remotePeerId = peerMsg["from"]?.jsonPrimitive?.intOrNull ?: remotePeerId
        val msg = peerMsg["msg"]?.jsonPrimitive?.contentOrNull ?: return
        val payload = runCatching { OpenNowJson.parseToJsonElement(msg).jsonObject }.getOrNull() ?: return
        when {
            payload["type"]?.jsonPrimitive?.contentOrNull == "offer" -> {
                val sdp = payload["sdp"]?.jsonPrimitive?.contentOrNull
                if (sdp != null) {
                    onEvent(SignalingEvent.Log(sdpDiagnosticSummary("Received offer", sdp)))
                    onEvent(SignalingEvent.Offer(sdp))
                }
            }
            payload["candidate"]?.jsonPrimitive?.contentOrNull != null -> {
                val candidate = IceCandidate(
                    payload["sdpMid"]?.jsonPrimitive?.contentOrNull,
                    payload["sdpMLineIndex"]?.jsonPrimitive?.intOrNull ?: 0,
                    payload["candidate"]?.jsonPrimitive?.contentOrNull.orEmpty(),
                )
                onEvent(SignalingEvent.Log("Received remote ICE candidate ${candidate.diagnosticSummary()}"))
                onEvent(SignalingEvent.RemoteIce(candidate))
            }
        }
    }

    private fun sendPeerInfo() {
        val (width, height) = streamResolutionPixels(settings)
        onEvent(SignalingEvent.Log("Sending peer info resolution=${width}x$height peer=$peerName"))
        sendJson(
            """
            {"ackid":${nextAckId()},"peer_info":{"browser":"Chrome","browserVersion":"131","connected":true,"id":$peerId,"name":"$peerName","peerRole":0,"resolution":"${width}x$height","version":2}}
            """.trimIndent(),
        )
    }

    private fun sendPeerMessage(message: String) {
        val escaped = message.replace("\\", "\\\\").replace("\"", "\\\"")
        sendJson("""{"peer_msg":{"from":$peerId,"to":$remotePeerId,"msg":"$escaped"},"ackid":${nextAckId()}}""")
    }

    private fun sendJson(text: String): Boolean = webSocket?.send(text) == true

    private fun nextAckId(): Int {
        ackCounter += 1
        return ackCounter
    }
}

private val SIGNALING_TLS_1_2 =
    ConnectionSpec.Builder(ConnectionSpec.MODERN_TLS)
        .tlsVersions(TlsVersion.TLS_1_2)
        .build()

internal fun signalingWebSocketHttpClient(base: OkHttpClient): OkHttpClient =
    base.newBuilder()
        // GFN already has an application heartbeat. Avoid a second WebSocket
        // ping loop and Android TV's TLS 1.3/Conscrypt reader spin on this
        // long-lived signaling socket; media remains DTLS/WebRTC and unchanged.
        .pingInterval(0, TimeUnit.MILLISECONDS)
        .connectionSpecs(listOf(SIGNALING_TLS_1_2))
        .build()

/**
 * Owns the app-UI side of an in-progress touch gesture independently from the bounds that first
 * claimed it. Compose overlays can disappear and replace one another between DOWN and UP (for
 * example, the stream-menu launcher is replaced by the menu panel). Removing the launcher's bounds
 * must not turn that already-owned finger back into a game touch or let its trailing UP activate a
 * control in the newly opened panel.
 */
