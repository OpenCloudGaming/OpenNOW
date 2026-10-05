import Foundation

/// What actually arrived on a UDP socket, by datagram class.
///
/// The decisive question on a silent NVST socket is not "did SRTP authenticate" but "did anything
/// arrive at all, and was it STUN or media". Counting by class separates NAT traversal failure
/// (nothing, or only our own retransmits) from a seat that answers STUN but never starts media.
public struct NvstInboundCounters: Equatable, Sendable {
    public var stunRequests = 0
    public var stunSuccessResponses = 0
    public var stunErrorResponses = 0
    public var dtls = 0
    public var rtp = 0
    public var other = 0
    public var bytes = 0
    /// Largest datagram seen, and how many exceeded a typical tunnelled MTU. Oversized packets on
    /// a path that cannot carry them look exactly like random loss.
    public var largestDatagram = 0
    public var datagramsOver1400 = 0
    public var punchesSent = 0
    public var responsesSent = 0
    /// Datagrams per remote `ip:port`, capped at a handful of entries. Which seat port the video
    /// actually arrives from is the one fact the punch destination has to match.
    public var datagramsByPeer: [String: UInt64] = [:]
    /// How many datagrams each wake-up drained. Wake-ups that keep hitting the per-wake-up cap
    /// mean the socket always has a backlog and this reader, not the sender, is setting the packet
    /// rate — which is how a 22 ms-per-packet receive path went unnoticed as "a slow seat".
    public var datagramsPerWakeUp: [Int: UInt64] = [:]
    /// Datagrams accepted in each whole second, so a run reports the highest rate it actually
    /// sustained rather than only its average. Gameplay is far burstier than a menu.
    public var datagramsPerSecond: [Int] = []
    /// Nanoseconds spent inside the drain loop, and inside the per-datagram pipeline within it.
    /// If the loop owns most of the wall clock, the packet rate is ours to set, not the seat's.
    public var drainNanoseconds: UInt64 = 0
    public var processNanoseconds: UInt64 = 0
    public var handlerNanoseconds: UInt64 = 0
    /// SRTCP receiver reports sent back on this socket. A video sender that never hears one backs
    /// its bitrate off, which looks like a slow stream rather than a missing feedback plane.
    public var receiverReportsSent = 0
    /// Whether the report timer was armed, and how often it fired. Separates "never scheduled"
    /// from "scheduled but produced nothing".
    public var receiverReportTimerArmed = false
    public var receiverReportPolls = 0
    /// `sendto` failures. A punch counter that only counts attempts cannot tell a NAT that never
    /// answers from a socket that never actually emitted a packet.
    public var sendFailures = 0
    /// `errno` of the first failed send, so the reason survives without flooding the log.
    public var firstSendError: Int32 = 0

    public init() {}

    public var total: Int {
        stunRequests + stunSuccessResponses + stunErrorResponses + dtls + rtp + other
    }

    public var summary: String {
        "in=\(total) stunReq=\(stunRequests) stunOk=\(stunSuccessResponses) stunErr=\(stunErrorResponses) dtls=\(dtls) rtp=\(rtp) other=\(other) bytes=\(bytes) punchesOut=\(punchesSent) pongsOut=\(responsesSent) rrOut=\(receiverReportsSent) rrArmed=\(receiverReportTimerArmed) rrPolls=\(receiverReportPolls) maxDatagram=\(largestDatagram) over1400=\(datagramsOver1400) peakPktPerSec=\(datagramsPerSecond.max() ?? 0) drainMs=\(drainNanoseconds / 1_000_000) processMs=\(processNanoseconds / 1_000_000) handlerMs=\(handlerNanoseconds / 1_000_000) perWake=\(datagramsPerWakeUp.sorted { $0.key < $1.key }.map { "\($0.key):\($0.value)" }.joined(separator: ",")) sendFailed=\(sendFailures)\(firstSendError == 0 ? "" : " firstErrno=\(firstSendError)") peers=\(datagramsByPeer.sorted { $0.value > $1.value }.map { "\($0.key)x\($0.value)" }.joined(separator: ","))"
    }

    /// Classifies one datagram. STUN message classes come from the two bits above the method
    /// (RFC 5389 §6): 0x000 request, 0x100 success, 0x110 error.
    public mutating func recordPeer(_ endpoint: String) {
        if datagramsByPeer[endpoint] != nil || datagramsByPeer.count < 6 {
            datagramsByPeer[endpoint, default: 0] += 1
        }
    }

    public mutating func record(datagram: Data, at second: Int = -1) {
        if second >= 0, second < 3600 {
            if datagramsPerSecond.count <= second {
                datagramsPerSecond.append(contentsOf: Array(repeating: 0, count: second + 1 - datagramsPerSecond.count))
            }
            datagramsPerSecond[second] += 1
        }
        bytes += datagram.count
        largestDatagram = max(largestDatagram, datagram.count)
        if datagram.count > 1400 { datagramsOver1400 += 1 }
        guard NvstDatagramClassifier.looksLikeSTUN(datagram) else {
            if NvstDatagramClassifier.looksLikeDTLS(datagram) {
                dtls += 1
            } else if NvstDatagramClassifier.looksLikeRTP(datagram) {
                rtp += 1
            } else {
                other += 1
            }
            return
        }
        let messageType = (UInt16(datagram[datagram.startIndex]) << 8) | UInt16(datagram[datagram.startIndex + 1])
        // RFC 5389 message class lives in bits 4 and 8: 0x0000 request, 0x0010 indication,
        // 0x0100 success, 0x0110 error. Indications fold into the request bucket so the error
        // bucket only ever counts real error responses.
        switch messageType & 0x0110 {
        case 0x0000, 0x0010: stunRequests += 1
        case 0x0100: stunSuccessResponses += 1
        default: stunErrorResponses += 1
        }
    }
}
