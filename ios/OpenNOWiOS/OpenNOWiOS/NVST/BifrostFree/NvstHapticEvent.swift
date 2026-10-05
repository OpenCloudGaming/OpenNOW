import Foundation

/// One rumble instruction from the seat for one gamepad slot.
///
/// The seat sends these as control command `0x010b` on `control_channel_reliable`. Recovered from
/// the official client (arm64 `libBifrost2` `ServerControl::handleServerCommand`, jump-table case
/// `0x10b` → `NvstClientEvent_t` type 7 → `NVB::Streamer::onNvscEvent` → `NVB_EVT_HAPTIC_EVENT`,
/// then `libGeronimo` `ForgeGamepad::processEvent` → `Forge_setRumbleState`):
///
/// ```
/// payload = [u16 LE kind][u16 LE blobLength][blob]
/// kind 1: blob = { u16 LE gamepadIndex, u16 LE leftMotor, u16 LE rightMotor } × N      (6 bytes each)
/// kind 2: blob = { u16 LE gamepadIndex, u16 LE leftMotor, u16 LE rightMotor, u16 LE ms } × N (8 bytes each)
/// ```
///
/// Motor values are 16-bit XInput amplitudes; Geronimo scales them `>> 8` for SDL and treats a zero
/// duration as 1000 ms. A kind-1 record has no duration: it is a *state*, held until the next
/// record for the same pad (games refresh it every frame while rumbling and send zeros to stop).
/// Records for slots above 3 are ignored, as the official client ignores them.
///
/// The seat only sends these after the client has enabled haptics with RI packet type 13
/// (`NvstRemoteInput.hapticsState(enabled:)`), which `RiClientBackend::enableHaptics` writes.
public struct NvstHapticEvent: Equatable, Sendable {
    /// Command `0x010b`. Was labelled "nvsc-client-event, likely haptics"; the disassembly settles it.
    public static let commandCode = NvstControlCommandCode.hapticEvent
    /// Kind 1: a rumble state with no duration.
    public static let stateKind: UInt16 = 1
    /// Kind 2: a rumble pulse with an explicit duration.
    public static let pulseKind: UInt16 = 2
    /// What Geronimo substitutes for a zero or absent duration.
    public static let defaultDurationMilliseconds: UInt16 = 1000

    public let gamepadIndex: UInt16
    /// Low-frequency / left-handle motor, `0...65535`.
    public let leftMotor: UInt16
    /// High-frequency / right-handle motor, `0...65535`.
    public let rightMotor: UInt16
    /// Nil for a kind-1 state record.
    public let durationMilliseconds: UInt16?

    public init(gamepadIndex: UInt16, leftMotor: UInt16, rightMotor: UInt16, durationMilliseconds: UInt16? = nil) {
        self.gamepadIndex = gamepadIndex
        self.leftMotor = leftMotor
        self.rightMotor = rightMotor
        self.durationMilliseconds = durationMilliseconds
    }

    /// The duration to hold the motors for, with Geronimo's default applied.
    public var effectiveDurationMilliseconds: UInt16 {
        guard let durationMilliseconds, durationMilliseconds > 0 else { return Self.defaultDurationMilliseconds }
        return durationMilliseconds
    }

    public var isSilent: Bool { leftMotor == 0 && rightMotor == 0 }

    /// Every record in one haptic command, or nil when `command` is not a haptic command. A haptic
    /// command whose blob is shorter than its declared length yields the records that fit; a kind
    /// this client does not know yields an empty array (still a haptic command, so the caller can
    /// log it rather than treat it as unparsed).
    public static func parse(_ command: NvstControlCommand) -> [NvstHapticEvent]? {
        guard command.code == commandCode else { return nil }
        var reader = NvstByteReader(command.payload)
        guard let kind = try? reader.u16LE(), let declaredLength = try? reader.u16LE() else { return [] }
        let recordLength: Int
        switch kind {
        case stateKind: recordLength = 6
        case pulseKind: recordLength = 8
        default: return []
        }
        let available = min(Int(declaredLength), reader.remaining)
        var events: [NvstHapticEvent] = []
        for _ in 0 ..< (available / recordLength) {
            guard let index = try? reader.u16LE(),
                  let left = try? reader.u16LE(),
                  let right = try? reader.u16LE() else { break }
            var duration: UInt16?
            if kind == pulseKind {
                guard let value = try? reader.u16LE() else { break }
                duration = value
            }
            guard index < 4 else { continue }
            events.append(NvstHapticEvent(gamepadIndex: index, leftMotor: left, rightMotor: right, durationMilliseconds: duration))
        }
        return events
    }

    /// The wire payload for a haptic command — the inverse of `parse`, for tests and fixtures.
    public static func payload(kind: UInt16, events: [NvstHapticEvent]) -> Data {
        let recordLength = kind == pulseKind ? 8 : 6
        var writer = NvstByteWriter(capacity: 4 + events.count * recordLength)
        writer.u16LE(kind)
        writer.u16LE(UInt16(clamping: events.count * recordLength))
        for event in events {
            writer.u16LE(event.gamepadIndex)
            writer.u16LE(event.leftMotor)
            writer.u16LE(event.rightMotor)
            if kind == pulseKind { writer.u16LE(event.durationMilliseconds ?? 0) }
        }
        return writer.data
    }

    public var summary: String {
        "pad=\(gamepadIndex) left=\(leftMotor) right=\(rightMotor) ms=\(durationMilliseconds.map(String.init) ?? "state")"
    }
}
