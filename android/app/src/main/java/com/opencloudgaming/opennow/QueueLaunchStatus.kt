package com.opencloudgaming.opennow

import android.content.Context

internal enum class QueueLaunchStatusKind {
    QueuePosition,
    WaitingForRig,
    ConnectingStream,
    ResumingSession,
    SettingUpRig,
    StartingSession,
}

internal data class QueueLaunchStatus(
    val kind: QueueLaunchStatusKind,
    val queuePosition: Int? = null,
)

private fun SessionInfo.isInQueueMode(): Boolean =
    seatSetupStep == 1 || (queuePosition ?: 0) > 1

private fun SessionInfo?.isPastQueue(): Boolean =
    this != null && (status in setOf(2, 3) || (seatSetupStep in 3..5 && !isInQueueMode()))

internal fun queueLaunchStatus(
    state: OpenNowUiState,
    queuePosition: Int? = queueDisplayPosition(state),
): QueueLaunchStatus {
    val session = state.streamSession
    return when {
        state.launchPhase.equals("Resuming session", ignoreCase = true) -> QueueLaunchStatus(QueueLaunchStatusKind.ResumingSession)
        state.launchPhase.equals("Connecting stream", ignoreCase = true) -> QueueLaunchStatus(QueueLaunchStatusKind.ConnectingStream)
        session.isPastQueue() ->
            QueueLaunchStatus(
                if (session?.seatSetupStep in 3..4 && session?.status !in setOf(2, 3)) {
                    QueueLaunchStatusKind.SettingUpRig
                } else {
                    QueueLaunchStatusKind.ConnectingStream
                },
            )
        queuePosition != null -> QueueLaunchStatus(QueueLaunchStatusKind.QueuePosition, queuePosition)
        session?.seatSetupStep == 1 -> QueueLaunchStatus(QueueLaunchStatusKind.WaitingForRig)
        state.launchPhase.equals("Setting up rig", ignoreCase = true) -> QueueLaunchStatus(QueueLaunchStatusKind.SettingUpRig)
        else -> QueueLaunchStatus(QueueLaunchStatusKind.StartingSession)
    }
}

internal fun queueLaunchStatusText(state: OpenNowUiState): String {
    val status = queueLaunchStatus(state)
    return when (status.kind) {
        QueueLaunchStatusKind.QueuePosition -> "Queue position ${status.queuePosition}"
        QueueLaunchStatusKind.WaitingForRig -> "Waiting for a rig"
        QueueLaunchStatusKind.ConnectingStream -> "Connecting..."
        QueueLaunchStatusKind.ResumingSession -> "Resuming session"
        QueueLaunchStatusKind.SettingUpRig -> "Setting up rig"
        QueueLaunchStatusKind.StartingSession -> "Starting session"
    }
}

internal fun localizedQueueLaunchStatusText(context: Context, state: OpenNowUiState): String {
    val localizedContext = localizedAndroidContext(context)
    val status = queueLaunchStatus(state)
    return when (status.kind) {
        QueueLaunchStatusKind.QueuePosition -> localizedContext.getString(R.string.queue_position, status.queuePosition)
        QueueLaunchStatusKind.WaitingForRig -> localizedContext.getString(R.string.queue_waiting_for_rig)
        QueueLaunchStatusKind.ConnectingStream -> localizedContext.getString(R.string.queue_connecting_stream)
        QueueLaunchStatusKind.ResumingSession -> localizedContext.getString(R.string.queue_resuming_session)
        QueueLaunchStatusKind.SettingUpRig -> localizedContext.getString(R.string.queue_setting_up_rig)
        QueueLaunchStatusKind.StartingSession -> localizedContext.getString(R.string.queue_starting_session)
    }
}

internal fun queueDisplayPosition(state: OpenNowUiState): Int? {
    val session = state.streamSession
    if (session.isPastQueue() || state.streamStatus == "connecting") return null
    return state.queuePosition?.takeIf { it > 0 } ?: queueDisplayPosition(session)
}

internal fun queueDisplayPosition(session: SessionInfo?): Int? {
    if (session.isPastQueue()) return null
    return session?.queuePosition?.takeIf { it > 0 }
}

/** Keep the lowest observed queue position for one session, ignoring later higher snapshots. */
internal fun lowerOnlyQueuePosition(previous: Int?, incoming: Int?): Int? =
    if (incoming == null) null else previous?.coerceAtMost(incoming) ?: incoming

internal fun stableQueueDisplayPosition(state: OpenNowUiState, session: SessionInfo): Int? {
    val incoming = queueDisplayPosition(session)
    val previous = state.streamSession
        ?.takeIf { it.sessionId == session.sessionId }
        ?.let { queueDisplayPosition(state) }
    return lowerOnlyQueuePosition(previous, incoming)
}

internal fun shouldShowQueueLaunchStatus(state: OpenNowUiState): Boolean {
    if (state.streamStatus == "idle") return false
    val sessionStatus = state.streamSession?.status
    return sessionStatus == null || sessionStatus !in setOf(2, 3)
}

internal fun isActivelyQueued(state: OpenNowUiState): Boolean =
    queueDisplayPosition(state) != null ||
        (state.streamStatus == "queue" && state.launchPhase.equals("Queue", ignoreCase = true))

internal class QueueReadyNotificationTracker {
    private var queuedSessionId: String? = null

    fun update(state: OpenNowUiState): Boolean {
        if (isActivelyQueued(state)) {
            state.streamSession?.sessionId?.let { queuedSessionId = it }
            return false
        }

        if (state.streamStatus == "idle") {
            reset()
            return false
        }
        if (state.streamStatus != "connecting") return false

        val currentSessionId = state.streamSession?.sessionId
        val completedObservedQueue = currentSessionId != null && currentSessionId == queuedSessionId
        reset()
        return completedObservedQueue
    }

    private fun reset() {
        queuedSessionId = null
    }
}
