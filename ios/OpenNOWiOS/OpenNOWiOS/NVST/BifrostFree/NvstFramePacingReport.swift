import Foundation

/// The client's frame-pacing feedback, command `0x203`.
///
/// Corrected 2026-08-28 against a real plaintext capture of the official client (SSL tap on
/// libBifrost2, 5120x2160@120 negotiated, real Expedition 33 gameplay, 1595 reports). The
/// original reading of this struct — from an earlier ~75 fps capture — got two fields backwards:
///
/// - `+16` is **16000 in every single report of both captures**, at 120 fps and at ~75 fps alike.
///   It does not move with the negotiated frame rate, so it cannot be "the target frame interval"
///   — it is a fixed reference the seat's pacer is built around regardless of session fps.
/// - `+24` was read as "a constant" because the old capture never left 13338. The new capture
///   never leaves 8333. `1_000_000 / 8333 = 120.01` and `1_000_000 / 13338 = 74.98` — this field
///   **is** the real target frame interval, and it tracks the session's actual negotiated fps.
///   Sending a hardcoded value here (this struct sent 13338 unconditionally) told the seat's
///   pacer to target ~75 fps no matter what the session actually negotiated.
/// - `+20` was read as a small, clamped pacing error (settling to a few hundred microseconds on
///   the old capture). The new capture never clamps: it ranges 6000–24000 us, often *above*
///   `+24`'s target, and starts at exactly `+16`'s value (16000) on the session's first report —
///   consistent with a raw measured frame time that has no prior sample to compare against yet,
///   not a bounded deviation.
///
/// `+8` (`groupCount`) is not "2 to 4" as the old capture suggested either: the new capture holds
/// 6–8, and the frame-number delta between consecutive reports matches it exactly, confirming
/// this is genuinely "frames since the last report" — the real reporting cadence is roughly every
/// 6–8 frames, not every 3rd.
///
/// Experiment F (2026-08-28): both captures were on the *same* one machine, so "fixed 16000 in
/// every report" was never actually tested against a client with a different real display. A
/// disassembly of Geronimo (`libGeronimo.dylib`) settled it: it links `CVDisplayLink` and its own
/// strings say plainly what `+16` is — `"Pace server frames to match client vsync"`,
/// `"]: Client Vsync :"`, `CVDisplayLinkGetActualOutputVideoRefreshPeriod` imported and called.
/// `+16` is the client's own real display vsync interval, queried, not a hardcoded reference —
/// 16000 us (~62.5 Hz) merely because that's what the capturing machine's display reported. On a
/// genuine 120 Hz display this should be ~8333 us, and the seat's `framePacing.mode:1` (which this
/// codebase already announces) paces its own frame generation to match it.
public struct NvstFramePacingReport: Equatable, Sendable {
    public static let commandCode = NvstControlCommandCode.clientProcessingTimes
    public static let payloadLength = 28
    static let version: UInt32 = 5
    /// The captured client reports one of these roughly every 6-8 frames.
    public static let framesPerReport: UInt32 = 7

    /// The frame this report is about.
    public let frameNumber: UInt32
    /// The session's real target frame interval in microseconds — 8333 for 120 fps, tracks
    /// whatever was actually negotiated. Goes on the wire at `+24`.
    public let targetFrameTimeMicroseconds: UInt32
    /// The client's raw measured frame time in microseconds, unclamped. Goes on the wire at
    /// `+20`. Can legitimately exceed `targetFrameTimeMicroseconds`.
    public let measuredFrameTimeMicroseconds: UInt32
    /// The client's real display vsync interval in microseconds — see the type doc's Experiment
    /// F. Goes on the wire at `+16`.
    public let displayVsyncMicroseconds: UInt32
    /// Frames since the previous report — the wire's `+8`.
    public let groupCount: UInt32

    /// `displayVsyncMicroseconds` has no default on purpose: it must be the client's real
    /// display vsync or the seat's pacer targets the wrong cadence (see the type doc's
    /// Experiment F), and a default silently reintroduced that bug once before.
    public init(frameNumber: UInt32,
                targetFrameTimeMicroseconds: UInt32,
                measuredFrameTimeMicroseconds: UInt32,
                displayVsyncMicroseconds: UInt32,
                groupCount: UInt32 = framesPerReport) {
        self.frameNumber = frameNumber
        self.targetFrameTimeMicroseconds = targetFrameTimeMicroseconds
        self.measuredFrameTimeMicroseconds = measuredFrameTimeMicroseconds
        self.displayVsyncMicroseconds = displayVsyncMicroseconds
        self.groupCount = groupCount
    }

    public var payload: Data {
        var writer = NvstByteWriter(capacity: Self.payloadLength)
        writer.u32LE(Self.version)
        writer.zeroes(4)
        writer.u32LE(groupCount)
        writer.u32LE(frameNumber)
        writer.u32LE(displayVsyncMicroseconds)
        writer.u32LE(measuredFrameTimeMicroseconds)
        writer.u32LE(targetFrameTimeMicroseconds)
        return writer.data
    }

    public var command: NvstControlCommand {
        NvstControlCommand(code: Self.commandCode, payload: payload)
    }
}
