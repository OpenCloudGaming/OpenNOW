import Foundation

/// The client's gamepad state, command `0x20d` on the input channel (SCTP stream 10).
///
/// Recovered by driving ten distinct gamepad states through the native stack and reading the
/// plaintext it produced. The body is NVIDIA's own wrapper around an XInput `XINPUT_GAMEPAD`:
/// the button mask is XInput's (`south` arrives as `0x1000`, XInput's `A`; `north` as `0x8000`,
/// its `Y`; `start` as `0x0010`), the triggers are single bytes, and the four axes are signed
/// 16-bit values scaled by 32768.
///
/// It shares the `0x20d` code and `[0x23][BE u64 µs]` prefix with the device descriptor in
/// `NvstInputActivation`; the gamepad form is four bytes longer and carries a sequence counter.
public struct NvstGamepadPacket: Equatable, Sendable {
    public static let commandCode = NvstControlCommandCode.gamepadEvent
    /// 1 header byte + 8 outer timestamp + 6 partially-reliable wrapper + 3 length prefix + 38
    /// event bytes. Was 52: the two length bytes were missing, see `capturedBody` below.
    public static let payloadLength = 54

    /// The 38-byte gamepad event: `GeronimoInputEventType.gamepad` (12) followed by the 34 bytes
    /// `RiNvscGamepadMapper` calls its "raw gamepad data". Only the XInput fields overlaid below
    /// ever move.
    ///
    /// This replaces a 43-byte `capturedBody` transcribed from a single idle reference packet. That
    /// transcription was wrong in the framing, not the fields: it carried `0x22`
    /// (`singleReliablePayloadTag`) where the wire carries `0x21` (`lengthPrefixedPayloadTag`)
    /// followed by a `u16` big-endian payload length, so every state packet we ever sent was two
    /// bytes short and tagged as a payload kind that has no length prefix. The seat could register
    /// the pad (a different message) and then silently ignore every state update — exactly the
    /// symptom. Ground truth is `libBifrost2`'s `RiClientBackend::sendGamepadEvent`
    /// (size `0x22` selects event type `0x0c`) plus the validated legacy wire fixture in tests.
    static let eventLength = 38
    static let capturedBody: [UInt8] = [
        0x0c, 0x00, 0x00, 0x00, 0x1a, 0x00, 0x00, 0x00, 0x01, 0x00,
        0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x55, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]
    /// Microseconds since the input session started, as a **u64 little-endian** at event[30].
    /// `RiNvscGamepadMapper::updateCaptureTimestamp(unsigned long long)` stores it with a single
    /// `movq %rsi, 0x2e(%rdi)`, and the mapper's raw data starts at `this+0x14` — so it is the last
    /// eight bytes of the 34-byte raw block, i.e. event[30..38]. It was previously read as a u32
    /// timestamp followed by a constant "length" byte, so every packet shipped a timestamp with
    /// 0x2b (later 0x20) sitting in its fifth byte: a value ~137 billion µs in the future.
    static let timestampFieldOffset = 30
    /// event[6] is the gamepad *index*, masked to 0...3 — `updateGamepadId` writes a u16 at
    /// `this+0x16` (raw byte 2), and `handleGamepadStateEvent` rejects ids >= 4 outright
    /// ("Gamepad ID %u is invalid").
    static let gamepadIdOffset = 6
    /// The pad a single-player session uses. Remote Co-Op guests occupy 1...3 and pass their own
    /// index per packet; `handleGamepadStateEvent` rejects anything >= 4.
    public static let gamepadDeviceIndex: UInt16 = 0

    /// The connected bitmap, u16 at event[8] — `updateGamepadsBitmap(unsigned short)` writes a u16
    /// at `this+0x18` (raw byte 4). Bit i marks gamepad i connected; bit (i+8) marks that same pad
    /// as an XInput-style device, which is how `NativeGamepadMonitor` built it on the vendored
    /// path — `1 << index | 1 << (index + 8)` — in the build where the pad worked. 3 (0b11)
    /// announces gamepads 0 AND 1, which was the "two controllers in Steam" symptom.
    /// 0x0101 = pad 0 connected, XInput-style.
    ///
    /// With Remote Co-Op the map is no longer constant: every connected pad must be announced in
    /// the same u16, host included, and the descriptor and the state packets must agree exactly or
    /// the seat registers the device and then drops its updates. `StreamGamepadTopology`
    /// builds that value; this constant is only the one-pad default.
    static let connectedBitmapOffset = 8
    public static let connectedBitmap: UInt16 = 0x0101

    /// The connected bitmap announcing exactly `indices`, in the seat's `1 << i | 1 << (i + 8)`
    /// form. Kept here so the descriptor, the state packet and the topology all derive it the
    /// same way.
    public static func connectedBitmap(for indices: some Sequence<Int>) -> UInt16 {
        let bitmap = indices.filter { (0..<4).contains($0) }.reduce(into: UInt16(0)) { bitmap, index in
            bitmap |= UInt16(1 << index) | UInt16(1 << (index + 8))
        }
        return bitmap == 0 ? connectedBitmap : bitmap
    }
    static let buttonsOffset = 12
    static let triggersOffset = 14
    static let leftStickXOffset = 16
    static let leftStickYOffset = 18
    static let rightStickXOffset = 20
    static let rightStickYOffset = 22

    /// Per-gamepad counter in the partially-reliable wrapper, **u16 big-endian**. It was a `UInt8`
    /// occupying only the low half, so it wrapped every 256 packets — about four seconds of play.
    public let sequence: UInt16
    public let timestampMicroseconds: UInt64
    public let buttons: UInt16
    public let leftTrigger: UInt8
    public let rightTrigger: UInt8
    public let leftStickX: Int16
    public let leftStickY: Int16
    public let rightStickX: Int16
    public let rightStickY: Int16
    /// Which of the seat's four pads this state belongs to. Player 1 is 0; Remote Co-Op guests
    /// take 1...3, matching `GamepadState.playerIndex`.
    public let gamepadIndex: UInt16
    /// Every pad the seat should believe is connected right now, not just this one.
    public let connectedBitmap: UInt16

    public init(sequence: UInt16,
                timestampMicroseconds: UInt64,
                buttons: UInt16,
                leftTrigger: UInt8 = 0,
                rightTrigger: UInt8 = 0,
                leftStickX: Int16 = 0,
                leftStickY: Int16 = 0,
                rightStickX: Int16 = 0,
                rightStickY: Int16 = 0,
                gamepadIndex: UInt16 = Self.gamepadDeviceIndex,
                connectedBitmap: UInt16 = Self.connectedBitmap) {
        self.gamepadIndex = gamepadIndex & 0x03
        self.connectedBitmap = connectedBitmap
        self.sequence = sequence
        self.timestampMicroseconds = timestampMicroseconds
        self.buttons = buttons
        self.leftTrigger = leftTrigger
        self.rightTrigger = rightTrigger
        self.leftStickX = leftStickX
        self.leftStickY = leftStickY
        self.rightStickX = rightStickX
        self.rightStickY = rightStickY
    }

    /// XInput's button mask. The three values the capture pins down are `A`, `Y` and `START`; the
    /// rest follow XInput's published layout rather than the vendored path's own mapping, which
    /// sent our `leftShoulder` as `0x0001` (XInput's D-pad up) and is not a layout worth copying.
    public struct Button {
        public static let dPadUp: UInt16 = 0x0001
        public static let dPadDown: UInt16 = 0x0002
        public static let dPadLeft: UInt16 = 0x0004
        public static let dPadRight: UInt16 = 0x0008
        public static let start: UInt16 = 0x0010
        public static let back: UInt16 = 0x0020
        public static let leftThumb: UInt16 = 0x0040
        public static let rightThumb: UInt16 = 0x0080
        public static let leftShoulder: UInt16 = 0x0100
        public static let rightShoulder: UInt16 = 0x0200
        /// Xbox/Guide/PS. XInput itself reserves this bit and hides it from `XInputGetState`, but
        /// GFN forwards it: OpenNOW's gamepad mapping sends `GUIDE 0x0400` like the official client.
        /// Without it the Guide button is inert in games that act on it.
        public static let guide: UInt16 = 0x0400
        public static let a: UInt16 = 0x1000
        public static let b: UInt16 = 0x2000
        public static let x: UInt16 = 0x4000
        public static let y: UInt16 = 0x8000
    }

    /// A normalised -1...1 axis as the wire's signed 16-bit value. The capture's 0.5, 0.25 and
    /// -0.75 land on exactly `value * 32768`.
    public static func axis(_ value: Float) -> Int16 {
        Int16(clamping: Int((max(-1, min(1, value)) * 32768).rounded()))
    }

    /// A normalised 0...1 trigger as the wire's byte.
    public static func trigger(_ value: Float) -> UInt8 {
        UInt8(clamping: Int((max(0, min(1, value)) * 255).rounded()))
    }

    /// The 38-byte `GeronimoInputEventType.gamepad` event, without any envelope.
    var event: [UInt8] {
        var body = Self.capturedBody
        func put16(_ value: UInt16, at offset: Int) {
            body[offset] = UInt8(truncatingIfNeeded: value)
            body[offset + 1] = UInt8(truncatingIfNeeded: value >> 8)
        }
        // Written from the packet rather than left to the captured literal, so the state packet
        // and the `0x20d` descriptor that registers the pad always name the same device and the
        // same set of connected pads.
        put16(gamepadIndex & 0x03, at: Self.gamepadIdOffset)
        put16(connectedBitmap, at: Self.connectedBitmapOffset)
        put16(buttons, at: Self.buttonsOffset)
        // One u16: left trigger low, right trigger high.
        put16(UInt16(leftTrigger) | (UInt16(rightTrigger) << 8), at: Self.triggersOffset)
        put16(UInt16(bitPattern: leftStickX), at: Self.leftStickXOffset)
        put16(UInt16(bitPattern: leftStickY), at: Self.leftStickYOffset)
        put16(UInt16(bitPattern: rightStickX), at: Self.rightStickXOffset)
        put16(UInt16(bitPattern: rightStickY), at: Self.rightStickYOffset)
        // Same clock and same units as the outer `0x23` timestamp: microseconds since the input
        // session started, which is what the vendored encoder passed straight through.
        for byte in 0..<8 {
            body[Self.timestampFieldOffset + byte] = UInt8(truncatingIfNeeded: timestampMicroseconds >> UInt64(byte * 8))
        }
        return body
    }

    /// `[0x23][u64 BE outer µs][0x26][gamepad index][u16 BE sequence][0x21][u16 BE 38][event]`,
    /// preserving the validated legacy encoder's length-prefixed partially-reliable envelope.
    public var payload: Data {
        var writer = NvstByteWriter(capacity: Self.payloadLength)
        writer.u8(GeronimoInputEnvelope.headerByte)
        writer.u64BE(timestampMicroseconds)
        writer.u8(GeronimoInputEnvelope.partiallyReliablePayloadTag)
        writer.u8(UInt8(truncatingIfNeeded: gamepadIndex & 0x03))
        writer.u16BE(sequence)
        writer.u8(GeronimoInputEnvelope.lengthPrefixedPayloadTag)
        writer.u16BE(UInt16(Self.eventLength))
        writer.bytes(event)
        return writer.data
    }

    public var command: NvstControlCommand {
        NvstControlCommand(code: Self.commandCode, payload: payload)
    }
}
