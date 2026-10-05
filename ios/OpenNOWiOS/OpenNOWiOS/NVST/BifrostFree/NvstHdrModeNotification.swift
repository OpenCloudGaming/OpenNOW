import Foundation

/// The seat's HDR mode notification, control command `0x010e`.
///
/// Recovered from the official client's `ServerControl::handleServerCommand` (arm64, jump-table
/// case `0x10e`): the payload is `[u32 LE mode][u32 LE detail]`. A mode above 2 is rejected with a
/// log line and no event; otherwise the dispatcher raises `NvstClientEvent_t` type 13 carrying the
/// mode, which the NVB layer republishes as `NVB_EVT_HDR_CHANGE_EVENT` and the official app uses to
/// switch its swap chain. The mode words themselves are not named in the binary; 0 is the
/// session's SDR start state and 1 arrives when a game switches its output to HDR. 2 is accepted
/// by the dispatcher and is presumably the "true HDR" (SDR-to-HDR tone mapping) mode the
/// `trueHdrParams.*` announce attributes describe — inferred, not captured.
public struct NvstHdrModeNotification: Equatable, Sendable {
    public static let commandCode = NvstControlCommandCode.hdrMode

    public enum Mode: UInt32, Equatable, Sendable {
        case sdr = 0
        case hdr = 1
        case trueHdr = 2
    }

    public let mode: Mode
    /// The second word. Logged by the official client next to the mode; its meaning is not recovered.
    public let detail: UInt32

    public init(mode: Mode, detail: UInt32 = 0) {
        self.mode = mode
        self.detail = detail
    }

    /// Nil when `command` is not an HDR mode notification or carries a mode the official client
    /// itself would reject.
    public static func parse(_ command: NvstControlCommand) -> NvstHdrModeNotification? {
        guard command.code == commandCode else { return nil }
        var reader = NvstByteReader(command.payload)
        guard let raw = try? reader.u32LE(), let mode = Mode(rawValue: raw) else { return nil }
        let detail = (try? reader.u32LE()) ?? 0
        return NvstHdrModeNotification(mode: mode, detail: detail)
    }

    public var isHDR: Bool { mode != .sdr }

    public var summary: String {
        let name: String = switch mode {
        case .sdr: "sdr"
        case .hdr: "hdr"
        case .trueHdr: "true-hdr"
        }
        return "mode=\(name) detail=\(detail)"
    }
}
