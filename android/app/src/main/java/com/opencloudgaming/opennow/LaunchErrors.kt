package com.opencloudgaming.opennow

private val LaunchErrorWhitespaceRegex = Regex("""\s+""")

internal class CloudMatchRequestStatusException(
    val statusCode: Int?,
    val statusDescription: String?,
    val unifiedErrorCode: String?,
) : IllegalStateException(
    buildString {
        append("CloudMatch returned status ")
        append(statusCode?.toString() ?: "unknown")
        statusDescription?.trim()?.takeIf { it.isNotEmpty() }?.let {
            append(": ")
            append(it)
        }
        unifiedErrorCode?.trim()?.takeIf { it.isNotEmpty() }?.let {
            append(" (unified error ")
            append(it)
            append(')')
        }
    },
)

internal fun normalizeLaunchErrorMessage(error: Throwable, gameTitle: String? = null): String {
    val text = error.message ?: return "Launch failed"
    val cloudMatchFailure = error.cloudMatchRequestStatusException()
    val terminalSession = error.terminalSessionStatusException()
    return when {
        terminalSession != null ->
            "The cloud session is no longer available (status ${terminalSession.status}). " +
                "Start the game again to open a new session."
        cloudMatchFailure?.isEntitlementError() == true ->
            "GeForce NOW rejected this launch because it could not verify a playable membership for the signed-in account. If Free is active, confirm you signed in with the same NVIDIA account, then sign out and back in."
        cloudMatchFailure?.isLimitedModeStreamingError() == true -> limitedModeStreamingMessage(gameTitle)
        cloudMatchFailure?.isQueueAbandoned() == true ->
            "The cloud provider ended this queue request. Start the game again to join a new queue."
        text.contains("patch", ignoreCase = true) || text.contains("maintenance", ignoreCase = true) ->
            "Game is patching or under maintenance. Try again when NVIDIA finishes updating it."
        else -> text
    }
}

internal fun isMissingGfnPlanError(error: Throwable): Boolean =
    error.cloudMatchRequestStatusException()?.isEntitlementError() == true

internal fun isAbandonedQueueError(error: Throwable): Boolean =
    error.cloudMatchRequestStatusException()?.isQueueAbandoned() == true

internal fun shouldForgetActiveSessionAfterResumeFailure(error: Throwable): Boolean =
    error.terminalSessionStatusException() != null

internal fun shouldOfferLowerSettingsRetry(
    error: Throwable,
    requestedSettings: StreamSettings,
): Boolean {
    if (requestedSettings == requestedSettings.loweredSessionLaunchProfile()) return false
    val cloudMatchFailure = error.cloudMatchRequestStatusException()
    return cloudMatchFailure?.isInternalServerError() == true ||
        error.causeChainMessages().any { message ->
            message.contains("internal server error", ignoreCase = true) ||
                message.contains("INTERNAL_ERROR_STATUS", ignoreCase = true) ||
                message.contains("8A8C0000", ignoreCase = true)
        }
}

private fun Throwable.causeChainMessages(): Sequence<String> = sequence {
    var current: Throwable? = this@causeChainMessages
    while (current != null) {
        current.message?.let { yield(it) }
        current = current.cause?.takeUnless { it === current }
    }
}

private fun Throwable.terminalSessionStatusException(): TerminalSessionStatusException? {
    var current: Throwable? = this
    while (current != null) {
        if (current is TerminalSessionStatusException) return current
        current = current.cause?.takeUnless { it === current }
    }
    return null
}

private fun Throwable.cloudMatchRequestStatusException(): CloudMatchRequestStatusException? {
    var current: Throwable? = this
    while (current != null) {
        if (current is CloudMatchRequestStatusException) return current
        current = current.cause?.takeUnless { it === current }
    }
    return null
}

private fun CloudMatchRequestStatusException.isEntitlementError(): Boolean =
    statusCode == 18 ||
        statusDescriptionToken().equals("ENTITLEMENT_FAILURE_STATUS", ignoreCase = true) ||
        normalizedUnifiedErrorCode() == "8A910006"

private fun CloudMatchRequestStatusException.isLimitedModeStreamingError(): Boolean =
    statusDescriptionToken().equals("STREAMING_NOT_ALLOWED_IN_LIMITED_MODE", ignoreCase = true) ||
        normalizedUnifiedErrorCode() == "8A91000D"

private fun CloudMatchRequestStatusException.isQueueAbandoned(): Boolean =
    statusCode == 69 ||
        statusDescriptionToken().equals("SESSION_REQUEST_IN_QUEUE_ABANDONED", ignoreCase = true) ||
        normalizedUnifiedErrorCode() == "4A8C300F"

private fun CloudMatchRequestStatusException.isInternalServerError(): Boolean =
    statusDescriptionToken().equals("INTERNAL_ERROR_STATUS", ignoreCase = true) ||
        normalizedUnifiedErrorCode() == "8A8C0000"

private fun CloudMatchRequestStatusException.statusDescriptionToken(): String? =
    statusDescription
        ?.trim()
        ?.takeWhile { !it.isWhitespace() }
        ?.takeIf { it.isNotEmpty() }

private fun CloudMatchRequestStatusException.normalizedUnifiedErrorCode(): String? {
    val raw = unifiedErrorCode?.trim()?.takeIf { it.isNotEmpty() } ?: return null
    val numeric = raw.toLongOrNull()
    return if (numeric != null) {
        (numeric and 0xFFFF_FFFFL).toString(16).uppercase()
    } else {
        raw.removePrefix("0x").removePrefix("0X").uppercase()
    }
}

private fun limitedModeStreamingMessage(gameTitle: String?): String {
    val title = gameTitle
        ?.replace(LaunchErrorWhitespaceRegex, " ")
        ?.trim()
        .orEmpty()
    return if (title.isNotBlank()) {
        "$title is only available for Priority or Ultimate members"
    } else {
        "This game is only available for Priority or Ultimate members"
    }
}
