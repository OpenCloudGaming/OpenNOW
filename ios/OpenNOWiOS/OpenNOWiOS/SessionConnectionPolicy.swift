import Foundation
import CoreFoundation

/// Owns the distinction between WebRTC signaling and RTSP control in CloudMatch replies.
enum SessionConnectionPolicy {
    struct Route: Equatable {
        let allocationHost: String
        let server: String
        let url: String
        let requiresNativeTransport: Bool
    }

    static func integer(_ value: Any?) -> Int? {
        if let string = value as? String { return Int(string.trimmingCharacters(in: .whitespacesAndNewlines)) }
        guard let number = value as? NSNumber,
              CFGetTypeID(number) != CFBooleanGetTypeID() else { return nil }
        return Int(number.stringValue) ?? Int(exactly: number.doubleValue)
    }

    static func isNative(_ connection: [String: Any]) -> Bool {
        let resource = (connection["resourcePath"] as? String ?? "").lowercased()
        return resource.hasPrefix("rtsp://") || resource.hasPrefix("rtsps://")
            || integer(connection["usage"]) == 16
            || [1, 6].contains(integer(connection["appLevelProtocol"]) ?? -1)
    }

    static func resolve(session: [String: Any], fallbackHost: String?) -> Route? {
        let connections = (session["connectionInfo"] as? [Any])?.compactMap { $0 as? [String: Any] } ?? []
        let candidates = connections.filter { !isNative($0) }
            + connections.filter { integer($0["usage"]) == 14 }
            + connections.filter { integer($0["usage"]) == 16 }
        for connection in candidates {
            guard [14, 16].contains(integer(connection["usage"]) ?? -1),
                  let route = route(connection, fallbackHost: fallbackHost) else { continue }
            return route
        }
        // An empty/partial poll must not synthesize a WebRTC route over a native allocation.
        return nil
    }

    static func mediaEndpoint(session: [String: Any]) -> (ip: String?, port: Int) {
        let connections = (session["connectionInfo"] as? [Any])?.compactMap { $0 as? [String: Any] } ?? []
        let candidates = [2, 17].flatMap { usage in
            connections.filter { integer($0["usage"]) == usage }
        } + connections.filter { integer($0["usage"]) == 14 && !isNative($0) }
            .sorted { (integer($0["port"]) ?? 0) > (integer($1["port"]) ?? 0) }
        for connection in candidates {
            // VIDEO/BUNDLE usages own media even when a provider reuses protocol tags.
            // An explicit RTSP resource is still a control route, never an ICE peer.
            let path = (connection["resourcePath"] as? String ?? "").lowercased()
            guard !path.hasPrefix("rtsp://"), !path.hasPrefix("rtsps://") else { continue }
            let resource = (connection["resourcePath"] as? String).flatMap { URLComponents(string: $0) }
            guard let host = normalizedEndpointHost(from: connection["ip"]) ?? normalizedEndpointHost(from: resource?.host),
                  !["0.0.0.0", "::", "[::]"].contains(host),
                  let port = integer(connection["port"]) ?? resource?.port,
                  (1...65535).contains(port) else { continue }
            return (host, port)
        }
        return (nil, 0)
    }

    private static func route(_ connection: [String: Any], fallbackHost: String?) -> Route? {
        let native = isNative(connection)
        let resource = (connection["resourcePath"] as? String)?
            .trimmingCharacters(in: .whitespacesAndNewlines).nonEmpty ?? "/nvst/"
        var components: URLComponents
        if resource.contains("://") {
            guard let parsed = URLComponents(string: resource),
                  ["wss", "https", "rtsp", "rtsps"].contains(parsed.scheme?.lowercased() ?? "") else { return nil }
            components = parsed
            if ["rtsp", "rtsps"].contains(parsed.scheme?.lowercased() ?? "") {
                components.port = 443
                components.path = "/nvst/"
                components.query = nil
            }
        } else {
            guard resource.hasPrefix("/"), !resource.hasPrefix("//"),
                  let host = normalizedEndpointHost(from: connection["ip"]) ?? normalizedEndpointHost(from: fallbackHost),
                  !host.isEmpty else { return nil }
            let authority = host.contains(":") && !host.hasPrefix("[") ? "[\(host)]" : host
            guard let parsed = URLComponents(string: "wss://\(authority)\(resource)") else { return nil }
            components = parsed
            // connectionInfo.port can be the UDP peer. Relative signaling uses 443.
            components.port = 443
        }
        guard components.user == nil, components.password == nil, components.fragment == nil,
              let host = normalizedEndpointHost(from: components.host), !host.isEmpty,
              !["0.0.0.0", "::", "[::]"].contains(host),
              (1...65535).contains(components.port ?? 443) else { return nil }
        components.scheme = "wss"
        guard let url = components.url else { return nil }
        let authority = host.contains(":") && !host.hasPrefix("[") ? "[\(host)]" : host
        let allocationHost = normalizedEndpointHost(from: connection["ip"])
            .flatMap { ["0.0.0.0", "::"].contains($0) ? nil : $0 } ?? host
        return Route(allocationHost: allocationHost, server: "\(authority):\(components.port ?? 443)", url: url.absoluteString,
                     requiresNativeTransport: native)
    }

    static func signInURL(base: String, peerName: String, sessionID: String) -> URL? {
        guard var components = URLComponents(string: base),
              ["wss", "https"].contains(components.scheme?.lowercased() ?? ""),
              components.host?.isEmpty == false,
              components.user == nil, components.password == nil, components.fragment == nil,
              (1...65535).contains(components.port ?? 443) else { return nil }
        components.scheme = "wss"
        let basePath = components.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
            .split(separator: "/").map(String.init).joined(separator: "/")
        components.path = "/" + basePath + (basePath.isEmpty ? "" : "/") + "sign_in"
        components.queryItems = [
            URLQueryItem(name: "peer_id", value: peerName), URLQueryItem(name: "version", value: "2"),
            URLQueryItem(name: "peer_role", value: "1"), URLQueryItem(name: "pairing_id", value: sessionID)
        ]
        return components.url
    }
}

extension ActiveSession {
    func transportPreferenceError(settings: AppSettings) -> String? {
        guard !settings.experimentalNativeNVSTEnabled, nativeTransportRequired == true else { return nil }
        return "This allocation requires Native NVST, which is disabled. End this session and start a new one to request WebRTC, or enable Native NVST in Settings → Stream → Connection."
    }

    func usesNativeTransport(settings: AppSettings) -> Bool {
        settings.experimentalNativeNVSTEnabled || nativeTransportRequired == true
    }
}

private extension String {
    var nonEmpty: String? { isEmpty ? nil : self }
}

func normalizedEndpointHost(from value: Any?) -> String? {
    if let stringValue = value as? String {
        return normalizedEndpointHost(from: stringValue)
    }
    if let stringValues = value as? [String] {
        return stringValues.compactMap { normalizedEndpointHost(from: $0) }.first
    }
    return nil
}

func normalizedEndpointHost(from stringValue: String?) -> String? {
    guard var raw = stringValue?.trimmingCharacters(in: .whitespacesAndNewlines), !raw.isEmpty else {
        return nil
    }
    let urlSchemes: Set<String> = ["http", "https", "ws", "wss", "rtsp", "rtsps"]
    if let components = URLComponents(string: raw),
       let scheme = components.scheme?.lowercased(),
       urlSchemes.contains(scheme) {
        return validEndpointHost(components.host)
    }

    if raw.hasPrefix("[") {
        guard let end = raw.firstIndex(of: "]") else { return nil }
        let hostStart = raw.index(after: raw.startIndex)
        return validEndpointHost(String(raw[hostStart..<end]))
    }

    if let separator = raw.firstIndex(where: { $0 == "/" || $0 == "?" || $0 == "#" }) {
        raw = String(raw[..<separator])
    }
    guard !raw.contains("@") else { return nil }

    let colonCount = raw.filter { $0 == ":" }.count
    if colonCount == 1, let separator = raw.lastIndex(of: ":") {
        let portStart = raw.index(after: separator)
        let port = raw[portStart...]
        if !port.isEmpty, port.allSatisfy({ $0.isNumber }) {
            raw = String(raw[..<separator])
        }
    }

    return validEndpointHost(raw)
}

private func validEndpointHost(_ host: String?) -> String? {
    guard let host else { return nil }
    let normalized = host.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !normalized.isEmpty,
          normalized.rangeOfCharacter(from: .whitespacesAndNewlines) == nil else {
        return nil
    }

    if normalized.contains(":") {
        let allowedIPv6 = CharacterSet(charactersIn: "0123456789abcdefABCDEF:.")
        guard normalized.unicodeScalars.allSatisfy({ allowedIPv6.contains($0) }) else {
            return nil
        }
        return normalized
    }

    let allowedHost = CharacterSet(charactersIn: "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789.-")
    guard normalized.unicodeScalars.allSatisfy({ allowedHost.contains($0) }),
          !normalized.hasPrefix("."),
          !normalized.hasSuffix("."),
          !normalized.hasPrefix("-"),
          !normalized.hasSuffix("-"),
          !normalized.contains("..") else {
        return nil
    }

    let labels = normalized.split(separator: ".", omittingEmptySubsequences: false)
    guard labels.allSatisfy({ !$0.isEmpty && !$0.hasPrefix("-") && !$0.hasSuffix("-") }) else {
        return nil
    }
    return normalized
}
