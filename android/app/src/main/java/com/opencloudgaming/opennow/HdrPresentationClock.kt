package com.opencloudgaming.opennow

import kotlin.math.roundToLong

/** Schedules decoded output only. Never drops an encoded reference or holds a Java codec slot. */
internal class HdrPresentationClock(requestedFps: Int) {
    private val framePeriodNs = 1_000_000_000.0 / requestedFps.coerceIn(1, 240)
    private var vsyncNs = 0L
    private var refreshPeriodNs = 0L
    private var nextIdealNs: Double? = null
    var burstCaps = 0
        private set
    var gapRebases = 0
        private set

    fun observeVsync(frameTimeNs: Long, periodNs: Long) {
        if (frameTimeNs <= vsyncNs || periodNs <= 0) return
        if (refreshPeriodNs != 0L && kotlin.math.abs(periodNs - refreshPeriodNs) > refreshPeriodNs / 10) {
            nextIdealNs = null
        }
        vsyncNs = frameTimeNs
        refreshPeriodNs = periodNs
    }

    fun reset() {
        vsyncNs = 0L
        refreshPeriodNs = 0L
        nextIdealNs = null
        burstCaps = 0
        gapRebases = 0
    }

    fun nextPresentationTimeNs(nowNs: Long): Long? {
        val period = refreshPeriodNs
        if (period <= 0 || nowNs < vsyncNs || nowNs - vsyncNs > 250_000_000L) {
            nextIdealNs = null
            return null // Missing/stale display clock: preserve immediate presentation.
        }
        // Two to three refresh periods of initial headroom absorb normal arrival jitter. A gap
        // rebases immediately instead of replaying an old schedule after network recovery.
        val firstTick = vsyncNs + ((nowNs - vsyncNs) / period + 3) * period
        val latestTick = firstTick
        val earliestTick = firstTick - period * 2
        var ideal = nextIdealNs ?: firstTick.toDouble()
        if (ideal < nowNs) {
            // The old schedule is exhausted after an upstream stall. Present the first
            // recovered picture one tick sooner, then restore the normal bounded headroom.
            // A normal next frame may be 2 ticks later; never replay a backlog of old targets.
            gapRebases++
            nextIdealNs = firstTick + maxOf(framePeriodNs, period.toDouble())
            return firstTick - period
        }
        var target = (vsyncNs + ((ideal - vsyncNs) / period).roundToLong() * period)
            .coerceAtLeast(earliestTick)
        if (target > latestTick) {
            // A delivery burst cannot fill Surface with arbitrarily distant future buffers.
            // Multiple outputs for this tick let Surface display the newest decoded picture.
            target = latestTick
            ideal = target.toDouble()
            burstCaps++
        }
        nextIdealNs = ideal + maxOf(framePeriodNs, period.toDouble())
        return target
    }
}
