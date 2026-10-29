import CryptoKit
import Foundation

/// Bounded, process-local history retained after a coordinator stops or retries.
/// No SDP, payloads, credentials, or peer identities are recorded here.
@MainActor
final class StreamConnectionDiagnostics {
    static let shared = StreamConnectionDiagnostics()

    struct Attempt {
        let id: UUID
        let startedAt: Date
        let endpoint: String
        var stage = "connecting"
        var packets = 0
        var invalidPackets = 0
        var offers = 0
        var sends = 0
        var sendFailures = 0
        var lastFailure: String?
        var closeCode: Int?
        var httpStatus: Int?
        var timing: String?
    }

    private let capacity: Int
    private(set) var attempts: [Attempt] = []

    init(capacity: Int = 8) { self.capacity = min(32, max(1, capacity)) }

    @discardableResult
    func begin(endpoint: String, now: Date = .now) -> UUID {
        let id = UUID()
        // URL queries can carry pairing/auth values, so retain only the sanitized host/path.
        var components = URLComponents(string: endpoint)
        components?.query = nil
        components?.user = nil
        components?.password = nil
        components?.fragment = nil
        let safe = DiagnosticsSanitizer.sanitize(String((components?.string ?? "unknown").prefix(400)))
        attempts.append(Attempt(id: id, startedAt: now, endpoint: safe))
        if attempts.count > capacity { attempts.removeFirst(attempts.count - capacity) }
        return id
    }

    func update(_ id: UUID?, _ update: (inout Attempt) -> Void) {
        guard let id, let index = attempts.firstIndex(where: { $0.id == id }) else { return }
        update(&attempts[index])
    }

    func failure(_ id: UUID?, error: Error) {
        let error = error as NSError
        update(id) {
            $0.lastFailure = DiagnosticsSanitizer.sanitize(String(error.domain.prefix(120))) + " code=\(error.code) at=\(DiagnosticsSanitizer.sanitize($0.stage))"
            if !["local_stop", "local_retry"].contains($0.stage) { $0.stage = "failed" }
        }
    }

    func export() -> String {
        let formatter = ISO8601DateFormatter()
        return (["attempts=\(attempts.count) retention=inMemoryOnly sendCompletionIsNotServerAcknowledgment=true"]
            + attempts.enumerated().map { index, value in
                "attempt=\(index + 1) started=\(formatter.string(from: value.startedAt)) endpoint=\(value.endpoint) "
                    + "stage=\(value.stage) packets=\(value.packets) invalid=\(value.invalidPackets) offers=\(value.offers) "
                    + "sends=\(value.sends) sendFailures=\(value.sendFailures) close=\(value.closeCode.map(String.init) ?? "none") "
                    + "http=\(value.httpStatus.map(String.init) ?? "none") failure=\(value.lastFailure ?? "none") timing=\(value.timing ?? "none")"
            }).joined(separator: "\n")
    }
}

enum DiagnosticsSanitizer {
    private static let sensitiveKeys: Set<String> = [
        "authorization", "proxyauthorization", "cookie", "setcookie", "password", "passwd",
        "secret", "accesstoken", "refreshtoken", "idtoken", "clienttoken", "authtoken",
        "authkey", "code", "codeverifier", "state"
    ]
    private static let identifierKeys: Set<String> = [
        "devicehashid", "deviceid", "xdeviceid", "sessionid", "subsessionid", "userid",
        "accountid", "subject", "sub", "nvclientid", "xrequestid", "requestid"
    ]
    private static let personalKeys: Set<String> = [
        "email", "username", "preferredusername", "displayname", "firstname", "lastname"
    ]

    static func sanitize(_ raw: String) -> String {
        let protectedVersions = protectingSemanticVersions(in: raw)
        var value = protectedVersions.value
        value = replacing(
            in: value,
            pattern: #"(?i)\b(Bearer|GFNJWT)\s+[A-Za-z0-9._~+/=-]+"#,
            with: "$1 [REDACTED]"
        )
        value = replacing(
            in: value,
            pattern: #"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\b"#,
            with: "[REDACTED_JWT]"
        )
        value = replacing(
            in: value,
            pattern: #"(?i)([?&](?:access_token|refresh_token|id_token|client_token|token|auth_key|code|code_verifier|state|password|secret)=)[^&#\s]+"#,
            with: "$1[REDACTED]"
        )
        value = replacing(
            in: value,
            pattern: #"(?i)([?&](?:deviceHashId|deviceId|sessionId|subSessionId|userId|accountId|requestId)=)[^&#\s]+"#,
            with: "$1[REDACTED_ID]"
        )
        value = replacing(
            in: value,
            pattern: #"(?i)([\"']?\b(?:authorization|proxy[_-]?authorization|cookie|set[_-]?cookie|password|passwd|secret|access[_-]?token|refresh[_-]?token|id[_-]?token|client[_-]?token|auth[_-]?token|auth[_-]?key|code[_-]?verifier)\b[\"']?\s*[:=]\s*)(?:\"[^\"]*\"|'[^']*'|[^\s,;&}]+)"#,
            with: "$1[REDACTED]"
        )
        value = replacing(
            in: value,
            pattern: #"(?i)([\"']?\b(?:device[_-]?hash[_-]?id|device[_-]?id|x-device-id|session[_-]?id|subsession[_-]?id|user[_-]?id|account[_-]?id|request[_-]?id|x-request-id)\b[\"']?\s*[:=]\s*)(?:\"[^\"]*\"|'[^']*'|[^\s,;&}]+)"#,
            with: "$1[REDACTED_ID]"
        )
        value = replacing(
            in: value,
            pattern: #"(?i)([\"']?\b(?:email|user[_-]?name|preferred[_-]?username|display[_-]?name|first[_-]?name|last[_-]?name)\b[\"']?\s*[:=]\s*)(?:\"[^\"]*\"|'[^']*'|[^\s,;&}]+)"#,
            with: "$1[REDACTED_PII]"
        )
        value = replacing(
            in: value,
            pattern: #"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b"#,
            with: "[REDACTED_EMAIL]"
        )
        value = replacing(
            in: value,
            pattern: #"/Users/[^/\s]+"#,
            with: "/Users/[REDACTED_USER]"
        )
        value = replacingMatches(
            in: value,
            pattern: #"(?<![A-Fa-f0-9])[0-9A-Fa-f]{8}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}-[0-9A-Fa-f]{12}(?![A-Fa-f0-9])"#,
            label: "ID"
        )
        value = replacingMatches(
            in: value,
            pattern: #"(?<=://)(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}(?![0-9])"#,
            label: "IP"
        )
        value = replacingMatches(
            in: value,
            pattern: #"(?<![0-9/])(?:25[0-5]|2[0-4]\d|1?\d?\d)(?:\.(?:25[0-5]|2[0-4]\d|1?\d?\d)){3}(?![0-9])"#,
            label: "IP"
        )
        value = replacingMatches(
            in: value,
            pattern: #"(?i)(?<![0-9a-f:])(?:[0-9a-f]{1,4}:){7}[0-9a-f]{1,4}(?![0-9a-f:])"#,
            label: "IP"
        )
        value = replacingMatches(
            in: value,
            pattern: #"(?i)(?<![0-9a-f:])(?:[0-9a-f]{1,4}(?::[0-9a-f]{1,4}){0,6})?::(?:[0-9a-f]{1,4}(?::[0-9a-f]{1,4}){0,6})?(?![0-9a-f:])"#,
            label: "IP"
        )
        value = replacingMatches(
            in: value,
            pattern: #"(?<![A-Za-z0-9_-])[A-Za-z0-9_-]{40,}(?![A-Za-z0-9_-])"#,
            label: "OPAQUE"
        )
        value = replacingMatches(
            in: value,
            pattern: #"(?<![A-Za-z0-9+/=])[A-Za-z0-9+/]{32,}={0,2}(?![A-Za-z0-9+/=])"#,
            label: "OPAQUE"
        )
        return protectedVersions.restoring(in: value)
    }

    static func redactedHeaders(_ headers: [String: String]) -> String {
        headers
            .sorted { $0.key.localizedCaseInsensitiveCompare($1.key) == .orderedAscending }
            .map { key, value in
                let normalized = normalizedKey(key)
                if sensitiveKeys.contains(normalized) {
                    return "\(key): [REDACTED]"
                }
                if identifierKeys.contains(normalized) {
                    return "\(key): \(fingerprint(value, label: "ID"))"
                }
                return "\(key): \(sanitize(value))"
            }
            .joined(separator: "\n")
    }

    static func redactedBody(_ data: Data?, headers: [String: String]) -> String {
        guard let data, !data.isEmpty else { return "<empty>" }
        let contentType = headers.first { $0.key.caseInsensitiveCompare("Content-Type") == .orderedSame }?.value.lowercased() ?? ""

        if contentType.contains("json") || data.first == 123 || data.first == 91 {
            if let object = try? JSONSerialization.jsonObject(with: data),
               JSONSerialization.isValidJSONObject(object),
               let redacted = try? JSONSerialization.data(
                   withJSONObject: redactJSONObject(object),
                   options: [.prettyPrinted, .sortedKeys]
               ),
               let text = String(data: redacted, encoding: .utf8) {
                return sanitize(text)
            }
        }

        if contentType.contains("application/x-www-form-urlencoded"),
           let raw = String(data: data, encoding: .utf8) {
            var components = URLComponents()
            components.percentEncodedQuery = raw
            if let items = components.queryItems {
                return items.map { item in
                    let normalized = normalizedKey(item.name)
                    let value = item.value ?? ""
                    if sensitiveKeys.contains(normalized) {
                        return "\(item.name)=[REDACTED]"
                    }
                    if identifierKeys.contains(normalized) {
                        return "\(item.name)=\(fingerprint(value, label: "ID"))"
                    }
                    return "\(item.name)=\(sanitize(value))"
                }.joined(separator: "&")
            }
        }

        guard let text = String(data: data, encoding: .utf8) else {
            return "<binary \(data.count) bytes>"
        }
        return sanitize(text)
    }

    static func fingerprint(_ value: String, label: String) -> String {
        let digest = SHA256.hash(data: Data(value.utf8))
        let short = digest.prefix(6).map { String(format: "%02x", $0) }.joined()
        return "[\(label):\(short)]"
    }

    private static func redactJSONObject(_ value: Any, key: String? = nil) -> Any {
        if let key {
            let normalized = normalizedKey(key)
            if sensitiveKeys.contains(normalized) {
                return "[REDACTED]"
            }
            if identifierKeys.contains(normalized) {
                return fingerprint(String(describing: value), label: "ID")
            }
            if personalKeys.contains(normalized) {
                return "[REDACTED_PII]"
            }
        }

        if let dictionary = value as? [String: Any] {
            var redacted: [String: Any] = [:]
            for (childKey, child) in dictionary {
                redacted[childKey] = redactJSONObject(child, key: childKey)
            }
            return redacted
        }
        if let array = value as? [Any] {
            return array.map { redactJSONObject($0) }
        }
        if let string = value as? String {
            return sanitize(string)
        }
        return value
    }

    private static func normalizedKey(_ key: String) -> String {
        key.lowercased().filter(\.isLetter)
    }

    private static func replacing(in value: String, pattern: String, with replacement: String) -> String {
        value.replacingOccurrences(of: pattern, with: replacement, options: .regularExpression)
    }

    private static func replacingMatches(in value: String, pattern: String, label: String) -> String {
        guard let regex = try? NSRegularExpression(pattern: pattern) else { return value }
        let range = NSRange(value.startIndex..., in: value)
        let matches = regex.matches(in: value, range: range)
        var result = value
        for match in matches.reversed() {
            guard let swiftRange = Range(match.range, in: result) else { continue }
            result.replaceSubrange(swiftRange, with: fingerprint(String(result[swiftRange]), label: label))
        }
        return result
    }

    private struct ProtectedText {
        var value: String
        var replacements: [String: String]

        func restoring(in sanitized: String) -> String {
            replacements.reduce(sanitized) { result, replacement in
                result.replacingOccurrences(of: replacement.key, with: replacement.value)
            }
        }
    }

    private static func protectingSemanticVersions(in raw: String) -> ProtectedText {
        let patterns = [
            #"(?i)\b[a-z0-9_-]*(?:version|build)\s*[:=]\s*\d+(?:\.\d+){3}\b"#,
            #"(?i)\b(?:Chrome|Safari|GFN-PC|NVIDIACEFClient)/\d+(?:\.\d+){3}\b"#
        ]
        var protected = ProtectedText(value: raw, replacements: [:])
        var nextIndex = 0
        for pattern in patterns {
            guard let regex = try? NSRegularExpression(pattern: pattern) else { continue }
            let matches = regex.matches(
                in: protected.value,
                range: NSRange(protected.value.startIndex..., in: protected.value)
            )
            for match in matches.reversed() {
                guard let range = Range(match.range, in: protected.value) else { continue }
                let placeholder = "[SAFE_VERSION_\(nextIndex)]"
                nextIndex += 1
                protected.replacements[placeholder] = String(protected.value[range])
                protected.value.replaceSubrange(range, with: placeholder)
            }
        }
        return protected
    }
}

/// One recorded request, in the shape the in-app log needs to draw a row without re-parsing the
/// rendered text. The rendered form stays authoritative for export; this is a header for it.
struct DiagnosticsTraceEntry: Identifiable, Equatable {
    let id = UUID()
    let date: Date
    let source: String
    let method: String
    let url: String
    let statusCode: Int
    let durationMs: Int
    let failed: Bool
    let rendered: String

    /// Last two path components, which is usually the only part that differs between calls.
    var shortPath: String {
        guard let components = URL(string: url)?.pathComponents.filter({ $0 != "/" }), !components.isEmpty else {
            return url
        }
        return components.suffix(2).joined(separator: "/")
    }

    func matches(_ query: String) -> Bool {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !trimmed.isEmpty else { return true }
        return source.lowercased().contains(trimmed)
            || url.lowercased().contains(trimmed)
            || method.lowercased().contains(trimmed)
            || String(statusCode).contains(trimmed)
    }
}

actor DiagnosticsHTTPTraceStore {
    static let shared = DiagnosticsHTTPTraceStore()

    private var traceEntries: [DiagnosticsTraceEntry] = []
    private var entries: [String] = []
    private var storedBytes = 0
    private let maximumEntries = 180
    private let maximumStoredBytes = 700_000
    private let maximumBodyCharacters = 24_000

    func record(
        source: String,
        request: URLRequest,
        response: HTTPURLResponse?,
        responseData: Data?,
        duration: TimeInterval,
        error: Error?
    ) {
        let requestHeaders = request.allHTTPHeaderFields ?? [:]
        let responseHeaders = response?.allHeaderFields.reduce(into: [String: String]()) { partial, item in
            partial[String(describing: item.key)] = String(describing: item.value)
        } ?? [:]
        let requestBody = bounded(DiagnosticsSanitizer.redactedBody(request.httpBody, headers: requestHeaders))
        let responseBody = bounded(DiagnosticsSanitizer.redactedBody(responseData, headers: responseHeaders))
        let url = DiagnosticsSanitizer.sanitize(request.url?.absoluteString ?? "unknown")
        let rendered = DiagnosticsSanitizer.sanitize(
            """
            --- api request ---
            time=\(ISO8601DateFormatter().string(from: Date())) source=\(source) durationMs=\(Int(duration * 1_000))
            \(request.httpMethod ?? "GET") \(url)
            request.headers
            \(DiagnosticsSanitizer.redactedHeaders(requestHeaders))
            request.body bytes=\(request.httpBody?.count ?? 0)
            \(requestBody)
            response.status=\(response?.statusCode ?? -1) bytes=\(responseData?.count ?? 0)
            response.headers
            \(DiagnosticsSanitizer.redactedHeaders(responseHeaders))
            response.body
            \(responseBody)
            transport.error=\(error.map { DiagnosticsSanitizer.sanitize($0.localizedDescription) } ?? "none")
            """
        )

        entries.append(rendered)
        traceEntries.append(
            DiagnosticsTraceEntry(
                date: Date(),
                source: source,
                method: request.httpMethod ?? "GET",
                url: url,
                statusCode: response?.statusCode ?? -1,
                durationMs: Int(duration * 1_000),
                failed: error != nil || !(200...299).contains(response?.statusCode ?? 0),
                rendered: rendered
            )
        )
        storedBytes += rendered.utf8.count
        while entries.count > maximumEntries || storedBytes > maximumStoredBytes {
            storedBytes -= entries.removeFirst().utf8.count
            if !traceEntries.isEmpty { traceEntries.removeFirst() }
        }
    }

    /// Newest first, because that is the one you came to look at.
    func recentEntries() -> [DiagnosticsTraceEntry] {
        traceEntries.reversed()
    }

    func export() -> String {
        guard !entries.isEmpty else { return "entries=0" }
        return "entries=\(entries.count) maxEntries=\(maximumEntries) maxStoredBytes=\(maximumStoredBytes)\n" + entries.joined(separator: "\n")
    }

    private func bounded(_ value: String) -> String {
        guard value.count > maximumBodyCharacters else { return value }
        return String(value.prefix(maximumBodyCharacters)) + "\n[TRUNCATED bodyChars=\(value.count)]"
    }
}

enum DiagnosticsHTTPRecorder {
    static func data(
        for request: URLRequest,
        using session: URLSession,
        source: String
    ) async throws -> (Data, URLResponse) {
        let startedAt = Date()
        do {
            let (data, response) = try await session.data(for: request)
            await DiagnosticsHTTPTraceStore.shared.record(
                source: source,
                request: request,
                response: response as? HTTPURLResponse,
                responseData: data,
                duration: Date().timeIntervalSince(startedAt),
                error: nil
            )
            return (data, response)
        } catch {
            await DiagnosticsHTTPTraceStore.shared.record(
                source: source,
                request: request,
                response: nil,
                responseData: nil,
                duration: Date().timeIntervalSince(startedAt),
                error: error
            )
            throw error
        }
    }
}

enum DiagnosticsPasteClient {
    private static let endpoint = URL(string: "https://paste.rtech.support/upload/opennow-ios-diagnostics.txt")!

    static func upload(_ diagnostics: String) async throws -> URL {
        var request = URLRequest(url: endpoint)
        request.httpMethod = "PUT"
        request.timeoutInterval = 45
        request.setValue("text/plain; charset=UTF-8", forHTTPHeaderField: "Content-Type")
        request.setValue("OpenNOW-iOS-Diagnostics/1.0", forHTTPHeaderField: "User-Agent")
        request.setValue("604800", forHTTPHeaderField: "Linx-Expiry")
        request.setValue("yes", forHTTPHeaderField: "Linx-Randomize")
        request.httpBody = DiagnosticsSanitizer.sanitize(diagnostics).data(using: .utf8)

        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 45
        configuration.timeoutIntervalForResource = 60
        configuration.urlCache = nil
        configuration.httpCookieStorage = nil
        let (data, response) = try await URLSession(configuration: configuration).data(for: request)
        guard let http = response as? HTTPURLResponse, http.statusCode == 200 else {
            throw NSError(
                domain: "OpenNOW.DiagnosticsPaste",
                code: (response as? HTTPURLResponse)?.statusCode ?? -1,
                userInfo: [NSLocalizedDescriptionKey: "The log paste service rejected the upload."]
            )
        }
        let raw = String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
        guard let url = URL(string: raw), url.scheme == "https", url.host == "paste.rtech.support" else {
            throw NSError(
                domain: "OpenNOW.DiagnosticsPaste",
                code: -2,
                userInfo: [NSLocalizedDescriptionKey: "The log paste service returned an invalid link."]
            )
        }
        return url
    }
}
