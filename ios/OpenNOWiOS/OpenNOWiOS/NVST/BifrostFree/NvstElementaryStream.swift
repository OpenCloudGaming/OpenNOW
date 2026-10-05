import Foundation

/// Elementary-stream shaping between the NVST receive path and VideoToolbox.
///
/// The wire carries Annex-B (start-code delimited) access units; VideoToolbox wants AVCC/HVCC
/// (length-prefixed samples) plus a format description built from the parameter sets. This type
/// does that conversion and nothing else, so it can be verified without a decode session.
public enum NvstElementaryStream {
    public struct ParameterSets: Equatable, Sendable {
        /// H.264: SPS. HEVC: SPS. AV1: the sequence header OBU in its size-less configuration
        /// form (`NvstAv1Obu.configurationOBU`), which is what the av1C record is built from.
        public var sequenceParameterSets: [Data] = []
        /// H.264: PPS. HEVC: PPS. AV1 carries nothing here.
        public var pictureParameterSets: [Data] = []
        /// HEVC only: VPS.
        public var videoParameterSets: [Data] = []

        /// H.264/HEVC completeness: SPS + PPS. AV1 has no picture parameter set — see
        /// `isComplete(for:)`, which is what the decoder uses.
        public var isComplete: Bool {
            !sequenceParameterSets.isEmpty && !pictureParameterSets.isEmpty
        }

        /// Whether the sets carry everything a format description for `codec` is built from.
        /// AV1's only parameter-set analogue is the sequence header OBU.
        public func isComplete(for codec: NVSTVideoCodec) -> Bool {
            switch codec {
            case .av1: !sequenceParameterSets.isEmpty
            case .h264, .hevc: isComplete
            }
        }

        /// VideoToolbox wants VPS, SPS, PPS in that order for HEVC and SPS, PPS for H.264. AV1
        /// takes just its sequence header OBU.
        public var ordered: [Data] {
            videoParameterSets + sequenceParameterSets + pictureParameterSets
        }
    }

    /// Everything the decoder needs from one access unit, produced in a single copy-free pass.
    public struct Prepared: Sendable {
        public var parameterSets = ParameterSets()
        /// Picture NAL units in length-prefixed (AVCC/HVCC) form, ready for the sample buffer.
        public var sample = Data()
    }

    /// One pass, one output allocation.
    ///
    /// The decode path used to call `parameterSets` and `sampleData` separately, and between them
    /// they copied the whole access unit about seven times and scanned it twice: each helper began
    /// with `[UInt8](accessUnit)`, each called `nalUnits` (which copies again), `pictureNalUnits`
    /// copied every NAL into its own `Data`, and `lengthPrefixed` grew an unreserved buffer. At
    /// 5120x2160 an access unit is 70–112 KB and this ran 120 times a second, on the queue that
    /// feeds the decoder — measured at 9.5 ms per frame against a 9.5 ms frame interval, which is
    /// a saturated pipeline and an unbounded queue behind it.
    public static func prepare(_ accessUnit: Data, codec: NVSTVideoCodec) -> Prepared {
        var prepared = Prepared()
        guard codec != .av1 else {
            // AV1's low-overhead OBU stream is already the length-delimited shape a VideoToolbox
            // sample wants (AV1-ISOBMFF: sample OBUs carry their own size field); the temporal
            // delimiter and padding are the only OBUs that are not sample data. The sequence
            // header OBU doubles as the parameter set the av1C format description is built from.
            // A unit that is not size-fielded OBUs yields an empty sample and no parameter sets —
            // the decoder names that framing failure instead of decoding garbage.
            guard let units = NvstAv1Obu.units(in: accessUnit) else { return prepared }
            var writer = NvstByteWriter(capacity: accessUnit.count)
            for unit in units {
                switch unit.type {
                case NvstAv1Obu.temporalDelimiterType, NvstAv1Obu.paddingType:
                    continue
                case NvstAv1Obu.sequenceHeaderType:
                    prepared.parameterSets.sequenceParameterSets.append(NvstAv1Obu.configurationOBU(for: unit, in: accessUnit))
                default:
                    break
                }
                writer.bytes(accessUnit[unit.offset..<(unit.offset + unit.headerLength + unit.payloadLength)])
            }
            prepared.sample = writer.data
            return prepared
        }
        var writer = NvstByteWriter(capacity: accessUnit.count)
        accessUnit.withUnsafeBytes { raw in
            guard let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self) else { return }
            let count = raw.count
            forEachNalUnit(base: base, count: count) { offset, length in
                let header = base[offset]
                let type: UInt8 = codec == .h264 ? (header & 0x1f) : ((header >> 1) & 0x3f)
                let unit = UnsafeRawBufferPointer(start: base + offset, count: length)
                switch disposition(ofNalType: type, codec: codec) {
                case .videoParameterSet: prepared.parameterSets.videoParameterSets.append(Data(unit))
                case .sequenceParameterSet: prepared.parameterSets.sequenceParameterSets.append(Data(unit))
                case .pictureParameterSet: prepared.parameterSets.pictureParameterSets.append(Data(unit))
                case .discard: break
                case .sample:
                    writer.u32BE(UInt32(length))
                    writer.bytes(unit)
                }
            }
        }
        prepared.sample = writer.data
        return prepared
    }

    /// What `prepare` does with one NAL unit.
    private enum NalDisposition {
        case videoParameterSet
        case sequenceParameterSet
        case pictureParameterSet
        /// Access-unit delimiters and filler data: VideoToolbox takes neither.
        case discard
        /// Picture data, appended to the length-prefixed sample.
        case sample
    }

    private static func disposition(ofNalType type: UInt8, codec: NVSTVideoCodec) -> NalDisposition {
        switch codec {
        case .h264: h264Disposition(ofNalType: type)
        case .hevc: hevcDisposition(ofNalType: type)
        // AV1 has no Annex-B NAL framing; `prepare` returns before reaching this.
        case .av1: .sample
        }
    }

    private static func h264Disposition(ofNalType type: UInt8) -> NalDisposition {
        switch type {
        case 7: .sequenceParameterSet
        case 8: .pictureParameterSet
        // 9 AUD, 12 filler.
        case 9, 12: .discard
        default: .sample
        }
    }

    private static func hevcDisposition(ofNalType type: UInt8) -> NalDisposition {
        switch type {
        case 32: .videoParameterSet
        case 33: .sequenceParameterSet
        case 34: .pictureParameterSet
        // 35 AUD, 38 filler.
        case 35, 38: .discard
        default: .sample
        }
    }

    /// Walks Annex-B NAL units in place, handing each one's header offset and length to `body`.
    private static func forEachNalUnit(base: UnsafePointer<UInt8>, count: Int, _ body: (Int, Int) -> Void) {
        guard count >= 3 else { return }
        var cursor = 0
        var currentStart: Int?
        while cursor <= count - 3 {
            let isFour = cursor <= count - 4
                && base[cursor] == 0 && base[cursor + 1] == 0 && base[cursor + 2] == 0 && base[cursor + 3] == 1
            let isThree = base[cursor] == 0 && base[cursor + 1] == 0 && base[cursor + 2] == 1
            guard isFour || isThree else {
                cursor += 1
                continue
            }
            let prefixLength = isFour ? 4 : 3
            if let start = currentStart, cursor > start { body(start, cursor - start) }
            currentStart = cursor + prefixLength
            cursor += prefixLength
        }
        if let start = currentStart, start < count { body(start, count - start) }
    }

    /// Extracts the parameter sets from an Annex-B access unit. Keyframes carry them inline; a
    /// delta frame usually carries none, which is why the decoder caches the last complete set.
    public static func parameterSets(in accessUnit: Data, codec: NVSTVideoCodec) -> ParameterSets {
        guard codec != .av1 else { return av1ParameterSets(in: accessUnit) }
        var sets = ParameterSets()
        let buffer = [UInt8](accessUnit)
        for unit in NvstAnnexB.nalUnits(accessUnit) where unit.offset < buffer.count && unit.length > 0 {
            let payload = Data(buffer[unit.offset..<(unit.offset + unit.length)])
            appendParameterSet(payload, nalHeader: buffer[unit.offset], codec: codec, to: &sets)
        }
        return sets
    }

    /// Files one NAL unit under its parameter-set kind; picture NALs and delimiters are skipped.
    private static func appendParameterSet(_ payload: Data, nalHeader: UInt8, codec: NVSTVideoCodec, to sets: inout ParameterSets) {
        switch codec {
        case .h264:
            switch nalHeader & 0x1f {
            case 7: sets.sequenceParameterSets.append(payload)
            case 8: sets.pictureParameterSets.append(payload)
            default: break
            }
        case .hevc:
            switch (nalHeader >> 1) & 0x3f {
            case 32: sets.videoParameterSets.append(payload)
            case 33: sets.sequenceParameterSets.append(payload)
            case 34: sets.pictureParameterSets.append(payload)
            default: break
            }
        case .av1: break
        }
    }

    /// AV1's only parameter-set analogue: the sequence header OBU in its wire form, harvested
    /// from a keyframe's access unit.
    private static func av1ParameterSets(in accessUnit: Data) -> ParameterSets {
        var sets = ParameterSets()
        guard let units = NvstAv1Obu.units(in: accessUnit) else { return sets }
        for unit in units where unit.type == NvstAv1Obu.sequenceHeaderType {
            sets.sequenceParameterSets.append(NvstAv1Obu.configurationOBU(for: unit, in: accessUnit))
        }
        return sets
    }

    /// The NAL units that belong in the sample buffer: everything except the parameter sets and
    /// the access-unit delimiter, which VideoToolbox takes through the format description.
    public static func pictureNalUnits(in accessUnit: Data, codec: NVSTVideoCodec) -> [Data] {
        guard codec != .av1 else {
            // AV1's sample units are whole OBUs minus the temporal delimiter and padding.
            guard let units = NvstAv1Obu.units(in: accessUnit) else { return [] }
            return units.compactMap { unit in
                guard unit.type != NvstAv1Obu.temporalDelimiterType, unit.type != NvstAv1Obu.paddingType else { return nil }
                return Data(accessUnit[unit.offset..<(unit.offset + unit.headerLength + unit.payloadLength)])
            }
        }
        let buffer = [UInt8](accessUnit)
        var units: [Data] = []
        for unit in NvstAnnexB.nalUnits(accessUnit) {
            guard unit.offset < buffer.count, unit.length > 0 else { continue }
            let header = buffer[unit.offset]
            switch codec {
            case .h264:
                let type = header & 0x1f
                // 7 SPS, 8 PPS, 9 AUD, 12 filler.
                if type == 7 || type == 8 || type == 9 || type == 12 { continue }
            case .hevc:
                let type = (header >> 1) & 0x3f
                // 32 VPS, 33 SPS, 34 PPS, 35 AUD, 38 filler.
                if (32...35).contains(type) || type == 38 { continue }
            case .av1:
                break
            }
            units.append(Data(buffer[unit.offset..<(unit.offset + unit.length)]))
        }
        return units
    }

    /// Converts Annex-B to length-prefixed (AVCC/HVCC) form with a 4-byte big-endian length per
    /// NAL unit — the `nal_length_size` VideoToolbox is configured with.
    public static func lengthPrefixed(_ nalUnits: [Data], lengthSize: Int = 4) -> Data {
        var writer = NvstByteWriter(capacity: nalUnits.reduce(0) { $0 + $1.count + lengthSize })
        for unit in nalUnits {
            let length = UInt32(unit.count)
            for shift in stride(from: (lengthSize - 1) * 8, through: 0, by: -8) {
                writer.u8(UInt8((length >> UInt32(shift)) & 0xff))
            }
            writer.bytes(unit)
        }
        return writer.data
    }

    /// Full conversion for one access unit: the picture NAL units in length-prefixed form. AV1
    /// samples are the OBU stream itself (size fields included), not a length-prefixed framing.
    public static func sampleData(for accessUnit: Data, codec: NVSTVideoCodec) -> Data {
        if codec == .av1 { return prepare(accessUnit, codec: codec).sample }
        return lengthPrefixed(pictureNalUnits(in: accessUnit, codec: codec))
    }
}
