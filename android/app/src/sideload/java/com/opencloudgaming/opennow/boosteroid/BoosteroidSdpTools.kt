// Adapted from OpenStroid Android. See android/app/src/sideload/assets/licenses/OpenStroid-NOTICE.txt.
package com.opencloudgaming.opennow.boosteroid

import com.opencloudgaming.opennow.VideoCodec
import com.opencloudgaming.opennow.redactDiagnosticUrl
import com.opencloudgaming.opennow.streamDiagnosticId
import java.util.Locale
import org.webrtc.IceCandidate

internal fun boosteroidSignalingUrlForDiagnostics(url: String, sessionId: String): String =
    redactDiagnosticUrl(url).replace(sessionId, streamDiagnosticId(sessionId))

internal fun IceCandidate.diagnosticSummary(): String {
    val raw = sdp
    val protocol = Regex("""\s(udp|tcp)\s""", RegexOption.IGNORE_CASE)
        .find(raw)
        ?.value
        ?.trim()
        ?.lowercase(Locale.US)
        ?: "unknown"
    val type = Regex("""\styp\s+([a-z0-9]+)""", RegexOption.IGNORE_CASE)
        .find(raw)
        ?.groupValues
        ?.getOrNull(1)
        ?.lowercase(Locale.US)
        ?: "unknown"
    val address = Regex("""candidate:\S+\s+\d+\s+\S+\s+\d+\s+([^\s]+)\s+(\d+)""")
        .find(raw)
        ?.let { match -> "${match.groupValues[1]}:${match.groupValues[2]}" }
        ?: "unknown"
    return "mid=${sdpMid.orEmpty()} line=$sdpMLineIndex type=$type protocol=$protocol address=$address raw=${raw.take(240)}"
}

internal fun sdpDiagnosticSummary(label: String, sdp: String): String {
    val lines = sdp.split(Regex("\\r?\\n")).filter { it.isNotBlank() }
    val media = lines.filter { it.startsWith("m=") }.joinToString("|").take(180)
    val candidateEndpoints = lines
        .filter { it.startsWith("a=candidate:") }
        .mapNotNull { line ->
            Regex("""a=candidate:\S+\s+\d+\s+\S+\s+\d+\s+([^\s]+)\s+(\d+)""")
                .find(line)
                ?.let { match -> "${match.groupValues[1]}:${match.groupValues[2]}" }
        }
        .distinct()
        .joinToString(limit = 6)
    val codecs = lines
        .filter { it.startsWith("a=rtpmap:") }
        .mapNotNull { line -> line.substringAfter(' ', "").substringBefore('/').takeIf { it.isNotBlank() } }
        .distinct()
        .take(12)
        .joinToString(",")
    val candidates = lines.count { it.startsWith("a=candidate:") }
    val hasIce = lines.any { it.startsWith("a=ice-ufrag:") } && lines.any { it.startsWith("a=ice-pwd:") }
    val hasFingerprint = lines.any { it.startsWith("a=fingerprint:") }
    return "$label lines=${lines.size} media=$media codecs=$codecs candidates=$candidates endpoints=$candidateEndpoints ice=$hasIce fingerprint=$hasFingerprint"
}

internal object BoosteroidSdpTools {
    private data class AllowedCodec(val mediaType: String, val codec: String)

    fun prepareOffer(sdp: String, selectedCodec: VideoCodec, gatewayCodec: String): String {
        val allowAv1 = selectedCodec == VideoCodec.AV1 && !gatewayCodec.equals("H264", ignoreCase = true)
        val allowed = buildList {
            if (allowAv1) add(AllowedCodec("video", "av1"))
            add(AllowedCodec("video", "h264"))
            add(AllowedCodec("video", "rtx"))
            add(AllowedCodec("video", "flexfec-03"))
            add(AllowedCodec("audio", "red"))
            add(AllowedCodec("audio", "opus"))
        }
        val removedExtensions = setOf(
            "urn:ietf:params:rtp-hdrext:ssrc-audio-level",
            "urn:ietf:params:rtp-hdrext:sdes:mid",
            "urn:ietf:params:rtp-hdrext:sdes:rtp-stream-id",
            "urn:ietf:params:rtp-hdrext:sdes:repaired-rtp-stream-id",
            "urn:ietf:params:rtp-hdrext:toffset",
            "urn:3gpp:video-orientation",
        )
        return filterAllowedCodecs(sdp, allowed)
            .lineSequence()
            .filterNot { line -> line.startsWith("a=extmap:") && removedExtensions.any(line::contains) }
            .joinToString(if (sdp.contains("\r\n")) "\r\n" else "\n")
    }

    private fun filterAllowedCodecs(sdp: String, allowed: List<AllowedCodec>): String {
        if (sdp.isBlank() || allowed.isEmpty()) return sdp
        val eol = if (sdp.contains("\r\n")) "\r\n" else "\n"
        val sections = mutableListOf<MutableList<String>>()
        var current = mutableListOf<String>()
        sdp.split(eol).forEach { line ->
            if (line.startsWith("m=")) {
                if (current.isNotEmpty()) sections += current
                current = mutableListOf(line)
            } else {
                current += line
            }
        }
        if (current.isNotEmpty()) sections += current

        return sections.joinToString(eol) { section ->
            val firstLine = section.firstOrNull()
            if (firstLine == null || !firstLine.startsWith("m=")) return@joinToString section.joinToString(eol)
            val mediaType = firstLine.substringAfter("m=").substringBefore(' ').lowercase(Locale.US)
            if (mediaType != "audio" && mediaType != "video") return@joinToString section.joinToString(eol)

            val codecMap = linkedMapOf<String, String>()
            val fmtpMap = linkedMapOf<String, String>()
            val rtxAssociations = linkedMapOf<String, String>()
            val redundancyAssociations = linkedMapOf<String, List<String>>()
            section.forEach { line ->
                when {
                    line.startsWith("a=rtpmap:") -> {
                        val body = line.substringAfter("a=rtpmap:")
                        val payload = body.substringBefore(' ').trim()
                        val codec = body.substringAfter(' ', "").substringBefore('/').lowercase(Locale.US)
                        if (payload.isNotBlank() && codec.isNotBlank()) codecMap[payload] = codec
                    }
                    line.startsWith("a=fmtp:") -> {
                        val body = line.substringAfter("a=fmtp:").trim()
                        val payload = body.substringBefore(' ').trim()
                        val params = body.substringAfter(' ', "")
                        if (payload.isBlank() || params.isBlank()) return@forEach
                        fmtpMap[payload] = params
                        Regex("""(?:^|;)\s*apt=(\d+)""", RegexOption.IGNORE_CASE)
                            .find(params)
                            ?.groupValues
                            ?.getOrNull(1)
                            ?.let { rtxAssociations[payload] = it }
                            ?: params.trim()
                                .takeIf { Regex("""\d+(?:/\d+)+""").matches(it) }
                                ?.split('/')
                                ?.let { redundancyAssociations[payload] = it }
                    }
                }
            }

            val allowedPayloads = linkedSetOf<String>()
            codecMap.forEach { (payload, codec) ->
                if (codec != "rtx" && allowed.any { it.mediaType == mediaType && it.codec == codec }) {
                    allowedPayloads += payload
                }
            }
            fmtpMap.forEach { (payload, params) ->
                when (codecMap[payload]) {
                    "h264" -> {
                        val profile = Regex("""profile-level-id=([0-9a-f]{6})""", RegexOption.IGNORE_CASE)
                            .find(params)
                            ?.groupValues
                            ?.getOrNull(1)
                        if (profile != null && profile.take(2).toIntOrNull(16)?.let { it >= 0x64 } == true) {
                            allowedPayloads -= payload
                            return@forEach
                        }
                        val packetizationMode = Regex("""packetization-mode=(\d+)""", RegexOption.IGNORE_CASE)
                            .find(params)
                            ?.groupValues
                            ?.getOrNull(1)
                            ?.toIntOrNull()
                        if (packetizationMode == 0) allowedPayloads -= payload
                    }
                    "av1" -> {
                        val profile = Regex("""profile=(\d+)""", RegexOption.IGNORE_CASE)
                            .find(params)
                            ?.groupValues
                            ?.getOrNull(1)
                            ?.toIntOrNull()
                        if (profile != null && profile != 0) allowedPayloads -= payload
                    }
                }
            }
            rtxAssociations.forEach { (rtx, primary) ->
                if (primary in allowedPayloads) allowedPayloads += rtx
            }
            redundancyAssociations.forEach { (redundant, primaries) ->
                if (primaries.all { it in allowedPayloads }) allowedPayloads += redundant
            }
            if (allowedPayloads.isEmpty()) return@joinToString section.joinToString(eol)

            val orderedPayloads = mutableListOf<String>()
            allowed.forEach { item ->
                codecMap.forEach { (payload, codec) ->
                    if (item.mediaType == mediaType && item.codec == codec && payload in allowedPayloads) {
                        orderedPayloads += payload
                    }
                }
            }
            codecMap.keys.filterTo(orderedPayloads) { it in allowedPayloads && it !in orderedPayloads }
            val mParts = firstLine.split(' ')
            val filtered = mutableListOf((mParts.take(3) + orderedPayloads).joinToString(" "))
            section.drop(1).forEach { line ->
                if (line.startsWith("a=rtpmap:") || line.startsWith("a=fmtp:") || line.startsWith("a=rtcp-fb:")) {
                    val payload = line.substringAfter(':').substringBefore(' ').trim()
                    if (payload !in allowedPayloads) return@forEach
                }
                filtered += line
            }
            filtered.joinToString(eol)
        }
    }
}
