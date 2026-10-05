import Foundation

/// Sends RTCP Receiver Reports and PLI over the bundle's `rtcp_on_sctp_private` SCTP data
/// channel, sustaining the seat's video send window ("RTCP over SCTP is a must for One SDK
/// video to function").
///
/// Two facts matter and are easy to get wrong:
/// - The channel label is `rtcp_on_sctp_private`, **not** `rtcp1`. The seat resets a DCEP open
///   whose label it does not recognise.
/// - Payloads on this channel are **plain** RTCP: DTLS already encrypts the SCTP association,
///   so no SRTCP layer is applied here. SRTCP (`NvstSrtcp`) belongs to the raw Mjolnir UDP
///   socket instead.
///
/// The transport owns the channel; this type owns cadence and payload construction.
public final class NvstFeedbackSender: @unchecked Sendable {
    /// Official feedback channel label. `rtcp1` is refused by the seat.
    public static let channelLabel = "rtcp_on_sctp_private"
    /// The seat resets a DCEP open on an unexpected SCTP stream id, so the official client's
    /// channel is opened across the low even ids until one survives.
    public static let streamIdentifierCandidates = 8

    public enum FeedbackError: LocalizedError, Equatable, Sendable {
        case notConfigured

        public var errorDescription: String? {
            switch self {
            case .notConfigured: "Feedback sender is not configured with a channel writer."
            }
        }
    }

    let lock = NSLock()
    private var write: (@Sendable (Data) -> Void)?
    private var timer: DispatchSourceTimer?
    let queue = DispatchQueue(label: "com.opennow.nvst.feedback")
    private var senderSSRC: UInt32 = 0
    private var mediaSSRC: UInt32 = 0
    private var highestExtendedSequence: UInt32 = 0
    private var cumulativeLost: UInt32 = 0
    private var fractionLost: UInt8 = 0
    private var interarrivalJitter: UInt32 = 0
    private var reportProvider: (@Sendable () -> NvstRtcpReportBlock?)?
    private var keyframeRequested = false
    private var reportsSent: UInt64 = 0
    private let interval: TimeInterval

    public init(interval: TimeInterval = 1.0) {
        self.interval = interval
    }

    public func configure(channelWriter: @escaping @Sendable (Data) -> Void,
                          senderSSRC: UInt32,
                          mediaSSRC: UInt32) {
        withLock {
            self.write = channelWriter
            self.senderSSRC = senderSSRC
            self.mediaSSRC = mediaSSRC
        }
    }

    /// The media SSRC is only known once the first authenticated video packet arrives.
    public func updateMediaSSRC(_ ssrc: UInt32) {
        withLock { self.mediaSSRC = ssrc }
    }

    public var sentReportCount: UInt64 { withLock { reportsSent } }

    public func start() {
        lock.lock()
        defer { lock.unlock() }
        guard timer == nil else { return }
        let timer = DispatchSource.makeTimerSource(queue: queue)
        timer.schedule(deadline: .now() + interval, repeating: interval, leeway: .milliseconds(100))
        timer.setEventHandler { [weak self] in
            self?.emit()
        }
        timer.resume()
        self.timer = timer
    }

    public func stop() {
        lock.lock()
        defer { lock.unlock() }
        timer?.cancel()
        timer = nil
    }

    /// Fresh report blocks straight from the receiver, consulted on every emit. When set, the
    /// RR carries the receiver's real RFC 3550 fraction-lost and jitter instead of the last
    /// `updateMediaState` snapshot.
    public func setReportProvider(_ provider: (@Sendable () -> NvstRtcpReportBlock?)?) {
        withLock { reportProvider = provider }
    }

    /// Updates the receiver-fed media stream state for the next RR.
    public func updateMediaState(highestExtendedSequence: UInt32, cumulativeLost: UInt32,
                                 fractionLost: UInt8 = 0, interarrivalJitter: UInt32 = 0) {
        withLock {
            self.highestExtendedSequence = max(self.highestExtendedSequence, highestExtendedSequence)
            self.cumulativeLost = cumulativeLost
            self.fractionLost = fractionLost
            self.interarrivalJitter = interarrivalJitter
        }
    }

    /// Arms a PLI on the next cadence tick. Loss recovery is what the seat answers with a
    /// fresh keyframe.
    public func requestKeyframe() {
        withLock { keyframeRequested = true }
    }

    /// Sends a PLI immediately (used when the reassembler reports an unrecoverable gap).
    public func sendKeyframeRequestNow() throws {
        var channelWriter: (@Sendable (Data) -> Void)?
        var sender: UInt32 = 0
        var media: UInt32 = 0
        withLock {
            channelWriter = write
            sender = senderSSRC
            media = mediaSSRC
        }
        guard let channelWriter else { throw FeedbackError.notConfigured }
        channelWriter(NvstRtcp.pictureLossIndication(senderSSRC: sender, mediaSSRC: media))
    }

    /// One cadence tick: the RR always, plus a PLI when a keyframe was requested.
    func nextPayloads() -> [Data] {
        var sender: UInt32 = 0
        var media: UInt32 = 0
        var highest: UInt32 = 0
        var lost: UInt32 = 0
        var fraction: UInt8 = 0
        var jitter: UInt32 = 0
        var wantsKeyframe = false
        var provider: (@Sendable () -> NvstRtcpReportBlock?)?
        withLock {
            sender = senderSSRC
            media = mediaSSRC
            highest = highestExtendedSequence
            lost = cumulativeLost
            fraction = fractionLost
            jitter = interarrivalJitter
            wantsKeyframe = keyframeRequested
            keyframeRequested = false
            provider = reportProvider
        }
        // Real reception statistics when the receiver is wired in: a flat-zero fraction-lost and
        // jitter tell the seat's congestion controller the network is perfect no matter what it
        // actually did, which is indistinguishable from telling it nothing.
        let block = provider?() ?? NvstRtcpReportBlock(
            sourceSSRC: media,
            fractionLost: fraction,
            cumulativeLost: lost,
            extendedHighestSequence: highest,
            interarrivalJitter: jitter
        )
        var payloads = [NvstRtcp.receiverReport(ssrc: sender, blocks: [block])]
        if wantsKeyframe {
            payloads.append(NvstRtcp.pictureLossIndication(senderSSRC: sender, mediaSSRC: media))
        }
        return payloads
    }

    /// Test seam for one cadence tick.
    func emitForTesting() { emit() }

    private func emit() {
        let (channelWriter, media) = withLock { (write, mediaSSRC) }
        guard let channelWriter else { return }
        // A Receiver Report about SSRC 0 describes a stream that does not exist. Upstream gates its
        // reports on a bound SSRC for the same reason, and a seat that resets the stream on a
        // malformed report takes the whole DTLS association with it.
        guard media != 0 else { return }
        for payload in nextPayloads() {
            channelWriter(payload)
        }
        withLock { reportsSent &+= 1 }
    }

    private func withLock<T>(_ body: () -> T) -> T {
        lock.lock()
        defer { lock.unlock() }
        return body()
    }
}
