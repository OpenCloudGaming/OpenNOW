import AudioToolbox
import Foundation

/// The bundle the native transport runs on: DTLS, SCTP and audio, with no peer-library types in it.
///
/// It replaces `NvstWebRtcBundle`, and everything protocol-shaped lives in the components it
/// composes — the handshake, the association and the two audio pipelines — so this type is their
/// wiring: which datagram goes to which layer, and which recovered parser sees which command.
///
/// Three members of the bundle it replaces are deliberately **not** carried over, because they
/// existed only to expose libwebrtc's own statistics and would be no-ops here:
///
/// - `roundTripMilliseconds` / `refreshTransportStatistics`: the native bundle has no ICE candidate
///   pair to time. `NvstBifrostFreeInput` already falls back to the Mjolnir socket's STUN round trip
///   and then the control connection's ping/pong, so latency keeps a real source once the call site
///   stops asking the bundle.
/// `controlChannelStats` is not one of them: the per-command `0x313` records are kept here, because
/// only the caller that sends a command knows its code.
public final class NvstNativeBundle: @unchecked Sendable {
    public struct MicrophoneSetup: Equatable, Sendable {
        public let volume: Double
        public let initiallyEnabled: Bool
        /// The UID of the microphone to capture from. Nil or empty is "Default Device", which
        /// follows the system default input for the whole session.
        public let deviceUniqueID: String?

        public init(volume: Double, initiallyEnabled: Bool, deviceUniqueID: String? = nil) {
            self.volume = min(max(volume.isFinite ? volume : 1, 0), 1)
            self.initiallyEnabled = initiallyEnabled
            self.deviceUniqueID = deviceUniqueID.flatMap { $0.isEmpty ? nil : $0 }
        }
    }

    public struct Identity: Equatable, Sendable {
        public let bundlePort: UInt16
        public let localAddress: String?
        public let dtlsFingerprint: String
    }

    /// What the transport needs for the HUD's A/V reading. Its fields are named as libwebrtc's were,
    /// so the arithmetic that consumes them is unchanged even though a native jitter buffer now
    /// produces them.
    public struct AudioReception: Equatable, Sendable {
        public var packets: UInt64 = 0
        public var ssrc: UInt32?
        public var bytes: UInt64 = 0
        public var samples: UInt64 = 0
        public var jitterBufferDelaySeconds: Double = 0
        public var jitterBufferEmitted: UInt64 = 0
        /// Packets that never arrived and were concealed.
        public var concealed: UInt64 = 0
        /// Packets discarded rather than concealed: failed authentication, or RED that did not add up.
        public var discarded: UInt64 = 0
    }

    public enum BundleError: LocalizedError, Equatable {
        case socketUnavailable(String)
        case handshakeFailed(String)
        case associationUnavailable
        case audioUnavailable(String)

        public var errorDescription: String? {
            switch self {
            case .socketUnavailable(let reason): "The NVST bundle could not bind its socket: \(reason)"
            case .handshakeFailed(let reason): "The NVST bundle's DTLS handshake failed: \(reason)"
            case .associationUnavailable: "The NVST bundle could not start its SCTP association."
            case .audioUnavailable(let reason): "The NVST bundle could not start audio: \(reason)"
            }
        }
    }

    // MARK: - Inbound hooks

    public var onFailure: (@Sendable (String) -> Void)?
    public var onControlChannelOpen: (@Sendable () -> Void)?
    public var onFeedbackChannelOpen: (@Sendable () -> Void)?
    public var onPartiallyReliableControlOpen: (@Sendable () -> Void)?
    public var onInputProtocolNegotiated: (@Sendable (UInt16) -> Void)?
    public var onRemoteAudio: (@Sendable (Int) -> Void)?
    public var onRemoteCursor: (@Sendable (NvstRemoteCursor) -> Void)?
    public var onSeatStats: (@Sendable (NvstSeatStats) -> Void)?
    public var onSeatTermination: (@Sendable (NvstSeatTermination) -> Void)?
    public var onHapticEvents: (@Sendable ([NvstHapticEvent]) -> Void)?
    public var onHdrMode: (@Sendable (NvstHdrModeNotification) -> Void)?
    /// The seat's live session-limit timer (`0x0103`), parsed from the same JSON the vendor client reads.
    public var onSessionLimitUpdate: (@Sendable (StreamSessionLimitUpdate) -> Void)?
    /// Decoded game audio as it reaches the speaker, before any local mute, so a recording and a
    /// Co-Op guest keep hearing the game while these speakers are silenced.
    public var onGameAudioFrame: (@Sendable (UnsafeRawPointer?, UInt32, Double, UInt32) -> Void)?
    public var onMicrophoneAudioFrame: (@Sendable (UnsafeRawPointer?, UInt32, Double, UInt32) -> Void)?
    /// The live capture meter, 0...1, at 20 Hz, read off the PCM the device actually captured.
    public var onMicrophoneLevel: (@Sendable (Double) -> Void)?
    /// The chosen microphone went away and capture fell back to the system default. The saved UID
    /// is not rewritten — the device is expected back.
    public var onMicrophoneDeviceFallback: (@Sendable (NvstCaptureDeviceChange) -> Void)?
    /// A microphone was plugged in or taken away, so the picker's rows are stale.
    public var onMicrophoneDeviceListChange: (@Sendable () -> Void)?

    // MARK: - State

    private static let controlStream: UInt16 = 0
    private static let partiallyReliableControlStream: UInt16 = 6
    private static let inputStream: UInt16 = 10
    private static let feedbackStream: UInt16 = 14
    /// The seat's game audio arrives 5 ms per packet (`x-nv-aqos.packetDuration:5`), which is the
    /// grid the Opus decoder and jitter buffer pull on.
    private static let decodedAudioFramesPerPacket = 240
    /// The seat's microphone contract asks for 10 ms frames (`x-nv-mic.frameSize:10`), the same
    /// `a=ptime:10` the verified working mic section carried. Sending 5 ms Opus here leaves the
    /// seat's virtual mic meter dead even though packets leave the host.
    private static let microphoneFramesPerPacket = 480
    private static let microphoneChannels = 2
    /// The native audio decode is stereo regardless of the surround preference; see `startAudioDevice`.
    private static let decodedAudioChannels = 2

    public private(set) var audioChannelCount: Int
    public private(set) var inputProtocolVersion: UInt16?
    public private(set) var microphoneSenderSsrc: UInt32?

    public var audioOutputLatencySeconds: Double? { audioDevice?.outputPathLatencySeconds }
    /// The seat's audio is a single stereo-or-surround stream, so the count is what the HUD reports
    /// as the track count on this transport.
    public var remoteAudioTrackCount: Int { audioChannelCount }

    public var isControlChannelOpen: Bool { association?.isChannelOpen(Self.controlStream) ?? false }
    public var isFeedbackChannelOpen: Bool { association?.isChannelOpen(Self.feedbackStream) ?? false }
    public var isInputChannelOpen: Bool { association?.isChannelOpen(Self.inputStream) ?? false }
    /// Input is accepted once the control channel — where the seat expects remote input — is open and
    /// the seat has announced its protocol version.
    public var isInputReady: Bool { isControlChannelOpen && inputProtocolVersion != nil }

    private let handoff: NVSTVideoHandoff
    private let identity: NvstDtlsIdentity
    private let logger: (@Sendable (String) -> Void)?
    private var microphoneSetup: MicrophoneSetup?
    private var reservation: NvstUdpPortReservation?
    private var transport: NvstDtlsTransport?
    private var association: NvstSctpAssociation?
    private var receivePipeline: NvstAudioReceivePipeline?
    private var sendPipeline: NvstAudioSendPipeline?
    private var audioDevice: NvstCoreAudioDevice?
    private var isClosed = false
    private var lastChannelOpenError: String?
    private var hasLoggedChannelOpens = false
    private struct ControlStat { var sent: UInt32 = 0; var failed: UInt32 = 0; var bytes: UInt64 = 0 }
    private let controlStatsLock = NSLock()
    private var controlStats: [UInt16: ControlStat] = [:]
    /// Microphone capture evidence, written from the CoreAudio render thread and read by the
    /// diagnostics line. `tx` alone cannot tell a silent capture from a seat that never decodes it.
    private let microphoneStatsLock = NSLock()
    private var microphoneCapturedFrames: UInt64 = 0
    private var microphoneCaptureLevel: Double = 0

    private let startsAudioDevice: Bool

    public init(handoff: NVSTVideoHandoff,
                identity: NvstDtlsIdentity,
                audioChannelCount: Int = 2,
                startsAudioDevice: Bool = true,
                logger: (@Sendable (String) -> Void)? = nil) {
        self.handoff = handoff
        self.startsAudioDevice = startsAudioDevice
        self.identity = identity
        self.audioChannelCount = NvstCoreAudioFormat.supportedPlayoutChannelCount(audioChannelCount)
        self.logger = logger
    }

    deinit { close() }

    // MARK: - Bring-up

    /// Binds the socket and arms the transport. The returned port and fingerprint are what ANNOUNCE
    /// carries; DTLS cannot complete before the seat has received them, so it is driven in the
    /// background and `onHandshakeComplete` fires when the seat starts answering. Nothing here talks
    /// to the RTSP negotiator, which keeps using `NvstBundleReserving`.
    public func prepare(microphone: MicrophoneSetup?, audioChannelCount: Int) async throws -> Identity {
        self.audioChannelCount = NvstCoreAudioFormat.supportedPlayoutChannelCount(audioChannelCount)
        self.microphoneSetup = microphone
        // ANNOUNCE goes out before the DTLS handshake can key a send pipeline, so whether the mic is
        // carried and the SSRC its RTP arrives with must be known now. The SSRC is deterministic, so
        // a requested microphone is advertised here rather than when the pipeline is finally built.
        microphoneSenderSsrc = microphone == nil ? nil : NvstAudioSendPipeline.microphoneSSRC
        let reservation = try NvstUdpPortReservation.bindEphemeral()
        self.reservation = reservation
        let descriptor = reservation.takeDescriptor()
        guard descriptor >= 0 else { throw BundleError.socketUnavailable("the socket was already taken") }

        let handshake = try NvstDtlsHandshake(role: .client,
                                              identity: identity,
                                              expectedPeerFingerprint: handoff.iceCredentials?.remoteDTLSFingerprint)
        let localAddress = NvstRoutedIPv4.discover()
        let transport = try NvstDtlsTransport(handshake: handshake,
                                              descriptor: descriptor,
                                              localAddress: localAddress,
                                              localPort: reservation.port,
                                              peerAddress: handoff.videoPeerIP,
                                              peerPort: handoff.videoPeerPort)
        self.transport = transport
        // The seat's front end routes this socket by the STUN username, so the punch must carry the
        // DESCRIBE ufrag — which already ends in the seat's internal bundle port — or the ClientHello
        // is never forwarded to the bundle service and the handshake dies on its deadline.
        if let credentials = handoff.iceCredentials {
            transport.natt = NvstDtlsTransport.NattIdentity(
                remoteUfrag: credentials.remoteUsernameFragment,
                localUfrag: credentials.localUsernameFragment,
                integrityKey: Data(credentials.remotePassword.utf8)
            )
        }
        // The transport owns the socket, so it is the layer that classifies what arrives: audio
        // beside DTLS, SCTP inside it. These three hooks are the whole of that routing.
        transport.onSecureAudio = { [weak self] datagram in
            self?.receivePipeline?.ingest(datagram)
        }
        transport.onApplicationData = { [weak self] payload in
            guard let self, let association = self.association else { return }
            association.feedInbound(payload)
            association.drainInbound()
            openChannelsWhenEstablished()
        }
        transport.onHandshakeComplete = { [weak self] in self?.beginPostHandshake() }
        transport.onHandshakeFailure = { [weak self] error in
            self?.logger?("NVST native bundle DTLS failed: \(error.localizedDescription)")
            self?.onFailure?("Native NVST DTLS failed: \(error.localizedDescription)")
        }
        transport.start()
        return Identity(bundlePort: reservation.port, localAddress: localAddress, dtlsFingerprint: identity.fingerprint)
    }

    /// The seat answers DTLS only once ANNOUNCE has reached it, so the audio keys, the device and the
    /// association all start from the handshake completing — on the transport's queue, never inside
    /// `prepare`.
    private func beginPostHandshake() {
        guard !isClosed else { return }
        do {
            try startAssociation()
            try startAudio(microphone: microphoneSetup)
            logger?("NVST native bundle DTLS established on port \(reservation?.port ?? 0)")
        } catch {
            logger?("NVST native bundle bring-up failed after DTLS: \(error.localizedDescription)")
            onFailure?("Native NVST audio/control setup failed: \(error.localizedDescription)")
        }
    }

    /// The seat's audio, decrypted and ordered, ready for the device to pull. Called from the render
    /// clock, so it does no work beyond draining what has already arrived.
    public func pullAudio() -> [Float] {
        receivePipeline?.pull() ?? []
    }

    // MARK: - Channels

    public func sendControl(_ command: NvstControlCommand) -> Bool {
        send(command, on: Self.controlStream)
    }

    public func sendPartiallyReliableControl(_ command: NvstControlCommand) -> Bool {
        send(command, on: Self.partiallyReliableControlStream)
    }

    public func sendInput(_ payload: Data) -> Bool {
        guard let association else { return false }
        return (try? association.send(payload, streamID: Self.inputStream, ppid: NvstSctpAssociation.PPID.binary)) != nil
    }

    public func sendReliableInput(_ payload: Data) -> Bool {
        sendInput(payload)
    }

    public func sendFeedback(_ payload: Data) -> Bool {
        guard let association else { return false }
        return (try? association.send(payload, streamID: Self.feedbackStream, ppid: NvstSctpAssociation.PPID.binary)) != nil
    }

    private func send(_ command: NvstControlCommand, on stream: UInt16) -> Bool {
        guard let association, let encoded = try? command.encoded else {
            recordControl(command.code.rawValue, bytes: 0, sent: false)
            return false
        }
        let accepted = (try? association.send(encoded, streamID: stream, ppid: NvstSctpAssociation.PPID.binary)) != nil
        recordControl(command.code.rawValue, bytes: UInt64(encoded.count), sent: accepted)
        return accepted
    }

    /// The `0x313` report's totals, kept here rather than on the association because the command
    /// code is only known to the caller that sends it.
    public var controlChannelStats: (totalSent: UInt32, totalFailed: UInt32, totalBytes: UInt64, commands: [NvstControlChannelCommandStats]) {
        controlStatsLock.lock()
        defer { controlStatsLock.unlock() }
        let commands = controlStats.sorted { $0.key < $1.key }.map {
            NvstControlChannelCommandStats(commandCode: $0.key,
                                           messagesSent: $0.value.sent,
                                           messagesFailed: $0.value.failed,
                                           aggregatedBytes: $0.value.bytes)
        }
        return (commands.reduce(0) { $0 &+ $1.messagesSent },
                commands.reduce(0) { $0 &+ $1.messagesFailed },
                commands.reduce(0) { $0 &+ $1.aggregatedBytes },
                commands)
    }

    private func recordControl(_ code: UInt16, bytes: UInt64, sent: Bool) {
        controlStatsLock.lock()
        defer { controlStatsLock.unlock() }
        var stat = controlStats[code] ?? ControlStat()
        stat.bytes &+= bytes
        if sent { stat.sent &+= 1 }
        if !sent { stat.failed &+= 1 }
        controlStats[code] = stat
    }

    // MARK: - Audio control



    /// Swaps the microphone the session captures from, mid-stream. Only the capture AudioUnit is
    /// replaced: the send pipeline, its SSRC and its RTP sequence were fixed at ANNOUNCE.

    /// The UID capture is running on, and whether that is a fallback from a device that is gone.

    public func audioReception() async -> AudioReception? {
        guard let receivePipeline else { return nil }
        let counters = receivePipeline.snapshot
        var reception = AudioReception()
        reception.packets = counters.datagrams
        reception.ssrc = counters.ssrc
        reception.bytes = counters.datagramBytes
        reception.samples = counters.decodedSamples
        reception.concealed = counters.packetsLost
        reception.discarded = counters.authenticationFailures &+ counters.malformedRedPackets &+ counters.replayDrops
        reception.jitterBufferDelaySeconds = receivePipeline.jitterBufferDwellSeconds
        reception.jitterBufferEmitted = receivePipeline.jitterBufferEmittedCount
        return reception
    }

    public func close() {
        guard !isClosed else { return }
        isClosed = true
        audioDevice?.stop()
        audioDevice = nil
        association?.close()
        association = nil
        receivePipeline = nil
        sendPipeline = nil
        transport?.close()
        transport = nil
        reservation?.release()
        reservation = nil
    }

    // MARK: - Wiring

    private func startAudio(microphone: MicrophoneSetup?) throws {
        guard let transport else { throw BundleError.audioUnavailable("no DTLS transport") }
        do {
            let (keys, profile) = try transport.exportedKeys()
            // Direction comes from the one place that states RFC 5764's ordering: we send with the
            // client values and receive with the server's. Duplicating the mapping here would let a
            // future edit swap it without touching the test that pins it.
            let directions = NvstAudioSrtpDirection.directions(from: keys)
            receivePipeline = try NvstAudioReceivePipeline(
                srtp: try NvstAudioSrtp(masterKey: directions.inbound.key, masterSalt: directions.inbound.salt, profile: profile),
                framesPerPacket: Self.decodedAudioFramesPerPacket,
                channels: Self.microphoneChannels
            )
            sendPipeline = nil
            if let microphone {
                let send = try NvstAudioSendPipeline(
                    srtp: try NvstAudioSrtp(masterKey: directions.outbound.key, masterSalt: directions.outbound.salt, profile: profile),
                    framesPerPacket: Self.microphoneFramesPerPacket,
                    channels: Self.microphoneChannels,
                    initialSequenceNumber: UInt16.random(in: 0...UInt16.max),
                    initialTimestamp: UInt32.random(in: 0...UInt32.max)
                )
                send.isMuted = !microphone.initiallyEnabled
                send.gain = Float(microphone.volume)
                sendPipeline = send
            }
            logger?("NVST native audio keying=DTLS-SRTP profile=\(profile.rawValue) tagBytes=\(profile.authenticationTagLength)")
            // The seat's audio is one stream; the transport only needs to know it is armed.
            onRemoteAudio?(audioChannelCount)
        } catch {
            throw BundleError.audioUnavailable(error.localizedDescription)
        }
        startAudioDevice()
    }

    private func startAudioDevice() {
        // The native decode is stereo end to end: the Opus decoder, the jitter buffer and the
        // receive pipeline all carry two channels. The device is therefore asked for stereo (its
        // own device may still fall to mono), and the fill maps the stereo decode onto whatever
        // channel count the hardware settled on rather than letting a wider layout misread it.
        let device = NvstCoreAudioDevice(playoutChannelCount: Self.decodedAudioChannels,
                                         capturesMicrophone: microphoneSetup != nil,
                                         preferredInputDeviceUID: microphoneSetup?.deviceUniqueID)
        let outputChannels = device.outputChannels
        device.fillPlayout = { [weak self] destination, sampleCount in
            guard let pipeline = self?.receivePipeline else {
                destination.update(repeating: 0, count: sampleCount)
                return
            }
            let frames = sampleCount / max(1, outputChannels)
            let stereo = pipeline.pull(sampleCount: frames * Self.decodedAudioChannels)
            let samples = NvstCoreAudioFormat.interleavedSamples(fromStereo: stereo, frames: frames, outputChannels: outputChannels)
            let count = min(samples.count, sampleCount)
            for index in 0..<count { destination[index] = samples[index] }
            if count < sampleCount { destination.advanced(by: count).update(repeating: 0, count: sampleCount - count) }
        }
        device.onGameAudio = { [weak self] pointer, frames, rate, channels in
            self?.onGameAudioFrame?(pointer, frames, rate, channels)
        }
        device.onMicrophoneAudio = { [weak self] pointer, frames, rate, channels in
            guard let self else { return }
            onMicrophoneAudioFrame?(pointer, frames, rate, channels)
            sendCapturedMicrophone(pointer: pointer, frames: frames)
        }
        device.onMicrophoneLevel = { [weak self] level in
            guard let self else { return }
            microphoneStatsLock.lock()
            microphoneCaptureLevel = level
            microphoneStatsLock.unlock()
            onMicrophoneLevel?(level)
        }
        device.onCaptureDeviceChange = { [weak self] change in
            guard let self, change.isFallback else { return }
            onMicrophoneDeviceFallback?(change)
        }
        device.onInputDeviceListChange = { [weak self] in self?.onMicrophoneDeviceListChange?() }
        device.isMicrophoneCaptureEnabled = { [weak self] in self?.sendPipeline?.isMuted == false }
        if startsAudioDevice {
            device.start()
            if !device.isPlayoutRunning {
                onFailure?("Native NVST audio output could not start (OSStatus \(device.lastStartStatus))")
            }
        }
        audioDevice = device
        logger?("NVST native audio device playout=\(device.isPlayoutRunning) capture=\(device.isCaptureRunning)"
                + " outRate=\(Int(device.outputSampleRate)) outChannels=\(device.outputChannels)"
                + " inRate=\(Int(device.inputSampleRate)) inChannels=\(device.inputChannels)"
                + " latencyMs=\(Int((device.outputPathLatencySeconds * 1000).rounded()))")
    }

    func sendCapturedMicrophone(pointer: UnsafeRawPointer?, frames: UInt32) {
        guard let pointer, let sendPipeline else { return }
        let list = pointer.assumingMemoryBound(to: AudioBufferList.self)
        guard let samples = NvstCoreAudioFormat.stereoCaptureSamples(bufferList: list, frames: frames) else { return }
        microphoneStatsLock.lock()
        microphoneCapturedFrames &+= UInt64(frames)
        microphoneStatsLock.unlock()
        for datagram in sendPipeline.push(capturedPCM: samples) {
            try? transport?.sendRaw(datagram)
        }
    }

    private func startAssociation() throws {
        let association = try NvstSctpAssociation { [weak self] packet in
            do {
                try self?.transport?.sendEncrypted(packet)
            } catch {
                self?.logger?("NVST SCTP DTLS send failed: \(error.localizedDescription)")
            }
        }
        self.association = association
        association.onInboundMessage = { [weak self] message in self?.handleInbound(message) }
        association.onChannelOpened = { [weak self] stream in self?.handleChannelOpened(stream) }
        openChannelsWhenEstablished()
    }

    private func openChannelsWhenEstablished() {
        guard let association, association.isEstablished else { return }
        do {
            try association.openChannels()
            lastChannelOpenError = nil
            guard !hasLoggedChannelOpens else { return }
            hasLoggedChannelOpens = true
            logger?("NVST native bundle SCTP established; sent \(association.requestedChannelCount) channel OPENs")
        } catch {
            let message = error.localizedDescription
            guard lastChannelOpenError != message else { return }
            lastChannelOpenError = message
            logger?("NVST SCTP channel OPEN failed: \(message)")
        }
    }

    // MARK: - Inbound

    private func handleInbound(_ message: NvstSctpAssociation.InboundMessage) {
        if let version = NvstRemoteInput.protocolVersion(in: message.payload), inputProtocolVersion == nil {
            inputProtocolVersion = version
            logger?("NVST native bundle input protocol version \(version)")
            onInputProtocolNegotiated?(version)
        }
        let (commands, _) = NvstControlCommand.parse(message.payload)
        for command in commands { dispatch(command) }
    }

    /// The recovered parsers, routed as the bundle this replaces routed them.
    private func dispatch(_ command: NvstControlCommand) {
        if let termination = NvstSeatTermination.parse(command) {
            logger?("NVST native bundle seat terminated the session: \(termination.summary)")
            onSeatTermination?(termination)
            return
        }
        if let stats = NvstSeatStats.from(command) {
            onSeatStats?(stats)
            return
        }
        if let haptics = NvstHapticEvent.parse(command) {
            if !haptics.isEmpty { onHapticEvents?(haptics) }
            return
        }
        if let hdrMode = NvstHdrModeNotification.parse(command) {
            onHdrMode?(hdrMode)
            return
        }
        if handleSessionLimitCommand(command) { return }
        if command.code == NvstAudioSurroundInfo.commandCode {
            if let info = NvstAudioSurroundInfo.parse(command) {
                logger?("NVST audio-surround-info \(info.summary)")
            }
            return
        }
        guard let cursor = NvstRemoteCursor.from(command) else { return }
        onRemoteCursor?(cursor)
    }

    private func handleChannelOpened(_ stream: UInt16) {
        let label = NvstSctpChannelProfile.official.first { $0.streamID == stream }?.label ?? "stream-\(stream)"
        logger?("NVST native bundle channel open: \(label)")
        switch stream {
        case Self.controlStream: onControlChannelOpen?()
        case Self.partiallyReliableControlStream: onPartiallyReliableControlOpen?()
        case Self.feedbackStream: onFeedbackChannelOpen?()
        default: break
        }
    }
}

extension NvstNativeBundle {
    /// Capture evidence for the diagnostics line: frames pulled from the device and the last meter
    /// reading, so a silent capture is visible rather than inferred from an advancing byte count.
    private var microphoneStatistics: (capturedFrames: UInt64, captureLevel: Double) {
        microphoneStatsLock.lock()
        defer { microphoneStatsLock.unlock() }
        return (microphoneCapturedFrames, microphoneCaptureLevel)
    }

    public var diagnosticSummary: String {
        let sctp = association == nil
            ? "down"
            : "\(association?.diagnosticState ?? "closed")(in=\(association?.inboundPackets ?? 0) opens=\(association?.requestedChannelCount ?? 0) resets=\(association?.streamResetsSeen ?? 0) appData=\(transport?.applicationDatagrams ?? 0))"
        let micStats = microphoneStatistics
        // `dev` is the device capture resolved to and `fallback` says the saved one is gone, which
        // together tell the chosen microphone from the default without opening the HUD.
        let deviceState = microphoneDeviceState
        let device = deviceState.uniqueID.map { "dev=\($0)fallback=\(deviceState.isFallback) " } ?? ""
        let mic = microphoneSenderSsrc == nil ? "off" : "on(\(device)ssrc=\(microphoneSenderSsrc.map(String.init) ?? "?"),tx=\(microphoneSentBytes),pkts=\(sendPipeline?.snapshot.packetsSent ?? 0),frames=\(micStats.capturedFrames),level=\(String(format: "%.3f", micStats.captureLevel)))"
        let audio = receivePipeline.map {
            "receive[datagrams=\($0.snapshot.datagrams) authenticated=\($0.snapshot.authenticated) decoded=\($0.snapshot.packetsDecoded) lost=\($0.snapshot.packetsLost) recovered=\($0.snapshot.recoveredPackets) tagFail=\($0.snapshot.authenticationFailures) decodeFail=\($0.snapshot.decodeFailures) redFail=\($0.snapshot.malformedRedPackets)]"
        } ?? "receive[down]"
        return "sctp=\(sctp) control=\(isControlChannelOpen) feedback=\(isFeedbackChannelOpen) input=\(isInputReady) mic=\(mic) \(audio)"
    }
}


extension NvstNativeBundle {
    /// Whether the microphone was armed for this session, and the SSRC its RTP arrives on. The send
    /// pipeline exists only when a setup was supplied, so this is what ANNOUNCE's
    /// `rtcMicOnNativeBundle` must follow: a pipeline with no requested microphone would advertise
    /// mic carriage the seat never asked for.
    public var microphoneNegotiation: (negotiated: Bool, senderSsrc: UInt32?) {
        let ssrc = microphoneSenderSsrc
        return (ssrc != nil, ssrc)
    }

    /// Mic chat bytes uploaded, as the `0x208` report wants them.
    public var microphoneSentBytes: UInt64 { sendPipeline?.snapshot.bytesSent ?? 0 }

    /// Silences this Mac's speakers only. Applied in the device after the tee, so a recording and a
    /// Co-Op guest keep hearing the game.
    public func setRemoteAudioMuted(_ muted: Bool) {
        audioDevice?.isPlayoutMuted = muted
    }

    /// Flips the microphone gate. Muting stops packets rather than sending silence, which is what the
    /// seat's jitter buffer conceals most cheaply and what makes "mic off" visible in the counters.
    public func setMicrophoneCaptureEnabled(_ enabled: Bool) {
        guard sendPipeline != nil else { return }
        sendPipeline?.isMuted = !enabled
    }

    public func setMicrophoneVolume(_ volume: Double) {
        sendPipeline?.gain = Float(min(max(volume.isFinite ? volume : 1, 0), 1))
    }

    public func setMicrophoneDevice(uid: String?) {
        audioDevice?.setPreferredInputDevice(uid: uid)
    }

    public var microphoneDeviceState: (uniqueID: String?, isFallback: Bool) {
        audioDevice?.captureDeviceState ?? (nil, false)
    }
}
