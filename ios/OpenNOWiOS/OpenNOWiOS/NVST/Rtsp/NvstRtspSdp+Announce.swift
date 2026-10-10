//  The client's ANNOUNCE body: what the client offers the seat. NvstRtspSdp.swift owns parsing
//  what the seat offers back.
//

import Foundation

extension NvstRtspSdp {
    /// Encoder tuning and timers copied from the official client's resolved config. Useful to
    /// match it exactly, but none of it is needed to start a stream, so it stays opt-in until a
    /// live run shows the seat tolerates it.
    private static let videoExtendedAllowlist: [(String, String)] = [
        ("mtuPacketSizeControl", "1"),
        ("rtpNackMaxRetries", "3"),
        ("rtpNackVersion", "2"),
        ("encoderPreset", "5"),
        ("encoderCscMode", "2"),
        ("encoderHdrCscMode", "4"),
        ("encoderFeatureSetting", "47"),
        ("encoderLtrFeatureSetting", "2"),
        ("encoderMultiRefFeatureSetting", "2"),
        ("encoderMultiPass", "0"),
        ("initialQp", "35"),
        ("minQp", "14"),
        ("lowQpBitrateKbps", "5000"),
        ("keyFrameScale", "4"),
        ("maxNumReferenceFrames", "0"),
        ("numTemporalLayers", "0"),
        ("refPicInvalidation", "1"),
        ("streamRecoveryType", "2"),
        ("fullFrameAssembly", "1"),
        ("transferProtocol", "1"),
        ("dejitterBufferLengthMs", "52"),
        ("pingBackIntervalMs", "3000"),
        ("pingBackTimeoutMs", "10000"),
        ("sendFirstFrameTimeoutMs", "50000"),
        ("sendFrameTimeoutMs", "7000"),
        ("timeoutLengthMs", "8000"),
    ]






    private static let generalExtendedAllowlist: [(String, String)] = [
        ("customMessageOnCC", "1"),
        ("maxQosMessagesSize", "1071"),
        ("transitSelectionSettings.enableDynamicTransitSelection", "1"),
        ("transitSelectionSettings.selectionEnforcementMethod", "1"),
    ]


    /// Strips key material out of a single SDP line so the ANNOUNCE body can still be diffed against
    /// a vendor capture from the logs. The SRTP master key is the sole confidentiality and integrity
    /// root for the whole video stream, and the ICE passwords authenticate the connectivity checks;
    /// the sinks behind the negotiator's logger (`~/Library/Logs`, the unified log, Sentry, the
    /// uploadable diagnostics bundle) all outlive the session, so neither may reach them.
    /// The key *ID* and the ICE *ufrag* are not secret and stay readable.
    public static func redactedForLog(_ line: String) -> String {
        guard let colon = line.firstIndex(of: ":") else { return line }
        let name = line[line.startIndex..<colon].lowercased()
        let carriesSecret = name.hasSuffix("encryptionkey")
            || name.contains("pwd")
            || name.contains("password")
        guard carriesSecret else { return line }
        return "\(line[line.startIndex..<colon]):[redacted-secret]"
    }

    /// `(bitDepth, chromaFormat)` for a session colour tier string. `chromaFormat` uses the
    /// `chroma_format_idc` convention (1 = 4:2:0, 2 = 4:2:2, 3 = 4:4:4); see `AnnounceOptions`.
    public static func colorFormat(forColorQuality colorQuality: String) -> (bitDepth: Int, chromaFormat: Int) {
        let normalized = colorQuality.lowercased()
        let bitDepth = normalized.hasPrefix("10bit") ? 10 : 8
        let chromaFormat = normalized.hasSuffix("444") ? 3 : (normalized.hasSuffix("422") ? 2 : 1)
        return (bitDepth, chromaFormat)
    }

    public static func buildAnnounceSdp(_ options: AnnounceOptions) -> String {
        var lines: [String] = [
            "v=0",
            // The official macOS handshake origin username is "unknown".
            "o=unknown 0 14 IN IPv4 127.0.0.1",
            "s=NVIDIA Streaming Client",
        ]
        lines.append(contentsOf: announceAttributes(options).lines)
        lines.append("a=x-nv-runtime.videoSrtp:1")
        lines.append(contentsOf: announceSecurityLines(options))
        lines.append(contentsOf: announcePortLines(options))
        lines.append(contentsOf: announceIceLines(options))
        lines.append("t=0 0")
        // The live capture announces the server's video port, not SDP's "0 = unused".
        lines.append("m=video \(options.videoPort)")
        if options.iceCredentials != nil || options.dtlsFingerprint != nil {
            lines.append("c=IN IP4 0.0.0.0")
        }
        lines.append("i=DeviceString, DeviceName")
        lines.append("")
        return lines.joined(separator: "\r\n")
    }

    /// Ordered attribute store, so the three layers — our defaults, the seat's own offer (it knows
    /// its encoder better than we do), then the values only the client can know — overwrite each
    /// other in a defined order while still emitting in first-seen order.
    struct AnnounceAttributes {
        private var order: [String] = []
        private var values: [String: String] = [:]

        mutating func set(_ name: String, _ value: String) {
            if values.updateValue(value, forKey: name) == nil { order.append(name) }
        }

        mutating func push(_ prefix: String, indexed: Bool, _ pairs: [(String, String)]) {
            for (key, value) in pairs {
                set("x-nv-\(prefix)\(indexed ? "[0]" : "").\(key)", value)
            }
        }

        func contains(_ name: String) -> Bool { values[name] != nil }

        mutating func remove(_ name: String) {
            guard values.removeValue(forKey: name) != nil else { return }
            order.removeAll { $0 == name }
        }

        var lines: [String] { order.map { "a=\($0):\(values[$0] ?? "")" } }
    }

    static func announceAttributes(_ options: AnnounceOptions) -> AnnounceAttributes {
        var attributes = AnnounceAttributes()
        applyCapturedBaseline(&attributes, options: options)
        applyOfferedAttributes(&attributes, options: options)
        applyVideoAttributes(&attributes, options: options)
        applyBitrateAttributes(&attributes, options: options)
        applyAudioAttributes(&attributes, options: options)
        applyMicrophoneAttributes(&attributes, options: options)
        // The A/B harness: whatever the run under test asks for wins over every layer above,
        // including the client-only ones, so a knob can be measured exactly as written.
        for (name, value) in options.announceOverrides where !name.isEmpty {
            attributes.set(name, value)
        }
        return attributes
    }

    /// The mic sender SSRC is only knowable by the client — NVST has no WebRTC signaling, so the
    /// bundle's answer never reaches the seat, and this attribute is the seat's only source for
    /// the SSRC the mic RTP arrives with (the vendor stack keeps `micSsrcConfig.senderSsrc` for
    /// exactly this). Emitted only when the bundle actually carries the mic send section, keeping
    /// the announce flag / answer invariant.
    static func applyMicrophoneAttributes(_ attributes: inout AnnounceAttributes, options: AnnounceOptions) {
        guard options.carriesMicrophoneOnBundle, let ssrc = options.microphoneSenderSsrc else { return }
        attributes.set("x-nv-mic.micSsrcConfig.senderSsrc", "\(ssrc)")
    }

    /// Windows `KSAUDIO_SPEAKER_5_1` / `KSAUDIO_SPEAKER_7_1_SURROUND` masks, the speaker layout
    /// convention the seat's Windows audio stack already speaks.
    static func surroundChannelMask(channels: Int) -> Int? {
        switch channels {
        case 6: 0x3F
        case 8: 0x63F
        default: nil
        }
    }

    /// Layer three, part two: the surround request. The captured stereo baseline only carries
    /// `surround.version:2`; a 5.1 or 7.1 session adds the enable flag, channel count and speaker
    /// mask (`libBifrost2` strings `x-nv-audio.surround.enable` / `numChannels` / `channelMask`).
    /// The bundle's answer must have been built for the same count, or the seat's multi-channel
    /// Opus lands on a stereo decoder.
    static func applyAudioAttributes(_ attributes: inout AnnounceAttributes, options: AnnounceOptions) {
        guard let mask = surroundChannelMask(channels: options.audioChannelCount) else { return }
        attributes.set("x-nv-audio.surround.enable", "1")
        attributes.set("x-nv-audio.surround.numChannels", String(options.audioChannelCount))
        attributes.set("x-nv-audio.surround.channelMask", String(mask))
    }

    /// Layer one: the captured official baseline plus the client-side packet pacing it needs.
    static func applyCapturedBaseline(_ attributes: inout AnnounceAttributes, options: AnnounceOptions) {
        // Base: what the official client actually announces, captured from its own RTSPS
        // WebSocket. Hand-curating this meant guessing which of ~160 attributes mattered.
        for (name, value) in NvstCapturedAnnounce.attributes where !isClientOwned(name) {
            attributes.set(name, value)
        }

        // Nothing hand-curated on top of the capture. Overriding it was how we ended up announcing
        // `videoSplitEncodeStripsPerFrame:3` where the official client sends 64 — a different
        // slice packetisation, and a frame rate to match. The extended block stays opt-in.
        if options.announcesExtendedSettings {
            attributes.push("video", indexed: true, videoExtendedAllowlist)
            attributes.push("general", indexed: false, generalExtendedAllowlist)
        }

        // The captured baseline packs a frame's packets into five groups across 1 ms. At 60 fps
        // that is a fifth of the frame interval; at 120 it is an eighth, and the result is a dense
        // UDP burst followed by a long gap — which is what a receive buffer overflows on and what
        // our own per-wake histogram showed thousands of times. Spread a high-refresh frame across
        // less than half its 8.33 ms interval instead. This changes packet timing only: not the
        // encode rate, not the display rate. Matches OpenNOW's native streamer, which made the same
        // change for the same reason.
        if let fps = options.fps, fps >= 100 {
            attributes.set("x-nv-packetPacing.maxDelayUs", "4000")
        }
    }

    /// Layer two: the seat's own offer.
    static func applyOfferedAttributes(_ attributes: inout AnnounceAttributes, options: AnnounceOptions) {
        // The seat's offer is authoritative about the settings we already send, and about its own
        // `general.*` block. It is not an invitation to echo everything it mentions: the offer
        // enumerates every stream index the seat could ever use, and answering with thousands of
        // attributes for streams that do not exist made the seat drop DTLS immediately. So the echo
        // is an intersection — the seat's value wins for keys we would otherwise hardcode — never a
        // union. Transport choice and credentials stay ours.
        if options.echoesOfferedAttributes {
            for (name, value) in options.offeredAttributes where !isClientOwned(name) {
                guard attributes.contains(name) else { continue }
                attributes.set(name, value)
            }
        }
    }

    /// Layer three, part one: viewport, frame rate, prefilter, cursor and codec — client-only
    /// knowledge, applied last so neither our defaults nor the offer can override it.
    static func applyVideoAttributes(_ attributes: inout AnnounceAttributes, options: AnnounceOptions) {
        let (width, height) = parseResolution(options.resolution)
        // Client-only knowledge, applied last so neither our defaults nor the offer can override it.
        // The captured base always sends prefilterMode:2/prefilterModel:4 on every index; a chosen
        // mode overrides index 0 the same way viewport/bitrate do. Off (0) also zeroes the levels so
        // the seat does not keep applying the captured defaults underneath an unset sharpen/denoise.
        if let prefilterMode = options.prefilterMode {
            attributes.set("x-nv-video[0].prefilterParams.prefilterMode", String(prefilterMode))
            if prefilterMode == 0 {
                attributes.set("x-nv-video[0].prefilterParams.sharpnessLevel", "0")
                attributes.set("x-nv-video[0].prefilterParams.denoiseLevel", "0")
            } else {
                if let prefilterModel = options.prefilterModel {
                    attributes.set("x-nv-video[0].prefilterParams.prefilterModel", String(prefilterModel))
                }
                if let prefilterSharpness = options.prefilterSharpness {
                    attributes.set("x-nv-video[0].prefilterParams.sharpnessLevel", String(prefilterSharpness))
                }
                if let prefilterDenoise = options.prefilterDenoise {
                    attributes.set("x-nv-video[0].prefilterParams.denoiseLevel", String(prefilterDenoise))
                }
            }
        }
        attributes.set("x-nv-video[0].clientViewportWd", String(width))
        attributes.set("x-nv-video[0].clientViewportHt", String(height))
        // The captured baseline is a 10-bit 4:2:0 session and says so on every announce; an 8-bit
        // or 4:4:4 session has to say what it actually negotiated or the announce contradicts the
        // session request.
        if let bitDepth = options.bitDepth {
            attributes.set("x-nv-video[0].bitDepth", String(bitDepth))
        }
        if let chromaFormat = options.chromaFormat {
            attributes.set("x-nv-video[0].chromaFormat", String(chromaFormat))
        }
        // Dynamic range is independent of bit depth. The captured baseline carries
        // BT.709 CSC even when CloudMatch requested HDR; override it explicitly,
        // as the WebRTC SDP path already does. Never advertise HDR with H.264.
        if let requestedHDR = options.hdrEnabled {
            let hdr = requestedHDR && options.codec != .h264
            attributes.set("x-nv-video[0].dynamicRangeMode", hdr ? "1" : "0")
            attributes.set("x-nv-video[0].encoderCscMode", hdr ? "5" : "3")
            attributes.set("x-nv-video[0].encoderHdrCscMode", "4")
            attributes.set("x-nv-video[0].dx9EnableHdr", hdr ? "1" : "0")
            if hdr { attributes.set("x-nv-video[0].bitDepth", "10") }
            if options.codec == .h264 {
                attributes.set("x-nv-video[0].bitDepth", "8")
                attributes.set("x-nv-video[0].chromaFormat", "1")
            }
        }
        // The seat's DESCRIBE is a 720p60 BASELINE (captured live: it offers clientViewport 1280x720
        // and video[0].maxFPS:60 even for a session we provisioned at 5120x2160@120). The client's
        // announce overrides that baseline — and we were overriding only the viewport (720->5K) while
        // leaving maxFPS at the baseline 60, so the encoder ran at 5K but CAPPED FRAME RATE AT 60.
        // The earlier "official announces no maxFPS" note came from a 720p60 capture, where 60 already
        // equalled the baseline so no override was needed; a 120 session must lift it. Set maxFPS and
        // the vendor's measured high-refresh DFC/GFC floor. This exact combination sustained 120 FPS
        // in repeated hardware runs; raising the floor to the ceiling made the client contradict the
        // server's dynamic frame controller without improving the negotiated limit.
        if let fps = options.fps, fps > 0 {
            attributes.set("x-nv-video[0].maxFPS", String(fps))
            if fps > 60 {
                attributes.set("x-nv-vqos[0].dfc.minTargetFps", "100")
                attributes.set("x-nv-vqos[0].gfc.minTargetFps", "100")
                // A real plaintext capture of the official client's own ANNOUNCE (2026-08-28, SSL
                // tap) shows it overrides these too — the seat's DESCRIBE default is
                // `dfcAlgoVersion:1`, and the captured client explicitly asks for version 2. This
                // codebase only ever sent `minTargetFps`, leaving the seat on its default algorithm
                // and every other DFC tuning value the captured client overrides.
                attributes.set("x-nv-vqos[0].dfc.enable", "1")
                attributes.set("x-nv-vqos[0].dfc.adjustResAndFps", "0")
                attributes.set("x-nv-vqos[0].dfc.dfcAlgoVersion", "2")
                attributes.set("x-nv-vqos[0].dfc.decodeFpsAdjPercent", "85")
                attributes.set("x-nv-vqos[0].dfc.qpMinUpperLimit", "39")
                attributes.set("x-nv-vqos[0].dfc.qpMaxResThresholdAdj", "6")
                attributes.set("x-nv-vqos[0].dfc.targetDownCooldownMs", "250")
                // Not overridden here: the separate `resControl.dfc` namespace, whose defaults step
                // the target frame rate down in 5% increments (`receiverFpsDecreasePercent:5`,
                // `useClientFpsPerf:1`). Disabling it was tested (2026-08-28) — the attributes were
                // accepted on the wire and changed nothing — and the official client leaves it
                // alone too, so we match it and leave it alone.
            }
        }
        // Required for parity with the official NVST ANNOUNCE shape. This protocol attribute
        // does not expose Cloud G-SYNC as a supported iOS feature.
        attributes.set("x-nv-video[0].cloudGsync", "1")
        // The captured baseline's frame-pacing pair is the Adaptive announce
        // (`mode:1`/`feedbackMode:1`, `FRAME_PACING_FEEDBACK_INTERVAL`). A different VSync mode
        // has to override both, in the same client-owned layer as everything else the client
        // knows: the pacer is configured at ANNOUNCE and the seat holds it for the session, so
        // this is the only place an Off/On choice reaches the seat (see `NvstVsyncMode`).
        if let vsyncMode = options.vsyncMode {
            attributes.set("x-nv-video[0].framePacing.mode", vsyncMode.framePacingMode)
            attributes.set("x-nv-video[0].framePacing.feedbackMode", vsyncMode.framePacingFeedbackMode)
        }
        // Re-tried and removed again (2026-08-28): announcing avoidDuplicateGameFrames:0 changed
        // nothing live — static scenes still read 44-61 fps — the seat silently drops attributes
        // its DESCRIBE never offered. The static-scene dip is the seat not encoding unchanged
        // frames, and no announce knob we can reach turns that off.
        // The installed native client's cursor policy. Without these the seat accepts the cursor
        // channel but never publishes local cursor shapes, so the client cannot tell when the game
        // shows or hides its pointer and is stuck with the seat's composited one.
        attributes.set("x-nv-runtime.mouseCursorCapture", "3")
        attributes.set("x-nv-runtime.mimicRemoteCursor", "0")
        if let codec = options.codec {
            // Which bitstream to encode is still ours: the capture is an HEVC session.
            attributes.set("x-nv-vqos[0].bitStreamFormat", bitStreamFormat(codec))
        }
    }

    /// Layer three, part two: congestion control and the bitrate envelope.
    static func applyBitrateAttributes(_ attributes: inout AnnounceAttributes, options: AnnounceOptions) {
        // The seat's rate control is one-way-delay based (the vendor default). Its OWD samples ride
        // in the client's `0x0207` QoS reports (the rtpTimestamp at +36), which we now send at
        // 18 Hz — so the delay controller has its evidence and this defaults ON. The loss-based
        // fallback parks the encoder at a floor (48 packets/s vs the official client's 401), so it
        // is only reachable via OPN_NVST_OWD_CC=0.
        if options.disablesOwdCongestionControl {
            attributes.set("x-nv-bwe.useOwdCongestionControl", "0")
        }
        if let bitrateKbps = options.bitrateKbps, bitrateKbps > 0 {
            attributes.set("x-nv-video[0].initialBitrateKbps", String(bitrateKbps))
            attributes.set("x-nv-video[0].initialPeakBitrateKbps", String(bitrateKbps))
        }
        if let maximumBitrateKbps = options.maximumBitrateKbps, maximumBitrateKbps > 0 {
            attributes.set("x-nv-vqos[0].bw.maximumBitrateKbps", String(maximumBitrateKbps))
            // The ceiling alone is not enough. OpenNOW's native NVST client announces these three
            // alongside it and documents why: "bitrate remains adaptive while dynamic
            // resolution/framerate stay disabled. Omitting these fields leaves the server near its
            // low default rate even when the UI ceiling is much higher." The two IIR factors are the
            // rate controller's smoothing constants; without them its adaptation stays near the
            // floor instead of climbing to the announced maximum.
            attributes.set("x-nv-vqos[0].bw.minimumBitrateKbps", "1000")
            attributes.set("x-nv-vqos[0].drc.bitrateIirFilterFactor", "128")
            attributes.set("x-nv-vqos[0].resControl.bitrateIirFilterFactor", "128")
            // Gradual Rate Control: the mechanism that lets the seat come DOWN from the announced
            // ceiling when our receiver reports loss. Without it the encoder holds the opening bid
            // regardless of feedback, which is what a 5K120 session at a 100 Mbps ceiling looks
            // like from here — measured 1% wire loss, 265 reference recoveries and 655 bad-data
            // frames in 65 s, with the seat re-sending keyframes into the same congestion.
            // `7` enables all H.264/H.265 modes, matching OpenNOW's native Linux client (which
            // moved off `0` for exactly this reason) and the key exists verbatim in libBifrost2.
            // DRC stays off, so resolution and frame rate remain predictable — only bitrate moves.
            attributes.set("x-nv-vqos[0].grc.enable", "7")
        }
        // Corrected 2026-08-28: the claim that the vendor sends 16666/16684 here was based on an
        // earlier, incomplete capture. A full byte-exact capture of the real client's own ANNOUNCE
        // (unmasked from an SSL tap, ~8.2 KB SDP body — see docs/NVST/
        // NativeNVST120FPSInvestigation.md Phase 18/19) shows it sends ONLY `minTargetFrameTimeUs`.
        // It never sends `targetFrameTimeUs`/`maxTargetFrameTimeUs` at all. Sending a hardcoded
        // 16666/16684 (~60 fps) here silently caps the PID controller's own range to ~60-126 fps
        // regardless of what `maxFPS` requests — invisible at 120 fps, where 126 fps happens to be
        // just above it, but a live 240 fps session hit exactly this: capped at ~120 fps despite a
        // correct `maxFPS:240` announce. `minTargetFrameTimeUs` alone is real, vendor-verified, and
        // constant across every negotiated fps this investigation captured (120 and ~75).
        attributes.set("x-nv-video[0].framePacing.pid.minTargetFrameTimeUs", "7936")
    }

    /// Session key material, ICE credentials and the local address.
    ///
    /// NOTE: everything here carries session key material. `redactedForLog` must keep pace with it —
    /// the ANNOUNCE body is logged line by line, and those sinks are durable.
    static func announceSecurityLines(_ options: AnnounceOptions) -> [String] {
        var lines: [String] = []
        if let key = options.encryptionKey {
            lines.append("a=x-nv-runtime.encryptionKey:\(key.aesKeyHex.uppercased())")
            lines.append("a=x-nv-runtime.encryptionKeyId:\(key.keyID)")
        }
        if let ice = options.iceCredentials {
            if !options.officialCloudPath {
                lines.append("a=x-nv-general.iceUsernameFragment:\(ice.usernameFragment)")
                lines.append("a=x-nv-general.iceUsernamePwd:\(ice.password)")
            }
            lines.append("a=x-nv-general.iceUserNameFragmentV2:\(ice.usernameFragment)")
            lines.append("a=x-nv-general.icePasswordV2:\(ice.password)")
        }
        if let localAddress = options.localAddress {
            lines.append("a=x-nv-general.clientPorts.localAddress:\(localAddress)")
        }
        return lines
    }

    /// The `clientPorts` / bundle block.
    static func announcePortLines(_ options: AnnounceOptions) -> [String] {
        var lines: [String] = []
        // The captured official ANNOUNCE carries none of this block — no `clientPorts.*`, no
        // `clientBundlePort`, no `nativeRtcOnBundlePort`, no `rtc*OnNativeBundle`, no
        // `enableUnifiedSocket` — and its session receives audio on the bundle port regardless.
        // Ours announces `clientPorts.audio:0`, and the seat's DESCRIBE comes back with
        // `m=audio 0`, i.e. audio rejected. Suppressing the block is how that is tested.
        if options.officialCloudPath, !Self.suppressesClientPortBlock {
            for name in ["video", "audio", "mic", "control", "bundle", "session"] {
                // Audio is announced as 0 only while the bundle carries it. On its own socket the
                // seat has to be told where to send it, since there is no SETUP for audio.
                if name == "audio", let audioPort = options.clientAudioPort, !options.carriesAudioOnBundle {
                    lines.append("a=x-nv-general.clientPorts.audio:\(audioPort)")
                    continue
                }
                lines.append("a=x-nv-general.clientPorts.\(name):0")
            }
        } else if !options.officialCloudPath, let videoPort = options.clientVideoPort ?? options.clientBundlePort {
            lines.append("a=x-nv-general.clientPorts.video:\(videoPort)")
        }
        if let bundlePort = options.clientBundlePort, !Self.suppressesClientPortBlock {
            lines.append("a=x-nv-general.clientBundlePort:\(bundlePort)")
        }
        if options.officialCloudPath, !Self.suppressesClientPortBlock {
            lines.append("a=x-nv-general.nativeRtcOnBundlePort:1")
            // Video stays on the dedicated raw-SRTP Mjolnir socket; audio/mic/data ride the bundle.
            // Video always stays on the dedicated raw-SRTP Mjolnir socket.
            lines.append("a=x-nv-general.rtcVideoOnNativeBundle:0")
            lines.append("a=x-nv-general.rtcAudioOnNativeBundle:\(options.carriesAudioOnBundle ? "1" : "0")")
            lines.append("a=x-nv-general.rtcMicOnNativeBundle:\(options.carriesMicrophoneOnBundle ? "1" : "0")")
            lines.append("a=x-nv-general.rtcDataChannelOnNativeBundle:\(options.carriesDataChannelOnBundle ? "1" : "0")")
            lines.append("a=x-nv-general.enableUnifiedSocket:0")
        }
        // Only when explicitly asked for. The captured official ANNOUNCE does not carry this at
        // all: announcing `rtcpOnSctp:1` tells the seat to expect receiver reports on an SCTP
        // channel the seat never opens for us, so it gets none and its congestion control backs the
        // bitrate and frame rate down. Feedback goes on the Mjolnir socket's SRTCP instead.
        if options.rtcpOnSctp {
            lines.append("a=x-nv-general.rtcpOnSctp:1")
        }
        return lines
    }

    /// The WebRTC-shaped ICE/DTLS answer. NVST `x-nv-general.*` alone does not arm inbound UDP:
    /// the official ANNOUNCE also carries a host candidate.
    static func announceIceLines(_ options: AnnounceOptions) -> [String] {
        var lines: [String] = []
        if let fingerprint = options.dtlsFingerprint {
            if !options.officialCloudPath {
                lines.append("a=x-nv-general.dtlsFingerprint:\(fingerprint)")
            }
            lines.append("a=x-nv-general.dtlsFingerprintV2:\(fingerprint)")
        }
        // NVST `x-nv-general.*` alone does not arm inbound UDP: the official ANNOUNCE also
        // carries a WebRTC-shaped ICE/DTLS answer with a host candidate.
        if let ice = options.iceCredentials {
            lines.append("a=ice-options:trickle")
            lines.append("a=ice-ufrag:\(ice.usernameFragment)")
            lines.append("a=ice-pwd:\(ice.password)")
        }
        if let fingerprint = options.dtlsFingerprint {
            lines.append("a=fingerprint:sha-256 \(fingerprint)")
            lines.append("a=setup:actpass")
        }
        if let address = options.localAddress, let port = options.clientBundlePort, port > 0 {
            lines.append("a=candidate:1 1 udp 2122260223 \(address) \(port) typ host")
        }
        return lines
    }

    /// Attributes the client decides, so an echoed offer must not overwrite them.
    static func isClientOwned(_ name: String) -> Bool {
        let clientOwned: Set<String> = [
            "x-nv-general.nativeRtcOnBundlePort",
            "x-nv-general.rtcVideoOnNativeBundle",
            "x-nv-general.rtcAudioOnNativeBundle",
            "x-nv-general.rtcMicOnNativeBundle",
            "x-nv-mic.micSsrcConfig.senderSsrc",
            "x-nv-general.rtcDataChannelOnNativeBundle",
            "x-nv-general.enableUnifiedSocket",
            "x-nv-general.rtcpOnSctp",
            "x-nv-general.iceUsernameFragment",
            "x-nv-general.iceUsernamePwd",
            "x-nv-general.iceUserNameFragmentV2",
            "x-nv-general.icePasswordV2",
            "x-nv-general.dtlsFingerprint",
            "x-nv-general.dtlsFingerprintV2",
            "x-nv-general.clientBundlePort",
            "x-nv-runtime.videoSrtp",
            "x-nv-runtime.encryptionKey",
            "x-nv-runtime.encryptionKeyId",
        ]
        if clientOwned.contains(name) { return true }
        return name.hasPrefix("x-nv-general.clientPorts.")
    }
}
