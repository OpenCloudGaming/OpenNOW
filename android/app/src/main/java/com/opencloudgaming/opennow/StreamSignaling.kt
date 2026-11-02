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
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.JsonNull
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
    val version = "3.0"
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
    private var connectionDiagnostic: StreamConnectionAttempt? = null

    fun connect() {
        val url = buildSignInUrl()
        val diagnostic = StreamConnectionDiagnostics.begin(url, reconnect)
        connectionDiagnostic = diagnostic
        val host = url.removePrefix("wss://").substringBefore("/")
        onEvent(SignalingEvent.Log("Signaling connecting url=${signalingUrlForDiagnostics(url, session.sessionId)} session=${streamDiagnosticId(session.sessionId)}"))
        val request = Request.Builder()
            .url(url)
            .header("Sec-WebSocket-Protocol", "x-nv-sessionid.${session.sessionId}")
            .header("Host", host)
            .header("Origin", "https://play.geforcenow.com")
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36")
            .build()
        webSocket = signalingHttp.newBuilder()
            .eventListener(signalingDiagnosticEventListener(diagnostic))
            .build().newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    diagnostic.opened(response.code, response.handshake?.tlsVersion?.javaName)
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
                            val sent = sendJson("""{"hb":1}""", "heartbeat")
                            NativeInputDiagnostics.retainResult("heartbeat.signaling.send", sent) {
                                "client heartbeat session=${streamDiagnosticId(session.sessionId)}"
                            }
                        }
                    }
                    onEvent(SignalingEvent.Connected)
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    handleMessage(text)
                }
                override fun onMessage(webSocket: WebSocket, bytes: ByteString) = onMessage(webSocket, bytes.utf8())

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    diagnostic.closing(code)
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    diagnostic.closed(code)
                    heartbeatJob?.cancel()
                    NativeInputDiagnostics.retain(
                        "heartbeat.signaling.lifecycle",
                        "signaling heartbeat stopped socketClosed=$code session=${streamDiagnosticId(session.sessionId)}",
                    )
                    onEvent(SignalingEvent.Disconnected("socket closed code=$code reason=${reason.ifBlank { "none" }}"))
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    diagnostic.failed(t, response?.code)
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
        sendPeerMessage(msg, "answer")
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
        sendPeerMessage(msg, "ice")
    }

    fun requestKeyframe(reason: String, backlogFrames: Int, attempt: Int) {
        val msg = buildJsonObject {
            put("type", "request_keyframe")
            put("reason", reason)
            put("backlogFrames", backlogFrames)
            put("attempt", attempt)
        }.toString()
        sendPeerMessage(msg, "keyframe")
    }

    fun disconnect() {
        connectionDiagnostic?.localClose()
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

    internal fun recordOfferTimeout() {
        connectionDiagnostic?.offerTimedOut()
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

    internal fun handleMessage(text: String) {
        val element = runCatching { OpenNowJson.parseToJsonElement(text) }.getOrNull()
        connectionDiagnostic?.received(element)
        val parsed = element as? JsonObject
        if (parsed == null) {
            onEvent(SignalingEvent.Log("Ignoring non-JSON signaling packet"))
            return
        }
        // Optional fields can be explicitly null. jsonObject/jsonPrimitive throw on a
        // different JSON shape, which would escape OkHttp's callback and kill the process.
        val peerInfo = parsed["peer_info"] as? JsonObject
        peerInfo?.let { info ->
            val role = (info["peer_role"] as? JsonPrimitive)?.intOrNull ?: (info["peerRole"] as? JsonPrimitive)?.intOrNull
            onEvent(SignalingEvent.Log("Signaling peer registration local=${(info["name"] as? JsonPrimitive)?.contentOrNull == peerName} id=${(info["id"] as? JsonPrimitive)?.intOrNull} role=$role"))
            if ((info["name"] as? JsonPrimitive)?.contentOrNull == peerName) {
                peerId = (info["id"] as? JsonPrimitive)?.intOrNull ?: peerId
            }
        }
        (parsed["ackid"] as? JsonPrimitive)?.intOrNull?.let { ack ->
            val shouldAck = (peerInfo?.get("id") as? JsonPrimitive)?.intOrNull != peerId
            if (shouldAck) sendJson("""{"ack":$ack}""", "ack")
        }
        parsed["error"]?.takeUnless { it == JsonNull }?.let { error ->
            onEvent(SignalingEvent.Log("Signaling server rejected request error=${sanitizeDiagnosticLogPayload(error.toString(), limit = 400)}"))
            // A removed registration cannot reconnect under the same peer name. Surface
            // the protocol error before the socket's generic EOF loses that information.
            if ((error as? JsonPrimitive)?.contentOrNull == "peerRemoved") {
                onEvent(SignalingEvent.Error("Signaling peerRemoved"))
                return
            }
        }
        signalingHeartbeatReply(parsed)?.let { reply ->
            // Match the desktop client and acknowledge server-driven
            // heartbeats immediately. The periodic client heartbeat remains
            // a separate keepalive when the server does not initiate one.
            val sent = sendJson(reply, "heartbeat_reply")
            NativeInputDiagnostics.retainResult("heartbeat.signaling.reply", sent) {
                "server heartbeat reply session=${streamDiagnosticId(session.sessionId)}"
            }
            return
        }
        val peerMsg = parsed["peer_msg"] as? JsonObject ?: return
        remotePeerId = (peerMsg["from"] as? JsonPrimitive)?.intOrNull ?: remotePeerId
        val msg = (peerMsg["msg"] as? JsonPrimitive)?.contentOrNull ?: return
        val payload = runCatching { OpenNowJson.parseToJsonElement(msg) as? JsonObject }.getOrNull() ?: return
        when {
            (payload["type"] as? JsonPrimitive)?.contentOrNull == "offer" -> {
                val sdp = (payload["sdp"] as? JsonPrimitive)?.contentOrNull
                if (sdp != null) {
                    onEvent(SignalingEvent.Log(sdpDiagnosticSummary("Received offer", sdp)))
                    onEvent(SignalingEvent.Offer(sdp))
                }
            }
            (payload["candidate"] as? JsonPrimitive)?.contentOrNull != null -> {
                val candidate = IceCandidate(
                    (payload["sdpMid"] as? JsonPrimitive)?.contentOrNull,
                    (payload["sdpMLineIndex"] as? JsonPrimitive)?.intOrNull ?: 0,
                    (payload["candidate"] as? JsonPrimitive)?.contentOrNull.orEmpty(),
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
            {"ackid":${nextAckId()},"peer_info":{"browser":"Chrome","browserVersion":"131.0.0.0","connected":true,"id":$peerId,"name":"$peerName","peer_role":1,"resolution":"${width}x$height","signalingVersion":"3.0","version":3}}
            """.trimIndent(),
            "peer_info",
        )
    }

    private fun sendPeerMessage(message: String, kind: String) {
        val escaped = message.replace("\\", "\\\\").replace("\"", "\\\"")
        sendJson("""{"peer_msg":{"from":$peerId,"to":$remotePeerId,"msg":"$escaped"},"ackid":${nextAckId()}}""", kind)
    }

    private fun sendJson(text: String, kind: String): Boolean {
        val accepted = webSocket?.send(text) == true
        connectionDiagnostic?.sent(kind, accepted)
        return accepted
    }

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
