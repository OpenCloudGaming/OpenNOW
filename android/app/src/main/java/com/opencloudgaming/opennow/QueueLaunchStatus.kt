package com.opencloudgaming.opennow

internal fun queueLaunchStatusText(state: OpenNowUiState): String {
    val session = state.streamSession
    val queuePosition = state.queuePosition?.takeIf { it > 0 }
        ?: session?.queuePosition?.takeIf { it > 0 }
    return when {
        queuePosition != null -> "Queue position $queuePosition"
        session?.seatSetupStep == 1 -> "Waiting for a rig"
        state.launchPhase.equals("Connecting stream", ignoreCase = true) -> "Connecting stream"
        state.launchPhase.equals("Resuming session", ignoreCase = true) -> "Resuming session"
        state.launchPhase.equals("Setting up rig", ignoreCase = true) -> "Setting up rig"
        else -> "Starting session"
    }
}

internal fun shouldShowQueueLaunchStatus(state: OpenNowUiState): Boolean {
    if (state.streamStatus == "idle") return false
    val sessionStatus = state.streamSession?.status
    return sessionStatus == null || sessionStatus !in setOf(2, 3)
}
