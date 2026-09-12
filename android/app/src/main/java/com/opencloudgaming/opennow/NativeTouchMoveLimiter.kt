package com.opencloudgaming.opennow

/**
 * Keeps only the newest absolute-touch MOVE inside a short send interval.
 *
 * Unlike relative mouse motion, intermediate absolute positions do not need to be accumulated.
 * Replaying all of them after sender or network backpressure only makes the remote finger trail
 * behind the real one. The leading MOVE is immediate and one latest trailing snapshot is retained.
 */
internal class NativeTouchMoveLimiter {
    private var lastSentAtMs: Long? = null
    private var lastSent: List<TouchRecord>? = null
    private var pending: List<TouchRecord>? = null

    val hasPendingMove: Boolean
        get() = pending != null

    fun offer(
        touches: List<TouchRecord>,
        nowMs: Long,
        minimumIntervalMs: Long,
    ): List<TouchRecord>? {
        require(minimumIntervalMs > 0L)
        if (touches.isEmpty()) return null
        val snapshot = touches.toList()
        if (snapshot.hasSameAbsolutePositions(lastSent)) {
            pending = null
            return null
        }
        if (snapshot.hasSameAbsolutePositions(pending)) {
            pending = snapshot
            return null
        }
        val lastSent = lastSentAtMs
        if (lastSent == null || nowMs - lastSent >= minimumIntervalMs) {
            pending = null
            lastSentAtMs = nowMs
            this.lastSent = snapshot
            return snapshot
        }
        pending = snapshot
        return null
    }

    fun delayUntilFlushMs(nowMs: Long, minimumIntervalMs: Long): Long? {
        require(minimumIntervalMs > 0L)
        if (pending == null) return null
        val lastSent = lastSentAtMs ?: return 0L
        return (minimumIntervalMs - (nowMs - lastSent)).coerceAtLeast(0L)
    }

    fun flush(nowMs: Long): List<TouchRecord>? {
        val snapshot = pending ?: return null
        pending = null
        lastSentAtMs = nowMs
        lastSent = snapshot
        return snapshot
    }

    fun reset() {
        lastSentAtMs = null
        lastSent = null
        pending = null
    }

    private fun List<TouchRecord>.hasSameAbsolutePositions(other: List<TouchRecord>?): Boolean =
        other != null && size == other.size && indices.all { index ->
            val current = this[index]
            val previous = other[index]
            current.slot == previous.slot &&
                current.phase == previous.phase &&
                current.x == previous.x &&
                current.y == previous.y
        }
}
