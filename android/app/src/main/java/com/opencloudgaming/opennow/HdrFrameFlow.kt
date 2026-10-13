package com.opencloudgaming.opennow

import java.util.Locale

/** Local monotonic times only; source timestamps are represented as deltas, never as identities. */
internal class HdrFrameFlow(private val fps: Int, private val log: (String, String) -> Unit) {
    internal class Frame internal constructor(
        internal val owner: HdrFrameFlow,
        internal val epoch: Long,
        val sequence: Long,
        val arrivalNs: Long,
        val sourceStepNs: Long?,
        val inputGapNs: Long?,
    ) {
        internal var submittedNs = 0L
        internal var outputNs = 0L
        internal var sinkNs = 0L
        fun sink(nowNs: Long) = owner.sink(this, nowNs)
        fun released(startNs: Long, endNs: Long, targetNs: Long?, presented: Boolean) =
            owner.released(this, startNs, endNs, targetNs, presented)
    }
    private var epoch = 0L
    private var sequence = 0L
    private var lastInput = 0L
    private var lastSource = 0L
    private var lastOutput = 0L
    private var lastSink = 0L
    private var reportAt = 0L
    private var inputs = 0
    private var outputs = 0
    private var sinks = 0
    private var presents = 0
    private var drops = 0
    private var inputGapMax = 0L
    private var outputGapMax = 0L
    private var sinkGapMax = 0L
    private var postDecodeMax = 0L
    private var releaseCallMax = 0L
    private val eventCounts = mutableMapOf<String, Int>()
    private val thresholdNs = maxOf(20_000_000L, 2_000_000_000L / fps.coerceIn(1, 240))

    @Synchronized fun input(nowNs: Long, sourceNs: Long): Frame {
        report(nowNs)
        val gap = lastInput.takeIf { it > 0 && nowNs >= it }?.let { nowNs - it }
        val sourceStep = lastSource.takeIf { it > 0 && sourceNs > it }?.let { sourceNs - it }
        val frame = Frame(this, epoch, ++sequence, nowNs, sourceStep, gap)
        lastInput = nowNs
        lastSource = sourceNs
        inputs++
        inputGapMax = maxOf(inputGapMax, gap ?: 0)
        if (gap != null && gap > thresholdNs) event("input", frame, nowNs, gap)
        return frame
    }

    @Synchronized fun submitted(frame: Frame, nowNs: Long) {
        if (current(frame)) frame.submittedNs = nowNs
    }

    @Synchronized fun output(frame: Frame, nowNs: Long) {
        if (!current(frame)) return
        val gap = gap(nowNs, lastOutput)
        lastOutput = nowNs
        frame.outputNs = nowNs
        outputs++
        outputGapMax = maxOf(outputGapMax, gap)
        if (gap > thresholdNs) event("output", frame, nowNs, gap)
    }

    @Synchronized private fun sink(frame: Frame, nowNs: Long) {
        if (!current(frame)) return
        val gap = gap(nowNs, lastSink)
        lastSink = nowNs
        frame.sinkNs = nowNs
        sinks++
        sinkGapMax = maxOf(sinkGapMax, gap)
        val postDecode = gap(nowNs, frame.outputNs)
        postDecodeMax = maxOf(postDecodeMax, postDecode)
        if (gap > thresholdNs || postDecode > thresholdNs) event("sink", frame, nowNs, gap)
    }

    @Synchronized private fun released(frame: Frame, startNs: Long, endNs: Long, targetNs: Long?, presented: Boolean) {
        if (!current(frame)) return
        if (presented) presents++ else drops++
        val callNs = gap(endNs, startNs)
        releaseCallMax = maxOf(releaseCallMax, callNs)
        if (callNs > thresholdNs) event("release", frame, endNs, callNs, targetNs)
        if (presented && (frame.inputGapNs ?: 0L) > thresholdNs) {
            event("recovery", frame, endNs, callNs, targetNs)
        }
    }

    @Synchronized fun reset() {
        epoch++
        sequence = 0
        lastInput = 0
        lastSource = 0
        lastOutput = 0
        lastSink = 0
        reportAt = 0
        clearWindow()
    }

    private fun current(frame: Frame) = frame.owner === this && frame.epoch == epoch
    private fun gap(now: Long, before: Long) = if (before > 0 && now >= before) now - before else 0L
    private fun ms(ns: Long?) = ns?.let { String.format(Locale.US, "%.3f", it / 1e6) } ?: "-"
    private fun event(stage: String, frame: Frame, now: Long, gap: Long, target: Long? = null) {
        val count = eventCounts[stage] ?: 0
        if (count >= 2) return // Bounded diagnostics must not become the source of a delivery stall.
        eventCounts[stage] = count + 1
        log("hdr.flow.$stage", "HDR flow stage=$stage epoch=$epoch seq=${frame.sequence} monoNs=$now " +
            "gapMs=${ms(gap)} inputGapMs=${ms(frame.inputGapNs)} sourceStepMs=${ms(frame.sourceStepNs)} " +
            "arrivalNs=${frame.arrivalNs} submitNs=${frame.submittedNs} outputNs=${frame.outputNs} sinkNs=${frame.sinkNs} " +
            "queueMs=${ms(if (frame.submittedNs > 0) gap(frame.submittedNs, frame.arrivalNs) else null)} " +
            "codecMs=${ms(if (frame.outputNs > 0) gap(frame.outputNs, frame.submittedNs) else null)} " +
            "postDecodeMs=${ms(if (frame.sinkNs > 0) gap(frame.sinkNs, frame.outputNs) else null)} targetNs=${target ?: 0}")
    }
    private fun report(now: Long) {
        if (reportAt == 0L) { reportAt = now; return }
        if (now - reportAt < 1_000_000_000L) return
        log("hdr.flow.summary", "HDR flow stage=summary epoch=$epoch monoNs=$now intervalMs=${ms(now - reportAt)} " +
            "inputs=$inputs outputs=$outputs sinks=$sinks presented=$presents dropped=$drops " +
            "inputGapMaxMs=${ms(inputGapMax)} outputGapMaxMs=${ms(outputGapMax)} sinkGapMaxMs=${ms(sinkGapMax)} " +
            "postDecodeMaxMs=${ms(postDecodeMax)} releaseCallMaxMs=${ms(releaseCallMax)}")
        reportAt = now
        clearWindow()
    }
    private fun clearWindow() {
        inputs = 0; outputs = 0; sinks = 0; presents = 0; drops = 0
        inputGapMax = 0; outputGapMax = 0; sinkGapMax = 0; postDecodeMax = 0; releaseCallMax = 0
        eventCounts.clear()
    }
}
