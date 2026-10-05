import Foundation
import CoreGraphics

enum NativeStreamVideoCodec: String, CaseIterable {
    case h264 = "H264"
    case h265 = "H265"
    case av1 = "AV1"

    static func normalized(_ rawValue: String) -> NativeStreamVideoCodec? {
        switch rawValue.trimmingCharacters(in: .whitespacesAndNewlines).uppercased() {
        case "AUTO":
            return nil
        case "AVC", "H.264", "H264":
            return .h264
        case "HEVC", "H.265", "H265":
            return .h265
        case "AV01", "AV1":
            return .av1
        default:
            return nil
        }
    }
}

struct NativeStreamIceCredentials: Equatable {
    var ufrag: String
    var password: String
    var fingerprint: String
}

enum NativeStreamSDP {
    static let defaultInputProtocolVersion = 3
    static let defaultPartialReliableThresholdMs = 30
    static let partiallyReliableGamepadMaskAll = 0x0f
    private static let absoluteMinBitrateKbps = 5_000

    static func preferredCodecName(for settings: AppSettings) -> String {
        NativeStreamVideoCodec.normalized(settings.preferredCodec)?.rawValue ?? NativeStreamVideoCodec.h264.rawValue
    }

    static func extractPublicIP(from hostOrIP: String?) -> String? {
        guard let hostOrIP = hostOrIP?.trimmingCharacters(in: .whitespacesAndNewlines),
              !hostOrIP.isEmpty else {
            return nil
        }
        let ipv4Pattern = #"^\d{1,3}(?:\.\d{1,3}){3}$"#
        if hostOrIP.range(of: ipv4Pattern, options: .regularExpression) != nil {
            return hostOrIP
        }
        let firstLabel = hostOrIP.split(separator: ".", maxSplits: 1, omittingEmptySubsequences: true).first.map(String.init) ?? ""
        let parts = firstLabel.split(separator: "-").map(String.init)
        guard parts.count == 4, parts.allSatisfy({ Int($0) != nil }) else { return nil }
        return parts.joined(separator: ".")
    }

    static func fixServerIP(in sdp: String, serverIP: String?) -> String {
        fixServerEndpoint(
            in: sdp,
            serverIP: serverIP,
            mediaIP: nil,
            mediaPort: 0
        )
    }

    static func fixServerEndpoint(
        in sdp: String,
        serverIP: String?,
        mediaIP: String?,
        mediaPort: Int
    ) -> String {
        let normalizedMediaIP = mediaIP?.trimmingCharacters(in: .whitespacesAndNewlines)
        let hasMediaEndpoint = normalizedMediaIP?.isEmpty == false && (1...65_535).contains(mediaPort)
        let signalingIP = extractPublicIP(from: serverIP)
        guard hasMediaEndpoint || signalingIP != nil else { return sdp }
        let lineEnding = sdp.contains("\r\n") ? "\r\n" : "\n"
        let lines = sdp.replacingOccurrences(of: "\r\n", with: "\n").split(separator: "\n", omittingEmptySubsequences: false)
        let rewritten = lines.map { rawLine -> String in
            let line = String(rawLine)
            if hasMediaEndpoint, let normalizedMediaIP {
                return rewriteIceCandidateEndpoint(
                    line,
                    mediaIP: normalizedMediaIP,
                    mediaPort: mediaPort
                )
            }
            guard let signalingIP else { return line }
            return rewriteIceCandidate(
                line,
                replacementIP: signalingIP,
                replacementPort: nil,
                requiredAddress: "0.0.0.0"
            )
        }
        return rewritten.joined(separator: lineEnding)
    }

    static func rewriteIceCandidateEndpoint(
        _ candidate: String,
        mediaIP: String?,
        mediaPort: Int
    ) -> String {
        guard let mediaIP = mediaIP?.trimmingCharacters(in: .whitespacesAndNewlines),
              !mediaIP.isEmpty,
              (1...65_535).contains(mediaPort) else {
            return candidate
        }
        return rewriteIceCandidate(candidate, replacementIP: mediaIP, replacementPort: mediaPort)
    }

    private static func rewriteIceCandidate(
        _ candidate: String,
        replacementIP: String,
        replacementPort: Int?,
        requiredAddress: String? = nil
    ) -> String {
        guard let pattern = try? NSRegularExpression(
            pattern: #"^(a=candidate:\S+\s+\d+\s+\S+\s+\d+\s+|candidate:\S+\s+\d+\s+\S+\s+\d+\s+)(\S+)(\s+)(\d+)(?=\s|$)"#
        ),
        let match = pattern.firstMatch(
            in: candidate,
            range: NSRange(candidate.startIndex..<candidate.endIndex, in: candidate)
        ),
        let prefixRange = Range(match.range(at: 1), in: candidate),
        let addressRange = Range(match.range(at: 2), in: candidate),
        let spacingRange = Range(match.range(at: 3), in: candidate),
        let portRange = Range(match.range(at: 4), in: candidate),
        requiredAddress.map({ String(candidate[addressRange]) == $0 }) ?? true else {
            return candidate
        }
        let suffix = candidate[portRange.upperBound...]
        let port = replacementPort.map(String.init) ?? String(candidate[portRange])
        return "\(candidate[prefixRange])\(replacementIP)\(candidate[spacingRange])\(port)\(suffix)"
    }

    static func extractIceCredentials(from sdp: String) -> NativeStreamIceCredentials {
        var credentials = NativeStreamIceCredentials(ufrag: "", password: "", fingerprint: "")
        for line in normalizedLines(sdp) {
            if line.hasPrefix("a=ice-ufrag:") {
                credentials.ufrag = String(line.dropFirst("a=ice-ufrag:".count)).trimmingCharacters(in: .whitespacesAndNewlines)
            } else if line.hasPrefix("a=ice-pwd:") {
                credentials.password = String(line.dropFirst("a=ice-pwd:".count)).trimmingCharacters(in: .whitespacesAndNewlines)
            } else if line.hasPrefix("a=fingerprint:sha-256 ") {
                credentials.fingerprint = String(line.dropFirst("a=fingerprint:sha-256 ".count)).trimmingCharacters(in: .whitespacesAndNewlines)
            }
        }
        return credentials
    }

    static func extractIceUfrag(from sdp: String) -> String {
        extractIceCredentials(from: sdp).ufrag
    }

    static func parseInputProtocolVersion(from sdp: String) -> Int {
        boundedIntAttribute("ri.version", in: sdp, fallback: defaultInputProtocolVersion, range: 1...255)
    }

    static func parsePartialReliableThresholdMs(from sdp: String) -> Int {
        boundedIntAttribute(
            "ri.partialReliableThresholdMs",
            in: sdp,
            fallback: defaultPartialReliableThresholdMs,
            range: 1...5000
        )
    }

    static func parsePartiallyReliableGamepadMask(from sdp: String) -> Int {
        intAttribute(
            "ri.enablePartiallyReliableTransferGamepad",
            in: sdp,
            fallback: partiallyReliableGamepadMaskAll
        )
    }

    static func preferCodec(in sdp: String, codec: NativeStreamVideoCodec, preferTenBit: Bool) -> String {
        let lineEnding = sdp.contains("\r\n") ? "\r\n" : "\n"
        let lines = normalizedLines(sdp)
        var payloadCodecs: [String: String] = [:]
        var payloadOrder: [String] = []
        var fmtpByPayload: [String: String] = [:]
        var rtxApt: [String: String] = [:]
        var inVideo = false

        for line in lines {
            if line.hasPrefix("m=video") {
                inVideo = true
                payloadOrder = Array(line.split(separator: " ").dropFirst(3).map(String.init))
                continue
            }
            if line.hasPrefix("m="), inVideo {
                inVideo = false
            }
            guard inVideo else { continue }
            if line.hasPrefix("a=rtpmap:") {
                let rest = String(line.dropFirst("a=rtpmap:".count))
                let parts = rest.split(separator: " ", maxSplits: 1).map(String.init)
                guard parts.count == 2 else { continue }
                let name = parts[1]
                    .split(separator: "/", maxSplits: 1)
                    .first
                    .map(String.init)?
                    .uppercased()
                payloadCodecs[parts[0]] = normalizedCodecName(name)
            } else if line.hasPrefix("a=fmtp:") {
                let rest = String(line.dropFirst("a=fmtp:".count))
                let parts = rest.split(separator: " ", maxSplits: 1).map(String.init)
                guard parts.count == 2 else { continue }
                fmtpByPayload[parts[0]] = parts[1]
                if let apt = firstCapture(in: parts[1], pattern: #"(?:^|;)\s*apt=(\d+)"#) {
                    rtxApt[parts[0]] = apt
                }
            }
        }

        let target = codec.rawValue
        var preferredPayloads = payloadOrder.filter { payloadCodecs[$0] == target }
        if codec == .h264 {
            preferredPayloads.sort {
                h264ProfilePriority(fmtpByPayload[$0]) < h264ProfilePriority(fmtpByPayload[$1])
            }
        } else if codec == .h265 {
            preferredPayloads.sort {
                h265ProfilePriority(fmtpByPayload[$0], preferTenBit: preferTenBit)
                    < h265ProfilePriority(fmtpByPayload[$1], preferTenBit: preferTenBit)
            }
        }
        guard !preferredPayloads.isEmpty else { return sdp }

        var allowed = Set(preferredPayloads)
        for (rtx, apt) in rtxApt where allowed.contains(apt) && payloadCodecs[rtx] == "RTX" {
            allowed.insert(rtx)
        }

        var output: [String] = []
        inVideo = false
        for line in lines {
            if line.hasPrefix("m=video") {
                inVideo = true
                let parts = line.split(separator: " ").map(String.init)
                let retained = parts.dropFirst(3).filter { allowed.contains($0) }
                output.append((Array(parts.prefix(3)) + retained).joined(separator: " "))
                continue
            }
            if line.hasPrefix("m="), inVideo {
                inVideo = false
            }
            if inVideo, isPayloadSpecificVideoLine(line), let payload = payloadID(from: line), !allowed.contains(payload) {
                continue
            }
            output.append(line)
        }
        return output.joined(separator: lineEnding)
    }

    static func mungeAnswerSDP(_ sdp: String, maxBitrateKbps: Int) -> String {
        let lineEnding = sdp.contains("\r\n") ? "\r\n" : "\n"
        let lines = normalizedLines(sdp)
        var output: [String] = []
        for index in lines.indices {
            let line = lines[index]
            if line.hasPrefix("a=fmtp:"), line.contains("minptime="), !line.contains("stereo=1") {
                output.append("\(line);stereo=1")
            } else {
                output.append(line)
            }

            if line.hasPrefix("m=video") || line.hasPrefix("m=audio") {
                let next = index + 1 < lines.count ? lines[index + 1] : ""
                guard !next.hasPrefix("b=") else { continue }
                output.append(line.hasPrefix("m=video") ? "b=AS:\(maxBitrateKbps)" : "b=AS:128")
            }
        }
        return output.joined(separator: lineEnding)
    }

    static func rewriteH265TierFlag(in sdp: String, tierFlag: Int) -> (sdp: String, replacements: Int) {
        let payloads = h265PayloadTypes(in: sdp)
        guard !payloads.isEmpty else { return (sdp, 0) }
        let lineEnding = sdp.contains("\r\n") ? "\r\n" : "\n"
        var replacements = 0
        let output = normalizedLines(sdp).map { line -> String in
            guard line.hasPrefix("a=fmtp:"),
                  let payload = payloadID(from: line),
                  payloads.contains(payload) else {
                return line
            }
            let next = line.replacingOccurrences(
                of: "tier-flag=1",
                with: "tier-flag=\(tierFlag)",
                options: [.caseInsensitive]
            )
            if next != line { replacements += 1 }
            return next
        }
        return (output.joined(separator: lineEnding), replacements)
    }

    static func rewriteH265LevelIdByProfile(in sdp: String, maxLevelByProfile: [Int: Int]) -> (sdp: String, replacements: Int) {
        let payloads = h265PayloadTypes(in: sdp)
        guard !payloads.isEmpty, !maxLevelByProfile.isEmpty else { return (sdp, 0) }
        let lineEnding = sdp.contains("\r\n") ? "\r\n" : "\n"
        var replacements = 0
        let output = normalizedLines(sdp).map { line -> String in
            guard line.hasPrefix("a=fmtp:"),
                  let payload = payloadID(from: line),
                  payloads.contains(payload) else {
                return line
            }
            let rest = String(line.dropFirst("a=fmtp:".count))
            let params = rest.split(separator: " ", maxSplits: 1).dropFirst().first.map(String.init) ?? ""
            guard let profile = firstCapture(in: params, pattern: #"(?:^|;)\s*profile-id=(\d+)"#).flatMap(Int.init),
                  let level = firstCapture(in: params, pattern: #"(?:^|;)\s*level-id=(\d+)"#).flatMap(Int.init),
                  let maxLevel = maxLevelByProfile[profile],
                  level > maxLevel else {
                return line
            }
            let next = line.replacingOccurrences(
                of: #"(level-id=)(\d+)"#,
                with: "$1\(maxLevel)",
                options: [.regularExpression, .caseInsensitive]
            )
            if next != line { replacements += 1 }
            return next
        }
        return (output.joined(separator: lineEnding), replacements)
    }

    static func negotiatesCodec(_ sdp: String, codec: NativeStreamVideoCodec) -> Bool {
        var inVideo = false
        for line in normalizedLines(sdp) {
            if line.hasPrefix("m=video") {
                inVideo = true
                continue
            }
            if line.hasPrefix("m="), inVideo {
                inVideo = false
            }
            guard inVideo, line.hasPrefix("a=rtpmap:") else { continue }
            let codecName = String(line.dropFirst("a=rtpmap:".count))
                .split(separator: " ", maxSplits: 1)
                .dropFirst()
                .first?
                .split(separator: "/", maxSplits: 1)
                .first
                .map(String.init)
            if normalizedCodecName(codecName) == codec.rawValue {
                return true
            }
        }
        return false
    }

    static func describeNegotiatedVideo(from sdp: String) -> String {
        var inVideo = false
        var firstPayload: String?
        var codecByPayload: [String: String] = [:]
        var fmtpByPayload: [String: String] = [:]

        for line in normalizedLines(sdp) {
            if line.hasPrefix("m=video") {
                inVideo = true
                firstPayload = line.split(separator: " ").dropFirst(3).first.map(String.init)
                continue
            }
            if line.hasPrefix("m="), inVideo {
                break
            }
            guard inVideo else { continue }
            if line.hasPrefix("a=rtpmap:") {
                let rest = String(line.dropFirst("a=rtpmap:".count))
                let parts = rest.split(separator: " ", maxSplits: 1).map(String.init)
                guard parts.count == 2 else { continue }
                let codec = parts[1].split(separator: "/", maxSplits: 1).first.map(String.init) ?? parts[1]
                codecByPayload[parts[0]] = normalizedCodecName(codec)
            } else if line.hasPrefix("a=fmtp:") {
                let rest = String(line.dropFirst("a=fmtp:".count))
                let parts = rest.split(separator: " ", maxSplits: 1).map(String.init)
                guard parts.count == 2 else { continue }
                fmtpByPayload[parts[0]] = parts[1]
            }
        }

        guard let payload = firstPayload,
              let codec = codecByPayload[payload] else {
            return ""
        }
        let fmtp = fmtpByPayload[payload].flatMap { params -> String? in
            if codec == "H264",
               let profile = firstCapture(in: params, pattern: #"(?:^|;)\s*profile-level-id=([0-9a-f]+)"#) {
                return "plid \(profile)"
            }
            if codec == "H265",
               let profile = firstCapture(in: params, pattern: #"(?:^|;)\s*profile-id=(\d+)"#) {
                return "profile \(profile)"
            }
            if codec == "AV1",
               let profile = firstCapture(in: params, pattern: #"(?:^|;)\s*profile=(\d+)"#) {
                return "profile \(profile)"
            }
            return nil
        }
        return ["ans \(codec)", "pt \(payload)", fmtp].compactMap { $0 }.joined(separator: " ")
    }

    static func describeOfferedVideoCodecs(from sdp: String) -> String {
        var inVideo = false
        var codecByPayload: [String: String] = [:]
        var fmtpByPayload: [String: String] = [:]
        var payloadOrder: [String] = []

        for line in normalizedLines(sdp) {
            if line.hasPrefix("m=video") {
                inVideo = true
                payloadOrder = Array(line.split(separator: " ").dropFirst(3).map(String.init))
                continue
            }
            if line.hasPrefix("m="), inVideo {
                break
            }
            guard inVideo else { continue }
            if line.hasPrefix("a=rtpmap:") {
                let rest = String(line.dropFirst("a=rtpmap:".count))
                let parts = rest.split(separator: " ", maxSplits: 1).map(String.init)
                guard parts.count == 2 else { continue }
                let codec = parts[1].split(separator: "/", maxSplits: 1).first.map(String.init) ?? parts[1]
                codecByPayload[parts[0]] = normalizedCodecName(codec)
            } else if line.hasPrefix("a=fmtp:") {
                let rest = String(line.dropFirst("a=fmtp:".count))
                let parts = rest.split(separator: " ", maxSplits: 1).map(String.init)
                guard parts.count == 2 else { continue }
                fmtpByPayload[parts[0]] = parts[1]
            }
        }

        let videoPayloads = payloadOrder.compactMap { payload -> String? in
            guard let codec = codecByPayload[payload], codec != "RTX" else { return nil }
            if codec == "H264",
               let profile = fmtpByPayload[payload].flatMap({ firstCapture(in: $0, pattern: #"(?:^|;)\s*profile-level-id=([0-9a-f]+)"#) }) {
                return "\(codec):\(profile)"
            }
            if codec == "H265",
               let profile = fmtpByPayload[payload].flatMap({ firstCapture(in: $0, pattern: #"(?:^|;)\s*profile-id=(\d+)"#) }) {
                return "\(codec):p\(profile)"
            }
            if codec == "AV1",
               let profile = fmtpByPayload[payload].flatMap({ firstCapture(in: $0, pattern: #"(?:^|;)\s*profile=(\d+)"#) }) {
                return "\(codec):p\(profile)"
            }
            return codec
        }
        guard !videoPayloads.isEmpty else { return "" }
        return "offer \(videoPayloads.joined(separator: ","))"
    }

    static func buildNvstSDP(
        offerSDP: String,
        localAnswerSDP: String,
        profile: StreamVideoProfile,
        settings: AppSettings,
        codec: NativeStreamVideoCodec
    ) -> String {
        let credentials = extractIceCredentials(from: localAnswerSDP)
        let threshold = parsePartialReliableThresholdMs(from: offerSDP)
        let colorQuality = StreamColorQuality(rawValue: settings.preferredColorQuality) ?? .eightBit420
        let supportsHighBitDepth = codec == .h265 || codec == .av1
        let bitDepth = supportsHighBitDepth && (settings.hdrEnabled || colorQuality.bitDepth == 10) ? 10 : 8
        let hdrEnabled = supportsHighBitDepth && settings.hdrEnabled
        // NVIDIA CSC encoding: (color space << 1) | range; preserve full range.
        let encoderCscMode = hdrEnabled ? 5 : 3 // BT.2020 / BT.709
        let maxBitrate = max(absoluteMinBitrateKbps, profile.maxBitrateKbps)
        let minimumBitrate = max(absoluteMinBitrateKbps, Int((Double(maxBitrate) * 0.35).rounded()))
        let startupBitrate = max(minimumBitrate, Int((Double(maxBitrate) * 0.70).rounded()))
        let isHighFPS = profile.fps >= 90
        let is120FPS = profile.fps == 120
        let is240FPS = profile.fps >= 240
        let isAV1 = codec == .av1
        var lines = [
            "v=0",
            "o=SdpTest test_id_13 14 IN IPv4 127.0.0.1",
            "s=-",
            "t=0 0",
            "a=general.icePassword:\(credentials.password)",
            "a=general.iceUserNameFragment:\(credentials.ufrag)",
            "a=general.dtlsFingerprint:\(credentials.fingerprint)",
            "m=video 0 RTP/AVP",
            "a=msid:fbc-video-0",
            "a=vqos.fec.rateDropWindow:10",
            "a=vqos.fec.minRequiredFecPackets:2",
            "a=vqos.fec.repairMinPercent:5",
            "a=vqos.fec.repairPercent:5",
            "a=vqos.fec.repairMaxPercent:35",
            "a=vqos.dynamicStreamingMode:0",
            "a=vqos.drc.enable:0",
            "a=video.dx9EnableNv12:1",
            "a=video.dx9EnableHdr:1",
            "a=vqos.qpg.enable:1",
            "a=vqos.resControl.qp.qpg.featureSetting:7",
            "a=bwe.useOwdCongestionControl:1",
            "a=video.enableRtpNack:1",
            "a=vqos.bw.txRxLag.minFeedbackTxDeltaMs:200",
            "a=vqos.drc.bitrateIirFilterFactor:18",
            "a=video.packetSize:1140",
            "a=packetPacing.minNumPacketsPerGroup:15"
        ]

        if isHighFPS {
            lines += [
                // A fixed user-selected FPS must not enable host dynamic frame control.
                "a=vqos.dfc.enable:0",
                "a=vqos.resControl.dfc.useClientFpsPerf:0",
                "a=vqos.dfc.adjustResAndFps:0",
                "a=bwe.iirFilterFactor:8",
                "a=video.encoderFeatureSetting:47",
                "a=video.encoderPreset:6",
                "a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600",
                "a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9",
                "a=video.fbcDynamicFpsGrabTimeoutMs:\(is120FPS ? 6 : 18)",
                "a=vqos.resControl.cpmRtc.serverResolutionUpdateCoolDownCount:\(is120FPS ? 6000 : 12000)"
            ]
        } else {
            lines += [
                "a=vqos.dfc.enable:0",
                "a=vqos.dfc.adjustResAndFps:0"
            ]
        }

        if is240FPS {
            lines += [
                "a=video.enableNextCaptureMode:1",
                "a=vqos.maxStreamFpsEstimate:240",
                "a=video.videoSplitEncodeStripsPerFrame:3",
                "a=video.updateSplitEncodeStateDynamically:1",
                "a=vqos.rtcPreemptiveIdrSettings.minBurstNackSize:65535",
                "a=vqos.rtcPreemptiveIdrSettings.minNackPacketCaptureAgeMs:65535"
            ]
        }

        lines += [
            // OpenNOW-Mac's NVST settings also disable the resControl-scoped DFC.
            // Disabling only the top-level DFC leaves this controller unspecified.
            "a=vqos.resControl.enable:0",
            "a=vqos.resControl.dfc.adjustResAndFps:0",
            "a=vqos.resControl.dfc.maxResLevels:0",
            "a=vqos.adjustStreamingFpsDuringOutOfFocus:0",
            "a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1",
            "a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1",
            "a=vqos.resControl.cpmRtc.featureMask:0",
            "a=vqos.resControl.cpmRtc.enable:0",
            "a=vqos.resControl.cpmRtc.minResolutionPercent:100",
            "a=vqos.resControl.cpmRtc.resolutionChangeHoldonMs:999999",
            "a=packetPacing.numGroups:\(is120FPS ? 3 : 5)",
            "a=packetPacing.maxDelayUs:1000",
            "a=packetPacing.minNumPacketsFrame:10",
            "a=video.rtpNackQueueLength:1024",
            "a=video.rtpNackQueueMaxPackets:512",
            "a=video.rtpNackMaxPacketCount:25",
            "a=vqos.drc.qpMaxResThresholdAdj:4",
            "a=vqos.grc.qpMaxResThresholdAdj:4",
            "a=vqos.drc.iirFilterFactor:100"
        ]

        if isAV1 {
            lines += [
                "a=vqos.drc.minQpHeadroom:20",
                "a=vqos.drc.lowerQpThreshold:100",
                "a=vqos.drc.upperQpThreshold:200",
                "a=vqos.drc.minAdaptiveQpThreshold:180",
                "a=vqos.drc.qpMaxResThresholdAdj:20",
                "a=vqos.drc.qpCodecThresholdAdj:0",
                "a=vqos.dfc.minQpHeadroom:20",
                "a=vqos.dfc.qpLowerLimit:100",
                "a=vqos.dfc.qpMaxUpperLimit:200",
                "a=vqos.dfc.qpMinUpperLimit:180",
                "a=vqos.dfc.qpMaxResThresholdAdj:20",
                "a=vqos.dfc.qpCodecThresholdAdj:0",
                "a=vqos.grc.minQpHeadroom:20",
                "a=vqos.grc.lowerQpThreshold:100",
                "a=vqos.grc.upperQpThreshold:200",
                "a=vqos.grc.minAdaptiveQpThreshold:180",
                "a=vqos.grc.qpMaxResThresholdAdj:20",
                "a=vqos.grc.qpCodecThresholdAdj:0",
                "a=video.minQp:25",
                "a=video.enableAv1RcPrecisionFactor:1"
            ]
        }

        lines += [
            "a=video.clientViewportWd:\(profile.width)",
            "a=video.clientViewportHt:\(profile.height)",
            "a=video.maxFPS:\(profile.fps)",
            "a=video.initialBitrateKbps:\(startupBitrate)",
            "a=video.initialPeakBitrateKbps:\(maxBitrate)",
            "a=vqos.bw.maximumBitrateKbps:\(maxBitrate)",
            "a=vqos.bw.minimumBitrateKbps:\(minimumBitrate)",
            "a=vqos.bw.peakBitrateKbps:\(maxBitrate)",
            "a=vqos.bw.serverPeakBitrateKbps:\(maxBitrate)",
            "a=vqos.bw.enableBandwidthEstimation:1",
            "a=vqos.bw.disableBitrateLimit:0",
            "a=vqos.grc.maximumBitrateKbps:\(maxBitrate)",
            "a=vqos.grc.enable:0",
            "a=video.maxNumReferenceFrames:4",
            "a=video.mapRtpTimestampsToFrames:1",
            "a=video.encoderCscMode:\(encoderCscMode)",
            "a=video.dynamicRangeMode:\(hdrEnabled ? 1 : 0)",
            "a=video.bitDepth:\(bitDepth)",
            "a=video.scalingFeature1:\(isAV1 ? 1 : 0)",
            "a=video.prefilterParams.prefilterModel:0",
            "m=audio 0 RTP/AVP",
            "a=msid:audio",
            "m=mic 0 RTP/AVP",
            "a=msid:mic",
            "a=rtpmap:0 PCMU/8000",
            "m=application 0 RTP/AVP",
            "a=msid:input_1",
            "a=ri.partialReliableThresholdMs:\(threshold)",
            "a=ri.hidDeviceMask:4294967295",
            "a=ri.enablePartiallyReliableTransferGamepad:15",
            "a=ri.enablePartiallyReliableTransferHid:4294967295",
            ""
        ]

        return lines.joined(separator: "\n")
    }

    private static func normalizedLines(_ sdp: String) -> [String] {
        sdp.replacingOccurrences(of: "\r\n", with: "\n")
            .split(separator: "\n", omittingEmptySubsequences: false)
            .map(String.init)
    }

    private static func normalizedCodecName(_ name: String?) -> String {
        switch name?.uppercased() {
        case "AVC", "H.264", "H264":
            return "H264"
        case "HEVC", "H.265", "H265":
            return "H265"
        case "AV01", "AV1":
            return "AV1"
        default:
            return name?.uppercased() ?? ""
        }
    }

    private static func isPayloadSpecificVideoLine(_ line: String) -> Bool {
        line.hasPrefix("a=rtpmap:") || line.hasPrefix("a=fmtp:") || line.hasPrefix("a=rtcp-fb:")
    }

    private static func payloadID(from line: String) -> String? {
        guard let colon = line.firstIndex(of: ":") else { return nil }
        let rest = line[line.index(after: colon)...]
        return rest.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: true).first.map(String.init)
    }

    private static func h265PayloadTypes(in sdp: String) -> Set<String> {
        var inVideo = false
        var payloads = Set<String>()
        for line in normalizedLines(sdp) {
            if line.hasPrefix("m=video") {
                inVideo = true
                continue
            }
            if line.hasPrefix("m="), inVideo {
                inVideo = false
            }
            guard inVideo, line.hasPrefix("a=rtpmap:") else { continue }
            let rest = String(line.dropFirst("a=rtpmap:".count))
            let parts = rest.split(separator: " ", maxSplits: 1).map(String.init)
            guard parts.count == 2 else { continue }
            let codecName = parts[1].split(separator: "/", maxSplits: 1).first.map(String.init)
            if normalizedCodecName(codecName) == "H265" {
                payloads.insert(parts[0])
            }
        }
        return payloads
    }

    private static func h265ProfilePriority(_ fmtp: String?, preferTenBit: Bool) -> Int {
        let profile = fmtp.flatMap { firstCapture(in: $0, pattern: #"(?:^|;)\s*profile-id=(\d+)"#) }
        if preferTenBit {
            switch profile {
            case "2": return 0
            case "1": return 1
            default: return 2
            }
        } else {
            switch profile {
            case "1": return 0
            case nil: return 1
            case "2": return 2
            default: return 3
            }
        }
    }

    private static func h264ProfilePriority(_ fmtp: String?) -> Int {
        let profile = fmtp.flatMap { firstCapture(in: $0, pattern: #"(?:^|;)\s*profile-level-id=([0-9a-f]+)"#) }?.lowercased()
        guard let profile else { return 3 }
        if profile.hasPrefix("64") { return 0 }
        if profile.hasPrefix("4d") { return 1 }
        if profile.hasPrefix("42") { return 2 }
        return 3
    }

    private static func intAttribute(_ attribute: String, in sdp: String, fallback: Int) -> Int {
        guard let raw = firstCapture(
            in: sdp,
            pattern: #"(?im)^a=\#(NSRegularExpression.escapedPattern(for: attribute)):([^\r\n]+)"#,
            group: 1
        )?.trimmingCharacters(in: .whitespacesAndNewlines) else {
            return fallback
        }
        if raw.lowercased().hasPrefix("0x") {
            return Int(raw.dropFirst(2), radix: 16) ?? fallback
        }
        return Int(raw) ?? fallback
    }

    private static func boundedIntAttribute(_ attribute: String, in sdp: String, fallback: Int, range: ClosedRange<Int>) -> Int {
        min(max(intAttribute(attribute, in: sdp, fallback: fallback), range.lowerBound), range.upperBound)
    }

    private static func firstCapture(in text: String, pattern: String, group: Int = 1) -> String? {
        guard let regex = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive, .anchorsMatchLines]) else {
            return nil
        }
        let nsRange = NSRange(text.startIndex..<text.endIndex, in: text)
        guard let match = regex.firstMatch(in: text, range: nsRange),
              match.numberOfRanges > group,
              let range = Range(match.range(at: group), in: text) else {
            return nil
        }
        return String(text[range])
    }
}
