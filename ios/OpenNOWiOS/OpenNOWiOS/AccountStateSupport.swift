import AuthenticationServices
import Foundation

struct CachedAccountSnapshot: Codable, Equatable {
    let schemaVersion: Int
    let cachedAt: TimeInterval
    let membershipTier: String
    let subscription: SubscriptionSnapshot?
    let accountConnectors: [AccountConnector]
    let availableRegions: [StreamRegion]
    let vpcId: String
}

enum OpenNOWErrorPresenter {
    /// Whether this error is a cancellation rather than a failure.
    ///
    /// A cancelled `URLSession` task throws `URLError.cancelled`, **not** `CancellationError`, so
    /// `catch is CancellationError` alone lets it through — and it arrives with the
    /// `localizedDescription` "cancelled", which is how "Failed to load account details: cancelled"
    /// reached the screen. It is not a failure at all: SwiftUI tears down a `.task` when its view
    /// goes away, and pull-to-refresh supersedes an in-flight request, both of which are the app
    /// working correctly. Nothing about a cancellation should ever be shown to the player.
    static func isCancellation(_ error: Error) -> Bool {
        if error is CancellationError { return true }
        let nsError = error as NSError
        return nsError.domain == NSURLErrorDomain && nsError.code == NSURLErrorCancelled
    }

    static func message(for error: Error, fallback: String) -> String {
        let nsError = error as NSError
        if nsError.domain == NSURLErrorDomain, nsError.code == NSURLErrorTimedOut {
            return "The request timed out. Check your connection and try again."
        }
        if nsError.domain == ASWebAuthenticationSessionError.errorDomain,
           nsError.code == ASWebAuthenticationSessionError.canceledLogin.rawValue {
            return "Sign-in was cancelled before it finished."
        }

        let raw = nsError.localizedDescription.trimmingCharacters(in: .whitespacesAndNewlines)
        if let parsed = parsedServerMessage(from: raw) {
            return normalized(parsed)
        }
        if raw.localizedCaseInsensitiveContains("access_denied") {
            return "NVIDIA did not authorize this sign-in. Please try again and complete the account prompt."
        }
        if raw.localizedCaseInsensitiveContains("invalid_grant") {
            return "This saved sign-in has expired. Sign in to this account again."
        }
        if raw.localizedCaseInsensitiveContains("login_required") {
            return "NVIDIA needs you to sign in to this account again."
        }
        return raw.isEmpty || raw == "The operation couldn’t be completed."
            ? fallback
            : normalized(raw)
    }

    private static func parsedServerMessage(from raw: String) -> String? {
        let candidates = [raw, jsonObjectSubstring(in: raw)].compactMap { $0 }
        for candidate in candidates {
            guard let data = candidate.data(using: .utf8),
                  let object = try? JSONSerialization.jsonObject(with: data),
                  let message = preferredMessage(in: object) else {
                continue
            }
            return message
        }
        return nil
    }

    private static func preferredMessage(in value: Any) -> String? {
        if let dictionary = value as? [String: Any] {
            for key in ["error_description", "errorMessage", "statusDescription", "message", "detail", "description"] {
                if let message = dictionary[key] as? String, !message.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    return message
                }
            }
            for key in ["error", "errors", "requestStatus", "response"] {
                if let nested = dictionary[key], let message = preferredMessage(in: nested) {
                    return message
                }
            }
            for nested in dictionary.values {
                if let message = preferredMessage(in: nested) {
                    return message
                }
            }
        } else if let array = value as? [Any] {
            for nested in array {
                if let message = preferredMessage(in: nested) {
                    return message
                }
            }
        }
        return nil
    }

    private static func jsonObjectSubstring(in raw: String) -> String? {
        guard let start = raw.firstIndex(of: "{"), let end = raw.lastIndex(of: "}"), start <= end else {
            return nil
        }
        return String(raw[start...end])
    }

    private static func normalized(_ raw: String) -> String {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasPrefix("INTERNAL_ERROR_STATUS") {
            return "GeForce NOW could not create this stream (\(trimmed)). Video decoding has not started. Try another server region or stream profile."
        }
        guard trimmed.range(of: #"^[A-Z0-9_]+$"#, options: .regularExpression) != nil else {
            return trimmed
        }
        return trimmed
            .replacingOccurrences(of: "_STATUS", with: "")
            .replacingOccurrences(of: "_", with: " ")
            .lowercased()
            .capitalized
    }
}
