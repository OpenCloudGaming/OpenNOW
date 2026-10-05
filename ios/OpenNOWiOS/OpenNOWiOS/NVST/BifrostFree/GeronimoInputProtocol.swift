/// The event-type word in a Geronimo input packet. The space is shared across directions but
/// not symmetric: ids verified against the official client's `RiClientBackend::Get*PacketId`
/// dispatch (arm64 disassembly, `docs/NVST/OfficialClientAudit.md`) are documented with their
/// direction.
public enum GeronimoInputEventType: UInt32, CaseIterable, Sendable {
    case heartbeat = 2
    case keyDown = 3
    case keyUp = 4
    case mouseAbsolute = 5
    /// Flag 0x1000 form of the absolute move, carrying two extra u16 fields.
    case mouseAbsoluteExtended = 6
    case mouseRelative = 7
    case mouseButtonDown = 8
    case mouseButtonUp = 9
    case mouseWheel = 10
    /// The 20-byte pre-XInput gamepad form; the modern 34-byte form is `gamepad`.
    case gamepadLegacy = 11
    case gamepad = 12
    /// Client-to-server gamepad haptics state (`GetHapticsPacketId`); the server-to-client
    /// haptic EVENT arrives as `haptic`.
    case hapticsState = 13
    /// Server-to-client haptic event, as received through the vendored bridge. In the
    /// client-to-server RI space 15 is a window-focus event; the directions do not share
    /// this id's meaning.
    case haptic = 15
    case windowGeometry = 16
    /// DS4/DS5-class HID report; the rumble/gyro path rides here rather than in RI packets.
    case hidReport = 17
    case hidChange = 18
    /// Caps/num/scroll state, a 1-byte bitfield (0x10/0x20/0x40).
    case lockKeys = 19
    case imeHotkey = 20
    case bulkPayloadPartiallyReliable = 21
    case touch = 22
    case utf8Text = 23
    case lowLevelTouch = 24
    case mouseSettings = 25
}

public enum GeronimoInputChannel: Sendable {
    public static let reliableLabel = "input_channel_v1"
    public static let partiallyReliableLabel = "input_channel_partially_reliable"
    public static let partialReliableInputLifetimeMs: Int32 = 5
    public static let partialReliableInputBacklogLimitBytes: UInt64 = 16 * 1024
    public static let mouseInputBacklogLimitBytes: UInt64 = 512
    public static let gamepadInputBacklogLimitBytes: UInt64 = 512
}

public enum GeronimoInputEnvelope: Sendable {
    public static let headerByte: UInt8 = 0x23
    public static let lengthPrefixedPayloadTag: UInt8 = 0x21
    public static let singleReliablePayloadTag: UInt8 = 0x22
    public static let partiallyReliablePayloadTag: UInt8 = 0x26
}

public enum GeronimoInputHandshake: Sendable {
    public static let littleEndianVersionMarker: UInt16 = 526
    public static let leadingVersionByte: UInt8 = 0x0e
}
