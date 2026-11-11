package com.opencloudgaming.opennow

import java.io.IOException
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.contentOrNull

/** Only an explicit credential rejection can invalidate a saved refresh mechanism. */
internal class AuthRefreshException(val statusCode: Int, val oauthError: String?) :
    IOException("Sign-in refresh failed (HTTP $statusCode)${oauthError?.let { ": $it" }.orEmpty()}") {
    val credentialsRejected: Boolean
        get() = statusCode in setOf(400, 401) && oauthError in setOf("invalid_grant", "invalid_token")
}

internal fun authRefreshResponse(statusCode: Int, text: String): JsonObject {
    val root = runCatching { OpenNowJson.parseToJsonElement(text) as? JsonObject }.getOrNull()
    if (statusCode !in 200..299) {
        val oauthError = (root?.get("error") as? JsonPrimitive)?.contentOrNull
            ?.takeIf { it.matches(Regex("[a-z_]{1,64}")) }
        throw AuthRefreshException(statusCode, oauthError)
    }
    return root ?: throw IOException("Sign-in refresh returned an invalid response. Try again shortly.")
}

/** A timeout, outage, wrong client ID, or failed user-info request must preserve saved sign-in. */
internal fun refreshCredentialsRejected(failures: List<Exception>): Boolean =
    failures.isNotEmpty() && failures.all { it is AuthRefreshException && it.credentialsRejected }

internal fun AuthSession?.isSameAuthAccount(expected: AuthSession): Boolean =
    this != null && user.userId == expected.user.userId &&
        provider.idpId == expected.provider.idpId && provider.code == expected.provider.code
