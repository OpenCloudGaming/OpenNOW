package com.opencloudgaming.opennow

import java.util.Locale

/** Allowlisted receiver counters only; neither RTP identities nor arbitrary stats are logged. */
internal class WebRtcRecoveryWindow {
    private data class Sample(val id: String, val ssrc: Double?, val atMs: Double, val values: Map<String, Double>)
    private var previous: Sample? = null

    fun reset() { previous = null }

    fun observe(id: String, atMs: Double, members: Map<String, Any>): WebRtcRecoveryInterval? {
        if (!isNewerStreamStatsSample(atMs, previous?.atMs)) return null
        val values = FIELDS.mapNotNull { key ->
            (members[key] as? Number)?.toDouble()?.takeIf { it.isFinite() }
                ?.takeIf { key == "packetsLost" || it >= 0.0 }?.let { key to it }
        }.toMap()
        val current = Sample(id, (members["ssrc"] as? Number)?.toDouble(), atMs, values)
        val old = previous
        previous = current
        if (old == null || old.id != id || old.ssrc != current.ssrc) return null
        if (listOf("packetsReceived", "bytesReceived", "framesDecoded").any { key ->
                val before = old.values[key]
                val after = values[key]
                before != null && after != null && after < before
            }) return null
        val deltas = values.mapNotNull { (key, after) ->
            old.values[key]?.let { before ->
                (after - before).takeIf { key == "packetsLost" || it >= 0.0 }?.let { key to it }
            }
        }.toMap()
        return WebRtcRecoveryInterval(atMs - old.atMs, deltas, values)
    }

    companion object {
        private val FIELDS = listOf(
            "packetsLost", "packetsReceived", "bytesReceived", "framesReceived", "framesDecoded",
            "nackCount", "pliCount", "firCount", "keyFramesDecoded", "retransmittedPacketsReceived",
            "fecPacketsReceived", "fecPacketsDiscarded", "packetsDiscarded", "framesDropped",
            "freezeCount", "totalFreezesDuration", "jitterBufferEmittedCount", "jitterBufferDelay",
            "jitterBufferTargetDelay", "jitterBufferMinimumDelay", "totalAssemblyTime",
            "framesAssembledFromMultiplePackets",
        )
    }
}

internal data class WebRtcRecoveryInterval(
    val elapsedMs: Double,
    val deltas: Map<String, Double>,
    val totals: Map<String, Double>,
) {
    fun meanMs(total: String, count: String): Double? {
        val denominator = deltas[count]?.takeIf { it > 0.0 } ?: return null
        return deltas[total]?.let { it * 1000.0 / denominator }
    }

    fun diagnostic(): String {
        fun number(value: Double?) = value?.let { String.format(Locale.US, "%.3f", it) } ?: "-"
        fun count(key: String) = deltas[key]?.toLong()?.toString() ?: "-"
        return "WebRTC recovery intervalMs=${number(elapsedMs)} " +
            "netLost=${count("packetsLost")} received=${count("packetsReceived")} " +
            "nack=${count("nackCount")} rtx=${count("retransmittedPacketsReceived")} " +
            "fec=${count("fecPacketsReceived")} fecDiscard=${count("fecPacketsDiscarded")} " +
            "packetDiscard=${count("packetsDiscarded")} frameDrop=${count("framesDropped")} " +
            "frames=${count("framesReceived")} decoded=${count("framesDecoded")} " +
            "keyframes=${count("keyFramesDecoded")} pli=${count("pliCount")} fir=${count("firCount")} " +
            "freezes=${count("freezeCount")} freezeMs=${number(deltas["totalFreezesDuration"]?.times(1000))} " +
            "rtxTotal=${totals["retransmittedPacketsReceived"]?.toLong() ?: "-"} " +
            "netLostTotal=${totals["packetsLost"]?.toLong() ?: "-"} " +
            "jbMeanMs=${number(meanMs("jitterBufferDelay", "jitterBufferEmittedCount"))} " +
            "jbTargetMs=${number(meanMs("jitterBufferTargetDelay", "jitterBufferEmittedCount"))} " +
            "jbMinMs=${number(meanMs("jitterBufferMinimumDelay", "jitterBufferEmittedCount"))} " +
            "assemblyMs=${number(meanMs("totalAssemblyTime", "framesAssembledFromMultiplePackets"))}"
    }
}
