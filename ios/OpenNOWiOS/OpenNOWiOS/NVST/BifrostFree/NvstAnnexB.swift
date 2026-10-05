import Foundation

/// Annex-B access-unit helpers for the NVST receive path.
///
/// GS-framed video payloads carry a small GameStream frame header before the first Annex-B
/// start code on the start-of-frame packet; everything after it is pure elementary stream.
/// Independently observed; upstream documents the same shapes as `h264_picture_payload`,
/// `h264_access_unit_is_keyframe` and `find_annex_b_start_code`.
public enum NvstAnnexB {
    /// The GS frame header precedes media by at most 64 bytes plus a 4-byte start code.
    public static let maxPictureHeaderBytes = 64 + 4

    /// Finds the first Annex-B start code: `(offset, prefixLength)` with prefixLength 4 for
    /// `00 00 00 01`, otherwise 3 for `00 00 01`.
    public static func findStartCode(_ bytes: Data) -> (offset: Int, prefixLength: Int)? {
        let bytes = [UInt8](bytes)
        guard bytes.count >= 3 else { return nil }
        // The *earliest* start code wins. Scanning for the four-byte form across the whole buffer
        // first would skip an earlier three-byte one and hand the decoder a truncated unit.
        var fourByte: Int?
        if bytes.count >= 4 {
            for index in 0...(bytes.count - 4) where bytes[index] == 0 && bytes[index + 1] == 0 && bytes[index + 2] == 0 && bytes[index + 3] == 1 {
                fourByte = index
                break
            }
        }
        var threeByte: Int?
        for index in 0...(bytes.count - 3) where bytes[index] == 0 && bytes[index + 1] == 0 && bytes[index + 2] == 1 {
            threeByte = index
            break
        }
        switch (fourByte, threeByte) {
        case (let four?, let three?): return four <= three ? (four, 4) : (three, 3)
        case (let four?, nil): return (four, 4)
        case (nil, let three?): return (three, 3)
        case (nil, nil): return nil
        }
    }

    /// Every NAL unit in an Annex-B buffer as `(headerByteOffset, length)` pairs, start codes
    /// excluded.
    public static func nalUnits(_ bytes: Data) -> [(offset: Int, length: Int)] {
        let buffer = [UInt8](bytes)
        var units: [(offset: Int, length: Int)] = []
        var cursor = 0
        var currentStart: Int?
        while cursor <= buffer.count - 3 {
            let isFour = cursor <= buffer.count - 4 && buffer[cursor] == 0 && buffer[cursor + 1] == 0 && buffer[cursor + 2] == 0 && buffer[cursor + 3] == 1
            let isThree = buffer[cursor] == 0 && buffer[cursor + 1] == 0 && buffer[cursor + 2] == 1
            guard isFour || isThree else {
                cursor += 1
                continue
            }
            let prefixLength = isFour ? 4 : 3
            if let start = currentStart, cursor > start {
                units.append((start, cursor - start))
            }
            currentStart = cursor + prefixLength
            cursor += prefixLength
        }
        if let start = currentStart, start < buffer.count {
            units.append((start, buffer.count - start))
        }
        return units
    }

    /// What the receive path needs to know about a completed access unit, from **one** pass over it
    /// and without copying it.
    ///
    /// The frame-completion path used to answer these two questions separately, and each answer
    /// cost a full `[UInt8]` copy plus a byte-by-byte scan — `strippingTrailingAccessUnitDelimiter`
    /// copied the unit and called `nalUnits` (which copied it again), then `isKeyframe` repeated
    /// both. Four copies and two scans of a 70–112 KB 5K frame, 120 times a second, on the thread
    /// that must also drain the socket.
    public struct AccessUnitScan: Equatable, Sendable {
        /// Offset of the last NAL unit's header byte, or nil when the buffer holds none.
        public var lastUnitOffset: Int?
        /// The header byte of that last unit, for the access-unit-delimiter test.
        public var lastUnitHeader: UInt8 = 0
        /// Offset of the start code introducing the last unit — where a trailing delimiter is cut.
        public var lastStartCodeOffset: Int?
        public var isKeyframe = false
    }

    public static func scan(_ bytes: Data, codec: NVSTVideoCodec) -> AccessUnitScan {
        var result = AccessUnitScan()
        // AV1 has no Annex-B units to locate, but the keyframe answer the caller needs lives in
        // the OBU stream's frame headers.
        guard codec != .av1 else {
            result.isKeyframe = NvstAv1Obu.containsKeyframe(in: bytes)
            return result
        }
        bytes.withUnsafeBytes { raw in
            guard let base = raw.baseAddress?.assumingMemoryBound(to: UInt8.self) else { return }
            let count = raw.count
            guard count >= 3 else { return }
            var cursor = 0
            while cursor <= count - 3 {
                let isFour = cursor <= count - 4
                    && base[cursor] == 0 && base[cursor + 1] == 0 && base[cursor + 2] == 0 && base[cursor + 3] == 1
                let isThree = base[cursor] == 0 && base[cursor + 1] == 0 && base[cursor + 2] == 1
                guard isFour || isThree else {
                    cursor += 1
                    continue
                }
                let prefixLength = isFour ? 4 : 3
                let headerOffset = cursor + prefixLength
                if headerOffset < count {
                    let header = base[headerOffset]
                    result.lastUnitOffset = headerOffset
                    result.lastUnitHeader = header
                    result.lastStartCodeOffset = cursor
                    switch codec {
                    case .h264: if header & 0x1f == 5 { result.isKeyframe = true }
                    case .hevc: if (16...21).contains((header >> 1) & 0x3f) { result.isKeyframe = true }
                    case .av1: break
                    }
                }
                cursor += prefixLength
            }
        }
        return result
    }

    /// H.264 `nal_unit_type == 5` (IDR slice) or HEVC IRAP (types 16…21) anywhere in the unit.
    public static func isKeyframe(_ bytes: Data, codec: NVSTVideoCodec = .h264) -> Bool {
        let buffer = [UInt8](bytes)
        for unit in nalUnits(bytes) {
            guard unit.offset < buffer.count else { continue }
            switch codec {
            case .h264:
                if buffer[unit.offset] & 0x1f == 5 { return true }
            case .hevc:
                let type = (buffer[unit.offset] >> 1) & 0x3f
                if (16...21).contains(type) { return true }
            case .av1:
                // AV1 does not use Annex-B framing; keyframe detection lives in the OBU parser.
                return false
            }
        }
        return false
    }

    /// Strips the GameStream frame header from a start-of-frame payload. `nil` when no start
    /// code appears in the bounded window, which upstream treats as a dropped packet rather
    /// than feeding the decoder garbage.
    public static func picturePayload(_ bytes: Data, codec: NVSTVideoCodec = .h264) -> Data? {
        // AV1 has no Annex-B start code, so the header boundary cannot be discovered by scanning:
        // the search would either fail outright or match a stray 00 00 01 inside the OBU data and
        // strip the wrong number of bytes. GFN's cloud NVST uses fixed sizes instead, keyed by the
        // first byte — and its extended header is 20 bytes, NOT the 44 of the similarly named
        // consumer GameStream layout.
        if codec == .av1 {
            switch bytes.first {
            case 0x01: return bytes.count > shortFrameHeaderBytes ? bytes.dropFirst(shortFrameHeaderBytes) : nil
            case 0x81: return bytes.count > gfnExtendedFrameHeaderBytes ? bytes.dropFirst(gfnExtendedFrameHeaderBytes) : nil
            default: return nil
            }
        }
        let windowLength = min(bytes.count, maxPictureHeaderBytes)
        let window = bytes.prefix(windowLength)
        guard let (offset, _) = findStartCode(window) else { return nil }
        // A slice sharing the packet's storage: the payload is otherwise discarded after push,
        // so this spares one copy of up to ~1.4 KB per frame start.
        return bytes.dropFirst(offset)
    }

    /// GFN cloud NVST AV1 frame-header sizes, selected by the payload's first byte.
    static let shortFrameHeaderBytes = 8
    static let gfnExtendedFrameHeaderBytes = 20
}
