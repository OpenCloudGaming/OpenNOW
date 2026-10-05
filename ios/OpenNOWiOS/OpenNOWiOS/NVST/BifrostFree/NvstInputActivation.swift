import Foundation

/// The two commands the native stack sends between the seat's input-protocol handshake and its
/// first input event.
///
/// The full chain, read off the native stack's own control channel in order:
/// `0x20b` with the enable clear, `0x20d`, `0x308`, `0x30d`, `0x320`, `0x321`, then `0x20b` with the enable
/// set — and only then the first `0x206`. Sending the pair without the rest, or in the wrong order,
/// is not the same handshake.
///
/// Recovered from plaintext captures of the native stack's own control channel. The order is
/// pinned by the timestamps the messages carry: in a session where the first input event is
/// stamped 23.18 s, `0x20d` goes out at 18.81 s and `0x20b` between them. Our remote-input packets
/// are byte-identical to the native stack's and the seat still never reacted to them, so this
/// pair is the missing prerequisite rather than the encoding.
public enum NvstInputActivation {
    public static let deviceCommandCode = NvstControlCommandCode.gamepadEvent
    public static let enableCommandCode = NvstControlCommandCode.rtpNackToggle
    /// `0x308` is NOT a "ready" ping — it is MOUSE CURSOR CAPTURE, and its one payload byte is a
    /// boolean. We had it as `ready` sending a zero byte, which told the seat to STOP compositing
    /// its cursor into the video while we carried on drawing our own: the double-cursor bug.
    /// OpenNOW's native client made the same misreading and corrected it; NVIDIA's own client
    /// enables capture for startup, waits for the first cursor notification, then disables it and
    /// renders the reported cursor locally.
    public static let mouseCursorCaptureCode = NvstControlCommandCode.mouseCursorCapture
    /// NVB feature type 8, "track remote cursor image" — distinct from capture. Bifrost keeps this
    /// on after turning capture back off so the seat keeps publishing cursor shape and mode
    /// changes. We were never sending it, so the seat had no reason to publish them at all.
    public static let mimicRemoteCursorCode = NvstControlCommandCode.mimicRemoteCursor

    /// The body of `0x20d` after its 9-byte `[0x23][BE u64 µs]` prefix, held byte for byte as
    /// captured. Every byte of it is identical across four samples taken from two different
    /// sessions, so none of it is session-derived; only the timestamp and the field this exposes
    /// as `descriptorIndex` move. What the body describes is not decoded — it is replayed.
    static let capturedDeviceBody: [UInt8] = [
        0x22, 0x0c, 0x00, 0x00, 0x00, 0x1a, 0x00, 0x00, 0x00, 0x02,
        0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x55, 0x00, 0x00,
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ]
    /// The CONNECTED BITMAP, a u16 at body[9..11] — the same field the state packet carries at its
    /// body[13..15]: bit i = gamepad i connected, bit (i+8) = that pad is XInput-style. It must
    /// match the state packet exactly, or the seat registers the device as one kind and then gets
    /// updates for another: the pad appears ("Xbox controller connected") and is immediately
    /// dropped. Only the low byte used to be written, which is how they drifted apart.
    static let descriptorIndexOffset = 9

    /// `0x20d`, the device descriptor. `0x23` then the session-relative microsecond clock as a
    /// big-endian 64-bit value — the same "versioned" prefix the remote-input path uses.
    /// The bitmap defaults to the gamepad state packet's own value, because the seat drops input
    /// from a device whose descriptor bitmap differs from the state packets it later receives.
    public static func deviceDescriptor(timestampMicroseconds: UInt64, connectedBitmap: UInt16 = NvstGamepadPacket.connectedBitmap) -> NvstControlCommand {
        var body = capturedDeviceBody
        body[descriptorIndexOffset] = UInt8(truncatingIfNeeded: connectedBitmap)
        body[descriptorIndexOffset + 1] = UInt8(truncatingIfNeeded: connectedBitmap >> 8)
        var writer = NvstByteWriter(capacity: 1 + 8 + body.count)
        writer.u8(GeronimoInputEnvelope.headerByte)
        writer.u64BE(timestampMicroseconds)
        writer.bytes(body)
        return NvstControlCommand(code: deviceCommandCode, payload: writer.data)
    }

    /// Whether the seat composites its own cursor into the encoded video.
    public static func mouseCursorCapture(isEnabled: Bool) -> NvstControlCommand {
        NvstControlCommand(code: mouseCursorCaptureCode, payload: Data([isEnabled ? 1 : 0]))
    }

    /// Whether the seat publishes cursor shape/mode notifications to us. Official symbol
    /// `sendServerMimicRemoteCursor`.
    public static func mimicRemoteCursor(isEnabled: Bool) -> NvstControlCommand {
        NvstControlCommand(code: mimicRemoteCursorCode, payload: Data([isEnabled ? 1 : 0]))
    }

    /// `0x20b`: three little-endian words. The captured samples are `[0, 1, 0]` early in a session
    /// and `[0, 122, 1]` / `[0, 124, 1]` immediately before input starts, so the last word reads as
    /// the enable and the middle one as a counter at the point it is sent.
    public static func enableInput(counter: UInt32, isEnabled: Bool = true) -> NvstControlCommand {
        var writer = NvstByteWriter(capacity: 12)
        writer.u32LE(0)
        writer.u32LE(counter)
        writer.u32LE(isEnabled ? 1 : 0)
        return NvstControlCommand(code: enableCommandCode, payload: writer.data)
    }
}
