// Adapted from OpenStroid Android. See android/app/src/sideload/assets/licenses/OpenStroid-NOTICE.txt.
package com.opencloudgaming.opennow.boosteroid

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.util.Log
import com.opencloudgaming.opennow.OpenNowJson
import com.opencloudgaming.opennow.StreamSettings
import com.opencloudgaming.opennow.VideoCodec
import com.opencloudgaming.opennow.streamDiagnosticId
import com.opencloudgaming.opennow.streamResolutionPixels
import java.net.URI
import java.net.URLEncoder
import java.util.UUID
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.webrtc.IceCandidate
import org.webrtc.PeerConnection
import org.webrtc.SessionDescription

internal sealed interface BoosteroidGatewayEvent {
    data object Connected : BoosteroidGatewayEvent
    data class Disconnected(val reason: String) : BoosteroidGatewayEvent
    data object WebRtcRequested : BoosteroidGatewayEvent
    data class StreamStatus(val value: JsonObject) : BoosteroidGatewayEvent
    data class CursorMessage(val value: JsonObject) : BoosteroidGatewayEvent
    data class ControllerMessage(val value: JsonObject) : BoosteroidGatewayEvent
    data class Error(val message: String) : BoosteroidGatewayEvent
    data class Log(val message: String) : BoosteroidGatewayEvent
}

internal class BoosteroidGatewayClient(
    private val session: BoosteroidSession,
    private val settings: StreamSettings,
    private val networkType: String,
    private val http: OkHttpClient,
    private val onEvent: (BoosteroidGatewayEvent) -> Unit,
) {
    private var controlSocket: WebSocket? = null
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    val peerId: String = UUID.randomUUID().toString()
    private val gatewayHost = session.gatewayHost
    private val sessionQuery = session.gatewayQuery
    val activeCodec: VideoCodec = if (settings.codec == VideoCodec.AV1) VideoCodec.AV1 else VideoCodec.H264
    private val webRtcApiHost: String = normalizeWebRtcApiHost(gatewayHost)
    private val webRtcApiBase: String = "https://$webRtcApiHost/webrtc/api"
    private var runtimeFramerate: Int = settings.fps
    private var runtimeFsrEnabled = false
    private var streamProfileCorrectionAttempts = 0

    fun connect() {
        if (gatewayHost.isBlank()) {
            onEvent(BoosteroidGatewayEvent.Error("Boosteroid did not return a streaming gateway host"))
            return
        }
        if (sessionQuery.isBlank()) {
            onEvent(BoosteroidGatewayEvent.Error("Boosteroid did not return a streaming gateway query"))
            return
        }
        val (width, height) = streamResolutionPixels(settings)
        val mergedQuery = mergeBoosteroidStreamRuntimeQuery(sessionQuery, settings)
        val url = "wss://$gatewayHost/?$mergedQuery"
        onEvent(
            BoosteroidGatewayEvent.Log(
                "Control socket connecting url=${boosteroidSignalingUrlForDiagnostics(url, session.sessionId)} " +
                    "session=${streamDiagnosticId(session.sessionId)} codec=${activeCodec.name} resolution=${width}x$height",
            ),
        )
        val request = Request.Builder()
            .url(url)
            .build()
        controlSocket = http.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    onEvent(BoosteroidGatewayEvent.Log("Control socket open http=${response.code}"))
                    onEvent(BoosteroidGatewayEvent.Connected)
                }

                override fun onMessage(webSocket: WebSocket, text: String) = handleMessage(text)
                override fun onMessage(webSocket: WebSocket, bytes: ByteString) = handleMessage(bytes.utf8())

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    webSocket.close(1000, null)
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    onEvent(BoosteroidGatewayEvent.Disconnected("socket closed code=$code reason=${reason.ifBlank { "none" }}"))
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    val responseText = response?.let { " http=${it.code} message=${it.message}" }.orEmpty()
                    onEvent(BoosteroidGatewayEvent.Error("${t.javaClass.simpleName}: ${t.message ?: "Control socket failed"}$responseText"))
                }
            },
        )
    }

    fun sendEvent(value: JsonObject): Boolean =
        controlSocket?.send(value.toString()) == true

    fun sendReady() {
        sendEvent(buildJsonObject {
            put("type", "settings")
            put("action", "ready")
        })
        sendEvent(buildJsonObject {
            put("type", "stream")
            put("action", "page")
            put("is_visible", true)
            put("is_focus", true)
        })
    }

    fun requestKeyframe(reason: String, attempt: Int) {
        sendEvent(buildJsonObject {
            put("type", "stream")
            put("action", "keyframe")
            put("reason", reason)
            put("attempt", attempt)
        })
    }

    suspend fun fetchGatewayCodec(): String = boosteroidResult {
        val root = getJson("$webRtcApiBase/getParams?sessionId=${queryEncode(session.sessionId)}")
            .asObject()
        (root?.string("codec") ?: root?.obj("data")?.string("codec")).orEmpty()
    }.getOrElse {
        onEvent(BoosteroidGatewayEvent.Log("getParams failed: ${it.message.orEmpty()}"))
        ""
    }

    suspend fun fetchIceServers(): List<PeerConnection.IceServer> = boosteroidResult {
        parseIceServers(getJson("$webRtcApiBase/getIceServers?sessionId=${queryEncode(session.sessionId)}"))
            .takeIf { it.isNotEmpty() }
            ?: defaultIceServers()
    }.getOrElse {
        onEvent(BoosteroidGatewayEvent.Log("getIceServers failed; using public STUN fallback: ${it.message.orEmpty()}"))
        defaultIceServers()
    }

    suspend fun sendOffer(sdp: String): SessionDescription {
        val request = buildJsonObject {
            put("type", "offer")
            put("sdp", sdp)
        }
        val response = postJson(
            "$webRtcApiBase/call?peerid=${queryEncode(peerId)}&sessionId=${queryEncode(session.sessionId)}",
            request,
        )
        val root = response.asObject() ?: error("Invalid WebRTC answer from Boosteroid gateway")
        val data = root.obj("data")
        val answer = data?.obj("answer") ?: root.obj("answer") ?: data ?: root
        val type = answer.string("type").orEmpty()
        val answerSdp = answer.string("sdp").orEmpty()
        if (!type.equals("answer", ignoreCase = true) || answerSdp.isBlank()) {
            error("Invalid WebRTC answer from Boosteroid gateway")
        }
        return SessionDescription(SessionDescription.Type.ANSWER, answerSdp)
    }

    suspend fun sendIceCandidate(candidate: IceCandidate) {
        val value = buildJsonObject {
            put("candidate", candidate.sdp)
            candidate.sdpMid?.let { put("sdpMid", it) }
            put("sdpMLineIndex", candidate.sdpMLineIndex)
        }
        boosteroidResult {
            postJson(
                "$webRtcApiBase/addIceCandidate?peerid=${queryEncode(peerId)}&sessionId=${queryEncode(session.sessionId)}",
                value,
            )
        }.onFailure {
            onEvent(BoosteroidGatewayEvent.Log("addIceCandidate failed: ${it.message.orEmpty()}"))
        }
    }

    suspend fun fetchRemoteIceCandidates(): List<IceCandidate> = boosteroidResult {
        parseIceCandidates(
            getJson(
                "$webRtcApiBase/getIceCandidate?peerid=${queryEncode(peerId)}&sessionId=${queryEncode(session.sessionId)}",
            ),
        )
    }.getOrElse { emptyList() }

    fun disconnect() {
        sendEvent(buildJsonObject {
            put("type", "settings")
            put("action", "terminating")
        })
        controlSocket?.close(1000, "Disconnected by OpenNOW")
        controlSocket = null
        if (webRtcApiHost.isNotBlank() && session.sessionId.isNotBlank()) {
            scope.launch {
                boosteroidResult {
                    getJson(
                        "$webRtcApiBase/hangup?peerid=${queryEncode(peerId)}&sessionId=${queryEncode(session.sessionId)}",
                    )
                }
            }.invokeOnCompletion { scope.cancel() }
        } else {
            scope.cancel()
        }
    }

    private fun handleMessage(text: String) {
        val parsed = runCatching { OpenNowJson.parseToJsonElement(text).asObject() }.getOrNull()
        if (parsed == null) {
            onEvent(BoosteroidGatewayEvent.Log("Ignoring non-JSON control frame"))
            return
        }
        val type = parsed.string("type")
        val action = parsed.string("action")
        when {
            type == "settings" && action == "webrtc" -> onEvent(BoosteroidGatewayEvent.WebRtcRequested)
            type == "stream" && action == "getstatus" -> sendGatewayStatus()
            type == "stream" && action == "setstatus" -> {
                val value = parsed.obj("value") ?: JsonObject(emptyMap())
                val reportedFps = value.int("framerate")?.let { if (it >= 90) 120 else 60 }
                value.boolean("fsr")?.let { runtimeFsrEnabled = it }
                onEvent(BoosteroidGatewayEvent.StreamStatus(value))
                if (
                    reportedFps != null &&
                    reportedFps != settings.fps &&
                    streamProfileCorrectionAttempts < BOOSTEROID_STREAM_PROFILE_CORRECTION_LIMIT
                ) {
                    streamProfileCorrectionAttempts += 1
                    runtimeFramerate = settings.fps
                    onEvent(
                        BoosteroidGatewayEvent.Log(
                            "Gateway reported ${reportedFps}fps; reapplying requested ${settings.fps}fps " +
                                "attempt=$streamProfileCorrectionAttempts",
                        ),
                    )
                    sendRuntimeStreamProfile()
                }
            }
            type == "cursor" -> onEvent(BoosteroidGatewayEvent.CursorMessage(parsed))
            type == "controller" -> onEvent(BoosteroidGatewayEvent.ControllerMessage(parsed))
            type == "mouse" || type == "keyboard" -> Unit
            type == "settings" && action == "streamIds" -> {
                onEvent(BoosteroidGatewayEvent.Log("Ignoring legacy Janus streamIds; WebRTC transport is active"))
            }
            else -> onEvent(BoosteroidGatewayEvent.Log("Control message type=${type.orEmpty()} action=${action.orEmpty()}"))
        }
    }

    private fun sendGatewayStatus() {
        sendEvent(buildJsonObject {
            put("type", "keyboard")
            put("action", "language")
            put("code", 1033)
        })
        sendEvent(buildJsonObject {
            put("type", "stream")
            put("action", "status")
            put("value", "ok")
            put("params", buildJsonObject {
                put("type", "web")
                put("ver", "opennow")
                put("gpu", "unknown")
                put("proto", 1)
                put("framerate_max", settings.fps)
                put("bitrate_max", settings.maxBitrateMbps.coerceAtLeast(1) * 1_000_000)
                put("hdr", settings.hdrEnabled)
                put("cursor_zip", true)
                put("filler", false)
                put("beta", 0)
                put("rtcEngine", "webrtc")
                put("rtcAudio", "pcm")
                put("network_type", networkType)
                if (activeCodec == VideoCodec.AV1) put("codec", "av1")
            })
        })
        sendRuntimeStreamProfile()
    }

    private fun sendRuntimeStreamProfile() {
        sendEvent(buildJsonObject {
            put("type", "stream")
            put("action", "refreshRate")
            put("value", runtimeFramerate)
        })
        sendEvent(buildJsonObject {
            put("type", "stream")
            put("action", "bitrate_max")
            put("value", settings.maxBitrateMbps.coerceAtLeast(1) * 1_000_000)
        })
        val (width, height) = streamResolutionPixels(settings)
        sendEvent(buildJsonObject {
            put("type", "stream")
            put("action", "screenSize")
            put("value", buildJsonObject {
                put("width", width)
                put("height", height)
            })
        })
        if (runtimeFsrEnabled) {
            sendEvent(buildJsonObject {
                put("type", "stream")
                put("action", "fsr")
                put("value", true)
            })
        }
    }

    private suspend fun getJson(url: String): JsonElement = executeJson(Request.Builder().url(url).get().build())

    private suspend fun postJson(url: String, value: JsonElement): JsonElement =
        executeJson(
            Request.Builder()
                .url(url)
                .post(value.toString().toRequestBody(BOOSTEROID_JSON_MEDIA_TYPE))
                .build(),
        )

    private suspend fun executeJson(request: Request): JsonElement =
        http.awaitBoosteroidResponse(request) { response ->
            check(response.isSuccessful) {
                "Boosteroid WebRTC API ${request.url.encodedPath} failed (${response.code})"
            }
            val body = response.body.string()
            if (body.isBlank()) JsonObject(emptyMap()) else OpenNowJson.parseToJsonElement(body)
        }

    private fun parseIceServers(payload: JsonElement): List<PeerConnection.IceServer> {
        val values = when (payload) {
            is JsonArray -> payload
            is JsonObject -> when (val data = payload["iceServers"] ?: payload["data"]) {
                is JsonArray -> data
                is JsonObject -> data["iceServers"] as? JsonArray ?: JsonArray(emptyList())
                else -> JsonArray(emptyList())
            }
            else -> JsonArray(emptyList())
        }
        return values.mapNotNull { value ->
            val item = value.asObject() ?: return@mapNotNull null
            val urls = when (val urlsValue = item["urls"] ?: item["url"]) {
                is JsonArray -> urlsValue.mapNotNull { it.asString()?.takeIf(String::isNotBlank) }
                else -> listOfNotNull(urlsValue.asString()?.takeIf(String::isNotBlank))
            }
            if (urls.isEmpty()) return@mapNotNull null
            PeerConnection.IceServer.builder(urls).apply {
                item.string("username")?.let(::setUsername)
                (item.string("credential") ?: item.string("password"))?.let(::setPassword)
            }.createIceServer()
        }
    }

    private fun parseIceCandidates(payload: JsonElement): List<IceCandidate> {
        val values = when (payload) {
            is JsonArray -> payload.toList()
            is JsonObject -> when (val data = payload["data"]) {
                is JsonArray -> data.toList()
                is JsonObject -> listOf(data)
                else -> if (payload["candidate"] != null) listOf(payload) else emptyList()
            }
            else -> emptyList()
        }
        return values.mapNotNull { value ->
            val item = value.asObject() ?: return@mapNotNull null
            val nested = item.obj("candidate")
            val source = nested ?: item
            val candidate = source.string("candidate") ?: item.string("candidate") ?: return@mapNotNull null
            IceCandidate(
                source.string("sdpMid") ?: item.string("sdpMid"),
                source.int("sdpMLineIndex") ?: item.int("sdpMLineIndex") ?: 0,
                candidate,
            )
        }
    }

    private fun defaultIceServers(): List<PeerConnection.IceServer> = listOf(
        PeerConnection.IceServer.builder("stun:stun.l.google.com:19302").createIceServer(),
        PeerConnection.IceServer.builder("stun:stun2.l.google.com:19302").createIceServer(),
    )

    private companion object {
        val BOOSTEROID_JSON_MEDIA_TYPE = "application/json; charset=utf-8".toMediaType()
    }
}

private fun normalizeWebRtcApiHost(gatewayHost: String): String {
    val uri = runCatching { URI("wss://$gatewayHost") }.getOrNull()
    val host = uri?.host.orEmpty().ifBlank {
        gatewayHost.removePrefix("[").substringBefore(']').substringBefore(':')
    }
    return if (host.contains(':') && !host.startsWith('[')) "[$host]" else host
}

private fun queryEncode(value: String): String =
    URLEncoder.encode(value, Charsets.UTF_8.name()).replace("+", "%20")

internal fun boosteroidNetworkType(context: Context): String {
    return runCatching {
        val manager = context.getSystemService(ConnectivityManager::class.java) ?: return@runCatching "unknown"
        val capabilities = manager.getNetworkCapabilities(manager.activeNetwork) ?: return@runCatching "unknown"
        when {
            capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN) -> "vpn"
            capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET) -> "ethernet"
            capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI) -> "wifi"
            capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR) -> "cellular"
            else -> "unknown"
        }
    }.getOrDefault("unknown")
}

private const val BOOSTEROID_STREAM_PROFILE_CORRECTION_LIMIT = 2
