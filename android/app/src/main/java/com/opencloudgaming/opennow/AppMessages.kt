package com.opencloudgaming.opennow

import android.content.Context
import kotlinx.coroutines.suspendCancellableCoroutine
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.longOrNull
import okhttp3.Call
import okhttp3.Callback
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import java.io.IOException
import java.util.concurrent.TimeUnit
import kotlin.coroutines.resume
import kotlin.coroutines.resumeWithException

internal const val APP_MESSAGE_CHECK_INTERVAL_MS = 5 * 60 * 1000L
private const val MESSAGE_URL = "https://api.printedwaste.com/releases/opennow/message"

data class AppMessage(val id: Long, val title: String, val body: String)

internal fun parseAppMessage(payload: String): AppMessage? {
    val root = OpenNowJson.parseToJsonElement(payload) as? JsonObject ?: error("Invalid message response")
    require(root["status"]?.jsonPrimitive?.booleanOrNull == true)
    if (root["data"] == JsonNull) return null
    val message = root["data"] as? JsonObject ?: error("Missing message")
    val id = message["id"]?.jsonPrimitive?.longOrNull ?: error("Missing message ID")
    val title = message["title"]?.jsonPrimitive?.contentOrNull?.trim().orEmpty()
    val body = message["body"]?.jsonPrimitive?.contentOrNull?.trim().orEmpty()
    require(id > 0 && title.length in 1..120 && body.length in 1..4000)
    require(message["active"]?.jsonPrimitive?.booleanOrNull == true)
    return AppMessage(id, title, body)
}

internal fun AppMessage.isUnacknowledged(lastAcknowledgedId: Long): Boolean = id > lastAcknowledgedId

internal class AppMessageRepository(context: Context, http: OkHttpClient) {
    private val preferences = context.getSharedPreferences("app_messages", Context.MODE_PRIVATE)
    private val client = http.newBuilder().callTimeout(15, TimeUnit.SECONDS).build()

    fun isUnacknowledged(message: AppMessage): Boolean =
        message.isUnacknowledged(preferences.getLong("acknowledged_id", 0L))

    // Called on Dispatchers.IO; only hide the dialog once acknowledgement is durable.
    fun acknowledge(message: AppMessage): Boolean = preferences.edit()
        .putLong("acknowledged_id", maxOf(message.id, preferences.getLong("acknowledged_id", 0L)))
        .commit()

    suspend fun fetch(): AppMessage? = suspendCancellableCoroutine { continuation ->
        val call = client.newCall(Request.Builder().url(MESSAGE_URL).header("Accept", "application/json").build())
        continuation.invokeOnCancellation { call.cancel() }
        call.enqueue(object : Callback {
            override fun onFailure(call: Call, e: IOException) {
                if (continuation.isActive) continuation.resumeWithException(e)
            }
            override fun onResponse(call: Call, response: Response) {
                try {
                    val message = response.use {
                        check(it.isSuccessful) { "Message check failed: ${it.code}" }
                        val bytes = it.peekBody(32_769).bytes()
                        require(bytes.size <= 32_768) { "Message response too large" }
                        parseAppMessage(bytes.toString(Charsets.UTF_8))
                    }
                    if (continuation.isActive) continuation.resume(message)
                } catch (error: Exception) {
                    if (continuation.isActive) continuation.resumeWithException(error)
                }
            }
        })
    }
}
