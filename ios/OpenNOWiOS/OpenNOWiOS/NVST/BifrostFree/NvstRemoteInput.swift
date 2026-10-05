import Foundation

/// NVST remote-input (RI) packet encoding.
///
/// Recovered from `RiClientBackend` in libBifrost2. An RI packet is
/// `[u32 big-endian length][u32 little-endian type][body]`, and the whole thing travels as the
/// payload of control command `0x206`. The mixed endianness is not a transcription slip: the
/// length is written byte-swapped (`bswapl`) while the type is a plain store, and the receive side
/// confirms it — `handleServerCommand`'s `0x206` branch compares the word at payload+4 against
/// small constants without swapping.
public enum NvstRemoteInput {
    /// The command that carries every RI packet.
    public static let commandCode = NvstControlCommandCode.remoteInput

    /// The keepalive the client writes to the reliable input channel every two seconds.
    public static let heartbeat = Data([2, 0, 0, 0])
    public static let heartbeatInterval: TimeInterval = 2

    /// The seat announces which remote-input protocol it speaks on the reliable input channel, and
    /// input is not accepted until it has. Two layouts are seen: a `0x020e` command whose payload
    /// carries the version, and a bare message whose first byte is `0x0e`.
    /// The input-protocol handshake, as the seat sent it in every capture: command `0x020e`
    /// with a two-byte payload carrying the version — `[u16 0x020e][u16 length][u16 version]`.
    /// Two shorter shapes that upstream's client also accepts are kept for parity with their
    /// readings documented; neither has ever appeared in a capture, so the full command is the
    /// real path and the value the bundle gates input on.
    public static func protocolVersion(in bytes: Data) -> UInt16? {
        guard bytes.count >= 2 else { return nil }
        var prefix = NvstByteReader(bytes)
        guard let first = try? prefix.u16LE() else { return nil }
        guard first == NvstControlCommandCode.inputProtocolVersion.rawValue else {
            // Upstream's bare shape: a message starting with the 0x0e tag byte reports its whole
            // first word as the "version", tag byte folded in.
            return bytes[bytes.startIndex] == 0x0e ? first : nil
        }
        // The captured shape: a full command, whose version is the payload's first u16.
        let (commands, _) = NvstControlCommand.parse(Data(bytes))
        if let command = commands.first, command.code == .inputProtocolVersion, command.payload.count >= 2 {
            var payload = NvstByteReader(command.payload)
            return try? payload.u16LE()
        }
        // Upstream's truncated shape `[u16 0x020e][u16 version]`: the word at offset 2 is the
        // version in that convention, though in the seat's full framing that offset holds the
        // length field. A lone code word with nothing after it is version 2 in upstream's tests.
        guard bytes.count >= 4 else { return 2 }
        var rest = NvstByteReader(bytes)
        guard (try? rest.skip(2)) != nil, let version = try? rest.u16LE() else { return nil }
        return version
    }

    /// Packet types, as `Get*PacketId` returns them. The full id space (1–27, `ValidId`) was
    /// verified against the official client's dispatch; see `docs/NVST/OfficialClientAudit.md`.
    public enum PacketType: UInt32, Equatable, Sendable {
        case envelope = 0x0e
        /// Relative pointer motion: `[i16 BE dx][i16 BE dy][u16 BE flags]`, selected when the
        /// builder's flag word is `0x10000` and the flags carry no `0x800` (absolute) bit.
        case keyDown = 3
        case keyUp = 4
        case absoluteMouseMove = 5
        /// Flag `0x1000` form of the absolute move, carrying two extra u16 fields.
        case absoluteMouseMoveExtended = 6
        case mouseMove = 7
        // Captured from the official client running the same gesture: a press encodes as 8 and the
        // matching release as 9. The flag pairs in the builder's jump table are (down, up), not
        // (up, down) as Win32's `MOUSEEVENTF_*` ordering suggests.
        case mouseButtonPress = 8
        case mouseButtonRelease = 9
        /// The wheel. Static RE read this as "a second motion form whose first word is zero and
        /// whose second is a scaled value"; the capture shows the scaled value is the notch count
        /// times Windows' `WHEEL_DELTA`, so it is the scroll wheel rather than a pointer move.
        case mouseWheel = 10
        /// The 20-byte pre-XInput gamepad form; the modern 34-byte form rides command `0x20d`.
        case gamepadLegacy = 11
        /// Client-to-server gamepad haptics state (`GetHapticsPacketId`).
        case hapticsState = 13
        /// Client-to-server window focus event, 6-byte body.
        case windowFocus = 15
        /// Client-to-server window geometry event, 14-byte body.
        case windowGeometry = 16
        /// DS4/DS5-class HID report (type word `0x11` with a multi-group flag in the MSB,
        /// per-device partially-reliable sequence). Rumble and gyro ride here.
        case hidReport = 17
        case hidChange = 0x12
        /// Caps/num/scroll state, a 1-byte bitfield (0x10/0x20/0x40). Previously mislabelled
        /// `keyboard`; actual key events are `keyDown`/`keyUp`.
        case lockKeys = 0x13
        /// IME hotkey, 1-byte `NvstImeHotKey_t` body.
        case imeHotkey = 20
        case bulkPayloadPartiallyReliable = 21
        case touch = 22
        case utf8Text = 23
        case lowLevelTouch = 24
        /// Mouse settings, two u32 fields.
        case mouseSettings = 25
    }

    /// The button ordinal the wire uses. NVIDIA's flag bits follow Win32's left/right/middle
    /// order, but the byte written into the packet is left=1, middle=2, right=3 — so a client
    /// enum that orders them differently has to be remapped rather than passed through.
    public enum Button: UInt8, Equatable, Sendable {
        case left = 1
        case middle = 2
        case right = 3
        case extra1 = 4
        case extra2 = 5
    }

    /// Frames a body as an RI packet.
    static func packet(type: PacketType, body: Data) -> Data {
        var writer = NvstByteWriter(capacity: 8 + body.count)
        writer.u32BE(UInt32(4 + body.count))
        writer.u32LE(type.rawValue)
        writer.bytes(body)
        return writer.data
    }

    /// How a packet is framed before it goes on a data channel. Which one the seat expects is not
    /// yet established, so each is selectable rather than assumed.
    public enum Framing: String, CaseIterable, Sendable {
        /// The packet exactly as `PreparePacket` leaves it.
        case plain
        /// `PreparePacket` plus the partially-reliable per-type sequence: a big-endian 16-bit
        /// counter appended by `handlePrSequenceNumber`.
        case sequenced
        /// `SendPacketV3`: a `0x23` marker and a big-endian 64-bit microsecond timestamp in front.
        case versioned
        /// `SendPacket`'s timestamp envelope: an outer type `0x0e` packet whose body is the inner
        /// packet zero-padded to an 8-byte slot boundary, then a single 64-bit microsecond
        /// timestamp in the envelope's last 8 bytes. The envelope's timestamp is little-endian,
        /// unlike `versioned`'s — the library stores one with `movq` and the other with `bswapq`.
        /// Inners larger than `envelopeMaxInnerLength` come back bare, as `SendPacket` does.
        case enveloped
    }

    /// The inner packet plus its trailing padding, before the timestamp, per `SendPacket`'s slot
    /// arithmetic (arm64 disassembly): inner packet and timestamp together occupy `slots*8` bytes
    /// where `slots = (innerLength + 8) / 8` — the timestamp gets its own 8-byte slot, so the
    /// padding between packet and timestamp is at least 8 bytes. That is what makes a 14-byte
    /// pointer packet 40 bytes and a 22-byte key packet 48. The library never writes the padding
    /// region — captures show it as zeros in a fresh buffer and as stale buffer content in a
    /// reused one — so the canonical encoding zeros it.
    static func envelopePaddedLength(innerLength: Int) -> Int {
        ((innerLength + 8) / 8) * 8 + 8
    }

    /// The largest inner packet the envelope accepts. `SendPacket` refuses slot counts of `0x7f`
    /// or more ("Failed to put in envelope %d byte remote input packet. Packet is too big.") and
    /// sends the packet bare instead — which is what the official client does for text packets
    /// whose body reaches 1000 bytes.
    static let envelopeMaxInnerLength = 0x7e * 8 - 1

    /// Applies a framing to an encoded packet.
    public static func framed(_ packet: Data, framing: Framing, sequence: UInt16, timestampMicroseconds: UInt64) -> Data {
        switch framing {
        case .plain:
            return packet
        case .sequenced:
            var writer = NvstByteWriter(capacity: packet.count + 2)
            writer.bytes(packet)
            writer.u16BE(sequence)
            return writer.data
        case .versioned:
            var writer = NvstByteWriter(capacity: 1 + 8 + packet.count)
            writer.u8(GeronimoInputEnvelope.headerByte)
            writer.u64BE(timestampMicroseconds)
            writer.bytes(packet)
            return writer.data
        case .enveloped:
            guard packet.count <= envelopeMaxInnerLength else { return packet }
            let paddedLength = envelopePaddedLength(innerLength: packet.count)
            var writer = NvstByteWriter(capacity: paddedLength + 8)
            writer.bytes(packet)
            writer.zeroes(paddedLength - packet.count)
            writer.u64LE(timestampMicroseconds)
            return self.packet(type: .envelope, body: writer.data)
        }
    }

    /// A key transition: `[BE u16 virtual key][BE u16 modifiers]` then padding, 14-byte body.
    ///
    /// Both fields confirmed from a capture of the vendored client: 'A' is VK 0x41, left shift is
    /// VK 0x00A0, and the modifier half is the low nibble of `KeyboardModifiers` (shift 0x01,
    /// control 0x02, option 0x04, command 0x08). The modifier field is why capitals worked as a
    /// *chord shortcut* (the shift key registered) but never produced capital letters — the letter
    /// packet has to carry the shift bit, not just be preceded by a shift-key event.
    public static func keyboard(virtualKey: UInt16, modifiers: UInt16 = 0, isPressed: Bool) -> Data {
        var writer = NvstByteWriter(capacity: 14)
        writer.u16BE(virtualKey)
        writer.u16BE(modifiers)
        writer.zeroes(10)
        return packet(type: isPressed ? .keyDown : .keyUp, body: writer.data)
    }

    /// An absolute pointer position, in the coordinate space the client declares alongside it.
    /// Captured from the native stack: inner type 5, with the position, the `0x0800` absolute flag
    /// and the viewport the position is measured in. Sending (1920, 1080) produced (1631, 687) with
    /// a declared viewport of 1632x688, which is the stream view's own content frame — so these are
    /// view coordinates, not stream-resolution ones.
    public static func absoluteMouseMove(x: UInt16,
                                         y: UInt16,
                                         viewportWidth: UInt16,
                                         viewportHeight: UInt16) -> Data {
        // Five fields, an inner length of 10. What follows them in the capture (`0080 0000 ...`) is
        // the envelope's padding, and it is not zeroed there — the vendored library reuses one
        // buffer, so its button packets show leftover viewport bytes in the same region.
        var writer = NvstByteWriter(capacity: 10)
        for word in [x, y, absoluteFlag, viewportWidth, viewportHeight] {
            writer.u16BE(word)
        }
        return packet(type: .absoluteMouseMove, body: writer.data)
    }

    /// The flag that marks a pointer position as absolute.
    public static let absoluteFlag: UInt16 = 0x0800

    /// Windows' `WHEEL_DELTA`: one notch.
    public static let wheelDelta = 120
    public static let wheelDelta16: Int16 = 120

    /// A wheel movement, in `WHEEL_DELTA` units — 120 per notch, which is what the wire carries
    /// (one downward notch was captured as -120).
    ///
    /// The value is **not** rescaled here. `MouseEvent.wheel.delta` already arrives in those units:
    /// the stream view's `accumulatedWheelDelta` multiplies by 120 on both of its branches, and the
    /// vendored transport passes the value straight to `sendMouseWheel`. Multiplying again turned
    /// one notch into 14400 and saturated a signed 16-bit field after two of them.
    public static func mouseWheel(delta: Int16) -> Data {
        var writer = NvstByteWriter(capacity: 6)
        writer.zeroes(2)
        writer.u16BE(UInt16(bitPattern: delta))
        writer.zeroes(2)
        return packet(type: .mouseWheel, body: writer.data)
    }

    /// A sideways wheel movement, in the same `WHEEL_DELTA` units.
    ///
    /// Measured: the type-10 body is three big-endian words; word 1 carries the vertical notch
    /// count (one downward notch captured as -120) and word 0 was zero in every capture of a
    /// vertical scroll.
    ///
    /// Inferred: that word 0 is the horizontal delta. No capture ever carried a non-zero value
    /// there. The reading rests on the body being the same three-word shape as the type-7 relative
    /// move — `[dx][dy][flags]` — with the vertical wheel sitting in the `dy` slot, which leaves
    /// the `dx` slot as where a sideways delta would go; Win32 pairs `WM_MOUSEWHEEL` and
    /// `WM_MOUSEHWHEEL` the same way, both carrying a `WHEEL_DELTA`-scaled short. Word 0 being a
    /// mode, an index or padding fits the same evidence.
    ///
    /// Which is why the two axes never share a packet: sending horizontal with the vertical word
    /// at zero means a misread of word 0 can only be weighed against a zero vertical delta, never
    /// against a real scroll. Vertical scrolling is the capture-verified path here and stays
    /// byte-identical to what it sends today.
    public static func mouseWheelHorizontal(delta: Int16) -> Data {
        var writer = NvstByteWriter(capacity: 6)
        writer.u16BE(UInt16(bitPattern: delta))
        writer.zeroes(4)
        return packet(type: .mouseWheel, body: writer.data)
    }

    /// A relative pointer move: `[i16 BE dx][i16 BE dy][u16 BE flags]`. The `0x800` flag would
    /// make it absolute, so a relative move leaves the flags at zero.
    public static func mouseMove(deltaX: Int16, deltaY: Int16, flags: UInt16 = 0) -> Data {
        var writer = NvstByteWriter(capacity: 6)
        for word in [UInt16(bitPattern: deltaX), UInt16(bitPattern: deltaY), flags] {
            writer.u16BE(word)
        }
        return packet(type: .mouseMove, body: writer.data)
    }

    /// A button transition. The second body byte is the high half of the builder's 16-bit
    /// argument, which is zero for a plain click.
    public static func mouseButton(_ button: Button, isPressed: Bool) -> Data {
        packet(type: isPressed ? .mouseButtonPress : .mouseButtonRelease,
               body: Data([button.rawValue, 0]))
    }

    /// The most UTF-8 bytes one text packet carries. `sendUnicodeEvent` admits chunks up to
    /// `0x3f9` (1017) bytes, then cuts them back at a code point boundary — at most three bytes
    /// back — so a chunk is never longer than `0x3f8`.
    public static let maxUtf8TextBodyBytes = 0x3f8

    /// The cut positions the official chunker probes, in order, when a chunk overflows: the byte
    /// at each offset must not be a UTF-8 continuation byte (`10xxxxxx`), or the code point it
    /// belongs to would be split. A cut at offset `n` keeps bytes `0..<n` and moves the code
    /// point starting at `n` whole into the next packet.
    static let utf8TextCutOffsets = [0x3f8, 0x3f7, 0x3f6, 0x3f5]

    /// Splits raw UTF-8 into text-packet bodies exactly as `RiClientBackend::sendUnicodeEvent`
    /// does. A tail whose four probe positions are all continuation bytes cannot be cut; the
    /// official client logs "Couldn't find valid utf-8 code point at the end of 1KB chunk in
    /// string during packetization" and drops it, and so does this — `droppedBytes` reports the
    /// remainder. Valid UTF-8 always has a leader byte within three of any position, so this can
    /// only happen for hand-built invalid input.
    public static func utf8TextBodies(_ utf8: Data) -> (bodies: [Data], droppedBytes: Int) {
        var bodies: [Data] = []
        var start = utf8.startIndex
        while start < utf8.endIndex {
            let remaining = utf8.distance(from: start, to: utf8.endIndex)
            if remaining <= maxUtf8TextBodyBytes {
                bodies.append(Data(utf8[start...]))
                break
            }
            var cut: Int?
            for offset in utf8TextCutOffsets where utf8[utf8.index(start, offsetBy: offset)] & 0xc0 != 0x80 {
                cut = offset
                break
            }
            guard let cut else { return (bodies, remaining) }
            let boundary = utf8.index(start, offsetBy: cut)
            bodies.append(Data(utf8[start..<boundary]))
            start = boundary
        }
        return (bodies, 0)
    }

    /// Text packets for a string — the raw-UTF-8 bodies under RI type 23, one per chunk, exactly
    /// the bytes `sendUnicodeEvent` puts on the wire. This is the protocol's own encoding for
    /// IME commits and pastes; the older virtual-keystroke typing was a fallback that could not
    /// reach anything a US layout cannot produce.
    public static func utf8TextPackets(forText text: String) -> (packets: [Data], droppedBytes: Int) {
        let (bodies, droppedBytes) = utf8TextBodies(Data(text.utf8))
        return (bodies.map { packet(type: .utf8Text, body: $0) }, droppedBytes)
    }

    /// An IME control hotkey: a single `NvstImeHotKey_t` byte, packet type 20, sent exactly as
    /// `sendImeControlEvent` sends it. The enum's named values are not recovered from the
    /// official binary, so the byte travels as the caller supplies it.
    public static func imeHotkey(_ code: UInt8) -> Data {
        packet(type: .imeHotkey, body: Data([code]))
    }

    /// Haptics on or off for this session: packet type 13 with a two-byte little-endian body,
    /// exactly what `RiClientBackend::enableHaptics(bool)` writes (`sturh w1, [x0, #0x19]` into the
    /// body slot, body length 2, then the ordinary `SendPacket` envelope). The seat sends no
    /// `0x010b` rumble commands until it has seen this with the flag set; the official client
    /// sends it when the app enables the "gamepad haptics" feature, which is whenever a
    /// rumble-capable pad is attached.
    public static func hapticsState(enabled: Bool) -> Data {
        var writer = NvstByteWriter(capacity: 2)
        writer.u16LE(enabled ? 1 : 0)
        return packet(type: .hapticsState, body: writer.data)
    }
}
