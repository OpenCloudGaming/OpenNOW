#if canImport(UIKit)
import SwiftUI
import UIKit
import AVFoundation
import CoreHaptics
import GameController
import OSLog

#if canImport(WebRTC) && !os(tvOS)
import WebRTC
#endif

enum NativeStreamerCapability {
    static var isAvailable: Bool {
        #if canImport(WebRTC) && !os(tvOS)
        return true
        #else
        return false
        #endif
    }
}

struct NativeStreamerView: View {
    let session: ActiveSession
    let settings: AppSettings
    let onEvent: (String) -> Void
    let onFallback: (String) -> Void

    var body: some View {
        #if canImport(WebRTC) && !os(tvOS)
        NativeStreamerRenderer(
            session: session,
            settings: settings,
            onEvent: onEvent,
            onFallback: onFallback
        )
        .ignoresSafeArea()
        #else
        Color.black
            .ignoresSafeArea()
            .task {
                onFallback("Native WebRTC is not linked for this target.")
            }
        #endif
    }
}

#if canImport(WebRTC) && !os(tvOS)
private struct NativeStreamerRenderer: UIViewRepresentable {
    let session: ActiveSession
    let settings: AppSettings
    let onEvent: (String) -> Void
    let onFallback: (String) -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(
            session: session,
            settings: settings,
            onEvent: onEvent,
            onFallback: onFallback
        )
    }

    func makeUIView(context: Context) -> OpenNOWNativeStreamRenderView {
        let view = OpenNOWNativeStreamRenderView(frame: .zero)
        context.coordinator.attach(to: view)
        DispatchQueue.main.async {
            view.becomeFirstResponder()
        }
        return view
    }

    func updateUIView(_ uiView: OpenNOWNativeStreamRenderView, context: Context) {
        context.coordinator.update(session: session, settings: settings, view: uiView)
        DispatchQueue.main.async {
            uiView.becomeFirstResponder()
        }
    }

    static func dismantleUIView(_ uiView: OpenNOWNativeStreamRenderView, coordinator: Coordinator) {
        coordinator.detach()
    }

    final class Coordinator {
        private var session: ActiveSession
        private var settings: AppSettings
        private let onEvent: (String) -> Void
        private let onFallback: (String) -> Void
        private let logger = Logger(subsystem: "OpenNOWiOS", category: "NativeStreamer")
        private var client: OpenNOWNativeStreamClient?
        private let hardwareInputBridge = OpenNOWNativeInputBridge()
        private var renderView: OpenNOWNativeStreamRenderView?

        init(
            session: ActiveSession,
            settings: AppSettings,
            onEvent: @escaping (String) -> Void,
            onFallback: @escaping (String) -> Void
        ) {
            self.session = session
            self.settings = settings
            self.onEvent = onEvent
            self.onFallback = onFallback
        }

        func attach(to view: OpenNOWNativeStreamRenderView) {
            renderView = view
            startClient(in: view)
        }

        func update(session: ActiveSession, settings: AppSettings, view: OpenNOWNativeStreamRenderView) {
            self.settings = settings
            guard self.session.id != session.id else {
                client?.update(settings: settings)
                hardwareInputBridge.updateHapticsAvailability()
                return
            }
            self.session = session
            client?.stop()
            startClient(in: view)
        }

        func detach() {
            hardwareInputBridge.detach()
            client?.stop()
            client = nil
            renderView = nil
        }

        private func startClient(in view: OpenNOWNativeStreamRenderView) {
            do {
                try AVAudioSession.sharedInstance().setCategory(.playback, mode: .moviePlayback, options: [.mixWithOthers])
                try AVAudioSession.sharedInstance().setActive(true)
            } catch {
                logger.notice("AVAudioSession setup failed: \(error.localizedDescription, privacy: .public)")
            }

            let client = OpenNOWNativeStreamClient(
                session: session,
                settings: settings,
                renderer: view.videoRenderer,
                onState: { [weak self] message in self?.onEvent("Status: \(message)") },
                onError: { [weak self] message in
                    self?.onEvent("Error: \(message)")
                    self?.onFallback(message)
                },
                onFallback: { [weak self] message in self?.onFallback(message) }
            )
            self.client = client
            view.inputSink = client
            hardwareInputBridge.attach(to: client)
            onEvent("Status: Starting native streamer")
            client.start()
        }
    }
}

private final class OpenNOWNativeStreamRenderView: UIView {
    let videoRenderer = RTCMTLVideoView(frame: .zero)
    private let touchCaptureView = OpenNOWNativeTouchCaptureView(frame: .zero)

    override var canBecomeFirstResponder: Bool { true }

    weak var inputSink: OpenNOWNativeInputSink? {
        didSet { touchCaptureView.inputSink = inputSink }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .black
        isUserInteractionEnabled = true
        videoRenderer.videoContentMode = .scaleAspectFit
        videoRenderer.backgroundColor = .black
        addSubview(videoRenderer)
        touchCaptureView.backgroundColor = .clear
        touchCaptureView.isUserInteractionEnabled = true
        touchCaptureView.isMultipleTouchEnabled = true
        addSubview(touchCaptureView)
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func layoutSubviews() {
        super.layoutSubviews()
        videoRenderer.frame = bounds
        touchCaptureView.frame = bounds
    }
}

private final class OpenNOWNativeTouchCaptureView: UIView {
    weak var inputSink: OpenNOWNativeInputSink?
    private var activeTouch: UITouch?
    private var lastPoint: CGPoint = .zero
    private var downPoint: CGPoint = .zero
    private var downAt: TimeInterval = 0
    private var selecting = false
    private var doubleTapDragCandidate = false
    private var lastTapAt: TimeInterval = -.infinity
    private var lastTapPoint: CGPoint = .zero

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard activeTouch == nil, let touch = touches.first else { return }
        activeTouch = touch
        let point = touch.location(in: self)
        downPoint = point
        lastPoint = point
        downAt = event?.timestamp ?? ProcessInfo.processInfo.systemUptime
        doubleTapDragCandidate = downAt - lastTapAt <= 0.32 && distance(point, lastTapPoint) <= 36
        selecting = false
        if doubleTapDragCandidate {
            lastTapAt = -.infinity
        }
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent?) {
        guard let touch = activeTouch, touches.contains(touch) else { return }
        let point = touch.location(in: self)
        let dx = point.x - lastPoint.x
        let dy = point.y - lastPoint.y
        if doubleTapDragCandidate, !selecting, distance(point, downPoint) > 10 {
            selecting = inputSink?.setTouchMouseButton(true) == true
            doubleTapDragCandidate = false
        }
        inputSink?.sendMouseMove(dx: Int(dx.rounded()), dy: Int(dy.rounded()), partiallyReliable: true)
        lastPoint = point
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent?) {
        finish(touches: touches, event: event)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent?) {
        if selecting {
            _ = inputSink?.setTouchMouseButton(false)
        }
        activeTouch = nil
        selecting = false
        doubleTapDragCandidate = false
    }

    private func finish(touches: Set<UITouch>, event: UIEvent?) {
        guard let touch = activeTouch, touches.contains(touch) else { return }
        let point = touch.location(in: self)
        let now = event?.timestamp ?? ProcessInfo.processInfo.systemUptime
        let wasTap = now - downAt <= 0.45 && distance(point, downPoint) <= 42
        activeTouch = nil
        doubleTapDragCandidate = false
        if selecting {
            selecting = false
            _ = inputSink?.setTouchMouseButton(false)
            return
        }
        if wasTap {
            inputSink?.sendTouchMouseClick()
            lastTapAt = now
            lastTapPoint = point
        }
    }

    private func distance(_ lhs: CGPoint, _ rhs: CGPoint) -> CGFloat {
        hypot(lhs.x - rhs.x, lhs.y - rhs.y)
    }
}

private protocol OpenNOWNativeInputSink: AnyObject {
    func sendKey(keycode: Int, scancode: Int, modifiers: Int, pressed: Bool)
    func sendMouseMove(dx: Int, dy: Int, partiallyReliable: Bool)
    func sendMouseButton(button: Int, pressed: Bool)
    func sendMouseWheel(delta: Int)
    func sendTouchMouseClick()
    func setTouchMouseButton(_ pressed: Bool) -> Bool
    func sendGamepadState(_ state: OpenNOWNativeGamepadState)
    func setHapticsAvailable(_ available: Bool)
}

private struct OpenNOWNativeGamepadState {
    var controllerId: Int
    var buttons: Int
    var leftTrigger: Int
    var rightTrigger: Int
    var leftStickX: Int
    var leftStickY: Int
    var rightStickX: Int
    var rightStickY: Int
    var connectedBitmap: Int
}

private final class OpenNOWNativeStreamClient: NSObject, OpenNOWNativeInputSink {
    private let logger = Logger(subsystem: "OpenNOWiOS", category: "NativeStreamClient")
    private let session: ActiveSession
    private var settings: AppSettings
    private weak var renderer: RTCVideoRenderer?
    private let onState: (String) -> Void
    private let onError: (String) -> Void
    private let onFallback: (String) -> Void
    private let queue = DispatchQueue(label: "OpenNOW.native.stream.client")
    private let inputEncoder = OpenNOWNativeInputEncoder()
    private var peerConnectionFactory: RTCPeerConnectionFactory?
    private var peerConnection: RTCPeerConnection?
    private var signaling: OpenNOWNativeSignalingClient?
    private var reliableInput: RTCDataChannel?
    private var partialInput: RTCDataChannel?
    private var videoTrack: RTCVideoTrack?
    private var audioTrack: RTCAudioTrack?
    private var heartbeatTimer: DispatchSourceTimer?
    private var statsTimer: DispatchSourceTimer?
    private var offerTimeout: DispatchSourceTimer?
    private var reconnectAttempts = 0
    private var generation = 0
    private var inputDropLogged = false
    private var hapticsAvailable = false
    private var hapticsAdvertised: Bool?
    private var partialGamepadMask = 0x0f
    private var lastIceState: RTCIceConnectionState = .new
    private var lastProgressAt = DispatchTime.now()
    private var lastDecodedFrames: Int64?
    private var lastBytesReceived: Int64?
    private var keyframeAttempts = 0

    init(
        session: ActiveSession,
        settings: AppSettings,
        renderer: RTCVideoRenderer,
        onState: @escaping (String) -> Void,
        onError: @escaping (String) -> Void,
        onFallback: @escaping (String) -> Void
    ) {
        self.session = session
        self.settings = settings
        self.renderer = renderer
        self.onState = onState
        self.onError = onError
        self.onFallback = onFallback
	        super.init()
	        RTCInitializeSSL()
	        let encoderFactory = RTCDefaultVideoEncoderFactory()
	        let decoderFactory = OpenNOWVideoDecoderFactory()
	        peerConnectionFactory = RTCPeerConnectionFactory(
	            encoderFactory: encoderFactory,
	            decoderFactory: decoderFactory
	        )
    }

    func update(settings: AppSettings) {
        queue.async { self.settings = settings }
    }

    func start() {
        queue.async {
            self.generation += 1
            self.reconnectAttempts = 0
            self.startTransport(generation: self.generation)
        }
    }

    func stop() {
        queue.sync {
            self.generation += 1
            self.closeTransport()
            if let renderer = self.renderer {
                self.videoTrack?.remove(renderer)
            }
            self.videoTrack = nil
            self.audioTrack = nil
        }
    }

    func sendKey(keycode: Int, scancode: Int, modifiers: Int, pressed: Bool) {
        queue.async {
            let packet = self.inputEncoder.encodeKey(
                keycode: keycode,
                scancode: scancode,
                modifiers: modifiers,
                pressed: pressed
            )
            _ = self.sendReliable(packet)
        }
    }

    func sendMouseMove(dx: Int, dy: Int, partiallyReliable: Bool) {
        queue.async {
            let packet = self.inputEncoder.encodeMouseMove(dx: dx, dy: dy)
            _ = self.sendInput(packet, partiallyReliable: partiallyReliable, fallbackToReliable: true)
        }
    }

    func sendMouseButton(button: Int, pressed: Bool) {
        queue.async {
            let packet = self.inputEncoder.encodeMouseButton(button: button, pressed: pressed)
            _ = self.sendReliable(packet)
        }
    }

    func sendMouseWheel(delta: Int) {
        queue.async {
            let packet = self.inputEncoder.encodeMouseWheel(delta: delta)
            _ = self.sendReliable(packet)
        }
    }

    func sendTouchMouseClick() {
        queue.async {
            guard self.setTouchMouseButtonOnQueue(true) else { return }
            self.queue.asyncAfter(deadline: .now() + .milliseconds(160)) {
                _ = self.setTouchMouseButtonOnQueue(false)
            }
        }
    }

    func setTouchMouseButton(_ pressed: Bool) -> Bool {
        var sent = false
        queue.sync {
            sent = setTouchMouseButtonOnQueue(pressed)
        }
        return sent
    }

    func sendGamepadState(_ state: OpenNOWNativeGamepadState) {
        queue.async {
            let usePartial = self.canSendGamepadPartiallyReliable(controllerId: state.controllerId)
            let packet = self.inputEncoder.encodeGamepadState(state, partiallyReliable: usePartial)
            _ = self.sendInput(packet, partiallyReliable: usePartial, fallbackToReliable: !usePartial)
        }
    }

    func setHapticsAvailable(_ available: Bool) {
        queue.async {
            self.hapticsAvailable = available
            self.advertiseHaptics(force: true)
        }
    }

    private func setTouchMouseButtonOnQueue(_ pressed: Bool) -> Bool {
        let packet = inputEncoder.encodeMouseButton(button: 1, pressed: pressed)
        let reliable = sendInput(packet, partiallyReliable: false, fallbackToReliable: true)
        let partial = sendInput(packet, partiallyReliable: true, fallbackToReliable: false)
        return reliable || partial
    }

    private func startTransport(generation: Int) {
        closeTransport(clearInputState: false)
        inputDropLogged = false
        hapticsAdvertised = nil
        keyframeAttempts = 0
        lastIceState = .new
        lastProgressAt = .now()
        emitState(reconnectAttempts > 0 ? "Reconnecting signaling" : "Connecting signaling")
        let profile = StreamSettingsResolver.profile(for: settings)
        let signaling = OpenNOWNativeSignalingClient(
            session: session,
            profile: profile,
            onEvent: { [weak self] event in
                self?.queue.async {
                    self?.handleSignaling(event, generation: generation)
                }
            }
        )
        self.signaling = signaling
        signaling.connect()
        startOfferTimeout(generation: generation)
    }

    private func closeTransport(clearInputState: Bool = true) {
        heartbeatTimer?.cancel()
        statsTimer?.cancel()
        offerTimeout?.cancel()
        heartbeatTimer = nil
        statsTimer = nil
        offerTimeout = nil
        signaling?.disconnect()
        signaling = nil
        reliableInput?.delegate = nil
        partialInput?.delegate = nil
        reliableInput = nil
        partialInput = nil
        peerConnection?.close()
        peerConnection = nil
        if clearInputState {
            inputEncoder.resetGamepadSequences()
        }
    }

    private func handleSignaling(_ event: OpenNOWNativeSignalingEvent, generation: Int) {
        guard generation == self.generation else { return }
        switch event {
        case .connected:
            emitState("Waiting for offer")
        case .disconnected(let reason):
            scheduleReconnect(reason: "Signaling disconnected: \(reason)", delay: 1.0, generation: generation)
        case .error(let message):
            scheduleReconnect(reason: "Signaling failed: \(message)", delay: 1.0, generation: generation)
        case .offer(let sdp):
            handleOffer(sdp, generation: generation)
        case .remoteIce(let candidate):
            peerConnection?.add(candidate) { [weak self] error in
                if let error {
                    self?.emitState("Remote ICE candidate failed: \(error.localizedDescription)")
                }
            }
        case .log(let message):
            emitState(message)
        }
    }

    private func handleOffer(_ rawOffer: String, generation: Int) {
        offerTimeout?.cancel()
        offerTimeout = nil
        let profile = StreamSettingsResolver.profile(for: settings)
        let prepared = prepareRemoteOffer(rawOffer)
        let preferredCodec = OpenNOWNativeSDP.resolvePreferredCodec(settings.preferredCodec, offerSdp: prepared)
        let preferred = OpenNOWNativeSDP.preferCodec(
            prepared,
            codec: preferredCodec,
            preferTenBit: OpenNOWNativeSDP.prefersTenBitVideo(settings: settings)
        )
        inputEncoder.protocolVersion = OpenNOWNativeSDP.parseInputProtocolVersion(preferred)
        partialGamepadMask = OpenNOWNativeSDP.parsePartiallyReliableGamepadMask(preferred)

        guard let pc = makePeerConnection() else {
            fail("Failed to create native WebRTC peer connection", generation: generation)
            return
        }
        ensureInputChannels(on: pc, offerSdp: preferred)
        let offer = RTCSessionDescription(type: .offer, sdp: preferred)
        pc.setRemoteDescription(offer) { [weak self] error in
            guard let self else { return }
            self.queue.async {
                guard generation == self.generation else { return }
                if let error {
                    self.fail("Native WebRTC rejected server offer: \(error.localizedDescription)", generation: generation)
                    return
                }
                let constraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
                self.applyVideoCodecPreferences(on: pc, codec: preferredCodec)
                pc.answer(for: constraints) { answer, error in
                    self.queue.async {
                        guard generation == self.generation else { return }
                        if let error {
                            self.fail("Native WebRTC failed to create answer: \(error.localizedDescription)", generation: generation)
                            return
                        }
                        guard let answer else {
                            self.fail("Native WebRTC returned an empty answer", generation: generation)
                            return
                        }
                        let munged = OpenNOWNativeSDP.mungeAnswerSdp(answer.sdp, maxBitrateKbps: profile.maxBitrateKbps)
                        if preferredCodec != "H264",
                           !OpenNOWNativeSDP.negotiatesCodec(munged, codec: preferredCodec) {
                            self.fail(
                                "\(preferredCodec) was requested but native WebRTC did not negotiate it in the local answer.",
                                generation: generation
                            )
                            return
                        }
                        let localAnswer = RTCSessionDescription(type: .answer, sdp: munged)
                        pc.setLocalDescription(localAnswer) { error in
                            self.queue.async {
                                guard generation == self.generation else { return }
                                if let error {
                                    self.fail("Native WebRTC failed to apply answer: \(error.localizedDescription)", generation: generation)
                                    return
                                }
                                let nvst = OpenNOWNativeSDP.buildNvstSdp(
                                    localAnswer: munged,
                                    offerSdp: preferred,
                                    settings: self.settings,
                                    profile: profile,
                                    codec: preferredCodec
                                )
                                self.signaling?.sendAnswer(sdp: munged, nvstSdp: nvst)
                                self.emitState("Native streamer negotiated")
                                self.startHeartbeat()
                                self.startStatsPolling()
                            }
                        }
                    }
                }
            }
        }
    }

    private func prepareRemoteOffer(_ rawOffer: String) -> String {
        var prepared = OpenNOWNativeSDP.fixServerIp(
            rawOffer,
            serverIp: session.serverIp ?? session.signalingServer ?? ""
        )
        let preferredCodec = OpenNOWNativeSDP.resolvePreferredCodec(settings.preferredCodec, offerSdp: prepared)
        guard preferredCodec == "H265" else { return prepared }

        let maxLevels = OpenNOWNativeCodecSupport.h265ReceiverMaxLevelsByProfile(factory: peerConnectionFactory)
        if !maxLevels.isEmpty {
            let rewritten = OpenNOWNativeSDP.rewriteH265LevelIdByProfile(prepared, maxLevelByProfile: maxLevels)
            if rewritten.replacements > 0 {
                emitState("HEVC level compatibility applied")
                prepared = rewritten.sdp
            }
        }

        if !OpenNOWNativeCodecSupport.supportsH265TierFlagOne(factory: peerConnectionFactory) {
            let rewritten = OpenNOWNativeSDP.rewriteH265TierFlag(prepared, tierFlag: 0)
            if rewritten.replacements > 0 {
                emitState("HEVC tier compatibility applied")
                prepared = rewritten.sdp
            }
        }

        return prepared
    }

    private func applyVideoCodecPreferences(on pc: RTCPeerConnection, codec: String) {
        let preferences = OpenNOWNativeCodecSupport.receiverCodecPreferences(
            factory: peerConnectionFactory,
            codec: codec,
            preferTenBit: OpenNOWNativeSDP.prefersTenBitVideo(settings: settings)
        )
        guard !preferences.isEmpty else {
            emitState("Native \(codec) is not listed in receiver capabilities")
            return
        }
        guard let transceiver = pc.transceivers.first(where: { transceiver in
            transceiver.mediaType == .video ||
                transceiver.receiver.track?.kind == kRTCMediaStreamTrackKindVideo
        }) else { return }
        do {
            try transceiver.setCodecPreferences(preferences, error: ())
            emitState("Native codec preference: \(codec)")
        } catch {
            emitState("Native codec preference failed: \(error.localizedDescription)")
        }
    }

    private func makePeerConnection() -> RTCPeerConnection? {
        if let peerConnection { return peerConnection }
        guard let factory = peerConnectionFactory else { return nil }
        let config = RTCConfiguration()
        config.iceServers = session.iceServers.map {
            RTCIceServer(urlStrings: $0.urls, username: $0.username, credential: $0.credential)
        }
        config.sdpSemantics = .unifiedPlan
        config.continualGatheringPolicy = .gatherContinually
        config.tcpCandidatePolicy = .disabled
        config.bundlePolicy = .maxBundle
        config.rtcpMuxPolicy = .require
        let constraints = RTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
        let pc = factory.peerConnection(with: config, constraints: constraints, delegate: self)
        peerConnection = pc
        return pc
    }

    private func ensureInputChannels(on pc: RTCPeerConnection, offerSdp: String) {
        if reliableInput == nil {
            let config = RTCDataChannelConfiguration()
            config.isOrdered = true
            if let channel = pc.dataChannel(forLabel: "input_channel_v1", configuration: config) {
                attachInputChannel(channel)
            }
        }
        if partialInput == nil {
            let config = RTCDataChannelConfiguration()
            config.isOrdered = false
            config.maxPacketLifeTime = Int32(OpenNOWNativeSDP.parsePartialReliableThresholdMs(offerSdp))
            if let channel = pc.dataChannel(forLabel: "input_channel_partially_reliable", configuration: config) {
                attachInputChannel(channel)
            }
        }
    }

    private func attachInputChannel(_ channel: RTCDataChannel) {
        channel.delegate = self
        switch OpenNOWNativeInputChannelRole(label: channel.label) {
        case .reliable:
            reliableInput = channel
        case .partiallyReliable:
            partialInput = channel
        case .other:
            break
        }
    }

    private func sendReliable(_ data: Data) -> Bool {
        if sendInput(data, partiallyReliable: false, fallbackToReliable: true) {
            return true
        }
        return sendInput(data, partiallyReliable: true, fallbackToReliable: false)
    }

    private func sendInput(_ data: Data, partiallyReliable: Bool, fallbackToReliable: Bool) -> Bool {
        let preferred = partiallyReliable ? partialInput : reliableInput
        let channel = preferred?.readyState == .open ? preferred : (fallbackToReliable ? reliableInput : nil)
        guard channel?.readyState == .open else {
            if !inputDropLogged {
                inputDropLogged = true
                logger.notice("Native input dropped: reliable=\(String(describing: self.reliableInput?.readyState.rawValue), privacy: .public) partial=\(String(describing: self.partialInput?.readyState.rawValue), privacy: .public)")
            }
            return false
        }
        return channel?.sendData(RTCDataBuffer(data: data, isBinary: true)) == true
    }

    private func canSendGamepadPartiallyReliable(controllerId: Int) -> Bool {
        guard partialInput?.readyState == .open else { return false }
        return (partialGamepadMask & (1 << (controllerId & 0x1f))) != 0
    }

    private func advertiseHaptics(force: Bool = false) {
        guard reliableInput?.readyState == .open else { return }
        guard force || hapticsAdvertised != hapticsAvailable else { return }
        if sendReliable(inputEncoder.encodeHapticsEnabled(hapticsAvailable)) {
            hapticsAdvertised = hapticsAvailable
        }
    }

    private func startHeartbeat() {
        heartbeatTimer?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + 1, repeating: 1)
        timer.setEventHandler { [weak self] in
            guard let self else { return }
            _ = self.sendReliable(self.inputEncoder.encodeHeartbeat())
        }
        heartbeatTimer = timer
        timer.resume()
    }

    private func startStatsPolling() {
        statsTimer?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + 1, repeating: 1)
        timer.setEventHandler { [weak self] in
            self?.pollStats()
        }
        statsTimer = timer
        timer.resume()
    }

    private func pollStats() {
        guard let peerConnection else { return }
        peerConnection.statistics { [weak self] report in
            guard let self else { return }
            self.queue.async {
                self.handleStats(report)
            }
        }
    }

    private func handleStats(_ report: RTCStatisticsReport) {
        var bytesReceived: Int64?
        var framesDecoded: Int64?
        for (_, statistic) in report.statistics {
            guard statistic.type == "inbound-rtp" else { continue }
            let values = statistic.values
            let kind = values["kind"] as? String ?? values["mediaType"] as? String
            guard kind == "video" else { continue }
            bytesReceived = Self.int64Value(values["bytesReceived"])
            framesDecoded = Self.int64Value(values["framesDecoded"])
            break
        }
        let progressed = (framesDecoded.map { $0 > (lastDecodedFrames ?? -1) } ?? false)
            || (bytesReceived.map { $0 > (lastBytesReceived ?? -1) } ?? false)
        if let bytesReceived { lastBytesReceived = bytesReceived }
        if let framesDecoded { lastDecodedFrames = framesDecoded }
        guard lastIceState == .connected || lastIceState == .completed else {
            lastProgressAt = .now()
            return
        }
        if progressed {
            lastProgressAt = .now()
            keyframeAttempts = 0
            return
        }
        let stalledMs = Self.elapsedMilliseconds(since: lastProgressAt)
        if stalledMs >= 14_000 {
            scheduleReconnect(reason: "Media stalled for \(stalledMs / 1000)s", delay: 0, generation: generation)
        } else if stalledMs >= 5_000 {
            keyframeAttempts += 1
            signaling?.requestKeyframe(reason: "media_stall", backlogFrames: 0, attempt: keyframeAttempts)
            emitState("Recovering video")
        }
    }

    private func startOfferTimeout(generation: Int) {
        offerTimeout?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + 12)
        timer.setEventHandler { [weak self] in
            guard let self, generation == self.generation, self.peerConnection == nil else { return }
            self.onFallback("Native streamer timed out waiting for a server offer.")
        }
        offerTimeout = timer
        timer.resume()
    }

    private func scheduleReconnect(reason: String, delay: TimeInterval, generation: Int) {
        guard generation == self.generation else { return }
        guard reconnectAttempts < 3 else {
            fail("\(reason). Native stream reconnect failed after 3 attempts.", generation: generation)
            return
        }
        reconnectAttempts += 1
        let nextGeneration = self.generation + 1
        self.generation = nextGeneration
        emitState("Reconnecting native stream (\(reconnectAttempts)/3)")
        closeTransport(clearInputState: false)
        queue.asyncAfter(deadline: .now() + delay) {
            guard nextGeneration == self.generation else { return }
            self.startTransport(generation: nextGeneration)
        }
    }

    private func fail(_ message: String, generation: Int) {
        guard generation == self.generation else { return }
        self.generation += 1
        closeTransport()
        emitError(message)
    }

    private func emitState(_ message: String) {
        DispatchQueue.main.async { self.onState(message) }
    }

    private func emitError(_ message: String) {
        DispatchQueue.main.async { self.onError(message) }
    }

    private static func int64Value(_ value: NSObject?) -> Int64? {
        if let number = value as? NSNumber {
            return number.int64Value
        }
        return value as? Int64
    }

    private static func elapsedMilliseconds(since start: DispatchTime) -> Int64 {
        let nanos = DispatchTime.now().uptimeNanoseconds - start.uptimeNanoseconds
        return Int64(nanos / 1_000_000)
    }
}

extension OpenNOWNativeStreamClient: RTCPeerConnectionDelegate {
    func peerConnection(_ peerConnection: RTCPeerConnection, didChange stateChanged: RTCSignalingState) {}

    func peerConnection(_ peerConnection: RTCPeerConnection, didAdd stream: RTCMediaStream) {
        queue.async {
            stream.videoTracks.first.map(self.attachVideoTrack)
            if let audio = stream.audioTracks.first {
                self.audioTrack = audio
                audio.isEnabled = true
            }
        }
    }

    func peerConnection(_ peerConnection: RTCPeerConnection, didRemove stream: RTCMediaStream) {}

    func peerConnectionShouldNegotiate(_ peerConnection: RTCPeerConnection) {}

    func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceConnectionState) {
        queue.async {
            self.lastIceState = newState
            switch newState {
            case .connected, .completed:
                self.lastProgressAt = .now()
                self.reconnectAttempts = 0
                self.emitState("Peer: connected")
            case .disconnected:
                self.emitState("ICE_DISCONNECTED")
                self.scheduleReconnect(reason: "ICE disconnected", delay: 3.5, generation: self.generation)
            case .failed:
                self.emitState("ICE_FAILED")
                self.scheduleReconnect(reason: "ICE failed", delay: 0.25, generation: self.generation)
            case .checking:
                self.emitState("ICE_CHECKING")
            case .new:
                self.emitState("ICE_NEW")
            case .closed, .count:
                break
            @unknown default:
                break
            }
        }
    }

    func peerConnection(_ peerConnection: RTCPeerConnection, didChange newState: RTCIceGatheringState) {}

    func peerConnection(_ peerConnection: RTCPeerConnection, didGenerate candidate: RTCIceCandidate) {
        queue.async {
            guard !candidate.sdp.localizedCaseInsensitiveContains(" tcp ") else { return }
            self.signaling?.sendIceCandidate(candidate)
        }
    }

    func peerConnection(_ peerConnection: RTCPeerConnection, didRemove candidates: [RTCIceCandidate]) {}

    func peerConnection(_ peerConnection: RTCPeerConnection, didOpen dataChannel: RTCDataChannel) {
        queue.async {
            self.attachInputChannel(dataChannel)
        }
    }

    func peerConnection(_ peerConnection: RTCPeerConnection, didStartReceivingOn transceiver: RTCRtpTransceiver) {
        queue.async {
            if let video = transceiver.receiver.track as? RTCVideoTrack {
                self.attachVideoTrack(video)
            }
            if let audio = transceiver.receiver.track as? RTCAudioTrack {
                self.audioTrack = audio
                audio.isEnabled = true
            }
        }
    }

    private func attachVideoTrack(_ track: RTCVideoTrack) {
        if videoTrack === track { return }
        if let oldTrack = videoTrack {
            if let renderer {
                oldTrack.remove(renderer)
            }
        }
        videoTrack = track
        track.isEnabled = true
        if let renderer {
            track.add(renderer)
        }
        emitState("Peer: connected")
    }
}

extension OpenNOWNativeStreamClient: RTCDataChannelDelegate {
    func dataChannelDidChangeState(_ dataChannel: RTCDataChannel) {
        queue.async {
            if dataChannel.readyState == .open {
                self.inputDropLogged = false
                self.inputEncoder.resetGamepadSequences()
                self.advertiseHaptics(force: true)
                self.emitState("Input ready")
            } else if dataChannel === self.reliableInput {
                self.hapticsAdvertised = nil
            }
        }
    }

    func dataChannel(_ dataChannel: RTCDataChannel, didReceiveMessageWith buffer: RTCDataBuffer) {
        queue.async {
            let bytes = [UInt8](buffer.data)
            if self.handleInputHandshake(bytes) {
                return
            }
            if let command = OpenNOWNativeHapticsParser.parse(bytes) {
                DispatchQueue.main.async {
                    OpenNOWNativeControllerHaptics.shared.apply(command)
                }
            }
        }
    }

    private func handleInputHandshake(_ bytes: [UInt8]) -> Bool {
        guard bytes.count >= 2 else { return false }
        let firstWord = Int(bytes[0]) | (Int(bytes[1]) << 8)
        let version: Int
        if firstWord == 526 {
            version = bytes.count >= 4 ? (Int(bytes[2]) | (Int(bytes[3]) << 8)) : 2
        } else if bytes[0] == 0x0e {
            version = firstWord
        } else {
            return false
        }
        inputEncoder.protocolVersion = max(2, version)
        inputEncoder.resetGamepadSequences()
        advertiseHaptics(force: true)
        emitState("Input ready")
        return true
    }
}

private enum OpenNOWNativeInputChannelRole {
    case reliable
    case partiallyReliable
    case other

    init(label: String) {
        switch label.lowercased() {
        case "input_channel_v1", "input_channel":
            self = .reliable
        case "input_channel_partially_reliable", "input_channel_pr":
            self = .partiallyReliable
        default:
            self = .other
        }
    }
}

private enum OpenNOWNativeSignalingEvent {
    case connected
    case disconnected(String)
    case error(String)
    case offer(String)
    case remoteIce(RTCIceCandidate)
    case log(String)
}

private final class OpenNOWNativeSignalingClient {
    private let session: ActiveSession
    private let profile: StreamVideoProfile
    private let onEvent: (OpenNOWNativeSignalingEvent) -> Void
    private let queue = DispatchQueue(label: "OpenNOW.native.signaling")
    private var task: URLSessionWebSocketTask?
    private var heartbeatTimer: DispatchSourceTimer?
    private var peerId = 0
    private var remotePeerId = 1
    private var ackCounter = 0
    private let peerName = "peer-\(UUID().uuidString.replacingOccurrences(of: "-", with: "").prefix(12))"
    private lazy var urlSession: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.httpAdditionalHeaders = [
            "Origin": "https://play.geforcenow.com",
            "User-Agent": Self.userAgent
        ]
        return URLSession(configuration: configuration)
    }()
    private static let userAgent = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/131.0.0.0 Safari/537.36"

    init(
        session: ActiveSession,
        profile: StreamVideoProfile,
        onEvent: @escaping (OpenNOWNativeSignalingEvent) -> Void
    ) {
        self.session = session
        self.profile = profile
        self.onEvent = onEvent
    }

    func connect() {
        queue.async {
            guard let url = self.buildSignInURL() else {
                self.onEvent(.error("Missing native signaling URL"))
                return
            }
            let task = self.urlSession.webSocketTask(
                with: url,
                protocols: ["x-nv-sessionid.\(self.session.id)"]
            )
            self.task = task
            task.resume()
            self.sendPeerInfo()
            self.startHeartbeat()
            self.receiveNext()
            self.onEvent(.connected)
        }
    }

    func disconnect() {
        queue.async {
            self.heartbeatTimer?.cancel()
            self.heartbeatTimer = nil
            self.task?.cancel(with: .normalClosure, reason: nil)
            self.task = nil
        }
    }

    func sendAnswer(sdp: String, nvstSdp: String?) {
        var payload: [String: Any] = ["type": "answer", "sdp": sdp]
        if let nvstSdp {
            payload["nvstSdp"] = nvstSdp
        }
        sendPeerMessage(payload)
    }

    func sendIceCandidate(_ candidate: RTCIceCandidate) {
        sendPeerMessage([
            "candidate": candidate.sdp,
            "sdpMid": candidate.sdpMid ?? "",
            "sdpMLineIndex": candidate.sdpMLineIndex
        ])
    }

    func requestKeyframe(reason: String, backlogFrames: Int, attempt: Int) {
        sendPeerMessage([
            "type": "request_keyframe",
            "reason": reason,
            "backlogFrames": backlogFrames,
            "attempt": attempt
        ])
    }

    private func buildSignInURL() -> URL? {
        let signalingServer = session.signalingServer ?? session.serverIp ?? URL(string: session.streamingBaseUrl)?.host ?? ""
        let fallbackHost = signalingServer.contains(":") ? signalingServer : "\(signalingServer):443"
        let base = session.signalingUrl?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false
            ? session.signalingUrl!
            : "wss://\(fallbackHost)/nvst/"
        guard var components = URLComponents(string: base) else { return nil }
        components.scheme = "wss"
        let path = components.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        components.path = "/\(path)/sign_in".replacingOccurrences(of: "//", with: "/")
        components.queryItems = [
            URLQueryItem(name: "peer_id", value: peerName),
            URLQueryItem(name: "version", value: "2"),
            URLQueryItem(name: "peer_role", value: "1"),
            URLQueryItem(name: "pairing_id", value: session.id)
        ]
        return components.url
    }

    private func receiveNext() {
        task?.receive { [weak self] result in
            guard let self else { return }
            self.queue.async {
                switch result {
                case .success(let message):
                    switch message {
                    case .string(let text):
                        self.handle(text)
                    case .data(let data):
                        self.handle(String(data: data, encoding: .utf8) ?? "")
                    @unknown default:
                        break
                    }
                    self.receiveNext()
                case .failure(let error):
                    self.onEvent(.error(error.localizedDescription))
                }
            }
        }
    }

    private func handle(_ text: String) {
        guard let data = text.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            onEvent(.log("Ignoring non-JSON signaling packet"))
            return
        }
        if let info = object["peer_info"] as? [String: Any],
           info["name"] as? String == peerName,
           let id = info["id"] as? Int {
            peerId = id
        }
        if let ack = object["ackid"] as? Int {
            let remoteInfo = object["peer_info"] as? [String: Any]
            let remoteId = remoteInfo?["id"] as? Int
            if remoteId != peerId {
                sendJSON(["ack": ack])
            }
        }
        if object["hb"] != nil {
            sendJSON(["hb": 1])
            return
        }
        guard let peerMessage = object["peer_msg"] as? [String: Any] else { return }
        remotePeerId = peerMessage["from"] as? Int ?? remotePeerId
        guard let message = peerMessage["msg"] as? String,
              let messageData = message.data(using: .utf8),
              let payload = try? JSONSerialization.jsonObject(with: messageData) as? [String: Any] else {
            return
        }
        if payload["type"] as? String == "offer", let sdp = payload["sdp"] as? String {
            onEvent(.offer(sdp))
            return
        }
        if let candidate = payload["candidate"] as? String {
            onEvent(.remoteIce(RTCIceCandidate(
                sdp: candidate,
                sdpMLineIndex: Int32(payload["sdpMLineIndex"] as? Int ?? 0),
                sdpMid: payload["sdpMid"] as? String
            )))
        }
    }

    private func sendPeerInfo() {
        sendJSON([
            "ackid": nextAck(),
            "peer_info": [
                "browser": "Chrome",
                "browserVersion": "131",
                "connected": true,
                "id": peerId,
                "name": peerName,
                "peerRole": 0,
                "resolution": "\(profile.width)x\(profile.height)",
                "version": 2
            ]
        ])
    }

    private func sendPeerMessage(_ payload: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: payload),
              let message = String(data: data, encoding: .utf8) else { return }
        sendJSON([
            "peer_msg": [
                "from": peerId,
                "to": remotePeerId,
                "msg": message
            ],
            "ackid": nextAck()
        ])
    }

    private func sendJSON(_ object: [String: Any]) {
        guard let data = try? JSONSerialization.data(withJSONObject: object),
              let text = String(data: data, encoding: .utf8) else { return }
        task?.send(.string(text)) { [weak self] error in
            if let error {
                self?.onEvent(.error(error.localizedDescription))
            }
        }
    }

    private func startHeartbeat() {
        heartbeatTimer?.cancel()
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + 5, repeating: 5)
        timer.setEventHandler { [weak self] in
            self?.sendJSON(["hb": 1])
        }
        heartbeatTimer = timer
        timer.resume()
    }

    private func nextAck() -> Int {
        ackCounter += 1
        return ackCounter
    }
}

private final class OpenNOWVideoDecoderFactory: NSObject, RTCVideoDecoderFactory {
    private let defaultFactory = RTCDefaultVideoDecoderFactory()

    func createDecoder(_ info: RTCVideoCodecInfo) -> RTCVideoDecoder? {
        if OpenNOWNativeSDP.normalizeCodec(info.name) == "AV1" {
            return RTCVideoDecoderAV1.av1Decoder()
        }
        return defaultFactory.createDecoder(info)
    }

    func supportedCodecs() -> [RTCVideoCodecInfo] {
        var codecs = defaultFactory.supportedCodecs()
        let existing = Set(codecs.map(Self.codecKey))
        for codec in RTCVideoDecoderAV1.supportedCodecs() where !existing.contains(Self.codecKey(codec)) {
            codecs.append(codec)
        }
        return codecs
    }

    private static func codecKey(_ codec: RTCVideoCodecInfo) -> String {
        let params = codec.parameters
            .map { "\($0.key.lowercased())=\($0.value)" }
            .sorted()
            .joined(separator: ";")
        return "\(OpenNOWNativeSDP.normalizeCodec(codec.name)):\(params)"
    }
}

private enum OpenNOWNativeCodecSupport {
    private static let auxiliaryVideoCodecs: Set<String> = ["RTX", "RED", "ULPFEC", "FLEXFEC-03"]

    static func receiverCodecPreferences(
        factory: RTCPeerConnectionFactory?,
        codec: String,
        preferTenBit: Bool
    ) -> [RTCRtpCodecCapability] {
        let receiverCaps = receiverCapabilities(factory: factory)
        let target = OpenNOWNativeSDP.normalizeCodec(codec)
        var preferred = receiverCaps.filter { codecName($0) == target }
        if target == "H265" {
            preferred.sort {
                h265ProfilePriority($0, preferTenBit: preferTenBit) <
                    h265ProfilePriority($1, preferTenBit: preferTenBit)
            }
        }
        guard !preferred.isEmpty else { return [] }
        let auxiliary = receiverCaps.filter { auxiliaryVideoCodecs.contains(codecName($0)) }
        var seen = Set<String>()
        return (preferred + auxiliary).filter { capability in
            seen.insert(preferenceKey(capability)).inserted
        }
    }

    static func h265ReceiverMaxLevelsByProfile(factory: RTCPeerConnectionFactory?) -> [Int: Int] {
        var result: [Int: Int] = [:]
        for capability in receiverH265Capabilities(factory: factory) {
            guard let profile = parameterInt(capability, "profile-id"),
                  let level = parameterInt(capability, "level-id") else { continue }
            result[profile] = max(result[profile] ?? 0, level)
        }
        return result.filter { $0.value > 0 }
    }

    static func supportsH265TierFlagOne(factory: RTCPeerConnectionFactory?) -> Bool {
        receiverH265Capabilities(factory: factory).contains {
            parameterInt($0, "tier-flag") == 1
        }
    }

    private static func receiverH265Capabilities(factory: RTCPeerConnectionFactory?) -> [RTCRtpCodecCapability] {
        receiverCapabilities(factory: factory).filter { codecName($0) == "H265" }
    }

    private static func receiverCapabilities(factory: RTCPeerConnectionFactory?) -> [RTCRtpCodecCapability] {
        guard let factory else { return [] }
        return factory.rtpReceiverCapabilities(forKind: kRTCMediaStreamTrackKindVideo).codecs
    }

    private static func codecName(_ capability: RTCRtpCodecCapability) -> String {
        let mimeSubtype = capability.mimeType.components(separatedBy: "/").last
        let candidates = [mimeSubtype, capability.name]
        for candidate in candidates {
            let normalized = OpenNOWNativeSDP.normalizeCodec(candidate ?? "")
            if !normalized.isEmpty {
                return normalized
            }
        }
        return ""
    }

    private static func h265ProfilePriority(_ capability: RTCRtpCodecCapability, preferTenBit: Bool) -> Int {
        let profile = parameterString(capability, "profile-id")
        if preferTenBit {
            switch profile {
            case "2": return 0
            case "1": return 1
            default: return 2
            }
        }
        switch profile {
        case "1": return 0
        case nil: return 1
        case "2": return 2
        default: return 3
        }
    }

    private static func parameterInt(_ capability: RTCRtpCodecCapability, _ key: String) -> Int? {
        parameterString(capability, key).flatMap(Int.init)
    }

    private static func parameterString(_ capability: RTCRtpCodecCapability, _ key: String) -> String? {
        capability.parameters.first {
            $0.key.caseInsensitiveCompare(key) == .orderedSame
        }?.value
    }

    private static func preferenceKey(_ capability: RTCRtpCodecCapability) -> String {
        let params = capability.parameters
            .map { "\($0.key.lowercased())=\($0.value)" }
            .sorted()
            .joined(separator: ";")
        return "\(codecName(capability)):\(params)"
    }
}

private enum OpenNOWNativeSDP {
    struct RewriteResult {
        let sdp: String
        let replacements: Int
    }

    static func resolvePreferredCodec(_ preferred: String, offerSdp: String) -> String {
        let normalized = normalizeCodec(preferred)
        if normalized == "AUTO" || normalized.isEmpty {
            return offerHasCodec(offerSdp, codec: "H264") ? "H264" : "H265"
        }
        return normalized == "HEVC" ? "H265" : normalized
    }

    static func prefersTenBitVideo(settings: AppSettings) -> Bool {
        settings.hdrEnabled || (StreamColorQuality(rawValue: settings.preferredColorQuality)?.bitDepth == 10)
    }

    static func fixServerIp(_ sdp: String, serverIp: String) -> String {
        guard let ip = extractPublicIp(serverIp) else { return sdp }
        var fixed = sdp.replacingOccurrences(of: "c=IN IP4 0.0.0.0", with: "c=IN IP4 \(ip)")
        fixed = fixed.replacingOccurrences(
            of: #"(a=candidate:\S+\s+\d+\s+\w+\s+\d+\s+)0\.0\.0\.0(\s+)"#,
            with: "$1\(ip)$2",
            options: .regularExpression
        )
        return fixed
    }

    static func preferCodec(_ sdp: String, codec: String, preferTenBit: Bool = false) -> String {
        let target = normalizeCodec(codec)
        let lineEnding = sdp.contains("\r\n") ? "\r\n" : "\n"
        let lines = sdp.components(separatedBy: CharacterSet.newlines)
        var inVideo = false
        var codecByPayload: [String: String] = [:]
        var rtxAptByPayload: [String: String] = [:]
        var fmtpByPayload: [String: String] = [:]
        for line in lines {
            if line.hasPrefix("m=video") {
                inVideo = true
            } else if line.hasPrefix("m="), inVideo {
                inVideo = false
            }
            guard inVideo else { continue }
            if line.hasPrefix("a=rtpmap:") {
                let rest = String(line.dropFirst("a=rtpmap:".count))
                let pieces = rest.split(maxSplits: 1, whereSeparator: { $0 == " " || $0 == "\t" }).map(String.init)
                if pieces.count == 2 {
                    codecByPayload[pieces[0]] = normalizeCodec(String(pieces[1].prefix { $0 != "/" }))
                }
            } else if line.hasPrefix("a=fmtp:") {
                let rest = String(line.dropFirst("a=fmtp:".count))
                let pieces = rest.split(maxSplits: 1, whereSeparator: { $0 == " " || $0 == "\t" }).map(String.init)
                if pieces.count == 2 {
                    fmtpByPayload[pieces[0]] = pieces[1]
                    if let apt = regexCapture(#"(?:^|;)\s*apt=(\d+)"#, in: pieces[1]) {
                        rtxAptByPayload[pieces[0]] = apt
                    }
                }
            }
        }
        var preferred = codecByPayload.filter { $0.value == target }.map(\.key)
        guard !preferred.isEmpty else { return sdp }
        if target == "H265" {
            preferred.sort {
                h265ProfilePriority(fmtpByPayload[$0], preferTenBit: preferTenBit) <
                    h265ProfilePriority(fmtpByPayload[$1], preferTenBit: preferTenBit)
            }
        }
        var allowed = Set(preferred)
        for (rtx, apt) in rtxAptByPayload where preferred.contains(apt) && codecByPayload[rtx] == "RTX" {
            allowed.insert(rtx)
        }
        var output: [String] = []
        inVideo = false
        for line in lines {
            if line.hasPrefix("m=video") {
                inVideo = true
                let parts = line.split(whereSeparator: { $0 == " " || $0 == "\t" }).map(String.init)
                let ordered = preferred + parts.dropFirst(3).filter { allowed.contains($0) && !preferred.contains($0) }
                output.append(ordered.isEmpty ? line : (parts.prefix(3) + ordered).joined(separator: " "))
                continue
            }
            if line.hasPrefix("m="), inVideo {
                inVideo = false
            }
            if inVideo && (line.hasPrefix("a=rtpmap:") || line.hasPrefix("a=fmtp:") || line.hasPrefix("a=rtcp-fb:")) {
                let payload = line.components(separatedBy: ":").dropFirst().joined(separator: ":").split(separator: " ").first.map(String.init) ?? ""
                if !allowed.contains(payload) {
                    continue
                }
            }
            output.append(line)
        }
        return output.joined(separator: lineEnding)
    }

    static func rewriteH265TierFlag(_ sdp: String, tierFlag: Int) -> RewriteResult {
        let payloads = h265PayloadTypes(sdp)
        guard !payloads.isEmpty else { return RewriteResult(sdp: sdp, replacements: 0) }
        let lineEnding = sdp.contains("\r\n") ? "\r\n" : "\n"
        var replacements = 0
        let output = sdp.components(separatedBy: CharacterSet.newlines).map { line -> String in
            guard line.hasPrefix("a=fmtp:") else { return line }
            let payload = line.components(separatedBy: ":").dropFirst().joined(separator: ":")
                .split(separator: " ")
                .first
                .map(String.init) ?? ""
            guard payloads.contains(payload) else { return line }
            let rewritten = line.replacingOccurrences(
                of: #"tier-flag=1"#,
                with: "tier-flag=\(tierFlag)",
                options: [.regularExpression, .caseInsensitive]
            )
            if rewritten != line {
                replacements += 1
            }
            return rewritten
        }
        return RewriteResult(sdp: output.joined(separator: lineEnding), replacements: replacements)
    }

    static func rewriteH265LevelIdByProfile(_ sdp: String, maxLevelByProfile: [Int: Int]) -> RewriteResult {
        let payloads = h265PayloadTypes(sdp)
        guard !payloads.isEmpty, !maxLevelByProfile.isEmpty else {
            return RewriteResult(sdp: sdp, replacements: 0)
        }
        let lineEnding = sdp.contains("\r\n") ? "\r\n" : "\n"
        var replacements = 0
        let output = sdp.components(separatedBy: CharacterSet.newlines).map { line -> String in
            guard line.hasPrefix("a=fmtp:") else { return line }
            let rest = line.components(separatedBy: ":").dropFirst().joined(separator: ":")
            let pieces = rest.split(maxSplits: 1, whereSeparator: { $0 == " " || $0 == "\t" }).map(String.init)
            guard pieces.count == 2, payloads.contains(pieces[0]) else { return line }
            let params = pieces[1]
            guard let profile = regexCapture(#"(?:^|;)\s*profile-id=(\d+)"#, in: params).flatMap(Int.init),
                  let level = regexCapture(#"(?:^|;)\s*level-id=(\d+)"#, in: params).flatMap(Int.init),
                  let maxLevel = maxLevelByProfile[profile],
                  level > maxLevel else { return line }
            let rewritten = line.replacingOccurrences(
                of: #"level-id=\d+"#,
                with: "level-id=\(maxLevel)",
                options: [.regularExpression, .caseInsensitive]
            )
            if rewritten != line {
                replacements += 1
            }
            return rewritten
        }
        return RewriteResult(sdp: output.joined(separator: lineEnding), replacements: replacements)
    }

    static func parseInputProtocolVersion(_ sdp: String) -> Int {
        regexCapture(#"a=ri\.version:(\d+)"#, in: sdp).flatMap(Int.init) ?? 2
    }

    static func parsePartialReliableThresholdMs(_ sdp: String) -> Int {
        let value = regexCapture(#"a=ri\.partialReliableThresholdMs:(\d+)"#, in: sdp).flatMap(Int.init) ?? 30
        return max(1, min(5000, value))
    }

    static func parsePartiallyReliableGamepadMask(_ sdp: String) -> Int {
        parseRiIntegerAttribute(sdp, "ri.enablePartiallyReliableTransferGamepad", fallback: 0x0f)
    }

    static func negotiatesCodec(_ sdp: String, codec: String) -> Bool {
        let target = normalizeCodec(codec)
        var inVideo = false
        for line in sdp.components(separatedBy: CharacterSet.newlines) {
            if line.hasPrefix("m=video") {
                inVideo = true
                continue
            }
            if line.hasPrefix("m="), inVideo {
                break
            }
            guard inVideo, line.hasPrefix("a=rtpmap:") else { continue }
            let rest = String(line.dropFirst("a=rtpmap:".count))
            let parts = rest.split(maxSplits: 1, whereSeparator: { $0 == " " || $0 == "\t" }).map(String.init)
            guard parts.count == 2 else { continue }
            if normalizeCodec(String(parts[1].prefix { $0 != "/" })) == target {
                return true
            }
        }
        return false
    }

    static func mungeAnswerSdp(_ sdp: String, maxBitrateKbps: Int) -> String {
        let lineEnding = sdp.contains("\r\n") ? "\r\n" : "\n"
        let lines = sdp.components(separatedBy: CharacterSet.newlines)
        var output: [String] = []
        for (index, line) in lines.enumerated() {
            if line.hasPrefix("a=fmtp:"), line.contains("minptime="), !line.contains("stereo=1") {
                output.append("\(line);stereo=1")
            } else {
                output.append(line)
            }
            if (line.hasPrefix("m=video") || line.hasPrefix("m=audio"))
                && !(lines[safe: index + 1]?.hasPrefix("b=") == true) {
                output.append(line.hasPrefix("m=video") ? "b=AS:\(maxBitrateKbps)" : "b=AS:128")
            }
        }
        return output.joined(separator: lineEnding)
    }

    static func buildNvstSdp(
        localAnswer: String,
        offerSdp: String,
        settings: AppSettings,
        profile: StreamVideoProfile,
        codec: String
    ) -> String {
        let credentials = extractIceCredentials(localAnswer)
        let threshold = parsePartialReliableThresholdMs(offerSdp)
        let maxBitrate = max(5000, profile.maxBitrateKbps)
        let minBitrate = max(5000, Int(Double(maxBitrate) * 0.35))
        let initialBitrate = max(minBitrate, Int(Double(maxBitrate) * 0.70))
        let bitDepth = (settings.hdrEnabled || settings.preferredColorQuality.contains("10")) ? 10 : 8
        let isHighFps = profile.fps >= 90
        let is120Fps = profile.fps == 120
        let isAv1 = normalizeCodec(codec) == "AV1"
        var lines = [
            "v=0",
            "o=SdpTest test_id_13 14 IN IPv4 127.0.0.1",
            "s=-",
            "t=0 0",
            "a=general.icePassword:\(credentials.pwd)",
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
            "a=vqos.dfc.enable:0",
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
        if isHighFps {
            lines += [
                "a=bwe.iirFilterFactor:8",
                "a=video.encoderFeatureSetting:47",
                "a=video.encoderPreset:6",
                "a=vqos.resControl.cpmRtc.badNwSkipFramesCount:600",
                "a=vqos.resControl.cpmRtc.decodeTimeThresholdMs:9",
                "a=video.fbcDynamicFpsGrabTimeoutMs:\(is120Fps ? 6 : 18)",
                "a=vqos.resControl.cpmRtc.serverResolutionUpdateCoolDownCount:\(is120Fps ? 6000 : 12000)"
            ]
        }
        if profile.fps >= 240 {
            lines += [
                "a=video.enableNextCaptureMode:1",
                "a=vqos.maxStreamFpsEstimate:240",
                "a=video.videoSplitEncodeStripsPerFrame:3",
                "a=video.updateSplitEncodeStateDynamically:1",
                "a=vqos.rtcPreemptiveIdrSettings.minBurstNackSize:65535",
                "a=vqos.rtcPreemptiveIdrSettings.minNackPacketCaptureAgeMs:65535"
            ]
        }
        if isAv1 {
            lines += [
                "a=vqos.drc.minQpHeadroom:20",
                "a=vqos.drc.lowerQpThreshold:100",
                "a=vqos.drc.upperQpThreshold:200",
                "a=vqos.drc.minAdaptiveQpThreshold:180",
                "a=vqos.drc.qpCodecThresholdAdj:0",
                "a=vqos.drc.qpMaxResThresholdAdj:20",
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
            "a=vqos.adjustStreamingFpsDuringOutOfFocus:0",
            "a=vqos.resControl.cpmRtc.ignoreOutOfFocusWindowState:1",
            "a=vqos.resControl.perfHistory.rtcIgnoreOutOfFocusWindowState:1",
            "a=vqos.resControl.cpmRtc.featureMask:0",
            "a=vqos.resControl.cpmRtc.enable:0",
            "a=vqos.resControl.cpmRtc.minResolutionPercent:100",
            "a=vqos.resControl.cpmRtc.resolutionChangeHoldonMs:999999",
            "a=packetPacing.numGroups:\(is120Fps ? 3 : 5)",
            "a=packetPacing.maxDelayUs:1000",
            "a=packetPacing.minNumPacketsFrame:10",
            "a=video.rtpNackQueueLength:1024",
            "a=video.rtpNackQueueMaxPackets:512",
            "a=video.rtpNackMaxPacketCount:25",
            "a=vqos.drc.qpMaxResThresholdAdj:4",
            "a=vqos.grc.qpMaxResThresholdAdj:4",
            "a=vqos.drc.iirFilterFactor:100",
            "a=video.clientViewportWd:\(profile.width)",
            "a=video.clientViewportHt:\(profile.height)",
            "a=video.maxFPS:\(profile.fps)",
            "a=video.initialBitrateKbps:\(initialBitrate)",
            "a=video.initialPeakBitrateKbps:\(maxBitrate)",
            "a=vqos.bw.maximumBitrateKbps:\(maxBitrate)",
            "a=vqos.bw.minimumBitrateKbps:\(minBitrate)",
            "a=vqos.bw.peakBitrateKbps:\(maxBitrate)",
            "a=vqos.bw.serverPeakBitrateKbps:\(maxBitrate)",
            "a=vqos.bw.enableBandwidthEstimation:1",
            "a=vqos.bw.disableBitrateLimit:0",
            "a=vqos.grc.maximumBitrateKbps:\(maxBitrate)",
            "a=vqos.grc.enable:0",
            "a=video.maxNumReferenceFrames:4",
            "a=video.mapRtpTimestampsToFrames:1",
            "a=video.encoderCscMode:3",
            "a=video.dynamicRangeMode:0",
            "a=video.bitDepth:\(bitDepth)",
            "a=video.scalingFeature1:\(isAv1 ? 1 : 0)",
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

    private static func offerHasCodec(_ sdp: String, codec: String) -> Bool {
        let target = normalizeCodec(codec)
        var inVideo = false
        for line in sdp.components(separatedBy: CharacterSet.newlines) {
            if line.hasPrefix("m=video") {
                inVideo = true
                continue
            }
            if line.hasPrefix("m="), inVideo {
                break
            }
            guard inVideo, line.hasPrefix("a=rtpmap:") else { continue }
            let rest = String(line.dropFirst("a=rtpmap:".count))
            let parts = rest.split(separator: " ", maxSplits: 1)
            guard parts.count == 2 else { continue }
            if normalizeCodec(String(parts[1].prefix { $0 != "/" })) == target {
                return true
            }
        }
        return false
    }

    private static func h265PayloadTypes(_ sdp: String) -> Set<String> {
        var inVideo = false
        var payloads = Set<String>()
        for line in sdp.components(separatedBy: CharacterSet.newlines) {
            if line.hasPrefix("m=video") {
                inVideo = true
                continue
            }
            if line.hasPrefix("m="), inVideo {
                break
            }
            guard inVideo, line.hasPrefix("a=rtpmap:") else { continue }
            let rest = String(line.dropFirst("a=rtpmap:".count))
            let parts = rest.split(maxSplits: 1, whereSeparator: { $0 == " " || $0 == "\t" }).map(String.init)
            guard parts.count == 2 else { continue }
            if normalizeCodec(String(parts[1].prefix { $0 != "/" })) == "H265" {
                payloads.insert(parts[0])
            }
        }
        return payloads
    }

    private static func h265ProfilePriority(_ fmtp: String?, preferTenBit: Bool) -> Int {
        let profileId = regexCapture(#"(?:^|;)\s*profile-id=(\d+)"#, in: fmtp ?? "")
        if preferTenBit {
            switch profileId {
            case "2": return 0
            case "1": return 1
            default: return 2
            }
        }
        switch profileId {
        case "1": return 0
        case nil: return 1
        case "2": return 2
        default: return 3
        }
    }

    private static func extractIceCredentials(_ sdp: String) -> (ufrag: String, pwd: String, fingerprint: String) {
        (
            regexCapture(#"a=ice-ufrag:([^\r\n]+)"#, in: sdp) ?? "",
            regexCapture(#"a=ice-pwd:([^\r\n]+)"#, in: sdp) ?? "",
            regexCapture(#"a=fingerprint:sha-256 ([^\r\n]+)"#, in: sdp) ?? ""
        )
    }

    private static func extractPublicIp(_ hostOrIp: String) -> String? {
        var value = hostOrIp.trimmingCharacters(in: .whitespacesAndNewlines)
        if let url = URL(string: value), let host = url.host {
            value = host
        }
        if value.hasPrefix("["),
           let end = value.firstIndex(of: "]") {
            value = String(value[value.index(after: value.startIndex)..<end])
        } else if value.filter({ $0 == ":" }).count == 1,
                  let colon = value.lastIndex(of: ":") {
            value = String(value[..<colon])
        }
        if value.range(of: #"^\d{1,3}(\.\d{1,3}){3}$"#, options: .regularExpression) != nil {
            return value
        }
        let parts = value.components(separatedBy: ".").first?.components(separatedBy: "-") ?? []
        if parts.count == 4, parts.allSatisfy({ Int($0) != nil }) {
            return parts.joined(separator: ".")
        }
        return nil
    }

    private static func parseRiIntegerAttribute(_ sdp: String, _ attribute: String, fallback: Int) -> Int {
        guard let raw = regexCapture("a=\(NSRegularExpression.escapedPattern(for: attribute)):([^\\r\\n]+)", in: sdp)?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        else { return fallback }
        if raw.lowercased().hasPrefix("0x") {
            return Int(raw.dropFirst(2), radix: 16) ?? fallback
        }
        return Int(raw) ?? fallback
    }

    static func normalizeCodec(_ value: String) -> String {
        let upper = value.trimmingCharacters(in: .whitespacesAndNewlines).uppercased()
        switch upper {
        case "HEVC", "H.265":
            return "H265"
        case "AV01":
            return "AV1"
        case "AVC", "H.264":
            return "H264"
        default:
            return upper
        }
    }

    private static func regexCapture(_ pattern: String, in text: String) -> String? {
        guard let regex = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]) else { return nil }
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        guard let match = regex.firstMatch(in: text, range: range),
              match.numberOfRanges > 1,
              let capture = Range(match.range(at: 1), in: text)
        else { return nil }
        return String(text[capture])
    }
}

private final class OpenNOWNativeInputEncoder {
    var protocolVersion = 3
    private var gamepadSequences: [Int: Int] = [:]

    func resetGamepadSequences() {
        gamepadSequences.removeAll()
    }

    func encodeHeartbeat() -> Data {
        var data = Data(count: 4)
        data.writeUInt32LE(2, at: 0)
        return data
    }

    func encodeKey(keycode: Int, scancode: Int, modifiers: Int, pressed: Bool) -> Data {
        var data = Data(count: 18)
        data.writeUInt32LE(pressed ? 3 : 4, at: 0)
        data.writeUInt16BE(UInt16(clamping: keycode), at: 4)
        data.writeUInt16BE(UInt16(clamping: modifiers), at: 6)
        data.writeUInt16BE(UInt16(clamping: scancode), at: 8)
        data.writeUInt64BE(Self.timestampUs(), at: 10)
        return wrapSingle(data)
    }

    func encodeMouseMove(dx: Int, dy: Int) -> Data {
        var data = Data(count: 22)
        data.writeUInt32LE(7, at: 0)
        data.writeInt16BE(Int16(clamping: dx), at: 4)
        data.writeInt16BE(Int16(clamping: dy), at: 6)
        data.writeInt16BE(0, at: 8)
        data.writeUInt32BE(0, at: 10)
        data.writeUInt64BE(Self.timestampUs(), at: 14)
        return wrapStreamEvent(data)
    }

    func encodeMouseButton(button: Int, pressed: Bool) -> Data {
        var data = Data(count: 18)
        data.writeUInt32LE(pressed ? 8 : 9, at: 0)
        data[4] = UInt8(clamping: button.clamped(to: 1...5))
        data.writeUInt32BE(0, at: 6)
        data.writeUInt64BE(Self.timestampUs(), at: 10)
        return wrapSingle(data)
    }

    func encodeMouseWheel(delta: Int) -> Data {
        var data = Data(count: 22)
        data.writeUInt32LE(10, at: 0)
        data.writeInt16BE(0, at: 4)
        data.writeInt16BE(Int16(clamping: delta), at: 6)
        data.writeInt16BE(0, at: 8)
        data.writeUInt32BE(0, at: 10)
        data.writeUInt64BE(Self.timestampUs(), at: 14)
        return wrapSingle(data)
    }

    func encodeHapticsEnabled(_ enabled: Bool) -> Data {
        var data = Data(count: 6)
        data.writeUInt32LE(13, at: 0)
        data.writeUInt16BE(enabled ? 1 : 0, at: 4)
        return wrapSingle(data)
    }

    func encodeGamepadState(_ state: OpenNOWNativeGamepadState, partiallyReliable: Bool) -> Data {
        var data = Data(count: 38)
        data.writeUInt32LE(12, at: 0)
        data.writeUInt16LE(26, at: 4)
        data.writeUInt16LE(UInt16(state.controllerId & 0x03), at: 6)
        data.writeUInt16LE(UInt16(state.connectedBitmap & 0xffff), at: 8)
        data.writeUInt16LE(20, at: 10)
        data.writeUInt16LE(UInt16(state.buttons & 0xffff), at: 12)
        data.writeUInt16LE(UInt16((state.leftTrigger & 0xff) | ((state.rightTrigger & 0xff) << 8)), at: 14)
        data.writeInt16LE(Int16(clamping: state.leftStickX), at: 16)
        data.writeInt16LE(Int16(clamping: state.leftStickY), at: 18)
        data.writeInt16LE(Int16(clamping: state.rightStickX), at: 20)
        data.writeInt16LE(Int16(clamping: state.rightStickY), at: 22)
        data.writeUInt16LE(0, at: 24)
        data.writeUInt16LE(85, at: 26)
        data.writeUInt16LE(0, at: 28)
        data.writeUInt64LE(Self.timestampUs(), at: 30)
        return partiallyReliable ? wrapGamepadPartiallyReliable(data, index: state.controllerId) : wrapStreamEvent(data)
    }

    private func wrapSingle(_ payload: Data) -> Data {
        guard protocolVersion > 2 else { return payload }
        var data = Data(count: 10)
        data[0] = 0x23
        data.writeUInt64BE(Self.timestampUs(), at: 1)
        data[9] = 0x22
        data.append(payload)
        return data
    }

    private func wrapStreamEvent(_ payload: Data) -> Data {
        guard protocolVersion > 2 else { return payload }
        var data = Data(count: 12)
        data[0] = 0x23
        data.writeUInt64BE(Self.timestampUs(), at: 1)
        data[9] = 0x21
        data.writeUInt16BE(UInt16(payload.count), at: 10)
        data.append(payload)
        return data
    }

    private func wrapGamepadPartiallyReliable(_ payload: Data, index: Int) -> Data {
        guard protocolVersion > 2 else { return payload }
        let sequence = gamepadSequences[index] ?? 1
        gamepadSequences[index] = (sequence + 1) & 0xffff
        var data = Data(count: 16)
        data[0] = 0x23
        data.writeUInt64BE(Self.timestampUs(), at: 1)
        data[9] = 0x26
        data[10] = UInt8(index & 0xff)
        data.writeUInt16BE(UInt16(sequence & 0xffff), at: 11)
        data[13] = 0x21
        data.writeUInt16BE(UInt16(payload.count), at: 14)
        data.append(payload)
        return data
    }

    private static func timestampUs() -> UInt64 {
        UInt64(ProcessInfo.processInfo.systemUptime * 1_000_000)
    }
}

private final class OpenNOWNativeInputBridge {
    private weak var sink: OpenNOWNativeInputSink?
    private var observers: [NSObjectProtocol] = []
    private var keyboard: GCKeyboard?
    private var mice: [GCMouse] = []
    private var controllers: [GCController] = []
    private var activeControllerIndex = 0
    private var publishScheduled = false
    private var pendingMouseDX = 0.0
    private var pendingMouseDY = 0.0
    private var mouseMoveScheduled = false

    func attach(to sink: OpenNOWNativeInputSink) {
        self.sink = sink
        if observers.isEmpty {
            startMonitoring()
        }
        attachCurrentDevices()
    }

    func detach() {
        observers.forEach(NotificationCenter.default.removeObserver)
        observers.removeAll()
        clearHandlers()
        sink = nil
    }

    func updateHapticsAvailability() {
        sink?.setHapticsAvailable(controllers.contains { $0.haptics != nil })
    }

    private func startMonitoring() {
        let center = NotificationCenter.default
        let names: [Notification.Name] = [
            .GCKeyboardDidConnect,
            .GCKeyboardDidDisconnect,
            .GCMouseDidConnect,
            .GCMouseDidDisconnect,
            .GCControllerDidConnect,
            .GCControllerDidDisconnect
        ]
        observers = names.map { name in
            center.addObserver(forName: name, object: nil, queue: .main) { [weak self] _ in
                self?.attachCurrentDevices()
            }
        }
    }

    private func attachCurrentDevices() {
        attachKeyboard()
        attachMice()
        attachControllers()
    }

    private func clearHandlers() {
        keyboard?.keyboardInput?.keyChangedHandler = nil
        keyboard = nil
        for mouse in mice {
            let input = mouse.mouseInput
            input?.mouseMovedHandler = nil
            input?.leftButton.pressedChangedHandler = nil
            input?.middleButton?.pressedChangedHandler = nil
            input?.rightButton?.pressedChangedHandler = nil
            input?.auxiliaryButtons?.forEach { $0.pressedChangedHandler = nil }
            input?.scroll.valueChangedHandler = nil
        }
        mice = []
        for controller in controllers {
            controller.extendedGamepad?.valueChangedHandler = nil
        }
        controllers = []
    }

    private func attachKeyboard() {
        let next = GCKeyboard.coalesced
        guard next !== keyboard else { return }
        keyboard?.keyboardInput?.keyChangedHandler = nil
        keyboard = next
        keyboard?.keyboardInput?.keyChangedHandler = { [weak self] keyboardInput, _, keyCode, pressed in
            guard let self, let mapping = Self.keyMap[keyCode] else { return }
            self.sink?.sendKey(
                keycode: mapping.keycode,
                scancode: mapping.scancode,
                modifiers: self.modifiers(for: keyboardInput, changedKey: keyCode, pressed: pressed),
                pressed: pressed
            )
        }
    }

    private func attachMice() {
        let next = GCMouse.mice()
        let changed = next.count != mice.count || zip(next, mice).contains { $0 !== $1 }
        guard changed else { return }
        for mouse in mice {
            mouse.mouseInput?.mouseMovedHandler = nil
        }
        mice = next
        for mouse in mice {
            guard let input = mouse.mouseInput else { continue }
            input.mouseMovedHandler = { [weak self] _, dx, dy in
                self?.queueMouseMove(dx: Double(dx), dy: -Double(dy))
            }
            wireMouseButton(input.leftButton, button: 1)
            wireMouseButton(input.middleButton, button: 2)
            wireMouseButton(input.rightButton, button: 3)
            input.auxiliaryButtons?.enumerated().forEach { index, button in
                wireMouseButton(button, button: min(5, index + 4))
            }
            input.scroll.valueChangedHandler = { [weak self] _, _, value in
                self?.sink?.sendMouseWheel(delta: Int((value * 120).rounded()))
            }
        }
    }

    private func wireMouseButton(_ button: GCControllerButtonInput?, button number: Int) {
        button?.pressedChangedHandler = { [weak self] _, _, pressed in
            self?.sink?.sendMouseButton(button: number, pressed: pressed)
        }
    }

    private func queueMouseMove(dx: Double, dy: Double) {
        guard dx.isFinite, dy.isFinite else { return }
        pendingMouseDX += dx
        pendingMouseDY += dy
        guard !mouseMoveScheduled else { return }
        mouseMoveScheduled = true
        DispatchQueue.main.async { [weak self] in
            self?.flushMouseMove()
        }
    }

    private func flushMouseMove() {
        mouseMoveScheduled = false
        let dx = Int(pendingMouseDX.rounded()).clamped(to: -32768...32767)
        let dy = Int(pendingMouseDY.rounded()).clamped(to: -32768...32767)
        pendingMouseDX = 0
        pendingMouseDY = 0
        guard dx != 0 || dy != 0 else { return }
        sink?.sendMouseMove(dx: dx, dy: dy, partiallyReliable: true)
    }

    private func attachControllers() {
        let next = GCController.controllers().filter { $0.extendedGamepad != nil }
        let changed = next.count != controllers.count || zip(next, controllers).contains { $0 !== $1 }
        guard changed else { return }
        for controller in controllers {
            controller.extendedGamepad?.valueChangedHandler = nil
        }
        controllers = next
        activeControllerIndex = min(activeControllerIndex, max(0, controllers.count - 1))
        for (index, controller) in controllers.enumerated() {
            controller.extendedGamepad?.valueChangedHandler = { [weak self] _, _ in
                self?.activeControllerIndex = index
                self?.scheduleControllerPublish()
            }
        }
        updateHapticsAvailability()
        publishControllerState()
    }

    private func scheduleControllerPublish() {
        guard !publishScheduled else { return }
        publishScheduled = true
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.01) { [weak self] in
            self?.publishScheduled = false
            self?.publishControllerState()
        }
    }

    private func publishControllerState() {
        guard let controller = controllers[safe: activeControllerIndex],
              let gamepad = controller.extendedGamepad else {
            sink?.sendGamepadState(OpenNOWNativeGamepadState(
                controllerId: 0,
                buttons: 0,
                leftTrigger: 0,
                rightTrigger: 0,
                leftStickX: 0,
                leftStickY: 0,
                rightStickX: 0,
                rightStickY: 0,
                connectedBitmap: 0
            ))
            return
        }
        let controllerId = activeControllerIndex & 0x03
        sink?.sendGamepadState(OpenNOWNativeGamepadState(
            controllerId: controllerId,
            buttons: buttonMask(gamepad),
            leftTrigger: normalizeTrigger(gamepad.leftTrigger.value),
            rightTrigger: normalizeTrigger(gamepad.rightTrigger.value),
            leftStickX: normalizeAxis(gamepad.leftThumbstick.xAxis.value),
            leftStickY: normalizeAxis(-gamepad.leftThumbstick.yAxis.value),
            rightStickX: normalizeAxis(gamepad.rightThumbstick.xAxis.value),
            rightStickY: normalizeAxis(-gamepad.rightThumbstick.yAxis.value),
            connectedBitmap: (1 << controllerId) | (1 << (controllerId + 8))
        ))
    }

    private func buttonMask(_ gamepad: GCExtendedGamepad) -> Int {
        var mask = 0
        if gamepad.dpad.up.isPressed { mask |= 0x0001 }
        if gamepad.dpad.down.isPressed { mask |= 0x0002 }
        if gamepad.dpad.left.isPressed { mask |= 0x0004 }
        if gamepad.dpad.right.isPressed { mask |= 0x0008 }
        if gamepad.buttonOptions?.isPressed == true { mask |= 0x0010 }
        if gamepad.buttonMenu.isPressed { mask |= 0x0020 }
        if gamepad.leftThumbstickButton?.isPressed == true { mask |= 0x0040 }
        if gamepad.rightThumbstickButton?.isPressed == true { mask |= 0x0080 }
        if gamepad.leftShoulder.isPressed { mask |= 0x0100 }
        if gamepad.rightShoulder.isPressed { mask |= 0x0200 }
        if gamepad.buttonHome?.isPressed == true { mask |= 0x0400 }
        if gamepad.buttonA.isPressed { mask |= 0x1000 }
        if gamepad.buttonB.isPressed { mask |= 0x2000 }
        if gamepad.buttonX.isPressed { mask |= 0x4000 }
        if gamepad.buttonY.isPressed { mask |= 0x8000 }
        return mask
    }

    private func normalizeAxis(_ value: Float) -> Int {
        Int((max(-1, min(1, value)) * 32767).rounded()).clamped(to: -32768...32767)
    }

    private func normalizeTrigger(_ value: Float) -> Int {
        Int((max(0, min(1, value)) * 255).rounded()).clamped(to: 0...255)
    }

    private func modifiers(for keyboardInput: GCKeyboardInput, changedKey: GCKeyCode, pressed: Bool) -> Int {
        var value = 0
        if isPressed(.leftShift, or: .rightShift, in: keyboardInput, changedKey: changedKey, pressed: pressed) { value |= 0x01 }
        if isPressed(.leftControl, or: .rightControl, in: keyboardInput, changedKey: changedKey, pressed: pressed) { value |= 0x02 }
        if isPressed(.leftAlt, or: .rightAlt, in: keyboardInput, changedKey: changedKey, pressed: pressed) { value |= 0x04 }
        if isPressed(.leftGUI, or: .rightGUI, in: keyboardInput, changedKey: changedKey, pressed: pressed) { value |= 0x08 }
        if isKeyPressed(.capsLock, in: keyboardInput, changedKey: changedKey, pressed: pressed) { value |= 0x10 }
        if isKeyPressed(.keypadNumLock, in: keyboardInput, changedKey: changedKey, pressed: pressed) { value |= 0x20 }
        return value
    }

    private func isPressed(_ lhs: GCKeyCode, or rhs: GCKeyCode, in keyboardInput: GCKeyboardInput, changedKey: GCKeyCode, pressed: Bool) -> Bool {
        isKeyPressed(lhs, in: keyboardInput, changedKey: changedKey, pressed: pressed)
            || isKeyPressed(rhs, in: keyboardInput, changedKey: changedKey, pressed: pressed)
    }

    private func isKeyPressed(_ keyCode: GCKeyCode, in keyboardInput: GCKeyboardInput, changedKey: GCKeyCode, pressed: Bool) -> Bool {
        keyCode == changedKey ? pressed : keyboardInput.button(forKeyCode: keyCode)?.isPressed == true
    }

    private typealias KeyMapping = (keycode: Int, scancode: Int)
    private static let keyMap: [GCKeyCode: KeyMapping] = [
        .keyA: (0x41, 0x001e), .keyB: (0x42, 0x0030), .keyC: (0x43, 0x002e), .keyD: (0x44, 0x0020),
        .keyE: (0x45, 0x0012), .keyF: (0x46, 0x0021), .keyG: (0x47, 0x0022), .keyH: (0x48, 0x0023),
        .keyI: (0x49, 0x0017), .keyJ: (0x4a, 0x0024), .keyK: (0x4b, 0x0025), .keyL: (0x4c, 0x0026),
        .keyM: (0x4d, 0x0032), .keyN: (0x4e, 0x0031), .keyO: (0x4f, 0x0018), .keyP: (0x50, 0x0019),
        .keyQ: (0x51, 0x0010), .keyR: (0x52, 0x0013), .keyS: (0x53, 0x001f), .keyT: (0x54, 0x0014),
        .keyU: (0x55, 0x0016), .keyV: (0x56, 0x002f), .keyW: (0x57, 0x0011), .keyX: (0x58, 0x002d),
        .keyY: (0x59, 0x0015), .keyZ: (0x5a, 0x002c),
        .one: (0x31, 0x0002), .two: (0x32, 0x0003), .three: (0x33, 0x0004), .four: (0x34, 0x0005),
        .five: (0x35, 0x0006), .six: (0x36, 0x0007), .seven: (0x37, 0x0008), .eight: (0x38, 0x0009),
        .nine: (0x39, 0x000a), .zero: (0x30, 0x000b),
        .returnOrEnter: (0x0d, 0x001c), .escape: (0x1b, 0x0001), .deleteOrBackspace: (0x08, 0x000e),
        .tab: (0x09, 0x000f), .spacebar: (0x20, 0x0039), .hyphen: (0xbd, 0x000c), .equalSign: (0xbb, 0x000d),
        .openBracket: (0xdb, 0x001a), .closeBracket: (0xdd, 0x001b), .backslash: (0xdc, 0x002b),
        .semicolon: (0xba, 0x0027), .quote: (0xde, 0x0028), .graveAccentAndTilde: (0xc0, 0x0029),
        .comma: (0xbc, 0x0033), .period: (0xbe, 0x0034), .slash: (0xbf, 0x0035),
        .leftShift: (0x10, 0x002a), .rightShift: (0x10, 0x0036), .leftControl: (0x11, 0x001d),
        .rightControl: (0x11, 0x011d), .leftAlt: (0x12, 0x0038), .rightAlt: (0x12, 0x0138),
        .leftGUI: (0x5b, 0x015b), .rightGUI: (0x5c, 0x015c),
        .rightArrow: (0x27, 0x014d), .leftArrow: (0x25, 0x014b), .downArrow: (0x28, 0x0150), .upArrow: (0x26, 0x0148)
    ]
}

private struct OpenNOWNativeHapticsCommand {
    let controllerId: Int
    let weakMagnitude: Int
    let strongMagnitude: Int
}

private enum OpenNOWNativeHapticsParser {
    static func parse(_ bytes: [UInt8]) -> OpenNOWNativeHapticsCommand? {
        guard bytes.count >= 2 else { return nil }
        let firstWord = readUInt16LE(bytes, 0)
        if firstWord == 267 {
            return parseLegacy(bytes, offset: 2)
        }
        switch firstWord & 0xff {
        case 34:
            return parseSubMessage(bytes, offset: 1)
        case 32, 33, 35, 36, 37:
            return nil
        default:
            return parseLegacy(bytes, offset: 0)
        }
    }

    private static func parseSubMessage(_ bytes: [UInt8], offset: Int) -> OpenNOWNativeHapticsCommand? {
        guard let type = readInt32LE(bytes, offset) else { return nil }
        if type == 267 { return parseLegacy(bytes, offset: offset + 4) }
        if type == 17 { return parseOpenController(bytes, offset: offset + 4) }
        return nil
    }

    private static func parseLegacy(_ bytes: [UInt8], offset: Int) -> OpenNOWNativeHapticsCommand? {
        guard bytes.indices.contains(offset + 9),
              readUInt16LE(bytes, offset) == 1,
              readUInt16LE(bytes, offset + 2) >= 6
        else { return nil }
        return OpenNOWNativeHapticsCommand(
            controllerId: readUInt16LE(bytes, offset + 4),
            weakMagnitude: readUInt16LE(bytes, offset + 6),
            strongMagnitude: readUInt16LE(bytes, offset + 8)
        )
    }

    private static func parseOpenController(_ bytes: [UInt8], offset: Int) -> OpenNOWNativeHapticsCommand? {
        guard bytes.indices.contains(offset + 8) else { return nil }
        let controllerByte = Int(bytes[offset])
        guard controllerByte >= 6, controllerByte < 10 else { return nil }
        guard bytes[offset + 3] == 5, (bytes[offset + 4] & 0xfe) == 0 else { return nil }
        return OpenNOWNativeHapticsCommand(
            controllerId: controllerByte - 6,
            weakMagnitude: Int(bytes[offset + 7]) << 8,
            strongMagnitude: Int(bytes[offset + 8]) << 8
        )
    }

    private static func readUInt16LE(_ bytes: [UInt8], _ offset: Int) -> Int {
        guard bytes.indices.contains(offset + 1) else { return 0 }
        return Int(bytes[offset]) | (Int(bytes[offset + 1]) << 8)
    }

    private static func readInt32LE(_ bytes: [UInt8], _ offset: Int) -> Int? {
        guard bytes.indices.contains(offset + 3) else { return nil }
        let raw = Int32(bitPattern:
            UInt32(bytes[offset])
            | (UInt32(bytes[offset + 1]) << 8)
            | (UInt32(bytes[offset + 2]) << 16)
            | (UInt32(bytes[offset + 3]) << 24)
        )
        return Int(raw)
    }
}

private final class OpenNOWNativeControllerHaptics {
    static let shared = OpenNOWNativeControllerHaptics()
    private var engines: [ObjectIdentifier: CHHapticEngine] = [:]
    private var lastRumbleEffectAt: [TimeInterval] = Array(repeating: 0, count: 4)
    private let rumbleDuration: TimeInterval = 0.09
    private let throttleDuration: TimeInterval = 0.035

    func apply(_ command: OpenNOWNativeHapticsCommand) {
        let slot = command.controllerId.clamped(to: 0...3)
        let weak = Double(command.weakMagnitude.clamped(to: 0...65535)) / 65535.0
        let strong = Double(command.strongMagnitude.clamped(to: 0...65535)) / 65535.0
        let combined = min(1, max(0, strong * 0.78 + weak * 0.48))
        guard let controller = findController(for: slot) else { return }
        let now = ProcessInfo.processInfo.systemUptime
        if combined > 0, lastRumbleEffectAt[slot] > 0, now - lastRumbleEffectAt[slot] <= throttleDuration {
            return
        }
        lastRumbleEffectAt[slot] = combined <= 0 ? 0 : now
        if combined <= 0 {
            stopRumble(for: controller)
            return
        }
        playRumble(on: controller, intensity: Float(combined), sharpness: Float(strong >= weak ? 0.62 : 0.35))
    }

    private func findController(for slot: Int) -> GCController? {
        let all = GCController.controllers()
        if all.indices.contains(slot), all[slot].haptics != nil {
            return all[slot]
        }
        let haptic = all.filter { $0.haptics != nil }
        if haptic.indices.contains(slot) { return haptic[slot] }
        return haptic.count == 1 ? haptic[0] : nil
    }

    private func playRumble(on controller: GCController, intensity: Float, sharpness: Float) {
        do {
            let engine = try hapticEngine(for: controller)
            try engine.start()
            let event = CHHapticEvent(
                eventType: .hapticContinuous,
                parameters: [
                    CHHapticEventParameter(parameterID: .hapticIntensity, value: intensity),
                    CHHapticEventParameter(parameterID: .hapticSharpness, value: sharpness)
                ],
                relativeTime: 0,
                duration: rumbleDuration
            )
            let pattern = try CHHapticPattern(events: [event], parameters: [])
            let player = try engine.makePlayer(with: pattern)
            try player.start(atTime: CHHapticTimeImmediate)
        } catch {
            stopRumble(for: controller)
        }
    }

    private func hapticEngine(for controller: GCController) throws -> CHHapticEngine {
        let key = ObjectIdentifier(controller)
        if let engine = engines[key] { return engine }
        guard let haptics = controller.haptics else { throw CocoaError(.featureUnsupported) }
        let locality: GCHapticsLocality = haptics.supportedLocalities.contains(.all) ? .all : .default
        guard let engine = haptics.createEngine(withLocality: locality) else { throw CocoaError(.featureUnsupported) }
        engine.isAutoShutdownEnabled = true
        engine.stoppedHandler = { [weak self, weak controller] _ in
            guard let controller else { return }
            self?.engines.removeValue(forKey: ObjectIdentifier(controller))
        }
        engines[key] = engine
        return engine
    }

    private func stopRumble(for controller: GCController) {
        let key = ObjectIdentifier(controller)
        engines[key]?.stop(completionHandler: nil)
        engines.removeValue(forKey: key)
    }
}

private extension Data {
    mutating func writeUInt16LE(_ value: UInt16, at offset: Int) {
        self[offset] = UInt8(value & 0xff)
        self[offset + 1] = UInt8((value >> 8) & 0xff)
    }

    mutating func writeUInt16BE(_ value: UInt16, at offset: Int) {
        self[offset] = UInt8((value >> 8) & 0xff)
        self[offset + 1] = UInt8(value & 0xff)
    }

    mutating func writeInt16LE(_ value: Int16, at offset: Int) {
        writeUInt16LE(UInt16(bitPattern: value), at: offset)
    }

    mutating func writeInt16BE(_ value: Int16, at offset: Int) {
        writeUInt16BE(UInt16(bitPattern: value), at: offset)
    }

    mutating func writeUInt32LE(_ value: UInt32, at offset: Int) {
        self[offset] = UInt8(value & 0xff)
        self[offset + 1] = UInt8((value >> 8) & 0xff)
        self[offset + 2] = UInt8((value >> 16) & 0xff)
        self[offset + 3] = UInt8((value >> 24) & 0xff)
    }

    mutating func writeUInt32BE(_ value: UInt32, at offset: Int) {
        self[offset] = UInt8((value >> 24) & 0xff)
        self[offset + 1] = UInt8((value >> 16) & 0xff)
        self[offset + 2] = UInt8((value >> 8) & 0xff)
        self[offset + 3] = UInt8(value & 0xff)
    }

    mutating func writeUInt64LE(_ value: UInt64, at offset: Int) {
        for index in 0..<8 {
            self[offset + index] = UInt8((value >> UInt64(index * 8)) & 0xff)
        }
    }

    mutating func writeUInt64BE(_ value: UInt64, at offset: Int) {
        for index in 0..<8 {
            self[offset + index] = UInt8((value >> UInt64((7 - index) * 8)) & 0xff)
        }
    }
}

private extension Collection {
    subscript(safe index: Index) -> Element? {
        indices.contains(index) ? self[index] : nil
    }
}

private extension Comparable {
    func clamped(to range: ClosedRange<Self>) -> Self {
        min(max(self, range.lowerBound), range.upperBound)
    }
}
#endif
#endif
