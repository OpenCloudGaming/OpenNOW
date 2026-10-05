import Foundation

/// Drives the classic NVST handshake — OPTIONS → DESCRIBE → SETUP → ANNOUNCE → PLAY — and turns
/// its answers into the `NVSTVideoHandoff` the Bifrost-free receiver consumes.
///
/// The handoff is *not* delivered by signaling: the client derives it here, and even generates
/// the video SRTP master key itself (`runtime.encryptionKey` in ANNOUNCE). Independently
/// observed; OpenNOW's MIT `opennow-stable/.../nvstRtsp/probe.ts` documents the same flow.
public struct NvstRtspNegotiator: Sendable {
    /// Video SETUP is ready: the Mjolnir raw-SRTP receiver should start reading now, before
    /// ANNOUNCE, exactly as official Bifrost arms `MjolnirVideoReceiver` first.
    public typealias VideoReadyHandler = @Sendable (NVSTVideoHandoff) async throws -> Void
    /// ANNOUNCE accepted: bring up ICE/DTLS on the bundle before PLAY.
    public typealias AnnounceReadyHandler = @Sendable (NVSTVideoHandoff) async throws -> Void

    let reserver: any NvstBundleReserving
    let logger: (@Sendable (String) -> Void)?
    private let connectionFactory: @Sendable (NvstRtspEndpoints.Target, Duration, (@Sendable (String) -> Void)?) -> any NvstRtspControlChannel

    public init(reserver: any NvstBundleReserving,
                logger: (@Sendable (String) -> Void)? = nil,
                connectionFactory: @escaping @Sendable (NvstRtspEndpoints.Target, Duration, (@Sendable (String) -> Void)?) -> any NvstRtspControlChannel = { target, timeout, logger in
                    NvstRtspConnection(target: target, timeout: timeout, logger: logger)
                }) {
        self.reserver = reserver
        self.logger = logger
        self.connectionFactory = connectionFactory
    }

    /// Walks the advertised control endpoints until one completes the handshake. A seat can
    /// advertise an endpoint it does not serve, and the last candidate is usually the assumed
    /// `:322` default, so a single failure must not end the attempt.
    public func negotiate(_ input: NvstRtspNegotiationInput,
                          onVideoReady: VideoReadyHandler? = nil,
                          onAnnounceReady: AnnounceReadyHandler? = nil) async throws -> NvstRtspSession {
        let candidates = NvstRtspEndpoints.candidates(input.rtspsEndpoints)
        guard !candidates.isEmpty else { throw NvstRtspNegotiationError.missingEndpoint }
        var bestError: Error = NvstRtspNegotiationError.missingEndpoint
        var bestProgress = -1
        for endpoint in candidates {
            do {
                return try await negotiate(endpoint: endpoint, input: input, onVideoReady: onVideoReady, onAnnounceReady: onAnnounceReady)
            } catch {
                logger?("NVST control endpoint \(endpoint) failed: \(error.localizedDescription)")
                // Report the candidate that got furthest, not the last one tried: a media port
                // failing OPTIONS says nothing, while the control port failing SETUP is the story.
                let progress = (error as? NvstEndpointFailure)?.progress ?? Self.progress(of: error)
                if progress > bestProgress {
                    bestProgress = progress
                    bestError = (error as? NvstEndpointFailure)?.underlying ?? error
                }
            }
        }
        throw bestError
    }

    /// What DESCRIBE established about the session, before any SETUP.
    struct DescribedSession {
        var sessionIdentifier: String
        var body: String
        var videoControl: String
        var hmacSeed: String?
        var ice: NvstRtspIceCredentials?
        var srtpProfile: NVSTSrtpProfile?
        var encryptionKey: NvstRuntimeEncryptionKey
        var remoteFingerprint: String?
        var pingVersion: String?
        var disablePlay: String?
        var officialCloudPath: Bool
        /// Whether the seat offered microphone carriage on the native WebRTC bundle
        /// (`x-nv-general.rtcMicOnNativeBundle:1` in DESCRIBE). Server-driven: the official
        /// client only ever echoes this flag, and production seats offer legacy mic transport
        /// instead, so a bundle mic m-section is built only under this offer.
        var microphoneOfferedOnBundle = false
    }

    /// The accepted video SETUP, and the request forms that got it accepted.
    struct VideoSetup {
        var response: NvstRtspResponse
        var uri: String
        var transport: String
    }

    /// The negotiated media handoff plus the ICE facts ANNOUNCE and the session record still need.
    struct ResolvedHandoff {
        var handoff: NVSTVideoHandoff
        var localIce: NvstRtspIceCredentials?
        var remoteUfrag: String?
    }

    private func negotiate(endpoint: String,
                           input: NvstRtspNegotiationInput,
                           onVideoReady: VideoReadyHandler?,
                           onAnnounceReady: AnnounceReadyHandler?) async throws -> NvstRtspSession {
        guard let target = NvstRtspEndpoints.parse(endpoint: endpoint) else {
            throw NvstRtspNegotiationError.invalidEndpoint(endpoint)
        }

        var steps: [String] = []
        let connection = connectionFactory(target, input.timeout, logger)
        do {
            try await connection.connect(sessionID: input.sessionID)
            steps.append("wss-open")

            let requestURI = target.requestURI
            let common = Self.commonHeaders(target: target, sessionID: input.sessionID)
            var described = try await describeSession(connection: connection,
                                                      requestURI: requestURI,
                                                      common: common,
                                                      input: input,
                                                      steps: &steps)

            let reservation = try await reserver.reserveBundle()
            let resolved = try await setUpStage(connection: connection,
                                                requestURI: requestURI,
                                                common: common,
                                                described: &described,
                                                reservation: reservation,
                                                input: input,
                                                steps: &steps)

            if let onVideoReady {
                try await onVideoReady(resolved.handoff)
                steps.append("native-receive-armed")
            }

            try await announceStage(connection: connection,
                                    requestURI: requestURI,
                                    common: common,
                                    described: described,
                                    resolved: resolved,
                                    reservation: reservation,
                                    input: input,
                                    steps: &steps)

            if let onAnnounceReady {
                try await onAnnounceReady(resolved.handoff)
                steps.append("native-announce-armed")
            }

            await play(connection: connection,
                       requestURI: requestURI,
                       described: described,
                       common: common,
                       steps: &steps)

            // The control connection stays up for the whole session. Keep it alive with RTSP
            // OPTIONS now that the handshake is done (a WebSocket ping makes the seat close it).
            await connection.startKeepAlive(uri: requestURI, headers: common + [("Session", described.sessionIdentifier)])

            return session(endpoint: endpoint,
                           connection: connection,
                           requestURI: requestURI,
                           common: common,
                           described: described,
                           resolved: resolved,
                           steps: steps)
        } catch {
            await connection.close()
            throw NvstEndpointFailure(progress: Self.progress(of: error, reachedSteps: steps), underlying: error)
        }
    }
}

// MARK: - Handshake stages

extension NvstRtspNegotiator {
    /// Headers every request on the control connection carries.
    static func commonHeaders(target: NvstRtspEndpoints.Target, sessionID: String) -> [(String, String)] {
        var common: [(String, String)] = [("X-GS-Version", "14.2"), ("Host", "\(target.host):\(target.port)")]
        let trimmed = sessionID.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty { common.append(("x-nv-sessionid", trimmed)) }
        return common
    }

    /// The `Session` header's identifier, without its timeout parameters.
    static func sessionIdentifier(in response: NvstRtspResponse) -> String? {
        guard let value = response.header("session")?.split(separator: ";").first
            .map({ String($0).trimmingCharacters(in: .whitespaces) }), !value.isEmpty else { return nil }
        return value
    }

    /// Video SETUP, then the optional audio SETUP, then the media handoff they resolve to.
    private func setUpStage(connection: any NvstRtspControlChannel,
                            requestURI: String,
                            common: [(String, String)],
                            described: inout DescribedSession,
                            reservation: NvstBundleReservation,
                            input: NvstRtspNegotiationInput,
                            steps: inout [String]) async throws -> ResolvedHandoff {
        // The seat only returns a hex `X-Nv-Ping-Payload` (the modern NATT/ICE identity) when
        // SETUP advertises the ping version; otherwise it falls back to the literal "PING"
        // keepalive and never arms the media relay to answer STUN.
        let baseSetupHeaders = common
            + [("Session", described.sessionIdentifier), ("x-nv-ping", described.pingVersion ?? "6")]

        let videoSetup = try await setUpVideo(connection: connection,
                                              requestURI: requestURI,
                                              described: described,
                                              reservation: reservation,
                                              baseSetupHeaders: baseSetupHeaders)
        steps.append("setup-video")

        let audioPeerPort = try await setUpAudio(connection: connection,
                                                 requestURI: requestURI,
                                                 described: described,
                                                 reservation: reservation,
                                                 baseSetupHeaders: baseSetupHeaders,
                                                 steps: &steps)

        if let refreshed = Self.sessionIdentifier(in: videoSetup.response) {
            described.sessionIdentifier = refreshed
        }

        return try resolveHandoff(setup: videoSetup,
                                  described: described,
                                  reservation: reservation,
                                  input: input,
                                  audioPeerPort: audioPeerPort)
    }

    /// Brings the bundle up, builds the ANNOUNCE body and sends it.
    ///
    /// The bundle can only be brought up now: it needs SETUP's ping payload for the remote ufrag
    /// and DESCRIBE's fingerprint. Its real port and fingerprint go into ANNOUNCE.
    private func announceStage(connection: any NvstRtspControlChannel,
                               requestURI: String,
                               common: [(String, String)],
                               described: DescribedSession,
                               resolved: ResolvedHandoff,
                               reservation: NvstBundleReservation,
                               input: NvstRtspNegotiationInput,
                               steps: inout [String]) async throws {
        let bundleIdentity = await reserver.bundleIdentity(for: resolved.handoff,
                                                           microphoneOfferedOnBundle: described.microphoneOfferedOnBundle)
        let bundlePort = bundleIdentity?.bundlePort ?? reservation.bundlePort
        let fingerprint = bundleIdentity?.dtlsFingerprint ?? reservation.dtlsFingerprint
        let localAddress = bundleIdentity?.localAddress ?? reservation.localAddress
        if bundleIdentity != nil {
            logger?("NVST bundle identity ready for ANNOUNCE (port=\(bundlePort), fingerprintBytes=\(fingerprint?.count ?? 0), microphone=\(bundleIdentity?.microphoneNegotiated == true))")
        }
        // The microphone identity only exists once the bundle is up: `reserveBundle()` runs
        // before the seat's identity is known, so the mic fields arrive with the bundle identity
        // and are folded into the reservation ANNOUNCE reads.
        var reservation = reservation
        if let bundleIdentity {
            reservation.microphoneNegotiated = bundleIdentity.microphoneNegotiated
            reservation.microphoneSenderSsrc = bundleIdentity.microphoneSenderSsrc
        }

        let options = announceOptions(input: input,
                                      described: described,
                                      resolved: resolved,
                                      reservation: reservation,
                                      bundlePort: bundlePort,
                                      dtlsFingerprint: fingerprint,
                                      localAddress: localAddress)
        try await announce(connection: connection,
                           requestURI: requestURI,
                           described: described,
                           common: common,
                           options: options,
                           steps: &steps)
        // Report what ANNOUNCE actually carried, not the pre-bundle placeholders.
        logger?("NVST ANNOUNCE ok (bundlePort=\(bundlePort), localIce=\(resolved.localIce != nil), dtlsFingerprintBytes=\(fingerprint?.count ?? 0), rtcpOnSctp=\(options.rtcpOnSctp ? 1 : 0))")
    }

    /// The session handle handed back to the caller, including how to tear the control channel down.
    func session(endpoint: String,
                         connection: any NvstRtspControlChannel,
                         requestURI: String,
                         common: [(String, String)],
                         described: DescribedSession,
                         resolved: ResolvedHandoff,
                         steps: [String]) -> NvstRtspSession {
        let releaseIdentifier = described.sessionIdentifier
        let logger = logger
        return NvstRtspSession(
            endpoint: endpoint,
            sessionIdentifier: described.sessionIdentifier,
            handoff: resolved.handoff,
            remoteIceUsernameFragment: resolved.remoteUfrag,
            remoteDTLSFingerprint: described.remoteFingerprint,
            hmacSeedPresent: described.hmacSeed != nil,
            steps: steps,
            release: { reason in
                await connection.stopKeepAlive()
                _ = try? await connection.request(method: "TEARDOWN", uri: requestURI, headers: common + [("Session", releaseIdentifier)])
                await connection.close()
                logger?("NVST RTSPS control session released (\(reason))")
            },
            controlRoundTripMilliseconds: { await connection.measuredRoundTripMilliseconds() },
            controlKeepAliveSummary: { await connection.keepAliveSummary() }
        )
    }

    /// OPTIONS then DESCRIBE. The seat answers DESCRIBE with its whole resolved configuration —
    /// thousands of attributes naming every rate-control, pacing and dynamic-fps decision it made
    /// for this session. It is the only place the seat states its own settings, so it is worth
    /// keeping when a run needs explaining.
    private func describeSession(connection: any NvstRtspControlChannel,
                                 requestURI: String,
                                 common: [(String, String)],
                                 input: NvstRtspNegotiationInput,
                                 steps: inout [String]) async throws -> DescribedSession {
        let options = try await connection.request(method: "OPTIONS", uri: requestURI, headers: common)
        try expectOK("OPTIONS", options)
        steps.append("options")

        // The seat keys the modern ping/HMAC media context off `x-nv-abtesting`; omitting it
        // is treated as a legacy client.
        let describe = try await connection.request(
            method: "DESCRIBE",
            uri: requestURI,
            headers: common + [("Accept", "application/sdp"), ("x-nv-abtesting", "2")]
        )
        try expectOK("DESCRIBE", describe)
        steps.append("describe")
        guard let sessionIdentifier = Self.sessionIdentifier(in: describe) else {
            throw NvstRtspNegotiationError.missingSessionHeader
        }

        let body = describe.body
        guard let videoControl = NvstRtspSdp.mediaControl(body, mediaType: "video") else {
            throw NvstRtspNegotiationError.missingVideoControl
        }
        guard NvstRtspSdp.mediaControl(body, mediaType: "audio") != nil else {
            throw NvstRtspNegotiationError.missingAudioControl
        }
        guard NvstRtspSdp.primaryControlStream(NvstRtspSdp.allMediaControls(body)) != nil else {
            throw NvstRtspNegotiationError.missingControlStream
        }

        let describedKey = NvstRtspSdp.runtimeEncryptionKey(body)
        let describedPingVersion = NvstRtspSdp.attribute(body, "general.pingVersion")
        let advertisesCloudPath = NvstRtspSdp.attribute(body, "general.nativeRtcOnBundlePort") == "1"
        let officialCloudPath = advertisesCloudPath && !input.forcesLegacyPath
        let microphoneOfferedOnBundle = NvstRtspSdp.attribute(body, "general.rtcMicOnNativeBundle") == "1"
        // Official Bifrost ALWAYS client-generates `runtime.encryptionKey` and sends it in
        // ANNOUNCE, even when a DTLS fingerprint is present: the video SRTP socket is keyed
        // by this runtime key, not by DTLS-SRTP. Skipping it means the seat never keys video.
        let described = DescribedSession(
            sessionIdentifier: sessionIdentifier,
            body: body,
            videoControl: videoControl,
            hmacSeed: NvstRtspSdp.hmacSeed(body),
            ice: NvstRtspSdp.iceCredentials(body),
            srtpProfile: NvstRtspSdp.advertisedSrtpProfile(sdp: body),
            encryptionKey: describedKey ?? NvstRtspSdp.generateClientEncryptionKey(),
            remoteFingerprint: NvstRtspSdp.attribute(body, "general.dtlsFingerprintV2")
                ?? NvstRtspSdp.attribute(body, "general.dtlsFingerprint"),
            pingVersion: describedPingVersion,
            disablePlay: NvstRtspSdp.attribute(body, "general.disablePlay"),
            officialCloudPath: officialCloudPath,
            microphoneOfferedOnBundle: microphoneOfferedOnBundle
        )
        logger?("NVST DESCRIBE ok (session=\(sessionIdentifier), videoControl=\(videoControl), hmac=\(described.hmacSeed != nil), describedKey=\(describedKey != nil), official=\(officialCloudPath)\(advertisesCloudPath && input.forcesLegacyPath ? " (cloud path advertised, legacy forced)" : ""), pingVersion=\(describedPingVersion ?? "absent"), micOnBundleOffered=\(microphoneOfferedOnBundle), remoteFingerprintBytes=\(described.remoteFingerprint?.count ?? 0))")
        // The seat's audio/mic configuration is otherwise invisible after the fact: the bundle mic
        // work needs to know what the seat says about redundancy, payload types and mic framing,
        // so the audio-relevant DESCRIBE attributes go to the diagnostic log verbatim.
        for line in Self.audioRelevantDescribeLines(body) {
            logger?("NVST DESCRIBE sdp \(line)")
        }
        return described
    }

    /// Walks the request-URI forms (then the Transport forms) until one is accepted: a live seat
    /// answered 400 to a resolved absolute URI while accepting OPTIONS and DESCRIBE on the same
    /// connection, and the accepted form is advertised nowhere.
    private func setUpVideo(connection: any NvstRtspControlChannel,
                            requestURI: String,
                            described: DescribedSession,
                            reservation: NvstBundleReservation,
                            baseSetupHeaders: [(String, String)]) async throws -> VideoSetup {
        let videoSetupCandidates = NvstRtspEndpoints.videoSetupURICandidates(
            control: described.videoControl,
            base: requestURI,
            officialCloudPath: described.officialCloudPath
        )
        let legacyTransport = "unicast;X-GS-ClientPort=\(reservation.mjolnirPort)-\(Int(reservation.mjolnirPort) + 1)"
        // Transport forms to try, in order. The official cloud path sends a literally empty
        // Transport; the legacy form is the fallback so one run separates "wrong request-URI"
        // from "wrong Transport".
        let transportForms: [String] = described.officialCloudPath ? ["", legacyTransport] : [legacyTransport, ""]
        var lastRejection = "none"
        for transport in transportForms {
            for candidate in videoSetupCandidates {
                let response = try await connection.request(
                    method: "SETUP",
                    uri: candidate,
                    headers: baseSetupHeaders + [("Transport", transport)]
                )
                if response.statusCode == 200 {
                    return VideoSetup(response: response, uri: candidate, transport: transport)
                }
                let detail = response.body.trimmingCharacters(in: .whitespacesAndNewlines).prefix(200)
                lastRejection = "\(response.statusCode) \(response.statusText)"
                logger?("NVST SETUP rejected uri=\(candidate) transport=\(transport.isEmpty ? "<empty>" : transport) status=\(lastRejection)\(detail.isEmpty ? "" : " body=\(detail)")")
            }
        }
        throw NvstRtspNegotiationError.requestFailed(
            "SETUP",
            400,
            "last=\(lastRejection); tried \(videoSetupCandidates.count) request-URI forms x \(transportForms.count) Transport forms"
        )
    }

    /// Audio has no SETUP in any capture of the official client — it takes audio on the bundle
    /// instead — and the DESCRIBE offers `m=audio 0`, a port of zero, which means the stream is
    /// inactive. Asking for it explicitly is the only route to receiving audio on a socket we own:
    /// announcing `clientPorts.audio` alone made the seat stop sending audio on the bundle without
    /// sending any to us. Strictly opt-in, and a refusal is logged rather than fatal, because the
    /// working configuration does not need it.
    ///
    /// Returns the port the seat will send audio from, when it accepted.
    private func setUpAudio(connection: any NvstRtspControlChannel,
                            requestURI: String,
                            described: DescribedSession,
                            reservation: NvstBundleReservation,
                            baseSetupHeaders: [(String, String)],
                            steps: inout [String]) async throws -> UInt16? {
        guard let audioPort = reservation.audioPort else { return nil }
        for candidate in NvstRtspEndpoints.videoSetupURICandidates(
            control: "streamid=audio/0", base: requestURI, officialCloudPath: described.officialCloudPath
        ) {
            let response = try await connection.request(
                method: "SETUP",
                uri: candidate,
                headers: baseSetupHeaders
                    + [("Transport", "unicast;X-GS-ClientPort=\(audioPort)-\(audioPort + 1)")]
            )
            let transport = response.header("transport") ?? ""
            logger?("NVST audio SETUP uri=\(candidate) status=\(response.statusCode) transport=\(transport.isEmpty ? "<empty>" : transport)")
            if response.statusCode == 200 {
                steps.append("setup-audio")
                // The seat names the port it will send audio from, and the NAT mapping has to be
                // opened towards that port rather than the video one.
                return NvstRtspMessage.extractVideoPeer(transport)?.port
            }
        }
        logger?("NVST audio SETUP was refused; audio has no socket of its own")
        return nil
    }

    /// Turns the accepted SETUP into the media handoff the native receiver runs on.
    private func resolveHandoff(setup: VideoSetup,
                                described: DescribedSession,
                                reservation: NvstBundleReservation,
                                input: NvstRtspNegotiationInput,
                                audioPeerPort: UInt16?) throws -> ResolvedHandoff {
        let setupProfile = NvstRtspSdp.advertisedSrtpProfile(headers: setup.response.headers)
        if let describedProfile = described.srtpProfile, let setupProfile, describedProfile != setupProfile {
            throw NvstRtspNegotiationError.conflictingSrtpProfile(describedProfile.rawValue, setupProfile.rawValue)
        }
        let srtpProfile = setupProfile ?? described.srtpProfile ?? .aeadAes256Gcm8
        guard let videoPeer = NvstRtspMessage.extractVideoPeer(setup.response.header("transport")) else {
            throw NvstRtspNegotiationError.missingVideoPeer
        }
        let pingPayload = setup.response.header("x-nv-ping-payload")
        let pingVersion = setup.response.header("x-nv-ping").flatMap(Int.init)
        if pingVersion == 6, described.ice == nil || pingPayload == nil {
            throw NvstRtspNegotiationError.missingIceCredentials
        }

        let remoteUfrag = NvstRtspEndpoints.resolveIceRemoteUfrag(
            pingPayload: pingPayload,
            describeUfrag: described.ice?.usernameFragment,
            pingVersion: pingVersion
        )
        let localIce = reservation.iceCredentials ?? (described.ice != nil ? NvstRtspSdp.generateIceCredentials() : nil)
        let iceCredentials: NVSTHandoffIceCredentials? = {
            guard let localIce, let remoteUfrag, let remotePassword = described.ice?.password else { return nil }
            return NVSTHandoffIceCredentials(
                localUsernameFragment: localIce.usernameFragment,
                localPassword: localIce.password,
                remoteUsernameFragment: remoteUfrag,
                remotePassword: remotePassword,
                remoteDTLSFingerprint: described.remoteFingerprint
            )
        }()

        var handoff = NVSTVideoHandoff(
            clientUDPPort: reservation.mjolnirPort,
            videoPeerIP: videoPeer.ip,
            videoPeerPort: videoPeer.port,
            srtpProfile: srtpProfile,
            srtpAESKey: try NVSTVideoHandoffParser.decodeFixedHex(described.encryptionKey.aesKeyHex, field: "runtime.encryptionKey", expectedLength: srtpProfile.masterKeyLength),
            srtpSalt: try NVSTVideoHandoffParser.decodeFixedHex(NvstRtspSdp.srtpSaltHex(keyID: described.encryptionKey.keyID), field: "srtpSaltHex", expectedLength: srtpProfile.masterSaltLength),
            codec: NVSTVideoCodec.parse(input.codec ?? "H264") ?? .h264,
            rtpPayloadType: 96,
            rtpSSRC: 0,
            reorderWindowPackets: 32,
            maxAccessUnitBytes: 2 * 1024 * 1024,
            // The handshake needs far longer than the 5 s media idle default.
            timeoutMilliseconds: 60_000,
            pingVersion: pingVersion.map { UInt8(clamping: $0) },
            pingPayload: pingPayload ?? "PING",
            mjolnirUDPPort: reservation.mjolnirPort,
            iceCredentials: iceCredentials
        )
        handoff.audioPeerPort = audioPeerPort
        handoff.videoPeerPunchPorts = NvstRtspMessage.extractVideoPeerPorts(setup.response.header("transport"))
        logger?("NVST SETUP ok uri=\(setup.uri) (official=\(described.officialCloudPath), bundlePort=\(reservation.bundlePort), mjolnirPort=\(reservation.mjolnirPort), peer=\(videoPeer.ip):\(videoPeer.port), punchPorts=\(handoff.effectiveVideoPeerPunchPorts.map(String.init).joined(separator: ",")), srtpProfile=\(srtpProfile.rawValue), pingVersion=\(pingVersion.map(String.init) ?? "legacy"), pingPayloadBytes=\(pingPayload?.utf8.count ?? 0), iceRemote=\(remoteUfrag ?? "absent"), transport=\(setup.transport.isEmpty ? "<empty>" : setup.transport), serverTransport=\(setup.response.header("transport") ?? "<none>"), gsVersion=\(setup.response.header("x-gs-version") ?? "<none>"), responseHeaders=\(setup.response.headers.keys.sorted().joined(separator: ",")))")
        return ResolvedHandoff(handoff: handoff, localIce: localIce, remoteUfrag: remoteUfrag)
    }

    private func announceOptions(input: NvstRtspNegotiationInput,
                                 described: DescribedSession,
                                 resolved: ResolvedHandoff,
                                 reservation: NvstBundleReservation,
                                 bundlePort: UInt16,
                                 dtlsFingerprint: String?,
                                 localAddress: String?) -> NvstRtspSdp.AnnounceOptions {
        NvstRtspSdp.AnnounceOptions(
            resolution: input.resolution,
            fps: input.fps,
            prefilterMode: input.prefilterMode,
            prefilterSharpness: input.prefilterSharpness,
            prefilterDenoise: input.prefilterDenoise,
            prefilterModel: input.prefilterModel,
            bitDepth: input.colorQuality.map { NvstRtspSdp.colorFormat(forColorQuality: $0).bitDepth },
            chromaFormat: input.colorQuality.map { NvstRtspSdp.colorFormat(forColorQuality: $0).chromaFormat },
            hdrEnabled: input.hdrEnabled,
            audioChannelCount: input.audioChannelCount,
            encryptionKey: described.encryptionKey,
            iceCredentials: resolved.localIce,
            videoPort: resolved.handoff.videoPeerPort,
            clientBundlePort: described.officialCloudPath ? bundlePort : nil,
            // Legacy path: video arrives on the socket SETUP advertised, not the bundle.
            clientVideoPort: described.officialCloudPath ? nil : reservation.mjolnirPort,
            clientAudioPort: reservation.audioPort,
            localAddress: localAddress ?? NvstRoutedIPv4.discover(),
            dtlsFingerprint: dtlsFingerprint,
            officialCloudPath: described.officialCloudPath,
            // The bundle opens `rtcp_on_sctp_private` and carries RR/PLI once it is up; with no
            // bundle the feedback plane stays on the Mjolnir socket's SRTCP instead.
            // The official client announces nothing here, and claiming RTCP-over-SCTP without
            // a feedback channel starves the seat of receiver reports.
            rtcpOnSctp: input.rtcpOnSctp,
            // Audio leaves the bundle exactly when a socket has been reserved for it.
            carriesAudioOnBundle: reservation.audioPort == nil,
            // The mic rides the bundle exactly when the bundle's answer really carries the send
            // section — the same invariant the audio flag has, enforced the same way: the flag
            // mirrors what the bundle reported rather than what the preferences asked for.
            carriesMicrophoneOnBundle: reservation.microphoneNegotiated,
            microphoneSenderSsrc: reservation.microphoneNegotiated ? reservation.microphoneSenderSsrc : nil,
            // The seat states its own encoder preferences in DESCRIBE; the answer echoes them
            // rather than contradicting them with a hardcoded default.
            offeredAttributes: NvstRtspSdp.offeredAttributes(described.body),
            codec: NVSTVideoCodec.parse(input.codec ?? "H264"),
            bitrateKbps: input.bitrateKbps,
            maximumBitrateKbps: input.maximumBitrateKbps,
            disablesOwdCongestionControl: input.disablesOwdCongestionControl,
            announcesExtendedSettings: input.announcesExtendedSettings,
            echoesOfferedAttributes: input.echoesOfferedAttributes,
            vsyncMode: input.vsyncMode,
            announceOverrides: input.announceOverrides
        )
    }

    /// Sends the ANNOUNCE. It is the one message whose content we cannot check from the seat's
    /// reply: it answers 200 to bodies it then cannot act on. Logging it lets the whole attribute
    /// set be diffed against the official client's capture without spending a live run.
    private func announce(connection: any NvstRtspControlChannel,
                          requestURI: String,
                          described: DescribedSession,
                          common: [(String, String)],
                          options: NvstRtspSdp.AnnounceOptions,
                          steps: inout [String]) async throws {
        let announceBody = NvstRtspSdp.buildAnnounceSdp(options)
        let announceLines = announceBody.components(separatedBy: "\r\n").filter { !$0.isEmpty }
        logger?("NVST ANNOUNCE sdp lines=\(announceLines.count)")
        for line in announceLines { logger?("NVST ANNOUNCE sdp \(NvstRtspSdp.redactedForLog(line))") }
        let response = try await connection.request(
            method: "ANNOUNCE",
            uri: described.officialCloudPath ? requestURI : "/",
            headers: common + [("Session", described.sessionIdentifier), ("Content-Type", "application/sdp")],
            body: announceBody
        )
        try expectOK("ANNOUNCE", response)
        steps.append("announce")
    }

    private func play(connection: any NvstRtspControlChannel,
                      requestURI: String,
                      described: DescribedSession,
                      common: [(String, String)],
                      steps: inout [String]) async {
        // The official client sends PLAY unless the seat explicitly disables it, so an absent
        // attribute means "not disabled" rather than "disabled". Only a literal 1 suppresses it;
        // treating absence as a skip would leave a seat that omits the attribute with no PLAY.
        guard described.disablePlay != "1" else {
            steps.append("play-skipped")
            return
        }
        let play = try? await connection.request(method: "PLAY",
                                                 uri: described.officialCloudPath ? requestURI : "/",
                                                 headers: common + [("Session", described.sessionIdentifier)])
        switch play?.statusCode {
        case 200:
            steps.append("play")
        case 455:
            // Bifrost ANNOUNCE-only seats answer 455; media already flows.
            steps.append("play-455")
        case .some(let code):
            steps.append("play-failed-\(code)")
        case nil:
            steps.append("play-timeout")
        }
    }

    /// A stage failure plus how far the handshake got, so a deep client-side failure outranks
    /// another endpoint's early handshake refusal. Without it an AV1 decoder-arming failure after
    /// a fully successful SETUP lost to a fallback port's `OPTIONS 400`, and the diagnostic named
    /// the wrong endpoint's refusal (measured live 2026-09-07).
    private struct NvstEndpointFailure: Error, LocalizedError {
        let progress: Int
        let underlying: Error
        var errorDescription: String? { underlying.localizedDescription }
    }

    /// How far through the handshake an error got, so the most informative failure survives.
    /// Non-negotiation errors are ranked by the steps the attempt completed: a bundle reservation
    /// or decoder failure is not a handshake refusal and must not read as one.
    private static func progress(of error: Error, reachedSteps steps: [String] = []) -> Int {
        guard let negotiation = error as? NvstRtspNegotiationError else {
            return nonNegotiationProgress(reachedSteps: steps)
        }
        switch negotiation {
        case .missingEndpoint, .invalidEndpoint: return 0
        case .requestFailed(let method, _, _):
            switch method.uppercased() {
            case "OPTIONS": return 1
            case "DESCRIBE": return 2
            case "SETUP": return 5
            case "ANNOUNCE": return 7
            default: return 8
            }
        case .missingSessionHeader: return 3
        case .missingVideoControl, .missingAudioControl, .missingControlStream: return 4
        case .missingVideoPeer, .missingIceCredentials, .conflictingSrtpProfile: return 6
        }
    }

    /// A failure that is not an RTSP refusal — bundle reservation, decoder arming — is ranked
    /// where the attempt's completed steps say it happened, so it outranks another endpoint's
    /// earlier handshake refusal.
    private static func nonNegotiationProgress(reachedSteps steps: [String]) -> Int {
        if steps.contains("setup-video") { return 6 }
        if steps.contains("describe") { return 4 }
        if steps.contains("options") { return 2 }
        return steps.contains("wss-open") ? 1 : 0
    }

    private func expectOK(_ method: String, _ response: NvstRtspResponse) throws {
        guard response.statusCode == 200 else {
            throw NvstRtspNegotiationError.requestFailed(method, response.statusCode, response.statusText)
        }
    }
}

extension NvstRtspNegotiator {
    /// The DESCRIBE lines that describe the seat's audio and microphone planes: the `x-nv-mic`,
    /// `x-nv-aqos` and `x-nv-audio` namespaces, the bundle/RTC flags, the client-port table and
    /// every codec line (`m=audio`, `rtpmap`, `fmtp`). Capped so a hostile body cannot flood the log.
    static func audioRelevantDescribeLines(_ body: String) -> [String] {
        let markers = ["x-nv-mic.", "x-nv-aqos.", "x-nv-audio.", "x-nv-general.rtc", "x-nv-general.clientPorts",
                       "x-nv-runtime.mic", "x-nv-general.nativeRtcOnBundlePort", "m=audio", "a=rtpmap", "a=fmtp",
                       "x-nv-general.separateMicStream", "nv-audio-surround"]
        let lines: [String] = body.components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { line in !line.isEmpty && markers.contains(where: line.contains) }
        return Array(lines.prefix(160))
    }
}
