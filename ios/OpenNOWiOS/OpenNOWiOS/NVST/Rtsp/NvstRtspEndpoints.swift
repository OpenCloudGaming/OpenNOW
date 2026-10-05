import Foundation

/// Where the NVST RTSPS control channel lives, and the small identity transforms the official
/// client applies between DESCRIBE/SETUP and the STUN keepalive.
public enum NvstRtspEndpoints {
    /// The NVST RTSPS control port. Seats that do not tag the endpoint still listen here, so it
    /// doubles as the last-resort candidate.
    public static let defaultControlPort: UInt16 = 322

    /// `connectionInfo` entries with `usage == 16` (or `appLevelProtocol == 6`) carry the RTSPS
    /// control endpoint. Some seats put a full `rtsps://host:port` in `resourcePath`; others only
    /// give a port, in which case the session's server host is used.
    public static func collect(connections: [[String: Any]], fallbackHost: String?, allowsAssumedControlPort: Bool = true) -> [String] {
        var endpoints: [String] = []
        var untagged: [String] = []
        var taggedOtherPort: [String] = []
        var seen = Set<String>()
        for connection in connections {
            let resourcePath = (connection["resourcePath"] as? String)?.trimmingCharacters(in: .whitespaces) ?? ""
            if resourcePath.lowercased().hasPrefix("rtsps://") || resourcePath.lowercased().hasPrefix("rtsp://") {
                if seen.insert(resourcePath).inserted { endpoints.append(resourcePath) }
                continue
            }
            let usage = integer(connection["usage"])
            let appLevelProtocol = integer(connection["appLevelProtocol"])
            let port = integer(connection["port"])
            // Tagged control endpoints first; an untagged entry already on :322 is the same
            // service under a different usage code, which is what our own captures showed.
            let isTagged = usage == 16 || appLevelProtocol == 6
            let isControlPort = port == Int(defaultControlPort)
            guard isTagged || isControlPort else { continue }
            let host = usableHost(connection["ip"]) ?? fallbackHost
            guard let host, !host.isEmpty, port > 0 else { continue }
            let synthesized = "rtsps://\(host):\(port)"
            guard seen.insert(synthesized).inserted else { continue }
            // A tagged entry already on :322 is the control channel; a tagged entry on another
            // port (the seat tags its media endpoints the same way) is only a fallback.
            switch (isTagged, isControlPort) {
            case (true, true): endpoints.append(synthesized)
            case (true, false): taggedOtherPort.append(synthesized)
            default: untagged.append(synthesized)
            }
        }
        endpoints.append(contentsOf: untagged)
        endpoints.append(contentsOf: taggedOtherPort)
        // Last resort: the seat may not advertise the control endpoint at all. Trying the default
        // port beats refusing to negotiate, and a wrong guess fails fast on connect.
        //
        // Resumes opt out. A session created by a WebRTC client advertises one `/nvst/` endpoint on
        // 443 and no `:322` at all until the seat accepts the hand-over and re-provisions for NVST
        // — the official client's own RESUME response only lists `rtsps://...:322` afterwards, on a
        // different seat host than the pre-claim listing named. Guessing :322 there reaches a host
        // that does not serve RTSP, which answers the WebSocket upgrade with HTTP 501.
        if allowsAssumedControlPort, let fallbackHost, !fallbackHost.isEmpty {
            let assumed = "rtsps://\(fallbackHost):\(defaultControlPort)"
            if seen.insert(assumed).inserted { endpoints.append(assumed) }
        }
        return endpoints
    }

    private static func usableHost(_ value: Any?) -> String? {
        guard let host = (value as? String)?.trimmingCharacters(in: .whitespaces), !host.isEmpty else { return nil }
        // 0.0.0.0 / empty placeholders appear on entries whose host is the session host.
        return host == "0.0.0.0" ? nil : host
    }

    /// Collects the endpoints out of a raw `startSession`/`resumeSession` JSON body.
    public static func collect(rawSessionJSON: String, fallbackHost: String?, allowsAssumedControlPort: Bool = true) -> [String] {
        guard let data = rawSessionJSON.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return [] }
        let connections = (object["connectionInfo"] as? [Any])?.compactMap { $0 as? [String: Any] } ?? []
        return collect(connections: connections, fallbackHost: fallbackHost, allowsAssumedControlPort: allowsAssumedControlPort)
    }

    /// Every usable candidate, in preference order. The negotiator walks them until one answers,
    /// because a seat can advertise a control endpoint it does not actually serve.
    public static func candidates(_ endpoints: [String]) -> [String] {
        var seen = Set<String>()
        return endpoints
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .filter { $0.lowercased().hasPrefix("rtsps://") || $0.lowercased().hasPrefix("rtsp://") }
            .filter { seen.insert($0).inserted }
    }

    public static func selectPrimary(_ endpoints: [String]) -> String? {
        endpoints
            .map { $0.trimmingCharacters(in: .whitespaces) }
            .first { $0.lowercased().hasPrefix("rtsps://") || $0.lowercased().hasPrefix("rtsp://") }
    }

    public struct Target: Equatable, Sendable {
        public let host: String
        public let port: UInt16
        /// `rtsps://host:port` — the RTSP request URI the seat expects (absolute form).
        public let requestURI: String

        public init(host: String, port: UInt16) {
            self.host = host
            self.port = port
            self.requestURI = "rtsps://\(host):\(port)"
        }
    }

    /// The NVST control port defaults to 322 when the endpoint omits it.
    public static func parse(endpoint: String) -> Target? {
        let normalized = endpoint
            .replacingOccurrences(of: "rtsps://", with: "https://", options: [.caseInsensitive, .anchored])
            .replacingOccurrences(of: "rtsp://", with: "http://", options: [.caseInsensitive, .anchored])
        guard let url = URL(string: normalized), let host = url.host, !host.isEmpty else { return nil }
        let port = UInt16(url.port ?? 322)
        return Target(host: host, port: port)
    }

    /// The official SETUP addresses `streamid=video/0/0` when DESCRIBE advertised `streamid=video/0`.
    public static func officialVideoSetupControl(_ control: String) -> String {
        if NvstRtspMessage.firstCapture(in: control, pattern: "^(streamid=video/[0-9]+)$") != nil {
            return control + "/0"
        }
        return control
    }

    /// Request-URI forms to try for video SETUP, in preference order.
    ///
    /// The official client addresses SETUP with the **bare** control token DESCRIBE advertised
    /// (`streamid=video/0/0`), not a resolved absolute URI — a live seat answered `400` to the
    /// absolute form after accepting OPTIONS and DESCRIBE on the same connection. The remaining
    /// forms are fallbacks so one run distinguishes "wrong URI form" from "wrong headers".
    public static func videoSetupURICandidates(control: String, base: String, officialCloudPath: Bool) -> [String] {
        let substream = officialCloudPath ? officialVideoSetupControl(control) : control
        var candidates = [substream]
        if substream != control { candidates.append(control) }
        for form in candidates.map({ resolveControlURI(base: base, control: $0) }) where !candidates.contains(form) {
            candidates.append(form)
        }
        var seen = Set<String>()
        return candidates.filter { seen.insert($0).inserted }
    }

    /// Resolves a relative `a=control:` value against the RTSP base URI.
    public static func resolveControlURI(base: String, control: String) -> String {
        let lower = control.lowercased()
        if lower.hasPrefix("rtsps://") || lower.hasPrefix("rtsp://") { return control }
        guard let target = parse(endpoint: base) else { return control }
        let scheme = base.lowercased().hasPrefix("rtsp://") ? "rtsp" : "rtsps"
        if control.hasPrefix("/") {
            return "\(scheme)://\(target.host):\(target.port)\(control)"
        }
        var trimmedBase = base
        while trimmedBase.hasSuffix("/") { trimmedBase.removeLast() }
        var trimmedControl = control
        while trimmedControl.hasPrefix("/") { trimmedControl.removeFirst() }
        return "\(trimmedBase)/\(trimmedControl)"
    }

    /// The official bundle's ICE remote ufrag is the SETUP ping payload + 1 (`…998` → `…999`).
    public static func incrementPingUfrag(_ payload: String) -> String? {
        guard !payload.isEmpty,
              payload.allSatisfy({ $0.isHexDigit }),
              payload.uppercased() != "PING" else { return nil }
        // The payload can exceed 64 bits, so increment as a hex string. The official client
        // renders the result lowercase and left-pads it back to the original width.
        var digits = Array(payload.lowercased())
        var index = digits.count - 1
        while index >= 0 {
            guard let value = digits[index].hexDigitValue else { return nil }
            if value == 15 {
                digits[index] = "0"
                index -= 1
                continue
            }
            digits[index] = Character(String(value + 1, radix: 16))
            return String(digits)
        }
        // Full carry: every digit was "f", so the width grows by one.
        return "1" + String(digits)
    }

    /// Official `NattHolePunch` STUN remote ufrag: the SETUP ping payload + 1 on the ICE
    /// bundle; otherwise the ping string itself. Never falls back to the DESCRIBE V2 ufrag
    /// while a SETUP ping payload is present — that keepalive identity is what the seat answers.
    public static func resolveIceRemoteUfrag(pingPayload: String?,
                                             describeUfrag: String?,
                                             pingVersion: Int?) -> String? {
        if let pingPayload, !pingPayload.isEmpty {
            if let incremented = incrementPingUfrag(pingPayload) { return incremented }
            if pingPayload.uppercased() == "PING" || pingVersion == 6 { return pingPayload }
        }
        return describeUfrag
    }

    private static func integer(_ value: Any?) -> Int {
        if let number = value as? NSNumber { return number.intValue }
        if let text = value as? String { return Int(text) ?? 0 }
        return 0
    }
}
