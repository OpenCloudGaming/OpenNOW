package com.opencloudgaming.opennow

/**
 * Coalesces high-rate analog snapshots while keeping the leading state change immediate.
 *
 * Gamepad packets contain the complete current state, so intermediate stick positions can be
 * replaced by the newest position. Motion events also carry D-pad hats and analog triggers;
 * changes to those controls must be sent immediately so a quick tap cannot be coalesced away.
 */
internal class GamepadStateBurstLimiter(
    private val minimumIntervalMs: Long,
) {
    init {
        require(minimumIntervalMs > 0L)
    }

    private var lastSentAtMs: Long? = null
    private var pendingControllerId: Int? = null
    private val motionControlsByController = mutableMapOf<Int, Long>()
    private val neutralSticksByController = mutableMapOf<Int, Int>()

    fun offer(
        controllerId: Int,
        nowMs: Long,
        hatButtons: Int = 0,
        leftTrigger: Int = 0,
        rightTrigger: Int = 0,
        neutralSticks: Int? = null,
    ): Int? {
        val controls = (hatButtons.toLong() shl 16) or
            ((leftTrigger.toLong() and 0xff) shl 8) or (rightTrigger.toLong() and 0xff)
        val controlsChanged = controls != (motionControlsByController.put(controllerId, controls) ?: 0L)
        val previousNeutralSticks = neutralSticks?.let { neutralSticksByController.put(controllerId, it) }
        val stickReleased = neutralSticks != null && previousNeutralSticks != null &&
            (neutralSticks and previousNeutralSticks.inv()) != 0
        val lastSent = lastSentAtMs
        if (controlsChanged || stickReleased || lastSent == null || nowMs - lastSent >= minimumIntervalMs) {
            pendingControllerId = null
            lastSentAtMs = nowMs
            return controllerId
        }
        pendingControllerId = controllerId
        return null
    }

    fun delayUntilFlushMs(nowMs: Long): Long? {
        if (pendingControllerId == null) return null
        val lastSent = lastSentAtMs ?: return 0L
        return (minimumIntervalMs - (nowMs - lastSent)).coerceAtLeast(0L)
    }

    fun flush(nowMs: Long): Int? {
        val controllerId = pendingControllerId ?: return null
        pendingControllerId = null
        lastSentAtMs = nowMs
        return controllerId
    }

    fun reset() {
        lastSentAtMs = null
        pendingControllerId = null
        motionControlsByController.clear()
        neutralSticksByController.clear()
    }
}
