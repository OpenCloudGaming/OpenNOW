//  What the bitstream declares about itself and which output surface to ask VideoToolbox for.
//

import CoreMedia
import CoreVideo
import Foundation
import VideoToolbox

@available(iOS 17.0, *)
extension NvstVideoToolboxDecoder {

    /// What the bitstream's parameter sets declare about sample depth and chroma layout. Read from
    /// the `hvcC` record VideoToolbox builds out of the SPS, so it is the decoder's own view of the
    /// stream rather than what the session negotiation asked for.
    public struct BitstreamFormat: Equatable, Sendable {
        public enum Chroma: Int, Sendable {
            case monochrome = 0
            case yuv420 = 1
            case yuv422 = 2
            case yuv444 = 3
        }
        public var bitDepth = 8
        public var chroma = Chroma.yuv420

        public var isTenBit: Bool { bitDepth > 8 }
        public var summary: String {
            let layout: String = switch chroma {
            case .monochrome: "4:0:0"
            case .yuv420: "4:2:0"
            case .yuv422: "4:2:2"
            case .yuv444: "4:4:4"
            }
            return "\(bitDepth)-bit \(layout)"
        }
    }

    /// Reads depth and chroma layout out of the HEVC decoder configuration record (`hvcC`, ISO
    /// 14496-15 §8.3.3.1): byte 16 carries `chromaFormat` in its low two bits and byte 17
    /// `bitDepthLumaMinus8` in its low three. AV1 reads the same fields out of the `av1C` record
    /// the description was built with. H.264 sessions on this service are 8-bit 4:2:0 — the
    /// 10-bit and 4:4:4 tiers are only offered on HEVC and AV1 — so `avcC` is not parsed.
    static func bitstreamFormat(from description: CMFormatDescription, codec: NVSTVideoCodec) -> BitstreamFormat {
        let atom: String
        switch codec {
        case .hevc: atom = "hvcC"
        case .av1: atom = "av1C"
        case .h264: return BitstreamFormat()
        }
        guard let atoms = CMFormatDescriptionGetExtension(description, extensionKey: kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms) as? [String: Any],
              let record = atoms[atom] as? Data else {
            return BitstreamFormat()
        }
        return codec == .av1 ? bitstreamFormat(av1C: record) : bitstreamFormat(hvcC: record)
    }

    /// Depth and chroma layout out of an `av1C` record (AV1-ISOBMFF §2.3.2): byte 2 packs
    /// `seq_tier_0`, `high_bitdepth`, `twelve_bit`, `monochrome` and the subsampling flags,
    /// exactly as `NvstAv1Obu.codecConfigurationRecord` wrote them.
    static func bitstreamFormat(av1C record: Data) -> BitstreamFormat {
        guard record.count >= 3 else { return BitstreamFormat() }
        let flags = record[record.startIndex + 2]
        var format = BitstreamFormat()
        let twelveBit = flags & 0x20 != 0
        format.bitDepth = twelveBit ? 12 : (flags & 0x40 != 0 ? 10 : 8)
        let subsamplingX = flags & 0x08 != 0
        let subsamplingY = flags & 0x04 != 0
        if flags & 0x10 != 0 {
            format.chroma = .monochrome
        } else if subsamplingX && subsamplingY {
            format.chroma = .yuv420
        } else if !subsamplingX && !subsamplingY {
            format.chroma = .yuv444
        } else {
            format.chroma = .yuv422
        }
        return format
    }

    static func bitstreamFormat(hvcC record: Data) -> BitstreamFormat {
        guard record.count >= 19 else { return BitstreamFormat() }
        let bytes = [UInt8](record)
        var format = BitstreamFormat()
        format.chroma = BitstreamFormat.Chroma(rawValue: Int(bytes[16] & 0x3)) ?? .yuv420
        format.bitDepth = 8 + Int(bytes[17] & 0x7)
        return format
    }

    /// Strict 4:4:4 accepts only matching 10-bit bi-planar surfaces. Full/video range
    /// are both understood by the renderer; neither changes chroma resolution or depth.
    static func validate444Bitstream(_ format: BitstreamFormat) throws {
        guard format.bitDepth == 10, format.chroma == .yuv444 else {
            throw DecoderError.requested444NotDelivered(format.summary)
        }
    }

    static func preferredOutputPixelFormats(for format: BitstreamFormat, requiresTenBit444: Bool = false) -> [OSType] {
        if requiresTenBit444 {
            return [kCVPixelFormatType_444YpCbCr10BiPlanarFullRange, kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange]
        }
        let fallback = kCVPixelFormatType_420YpCbCr8BiPlanarFullRange
        var preferred: [OSType] = []
        switch (format.chroma, format.isTenBit) {
        case (.yuv444, true): preferred = [kCVPixelFormatType_444YpCbCr10BiPlanarFullRange, kCVPixelFormatType_444YpCbCr10BiPlanarVideoRange, kCVPixelFormatType_420YpCbCr10BiPlanarFullRange]
        case (.yuv444, false): preferred = [kCVPixelFormatType_444YpCbCr8BiPlanarFullRange]
        case (.yuv422, true): preferred = [kCVPixelFormatType_422YpCbCr10BiPlanarFullRange, kCVPixelFormatType_420YpCbCr10BiPlanarFullRange]
        case (.yuv422, false): preferred = [kCVPixelFormatType_422YpCbCr8BiPlanarFullRange]
        case (_, true): preferred = [kCVPixelFormatType_420YpCbCr10BiPlanarFullRange]
        default: preferred = []
        }
        return format.isTenBit ? preferred : preferred + [fallback]
    }

    /// Offer both compatible 4:4:4 ranges together so VideoToolbox can choose its
    /// preferred output, rather than forcing a range conversion. Single-format requests
    /// remain a fallback for decoders that decline a Core Video format array.
    static func outputPixelFormatRequests(for format: BitstreamFormat, requiresTenBit444: Bool,
                                         sourceIsFullRange: Bool) -> [[OSType]] {
        var candidates = preferredOutputPixelFormats(for: format, requiresTenBit444: requiresTenBit444)
        if requiresTenBit444 {
            if !sourceIsFullRange { candidates.reverse() }
            return [candidates] + candidates.map { [$0] }
        }
        return candidates.map { [$0] }
    }

    static func pixelFormatName(_ format: OSType) -> String {
        guard format != 0 else { return "-" }
        let bytes = [UInt8((format >> 24) & 0xff), UInt8((format >> 16) & 0xff), UInt8((format >> 8) & 0xff), UInt8(format & 0xff)]
        return String(bytes: bytes, encoding: .ascii) ?? String(format: "0x%08x", format)
    }

    func makeFormatDescription(_ sets: NvstElementaryStream.ParameterSets) throws -> CMVideoFormatDescription {
        if codec == .av1 { return try makeAv1FormatDescription(sets) }
        let ordered = sets.ordered
        guard !ordered.isEmpty else { throw DecoderError.missingParameterSets }
        // One contiguous allocation so the pointer array stays valid for the whole call;
        // taking addresses out of per-element `withUnsafeBufferPointer` closures would dangle.
        let sizes = ordered.map(\.count)
        let total = sizes.reduce(0, +)
        let storage = UnsafeMutableBufferPointer<UInt8>.allocate(capacity: total)
        defer { storage.deallocate() }
        var offset = 0
        var pointers: [UnsafePointer<UInt8>] = []
        for set in ordered {
            set.copyBytes(to: storage.baseAddress!.advanced(by: offset), count: set.count)
            pointers.append(UnsafePointer(storage.baseAddress!.advanced(by: offset)))
            offset += set.count
        }

        var description: CMFormatDescription?
        let status: OSStatus = pointers.withUnsafeBufferPointer { pointerBuffer in
            sizes.withUnsafeBufferPointer { sizeBuffer in
                switch codec {
                case .hevc:
                    CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                        allocator: kCFAllocatorDefault,
                        parameterSetCount: pointerBuffer.count,
                        parameterSetPointers: pointerBuffer.baseAddress!,
                        parameterSetSizes: sizeBuffer.baseAddress!,
                        nalUnitHeaderLength: 4,
                        extensions: nil,
                        formatDescriptionOut: &description
                    )
                default:
                    CMVideoFormatDescriptionCreateFromH264ParameterSets(
                        allocator: kCFAllocatorDefault,
                        parameterSetCount: pointerBuffer.count,
                        parameterSetPointers: pointerBuffer.baseAddress!,
                        parameterSetSizes: sizeBuffer.baseAddress!,
                        nalUnitHeaderLength: 4,
                        formatDescriptionOut: &description
                    )
                }
            }
        }
        guard status == noErr, let description else { throw DecoderError.formatDescriptionFailed(status) }
        let format = Self.bitstreamFormat(from: description, codec: codec)
        statsLock.lock()
        currentBitstreamFormat = format
        statsLock.unlock()
        // The seat's parameter sets, verbatim, once per format description. Whether the decoder
        // may hold frames for reordering is written in the SPS (`sps_max_num_reorder_pics`, and
        // the VUI's bitstream restriction), and that decides how long a decoded frame waits inside
        // VideoToolbox before this app sees it.
        let hex = sets.ordered.map { data in data.map { String(format: "%02x", $0) }.joined() }.joined(separator: " ")
        onDecodeFailure?(0, "NVST parameter sets \(format.summary): \(hex)")
        return description
    }

    /// VideoToolbox takes AV1 as a plain `av01` format description whose sample-description
    /// extension carries the `av1C` record — there is no parameter-set factory the way H.264 and
    /// HEVC have. The record comes from the stream's own sequence header OBU, so the description
    /// always matches the bitstream rather than the negotiation's assumptions.
    private func makeAv1FormatDescription(_ sets: NvstElementaryStream.ParameterSets) throws -> CMVideoFormatDescription {
        // `configuration` is the sequence header OBU in its wire form (size field included);
        // re-walking it locates the payload for the record's flag bytes.
        guard let configuration = sets.ordered.first,
              let unit = NvstAv1Obu.units(in: configuration)?.first,
              unit.type == NvstAv1Obu.sequenceHeaderType,
              let header = NvstAv1Obu.parseSequenceHeader(configuration.subdata(in: unit.payloadOffset..<(unit.payloadOffset + unit.payloadLength))) else {
            throw DecoderError.missingParameterSets
        }
        let record = NvstAv1Obu.codecConfigurationRecord(header: header, configurationOBU: configuration)
        var extensions: [CFString: Any] = [
            kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms: ["av1C": record],
        ]
        extensions[kCMFormatDescriptionExtension_FullRangeVideo] = header.fullRange
        if header.colorPrimaries == 9 { extensions[kCMFormatDescriptionExtension_ColorPrimaries] = kCMFormatDescriptionColorPrimaries_ITU_R_2020 }
        else if header.colorPrimaries == 1 { extensions[kCMFormatDescriptionExtension_ColorPrimaries] = kCMFormatDescriptionColorPrimaries_ITU_R_709_2 }
        if header.transferCharacteristics == 16 { extensions[kCMFormatDescriptionExtension_TransferFunction] = kCMFormatDescriptionTransferFunction_SMPTE_ST_2084_PQ }
        else if header.transferCharacteristics == 18 { extensions[kCMFormatDescriptionExtension_TransferFunction] = kCMFormatDescriptionTransferFunction_ITU_R_2100_HLG }
        else if [1,6,14,15].contains(header.transferCharacteristics) { extensions[kCMFormatDescriptionExtension_TransferFunction] = kCMFormatDescriptionTransferFunction_ITU_R_709_2 }
        if header.matrixCoefficients == 9 { extensions[kCMFormatDescriptionExtension_YCbCrMatrix] = kCMFormatDescriptionYCbCrMatrix_ITU_R_2020 }
        else if header.matrixCoefficients == 1 { extensions[kCMFormatDescriptionExtension_YCbCrMatrix] = kCMFormatDescriptionYCbCrMatrix_ITU_R_709_2 }
        var description: CMFormatDescription?
        let status = CMVideoFormatDescriptionCreate(
            allocator: kCFAllocatorDefault,
            codecType: kCMVideoCodecType_AV1,
            width: Int32(clamping: header.maxFrameWidth),
            height: Int32(clamping: header.maxFrameHeight),
            extensions: extensions as CFDictionary,
            formatDescriptionOut: &description
        )
        guard status == noErr, let description else { throw DecoderError.formatDescriptionFailed(status) }
        let format = Self.bitstreamFormat(av1C: record)
        statsLock.lock()
        currentBitstreamFormat = format
        statsLock.unlock()
        let hex = configuration.map { String(format: "%02x", $0) }.joined()
        onDecodeFailure?(0, "NVST parameter sets \(format.summary): \(hex)")
        return description
    }

    func makeSampleBuffer(sample: Data,
                                  formatDescription: CMVideoFormatDescription,
                                  presentationTime: CMTime) throws -> CMSampleBuffer {
        var blockBuffer: CMBlockBuffer?
        // The sample is already a contiguous `Data`; copying it into an `[UInt8]` first was a whole
        // extra pass over a 5K access unit for nothing. `CMBlockBufferReplaceDataBytes` copies from
        // whatever pointer it is given.
        let sampleCount = sample.count
        var status = CMBlockBufferCreateWithMemoryBlock(
            allocator: kCFAllocatorDefault,
            memoryBlock: nil,
            blockLength: sampleCount,
            blockAllocator: kCFAllocatorDefault,
            customBlockSource: nil,
            offsetToData: 0,
            dataLength: sampleCount,
            flags: 0,
            blockBufferOut: &blockBuffer
        )
        guard status == noErr, let blockBuffer else { throw DecoderError.blockBufferFailed(status) }
        status = sample.withUnsafeBytes { raw in
            guard let base = raw.baseAddress else { return kCMBlockBufferBadPointerParameterErr }
            return CMBlockBufferReplaceDataBytes(with: base, blockBuffer: blockBuffer, offsetIntoDestination: 0, dataLength: sampleCount)
        }
        guard status == noErr else { throw DecoderError.blockBufferFailed(status) }

        var sampleBuffer: CMSampleBuffer?
        var sampleSize = sampleCount
        var timing = CMSampleTimingInfo(
            duration: .invalid,
            presentationTimeStamp: presentationTime,
            decodeTimeStamp: .invalid
        )
        status = CMSampleBufferCreateReady(
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
        guard status == noErr, let sampleBuffer else { throw DecoderError.sampleBufferFailed(status) }
        return sampleBuffer
    }

    static func tearDown(_ session: VTDecompressionSession?) {
        guard let session else { return }
        VTDecompressionSessionWaitForAsynchronousFrames(session)
        VTDecompressionSessionInvalidate(session)
    }

    static func milliseconds(from start: UInt64, to end: UInt64) -> Double {
        end > start ? Double(end - start) / 1_000_000 : 0
    }
}
