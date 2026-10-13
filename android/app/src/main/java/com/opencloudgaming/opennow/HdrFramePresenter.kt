package com.opencloudgaming.opennow

import android.os.Handler
import android.os.HandlerThread
import android.os.Process
import android.view.Choreographer
import java.util.Locale

/** Owns the HDR display clock. Surface destruction/recreation cannot keep an old ticker alive. */
internal class HdrFramePresenter(requestedFps: Int) {
    private val lock = Any()
    private val clock = HdrPresentationClock(requestedFps)
    private var thread: HandlerThread? = null
    private var epoch = 0
    private var statsAtNs = 0L
    private var presented = 0
    private var immediate = 0
    private var leadTotalNs = 0L
    private var leadMaxNs = 0L
    private var lastCaps = 0
    private var lastRebases = 0
    private var displayPeriodNs = 0L

    fun start(refreshRate: Float) = start { refreshRate }

    fun start(refreshRate: () -> Float) = synchronized(lock) {
        stopLocked()
        val generation = epoch
        fun refreshPeriod() = (1_000_000_000.0 /
            refreshRate().takeIf { it.isFinite() && it > 0f }.let { it ?: 60f }).toLong()
        var nominalPeriod = refreshPeriod()
        val worker = HandlerThread("OpenNOW-HDR-vsync", Process.THREAD_PRIORITY_DISPLAY).also { it.start() }
        thread = worker
        Handler(worker.looper).post {
            val choreographer = Choreographer.getInstance()
            var previousTick = 0L
            var observedPeriod = nominalPeriod
            var lastRateCheckNs = 0L
            val callback = object : Choreographer.FrameCallback {
                override fun doFrame(frameTimeNanos: Long) {
                    synchronized(lock) {
                        if (epoch != generation || thread !== worker) return
                        if (frameTimeNanos - lastRateCheckNs >= 1_000_000_000L) {
                            val currentPeriod = refreshPeriod()
                            if (kotlin.math.abs(currentPeriod - nominalPeriod) > nominalPeriod / 10) {
                                observedPeriod = currentPeriod
                                previousTick = 0L
                            }
                            nominalPeriod = currentPeriod
                            lastRateCheckNs = frameTimeNanos
                        }
                        val delta = frameTimeNanos - previousTick
                        if (previousTick > 0 && delta in nominalPeriod / 2..nominalPeriod * 3 / 2) {
                            observedPeriod = (observedPeriod * 9 + delta) / 10
                        }
                        previousTick = frameTimeNanos
                        displayPeriodNs = observedPeriod
                        clock.observeVsync(frameTimeNanos, observedPeriod)
                        choreographer.postFrameCallback(this)
                    }
                }
            }
            synchronized(lock) {
                if (epoch == generation && thread === worker) choreographer.postFrameCallback(callback)
            }
        }
    }

    fun present(buffer: HdrSurfaceBuffer): Boolean {
        val now = System.nanoTime()
        val (generation, target) = synchronized(lock) { epoch to clock.nextPresentationTimeNs(now) }
        val accepted = if (target == null) buffer.present() else buffer.presentAt(target)
        if (accepted) synchronized(lock) {
            if (epoch == generation) {
                if (statsAtNs == 0L) statsAtNs = now
                presented++
                if (target == null) immediate++
                val lead = if (target == null) 0L else (target - now).coerceAtLeast(0L)
                leadTotalNs += lead
                leadMaxNs = maxOf(leadMaxNs, lead)
                if (now - statsAtNs >= 1_000_000_000L) {
                    NativeInputDiagnostics.addRetained("hdr.presentation", String.format(Locale.US,
                        "HDR presentation submitted=%d immediate=%d leadMeanMs=%.3f leadMaxMs=%.3f " +
                            "refreshPeriodMs=%.3f burstCaps=%d gapRebases=%d",
                        presented, immediate, leadTotalNs / presented / 1e6, leadMaxNs / 1e6,
                        displayPeriodNs / 1e6, clock.burstCaps - lastCaps, clock.gapRebases - lastRebases))
                    lastCaps = clock.burstCaps
                    lastRebases = clock.gapRebases
                    statsAtNs = now
                    presented = 0
                    immediate = 0
                    leadTotalNs = 0
                    leadMaxNs = 0
                }
            }
        }
        return accepted
    }

    fun stop() = synchronized(lock) { stopLocked() }

    private fun stopLocked() {
        epoch++
        clock.reset()
        thread?.quitSafely()
        thread = null
        statsAtNs = 0L
        presented = 0
        immediate = 0
        leadTotalNs = 0
        leadMaxNs = 0
        lastCaps = 0
        lastRebases = 0
        displayPeriodNs = 0L
    }
}
