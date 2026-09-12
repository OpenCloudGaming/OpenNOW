package com.opencloudgaming.opennow

import java.net.InetAddress
import java.util.Locale

object SdpTools {
    data class RewriteResult(val sdp: String, val replacements: Int)

    fun fixServerIp(sdp: String, serverIp: String): String =
        fixServerEndpoint(sdp, serverIp, mediaConnectionInfo = null)

    fun fixServerEndpoint(sdp: String, serverIp: String, mediaConnectionInfo: MediaConnectionInfo?): String {
        val signalingIp = extractPublicIp(serverIp) ?: return sdp
        val mediaIp = mediaConnectionInfo?.ip?.let(::extractPublicIp) ?: signalingIp
        val mediaPort = mediaConnectionInfo?.port?.takeIf { it in 1..65535 }
        return sdp
            .replace(Regex("c=IN IP4 ([^\\r\\n]+)")) { match ->
                val address = match.groupValues[1]
                if (shouldRewriteRemoteEndpoint(address, mediaConnectionInfo != null)) "c=IN IP4 $mediaIp" else match.value
            }
            .replace(Regex("(a=candidate:\\S+\\s+\\d+\\s+\\w+\\s+\\d+\\s+)([^\\s]+)\\s+(\\d+)(\\s+)")) { match ->
                val address = match.groupValues[2]
                val port = match.groupValues[3]
                if (shouldRewriteRemoteEndpoint(address, mediaConnectionInfo != null)) {
                    "${match.groupValues[1]}$mediaIp ${mediaPort ?: port}${match.groupValues[4]}"
                } else {
                    match.value
                }
            }
    }

    fun preferCodec(sdp: String, settings: StreamSettings): String =
        preferCodec(sdp, settings.codec, settings.prefersTenBitVideo())

    fun preferCodec(sdp: String, codec: VideoCodec): String =
        preferCodec(sdp, codec, preferTenBit = codec != VideoCodec.H265)

    private fun preferCodec(sdp: String, codec: VideoCodec, preferTenBit: Boolean): String {
        val target = when (codec) {
            VideoCodec.H264 -> "H264"
            VideoCodec.H265 -> "H265"
            VideoCodec.AV1 -> "AV1"
        }
        val lineEnding = if (sdp.contains("\r\n")) "\r\n" else "\n"
        val lines = sdp.split(Regex("\\r?\\n"))
        var inVideo = false
        val codecByPt = mutableMapOf<String, String>()
        val rtxApt = mutableMapOf<String, String>()
        val fmtpByPt = mutableMapOf<String, String>()
        lines.forEach { line ->
            if (line.startsWith("m=video")) inVideo = true else if (line.startsWith("m=") && inVideo) inVideo = false
            if (inVideo && line.startsWith("a=rtpmap:")) {
                val rest = line.substringAfter(":")
                val pt = rest.substringBefore(" ")
                val name = rest.substringAfter(" ").substringBefore("/").uppercase(Locale.US).let { if (it == "HEVC") "H265" else it }
                codecByPt[pt] = name
            }
            if (inVideo && line.startsWith("a=fmtp:")) {
                val rest = line.substringAfter(":")
                val pt = rest.substringBefore(" ")
                val params = rest.substringAfter(" ", "")
                fmtpByPt[pt] = params
                Regex("(?:^|;)\\s*apt=(\\d+)").find(params)?.groupValues?.getOrNull(1)?.let { rtxApt[pt] = it }
            }
        }
        val preferred = codecByPt.filterValues { it == target }.keys.toMutableList()
        if (preferred.isEmpty()) return sdp
        if (codec == VideoCodec.H265) {
            preferred.sortBy { pt -> h265ProfilePriority(fmtpByPt[pt], preferTenBit) }
        }
        val allowed = preferred.toMutableSet()
        rtxApt.forEach { (rtx, apt) ->
            if (apt in preferred && codecByPt[rtx] == "RTX") allowed += rtx
        }
        // FlexFEC repairs the selected video stream regardless of its codec. The receiver
        // supports it, but removing it here prevents negotiation even when the server offers
        // repair packets and applyVideoCodecPreferences retains the receiver capability.
        allowed += codecByPt.filterValues { it == "FLEXFEC-03" }.keys
        val output = mutableListOf<String>()
        inVideo = false
        lines.forEach { line ->
            if (line.startsWith("m=video")) {
                inVideo = true
                val parts = line.split(Regex("\\s+"))
                val ordered = preferred + parts.drop(3).filter { it in allowed && it !in preferred }
                output += if (ordered.isNotEmpty()) (parts.take(3) + ordered).joinToString(" ") else line
                return@forEach
            }
            if (line.startsWith("m=") && inVideo) inVideo = false
            if (inVideo && (line.startsWith("a=rtpmap:") || line.startsWith("a=fmtp:") || line.startsWith("a=rtcp-fb:"))) {
                val pt = line.substringAfter(":").substringBefore(" ")
                val wildcardFeedback = line.startsWith("a=rtcp-fb:*") && pt == "*"
                if (pt !in allowed && !wildcardFeedback) return@forEach
            }
            output += line
        }
        return output.joinToString(lineEnding)
    }

    fun rewriteH265TierFlag(sdp: String, tierFlag: Int): RewriteResult {
        val payloads = h265PayloadTypes(sdp)
        if (payloads.isEmpty()) return RewriteResult(sdp, 0)
        val lineEnding = if (sdp.contains("\r\n")) "\r\n" else "\n"
        var replacements = 0
        val output = sdp.split(Regex("\\r?\\n")).map { line ->
            if (!line.startsWith("a=fmtp:")) return@map line
            val pt = line.substringAfter(":").substringBefore(" ")
            if (pt !in payloads) return@map line
            val next = line.replace(Regex("tier-flag=1", RegexOption.IGNORE_CASE), "tier-flag=$tierFlag")
            if (next != line) replacements += 1
            next
        }
        return RewriteResult(output.joinToString(lineEnding), replacements)
    }

    fun rewriteH265LevelIdByProfile(sdp: String, maxLevelByProfile: Map<Int, Int>): RewriteResult {
        val payloads = h265PayloadTypes(sdp)
        if (payloads.isEmpty() || maxLevelByProfile.isEmpty()) return RewriteResult(sdp, 0)
        val lineEnding = if (sdp.contains("\r\n")) "\r\n" else "\n"
        var replacements = 0
        val output = sdp.split(Regex("\\r?\\n")).map { line ->
            if (!line.startsWith("a=fmtp:")) return@map line
            val rest = line.substringAfter(":")
            val pt = rest.substringBefore(" ")
            val params = rest.substringAfter(" ", "")
            if (pt !in payloads || params.isBlank()) return@map line
            val profile = Regex("(?:^|;)\\s*profile-id=(\\d+)", RegexOption.IGNORE_CASE)
                .find(params)
                ?.groupValues
                ?.getOrNull(1)
                ?.toIntOrNull()
                ?: return@map line
            val level = Regex("(?:^|;)\\s*level-id=(\\d+)", RegexOption.IGNORE_CASE)
                .find(params)
                ?.groupValues
                ?.getOrNull(1)
                ?.toIntOrNull()
                ?: return@map line
            val maxLevel = maxLevelByProfile[profile] ?: return@map line
            if (level <= maxLevel) return@map line
            val next = line.replace(Regex("(level-id=)(\\d+)", RegexOption.IGNORE_CASE), "$1$maxLevel")
            if (next != line) replacements += 1
            next
        }
        return RewriteResult(output.joinToString(lineEnding), replacements)
    }

    fun negotiatesCodec(sdp: String, codec: VideoCodec): Boolean {
        val target = when (codec) {
            VideoCodec.H264 -> "H264"
            VideoCodec.H265 -> "H265"
            VideoCodec.AV1 -> "AV1"
        }
        var inVideo = false
        sdp.split(Regex("\\r?\\n")).forEach { line ->
            if (line.startsWith("m=video")) {
                inVideo = true
                return@forEach
            }
            if (line.startsWith("m=") && inVideo) {
                inVideo = false
            }
            if (!inVideo || !line.startsWith("a=rtpmap:")) return@forEach
            val codecName = line.substringAfter(" ")
                .substringBefore("/")
                .uppercase(Locale.US)
                .let { if (it == "HEVC") "H265" else it }
            if (codecName == target) return true
        }
        return false
    }

    private fun h265PayloadTypes(sdp: String): Set<String> {
        var inVideo = false
        val payloads = mutableSetOf<String>()
        sdp.split(Regex("\\r?\\n")).forEach { line ->
            if (line.startsWith("m=video")) {
                inVideo = true
                return@forEach
            }
            if (line.startsWith("m=") && inVideo) {
                inVideo = false
            }
            if (!inVideo || !line.startsWith("a=rtpmap:")) return@forEach
            val rest = line.substringAfter(":")
            val pt = rest.substringBefore(" ")
            val codecName = rest.substringAfter(" ")
                .substringBefore("/")
                .uppercase(Locale.US)
                .let { if (it == "HEVC") "H265" else it }
            if (pt.isNotBlank() && codecName == "H265") payloads += pt
        }
        return payloads
    }

    private fun h265ProfilePriority(fmtp: String?, preferTenBit: Boolean): Int {
        val profileId = Regex("(?:^|;)\\s*profile-id=(\\d+)")
            .find(fmtp.orEmpty())
            ?.groupValues
            ?.getOrNull(1)
        return if (preferTenBit) {
            when (profileId) {
                "2" -> 0
                "1" -> 1
                else -> 2
            }
        } else {
            when (profileId) {
                "1" -> 0
                null -> 1
                "2" -> 2
                else -> 3
            }
        }
    }

    private fun StreamSettings.prefersTenBitVideo(): Boolean =
        hdrEnabled ||
            colorQuality == ColorQuality.TenBit420 ||
            colorQuality == ColorQuality.TenBit444

    fun mungeAnswerSdp(sdp: String, maxBitrateKbps: Int): String {
        val lineEnding = if (sdp.contains("\r\n")) "\r\n" else "\n"
        val out = mutableListOf<String>()
        val lines = sdp.split(Regex("\\r?\\n"))
        lines.forEachIndexed { index, line ->
            val rewritten = if (line.startsWith("a=fmtp:") && line.contains("minptime=") && !line.contains("stereo=1")) "$line;stereo=1" else line
            out += rewritten
            if ((line.startsWith("m=video") || line.startsWith("m=audio")) && !lines.getOrNull(index + 1).orEmpty().startsWith("b=")) {
                out += if (line.startsWith("m=video")) "b=AS:$maxBitrateKbps" else "b=AS:128"
            }
        }
        return out.joinToString(lineEnding)
    }

    /**
     * Replaces the existing b=AS bandwidth line in the video section of an SDP string, leaving
     * audio untouched. Unlike [mungeAnswerSdp] this is safe to call repeatedly on the same SDP
     * (idempotent), which is what a mid-stream bitrate ceiling update needs.
     */
    fun replaceVideoBitrateInSdp(sdp: String, maxBitrateKbps: Int): String {
        val lineEnding = if (sdp.contains("\r\n")) "\r\n" else "\n"
        val lines = sdp.split(Regex("\r?\n"))
        val out = mutableListOf<String>()
        var inVideoSection = false
        var bitrateReplaced = false
        for (line in lines) {
            if (line.startsWith("m=")) {
                inVideoSection = line.startsWith("m=video")
                bitrateReplaced = false
            }
            if (inVideoSection && !bitrateReplaced && line.startsWith("b=AS:")) {
                out += "b=AS:$maxBitrateKbps"
                bitrateReplaced = true
                continue
            }
            out += line
        }
        return out.joinToString(lineEnding)
    }

    fun parseInputProtocolVersion(sdp: String): Int =
        Regex("a=ri\\.version:(\\d+)").find(sdp)?.groupValues?.getOrNull(1)?.toIntOrNull()
            ?: DEFAULT_INPUT_PROTOCOL_VERSION

    fun parsePartialReliableThresholdMs(sdp: String): Int =
        Regex("a=ri\\.partialReliableThresholdMs:(\\d+)")
            .find(sdp)
            ?.groupValues
            ?.getOrNull(1)
            ?.toIntOrNull()
            ?.coerceIn(1, 5000)
            ?: 30

    fun parsePartiallyReliableGamepadMask(sdp: String): Int =
        parseRiIntegerAttribute(
            sdp,
            "ri.enablePartiallyReliableTransferGamepad",
            PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL,
        )

    fun parseHidDeviceMask(sdp: String): Int =
        parseRiIntegerAttribute(sdp, "ri.hidDeviceMask", HID_DEVICE_MASK_ALL)

    fun parsePartiallyReliableHidMask(sdp: String): Int =
        parseRiIntegerAttribute(
            sdp,
            "ri.enablePartiallyReliableTransferHid",
            HID_DEVICE_MASK_ALL,
        )

    fun supportsPartiallyReliableHidInput(
        hidDeviceMask: Int,
        partiallyReliableHidMask: Int,
        inputType: Int,
    ): Boolean {
        if (inputType !in 0..31) return false
        val inputMask = 1 shl inputType
        return (hidDeviceMask and inputMask) != 0 &&
            (partiallyReliableHidMask and inputMask) != 0
    }

    fun buildNvstSdp(offerSdp: String, settings: StreamSettings, localAnswer: String): String {
        val (width, height) = streamResolutionPixels(settings)
        val ufrag = Regex("a=ice-ufrag:([^\\r\\n]+)").find(localAnswer)?.groupValues?.getOrNull(1)?.trim().orEmpty()
        val pwd = Regex("a=ice-pwd:([^\\r\\n]+)").find(localAnswer)?.groupValues?.getOrNull(1)?.trim().orEmpty()
        val fingerprint = Regex("a=fingerprint:sha-256 ([^\\r\\n]+)").find(localAnswer)?.groupValues?.getOrNull(1)?.trim().orEmpty()
        val threshold = Regex("a=ri\\.partialReliableThresholdMs:(\\d+)").find(offerSdp)?.groupValues?.getOrNull(1)?.toIntOrNull() ?: 30
        val hidDeviceMask = parseHidDeviceMask(offerSdp)
        val partiallyReliableHidMask = parsePartiallyReliableHidMask(offerSdp)
        val bitDepth = if (settings.hdrEnabled || settings.colorQuality == ColorQuality.TenBit420 || settings.colorQuality == ColorQuality.TenBit444) 10 else 8
        val bitrate = StreamNetworkAdaptation.bitrateRange(settings.maxBitrateMbps)
        val maxBitrate = bitrate.maximumKbps
        val minBitrate = bitrate.minimumKbps
        val initialBitrate = bitrate.initialKbps
        val isHighFps = settings.fps > 60
        val is90Fps = settings.fps == 90
        val is120Fps = settings.fps == 120
        val isAtLeast240Fps = settings.fps >= 240
        val isAv1 = settings.codec == VideoCodec.AV1
        val minTargetFrameTimeUs = ((1_000_000L * 95L) / (settings.fps.coerceAtLeast(1) * 100L))
            .coerceAtLeast(1000L)
        return buildList {
            add("v=0")
            add("o=SdpTest test_id_13 14 IN IPv4 127.0.0.1")
            add("s=-")
            add("t=0 0")
            add("a=general.icePassword:$pwd")
            add("a=general.iceUserNameFragment:$ufrag")
            add("a=general.dtlsFingerprint:$fingerprint")
            add("m=video 0 RTP/AVP")
            add("a=msid:fbc-video-0")
            add("a=vqos.fec.rateDropWindow:10")
            add("a=vqos.fec.minRequiredFecPackets:2")
            add("a=vqos.fec.repairMinPercent:5")
            add("a=vqos.fec.repairPercent:5")
            add("a=vqos.fec.repairMaxPercent:35")
            add("a=vqos.bllFec.enable:0")
            add("a=vqos.dynamicStreamingMode:${StreamNetworkAdaptation.DYNAMIC_STREAMING_MODE}")
            add("a=vqos.drc.enable:1")
            add("a=vqos.dfc.enable:0")
            add("a=vqos.dfc.adjustResAndFps:0")
            add("a=vqos.calculateAvgVideoStreamingBitrate:1")
            add("a=video.dx9EnableNv12:1")
            add("a=video.dx9EnableHdr:${if (settings.hdrEnabled) 1 else 0}")
            add("a=vqos.qpg.enable:1")
            add("a=vqos.resControl.qp.qpg.featureSetting:7")
            add("a=video.adaptiveQuantization.spatialAQSetting:7")
            add("a=video.adaptiveQuantization.temporalAQSetting:0")
            add("a=video.adaptiveQuantization.spatialAQStrength:12")
            add("a=video.adaptiveQuantization.qpThresholdAdjPercent:2")
            add("a=video.adaptiveQuantization.saqAdaptMinQpThresholdPercent:40")
            add("a=video.adaptiveQuantization.saqAdaptMaxQpThresholdPercent:100")
            add("a=video.adaptiveQuantization.saqAdaptDecayStrengthX100:250")
            add("a=video.adaptiveQuantization.perfAdjEnablement:1")
            add("a=video.framePacing.mode:2")
            add("a=video.framePacing.pid.minTargetFrameTimeUs:$minTargetFrameTimeUs")
            add("a=bwe.useOwdCongestionControl:1")
            add("a=video.enableRtpNack:1")
            add("a=vqos.bw.txRxLag.minFeedbackTxDeltaMs:200")
            add("a=vqos.drc.bitrateIirFilterFactor:18")
            add("a=video.packetSize:1140")
            add("a=packetPacing.version:3")
            add("a=packetPacing.mode:1")
            add("a=packetPacing.minNumPacketsPerGroup:15")
            add("a=packetPacing.enableAccurateSleep:1")
            add("a=packetPacing.enableSmoothTransition:1")
            add("a=packetPacing.allowFpsBasedToggle:1")
            add("a=vqos.relaxMaxBitrate.overrideAvgBitrateThresholdPercent:4")
            add("a=vqos.relaxMaxBitrate.customAvgBitrateThresholdPercent:65")
            add("a=vqos.relaxMaxBitrate.overrideAvgQpThresholdPercent:7")
            add("a=vqos.relaxMaxBitrate.customAvgQpThresholdPercent:51")
            add("a=vqos.relaxMaxBitrate.iirFilterFactor:120")
            add("a=vqos.qpDelta.qpDeltaMaxPercent:10")
            add("a=vqos.qpDelta.qpDeltaSurfaceAdjustmentStrengthPercent:70")
            add("a=vqos.qpDelta.qpDeltaVbvUsageFactorPercentH264:100")
            add("a=vqos.qpDelta.qpDeltaVbvUsageFactorPercentH265:100")
            add("a=vqos.qpDelta.qpDeltaVbvUsageFactorPercentAv1:100")
            add("a=vqos.qpDelta.qpDeltaMinPercent:60")
            add("a=vqos.qpDelta.qpDeltaIirFactor:60")
            add("a=vqos.qpDelta.qpDeltaThrottlePercent:100")
            if (isHighFps) {
                add("a=vqos.resControl.dfc.useClientFpsPerf:0")
                add("a=bwe.iirFilterFactor:8")
                add("a=video.encoderFeatureSetting:47")
                add("a=video.encoderPreset:6")
                val captureTuning = when {
                    is90Fps -> 9 to 11
                    is120Fps -> 6 to 9
                    isAtLeast240Fps -> 18 to 9
                    else -> null
                }
                captureTuning?.let { (grabTimeoutMs, decodeThresholdMs) ->
                    add("a=video.fbcDynamicFpsGrabTimeoutMs:$grabTimeoutMs")
                    add("a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:$decodeThresholdMs")
                }
                add("a=vqos.maxStreamFpsEstimate:${settings.fps}")
            }
            if (isAtLeast240Fps) {
                add("a=video.enableNextCaptureMode:1")
                val splitEncodeStrips = if (isAv1 && width * height >= HIGH_RESOLUTION_AV1_SPLIT_ENCODE_PIXELS) 63 else 3
                add("a=video.videoSplitEncodeStripsPerFrame:$splitEncodeStrips")
                add("a=video.updateSplitEncodeStateDynamically:1")
                add("a=vqos.rtcPreemptiveIdrSettings.minBurstNackSize:65535")
                add("a=vqos.rtcPreemptiveIdrSettings.minNackPacketCaptureAgeMs:65535")
            }
            add("a=vqos.adjustStreamingFpsDuringOutOfFocus:0")
            add("a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1")
            add("a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1")
            add("a=vqos.resControl.cpmRtc.featureMask:0")
            add("a=vqos.resControl.cpmRtc.enable:0")
            add("a=packetPacing.numGroups:${if (is120Fps) 3 else 5}")
            add("a=packetPacing.maxDelayUs:1000")
            add("a=packetPacing.minNumPacketsFrame:10")
            add("a=video.rtpNackQueueLength:1024")
            add("a=video.rtpNackQueueMaxPackets:512")
            add("a=video.rtpNackMaxPacketCount:25")
            add("a=vqos.drc.qpMaxResThresholdAdj:4")
            add("a=vqos.grc.qpMaxResThresholdAdj:4")
            add("a=vqos.drc.iirFilterFactor:100")
            if (isAv1) {
                add("a=vqos.drc.minQpHeadroom:20")
                add("a=vqos.drc.lowerQpThreshold:100")
                add("a=vqos.drc.upperQpThreshold:200")
                add("a=vqos.drc.minAdaptiveQpThreshold:180")
                add("a=vqos.drc.qpCodecThresholdAdj:0")
                add("a=vqos.drc.qpMaxResThresholdAdj:20")
                add("a=vqos.dfc.minQpHeadroom:20")
                add("a=vqos.dfc.qpLowerLimit:100")
                add("a=vqos.dfc.qpMaxUpperLimit:200")
                add("a=vqos.dfc.qpMinUpperLimit:180")
                add("a=vqos.dfc.qpMaxResThresholdAdj:20")
                add("a=vqos.dfc.qpCodecThresholdAdj:0")
                add("a=vqos.grc.minQpHeadroom:20")
                add("a=vqos.grc.lowerQpThreshold:100")
                add("a=vqos.grc.upperQpThreshold:200")
                add("a=vqos.grc.minAdaptiveQpThreshold:180")
                add("a=vqos.grc.qpMaxResThresholdAdj:20")
                add("a=vqos.grc.qpCodecThresholdAdj:0")
                add("a=video.minQp:25")
                add("a=video.enableAv1RcPrecisionFactor:1")
            }
            add("a=video.clientViewportWd:$width")
            add("a=video.clientViewportHt:$height")
            add("a=video.maxFPS:${settings.fps}")
            add("a=video.initialBitrateKbps:$initialBitrate")
            add("a=video.initialPeakBitrateKbps:$initialBitrate")
            add("a=vqos.bw.maximumBitrateKbps:$maxBitrate")
            add("a=vqos.bw.minimumBitrateKbps:$minBitrate")
            add("a=vqos.bw.peakBitrateKbps:$maxBitrate")
            add("a=vqos.bw.serverPeakBitrateKbps:$maxBitrate")
            add("a=vqos.bw.enableBandwidthEstimation:1")
            add("a=vqos.bw.disableBitrateLimit:0")
            add("a=vqos.grc.maximumBitrateKbps:$maxBitrate")
            add("a=vqos.grc.enable:0")
            add("a=video.maxNumReferenceFrames:4")
            add("a=video.mapRtpTimestampsToFrames:1")
            add("a=video.encoderCscMode:3")
            add("a=video.dynamicRangeMode:0")
            add("a=video.bitDepth:$bitDepth")
            // Keep codec-specific horizontal scaling disabled. Network resolution
            // adaptation is owned by DRC above, with the requested viewport retained.
            add("a=video.scalingFeature1:0")
            add("a=video.prefilterParams.prefilterModel:0")
            add("m=audio 0 RTP/AVP")
            add("a=msid:audio")
            add("m=mic 0 RTP/AVP")
            add("a=msid:mic")
            add("a=rtpmap:0 PCMU/8000")
            add("m=application 0 RTP/AVP")
            add("a=msid:input_1")
            add("a=ri.partialReliableThresholdMs:$threshold")
            add("a=ri.hidDeviceMask:${hidDeviceMask.toUInt()}")
            add("a=ri.enablePartiallyReliableTransferGamepad:15")
            add("a=ri.enablePartiallyReliableTransferHid:${partiallyReliableHidMask.toUInt()}")
            add("")
        }.joinToString("\n")
    }

    private fun extractPublicIp(hostOrIp: String): String? {
        if (Regex("^\\d{1,3}(\\.\\d{1,3}){3}$").matches(hostOrIp)) return hostOrIp
        val first = hostOrIp.substringBefore(".")
        val parts = first.split("-")
        return if (parts.size == 4 && parts.all { it.all(Char::isDigit) }) parts.joinToString(".") else null
    }

    private fun shouldRewriteRemoteEndpoint(address: String, hasMediaEndpoint: Boolean): Boolean {
        val remoteAddress = parseIpv4Address(address) ?: return false
        if (remoteAddress.inetAddress.isAnyLocalAddress) return true
        return hasMediaEndpoint && remoteAddress.isUnroutable()
    }

    private data class RemoteIpv4Address(
        val octets: List<Int>,
        val inetAddress: InetAddress,
    ) {
        fun isUnroutable(): Boolean =
            inetAddress.isLoopbackAddress ||
                inetAddress.isSiteLocalAddress ||
                inetAddress.isLinkLocalAddress ||
                inetAddress.isMulticastAddress ||
                isCarrierGradeNatAddress(octets)
    }

    private fun parseIpv4Address(address: String): RemoteIpv4Address? {
        val octets = address.split(".").map { it.toIntOrNull() ?: return null }
        if (octets.size != 4 || octets.any { it !in 0..255 }) return null
        val inetAddress = InetAddress.getByAddress(octets.map { it.toByte() }.toByteArray())
        return RemoteIpv4Address(octets, inetAddress)
    }

    private fun isCarrierGradeNatAddress(octets: List<Int>): Boolean {
        return octets[0] == 100 && octets[1] in 64..127
    }

    private fun parseRiIntegerAttribute(sdp: String, attribute: String, fallback: Int): Int {
        val escaped = Regex.escape(attribute)
        val raw = Regex("a=$escaped:([^\\r\\n]+)", RegexOption.IGNORE_CASE)
            .find(sdp)
            ?.groupValues
            ?.getOrNull(1)
            ?.trim()
            ?: return fallback
        val parsed = if (raw.startsWith("0x", ignoreCase = true)) {
            raw.drop(2).toULongOrNull(16)?.toInt()
        } else {
            raw.toLongOrNull()?.toInt()
        }
        return parsed ?: fallback
    }

    private const val HIGH_RESOLUTION_AV1_SPLIT_ENCODE_PIXELS = 2_764_800
    private const val PARTIALLY_RELIABLE_GAMEPAD_MASK_ALL = 0x0f
    private const val HID_DEVICE_MASK_ALL = -1
}
