import Foundation

/// The client's VSync mode, matching the official GeForce NOW client's Settings → Gameplay
/// control (a three-position picker: Off, On, Adaptive — the same trio the in-stream HUD cycles).
///
/// How the modes map onto the wire is inferred from three recovered sources, and each mapping is
/// documented at its property:
///
/// - The captured official ANNOUNCE (2026-08-24) always carries `framePacing.mode:1` —
///   `framePacing.feedbackMode:1` — and the official streaming profiles all pin their `vSync`
///   field to Adaptive, so the captured baseline *is* the Adaptive announce.
/// - libBifrost2's own strings name the feedback modes `FRAME_PACING_FEEDBACK_NEVER` /
///   `FRAME_PACING_FEEDBACK_INTERVAL`, and Geronimo's "Pace server frames to match client vsync"
///   describes what the interval feedback buys: the seat targets the vsync interval the client
///   reports in `NvstFramePacingReport` (+16).
/// - The official app's bridge exposes `SetVsyncEnabled` and `SetAdaptiveFramePacingEnabled` as
///   separate commands, so "On" (paced, no adaptive feedback) and "Adaptive" (paced, interval
///   feedback) are distinct states, with "Off" (not paced) third.
///
/// The seat-facing half (`framePacing.mode` / `framePacing.feedbackMode`) is fixed at ANNOUNCE —
/// the seat's pacer is configured there and there is no recovered control-plane command that
/// re-configures it mid-session (the `VSYNC_INTERVAL` feature the string
/// "Failed to set feature VSYNC_INTERVAL for stream %hu to %uus" names has not been recovered
/// into a command code). The client-facing half — whether `0x203` frame-pacing reports go out at
/// all, and which vsync interval they claim — changes live through `NvstVideoPipeline.applyVsyncMode`.
public enum NvstVsyncMode: Int, CaseIterable, Sendable, Equatable {
    case off = 0
    case on = 1
    case adaptive = 2

    /// The label the Settings row and the HUD show, as the official client's own UI names them.
    public var label: String {
        switch self {
        case .off: "Off"
        case .on: "On"
        case .adaptive: "Adaptive"
        }
    }

    /// `x-nv-video[0].framePacing.mode`: whether the seat's pacer is active at all. `1` (paced)
    /// is the captured official baseline; `0` lets the seat's encoder free-run.
    public var framePacingMode: String {
        switch self {
        case .off: "0"
        case .on, .adaptive: "1"
        }
    }

    /// `x-nv-video[0].framePacing.feedbackMode`: whether the seat consumes the client's interval
    /// feedback — `FRAME_PACING_FEEDBACK_INTERVAL` (1) or `FRAME_PACING_FEEDBACK_NEVER` (0).
    /// Only Adaptive asks for it, matching the official bridge's separate
    /// `SetAdaptiveFramePacingEnabled` switch.
    public var framePacingFeedbackMode: String {
        switch self {
        case .off, .on: "0"
        case .adaptive: "1"
        }
    }

    /// Whether the client keeps sending `0x203` frame-pacing reports. Only Adaptive feeds the
    /// seat the client's cadence (the baseline the official client always sends); Off and On
    /// leave the pacer on its announced target, so the reports would claim a cadence the seat
    /// was not configured to follow.
    public var isSendingFramePacingReports: Bool {
        switch self {
        case .off, .on: false
        case .adaptive: true
        }
    }

    /// The vsync interval the report's +16 field claims: the client's real display interval when
    /// the seat is asked to pace to the display, or the negotiated stream interval when it is
    /// only asked to hold a steady target.
    /// Follows `NvstFramePacingReport`'s type doc (Experiment F): the +16 interval is what the
    /// seat's pacer targets, so "On" must claim the session's own frame interval or the seat
    /// would pace to a display the client never asked it to match.
    var isReportingDisplayVsync: Bool {
        switch self {
        case .off, .on: false
        case .adaptive: true
        }
    }
}
