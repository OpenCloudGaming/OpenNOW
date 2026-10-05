import Foundation
import CoreMedia
import VideoToolbox

#if canImport(WebRTC) && os(iOS)
@preconcurrency import WebRTC
#endif

struct NativeStreamCodecCapability: Equatable {
    let codec: NativeStreamVideoCodec
    let videoToolboxHardwareDecode: Bool
    let webRTCSupported: Bool
    let webRTCProfileSummary: [String]

    var launchSafe: Bool {
        switch codec {
        case .h264:
            return webRTCSupported
        case .h265, .av1:
            return videoToolboxHardwareDecode && webRTCSupported
        }
    }
}

struct NativeStreamCodecReport: Equatable {
    let capabilities: [NativeStreamCodecCapability]

    func capability(for codec: NativeStreamVideoCodec) -> NativeStreamCodecCapability? {
        capabilities.first { $0.codec == codec }
    }

    func launchSafeCodec(preferred: String) -> NativeStreamVideoCodec {
        if let requested = NativeStreamVideoCodec.normalized(preferred),
           capability(for: requested)?.launchSafe == true {
            return requested
        }
        return [.h265, .av1, .h264].first { capability(for: $0)?.launchSafe == true } ?? .h264
    }

    var summary: String {
        capabilities.map { capability in
            let safe = capability.launchSafe ? "safe" : "unsafe"
            let vt = capability.videoToolboxHardwareDecode ? "VT=hw" : "VT=no-hw"
            let rtc = capability.webRTCSupported ? "WebRTC=yes" : "WebRTC=no"
            return "\(capability.codec.rawValue):\(safe):\(vt):\(rtc)"
        }.joined(separator: " ")
    }
}

struct NativeStreamLaunchSettings: Equatable {
    let settings: AppSettings
    let selectedCodec: NativeStreamVideoCodec
    let reason: String?
}

enum NativeStreamLaunchSettingsResolver {
    static func resolve(
        _ settings: AppSettings,
        codecReport: NativeStreamCodecReport = NativeStreamCodecProbe.report()
    ) -> NativeStreamLaunchSettings {
        var normalized = settings
        normalized.normalizeStreamDefaults()

        if let requested = NativeStreamVideoCodec.normalized(normalized.preferredCodec) {
            if codecReport.capability(for: requested)?.launchSafe == true {
                return NativeStreamLaunchSettings(settings: normalized, selectedCodec: requested, reason: nil)
            }

            // An explicit codec choice is session authority. Report that this runtime cannot use
            // it, but do not silently rewrite codec, resolution, FPS, bitrate, HDR, or colour.
            // The stream surface will stop with a precise compatibility error before attempting
            // to decode it; only Auto may choose another available codec.
            return NativeStreamLaunchSettings(
                settings: normalized,
                selectedCodec: requested,
                reason: "\(requested.rawValue) is not available through iOS WebRTC; preserving the selected profile"
            )
        }

        let selected = codecReport.launchSafeCodec(preferred: normalized.preferredCodec)
        var resolved = normalized
        resolved.preferredCodec = selected.rawValue
        resolved.normalizeStreamDefaults()
        return NativeStreamLaunchSettings(
            settings: resolved,
            selectedCodec: selected,
            reason: "Auto codec resolved to \(selected.rawValue)"
        )
    }
}

enum NativeStreamCodecProbe {
    static func report() -> NativeStreamCodecReport {
        let supportedWebRTCCodecs = webRTCSupportedCodecs()
        let capabilities = NativeStreamVideoCodec.allCases.map { codec in
            NativeStreamCodecCapability(
                codec: codec,
                videoToolboxHardwareDecode: videoToolboxHardwareDecodeSupported(for: codec),
                webRTCSupported: supportedWebRTCCodecs[codec]?.isEmpty == false,
                webRTCProfileSummary: supportedWebRTCCodecs[codec] ?? []
            )
        }
        return NativeStreamCodecReport(capabilities: capabilities)
    }

    private static func videoToolboxHardwareDecodeSupported(for codec: NativeStreamVideoCodec) -> Bool {
        switch codec {
        case .h264:
            return VTIsHardwareDecodeSupported(kCMVideoCodecType_H264)
        case .h265:
            return VTIsHardwareDecodeSupported(kCMVideoCodecType_HEVC)
        case .av1:
            if #available(iOS 16.0, tvOS 16.0, *) {
                return VTIsHardwareDecodeSupported(kCMVideoCodecType_AV1)
            }
            return false
        }
    }

    private static func webRTCSupportedCodecs() -> [NativeStreamVideoCodec: [String]] {
        #if canImport(WebRTC) && os(iOS)
        let factory = NativeStreamVideoDecoderFactory()
        return factory.supportedCodecs().reduce(into: [NativeStreamVideoCodec: [String]]()) { partial, info in
            guard let codec = NativeStreamVideoCodec.normalized(info.name) else { return }
            let params = info.parameters
                .map { "\($0.key)=\($0.value)" }
                .sorted()
                .joined(separator: " ")
            let summary = params.isEmpty ? codec.rawValue : "\(codec.rawValue) \(params)"
            partial[codec, default: []].append(summary)
        }
        #else
        return [.h264: ["H264"]]
        #endif
    }
}

#if canImport(WebRTC) && os(iOS)
/// The bundled SDK exposes only the process-wide field-trial initializer. Its
/// configuration must be established once, before SSL, codec factories or views.
enum NativeStreamWebRTCPolicy {
    private static let initialized: Void = {
        // The default zero-playout decode spacing is 8 ms. At 120 Hz that
        // schedules frames almost a whole refresh apart before timer variance.
        // Keep compressed-frame correctness/backpressure in the hardware decoder;
        // allow the receiver to hand an available frame over promptly.
        RTCInitFieldTrialDictionary(["WebRTC-ZeroPlayoutDelay": "min_pacing:1ms"])
    }()

    static func initialize() { _ = initialized }
}

final class NativeStreamVideoDecoderFactory: NSObject, RTCVideoDecoderFactory {
    private let defaultFactory: RTCDefaultVideoDecoderFactory = {
        NativeStreamWebRTCPolicy.initialize()
        return RTCDefaultVideoDecoderFactory()
    }()

    func createDecoder(_ info: RTCVideoCodecInfo) -> RTCVideoDecoder? {
        if NativeStreamVideoCodec.normalized(info.name) == .av1,
           NativeStreamAV1HardwareDecoder.isSupported {
            return NativeStreamAV1VideoDecoder()
        }
        guard NativeStreamVideoCodec.normalized(info.name) == .h265 else {
            return defaultFactory.createDecoder(info)
        }
        guard NativeStreamHEVCVideoDecoder.isSupported else { return nil }
        return NativeStreamHEVCVideoDecoder()
    }

    func supportedCodecs() -> [RTCVideoCodecInfo] {
        let defaultCodecs = defaultFactory.supportedCodecs().filter {
            NativeStreamVideoCodec.normalized($0.name) != .h265
        }
        guard NativeStreamHEVCVideoDecoder.isSupported else { return defaultCodecs }
        return defaultCodecs + NativeStreamHEVCVideoDecoder.supportedCodecInfos
    }
}

private final class NativeStreamHEVCFrameMetadata {
    let rtpTimestamp: UInt32
    let timestampNs: Int64
    let rotation: RTCVideoRotation

    init(encodedImage: RTCEncodedImage) {
        rtpTimestamp = encodedImage.timeStamp
        if encodedImage.captureTimeMs > 0 {
            timestampNs = encodedImage.captureTimeMs * 1_000_000
        } else {
            timestampNs = Int64(encodedImage.timeStamp) * 1_000_000_000 / 90_000
        }
        rotation = encodedImage.rotation
    }
}

private struct NativeStreamHEVCNALUnit {
    let type: UInt8
    let payload: Data
}

private final class NativeStreamHEVCVideoDecoder: NSObject, RTCVideoDecoder {
    static var isSupported: Bool {
        VTIsHardwareDecodeSupported(kCMVideoCodecType_HEVC)
    }

    static let supportedCodecInfos: [RTCVideoCodecInfo] = [
        RTCVideoCodecInfo(name: "H265", parameters: [
            "profile-id": "1",
            "level-id": "153",
            "tier-flag": "0"
        ]),
        RTCVideoCodecInfo(name: "H265", parameters: [
            "profile-id": "2",
            "level-id": "153",
            "tier-flag": "0"
        ])
    ]

    private let lock = NSLock()
    private var callback: RTCVideoDecoderCallback?
    private var formatDescription: CMVideoFormatDescription?
    private var decompressionSession: VTDecompressionSession?
    private var parameterSetKey: [Data]?
    private var loggedFirstFrame = false

    func setCallback(_ callback: @escaping RTCVideoDecoderCallback) {
        lock.withLock {
            self.callback = callback
        }
    }

    func startDecode(withNumberOfCores numberOfCores: Int32) -> Int {
        0
    }

    func release() -> Int {
        lock.withLock {
            if let decompressionSession {
                VTDecompressionSessionInvalidate(decompressionSession)
            }
            decompressionSession = nil
            formatDescription = nil
            parameterSetKey = nil
            callback = nil
            loggedFirstFrame = false
        }
        return 0
    }

    func decode(
        _ encodedImage: RTCEncodedImage,
        missingFrames: Bool,
        codecSpecificInfo info: RTCCodecSpecificInfo?,
        renderTimeMs: Int64
    ) -> Int {
        let nalUnits = Self.parseNALUnits(from: encodedImage.buffer)
        guard !nalUnits.isEmpty else { return -1 }

        if updateFormatDescriptionIfNeeded(from: nalUnits) != 0 {
            return -1
        }

        guard let sampleBuffer = makeSampleBuffer(from: nalUnits, encodedImage: encodedImage) else {
            return -1
        }

        let metadata = NativeStreamHEVCFrameMetadata(encodedImage: encodedImage)
        let metadataRef = Unmanaged.passRetained(metadata)
        var infoFlags = VTDecodeInfoFlags()
        guard let decompressionSession = lock.withLock({ self.decompressionSession }) else {
            metadataRef.release()
            return Int(kVTInvalidSessionErr)
        }
        let status = VTDecompressionSessionDecodeFrame(
            decompressionSession,
            sampleBuffer: sampleBuffer,
            flags: [],
            frameRefcon: metadataRef.toOpaque(),
            infoFlagsOut: &infoFlags
        )
        if status != noErr {
            metadataRef.release()
            NSLog("[OpenNOW] HEVC decode failed status=%d", status)
            return Int(status)
        }
        return 0
    }

    func implementationName() -> String {
        "OpenNOW-VideoToolbox-HEVC"
    }

    private func updateFormatDescriptionIfNeeded(from nalUnits: [NativeStreamHEVCNALUnit]) -> Int {
        guard let vps = nalUnits.last(where: { $0.type == 32 })?.payload,
              let sps = nalUnits.last(where: { $0.type == 33 })?.payload,
              let pps = nalUnits.last(where: { $0.type == 34 })?.payload else {
            return lock.withLock { decompressionSession == nil ? -1 : 0 }
        }

        let nextKey = [vps, sps, pps]
        if lock.withLock({ parameterSetKey == nextKey && decompressionSession != nil }) {
            return 0
        }

        var nextFormatDescription: CMVideoFormatDescription?
        let formatStatus = withHEVCParameterSetPointers([vps, sps, pps]) { pointers, sizes, count in
            CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                allocator: kCFAllocatorDefault,
                parameterSetCount: count,
                parameterSetPointers: pointers,
                parameterSetSizes: sizes,
                nalUnitHeaderLength: 4,
                extensions: nil,
                formatDescriptionOut: &nextFormatDescription
            )
        }
        guard formatStatus == noErr, let nextFormatDescription else {
            NSLog("[OpenNOW] HEVC format description failed status=%d", formatStatus)
            return Int(formatStatus)
        }

        guard let sourceDepth = NativeStreamHEVCOutput.bitDepth(sps: sps),
              sourceDepth == 8 || sourceDepth == 10 else {
            NSLog("[OpenNOW] HEVC SPS has unsupported or invalid source bit depth")
            return -1
        }

        let decoderSpecification: [CFString: Any]?
        if #available(iOS 17.0, *) {
            decoderSpecification = [
                kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder: true
            ]
        } else {
            decoderSpecification = nil
        }
        let imageBufferAttributes: [CFString: Any] = [
            kCVPixelBufferPixelFormatTypeKey: NativeStreamHEVCOutput.pixelFormat(sps: sps),
            kCVPixelBufferMetalCompatibilityKey: true,
            kCVPixelBufferIOSurfacePropertiesKey: [:]
        ]
        var callbackRecord = VTDecompressionOutputCallbackRecord(
            decompressionOutputCallback: Self.decompressionOutputCallback,
            decompressionOutputRefCon: Unmanaged.passUnretained(self).toOpaque()
        )
        var nextSession: VTDecompressionSession?
        let sessionStatus = VTDecompressionSessionCreate(
            allocator: kCFAllocatorDefault,
            formatDescription: nextFormatDescription,
            decoderSpecification: decoderSpecification.map { $0 as CFDictionary },
            imageBufferAttributes: imageBufferAttributes as CFDictionary,
            outputCallback: &callbackRecord,
            decompressionSessionOut: &nextSession
        )
        guard sessionStatus == noErr, let nextSession else {
            NSLog("[OpenNOW] HEVC decompression session failed status=%d", sessionStatus)
            return Int(sessionStatus)
        }

        lock.withLock {
            if let decompressionSession {
                VTDecompressionSessionInvalidate(decompressionSession)
            }
            formatDescription = nextFormatDescription
            decompressionSession = nextSession
            parameterSetKey = nextKey
            loggedFirstFrame = false
        }
        NSLog("[OpenNOW] HEVC decompression session ready output=%u", NativeStreamHEVCOutput.pixelFormat(sps: sps))
        return 0
    }

    private func makeSampleBuffer(from nalUnits: [NativeStreamHEVCNALUnit], encodedImage: RTCEncodedImage) -> CMSampleBuffer? {
        let payload = Self.lengthPrefixedPayload(from: nalUnits)
        guard !payload.isEmpty else { return nil }

        var blockBuffer: CMBlockBuffer?
        let blockStatus = CMBlockBufferCreateWithMemoryBlock(
            allocator: kCFAllocatorDefault,
            memoryBlock: nil,
            blockLength: payload.count,
            blockAllocator: kCFAllocatorDefault,
            customBlockSource: nil,
            offsetToData: 0,
            dataLength: payload.count,
            flags: 0,
            blockBufferOut: &blockBuffer
        )
        guard blockStatus == noErr, let blockBuffer else { return nil }

        let replaceStatus = payload.withUnsafeBytes { bytes in
            guard let base = bytes.baseAddress else { return kCMBlockBufferBadPointerParameterErr }
            return CMBlockBufferReplaceDataBytes(
                with: base,
                blockBuffer: blockBuffer,
                offsetIntoDestination: 0,
                dataLength: payload.count
            )
        }
        guard replaceStatus == noErr else { return nil }

        guard let formatDescription = lock.withLock({ self.formatDescription }) else { return nil }
        var timing = CMSampleTimingInfo(
            duration: .invalid,
            presentationTimeStamp: CMTime(value: CMTimeValue(encodedImage.timeStamp), timescale: 90_000),
            decodeTimeStamp: .invalid
        )
        var sampleSize = payload.count
        var sampleBuffer: CMSampleBuffer?
        let sampleStatus = CMSampleBufferCreateReady(
            allocator: kCFAllocatorDefault,
            dataBuffer: blockBuffer,
            formatDescription: formatDescription,
            sampleCount: 1,
            sampleTimingEntryCount: 1,
            sampleTimingArray: &timing,
            sampleSizeEntryCount: 1,
            sampleSizeArray: &sampleSize,
            sampleBufferOut: &sampleBuffer
        )
        guard sampleStatus == noErr else { return nil }
        return sampleBuffer
    }

    private func handleDecodedFrame(
        status: OSStatus,
        imageBuffer: CVImageBuffer?,
        metadata: NativeStreamHEVCFrameMetadata?
    ) {
        guard status == noErr, let imageBuffer, let metadata else {
            if status != noErr {
                NSLog("[OpenNOW] HEVC output callback failed status=%d", status)
            }
            return
        }

        let frameCallback = lock.withLock { callback }
        guard let frameCallback else { return }

        let pixelBuffer = RTCCVPixelBuffer(pixelBuffer: imageBuffer)
        let frame = RTCVideoFrame(
            buffer: pixelBuffer,
            rotation: metadata.rotation,
            timeStampNs: metadata.timestampNs
        )
        frame.timeStamp = Int32(bitPattern: metadata.rtpTimestamp)
        frameCallback(frame)

        let shouldLogFirstFrame = lock.withLock { () -> Bool in
            if loggedFirstFrame { return false }
            loggedFirstFrame = true
            return true
        }
        if shouldLogFirstFrame {
            NSLog("[OpenNOW] HEVC decoded first frame format=%u color=%@", CVPixelBufferGetPixelFormatType(imageBuffer), NativeStreamHDRTransfer.colorMode(in: imageBuffer))
        }
    }

    private static let decompressionOutputCallback: VTDecompressionOutputCallback = {
        refCon,
        sourceFrameRefCon,
        status,
        _,
        imageBuffer,
        _,
        _ in
        guard let refCon else { return }
        let decoder = Unmanaged<NativeStreamHEVCVideoDecoder>.fromOpaque(refCon).takeUnretainedValue()
        let metadata = sourceFrameRefCon.map {
            Unmanaged<NativeStreamHEVCFrameMetadata>.fromOpaque($0).takeRetainedValue()
        }
        decoder.handleDecodedFrame(status: status, imageBuffer: imageBuffer, metadata: metadata)
    }

    private static func parseNALUnits(from data: Data) -> [NativeStreamHEVCNALUnit] {
        guard !data.isEmpty else { return [] }
        if let annexB = parseAnnexBNALUnits(from: data), !annexB.isEmpty {
            return annexB
        }
        if let lengthPrefixed = parseLengthPrefixedNALUnits(from: data), !lengthPrefixed.isEmpty {
            return lengthPrefixed
        }
        return [makeNALUnit(payload: data)].compactMap { $0 }
    }

    private static func parseAnnexBNALUnits(from data: Data) -> [NativeStreamHEVCNALUnit]? {
        let bytes = [UInt8](data)
        var starts: [(codeOffset: Int, payloadOffset: Int)] = []
        var index = 0
        while index + 3 <= bytes.count {
            if index + 3 < bytes.count,
               bytes[index] == 0,
               bytes[index + 1] == 0,
               bytes[index + 2] == 0,
               bytes[index + 3] == 1 {
                starts.append((index, index + 4))
                index += 4
            } else if bytes[index] == 0,
                      bytes[index + 1] == 0,
                      bytes[index + 2] == 1 {
                starts.append((index, index + 3))
                index += 3
            } else {
                index += 1
            }
        }
        guard !starts.isEmpty else { return nil }

        var units: [NativeStreamHEVCNALUnit] = []
        for position in starts.indices {
            let payloadStart = starts[position].payloadOffset
            let payloadEnd = position + 1 < starts.count ? starts[position + 1].codeOffset : bytes.count
            guard payloadStart < payloadEnd else { continue }
            let payload = Data(bytes[payloadStart..<payloadEnd])
            if let unit = makeNALUnit(payload: payload) {
                units.append(unit)
            }
        }
        return units
    }

    private static func parseLengthPrefixedNALUnits(from data: Data) -> [NativeStreamHEVCNALUnit]? {
        let bytes = [UInt8](data)
        var offset = 0
        var units: [NativeStreamHEVCNALUnit] = []
        while offset + 4 <= bytes.count {
            let length = (Int(bytes[offset]) << 24)
                | (Int(bytes[offset + 1]) << 16)
                | (Int(bytes[offset + 2]) << 8)
                | Int(bytes[offset + 3])
            offset += 4
            guard length > 0, offset + length <= bytes.count else { return nil }
            let payload = Data(bytes[offset..<(offset + length)])
            if let unit = makeNALUnit(payload: payload) {
                units.append(unit)
            }
            offset += length
        }
        return offset == bytes.count ? units : nil
    }

    private static func makeNALUnit(payload: Data) -> NativeStreamHEVCNALUnit? {
        guard payload.count >= 2 else { return nil }
        let type = payload[payload.startIndex] >> 1 & 0x3f
        return NativeStreamHEVCNALUnit(type: type, payload: payload)
    }

    private static func lengthPrefixedPayload(from nalUnits: [NativeStreamHEVCNALUnit]) -> Data {
        var output = Data()
        output.reserveCapacity(nalUnits.reduce(0) { $0 + 4 + $1.payload.count })
        for unit in nalUnits {
            var length = UInt32(unit.payload.count).bigEndian
            withUnsafeBytes(of: &length) { output.append(contentsOf: $0) }
            output.append(unit.payload)
        }
        return output
    }

    private func withHEVCParameterSetPointers<T>(
        _ parameterSets: [Data],
        _ body: (UnsafePointer<UnsafePointer<UInt8>>, UnsafePointer<Int>, Int) -> T
    ) -> T {
        parameterSets[0].withUnsafeBytes { vpsBytes in
            parameterSets[1].withUnsafeBytes { spsBytes in
                parameterSets[2].withUnsafeBytes { ppsBytes in
                    let pointers: [UnsafePointer<UInt8>] = [
                        vpsBytes.bindMemory(to: UInt8.self).baseAddress!,
                        spsBytes.bindMemory(to: UInt8.self).baseAddress!,
                        ppsBytes.bindMemory(to: UInt8.self).baseAddress!
                    ]
                    let sizes = parameterSets.map(\.count)
                    return pointers.withUnsafeBufferPointer { pointerBuffer in
                        sizes.withUnsafeBufferPointer { sizeBuffer in
                            body(pointerBuffer.baseAddress!, sizeBuffer.baseAddress!, pointers.count)
                        }
                    }
                }
            }
        }
    }
}

private extension NSLock {
    func withLock<T>(_ body: () -> T) -> T {
        lock()
        defer { unlock() }
        return body()
    }
}
#endif

enum NativeStreamSelfTest {
    static func run() {
        var failures: [String] = []
        let report = NativeStreamCodecProbe.report()
        print("[STREAMER-SELFTEST] codec report \(report.summary)")
        for codec in NativeStreamVideoCodec.allCases {
            guard let capability = report.capability(for: codec) else {
                failures.append("missing codec capability \(codec.rawValue)")
                continue
            }
            print("[STREAMER-SELFTEST] \(codec.rawValue) profiles \(capability.webRTCProfileSummary.joined(separator: ", "))")
        }

        let encoder = NativeStreamInputEncoder()
        encoder.setProtocolVersion(3)
        let key = encoder.encodeKeyDown(mapping: .init(virtualKey: 0x41, scanCode: 0x001e), modifiers: 0)
        assertSelfTest(key.count == 28 && key.first == 0x23 && key[9] == 0x22, "keyboard packet v3 wrapper", failures: &failures)
        let mouse = encoder.encodeMouseMove(dx: 12, dy: -7)
        assertSelfTest(mouse.count == 34 && mouse.first == 0x23 && mouse[9] == 0x21, "mouse packet v3 wrapper", failures: &failures)
        let gamepad = encoder.encodeGamepadState(
            .init(
                controllerId: 0,
                buttons: 0x1000,
                leftTrigger: 0,
                rightTrigger: 255,
                leftStickX: 0,
                leftStickY: 0,
                rightStickX: 0,
                rightStickY: 0,
                connected: true
            ),
            bitmap: 0x0101,
            partiallyReliable: true
        )
        assertSelfTest(gamepad.count == 54 && gamepad[9] == 0x26 && gamepad[13] == 0x21, "gamepad PR wrapper", failures: &failures)

        let sampleOffer = """
        v=0
        o=- 0 0 IN IP4 0.0.0.0
        s=-
        t=0 0
        a=ri.version:3
        a=ri.partialReliableThresholdMs:45
        a=ri.enablePartiallyReliableTransferGamepad:0x0f
        m=video 9 UDP/TLS/RTP/SAVPF 96 97 98 99
        c=IN IP4 0.0.0.0
        a=rtpmap:96 H264/90000
        a=rtpmap:97 rtx/90000
        a=fmtp:97 apt=96
        a=rtpmap:98 H265/90000
        a=fmtp:98 profile-id=1
        a=rtpmap:99 rtx/90000
        a=fmtp:99 apt=98
        a=candidate:1 1 udp 2130706431 0.0.0.0 49003 typ host
        a=ice-ufrag:offerufrag
        a=ice-pwd:offerpwd
        a=fingerprint:sha-256 AA:BB
        """
        let fixed = NativeStreamSDP.fixServerIP(in: sampleOffer, serverIP: "1-2-3-4.foo")
        assertSelfTest(fixed.contains("a=candidate:1 1 udp 2130706431 1.2.3.4 49003 typ host") && fixed.contains("c=IN IP4 1.2.3.4"), "candidate IP rewrite", failures: &failures)
        assertSelfTest(NativeStreamSDP.parseInputProtocolVersion(from: sampleOffer) == 3, "input protocol parse", failures: &failures)
        assertSelfTest(NativeStreamSDP.parsePartialReliableThresholdMs(from: sampleOffer) == 45, "partial reliable parse", failures: &failures)
        let preferred = NativeStreamSDP.preferCodec(in: sampleOffer, codec: .h265, preferTenBit: false)
        assertSelfTest(preferred.contains("m=video 9 UDP/TLS/RTP/SAVPF 98 99"), "codec preference rewrite", failures: &failures)

        let sampleAnswer = """
        v=0
        o=- 0 0 IN IP4 0.0.0.0
        s=-
        t=0 0
        a=ice-ufrag:answerufrag
        a=ice-pwd:answerpassword
        a=fingerprint:sha-256 AA:BB
        """
        var profileMatrixFailures: [String] = []
        for choice in StreamSettingsResolver.resolutionChoices {
            let membershipTier: String
            switch choice.requiredPlan {
            case .free: membershipTier = "FREE"
            case .priority: membershipTier = "PRIORITY"
            case .ultimate: membershipTier = "ULTIMATE"
            }

            for codec in NativeStreamVideoCodec.allCases {
                var matrixSettings = AppSettings.default
                matrixSettings.streamPreset = .custom
                matrixSettings.preferredAspectRatio = choice.aspectRatio
                matrixSettings.preferredResolution = choice.value
                matrixSettings.preferredFPS = 60
                matrixSettings.preferredCodec = codec.rawValue
                matrixSettings.normalizeStreamDefaults()

                let profile = StreamSettingsResolver.profile(
                    for: matrixSettings,
                    nativeBounds: .zero,
                    nativeScale: 1,
                    userInterfaceIdiom: .phone,
                    membershipTier: membershipTier
                )
                let nvstSDP = NativeStreamSDP.buildNvstSDP(
                    offerSDP: sampleOffer,
                    localAnswerSDP: sampleAnswer,
                    profile: profile,
                    settings: matrixSettings,
                    codec: codec
                )
                let resolutionIsExact = profile.resolutionString == choice.value
                let carriesViewport = nvstSDP.contains("a=video.clientViewportWd:\(profile.width)")
                    && nvstSDP.contains("a=video.clientViewportHt:\(profile.height)")
                let carriesFPS = nvstSDP.contains("a=video.maxFPS:\(profile.fps)")
                if !resolutionIsExact || !carriesViewport || !carriesFPS {
                    profileMatrixFailures.append("\(codec.rawValue) \(choice.value)")
                }
            }
        }
        assertSelfTest(
            profileMatrixFailures.isEmpty,
            "codec and resolution request matrix \(NativeStreamVideoCodec.allCases.count * StreamSettingsResolver.resolutionChoices.count) profiles",
            failures: &failures
        )
        if !profileMatrixFailures.isEmpty {
            failures.append("invalid request profiles: \(profileMatrixFailures.joined(separator: ", "))")
        }

        var h265Settings = AppSettings.default
        h265Settings.preferredCodec = "H265"
        h265Settings.preferredFPS = 120
        h265Settings.maxBitrateMbps = 100
        let h265UnsafeReport = NativeStreamCodecReport(capabilities: [
            NativeStreamCodecCapability(codec: .h264, videoToolboxHardwareDecode: true, webRTCSupported: true, webRTCProfileSummary: ["H264"]),
            NativeStreamCodecCapability(codec: .h265, videoToolboxHardwareDecode: true, webRTCSupported: false, webRTCProfileSummary: []),
            NativeStreamCodecCapability(codec: .av1, videoToolboxHardwareDecode: true, webRTCSupported: true, webRTCProfileSummary: ["AV1"])
        ])
        let adjusted = NativeStreamLaunchSettingsResolver.resolve(h265Settings, codecReport: h265UnsafeReport).settings
        assertSelfTest(
            adjusted.preferredCodec == "H264"
                && adjusted.preferredFPS == 60
                && adjusted.maxBitrateMbps == 75,
            "unsafe H265 prelaunch safe profile",
            failures: &failures
        )

        if failures.isEmpty {
            print("[STREAMER-SELFTEST] PASS")
        } else {
            print("[STREAMER-SELFTEST] FAIL \(failures.joined(separator: "; "))")
        }
    }

    private static func assertSelfTest(_ condition: Bool, _ message: String, failures: inout [String]) {
        if condition {
            print("[STREAMER-SELFTEST] ok \(message)")
        } else {
            failures.append(message)
        }
    }
}

// Read source precision from the SPS rather than the requested HDR setting.
// SDR Main10 also needs P010; an HDR request must not relabel an 8-bit source.
enum NativeStreamHEVCOutput {
    static func pixelFormat(sps: Data) -> OSType {
        bitDepth(sps: sps) == 10
            ? kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange
            : kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange
    }

    static func bitDepth(sps: Data) -> Int? {
        guard sps.count > 2 else { return nil }
        var bytes: [UInt8] = []
        var zeros = 0
        for byte in sps.dropFirst(2) {
            if zeros >= 2 && byte == 3 { zeros = 0; continue }
            bytes.append(byte)
            zeros = byte == 0 ? zeros + 1 : 0
        }
        var reader = Bits(bytes: bytes)
        guard reader.read(4) != nil, let layers = reader.read(3),
              reader.read(1) != nil, reader.read(96) != nil else { return nil }
        var profiles: [Bool] = [], levels: [Bool] = []
        for _ in 0..<layers {
            guard let profile = reader.read(1), let level = reader.read(1) else { return nil }
            profiles.append(profile == 1); levels.append(level == 1)
        }
        if layers > 0 && reader.read((8 - layers) * 2) == nil { return nil }
        for i in 0..<layers {
            if profiles[i] && reader.read(88) == nil { return nil }
            if levels[i] && reader.read(8) == nil { return nil }
        }
        guard reader.ue() != nil, let chroma = reader.ue(), chroma <= 3 else { return nil }
        if chroma == 3 && reader.read(1) == nil { return nil }
        guard reader.ue() != nil, reader.ue() != nil, let window = reader.read(1) else { return nil }
        if window == 1 {
            for _ in 0..<4 { guard reader.ue() != nil else { return nil } }
        }
        guard let luma = reader.ue(), let chromaDepth = reader.ue(),
              luma == chromaDepth, luma <= 8 else { return nil }
        return 8 + luma
    }

    private struct Bits {
        let bytes: [UInt8]
        var offset = 0
        mutating func read(_ count: Int) -> Int? {
            guard count >= 0, offset + count <= bytes.count * 8 else { return nil }
            var value = 0
            for _ in 0..<count {
                // Long profile fields are skipped; only small fields need their value.
                if count <= 32 { value = (value << 1) | Int((bytes[offset / 8] >> (7 - offset % 8)) & 1) }
                offset += 1
            }
            return value
        }
        mutating func ue() -> Int? {
            var zeros = 0
            while true {
                guard let bit = read(1) else { return nil }
                if bit == 1 { break }
                zeros += 1
                if zeros > 30 { return nil }
            }
            guard let suffix = read(zeros) else { return nil }
            return (1 << zeros) - 1 + suffix
        }
    }
}
