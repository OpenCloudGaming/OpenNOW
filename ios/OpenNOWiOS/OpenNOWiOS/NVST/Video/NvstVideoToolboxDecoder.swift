import CoreMedia
import CoreVideo
import Foundation
import VideoToolbox

/// Hardware decode for the Bifrost-free NVST video path: Annex-B access units in,
/// `CVPixelBuffer` out.
///
/// The session is (re)built whenever the stream's parameter sets change, which is what a
/// resolution or codec-profile switch looks like on the wire. Parameter sets are cached because
/// only keyframes carry them inline.
@available(iOS 17.0, *)
public final class NvstVideoToolboxDecoder: @unchecked Sendable {
    public enum DecoderError: LocalizedError, Equatable, Sendable {
        case hardwareRequired
        case requested444NotDelivered(String)
        case unsupported444Hardware(OSStatus)
        case output444NotPreserved
        case missingParameterSets
        case formatDescriptionFailed(OSStatus)
        case sessionCreationFailed(OSStatus)
        case blockBufferFailed(OSStatus)
        case sampleBufferFailed(OSStatus)
        case decodeFailed(OSStatus)

        public var errorDescription: String? {
            switch self {
            case .hardwareRequired: "Native NVST requires a hardware video decoder on this device."
            case .requested444NotDelivered(let actual): "Requested 10-bit 4:4:4, but the host delivered \(actual). Select 10-bit 4:2:0 to continue."
            case .unsupported444Hardware(let status): "This device could not create a hardware 10-bit 4:4:4 decoder (OSStatus \(status)). Select 10-bit 4:2:0 to continue."
            case .output444NotPreserved: "The decoder did not preserve a 10-bit 4:4:4 output surface. Select 10-bit 4:2:0 to continue."
            case .missingParameterSets: "NVST video stream has not delivered a keyframe with parameter sets yet."
            case .formatDescriptionFailed(let status): "NVST decoder could not build a format description (OSStatus \(status))."
            case .sessionCreationFailed(let status): "NVST decoder could not create a decompression session (OSStatus \(status))."
            case .blockBufferFailed(let status): "NVST decoder could not wrap the access unit (OSStatus \(status))."
            case .sampleBufferFailed(let status): "NVST decoder could not build a sample buffer (OSStatus \(status))."
            case .decodeFailed(let status): "NVST decoder rejected a frame (OSStatus \(status))."
            }
        }
    }

    /// RTP media clock for NVST video.
    public static let clockRate: Int32 = 90_000

    /// Guards the session/format state. Never held across a VideoToolbox call: the output
    /// handler runs on a decode thread and takes `statsLock`, so holding one lock across both
    /// would deadlock against `VTDecompressionSessionWaitForAsynchronousFrames`.
    private let stateLock = NSLock()
    let statsLock = NSLock()
    let codec: NVSTVideoCodec
    let requiresTenBit444: Bool
    private var parameterSets = NvstElementaryStream.ParameterSets()
    private var formatDescription: CMVideoFormatDescription?
    var session: VTDecompressionSession?
    private var decodedFrames: UInt64 = 0
    private var failedFrames: UInt64 = 0
    private var firstFailureStatus: OSStatus = noErr
    private var lastFailureStatus: OSStatus = noErr
    private var loggedFailures = 0
    private var loggedAccepted = 0
    static let maxLoggedAccepted = 6
    static let maxLoggedFailures = 12

    /// Set by the transport so a rejected access unit can describe itself, and so the frame the
    /// decoder could not use can be named back to the seat.
    /// Builds the decompression session from parameter sets ahead of the stream's first keyframe.
    /// Kept for the first-keyframe gate's test; the transport no longer calls it (see
    /// `NvstBifrostFreeTransport.startVideo` for the measured reason).
    public func prewarm(parameterSets sets: NvstElementaryStream.ParameterSets) {
        guard sets.isComplete(for: codec) else { return }
        _ = try? prepareSession(for: sets)
    }

    /// Whether a keyframe has been submitted yet; see the gate at the top of `decode`.
    private var hasSeenKeyframe = false
    /// One-shot so the first AV1 keyframe's wire bytes are logged exactly once per session.
    private var loggedFirstAv1Keyframe = false

    public var onDecodeFailure: (@Sendable (UInt32, String) -> Void)?

    /// `bytes=N nals=[type:length, …]` (AV1: `obus=[…]`), so a rejected access unit can be
    /// compared with an accepted one without a packet capture.
    static func accessUnitShape(_ bytes: Data, codec: NVSTVideoCodec) -> String {
        guard codec != .av1 else { return NvstAv1Obu.accessUnitShape(bytes) }
        let buffer = [UInt8](bytes)
        let units = NvstAnnexB.nalUnits(bytes)
        let described = units.prefix(12).map { unit -> String in
            guard unit.offset < buffer.count else { return "?" }
            let header = buffer[unit.offset]
            let type: Int = switch codec {
            case .h264: Int(header & 0x1f)
            case .hevc: Int((header >> 1) & 0x3f)
            case .av1: Int(header)
            }
            let head = buffer[unit.offset..<min(unit.offset + 4, buffer.count)]
                .map { String(format: "%02x", $0) }.joined()
            return "\(type):\(unit.length):\(head)"
        }
        return "bytes=\(bytes.count) nals=[\(described.joined(separator: ", "))]"
    }

    public var onPixelBuffer: (@Sendable (CVPixelBuffer, CMTime, Bool) -> Void)?
    /// Identifies the exact submission, including failures and callbacks that occur
    /// before DecodeFrame returns. Completion order must not determine frame timing.
    public var onDecodeCompleted: (@Sendable (UInt32, Bool) -> Void)?
    public var onFatalFormatFailure: (@Sendable (String) -> Void)?

    public init(codec: NVSTVideoCodec, requiresTenBit444: Bool = false) {
        self.codec = codec
        self.requiresTenBit444 = requiresTenBit444
    }

    public var decodedFrameCount: UInt64 { statsLock.lock(); defer { statsLock.unlock() }; return decodedFrames }
    /// `WIDTHxHEIGHT` of the last decoded frame, or nil before the first one.
    public var decodedResolution: String? {
        statsLock.lock()
        defer { statsLock.unlock() }
        guard decodedWidth > 0, decodedHeight > 0 else { return nil }
        return "\(decodedWidth)x\(decodedHeight)"
    }
    private var decodedWidth = 0
    private var decodedHeight = 0

    /// The last format description's declared depth and chroma layout, or nil before the first
    /// keyframe.
    public var bitstreamFormat: BitstreamFormat? {
        statsLock.lock()
        defer { statsLock.unlock() }
        return currentBitstreamFormat
    }
    var currentBitstreamFormat: BitstreamFormat?
    /// Four-character name of the actual decoded surface, or the requested format before output.
    public var outputPixelFormatName: String {
        statsLock.lock()
        defer { statsLock.unlock() }
        return Self.pixelFormatName(outputPixelFormat)
    }
    private var outputPixelFormat: OSType = 0
    public var failedFrameCount: UInt64 { statsLock.lock(); defer { statsLock.unlock() }; return failedFrames }
    /// `first/last` OSStatus of the asynchronous decode failures, or "-" when there were none.
    public var failureStatusSummary: String {
        statsLock.lock()
        defer { statsLock.unlock() }
        guard firstFailureStatus != noErr || lastFailureStatus != noErr else { return "-" }
        return firstFailureStatus == lastFailureStatus ? "\(firstFailureStatus)" : "\(firstFailureStatus)/\(lastFailureStatus)"
    }

    public func invalidate() {
        stateLock.lock()
        let expiring = session
        session = nil
        formatDescription = nil
        parameterSets = NvstElementaryStream.ParameterSets()
        stateLock.unlock()
        Self.tearDown(expiring)
    }

    /// Decodes one access unit. Throws `missingParameterSets` until the first keyframe arrives,
    /// which is the normal state while the seat is still answering the initial PLI.
    @discardableResult
    public func decode(_ unit: NvstAccessUnit) throws -> Bool {
        let decodeStart = DispatchTime.now().uptimeNanoseconds
        // Nothing before the stream's first keyframe can be decoded: a P-frame with no reference
        // comes out as garbage and leaves the session behind for good (measured 2026-09-05 — the
        // first prewarmed session decoded a stray P-frame first and never got under 20 ms a frame).
        // Without a prewarm this gate was implicit, because the parameter sets only arrive with the
        // keyframe; a prewarmed session has them already, so it is stated here.
        stateLock.lock()
        let awaitingFirstKeyframe = !hasSeenKeyframe
        if unit.isKeyframe { hasSeenKeyframe = true }
        stateLock.unlock()
        guard !awaitingFirstKeyframe || unit.isKeyframe else { throw DecoderError.missingParameterSets }
        // One pass produces both the parameter sets and the length-prefixed sample. See
        // `NvstElementaryStream.prepare` for what this replaced.
        let prepared = NvstElementaryStream.prepare(unit.bytes, codec: codec)

        let sample = prepared.sample
        noteAv1FrameShape(unit, sample: sample)
        guard !sample.isEmpty else { return false }

        let (session, description) = try prepareSession(for: prepared.parameterSets)

        let buildStart = DispatchTime.now().uptimeNanoseconds
        let sampleBuffer = try makeSampleBuffer(
            sample: sample,
            formatDescription: description,
            presentationTime: CMTime(value: CMTimeValue(unit.rtpTimestamp), timescale: Self.clockRate)
        )
        let submitStart = DispatchTime.now().uptimeNanoseconds

        var flagsOut = VTDecodeInfoFlags()
        let isKeyframe = unit.isKeyframe
        // Described only when a report is actually going to be emitted. Building it eagerly walked
        // every NAL of every access unit — a per-frame scan of up to 112 KB whose result was thrown
        // away for all but the first handful of frames.
        let bytes = unit.bytes
        let codec = codec
        let shape: @Sendable () -> String = { Self.accessUnitShape(bytes, codec: codec) }
        let logFailure = onDecodeFailure
        let frameIndex = unit.frameIndex
        let status = VTDecompressionSessionDecodeFrame(
            session,
            sampleBuffer: sampleBuffer,
            // WebRTC's own VideoToolbox decoder (`RTCVideoDecoderH264.mm`, the path the vendored
            // WebRTC-based build uses) sets only `_EnableAsynchronousDecompression`. Temporal
            // processing buffers frames for B-frame reordering — pure added latency on a
            // low-latency cloud-gaming stream that encodes without B-frames.
            flags: [._EnableAsynchronousDecompression],
            infoFlagsOut: &flagsOut,
            outputHandler: { [weak self] status, _, imageBuffer, presentationTime, _ in
                guard let self else { return }
                handleDecodedFrame(status: status,
                                   imageBuffer: imageBuffer,
                                   presentationTime: presentationTime,
                                   frameIndex: frameIndex,
                                   isKeyframe: isKeyframe,
                                   shape: shape,
                                   logFailure: logFailure)
            }
        )
        noteStageTimings(prepare: decodeStart, build: buildStart, submit: submitStart)
        guard status == noErr else {
            statsLock.lock()
            failedFrames &+= 1
            statsLock.unlock()
            // A rejected frame usually means the session lost its reference chain; force a
            // rebuild so the next keyframe can restart cleanly.
            stateLock.lock()
            let broken = self.session
            self.session = nil
            stateLock.unlock()
            Self.tearDown(broken)
            throw DecoderError.decodeFailed(status)
        }
        return true
    }

    /// AV1 frame accounting ahead of session setup: the first keyframe's wire bytes, logged once
    /// per session — the capture that settles whether the seat's access unit is a bare OBU stream
    /// or carries AV1-Annex-B temporal-unit prefixes — and a name for frames that are not
    /// size-fielded OBU streams at all, so a misframed stream invalidates a frame and asks the
    /// seat for a fresh keyframe instead of silently never decoding.
    private func noteAv1FrameShape(_ unit: NvstAccessUnit, sample: Data) {
        guard codec == .av1 else { return }
        if unit.isKeyframe {
            stateLock.lock()
            let first = !loggedFirstAv1Keyframe
            loggedFirstAv1Keyframe = true
            stateLock.unlock()
            if first {
                let head = unit.bytes.prefix(96).map { String(format: "%02x", $0) }.joined()
                onDecodeFailure?(0, "NVST AV1 first keyframe, \(unit.bytes.count) bytes, head: \(head)")
            }
        }
        if sample.isEmpty, !unit.bytes.isEmpty {
            onDecodeFailure?(unit.frameIndex, "NVST AV1 access unit is not a size-fielded OBU stream; frame dropped \(Self.accessUnitShape(unit.bytes, codec: codec))")
        }
    }

    /// Installs the parameter sets this access unit carries and returns the session and format
    /// description to decode it with, rebuilding either when the stream geometry changed.
    private func prepareSession(for incoming: NvstElementaryStream.ParameterSets) throws -> (VTDecompressionSession, CMFormatDescription) {
        stateLock.lock()
        var expiring: VTDecompressionSession?
        if incoming.isComplete(for: codec), incoming != parameterSets {
            parameterSets = incoming
            // New parameter sets mean new stream geometry: rebuild before decoding this frame.
            expiring = session
            session = nil
            formatDescription = nil
        }
        let sets = parameterSets
        var description = formatDescription
        stateLock.unlock()
        Self.tearDown(expiring)

        guard sets.isComplete(for: codec) else { throw DecoderError.missingParameterSets }
        if description == nil {
            description = try makeFormatDescription(sets)
        }
        guard let description else { throw DecoderError.missingParameterSets }

        stateLock.lock()
        formatDescription = description
        var active = session
        stateLock.unlock()
        if active == nil {
            active = try makeSession(formatDescription: description)
            stateLock.lock()
            // Another frame may have installed a session already; keep the first one.
            if let existing = session {
                let redundant = active
                active = existing
                stateLock.unlock()
                Self.tearDown(redundant)
            } else {
                session = active
                stateLock.unlock()
            }
        }
        guard let active else { throw DecoderError.sessionCreationFailed(-1) }
        return (active, description)
    }

    /// VideoToolbox's asynchronous answer for one submitted frame.
    private func handleDecodedFrame(status: OSStatus,
                                    imageBuffer: CVImageBuffer?,
                                    presentationTime: CMTime,
                                    frameIndex: UInt32,
                                    isKeyframe: Bool,
                                    shape: @escaping @Sendable () -> String,
                                    logFailure: ((UInt32, String) -> Void)?) {
        if status == noErr, let imageBuffer, requiresTenBit444,
           !NativeStreamTenBitSurface.preserves444(imageBuffer) {
            statsLock.lock(); failedFrames &+= 1; statsLock.unlock()
            onDecodeCompleted?(frameIndex, false)
            let message = DecoderError.output444NotPreserved.localizedDescription
            logFailure?(frameIndex, message)
            onFatalFormatFailure?(message)
            return
        }
        guard status == noErr, let imageBuffer else {
            statsLock.lock()
            failedFrames &+= 1
            let shouldReport = loggedFailures < Self.maxLoggedFailures
            if shouldReport { loggedFailures += 1 }
            // Asynchronous decode reports its reason only here. Counting without it turns every
            // decoder problem into the same unreadable number.
            if firstFailureStatus == noErr { firstFailureStatus = status }
            lastFailureStatus = status
            statsLock.unlock()
            if shouldReport {
                logFailure?(frameIndex, "NVST decode rejected OSStatus \(status) frame=\(frameIndex) keyframe=\(isKeyframe) \(shape())")
            }
            onDecodeCompleted?(frameIndex, false)
            return
        }
        onDecodeCompleted?(frameIndex, true)
        statsLock.lock()
        decodedFrames &+= 1
        // Measured from the decoded surface, not from what the negotiation asked for: a seat that
        // ignores the requested geometry is otherwise invisible here.
        decodedWidth = CVPixelBufferGetWidth(imageBuffer)
        decodedHeight = CVPixelBufferGetHeight(imageBuffer)
        outputPixelFormat = CVPixelBufferGetPixelFormatType(imageBuffer)
        let handler = onPixelBuffer
        let shouldReportAccepted = loggedAccepted < Self.maxLoggedAccepted
        if shouldReportAccepted { loggedAccepted += 1 }
        statsLock.unlock()
        // A rejected unit only means something next to an accepted one.
        if shouldReportAccepted {
            logFailure?(0, "NVST decode accepted frame=\(frameIndex) keyframe=\(isKeyframe) \(shape())")
        }
        handler?(imageBuffer, presentationTime, isKeyframe)
    }

    /// Where `decode` actually spends its time. `submit` is VideoToolbox itself — it applies
    /// backpressure once its queue is full, so a rising submit time means the decoder, not our
    /// own work, is the limit. `prepare` and `build` are ours: the parameter-set scan and the
    /// Annex-B-to-sample-buffer copy.
    public struct StageTimings: Sendable, Equatable {
        public var prepareMilliseconds = 0.0
        public var buildMilliseconds = 0.0
        public var submitMilliseconds = 0.0
    }

    /// Peaks and means together: a peak names the worst stall, a mean names the steady-state cost
    /// that decides whether the client can keep up with the stream's frame rate at all.
    public var stageTimingSummary: String {
        statsLock.lock()
        defer { statsLock.unlock() }
        let frames = max(1, timedFrames)
        return String(format: "peak[prepare=%.1f build=%.1f submit=%.1f] mean[prepare=%.2f build=%.2f submit=%.2f]ms",
                      peakStages.prepareMilliseconds, peakStages.buildMilliseconds, peakStages.submitMilliseconds,
                      totalStages.prepareMilliseconds / Double(frames),
                      totalStages.buildMilliseconds / Double(frames),
                      totalStages.submitMilliseconds / Double(frames))
    }

    public var peakStageTimings: StageTimings { statsLock.lock(); defer { statsLock.unlock() }; return peakStages }
    private var peakStages = StageTimings()
    private var totalStages = StageTimings()
    private var timedFrames: UInt64 = 0

    private func noteStageTimings(prepare: UInt64, build: UInt64, submit: UInt64) {
        let now = DispatchTime.now().uptimeNanoseconds
        let prepareMs = Self.milliseconds(from: prepare, to: build)
        let buildMs = Self.milliseconds(from: build, to: submit)
        let submitMs = Self.milliseconds(from: submit, to: now)
        statsLock.lock()
        peakStages.prepareMilliseconds = max(peakStages.prepareMilliseconds, prepareMs)
        peakStages.buildMilliseconds = max(peakStages.buildMilliseconds, buildMs)
        peakStages.submitMilliseconds = max(peakStages.submitMilliseconds, submitMs)
        totalStages.prepareMilliseconds += prepareMs
        totalStages.buildMilliseconds += buildMs
        totalStages.submitMilliseconds += submitMs
        timedFrames &+= 1
        statsLock.unlock()
    }

    /// Blocks until the session's queued frames have drained, so it must never run under a lock
    /// the output handler needs.
    public func drain() {
        stateLock.lock()
        let active = session
        stateLock.unlock()
        guard let active else { return }
        VTDecompressionSessionWaitForAsynchronousFrames(active)
    }

    // MARK: - Session

    private func makeSession(formatDescription: CMVideoFormatDescription) throws -> VTDecompressionSession {
        // Preserve bit depth and chroma in IOSurface-backed bi-planar output. Strict
        // 4:4:4 offers both compatible ranges so the hardware may use its native surface.
        statsLock.lock()
        let bitstream = currentBitstreamFormat ?? BitstreamFormat()
        statsLock.unlock()
        if requiresTenBit444 { try Self.validate444Bitstream(bitstream) }
        // Ask for hardware explicitly rather than taking the default. VideoToolbox will fall back to
        // a software decoder without saying so, and a software HEVC decode at 5120x2160 cannot hold
        // 120 fps — which is indistinguishable, from the outside, from "decode is slow".
        let specification: [CFString: Any] = [
            kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder: true,
        ]
        var created: VTDecompressionSession?
        var status: OSStatus = noErr
        var chosenFormat: OSType = 0
        let fullRange = (CMFormatDescriptionGetExtension(formatDescription,
            extensionKey: kCMFormatDescriptionExtension_FullRangeVideo) as? NSNumber)?.boolValue ?? false
        let requests = Self.outputPixelFormatRequests(for: bitstream, requiresTenBit444: requiresTenBit444,
            sourceIsFullRange: fullRange)
        var selectedRequest: [OSType] = []
        for candidate in requests {
            let pixelFormats: Any
            if candidate.count == 1 { pixelFormats = NSNumber(value: candidate[0]) }
            else { pixelFormats = candidate.map { NSNumber(value: $0) } as NSArray }
            let attributes: [CFString: Any] = [
                kCVPixelBufferPixelFormatTypeKey: pixelFormats,
                kCVPixelBufferIOSurfacePropertiesKey: [:] as CFDictionary,
                kCVPixelBufferMetalCompatibilityKey: true,
            ]
            var attempt: VTDecompressionSession?
            status = VTDecompressionSessionCreate(
                allocator: kCFAllocatorDefault,
                formatDescription: formatDescription,
                decoderSpecification: specification as CFDictionary,
                imageBufferAttributes: attributes as CFDictionary,
                outputCallback: nil,
                decompressionSessionOut: &attempt
            )
            if status == noErr, let attempt {
                created = attempt
                selectedRequest = candidate
                chosenFormat = candidate.count == 1 ? candidate[0] : 0
                break
            }
            onDecodeFailure?(0, "NVST decoder declined output \(candidate.map(Self.pixelFormatName).joined(separator: ",")) for \(bitstream.summary) (OSStatus \(status)); trying the next format")
        }
        guard status == noErr, let created else {
            if requiresTenBit444 { throw DecoderError.unsupported444Hardware(status) }
            throw DecoderError.sessionCreationFailed(status)
        }
        statsLock.lock()
        outputPixelFormat = chosenFormat
        statsLock.unlock()
        VTSessionSetProperty(created, key: kVTDecompressionPropertyKey_RealTime, value: kCFBooleanTrue)
        // And then verify it, because asking is not getting.
        var usingHardware: Unmanaged<CFTypeRef>?
        VTSessionCopyProperty(created,
                              key: kVTDecompressionPropertyKey_UsingHardwareAcceleratedVideoDecoder,
                              allocator: kCFAllocatorDefault,
                              valueOut: &usingHardware)
        let isHardware = (usingHardware?.takeRetainedValue() as? NSNumber)?.boolValue ?? false
        guard isHardware else {
            VTDecompressionSessionInvalidate(created)
            throw DecoderError.hardwareRequired
        }
        statsLock.lock()
        usesHardwareDecoder = isHardware
        statsLock.unlock()
        var performanceFormats: Unmanaged<CFTypeRef>?
        VTSessionCopyProperty(created, key: kVTDecompressionPropertyKey_SupportedPixelFormatsOrderedByPerformance,
            allocator: kCFAllocatorDefault, valueOut: &performanceFormats)
        let performance = (performanceFormats?.takeRetainedValue() as? [NSNumber])?
            .prefix(8).map { Self.pixelFormatName($0.uint32Value) }.joined(separator: ",") ?? "unavailable"
        onDecodeFailure?(0, "NVST decoder session created codec=\(codec.rawValue) hardware=\(isHardware) bitstream=\(bitstream.summary) sourceRange=\(fullRange ? "full" : "video") requested=\(selectedRequest.map(Self.pixelFormatName).joined(separator: ",")) nativeFastest=\(performance)")
        // Creating a hardware decompression session costs hundreds of milliseconds, and it happens
        // on the frame path — so a rebuild storm reads as random latency spikes. Counted so a spike
        // can be attributed to it instead of guessed at.
        statsLock.lock()
        sessionsCreated &+= 1
        statsLock.unlock()
        return created
    }

    /// How many decompression sessions this decoder has built. One per session is normal; more
    /// means the session is being torn down and rebuilt mid-stream.
    public var sessionCreationCount: UInt64 { statsLock.lock(); defer { statsLock.unlock() }; return sessionsCreated }
    /// Whether VideoToolbox actually gave us the hardware decoder.
    public var isHardwareAccelerated: Bool { statsLock.lock(); defer { statsLock.unlock() }; return usesHardwareDecoder }
    private var usesHardwareDecoder = false
    private var sessionsCreated: UInt64 = 0

}
