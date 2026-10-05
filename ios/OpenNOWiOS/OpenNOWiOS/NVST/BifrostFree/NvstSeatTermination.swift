import Foundation

/// The seat's `0x0109` termination notification: its own verdict on why the session ended.
///
/// NVIDIA's dispatcher turns this command into the client's session-ended callback. It is the one
/// signal that separates a seat that has *decided* the session is over — the game exited, an
/// operator commanded it, the seat's frame grab failed — from a link that merely went quiet.
/// Reading it as a log line and nothing more left the client reconnecting to a seat that had
/// already ended the session: a title that quit to desktop produced a stall, then a full budget of
/// in-place recovery attempts against CloudMatch, and finally a "stream stalled" report about a
/// session the seat had explained three seconds earlier.
public struct NvstSeatTermination: Equatable, Sendable {
    /// `NVST_DISCONN_BIFROST_INITIATED_SESSION_PAUSE`. Arrives through the same notification, but
    /// the cloud session survives it and stays resumable, so it is not a session end.
    public static let sessionPauseResult: UInt32 = 0x8003_000e

    /// The `NvstResult_t` the seat reported.
    public let result: UInt32
    /// The notification's whole payload. Seats have been captured sending 4 and 8 bytes and only
    /// the first word is understood, so the size stays part of the description rather than being
    /// quietly dropped.
    public let payload: Data

    public init(result: UInt32, payload: Data) {
        self.result = result
        self.payload = payload
    }

    /// The termination notification is the one command whose reason word is network order; the
    /// byte-order rule lives with the command that carries it.
    public static func parse(_ command: NvstControlCommand) -> NvstSeatTermination? {
        guard let result = command.terminationReason else { return nil }
        return NvstSeatTermination(result: result, payload: command.payload)
    }

    public var resultName: String? { NvstResultCode.name(for: result) }

    /// Whether the cloud session survives this notification and can be resumed.
    public var isSessionPause: Bool { result == Self.sessionPauseResult }

    /// One log-line description: the named result, plus the payload size that produced it.
    public var summary: String { "\(NvstResultCode.describe(result)) len=\(payload.count)" }
}
