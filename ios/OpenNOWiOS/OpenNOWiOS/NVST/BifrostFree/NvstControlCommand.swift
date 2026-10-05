import Foundation

/// A control-plane encoding failure. The only way a command can fail to frame is a payload
/// that exceeds the 16-bit length field, which is a caller bug rather than wire data.
public enum NvstControlCommandError: LocalizedError, Equatable, Sendable {
    case payloadTooLarge(Int)

    public var errorDescription: String? {
        switch self {
        case .payloadTooLarge(let count):
            "NVST control payload of \(count) bytes does not fit the 16-bit length field."
        }
    }
}

/// A control-plane command code. The code space is fixed by NVIDIA's dispatcher but not
/// closed — seats add codes over time — so this is an extensible enum: recovered codes get
/// named statics, and anything else stays representable through its raw value.
public struct NvstControlCommandCode: Equatable, Hashable, Sendable,
                                      ExpressibleByIntegerLiteral, CustomStringConvertible {
    public let rawValue: UInt16

    public init(rawValue: UInt16) { self.rawValue = rawValue }
    public init(_ rawValue: UInt16) { self.init(rawValue: rawValue) }
    public init(integerLiteral value: UInt16) { self.init(rawValue: value) }

    public var description: String { String(format: "0x%04x", rawValue) }

    // Server-to-client notifications occupy 0x1xx.
    public static let qosInfo: NvstControlCommandCode = 0x0101
    public static let commandOutcomeReport: NvstControlCommandCode = 0x0102
    public static let terminationTimer: NvstControlCommandCode = 0x0103
    public static let gamepadHandling: NvstControlCommandCode = 0x0106
    public static let termination: NvstControlCommandCode = 0x0109
    /// Custom-message fragment, not an RI packet: inbound it becomes an
    /// `NvstMessageForClient_t` with msgType 7 via `signalOnServerMessage`; outbound the official
    /// `sendCustomMessage` picks 0x113 only for messageType 0xe and 0x10a otherwise.
    public static let customMessage: NvstControlCommandCode = 0x010a
    /// Gamepad rumble from the seat: `[u16 kind][u16 length][records]`, see `NvstHapticEvent`.
    /// The dispatcher turns it into `NvstClientEvent_t` type 7, which the NVB layer publishes as
    /// `NVB_EVT_HAPTIC_EVENT` (arm64 disassembly of `handleServerCommand` and `onNvscEvent`).
    public static let hapticEvent: NvstControlCommandCode = 0x010b
    /// Older name for `hapticEvent`, kept for the call sites that predate the disassembly.
    public static let nvscClientEvent: NvstControlCommandCode = hapticEvent
    /// Verified against the official `handleServerCommand` dispatch; this code was previously
    /// misread as HDR mode.
    public static let controllerScheme: NvstControlCommandCode = 0x010d
    public static let hdrMode: NvstControlCommandCode = 0x010e
    public static let systemCursor: NvstControlCommandCode = 0x010f
    /// "Server sent bitmap cursor info with ID: %u, size: %u" — the official client parses it;
    /// this resolves the standing ambiguity documented on `NvstRemoteCursor.bitmapCursorCode`.
    public static let bitmapCursor: NvstControlCommandCode = 0x0110
    public static let videoStreamProgress: NvstControlCommandCode = 0x0111
    public static let framePacingNotification: NvstControlCommandCode = 0x0112
    /// Custom-message fragment carrying msgType 8; see `customMessage`.
    public static let customMessageFinal: NvstControlCommandCode = 0x0113

    // The 0x2xx/0x3xx/0x4xx ranges are what the client sends.
    public static let pingBackAck: NvstControlCommandCode = 0x0200
    public static let audioStats: NvstControlCommandCode = 0x0202
    public static let clientProcessingTimes: NvstControlCommandCode = 0x0203
    public static let frameDecodedStats: NvstControlCommandCode = 0x0204
    public static let dcStats: NvstControlCommandCode = 0x0205
    /// Bidirectional remote-input command, not "cursor-detail"; inbound it carries PACKET_HID
    /// (0x11) and GENERIC_DEVICE_RESPONSE_EVENT (0x1a) subtypes.
    public static let remoteInput: NvstControlCommandCode = 0x0206
    public static let qosFeedback: NvstControlCommandCode = 0x0207
    /// 0x48-byte RTP stats report (`NvscClientPipeline::sendRtpStats`).
    public static let rtpStats: NvstControlCommandCode = 0x0208
    /// Reinforcement-learning congestion feedback (`NvstQosManager::obtainRlFeedback`); drives
    /// the seat's rate control directly.
    public static let rlFeedback: NvstControlCommandCode = 0x0209
    /// 0x1c-byte companion RTP stats report, sent alongside `rtpStats`.
    public static let rtpStatsCompact: NvstControlCommandCode = 0x020a
    public static let rtpNackToggle: NvstControlCommandCode = 0x020b
    public static let fecUpdate: NvstControlCommandCode = 0x020c
    public static let gamepadEvent: NvstControlCommandCode = 0x020d
    public static let inputProtocolVersion: NvstControlCommandCode = 0x020e
    public static let clientStats: NvstControlCommandCode = 0x020f
    /// ECN congestion feedback (`NvstQosManager::obtainEcnFeedback`).
    public static let ecnFeedback: NvstControlCommandCode = 0x0210
    public static let clientBlobContainers: NvstControlCommandCode = 0x0211
    public static let disconnectNotification: NvstControlCommandCode = 0x0300
    /// Sent before `disconnectNotification` when the teardown carries a non-zero reason:
    /// a single u32 reason word.
    public static let disconnectionReason: NvstControlCommandCode = 0x030b
    public static let frameInvalidationRange: NvstControlCommandCode = 0x0301
    public static let idrRequest: NvstControlCommandCode = 0x0302
    public static let frameRateChange: NvstControlCommandCode = 0x0303
    public static let statsRecording: NvstControlCommandCode = 0x0304
    public static let etwTraceControl: NvstControlCommandCode = 0x0305
    public static let networkCapture: NvstControlCommandCode = 0x0306
    public static let mouseCursorCapture: NvstControlCommandCode = 0x0308
    public static let nvwacInvalidation: NvstControlCommandCode = 0x030c
    /// Official symbol `sendServerMimicRemoteCursor`: whether the seat pushes cursor shape/mode
    /// notifications to the client.
    public static let mimicRemoteCursor: NvstControlCommandCode = 0x030d
    public static let serverContentCapture: NvstControlCommandCode = 0x030e
    public static let resolutionChange: NvstControlCommandCode = 0x030f
    public static let frameGenericStats: NvstControlCommandCode = 0x0310
    public static let remoteTracePrint: NvstControlCommandCode = 0x0311
    public static let recoveryMode: NvstControlCommandCode = 0x0312
    public static let perfIndicator: NvstControlCommandCode = 0x0314
    /// Control-channel statistics (`sendControlChannelStats`).
    public static let controlChannelStats: NvstControlCommandCode = 0x0313
    public static let networkThrottling: NvstControlCommandCode = 0x0315
    public static let packetSizeChange: NvstControlCommandCode = 0x0316
    public static let rtpNackRequest: NvstControlCommandCode = 0x0317
    public static let maxBitrateChange: NvstControlCommandCode = 0x0318
    public static let remoteTracePrintPair: NvstControlCommandCode = 0x031e
    public static let gsBlobDefs: NvstControlCommandCode = 0x031f
    public static let windowStateChange: NvstControlCommandCode = 0x0320
    public static let systemStateChange: NvstControlCommandCode = 0x0321
    public static let mimicCursorStrategy: NvstControlCommandCode = 0x0322
    public static let l4sStateChange: NvstControlCommandCode = 0x0323
    public static let videoQualitySnapshot: NvstControlCommandCode = 0x0324
    public static let vqsResponse: NvstControlCommandCode = 0x0325
    public static let qpDeltaMap: NvstControlCommandCode = 0x0327
    public static let qosPreferenceChange: NvstControlCommandCode = 0x0329
    public static let stutterIndicator: NvstControlCommandCode = 0x0405
    public static let riDeviceOverlay: NvstControlCommandCode = 0x0406
    /// Sent once at session start (`sessionController->sendAudioConfig`); pairs with the
    /// inbound `audioSurroundInfo` notification.
    public static let audioConfig: NvstControlCommandCode = 0x0407
    public static let audioSurroundInfo: NvstControlCommandCode = 0x0408

    /// The name recovered from the control-plane dispatcher, when this is a known code.
    public var name: String? {
        switch rawValue {
        case 0x101: "qos-info"
        case 0x102: "command-outcome"
        case 0x103, 0x104, 0x105: "termination-timer"
        case 0x106: "gamepad-handling"
        case 0x109: "termination"
        case 0x10a: "custom-message"
        case 0x10b: "haptic-event"
        case 0x10d: "controller-scheme"
        case 0x10e: "hdr-mode"
        case 0x10f: "cursor-info"
        case 0x110: "bitmap-cursor"
        case 0x111: "video-stream-progress"
        case 0x112: "frame-pacing"
        case 0x113: "custom-message-final"
        case 0x200: "ping-back-ack"
        case 0x202: "audio-stats"
        case 0x203: "client-processing-times"
        case 0x204: "frame-decoded-stats"
        case 0x205: "dc-stats"
        case 0x206: "ri-command"
        case 0x207: "qos-feedback"
        case 0x208: "rtp-stats"
        case 0x209: "rl-feedback"
        case 0x20a: "rtp-stats-compact"
        case 0x20b: "rtp-nack-toggle"
        case 0x20c: "fec-update"
        case 0x20d: "gamepad-event"
        case 0x20e: "input-protocol-version"
        case 0x20f: "client-stats"
        case 0x210: "ecn-feedback"
        case 0x211: "client-blob-containers"
        case 0x300: "disconnect-notification"
        case 0x30b: "disconnection-reason"
        case 0x301: "frame-invalidation-range"
        case 0x302: "idr-request"
        case 0x303: "frame-rate-change"
        case 0x304: "stats-recording"
        case 0x305: "etw-trace-control"
        case 0x306: "network-capture"
        case 0x308: "mouse-cursor-capture"
        case 0x30c: "nvwac-invalidation"
        case 0x30d: "mimic-remote-cursor"
        case 0x30e: "server-content-capture"
        case 0x30f: "resolution-change"
        case 0x310: "frame-generic-stats"
        case 0x311: "remote-trace-print"
        case 0x312: "recovery-mode"
        case 0x313: "control-channel-stats"
        case 0x314: "perf-indicator"
        case 0x315: "network-throttling"
        case 0x316: "packet-size-change"
        case 0x317: "rtp-nack-request"
        case 0x318: "max-bitrate-change"
        case 0x31e: "remote-trace-print-pair"
        case 0x31f: "gs-blob-defs"
        case 0x320: "window-state-change"
        case 0x321: "system-state-change"
        case 0x322: "mimic-cursor-strategy"
        case 0x323: "l4s-state-change"
        case 0x324: "video-quality-snapshot"
        case 0x325: "vqs-response"
        case 0x327: "qp-delta-map"
        case 0x329: "qos-preference-change"
        case 0x405: "stutter-indicator"
        case 0x406: "ri-device-overlay"
        case 0x407: "audio-config"
        case 0x408: "audio-surround-info"
        default: nil
        }
    }
}

/// One NVST control-plane command packet.
///
/// The wire format is fixed by `ReadCommandPacket`/`CommandPacketWrite` in NVIDIA's control
/// plane: a little-endian 16-bit command code, a little-endian 16-bit payload length, then the
/// payload. Several packets can share one SCTP message, so parsing walks the buffer.
public struct NvstControlCommand: Equatable, Sendable {
    public static let headerLength = 4

    public let code: NvstControlCommandCode
    public let payload: Data

    public init(code: NvstControlCommandCode, payload: Data) {
        self.code = code
        self.payload = payload
    }

    /// Parses every command packet in one channel message. A truncated trailing packet is
    /// reported through `trailing` rather than dropped, because a short read is itself evidence
    /// that the framing assumption is wrong.
    public static func parse(_ data: Data) -> (commands: [NvstControlCommand], trailing: Data) {
        var commands: [NvstControlCommand] = []
        var reader = NvstByteReader(data)
        while reader.remaining >= headerLength {
            var frame = reader
            guard let code = try? frame.u16LE(),
                  let length = try? frame.u16LE(),
                  let payload = try? frame.bytes(Int(length)) else { break }
            reader = frame
            commands.append(NvstControlCommand(code: NvstControlCommandCode(rawValue: code), payload: payload))
        }
        return (commands, reader.unread)
    }

    /// The packet as it goes on the wire. Throws instead of silently mis-framing when the payload
    /// does not fit the 16-bit length field: a clamped length with an unclamped body makes the
    /// receiver misparse every command that follows in the same SCTP message.
    public var encoded: Data {
        get throws {
            guard payload.count <= UInt16.max else {
                throw NvstControlCommandError.payloadTooLarge(payload.count)
            }
            var writer = NvstByteWriter(capacity: Self.headerLength + payload.count)
            writer.u16LE(code.rawValue)
            writer.u16LE(UInt16(payload.count))
            writer.bytes(payload)
            return writer.data
        }
    }

    /// The control-channel keepalive. `ServerControl::sendPingBackACK` writes command `0x200` with
    /// a single 32-bit stream value; the seat ends the session with `NVST_NETERR_CLIENT_TIMED_OUT`
    /// when it does not arrive inside `general.enetControlChannel.pingBackTimeoutMs` (10 s, with
    /// the client sending every 3 s).
    public static let pingBackAckCode = NvstControlCommandCode.pingBackAck
    public static let pingBackIntervalSeconds: TimeInterval = 3

    public static func pingBackAck(streamValue: UInt32) -> NvstControlCommand {
        var writer = NvstByteWriter(capacity: 4)
        writer.u32LE(streamValue)
        return NvstControlCommand(code: pingBackAckCode, payload: writer.data)
    }

    /// `ServerControl::sendIdrRequest`: command `0x302` with the stream index as a 16-bit
    /// little-endian payload. This is how a keyframe is asked for when the seat never opens
    /// `rtcp_on_sctp_private`, so a PLI has nowhere to go.
    public static let idrRequestCode = NvstControlCommandCode.idrRequest

    public static func idrRequest(streamIndex: UInt16 = 0) -> NvstControlCommand {
        var writer = NvstByteWriter(capacity: 2)
        writer.u16LE(streamIndex)
        return NvstControlCommand(code: idrRequestCode, payload: writer.data)
    }

    /// `ServerControl::sendFrameInvalidationRange`: command `0x301` with three 64-bit
    /// little-endian words — first frame, last frame, stream index. This is the recovery the
    /// official client is configured for (`video[0].refPicInvalidation:1`): rather than demanding a
    /// full IDR, it names the frames it could not decode so the encoder stops referencing them.
    public static let frameInvalidationRangeCode = NvstControlCommandCode.frameInvalidationRange

    public static func frameInvalidationRange(first: UInt64, last: UInt64, streamIndex: UInt64 = 0) -> NvstControlCommand {
        var writer = NvstByteWriter(capacity: 24)
        for word in [first, last, streamIndex] { writer.u64LE(word) }
        return NvstControlCommand(code: frameInvalidationRangeCode, payload: writer.data)
    }

    /// `ServerControl::sendWindowStateChange` / `sendSystemStateChange`: commands `0x320` and
    /// `0x321`, each a 12-byte payload of three little-endian 32-bit words. The official client
    /// sends both exactly once, shortly after PLAY, with zeros — its log reads
    /// `Window State Change: 0 requested at frame number: 0`.
    public static let windowStateChangeCode = NvstControlCommandCode.windowStateChange
    public static let systemStateChangeCode = NvstControlCommandCode.systemStateChange

    /// Three little-endian `UInt32`s: stream index, requested state, frame number — in that order.
    /// We previously wrote the state into the FIRST word, which only looked right because every
    /// value we sent was zero. It matters now that the window state is non-zero.
    static func stateChange(code: NvstControlCommandCode, streamIndex: UInt32 = 0, state: UInt32, frameNumber: UInt32) -> NvstControlCommand {
        var writer = NvstByteWriter(capacity: 12)
        for word in [streamIndex, state, frameNumber] { writer.u32LE(word) }
        return NvstControlCommand(code: code, payload: writer.data)
    }

    /// The desktop client announces window state 19 at frame zero once input is activated. An
    /// all-zero window state leaves the session looking inactive to the seat, which then never
    /// sends its system-cursor mode updates — so the client cannot know when to hide its own
    /// pointer. That silence is half of the double-cursor bug.
    public static let activeWindowState: UInt32 = 19

    public static func windowStateChange(state: UInt32 = activeWindowState, frameNumber: UInt32 = 0) -> NvstControlCommand {
        stateChange(code: windowStateChangeCode, state: state, frameNumber: frameNumber)
    }

    public static func systemStateChange(state: UInt32 = 0, frameNumber: UInt32 = 0) -> NvstControlCommand {
        stateChange(code: systemStateChangeCode, state: state, frameNumber: frameNumber)
    }

    /// The termination notification carries its `NvstResult_t` as a big-endian 32-bit word — the
    /// dispatcher byte-swaps it before formatting, so the wire order is network order.
    public var terminationReason: UInt32? {
        guard code == .termination, payload.count >= 4 else { return nil }
        var reader = NvstByteReader(payload)
        return try? reader.u32BE()
    }

    /// The command the server is reporting on, plus its outcome.
    public var commandOutcome: (command: UInt16, outcome: UInt16)? {
        guard code == .commandOutcomeReport, payload.count >= 4 else { return nil }
        var reader = NvstByteReader(payload)
        guard let command = try? reader.u16LE(), let outcome = try? reader.u16LE() else { return nil }
        return (command, outcome)
    }

    /// Whether the payload is printable text — the seat sends some control messages as JSON.
    public var isTextual: Bool {
        guard payload.count >= 2 else { return false }
        let sample = payload.prefix(64)
        let printable = sample.filter { $0 == 0x09 || $0 == 0x0a || $0 == 0x0d || ($0 >= 0x20 && $0 < 0x7f) }
        return printable.count == sample.count && (payload.first == UInt8(ascii: "{") || payload.first == UInt8(ascii: "["))
    }

    /// The payload decoded as UTF-8, bounded so one message cannot flood the log.
    public func text(limit: Int = 600) -> String {
        let body = payload.prefix(limit)
        let decoded = String(decoding: body, as: UTF8.self)
        return payload.count > limit ? decoded + "…(\(payload.count) bytes)" : decoded
    }

    /// A single log-line description: named code, payload size, and the decoded fields that say
    /// why a session is ending.
    public var summary: String {
        var text = code.description
        if let name = code.name { text += " (\(name))" }
        text += " len=\(payload.count)"
        if let reason = terminationReason { text += " reason=\(NvstResultCode.describe(reason))" }
        if let outcome = commandOutcome {
            text += String(format: " forCommand=0x%04x outcome=%u", outcome.command, outcome.outcome)
        }
        return text
    }
}
