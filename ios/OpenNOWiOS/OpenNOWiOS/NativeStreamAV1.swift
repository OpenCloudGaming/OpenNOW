import Foundation
import CoreMedia
import CoreVideo
import VideoToolbox

/// AV1 section 5.5 and the AV1 ISOBMFF codec configuration record.
/// Parses configuration only; compressed frames remain with VideoToolbox.
struct NativeStreamAV1Configuration: Equatable {
    let width: Int
    let height: Int
    let profile: Int
    let level: Int
    let tier: Int
    let bitDepth: Int
    let monochrome: Int
    let subsamplingX: Int
    let subsamplingY: Int
    let chromaPosition: Int
    let primaries: Int
    let transfer: Int
    let matrix: Int
    let fullRange: Bool
    let sequenceOBU: Data

    var codecConfiguration: Data {
        var bytes = Data([
            0x81, UInt8(profile << 5 | level),
            UInt8(tier << 7 | (bitDepth > 8 ? 1 : 0) << 6 | (bitDepth == 12 ? 1 : 0) << 5
                  | monochrome << 4 | subsamplingX << 3 | subsamplingY << 2 | chromaPosition), 0
        ])
        bytes.append(sequenceOBU)
        return bytes
    }

    static func parse(_ frame: Data) throws -> NativeStreamAV1Configuration? {
        // Inspect OBU headers without copying the entire compressed frame. Only
        // the small sequence header is copied when it is actually present.
        try frame.withUnsafeBytes { raw in
            try parseHeaders(raw.bindMemory(to: UInt8.self))
        }
    }

    private static func parseHeaders(_ bytes: UnsafeBufferPointer<UInt8>) throws -> NativeStreamAV1Configuration? {
        var offset = 0
        while offset < bytes.count {
            let start = offset
            let header = bytes[offset]; offset += 1
            guard header & 0x81 == 0 else { throw ParseError.malformed }
            if header & 4 != 0 {
                guard offset < bytes.count, bytes[offset] & 7 == 0 else { throw ParseError.malformed }
                offset += 1
            }
            let headerEnd = offset
            var count = bytes.count - offset
            if header & 2 != 0 {
                count = 0
                var terminated = false
                for shift in stride(from: 0, through: 49, by: 7) {
                    guard offset < bytes.count else { throw ParseError.malformed }
                    let value = bytes[offset]; offset += 1
                    count |= Int(value & 127) << shift
                    if value & 128 == 0 { terminated = true; break }
                }
                guard terminated else { throw ParseError.malformed }
            }
            guard count <= bytes.count - offset else { throw ParseError.malformed }
            if (header >> 3) & 15 == 1 {
                var obu = Data(bytes[start..<headerEnd])
                obu[0] |= 2
                var remaining = count
                repeat {
                    obu.append(UInt8(remaining & 127) | (remaining > 127 ? 128 : 0))
                    remaining >>= 7
                } while remaining > 0
                obu.append(contentsOf: bytes[offset..<(offset + count)])
                return try parseSequence(Data(bytes[offset..<(offset + count)]), obu: obu)
            }
            offset += count
        }
        return nil
    }

    enum ParseError: Error { case malformed, unsupported }

    private static func parseSequence(_ data: Data, obu: Data) throws -> Self {
        var bits = Bits(data)
        let profile = try bits.read(3)
        guard profile <= 2 else { throw ParseError.malformed }
        _ = try bits.read(1) // still_picture
        let reduced = try bits.read(1) == 1
        var level = 0, tier = 0
        if reduced {
            level = try bits.read(5)
        } else {
            var model = false, delayBits = 0
            if try bits.read(1) == 1 {
                try bits.skip(64)
                if try bits.read(1) == 1 { try bits.skipUVLC() }
                model = try bits.read(1) == 1
                if model {
                    delayBits = try bits.read(5) + 1
                    try bits.skip(42)
                }
            }
            let displayDelay = try bits.read(1) == 1
            let points = try bits.read(5) + 1
            for index in 0..<points {
                try bits.skip(12)
                let nextLevel = try bits.read(5)
                let nextTier = nextLevel > 7 ? try bits.read(1) : 0
                if index == 0 { level = nextLevel; tier = nextTier }
                if model, try bits.read(1) == 1 { try bits.skip(delayBits * 2 + 1) }
                if displayDelay, try bits.read(1) == 1 { try bits.skip(4) }
            }
        }
        let widthBits = try bits.read(4) + 1
        let heightBits = try bits.read(4) + 1
        let width = try bits.read(widthBits) + 1
        let height = try bits.read(heightBits) + 1
        if !reduced, try bits.read(1) == 1 { try bits.skip(7) }
        try bits.skip(3)
        if !reduced {
            try bits.skip(4)
            let orderHint = try bits.read(1) == 1
            if orderHint { try bits.skip(2) }
            let chooseScreen = try bits.read(1) == 1
            let screenTools = chooseScreen ? 2 : try bits.read(1)
            if screenTools > 0, try bits.read(1) == 0 { try bits.skip(1) }
            if orderHint { try bits.skip(3) }
        }
        try bits.skip(3)
        let highDepth = try bits.read(1) == 1
        let twelve = profile == 2 && highDepth ? try bits.read(1) == 1 : false
        let depth = twelve ? 12 : highDepth ? 10 : 8
        let mono = profile == 1 ? 0 : try bits.read(1)
        let hasColor = try bits.read(1) == 1
        let primaries = hasColor ? try bits.read(8) : 2
        let transfer = hasColor ? try bits.read(8) : 2
        let matrix = hasColor ? try bits.read(8) : 2
        var fullRange = false, x = 1, y = 1, chroma = 0
        if mono == 1 {
            fullRange = try bits.read(1) == 1
        } else {
            if primaries == 1 && transfer == 13 && matrix == 0 {
                fullRange = true; x = 0; y = 0
            } else {
                fullRange = try bits.read(1) == 1
                if profile == 1 { x = 0; y = 0 }
                if profile == 2 {
                    x = depth == 12 ? try bits.read(1) : 1
                    y = depth == 12 && x == 1 ? try bits.read(1) : 0
                }
                if x == 1 && y == 1 { chroma = try bits.read(2) }
            }
            try bits.skip(1)
        }
        try bits.skip(1) // film_grain_params_present
        // Validate trailing_bits rather than accepting truncated configuration.
        guard try bits.read(1) == 1 else { throw ParseError.malformed }
        while bits.remaining > 0 {
            guard try bits.read(1) == 0 else { throw ParseError.malformed }
        }
        return Self(width: width, height: height, profile: profile, level: level, tier: tier,
                    bitDepth: depth, monochrome: mono, subsamplingX: x, subsamplingY: y,
                    chromaPosition: chroma, primaries: primaries, transfer: transfer,
                    matrix: matrix, fullRange: fullRange, sequenceOBU: obu)
    }

    private struct Bits {
        let bytes: [UInt8]
        var position = 0
        init(_ data: Data) { bytes = Array(data) }
        var remaining: Int { bytes.count * 8 - position }
        mutating func read(_ count: Int) throws -> Int {
            guard count >= 0, count <= 32, count <= remaining else { throw ParseError.malformed }
            var value = 0
            for _ in 0..<count {
                value = value << 1 | Int((bytes[position / 8] >> (7 - position % 8)) & 1)
                position += 1
            }
            return value
        }
        mutating func skip(_ count: Int) throws {
            guard count >= 0, count <= remaining else { throw ParseError.malformed }
            position += count
        }
        mutating func skipUVLC() throws {
            var leading = 0
            while try read(1) == 0 {
                leading += 1
                if leading == 32 { return }
            }
            try skip(leading)
        }
    }
}

/// Shared with the macOS validation probe so real encoded samples can exercise
/// the same configuration/session creation as the iOS WebRTC adapter.
final class NativeStreamAV1HardwareDecoder {
    private let lock = NSRecursiveLock()
    private var session: VTDecompressionSession?
    private var format: CMVideoFormatDescription?
    private var configuration: NativeStreamAV1Configuration?
    private let admission = NativeStreamDecodeAdmission(maximumInFlight: 2)

    static var isSupported: Bool { VTIsHardwareDecodeSupported(kCMVideoCodecType_AV1) }

    deinit { release() }

    func release() {
        lock.lock(); defer { lock.unlock() }
        if let session { VTDecompressionSessionInvalidate(session) }
        session = nil; format = nil; configuration = nil
    }

    func decode(_ payload: Data, timestamp: UInt32, asynchronous: Bool = false,
                output: @escaping (OSStatus, CVPixelBuffer?) -> Void) -> OSStatus {
        lock.lock(); defer { lock.unlock() }
        do {
            if let config = try NativeStreamAV1Configuration.parse(payload), config != configuration {
                let status = configure(config)
                guard status == noErr else { return status }
            }
        } catch {
            NSLog("[OpenNOW] AV1 configuration rejected: %@", String(describing: error))
            return kVTVideoDecoderBadDataErr
        }
        guard let session, let format, !payload.isEmpty else { return kVTVideoDecoderBadDataErr }
        var block: CMBlockBuffer?
        var status = CMBlockBufferCreateWithMemoryBlock(allocator: kCFAllocatorDefault,
            memoryBlock: nil, blockLength: payload.count, blockAllocator: kCFAllocatorDefault,
            customBlockSource: nil, offsetToData: 0, dataLength: payload.count, flags: 0,
            blockBufferOut: &block)
        guard status == noErr, let block else { return status }
        status = payload.withUnsafeBytes { bytes in
            CMBlockBufferReplaceDataBytes(with: bytes.baseAddress!, blockBuffer: block,
                                          offsetIntoDestination: 0, dataLength: payload.count)
        }
        guard status == noErr else { return status }
        var timing = CMSampleTimingInfo(duration: .invalid,
            presentationTimeStamp: CMTime(value: Int64(timestamp), timescale: 90_000), decodeTimeStamp: .invalid)
        var size = payload.count
        var sample: CMSampleBuffer?
        status = CMSampleBufferCreateReady(allocator: kCFAllocatorDefault, dataBuffer: block,
            formatDescription: format, sampleCount: 1, sampleTimingEntryCount: 1,
            sampleTimingArray: &timing, sampleSizeEntryCount: 1, sampleSizeArray: &size,
            sampleBufferOut: &sample)
        guard status == noErr, let sample else { return status }
        // Bound hardware submissions without dropping compressed reference frames.
        // Async output lets WebRTC schedule the next frame while hardware decodes.
        guard let permit = admission.acquire() else { return kVTInvalidSessionErr }
        var infoFlags = VTDecodeInfoFlags()
        let flags: VTDecodeFrameFlags = asynchronous ? ._EnableAsynchronousDecompression : []
        let result = VTDecompressionSessionDecodeFrame(session, sampleBuffer: sample, flags: flags,
            infoFlagsOut: &infoFlags) { status, _, buffer, _, _ in
                defer { permit.complete() }
                output(status, buffer)
            }
        if result != noErr || infoFlags.contains(.frameDropped) { permit.complete() }
        return result
    }

    private func configure(_ config: NativeStreamAV1Configuration) -> OSStatus {
        guard Self.isSupported, config.profile == 0, config.bitDepth <= 10,
              config.subsamplingX == 1, config.subsamplingY == 1 else {
            return kVTCouldNotFindVideoDecoderErr
        }
        var extensions: [CFString: Any] = [
            kCMFormatDescriptionExtension_FormatName: "av01",
            kCMFormatDescriptionExtension_Depth: 24,
            kCMFormatDescriptionExtension_FullRangeVideo: config.fullRange,
            kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms: ["av1C": config.codecConfiguration],
            "BitsPerComponent" as CFString: config.bitDepth
        ]
        if config.primaries == 9 {
            extensions[kCMFormatDescriptionExtension_ColorPrimaries] = kCMFormatDescriptionColorPrimaries_ITU_R_2020
        } else if config.primaries == 1 {
            extensions[kCMFormatDescriptionExtension_ColorPrimaries] = kCMFormatDescriptionColorPrimaries_ITU_R_709_2
        }
        switch config.transfer {
        case 16: extensions[kCMFormatDescriptionExtension_TransferFunction] = kCMFormatDescriptionTransferFunction_SMPTE_ST_2084_PQ
        case 18: extensions[kCMFormatDescriptionExtension_TransferFunction] = kCMFormatDescriptionTransferFunction_ITU_R_2100_HLG
        case 1, 6, 14, 15: extensions[kCMFormatDescriptionExtension_TransferFunction] = kCMFormatDescriptionTransferFunction_ITU_R_709_2
        default: break
        }
        if config.matrix == 9 {
            extensions[kCMFormatDescriptionExtension_YCbCrMatrix] = kCMFormatDescriptionYCbCrMatrix_ITU_R_2020
        } else if config.matrix == 1 {
            extensions[kCMFormatDescriptionExtension_YCbCrMatrix] = kCMFormatDescriptionYCbCrMatrix_ITU_R_709_2
        }
        var nextFormat: CMVideoFormatDescription?
        var status = CMVideoFormatDescriptionCreate(allocator: kCFAllocatorDefault,
            codecType: kCMVideoCodecType_AV1, width: Int32(config.width), height: Int32(config.height),
            extensions: extensions as CFDictionary, formatDescriptionOut: &nextFormat)
        guard status == noErr, let nextFormat else { return status }
        var specification: [CFString: Any] = [:]
        if #available(iOS 17.0, macOS 10.9, *) {
            specification[kVTVideoDecoderSpecification_RequireHardwareAcceleratedVideoDecoder] = true
        }
        let pixelFormat: OSType = config.bitDepth == 10
            ? (config.fullRange ? kCVPixelFormatType_420YpCbCr10BiPlanarFullRange : kCVPixelFormatType_420YpCbCr10BiPlanarVideoRange)
            : (config.fullRange ? kCVPixelFormatType_420YpCbCr8BiPlanarFullRange : kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange)
        let attributes: [CFString: Any] = [kCVPixelBufferPixelFormatTypeKey: pixelFormat,
            kCVPixelBufferMetalCompatibilityKey: true, kCVPixelBufferIOSurfacePropertiesKey: [:]]
        var nextSession: VTDecompressionSession?
        status = VTDecompressionSessionCreate(allocator: kCFAllocatorDefault,
            formatDescription: nextFormat, decoderSpecification: specification as CFDictionary,
            imageBufferAttributes: attributes as CFDictionary, outputCallback: nil,
            decompressionSessionOut: &nextSession)
        guard status == noErr, let nextSession else { return status }
        VTSessionSetProperty(nextSession, key: kVTDecompressionPropertyKey_RealTime, value: kCFBooleanTrue)
        if let session { VTDecompressionSessionInvalidate(session) }
        session = nextSession; format = nextFormat; configuration = config
        NSLog("[OpenNOW] VideoToolbox AV1 ready %dx%d depth=%d primaries=%d transfer=%d matrix=%d pixelFormat=%u",
              config.width, config.height, config.bitDepth, config.primaries, config.transfer, config.matrix, pixelFormat)
        return noErr
    }
}

#if canImport(WebRTC) && os(iOS)
@preconcurrency import WebRTC

final class NativeStreamAV1VideoDecoder: NSObject, RTCVideoDecoder {
    private let hardware = NativeStreamAV1HardwareDecoder()
    private let lock = NSLock()
    private var callback: RTCVideoDecoderCallback?
    func setCallback(_ callback: @escaping RTCVideoDecoderCallback) {
        lock.lock(); self.callback = callback; lock.unlock()
    }
    func startDecode(withNumberOfCores numberOfCores: Int32) -> Int { 0 }
    func release() -> Int {
        hardware.release()
        lock.lock(); callback = nil; lock.unlock()
        return 0
    }
    func implementationName() -> String { "OpenNOW-VideoToolbox-AV1" }
    func decode(_ image: RTCEncodedImage, missingFrames: Bool,
                codecSpecificInfo info: RTCCodecSpecificInfo?, renderTimeMs: Int64) -> Int {
        let timestamp = image.timeStamp
        let timestampNs = image.captureTimeMs > 0 ? image.captureTimeMs * 1_000_000
            : Int64(timestamp) * 1_000_000_000 / 90_000
        let rotation = image.rotation
        return Int(hardware.decode(image.buffer, timestamp: timestamp, asynchronous: true) { [weak self] status, buffer in
            guard let self else { return }
            guard status == noErr, let buffer else {
                if status != noErr { NSLog("[OpenNOW] AV1 hardware output failed status=%d", status) }
                return
            }
            self.lock.lock(); let callback = self.callback; self.lock.unlock()
            let frame = RTCVideoFrame(buffer: RTCCVPixelBuffer(pixelBuffer: buffer),
                rotation: rotation, timeStampNs: timestampNs)
            frame.timeStamp = Int32(bitPattern: timestamp)
            callback?(frame)
        })
    }
}
#endif

/// Backpressure for asynchronous hardware work. Every admission is returned once,
/// including immediate decode errors, dropped outputs and callback cancellation.
final class NativeStreamDecodeAdmission {
    private let semaphore: DispatchSemaphore
    init(maximumInFlight: Int) {
        precondition(maximumInFlight > 0)
        semaphore = DispatchSemaphore(value: maximumInFlight)
    }
    func acquire(timeout: DispatchTime = .distantFuture) -> Permit? {
        guard semaphore.wait(timeout: timeout) == .success else { return nil }
        return Permit(semaphore: semaphore)
    }
    final class Permit {
        private let lock = NSLock()
        private let semaphore: DispatchSemaphore
        private var completed = false
        init(semaphore: DispatchSemaphore) { self.semaphore = semaphore }
        func complete() {
            lock.lock()
            let shouldSignal = !completed
            completed = true
            lock.unlock()
            if shouldSignal { semaphore.signal() }
        }
        deinit { complete() }
    }
}
