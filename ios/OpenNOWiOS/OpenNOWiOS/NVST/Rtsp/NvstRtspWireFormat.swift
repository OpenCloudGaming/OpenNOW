import CryptoKit
import Foundation

/// RTSP-over-WebSocket wire format for the NVST `:322` control channel.
///
/// The NVST control plane is classic RTSP (OPTIONS → DESCRIBE → SETUP → ANNOUNCE → PLAY)
/// tunnelled inside a WebSocket that is upgraded over raw TLS. The upgrade must be shaped
/// exactly like the official client's Poco `WebSocket::connect` (`GET /rtsp`, `Content-Length: 0`,
/// `x-nv-sessionid`) — a stock HTTP client's extra headers get a `400` from the seat.
///
/// Independently observed on the wire; OpenNOW's MIT `opennow-stable` documents the same
/// shapes in `src/main/platforms/gfn/nvstRtsp/{websocketTransport,rtspClient}.ts`.
public enum NvstWebSocketUpgrade {
    /// Official upgrade target. The seat rejects `/` and the empty form with HTTP 400.
    public static let requestTarget = "/rtsp"
    private static let acceptGUID = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11"

    public static func request(host: String, port: UInt16, secWebSocketKey: String, sessionID: String?) -> String {
        var request = "GET \(requestTarget) HTTP/1.1\r\n"
        request += "Host: \(host):\(port)\r\n"
        request += "Connection: Upgrade\r\n"
        request += "Upgrade: websocket\r\n"
        request += "Sec-WebSocket-Version: 13\r\n"
        request += "Sec-WebSocket-Key: \(secWebSocketKey)\r\n"
        request += "Content-Length: 0\r\n"
        if let sessionID, !sessionID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            request += "x-nv-sessionid: \(sessionID.trimmingCharacters(in: .whitespacesAndNewlines))\r\n"
        }
        return request + "\r\n"
    }

    public static func expectedAccept(for secWebSocketKey: String) -> String {
        let digest = Insecure.SHA1.hash(data: Data((secWebSocketKey + acceptGUID).utf8))
        return Data(digest).base64EncodedString()
    }

    public static func generateKey() -> String {
        var bytes = [UInt8](repeating: 0, count: 16)
        for index in bytes.indices { bytes[index] = UInt8.random(in: 0...255) }
        return Data(bytes).base64EncodedString()
    }

    public struct Response: Equatable, Sendable {
        public let statusCode: Int
        public let headers: [String: String]
        public let leftover: Data
    }

    /// Parses the upgrade response prefix. Returns nil while the header block is incomplete.
    public static func parseResponse(_ buffer: Data) -> Response? {
        guard let separator = buffer.range(of: Data("\r\n\r\n".utf8)) else { return nil }
        let headerText = NvstRtspText.decode(buffer[buffer.startIndex..<separator.lowerBound])
        let leftover = Data(buffer[separator.upperBound...])
        let lines = headerText.components(separatedBy: "\r\n")
        let statusLine = lines.first ?? ""
        let parts = statusLine.split(separator: " ", omittingEmptySubsequences: true)
        let statusCode = parts.count >= 2 ? Int(parts[1]) ?? 0 : 0
        var headers: [String: String] = [:]
        for line in lines.dropFirst() {
            guard let colon = line.firstIndex(of: ":") else { continue }
            let name = line[line.startIndex..<colon].trimmingCharacters(in: .whitespaces).lowercased()
            let value = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
            if !name.isEmpty { headers[name] = value }
        }
        return Response(statusCode: statusCode, headers: headers, leftover: leftover)
    }
}

/// Minimal client-side WebSocket frame codec: masked text frames out, text/binary in.
public enum NvstWebSocketFrame {
    /// Encodes one masked client ping frame (FIN + opcode 0x9).
    ///
    /// The official client sends these on the RTSPS control connection roughly every two seconds
    /// for the life of the session — captured from its own `SSL_write`. Nothing else travels on
    /// that connection between PLAY and teardown, so without them the seat sees an idle control
    /// session.
    public static func encodePing(_ payload: Data = Data()) -> Data {
        encode(opcode: 0x9, payload: payload)
    }

    /// Encodes one masked client pong frame (FIN + opcode 0xA), echoing the ping's payload.
    /// Poco's WebSocket answers server pings automatically; a client that never pongs gets its
    /// connection closed by the seat shortly after the handshake.
    static func encodePong(payload: Data = Data()) -> Data {
        encode(opcode: 0xa, payload: payload)
    }

    /// Encodes one masked client text frame (FIN + opcode 0x1), as Poco does.
    public static func encodeText(_ payload: Data) -> Data {
        encode(opcode: 0x1, payload: payload)
    }

    static func encode(opcode: UInt8, payload: Data) -> Data {
        var mask = [UInt8](repeating: 0, count: 4)
        for index in mask.indices { mask[index] = UInt8.random(in: 0...255) }
        var writer = NvstByteWriter(capacity: 14 + payload.count)
        writer.u8(0x80 | opcode)
        let length = payload.count
        if length < 126 {
            writer.u8(UInt8(0x80 | length))
        } else if length < 65_536 {
            writer.u8(0x80 | 126)
            writer.u16BE(UInt16(length))
        } else {
            writer.u8(0x80 | 127)
            writer.u64BE(UInt64(length))
        }
        writer.bytes(mask)
        var masked = Data(capacity: length)
        for (offset, byte) in payload.enumerated() {
            masked.append(byte ^ mask[offset % 4])
        }
        writer.bytes(masked)
        return writer.data
    }
}

public enum NvstWebSocketFrameError: LocalizedError, Equatable, Sendable {
    case closed
    case frameTooLarge

    public var errorDescription: String? {
        switch self {
        case .closed: "The NVST RTSPS WebSocket was closed by the peer."
        case .frameTooLarge: "The NVST RTSPS WebSocket sent an oversized frame."
        }
    }
}

/// Incremental reader that yields complete data/text frame payloads. Close ends the stream;
/// pongs are counted (they answer the keepalive pings and time the control path's round trip);
/// other control frames are ignored, matching the official probe.
public struct NvstWebSocketFrameReader: Sendable {
    private var buffer = Data()
    /// Pong frames seen so far — the caller diffs this across `push` calls to time its pings.
    public private(set) var pongsSeen = 0
    /// Server pings awaiting a pong reply; the caller drains these after each `push`.
    private var pendingPings: [Data] = []

    public init() {}

    /// Ping payloads received since the last call. Each needs a pong echoing its payload.
    public mutating func takePings() -> [Data] {
        defer { pendingPings.removeAll() }
        return pendingPings
    }

    public mutating func push(_ chunk: Data) throws -> [Data] {
        buffer.append(chunk)
        var messages: [Data] = []
        while let frame = try nextFrame() {
            switch frame.opcode {
            case 0x1, 0x2: messages.append(frame.payload)
            case 0x8: throw NvstWebSocketFrameError.closed
            case 0x9: pendingPings.append(frame.payload)
            case 0xa: pongsSeen += 1
            default: break
            }
        }
        return messages
    }

    /// One complete frame consumed from `buffer`, or nil when only a partial frame has arrived.
    private mutating func nextFrame() throws -> (opcode: UInt8, payload: Data)? {
        var reader = NvstByteReader(buffer)
        guard reader.remaining >= 2,
              let firstByte = try? reader.u8(),
              let secondByte = try? reader.u8() else { return nil }
        let masked = (secondByte & 0x80) != 0
        guard let payloadLength = try Self.payloadLength(secondByte, reader: &reader) else { return nil }
        let maskLength = masked ? 4 : 0
        guard reader.remaining >= maskLength + payloadLength else { return nil }
        var mask = Data()
        if masked {
            guard let frameMask = try? reader.bytes(4) else { return nil }
            mask = frameMask
        }
        guard var payload = try? reader.bytes(payloadLength) else { return nil }
        if masked {
            let maskBytes = [UInt8](mask)
            for index in payload.indices {
                payload[index] ^= maskBytes[(index - payload.startIndex) % 4]
            }
        }
        buffer = reader.unread
        return (firstByte & 0x0f, payload)
    }

    /// The frame's payload length, following the 126/127 extended-length escapes. Nil means the
    /// extended field has not arrived yet.
    private static func payloadLength(_ secondByte: UInt8, reader: inout NvstByteReader) throws -> Int? {
        let base = Int(secondByte & 0x7f)
        if base == 126 {
            guard let extended = try? reader.u16BE() else { return nil }
            return Int(extended)
        }
        if base == 127 {
            guard let extended = try? reader.u64BE() else { return nil }
            guard extended <= UInt64(Int(Int32.max)) else { throw NvstWebSocketFrameError.frameTooLarge }
            return Int(extended)
        }
        return base
    }
}

public struct NvstRtspResponse: Equatable, Sendable {
    public let statusCode: Int
    public let statusText: String
    public let headers: [String: String]
    public let body: String

    public init(statusCode: Int, statusText: String, headers: [String: String], body: String) {
        self.statusCode = statusCode
        self.statusText = statusText
        self.headers = headers
        self.body = body
    }

    public func header(_ name: String) -> String? { headers[name.lowercased()] }
}

public enum NvstRtspMessageError: LocalizedError, Equatable, Sendable {
    case invalidStatusLine(String)
    case invalidContentLength(Int)

    public var errorDescription: String? {
        switch self {
        case .invalidStatusLine(let line): "Invalid RTSP status line: \(line)"
        case .invalidContentLength(let length): "Invalid RTSP Content-Length: \(length)"
        }
    }
}

public enum NvstRtspMessage {
    /// Ceiling for a declared `Content-Length`. The largest real body is an SDP of a few KB, so this
    /// is far above anything legitimate while still bounding what a hostile seat can make us buffer.
    static let maximumBodyByteLength = 4 * 1_024 * 1_024

    /// `Content-Length` is emitted only when a body exists; empty header values are preserved
    /// because the official SETUP sends a literally empty `Transport:` on the cloud path.
    public static func buildRequest(method: String,
                                    uri: String,
                                    headers: [(String, String)] = [],
                                    body: String = "",
                                    cseq: Int) -> String {
        var ordered: [(String, String)] = [("CSeq", String(cseq)), ("Request-Id", String(cseq))]
        ordered.append(contentsOf: headers)
        if !body.isEmpty {
            ordered.append(("Content-Length", String(body.utf8.count)))
        }
        var message = "\(method) \(uri) RTSP/1.0\r\n"
        for (name, value) in ordered {
            message += "\(name): \(value)\r\n"
        }
        message += "\r\n"
        if !body.isEmpty { message += body }
        return message
    }

    public static func parseResponse(_ raw: String) throws -> NvstRtspResponse {
        let normalized = raw.replacingOccurrences(of: "\r\n", with: "\n")
        let split = normalized.range(of: "\n\n")
        let headerBlock = split.map { String(normalized[normalized.startIndex..<$0.lowerBound]) } ?? normalized
        let body = split.map { String(normalized[$0.upperBound...]) } ?? ""
        let headerLines = headerBlock.components(separatedBy: "\n").filter { !$0.isEmpty }
        let statusLine = headerLines.first ?? ""
        guard let (code, text) = parseStatusLine(statusLine) else {
            throw NvstRtspMessageError.invalidStatusLine(String(statusLine.prefix(120)))
        }
        var headers: [String: String] = [:]
        for line in headerLines.dropFirst() {
            guard let colon = line.firstIndex(of: ":"), colon != line.startIndex else { continue }
            let name = line[line.startIndex..<colon].trimmingCharacters(in: .whitespaces).lowercased()
            headers[name] = line[line.index(after: colon)...].trimmingCharacters(in: .whitespaces)
        }
        return NvstRtspResponse(statusCode: code, statusText: text, headers: headers, body: body)
    }

    private static func parseStatusLine(_ line: String) -> (Int, String)? {
        let parts = line.split(separator: " ", maxSplits: 2, omittingEmptySubsequences: false)
        guard parts.count >= 2 else { return nil }
        let version = parts[0].uppercased()
        guard version.hasPrefix("RTSP/") || version.hasPrefix("HTTP/") else { return nil }
        guard let code = Int(parts[1]), code >= 100, code <= 599 else { return nil }
        let text = parts.count > 2 ? parts[2].trimmingCharacters(in: .whitespaces) : ""
        return (code, text)
    }

    /// Splits a framed RTSP response out of a byte buffer, honouring `Content-Length`.
    /// Returns nil while the message is still incomplete.
    public static func extractResponse(from buffer: inout Data) throws -> NvstRtspResponse? {
        let text = NvstRtspText.decode(buffer)
        let crlf = text.range(of: "\r\n\r\n")
        let lf = text.range(of: "\n\n")
        let separator: Range<String.Index>?
        if let crlf, lf == nil || crlf.lowerBound <= lf!.lowerBound {
            separator = crlf
        } else {
            separator = lf
        }
        guard let separator else { return nil }
        let headerText = String(text[text.startIndex..<separator.lowerBound])
        let declaredLength = headerText
            .components(separatedBy: .newlines)
            .compactMap { line -> Int? in
                let lower = line.lowercased()
                guard lower.hasPrefix("content-length:") else { return nil }
                return Int(line.dropFirst("content-length:".count).trimmingCharacters(in: .whitespaces))
            }
            .first ?? 0
        // A seat that answers with a negative or absurd Content-Length must not be able to drive
        // `total` out of range: Data.prefix/removeFirst trap on negative counts, which turns one
        // hostile response into a client crash.
        guard declaredLength >= 0, declaredLength <= maximumBodyByteLength else {
            throw NvstRtspMessageError.invalidContentLength(declaredLength)
        }
        let contentLength = declaredLength
        let headerByteLength = String(text[text.startIndex..<separator.upperBound]).utf8.count
        guard buffer.count - headerByteLength >= contentLength else { return nil }
        let total = headerByteLength + contentLength
        let raw = NvstRtspText.decode(buffer.prefix(total))
        buffer.removeFirst(total)
        return try parseResponse(raw)
    }

    /// `X-GS-ServerPort=<port>` + `source=<ip>` out of the SETUP `Transport` header.
    public static func extractVideoPeer(_ transport: String?) -> (ip: String, port: UInt16)? {
        guard let transport else { return nil }
        guard let port = firstCapture(in: transport, pattern: "X-GS-ServerPort=([0-9]+)").flatMap(UInt16.init),
              let source = firstCapture(in: transport, pattern: "source=([^;,\\s]+)") else { return nil }
        return (source, port)
    }

    /// Every seat port named by `X-GS-ServerPort`. The header is a range (`5004-5005` on every
    /// captured seat). Video has only ever been seen arriving from the first port, but the header
    /// promises the range, and a seat answering from the second port would be dropped by a NAT that
    /// only had the first one open — an outage indistinguishable from "the seat sent nothing".
    /// Punching the whole range costs one datagram per port and closes that door.
    public static func extractVideoPeerPorts(_ transport: String?) -> [UInt16] {
        guard let transport,
              let captured = firstCapture(in: transport, pattern: "X-GS-ServerPort=([0-9]+(?:-[0-9]+)?)") else { return [] }
        let bounds = captured.split(separator: "-").compactMap { UInt16($0) }
        guard let first = bounds.first else { return [] }
        let last = bounds.count > 1 ? bounds[1] : first
        guard last >= first, Int(last) - Int(first) < 16 else { return [first] }
        return Array(first...last)
    }

    static func firstCapture(in text: String, pattern: String) -> String? {
        guard let regex = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive]) else { return nil }
        let range = NSRange(text.startIndex..<text.endIndex, in: text)
        guard let match = regex.firstMatch(in: text, options: [], range: range), match.numberOfRanges > 1,
              let captured = Range(match.range(at: 1), in: text) else { return nil }
        return String(text[captured])
    }
}

enum NvstRtspText {
    /// Lossy UTF-8 decode: RTSP control traffic is ASCII, and a malformed byte must not
    /// abort the exchange.
    static func decode(_ data: Data) -> String {
        String(data: data, encoding: .utf8) ?? String(decoding: data, as: UTF8.self)
    }
}
