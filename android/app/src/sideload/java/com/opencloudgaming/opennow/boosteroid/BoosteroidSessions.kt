package com.opencloudgaming.opennow.boosteroid

internal data class BoosteroidSession(
    val sessionId: String,
    val status: Int,
    val queuePosition: Int? = null,
    val gatewayHost: String = "",
    val gatewayQuery: String = "",
)

internal data class BoosteroidActiveSession(
    val sessionId: String,
    val appId: Int,
    val status: Int,
    val queuePosition: Int?,
    val sessionToken: String?,
    val sessionQueries: List<String>,
    val gateways: List<String>,
)

internal fun BoosteroidSession.isReadyForStream(): Boolean =
    status == 2 && gatewayHost.isNotBlank() && gatewayQuery.isNotBlank()
