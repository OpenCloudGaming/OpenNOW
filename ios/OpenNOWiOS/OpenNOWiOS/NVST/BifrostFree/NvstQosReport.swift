import Foundation

/// The client's QoS feedback report, command `0x207`.
///
/// Recovered from 532 captured reports from the official client (its own `SSL_write`). Every one is
/// exactly 52 bytes — an earlier reading of "variable length" was measuring whole SCTP payloads
/// carrying several commands, not the declared command length.
///
/// This is the report the seat's rate control actually reads. It advertises
/// `bw.txRxLag.minFeedbackTxDeltaMs:200` and `bwe.useOwdCongestionControl:1`, and without these
/// samples a delay-based controller has no evidence the path is healthy: measured 48 packets per
/// second against the official client's 401 on the same title.
public struct NvstQosReport: Equatable, Sendable {
    /// Constant in every captured report.
    public static let version: UInt32 = 7
    /// The clock the timestamp field advances on, confirmed at 2,704,631 ticks across a 30.05 s
    /// session.
    public static let timestampClockRate: Double = 90_000
    /// Captured cadence: 532 reports across ~30 s.
    public static let interval: TimeInterval = 1.0 / 18.0
    /// Two 16-bit fields that never move across the capture.
    static let constantPair: UInt16 = 1000
    /// The flat capability the capture reports.
    public static let defaultLinkCapabilityKbps: UInt16 = 12708

    public let sequence: UInt32
    /// Frames, not packets. The capture's own arithmetic settles it: 2,925,246 bytes over 1789
    /// units is 1635 bytes each, which is above the 1280-byte `video[0].packetSize` the session
    /// announces, so a unit cannot be a packet. The per-frame `0x204` sizes independently sum to
    /// 2,914,715 bytes over the same session.
    public let framesReceived: UInt32
    public let bytesReceived: UInt32
    /// The client's link capability in kbps, which the capture holds flat at 12708 for 528 of its
    /// 532 reports — while that same session was only receiving 780 kbps. It is a capability
    /// figure, not a measurement. Reporting our own observed throughput here instead closes the
    /// rate-control loop at whatever we are currently being given: runs that reported observed
    /// throughput measured 215-297 frames per 30 s against 412-503 for runs that sent no report
    /// at all.
    public let linkCapabilityKbps: UInt16
    /// The RTP timestamp of the most recently received packet — the client's position in the
    /// seat's own media clock, which is what `vqos[0].bw.txRxLag` differences against its send
    /// position. Two things rule out the session-relative elapsed time this first reported: the
    /// captured field advances at 90,154 ticks per second, matching the 90 kHz RTP clock and not
    /// any client-side unit, and its very first sample is already 1,794,376 ticks — 19.9 seconds —
    /// on a report sent at session start, which no elapsed-time counter can be.
    public let rtpTimestamp: UInt32
    public let previousBytesReceived: UInt32
    /// `+20`: the per-report one-way-delay sample the seat's OWD rate control reads, in
    /// microseconds. The capture ranges 0…917 with a mean of 174 and is ~0 through warm-up.
    public let delayMicroseconds: UInt32
    /// `+24`: the same shape, smaller — capture mean 59. Modelled as how much the delay sample
    /// MOVED since the last report, which is the trend an OWD controller acts on and is small by
    /// construction.
    public let delayTrendMicroseconds: UInt32
    /// `+44`: bits received during this report interval. The capture holds ~48–50k steady on a
    /// session receiving about 780 kbps, and 780000 / 18 reports per second is 43k — a rate
    /// estimate per interval, not a cumulative count, which also matches its 0 through warm-up.
    public let intervalBits: UInt32
    /// The capture's flag at `+28`: 0 for its first 34 reports and 2 for the remaining 498. At the
    /// captured 18 Hz cadence that is a flip about 1.9 seconds in. What it actually reports is not
    /// established, so this models the observed timing and nothing more.
    public let isWarmedUp: Bool

    public init(sequence: UInt32,
                framesReceived: UInt32,
                bytesReceived: UInt32,
                linkCapabilityKbps: UInt16 = NvstQosReport.defaultLinkCapabilityKbps,
                rtpTimestamp: UInt32,
                previousBytesReceived: UInt32,
                delayMicroseconds: UInt32 = 0,
                delayTrendMicroseconds: UInt32 = 0,
                intervalBits: UInt32 = 0,
                isWarmedUp: Bool = true) {
        self.sequence = sequence
        self.framesReceived = framesReceived
        self.bytesReceived = bytesReceived
        self.linkCapabilityKbps = linkCapabilityKbps
        self.rtpTimestamp = rtpTimestamp
        self.previousBytesReceived = previousBytesReceived
        self.delayMicroseconds = delayMicroseconds
        self.delayTrendMicroseconds = delayTrendMicroseconds
        self.intervalBits = intervalBits
        self.isWarmedUp = isWarmedUp
    }

    /// How long the capture takes to flip `isWarmedUp`.
    public static let warmUpSeconds: TimeInterval = 1.9

    /// The 52-byte payload, all fields little-endian.
    public var payload: Data {
        var writer = NvstByteWriter(capacity: 52)
        writer.u32LE(Self.version)          // +0
        writer.u32LE(0)                     // +4
        writer.u32LE(sequence)              // +8
        writer.u32LE(framesReceived)        // +12
        writer.u32LE(bytesReceived)         // +16
        // +20/+24/+44 decoded from 781 real reports in the official client's SSL_write tap
        // (2026-08-24):
        //   +20: 0…917, mean 174, non-cumulative, ~0 during the first ~2 warm-up reports.
        //   +24: 0…644, mean 59, same shape, smaller.
        //   +44: 0 during warm-up then ~48k…50k steady (0…251741 over the run).
        //   +40: 0 in every report (so we are right to send 0 there).
        // These three were sent as zero for as long as this client existed, which reads to a
        // delay-based controller as a path with no measurable delay and no measurable throughput.
        // The vendored client, on the same seat and network, holds a steady 120 fps where ours
        // averages 91 — and these are the last documented difference between its feedback and ours.
        // Filled from what we actually measure: RTCP interarrival jitter for the delay samples, and
        // bits carried since the previous report for the rate estimate.
        writer.u32LE(delayMicroseconds)     // +20
        writer.u32LE(delayTrendMicroseconds) // +24
        writer.u16LE(isWarmedUp ? 2 : 0)  // +28
        writer.u16LE(Self.constantPair)   // +30
        writer.u16LE(Self.constantPair)   // +32
        writer.u16LE(linkCapabilityKbps)  // +34
        writer.u32LE(rtpTimestamp)          // +36
        writer.u32LE(0)                     // +40  (vendor: always 0)
        writer.u32LE(intervalBits)          // +44
        writer.u32LE(previousBytesReceived) // +48
        return writer.data
    }

    public var command: NvstControlCommand {
        NvstControlCommand(code: .qosFeedback, payload: payload)
    }
}
